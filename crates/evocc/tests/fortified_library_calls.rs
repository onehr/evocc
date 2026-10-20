//! Fortified C library calls: the object-size checked builtins that the
//! macOS SDK and glibc headers select, GNU argument-pack forwarding and the
//! diagnostic for an unknown builtin.
//!
//! The guard-pattern and interpreter tests need no SDK. The run tests need
//! a macOS host. The Linux comparison with gcc-14 needs the sealed image and
//! is ignored by default; run it explicitly and serially.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures/fortify").join(name)
}

/// A fresh directory, removed on drop unless the test failed.
struct Work(PathBuf);

impl Work {
    fn new(parent: &Path, label: &str) -> Self {
        let path = parent.join(format!(
            "evocc-fortify-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        for name in [
            "guard15.h",
            "guarded.c",
            "calls.c",
            "overflow.c",
            "pack.c",
            "unistd.c",
            "extra-strings.c",
            "checked-print.c",
            "cache-flush.c",
        ] {
            std::fs::copy(fixture(name), path.join(name)).unwrap();
        }
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn evocc(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_evocc"))
            .arg("--error-format=short")
            .current_dir(&self.0)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained fortify work directory: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The direct call targets of an assembly file, in order.
fn calls(assembly: &str) -> Vec<String> {
    assembly
        .lines()
        .filter_map(|line| line.trim().strip_prefix("bl "))
        .map(|target| target.trim().to_owned())
        .collect()
}

fn run_stdin(args: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn the_macos_15_guard_pattern_compiles_without_an_sdk() {
    let work = Work::new(&std::env::temp_dir(), "guard");
    for (target, prefix) in [
        ("aarch64-apple-darwin", "_"),
        ("aarch64-unknown-linux-gnu", ""),
    ] {
        for level in ["-O0", "-O2"] {
            let output = work.evocc(&[
                "-S",
                level,
                &format!("--target={target}"),
                "-nostdinc",
                "guarded.c",
                "-o",
                "guarded.s",
            ]);
            succeeded(&output);
            let called = calls(&std::fs::read_to_string(work.file("guarded.s")).unwrap());
            // The size of `second` is known and its source length is not, so
            // strcpy keeps its check. The other sizes are proven or unknown.
            let expected = [
                "vsnprintf",
                "snprintf",
                "memcpy",
                "__strcpy_chk",
                "format",
                "puts",
            ]
            .map(|name| format!("{prefix}{name}"));
            assert_eq!(called, expected, "{target} {level}");
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn the_guard_pattern_program_runs_on_the_host() {
    let work = Work::new(&std::env::temp_dir(), "guard-run");
    for level in ["-O0", "-O2"] {
        succeeded(&work.evocc(&[level, "-nostdinc", "guarded.c", "-o", "guarded"]));
        let output = Command::new(work.file("guarded")).output().unwrap();
        succeeded(&output);
        assert_eq!(output.stdout, b"x=42!\n", "{level}");
    }
}

#[test]
fn an_unknown_builtin_names_itself_at_the_macro_use() {
    let source = "#define copy(d, s) __builtin___unknown_chk(d, s, __builtin_object_size(d, 0))\n\
                  int main(void) {\n    char d[4];\n    copy(d, \"a\");\n    return 0;\n}\n";
    let output = run_stdin(&["--color=never", "--emit=asm"], source);
    assert_eq!(output.status.code(), Some(1));
    let diagnostics = String::from_utf8(output.stderr).unwrap();
    assert!(
        diagnostics.starts_with(
            "error[E2949]: unknown-builtin: __builtin___unknown_chk\n  --> /dev/stdin:4:5\n"
        ),
        "{diagnostics}"
    );
    assert!(diagnostics.contains("    copy(d, \"a\");"), "{diagnostics}");
    assert!(!diagnostics.contains("W0003"), "{diagnostics}");
}

#[test]
fn unknown_reserved_builtins_are_refused_even_without_warnings() {
    // The GNU builtin namespaces are reserved. A call of an unknown name in
    // them is never an implicit external call, whatever the warning level.
    for (call, name) in [
        ("__builtin_not_provided(1)", "__builtin_not_provided"),
        // `__atomic_fetch_add` is admitted since L11b; `__atomic_fetch_min`
        // is not (EvoIR 10.1.14).
        (
            "(int)__atomic_fetch_min(&value, 1, 5)",
            "__atomic_fetch_min",
        ),
        ("__sync_fetch_and_frob(&value, 1)", "__sync_fetch_and_frob"),
    ] {
        let source = format!("unsigned value;\nint main(void) {{\n    return {call};\n}}\n");
        for flags in [
            &["-w"][..],
            &["-w", "-std=gnu89"],
            &["-Wno-implicit-function-declaration"],
        ] {
            let mut args = flags.to_vec();
            args.extend(["--error-format=short", "--emit=asm"]);
            let output = run_stdin(&args, &source);
            let diagnostics = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{flags:?} {call}: {diagnostics}"
            );
            assert!(
                diagnostics.contains(&format!("error[E2949]: unknown-builtin: {name}")),
                "{flags:?} {call}: {diagnostics}"
            );
        }
    }
    // This inventory probe compiled to an implicit call under -w and then
    // crashed at run time. Since L11c `__atomic_load` is a builtin, so the
    // probe compiles to inline atomic accesses, never an external call.
    let probe =
        std::fs::read_to_string(root().join("tests/fixtures/atomic-scope-boundaries.c")).unwrap();
    let output = run_stdin(
        &["-w", "-DCASE=2", "--error-format=short", "--emit=asm"],
        &probe,
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = String::from_utf8_lossy(&output.stdout);
    assert!(!assembly.contains("bl __atomic") && !assembly.contains("bl ___atomic"));
    assert!(assembly.contains("ldar"), "{assembly}");
}

#[test]
fn the_interpreters_execute_checked_memory_calls_with_their_check() {
    let program = |length: &str| {
        format!(
            "static unsigned long length(unsigned long n) {{ return n; }}\n\
             int main(void) {{\n\
             char a[8], b[16] = \"abcdefghijklmno\";\n\
             __builtin___memcpy_chk(a, b, length({length}), __builtin_object_size(a, 0));\n\
             __builtin___memset_chk(a + 4, 'z', length(4), __builtin_object_size(a + 4, 0));\n\
             __builtin___memmove_chk(a, a + 1, length(3), __builtin_object_size(a, 0));\n\
             return a[0] + a[7];\n}}\n"
        )
    };
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run_stdin(&["-O2", mode], &program("4"));
        succeeded(&output);
        assert_eq!(output.stdout, b"220\n", "{mode}");
        let output = run_stdin(&["-O2", "--error-format=short", mode], &program("12"));
        assert_eq!(output.status.code(), Some(1), "{mode}");
        let diagnostics = String::from_utf8(output.stderr).unwrap();
        assert!(
            diagnostics.contains("error[E3066]: object-size-check-failed"),
            "{mode}: {diagnostics}"
        );
    }
    let output = run_stdin(&["--check", "--error-format=json"], &program("12"));
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("{\"code\":\"E3066\","));
    // The interpreters have no model of the string functions. The checked
    // entry point is an external call, as the plain function is.
    let output = run_stdin(
        &["--check", "--error-format=json"],
        "int main(void) { char a[8]; __builtin___strcpy_chk(a, \"abc\", __builtin_object_size(a, 1)); return a[0]; }",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("{\"code\":\"E3039\","));
}

#[test]
fn argument_packs_are_substituted_at_each_call() {
    let source = std::fs::read_to_string(fixture("pack.c")).unwrap();
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run_stdin(&[level, mode], &source);
            succeeded(&output);
            assert_eq!(output.stdout, b"189\n", "{level} {mode}");
        }
    }
    // A body that keeps the pack cannot be lowered.
    let output = run_stdin(
        &["--error-format=short", "--emit=asm"],
        "int g(int, ...);\nint f(int n, ...) { return g(n, __builtin_va_arg_pack()); }\n",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).starts_with(
            "/dev/stdin:2:33: error[E2950]: evoir-lowering-failed: invalid-va-arg-pack"
        ),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn additional_checked_builtins_are_advertised_and_cache_flush_is_a_real_call() {
    for name in [
        "__builtin___memccpy_chk",
        "__builtin___strlcpy_chk",
        "__builtin___strlcat_chk",
        "__builtin___printf_chk",
        "__builtin___vprintf_chk",
        "__builtin___fprintf_chk",
        "__builtin___vfprintf_chk",
        "__builtin___clear_cache",
    ] {
        succeeded(&run_stdin(
            &["-x", "c", "-fsyntax-only"],
            &format!("_Static_assert(__has_builtin({name}), \"{name}\");"),
        ));
    }
    for (target, callee) in [
        ("aarch64-apple-darwin", "___clear_cache"),
        ("aarch64-unknown-linux-gnu", "__clear_cache"),
    ] {
        let output = run_stdin(
            &["--emit=asm", &format!("--target={target}")],
            "void flush(void *a, void *b) { __builtin___clear_cache(a, b); }",
        );
        succeeded(&output);
        assert_eq!(calls(&String::from_utf8(output.stdout).unwrap()), [callee]);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn argument_packs_run_natively() {
    let work = Work::new(&std::env::temp_dir(), "pack");
    for level in ["-O0", "-O2"] {
        succeeded(&work.evocc(&[level, "pack.c", "-o", "pack"]));
        let status = Command::new(work.file("pack")).status().unwrap();
        assert_eq!(status.code(), Some(189), "{level}");
    }
}

/// Exit status, or 128 plus the signal, as a shell reports it.
#[cfg(target_os = "macos")]
fn shell_status(output: &Output) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    output
        .status
        .code()
        .or_else(|| output.status.signal().map(|signal| 128 + signal))
        .unwrap()
}

#[cfg(target_os = "macos")]
#[test]
fn fortified_calls_with_the_host_sdk_match_apple_clang() {
    let work = Work::new(&std::env::temp_dir(), "host");
    let clang = |source: &str, output: &str| {
        let result = Command::new("clang")
            .current_dir(&work.0)
            .args(["-O2", "-w", source, "-o", output])
            .output()
            .unwrap();
        succeeded(&result);
    };
    clang("calls.c", "calls-clang");
    clang("overflow.c", "overflow-clang");
    let reference = Command::new(work.file("calls-clang")).output().unwrap();
    succeeded(&reference);
    for level in ["-O0", "-O2"] {
        succeeded(&work.evocc(&[level, "calls.c", "-o", "calls"]));
        let output = Command::new(work.file("calls")).output().unwrap();
        succeeded(&output);
        assert_eq!(output.stdout, reference.stdout, "{level}");

        succeeded(&work.evocc(&[level, "overflow.c", "-o", "overflow"]));
        for (argument, call) in [
            ("short", "strcpy"),
            ("much-too-long", "strcpy"),
            ("much-too-long", "memcpy"),
            ("much-too-long", "snprintf"),
        ] {
            let run = |program: &str| {
                Command::new(work.file(program))
                    .args([argument, call])
                    .output()
                    .unwrap()
            };
            let expected = run("overflow-clang");
            let actual = run("overflow");
            assert_eq!(
                shell_status(&actual),
                shell_status(&expected),
                "{level} {argument} {call}"
            );
            assert_eq!(actual.stdout, expected.stdout, "{level} {argument} {call}");
            if argument != "short" {
                // The C library stops the program; it does not return.
                assert_ne!(shell_status(&actual), 0, "{level} {call}");
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn additional_string_checks_and_cache_flush_match_apple_clang() {
    let work = Work::new(&std::env::temp_dir(), "extra-host");
    for program in ["extra-strings", "cache-flush"] {
        succeeded(
            &Command::new("clang")
                .current_dir(&work.0)
                .args([
                    "-O2",
                    &format!("{program}.c"),
                    "-o",
                    &format!("{program}-clang"),
                ])
                .output()
                .unwrap(),
        );
    }
    for level in ["-O0", "-O2"] {
        for program in ["extra-strings", "cache-flush"] {
            succeeded(&work.evocc(&[level, &format!("{program}.c"), "-o", program]));
        }
        for operation in ["0", "1", "2"] {
            for length in ["0", "1", "3", "8", "9"] {
                let run = |program: &str| {
                    Command::new(work.file(program))
                        .args([length, operation])
                        .output()
                        .unwrap()
                };
                let expected = run("extra-strings-clang");
                let actual = run("extra-strings");
                assert_eq!(
                    shell_status(&actual),
                    shell_status(&expected),
                    "{level} {operation} {length}: {}",
                    String::from_utf8_lossy(&actual.stderr)
                );
                assert_eq!(
                    actual.stdout, expected.stdout,
                    "{level} {operation} {length}"
                );
                assert_eq!(actual.status.success(), length != "9");
            }
        }
        for program in ["cache-flush", "cache-flush-clang"] {
            let output = Command::new(work.file(program)).output().unwrap();
            succeeded(&output);
            assert_eq!(output.stdout, b"returned=17\nreturned=18\n");
        }
    }
}

/// Compile with EvoCC for Linux, then assemble, link and run both the EvoCC
/// output and the gcc-14 build of the same source in the sealed image.
#[test]
#[ignore = "sealed Linux fortify lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_fortified_programs_match_gcc_14() {
    let evidence = root().join(".evidence/rust-batch1-20261013");
    std::fs::create_dir_all(&evidence).unwrap();
    let work = Work::new(&evidence.canonicalize().unwrap(), "linux");
    let sysroot = root().join("sysroot/aarch64-unknown-linux-gnu");
    let sysroot = format!("--sysroot={}", sysroot.display());
    for level in ["2", "3"] {
        for program in [
            "calls",
            "overflow",
            "unistd",
            "checked-print",
            "cache-flush",
        ] {
            let output = work.evocc(&[
                "-S",
                "-O2",
                "--target=aarch64-unknown-linux-gnu",
                &sysroot,
                &format!("-D_FORTIFY_SOURCE={level}"),
                &format!("{program}.c"),
                "-o",
                &format!("{program}-evocc-{level}.s"),
            ]);
            succeeded(&output);
        }
    }
    let script = r#"set -eu
cd /work
for level in 2 3; do
  for program in calls overflow unistd checked-print cache-flush; do
    gcc-14 -O2 -D_FORTIFY_SOURCE=$level $program.c -o $program-gcc-$level
    gcc-14 $program-evocc-$level.s -o $program-evocc-$level
  done
  for program in calls unistd checked-print cache-flush; do
    ./$program-gcc-$level > gcc.out
    ./$program-evocc-$level > evocc.out
    cmp gcc.out evocc.out
  done
  for operation in p f v s; do
    set +e
    ./checked-print-gcc-$level $operation > gcc.out 2> gcc.err; gcc=$?
    ./checked-print-evocc-$level $operation > evocc.out 2> evocc.err; evocc=$?
    set -e
    test "$gcc" = 134
    test "$evocc" = "$gcc"
    cmp gcc.out evocc.out
    cmp gcc.err evocc.err
    echo "level $level [writable-format $operation] status $evocc: $(cat evocc.err)"
  done
  set +e
  ./unistd-gcc-$level much-too-long read > gcc.out 2> gcc.err; gcc=$?
  ./unistd-evocc-$level much-too-long read > evocc.out 2> evocc.err; evocc=$?
  set -e
  test "$gcc" = "$evocc"
  cmp gcc.err evocc.err
  echo "level $level [much-too-long read] status $evocc: $(cat evocc.out evocc.err)"
  for case in "short strcpy" "much-too-long strcpy" "much-too-long memcpy" "much-too-long snprintf"; do
    set +e
    ./overflow-gcc-$level $case > gcc.out 2> gcc.err; gcc=$?
    ./overflow-evocc-$level $case > evocc.out 2> evocc.err; evocc=$?
    set -e
    test "$gcc" = "$evocc"
    cmp gcc.out evocc.out
    cmp gcc.err evocc.err
    echo "level $level [$case] status $evocc: $(cat evocc.out evocc.err)"
  done
done
"#;
    let output = Command::new("/usr/local/bin/container")
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
        .args(["evo-nexlab:sealed", "bash", "-c", script])
        .output()
        .unwrap();
    succeeded(&output);
    let report = String::from_utf8(output.stdout).unwrap();
    eprint!("{report}");
    for level in ["2", "3"] {
        for call in ["strcpy", "memcpy", "snprintf", "read"] {
            assert!(
                report.contains(&format!(
                    "level {level} [much-too-long {call}] status 134: *** buffer overflow detected ***: terminated"
                )),
                "{report}"
            );
        }
    }
}
