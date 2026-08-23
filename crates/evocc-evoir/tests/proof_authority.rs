//! What the scalar equality proofs actually establish.
//!
//! A replay is only a soundness argument if the rules it replays against come
//! from an authority the producer could not choose. These tests pin which of
//! the two situations the optimizer is currently in, so that the number a
//! receipt reports is bound to a measured property rather than to an adjective
//! someone wrote by hand.

use evocc_evoir::{
    optimize_scalar_module, Block, BlockArg, Function, FunctionParam, Inst, Module, ProofAuthority,
    ScalarEqBudget, Value,
};
use evocc_proof::ProofContext;
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
        globals: Vec::new(),
        functions: vec![Function {
            name: "main".to_owned(),
            params: vec![FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(32),
            }],
            ret_tys: vec![EvoIRType::Bits(32)],
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
        }],
    }
}

/// The replay succeeds against the certificate's own rule list and fails against
/// an authority that does not contain those rules. The gap between the two is
/// exactly the extent to which the producer is authorizing itself: a check that
/// only passes when the subject supplies the standard is a consistency check,
/// not a proof.
#[test]
fn replaying_against_the_certificates_own_rules_is_not_an_external_check() {
    let outcome = optimize_scalar_module(&identity_module(), ScalarEqBudget::new(512, 16));
    assert!(
        !outcome.report.receipts.is_empty(),
        "the fixture must produce at least one rewrite to reason about"
    );

    for receipt in &outcome.report.receipts {
        let own = ProofContext::new(receipt.certificate.rules.clone())
            .expect("a certificate's own rules are structurally well formed");
        assert!(
            own.replay(&receipt.certificate).is_ok(),
            "replay against self-supplied rules always succeeds"
        );

        let empty = ProofContext::new(Vec::new()).expect("an empty authority is well formed");
        assert!(
            empty.replay(&receipt.certificate).is_err(),
            "the same certificate is refused by an authority that did not grant its rules, \
             which is what makes the self-supplied case vacuous"
        );
    }
}

/// The report states which authority it verified against, so a receipt can quote
/// a measured field instead of asserting independence in prose. Until a reviewed
/// registry exists that can express the constant side conditions of the P0
/// rules, the honest answer is that the producer supplied its own.
#[test]
fn the_report_states_which_authority_it_verified_against() {
    let outcome = optimize_scalar_module(&identity_module(), ScalarEqBudget::new(512, 16));

    assert_eq!(
        outcome.report.proof_authority,
        ProofAuthority::ProducerSupplied,
        "the optimizer manufactures one exact rule per candidate and replays against it, \
         so it cannot yet claim an external authority"
    );
}
