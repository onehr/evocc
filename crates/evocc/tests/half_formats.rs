//! Binary16 and bfloat16 execution (L03), as the 2026-10-13 rule decides.
//!
//! `_Float16`, `__fp16` and `__bf16` are storage formats. Each operation
//! converts its operands to binary32, evaluates in binary32 and rounds once
//! to the half format where the value is stored, as GCC 14 does on Armv8-A.
//! An `__fp16` operand promotes to `float`. Each conversion rounds once. A
//! half value crosses a call in the low 16 bits of a vector register, and a
//! binary16 record of one to four members is an HFA; a record that AAPCS64
//! makes an HFA because of a `__bf16` member is refused (E3049), and so are
//! anonymous `_Float16` and `__bf16` arguments on Darwin.
//!
//! The tests marked as the sealed lane need `evo-nexlab:sealed` and run
//! explicitly and serially. The Darwin tests run on this host.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
const KINDS: [u32; 3] = [0, 1, 2];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name)
}

fn packet(name: &str) -> PathBuf {
    fixture("rules-20261013").join(name)
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The machine-read lines of a recorded fixture, after its `records:` line.
fn records(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(fixture(name)).unwrap();
    let (_, body) = text.split_once("\nrecords:\n").expect("records marker");
    body.lines().map(str::to_owned).collect()
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("evocc-half-formats-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn copy(&self, source: &Path) -> PathBuf {
        let path = self.0.join(source.file_name().unwrap());
        std::fs::copy(source, &path).unwrap();
        path
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed half-format probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn evocc<S: AsRef<std::ffi::OsStr>>(target: &str, args: &[S], source: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .arg(format!("--target={target}"))
        .args(["-std=gnu11", "-w", "--error-format=short"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.args(args).arg(source).output().unwrap()
}

/// Run `script` in the sealed Linux image with `work` mounted at `/work`.
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

/// Main's value as an interpreter reports it.
fn interpreted(output: &Output) -> Option<i64> {
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

/// The first diagnostic code of a failed command.
fn first_code(output: &Output) -> Option<String> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr.split(['[', ']', ' ', ':']).find_map(|word| {
        (word.len() == 5
            && word.starts_with(['E', 'W'])
            && word[1..].bytes().all(|b| b.is_ascii_digit()))
        .then(|| word.to_owned())
    })
}

/// Run every job on a few threads; return the failures, sorted.
fn parallel<J: Sync>(jobs: &[J], check: impl Fn(&J) -> Option<String> + Sync) -> Vec<String> {
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(2, 6));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(job) = jobs.get(index) else {
                    break;
                };
                if let Some(failure) = check(job) {
                    failures.lock().unwrap().push(failure);
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap();
    failures.sort();
    failures
}

/// Run a native Darwin program built by EvoCC or by Apple clang.
#[cfg(target_os = "macos")]
fn darwin_program(
    work: &Work,
    name: &str,
    compiler: &str,
    args: &[String],
    source: &Path,
) -> Output {
    let program = work.path(name);
    let build = if compiler == "evocc" {
        let mut args = args.to_vec();
        args.extend(["-o".into(), program.to_str().unwrap().into()]);
        evocc(DARWIN, &args, source)
    } else {
        Command::new("clang")
            .args(["-std=gnu11", "-w"])
            .args(args)
            .arg(source)
            .arg("-o")
            .arg(&program)
            .output()
            .unwrap()
    };
    assert!(
        build.status.success(),
        "{compiler} {name}\n{}",
        text(&build)
    );
    Command::new(&program).output().unwrap()
}

// ---------------------------------------------------------------------------
// The fixed batch probe and its original commands.

/// The reference line of floating-half-boundaries.c for each KIND; both
/// cases print it, and so do GCC 14 and Clang 19 at -O0 and -O2.
fn boundary_line(kind: u32) -> &'static str {
    match kind {
        0 => "size=2 align=2 unary=2 bytes=4000 input=3e00",
        1 => "size=2 align=2 unary=4 bytes=4000 input=3e00",
        _ => "size=2 align=2 unary=2 bytes=4000 input=3fc0",
    }
}

/// The six cases of the fixed batch that EvoCC refused run unchanged in
/// both interpreters on both targets. A local function takes the place of
/// printf and compares the printed line with the reference line.
#[test]
fn boundary_probe_prints_the_reference_lines_in_both_interpreters() {
    let wrapper = fixture("floating-half-boundaries-interpreted.c");
    let mut jobs = Vec::new();
    for target in TARGETS {
        for kind in KINDS {
            for case in 0..2 {
                for interpreter in INTERPRETERS {
                    for level in LEVELS {
                        jobs.push((target, kind, case, interpreter, level));
                    }
                }
            }
        }
    }
    let failures = parallel(&jobs, |&(target, kind, case, interpreter, level)| {
        let args = [
            format!("-DKIND={kind}"),
            format!("-DCASE={case}"),
            level.into(),
            interpreter.into(),
        ];
        let output = evocc(target, &args, &wrapper);
        (interpreted(&output) != Some(0)).then(|| {
            format!(
                "{target} KIND={kind} CASE={case} {interpreter} {level}: {}",
                text(&output)
            )
        })
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The eight original commands that T01b recorded: KIND=1 and 2, CASE=1,
/// -O0 and -O2, EvoIR and native emission. They refused with E3049; they
/// now emit the half interface. The EvoIR interpreter stops at printf.
#[test]
fn original_boundary_commands_emit_the_half_interface() {
    let probe = fixture("floating-half-boundaries.c");
    let work = Work::new("original");
    for (kind, ty) in [(1, "f16"), (2, "bf16")] {
        for level in LEVELS {
            let defines = [format!("-DKIND={kind}"), "-DCASE=1".to_owned()];
            let mut args = defines.to_vec();
            args.extend([level.into(), "--emit=evoir".into()]);
            let output = evocc(LINUX, &args, &probe);
            assert!(
                output.status.success(),
                "KIND={kind} {level}\n{}",
                text(&output)
            );
            let module = String::from_utf8_lossy(&output.stdout);
            assert!(
                module.contains(&format!("func @step(%arg0: {ty},")),
                "KIND={kind} {level}\n{module}"
            );
            let assembly = work.path(&format!("boundary-{kind}{level}.s"));
            let mut args = defines.to_vec();
            args.extend([
                level.into(),
                "-S".into(),
                "-o".into(),
                assembly.to_str().unwrap().into(),
            ]);
            let output = evocc(LINUX, &args, &probe);
            assert!(
                output.status.success(),
                "KIND={kind} {level}\n{}",
                text(&output)
            );
            let mut args = defines.to_vec();
            args.extend([level.into(), "--interpret-evoir".into()]);
            let output = evocc(LINUX, &args, &probe);
            assert_eq!(
                first_code(&output).as_deref(),
                Some("E3039"),
                "{}",
                text(&output)
            );
        }
    }
}

/// Native Linux code prints the reference line for the six cases at -O0
/// and -O2, and the 24 reference executions of GCC 14 and Clang 19 still
/// print it.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn boundary_probe_prints_the_reference_lines_natively() {
    let work = Work::new("boundaries");
    let probe = work.copy(&fixture("floating-half-boundaries.c"));
    let mut script = String::from("cd /work\n");
    for kind in KINDS {
        for case in 0..2 {
            for level in LEVELS {
                let name = format!("evocc-{kind}-{case}{level}.s");
                let args = [
                    format!("-DKIND={kind}"),
                    format!("-DCASE={case}"),
                    level.into(),
                    "-S".into(),
                    "-o".into(),
                    work.path(&name).to_str().unwrap().into(),
                ];
                let output = evocc(LINUX, &args, &probe);
                assert!(
                    output.status.success(),
                    "KIND={kind} CASE={case}\n{}",
                    text(&output)
                );
                script.push_str(&format!(
                    "gcc-14 {name} -o t -lm && echo \"evocc {kind} {level} $(./t) exit=$?\"\n"
                ));
                for compiler in ["gcc-14", "clang-19"] {
                    script.push_str(&format!(
                        "{compiler} -std=gnu11 {level} -w -DKIND={kind} -DCASE={case} floating-half-boundaries.c -o r -lm && echo \"{compiler} {kind} {level} $(./r) exit=$?\"\n"
                    ));
                }
            }
        }
    }
    let output = sealed(&work, &script);
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    let mut seen = 0;
    for line in stdout.lines() {
        let words = line.splitn(4, ' ').collect::<Vec<_>>();
        let kind: u32 = words[1].parse().unwrap();
        assert_eq!(
            words[3],
            format!("{} exit=0", boundary_line(kind)),
            "{line}"
        );
        seen += 1;
    }
    // Six cases at two levels: 12 EvoCC and 24 reference executions.
    assert_eq!(seen, 36, "{stdout}");
}

/// Darwin: EvoCC and Apple clang print the reference line for the six
/// cases at -O0 and -O2.
#[cfg(target_os = "macos")]
#[test]
fn darwin_boundary_probe_agrees_with_apple_clang() {
    let work = Work::new("darwin-boundaries");
    let probe = fixture("floating-half-boundaries.c");
    for kind in KINDS {
        for case in 0..2 {
            for level in LEVELS {
                for compiler in ["evocc", "clang"] {
                    let args = [
                        format!("-DKIND={kind}"),
                        format!("-DCASE={case}"),
                        level.into(),
                    ];
                    let run = darwin_program(
                        &work,
                        &format!("{compiler}-{kind}-{case}{level}"),
                        compiler,
                        &args,
                        &probe,
                    );
                    assert_eq!(
                        run.status.code(),
                        Some(0),
                        "{compiler} {kind} {case} {level}"
                    );
                    assert_eq!(
                        String::from_utf8_lossy(&run.stdout),
                        format!("{}\n", boundary_line(kind)),
                        "{compiler} KIND={kind} CASE={case} {level}"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The packet's probes.

/// What one build of l03-half-arithmetic.c gives.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    /// Main returns this class.
    Class(i64),
    /// The build stops at this diagnostic.
    Refused(&'static str),
    /// The case has no form for the kind: `__fp16` has no constant suffix.
    Absent,
}

/// The outcome of l03-half-arithmetic.c on `target`: GCC 14's class, one
/// rounding of a binary32 evaluation. The `_Float16` and `__bf16` mixture of
/// case 7 is E2961. Cases 13 and 14 spell `__bf16` constants with the `bf16`
/// suffix, which Apple clang rejects, so on Darwin they are E0112.
fn arithmetic_outcome(target: &str, kind: u32, case: u32) -> Outcome {
    match case {
        0 | 2 | 6 | 12 => Outcome::Class(2),
        1 => Outcome::Class(if kind == 2 { 1 } else { 2 }),
        3 | 4 | 8..=11 => Outcome::Class(1),
        5 => Outcome::Class(if kind == 1 { 8 } else { 15 }),
        7 => Outcome::Refused("E2961"),
        13 | 14 if kind == 1 => Outcome::Absent,
        13 | 14 if kind == 2 && target == DARWIN => Outcome::Refused("E0112"),
        13 | 14 => Outcome::Class(2),
        _ => unreachable!("the probe has cases 0 to 14"),
    }
}

/// The 15 cases of the arithmetic probe give GCC 14's class in both
/// interpreters on both targets, or the named refusal of
/// `arithmetic_outcome`.
#[test]
fn arithmetic_probe_gives_the_gcc_14_classes_in_both_interpreters() {
    let probe = packet("l03-half-arithmetic.c");
    let mut jobs = Vec::new();
    for target in TARGETS {
        for kind in KINDS {
            for case in 0..15 {
                for interpreter in INTERPRETERS {
                    for level in LEVELS {
                        jobs.push((target, kind, case, interpreter, level));
                    }
                }
            }
        }
    }
    let failures = parallel(&jobs, |&(target, kind, case, interpreter, level)| {
        let outcome = arithmetic_outcome(target, kind, case);
        if outcome == Outcome::Absent {
            return None;
        }
        let args = [
            format!("-DKIND={kind}"),
            format!("-DCASE={case}"),
            level.into(),
            interpreter.into(),
        ];
        let output = evocc(target, &args, &probe);
        let observed = match outcome {
            Outcome::Class(class) => interpreted(&output) == Some(class),
            Outcome::Refused(code) => first_code(&output).as_deref() == Some(code),
            Outcome::Absent => unreachable!(),
        };
        (!observed).then(|| {
            format!(
                "{target} KIND={kind} CASE={case} {interpreter} {level}: {}",
                text(&output)
            )
        })
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The 22 cases of the conversion probe match the single IEEE rounding in
/// both interpreters. On Darwin, case 21 assumes a binary128 long double
/// that the target does not have, so main reports bit 5 alone.
#[test]
fn conversion_probe_rounds_once_in_both_interpreters() {
    let probe = packet("l03-half-conversions.c");
    let mut jobs = Vec::new();
    for target in TARGETS {
        for kind in KINDS {
            for interpreter in INTERPRETERS {
                for level in LEVELS {
                    jobs.push((target, kind, interpreter, level));
                }
            }
        }
    }
    let failures = parallel(&jobs, |&(target, kind, interpreter, level)| {
        let args = [format!("-DKIND={kind}"), level.into(), interpreter.into()];
        let output = evocc(target, &args, &probe);
        let expected = if target == DARWIN { 32 } else { 0 };
        (interpreted(&output) != Some(expected)).then(|| {
            format!(
                "{target} KIND={kind} {interpreter} {level}: {}",
                text(&output)
            )
        })
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Darwin native code gives the same classes and conversions.
#[cfg(target_os = "macos")]
#[test]
fn darwin_native_probes_give_the_gcc_14_results() {
    let work = Work::new("darwin-probes");
    for kind in KINDS {
        for level in LEVELS {
            let args = [format!("-DKIND={kind}"), level.into()];
            let run = darwin_program(
                &work,
                &format!("conversions-{kind}{level}"),
                "evocc",
                &args,
                &packet("l03-half-conversions.c"),
            );
            assert_eq!(run.status.code(), Some(32), "KIND={kind} {level}");
            for case in 0..15 {
                let args = [
                    format!("-DKIND={kind}"),
                    format!("-DCASE={case}"),
                    level.into(),
                ];
                let class = match arithmetic_outcome(DARWIN, kind, case) {
                    Outcome::Class(class) => class,
                    Outcome::Refused(code) => {
                        let mut args = args.to_vec();
                        args.extend(["-c".into(), "-o".into(), "/dev/null".into()]);
                        let output = evocc(DARWIN, &args, &packet("l03-half-arithmetic.c"));
                        assert_eq!(
                            first_code(&output).as_deref(),
                            Some(code),
                            "{}",
                            text(&output)
                        );
                        continue;
                    }
                    Outcome::Absent => continue,
                };
                let run = darwin_program(
                    &work,
                    &format!("arithmetic-{kind}-{case}{level}"),
                    "evocc",
                    &args,
                    &packet("l03-half-arithmetic.c"),
                );
                assert_eq!(
                    run.status.code(),
                    Some(class as i32),
                    "KIND={kind} CASE={case} {level}"
                );
            }
        }
    }
}

/// Linux native code gives GCC 14's classes and the single rounding of
/// every conversion, and GCC 14 still gives them in the sealed image.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_probes_give_the_gcc_14_results() {
    let work = Work::new("linux-probes");
    work.copy(&packet("l03-half-arithmetic.c"));
    work.copy(&packet("l03-half-conversions.c"));
    let mut script = String::from("cd /work\n");
    let mut expected = Vec::new();
    for kind in KINDS {
        for level in LEVELS {
            let mut builds = vec![("conversions".to_owned(), Vec::new(), 0)];
            for case in 0..15 {
                if let Outcome::Class(class) = arithmetic_outcome(LINUX, kind, case) {
                    builds.push((
                        "arithmetic".to_owned(),
                        vec![format!("-DCASE={case}")],
                        class,
                    ));
                }
            }
            for (probe, defines, status) in builds {
                let name = format!("{probe}-{kind}{}{level}", defines.join(""));
                let mut args = defines.clone();
                args.extend([
                    format!("-DKIND={kind}"),
                    level.into(),
                    "-S".into(),
                    "-o".into(),
                    work.path(&format!("{name}.s")).to_str().unwrap().into(),
                ]);
                let output = evocc(LINUX, &args, &packet(&format!("l03-half-{probe}.c")));
                assert!(output.status.success(), "{name}\n{}", text(&output));
                script.push_str(&format!(
                    "gcc-14 {name}.s -o t -lm; ./t; echo \"evocc {name} $?\"\n\
                     gcc-14 -std=gnu11 -w {level} -DKIND={kind} {} l03-half-{probe}.c -o r -lm; ./r; echo \"gcc-14 {name} $?\"\n",
                    defines.join(" ")
                ));
                expected.push(format!("evocc {name} {status}"));
                expected.push(format!("gcc-14 {name} {status}"));
            }
        }
    }
    let output = sealed(&work, &script);
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), expected);
}

/// One anonymous `_Float16` or `__bf16` argument stays unpromoted: EvoCC in
/// both roles agrees with GCC 14 and Clang 19 at -O0 and -O2.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn anonymous_halves_agree_with_gcc_14_and_clang_19() {
    let work = Work::new("varargs");
    let probe = work.copy(&packet("l03-half-varargs.c"));
    let mut script = String::from("set -e\ncd /work\n");
    let mut lines = 0;
    for kind in [0, 2] {
        for level in LEVELS {
            for role in 0..2 {
                let name = format!("evocc-{kind}-{role}{level}");
                let args = [
                    format!("-DKIND={kind}"),
                    format!("-DROLE={role}"),
                    level.into(),
                    "-S".into(),
                    "-o".into(),
                    work.path(&format!("{name}.s")).to_str().unwrap().into(),
                ];
                let output = evocc(LINUX, &args, &probe);
                assert!(output.status.success(), "{name}\n{}", text(&output));
                script.push_str(&format!("gcc-14 -c {name}.s -o {name}.o\n"));
                for compiler in ["gcc-14", "clang-19"] {
                    script.push_str(&format!(
                        "{compiler} -std=gnu11 -w {level} -DKIND={kind} -DROLE={role} -c l03-half-varargs.c -o {compiler}-{kind}-{role}{level}.o\n"
                    ));
                }
            }
            for (callee, caller) in [
                ("evocc", "evocc"),
                ("evocc", "gcc-14"),
                ("gcc-14", "evocc"),
                ("evocc", "clang-19"),
                ("clang-19", "evocc"),
            ] {
                script.push_str(&format!(
                    "gcc-14 {caller}-{kind}-1{level}.o {callee}-{kind}-0{level}.o -o t\n\
                     echo \"{kind} {level} callee={callee} caller={caller} $(./t)\"\n"
                ));
                lines += 1;
            }
        }
    }
    let output = sealed(&work, &script);
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    for line in stdout.lines() {
        let expected = if line.starts_with("0 ") {
            "3e00 3e00"
        } else {
            "3fc0 3fc0"
        };
        assert!(line.ends_with(expected), "{line}");
    }
    assert_eq!(stdout.lines().count(), lines, "{stdout}");
}

/// Conversions into `_Float16` and `__bf16` follow the rounding direction
/// and raise GCC 14's flags in native code on both targets.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn rounding_directions_and_flags_agree_with_gcc_14() {
    let work = Work::new("fenv");
    let probe = work.copy(&packet("l03-half-fenv.c"));
    let mut script = String::from("set -e\ncd /work\n");
    for kind in [0, 2] {
        for level in LEVELS {
            let name = format!("fenv-{kind}{level}.s");
            let args = [
                format!("-DKIND={kind}"),
                "-frounding-math".into(),
                "-ffp-contract=off".into(),
                level.into(),
                "-S".into(),
                "-o".into(),
                work.path(&name).to_str().unwrap().into(),
            ];
            let output = evocc(LINUX, &args, &probe);
            assert!(output.status.success(), "{name}\n{}", text(&output));
            script.push_str(&format!(
                "gcc-14 {name} -o t -lm\n./t | sed 's/^/KIND={kind} /'\n\
                 gcc-14 -std=gnu11 -w {level} -frounding-math -ffp-contract=off -DKIND={kind} l03-half-fenv.c -o r -lm\n./r | sed 's/^/KIND={kind} /'\n"
            ));
        }
    }
    let output = sealed(&work, &script);
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let recorded = records("floating-half-fenv.out");
    let mut expected = Vec::new();
    for kind in [0, 2] {
        let lines = recorded
            .iter()
            .filter(|line| line.starts_with(&format!("KIND={kind} ")))
            .cloned()
            .collect::<Vec<_>>();
        for _ in 0..4 {
            expected.extend(lines.iter().cloned());
        }
    }
    assert_eq!(
        stdout.lines().map(str::to_owned).collect::<Vec<_>>(),
        expected
    );
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_rounding_directions_and_flags_agree_with_gcc_14() {
    let work = Work::new("darwin-fenv");
    let recorded = records("floating-half-fenv.out");
    for kind in [0, 2] {
        let expected = recorded
            .iter()
            .filter_map(|line| line.strip_prefix(&format!("KIND={kind} ")))
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        for level in LEVELS {
            let args = [
                format!("-DKIND={kind}"),
                "-frounding-math".into(),
                "-ffp-contract=off".into(),
                level.into(),
            ];
            let compilers: &[&str] = if kind == 0 {
                &["evocc", "clang"]
            } else {
                &["evocc"]
            };
            for compiler in compilers {
                let run = darwin_program(
                    &work,
                    &format!("{compiler}-fenv-{kind}{level}"),
                    compiler,
                    &args,
                    &packet("l03-half-fenv.c"),
                );
                assert!(run.status.success(), "{compiler} {kind} {level}");
                assert_eq!(
                    String::from_utf8_lossy(&run.stdout),
                    expected,
                    "{compiler} KIND={kind} {level}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Calls.

/// The crossed ABI probe as one unit runs in both interpreters on both
/// targets: named arguments and results, HFAs, records, promoted anonymous
/// arguments and, on Linux, stack arguments and unpromoted anonymous halves.
#[test]
fn half_calls_agree_in_both_interpreters() {
    let unit = fixture("floating-half-abi-interpreted.c");
    let mut jobs = Vec::new();
    for target in TARGETS {
        for kind in KINDS {
            for interpreter in INTERPRETERS {
                for level in LEVELS {
                    jobs.push((target, kind, interpreter, level));
                }
            }
        }
    }
    let failures = parallel(&jobs, |&(target, kind, interpreter, level)| {
        let mut args = vec![format!("-DKIND={kind}"), level.into(), interpreter.into()];
        if target == LINUX {
            args.push("-DLINUX".into());
        }
        let output = evocc(target, &args, &unit);
        (interpreted(&output) != Some(0)).then(|| {
            format!(
                "{target} KIND={kind} {interpreter} {level}: {}",
                text(&output)
            )
        })
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Crossed calls between EvoCC, GCC 14 and Clang 19 in both directions, at
/// -O0 and -O2: the packet's probe, with -DNO_HFA for `__bf16`, and the
/// half-format probe of this item. A caller that leaves garbage above each
/// half argument does not change what an EvoCC callee reads. A record of
/// both half formats passes in general registers, as GCC 14 passes it; Clang
/// 19 passes it in vector registers, so a crossed Clang call fails that
/// shape first (status 10, shape 9), a recorded incompatibility.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn crossed_calls_agree_with_gcc_14_and_clang_19() {
    let work = Work::new("crossed-linux");
    for name in [
        "floating-half-abi.h",
        "floating-half-abi-worker.c",
        "floating-half-abi-driver.c",
        "floating-half-abi-garbage.c",
    ] {
        work.copy(&fixture(name));
    }
    for name in ["l03-half-abi-driver.c", "l03-half-abi-worker.c"] {
        work.copy(&packet(name));
    }
    let mut script = String::from("set -e\ncd /work\n");
    let mut expected = Vec::new();
    for kind in KINDS {
        for level in LEVELS {
            for (probe, worker, driver, options) in [
                (
                    "packet",
                    "l03-half-abi-worker.c",
                    "l03-half-abi-driver.c",
                    if kind == 2 { "-DNO_HFA" } else { "" },
                ),
                (
                    "half",
                    "floating-half-abi-worker.c",
                    "floating-half-abi-driver.c",
                    "-DLINUX",
                ),
            ] {
                let stem = format!("{probe}-{kind}{level}");
                for (side, source) in [("w", worker), ("d", driver)] {
                    let mut args = vec![
                        format!("-DKIND={kind}"),
                        level.into(),
                        "-S".into(),
                        "-o".into(),
                        work.path(&format!("{stem}-{side}.s"))
                            .to_str()
                            .unwrap()
                            .into(),
                    ];
                    if !options.is_empty() {
                        args.push(options.into());
                    }
                    let output = evocc(LINUX, &args, &work.path(source));
                    assert!(output.status.success(), "{stem} {side}\n{}", text(&output));
                    script.push_str(&format!(
                        "gcc-14 -c {stem}-{side}.s -o {stem}-{side}-evocc.o\n"
                    ));
                    for compiler in ["gcc-14", "clang-19"] {
                        script.push_str(&format!(
                            "{compiler} -std=gnu11 -w {level} -DKIND={kind} {options} -c {source} -o {stem}-{side}-{compiler}.o\n"
                        ));
                    }
                }
                for (w, d) in [
                    ("evocc", "evocc"),
                    ("evocc", "gcc-14"),
                    ("gcc-14", "evocc"),
                    ("evocc", "clang-19"),
                    ("clang-19", "evocc"),
                ] {
                    script.push_str(&format!(
                        "gcc-14 {stem}-d-{d}.o {stem}-w-{w}.o -o t\n\
                         if ./t; then echo \"{stem} worker={w} driver={d} 0\"; else echo \"{stem} worker={w} driver={d} $?\"; fi\n"
                    ));
                    let clang = w == "clang-19" || d == "clang-19";
                    let status = if probe == "half" && clang { 10 } else { 0 };
                    expected.push(format!("{stem} worker={w} driver={d} {status}"));
                }
                if probe == "half" {
                    for compiler in ["gcc-14", "clang-19"] {
                        script.push_str(&format!(
                            "{compiler} -std=gnu11 -w {level} -DKIND={kind} -DLINUX -c floating-half-abi-garbage.c -o garbage.o\n\
                             gcc-14 garbage.o {stem}-w-evocc.o -o t\n\
                             if ./t; then echo \"{stem} worker=evocc garbage={compiler} 0\"; else echo \"{stem} worker=evocc garbage={compiler} $?\"; fi\n"
                        ));
                        expected.push(format!("{stem} worker=evocc garbage={compiler} 0"));
                    }
                }
            }
        }
    }
    let output = sealed(&work, &script);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(stdout.lines().collect::<Vec<_>>(), expected);
}

/// Darwin: EvoCC and Apple clang in both directions for the named shapes
/// at -O0 and -O2, and an Apple clang caller that leaves garbage above each
/// half argument.
#[cfg(target_os = "macos")]
#[test]
fn crossed_calls_agree_with_apple_clang() {
    let work = Work::new("crossed-darwin");
    for name in [
        "floating-half-abi.h",
        "floating-half-abi-worker.c",
        "floating-half-abi-driver.c",
        "floating-half-abi-garbage.c",
    ] {
        work.copy(&fixture(name));
    }
    for kind in KINDS {
        for level in LEVELS {
            let object = |side: &str, compiler: &str| {
                work.path(&format!("{side}-{kind}{level}-{compiler}.o"))
            };
            for side in ["worker", "driver", "garbage"] {
                let source = work.path(&format!("floating-half-abi-{side}.c"));
                if side != "garbage" {
                    let args = [
                        format!("-DKIND={kind}"),
                        level.into(),
                        "-c".into(),
                        "-o".into(),
                        object(side, "evocc").to_str().unwrap().into(),
                    ];
                    let output = evocc(DARWIN, &args, &source);
                    assert!(
                        output.status.success(),
                        "{side} {kind} {level}\n{}",
                        text(&output)
                    );
                }
                let clang = Command::new("clang")
                    .args(["-std=gnu11", "-w", level, "-c", &format!("-DKIND={kind}")])
                    .arg(&source)
                    .arg("-o")
                    .arg(object(side, "clang"))
                    .output()
                    .unwrap();
                assert!(clang.status.success(), "{}", text(&clang));
            }
            for (worker, driver) in [("evocc", "evocc"), ("evocc", "clang"), ("clang", "evocc")] {
                for (caller, label) in [("driver", driver), ("garbage", "clang")] {
                    if caller == "garbage" && worker != "evocc" {
                        continue;
                    }
                    let program = work.path(&format!("{kind}{level}-{worker}-{caller}-{label}"));
                    let link = Command::new("clang")
                        .arg(object(caller, label))
                        .arg(object("worker", worker))
                        .arg("-o")
                        .arg(&program)
                        .output()
                        .unwrap();
                    assert!(link.status.success(), "{}", text(&link));
                    let run = Command::new(&program).output().unwrap();
                    assert_eq!(
                        run.status.code(),
                        Some(0),
                        "KIND={kind} {level} worker={worker} {caller}={label}"
                    );
                }
            }
        }
    }
}

/// An HFA of `__bf16` members is refused at every call boundary, in both
/// interpreters and in native code, on both targets: AAPCS64 passes it in
/// vector registers, and GCC 14 in general registers (PR 120007).
#[test]
fn bfloat16_hfa_interfaces_are_refused() {
    let work = Work::new("bf16-hfa");
    let sources = [
        "struct B { __bf16 a, b, c; };\n\
         __attribute__((noinline)) struct B f(struct B v) { return v; }\n\
         int main(void) { struct B b = {0}; return (int)f(b).a; }\n",
        "struct B { __bf16 a[2]; };\n\
         extern struct B g(void);\n\
         int main(void) { return (int)g().a[0]; }\n",
    ];
    for (index, source) in sources.iter().enumerate() {
        let path = work.path(&format!("bf16-hfa-{index}.c"));
        std::fs::write(&path, source).unwrap();
        for target in TARGETS {
            for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
                if index == 1 && mode != "--emit=asm" {
                    continue;
                }
                let output = evocc(target, &[mode], &path);
                assert_eq!(
                    first_code(&output).as_deref(),
                    Some("E3049"),
                    "{target} {mode} {source}\n{}",
                    text(&output)
                );
            }
        }
    }
    let output = evocc(
        LINUX,
        &["-DKIND=2", "-S", "-o", "/dev/null"],
        &packet("l03-half-abi-worker.c"),
    );
    assert_eq!(
        first_code(&output).as_deref(),
        Some("E3049"),
        "{}",
        text(&output)
    );
}

/// A record that mixes `__bf16` and `_Float16` members is no HFA under
/// AAPCS64, which gives the two formats two fundamental types. On Linux it
/// passes as an ordinary composite in general registers, as GCC 14 passes it
/// (the crossed calls test it). Apple clang passes it in vector registers, so
/// on Darwin it is refused at every call boundary, in both interpreters and
/// in native code, and as an anonymous argument.
#[test]
fn mixed_half_records_pass_as_composites_on_linux_only() {
    let work = Work::new("mixed-records");
    let sources = [
        "struct M { __bf16 a; _Float16 b; };\n\
         __attribute__((noinline)) int f(struct M v) { return (int)v.b; }\n\
         int main(void) { struct M m = {0}; return f(m); }\n",
        "struct M { _Float16 a; __bf16 b[2]; };\n\
         __attribute__((noinline)) struct M f(void) { struct M m = {0}; return m; }\n\
         int main(void) { return (int)f().a; }\n",
        "struct M { __bf16 a; _Float16 b; };\n\
         static int f(int n, ...) { return n; }\n\
         int main(void) { struct M m = {0}; return f(0, m); }\n",
        "#include <stdarg.h>\n\
         struct M { __bf16 a; _Float16 b; };\n\
         static int f(int n, ...) { va_list ap; va_start(ap, n); struct M m = va_arg(ap, struct M); va_end(ap); return (int)m.b; }\n\
         int main(void) { return f(1, (struct M){0}); }\n",
    ];
    for (index, source) in sources.iter().enumerate() {
        let path = work.path(&format!("mixed-{index}.c"));
        std::fs::write(&path, source).unwrap();
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let output = evocc(DARWIN, &[mode], &path);
            assert_eq!(
                first_code(&output).as_deref(),
                Some("E3049"),
                "{mode} {source}\n{}",
                text(&output)
            );
            let output = evocc(LINUX, &[mode], &path);
            assert!(
                output.status.success(),
                "{mode} {source}\n{}",
                text(&output)
            );
            if mode != "--emit=asm" {
                assert_eq!(interpreted(&output), Some(0), "{mode} {source}");
            }
        }
    }
}

/// Apple clang passes a record of both half formats in vector registers,
/// which is why Darwin refuses it: the callee reads its second member from
/// `s1`, not from the general register of a composite.
#[cfg(target_os = "macos")]
#[test]
fn apple_clang_passes_a_mixed_half_record_in_vector_registers() {
    let work = Work::new("apple-mixed");
    let source = work.path("second.c");
    std::fs::write(
        &source,
        "struct M { _Float16 a; __bf16 b; };\n\
         unsigned second(struct M m) { unsigned short u; __builtin_memcpy(&u, &m.b, 2); return u; }\n",
    )
    .unwrap();
    let output = Command::new("clang")
        .args(["-O2", "-S", "-o", "-"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let assembly = String::from_utf8_lossy(&output.stdout);
    assert!(assembly.contains("fmov\tw8, s1"), "{assembly}");
    assert!(!assembly.contains("lsr\tw0, w0, #16"), "{assembly}");
}

/// The `bf16` and `BF16` constant suffixes and the `__BFLT16_*` macros are
/// Linux only, as GCC 14 has them and Apple clang has neither: on Darwin
/// such a constant is a stray token (E0112), as before L03. The `f16` and
/// `F16` suffixes and the `__FLT16_*` macros exist on both targets.
#[test]
fn bfloat16_constants_and_macros_are_linux_only() {
    let work = Work::new("bf16-constants");
    for suffix in ["bf16", "BF16"] {
        let path = work.path(&format!("constant-{suffix}.c"));
        std::fs::write(
            &path,
            format!("float f = 1.5{suffix};\nint main(void) {{ return f != 1.5f; }}\n"),
        )
        .unwrap();
        for mode in ["--check", "--emit=evoir", "--emit=asm"] {
            let output = evocc(DARWIN, &[mode], &path);
            assert_eq!(
                first_code(&output).as_deref(),
                Some("E0112"),
                "{suffix} {mode}\n{}",
                text(&output)
            );
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("stray-token"),
                "{}",
                text(&output)
            );
            let output = evocc(LINUX, &[mode], &path);
            assert!(
                output.status.success(),
                "{suffix} {mode}\n{}",
                text(&output)
            );
        }
    }
    for suffix in ["f16", "F16"] {
        let path = work.path(&format!("constant-{suffix}.c"));
        std::fs::write(
            &path,
            format!("float f = 1.5{suffix};\nint main(void) {{ return f != 1.5f; }}\n"),
        )
        .unwrap();
        for target in TARGETS {
            for mode in INTERPRETERS {
                let output = evocc(target, &[mode], &path);
                assert_eq!(
                    interpreted(&output),
                    Some(0),
                    "{target} {suffix} {mode}\n{}",
                    text(&output)
                );
            }
        }
    }
    let path = work.path("macros.c");
    std::fs::write(
        &path,
        "int main(void) {\n\
         #ifdef __BFLT16_MAX__\n\
             int bfloat = 1;\n\
         #else\n\
             int bfloat = 0;\n\
         #endif\n\
         #ifdef __FLT16_MAX__\n\
             int binary = 1;\n\
         #else\n\
             int binary = 0;\n\
         #endif\n\
             return bfloat << 1 | binary;\n\
         }\n",
    )
    .unwrap();
    for (target, expected) in [(LINUX, 3), (DARWIN, 1)] {
        let output = evocc(target, &["--interpret-c-hir"], &path);
        assert_eq!(
            interpreted(&output),
            Some(expected),
            "{target}\n{}",
            text(&output)
        );
    }
}

/// Darwin has no stable convention for an anonymous `_Float16` or `__bf16`
/// argument, so the argument and `va_arg` of either type are refused in both
/// interpreters and in native code. An anonymous `__fp16` becomes `double`.
#[test]
fn darwin_anonymous_halves_are_refused() {
    for kind in [0, 2] {
        for role in 0..2 {
            // The callee alone has no main, and the caller's callee is
            // external, which the C-HIR interpreter refuses first (E3039).
            let modes: &[&str] = if role == 0 {
                &["--emit=asm"]
            } else {
                &["--interpret-evoir", "--emit=asm"]
            };
            for mode in modes {
                let args = [
                    format!("-DKIND={kind}"),
                    format!("-DROLE={role}"),
                    (*mode).into(),
                ];
                let output = evocc(DARWIN, &args, &packet("l03-half-varargs.c"));
                assert_eq!(
                    first_code(&output).as_deref(),
                    Some("E3049"),
                    "KIND={kind} ROLE={role} {mode}\n{}",
                    text(&output)
                );
            }
        }
    }
    let work = Work::new("darwin-anonymous");
    let path = work.path("callee.c");
    std::fs::write(
        &path,
        "#include <stdarg.h>\n\
         static int f(int n, ...) { va_list ap; va_start(ap, n); _Float16 h = va_arg(ap, _Float16); va_end(ap); return (int)h; }\n\
         int main(void) { return f(1, 0); }\n",
    )
    .unwrap();
    for mode in INTERPRETERS {
        let output = evocc(DARWIN, &[mode], &path);
        assert_eq!(
            first_code(&output).as_deref(),
            Some("E3049"),
            "{}",
            text(&output)
        );
    }
    let path = work.path("caller.c");
    std::fs::write(
        &path,
        "static int f(int n, ...) { return n; }\n\
         int main(void) { __bf16 h = 1; _Float16 g = 1; return f(1, h) + f(1, g); }\n",
    )
    .unwrap();
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = evocc(DARWIN, &[mode], &path);
        assert_eq!(
            first_code(&output).as_deref(),
            Some("E3049"),
            "{mode}\n{}",
            text(&output)
        );
        let output = evocc(LINUX, &[mode], &path);
        assert!(output.status.success(), "{mode}\n{}", text(&output));
    }
    let path = work.path("promoted.c");
    std::fs::write(
        &path,
        "#include <stdarg.h>\n\
         static double f(int n, ...) { va_list ap; va_start(ap, n); double d = va_arg(ap, double); va_end(ap); return d; }\n\
         int main(void) { __fp16 h = 1.5f; return f(1, h) != 1.5; }\n",
    )
    .unwrap();
    for mode in INTERPRETERS {
        let output = evocc(DARWIN, &[mode], &path);
        assert_eq!(interpreted(&output), Some(0), "{}", text(&output));
    }
    let output = evocc(DARWIN, &["--emit=asm"], &path);
    assert!(output.status.success(), "{}", text(&output));
}

/// A binary16 HFA has no stack placement in this revision, and a half
/// argument has none under the Apple convention: native code refuses both
/// (E4010).
#[test]
fn half_arguments_without_a_stack_placement_are_refused() {
    let work = Work::new("stack");
    let hfa = work.path("hfa.c");
    std::fs::write(
        &hfa,
        "struct H2 { _Float16 a, b; };\n\
         int f(double, double, double, double, double, double, double, struct H2);\n\
         int g(struct H2 h) { return f(0, 0, 0, 0, 0, 0, 0, h); }\n",
    )
    .unwrap();
    let scalars = work.path("scalars.c");
    std::fs::write(
        &scalars,
        "int f(_Float16, _Float16, _Float16, _Float16, _Float16, _Float16, _Float16, _Float16, _Float16);\n\
         int g(_Float16 h) { return f(h, h, h, h, h, h, h, h, h); }\n",
    )
    .unwrap();
    for (target, source) in [(LINUX, &hfa), (DARWIN, &hfa), (DARWIN, &scalars)] {
        let output = evocc(target, &["--emit=asm"], source);
        assert_eq!(
            first_code(&output).as_deref(),
            Some("E4010"),
            "{target}\n{}",
            text(&output)
        );
    }
    let output = evocc(LINUX, &["--emit=asm"], &scalars);
    assert!(output.status.success(), "{}", text(&output));
}

// ---------------------------------------------------------------------------
// The special-value grid.

/// One recorded line of floating-half-grid.out.
struct GridLine {
    selection: String,
    kind: u32,
    mode: u32,
    name: String,
    hash: String,
}

fn grid_lines() -> Vec<GridLine> {
    records("floating-half-grid.out")
        .iter()
        .map(|line| {
            let words = line.split(' ').collect::<Vec<_>>();
            let [selection, kind, mode, name, hash] = words[..] else {
                panic!("{line}");
            };
            GridLine {
                selection: selection.to_owned(),
                kind: kind.strip_prefix("KIND=").unwrap().parse().unwrap(),
                mode: mode.strip_prefix("MODE=").unwrap().parse().unwrap(),
                name: name.to_owned(),
                hash: hash.to_owned(),
            }
        })
        .collect()
}

/// The printed lines of a grid build: KIND=k MODE=m NAME HASH.
fn grid_output(selection: &str, kind: u32, mode: u32) -> Vec<String> {
    grid_lines()
        .iter()
        .filter(|line| line.selection == selection && line.kind == kind && line.mode == mode)
        .map(|line| format!("KIND={kind} MODE={mode} {} {}", line.name, line.hash))
        .collect()
}

/// The five floating environments of the grid: round to nearest without
/// flags, and each rounding direction with flags.
fn grid_variants() -> [(&'static str, u32, Vec<String>); 5] {
    let strict = |mode: u32| {
        vec![
            "-frounding-math".to_owned(),
            "-DFLAGS".to_owned(),
            format!("-DMODE={mode}"),
        ]
    };
    [
        ("plain", 0, Vec::new()),
        ("strict", 0, strict(0)),
        ("strict", 1, strict(1)),
        ("strict", 2, strict(2)),
        ("strict", 3, strict(3)),
    ]
}

fn expect<S: AsRef<str>>(hashes: &[S]) -> String {
    let list = hashes
        .iter()
        .map(|hash| format!("0x{}ull", hash.as_ref()))
        .collect::<Vec<_>>()
        .join(",");
    format!("-DEXPECT={list}")
}

/// One interpreter takes the whole grid of one kind and environment, bit
/// for bit and flag for flag: each operation of the two row slices, which
/// together take every pair, and every conversion at once. The step budget
/// of an interpreter admits one operation of a slice per run.
fn grid_in_interpreter(interpreter: &'static str) {
    let grid = fixture("floating-half-grid.c");
    let lines = grid_lines();
    let mut jobs = Vec::new();
    for kind in KINDS {
        for (selection, mode, options) in grid_variants() {
            for row in 0..2 {
                let slice = format!("{selection}-row{row}");
                let hashes = lines
                    .iter()
                    .filter(|line| {
                        line.selection == slice && line.kind == kind && line.mode == mode
                    })
                    .map(|line| line.hash.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(hashes.len(), 13, "{slice} {kind} {mode}");
                for (index, hash) in hashes.iter().enumerate() {
                    let (part, op) = if index < 7 {
                        (1, index)
                    } else {
                        (2, index - 7)
                    };
                    let mut args = options.clone();
                    args.extend([
                        format!("-DKIND={kind}"),
                        format!("-DPART={part}"),
                        format!("-DOP={op}"),
                        format!("-DROW_FIRST={row}"),
                        "-DROW_STEP=2".into(),
                        "-DCHECK".into(),
                        expect(&[hash]),
                    ]);
                    jobs.push((
                        format!("{slice} KIND={kind} MODE={mode} line {index}"),
                        args,
                    ));
                }
            }
            let hashes = lines
                .iter()
                .filter(|line| {
                    line.selection == selection && line.kind == kind && line.mode == mode
                })
                .map(|line| line.hash.as_str())
                .collect::<Vec<_>>();
            assert_eq!(hashes.len(), 30, "{selection} {kind} {mode}");
            let mut args = options.clone();
            args.extend([
                format!("-DKIND={kind}"),
                "-DOTHER".into(),
                "-DQUAD".into(),
                "-DWIDE".into(),
                "-DPART=3".into(),
                "-DCHECK".into(),
                expect(&hashes[13..]),
            ]);
            jobs.push((
                format!("{selection} KIND={kind} MODE={mode} conversions"),
                args,
            ));
        }
    }
    let failures = parallel(&jobs, |(label, args)| {
        let mut args = args.clone();
        args.extend(["-ffp-contract=off".into(), interpreter.into()]);
        let output = evocc(LINUX, &args, &grid);
        (interpreted(&output) != Some(0)).then(|| format!("{label}: {}", text(&output)))
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn special_value_grid_matches_gcc_14_in_the_c_hir_interpreter() {
    grid_in_interpreter("--interpret-c-hir");
}

#[test]
fn special_value_grid_matches_gcc_14_in_the_evoir_interpreter() {
    grid_in_interpreter("--interpret-evoir");
}

/// Linux native code prints the recorded lines of the whole grid at -O0 and
/// -O2 in the five environments, binary128 and 128-bit integers included,
/// and GCC 14 still prints them.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn special_value_grid_matches_gcc_14_natively() {
    let work = Work::new("grid");
    let grid = work.copy(&fixture("floating-half-grid.c"));
    let mut script = String::from("set -e\ncd /work\n");
    let mut expected = Vec::new();
    for kind in KINDS {
        for (selection, mode, options) in grid_variants() {
            for level in LEVELS {
                let name = format!("grid-{kind}-{selection}{mode}{level}.s");
                let mut args = options.clone();
                args.extend([
                    format!("-DKIND={kind}"),
                    "-DOTHER".into(),
                    "-DQUAD".into(),
                    "-DWIDE".into(),
                    "-ffp-contract=off".into(),
                    level.into(),
                    "-S".into(),
                    "-o".into(),
                    work.path(&name).to_str().unwrap().into(),
                ]);
                let output = evocc(LINUX, &args, &grid);
                assert!(output.status.success(), "{name}\n{}", text(&output));
                script.push_str(&format!(
                    "gcc-14 {name} -o t -lm\n./t\n\
                     gcc-14 -std=gnu11 -w {level} -ffp-contract=off {} -DKIND={kind} -DOTHER -DQUAD -DWIDE floating-half-grid.c -o r -lm\n./r\n",
                    options.join(" ")
                ));
                let lines = grid_output(selection, kind, mode);
                expected.extend(lines.iter().cloned());
                expected.extend(lines);
            }
        }
    }
    let output = sealed(&work, &script);
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let observed = stdout.lines().map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(observed.len(), expected.len(), "{stdout}");
    for (observed, expected) in observed.iter().zip(&expected) {
        assert_eq!(observed, expected);
    }
}

/// The lines where Apple clang may differ from IEEE binary32 evaluation, as
/// the grid's fixture states: its native binary16 arithmetic prefers a
/// signaling NaN of a pair and keeps a signaling NaN in negation, and its
/// bfloat16 conversions do not quiet a signaling NaN and give 0x8000 for a
/// binary64 NaN.
fn apple_lines(kind: u32) -> &'static [&'static str] {
    if kind == 2 {
        &["nan_pairs", "neg_snan", "widen32_snan", "narrow64_nan"]
    } else {
        &["nan_pairs", "neg_snan"]
    }
}

/// Darwin native code prints GCC 14's line for every operation and
/// conversion of the grid without binary128 (the target's long double is
/// binary64), at -O0 and -O2 in the five environments. Apple clang prints
/// the same lines except the ones that `apple_lines` names; it rejects
/// strict `__bf16` conversions and the conversions between the half formats,
/// so those builds are EvoCC's alone.
#[cfg(target_os = "macos")]
#[test]
fn darwin_grid_matches_gcc_14_and_apple_clang() {
    let work = Work::new("darwin-grid");
    let grid = fixture("floating-half-grid.c");
    for kind in KINDS {
        for (selection, mode, options) in grid_variants() {
            let expected = grid_output(selection, kind, mode)
                .into_iter()
                .filter(|line| !line.contains(" narrow128 ") && !line.contains(" widen128 "))
                .map(|line| format!("{line}\n"))
                .collect::<String>();
            for level in LEVELS {
                let mut args = options.clone();
                args.extend([
                    format!("-DKIND={kind}"),
                    "-ffp-contract=off".into(),
                    level.into(),
                ]);
                let mut evocc_args = args.clone();
                evocc_args.extend(["-DOTHER".into(), "-DWIDE".into()]);
                let run = darwin_program(
                    &work,
                    &format!("evocc-{kind}-{selection}{mode}{level}"),
                    "evocc",
                    &evocc_args,
                    &grid,
                );
                assert!(run.status.success(), "{kind} {selection} {mode} {level}");
                let observed = String::from_utf8_lossy(&run.stdout).into_owned();
                assert_eq!(
                    observed, expected,
                    "KIND={kind} {selection} MODE={mode} {level}"
                );
                if kind == 2 && selection == "strict" {
                    continue;
                }
                let clang = darwin_program(
                    &work,
                    &format!("clang-{kind}-{selection}{mode}{level}"),
                    "clang",
                    &args,
                    &grid,
                );
                assert!(clang.status.success(), "{kind} {selection} {mode} {level}");
                let differs = |line: &&str| {
                    apple_lines(kind)
                        .iter()
                        .any(|name| line.contains(&format!(" {name} ")))
                };
                let compared = |lines: &str| {
                    lines
                        .lines()
                        .filter(|line| !differs(line))
                        .filter(|line| {
                            !["to_other", "from_other", "from_int128", "from_uint128"]
                                .iter()
                                .any(|name| line.contains(&format!(" {name} ")))
                        })
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    compared(&String::from_utf8_lossy(&clang.stdout)),
                    compared(&observed),
                    "Apple clang KIND={kind} {selection} MODE={mode} {level}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Translation-time evaluation.

/// The fold records: the reference lines (run time), and GCC 14's own
/// translation-time lines, which the record explains.
fn fold_output(selection: &str, kind: u32) -> Vec<String> {
    records("floating-half-fold.out")
        .iter()
        .filter_map(|line| line.strip_prefix(&format!("{selection} ")))
        .filter(|line| line.starts_with(&format!("KIND={kind} ")))
        .map(str::to_owned)
        .collect()
}

/// Both interpreters fold every operation and conversion of the special
/// values to the run-time bits (FOLD=1), and compute them at run time
/// (FOLD=0), one operation per run.
#[test]
fn translation_time_values_match_the_run_time_in_both_interpreters() {
    let probe = fixture("floating-half-fold.c");
    let mut jobs = Vec::new();
    for kind in KINDS {
        let lines = fold_output("fold", kind);
        assert_eq!(lines.len(), 21);
        for (op, line) in lines.iter().enumerate() {
            let hash = line.rsplit(' ').next().unwrap().to_owned();
            for fold in 0..2 {
                for interpreter in INTERPRETERS {
                    jobs.push((kind, op, hash.clone(), fold, interpreter));
                }
            }
        }
    }
    let failures = parallel(&jobs, |(kind, op, hash, fold, interpreter)| {
        let args = [
            format!("-DKIND={kind}"),
            format!("-DFOLD={fold}"),
            format!("-DOP={op}"),
            "-DCHECK".into(),
            expect(&[hash]),
            "-ffp-contract=off".into(),
            (*interpreter).into(),
        ];
        let output = evocc(LINUX, &args, &probe);
        (interpreted(&output) != Some(0)).then(|| {
            format!(
                "KIND={kind} FOLD={fold} OP={op} {interpreter}: {}",
                text(&output)
            )
        })
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Static half objects, half constants, the binary32 value of conditional,
/// comma and statement expressions, and the predefined half macros, as
/// GCC 14 gives them, in both interpreters on both targets.
#[test]
fn translation_time_constants_match_gcc_14_in_both_interpreters() {
    let probe = fixture("floating-half-constants.c");
    let mut jobs = Vec::new();
    for target in TARGETS {
        for interpreter in INTERPRETERS {
            for level in LEVELS {
                jobs.push((target, interpreter, level));
            }
        }
    }
    let failures = parallel(&jobs, |&(target, interpreter, level)| {
        let output = evocc(target, &[level, interpreter], &probe);
        (interpreted(&output) != Some(0))
            .then(|| format!("{target} {interpreter} {level}: {}", text(&output)))
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Linux native code folds and executes to the reference lines at -O0 and
/// -O2. GCC 14's run time gives the reference lines, and its translation
/// time gives the recorded gcc-fold lines. The constants probe passes for
/// both compilers.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn translation_time_values_match_gcc_14_natively() {
    let work = Work::new("fold");
    let fold = work.copy(&fixture("floating-half-fold.c"));
    let constants = work.copy(&fixture("floating-half-constants.c"));
    let mut script = String::from("set -e\ncd /work\n");
    let mut expected = Vec::new();
    for level in LEVELS {
        for kind in KINDS {
            for form in 0..2 {
                let name = format!("fold-{kind}-{form}{level}.s");
                let args = [
                    format!("-DKIND={kind}"),
                    format!("-DFOLD={form}"),
                    "-ffp-contract=off".into(),
                    level.into(),
                    "-S".into(),
                    "-o".into(),
                    work.path(&name).to_str().unwrap().into(),
                ];
                let output = evocc(LINUX, &args, &fold);
                assert!(output.status.success(), "{name}\n{}", text(&output));
                script.push_str(&format!(
                    "gcc-14 {name} -o t\n./t\n\
                     gcc-14 -std=gnu11 -w {level} -ffp-contract=off -DKIND={kind} -DFOLD={form} floating-half-fold.c -o r\n./r\n"
                ));
                expected.extend(fold_output("fold", kind));
                expected.extend(fold_output(
                    if form == 0 { "fold" } else { "gcc-fold" },
                    kind,
                ));
            }
        }
        let name = format!("constants{level}.s");
        let args = [
            level.to_owned(),
            "-S".into(),
            "-o".into(),
            work.path(&name).to_str().unwrap().into(),
        ];
        let output = evocc(LINUX, &args, &constants);
        assert!(output.status.success(), "{name}\n{}", text(&output));
        script.push_str(&format!(
            "gcc-14 {name} -o t && ./t && echo \"evocc constants {level} 0\"\n\
             gcc-14 -std=gnu11 -w {level} floating-half-constants.c -o r && ./r && echo \"gcc-14 constants {level} 0\"\n"
        ));
        expected.push(format!("evocc constants {level} 0"));
        expected.push(format!("gcc-14 constants {level} 0"));
    }
    let output = sealed(&work, &script);
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.lines().map(str::to_owned).collect::<Vec<_>>(),
        expected
    );
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_translation_time_values_match_the_run_time() {
    let work = Work::new("darwin-fold");
    for kind in KINDS {
        let expected = fold_output("fold", kind)
            .into_iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        for form in 0..2 {
            for level in LEVELS {
                let args = [
                    format!("-DKIND={kind}"),
                    format!("-DFOLD={form}"),
                    "-ffp-contract=off".into(),
                    level.into(),
                ];
                let run = darwin_program(
                    &work,
                    &format!("fold-{kind}-{form}{level}"),
                    "evocc",
                    &args,
                    &fixture("floating-half-fold.c"),
                );
                assert!(run.status.success(), "KIND={kind} FOLD={form} {level}");
                assert_eq!(
                    String::from_utf8_lossy(&run.stdout),
                    expected,
                    "KIND={kind} FOLD={form} {level}"
                );
            }
        }
    }
    for level in LEVELS {
        let run = darwin_program(
            &work,
            &format!("constants{level}"),
            "evocc",
            &[level.to_owned()],
            &fixture("floating-half-constants.c"),
        );
        assert_eq!(run.status.code(), Some(0), "constants {level}");
    }
}

// ---------------------------------------------------------------------------
// Representations and refusals.

/// The EvoIR text spells a half value with its own type and a binary16 HFA
/// with `hfa=Nxf16`. A half operation is a binary32 operation between
/// conversions; a half constant of the source is a binary32 constant.
#[test]
fn evoir_text_spells_the_half_formats() {
    let work = Work::new("evoir");
    let path = work.path("text.c");
    std::fs::write(
        &path,
        "struct H3 { _Float16 a, b, c; };\n\
         __bf16 g(__bf16 x) { return x * (__bf16)2.5f; }\n\
         _Float16 h(_Float16 x) { return x + 1.0f16; }\n\
         struct H3 r(struct H3 v) { return v; }\n",
    )
    .unwrap();
    let output = evocc(LINUX, &["--emit=evoir"], &path);
    assert!(output.status.success(), "{}", text(&output));
    let module = String::from_utf8_lossy(&output.stdout);
    for fragment in [
        "func @g(%arg0: bf16, ",
        "-> (bf16, ",
        "func @h(%arg0: f16, ",
        "fp.convert %v",
        ": bf16",
        ": f16",
        "hfa=3xf16",
    ] {
        assert!(module.contains(fragment), "{fragment}\n{module}");
    }
    for line in module.lines() {
        for op in ["fp.add", "fp.sub", "fp.mul", "fp.div", "fp.neg"] {
            if line.contains(op) {
                assert!(line.ends_with(": f32"), "{line}");
            }
        }
    }
}

/// The forms that L03 does not admit keep a named refusal.
#[test]
fn retained_half_refusals_keep_their_codes() {
    let work = Work::new("refusals");
    let cases: &[(&str, &str, &str)] = &[
        (
            "mixed-arithmetic",
            "int main(void) { _Float16 a = 1; __bf16 b = 1; return a + b > 1; }",
            "E2961",
        ),
        (
            "mixed-comparison",
            "int main(void) { _Float16 a = 1; __bf16 b = 1; return a == b; }",
            "E2961",
        ),
        (
            "mixed-conditional",
            "int main(void) { _Float16 a = 1; __bf16 b = 1; return (a ? a : b) > 0; }",
            "E2961",
        ),
        (
            "mixed-compound",
            "int main(void) { _Float16 a = 1; __bf16 b = 1; a += b; return a > 1; }",
            "E2961",
        ),
        (
            "atomic",
            "_Atomic _Float16 a; int main(void) { a = 1; return 0; }",
            "E2702",
        ),
        (
            "vector-size",
            "typedef __bf16 v4 __attribute__((vector_size(8))); int main(void) { v4 a = {1}; return a[0] > 0; }",
            "E2903",
        ),
        (
            "neon-vector",
            "int main(void) { __Float16x4_t a; __builtin_memset(&a, 0, sizeof a); __Float16x4_t b = a; return sizeof b == 0; }",
            "E3050",
        ),
        (
            "interchange-complex",
            "int main(void) { _Complex _Float16 a = 1; return 0; }",
            "E2202",
        ),
        (
            "arm-complex",
            "int main(void) { _Complex __bf16 a; return 0; }",
            "E1007",
        ),
        (
            "assembly",
            "int main(void) { _Float16 a = 1; __asm__ volatile(\"\" : \"+w\"(a)); return a > 0; }",
            "E3021",
        ),
        (
            "classification",
            "int main(void) { __bf16 a = 1; return __builtin_isnan(a); }",
            "E2951",
        ),
    ];
    for (name, source, code) in cases {
        let path = work.path(&format!("{name}.c"));
        std::fs::write(&path, source).unwrap();
        for target in TARGETS {
            let output = evocc(target, &["--check"], &path);
            assert_eq!(
                first_code(&output).as_deref(),
                Some(*code),
                "{name} {target}\n{}",
                text(&output)
            );
        }
    }
    let path = work.path("general-registers.c");
    std::fs::write(
        &path,
        "unsigned short g(unsigned short x) { _Float16 h; __builtin_memcpy(&h, &x, 2); _Float16 k = h; __builtin_memcpy(&x, &k, 2); return x; }\n",
    )
    .unwrap();
    let output = evocc(LINUX, &["-mgeneral-regs-only", "--emit=asm"], &path);
    assert_eq!(
        first_code(&output).as_deref(),
        Some("E4002"),
        "{}",
        text(&output)
    );
    let output = evocc(LINUX, &["--emit=asm"], &path);
    assert!(output.status.success(), "{}", text(&output));
    let output = evocc(LINUX, &["-fexcess-precision=16", "--emit=evoir"], &path);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("unsupported option `-fexcess-precision=16`"),
        "{}",
        text(&output)
    );
}
