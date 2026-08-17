use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DAY_1_TO_3_CONTROLS: &[(&str, &str)] = &[
    ("object declaration", "int value;"),
    ("expression precedence", "int sink = left + middle * right;"),
    ("function declarator", "int compute(int left, long right);"),
];

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn run_parse_only(source: &str) -> Output {
    let mut child = Command::new(evocc())
        .args(["--parse-only", "/dev/stdin"])
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

    let deadline = Instant::now() + Duration::from_secs(2);
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

fn assert_parse_success(case: &str, source: &str) {
    let output = run_parse_only(source);
    let observed = (
        output.status.code(),
        String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    );
    assert_eq!(
        observed,
        (Some(0), String::new(), String::new()),
        "{case}: --parse-only must accept the translation unit silently"
    );
}

#[test]
fn day_1_to_3_frontend_controls_remain_green() {
    assert_eq!(DAY_1_TO_3_CONTROLS.len(), 3, "control denominator changed");
    for (case, source) in DAY_1_TO_3_CONTROLS {
        assert_parse_success(case, source);
    }
}

#[test]
fn day_4_struct_declaration_is_accepted() {
    assert_parse_success(
        "struct declaration",
        "struct Pair { int left; int right; };",
    );
}
