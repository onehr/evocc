use std::io::Write;
use std::process::{Command, Stdio};

fn assembly(target: &str, flags: &[&str], source: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--emit=asm", "--target", target])
        .args(flags)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn body<'a>(text: &'a str, target: &str, name: &str) -> &'a str {
    let prefix = if target.contains("apple") { "_" } else { "" };
    text.split_once(&format!("\n{prefix}{name}:\n"))
        .unwrap()
        .1
        .split(".cfi_endproc")
        .next()
        .unwrap()
}

fn assert_policy(text: &str, framed: bool, signed: bool) {
    assert_eq!(text.contains("stp x29, x30"), framed, "{text}");
    assert_eq!(text.contains("ldp x29, x30"), framed, "{text}");
    assert_eq!(text.contains("mov x29, sp"), framed, "{text}");
    assert_eq!(text.contains(".cfi_def_cfa 29"), framed, "{text}");
    assert_eq!(
        text.matches("paciasp").count(),
        usize::from(signed),
        "{text}"
    );
    let returns = text.lines().filter(|line| line.trim() == "ret").count();
    assert!(returns > 0, "{text}");
    assert_eq!(
        text.matches("autiasp").count(),
        if signed { returns } else { 0 },
        "{text}"
    );
    assert_eq!(
        text.matches(".cfi_negate_ra_state").count(),
        if signed { returns + 1 } else { 0 },
        "{text}"
    );
    if framed && signed {
        assert!(
            text.find("paciasp").unwrap() < text.find("stp x29, x30").unwrap(),
            "{text}"
        );
        assert!(text.contains("ldp x29, x30, [sp], #16\n.cfi_def_cfa 31, 0\n.cfi_restore 29\n.cfi_restore 30\n    autiasp"), "{text}");
    }
}

#[test]
fn signing_and_leaf_frames_follow_independent_effective_requests() {
    let source = include_str!("../../../tests/fixtures/return-signing-policy.c");
    for target in ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        for optimization in ["-O0", "-O2"] {
            for (policy, sign_leaf) in [("pac-ret", false), ("pac-ret+leaf", true)] {
                for force_leaf in [false, true] {
                    let option = format!("-mbranch-protection={policy}");
                    let mut flags = vec![
                        optimization,
                        "-funwind-tables",
                        "-fno-omit-frame-pointer",
                        "-mgeneral-regs-only",
                        &option,
                    ];
                    if force_leaf {
                        flags.push("-mno-omit-leaf-frame-pointer");
                    }
                    let text = assembly(target, &flags, source);
                    assert_policy(
                        body(&text, target, "plain_leaf"),
                        force_leaf,
                        force_leaf || sign_leaf,
                    );
                    for name in [
                        "calling",
                        "storage",
                        "opaque",
                        "stack_argument",
                        "frame_address",
                    ] {
                        assert_policy(body(&text, target, name), true, true);
                    }
                }
            }
        }
    }
}

#[test]
fn calls_inserted_by_lowering_and_runtime_stack_uses_keep_signed_frames() {
    for source in [
        "__int128 f(__int128 a, __int128 b) { return a / b; }",
        "long f(long n) { volatile char a[n]; a[0] = 1; return a[0]; }",
        "long f(int n, ...) { __builtin_va_list ap; __builtin_va_start(ap,n); long x=__builtin_va_arg(ap,long); __builtin_va_end(ap); return x; }",
        "void *f(void) { return __builtin_return_address(0); }",
    ] {
        let target = "aarch64-unknown-linux-gnu";
        let text = assembly(target, &["-O2", "-funwind-tables", "-mbranch-protection=pac-ret"], source);
        assert_policy(body(&text, target, "f"), true, true);
    }
}

#[test]
fn patchable_nops_and_unused_stack_guard_policy_do_not_store_the_link_register() {
    let target = "aarch64-unknown-linux-gnu";
    let source = "unsigned long f(unsigned long x) { return x ^ 7; }";
    let text = assembly(
        target,
        &[
            "-O2",
            "-funwind-tables",
            "-mbranch-protection=pac-ret",
            "-fpatchable-function-entry=4,2",
            "-fno-omit-frame-pointer",
            "-mgeneral-regs-only",
            "-mstack-protector-guard=sysreg",
            "-mstack-protector-guard-reg=sp_el0",
            "-mstack-protector-guard-offset=64",
        ],
        source,
    );
    assert_policy(body(&text, target, "f"), false, false);
    assert_eq!(text.matches("    nop\n").count(), 4, "{text}");
    assert!(text.contains("__patchable_function_entries"), "{text}");
}
