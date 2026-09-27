use std::process::Command;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/return-conversion-constraints.c"
);

const CALL_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/call-conversion-constraints.c"
);

#[test]
fn fixed_prototype_arguments_share_the_assignment_constraints() {
    for case in 1..=9 {
        for mode in [
            "--parse-only",
            "--interpret-c-hir",
            "--interpret-evoir",
            "--emit=asm",
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args([mode, "-std=gnu11", &format!("-DCASE={case}"), CALL_FIXTURE])
                .output()
                .unwrap();
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "case {case} {mode}");
            assert!(error.contains("E2202"), "case {case} {mode}: {error}");
            assert!(output.stdout.is_empty());
        }
    }
}

#[test]
fn legitimate_call_conversions_keep_defaults_unions_and_effects() {
    for mode in [
        "--parse-only",
        "--interpret-c-hir",
        "--interpret-evoir",
        "--emit=asm",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([mode, "-std=gnu11", CALL_FIXTURE])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret-") {
            assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
        }
    }
}

#[test]
fn return_constraints_reject_before_execution_or_ir_construction() {
    for case in 1..=11 {
        let code = if (8..=10).contains(&case) {
            "E1207"
        } else {
            "E2202"
        };
        for mode in [
            "--parse-only",
            "--interpret-c-hir",
            "--interpret-evoir",
            "--emit=asm",
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args([mode, "-std=gnu11", &format!("-DCASE={case}"), FIXTURE])
                .output()
                .unwrap();
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "case {case} {mode}");
            assert!(error.contains(code), "case {case} {mode}: {error}");
            assert!(!error.contains("E3101"), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
}

#[test]
fn legitimate_gnu_return_conversions_and_explicit_casts_are_preserved() {
    for mode in [
        "--parse-only",
        "--interpret-c-hir",
        "--interpret-evoir",
        "--emit=asm",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([mode, "-std=gnu11", FIXTURE])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret-") {
            assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
        }
    }
}
