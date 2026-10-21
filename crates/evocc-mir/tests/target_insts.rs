//! N03a step 2a freezes instruction facts, not a generic allocator.
//!
//! Minimal facade: MachInst has associated Reg plus uses, defs,
//! operand_slots(definitions), writes_flags, is_call and discardable.
//! MachTerm has associated Reg plus uses, operand_slots, edges and is_exit.
//! operand_slots is the existing mutable occurrence traversal, not a second
//! interpretation of semantic reads. In particular, term operand_slots means
//! the former linscan::term_slots (coalescing), not "rewrite every semantic use".
//! No instruction selection, interpreter, allocation or ABI change is admitted.

use std::collections::BTreeSet;

use evocc_evoir::FloatingOp;
use evocc_memory::{AtomicOrder, AtomicRmwOp, CopyVolatility};
use evocc_mir::generic::machine::{MachInst, MachTerm};
use evocc_mir::isa::aarch64::insts;
use evocc_mir::{
    inst_defs, inst_uses, term_edges, term_uses, writes_flags, AsmBody, AsmClobber, AsmConstraint,
    AsmDirection, AsmImmediate, AsmOperand, AsmValue, BinOp, CarryOp, Cond, ExtOp, FlagOp,
    FramePlace, LoadKind, MInst, MTerm, RegClass, StoreKind, SymbolKind, UnaryOp, VReg, WideMul,
    Width,
};
use evocc_proof::sha256_hex;
use evocc_types::{neon::AesOperation, FloatKind};

const DESCRIPTION: &str = include_str!("../../../targets/aarch64/insts");
// The existing public query is const. A trait-only replacement must not
// silently break callers that use it while constructing static metadata.
const TRAP_TERM: &MTerm = &MTerm::Brk { imm: 3001 };
const EMPTY_JUMP_TERM: &MTerm = &MTerm::B {
    target: 7,
    args: Vec::new(),
};
const TRAP_IS_EXIT: bool = evocc_mir::allocverify::is_exit(TRAP_TERM);
const EMPTY_JUMP_IS_EXIT: bool = evocc_mir::allocverify::is_exit(EMPTY_JUMP_TERM);
const INST_FAMILIES: &[&str] = &[
    "Floating",
    "Aes",
    "AtomicRmw",
    "Vector",
    "Atomic",
    "Mov",
    "MovImm",
    "Adrp",
    "AddPageOff",
    "Adr",
    "Load",
    "Store",
    "Binary",
    "BinaryImm",
    "FlagBinary",
    "WideMultiply",
    "Msub",
    "Madd",
    "CarryBinary",
    "AddCarryFlags",
    "SubCarryFlags",
    "TstImm",
    "Unary",
    "Extend",
    "Bfx",
    "Cmp",
    "CmpImm",
    "Cset",
    "Csel",
    "Bl",
    "Blr",
    "Ldp",
    "Stp",
    "SpillStore",
    "SpillReload",
    "IncomingArg",
    "OutgoingArg",
    "MemCopy",
    "FrameSlot",
    "StackAlloc",
    "StackRelease",
    "FrameAddress",
    "IndirectResult",
    "FpEnvCheck",
    "VaSaveArea",
    "Asm",
];
const TERM_FAMILIES: &[&str] = &[
    "Indirect", "B", "BCond", "Cb", "Tb", "Ret", "Brk", "AsmGoto",
];

fn g(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr64)
}
fn f(id: u32) -> VReg {
    VReg::new(id, RegClass::Flags)
}
fn d(id: u32) -> VReg {
    VReg::new(id, RegClass::Simd64)
}
fn q(id: u32) -> VReg {
    VReg::new(id, RegClass::Simd128)
}

#[test]
fn description_has_canonical_digest_and_all_current_families() {
    // The header and one row per enum family are the v1 envelope. Operand
    // selector payloads are target-specific, not a generic opcode language.
    assert!(DESCRIPTION.starts_with("version insts.v1\ntarget aarch64\n"));
    assert!(DESCRIPTION.ends_with('\n'));
    assert!(DESCRIPTION.is_ascii());
    assert!(!DESCRIPTION.contains('\r'));
    let digests: Vec<_> = DESCRIPTION
        .lines()
        .filter_map(|line| line.strip_prefix("sha256 "))
        .collect();
    assert_eq!(digests.len(), 1);
    let payload: String = DESCRIPTION
        .split_inclusive('\n')
        .filter(|line| !line.starts_with("sha256 "))
        .collect();
    assert_eq!(digests[0], sha256_hex(payload.as_bytes()));
    assert_eq!(digests[0], insts::DESCRIPTION_DIGEST);
    assert_eq!(insts::INST_FAMILIES, INST_FAMILIES);
    assert_eq!(insts::TERM_FAMILIES, TERM_FAMILIES);
    for (prefix, expected) in [("inst ", INST_FAMILIES), ("term ", TERM_FAMILIES)] {
        let rows: Vec<_> = DESCRIPTION
            .lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .map(|row| {
                row.split_once(' ')
                    .expect("family has a descriptor payload")
                    .0
            })
            .collect();
        assert_eq!(rows, expected);
        assert_eq!(
            rows.iter().copied().collect::<BTreeSet<_>>().len(),
            rows.len()
        );
    }
}

struct InstCase {
    family: &'static str,
    inst: MInst,
    defs: Vec<VReg>,
    uses: Vec<VReg>,
    rewrite_defs: Vec<VReg>,
    rewrite_uses: Vec<VReg>,
    flags: bool,
    discardable: bool,
    call: bool,
}

fn case(
    family: &'static str,
    inst: MInst,
    defs: &[VReg],
    uses: &[VReg],
    properties: (bool, bool, bool),
) -> InstCase {
    InstCase {
        family,
        inst,
        defs: defs.to_vec(),
        uses: uses.to_vec(),
        rewrite_defs: defs.to_vec(),
        rewrite_uses: uses.to_vec(),
        flags: properties.0,
        discardable: properties.1,
        call: properties.2,
    }
}

fn assembly(flags: bool) -> AsmBody {
    let mut read_write = AsmOperand::reg(AsmConstraint::Register(AsmDirection::ReadWrite), g(10));
    read_write.fixed_register = Some(19);
    let mut early = AsmOperand::reg(AsmConstraint::EarlyClobber(AsmDirection::Write), g(12));
    early.fixed_register = Some(20);
    AsmBody {
        template: "fixture %0 %1 %2".into(),
        volatile: true,
        outputs: vec![
            read_write,
            AsmOperand::reg(
                AsmConstraint::Memory {
                    direction: AsmDirection::Write,
                    offsettable: false,
                },
                g(11),
            ),
            early,
        ],
        inputs: vec![
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), g(13)),
            AsmOperand::reg(AsmConstraint::Tied(2), g(12)),
            AsmOperand::imm(AsmImmediate::Integer, 7),
            AsmOperand {
                constraint: AsmConstraint::RegisterOrZero,
                fixed_register: None,
                name: None,
                value: AsmValue::Zero(RegClass::Gpr64),
            },
        ],
        clobbers: if flags {
            vec![
                AsmClobber::Register(0),
                AsmClobber::Flags,
                AsmClobber::Memory,
            ]
        } else {
            vec![AsmClobber::Register(0), AsmClobber::Memory]
        },
    }
}

fn instruction_cases() -> Vec<InstCase> {
    let mut rows = vec![
        case(
            "Mov",
            MInst::Mov {
                dst: g(1),
                src: g(2),
            },
            &[g(1)],
            &[g(2)],
            (false, true, false),
        ),
        case(
            "MovImm",
            MInst::MovImm { dst: g(1), imm: 7 },
            &[g(1)],
            &[],
            (false, true, false),
        ),
        case(
            "Adrp",
            MInst::Adrp {
                dst: g(1),
                symbol: "item".into(),
                kind: SymbolKind::Global,
            },
            &[g(1)],
            &[],
            (false, true, false),
        ),
        case(
            "AddPageOff",
            MInst::AddPageOff {
                dst: g(1),
                src: g(2),
                symbol: "item".into(),
                kind: SymbolKind::Global,
            },
            &[g(1)],
            &[g(2)],
            (false, true, false),
        ),
        case(
            "Adr",
            MInst::Adr {
                dst: g(1),
                block: 7,
            },
            &[g(1)],
            &[],
            (false, true, false),
        ),
        case(
            "Load",
            MInst::Load {
                align: 8,
                dst: g(1),
                base: g(2),
                offset: 8,
                kind: LoadKind::X,
                volatile: false,
                mask: None,
            },
            &[g(1)],
            &[g(2)],
            (false, false, false),
        ),
        case(
            "Store",
            MInst::Store {
                align: 8,
                src: g(2),
                base: g(3),
                offset: 8,
                kind: StoreKind::X,
                volatile: false,
                mask: None,
            },
            &[],
            &[g(2), g(3)],
            (false, false, false),
        ),
        case(
            "Binary",
            MInst::Binary {
                op: BinOp::Add,
                dst: g(1),
                lhs: g(2),
                rhs: g(3),
                width: Width::X,
            },
            &[g(1)],
            &[g(2), g(3)],
            (false, true, false),
        ),
        case(
            "BinaryImm",
            MInst::BinaryImm {
                op: BinOp::Add,
                dst: g(1),
                lhs: g(2),
                imm: 7,
                width: Width::X,
            },
            &[g(1)],
            &[g(2)],
            (false, true, false),
        ),
        case(
            "FlagBinary",
            MInst::FlagBinary {
                op: FlagOp::Adds,
                dst: g(1),
                flags: f(4),
                lhs: g(2),
                rhs: g(3),
                width: Width::X,
            },
            &[g(1), f(4)],
            &[g(2), g(3)],
            (true, true, false),
        ),
        case(
            "WideMultiply",
            MInst::WideMultiply {
                op: WideMul::Smulh,
                dst: g(1),
                lhs: g(2),
                rhs: g(3),
            },
            &[g(1)],
            &[g(2), g(3)],
            (false, true, false),
        ),
        case(
            "Msub",
            MInst::Msub {
                dst: g(1),
                n: g(2),
                m: g(3),
                a: g(4),
                width: Width::X,
            },
            &[g(1)],
            &[g(2), g(3), g(4)],
            (false, true, false),
        ),
        case(
            "Madd",
            MInst::Madd {
                dst: g(1),
                n: g(2),
                m: g(3),
                a: g(4),
                width: Width::X,
            },
            &[g(1)],
            &[g(2), g(3), g(4)],
            (false, true, false),
        ),
        case(
            "CarryBinary",
            MInst::CarryBinary {
                op: CarryOp::Adc,
                dst: g(1),
                flags: f(4),
                lhs: g(2),
                rhs: g(3),
                width: Width::X,
            },
            &[g(1)],
            &[f(4), g(2), g(3)],
            (false, true, false),
        ),
        case(
            "AddCarryFlags",
            MInst::AddCarryFlags {
                dst: f(1),
                flags_in: f(4),
                lhs: g(2),
                rhs: g(3),
                width: Width::X,
            },
            &[f(1)],
            &[f(4), g(2), g(3)],
            (true, true, false),
        ),
        case(
            "SubCarryFlags",
            MInst::SubCarryFlags {
                dst: f(1),
                flags_in: f(4),
                lhs: g(2),
                rhs: g(3),
                width: Width::X,
            },
            &[f(1)],
            &[f(4), g(2), g(3)],
            (true, true, false),
        ),
        case(
            "TstImm",
            MInst::TstImm {
                dst: f(1),
                lhs: g(2),
                imm: 64,
                width: Width::X,
            },
            &[f(1)],
            &[g(2)],
            (true, true, false),
        ),
        case(
            "Unary",
            MInst::Unary {
                op: UnaryOp::Clz,
                dst: g(1),
                src: g(2),
                width: Width::X,
            },
            &[g(1)],
            &[g(2)],
            (false, true, false),
        ),
        case(
            "Extend",
            MInst::Extend {
                op: ExtOp::Sxtw,
                dst: g(1),
                src: VReg::new(2, RegClass::Gpr32),
            },
            &[g(1)],
            &[VReg::new(2, RegClass::Gpr32)],
            (false, true, false),
        ),
        case(
            "Bfx",
            MInst::Bfx {
                signed: false,
                dst: g(1),
                src: g(2),
                lsb: 3,
                bits: 9,
                width: Width::X,
            },
            &[g(1)],
            &[g(2)],
            (false, true, false),
        ),
        case(
            "Cmp",
            MInst::Cmp {
                dst: f(1),
                lhs: g(2),
                rhs: g(3),
                width: Width::X,
            },
            &[f(1)],
            &[g(2), g(3)],
            (true, true, false),
        ),
        case(
            "CmpImm",
            MInst::CmpImm {
                dst: f(1),
                lhs: g(2),
                imm: 7,
                width: Width::X,
            },
            &[f(1)],
            &[g(2)],
            (true, true, false),
        ),
        case(
            "Cset",
            MInst::Cset {
                dst: g(1),
                flags: f(2),
                cond: Cond::Eq,
            },
            &[g(1)],
            &[f(2)],
            (false, true, false),
        ),
        case(
            "Csel",
            MInst::Csel {
                dst: g(1),
                flags: f(4),
                lhs: g(2),
                rhs: g(3),
                cond: Cond::Eq,
                width: Width::X,
            },
            &[g(1)],
            &[f(4), g(2), g(3)],
            (false, true, false),
        ),
        case(
            "Bl",
            MInst::Bl {
                returns_twice: false,
                symbol: "callee".into(),
                args: vec![g(2), d(3)],
                results: vec![g(1), d(4)],
                fp_mode: None,
            },
            &[g(1), d(4)],
            &[g(2), d(3)],
            (false, false, true),
        ),
        case(
            "Blr",
            MInst::Blr {
                target: g(5),
                args: vec![g(2), d(3)],
                results: vec![g(1), d(4)],
            },
            &[g(1), d(4)],
            &[g(5), g(2), d(3)],
            (false, false, true),
        ),
        case(
            "Ldp",
            MInst::Ldp {
                rt1: g(1),
                rt2: g(2),
                base: g(3),
                offset: 16,
                writeback: false,
                volatile: false,
            },
            &[g(1), g(2)],
            &[g(3)],
            (false, false, false),
        ),
        case(
            "Stp",
            MInst::Stp {
                rt1: g(1),
                rt2: g(2),
                base: g(3),
                offset: 16,
                writeback: false,
                volatile: false,
            },
            &[],
            &[g(1), g(2), g(3)],
            (false, false, false),
        ),
        case(
            "SpillStore",
            MInst::SpillStore { src: g(2), slot: 7 },
            &[],
            &[g(2)],
            (false, false, false),
        ),
        case(
            "SpillReload",
            MInst::SpillReload { dst: g(1), slot: 7 },
            &[g(1)],
            &[],
            (false, false, false),
        ),
        case(
            "IncomingArg",
            MInst::IncomingArg { dst: g(1), slot: 7 },
            &[g(1)],
            &[],
            (false, false, false),
        ),
        case(
            "OutgoingArg",
            MInst::OutgoingArg { src: g(2), slot: 7 },
            &[],
            &[g(2)],
            (false, false, false),
        ),
        case(
            "MemCopy",
            MInst::MemCopy {
                dst: g(1),
                src: g(2),
                size: 16,
                align: 8,
                volatile: CopyVolatility::None,
            },
            &[],
            &[g(1), g(2)],
            (false, false, false),
        ),
        case(
            "FrameSlot",
            MInst::FrameSlot {
                dst: g(1),
                origin: 7,
                size: 16,
                align: 8,
            },
            &[g(1)],
            &[],
            // A fixed address has no effect if nothing uses it. Dynamic
            // allocation below remains observable and is not discardable.
            (false, true, false),
        ),
        case(
            "StackAlloc",
            MInst::StackAlloc {
                dst: g(1),
                size: g(2),
                origin: 7,
                align: 16,
            },
            &[g(1)],
            &[g(2)],
            (false, false, false),
        ),
        case(
            "StackRelease",
            MInst::StackRelease { address: g(2) },
            &[],
            &[g(2)],
            (false, false, false),
        ),
        case(
            "FrameAddress",
            MInst::FrameAddress {
                dst: g(1),
                place: FramePlace::FramePointer,
            },
            &[g(1)],
            &[],
            (false, false, false),
        ),
        case(
            "IndirectResult",
            MInst::IndirectResult,
            &[],
            &[],
            (false, false, false),
        ),
        case(
            "FpEnvCheck",
            MInst::FpEnvCheck { strict: false },
            &[],
            &[],
            (false, false, false),
        ),
        case(
            "FpEnvCheck",
            MInst::FpEnvCheck { strict: true },
            &[],
            &[],
            (false, false, false),
        ),
        case(
            "VaSaveArea",
            MInst::VaSaveArea { named: 3 },
            &[],
            &[],
            (false, false, false),
        ),
    ];
    for strict in [false, true] {
        for (op, args) in [
            (FloatingOp::Constant(0), vec![]),
            (FloatingOp::Neg, vec![d(2)]),
            (FloatingOp::Add, vec![d(2), d(3)]),
            (FloatingOp::Fma, vec![d(2), d(3), d(4)]),
        ] {
            rows.push(case(
                "Floating",
                MInst::Floating {
                    strict,
                    integer_bits: 0,
                    dst: d(1),
                    op,
                    kind: FloatKind::F64,
                    args: args.clone(),
                },
                &[d(1)],
                &args,
                (false, false, false),
            ));
        }
    }
    for (op, args) in [
        (AesOperation::Encrypt, vec![q(2), q(3)]),
        (AesOperation::Mix, vec![q(2)]),
    ] {
        rows.push(case(
            "Aes",
            MInst::Aes {
                dst: q(1),
                op,
                args: args.clone(),
            },
            &[q(1)],
            &args,
            (false, false, false),
        ));
    }
    for (extract, dst, args) in [
        (false, q(1), vec![g(2), g(3)]),
        (true, g(1), vec![q(2), g(3)]),
    ] {
        rows.push(case(
            "Vector",
            MInst::Vector {
                dst,
                extract,
                lane_bits: 64,
                bytes: 16,
                args: args.clone(),
            },
            &[dst],
            &args,
            (false, false, false),
        ));
    }
    for load in [false, true] {
        let (defs, uses) = if load {
            (vec![g(1)], vec![g(2)])
        } else {
            (vec![], vec![g(1), g(2)])
        };
        rows.push(case(
            "Atomic",
            MInst::Atomic {
                load,
                value: g(1),
                base: g(2),
                kind: LoadKind::X,
                order: AtomicOrder::SeqCst,
                volatile: true,
            },
            &defs,
            &uses,
            (false, false, false),
        ));
    }
    for expected in [None, Some(g(4))] {
        let mut uses = vec![g(2), g(3)];
        uses.extend(expected);
        rows.push(case(
            "AtomicRmw",
            MInst::AtomicRmw {
                dst: g(1),
                base: g(2),
                value: g(3),
                expected,
                operation: if expected.is_some() {
                    AtomicRmwOp::CompareExchange
                } else {
                    AtomicRmwOp::Add
                },
                kind: LoadKind::X,
                order: AtomicOrder::SeqCst,
                volatile: true,
            },
            &[g(1)],
            &uses,
            (false, false, false),
        ));
    }
    for flags in [false, true] {
        let mut row = case(
            "Asm",
            MInst::Asm(assembly(flags)),
            &[g(10), g(12)],
            &[g(13), g(12), g(10), g(11)],
            (flags, false, false),
        );
        row.rewrite_uses = vec![g(11), g(13), g(12)];
        rows.push(row);
    }
    // These public raw structures are not being admitted as executable asm.
    // Preserve the existing fact-query boundary even when direction metadata
    // would be rejected elsewhere: defined() ignores output direction, while
    // mutable slot classification consults direction().writes().
    let mut unusual = case(
        "Asm",
        MInst::Asm(AsmBody {
            template: String::new(),
            volatile: false,
            outputs: vec![AsmOperand::reg(
                AsmConstraint::Register(AsmDirection::Read),
                g(30),
            )],
            inputs: vec![AsmOperand::reg(
                AsmConstraint::Register(AsmDirection::Write),
                g(31),
            )],
            clobbers: vec![],
        }),
        &[g(30)],
        &[g(31)],
        (false, false, false),
    );
    unusual.rewrite_defs = vec![g(31)];
    unusual.rewrite_uses = vec![g(30)];
    rows.push(unusual);
    rows
}

#[test]
fn every_instruction_family_keeps_ordered_semantic_operands() {
    let cases = instruction_cases();
    let covered: BTreeSet<_> = cases.iter().map(|row| row.family).collect();
    assert_eq!(covered, INST_FAMILIES.iter().copied().collect());
    for row in cases {
        assert_eq!(MachInst::defs(&row.inst), row.defs, "{}", row.family);
        assert_eq!(MachInst::uses(&row.inst), row.uses, "{}", row.family);
        assert_eq!(
            inst_defs(&row.inst),
            row.defs,
            "{} compatibility defs",
            row.family
        );
        assert_eq!(
            inst_uses(&row.inst),
            row.uses,
            "{} compatibility uses",
            row.family
        );
    }
}

#[test]
fn every_instruction_family_keeps_mutable_occurrence_order() {
    for row in instruction_cases() {
        for (definitions, expected) in [(true, row.rewrite_defs), (false, row.rewrite_uses)] {
            let mut changed = row.inst.clone();
            {
                let slots = MachInst::operand_slots(&mut changed, definitions);
                let observed: Vec<_> = slots.iter().map(|slot| **slot).collect();
                assert_eq!(
                    observed, expected,
                    "{} definitions={definitions}",
                    row.family
                );
                for (index, slot) in slots.into_iter().enumerate() {
                    slot.id += 1000 + index as u32;
                }
            }
            let expected_changed: Vec<_> = expected
                .iter()
                .enumerate()
                .map(|(index, reg)| VReg::new(reg.id + 1000 + index as u32, reg.class))
                .collect();
            {
                let slots = MachInst::operand_slots(&mut changed, definitions);
                let observed: Vec<_> = slots.iter().map(|slot| **slot).collect();
                assert_eq!(observed, expected_changed, "{} changed slots", row.family);
                for (slot, original) in slots.into_iter().zip(&expected) {
                    *slot = *original;
                }
            }
            assert_eq!(
                changed, row.inst,
                "{} nonoperand payload changed",
                row.family
            );
        }
    }
}

#[test]
fn every_instruction_family_keeps_flags_call_and_dce_classification() {
    for row in instruction_cases() {
        assert_eq!(
            MachInst::writes_flags(&row.inst),
            row.flags,
            "{} flags",
            row.family
        );
        assert_eq!(
            writes_flags(&row.inst),
            row.flags,
            "{} flags compatibility",
            row.family
        );
        assert_eq!(
            MachInst::is_call(&row.inst),
            row.call,
            "{} call",
            row.family
        );
        assert_eq!(
            MachInst::discardable(&row.inst),
            row.discardable,
            "{} DCE",
            row.family
        );
    }
}

#[test]
fn assembly_read_write_occurrences_are_rewritten_once_in_the_original_order() {
    let mut inst = MInst::Asm(assembly(true));
    for slot in MachInst::operand_slots(&mut inst, false) {
        slot.id += 100;
    }
    for slot in MachInst::operand_slots(&mut inst, true) {
        slot.id += 1000;
    }
    assert_eq!(MachInst::defs(&inst), vec![g(1010), g(1012)]);
    assert_eq!(MachInst::uses(&inst), vec![g(113), g(112), g(1010), g(111)]);
    let MInst::Asm(body) = inst else {
        unreachable!()
    };
    assert_eq!(body.outputs[0].fixed_register, Some(19));
    assert_eq!(body.outputs[2].fixed_register, Some(20));
    assert_eq!(body.inputs[1].constraint, AsmConstraint::Tied(2));
}

struct TermCase {
    family: &'static str,
    term: MTerm,
    uses: Vec<VReg>,
    rewrite: Vec<VReg>,
    edges: Vec<(u32, Vec<VReg>)>,
    exit: bool,
}

fn terminator_cases() -> Vec<TermCase> {
    vec![
        TermCase {
            family: "Indirect",
            term: MTerm::Indirect {
                target: g(1),
                edges: vec![(7, vec![g(2), g(3)]), (8, vec![g(4)])],
            },
            uses: vec![g(1), g(2), g(3), g(4)],
            rewrite: vec![g(1), g(2), g(3), g(4)],
            edges: vec![(7, vec![g(2), g(3)]), (8, vec![g(4)])],
            exit: false,
        },
        TermCase {
            family: "B",
            term: MTerm::B {
                target: 7,
                args: vec![g(2), g(3)],
            },
            uses: vec![g(2), g(3)],
            rewrite: vec![],
            edges: vec![(7, vec![g(2), g(3)])],
            exit: false,
        },
        TermCase {
            family: "BCond",
            term: MTerm::BCond {
                flags: f(1),
                cond: Cond::Eq,
                taken: 7,
                taken_args: vec![g(2)],
                untaken: 8,
                untaken_args: vec![g(3)],
            },
            uses: vec![f(1), g(2), g(3)],
            rewrite: vec![f(1)],
            edges: vec![(7, vec![g(2)]), (8, vec![g(3)])],
            exit: false,
        },
        TermCase {
            family: "Cb",
            term: MTerm::Cb {
                nonzero: true,
                src: g(1),
                taken: 7,
                taken_args: vec![g(2)],
                untaken: 8,
                untaken_args: vec![g(3)],
            },
            uses: vec![g(1), g(2), g(3)],
            rewrite: vec![g(1)],
            edges: vec![(7, vec![g(2)]), (8, vec![g(3)])],
            exit: false,
        },
        TermCase {
            family: "Tb",
            term: MTerm::Tb {
                nonzero: true,
                src: g(1),
                bit: 2,
                taken: 7,
                taken_args: vec![g(2)],
                untaken: 8,
                untaken_args: vec![g(3)],
            },
            uses: vec![g(1), g(2), g(3)],
            rewrite: vec![g(1)],
            edges: vec![(7, vec![g(2)]), (8, vec![g(3)])],
            exit: false,
        },
        TermCase {
            family: "Ret",
            term: MTerm::Ret {
                values: vec![g(2), d(3)],
            },
            uses: vec![g(2), d(3)],
            rewrite: vec![g(2), d(3)],
            edges: vec![],
            exit: true,
        },
        TermCase {
            family: "Brk",
            term: MTerm::Brk { imm: 3001 },
            uses: vec![],
            rewrite: vec![],
            edges: vec![],
            exit: true,
        },
        TermCase {
            family: "AsmGoto",
            term: MTerm::AsmGoto {
                body: assembly(true),
                labels: vec![(7, vec![g(20)]), (8, vec![g(21)])],
                fallthrough: 9,
                fallthrough_args: vec![g(22)],
            },
            uses: vec![g(13), g(12), g(10), g(11), g(20), g(21), g(22)],
            rewrite: vec![g(10), g(11), g(12), g(13), g(12)],
            edges: vec![(7, vec![g(20)]), (8, vec![g(21)]), (9, vec![g(22)])],
            exit: false,
        },
    ]
}

#[test]
fn every_terminator_family_keeps_uses_edges_and_existing_exit_classification() {
    assert!(std::hint::black_box(TRAP_IS_EXIT));
    assert!(!std::hint::black_box(EMPTY_JUMP_IS_EXIT));
    let cases = terminator_cases();
    assert_eq!(
        cases.iter().map(|row| row.family).collect::<Vec<_>>(),
        TERM_FAMILIES
    );
    for row in cases {
        assert_eq!(MachTerm::uses(&row.term), row.uses, "{} uses", row.family);
        assert_eq!(
            term_uses(&row.term),
            row.uses,
            "{} uses compatibility",
            row.family
        );
        let observed: Vec<_> = MachTerm::edges(&row.term)
            .into_iter()
            .map(|(id, args)| (id, args.to_vec()))
            .collect();
        assert_eq!(observed, row.edges, "{} edges", row.family);
        let old: Vec<_> = term_edges(&row.term)
            .into_iter()
            .map(|(id, args)| (id, args.to_vec()))
            .collect();
        assert_eq!(old, row.edges, "{} edges compatibility", row.family);
        // Match allocverify::is_exit, not edges().is_empty().
        assert_eq!(
            MachTerm::is_exit(&row.term),
            row.exit,
            "{} exit",
            row.family
        );
        assert_eq!(evocc_mir::allocverify::is_exit(&row.term), row.exit);
    }
}

#[test]
fn terminator_mutable_slots_match_coalescing_not_all_semantic_uses() {
    for row in terminator_cases() {
        let mut term = row.term.clone();
        {
            let slots = MachTerm::operand_slots(&mut term);
            let observed: Vec<_> = slots.iter().map(|slot| **slot).collect();
            assert_eq!(observed, row.rewrite, "{} coalescing slots", row.family);
            for (index, slot) in slots.into_iter().enumerate() {
                slot.id += 1000 + index as u32;
            }
        }
        {
            let slots = MachTerm::operand_slots(&mut term);
            let expected: Vec<_> = row
                .rewrite
                .iter()
                .enumerate()
                .map(|(index, reg)| VReg::new(reg.id + 1000 + index as u32, reg.class))
                .collect();
            assert_eq!(
                slots.iter().map(|slot| **slot).collect::<Vec<_>>(),
                expected
            );
            for (slot, original) in slots.into_iter().zip(&row.rewrite) {
                *slot = *original;
            }
        }
        assert_eq!(term, row.term, "{} nonoperand payload changed", row.family);
    }
}

#[test]
fn indirect_edges_are_rewritten_but_direct_edge_arguments_are_not() {
    let mut indirect = MTerm::Indirect {
        target: g(1),
        edges: vec![(7, vec![g(1), g(2)])],
    };
    for slot in MachTerm::operand_slots(&mut indirect) {
        if slot.id == 1 {
            *slot = g(90);
        }
    }
    assert_eq!(MachTerm::uses(&indirect), vec![g(90), g(90), g(2)]);
    let mut direct = MTerm::BCond {
        flags: f(1),
        cond: Cond::Eq,
        taken: 7,
        taken_args: vec![g(2)],
        untaken: 8,
        untaken_args: vec![g(3)],
    };
    for slot in MachTerm::operand_slots(&mut direct) {
        *slot = f(90);
    }
    assert_eq!(MachTerm::uses(&direct), vec![f(90), g(2), g(3)]);
}

#[test]
fn an_edge_less_raw_indirect_is_not_an_exit() {
    // Classification preserves the current enum match; this is not validation
    // or native admission of a computed branch without an edge domain.
    let raw = MTerm::Indirect {
        target: g(1),
        edges: vec![],
    };
    assert!(MachTerm::edges(&raw).is_empty());
    assert!(!MachTerm::is_exit(&raw));
    assert!(!evocc_mir::allocverify::is_exit(&raw));
}
