//! evocc: equality-native, self-contained C compiler.
//!
//! Day-3 scope: the driver now supports -E (preprocess) and --parse-only
//! (parse to C-HIR). Every other pipeline stage remains NOT_IMPLEMENTED.
//!
//! The shell delegates real work to the crates built in Day 2-3:
//!   -E           → evocc-lex → evocc-cpp → token text output
//!   --parse-only → evocc-lex → evocc-parser → C-HIR (validate only)
//!
//! Probe mode (invocation ends with --probe) is preserved from Day 1-2
//! and answers NOT_IMPLEMENTED for stages beyond the current capability.

use std::fs;
use std::io::Write;
use std::process::ExitCode;

use evocc_common::{Arena, FileId};
use evocc_cpp::Preprocessor;
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DriverMode {
    Preprocess,
    ParseOnly,
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
    definitions: Vec<String>,
    standard_includes: bool,
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
    "c-hir-emit",
    "evoir-emit",
    "eqsat-optimize",
];

const FLAG_MAP: &[(&str, &str)] = &[
    ("-E", "preprocess"),
    ("--parse-only", "parse-only"),
    ("--emit=c-hir", "c-hir-emit"),
    ("--emit=evoir", "evoir-emit"),
    ("-Oeq", "eqsat-optimize"),
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
        eprintln!("evocc: unrecognized invocation is NOT_IMPLEMENTED");
        return ExitCode::from(NOT_IMPLEMENTED_EXIT);
    }

    if is_probe(&args) {
        let probe_args: &[String] = &args[..args.len() - 1];
        for (flag, capability) in FLAG_MAP {
            if probe_args.iter().any(|a| a == flag || a.starts_with(flag)) {
                return not_implemented(capability);
            }
        }
        if probe_args.len() == 1 && CAPABILITIES.contains(&probe_args[0].as_str()) {
            return not_implemented(&probe_args[0]);
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
        DriverMode::ParseOnly => run_parse(&options),
    }
}

fn parse_options(args: &[String]) -> Result<DriverOptions, String> {
    let mut mode = None;
    let mut target = None;
    let mut input = None;
    let mut include_paths = Vec::new();
    let mut definitions = Vec::new();
    let mut standard_includes = true;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "-E" => set_mode(&mut mode, DriverMode::Preprocess)?,
            "--parse-only" => set_mode(&mut mode, DriverMode::ParseOnly)?,
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
            "-P" | "-undef" => {}
            "-nostdinc" => standard_includes = false,
            "-I" | "-D" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{arg} requires an argument"))?
                    .clone();
                if arg == "-I" {
                    include_paths.push(value);
                } else {
                    definitions.push(value);
                }
            }
            _ if arg.starts_with("-I") && arg.len() > 2 => {
                include_paths.push(arg[2..].to_owned());
            }
            _ if arg.starts_with("-D") && arg.len() > 2 => {
                definitions.push(arg[2..].to_owned());
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

    Ok(DriverOptions {
        mode: mode.ok_or_else(|| "exactly one of -E or --parse-only is required".to_string())?,
        target: target.unwrap_or(DriverTarget::Aarch64UnknownLinuxGnu),
        input: input.ok_or_else(|| "an input file is required".to_string())?,
        include_paths,
        definitions,
        standard_includes,
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
        return Err("exactly one of -E or --parse-only is required".into());
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
        DriverTarget::Aarch64UnknownLinuxGnu => Preprocessor::new(),
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
    for definition in &options.definitions {
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
        let text = match token.text {
            evocc_lex::TokenText::Decoded(text) => text,
            evocc_lex::TokenText::Borrowed => source
                .get(token.span.byte_start as usize..token.span.byte_end as usize)
                .ok_or_else(|| "invalid macro replacement span".to_string())?
                .to_owned(),
        };
        tokens.push(evocc_lex::Token {
            kind: token.kind,
            span: token.span,
            text: evocc_lex::TokenText::Decoded(text),
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
    for token in tokens.iter().filter(|token| token.kind != TokenKind::Eof) {
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

fn run_parse(options: &DriverOptions) -> ExitCode {
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
    let unit = parser.parse_translation_unit_in(&arena);
    if parser.had_error() || (unit.is_empty() && !src.trim().is_empty()) {
        if let Some(diagnostic) = parser.diagnostic() {
            emit_error(
                pp.source_path(diagnostic.span.file)
                    .unwrap_or(options.input.as_str()),
                diagnostic.span.line_start.max(1),
                diagnostic.span.column_start.max(1),
                diagnostic.code,
                &diagnostic.message,
            );
        } else {
            emit_error(&options.input, 1, 1, "E1001", "expected-declaration");
        }
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
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
