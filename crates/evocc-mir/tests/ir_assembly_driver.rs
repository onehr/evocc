//! The native consumer must lower the exact canonical IR, without reoptimizing.
use std::{fs, process::Command};

#[test]
fn canonical_digest_binds_the_actual_backend_input() {
    let directory = std::env::temp_dir().join(format!("evocc-ir-asm-test-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("input.evo");
    let module = evocc_evoir::parse_section9_module(include_bytes!(
        "../../../tests/fixtures/eqsat-performance/identity-tail.evo"
    ))
    .unwrap();
    let text = evocc_evoir::render_module(&module);
    fs::write(&path, &text).unwrap();
    let digest = evocc_proof::sha256_hex(text.as_bytes());
    let noncanonical = format!("{text}\n");
    for target in ["darwin-arm64", "linux-aarch64"] {
        let run = Command::new(env!("CARGO_BIN_EXE_evo-ir-asm"))
            .args([target, &digest])
            .arg(&path)
            .output()
            .unwrap();
        assert!(run.status.success(), "{:?}", run);
        assert!(!run.stdout.is_empty());
    }
    for (target, hash, bytes) in [
        ("unknown", digest.clone(), text.clone()),
        ("darwin-arm64", "0".repeat(64), text.clone()),
        ("darwin-arm64", digest.clone(), text + "\n"),
        (
            "darwin-arm64",
            evocc_proof::sha256_hex(noncanonical.as_bytes()),
            noncanonical,
        ),
        (
            "darwin-arm64",
            evocc_proof::sha256_hex(b"not IR"),
            "not IR".into(),
        ),
    ] {
        fs::write(&path, bytes).unwrap();
        let run = Command::new(env!("CARGO_BIN_EXE_evo-ir-asm"))
            .args([target, &hash])
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(run.status.code(), Some(2));
        assert!(run.stdout.is_empty());
    }
    fs::remove_dir_all(directory).unwrap();
}
