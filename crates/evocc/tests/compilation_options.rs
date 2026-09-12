use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(arguments: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(arguments)
        .arg("/dev/stdin")
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

fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn zero_initialization_is_explicit_and_runs_at_each_declaration() {
    let source = include_str!("../../../tests/fixtures/automatic-zero.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        assert_eq!(
            successful(run(&[mode, "-ftrivial-auto-var-init=zero"], source)).trim(),
            "0"
        );
    }
    successful(run(&["--emit=asm", "-ftrivial-auto-var-init=zero"], source));
}

#[test]
fn zero_initialization_does_not_erase_poison_without_the_option_or_for_exempt_objects() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        for (flags, source) in [
            (vec![mode], "int main(void) { int x; return x; }"),
            (
                vec![
                    mode,
                    "-ftrivial-auto-var-init=zero",
                    "-ftrivial-auto-var-init=uninitialized",
                ],
                "int main(void) { int x; return x; }",
            ),
            (
                vec![mode, "-ftrivial-auto-var-init=zero"],
                "int main(void) { int x __attribute__((uninitialized)); return x; }",
            ),
        ] {
            let output = run(&flags, source);
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3001"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn hosted_mode_and_automatic_initialization_follow_the_last_option() {
    for (flags, expected) in [
        (vec!["-E", "-ffreestanding"], "0"),
        (vec!["-E", "-ffreestanding", "-fhosted"], "1"),
        (vec!["-E", "-fhosted", "-ffreestanding"], "0"),
    ] {
        assert_eq!(
            successful(run(&flags, "__STDC_HOSTED__\n")).trim(),
            expected
        );
    }
    assert_eq!(
        successful(run(
            &[
                "--interpret-evoir",
                "-ftrivial-auto-var-init=uninitialized",
                "-ftrivial-auto-var-init=zero"
            ],
            "int main(void) { int x; return x; }"
        ))
        .trim(),
        "0"
    );
}

#[test]
fn freestanding_disables_only_ordinary_library_name_substitution() {
    let source = "int abs(int x){return x+100;} int main(void){volatile int x=-7; return abs(x)+__builtin_abs(x);}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        for (options, expected) in [
            (vec![mode, "-ffreestanding"], "100"),
            (vec![mode, "-ffreestanding", "-fbuiltin"], "14"),
            (vec![mode, "-fbuiltin", "-ffreestanding"], "100"),
        ] {
            assert_eq!(successful(run(&options, source)).trim(), expected);
        }
    }
}
