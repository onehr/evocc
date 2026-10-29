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
use std::io::{Read, Write};
use std::num::NonZeroUsize;
use std::process::ExitCode;

use evocc_chir::{interp::InterpreterLimits, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_cpp::{
    is_arm_neon_header_marker, pragma_pack_action, pragma_visibility_action, PragmaPackAction,
    PragmaVisibilityAction, Preprocessor,
};
use evocc_eqopt::{run_scalar_chain_with_checker, EqualityState, ExtractionPolicy};
use evocc_evoir::{InterpreterLimits as EvoIrLimits, Module, RuntimeValue};
use evocc_lex::{Lexer, TokenKind};
use evocc_mir::emit::{CodegenOptions, StackGuard};
use evocc_mir::{compile_module_with_entries, NativeStop, RelocationModel, Target as MirTarget};
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

mod cc;
mod core_export;
mod darwin;
mod deps;
mod diagnostics;
mod dwarf;
mod lints;
mod options;
mod proof_required;
mod repairs;
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
    lints: lints::Levels,
    repair_mode: Option<String>,
    error_format: diagnostics::Format,
    color: diagnostics::Color,
    source_bytes: std::cell::RefCell<Vec<u8>>,
    included_bytes: std::cell::RefCell<std::collections::BTreeMap<String, Vec<u8>>>,
    target: DriverTarget,
    input: String,
    source_from_stdin: bool,
    /// `-I` and `-F` directories in command-line order.
    include_paths: Vec<SearchPath>,
    /// `-isystem` and `-iframework` directories in command-line order.
    system_include_paths: Vec<SearchPath>,
    forced_includes: Vec<String>,
    macro_prefix_maps: Vec<(String, String)>,
    macro_actions: Vec<MacroAction>,
    standard_includes: bool,
    core_entries: Vec<String>,
    defined_functions: bool,
    program_arguments: Option<Vec<Vec<u8>>>,
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
    /// The last of `-ftrapping-math` (the default) and `-fno-trapping-math`.
    trapping_math: bool,
    standard_builtins: bool,
    /// `-fno-builtin-NAME`: the names stay disabled after `-fbuiltin`.
    disabled_builtins: std::collections::BTreeSet<String>,
    /// False under `-fno-inline`: only `always_inline` callees are substituted.
    optional_inlining: bool,
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
    /// The macOS deployment target on `aarch64-apple-darwin`, `None` on
    /// Linux. It gives the availability attributes their meaning.
    darwin_deployment: Option<evocc_cpp::availability::Version>,
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
        if line != 0 && column != 0 {
            let bytes = self.diagnostic_source(file);
            let start = bytes
                .split_inclusive(|b| *b == b'\n')
                .take(line as usize - 1)
                .map(<[u8]>::len)
                .sum::<usize>()
                + column as usize
                - 1;
            let end = (start + 1).min(bytes.len()).max(start);
            self.emit_span(
                file,
                evocc_common::SourceSpan::new(
                    FileId::DUMMY,
                    start as u32,
                    end as u32,
                    line,
                    column,
                ),
                code,
                message,
            );
        } else {
            let mut diagnostic =
                diagnostics::Diagnostic::argument(code, message, (code == "E0003").then_some(file));
            diagnostic.notes.push(format!("input file: {file}"));
            eprint!(
                "{}",
                diagnostics::render(&diagnostic, self.error_format, self.color)
            );
        }
    }

    fn emit_execution_error(
        &self,
        file: &str,
        location: Option<(u32, u32)>,
        code: &str,
        message: &str,
    ) {
        let (line, column) = location.unwrap_or((0, 0));
        self.emit_error(file, line, column, code, message);
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

/// One directory of the include search. A framework directory (`-F`,
/// `-iframework`) serves `<Name/Header.h>` from `Name.framework`, as Apple
/// clang searches it (EvoIR 10.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchPath {
    pub path: String,
    pub framework: bool,
}

impl SearchPath {
    pub(crate) fn normal(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            framework: false,
        }
    }

    pub(crate) fn framework(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            framework: true,
        }
    }
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
    diagnostics::Format::Text.usage(&format!("capability `{capability}` is NOT_IMPLEMENTED"));
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
        eprintln!(
            "       evocc --emit=core-c-hir --core-entry=NAME -O0 -fno-builtin <file> (source input only, not a proof)"
        );
        eprintln!("       evocc --emit=asm [--target={SUPPORTED_TARGET}] <file>");
        eprintln!(
            "       evocc --emit=asm --proof-required=NEW_DIR --core-entry=NAME -O0 -fno-builtin [--proof-checker=PATH --proof-checker-sha256=SHA256 --proof-timeout-ms=N --proof-proposal=NAME=PATH] [-Oeq-scalar --eq-checker-sha256=SHA256 ...] <file>"
        );
        eprintln!("       evocc --interpret-evoir [--target={SUPPORTED_TARGET}] <file>");
        eprintln!(
            "       evocc --check [--error-format=text|short|json|concise] [--color=auto|always|never] <file>"
        );
        eprintln!("       evocc --explain E3xxx");
        eprintln!("       -Oeq-scalar [--eq-policy=NAME | --eq-weights=VECTOR]");
        eprintln!("       --ub-checks=checked|unchecked (native only; default checked)");
        eprintln!(
            "       --frame-opt=paired-saves,shared-epilogue,leaf-frames (native only; default none)"
        );
        eprintln!(
            "       --isel-opt=constants,local,extensions,controls (native only; default none)"
        );
        eprintln!("       --regalloc-opt=copy-hints,reload-reuse (native only; default none)");
        eprintln!("       --opt-pass=identity [--pass-report NEW_JSON_PATH] (development TV)");
        eprintln!("       [--eq-max-nodes=N --eq-max-epochs=N --eq-max-transactions=N]");
        eprintln!(
            "       --eq-checker-sha256=TRUSTED_SHA256 [--eq-checker=PATH --eq-checker-timeout-ms=N --eq-proof-dir=NEW_DIR]"
        );
        error_format.usage("unrecognized invocation is NOT_IMPLEMENTED");
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
        error_format.usage(&format!(
            "unrecognized probe invocation is NOT_IMPLEMENTED: {probe_args:?}"
        ));
        return ExitCode::from(NOT_IMPLEMENTED_EXIT);
    }

    if let Some(option) = args.iter().find(|arg| {
        matches!(
            arg.as_str(),
            "-pedantic-errors" | "--pedantic-errors" | "-Werror=pedantic"
        )
    }) {
        let mut diagnostic = diagnostics::Diagnostic::argument(
            "E2968",
            "unsupported-pedantic-errors: EvoCC has no ISO pedantic diagnostics",
            Some(option),
        );
        diagnostic
            .help
            .push("use -pedantic or -Wpedantic for the supported warning policy".into());
        eprint!(
            "{}",
            diagnostics::render(&diagnostic, error_format, diagnostics::Color::Auto)
        );
        return ExitCode::from(1);
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
            "--parse-only"
                | "--list-defined-functions"
                | "--interpret-c-hir"
                | "--interpret-evoir"
                | "--check"
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
    options::parse(args, lenient)
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

/// Emit `#warning` records at the selected cpp lint level. Returns true when
/// one is denied, so the caller publishes no output.
fn emit_cpp_warnings(options: &DriverOptions, pp: &evocc_cpp::Preprocessor) -> bool {
    let mut denied = false;
    for warning in pp.warnings() {
        let level = options
            .lints
            .level(9, options.language.legacy_declarations());
        if level == lints::Level::Allow {
            continue;
        }
        let file = pp.source_path(warning.span.file).unwrap_or(&options.input);
        let bytes = options.diagnostic_source(file);
        let mut diagnostic = diagnostics::Diagnostic::new(
            warning.code,
            &warning.message,
            diagnostics::Span::source(file, &bytes, warning.span, &warning.message),
        );
        diagnostic.level = if level == lints::Level::Deny {
            "error"
        } else {
            "warning"
        };
        eprint!(
            "{}",
            diagnostics::render(&diagnostic, options.error_format, options.color)
        );
        denied |= level == lints::Level::Deny;
    }
    denied
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
    let cpp_denied = emit_cpp_warnings(options, &pp);
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
    if cpp_denied {
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
    // Keep the physical filename for includes and diagnostics; only the main
    // source bytes come from the editor snapshot.
    let read = if options.source_from_stdin {
        let mut bytes = Vec::new();
        std::io::stdin()
            .lock()
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    } else {
        fs::read(file)
    };
    let bytes = match read {
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
        darwin::apply_predefined_profile(
            &mut pp,
            options
                .darwin_deployment
                .ok_or("the Darwin target has a deployment target")?,
        )?;
    }
    // Keep GNU header compatibility while exposing the actual compiler identity.
    pp.define("__EVOCC__".into(), decoded_tokens("1")?);
    pp.define(
        "__EVOCC_VERSION__".into(),
        decoded_tokens(concat!("\"", env!("CARGO_PKG_VERSION"), "\""))?,
    );
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
        // GCC 14 defines it after a final `-fno-trapping-math`; Apple clang
        // defines no such macro.
        (
            "__NO_TRAPPING_MATH__",
            !options.trapping_math && !options.target.is_darwin(),
            "1",
        ),
        // ACLE: bit 0 is the A key, bit 2 protects leaf functions.
        (
            "__ARM_FEATURE_PAC_DEFAULT",
            options.codegen.sign_returns,
            if options.codegen.sign_leaf_returns {
                "5"
            } else {
                "1"
            },
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
    for (name, replacement) in wide_character_macros(options) {
        match replacement {
            Some(replacement) => pp.define(name.to_owned(), decoded_tokens(&replacement)?),
            None => pp.undefine(name),
        }
    }
    pp.set_signed_char(options.signed_char);
    let wide_character =
        evocc_chir::integer::integer_type_for_rank(options.target_layout().wide_character_rank());
    pp.set_wide_character(
        wide_character.bits(),
        !wide_character.is_signed(),
        options.short_wchar && options.target.is_darwin(),
    );
    pp.set_target_follows_apple_clang(options.target.is_darwin());
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
    // Quoted includes search the active file's parent in the preprocessor.
    // Do not add it to the angle search chain ahead of explicit -I roots.
    // System directories follow every -I and -F directory regardless of
    // option order. A directory in both lists keeps only its system
    // position, so include_next sees the same ordered search chain as
    // include/has_include.
    let directory_identity = |directory: &SearchPath| {
        (
            std::fs::canonicalize(&directory.path)
                .unwrap_or_else(|_| std::path::PathBuf::from(&directory.path)),
            directory.framework,
        )
    };
    let add = |pp: &mut Preprocessor, directory: &SearchPath, system: bool| {
        if system && directory.framework {
            pp.add_system_framework_path(directory.path.clone());
        } else if system {
            pp.add_system_include_path(directory.path.clone());
        } else if directory.framework {
            pp.add_framework_path(directory.path.clone());
        } else {
            pp.add_include_path(directory.path.clone());
        }
    };
    let target_directories = toolchain::target_system_directories(options);
    let system_directories = options
        .system_include_paths
        .iter()
        .chain(target_directories.iter().flat_map(|target| &target.user))
        .chain(target_directories.iter().flat_map(|target| &target.system))
        .map(directory_identity)
        .collect::<std::collections::BTreeSet<_>>();
    // Apple clang searches the user directories of the target, such as
    // `/usr/local/include`, after the command line's -I and -F directories
    // and before its -isystem and -iframework directories.
    for directory in &options.include_paths {
        if !system_directories.contains(&directory_identity(directory)) {
            add(&mut pp, directory, false);
        }
    }
    let explicit_system_directories = options
        .system_include_paths
        .iter()
        .map(directory_identity)
        .collect::<std::collections::BTreeSet<_>>();
    for directory in target_directories.iter().flat_map(|target| &target.user) {
        if !explicit_system_directories.contains(&directory_identity(directory)) {
            add(&mut pp, directory, true);
        }
    }
    for directory in &options.system_include_paths {
        add(&mut pp, directory, true);
    }
    // The target's own system directories follow every directory that the
    // command line names, as the platform compilers search them.
    if let Some(target) = &target_directories {
        pp.set_standard_includes_enabled(false);
        for directory in &target.system {
            if !explicit_system_directories.contains(&directory_identity(directory)) {
                add(&mut pp, directory, true);
            }
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
                let body = decoded_tokens_in_mode(replacement, options.language)?;
                if pp.macros().get(name).is_some_and(|previous| {
                    previous.is_function || !same_macro_replacement(&previous.body, &body)
                }) {
                    // Kconfig uses conflicting predefined macros under -Werror
                    // as capability probes. Accept the replacement only under
                    // the same global warning policy as other driver warnings.
                    let message = format!("macro `{name}` redefined");
                    match options.lints.driver_warning() {
                        lints::Level::Deny => return Err(message),
                        lints::Level::Warn => {
                            let mut diagnostic = diagnostics::Diagnostic::command("", &message);
                            diagnostic.level = "warning";
                            eprint!(
                                "{}",
                                diagnostics::render(
                                    &diagnostic,
                                    options.error_format,
                                    options.color
                                )
                            );
                        }
                        lints::Level::Allow => {}
                    }
                }
                pp.define(name.to_owned(), body);
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

/// The `wchar_t` macros of the selected target, spelled as its compiler
/// prints them. GCC 14 on Linux defines `__WCHAR_MIN__`; Apple clang defines
/// no minimum and marks an unsigned `wchar_t` with `__WCHAR_UNSIGNED__`.
/// `None` leaves a name undefined.
fn wide_character_macros(options: &DriverOptions) -> [(&'static str, Option<String>); 7] {
    let rank = options.target_layout().wide_character_rank();
    let bits = evocc_chir::integer::integer_type_for_rank(rank).bits();
    let size = Some((bits / 8).to_string());
    let width = Some(bits.to_string());
    let (type_name, minimum, maximum, unsigned) = match (options.target, rank) {
        (DriverTarget::Aarch64AppleDarwin, evocc_types::IntRank::Int) => {
            ("int", None, "2147483647", None)
        }
        (DriverTarget::Aarch64AppleDarwin, evocc_types::IntRank::UShort) => {
            ("unsigned short", None, "65535", Some("1".to_owned()))
        }
        (DriverTarget::Aarch64UnknownLinuxGnu, evocc_types::IntRank::UInt) => {
            ("unsigned int", Some("0U".to_owned()), "0xffffffffU", None)
        }
        (DriverTarget::Aarch64UnknownLinuxGnu, evocc_types::IntRank::UShort) => {
            ("short unsigned int", Some("0U".to_owned()), "0xffffU", None)
        }
        _ => unreachable!("each target has one wchar_t of each width"),
    };
    [
        ("__ARM_SIZEOF_WCHAR_T", size.clone()),
        ("__SIZEOF_WCHAR_T__", size),
        ("__WCHAR_WIDTH__", width),
        ("__WCHAR_MIN__", minimum),
        ("__WCHAR_MAX__", Some(maximum.to_owned())),
        ("__WCHAR_TYPE__", Some(type_name.to_owned())),
        ("__WCHAR_UNSIGNED__", unsigned),
    ]
}

fn same_macro_replacement(left: &[evocc_lex::Token], right: &[evocc_lex::Token]) -> bool {
    // Compare preprocessing tokens, not source locations or evaluated values.
    // Leading/trailing space is irrelevant; internal whitespace separation is
    // part of an identical replacement list (comments count as whitespace).
    left.len() == right.len()
        && left.iter().zip(right).enumerate().all(|(index, (a, b))| {
            a.kind == b.kind
                && a.text.spelling() == b.text.spelling()
                && (index == 0 || a.preceded_by_whitespace == b.preceded_by_whitespace)
        })
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
                return Err("invalid macro option encoding".into());
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
    // Each compilation selects its own inline policy before any substitution.
    evocc_chir::inline::set_always_inline_only(!options.optional_inlining);
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
    let cpp_denied = emit_cpp_warnings(options, &pp);
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
    if cpp_denied {
        return ExitCode::from(1);
    }
    let processed = result.into_vec();
    let expanded: Vec<_> = processed
        .iter()
        .filter(|t| t.span != t.spelling_span)
        .map(|t| t.span)
        .collect();

    let arena = Arena::default();
    let mut parser = Parser::from_tokens(processed, FileId::new(1));
    parser.set_target_layout(options.target_layout());
    parser.set_general_registers_only(options.codegen.general_registers_only);
    if let Some(deployment) = options.darwin_deployment {
        parser.set_availability_target(deployment);
    }
    parser.set_standard_builtins(options.standard_builtins);
    parser.set_disabled_builtins(options.disabled_builtins.iter().map(String::as_str));
    parser.set_legacy_declarations(options.language.legacy_declarations());
    parser.enable_lints();
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
                let file = pp
                    .source_path(diagnostic.span.file)
                    .unwrap_or(&options.input);
                let mut rendered = diagnostics::Diagnostic::new(
                    diagnostic.code,
                    &diagnostic.message,
                    diagnostics::Span::source(
                        file,
                        &options.diagnostic_source(file),
                        diagnostic.span,
                        &diagnostic.message,
                    ),
                );
                if let Some(span) = diagnostic.previous_declaration {
                    let file = pp.source_path(span.file).unwrap_or(&options.input);
                    let mut secondary = diagnostics::Span::source(
                        file,
                        &options.diagnostic_source(file),
                        span,
                        "previous declaration is here",
                    );
                    secondary.primary = false;
                    rendered.spans.push(secondary);
                }
                eprint!(
                    "{}",
                    diagnostics::render(&rendered, options.error_format, options.color)
                );
            }
        } else {
            options.emit_error(&options.input, 0, 0, "E1001", "expected-declaration");
        }
        return ExitCode::from(1);
    }

    let mut denied = false;
    let mut repair_diagnostics = Vec::new();
    for finding in parser.lint_findings() {
        // System search paths suppress ordinary source warnings, not parser
        // errors, #warning, the modern-C undeclared-function constraint, or
        // an incompatible pointer conversion that stays an error (W0012).
        if pp.source_is_system_header(finding.span.file)
            && !options.lints.system_header_warnings()
            && (finding.lint != 2 || options.language.legacy_declarations())
            && !options
                .lints
                .reports_in_system_header(finding.lint, options.language.legacy_declarations())
        {
            continue;
        }
        let level = options
            .lints
            .level(finding.lint, options.language.legacy_declarations());
        if level == lints::Level::Allow {
            continue;
        }
        let file = pp.source_path(finding.span.file).unwrap_or(&options.input);
        let mut diagnostic = diagnostics::Diagnostic::new(
            &format!("W{:04}", finding.lint + 1),
            finding.message(),
            diagnostics::Span::source(
                file,
                &options.diagnostic_source(file),
                finding.span,
                finding.message(),
            ),
        );
        diagnostic.level = if level == lints::Level::Deny {
            "error"
        } else {
            "warning"
        };
        diagnostic.help.push(finding.help().into());
        diagnostic.repairs.extend(repairs::suggest(
            *finding,
            parser.lint_findings(),
            &unit,
            &diagnostic.spans[0],
            &expanded,
            &options.target_layout(),
        ));
        repair_diagnostics.push(diagnostic);
        denied |= level == lints::Level::Deny;
    }
    if options.repair_mode.is_some() && !denied {
        return repairs::finish(options, &repair_diagnostics).unwrap_or_else(|error| {
            let message = format!("cannot apply repairs: {error}");
            options.emit_error(&options.input, 0, 0, "E0004", &message);
            ExitCode::from(1)
        });
    }
    for diagnostic in repair_diagnostics {
        eprint!(
            "{}",
            diagnostics::render(&diagnostic, options.error_format, options.color)
        );
    }
    if denied {
        return ExitCode::from(1);
    }

    match action {
        ParsedAction::Validate => {
            if options.defined_functions {
                for declaration in unit.iter() {
                    if let CHirDecl::Function(f) = declaration {
                        if f.body.is_some() {
                            println!("{}", f.name.spelling);
                        }
                    }
                }
            }
            ExitCode::SUCCESS
        }
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
                let (mut module, rewrites) = if options.optimize_scalar_evoir {
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
                        if let Err(detail) = evocc_eqopt::write_target_identity_chain(
                            &module,
                            options.target.triple(),
                            path,
                        ) {
                            options
                                .error_format
                                .usage(&format!("identity-chain publication failed: {detail}"));
                            return ExitCode::from(1);
                        }
                    }
                    (module, None)
                };
                match action {
                    ParsedAction::EmitEvoIr => emit_evoir(&module, options.target.triple()),
                    ParsedAction::EmitAsm => match &options.proof {
                        Some(proof) => proof_required::run(
                            options,
                            proof,
                            &src,
                            &unit,
                            &module,
                            rewrites.as_ref(),
                            resolved,
                            &pp,
                        ),
                        None => emit_asm(options, &module, resolved, &pp),
                    },
                    ParsedAction::InterpretEvoIr => interpret_evoir(
                        options,
                        &mut module,
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
                let entries = if options.proof.is_some() {
                    options
                        .core_entries
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                evocc_lower::lower_unit_with_entries(
                    &unit,
                    &target,
                    options.optimization != 0,
                    &entries,
                    |source, module| {
                        consume(source, module, evocc_lower::SourceLocations::default())
                    },
                )
            };
            match lowered {
                Ok(result) => result,
                Err(error) => {
                    if let Some(span) = error.span() {
                        options.emit_span(
                            pp.source_path(span.file).unwrap_or(&options.input),
                            span,
                            error.code(),
                            &error.message(),
                        );
                    } else {
                        options.emit_error(&options.input, 0, 0, error.code(), &error.message());
                    }
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
        let (arguments, _) = evoir_main_contract_canonical(&module)?;
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
    let mut equality = EqualityState::for_target_module(&module, options.target.triple())?;
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

    let default_arguments = [options.input.as_bytes().to_vec()];
    let arguments = options
        .program_arguments
        .as_deref()
        .unwrap_or(&default_arguments);
    let outcome = match evocc_chir::interp::interpret_program_with_arguments(
        unit,
        entry,
        &options.target_layout(),
        InterpreterLimits::default(),
        Some(arguments),
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(span) = error.span() {
                options.emit_span(
                    pp.source_path(span.file).unwrap_or(&options.input),
                    span,
                    error.code(),
                    &error.message(),
                );
            } else {
                options.emit_execution_error(&options.input, None, error.code(), &error.message());
            }
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

/// EvoIR Section 9.1: the text of a target other than the implicit Linux
/// target names that target in its first line.
fn emit_evoir(module: &Module, target: &str) -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    if write!(
        stdout,
        "{}",
        evocc_evoir::render_target_module(module, target)
    )
    .is_err()
    {
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
fn emit_asm(
    options: &DriverOptions,
    module: &Module,
    unit: &evocc_chir::CHirUnit<'_>,
    pp: &Preprocessor,
) -> ExitCode {
    let text = match assemble(options, module, unit, pp) {
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
    pp: &Preprocessor,
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
    let entries = if options.proof.is_some() {
        options
            .core_entries
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut text = match compile_module_with_entries(
        target,
        module,
        &|_| true,
        relocation,
        &options.codegen,
        &entries,
    ) {
        Ok(text) => text,
        Err(stop) => {
            match &stop {
                NativeStop::Lowering(refusals) => {
                    for refusal in refusals {
                        let message = refusal.message();
                        let span = unit.iter().find_map(|declaration| {
                            let evocc_chir::CHirDecl::Function(function) = declaration else {
                                return None;
                            };
                            function.body?;
                            let linkage = unit.symbol_linkage().ok()?;
                            let name =
                                linkage.function_name(function.symbol, &function.name.spelling);
                            // The assembler label may be on an earlier prototype.
                            // Match the same Darwin prefix removal as emission.
                            let labeled = unit.iter().any(|declaration| {
                                matches!(declaration,
                                evocc_chir::CHirDecl::Function(prior)
                                if prior.symbol == function.symbol && prior.asm_label.is_some())
                            });
                            let name = if options.target.is_darwin() && labeled {
                                name.strip_prefix('_')
                                    .filter(|name| !name.is_empty())
                                    .unwrap_or(name)
                            } else {
                                name
                            };
                            (name == refusal.function).then_some(function.name.span)
                        });
                        if let Some(span) = span {
                            options.emit_span(
                                pp.source_path(span.file).unwrap_or(&options.input),
                                span,
                                refusal.code,
                                &message,
                            );
                        } else {
                            options.emit_error(&options.input, 0, 0, refusal.code, &message);
                        }
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
    module: &mut Module,
    pp: &Preprocessor,
    locations: &evocc_lower::SourceLocations,
    original: &[evocc_evoir::Function],
) -> ExitCode {
    let (arguments, declared_results) = match evoir_main_contract(module, &options.input) {
        Ok(contract) => contract,
        Err(message) => {
            options.emit_execution_error(&options.input, None, "E3007", message);
            return ExitCode::from(1);
        }
    };
    let mut site = None;
    let outcome = match evocc_evoir::interpret_program_with_target_and_site(
        module,
        &arguments,
        &options.target_layout(),
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

/// Add the hosted argv objects and a canonical wrapper around a C `main` that
/// has `(int, char **)` before its memory/world effect parameters.
fn install_hosted_main(module: &mut Module, index: usize, argv0: &str) -> Result<(), &'static str> {
    let params = module.functions[index].params.clone();
    let ret_tys = module.functions[index].ret_tys.clone();
    let effects = module.functions[index].effects;
    let (argc_ty, argv_ty, memory_ty, world_ty) = (
        params[0].ty.clone(),
        params[1].ty.clone(),
        params[2].ty.clone(),
        params[3].ty.clone(),
    );
    let mut origin = module
        .globals
        .iter()
        .map(|global| global.origin)
        .chain(module.functions.iter().flat_map(|function| {
            function.blocks.iter().flat_map(|block| {
                block.insts.iter().filter_map(|inst| match inst {
                    evocc_evoir::Inst::Alloca { origin, .. }
                    | evocc_evoir::Inst::DynamicAlloca { origin, .. } => Some(*origin),
                    _ => None,
                })
            })
        }))
        .max()
        .unwrap_or(0)
        + 1;
    let string_symbol = format!("__evocc_argv0_{origin}");
    let table_symbol = format!("__evocc_argv_{origin}");
    let string_bytes = {
        let mut bytes = argv0.as_bytes().to_vec();
        bytes.push(0);
        bytes
    };
    let string_layout = evocc_evoir::AccessLayout::new(string_bytes.len() as u64, 1);
    let table_layout = evocc_evoir::AccessLayout::new(16, 8);
    module.globals.push(evocc_evoir::GlobalObject {
        read_only: false,
        section: None,
        linkage: evocc_evoir::Linkage::Strong,
        relocs: Vec::new(),
        symbol: string_symbol.clone(),
        origin,
        kind: evocc_evoir::GlobalKind::File,
        visibility: evocc_evoir::GlobalVisibility::Internal,
        layout: string_layout,
        initialized: vec![true; string_bytes.len()],
        bytes: string_bytes,
    });
    origin += 1;
    module.globals.push(evocc_evoir::GlobalObject {
        read_only: false,
        section: None,
        linkage: evocc_evoir::Linkage::Strong,
        relocs: vec![evocc_evoir::Relocation {
            offset: 0,
            symbol: string_symbol,
            addend: 0,
            exposed: false,
            difference: None,
        }],
        symbol: table_symbol.clone(),
        origin,
        kind: evocc_evoir::GlobalKind::File,
        visibility: evocc_evoir::GlobalVisibility::Internal,
        layout: table_layout,
        initialized: vec![true; 16],
        bytes: vec![0; 16],
    });
    let mut original_name = "__evocc_hosted_main".to_owned();
    while module
        .functions
        .iter()
        .any(|function| function.name == original_name)
    {
        original_name.push('_');
    }
    module.functions[index].name = original_name.clone();
    let memory = evocc_evoir::Value {
        id: 0,
        ty: memory_ty.clone(),
    };
    let world = evocc_evoir::Value {
        id: 1,
        ty: world_ty.clone(),
    };
    let argc = evocc_evoir::Value { id: 2, ty: argc_ty };
    let argv = evocc_evoir::Value { id: 3, ty: argv_ty };
    let result = evocc_evoir::Value {
        id: 4,
        ty: ret_tys[0].clone(),
    };
    let memory_out = evocc_evoir::Value {
        id: 5,
        ty: memory_ty.clone(),
    };
    let world_out = evocc_evoir::Value {
        id: 6,
        ty: world_ty.clone(),
    };
    module.functions.push(evocc_evoir::Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        name: "main".to_owned(),
        params: vec![
            evocc_evoir::FunctionParam {
                name: "memory".to_owned(),
                ty: memory_ty,
            },
            evocc_evoir::FunctionParam {
                name: "world".to_owned(),
                ty: world_ty,
            },
        ],
        param_marks: Vec::new(),
        variadic: false,
        ret_tys,
        effects,
        visibility: evocc_evoir::GlobalVisibility::External,
        blocks: vec![evocc_evoir::Block {
            id: 0,
            args: vec![
                evocc_evoir::BlockArg {
                    name: "memory".to_owned(),
                    value: memory.clone(),
                },
                evocc_evoir::BlockArg {
                    name: "world".to_owned(),
                    value: world.clone(),
                },
            ],
            insts: vec![
                evocc_evoir::Inst::Const(argc.clone(), 1),
                evocc_evoir::Inst::GlobalAddr {
                    result: argv.clone(),
                    symbol: table_symbol,
                },
                evocc_evoir::Inst::Call {
                    results: vec![result.clone()],
                    callee: original_name,
                    args: vec![argc, argv],
                    arg_marks: Vec::new(),
                    returns_twice: false,
                    mem_in: memory,
                    world_in: world,
                    mem_out: memory_out.clone(),
                    world_out: world_out.clone(),
                },
            ],
            terminator: evocc_evoir::Inst::Ret(vec![result, memory_out, world_out]),
        }],
    });
    Ok(())
}

fn evoir_main_contract_canonical(
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

fn evoir_main_contract(
    module: &mut Module,
    argv0: &str,
) -> Result<(Vec<RuntimeValue>, Vec<EvoIRType>), &'static str> {
    let index = module
        .functions
        .iter()
        .position(|function| function.name == "main")
        .ok_or("evoir-entry-function-not-found: main")?;
    let params = module.functions[index].params.clone();
    let ret_tys = module.functions[index].ret_tys.clone();
    let arguments = match params.as_slice() {
        [] => Vec::new(),
        [memory, world]
            if memory.ty == EvoIRType::Mem(Region::All) && world.ty == EvoIRType::World =>
        {
            vec![
                RuntimeValue::new(memory.ty.clone(), 0),
                RuntimeValue::new(world.ty.clone(), 0),
            ]
        }
        [argc, argv, memory, world]
            if argc.ty == EvoIRType::Bits(32)
                && matches!(argv.ty, EvoIRType::Ptr(_))
                && memory.ty == EvoIRType::Mem(Region::All)
                && world.ty == EvoIRType::World =>
        {
            install_hosted_main(module, index, argv0)?;
            vec![
                RuntimeValue::new(memory.ty.clone(), 0),
                RuntimeValue::new(world.ty.clone(), 0),
            ]
        }
        _ => return Err("evoir-main-parameters-are-not-canonical"),
    };
    let has_effect_results = match ret_tys.as_slice() {
        [EvoIRType::Bits(1..=64)] => false,
        [EvoIRType::Bits(1..=64), EvoIRType::Mem(Region::All), EvoIRType::World] => true,
        _ => return Err("evoir-main-results-are-not-canonical"),
    };
    if has_effect_results != !arguments.is_empty() {
        return Err("evoir-main-effect-interface-is-inconsistent");
    }
    Ok((arguments, ret_tys))
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
