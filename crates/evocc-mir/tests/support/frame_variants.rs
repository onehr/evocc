//! Opt-in N03f/N03d replay of existing native populations. Production code
//! never reads these variables; both unset call the original entry point.
#![allow(dead_code)]

use evocc_evoir::Module;
use evocc_mir::{emit::CodegenOptions, emit::FrameVariants, NativeStop, RelocationModel, Target};

pub fn compile_module_for(
    target: Target,
    module: &Module,
    keep: &dyn Fn(&str) -> bool,
) -> Result<String, NativeStop> {
    let frame_value = std::env::var_os("EVOCC_MIR_FRAME_OPT");
    let selection_value = std::env::var_os("EVOCC_MIR_ISEL_OPT");
    if frame_value.is_none() && selection_value.is_none() {
        return evocc_mir::compile_module_for(target, module, keep);
    }
    let frame = frame_value.map_or(FrameVariants::OFF, |value| {
        FrameVariants::parse(value.to_str().expect("UTF-8 frame variant request"))
            .expect("valid native-test frame variant request")
    });
    let compact_constants = selection_value.is_some_and(|value| {
        evocc_mir::emit::parse_constant_selection(value.to_str().expect("UTF-8 selection request"))
            .expect("valid native-test selection request")
    });
    evocc_mir::compile_module_with_options(
        target,
        module,
        keep,
        RelocationModel::PositionIndependent,
        &CodegenOptions {
            frame,
            compact_constants,
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
