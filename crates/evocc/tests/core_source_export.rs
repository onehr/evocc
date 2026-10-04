//! Source-input export only: none of these tests grants a compilation proof.
use std::io::Write;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn invoke(source: &str, arguments: &[&str]) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let file = std::env::temp_dir().join(format!(
        "evocc-core-source-{}-{}.c",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(arguments)
        .arg(&file)
        .output()
        .unwrap();
    std::fs::remove_file(file).unwrap();
    output
}

fn exported(source: &str, entry: &str) -> String {
    let flag = format!("--core-entry={entry}");
    let output = invoke(source, &["--emit=core-c-hir", &flag, "-O0", "-fno-builtin"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("[\"core-v1\",\"aarch64-unknown-linux-gnu\",\"checked\","));
    assert!(!text.contains("VERIFIED"));
    text
}

#[test]
fn unchanged_original_bodies_keep_control_and_conversions() {
    let source = include_str!("../../../tests/fixtures/verification/musl-abs.c");
    let text = exported(source, "abs");
    assert_eq!(text.matches("[\"function\"").count(), 1);
    assert_eq!(text.matches("[\"conditional\"").count(), 1);
    assert_eq!(text.matches("[\"unary\",\"int\",\"neg\"").count(), 1);
    assert_eq!(text.matches("[\"return\"").count(), 1);
    assert!(text.contains("[\"binary\",\"int\",\"gt\""));
    assert!(text.contains("[\"lit\",\"int\",\"00000000\"]"));
    assert_eq!(text, exported(source, "abs"));

    let fixed = exported(
        include_str!("../../../tests/fixtures/verification/doom-fixedmul.c"),
        "FixedMul",
    );
    assert_eq!(
        fixed.matches("[\"cast\",\"long\",[\"long\",false]").count(),
        2
    );
    assert!(fixed.contains("[\"binary\",\"long\",\"mul\""));
    assert!(fixed.contains("[\"binary\",\"long\",\"shr\""));
    assert!(fixed.contains("[\"lit\",\"int\",\"00000010\"]"));
    assert!(fixed.contains("[\"return\",[\"int\",false]"));
}

#[test]
fn complete_call_closure_and_all_stack_arguments_are_exported() {
    let text = exported(
        include_str!("../../../tests/fixtures/verification/pressure.c"),
        "pressure",
    );
    assert_eq!(text.matches("[\"function\"").count(), 2);
    assert_eq!(text.matches("[\"argument\"").count(), 32);
    assert_eq!(text.matches("[\"parameter\"").count(), 33);
    assert!(text.find("\"wide\"").unwrap() < text.find("\"pressure\"").unwrap());
    assert!(text.contains("[\"lit\",\"ulong\",\"000000000000001f\"]"));
}

#[test]
fn loops_scopes_updates_and_dead_but_admitted_syntax_survive() {
    let source = include_str!("../../../tests/fixtures/verification/control.c");
    let text = exported(source, "loop_join");
    for tag in [
        "for", "while", "do_while", "break", "continue", "object", "compound",
    ] {
        assert!(text.contains(&format!("[\"{tag}\"")), "missing {tag}");
    }
    let text = exported("int f(int x) { if (0) { x++; } return x; }", "f");
    assert!(text.contains("[\"update\",\"int\",\"post_inc\""));
    let text = exported(
        "unsigned long long f(const unsigned long x) { return x + 0xffffffffffffffffULL; }",
        "f",
    );
    assert!(text.contains("[\"ulong\",true]"));
    assert!(text.contains("[\"lit\",\"ulong_long\",\"ffffffffffffffff\"]"));
}

#[test]
fn unsupported_inputs_publish_no_partial_source_tree() {
    for source in [
        "int f(int *p) { return *p; }",
        "int f(int x) { if (0) { asm(\"nop\"); } return x; }",
        "int ext(int); int f(int x) { return ext(x); }",
        "int f(void) { int x; return x; }",
        "int f(); int f(int x) { return x; }",
    ] {
        let output = invoke(
            source,
            &["--emit=core-c-hir", "--core-entry=f", "-O0", "-fno-builtin"],
        );
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("source export refused"));
    }
}

#[test]
fn export_options_do_not_silently_change_other_compilation_paths() {
    for arguments in [
        vec!["--emit=core-c-hir", "--core-entry=f"],
        vec!["--emit=core-c-hir", "--core-entry=f", "-O2", "-fno-builtin"],
        vec!["--emit=core-c-hir", "-O0", "-fno-builtin"],
        vec![
            "--emit=core-c-hir",
            "--core-entry=f",
            "--core-entry=f",
            "-fno-builtin",
        ],
        vec!["--emit=evoir", "--core-entry=f"],
    ] {
        let output = invoke("int f(int x) { return x; }", &arguments);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn complete_source_execution_fixture_agrees_in_existing_interpreters() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/verification/source-execution.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([mode, "-O0", "-fno-builtin"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--emit=core-c-hir",
            "--core-entry=main",
            "-O0",
            "-fno-builtin",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let tree = String::from_utf8(output.stdout).unwrap();
    assert_eq!(tree.matches("[\"function\"").count(), 10);
    assert!(!tree.contains("VERIFIED"));
}
