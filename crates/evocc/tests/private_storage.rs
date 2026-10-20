//! Native storage and stack protection follow the surviving private objects.
use std::io::Write;
use std::process::{Command, Stdio};

fn emit(source: &str, optimize: &str, target: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--emit=asm",
            optimize,
            target,
            "-fstack-protector-strong",
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

#[test]
fn promoted_objects_do_not_request_a_native_canary() {
    for target in [
        "--target=aarch64-unknown-linux-gnu",
        "--target=aarch64-apple-darwin",
    ] {
        for source in [
            "int f(int x){int a[2]; a[0]=x; a[1]=3; return a[0]+a[1];}",
            "struct S{long x;}; long f(long x){struct S a={x},b; struct S *p=&b; b=a; return p->x;}",
        ] {
            assert!(emit(source, "-O0", target).contains("__stack_chk_fail"));
            let after = emit(source, "-O2", target);
            assert!(!after.contains("__stack_chk_fail"), "{after}");
        }
    }
}

#[test]
fn escaping_volatile_and_dynamic_storage_keep_stack_protection() {
    for target in [
        "--target=aarch64-unknown-linux-gnu",
        "--target=aarch64-apple-darwin",
    ] {
        for source in [
            "extern void use(int*); int f(int x){int a[1]; a[0]=x; use(a); return a[0];}",
            "int f(int x){volatile int a[1]; a[0]=x; return a[0];}",
            "int f(int x){int a[x]; a[x-1]=3; return a[x-1];}",
            "int f(int x){int a[1]; a[0]=x; asm volatile(\"\"::\"r\"(a):\"memory\"); return a[0];}",
        ] {
            let after = emit(source, "-O2", target);
            assert!(after.contains("__stack_chk_fail"), "{after}");
        }
    }
}
