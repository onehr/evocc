//! Section 2: block-argument SSA over virtual registers, and the `flags`
//! adjacency rule.
//!
//! "A `flags` value may be consumed only by the instruction that immediately
//! follows its producer in the same block, and nothing between them may write
//! flags. This is a legality rule the verifier of Section 9 checks, not a
//! hint."

use evocc_mir::{
    validate_module, BinOp, Cond, MBlock, MFunction, MInst, MModule, MTerm, RegClass, VReg, Width,
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

fn one_block(args: Vec<VReg>, insts: Vec<MInst>, term: MTerm) -> MModule {
    MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![MBlock {
                id: 0,
                args,
                insts,
                term,
            }],
        }],
    }
}

#[test]
fn call_flag_clobber_can_precede_a_new_compare() {
    let module = one_block(
        vec![g(0)],
        vec![
            MInst::Bl {
                returns_twice: false,
                fp_mode: None,
                symbol: "touch".into(),
                args: vec![],
                results: vec![],
            },
            MInst::CmpImm {
                dst: f(1),
                lhs: g(0),
                imm: 0,
                width: Width::X,
            },
            MInst::Cset {
                dst: w(2),
                flags: f(1),
                cond: Cond::Eq,
            },
        ],
        MTerm::Ret { values: vec![w(2)] },
    );
    validate_module(&module).unwrap();
}

#[test]
fn assembly_goto_results_exist_on_edges_not_on_input() {
    use evocc_mir::{AsmBody, AsmConstraint, AsmDirection, AsmOperand};
    let body = AsmBody {
        template: "mov %w0, %w1".into(),
        volatile: true,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(1),
        )],
        inputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Read),
            w(0),
        )],
        clobbers: vec![],
    };
    let mut module = one_block(
        vec![w(0)],
        vec![],
        MTerm::AsmGoto {
            body,
            labels: vec![(1, vec![w(1)])],
            fallthrough: 1,
            fallthrough_args: vec![w(1)],
        },
    );
    module.functions[0].blocks.push(MBlock {
        id: 1,
        args: vec![w(2)],
        insts: vec![],
        term: MTerm::Ret { values: vec![w(2)] },
    });
    validate_module(&module).unwrap();
    let mut forged = module.clone();
    assert_eq!(
        evocc_mir::parse_module(&evocc_mir::render_module(&module)).unwrap(),
        module
    );
    let MTerm::AsmGoto { body, .. } = &mut forged.functions[0].blocks[0].term else {
        unreachable!()
    };
    body.inputs[0].value = evocc_mir::AsmValue::Reg(w(1));
    assert!(validate_module(&forged)
        .unwrap_err()
        .contains("without a definition"));
    let MTerm::AsmGoto { body, .. } = &mut module.functions[0].blocks[0].term else {
        unreachable!()
    };
    body.outputs[0].value = evocc_mir::AsmValue::Reg(w(0));
    assert!(validate_module(&module)
        .unwrap_err()
        .contains("defined twice"));
}

#[test]
fn tst_requires_an_encodable_logical_immediate() {
    for imm in [0, 5, u64::MAX] {
        let module = one_block(
            vec![g(0)],
            vec![
                MInst::TstImm {
                    dst: f(1),
                    lhs: g(0),
                    imm,
                    width: Width::X,
                },
                MInst::Cset {
                    dst: w(2),
                    flags: f(1),
                    cond: Cond::Eq,
                },
            ],
            MTerm::Ret { values: vec![w(2)] },
        );
        let error = validate_module(&module).expect_err("unencodable tst must be refused");
        assert!(error.contains("unencodable tst immediate"), "{error}");
    }
}

#[test]
fn indirect_successors_are_nonempty_unique_and_sorted() {
    for edges in [
        vec![],
        vec![(2, vec![]), (1, vec![])],
        vec![(1, vec![]), (1, vec![])],
    ] {
        let module = one_block(
            vec![g(0)],
            vec![],
            MTerm::Indirect {
                target: g(0),
                edges,
            },
        );
        let error = validate_module(&module).expect_err("invalid successor set");
        assert!(error.contains("successors"), "{error}");
    }
}

#[test]
fn indirect_execution_refuses_a_value_that_does_not_name_a_successor() {
    let mut module = one_block(
        vec![g(0)],
        vec![],
        MTerm::Indirect {
            target: g(0),
            edges: vec![(1, vec![])],
        },
    );
    module.functions[0].blocks.push(MBlock {
        id: 1,
        args: vec![],
        insts: vec![],
        term: MTerm::Ret { values: vec![] },
    });
    validate_module(&module).unwrap();
    let error = evocc_mir::interpret_module(
        &module,
        "f",
        &[evocc_mir::MirValue::Int(0)],
        evocc_mir::MirLimits::new(20),
    )
    .unwrap_err();
    assert_eq!(error.code(), "E3048");
}

#[test]
fn native_emission_checks_the_complete_lowered_module() {
    let text = "func @f(%arg0: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>):
    ret %v0
}
";
    let mut module = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    // Individual functions remain valid, but the module has two definitions.
    // The native entry must not publish assembly from this invalid lowering.
    module.functions.push(module.functions[0].clone());
    let error = evocc_mir::compile_module_for(evocc_mir::Target::Linux, &module, &|_| true)
        .expect_err("invalid lowered module");
    assert!(
        matches!(error, evocc_mir::NativeStop::Verification(_)),
        "{error:?}"
    );
    assert!(error.detail().contains("duplicate function symbol"));
}

#[test]
fn a_flags_value_consumed_by_the_next_instruction_is_legal() {
    let module = one_block(
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
    );
    assert_eq!(validate_module(&module), Ok(()));
}

#[test]
fn a_flags_value_consumed_by_the_terminator_after_its_producer_is_legal() {
    let module = one_block(
        vec![w(0), w(1)],
        vec![MInst::Cmp {
            dst: f(2),
            lhs: w(0),
            rhs: w(1),
            width: Width::W,
        }],
        MTerm::BCond {
            flags: f(2),
            cond: Cond::Lo,
            taken: 0,
            taken_args: vec![w(0), w(1)],
            untaken: 0,
            untaken_args: vec![w(0), w(1)],
        },
    );
    assert_eq!(validate_module(&module), Ok(()));
}

#[test]
fn a_flags_value_consumed_two_instructions_later_is_refused() {
    let module = one_block(
        vec![w(0), w(1)],
        vec![
            MInst::Cmp {
                dst: f(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Binary {
                op: BinOp::Add,
                dst: w(4),
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
    );
    assert!(validate_module(&module).is_err());
}

#[test]
fn a_second_flags_writer_between_producer_and_consumer_is_refused() {
    let module = one_block(
        vec![w(0), w(1)],
        vec![
            MInst::Cmp {
                dst: f(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Cmp {
                dst: f(5),
                lhs: w(1),
                rhs: w(0),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(3),
                flags: f(2),
                cond: Cond::Eq,
            },
        ],
        MTerm::Ret { values: vec![w(3)] },
    );
    assert!(validate_module(&module).is_err());
}

#[test]
fn a_flags_value_that_crosses_a_block_edge_is_refused() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: vec![MInst::Cmp {
                        dst: f(1),
                        lhs: w(0),
                        rhs: w(0),
                        width: Width::W,
                    }],
                    term: MTerm::B {
                        target: 1,
                        args: vec![f(1)],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![f(2)],
                    insts: vec![MInst::Cset {
                        dst: w(3),
                        flags: f(2),
                        cond: Cond::Eq,
                    }],
                    term: MTerm::Ret { values: vec![w(3)] },
                },
            ],
        }],
    };
    assert!(validate_module(&module).is_err());
}

#[test]
fn a_virtual_register_defined_twice_is_refused() {
    let module = one_block(
        vec![w(0)],
        vec![
            MInst::MovImm { dst: w(1), imm: 1 },
            MInst::MovImm { dst: w(1), imm: 2 },
        ],
        MTerm::Ret { values: vec![w(1)] },
    );
    assert!(validate_module(&module).is_err());
}

#[test]
fn a_use_without_a_definition_is_refused() {
    let module = one_block(vec![w(0)], Vec::new(), MTerm::Ret { values: vec![w(9)] });
    assert!(validate_module(&module).is_err());
}

#[test]
fn an_edge_whose_arity_does_not_match_the_target_is_refused() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: Vec::new(),
                    term: MTerm::B {
                        target: 1,
                        args: Vec::new(),
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![w(1)],
                    insts: Vec::new(),
                    term: MTerm::Ret { values: vec![w(1)] },
                },
            ],
        }],
    };
    assert!(validate_module(&module).is_err());
}

#[test]
fn an_edge_whose_class_does_not_match_the_target_is_refused() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![g(0)],
                    insts: Vec::new(),
                    term: MTerm::B {
                        target: 1,
                        args: vec![g(0)],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![w(1)],
                    insts: Vec::new(),
                    term: MTerm::Ret { values: vec![w(1)] },
                },
            ],
        }],
    };
    assert!(validate_module(&module).is_err());
}

#[test]
fn an_unreachable_block_is_refused() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: Vec::new(),
                    insts: Vec::new(),
                    term: MTerm::Ret { values: Vec::new() },
                },
                MBlock {
                    id: 1,
                    args: Vec::new(),
                    insts: Vec::new(),
                    term: MTerm::Ret { values: Vec::new() },
                },
            ],
        }],
    };
    assert!(validate_module(&module).is_err());
}
