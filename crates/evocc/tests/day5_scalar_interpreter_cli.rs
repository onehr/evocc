mod support;

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const WATCHDOG: Duration = Duration::from_secs(2);

fn run_interpreter(source: &str) -> Output {
    support::warm_evocc();
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--target=aarch64-unknown-linux-gnu",
            "--interpret-c-hir",
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evocc");

    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(source.as_bytes())
        .expect("write source");
    let deadline = Instant::now() + WATCHDOG;
    loop {
        match child.try_wait().expect("poll evocc") {
            Some(_) => return child.wait_with_output().expect("collect evocc output"),
            None if Instant::now() >= deadline => {
                child.kill().expect("kill timed-out evocc");
                let output = child.wait_with_output().expect("collect timed-out evocc");
                panic!(
                    "evocc exceeded the two-second watchdog; stdout={:?} stderr={:?}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn assert_no_panic(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("panicked at"), "panic leaked: {stderr}");
    assert!(
        !stderr.contains("stack backtrace"),
        "backtrace leaked: {stderr}"
    );
}

#[test]
fn scalar_interpreter_cli_executes_main() {
    let output = run_interpreter("int main(void) { return 7; }\n");

    assert_no_panic(&output);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"7\n");
    assert_eq!(output.stderr, b"");
}

#[test]
fn scalar_interpreter_cli_reports_runtime_error() {
    let output =
        run_interpreter("int main(void) { int left = 7; int zero = 0; return left / zero; }\n");

    assert_no_panic(&output);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        output.stderr,
        b"/dev/stdin:1:53: error[E3002]: integer-division-by-zero\n"
    );
}

#[test]
fn scalar_interpreter_cli_prints_the_complete_wide_result() {
    for (source, expected) in [
        (
            "unsigned __int128 main(void){return ~(unsigned __int128)0;}",
            "340282366920938463463374607431768211455\n",
        ),
        (
            "__int128 main(void){return -((__int128)1<<100);}",
            "-1267650600228229401496703205376\n",
        ),
    ] {
        let output = run_interpreter(source);
        assert_no_panic(&output);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
}
