//! The Darwin C data layout of `long double` (T03i, audit findings A05 and
//! A24). On `aarch64-apple-darwin`, `long double` is a distinct C type with
//! the IEEE binary64 format, size 8 and alignment 8, as under Apple clang.
//! Darwin EvoIR text carries its target, and the readers of the Linux format
//! refuse it.

use std::path::PathBuf;
use std::process::{Command, Output};

const DARWIN: &str = "--target=aarch64-apple-darwin";

/// Layout, type identity, constants and arithmetic. Each failed check sets
/// one bit of the exit status.
const LAYOUT: &str = r#"
struct s { char c; long double d; int x; };
struct pair { long double a, b; };
static const long double third_constant = 1.0L / 3;
static long double table[2] = { 0.1L, 1e300L };
int main(void)
{
    int bad = 0;
    bad |= (sizeof(long double) != 8) << 0;
    bad |= (__SIZEOF_LONG_DOUBLE__ != 8) << 1;
    bad |= (_Alignof(long double) != 8 || sizeof(struct pair) != 16) << 2;
    bad |= (__builtin_offsetof(struct s, x) != 16 || sizeof(struct s) != 24) << 3;
    bad |= (__LDBL_MANT_DIG__ != 53 || __LDBL_MAX__ != __DBL_MAX__
            || __LDBL_EPSILON__ != __DBL_EPSILON__) << 4;
    bad |= _Generic((long double)0, double: 1, long double: 0, default: 1) << 5;
    bad |= _Generic(1.0L + 1.0, double: 1, long double: 0, default: 1) << 5;
    volatile double one = 1.0, three = 3.0;
    volatile long double third = (long double)one / three;
    bad |= ((double)third != one / three || third != third_constant) << 6;
    bad |= (table[0] != 0.1 || table[1] * 10 != 1e301) << 7;
    return bad;
}
"#;

/// Host C library and math library calls with `long double` arguments,
/// results and variadic arguments.
#[cfg(target_os = "macos")]
const LIBRARY: &str = r#"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
int main(void)
{
    volatile long double one = 1.0L;
    long double x = one / 3, y = 0;
    long double a[3] = { 1.5L, -2.25L, 1e300L };
    printf("%.25Lg %La\n", x, x);
    if (sscanf("2.5e-3", "%Lf", &y) != 1)
        return 1;
    printf("%.25Lg %.25Lg\n", y * 4, sqrtl(2.0L));
    printf("%.25Lg %Lg %Lg %Lg\n", strtold("0.1", 0), a[0], a[1], a[2] * 10);
    return 0;
}
"#;

/// Calls in both directions between EvoCC and Apple clang, with scalar and
/// record arguments and results.
#[cfg(target_os = "macos")]
const CALLEE: &str = r#"
struct pair { long double a, b; };
long double scale(long double x, double y, long double z) { return x * y + z; }
struct pair swap(struct pair p) { struct pair r = { p.b, p.a * 2 }; return r; }
"#;

#[cfg(target_os = "macos")]
const CALLER: &str = r#"
struct pair { long double a, b; };
long double scale(long double, double, long double);
struct pair swap(struct pair);
int main(void)
{
    struct pair p = { 1.25L, 3.5L };
    struct pair q = swap(p);
    if (scale(1.5L, 2.0, 0.25L) != 3.25L || q.a != 3.5L || q.b != 2.5L)
        return 1;
    return 42;
}
"#;

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A fresh directory, removed on success.
struct Work(PathBuf);

impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-darwin-long-double-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).unwrap();
    }

    fn evocc(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_evocc"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }

    #[cfg(target_os = "macos")]
    fn clang(&self, args: &[&str]) -> Output {
        Command::new("clang")
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
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

/// Both interpreters execute the Darwin layout. Each prints the exit status
/// of `main`.
#[test]
fn both_interpreters_use_the_darwin_long_double() {
    let work = Work::new("interpreters");
    work.write("layout.c", LAYOUT);
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = work.evocc(&[DARWIN, mode, "layout.c"]);
        succeeded(&output);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "0",
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// The EvoIR of the Darwin target uses binary64 for `long double`, and its
/// text names the target in its first line.
#[test]
fn darwin_evoir_names_its_target_and_has_no_binary128() {
    let work = Work::new("evoir");
    work.write("layout.c", LAYOUT);
    let output = work.evocc(&[DARWIN, "--emit=evoir", "layout.c"]);
    succeeded(&output);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.starts_with("target aarch64-apple-darwin\n"),
        "{}",
        &text[..text.len().min(200)]
    );
    assert!(!text.contains("f128"), "{text}");
    let error = evocc_evoir::parse_section9_module(text.as_bytes())
        .expect_err("the Linux reader refuses Darwin text");
    assert!(error.contains("aarch64-apple-darwin"), "{error}");
    let module = evocc_evoir::parse_target_module(text.as_bytes(), "aarch64-apple-darwin")
        .expect("the Darwin reader accepts it");
    assert_eq!(
        evocc_evoir::render_target_module(&module, "aarch64-apple-darwin"),
        text
    );
    // The Linux text is unchanged: it has no target line.
    let linux = work.evocc(&[
        "--target=aarch64-unknown-linux-gnu",
        "--emit=evoir",
        "layout.c",
    ]);
    succeeded(&linux);
    let linux = String::from_utf8(linux.stdout).unwrap();
    assert!(!linux.starts_with("target "), "{linux}");
    evocc_evoir::parse_section9_module(linux.as_bytes()).expect("Linux text");
}

/// Scalar rewrites retain the Darwin profile even when the surrounding
/// program contains its binary64 long-double operations and objects.
#[test]
fn scalar_equality_saturation_preserves_the_darwin_layout() {
    let work = Work::new("eqsat");
    work.write("layout.c", LAYOUT);
    let pin = format!(
        "--eq-checker-sha256={}",
        evocc_proof::sha256_hex(&std::fs::read(env!("CARGO_BIN_EXE_evocc-checker")).unwrap())
    );
    for extra in [
        &["-Oeq-scalar", pin.as_str()][..],
        &["--eq-proof-dir=chain"][..],
    ] {
        let mut args = vec![DARWIN, "--emit=evoir"];
        args.extend_from_slice(extra);
        args.push("layout.c");
        let output = work.evocc(&args);
        succeeded(&output);
        evocc_evoir::parse_target_module(&output.stdout, "aarch64-apple-darwin").unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("f64"), "{text}");
        assert!(!text.contains("f128"), "{text}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn native_darwin_long_double_matches_apple_clang() {
    let work = Work::new("native");
    work.write("layout.c", LAYOUT);
    work.write("library.c", LIBRARY);
    for level in ["-O0", "-O2"] {
        succeeded(&work.evocc(&[level, "layout.c", "-o", "layout"]));
        let run = Command::new(work.0.join("layout")).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{level}: {run:?}");

        succeeded(&work.evocc(&[level, "library.c", "-o", "library-evocc"]));
        succeeded(&work.clang(&[level, "library.c", "-o", "library-clang"]));
        let mine = Command::new(work.0.join("library-evocc")).output().unwrap();
        let theirs = Command::new(work.0.join("library-clang")).output().unwrap();
        succeeded(&mine);
        succeeded(&theirs);
        assert_eq!(
            String::from_utf8_lossy(&mine.stdout),
            String::from_utf8_lossy(&theirs.stdout),
            "{level}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn long_double_calls_agree_with_apple_clang_in_both_directions() {
    let work = Work::new("crossed");
    work.write("callee.c", CALLEE);
    work.write("caller.c", CALLER);
    for level in ["-O0", "-O2"] {
        // EvoCC callee, clang caller.
        succeeded(&work.evocc(&[level, "-c", "callee.c", "-o", "callee.o"]));
        succeeded(&work.clang(&[level, "-c", "caller.c", "-o", "caller.o"]));
        succeeded(&work.clang(&["caller.o", "callee.o", "-o", "first"]));
        let run = Command::new(work.0.join("first")).output().unwrap();
        assert_eq!(run.status.code(), Some(42), "{level} EvoCC callee: {run:?}");
        // clang callee, EvoCC caller.
        succeeded(&work.clang(&[level, "-c", "callee.c", "-o", "callee.o"]));
        succeeded(&work.evocc(&[level, "-c", "caller.c", "-o", "caller.o"]));
        succeeded(&work.clang(&["caller.o", "callee.o", "-o", "second"]));
        let run = Command::new(work.0.join("second")).output().unwrap();
        assert_eq!(run.status.code(), Some(42), "{level} EvoCC caller: {run:?}");
    }
}
