//! Hosted `main` arguments for both scalar interpreters.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

const MAIN_ARGC_ARGV: &str = "int main(int argc, char *argv[]) {\n\
    return argc == 1 && argv[0] != 0 && argv[0][0] != 0 && argv[1] == 0 ? 0 : 7;\n\
}\n";
const MAIN_ARGC_ARGV_ARRAY: &str = "int main(int argc, char *argv[2]) {\n\
    return argc == 1 && argv[0] != 0 && argv[1] == 0 ? 0 : 7;\n\
}\n";
const MAIN_VOID: &str = "int main(void) { return 0; }\n";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn temp_file(source: &str) -> PathBuf {
    let directory = root().join("target/frontend/tmp").join(format!(
        "d28c-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("program.c");
    std::fs::write(&path, source).unwrap();
    path
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn run_compiler(path: &Path, target: &str, level: &str, mode: &str) -> Output {
    let mut command = Command::new(evocc());
    command.args(["--target", target, "-std=gnu17", level, mode]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.arg(path).output().unwrap()
}

fn run_evo(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_evo"))
        .args(["run"])
        .arg(path)
        .output()
        .unwrap()
}

fn assert_success(output: &Output, context: &str) {
    assert!(output.status.success(), "{context}\n{}", text(output));
}

fn assert_exit_zero(output: &Output, context: &str) {
    assert_success(output, context);
    assert_eq!(output.stdout, b"0\n", "{context}\n{}", text(output));
}

#[test]
fn main_arguments_match_between_interpreter_options_and_evo_run() {
    for source in [MAIN_ARGC_ARGV, MAIN_ARGC_ARGV_ARRAY] {
        let path = temp_file(source);
        let evo = run_evo(&path);
        assert_success(&evo, "evo run");
        for target in TARGETS {
            for level in LEVELS {
                for mode in ["--interpret-c-hir", "--interpret-evoir"] {
                    let output = run_compiler(&path, target, level, mode);
                    assert_exit_zero(&output, &format!("{target} {level} {mode}"));
                }
            }
        }
    }

    let path = temp_file(MAIN_VOID);
    let evo = run_evo(&path);
    assert_success(&evo, "evo run main(void)");
    for target in TARGETS {
        for level in LEVELS {
            for mode in ["--interpret-c-hir", "--interpret-evoir"] {
                let output = run_compiler(&path, target, level, mode);
                assert_exit_zero(&output, &format!("{target} {level} {mode} main(void)"));
            }
        }
    }
}

#[test]
fn torture_originals_run_in_both_interpreters() {
    for name in ["scal-to-vec1.c", "scal-to-vec3.c"] {
        let path = root()
            .join(".evidence/rust-r10-20261021/originals/src/torture")
            .join(name);
        let evo = run_evo(&path);
        assert_success(&evo, name);
        for target in TARGETS {
            for mode in ["--interpret-c-hir", "--interpret-evoir"] {
                let output = run_compiler(&path, target, "-O0", mode);
                assert_exit_zero(&output, &format!("{name} {target} {mode}"));
            }
        }
    }
}

#[test]
fn other_main_signatures_keep_their_refusals() {
    for source in [
        "int main(int argc) { return argc; }\n",
        "int main(char *argv[]) { return argv != 0; }\n",
    ] {
        let path = temp_file(source);
        for target in TARGETS {
            for mode in ["--interpret-c-hir", "--interpret-evoir"] {
                let output = run_compiler(&path, target, "-O0", mode);
                assert!(
                    !output.status.success(),
                    "{target} {mode}\n{}",
                    text(&output)
                );
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains("E3007"),
                    "{target} {mode}\n{}",
                    text(&output)
                );
            }
        }
    }
}
