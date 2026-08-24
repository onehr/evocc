//! The C-HIR dump shows every attribute the source had, and reports which were
//! applied and which were preserved without effect (specification 10.1.9).
//!
//! The preserved-not-applied set is what the stage receipt lists by name, so
//! it is produced by the compiler rather than by hand.

use std::io::Write;
use std::process::Command;

fn evocc(args: &[&str], source: &str) -> (i32, String, String) {
    let dir = std::env::temp_dir().join(format!("evocc-attr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("input.c");
    std::fs::File::create(&path).unwrap().write_all(source.as_bytes()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .arg(&path)
        .output()
        .expect("evocc runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const KERNEL_FUNCTION: &str = "static inline __attribute__((__gnu_inline__)) __attribute__((__unused__)) \
    __attribute__((__no_instrument_function__)) void *offset_to_ptr(const int *off) { return 0; }\n";

#[test]
fn dump_lists_every_attribute_with_its_attachment() {
    let (code, stdout, stderr) = evocc(&["--emit=c-hir"], KERNEL_FUNCTION);
    assert_eq!(code, 0, "c-hir dump must succeed; stderr: {stderr}");
    for name in ["gnu_inline", "unused", "no_instrument_function"] {
        assert!(
            stdout.contains(&format!("attribute {name}")),
            "dump must list `{name}` on offset_to_ptr; dump was:\n{stdout}"
        );
    }
}

#[test]
fn dump_reports_applied_and_preserved_not_applied_by_name() {
    let source = format!(
        "{KERNEL_FUNCTION}typedef __signed__ __int128 __s128 __attribute__((aligned(16)));\n"
    );
    let (code, stdout, stderr) = evocc(&["--emit=c-hir"], &source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("applied: aligned"), "{stdout}");
    for name in ["gnu_inline", "unused", "no_instrument_function"] {
        assert!(stdout.contains(&format!("preserved-not-applied: {name}")), "{stdout}");
    }
}
