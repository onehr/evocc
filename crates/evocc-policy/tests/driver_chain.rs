//! The released CLI and policy fitness must optimize the same chain.
use evocc_eqopt::ExtractionPolicy;
use evocc_policy::{evaluate, Corpus};
use evocc_randgate::ProofChecker;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::PathBuf, process::Command};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("evocc-chain-parity-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_and_evaluator_bind_identical_verified_chain_and_output() {
    let root = Temp::new();
    let bin = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("evocc");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/verified-eqsat-chain.c");
    let plain = Command::new(&bin)
        .arg("--emit=evoir")
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    let identity_dir = root.0.join("identity");
    let identity = Command::new(&bin)
        .arg("--emit=evoir")
        .arg(format!("--eq-proof-dir={}", identity_dir.display()))
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        identity.status.success(),
        "{}",
        String::from_utf8_lossy(&identity.stderr)
    );
    assert_eq!(
        identity.stdout, plain.stdout,
        "identity export must not enable EqSat"
    );
    assert_eq!(
        fs::read(identity_dir.join("input.evo")).unwrap(),
        plain.stdout
    );
    assert_eq!(
        fs::read(identity_dir.join("output.evo")).unwrap(),
        plain.stdout
    );
    assert!(fs::read_to_string(identity_dir.join("chain.json"))
        .unwrap()
        .contains("\"attempted_transactions\":0"));
    fs::write(root.0.join("input.evo"), &plain.stdout).unwrap();
    let corpus = Corpus::from_directory(&root.0, None).unwrap();
    let checker = ProofChecker::locate().unwrap();
    let result = evaluate(&ExtractionPolicy::default(), &corpus, 0, 100_000, &checker);
    assert_eq!(result.aggregate.refused, 0);
    let chain = result.programs[0].chain_record.as_ref().unwrap();
    let proof_dir = root.0.join("proofs");
    let output = Command::new(&bin)
        .arg("--emit=evoir")
        .arg("-Oeq-scalar")
        .arg(format!("--eq-proof-dir={}", proof_dir.display()))
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(proof_dir.join("chain.json")).unwrap(),
        *chain
    );
    let canonical = Command::new("python3").arg("-c").arg("import json,sys; p=sys.argv[1]; s=open(p).read(); assert json.dumps(json.loads(s),sort_keys=True,separators=(',',':')) == s")
        .arg(proof_dir.join("chain.json")).output().unwrap();
    assert!(
        canonical.status.success(),
        "{}",
        String::from_utf8_lossy(&canonical.stderr)
    );
    assert_eq!(
        evocc_proof::sha256_hex(&output.stdout),
        result.programs[0].output_digest
    );
    assert!(
        result.programs[0].transactions >= 2,
        "terminal no-op must be checked"
    );
    for index in 0..result.programs[0].transactions {
        checker
            .check(
                &fs::read_to_string(proof_dir.join(format!("{index:03}.before.evo"))).unwrap(),
                &fs::read_to_string(proof_dir.join(format!("{index:03}.after.evo"))).unwrap(),
                &fs::read_to_string(proof_dir.join(format!("{index:03}.proof.evp"))).unwrap(),
            )
            .unwrap();
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        // Reuse the actual driver-verified IR; do not optimize it again.
        let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        let assembly = evocc_mir::compile_module(&module).unwrap();
        let toolchain = evocc_mir::Toolchain::locate().unwrap();
        let baseline = evocc_evoir::parse_section9_module(&identity.stdout).unwrap();
        let baseline_assembly = evocc_mir::compile_module(&baseline).unwrap();
        let baseline_object = toolchain
            .assemble("identity-chain", &baseline_assembly)
            .unwrap();
        let baseline_native = toolchain.link("identity-chain", &baseline_object).unwrap();
        assert_eq!(toolchain.run(&baseline_native).unwrap(), 0);
        // Optional, explicit retention for the R01a acceptance receipt. An
        // existing destination is never adopted or cleaned by this test.
        let retained = std::env::var_os("EVOCC_R01A_RECEIPT_DIR").map(PathBuf::from);
        if let Some(directory) = &retained {
            fs::create_dir(directory).unwrap();
            fs::write(directory.join("program.c"), fs::read(&source).unwrap()).unwrap();
            fs::write(directory.join("program.s"), &assembly).unwrap();
            for file in fs::read_dir(&proof_dir).unwrap() {
                let file = file.unwrap();
                fs::copy(file.path(), directory.join(file.file_name())).unwrap();
            }
        }
        let object = toolchain.assemble("verified-chain", &assembly).unwrap();
        let object_digest = evocc_proof::sha256_hex(&fs::read(&object).unwrap());
        if let Some(directory) = &retained {
            fs::copy(&object, directory.join("program.o")).unwrap();
        }
        let native = toolchain.link("verified-chain", &object).unwrap();
        let executable_digest = evocc_proof::sha256_hex(&fs::read(&native).unwrap());
        if let Some(directory) = &retained {
            fs::copy(&native, directory.join("program")).unwrap();
        }
        assert_eq!(toolchain.run(&native).unwrap(), 0);
        if let Some(directory) = &retained {
            // Published last; failed execution leaves no PASS receipt.
            let receipt = format!("{{\"schema\":\"evocc-r01a-native-v1\",\"scope\":\"Darwin-arm64-focused-smoke-not-G2\",\"compiler_digest\":\"{}\",\"checker_digest\":\"{}\",\"source_digest\":\"{}\",\"chain_record_digest\":\"{}\",\"verified_ir_digest\":\"{}\",\"assembly_digest\":\"{}\",\"object_digest\":\"{}\",\"executable_digest\":\"{}\",\"exit_code\":0,\"toolchain\":{:?}}}\n",
                evocc_proof::sha256_hex(&fs::read(&bin).unwrap()), checker.identity_digest().unwrap(),
                evocc_proof::sha256_hex(&fs::read(&source).unwrap()), evocc_proof::sha256_hex(chain.as_bytes()),
                result.programs[0].output_digest, evocc_proof::sha256_hex(assembly.as_bytes()),
                object_digest, executable_digest, toolchain.version());
            fs::write(directory.join("native.json"), receipt).unwrap();
        }
        eprintln!("R01a native Darwin arm64: compiler={} checker={} chain={} verified_ir={} object={} executable={} exit=0 toolchain={}",
            evocc_proof::sha256_hex(&fs::read(&bin).unwrap()),
            checker.identity_digest().unwrap(), evocc_proof::sha256_hex(chain.as_bytes()),
            result.programs[0].output_digest, object_digest, executable_digest, toolchain.version());
    }
    let again = Command::new(&bin)
        .arg("--emit=evoir")
        .arg("-Oeq-scalar")
        .arg(format!("--eq-proof-dir={}", proof_dir.display()))
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        !again.status.success(),
        "must not overwrite previous evidence"
    );
    assert!(again.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(proof_dir.join("chain.json")).unwrap(),
        *chain
    );
    let failed_dir = root.0.join("missing-checker");
    let missing = Command::new(&bin)
        .arg("--emit=asm")
        .arg("-Oeq-scalar")
        .arg(format!("--eq-proof-dir={}", failed_dir.display()))
        .arg(format!("--eq-checker={}", root.0.join("absent").display()))
        .arg(&source)
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(
        missing.stdout.is_empty(),
        "no assembly may escape verification failure"
    );
    assert!(fs::read_to_string(failed_dir.join("chain.json"))
        .unwrap()
        .contains("\"publication\":\"REJECTED\""));
}

#[test]
fn invalid_policy_options_fail_before_emission() {
    let bin = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("evocc");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/verified-eqsat-chain.c");
    for option in [
        "--eq-max-transactions=0",
        "--eq-max-transactions=9",
        "--eq-max-nodes=100001",
        "--eq-max-epochs=1025",
        "--eq-checker-timeout-ms=0",
        "--eq-policy=unknown",
        "--eq-surprise=1",
        "--eq-weights=1,2",
        "--eq-max-nodes=-1",
    ] {
        let out = Command::new(&bin)
            .args(["--emit=asm", "-Oeq-scalar", option])
            .arg(&source)
            .output()
            .unwrap();
        assert!(!out.status.success(), "accepted {option}");
        assert!(out.stdout.is_empty());
    }
    let out = Command::new(&bin)
        .args(["--emit=asm", "--eq-max-transactions=2"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "cannot ignore an inactive policy flag"
    );
    assert!(out.stdout.is_empty());
}
