use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn run_parse_only(source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg("--error-format=short")
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
                    "evocc timed out; stdout={:?} stderr={:?}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

#[test]
fn parse_only_accepts_the_frozen_kernel_style_signed_alias() {
    let output = run_parse_only("typedef __signed__ char __s8;\n");
    assert_eq!(
        (
            output.status.code(),
            String::from_utf8(output.stdout).expect("stdout is UTF-8"),
            String::from_utf8(output.stderr).expect("stderr is UTF-8"),
        ),
        (Some(0), String::new(), String::new()),
        "the pre-fix observation was /dev/stdin:1:9: error[E1001]: expected-declaration",
    );
}

#[test]
fn parse_only_rejects_repeated_gnu_signed_alias_consistently() {
    let output = run_parse_only("typedef __signed__ __signed__ int bad;\n");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr is UTF-8"),
        "/dev/stdin:1:9: error[E1007]: invalid-type-specifier-combination\n",
    );
}

#[test]
fn preprocessing_can_expand_and_paste_the_gnu_signed_spelling() {
    let sources = [
        "#define __signed__ unsigned\ntypedef __signed__ int T;\n",
        concat!(
            "#define CAT_RAW(a, b) a ## b\n",
            "typedef CAT_RAW(__signed, __) char T;\n",
        ),
    ];

    for source in sources {
        let output = run_parse_only(source);
        assert_eq!(
            (
                output.status.code(),
                String::from_utf8(output.stdout).expect("stdout is UTF-8"),
                String::from_utf8(output.stderr).expect("stderr is UTF-8"),
            ),
            (Some(0), String::new(), String::new()),
            "source: {source:?}",
        );
    }
}
