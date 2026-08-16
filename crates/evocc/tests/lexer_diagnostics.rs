use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn run_bounded(input: &[u8]) -> (ExitStatus, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(evocc())
        .args(["-E", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("lexer driver timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut stderr)
        .unwrap();
    (status, stdout, stderr)
}

fn assert_rejected(id: &str, input: &[u8], code: &str, line: u32, column: u32) {
    let (status, stdout, stderr) = run_bounded(input);
    assert_eq!(
        status.code(),
        Some(1),
        "{id}: status={status:?} stderr={:?}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(stdout.is_empty(), "{id}: rejection wrote stdout");
    let stderr = String::from_utf8(stderr).unwrap();
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "{id}: diagnostic must be one complete record: {stderr:?}"
    );
    assert!(
        lines[0].starts_with(&format!("/dev/stdin:{line}:{column}: error[{code}]: ")),
        "{id}: unexpected diagnostic: {stderr:?}"
    );
}

#[test]
fn raw_and_unterminated_inputs_fail_with_structured_diagnostics() {
    assert_rejected("invalid-token", b"@", "E0001", 1, 1);
    assert_rejected("invalid-utf8", &[0xff], "E0002", 1, 1);
    assert_rejected("unterminated-string", b"\"unterminated", "E0001", 1, 1);
    assert_rejected("unterminated-char", b"'x", "E0001", 1, 1);
    assert_rejected("unterminated-comment", b"/*", "E0001", 1, 1);
}

#[test]
fn valid_control_is_not_rejected() {
    let (status, _stdout, stderr) = run_bounded(b"int value;\n");
    assert_eq!(
        status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(stderr.is_empty());
}
