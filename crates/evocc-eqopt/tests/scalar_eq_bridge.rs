use evocc_eqopt::{ingest_scalar_inst, optimize_scalar_module, ScalarEqBudget, ScalarOperator};
use evocc_evoir::{
    interpret_module, validate_module, Block, BlockArg, CmpOp, Function, FunctionParam, Inst,
    InterpreterLimits, Module, RuntimeValue, TrapReason, Value,
};
use evocc_proof::ProofContext;
use evocc_types::EvoIRType;

fn bits(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(32),
    }
}

fn bit(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(1),
    }
}

fn module(insts: Vec<Inst>, returns: Vec<Value>, return_types: Vec<EvoIRType>) -> Module {
    Module {
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            visibility: evocc_evoir::GlobalVisibility::External,
            variadic: false,
            name: "main".to_owned(),
            params: vec![FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(32),
            }],
            ret_tys: return_types,
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![BlockArg {
                    name: "x".to_owned(),
                    value: bits(0),
                }],
                insts,
                terminator: Inst::Ret(returns),
            }],
            param_marks: Vec::new(),
        }],
    }
}

fn execute(module: &Module, x: u64) -> Vec<RuntimeValue> {
    interpret_module(
        module,
        "main",
        &[RuntimeValue::new(EvoIRType::Bits(32), x)],
        InterpreterLimits::new(100_000),
    )
    .expect("module executes")
    .return_values
}

#[test]
fn pure_scalar_region_round_trips_through_a_real_egraph_without_drift() {
    let input = module(
        vec![Inst::Add(bits(1), bits(0), bits(0))],
        vec![bits(1)],
        vec![EvoIRType::Bits(32)],
    );
    let outcome = optimize_scalar_module(&input, ScalarEqBudget::new(64, 8));
    assert_eq!(outcome.module, input);
    assert_eq!(outcome.report.regions_admitted, 1);
    // `add(?x, ?x)` is one of the three spellings of a doubling that
    // `p0.scalar.v4` makes one class, so the class gains the shift and the
    // merge is real. The transient module keeps the instruction it started
    // with: which spelling survives is the extraction policy's choice and is
    // taken where the certificate is written, so no value is optimized here.
    assert_eq!(outcome.report.merges, 1);
    assert_eq!(outcome.report.optimized_values, 0);
    assert!(!outcome.report.fell_back);
    assert_eq!(execute(&outcome.module, 17), execute(&input, 17));
}

#[test]
fn effect_nodes_are_refused_at_the_bridge_boundary() {
    let load = Inst::Load {
        result: bits(4),
        address: Value {
            id: 0,
            ty: EvoIRType::Ptr(evocc_types::AddressSpace::Generic),
        },
        mem_in: Value {
            id: 1,
            ty: EvoIRType::Mem(evocc_types::Region::All),
        },
        world_in: Value {
            id: 2,
            ty: EvoIRType::World,
        },
        mem_out: Value {
            id: 5,
            ty: EvoIRType::Mem(evocc_types::Region::All),
        },
        world_out: Value {
            id: 6,
            ty: EvoIRType::World,
        },
        volatile: false,
    };
    let error = ingest_scalar_inst(&load).expect_err("load must not enter scalar egraph");
    assert_eq!(error.code(), "E4001");
    assert_eq!(
        error.message(),
        "effect-bearing-instruction-is-not-scalar-egraph-eligible"
    );
}

#[test]
fn p0_rewrites_extract_cheaper_evoir_and_every_merge_replays_independently() {
    let input = module(
        vec![
            Inst::Const(bits(1), 0),
            Inst::Const(bits(2), 1),
            Inst::Const(bits(3), 2),
            Inst::Const(bits(4), 3),
            Inst::Add(bits(5), bits(0), bits(1)),
            Inst::Mul(bits(6), bits(5), bits(2)),
            Inst::And(bits(7), bits(6), bits(1)),
            Inst::Or(bits(8), bits(6), bits(1)),
            Inst::Sub(bits(9), bits(1), bits(8)),
            Inst::Sub(bits(10), bits(1), bits(9)),
            Inst::ICmp(bit(11), CmpOp::Eq, bits(10), bits(10)),
            Inst::Add(bits(12), bits(3), bits(4)),
        ],
        vec![bits(6), bits(7), bits(8), bits(10), bit(11), bits(12)],
        vec![
            EvoIRType::Bits(32),
            EvoIRType::Bits(32),
            EvoIRType::Bits(32),
            EvoIRType::Bits(32),
            EvoIRType::Bits(1),
            EvoIRType::Bits(32),
        ],
    );
    validate_module(&input).unwrap();
    let before = execute(&input, 19);
    let outcome = optimize_scalar_module(&input, ScalarEqBudget::new(512, 16));
    validate_module(&outcome.module).unwrap();
    assert_eq!(execute(&outcome.module, 19), before);
    assert!(outcome.report.optimized_values >= 7, "{:?}", outcome.report);
    assert_eq!(outcome.report.receipts.len(), outcome.report.merges);
    assert!(!outcome.report.receipts.is_empty());
    for receipt in &outcome.report.receipts {
        let context = ProofContext::new(receipt.certificate.rules.clone()).unwrap();
        let replay = context.replay(&receipt.certificate).unwrap();
        assert_eq!(replay.rewrite_unions_replayed, 1);
    }
    let text = format!("{:?}", outcome.module.functions[0].blocks[0].insts);
    assert!(!text.contains("Add(Value { id: 5"), "{text}");
    assert!(!text.contains("Mul(Value { id: 6"), "{text}");
    assert!(
        text.contains("Const(Value { id: 7") && text.contains(", 0)"),
        "{text}"
    );
    assert!(
        text.contains("Const(Value { id: 11") && text.contains(", 1)"),
        "{text}"
    );
    assert!(
        text.contains("Const(Value { id: 12") && text.contains(", 5)"),
        "{text}"
    );
}

#[test]
fn a_budget_reached_before_the_first_rewrite_changes_nothing() {
    let input = module(
        vec![
            Inst::Const(bits(1), 0),
            Inst::Add(bits(2), bits(0), bits(1)),
        ],
        vec![bits(2)],
        vec![EvoIRType::Bits(32)],
    );
    let outcome = optimize_scalar_module(&input, ScalarEqBudget::new(1, 1));
    assert_eq!(outcome.module, input);
    assert!(outcome.report.budget_exhausted);
    // The stop lands before the only candidate, so nothing was admitted and
    // nothing is discarded. Section 8.11 makes a ceiling a successful stop that
    // keeps what it had; falling back would throw away work a checker would
    // have verified, so the sweep no longer reports one.
    assert!(!outcome.report.fell_back);
    assert_eq!(outcome.report.merges, 0);
    assert!(outcome.report.receipts.is_empty());
}

#[test]
fn operator_table_is_explicit_for_every_day7_p0_instruction() {
    assert_eq!(
        ingest_scalar_inst(&Inst::Const(bits(1), 9)).unwrap(),
        ScalarOperator::Const
    );
    assert_eq!(
        ingest_scalar_inst(&Inst::Add(bits(2), bits(0), bits(1))).unwrap(),
        ScalarOperator::Add
    );
    assert_eq!(
        ingest_scalar_inst(&Inst::Sub(bits(2), bits(0), bits(1))).unwrap(),
        ScalarOperator::Sub
    );
    assert_eq!(
        ingest_scalar_inst(&Inst::Mul(bits(2), bits(0), bits(1))).unwrap(),
        ScalarOperator::Mul
    );
    assert_eq!(
        ingest_scalar_inst(&Inst::And(bits(2), bits(0), bits(1))).unwrap(),
        ScalarOperator::And
    );
    assert_eq!(
        ingest_scalar_inst(&Inst::Or(bits(2), bits(0), bits(1))).unwrap(),
        ScalarOperator::Or
    );
    assert_eq!(
        ingest_scalar_inst(&Inst::ICmp(bit(2), CmpOp::Eq, bits(0), bits(1))).unwrap(),
        ScalarOperator::ICmp(CmpOp::Eq)
    );
}

#[test]
fn a_trapping_successor_never_licenses_predecessor_edge_strengthening() {
    let input = Module {
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            visibility: evocc_evoir::GlobalVisibility::External,
            variadic: false,
            name: "branch_to_report".to_owned(),
            params: vec![
                FunctionParam {
                    name: "value".to_owned(),
                    ty: EvoIRType::Bits(32),
                },
                FunctionParam {
                    name: "take_report".to_owned(),
                    ty: EvoIRType::Bits(1),
                },
            ],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: evocc_evoir::EffectSummary {
                may_trap: true,
                ..Default::default()
            },
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![
                        BlockArg {
                            name: "value".to_owned(),
                            value: bits(0),
                        },
                        BlockArg {
                            name: "take_report".to_owned(),
                            value: bit(1),
                        },
                    ],
                    insts: vec![
                        Inst::Const(bits(2), 0),
                        Inst::Add(bits(3), bits(0), bits(2)),
                    ],
                    terminator: Inst::CondBr(bit(1), 1, Vec::new(), 2, vec![bits(3)]),
                },
                Block {
                    id: 1,
                    args: Vec::new(),
                    insts: Vec::new(),
                    terminator: Inst::Trap(TrapReason::UnreachableReached),
                },
                Block {
                    id: 2,
                    args: vec![BlockArg {
                        name: "result".to_owned(),
                        value: bits(4),
                    }],
                    insts: Vec::new(),
                    terminator: Inst::Ret(vec![bits(4)]),
                },
            ],
            param_marks: Vec::new(),
        }],
    };
    validate_module(&input).expect("input module is valid");

    let outcome = optimize_scalar_module(&input, ScalarEqBudget::new(128, 8));
    validate_module(&outcome.module).expect("optimized module is valid");
    assert!(outcome.report.optimized_values >= 1, "{:?}", outcome.report);
    assert!(matches!(
        outcome.module.functions[0].blocks[0].terminator,
        Inst::CondBr(_, 1, _, 2, _)
    ));
    assert_eq!(
        outcome.module.functions[0].blocks[1].terminator,
        Inst::Trap(TrapReason::UnreachableReached)
    );
}
