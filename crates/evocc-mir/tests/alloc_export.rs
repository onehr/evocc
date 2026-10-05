//! The allocation export uses the same input boundary and the same post-SSA
//! snapshot as the V02 machine export. It is not a proof; the Lean checker
//! checks the export.
use std::{fs, process::Command};

#[test]
fn allocation_export_embeds_the_post_ssa_snapshot_and_refuses_wrong_digests() {
    let directory =
        std::env::temp_dir().join(format!("evocc-alloc-export-test-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("input.evo");
    let input = "func @choose(%arg0: bits<32>) -> (bits<32>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: bits<32>):\n    %v1 = const.bits<32> 19\n    br ^b1(%v0)\n\n^b1(%v2: bits<32>):\n    ret %v2\n}\n";
    let parsed = evocc_evoir::parse_section9_module(input.as_bytes()).unwrap();
    let input = evocc_evoir::render_module(&parsed);
    fs::write(&path, &input).unwrap();
    let digest = evocc_proof::sha256_hex(input.as_bytes());

    let export = Command::new(env!("CARGO_BIN_EXE_evo-alloc-export"))
        .arg(&digest)
        .arg(&path)
        .output()
        .unwrap();
    assert!(export.status.success(), "{export:?}");
    let snapshot = Command::new(env!("CARGO_BIN_EXE_evo-ir-asm"))
        .args(["linux-aarch64", &digest])
        .arg(&path)
        .arg("--emit=mir-post-ssa-json")
        .output()
        .unwrap();
    assert!(snapshot.status.success(), "{snapshot:?}");
    let text = String::from_utf8(export.stdout).unwrap();
    let pre = String::from_utf8(snapshot.stdout).unwrap();
    let prefix = format!("[\"core-alloc-pair-v1\",{},", pre.trim_end());
    assert!(text.starts_with(&prefix), "{text}");
    assert!(text.contains("[\"core-alloc-v1\",\"aarch64-unknown-linux-gnu\",\"checked\","));

    let refused = Command::new(env!("CARGO_BIN_EXE_evo-alloc-export"))
        .arg("incorrect")
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(refused.stdout.is_empty());
    fs::remove_dir_all(directory).unwrap();
}
