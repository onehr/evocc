use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["-O2", mode, "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn inline_constant_queries_are_recomputed_after_parameter_substitution() {
    let source = include_str!("../../../tests/fixtures/inline-constant-query.c");
    let output = run(source, "--interpret-evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_required"));
}

#[test]
fn return_address_extraction_is_an_evaluated_identity_not_an_external_call() {
    let source = include_str!("../../../tests/fixtures/extract-return-address.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("__builtin_extract_return_addr"));
}
