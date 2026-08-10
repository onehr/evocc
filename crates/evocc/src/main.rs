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
//! Day-2 adds five pipeline-stage probes matching the acceptance contract:
//! preprocess, parse, c-hir-emit, evoir-emit, and eqsat-optimize. All remain
//! RED (NOT_IMPLEMENTED) because the data structures exist but the pipeline
//! stages that connect them do not.

use std::process::ExitCode;

/// Capabilities known to the flight lab. The first group is the original
/// Day-1 set; the second group covers the five pipeline stages probed by
/// Day-2 acceptance tests.
const CAPABILITIES: &[&str] = &[
    "parse",
    "evoir",
    "eqsat",
    "codegen",
    "kernel-build",
    "preprocess",
    "parse-only",
    "c-hir-emit",
    "evoir-emit",
    "eqsat-optimize",
];

/// CLI flags that map to capability names. Any invocation that ends with
/// `--probe` and contains a known flag registers as a capability probe.
const FLAG_MAP: &[(&str, &str)] = &[
    ("-E", "preprocess"),
    ("--parse-only", "parse-only"),
    ("--emit=c-hir", "c-hir-emit"),
    ("--emit=evoir", "evoir-emit"),
    ("-Oeq", "eqsat-optimize"),
];

/// Exit status for every unimplemented capability. Distinct from generic
/// failure (1) so the harness can distinguish "correctly absent" from
/// "broken in an unknown way".
const NOT_IMPLEMENTED_EXIT: u8 = 69; // EX_UNAVAILABLE: service unavailable.

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Probe-mode invocation: ends with --probe.
    if args.last().map(|s| s.as_str()) == Some("--probe") {
        let probe_args: Vec<&str> = args[..args.len() - 1].iter().map(|s| s.as_str()).collect();

        // If any known flag appears, map it to a capability name.
        for (flag, capability) in FLAG_MAP {
            if probe_args.iter().any(|a| a == flag || a.starts_with(flag)) {
                eprintln!("evocc: capability `{capability}` is NOT_IMPLEMENTED");
                return ExitCode::from(NOT_IMPLEMENTED_EXIT);
            }
        }

        // Fallback: check the positional probe pattern from Day-1.
        if probe_args.len() == 1 && CAPABILITIES.contains(&probe_args[0]) {
            let cap = probe_args[0];
            eprintln!("evocc: capability `{cap}` is NOT_IMPLEMENTED");
            return ExitCode::from(NOT_IMPLEMENTED_EXIT);
        }

        // Unknown flag combination with --probe: still fail closed.
        eprintln!("evocc: unrecognized probe invocation is NOT_IMPLEMENTED: {args:?}");
        return ExitCode::from(NOT_IMPLEMENTED_EXIT);
    }

    // Non-probe invocations: spec-compliant or not, all fail closed.
    if args.is_empty() {
        eprintln!("usage: evocc <{}> --probe", CAPABILITIES.join("|"));
        eprintln!("evocc: no compiler pipeline exists yet; every capability is NOT_IMPLEMENTED");
    } else {
        eprintln!("evocc: unrecognized invocation is NOT_IMPLEMENTED: {args:?}");
    }
    ExitCode::from(NOT_IMPLEMENTED_EXIT)
}
