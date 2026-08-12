use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn unique_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("c30-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

fn stdin_run(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(evocc())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn cpp_flags_have_real_semantics() {
    let root = unique_dir("cpp-flags");
    let include = root.join("include");
    fs::create_dir(&include).unwrap();
    fs::write(include.join("witness.h"), "#define HEADER_VALUE 73\n").unwrap();
    let source = root.join("input.c");
    fs::write(&source, "#include <witness.h>\nCLI_VALUE HEADER_VALUE\n").unwrap();

    let output = Command::new(evocc())
        .args([
            "-std=gnu11",
            "-E",
            "-P",
            "-undef",
            "-nostdinc",
            &format!("-I{}", include.display()),
            "-DCLI_VALUE=42",
            source.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "42 73");
    assert!(output.stderr.is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_inputs_fail_closed() {
    for input in [
        b"int value".as_slice(),
        b"int main(void) { return 0;".as_slice(),
        b"int value; @".as_slice(),
    ] {
        let output = stdin_run(&["--parse-only", "/dev/stdin"], input);
        assert_eq!(output.status.code(), Some(1), "input={input:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error["));
    }

    let output = stdin_run(&["-E", "/dev/stdin"], b"#error stop now\n");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error[E0104]"));
}

#[test]
fn preprocessed_output_does_not_fuse_tokens() {
    for (input, forbidden) in [
        ("int x = left + +right;\n", "left++right"),
        ("int x = left / *ptr;\n", "left/*ptr"),
        ("int x = left - -right;\n", "left--right"),
    ] {
        let output = stdin_run(&["-E", "/dev/stdin"], input.as_bytes());
        assert!(output.status.success());
        let rendered = String::from_utf8(output.stdout).unwrap();
        assert!(!rendered.contains(forbidden), "{rendered}");
    }
}
