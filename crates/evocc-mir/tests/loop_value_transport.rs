use evocc_mir::{
    out_of_ssa, AsmBody, AsmClobber, AsmConstraint, AsmDirection, AsmOperand, BinOp, Cond, MBlock,
    MFunction, MInst, MModule, MTerm, MirLimits, MirValue, RegClass, VReg, Width,
};

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

fn loop_sum(exchange: bool) -> MFunction {
    MFunction {
        name: "sum".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![w(0), w(1), w(2)],
                insts: vec![MInst::MovImm { dst: w(3), imm: 0 }],
                term: MTerm::B {
                    target: 1,
                    args: vec![w(0), w(1), w(2), w(3), w(3)],
                },
            },
            MBlock {
                id: 1,
                args: (4..9).map(w).collect(),
                insts: vec![MInst::Cmp {
                    width: Width::W,
                    dst: VReg::new(9, RegClass::Flags),
                    lhs: w(7),
                    rhs: w(6),
                }],
                term: MTerm::BCond {
                    cond: Cond::Lo,
                    flags: VReg::new(9, RegClass::Flags),
                    taken: 2,
                    taken_args: (4..9).map(w).collect(),
                    untaken: 3,
                    untaken_args: vec![w(8)],
                },
            },
            MBlock {
                id: 2,
                args: (10..15).map(w).collect(),
                insts: vec![
                    MInst::Binary {
                        op: BinOp::Add,
                        width: Width::W,
                        dst: w(15),
                        lhs: w(14),
                        rhs: w(10),
                    },
                    MInst::BinaryImm {
                        op: BinOp::Add,
                        width: Width::W,
                        dst: w(16),
                        lhs: w(13),
                        imm: 1,
                    },
                ],
                term: MTerm::B {
                    target: 1,
                    args: if exchange {
                        vec![w(11), w(10), w(12), w(16), w(15)]
                    } else {
                        vec![w(10), w(11), w(12), w(16), w(15)]
                    },
                },
            },
            MBlock {
                id: 3,
                args: vec![w(17)],
                insts: vec![],
                term: MTerm::Ret {
                    values: vec![w(17)],
                },
            },
        ],
    }
}

fn module(function: MFunction) -> MModule {
    MModule {
        globals: vec![],
        functions: vec![function],
    }
}

#[test]
fn invariant_loop_arguments_share_their_one_reaching_value() {
    let before = loop_sum(false);
    evocc_mir::validate_module(&module(before.clone())).unwrap();
    let after = out_of_ssa(&before);
    let stable = [4, 5, 6, 10, 11, 12];
    assert!(
        after
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .all(|i| { !matches!(i, MInst::Mov { dst, .. } if stable.contains(&dst.id)) }),
        "unchanged loop arguments still acquire fresh runtime copies"
    );
    assert_eq!(after.blocks[0].args, before.blocks[0].args);
    let allocation = evocc_mir::allocate(&after).unwrap();
    evocc_mir::verify_allocation(&allocation).unwrap();
}

#[test]
fn ordinary_assembly_retains_effects_but_not_unrelated_identity_copies() {
    let mut before = loop_sum(false);
    let assembly = AsmBody {
        template: "add %w0, %w1, %w2".into(),
        volatile: true,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::EarlyClobber(AsmDirection::Write),
            w(18),
        )],
        inputs: vec![
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(10)),
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(11)),
        ],
        clobbers: vec![
            AsmClobber::Register(9),
            AsmClobber::Flags,
            AsmClobber::Memory,
        ],
    };
    before.blocks[2]
        .insts
        .insert(0, MInst::Asm(assembly.clone()));
    evocc_mir::validate_module(&module(before.clone())).unwrap();
    let after = out_of_ssa(&before);
    let stable = [4, 5, 6, 10, 11, 12];
    assert!(
        after
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .all(|i| !matches!(i, MInst::Mov { dst, .. } if stable.contains(&dst.id))),
        "ordinary assembly unnecessarily blocks direct-edge identity forwarding"
    );
    let retained: Vec<_> = after
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter_map(|i| if let MInst::Asm(a) = i { Some(a) } else { None })
        .collect();
    assert_eq!(retained.len(), 1);
    let mut expected = assembly;
    expected.inputs[0].value = evocc_mir::AsmValue::Reg(w(0));
    expected.inputs[1].value = evocc_mir::AsmValue::Reg(w(1));
    assert_eq!(retained[0], &expected);
    evocc_mir::verify_allocation(&evocc_mir::allocate(&after).unwrap()).unwrap();
}

#[test]
fn assembly_fixed_locations_do_not_merge_even_for_one_incoming_value() {
    let input = |id, number| {
        let mut operand = AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(id));
        operand.fixed_register = Some(number);
        operand
    };
    let function = MFunction {
        name: "fixed".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![w(0)],
                insts: vec![],
                term: MTerm::B {
                    target: 1,
                    args: vec![w(0), w(0)],
                },
            },
            MBlock {
                id: 1,
                args: vec![w(1), w(2)],
                insts: vec![MInst::Asm(AsmBody {
                    template: "add %w0, %w1, %w2".into(),
                    volatile: true,
                    outputs: vec![AsmOperand::reg(
                        AsmConstraint::EarlyClobber(AsmDirection::Write),
                        w(3),
                    )],
                    inputs: vec![input(1, 19), input(2, 20)],
                    clobbers: vec![],
                })],
                term: MTerm::Ret { values: vec![w(3)] },
            },
        ],
    };
    evocc_mir::validate_module(&module(function.clone())).unwrap();
    let after = out_of_ssa(&function);
    let copies = after
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter(|i| matches!(i, MInst::Mov { .. }))
        .count();
    assert_eq!(copies, 2);
    evocc_mir::verify_allocation(&evocc_mir::allocate(&after).unwrap()).unwrap();
}

#[test]
fn identical_value_may_supply_two_distinct_fixed_assembly_inputs() {
    for second in [19, 20] {
        let input = |number| {
            let mut operand = AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(0));
            operand.fixed_register = Some(number);
            operand
        };
        let function = MFunction {
            name: "fixed_inputs".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![w(0)],
                insts: vec![MInst::Asm(AsmBody {
                    template: "add %w0, %w1, %w2".into(),
                    volatile: true,
                    outputs: vec![AsmOperand::reg(
                        AsmConstraint::EarlyClobber(AsmDirection::Write),
                        w(1),
                    )],
                    inputs: vec![input(19), input(second)],
                    clobbers: vec![],
                })],
                term: MTerm::Ret { values: vec![w(1)] },
            }],
        };
        evocc_mir::validate_module(&module(function.clone())).unwrap();
        let allocated = evocc_mir::allocate(&out_of_ssa(&function)).unwrap();
        evocc_mir::verify_allocation(&allocated).unwrap();
        let body = allocated
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .find_map(|i| if let MInst::Asm(a) = i { Some(a) } else { None })
            .unwrap();
        assert_eq!(
            allocated.assignment[&body.inputs[0].register().unwrap().id],
            19
        );
        assert_eq!(
            allocated.assignment[&body.inputs[1].register().unwrap().id],
            second
        );
    }
}

#[test]
fn contradictory_fixed_tied_operands_remain_rejected() {
    let mut output = AsmOperand::reg(AsmConstraint::Register(AsmDirection::Write), w(1));
    output.fixed_register = Some(19);
    let mut tied = AsmOperand::reg(AsmConstraint::Tied(0), w(1));
    tied.fixed_register = Some(20);
    let function = MFunction {
        name: "bad_tie".into(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![w(0)],
            insts: vec![
                MInst::Mov {
                    dst: w(1),
                    src: w(0),
                },
                MInst::Asm(AsmBody {
                    template: "add %w0, %w1, #1".into(),
                    volatile: true,
                    outputs: vec![output],
                    inputs: vec![tied],
                    clobbers: vec![],
                }),
            ],
            term: MTerm::Ret { values: vec![w(1)] },
        }],
    };
    assert!(evocc_mir::allocate(&function)
        .unwrap_err()
        .contains("conflicting tied"));
}

#[test]
fn distinct_fixed_inputs_preserve_values_under_real_spilling() {
    for class in [
        RegClass::Gpr32,
        RegClass::Gpr64,
        RegClass::Simd64,
        RegClass::Simd128,
    ] {
        let r = |id| VReg::new(id, class);
        let constraint = |direction| {
            if class.is_simd() {
                AsmConstraint::Simd(direction)
            } else {
                AsmConstraint::Register(direction)
            }
        };
        let base: u8 = if class.is_simd() { 64 } else { 0 };
        let input = |number: u8| {
            let mut operand = AsmOperand::reg(constraint(AsmDirection::Read), r(0));
            operand.fixed_register = Some(base + number);
            operand
        };
        let function = MFunction {
            name: "pressure_fixed".into(),
            blocks: vec![MBlock {
                id: 0,
                args: (0..32).map(r).collect(),
                insts: vec![
                    MInst::Asm(AsmBody {
                        template: "// %0 %1 %2".into(),
                        volatile: true,
                        outputs: vec![AsmOperand::reg(constraint(AsmDirection::Write), r(32))],
                        inputs: vec![input(19), input(20)],
                        clobbers: vec![],
                    }),
                    MInst::Bl {
                        fp_mode: None,
                        symbol: "consume".into(),
                        args: (0..33).map(r).collect(),
                        results: vec![],
                    },
                ],
                term: MTerm::Ret {
                    values: vec![r(32)],
                },
            }],
        };
        evocc_mir::validate_module(&module(function.clone())).unwrap();
        let allocated =
            evocc_mir::allocate_for(evocc_mir::Target::Linux, &out_of_ssa(&function)).unwrap();
        assert!(allocated.spilled > 0);
        evocc_mir::verify_allocation(&allocated).unwrap();
    }
}

#[test]
fn exchanging_values_and_counters_are_not_invariant() {
    for exchange in [false, true] {
        let before = loop_sum(exchange);
        let after = out_of_ssa(&before);
        let allocation = evocc_mir::allocate(&after).unwrap();
        evocc_mir::verify_allocation(&allocation).unwrap();
        for a in [0, 2, u32::MAX] {
            for b in [1, 9, u32::MAX - 1] {
                for n in 0..8 {
                    let args = [a, b, n].map(|v| MirValue::Int(u64::from(v)));
                    let expected = (0..n).fold(0_u32, |s, i| {
                        s.wrapping_add(if exchange && i % 2 != 0 { b } else { a })
                    });
                    let result = evocc_mir::interpret_module(
                        &module(before.clone()),
                        "sum",
                        &args,
                        MirLimits::new(10_000),
                    )
                    .unwrap();
                    assert_eq!(result.results, [MirValue::Int(u64::from(expected))]);
                }
            }
        }
    }
}

#[test]
fn different_bindings_to_the_same_target_do_not_collapse() {
    let function = MFunction {
        name: "choice".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![w(0), w(1), w(2)],
                insts: vec![],
                term: MTerm::Cb {
                    nonzero: true,
                    src: w(0),
                    taken: 1,
                    taken_args: vec![w(1)],
                    untaken: 1,
                    untaken_args: vec![w(2)],
                },
            },
            MBlock {
                id: 1,
                args: vec![w(3)],
                insts: vec![],
                term: MTerm::Ret { values: vec![w(3)] },
            },
        ],
    };
    evocc_mir::validate_module(&module(function.clone())).unwrap();
    let after = out_of_ssa(&function);
    let sources: Vec<_> = after
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter_map(|i| {
            if let MInst::Mov { dst, src } = i {
                (dst.id == 3).then_some(src.id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(sources, [1, 2]);
    evocc_mir::verify_allocation(&evocc_mir::allocate(&after).unwrap()).unwrap();
}

#[test]
fn forwarding_is_bit_exact_for_every_register_class_and_still_spills() {
    for class in [
        RegClass::Gpr32,
        RegClass::Gpr64,
        RegClass::Simd32,
        RegClass::Simd64,
        RegClass::Simd128,
    ] {
        let r = |id| VReg::new(id, class);
        let function = MFunction {
            name: "pressure".into(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: (0..32).map(r).collect(),
                    insts: vec![],
                    term: MTerm::B {
                        target: 1,
                        args: (0..32).map(r).collect(),
                    },
                },
                MBlock {
                    id: 1,
                    args: (32..64).map(r).collect(),
                    insts: vec![MInst::Bl {
                        fp_mode: None,
                        symbol: "consume".into(),
                        args: (32..64).map(r).collect(),
                        results: vec![],
                    }],
                    term: MTerm::Ret {
                        values: vec![r(32)],
                    },
                },
            ],
        };
        evocc_mir::validate_module(&module(function.clone())).unwrap();
        let after = out_of_ssa(&function);
        assert!(after
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .all(|i| !matches!(i, MInst::Mov { .. })));
        let allocation = evocc_mir::allocate_for(evocc_mir::Target::Linux, &after).unwrap();
        assert!(
            allocation.spilled > 0,
            "{class:?} must exercise real spills"
        );
        evocc_mir::verify_allocation(&allocation).unwrap();
    }
}
