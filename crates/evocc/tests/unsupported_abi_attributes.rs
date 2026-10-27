//! An unsupported calling convention must not produce an ordinary function.
//! In particular, adding a frame before a naked asm body changes its return.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn compile(source: &str, target: &str, mode: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.args(["--target", target, mode, "-w"]);
    if mode == "-fsyntax-only" {
        command.args(["-x", "c"]);
    }
    let mut child = command
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn naked_functions_and_unsupported_calling_conventions_are_refused() {
    for target in ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        for attribute in [
            "naked",
            "interrupt",
            "interrupt_handler",
            "pcs(\"aapcs\")",
            "aarch64_vector_pcs",
            "aarch64_sve_pcs",
            "preserve_all",
            "preserve_most",
            "preserve_none",
            "no_callee_saved_registers",
            "no_caller_saved_registers",
            "ms_abi",
            "sysv_abi",
            "stdcall",
            "fastcall",
            "thiscall",
            "vectorcall",
            "regcall",
            "swiftcall",
            "swiftasynccall",
            "regparm(3)",
            "sseregparm",
            "callee_pop_aggregate_return(1)",
            // D16 accepts a `returns_twice` definition; see
            // returns_twice_attribute.rs.
            "target(\"+sve\")",
            "target_clones(\"default\")",
            "target_version(\"sve\")",
            "optimize(\"-fwrapv\")",
        ] {
            let source = format!("__attribute__(({attribute})) int f(void) {{ __asm__ volatile(\"mov w0, #7\\n\\tret\"); }}");
            for mode in ["--emit=asm", "-fsyntax-only"] {
                let output = compile(&source, target, mode);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert_eq!(
                    output.status.code(),
                    Some(1),
                    "{target} {attribute} {mode}\n{stderr}"
                );
                assert!(stderr.contains("E2952"), "{target} {attribute}\n{stderr}");
                assert!(
                    output.stdout.is_empty(),
                    "refusal published output for {attribute}"
                );
            }
        }
    }
}

#[test]
fn attribute_spellings_and_prior_declarations_do_not_bypass_refusal() {
    for source in [
        "__attribute__((__naked__)) int f(void) { __asm__(\"ret\"); }",
        "[[gnu::naked]] int f(void) { __asm__(\"ret\"); }",
        "int f(void) __attribute__((naked)); int f(void) { __asm__(\"ret\"); }",
        "static inline __attribute__((naked)) int f(void) { __asm__(\"ret\"); } int g(void) { return f(); }",
        "typedef int __attribute__((aarch64_vector_pcs)) (*callback)(void);",
    ] {
        let output = compile(source, "aarch64-unknown-linux-gnu", "--emit=asm");
        assert_eq!(output.status.code(), Some(1), "{source}\n{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2952"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn unrelated_attributes_keep_their_existing_meaning() {
    for source in [
        "__attribute__((unused, cold)) int f(void) { return 7; }",
        "[[other::naked]] int f(void) { return 7; }",
        "__attribute__((noinline)) int f(void) { return 7; }",
    ] {
        let output = compile(source, "aarch64-unknown-linux-gnu", "--emit=asm");
        assert!(
            output.status.success(),
            "{source}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty());
    }
}

#[test]
fn disabling_automatic_vectorization_preserves_scalar_execution() {
    let body = "int f(int x) { return x + 2; } int main(void) { return f(5) - 7; }";
    for target in ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        let baseline = compile(body, target, "--emit=asm");
        assert!(baseline.status.success());
        for attribute in [
            "__attribute__((optimize(\"no-tree-vectorize\")))",
            "__attribute__((__optimize__(\"-fno-tree-loop-vectorize\", \"no-tree-slp-vectorize\")))",
            "[[gnu::optimize(\"no-tree-loop-vectorize,no-tree-slp-vectorize\")]]",
            "[[__gnu__::optimize(\"no-tree-\" \"vectorize\")]]",
            "__attribute__((optimize(\"no-tree-\\166ectorize\")))",
        ] {
            let source = format!("{attribute} {body}");
            for mode in ["--interpret-c-hir", "--interpret-evoir", "-fsyntax-only", "--emit=asm"] {
                let output = compile(&source, target, mode);
                assert!(output.status.success(), "{target} {attribute} {mode}\n{}", String::from_utf8_lossy(&output.stderr));
                if mode.starts_with("--interpret") {
                    assert_eq!(output.stdout, b"0\n");
                } else if mode == "--emit=asm" {
                    assert_eq!(output.stdout, baseline.stdout);
                }
            }
        }
    }
}

#[test]
fn vectorization_restrictions_cannot_hide_other_function_options() {
    for arguments in [
        "",
        "\"\"",
        "\"tree-vectorize\"",
        "\"no-tree-vectorize,wrapv\"",
        "\"no-tree-vectorize,\"",
        "\"no-tree-vectorize\\0wrapv\"",
        "\"no-tree-vectorize\" + 0",
    ] {
        let source = format!("__attribute__((optimize({arguments}))) int f(void) {{ return 7; }}");
        let output = compile(&source, "aarch64-unknown-linux-gnu", "--emit=asm");
        assert_eq!(output.status.code(), Some(1), "{arguments}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2952"));
        assert!(output.stdout.is_empty());
    }
}
