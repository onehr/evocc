//! The parts of the `aarch64-apple-darwin` target that the driver owns: the
//! predefined macros and the assembler names of declarations.

use std::collections::HashMap;

use evocc_chir::{CHirDecl, CHirUnit};
use evocc_common::FileId;
use evocc_cpp::{Lexer, Preprocessor, TokenKind};
use evocc_evoir::{Inst, Module};

/// The macros that differ from the Linux profile.
const PREDEFINED: &str = include_str!("aarch64-apple-darwin.h");

/// The deployment version when `MACOSX_DEPLOYMENT_TARGET` is absent. It is
/// the first macOS release for arm64.
const DEFAULT_DEPLOYMENT: u32 = 11_00_00;

/// Turn the Linux profile of `pp` into the Darwin profile.
pub(crate) fn apply_predefined_profile(pp: &mut Preprocessor) -> Result<(), String> {
    let mut lexer = Lexer::from_bytes(FileId::DUMMY, PREDEFINED.as_bytes());
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("invalid token in the Darwin predefined profile".into());
        }
        tokens.push(token);
    }
    let output = pp.process_bytes(evocc_cpp::TokenStream::new(tokens), PREDEFINED.as_bytes());
    if pp.has_errors()
        || output
            .into_vec()
            .iter()
            .any(|token| token.kind != TokenKind::Eof)
    {
        return Err("the Darwin predefined profile is not definitions only".into());
    }
    // The SDK's <stdatomic.h> is Clang's, which calls these builtins.
    pp.add_target_builtins(evocc_parser::builtins::CLANG_ATOMICS.iter().copied());
    let version = deployment_version().to_string();
    for name in [
        "__ENVIRONMENT_MAC_OS_X_VERSION_MIN_REQUIRED__",
        "__ENVIRONMENT_OS_VERSION_MIN_REQUIRED__",
    ] {
        pp.define(name.to_owned(), crate::decoded_tokens(&version)?);
    }
    Ok(())
}

/// The minimum macOS version as Apple's headers encode it: 14.2 is 140200.
/// `MACOSX_DEPLOYMENT_TARGET` selects it, as it does for Apple's tools.
fn deployment_version() -> u32 {
    let Ok(text) = std::env::var("MACOSX_DEPLOYMENT_TARGET") else {
        return DEFAULT_DEPLOYMENT;
    };
    let mut parts = text.trim().split('.').map(str::parse::<u32>);
    match (parts.next(), parts.next(), parts.next()) {
        (Some(Ok(major)), minor, patch) if (10..100).contains(&major) => {
            let minor = minor.and_then(Result::ok).unwrap_or(0).min(99);
            let patch = patch.and_then(Result::ok).unwrap_or(0).min(99);
            major * 10_000 + minor * 100 + patch
        }
        _ => DEFAULT_DEPLOYMENT,
    }
}

/// Give each symbol that an assembler label names its Darwin spelling.
///
/// Apple's headers write the platform prefix into the label, for example
/// `int close(int) __asm("_close")`. The backend adds the `_` prefix to every
/// symbol, so the module must hold the name without it.
pub(crate) fn apply_assembler_labels(unit: &CHirUnit<'_>, module: &mut Module) {
    let names: HashMap<String, String> = unit
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Function(function) => function.asm_label.as_ref(),
            CHirDecl::Object(object) => object.asm_label.as_ref(),
            _ => None,
        })
        .filter_map(|label| {
            label
                .strip_prefix('_')
                .filter(|name| !name.is_empty())
                .map(|name| (label.clone(), name.to_owned()))
        })
        .collect();
    if names.is_empty() {
        return;
    }
    let rename = |name: &mut String| {
        if let Some(spelling) = names.get(name.as_str()) {
            name.clone_from(spelling);
        }
    };
    for function in &mut module.functions {
        rename(&mut function.name);
        for block in &mut function.blocks {
            for inst in &mut block.insts {
                match inst {
                    Inst::Call { callee, .. } => rename(callee),
                    Inst::FuncAddr { symbol, .. } | Inst::GlobalAddr { symbol, .. } => {
                        rename(symbol)
                    }
                    _ => {}
                }
            }
        }
    }
    for declaration in &mut module.declares {
        rename(&mut declaration.name);
    }
    for alias in &mut module.aliases {
        rename(&mut alias.name);
        rename(&mut alias.target);
    }
    for global in &mut module.globals {
        rename(&mut global.symbol);
        for relocation in &mut global.relocs {
            rename(&mut relocation.symbol);
        }
    }
    for object in &mut module.extern_objects {
        rename(&mut object.symbol);
    }
    for registration in &mut module.lifecycle {
        rename(&mut registration.function);
    }
}
