//! N09k: a call without a prototype of a function with an old-style
//! definition follows C11 6.5.2.2p6 (EvoIR 3.10, 6.1 and 10.1.7). The
//! arguments take the default argument promotions, no argument type is
//! checked against a parameter type, and the call compiles as GCC 14
//! compiles it. Both interpreters and `--check` report E3068 at a call whose
//! argument count differs, or whose promoted argument type does not agree
//! with the promoted parameter type outside the two exceptions. With a
//! prototype in scope nothing changes. GCC 14.2.0-19 (evo-nexlab:sealed) and
//! Apple clang 21 gave the measured results on 2026-10-27: GCC 14 gives no
//! diagnostic for these calls in gnu89 to gnu17.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const MODES: [&str; 4] = ["-std=gnu89", "-std=gnu99", "-std=gnu11", "-std=gnu17"];
const INTERPRETERS: [&str; 3] = ["--interpret-c-hir", "--interpret-evoir", "--check"];
const MISMATCH: &str = "error[E3068]: old-style-call-mismatch";
static NEXT: AtomicU32 = AtomicU32::new(0);

/// The 25 measured sources (`out/n09k/reference/sources/`): the name, the
/// text, the call line and the output of a defined call.
const SOURCES: [(&str, &str, usize, Option<&str>); 25] = [
    (
        "match-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(a, d)
int a;
double d;
{
  printf("%d %.1f\n", a, d);
  return 0;
}

int main(void)
{
  f(1, 2.5);
  return 0;
}
"#,
        13,
        Some("1 2.5\n"),
    ),
    (
        "match-proto",
        r#"#include <stdio.h>
int f(int a, double d);
int f(a, d)
int a;
double d;
{
  printf("%d %.1f\n", a, d);
  return 0;
}

int main(void)
{
  f(1, 2.5);
  return 0;
}
"#,
        13,
        Some("1 2.5\n"),
    ),
    (
        "char-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(c)
char c;
{
  printf("%d\n", c);
  return 0;
}

int main(void)
{
  char x = 65;
  f(x);
  return 0;
}
"#,
        13,
        Some("65\n"),
    ),
    (
        "char-proto",
        r#"#include <stdio.h>
int f(char c);
int f(c)
char c;
{
  printf("%d\n", c);
  return 0;
}

int main(void)
{
  char x = 65;
  f(x);
  return 0;
}
"#,
        13,
        Some("65\n"),
    ),
    (
        "char-proto-promoted",
        r#"#include <stdio.h>
int f(int c);
int f(c)
char c;
{
  printf("%d\n", c);
  return 0;
}

int main(void)
{
  char x = 65;
  f(x);
  return 0;
}
"#,
        13,
        Some("65\n"),
    ),
    (
        "short-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(s)
short s;
{
  printf("%d\n", s);
  return 0;
}

int main(void)
{
  short x = 1234;
  f(x);
  return 0;
}
"#,
        13,
        Some("1234\n"),
    ),
    (
        "short-proto",
        r#"#include <stdio.h>
int f(short s);
int f(s)
short s;
{
  printf("%d\n", s);
  return 0;
}

int main(void)
{
  short x = 1234;
  f(x);
  return 0;
}
"#,
        13,
        Some("1234\n"),
    ),
    (
        "short-proto-promoted",
        r#"#include <stdio.h>
int f(int s);
int f(s)
short s;
{
  printf("%d\n", s);
  return 0;
}

int main(void)
{
  short x = 1234;
  f(x);
  return 0;
}
"#,
        13,
        Some("1234\n"),
    ),
    (
        "float-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(v)
float v;
{
  printf("%.6f\n", (double)v);
  return 0;
}

int main(void)
{
  float x = 1.5f;
  f(x);
  return 0;
}
"#,
        13,
        Some("1.500000\n"),
    ),
    (
        "float-proto",
        r#"#include <stdio.h>
int f(float v);
int f(v)
float v;
{
  printf("%.6f\n", (double)v);
  return 0;
}

int main(void)
{
  float x = 1.5f;
  f(x);
  return 0;
}
"#,
        13,
        Some("1.500000\n"),
    ),
    (
        "float-proto-promoted",
        r#"#include <stdio.h>
int f(double v);
int f(v)
float v;
{
  printf("%.6f\n", (double)v);
  return 0;
}

int main(void)
{
  float x = 1.5f;
  f(x);
  return 0;
}
"#,
        13,
        Some("1.500000\n"),
    ),
    (
        "ptr-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(ip)
short *ip;
{
  printf("%d\n", ip[1]);
  return 0;
}

short p[5] = {1, 2, 3, 4, 5};
int main(void)
{
  f(&p);
  return 0;
}
"#,
        13,
        None,
    ),
    (
        "ptr-proto",
        r#"#include <stdio.h>
int f(short *ip);
int f(ip)
short *ip;
{
  printf("%d\n", ip[1]);
  return 0;
}

short p[5] = {1, 2, 3, 4, 5};
int main(void)
{
  f(&p);
  return 0;
}
"#,
        13,
        None,
    ),
    (
        "sign-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(u)
unsigned u;
{
  printf("%u\n", u);
  return 0;
}

int main(void)
{
  f(42);
  return 0;
}
"#,
        12,
        Some("42\n"),
    ),
    (
        "sign-proto",
        r#"#include <stdio.h>
int f(unsigned u);
int f(u)
unsigned u;
{
  printf("%u\n", u);
  return 0;
}

int main(void)
{
  f(42);
  return 0;
}
"#,
        12,
        Some("42\n"),
    ),
    (
        "unsign-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(i)
int i;
{
  printf("%d\n", i);
  return 0;
}

int main(void)
{
  f(42u);
  return 0;
}
"#,
        12,
        Some("42\n"),
    ),
    (
        "unsign-proto",
        r#"#include <stdio.h>
int f(int i);
int f(i)
int i;
{
  printf("%d\n", i);
  return 0;
}

int main(void)
{
  f(42u);
  return 0;
}
"#,
        12,
        Some("42\n"),
    ),
    (
        "charptr-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(s)
char *s;
{
  printf("%s\n", s);
  return 0;
}

int main(void)
{
  void *v = "hi";
  f(v);
  return 0;
}
"#,
        13,
        Some("hi\n"),
    ),
    (
        "charptr-proto",
        r#"#include <stdio.h>
int f(char *s);
int f(s)
char *s;
{
  printf("%s\n", s);
  return 0;
}

int main(void)
{
  void *v = "hi";
  f(v);
  return 0;
}
"#,
        13,
        Some("hi\n"),
    ),
    (
        "voidptr-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(v)
void *v;
{
  printf("%s\n", (char *)v);
  return 0;
}

int main(void)
{
  char *s = "hi";
  f(s);
  return 0;
}
"#,
        13,
        Some("hi\n"),
    ),
    (
        "voidptr-proto",
        r#"#include <stdio.h>
int f(void *v);
int f(v)
void *v;
{
  printf("%s\n", (char *)v);
  return 0;
}

int main(void)
{
  char *s = "hi";
  f(s);
  return 0;
}
"#,
        13,
        Some("hi\n"),
    ),
    (
        "many-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(a)
int a;
{
  printf("%d\n", a);
  return 0;
}

int main(void)
{
  f(1, 2);
  return 0;
}
"#,
        12,
        None,
    ),
    (
        "many-proto",
        r#"#include <stdio.h>
int f(int a);
int f(a)
int a;
{
  printf("%d\n", a);
  return 0;
}

int main(void)
{
  f(1, 2);
  return 0;
}
"#,
        12,
        None,
    ),
    (
        "few-noproto",
        r#"#include <stdio.h>
/* no prototype in scope */
int f(a, b)
int a;
int b;
{
  printf("%d\n", a);
  return 0;
}

int main(void)
{
  f(1);
  return 0;
}
"#,
        13,
        None,
    ),
    (
        "few-proto",
        r#"#include <stdio.h>
int f(int a, int b);
int f(a, b)
int a;
int b;
{
  printf("%d\n", a);
  return 0;
}

int main(void)
{
  f(1);
  return 0;
}
"#,
        13,
        None,
    ),
];

/// GCC torture suite, `gcc/testsuite/gcc.c-torture/execute/920302-1.c`
/// (releases/gcc-16.2.0, pinned in `tools/ladder/e0.py`), byte for byte;
/// SHA-256 ebcf15377317e655ba378a31413ec0ca49e8f17c3c357d94de6af2d2a6514dc4.
const ORIGINAL_920302_1: &str = r#"/* { dg-require-effective-target label_values } */

void abort (void);
void exit (int);

short optab[5];
char buf[10];
void
execute (ip)
     register short *ip;
{
  register void *base = &&x;
  char *bp = buf;
  static void *tab[] = {&&x, &&y, &&z};
  if (ip == 0)
    {
      int i;
      for (i = 0; i < 3; ++i)
	optab[i] = (short)(tab[i] - base);
      return;
    }
x:  *bp++='x';
    goto *(base + *ip++);
y:  *bp++='y';
    goto *(base + *ip++);
z:  *bp++='z';
    *bp=0;
    return;
}

short p[5];

int
main (void)
{
  execute ((short *) 0);
  p[0] = optab[1];
  p[1] = optab[0];
  p[2] = optab[1];
  p[3] = optab[2];
  execute (&p);
  if (__builtin_strcmp (buf, "xyxyz"))
    abort ();
  exit (0);
}
"#;

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-old-style-calls-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn source(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
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

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc(target: &str, flags: &[&str], source: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .arg(format!("--target={target}"))
        .arg("--error-format=short");
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.args(flags).arg(source).output().unwrap()
}

fn show(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn source(name: &str) -> (&'static str, usize, Option<&'static str>) {
    let (_, text, line, output) = SOURCES
        .iter()
        .find(|(label, ..)| *label == name)
        .copied()
        .unwrap();
    (text, line, output)
}

/// The source without its `@` marker, and the line and column of the marker.
fn marked(source: &str) -> (String, (usize, usize)) {
    let offset = source.find('@').expect("marker");
    let line = source[..offset].matches('\n').count() + 1;
    let column = offset - source[..offset].rfind('\n').map_or(0, |at| at + 1) + 1;
    (source.replacen('@', "", 1), (line, column))
}

/// The call line of a measured source, and the column of its callee `f`.
fn call_position(text: &str, line: usize) -> (usize, usize) {
    let call = text.lines().nth(line - 1).unwrap();
    (line, call.find("f(").unwrap() + 1)
}

/// Rule 2 and 3: GCC 14 gives no diagnostic for a call without a prototype
/// in gnu89 to gnu17, and EvoCC compiles each such call on both targets.
#[test]
fn calls_without_a_prototype_have_no_diagnostic() {
    let work = Work::new("no-diagnostic");
    let mut failures = Vec::new();
    let mut sources: Vec<(&str, &str)> = SOURCES
        .iter()
        .filter(|(name, ..)| name.ends_with("-noproto"))
        .map(|(name, text, ..)| (*name, *text))
        .collect();
    sources.push(("920302-1", ORIGINAL_920302_1));
    for target in TARGETS {
        for mode in MODES {
            for (name, text) in &sources {
                let path = work.source("unit.c", text);
                for action in ["-fsyntax-only", "--emit=asm"] {
                    let output = evocc(target, &[mode, action], &path);
                    if output.status.code() != Some(0) || !output.stderr.is_empty() {
                        failures.push(format!(
                            "{target} {mode} {action} {name}\n{}",
                            show(&output)
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// EvoCC refuses the gnu23 mode. GCC 14 accepts the old-style definitions
/// there with `-Wold-style-definition`, and Apple clang refuses them.
#[test]
fn gnu23_keeps_its_mode_refusal() {
    let work = Work::new("gnu23");
    for (name, text, ..) in SOURCES {
        let path = work.source("unit.c", text);
        for target in TARGETS {
            let output = evocc(target, &["-std=gnu23", "-fsyntax-only"], &path);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{target} {name}\n{}",
                show(&output)
            );
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("unsupported C language mode `-std=gnu23`"),
                "{target} {name}\n{}",
                show(&output)
            );
        }
    }
}

/// Rule 4: with a prototype in scope nothing changes. The incompatible
/// pointer argument is W0012 (GCC 14: a warning in gnu89, an error later),
/// and too many or too few arguments keep E3051 at code generation; GCC 14
/// refuses them in the front end.
#[test]
fn calls_with_a_prototype_do_not_change() {
    let work = Work::new("prototype");
    let mut failures = Vec::new();
    for target in TARGETS {
        for mode in MODES {
            for (name, text, line, _) in SOURCES {
                if name.ends_with("-noproto") {
                    continue;
                }
                let path = work.source("unit.c", text);
                let (call_line, column) = call_position(text, line);
                let syntax = evocc(target, &[mode, "-fsyntax-only"], &path);
                let assembly = evocc(target, &[mode, "--emit=asm"], &path);
                let stderr = String::from_utf8_lossy(&syntax.stderr).into_owned();
                let asm_stderr = String::from_utf8_lossy(&assembly.stderr).into_owned();
                let ok = match name {
                    "ptr-proto" => {
                        let level = if mode == "-std=gnu89" {
                            "warning"
                        } else {
                            "error"
                        };
                        let expected = format!(
                            "{}:{call_line}:{}: {level}[W0012]: incompatible pointer types\n",
                            path.display(),
                            column + 2
                        );
                        stderr == expected
                            && syntax.status.code() == Some(i32::from(level == "error"))
                    }
                    "many-proto" | "few-proto" => {
                        syntax.status.code() == Some(0)
                            && stderr.is_empty()
                            && assembly.status.code() == Some(1)
                            && asm_stderr.lines().next().is_some_and(|first| {
                                first.starts_with(&format!(
                                    "{}:{call_line}:{column}: error[E3051]",
                                    path.display()
                                ))
                            })
                    }
                    _ => {
                        syntax.status.code() == Some(0)
                            && stderr.is_empty()
                            && assembly.status.code() == Some(0)
                            && asm_stderr.is_empty()
                    }
                };
                if !ok {
                    failures.push(format!(
                        "{target} {mode} {name}\n{}\n{}",
                        show(&syntax),
                        show(&assembly)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Rule 2: a call whose promoted argument types are the promoted parameter
/// types compiles to the code of the call with the promoted prototype.
#[test]
fn defined_calls_compile_as_with_the_promoted_prototype() {
    let work = Work::new("same-code");
    for (unprototyped, prototyped) in [
        ("match-noproto", "match-proto"),
        ("char-noproto", "char-proto-promoted"),
        ("short-noproto", "short-proto-promoted"),
        ("float-noproto", "float-proto-promoted"),
        ("sign-noproto", "sign-proto"),
        ("charptr-noproto", "charptr-proto"),
        ("voidptr-noproto", "voidptr-proto"),
    ] {
        for target in TARGETS {
            let (left, ..) = source(unprototyped);
            let (right, ..) = source(prototyped);
            let left = evocc(
                target,
                &["-std=gnu17", "-O2", "--emit=asm"],
                &work.source("a.c", left),
            );
            let right = evocc(
                target,
                &["-std=gnu17", "-O2", "--emit=asm"],
                &work.source("b.c", right),
            );
            assert!(
                left.status.success(),
                "{target} {unprototyped}\n{}",
                show(&left)
            );
            assert!(
                right.status.success(),
                "{target} {prototyped}\n{}",
                show(&right)
            );
            assert_eq!(left.stdout, right.stdout, "{target} {unprototyped}");
        }
    }
}

/// The measured sources whose call is defined, with their reference output.
fn defined_sources() -> Vec<(&'static str, &'static str, &'static str)> {
    SOURCES
        .iter()
        .filter_map(|(name, text, _, output)| output.map(|output| (*name, *text, output)))
        .collect()
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn defined_calls_print_the_reference_output_on_darwin() {
    let work = Work::new("darwin");
    for (name, text, expected) in defined_sources() {
        let path = work.source("unit.c", text);
        for level in ["-O0", "-O2"] {
            let program = work.0.join("unit");
            let output = evocc(
                DARWIN,
                &["-std=gnu17", level, "-o", program.to_str().unwrap()],
                &path,
            );
            assert!(output.status.success(), "{name} {level}\n{}", show(&output));
            let run = Command::new(&program).output().unwrap();
            assert_eq!(run.status.code(), Some(0), "{name} {level}\n{}", show(&run));
            assert_eq!(
                String::from_utf8_lossy(&run.stdout),
                expected,
                "{name} {level}"
            );
        }
    }
}

fn sealed(work: &Work, script: &str) -> Output {
    Command::new("container")
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
        .args(["evo-nexlab:sealed", "sh", "-c", script])
        .output()
        .unwrap()
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn defined_calls_print_the_reference_output_in_the_sealed_linux_image() {
    let work = Work::new("linux");
    let mut script = String::from("set -eu\ncd /work\n");
    let defined = defined_sources();
    for (index, (name, text, _)) in defined.iter().enumerate() {
        let path = work.source(&format!("case{index}.c"), text);
        let output = evocc(LINUX, &["-std=gnu17", "-O2", "--emit=asm"], &path);
        assert!(output.status.success(), "{name}\n{}", show(&output));
        std::fs::write(work.0.join(format!("case{index}.s")), &output.stdout).unwrap();
        script.push_str(&format!(
            "gcc-14 case{index}.s -o case{index}\n./case{index} > case{index}.out\n"
        ));
    }
    let run = sealed(&work, &script);
    assert_eq!(run.status.code(), Some(0), "{}", show(&run));
    for (index, (name, _, expected)) in defined.iter().enumerate() {
        let output = std::fs::read_to_string(work.0.join(format!("case{index}.out"))).unwrap();
        assert_eq!(output, *expected, "{name}");
    }
}

/// Rule 5: an undefined call: each marked call reports E3068 in both
/// interpreters and in `--check`, at the call.
const UNDEFINED: [&str; 9] = [
    "int f(u) unsigned u; { return u != 0; }\nint main(void) { return @f(-1); }\n",
    "int f(i) int i; { return i; }\nint main(void) { return @f(3000000000u) != 0; }\n",
    "int f(a) int a; { return a; }\nint main(void) { return @f(1L) - 1; }\n",
    "int f(a) int a; { return a; }\nint main(void) { return @f(1.5) - 1; }\n",
    "int f();\nshort p[5] = {1, 2, 3, 4, 5};\nint main(void) { return @f(&p) - 2; }\nint f(ip) short *ip; { return ip[1]; }\n",
    "int f(n) int n; { return n ? @f(n - 1L) : 0; }\nint main(void) { return f(3); }\n",
    "int f() { return 1; }\nint main(void) { return @f(1) - 1; }\n",
    "int h(q) const int *q; { return *q; }\nint v = 7;\nint main(void) { return @h(&v) - 7; }\n",
    "int f(ip) short *ip; { return ip[1]; }\nextern __typeof__(f) f;\nshort p[5] = {1, 2, 3, 4, 5};\nint main(void) { return @f(&p) - 2; }\n",
];

#[test]
fn undefined_calls_report_e3068_at_the_call() {
    let work = Work::new("undefined");
    let mut cases: Vec<(String, (usize, usize))> =
        UNDEFINED.iter().map(|text| marked(text)).collect();
    for name in ["ptr-noproto", "many-noproto", "few-noproto"] {
        let (text, line, _) = source(name);
        cases.push((text.to_owned(), call_position(text, line)));
    }
    let mut failures = Vec::new();
    for target in TARGETS {
        for (text, (line, column)) in &cases {
            let path = work.source("unit.c", text);
            for mode in INTERPRETERS {
                let output = evocc(target, &["-std=gnu17", mode], &path);
                let expected = format!("{}:{line}:{column}: {MISMATCH}", path.display());
                let first = String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_owned();
                if output.status.code() != Some(1) || first != expected {
                    failures.push(format!("{target} {mode}\n{text}{}", show(&output)));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Rule 5 and its two exceptions: these calls are defined, so the
/// interpreters and `--check` report nothing and `main` returns 0.
#[test]
fn exception_pairs_are_defined() {
    let work = Work::new("defined");
    for text in [
        "int f(u) unsigned u; { return u != 42; }\nint main(void) { return f(42); }\n",
        "int f(i) int i; { return i != 42; }\nint main(void) { return f(42u); }\n",
        "int f(v) void *v; { return v == 0; }\nchar s[] = \"hi\";\nint main(void) { return f(s); }\n",
        "int f(s) char *s; { return s[0] != 'h'; }\nint main(void) { void *v = \"hi\"; return f(v); }\n",
        "int f(s) const char *s; { return s[0] != 'h'; }\nunsigned char u[] = \"hi\";\nint main(void) { return f(u); }\n",
        "int f(c) char c; { return c != 65; }\nint main(void) { signed char x = 65; return f(x); }\n",
        "int f(ip) short *ip; { return ip[1] != 2; }\nshort p[5] = {1, 2, 3, 4, 5};\nint main(void) { return f(&p[0]); }\n",
        "int f() { return 0; }\nint main(void) { return f(); }\n",
        "int f(d) double d; { return d != 2.5; }\nint main(void) { return f(2.5f); }\n",
        "int f(n) int n; { return n ? f(n - 1) : 0; }\nint main(void) { return f(3); }\n",
    ] {
        let path = work.source("unit.c", text);
        for target in TARGETS {
            for mode in INTERPRETERS {
                let output = evocc(target, &["-std=gnu17", mode], &path);
                assert_eq!(output.status.code(), Some(0), "{target} {mode}\n{text}{}", show(&output));
                assert!(output.stderr.is_empty(), "{target} {mode}\n{text}{}", show(&output));
            }
        }
    }
}

/// The EvoIR names the obligation, and the machine code has no check for it:
/// the call runs as GCC 14 runs it (EvoMIR 5.2).
#[test]
fn the_obligation_is_in_evoir_and_not_in_machine_code() {
    let work = Work::new("obligation");
    for name in ["ptr-noproto", "many-noproto", "few-noproto"] {
        let (text, ..) = source(name);
        let path = work.source("unit.c", text);
        for target in TARGETS {
            let ir = evocc(target, &["-std=gnu17", "--emit=evoir"], &path);
            assert!(ir.status.success(), "{target} {name}\n{}", show(&ir));
            let ir = String::from_utf8_lossy(&ir.stdout);
            assert!(ir.contains("ub.assume.old_style_call"), "{target} {name}");
            // The call goes through the function's address with the
            // signature of the promoted arguments (EvoIR 10.1.7).
            assert!(ir.contains("ptr.from_int.fnptr<"), "{target} {name}\n{ir}");
            for level in ["-O0", "-O2"] {
                let assembly = evocc(target, &["-std=gnu17", level, "--emit=asm"], &path);
                assert!(
                    assembly.status.success(),
                    "{target} {name}\n{}",
                    show(&assembly)
                );
                assert!(
                    !String::from_utf8_lossy(&assembly.stdout).contains("#3068"),
                    "{target} {name} {level}"
                );
            }
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn undefined_calls_compile_and_exit_zero_on_darwin() {
    let work = Work::new("undefined-darwin");
    for name in ["ptr-noproto", "many-noproto", "few-noproto"] {
        let (text, ..) = source(name);
        let path = work.source("unit.c", text);
        let program = work.0.join("unit");
        let output = evocc(
            DARWIN,
            &["-std=gnu17", "-O2", "-o", program.to_str().unwrap()],
            &path,
        );
        assert!(output.status.success(), "{name}\n{}", show(&output));
        let run = Command::new(&program).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{name}\n{}", show(&run));
    }
}

/// Rule 5: `--check` reports the undefined call of `920302-1` at line 41.
#[test]
fn the_920302_1_original_reports_e3068_at_line_41() {
    let work = Work::new("original-check");
    let path = work.source("920302-1.c", ORIGINAL_920302_1);
    for target in TARGETS {
        for mode in INTERPRETERS {
            let output = evocc(target, &[mode], &path);
            let expected = format!("{}:41:3: {MISMATCH}", path.display());
            assert_eq!(
                output.status.code(),
                Some(1),
                "{target} {mode}\n{}",
                show(&output)
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stderr).lines().next(),
                Some(expected.as_str()),
                "{target} {mode}"
            );
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn the_920302_1_original_runs_natively_on_darwin() {
    let work = Work::new("original-darwin");
    let path = work.source("920302-1.c", ORIGINAL_920302_1);
    for level in ["-O0", "-O2"] {
        let program = work.0.join("920302-1");
        let output = evocc(DARWIN, &[level, "-o", program.to_str().unwrap()], &path);
        assert!(output.status.success(), "{level}\n{}", show(&output));
        assert!(output.stderr.is_empty(), "{level}\n{}", show(&output));
        let run = Command::new(&program).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{level}\n{}", show(&run));
        assert!(
            run.stdout.is_empty() && run.stderr.is_empty(),
            "{level}\n{}",
            show(&run)
        );
    }
}

/// The torture lane's commands: EvoCC writes the assembly, GCC 14 assembles
/// and links it, and the program exits 0 with no output.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn the_920302_1_original_runs_in_the_sealed_linux_image() {
    let work = Work::new("original-linux");
    let path = work.source("920302-1.c", ORIGINAL_920302_1);
    for level in ["-O0", "-O2"] {
        let output = evocc(LINUX, &[level, "--emit=asm"], &path);
        assert!(output.status.success(), "{level}\n{}", show(&output));
        assert!(output.stderr.is_empty(), "{level}\n{}", show(&output));
        std::fs::write(work.0.join("subject.s"), &output.stdout).unwrap();
        let run = sealed(
            &work,
            "cd /work && gcc-14 subject.s -o subject && ./subject > out.txt 2>&1; status=$?; cat out.txt; exit $status",
        );
        assert_eq!(run.status.code(), Some(0), "{level}\n{}", show(&run));
        assert!(run.stdout.is_empty(), "{level}\n{}", show(&run));
    }
}

/// A prototype in scope wins: a file-scope prototype after the definition,
/// and a block-scope prototype in its block only (the plan's `m1.c` and
/// `m2.c`; GCC 14 reports the same two lines).
#[test]
fn prototypes_in_scope_win() {
    let work = Work::new("scope");
    for marked_source in [
        "int f(ip) short *ip; { return ip[1]; }\nint f(short *ip);\nshort p[5] = {1, 2, 3, 4, 5};\nint main(void) { return f(@&p) - 2; }\n",
        "int f(ip) short *ip; { return ip[1]; }\nshort p[5] = {1, 2, 3, 4, 5};\nint g(void) { int f(short *); return f(@&p); }\nint main(void) { return f(&p) - 2; }\n",
    ] {
        let (text, (line, column)) = marked(marked_source);
        let path = work.source("unit.c", &text);
        for target in TARGETS {
            let output = evocc(target, &["-std=gnu17", "-fsyntax-only"], &path);
            assert_eq!(output.status.code(), Some(1), "{target}\n{text}{}", show(&output));
            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                format!(
                    "{}:{line}:{column}: error[W0012]: incompatible pointer types\n",
                    path.display()
                ),
                "{target}\n{text}"
            );
        }
    }
}

/// The substitution of 10.1.9 keeps a call that does not agree, so its
/// obligation stays; an agreeing call is substituted as before.
#[test]
fn substitution_keeps_the_obligation() {
    let work = Work::new("substitution");
    for target in TARGETS {
        let (text, (line, column)) =
            marked("static int f(a) int a; { return a; }\nint main(void) { return @f(1L) - 1; }\n");
        let path = work.source("unit.c", &text);
        let output = evocc(target, &["-std=gnu17", "-O2", "--check"], &path);
        assert_eq!(output.status.code(), Some(1), "{target}\n{}", show(&output));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).lines().next(),
            Some(format!("{}:{line}:{column}: {MISMATCH}", path.display()).as_str()),
            "{target}"
        );
        let path = work.source(
            "agree.c",
            "static int f(a) int a; { return a; }\nint main(void) { return f(1) - 1; }\n",
        );
        let output = evocc(target, &["-std=gnu17", "-O2", "--check"], &path);
        assert_eq!(output.status.code(), Some(0), "{target}\n{}", show(&output));
    }
}
