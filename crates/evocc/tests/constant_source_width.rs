use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(mode: &str, source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([mode, "/dev/stdin"])
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

// The reviewed rule's GCC-14/Clang-19 O0/O2 probe is
// .evidence/builtin-probes/20260909-constant-contexts/constants.c.
// This RED pins the approved values while the body revision is pending.
#[test]
fn constant_arithmetic_preserves_the_source_integer_width() {
    let source = r#"int main(void) {
        int b = -2147483647;
        if (!(b >= (unsigned)(2147483647 + 2))) return 1;
        if (2147483647 + 2 != -2147483647) return 2;
        if ((-2147483647 - 1) - 1 != 2147483647) return 3;
        if (1073741824 * 3 != -1073741824) return 4;
        if (9223372036854775807L + 1L != (-9223372036854775807L - 1L)) return 5;
        if ((unsigned char)255 + (unsigned char)1 != 256) return 6;
        if ((unsigned)2147483647 + 2 != 2147483649U) return 7;
        return 0;
    }"#;
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(mode, source);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn runtime_signed_overflow_keeps_its_obligation() {
    let source = "int main(void){volatile int a=2147483647;return a+2;}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(mode, source);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3004"));
    }
}
