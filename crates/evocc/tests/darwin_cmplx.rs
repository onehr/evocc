//! Darwin `CMPLX` refusal and the `__builtin_complex` replacement.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU64, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
#[cfg(target_os = "macos")]
const DARWIN: &str = "aarch64-apple-darwin";
#[cfg(target_os = "macos")]
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
#[cfg(target_os = "macos")]
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
const BUILTIN_CHECK: &str = r#"#include <complex.h>
int main(void) {
    volatile double inf = __builtin_inf();
    volatile double nan = __builtin_nan("");
    double _Complex d = __builtin_complex(1.0, 2.0);
    double _Complex di = __builtin_complex(1.0, inf);
    double _Complex dn = __builtin_complex(1.0, nan);
    double _Complex dz = __builtin_complex(1.0, -0.0);
    float _Complex f = __builtin_complex(1.0f, 2.0f);
    float _Complex fi = __builtin_complex(1.0f, (float)inf);
    float _Complex fn = __builtin_complex(1.0f, (float)nan);
    float _Complex fz = __builtin_complex(1.0f, -0.0f);
    long double _Complex l = __builtin_complex(1.0L, 2.0L);
    long double _Complex li = __builtin_complex(1.0L, (long double)inf);
    long double _Complex ln = __builtin_complex(1.0L, (long double)nan);
    long double _Complex lz = __builtin_complex(1.0L, -0.0L);
    if (__real__ d != 1.0 || __imag__ d != 2.0) return 1;
    if (!__builtin_isinf(__imag__ di) || !__builtin_isnan(__imag__ dn)) return 2;
    if (!__builtin_signbit(__imag__ dz)) return 3;
    if (__real__ f != 1.0f || __imag__ f != 2.0f) return 4;
    if (!__builtin_isinf(__imag__ fi) || !__builtin_isnan(__imag__ fn)) return 5;
    if (!__builtin_signbit(__imag__ fz)) return 6;
    if (__real__ l != 1.0L || __imag__ l != 2.0L) return 7;
    if (!__builtin_isinf(__imag__ li) || !__builtin_isnan(__imag__ ln)) return 8;
    if (!__builtin_signbit(__imag__ lz)) return 9;
    return 0;
}
"#;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn command(target: &str) -> Command {
    let mut command = Command::new(evocc());
    command.args(["--target", target, "-std=gnu17"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command
}

#[cfg(target_os = "macos")]
fn compile(source: &str, target: &str, level: &str, _mode: &str) -> Output {
    let directory = root().join("target/frontend/tmp").join(format!(
        "d27b-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let source_file = directory.join("source.c");
    let assembly = directory.join("source.s");
    std::fs::write(&source_file, source).unwrap();
    let output = command(target)
        .args([level, "-S", "-o", assembly.to_str().unwrap()])
        .arg(&source_file)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(directory);
    output
}

fn run_interpreter(source: &str, target: &str, level: &str, mode: &str) -> Output {
    let mut command = command(target);
    command
        .args([level, mode, "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    use std::io::Write;
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[cfg(target_os = "macos")]
fn has_code(output: &Output, code: &str) -> bool {
    String::from_utf8_lossy(&output.stderr).contains(&format!("[{code}]"))
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_cmplx_macros_keep_w0003_and_point_to_builtin() {
    let sources = ["double _Complex", "float _Complex", "long double _Complex"];
    let calls = ["CMPLX", "CMPLXF", "CMPLXL"];
    for (ty, call) in sources.iter().zip(calls) {
        let source = format!(
            "#include <complex.h>\nint main(void) {{ {ty} z = {call}(1.0, 2.0); return __real__ z != 1.0; }}\n"
        );
        for level in LEVELS {
            let output = compile(&source, DARWIN, level, "-S");
            assert!(
                !output.status.success(),
                "{call} {level}\n{}",
                text(&output)
            );
            assert!(
                has_code(&output, "W0003"),
                "{call} {level}\n{}",
                text(&output)
            );
        }
    }

    let explain = Command::new(env!("CARGO_BIN_EXE_evo"))
        .args(["explain", "W0003"])
        .output()
        .unwrap();
    assert!(explain.status.success(), "{}", text(&explain));
    assert!(String::from_utf8_lossy(&explain.stdout).contains("__builtin_complex"));
}

#[cfg(target_os = "macos")]
#[test]
fn builtin_complex_preserves_values_on_both_targets() {
    for target in TARGETS {
        for level in LEVELS {
            for mode in INTERPRETERS {
                let output = run_interpreter(BUILTIN_CHECK, target, level, mode);
                assert!(
                    output.status.success(),
                    "{target} {level} {mode}\n{}",
                    text(&output)
                );
                assert_eq!(
                    output.stdout,
                    b"0\n",
                    "{target} {level} {mode}\n{}",
                    text(&output)
                );
            }
        }
    }
}

#[test]
fn linux_cmplx_matches_the_reference_values() {
    for call in [
        "CMPLX(1.0, 2.0)",
        "CMPLX(1.0, (double)__builtin_inf())",
        "CMPLX(1.0, (double)__builtin_nan(\"\"))",
    ] {
        let source = format!(
            "#include <complex.h>\nint main(void) {{ double _Complex z = {call}; return __real__ z != 1.0 || !(__imag__ z == __imag__ z || __imag__ z != __imag__ z); }}\n"
        );
        for level in LEVELS {
            for mode in INTERPRETERS {
                let output = run_interpreter(&source, LINUX, level, mode);
                assert!(
                    output.status.success(),
                    "{call} {level} {mode}\n{}",
                    text(&output)
                );
                assert_eq!(
                    output.stdout,
                    b"0\n",
                    "{call} {level} {mode}\n{}",
                    text(&output)
                );
            }
        }
    }
}
