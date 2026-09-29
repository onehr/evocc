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
            identity_cache: std::sync::Mutex::new(None),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn locate() -> Result<Self, String> {
        let mut directory = std::env::current_exe().map_err(|e| e.to_string())?;
        directory.pop();
        if directory.ends_with("deps") {
            directory.pop();
        }
        let binary = directory.join(format!("evo-proofcheck{}", std::env::consts::EXE_SUFFIX));
        if !binary.is_file() {
            return Err(format!(
                "{} does not exist; build the standalone checker",
                binary.display()
            ));
        }
        Ok(Self::new(binary, std::env::temp_dir()))
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
        match self.check_typed(before, after, proof) {
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
        if self.timeout.is_zero() {
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
                Ok(None) if start.elapsed() < self.timeout => {
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
