use evocc_eqopt::{optimize_scalar_module_with_evp1, ProofAuthority, ScalarEqBudget};
use evocc_evoir::{
    interpret_module, render_module, validate_module, Block, BlockArg, CmpOp, Function,
    FunctionParam, Inst, InterpreterLimits, Module, RuntimeValue, Value,
};
use evocc_proof::{parse_evp1, render_evp1, sha256_hex};
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

fn input() -> Module {
    Module {
        globals: Vec::new(),
        functions: vec![Function {
            name: "main".to_owned(),
            params: vec![FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(32),
            }],
            ret_tys: vec![
                EvoIRType::Bits(32),
                EvoIRType::Bits(32),
                EvoIRType::Bits(32),
                EvoIRType::Bits(32),
                EvoIRType::Bits(32),
                EvoIRType::Bits(1),
                EvoIRType::Bits(32),
            ],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![BlockArg {
                    name: "x".to_owned(),
                    value: bits(0),
                }],
                insts: vec![
                    Inst::Const(bits(1), 0),
                    Inst::Add(bits(2), bits(0), bits(1)),
                    Inst::Const(bits(3), 1),
                    Inst::Mul(bits(4), bits(2), bits(3)),
                    Inst::And(bits(5), bits(4), bits(1)),
                    Inst::Or(bits(6), bits(4), bits(1)),
                    Inst::Sub(bits(7), bits(1), bits(0)),
                    Inst::Sub(bits(8), bits(1), bits(7)),
                    Inst::ICmp(bit(9), CmpOp::Eq, bits(0), bits(0)),
                    Inst::Const(bits(10), 2),
                    Inst::Const(bits(11), 3),
                    Inst::Add(bits(12), bits(10), bits(11)),
                ],
                terminator: Inst::Ret(vec![
                    bits(2),
                    bits(4),
                    bits(5),
                    bits(6),
                    bits(8),
                    bit(9),
                    bits(12),
                ]),
            }],
        }],
    }
}

fn execute(module: &Module, input: u64) -> Vec<RuntimeValue> {
    interpret_module(
        module,
        "main",
        &[RuntimeValue::new(EvoIRType::Bits(32), input)],
        InterpreterLimits::new(100_000),
    )
    .expect("defined P0 module executes")
    .return_values
}

#[test]
fn producer_emits_canonical_reviewed_multi_region_evp1() {
    let before = input();
    validate_module(&before).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(100_000, 1_024));
    validate_module(&outcome.module).unwrap();
    assert_eq!(execute(&outcome.module, 19), execute(&before, 19));
    assert_eq!(outcome.certificate.regions.len(), 7);
    assert_eq!(outcome.report.merges, 7);
    assert!(matches!(
        outcome.report.proof_authority,
        ProofAuthority::Reviewed {
            registry: "p0.scalar.v1"
        }
    ));
    let certificate_bytes = render_evp1(&outcome.certificate);
    assert_eq!(
        parse_evp1(certificate_bytes.as_bytes()).unwrap(),
        outcome.certificate
    );
    assert_eq!(
        outcome.certificate.input_digest,
        sha256_hex(render_module(&before).as_bytes())
    );
    assert_eq!(
        outcome.certificate.output_digest,
        sha256_hex(render_module(&outcome.module).as_bytes())
    );
}

#[test]
fn a_budget_reached_before_anything_was_admitted_yields_no_region() {
    let before = input();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(0, 0));
    assert_eq!(outcome.module, before);
    assert!(outcome.report.budget_exhausted);
    // Section 11.3, revision 1.9.38: a ceiling stop keeps every region admitted
    // before it. This budget stops before the first one, so there is nothing to
    // keep and the certificate claims no region -- which is a different thing
    // from falling back and discarding what had been admitted.
    assert!(!outcome.report.fell_back);
    assert!(outcome.certificate.regions.is_empty());
    assert_eq!(
        outcome.certificate.input_digest,
        outcome.certificate.output_digest
    );
}
