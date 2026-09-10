//! Symbol linkage, resolved once from the declaration inventory of a unit.

use crate::{CHirDecl, CHirUnit, StorageClass, StringLit, StringPrefix, SymbolId};
use evocc_common::SourceSpan;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub struct LinkageError {
    pub code: &'static str,
    pub message: &'static str,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Default)]
pub struct SymbolLinkage {
    pub weak: BTreeSet<SymbolId>,
    pub aliases: BTreeMap<SymbolId, SymbolId>,
    /// An explicit default visibility is retained to detect later conflicts.
    pub hidden: BTreeMap<SymbolId, bool>,
    pub object_sections: BTreeMap<SymbolId, String>,
    function_names: BTreeMap<SymbolId, String>,
    assembly_targets: BTreeMap<SymbolId, SymbolId>,
}

impl SymbolLinkage {
    pub fn target(&self, symbol: SymbolId) -> SymbolId {
        let symbol = self.aliases.get(&symbol).copied().unwrap_or(symbol);
        self.assembly_targets
            .get(&symbol)
            .copied()
            .unwrap_or(symbol)
    }

    pub fn function_name<'a>(&'a self, symbol: SymbolId, spelling: &'a str) -> &'a str {
        self.function_names
            .get(&symbol)
            .map_or(spelling, String::as_str)
    }

    pub fn derive(unit: &CHirUnit<'_>) -> Result<Self, LinkageError> {
        let mut result = Self::default();
        let mut names = BTreeMap::new();
        let mut definitions = BTreeSet::new();
        let mut written_aliases = BTreeMap::new();
        let mut function_declarations = BTreeMap::new();
        // A symbol retains the scope of its first declaration. A later
        // file-scope definition can therefore have a block-scoped identity.
        let file_declarations = unit
            .iter()
            .map(|d| d as *const CHirDecl<'_>)
            .collect::<std::collections::HashSet<_>>();
        let mut declarations = Vec::new();
        crate::inline::for_each_declaration(unit, |d| declarations.push(d));
        for declaration in declarations {
            if let CHirDecl::Function(function) = declaration {
                function_declarations
                    .entry(function.symbol)
                    .and_modify(|old: &mut &crate::FunctionDecl<'_>| {
                        if function.body.is_some() {
                            *old = function;
                        }
                    })
                    .or_insert(function);
                if let Some(name) = &function.asm_label {
                    result.function_names.insert(function.symbol, name.clone());
                }
            }
            let (symbol, name, storage, attributes, defined) = match declaration {
                CHirDecl::Function(f) => (
                    f.symbol,
                    &f.name.spelling,
                    f.storage,
                    &f.attributes,
                    f.body.is_some(),
                ),
                CHirDecl::Object(g) => (
                    g.symbol,
                    &g.name.spelling,
                    g.storage,
                    &g.attributes,
                    g.storage != StorageClass::Extern,
                ),
                _ => continue,
            };
            let file_scope = file_declarations.contains(&(declaration as *const CHirDecl<'_>));
            if file_scope {
                names.insert(name.clone(), symbol);
            }
            for attribute in attributes {
                let is_visibility = attribute.semantic_name() == Some("visibility");
                let is_section = attribute.semantic_name() == Some("section")
                    && matches!(declaration, CHirDecl::Object(_));
                if !is_visibility && !is_section {
                    continue;
                }
                let error = || LinkageError {
                    code: "E2943",
                    message: "invalid-or-conflicting-symbol-attribute",
                    span: attribute.span,
                };
                let [argument] = attribute.arguments.as_slice() else {
                    return Err(error());
                };
                let inner = argument
                    .text
                    .strip_prefix('"')
                    .and_then(|text| text.strip_suffix('"'))
                    .ok_or_else(error)?;
                let value = StringLit::single(inner, StringPrefix::None)
                    .decoded_units()
                    .ok_or_else(error)?
                    .into_iter()
                    .map(char::from_u32)
                    .collect::<Option<String>>()
                    .ok_or_else(error)?;
                if value.is_empty() || value.chars().any(char::is_control) {
                    return Err(error());
                }
                if is_visibility {
                    let hidden = match value.as_str() {
                        "default" => false,
                        "hidden" => true,
                        _ => return Err(error()),
                    };
                    if result
                        .hidden
                        .insert(symbol, hidden)
                        .is_some_and(|old| old != hidden)
                    {
                        return Err(error());
                    }
                } else if result
                    .object_sections
                    .insert(symbol, value.clone())
                    .is_some_and(|old| old != value)
                {
                    return Err(error());
                }
            }
            let alias = attributes
                .iter()
                .find(|a| a.semantic_name() == Some("alias"));
            if file_scope && defined && alias.is_none() {
                definitions.insert(symbol);
            }
            if attributes.iter().any(|a| a.semantic_name() == Some("weak")) {
                if storage == StorageClass::Static
                    || unit
                        .symbols()
                        .symbol(symbol)
                        .is_ok_and(|s| s.storage_class() == Some(StorageClass::Static))
                {
                    return Err(LinkageError {
                        code: "E3060",
                        message: "weak-internal-symbol",
                        span: declaration.span(),
                    });
                }
                result.weak.insert(symbol);
            }
            if let Some(alias) = alias {
                let error = || LinkageError {
                    code: "E3061",
                    message: "alias-target-not-defined",
                    span: alias.span,
                };
                let [argument] = alias.arguments.as_slice() else {
                    return Err(error());
                };
                let inner = argument
                    .text
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .ok_or_else(error)?;
                let units = StringLit::single(inner, StringPrefix::None)
                    .decoded_units()
                    .ok_or_else(error)?;
                let target = units
                    .into_iter()
                    .map(char::from_u32)
                    .collect::<Option<String>>()
                    .ok_or_else(error)?;
                if let Some((previous, _)) =
                    written_aliases.insert(symbol, (target.clone(), alias.span))
                {
                    if previous != target {
                        return Err(error());
                    }
                }
            }
        }
        for (&symbol, (_, span)) in &written_aliases {
            if definitions.contains(&symbol) {
                return Err(LinkageError {
                    code: "E3041",
                    message: "symbol-declared-and-defined",
                    span: *span,
                });
            }
            let mut current = symbol;
            let mut visited = BTreeSet::new();
            while let Some((name, _)) = written_aliases.get(&current) {
                let error = || LinkageError {
                    code: "E3061",
                    message: "alias-target-not-defined",
                    span: *span,
                };
                if !visited.insert(current) {
                    return Err(error());
                }
                current = *names.get(name).ok_or_else(error)?;
            }
            if !definitions.contains(&current) {
                return Err(LinkageError {
                    code: "E3061",
                    message: "alias-target-not-defined",
                    span: *span,
                });
            }
            result.aliases.insert(symbol, current);
        }
        // An assembler label changes linker identity, not the C identifier.
        // Resolve references to a same-unit body once, without making this
        // identity relation an emitted ELF alias (which would alias itself).
        let mut assembly_definitions = BTreeMap::new();
        for (&symbol, function) in &function_declarations {
            if function.body.is_some() {
                let name = result
                    .function_name(symbol, &function.name.spelling)
                    .to_owned();
                if assembly_definitions.insert(name, symbol).is_some() {
                    return Err(LinkageError {
                        code: "E3041",
                        message: "symbol-declared-and-defined",
                        span: function.span,
                    });
                }
            }
        }
        for (&symbol, function) in &function_declarations {
            let name = result.function_name(symbol, &function.name.spelling);
            if let Some(&target) = assembly_definitions.get(name) {
                if !unit
                    .types()
                    .compatible(function.ty, function_declarations[&target].ty)
                {
                    return Err(LinkageError {
                        code: "E2002",
                        message: "incompatible-redeclaration",
                        span: function.span,
                    });
                }
                if target != symbol {
                    result.assembly_targets.insert(symbol, target);
                }
            }
        }
        Ok(result)
    }
}
