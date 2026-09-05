use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str, mode: &str) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "evocc-external-{}-{}.c",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
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

#[test]
fn referenced_external_symbols_lower_without_invented_bodies() {
    for (source, expected) in [
        (
            "extern int g(int); int f(int x){return g(x);}",
            "declare @g(",
        ),
        (
            "extern int x; int f(void){return x;}",
            "extern @x size=4 align=4",
        ),
        (
            "void f(void*p){__builtin_memset(p,0,1);}",
            "declare @memset(",
        ),
        (
            "extern int g(int); int (*f(void))(int){return g;}",
            "declare @g(",
        ),
    ] {
        let output = run(source, "--emit=evoir");
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(expected), "{text}");
        assert!(!text.contains("func @g("));
        evocc_evoir::parse_section9_module(text.as_bytes()).expect("valid external references");
    }
}

#[test]
fn unresolved_runtime_symbols_fail_only_when_reached() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        for (source, code) in [
            ("extern int g(void); int main(void){return g();}", "E3039"),
            ("extern int x; int main(void){return x;}", "E3040"),
        ] {
            let output = run(source, mode);
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(code),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = run(
            "extern int g(void); extern int x; int main(void){if(0)return g()+x;return 0;}",
            mode,
        );
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn external_memory_library_executes_real_bytes() {
    let source = "int main(void){char a[8]; char b[8]; __builtin_memset(a,0,8); __builtin_memcpy(b,a,8); __builtin_memmove(b+1,b,4); return __builtin_memcmp(a,b,8) + __builtin_strcmp(a,b) + __builtin_strncmp(a,b,8) + __builtin_strlen(a);}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn unprototyped_external_calls_require_one_promoted_signature() {
    let output = run(
        "extern int g(); int f(void){g(1);return g(1L);}",
        "--emit=evoir",
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("E2931"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
