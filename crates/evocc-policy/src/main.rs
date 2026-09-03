//! `evo-policy-eval`: measure one optimizer policy over one corpus.
//!
//! The report is JSON on standard output and is a function of the policy, the
//! corpus and the seed alone -- except for wall time, which `--elide-timing`
//! removes when a byte-comparable artifact is what is wanted.

use std::path::PathBuf;
use std::process::ExitCode;

use evocc_eqopt::{policy_by_name, shipped_policies, OperatorWeights};
use evocc_policy::{evaluate, searched_policy, Corpus, TimingMode};
use evocc_randgate::{GeneratorConfig, ProofChecker};

const DEFAULT_DENYLIST: &str = "tests/corpora/kernel-cohort-digests.json";
const DEFAULT_STEPS: u64 = 200_000;

const USAGE: &str = "usage: evo-policy-eval (--policy NAME | --weights VECTOR) --seed N \
[--generated FIRST:COUNT | --corpus DIRECTORY] [--steps N] [--denylist PATH] \
[--elide-timing]\n  VECTOR is add=1,sub=1,mul=1,and=1,or=1,xor=1,shl=1,lshr=1,ashr=1,\
zext=1,sext=1,trunc=1,icmp=1";

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
    policy: Option<String>,
    weights: Option<OperatorWeights>,
    seed: u64,
    generated: Option<(u64, u64)>,
    corpus: Option<PathBuf>,
    steps: u64,
    denylist: PathBuf,
    timing: TimingMode,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut policy = None;
    let mut weights = None;
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
            "--weights" => weights = Some(OperatorWeights::parse(value)?),
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
    if policy.is_some() == weights.is_some() {
        return Err(format!(
            "{USAGE}\nexactly one of --policy and --weights names the cost function"
        ));
    }
    Ok(Options {
        policy,
        weights,
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
    let policy = match (&options.policy, options.weights) {
        (Some(name), None) => policy_by_name(name).ok_or_else(|| {
            let known = shipped_policies()
                .iter()
                .map(|policy| policy.name())
                .collect::<Vec<_>>()
                .join(", ");
            format!("no policy named {name}; this producer ships {known}")
        })?,
        (None, Some(weights)) => searched_policy(weights),
        _ => return Err(USAGE.to_owned()),
    };
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
