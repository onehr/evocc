//! Opt-in N03f replay of existing native populations. Production code never
//! reads this variable; an unset variable calls the original entry point.
#![allow(dead_code)]

use evocc_evoir::Module;
use evocc_mir::{emit::CodegenOptions, emit::FrameVariants, NativeStop, RelocationModel, Target};

pub fn compile_module_for(
    target: Target,
    module: &Module,
    keep: &dyn Fn(&str) -> bool,
) -> Result<String, NativeStop> {
    let Some(value) = std::env::var_os("EVOCC_MIR_FRAME_OPT") else {
        return evocc_mir::compile_module_for(target, module, keep);
    };
    let frame = FrameVariants::parse(value.to_str().expect("UTF-8 frame variant request"))
        .expect("valid native-test frame variant request");
    evocc_mir::compile_module_with_options(
        target,
        module,
        keep,
        RelocationModel::PositionIndependent,
        &CodegenOptions {
            frame,
            ..Default::default()
        },
    )
}

pub fn compile_module_with(
    module: &Module,
    keep: &dyn Fn(&str) -> bool,
) -> Result<String, NativeStop> {
    compile_module_for(Target::Darwin, module, keep)
}

pub fn compile_module(module: &Module) -> Result<String, NativeStop> {
    compile_module_with(module, &|_| true)
}
