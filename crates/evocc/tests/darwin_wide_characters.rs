//! The wide character types of each target (EvoIR 10.5, D22). On
//! `aarch64-apple-darwin`, `wchar_t` and `wint_t` are `int`, as Apple clang
//! defines them; `-fshort-wchar` makes `wchar_t` `unsigned short`. On
//! `aarch64-unknown-linux-gnu`, both stay GCC 14's `unsigned int`. The type
//! of `L'x'` and of the elements of `L"..."` is the type of `wchar_t`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];

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

fn succeeded(output: &Output, label: &str) {
    assert!(output.status.success(), "{label}\n{}", text(output));
}

/// A fresh directory, removed on success.
struct Work(PathBuf);

impl Work {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("evocc-darwin-wide-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    /// One driver action for `target`, with the checked-in sysroot on Linux.
    fn evocc(&self, target: &str, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
        command
            .current_dir(&self.0)
            .arg(format!("--target={target}"))
            .arg("--error-format=short");
        if target == LINUX {
            command.arg(format!(
                "--sysroot={}",
                root().join("sysroot").join(LINUX).display()
            ));
        }
        command.args(args).output().unwrap()
    }

    /// The cc driver for the host target, as a build system invokes it.
    #[cfg(target_os = "macos")]
    fn cc(&self, args: &[&str]) -> Output {
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

    #[cfg(target_os = "macos")]
    fn run(&self, name: &str) -> Output {
        Command::new(self.0.join(name)).output().unwrap()
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

/// Run `main` of `source` in both interpreters and require a zero result.
fn interpret_to_zero(work: &Work, target: &str, flags: &[&str], name: &str, source: &str) {
    let path = work.write(name, source);
    let path = path.to_str().unwrap();
    for mode in INTERPRETERS {
        let mut args = flags.to_vec();
        args.extend([mode, path]);
        let output = work.evocc(target, &args);
        let label = format!("{target} {flags:?} {mode}");
        succeeded(&output, &label);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "0",
            "{label}: each set bit names a failed check\n{}",
            text(&output)
        );
    }
}

/// Each failed check sets one bit of the result of `main`.
const DARWIN_TYPES: &str = r#"
typedef __WCHAR_TYPE__ wchar_t;
typedef __WINT_TYPE__ wint_t;
static const wchar_t text[] = L"\xffffffff" L"a";
int main(void)
{
    int bad = 0;
    bad |= !((wchar_t)-1 < 0) << 0;
    bad |= (__WCHAR_MAX__ != 2147483647) << 1;
    bad |= !__builtin_types_compatible_p(wchar_t, int) << 2;
    bad |= !__builtin_types_compatible_p(__typeof__(L'x'), int) << 3;
    bad |= !__builtin_types_compatible_p(__typeof__(*L"x"), int) << 4;
    bad |= (text[0] != -1 || text[1] != 'a' || text[2] != 0) << 5;
    bad |= (sizeof text != 12 || sizeof(wchar_t) != 4) << 6;
    bad |= !(-L'a' < 0) << 7;
    bad |= ((long)(wchar_t)-2 != -2) << 8;
    bad |= !__builtin_types_compatible_p(wint_t, int) << 9;
    bad |= (__WINT_MAX__ != 2147483647 || (wint_t)-1 >= 0) << 10;
    bad |= (__WCHAR_WIDTH__ != 32 || __SIZEOF_WCHAR_T__ != 4) << 11;
    wchar_t local[] = L"\x80000000" L"é";
    bad |= (local[0] >= 0 || local[0] != -2147483647 - 1) << 12;
    bad |= (local[1] != 0xe9 || L'é' != 0xe9) << 13;
    bad |= _Generic(L'x', int: 0, default: 1) << 14;
    bad |= _Generic(L"x"[0], int: 0, default: 1) << 15;
    int widened[] = L"ab";
    bad |= (sizeof widened != 12 || widened[1] != 'b') << 16;
#if defined(__WCHAR_MIN__) || defined(__WINT_MIN__) || defined(__WCHAR_UNSIGNED__)
    bad |= 1 << 17;
#endif
    return bad;
}
"#;

/// Darwin with `-fshort-wchar`: `wchar_t` is `unsigned short`, as Apple
/// clang defines it, and `wint_t` stays `int`.
const DARWIN_SHORT_TYPES: &str = r#"
typedef __WCHAR_TYPE__ wchar_t;
typedef __WINT_TYPE__ wint_t;
int main(void)
{
    int bad = 0;
    bad |= ((wchar_t)-1 != 65535 || (wchar_t)-1 < 0) << 0;
    bad |= (__WCHAR_MAX__ != 65535 || __WCHAR_UNSIGNED__ != 1) << 1;
    bad |= !__builtin_types_compatible_p(wchar_t, unsigned short) << 2;
    bad |= !__builtin_types_compatible_p(__typeof__(*L"x"), unsigned short) << 3;
    bad |= (sizeof(L"\U0001f642") != 6 || L"\U0001f642"[0] != 0xd83d) << 4;
    bad |= !__builtin_types_compatible_p(wint_t, int) << 5;
    bad |= (__WCHAR_WIDTH__ != 16 || __SIZEOF_WCHAR_T__ != 2) << 6;
#ifdef __WCHAR_MIN__
    bad |= 1 << 7;
#endif
    return bad;
}
"#;

/// The Linux types stay GCC 14's `unsigned int`.
const LINUX_TYPES: &str = r#"
typedef __WCHAR_TYPE__ wchar_t;
typedef __WINT_TYPE__ wint_t;
static const wchar_t text[] = L"\xffffffff" L"a";
int main(void)
{
    int bad = 0;
    bad |= ((wchar_t)-1 < 0) << 0;
    bad |= (__WCHAR_MAX__ != 4294967295U || __WCHAR_MIN__ != 0U) << 1;
    bad |= !__builtin_types_compatible_p(wchar_t, unsigned int) << 2;
    bad |= !__builtin_types_compatible_p(__typeof__(L'x'), unsigned int) << 3;
    bad |= !__builtin_types_compatible_p(__typeof__(*L"x"), unsigned int) << 4;
    bad |= (text[0] != 0xffffffffU || text[1] != 'a') << 5;
    bad |= (-L'a' < 0) << 6;
    bad |= ((long)(wchar_t)-2 != 4294967294L) << 7;
    bad |= !__builtin_types_compatible_p(wint_t, unsigned int) << 8;
    bad |= (__WINT_MAX__ != 4294967295U || __WINT_MIN__ != 0U) << 9;
    bad |= _Generic(L'x', unsigned int: 0, default: 1) << 10;
#ifdef __WCHAR_UNSIGNED__
    bad |= 1 << 11;
#endif
    return bad;
}
"#;

#[test]
fn darwin_wchar_t_and_wint_t_are_int_in_both_interpreters() {
    let work = Work::new("types");
    interpret_to_zero(&work, DARWIN, &[], "types.c", DARWIN_TYPES);
}

#[test]
fn darwin_short_wchar_is_unsigned_short_in_both_interpreters() {
    let work = Work::new("short");
    interpret_to_zero(
        &work,
        DARWIN,
        &["-fshort-wchar"],
        "short.c",
        DARWIN_SHORT_TYPES,
    );
}

#[test]
fn linux_keeps_the_unsigned_int_wchar_t_in_both_interpreters() {
    let work = Work::new("linux");
    interpret_to_zero(&work, LINUX, &[], "linux.c", LINUX_TYPES);
}

/// Each macro name and its replacement, both stringized, or `undefined`.
const SPELLINGS: &str = r#"
#define S(x) #x
#define V(x) S(x)
#define NAME(m) #m = V(m)
NAME(__WCHAR_TYPE__)
NAME(__WCHAR_MAX__)
NAME(__WCHAR_WIDTH__)
NAME(__SIZEOF_WCHAR_T__)
NAME(__ARM_SIZEOF_WCHAR_T)
NAME(__WINT_TYPE__)
NAME(__WINT_MAX__)
NAME(__WINT_WIDTH__)
NAME(__SIZEOF_WINT_T__)
#ifdef __WCHAR_MIN__
NAME(__WCHAR_MIN__)
#else
"__WCHAR_MIN__" = "undefined"
#endif
#ifdef __WINT_MIN__
NAME(__WINT_MIN__)
#else
"__WINT_MIN__" = "undefined"
#endif
#ifdef __WCHAR_UNSIGNED__
NAME(__WCHAR_UNSIGNED__)
#else
"__WCHAR_UNSIGNED__" = "undefined"
#endif
"#;

/// The `"NAME" = "value"` pairs of preprocessed output, in source order.
/// The pairs are read from the string literals alone, because the line
/// structure of `-E` output is not part of its contract.
fn spellings(output: &Output) -> Vec<(String, String)> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let tokens: String = stdout
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect();
    let literals: Vec<&str> = tokens.split('"').skip(1).step_by(2).collect();
    literals
        .chunks(2)
        .filter_map(|pair| match pair {
            [name, value] => Some(((*name).to_owned(), (*value).to_owned())),
            _ => None,
        })
        .collect()
}

fn expected(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

/// The spellings that `xcrun clang -E -dM -x c /dev/null` prints for
/// arm64 macOS: Apple clang 21.0.0 on 2026-10-21.
const DARWIN_SPELLINGS: [(&str, &str); 12] = [
    ("__WCHAR_TYPE__", "int"),
    ("__WCHAR_MAX__", "2147483647"),
    ("__WCHAR_WIDTH__", "32"),
    ("__SIZEOF_WCHAR_T__", "4"),
    ("__ARM_SIZEOF_WCHAR_T", "4"),
    ("__WINT_TYPE__", "int"),
    ("__WINT_MAX__", "2147483647"),
    ("__WINT_WIDTH__", "32"),
    ("__SIZEOF_WINT_T__", "4"),
    ("__WCHAR_MIN__", "undefined"),
    ("__WINT_MIN__", "undefined"),
    ("__WCHAR_UNSIGNED__", "undefined"),
];

/// The same query with `-fshort-wchar`.
const DARWIN_SHORT_SPELLINGS: [(&str, &str); 12] = [
    ("__WCHAR_TYPE__", "unsigned short"),
    ("__WCHAR_MAX__", "65535"),
    ("__WCHAR_WIDTH__", "16"),
    ("__SIZEOF_WCHAR_T__", "2"),
    ("__ARM_SIZEOF_WCHAR_T", "2"),
    ("__WINT_TYPE__", "int"),
    ("__WINT_MAX__", "2147483647"),
    ("__WINT_WIDTH__", "32"),
    ("__SIZEOF_WINT_T__", "4"),
    ("__WCHAR_MIN__", "undefined"),
    ("__WINT_MIN__", "undefined"),
    ("__WCHAR_UNSIGNED__", "1"),
];

/// GCC 14's spellings for AArch64 Linux, as the sealed predefined list of
/// `crates/evocc-cpp/predefined/aarch64-unknown-linux-gnu.h` records them.
const LINUX_SPELLINGS: [(&str, &str); 12] = [
    ("__WCHAR_TYPE__", "unsigned int"),
    ("__WCHAR_MAX__", "0xffffffffU"),
    ("__WCHAR_WIDTH__", "32"),
    ("__SIZEOF_WCHAR_T__", "4"),
    ("__ARM_SIZEOF_WCHAR_T", "4"),
    ("__WINT_TYPE__", "unsigned int"),
    ("__WINT_MAX__", "0xffffffffU"),
    ("__WINT_WIDTH__", "32"),
    ("__SIZEOF_WINT_T__", "4"),
    ("__WCHAR_MIN__", "0U"),
    ("__WINT_MIN__", "0U"),
    ("__WCHAR_UNSIGNED__", "undefined"),
];

#[test]
fn the_wide_macros_are_spelled_as_each_target_compiler_prints_them() {
    let work = Work::new("spellings");
    work.write("spellings.c", SPELLINGS);
    for (target, flags, pairs) in [
        (DARWIN, &[][..], &DARWIN_SPELLINGS),
        (DARWIN, &["-fshort-wchar"][..], &DARWIN_SHORT_SPELLINGS),
        (LINUX, &[][..], &LINUX_SPELLINGS),
    ] {
        let mut args = flags.to_vec();
        args.extend(["-E", "spellings.c"]);
        let output = work.evocc(target, &args);
        succeeded(&output, target);
        assert_eq!(
            spellings(&output),
            expected(pairs),
            "{target} {flags:?}\n{}",
            text(&output)
        );
    }
}

/// Apple clang prints the same spellings on this machine.
#[cfg(target_os = "macos")]
#[test]
fn the_darwin_wide_macros_match_apple_clang() {
    let work = Work::new("clang-spellings");
    work.write("spellings.c", SPELLINGS);
    for flags in [&[][..], &["-fshort-wchar"][..]] {
        let mut args = flags.to_vec();
        args.extend(["-E", "-P", "spellings.c"]);
        let theirs = work.clang(&args);
        succeeded(&theirs, "clang -E");
        let mut args = flags.to_vec();
        args.extend(["-E", "spellings.c"]);
        let mine = work.evocc(DARWIN, &args);
        succeeded(&mine, "evocc -E");
        assert_eq!(spellings(&mine), spellings(&theirs), "{flags:?}");
        assert_eq!(spellings(&mine).len(), 12, "{flags:?}\n{}", text(&mine));
    }
}

/// A program that prints the wide limits of the SDK headers and calls the
/// C library with wide strings.
#[cfg(target_os = "macos")]
const LIMITS: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <wchar.h>
#include <wctype.h>
int main(void)
{
    volatile wchar_t minus = (wchar_t)-1;
    printf("%d %lld %lld %zu\n", minus < 0, (long long)WCHAR_MIN,
           (long long)WCHAR_MAX, sizeof(wchar_t));
    printf("%lld %lld %d\n", (long long)WINT_MIN, (long long)WINT_MAX,
           WEOF < 0);
    const wchar_t *text = L"été \U0001f642";
    printf("%zu %d %d\n", wcslen(text), wcscmp(L"ab", L"ac") < 0,
           wcschr(text, L't') == text + 1);
    printf("%d %ld\n", towupper(L'a') == L'A', (long)minus);
    return 0;
}
"#;

#[cfg(target_os = "macos")]
#[test]
fn native_darwin_wide_characters_match_apple_clang() {
    let work = Work::new("native");
    work.write("limits.c", LIMITS);
    for level in ["-O0", "-O2"] {
        succeeded(
            &work.cc(&[level, "limits.c", "-o", "limits-evocc"]),
            "evocc",
        );
        succeeded(
            &work.clang(&[level, "limits.c", "-o", "limits-clang"]),
            "clang",
        );
        let mine = work.run("limits-evocc");
        let theirs = work.run("limits-clang");
        succeeded(&mine, "EvoCC program");
        succeeded(&theirs, "clang program");
        assert_eq!(
            String::from_utf8_lossy(&mine.stdout),
            String::from_utf8_lossy(&theirs.stdout),
            "{level}"
        );
        assert!(
            String::from_utf8_lossy(&mine.stdout).starts_with("1 -2147483648 2147483647 4\n"),
            "{level}: {}",
            String::from_utf8_lossy(&mine.stdout)
        );
    }
}

/// `wchar_t` arguments, results, comparisons and wide string elements cross
/// calls between EvoCC and Apple clang.
#[cfg(target_os = "macos")]
const CALLEE: &str = r#"
#include <stddef.h>
wchar_t smaller(wchar_t a, wchar_t b) { return a < b ? a : b; }
long widen(wchar_t c) { return c; }
int negative(wchar_t c) { return c < 0; }
wchar_t pick(const wchar_t *s, int i) { return s[i]; }
const wchar_t *table(void) { return L"\xffffffff" L"b"; }
"#;

#[cfg(target_os = "macos")]
const CALLER: &str = r#"
#include <stddef.h>
wchar_t smaller(wchar_t, wchar_t);
long widen(wchar_t);
int negative(wchar_t);
wchar_t pick(const wchar_t *, int);
const wchar_t *table(void);
int main(void)
{
    if (smaller((wchar_t)-1, L'a') != (wchar_t)-1 || smaller(L'z', L'a') != L'a')
        return 1;
    if (widen((wchar_t)-5) != -5 || widen(L'é') != 0xe9)
        return 2;
    if (!negative((wchar_t)-1) || negative(L'z'))
        return 3;
    if (pick(L"\xffffffff" L"b", 0) != -1 || pick(L"ab", 1) != L'b')
        return 4;
    if (table()[0] >= 0 || table()[1] != L'b' || table()[2] != 0)
        return 5;
    return 42;
}
"#;

#[cfg(target_os = "macos")]
#[test]
fn wchar_t_calls_agree_with_apple_clang_in_both_directions() {
    let work = Work::new("crossed");
    work.write("callee.c", CALLEE);
    work.write("caller.c", CALLER);
    for level in ["-O0", "-O2"] {
        // EvoCC callee, clang caller.
        succeeded(
            &work.cc(&[level, "-c", "callee.c", "-o", "callee.o"]),
            "evocc callee",
        );
        succeeded(
            &work.clang(&[level, "-c", "caller.c", "-o", "caller.o"]),
            "clang caller",
        );
        succeeded(
            &work.clang(&["caller.o", "callee.o", "-o", "first"]),
            "link",
        );
        let run = work.run("first");
        assert_eq!(run.status.code(), Some(42), "{level} EvoCC callee: {run:?}");
        // clang callee, EvoCC caller.
        succeeded(
            &work.clang(&[level, "-c", "callee.c", "-o", "callee.o"]),
            "clang callee",
        );
        succeeded(
            &work.cc(&[level, "-c", "caller.c", "-o", "caller.o"]),
            "evocc caller",
        );
        succeeded(
            &work.clang(&["caller.o", "callee.o", "-o", "second"]),
            "link",
        );
        let run = work.run("second");
        assert_eq!(run.status.code(), Some(42), "{level} EvoCC caller: {run:?}");
    }
}
