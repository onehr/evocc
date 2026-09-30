//! Exact-input MIR adapter. A matching hash is not proof admission: callers
//! must independently verify the chain before passing its canonical output.
use std::io::Write;
use std::process::ExitCode;

fn emit(args: &[String]) -> Result<String, String> {
    if !matches!(args.len(), 3 | 4) {
        return Err("usage: evo-ir-asm (darwin-arm64|linux-aarch64) SHA256 INPUT.evo [--ub-checks=checked|unchecked]".into());
    }
    let profile = match args.get(3) {
        Some(flag) => evocc_mir::UbCheckProfile::parse(
            flag.strip_prefix("--ub-checks=")
                .ok_or("expected --ub-checks=PROFILE")?,
        )?,
        None => evocc_mir::UbCheckProfile::Checked,
    };
    let target = match args[0].as_str() {
        "darwin-arm64" => evocc_mir::Target::Darwin,
        "linux-aarch64" => evocc_mir::Target::Linux,
        _ => return Err("unsupported target".into()),
    };
    let bytes = std::fs::read(&args[2]).map_err(|e| e.to_string())?;
    if evocc_proof::sha256_hex(&bytes) != args[1] {
        return Err("codegen input digest mismatch".into());
    }
    let module = evocc_evoir::parse_section9_module(&bytes).map_err(|e| e.to_string())?;
    evocc_evoir::validate_module(&module).map_err(|e| e.to_string())?;
    if evocc_evoir::render_module(&module).as_bytes() != bytes {
        return Err("codegen input is not canonical EvoIR".into());
    }
    // Only obligation branch emission varies. PIC and allocation checks remain.
    // This adapter never reruns frontend lowering or EqSat.
    evocc_mir::compile_module_with_options(
        target,
        &module,
        &|_| true,
        evocc_mir::RelocationModel::PositionIndependent,
        &evocc_mir::emit::CodegenOptions {
            ub_checks: profile,
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:?}"))
}

fn main() -> ExitCode {
    match emit(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(assembly) => match std::io::stdout().lock().write_all(assembly.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(2)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
