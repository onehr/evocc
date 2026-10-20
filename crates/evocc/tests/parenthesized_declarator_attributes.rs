//! GNU attributes after the `(` of a parenthesized declarator, through the
//! driver (EvoIR 10.1.9, D19). libxml's `xmlmemory.h:66` declares
//! `typedef void *(LIBXML_ATTR_ALLOC_SIZE(1) XMLCALL *xmlMallocFunc)(size_t)`,
//! and 53 libxml and libexslt headers of the Darwin SDK stopped there with
//! E1001. `crates/evocc-parser/tests/parenthesized_declarator_attributes.rs`
//! pins the positions and the refusals.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-paren-attributes-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn source(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn evocc(source: &Path, target: &str, mode: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .arg(format!("--target={target}"))
        .args(["-w", "--error-format=short"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.args(mode).arg(source).output().unwrap()
}

/// The declared entities keep their types and their values.
const PROGRAM: &str = r#"typedef unsigned long size_t;
typedef void *(__attribute__((alloc_size(1))) *allocator)(size_t size);
static char pool[64];
static void *grab(size_t size) { return size <= sizeof pool ? pool : 0; }
static allocator current = grab;
static int (__attribute__((unused)) counted) = 3;
static int (__attribute__((unused)) table)[3] = {4, 5, 6};
static int (__attribute__((unused)) twice)(int value) { return value * 2; }
struct holder { int (__attribute__((unused)) member); void (__attribute__((unused)) *hook)(int); };
static int seen;
static void record(int value) { seen = value; }
static void call(void (__attribute__((unused)) *callback)(int), int value) { callback(value); }
int main(void) {
    struct holder holder = {7, record};
    char *block = current(16);
    int (__attribute__((unused)) local), (__attribute__((unused)) other) = 9;
    local = twice(counted) + table[2];
    call(holder.hook, holder.member + other);
    return !(block == pool && local == 12 && seen == 16 && sizeof(table) == 3 * sizeof(int));
}
"#;

#[test]
fn declarations_with_parenthesized_attributes_run_on_both_targets() {
    let work = Work::new("program");
    let path = work.source("program.c", PROGRAM);
    for target in [LINUX, DARWIN] {
        for mode in [&["--interpret-c-hir"][..], &["--interpret-evoir"]] {
            let output = evocc(&path, target, mode);
            assert!(
                output.status.success(),
                "{target} {mode:?}\n{}",
                text(&output)
            );
            assert_eq!(
                output.stdout,
                b"0\n",
                "{target} {mode:?}\n{}",
                text(&output)
            );
        }
        for level in ["-O0", "-O2"] {
            let output = evocc(&path, target, &[level, "--emit=asm"]);
            assert!(
                output.status.success(),
                "{target} {level}\n{}",
                text(&output)
            );
        }
    }
}

/// Darwin: the libxml and libexslt headers of the sweep group, and a native
/// program that Apple clang links.
#[cfg(target_os = "macos")]
#[test]
fn darwin_libxml_headers_compile() {
    let work = Work::new("darwin");
    for header in [
        "libxml/xmlmemory.h",
        "libxml/tree.h",
        "libxml/parser.h",
        "libexslt/exslt.h",
    ] {
        let path = work.source(
            "unit.c",
            &format!("#include <{header}>\nint main(void) {{ return 0; }}\n"),
        );
        for level in ["-O0", "-O2"] {
            let output = evocc(&path, DARWIN, &[level, "--emit=asm"]);
            assert!(
                output.status.success(),
                "{header} {level}\n{}",
                text(&output)
            );
        }
    }
    let path = work.source("program.c", PROGRAM);
    for level in ["-O0", "-O2"] {
        let object = work.0.join(format!("program{level}.o"));
        let output = evocc(
            &path,
            DARWIN,
            &[level, "-c", "-o", object.to_str().unwrap()],
        );
        assert!(output.status.success(), "{level}\n{}", text(&output));
        let program = work.0.join(format!("program{level}"));
        let output = Command::new("clang")
            .arg(&object)
            .arg("-o")
            .arg(&program)
            .output()
            .unwrap();
        assert!(output.status.success(), "{level}\n{}", text(&output));
        assert_eq!(
            Command::new(&program).status().unwrap().code(),
            Some(0),
            "{level}"
        );
    }
}
