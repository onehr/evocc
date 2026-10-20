//! L07: GNU integer-complex division follows GCC 14's wide algorithm in the
//! component type on both targets (EvoIR 10.1.14). Apple clang uses the
//! textbook formula; the Darwin masks below are the EvoCC rule, not a claim
//! of agreement with Apple clang.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TARGETS: [&str; 2] = ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
const LEVELS: [&str; 2] = ["-O0", "-O2"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn probe() -> PathBuf {
    root().join("tests/fixtures/rules-20261013/l07-integer-complex-division.c")
}

fn boundaries() -> PathBuf {
    root().join("tests/fixtures/integer-complex-division-boundaries.c")
}

/// Every defined case with its expected exit status. The probe's masks are
/// the packet's GCC 14 results; bit 0 means "equals the wide algorithm".
fn defined_cases() -> Vec<(PathBuf, u8, i32)> {
    let mut cases = Vec::new();
    for (case, mask) in [1, 1, 1, 5, 5, 1].into_iter().enumerate() {
        cases.push((probe(), case as u8, mask));
    }
    for case in 0..9 {
        cases.push((boundaries(), case, 0));
    }
    cases
}

fn evocc(source: &Path, case: u8, target: &str, level: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .arg(format!("--target={target}"))
        .arg("-std=gnu11")
        .arg(level)
        .arg(format!("-DCASE={case}"))
        .arg(source);
    command
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn both_interpreters_follow_the_wide_algorithm_on_both_targets() {
    for target in TARGETS {
        for mode in INTERPRETERS {
            for level in LEVELS {
                for (source, case, expected) in defined_cases() {
                    let output = evocc(&source, case, target, level)
                        .arg(mode)
                        .output()
                        .unwrap();
                    assert!(output.status.success(), "{}", text(&output));
                    assert_eq!(
                        String::from_utf8_lossy(&output.stdout).trim(),
                        expected.to_string(),
                        "{} CASE={case} {target} {mode} {level}",
                        source.display()
                    );
                }
            }
        }
    }
}

#[test]
fn every_undefined_step_keeps_its_obligation() {
    let cases = [
        (10, "E3002"),
        (11, "E3004"),
        (12, "E3004"),
        (13, "E3004"),
        (14, "E3004"),
        (15, "E3002"),
    ];
    for target in TARGETS {
        for mode in INTERPRETERS {
            for level in LEVELS {
                for (case, code) in cases {
                    let output = evocc(&boundaries(), case, target, level)
                        .arg(mode)
                        .output()
                        .unwrap();
                    assert!(!output.status.success(), "CASE={case} {target} {mode}");
                    assert!(
                        String::from_utf8_lossy(&output.stderr).contains(code),
                        "CASE={case} {target} {mode} {level}: {}",
                        text(&output)
                    );
                }
            }
        }
    }
}

#[test]
fn a_zero_constant_divisor_is_not_a_static_constant() {
    for target in TARGETS {
        let output = evocc(&boundaries(), 16, target, "-O0")
            .arg("--emit=evoir")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{}", text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E2208"),
            "{}",
            text(&output)
        );
    }
}

#[test]
fn only_the_selected_branch_divides() {
    // A select would execute both quotients and report a false E3002 for a
    // divisor with one zero component (CASE 7 above). The lowered quotient
    // branches on |c| < |d| instead.
    let output = evocc(&boundaries(), 7, TARGETS[0], "-O0")
        .arg("--emit=evoir")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let module = String::from_utf8_lossy(&output.stdout);
    let start = module.find("@divide").expect("the divide function");
    let body = &module[start..];
    let body = &body[..body.find("\nfunc").unwrap_or(body.len())];
    assert!(body.contains("cond_br"), "{body}");
    assert_eq!(body.matches(" sdiv.bits<32> ").count(), 6, "{body}");
}

#[test]
fn limited_range_options_do_not_change_integer_division() {
    // The 2026-10-13 decision: -fcx-limited-range and -ffast-math select no
    // other integer algorithm. The cc driver ignores both with a warning.
    let work = std::env::temp_dir().join(format!("evocc-l07-options-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    for option in ["-fcx-limited-range", "-ffast-math"] {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args(["--target=aarch64-unknown-linux-gnu", "-std=gnu11", "-S"])
            .arg(option)
            .arg("-DCASE=1")
            .arg(probe())
            .arg("-o")
            .arg(work.join("probe.s"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{option}: {}", text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("has no effect"),
            "{option}: {}",
            text(&output)
        );
    }
    let _ = std::fs::remove_dir_all(&work);
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("evocc-l07-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed L07 probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_native_code_follows_the_wide_algorithm() {
    let work = Work::new("darwin");
    for level in LEVELS {
        for (source, case, expected) in defined_cases() {
            let program = work.0.join("program");
            let output = evocc(&source, case, TARGETS[1], level)
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", text(&output));
            let run = Command::new(&program).output().unwrap();
            assert_eq!(
                run.status.code(),
                Some(expected),
                "{} CASE={case} {level}",
                source.display()
            );
        }
    }
}

/// Linux native code: GCC 14 links and runs the EvoCC assembly, and compiles
/// the same source as the reference. Both must give the expected status.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_code_agrees_with_gcc_14() {
    let work = Work::new("linux");
    let mut script = String::from("set -eu\ncd /work\n");
    let mut expected_lines = 0;
    for (level_index, level) in LEVELS.into_iter().enumerate() {
        for (index, (source, case, expected)) in defined_cases().into_iter().enumerate() {
            let name = format!("division-{level_index}-{index}");
            let output = evocc(&source, case, TARGETS[0], level)
                .arg("-S")
                .arg("-o")
                .arg(work.0.join(format!("{name}.s")))
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", text(&output));
            let file = source.file_name().unwrap().to_string_lossy().into_owned();
            std::fs::copy(&source, work.0.join(&file)).unwrap();
            script.push_str(&format!(
                "gcc-14 {name}.s -o {name}\n\
                 gcc-14 -std=gnu11 {level} -w -DCASE={case} {file} -o {name}-gcc\n\
                 set +e\n./{name}\nevocc=$?\n./{name}-gcc\ngcc=$?\nset -e\n\
                 test \"$evocc\" -eq {expected}\ntest \"$gcc\" -eq {expected}\n\
                 echo '{name} {file} CASE={case} {level}: evocc={expected} gcc={expected}'\n"
            ));
            expected_lines += 1;
        }
    }
    // The integer-complex profile prints its divide row like GCC.
    let profile = root().join("tests/fixtures/integer-complex-profile.c");
    std::fs::copy(&profile, work.0.join("profile.c")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(format!("--target={}", TARGETS[0]))
        .arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(TARGETS[0]).display()
        ))
        .args(["-std=gnu11", "-O2", "-S", "-o"])
        .arg(work.0.join("profile.s"))
        .arg(&profile)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    script.push_str(
        "gcc-14 profile.s -o profile\ngcc-14 -std=gnu11 -O2 -w profile.c -o profile-gcc\n\
         ./profile > profile.out\n./profile-gcc > profile-gcc.out\n\
         cmp profile.out profile-gcc.out\ngrep 'divide=4/3' profile.out\n",
    );
    expected_lines += 1;
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
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), expected_lines, "{stdout}");
    println!("{stdout}");
}
