//! Symbol linkage, resolved once from the declaration inventory of a unit.

use crate::{CHirDecl, CHirUnit, StorageClass, SymbolId};
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
    pub patchable_entries: BTreeMap<SymbolId, (u16, u16)>,
    pub weak: BTreeSet<SymbolId>,
    pub aliases: BTreeMap<SymbolId, SymbolId>,
    /// Effective ELF visibility, independent of C linkage and symbol strength.
    /// An explicit default is retained to detect later conflicts.
    pub visibility: BTreeMap<SymbolId, SymbolVisibility>,
    pub object_sections: BTreeMap<SymbolId, String>,
    pub function_sections: BTreeMap<SymbolId, String>,
    pub tls_models: BTreeMap<SymbolId, evocc_types::TlsModel>,
    function_names: BTreeMap<SymbolId, String>,
    assembly_targets: BTreeMap<SymbolId, SymbolId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolVisibility {
    Default,
    Hidden,
    Protected,
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
        let roles =
            crate::function_definitions::FunctionDefinitions::derive(unit).map_err(|e| {
                LinkageError {
                    code: "E2002",
                    message: "incompatible-redeclaration",
                    span: e.duplicate,
                }
            })?;
        let mut result = Self {
            patchable_entries: crate::function_definitions::patchable_entries(unit).map_err(
                |span| LinkageError {
                    code: "E2943",
                    message: "invalid-or-conflicting-symbol-attribute",
                    span,
                },
            )?,
            ..Self::default()
        };
        let mut names = BTreeMap::new();
        let mut definitions = BTreeSet::new();
        let mut written_aliases = BTreeMap::new();
        let mut function_declarations = BTreeMap::new();
        let mut section_positions = BTreeMap::new();
        let mut definition_positions = BTreeMap::new();
        // A symbol retains the scope of its first declaration. A later
        // file-scope definition can therefore have a block-scoped identity.
        let file_declarations = unit
            .iter()
            .map(|d| d as *const CHirDecl<'_>)
            .collect::<std::collections::HashSet<_>>();
        let mut declarations = Vec::new();
        let mut defined_symbols = BTreeSet::new();
        let mut internal_symbols = BTreeSet::new();
        crate::inline::for_each_declaration_ordered(unit, |d| declarations.push(d));
        for (position, declaration) in declarations.into_iter().enumerate() {
            if let CHirDecl::Function(function) = declaration {
                function_declarations
                    .entry(function.symbol)
                    .and_modify(|old: &mut &crate::FunctionDecl<'_>| {
                        if roles.is_definition(function) {
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
                    roles.is_definition(f),
                ),
                CHirDecl::Object(g) => (
                    g.symbol,
                    &g.name.spelling,
                    g.storage,
                    &g.attributes,
                    g.storage.base() != StorageClass::Extern,
                ),
                _ => continue,
            };
            if defined {
                defined_symbols.insert(symbol);
            }
            if storage.base() == StorageClass::Static {
                internal_symbols.insert(symbol);
            }
            let file_scope = file_declarations.contains(&(declaration as *const CHirDecl<'_>));
            for attribute in attributes
                .iter()
                .filter(|attribute| attribute.semantic_name() == Some("tls_model"))
            {
                let error = || LinkageError {
                    code: "E2943",
                    message: "invalid-or-conflicting-symbol-attribute",
                    span: attribute.span,
                };
                let [argument] = attribute.arguments.as_slice() else {
                    return Err(error());
                };
                let model = argument
                    .string_literal()
                    .and_then(|value| evocc_types::TlsModel::parse(&value.value))
                    .ok_or_else(error)?;
                if !matches!(declaration, CHirDecl::Object(_))
                    || !storage.is_thread_local()
                    || result
                        .tls_models
                        .insert(symbol, model)
                        .is_some_and(|old| old != model)
                {
                    return Err(error());
                }
            }
            if file_scope {
                names.insert(name.clone(), symbol);
            }
            for attribute in attributes {
                let is_visibility = attribute.semantic_name() == Some("visibility");
                let is_section = attribute.semantic_name() == Some("section");
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
                if is_section
                    && matches!(declaration, CHirDecl::Function(_))
                    && has_numeric_escape(&argument.text)
                {
                    return Err(error());
                }
                let bytes = argument
                    .string_literal()
                    .ok_or_else(error)?
                    .decoded_units()
                    .ok_or_else(error)?
                    .into_iter()
                    .map(u8::try_from)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| error())?;
                // Narrow execution units are UTF-8 bytes, not individual
                // Unicode scalar values to encode a second time.
                let value = String::from_utf8(bytes).map_err(|_| error())?;
                if value.is_empty() || value.chars().any(char::is_control) {
                    return Err(error());
                }
                if is_visibility {
                    let visibility = match value.as_str() {
                        "default" => SymbolVisibility::Default,
                        // The target profile normalizes the source spelling,
                        // not the C linkage; the Attribute retains its text.
                        "hidden" | "internal" => SymbolVisibility::Hidden,
                        "protected" => SymbolVisibility::Protected,
                        _ => return Err(error()),
                    };
                    if result
                        .visibility
                        .insert(symbol, visibility)
                        .is_some_and(|old| old != visibility)
                    {
                        return Err(error());
                    }
                } else {
                    // GCC applies a first section request after a definition,
                    // but Clang ignores it. Do not silently choose a layout.
                    if matches!(declaration, CHirDecl::Function(_))
                        && definitions.contains(&symbol)
                        && !result.function_sections.contains_key(&symbol)
                    {
                        return Err(error());
                    }
                    if matches!(declaration, CHirDecl::Function(_)) {
                        section_positions.entry(symbol).or_insert(position);
                    }
                    let sections = if matches!(declaration, CHirDecl::Function(_)) {
                        &mut result.function_sections
                    } else {
                        &mut result.object_sections
                    };
                    if sections
                        .insert(symbol, value.clone())
                        .is_some_and(|old| old != value)
                    {
                        return Err(error());
                    }
                }
            }
            let alias = attributes
                .iter()
                .find(|a| a.semantic_name() == Some("alias"));
            if file_scope && defined && alias.is_none() {
                definitions.insert(symbol);
                definition_positions.insert(symbol, position);
            }
            if attributes.iter().any(|a| a.semantic_name() == Some("weak")) {
                if storage.base() == StorageClass::Static
                    || unit.symbols().symbol(symbol).is_ok_and(|s| {
                        s.storage_class().map(StorageClass::base) == Some(StorageClass::Static)
                    })
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
                let units = argument
                    .string_literal()
                    .ok_or_else(error)?
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
            if roles.is_definition(function) {
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
        // Distinct C identifiers can name one assembler-level function. Its
        // placement is merged with its definition, not guessed by emission.
        for (&symbol, &target) in &result.assembly_targets {
            if let Some(section) = result.function_sections.get(&symbol).cloned() {
                if !result.function_sections.contains_key(&target)
                    && section_positions.get(&symbol) > definition_positions.get(&target)
                {
                    return Err(LinkageError {
                        code: "E2943",
                        message: "invalid-or-conflicting-symbol-attribute",
                        span: function_declarations[&symbol].span,
                    });
                }
                if result
                    .function_sections
                    .insert(target, section.clone())
                    .is_some_and(|old| old != section)
                {
                    return Err(LinkageError {
                        code: "E2943",
                        message: "invalid-or-conflicting-symbol-attribute",
                        span: function_declarations[&symbol].span,
                    });
                }
            }
        }
        for (&symbol, &target) in &result.aliases {
            if let Some(section) = result.function_sections.get(&symbol) {
                if section
                    != result
                        .function_sections
                        .get(&target)
                        .map_or(".text", String::as_str)
                {
                    return Err(LinkageError {
                        code: "E2943",
                        message: "invalid-or-conflicting-symbol-attribute",
                        span: written_aliases[&symbol].1,
                    });
                }
            }
        }
        // EvoIR 10.1.9: a function or object declaration that the macOS
        // deployment target makes a weak import is a weak reference, unless
        // the unit defines it or gives it internal linkage, as in Apple clang.
        for &symbol in unit.weak_imports() {
            if !defined_symbols.contains(&symbol) && !internal_symbols.contains(&symbol) {
                result.weak.insert(symbol);
            }
        }
        Ok(result)
    }
}

fn has_numeric_escape(spelling: &str) -> bool {
    let mut characters = spelling.chars();
    while let Some(character) = characters.next() {
        if character == '\\'
            && characters
                .next()
                .is_some_and(|escaped| escaped == 'x' || ('0'..='7').contains(&escaped))
        {
            return true;
        }
    }
    false
}
