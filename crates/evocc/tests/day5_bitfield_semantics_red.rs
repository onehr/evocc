use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SOURCE: &str = "struct S { unsigned int x : 33; };\n";

fn run_parse_only() -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg("--error-format=short")
        .args([
            "--target=aarch64-unknown-linux-gnu",
            "--parse-only",
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evocc");

    let _ = child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(SOURCE.as_bytes());

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait().expect("poll evocc") {
            Some(_) => return child.wait_with_output().expect("collect evocc output"),
            None if Instant::now() >= deadline => {
                child.kill().expect("kill timed-out evocc");
                return child.wait_with_output().expect("collect timed-out evocc");
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

#[test]
fn overwide_unsigned_int_bitfield_is_rejected_at_its_width() {
    let output = run_parse_only();
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");

    assert_eq!(output.status.code(), Some(1), "stderr={stderr:?}");
    assert_eq!(stdout, "");
    assert_eq!(
        stderr,
        "/dev/stdin:1:29: error[E1301]: bit-field-width-exceeds-type: 33 > 32\n"
    );
    assert!(!stderr.contains("panicked at"), "panic leaked: {stderr}");
    assert!(
        !stderr.contains("stack backtrace"),
        "backtrace leaked: {stderr}"
    );
}
