//! The reviewed string-constant rule is independent of target and optimizer.
//! Reference compilers have wider or optimization-dependent classifications;
//! the frozen masks below are the approved EvoCC rule, not a parity claim.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn cases() -> Vec<(PathBuf, Option<u8>, i32)> {
    let fixtures = root().join("tests/fixtures");
    let directory = fixtures.join("rules-20261013");
    let mut cases = Vec::new();
    for (case, expected) in [135, 193, 181, 26, 5].into_iter().enumerate() {
        cases.push((
            directory.join("k07-strlen-ice-matrix.c"),
            Some(case as u8),
            expected,
        ));
    }
    for case in 0..9 {
        cases.push((
            directory.join("k07-strlen-ice-contexts.c"),
            Some(case),
            if case == 8 { 3 } else { 0 },
        ));
    }
    for (case, expected) in [101, 0].into_iter().enumerate() {
        cases.push((
            directory.join("k07-strlen-call-effects.c"),
            Some(case as u8),
            expected,
        ));
    }
    cases.push((fixtures.join("fortify-nonliteral-strlen-ice.c"), None, 7));
    cases.push((fixtures.join("fortify-single-trip-query.c"), None, 0));
    cases
}

fn command(source: &Path, case: Option<u8>, level: &str, target: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .arg(format!("--target={target}"))
        .arg(level)
        .arg(source);
    if let Some(case) = case {
        command.arg(format!("-DCASE={case}"));
    }
    command
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn both_interpreters_apply_the_same_string_rule_at_every_level() {
    for target in ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            for level in ["-O0", "-O2"] {
                for (source, case, expected) in cases() {
                    let output = command(&source, case, level, target)
                        .arg(mode)
                        .output()
                        .unwrap();
                    success(&output);
                    assert_eq!(
                        String::from_utf8_lossy(&output.stdout).trim(),
                        expected.to_string(),
                        "{} CASE={case:?} {target} {mode} {level}",
                        source.display()
                    );
                }
            }
        }
    }
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("evocc-string-{label}-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed string probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_native_string_classifications_and_effects_match_the_contract() {
    let work = Work::new("darwin");
    for level in ["-O0", "-O2"] {
        for (source, case, expected) in cases() {
            let output = work.0.join("program");
            success(
                &command(&source, case, level, "aarch64-apple-darwin")
                    .arg("-o")
                    .arg(&output)
                    .output()
                    .unwrap(),
            );
            let run = Command::new(&output).output().unwrap();
            assert_eq!(
                run.status.code(),
                Some(expected),
                "{} {case:?} {level}: {}",
                source.display(),
                String::from_utf8_lossy(&run.stderr)
            );
        }
    }
}

#[test]
#[ignore = "sealed Linux string lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_string_classifications_and_effects_match_the_contract() {
    let work = Work::new("linux");
    let mut script = String::from("set -eu\ncd /work\n");
    for (level_index, level) in ["-O0", "-O2"].into_iter().enumerate() {
        for (index, (source, case, expected)) in cases().into_iter().enumerate() {
            let name = format!("string-{level_index}-{index}");
            success(
                &command(&source, case, level, "aarch64-unknown-linux-gnu")
                    .arg("-S")
                    .arg("-o")
                    .arg(work.0.join(format!("{name}.s")))
                    .output()
                    .unwrap(),
            );
            script.push_str(&format!(
                "gcc-14 {name}.s -o {name}\nset +e\n./{name}\nactual=$?\nset -e\n\
                 test \"$actual\" -eq {expected}\necho '{name}: {expected}'\n"
            ));
        }
    }
    let output = Command::new("container")
        .args([
            "run",
            "--rm",
            "--platform",
            "linux/arm64",
            "--network",
            "none",
            "-v",
        ])
        .arg(format!("{}:/work", work.0.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", &script])
        .output()
        .unwrap();
    success(&output);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).lines().count(),
        2 * cases().len()
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
