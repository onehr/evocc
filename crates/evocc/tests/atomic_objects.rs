//! L11a: `_Atomic` objects (EvoIR 10.1.21). The layout follows GCC 14 on
//! Linux and Apple clang on Darwin. A read is a `seq_cst` atomic load and a
//! simple assignment a `seq_cst` atomic store. Compound assignment, `++` and
//! `--` are one `seq_cst` read-modify-write: a compare-exchange loop that
//! keeps the obligations of the ordinary computation. Floating updates,
//! whole-record accesses and 16-byte objects keep E2702. Both interpreters
//! run one thread, so they show values and obligations, not inter-thread
//! ordering. The crossed native tests carry the layout and ordering claims,
//! and a passing run of them is not a memory-model proof either.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
/// The operator probe's cases inside this slice; 3, 4 and 5 stay refused.
const VALUE_CASES: [u8; 6] = [0, 1, 2, 6, 7, 8];

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

fn refused_on(target: &str, source: &str, code: &str) {
    for mode in ["--emit=evoir", "--interpret-c-hir"] {
        let output = stdin(source, &[&format!("--target={target}"), "-std=gnu11", mode]);
        assert!(!output.status.success(), "{target} {mode}: {source}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("error[{code}]")),
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
        refused_on(target, source, code);
    }
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

/// Run `source` in both interpreters on `target` and return main's value.
fn executes_on(target: &str, source: &str, level: &str) -> Vec<String> {
    INTERPRETERS
        .iter()
        .map(|mode| {
            let mut args = target_args(target);
            args.extend(["-std=gnu11", level, mode].map(String::from));
            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            let output = stdin(source, &args);
            assert!(
                output.status.success(),
                "{target} {mode}: {}",
                text(&output)
            );
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        })
        .collect()
}

fn executes_to_zero(source: &str) {
    for target in TARGETS {
        for level in LEVELS {
            for value in executes_on(target, source, level) {
                assert_eq!(value, "0", "{target} {level}: {source}");
            }
        }
    }
}

fn with_case(path: &Path, define: &str, value: u8) -> String {
    format!(
        "#define {define} {value}\n{}",
        std::fs::read_to_string(path).unwrap()
    )
}

#[test]
fn the_scope_fixture_update_executes_in_both_interpreters() {
    executes_to_zero(&with_case(&fixture("atomic-scope-boundaries.c"), "CASE", 0));
}

#[test]
fn operators_on_atomic_objects_execute_in_both_interpreters() {
    for case in VALUE_CASES {
        executes_to_zero(&with_case(&probe("l11a-atomic-objects.c"), "CASE", case));
    }
}

#[test]
fn signed_overflow_and_division_keep_their_obligations_in_an_update() {
    for (source, code) in [
        (
            "int main(void){_Atomic int x=2147483647;x+=1;return 0;}",
            "E3004",
        ),
        (
            "int main(void){_Atomic int x=2147483647;x++;return 0;}",
            "E3004",
        ),
        (
            "int main(void){_Atomic int x=1;int z=0;x/=z;return 0;}",
            "E3002",
        ),
        (
            "int main(void){_Atomic int x=1;int z=40;x<<=z;return 0;}",
            "E3005",
        ),
        ("int main(void){_Atomic int x;x+=1;return 0;}", "E3001"),
        ("int main(void){_Atomic int x;return x;}", "E3001"),
    ] {
        for target in TARGETS {
            for mode in INTERPRETERS {
                let output = stdin(source, &[&format!("--target={target}"), mode]);
                assert!(!output.status.success(), "{mode} accepted {source}");
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains(code),
                    "{mode} {source}: {}",
                    text(&output)
                );
            }
        }
    }
    // A narrow signed type is promoted first, so its update has no overflow.
    executes_to_zero("int main(void){_Atomic signed char c=127;c++;c+=1;return c!=-127;}");
}

#[test]
fn floating_records_and_sixteen_byte_objects_keep_named_refusals() {
    let objects = probe("l11a-atomic-objects.c");
    // Floating compound assignment, long double and whole-record access.
    for case in [3, 4, 5] {
        refused(&with_case(&objects, "CASE", case), "E2702");
    }
    for source in [
        "_Atomic double d; void f(void){d += 1;}",
        "_Atomic double d; void f(void){d++;}",
        "_Atomic float d; void f(void){--d;}",
        "_Atomic int i; void f(void){i += 1.5;}",
        "_Atomic int i; void f(void){i *= 2.0f;}",
        "struct p {int a, b;}; _Atomic struct p s, t; void f(void){s = t;}",
        "struct p {int a, b;}; _Atomic struct p s; struct p f(void){return s;}",
        "struct p {int a, b;}; _Atomic struct p s; void g(struct p); void f(void){g(s);}",
        "struct p {int a, b;}; struct h {_Atomic struct p pair;}; \
         struct p f(struct h *x){return x->pair;}",
        "struct p {int a, b;}; struct h {_Atomic struct p pair;}; \
         void f(struct h *x, struct p v){x->pair = v;}",
        "_Atomic __int128 q; void f(void){q = 1;}",
        "_Atomic __int128 q; __int128 f(void){return q;}",
        "_Atomic _Complex float z; void f(void){z = 1;}",
    ] {
        refused(source, "E2702");
    }
    // Linux long double is 16 bytes; Darwin's is binary64.
    refused_on(
        LINUX,
        "_Atomic long double q; void f(void){q = 1;}",
        "E2702",
    );
    let darwin = executes_on(
        DARWIN,
        "_Atomic long double q = 1; int main(void){q = 2.5L; return q != 2.5L;}",
        "-O0",
    );
    assert_eq!(darwin, ["0", "0"]);
    // Floating reads and stores are admitted on both targets.
    executes_to_zero(&std::fs::read_to_string(fixture("atomic-objects-floating.c")).unwrap());
}

#[cfg(target_os = "macos")]
#[test]
fn atomic_layouts_follow_gcc_on_linux_and_apple_clang_on_darwin() {
    // size * 32 + alignment, case by case (l11a-atomic-layout.c).
    let linux = [
        33, 66, 132, 264, 264, 132, 264, 528, 528, 97, 194, 264, 161, 388, 528, 545, 33, 66,
    ];
    let darwin = [
        33, 66, 132, 264, 264, 132, 264, 264, 528, 132, 264, 264, 264, 528, 528, 545, 33, 66,
    ];
    for (target, expected) in [(LINUX, linux), (DARWIN, darwin)] {
        for (case, value) in expected.iter().enumerate() {
            let source = with_case(&probe("l11a-atomic-layout.c"), "CASE", case as u8);
            for observed in executes_on(target, &source, "-O0") {
                assert_eq!(observed, value.to_string(), "{target} CASE={case}");
            }
        }
    }
    // The extra bytes of a Darwin atomic record are padding, and an array of
    // atomic elements uses the atomic element size.
    let source = "struct c3 { char c[3]; }; _Atomic struct c3 cells[2]; \
        int main(void){return (int)sizeof cells * 100 \
            + (int)((char *)&cells[1] - (char *)&cells[0]) * 10 + (int)_Alignof(cells[0]);}";
    assert_eq!(executes_on(LINUX, source, "-O0"), ["631", "631"]);
    assert_eq!(executes_on(DARWIN, source, "-O0"), ["844", "844"]);
}

#[test]
fn atomic_constraint_violations_are_errors() {
    let negative = probe("l11a-atomic-negative.c");
    for (case, code) in [
        (0, Some("E2956")), // member of an atomic record
        (1, Some("E2955")), // _Atomic(int[3])
        (2, Some("E2955")), // _Atomic(const int)
        (3, Some("W0012")), // int * from &atomic_int
        (4, Some("E2955")), // atomic bit-field
        (5, None),          // update of an atomic array element
        (6, Some("E2955")), // _Atomic on a function typedef
    ] {
        let source = with_case(&negative, "NEG", case);
        for target in TARGETS {
            let output = stdin(&source, &[&format!("--target={target}"), "--emit=evoir"]);
            match code {
                Some(code) => {
                    assert!(!output.status.success(), "NEG={case}");
                    assert!(
                        String::from_utf8_lossy(&output.stderr).contains(&format!("[{code}]")),
                        "NEG={case}: {}",
                        text(&output)
                    );
                }
                None => assert!(output.status.success(), "NEG={case}: {}", text(&output)),
            }
        }
    }
    for source in [
        "_Atomic(_Atomic int) x;",
        "_Atomic(volatile int) x;",
        "_Atomic(void (void)) *p;",
        "typedef int pair[2]; _Atomic pair x;",
        "struct s { int a; }; _Atomic struct s *p; int f(void){return p->a;}",
        "union u { int a; }; _Atomic union u v; void f(void){v.a = 1;}",
    ] {
        let code = if source.contains("->") || source.contains("v.a") {
            "E2956"
        } else {
            "E2955"
        };
        refused(source, code);
    }
    // The specifier form names the same type as the qualifier, and an
    // unevaluated member access is allowed.
    executes_to_zero(
        "struct s { int a, b; }; _Atomic(struct s) whole; \
         _Atomic(int) counter = 4; _Atomic(int *) cursor; \
         int main(void){_Atomic int *p = &counter; cursor = 0; *p += 1; \
             return (int)sizeof(whole.a) + counter - 9 + (cursor != 0) \
                 + (_Alignof(_Atomic(struct s)) != 8);}",
    );
}

#[test]
fn typeof_of_a_value_auto_type_and_generic_see_the_non_atomic_type() {
    executes_to_zero(
        "_Atomic int a = 3; \
         int main(void){ __typeof__((void)0, a) t = a; t += 1; __auto_type u = a; u *= 2; \
             __typeof__(a) v = 4; v += 1; \
             return _Generic(a, int: 0, default: 1) + _Generic(t, int: 0, default: 2) \
                 + (t != 4) + (u != 6) + (v != 5);}",
    );
}

/// The admitted part of the code-shape probe: every function except the
/// floating, long double and record ones, which stay E2702.
fn codegen_subset() -> String {
    std::fs::read_to_string(probe("l11a-codegen.c"))
        .unwrap()
        .lines()
        .filter(|line| {
            !["add_d", "add_q", "read_s", "write_s"]
                .iter()
                .any(|name| line.contains(name))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn reads_stores_and_updates_are_seq_cst_atomic_accesses_in_ir_and_assembly() {
    let source = codegen_subset();
    for target in TARGETS {
        for level in LEVELS {
            let ir = stdin(
                &source,
                &[&format!("--target={target}"), level, "--emit=evoir"],
            );
            assert!(ir.status.success(), "{}", text(&ir));
            let ir = String::from_utf8_lossy(&ir.stdout);
            for form in [
                "atomic.load seq_cst",
                "atomic.store seq_cst",
                "atomic.rmw.cmpxchg seq_cst",
            ] {
                assert!(ir.contains(form), "{target} {level} missing {form}: {ir}");
            }
            // No ordinary access touches an atomic object.
            assert!(!ir.contains(" = load "), "{target} {level}: {ir}");
            assert!(!ir.contains(" = store "), "{target} {level}: {ir}");
            assert!(!ir.contains("atomic.load relaxed"), "{ir}");
            let asm = stdin(
                &source,
                &[&format!("--target={target}"), level, "--emit=asm"],
            );
            assert!(asm.status.success(), "{}", text(&asm));
            let asm = String::from_utf8_lossy(&asm.stdout);
            for form in ["ldar ", "stlr ", "ldaxr", "stlxr"] {
                assert!(asm.contains(form), "{target} {level} missing {form}: {asm}");
            }
            assert!(
                !asm.contains("bl __atomic") && !asm.contains("bl ___atomic"),
                "{asm}"
            );
        }
    }
    for name in ["add_d", "read_s", "write_s"] {
        let line = std::fs::read_to_string(probe("l11a-codegen.c"))
            .unwrap()
            .lines()
            .find(|line| line.contains(name))
            .unwrap()
            .to_owned();
        refused(
            &format!(
                "struct pair {{ int a, b; }}; _Atomic double d; _Atomic struct pair s;\n{line}"
            ),
            "E2702",
        );
    }
}

#[test]
fn initialization_is_not_an_atomic_event_and_volatile_atomic_is_one_event() {
    let output = stdin(
        "int main(void){_Atomic int z = 4; static _Atomic long w = 7; (void)&z; (void)&w; return 0;}",
        &[&format!("--target={LINUX}"), "--emit=evoir"],
    );
    assert!(output.status.success(), "{}", text(&output));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("atomic."));
    let output = stdin(
        "static volatile _Atomic int v; int f(void){return v;} void g(void){v = 1;}",
        &[&format!("--target={LINUX}"), "--emit=evoir"],
    );
    assert!(output.status.success(), "{}", text(&output));
    let ir = String::from_utf8_lossy(&output.stdout);
    assert!(ir.contains("atomic.load seq_cst volatile"), "{ir}");
    assert!(ir.contains("atomic.store seq_cst volatile"), "{ir}");
    assert!(
        !ir.contains("= load volatile") && !ir.contains("= store volatile"),
        "{ir}"
    );
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("evocc-l11a-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed L11a probe: {}", self.0.display());
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

/// The value probes as native programs.
fn native_value_sources() -> Vec<(String, PathBuf, Vec<String>)> {
    let mut sources = vec![(
        "scope-0".to_owned(),
        fixture("atomic-scope-boundaries.c"),
        vec!["-DCASE=0".to_owned()],
    )];
    for case in VALUE_CASES {
        sources.push((
            format!("objects-{case}"),
            probe("l11a-atomic-objects.c"),
            vec![format!("-DCASE={case}")],
        ));
    }
    sources.push((
        "floating".to_owned(),
        fixture("atomic-objects-floating.c"),
        Vec::new(),
    ));
    sources
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_native_values_layout_and_threads_agree_with_apple_clang() {
    let work = Work::new("darwin");
    let run = |program: &Path| Command::new(program).output().unwrap();
    for level in LEVELS {
        for (name, source, defines) in native_value_sources() {
            let program = work.0.join(format!("{name}{level}"));
            let defines = defines.iter().map(String::as_str).collect::<Vec<_>>();
            compile(
                DARWIN,
                level,
                "-c",
                &source,
                &program.with_extension("o"),
                &defines,
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
        let worker = fixture("atomic-objects-crossed-worker.c");
        let object = fixture("atomic-objects-crossed-object.c");
        let evocc_worker = work.0.join(format!("worker-evocc{level}.o"));
        let evocc_object = work.0.join(format!("object-evocc{level}.o"));
        compile(
            DARWIN,
            level,
            "-c",
            &worker,
            &evocc_worker,
            &["-DSIDE=evocc"],
        );
        compile(DARWIN, level, "-c", &object, &evocc_object, &[]);
        let clang_worker = work.0.join(format!("worker-clang{level}.o"));
        let clang_object = work.0.join(format!("object-clang{level}.o"));
        for (source, output, define) in [
            (&worker, &clang_worker, Some("-DSIDE=clang")),
            (&object, &clang_object, None),
        ] {
            let result = Command::new("clang")
                .args(["-std=gnu11", level, "-c"])
                .args(define)
                .arg(source)
                .arg("-o")
                .arg(output)
                .output()
                .unwrap();
            assert!(result.status.success(), "{}", text(&result));
        }
        for (first, second, object) in [
            ("evocc", "clang", &clang_object),
            ("clang", "evocc", &clang_object),
            ("evocc", "evocc", &clang_object),
            ("clang", "clang", &evocc_object),
            ("evocc", "clang", &evocc_object),
        ] {
            let program = work.0.join(format!("threads-{first}-{second}"));
            let link = Command::new("clang")
                .args(["-std=gnu11", "-O2"])
                .arg(format!("-DFIRST={first}"))
                .arg(format!("-DSECOND={second}"))
                .arg(fixture("atomic-objects-crossed-driver.c"))
                .arg(&evocc_worker)
                .arg(&clang_worker)
                .arg(object)
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(link.status.success(), "{}", text(&link));
            let output = run(&program);
            assert_eq!(
                output.status.code(),
                Some(0),
                "{first}/{second} {} {level}: {}",
                object.display(),
                text(&output)
            );
        }
    }
}

/// Linux: the value probes natively, and the crossed layout and thread test
/// with GCC 14 in both directions, at `-O0` and `-O2`.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_values_layout_and_threads_agree_with_gcc_14() {
    let work = Work::new("linux");
    let mut script = String::from("set -eu\ncd /work\n");
    let mut lines = 0;
    for name in [
        "atomic-objects-crossed.h",
        "atomic-objects-crossed-worker.c",
        "atomic-objects-crossed-object.c",
        "atomic-objects-crossed-driver.c",
    ] {
        std::fs::copy(fixture(name), work.0.join(name)).unwrap();
    }
    for level in LEVELS {
        for (name, source, defines) in native_value_sources() {
            let name = format!("{name}{level}");
            let defines = defines.iter().map(String::as_str).collect::<Vec<_>>();
            compile(
                LINUX,
                level,
                "-S",
                &source,
                &work.0.join(format!("{name}.s")),
                &defines,
            );
            script.push_str(&format!(
                "gcc-14 {name}.s -o {name} -latomic\n./{name}\necho '{name}: 0'\n"
            ));
            lines += 1;
        }
        let worker = format!("worker-evocc{level}");
        let object = format!("object-evocc{level}");
        compile(
            LINUX,
            level,
            "-S",
            &work.0.join("atomic-objects-crossed-worker.c"),
            &work.0.join(format!("{worker}.s")),
            &["-DSIDE=evocc"],
        );
        compile(
            LINUX,
            level,
            "-S",
            &work.0.join("atomic-objects-crossed-object.c"),
            &work.0.join(format!("{object}.s")),
            &[],
        );
        script.push_str(&format!(
            "gcc-14 -std=gnu11 {level} -c -DSIDE=gcc atomic-objects-crossed-worker.c -o worker-gcc{level}.o\n\
             gcc-14 -std=gnu11 {level} -c atomic-objects-crossed-object.c -o object-gcc{level}.o\n"
        ));
        for (first, second, defined) in [
            ("evocc", "gcc", "gcc"),
            ("gcc", "evocc", "gcc"),
            ("evocc", "evocc", "gcc"),
            ("gcc", "gcc", "evocc"),
            ("evocc", "gcc", "evocc"),
        ] {
            let object = if defined == "gcc" {
                format!("object-gcc{level}.o")
            } else {
                format!("{object}.s")
            };
            script.push_str(&format!(
                "gcc-14 -std=gnu11 -O2 -pthread -DFIRST={first} -DSECOND={second} \
                 atomic-objects-crossed-driver.c {worker}.s worker-gcc{level}.o {object} \
                 -o threads -latomic\n\
                 ./threads | tail -1 | sed 's/^/{level} {first}\\/{second} object={defined}: /'\n"
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
