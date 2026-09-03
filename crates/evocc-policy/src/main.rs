//! `evo-policy-eval`: measure one optimizer policy over one corpus.

use std::process::ExitCode;

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

fn run(_arguments: &[String]) -> Result<String, String> {
    Err(USAGE.to_owned())
}
