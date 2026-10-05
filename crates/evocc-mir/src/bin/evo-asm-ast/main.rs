//! Export the restricted assembly AST of one checked Linux AArch64 compilation.
//!
//! The input is exact canonical EvoIR, as for `evo-ir-asm`. The library
//! function `evocc_mir::asm_ast::export_core_asm` runs the existing native
//! pipeline with default options, emits the actual text, and builds the
//! assembly AST and the frame hints from the same allocated functions. It
//! refuses if the text of these allocated functions is not the text that
//! `compile_module_with_options` writes. Export is not proof admission: the
//! Lean print check and the Lean checker decide.
//!
//! Usage: `evo-asm-ast linux-aarch64 SHA256 INPUT.evo [--text=OUT.s]`.
//! The AST is written to standard output.

use std::io::Write;
use std::process::ExitCode;

fn export(args: &[String]) -> evocc_mir::asm_ast::Result<(String, String)> {
    if args.len() < 3 || args.len() > 4 {
        return Err("usage: evo-asm-ast linux-aarch64 SHA256 INPUT.evo [--text=OUT.s]".into());
    }
    if args[0] != "linux-aarch64" {
        return Err("the core assembly profile requires linux-aarch64".into());
    }
    let bytes = std::fs::read(&args[2]).map_err(|e| e.to_string())?;
    if evocc_proof::sha256_hex(&bytes) != args[1] {
        return Err("codegen input digest mismatch".into());
    }
    let module = evocc_evoir::parse_section9_module(&bytes).map_err(|e| e.to_string())?;
    evocc_evoir::validate_module(&module).map_err(|e| e.to_string())?;
    if evocc_evoir::render_module(&module).as_bytes() != bytes {
        return Err("codegen input is not canonical EvoIR".into());
    }
    evocc_mir::asm_ast::export_core_asm(&module)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match export(&args) {
        Ok((ast, text)) => {
            if let Some(path) = args.get(3) {
                let Some(path) = path.strip_prefix("--text=") else {
                    eprintln!("unknown option {path}");
                    return ExitCode::from(2);
                };
                if let Err(error) = std::fs::write(path, text) {
                    eprintln!("{error}");
                    return ExitCode::from(2);
                }
            }
            match std::io::stdout().lock().write_all(ast.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::from(2)
                }
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
