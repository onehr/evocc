//! `#warning` levels and the `-pedantic-errors` driver refusal.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const STANDARDS: [&str; 2] = ["gnu17", "c99"];
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

const WARNING: &str = "#warning \"text\"\nint main(void) { return 0; }\n";
const CLEAN: &str = "int main(void) { return 0; }\n";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn command(target: &str, standard: &str) -> Command {
    let mut command = Command::new(evocc());
    command.args(["--target", target, &format!("-std={standard}")]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command
}

fn run(source: &str, target: &str, standard: &str, args: &[&str]) -> Output {
    let mut command = command(target, standard);
    command.args(args);
    command
        .args(["--parse-only", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn has_code(output: &Output, code: &str) -> bool {
    String::from_utf8_lossy(&output.stderr).contains(&format!("[{code}]"))
}

fn temp_source(source: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "werror-warning-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("source.c");
    std::fs::write(&path, source).unwrap();
    path
}

#[test]
fn warning_levels_match_the_gcc_matrix() {
    for target in TARGETS {
        for standard in STANDARDS {
            let plain = run(WARNING, target, standard, &[]);
            assert!(
                plain.status.success(),
                "{target} {standard}\n{}",
                text(&plain)
            );
            assert!(
                has_code(&plain, "W0010"),
                "{target} {standard}\n{}",
                text(&plain)
            );

            let error = run(WARNING, target, standard, &["-Werror"]);
            assert!(
                !error.status.success(),
                "{target} {standard}\n{}",
                text(&error)
            );
            assert!(
                has_code(&error, "W0010"),
                "{target} {standard}\n{}",
                text(&error)
            );

            let cpp = run(WARNING, target, standard, &["-Werror=cpp"]);
            assert!(!cpp.status.success(), "{target} {standard}\n{}", text(&cpp));
            assert!(
                has_code(&cpp, "W0010"),
                "{target} {standard}\n{}",
                text(&cpp)
            );

            let no_cpp = run(WARNING, target, standard, &["-Wno-cpp"]);
            assert!(
                no_cpp.status.success(),
                "{target} {standard}\n{}",
                text(&no_cpp)
            );
            assert!(!has_code(&no_cpp, "W0010"));

            let quiet = run(WARNING, target, standard, &["-w"]);
            assert!(
                quiet.status.success(),
                "{target} {standard}\n{}",
                text(&quiet)
            );
            assert!(!has_code(&quiet, "W0010"));

            let quiet_error = run(WARNING, target, standard, &["-w", "-Werror"]);
            assert!(
                quiet_error.status.success(),
                "{target} {standard}\n{}",
                text(&quiet_error)
            );
            assert!(!has_code(&quiet_error, "W0010"));

            let error_quiet = run(WARNING, target, standard, &["-Werror", "-w"]);
            assert!(
                error_quiet.status.success(),
                "{target} {standard}\n{}",
                text(&error_quiet)
            );
            assert!(!has_code(&error_quiet, "W0010"));

            let no_error = run(WARNING, target, standard, &["-Wno-error"]);
            assert!(
                no_error.status.success(),
                "{target} {standard}\n{}",
                text(&no_error)
            );
            assert!(has_code(&no_error, "W0010"));

            let suppress_cpp_error = run(WARNING, target, standard, &["-Werror", "-Wno-error=cpp"]);
            assert!(
                suppress_cpp_error.status.success(),
                "{target} {standard}\n{}",
                text(&suppress_cpp_error)
            );
            assert!(has_code(&suppress_cpp_error, "W0010"));

            let clean = run(CLEAN, target, standard, &["-Werror"]);
            assert!(
                clean.status.success(),
                "{target} {standard}\n{}",
                text(&clean)
            );
            assert!(!has_code(&clean, "W0010"));
        }
    }
}

#[test]
fn warning_error_writes_no_output_file() {
    let source = temp_source(WARNING);
    let output = command(DARWIN, "gnu17")
        .args(["-Werror", "-S", "-o", "/dev/null"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(!output.status.success(), "{}", text(&output));
    assert!(has_code(&output, "W0010"), "{}", text(&output));
}

#[test]
fn pedantic_errors_is_a_named_refusal() {
    let source = "int main(void) { return ({1;}); }\n";
    for target in TARGETS {
        for option in ["-pedantic-errors", "--pedantic-errors", "-Werror=pedantic"] {
            let refused = run(source, target, "gnu17", &[option]);
            assert!(
                !refused.status.success(),
                "{target} {option}\n{}",
                text(&refused)
            );
            assert!(
                has_code(&refused, "E2968"),
                "{target} {option}\n{}",
                text(&refused)
            );
        }

        for option in ["-pedantic", "-Wpedantic"] {
            let accepted = run(CLEAN, target, "gnu17", &[option]);
            assert!(
                accepted.status.success(),
                "{target} {option}\n{}",
                text(&accepted)
            );
        }
    }
}

#[test]
fn no_error_does_not_enable_pointer_arith() {
    let source = "int main(void) { void *v = 0; v = v + 1; return 0; }\n";
    for target in TARGETS {
        let output = run(source, target, "gnu17", &["-Wno-error=pointer-arith"]);
        assert!(output.status.success(), "{target}\n{}", text(&output));
        assert!(!has_code(&output, "W0007"), "{target}\n{}", text(&output));
    }
}
