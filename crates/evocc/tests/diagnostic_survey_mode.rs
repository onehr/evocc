//! `--survey` reports every diagnostic; the default reports the first.

use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn evocc(args: &[&str], source: &str) -> (i32, String) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("evocc-survey-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("in-{}.c", NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::File::create(&path)
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .arg(&path)
        .output()
        .expect("evocc runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// File-scope findings only: recovery is sound across external declarations
// but not yet after a declaration fails inside a function body.
const THREE: &str = "int a[-1];\nint b[-2];\nstruct s { int c[-3]; };\n";

#[test]
fn the_default_reports_one_diagnostic() {
    let (code, stderr) = evocc(&["--parse-only"], THREE);
    assert_ne!(code, 0);
    assert_eq!(
        stderr.lines().filter(|l| l.contains("error[")).count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("E2923"), "{stderr}");
}

#[test]
fn survey_reports_every_diagnostic() {
    let (code, stderr) = evocc(&["--parse-only", "--survey"], THREE);
    assert_ne!(code, 0);
    let errors: Vec<&str> = stderr.lines().filter(|l| l.contains("error[")).collect();
    assert!(errors.len() >= 3, "expected every finding, got {errors:?}");
    assert!(errors[0].contains("E2923"), "{stderr}");
    assert!(errors.iter().all(|l| l.contains("E2923")), "{stderr}");
}

#[test]
fn survey_recovers_after_a_failed_function_scope() {
    let source = "int a[-1];\nvoid f(void) { goto nowhere; }\nint b[-2];\n";
    let (code, stderr) = evocc(&["--parse-only", "--survey"], source);
    assert_eq!(code, 1, "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    let errors: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("error["))
        .collect();
    assert_eq!(errors.len(), 3, "{stderr}");
    assert!(errors[0].contains("E2923"), "{stderr}");
    assert!(errors[1].contains("E2920"), "{stderr}");
    assert!(errors[2].contains("E2923"), "{stderr}");
}

#[test]
fn survey_of_a_clean_unit_succeeds_silently() {
    let (code, stderr) = evocc(&["--parse-only", "--survey"], "int a = 1;\n");
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stderr.lines().filter(|l| l.contains("error[")).count(), 0);
}
