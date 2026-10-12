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
    let _ = child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(input.as_bytes());
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
    let replaced = run(
        &[
            common[0],
            common[1],
            common[2],
            "-D__GNUC__=9",
            "/dev/stdin",
        ],
        "__GNUC__\n",
    );
    assert!(replaced.status.success());
    assert_eq!(String::from_utf8_lossy(&replaced.stdout).trim(), "9");
    assert!(String::from_utf8_lossy(&replaced.stderr).contains("macro `__GNUC__` redefined"));
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
fn macro_redefinition_preserves_the_kernel_capability_probe_exit_status() {
    // Kconfig negates the result: redefining a built-in under -Werror must
    // fail, or Linux incorrectly concludes that integer-128 is unavailable.
    for mode in ["-E", "--parse-only", "--emit=evoir", "-S", "-c"] {
        let mut args = vec![
            mode,
            "--target=aarch64-unknown-linux-gnu",
            "-Werror",
            "-D__SIZEOF_INT128__=0",
        ];
        if matches!(mode, "-S" | "-c") {
            args.extend(["-x", "c"]);
        }
        args.push("/dev/stdin");
        let result = run(&args, "int main(void) { return 0; }\n");
        assert!(
            !result.status.success(),
            "mode {mode}: accepted a conflicting definition"
        );
        assert!(
            result.stdout.is_empty(),
            "mode {mode}: emitted output after rejection"
        );
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("macro `__SIZEOF_INT128__` redefined"),
            "mode {mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn macro_redefinitions_obey_global_warning_controls() {
    for (control, warned) in [
        ("-w", false),
        ("-Wno-error", true),
        ("--cap-lints=allow", false),
        ("--cap-lints=warn", true),
    ] {
        let result = run(
            &[
                "-E",
                "-P",
                "-Werror",
                control,
                "-D__SIZEOF_INT128__=0",
                "/dev/stdin",
            ],
            "__SIZEOF_INT128__\n",
        );
        assert!(
            result.status.success(),
            "{control}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "0");
        assert_eq!(!result.stderr.is_empty(), warned, "{control}");
    }
}

#[test]
fn equivalent_macro_definitions_and_explicit_undefinition_are_not_redefinitions() {
    for definitions in [
        vec!["-D__SIZEOF_INT128__=16"],
        vec!["-D__SIZEOF_INT128__=  16  "],
        vec!["-U__SIZEOF_INT128__", "-D__SIZEOF_INT128__=0"],
        vec!["-DVALUE=a + b", "-DVALUE= a  +/**/ b "],
        vec!["-DVALUE=", "-DVALUE= "],
    ] {
        let mut args = vec!["-E", "-Werror"];
        args.extend(definitions);
        args.push("/dev/stdin");
        let result = run(&args, "");
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
    }
    for definitions in [["-DVALUE=1", "-DVALUE=2"], ["-DVALUE=a+b", "-DVALUE=a +b"]] {
        let result = run(
            &[
                "-E",
                "-Werror",
                definitions[0],
                definitions[1],
                "/dev/stdin",
            ],
            "",
        );
        assert!(!result.status.success(), "{definitions:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("macro `VALUE` redefined"));
    }
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
        stderr.contains("error[E0112]: stray-token"),
        "unexpected diagnostic: {stderr:?}"
    );
}
