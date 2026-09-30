//! R01a: the run, not an individual producer transaction, owns publication.
use evocc_eqopt::{
    run_verified_scalar_chain, valid_transaction_state, CertificateState, ChainStop, CostFunction,
    EqualityState, ExtractionPolicy, OperatorWeights, RejectionReason, SchedulePolicy,
    TransactionResult, TransactionStop, Verification,
};
use evocc_evoir::{parse_section9_module, render_module, Module};

fn input() -> Module {
    parse_section9_module(br#"func @main(%arg0: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>):
    %v1 = const.bits<32> 0
    %v2 = add.bits<32> %v0, %v1
    ret %v2
}
"#).expect("valid reduced input")
}

#[test]
fn inconsistent_public_wall_scopes_must_not_panic() {
    let resources = evocc_eqopt::ChainResources {
        total_wall_nanos: 10,
        checker_wall_nanos: 2,
        identity_wall_nanos: 1,
    };
    assert_eq!(resources.wrapper_wall_nanos(3).unwrap(), 4);
    for producer in [8, 9, 11, u128::MAX] {
        let result = std::panic::catch_unwind(|| resources.wrapper_wall_nanos(producer));
        assert!(result
            .unwrap()
            .unwrap_err()
            .contains("inconsistent chain wall scopes"));
    }
    let overflowing_scopes = evocc_eqopt::ChainResources {
        total_wall_nanos: u128::MAX,
        checker_wall_nanos: u128::MAX,
        identity_wall_nanos: 1,
    };
    assert!(overflowing_scopes.wrapper_wall_nanos(1).is_err());
}

#[test]
fn chain_checks_the_terminal_zero_region_certificate() {
    let module = input();
    let mut state = EqualityState::for_module(&module).unwrap();
    let mut calls = 0;
    let run = run_verified_scalar_chain(
        &module,
        &mut state,
        &ExtractionPolicy::default(),
        |bundle| {
            calls += 1;
            assert!(!bundle.proof.is_empty());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(run.stop, ChainStop::NoChange);
    assert_eq!(run.transactions.len(), 2);
    assert_eq!(run.transactions[0].result, TransactionResult::Changed);
    assert_eq!(run.transactions[1].result, TransactionResult::Unchanged);
    assert!(run
        .transactions
        .iter()
        .all(|t| t.verification == Verification::Verified));
    assert_ne!(render_module(&run.module), render_module(&module));
    run.validate().unwrap();
}

#[test]
fn selected_pair_telemetry_records_rejected_gates_before_admission_caps() {
    for weight in [0, 1, 5] {
        let module = input();
        let policy = ExtractionPolicy::default()
            .with_cost(CostFunction::Vector(OperatorWeights::uniform(weight)));
        let mut state = EqualityState::for_module(&module).unwrap();
        let run = run_verified_scalar_chain(&module, &mut state, &policy, |_| Ok(())).unwrap();
        let report = &run.transactions[0].report;
        assert_eq!(report.policy_decisions.len(), 1);
        let decision = &report.policy_decisions[0];
        assert_eq!(decision.root_axis, Some(evocc_eqopt::CostAxis::Add));
        assert_eq!(decision.replacement_axis, None);
        assert_eq!(decision.root_cost, weight);
        assert_eq!(decision.replacement_cost, 0);
        assert_eq!(decision.kept, weight != 0);
        assert_eq!(decision.function_index, 0);
        assert_eq!(decision.root, 2);
        assert_eq!(report.telemetry.declined_policy, usize::from(weight == 0));
    }
}

#[test]
fn repeated_rule_firings_in_distinct_regions_do_not_share_fuel() {
    // Reduced from zstd_compress.c: 300 independent two-node regions each
    // fire known_sext once. They are not 300 firings in one region.
    let mut source = String::from(
        "func @many() -> (bits<64>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry():\n",
    );
    for i in 0..300 {
        source.push_str(&format!(
            "    %v{} = const.bits<32> {}\n    %v{} = sext.bits<64> %v{}\n",
            2 * i,
            i + 1,
            2 * i + 1,
            2 * i
        ));
    }
    source.push_str("    ret %v599\n}\n");
    let module = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = evocc_eqopt::optimize_scalar_module_with_evp1(
        &module,
        evocc_eqopt::ScalarEqBudget::default(),
    );
    assert_eq!(outcome.certificate.regions.len(), 300);
    let budget = outcome.certificate.budget.unwrap();
    assert_eq!(budget.n, 2);
    assert_eq!(budget.fuel.consumed, 1);
    assert_eq!(budget.fuel.bound, 256);
    let checker = evocc_proof::StandaloneProofChecker::locate().unwrap();
    let expected = checker.identity_digest().unwrap(); // This test's workspace checker.
    let checker = checker.with_expected_digest(expected);
    let mut state = EqualityState::for_module(&module).unwrap();
    let run = evocc_eqopt::run_scalar_chain_with_checker(
        &module,
        &mut state,
        &ExtractionPolicy::default(),
        &checker,
    )
    .unwrap_or_else(|error| panic!("{}", error.detail));
    assert_eq!(run.transactions[0].report.optimized_values, 300);
    for candidate in [&module, &run.module] {
        let result = evocc_evoir::interpret_module(
            candidate,
            "many",
            &[],
            evocc_evoir::InterpreterLimits::new(10_000),
        )
        .unwrap();
        assert_eq!(
            result.return_values,
            vec![evocc_evoir::RuntimeValue::new(
                evocc_evoir::EvoIRType::Bits(64),
                300
            )]
        );
    }
}

#[test]
fn region_normalization_retains_its_own_fact_environment() {
    let module = parse_section9_module(br#"func @main() -> (bits<64>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry():
    %v0 = const.bits<64> 2021813942389193322
    br ^b1(%v0)

^b1(%v1: bits<64>):
    %v2 = const.bits<64> 2
    %v3 = mul.bits<64> %v1, %v2
    %v4 = const.bits<64> 4294967295
    %v5 = and.bits<64> %v3, %v4
    %v6 = const.bits<64> 18446744073709551615
    %v7 = and.bits<64> %v5, %v6
    ret %v7
}
"#).unwrap();
    let mut state = EqualityState::for_module(&module).unwrap();
    let checker = evocc_proof::StandaloneProofChecker::locate().unwrap();
    let expected = checker.identity_digest().unwrap(); // Trust this test's workspace build.
    let checker = checker.with_expected_digest(expected);
    let run = evocc_eqopt::run_scalar_chain_with_checker(
        &module,
        &mut state,
        &ExtractionPolicy::default(),
        &checker,
    )
    .unwrap_or_else(|error| panic!("{}", error.detail));
    assert!(run.transactions[0].report.optimized_values > 0);
    for candidate in [&module, &run.module] {
        let outcome = evocc_evoir::interpret_module(
            candidate,
            "main",
            &[],
            evocc_evoir::InterpreterLimits::new(100_000),
        )
        .unwrap();
        assert_eq!(
            outcome.return_values,
            vec![evocc_evoir::RuntimeValue::new(
                evocc_types::EvoIRType::Bits(64),
                471102676
            )]
        );
    }
    state.verify_history().unwrap();
}

#[test]
fn rejected_second_transaction_rolls_back_the_entire_state() {
    let module = input();
    let mut state = EqualityState::for_module(&module).unwrap();
    let original = state.audit();
    let mut calls = 0;
    let failure =
        run_verified_scalar_chain(&module, &mut state, &ExtractionPolicy::default(), |_| {
            calls += 1;
            if calls == 2 {
                Err(RejectionReason::ProofInvalid)
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(calls, 2);
    assert_eq!(failure.verified_prefix, 1);
    assert_eq!(failure.rejection_reason, RejectionReason::ProofInvalid);
    assert_eq!(
        failure.transactions[1].rejection_reason,
        RejectionReason::ProofInvalid
    );
    assert_eq!(state.audit(), original);
}

#[cfg(unix)]
#[test]
fn successful_replays_cannot_commit_after_checker_identity_changes() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let directory =
        std::env::temp_dir().join(format!("evocc-checker-mutation-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("checker");
    let real = evocc_proof::StandaloneProofChecker::locate().unwrap();
    let quoted = real.binary().to_str().unwrap().replace('\'', "'\\''");
    // Replay with the real checker, then mutate only this test-owned wrapper.
    fs::write(&path, format!("#!/bin/sh\n'{quoted}' \"$@\"\nstatus=$?\nprintf '\\n# changed\\n' >> \"$0\"\nexit \"$status\"\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    let checker = evocc_proof::StandaloneProofChecker::new(path, directory.clone());
    let identity = checker.identity_digest().unwrap();
    let checker = checker.with_expected_digest(identity);
    let module = input();
    let mut state = EqualityState::for_module(&module).unwrap();
    state.ingest_module(&module).unwrap();
    let original = state.audit();
    let failure = evocc_eqopt::run_scalar_chain_with_checker(
        &module,
        &mut state,
        &ExtractionPolicy::default(),
        &checker,
    )
    .unwrap_err();
    assert_eq!(failure.rejection_reason, RejectionReason::BundleMismatch);
    assert_eq!(failure.verified_prefix, 2);
    assert_eq!(state.audit(), original);
    assert!(failure
        .transactions
        .iter()
        .all(|t| t.verification == Verification::Verified));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn legacy_persistent_view_uses_the_same_remap_and_commit_as_certified_path() {
    let module = input();
    let mut legacy = EqualityState::for_module(&module).unwrap();
    legacy.ingest_module(&module).unwrap();
    let mut certified = legacy.clone();
    let budget = evocc_eqopt::ScalarEqBudget::new(100_000, 1_024);
    let a = evocc_eqopt::optimize_scalar_module_persistent(&module, &mut legacy, budget);
    let b =
        evocc_eqopt::optimize_scalar_module_certified_persistent(&module, &mut certified, budget);
    assert_eq!(a.module, b.module);
    assert_eq!(a.report, b.report);
    assert_eq!(legacy.audit(), certified.audit());
    let before = legacy.audit();
    let second = evocc_eqopt::optimize_scalar_module_persistent(&a.module, &mut legacy, budget);
    assert!(
        !second.report.fell_back,
        "the next public transaction must accept the remapped identity"
    );
    assert_eq!(second.module, a.module);
    assert_eq!(legacy.audit(), before);
}

#[test]
fn unpinned_checker_and_out_of_profile_deadline_cannot_commit_state() {
    let module = input();
    let mut state = EqualityState::for_module(&module).unwrap();
    let original = state.audit();
    let checker = evocc_proof::StandaloneProofChecker::new(
        evocc_proof::StandaloneProofChecker::locate()
            .unwrap()
            .binary()
            .clone(),
        std::env::temp_dir(),
    );
    let failure = evocc_eqopt::run_scalar_chain_with_checker(
        &module,
        &mut state,
        &ExtractionPolicy::default(),
        &checker,
    )
    .unwrap_err();
    assert!(failure.detail.contains("missing trusted"));
    assert!(failure.transactions.is_empty());
    assert_eq!(state.audit(), original);
    let expected = checker.identity_digest().unwrap();
    let checker = checker
        .with_expected_digest(expected)
        .with_timeout(std::time::Duration::from_millis(300_001));
    let failure = evocc_eqopt::run_scalar_chain_with_checker(
        &module,
        &mut state,
        &ExtractionPolicy::default(),
        &checker,
    )
    .unwrap_err();
    assert!(failure.detail.contains("deadline"));
    assert!(failure.transactions.is_empty());
    assert_eq!(state.audit(), original);
}

#[test]
fn zero_budget_is_checked_and_stops_without_hidden_extra_transactions() {
    let module = input();
    let mut state = EqualityState::for_module(&module).unwrap();
    let policy = ExtractionPolicy::default().with_schedule(SchedulePolicy {
        max_nodes: 0,
        max_epochs: 0,
        max_transactions: 8,
    });
    let run = run_verified_scalar_chain(&module, &mut state, &policy, |_| Ok(())).unwrap();
    assert_eq!(run.stop, ChainStop::TransactionBudget);
    assert_eq!(run.transactions.len(), 1);
    assert_eq!(render_module(&module), render_module(&run.module));
    run.validate().unwrap();
}

#[test]
fn malformed_and_invalid_checker_verdicts_are_distinct() {
    assert_eq!(
        RejectionReason::from_checker_exit(Some(2)),
        RejectionReason::CertificateMalformed
    );
    assert_eq!(
        RejectionReason::from_checker_exit(Some(3)),
        RejectionReason::ProofInvalid
    );
    assert_eq!(
        RejectionReason::from_checker_exit(Some(1)),
        RejectionReason::CheckerError
    );
    assert_eq!(
        RejectionReason::from_checker_exit(None),
        RejectionReason::CheckerError
    );
}

#[test]
fn transaction_ceiling_is_part_of_policy_identity() {
    let p = ExtractionPolicy::default();
    let q = p.with_schedule(SchedulePolicy {
        max_transactions: 1,
        ..p.schedule()
    });
    assert_ne!(p.identity_digest(), q.identity_digest());
    assert_eq!(p.identity_digest(), p.identity_digest());
}

#[test]
fn closed_state_reason_product_has_exactly_45_legal_combinations() {
    let mut count = 0;
    for result in [
        TransactionResult::Changed,
        TransactionResult::Unchanged,
        TransactionResult::RolledBackError,
    ] {
        for stop in [
            TransactionStop::Normal,
            TransactionStop::PolicyBudget,
            TransactionStop::InternalError,
        ] {
            for cert in [CertificateState::Emitted, CertificateState::None] {
                for v in [
                    Verification::NotChecked,
                    Verification::Verified,
                    Verification::Rejected,
                ] {
                    for r in [
                        RejectionReason::None,
                        RejectionReason::CertificateMalformed,
                        RejectionReason::ProofInvalid,
                        RejectionReason::CheckerTimeout,
                        RejectionReason::CheckerUnavailable,
                        RejectionReason::CheckerError,
                        RejectionReason::CertificateMissing,
                        RejectionReason::BundleMismatch,
                    ] {
                        let expected = match (result, stop, cert, v, r) {
                            (
                                TransactionResult::RolledBackError,
                                TransactionStop::InternalError,
                                CertificateState::None,
                                Verification::NotChecked,
                                RejectionReason::None,
                            ) => true,
                            (
                                TransactionResult::Changed | TransactionResult::Unchanged,
                                TransactionStop::Normal | TransactionStop::PolicyBudget,
                                CertificateState::None,
                                Verification::Rejected,
                                RejectionReason::CertificateMissing,
                            ) => true,
                            (a, b, CertificateState::Emitted, c, d)
                                if (a == TransactionResult::RolledBackError)
                                    == (b == TransactionStop::InternalError) =>
                            {
                                match c {
                                    Verification::NotChecked | Verification::Verified => {
                                        d == RejectionReason::None
                                    }
                                    Verification::Rejected => !matches!(
                                        d,
                                        RejectionReason::None | RejectionReason::CertificateMissing
                                    ),
                                }
                            }
                            _ => false,
                        };
                        // Producer failure explains the chain, never a checker verdict.
                        assert!(!valid_transaction_state(
                            result,
                            stop,
                            cert,
                            v,
                            RejectionReason::ProducerError
                        ));
                        let got = valid_transaction_state(result, stop, cert, v, r);
                        assert_eq!(got, expected, "{result:?} {stop:?} {cert:?} {v:?} {r:?}");
                        count += usize::from(got);
                    }
                }
            }
        }
    }
    assert_eq!(count, 45);
}

#[test]
fn independent_checker_accepts_every_bundle_and_mutations_are_rejected() {
    let module = input();
    let checker = evocc_proof::StandaloneProofChecker::locate().unwrap();
    let expected = checker.identity_digest().unwrap(); // Trust this test's workspace build.
    let checker = checker.with_expected_digest(expected);
    let mut state = EqualityState::for_module(&module).unwrap();
    let run = run_verified_scalar_chain(&module, &mut state, &ExtractionPolicy::default(), |b| {
        RejectionReason::from_checker_outcome(checker.check_typed(&b.before, &b.after, &b.proof))
    })
    .unwrap();
    for b in run.transactions.iter().map(|t| &t.bundle) {
        assert_eq!(
            checker.check_typed(&b.before, &b.after, "not a certificate"),
            evocc_proof::CheckerOutcome::Malformed
        );
        let mut proof = evocc_proof::parse_evp1(b.proof.as_bytes()).unwrap();
        proof.output_digest = "0".repeat(64);
        assert_eq!(
            checker.check_typed(&b.before, &b.after, &evocc_proof::render_evp1(&proof)),
            evocc_proof::CheckerOutcome::Invalid
        );
    }
    let mut bad = run.clone();
    bad.transactions[1].input_digest = "0".repeat(64);
    assert!(bad.validate().is_err());
    let mut bad = run.clone();
    bad.max_transactions += 1;
    assert!(bad.validate().is_err());
    let mut bad = run.clone();
    bad.transactions[0].report.telemetry.roots_visited = usize::MAX;
    assert!(bad.validate().is_err());
    let mut bad = run.clone();
    bad.transactions[0].bundle.after.push(' ');
    assert!(bad.validate().is_err());
}

#[test]
fn policy_is_respected_by_persistent_path_and_foreign_state_is_rejected() {
    let module = input();
    let policy =
        ExtractionPolicy::default().with_cost(CostFunction::Vector(OperatorWeights::uniform(0)));
    let mut state = EqualityState::for_module(&module).unwrap();
    let run = run_verified_scalar_chain(&module, &mut state, &policy, |_| Ok(())).unwrap();
    assert_eq!(run.transactions.len(), 1);
    assert_eq!(run.module, module);
    assert!(run.transactions[0].report.telemetry.declined_policy > 0);
    let mut other = module.clone();
    other.functions[0].name = "other".to_owned();
    let original = state.audit();
    assert!(
        run_verified_scalar_chain(&other, &mut state, &policy, |_| panic!(
            "must not launch checker"
        ))
        .is_err()
    );
    assert_eq!(state.audit(), original);
}

#[test]
fn partial_budget_keeps_verified_regions_and_stops_the_chain() {
    let original = render_module(&input());
    let module = parse_section9_module(
        original
            .replace(
                "    ret %v2",
                "    %v3 = xor.bits<32> %v2, %v1\n    ret %v3",
            )
            .as_bytes(),
    )
    .unwrap();
    let policy = ExtractionPolicy::default().with_schedule(SchedulePolicy {
        max_epochs: 1,
        ..Default::default()
    });
    let checker = evocc_proof::StandaloneProofChecker::locate().unwrap();
    let expected = checker.identity_digest().unwrap(); // Trust this test's workspace build.
    let checker = checker.with_expected_digest(expected);
    let mut state = EqualityState::for_module(&module).unwrap();
    let run =
        evocc_eqopt::run_scalar_chain_with_checker(&module, &mut state, &policy, &checker).unwrap();
    assert_eq!(run.stop, ChainStop::TransactionBudget);
    assert_eq!(run.transactions.len(), 1);
    let t = &run.transactions[0];
    assert_eq!(t.result, TransactionResult::Changed);
    assert_eq!(t.report.optimized_values, 1);
    assert!(t.report.telemetry.search_events_hit);
    assert!(!t.report.telemetry.admission_regions_hit);
    assert_ne!(t.bundle.before, t.bundle.after);
    assert_eq!(
        evocc_proof::parse_evp1(t.bundle.proof.as_bytes())
            .unwrap()
            .regions
            .len(),
        1
    );
}

#[test]
fn eligible_no_rule_is_a_legitimate_noop() {
    let original = render_module(&input());
    let module = parse_section9_module(
        original
            .replace("add.bits<32> %v0, %v1", "sub.bits<32> %v1, %v0")
            .as_bytes(),
    )
    .unwrap();
    let mut state = EqualityState::for_module(&module).unwrap();
    let run =
        run_verified_scalar_chain(
            &module,
            &mut state,
            &ExtractionPolicy::default(),
            |_| Ok(()),
        )
        .unwrap();
    assert_eq!(run.transactions[0].result, TransactionResult::Unchanged);
    assert!(run.transactions[0].report.telemetry.roots_eligible > 0);
    assert_eq!(run.transactions[0].report.telemetry.roots_proposed, 0);
}

#[test]
fn emitted_definitions_remain_bound_for_later_congruence() {
    let original = render_module(&input());
    let module = parse_section9_module(original.replace("    %v1 = const.bits<32> 0\n    %v2 = add.bits<32> %v0, %v1\n    ret %v2", "    %v1 = const.bits<32> 2\n    %v2 = mul.bits<32> %v0, %v1\n    %v3 = add.bits<32> %v0, %v0\n    %v4 = xor.bits<32> %v2, %v3\n    ret %v4").as_bytes()).unwrap();
    let mut state = EqualityState::for_module(&module).unwrap();
    let checker = evocc_proof::StandaloneProofChecker::locate().unwrap();
    let expected = checker.identity_digest().unwrap(); // Trust this test's workspace build.
    let checker = checker.with_expected_digest(expected);
    let run = evocc_eqopt::run_scalar_chain_with_checker(
        &module,
        &mut state,
        &evocc_eqopt::policy_by_name("prefer-add").unwrap(),
        &checker,
    )
    .unwrap_or_else(|e| panic!("{}", e.detail));
    assert!(run.transactions.len() > 1);
    state.verify_history().unwrap();
}

#[test]
fn newly_emitted_result_keeps_the_original_equality_class() {
    let module = parse_section9_module(
        render_module(&input())
            .replace("const.bits<32> 0", "const.bits<32> 2")
            .replace("add.bits<32> %v0, %v1", "mul.bits<32> %v0, %v1")
            .as_bytes(),
    )
    .unwrap();
    let mut state = EqualityState::for_module(&module).unwrap();
    state.ingest_module(&module).unwrap();
    let original = state.class_of("main", 2).unwrap();
    let policy = evocc_eqopt::policy_by_name("prefer-add").unwrap();
    let out = evocc_eqopt::optimize_scalar_module_certified_persistent_with_policy(
        &module, &mut state, &policy,
    );
    assert_eq!(out.report.optimized_values, 1);
    let evocc_evoir::Inst::Ret(values) = &out.module.functions[0].blocks[0].terminator else {
        panic!("return")
    };
    assert_eq!(
        state.class_of("main", values[0].id),
        Some(state.canonical_class(original)),
        "emitted value lost its persistent binding"
    );
}

#[test]
fn known_edge_fact_folds_newly_emitted_root_in_the_next_transaction() {
    let module = parse_section9_module(br#"func @main() -> (bits<7>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry():
    %v0 = const.bits<7> 63
    br ^b1(%v0)

^b1(%v1: bits<7>):
    %v2 = const.bits<7> 2
    %v3 = mul.bits<7> %v1, %v2
    ret %v3
}
"#).unwrap();
    let mut state = EqualityState::for_module(&module).unwrap();
    let checker = evocc_proof::StandaloneProofChecker::locate().unwrap();
    let expected = checker.identity_digest().unwrap(); // Trust this test's workspace build.
    let checker = checker.with_expected_digest(expected);
    let run = evocc_eqopt::run_scalar_chain_with_checker(
        &module,
        &mut state,
        &evocc_eqopt::policy_by_name("prefer-add").unwrap(),
        &checker,
    )
    .unwrap_or_else(|e| panic!("{}", e.detail));
    assert!(run.transactions.len() >= 3);
    assert!(render_module(&run.module).contains("const.bits<7> 126"));
}
