//! Proof-required compilation (V06). The acceptance tests run the compiled Lean
//! checker. Build it with `lake build evoir-sem` in `lean/`, or name it with
//! `EVOCC_PROOF_CHECKER`. Each failure test shows a failure exit code, an
//! empty standard output, and no `output.s` or `record.json`.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const REJECTED: i32 = 3;
const INPUT: i32 = 4;
const CHECKER: i32 = 5;
const UNSUPPORTED: i32 = 6;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    repository().join("tests/fixtures/verification").join(name)
}

fn test_checker(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/proof-required")
        .join(name)
}

fn lean_checker() -> PathBuf {
    let checker = std::env::var_os("EVOCC_PROOF_CHECKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository().join("lean/.lake/build/bin/evoir-sem"));
    assert!(
        checker.is_file(),
        "the compiled Lean checker {} is missing; run `lake build evoir-sem` in lean/",
        checker.display()
    );
    checker
}

/// A new path for a proof directory. The driver creates the directory. The
/// test removes it at the end.
struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn new_directory() -> Scratch {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "evocc-proof-required-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    Scratch(path)
}

fn source_file(text: &str) -> PathBuf {
    let path = new_directory().with_extension("c");
    std::fs::write(&path, text).unwrap();
    path
}

fn evocc(arguments: &[&str], source: &Path, environment: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .args(arguments)
        .arg(source)
        .env_remove("EVOCC_PROOF_CHECKER");
    for (key, value) in environment {
        command.env(key, value);
    }
    command.output().unwrap()
}

/// Proof-required arguments with one or more entries.
fn proof_arguments(directory: &Path, checker: &Path, extra: &[&str]) -> Vec<String> {
    let mut arguments = vec![
        "--emit=asm".to_owned(),
        format!("--proof-required={}", directory.display()),
        format!("--proof-checker={}", checker.display()),
        "-O0".to_owned(),
        "-fno-builtin".to_owned(),
    ];
    arguments.extend(extra.iter().map(|argument| (*argument).to_owned()));
    arguments
}

fn run_proof(
    directory: &Path,
    checker: &Path,
    extra: &[&str],
    source: &Path,
    environment: &[(&str, &Path)],
) -> Output {
    let arguments = proof_arguments(directory, checker, extra);
    let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    evocc(&arguments, source, environment)
}

/// The failure contract: the exit code, no standard output, no output file and
/// no record.
fn assert_refused(output: &Output, directory: &Path, code: i32, reason: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(code), "{stderr}");
    assert!(output.stdout.is_empty(), "standard output must be empty");
    assert!(
        !directory.join("output.s").exists(),
        "output.s must not exist"
    );
    assert!(
        !directory.join("record.json").exists(),
        "record.json must not exist"
    );
    assert!(stderr.contains("no output was written"), "{stderr}");
    assert!(stderr.contains(reason), "expected `{reason}` in: {stderr}");
}

fn accept(source: &Path, extra: &[&str]) -> (Scratch, String) {
    let directory = new_directory();
    let output = run_proof(&directory, &lean_checker(), extra, source, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("checked to post-SSA EvoMIR, not to final assembly"));

    // The proof mode changes no byte of the assembly.
    let ordinary = evocc(&["--emit=asm", "-O0", "-fno-builtin"], source, &[]);
    assert!(ordinary.status.success());
    assert_eq!(output.stdout, ordinary.stdout);
    assert_eq!(
        std::fs::read(directory.join("output.s")).unwrap(),
        ordinary.stdout
    );
    let ordinary = String::from_utf8(ordinary.stdout).unwrap();
    assert!(!ordinary.contains("CHECKED") && !ordinary.to_lowercase().contains("verified"));

    let record = std::fs::read_to_string(directory.join("record.json")).unwrap();
    assert!(record.contains("\"result\": \"CHECKED_TO_POST_SSA_EVOMIR\""));
    assert!(record.contains("\"scope\": \"checked to post-SSA EvoMIR, not to final assembly\""));
    assert!(record.contains("\"all_stages_checked\": false"));
    assert!(!record.to_lowercase().contains("verified"), "{record}");
    for stage in ["V03", "V04", "V05"] {
        let row = format!("\"id\": \"{stage}\",");
        let at = record.find(&row).unwrap();
        assert!(record[at..].contains("\"status\": \"NOT_CHECKED\""));
    }
    assert_eq!(record.matches("\"status\": \"CHECKED\"").count(), 2);
    for digest in [
        evocc_proof::sha256_hex(&ordinary.into_bytes()),
        evocc_proof::sha256_hex(&std::fs::read(lean_checker()).unwrap()),
        evocc_proof::sha256_hex(&std::fs::read(source).unwrap()),
    ] {
        assert!(record.contains(&digest), "record must name digest {digest}");
    }
    for input in std::fs::read_dir(directory.join("inputs")).unwrap() {
        let digest = evocc_proof::sha256_hex(&std::fs::read(input.unwrap().path()).unwrap());
        assert!(
            record.contains(&digest),
            "record must name input digest {digest}"
        );
    }
    (directory, record)
}

#[test]
fn pilot_entries_are_checked_to_post_ssa_evomir() {
    let (_, record) = accept(&fixture("musl-abs.c"), &["--core-entry=abs"]);
    assert_eq!(record.matches("\"result\": \"ACCEPTED\"").count(), 2);
    accept(&fixture("doom-fixedmul.c"), &["--core-entry=FixedMul"]);
    let proposal = format!(
        "--proof-proposal=loop_join={}",
        fixture("loop-join.cutpoints.json").display()
    );
    let (directory, record) = accept(
        &fixture("control.c"),
        &[
            "--core-entry=loop_join",
            "--core-entry=short_circuit",
            &proposal,
        ],
    );
    assert!(directory.join("inputs/proposal-loop_join.json").is_file());
    assert!(record.contains("entry=loop_join route=loop"));
    assert_eq!(record.matches("\"result\": \"ACCEPTED\"").count(), 4);
    let (_, record) = accept(
        &fixture("pressure.c"),
        &["--core-entry=pressure", "--core-entry=wide"],
    );
    assert!(record.contains("entry=pressure route=call"));
}

#[test]
fn changed_evoir_is_rejected_by_the_lean_checker() {
    let directory = new_directory();
    let output = run_proof(
        &directory,
        &test_checker("changing-checker.sh"),
        &["--core-entry=abs"],
        &fixture("musl-abs.c"),
        &[
            ("EVOCC_TEST_LEAN_CHECKER", &lean_checker()),
            ("EVOCC_TEST_ACTION", Path::new("change-ir")),
        ],
    );
    assert_refused(&output, &directory, REJECTED, "stage V01");
    let ir = std::fs::read_to_string(directory.join("inputs/evoir.evo")).unwrap();
    assert!(
        ir.contains("ret %v8"),
        "the test checker must change the EvoIR"
    );
}

#[test]
fn evoir_changed_after_acceptance_is_refused() {
    let directory = new_directory();
    let output = run_proof(
        &directory,
        &test_checker("changing-checker.sh"),
        &["--core-entry=abs"],
        &fixture("musl-abs.c"),
        &[
            ("EVOCC_TEST_LEAN_CHECKER", &lean_checker()),
            ("EVOCC_TEST_ACTION", Path::new("change-ir-after-acceptance")),
        ],
    );
    assert_refused(
        &output,
        &directory,
        INPUT,
        "changed or is missing after the check",
    );
    let accepted = std::fs::read_to_string(directory.join("checks/V02-abs.out")).unwrap();
    assert!(accepted.starts_with("STAGE_ACCEPTED stage=V02 entry=abs"));
}

#[test]
fn missing_proof_input_is_refused() {
    let directory = new_directory();
    let output = run_proof(
        &directory,
        &test_checker("changing-checker.sh"),
        &["--core-entry=abs"],
        &fixture("musl-abs.c"),
        &[
            ("EVOCC_TEST_LEAN_CHECKER", &lean_checker()),
            ("EVOCC_TEST_ACTION", Path::new("remove-ssa")),
        ],
    );
    assert_refused(&output, &directory, INPUT, "not supported or not readable");
    assert!(!directory.join("inputs/ssa-evomir.json").exists());

    let directory = new_directory();
    let output = run_proof(
        &directory,
        &lean_checker(),
        &[
            "--core-entry=loop_join",
            "--core-entry=short_circuit",
            "--proof-proposal=loop_join=/nonexistent/loop.json",
        ],
        &fixture("control.c"),
        &[],
    );
    assert_refused(&output, &directory, INPUT, "is not readable");
    assert!(!directory.exists());

    // Without a proposal, the finite request cannot check the loop.
    let directory = new_directory();
    let output = run_proof(
        &directory,
        &lean_checker(),
        &["--core-entry=loop_join", "--core-entry=short_circuit"],
        &fixture("control.c"),
        &[],
    );
    assert_refused(&output, &directory, REJECTED, "stage V01");
}

#[test]
fn missing_checker_is_refused() {
    let directory = new_directory();
    let output = run_proof(
        &directory,
        Path::new("/nonexistent/evoir-sem"),
        &["--core-entry=abs"],
        &fixture("musl-abs.c"),
        &[],
    );
    assert_refused(&output, &directory, CHECKER, "is not readable");
    assert!(!directory.exists());

    let arguments = [
        "--emit=asm".to_owned(),
        format!("--proof-required={}", directory.display()),
        "--core-entry=abs".to_owned(),
        "-O0".to_owned(),
        "-fno-builtin".to_owned(),
    ];
    let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let output = evocc(&arguments, &fixture("musl-abs.c"), &[]);
    assert_refused(&output, &directory, CHECKER, "no checker");

    let output = run_proof(
        &directory,
        &lean_checker(),
        &[
            "--core-entry=abs",
            &format!("--proof-checker-sha256={}", "0".repeat(64)),
        ],
        &fixture("musl-abs.c"),
        &[],
    );
    assert_refused(&output, &directory, CHECKER, "is not the pinned digest");
}

#[test]
fn checker_time_out_is_refused() {
    let directory = new_directory();
    let start = Instant::now();
    let output = run_proof(
        &directory,
        &test_checker("sleeping-checker.sh"),
        &["--core-entry=abs", "--proof-timeout-ms=300"],
        &fixture("musl-abs.c"),
        &[],
    );
    assert_refused(
        &output,
        &directory,
        CHECKER,
        "timed out after 300 ms in stage V01",
    );
    assert!(start.elapsed() < Duration::from_secs(25));
}

#[test]
fn checker_without_matching_acceptance_is_refused() {
    let directory = new_directory();
    let output = run_proof(
        &directory,
        &test_checker("wrong-entry-checker.sh"),
        &["--core-entry=abs"],
        &fixture("musl-abs.c"),
        &[],
    );
    assert_refused(&output, &directory, CHECKER, "no acceptance line");
}

#[test]
fn source_outside_core_v1_is_refused() {
    let checker = lean_checker();
    for (text, reason) in [
        ("int f(int *p) { return *p; }", "outside core-v1"),
        (
            "int f(int x) { if (0) { asm(\"nop\"); } return x; }",
            "outside core-v1",
        ),
        (
            "int ext(int); int f(int x) { return ext(x); }",
            "outside core-v1",
        ),
        ("int g; int f(int x) { return x; }", "data object"),
        (
            "int f(int x) { return x; } int h(int x) { return x + 1; }",
            "add --core-entry=h",
        ),
    ] {
        let source = source_file(text);
        let directory = new_directory();
        let output = run_proof(&directory, &checker, &["--core-entry=f"], &source, &[]);
        assert_refused(&output, &directory, UNSUPPORTED, reason);
        assert!(
            !directory.exists(),
            "no proof input for an unsupported source"
        );
        std::fs::remove_file(source).unwrap();
    }
}

#[test]
fn proof_options_are_admitted_only_with_the_baseline_path() {
    let source = fixture("musl-abs.c");
    let directory = new_directory();
    let checker = lean_checker();
    for extra in [
        vec!["--core-entry=abs", "-O2"],
        vec!["--core-entry=abs", "-Oeq-scalar"],
        vec!["--core-entry=abs", "--ub-checks=unchecked"],
        vec!["--core-entry=abs", "--frame-opt=paired-saves"],
        vec!["--core-entry=abs", "--proof-proposal=other=/dev/null"],
        vec![],
    ] {
        let output = run_proof(&directory, &checker, &extra, &source, &[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!directory.exists());
    }
    for arguments in [
        vec!["--emit=asm", "--proof-checker=/x", "-O0", "-fno-builtin"],
        vec![
            "--emit=evoir",
            "--proof-required=/tmp/x",
            "--core-entry=abs",
            "-O0",
            "-fno-builtin",
        ],
    ] {
        let output = evocc(&arguments, &source, &[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
    }

    // The proof directory must be new.
    std::fs::create_dir(&*directory).unwrap();
    let output = run_proof(&directory, &checker, &["--core-entry=abs"], &source, &[]);
    assert_refused(&output, &directory, INPUT, "cannot create");
}
