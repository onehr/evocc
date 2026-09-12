//! Appendix A: the canonical textual form.
//!
//! The specification prints exactly one pre-allocation function, so that
//! function is the anchor: the renderer must produce those bytes and the
//! parser must read them back into the same module. Every other shape here is
//! the same conventions carried to a form Appendix A does not print, which is
//! a recorded gap rather than a rule this crate invented.

use evocc_mir::{
    parse_module, render_module, BinOp, Cond, LoadKind, MBlock, MFunction, MInst, MModule, MTerm,
    RegClass, StoreKind, VReg, Width,
};

/// The exact block printed by Appendix A under "Before allocation".
const APPENDIX_A: &str = "func @dot {\n\
^entry(%v0:gpr64, %v1:gpr64):\n\
\x20   %v2:gpr32 = ldr.w [%v0, #0]\n\
\x20   %v3:gpr32 = ldr.w [%v1, #0]\n\
\x20   %v4:gpr32 = mul.w %v2, %v3\n\
\x20   ret %v4\n\
}\n";

fn appendix_a_module() -> MModule {
    let v0 = VReg::new(0, RegClass::Gpr64);
    let v1 = VReg::new(1, RegClass::Gpr64);
    let v2 = VReg::new(2, RegClass::Gpr32);
    let v3 = VReg::new(3, RegClass::Gpr32);
    let v4 = VReg::new(4, RegClass::Gpr32);
    MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "dot".to_owned(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![v0, v1],
                insts: vec![
                    MInst::Load {
                        dst: v2,
                        base: v0,
                        offset: 0,
                        kind: LoadKind::W,
                        volatile: false,
                    },
                    MInst::Load {
                        dst: v3,
                        base: v1,
                        offset: 0,
                        kind: LoadKind::W,
                        volatile: false,
                    },
                    MInst::Binary {
                        op: BinOp::Mul,
                        dst: v4,
                        lhs: v2,
                        rhs: v3,
                        width: Width::W,
                    },
                ],
                term: MTerm::Ret { values: vec![v4] },
            }],
        }],
    }
}

#[test]
fn the_renderer_emits_the_appendix_a_bytes() {
    assert_eq!(render_module(&appendix_a_module()), APPENDIX_A);
}

#[test]
fn the_parser_reads_the_appendix_a_bytes() {
    assert_eq!(
        parse_module(APPENDIX_A).expect("parse"),
        appendix_a_module()
    );
}

/// Every form this revision emits, in one function, so a spelling that drifts
/// is a failure rather than a discrepancy nobody notices.
fn every_form() -> MModule {
    let g = |id| VReg::new(id, RegClass::Gpr64);
    let w = |id| VReg::new(id, RegClass::Gpr32);
    let f = |id| VReg::new(id, RegClass::Flags);
    MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "shapes".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![g(0), w(1)],
                    insts: vec![
                        MInst::Mov {
                            dst: w(2),
                            src: w(1),
                        },
                        MInst::MovImm { dst: w(3), imm: 42 },
                        MInst::Adrp {
                            dst: g(4),
                            symbol: "counter".to_owned(),
                            kind: evocc_mir::SymbolKind::Global,
                        },
                        MInst::AddPageOff {
                            dst: g(5),
                            src: g(4),
                            symbol: "counter".to_owned(),
                            kind: evocc_mir::SymbolKind::Global,
                        },
                        MInst::Load {
                            dst: w(6),
                            base: g(5),
                            offset: 4,
                            kind: LoadKind::B,
                            volatile: true,
                        },
                        MInst::Store {
                            src: w(6),
                            base: g(5),
                            offset: -8,
                            kind: StoreKind::H,
                            volatile: false,
                        },
                        MInst::BinaryImm {
                            op: BinOp::Add,
                            dst: w(7),
                            lhs: w(6),
                            imm: 3,
                            width: Width::W,
                        },
                        MInst::Msub {
                            dst: g(8),
                            n: g(0),
                            m: g(0),
                            a: g(0),
                            width: Width::X,
                        },
                        MInst::Extend {
                            op: evocc_mir::ExtOp::Sxtb,
                            dst: w(9),
                            src: w(7),
                        },
                        MInst::Bfx {
                            signed: true,
                            dst: w(10),
                            src: w(9),
                            lsb: 0,
                            bits: 5,
                            width: Width::W,
                        },
                        MInst::Bfx {
                            signed: false,
                            dst: g(11),
                            src: g(8),
                            lsb: 2,
                            bits: 33,
                            width: Width::X,
                        },
                        MInst::FrameSlot {
                            dst: g(12),
                            origin: 3,
                            size: 8,
                            align: 8,
                        },
                        MInst::Bl {
                            symbol: "helper".to_owned(),
                            args: vec![g(12), w(10)],
                            results: vec![w(13), g(20)],
                        },
                        MInst::Blr {
                            target: g(11),
                            args: Vec::new(),
                            results: Vec::new(),
                        },
                        MInst::MemCopy {
                            dst: g(12),
                            src: g(20),
                            size: 16,
                            align: 8,
                            volatile: evocc_memory::CopyVolatility::None,
                        },
                        MInst::WideMultiply {
                            op: evocc_mir::WideMul::Smull,
                            dst: g(21),
                            lhs: w(13),
                            rhs: w(10),
                        },
                        MInst::WideMultiply {
                            op: evocc_mir::WideMul::Smulh,
                            dst: g(22),
                            lhs: g(21),
                            rhs: g(20),
                        },
                        MInst::Cmp {
                            dst: f(14),
                            lhs: w(13),
                            rhs: w(10),
                            width: Width::W,
                        },
                        MInst::Cset {
                            dst: w(15),
                            flags: f(14),
                            cond: Cond::Lo,
                        },
                        MInst::CmpImm {
                            dst: f(16),
                            lhs: w(15),
                            imm: 0,
                            width: Width::W,
                        },
                        MInst::Csel {
                            dst: w(17),
                            flags: f(16),
                            lhs: w(15),
                            rhs: w(13),
                            cond: Cond::Ne,
                            width: Width::W,
                        },
                    ],
                    term: MTerm::Tb {
                        nonzero: true,
                        src: w(17),
                        bit: 0,
                        taken: 1,
                        taken_args: vec![w(17)],
                        untaken: 2,
                        untaken_args: Vec::new(),
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![w(18)],
                    insts: vec![
                        MInst::FlagBinary {
                            op: evocc_mir::FlagOp::Adds,
                            dst: w(23),
                            flags: f(24),
                            lhs: w(18),
                            rhs: w(18),
                            width: Width::W,
                        },
                        MInst::Cset {
                            dst: w(25),
                            flags: f(24),
                            cond: Cond::Vc,
                        },
                        MInst::Cmp {
                            dst: f(19),
                            lhs: w(18),
                            rhs: w(25),
                            width: Width::W,
                        },
                    ],
                    term: MTerm::BCond {
                        flags: f(19),
                        cond: Cond::Ge,
                        taken: 2,
                        taken_args: Vec::new(),
                        untaken: 3,
                        untaken_args: Vec::new(),
                    },
                },
                MBlock {
                    id: 2,
                    args: Vec::new(),
                    insts: Vec::new(),
                    term: MTerm::Brk { imm: 3002 },
                },
                MBlock {
                    id: 3,
                    args: Vec::new(),
                    insts: Vec::new(),
                    term: MTerm::Ret {
                        values: vec![w(2), g(0)],
                    },
                },
            ],
        }],
    }
}

#[test]
fn every_form_round_trips_byte_for_byte() {
    let module = every_form();
    let text = render_module(&module);
    let parsed = parse_module(&text).expect("parse");
    assert_eq!(parsed, module);
    assert_eq!(render_module(&parsed), text);
}

#[test]
fn canonical_bytes_obey_the_byte_form_rules() {
    let text = render_module(&every_form());
    assert!(text.ends_with('\n'), "the final byte is a newline");
    assert!(!text.ends_with("\n\n"), "no trailing blank line");
    assert!(!text.contains('\t'), "never a tab");
    assert!(!text.contains('\r'), "never a carriage return");
    for line in text.lines() {
        assert_eq!(line, line.trim_end(), "no trailing whitespace on any line");
    }
}

#[test]
fn a_parser_rejects_a_non_canonical_indent() {
    let text = APPENDIX_A.replace("    %v2", "  %v2");
    assert!(parse_module(&text).is_err());
}

#[test]
fn a_parser_rejects_an_unknown_mnemonic() {
    let text = APPENDIX_A.replace("mul.w", "fmul.w");
    assert!(parse_module(&text).is_err());
}

/// Revision 0.2's grammar writes a conditional terminator without naming the
/// register that set the flags, so the parser recovers it from the instruction
/// Section 2's adjacency rule requires to precede it.
#[test]
fn a_conditional_terminator_names_no_condition_register() {
    let text = render_module(&every_form());
    assert!(text.contains("    b.ge, ^b2(), ^b3()\n"), "{text}");
    assert!(!text.contains("b.cond"), "{text}");
}

/// A call writes its results after the arrow, and both are definitions.
#[test]
fn a_call_spells_its_results_after_an_arrow() {
    let text = render_module(&every_form());
    assert!(
        text.contains("    bl @helper(%v12, %v10) -> %v13:gpr32, %v20:gpr64\n"),
        "{text}"
    );
    assert!(text.contains("    blr %v11()\n"), "{text}");
}

#[test]
fn the_two_pseudo_forms_have_their_own_spellings() {
    let text = render_module(&every_form());
    assert!(
        text.contains("    %v12:gpr64 = frame.slot [origin=3, size=8, align=8]\n"),
        "{text}"
    );
    assert!(
        text.contains("    mem.copy [%v12], [%v20], #16, #8\n"),
        "{text}"
    );
}

/// `adds` and `subs` write a register and the flags, which the grammar's
/// single-destination production cannot spell; both definitions are written
/// before the `=`.
#[test]
fn a_flag_setting_arithmetic_form_spells_both_destinations() {
    let text = render_module(&every_form());
    assert!(
        text.contains("    %v23:gpr32, %v24:flags = adds.w %v18, %v18\n"),
        "{text}"
    );
}

#[test]
fn a_function_returns_up_to_eight_results() {
    let text = render_module(&every_form());
    assert!(text.contains("    ret %v2, %v0\n"), "{text}");
}
