use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const PROGRAMS: usize = 128;
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

const CORPUS_PRODUCER: &str = r#"
use std::{collections::BTreeSet, env, fs, path::Path};

use evocc_common::{Arena, FileId};
use evocc_eqopt::{ScalarEqBudget, optimize_scalar_module_with_evp1};
use evocc_evoir::{InterpreterLimits, interpret_module, render_module, validate_module};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_proof::{EvpEvent, render_evp1};
use evocc_types::TargetDataLayout;

const PROGRAMS: usize = 128;

fn source(ordinal: usize) -> (String, u64) {
    let initial = ordinal % 97 + 3;
    let source = format!(
        "int main(void) {{ int x_{ordinal:03} = {initial}; \
         int add_zero = x_{ordinal:03} + 0; int mul_one = add_zero * 1; \
         int and_zero = mul_one & 0; int or_zero = mul_one | 0; \
         int double_neg = -(-x_{ordinal:03}); \
         int identity = x_{ordinal:03} == x_{ordinal:03}; \
         return (or_zero + and_zero + double_neg + identity + (2 + 3)) % 251; }}\n"
    );
    (source, ((initial * 2 + 6) % 251) as u64)
}

fn lower(source: &str, ordinal: usize) -> evocc_evoir::Module {
    let arena = Arena::default();
    let file = FileId::new(19_000 + ordinal as u32);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error);
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "case {ordinal}: {:?}", parser.diagnostic());
    lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap()
}

fn write_case(root: &Path, ordinal: usize, source: &str, expected: u64) -> (usize, usize) {
    let before = lower(source, ordinal);
    validate_module(&before).unwrap();
    let before_result = interpret_module(
        &before,
        "main",
        &[],
        InterpreterLimits::new(100_000),
    )
    .unwrap();
    assert_eq!(before_result.return_values[0].raw_bits, expected);

    let outcome = optimize_scalar_module_with_evp1(
        &before,
        ScalarEqBudget::new(20_000, 256),
    );
    assert!(!outcome.report.fell_back);
    assert!(!outcome.certificate.regions.is_empty());
    validate_module(&outcome.module).unwrap();
    let after_result = interpret_module(
        &outcome.module,
        "main",
        &[],
        InterpreterLimits::new(100_000),
    )
    .unwrap();
    assert_eq!(before_result.return_values, after_result.return_values);

    let stem = format!("{ordinal:03}");
    fs::write(root.join(format!("{stem}.before.evo")), render_module(&before)).unwrap();
    fs::write(root.join(format!("{stem}.after.evo")), render_module(&outcome.module)).unwrap();
    fs::write(root.join(format!("{stem}.proof.evp")), render_evp1(&outcome.certificate)).unwrap();
    let rule_instances = outcome
        .certificate
        .regions
        .iter()
        .flat_map(|region| &region.events)
        .filter(|event| matches!(event, EvpEvent::Rewrite(_)))
        .count();
    (outcome.certificate.regions.len(), rule_instances)
}

fn main() {
    let root = env::args_os().nth(1).expect("output directory");
    let root = Path::new(&root);
    fs::create_dir_all(root).unwrap();
    let mut unique = BTreeSet::new();
    let mut regions = 0;
    let mut rule_instances = 0;
    let mut optimized = 0;
    for ordinal in 0..PROGRAMS {
        let (source, expected) = source(ordinal);
        assert!(unique.insert(source.clone()));
        let (case_regions, case_rule_instances) = write_case(root, ordinal, &source, expected);
        if case_regions != 0 {
            optimized += 1;
        }
        regions += case_regions;
        rule_instances += case_rule_instances;
    }
    fs::write(
        root.join("measurement.txt"),
        format!(
            "programs={PROGRAMS}\noptimized={optimized}\nregions={regions}\nruleInstances={rule_instances}\n"
        ),
    )
    .unwrap();
}
"#;

struct Corpus {
    root: PathBuf,
}

impl Corpus {
    fn new() -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-day8-corpus-replay-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create corpus root");
        Self { root }
    }

    fn case(&self, ordinal: usize, suffix: &str) -> PathBuf {
        self.root.join(format!("{ordinal:03}.{suffix}"))
    }
}

impl Drop for Corpus {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
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

fn build_corpus(corpus: &Corpus) {
    let client = corpus.root.join("producer-client");
    fs::create_dir_all(client.join("src")).expect("create corpus producer client");
    let proofcheck = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crates = proofcheck.parent().expect("proofcheck lives under crates");
    fs::write(
        client.join("Cargo.toml"),
        format!(
            r#"[package]
name = "day8-corpus-producer-client"
version = "0.0.0"
edition = "2021"

[workspace]

[dependencies]
evocc-common = {{ path = {:?} }}
evocc-eqopt = {{ path = {:?} }}
evocc-evoir = {{ path = {:?} }}
evocc-lex = {{ path = {:?} }}
evocc-lower = {{ path = {:?} }}
evocc-parser = {{ path = {:?} }}
evocc-proof = {{ path = {:?} }}
evocc-types = {{ path = {:?} }}
"#,
            crates.join("evocc-common"),
            crates.join("evocc-eqopt"),
            crates.join("evocc-evoir"),
            crates.join("evocc-lex"),
            crates.join("evocc-lower"),
            crates.join("evocc-parser"),
            crates.join("evocc-proof"),
            crates.join("evocc-types"),
        ),
    )
    .expect("write corpus producer manifest");
    fs::write(client.join("src/main.rs"), CORPUS_PRODUCER).expect("write corpus producer source");
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--offline", "--"])
        .arg(&corpus.root)
        .current_dir(&client)
        .env("CARGO_TARGET_DIR", client.join("target"))
        .output()
        .expect("run corpus producer");
    assert_status(&output, 0, "corpus producer");
}

fn run_checker(before: &Path, after: &Path, proof: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
        .args([before, after, proof])
        .output()
        .expect("run standalone proof checker")
}

#[test]
fn deterministic_128_program_corpus_replays_in_the_standalone_checker() {
    let corpus = Corpus::new();
    build_corpus(&corpus);
    let measurement =
        fs::read_to_string(corpus.root.join("measurement.txt")).expect("read producer measurement");
    let measured = measurement
        .lines()
        .map(|line| {
            let (name, value) = line.split_once('=').expect("measurement key=value");
            (name, value.parse::<usize>().expect("measurement integer"))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(measured.get("programs"), Some(&PROGRAMS));
    assert_eq!(
        measured.get("optimized"),
        Some(&PROGRAMS),
        "the reviewed ruling must not turn optimization into a false-green no-op"
    );
    assert!(measured["regions"] > 0);
    assert!(measured["ruleInstances"] >= measured["regions"]);

    let mut accepted = 0;
    for ordinal in 0..PROGRAMS {
        let output = run_checker(
            &corpus.case(ordinal, "before.evo"),
            &corpus.case(ordinal, "after.evo"),
            &corpus.case(ordinal, "proof.evp"),
        );
        assert_status(&output, 0, &format!("corpus case {ordinal:03}"));
        accepted += 1;
    }

    assert_eq!(accepted, PROGRAMS, "the gate forbids skips");
    println!(
        "Day-8 reviewed proof corpus measured: accepted={accepted}/{PROGRAMS}, optimized={}/{PROGRAMS}, regions={}, ruleInstances={}, skipped=0",
        measured["optimized"], measured["regions"], measured["ruleInstances"]
    );
}
