mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_TEMP_TREE: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-day4-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create isolated source tree");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::write(&path, contents).expect("write source fixture");
        path
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove isolated source tree");
    }
}

fn run_frontend(mode: &str, input: &Path) -> Output {
    support::warm_evocc();
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(mode)
        .arg(input)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evocc");

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait().expect("poll evocc") {
            Some(_) => return child.wait_with_output().expect("collect evocc output"),
            None if Instant::now() >= deadline => {
                child.kill().expect("kill timed-out evocc");
                let output = child.wait_with_output().expect("collect timed-out evocc");
                panic!(
                    "evocc exceeded the two-second watchdog; stdout={:?} stderr={:?}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn run_parse_only(input: &Path) -> Output {
    run_frontend("--parse-only", input)
}

#[test]
fn valid_header_and_main_parse_silently() {
    let tree = TempTree::new("cli-valid-include");
    tree.write("model.h", "struct Model {\n    int field;\n};\n");
    let main = tree.write(
        "consumer.c",
        "#include \"model.h\"\nstruct Model current;\n",
    );

    let output = run_parse_only(&main);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
}

#[test]
fn malformed_header_diagnostic_names_header_path_and_exact_position() {
    let tree = TempTree::new("cli-broken-include");
    let header = tree.write("broken.h", "int okay;\nint broken = !;\n");
    let main = tree.write("consumer.c", "#include \"broken.h\"\nint after;\n");

    let output = run_parse_only(&main);
    let canonical_header = fs::canonicalize(header).expect("canonical broken.h path");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic is UTF-8"),
        format!(
            "{}:2:15: error[E1003]: expected-expression\n",
            canonical_header.to_string_lossy()
        )
    );
}

#[test]
fn unmatched_header_conditionals_fail_without_panicking() {
    let cases = [
        ("orphan-elif", "#elif 1"),
        ("orphan-else", "#else"),
        ("orphan-endif", "#endif"),
        ("unterminated-if", "#if 1"),
    ];
    let mut mismatches = Vec::new();

    for (name, directive) in cases {
        let tree = TempTree::new(name);
        let header = tree.write(
            &format!("{name}.h"),
            &format!("int before;\n{directive}\nint after;\n"),
        );
        let main = tree.write("consumer.c", &format!("#include \"{name}.h\"\nint root;\n"));
        let output = run_frontend("-E", &main);
        let canonical_header = fs::canonicalize(header).expect("canonical header path");
        let expected_stderr = format!(
            "{}:2:1: error[E0102]: unmatched-conditional\n",
            canonical_header.to_string_lossy()
        );
        let actual = (
            output.status.code(),
            String::from_utf8(output.stdout).expect("stdout is UTF-8"),
            String::from_utf8(output.stderr).expect("stderr is UTF-8"),
        );
        let expected = (Some(1), String::new(), expected_stderr);
        if actual != expected {
            mismatches.push(format!("{name}: expected {expected:?}, got {actual:?}"));
        }
    }

    assert!(
        mismatches.is_empty(),
        "unmatched CLI conditional contract failed:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn include_conditionals_cannot_pair_with_caller_conditionals() {
    let cases = [
        (
            "header-closes-caller",
            "#endif\nint header_value;\n",
            "#if 1\n#include \"cross.h\"\n#endif\nint root;\n",
            false,
        ),
        (
            "caller-closes-header",
            "#if 1\nint header_value;\n",
            "#include \"cross.h\"\n#endif\nint root;\n",
            true,
        ),
    ];
    let mut mismatches = Vec::new();

    for (name, header_source, main_source, caller_is_orphan) in cases {
        let tree = TempTree::new(name);
        let header = tree.write("cross.h", header_source);
        let main = tree.write("main.c", main_source);
        let canonical_header = fs::canonicalize(header).expect("canonical cross.h path");
        let output = run_frontend("-E", &main);
        let mut expected_stderr = format!(
            "{}:1:1: error[E0102]: unmatched-conditional\n",
            canonical_header.to_string_lossy()
        );
        if caller_is_orphan {
            expected_stderr.push_str(&format!(
                "{}:2:1: error[E0102]: unmatched-conditional\n",
                main.to_string_lossy()
            ));
        }
        let actual = (
            output.status.code(),
            String::from_utf8(output.stdout).expect("stdout is UTF-8"),
            String::from_utf8(output.stderr).expect("stderr is UTF-8"),
        );
        let expected = (Some(1), String::new(), expected_stderr);
        if actual != expected {
            mismatches.push(format!("{name}: expected {expected:?}, got {actual:?}"));
        }
    }

    assert!(
        mismatches.is_empty(),
        "include conditional boundary CLI contract failed:\n{}",
        mismatches.join("\n")
    );
}
