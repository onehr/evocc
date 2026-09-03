//! `evo-cost-search`: search the cost vector on a training corpus.
//!
//! The search is a (mu+lambda) evolutionary loop stated in `search.rs` and
//! seeded from the command line, so a run is reproducible from its arguments
//! alone. Fitness is the total dynamic interpreter step count after
//! optimization, measured through the same harness `evo-policy-eval` uses; a
//! candidate that did not replay every certificate is discarded and counted.
//!
//! The corpus is a generated seed range. The held-out range is a separate one,
//! and the search never reads it.

use std::process::ExitCode;

use evocc_policy::{search_cost_weights, training_corpus, Corpus, SearchConfig, HELD_OUT_RANGE};
use evocc_randgate::{GeneratorConfig, ProofChecker};

const DEFAULT_STEPS: u64 = 200_000;

const USAGE: &str = "usage: evo-cost-search [--search-seed N] [--harness-seed N] \
[--population N] [--parents N] [--children N] [--generations N] [--weight-max N] \
[--generated FIRST:COUNT] [--steps N]";

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn number(text: &str) -> Result<u64, String> {
    text.parse::<u64>()
        .map_err(|_| format!("{text} is not a non-negative integer"))
}

fn count(text: &str) -> Result<usize, String> {
    text.parse::<usize>()
        .map_err(|_| format!("{text} is not a non-negative integer"))
}

fn run(arguments: &[String]) -> Result<String, String> {
    let mut config = SearchConfig::default();
    let mut harness_seed = 7_u64;
    let mut steps = DEFAULT_STEPS;
    let mut generated: Option<(u64, u64)> = None;

    let mut index = 0;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        let value = arguments.get(index + 1).ok_or_else(|| USAGE.to_owned())?;
        match flag {
            "--search-seed" => config.seed = number(value)?,
            "--harness-seed" => harness_seed = number(value)?,
            "--population" => config.population = count(value)?,
            "--parents" => config.parents = count(value)?,
            "--children" => config.children = count(value)?,
            "--generations" => config.generations = count(value)?,
            "--weight-max" => config.weight_max = number(value)?,
            "--steps" => steps = number(value)?,
            "--generated" => {
                let (first, span) = value
                    .split_once(':')
                    .ok_or_else(|| format!("{value} is not FIRST:COUNT"))?;
                generated = Some((number(first)?, number(span)?));
            }
            _ => return Err(USAGE.to_owned()),
        }
        index += 2;
    }

    if config.population == 0 || config.parents == 0 {
        return Err("a search needs at least one parent and one initial vector".to_owned());
    }

    let corpus = match generated {
        Some((first, span)) => {
            // A caller may train on any generated range but one: the held-out
            // range is what judges the result, and a search that saw it would
            // report a number about itself.
            let (held_first, held_count) = HELD_OUT_RANGE;
            if first < held_first + held_count && held_first < first + span {
                return Err(
                    "the requested range overlaps the held-out corpus; a search never trains \
                     on the corpus that judges it"
                        .to_owned(),
                );
            }
            Corpus::generated(first, span, GeneratorConfig::default())
        }
        None => training_corpus(),
    };

    let checker = ProofChecker::locate()?;
    let outcome = search_cost_weights(&config, &corpus, harness_seed, steps, &checker);
    Ok(outcome.render_json())
}
