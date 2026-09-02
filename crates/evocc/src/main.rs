//! evocc: equality-native, self-contained C compiler.
//!
//! The driver supports preprocessing, C-HIR validation, both scalar Day-6
//! interpreters, and canonical EvoIR text emission. Later optimization and
//! code-generation stages remain NOT_IMPLEMENTED.
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

use evocc_chir::{
    interp::{interpret_scalar_function, InterpreterLimits},
    CHirDecl,
};
use evocc_common::{Arena, FileId};
use evocc_cpp::{pragma_pack_action, Preprocessor};
use evocc_eqopt::{optimize_scalar_module_certified_persistent, EqualityState, ScalarEqBudget};
use evocc_evoir::{
    interpret_module, render_module, InterpreterLimits as EvoIrLimits, Module, RuntimeValue,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

#[derive(Clone, Copy, PartialEq, Eq)]
enum DriverMode {
    Preprocess,
    ParseOnly,
    InterpretCHir,
    EmitCHir,
    EmitEvoIr,
    InterpretEvoIr,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParsedAction {
    Validate,
    InterpretCHir,
    EmitCHir,
    EmitEvoIr,
    InterpretEvoIr,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DriverTarget {
    Aarch64UnknownLinuxGnu,
}

struct DriverOptions {
    mode: DriverMode,
    target: DriverTarget,
    input: String,
    include_paths: Vec<String>,
    forced_includes: Vec<String>,
    macro_prefix_maps: Vec<(String, String)>,
    macro_actions: Vec<MacroAction>,
    standard_includes: bool,
    optimize_scalar_evoir: bool,
    /// Report every diagnostic rather than the first.
    ///
    /// A build wants one unambiguous first-error site. Surveying a body of
    /// code for what it requires wants the whole set, or it takes one build
    /// per unsupported construct to enumerate.
    survey: bool,
}

enum MacroAction {
    Define(String),
    Undefine(String),
}

const SUPPORTED_TARGET: &str = "aarch64-unknown-linux-gnu";

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
    "eqsat-optimize",
    "eq-scalar-optimize",
];

const FLAG_MAP: &[(&str, &str)] = &[
    ("-E", "preprocess"),
    ("--parse-only", "parse-only"),
    ("--interpret-c-hir", "interpret-c-hir"),
    ("--emit=c-hir", "c-hir-emit"),
    ("--emit=evoir", "evoir-emit"),
    ("--interpret-evoir", "evoir-interpret"),
    ("-Oeq", "eqsat-optimize"),
    ("-Oeq-scalar", "eq-scalar-optimize"),
];

const IMPLEMENTED_EVOIR_PROBES: &[&str] = &[
    "evoir-emit",
    "evoir-interpret",
    "--emit=evoir",
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

    if args.is_empty() {
        eprintln!("usage: evocc <{}> --probe", CAPABILITIES.join("|"));
        eprintln!("       evocc -E [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --parse-only [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --interpret-c-hir [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --emit=evoir [--target={SUPPORTED_TARGET}] <file>");
        eprintln!("       evocc --interpret-evoir [--target={SUPPORTED_TARGET}] <file>");
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

    let options = match parse_options(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("evocc: {message}");
            return ExitCode::from(1);
        }
    };
    match options.mode {
        DriverMode::Preprocess => run_preprocess(&options),
        DriverMode::ParseOnly => run_parsed(&options, ParsedAction::Validate),
        DriverMode::InterpretCHir => run_parsed(&options, ParsedAction::InterpretCHir),
        DriverMode::EmitCHir => run_parsed(&options, ParsedAction::EmitCHir),
        DriverMode::EmitEvoIr => run_parsed(&options, ParsedAction::EmitEvoIr),
        DriverMode::InterpretEvoIr => run_parsed(&options, ParsedAction::InterpretEvoIr),
    }
}

fn parse_options(args: &[String]) -> Result<DriverOptions, String> {
    let mut mode = None;
    let mut target = None;
    let mut input = None;
    let mut include_paths = Vec::new();
    let mut forced_includes = Vec::new();
    let mut macro_prefix_maps = Vec::new();
    let mut macro_actions = Vec::new();
    let mut standard_includes = true;
    let mut optimize_scalar_evoir = false;
    let mut survey = false;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "-E" => set_mode(&mut mode, DriverMode::Preprocess)?,
            "--parse-only" => set_mode(&mut mode, DriverMode::ParseOnly)?,
            "--interpret-c-hir" => set_mode(&mut mode, DriverMode::InterpretCHir)?,
            "--emit=c-hir" => set_mode(&mut mode, DriverMode::EmitCHir)?,
            "--emit=evoir" => set_mode(&mut mode, DriverMode::EmitEvoIr)?,
            "--interpret-evoir" => set_mode(&mut mode, DriverMode::InterpretEvoIr)?,
            "-Oeq-scalar" => optimize_scalar_evoir = true,
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
            "-P" | "-undef" => {}
            "-nostdinc" => standard_includes = false,
            "-I" | "-D" | "-U" | "-include" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{arg} requires an argument"))?
                    .clone();
                match arg.as_str() {
                    "-I" => include_paths.push(value),
                    "-D" => macro_actions.push(MacroAction::Define(value)),
                    "-U" => macro_actions.push(MacroAction::Undefine(value)),
                    "-include" => forced_includes.push(value),
                    _ => unreachable!("matched driver option"),
                }
            }
            _ if arg.starts_with("-I") && arg.len() > 2 => {
                include_paths.push(arg[2..].to_owned());
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
            "-std=gnu11" => {}
            _ if arg.starts_with("-std=") => {
                return Err(format!("unsupported C language mode `{arg}`"));
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

    let mode = mode.ok_or_else(|| {
            "exactly one of -E, --parse-only, --interpret-c-hir, --emit=evoir, or --interpret-evoir is required"
                .to_string()
        })?;
    if optimize_scalar_evoir && !matches!(mode, DriverMode::EmitEvoIr | DriverMode::InterpretEvoIr)
    {
        return Err("-Oeq-scalar requires --emit=evoir or --interpret-evoir".to_owned());
    }
    Ok(DriverOptions {
        mode,
        target: target.unwrap_or(DriverTarget::Aarch64UnknownLinuxGnu),
        input: input.ok_or_else(|| "an input file is required".to_string())?,
        include_paths,
        forced_includes,
        macro_prefix_maps,
        macro_actions,
        standard_includes,
        optimize_scalar_evoir,
        survey,
    })
}

fn parse_target(value: &str) -> Result<DriverTarget, String> {
    match value {
        SUPPORTED_TARGET => Ok(DriverTarget::Aarch64UnknownLinuxGnu),
        _ => Err(format!(
            "unsupported target `{value}`; supported target is `{SUPPORTED_TARGET}`"
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
            "exactly one of -E, --parse-only, --interpret-c-hir, --emit=evoir, or --interpret-evoir is required"
                .into(),
        );
    }
    Ok(())
}

fn run_preprocess(options: &DriverOptions) -> ExitCode {
    let (src, tokens) = match read_and_lex(&options.input) {
        Ok(value) => value,
        Err(code) => return code,
    };

    let mut pp = match configured_preprocessor(options) {
        Ok(pp) => pp,
        Err(message) => {
            emit_error(&options.input, 1, 1, "E0101", &message);
            return ExitCode::from(1);
        }
    };
    let result = pp.process(evocc_cpp::TokenStream::new(tokens), &src);
    if pp.has_errors() {
        for error in pp.errors() {
            emit_error(
                pp.source_path(error.span.file)
                    .unwrap_or(options.input.as_str()),
                error.span.line_start.max(1),
                error.span.column_start.max(1),
                error.code,
                &error.message,
            );
        }
        return ExitCode::from(1);
    }

    let rendered = match render_preprocessed(&result.into_vec(), &src) {
        Ok(rendered) => rendered,
        Err(message) => {
            emit_error(&options.input, 1, 1, "E0003", &message);
            return ExitCode::from(1);
        }
    };
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{rendered}").is_err() {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn read_and_lex(file: &str) -> Result<(String, Vec<evocc_lex::Token>), ExitCode> {
    let bytes = match fs::read(file) {
        Ok(s) => s,
        Err(e) => {
            emit_error(file, 1, 1, "E0003", &format!("cannot read source: {e}"));
            return Err(ExitCode::from(1));
        }
    };
    let src = match String::from_utf8(bytes) {
        Ok(src) => src,
        Err(_) => {
            emit_error(file, 1, 1, "E0002", "source is not valid UTF-8");
            return Err(ExitCode::from(1));
        }
    };

    let mut tokens = Vec::new();
    let mut lex = Lexer::new(FileId::new(1), &src);
    while let Some(token) = lex.next() {
        if token.kind == TokenKind::Error {
            emit_error(
                file,
                token.span.line_start.max(1),
                token.span.column_start.max(1),
                "E0001",
                "invalid token",
            );
            return Err(ExitCode::from(1));
        }
        tokens.push(token);
    }
    Ok((src, tokens))
}

fn configured_preprocessor(options: &DriverOptions) -> Result<Preprocessor, String> {
    // Dispatch through the selected frontend target even though the Day-3
    // preprocessor does not yet expose target-specific predefined macros.
    // This is target selection for preprocessing/parsing, not a codegen claim.
    let mut pp = match options.target {
        DriverTarget::Aarch64UnknownLinuxGnu => Preprocessor::for_target(SUPPORTED_TARGET)?,
    };
    pp.current_file = options.input.clone();
    pp.set_standard_includes_enabled(options.standard_includes);
    pp.set_predefined_date(&predefined_date());
    if let Some(parent) = std::path::Path::new(&options.input).parent() {
        pp.add_include_path(parent.to_string_lossy().to_string());
    }
    for include_path in &options.include_paths {
        pp.add_include_path(include_path.clone());
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
                pp.define(name.to_owned(), decoded_tokens(replacement)?);
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
    let mut tokens = Vec::new();
    let mut lexer = Lexer::new(FileId::DUMMY, source);
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Eof {
            break;
        }
        if token.kind == TokenKind::Error {
            return Err("invalid token in macro replacement".into());
        }
        let text = match &token.text {
            evocc_lex::TokenText::Decoded(text) => text.clone(),
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

fn render_preprocessed(tokens: &[evocc_lex::Token], source: &str) -> Result<String, String> {
    let mut rendered = String::new();
    let mut unseparated: Vec<(TokenKind, String)> = Vec::new();
    for token in tokens
        .iter()
        .filter(|token| token.kind != TokenKind::Eof && pragma_pack_action(token).is_none())
    {
        let text = match &token.text {
            evocc_lex::TokenText::Decoded(text) => text.clone(),
            evocc_lex::TokenText::Borrowed => source
                .get(token.span.byte_start as usize..token.span.byte_end as usize)
                .ok_or_else(|| "token span does not belong to the active source".to_string())?
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
    let (src, tokens) = match read_and_lex(&options.input) {
        Ok(value) => value,
        Err(code) => return code,
    };

    let mut pp = match configured_preprocessor(options) {
        Ok(pp) => pp,
        Err(message) => {
            emit_error(&options.input, 1, 1, "E0101", &message);
            return ExitCode::from(1);
        }
    };
    let result = pp.process(evocc_cpp::TokenStream::new(tokens), &src);
    if pp.has_errors() {
        for error in pp.errors() {
            emit_error(
                pp.source_path(error.span.file)
                    .unwrap_or(options.input.as_str()),
                error.span.line_start.max(1),
                error.span.column_start.max(1),
                error.code,
                &error.message,
            );
        }
        return ExitCode::from(1);
    }
    let processed = result.into_vec();

    let arena = Arena::default();
    let mut parser = Parser::new(processed, FileId::new(1), &src);
    if !options.survey {
        parser.set_error_limit(NonZeroUsize::MIN);
    }
    let unit = parser.parse_translation_unit_in(&arena);
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
                emit_error(
                    pp.source_path(diagnostic.span.file)
                        .unwrap_or(options.input.as_str()),
                    diagnostic.span.line_start.max(1),
                    diagnostic.span.column_start.max(1),
                    diagnostic.code,
                    &diagnostic.message,
                );
            }
        } else {
            emit_error(&options.input, 1, 1, "E1001", "expected-declaration");
        }
        return ExitCode::from(1);
    }

    match action {
        ParsedAction::Validate => ExitCode::SUCCESS,
        ParsedAction::InterpretCHir => interpret_chir(options, &pp, &unit),
        ParsedAction::EmitCHir => emit_chir(&unit),
        ParsedAction::EmitEvoIr | ParsedAction::InterpretEvoIr => {
            let target = match options.target {
                DriverTarget::Aarch64UnknownLinuxGnu => {
                    TargetDataLayout::aarch64_unknown_linux_gnu()
                }
            };
            let module = match lower_unit(&unit, &target) {
                Ok(module) => module,
                Err(error) => {
                    let span = error.span();
                    emit_error(
                        span.and_then(|span| pp.source_path(span.file))
                            .unwrap_or(options.input.as_str()),
                        span.map_or(1, |span| span.line_start.max(1)),
                        span.map_or(1, |span| span.column_start.max(1)),
                        error.code(),
                        &error.message(),
                    );
                    return ExitCode::from(1);
                }
            };
            let module = if options.optimize_scalar_evoir {
                optimize_scalar_evoir(module)
            } else {
                module
            };
            match action {
                ParsedAction::EmitEvoIr => emit_evoir(&module),
                ParsedAction::InterpretEvoIr => interpret_evoir(options, &module),
                ParsedAction::Validate | ParsedAction::InterpretCHir | ParsedAction::EmitCHir => {
                    unreachable!("matched EvoIR action")
                }
            }
        }
    }
}

fn optimize_scalar_evoir(module: evocc_evoir::Module) -> evocc_evoir::Module {
    let Ok(mut equality) = EqualityState::for_module(&module) else {
        return module;
    };
    if equality.ingest_module(&module).is_err() {
        return module;
    }
    optimize_scalar_module_certified_persistent(&module, &mut equality, ScalarEqBudget::default())
        .module
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
        emit_error(
            &options.input,
            1,
            1,
            "E3007",
            "interpreter-entry-function-not-found: main",
        );
        return ExitCode::from(1);
    };

    let outcome = match interpret_scalar_function(unit, entry, InterpreterLimits::default()) {
        Ok(outcome) => outcome,
        Err(error) => {
            let span = error.span();
            emit_error(
                span.and_then(|span| pp.source_path(span.file))
                    .unwrap_or(options.input.as_str()),
                span.map_or(1, |span| span.line_start.max(1)),
                span.map_or(1, |span| span.column_start.max(1)),
                error.code(),
                &error.message(),
            );
            return ExitCode::from(1);
        }
    };
    let Some(value) = outcome.return_value else {
        emit_error(
            &options.input,
            1,
            1,
            "E3007",
            "interpreter-main-returned-no-value",
        );
        return ExitCode::from(1);
    };
    let mathematical = match value.mathematical_value(unit.types()) {
        Ok(value) => value,
        Err(_) => {
            emit_error(
                &options.input,
                1,
                1,
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

fn interpret_evoir(options: &DriverOptions, module: &Module) -> ExitCode {
    let (arguments, declared_results) = match evoir_main_contract(module) {
        Ok(contract) => contract,
        Err(message) => {
            emit_error(&options.input, 1, 1, "E3007", message);
            return ExitCode::from(1);
        }
    };
    let outcome = match interpret_module(module, "main", &arguments, EvoIrLimits::new(100_000)) {
        Ok(outcome) => outcome,
        Err(error) => {
            emit_error(&options.input, 1, 1, error.code(), &error.message());
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
        emit_error(
            &options.input,
            1,
            1,
            "E3007",
            "evoir-main-result-does-not-match-declaration",
        );
        return ExitCode::from(1);
    }
    let Some(value) = outcome.return_values.first() else {
        emit_error(
            &options.input,
            1,
            1,
            "E3007",
            "evoir-main-returned-no-value",
        );
        return ExitCode::from(1);
    };
    let mathematical = match signed_scalar(value) {
        Ok(value) => value,
        Err(message) => {
            emit_error(&options.input, 1, 1, "E3007", message);
            return ExitCode::from(1);
        }
    };
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{mathematical}").is_err() {
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

fn emit_error(file: &str, line: u32, column: u32, code: &str, message: &str) {
    eprintln!("{file}:{line}:{column}: error[{code}]: {message}");
}

fn predefined_date() -> String {
    let seconds = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0)
        });
    c_date_from_days(seconds.div_euclid(86_400))
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
