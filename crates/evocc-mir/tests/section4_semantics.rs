//! Section 4: the executable semantics of every form this revision emits, and
//! the Section 3 sub-width rules that make a form safe to use on a value
//! narrower than its register.

use evocc_mir::{
    interpret_module, BinOp, Cond, ExtOp, MBlock, MFunction, MInst, MModule, MTerm, MirError,
    MirLimits, MirValue, RegClass, VReg, Width,
};

fn g(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr64)
}

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

fn f(id: u32) -> VReg {
    VReg::new(id, RegClass::Flags)
}

fn run(args: Vec<VReg>, insts: Vec<MInst>, term: MTerm, inputs: &[MirValue]) -> Option<MirValue> {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "main".to_owned(),
            blocks: vec![MBlock {
                id: 0,
                args,
                insts,
                term,
            }],
        }],
    };
    interpret_module(&module, "main", inputs, MirLimits::new(10_000))
        .expect("execution")
        .result
}

fn int(value: u64) -> MirValue {
    MirValue::Int(value)
}

fn bits(value: Option<MirValue>) -> u64 {
    match value {
        Some(MirValue::Int(raw)) => raw,
        other => panic!("expected raw bits, got {other:?}"),
    }
}

#[test]
fn a_thirty_two_bit_destination_clears_the_upper_half() {
    let result = run(
        vec![g(0)],
        vec![MInst::Binary {
            op: BinOp::Add,
            dst: w(1),
            lhs: g(0),
            rhs: g(0),
            width: Width::W,
        }],
        MTerm::Ret { value: Some(w(1)) },
        &[int(0xffff_ffff_0000_0001)],
    );
    assert_eq!(bits(result), 2);
}

#[test]
fn wrapping_arithmetic_matches_the_register_width() {
    let result = run(
        vec![w(0), w(1)],
        vec![MInst::Binary {
            op: BinOp::Sub,
            dst: w(2),
            lhs: w(0),
            rhs: w(1),
            width: Width::W,
        }],
        MTerm::Ret { value: Some(w(2)) },
        &[int(1), int(2)],
    );
    assert_eq!(bits(result), 0xffff_ffff);
}

#[test]
fn msub_computes_a_remainder() {
    // rd := ra - rn * rm, the Section 4 spelling of a remainder after a divide.
    let result = run(
        vec![w(0), w(1)],
        vec![
            MInst::Binary {
                op: BinOp::UDiv,
                dst: w(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Msub {
                dst: w(3),
                n: w(2),
                m: w(1),
                a: w(0),
                width: Width::W,
            },
        ],
        MTerm::Ret { value: Some(w(3)) },
        &[int(17), int(5)],
    );
    assert_eq!(bits(result), 2);
}

#[test]
fn division_by_zero_yields_zero_on_the_machine() {
    // Section 4: "division by zero yields 0 on the machine, which is why
    // EvoIR's divisor_nonzero guard must have been asserted before".
    let result = run(
        vec![w(0), w(1)],
        vec![MInst::Binary {
            op: BinOp::SDiv,
            dst: w(2),
            lhs: w(0),
            rhs: w(1),
            width: Width::W,
        }],
        MTerm::Ret { value: Some(w(2)) },
        &[int(9), int(0)],
    );
    assert_eq!(bits(result), 0);
}

#[test]
fn a_register_shift_uses_the_low_five_or_six_bits_of_the_amount() {
    let result = run(
        vec![w(0), w(1)],
        vec![MInst::Binary {
            op: BinOp::Lsl,
            dst: w(2),
            lhs: w(0),
            rhs: w(1),
            width: Width::W,
        }],
        MTerm::Ret { value: Some(w(2)) },
        &[int(1), int(33)],
    );
    assert_eq!(bits(result), 2, "33 mod 32 is one");
}

#[test]
fn asr_replicates_the_sign_bit() {
    let result = run(
        vec![w(0)],
        vec![MInst::BinaryImm {
            op: BinOp::Asr,
            dst: w(1),
            lhs: w(0),
            imm: 4,
            width: Width::W,
        }],
        MTerm::Ret { value: Some(w(1)) },
        &[int(0xffff_fff0)],
    );
    assert_eq!(bits(result), 0xffff_ffff);
}

#[test]
fn sbfx_sign_extends_from_the_named_bit() {
    let result = run(
        vec![w(0)],
        vec![MInst::Bfx {
            signed: true,
            dst: w(1),
            src: w(0),
            lsb: 0,
            bits: 3,
            width: Width::W,
        }],
        MTerm::Ret { value: Some(w(1)) },
        &[int(0b101)],
    );
    assert_eq!(bits(result), 0xffff_fffd);
}

#[test]
fn ubfx_zero_extends_from_the_named_bit() {
    let result = run(
        vec![w(0)],
        vec![MInst::Bfx {
            signed: false,
            dst: w(1),
            src: w(0),
            lsb: 0,
            bits: 3,
            width: Width::W,
        }],
        MTerm::Ret { value: Some(w(1)) },
        &[int(0b1111_1101)],
    );
    assert_eq!(bits(result), 0b101);
}

#[test]
fn the_extension_forms_read_their_fixed_widths() {
    for (op, input, expected) in [
        (ExtOp::Sxtb, 0x80_u64, 0xffff_ff80_u64),
        (ExtOp::Uxtb, 0x1ff, 0xff),
        (ExtOp::Sxth, 0x8000, 0xffff_8000),
        (ExtOp::Uxth, 0x1_ffff, 0xffff),
    ] {
        let result = run(
            vec![w(0)],
            vec![MInst::Extend {
                op,
                dst: w(1),
                src: w(0),
            }],
            MTerm::Ret { value: Some(w(1)) },
            &[int(input)],
        );
        assert_eq!(bits(result), expected, "{op:?}");
    }
}

#[test]
fn cset_writes_zero_or_one_into_the_whole_register() {
    for (cond, lhs, rhs, expected) in [
        (Cond::Eq, 3_u64, 3_u64, 1_u64),
        (Cond::Ne, 3, 3, 0),
        (Cond::Lt, 0xffff_ffff, 1, 1),
        (Cond::Lo, 0xffff_ffff, 1, 0),
        (Cond::Hs, 0xffff_ffff, 1, 1),
        (Cond::Le, 2, 2, 1),
        (Cond::Gt, 2, 2, 0),
        (Cond::Ge, 2, 2, 1),
        (Cond::Ls, 2, 2, 1),
        (Cond::Hi, 2, 2, 0),
    ] {
        let result = run(
            vec![w(0), w(1)],
            vec![
                MInst::Cmp {
                    dst: f(2),
                    lhs: w(0),
                    rhs: w(1),
                    width: Width::W,
                },
                MInst::Cset {
                    dst: w(3),
                    flags: f(2),
                    cond,
                },
            ],
            MTerm::Ret { value: Some(w(3)) },
            &[int(lhs), int(rhs)],
        );
        assert_eq!(bits(result), expected, "{cond:?}");
    }
}

#[test]
fn csel_selects_on_the_condition() {
    let result = run(
        vec![w(0), w(1)],
        vec![
            MInst::Cmp {
                dst: f(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Csel {
                dst: w(3),
                flags: f(2),
                lhs: w(0),
                rhs: w(1),
                cond: Cond::Lo,
                width: Width::W,
            },
        ],
        MTerm::Ret { value: Some(w(3)) },
        &[int(4), int(9)],
    );
    assert_eq!(bits(result), 4);
}

#[test]
fn tbnz_branches_on_bit_zero() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "main".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: Vec::new(),
                    term: MTerm::Tb {
                        nonzero: true,
                        src: w(0),
                        bit: 0,
                        taken: 1,
                        taken_args: Vec::new(),
                        untaken: 2,
                        untaken_args: Vec::new(),
                    },
                },
                MBlock {
                    id: 1,
                    args: Vec::new(),
                    insts: vec![MInst::MovImm { dst: w(1), imm: 7 }],
                    term: MTerm::Ret { value: Some(w(1)) },
                },
                MBlock {
                    id: 2,
                    args: Vec::new(),
                    insts: vec![MInst::MovImm { dst: w(2), imm: 9 }],
                    term: MTerm::Ret { value: Some(w(2)) },
                },
            ],
        }],
    };
    let taken = interpret_module(&module, "main", &[int(0b10)], MirLimits::new(100))
        .expect("run")
        .result;
    assert_eq!(bits(taken), 9, "bit zero is clear");
    let untaken = interpret_module(&module, "main", &[int(0b11)], MirLimits::new(100))
        .expect("run")
        .result;
    assert_eq!(bits(untaken), 7, "bit zero is set");
}

#[test]
fn brk_traps_with_its_immediate() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "main".to_owned(),
            blocks: vec![MBlock {
                id: 0,
                args: Vec::new(),
                insts: Vec::new(),
                term: MTerm::Brk { imm: 3002 },
            }],
        }],
    };
    let error = interpret_module(&module, "main", &[], MirLimits::new(100)).unwrap_err();
    assert_eq!(error, MirError::Trap { imm: 3002 });
}

#[test]
fn the_step_budget_stops_a_loop() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "main".to_owned(),
            blocks: vec![MBlock {
                id: 0,
                args: Vec::new(),
                insts: Vec::new(),
                term: MTerm::B {
                    target: 0,
                    args: Vec::new(),
                },
            }],
        }],
    };
    let error = interpret_module(&module, "main", &[], MirLimits::new(64)).unwrap_err();
    assert_eq!(error, MirError::BudgetExhausted);
}

#[test]
fn bl_passes_arguments_and_returns_one_result() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![
            MFunction {
                name: "twice".to_owned(),
                blocks: vec![MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: vec![MInst::Binary {
                        op: BinOp::Add,
                        dst: w(1),
                        lhs: w(0),
                        rhs: w(0),
                        width: Width::W,
                    }],
                    term: MTerm::Ret { value: Some(w(1)) },
                }],
            },
            MFunction {
                name: "main".to_owned(),
                blocks: vec![MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: vec![MInst::Bl {
                        symbol: "twice".to_owned(),
                        args: vec![w(0)],
                        result: Some(w(1)),
                    }],
                    term: MTerm::Ret { value: Some(w(1)) },
                }],
            },
        ],
    };
    let result = interpret_module(&module, "main", &[int(21)], MirLimits::new(1000))
        .expect("run")
        .result;
    assert_eq!(bits(result), 42);
}

#[test]
fn a_frame_slot_is_a_fresh_object_on_every_execution() {
    // EvoIR Section 3.7: every dynamic execution of an alloca receives a fresh
    // object identity and the next dynamic instance of its origin site. A slot
    // whose address were computed once per activation could not do that, so
    // two executions of the same slot must not alias.
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "main".to_owned(),
            blocks: vec![MBlock {
                id: 0,
                args: Vec::new(),
                insts: vec![
                    MInst::FrameSlot {
                        dst: g(0),
                        origin: 0,
                        size: 8,
                        align: 8,
                    },
                    MInst::FrameSlot {
                        dst: g(1),
                        origin: 0,
                        size: 8,
                        align: 8,
                    },
                    MInst::MovImm { dst: g(2), imm: 5 },
                    MInst::Store {
                        src: g(2),
                        base: g(0),
                        offset: 0,
                        kind: evocc_mir::StoreKind::X,
                        volatile: false,
                    },
                    MInst::MovImm { dst: g(3), imm: 9 },
                    MInst::Store {
                        src: g(3),
                        base: g(1),
                        offset: 0,
                        kind: evocc_mir::StoreKind::X,
                        volatile: false,
                    },
                    MInst::Load {
                        dst: g(4),
                        base: g(0),
                        offset: 0,
                        kind: evocc_mir::LoadKind::X,
                        volatile: false,
                    },
                ],
                term: MTerm::Ret { value: Some(g(4)) },
            }],
        }],
    };
    let result = interpret_module(&module, "main", &[], MirLimits::new(1000))
        .expect("run")
        .result;
    assert_eq!(bits(result), 5, "the second slot is a different object");
}

#[test]
fn an_unmasked_comparison_reads_the_bits_above_a_narrow_value() {
    // The counterexample to Section 3's first row. It lists icmp.eq and icmp.ne
    // among the operations that need no inserted extension, "the low N bits of
    // the result depend only on the low N bits of the operands". Two bits<3>
    // values whose low three bits are both 0b101 compare unequal under a plain
    // cmp.w, because trunc inserts nothing and the bits above bit 2 survive.
    // The lowering therefore gives equality the same zero-extension as an
    // unsigned ordered comparison; this test pins what the literal reading
    // would produce.
    let unmasked = run(
        vec![w(0), w(1)],
        vec![
            MInst::Cmp {
                dst: f(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(3),
                flags: f(2),
                cond: Cond::Eq,
            },
        ],
        MTerm::Ret { value: Some(w(3)) },
        &[int(0b1101), int(0b0101)],
    );
    assert_eq!(bits(unmasked), 0, "the bits above bit two decided it");

    let masked = run(
        vec![w(0), w(1)],
        vec![
            MInst::BinaryImm {
                op: BinOp::And,
                dst: w(4),
                lhs: w(0),
                imm: 0b111,
                width: Width::W,
            },
            MInst::BinaryImm {
                op: BinOp::And,
                dst: w(5),
                lhs: w(1),
                imm: 0b111,
                width: Width::W,
            },
            MInst::Cmp {
                dst: f(2),
                lhs: w(4),
                rhs: w(5),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(3),
                flags: f(2),
                cond: Cond::Eq,
            },
        ],
        MTerm::Ret { value: Some(w(3)) },
        &[int(0b1101), int(0b0101)],
    );
    assert_eq!(
        bits(masked),
        1,
        "the inserted mask restores the EvoIR answer"
    );
}
