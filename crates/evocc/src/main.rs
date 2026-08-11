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
use std::process::ExitCode;

use evocc_common::{FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_cpp::Preprocessor;
use evocc_parser::Parser;

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
        eprintln!("       evocc -E <file>");
        eprintln!("       evocc --parse-only <file>");
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

    // --- Real invocations (non-probe) ------------------------------------

    match args.as_slice() {
        [flag, file] if flag == "-E" => run_preprocess(file),
        [flag, file] if flag == "--parse-only" => run_parse(file),
        _ => {
            eprintln!("evocc: unrecognized invocation is NOT_IMPLEMENTED: {args:?}");
            ExitCode::from(NOT_IMPLEMENTED_EXIT)
        }
    }
}

fn run_preprocess(file: &str) -> ExitCode {
    let src = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("evocc: cannot read `{file}`: {e}");
            return ExitCode::from(NOT_IMPLEMENTED_EXIT);
        }
    };

    let mut tokens = Vec::new();
    {
        let mut lex = Lexer::new(FileId::new(1), &src);
        while let Some(tok) = lex.next() {
            tokens.push(tok);
        }
    }

    let mut pp = Preprocessor::new();
    if let Some(parent) = std::path::Path::new(file).parent() {
        pp.add_include_path(parent.to_string_lossy().to_string());
    }
    for include_path in &[".", "/usr/include", "/usr/local/include"] {
        pp.add_include_path(include_path.to_string());
    }
    let stream = evocc_cpp::TokenStream::new(tokens);
    let result = pp.process(stream, &src);
    let final_tokens = result.into_vec();

    // Output token text: decoded tokens carry their own text (from macro
    // expansion); borrowed tokens reconstruct text from the source span.
    let mut prev_kind: Option<TokenKind> = None;
    for tok in &final_tokens {
        if tok.kind == TokenKind::Eof {
            continue;
        }
        let text = match &tok.text {
            evocc_lex::TokenText::Decoded(s) => s.as_str(),
            evocc_lex::TokenText::Borrowed => {
                let end = (tok.span.byte_end as usize).min(src.len());
                let start = (tok.span.byte_start as usize).min(end);
                &src[start..end]
            }
        };
        if text.is_empty() {
            continue;
        }
        // gcc -E preserves original source spacing; our tokens carry no
        // whitespace, so we approximate by putting a space between every
        // pair of tokens except where they are naturally tight (close-paren
        // after a token, token after open-paren).
        let no_space_before = matches!(tok.kind,
            TokenKind::Semicolon | TokenKind::Comma | TokenKind::RParen |
            TokenKind::RBracket | TokenKind::RBrace | TokenKind::Colon
        );
        let no_space_after = matches!(prev_kind,
            Some(TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace)
        );
        if !no_space_before && !no_space_after && prev_kind.is_some() {
            print!(" ");
        }
        print!("{text}");
        prev_kind = Some(tok.kind);
    }
    println!();

    ExitCode::from(0)
}

fn run_parse(file: &str) -> ExitCode {
    let src = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("evocc: cannot read `{file}`: {e}");
            return ExitCode::from(NOT_IMPLEMENTED_EXIT);
        }
    };

    let mut tokens = Vec::new();
    {
        let mut lex = Lexer::new(FileId::new(1), &src);
        while let Some(tok) = lex.next() {
            tokens.push(tok);
        }
    }

    let mut pp = Preprocessor::new();
    let stream = evocc_cpp::TokenStream::new(tokens);
    let result = pp.process(stream, &src);
    let processed = result.into_vec();

    let mut parser = Parser::new(processed, FileId::new(1), &src);
    let unit = parser.parse_translation_unit();

    if unit.is_empty() {
        eprintln!("evocc: `{file}` parsed to an empty translation unit");
    }

    ExitCode::from(0)
}
