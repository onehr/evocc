//! L11c: the generic `__atomic_load`, `__atomic_store`, `__atomic_exchange`
//! and `__atomic_compare_exchange` forms (EvoIR 10.1.14) on objects of 1,
//! 2, 4 and 8 bytes whose alignment is at least their size: records, unions
//! and scalars. The forms move the object representation as `copy` does, so
//! padding and pointer provenance survive; a compare-exchange compares every
//! byte and reports E3001 on an uninitialized one. 16-byte, misaligned and
//! other objects stay E2913. Both interpreters run one thread. The crossed
//! native tests carry the ordering claim, and a passing run of them is not a
//! memory-model proof either.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
const LEVELS: [&str; 2] = ["-O0", "-O2"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name)
}

fn probe(name: &str) -> PathBuf {
    fixture(&format!("rules-20261013/{name}"))
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

/// The target option, with the pinned sysroot on Linux for system headers.
fn target_args(target: &str) -> Vec<String> {
    let mut args = vec![format!("--target={target}")];
    if target == LINUX {
        args.push(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    args
}

/// Compile a source text from standard input.
fn stdin(source: &str, target: &str, extra: &[&str]) -> Output {
    let mut child = evocc()
        .args(target_args(target))
        .arg("-std=gnu11")
        .args(extra)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn with_case(path: &Path, define: &str, value: u8) -> String {
    format!(
        "#define {define} {value}\n{}",
        std::fs::read_to_string(path).unwrap()
    )
}

/// Run `source` in both interpreters at both levels; return main's values.
fn executes_on(target: &str, source: &str) -> Vec<String> {
    let mut values = Vec::new();
    for level in LEVELS {
        for mode in INTERPRETERS {
            let output = stdin(source, target, &[level, mode]);
            assert!(
                output.status.success(),
                "{target} {mode} {level}: {}",
                text(&output)
            );
            values.push(String::from_utf8_lossy(&output.stdout).trim().to_owned());
        }
    }
    values
}

fn executes_to_zero(source: &str) {
    for target in TARGETS {
        for value in executes_on(target, source) {
            assert_eq!(value, "0", "{target}: {source}");
        }
    }
}

/// `code` in both interpreters and at IR emission, on both targets.
fn refused_on(target: &str, source: &str, code: &str, extra: &[&str]) {
    for mode in ["--emit=evoir", "--interpret-c-hir", "--interpret-evoir"] {
        let mut args = extra.to_vec();
        args.push(mode);
        let output = stdin(source, target, &args);
        assert!(
            !output.status.success(),
            "{target} {mode} accepted: {source}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("[{code}]")),
            "{target} {mode} {source}: {stderr}"
        );
        assert!(
            output.stdout.is_empty(),
            "{source}: a refusal emitted output"
        );
    }
}

fn refused(source: &str, code: &str) {
    for target in TARGETS {
        refused_on(target, source, code, &[]);
    }
}

#[test]
fn the_scope_fixture_record_case_executes_in_both_interpreters() {
    executes_to_zero(&with_case(&fixture("atomic-scope-boundaries.c"), "CASE", 2));
}

#[test]
fn eight_byte_records_and_padding_bytes_follow_the_byte_rule() {
    // CASE 0 of the aggregate probe: all four forms on an 8-byte record.
    executes_to_zero(&with_case(&probe("l11c-aggregate.c"), "CASE", 0));
    // Equal members with different padding bytes compare unequal (CASE 0),
    // and load, store and exchange move an uninitialized padding byte
    // (CASE 2). The record is 4-aligned; the probe's CASE 4 record has
    // alignment 2 and stays refused.
    for case in [0, 2] {
        executes_to_zero(&with_case(
            &fixture("atomic-records-padding.c"),
            "CASE",
            case,
        ));
    }
    // CASE 5: the lock-free answers for 8, 16, 3 and 12 bytes.
    for (target, answer) in [(LINUX, "1"), (DARWIN, "3")] {
        for value in executes_on(target, &with_case(&probe("l11c-aggregate.c"), "CASE", 5)) {
            assert_eq!(value, answer, "{target}");
        }
    }
}

#[test]
fn every_size_and_object_kind_moves_its_representation() {
    executes_to_zero(
        "struct __attribute__((aligned(2))) two { char a, b; }; \
         struct __attribute__((aligned(4))) four { short a; char b; }; \
         union __attribute__((aligned(8))) both { long l; int i[2]; }; \
         struct one { char c; }; \
         int main(void) { \
             static struct one c1 = {1}; static struct two c2 = {2, 3}; \
             static struct four c4 = {4, 5}; static union both c8 = {.i = {6, 7}}; \
             struct one o1, n1 = {11}; struct two o2, n2 = {12, 13}; \
             struct four o4, n4 = {14, 15}; union both o8, n8 = {.i = {16, 17}}; \
             __atomic_exchange(&c1, &n1, &o1, 5); __atomic_exchange(&c2, &n2, &o2, 4); \
             __atomic_exchange(&c4, &n4, &o4, 2); __atomic_exchange(&c8, &n8, &o8, 0); \
             if (o1.c != 1 || o2.b != 3 || o4.b != 5 || o8.i[1] != 7) return 1; \
             if (c1.c != 11 || c2.a != 12 || c4.a != 14 || c8.i[0] != 16) return 2; \
             int x = 5, y, z = 9; double d = 1.5, e; int *p = &x, *q; \
             __atomic_load(&x, &y, 5); __atomic_store(&x, &z, 3); __atomic_load(&d, &e, 2); \
             __atomic_exchange(&p, &p, &q, 5); \
             if (!__atomic_compare_exchange(&x, &z, &y, 1, 5, 5) || x != 5) return 3; \
             if (__atomic_compare_exchange(&x, &z, &y, 0, 4, 2) || z != 5) return 4; \
             return y != 5 || e != 1.5 || *q != 5; }",
    );
}

#[test]
fn an_uninitialized_compared_byte_is_e3001() {
    // The padding byte of `want` is uninitialized when the compare-exchange
    // reads it.
    let source = with_case(&fixture("atomic-records-padding.c"), "CASE", 1);
    for target in TARGETS {
        for mode in INTERPRETERS {
            let output = stdin(&source, target, &[mode]);
            assert!(
                !output.status.success(),
                "{target} {mode}: {}",
                text(&output)
            );
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3001"),
                "{target} {mode}: {}",
                text(&output)
            );
        }
    }
}

#[test]
fn pointer_bytes_keep_their_provenance_through_the_forms() {
    let pointer = "#include <string.h>\n\
        struct __attribute__((aligned(8))) ref { int *p; }; static int first = 3, second = 7; \
        int main(void) { struct ref cell = {&first}, want = {&first}, next = {&second}, seen; \
            if (!__atomic_compare_exchange(&cell, &want, &next, 0, 5, 5)) return 1; \
            __atomic_load(&cell, &seen, 5); return *seen.p != 7; }";
    executes_to_zero(pointer);
    let integer = "#include <string.h>\n\
        struct __attribute__((aligned(8))) ref { int *p; }; static int second = 7; \
        int main(void) { struct ref cell, seen; unsigned long raw = (unsigned long)&second; \
            memcpy(&cell, &raw, sizeof raw); __atomic_load(&cell, &seen, 5); return *seen.p; }";
    for target in TARGETS {
        for mode in INTERPRETERS {
            let output = stdin(integer, target, &[mode]);
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3013"),
                "{target} {mode}: {}",
                text(&output)
            );
        }
    }
}

#[test]
fn sixteen_byte_misaligned_and_other_shapes_stay_refused_in_every_mode() {
    let aggregate = probe("l11c-aggregate.c");
    let misaligned = probe("l11c-misaligned.c");
    let mut sources = vec![
        with_case(&aggregate, "CASE", 1), // 16 bytes
        with_case(&aggregate, "CASE", 2), // 3 bytes
        with_case(&aggregate, "CASE", 3), // 12 bytes
        with_case(&aggregate, "CASE", 4), // 4 bytes at alignment 2
        with_case(&misaligned, "CASE", 0),
        with_case(&misaligned, "CASE", 1),
        with_case(&fixture("atomic-cas-boundaries.c"), "CASE", 2), // 16 bytes
    ];
    for source in [
        "int a[2], b[2]; void f(void){__atomic_load(&a, &b, 5);}",
        "int x; long y; void f(void){__atomic_load(&x, &y, 5);}",
        "const int x = 1; int y; void f(void){__atomic_store(&x, &y, 5);}",
        "int x; const int y = 1; void f(void){__atomic_load(&x, &y, 5);}",
        "int x, y; const int e = 1; int f(void){return __atomic_compare_exchange(&x, &e, &y, 0, 5, 5);}",
        "__int128 x, y; void f(void){__atomic_load(&x, &y, 5);}",
        "struct s { char c[3]; } x, y; void f(void){__atomic_store(&x, &y, 5);}",
        "void (*g)(void); int x; void f(void){__atomic_load(&g, &x, 5);}",
    ] {
        sources.push(source.to_owned());
    }
    for source in sources {
        for target in TARGETS {
            // Never an implicit call, at any warning level or language mode.
            for extra in [&[][..], &["-w"], &["-std=gnu89"], &["-w", "-std=gnu89"]] {
                let asm = stdin(&source, target, &[&["--emit=asm"][..], extra].concat());
                assert!(!asm.status.success(), "{target} {extra:?}: {source}");
                assert!(
                    !String::from_utf8_lossy(&asm.stdout).contains("bl __atomic")
                        && !String::from_utf8_lossy(&asm.stdout).contains("bl ___atomic"),
                    "{source}"
                );
                refused_on(target, &source, "E2913", extra);
            }
        }
    }
    // Linux `long double` is 16 bytes; Darwin's is binary64 and admitted.
    let quad = "long double x, y; void f(void){__atomic_load(&x, &y, 5);}";
    refused_on(LINUX, quad, "E2913", &[]);
    assert!(stdin(quad, DARWIN, &["--emit=evoir"]).status.success());
    // Orders follow the operation, as for the scalar forms.
    for source in [
        "int x, y; void f(void){__atomic_load(&x, &y, 3);}",
        "int x, y; void f(void){__atomic_store(&x, &y, 2);}",
        "int x, y, z; int f(void){return __atomic_compare_exchange(&x, &y, &z, 0, 2, 5);}",
    ] {
        refused(source, "E2947");
    }
}

#[test]
fn has_builtin_answers_one_for_the_generic_forms() {
    let source = "#if !__has_builtin(__atomic_load) || !__has_builtin(__atomic_store) \
        || !__has_builtin(__atomic_exchange) || !__has_builtin(__atomic_compare_exchange)\n\
        #error missing\n#endif\nint ok;\n";
    let output = stdin(source, LINUX, &["-E"]);
    assert!(output.status.success(), "{}", text(&output));
}

#[test]
fn the_linux_stdatomic_header_executes_in_both_interpreters() {
    // L11a and L11c together: GCC's <stdatomic.h> of the sysroot uses the
    // generic forms on `_Atomic` scalars, `__auto_type` and `__typeof__`.
    // The interpreters refuse the probe's two ordered fences (E3021, as
    // L11b specifies), so they run it without those lines; the sealed lane
    // runs the complete probe natively.
    let complete = std::fs::read_to_string(probe("l11a-stdatomic.c")).unwrap();
    let output = stdin(&complete, LINUX, &["--interpret-c-hir"]);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("E3021"),
        "{}",
        text(&output)
    );
    let source = complete
        .lines()
        .filter(|line| !line.contains("_fence("))
        .collect::<Vec<_>>()
        .join("\n");
    for value in executes_on(LINUX, &source) {
        assert_eq!(value, "0");
    }
}

#[test]
fn native_code_uses_one_atomic_access_of_the_whole_object() {
    let source = "struct __attribute__((aligned(8))) pair { unsigned a, b; }; \
        void load(struct pair *p, struct pair *r) { __atomic_load(p, r, 2); } \
        void store(struct pair *p, struct pair *v) { __atomic_store(p, v, 3); } \
        void xchg(struct pair *p, struct pair *v, struct pair *r) { __atomic_exchange(p, v, r, 5); } \
        int cas(struct pair *p, struct pair *e, struct pair *d) \
        { return __atomic_compare_exchange(p, e, d, 0, 5, 5); }";
    for target in TARGETS {
        for level in LEVELS {
            let ir = stdin(source, target, &[level, "--emit=evoir"]);
            assert!(ir.status.success(), "{}", text(&ir));
            let ir = String::from_utf8_lossy(&ir.stdout);
            for form in [
                "atomic.copy.load acquire",
                "atomic.copy.store release",
                "atomic.copy.exchange seq_cst",
                "atomic.copy.cmpxchg seq_cst",
            ] {
                assert!(ir.contains(form), "{target} {level} missing {form}: {ir}");
            }
            let asm = stdin(source, target, &[level, "--emit=asm"]);
            assert!(asm.status.success(), "{}", text(&asm));
            let asm = String::from_utf8_lossy(&asm.stdout);
            for form in ["ldar x", "stlr x", "ldaxr x", "stlxr"] {
                assert!(asm.contains(form), "{target} {level} missing {form}: {asm}");
            }
            assert!(
                !asm.contains("bl __atomic") && !asm.contains("bl ___atomic"),
                "{asm}"
            );
            assert!(!asm.contains("__aarch64_"), "{asm}");
        }
    }
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("evocc-l11c-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed L11c probe: {}", self.0.display());
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
        .args(target_args(target))
        .args(["-std=gnu11", level, action])
        .args(defines)
        .arg(source)
        .arg("-o")
        .arg(output)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", text(&result));
}

/// The value probes as native programs.
fn native_value_sources() -> Vec<(String, PathBuf, String)> {
    vec![
        (
            "scope-2".into(),
            fixture("atomic-scope-boundaries.c"),
            "-DCASE=2".into(),
        ),
        (
            "aggregate-0".into(),
            probe("l11c-aggregate.c"),
            "-DCASE=0".into(),
        ),
        (
            "padding-0".into(),
            fixture("atomic-records-padding.c"),
            "-DCASE=0".into(),
        ),
        (
            "padding-2".into(),
            fixture("atomic-records-padding.c"),
            "-DCASE=2".into(),
        ),
    ]
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_native_values_and_threads_agree_with_apple_clang() {
    let work = Work::new("darwin");
    let run = |program: &Path| Command::new(program).output().unwrap();
    for level in LEVELS {
        for (name, source, define) in native_value_sources() {
            let program = work.0.join(format!("{name}{level}"));
            compile(
                DARWIN,
                level,
                "-c",
                &source,
                &program.with_extension("o"),
                &[&define],
            );
            let link = Command::new("clang")
                .arg(program.with_extension("o"))
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(link.status.success(), "{}", text(&link));
            assert_eq!(run(&program).status.code(), Some(0), "{name} {level}");
        }
        let worker = fixture("atomic-records-crossed-worker.c");
        let evocc_worker = work.0.join(format!("worker-evocc{level}.o"));
        compile(
            DARWIN,
            level,
            "-c",
            &worker,
            &evocc_worker,
            &["-DSIDE=evocc"],
        );
        let clang_worker = work.0.join(format!("worker-clang{level}.o"));
        let result = Command::new("clang")
            .args(["-std=gnu11", level, "-c", "-DSIDE=clang"])
            .arg(&worker)
            .arg("-o")
            .arg(&clang_worker)
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", text(&result));
        for (first, second) in [("evocc", "clang"), ("clang", "evocc"), ("evocc", "evocc")] {
            let program = work.0.join(format!("threads-{first}-{second}"));
            let link = Command::new("clang")
                .args(["-std=gnu11", "-O2"])
                .arg(format!("-DFIRST={first}"))
                .arg(format!("-DSECOND={second}"))
                .arg(fixture("atomic-records-crossed-driver.c"))
                .arg(&evocc_worker)
                .arg(&clang_worker)
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(link.status.success(), "{}", text(&link));
            let output = run(&program);
            assert_eq!(
                output.status.code(),
                Some(0),
                "{first}/{second} {level}: {}",
                text(&output)
            );
        }
    }
}

/// Linux: the value probes natively, and the crossed threads with GCC 14 and
/// Clang 19 in both directions, at `-O0` and `-O2`.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_values_and_threads_agree_with_gcc_14_and_clang_19() {
    let work = Work::new("linux");
    let mut script = String::from("set -eu\ncd /work\n");
    let mut lines = 0;
    for name in [
        "atomic-records-crossed.h",
        "atomic-records-crossed-worker.c",
        "atomic-records-crossed-driver.c",
    ] {
        std::fs::copy(fixture(name), work.0.join(name)).unwrap();
    }
    for level in LEVELS {
        let mut sources = native_value_sources();
        // The complete header probe, fences included; Darwin's header needs
        // Clang's `__c11_atomic_*` builtins, which EvoCC does not provide.
        sources.push((
            "stdatomic".into(),
            probe("l11a-stdatomic.c"),
            "-DSTDATOMIC=1".into(),
        ));
        for (name, source, define) in sources {
            let name = format!("{name}{level}");
            compile(
                LINUX,
                level,
                "-S",
                &source,
                &work.0.join(format!("{name}.s")),
                &[&define],
            );
            script.push_str(&format!(
                "gcc-14 {name}.s -o {name}\n./{name}\necho '{name}: 0'\n"
            ));
            lines += 1;
        }
        let worker = format!("worker-evocc{level}");
        compile(
            LINUX,
            level,
            "-S",
            &work.0.join("atomic-records-crossed-worker.c"),
            &work.0.join(format!("{worker}.s")),
            &["-DSIDE=evocc"],
        );
        script.push_str(&format!(
            "gcc-14 -std=gnu11 {level} -c -DSIDE=gcc atomic-records-crossed-worker.c -o worker-gcc{level}.o\n\
             clang-19 -std=gnu11 {level} -c -DSIDE=clang atomic-records-crossed-worker.c -o worker-clang{level}.o\n"
        ));
        for (first, second) in [
            ("evocc", "gcc"),
            ("gcc", "evocc"),
            ("evocc", "clang"),
            ("clang", "evocc"),
            ("evocc", "evocc"),
        ] {
            script.push_str(&format!(
                "gcc-14 -std=gnu11 -O2 -pthread -DFIRST={first} -DSECOND={second} \
                 atomic-records-crossed-driver.c {worker}.s worker-gcc{level}.o worker-clang{level}.o \
                 -o threads -latomic\n\
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
    assert_eq!(stdout.matches("failures=0").count(), 10, "{stdout}");
    println!("{stdout}");
}
