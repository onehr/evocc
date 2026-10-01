//! Hostile g0 bundles must fail the ordinary independent checker interface.
use evocc_eqopt::{optimize_scalar_module_with_policy, policy_by_name, ExtractionPolicy};
use evocc_evoir::{parse_section9_module, render_module, validate_module, Inst, Module};
use evocc_proof::{render_evp1, sha256_hex, EvpCertificate, EvpEvent};
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn check(before: &Module, after: &Module, proof: &EvpCertificate) -> i32 {
    let dir = std::env::temp_dir().join(format!(
        "evo-guard-proof-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("before.evo"), render_module(before)).unwrap();
    fs::write(dir.join("after.evo"), render_module(after)).unwrap();
    fs::write(dir.join("proof.evp"), render_evp1(proof)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
        .args([
            dir.join("before.evo"),
            dir.join("after.evo"),
            dir.join("proof.evp"),
        ])
        .output()
        .unwrap();
    fs::remove_dir_all(dir).unwrap();
    output.status.code().unwrap_or(-1)
}

fn fixture() -> Module {
    parse_section9_module(include_bytes!(
        "../../../tests/fixtures/eqsat-size/guard-attribution.evo"
    ))
    .unwrap()
}

fn program(n: u16, m: u16, body: &str) -> Module {
    let source = format!("func @main(%arg0: bits<{n}>, %arg1: bits<{m}>, %arg2: mem<all>, %arg3: world) -> (bits<1>, mem<all>, world) cc = target_default\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}\n{{\n^entry(%v0: bits<{n}>, %v1: bits<{m}>, %v2: mem<all>, %v3: world):\n{body}\n    ub.assume.signed_overflow %v6\n    ret %v6, %v2, %v3\n}}\n");
    let result = parse_section9_module(source.as_bytes()).unwrap();
    validate_module(&result).unwrap();
    result
}

fn guard(before: &Module) -> evocc_eqopt::CertifiedScalarEqOutcome {
    optimize_scalar_module_with_policy(before, &policy_by_name("guard-known-true").unwrap())
}

#[test]
fn seven_rule_forms_replay_and_leave_obligations_in_ir() {
    for (n,m,body) in [
        (32,32,"    %v4 = const.bits<32> 2\n    %v6 = divisor_nonzero %v4"),
        (32,32,"    %v4 = const.bits<32> 0\n    %v6 = signed_div_rem_no_overflow %v0, %v4"),
        (32,32,"    %v4 = const.bits<32> 0\n    %v6 = signed_add_defined %v0, %v4"),
        (32,32,"    %v4 = const.bits<32> 0\n    %v6 = signed_sub_defined %v0, %v4"),
        (32,32,"    %v4 = const.bits<32> 1\n    %v6 = signed_mul_defined %v0, %v4"),
        (64,2,"    %v5 = const.bits<2> 3\n    %v6 = shift_count_defined.unsigned %v0, %v5"),
        (64,2,"    %v5 = const.bits<2> 1\n    %v6 = shift_count_defined.signed %v0, %v5"),
        (64,2,"    %v6 = shift_count_defined.unsigned %v0, %v1"),
        (32,32,"    %v4 = const.bits<32> 1\n    %v5 = or.bits<32> %v0, %v4\n    %v6 = divisor_nonzero %v5"),
        (32,32,"    %v4 = const.bits<32> 7\n    %v5 = and.bits<32> %v1, %v4\n    %v6 = shift_count_defined.signed %v0, %v5"),
    ] {
        let before = program(n,m,body);
        let out = guard(&before);
        assert_eq!(out.certificate.regions.len(),1,"{body}: {:?}",out.report);
        assert_eq!(check(&before,&out.module,&out.certificate),0,"{body}");
        assert!(out.module.functions[0].blocks[0].insts.iter().any(|i| matches!(i,Inst::UbAssume(..))));
        let p0 = optimize_scalar_module_with_policy(&before,&ExtractionPolicy::default());
        assert!(!p0.certificate.regions.iter().any(|r| r.root_value==6));
    }
}

#[test]
fn invalid_division_and_shift_boundaries_are_not_certified() {
    for (n,m,body) in [
        (32,32,"    %v4 = const.bits<32> 2147483648\n    %v5 = const.bits<32> 4294967295\n    %v6 = signed_div_rem_no_overflow %v4, %v5"),
        (32,32,"    %v5 = const.bits<32> 32\n    %v6 = shift_count_defined.unsigned %v0, %v5"),
        (32,32,"    %v5 = const.bits<32> 4294967295\n    %v6 = shift_count_defined.signed %v0, %v5"),
        (2,64,"    %v5 = const.bits<64> 3\n    %v6 = shift_count_defined.unsigned %v0, %v5"),
        (64,2,"    %v5 = const.bits<2> 3\n    %v6 = shift_count_defined.signed %v0, %v5"),
        (64,2,"    %v4 = uninitialized.bits<64>\n    %v5 = const.bits<2> 1\n    %v6 = shift_count_defined.unsigned %v4, %v5"),
    ] {
        let before = program(n,m,body);
        let out = guard(&before);
        assert_eq!(out.module,before,"{body}");
        assert!(out.certificate.regions.is_empty(),"{body}");
        assert_eq!(check(&before,&out.module,&out.certificate),0);
    }
}

#[test]
fn independent_checker_rejects_all_one_field_certificate_forgeries() {
    let before = fixture();
    let out = guard(&before);
    assert_eq!(check(&before, &out.module, &out.certificate), 0);
    for mutation in 0..7 {
        let mut proof = out.certificate.clone();
        match mutation {
            0 => proof.registry_digest = "0".repeat(64),
            1 => proof.regions[0].root_value = 6,
            2 => {
                let r = proof.regions[0]
                    .events
                    .iter_mut()
                    .find_map(|e| {
                        if let EvpEvent::Rewrite(r) = e {
                            Some(r)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                r.rule = "g0_004_sub".to_owned();
            }
            3 => {
                let r = proof.regions[0]
                    .events
                    .iter_mut()
                    .find_map(|e| {
                        if let EvpEvent::Rewrite(r) = e {
                            Some(r)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                r.premises[0].value = 0;
            }
            4 => {
                let n = proof.regions[0]
                    .events
                    .iter_mut()
                    .find_map(|e| {
                        if let EvpEvent::Node(n) = e {
                            Some(n)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                n.sort = "bits<2>".to_owned();
            }
            5 => {
                let n = proof.regions[0]
                    .events
                    .iter_mut()
                    .find_map(|e| {
                        if let EvpEvent::Node(n) = e {
                            (n.operation == "signed_add_defined").then_some(n)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                n.operation = "signed_sub_defined".to_owned();
            }
            6 => {
                proof.registry_name = "p0.scalar.v4".to_owned();
                proof.registry_digest =
                    "1a5391603386e18e18bfb1b2fc61985c9eb252c6e65bcb3259104bd49b2a9d74".to_owned();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            check(&before, &out.module, &proof),
            3,
            "mutation {mutation}"
        );
    }
}

#[test]
fn splice_cannot_delete_obligations_or_weaken_effects() {
    let before = fixture();
    let out = guard(&before);
    for mutation in 0..2 {
        let mut after = out.module.clone();
        if mutation == 0 {
            after.functions[0].blocks[0]
                .insts
                .retain(|i| !matches!(i, Inst::UbAssume(..)));
            after.functions[0].effects.may_trap = false;
            validate_module(&after).unwrap();
        } else {
            after.functions[0].effects.may_trap = false;
        }
        let mut proof = out.certificate.clone();
        proof.output_digest = sha256_hex(render_module(&after).as_bytes());
        // A false effect summary is malformed IR (exit 2), not a checked
        // equality counterexample (exit 3). Keep those buckets distinct.
        assert_eq!(
            check(&before, &after, &proof),
            if mutation == 0 { 3 } else { 2 }
        );
    }
}

#[test]
fn g0_uses_the_existing_verified_persistent_chain() {
    let before = fixture();
    let mut state = evocc_eqopt::EqualityState::for_module(&before).unwrap();
    let result = evocc_eqopt::run_verified_scalar_chain(
        &before,
        &mut state,
        &policy_by_name("guard-known-true").unwrap(),
        |bundle| {
            let b = parse_section9_module(bundle.before.as_bytes()).unwrap();
            let a = parse_section9_module(bundle.after.as_bytes()).unwrap();
            let p = evocc_proof::parse_evp1(bundle.proof.as_bytes()).unwrap();
            if check(&b, &a, &p) == 0 {
                Ok(())
            } else {
                Err(evocc_eqopt::RejectionReason::ProofInvalid)
            }
        },
    )
    .unwrap();
    assert_eq!(result.transactions.len(), 2);
    assert_eq!(result.transactions[0].report.optimized_values, 1);
    assert_eq!(result.stop, evocc_eqopt::ChainStop::NoChange);
    state.verify_history().unwrap();
    result.validate().unwrap();
}

#[test]
fn count_signedness_cannot_be_forged_by_one_side() {
    let before = program(64, 2, "    %v6 = shift_count_defined.unsigned %v0, %v1");
    let out = guard(&before);
    assert_eq!(check(&before, &out.module, &out.certificate), 0);
    let mut proof = out.certificate.clone();
    for event in &mut proof.regions[0].events {
        match event {
            EvpEvent::Node(n) if n.operation == "shift_count_defined.unsigned" => {
                n.operation = "shift_count_defined.signed".to_owned()
            }
            EvpEvent::Rewrite(r) => r.rule = "g0_007_signed_count".to_owned(),
            _ => {}
        }
    }
    assert_eq!(check(&before, &out.module, &proof), 3);
}

#[test]
fn all_edge_seed_agreement_and_poison_are_still_required() {
    for (right, expected) in [("%v4", 1), ("%v0", 0), ("%v9", 0)] {
        let poison = if right == "%v9" {
            "    %v9 = uninitialized.bits<32>\n"
        } else {
            ""
        };
        let source = format!("func @main(%arg0: bits<32>, %arg1: bits<1>, %arg2: mem<all>, %arg3: world) -> (bits<1>, mem<all>, world) cc = target_default\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}\n{{\n^entry(%v0: bits<32>, %v1: bits<1>, %v2: mem<all>, %v3: world):\n    %v4 = const.bits<32> 1\n{poison}    cond_br %v1, ^b1(%v4, %v2, %v3), ^b1({right}, %v2, %v3)\n\n^b1(%v5: bits<32>, %v6: mem<all>, %v7: world):\n    %v8 = divisor_nonzero %v5\n    ub.assume.division_by_zero %v8\n    ret %v8, %v6, %v7\n}}\n");
        let before = parse_section9_module(source.as_bytes()).unwrap();
        validate_module(&before).unwrap();
        let out = guard(&before);
        assert_eq!(out.certificate.regions.len(), expected);
        assert_eq!(check(&before, &out.module, &out.certificate), 0);
    }
}
