//! GNU `returns_twice` (EvoIR 10.1.9). EvoCC has no execution rule for a
//! second return, so a call of such a function must not compile as an
//! ordinary call. A prototype only records the attribute: glibc's
//! `<pthread.h>` declares `__sigsetjmp_cancel` with it. A definition, a call
//! and every other use of the designator are E2952, in any declaration
//! order. A use inside a function that the unit never retains is not
//! reported, as for the other execution-only refusals.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const MESSAGE: &str = "error[E2952]: unsupported-function-attribute: returns_twice";

/// Every driver path that must agree: native assembly at two levels, the
/// syntax check, `evo check` and both interpreters.
const MODES: [&[&str]; 6] = [
    &["-O0", "--emit=asm"],
    &["-O2", "--emit=asm"],
    &["-fsyntax-only"],
    &["--check"],
    &["--interpret-c-hir"],
    &["--interpret-evoir"],
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name)
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
            "evocc-returns-twice-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn source(&self, index: usize, source: &str) -> PathBuf {
        let path = self.0.join(format!("case{index}.c"));
        std::fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed returns_twice probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn evocc(source: &Path, target: &str, mode: &[&str], options: &[&str]) -> Output {
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
    command
        .args(mode)
        .args(options)
        .arg(source)
        .output()
        .unwrap()
}

/// One-based line and column of the first occurrence of `needle`.
fn position(source: &str, needle: &str) -> (usize, usize) {
    let offset = source.find(needle).expect("the needle is in the source");
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let column = offset - before.rfind('\n').map_or(0, |newline| newline + 1) + 1;
    (line, column)
}

fn accepted(source: &Path, target: &str, options: &[&str]) {
    for mode in MODES {
        let output = evocc(source, target, mode, options);
        let label = format!("{} {target} {mode:?} {options:?}", source.display());
        assert!(output.status.success(), "{label}\n{}", text(&output));
        if mode.contains(&"--emit=asm") {
            assert!(!output.stdout.is_empty(), "{label}");
        } else if mode[0].starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n", "{label}\n{}", text(&output));
        }
    }
}

fn refused_at(source: &Path, target: &str, options: &[&str], line: usize, column: usize) {
    let expected = format!("{}:{line}:{column}: {MESSAGE}", source.display());
    for mode in MODES {
        let output = evocc(source, target, mode, options);
        let label = format!("{} {target} {mode:?} {options:?}", source.display());
        assert_eq!(output.status.code(), Some(1), "{label}\n{}", text(&output));
        assert!(
            output.stdout.is_empty(),
            "{label} published output\n{}",
            text(&output)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.lines().next(),
            Some(expected.as_str()),
            "{label}\n{stderr}"
        );
    }
}

#[test]
fn prototypes_record_the_attribute_without_a_refusal() {
    let work = Work::new("prototypes");
    let main = "int main(void) { return 0; }\n";
    for (index, declarations) in [
        "int f(void) __attribute__((returns_twice));\n",
        "__attribute__((__returns_twice__)) int f(void);\n",
        "[[gnu::returns_twice]] int f(void);\n",
        "[[__gnu__::__returns_twice__]] int f(void);\n",
        "int g(void), __attribute__((returns_twice)) f(void);\n",
        "extern int f(int) __asm__(\"h\") __attribute__((__nothrow__)) __attribute__((returns_twice));\n",
        "typedef int F(void);\nF f __attribute__((returns_twice));\n",
        "int f(void) __attribute__((returns_twice));\nint f(void) __attribute__((returns_twice));\n",
        // Only retained functions report their uses.
        "int f(void) __attribute__((returns_twice));\nstatic inline int g(void) { return f(); }\n",
        "int f(void);\nstatic inline int g(void) { return f(); }\nint f(void) __attribute__((returns_twice));\n",
        "int f(void) __attribute__((returns_twice));\nstatic inline int (*g(void))(void) { return &f; }\n",
    ]
    .into_iter()
    .enumerate()
    {
        let source = work.source(index, &format!("{declarations}{main}"));
        for target in TARGETS {
            accepted(&source, target, &[]);
        }
    }
    let source = work.source(
        99,
        "int main(void) {\n    extern int f(void) __attribute__((returns_twice));\n    return 0;\n}\n",
    );
    for target in TARGETS {
        accepted(&source, target, &[]);
    }
}

#[test]
fn every_use_of_a_returns_twice_function_is_refused_at_the_use() {
    let work = Work::new("uses");
    let prototype = "int f(void) __attribute__((returns_twice));\n";
    for (index, (source, needle)) in [
        (
            format!("{prototype}int main(void) {{ return f(); }}\n"),
            "f(); }",
        ),
        (
            format!("{prototype}int main(void) {{ int (*p)(void) = f; return p == 0; }}\n"),
            "f; return",
        ),
        (
            format!("{prototype}int main(void) {{ return &f == 0; }}\n"),
            "f == 0",
        ),
        (
            format!("{prototype}int (*p)(void) = f;\nint main(void) {{ return p == 0; }}\n"),
            "f;\n",
        ),
        (
            format!("{prototype}int main(void) {{ return sizeof(f()) == 0; }}\n"),
            "f()) == 0",
        ),
        (
            format!(
                "{prototype}static inline int g(void) {{ return f(); }}\nint main(void) {{ return g(); }}\n"
            ),
            "f(); }\nint main",
        ),
        // A later declaration that adds the attribute still refuses the call.
        (
            "int f(void);\nint main(void) { return f(); }\nint f(void) __attribute__((returns_twice));\n"
                .to_owned(),
            "f(); }",
        ),
        (
            "int f(void);\nint main(void) { int (*p)(void) = f; return p == 0; }\n\
             int f(void) __attribute__((returns_twice));\n"
                .to_owned(),
            "f; return",
        ),
        (
            "int main(void) {\n    extern int f(void) __attribute__((returns_twice));\n    return f();\n}\n"
                .to_owned(),
            "f();\n",
        ),
        (
            "int main(void) {\n    { extern int f(void) __attribute__((returns_twice)); }\n    extern int f(void);\n    return f();\n}\n"
                .to_owned(),
            "f();\n",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let path = work.source(index, &source);
        let (line, column) = position(&source, needle);
        for target in TARGETS {
            refused_at(&path, target, &[], line, column);
        }
    }
}

#[test]
fn definitions_of_a_returns_twice_function_are_refused() {
    let work = Work::new("definitions");
    let main = "int main(void) { return 0; }\n";
    for (index, (declarations, needle)) in [
        (
            "int f(void) __attribute__((returns_twice));\nint f(void) { return 0; }\n",
            "f(void) { return",
        ),
        (
            "int f(void) { return 0; }\nint f(void) __attribute__((returns_twice));\n",
            "f(void) { return",
        ),
        (
            "static int f(void) { return 0; }\nstatic int f(void) __attribute__((returns_twice));\n",
            "f(void) { return",
        ),
        (
            "__attribute__((returns_twice)) int f(void) { return 0; }\n",
            "returns_twice",
        ),
        (
            "[[gnu::returns_twice]] int f(void) { return 0; }\n",
            "gnu::returns_twice",
        ),
        (
            "int f(void) __attribute__((returns_twice)) { return 0; }\n",
            "returns_twice",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!("{declarations}{main}");
        let path = work.source(index, &source);
        let (line, column) = position(&source, needle);
        for target in TARGETS {
            refused_at(&path, target, &[], line, column);
        }
    }
}

#[test]
fn other_placements_keep_the_attribute_refusal() {
    let work = Work::new("placements");
    let main = "int main(void) { return 0; }\n";
    for (index, declarations) in [
        // Objects, typedefs and function pointers.
        "__attribute__((returns_twice)) int x;\n",
        "int x __attribute__((returns_twice));\n",
        "__attribute__((returns_twice)) int f(void), x;\n",
        "__attribute__((returns_twice)) int x, f(void);\n",
        "typedef int F(void) __attribute__((returns_twice));\n",
        "typedef __attribute__((returns_twice)) int F(void);\n",
        "int (*p)(void) __attribute__((returns_twice));\n",
        "__attribute__((returns_twice)) int (*p)(void);\n",
        "int (* __attribute__((returns_twice)) p)(void);\n",
        "int f(void) [[gnu::returns_twice]];\n",
        // Parameters and members.
        "int g(int h(void) __attribute__((returns_twice)));\n",
        "int g(__attribute__((returns_twice)) int x);\n",
        "struct s { __attribute__((returns_twice)) int x; };\n",
        "struct s { int (*p)(void) __attribute__((returns_twice)); };\n",
        "struct __attribute__((returns_twice)) s { int x; };\n",
        // Type names and statements.
        "int g(void) { return (int)(__attribute__((returns_twice)) long)0; }\n",
        "int g(void) { return (int (*)(void) __attribute__((returns_twice)))0 != 0; }\n",
        "int g(void) { return sizeof(__attribute__((returns_twice)) int); }\n",
        "int g(void) { __attribute__((returns_twice)); return 0; }\n",
        "int g(void) { l: __attribute__((returns_twice)); return 0; }\n",
        "int g(void) { for (__attribute__((returns_twice)) int i = 0; i < 1; i++) ; return 0; }\n",
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!("{declarations}{main}");
        let path = work.source(index, &source);
        // A bracketed attribute is reported at its namespace.
        let needle = if source.contains("gnu::") {
            "gnu::"
        } else {
            "returns_twice"
        };
        let (line, column) = position(&source, needle);
        for target in TARGETS {
            refused_at(&path, target, &[], line, column);
        }
    }
}

#[test]
fn linux_pthread_header_compiles_without_cancellation_macros() {
    accepted(&fixture("returns-twice-pthread-threads.c"), LINUX, &[]);
    accepted(
        &fixture("returns-twice-pthread-cleanup.c"),
        LINUX,
        &["-DUNUSED"],
    );
}

#[test]
fn linux_cleanup_macros_are_refused_at_the_cancellation_call() {
    let path = fixture("returns-twice-pthread-cleanup.c");
    let source = std::fs::read_to_string(&path).unwrap();
    let (line, column) = position(&source, "pthread_cleanup_push(release");
    refused_at(&path, LINUX, &[], line, column);
}

/// Linux: EvoCC assembly that includes `<pthread.h>` links with GCC 14 and
/// runs a thread at `-O0` and `-O2`.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_threads_link_with_gcc_14_and_run() {
    let work = Work::new("linux");
    let mut script = String::from("set -eu\ncd /work\n");
    let mut lines = 0;
    for level in ["-O0", "-O2"] {
        let name = format!("threads{level}");
        let assembly = work.0.join(format!("{name}.s"));
        let output = evocc(
            &fixture("returns-twice-pthread-threads.c"),
            LINUX,
            &[level, "-DSPAWN", "-S"],
            &["-o", assembly.to_str().unwrap()],
        );
        assert!(output.status.success(), "{level}\n{}", text(&output));
        script.push_str(&format!(
            "gcc-14 -pthread {name}.s -o {name}\n./{name}\necho '{name}: 0'\n"
        ));
        lines += 1;
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
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), lines, "{stdout}");
    println!("{stdout}");
}
