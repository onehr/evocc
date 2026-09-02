use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CLIENT: AtomicU64 = AtomicU64::new(0);

struct ClientProject(PathBuf);

impl ClientProject {
    fn new() -> Self {
        let id = NEXT_CLIENT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-day8-state-client-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).expect("create state client");

        let eqopt = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let crates = eqopt.parent().expect("eqopt lives under crates");
        let manifest = format!(
            r#"[package]
name = "day8-state-client"
version = "0.0.0"
edition = "2021"

[workspace]

[dependencies]
evocc-eqopt = {{ path = {:?} }}
evocc-evoir = {{ path = {:?} }}
evocc-proof = {{ path = {:?} }}
evocc-types = {{ path = {:?} }}
"#,
            eqopt,
            crates.join("evocc-evoir"),
            crates.join("evocc-proof"),
            crates.join("evocc-types")
        );
        fs::write(root.join("Cargo.toml"), manifest).expect("write state client manifest");
        fs::write(root.join("src/main.rs"), CLIENT).expect("write state client source");
        Self(root)
    }
}

impl Drop for ClientProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const CLIENT: &str = r#"
use evocc_eqopt::{
    optimize_scalar_module_certified_persistent, EqualityState, ProofAuthority, ScalarEqBudget,
};
use evocc_evoir::{Block, BlockArg, Function, FunctionParam, Inst, Module, Value};
use evocc_proof::{parse_evp1, render_evp1};
use evocc_types::EvoIRType;

fn bits(id: u32) -> Value {
    Value { id, ty: EvoIRType::Bits(32) }
}

fn input_module() -> Module {
    Module {
        globals: Vec::new(),
        functions: vec![Function {
            name: "main".to_owned(),
            params: vec![FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(32),
            }],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![BlockArg {
                    name: "x".to_owned(),
                    value: bits(0),
                }],
                insts: vec![
                    Inst::Const(bits(1), 0),
                    Inst::Add(bits(2), bits(0), bits(1)),
                ],
                terminator: Inst::Ret(vec![bits(2)]),
            }],
        }],
    }
}

fn main() {
    let input = input_module();
    let mut state = EqualityState::for_module(&input).expect("bind canonical identity");
    assert_eq!(state.revision(), 0);
    state.ingest_module(&input).expect("ingest module");
    assert_eq!(state.revision(), 1);
    let root_class = state.class_of("main", 2).expect("root class");

    let first = optimize_scalar_module_certified_persistent(
        &input,
        &mut state,
        ScalarEqBudget::new(100_000, 1_024),
    );
    assert!(!first.report.fell_back);
    assert!(matches!(
        first.report.proof_authority,
        ProofAuthority::Reviewed { registry: "p0.scalar.v1" }
    ));
    assert_ne!(first.module, input);
    assert_eq!(first.certificate.regions.len(), 1);
    let proof = render_evp1(&first.certificate);
    assert_eq!(parse_evp1(proof.as_bytes()).unwrap(), first.certificate);
    assert_eq!(state.revision(), 2);
    assert!(state.class_node_count(root_class) >= 2);
    state.verify_history().expect("one-pass history replays");

    let extracted_class = state.class_of("main", 0).expect("transferred mapping");
    assert_eq!(extracted_class, state.canonical_class(root_class));
    let after_first = state.audit();
    let second = optimize_scalar_module_certified_persistent(
        &first.module,
        &mut state,
        ScalarEqBudget::new(100_000, 1_024),
    );
    assert_eq!(second.module, first.module);
    assert!(second.certificate.regions.is_empty());
    assert_eq!(state.audit(), after_first);
    state.verify_history().expect("two-pass history replays");

    let mut unseeded = EqualityState::for_module(&input).expect("bind unseeded identity");
    let before_unseeded = unseeded.audit();
    let refused = optimize_scalar_module_certified_persistent(
        &input,
        &mut unseeded,
        ScalarEqBudget::new(100_000, 1_024),
    );
    assert!(refused.report.fell_back);
    assert!(refused.report.proof_replay_failed);
    assert_eq!(refused.module, input);
    assert!(refused.certificate.regions.is_empty());
    assert_eq!(unseeded.audit(), before_unseeded);

    let mut fallback = EqualityState::for_module(&input).expect("bind fallback identity");
    let before_failure = fallback.audit();
    let outcome = optimize_scalar_module_certified_persistent(
        &input,
        &mut fallback,
        ScalarEqBudget::new(0, 0),
    );
    // Section 11.3, revision 1.9.38: a ceiling stop keeps every region admitted
    // before it. This budget stops before the first one, so nothing is kept and
    // nothing is discarded; the transaction did not fall back.
    assert!(outcome.report.budget_exhausted);
    assert!(!outcome.report.fell_back);
    assert_eq!(outcome.module, input);
    assert!(outcome.certificate.regions.is_empty());
    assert_eq!(
        outcome.certificate.input_digest,
        outcome.certificate.output_digest
    );
    assert_eq!(fallback.audit(), before_failure);
    assert_eq!(fallback.revision(), 0);
}
"#;

#[test]
fn day8_persistent_equality_accumulation_and_transaction_contract() {
    let client = ClientProject::new();
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--offline"])
        .current_dir(&client.0)
        .env("CARGO_TARGET_DIR", client.0.join("target"))
        .output()
        .expect("run persistent-state client");
    assert_eq!(
        output.status.code(),
        Some(0),
        "client stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
