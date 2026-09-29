//! Process/exit boundaries; these stubs test invocation, not proof soundness.
#![cfg(unix)]
use evocc_proof::{CheckerOutcome as O, StandaloneProofChecker};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "evocc-checker-test-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn checker(&self, body: &str) -> StandaloneProofChecker {
        let binary = self.0.join("checker");
        fs::write(&binary, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        StandaloneProofChecker::new(binary, self.0.clone())
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn exits_are_distinct_and_scratch_is_released() {
    let dir = Temp::new();
    for (code, expected) in [
        (0, O::Valid),
        (2, O::Malformed),
        (3, O::Invalid),
        (1, O::Error),
    ] {
        assert_eq!(
            dir.checker(&format!("exit {code}"))
                .check_typed("before", "after", "proof"),
            expected
        );
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }
}

#[test]
fn timeout_reaps_child_and_unavailable_does_not_leak_scratch() {
    let dir = Temp::new();
    let checker = dir
        .checker("exec /bin/sleep 5")
        .with_timeout(Duration::from_millis(10));
    for _ in 0..3 {
        assert_eq!(checker.check_typed("b", "a", "p"), O::Timeout);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }
    let missing = StandaloneProofChecker::new(dir.0.join("absent"), dir.0.clone());
    assert_eq!(missing.check_typed("b", "a", "p"), O::Unavailable);
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[test]
fn identity_cache_compares_contents_not_size_or_path() {
    let dir = Temp::new();
    let checker = dir.checker("exit 0");
    let first = checker.identity_digest().unwrap();
    assert_eq!(first, checker.identity_digest().unwrap());
    let original = fs::metadata(checker.binary()).unwrap();
    fs::write(checker.binary(), "#!/bin/sh\nexit 3\n").unwrap();
    assert_eq!(
        fs::metadata(checker.binary()).unwrap().len(),
        original.len()
    );
    assert_ne!(first, checker.identity_digest().unwrap());
    assert_eq!(checker.check_typed("b", "a", "p"), O::Invalid);
}
