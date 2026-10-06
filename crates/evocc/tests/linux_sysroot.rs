//! Linux header discovery must select one root, without mixing fallback roots.
//! These tests preprocess small headers. They need no container or assembler.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);
const TARGET: &str = "aarch64-unknown-linux-gnu";

#[test]
fn header_export_contract_checks_run_in_the_workspace_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .arg(root.join("tools/tests/test_sysroot.py"))
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Work(PathBuf);

impl Work {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "evocc-linux-headers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::copy(env!("CARGO_BIN_EXE_evocc"), root.join("bin/evocc")).unwrap();
        std::fs::write(root.join("probe.c"), "#include <stdio.h>\nSELECTED_ROOT\n").unwrap();
        Self(root)
    }

    fn headers(&self, name: &str, value: u32) -> PathBuf {
        let root = self.0.join(name);
        std::fs::create_dir_all(root.join("usr/include")).unwrap();
        std::fs::write(
            root.join("usr/include/stdio.h"),
            format!("#define SELECTED_ROOT {value}\n"),
        )
        .unwrap();
        root
    }

    fn command(&self) -> Command {
        let mut command = Command::new(self.0.join("bin/evocc"));
        command
            .current_dir(&self.0)
            .env_remove("EVOCC_SYSROOT")
            .env_remove("EVOCC_BENCH_SYSROOT")
            .env_remove("EVOCC_CC")
            .env_remove("EVOCC_AS")
            .env("PATH", self.0.join("empty-path"))
            .args(["--error-format=short", &format!("--target={TARGET}")]);
        command
    }

    fn selected(&self, output: Output, root: &Path, value: u32) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stderr}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(&value.to_string()));
        assert!(
            stderr.contains(&format!(
                "Sysroot: {}",
                root.canonicalize().unwrap().display()
            )),
            "{stderr}"
        );
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn explicit_then_environment_then_bundled_headers() {
    let work = Work::new();
    let bundled = work.headers(&format!("sysroot/{TARGET}"), 11);
    let environment = work.headers("environment", 22);
    let explicit = work.headers("explicit", 33);
    work.selected(
        work.command()
            .args(["-v", "-E", "probe.c"])
            .output()
            .unwrap(),
        &bundled,
        11,
    );
    work.selected(
        work.command()
            .env("EVOCC_SYSROOT", &environment)
            .args(["-v", "-E", "probe.c"])
            .output()
            .unwrap(),
        &environment,
        22,
    );
    work.selected(
        work.command()
            .env("EVOCC_SYSROOT", &environment)
            .arg(format!("--sysroot={}", explicit.display()))
            .args(["-v", "-E", "probe.c"])
            .output()
            .unwrap(),
        &explicit,
        33,
    );
}

#[test]
fn a_missing_selected_root_does_not_fall_back() {
    let work = Work::new();
    work.headers(&format!("sysroot/{TARGET}"), 11);
    for explicit in [false, true] {
        let mut command = work.command();
        if explicit {
            command.arg("--sysroot=missing");
        } else {
            command.env("EVOCC_SYSROOT", "missing");
        }
        let output = command.args(["-E", "probe.c"]).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{stderr}");
        assert!(stderr.contains("Linux sysroot"), "{stderr}");
        for remedy in [
            "--sysroot",
            "EVOCC_SYSROOT",
            "sysroot/aarch64-unknown-linux-gnu",
        ] {
            assert!(stderr.contains(remedy), "{stderr}");
        }
    }
}

#[test]
fn no_standard_includes_needs_no_sysroot() {
    let work = Work::new();
    std::fs::write(work.0.join("plain.c"), "int answer(void) { return 42; }\n").unwrap();
    let output = work
        .command()
        .args(["-nostdinc", "-E", "plain.c"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn resource_and_fixed_headers_precede_library_headers() {
    let work = Work::new();
    let root = work.headers(&format!("sysroot/{TARGET}"), 11);
    for (directory, file, body) in [
        (
            "usr/lib/gcc/aarch64-linux-gnu/14/include",
            "stddef.h",
            "#define RESOURCE 14\n",
        ),
        (
            "usr/lib/gcc/aarch64-linux-gnu/14/include-fixed",
            "fixed.h",
            "#define FIXED 15\n",
        ),
        (
            "usr/include/aarch64-linux-gnu",
            "fixed.h",
            "#error wrong header order\n",
        ),
    ] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
        std::fs::write(root.join(directory).join(file), body).unwrap();
    }
    std::fs::write(
        work.0.join("probe.c"),
        "#include <stddef.h>\n#include <fixed.h>\nRESOURCE + FIXED\n",
    )
    .unwrap();
    let output = work.command().args(["-E", "probe.c"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tokens = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .collect::<String>();
    assert!(tokens.contains("14+15"), "{tokens}");
}

#[test]
#[cfg(not(target_os = "linux"))]
fn cross_link_requires_an_explicit_linker_before_selecting_tools() {
    let work = Work::new();
    std::fs::write(work.0.join("input.o"), []).unwrap();
    let output = work.command().arg("input.o").output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("--linker="), "{stderr}");
    assert!(stderr.contains("-c"), "{stderr}");
    assert!(!work.0.join("a.out").exists());
}

#[test]
#[cfg(unix)]
fn preprocessed_assembly_receives_the_selected_header_root() {
    use std::os::unix::fs::PermissionsExt;
    let work = Work::new();
    let root = work.headers(&format!("sysroot/{TARGET}"), 11);
    std::fs::write(work.0.join("input.S"), "#include <stdio.h>\n").unwrap();
    let assembler = work.0.join("record-assembler");
    std::fs::write(
        &assembler,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > assembler-args\n",
    )
    .unwrap();
    std::fs::set_permissions(&assembler, std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = work
        .command()
        .arg(format!("--assembler={}", assembler.display()))
        .args(["-c", "input.S"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let arguments = std::fs::read_to_string(work.0.join("assembler-args")).unwrap();
    assert!(
        arguments.lines().any(|line| {
            line == format!("--sysroot={}", root.canonicalize().unwrap().display())
        }),
        "{arguments}"
    );
}
