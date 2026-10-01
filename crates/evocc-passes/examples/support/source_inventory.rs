//! Resolve the same normalized declaration inventory used by C lowering.
use std::collections::BTreeMap;

use evocc_chir::CHirDecl;
use evocc_common::{Arena, FileId};
use evocc_evoir::Module;
use evocc_lex::Lexer;
use evocc_passes::preflight::SourcePermission;
use evocc_types::TargetDataLayout;

/// Input is already preprocessed with the original source argv. Reuse the
/// preprocessor only to restore its semantic pragma tokens, then require the
/// caller to compare this lowered module with that original driver's output.
pub fn lower_preprocessed(
    source: &str,
) -> Result<(Module, BTreeMap<String, SourcePermission>), String> {
    let arena = Arena::default();
    let file = FileId::new(1);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut pp = evocc_cpp::Preprocessor::new();
    pp.set_standard_includes_enabled(false);
    let processed = pp.process(evocc_cpp::TokenStream::new(tokens), source);
    if pp.has_errors() {
        return Err(format!("preprocessed input: {:?}", pp.errors()));
    }
    let mut parser = evocc_parser::Parser::from_tokens(processed.into_vec(), file);
    parser.set_target_layout(TargetDataLayout::aarch64_unknown_linux_gnu());
    // Frozen S5 source lane: GNU11, no legacy declarations or gnu89-inline.
    parser.set_legacy_declarations(false);
    parser.set_gnu_inline(false);
    let unit = parser.parse_translation_unit_in(&arena);
    if parser.had_error() {
        return Err(format!("source inventory: {:?}", parser.diagnostic()));
    }
    evocc_lower::lower_unit_with_source(
        &unit,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        true,
        |resolved, module| {
            let linkage = resolved.symbol_linkage().map_err(|e| format!("{e:?}"))?;
            let mut permissions = BTreeMap::new();
            evocc_chir::inline::for_each_declaration_ordered(resolved, |declaration| {
                let CHirDecl::Function(function) = declaration else {
                    return;
                };
                let name = linkage
                    .function_name(function.symbol, &function.name.spelling)
                    .to_string();
                let permission = permissions.entry(name).or_insert(SourcePermission::Allowed);
                // Retention attributes are explicit roots even though EvoIR has
                // no separate retained-body field. Do not infer permission from
                // an apparently single-use lowered symbol in that case.
                if function.attributes.iter().any(|attribute| {
                    matches!(
                        attribute.semantic_name(),
                        Some("noinline" | "noipa" | "noclone" | "used" | "retain")
                    )
                }) {
                    *permission = SourcePermission::Barrier;
                }
            });
            Ok((module, permissions))
        },
    )
    .map_err(|e| e.to_string())?
}
