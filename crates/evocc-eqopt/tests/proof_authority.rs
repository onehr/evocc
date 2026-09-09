//! The reviewed authority recorded by the Day 8 EVP1 producer.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ProofAuthority, ScalarEqBudget};
use evocc_evoir::{Block, BlockArg, Function, FunctionParam, Inst, Module, Value};
use evocc_proof::{parse_evp1, render_evp1};
use evocc_types::EvoIRType;

fn bits(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(32),
    }
}

/// `f(x) = (x + 0) * 1`, whose two identities are P0 rewrite opportunities.
fn identity_module() -> Module {
    Module {
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            alignment: 4,
            linkage: evocc_evoir::Linkage::Strong,
            visibility: evocc_evoir::GlobalVisibility::External,
            variadic: false,
            name: "main".to_owned(),
            params: vec![FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(32),
            }],
            ret_tys: vec![EvoIRType::Bits(32)],
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
                ],
                terminator: Inst::Ret(vec![bits(4)]),
            }],
            param_marks: Vec::new(),
        }],
    }
}

#[test]
fn reviewed_certificate_names_a_digest_instead_of_supplying_rules() {
    let outcome =
        optimize_scalar_module_with_evp1(&identity_module(), ScalarEqBudget::new(512, 16));
    assert!(!outcome.certificate.regions.is_empty());
    assert_eq!(outcome.certificate.registry_name, "p0.scalar.v1");
    assert_eq!(
        outcome.certificate.registry_digest,
        "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9"
    );
    let bytes = render_evp1(&outcome.certificate);
    assert_eq!(parse_evp1(bytes.as_bytes()).unwrap(), outcome.certificate);
}

#[test]
fn the_report_states_which_authority_it_verified_against() {
    let outcome =
        optimize_scalar_module_with_evp1(&identity_module(), ScalarEqBudget::new(512, 16));

    assert_eq!(
        outcome.report.proof_authority,
        ProofAuthority::Reviewed {
            registry: "p0.scalar.v1"
        }
    );
}
