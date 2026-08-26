use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run evocc");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(input.as_bytes())
        .expect("write source");
    child.wait_with_output().expect("wait for evocc")
}

fn stdout(args: &[&str], input: &str) -> String {
    let output = run(args, input);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout)
        .expect("preprocessed output is UTF-8")
        .trim()
        .to_owned()
}

#[test]
fn command_line_definitions_and_undefinitions_follow_the_target_profile() {
    let common = ["-E", "-P", "--target=aarch64-unknown-linux-gnu"];
    assert_eq!(
        stdout(
            &[
                common[0],
                common[1],
                common[2],
                "-D__GNUC__=9",
                "/dev/stdin"
            ],
            "__GNUC__\n"
        ),
        "9"
    );
    assert_eq!(
        stdout(
            &[common[0], common[1], common[2], "/dev/stdin"],
            "__GNUC__\n"
        ),
        "14"
    );
    assert_eq!(
        stdout(
            &[common[0], common[1], common[2], "-U__GNUC__", "/dev/stdin"],
            "#ifdef __GNUC__\ny\n#else\nn\n#endif\n",
        ),
        "n"
    );
}

#[test]
fn a_pasted_nonliteral_reaches_the_c_lexer_diagnostic_at_the_parser_boundary() {
    let output = run(
        &[
            "--parse-only",
            "--target=aarch64-unknown-linux-gnu",
            "/dev/stdin",
        ],
        "#define CAT(a, b) a##b\nint value = CAT(1, x);\n",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic is UTF-8");
    assert!(
        stderr.contains("error[E0001]: invalid token"),
        "unexpected diagnostic: {stderr:?}"
    );
}
