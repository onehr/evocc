//! `evo-randgate`: run the randomized semantic gate from the command line.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use evocc_eqopt::ScalarEqBudget;
use evocc_randgate::{run_campaign, CampaignReport, GateBudget, GeneratorConfig, ProofChecker};

const USAGE: &str = "usage: evo-randgate --seed N --count M \
[--mode standard|native-main] [--masks on|off] \
[--budget-nodes N] [--budget-epochs N] [--steps N] [--out DIRECTORY]";

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        Ok((text, refused)) => {
            print!("{text}");
            if refused {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

struct Options {
    seed: u64,
    count: u64,
    config: GeneratorConfig,
    budget: GateBudget,
    out: Option<PathBuf>,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut seed = None;
    let mut count = None;
    let mut nodes = 20_000_usize;
    let mut epochs = 256_usize;
    let mut steps = GateBudget::default().step_budget;
    let mut out = None;
    let mut config = GeneratorConfig::default();
    let mut masks = false;

    let mut index = 0;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        let value = arguments.get(index + 1).ok_or_else(|| USAGE.to_owned())?;
        match flag {
            "--seed" => seed = Some(number(value)?),
            "--count" => count = Some(number(value)?),
            "--budget-nodes" => nodes = usize::try_from(number(value)?).map_err(|_| USAGE)?,
            "--budget-epochs" => epochs = usize::try_from(number(value)?).map_err(|_| USAGE)?,
            "--steps" => steps = number(value)?,
            "--out" => out = Some(PathBuf::from(value)),
            "--mode" => {
                config = match value.as_str() {
                    "standard" => GeneratorConfig::default(),
                    "native-main" => GeneratorConfig::native_main(),
                    other => return Err(format!("{other} is not a generator mode")),
                };
            }
            // D25: masked accesses, in either mode.
            "--masks" => {
                masks = match value.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("{other} is not on or off")),
                };
            }
            _ => return Err(USAGE.to_owned()),
        }
        index += 2;
    }

    if masks {
        config = config.with_masked_accesses();
    }
    Ok(Options {
        seed: seed.ok_or_else(|| USAGE.to_owned())?,
        count: count.ok_or_else(|| USAGE.to_owned())?,
        config,
        budget: GateBudget {
            scalar: ScalarEqBudget::new(nodes, epochs),
            step_budget: steps,
        },
        out,
    })
}

fn number(text: &str) -> Result<u64, String> {
    text.parse::<u64>()
        .map_err(|_| format!("{text} is not a non-negative integer"))
}

fn run(arguments: &[String]) -> Result<(String, bool), String> {
    let options = parse(arguments)?;
    let checker = ProofChecker::locate()?;
    let report = run_campaign(
        options.seed,
        options.count,
        options.config,
        options.budget,
        &checker,
    );
    let mut text = report.render();
    if let Some(directory) = &options.out {
        text.push_str(&save(&report, directory)?);
    }
    let refused = report.mismatched != 0;
    Ok((text, refused))
}

/// Write every retained minimal reproducer next to the seed that produced it.
fn save(report: &CampaignReport, directory: &PathBuf) -> Result<String, String> {
    if report.mismatches.is_empty() {
        return Ok(String::new());
    }
    fs::create_dir_all(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let mut written = String::new();
    for mismatch in &report.mismatches {
        let stem = format!("seed-{:020}", mismatch.seed);
        for (suffix, contents) in [
            ("before.evo", &mismatch.before),
            ("after.evo", &mismatch.after),
            ("proof.evp", &mismatch.proof),
        ] {
            let path = directory.join(format!("{stem}.{suffix}"));
            fs::write(&path, contents).map_err(|error| format!("{}: {error}", path.display()))?;
        }
        let path = directory.join(format!("{stem}.report.txt"));
        let contents = format!(
            "seed={}\nkind={:?}\ndetail={}\n",
            mismatch.seed, mismatch.kind, mismatch.detail
        );
        fs::write(&path, contents).map_err(|error| format!("{}: {error}", path.display()))?;
        written.push_str(&format!("reproducer={}\n", directory.join(stem).display()));
    }
    Ok(written)
}
