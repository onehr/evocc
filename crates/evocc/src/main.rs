//! evocc: equality-native, self-contained C compiler.
//!
//! Flight-lab scaffold. No compiler capability exists yet; the binary's only
//! honest behavior is to fail closed. The Day-1 acceptance contract requires
//! every capability probe to:
//!
//!   1. print a line containing `NOT_IMPLEMENTED` to stderr,
//!   2. exit with a nonzero status,
//!   3. never emit another compiler's version string (which would indicate
//!      the probe was answered by delegation to GCC or Clang).
//!
//! Anything else — a silent success, a panic, or a forwarded invocation —
//! would turn a red gate into a green lie.

use std::process::ExitCode;

/// Capabilities the flight lab probes. These names are part of the
/// acceptance contract; the harness invokes `evocc <capability> --probe`.
const CAPABILITIES: &[&str] = &[
    "parse",
    "evoir",
    "eqsat",
    "codegen",
    "kernel-build",
];

/// Exit status for every unimplemented capability. Distinct from generic
/// failure (1) so the harness can distinguish "correctly absent" from
/// "broken in an unknown way" if it ever needs to.
const NOT_IMPLEMENTED_EXIT: u8 = 69; // EX_UNAVAILABLE: service unavailable.

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.as_slice() {
        [capability, flag] if flag == "--probe" && CAPABILITIES.contains(&capability.as_str()) => {
            eprintln!("evocc: capability `{capability}` is NOT_IMPLEMENTED");
            ExitCode::from(NOT_IMPLEMENTED_EXIT)
        }
        [] => {
            eprintln!("usage: evocc <{}> --probe", CAPABILITIES.join("|"));
            eprintln!("evocc: no compiler pipeline exists yet; every capability is NOT_IMPLEMENTED");
            ExitCode::from(NOT_IMPLEMENTED_EXIT)
        }
        _ => {
            // Unknown invocations fail closed too. A real driver will parse
            // Kbuild-style flags here later; until then, guessing is lying.
            eprintln!("evocc: unrecognized invocation is NOT_IMPLEMENTED: {args:?}");
            ExitCode::from(NOT_IMPLEMENTED_EXIT)
        }
    }
}
