//! N09e: `optimize` arguments that EvoCC already satisfies are accepted with
//! no effect on code, and every other argument keeps E2952 (EvoIR 10.1.9).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
static NEXT: AtomicU32 = AtomicU32::new(0);

/// The `-f` options of the rule, without `-f`. The strict driver accepts
/// each of them on the command line with no change of code.
const F_OPTIONS: [&str; 12] = [
    "no-strict-aliasing",
    "no-optimize-sibling-calls",
    "no-align-functions",
    "no-align-jumps",
    "no-align-labels",
    "no-align-loops",
    "no-prefetch-loop-arrays",
    "no-tree-ch",
    "no-tree-loop-distribute-patterns",
    "no-tree-vectorize",
    "no-tree-loop-vectorize",
    "no-tree-slp-vectorize",
];
const LEVELS: [&str; 7] = ["O", "O0", "O1", "O2", "O3", "Os", "Og"];

/// Each accepted attribute. GCC 14 accepts each one without a diagnostic.
fn accepted() -> Vec<String> {
    let mut arguments: Vec<String> = ["0", "1", "2", "3", "1 + 2", "sizeof (char)"]
        .map(String::from)
        .to_vec();
    for level in LEVELS {
        arguments.extend([format!("\"{level}\""), format!("\"-{level}\"")]);
    }
    for option in F_OPTIONS {
        arguments.extend([format!("\"{option}\""), format!("\"-f{option}\"")]);
    }
    // The first two left the refusal list of unsupported_abi_attributes.rs.
    arguments.extend(
        [
            "3",
            "\"no-tree-vectorize\", \"no-strict-aliasing\"",
            "3, \"O2,-fno-tree-ch\"",
            "\"-O2,no-strict-aliasing,-fno-align-loops\"",
            "\"no-tree-\" \"vectorize\"",
        ]
        .map(String::from),
    );
    let mut attributes: Vec<String> = arguments
        .iter()
        .map(|arguments| format!("__attribute__((optimize({arguments})))"))
        .collect();
    attributes.extend(
        [
            "__attribute__((__optimize__(\"O2\")))",
            "[[gnu::optimize(\"no-strict-aliasing\")]]",
            "[[__gnu__::optimize(1)]]",
        ]
        .map(String::from),
    );
    attributes
}

/// Each refused argument list: a semantic or unknown option, an empty
/// option, a value outside 0 to 3, a form outside the rule, a mixture.
const REFUSED: &[&str] = &[
    "\"Ofast\"",
    "\"-Ofast\"",
    "\"Oz\"",
    "\"fast-math\"",
    "\"-ffast-math\"",
    "\"wrapv\"",
    "\"-fwrapv\"",
    "\"strict-aliasing\"",
    "\"unroll-loops\"",
    "\"tree-vectorize\"",
    "\"omit-frame-pointer\"",
    "\"stack-protector\"",
    "\"\"",
    "\"bogus-option\"",
    "\"-fbogus-option\"",
    "\"no-strict-aliasing,wrapv\"",
    "\"no-tree-vectorize\", \"wrapv\"",
    "3, \"unroll-loops\"",
    "\"no-tree-vectorize,\"",
    "4",
    "-1",
    "\"2\"",
    "\"fno-strict-aliasing\"",
    "\"-no-strict-aliasing\"",
    "\"O4\"",
    "",
];

const BODY: &str = "int f(void) { return 7; }\nint main(void) { return f() - 7; }\n";

/// The attribute on the definition, and on an earlier declaration.
fn placements(attribute: &str) -> [(String, String); 2] {
    [
        (format!("{attribute} {BODY}"), BODY.to_owned()),
        (
            format!("{attribute} int f(void);\n{BODY}"),
            format!("int f(void);\n{BODY}"),
        ),
    ]
}

struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-optimize-levels-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
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
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc(target: &str, arguments: &[&str], source: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.arg(format!("--target={target}"));
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.args(arguments).arg(source).output().unwrap()
}

fn detail(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The output of a run that succeeded with no diagnostic.
fn clean(output: Output, context: &str) -> Vec<u8> {
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "{context}: {}",
        detail(&output)
    );
    output.stdout
}

#[test]
fn each_satisfied_option_is_accepted_without_an_effect() {
    let work = Work::new();
    for target in [LINUX, DARWIN] {
        for attribute in accepted() {
            for (index, (text, plain)) in placements(&attribute).iter().enumerate() {
                let source = work.source("input.c", text);
                let baseline = work.source("plain.c", plain);
                let context = format!("{target} {attribute} placement {index}");
                let expected = clean(evocc(target, &["--emit=asm"], &baseline), &context);
                let assembly = clean(evocc(target, &["--emit=asm"], &source), &context);
                assert!(assembly == expected, "{context}: the assembly changed");
                clean(evocc(target, &["-fsyntax-only"], &source), &context);
                if index == 1 {
                    continue;
                }
                for mode in ["--interpret-c-hir", "--interpret-evoir"] {
                    let output = clean(evocc(target, &[mode], &source), &context);
                    assert_eq!(output, b"0\n", "{context} {mode}");
                }
                let output = clean(evocc(target, &["--check"], &source), &context);
                assert!(String::from_utf8_lossy(&output).contains("main returned 0"));
            }
        }
    }
}

#[test]
fn every_other_option_keeps_its_refusal() {
    let work = Work::new();
    for target in [LINUX, DARWIN] {
        for arguments in REFUSED {
            let attribute = format!("__attribute__((optimize({arguments})))");
            for (index, (text, _)) in placements(&attribute).iter().enumerate() {
                let source = work.source("input.c", text);
                for mode in ["--emit=asm", "-fsyntax-only"] {
                    let output = evocc(target, &[mode], &source);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    assert!(
                        output.status.code() == Some(1)
                            && stderr.contains("E2952")
                            && stderr.contains("optimize")
                            && output.stdout.is_empty(),
                        "{target} {arguments:?} placement {index} {mode}: {}",
                        detail(&output)
                    );
                }
            }
        }
    }
}

/// Each `-f` option that `optimize` accepts is also accepted by the strict
/// driver with no change of code, so the two lists cannot drift apart.
#[test]
fn the_drivers_accept_each_satisfied_f_option() {
    let work = Work::new();
    let source = work.source("input.c", BODY);
    for target in [LINUX, DARWIN] {
        let expected = clean(evocc(target, &["--emit=asm"], &source), target);
        for option in F_OPTIONS {
            let option = format!("-f{option}");
            let context = format!("{target} {option}");
            let assembly = clean(evocc(target, &["--emit=asm", &option], &source), &context);
            assert!(assembly == expected, "{context}: the assembly changed");
            clean(
                evocc(target, &["-fsyntax-only", "-Werror", &option], &source),
                &context,
            );
        }
    }
}

const ORIGINALS: [(&str, &str); 4] = [
    ("pr60062", PR60062),
    ("pr60072", PR60072),
    ("alias-1", ALIAS_1),
    ("pr79043", PR79043),
];

/// Apple clang ignores `optimize`; at -O2, its strict aliasing makes
/// alias-1 and pr79043 abort. EvoCC derives no disjointness from C types.
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn the_torture_originals_run_natively_on_darwin() {
    let work = Work::new();
    for (name, text) in ORIGINALS {
        let source = work.source(&format!("{name}.c"), text);
        for level in ["-O0", "-O2"] {
            let object = work.0.join(format!("{name}.o"));
            let program = work.0.join(name);
            let compiled = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args([&format!("--target={DARWIN}"), level, "-c"])
                .arg(&source)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                compiled.status.success(),
                "{name} {level}: {}",
                detail(&compiled)
            );
            let linked = Command::new("xcrun")
                .args(["clang"])
                .arg(&object)
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(
                linked.status.success(),
                "{name} {level}: {}",
                detail(&linked)
            );
            let run = Command::new(&program).output().unwrap();
            assert_eq!(
                run.status.code(),
                Some(0),
                "{name} {level}: {}",
                detail(&run)
            );
        }
    }
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn the_torture_originals_run_in_the_sealed_linux_image() {
    let work = Work::new();
    let mut script = String::from("set -eu\ncd /work\n");
    for (name, text) in ORIGINALS {
        let source = work.source(&format!("{name}.c"), text);
        for level in ["-O0", "-O2"] {
            let assembly = work.0.join(format!("{name}{level}.s"));
            let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .arg(format!("--target={LINUX}"))
                .arg(format!(
                    "--sysroot={}",
                    root().join("sysroot").join(LINUX).display()
                ))
                .args([level, "-S"])
                .arg(&source)
                .arg("-o")
                .arg(&assembly)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{name} {level}: {}",
                detail(&output)
            );
            script.push_str(&format!(
                "gcc-14 {name}{level}.s -o subject\n./subject\necho '{name} {level}: PASS'\n"
            ));
        }
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
    assert!(output.status.success(), "{}", detail(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.matches(": PASS").count(), 8, "{stdout}");
}

// GCC torture suite at commit 78d4ac73dd39 (the ladder pin),
// gcc/testsuite/gcc.c-torture/execute/pr60062.c, byte for byte.
// SHA-256 76562729f3ce8b46ec7c4028fa6ce382b197f683272c71a1bb68cd3028e86409.
const PR60062: &str = r#"/* PR target/60062 */

int a;

static void
foo (const char *p1, int p2)
{
  if (__builtin_strcmp (p1, "hello") != 0)
    __builtin_abort ();
}

static void
bar (const char *p1)
{
  if (__builtin_strcmp (p1, "hello") != 0)
    __builtin_abort ();
}

__attribute__((optimize (0))) int
main ()
{
  foo ("hello", a);
  bar ("hello");
  return 0;
}
"#;

// GCC torture suite at commit 78d4ac73dd39 (the ladder pin),
// gcc/testsuite/gcc.c-torture/execute/pr60072.c, byte for byte.
// SHA-256 277cbc5423863ba4dc1252cc9ea8a333f839d192c75f5062b42f037d528a97cd.
const PR60072: &str = r#"/* PR target/60072 */

int c = 1;

__attribute__ ((optimize (1)))
static int *foo (int *p)
{
  return p;
}

int
main ()
{
  *foo (&c) = 2;
  return c - 2;
}
"#;

// GCC torture suite at commit 78d4ac73dd39 (the ladder pin),
// gcc/testsuite/gcc.c-torture/execute/alias-1.c, byte for byte.
// SHA-256 fd08dfdb4638caa58a4d9f1cf776db731899779e5b64d6b666d4ae9c104be57d.
const ALIAS_1: &str = r#"int val;

int *ptr = &val;
float *ptr2 = (float *) &val;

__attribute__((optimize ("-fno-strict-aliasing")))
void
typepun (void)
{
  *ptr2=0;
}

int
main(void)
{
  *ptr=1;
  typepun ();
  if (*ptr)
    __builtin_abort ();
}

"#;

// GCC torture suite at commit 78d4ac73dd39 (the ladder pin),
// gcc/testsuite/gcc.c-torture/execute/pr79043.c, byte for byte.
// SHA-256 b37ea12ac505bec71f75c369e0f54242b9c09d2fb5b3d2ee2a1ea960a9720b83.
const PR79043: &str = r#"/* PR ipa/78791 */

int val;

int *ptr = &val;
float *ptr2 = (float *) &val;

static void
__attribute__((always_inline, optimize ("-fno-strict-aliasing")))
typepun ()
{
  *ptr2=0;
}

int
main(void)
{
  *ptr=1;
  typepun ();
  if (*ptr)
    __builtin_abort ();
}
"#;
