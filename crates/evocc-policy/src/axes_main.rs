//! `evo-cost-axes`: how many of the weight axes a registry gives a policy a
//! use for.
//!
//! One coordinate sweep around a baseline vector, counting the programs each
//! single-axis move changes. It measures the search space rather than
//! searching it, and like the search it reads no path: its corpus is a
//! generated seed range, so no acceptance corpus can reach it.

use std::process::ExitCode;

use evocc_eqopt::{policy_by_name, OperatorWeights};
use evocc_policy::{axis_sensitivity, Corpus};
use evocc_randgate::{GeneratorConfig, ProofChecker};

const DEFAULT_STEPS: u64 = 200_000;
const DEFAULT_WEIGHT_MAX: u64 = 7;

const USAGE: &str = "usage: evo-cost-axes --generated FIRST:COUNT --seed N \
[--baseline VECTOR | --baseline-policy NAME] [--weight-max N] [--steps N]";

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

fn run(arguments: &[String]) -> Result<String, String> {
    let mut generated: Option<(u64, u64)> = None;
    let mut seed = None;
    let mut baseline = None;
    let mut weight_max = DEFAULT_WEIGHT_MAX;
    let mut steps = DEFAULT_STEPS;

    let mut index = 0;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        let value = arguments.get(index + 1).ok_or_else(|| USAGE.to_owned())?;
        match flag {
            "--generated" => {
                let (first, span) = value
                    .split_once(':')
                    .ok_or_else(|| format!("{value} is not FIRST:COUNT"))?;
                generated = Some((number(first)?, number(span)?));
            }
            "--seed" => seed = Some(number(value)?),
            "--baseline" => baseline = Some(OperatorWeights::parse(value)?),
            "--baseline-policy" => {
                baseline = Some(
                    policy_by_name(value)
                        .ok_or_else(|| format!("no policy named {value}"))?
                        .cost()
                        .weights(),
                );
            }
            "--weight-max" => weight_max = number(value)?,
            "--steps" => steps = number(value)?,
            _ => return Err(USAGE.to_owned()),
        }
        index += 2;
    }

    let (first, span) = generated.ok_or_else(|| USAGE.to_owned())?;
    let seed = seed.ok_or_else(|| USAGE.to_owned())?;
    let baseline = baseline.ok_or_else(|| {
        format!("{USAGE}\na baseline vector or a shipped policy to take one from is required")
    })?;

    let corpus = Corpus::generated(first, span, GeneratorConfig::default());
    let checker = ProofChecker::locate()?;
    let sweep = axis_sensitivity(baseline, weight_max, &corpus, seed, steps, &checker);
    Ok(sweep.render_json())
}
