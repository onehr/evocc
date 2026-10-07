//! Exercise user commands through their installed process boundary.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "evocc-user-command-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn source(&self, text: &str) -> PathBuf {
        let path = self.0.join("input.c");
        std::fs::write(&path, text).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn evo(args: &[&str]) -> Output {
    let compiler = PathBuf::from(env!("CARGO_BIN_EXE_evocc"));
    Command::new(compiler.with_file_name("evo"))
        .args(args)
        .env("EVOCC", compiler)
        .env(
            "EVOCC_PROOF_CHECKER",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lean/.lake/build/bin/evoir-sem"),
        )
        .output()
        .expect("the evo binary must be built beside evocc")
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn global_help_and_missing_command_share_usage_with_distinct_statuses() {
    let missing = evo(&[]);
    assert_eq!(missing.status.code(), Some(64));
    for flag in ["help", "--help"] {
        let output = evo(&[flag]);
        success(&output);
        assert_eq!(output.stdout, missing.stdout);
        assert!(output.stderr.is_empty());
        help_json(&[flag, "--json"], &output.stdout);
    }
}

fn help_json(args: &[&str], usage: &[u8]) {
    let output = evo(args);
    success(&output);
    let usage = String::from_utf8_lossy(usage);
    let expected = format!("{{\"usage\":\"{}\",\"exit_code\":0}}\n", usage.trim());
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(output.stderr.is_empty());
}

#[test]
fn each_public_verb_has_help() {
    for verb in ["run", "check", "explain", "prove", "build", "version"] {
        let output = evo(&[verb, "--help"]);
        success(&output);
        assert!(String::from_utf8_lossy(&output.stdout).contains(&format!("evo {verb}")));
        help_json(&[verb, "--help", "--json"], &output.stdout);
    }
    assert!(!evo(&["unknown"]).status.success());
}

#[test]
fn run_returns_the_program_status_and_preserves_arguments() {
    let temp = Scratch::new();
    let input = temp.source("int main(void) { return 7; }");
    let output = evo(&["run", input.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let input = temp.source(
        r#"int main(int argc, char **argv) {
        if (argc != 3 || !argv[0] || !argv[0][0] || argv[argc] != 0) return 1;
        if (argv[1][0] != 'a' || argv[1][1] != ' ' || argv[1][2] != 'b') return 2;
        if (argv[2][0] != 0) return 3;
        argv[1][0] = 'z'; return argv[1][0] != 'z';
    }"#,
    );
    success(&evo(&["run", input.to_str().unwrap(), "--", "a b", ""]));
}

#[test]
fn check_reports_undefined_behavior_but_not_a_nonzero_return() {
    let temp = Scratch::new();
    let input = temp.source("int main(void) { return 7; }");
    success(&evo(&["check", input.to_str().unwrap()]));
    temp.source("int main(void) { int zero = 0; return 1 / zero; }");
    let output = evo(&["check", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("error[E3002]"), "{error}");
    assert!(error.contains("-->") && error.contains('^'), "{error}");
}

#[test]
fn explain_prints_the_tested_page() {
    let output = evo(&["explain", "E3002"]);
    success(&output);
    let expected = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--explain", "E3002"])
        .output()
        .unwrap();
    assert_eq!(output.stdout, expected.stdout);
    assert_eq!(evo(&["explain", "E9999"]).status.code(), Some(1));
}

#[test]
fn version_reports_the_underlying_compiler() {
    let output = evo(&["version"]);
    success(&output);
    let expected = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(output.stdout, expected.stdout);
}

#[test]
fn prove_checks_all_definitions_and_keeps_the_record() {
    let temp = Scratch::new();
    let input =
        temp.source("int add(int a, int b) { return a+b; } int main(void) { return add(1,2); }");
    let assembly = temp.0.join("out.s");
    let output = evo(&[
        "prove",
        input.to_str().unwrap(),
        "-o",
        assembly.to_str().unwrap(),
    ]);
    success(&output);
    let text = String::from_utf8(output.stdout).unwrap();
    let record = text
        .trim()
        .strip_prefix("checked from typed Core source to final assembly text: ")
        .expect("one success line must identify the retained record");
    let value = std::fs::read_to_string(record).unwrap();
    assert!(value.contains("CHECKED_TO_FINAL_ASSEMBLY"));
    assert!(value.contains("add") && value.contains("main"));
    assert!(std::fs::metadata(&assembly).unwrap().len() > 0);
    // The record is a product outside our input scratch directory.
    let record_dir = Path::new(record).parent().unwrap().parent().unwrap();
    assert!(record_dir
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("evo-proof-"));
    std::fs::remove_dir_all(record_dir).unwrap();
}

#[test]
fn run_preserves_non_utf8_argument_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let temp = Scratch::new();
    let input = temp.source("int main(int n, char **v) { return n != 2 || (unsigned char)v[1][0] != 255 || v[1][1] != 0; }");
    let compiler = PathBuf::from(env!("CARGO_BIN_EXE_evocc"));
    let output = Command::new(compiler.with_file_name("evo"))
        .arg("run")
        .arg(input)
        .arg("--")
        .arg(std::ffi::OsString::from_vec(vec![255]))
        .env("EVOCC", compiler)
        .output()
        .unwrap();
    success(&output);
}

#[test]
fn prove_keeps_and_names_files_when_output_cannot_be_written() {
    let temp = Scratch::new();
    let input = temp.source("int main(void) { return 0; }");
    let output = evo(&[
        "prove",
        input.to_str().unwrap(),
        "-o",
        temp.0.join("missing/out.s").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let root = stderr
        .lines()
        .find_map(|line| line.strip_prefix("evo: proof files kept at "))
        .unwrap();
    assert!(Path::new(root).join("checked/record.json").is_file());
    assert!(Path::new(root)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("evo-proof-"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_failure_does_not_report_an_expected_configure_probe() {
    use std::os::unix::fs::PermissionsExt;
    let temp = Scratch::new();
    std::fs::write(
        temp.0.join("configure"),
        "#!/bin/sh\nprintf 'not C' > probe.c\neval \"$CC -c probe.c -o probe.o\" >/dev/null 2>&1\nexit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(
        temp.0.join("configure"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    std::fs::write(temp.0.join("Makefile"), "all:\n\tfalse\n").unwrap();
    let output = evo(&["build", temp.0.to_str().unwrap()]);
    assert!(!output.status.success());
    let receipt = std::fs::read_to_string(temp.0.join("evo-build-receipt.json")).unwrap();
    assert!(
        receipt.contains("\"failing_command\":[\"make\","),
        "{receipt}"
    );
    assert!(!receipt.contains("probe.c"), "{receipt}");
}

#[test]
fn build_lists_source_inputs_and_counts_coded_warnings() {
    let temp = Scratch::new();
    std::fs::write(
        temp.0.join("one.c"),
        "int f(void) { int unused; return 0; }\n",
    )
    .unwrap();
    std::fs::write(temp.0.join("two.c"), "int g(void) { return 0; }\n").unwrap();
    std::fs::write(
        temp.0.join("Makefile"),
        "all:\n\t$(CC) -Wunused-variable -c one.c -o not-a-source.c\n\t$(CC) -c two.c -o two.o\n",
    )
    .unwrap();
    let output = evo(&[
        "build",
        temp.0.to_str().unwrap(),
        "--",
        "CC=missing-compiler",
    ]);
    success(&output);
    let receipt = std::fs::read_to_string(temp.0.join("evo-build-receipt.json")).unwrap();
    assert!(
        receipt.contains("one.c\"") && receipt.contains("two.c\""),
        "{receipt}"
    );
    assert!(!receipt.contains("not-a-source.c"), "{receipt}");
    assert!(receipt.contains("\"W0001\":1"), "{receipt}");
    assert!(receipt.contains("\"failing_command\":null"), "{receipt}");
}

#[test]
fn build_runs_the_real_cjson_makefile_and_records_compilation() {
    let temp = Scratch::new();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.build/ladder/cjson/cJSON-1.7.19");
    assert!(
        source.is_dir(),
        "the pinned cJSON source is required for this acceptance test"
    );
    let project = temp.0.join("cjson");
    assert!(Command::new("cp")
        .arg("-R")
        .arg(&source)
        .arg(&project)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("make")
        .arg("clean")
        .current_dir(&project)
        .output()
        .unwrap()
        .status
        .success());
    let output = evo(&[
        "build",
        project.to_str().unwrap(),
        "--",
        "all",
        "test",
        "-j2",
    ]);
    success(&output);
    let receipt = std::fs::read_to_string(project.join("evo-build-receipt.json")).unwrap();
    for field in [
        "version",
        "target",
        "sources",
        "warnings",
        "failing_command",
        "exit_code",
    ] {
        assert!(receipt.contains(&format!("\"{field}\"")), "{receipt}");
    }
    assert!(receipt.contains("cJSON.c"), "{receipt}");
    assert!(receipt.contains("\"exit_code\":0"), "{receipt}");
    assert!(project.join("evo-build.log").is_file());
}
