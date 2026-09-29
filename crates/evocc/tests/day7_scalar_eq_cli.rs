use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const TARGET: &str = "--target=aarch64-unknown-linux-gnu";
const SOURCE: &str = "int main(void) { int x = 19; int add_zero = x + 0; int mul_one = add_zero * 1; int and_zero = mul_one & 0; int or_zero = mul_one | 0; return or_zero + and_zero; }\n";

struct TempSource(PathBuf);

impl TempSource {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "evocc-day7-scalar-eq-{}-{nonce}.c",
            std::process::id()
        ));
        fs::write(&path, SOURCE).unwrap();
        Self(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for TempSource {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_evocc"))
        .env(
            "EVOCC_CHECKER_SHA256",
            evocc_proof::sha256_hex(&fs::read(env!("CARGO_BIN_EXE_evocc-checker")).unwrap()),
        )
        .args(args)
        .output()
        .expect("run evocc")
}

#[test]
fn exact_scalar_eq_probes_are_green_while_broad_and_near_miss_probes_stay_red() {
    for args in [
        &["-Oeq-scalar", "--probe"][..],
        &["eq-scalar-optimize", "--probe"][..],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(0), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(output.stderr.is_empty(), "{args:?}");
    }
    for args in [
        &["-Oeq", "--probe"][..],
        &["eqsat-optimize", "--probe"][..],
        &["-Oeq-scalarXYZ", "--probe"][..],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(69), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("NOT_IMPLEMENTED"));
    }
}

#[test]
fn scalar_eq_flag_preserves_execution_and_emits_the_extracted_evoir() {
    let source = TempSource::new();
    let baseline = run(&[TARGET, "--interpret-evoir", source.path()]);
    let optimized = run(&[TARGET, "-Oeq-scalar", "--interpret-evoir", source.path()]);
    assert_eq!(baseline.status.code(), Some(0));
    assert_eq!(optimized.status.code(), Some(0));
    assert_eq!(baseline.stdout, b"19\n");
    assert_eq!(optimized.stdout, baseline.stdout);
    assert!(baseline.stderr.is_empty());
    assert!(optimized.stderr.is_empty());

    let baseline_ir = run(&[TARGET, "--emit=evoir", source.path()]);
    let optimized_ir = run(&[TARGET, "-Oeq-scalar", "--emit=evoir", source.path()]);
    assert_eq!(baseline_ir.status.code(), Some(0));
    assert_eq!(optimized_ir.status.code(), Some(0));
    assert_ne!(optimized_ir.stdout, baseline_ir.stdout);
    let text = String::from_utf8(optimized_ir.stdout).unwrap();
    assert!(!text.contains("add.bits<32>"), "{text}");
    assert!(!text.contains("mul.bits<32>"), "{text}");
    assert!(!text.contains("and.bits<32>"), "{text}");
    assert!(!text.contains("or.bits<32>"), "{text}");
}
