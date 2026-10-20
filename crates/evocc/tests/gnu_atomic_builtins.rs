//! L11b: the GNU `__atomic` read-modify-write, exchange, compare-exchange,
//! test-and-set, clear, fence and lock-free builtins on 1-, 2-, 4- and 8-byte
//! scalars (EvoIR 10.1.14). Both interpreters run one thread: they show
//! values, operand effects, the write-back and the order of atomic events,
//! not inter-thread ordering. A fence other than a relaxed one is the
//! ordered barrier assembly that both interpreters refuse (E3021). The
//! crossed native tests carry the ordering claim, and a passing run of them
//! is not a memory-model proof either.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
/// The value probe's cases with a defined answer; CASE 6 is 16 bytes.
const VALUE_CASES: [u8; 8] = [0, 1, 2, 3, 4, 5, 7, 8];
/// CASE 5 reaches a seq_cst fence, which the interpreters do not execute.
const INTERPRETED_CASES: [u8; 7] = [0, 1, 2, 3, 4, 7, 8];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name)
}

fn probe() -> PathBuf {
    fixture("rules-20261013/l11b-rmw-cas.c")
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn evocc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_evocc"))
}

/// Compile a source text from standard input.
fn stdin(source: &str, args: &[&str]) -> Output {
    let mut child = evocc()
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

fn refused(source: &str, code: &str) {
    refused_in(source, code, &[&[], &["-w"]]);
}

fn refused_in(source: &str, code: &str, modes: &[&[&str]]) {
    for target in TARGETS {
        let target = format!("--target={target}");
        for extra in modes {
            let mut args = vec![target.as_str(), "--emit=evoir"];
            args.extend_from_slice(extra);
            let output = stdin(source, &args);
            assert!(!output.status.success(), "{source} {args:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains(&format!("error[{code}]")),
                "{source} {args:?}: {stderr}"
            );
            assert!(output.stdout.is_empty(), "{source}: a refusal emitted IR");
        }
    }
}

fn accepted(source: &str) {
    for target in TARGETS {
        let output = stdin(source, &[&format!("--target={target}"), "--emit=evoir"]);
        assert!(output.status.success(), "{source}: {}", text(&output));
    }
}

#[test]
fn both_interpreters_execute_the_value_matrix_on_both_targets() {
    for target in TARGETS {
        for mode in INTERPRETERS {
            for level in LEVELS {
                for case in INTERPRETED_CASES {
                    let output = evocc()
                        .arg(format!("--target={target}"))
                        .args(["-std=gnu11", level, mode])
                        .arg(format!("-DCASE={case}"))
                        .arg(probe())
                        .output()
                        .unwrap();
                    assert!(output.status.success(), "CASE={case}: {}", text(&output));
                    // CASE 3 adds bytes (0), CASE 7 evaluates left to right (0).
                    assert_eq!(
                        String::from_utf8_lossy(&output.stdout).trim(),
                        "0",
                        "CASE={case} {target} {mode} {level}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_existing_boundary_fixtures_now_execute() {
    let scope = fixture("atomic-scope-boundaries.c");
    let cas = fixture("atomic-cas-boundaries.c");
    for (source, case) in [(&scope, 1), (&cas, 0), (&cas, 1)] {
        for mode in INTERPRETERS {
            let output = evocc()
                .arg(format!("--target={LINUX}"))
                .arg(format!(
                    "--sysroot={}",
                    root().join("sysroot").join(LINUX).display()
                ))
                .arg(mode)
                .arg(format!("-DCASE={case}"))
                .arg(source)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", text(&output));
            assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
        }
    }
}

#[test]
fn operands_outside_the_domain_are_refused() {
    let negative = std::fs::read_to_string(fixture("rules-20261013/l11b-rmw-negative.c")).unwrap();
    for (case, code) in [
        (0, Some("E2913")),  // fetch_add on _Bool
        (1, Some("E2913")),  // fetch_add on float
        (2, Some("E2913")),  // a record (L11c)
        (3, Some("E2913")),  // a const object
        (4, Some("E2947")),  // failure order release
        (5, Some("E2947")),  // failure order acq_rel
        (6, Some("E2947")),  // clear with acquire
        (7, Some("E2947")),  // store with acquire
        (8, None),           // a void * object adds bytes
        (9, Some("E2947")),  // order constant 6
        (10, Some("E2947")), // failure stronger than success
    ] {
        let source = format!("#define NEG {case}\n{negative}");
        match code {
            Some(code) => refused(&source, code),
            None => accepted(&source),
        }
    }
    let probe = std::fs::read_to_string(probe()).unwrap();
    // 16-byte operands stay refused in this slice.
    refused(&format!("#define CASE 6\n{probe}"), "E2913");
    for source in [
        "int *p; int f(void){return __atomic_fetch_and(&p,1,5)!=0;}",
        "void (*f)(void); int g(void){return __atomic_fetch_add(&f,1,5)!=0;}",
        "_Bool b; int f(void){return __atomic_fetch_or(&b,1,5);}",
        "unsigned __int128 q; int f(void){return __atomic_exchange_n(&q,1,5)!=0;}",
        "int v, e; int f(void){return __atomic_compare_exchange_n(&v,e,1,0,5,5);}",
    ] {
        refused(source, "E2913");
    }
}

/// E2947 exactly where GCC 14 warns: a compare-exchange failure order that
/// is release or acq_rel or stronger than the success order, a clear that is
/// not relaxed, release or seq_cst, and every constant outside 0..5.
#[test]
fn constant_orders_follow_the_gcc_warning_set() {
    for success in 0..6 {
        for failure in 0..6 {
            let source = format!(
                "int v, e; int f(void){{return __atomic_compare_exchange_n(&v,&e,1,1,{success},{failure});}}"
            );
            if failure == 3 || failure == 4 || failure > success {
                refused(&source, "E2947");
            } else {
                accepted(&source);
            }
        }
    }
    for order in 0..6 {
        let clear = format!("_Bool b; void f(void){{__atomic_clear(&b,{order});}}");
        if matches!(order, 0 | 3 | 5) {
            accepted(&clear);
        } else {
            refused(&clear, "E2947");
        }
        for form in [
            "__atomic_fetch_add(&v,1,O)",
            "__atomic_nand_fetch(&v,1,O)",
            "__atomic_exchange_n(&v,1,O)",
            "__atomic_test_and_set(&v,O)",
        ] {
            accepted(&format!(
                "int v; int f(void){{return {};}}",
                form.replace('O', &order.to_string())
            ));
        }
        accepted(&format!(
            "void f(void){{__atomic_thread_fence({order});__atomic_signal_fence({order});}}"
        ));
    }
    for order in ["6", "-1", "65536"] {
        for form in [
            "return __atomic_fetch_add(&v,1,O);",
            "return __atomic_exchange_n(&v,1,O);",
            "return __atomic_test_and_set(&v,O);",
            "__atomic_thread_fence(O); return 0;",
            "__atomic_signal_fence(O); return 0;",
            "return __atomic_compare_exchange_n(&v,&v,1,0,O,0);",
            "return __atomic_compare_exchange_n(&v,&v,1,0,5,O);",
        ] {
            refused(
                &format!("int v; int f(void){{{}}}", form.replace('O', order)),
                "E2947",
            );
        }
    }
}

#[test]
fn unadmitted_spellings_stay_refused_at_every_warning_level() {
    // The generic `__atomic_load`, `__atomic_store`, `__atomic_exchange` and
    // `__atomic_compare_exchange` forms are admitted since L11c
    // (generic_atomic_records.rs executes them).
    for source in [
        "int v; int f(void){return __atomic_fetch_min(&v,1,5);}",
        "int v; int f(void){return __sync_fetch_and_min(&v,1);}",
        "int v; int f(void){return __atomic_fetch_add_4(&v,1,5);}",
    ] {
        // Never an implicit declaration, whatever the warning level or mode.
        refused_in(
            source,
            "E2949",
            &[&[], &["-w"], &["-std=gnu89"], &["-w", "-std=gnu89"]],
        );
    }
}

#[test]
fn has_builtin_answers_one_for_each_admitted_builtin() {
    let names = [
        "__atomic_exchange_n",
        "__atomic_compare_exchange_n",
        "__atomic_fetch_add",
        "__atomic_fetch_sub",
        "__atomic_fetch_and",
        "__atomic_fetch_or",
        "__atomic_fetch_xor",
        "__atomic_fetch_nand",
        "__atomic_add_fetch",
        "__atomic_sub_fetch",
        "__atomic_and_fetch",
        "__atomic_or_fetch",
        "__atomic_xor_fetch",
        "__atomic_nand_fetch",
        "__atomic_test_and_set",
        "__atomic_clear",
        "__atomic_thread_fence",
        "__atomic_signal_fence",
        "__atomic_always_lock_free",
        "__atomic_is_lock_free",
        "__atomic_load_n",
        "__atomic_store_n",
        "__atomic_load",
        "__atomic_store",
        "__atomic_exchange",
        "__atomic_compare_exchange",
    ];
    let mut source = String::new();
    for name in names {
        source.push_str(&format!(
            "#if !__has_builtin({name})\n#error {name}\n#endif\n"
        ));
    }
    source.push_str("#if __has_builtin(__atomic_fetch_min) || __has_builtin(__atomic_fetch_add_4)\n#error unadmitted\n#endif\nint ok;\n");
    let output = stdin(&source, &["-E"]);
    assert!(output.status.success(), "{}", text(&output));
}

#[test]
fn lock_free_queries_are_constants_of_the_target() {
    for (target, sixteen) in [(LINUX, 0), (DARWIN, 1)] {
        let source = format!(
            "_Static_assert(__atomic_always_lock_free(1,0)&&__atomic_always_lock_free(2,0)\
             &&__atomic_always_lock_free(4,0)&&__atomic_always_lock_free(8,0),\"\");\
             _Static_assert(!__atomic_always_lock_free(3,0)&&!__atomic_always_lock_free(32,0),\"\");\
             _Static_assert(__atomic_always_lock_free(16,0)=={sixteen},\"\");\
             int v; long w; int main(void){{return !__atomic_is_lock_free(4,&v)+!__atomic_is_lock_free(sizeof w,0);}}"
        );
        for mode in INTERPRETERS {
            let output = stdin(&source, &[&format!("--target={target}"), mode]);
            assert!(output.status.success(), "{}", text(&output));
            assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
        }
    }
    // An answer that needs the run-time library call is refused.
    refused(
        "char c[4]; int f(void){return __atomic_is_lock_free(4,c);}",
        "E2935",
    );
}

#[test]
fn native_code_is_inline_and_fences_select_their_barrier() {
    let source = "\
        int fadd(int *p, int v) { return __atomic_fetch_add(p, v, 5); }\n\
        unsigned char fnand(unsigned char *p, unsigned char v) { return __atomic_fetch_nand(p, v, 0); }\n\
        long xchg(long *p, long v) { return __atomic_exchange_n(p, v, 5); }\n\
        int cas(int *p, int *e, int d) { return __atomic_compare_exchange_n(p, e, d, 0, 5, 5); }\n\
        int casw(short *p, short *e, short d) { return __atomic_compare_exchange_n(p, e, d, 1, 2, 0); }\n\
        int tas(_Bool *p) { return __atomic_test_and_set(p, 5); }\n\
        void clear(_Bool *p) { __atomic_clear(p, 3); }\n\
        int *bump(int **p) { return __atomic_add_fetch(p, 4, 5); }\n\
        void fence(void) { __atomic_thread_fence(5); }\n\
        void fence_acquire(void) { __atomic_thread_fence(2); }\n\
        void signal_fence(void) { __atomic_signal_fence(5); }\n";
    for target in TARGETS {
        for level in LEVELS {
            let output = stdin(
                source,
                &[&format!("--target={target}"), level, "--emit=asm"],
            );
            assert!(output.status.success(), "{}", text(&output));
            let asm = String::from_utf8_lossy(&output.stdout);
            assert!(
                !asm.contains("bl __atomic") && !asm.contains("bl ___atomic"),
                "{asm}"
            );
            assert!(!asm.contains("__aarch64_"), "{asm}");
            assert!(asm.contains("ldaxr") && asm.contains("stlxr"), "{asm}");
            assert!(asm.contains("ldxrb") && asm.contains("stxrb"), "{asm}");
            assert!(asm.contains("dmb ishld"), "{asm}");
            assert!(asm.contains("dmb ish\n"), "{asm}");
        }
    }
}

#[test]
fn test_and_set_and_clear_execute_in_both_interpreters() {
    // CASE 5 of the probe without its fences, which the interpreters refuse.
    let source = "int main(void){_Bool f=0;unsigned char c=0;int word=0;\
        if(__atomic_test_and_set(&f,5)||!__atomic_test_and_set(&f,2))return 1;\
        __atomic_clear(&f,3);\
        if(f||__atomic_test_and_set(&c,0))return 2;\
        if(__atomic_test_and_set(&word,4)||word!=1)return 3;\
        __atomic_clear(&word,0);\
        return c!=1||word!=0;}";
    for target in TARGETS {
        for mode in INTERPRETERS {
            let output = stdin(source, &[&format!("--target={target}"), mode]);
            assert!(output.status.success(), "{}", text(&output));
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                "0",
                "{target} {mode}"
            );
        }
    }
}

#[test]
fn fences_are_ordered_barriers_that_the_interpreters_refuse() {
    let output = stdin(
        "void f(int o){__atomic_thread_fence(2);__atomic_thread_fence(5);__atomic_signal_fence(5);\
         __atomic_thread_fence(0);__atomic_signal_fence(0);__atomic_thread_fence(o);}",
        &[&format!("--target={LINUX}"), "--emit=evoir"],
    );
    assert!(output.status.success(), "{}", text(&output));
    let module = String::from_utf8_lossy(&output.stdout);
    // acquire, seq_cst, the signal fence and the run-time order (seq_cst);
    // the two relaxed fences emit nothing.
    assert_eq!(module.matches(" = asm volatile ").count(), 4, "{module}");
    assert_eq!(module.matches("\"dmb ishld\"").count(), 1, "{module}");
    assert_eq!(module.matches("\"dmb ish\"").count(), 2, "{module}");
    assert_eq!(
        module.matches(" = asm volatile \"\" ").count(),
        1,
        "{module}"
    );
    for target in TARGETS {
        for mode in INTERPRETERS {
            let output = evocc()
                .arg(format!("--target={target}"))
                .args(["-std=gnu11", mode, "-DCASE=5"])
                .arg(probe())
                .output()
                .unwrap();
            assert!(!output.status.success(), "{}", text(&output));
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3021"),
                "{}",
                text(&output)
            );
            let relaxed = stdin(
                "int main(void){__atomic_thread_fence(0);__atomic_signal_fence(0);return 0;}",
                &[&format!("--target={target}"), mode],
            );
            assert!(relaxed.status.success(), "{}", text(&relaxed));
            assert_eq!(String::from_utf8_lossy(&relaxed.stdout).trim(), "0");
        }
    }
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("evocc-l11b-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed L11b probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn compile(
    target: &str,
    level: &str,
    action: &str,
    source: &Path,
    output: &Path,
    defines: &[&str],
) {
    let result = evocc()
        .arg(format!("--target={target}"))
        .args(["-std=gnu11", level, action])
        .args(defines)
        .arg(source)
        .arg("-o")
        .arg(output)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", text(&result));
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_native_values_and_threads_agree_with_apple_clang() {
    let work = Work::new("darwin");
    for level in LEVELS {
        for case in VALUE_CASES {
            let program = work.0.join(format!("value-{case}"));
            compile(
                DARWIN,
                level,
                "-c",
                &probe(),
                &program.with_extension("o"),
                &[&format!("-DCASE={case}")],
            );
            let link = Command::new("clang")
                .arg(program.with_extension("o"))
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(link.status.success(), "{}", text(&link));
            let run = Command::new(&program).output().unwrap();
            assert_eq!(run.status.code(), Some(0), "CASE={case} {level}");
        }
        let worker = fixture("atomic-gnu-crossed-worker.c");
        compile(
            DARWIN,
            level,
            "-c",
            &worker,
            &work.0.join("evocc.o"),
            &["-DSIDE=evocc"],
        );
        let reference = Command::new("clang")
            .args(["-std=gnu11", level, "-c", "-DSIDE=clang"])
            .arg(&worker)
            .arg("-o")
            .arg(work.0.join("clang.o"))
            .output()
            .unwrap();
        assert!(reference.status.success(), "{}", text(&reference));
        for (first, second) in [("evocc", "clang"), ("clang", "evocc"), ("evocc", "evocc")] {
            let program = work.0.join(format!("threads-{first}-{second}"));
            let link = Command::new("clang")
                .args(["-std=gnu11", "-O2"])
                .arg(format!("-DFIRST={first}"))
                .arg(format!("-DSECOND={second}"))
                .arg(fixture("atomic-gnu-crossed-driver.c"))
                .arg(work.0.join("evocc.o"))
                .arg(work.0.join("clang.o"))
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(link.status.success(), "{}", text(&link));
            let run = Command::new(&program).output().unwrap();
            assert_eq!(
                run.status.code(),
                Some(0),
                "{first}/{second} {level}: {}",
                text(&run)
            );
        }
    }
}

/// Linux: the value matrix natively, and the crossed threads with GCC 14 and
/// Clang 19 in both directions, at `-O0` and `-O2`.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_values_and_threads_agree_with_gcc_14() {
    let work = Work::new("linux");
    let mut script = String::from("set -eu\ncd /work\n");
    let mut lines = 0;
    std::fs::copy(
        fixture("atomic-gnu-crossed-worker.c"),
        work.0.join("worker.c"),
    )
    .unwrap();
    std::fs::copy(
        fixture("atomic-gnu-crossed-driver.c"),
        work.0.join("driver.c"),
    )
    .unwrap();
    for level in LEVELS {
        for case in VALUE_CASES {
            let name = format!("value{level}-{case}");
            compile(
                LINUX,
                level,
                "-S",
                &probe(),
                &work.0.join(format!("{name}.s")),
                &[&format!("-DCASE={case}")],
            );
            script.push_str(&format!(
                "gcc-14 {name}.s -o {name}\n./{name}\necho '{name}: 0'\n"
            ));
            lines += 1;
        }
        let stem = format!("worker{level}");
        compile(
            LINUX,
            level,
            "-S",
            &fixture("atomic-gnu-crossed-worker.c"),
            &work.0.join(format!("{stem}.s")),
            &["-DSIDE=evocc"],
        );
        script.push_str(&format!(
            "gcc-14 -std=gnu11 {level} -c -DSIDE=gcc worker.c -o {stem}-gcc.o\n\
             clang-19 -std=gnu11 {level} -c -DSIDE=clang worker.c -o {stem}-clang.o\n"
        ));
        for (first, second) in [
            ("evocc", "gcc"),
            ("gcc", "evocc"),
            ("evocc", "clang"),
            ("clang", "evocc"),
            ("evocc", "evocc"),
        ] {
            script.push_str(&format!(
                "gcc-14 -std=gnu11 -O2 -pthread -DFIRST={first} -DSECOND={second} driver.c \
                 {stem}.s {stem}-gcc.o {stem}-clang.o -o threads\n\
                 ./threads | tail -1 | sed 's/^/{level} {first}\\/{second}: /'\n"
            ));
            lines += 1;
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
    assert!(
        !stdout.contains("failures=") || stdout.matches("failures=0").count() == 10,
        "{stdout}"
    );
    println!("{stdout}");
}
