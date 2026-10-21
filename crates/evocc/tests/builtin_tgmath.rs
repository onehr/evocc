//! `__builtin_tgmath` (EvoIR 10.1.14, D20), as GCC 14 documents it. The
//! leading arguments are pointers to prototyped functions with the same
//! parameter count, and the remaining arguments go to the one that the
//! `<tgmath.h>` rules select: an integer argument counts as `double`, and the
//! usual arithmetic conversions combine the type-generic arguments. Each
//! argument is evaluated once. glibc's `<tgmath.h>` defines its macros with
//! the builtin when the compiler reports GCC 8 or later; before D20 a call such
//! as `sqrt(x)` was E2949. Since L04, a selection of a `float`, `double` or
//! `long double` complex function is an ordinary call; a selection of another
//! complex function, such as a `_Float32 _Complex` one, stays E2959.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];

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
            "evocc-builtin-tgmath-{label}-{}",
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
            eprintln!("retained failed tgmath probe: {}", self.0.display());
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

/// Functions that report which one the builtin called, so both interpreters
/// can run the selection. `main` returns the number of wrong selections.
const SELECTION: &str = r#"static int calls;
static int tag_f(float x) { calls++; return 1; }
static int tag_d(double x) { calls++; return 2; }
static int tag_l(long double x) { calls++; return 3; }
static float twice_f(float x) { return x * 2; }
static double twice_d(double x) { return x * 2; }
static long double twice_l(long double x) { return x * 2; }
static int pair_f(float x, float y) { return 10; }
static int pair_d(double x, double y) { return 20; }
static int pair_l(long double x, long double y) { return 30; }
static int three_f(float x, float y, float z) { return 100; }
static int three_d(double x, double y, double z) { return 200; }
static int three_l(long double x, long double y, long double z) { return 300; }
static float split_f(float x, int *e) { *e = 1; return x; }
static double split_d(double x, int *e) { *e = 2; return x; }
static long double split_l(long double x, int *e) { *e = 3; return x; }
static float scale_f(float x, int n) { return x + n; }
static double scale_d(double x, int n) { return x + n * 10; }
static long double scale_l(long double x, int n) { return x + n * 100; }
static float narrow_d(double x) { return 4; }
static float narrow_l(long double x) { return 5; }
#define TAG(x) __builtin_tgmath(tag_f, tag_d, tag_l, x)
#define TWICE(x) __builtin_tgmath(twice_f, twice_d, twice_l, x)
#define PAIR(x, y) __builtin_tgmath(pair_f, pair_d, pair_l, x, y)
#define THREE(x, y, z) __builtin_tgmath(three_f, three_d, three_l, x, y, z)
#define SPLIT(x, e) __builtin_tgmath(split_f, split_d, split_l, x, e)
#define SCALE(x, n) __builtin_tgmath(scale_f, scale_d, scale_l, x, n)
#define NARROW(x) __builtin_tgmath(narrow_d, narrow_l, x)
_Static_assert(_Generic(TWICE(1.0f), float: 1, default: 0), "float");
_Static_assert(_Generic(TWICE(1.0), double: 1, default: 0), "double");
_Static_assert(_Generic(TWICE(1.0L), long double: 1, default: 0), "long double");
_Static_assert(_Generic(TWICE(1), double: 1, default: 0), "int as double");
_Static_assert(_Generic(TWICE(TWICE(1.0f)), float: 1, default: 0), "nested");
_Static_assert(_Generic(SPLIT(1.0f, 0), float: 1, default: 0), "fixed pointer");
_Static_assert(_Generic(NARROW(1.0L), float: 1, default: 0), "narrowing");
int main(void) {
    int wrong = 0, e = 0, n = 0;
    volatile float f = 1.5f;
    volatile long double l = 2.0L;
    wrong += TAG(1.0f) != 1;
    wrong += TAG(f) != 1;
    wrong += TAG(1.0) != 2;
    wrong += TAG(1.0L) != 3;
    wrong += TAG(l) != 3;
    wrong += TAG(1) != 2;
    wrong += TAG('a') != 2;
    wrong += TAG(1u) != 2;
    wrong += PAIR(1.0f, f) != 10;
    wrong += PAIR(1.0f, 1.0) != 20;
    wrong += PAIR(f, 1) != 20;
    wrong += PAIR(1.0f, l) != 30;
    wrong += PAIR(1, 2) != 20;
    wrong += THREE(f, 2.0f, 3.0f) != 100;
    wrong += THREE(1.0f, 2.0f, 3) != 200;
    wrong += THREE(1.0f, 2.0L, f) != 300;
    wrong += SPLIT(f, &e) != 1.5f || e != 1;
    wrong += SPLIT(2.5, &e) != 2.5 || e != 2;
    wrong += SPLIT(l, &e) != 2.0L || e != 3;
    wrong += SPLIT(7, &e) != 7.0 || e != 2;
    wrong += SCALE(f, 1) != 2.5f;
    wrong += SCALE(1.0, 2) != 21.0;
    wrong += SCALE(l, 3) != 302.0L;
    wrong += NARROW(1.0f) != 4;
    wrong += NARROW(1.0) != 4;
    wrong += NARROW(1.0L) != 5;
    wrong += NARROW(1) != 4;
    wrong += TWICE(TWICE(f)) != 6.0f;
    calls = 0;
    wrong += TAG((n++, 2.0)) != 2 || n != 1 || calls != 1;
    return wrong;
}
"#;

#[test]
fn selection_follows_the_tgmath_rules_in_every_execution_path() {
    let work = Work::new("selection");
    let path = work.source("selection.c", SELECTION);
    for target in TARGETS {
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

/// Function lists and arguments that GCC 14 refuses, and selections of a
/// complex function whose type has no admitted values.
#[test]
fn malformed_and_complex_calls_keep_named_diagnostics() {
    let work = Work::new("diagnostics");
    let prelude = "float tf(float); double td(double); long double tl(long double);\n\
                   int sf(float); int sd(double);\n\
                   _Complex double cd(_Complex double); _Complex float cf(_Complex float);\n\
                   _Complex long double cl(_Complex long double);\n\
                   _Float32 _Complex c32(_Float32 _Complex); _Float32 t32(_Float32);\n\
                   double ad(_Complex double); float af(_Complex float); long double al(_Complex long double);\n";
    for (name, call, code) in [
        ("too-few", "__builtin_tgmath(tf, 1.0f)", "E2909"),
        (
            "argument-as-function",
            "__builtin_tgmath(tf, 1.0f, 2.0f)",
            "E2951",
        ),
        ("not-a-function", "__builtin_tgmath(x, td, 1.0)", "E2951"),
        ("same-types", "__builtin_tgmath(td, td, 1.0)", "E2951"),
        (
            "pointer-argument",
            "__builtin_tgmath(tf, td, tl, &x)",
            "E2951",
        ),
        ("no-match", "__builtin_tgmath(sf, sd, 1.0L)", "E2951"),
        (
            "interchange-complex",
            "__builtin_tgmath(tf, td, tl, t32, cf, cd, cl, c32, w)",
            "E2959",
        ),
    ] {
        let path = work.source(
            &format!("{name}.c"),
            &format!(
                "{prelude}int x; _Complex double z; _Float32 _Complex w;\n\
                 int f(void) {{ return (int)({call}); }}\nint main(void) {{ return 0; }}\n"
            ),
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
                "{name} {target}: expected {code}\n{stderr}"
            );
        }
    }
}

/// Complex arguments and complex-only lists select complex functions, as
/// GCC 14 does: a complex argument asks for the complex function of the
/// combined real type, and an integer complex argument counts as
/// `_Complex double`. Both interpreters run the selected functions.
const COMPLEX_SELECTION: &str = r#"static int tag_f(float x) { return 1; }
static int tag_d(double x) { return 2; }
static int tag_l(long double x) { return 3; }
static int tag_cf(float _Complex x) { return 11; }
static int tag_cd(double _Complex x) { return 12; }
static int tag_cl(long double _Complex x) { return 13; }
static float part_cf(float _Complex x) { return __imag__ x; }
static double part_cd(double _Complex x) { return __imag__ x * 10; }
static long double part_cl(long double _Complex x) { return __imag__ x * 100; }
static double _Complex twice_cd(double _Complex x) { return x * 2; }
static float _Complex twice_cf(float _Complex x) { return x * 2; }
static long double _Complex twice_cl(long double _Complex x) { return x * 2; }
#define TAG(x) __builtin_tgmath(tag_f, tag_d, tag_l, tag_cf, tag_cd, tag_cl, x)
#define PART(x) __builtin_tgmath(part_cf, part_cd, part_cl, x)
#define TWICE(x) __builtin_tgmath(twice_cf, twice_cd, twice_cl, x)
_Static_assert(_Generic(TWICE(1.0f), float _Complex: 1, default: 0), "real to complex");
_Static_assert(_Generic(PART(1.0), double: 1, default: 0), "complex-only list");
int main(void) {
    int wrong = 0;
    volatile float _Complex f = 1.0f + 2.0if;
    volatile long double _Complex l = 3.0L;
    volatile _Complex int i = 2 + 3i;
    wrong += TAG(1.0f) != 1;
    wrong += TAG(f) != 11;
    wrong += TAG(1.0 + 2.0i) != 12;
    wrong += TAG(l) != 13;
    wrong += TAG(i) != 12;
    wrong += PART(f) != 2.0f;
    wrong += PART(1.0) != 0;
    wrong += PART(i) != 30;
    wrong += PART(l) != 0;
    wrong += __imag__ TWICE(f) != 4.0f;
    wrong += __real__ TWICE(2) != 4.0;
    return wrong;
}
"#;

#[test]
fn complex_selections_follow_the_tgmath_rules() {
    let work = Work::new("complex-selection");
    let path = work.source("complex.c", COMPLEX_SELECTION);
    for target in TARGETS {
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

/// glibc's `<tgmath.h>`: unary, binary and ternary macros, `frexp`, `ldexp`
/// and nested calls, for `float`, `double`, `long double` and `int`
/// arguments. Each line prints the size of the result type and its value.
const GLIBC_TGMATH: &str = r#"#include <stdio.h>
#include <tgmath.h>
static volatile float vf = 2.0f;
static volatile double vd = 3.0;
static volatile long double vl = 5.0L;
static volatile int vi = 7;
#define SHOW(e) printf("%-34s %zu %La\n", #e, sizeof(e), (long double)(e))
int main(void) {
    float f = vf;
    double d = vd;
    long double l = vl;
    int i = vi;
    int e1 = 0, e2 = 0, e3 = 0, e4 = 0;
    SHOW(sqrt(f)); SHOW(sqrt(d)); SHOW(sqrt(l)); SHOW(sqrt(i));
    SHOW(sin(f)); SHOW(exp(d)); SHOW(log(l)); SHOW(cbrt(i));
    SHOW(fabs(-f)); SHOW(floor(l / 3)); SHOW(lrint(l)); SHOW(round(f / 3));
    SHOW(pow(f, f)); SHOW(pow(f, d)); SHOW(pow(i, f)); SHOW(pow(l, i));
    SHOW(atan2(f, i)); SHOW(fmod(l, d)); SHOW(hypot(i, i)); SHOW(copysign(f, -1));
    SHOW(fma(f, f, f)); SHOW(fma(f, d, f)); SHOW(fma(i, i, l)); SHOW(fma(i, i, i));
    SHOW(frexp(f * 3, &e1)); SHOW(frexp(d * 3, &e2));
    SHOW(frexp(l * 3, &e3)); SHOW(frexp(i * 3, &e4));
    printf("%d %d %d %d\n", e1, e2, e3, e4);
    SHOW(ldexp(f, 3)); SHOW(ldexp(d, i)); SHOW(ldexp(l, -2)); SHOW(ldexp(i, 4));
    SHOW(sqrt(pow(f, 2))); SHOW(pow(sqrt(d), fabs(f)));
    SHOW(fma(sqrt(l), pow(i, i), sqrt(f))); SHOW(ldexp(sqrt(i), frexp(d, &e1) > 0));
    return 0;
}
"#;

#[test]
fn linux_tgmath_header_calls_compile_at_both_levels() {
    let work = Work::new("glibc");
    let path = work.source("tgmath.c", GLIBC_TGMATH);
    for level in ["-O0", "-O2"] {
        let output = evocc(&path, LINUX, &[level, "--emit=asm"]);
        assert!(output.status.success(), "{level}\n{}", text(&output));
    }
}

/// Linux: EvoCC and GCC 14 programs print the same lines in the sealed image
/// at -O0 and -O2.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_tgmath_calls_agree_with_gcc_14() {
    let work = Work::new("linux");
    let path = work.source("tgmath.c", GLIBC_TGMATH);
    let mut script = String::from("set -eu\ncd /work\n");
    for level in ["-O0", "-O2"] {
        let assembly = work.0.join(format!("tgmath{level}.s"));
        let output = evocc(
            &path,
            LINUX,
            &[level, "-S", "-o", assembly.to_str().unwrap()],
        );
        assert!(output.status.success(), "{level}\n{}", text(&output));
        script.push_str(&format!(
            "gcc-14 -std=gnu11 -w {level} tgmath.c -lm -o gcc{level}\n\
             gcc-14 tgmath{level}.s -lm -o evocc{level}\n\
             ./gcc{level} > gcc{level}.txt\n\
             ./evocc{level} > evocc{level}.txt\n\
             cmp gcc{level}.txt evocc{level}.txt\n\
             echo \"{level} identical $(wc -l < gcc{level}.txt) lines\"\n"
        ));
    }
    script.push_str("cat evocc-O2.txt\n");
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
    for level in ["-O0", "-O2"] {
        assert!(
            stdout.contains(&format!("{level} identical 37 lines")),
            "{stdout}"
        );
    }
}
