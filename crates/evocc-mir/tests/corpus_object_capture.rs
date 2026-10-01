//! N03a's opt-in corpus capture must compare complete objects and populations.
//! These are filesystem-only tests: no compiler, assembler or container runs.

#[path = "support/object_capture.rs"]
mod object_capture;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use object_capture::{
    memory_case_ids, CaptureConfig, CaptureMode, ObjectCapture, CAPTURE_LIMIT_BYTES,
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "evocc-object-capture-test-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("a fresh test-owned directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ids() -> Vec<String> {
    (0..384)
        .map(|index| format!("defined.capture.{index:03}"))
        .collect()
}

fn bytes(index: usize) -> Vec<u8> {
    // The byte comparator must not reinterpret, normalize or strip the object.
    let mut bytes = b"complete object bytes\0".to_vec();
    bytes.extend_from_slice(&(index as u32).to_le_bytes());
    bytes
}

fn populate(capture: &mut ObjectCapture, names: &[String]) {
    for (index, id) in names.iter().enumerate() {
        let object = bytes(index);
        capture.observe_pair(id, &object, &object).unwrap();
    }
}

fn record(root: &Path) {
    let names = ids();
    let mut capture =
        ObjectCapture::new(root, "darwin", CaptureMode::Record, CAPTURE_LIMIT_BYTES).unwrap();
    populate(&mut capture, &names);
    capture.finish(&names, &names, &names).unwrap();
}

#[test]
fn full_population_records_one_object_per_id_and_compares_across_runs() {
    let scratch = Scratch::new();
    record(&scratch.0);
    let baseline = scratch.0.join("baseline/darwin");
    assert_eq!(std::fs::read_dir(&baseline).unwrap().count(), 384);
    let names = ids();
    let mut capture = ObjectCapture::new(
        &scratch.0,
        "darwin",
        CaptureMode::Compare,
        CAPTURE_LIMIT_BYTES,
    )
    .unwrap();
    populate(&mut capture, &names);
    capture.finish(&names, &names, &names).unwrap();
    assert_eq!(std::fs::read_dir(&baseline).unwrap().count(), 384);
}

#[test]
fn same_size_cross_revision_mutation_is_rejected_without_touching_baseline() {
    let scratch = Scratch::new();
    record(&scratch.0);
    let id = &ids()[0];
    let baseline = scratch.0.join("baseline/darwin").join(format!("{id}.o"));
    let original = std::fs::read(&baseline).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    let mut capture = ObjectCapture::new(
        &scratch.0,
        "darwin",
        CaptureMode::Compare,
        CAPTURE_LIMIT_BYTES,
    )
    .unwrap();
    assert!(capture.observe_pair(id, &changed, &changed).is_err());
    drop(capture);
    assert_eq!(std::fs::read(&baseline).unwrap(), original);
}

#[test]
fn unequal_twins_are_not_a_recordable_baseline() {
    let scratch = Scratch::new();
    let mut capture = ObjectCapture::new(
        &scratch.0,
        "linux",
        CaptureMode::Record,
        CAPTURE_LIMIT_BYTES,
    )
    .unwrap();
    assert!(capture.observe_pair(&ids()[0], b"same", b"sAme").is_err());
    drop(capture);
    assert!(!scratch.0.join("baseline/linux").exists());
    assert!(!scratch.0.join("pending/linux").exists());
}

#[test]
fn missing_one_object_cannot_be_promoted_to_a_complete_baseline() {
    let scratch = Scratch::new();
    let names = ids();
    let mut capture = ObjectCapture::new(
        &scratch.0,
        "darwin",
        CaptureMode::Record,
        CAPTURE_LIMIT_BYTES,
    )
    .unwrap();
    populate(&mut capture, &names[..383]);
    assert!(capture.finish(&names, &names, &names).is_err());
    assert!(!scratch.0.join("baseline/darwin").exists());
    assert!(!scratch.0.join("pending/darwin").exists());
}

#[test]
fn duplicate_object_or_execution_does_not_satisfy_the_denominator() {
    let scratch = Scratch::new();
    let names = ids();
    let mut capture = ObjectCapture::new(
        &scratch.0,
        "darwin",
        CaptureMode::Record,
        CAPTURE_LIMIT_BYTES,
    )
    .unwrap();
    populate(&mut capture, &names);
    let object = bytes(0);
    assert!(capture.observe_pair(&names[0], &object, &object).is_err());
    assert!(capture.finish(&names, &names, &names).is_err());

    let mut capture = ObjectCapture::new(
        &scratch.0,
        "darwin",
        CaptureMode::Record,
        CAPTURE_LIMIT_BYTES,
    )
    .unwrap();
    populate(&mut capture, &names);
    let mut ran = names.clone();
    ran[383] = ran[0].clone();
    assert!(capture.finish(&names, &ran, &names).is_err());
    assert!(!scratch.0.join("baseline/darwin").exists());
}

#[test]
fn equal_cardinality_with_a_different_case_id_is_not_coverage() {
    let scratch = Scratch::new();
    let names = ids();
    let mut capture = ObjectCapture::new(
        &scratch.0,
        "darwin",
        CaptureMode::Record,
        CAPTURE_LIMIT_BYTES,
    )
    .unwrap();
    populate(&mut capture, &names);
    let mut changed = names.clone();
    changed[0] = "not.the.same.case".into();
    assert!(capture.finish(&names, &changed, &names).is_err());
    assert!(!scratch.0.join("baseline/darwin").exists());
}

#[test]
fn day7_case_identity_comes_from_the_existing_manifest_not_filename() {
    let scratch = Scratch::new();
    let mut manifest = String::new();
    for index in 0..128 {
        let filename = format!("{index:03}-memory.c");
        std::fs::write(scratch.0.join(&filename), b"int main(void) { return 0; }\n").unwrap();
        manifest.push_str(&format!(
            "{index}\t{filename}\tday7.defined.memory.{index:03}\tunused-in-this-reader\n"
        ));
    }
    std::fs::write(scratch.0.join("manifest.tsv"), &manifest).unwrap();
    let names = memory_case_ids(&scratch.0).unwrap();
    assert_eq!(names.len(), 128);
    assert_eq!(
        names[&scratch.0.join("000-memory.c")],
        "day7.defined.memory.000"
    );
    std::fs::write(
        scratch.0.join("manifest.tsv"),
        format!("{manifest}{manifest}"),
    )
    .unwrap();
    assert!(memory_case_ids(&scratch.0).is_err());
}

#[test]
fn agreement_and_expected_ids_must_both_be_complete() {
    let scratch = Scratch::new();
    let names = ids();
    let mut capture = ObjectCapture::new(
        &scratch.0,
        "darwin",
        CaptureMode::Record,
        CAPTURE_LIMIT_BYTES,
    )
    .unwrap();
    populate(&mut capture, &names);
    assert!(capture.finish(&names, &names, &names[..383]).is_err());

    let mut capture = ObjectCapture::new(
        &scratch.0,
        "darwin",
        CaptureMode::Record,
        CAPTURE_LIMIT_BYTES,
    )
    .unwrap();
    populate(&mut capture, &names[..383]);
    assert!(capture
        .finish(&names[..383], &names[..383], &names[..383])
        .is_err());
}

#[test]
fn record_never_overwrites_a_prior_baseline_and_compare_requires_one() {
    let scratch = Scratch::new();
    assert!(ObjectCapture::new(
        &scratch.0,
        "linux",
        CaptureMode::Compare,
        CAPTURE_LIMIT_BYTES,
    )
    .is_err());
    record(&scratch.0);
    let object = scratch.0.join("baseline/darwin/defined.capture.000.o");
    let original = std::fs::read(&object).unwrap();
    assert!(ObjectCapture::new(
        &scratch.0,
        "darwin",
        CaptureMode::Record,
        CAPTURE_LIMIT_BYTES,
    )
    .is_err());
    assert_eq!(std::fs::read(&object).unwrap(), original);
}

#[test]
fn retention_limit_is_checked_before_persisting_the_next_object() {
    assert_eq!(CAPTURE_LIMIT_BYTES, 500 * 1024 * 1024);
    let scratch = Scratch::new();
    let mut capture = ObjectCapture::new(&scratch.0, "linux", CaptureMode::Record, 4).unwrap();
    assert!(capture.observe_pair(&ids()[0], b"large", b"large").is_err());
    assert_eq!(
        std::fs::read_dir(scratch.0.join("pending/linux"))
            .unwrap()
            .count(),
        0
    );
    drop(capture);
    assert!(!scratch.0.join("pending/linux").exists());
}

#[test]
fn capture_configuration_is_opt_in_paired_and_absolute() {
    assert!(CaptureConfig::parse(None, None).unwrap().is_none());
    let scratch = Scratch::new();
    let directory = scratch.0.as_os_str();
    assert!(CaptureConfig::parse(Some(directory), None).is_err());
    assert!(CaptureConfig::parse(None, Some(OsStr::new("record"))).is_err());
    assert!(CaptureConfig::parse(Some(directory), Some(OsStr::new("invalid"))).is_err());
    assert!(
        CaptureConfig::parse(Some(OsStr::new("relative")), Some(OsStr::new("record"))).is_err()
    );
    assert!(
        CaptureConfig::parse(Some(directory), Some(OsStr::new("record")))
            .unwrap()
            .is_some()
    );
    assert!(
        CaptureConfig::parse(Some(directory), Some(OsStr::new("compare")))
            .unwrap()
            .is_some()
    );
}

#[test]
fn fixed_assembler_directory_is_an_explicit_toolchain_entry_point() {
    // Compile-time interface check only; the pure file suite never finds clang.
    let _locate: fn(&Path) -> Result<evocc_mir::Toolchain, String> =
        evocc_mir::Toolchain::locate_in;
}
