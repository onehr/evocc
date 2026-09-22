use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str, mode: &str) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let file = std::env::temp_dir().join(format!(
        "evocc-atomic-{}-{}.c",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&file, source).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.args(["--target=aarch64-unknown-linux-gnu", "-O2", mode]);
    if mode != "--interpret-c-hir" {
        command.arg("-Oeq-scalar");
    }
    let result = command.arg(&file).output().unwrap();
    fs::remove_file(file).unwrap();
    result
}

#[test]
fn scalar_atomic_values_and_operand_effects_execute_in_both_interpreters() {
    let source = include_str!("../../../tests/fixtures/scalar-atomic-access.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let result = run(source, mode);
        assert!(
            result.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, b"0\n", "{mode}");
    }
}

#[test]
fn scalar_atomic_operations_keep_order_in_ir_and_machine_instructions() {
    let source = include_str!("../../../tests/fixtures/scalar-atomic-access.c");
    let ir = run(source, "--emit=evoir");
    assert!(
        ir.status.success(),
        "{}",
        String::from_utf8_lossy(&ir.stderr)
    );
    let ir = String::from_utf8(ir.stdout).unwrap();
    for form in [
        "atomic.load relaxed",
        "atomic.load acquire",
        "atomic.load consume",
        "atomic.load seq_cst",
        "atomic.store relaxed",
        "atomic.store release",
        "atomic.store seq_cst",
        "atomic=true",
    ] {
        assert!(ir.contains(form), "missing {form}: {ir}");
    }
    assert!(!ir.contains("declare @__atomic"));
    let assembly = run(source, "--emit=asm");
    assert!(
        assembly.status.success(),
        "{}",
        String::from_utf8_lossy(&assembly.stderr)
    );
    let assembly = String::from_utf8(assembly.stdout).unwrap();
    for form in [
        "ldrb ", "ldarh ", "ldar w", "ldar x", "strb ", "stlrh ", "stlr w", "stlr x",
    ] {
        assert!(assembly.contains(form), "missing {form}: {assembly}");
    }
    assert!(!assembly.contains("bl __atomic"));
}

#[test]
fn atomic_type_order_and_constant_context_errors_are_not_silent() {
    for (source, code) in [
        ("int f(_Bool*p){return __sync_fetch_and_add(p,1);}", "E2913"),
        (
            "int f(const int*p){return __sync_fetch_and_add(p,1);}",
            "E2913",
        ),
        (
            "int f(int*p){return __sync_bool_compare_and_swap(p,1);}",
            "E2909",
        ),
        (
            "int f(double*p){return __sync_fetch_and_add(p,1);}",
            "E2913",
        ),
        (
            "int x; _Static_assert(__sync_add_and_fetch(&x,1),\"not ICE\");",
            "E2912",
        ),
        ("int f(int*p){return __atomic_load_n(p,3);}", "E2947"),
        ("int f(int*p){return __atomic_load_n(p,4);}", "E2947"),
        ("void f(int*p){__atomic_store_n(p,1,1);}", "E2947"),
        ("void f(int*p){__atomic_store_n(p,1,2);}", "E2947"),
        ("void f(int*p){__atomic_store_n(p,1,4);}", "E2947"),
        ("int f(int*p){return __atomic_load_n(p,-1);}", "E2947"),
        ("int f(int*p){return __atomic_load_n(p,6);}", "E2947"),
        ("int f(int*p){return __atomic_load_n(p);}", "E2909"),
        ("void f(const int*p){__atomic_store_n(p,1,0);}", "E2913"),
        ("long f(void*p){return __atomic_load_n(p,0);}", "E2913"),
        ("int f(double*p){return __atomic_load_n(p,0);}", "E2913"),
        (
            "__int128 f(__int128*p){return __atomic_load_n(p,0);}",
            "E2913",
        ),
        (
            "int x; _Static_assert(__atomic_load_n(&x,0)==0,\"not ICE\");",
            "E2912",
        ),
    ] {
        let output = run(source, "--emit=evoir");
        assert!(!output.status.success(), "accepted: {source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn atomic_pointer_provenance_and_alignment_use_existing_memory_checks() {
    for (source, code) in [
        (
            "int main(void){int x;return __atomic_load_n(&x,0);}",
            "E3001",
        ),
        (
            "int main(void){return __atomic_load_n((int*)0,0);}",
            "E3009",
        ),
        (
            "int main(void){long a[2]={0};return __atomic_load_n((int*)((char*)a+1),0);}",
            "E3012",
        ),
        (
            "int main(void){int x=1;int*p=&x;*(unsigned char*)&p=0;return *__atomic_load_n(&p,0);}",
            "E3013",
        ),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(!output.status.success(), "{mode} accepted: {source}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(code),
                "{mode} {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
