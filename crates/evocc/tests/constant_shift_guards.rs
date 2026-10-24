//! Constant shift counts must not keep a run-time count guard.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
const KERNEL_FLAGS: [&str; 13] = [
    "-O2",
    "-std=gnu11",
    "-fno-strict-aliasing",
    "-fno-strict-overflow",
    "-mgeneral-regs-only",
    "-mbranch-protection=pac-ret",
    "-fno-omit-frame-pointer",
    "-fno-optimize-sibling-calls",
    "-fstack-protector-strong",
    "-ftrivial-auto-var-init=zero",
    "-funsigned-char",
    "-fno-PIE",
    "-w",
];
static NEXT: AtomicU64 = AtomicU64::new(0);

const SHIFT_VALUES: &str = "int main(void) {
    unsigned long x = 3;
    unsigned long a = x << 4;
    unsigned long b = x >> 63;
    int zero = 0;
    int c = zero << 31;
    unsigned char y = 0x80;
    unsigned char d = y >> 7;
    if (a != 48 || b != 0 || c != 0 || d != 1) return 1;
    return 0;
}
";
const VARIABLE_SHIFT: &str = "unsigned long f(unsigned long x, unsigned n) { return x << n; }\n";
const INVALID_64: &str = "int main(void) { unsigned long x = 1; return (x << 64) != 0; }\n";
const INVALID_NEG: &str = "int main(void) { unsigned long x = 1; return (x << -1) != 0; }\n";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}
fn command(target: &str) -> Command {
    let mut c = Command::new(evocc());
    c.args(["--target", target, "-std=gnu17"]);
    if target == LINUX {
        c.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    c
}
fn text(o: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        o.status,
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}
fn assembly(source: &str, target: &str, flags: &[&str]) -> Output {
    let dir = root().join("target/frontend/tmp").join(format!(
        "n09c-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("source.c");
    let out = dir.join("source.s");
    std::fs::write(&file, source).unwrap();
    let mut result = command(target)
        .args(flags)
        .args(["-S", "-o", out.to_str().unwrap()])
        .arg(&file)
        .output()
        .unwrap();
    if let Ok(bytes) = std::fs::read(&out) {
        result.stdout = bytes;
    }
    let _ = std::fs::remove_dir_all(dir);
    result
}
fn run(source: &str, target: &str, mode: &str, level: &str) -> Output {
    let dir = root().join("target/frontend/tmp").join(format!(
        "n09c-run-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("source.c");
    std::fs::write(&file, source).unwrap();
    let out = command(target)
        .args([level, mode])
        .arg(&file)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(dir);
    out
}
fn has_brk(s: &str) -> bool {
    s.lines().any(|line| line.trim().starts_with("brk #3005"))
}
fn code_has(o: &Output, code: &str) -> bool {
    String::from_utf8_lossy(&o.stderr).contains(&format!("[{code}]"))
}

#[test]
fn leaf_and_o2_have_no_constant_count_guard() {
    let leaf = root().join("tests/fixtures/kernel-codegen-reductions/leaf-helper-call.c");
    let leaf = std::fs::read_to_string(leaf).unwrap();
    for target in TARGETS {
        for flags in [&KERNEL_FLAGS[..], &["-O2", "-w"][..]] {
            let output = assembly(&leaf, target, flags);
            assert!(
                output.status.success(),
                "{target} {flags:?}\n{}",
                text(&output)
            );
            let asm = String::from_utf8(output.stdout).unwrap();
            assert!(
                !has_brk(&asm),
                "{target} {flags:?}: count guard remains\n{asm}"
            );
            assert!(
                !asm.lines()
                    .any(|line| line.contains("cmp") && line.contains("#64")),
                "{target} {flags:?}: constant-width cmp remains\n{asm}"
            );
        }
    }
}

#[test]
fn constant_and_variable_shifts_match_the_values() {
    for target in TARGETS {
        for level in LEVELS {
            for mode in INTERPRETERS {
                let output = run(SHIFT_VALUES, target, mode, level);
                assert!(
                    output.status.success() && output.stdout == b"0\n",
                    "{target} {level} {mode}\n{}",
                    text(&output)
                );
            }
        }
        let variable = assembly(VARIABLE_SHIFT, target, &["-O2", "-w"]);
        assert!(variable.status.success(), "{target}\n{}", text(&variable));
        let text = String::from_utf8(variable.stdout).unwrap();
        assert!(
            text.contains("brk #3005"),
            "{target}: variable guard missing\n{text}"
        );
    }
}

#[test]
fn invalid_constant_counts_keep_e3005() {
    for source in [INVALID_64, INVALID_NEG] {
        for target in TARGETS {
            for mode in INTERPRETERS {
                let output = run(source, target, mode, "-O0");
                assert!(
                    !output.status.success(),
                    "{target} {mode}\n{}",
                    text(&output)
                );
                assert!(
                    code_has(&output, "E3005"),
                    "{target} {mode}\n{}",
                    text(&output)
                );
            }
        }
    }
}

#[test]
fn constant_shift_program_passes_the_proof_path() {
    let dir = root()
        .join("target/frontend/tmp")
        .join(format!("n09c-prove-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("shift.c");
    let output = dir.join("shift.s");
    std::fs::write(
        &source,
        "int main(void) { unsigned long x = 3; return (x << 4) != 48 || (x >> 63) != 0; }\n",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_evo"))
        .args(["prove"])
        .arg(&source)
        .args(["-o"])
        .arg(&output)
        .args(["--entry", "main"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", text(&result));
    assert!(output.is_file(), "proof-required output missing");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "host native values; run explicitly"]
fn constant_shift_values_match_native_references() {
    let dir = root()
        .join("target/frontend/tmp")
        .join(format!("n09c-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("shift.c");
    std::fs::write(&source, SHIFT_VALUES).unwrap();
    let output = dir.join("shift");
    let clang = Command::new("clang")
        .args(["-std=gnu11", "-O0"])
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .status()
        .unwrap();
    assert!(clang.success());
    assert!(Command::new(&output).status().unwrap().success());
    let gcc = Command::new("container")
        .args([
            "run",
            "--rm",
            "--platform",
            "linux/arm64",
            "--network",
            "none",
            "-v",
        ])
        .arg(format!("{}:/work", dir.display()))
        .args([
            "evo-nexlab:sealed",
            "sh",
            "-c",
            "cd /work && gcc-14 -std=gnu11 -O0 shift.c -o shift-gcc && ./shift-gcc",
        ])
        .output()
        .unwrap();
    assert!(gcc.status.success(), "{}", text(&gcc));
    let _ = std::fs::remove_dir_all(dir);
}
