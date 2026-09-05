//! Real preprocessing and lowering must agree on declaration identities.

use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn compile(source: &str, mode: &str) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let nonce = SERIAL.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "evocc-declaration-{}-{nonce}.c",
        std::process::id()
    ));
    fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", mode])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    output
}

fn lowers(source: &str) -> String {
    let parsed = compile(source, "--parse-only");
    assert!(
        parsed.status.success(),
        "parse: {}",
        String::from_utf8_lossy(&parsed.stderr)
    );
    let lowered = compile(source, "--emit=evoir");
    assert!(
        lowered.status.success(),
        "lower: {}",
        String::from_utf8_lossy(&lowered.stderr)
    );
    String::from_utf8(lowered.stdout).unwrap()
}

#[test]
fn functions_from_one_macro_have_distinct_parameter_scopes() {
    let source = "#define FUNCTIONS() int f(int x) { return x; } int g(int x) { return x + 1; }\nFUNCTIONS()\n";
    let ir = lowers(source);
    assert!(ir.contains("func @f("));
    assert!(ir.contains("func @g("));
}

#[test]
fn functions_from_one_macro_have_distinct_empty_scopes() {
    let source = "#define FUNCTIONS() int f(void) { int a = 1; return a; } int g(void) { int b = 2; return b; }\nFUNCTIONS()\n";
    let ir = lowers(source);
    assert!(ir.contains("func @f("));
    assert!(ir.contains("func @g("));
}

#[test]
fn extern_then_definition_preserves_one_object_identity() {
    for source in [
        "extern int a; int a = 3; int main(void) { return a; }",
        "extern int a[]; int a[4] = {1,2,3,4}; int main(void) { return a[3]; }",
        "extern const unsigned char a[]; const unsigned char a[4] = {1,2,3,4}; int main(void) { return a[3]; }",
    ] {
        lowers(source);
    }
}

#[test]
fn initializer_completes_written_unbounded_array() {
    for source in [
        "int a[] = {1,2,3,4}; int main(void) { return a[3]; }",
        "extern const unsigned char a[]; const unsigned char a[] = {1,2,3,4}; int main(void) { return a[3]; }",
        "char a[] = \"abc\"; int main(void) { return a[1]; }",
    ] {
        lowers(source);
    }
}

#[test]
fn object_alignment_accumulates_without_changing_type_identity() {
    for (declarations, alignment) in [
        (
            "extern long a[1]; long a[1] __attribute__((aligned(16)));",
            16,
        ),
        (
            "long a[1] __attribute__((aligned(16))); extern long a[1];",
            16,
        ),
        (
            "extern long a[1] __attribute__((aligned(32))); long a[1];",
            32,
        ),
        (
            "long a[1]; extern long a[1] __attribute__((aligned(32)));",
            32,
        ),
        (
            "extern __typeof__(long[1]) a; __typeof__(long[1]) a __attribute__((aligned(16)));",
            16,
        ),
    ] {
        let source = format!("{declarations} int main(void) {{ return sizeof(a); }}");
        let ir = lowers(&source);
        assert!(
            ir.lines().any(|line| line.starts_with("global @a ")
                && line.contains(&format!("size=8 align={alignment} "))),
            "{ir}"
        );
        let interpreted = compile(&source, "--interpret-c-hir");
        assert!(
            interpreted.status.success(),
            "{}",
            String::from_utf8_lossy(&interpreted.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&interpreted.stdout).trim(), "8");
    }
}

#[test]
fn unused_incomplete_enum_function_declarations_emit_nothing() {
    let ir = lowers("enum aarch64_insn_encoding_class aarch64_get_insn_class(unsigned int insn);");
    assert!(ir.trim().is_empty(), "{ir}");
}

#[test]
fn incomplete_enum_function_uses_are_diagnosed_at_the_use() {
    for source in [
        "enum e f(unsigned int); void g(void) { f(0); }",
        "enum e f(unsigned int); void g(void) { &f; }",
        "enum e f(unsigned int); void g(void) { f(0); } enum e { A };",
    ] {
        let output = compile(source, "--parse-only");
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && diagnostic.contains("E2930"),
            "{diagnostic}"
        );
    }
    lowers("enum e f(unsigned int); enum e { A }; enum e f(unsigned int x) { return A; } int main(void) { return f(0); }");
}
