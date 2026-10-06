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
        let requested = std::env::temp_dir().join(format!(
            "evocc-day5-macro-prefix-map-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&requested).expect("create isolated source tree");
        let root = fs::canonicalize(requested).expect("canonical temporary source tree");
        Self { root }
    }

    fn mkdir(&self, relative: &str) {
        fs::create_dir_all(self.root.join(relative)).expect("create fixture directory");
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
        .arg("--error-format=short")
        .args(args)
        .output()
        .expect("run evocc")
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("fixture path is UTF-8")
}

fn map_flag(old: &Path, new: &str) -> String {
    format!("-fmacro-prefix-map={}={new}", path_text(old))
}

#[test]
fn main_file_macro_uses_mapped_display_path_only() {
    let tree = TempTree::new("main-file");
    let main = tree.write(
        "main.c",
        "const char *main_file = __FILE__;\n__BASE_FILE__\n",
    );
    let flag = map_flag(&tree.root, "/virtual/source");

    let output = run(&["-E", "-P", &flag, path_text(&main)]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("\"/virtual/source/main.c\""), "{stdout}");
    assert!(
        stdout.contains("__BASE_FILE__"),
        "Day 5 only claims context-dependent __FILE__ mapping: {stdout}"
    );
    assert!(!stdout.contains(path_text(&tree.root)), "{stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn forced_header_file_macro_uses_mapped_canonical_path() {
    let tree = TempTree::new("forced-header");
    let header = tree.write(
        "forced.h",
        "const char *forced_file = __FILE__;\n#define FORCED_READY 1\n",
    );
    let main = tree.write(
        "main.c",
        "const char *main_file = __FILE__;\nFORCED_READY\n",
    );
    let flag = map_flag(&tree.root, "/virtual/tree");

    let output = run(&[
        "-E",
        "-P",
        "-include",
        path_text(&header),
        &flag,
        path_text(&main),
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("\"/virtual/tree/forced.h\""), "{stdout}");
    assert!(stdout.contains("\"/virtual/tree/main.c\""), "{stdout}");
    assert!(
        stdout.contains('1'),
        "forced header was not processed: {stdout}"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn mapping_does_not_change_nested_include_lookup() {
    let tree = TempTree::new("nested-include");
    tree.mkdir("headers/nested");
    tree.write(
        "headers/outer.h",
        "#include \"nested/inner.h\"\nconst char *outer_file = __FILE__;\n",
    );
    tree.write(
        "headers/nested/inner.h",
        "const char *inner_file = __FILE__;\n",
    );
    let main = tree.write("main.c", "#include \"headers/outer.h\"\nint after;\n");
    let flag = map_flag(&tree.root, "mapped-root");

    let output = run(&["-E", "-P", &flag, path_text(&main)]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        stdout.contains("\"mapped-root/headers/nested/inner.h\""),
        "{stdout}"
    );
    assert!(
        stdout.contains("\"mapped-root/headers/outer.h\""),
        "{stdout}"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_replacement_with_trailing_slash_old_yields_relative_file_name() {
    let tree = TempTree::new("empty-replacement");
    let main = tree.write("main.c", "const char *main_file = __FILE__;\n");
    let flag = format!(
        "-fmacro-prefix-map={}/=",
        path_text(&tree.root).trim_end_matches('/')
    );

    let output = run(&["-E", "-P", &flag, path_text(&main)]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("\"main.c\""), "{stdout}");
    assert!(!stdout.contains(path_text(&tree.root)), "{stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn repeated_maps_use_the_last_matching_option() {
    let tree = TempTree::new("repeated-map");
    let main = tree.write("main.c", "const char *main_file = __FILE__;\n");
    let broad = map_flag(&tree.root, "/first");
    let exact = map_flag(&main, "/second.c");

    for (flags, expected, rejected) in [
        (
            [broad.as_str(), exact.as_str()],
            "\"/second.c\"",
            "/first/main.c",
        ),
        (
            [exact.as_str(), broad.as_str()],
            "\"/first/main.c\"",
            "/second.c",
        ),
    ] {
        let output = run(&["-E", "-P", flags[0], flags[1], path_text(&main)]);

        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
        assert!(stdout.contains(expected), "{stdout}");
        assert!(!stdout.contains(rejected), "{stdout}");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn mapped_header_error_still_names_exact_physical_canonical_path() {
    let tree = TempTree::new("physical-diagnostic");
    tree.mkdir("headers");
    let header = tree.write("headers/broken.h", "#error physical failure\n");
    let main = tree.write("main.c", "#include \"headers/broken.h\"\nint after;\n");
    let flag = map_flag(&tree.root, "/virtual/tree");

    let output = run(&["-E", "-P", &flag, path_text(&main)]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic is UTF-8"),
        format!(
            "{}:1:1: error[E0104]: error-directive: physical failure\n",
            path_text(&header)
        )
    );
}

#[test]
fn malformed_joined_maps_are_rejected_deterministically() {
    let tree = TempTree::new("malformed");
    let main = tree.write("main.c", "__FILE__\n");

    for (flag, payload) in [
        ("-fmacro-prefix-map=missing-separator", "missing-separator"),
        ("-fmacro-prefix-map==virtual", "=virtual"),
    ] {
        let output = run(&["-E", flag, path_text(&main)]);

        assert_eq!(output.status.code(), Some(1), "flag={flag}");
        assert!(output.stdout.is_empty(), "flag={flag}");
        assert_eq!(
            String::from_utf8(output.stderr).expect("diagnostic is UTF-8"),
            format!(
                "evocc: invalid -fmacro-prefix-map `{payload}`; expected non-empty OLD in OLD=NEW\n"
            ),
            "flag={flag}"
        );
    }
}
