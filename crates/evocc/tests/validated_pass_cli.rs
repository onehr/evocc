//! N02's public opt-in path must not change default code or claim a proof.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

fn run(flags: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(flags)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

struct Report(PathBuf);
impl Report {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "evocc-n02-report-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}
impl Drop for Report {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

const SOURCE: &str = "static int twice(int x){return x+x;}int main(void){return twice(7);}";

#[test]
fn no_pass_and_identity_preserve_existing_ir_and_assembly_bytes() {
    for mode in ["--emit=evoir", "--emit=asm"] {
        let default = run(&[mode, "-O2"], SOURCE);
        assert!(
            default.status.success(),
            "{}",
            String::from_utf8_lossy(&default.stderr)
        );
        let identity = run(&[mode, "-O2", "--opt-pass=identity"], SOURCE);
        assert!(
            identity.status.success(),
            "{}",
            String::from_utf8_lossy(&identity.stderr)
        );
        assert_eq!(default.stdout, identity.stdout);
    }
}

#[test]
fn pass_report_contains_real_execution_steps_and_no_certificate_claim() {
    let path = Report::new();
    let out = run(
        &[
            "--emit=evoir",
            "--opt-pass=identity",
            "--pass-report",
            path.path(),
        ],
        SOURCE,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = std::fs::read_to_string(&path.0).unwrap();
    assert!(json.contains("\"schema\":\"evocc-pass-report-v1\""));
    assert!(json.contains("\"class\":\"VALIDATED\""));
    assert!(json.contains("\"outcome\":\"DEFINED_MATCH\""));
    assert!(!json.contains("CHANGED_VERIFIED"));
    assert!(!json.contains("\"before_steps\":null"));
    let check = Command::new("python3").args(["-c", "import json,sys;x=json.load(open(sys.argv[1]));r=x['passes'][0]['executions'][0];assert r['before_steps']>0 and r['before_steps']==r['after_steps'];assert x['native_validation']=='NOT_RUN'", path.path()]).status().unwrap();
    assert!(check.success());
}

#[test]
fn report_only_keeps_empty_pipeline_and_does_not_execute() {
    let report = Report::new();
    let source = "int library(int x){return x*3;}";
    let default = run(&["--emit=asm"], source);
    let recorded = run(&["--emit=asm", "--pass-report", report.path()], source);
    assert!(
        recorded.status.success(),
        "{}",
        String::from_utf8_lossy(&recorded.stderr)
    );
    assert_eq!(default.stdout, recorded.stdout);
    assert!(std::fs::read_to_string(&report.0)
        .unwrap()
        .contains("\"passes\":[]"));
}

#[test]
fn invalid_or_opaque_requests_fail_before_publishing() {
    for (flags, source) in [
        (vec!["--emit=evoir", "--opt-pass=unknown"], SOURCE),
        (vec!["--parse-only", "--opt-pass=identity"], SOURCE),
        (
            vec!["--emit=evoir", "--opt-pass=identity"],
            "int main(int argc,char **argv){return argc;}",
        ),
        (
            vec!["--emit=evoir", "--opt-pass=identity"],
            "volatile int x;int main(void){return x;}",
        ),
    ] {
        let out = run(&flags, source);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn occupied_report_path_is_not_overwritten() {
    let report = Report::new();
    std::fs::write(&report.0, b"retained evidence").unwrap();
    let output = run(
        &[
            "--emit=evoir",
            "--opt-pass=identity",
            "--pass-report",
            report.path(),
        ],
        SOURCE,
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(&report.0).unwrap(), b"retained evidence");
}
