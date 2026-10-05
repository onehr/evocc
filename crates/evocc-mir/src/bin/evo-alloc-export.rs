//! Export the actual allocator input, output and proposed block states for
//! the Lean allocation checker. The input boundary is the one of
//! `evo-ir-asm --emit=mir-post-ssa-json`: canonical EvoIR with a matching
//! digest, checked Linux lowering and no backend variants. The output is
//! not a proof; the Lean checker must accept it.
use std::io::Write;
use std::process::ExitCode;

fn emit(args: &[String]) -> Result<String, String> {
    if args.len() != 2 {
        return Err("usage: evo-alloc-export SHA256 INPUT.evo".into());
    }
    let bytes = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
    if evocc_proof::sha256_hex(&bytes) != args[0] {
        return Err("allocation input digest mismatch".into());
    }
    let module = evocc_evoir::parse_section9_module(&bytes).map_err(|e| e.to_string())?;
    evocc_evoir::validate_module(&module).map_err(|e| e.to_string())?;
    if evocc_evoir::render_module(&module).as_bytes() != bytes {
        return Err("allocation input is not canonical EvoIR".into());
    }
    let target = evocc_mir::Target::Linux;
    let keep = evocc_mir::emitted_functions(&module);
    let mut lowered = evocc_mir::lower_module_for_profile(
        target,
        &module,
        &|name| !keep.contains(name),
        evocc_mir::UbCheckProfile::Checked,
    )
    .map_err(|e| format!("{e:?}"))?;
    for function in &mut lowered.functions {
        *function = evocc_mir::regalloc::out_of_ssa(function);
    }
    evocc_mir::verify_alloc::export_core_allocation(&lowered)
}

fn main() -> ExitCode {
    match emit(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(text) => match std::io::stdout().lock().write_all(text.as_bytes()) {
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
