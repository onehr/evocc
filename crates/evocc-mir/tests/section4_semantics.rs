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
        .results
        .into_iter()
        .next()
}

fn results(args: Vec<VReg>, insts: Vec<MInst>, term: MTerm, inputs: &[MirValue]) -> Vec<MirValue> {
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
        .results
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
        MTerm::Ret { values: vec![w(1)] },
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
        MTerm::Ret { values: vec![w(2)] },
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
        MTerm::Ret { values: vec![w(3)] },
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
        MTerm::Ret { values: vec![w(2)] },
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
        MTerm::Ret { values: vec![w(2)] },
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
        MTerm::Ret { values: vec![w(1)] },
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
        MTerm::Ret { values: vec![w(1)] },
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
        MTerm::Ret { values: vec![w(1)] },
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
            MTerm::Ret { values: vec![w(1)] },
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
            MTerm::Ret { values: vec![w(3)] },
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
        MTerm::Ret { values: vec![w(3)] },
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
                    term: MTerm::Ret { values: vec![w(1)] },
                },
                MBlock {
                    id: 2,
                    args: Vec::new(),
                    insts: vec![MInst::MovImm { dst: w(2), imm: 9 }],
                    term: MTerm::Ret { values: vec![w(2)] },
                },
            ],
        }],
    };
    let taken = interpret_module(&module, "main", &[int(0b10)], MirLimits::new(100))
        .expect("run")
        .results
        .into_iter()
        .next();
    assert_eq!(bits(taken), 9, "bit zero is clear");
    let untaken = interpret_module(&module, "main", &[int(0b11)], MirLimits::new(100))
        .expect("run")
        .results
        .into_iter()
        .next();
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
                    term: MTerm::Ret { values: vec![w(1)] },
                }],
            },
            MFunction {
                name: "main".to_owned(),
                blocks: vec![MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: vec![MInst::Bl {
                        returns_twice: false,
                        fp_mode: None,
                        symbol: "twice".to_owned(),
                        args: vec![w(0)],
                        results: vec![w(1)],
                    }],
                    term: MTerm::Ret { values: vec![w(1)] },
                }],
            },
        ],
    };
    let result = interpret_module(&module, "main", &[int(21)], MirLimits::new(1000))
        .expect("run")
        .results
        .into_iter()
        .next();
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
                        align: 0,
                        src: g(2),
                        base: g(0),
                        offset: 0,
                        kind: evocc_mir::StoreKind::X,
                        volatile: false,
                        mask: None,
                    },
                    MInst::MovImm { dst: g(3), imm: 9 },
                    MInst::Store {
                        align: 0,
                        src: g(3),
                        base: g(1),
                        offset: 0,
                        kind: evocc_mir::StoreKind::X,
                        volatile: false,
                        mask: None,
                    },
                    MInst::Load {
                        align: 0,
                        dst: g(4),
                        base: g(0),
                        offset: 0,
                        kind: evocc_mir::LoadKind::X,
                        volatile: false,
                        mask: None,
                    },
                ],
                term: MTerm::Ret { values: vec![g(4)] },
            }],
        }],
    };
    let result = interpret_module(&module, "main", &[], MirLimits::new(1000))
        .expect("run")
        .results
        .into_iter()
        .next();
    assert_eq!(bits(result), 5, "the second slot is a different object");
}

#[test]
fn an_unmasked_comparison_reads_the_bits_above_a_narrow_value() {
    // Revision 0.2 moved icmp.eq and icmp.ne into Section 3's zero-extended
    // row and prints this input as the reason: "a comparison reads every bit
    // of the register, so trunc.bits<3> of 0xffffffff compared with the
    // constant 7 answered 0 where EvoIR answers 1". This pins what the row it
    // was moved out of would still produce.
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
        MTerm::Ret { values: vec![w(3)] },
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
        MTerm::Ret { values: vec![w(3)] },
        &[int(0b1101), int(0b0101)],
    );
    assert_eq!(
        bits(masked),
        1,
        "the inserted mask restores the EvoIR answer"
    );
}

#[test]
fn adds_and_subs_write_a_result_and_the_overflow_flag() {
    // Section 4: "`adds`, `subs rd, rn, rm` | as `add`/`sub`, and write `nzcv`;
    // `v` is the signed-overflow flag, `c` the carry".
    for (op, lhs, rhs, sum, overflowed) in [
        (evocc_mir::FlagOp::Adds, 1_u64, 2_u64, 3_u64, false),
        (evocc_mir::FlagOp::Adds, 0x7fff_ffff, 1, 0x8000_0000, true),
        (evocc_mir::FlagOp::Subs, 5, 3, 2, false),
        (evocc_mir::FlagOp::Subs, 0x8000_0000, 1, 0x7fff_ffff, true),
    ] {
        let value = run(
            vec![w(0), w(1)],
            vec![
                MInst::FlagBinary {
                    op,
                    dst: w(2),
                    flags: f(3),
                    lhs: w(0),
                    rhs: w(1),
                    width: Width::W,
                },
                MInst::Cset {
                    dst: w(4),
                    flags: f(3),
                    cond: Cond::Vc,
                },
            ],
            MTerm::Ret {
                values: vec![w(2), w(4)],
            },
            &[int(lhs), int(rhs)],
        );
        assert_eq!(value, Some(MirValue::Int(sum)), "{op:?} {lhs} {rhs}");
        let defined = results(
            vec![w(0), w(1)],
            vec![
                MInst::FlagBinary {
                    op,
                    dst: w(2),
                    flags: f(3),
                    lhs: w(0),
                    rhs: w(1),
                    width: Width::W,
                },
                MInst::Cset {
                    dst: w(4),
                    flags: f(3),
                    cond: Cond::Vc,
                },
            ],
            MTerm::Ret {
                values: vec![w(2), w(4)],
            },
            &[int(lhs), int(rhs)],
        );
        assert_eq!(
            defined[1],
            MirValue::Int(u64::from(!overflowed)),
            "{op:?} {lhs} {rhs}"
        );
    }
}

#[test]
fn the_carry_conditions_read_a_subtraction() {
    for (lhs, rhs, borrowed) in [(3_u64, 5_u64, true), (5, 3, false), (4, 4, false)] {
        let value = run(
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
                    cond: Cond::Cc,
                },
            ],
            MTerm::Ret { values: vec![w(3)] },
            &[int(lhs), int(rhs)],
        );
        assert_eq!(bits(value), u64::from(borrowed), "{lhs} {rhs}");
    }
}

#[test]
fn the_wide_signed_multiplies_produce_exact_products() {
    // smull: the signed 32x32 product, exact in 64 bits.
    let low = run(
        vec![w(0), w(1)],
        vec![MInst::WideMultiply {
            op: evocc_mir::WideMul::Smull,
            dst: g(2),
            lhs: w(0),
            rhs: w(1),
        }],
        MTerm::Ret { values: vec![g(2)] },
        &[int(0xffff_ffff), int(3)],
    );
    assert_eq!(bits(low), (-3_i64) as u64, "minus one times three");

    // smulh: the high half of the signed 64x64 product.
    let high = run(
        vec![g(0), g(1)],
        vec![MInst::WideMultiply {
            op: evocc_mir::WideMul::Smulh,
            dst: g(2),
            lhs: g(0),
            rhs: g(1),
        }],
        MTerm::Ret { values: vec![g(2)] },
        &[int(1_u64 << 40), int(1_u64 << 40)],
    );
    assert_eq!(bits(high), 1 << 16);
}

#[test]
fn a_function_returns_up_to_eight_results() {
    let values = results(
        vec![w(0), g(1)],
        vec![MInst::MovImm { dst: w(2), imm: 7 }],
        MTerm::Ret {
            values: vec![w(0), g(1), w(2)],
        },
        &[int(4), int(9)],
    );
    assert_eq!(
        values,
        vec![MirValue::Int(4), MirValue::Int(9), MirValue::Int(7)]
    );
}

#[test]
fn a_call_binds_every_result_it_declares() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![
            MFunction {
                name: "pair".to_owned(),
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
                    term: MTerm::Ret {
                        values: vec![w(1), w(0)],
                    },
                }],
            },
            MFunction {
                name: "main".to_owned(),
                blocks: vec![MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: vec![MInst::Bl {
                        returns_twice: false,
                        fp_mode: None,
                        symbol: "pair".to_owned(),
                        args: vec![w(0)],
                        results: vec![w(1), w(2)],
                    }],
                    term: MTerm::Ret {
                        values: vec![w(1), w(2)],
                    },
                }],
            },
        ],
    };
    let outcome = interpret_module(&module, "main", &[int(5)], MirLimits::new(1000))
        .expect("run")
        .results;
    assert_eq!(outcome, vec![MirValue::Int(10), MirValue::Int(5)]);
}

#[test]
fn mem_copy_transfers_bytes_and_leaves_the_source_alone() {
    // Section 4: mem.copy "copies size bytes from src to dst, transferring
    // per-byte initializedness and pointer fragments exactly as EvoIR Section
    // 3.8's copy does".
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "main".to_owned(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![g(0)],
                insts: vec![
                    MInst::FrameSlot {
                        dst: g(1),
                        origin: 0,
                        size: 8,
                        align: 8,
                    },
                    MInst::FrameSlot {
                        dst: g(2),
                        origin: 1,
                        size: 8,
                        align: 8,
                    },
                    MInst::Store {
                        align: 0,
                        src: g(0),
                        base: g(1),
                        offset: 0,
                        kind: evocc_mir::StoreKind::X,
                        volatile: false,
                        mask: None,
                    },
                    MInst::MemCopy {
                        dst: g(2),
                        src: g(1),
                        size: 8,
                        align: 8,
                        volatile: evocc_memory::CopyVolatility::None,
                    },
                    MInst::Load {
                        align: 0,
                        dst: g(3),
                        base: g(2),
                        offset: 0,
                        kind: evocc_mir::LoadKind::X,
                        volatile: false,
                        mask: None,
                    },
                ],
                term: MTerm::Ret { values: vec![g(3)] },
            }],
        }],
    };
    for (mode, count) in [
        (evocc_memory::CopyVolatility::None, 0),
        (evocc_memory::CopyVolatility::Read, 1),
        (evocc_memory::CopyVolatility::ReadWrite, 2),
    ] {
        let mut variant = module.clone();
        let MInst::MemCopy { volatile, .. } = &mut variant.functions[0].blocks[0].insts[3] else {
            panic!("copy")
        };
        *volatile = mode;
        let value =
            interpret_module(&variant, "main", &[int(0x1234)], MirLimits::new(1000)).expect("run");
        assert_eq!(value.results, vec![MirValue::Int(0x1234)]);
        assert_eq!(value.world.volatile_trace().len(), count);
    }
}

#[test]
fn mem_copy_of_an_uninitialized_source_moves_representation_not_values() {
    // Section 4: "the loads of the expansion never report an uninitialized
    // read, because they move representation, not values." A copy of bytes
    // nothing has written is therefore defined, and only a later scalar load
    // of one reports E3001.
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "main".to_owned(),
            blocks: vec![MBlock {
                id: 0,
                args: Vec::new(),
                insts: vec![
                    MInst::FrameSlot {
                        dst: g(1),
                        origin: 0,
                        size: 8,
                        align: 8,
                    },
                    MInst::FrameSlot {
                        dst: g(2),
                        origin: 1,
                        size: 8,
                        align: 8,
                    },
                    MInst::MemCopy {
                        dst: g(2),
                        src: g(1),
                        size: 8,
                        align: 8,
                        volatile: evocc_memory::CopyVolatility::None,
                    },
                ],
                term: MTerm::Ret { values: Vec::new() },
            }],
        }],
    };
    assert!(interpret_module(&module, "main", &[], MirLimits::new(1000)).is_ok());
}

// ---------------------------------------------------------------------------
// The three rows of Section 5 that are written for a value filling its
// register. Each demonstration is the machine sequence the row states, run on
// the input that separates it from what EvoIR answers.
// ---------------------------------------------------------------------------

#[test]
fn the_overflow_flag_reports_the_register_not_the_source_width() {
    // Section 5: "`signed_add_defined %a, %b` | `adds`/`subs` into a scratch,
    // then `cset vc`". Two three-bit threes sum to six, which is outside the
    // range of bits<3>, so EvoIR answers 0; the sign-extended operands do not
    // overflow a 32-bit register, so the row answers 1.
    let literal = run(
        vec![w(0), w(1)],
        vec![
            MInst::Bfx {
                signed: true,
                dst: w(2),
                src: w(0),
                lsb: 0,
                bits: 3,
                width: Width::W,
            },
            MInst::Bfx {
                signed: true,
                dst: w(3),
                src: w(1),
                lsb: 0,
                bits: 3,
                width: Width::W,
            },
            MInst::FlagBinary {
                op: evocc_mir::FlagOp::Adds,
                dst: w(4),
                flags: f(5),
                lhs: w(2),
                rhs: w(3),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(6),
                flags: f(5),
                cond: Cond::Vc,
            },
        ],
        MTerm::Ret { values: vec![w(6)] },
        &[int(3), int(3)],
    );
    assert_eq!(bits(literal), 1, "the register did not overflow");

    // The bitfield comparison the multiply row already states, read at three
    // bits instead of thirty-two, answers what EvoIR answers.
    let closed = run(
        vec![w(0), w(1)],
        vec![
            MInst::Bfx {
                signed: true,
                dst: w(2),
                src: w(0),
                lsb: 0,
                bits: 3,
                width: Width::W,
            },
            MInst::Bfx {
                signed: true,
                dst: w(3),
                src: w(1),
                lsb: 0,
                bits: 3,
                width: Width::W,
            },
            MInst::Binary {
                op: BinOp::Add,
                dst: w(4),
                lhs: w(2),
                rhs: w(3),
                width: Width::W,
            },
            MInst::Bfx {
                signed: true,
                dst: w(7),
                src: w(4),
                lsb: 0,
                bits: 3,
                width: Width::W,
            },
            MInst::Cmp {
                dst: f(5),
                lhs: w(4),
                rhs: w(7),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(6),
                flags: f(5),
                cond: Cond::Eq,
            },
        ],
        MTerm::Ret { values: vec![w(6)] },
        &[int(3), int(3)],
    );
    assert_eq!(bits(closed), 0, "six does not fit in three signed bits");
}

#[test]
fn the_left_shift_round_trip_admits_a_negative_operand() {
    // Section 5: "`signed_shl_defined.<s> %x, %amt` | `shift_count_defined` as
    // above, and `(x << amt) >> amt == x` with an arithmetic right shift".
    // EvoIR Section 3.3.1 is also "false for ... a negative left operand".
    // Minus one shifted by zero returns minus one, so the round trip holds and
    // the row answers 1 where EvoIR answers 0.
    let round_trip = run(
        vec![w(0), w(1)],
        vec![
            MInst::Binary {
                op: BinOp::Lsl,
                dst: w(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Binary {
                op: BinOp::Asr,
                dst: w(3),
                lhs: w(2),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Cmp {
                dst: f(4),
                lhs: w(3),
                rhs: w(0),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(5),
                flags: f(4),
                cond: Cond::Eq,
            },
        ],
        MTerm::Ret { values: vec![w(5)] },
        &[int(0xffff_ffff), int(0)],
    );
    assert_eq!(bits(round_trip), 1, "minus one shifted by zero round-trips");

    // The condition the row omits.
    let non_negative = run(
        vec![w(0)],
        vec![
            MInst::MovImm { dst: w(1), imm: 0 },
            MInst::Cmp {
                dst: f(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(3),
                flags: f(2),
                cond: Cond::Ge,
            },
        ],
        MTerm::Ret { values: vec![w(3)] },
        &[int(0xffff_ffff)],
    );
    assert_eq!(bits(non_negative), 0, "minus one is a negative operand");
}

#[test]
fn a_zero_extended_signed_shift_amount_loses_its_sign() {
    // Section 5: "`shift_count_defined.<s> %x, %amt` | zero-extend `%amt` per
    // Section 3, `cmp` with `#N`, `cset lo`". A one-bit signed amount of 1
    // decodes as minus one, which EvoIR puts outside `[0, N)`; zero-extended
    // it is 1 and compares below a bound of 3.
    let zero_extended = run(
        vec![w(0)],
        vec![
            MInst::BinaryImm {
                op: BinOp::And,
                dst: w(1),
                lhs: w(0),
                imm: 1,
                width: Width::W,
            },
            MInst::CmpImm {
                dst: f(2),
                lhs: w(1),
                imm: 3,
                width: Width::W,
            },
            MInst::Cset {
                dst: w(3),
                flags: f(2),
                cond: Cond::Lo,
            },
        ],
        MTerm::Ret { values: vec![w(3)] },
        &[int(1)],
    );
    assert_eq!(bits(zero_extended), 1, "one is below three");

    // Extending by the declared signedness puts every negative amount above
    // the bound when the comparison is unsigned.
    let sign_extended = run(
        vec![w(0)],
        vec![
            MInst::Bfx {
                signed: true,
                dst: w(1),
                src: w(0),
                lsb: 0,
                bits: 1,
                width: Width::W,
            },
            MInst::CmpImm {
                dst: f(2),
                lhs: w(1),
                imm: 3,
                width: Width::W,
            },
            MInst::Cset {
                dst: w(3),
                flags: f(2),
                cond: Cond::Lo,
            },
        ],
        MTerm::Ret { values: vec![w(3)] },
        &[int(1)],
    );
    assert_eq!(bits(sign_extended), 0, "minus one is not a defined count");
}
