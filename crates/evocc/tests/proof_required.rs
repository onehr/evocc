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

/// The text of one stage row of the record, up to the next stage row.
fn row<'a>(record: &'a str, stage: &str) -> &'a str {
    let at = record.find(&format!("\"id\": \"{stage}\",")).unwrap();
    let end = record[at + 1..]
        .find("\"id\": \"V")
        .map_or(record.len(), |next| at + 1 + next);
    &record[at..end]
}

fn digest(path: &Path) -> String {
    evocc_proof::sha256_hex(&std::fs::read(path).unwrap())
}

const SCOPE: &str = "checked to allocated EvoMIR, not to final assembly";

/// The record assertions of every accepted compilation. `checked` lists the
/// stages with the status CHECKED. The other stages are NOT_CHECKED.
fn assert_record(record: &str, checked: &[&str]) {
    assert!(record.contains("\"result\": \"CHECKED_TO_ALLOCATED_EVOMIR\""));
    assert!(record.contains(&format!("\"scope\": \"{SCOPE}\"")));
    assert!(record.contains("\"all_stages_checked\": false"));
    assert!(!record.to_lowercase().contains("verified"), "{record}");
    let order = ["V01", "V05", "V02", "V03", "V04"]
        .map(|stage| record.find(&format!("\"id\": \"{stage}\",")).unwrap());
    assert!(order.is_sorted(), "stage order {order:?}");
    for stage in ["V01", "V05", "V02", "V03", "V04"] {
        let status = if checked.contains(&stage) {
            "CHECKED"
        } else {
            "NOT_CHECKED"
        };
        let expected = format!("\"status\": \"{status}\"");
        assert!(row(record, stage).contains(&expected), "{stage}: {status}");
    }
    assert!(row(record, "V04").contains("\"reason\": \"not integrated\""));
    assert!(row(record, "V03").contains("\"path\": \"inputs/post-ssa-evomir.json\""));
    assert!(row(record, "V03").contains("\"path\": \"inputs/allocation.json\""));
    assert!(row(record, "V03").contains("scope=post-ssa-mir-to-allocated-mir"));
}

/// The digest chain of the record: each pair of files names one link, in
/// stage order. Both files of a pair must have the same digest.
fn assert_chain(directory: &Path, record: &str, pairs: &[(&str, &str)]) {
    let inputs = directory.join("inputs");
    let chain = &record[record.find("\"chain\"").unwrap()..];
    let mut at = 0;
    for (left, right) in pairs {
        let link = digest(&inputs.join(left));
        assert_eq!(link, digest(&inputs.join(right)), "{left} and {right}");
        at += chain[at..]
            .find(&link)
            .expect("the chain names each link in order")
            + 1;
    }
    assert!(chain[at..].contains("\"stage\": \"V03\""));
    assert_eq!(chain.matches("\"read_by\"").count(), pairs.len());
}

fn accept(source: &Path, extra: &[&str]) -> (Scratch, String) {
    let directory = new_directory();
    let output = run_proof(&directory, &lean_checker(), extra, source, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains(SCOPE));

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
    assert_record(&record, &["V01", "V02", "V03"]);
    // Without -Oeq-scalar, the V05 row says that no rewrite ran, and V01 and
    // V02 read the same EvoIR file.
    assert!(row(&record, "V05").contains("no rewrite ran"));
    assert!(!directory.join("inputs/rewrite-000.proof.evp").exists());
    assert!(!record.contains("rewrite_checker"));
    assert_chain(
        &directory,
        &record,
        &[
            ("evoir.evo", "evoir.evo"),
            ("post-ssa-evomir.json", "post-ssa-evomir.json"),
        ],
    );
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
fn pilot_entries_are_checked_to_allocated_evomir() {
    let (directory, record) = accept(&fixture("musl-abs.c"), &["--core-entry=abs"]);
    assert_eq!(record.matches("\"result\": \"ACCEPTED\"").count(), 3);
    // The allocation export is the frozen V03 fixture of this module.
    assert_eq!(
        std::fs::read(directory.join("inputs/allocation.json")).unwrap(),
        std::fs::read(repository().join("lean/EvoIR/Alloc/Fixtures/musl-abs.json")).unwrap()
    );
    drop(directory);
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
    assert_eq!(record.matches("\"result\": \"ACCEPTED\"").count(), 6);
    drop(directory);
    let (_, record) = accept(
        &fixture("pressure.c"),
        &["--core-entry=pressure", "--core-entry=wide"],
    );
    assert!(record.contains("entry=pressure route=call"));
    assert!(record.contains("stage=V03 entry=wide functions=2"));
}

/// `-Oeq-scalar` with the pinned Rust certificate checker of this build.
fn rewrite_arguments(policy: Option<&str>) -> Vec<String> {
    let checker = env!("CARGO_BIN_EXE_evocc-checker");
    let mut arguments = vec![
        "-Oeq-scalar".to_owned(),
        format!("--eq-checker={checker}"),
        format!("--eq-checker-sha256={}", digest(Path::new(checker))),
    ];
    arguments.extend(policy.map(|policy| format!("--eq-policy={policy}")));
    arguments
}

/// Accept an optimized compilation. Check the stage order, the assembly and
/// the digest chain from V01 through each V05 step to V02 and V03.
fn accept_rewrites(source: &Path, entries: &[&str], rewrite: &[String]) -> (Scratch, String) {
    let directory = new_directory();
    let mut extra = entries.to_vec();
    extra.extend(rewrite.iter().map(String::as_str));
    let output = run_proof(&directory, &lean_checker(), &extra, source, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains(SCOPE));

    // The assembly is the assembly of the ordinary optimized compilation.
    let mut ordinary = vec!["--emit=asm", "-O0", "-fno-builtin"];
    ordinary.extend(rewrite.iter().map(String::as_str));
    let ordinary = evocc(&ordinary, source, &[]);
    assert!(ordinary.status.success());
    assert_eq!(output.stdout, ordinary.stdout);
    assert_eq!(
        std::fs::read(directory.join("output.s")).unwrap(),
        ordinary.stdout
    );

    let record = std::fs::read_to_string(directory.join("record.json")).unwrap();
    assert_record(&record, &["V01", "V05", "V02", "V03"]);
    assert!(row(&record, "V01").contains("\"path\": \"inputs/evoir.evo\""));
    assert!(row(&record, "V02").contains("\"path\": \"inputs/evoir-rewritten.evo\""));
    // The record names the Rust certificate checker that accepted each step.
    let rust_checker = digest(Path::new(env!("CARGO_BIN_EXE_evocc-checker")));
    let at = record.find("\"rewrite_checker\"").unwrap();
    assert!(record[at..].starts_with(&format!(
        "\"rewrite_checker\": {{\n    \"path\": \"{}\",\n    \"sha256\": \"{rust_checker}\"",
        env!("CARGO_BIN_EXE_evocc-checker")
    )));

    // Adjacent stages read the same bytes, and the record names each link in
    // stage order.
    let inputs = directory.join("inputs");
    let steps = (0..)
        .take_while(|step| {
            inputs
                .join(format!("rewrite-{step:03}.proof.evp"))
                .is_file()
        })
        .count();
    assert!(steps >= 1);
    let mut files = vec!["evoir.evo".to_owned()];
    for step in 0..steps {
        files.push(format!("rewrite-{step:03}.before.evo"));
        files.push(format!("rewrite-{step:03}.after.evo"));
    }
    files.push("evoir-rewritten.evo".to_owned());
    let mut pairs = files
        .chunks(2)
        .map(|pair| (pair[0].as_str(), pair[1].as_str()))
        .collect::<Vec<_>>();
    pairs.push(("post-ssa-evomir.json", "post-ssa-evomir.json"));
    assert_chain(&directory, &record, &pairs);
    for input in std::fs::read_dir(&inputs).unwrap() {
        let digest = digest(&input.unwrap().path());
        assert!(
            record.contains(&digest),
            "record must name input digest {digest}"
        );
    }
    (directory, record)
}

#[test]
fn rewrites_are_checked_between_v01_and_v02() {
    let fixtures = fixture("rewrite");
    let same = |directory: &Path, name: &str| {
        for (exported, frozen) in [
            ("rewrite-000.before.evo", "before.evo"),
            ("rewrite-000.after.evo", "after.evo"),
            ("rewrite-000.proof.evp", "proof.evp"),
        ] {
            assert_eq!(
                std::fs::read(directory.join("inputs").join(exported)).unwrap(),
                std::fs::read(fixtures.join(format!("{name}.{frozen}"))).unwrap(),
                "{name}.{frozen}"
            );
        }
    };

    // g0: the FixedMul shift count guard fold.
    let (directory, record) = accept_rewrites(
        &fixture("doom-fixedmul.c"),
        &["--core-entry=FixedMul"],
        &rewrite_arguments(Some("guard-known-true")),
    );
    same(&directory, "fixedmul-g0");
    assert!(record.contains(
        "stage=V05 entry=FixedMul registry=g0.ub.v1 regions=FixedMul/entry/6 rules=g0_007_signed_count"
    ));
    let certificate = digest(&directory.join("inputs/rewrite-000.proof.evp"));
    assert!(row(&record, "V05").contains(&certificate));
    drop(directory);

    // p0: the short_circuit comparison folds.
    let proposal = format!(
        "--proof-proposal=loop_join={}",
        fixture("loop-join.cutpoints.json").display()
    );
    let (directory, record) = accept_rewrites(
        &fixture("control.c"),
        &[
            "--core-entry=loop_join",
            "--core-entry=short_circuit",
            &proposal,
        ],
        &rewrite_arguments(None),
    );
    same(&directory, "control-p0");
    assert!(record.contains(
        "stage=V05 entry=short_circuit registry=p0.scalar.v1 regions=short_circuit/b1/2,short_circuit/b7/2 rules=p0_107_fold_icmp_ne"
    ));
    assert!(record.contains("entry=loop_join route=loop"));
}

/// A test checker. For each binding of the role `EVOCC_TEST_ROLE`, it applies
/// the sed program `EVOCC_TEST_SED` to the proof input. Then it runs the real
/// Lean checker.
const CHANGING_ROLE_CHECKER: &str = r#"#!/bin/sh
for binding in "$@"; do
    case "$binding" in
        "$EVOCC_TEST_ROLE"=*)
            file=${binding#*=}
            sed -e "$EVOCC_TEST_SED" "$file" > "$file.changed" && mv "$file.changed" "$file"
            ;;
    esac
done
exec "$EVOCC_TEST_LEAN_CHECKER" "$@"
"#;

/// Run proof-required mode with `CHANGING_ROLE_CHECKER`: the proof input of
/// `role` changes after export and before the Lean checker reads it.
fn run_changed(directory: &Path, extra: &[&str], source: &Path, role: &str, sed: &str) -> Output {
    use std::os::unix::fs::PermissionsExt;
    let tools = new_directory();
    std::fs::create_dir(&*tools).unwrap();
    let script = tools.join("changing-role-checker.sh");
    std::fs::write(&script, CHANGING_ROLE_CHECKER).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    run_proof(
        directory,
        &script,
        extra,
        source,
        &[
            ("EVOCC_TEST_LEAN_CHECKER", &lean_checker()),
            ("EVOCC_TEST_ROLE", Path::new(role)),
            ("EVOCC_TEST_SED", Path::new(sed)),
        ],
    )
}

#[test]
fn changed_allocation_exports_are_rejected() {
    let read = |directory: &Path| {
        std::fs::read_to_string(directory.join("inputs/allocation.json")).unwrap()
    };

    // Two exchanged registers: `subs` computes x - 0 instead of 0 - x.
    let directory = new_directory();
    let output = run_changed(
        &directory,
        &["--core-entry=abs"],
        &fixture("musl-abs.c"),
        "allocation",
        r#"s/\["gpr",2,"w"],\["gpr",0,"w"],"w"]/["gpr",0,"w"],["gpr",2,"w"],"w"]/"#,
    );
    assert_refused(&output, &directory, REJECTED, "stage V03");
    assert_refused(&output, &directory, REJECTED, "allocation not confirmed");
    assert!(read(&directory).contains(r#"["flags"],["gpr",0,"w"],["gpr",2,"w"],"w"]"#));
    drop(directory);

    // A reload from a wrong spill slot before an outgoing stack argument.
    let directory = new_directory();
    let output = run_changed(
        &directory,
        &["--core-entry=pressure", "--core-entry=wide"],
        &fixture("pressure.c"),
        "allocation",
        r#"s/\["spill_reload",\["gpr",8,"x"],7]/["spill_reload",["gpr",8,"x"],0]/"#,
    );
    assert_refused(&output, &directory, REJECTED, "stage V03");
    assert_refused(
        &output,
        &directory,
        REJECTED,
        "allocation not confirmed in function pressure",
    );
    assert!(!read(&directory).contains(r#"["spill_reload",["gpr",8,"x"],7]"#));
    drop(directory);

    // The program before allocation in the export is not the post-SSA file
    // that V02 accepted: the comparison constant changes from 0 to 1.
    let directory = new_directory();
    let output = run_changed(
        &directory,
        &["--core-entry=abs"],
        &fixture("musl-abs.c"),
        "allocation",
        r#"s/\[0,"gpr32"],0,"w"]/[0,"gpr32"],1,"w"]/"#,
    );
    assert_refused(&output, &directory, REJECTED, "stage V03");
    assert_refused(&output, &directory, REJECTED, "is not the post-SSA input");
    assert!(read(&directory).contains(r#"[0,"gpr32"],1,"w"]"#));
    let post_ssa = directory.join("inputs/post-ssa-evomir.json");
    assert!(std::fs::read_to_string(post_ssa)
        .unwrap()
        .contains(r#"[0,"gpr32"],0,"w"]"#));
}

#[test]
fn rejected_rewrites_give_no_output() {
    // The Lean checker does not support the p0 alias form. There is no
    // fallback to the program without the rewrite.
    let directory = new_directory();
    let mut extra = vec!["--core-entry=pressure", "--core-entry=wide"];
    let p0 = rewrite_arguments(None);
    extra.extend(p0.iter().map(String::as_str));
    let output = run_proof(
        &directory,
        &lean_checker(),
        &extra,
        &fixture("pressure.c"),
        &[],
    );
    assert_refused(&output, &directory, REJECTED, "stage V05");
    assert_refused(
        &output,
        &directory,
        REJECTED,
        "regions=pressure/entry/1 rules=p0_001_add_zero_rhs",
    );
    drop(directory);

    // A certificate that changes after export: the premise says that the
    // guard is false.
    let directory = new_directory();
    let mut extra = vec!["--core-entry=FixedMul"];
    let g0 = rewrite_arguments(Some("guard-known-true"));
    extra.extend(g0.iter().map(String::as_str));
    let output = run_changed(
        &directory,
        &extra,
        &fixture("doom-fixedmul.c"),
        "certificate",
        "s/premises c6:1:1/premises c6:1:0/",
    );
    assert_refused(&output, &directory, REJECTED, "stage V05");
    assert_refused(&output, &directory, REJECTED, "for entry `FixedMul` step 0");
    let changed = std::fs::read_to_string(directory.join("inputs/rewrite-000.proof.evp")).unwrap();
    assert!(
        changed.contains("premises c6:1:0"),
        "the test checker must change the certificate"
    );
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
        // -Oeq-scalar requires the pinned Rust certificate checker.
        vec!["--core-entry=abs", "-Oeq-scalar"],
        vec!["--core-entry=abs", "--eq-proof-dir=/tmp/x"],
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
