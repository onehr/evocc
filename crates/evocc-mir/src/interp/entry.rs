use super::*;

/// Interpret a module's named entry function.
pub fn interpret_module(
    module: &MModule,
    entry: &str,
    args: &[MirValue],
    limits: MirLimits,
) -> Result<MirOutcome, MirError> {
    interpret_module_with_target(
        module,
        entry,
        args,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        limits,
    )
}

pub fn interpret_module_with_target(
    module: &MModule,
    entry: &str,
    args: &[MirValue],
    target: &TargetDataLayout,
    limits: MirLimits,
) -> Result<MirOutcome, MirError> {
    interpret_module_with_fp_environment(
        module,
        entry,
        args,
        limits,
        evocc_types::floating::FpEnvironment::default(),
        target,
    )
}
