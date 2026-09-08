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
}

impl SymbolLinkage {
    pub fn target(&self, symbol: SymbolId) -> SymbolId {
        self.aliases.get(&symbol).copied().unwrap_or(symbol)
    }

    pub fn derive(unit: &CHirUnit<'_>) -> Result<Self, LinkageError> {
        let mut result = Self::default();
        let mut names = BTreeMap::new();
        let mut definitions = BTreeSet::new();
        let mut written_aliases = BTreeMap::new();
        // A symbol retains the scope of its first declaration. A later
        // file-scope definition can therefore have a block-scoped identity.
        let file_declarations = unit
            .iter()
            .map(|d| d as *const CHirDecl<'_>)
            .collect::<std::collections::HashSet<_>>();
        let mut declarations = Vec::new();
        crate::inline::for_each_declaration(unit, |d| declarations.push(d));
        for declaration in declarations {
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
            let alias = attributes.iter().find(|a| a.name == "alias");
            if file_scope && defined && alias.is_none() {
                definitions.insert(symbol);
            }
            if attributes.iter().any(|a| a.name == "weak") {
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
        Ok(result)
    }
}
