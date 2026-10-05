//! The assembly AST exporter uses the actual native pipeline and refuses
//! inputs outside the checked Linux core profile.
use std::{fs, path::PathBuf, process::Command};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/verification")
        .join(name)
}

fn run(binary: &str, args: &[&str]) -> std::process::Output {
    Command::new(binary).args(args).output().unwrap()
}

#[test]
fn exported_text_is_the_compiler_text_and_variants_refuse() {
    let directory = std::env::temp_dir().join(format!("evocc-asm-ast-test-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    for name in ["machine-private.evo", "machine-private-call-loop.evo"] {
        let input = fixture(name);
        let bytes = fs::read(&input).unwrap();
        let digest = evocc_proof::sha256_hex(&bytes);
        let input = input.to_str().unwrap();
        let text_path = directory.join(format!("{name}.s"));
        let text_flag = format!("--text={}", text_path.to_str().unwrap());
        let exported = run(
            env!("CARGO_BIN_EXE_evo-asm-ast"),
            &["linux-aarch64", &digest, input, &text_flag],
        );
        assert!(exported.status.success(), "{exported:?}");
        assert!(exported.stdout.starts_with(b"[\"core-asm-v1\","));
        let compiled = run(
            env!("CARGO_BIN_EXE_evo-ir-asm"),
            &["linux-aarch64", &digest, input],
        );
        assert!(compiled.status.success(), "{compiled:?}");
        assert_eq!(fs::read(&text_path).unwrap(), compiled.stdout);
        for args in [
            vec!["darwin-arm64", digest.as_str(), input],
            vec!["linux-aarch64", "incorrect", input],
            vec!["linux-aarch64", digest.as_str(), input, "--unknown"],
        ] {
            let refused = run(env!("CARGO_BIN_EXE_evo-asm-ast"), &args);
            assert_eq!(refused.status.code(), Some(2), "{refused:?}");
            assert!(refused.stdout.is_empty());
        }
    }
    fs::remove_dir_all(directory).unwrap();
}
