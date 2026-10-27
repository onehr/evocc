//! N09f: the x86 calling conventions are ignored with the warning W0011
//! (EvoIR 10.1.9). On AArch64, GCC 14 and Apple clang warn about `stdcall`,
//! `fastcall`, `thiscall`, `vectorcall`, `regcall` and `sseregparm`, and
//! GCC 14 about `regparm`, and ignore them. EvoCC warns at the attribute
//! name. The attribute has no other effect: the code equals the code without
//! it, and each call is a base AAPCS64 call. Where a reference refuses a
//! form, E2952 stays. Apple clang refuses `regparm`, so Darwin keeps E2952
//! for it. GCC 14 and Apple clang 21 gave the expected results on
//! 2026-10-27.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
static NEXT: AtomicU32 = AtomicU32::new(0);

const WARNING: &str = "warning[W0011]: x86 calling convention ignored";
const ERROR: &str = "error[W0011]: x86 calling convention ignored";

/// The six conventions that both references ignore with a warning.
const NAMES: [&str; 6] = [
    "stdcall",
    "fastcall",
    "thiscall",
    "vectorcall",
    "regcall",
    "sseregparm",
];

/// The conventions that EvoCC ignores on a target: `regparm` only on Linux.
fn ignored(target: &str) -> Vec<&'static str> {
    let mut names = NAMES.to_vec();
    if target == LINUX {
        names.push("regparm(2)");
    }
    names
}

/// The conventions that Apple clang implements. It refuses one on a
/// statement or with an argument. It knows `vectorcall` only in the
/// `__attribute__` syntax, and it does not know `sseregparm`.
fn known_to_apple_clang(name: &str, bracketed: bool) -> bool {
    matches!(name, "stdcall" | "fastcall" | "thiscall" | "regcall")
        || (name == "vectorcall" && !bracketed)
}

/// Each measured placement where both references warn and ignore the
/// attribute. `{A}` is the GNU specifier, `{N}` the attribute with its
/// arguments and `{U}` its spelling with `__`. The first seven are the
/// placements of the goal.
const WARNED: [(&str, &str); 44] = [
    ("declaration", "int {A} f(int, int);\n"),
    ("definition", "int {A} f(int a,int b) { return a+b; }\n"),
    ("typedef", "typedef int {A} Fn(int,int); Fn f;\n"),
    (
        "pointer",
        "int f(int a,int b) {return a+b;} int ({A} *fp)(int,int)=f;\n",
    ),
    ("parameter", "int f(int {A} a) {return a;}\n"),
    ("member", "struct S {int {A} a;};\n"),
    ("object", "int {A} value;\n"),
    ("after-declarator", "int f(int, int) {A};\n"),
    (
        "underscore-spelling",
        "int __attribute__(({U})) f(int, int);\n",
    ),
    ("bracket-gnu", "[[gnu::{N}]] int f(int, int);\n"),
    ("bracket-__gnu__", "[[__gnu__::{N}]] int f(int, int);\n"),
    ("second-declarator", "int g(void), {A} f(void);\n"),
    (
        "prototype-then-definition",
        "int f(void) {A};\nint f(void) { return 0; }\n",
    ),
    (
        "definition-then-prototype",
        "int f(void) { return 0; }\nint f(void) {A};\n",
    ),
    ("call", "int f(void) {A};\nint main(void) { return f(); }\n"),
    (
        "address",
        "int f(void) {A};\nint main(void) { int (*p)(void) = f; return p == 0; }\n",
    ),
    ("typedef-after", "typedef int F(void) {A};\n"),
    ("typedef-specifiers", "typedef {A} int F(void);\n"),
    ("pointer-object-after", "int (*p)(void) {A};\n"),
    ("pointer-star", "int (* {A} p)(void);\n"),
    ("object-and-function", "{A} int f(void), x;\n"),
    ("parameter-function", "int g(int h(void) {A});\n"),
    (
        "member-function-pointer",
        "struct s { int (*p)(void) {A}; };\n",
    ),
    (
        "sizeof-type-name",
        "int g(void) { return sizeof({A} int); }\n",
    ),
    ("label", "void g(void) { l: {A}; }\n"),
    ("tag-closing-brace", "struct t { int a; } {A};\n"),
    ("enumerator", "enum e { E1 {A} };\n"),
    ("result-pointer-star", "int * {A} f(void);\n"),
    (
        "function-pointer-typedef",
        "typedef int ({A} *F)(int, int);\n",
    ),
    ("old-style-parameter", "int f(a) int {A} a; { return a; }\n"),
    (
        "block-scope-prototype",
        "int main(void) { int {A} f(int, int); return 0; }\n",
    ),
    ("abstract-parameter", "int g(int ({A} *)(void));\n"),
    (
        "sizeof-after-specifier",
        "int g(void) { return sizeof(int {A}); }\n",
    ),
    (
        "cast-after-specifier",
        "int g(void) { return (int {A})0; }\n",
    ),
    (
        "sizeof-after-star",
        "int g(void) { return sizeof(int * {A}); }\n",
    ),
    (
        "cast-after-star",
        "int g(void) { return (int * {A})0 != 0; }\n",
    ),
    ("cast-leading", "int g(void) { return ({A} int)0; }\n"),
    (
        "alignof-after-specifier",
        "int g(void) { return _Alignof(int {A}); }\n",
    ),
    ("parameter-abstract-suffix", "int g(int (*)(void) {A});\n"),
    ("parameter-array-suffix", "int g(int [2] {A});\n"),
    ("parameter-specifier", "int g(int {A});\n"),
    ("parameter-star", "int g(int * {A});\n"),
    ("parameter-named-suffix", "int g(int (*p)(void) {A});\n"),
    (
        "local-object-suffix",
        "void h(void) { int (*q)(void) {A} = 0; (void)q; }\n",
    ),
];

/// GCC 14 and Apple clang refuse an attribute after the abstract declarator
/// of a type name, for every attribute. Each name keeps E2952 there.
const TYPE_NAME_SUFFIXES: [(&str, &str); 5] = [
    (
        "cast-after-abstract-function",
        "int g(void) { return (int (*)(void) {A})0 != 0; }\n",
    ),
    (
        "sizeof-after-abstract-function",
        "int g(void) { return sizeof(int (*)(void) {A}); }\n",
    ),
    (
        "sizeof-after-abstract-array",
        "int g(void) { return sizeof(int [2] {A}); }\n",
    ),
    (
        "compound-literal-after-abstract",
        "int g(void) { return (int (*)(void) {A}){0} != 0; }\n",
    ),
    (
        "typeof-after-abstract",
        "__typeof__(int (*)(void) {A}) p;\n",
    ),
];

/// Null statements, and whether the spelling is the bracket syntax. Apple
/// clang refuses a convention that it knows there; GCC 14 warns.
const STATEMENTS: [(&str, &str, bool); 4] = [
    ("null-statement", "void g(void) { {A}; }\n", false),
    (
        "underscore-null-statement",
        "void g(void) { __attribute__(({U})); }\n",
        false,
    ),
    (
        "bracket-null-statement",
        "void g(void) { [[gnu::{N}]]; }\n",
        true,
    ),
    (
        "bracket-__gnu__-null-statement",
        "void g(void) { [[__gnu__::{N}]]; }\n",
        true,
    ),
];

/// Argument lists. Apple clang refuses an argument of a convention that it
/// knows and ignores the others; GCC 14 ignores every list with the warning.
const ARGUMENTS: [&str; 11] = [
    "stdcall()",
    "stdcall(1)",
    "stdcall(x)",
    "fastcall()",
    "fastcall(1)",
    "thiscall(1)",
    "vectorcall(1)",
    "regcall(1)",
    "sseregparm()",
    "sseregparm(1)",
    "sseregparm(x, 2)",
];

/// The `regparm` lists that GCC 14 ignores with the warning.
const REGPARM_ARGUMENTS: [&str; 9] = [
    "regparm",
    "regparm()",
    "regparm(0)",
    "regparm(3)",
    "regparm(4)",
    "regparm(-1)",
    "regparm(x)",
    "regparm(1, 2)",
    "regparm(\"a\")",
];

const ARGUMENT_PLACEMENTS: [(&str, &str, bool); 4] = [
    ("declaration", "int {A} f(int, int);\n", false),
    (
        "definition",
        "int {A} f(int a, int b) { return a + b; }\n",
        false,
    ),
    ("object", "int {A} value;\n", false),
    (
        "bracket-declaration",
        "[[gnu::{N}]] int f(int, int);\n",
        true,
    ),
];

/// The warning levels of GCC 14 and Apple clang. EvoCC adds `--cap-lints`.
#[derive(Clone, Copy, Debug)]
enum Level {
    Warning,
    Error,
    Silent,
}

const LEVELS: [(&[&str], Level); 22] = [
    (&[], Level::Warning),
    (&["-Wno-attributes"], Level::Silent),
    (&["-Werror"], Level::Error),
    (&["-Werror=attributes"], Level::Error),
    (&["-Werror", "-Wno-error=attributes"], Level::Warning),
    (&["-w"], Level::Silent),
    (&["-Wattributes"], Level::Warning),
    (&["-Wno-error=attributes"], Level::Warning),
    (&["-Wall"], Level::Warning),
    (&["-Wextra"], Level::Warning),
    (&["-Wpedantic"], Level::Warning),
    (&["-Werror=attributes", "-Wno-attributes"], Level::Silent),
    (&["-Wno-attributes", "-Werror=attributes"], Level::Error),
    (&["-Wno-attributes", "-Wattributes"], Level::Warning),
    (&["-w", "-Werror=attributes"], Level::Silent),
    (&["-Werror", "-w"], Level::Silent),
    (&["-Werror", "-Wno-attributes"], Level::Silent),
    (&["-Werror", "--cap-lints=warn"], Level::Warning),
    (&["--cap-lints=allow"], Level::Silent),
    // Measured differences. Apple clang's -Wno-ignored-attributes and
    // -Wno-unknown-attributes, and GCC 14's -Wno-attributes=gnu::stdcall,
    // silence their own warning. EvoCC accepts them with no effect.
    (&["-Wno-ignored-attributes"], Level::Warning),
    (&["-Wno-unknown-attributes"], Level::Warning),
    (&["-Wno-attributes=gnu::stdcall"], Level::Warning),
];

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-x86-conventions-{label}-{}-{}",
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

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The source of a template with an attribute.
fn spell(template: &str, attribute: &str) -> String {
    let (name, arguments) = attribute.split_at(attribute.find('(').unwrap_or(attribute.len()));
    template
        .replace("{A}", &format!("__attribute__(({attribute}))"))
        .replace("{N}", attribute)
        .replace("{U}", &format!("__{name}__{arguments}"))
}

/// The source of a template without the attribute.
fn plain(template: &str) -> String {
    template
        .replace("{A}", "")
        .replace("[[gnu::{N}]] ", "")
        .replace("[[__gnu__::{N}]] ", "")
        .replace("__attribute__(({U})) ", "")
}

/// One-based line and column of each attribute name: the first token after
/// `__attribute__((` or `[[`. EvoCC and Apple clang report there; GCC 14
/// reports at the start of the declaration.
fn name_positions(source: &str) -> Vec<(usize, usize)> {
    let mut offsets = Vec::new();
    for (opening, length) in [("__attribute__((", 15), ("[[", 2)] {
        offsets.extend(source.match_indices(opening).map(|(at, _)| at + length));
    }
    offsets.sort_unstable();
    offsets
        .into_iter()
        .map(|offset| {
            let before = &source[..offset];
            let line = before.matches('\n').count() + 1;
            let column = offset - before.rfind('\n').map_or(0, |newline| newline + 1) + 1;
            (line, column)
        })
        .collect()
}

fn name_position(source: &str) -> (usize, usize) {
    let positions = name_positions(source);
    assert_eq!(positions.len(), 1, "one attribute in {source}");
    positions[0]
}

fn at(path: &Path, position: (usize, usize), diagnostic: &str) -> String {
    format!(
        "{}:{}:{}: {diagnostic}",
        path.display(),
        position.0,
        position.1
    )
}

fn refusal(attribute: &str) -> String {
    let name = attribute.split('(').next().unwrap();
    format!("error[E2952]: unsupported-function-attribute: {name}")
}

/// The run succeeds, and its diagnostics are exactly `expected`.
fn warned(output: &Output, expected: &[String], context: &str) {
    assert_eq!(output.status.code(), Some(0), "{context}\n{}", text(output));
    let lines = expected
        .iter()
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        lines,
        "{context}\n{}",
        text(output)
    );
}

/// The run fails with `expected` as its first diagnostic and no output.
fn refused(output: &Output, expected: &str, context: &str) {
    assert_eq!(output.status.code(), Some(1), "{context}\n{}", text(output));
    assert!(output.stdout.is_empty(), "{context}\n{}", text(output));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.lines().next(), Some(expected), "{context}\n{stderr}");
}

/// Rules 3 and 4: each name warns once, at its name, in each placement where
/// the reference of the target warns. At -O0 and -O2 the assembly equals the
/// assembly of the same source without the attribute.
#[test]
fn each_name_warns_and_changes_no_code_in_each_placement() {
    let work = Work::new("placements");
    for target in TARGETS {
        for (label, template) in WARNED {
            let baseline = work.source("plain.c", &plain(template));
            let assembly = ["-O0", "-O2"].map(|level| {
                let output = evocc(target, &[level, "--emit=asm"], &baseline);
                warned(&output, &[], &format!("{target} {label} without"));
                output.stdout
            });
            for attribute in ignored(target) {
                let source = spell(template, attribute);
                let path = work.source("unit.c", &source);
                let context = format!("{target} {label} {attribute}\n{source}");
                let expected = [at(&path, name_position(&source), WARNING)];
                warned(
                    &evocc(target, &["-fsyntax-only"], &path),
                    &expected,
                    &context,
                );
                for (level, baseline) in ["-O0", "-O2"].into_iter().zip(&assembly) {
                    let output = evocc(target, &[level, "--emit=asm"], &path);
                    warned(&output, &expected, &format!("{context} {level}"));
                    assert_eq!(&output.stdout, baseline, "{context} {level}");
                }
            }
        }
    }
}

/// Rule 6: both references refuse an attribute after the abstract declarator
/// of a type name, so each name keeps E2952 there.
#[test]
fn a_type_name_suffix_keeps_its_refusal() {
    let work = Work::new("suffixes");
    for target in TARGETS {
        for (label, template) in TYPE_NAME_SUFFIXES {
            for attribute in ignored(target) {
                let source = spell(template, attribute);
                let path = work.source("unit.c", &source);
                let expected = at(&path, name_position(&source), &refusal(attribute));
                let output = evocc(target, &["-fsyntax-only"], &path);
                refused(&output, &expected, &format!("{target} {label} {attribute}"));
            }
        }
    }
}

/// Rule 6 on Darwin: Apple clang refuses a convention that it knows on a null
/// statement or with an argument, so E2952 stays there. It warns about the
/// other forms, and GCC 14 warns about all of them.
#[test]
fn statements_and_arguments_follow_the_reference_of_the_target() {
    let work = Work::new("forms");
    let check = |target: &str, template: &str, attribute: &str, refuse: bool, label: &str| {
        let source = spell(template, attribute);
        let path = work.source("unit.c", &source);
        let position = name_position(&source);
        let output = evocc(target, &["-fsyntax-only"], &path);
        let context = format!("{target} {label} {attribute}\n{source}");
        if refuse {
            refused(&output, &at(&path, position, &refusal(attribute)), &context);
        } else {
            warned(&output, &[at(&path, position, WARNING)], &context);
        }
    };
    for target in TARGETS {
        for (label, template, bracketed) in STATEMENTS {
            for attribute in ignored(target) {
                let name = attribute.split('(').next().unwrap();
                let refuse = target == DARWIN && known_to_apple_clang(name, bracketed);
                check(target, template, attribute, refuse, label);
            }
        }
        let mut lists = ARGUMENTS.to_vec();
        if target == LINUX {
            lists.extend(REGPARM_ARGUMENTS);
        }
        for (label, template, bracketed) in ARGUMENT_PLACEMENTS {
            for &attribute in &lists {
                let name = attribute.split('(').next().unwrap();
                let arguments = attribute.contains('(') && !attribute.ends_with("()");
                let refuse = target == DARWIN && arguments && known_to_apple_clang(name, bracketed);
                check(target, template, attribute, refuse, label);
            }
        }
    }
}

/// Rule 4: Apple clang refuses `regparm` in every placement, so Darwin keeps
/// the rule of today. A prototype records it (D17), which is the one
/// difference from Apple clang; every other placement is E2952 at the name.
#[test]
fn darwin_keeps_the_regparm_rule() {
    let work = Work::new("darwin-regparm");
    for (label, template) in &WARNED[..7] {
        let source = spell(template, "regparm(2)");
        let path = work.source("unit.c", &source);
        let output = evocc(DARWIN, &["-fsyntax-only"], &path);
        if *label == "declaration" {
            warned(&output, &[], label);
        } else {
            let expected = at(&path, name_position(&source), &refusal("regparm"));
            refused(&output, &expected, label);
        }
    }
}

/// The option cases of rule 1 and their orders, on both targets.
#[test]
fn warning_options_set_the_level_as_in_the_references() {
    let work = Work::new("options");
    for target in TARGETS {
        for attribute in ignored(target) {
            if attribute != "stdcall" && attribute != "sseregparm" && attribute != "regparm(2)" {
                continue;
            }
            let source = format!("int __attribute__(({attribute})) f(int, int);\n");
            let path = work.source("unit.c", &source);
            let position = name_position(&source);
            for (flags, level) in LEVELS {
                let mut arguments = flags.to_vec();
                arguments.push("-fsyntax-only");
                let output = evocc(target, &arguments, &path);
                let context = format!("{target} {attribute} {flags:?}");
                match level {
                    Level::Warning => warned(&output, &[at(&path, position, WARNING)], &context),
                    Level::Error => refused(&output, &at(&path, position, ERROR), &context),
                    Level::Silent => warned(&output, &[], &context),
                }
            }
        }
    }
}

/// Every driver path warns, and with -Werror=attributes none publishes
/// output: the assembly on stdout or in a file, the syntax check, `--check`
/// and both interpreters.
#[test]
fn each_driver_path_reports_the_warning_and_its_error() {
    let work = Work::new("modes");
    let source = "__attribute__((stdcall)) int add(int a, int b) { return a + b; }\n\
                  int main(void) { return add(2, 3) - 5; }\n";
    let path = work.source("unit.c", source);
    let position = name_position(source);
    let file = work.0.join("unit.s");
    let file_name = file.to_str().unwrap().to_owned();
    let modes: [&[&str]; 6] = [
        &["--emit=asm"],
        &["-S", "-o", file_name.as_str()],
        &["-fsyntax-only"],
        &["--check"],
        &["--interpret-c-hir"],
        &["--interpret-evoir"],
    ];
    for target in TARGETS {
        for mode in modes {
            let context = format!("{target} {mode:?}");
            let output = evocc(target, mode, &path);
            warned(&output, &[at(&path, position, WARNING)], &context);
            if mode[0].starts_with("--interpret") {
                assert_eq!(output.stdout, b"0\n", "{context}");
            }
            if mode[0] == "--check" {
                assert!(
                    String::from_utf8_lossy(&output.stdout)
                        .starts_with("check completed: main returned 0."),
                    "{context}\n{}",
                    text(&output)
                );
            }
            let _ = std::fs::remove_file(&file);
            let mut denied = vec!["-Werror=attributes"];
            denied.extend_from_slice(mode);
            let output = evocc(target, &denied, &path);
            refused(
                &output,
                &at(&path, position, ERROR),
                &format!("{context} denied"),
            );
            assert!(
                !file.exists(),
                "{context}: a denied warning wrote {file_name}"
            );
        }
    }
}

/// The attribute changes no type: two declarations with different
/// conventions agree, a pointer with one converts to and from a pointer
/// without one, and the type queries see one type. GCC 14 and Apple clang
/// warn four times, and the program exits 0.
const TYPES: &str = "int __attribute__((stdcall)) f(int);
int f(int) __attribute__((fastcall));
int __attribute__((sseregparm)) f(int a) { return a + 1; }
int g(int a) { return a + 2; }
int (__attribute__((vectorcall)) *p)(int) = g;
int (*q)(int) = f;
int main(void) {
    int same = __builtin_types_compatible_p(__typeof__(f), __typeof__(g))
        && __builtin_types_compatible_p(__typeof__(p), int (*)(int))
        && _Generic(&f, int (*)(int): 1, default: 0);
    return same && p(1) == 3 && q(1) == 2 ? 0 : 1;
}
";

#[test]
fn the_attribute_changes_no_type() {
    let work = Work::new("types");
    let path = work.source("types.c", TYPES);
    let expected = name_positions(TYPES)
        .into_iter()
        .map(|position| at(&path, position, WARNING))
        .collect::<Vec<_>>();
    assert_eq!(expected.len(), 4);
    for target in TARGETS {
        let modes: [&[&str]; 4] = [
            &["--check"],
            &["--interpret-c-hir"],
            &["--interpret-evoir"],
            &["-O2", "--emit=asm"],
        ];
        for mode in modes {
            let output = evocc(target, mode, &path);
            warned(&output, &expected, &format!("{target} {mode:?}"));
            if mode[0].starts_with("--interpret") {
                assert_eq!(output.stdout, b"0\n", "{target} {mode:?}");
            }
        }
    }
}

const PARAMETERS: &str =
    "long a, long b, long c, long d, long e, long f, long g, long h, long i, double x";
const BODY: &str = "{ return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h + 9 * i + (long)(x * 10.0); }";
const TYPE_LIST: &str = "long, long, long, long, long, long, long, long, long, double";
const MAIN: &str = "int main(void) { return mix(1, 2, 3, 4, 5, 6, 7, 8, 9, 2.5) == 310 ? 0 : 1; }";

/// A callee with nine integer arguments, so that one is on the stack, and a
/// double, with or without the attribute.
fn callee(attribute: Option<&str>) -> String {
    let mark = attribute.map_or(String::new(), |a| format!("__attribute__(({a})) "));
    format!("{mark}long mix({PARAMETERS}) {BODY}\n")
}

/// Its caller, whose prototype has the attribute or not.
fn caller(attribute: Option<&str>) -> String {
    let mark = attribute.map_or(String::new(), |a| format!("__attribute__(({a})) "));
    format!("{mark}long mix({TYPE_LIST});\n{MAIN}\n")
}

/// Each pair of objects, where the attribute is on one side only.
const PAIRS: [(&str, &str); 4] = [
    ("evocc-callee-attribute", "reference-caller-plain"),
    ("reference-callee-attribute", "evocc-caller-plain"),
    ("evocc-caller-attribute", "reference-callee-plain"),
    ("reference-caller-attribute", "evocc-callee-plain"),
];

fn units(attribute: &str) -> [(&'static str, String); 4] {
    [
        ("callee-attribute", callee(Some(attribute))),
        ("callee-plain", callee(None)),
        ("caller-attribute", caller(Some(attribute))),
        ("caller-plain", caller(None)),
    ]
}

/// Darwin: calls in both directions with objects that Apple clang compiled.
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_calls_agree_with_apple_clang_in_both_directions() {
    let work = Work::new("darwin-calls");
    for attribute in NAMES {
        for level in ["-O0", "-O2"] {
            let object = |label: &str| work.0.join(format!("{label}{level}.o"));
            for (label, source) in units(attribute) {
                let path = work.source(&format!("{label}.c"), &source);
                let evocc_object = object(&format!("evocc-{label}"));
                let output = evocc(
                    DARWIN,
                    &[level, "-c", "-o", evocc_object.to_str().unwrap()],
                    &path,
                );
                assert!(
                    output.status.success(),
                    "{attribute} {level} {label}\n{}",
                    text(&output)
                );
                let output = Command::new("clang")
                    .args(["-std=gnu17", level, "-w", "-c"])
                    .arg(&path)
                    .arg("-o")
                    .arg(object(&format!("reference-{label}")))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{attribute} {level} {label}\n{}",
                    text(&output)
                );
            }
            for (first, second) in PAIRS {
                let program = work.0.join(format!("{first}-{second}{level}"));
                let link = Command::new("clang")
                    .arg(object(first))
                    .arg(object(second))
                    .arg("-o")
                    .arg(&program)
                    .output()
                    .unwrap();
                assert!(link.status.success(), "{}", text(&link));
                let run = Command::new(&program).output().unwrap();
                assert_eq!(
                    run.status.code(),
                    Some(0),
                    "{attribute} {level} {first} {second}"
                );
            }
        }
    }
}

/// Linux: calls in both directions with objects that GCC 14 compiled, in the
/// sealed image. `regparm` is one of the names on Linux.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_calls_agree_with_gcc_14_in_both_directions() {
    let work = Work::new("linux-calls");
    let mut script = String::from("set -eu\ncd /work\n");
    let mut lines = 0;
    let mut attributes = NAMES.to_vec();
    attributes.push("regparm(3)");
    for (index, attribute) in attributes.into_iter().enumerate() {
        for level in ["-O0", "-O2"] {
            let stem = format!("u{index}{level}");
            for (label, source) in units(attribute) {
                let path = work.source(&format!("{stem}-{label}.c"), &source);
                let assembly = work.0.join(format!("{stem}-evocc-{label}.s"));
                let output = evocc(
                    LINUX,
                    &[level, "-S", "-o", assembly.to_str().unwrap()],
                    &path,
                );
                assert!(
                    output.status.success(),
                    "{attribute} {level} {label}\n{}",
                    text(&output)
                );
                script.push_str(&format!(
                    "gcc-14 -std=gnu17 {level} -w -c {stem}-{label}.c -o {stem}-reference-{label}.o\n"
                ));
            }
            for (first, second) in PAIRS {
                let file = |label: &str| {
                    if label.starts_with("evocc") {
                        format!("{stem}-{label}.s")
                    } else {
                        format!("{stem}-{label}.o")
                    }
                };
                script.push_str(&format!(
                    "gcc-14 {} {} -o x\n./x\necho '{attribute} {level} {first} {second}: 0'\n",
                    file(first),
                    file(second)
                ));
                lines += 1;
            }
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
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), lines, "{stdout}");
    println!("{stdout}");
}

// c-testsuite at commit 5c7275656d75 (the ladder pin),
// tests/single-exec/00210.c, byte for byte. It is TinyCC's
// tests/tests2/82_attribs_position.c. SHA-256
// 853898afb39a62bfe78e49ec55c71d91a50b75f20ea9d1ffe5976663f45d48e1.
const CTEST_00210: &str = r#"typedef unsigned short uint16_t;
typedef unsigned char uint8_t;

typedef union Unaligned16a {
  uint16_t u;
  uint8_t b[2];
} __attribute__((packed)) Unaligned16a;

typedef union __attribute__((packed)) Unaligned16b {
  uint16_t u;
  uint8_t b[2];
} Unaligned16b;

extern void foo (void) __attribute__((stdcall));
void __attribute__((stdcall)) foo (void)
{
}

/* The actual attribute isn't important, must just be
   parsable.  */
#define ATTR __attribute__((__noinline__))
int ATTR actual_function() {
  return 42;
}

extern int printf (const char *, ...);
int main()
{
    void *function_pointer = &actual_function;

    int a = ((ATTR int(*) (void)) function_pointer)();
    printf("%i\n", a);

    /* In the following we once misparsed 'ATTR *' is a btype
       and hence the whole type was garbled.  */
    int b = ( (int(ATTR *)(void))  function_pointer)();
    printf("%i\n", b);

    return 0;
}
"#;

// tests/single-exec/00210.c.expected. SHA-256
// 7c4ee899b2e4bcd92b425368ebbefeda017a77ad176d66356a0190b7e62f83da.
const CTEST_00210_EXPECTED: &str = "42\n42\n";

/// The original warns at both `stdcall` attributes, as Apple clang does, and
/// compiles on both targets. GCC 14 warns at lines 14 and 16.
#[test]
fn the_ctest_00210_original_warns_twice_and_compiles() {
    let work = Work::new("ctest");
    let path = work.source("00210.c", CTEST_00210);
    for target in TARGETS {
        let output = evocc(target, &["--emit=asm"], &path);
        let expected = [at(&path, (14, 39), WARNING), at(&path, (15, 21), WARNING)];
        warned(&output, &expected, target);
        assert!(!output.stdout.is_empty(), "{target}");
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn the_ctest_00210_original_runs_natively_on_darwin() {
    let work = Work::new("ctest-darwin");
    let path = work.source("00210.c", CTEST_00210);
    let object = work.0.join("00210.o");
    let output = evocc(DARWIN, &["-c", "-o", object.to_str().unwrap()], &path);
    assert!(output.status.success(), "{}", text(&output));
    let program = work.0.join("00210");
    let link = Command::new("clang")
        .arg(&object)
        .arg("-o")
        .arg(&program)
        .output()
        .unwrap();
    assert!(link.status.success(), "{}", text(&link));
    let run = Command::new(&program).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", text(&run));
    assert_eq!(String::from_utf8_lossy(&run.stdout), CTEST_00210_EXPECTED);
}

/// The ctest lane's commands: EvoCC writes the assembly, GCC 14 assembles and
/// links it statically, and the program prints 00210.c.expected.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn the_ctest_00210_original_runs_in_the_sealed_linux_image() {
    let work = Work::new("ctest-linux");
    let path = work.source("00210.c", CTEST_00210);
    let output = evocc(LINUX, &["--emit=asm"], &path);
    assert!(output.status.success(), "{}", text(&output));
    std::fs::write(work.0.join("subject.s"), &output.stdout).unwrap();
    let run = Command::new("container")
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
        .args([
            "evo-nexlab:sealed",
            "sh",
            "-c",
            "cd /work && gcc-14 -c subject.s -o subject.o && gcc-14 -static -o subject subject.o -lm && ./subject",
        ])
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", text(&run));
    assert_eq!(String::from_utf8_lossy(&run.stdout), CTEST_00210_EXPECTED);
}
