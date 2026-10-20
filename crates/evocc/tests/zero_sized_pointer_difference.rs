//! A zero byte stride still has the pointer-difference division obligation.

use std::io::Write;
use std::process::{Command, Stdio};

const SOURCES: [&str; 2] = [
    "struct empty {}; struct empty objects[2]; int main(void){return &objects[1]-&objects[0];}",
    "int objects[2][0]; int main(void){return &objects[1]-&objects[0];}",
];

#[test]
fn zero_sized_pointer_difference_keeps_its_guard_through_native_lowering() {
    for source in SOURCES {
        for (mode, required) in [
            ("--emit=evoir", "ub.assume.division_by_zero"),
            ("--emit=asm", "brk #3002"),
        ] {
            let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args(["--target=aarch64-unknown-linux-gnu", mode, "/dev/stdin"])
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
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.contains(required), "{mode}: {text}");
            if mode == "--emit=evoir" {
                assert!(text.find("divisor_nonzero").unwrap() < text.find("sdiv.").unwrap());
            }
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn checked_native_pointer_difference_traps() {
    use std::os::unix::process::ExitStatusExt;
    let directory =
        std::env::temp_dir().join(format!("evocc-pointer-stride-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for source in SOURCES {
        let input = directory.join("input.c");
        let program = directory.join("program");
        std::fs::write(&input, source).unwrap();
        // Execute the diagnostic path before native compilation. UB is the
        // expected result of this regression, not an admission to safe C.
        let checked = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .arg("--check")
            .arg(&input)
            .output()
            .unwrap();
        assert_eq!(checked.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&checked.stderr).contains("E3002"));
        let compiled = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .arg(&input)
            .arg("-o")
            .arg(&program)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let result = Command::new(&program).output().unwrap();
        assert_eq!(
            result.status.signal(),
            Some(5),
            "expected SIGTRAP: {result:?}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}
