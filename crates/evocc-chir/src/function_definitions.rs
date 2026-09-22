//! Resolve function-body roles from the complete file-scope declaration chain.
//!
//! An inline body is not a linkable definition. In particular taking its
//! address never turns a GNU extern-inline body into an external definition.

use crate::{CHirDecl, CHirUnit, FunctionDecl, StorageClass, SymbolId};
use evocc_common::SourceSpan;
use std::collections::{BTreeMap, BTreeSet};

/// Entry attributes merge before the definition. A conflicting declaration
/// after it has divergent GCC/Clang semantics and is rejected, not discarded.
pub fn patchable_entries(
    unit: &CHirUnit<'_>,
) -> Result<BTreeMap<SymbolId, (u16, u16)>, SourceSpan> {
    let mut result = BTreeMap::new();
    let mut defined = BTreeSet::new();
    let mut declarations = Vec::new();
    crate::inline::for_each_declaration_ordered(unit, |decl| declarations.push(decl));
    for declaration in declarations {
        let CHirDecl::Function(function) = declaration else {
            continue;
        };
        for attribute in &function.attributes {
            if attribute.semantic_name() != Some("patchable_function_entry") {
                continue;
            }
            let entry = patchable_entry(attribute)?;
            if defined.contains(&function.symbol) && result.get(&function.symbol) != Some(&entry) {
                return Err(attribute.span);
            }
            result.insert(function.symbol, entry);
        }
        if function.body.is_some() {
            defined.insert(function.symbol);
        }
    }
    Ok(result)
}

pub fn patchable_entry(attribute: &crate::Attribute) -> Result<(u16, u16), SourceSpan> {
    let number = |arg: &crate::AttributeArgument| {
        arg.integer
            .and_then(|value| u16::try_from(value).ok())
            .ok_or(attribute.span)
    };
    let (total, before) = match attribute.arguments.as_slice() {
        [total] => (number(total)?, 0),
        [total, before] => (number(total)?, number(before)?),
        _ => return Err(attribute.span),
    };
    if before > total {
        return Err(attribute.span);
    }
    Ok((total, before))
}

#[derive(Debug, Clone)]
pub struct DefinitionConflict {
    pub symbol: SymbolId,
    pub first: SourceSpan,
    pub duplicate: SourceSpan,
}

#[derive(Default)]
pub struct FunctionDefinitions<'a> {
    pub definitions: BTreeMap<SymbolId, &'a FunctionDecl<'a>>,
    pub inline_bodies: BTreeMap<SymbolId, &'a FunctionDecl<'a>>,
    pub always_inline: BTreeSet<SymbolId>,
}

impl<'a> FunctionDefinitions<'a> {
    pub fn derive(unit: &CHirUnit<'a>) -> Result<Self, DefinitionConflict> {
        let mut chains = BTreeMap::<SymbolId, Vec<&'a FunctionDecl<'a>>>::new();
        for declaration in unit.iter() {
            if let CHirDecl::Function(f) = declaration {
                let chain = chains.entry(f.symbol).or_default();
                // The allocation manifest diagnoses a reused declaration node
                // as such, rather than confusing it with two written bodies.
                if !chain.iter().any(|old| std::ptr::eq(*old, f)) {
                    chain.push(f);
                }
            }
        }
        let mut result = Self::default();
        for (symbol, chain) in chains {
            if chain.iter().any(|f| {
                f.attributes
                    .iter()
                    .any(|a| a.semantic_name() == Some("always_inline"))
            }) {
                result.always_inline.insert(symbol);
            }
            let bodies: Vec<_> = chain.iter().copied().filter(|f| f.body.is_some()).collect();
            if bodies.is_empty() {
                continue;
            }
            let internal = bodies[0].has_internal_linkage(unit.symbols());
            let gnu = unit.gnu_inline()
                || chain.iter().any(|f| {
                    !f.inline_spans.is_empty()
                        && f.attributes
                            .iter()
                            .any(|a| a.semantic_name() == Some("gnu_inline"))
                });
            let gnu_inline_only = |f: &FunctionDecl<'_>| {
                !internal
                    && gnu
                    && !f.inline_spans.is_empty()
                    && f.storage.base() == StorageClass::Extern
            };
            // GNU permits an inline-only body followed by the real definition,
            // not two ordinary definitions or the reverse definition order.
            if bodies.len() > 1
                && (bodies.len() != 2 || !gnu_inline_only(bodies[0]) || gnu_inline_only(bodies[1]))
            {
                return Err(DefinitionConflict {
                    symbol,
                    first: bodies[0].span,
                    duplicate: bodies[1].span,
                });
            }
            let body = *bodies.last().unwrap();
            let provides_definition = if internal || bodies.len() == 2 {
                true
            } else if gnu {
                !gnu_inline_only(body)
                    || chain.iter().any(|f| {
                        !f.inline_spans.is_empty() && f.storage.base() != StorageClass::Extern
                    })
            } else {
                chain
                    .iter()
                    .any(|f| f.inline_spans.is_empty() || f.storage.base() == StorageClass::Extern)
            };
            if provides_definition {
                result.definitions.insert(symbol, body);
            }
            // A later real definition supersedes the inline-only body for
            // direct calls too, including calls written before that definition.
            // Select its own parameter/local identities, not those of the
            // earlier body merely because that node spells `inline`.
            if chain.iter().any(|f| !f.inline_spans.is_empty()) {
                result.inline_bodies.insert(symbol, body);
            }
        }
        Ok(result)
    }

    pub fn is_definition(&self, function: &FunctionDecl<'_>) -> bool {
        self.definitions
            .get(&function.symbol)
            .is_some_and(|definition| std::ptr::eq(*definition, function))
    }
}
