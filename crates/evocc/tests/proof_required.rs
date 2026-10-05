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

const SCOPE: &str = "checked from typed Core source to final assembly text";

/// The record assertions of every accepted compilation. `checked` lists the
/// stages with the status CHECKED. The other stages are NOT_CHECKED.
fn assert_record(record: &str, checked: &[&str]) {
    assert!(record.contains("\"result\": \"CHECKED_TO_FINAL_ASSEMBLY\""));
    assert!(record.contains(&format!("\"scope\": \"{SCOPE}\"")));
    assert!(record.contains("\"all_stages_checked\": true"));
    assert!(record.contains("\"checked\": true"));
    assert!(!record.to_lowercase().contains("verified"), "{record}");
    for trusted in [
        "the Lean kernel",
        "the readers of the input files",
        "the printer of the assembly AST",
        "the AArch64 subset",
        "AbiSourceEntry",
        "only the low w bits of its register or 8-byte stack slot",
        "the assembler, the linker",
    ] {
        assert!(
            record.contains(trusted),
            "the trusted list must name {trusted}"
        );
    }
    assert!(
        !record.contains("zero-extended argument values"),
        "the trusted list must not name the removed argument limit"
    );
    let order = ["V01", "V05", "V02", "V03", "V04", "PIPELINE"]
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
    assert!(row(record, "V03").contains("\"path\": \"inputs/post-ssa-evomir.json\""));
    assert!(row(record, "V03").contains("\"path\": \"inputs/allocation.json\""));
    assert!(row(record, "V03").contains("scope=post-ssa-mir-to-allocated-mir"));
    for path in [
        "inputs/allocation.json",
        "inputs/asm-ast.json",
        "inputs/assembly.s",
    ] {
        assert!(row(record, "V04").contains(&format!("\"path\": \"{path}\"")));
    }
    assert!(row(record, "V04").contains("scope=allocated-mir-to-assembly-text"));
    // The final decision: one pipeline check for each entry, after the stage
    // rows.
    let pipeline = &record[record.find("\"id\": \"PIPELINE\",").unwrap()..];
    assert!(pipeline.contains("\"status\": \"CHECKED\""));
    assert!(pipeline.contains("EvoIR.Pipeline.checkPipelineTexts_sound"));
    assert!(pipeline.contains("PIPELINE_ACCEPTED entry="));
    assert!(pipeline.contains("scope=source-to-assembly-text"));
}

/// The digest chain of the record: each pair of files names one link, in
/// stage order. Both files of a pair must have the same digest. `links` are
/// the links before the post-SSA file. The chain then has three fixed links:
/// the post-SSA file of V02 and V03, the allocation export of V03 and V04, and
/// the text that V04 checked and the published `output.s`.
fn assert_chain(directory: &Path, record: &str, links: &[(&str, &str)]) {
    let mut pairs = links
        .iter()
        .map(|(left, right)| (format!("inputs/{left}"), format!("inputs/{right}")))
        .collect::<Vec<_>>();
    for file in ["post-ssa-evomir.json", "allocation.json"] {
        pairs.push((format!("inputs/{file}"), format!("inputs/{file}")));
    }
    pairs.push(("inputs/assembly.s".into(), "output.s".into()));
    let chain = &record[record.find("\"chain\"").unwrap()..];
    let mut at = 0;
    for (left, right) in &pairs {
        let link = digest(&directory.join(left));
        assert_eq!(link, digest(&directory.join(right)), "{left} and {right}");
        at += chain[at..]
            .find(&link)
            .expect("the chain names each link in order")
            + 1;
    }
    assert!(chain[at..].contains("\"stage\": \"publication\""));
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
    assert_record(&record, &["V01", "V02", "V03", "V04"]);
    // Without -Oeq-scalar, the V05 row says that no rewrite ran, and V01 and
    // V02 read the same EvoIR file.
    assert!(row(&record, "V05").contains("no rewrite ran"));
    assert!(!directory.join("inputs/rewrite-000.proof.evp").exists());
    assert!(!record.contains("rewrite_checker"));
    assert_chain(&directory, &record, &[("evoir.evo", "evoir.evo")]);
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
fn pilot_entries_are_checked_to_final_assembly_text() {
    let (directory, record) = accept(&fixture("musl-abs.c"), &["--core-entry=abs"]);
    assert_eq!(record.matches("\"result\": \"ACCEPTED\"").count(), 5);
    // The allocation export, the AST and the text are the frozen V03 and V04
    // fixtures of this module.
    for (exported, frozen) in [
        ("allocation.json", "lean/EvoIR/Alloc/Fixtures/musl-abs.json"),
        (
            "asm-ast.json",
            "tests/fixtures/verification/asm/musl-abs.ast.json",
        ),
        ("assembly.s", "tests/fixtures/verification/asm/musl-abs.s"),
    ] {
        assert_eq!(
            std::fs::read(directory.join("inputs").join(exported)).unwrap(),
            std::fs::read(repository().join(frozen)).unwrap(),
            "{exported}"
        );
    }
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
    assert_eq!(record.matches("\"result\": \"ACCEPTED\"").count(), 10);
    drop(directory);
    let (_, record) = accept(
        &fixture("pressure.c"),
        &["--core-entry=pressure", "--core-entry=wide"],
    );
    assert!(record.contains("entry=pressure route=call"));
    assert!(record.contains("stage=V03 entry=wide functions=2"));
    assert!(record.contains("stage=V04 entry=wide functions=2"));
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
    assert_record(&record, &["V01", "V05", "V02", "V03", "V04"]);
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
    let pairs = files
        .chunks(2)
        .map(|pair| (pair[0].as_str(), pair[1].as_str()))
        .collect::<Vec<_>>();
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

/// A test checker. For each binding of a role in the space-separated list
/// `EVOCC_TEST_ROLE`, it applies the sed program `EVOCC_TEST_SED` to the proof
/// input. Then it runs the real Lean checker.
const CHANGING_ROLE_CHECKER: &str = r#"#!/bin/sh
if [ -n "$EVOCC_TEST_COMMAND" ] && [ "$1" != "$EVOCC_TEST_COMMAND" ]; then
    exec "$EVOCC_TEST_LEAN_CHECKER" "$@"
fi
for binding in "$@"; do
    role=${binding%%=*}
    case " $EVOCC_TEST_ROLE " in
        *" $role "*)
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
    run_changed_in("", directory, extra, source, role, sed)
}

/// As `run_changed`, but if `command` is not empty, the change occurs only in
/// the checker runs with that first argument.
fn run_changed_in(
    command: &str,
    directory: &Path,
    extra: &[&str],
    source: &Path,
    role: &str,
    sed: &str,
) -> Output {
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
            ("EVOCC_TEST_COMMAND", Path::new(command)),
        ],
    )
}

#[test]
fn the_pipeline_check_is_the_final_decision() {
    // Every stage row accepts the original files. Then the text changes, and
    // only the pipeline check reads the changed text. It refuses, and the
    // driver publishes nothing.
    let directory = new_directory();
    let output = run_changed_in(
        "--check-pipeline",
        &directory,
        &["--core-entry=abs"],
        &fixture("musl-abs.c"),
        "text",
        "s/cmp w0, #0/cmp w0, #1/",
    );
    assert_refused(&output, &directory, REJECTED, "stage PIPELINE");
    assert_refused(&output, &directory, REJECTED, "PIPELINE_REJECTED stage=V04");
    let accepted = std::fs::read_to_string(directory.join("checks/V04-abs.out")).unwrap();
    assert!(accepted.starts_with("STAGE_ACCEPTED stage=V04 entry=abs"));
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
fn changed_assembly_inputs_are_rejected() {
    let read = |directory: &Path, file: &str| {
        std::fs::read_to_string(directory.join("inputs").join(file)).unwrap()
    };

    // One changed instruction in the text that the driver would publish.
    let directory = new_directory();
    let output = run_changed(
        &directory,
        &["--core-entry=abs"],
        &fixture("musl-abs.c"),
        "text",
        "s/cmp w0, #0/cmp w0, #1/",
    );
    assert_refused(&output, &directory, REJECTED, "stage V04");
    assert_refused(
        &output,
        &directory,
        REJECTED,
        "the printed AST is not the assembly text at line 11",
    );
    assert!(read(&directory, "assembly.s").contains("cmp w0, #1"));
    drop(directory);

    // The prologue of `wide` saves x19 and x20 in exchanged slots, and the
    // AST and the text agree. The print check passes; checkAsm refuses.
    // The second change puts x19 at a wrong stack offset.
    let pressure = ["--core-entry=pressure", "--core-entry=wide"];
    for (sed, changed) in [
        (
            r#"1s/\["str","x",19,"sp",56],\["str","x",20,"sp",64]/["str","x",20,"sp",56],["str","x",19,"sp",64]/;11s/x19/x20/;12s/x20/x19/"#,
            r#"["str","x",20,"sp",56],["str","x",19,"sp",64]"#,
        ),
        (
            r#"1s/\["str","x",19,"sp",56]/["str","x",19,"sp",48]/;11s/#56/#48/"#,
            r#"["str","x",19,"sp",48]"#,
        ),
    ] {
        let directory = new_directory();
        let output = run_changed(
            &directory,
            &pressure,
            &fixture("pressure.c"),
            "asm text",
            sed,
        );
        assert_refused(&output, &directory, REJECTED, "stage V04");
        assert_refused(
            &output,
            &directory,
            REJECTED,
            "frame layout or emission not confirmed",
        );
        assert!(read(&directory, "asm-ast.json").contains(changed));
    }
}

#[test]
fn private_frame_objects_are_refused() {
    // A source that keeps a private frame object is outside core-v1. The
    // driver refuses it before any stage runs.
    let source =
        source_file("unsigned f(unsigned x) { unsigned a = x; unsigned *p = &a; return *p; }");
    let directory = new_directory();
    let output = run_proof(
        &directory,
        &lean_checker(),
        &["--core-entry=f"],
        &source,
        &[],
    );
    assert_refused(&output, &directory, UNSUPPORTED, "outside core-v1");
    assert!(!directory.exists());
    std::fs::remove_file(source).unwrap();

    // V04 itself refuses the frozen private frame objects of `machine-private`:
    // the printed AST is the text, but checkAsm does not accept the frame.
    let scratch = new_directory();
    std::fs::create_dir(&*scratch).unwrap();
    let entry = scratch.join("source.json");
    let exported = evocc(
        &[
            "--emit=core-c-hir",
            "--core-entry=private_word",
            "-O0",
            "-fno-builtin",
        ],
        &fixture("machine-private.c"),
        &[],
    );
    assert!(exported.status.success());
    std::fs::write(&entry, exported.stdout).unwrap();
    let frozen = fixture("asm");
    let binding = |role: &str, path: &Path| format!("{role}={}", path.display());
    let output = Command::new(lean_checker())
        .args(["--check-stage", "V04"])
        .arg(binding("source", &entry))
        .arg(binding(
            "allocation",
            &frozen.join("machine-private.alloc.json"),
        ))
        .arg(binding("asm", &frozen.join("machine-private.ast.json")))
        .arg(binding("text", &frozen.join("machine-private.s")))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "STAGE_REJECTED stage=V04 entry=private_word frame layout or emission not confirmed"
    ));
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
