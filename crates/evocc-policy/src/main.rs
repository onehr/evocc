//! `evo-policy-eval`: measure one optimizer policy over one corpus.
//!
//! The report is JSON on standard output and is a function of the policy, the
//! corpus and the seed alone -- except for wall time, which `--elide-timing`
//! removes when a byte-comparable artifact is what is wanted.

use std::path::PathBuf;
use std::process::ExitCode;

use evocc_eqopt::{policy_by_name, shipped_policies};
use evocc_policy::{evaluate, Corpus, TimingMode};
use evocc_randgate::{GeneratorConfig, ProofChecker};

const DEFAULT_DENYLIST: &str = "tests/corpora/kernel-cohort-digests.json";
const DEFAULT_STEPS: u64 = 200_000;

const USAGE: &str = "usage: evo-policy-eval --policy NAME --seed N \
[--generated FIRST:COUNT | --corpus DIRECTORY] [--steps N] [--denylist PATH] \
[--elide-timing]";

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

struct Options {
    policy: String,
    seed: u64,
    generated: Option<(u64, u64)>,
    corpus: Option<PathBuf>,
    steps: u64,
    denylist: PathBuf,
    timing: TimingMode,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut policy = None;
    let mut seed = None;
    let mut generated = None;
    let mut corpus = None;
    let mut steps = DEFAULT_STEPS;
    let mut denylist = PathBuf::from(DEFAULT_DENYLIST);
    let mut timing = TimingMode::Measured;

    let mut index = 0;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        if flag == "--elide-timing" {
            timing = TimingMode::Elided;
            index += 1;
            continue;
        }
        let value = arguments.get(index + 1).ok_or_else(|| USAGE.to_owned())?;
        match flag {
            "--policy" => policy = Some(value.clone()),
            "--seed" => seed = Some(number(value)?),
            "--generated" => generated = Some(range(value)?),
            "--corpus" => corpus = Some(PathBuf::from(value)),
            "--steps" => steps = number(value)?,
            "--denylist" => denylist = PathBuf::from(value),
            _ => return Err(USAGE.to_owned()),
        }
        index += 2;
    }

    if generated.is_some() == corpus.is_some() {
        return Err(format!("{USAGE}\nexactly one corpus source is required"));
    }
    Ok(Options {
        policy: policy.ok_or_else(|| USAGE.to_owned())?,
        seed: seed.ok_or_else(|| USAGE.to_owned())?,
        generated,
        corpus,
        steps,
        denylist,
        timing,
    })
}

fn number(text: &str) -> Result<u64, String> {
    text.parse::<u64>()
        .map_err(|_| format!("{text} is not a non-negative integer"))
}

fn range(text: &str) -> Result<(u64, u64), String> {
    let (first, count) = text
        .split_once(':')
        .ok_or_else(|| format!("{text} is not FIRST:COUNT"))?;
    Ok((number(first)?, number(count)?))
}

fn run(arguments: &[String]) -> Result<String, String> {
    let options = parse(arguments)?;
    let policy = policy_by_name(&options.policy).ok_or_else(|| {
        let known = shipped_policies()
            .iter()
            .map(|policy| policy.name())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "no policy named {}; this producer ships {known}",
            options.policy
        )
    })?;
    let corpus = match (options.generated, &options.corpus) {
        (Some((first, count)), None) => Corpus::generated(first, count, GeneratorConfig::default()),
        (None, Some(directory)) => Corpus::from_directory(directory, Some(&options.denylist))
            .map_err(|refusal| refusal.message())?,
        _ => return Err(USAGE.to_owned()),
    };
    let checker = ProofChecker::locate()?;
    let report = evaluate(&policy, &corpus, options.seed, options.steps, &checker);
    Ok(report.render_json(options.timing))
}
