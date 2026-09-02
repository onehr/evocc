//! `evo-randgate`: run the randomized semantic gate from the command line.

use std::process::ExitCode;

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

fn run(arguments: &[String]) -> Result<String, String> {
    let _ = arguments;
    todo!("command-line gate driver")
}
