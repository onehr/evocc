use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{render_module, validate_module};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_proof::render_evp1;
use evocc_types::TargetDataLayout;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct ProofFiles {
    directory: PathBuf,
    before: PathBuf,
    after: PathBuf,
    proof: PathBuf,
}

impl ProofFiles {
    fn new() -> Self {
        let ordinal = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "evocc-pointer-memory-proof-{}-{ordinal}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create independent proof fixture directory");
        Self {
            before: directory.join("before.evo"),
            after: directory.join("after.evo"),
            proof: directory.join("proof.evp"),
            directory,
        }
    }
}

impl Drop for ProofFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(9_203);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "fixture must parse before proof production: {:?}",
        parser.diagnostic()
    );
    unit
}

#[test]
fn pointer_memory_lowering_replays_at_the_independent_proof_boundary() {
    let source = concat!(
        "int identity(int value) { return value + 0; } ",
        "struct Node { int value; struct Node *next; }; ",
        "int main(void) { struct Node a; struct Node b; int values[3] = {4, 5, 6}; ",
        "a.next = &b; b.value = 9; struct Node *node = &a; int *p = values; p++; ",
        "int selected = *p + node->next->value; return identity(selected); }",
    );
    replays(source);
}

#[test]
fn dynamic_allocation_size_survives_independent_reassembly() {
    replays("int identity(unsigned long n) { int *p = __builtin_alloca(n + 0); *p = 17; return *p; } int main(void) { return identity(4); }");
}

fn replays(source: &str) {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let before = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("pointer memory fixture lowers");
    validate_module(&before).expect("before module validates");

    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    validate_module(&outcome.module).expect("optimized module validates");
    assert!(
        outcome.report.optimized_values > 0,
        "the add-zero witness must prevent a vacuous proof"
    );

    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).expect("write before module");
    fs::write(&files.after, render_module(&outcome.module)).expect("write after module");
    fs::write(&files.proof, render_evp1(&outcome.certificate)).expect("write certificate");

    let output = Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
        .args([&files.before, &files.after, &files.proof])
        .output()
        .expect("run independent proof checker");
    assert_eq!(
        output.status.code(),
        Some(0),
        "proof checker rejected pointer-memory lowering: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
