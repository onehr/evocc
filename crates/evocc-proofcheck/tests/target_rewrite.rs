//! The target line is part of the certified bytes, not a host-side hint.
use evocc_eqopt::{
    run_scalar_chain_with_checker, CostFunction, EqualityState, ExtractionPolicy, RegistryCeiling,
    SchedulePolicy,
};
use evocc_evoir::{parse_section9_module, render_target_module, IMPLICIT_TARGET};
use evocc_proof::{parse_evp1, render_evp1, sha256_hex, CheckerOutcome, StandaloneProofChecker};

const DARWIN: &str = "aarch64-apple-darwin";

fn checker() -> StandaloneProofChecker {
    let binary = std::path::PathBuf::from(env!("CARGO_BIN_EXE_evo-proofcheck"));
    let digest = sha256_hex(&std::fs::read(&binary).unwrap());
    StandaloneProofChecker::new(binary, std::env::temp_dir()).with_expected_digest(digest)
}

fn module(body: &str, result_width: u16) -> evocc_evoir::Module {
    parse_section9_module(format!(
        "func @f(%arg0: bits<8>) -> (bits<{result_width}>) cc = target_default\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}\n{{\n^entry(%v0: bits<8>):\n{body}\n    ret %v2\n}}\n"
    ).as_bytes()).unwrap()
}

#[test]
fn every_registry_replays_under_both_explicit_text_profiles() {
    let checker = checker();
    for (registry, width, body) in [
        (
            RegistryCeiling::V1,
            8,
            "    %v1 = const.bits<8> 0\n    %v2 = add.bits<8> %v0, %v1",
        ),
        (
            RegistryCeiling::V2,
            8,
            "    %v1 = const.bits<8> 255\n    %v2 = or.bits<8> %v0, %v1",
        ),
        (
            RegistryCeiling::V3,
            32,
            "    %v1 = const.bits<8> 1\n    %v2 = zext.bits<32> %v1",
        ),
        (
            RegistryCeiling::V4,
            8,
            "    %v1 = const.bits<8> 2\n    %v2 = mul.bits<8> %v0, %v1",
        ),
        (
            RegistryCeiling::GuardV1,
            1,
            "    %v1 = const.bits<8> 1\n    %v2 = divisor_nonzero %v1",
        ),
    ] {
        let before = module(body, width);
        let policy = ExtractionPolicy::new(
            "target-replay",
            registry,
            CostFunction::WeightedOperators,
            SchedulePolicy::default(),
        );
        let mut runs = Vec::new();
        for target in [IMPLICIT_TARGET, DARWIN] {
            let mut state = EqualityState::for_target_module(&before, target).unwrap();
            let run = run_scalar_chain_with_checker(&before, &mut state, &policy, &checker)
                .unwrap_or_else(|error| panic!("{}: {}", registry.name(), error.detail));
            assert_eq!(run.target, target);
            run.validate().unwrap();
            let bundle = &run.transactions[0].bundle;
            let proof = parse_evp1(bundle.proof.as_bytes()).unwrap();
            assert_eq!(proof.registry_name, registry.name());
            assert!(
                !proof.regions.is_empty(),
                "{} must really rewrite",
                registry.name()
            );
            assert_eq!(bundle.before, render_target_module(&before, target));
            assert_eq!(proof.input_digest, sha256_hex(bundle.before.as_bytes()));
            assert_eq!(proof.output_digest, sha256_hex(bundle.after.as_bytes()));
            assert_eq!(
                run.output_digest,
                sha256_hex(render_target_module(&run.module, target).as_bytes())
            );
            state.verify_history().unwrap();
            runs.push(run);
        }
        // The target affects the binding, not the meaning or width of an
        // integer rule. Neither registry includes memory, pointer or FP rules.
        assert_eq!(runs[0].module, runs[1].module);
        assert_ne!(runs[0].chain_id, runs[1].chain_id);
        let linux = &runs[0].transactions[0].bundle;
        let darwin = &runs[1].transactions[0].bundle;
        assert_eq!(
            checker.check_typed(&linux.before, &linux.after, &darwin.proof),
            CheckerOutcome::Invalid
        );
        assert_eq!(
            checker.check_typed(&darwin.before, &darwin.after, &linux.proof),
            CheckerOutcome::Invalid
        );

        // Rehashing a changed output cannot switch its profile. A valid proof
        // must use one target for before, after and independent reconstruction.
        let mut forged = parse_evp1(darwin.proof.as_bytes()).unwrap();
        forged.output_digest = sha256_hex(linux.after.as_bytes());
        assert_eq!(
            checker.check_typed(&darwin.before, &linux.after, &render_evp1(&forged)),
            CheckerOutcome::Malformed
        );
        let unknown = darwin.before.replacen(DARWIN, "aarch64-unknown-none", 1);
        assert_eq!(
            checker.check_typed(&unknown, &darwin.after, &darwin.proof),
            CheckerOutcome::Malformed
        );
    }
}

#[test]
fn empty_modules_and_state_identity_keep_the_target() {
    let input = evocc_evoir::Module::default();
    let checker = checker();
    let mut linux = EqualityState::for_module(&input).unwrap();
    let mut darwin = EqualityState::for_target_module(&input, DARWIN).unwrap();
    assert_ne!(linux.audit(), darwin.audit());
    assert!(EqualityState::for_target_module(&input, "aarch64-unknown-none").is_err());
    for state in [&mut linux, &mut darwin] {
        let run =
            run_scalar_chain_with_checker(&input, state, &ExtractionPolicy::default(), &checker)
                .unwrap();
        assert_eq!(run.transactions.len(), 1);
        assert_eq!(
            run.transactions[0].bundle.before,
            render_target_module(&input, state.target())
        );
        let mut forged = run;
        forged.target = if state.target() == DARWIN {
            IMPLICIT_TARGET
        } else {
            DARWIN
        }
        .into();
        assert!(forged.validate().is_err());
    }
}
