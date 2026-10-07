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
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

// The reviewed rule's GCC-14/Clang-19 O0/O2 probe is
// .evidence/builtin-probes/20260909-constant-contexts/constants.c.
// Additional boundaries and surrounding effects are in the shared fixture.
#[test]
fn constant_arithmetic_preserves_the_source_integer_width() {
    let source = include_str!("../../../tests/fixtures/constant-source-width.c");
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
    for source in [
        "int main(void){volatile int a=2147483647;return a+2;}",
        "int main(void){int a=2147483647;return a+2;}",
        "int main(void){int a=-1;return a+(-2147483647-1);}",
        "int main(void){int a=-2147483647-1;return -a;}",
        "int main(void){int a=1073741824;return a*3;}",
        "int add(int a){return a+2;} int main(void){return add(2147483647);}",
        "int main(void){return ((-2147483647-1)/-1)+(-2147483647-1);}",
        "int main(void){if (((-2147483647-1)/-1)<0) return 1; return 0;}",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(mode, source);
            assert!(!output.status.success(), "{mode}: {source}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3004"),
                "{mode}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn invalid_constant_operations_are_not_replaced_with_values() {
    for (expression, diagnostic) in [("1/0", "E3002"), ("1%0", "E3003"), ("1<<32", "E3005")] {
        let source = format!("int main(void){{return {expression};}}");
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(mode, &source);
            assert!(!output.status.success(), "{mode}: {source}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(diagnostic),
                "{mode}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
