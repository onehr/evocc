//! Exact-input MIR adapter. A matching hash is not proof admission: callers
//! must independently verify the chain before passing its canonical output.
use std::io::Write;
use std::process::ExitCode;

fn emit(args: &[String]) -> Result<String, String> {
    if args.len() < 3 {
        return Err("usage: evo-ir-asm (darwin-arm64|linux-aarch64) SHA256 INPUT.evo [--ub-checks=checked|unchecked] [--retain-dead-computations] [--dce-report]".into());
    }
    let mut profile = evocc_mir::UbCheckProfile::Checked;
    let mut retain_dead = false;
    let mut report = false;
    let mut seen = std::collections::BTreeSet::new();
    for flag in &args[3..] {
        let key = flag.split('=').next().unwrap_or(flag);
        if !seen.insert(key) {
            return Err("duplicate backend option".into());
        }
        if let Some(value) = flag.strip_prefix("--ub-checks=") {
            profile = evocc_mir::UbCheckProfile::parse(value)?;
        } else if flag == "--retain-dead-computations" {
            retain_dead = true;
        } else if flag == "--dce-report" {
            report = true;
        } else {
            return Err("unknown backend option".into());
        }
    }
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
    if report {
        let keep = evocc_mir::emitted_functions(&module);
        let lowered = evocc_mir::lower_module_for_profile(
            target,
            &module,
            &|name| !keep.contains(name),
            profile,
        )
        .map_err(|e| format!("{e:?}"))?;
        let mut direct_unused = 0;
        for f in &module.functions {
            if !keep.contains(&f.name) {
                continue;
            }
            let uses: std::collections::BTreeSet<_> = f
                .blocks
                .iter()
                .flat_map(|b| b.insts.iter().chain(std::iter::once(&b.terminator)))
                .flat_map(evocc_evoir::inst_uses)
                .map(|v| v.id)
                .collect();
            for inst in f.blocks.iter().flat_map(|b| &b.insts) {
                if matches!(
                    inst,
                    evocc_evoir::Inst::Const(..)
                        | evocc_evoir::Inst::Add(..)
                        | evocc_evoir::Inst::Sub(..)
                        | evocc_evoir::Inst::Mul(..)
                        | evocc_evoir::Inst::And(..)
                        | evocc_evoir::Inst::Or(..)
                        | evocc_evoir::Inst::Xor(..)
                        | evocc_evoir::Inst::Shl(..)
                        | evocc_evoir::Inst::LShr(..)
                        | evocc_evoir::Inst::AShr(..)
                        | evocc_evoir::Inst::ZExt(..)
                        | evocc_evoir::Inst::SExt(..)
                        | evocc_evoir::Inst::Trunc(..)
                        | evocc_evoir::Inst::ICmp(..)
                ) && evocc_evoir::inst_results(inst)
                    .iter()
                    .all(|v| !uses.contains(&v.id))
                {
                    direct_unused += 1;
                }
            }
        }
        let mut before = 0;
        let mut dead_before = 0;
        let mut after = 0;
        let mut dead_after = 0;
        for f in &lowered.functions {
            if !keep.contains(&f.name) {
                continue;
            }
            before += f.blocks.iter().map(|b| b.insts.len()).sum::<usize>();
            dead_before += evocc_mir::regalloc::dead_computation_count(f);
            let lowered = evocc_mir::regalloc::out_of_ssa_with_dce(f, !retain_dead);
            after += lowered.blocks.iter().map(|b| b.insts.len()).sum::<usize>();
            dead_after += evocc_mir::regalloc::dead_computation_count(&lowered);
        }
        eprintln!("{{\"schema\":\"evocc-dce-attribution-v1\",\"baseline_dce\":{},\"ir_direct_unused_p0_results\":{direct_unused},\"mir_instructions_before_ssa\":{before},\"mir_dead_before_ssa\":{dead_before},\"mir_instructions_after_ssa\":{after},\"mir_dead_after_ssa\":{dead_after},\"claim\":\"PRODUCER_DIAGNOSTIC_NOT_EQSAT_BENEFIT\"}}", !retain_dead);
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
            retain_dead_computations: retain_dead,
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
