use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_proof::{parse_evp1, render_evp1, sha256_hex, EvpCertificate, EvpEvent, EvpNodeSource};

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const REGISTRY_SHA256: &str = "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9";
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

const PRODUCER_CLIENT: &str = r#"
use std::{env, fs};
use evocc_eqopt::{ScalarEqBudget, optimize_scalar_module_with_evp1};
use evocc_evoir::{Block, BlockArg, Function, FunctionParam, Inst, Module, Value, render_module};
use evocc_proof::render_evp1;
use evocc_types::EvoIRType;

fn bits(id: u32) -> Value { Value { id, ty: EvoIRType::Bits(32) } }

fn main() {
    let paths = env::args_os().skip(1).collect::<Vec<_>>();
    assert_eq!(paths.len(), 3);
    let before = Module {
        globals: Vec::new(),
        functions: vec![Function {
            name: "main".to_owned(),
            params: vec![FunctionParam { name: "x".to_owned(), ty: EvoIRType::Bits(32) }],
            ret_tys: vec![EvoIRType::Bits(32), EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![BlockArg { name: "x".to_owned(), value: bits(0) }],
                insts: vec![
                    Inst::Const(bits(1), 0),
                    Inst::Add(bits(2), bits(0), bits(1)),
                    Inst::Const(bits(3), 2),
                    Inst::Const(bits(4), 3),
                    Inst::Add(bits(5), bits(3), bits(4)),
                ],
                terminator: Inst::Ret(vec![bits(2), bits(5)]),
            }],
        }],
    };
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    assert_eq!(outcome.certificate.regions.len(), 2);
    fs::write(&paths[0], render_module(&before)).unwrap();
    fs::write(&paths[1], render_module(&outcome.module)).unwrap();
    fs::write(&paths[2], render_evp1(&outcome.certificate)).unwrap();
}
"#;

struct Inputs {
    dir: PathBuf,
    before: PathBuf,
    after: PathBuf,
    proof: PathBuf,
}

impl Inputs {
    fn new() -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("evocc-day8-proof-{}-{id}", std::process::id()));
        fs::create_dir(&dir).expect("create proof fixture directory");
        Self {
            before: dir.join("before.evo"),
            after: dir.join("after.evo"),
            proof: dir.join("proof.evp"),
            dir,
        }
    }

    fn write(&self, before: &[u8], after: &[u8], proof: &str) {
        fs::write(&self.before, before).expect("write before.evo");
        fs::write(&self.after, after).expect("write after.evo");
        fs::write(&self.proof, proof).expect("write proof.evp");
    }

    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([
                self.before.as_path(),
                self.after.as_path(),
                self.proof.as_path(),
            ])
            .output()
            .expect("run evo-proofcheck")
    }
}

impl Drop for Inputs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn run_real_producer(inputs: &Inputs) {
    let client = inputs.dir.join("producer-client");
    fs::create_dir_all(client.join("src")).expect("create producer client");
    let proofcheck = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crates = proofcheck.parent().expect("proofcheck lives under crates");
    fs::write(
        client.join("Cargo.toml"),
        format!(
            r#"[package]
name = "day8-evp1-producer-client"
version = "0.0.0"
edition = "2021"

[workspace]

[dependencies]
evocc-eqopt = {{ path = {:?} }}
evocc-evoir = {{ path = {:?} }}
evocc-proof = {{ path = {:?} }}
evocc-types = {{ path = {:?} }}
"#,
            crates.join("evocc-eqopt"),
            crates.join("evocc-evoir"),
            crates.join("evocc-proof"),
            crates.join("evocc-types"),
        ),
    )
    .expect("write producer manifest");
    fs::write(client.join("src/main.rs"), PRODUCER_CLIENT).expect("write producer client");
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--offline", "--"])
        .args([&inputs.before, &inputs.after, &inputs.proof])
        .current_dir(&client)
        .env("CARGO_TARGET_DIR", client.join("target"))
        .output()
        .expect("run real EVP1 producer");
    assert_status(&output, 0, "real producer client");
}

fn certificate(input: &str, output: &str) -> String {
    format!("evp1\nregistry p0.scalar.v1 {REGISTRY_SHA256}\ninput {input}\noutput {output}\n")
}

fn add_zero_certificate(input: &str, output: &str) -> String {
    format!(
        concat!(
            "evp1\n",
            "registry p0.scalar.v1 {REGISTRY_SHA256}\n",
            "input {input}\n",
            "region main/entry/1 root %v2 witnesses 2 seeds 1\n",
            "witness 0 main/entry/0\n",
            "witness 1 main/entry/1\n",
            "seed 0 %v0 bits<32> input c0\n",
            "node 0 n0 c1 source=before op=const sort=bits<32> effect=pure attrs=value=0 children=-\n",
            "node 1 n1 c2 source=before op=add sort=bits<32> effect=pure attrs=- children=c0,c1\n",
            "rewrite 2 p0_001_add_zero_rhs c2 c0 c0 node:n1 class:c0 bind ?x:c0\n",
            "extract 0 c0 seed:0\n",
            "root c0\n",
            "endregion\n",
            "output {output}\n",
        ),
        REGISTRY_SHA256 = REGISTRY_SHA256,
        input = input,
        output = output,
    )
}

fn assert_status(output: &Output, expected: i32, label: &str) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{label}: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn day8_proof_format_eligibility_and_binding_contract() {
    let probe = Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
        .arg("--probe")
        .output()
        .expect("probe evo-proofcheck");
    assert_status(&probe, 0, "probe");
    assert!(probe.stdout.is_empty());
    assert!(probe.stderr.is_empty());

    let inputs = Inputs::new();
    inputs.write(b"", b"", &certificate(EMPTY_SHA256, EMPTY_SHA256));
    assert_status(&inputs.run(), 0, "valid zero-region certificate");

    inputs.write(
        b"",
        b"",
        &certificate(
            "0000000000000000000000000000000000000000000000000000000000000000",
            EMPTY_SHA256,
        ),
    );
    assert_status(&inputs.run(), 3, "certificate bound to different input");

    let nonempty_after = b"func @main() -> () cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n{\n^entry():\n    ret\n}\n";
    inputs.write(
        b"",
        nonempty_after,
        &certificate(EMPTY_SHA256, EMPTY_SHA256),
    );
    assert_status(&inputs.run(), 3, "zero-region proof cannot change output");

    inputs.write(b"\n", b"", &certificate(EMPTY_SHA256, EMPTY_SHA256));
    assert_status(&inputs.run(), 2, "noncanonical module is malformed");

    inputs.write(b"", b"", "evp0\n");
    assert_status(&inputs.run(), 2, "unknown proof version is malformed");
}

#[test]
fn nonempty_reviewed_rewrite_is_replayed_and_reassembled() {
    let before = concat!(
        "func @main(%arg0: bits<32>) -> (bits<32>) cc = target_default\n",
        "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n",
        "{\n",
        "^entry(%v0: bits<32>):\n",
        "    %v1 = const.bits<32> 0\n",
        "    %v2 = add.bits<32> %v0, %v1\n",
        "    ret %v2\n",
        "}\n",
    );
    let after = concat!(
        "func @main(%arg0: bits<32>) -> (bits<32>) cc = target_default\n",
        "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n",
        "{\n",
        "^entry(%v0: bits<32>):\n",
        "    %v1 = const.bits<32> 0\n",
        "    ret %v0\n",
        "}\n",
    );
    let input_digest = evocc_proof::sha256_hex(before.as_bytes());
    let output_digest = evocc_proof::sha256_hex(after.as_bytes());
    let inputs = Inputs::new();
    inputs.write(
        before.as_bytes(),
        after.as_bytes(),
        &add_zero_certificate(&input_digest, &output_digest),
    );
    assert_status(&inputs.run(), 0, "reviewed add-zero proof");

    let hidden_witness = add_zero_certificate(&input_digest, &output_digest)
        .replace("witness 0 main/entry/0", "witness 0 main/entry/1");
    inputs.write(before.as_bytes(), after.as_bytes(), &hidden_witness);
    assert_status(
        &inputs.run(),
        3,
        "certificate cannot redefine the independently derived witness closure",
    );

    let forged_seed =
        add_zero_certificate(&input_digest, &output_digest).replace("seed 0 %v0", "seed 0 %v1");
    inputs.write(before.as_bytes(), after.as_bytes(), &forged_seed);
    assert_status(
        &inputs.run(),
        3,
        "certificate cannot redefine the independently derived seed manifest",
    );

    let mutated = after.replace("ret %v0", "ret %v1");
    inputs.write(
        before.as_bytes(),
        mutated.as_bytes(),
        &add_zero_certificate(&input_digest, &output_digest),
    );
    assert_status(&inputs.run(), 3, "after-module mutation");
}

#[test]
fn reviewed_replay_is_derived_from_module_identity_not_one_hardcoded_fixture() {
    let before = concat!(
        "func @renamed(%arg0: bits<32>) -> (bits<32>) cc = target_default\n",
        "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n",
        "{\n",
        "^entry(%v7: bits<32>):\n",
        "    %v8 = const.bits<32> 0\n",
        "    %v9 = add.bits<32> %v7, %v8\n",
        "    ret %v9\n",
        "}\n",
    );
    let after = concat!(
        "func @renamed(%arg0: bits<32>) -> (bits<32>) cc = target_default\n",
        "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n",
        "{\n",
        "^entry(%v0: bits<32>):\n",
        "    %v1 = const.bits<32> 0\n",
        "    ret %v0\n",
        "}\n",
    );
    let input = evocc_proof::sha256_hex(before.as_bytes());
    let output = evocc_proof::sha256_hex(after.as_bytes());
    let proof = format!(
        concat!(
            "evp1\n",
            "registry p0.scalar.v1 {REGISTRY_SHA256}\n",
            "input {input}\n",
            "region renamed/entry/1 root %v9 witnesses 2 seeds 1\n",
            "witness 0 renamed/entry/0\n",
            "witness 1 renamed/entry/1\n",
            "seed 0 %v7 bits<32> input c0\n",
            "node 0 n0 c1 source=before op=const sort=bits<32> effect=pure attrs=value=0 children=-\n",
            "node 1 n1 c2 source=before op=add sort=bits<32> effect=pure attrs=- children=c0,c1\n",
            "rewrite 2 p0_001_add_zero_rhs c2 c0 c0 node:n1 class:c0 bind ?x:c0\n",
            "extract 0 c0 seed:0\n",
            "root c0\n",
            "endregion\n",
            "output {output}\n",
        ),
        REGISTRY_SHA256 = REGISTRY_SHA256,
        input = input,
        output = output,
    );
    let inputs = Inputs::new();
    inputs.write(before.as_bytes(), after.as_bytes(), &proof);
    assert_status(
        &inputs.run(),
        0,
        "reviewed replay with independent symbol and value identities",
    );
}

#[test]
fn real_producer_multi_region_evp1_is_accepted_by_the_standalone_checker() {
    let inputs = Inputs::new();
    run_real_producer(&inputs);
    assert_status(
        &inputs.run(),
        0,
        "real producer and independent checker interoperability",
    );

    let after = fs::read_to_string(&inputs.after).expect("read producer after-module");
    fs::write(&inputs.after, after.replace("ret %v0, %v4", "ret %v1, %v4"))
        .expect("mutate after-module outside certificate result");
    assert_status(&inputs.run(), 3, "mutated real producer after-module");
}

fn write_certificate(inputs: &Inputs, before: &[u8], after: &[u8], proof: &EvpCertificate) {
    inputs.write(before, after, &render_evp1(proof));
}

#[test]
fn optimizer_output_mutation_campaign_exercises_the_independent_boundary() {
    let inputs = Inputs::new();
    run_real_producer(&inputs);
    assert_status(&inputs.run(), 0, "mutation campaign baseline");
    let before = fs::read(&inputs.before).expect("read producer before-module");
    let after = fs::read(&inputs.after).expect("read producer after-module");
    let proof_bytes = fs::read(&inputs.proof).expect("read producer proof");
    let baseline = parse_evp1(&proof_bytes).expect("parse real producer proof");
    let mut killed = 0;

    let mutated_after = String::from_utf8(after.clone())
        .unwrap()
        .replace("ret %v0, %v4", "ret %v1, %v4")
        .into_bytes();
    assert_ne!(mutated_after, after);
    let mut resigned = baseline.clone();
    resigned.output_digest = sha256_hex(&mutated_after);
    write_certificate(&inputs, &before, &mutated_after, &resigned);
    assert_status(
        &inputs.run(),
        3,
        "semantic output mutation with a recomputed producer digest",
    );
    killed += 1;

    let mut wrong_rule = baseline.clone();
    let rewrite = wrong_rule
        .regions
        .iter_mut()
        .flat_map(|region| &mut region.events)
        .find_map(|event| match event {
            EvpEvent::Rewrite(rewrite) => Some(rewrite),
            _ => None,
        })
        .expect("producer emitted a rewrite");
    rewrite.rule = "p0_003_mul_one_rhs".to_owned();
    write_certificate(&inputs, &before, &after, &wrong_rule);
    assert_status(&inputs.run(), 3, "producer-selected wrong reviewed rule");
    killed += 1;

    let mut wrong_constant = baseline.clone();
    let attribute = wrong_constant
        .regions
        .iter_mut()
        .flat_map(|region| &mut region.events)
        .find_map(|event| match event {
            EvpEvent::Node(node) if node.source == EvpNodeSource::Derived => {
                node.attributes.first_mut()
            }
            _ => None,
        })
        .expect("producer emitted a derived constant");
    attribute.1 = attribute.1.wrapping_add(1);
    write_certificate(&inputs, &before, &after, &wrong_constant);
    assert_status(&inputs.run(), 3, "producer-computed wrong fold payload");
    killed += 1;

    let mut wrong_binding = baseline.clone();
    let binding = wrong_binding
        .regions
        .iter_mut()
        .flat_map(|region| &mut region.events)
        .find_map(|event| match event {
            EvpEvent::Rewrite(rewrite) => rewrite.bindings.first_mut(),
            _ => None,
        })
        .expect("producer emitted a rewrite binding");
    binding.1 = binding.1.wrapping_add(1);
    write_certificate(&inputs, &before, &after, &wrong_binding);
    assert_status(&inputs.run(), 3, "producer-computed wrong substitution");
    killed += 1;

    let mut hidden_witness = baseline.clone();
    hidden_witness.regions[0].witnesses.remove(0);
    write_certificate(&inputs, &before, &after, &hidden_witness);
    assert_status(&inputs.run(), 3, "producer hid one eligibility witness");
    killed += 1;

    let digest = sha256_hex(&before);
    let disabled = EvpCertificate {
        registry_name: baseline.registry_name,
        registry_digest: baseline.registry_digest,
        input_digest: digest.clone(),
        regions: Vec::new(),
        output_digest: digest,
    };
    write_certificate(&inputs, &before, &before, &disabled);
    assert_status(
        &inputs.run(),
        0,
        "a producer mutant that only disables rewrites remains correct",
    );

    assert_eq!(killed, 5);
    println!(
        "Day-8 proof-boundary mutation measured: semanticCorruptionsRejected={killed}/5, disabledRewriteAccepted=1/1"
    );
}
