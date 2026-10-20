//! Calls that return twice (EvoIR 3.10, D16), with GCC 14's rule.
//!
//! A function returns twice when a declaration of it carries `returns_twice`,
//! or by GCC's name rule (`special_function_p`): external linkage and the C
//! identifier `setjmp` or `sigsetjmp` after one leading `__` or `_` is
//! removed, or exactly `savectx`, `vfork` or `getcontext`. A direct call of
//! such a function is `call.returns_twice` in EvoIR, also through `*f` or
//! `&f`. An indirect call is an ordinary call. The function that makes such a
//! call is never inlined, and neither is the function that returns twice.
//! Every value live across the call stays in a private stack slot; the
//! native runs compare that with Apple clang and with GCC 14.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures() -> PathBuf {
    root().join("tests/fixtures/returns-twice")
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
            "evocc-returns-twice-calls-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        for entry in std::fs::read_dir(fixtures()).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), path.join(entry.file_name())).unwrap();
        }
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
            eprintln!("retained failed returns-twice probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn evocc(source: &Path, target: &str, arguments: &[&str]) -> Output {
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
    command.args(arguments).arg(source).output().unwrap()
}

/// The EvoIR text of `source` at `-O2`.
fn evoir(work: &Work, name: &str, source: &str, target: &str) -> String {
    let path = work.source(name, source);
    let output = evocc(&path, target, &["-O2", "--emit=evoir"]);
    assert!(output.status.success(), "{source}\n{}", text(&output));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn gcc_names_mark_a_direct_call() {
    let work = Work::new("names");
    // `__sigsetjmp` has eleven bytes, the limit of the rule.
    for name in [
        "setjmp",
        "_setjmp",
        "__setjmp",
        "sigsetjmp",
        "_sigsetjmp",
        "__sigsetjmp",
        "savectx",
        "vfork",
        "getcontext",
    ] {
        let source = format!(
            "int {name}(void *);\nstatic char buffer[512];\nint main(void) {{ return {name}(buffer); }}\n"
        );
        for target in TARGETS {
            let text = evoir(&work, "named.c", &source, target);
            assert!(
                text.contains(&format!(" = call.returns_twice @{name}(")),
                "{target} {name}\n{text}"
            );
        }
    }
    // Only one leading `__` or `_` is removed, and only from the two jump
    // names; every other spelling, also of twelve bytes, is ordinary.
    for name in [
        "___setjmp",
        "___sigsetjmp",
        "_vfork",
        "__vfork",
        "_getcontext",
        "_savectx",
        "setjmp_",
        "sigsetjmp1",
        "longjmp",
        "fork",
        "setcontext",
    ] {
        let source = format!(
            "int {name}(void *);\nstatic char buffer[512];\nint main(void) {{ return {name}(buffer); }}\n"
        );
        for target in TARGETS {
            let text = evoir(&work, "ordinary.c", &source, target);
            assert!(
                text.contains(&format!(" = call @{name}(")) && !text.contains("returns_twice"),
                "{target} {name}\n{text}"
            );
        }
    }
}

#[test]
fn linkage_labels_and_designators_decide_the_mark() {
    let work = Work::new("designators");
    let marked = |text: &str| text.contains(" = call.returns_twice @");
    for (label, source, expected) in [
        // A function with internal linkage is not one of GCC's names.
        (
            "static",
            "static __attribute__((noinline)) int setjmp(void *p) { return p == 0; }\n\
             static char buffer[8];\nint main(void) { return setjmp(buffer); }\n",
            false,
        ),
        // The C identifier counts, not the assembler name.
        (
            "label alias",
            "int h(void *) __asm__(\"setjmp\");\nstatic char buffer[8];\n\
             int main(void) { return h(buffer); }\n",
            false,
        ),
        (
            "renamed setjmp",
            "int setjmp(void *) __asm__(\"evocc_point\");\nstatic char buffer[8];\n\
             int main(void) { return setjmp(buffer); }\n",
            true,
        ),
        // A call through a pointer object is an ordinary call.
        (
            "indirect",
            "int setjmp(void *);\nstatic char buffer[8];\n\
             int main(void) { int (*p)(void *) = setjmp; return p(buffer); }\n",
            false,
        ),
        // GCC folds `*f` and `&f`, so these calls are direct.
        (
            "dereference",
            "int setjmp(void *);\nstatic char buffer[8];\n\
             int main(void) { return (*setjmp)(buffer); }\n",
            true,
        ),
        (
            "address",
            "int setjmp(void *);\nstatic char buffer[8];\n\
             int main(void) { return (&setjmp)(buffer); }\n",
            true,
        ),
        (
            "chain",
            "int setjmp(void *);\nstatic char buffer[8];\n\
             int main(void) { return (**&*setjmp)(buffer); }\n",
            true,
        ),
        // EvoCC's conservative addition: GCC 14 does not recognize a
        // block-scope declaration, and EvoCC does.
        (
            "block scope",
            "int main(void) { int vfork(void); return vfork(); }\n",
            true,
        ),
        // The attribute marks any name, in any declaration order.
        (
            "attribute",
            "int point(void);\nint main(void) { return point(); }\n\
             int point(void) __attribute__((returns_twice));\n",
            true,
        ),
    ] {
        for target in TARGETS {
            let text = evoir(&work, "designator.c", source, target);
            assert_eq!(marked(&text), expected, "{target} {label}\n{text}");
            if !expected {
                assert!(!text.contains("returns_twice"), "{target} {label}\n{text}");
            }
        }
    }
}

#[test]
fn a_function_that_returns_twice_or_calls_such_a_function_is_not_inlined() {
    let work = Work::new("inline");
    let wrapper = |callee: &str| {
        format!(
            "int {callee}(void *);\nstatic char buffer[512];\n\
             static inline int wrap(void) {{ return {callee}(buffer) + 1; }}\n\
             int main(void) {{ return wrap(); }}\n"
        )
    };
    for target in TARGETS {
        // The control: an ordinary callee lets `wrap` be substituted.
        let text = evoir(&work, "control.c", &wrapper("ordinary"), target);
        assert!(
            !text.contains(" = call @wrap(") && text.contains(" = call @ordinary("),
            "{target}\n{text}"
        );
        let text = evoir(&work, "wrapper.c", &wrapper("sigsetjmp"), target);
        assert!(
            text.contains("func @wrap(")
                && text.contains(" = call @wrap(")
                && text.contains(" = call.returns_twice @sigsetjmp("),
            "{target}\n{text}"
        );
        // A second return can deliver another value, so the result of a
        // returns-twice function is never a constant.
        let point = "static inline __attribute__((returns_twice)) int point(void) { return 0; }\n\
                     int main(void) { return point(); }\n";
        let text = evoir(&work, "point.c", point, target);
        assert!(
            text.contains("func @point(") && text.contains(" = call.returns_twice @point("),
            "{target}\n{text}"
        );
        let control = point.replace(" __attribute__((returns_twice))", "");
        let text = evoir(&work, "constant.c", &control, target);
        assert!(!text.contains(" = call @point("), "{target}\n{text}");
    }
}

#[test]
fn an_always_inline_function_that_calls_setjmp_is_refused() {
    let work = Work::new("always");
    let source = "#include <setjmp.h>\nstatic jmp_buf buffer;\n\
                  static inline __attribute__((always_inline)) int f(void) { return setjmp(buffer); }\n\
                  int main(void) { return f(); }\n";
    let path = work.source("always.c", source);
    let line = 3;
    let column = source
        .lines()
        .nth(2)
        .unwrap()
        .find("setjmp(buffer)")
        .unwrap()
        + 1;
    for target in TARGETS {
        for mode in [
            &["-O0", "--emit=asm"][..],
            &["-O2", "--emit=asm"],
            &["-fsyntax-only"],
            &["--check"],
            &["--interpret-c-hir"],
            &["--interpret-evoir"],
        ] {
            let output = evocc(&path, target, mode);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{target} {mode:?}\n{}",
                text(&output)
            );
            assert!(
                output.stdout.is_empty(),
                "{target} {mode:?}\n{}",
                text(&output)
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            let expected = format!(
                "{}:{line}:{column}: error[E2957]: always-inline-returns-twice",
                path.display()
            );
            assert_eq!(
                stderr.lines().next(),
                Some(expected.as_str()),
                "{target} {mode:?}\n{stderr}"
            );
        }
        // An unused static function is not retained, so nothing is refused.
        let unused = work.source("unused.c", &source.replace("return f(); }", "return 0; }"));
        let output = evocc(&unused, target, &["-O2", "--emit=asm"]);
        assert!(output.status.success(), "{target}\n{}", text(&output));
    }
}

/// musl's `src/time/timer_create.c` calls `setjmp` in a loop. Its reduction
/// `timer-loop.c` keeps the allocation of the original: each iteration copies
/// the kept values before the call, and the spill store of each copy writes
/// the copy's private slot before the copy itself. The allocation check takes
/// that store as the copy and still guards the slot around the loop. A check
/// that ignored the pending copy refused both levels on both targets (E4020).
#[test]
fn a_returns_twice_call_in_a_loop_compiles() {
    let source = fixtures().join("timer-loop.c");
    for target in TARGETS {
        for level in ["-O0", "-O2"] {
            let output = evocc(&source, target, &[level, "--emit=evoir"]);
            assert!(
                output.status.success(),
                "{target} {level}\n{}",
                text(&output)
            );
            let evoir = String::from_utf8_lossy(&output.stdout);
            assert!(
                evoir.contains(" = call.returns_twice @setjmp("),
                "{target} {level}\n{evoir}"
            );
            let output = evocc(&source, target, &[level, "--emit=asm"]);
            assert!(
                output.status.success(),
                "{target} {level}\n{}",
                text(&output)
            );
        }
    }
}

/// Run `program` and return its exit status.
#[cfg(target_os = "macos")]
fn status(program: &Path) -> Option<i32> {
    Command::new(program).output().unwrap().status.code()
}

/// Darwin: EvoCC objects agree with Apple clang at `-O0` and `-O2`. For the
/// two-unit program, each unit comes from each compiler.
#[cfg(target_os = "macos")]
#[test]
fn darwin_programs_agree_with_apple_clang() {
    let work = Work::new("darwin");
    let run = |command: &mut Command| {
        let output = command.current_dir(&work.0).output().unwrap();
        assert!(output.status.success(), "{command:?}\n{}", text(&output));
    };
    let mut failures = Vec::new();
    let assembled = work.0.join("checkpoint.o");
    run(Command::new("clang")
        .args(["-c", "my-checkpoint-darwin.s", "-o"])
        .arg(&assembled));
    for level in ["-O0", "-O2"] {
        for program in ["red-probe", "jumps", "vfork", "user-prototype"] {
            let extra: &[&str] = if program == "user-prototype" {
                &["checkpoint.o"]
            } else {
                &[]
            };
            let source = format!("{program}.c");
            let reference = work.0.join(format!("{program}-clang{level}"));
            run(Command::new("clang")
                .args([level, source.as_str()])
                .args(extra)
                .arg("-o")
                .arg(&reference));
            let object = work.0.join(format!("{program}{level}.o"));
            let output = evocc(
                &work.0.join(&source),
                DARWIN,
                &[level, "-c", "-o", object.to_str().unwrap()],
            );
            assert!(
                output.status.success(),
                "{program} {level}\n{}",
                text(&output)
            );
            let linked = work.0.join(format!("{program}-evocc{level}"));
            run(Command::new("clang")
                .arg(&object)
                .args(extra)
                .arg("-o")
                .arg(&linked));
            let (expected, actual) = (status(&reference), status(&linked));
            if expected != Some(0) || actual != Some(0) {
                failures.push(format!(
                    "{program} {level}: clang {expected:?}, EvoCC {actual:?}"
                ));
            }
        }
        for unit in ["cross-main", "cross-other"] {
            let source = format!("{unit}.c");
            run(Command::new("clang").args([
                level,
                "-c",
                source.as_str(),
                "-o",
                &format!("{unit}-clang{level}.o"),
            ]));
            let object = work.0.join(format!("{unit}-evocc{level}.o"));
            let output = evocc(
                &work.0.join(&source),
                DARWIN,
                &[level, "-c", "-o", object.to_str().unwrap()],
            );
            assert!(output.status.success(), "{unit} {level}\n{}", text(&output));
        }
        for main in ["clang", "evocc"] {
            for other in ["clang", "evocc"] {
                let linked = work.0.join(format!("cross-{main}-{other}{level}"));
                run(Command::new("clang")
                    .arg(format!("cross-main-{main}{level}.o"))
                    .arg(format!("cross-other-{other}{level}.o"))
                    .arg("-o")
                    .arg(&linked));
                if status(&linked) != Some(0) {
                    failures.push(format!(
                        "cross {level}: main {main}, other {other}: {:?}",
                        status(&linked)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Linux: EvoCC assembly agrees with GCC 14 at `-O0` and `-O2`, in the sealed
/// image. For the two-unit program, each unit comes from each compiler.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_programs_agree_with_gcc_14() {
    let work = Work::new("linux");
    let mut script = String::from("set -u\ncd /work\nfailed=0\n");
    let mut lines = 0;
    let check = |label: &str| format!("s=$?; echo \"{label} $s\"; [ \"$s\" -eq 0 ] || failed=1\n");
    for level in ["-O0", "-O2"] {
        for program in [
            "red-probe",
            "jumps",
            "vfork",
            "getcontext",
            "user-prototype",
            "pthread-cancel",
            "cross-main",
            "cross-other",
        ] {
            let assembly = work.0.join(format!("{program}{level}.s"));
            let output = evocc(
                &work.0.join(format!("{program}.c")),
                LINUX,
                &[level, "-S", "-o", assembly.to_str().unwrap()],
            );
            assert!(
                output.status.success(),
                "{program} {level}\n{}",
                text(&output)
            );
        }
        for program in [
            "red-probe",
            "jumps",
            "vfork",
            "getcontext",
            "user-prototype",
            "pthread-cancel",
        ] {
            let extra = if program == "user-prototype" {
                " my-checkpoint-linux.s"
            } else {
                ""
            };
            script.push_str(&format!(
                "gcc-14 -std=gnu11 -w {level} -pthread {program}.c{extra} -o {program}-gcc{level} && ./{program}-gcc{level}; {}",
                check(&format!("{program} {level} gcc-14"))
            ));
            script.push_str(&format!(
                "gcc-14 -pthread {program}{level}.s{extra} -o {program}-evocc{level} && ./{program}-evocc{level}; {}",
                check(&format!("{program} {level} evocc"))
            ));
            lines += 2;
        }
        script.push_str(&format!(
            "gcc-14 -std=gnu11 -w {level} -c cross-main.c -o cross-main-gcc{level}.o\n\
             gcc-14 -std=gnu11 -w {level} -c cross-other.c -o cross-other-gcc{level}.o\n\
             gcc-14 -c cross-main{level}.s -o cross-main-evocc{level}.o\n\
             gcc-14 -c cross-other{level}.s -o cross-other-evocc{level}.o\n"
        ));
        for main in ["gcc", "evocc"] {
            for other in ["gcc", "evocc"] {
                script.push_str(&format!(
                    "gcc-14 cross-main-{main}{level}.o cross-other-{other}{level}.o -o cross-{main}-{other}{level} && ./cross-{main}-{other}{level}; {}",
                    check(&format!("cross {level} main-{main} other-{other}"))
                ));
                lines += 1;
            }
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

/// The pinned ladder sources that call `setjmp` (D16 exit): cJSON's Unity
/// harness, whose `TEST_PROTECT` is `setjmp`, and zlib's puff, blast and
/// minizip. Each compiles at `-O0` and `-O2`, and each `setjmp` call is a
/// returns-twice call. The sources are read in place and never changed.
#[test]
fn pinned_ladder_programs_that_call_setjmp_compile() {
    let ladder = root().join(".build/ladder");
    assert!(
        ladder.is_dir(),
        "the pinned ladder sources are required for this acceptance test"
    );
    let zlib = ladder.join("zlib/zlib-1.3.2");
    for (unit, options) in [
        (
            ladder.join("cjson/cJSON-1.7.19/tests/unity/src/unity.c"),
            vec!["-std=c89".to_owned()],
        ),
        (zlib.join("contrib/puff/puff.c"), Vec::new()),
        (zlib.join("contrib/blast/blast.c"), Vec::new()),
        (
            zlib.join("contrib/minizip/zip.c"),
            vec![format!("-I{}", zlib.display())],
        ),
    ] {
        for level in ["-O0", "-O2"] {
            let mut arguments = vec![level.to_owned(), "--emit=evoir".to_owned()];
            arguments.extend(options.iter().cloned());
            let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
            let output = evocc(&unit, LINUX, &arguments);
            assert!(
                output.status.success(),
                "{} {level}\n{}",
                unit.display(),
                text(&output)
            );
            let evoir = String::from_utf8_lossy(&output.stdout);
            assert!(
                evoir.contains(" = call.returns_twice @_setjmp("),
                "{} {level}: no returns-twice call",
                unit.display()
            );
            let mut arguments = arguments.clone();
            arguments[1] = "--emit=asm";
            let output = evocc(&unit, LINUX, &arguments);
            assert!(
                output.status.success(),
                "{} {level}\n{}",
                unit.display(),
                text(&output)
            );
        }
    }
}

/// Linux: zlib's puff and blast reach their `longjmp` on truncated input and
/// return a second time from `setjmp`. EvoCC builds of the two units, linked
/// with the GCC 14 drivers, give GCC 14's statuses on the original test files
/// and on the truncated inputs of puff's coverage target. cJSON's Unity tests
/// pass with EvoCC's `unity.c`.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_pinned_programs_agree_with_gcc_14() {
    let work = Work::new("ladder");
    let zlib = root().join(".build/ladder/zlib/zlib-1.3.2/contrib");
    let cjson = root().join(".build/ladder/cjson/cJSON-1.7.19");
    for (from, to) in [
        (zlib.join("puff/puff.c"), "puff.c"),
        (zlib.join("puff/puff.h"), "puff.h"),
        (zlib.join("puff/pufftest.c"), "pufftest.c"),
        (zlib.join("puff/zeros.raw"), "zeros.raw"),
        (zlib.join("blast/blast.c"), "blast.c"),
        (zlib.join("blast/blast.h"), "blast.h"),
        (zlib.join("blast/blast-test.c"), "blast-test.c"),
        (zlib.join("blast/test.pk"), "test.pk"),
        (zlib.join("blast/test.txt"), "test.txt"),
    ] {
        std::fs::copy(&from, work.0.join(to)).unwrap();
    }
    let copied = Command::new("cp")
        .arg("-R")
        .arg(&cjson)
        .arg(work.0.join("cjson"))
        .status()
        .unwrap();
    assert!(copied.success());
    // puff's coverage target: each input and the status it expects.
    let inputs = [
        ("04", 2),
        ("00", 2),
        ("00 00 00 00 00", 254),
        ("00 01 00 fe ff", 2),
        ("02 7e ff ff", 246),
        ("02", 2),
        ("04 80 49 92 24 49 92 24 0f b4 ff ff c3 04", 2),
        ("04 80 49 92 24 49 92 24 71 ff ff 93 11 00", 249),
        ("04 c0 81 08 00 00 00 00 20 7f eb 0b 00 00", 246),
        ("1a 07", 246),
        ("0c c0 81 00 00 00 00 00 90 ff 6b 04", 245),
        ("fc 00 00", 253),
        ("04 00 fe ff", 252),
        ("04 00 24 49", 251),
        ("04 80 49 92 24 49 92 24 0f b4 ff ff c3 84", 248),
        ("04 00 24 e9 ff ff", 250),
        ("04 00 24 e9 ff 6d", 247),
    ];
    let unity_tests = ["parse_number", "parse_string", "print_number", "misc_tests"];
    let mut script = String::from("set -u\ncd /work\nfailed=0\n");
    let mut lines = 0;
    fn check(
        script: &mut String,
        lines: &mut usize,
        label: String,
        command: String,
        expected: i32,
    ) {
        script.push_str(&format!(
            "{command}; s=$?; echo \"{label} $s\"; [ \"$s\" -eq {expected} ] || failed=1\n"
        ));
        *lines += 1;
    }
    for level in ["-O0", "-O2"] {
        for unit in ["puff", "blast"] {
            let assembly = work.0.join(format!("{unit}{level}.s"));
            let output = evocc(
                &work.0.join(format!("{unit}.c")),
                LINUX,
                &[level, "-S", "-o", assembly.to_str().unwrap()],
            );
            assert!(output.status.success(), "{unit} {level}\n{}", text(&output));
        }
        let unity = work.0.join(format!("unity{level}.s"));
        let output = evocc(
            &work.0.join("cjson/tests/unity/src/unity.c"),
            LINUX,
            &[level, "-std=c89", "-S", "-o", unity.to_str().unwrap()],
        );
        assert!(output.status.success(), "unity {level}\n{}", text(&output));
        for (compiler, puff, blast) in [
            (
                "gcc-14",
                format!("gcc-14 -std=gnu11 -w {level} puff.c"),
                format!("gcc-14 -std=gnu11 -w {level} blast.c"),
            ),
            (
                "evocc",
                format!("gcc-14 puff{level}.s"),
                format!("gcc-14 blast{level}.s"),
            ),
        ] {
            script.push_str(&format!(
                "{puff} pufftest.c -o puff-{compiler}{level}\n{blast} blast-test.c -o blast-{compiler}{level}\n"
            ));
            check(
                &mut script,
                &mut lines,
                format!("puff zeros.raw {level} {compiler}"),
                format!("./puff-{compiler}{level} zeros.raw > /dev/null"),
                0,
            );
            for (input, expected) in inputs {
                let bytes: String = input
                    .split(' ')
                    .map(|byte| format!("\\{:03o}", u8::from_str_radix(byte, 16).unwrap()))
                    .collect();
                check(
                    &mut script,
                    &mut lines,
                    format!("puff '{input}' {level} {compiler}"),
                    format!("printf '{bytes}' | ./puff-{compiler}{level} > /dev/null 2>&1"),
                    expected,
                );
            }
            check(
                &mut script,
                &mut lines,
                format!("blast test.pk {level} {compiler}"),
                format!("./blast-{compiler}{level} < test.pk | cmp - test.txt"),
                0,
            );
            // Truncated input runs out inside the decoder, which jumps back.
            check(
                &mut script,
                &mut lines,
                format!("blast truncated {level} {compiler}"),
                format!("head -c 5 test.pk | ./blast-{compiler}{level} > /dev/null 2>&1"),
                2,
            );
        }
        for test in unity_tests {
            script.push_str(&format!(
                "(cd cjson/tests && gcc-14 -std=c89 -w {level} -c {test}.c -o {test}{level}.o && gcc-14 {test}{level}.o ../../unity{level}.s -lm -o {test}{level})\n"
            ));
            check(
                &mut script,
                &mut lines,
                format!("unity {test} {level} evocc"),
                format!("(cd cjson/tests && ./{test}{level} > /dev/null)"),
                0,
            );
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
    assert_eq!(stdout.lines().count(), lines, "{stdout}");
}
