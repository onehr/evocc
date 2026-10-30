//! C-HIR interpreter symbols.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn symbol_order(&self) -> &SymbolOrder {
        self.symbol_order.get_or_init(|| {
            let mut symbols = self
                .unit
                .iter()
                .filter_map(|declaration| match declaration {
                    CHirDecl::Function(function) if function.body.is_some() => {
                        Some(function.symbol)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let defined = symbols.iter().copied().collect::<BTreeSet<_>>();
            let mut referenced = ReferencedSymbols::default();
            for declaration in self.unit.iter() {
                match declaration {
                    CHirDecl::Function(function) => {
                        if let Some(body) = function.body {
                            collect_statement_symbols(body, &mut referenced);
                        }
                    }
                    CHirDecl::Object(object) => {
                        if let Some(initializer) = &object.initializer {
                            for action in initializer.plan().actions() {
                                if let InitAction::StoreScalar { value, .. }
                                | InitAction::CopyAggregate { value, .. } = action
                                {
                                    collect_expression_symbols(value, &mut referenced);
                                }
                            }
                        }
                    }
                    CHirDecl::Asm(_) => collect_declaration_symbols(declaration, &mut referenced),
                    CHirDecl::Tag(_) | CHirDecl::Typedef(_) => {}
                }
            }
            let mut functions = BTreeMap::new();
            let mut objects = BTreeSet::new();
            for symbol in referenced.named {
                let symbol = self
                    .linkage
                    .target(linked_library_definition(self.unit, symbol));
                let Ok(entry) = self.unit.symbols().symbol(symbol) else {
                    continue;
                };
                match entry.kind() {
                    SymbolKind::Function if !defined.contains(&symbol) => {
                        functions.insert(entry.spelling().to_owned(), symbol);
                    }
                    SymbolKind::Object
                        if entry.storage_class().map(StorageClass::base)
                            == Some(StorageClass::Extern)
                            && !self
                                .manifest
                                .as_ref()
                                .is_some_and(|manifest| manifest.object_site(symbol).is_some()) =>
                    {
                        objects.insert(entry.spelling().to_owned());
                    }
                    _ => {}
                }
            }
            symbols.extend(functions.into_values());
            let mut labels = BTreeMap::new();
            let mut identities = BTreeMap::new();
            for declaration in self.unit.iter() {
                if let CHirDecl::Function(function) = declaration {
                    if let Some(body) = function.body {
                        let mut references = ReferencedSymbols::default();
                        collect_statement_symbols(body, &mut references);
                        for (node, label) in references.labels {
                            let next = symbols.len() as u64 + identities.len() as u64 + 1;
                            let identity = *identities
                                .entry((function.symbol, label.clone()))
                                .or_insert(next);
                            labels.insert(node, (function.symbol, label, identity));
                        }
                    }
                }
            }
            let code_identities = symbols.len() as u64 + identities.len() as u64;
            SymbolOrder {
                labels,
                code_identities,
                functions: symbols,
                objects: objects
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, name)| (name, u64::MAX - ordinal as u64))
                    .collect(),
            }
        })
    }

    pub(super) fn function_symbol(&self, identity: u64) -> Option<SymbolId> {
        let index = identity
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())?;
        self.symbol_order().functions.get(index).copied()
    }
}
