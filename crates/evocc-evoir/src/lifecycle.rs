//! Explicit program entry; ordinary interpret_module remains a function call.
use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Initializer,
    Main,
    Finalizer,
}

pub(crate) fn program_entries(
    module: &Module,
    main: usize,
    program: bool,
) -> std::collections::VecDeque<(usize, EntryKind)> {
    let mut entries = std::collections::VecDeque::new();
    if program {
        let mut init: Vec<_> = module
            .lifecycle
            .iter()
            .enumerate()
            .filter(|(_, r)| r.phase == LifecyclePhase::Constructor)
            .collect();
        init.sort_by_key(|(i, r)| (r.priority, *i));
        for (_, r) in init {
            entries.push_back((
                module
                    .functions
                    .iter()
                    .position(|f| f.name == r.function)
                    .expect("validated registration"),
                EntryKind::Initializer,
            ));
        }
    }
    entries.push_back((main, EntryKind::Main));
    if program {
        let mut fini: Vec<_> = module
            .lifecycle
            .iter()
            .enumerate()
            .filter(|(_, r)| r.phase == LifecyclePhase::Destructor)
            .collect();
        fini.sort_by_key(|(i, r)| std::cmp::Reverse((r.priority, *i)));
        for (_, r) in fini {
            entries.push_back((
                module
                    .functions
                    .iter()
                    .position(|f| f.name == r.function)
                    .expect("validated registration"),
                EntryKind::Finalizer,
            ));
        }
    }
    entries
}

/// One ELF load instance, one memory state, and one budget for init/main/fini.
/// Loader-driven dlopen/dlclose is not simulated by this single-module API.
pub fn interpret_program(
    module: &Module,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    interpret_program_with_site(module, entry_args, limits, &mut None)
}

/// Preserve the execution site on failure without changing the error or
/// executing the program a second time. Initialization errors have no site.
pub fn interpret_program_with_site(
    module: &Module,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
    site: &mut Option<ExecutionSite>,
) -> Result<InterpreterOutcome, InterpreterError> {
    *site = None;
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let alignment = module
        .functions
        .iter()
        .map(|f| f.alignment)
        .max()
        .unwrap_or(4);
    let state = ExecutionState::for_target(&target, alignment)?;
    interpret_module_execution(module, "main", entry_args, limits, state, true, site)
}
