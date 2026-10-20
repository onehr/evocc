//! A completed EvoIR execution cannot recover object identity after ptr.to_int.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(mode: &str, source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([mode, "--error-format=json", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn completed_checks_disclose_all_three_pointer_relation_limits() {
    for source in [
        "int main(void) { int x[4], y; return &x[3] - &y; }",
        "int main(void) { int a, b; return &a < &b; }",
        "int main(void) { int x[4]; return (int *)((char *)x + 1) - x; }",
    ] {
        let checked = run("--check", source);
        assert_eq!(checked.status.code(), Some(0), "{checked:?}");
        let output = String::from_utf8(checked.stdout).unwrap();
        assert!(
            output.contains("Pointer subtraction and ordering obligations are not checked"),
            "{output}"
        );
        assert!(
            !output.contains("No undefined behavior was found"),
            "{output}"
        );

        // The C-HIR interpreter still has object identity. Its documented
        // refusal is not an EvoIR finding and must not be reported as one.
        let original = run("--interpret-c-hir", source);
        assert_eq!(original.status.code(), Some(1), "{original:?}");
        let errors = String::from_utf8(original.stderr).unwrap();
        assert!(errors.contains("E3007"), "{errors}");
        assert!(errors.contains("pointer subtraction"), "{errors}");
    }
}

#[test]
fn defined_same_object_relations_still_execute() {
    for source in [
        "int main(void) { int x[4]; return (&x[4]-&x[0]) != 4; }",
        "int main(void) { int x[4]; return !(&x[0] < &x[4]); }",
        "struct s { int a, b; }; int main(void) { struct s x; return !(&x.a < &x.b); }",
    ] {
        for mode in ["--check", "--interpret-c-hir"] {
            let output = run(mode, source);
            assert_eq!(output.status.code(), Some(0), "{mode}: {output:?}");
        }
    }
}
