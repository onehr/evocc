//! `__builtin_fpclassify` (EvoIR 10.1.14, D21), as GCC 14 documents it.
//! `__builtin_fpclassify(FP_NAN, FP_INFINITE, FP_NORMAL, FP_SUBNORMAL,
//! FP_ZERO, x)` returns the class argument that matches the class of `x`.
//! The five class arguments are integer constant expressions. `x` is
//! type-generic: a `float` is not promoted, so a `float` subnormal stays
//! subnormal. glibc's `<math.h>` defines `fpclassify` with this builtin for
//! GCC; before D21 the call was E2949.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];

/// Every driver path that runs `main`: `evo check` and both interpreters.
const EXECUTE: [&[&str]; 3] = [&["--check"], &["--interpret-c-hir"], &["--interpret-evoir"]];

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
        let path =
            std::env::temp_dir().join(format!("evocc-fpclassify-{label}-{}", std::process::id()));
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
            eprintln!("retained failed fpclassify probe: {}", self.0.display());
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

/// Each class of each type, both signs, as constants and through volatile
/// objects. `main` returns the number of wrong answers.
const CLASSES: &str = r#"#include <float.h>
#define CLASS(x) __builtin_fpclassify(10, 11, 12, 13, 14, x)
#define NEGATIVE(x) __builtin_fpclassify(-5, -4, -3, -2, -1, x)
static int failures;
static void expect(int actual, int expected) { failures += actual != expected; }
static volatile float zf = 0.0f, of = 1.0f;
static volatile double zd = 0.0, od = 1.0;
static volatile long double zl = 0.0L, ol = 1.0L;
#define CHECK(T, Z, O, INF, MIN, TRUE_MIN, MAX)                               \
    do {                                                                    \
        T inf = INF, nan = inf - inf;                                       \
        T values[10] = {nan, -nan, inf, -inf, O, -(MAX), (MIN) / 2,         \
                        -(TRUE_MIN), Z, -(Z)};                              \
        int expected[10] = {10, 10, 11, 11, 12, 12, 13, 13, 14, 14};        \
        for (int i = 0; i < 10; i++) {                                      \
            expect(CLASS(values[i]), expected[i]);                          \
            expect(NEGATIVE(values[i]), expected[i] - 15);                  \
        }                                                                   \
        expect(CLASS((MIN) * 1), 12);                                       \
        expect(CLASS(TRUE_MIN), 13);                                        \
        expect(CLASS((T)0), 14);                                            \
    } while (0)
int main(void) {
    CHECK(float, zf, of, __builtin_inff(), FLT_MIN, FLT_TRUE_MIN, FLT_MAX);
    CHECK(double, zd, od, __builtin_inf(), DBL_MIN, DBL_TRUE_MIN, DBL_MAX);
    CHECK(long double, zl, ol, __builtin_infl(), LDBL_MIN, LDBL_TRUE_MIN, LDBL_MAX);
    /* No promotion: these are subnormal in their own type only. */
    expect(CLASS(FLT_TRUE_MIN), 13);
    expect(CLASS(FLT_MIN / 2), 13);
    expect(CLASS(DBL_TRUE_MIN), 13);
    expect(CLASS((float)DBL_TRUE_MIN), 14);
    /* The constants fold, and each argument is evaluated once. */
    static const int folded = __builtin_fpclassify(1, 2, 3, 4, 5, 1.0f);
    expect(folded, 3);
    int calls = 0;
    expect(CLASS((calls++, zd)), 14);
    expect(calls, 1);
    return failures;
}
"#;

#[test]
fn each_class_of_each_type_in_every_execution_path() {
    let work = Work::new("classes");
    let path = work.source("classes.c", CLASSES);
    for target in TARGETS {
        for mode in EXECUTE {
            let output = evocc(&path, target, mode);
            let label = format!("{target} {mode:?}");
            assert!(output.status.success(), "{label}\n{}", text(&output));
            if mode[0].starts_with("--interpret") {
                assert_eq!(output.stdout, b"0\n", "{label}\n{}", text(&output));
            }
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

/// `__has_builtin` reports the builtin, and the type of its result is `int`.
#[test]
fn the_builtin_is_reported_and_has_type_int() {
    let work = Work::new("query");
    let path = work.source(
        "query.c",
        "#if !__has_builtin(__builtin_fpclassify)\n#error not reported\n#endif\n\
         _Static_assert(_Generic(__builtin_fpclassify(0, 1, 2, 3, 4, 1.0f), int: 1, default: 0), \"int\");\n\
         int main(void) { return 0; }\n",
    );
    for target in TARGETS {
        let output = evocc(&path, target, &["--check"]);
        assert!(output.status.success(), "{target}\n{}", text(&output));
    }
}

/// A class argument that is not an integer constant expression, an operand
/// that is not `float`, `double` or `long double`, and a wrong count.
#[test]
fn malformed_calls_keep_named_diagnostics() {
    let work = Work::new("malformed");
    for (name, body, code) in [
        (
            "arity",
            "return __builtin_fpclassify(0, 1, 2, 3, 4);",
            "E2909",
        ),
        (
            "variable-class",
            "return __builtin_fpclassify(n, 1, 2, 3, 4, 1.0);",
            "E2951",
        ),
        (
            "floating-class",
            "return __builtin_fpclassify(0.0, 1, 2, 3, 4, 1.0);",
            "E2951",
        ),
        (
            "integer-operand",
            "return __builtin_fpclassify(0, 1, 2, 3, 4, n);",
            "E2951",
        ),
        (
            "pointer-operand",
            "return __builtin_fpclassify(0, 1, 2, 3, 4, &n);",
            "E2951",
        ),
    ] {
        let path = work.source(
            &format!("{name}.c"),
            &format!("int f(int n) {{ {body} }}\nint main(void) {{ return f(0); }}\n"),
        );
        for target in TARGETS {
            let output = evocc(&path, target, &["-fsyntax-only"]);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(1), "{name} {target}\n{stderr}");
            assert!(
                stderr
                    .lines()
                    .next()
                    .is_some_and(|line| line.contains(code)),
                "{name} {target}\n{stderr}"
            );
        }
    }
}

/// glibc's `fpclassify` macro reaches the builtin.
const GLIBC_FPCLASSIFY: &str = r#"#include <float.h>
#include <math.h>
static volatile float tiny = FLT_TRUE_MIN;
static volatile double zero = 0.0, big = DBL_MAX;
static volatile long double one = 1.0L;
int main(void) {
    int wrong = 0;
    wrong += fpclassify(tiny) != FP_SUBNORMAL;
    wrong += fpclassify(-zero) != FP_ZERO;
    wrong += fpclassify(big * 2) != FP_INFINITE;
    wrong += fpclassify(zero / zero) != FP_NAN;
    wrong += fpclassify(one) != FP_NORMAL;
    wrong += fpclassify(LDBL_TRUE_MIN) != FP_SUBNORMAL;
    wrong += fpclassify((float)DBL_TRUE_MIN) != FP_ZERO;
    return wrong;
}
"#;

#[test]
fn linux_math_header_fpclassify_runs_in_every_execution_path() {
    let work = Work::new("glibc");
    let path = work.source("glibc.c", GLIBC_FPCLASSIFY);
    for mode in EXECUTE {
        let output = evocc(&path, LINUX, mode);
        let label = format!("{mode:?}");
        assert!(output.status.success(), "{label}\n{}", text(&output));
        if mode[0].starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n", "{label}\n{}", text(&output));
        }
    }
}

/// Darwin: native programs agree with Apple clang at -O0 and -O2. The SDK
/// `fpclassify` calls library functions, so it runs natively only.
#[cfg(target_os = "macos")]
#[test]
fn darwin_native_classes_agree_with_apple_clang() {
    let work = Work::new("darwin");
    for (name, source) in [("classes", CLASSES), ("sdk", GLIBC_FPCLASSIFY)] {
        let path = work.source(&format!("{name}.c"), source);
        let reference = work.0.join(format!("{name}-clang"));
        let output = Command::new("clang")
            .args(["-std=gnu11", "-w", "-O2"])
            .arg(&path)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        assert!(output.status.success(), "{name}\n{}", text(&output));
        let status = Command::new(&reference).status().unwrap();
        assert_eq!(status.code(), Some(0), "{name} Apple clang");
        for level in ["-O0", "-O2"] {
            let object = work.0.join(format!("{name}{level}.o"));
            let output = evocc(
                &path,
                DARWIN,
                &[level, "-c", "-o", object.to_str().unwrap()],
            );
            assert!(output.status.success(), "{name} {level}\n{}", text(&output));
            let program = work.0.join(format!("{name}{level}"));
            let output = Command::new("clang")
                .arg(&object)
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(output.status.success(), "{name} {level}\n{}", text(&output));
            let status = Command::new(&program).status().unwrap();
            assert_eq!(status.code(), Some(0), "{name} {level}");
        }
    }
}

/// Linux: EvoCC and GCC 14 programs agree in the sealed image at -O0 and -O2,
/// for the builtin and for glibc's `fpclassify`.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_classes_agree_with_gcc_14() {
    let work = Work::new("linux");
    let mut script = String::from("set -u\ncd /work\nfailed=0\n");
    let mut lines = 0;
    for (name, source) in [("classes", CLASSES), ("glibc", GLIBC_FPCLASSIFY)] {
        let path = work.source(&format!("{name}.c"), source);
        for level in ["-O0", "-O2"] {
            let assembly = work.0.join(format!("{name}{level}.s"));
            let output = evocc(
                &path,
                LINUX,
                &[level, "-S", "-o", assembly.to_str().unwrap()],
            );
            assert!(output.status.success(), "{name} {level}\n{}", text(&output));
            script.push_str(&format!(
                "gcc-14 -std=gnu11 -w {level} {name}.c -lm -o {name}-gcc{level} && ./{name}-gcc{level}; s=$?; echo \"{name} {level} gcc-14 $s\"; [ $s -eq 0 ] || failed=1\n\
                 gcc-14 {name}{level}.s -lm -o {name}-evocc{level} && ./{name}-evocc{level}; s=$?; echo \"{name} {level} evocc $s\"; [ $s -eq 0 ] || failed=1\n"
            ));
            lines += 2;
        }
    }
    script.push_str("exit $failed\n");
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        stdout.lines().filter(|line| line.ends_with(" 0")).count(),
        lines,
        "{stdout}"
    );
}
