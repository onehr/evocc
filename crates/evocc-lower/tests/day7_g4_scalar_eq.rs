use std::collections::BTreeSet;

use evocc_common::{Arena, FileId};
use evocc_eqopt::{optimize_scalar_module_with_evp1, ProofAuthority, ScalarEqBudget};
use evocc_evoir::{interpret_module, validate_module, InterpreterLimits, RuntimeValue};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_proof::{parse_evp1, render_evp1};
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
fn deterministic_scalar_p0_corpus_is_clean_before_and_after_with_reviewed_certificates() {
    let programs = sources();
    let mut executions_clean = 0;
    let mut rule_instances = 0;
    let mut canonical_certificates = 0;
    let mut optimized_programs = 0;
    let mut failures = Vec::new();

    for (ordinal, (id, source, expected)) in programs.iter().enumerate() {
        let input = lower(source, ordinal);
        if let Err(error) = validate_module(&input) {
            failures.push(format!("{id}: input validation: {error}"));
            continue;
        }
        let before = execute(&input);
        let outcome = optimize_scalar_module_with_evp1(&input, ScalarEqBudget::new(20_000, 256));
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
        if outcome.report.optimized_values == 0 || outcome.certificate.regions.is_empty() {
            failures.push(format!("{id}: no measured P0 rewrite"));
            continue;
        }
        optimized_programs += 1;
        rule_instances += outcome.certificate.regions.len();
        let certificate = render_evp1(&outcome.certificate);
        if parse_evp1(certificate.as_bytes()).as_ref() == Ok(&outcome.certificate) {
            canonical_certificates += 1;
        } else {
            failures.push(format!("{id}: producer emitted noncanonical EVP1"));
        }
        assert_eq!(
            outcome.report.proof_authority,
            ProofAuthority::Reviewed {
                registry: "p0.scalar.v3"
            },
            "{id}: the producer must name the narrowest reviewed registry containing every rule it fired"
        );
        executions_clean += 1;
    }

    assert!(
        failures.is_empty()
            && executions_clean == PROGRAMS
            && optimized_programs == PROGRAMS
            && rule_instances > 0
            && canonical_certificates == PROGRAMS,
        "Day-7 G4 gate: executions={executions_clean}/{PROGRAMS}, optimized={optimized_programs}/{PROGRAMS}, \
         canonicalCertificates={canonical_certificates}/{PROGRAMS}, ruleInstances={rule_instances}; failures:\n{}",
        failures.join("\n")
    );
    println!(
        "Day-7 G4 measured: executions={executions_clean}/{PROGRAMS}, \
         optimized={optimized_programs}/{PROGRAMS}, \
         canonicalCertificates={canonical_certificates}/{PROGRAMS}, \
         ruleInstances={rule_instances}, authority=p0.scalar.v3"
    );
}
