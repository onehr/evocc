//! Multi-character constants such as `'TEXT'` (EvoIR 10.5, D19). Apple's
//! Security and CoreServices headers define enumerators with them. An
//! ordinary constant with two or more ASCII characters or escapes of at most
//! `\xff` has type `int`; each element shifts the value left by eight bits and
//! adds its byte, and the value keeps its low 32 bits, as GCC 14 and Apple
//! clang 21 compute it. A form that the two compilers read differently keeps
//! `E1006`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const DARWIN: &str = "aarch64-apple-darwin";
const LINUX: &str = "aarch64-unknown-linux-gnu";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];

/// The lines that GCC 14 and Apple clang 21 print for `values.c`.
const EXPECTED: &str = "1 24930 6382179 1633837924\n65535 255 -1\n258 65281 1650680933\n4 10023\n";

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root()
        .join("tests/fixtures/multicharacter-constants")
        .join(name)
}

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "evocc-multicharacter-{}-{}",
        std::process::id(),
        NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
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

fn evocc(target: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.arg(format!("--target={target}"));
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.args(args).output().unwrap()
}

#[test]
fn both_interpreters_compute_the_values_on_both_targets() {
    let path = fixture("interpreted.c");
    for target in TARGETS {
        for mode in INTERPRETERS {
            for level in ["-O0", "-O2"] {
                let output = evocc(target, &[level, mode, path.to_str().unwrap()]);
                assert!(
                    output.status.success(),
                    "{target} {mode}\n{}",
                    text(&output)
                );
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout).trim(),
                    "0",
                    "{target} {mode} {level}\n{}",
                    text(&output)
                );
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_programs_print_what_apple_clang_prints() {
    let directory = scratch();
    let source = fixture("values.c");
    for level in ["-O0", "-O2"] {
        let mine = directory.join(format!("values-evocc{level}"));
        let theirs = directory.join(format!("values-clang{level}"));
        let built = evocc(
            DARWIN,
            &[
                level,
                source.to_str().unwrap(),
                "-o",
                mine.to_str().unwrap(),
            ],
        );
        assert!(built.status.success(), "{}", text(&built));
        let built = Command::new("clang")
            .args(["-w", level])
            .arg(&source)
            .arg("-o")
            .arg(&theirs)
            .output()
            .unwrap();
        assert!(built.status.success(), "{}", text(&built));
        let mine = Command::new(&mine).output().unwrap();
        let theirs = Command::new(&theirs).output().unwrap();
        assert!(mine.status.success(), "{}", text(&mine));
        assert_eq!(mine.stdout, theirs.stdout, "{level}");
        assert_eq!(String::from_utf8_lossy(&mine.stdout), EXPECTED);
    }
    let _ = std::fs::remove_dir_all(&directory);
}

/// GCC 14 prints the same lines on Linux.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_programs_print_what_gcc_14_prints() {
    let directory = scratch();
    std::fs::copy(fixture("values.c"), directory.join("values.c")).unwrap();
    let mut script = String::from("set -e\ncd /work\n");
    for level in ["-O0", "-O2"] {
        let assembly = directory.join(format!("values{level}.s"));
        let output = evocc(
            LINUX,
            &[
                level,
                "-S",
                directory.join("values.c").to_str().unwrap(),
                "-o",
                assembly.to_str().unwrap(),
            ],
        );
        assert!(output.status.success(), "{}", text(&output));
        script.push_str(&format!(
            "gcc-14 values{level}.s -o evocc{level} && ./evocc{level} > evocc{level}.txt\n\
             gcc-14 -w {level} values.c -o gcc{level} && ./gcc{level} > gcc{level}.txt\n\
             cmp evocc{level}.txt gcc{level}.txt\ncat evocc{level}.txt\n"
        ));
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
        .arg(format!("{}:/work", directory.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", &script])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{EXPECTED}{EXPECTED}")
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn forms_that_the_compilers_read_differently_keep_e1006() {
    for source in [
        "int x = L'ab';\n",
        "int x = u'ab';\n",
        "int x = U'ab';\n",
        "int x = 'a\\u00e9';\n",
        "int x = 'a\\x100';\n",
        "int x = 'a\\400';\n",
        "#if 'a\\x100'\n#endif\n",
    ] {
        for target in TARGETS {
            let directory = scratch();
            let path = directory.join("source.c");
            std::fs::write(&path, source).unwrap();
            let output = evocc(
                target,
                &[
                    "--error-format=short",
                    "--parse-only",
                    path.to_str().unwrap(),
                ],
            );
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("error[E1006]"),
                "{target} {source:?}\n{}",
                text(&output)
            );
            let _ = std::fs::remove_dir_all(&directory);
        }
    }
}
