use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str, mode: &str) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let file = std::env::temp_dir().join(format!(
        "evocc-lifecycle-{}-{}.c",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&file, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", mode])
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_file(file).unwrap();
    output
}

#[test]
fn constructor_destructor_registration_survives_internal_function_elimination() {
    let output = run("static int state; static void a(void) __attribute__((constructor(201))); static void a(void){state=1;} static void b(void) __attribute__((destructor(202))); static void b(void){state=0;} int main(void){return state;}", "--emit=asm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = String::from_utf8_lossy(&output.stdout);
    assert!(assembly.contains(".init_array.00201"), "{assembly}");
    assert!(assembly.contains(".fini_array.00202"), "{assembly}");
}

#[test]
fn pure_lifecycle_handlers_still_participate_in_program_effect_threading() {
    for (attribute, section) in [
        ("constructor", ".init_array"),
        ("destructor", ".fini_array"),
    ] {
        for source in [
            format!("static void f(void) __attribute__(({attribute})); static void f(void){{}} int main(void){{return 0;}}"),
            format!("static void f(void){{}} static void f(void) __attribute__(({attribute})); int main(void){{return 0;}}"),
            format!("static __attribute__(({attribute})) void f(void){{(void)0;}} int main(void){{return 0;}}"),
        ] {
            for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
                let output = run(&source, mode);
                assert!(output.status.success(), "{source}\n{mode}: {}", String::from_utf8_lossy(&output.stderr));
                if mode == "--emit=asm" {
                    assert!(String::from_utf8_lossy(&output.stdout).contains(section));
                } else {
                    assert_eq!(output.stdout, b"0\n");
                }
            }
        }
    }
}

#[test]
fn tls_is_an_instance_address_not_a_process_global() {
    let output = run(
        "static __thread int x=7; int *address(void){return &x;} int read(void){return x;}",
        "--emit=evoir",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ir = String::from_utf8_lossy(&output.stdout);
    assert!(ir.contains("tls_addr"), "{ir}");
    assert!(!ir.contains("global_addr @x"), "{ir}");
}

#[test]
fn program_lifecycle_runs_in_one_state_in_priority_order() {
    let source = "static int state; static void a(void) __attribute__((constructor(202))); static void b(void) __attribute__((constructor(201))); static void a(void){state=state*10+2;} static void b(void){state=state*10+1;} static void c(void) __attribute__((destructor(202))); static void d(void) __attribute__((destructor(201))); static void c(void){if(state!=123) __builtin_trap();state=4;} static void d(void){if(state!=4) __builtin_trap();} int main(void){if(state!=12)return 9;state=123;return 0;}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
    }
}

#[test]
fn exit_runs_finalizers_but_immediate_termination_does_not() {
    for name in ["exit", "_Exit", "quick_exit"] {
        let body = if name == "exit" {
            "if(x!=7)__builtin_trap();"
        } else {
            "__builtin_trap();"
        };
        let source = format!("extern void {name}(int); static int x; static void end(void) __attribute__((destructor)); static void end(void){{{body}}} int main(void){{x=7;{name}(3);return 99;}}");
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(&source, mode);
            assert!(
                output.status.success(),
                "{name} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "3");
        }
    }
}

#[test]
fn tls_initializers_and_function_static_instances_execute_in_both_interpreters() {
    let source = "int global=9; _Thread_local int x=7; static _Thread_local int *p=&global; __thread int *zero; int f(void){static __thread int n=10; return ++n;} int main(void){if(x!=7||*p!=9||zero!=0)return 1;x=12;if(f()!=11||f()!=12)return 2;return x-12;}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = run(source, mode);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
}

#[test]
fn tls_storage_redeclarations_and_static_address_rejections() {
    for source in [
        "extern __thread int x; __thread int x=3; int f(void){return x;}",
        "__thread extern int x; __thread int x;",
        "static int __thread x; int f(void){return x;}",
    ] {
        let out = run(source, "--emit=evoir");
        assert!(
            out.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    for source in [
        "__thread int x; int *p=&x;",
        "__thread int x; __thread int *p=&x;",
        "int x; extern __thread int x;",
        "__thread int x; extern int x;",
        "void f(void){__thread int x;}",
    ] {
        let out = run(source, "--emit=evoir");
        assert!(
            !out.status.success(),
            "accepted invalid TLS declaration: {source}"
        );
    }
}

#[test]
fn const_tls_instances_freeze_after_their_initializers() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = run(
            "int g=9; __thread int *const p=&g; int main(void){return *p-9;}",
            mode,
        );
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
        let out = run(
            "__thread const int x=7; int main(void){*(int*)&x=8;return 0;}",
            mode,
        );
        assert!(!out.status.success(), "{mode} allowed a const TLS store");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("E3014"),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn lifecycle_does_not_skip_unmodelled_calls_or_reinitialize_for_nested_calls() {
    let source = "static int n; static void init(void) __attribute__((constructor)); static void init(void){++n;} int helper(void){return n;} int main(void){return helper()+helper();}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = run(source, mode);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"2\n");
        let out=run("extern void unavailable(void); static void init(void) __attribute__((constructor)); static void init(void){unavailable();} int main(void){return 0;}",mode);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("E3039"));
    }
}

#[test]
fn a_constructor_declaration_does_not_register_an_undefined_function() {
    let source =
        "extern void elsewhere(void) __attribute__((constructor)); int main(void){return 0;}";
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let out = run(source, mode);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(out.stdout, b"0\n");
        } else {
            assert!(!String::from_utf8_lossy(&out.stdout).contains(".init_array"));
        }
    }
}

#[test]
fn conflicting_tls_models_are_not_silently_overwritten() {
    for source in [
        "__thread int x __attribute__((tls_model(\"local-exec\"))); extern __thread int x __attribute__((tls_model(\"initial-exec\"))); int f(void){return x;}",
        "int x __attribute__((tls_model(\"local-exec\"))); int f(void){return x;}",
        "__thread int x __attribute__((tls_model(\"unknown\"))); int f(void){return x;}",
    ] {
        let out=run(source,"--emit=evoir");
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("E2943"),"{}",String::from_utf8_lossy(&out.stderr));
    }
}

#[test]
fn tls_template_initialization_preserves_exposed_addresses_and_custom_sections() {
    let source="int a[2]={4,9}; static __thread unsigned long p=(unsigned long)&a[1]; static __thread int x __attribute__((section(\".tdata.local\")))=3; int main(void){return *(int *)p+x-12;}";
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let out = run(source, mode);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(out.stdout, b"0\n");
        } else {
            assert!(
                String::from_utf8_lossy(&out.stdout).contains(".tdata.local\" ,\"awT\"")
                    || String::from_utf8_lossy(&out.stdout).contains(".tdata.local\",\"awT\"")
            );
        }
    }
}

#[test]
fn tls_aliases_share_the_same_aligned_thread_instance() {
    let source = include_str!("../../../tests/fixtures/tls-alias.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let out = run(source, mode);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(out.stdout, b"0\n");
        }
    }
}
