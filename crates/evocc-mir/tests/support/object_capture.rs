//! Bounded, opt-in object retention for the two existing C-corpus tests.
//!
//! This shared test module has consumers which use different subsets of it.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const CAPTURE_LIMIT_BYTES: u64 = 500 * 1024 * 1024;
const EXPECTED_CASES: usize = 384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMode {
    Record,
    Compare,
}

#[derive(Debug)]
pub struct CaptureConfig {
    directory: PathBuf,
    mode: CaptureMode,
}

impl CaptureConfig {
    pub fn parse(directory: Option<&OsStr>, mode: Option<&OsStr>) -> Result<Option<Self>, String> {
        let (directory, mode) = match (directory, mode) {
            (None, None) => return Ok(None),
            (Some(directory), Some(mode)) => (PathBuf::from(directory), mode),
            _ => return Err("object capture requires both DIR and MODE".into()),
        };
        if !directory.is_absolute() || directory.parent().is_none() {
            return Err("object capture requires an absolute, non-root directory".into());
        }
        let mode = match mode.to_str() {
            Some("record") => CaptureMode::Record,
            Some("compare") => CaptureMode::Compare,
            _ => return Err("object capture mode must be record or compare".into()),
        };
        Ok(Some(Self { directory, mode }))
    }

    pub fn from_env() -> Result<Option<Self>, String> {
        let directory = std::env::var_os("EVOCC_MIR_OBJECT_CAPTURE_DIR");
        let mode = std::env::var_os("EVOCC_MIR_OBJECT_CAPTURE_MODE");
        Self::parse(directory.as_deref(), mode.as_deref())
    }

    pub fn start(&self, target: &str) -> Result<ObjectCapture, String> {
        ObjectCapture::new(&self.directory, target, self.mode, CAPTURE_LIMIT_BYTES)
    }
}

/// Only directories created by this invocation are eligible for cleanup.
struct OwnedDirectory(PathBuf);

impl OwnedDirectory {
    fn create(path: PathBuf) -> Result<Self, String> {
        std::fs::create_dir(&path)
            .map_err(|error| format!("create fresh {}: {error}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn directory_bytes(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.is_symlink() {
        return Err(format!("capture path is a symlink: {}", path.display()));
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Err(format!(
            "capture path is not a regular file/directory: {}",
            path.display()
        ));
    }
    let mut bytes = 0u64;
    for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        bytes = bytes
            .checked_add(directory_bytes(&path)?)
            .ok_or("capture byte count overflow")?;
    }
    Ok(bytes)
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

pub struct ObjectCapture {
    root: PathBuf,
    target: String,
    mode: CaptureMode,
    baseline: PathBuf,
    pending: Option<OwnedDirectory>,
    scratch: OwnedDirectory,
    seen: BTreeMap<String, String>,
    limit: u64,
    retained_bytes: u64,
    object_bytes: u64,
    failed: bool,
}

impl ObjectCapture {
    pub fn new(root: &Path, target: &str, mode: CaptureMode, limit: u64) -> Result<Self, String> {
        if !root.is_absolute() || root.parent().is_none() {
            return Err("capture root must be an absolute non-root directory".into());
        }
        if !matches!(target, "darwin" | "linux") {
            return Err("unknown corpus capture target".into());
        }
        std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
        let retained_bytes = directory_bytes(root)?;
        if retained_bytes > limit {
            return Err("capture already exceeds the retention limit".into());
        }
        let baseline = root.join("baseline").join(target);
        match mode {
            CaptureMode::Record if baseline.exists() => {
                return Err("record refuses to overwrite an existing baseline".into());
            }
            CaptureMode::Compare if !baseline.is_dir() => {
                return Err("compare requires an existing baseline".into());
            }
            _ => {}
        }
        for parent in ["baseline", "pending", "scratch"] {
            std::fs::create_dir_all(root.join(parent)).map_err(|error| error.to_string())?;
        }
        let pending = if mode == CaptureMode::Record {
            Some(OwnedDirectory::create(root.join("pending").join(target))?)
        } else {
            None
        };
        let scratch = OwnedDirectory::create(root.join("scratch").join(target))?;
        Ok(Self {
            root: root.to_owned(),
            target: target.to_owned(),
            mode,
            baseline,
            pending,
            scratch,
            seen: BTreeMap::new(),
            limit,
            retained_bytes,
            object_bytes: 0,
            failed: false,
        })
    }

    pub fn scratch_dir(&self) -> &Path {
        &self.scratch.0
    }

    /// Remaining on-disk space inside this capture, checked at phase boundaries.
    pub fn remaining_bytes(&self) -> Result<u64, String> {
        self.limit
            .checked_sub(directory_bytes(&self.root)?)
            .ok_or_else(|| "capture exceeds the retention limit".into())
    }

    pub fn observe_pair(&mut self, id: &str, first: &[u8], second: &[u8]) -> Result<(), String> {
        let result = self.observe_pair_inner(id, first, second);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn observe_pair_inner(&mut self, id: &str, first: &[u8], second: &[u8]) -> Result<(), String> {
        if self.failed {
            return Err("capture has an earlier failure".into());
        }
        if !safe_id(id) || self.seen.contains_key(id) {
            return Err(format!("invalid or duplicate object case ID: {id}"));
        }
        if first != second {
            return Err(format!("object twins differ: {id}"));
        }
        let bytes = u64::try_from(first.len()).map_err(|error| error.to_string())?;
        let total = self
            .object_bytes
            .checked_add(bytes)
            .ok_or("object byte count overflow")?;
        match self.mode {
            CaptureMode::Record => {
                let retained = self
                    .retained_bytes
                    .checked_add(bytes)
                    .ok_or("capture byte count overflow")?;
                if retained > self.limit {
                    return Err(format!("capture retention limit exceeded by {id}"));
                }
                let path = self.pending.as_ref().unwrap().0.join(format!("{id}.o"));
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|error| error.to_string())?;
                file.write_all(first).map_err(|error| error.to_string())?;
                self.retained_bytes = retained;
            }
            CaptureMode::Compare => {
                let path = self.baseline.join(format!("{id}.o"));
                let baseline = std::fs::read(&path)
                    .map_err(|error| format!("read baseline {}: {error}", path.display()))?;
                if baseline != first {
                    return Err(format!("cross-revision object bytes differ: {id}"));
                }
            }
        }
        self.object_bytes = total;
        self.seen
            .insert(id.to_owned(), evocc_proof::sha256_hex(first));
        Ok(())
    }

    pub fn finish(
        mut self,
        expected: &[String],
        executed: &[String],
        agreeing: &[String],
    ) -> Result<(), String> {
        if self.failed {
            return Err("capture has an earlier failure".into());
        }
        let unique = |label: &str, values: &[String]| -> Result<BTreeSet<String>, String> {
            let set: BTreeSet<_> = values.iter().cloned().collect();
            if values.len() != EXPECTED_CASES || set.len() != EXPECTED_CASES {
                return Err(format!("{label} must contain exactly 384 unique case IDs"));
            }
            Ok(set)
        };
        let expected = unique("expected population", expected)?;
        if unique("executed population", executed)? != expected
            || unique("agreeing population", agreeing)? != expected
            || self.seen.keys().cloned().collect::<BTreeSet<_>>() != expected
        {
            return Err("object/execution/agreement case IDs differ from the population".into());
        }
        let objects = self
            .pending
            .as_ref()
            .map_or(&self.baseline, |directory| &directory.0);
        let mut stored = BTreeSet::new();
        for entry in std::fs::read_dir(objects).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("baseline contains a non-file entry".into());
            }
            let name = entry.file_name();
            let id = name
                .to_str()
                .and_then(|name| name.strip_suffix(".o"))
                .ok_or("baseline contains a non-object entry")?;
            stored.insert(id.to_owned());
        }
        if stored != expected {
            return Err("stored object IDs differ from the complete population".into());
        }
        self.remaining_bytes()?;
        if let Some(pending) = self.pending.take() {
            std::fs::rename(&pending.0, &self.baseline).map_err(|error| error.to_string())?;
        }
        for (id, digest) in &self.seen {
            println!("objectCaptureCase={} id={id} sha256={digest}", self.target);
        }
        println!(
            "objectCapture={} mode={:?} objects={} executed=384 agreeing=384 bytes={}",
            self.target,
            self.mode,
            self.seen.len(),
            self.object_bytes
        );
        Ok(())
    }
}

/// Day 7 filenames are ordinals/categories, not the corpus's actual IDs.
pub fn memory_case_ids(directory: &Path) -> Result<BTreeMap<PathBuf, String>, String> {
    let manifest = std::fs::read_to_string(directory.join("manifest.tsv"))
        .map_err(|error| error.to_string())?;
    let mut by_path = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for row in manifest.lines() {
        let fields: Vec<_> = row.split('\t').collect();
        if fields.len() != 4 || !safe_id(fields[1]) || !safe_id(fields[2]) {
            return Err("malformed Day 7 source manifest row".into());
        }
        let path = directory.join(fields[1]);
        if !path.is_file()
            || by_path.insert(path, fields[2].to_owned()).is_some()
            || !ids.insert(fields[2])
        {
            return Err("missing or duplicate Day 7 manifest entry".into());
        }
    }
    if by_path.len() != 128 {
        return Err("Day 7 manifest must contain 128 unique case IDs".into());
    }
    Ok(by_path)
}
