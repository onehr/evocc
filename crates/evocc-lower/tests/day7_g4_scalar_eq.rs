use std::collections::BTreeSet;

use evocc_common::{Arena, FileId};
use evocc_eqopt::{optimize_scalar_module, ProofAuthority, ScalarEqBudget};
use evocc_evoir::{interpret_module, validate_module, InterpreterLimits, RuntimeValue};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_proof::ProofContext;
use evocc_types::TargetDataLayout;

const PROGRAMS: usize = 128;
const STEP_BUDGET: u64 = 100_000;

fn sources() -> Vec<(String, String, u64)> {
    let programs = (0..PROGRAMS)
        .map(|ordinal| {
            let initial = ordinal % 97 + 3;
            let source = format!(
                "int main(void) {{ int x_{ordinal:03} = {initial}; \
                 int add_zero = x_{ordinal:03} + 0; int mul_one = add_zero * 1; \
                 int and_zero = mul_one & 0; int or_zero = mul_one | 0; \
                 int double_neg = -(-x_{ordinal:03}); \
                 int identity = x_{ordinal:03} == x_{ordinal:03}; \
                 return (or_zero + and_zero + double_neg + identity + (2 + 3)) % 251; }}\n"
            );
            let expected = (initial * 2 + 6) % 251;
            (
                format!("day7.g4.scalar-p0.{ordinal:03}"),
                source,
                expected as u64,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(programs.len(), PROGRAMS);
    assert_eq!(
        programs
            .iter()
            .map(|(_, source, _)| source.as_bytes())
            .collect::<BTreeSet<_>>()
            .len(),
        PROGRAMS,
        "G4 sources must be byte-unique"
    );
    programs
}

fn lower(source: &str, ordinal: usize) -> evocc_evoir::Module {
    let arena = Arena::default();
    let file = FileId::new(9_000 + ordinal as u32);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(
            token.kind,
            TokenKind::Error,
            "lexer rejected case {ordinal}"
        );
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(
        !parser.had_error(),
        "parser rejected case {ordinal}: {:?}",
        parser.diagnostic()
    );
    lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("lowering case {ordinal}: {error}"))
}

fn execute(module: &evocc_evoir::Module) -> Vec<RuntimeValue> {
    interpret_module(module, "main", &[], InterpreterLimits::new(STEP_BUDGET))
        .expect("defined G4 program executes")
        .return_values
}

#[test]
fn deterministic_scalar_p0_corpus_is_clean_before_and_after_with_full_proof_replay() {
    let programs = sources();
    let mut executions_clean = 0;
    let mut receipts = 0;
    let mut receipts_replayed = 0;
    let mut optimized_programs = 0;
    let mut failures = Vec::new();

    for (ordinal, (id, source, expected)) in programs.iter().enumerate() {
        let input = lower(source, ordinal);
        if let Err(error) = validate_module(&input) {
            failures.push(format!("{id}: input validation: {error}"));
            continue;
        }
        let before = execute(&input);
        let outcome = optimize_scalar_module(&input, ScalarEqBudget::new(20_000, 256));
        if outcome.report.fell_back {
            failures.push(format!("{id}: unexpected optimizer fallback"));
            continue;
        }
        if let Err(error) = validate_module(&outcome.module) {
            failures.push(format!("{id}: output validation: {error}"));
            continue;
        }
        let after = execute(&outcome.module);
        if before != after || after.first().map(|value| value.raw_bits) != Some(*expected) {
            failures.push(format!(
                "{id}: before={before:?}, after={after:?}, expected={expected}"
            ));
            continue;
        }
        if outcome.report.optimized_values == 0 || outcome.report.receipts.is_empty() {
            failures.push(format!("{id}: no measured P0 rewrite"));
            continue;
        }
        optimized_programs += 1;
        let mut case_replays = 0;
        for receipt in &outcome.report.receipts {
            receipts += 1;
            // Replaying against the certificate's own rules checks that the
            // recorded merges follow from the recorded rules. It does not check
            // that those rules are sound, so this loop measures consistency and
            // the assertion below states which authority produced that number.
            let context = match ProofContext::new(receipt.certificate.rules.clone()) {
                Ok(context) => context,
                Err(error) => {
                    failures.push(format!("{id}: proof context: {error}"));
                    continue;
                }
            };
            match context.replay(&receipt.certificate) {
                Ok(report) if report.rewrite_unions_replayed == 1 => {
                    receipts_replayed += 1;
                    case_replays += 1;
                }
                Ok(report) => failures.push(format!("{id}: replay count: {report:?}")),
                Err(error) => failures.push(format!("{id}: replay: {error}")),
            }
        }
        assert_eq!(
            outcome.report.proof_authority,
            ProofAuthority::ProducerSupplied,
            "{id}: the measured replay count must be reported under the authority that \
             actually produced it"
        );
        if case_replays == outcome.report.receipts.len() {
            executions_clean += 1;
        }
    }

    assert!(
        failures.is_empty()
            && executions_clean == PROGRAMS
            && optimized_programs == PROGRAMS
            && receipts > 0
            && receipts_replayed == receipts,
        "Day-7 G4 gate: executions={executions_clean}/{PROGRAMS}, optimized={optimized_programs}/{PROGRAMS}, \
         proof={receipts_replayed}/{receipts}; failures:\n{}",
        failures.join("\n")
    );
    println!(
        "Day-7 G4 measured: executions={executions_clean}/{PROGRAMS}, \
         optimized={optimized_programs}/{PROGRAMS}, \
         consistencyReplays={receipts_replayed}/{receipts}, authority=ProducerSupplied"
    );
}
