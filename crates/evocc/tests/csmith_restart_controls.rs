use std::io::Write;
use std::process::{Command, Stdio};

const SOURCE: &str = include_str!("../../../tests/fixtures/csmith-finite-inner-restarts.c");
const CALLERS: &str = include_str!("../../../tests/fixtures/csmith-caller-restarts.c");

// These bounded controls exercise the source transitions behind unusable
// reference runs. Passing them does not supply an original Csmith checksum.
#[test]
fn reached_restart_transitions_preserve_widths_aliases_and_guards() {
    check(SOURCE);
}

#[test]
fn finite_callees_preserve_their_callers_restart_guards() {
    check(CALLERS);
}

fn check(source: &str) {
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args([
                    "--target=aarch64-unknown-linux-gnu",
                    level,
                    mode,
                    "/dev/stdin",
                ])
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
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{level} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"0\n", "{level} {mode}");
        }
    }
}
