use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_TREE: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-day5-forced-include-{label}-{}-{sequence}",
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

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .output()
        .expect("run evocc")
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("fixture path is UTF-8")
}

#[test]
fn forced_header_macro_is_visible_in_main_source() {
    let tree = TempTree::new("macro-visible");
    let header = tree.write("forced.h", "#define FORCED_VALUE 41\n");
    let main = tree.write("main.c", "FORCED_VALUE\n");

    let output = run(&["-E", "-P", "-include", path_text(&header), path_text(&main)]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "41");
    assert!(output.stderr.is_empty());
}

#[test]
fn forced_headers_share_macros_in_command_line_order() {
    let tree = TempTree::new("ordered-macros");
    let first = tree.write("first.h", "#define ORDER_VALUE 11\n");
    let second = tree.write("second.h", "#undef ORDER_VALUE\n#define ORDER_VALUE 22\n");
    let main = tree.write("main.c", "ORDER_VALUE\n");

    let output = run(&[
        "-E",
        "-P",
        "-include",
        path_text(&first),
        "-include",
        path_text(&second),
        path_text(&main),
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "22");
    assert!(output.stderr.is_empty());
}

#[test]
fn command_line_definitions_apply_before_forced_headers_regardless_of_argv_position() {
    let tree = TempTree::new("late-command-line-definition");
    let header = tree.write(
        "requires-late.h",
        concat!(
            "#ifndef LATE_VALUE\n",
            "#error command-line definition was applied too late\n",
            "#endif\n",
        ),
    );
    let main = tree.write("main.c", "LATE_VALUE\n");

    // This is the ordering used by the frozen Kbuild argv: forced headers
    // appear before later -D options, but all command-line macro definitions
    // must exist before any forced header is processed.
    let output = run(&[
        "-E",
        "-P",
        "-include",
        path_text(&header),
        "-DLATE_VALUE=31",
        path_text(&main),
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "31");
    assert!(output.stderr.is_empty());
}

#[test]
fn pragma_once_is_scoped_to_one_forced_header_identity() {
    let tree = TempTree::new("pragma-once-identity");
    let first = tree.write("first.h", "#pragma once\nint first_marker;\n");
    let second = tree.write("second.h", "int second_marker;\n");
    let main = tree.write("main.c", "int main_marker;\n");

    let output = run(&[
        "-E",
        "-P",
        "-include",
        path_text(&first),
        "-include",
        path_text(&second),
        "-include",
        path_text(&first),
        path_text(&main),
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.matches("first_marker").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("second_marker").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("main_marker").count(), 1, "{stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn parser_diagnostic_inside_forced_header_names_canonical_header_location() {
    let tree = TempTree::new("header-diagnostic");
    let header = tree.write("broken.h", "int okay;\nint broken = !;\n");
    let main = tree.write("main.c", "int after;\n");
    let canonical_header = fs::canonicalize(&header).expect("canonical forced header path");

    let output = run(&[
        "--parse-only",
        "-include",
        path_text(&header),
        path_text(&main),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic is UTF-8"),
        format!(
            "{}:2:15: error[E1003]: expected-expression\n",
            canonical_header.to_string_lossy()
        )
    );
}

#[test]
fn missing_forced_header_fails_with_stable_diagnostic() {
    let tree = TempTree::new("missing-header");
    let main = tree.write("main.c", "int after;\n");
    let missing = tree.root.join("missing.h");

    let output = run(&["-E", "-include", path_text(&missing), path_text(&main)]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic is UTF-8"),
        format!(
            "{}:1:1: error[E0105]: forced-include-not-found: {}\n",
            main.to_string_lossy(),
            missing.to_string_lossy()
        )
    );
}

#[test]
fn unobserved_joined_forced_include_spelling_remains_unsupported() {
    let tree = TempTree::new("joined-spelling");
    let header = tree.write("forced.h", "#define VALUE 1\n");
    let main = tree.write("main.c", "VALUE\n");
    let joined = format!("-include={}", header.to_string_lossy());

    let output = run(&["-E", &joined, path_text(&main)]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic is UTF-8"),
        format!("evocc: unsupported option `{joined}`\n")
    );
}
