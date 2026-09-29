//! Bounded invocation of the independent checker, not a second proof checker.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckerOutcome {
    Valid,
    Malformed,
    Invalid,
    Timeout,
    Unavailable,
    Error,
}

impl CheckerOutcome {
    pub const fn from_exit(code: Option<i32>) -> Self {
        match code {
            Some(0) => Self::Valid,
            Some(2) => Self::Malformed,
            Some(3) => Self::Invalid,
            _ => Self::Error,
        }
    }
}

#[derive(Debug)]
pub struct StandaloneProofChecker {
    binary: PathBuf,
    scratch: PathBuf,
    timeout: Duration,
    legacy_timeout: Option<Duration>,
    expected_digest: Option<String>,
    identity_cache: std::sync::Mutex<Option<(Vec<u8>, String)>>,
}

/// Only this newly created, private directory is ever recursively removed.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl StandaloneProofChecker {
    pub fn new(binary: PathBuf, scratch: PathBuf) -> Self {
        Self {
            binary,
            scratch,
            timeout: Duration::from_secs(30),
            legacy_timeout: None,
            expected_digest: None,
            identity_cache: std::sync::Mutex::new(None),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self.legacy_timeout = Some(timeout);
        self
    }

    /// Trust comes from the build/release caller, never from a certificate or
    /// from observing whichever executable happens to be next to the driver.
    pub fn with_expected_digest(mut self, digest: String) -> Self {
        self.expected_digest = Some(digest);
        self
    }

    pub fn admitted_identity(&self) -> Result<String, String> {
        let expected = self.expected_digest.as_deref()
            .ok_or("missing trusted checker SHA-256; supply an independently selected build/release identity")?;
        if expected.len() != 64
            || !expected
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid trusted checker SHA-256".into());
        }
        let actual = self.identity_digest()?;
        if actual != expected {
            return Err("checker SHA-256 does not match trusted identity".into());
        }
        Ok(actual)
    }

    pub fn locate() -> Result<Self, String> {
        Self::locate_named("evo-proofcheck")
    }

    pub fn locate_named(name: &str) -> Result<Self, String> {
        let mut directory = std::env::current_exe().map_err(|e| e.to_string())?;
        directory.pop();
        if directory.ends_with("deps") {
            directory.pop();
        }
        let binary = directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        if !binary.is_file() {
            return Err(format!(
                "{} does not exist; build the standalone checker",
                binary.display()
            ));
        }
        let mut checker = Self::new(binary, std::env::temp_dir());
        if let Ok(expected) = std::env::var("EVOCC_CHECKER_SHA256") {
            checker = checker.with_expected_digest(expected);
        }
        if let Ok(value) = std::env::var("EVOCC_CHECKER_TIMEOUT_MS") {
            let millis: u64 = value.parse().map_err(|_| "invalid checker deadline")?;
            if !(1..=300_000).contains(&millis) {
                return Err("checker deadline must be 1..300000 ms".into());
            }
            checker = checker.with_timeout(Duration::from_millis(millis));
        }
        Ok(checker)
    }

    pub fn binary(&self) -> &PathBuf {
        &self.binary
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn identity_digest(&self) -> Result<String, String> {
        let bytes = fs::read(&self.binary).map_err(|e| e.to_string())?;
        let mut cache = self.identity_cache.lock().map_err(|e| e.to_string())?;
        // Re-read and compare every byte, not just length/mtime. Cache only the
        // expensive hash; replacing a checker with equal-sized contents is seen.
        if let Some((previous, digest)) = cache.as_ref() {
            if previous == &bytes {
                return Ok(digest.clone());
            }
        }
        let digest = crate::sha256_hex(&bytes);
        *cache = Some((bytes, digest.clone()));
        Ok(digest)
    }

    pub fn check(&self, before: &str, after: &str, proof: &str) -> Result<(), String> {
        // Legacy randgate callers had no deadline. Only an explicitly chosen
        // deadline changes that contract; verified chains always use check_typed.
        match self.invoke(before, after, proof, self.legacy_timeout) {
            CheckerOutcome::Valid => Ok(()),
            CheckerOutcome::Malformed => {
                Err("evo-proofcheck reported malformed input (status 2)".into())
            }
            CheckerOutcome::Invalid => {
                Err("evo-proofcheck refused the certificate (status 3)".into())
            }
            result => Err(format!("independent checker: {result:?}")),
        }
    }

    pub fn check_typed(&self, before: &str, after: &str, proof: &str) -> CheckerOutcome {
        self.invoke(before, after, proof, Some(self.timeout))
    }

    fn invoke(
        &self,
        before: &str,
        after: &str,
        proof: &str,
        timeout: Option<Duration>,
    ) -> CheckerOutcome {
        if timeout.is_some_and(|t| t.is_zero()) {
            return CheckerOutcome::Error;
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = self.scratch.join(format!(
            "evocc-proof-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // create_dir, not create_dir_all: never adopt a pre-existing workspace.
        if fs::create_dir(&directory).is_err() {
            return CheckerOutcome::Error;
        }
        let scratch = Scratch(directory);
        if write_inputs(&scratch.0, before, after, proof).is_err() {
            return CheckerOutcome::Error;
        }
        let mut child = match Command::new(&self.binary)
            .args([
                scratch.0.join("before.evo"),
                scratch.0.join("after.evo"),
                scratch.0.join("proof.evp"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return CheckerOutcome::Unavailable,
        };
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return CheckerOutcome::from_exit(status.code()),
                Ok(None) if timeout.is_none_or(|t| start.elapsed() < t) => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return if result.is_err() {
                        CheckerOutcome::Error
                    } else {
                        CheckerOutcome::Timeout
                    };
                }
            }
        }
    }
}

fn write_inputs(dir: &Path, before: &str, after: &str, proof: &str) -> std::io::Result<()> {
    fs::write(dir.join("before.evo"), before)?;
    fs::write(dir.join("after.evo"), after)?;
    fs::write(dir.join("proof.evp"), proof)
}
