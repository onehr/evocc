//! Real preprocessing and lowering must agree on declaration identities.

use std::fs;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn compile(source: &str, mode: &str) -> Output {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
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
