//! A backend refusal retains its source function and the EvoIR location.
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, target: &str, flags: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(format!("--target={target}"))
        .args(["--emit=asm", "--error-format=short", "-w"])
        .args(flags)
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
fn a_refused_function_does_not_hide_behind_a_translation_unit_error() {
    // A 16-byte vector needs a SIMD register, which -mgeneral-regs-only
    // refuses in the backend. Tiny vectors at a call boundary lower (L06).
    let source = "typedef int v4si __attribute__((vector_size(16)));\nint clean(void) { return 0; }\nv4si bad(v4si x) { return x; }\nint local(void) { int x[4]={1,2,3,4}; return x[1]; }\n";
    for target in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
        let output = run(source, target, &["-mgeneral-regs-only"]);
        let message = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{message}");
        assert!(output.stdout.is_empty(), "a refused unit has no assembly");
        assert!(
            message.contains("/dev/stdin:3:6: error[E4002]"),
            "{message}"
        );
        assert!(message.contains("not-lowered: vector"), "{message}");
        assert!(message.contains("@bad block 0 instruction 0"), "{message}");
        assert!(message.contains("-mgeneral-regs-only"), "{message}");
        assert!(!message.contains("<command-line>"), "{message}");
    }
}

#[test]
fn darwin_profile_refusals_are_lowering_diagnostics() {
    for (source, flags, detail) in [
        ("int f(void) __attribute__((section(\".text.custom\")));\nint f(void) { return 0; }\n", &[][..], "explicit function sections"),
        ("__thread int value;\nint f(void) { return value; }\n", &[][..], "ELF TLS"),
        ("int f(void) __attribute__((patchable_function_entry(2,1)));\nint f(void) { return 0; }\n", &[][..], "patchable function entries"),
        ("int f(void) { return 0; }\n", &["-ffunction-sections"][..], "individual function/data sections"),
    ] {
        let output = run(source, "aarch64-apple-darwin", flags);
        let message = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{message}");
        assert!(output.stdout.is_empty());
        assert!(message.contains("error[E4009]: not-lowered: unsupported-form"), "{message}");
        assert!(message.contains(detail), "{message}");
        assert!(!message.contains("E0004"), "{message}");
        let admitted = run(source, "aarch64-unknown-linux-gnu", flags);
        assert!(admitted.status.success(), "{admitted:?}");
    }
}

#[test]
fn assembler_labels_do_not_lose_the_source_function() {
    let source = "typedef int v4si __attribute__((vector_size(16)));\nv4si bad(v4si x) __asm__(\"_renamed\");\nv4si bad(v4si x) { return x; }\n";
    let output = run(source, "aarch64-apple-darwin", &["-mgeneral-regs-only"]);
    let message = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{message}");
    assert!(
        message.contains("/dev/stdin:3:6: error[E4002]"),
        "{message}"
    );
    assert!(
        message.contains("@renamed block 0 instruction 0"),
        "{message}"
    );
}
