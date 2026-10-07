use std::io::Write;
use std::process::{Command, Output, Stdio};

fn parse(source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg("--error-format=short")
        .args([
            "--parse-only",
            "--target=aarch64-unknown-linux-gnu",
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run evocc");
    let _ = child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(source.as_bytes());
    child.wait_with_output().expect("wait for evocc")
}

fn assert_stray_token(source: &str, line: u32, column: u32) {
    let output = parse(source);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic is UTF-8");
    assert_eq!(
        stderr,
        format!("/dev/stdin:{line}:{column}: error[E0112]: stray-token\n"),
        "unexpected diagnostic"
    );
}

#[test]
fn other_character_reaching_the_c_lexer_is_a_stray_token() {
    assert_stray_token("int x = \\;\n", 1, 9);
}

#[test]
fn leading_dot_preprocessing_number_without_a_c_form_is_a_stray_token() {
    assert_stray_token("int x = .5foo;\n", 1, 9);
}

#[test]
fn multi_dot_preprocessing_number_without_a_c_form_is_a_stray_token() {
    assert_stray_token("int x = 1.2.3e+x;\n", 1, 9);
}
