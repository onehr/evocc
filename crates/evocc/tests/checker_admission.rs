//! Package-local Cargo dependency and trust admission, without a prebuilt sibling.
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "evocc-checker-admission-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("main.c"), "int main(void) { return 19 + 0; }\n").unwrap();
        Self(dir)
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_evocc"));
        cmd.env_remove("EVOCC_CHECKER_SHA256")
            .args(["--emit=evoir", "-Oeq-scalar"])
            .arg(self.0.join("main.c"));
        cmd
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn trusted() -> String {
    evocc_proof::sha256_hex(&fs::read(env!("CARGO_BIN_EXE_evocc-checker")).unwrap())
}

#[test]
fn cargo_builds_packaged_checker_and_pinned_default_path_executes() {
    let f = Fixture::new();
    let out = f
        .command()
        .arg(format!("--eq-checker-sha256={}", trusted()))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.stdout.is_empty());
}

#[test]
fn missing_or_wrong_pin_cannot_emit_code() {
    let f = Fixture::new();
    for pin in [None, Some("0".repeat(64)), Some("malformed".into())] {
        let mut cmd = f.command();
        if let Some(pin) = pin {
            cmd.arg(format!("--eq-checker-sha256={pin}"));
        }
        let out = cmd.output().unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("SHA-256")
                || String::from_utf8_lossy(&out.stderr).contains("sha256")
        );
    }
}

#[cfg(unix)]
#[test]
fn substituted_always_success_checker_is_not_executed() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let fake = f.0.join("checker");
    let marker = f.0.join("executed");
    fs::write(
        &fake,
        format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let out = f
        .command()
        .arg(format!("--eq-checker={}", fake.display()))
        .arg(format!("--eq-checker-sha256={}", trusted()))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(
        !marker.exists(),
        "untrusted executable ran before admission"
    );
}

#[test]
fn cost_source_conflicts_reject_in_both_argument_orders() {
    let f = Fixture::new();
    let a = "--eq-policy=instruction-count";
    let b = "--eq-weights=add=1,sub=1,mul=1,and=1,or=1,xor=1,shl=1,lshr=1,ashr=1,zext=1,sext=1,trunc=1,icmp=1";
    for args in [[a, b], [b, a], [a, a], [b, b]] {
        let out = f.command().args(args).output().unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("exclusive"));
    }
}

#[test]
fn bundle_publication_failure_keeps_original_rejection() {
    let f = Fixture::new();
    let out = f
        .command()
        .arg(format!("--eq-proof-dir={}", f.0.display()))
        .arg(format!("--eq-checker-sha256={}", "0".repeat(64)))
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(error.contains("does not match trusted identity"), "{error}");
    assert!(error.contains("additionally"), "{error}");
    assert!(f.0.join("main.c").is_file());
}
