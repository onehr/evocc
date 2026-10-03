//! The native consumer must lower the exact canonical IR, without reoptimizing.
use std::{fs, process::Command};

#[test]
fn copy_hints_are_shared_by_the_exact_ir_adapter_and_diagnostic_path() {
    let directory = std::env::temp_dir().join(format!("evocc-ir-copy-test-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("input.evo");
    let module = evocc_evoir::parse_section9_module(include_bytes!(
        "../../../tests/fixtures/eqsat-performance/identity-tail.evo"
    ))
    .unwrap();
    let text = evocc_evoir::render_module(&module);
    fs::write(&path, &text).unwrap();
    let digest = evocc_proof::sha256_hex(text.as_bytes());
    for target in ["darwin-arm64", "linux-aarch64"] {
        let run = |flags: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_evo-ir-asm"))
                .args([target, &digest])
                .arg(&path)
                .args(flags)
                .output()
                .unwrap()
        };
        for option in ["copy-hints", "reload-reuse", "copy-hints,reload-reuse"] {
            let flag = format!("--regalloc-opt={option}");
            let normal = run(&[&flag]);
            let report = run(&[&flag, "--dce-report"]);
            assert!(normal.status.success(), "{normal:?}");
            assert!(report.status.success(), "{report:?}");
            assert_eq!(normal.stdout, report.stdout);
            assert!(String::from_utf8_lossy(&normal.stdout)
                .contains(&format!("// evocc-native-regalloc={option}\n")));
            assert!(String::from_utf8_lossy(&report.stderr)
                .contains("PRODUCER_DIAGNOSTIC_NOT_EQSAT_BENEFIT"));
        }
        for flags in [
            vec!["--regalloc-opt="],
            vec!["--regalloc-opt=unknown"],
            vec!["--regalloc-opt=copy-hints,copy-hints"],
            vec!["--regalloc-opt=copy-hints", "--regalloc-opt=copy-hints"],
        ] {
            let output = run(&flags);
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
        }
    }
    assert_eq!(fs::read_to_string(path).unwrap(), text);
    fs::remove_dir_all(directory).unwrap();
}

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

#[test]
fn constant_selection_is_shared_by_the_exact_ir_adapter() {
    let directory =
        std::env::temp_dir().join(format!("evocc-ir-constant-test-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("input.evo");
    let module = evocc_evoir::parse_section9_module(b"func @constant() -> (bits<64>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry():\n    %v0 = const.bits<64> 18446744073709551615\n    ret %v0\n}\n").unwrap();
    let text = evocc_evoir::render_module(&module);
    fs::write(&path, &text).unwrap();
    let digest = evocc_proof::sha256_hex(text.as_bytes());
    for target in ["darwin-arm64", "linux-aarch64"] {
        for frames in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_evo-ir-asm"));
            command
                .args([target, &digest])
                .arg(&path)
                .arg("--isel-opt=constants");
            if frames {
                command.arg("--frame-opt=paired-saves,shared-epilogue,leaf-frames");
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{output:?}");
            let assembly = String::from_utf8(output.stdout).unwrap();
            assert!(assembly.contains("// evocc-native-isel=constants\n"));
            assert!(assembly.contains("movn x0, #0\n"), "{assembly}");
            assert!(!assembly.contains("movk"), "{assembly}");
        }
        for flags in [
            vec!["--isel-opt="],
            vec!["--isel-opt=unknown"],
            vec!["--isel-opt=constants,constants"],
            vec!["--isel-opt=constants", "--isel-opt=constants"],
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_evo-ir-asm"))
                .args([target, &digest])
                .arg(&path)
                .args(flags)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
        }
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn local_selection_uses_the_same_canonical_option_list_without_changing_ir() {
    let directory =
        std::env::temp_dir().join(format!("evocc-ir-local-test-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("input.evo");
    let module = evocc_evoir::parse_section9_module(b"func @product(%arg0: bits<64>, %arg1: bits<64>) -> (bits<64>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: bits<64>, %v1: bits<64>):\n    %v2 = mul.bits<64> %v0, %v1\n    %v3 = add.bits<64> %v0, %v2\n    ret %v3\n}\n").unwrap();
    let text = evocc_evoir::render_module(&module);
    fs::write(&path, &text).unwrap();
    let digest = evocc_proof::sha256_hex(text.as_bytes());
    for target in ["darwin-arm64", "linux-aarch64"] {
        let mut outputs = vec![];
        for flags in ["local,constants", "constants,local"] {
            let output = Command::new(env!("CARGO_BIN_EXE_evo-ir-asm"))
                .args([target, &digest])
                .arg(&path)
                .arg(format!("--isel-opt={flags}"))
                .arg("--frame-opt=paired-saves,shared-epilogue,leaf-frames")
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let assembly = String::from_utf8(output.stdout).unwrap();
            assert!(assembly.contains("// evocc-native-isel=constants,local\n"));
            assert!(assembly.contains("    madd "), "{assembly}");
            assert!(assembly.contains("// evocc-local-selected local_multiply_arithmetic=1"));
            outputs.push(assembly);
        }
        assert_eq!(outputs[0], outputs[1]);
        let output = Command::new(env!("CARGO_BIN_EXE_evo-ir-asm"))
            .args([target, &digest])
            .arg(&path)
            .arg("--isel-opt=local,local")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn extension_selection_reuses_the_exact_ir_adapter_without_rewriting_its_input() {
    let directory =
        std::env::temp_dir().join(format!("evocc-ir-extension-test-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("input.evo");
    let text="func @duplicate(%arg0: bits<8>) -> (bits<64>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: bits<8>):\n    %v1 = sext.bits<64> %v0\n    %v2 = sext.bits<64> %v0\n    %v3 = add.bits<64> %v1, %v2\n    ret %v3\n}\n";
    fs::write(&path, text).unwrap();
    let digest = evocc_proof::sha256_hex(text.as_bytes());
    for target in ["darwin-arm64", "linux-aarch64"] {
        for flags in [
            "extensions",
            "constants,extensions",
            "local,extensions",
            "constants,local,extensions",
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_evo-ir-asm"))
                .args([target, &digest])
                .arg(&path)
                .arg(format!("--isel-opt={flags}"))
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let assembly = String::from_utf8(output.stdout).unwrap();
            assert!(
                assembly.contains(&format!("// evocc-native-isel={flags}\n")),
                "{assembly}"
            );
            assert!(
                assembly.contains("// evocc-local-selected reuse_integer_extend=1"),
                "{assembly}"
            );
        }
    }
    assert_eq!(fs::read_to_string(path).unwrap(), text);
    fs::remove_dir_all(directory).unwrap();
}
