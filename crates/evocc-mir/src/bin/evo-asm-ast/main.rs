//! Export the restricted assembly AST of one checked Linux AArch64 compilation.
//!
//! The input is exact canonical EvoIR, as for `evo-ir-asm`. The program runs
//! the existing native pipeline with default options, emits the actual text,
//! and builds the assembly AST and the frame hints from the same allocated
//! functions. It refuses if the text of these allocated functions is not the
//! text that `compile_module_with_options` writes. Export is not proof
//! admission: the Lean print check and the Lean checker decide.
//!
//! Usage: `evo-asm-ast linux-aarch64 SHA256 INPUT.evo [--text=OUT.s]`.
//! The AST is written to standard output.

mod ast;

use std::io::Write;
use std::process::ExitCode;

use evocc_evoir::{GlobalVisibility, Linkage};

fn export(args: &[String]) -> ast::Result<(String, String)> {
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
    if !module.globals.is_empty()
        || !module.aliases.is_empty()
        || !module.assembly.is_empty()
        || !module.lifecycle.is_empty()
        || !module.tls.is_empty()
        || !module.extern_objects.is_empty()
        || !module.declares.is_empty()
    {
        return Err("module outside the core assembly profile".into());
    }
    for function in &module.functions {
        if function.alignment != 4
            || function.section.is_some()
            || function.patchable_entry.is_some()
            || function.stack_protect
            || function.linkage == Linkage::Weak
            || !matches!(
                function.visibility,
                GlobalVisibility::External | GlobalVisibility::Internal
            )
        {
            return Err(format!(
                "function {} outside the core assembly profile",
                function.name
            ));
        }
    }
    let target = evocc_mir::Target::Linux;
    let profile = evocc_mir::UbCheckProfile::Checked;
    let keep = evocc_mir::emitted_functions(&module);
    let lowered =
        evocc_mir::lower_module_for_profile(target, &module, &|name| !keep.contains(name), profile)
            .map_err(|e| format!("{e:?}"))?;
    let mut functions = Vec::new();
    for function in &lowered.functions {
        if !keep.contains(&function.name) {
            continue;
        }
        let destroyed = evocc_mir::out_of_ssa(function);
        let mut allocated =
            evocc_mir::allocate_for_with_options(target, &destroyed, Default::default())
                .map_err(|detail| format!("@{}: {detail}", function.name))?;
        allocated.internal = module
            .functions
            .iter()
            .find(|source| source.name == function.name)
            .is_some_and(|source| source.visibility == GlobalVisibility::Internal);
        evocc_mir::verify_allocation_for(target, &allocated)
            .map_err(|refusal| format!("@{}: {}", function.name, refusal.detail))?;
        functions.push(allocated);
    }
    let unit = evocc_mir::emit_module_unit(target, &functions, &[], &[]);
    let text = format!("// evocc-native-ub-checks={}\n{unit}", profile.name());
    let actual = evocc_mir::compile_module_with_options(
        target,
        &module,
        &|_| true,
        evocc_mir::RelocationModel::PositionIndependent,
        &evocc_mir::emit::CodegenOptions::default(),
    )
    .map_err(|e| format!("{e:?}"))?;
    if actual != text {
        return Err("allocated functions do not reproduce the compiler text".into());
    }
    let mut nodes = Vec::new();
    for allocated in &functions {
        let frame = evocc_mir::frame_of_for(target, allocated);
        nodes.push(ast::function(allocated, &frame)?);
    }
    let root = ast::Node::Array(vec![
        ast::Node::Text("core-asm-v1".into()),
        ast::Node::Text("aarch64-unknown-linux-gnu".into()),
        ast::Node::Text("checked".into()),
        ast::Node::Text(format!(
            "EvoCC {} aarch64-unknown-linux-gnu",
            env!("CARGO_PKG_VERSION")
        )),
        ast::Node::Array(nodes),
    ]);
    let mut output = String::new();
    root.render(&mut output);
    output.push('\n');
    Ok((output, actual))
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
