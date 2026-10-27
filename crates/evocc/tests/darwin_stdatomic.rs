//! L11a on Darwin: the SDK's `<stdatomic.h>` is Clang's resource header, so
//! it needs Clang's `__c11_atomic_*` builtins and `__CLANG_ATOMIC_*` macros
//! (EvoIR 10.1.21). EvoCC provides exactly the ones that the header uses,
//! with Apple clang's semantics, on the atomic forms of EvoIR 3.8. Linux keeps
//! GCC's header and has neither. Both interpreters run one thread, so they
//! show values, obligations and the order of atomic events, not
//! inter-thread ordering. The crossed native tests with Apple clang carry the
//! ordering claim, and a passing run of them is not a memory-model proof
//! either.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
#[cfg(target_os = "macos")]
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
#[cfg(target_os = "macos")]
const LEVELS: [&str; 2] = ["-O0", "-O2"];
#[cfg(target_os = "macos")]
const STANDARDS: [&str; 2] = ["-std=gnu11", "-std=gnu17"];
/// The cases of the macro fixture; CASE 8 reaches ordered fences.
#[cfg(target_os = "macos")]
const CASES: [u8; 9] = [0, 1, 2, 3, 4, 5, 6, 7, 8];
#[cfg(target_os = "macos")]
const INTERPRETED_CASES: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
/// The `__c11_atomic_*` builtins that Clang's `<stdatomic.h>` uses.
const ADMITTED: [&str; 14] = [
    "__c11_atomic_init",
    "__c11_atomic_thread_fence",
    "__c11_atomic_signal_fence",
    "__c11_atomic_is_lock_free",
    "__c11_atomic_store",
    "__c11_atomic_load",
    "__c11_atomic_exchange",
    "__c11_atomic_compare_exchange_strong",
    "__c11_atomic_compare_exchange_weak",
    "__c11_atomic_fetch_add",
    "__c11_atomic_fetch_sub",
    "__c11_atomic_fetch_and",
    "__c11_atomic_fetch_or",
    "__c11_atomic_fetch_xor",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name)
}

fn probe() -> PathBuf {
    fixture("rules-20261013/l11a-stdatomic.c")
}

#[cfg(target_os = "macos")]
fn macros() -> PathBuf {
    fixture("darwin-stdatomic-macros.c")
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

#[cfg(target_os = "macos")]
fn with_case(path: &Path, value: u8) -> String {
    format!(
        "#define CASE {value}\n{}",
        std::fs::read_to_string(path).unwrap()
    )
}

/// The probe without the lines of its two ordered fences, which both
/// interpreters refuse (E3021).
fn probe_without_fences() -> String {
    std::fs::read_to_string(probe())
        .unwrap()
        .lines()
        .filter(|line| !line.contains("_fence("))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `main`'s value in both interpreters on Darwin, at both levels.
#[cfg(target_os = "macos")]
fn interpreted(source: &str) -> Vec<String> {
    let mut values = Vec::new();
    for level in LEVELS {
        for mode in INTERPRETERS {
            let output = stdin(source, DARWIN, &["-std=gnu11", level, mode]);
            assert!(output.status.success(), "{mode} {level}: {}", text(&output));
            values.push(String::from_utf8_lossy(&output.stdout).trim().to_owned());
        }
    }
    values
}

/// `code` at IR emission and in both interpreters on `target`.
fn refused_on(target: &str, source: &str, code: &str) {
    for mode in ["--emit=evoir", "--interpret-c-hir", "--interpret-evoir"] {
        for extra in [&[][..], &["-w"]] {
            let mut args = vec!["-std=gnu11", mode];
            args.extend_from_slice(extra);
            let output = stdin(source, target, &args);
            assert!(
                !output.status.success(),
                "{target} {mode} accepted: {source}"
            );
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
}

fn refused(source: &str, code: &str) {
    refused_on(DARWIN, source, code);
}

fn accepted(source: &str) {
    let output = stdin(source, DARWIN, &["-std=gnu11", "--emit=evoir"]);
    assert!(output.status.success(), "{source}: {}", text(&output));
}

#[cfg(target_os = "macos")]
#[test]
fn the_header_compiles_at_both_levels_and_in_both_language_modes() {
    let mut sources = vec![std::fs::read_to_string(probe()).unwrap()];
    sources.extend(CASES.iter().map(|case| with_case(&macros(), *case)));
    for source in &sources {
        for standard in STANDARDS {
            for level in LEVELS {
                let asm = stdin(source, DARWIN, &[standard, level, "--emit=asm"]);
                assert!(asm.status.success(), "{standard} {level}: {}", text(&asm));
                let asm = String::from_utf8_lossy(&asm.stdout);
                // Every operation is inline: no library call names an atomic.
                for call in ["bl ___atomic", "bl _atomic_", "bl ___c11"] {
                    assert!(!asm.contains(call), "{standard} {level} {call}: {asm}");
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn the_probe_executes_in_both_interpreters_without_its_fences() {
    // The complete probe reaches an ordered fence: the barrier assembly that
    // both interpreters refuse, as on Linux (EvoIR 10.1.14).
    let complete = std::fs::read_to_string(probe()).unwrap();
    for mode in INTERPRETERS {
        let output = stdin(&complete, DARWIN, &["-std=gnu11", mode]);
        assert!(!output.status.success(), "{mode}: {}", text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3021"),
            "{mode}: {}",
            text(&output)
        );
    }
    for value in interpreted(&probe_without_fences()) {
        assert_eq!(value, "0");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn every_macro_and_generic_function_executes_in_both_interpreters() {
    for case in INTERPRETED_CASES {
        for value in interpreted(&with_case(&macros(), case)) {
            assert_eq!(value, "0", "CASE={case}");
        }
    }
    // Ordered fences are the refused barrier; relaxed ones run (CASE 7).
    let fences = with_case(&macros(), 8);
    for mode in INTERPRETERS {
        let output = stdin(&fences, DARWIN, &["-std=gnu11", mode]);
        assert!(!output.status.success(), "{mode}: {}", text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3021"),
            "{mode}: {}",
            text(&output)
        );
    }
}

/// Whether Apple clang 21 diagnoses an order constant with
/// `-Watomic-memory-ordering`, as measured for every constant from -1 to 7
/// and 65536: a load diagnoses release and acq_rel, a store consume, acquire
/// and acq_rel, a failure order release and acq_rel, and every form a
/// constant outside 0..5. A failure order may be stronger than the success
/// order. A fence is never diagnosed.
fn clang_diagnoses(form: &str, order: i64) -> bool {
    form != "fence"
        && (!(0..=5).contains(&order)
            || match form {
                "load" | "failure" => matches!(order, 3 | 4),
                "store" => matches!(order, 1 | 2 | 4),
                _ => false,
            })
}

/// EvoCC's E2947 set: Apple clang's, and a fence constant outside 0..5, as
/// for the GNU fences.
fn evocc_refuses(form: &str, order: i64) -> bool {
    clang_diagnoses(form, order) || (form == "fence" && !(0..=5).contains(&order))
}

fn order_forms() -> Vec<(&'static str, &'static str)> {
    vec![
        ("load", "(void)__c11_atomic_load(&a, O)"),
        ("store", "__c11_atomic_store(&a, 1, O)"),
        ("exchange", "(void)__c11_atomic_exchange(&a, 1, O)"),
        ("fetch", "(void)__c11_atomic_fetch_add(&a, 1, O)"),
        ("fetch", "(void)__c11_atomic_fetch_sub(&a, 1, O)"),
        ("fetch", "(void)__c11_atomic_fetch_and(&a, 1, O)"),
        ("fetch", "(void)__c11_atomic_fetch_or(&a, 1, O)"),
        ("fetch", "(void)__c11_atomic_fetch_xor(&a, 1, O)"),
        ("fetch", "(void)__c11_atomic_fetch_add(&p, 1, O)"),
        (
            "success",
            "(void)__c11_atomic_compare_exchange_strong(&a, &e, 1, O, 0)",
        ),
        (
            "success",
            "(void)__c11_atomic_compare_exchange_weak(&a, &e, 1, O, 0)",
        ),
        (
            "failure",
            "(void)__c11_atomic_compare_exchange_strong(&a, &e, 1, 5, O)",
        ),
        (
            "failure",
            "(void)__c11_atomic_compare_exchange_weak(&a, &e, 1, 0, O)",
        ),
        ("fence", "__c11_atomic_thread_fence(O)"),
        ("fence", "__c11_atomic_signal_fence(O)"),
    ]
}

fn order_source(body: &str, order: i64) -> String {
    format!(
        "_Atomic int a; _Atomic(int *) p; int f(void) {{ int e = 0; {}; return e; }}\n",
        body.replace('O', &format!("({order})"))
    )
}

const ORDERS: [i64; 10] = [-1, 0, 1, 2, 3, 4, 5, 6, 7, 65536];

#[test]
fn constant_orders_follow_the_apple_clang_warning_set() {
    for (form, body) in order_forms() {
        for order in ORDERS {
            let source = order_source(body, order);
            if evocc_refuses(form, order) {
                refused(&source, "E2947");
            } else {
                accepted(&source);
            }
        }
    }
    // A run-time order is evaluated once and makes the operation seq_cst.
    let output = stdin(
        "_Atomic int a; int f(int o){ int e = 0; __c11_atomic_store(&a, 1, o); \
         return __c11_atomic_load(&a, o) + __c11_atomic_compare_exchange_strong(&a, &e, 2, o, o); }",
        DARWIN,
        &["--emit=evoir"],
    );
    assert!(output.status.success(), "{}", text(&output));
    let ir = String::from_utf8_lossy(&output.stdout);
    for form in [
        "atomic.store seq_cst",
        "atomic.load seq_cst",
        "atomic.rmw.cmpxchg seq_cst",
    ] {
        assert!(ir.contains(form), "missing {form}: {ir}");
    }
}

/// Apple clang itself, for every form and constant above: it warns exactly
/// where `clang_diagnoses` says.
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn the_order_table_is_the_one_apple_clang_applies() {
    let work = Work::new("orders");
    for (form, body) in order_forms() {
        for order in ORDERS {
            let path = work.0.join("order.c");
            std::fs::write(&path, order_source(body, order)).unwrap();
            let output = Command::new("clang")
                .args(["-std=gnu11", "-c", "-o", "/dev/null"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", text(&output));
            let warned =
                String::from_utf8_lossy(&output.stderr).contains("[-Watomic-memory-ordering]");
            assert_eq!(
                warned,
                clang_diagnoses(form, order),
                "{form} order {order}: {}",
                text(&output)
            );
        }
    }
}

#[test]
fn compare_exchange_carries_the_stronger_of_its_two_orders() {
    for (success, failure, merged) in [
        (0, 5, "seq_cst"),
        (0, 2, "acquire"),
        (0, 1, "consume"),
        (1, 2, "acquire"),
        (3, 0, "release"),
        (3, 2, "acq_rel"),
        (3, 5, "seq_cst"),
        (4, 5, "seq_cst"),
        (2, 0, "acquire"),
        (5, 0, "seq_cst"),
    ] {
        let source = format!(
            "_Atomic int a; int f(int *e){{ return __c11_atomic_compare_exchange_weak(&a, e, 1, {success}, {failure}); }}"
        );
        let output = stdin(&source, DARWIN, &["--emit=evoir"]);
        assert!(output.status.success(), "{}", text(&output));
        let ir = String::from_utf8_lossy(&output.stdout);
        assert!(
            ir.contains(&format!("atomic.rmw.cmpxchg {merged} ")),
            "{success}/{failure} is not {merged}: {ir}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn ir_and_assembly_use_the_existing_atomic_forms() {
    let source = "#include <stdatomic.h>\n\
        atomic_int i; _Atomic(int *) p; _Atomic(double) d; atomic_flag f;\n\
        void init(void) { atomic_init(&i, 3); }\n\
        int load(void) { return atomic_load_explicit(&i, memory_order_acquire); }\n\
        void store(int v) { atomic_store_explicit(&i, v, memory_order_release); }\n\
        int *bump(void) { return atomic_fetch_add(&p, 3); }\n\
        int *drop(void) { return atomic_fetch_sub_explicit(&p, 1, memory_order_relaxed); }\n\
        double swap(double v) { return atomic_exchange(&d, v); }\n\
        int set(void) { return atomic_flag_test_and_set(&f); }\n\
        void clear(void) { atomic_flag_clear_explicit(&f, memory_order_release); }\n\
        int mask(int v) { return atomic_fetch_and_explicit(&i, v, memory_order_acq_rel); }\n";
    for level in LEVELS {
        let ir = stdin(source, DARWIN, &[level, "--emit=evoir"]);
        assert!(ir.status.success(), "{}", text(&ir));
        let ir = String::from_utf8_lossy(&ir.stdout);
        for form in [
            // atomic_init is a relaxed store, as GCC's atomic_init is.
            "atomic.store relaxed",
            "atomic.load acquire",
            "atomic.store release",
            "atomic.rmw.add seq_cst",
            "atomic.rmw.sub relaxed",
            "atomic.rmw.exchange seq_cst",
            "atomic.rmw.and acq_rel",
            "fp.to_bits",
            "fp.from_bits",
        ] {
            assert!(ir.contains(form), "{level} missing {form}: {ir}");
        }
        let asm = stdin(source, DARWIN, &[level, "--emit=asm"]);
        assert!(asm.status.success(), "{}", text(&asm));
        let asm = String::from_utf8_lossy(&asm.stdout);
        for form in ["ldar w", "stlr w", "ldaxr", "stlxr"] {
            assert!(asm.contains(form), "{level} missing {form}: {asm}");
        }
        assert!(
            !asm.contains("bl ___atomic") && !asm.contains("bl _atomic_"),
            "{asm}"
        );
    }
}

#[test]
fn shapes_outside_the_admitted_domain_keep_named_refusals() {
    for source in [
        // The object must be _Atomic.
        "int x; int f(void){return __c11_atomic_load(&x, 5);}",
        "int x; void f(void){__c11_atomic_init(&x, 1);}",
        "int x; int f(void){return __c11_atomic_fetch_add(&x, 1, 5);}",
        // Only a load reads a const object.
        "const _Atomic int c = 1; void f(void){__c11_atomic_store(&c, 1, 5);}",
        "const _Atomic int c = 1; void f(void){__c11_atomic_init(&c, 1);}",
        "const _Atomic int c = 1; int f(void){return __c11_atomic_exchange(&c, 1, 5);}",
        "const _Atomic int c = 1; int f(void){return __c11_atomic_fetch_add(&c, 1, 5);}",
        // Arithmetic needs an integer other than _Bool, or an object pointer.
        "_Atomic _Bool b; int f(void){return __c11_atomic_fetch_add(&b, 1, 5);}",
        "_Atomic _Bool b; int f(void){return __c11_atomic_fetch_or(&b, 1, 5);}",
        "_Atomic float r; int f(void){return __c11_atomic_fetch_add(&r, 1.0f, 5) != 0;}",
        "_Atomic double r; int f(void){return __c11_atomic_fetch_sub(&r, 1.0, 5) != 0;}",
        "_Atomic double r; int f(void){return __c11_atomic_fetch_and(&r, 1, 5) != 0;}",
        "_Atomic(int *) p; int f(void){return __c11_atomic_fetch_or(&p, 1, 5) != 0;}",
        "_Atomic(void *) p; int f(void){return __c11_atomic_fetch_add(&p, 1, 5) != 0;}",
        "_Atomic(void (*)(void)) p; int f(void){return __c11_atomic_fetch_add(&p, 1, 5) != 0;}",
        "struct s; _Atomic(struct s *) p; int f(void){return __c11_atomic_fetch_add(&p, 1, 5) != 0;}",
        "_Atomic(int *) p; int f(void){return __c11_atomic_fetch_add(&p, (int *)0, 5) != 0;}",
        // Records, unions, complex and 16-byte objects.
        "struct s { int a, b; }; _Atomic struct s r; int f(void){return __c11_atomic_load(&r, 5).a;}",
        "union u { int a; }; _Atomic union u r; union u v; void f(void){__c11_atomic_store(&r, v, 5);}",
        "_Atomic _Complex float z; void f(void){(void)__c11_atomic_load(&z, 5);}",
        "_Atomic __int128 q; int f(void){return __c11_atomic_load(&q, 5) != 0;}",
        "_Atomic unsigned __int128 q; void f(void){__c11_atomic_store(&q, 1, 5);}",
        // The expected object is an unqualified object of the value type.
        "_Atomic int a; const int e = 1; int f(void){return __c11_atomic_compare_exchange_strong(&a, &e, 2, 5, 5);}",
        "_Atomic int a; volatile int e; int f(void){return __c11_atomic_compare_exchange_strong(&a, &e, 2, 5, 5);}",
        "_Atomic int a; long e; int f(void){return __c11_atomic_compare_exchange_weak(&a, &e, 2, 5, 5);}",
        "_Atomic int a; int f(void){return __c11_atomic_compare_exchange_strong(&a, 0, 2, 5, 5);}",
        // A value that does not convert to the object type.
        "_Atomic(int *) p; void f(void){__c11_atomic_store(&p, 1, 5);}",
        "struct s { int a; } v; _Atomic int a; void f(void){__c11_atomic_store(&a, v, 5);}",
        // The lock-free query takes an integer constant size.
        "int f(int n){return __c11_atomic_is_lock_free(n);}",
        "_Atomic int a; int f(void){return __c11_atomic_is_lock_free(&a);}",
        "_Atomic int a; int f(void){return __c11_atomic_load(&a, 1.5);}",
    ] {
        refused(source, "E2913");
    }
    // A size that is not lock-free needs the run-time library call that
    // Apple clang emits.
    for size in ["3", "0", "32", "12"] {
        refused(
            &format!("int f(void){{return __c11_atomic_is_lock_free({size});}}"),
            "E2935",
        );
    }
    // Clang's other `__c11_atomic_*` spellings are not provided, and are
    // never an implicit declaration.
    for name in [
        "__c11_atomic_fetch_nand",
        "__c11_atomic_fetch_max",
        "__c11_atomic_fetch_min",
        "__c11_atomic_unknown",
    ] {
        refused(
            &format!("_Atomic int a; int f(void){{return {name}(&a, 1, 5);}}"),
            "E2949",
        );
    }
    for source in [
        "_Atomic int a; int f(void){return __c11_atomic_load(&a);}",
        "_Atomic int a; int f(void){return __c11_atomic_fetch_add(&a, 1, 5, 5);}",
        "_Atomic int a; void f(void){__c11_atomic_init(&a);}",
        "int f(void){return __c11_atomic_is_lock_free(4, 0);}",
        "void f(void){__c11_atomic_thread_fence();}",
    ] {
        refused(source, "E2909");
    }
    refused("int (*f(void))(void){return __c11_atomic_load;}", "E2910");
}

#[test]
fn has_builtin_answers_for_the_provided_builtins_on_darwin_only() {
    let mut source = String::new();
    for name in ADMITTED {
        source.push_str(&format!(
            "#if __has_builtin({name}) != TARGET_HAS\n#error {name}\n#endif\n"
        ));
    }
    source.push_str(
        "#if __has_builtin(__c11_atomic_fetch_nand) || __has_builtin(__c11_atomic_fetch_max)\n\
         #error unadmitted\n#endif\n\
         #if defined(__CLANG_ATOMIC_INT_LOCK_FREE) != TARGET_HAS\n#error macro\n#endif\nint ok;\n",
    );
    for (target, has) in [(LINUX, "0"), (DARWIN, "1")] {
        let output = stdin(&source, target, &["-E", &format!("-DTARGET_HAS={has}")]);
        assert!(output.status.success(), "{target}: {}", text(&output));
    }
    // Each lock-free macro that the header reads is 2, as Apple clang's.
    let mut source = String::new();
    for name in [
        "BOOL", "CHAR", "CHAR16_T", "CHAR32_T", "WCHAR_T", "SHORT", "INT", "LONG", "LLONG",
        "POINTER",
    ] {
        source.push_str(&format!(
            "#if __CLANG_ATOMIC_{name}_LOCK_FREE != 2\n#error {name}\n#endif\n"
        ));
    }
    source.push_str(
        "#ifdef __CLANG_ATOMIC_CHAR8_T_LOCK_FREE\n#error char8_t is C23\n#endif\nint ok;\n",
    );
    let output = stdin(&source, DARWIN, &["-E"]);
    assert!(output.status.success(), "{}", text(&output));
}

#[test]
fn linux_keeps_the_gcc_header_and_no_c11_builtin() {
    // GCC 14 has no `__c11_atomic_*` builtin, so on Linux the name is an
    // undeclared function, as before.
    let output = stdin(
        "_Atomic int x; int f(void){return __c11_atomic_load(&x, 5);}",
        LINUX,
        &["--emit=evoir"],
    );
    assert!(!output.status.success(), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("error[W0003]"),
        "{}",
        text(&output)
    );
    // The Linux header still uses GCC's generic forms.
    let output = stdin(
        &probe_without_fences(),
        LINUX,
        &["-std=gnu11", "--emit=evoir"],
    );
    assert!(output.status.success(), "{}", text(&output));
    let ir = String::from_utf8_lossy(&output.stdout);
    assert!(ir.contains("atomic.copy.load seq_cst"), "{ir}");
    for target in TARGETS {
        let output = stdin(
            "#ifdef __clang__\n#error identity\n#endif\nint ok;\n",
            target,
            &["-E"],
        );
        assert!(output.status.success(), "{target}: {}", text(&output));
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
struct Work(PathBuf);
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-darwin-stdatomic-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed L11a probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn compile(level: &str, source: &Path, output: &Path, defines: &[&str]) {
    let result = evocc()
        .arg(format!("--target={DARWIN}"))
        .args(["-std=gnu11", level, "-c"])
        .args(defines)
        .arg(source)
        .arg("-o")
        .arg(output)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", text(&result));
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn clang(arguments: &[&str], inputs: &[&Path], output: &Path) {
    let result = Command::new("clang")
        .args(arguments)
        .args(inputs)
        .arg("-o")
        .arg(output)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", text(&result));
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_native_values_agree_with_apple_clang() {
    let work = Work::new("values");
    let mut sources = vec![("probe".to_owned(), probe(), Vec::new())];
    for case in CASES {
        sources.push((
            format!("case{case}"),
            macros(),
            vec![format!("-DCASE={case}")],
        ));
    }
    for level in LEVELS {
        for (name, source, defines) in &sources {
            let defines = defines.iter().map(String::as_str).collect::<Vec<_>>();
            // Apple clang's build gives the expected status.
            let reference = work.0.join(format!("{name}-clang{level}"));
            let mut arguments = vec!["-std=gnu11", level];
            arguments.extend(&defines);
            clang(&arguments, &[source], &reference);
            let expected = Command::new(&reference).output().unwrap();
            assert_eq!(expected.status.code(), Some(0), "clang {name} {level}");
            let program = work.0.join(format!("{name}-evocc{level}"));
            compile(level, source, &program.with_extension("o"), &defines);
            clang(&[], &[&program.with_extension("o")], &program);
            let run = Command::new(&program).output().unwrap();
            assert_eq!(
                run.status.code(),
                Some(0),
                "evocc {name} {level}: {}",
                text(&run)
            );
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_crossed_threads_agree_with_apple_clang_in_both_directions() {
    let work = Work::new("threads");
    let worker = fixture("darwin-stdatomic-crossed-worker.c");
    let driver = fixture("darwin-stdatomic-crossed-driver.c");
    for level in LEVELS {
        let evocc_worker = work.0.join(format!("worker-evocc{level}.o"));
        compile(level, &worker, &evocc_worker, &["-DSIDE=evocc"]);
        let clang_worker = work.0.join(format!("worker-clang{level}.o"));
        clang(
            &["-std=gnu11", level, "-c", "-DSIDE=clang"],
            &[&worker],
            &clang_worker,
        );
        // The driver's own <stdatomic.h> calls are Apple clang's, except in
        // the last run, where EvoCC builds the driver as well.
        for (first, second, driver_by) in [
            ("evocc", "clang", "clang"),
            ("clang", "evocc", "clang"),
            ("evocc", "evocc", "clang"),
            ("clang", "evocc", "evocc"),
        ] {
            let program = work
                .0
                .join(format!("threads-{first}-{second}-{driver_by}{level}"));
            let defines = [format!("-DFIRST={first}"), format!("-DSECOND={second}")];
            let driver_input = if driver_by == "evocc" {
                let object = work.0.join(format!("driver-{first}-{second}{level}.o"));
                let defines = defines.iter().map(String::as_str).collect::<Vec<_>>();
                compile(level, &driver, &object, &defines);
                object
            } else {
                driver.clone()
            };
            let mut arguments = vec!["-std=gnu11", "-O2"];
            arguments.extend(defines.iter().map(String::as_str));
            clang(
                &arguments,
                &[&driver_input, &evocc_worker, &clang_worker],
                &program,
            );
            let run = Command::new(&program).output().unwrap();
            let stdout = String::from_utf8_lossy(&run.stdout);
            assert_eq!(
                run.status.code(),
                Some(0),
                "{first}/{second} driver {driver_by} {level}: {}",
                text(&run)
            );
            assert!(stdout.contains("failures=0 tests=142"), "{stdout}");
            println!(
                "{level} {first}/{second} driver {driver_by}: {}",
                stdout.trim()
            );
        }
    }
}
