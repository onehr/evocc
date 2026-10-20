//! Opaque variadic cursors use the same target layout as their C storage.
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, mode: &str, target: &str, level: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([mode, &format!("--target={target}"), level, "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn both_interpreters_copy_adjacent_target_sized_lists() {
    let source = include_str!("../../../tests/fixtures/variadic-list-storage.c");
    for target in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
        for mode in ["--interpret-evoir", "--interpret-c-hir"] {
            for level in ["-O0", "-O2"] {
                let output = run(source, mode, target, level);
                assert!(
                    output.status.success(),
                    "{target} {mode} {level}: {output:?}"
                );
                assert_eq!(output.stdout, b"0\n", "{target} {mode} {level}: {output:?}");
            }
        }
    }
}

#[test]
fn ended_exhausted_and_mismatched_lists_are_still_findings() {
    for body in [
        "__builtin_va_end(ap); return __builtin_va_arg(ap,int);",
        "int a=__builtin_va_arg(ap,int); return a+__builtin_va_arg(ap,int);",
        "return (int)__builtin_va_arg(ap,long);",
    ] {
        let source = format!("int f(int n,...){{__builtin_va_list ap; __builtin_va_start(ap,n); {body}}} int main(void){{return f(1,7);}}");
        for target in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
            for mode in ["--check", "--interpret-c-hir"] {
                for level in ["-O0", "-O2"] {
                    let output = run(&source, mode, target, level);
                    assert!(
                        !output.status.success(),
                        "{target} {mode} {level}: {output:?}"
                    );
                    assert!(
                        String::from_utf8_lossy(&output.stderr).contains("E3038"),
                        "{target} {mode} {level}: {output:?}"
                    );
                }
            }
        }
    }
}
