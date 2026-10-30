//! Lints W0006 to W0009 for the musl pointer warning options.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

const POINTER_SIGN: &str = "\
void assign(void) { char *p = 0; unsigned char *q = 0; p = q; }\n\
void init(void) { unsigned char *q = 0; char *p = q; }\n\
char *ret(unsigned char *q) { return q; }\n\
void take(char *p);\n\
void argument(void) { unsigned char *q = 0; take(q); }\n\
void assign_int(void) { int *p = 0; unsigned int *q = 0; p = q; }\n";

const POINTER_SIGN_CLEAN: &str = "\
void assign(void) { char *p = 0; char *q = 0; p = q; }\n\
void init(void) { char *q = 0; char *p = q; }\n\
char *ret(char *q) { return q; }\n\
void take(char *p);\n\
void argument(void) { char *q = 0; take(q); }\n\
int main(void) { return 0; }\n";

const POINTER_ARITH: &str = "\
void add(void *p) { p = p + 1; }\n\
void sub(void *p) { p = p - 1; }\n\
void inc(void *p) { p++; }\n\
void dec(void *p) { p--; }\n\
void add_assign(void *p) { p += 1; }\n\
void sub_assign(void *p) { p -= 1; }\n\
int f(void) { return 0; }\n\
void function_add(void) { int (*p)(void) = f; p = p + 1; }\n";

const POINTER_ARITH_CLEAN: &str = "\
void add(char *p) { p = p + 1; }\n\
void sub(char *p) { p = p - 1; }\n\
void inc(char *p) { p++; }\n\
void dec(char *p) { p--; }\n\
void add_assign(char *p) { p += 1; }\n\
void sub_assign(char *p) { p -= 1; }\n\
int main(void) { return 0; }\n";

const POINTER_ARITH_SIZES: &str = "\
int a = sizeof(void);\n\
int b = _Alignof(void);\n\
int f(void) { return 0; }\n\
int c = sizeof(f);\n\
int d = _Alignof(f);\n";

const QUALIFIERS: &str = "\
void assign(void) { const int *p = 0; int *q = p; }\n\
void init(void) { const int *p = 0; int *q = p; }\n\
int *ret(const int *p) { return p; }\n\
void take(int *p);\n\
void argument(void) { const int *p = 0; take(p); }\n\
void array(void) { const int a[2] = {0}; int (*p)[2] = &a; }\n";

const QUALIFIERS_CLEAN: &str = "\
void assign(void) { const int *p = 0; const int *q = p; }\n\
void init(void) { const int *p = 0; const int *q = p; }\n\
const int *ret(const int *p) { return p; }\n\
void take(const int *p);\n\
void argument(void) { const int *p = 0; take(p); }\n\
void array(void) { const int a[2] = {0}; const int (*p)[2] = &a; }\n\
int main(void) { return 0; }\n";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn command(target: &str) -> Command {
    let mut command = Command::new(evocc());
    command.args(["--target", target, "-std=gnu17"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command
}

fn run(source: &str, target: &str, args: &[&str]) -> Output {
    let mut command = command(target);
    command.args(args);
    command
        .args(["--parse-only", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn has_code(output: &Output, code: &str) -> bool {
    String::from_utf8_lossy(&output.stderr).contains(&format!("[{code}]"))
}

fn compile_assembly(source: &str, target: &str, args: &[&str]) -> Output {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "pointer-lint-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let source_file = directory.join("source.c");
    let assembly = directory.join("source.s");
    std::fs::write(&source_file, source).unwrap();
    let output = command(target)
        .args(args)
        .args(["-S", "-o", assembly.to_str().unwrap()])
        .arg(&source_file)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(directory);
    output
}

#[test]
fn pointer_sign_lint_matches_the_option_levels() {
    for target in TARGETS {
        let quiet = run(POINTER_SIGN, target, &[]);
        assert!(quiet.status.success(), "{target}\n{}", text(&quiet));
        assert!(!has_code(&quiet, "W0006"));

        let warning = run(POINTER_SIGN, target, &["-Wpointer-sign"]);
        assert!(warning.status.success(), "{target}\n{}", text(&warning));
        assert!(has_code(&warning, "W0006"), "{target}\n{}", text(&warning));

        let error = run(POINTER_SIGN, target, &["-Werror=pointer-sign"]);
        assert!(!error.status.success(), "{target}\n{}", text(&error));
        assert!(has_code(&error, "W0006"), "{target}\n{}", text(&error));

        let wall = run(POINTER_SIGN, target, &["-Wall"]);
        assert!(wall.status.success(), "{target}\n{}", text(&wall));
        assert!(has_code(&wall, "W0006"), "{target}\n{}", text(&wall));
    }
}

#[test]
fn pointer_arith_lint_matches_the_option_levels() {
    for target in TARGETS {
        let quiet = run(POINTER_ARITH, target, &[]);
        assert!(quiet.status.success(), "{target}\n{}", text(&quiet));
        assert!(!has_code(&quiet, "W0007"));

        let warning = run(POINTER_ARITH, target, &["-Wpointer-arith"]);
        assert!(warning.status.success(), "{target}\n{}", text(&warning));
        assert!(has_code(&warning, "W0007"), "{target}\n{}", text(&warning));

        let error = run(POINTER_ARITH, target, &["-Werror=pointer-arith"]);
        assert!(!error.status.success(), "{target}\n{}", text(&error));
        assert!(has_code(&error, "W0007"), "{target}\n{}", text(&error));

        let wall = run(POINTER_ARITH, target, &["-Wall"]);
        assert!(wall.status.success(), "{target}\n{}", text(&wall));
        assert!(!has_code(&wall, "W0007"), "{target}\n{}", text(&wall));

        let pedantic = run(POINTER_ARITH, target, &["-Wpedantic"]);
        assert!(pedantic.status.success(), "{target}\n{}", text(&pedantic));
        assert!(
            has_code(&pedantic, "W0007"),
            "{target}\n{}",
            text(&pedantic)
        );
    }
}

#[test]
fn pointer_arith_sizeof_and_alignof_are_w0007() {
    for target in TARGETS {
        let quiet = run(POINTER_ARITH_SIZES, target, &[]);
        assert!(quiet.status.success(), "{target}\n{}", text(&quiet));
        assert!(!has_code(&quiet, "W0007"));
        let warning = run(POINTER_ARITH_SIZES, target, &["-Wpointer-arith"]);
        assert!(warning.status.success(), "{target}\n{}", text(&warning));
        assert!(has_code(&warning, "W0007"), "{target}\n{}", text(&warning));
        let error = run(POINTER_ARITH_SIZES, target, &["-Werror=pointer-arith"]);
        assert!(!error.status.success(), "{target}\n{}", text(&error));
        assert!(has_code(&error, "W0007"), "{target}\n{}", text(&error));
    }
}

#[test]
fn discarded_qualifier_lints_match_the_option_levels() {
    for target in TARGETS {
        let quiet = run(QUALIFIERS, target, &[]);
        assert!(quiet.status.success(), "{target}\n{}", text(&quiet));
        assert!(has_code(&quiet, "W0008"), "{target}\n{}", text(&quiet));
        assert!(has_code(&quiet, "W0009"), "{target}\n{}", text(&quiet));

        let error = run(QUALIFIERS, target, &["-Werror=discarded-qualifiers"]);
        assert!(!error.status.success(), "{target}\n{}", text(&error));
        assert!(has_code(&error, "W0008"), "{target}\n{}", text(&error));

        let array_error = run(QUALIFIERS, target, &["-Werror=discarded-array-qualifiers"]);
        assert!(
            !array_error.status.success(),
            "{target}\n{}",
            text(&array_error)
        );
        assert!(
            has_code(&array_error, "W0009"),
            "{target}\n{}",
            text(&array_error)
        );
    }
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_sealed_warning_sets_match_gcc() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("pointer-lint-gcc-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let cases = [
        (
            POINTER_SIGN,
            "-Wpointer-sign",
            true,
            "W0006",
            "-Wpointer-sign",
        ),
        (
            POINTER_SIGN,
            "-Werror=pointer-sign",
            false,
            "W0006",
            "-Werror=pointer-sign",
        ),
        (
            POINTER_ARITH,
            "-Wpointer-arith",
            true,
            "W0007",
            "-Wpointer-arith",
        ),
        (QUALIFIERS, "", true, "W0008", "-Wdiscarded-qualifiers"),
        (
            QUALIFIERS,
            "",
            true,
            "W0009",
            "-Wdiscarded-array-qualifiers",
        ),
    ];
    for (index, (source, option, expect_success, code, gcc_option)) in cases.iter().enumerate() {
        let file = directory.join(format!("case-{index}.c"));
        std::fs::write(&file, source).unwrap();
        let mut evocc = command(LINUX);
        evocc.args(["--parse-only", "-std=c99"]);
        if !option.is_empty() {
            evocc.arg(option);
        }
        let evocc = evocc.arg(&file).output().unwrap();
        assert_eq!(evocc.status.success(), *expect_success, "{}", text(&evocc));
        assert!(has_code(&evocc, code), "{}", text(&evocc));

        let script =
            format!("cd /work && gcc-14 -std=c99 -fsyntax-only {gcc_option} case-{index}.c 2>&1");
        let gcc = Command::new("container")
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
        let gcc_text = String::from_utf8_lossy(&gcc.stdout);
        assert!(gcc_text.contains(gcc_option), "case {index}: {gcc_text}");
    }
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn clean_cases_keep_the_same_assembly_with_and_without_options() {
    let cases = [
        (POINTER_SIGN_CLEAN, "-Wpointer-sign"),
        (POINTER_ARITH_CLEAN, "-Wpointer-arith"),
        (QUALIFIERS_CLEAN, "-Werror=discarded-qualifiers"),
        (QUALIFIERS_CLEAN, "-Werror=discarded-array-qualifiers"),
    ];
    for target in TARGETS {
        for (source, option) in cases {
            let plain = compile_assembly(source, target, &[]);
            let with = compile_assembly(source, target, &[option]);
            assert!(
                plain.status.success(),
                "{target} {option}\n{}",
                text(&plain)
            );
            assert!(with.status.success(), "{target} {option}\n{}", text(&with));
            assert_eq!(
                plain.stdout, with.stdout,
                "{target} {option} changed clean assembly"
            );
        }
    }
}
