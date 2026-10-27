//! N09j: incompatible pointer types, the lint W0012, as in GCC 14 (EvoIR 6.3
//! and 10.1.18). GCC 14 reports `-Wincompatible-pointer-types` for an
//! assignment, an initialization, a prototyped argument, a return and a
//! conditional operator whose pointer targets are incompatible. It is a
//! warning in gnu89 and an error in gnu99 and later modes, `-fpermissive` and
//! `-Wno-error=incompatible-pointer-types` lower it to a warning, and
//! `-Wno-incompatible-pointer-types` removes it. EvoCC follows GCC 14 on both
//! targets, so on Darwin it is stricter than Apple clang, which warns. A
//! warned conversion is the conversion of an explicit cast, and such a
//! conditional has type `void *`.
//!
//! The matrix is the measurement of 2026-10-27 (GCC 14.2.0-19 in
//! evo-nexlab:sealed): five cases times ten directed pointer pairs, in four
//! language modes, with 37 option sets. GCC 14 gives the same result for each
//! of the 50 sources; it depends only on the mode and the options.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const MODES: [&str; 4] = ["-std=gnu89", "-std=gnu99", "-std=gnu11", "-std=gnu17"];
const MESSAGE: &str = "W0012]: incompatible pointer types";
static NEXT: AtomicU32 = AtomicU32::new(0);

/// The ten directed pairs: a label, the target type and the source type, as
/// declarators of `{}`.
const PAIRS: [(&str, &str, &str); 10] = [
    ("int-long", "int *{}", "long *{}"),
    ("long-int", "long *{}", "int *{}"),
    ("structA-structB", "struct A *{}", "struct B *{}"),
    ("structB-structA", "struct B *{}", "struct A *{}"),
    ("arr3-arr4", "int (*{})[3]", "int (*{})[4]"),
    ("arr4-arr3", "int (*{})[4]", "int (*{})[3]"),
    ("arr3-int", "int (*{})[3]", "int *{}"),
    ("int-arr3", "int *{}", "int (*{})[3]"),
    ("fnint-fnlong", "int (*{})(int)", "int (*{})(long)"),
    ("fnlong-fnint", "int (*{})(long)", "int (*{})(int)"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Assign,
    Init,
    Arg,
    Return,
    Cond,
}

const CASES: [Case; 5] = [
    Case::Assign,
    Case::Init,
    Case::Arg,
    Case::Return,
    Case::Cond,
];

/// The result that GCC 14 gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    None,
    Warning,
    Error,
}
use Expect::{Error as E, None as N, Warning as W};

/// Each measured option set in its order, with GCC 14's result in gnu89 and
/// in gnu99, gnu11 and gnu17.
const OPTION_SETS: [(&[&str], Expect, Expect); 37] = [
    (&[], W, E),
    (&["-Wno-error=incompatible-pointer-types"], W, W),
    (&["-fpermissive"], W, W),
    (&["-Wno-incompatible-pointer-types"], N, N),
    (&["-w"], N, E),
    (&["-Werror"], E, E),
    (&["-Werror=incompatible-pointer-types"], E, E),
    (
        &["-Wno-error=incompatible-pointer-types", "-fpermissive"],
        W,
        W,
    ),
    (
        &[
            "-Wno-error=incompatible-pointer-types",
            "-Wno-incompatible-pointer-types",
        ],
        N,
        N,
    ),
    (&["-Wno-error=incompatible-pointer-types", "-w"], N, N),
    (&["-Wno-error=incompatible-pointer-types", "-Werror"], W, W),
    (
        &[
            "-Wno-error=incompatible-pointer-types",
            "-Werror=incompatible-pointer-types",
        ],
        E,
        E,
    ),
    (
        &["-fpermissive", "-Wno-error=incompatible-pointer-types"],
        W,
        W,
    ),
    (&["-fpermissive", "-Wno-incompatible-pointer-types"], N, N),
    (&["-fpermissive", "-w"], N, N),
    (&["-fpermissive", "-Werror"], E, E),
    (
        &["-fpermissive", "-Werror=incompatible-pointer-types"],
        E,
        E,
    ),
    (
        &[
            "-Wno-incompatible-pointer-types",
            "-Wno-error=incompatible-pointer-types",
        ],
        N,
        N,
    ),
    (&["-Wno-incompatible-pointer-types", "-fpermissive"], N, N),
    (&["-Wno-incompatible-pointer-types", "-w"], N, N),
    (&["-Wno-incompatible-pointer-types", "-Werror"], N, N),
    (
        &[
            "-Wno-incompatible-pointer-types",
            "-Werror=incompatible-pointer-types",
        ],
        E,
        E,
    ),
    (&["-w", "-Wno-error=incompatible-pointer-types"], N, N),
    (&["-w", "-fpermissive"], N, N),
    (&["-w", "-Wno-incompatible-pointer-types"], N, N),
    (&["-w", "-Werror"], N, E),
    (&["-w", "-Werror=incompatible-pointer-types"], N, E),
    (&["-Werror", "-Wno-error=incompatible-pointer-types"], W, W),
    (&["-Werror", "-fpermissive"], E, E),
    (&["-Werror", "-Wno-incompatible-pointer-types"], N, N),
    (&["-Werror", "-w"], N, E),
    (&["-Werror", "-Werror=incompatible-pointer-types"], E, E),
    (
        &[
            "-Werror=incompatible-pointer-types",
            "-Wno-error=incompatible-pointer-types",
        ],
        W,
        W,
    ),
    (
        &["-Werror=incompatible-pointer-types", "-fpermissive"],
        E,
        E,
    ),
    (
        &[
            "-Werror=incompatible-pointer-types",
            "-Wno-incompatible-pointer-types",
        ],
        N,
        N,
    ),
    (&["-Werror=incompatible-pointer-types", "-w"], N, E),
    (&["-Werror=incompatible-pointer-types", "-Werror"], E, E),
];

fn gcc14(mode: &str, gnu89: Expect, later: Expect) -> Expect {
    if mode == "-std=gnu89" {
        gnu89
    } else {
        later
    }
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-incompatible-pointers-{label}-{}-{}",
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

fn declare(declarator: &str, name: &str) -> String {
    declarator.replace("{}", name)
}

fn type_name(declarator: &str) -> String {
    declarator.replace("{}", "")
}

/// One measured source, and the line and column where GCC 14 reports. With
/// `cast`, the conversion is written as an explicit cast to the target type
/// (`void *` for both operands of the conditional).
fn measured(case: Case, target: &str, source: &str, cast: bool) -> (String, (usize, usize)) {
    let q = if cast {
        format!("({})q", type_name(target))
    } else {
        "q".to_owned()
    };
    match case {
        Case::Assign => (
            format!(
                "struct A; struct B;\n{};\n{};\nvoid f(void)\n{{\n  p = {q};\n}}\n",
                declare(target, "p"),
                declare(source, "q")
            ),
            (6, 5),
        ),
        Case::Init => {
            let line = format!("  {} = ", declare(target, "p"));
            let column = line.len() + 1;
            (
                format!(
                    "struct A; struct B;\n{};\nvoid f(void)\n{{\n{line}{q};\n  (void)p;\n}}\n",
                    declare(source, "q")
                ),
                (5, column),
            )
        }
        Case::Arg => (
            format!(
                "struct A; struct B;\n{};\nvoid g({});\nvoid f(void)\n{{\n  g({q});\n}}\n",
                declare(source, "q"),
                declare(target, "x")
            ),
            (6, 5),
        ),
        Case::Return => (
            format!(
                "struct A; struct B;\n{};\n{}\n{{\n  return {q};\n}}\n",
                declare(source, "q"),
                declare(target, "f(void)")
            ),
            (5, 10),
        ),
        Case::Cond => {
            let operands = if cast {
                "(void *)p : (void *)q"
            } else {
                "p : q"
            };
            (
                format!(
                    "struct A; struct B;\n{};\n{};\nint c;\nvoid f(void)\n{{\n  (void)(c ? {operands});\n}}\n",
                    declare(target, "p"),
                    declare(source, "q")
                ),
                (7, 16),
            )
        }
    }
}

/// The exact diagnostics of a run that GCC 14 answers with `expect`.
fn expected_stderr(path: &Path, position: (usize, usize), expect: Expect) -> String {
    let level = match expect {
        Expect::None => return String::new(),
        Expect::Warning => "warning",
        Expect::Error => "error",
    };
    format!(
        "{}:{}:{}: {level}[{MESSAGE}\n",
        path.display(),
        position.0,
        position.1
    )
}

/// One compilation of the matrix and its expected result.
struct Job {
    target: &'static str,
    flags: Vec<&'static str>,
    source: String,
    position: (usize, usize),
    expect: Expect,
    strict: bool,
    context: String,
}

/// Run the jobs on four threads and collect every disagreement.
fn run_jobs(label: &str, jobs: Vec<Job>) {
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let jobs = &jobs;
            let next = &next;
            let failures = &failures;
            scope.spawn(move || {
                let work = Work::new(&format!("{label}-{worker}"));
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(index) else { break };
                    let path = work.source(&format!("case{index}.c"), &job.source);
                    let mut flags = job.flags.clone();
                    flags.push(if job.strict {
                        "--emit=asm"
                    } else {
                        "-fsyntax-only"
                    });
                    let output = evocc(job.target, &flags, &path);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let wanted = expected_stderr(&path, job.position, job.expect);
                    let code = if job.expect == Expect::Error { 1 } else { 0 };
                    let produced = !output.stdout.is_empty();
                    let output_ok = !job.strict || produced == (code == 0);
                    if output.status.code() != Some(code) || stderr != wanted || !output_ok {
                        failures.lock().unwrap().push(format!(
                            "{}: expected {:?}\n  {}",
                            job.context,
                            job.expect,
                            stderr.trim_end().replace('\n', "\n  ")
                        ));
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    assert!(
        failures.is_empty(),
        "{} of {} runs disagree with GCC 14:\n{}",
        failures.len(),
        jobs.len(),
        failures[..failures.len().min(40)].join("\n")
    );
}

/// Rule 2: each of the 50 sources follows GCC 14 in each mode, on both
/// targets, by default and with the two options of the LLVM originals.
#[test]
fn measured_cases_follow_gcc_14() {
    let mut jobs = Vec::new();
    for target in TARGETS {
        for mode in MODES {
            for (options, gnu89, later) in [OPTION_SETS[0], OPTION_SETS[1], OPTION_SETS[3]] {
                for case in CASES {
                    for (pair, left, right) in PAIRS {
                        let (source, position) = measured(case, left, right, false);
                        let mut flags = vec![mode];
                        flags.extend_from_slice(options);
                        jobs.push(Job {
                            target,
                            context: format!("{target} {flags:?} {case:?} {pair}"),
                            flags,
                            source,
                            position,
                            expect: gcc14(mode, gnu89, later),
                            strict: false,
                        });
                    }
                }
            }
        }
    }
    run_jobs("cases", jobs);
}

/// Rule 2: every measured option set and order, on one pair of each case.
#[test]
fn measured_option_sets_and_orders_follow_gcc_14() {
    let mut jobs = Vec::new();
    for target in TARGETS {
        for mode in MODES {
            for (options, gnu89, later) in OPTION_SETS {
                for case in CASES {
                    let (source, position) = measured(case, "int *{}", "long *{}", false);
                    let mut flags = vec![mode];
                    flags.extend_from_slice(options);
                    jobs.push(Job {
                        target,
                        context: format!("{target} {flags:?} {case:?}"),
                        flags,
                        source,
                        position,
                        expect: gcc14(mode, gnu89, later),
                        strict: false,
                    });
                }
            }
        }
    }
    run_jobs("options", jobs);
}

/// The strict driver gives the same levels as the cc command line, and it
/// accepts `-fpermissive`.
#[test]
fn the_strict_driver_follows_the_same_levels() {
    let mut jobs = Vec::new();
    for target in TARGETS {
        for mode in MODES {
            for (options, gnu89, later) in &OPTION_SETS[..7] {
                for case in CASES {
                    let (source, position) = measured(case, "int *{}", "long *{}", false);
                    let mut flags = vec![mode];
                    flags.extend_from_slice(options);
                    jobs.push(Job {
                        target,
                        context: format!("{target} --emit=asm {flags:?} {case:?}"),
                        flags,
                        source,
                        position,
                        expect: gcc14(mode, *gnu89, *later),
                        strict: true,
                    });
                }
            }
        }
    }
    run_jobs("strict", jobs);
}

/// Three-option sets, `-fno-permissive` and the ISO modes, as GCC 14 gives
/// them (the plan's probe `n1.c`; `-fpermissive` is the default only for
/// C89 and GNU89).
#[test]
fn other_option_sets_and_iso_modes_follow_gcc_14() {
    let sets: [(&[&'static str], Expect); 9] = [
        (&["-std=gnu17", "-fpermissive", "-Werror", "-w"], N),
        (
            &[
                "-std=gnu17",
                "-Werror=incompatible-pointer-types",
                "-fpermissive",
                "-w",
            ],
            N,
        ),
        (
            &[
                "-std=gnu17",
                "-Wno-error=incompatible-pointer-types",
                "-Werror=incompatible-pointer-types",
                "-w",
            ],
            E,
        ),
        (&["-std=gnu17", "-fpermissive", "-fno-permissive"], E),
        (&["-std=gnu17", "-Wincompatible-pointer-types"], E),
        (&["-std=gnu17", "-Wno-error"], E),
        (&["-std=c89"], W),
        (&["-ansi"], W),
        (&["-std=c99"], E),
    ];
    let mut jobs = Vec::new();
    for target in TARGETS {
        for (flags, expect) in sets {
            let (source, position) = measured(Case::Init, "int *{}", "long *{}", false);
            jobs.push(Job {
                target,
                context: format!("{target} {flags:?}"),
                flags: flags.to_vec(),
                source,
                position,
                expect,
                strict: false,
            });
        }
    }
    run_jobs("other", jobs);
}

/// EvoCC refuses the gnu23 mode, which GCC 14 accepts.
#[test]
fn gnu23_keeps_its_mode_refusal() {
    let work = Work::new("gnu23");
    let (source, _) = measured(Case::Init, "int *{}", "long *{}", false);
    let path = work.source("unit.c", &source);
    for target in TARGETS {
        let output = evocc(target, &["-std=gnu23", "-fsyntax-only"], &path);
        assert_eq!(output.status.code(), Some(1), "{target}\n{}", text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("unsupported C language mode `-std=gnu23`"),
            "{target}\n{}",
            text(&output)
        );
    }
}

/// Rule 4: a warned or allowed conversion is the conversion of an explicit
/// cast, so the assembly is the assembly of the cast source.
#[test]
fn a_warned_conversion_is_an_explicit_cast() {
    let work = Work::new("casts");
    let mut failures = Vec::new();
    for target in TARGETS {
        for flags in [
            &["-std=gnu89", "--emit=asm"][..],
            &[
                "-std=gnu17",
                "-Wno-incompatible-pointer-types",
                "--emit=asm",
            ][..],
        ] {
            for case in CASES {
                for (pair, left, right) in PAIRS {
                    let (implicit, position) = measured(case, left, right, false);
                    let (explicit, _) = measured(case, left, right, true);
                    let implicit_path = work.source("implicit.c", &implicit);
                    let explicit_path = work.source("explicit.c", &explicit);
                    let warned = evocc(target, flags, &implicit_path);
                    let cast = evocc(target, flags, &explicit_path);
                    let expect = if flags[0] == "-std=gnu89" { W } else { N };
                    let context = format!("{target} {flags:?} {case:?} {pair}");
                    if warned.status.code() != Some(0)
                        || String::from_utf8_lossy(&warned.stderr)
                            != expected_stderr(&implicit_path, position, expect)
                    {
                        failures.push(format!("{context}: implicit\n{}", text(&warned)));
                    } else if cast.status.code() != Some(0) || !cast.stderr.is_empty() {
                        failures.push(format!("{context}: explicit\n{}", text(&cast)));
                    } else if warned.stdout != cast.stdout || warned.stdout.is_empty() {
                        failures.push(format!("{context}: assembly differs"));
                    }
                }
            }
        }
    }
    // A file-scope initializer and a member initializer use the same cast.
    for (implicit, explicit) in [
        ("int i;\nlong *q = &i;\n", "int i;\nlong *q = (long *)&i;\n"),
        (
            "struct s { int (*fn)(int); };\nint g(long);\nstruct s v = { g };\n",
            "struct s { int (*fn)(int); };\nint g(long);\nstruct s v = { (int (*)(int))g };\n",
        ),
    ] {
        for target in TARGETS {
            let flags = [
                "-std=gnu17",
                "-Wno-incompatible-pointer-types",
                "--emit=asm",
            ];
            let warned = evocc(target, &flags, &work.source("implicit.c", implicit));
            let cast = evocc(target, &flags, &work.source("explicit.c", explicit));
            if !warned.status.success()
                || !warned.stderr.is_empty()
                || warned.stdout != cast.stdout
                || !cast.status.success()
            {
                failures.push(format!(
                    "{target} {implicit}\n{}\n{}",
                    text(&warned),
                    text(&cast)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// GCC 14 and Apple clang give such a conditional the type `void *`, without
/// the qualifiers of the operands.
#[test]
fn a_mismatched_conditional_has_type_void_pointer() {
    let work = Work::new("conditional-type");
    let path = work.source(
        "unit.c",
        "const int *p; long *q; int c;\n\
         _Static_assert(__builtin_types_compatible_p(__typeof__(c ? p : q), void *), \"void *\");\n\
         int main(void) { return 0; }\n",
    );
    for target in TARGETS {
        let output = evocc(
            target,
            &[
                "-std=gnu17",
                "-Wno-incompatible-pointer-types",
                "-fsyntax-only",
            ],
            &path,
        );
        assert_eq!(output.status.code(), Some(0), "{target}\n{}", text(&output));
        assert!(output.stderr.is_empty(), "{target}\n{}", text(&output));
    }
}

/// The source without its `@` marker, and the line and column of the marker.
fn marked(source: &str) -> (String, (usize, usize)) {
    let offset = source.find('@').expect("marker");
    let line = source[..offset].matches('\n').count() + 1;
    let column = offset - source[..offset].rfind('\n').map_or(0, |at| at + 1) + 1;
    (source.replacen('@', "", 1), (line, column))
}

/// Forms outside the matrix that GCC 14 reports with
/// `-Wincompatible-pointer-types`, each at the marker (the plan's probes
/// `c1.c`, `c2.c`, `j1.c`, `j2.c` and `n1.c`). EvoCC accepted the
/// conditionals, `_Atomic int *` from `int *` and `(long *)0` before.
const BEYOND: [&str; 22] = [
    "int *s = @\"abc\";\n",
    "void g(int *);\nvoid f(void) { g(@\"abc\"); }\n",
    "long long *l;\nvoid f(void) { long *q = @l; (void)q; }\n",
    "enum E { X, Y } e;\nint *p = @&e;\n",
    "int i;\nenum E { X, Y };\nenum E *p = @&i;\n",
    "int **pp;\nvoid f(void) { const int *const *q = @pp; (void)q; }\n",
    "int i;\n_Atomic int *a = @&i;\n",
    "_Atomic int counter;\nint *f(void) { return @&counter; }\n",
    "int *a = @(long *)0;\n",
    "int *f(void) { return @(long *)0; }\n",
    "int *p; int c;\nvoid f(void) { (void)(c ? p @: (long *)0); }\n",
    "int *a; unsigned *b; int c;\nvoid f(void) { (void)(c ? a @: b); }\n",
    "char *a; unsigned char *b; int c;\nvoid f(void) { (void)(c ? a @: b); }\n",
    "char *a; signed char *b; int c;\nvoid f(void) { char *r = c ? a @: b; (void)r; }\n",
    "const int *a; long *b; int c;\nvoid f(void) { (void)(c ? a @: b); }\n",
    "long *a; long long *b; int c;\nvoid f(void) { (void)(c ? a @: b); }\n",
    "int *a; int c;\nvoid f(void) { (void)(c ? a @: \"abc\"); }\n",
    "int i;\nlong *q = @&i;\n",
    "int *p; long *q;\nvoid f(void) { p @= q; }\n",
    "int *p; long *q;\nvoid f(void) { p @= q, p = 0; }\n",
    "struct s { int (*fn)(int); };\nint g(long);\nstruct s v = { @g };\n",
    "int *p; long *q;\nvoid f(void) { (void)(p ?@: q); }\n",
];

#[test]
fn forms_beyond_the_matrix_follow_gcc_14() {
    let work = Work::new("beyond");
    let mut failures = Vec::new();
    for target in TARGETS {
        for marked_source in BEYOND {
            let (source, position) = marked(marked_source);
            let path = work.source("unit.c", &source);
            let output = evocc(target, &["-std=gnu17", "-fsyntax-only"], &path);
            if output.status.code() != Some(1)
                || String::from_utf8_lossy(&output.stderr) != expected_stderr(&path, position, E)
            {
                failures.push(format!("{target}: {source}{}", text(&output)));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Pairs that GCC 14 accepts without a diagnostic stay silent.
#[test]
fn compatible_pairs_stay_silent() {
    let work = Work::new("silent");
    for target in TARGETS {
        for source in [
            "enum E { X, Y } e;\nunsigned *u = &e;\n",
            "unsigned char *u = \"x\";\n",
            "int (*fp)(void); void *vp; int c;\nvoid f(void) { (void)(c ? fp : vp); (void)(c ? vp : fp); }\n",
            "int *p; int c;\nvoid f(void) { (void)(c ? p : (void *)0); (void)(c ? p : 0); (void)(c ? 0 : p); }\n",
            "int *p; const int *q; int c;\nvoid f(void) { const int *r = c ? p : q; (void)r; }\n",
            "int g(void);\nvoid *v = (void *)g;\nvoid *w = g;\n",
            "int a[3];\nint (*p)[] = &a;\n",
            "void *v; int *p;\nvoid f(void) { p = v; v = p; }\n",
            "int *p;\nvoid f(void) { p = 0; p = (void *)0; }\n",
        ] {
            let path = work.source("unit.c", source);
            let output = evocc(target, &["-std=gnu17", "-fsyntax-only"], &path);
            assert_eq!(output.status.code(), Some(0), "{target} {source}\n{}", text(&output));
            assert!(output.stderr.is_empty(), "{target} {source}\n{}", text(&output));
        }
    }
}

/// Rule 3: every other conversion keeps its rule, also with `-fpermissive`
/// and `-Wno-incompatible-pointer-types`.
#[test]
fn other_conversions_keep_their_rule() {
    let work = Work::new("other");
    let refused = [
        ("int *p = @1;\n", "error[E2202]: invalid-initializer-type"),
        ("int *q;\nint f(void) { int x = @q; return x; }\n", "error[E2202]: invalid-initializer-type"),
        (
            "struct A { int n; } a;\nstruct B { int n; } b;\nvoid f(void) { struct A c = @b; (void)c; }\n",
            "error[E2202]: invalid-initializer-type",
        ),
        ("_Atomic int ai;\nvoid *v = @&ai;\n", "error[E2202]: invalid-initializer-type"),
        (
            "typedef union { int *p; long *q; } U __attribute__((transparent_union));\nvoid f(U);\nvoid g(double *p) { f(@p); }\n",
            "error[E2202]: invalid-initializer-type",
        ),
        ("int *p;\nvoid f(void) { @p = 7; }\n", "error[E2102]: invalid-update-operands"),
    ];
    for target in TARGETS {
        for extra in [
            &[][..],
            &["-fpermissive"][..],
            &["-Wno-incompatible-pointer-types"][..],
        ] {
            let mut flags = vec!["-std=gnu17", "-fsyntax-only"];
            flags.extend_from_slice(extra);
            for (marked_source, diagnostic) in refused {
                let (source, (line, column)) = marked(marked_source);
                let path = work.source("unit.c", &source);
                let output = evocc(target, &flags, &path);
                let first = format!("{}:{line}:{column}: {diagnostic}", path.display());
                assert_eq!(
                    output.status.code(),
                    Some(1),
                    "{target} {flags:?} {source}\n{}",
                    text(&output)
                );
                assert_eq!(
                    String::from_utf8_lossy(&output.stderr).lines().next(),
                    Some(first.as_str()),
                    "{target} {flags:?} {source}"
                );
            }
        }
        let (source, (line, column)) = marked("void f(const int *p) { int *q = @p; (void)q; }\n");
        let path = work.source("qualifier.c", &source);
        let output = evocc(target, &["-std=gnu17", "-fsyntax-only"], &path);
        assert_eq!(output.status.code(), Some(0), "{}", text(&output));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!(
                "{}:{line}:{column}: warning[W0008]: pointer target discards qualifiers\n",
                path.display()
            )
        );
        let (source, (line, column)) = marked("void f(unsigned *p) { int *q = @p; (void)q; }\n");
        let path = work.source("sign.c", &source);
        let silent = evocc(target, &["-std=gnu17", "-fsyntax-only"], &path);
        assert!(
            silent.status.success() && silent.stderr.is_empty(),
            "{}",
            text(&silent)
        );
        let warned = evocc(target, &["-std=gnu17", "-Wall", "-fsyntax-only"], &path);
        assert_eq!(
            String::from_utf8_lossy(&warned.stderr),
            format!(
                "{}:{line}:{column}: warning[W0006]: pointer target differs in signedness\n",
                path.display()
            ),
            "{}",
            text(&warned)
        );
    }
}

/// GCC 14 reports only the incompatibility, never a qualifier warning with it.
#[test]
fn an_incompatible_pair_reports_no_qualifier_warning() {
    let work = Work::new("one-report");
    let (source, position) = marked("void f(const int *p) { long *q = @p; (void)q; }\n");
    let path = work.source("unit.c", &source);
    for target in TARGETS {
        let output = evocc(target, &["-std=gnu89", "-fsyntax-only"], &path);
        assert_eq!(output.status.code(), Some(0), "{target}\n{}", text(&output));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            expected_stderr(&path, position, W),
            "{target}"
        );
    }
}

/// In a system header GCC 14 reports the error of a deny base level, also
/// under `-w`, and suppresses each warning (the plan's probe of the header
/// rule).
#[test]
fn system_headers_follow_gcc_14() {
    let work = Work::new("system-header");
    std::fs::create_dir_all(work.0.join("sys")).unwrap();
    work.source(
        "sys/h.h",
        "static inline int *h(long *q) { int *p = q; return p; }\n",
    );
    let path = work.source(
        "unit.c",
        "#include <h.h>\nint main(void) { long l = 0; return h(&l) != 0; }\n",
    );
    let system = format!("-isystem{}", work.0.join("sys").display());
    for target in TARGETS {
        for (flags, expect) in [
            (&["-std=gnu17"][..], E),
            (&["-std=gnu17", "-w"][..], E),
            (&["-std=gnu17", "-Werror=incompatible-pointer-types"][..], E),
            (&["-std=gnu89"][..], N),
            (&["-std=gnu89", "-Werror"][..], N),
            (&["-std=gnu89", "-Werror=incompatible-pointer-types"][..], N),
            (&["-std=gnu17", "-fpermissive"][..], N),
            (
                &["-std=gnu17", "-Wno-error=incompatible-pointer-types"][..],
                N,
            ),
            (
                &[
                    "-std=gnu17",
                    "-Wno-error=incompatible-pointer-types",
                    "-Wsystem-headers",
                ][..],
                W,
            ),
        ] {
            let mut all = vec![system.as_str(), "-fsyntax-only"];
            all.extend_from_slice(flags);
            let output = evocc(target, &all, &path);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let context = format!("{target} {flags:?}\n{}", text(&output));
            match expect {
                Expect::None => {
                    assert_eq!(output.status.code(), Some(0), "{context}");
                    assert!(stderr.is_empty(), "{context}");
                }
                Expect::Warning => {
                    assert_eq!(output.status.code(), Some(0), "{context}");
                    assert!(stderr.contains("h.h:1:42: warning[W0012]"), "{context}");
                }
                Expect::Error => {
                    assert_eq!(output.status.code(), Some(1), "{context}");
                    assert!(stderr.contains("h.h:1:42: error[W0012]"), "{context}");
                }
            }
        }
    }
}

const VALID_ACCESS: &str = "short p[5] = {1, 2, 3, 4, 5};\n\
    int main(void) {\n\
    \x20 short *ip = &p;\n\
    \x20 return ip[1] - 2;\n\
    }\n";
const INVALID_ACCESS: &str = "short p[5] = {1, 2, 3, 4, 5};\n\
    int main(void) {\n\
    \x20 short *ip = &p;\n\
    \x20 return ip[5];\n\
    }\n";

/// Rule 4: both interpreters and `--check` keep each check of a later
/// access, as after the explicit cast: a valid access returns 0, and an
/// access past the object is the existing E3011.
#[test]
fn interpreters_keep_the_checks_of_a_later_access() {
    let work = Work::new("interpreters");
    let flags = ["-std=gnu17", "-Wno-incompatible-pointer-types"];
    for target in TARGETS {
        for (source, valid) in [(VALID_ACCESS, true), (INVALID_ACCESS, false)] {
            let cast = source.replace("= &p;", "= (short *)&p;");
            for mode in ["--interpret-c-hir", "--interpret-evoir", "--check"] {
                let mut all = flags.to_vec();
                all.push(mode);
                let path = work.source("unit.c", source);
                let output = evocc(target, &all, &path);
                let path = work.source("unit.c", &cast);
                let explicit = evocc(target, &all, &path);
                let context = format!("{target} {mode} valid={valid}\n{}", text(&output));
                assert_eq!(
                    output.status,
                    explicit.status,
                    "{context}\n{}",
                    text(&explicit)
                );
                assert_eq!(
                    output.stdout,
                    explicit.stdout,
                    "{context}\n{}",
                    text(&explicit)
                );
                assert_eq!(
                    output.stderr,
                    explicit.stderr,
                    "{context}\n{}",
                    text(&explicit)
                );
                if valid {
                    assert_eq!(output.status.code(), Some(0), "{context}");
                    assert!(output.stderr.is_empty(), "{context}");
                } else {
                    assert_eq!(output.status.code(), Some(1), "{context}");
                    let first = String::from_utf8_lossy(&output.stderr)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_owned();
                    assert!(
                        first.contains(":4:") && first.contains("error[E3011]"),
                        "{context}"
                    );
                }
            }
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn a_valid_later_access_runs_natively_on_darwin() {
    let work = Work::new("darwin");
    let path = work.source("unit.c", VALID_ACCESS);
    let object = work.0.join("unit.o");
    let output = evocc(
        DARWIN,
        &[
            "-std=gnu17",
            "-Wno-incompatible-pointer-types",
            "-c",
            "-o",
            object.to_str().unwrap(),
        ],
        &path,
    );
    assert!(output.status.success(), "{}", text(&output));
    let program = work.0.join("unit");
    let link = Command::new("clang")
        .arg(&object)
        .arg("-o")
        .arg(&program)
        .output()
        .unwrap();
    assert!(link.status.success(), "{}", text(&link));
    let run = Command::new(&program).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", text(&run));
}

fn sealed(work: &Work, script: &str, mounts: &[(PathBuf, &str)]) -> Output {
    let mut command = Command::new("container");
    command.args([
        "run",
        "--rm",
        "--platform",
        "linux/arm64",
        "--network",
        "none",
        "-v",
    ]);
    command.arg(format!("{}:/work", work.0.display()));
    for (source, target) in mounts {
        command
            .arg("-v")
            .arg(format!("{}:{target}:ro", source.display()));
    }
    command
        .args(["evo-nexlab:sealed", "sh", "-c", script])
        .output()
        .unwrap()
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn a_valid_later_access_runs_in_the_sealed_linux_image() {
    let work = Work::new("linux");
    let path = work.source("unit.c", VALID_ACCESS);
    let output = evocc(
        LINUX,
        &[
            "-std=gnu17",
            "-Wno-incompatible-pointer-types",
            "--emit=asm",
        ],
        &path,
    );
    assert!(output.status.success(), "{}", text(&output));
    std::fs::write(work.0.join("unit.s"), &output.stdout).unwrap();
    let run = sealed(&work, "cd /work && gcc-14 unit.s -o unit && ./unit", &[]);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run));
}

/// The pinned LLVM test-suite tree of the application ladder (read only).
const LLVM_TREE: &str =
    ".build/ladder/llvm-test-suite/llvm-test-suite-2919e55176a572ee085cb46b291c47b7092c363e";

/// PolyBench `correlation` and Shootout `methcall` were old PASS programs of
/// the LLVM test suite. Their original commands pass
/// `-Wno-error=incompatible-pointer-types` and `-w`. EvoCC compiles each
/// with its original arguments (the compiler path, `-o` and `-c` replaced by
/// `--emit=asm`), GCC 14 links it with the original link rule, and the lit
/// output (the merged output, then `exit N`) equals the suite's reference:
/// for `correlation` through its MD5 (`HashProgramOutput.sh`).
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn the_two_llvm_originals_run_in_the_sealed_linux_image() {
    let tree = root().join(LLVM_TREE);
    assert!(
        tree.is_dir(),
        "the pinned LLVM test-suite tree is required for this acceptance test"
    );
    let single = tree.join("SingleSource/Benchmarks");
    let utilities = single.join("Polybench/utilities");
    let originals: [(&str, Vec<String>, PathBuf); 2] = [
        (
            "correlation",
            [
                "-DFP_ABSTOLERANCE=1e-5",
                "-DNDEBUG",
                "-O3",
                "-DNDEBUG",
                "-w",
                "-Werror=date-time",
                "-I",
                utilities.to_str().unwrap(),
                "-DPOLYBENCH_DUMP_ARRAYS",
                "-ffp-contract=off",
                "-DFMA_DISABLED=1",
                "-Wno-error=incompatible-pointer-types",
                "-ffp-contract=off",
                "-DFMA_DISABLED=1",
            ]
            .map(str::to_owned)
            .to_vec(),
            single.join("Polybench/datamining/correlation/correlation.c"),
        ),
        (
            "methcall",
            [
                "-DNDEBUG",
                "-O3",
                "-DNDEBUG",
                "-std=gnu17",
                "-w",
                "-Werror=date-time",
                "-Wno-error=incompatible-pointer-types",
            ]
            .map(str::to_owned)
            .to_vec(),
            single.join("Shootout/methcall.c"),
        ),
    ];
    let work = Work::new("llvm");
    for (name, arguments, source) in &originals {
        let mut flags: Vec<&str> = arguments.iter().map(String::as_str).collect();
        flags.push("--emit=asm");
        let output = evocc(LINUX, &flags, source);
        assert!(output.status.success(), "{name}: {}", text(&output));
        assert!(output.stderr.is_empty(), "{name}: {}", text(&output));
        std::fs::write(work.0.join(format!("{name}.s")), &output.stdout).unwrap();
    }
    let script = "set -eu\ncd /work\n\
        for name in correlation methcall; do\n\
          gcc-14 -O3 -DNDEBUG -no-pie $name.s -o $name -lm\n\
          status=0; ./$name < /dev/null > $name.out 2>&1 || status=$?\n\
          echo \"exit $status\" >> $name.out\n\
        done\n\
        md5sum < correlation.out\n\
        cat methcall.out\n";
    let run = sealed(&work, script, &[]);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        stdout,
        "393859cceab734224d73b1d142485f2f  -\ntrue\n\ntrue\n\nexit 0\n",
        "{}",
        text(&run)
    );
}
