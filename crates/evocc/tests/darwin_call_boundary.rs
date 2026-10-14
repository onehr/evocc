//! Darwin calls across the compiler boundary (T03h and D04; audit findings
//! A02, A03 and A04). On the macOS host, every shape of
//! `tests/fixtures/darwin-call-boundary/` gives the Apple clang value when
//! EvoCC compiles the callee and Apple clang the caller, and when EvoCC
//! compiles the caller and Apple clang the callee, at `-O0` and `-O2`. The
//! caller returns 0, or the number of the first check that failed.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/darwin-call-boundary")
}

fn succeeded(what: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{what}: status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A fresh directory with the fixture sources, removed on success.
struct Work(PathBuf);

impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-darwin-call-boundary-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        for entry in std::fs::read_dir(fixtures()).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), path.join(entry.file_name())).unwrap();
        }
        Self(path)
    }

    /// Compile `source` to `object` with EvoCC or Apple clang.
    fn compile(&self, evocc: bool, level: &str, source: &str, object: &str) {
        let program = if evocc {
            env!("CARGO_BIN_EXE_evocc")
        } else {
            "clang"
        };
        let output = Command::new(program)
            .current_dir(&self.0)
            .args([level, "-c", source, "-o", object])
            .output()
            .unwrap();
        succeeded(&format!("{program} {level} {source}"), &output);
    }

    /// Link the two objects with Apple clang and return the exit status.
    fn run(&self, caller: &str, callee: &str, program: &str) -> Option<i32> {
        let output = Command::new("clang")
            .current_dir(&self.0)
            .args([caller, callee, "-o", program])
            .output()
            .unwrap();
        succeeded(&format!("link {program}"), &output);
        Command::new(self.0.join(program))
            .output()
            .unwrap()
            .status
            .code()
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained: {}", self.0.display());
            return;
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_call_shape_agrees_with_apple_clang_in_both_directions() {
    let work = Work::new("shapes");
    let mut failures = Vec::new();
    for level in ["-O0", "-O2"] {
        work.compile(false, level, "callee.c", "clang-callee.o");
        work.compile(false, level, "caller.c", "clang-caller.o");
        work.compile(true, level, "callee.c", "evocc-callee.o");
        work.compile(true, level, "caller.c", "evocc-caller.o");
        // The reference pair checks the expected values themselves.
        assert_eq!(
            work.run("clang-caller.o", "clang-callee.o", "reference"),
            Some(0),
            "{level}: the Apple clang pair"
        );
        for (caller, callee, label) in [
            ("clang-caller.o", "evocc-callee.o", "EvoCC callee"),
            ("evocc-caller.o", "clang-callee.o", "EvoCC caller"),
            ("evocc-caller.o", "evocc-callee.o", "EvoCC pair"),
        ] {
            let status = work.run(caller, callee, "crossed");
            if status != Some(0) {
                failures.push(format!("{level} {label}: check {status:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
