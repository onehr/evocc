//! Produce one real `before.evo` / `after.evo` / `proof.evp` triple.
//!
//! Until now a triple existed only inside a test's temporary directory, built
//! by a generated client project
//! (`crates/evocc-proofcheck/tests/day8_corpus_replay.rs`). Nothing in the
//! repository could be handed to `evo-proofcheck` directly, and nothing
//! recorded how much a given input actually exercises the registry. This driver
//! writes the three files plus the event counts a reader needs in order to say
//! what the triple demonstrates, so a sealed fixture states its own contents
//! instead of leaving them to be asserted elsewhere.
//!
//! ```text
//! cargo run -p evocc-eqopt --example certificate_triple -- <input.c|input.evo> <output-directory>
//! ```
//!
//! The optimization is the shipped default policy of Section 7.5, unchanged:
//! the point of a sealed triple is that the checker accepts what the producer
//! ships, not what a driver configured for the occasion produced.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use evocc_common::{Arena, FileId};
use evocc_eqopt::{optimize_scalar_module_with_policy, ExtractionPolicy};
use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, validate_module, EvoIRType,
    InterpreterLimits, Module, RuntimeValue,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_proof::{EvpCertificate, EvpEvent};
use evocc_types::TargetDataLayout;

/// The interpreter budget the equivalence spot check runs under. It only has to
/// cover a fixture small enough to read, and a budget that ran out is reported
/// rather than silently skipped.
const STEP_BUDGET: u64 = 200_000;

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let [input, output] = arguments.as_slice() else {
        eprintln!("usage: certificate_triple <input.c|input.evo> <output-directory>");
        return ExitCode::from(2);
    };
    match run(Path::new(input), Path::new(output)) {
        Ok(summary) => {
            print!("{summary}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("certificate_triple: {message}");
            ExitCode::from(1)
        }
    }
}

fn run(input: &Path, output: &Path) -> Result<String, String> {
    let before = read_module(input)?;
    validate_module(&before)
        .map_err(|message| format!("the input is not valid EvoIR: {message}"))?;

    let outcome = optimize_scalar_module_with_policy(&before, &ExtractionPolicy::default());
    validate_module(&outcome.module)
        .map_err(|message| format!("the optimized module is not valid EvoIR: {message}"))?;

    let executed = spot_check(&before, &outcome.module)?;

    std::fs::create_dir_all(output)
        .map_err(|error| format!("create {}: {error}", output.display()))?;
    write(output, "before.evo", &render_module(&before))?;
    write(output, "after.evo", &render_module(&outcome.module))?;
    write(
        output,
        "proof.evp",
        &evocc_proof::render_evp1(&outcome.certificate),
    )?;

    let counts = Counts::of(&outcome.certificate, input, executed).render();
    write(output, "counts.json", &counts)?;
    Ok(counts)
}

/// A `.evo` file is the canonical text of Section 9; anything else is C, lowered
/// through the front end the corpus replay already uses.
fn read_module(input: &Path) -> Result<Module, String> {
    let bytes =
        std::fs::read(input).map_err(|error| format!("read {}: {error}", input.display()))?;
    if input
        .extension()
        .is_some_and(|extension| extension == "evo")
    {
        return parse_section9_module(&bytes)
            .map_err(|error| format!("parse {}: {error:?}", input.display()));
    }
    let source = String::from_utf8(bytes)
        .map_err(|error| format!("{} is not UTF-8: {error}", input.display()))?;
    lower(&source)
}

fn lower(source: &str) -> Result<Module, String> {
    let arena = Arena::default();
    let file = FileId::new(1);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!("the lexer refused a token at {:?}", token.span));
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    if parser.had_error() {
        return Err(format!(
            "the parser refused the source: {:?}",
            parser.diagnostic()
        ));
    }
    lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .map_err(|error| format!("lowering refused the unit: {error:?}"))
}

/// Run `main` on both modules when the entry takes no value argument.
///
/// The memory and world tokens of Section 1.4 are the only entry arguments a
/// deterministic run may invent, so an entry that also takes a `bits<N>` or a
/// pointer is left unexecuted rather than executed on a value this driver made
/// up. This is a spot check, not the verification: the certificate is what the
/// checker replays. A fixture whose entry cannot be executed here is still a
/// fixture, and the counts record that the check did not run rather than
/// implying that it passed.
fn spot_check(before: &Module, after: &Module) -> Result<bool, String> {
    let limits = InterpreterLimits::new(STEP_BUDGET);
    let Some(arguments) = entry_arguments(before) else {
        return Ok(false);
    };
    let Ok(expected) = interpret_module(before, "main", &arguments, limits) else {
        return Ok(false);
    };
    let observed = interpret_module(after, "main", &arguments, limits)
        .map_err(|error| format!("the optimized module stopped executing: {error:?}"))?;
    if expected.return_values != observed.return_values {
        return Err(format!(
            "optimization changed the result: {:?} became {:?}",
            expected.return_values, observed.return_values
        ));
    }
    Ok(true)
}

/// The entry arguments of `@main`, or `None` when one of them carries a value.
fn entry_arguments(module: &Module) -> Option<Vec<RuntimeValue>> {
    let entry = module
        .functions
        .iter()
        .find(|function| function.name == "main")?;
    entry
        .blocks
        .first()?
        .args
        .iter()
        .map(|argument| match &argument.value.ty {
            ty @ (EvoIRType::Mem(_) | EvoIRType::World) => Some(RuntimeValue::new(ty.clone(), 0)),
            _ => None,
        })
        .collect()
}

fn write(directory: &Path, name: &str, contents: &str) -> Result<(), String> {
    let path = directory.join(name);
    std::fs::write(&path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

/// Everything a reader of a sealed triple needs in order to say what it covers.
struct Counts {
    input: String,
    registry_name: String,
    registry_digest: String,
    regions: usize,
    witnesses: usize,
    seeds: usize,
    nodes: usize,
    rewrites: usize,
    congruences: usize,
    epochs: usize,
    extractions: usize,
    premises: usize,
    rules: BTreeMap<String, usize>,
    executed: bool,
}

impl Counts {
    fn of(certificate: &EvpCertificate, input: &Path, executed: bool) -> Self {
        let mut counts = Self {
            input: input.file_name().map_or_else(
                || input.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            registry_name: certificate.registry_name.clone(),
            registry_digest: certificate.registry_digest.clone(),
            regions: certificate.regions.len(),
            witnesses: 0,
            seeds: 0,
            nodes: 0,
            rewrites: 0,
            congruences: 0,
            epochs: 0,
            extractions: 0,
            premises: 0,
            rules: BTreeMap::new(),
            executed,
        };
        for region in &certificate.regions {
            counts.witnesses += region.witnesses.len();
            counts.seeds += region.seeds.len();
            counts.extractions += region.extractions.len();
            for event in &region.events {
                match event {
                    EvpEvent::Node(_) => counts.nodes += 1,
                    EvpEvent::Rewrite(rewrite) => {
                        counts.rewrites += 1;
                        counts.premises += rewrite.premises.len();
                        *counts.rules.entry(rewrite.rule.clone()).or_default() += 1;
                    }
                    EvpEvent::Congruence(_) => counts.congruences += 1,
                    EvpEvent::Epoch(_) => counts.epochs += 1,
                }
            }
        }
        counts
    }

    /// Deterministic JSON: the rule map is a `BTreeMap`, so two runs over one
    /// input render byte-identical counts and a fixture can be sealed.
    fn render(&self) -> String {
        let mut text = String::from("{\n");
        let _ = writeln!(text, "  \"input\": \"{}\",", self.input);
        let _ = writeln!(text, "  \"registryName\": \"{}\",", self.registry_name);
        let _ = writeln!(text, "  \"registryDigest\": \"{}\",", self.registry_digest);
        let _ = writeln!(text, "  \"regions\": {},", self.regions);
        let _ = writeln!(text, "  \"witnesses\": {},", self.witnesses);
        let _ = writeln!(text, "  \"seeds\": {},", self.seeds);
        text.push_str("  \"events\": {\n");
        let _ = writeln!(text, "    \"node\": {},", self.nodes);
        let _ = writeln!(text, "    \"rewrite\": {},", self.rewrites);
        let _ = writeln!(text, "    \"congruence\": {},", self.congruences);
        let _ = writeln!(text, "    \"epoch\": {},", self.epochs);
        let _ = writeln!(text, "    \"extract\": {}", self.extractions);
        text.push_str("  },\n");
        let _ = writeln!(text, "  \"premises\": {},", self.premises);
        let _ = writeln!(text, "  \"entryExecuted\": {},", self.executed);
        text.push_str("  \"rules\": {\n");
        let last = self.rules.len().saturating_sub(1);
        for (index, (rule, count)) in self.rules.iter().enumerate() {
            let comma = if index == last { "" } else { "," };
            let _ = writeln!(text, "    \"{rule}\": {count}{comma}");
        }
        text.push_str("  }\n}\n");
        text
    }
}
