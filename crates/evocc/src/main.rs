//! evocc: equality-native, self-contained C compiler.
//!
//! The driver has two command lines. The cc command line (`cc.rs`) takes
//! several inputs and the actions `-E`, `-S`, `-c` and link, and it runs the
//! platform assembler and linker. The development command line takes one
//! input and one action: preprocessing, C-HIR validation, both scalar Day-6
//! interpreters, canonical EvoIR text emission, and AArch64 assembly emission
//! through EvoMIR.
//!
//! The shell delegates real work to the crates built in Day 2-3:
//!   -E           → evocc-lex → evocc-cpp → token text output
//!   --parse-only → evocc-lex → evocc-parser → C-HIR (validate only)
//!
//! Probe mode (invocation ends with --probe) is preserved from Day 1-2
//! and answers NOT_IMPLEMENTED for stages beyond the current capability.

use std::fs;
use std::io::Write;
use std::num::NonZeroUsize;
use std::process::ExitCode;

use evocc_chir::{interp::InterpreterLimits, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_cpp::{
    is_arm_neon_header_marker, pragma_pack_action, pragma_visibility_action, PragmaPackAction,
    PragmaVisibilityAction, Preprocessor,
};
use evocc_eqopt::{run_scalar_chain_with_checker, EqualityState, ExtractionPolicy};
use evocc_evoir::{render_module, InterpreterLimits as EvoIrLimits, Module, RuntimeValue};
use evocc_lex::{Lexer, TokenKind};
use evocc_mir::emit::{CodegenOptions, StackGuard};
use evocc_mir::{compile_module_with_options, NativeStop, RelocationModel, Target as MirTarget};
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

mod cc;
mod core_export;
mod darwin;
mod deps;
mod diagnostics;
mod dwarf;
mod proof_required;
mod toolchain;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DriverMode {
    Preprocess,
    ParseOnly,
    InterpretCHir,
    EmitCHir,
    EmitCoreCHir,
    EmitEvoIr,
    EmitAsm,
    InterpretEvoIr,
    Check,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParsedAction {
    Validate,
    InterpretCHir,
    EmitCHir,
    EmitCoreCHir,
    EmitEvoIr,
    EmitAsm,
    InterpretEvoIr,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DriverTarget {
    Aarch64UnknownLinuxGnu,
    Aarch64AppleDarwin,
}

impl DriverTarget {
    fn triple(self) -> &'static str {
        match self {
            Self::Aarch64UnknownLinuxGnu => SUPPORTED_TARGET,
            Self::Aarch64AppleDarwin => DARWIN_TARGET,
        }
    }

    /// The target of the machine that runs the driver. The cc actions use it
    /// when the command line names no target.
    fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::Aarch64AppleDarwin
        } else {
            Self::Aarch64UnknownLinuxGnu
        }
    }

    fn is_darwin(self) -> bool {
        self == Self::Aarch64AppleDarwin
    }
}

#[derive(Clone, Copy)]
enum PositionIndependence {
    Disabled,
    Pic(u8),
    Pie(u8),
}

impl PositionIndependence {
    fn macro_levels(self) -> (u8, u8) {
        match self {
            Self::Disabled => (0, 0),
            Self::Pic(level) => (level, 0),
            Self::Pie(level) => (level, level),
        }
    }

    fn relocation(self) -> RelocationModel {
        match self {
            Self::Disabled => RelocationModel::Static,
            _ => RelocationModel::PositionIndependent,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LanguageMode {
    C89,
    Gnu89,
    C99,
    C11,
    C17,
    Gnu99,
    Gnu11,
    Gnu17,
}

impl LanguageMode {
    fn legacy_declarations(self) -> bool {
        matches!(self, Self::C89 | Self::Gnu89)
    }

    fn iso_keywords(self) -> bool {
        matches!(self, Self::C89 | Self::C99 | Self::C11 | Self::C17)
    }

    fn legacy_literal_prefixes(self) -> bool {
        self.legacy_declarations() || matches!(self, Self::Gnu99 | Self::C99)
    }
}

struct DriverOptions {
    mode: DriverMode,
    error_format: diagnostics::Format,
    color: diagnostics::Color,
    source_bytes: std::cell::RefCell<Vec<u8>>,
    included_bytes: std::cell::RefCell<std::collections::BTreeMap<String, Vec<u8>>>,
    target: DriverTarget,
    input: String,
    include_paths: Vec<String>,
    system_include_paths: Vec<String>,
    forced_includes: Vec<String>,
    macro_prefix_maps: Vec<(String, String)>,
    macro_actions: Vec<MacroAction>,
    standard_includes: bool,
    core_entries: Vec<String>,
    /// `--proof-required=NEW_DIR`: no output unless every integrated stage accepts.
    proof: Option<proof_required::ProofOptions>,
    optimize_scalar_evoir: bool,
    opt_passes: Vec<String>,
    pass_report: Option<std::path::PathBuf>,
    eq_policy: ExtractionPolicy,
    eq_checker: Option<std::path::PathBuf>,
    eq_checker_digest: Option<String>,
    eq_proof_dir: Option<std::path::PathBuf>,
    eq_timeout_ms: u64,
    optimization: u8,
    debug_types: bool,
    signed_wrapping: bool,
    dynamic_rounding: bool,
    standard_builtins: bool,
    hosted: bool,
    pthread: bool,
    automatic_zero: bool,
    tls_model: evocc_types::TlsModel,
    stack_protector: bool,
    patchable_entry: Option<(u16, u16)>,
    minimum_function_alignment: u32,
    codegen: CodegenOptions,
    language: LanguageMode,
    gnu_inline: bool,
    short_wchar: bool,
    signed_char: bool,
    position_independence: PositionIndependence,
    /// Report every diagnostic rather than the first.
    ///
    /// A build wants one unambiguous first-error site. Surveying a body of
    /// code for what it requires wants the whole set, or it takes one build
    /// per unsupported construct to enumerate.
    survey: bool,
    /// `--sysroot=DIR` or `-isysroot DIR`: the root of the target's headers.
    system_root: Option<String>,
    /// The cc command line runs this compilation. On a host of the same
    /// target, a cc compilation searches the host's system headers.
    cc_mode: bool,
    /// The `-f` and `-m` options that the cc command line ignores.
    ignored_options: Vec<String>,
    /// The output of a cc compilation, which the cc command line writes to a
    /// file. `None` sends the output to standard output.
    captured: Option<std::cell::RefCell<Vec<u8>>>,
    /// The source files that preprocessing opened, the input file first.
    opened_files: std::cell::RefCell<Vec<String>>,
}

impl DriverOptions {
    fn diagnostic_source(&self, file: &str) -> Vec<u8> {
        if file == self.input {
            self.source_bytes.borrow().clone()
        } else {
            self.included_bytes
                .borrow()
                .get(file)
                .cloned()
                .unwrap_or_default()
        }
    }

    fn emit_span(&self, file: &str, span: evocc_common::SourceSpan, code: &str, message: &str) {
        let bytes = self.diagnostic_source(file);
        let diagnostic = diagnostics::Diagnostic::new(
            code,
            message,
            diagnostics::Span::source(file, &bytes, span, message),
        );
        eprint!(
            "{}",
            diagnostics::render(&diagnostic, self.error_format, self.color)
        );
    }

    fn emit_error(&self, file: &str, line: u32, column: u32, code: &str, message: &str) {
        let location = (line != 0 && column != 0)
            .then_some((line, column))
            .or_else(|| (self.error_format == diagnostics::Format::Text).then_some((1, 1)));
        self.error_format.error(
            file,
            location,
            code,
            message,
            diagnostics::phase(code),
            "compilation",
        );
    }

    fn emit_execution_error(
        &self,
        file: &str,
        location: Option<(u32, u32)>,
        code: &str,
        message: &str,
    ) {
        let location = location.or_else(|| {
            (self.mode != DriverMode::Check && self.error_format == diagnostics::Format::Text)
                .then_some((1, 1))
        });
        self.error_format.error(
            file,
            location,
            code,
            message,
            "execution",
            diagnostics::execution_category(code),
        );
    }

    fn target_layout(&self) -> TargetDataLayout {
        match self.target {
            DriverTarget::Aarch64UnknownLinuxGnu => TargetDataLayout::aarch64_unknown_linux_gnu(),
            DriverTarget::Aarch64AppleDarwin => TargetDataLayout::aarch64_apple_darwin(),
        }
        .with_short_wchar(self.short_wchar)
        .with_signed_char(self.signed_char)
    }

    /// Write the output of the selected action. A cc compilation keeps it for
    /// a file; every other invocation writes it to standard output.
    fn write_output(&self, bytes: &[u8]) -> bool {
        match &self.captured {
            Some(buffer) => {
                buffer.borrow_mut().extend_from_slice(bytes);
                true
            }
            None => std::io::stdout().lock().write_all(bytes).is_ok(),
        }
    }

    /// Keep the list of source files that `pp` opened, for dependency rules.
    fn record_opened_files(&self, pp: &Preprocessor) {
        *self.opened_files.borrow_mut() =
            pp.opened_sources().into_iter().map(str::to_owned).collect();
        *self.included_bytes.borrow_mut() = pp
            .opened_sources()
            .into_iter()
            .filter(|path| *path != self.input)
            .filter_map(|path| {
                pp.source_contents(path)
                    .map(|bytes| (path.to_owned(), bytes.to_vec()))
            })
            .collect();
    }
}

enum MacroAction {
    Define(String),
    Undefine(String),
}

const SUPPORTED_TARGET: &str = "aarch64-unknown-linux-gnu";
const DARWIN_TARGET: &str = "aarch64-apple-darwin";

const CAPABILITIES: &[&str] = &[
    "parse",
    "evoir",
    "eqsat",
    "codegen",
    "kernel-build",
    "preprocess",
    "parse-only",
    "interpret-c-hir",
    "c-hir-emit",
    "evoir-emit",
    "evoir-interpret",
    "asm-emit",
    "eqsat-optimize",
    "eq-scalar-optimize",
];

const FLAG_MAP: &[(&str, &str)] = &[
    ("-E", "preprocess"),
    ("--parse-only", "parse-only"),
    ("--interpret-c-hir", "interpret-c-hir"),
    ("--emit=c-hir", "c-hir-emit"),
    ("--emit=evoir", "evoir-emit"),
    ("--emit=asm", "asm-emit"),
    ("--interpret-evoir", "evoir-interpret"),
    ("-Oeq", "eqsat-optimize"),
    ("-Oeq-scalar", "eq-scalar-optimize"),
];

const IMPLEMENTED_EVOIR_PROBES: &[&str] = &[
    "evoir-emit",
    "evoir-interpret",
    "asm-emit",
    "--emit=evoir",
    "--emit=asm",
    "--interpret-evoir",
    "eq-scalar-optimize",
    "-Oeq-scalar",
];

const NOT_IMPLEMENTED_EXIT: u8 = 69;

fn not_implemented(capability: &str) -> ExitCode {
    eprintln!("evocc: capability `{capability}` is NOT_IMPLEMENTED");
    ExitCode::from(NOT_IMPLEMENTED_EXIT)
}

fn is_probe(args: &[String]) -> bool {
    args.last().map(|s| s.as_str()) == Some("--probe")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let error_format = diagnostics::Format::for_usage(&args);

    if args.iter().any(|arg| arg == "--explain") {
        if args.len() != 2 || args[0] != "--explain" {
            error_format
                .usage("--explain requires exactly one diagnostic code and no other options");
            return ExitCode::from(1);
        }
        return match diagnostics::explain(&args[1]) {
            Ok(text) => {
                if std::io::stdout().lock().write_all(text.as_bytes()).is_ok() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                }
            }
            Err(message) => {
                error_format.usage(&message);
                ExitCode::from(1)
            }
        };
    }

    if args.iter().any(|arg| arg == "--version") {
        return cc::print_version(&args);
    }

    if args.is_empty() {
        eprintln!(
            "usage: evocc [-E|-S|-c] [-o OUTPUT] [OPTIONS] INPUT... (cc command line; see --help)"
        );
        eprintln!("       evocc <{}> --probe", CAPABILITIES.join("|"));
        eprintln!("       evocc -E [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --parse-only [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --interpret-c-hir [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --emit=evoir [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --emit=core-c-hir --core-entry=NAME -O0 -fno-builtin <file> (source input only, not a proof)");
        eprintln!("       evocc --emit=asm [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --emit=asm --proof-required=NEW_DIR --core-entry=NAME -O0 -fno-builtin [--proof-checker=PATH --proof-checker-sha256=SHA256 --proof-timeout-ms=N --proof-proposal=NAME=PATH] [-Oeq-scalar --eq-checker-sha256=SHA256 ...] <file>");
        eprintln!("       evocc --interpret-evoir [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --check [--error-format=text|json] <file>");
        eprintln!("       evocc --explain E3xxx");
        eprintln!("       -Oeq-scalar [--eq-policy=NAME | --eq-weights=VECTOR]");
        eprintln!("       --ub-checks=checked|unchecked (native only; default checked)");
        eprintln!("       --frame-opt=paired-saves,shared-epilogue,leaf-frames (native only; default none)");
        eprintln!(
            "       --isel-opt=constants,local,extensions,controls (native only; default none)"
        );
        eprintln!("       --regalloc-opt=copy-hints,reload-reuse (native only; default none)");
        eprintln!("       --opt-pass=identity [--pass-report NEW_JSON_PATH] (development TV)");
        eprintln!("       [--eq-max-nodes=N --eq-max-epochs=N --eq-max-transactions=N]");
        eprintln!("       --eq-checker-sha256=TRUSTED_SHA256 [--eq-checker=PATH --eq-checker-timeout-ms=N --eq-proof-dir=NEW_DIR]");
        eprintln!("evocc: unrecognized invocation is NOT_IMPLEMENTED");
        return ExitCode::from(NOT_IMPLEMENTED_EXIT);
    }

    if is_probe(&args) {
        let probe_args: &[String] = &args[..args.len() - 1];
        if probe_args.len() == 1 {
            let probe = probe_args[0].as_str();
            if IMPLEMENTED_EVOIR_PROBES.contains(&probe) {
                return ExitCode::SUCCESS;
            }
            if CAPABILITIES.contains(&probe) {
                return not_implemented(probe);
            }
            if let Some((_, capability)) = FLAG_MAP.iter().find(|(flag, _)| *flag == probe) {
                return not_implemented(capability);
            }
        }
        for (flag, capability) in FLAG_MAP {
            if probe_args.iter().any(|argument| argument == flag) {
                return not_implemented(capability);
            }
        }
        // Unknown probe: fail closed.
        eprintln!("evocc: unrecognized probe invocation is NOT_IMPLEMENTED: {probe_args:?}");
        return ExitCode::from(NOT_IMPLEMENTED_EXIT);
    }

    if !is_development_invocation(&args) {
        return cc::run(&args);
    }

    let options = match parse_options(&args, false) {
        Ok(options) => options,
        Err(message) => {
            error_format.usage(&message);
            return ExitCode::from(1);
        }
    };
    run_options(&options)
}

/// The development actions keep the strict command line: one input, one
/// action, every option known, and `aarch64-unknown-linux-gnu` as the default
/// target. Every other invocation is a cc invocation.
fn is_development_invocation(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--parse-only" | "--interpret-c-hir" | "--interpret-evoir" | "--check"
        ) || arg.starts_with("--emit=")
    })
}

/// Run the action that `options` selects.
fn run_options(options: &DriverOptions) -> ExitCode {
    match options.mode {
        DriverMode::Preprocess => run_preprocess(options),
        DriverMode::ParseOnly => run_parsed(options, ParsedAction::Validate),
        DriverMode::InterpretCHir => run_parsed(options, ParsedAction::InterpretCHir),
        DriverMode::EmitCHir => run_parsed(options, ParsedAction::EmitCHir),
        DriverMode::EmitCoreCHir => run_parsed(options, ParsedAction::EmitCoreCHir),
        DriverMode::EmitEvoIr => run_parsed(options, ParsedAction::EmitEvoIr),
        DriverMode::EmitAsm => run_parsed(options, ParsedAction::EmitAsm),
        DriverMode::InterpretEvoIr => run_parsed(options, ParsedAction::InterpretEvoIr),
        DriverMode::Check => run_parsed(options, ParsedAction::InterpretEvoIr),
    }
}

/// Parse one compilation. With `lenient`, an `-f` or `-m` option that the
/// driver does not know goes to `ignored_options` and is not an error.
fn parse_options(args: &[String], lenient: bool) -> Result<DriverOptions, String> {
    let mut mode = None;
    let mut error_format = None;
    let mut color = diagnostics::Color::Auto;
    let mut target = None;
    let mut input = None;
    let mut include_paths = Vec::new();
    let mut system_include_paths = Vec::new();
    let mut forced_includes = Vec::new();
    let mut macro_prefix_maps = Vec::new();
    let mut macro_actions = Vec::new();
    let mut standard_includes = true;
    let mut core_entries = Vec::new();
    let mut proof_arguments = proof_required::ProofArguments::default();
    let mut optimize_scalar_evoir = false;
    let mut opt_passes = Vec::new();
    let mut pass_report = None;
    let mut eq_policy = ExtractionPolicy::default();
    let mut eq_checker = None;
    let mut eq_checker_digest = None;
    let mut eq_cost_selected = false;
    let mut eq_proof_dir = None;
    let mut eq_timeout_ms = 30_000;
    let mut eq_options = false;
    let mut optimization = 0;
    let mut debug_types = false;
    let mut signed_wrapping = false;
    let mut dynamic_rounding = false;
    let mut standard_builtins = true;
    let mut hosted = true;
    let mut pthread = false;
    let mut automatic_zero = false;
    let mut tls_model = evocc_types::TlsModel::default();
    let mut stack_protector = false;
    let mut patchable_entry = None;
    let mut minimum_function_alignment = 4;
    let mut codegen = CodegenOptions::default();
    let mut ub_checks_selected = false;
    let mut frame_variants_selected = false;
    let mut selection_variants_selected = false;
    let mut allocation_variants_selected = false;
    let mut guard_is_sysreg = false;
    let mut guard_register = None;
    let mut guard_offset = None;
    let mut language = LanguageMode::Gnu11;
    let mut gnu_inline = None;
    let mut short_wchar = false;
    // The target decides both defaults, and the target is known only at the end.
    let mut signed_char = None;
    let mut position_independence = None;
    let mut survey = false;
    let mut system_root = None;
    let mut ignored_options = Vec::new();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "-E" => set_mode(&mut mode, DriverMode::Preprocess)?,
            "--parse-only" => set_mode(&mut mode, DriverMode::ParseOnly)?,
            "--interpret-c-hir" => set_mode(&mut mode, DriverMode::InterpretCHir)?,
            "--emit=c-hir" => set_mode(&mut mode, DriverMode::EmitCHir)?,
            "--emit=core-c-hir" => set_mode(&mut mode, DriverMode::EmitCoreCHir)?,
            _ if arg.starts_with("--core-entry=") => {
                let name = arg.strip_prefix("--core-entry=").unwrap();
                if name.is_empty() || core_entries.iter().any(|entry| entry == name) {
                    return Err(
                        "--core-entry requires a nonempty, nonduplicate function name".into(),
                    );
                }
                core_entries.push(name.to_owned());
            }
            _ if arg.starts_with("--proof-") => proof_arguments.parse(arg)?,
            "--emit=evoir" => set_mode(&mut mode, DriverMode::EmitEvoIr)?,
            "--emit=asm" => set_mode(&mut mode, DriverMode::EmitAsm)?,
            "--interpret-evoir" => set_mode(&mut mode, DriverMode::InterpretEvoIr)?,
            "--check" => set_mode(&mut mode, DriverMode::Check)?,
            _ if arg.starts_with("--color=") => {
                color = diagnostics::Color::parse(arg.strip_prefix("--color=").unwrap())?;
            }
            _ if arg.starts_with("--error-format=") => {
                let selected =
                    diagnostics::Format::parse(arg.strip_prefix("--error-format=").unwrap())?;
                if error_format.replace(selected).is_some() {
                    return Err("--error-format may appear only once".into());
                }
            }
            "-Oeq-scalar" => optimize_scalar_evoir = true,
            _ if arg.starts_with("--opt-pass=") => {
                let name = arg.strip_prefix("--opt-pass=").unwrap();
                if name != "identity" {
                    return Err(format!(
                        "unknown development pass `{name}`; supported: identity"
                    ));
                }
                opt_passes.push(name.to_owned());
            }
            "--pass-report" => {
                index += 1;
                let path = args.get(index).ok_or("--pass-report requires a path")?;
                if path.is_empty()
                    || pass_report
                        .replace(std::path::PathBuf::from(path))
                        .is_some()
                {
                    return Err("--pass-report requires one nonempty path".into());
                }
            }
            _ if arg.starts_with("--pass-report=") => {
                let path = arg.strip_prefix("--pass-report=").unwrap();
                if path.is_empty()
                    || pass_report
                        .replace(std::path::PathBuf::from(path))
                        .is_some()
                {
                    return Err("--pass-report requires one nonempty path".into());
                }
            }
            _ if arg.starts_with("--ub-checks=") => {
                if ub_checks_selected {
                    return Err("--ub-checks may appear only once".into());
                }
                ub_checks_selected = true;
                codegen.ub_checks =
                    evocc_mir::UbCheckProfile::parse(arg.strip_prefix("--ub-checks=").unwrap())?;
            }
            // EvoMIR revision 0.56: opt-in, emission-only frame variants.
            _ if arg.starts_with("--frame-opt=") => {
                if frame_variants_selected {
                    return Err("--frame-opt may appear only once".into());
                }
                frame_variants_selected = true;
                codegen.frame = evocc_mir::emit::FrameVariants::parse(
                    arg.strip_prefix("--frame-opt=").unwrap(),
                )?;
            }
            _ if arg.starts_with("--isel-opt=") => {
                if selection_variants_selected {
                    return Err("--isel-opt may appear only once".into());
                }
                selection_variants_selected = true;
                let selected = evocc_mir::emit::SelectionVariants::parse(
                    arg.strip_prefix("--isel-opt=").unwrap(),
                )?;
                codegen.compact_constants = selected.constants;
                codegen.local_selection = selected.local;
                codegen.reuse_extensions = selected.extensions;
                codegen.select_controls = selected.controls;
            }
            _ if arg.starts_with("--regalloc-opt=") => {
                if allocation_variants_selected {
                    return Err("--regalloc-opt may appear only once".into());
                }
                allocation_variants_selected = true;
                codegen.allocation = evocc_mir::generic::allocation::AllocationOptions::parse(
                    arg.strip_prefix("--regalloc-opt=").unwrap(),
                )?;
            }
            _ if arg.starts_with("--eq-") => {
                let (key, value) = arg
                    .split_once('=')
                    .ok_or("EqSat options require --eq-KEY=VALUE")?;
                if value.is_empty() {
                    return Err("empty EqSat option".into());
                }
                eq_options |= key != "--eq-proof-dir";
                match key {
                    "--eq-policy" => {
                        if eq_cost_selected {
                            return Err("--eq-policy and --eq-weights are exclusive and may appear only once".into());
                        }
                        eq_cost_selected = true;
                        let selected =
                            evocc_eqopt::policy_by_name(value).ok_or("unknown EqSat policy")?;
                        eq_policy = selected.with_schedule(eq_policy.schedule());
                    }
                    "--eq-weights" => {
                        if eq_cost_selected {
                            return Err("--eq-policy and --eq-weights are exclusive and may appear only once".into());
                        }
                        eq_cost_selected = true;
                        eq_policy = eq_policy.with_cost(evocc_eqopt::CostFunction::Vector(
                            evocc_eqopt::OperatorWeights::parse(value)?,
                        ))
                    }
                    "--eq-checker" => eq_checker = Some(std::path::PathBuf::from(value)),
                    "--eq-checker-sha256" => eq_checker_digest = Some(value.to_owned()),
                    "--eq-proof-dir" => eq_proof_dir = Some(std::path::PathBuf::from(value)),
                    "--eq-checker-timeout-ms" => {
                        eq_timeout_ms = value.parse().map_err(|_| "invalid checker timeout")?
                    }
                    "--eq-max-nodes" | "--eq-max-epochs" | "--eq-max-transactions" => {
                        let number = value.parse().map_err(|_| "invalid EqSat resource limit")?;
                        let mut schedule = eq_policy.schedule();
                        match key {
                            "--eq-max-nodes" => schedule.max_nodes = number,
                            "--eq-max-epochs" => schedule.max_epochs = number,
                            _ => schedule.max_transactions = number,
                        }
                        eq_policy = eq_policy.with_schedule(schedule);
                    }
                    _ => return Err(format!("unknown EqSat option {key}")),
                }
            }
            "-O0" => optimization = 0,
            "-O" | "-O1" | "-O2" | "-O3" | "-Og" => optimization = 1,
            "-Os" => optimization = 2,
            "-g" | "-gdwarf-4" => debug_types = true,
            "-g0" => debug_types = false,
            "-mcmodel=tiny" => codegen.tiny_addresses = true,
            "-mcmodel=small" => codegen.tiny_addresses = false,
            "-ffunction-sections" => codegen.function_sections = true,
            "-fno-function-sections" => codegen.function_sections = false,
            "-fdata-sections" => codegen.data_sections = true,
            "-fno-data-sections" => codegen.data_sections = false,
            "-fasynchronous-unwind-tables" | "-funwind-tables" => codegen.unwind_tables = true,
            "-fno-asynchronous-unwind-tables" | "-fno-unwind-tables" => {
                codegen.unwind_tables = false
            }
            "-fwrapv" | "-fno-strict-overflow" => signed_wrapping = true,
            "-fno-wrapv" | "-fstrict-overflow" => signed_wrapping = false,
            // No pass derives non-aliasing from the C access type. Existing
            // object/provenance, initializedness and permission checks remain.
            "-fno-strict-aliasing" => {}
            // Omission is permission, not a request to enable a frame variant.
            "-fomit-frame-pointer" => codegen.frame_pointer_required = false,
            "-fno-omit-frame-pointer" => codegen.frame_pointer_required = true,
            "-mno-omit-leaf-frame-pointer" => codegen.leaf_frame_pointer_required = true,
            // No sibling-call transform exists in this backend.
            "-fno-optimize-sibling-calls" => {}
            // There is no optional function/branch padding, automatic array
            // prefetch, loop-header copying or loop-to-libcall transform.
            // These disabling options select that existing contract; source
            // and minimum function alignment remain mandatory. Do not admit
            // the positive transforms or unrelated GCC tuning options here.
            "-fno-align-functions"
            | "-fno-align-jumps"
            | "-fno-align-labels"
            | "-fno-align-loops"
            | "-fno-prefetch-loop-arrays"
            | "-fno-tree-ch"
            | "-fno-tree-loop-distribute-patterns" => {}
            // Bounded GCC tuning compatibility (EvoMIR Section 6): allocation
            // already spans each whole function; no code-hoisting pass exists;
            // block placement never duplicates blocks for a trace cache. These
            // select existing behavior, not GCC's coloring/layout heuristics.
            // Do not generalize to loop regions, hoisting or STC transforms.
            "-fira-region=one" | "-fira-hoist-pressure" | "-freorder-blocks-algorithm=simple" => {}
            "-ffp-contract=off" | "-fno-fast-math" => {}
            // FLT_EVAL_METHOD=0 already rounds every arithmetic operation,
            // assignment and cast to its semantic source format. Preserve
            // that contract without changing rounding/environment selection.
            "-fexcess-precision=standard" => {}
            "-frounding-math" => dynamic_rounding = true,
            "-fno-rounding-math" => dynamic_rounding = false,
            // These options permit transforms that this compiler never makes,
            // so the cc command line can ignore them without a wrong result.
            "-ffast-math" | "-ffp-contract=fast" | "-ffp-contract=on" if lenient => {
                ignored_options.push(arg.clone());
            }
            "-ffast-math" | "-ffp-contract=fast" | "-ffp-contract=on" => {
                return Err(format!("unsupported floating-point profile option `{arg}`"));
            }
            "-fno-builtin" => standard_builtins = false,
            "-fbuiltin" => standard_builtins = true,
            "-ffreestanding" => {
                hosted = false;
                standard_builtins = false;
            }
            "-fhosted" => {
                hosted = true;
                standard_builtins = true;
            }
            "-pthread" => pthread = true,
            "-ftrivial-auto-var-init=zero" => automatic_zero = true,
            "-ftrivial-auto-var-init=uninitialized" => automatic_zero = false,
            _ if arg.starts_with("-ftls-model=") => {
                tls_model = evocc_types::TlsModel::parse(&arg[12..])
                    .ok_or_else(|| format!("unsupported TLS model `{arg}`"))?;
            }
            "-fstack-protector-strong" => stack_protector = true,
            "-fno-stack-protector" => stack_protector = false,
            _ if arg.starts_with("-fmin-function-alignment=") => {
                let value = &arg["-fmin-function-alignment=".len()..];
                minimum_function_alignment = value
                    .bytes()
                    .all(|b| b.is_ascii_digit())
                    .then(|| value.parse::<u32>().ok())
                    .flatten()
                    .filter(|value| *value <= 65536 && (*value == 0 || value.is_power_of_two()))
                    .map(|value| value.max(4))
                    .ok_or_else(|| format!("unsupported minimum function alignment `{value}`"))?;
            }
            _ if arg.starts_with("-fpatchable-function-entry=") => {
                let value = &arg["-fpatchable-function-entry=".len()..];
                let (total, before) = value.split_once(',').unwrap_or((value, "0"));
                let number = |text: &str| {
                    text.bytes()
                        .all(|b| b.is_ascii_digit())
                        .then(|| text.parse::<u16>().ok())
                        .flatten()
                };
                let pair = number(total).zip(number(before)).filter(|(n, m)| m <= n);
                patchable_entry = Some(
                    pair.ok_or_else(|| format!("invalid patchable function entry `{value}`"))?,
                );
            }
            "-mbranch-protection=pac-ret" => codegen.sign_returns = true,
            "-mbranch-protection=none" => codegen.sign_returns = false,
            "-mgeneral-regs-only" => codegen.general_registers_only = true,
            "-mno-general-regs-only" => codegen.general_registers_only = false,
            "-mstrict-align" => codegen.strict_alignment = true,
            "-mno-strict-align" => codegen.strict_alignment = false,
            "-mstack-protector-guard=sysreg" => guard_is_sysreg = true,
            "-mstack-protector-guard=global" => guard_is_sysreg = false,
            _ if arg.starts_with("-mstack-protector-guard-reg=") => {
                let register = arg.trim_start_matches("-mstack-protector-guard-reg=");
                if !matches!(
                    register,
                    "sp_el0" | "tpidr_el0" | "tpidrro_el0" | "tpidr_el1" | "tpidr_el2"
                ) {
                    return Err(format!("unsupported stack guard register `{register}`"));
                }
                guard_register = Some(register.to_owned());
            }
            _ if arg.starts_with("-mstack-protector-guard-offset=") => {
                guard_offset = Some(
                    arg.trim_start_matches("-mstack-protector-guard-offset=")
                        .parse::<i64>()
                        .map_err(|_| format!("invalid stack guard offset `{arg}`"))?,
                );
            }
            "-fshort-wchar" => short_wchar = true,
            "-fno-short-wchar" => short_wchar = false,
            "-fsigned-char" | "-fno-unsigned-char" => signed_char = Some(true),
            "-funsigned-char" | "-fno-signed-char" => signed_char = Some(false),
            "-fpic" => position_independence = Some(PositionIndependence::Pic(1)),
            "-fPIC" => position_independence = Some(PositionIndependence::Pic(2)),
            "-fpie" => position_independence = Some(PositionIndependence::Pie(1)),
            "-fPIE" => position_independence = Some(PositionIndependence::Pie(2)),
            "-fno-pic" | "-fno-PIC" | "-fno-pie" | "-fno-PIE" => {
                position_independence = Some(PositionIndependence::Disabled);
            }
            "--target" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--target requires an argument".to_string())?;
                set_target(&mut target, parse_target(value)?)?;
            }
            _ if arg.starts_with("--target=") => {
                let value = &arg["--target=".len()..];
                set_target(&mut target, parse_target(value)?)?;
            }
            "--survey" => survey = true,
            "--sysroot" | "-isysroot" => {
                index += 1;
                let value = args
                    .get(index)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("{arg} requires an argument"))?;
                system_root = Some(value.clone());
            }
            _ if arg.starts_with("--sysroot=") => {
                let value = &arg["--sysroot=".len()..];
                if value.is_empty() {
                    return Err("--sysroot requires a directory".into());
                }
                system_root = Some(value.to_owned());
            }
            "-P" | "-undef" => {}
            "-nostdinc" => standard_includes = false,
            "-I" | "-D" | "-U" | "-include" | "-isystem" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{arg} requires an argument"))?
                    .clone();
                match arg.as_str() {
                    "-I" => include_paths.push(value),
                    "-isystem" => system_include_paths.push(value),
                    "-D" => macro_actions.push(MacroAction::Define(value)),
                    "-U" => macro_actions.push(MacroAction::Undefine(value)),
                    "-include" => forced_includes.push(value),
                    _ => unreachable!("matched driver option"),
                }
            }
            _ if arg.starts_with("-I") && arg.len() > 2 => {
                include_paths.push(arg[2..].to_owned());
            }
            _ if arg.starts_with("-isystem") && arg.len() > "-isystem".len() => {
                system_include_paths.push(arg["-isystem".len()..].to_owned());
            }
            _ if arg.starts_with("-D") && arg.len() > 2 => {
                macro_actions.push(MacroAction::Define(arg[2..].to_owned()));
            }
            _ if arg.starts_with("-U") && arg.len() > 2 => {
                macro_actions.push(MacroAction::Undefine(arg[2..].to_owned()));
            }
            _ if arg.starts_with("-fmacro-prefix-map=") => {
                let payload = &arg["-fmacro-prefix-map=".len()..];
                let Some((old, new)) = payload.split_once('=') else {
                    return Err(format!(
                        "invalid -fmacro-prefix-map `{payload}`; expected non-empty OLD in OLD=NEW"
                    ));
                };
                if old.is_empty() {
                    return Err(format!(
                        "invalid -fmacro-prefix-map `{payload}`; expected non-empty OLD in OLD=NEW"
                    ));
                }
                macro_prefix_maps.push((old.to_owned(), new.to_owned()));
            }
            "-std=gnu11" => language = LanguageMode::Gnu11,
            "-std=gnu99" | "-std=gnu9x" => language = LanguageMode::Gnu99,
            "-std=c89" | "-std=c90" | "-ansi" | "-std=iso9899:1990" => language = LanguageMode::C89,
            "-std=c99" | "-std=c9x" | "-std=iso9899:1999" => language = LanguageMode::C99,
            "-std=c11" | "-std=c1x" | "-std=iso9899:2011" => language = LanguageMode::C11,
            "-std=c17" | "-std=c18" | "-std=iso9899:2017" | "-std=iso9899:2018" => {
                language = LanguageMode::C17;
            }
            "-std=gnu17" | "-std=gnu18" => language = LanguageMode::Gnu17,
            "-std=gnu89" | "-std=gnu90" => language = LanguageMode::Gnu89,
            "-fgnu89-inline" => gnu_inline = Some(true),
            "-fno-gnu89-inline" => gnu_inline = Some(false),
            _ if arg.starts_with("-std=") => {
                return Err(format!("unsupported C language mode `{arg}`"));
            }
            _ if lenient && (arg.starts_with("-f") || arg.starts_with("-m")) => {
                ignored_options.push(arg.clone());
            }
            _ if arg.starts_with('-') && arg != "/dev/stdin" => {
                return Err(format!("unsupported option `{arg}`"));
            }
            _ => {
                if input.replace(arg.clone()).is_some() {
                    return Err("multiple input files are not supported".into());
                }
            }
        }
        index += 1;
    }

    if language.legacy_declarations() && gnu_inline == Some(false) {
        return Err("unsupported inline mode: -fno-gnu89-inline with C89/GNU89".into());
    }

    let mode = mode.ok_or_else(|| {
            "exactly one of -E, --parse-only, --interpret-c-hir, --emit=evoir, --emit=asm, or --interpret-evoir is required"
                .to_string()
        })?;
    let proof = proof_arguments.admit(proof_required::Admission {
        emit_asm: mode == DriverMode::EmitAsm,
        entries: &core_entries,
        baseline: optimization == 0 && !standard_builtins,
        rewrites: optimize_scalar_evoir,
        rewrite_checker_pinned: eq_checker_digest.is_some(),
        unchanged_backend: opt_passes.is_empty()
            && pass_report.is_none()
            && (optimize_scalar_evoir || (eq_proof_dir.is_none() && !eq_options))
            && !frame_variants_selected
            && !selection_variants_selected
            && !allocation_variants_selected
            && codegen.ub_checks == evocc_mir::UbCheckProfile::Checked,
    })?;
    if mode == DriverMode::EmitCoreCHir {
        if core_entries.is_empty() || optimization != 0 || standard_builtins {
            return Err("Core C-HIR export requires --core-entry=NAME, -O0 and -fno-builtin; it is not a compilation proof".into());
        }
    } else if !core_entries.is_empty() && proof.is_none() {
        return Err("--core-entry requires --emit=core-c-hir".into());
    }
    if optimize_scalar_evoir
        && !matches!(
            mode,
            DriverMode::EmitEvoIr
                | DriverMode::EmitAsm
                | DriverMode::InterpretEvoIr
                | DriverMode::Check
        )
    {
        return Err(
            "-Oeq-scalar requires --emit=evoir, --emit=asm or --interpret-evoir".to_owned(),
        );
    }
    if (!opt_passes.is_empty() || pass_report.is_some())
        && !matches!(
            mode,
            DriverMode::EmitEvoIr
                | DriverMode::EmitAsm
                | DriverMode::InterpretEvoIr
                | DriverMode::Check
        )
    {
        return Err(
            "development passes and --pass-report require an EvoIR execution or emission mode"
                .into(),
        );
    }
    let schedule = eq_policy.schedule();
    if eq_proof_dir.is_some()
        && !matches!(
            mode,
            DriverMode::EmitEvoIr
                | DriverMode::EmitAsm
                | DriverMode::InterpretEvoIr
                | DriverMode::Check
        )
    {
        return Err("--eq-proof-dir requires an EvoIR execution or emission mode".into());
    }
    if (eq_options && !optimize_scalar_evoir)
        || schedule.max_nodes > 100_000
        || schedule.max_epochs > 1_024
        || !(1..=8).contains(&schedule.max_transactions)
        || !(1..=300_000).contains(&eq_timeout_ms)
    {
        return Err("EqSat options require -Oeq-scalar and admitted resource limits".into());
    }
    if guard_is_sysreg {
        codegen.stack_guard = StackGuard::SystemRegister {
            register: guard_register.ok_or("sysreg stack guard requires its register")?,
            offset: guard_offset.ok_or("sysreg stack guard requires its byte offset")?,
        };
    }
    if mode == DriverMode::Check && ub_checks_selected {
        return Err("--ub-checks is native-only and cannot weaken --check".into());
    }
    if mode == DriverMode::Check && frame_variants_selected {
        return Err("--frame-opt is native-only and has no meaning for --check".into());
    }
    if mode == DriverMode::Check && selection_variants_selected {
        return Err("--isel-opt is native-only and has no meaning for --check".into());
    }
    if mode == DriverMode::Check && allocation_variants_selected {
        return Err("--regalloc-opt is native-only and has no meaning for --check".into());
    }
    let target = target.unwrap_or(DriverTarget::Aarch64UnknownLinuxGnu);
    // Apple's arm64 convention passes every anonymous argument on the stack,
    // and its `va_list` is one pointer.
    codegen.anonymous_on_stack = target.is_darwin();
    Ok(DriverOptions {
        mode,
        error_format: error_format.unwrap_or_default(),
        color,
        source_bytes: std::cell::RefCell::new(Vec::new()),
        included_bytes: std::cell::RefCell::new(std::collections::BTreeMap::new()),
        target,
        input: input.ok_or_else(|| "an input file is required".to_string())?,
        include_paths,
        system_include_paths,
        forced_includes,
        macro_prefix_maps,
        macro_actions,
        standard_includes,
        core_entries,
        proof,
        optimize_scalar_evoir,
        opt_passes,
        pass_report,
        eq_policy,
        eq_checker,
        eq_checker_digest,
        eq_proof_dir,
        eq_timeout_ms,
        optimization,
        debug_types,
        signed_wrapping,
        dynamic_rounding,
        standard_builtins,
        hosted,
        pthread,
        automatic_zero,
        tls_model,
        stack_protector,
        patchable_entry,
        minimum_function_alignment,
        codegen,
        language,
        gnu_inline: gnu_inline.unwrap_or(language.legacy_declarations()),
        short_wchar,
        // Plain char is signed on Darwin and unsigned on AArch64 Linux.
        signed_char: signed_char.unwrap_or(target.is_darwin()),
        // Darwin code is always position independent, and Apple's compiler
        // defines the PIC macros but not the PIE macros.
        position_independence: position_independence.unwrap_or(if target.is_darwin() {
            PositionIndependence::Pic(2)
        } else {
            PositionIndependence::Pie(2)
        }),
        survey,
        system_root,
        cc_mode: false,
        ignored_options,
        captured: None,
        opened_files: std::cell::RefCell::new(Vec::new()),
    })
}

fn parse_target(value: &str) -> Result<DriverTarget, String> {
    match value {
        SUPPORTED_TARGET => Ok(DriverTarget::Aarch64UnknownLinuxGnu),
        DARWIN_TARGET => Ok(DriverTarget::Aarch64AppleDarwin),
        _ => Err(format!(
            "unsupported target `{value}`; supported targets are `{SUPPORTED_TARGET}` and `{DARWIN_TARGET}`"
        )),
    }
}

fn set_target(slot: &mut Option<DriverTarget>, target: DriverTarget) -> Result<(), String> {
    if slot.replace(target).is_some() {
        return Err("--target may be specified only once".into());
    }
    Ok(())
}

fn set_mode(slot: &mut Option<DriverMode>, mode: DriverMode) -> Result<(), String> {
    if slot.replace(mode).is_some() {
        return Err(
            "exactly one of -E, --parse-only, --interpret-c-hir, --emit=evoir, --emit=asm, or --interpret-evoir is required"
                .into(),
        );
    }
    Ok(())
}

fn run_preprocess(options: &DriverOptions) -> ExitCode {
    let (src, tokens) = match read_and_lex(options) {
        Ok(value) => value,
        Err(code) => return code,
    };

    let mut pp = match configured_preprocessor(options) {
        Ok(pp) => pp,
        Err(message) => {
            options.emit_error(&options.input, 0, 0, "E0101", &message);
            return ExitCode::from(1);
        }
    };
    let result = pp.process_bytes(evocc_cpp::TokenStream::new(tokens), &src);
    options.record_opened_files(&pp);
    if pp.has_errors() {
        for error in pp.errors() {
            options.emit_span(
                pp.source_path(error.span.file)
                    .unwrap_or(options.input.as_str()),
                error.span,
                error.code,
                &error.message,
            );
        }
        return ExitCode::from(1);
    }

    let rendered = match render_preprocessed(&result.into_vec(), &src) {
        Ok(rendered) => rendered,
        Err(message) => {
            options.emit_error(&options.input, 0, 0, "E0003", &message);
            return ExitCode::from(1);
        }
    };
    if !options.write_output(format!("{rendered}\n").as_bytes()) {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn read_and_lex(options: &DriverOptions) -> Result<(Vec<u8>, Vec<evocc_lex::Token>), ExitCode> {
    let file = options.input.as_str();
    let language = options.language;
    let bytes = match fs::read(file) {
        Ok(s) => s,
        Err(e) => {
            options.emit_error(file, 0, 0, "E0003", &format!("cannot read source: {e}"));
            return Err(ExitCode::from(1));
        }
    };
    let src = bytes;
    options.source_bytes.replace(src.clone());

    let mut tokens = Vec::new();
    let mut lex = Lexer::from_bytes(FileId::new(1), &src);
    lex.set_legacy_literal_prefixes(language.legacy_literal_prefixes());
    while let Some(token) = lex.next() {
        if matches!(token.kind, TokenKind::Error | TokenKind::EncodingError) {
            options.emit_span(
                file,
                token.span,
                if token.kind == TokenKind::EncodingError {
                    "E0002"
                } else {
                    "E0001"
                },
                if token.kind == TokenKind::EncodingError {
                    "invalid encoding at this source position"
                } else {
                    "invalid token"
                },
            );
            return Err(ExitCode::from(1));
        }
        tokens.push(token);
    }
    Ok((src, tokens))
}

fn configured_preprocessor(options: &DriverOptions) -> Result<Preprocessor, String> {
    // The preprocessor crate has one target profile, AArch64 Linux. The
    // Darwin profile is that profile with the differences that Apple's
    // compiler predefines for arm64 macOS applied on top.
    let mut pp = Preprocessor::for_target(SUPPORTED_TARGET)?;
    if options.target.is_darwin() {
        darwin::apply_predefined_profile(&mut pp)?;
    }
    pp.current_file = options.input.clone();
    pp.set_standard_includes_enabled(options.standard_includes);
    let (date, time) = predefined_timestamp();
    pp.set_predefined_date(&date);
    pp.set_predefined_time(&time);
    for (name, enabled, value) in [
        ("__OPTIMIZE__", options.optimization != 0, "1"),
        ("__OPTIMIZE_SIZE__", options.optimization == 2, "1"),
        ("__NO_INLINE__", options.optimization == 0, "1"),
        ("__SSP_STRONG__", options.stack_protector, "3"),
        ("_REENTRANT", options.pthread, "1"),
        (
            "__ARM_FEATURE_PAC_DEFAULT",
            options.codegen.sign_returns,
            "1",
        ),
    ] {
        pp.undefine(name);
        if enabled {
            pp.define(name.to_owned(), decoded_tokens(value)?);
        }
    }
    // The sealed target macro census is a header-compatibility baseline, not
    // a claim that this compiler implements Annex F, complex FP or fast FMA.
    for name in [
        "__FP_FAST_FMA",
        "__FP_FAST_FMAF",
        "__FP_FAST_FMAF32",
        "__FP_FAST_FMAF32x",
        "__FP_FAST_FMAF64",
        "__STDC_IEC_559__",
        "__STDC_IEC_559_COMPLEX__",
        "__STDC_IEC_60559_BFP__",
        "__STDC_IEC_60559_COMPLEX__",
    ] {
        pp.undefine(name);
    }
    for name in [
        "__GCC_IEC_559",
        "__GCC_IEC_559_COMPLEX",
        "__FLT_EVAL_METHOD__",
        "__FLT_EVAL_METHOD_TS_18661_3__",
    ] {
        pp.define(name.to_owned(), decoded_tokens("0")?);
    }
    if options.codegen.general_registers_only {
        for name in [
            "__ARM_FEATURE_FMA",
            "__ARM_FEATURE_NUMERIC_MAXMIN",
            "__ARM_FP",
            "__ARM_FP16_ARGS",
            "__ARM_FP16_FORMAT_IEEE",
            "__ARM_NEON",
            "__ARM_NEON__",
        ] {
            pp.undefine(name);
        }
    }
    pp.define(
        "__STDC_HOSTED__".to_owned(),
        decoded_tokens(if options.hosted { "1" } else { "0" })?,
    );
    let (pic, pie) = options.position_independence.macro_levels();
    for (name, level) in [
        ("__PIC__", pic),
        ("__pic__", pic),
        ("__PIE__", pie),
        ("__pie__", pie),
    ] {
        pp.undefine(name);
        if level != 0 {
            pp.define(name.to_owned(), decoded_tokens(&level.to_string())?);
        }
    }
    let wchar_rank = options.target_layout().wide_character_rank();
    let wchar_type = evocc_chir::integer::integer_type_for_rank(wchar_rank);
    let wchar_bits = wchar_type.bits();
    let wchar_max = (1_u64 << wchar_bits) - 1;
    for (name, replacement) in [
        ("__ARM_SIZEOF_WCHAR_T", (wchar_bits / 8).to_string()),
        ("__SIZEOF_WCHAR_T__", (wchar_bits / 8).to_string()),
        ("__WCHAR_WIDTH__", wchar_bits.to_string()),
        ("__WCHAR_MIN__", "0U".to_owned()),
        ("__WCHAR_MAX__", format!("{wchar_max:#x}U")),
        (
            "__WCHAR_TYPE__",
            match wchar_rank {
                evocc_types::IntRank::UShort => "short unsigned int",
                evocc_types::IntRank::UInt => "unsigned int",
                _ => unreachable!("the selected target has unsigned 16-bit or 32-bit wchar_t"),
            }
            .to_owned(),
        ),
    ] {
        pp.define(name.to_owned(), decoded_tokens(&replacement)?);
    }
    pp.set_signed_char(options.signed_char);
    pp.set_legacy_literal_prefixes(options.language.legacy_literal_prefixes());
    if options.signed_char {
        pp.undefine("__CHAR_UNSIGNED__");
    } else if options.target.is_darwin() {
        pp.define("__CHAR_UNSIGNED__".to_owned(), decoded_tokens("1")?);
    }
    match options.language {
        LanguageMode::C89 | LanguageMode::Gnu89 => pp.undefine("__STDC_VERSION__"),
        LanguageMode::C99 | LanguageMode::Gnu99 => {
            pp.define("__STDC_VERSION__".to_owned(), decoded_tokens("199901L")?);
        }
        LanguageMode::C11 | LanguageMode::Gnu11 => {}
        LanguageMode::C17 | LanguageMode::Gnu17 => {
            pp.define("__STDC_VERSION__".to_owned(), decoded_tokens("201710L")?);
        }
    }
    if options.language.iso_keywords() {
        pp.define("__STRICT_ANSI__".to_owned(), decoded_tokens("1")?);
    }
    if options.gnu_inline {
        pp.undefine("__GNUC_STDC_INLINE__");
        pp.define("__GNUC_GNU_INLINE__".to_owned(), decoded_tokens("1")?);
    }
    if let Some(parent) = std::path::Path::new(&options.input).parent() {
        pp.add_include_path(parent.to_string_lossy().to_string());
    }
    // System directories follow every -I directory regardless of option
    // order. A directory in both lists keeps only its system position, so
    // include_next sees the same ordered search chain as include/has_include.
    let directory_identity = |path: &String| {
        std::fs::canonicalize(path).unwrap_or_else(|_| std::path::PathBuf::from(path))
    };
    let system_directories = options
        .system_include_paths
        .iter()
        .map(directory_identity)
        .collect::<std::collections::BTreeSet<_>>();
    for include_path in &options.include_paths {
        if !system_directories.contains(&directory_identity(include_path)) {
            pp.add_include_path(include_path.clone());
        }
    }
    for include_path in &options.system_include_paths {
        pp.add_include_path(include_path.clone());
    }
    // The target's own system directories follow every directory that the
    // command line names, as the platform compilers search them.
    if let Some(directories) = toolchain::target_system_directories(options) {
        pp.set_standard_includes_enabled(false);
        for directory in directories {
            pp.add_include_path(directory);
        }
    }
    for forced_include in &options.forced_includes {
        pp.add_forced_include(forced_include.clone());
    }
    for (old, new) in &options.macro_prefix_maps {
        pp.add_macro_prefix_map(old.clone(), new.clone());
    }
    for action in &options.macro_actions {
        match action {
            MacroAction::Define(definition) => {
                let (name, replacement) = definition
                    .split_once('=')
                    .map_or((definition.as_str(), "1"), |(name, replacement)| {
                        (name, replacement)
                    });
                if !is_identifier(name) {
                    return Err(format!("invalid macro name in -D: `{name}`"));
                }
                pp.define(
                    name.to_owned(),
                    decoded_tokens_in_mode(replacement, options.language)?,
                );
            }
            MacroAction::Undefine(name) => {
                if !is_identifier(name) {
                    return Err(format!("invalid macro name in -U: `{name}`"));
                }
                pp.undefine(name);
            }
        }
    }
    Ok(pp)
}

fn decoded_tokens(source: &str) -> Result<Vec<evocc_lex::Token>, String> {
    decoded_tokens_in_mode(source, LanguageMode::Gnu11)
}

fn decoded_tokens_in_mode(
    source: &str,
    language: LanguageMode,
) -> Result<Vec<evocc_lex::Token>, String> {
    let mut tokens = Vec::new();
    let mut lexer = Lexer::new(FileId::DUMMY, source);
    lexer.set_legacy_literal_prefixes(language.legacy_literal_prefixes());
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Eof {
            break;
        }
        if token.kind == TokenKind::Error {
            return Err("invalid token in macro replacement".into());
        }
        let text = match &token.text {
            evocc_lex::TokenText::Decoded(text) => text.clone(),
            evocc_lex::TokenText::RawBytes { .. } => {
                return Err("invalid macro option encoding".into())
            }
            evocc_lex::TokenText::Borrowed => source
                .get(token.span.byte_start as usize..token.span.byte_end as usize)
                .ok_or_else(|| "invalid macro replacement span".to_string())?
                .to_owned(),
        };
        tokens.push(evocc_lex::Token {
            text: evocc_lex::TokenText::Decoded(text),
            ..token
        });
    }
    Ok(tokens)
}

fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn render_preprocessed(tokens: &[evocc_lex::Token], source: &[u8]) -> Result<String, String> {
    let mut rendered = String::new();
    let mut unseparated: Vec<(TokenKind, String)> = Vec::new();
    for token in tokens.iter().filter(|token| token.kind != TokenKind::Eof) {
        // Semantic directives cross the internal token boundary as markers,
        // but -E must emit their source form for the next parser invocation.
        if let Some(enabled) = evocc_cpp::pragma_fenv_access(token) {
            if !rendered.is_empty() && !rendered.ends_with('\n') {
                rendered.push('\n');
            }
            rendered.push_str(if enabled {
                "#pragma STDC FENV_ACCESS ON\n"
            } else {
                "#pragma STDC FENV_ACCESS OFF\n"
            });
            unseparated.clear();
            continue;
        }
        if let Some(action) = pragma_pack_action(token) {
            if !rendered.is_empty() && !rendered.ends_with('\n') {
                rendered.push('\n');
            }
            match action {
                PragmaPackAction::Set(alignment) => {
                    rendered.push_str(&format!("#pragma pack({alignment})\n"));
                }
                PragmaPackAction::Reset => rendered.push_str("#pragma pack()\n"),
                PragmaPackAction::Push(None) => rendered.push_str("#pragma pack(push)\n"),
                PragmaPackAction::Push(Some(alignment)) => {
                    rendered.push_str(&format!("#pragma pack(push, {alignment})\n"));
                }
                PragmaPackAction::Pop => rendered.push_str("#pragma pack(pop)\n"),
            }
            unseparated.clear();
            continue;
        }
        if is_arm_neon_header_marker(token) {
            if !rendered.is_empty() && !rendered.ends_with('\n') {
                rendered.push('\n');
            }
            rendered.push_str("#pragma GCC aarch64 \"arm_neon.h\"\n");
            unseparated.clear();
            continue;
        }
        if let Some(action) = pragma_visibility_action(token) {
            if !rendered.is_empty() && !rendered.ends_with('\n') {
                rendered.push('\n');
            }
            match action {
                PragmaVisibilityAction::Push { visibility } => {
                    rendered.push_str(&format!("#pragma GCC visibility push({visibility})\n"));
                }
                PragmaVisibilityAction::Pop => rendered.push_str("#pragma GCC visibility pop\n"),
            }
            unseparated.clear();
            continue;
        }
        let text = match &token.text {
            evocc_lex::TokenText::Decoded(text) => text.clone(),
            evocc_lex::TokenText::RawBytes { escaped, .. } => escaped.clone(),
            evocc_lex::TokenText::Borrowed => std::str::from_utf8(
                source
                    .get(token.span.byte_start as usize..token.span.byte_end as usize)
                    .ok_or_else(|| "token span does not belong to the active source".to_string())?,
            )
            .map_err(|_| "legacy token has invalid encoding".to_string())?
            .to_owned(),
        };
        if text.is_empty() {
            continue;
        }
        if !unseparated.is_empty() && !can_concatenate(&unseparated, token.kind, &text) {
            rendered.push(' ');
            unseparated.clear();
        }
        rendered.push_str(&text);
        unseparated.push((token.kind, text));
    }
    Ok(rendered)
}

fn can_concatenate(group: &[(TokenKind, String)], kind: TokenKind, text: &str) -> bool {
    let candidate = group
        .iter()
        .map(|(_, spelling)| spelling.as_str())
        .chain(core::iter::once(text))
        .collect::<String>();
    let expected = group
        .iter()
        .map(|(expected_kind, _)| *expected_kind)
        .chain(core::iter::once(kind))
        .collect::<Vec<_>>();
    let mut lexer = Lexer::new(FileId::DUMMY, &candidate);
    let mut actual = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind != TokenKind::Eof {
            actual.push(token.kind);
        }
    }
    actual == expected
}

fn run_parsed(options: &DriverOptions, action: ParsedAction) -> ExitCode {
    let (src, tokens) = match read_and_lex(options) {
        Ok(value) => value,
        Err(code) => return code,
    };

    let mut pp = match configured_preprocessor(options) {
        Ok(pp) => pp,
        Err(message) => {
            options.emit_error(&options.input, 0, 0, "E0101", &message);
            return ExitCode::from(1);
        }
    };
    let result = pp.process_bytes(evocc_cpp::TokenStream::new(tokens), &src);
    options.record_opened_files(&pp);
    if pp.has_errors() {
        for error in pp.errors() {
            options.emit_span(
                pp.source_path(error.span.file)
                    .unwrap_or(options.input.as_str()),
                error.span,
                error.code,
                &error.message,
            );
        }
        return ExitCode::from(1);
    }
    let processed = result.into_vec();

    let arena = Arena::default();
    let mut parser = Parser::from_tokens(processed, FileId::new(1));
    parser.set_target_layout(options.target_layout());
    parser.set_standard_builtins(options.standard_builtins);
    parser.set_legacy_declarations(options.language.legacy_declarations());
    parser.use_language_keywords(
        options.language.iso_keywords(),
        options.language.legacy_declarations(),
    );
    parser.set_gnu_inline(options.gnu_inline);
    if !options.survey {
        parser.set_error_limit(NonZeroUsize::MIN);
    }
    let mut unit = parser.parse_translation_unit_in(&arena);
    unit.set_signed_wrapping(options.signed_wrapping);
    unit.set_dynamic_rounding(options.dynamic_rounding);
    unit.set_automatic_zero(options.automatic_zero);
    unit.set_tls_model(options.tls_model);
    unit.set_stack_protector(options.stack_protector);
    unit.set_minimum_function_alignment(options.minimum_function_alignment);
    if parser.had_error() {
        // A build reports the first diagnostic only. The error limit bounds the
        // declaration loop, not the reports made while parsing one
        // declaration, so the first must be selected here rather than assumed.
        let all = parser.diagnostics();
        let reported = if options.survey {
            all
        } else {
            &all[..all.len().min(1)]
        };
        if !reported.is_empty() {
            for diagnostic in reported {
                options.emit_span(
                    pp.source_path(diagnostic.span.file)
                        .unwrap_or(options.input.as_str()),
                    diagnostic.span,
                    diagnostic.code,
                    &diagnostic.message,
                );
            }
        } else {
            options.emit_error(&options.input, 0, 0, "E1001", "expected-declaration");
        }
        return ExitCode::from(1);
    }

    match action {
        ParsedAction::Validate => ExitCode::SUCCESS,
        ParsedAction::InterpretCHir => interpret_chir(options, &pp, &unit),
        ParsedAction::EmitCHir => emit_chir(&unit),
        ParsedAction::EmitCoreCHir => {
            let entries = options
                .core_entries
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            match core_export::export_core(&unit, &options.target_layout(), &entries) {
                Ok(text) => {
                    print!("{text}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    options.error_format.usage(&format!(
                        "core-v1 source export refused: {:?}; no compilation proof was attempted",
                        error.reason
                    ));
                    ExitCode::from(1)
                }
            }
        }
        ParsedAction::EmitEvoIr | ParsedAction::EmitAsm | ParsedAction::InterpretEvoIr => {
            let target = options.target_layout();
            let consume = |resolved: &evocc_chir::CHirUnit<'_>,
                           mut module: Module,
                           locations: evocc_lower::SourceLocations| {
                let original = matches!(action, ParsedAction::InterpretEvoIr)
                    .then(|| module.functions.clone());
                if options.target.is_darwin() {
                    darwin::apply_assembler_labels(resolved, &mut module);
                }
                for function in &mut module.functions {
                    if function.patchable_entry.is_none() {
                        function.patchable_entry = options.patchable_entry;
                    }
                }
                // Validated passes run before the independently certified
                // EqSat chain. The default path performs no TV executions.
                let module = match apply_development_passes(module, options) {
                    Ok(module) => module,
                    Err(detail) => {
                        options
                            .error_format
                            .usage(&format!("translation validation failed: {detail}"));
                        return ExitCode::from(1);
                    }
                };
                let (module, rewrites) = if options.optimize_scalar_evoir {
                    match optimize_scalar_evoir(module, options) {
                        Ok((module, rewrites)) => (module, Some(rewrites)),
                        Err(detail) => {
                            options
                                .error_format
                                .usage(&format!("verified optimization failed: {detail}"));
                            return ExitCode::from(1);
                        }
                    }
                } else {
                    if let Some(path) = &options.eq_proof_dir {
                        if let Err(detail) = evocc_eqopt::write_identity_chain(&module, path) {
                            options
                                .error_format
                                .usage(&format!("identity-chain publication failed: {detail}"));
                            return ExitCode::from(1);
                        }
                    }
                    (module, None)
                };
                match action {
                    ParsedAction::EmitEvoIr => emit_evoir(&module),
                    ParsedAction::EmitAsm => match &options.proof {
                        Some(proof) => proof_required::run(
                            options,
                            proof,
                            &src,
                            &unit,
                            &module,
                            rewrites.as_ref(),
                            resolved,
                        ),
                        None => emit_asm(options, &module, resolved),
                    },
                    ParsedAction::InterpretEvoIr => interpret_evoir(
                        options,
                        &module,
                        &pp,
                        &locations,
                        original.as_deref().unwrap_or_default(),
                    ),
                    _ => unreachable!("matched EvoIR action"),
                }
            };
            let lowered = if matches!(action, ParsedAction::InterpretEvoIr) {
                evocc_lower::lower_unit_with_locations(
                    &unit,
                    &target,
                    options.optimization != 0,
                    consume,
                )
            } else {
                evocc_lower::lower_unit_with_source(
                    &unit,
                    &target,
                    options.optimization != 0,
                    |source, module| {
                        consume(source, module, evocc_lower::SourceLocations::default())
                    },
                )
            };
            match lowered {
                Ok(result) => result,
                Err(error) => {
                    let span = error.span();
                    options.emit_error(
                        span.and_then(|span| pp.source_path(span.file))
                            .unwrap_or(options.input.as_str()),
                        span.map_or(0, |span| span.line_start.max(1)),
                        span.map_or(0, |span| span.column_start.max(1)),
                        error.code(),
                        &error.message(),
                    );
                    ExitCode::from(1)
                }
            }
        }
    }
}

fn apply_development_passes(module: Module, options: &DriverOptions) -> Result<Module, String> {
    if options.opt_passes.is_empty() && options.pass_report.is_none() {
        return Ok(module);
    }
    let identity = evocc_passes::IdentityPass;
    let passes = options
        .opt_passes
        .iter()
        .map(|_| &identity as &dyn evocc_passes::ModulePass)
        .collect::<Vec<_>>();
    let inputs = if passes.is_empty() {
        Vec::new()
    } else {
        let (arguments, _) = evoir_main_contract(&module)?;
        vec![evocc_passes::ExecutionInput::main(
            "main", arguments, 100_000,
        )]
    };
    let output =
        evocc_passes::run_pipeline(&module, &passes, &inputs).map_err(|error| error.to_string())?;
    if let Some(path) = &options.pass_report {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("cannot create pass report {}: {error}", path.display()))?;
        file.write_all(output.report.render_json().as_bytes())
            .map_err(|error| format!("cannot write pass report {}: {error}", path.display()))?;
    }
    Ok(output.module)
}

/// The result is the optimized module and the rewrite chain that gives it.
fn optimize_scalar_evoir(
    module: evocc_evoir::Module,
    options: &DriverOptions,
) -> Result<(evocc_evoir::Module, proof_required::Rewrites), String> {
    let checker = match &options.eq_checker {
        Some(binary) => {
            evocc_proof::StandaloneProofChecker::new(binary.clone(), std::env::temp_dir())
        }
        None => evocc_proof::StandaloneProofChecker::locate_named("evocc-checker")?,
    }
    .with_timeout(std::time::Duration::from_millis(options.eq_timeout_ms))
    .with_expected_digest(options.eq_checker_digest.clone()
        .or_else(|| std::env::var("EVOCC_CHECKER_SHA256").ok())
        .ok_or("-Oeq-scalar requires --eq-checker-sha256 or EVOCC_CHECKER_SHA256 from a trusted build/release")?);
    let mut equality = EqualityState::for_module(&module)?;
    match run_scalar_chain_with_checker(&module, &mut equality, &options.eq_policy, &checker) {
        Ok(run) => {
            if let Some(path) = &options.eq_proof_dir {
                run.write_bundle(path)?;
            }
            let rewrites = proof_required::Rewrites {
                initial: module,
                steps: run.transactions.into_iter().map(|t| t.bundle).collect(),
                checker: checker.binary().clone(),
                checker_sha256: run.checker_digest.unwrap_or_default(),
            };
            Ok((run.module, rewrites))
        }
        Err(failure) => {
            if let Some(path) = &options.eq_proof_dir {
                if let Err(publication) = failure.write_bundle(path) {
                    return Err(format!("{}; additionally, {publication}", failure.detail));
                }
            }
            Err(failure.detail)
        }
    }
}

fn interpret_chir(
    options: &DriverOptions,
    pp: &Preprocessor,
    unit: &evocc_chir::CHirUnit<'_>,
) -> ExitCode {
    let Some(entry) = unit.iter().find_map(|declaration| match declaration {
        CHirDecl::Function(function)
            if function.body.is_some() && function.name.spelling == "main" =>
        {
            Some(function.symbol)
        }
        _ => None,
    }) else {
        options.emit_execution_error(
            &options.input,
            None,
            "E3007",
            "interpreter-entry-function-not-found: main",
        );
        return ExitCode::from(1);
    };

    let outcome = match evocc_chir::interp::interpret_program(
        unit,
        entry,
        &options.target_layout(),
        InterpreterLimits::default(),
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            let span = error.span();
            options.emit_execution_error(
                span.and_then(|span| pp.source_path(span.file))
                    .unwrap_or(options.input.as_str()),
                span.map(|span| (span.line_start.max(1), span.column_start.max(1))),
                error.code(),
                &error.message(),
            );
            return ExitCode::from(1);
        }
    };
    let Some(value) = outcome.return_value else {
        options.emit_execution_error(
            &options.input,
            None,
            "E3007",
            "interpreter-main-returned-no-value",
        );
        return ExitCode::from(1);
    };
    let mathematical = match value.mathematical_value(unit.types()) {
        Ok(value) => value.to_string(),
        Err(evocc_chir::interp::ScalarValueError::OutsideSignedCarrier) => {
            value.raw_bits.to_string()
        }
        Err(_) => {
            options.emit_execution_error(
                &options.input,
                None,
                "E3007",
                "interpreter-main-return-type-is-not-scalar-integer",
            );
            return ExitCode::from(1);
        }
    };
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{mathematical}").is_err() {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// `--emit=c-hir`: the declaration and attribute summary of Section 10.1.9.
fn emit_chir(unit: &evocc_chir::CHirUnit<'_>) -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    if write!(stdout, "{}", evocc_chir::render_unit_summary(unit)).is_err() {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn emit_evoir(module: &Module) -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    if write!(stdout, "{}", render_module(module)).is_err() {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// Emit the module as target assembly, or report why the backend stopped.
///
/// A refusal is a named diagnostic in the same shape as every other stage, so
/// a survey over the kernel can count what stopped each translation unit.
/// Lowering refusals carry their own EvoMIR codes; the remaining stops are
/// driver-level reports of an internal stage that declined.
fn emit_asm(options: &DriverOptions, module: &Module, unit: &evocc_chir::CHirUnit<'_>) -> ExitCode {
    let text = match assemble(options, module, unit) {
        Ok(text) => text,
        Err(code) => return code,
    };
    if !options.write_output(text.as_bytes()) {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// The assembly text of `--emit=asm`. An error is already reported.
fn assemble(
    options: &DriverOptions,
    module: &Module,
    unit: &evocc_chir::CHirUnit<'_>,
) -> Result<String, ExitCode> {
    let (target, relocation) = match options.target {
        DriverTarget::Aarch64UnknownLinuxGnu => {
            (MirTarget::Linux, options.position_independence.relocation())
        }
        // Mach-O on arm64 admits only position-independent code.
        DriverTarget::Aarch64AppleDarwin => {
            (MirTarget::Darwin, RelocationModel::PositionIndependent)
        }
    };
    let mut text = match compile_module_with_options(
        target,
        module,
        &|_| true,
        relocation,
        &options.codegen,
    ) {
        Ok(text) => text,
        Err(stop) => {
            match &stop {
                NativeStop::Lowering(codes) => {
                    for code in codes {
                        options.emit_error(&options.input, 0, 0, code, "evomir-lowering-refused");
                    }
                }
                other => options.emit_error(
                    &options.input,
                    0,
                    0,
                    "E0004",
                    &format!(
                        "native-emission-stopped: {}: {}",
                        other.component(),
                        other.detail()
                    ),
                ),
            }
            return Err(ExitCode::from(1));
        }
    };
    // The debug type writer emits ELF sections. A Darwin object has no debug
    // information.
    if options.debug_types && !options.target.is_darwin() {
        match dwarf::type_information(unit, module, &options.target_layout(), &options.input) {
            Ok(debug) => text.push_str(&debug),
            Err(error) => {
                options.emit_error(
                    &options.input,
                    0,
                    0,
                    "E0004",
                    &format!("debug-type-emission: {error}"),
                );
                return Err(ExitCode::from(1));
            }
        }
    }
    Ok(text)
}

fn interpret_evoir(
    options: &DriverOptions,
    module: &Module,
    pp: &Preprocessor,
    locations: &evocc_lower::SourceLocations,
    original: &[evocc_evoir::Function],
) -> ExitCode {
    let (arguments, declared_results) = match evoir_main_contract(module) {
        Ok(contract) => contract,
        Err(message) => {
            options.emit_execution_error(&options.input, None, "E3007", message);
            return ExitCode::from(1);
        }
    };
    let mut site = None;
    let outcome = match evocc_evoir::interpret_program_with_site(
        module,
        &arguments,
        EvoIrLimits::new(100_000),
        &mut site,
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            let origin = site.and_then(|site| {
                let function = module.functions.get(site.function)?;
                let original = original
                    .iter()
                    .find(|before| before.name == function.name)?;
                let block = function
                    .blocks
                    .iter()
                    .find(|block| block.id == site.block)?;
                let instruction = block
                    .insts
                    .get(site.instruction)
                    .unwrap_or(&block.terminator);
                // Do not use value IDs from a function rewritten after lowering.
                let precise = (function == original)
                    .then(|| locations.instruction(&original.name, instruction))
                    .flatten();
                precise
                    .or_else(|| locations.declaration(&original.name))
                    .map(|span| (span, precise.is_some()))
            });
            if let Some((span, precise)) = origin {
                let file = pp.source_path(span.file).unwrap_or(&options.input);
                let bytes = options.diagnostic_source(file);
                let mut diagnostic = diagnostics::Diagnostic::new(
                    error.code(),
                    &error.message(),
                    diagnostics::Span::source(file, &bytes, span, &error.message()),
                );
                diagnostic.notes.push(if precise { "this operation failed on the executed path" } else { "the executing function is shown; this instruction has no retained expression position" }.into());
                if matches!(error.code(), "E3002" | "E3003") {
                    diagnostic.help.push(
                        "check the divisor before the division or remainder operation".into(),
                    );
                }
                eprint!(
                    "{}",
                    diagnostics::render(&diagnostic, options.error_format, options.color)
                );
            } else {
                options.emit_execution_error(&options.input, None, error.code(), &error.message());
            }
            return ExitCode::from(1);
        }
    };
    if outcome.return_values.len() != declared_results.len()
        || outcome
            .return_values
            .iter()
            .zip(&declared_results)
            .any(|(value, declared)| &value.ty != declared)
    {
        options.emit_execution_error(
            &options.input,
            None,
            "E3007",
            "evoir-main-result-does-not-match-declaration",
        );
        return ExitCode::from(1);
    }
    let Some(value) = outcome.return_values.first() else {
        options.emit_execution_error(
            &options.input,
            None,
            "E3007",
            "evoir-main-returned-no-value",
        );
        return ExitCode::from(1);
    };
    let mathematical = match signed_scalar(value) {
        Ok(value) => value,
        Err(message) => {
            options.emit_execution_error(&options.input, None, "E3007", message);
            return ExitCode::from(1);
        }
    };
    let mut stdout = std::io::stdout().lock();
    let output = if options.mode == DriverMode::Check {
        options.error_format.completion(mathematical)
    } else {
        format!("{mathematical}\n")
    };
    if stdout.write_all(output.as_bytes()).is_err() {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn evoir_main_contract(
    module: &Module,
) -> Result<(Vec<RuntimeValue>, Vec<EvoIRType>), &'static str> {
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .ok_or("evoir-entry-function-not-found: main")?;
    let arguments = match main.params.as_slice() {
        [] => Vec::new(),
        [memory, world]
            if memory.ty == EvoIRType::Mem(Region::All) && world.ty == EvoIRType::World =>
        {
            vec![
                RuntimeValue::new(memory.ty.clone(), 0),
                RuntimeValue::new(world.ty.clone(), 0),
            ]
        }
        _ => return Err("evoir-main-parameters-are-not-canonical"),
    };
    let has_effect_results = match main.ret_tys.as_slice() {
        [EvoIRType::Bits(1..=64)] => false,
        [EvoIRType::Bits(1..=64), EvoIRType::Mem(Region::All), EvoIRType::World] => true,
        _ => return Err("evoir-main-results-are-not-canonical"),
    };
    if has_effect_results != !arguments.is_empty() {
        return Err("evoir-main-effect-interface-is-inconsistent");
    }
    Ok((arguments, main.ret_tys.clone()))
}

fn signed_scalar(value: &RuntimeValue) -> Result<i128, &'static str> {
    let EvoIRType::Bits(width @ 1..=64) = value.ty else {
        return Err("evoir-main-return-type-is-not-scalar-integer");
    };
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    let raw = value.raw_bits & mask;
    let sign_bit = 1_u64 << (width - 1);
    Ok(if raw & sign_bit == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    })
}

fn predefined_timestamp() -> (String, String) {
    let seconds = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0)
        });
    let day_seconds = seconds.rem_euclid(86_400);
    (
        c_date_from_days(seconds.div_euclid(86_400)),
        format!(
            "{:02}:{:02}:{:02}",
            day_seconds / 3600,
            day_seconds / 60 % 60,
            day_seconds % 60
        ),
    )
}

fn c_date_from_days(days_since_epoch: i64) -> String {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    let month_name = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(month - 1) as usize];
    format!("{month_name} {day:>2} {year:04}")
}
