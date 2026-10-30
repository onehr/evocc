//! Parser functions.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn complete_identifier_parameters(
        &mut self,
        declarator: &mut ParsedDeclarator<'arena>,
    ) -> Option<()> {
        let Some(DerivedDeclarator::Function {
            parameters: ParameterSyntax::IdentifierList { parameters },
            scope,
            ..
        }) = declarator.derived.first_mut()
        else {
            self.report("E1001", "expected-declaration", declarator.span);
            return None;
        };
        let outer_scope = self.current_scope;
        self.current_scope = *scope;
        let result = (|| {
            let mut declared = std::collections::HashSet::new();
            while self.peek_kind() != Some(TokenKind::LBrace) {
                let storage = self.parse_storage_class();
                if !matches!(storage, StorageClass::None | StorageClass::Register) {
                    self.report("E1001", "expected-declaration", self.span());
                    return None;
                }
                let mut attributes =
                    self.parse_attribute_specifiers(AttributePlacement::DeclarationSpecifiers);
                let Some(specifier) =
                    self.parse_type_specifier_inner(None, None, Some(&mut attributes), true)
                else {
                    self.report("E1001", "expected-declaration", self.span());
                    return None;
                };
                loop {
                    let declaration = self.parse_declarator(false)?;
                    let Some(parameter) = parameters.iter_mut().find(|parameter| {
                        parameter
                            .name
                            .as_ref()
                            .is_some_and(|name| name.spelling == declaration.name.spelling)
                    }) else {
                        self.report("E1001", "expected-declaration", declaration.name.span);
                        return None;
                    };
                    if !declared.insert(declaration.name.spelling.clone()) {
                        self.report("E2002", "incompatible-redeclaration", declaration.name.span);
                        return None;
                    }
                    self.validate_variable_array_bounds(&declaration.derived, false);
                    let unadjusted =
                        self.apply_declarator(specifier.surface.type_id(), &declaration.derived);
                    let ty = self.adjust_parameter_type(unadjusted, &declaration.derived);
                    let mut parameter_attributes = attributes.clone();
                    parameter_attributes.extend(declaration.attributes.iter().cloned());
                    parameter_attributes
                        .extend(self.parse_attribute_specifiers(AttributePlacement::Parameter));
                    self.settle_convention_attributes(&parameter_attributes, Settlement::Other);
                    let ty =
                        self.apply_declaration_attributes(ty, &mut parameter_attributes, false);
                    parameter.ty = ty;
                    parameter.storage = storage;
                    parameter.specifier = specifier.surface.clone();
                    parameter.written = self.written_declarator(&declaration);
                    parameter.attributes = parameter_attributes;
                    parameter.span = self.merge_span(specifier.surface.span(), declaration.span);
                    if !self.match_kind(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::Semicolon)?;
                if let Some(event) = specifier.tag_event {
                    let declaration =
                        self.finish_tag_event(event, specifier.surface.span(), Vec::new());
                    self.side_declarations.push(declaration);
                }
            }
            if !self.legacy_declarations && declared.len() != parameters.len() {
                self.report("E1001", "expected-declaration", self.span());
                return None;
            }
            Some(())
        })();
        self.current_scope = outer_scope;
        result
    }

    pub(super) fn bind_parameters(&mut self, syntax: &mut ParameterSyntax) {
        let (ParameterSyntax::Prototype { parameters, .. }
        | ParameterSyntax::IdentifierList { parameters }) = syntax
        else {
            return;
        };
        for parameter in parameters {
            let Some(name) = parameter.name.as_ref() else {
                parameter.symbol = None;
                continue;
            };
            let data = SymbolData::Parameter {
                storage: parameter.storage,
            };
            let symbol = self
                .symbol_in_current_scope(&name.spelling)
                .filter(|symbol| {
                    self.symbols.symbol(*symbol).is_ok_and(|symbol| {
                        symbol.data() == data
                            && symbol.ty() == parameter.ty
                            && symbol.span() == name.span
                    })
                })
                .unwrap_or_else(|| self.declare_symbol(name, data, parameter.ty));
            self.add_entity_attributes(symbol, &parameter.attributes);
            parameter.symbol = Some(symbol);
        }
    }

    pub(super) fn parse_function_body(
        &mut self,
        function_name: &SpannedName,
    ) -> &'arena CHirStmt<'arena> {
        let outer_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let outer_switch_depth = std::mem::replace(&mut self.switch_depth, 0);
        let implicit_function_name = self
            .function_body_mentions_implicit_name()
            .then(|| self.declare_implicit_function_name(function_name));
        // The function's outer compound statement and its parameters share
        // one function scope. Nested compounds create block scopes normally.
        let parsed_body = self.parse_compound_statement(false);
        self.loop_depth = outer_loop_depth;
        self.switch_depth = outer_switch_depth;
        // EvoIR 10.1.21: a read of an `_Atomic` object outside the admitted
        // scalars is refused where it is written, as an update is, and an
        // evaluated member access of an atomic record or union is an error.
        for finding in evocc_chir::atomic_access_findings(
            parsed_body,
            &self.symbols,
            &self.types,
            &self.target_layout,
        ) {
            match finding {
                evocc_chir::AtomicFinding::UnadmittedRead(span) => {
                    self.report("E2702", "atomic-update-unsupported", span);
                }
                evocc_chir::AtomicFinding::MemberAccess(span) => {
                    self.report("E2956", "atomic-member-access", span);
                }
            }
        }
        let Some((_, function_name_declaration)) = implicit_function_name else {
            return parsed_body;
        };
        let CHirStmt::Compound(items, span, fenv) = parsed_body else {
            unreachable!("a function body is one compound statement")
        };
        let mut with_function_name = Vec::with_capacity(items.len() + 1);
        with_function_name.push(CHirBlockItem::Declaration(function_name_declaration));
        with_function_name.extend(items.iter().cloned());
        self.alloc_node(CHirStmt::Compound(with_function_name, *span, *fenv))
    }

    pub(super) fn prepend_parameter_bounds(
        &self,
        body: &'arena CHirStmt<'arena>,
        declarator: &ParsedDeclarator<'arena>,
    ) -> &'arena CHirStmt<'arena> {
        let Some(DerivedDeclarator::Function { entry_bounds, .. }) = declarator.derived.first()
        else {
            return body;
        };
        if entry_bounds.is_empty() {
            return body;
        }
        let CHirStmt::Compound(items, span, fenv) = body else {
            unreachable!("a function definition has a compound body")
        };
        // Array adjustment removes only the parameter's outer array type.
        // Capture all written sizes at entry, before the body can change the
        // parameters; inner dimensions keep those identities in their types.
        let mut with_bounds = Vec::with_capacity(entry_bounds.len() + items.len());
        with_bounds.extend(entry_bounds.iter().copied().map(CHirBlockItem::Declaration));
        with_bounds.extend(items.iter().cloned());
        self.alloc_node(CHirStmt::Compound(with_bounds, *span, *fenv))
    }

    pub(super) fn function_body_mentions_implicit_name(&self) -> bool {
        let mut brace_depth = 0_u32;
        for token in &self.tokens[self.pos..] {
            match token.kind {
                TokenKind::LBrace => brace_depth = brace_depth.saturating_add(1),
                TokenKind::RBrace => {
                    if brace_depth == 0 {
                        return false;
                    }
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        return false;
                    }
                }
                TokenKind::Identifier
                    if brace_depth > 0
                        && implicit_function_name(self.token_text(token)).is_some() =>
                {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    pub(super) fn declare_implicit_function_name(
        &mut self,
        function_name: &SpannedName,
    ) -> (SymbolId, &'arena CHirDecl<'arena>) {
        let literal = StringLit::single(function_name.spelling.clone(), StringPrefix::None);
        let bound = literal
            .explicit_unit_count()
            .and_then(|count| count.checked_add(1))
            .expect("a C identifier has a representable narrow string length");
        let character = self.intern(
            TypeKind::Int(IntRank::Char),
            Qualifiers {
                is_const: true,
                ..Qualifiers::default()
            },
        );
        let array = self.intern(
            TypeKind::Array {
                element: character,
                bound: ArrayBound::Fixed(bound),
            },
            Qualifiers::default(),
        );
        let name = SpannedName {
            spelling: "__func__".to_owned(),
            span: function_name.span,
        };
        let symbol = self.declare_symbol(
            &name,
            SymbolData::Object {
                storage: StorageClass::Static,
            },
            array,
        );
        self.mark_definition(symbol, &name);
        let value = self.alloc_node(CHirExpr::StringLit(literal.clone(), function_name.span));
        let initializer = ObjectInitializer::new(
            WrittenInitializer::Expression(value),
            InitPlan::new(
                array,
                vec![InitAction::store_string(
                    SubobjectPath::root(),
                    array,
                    literal,
                    function_name.span,
                )],
            ),
        );
        let declaration = self.alloc_node(CHirDecl::Object(ObjectDecl {
            symbol,
            name,
            ty: array,
            specifier: TypeSpecifier::Builtin {
                ty: character,
                span: function_name.span,
            },
            written: WrittenDeclarator {
                layers: vec![WrittenDeclaratorLayer::Array {
                    bound: ArrayBound::Fixed(bound),
                    parameter_qualifiers: Qualifiers::default(),
                    static_bound: false,
                    span: function_name.span,
                }],
                span: function_name.span,
            },
            storage: StorageClass::Static,
            initializer: Some(initializer),
            cleanup: None,
            asm_label: None,
            asm_label_span: None,
            gnu_extension_spans: Vec::new(),
            attributes: Vec::new(),
            span: function_name.span,
        }));
        (symbol, declaration)
    }

    pub(super) fn in_function_scope(&self) -> bool {
        let mut scope = Some(self.current_scope);
        while let Some(id) = scope {
            let current = self
                .symbols
                .scope(id)
                .expect("the parser current scope chain is always live");
            if current.kind() == ScopeKind::Function {
                return true;
            }
            scope = current.parent();
        }
        false
    }

    pub(super) fn parse_required_constant_conditional(
        &mut self,
    ) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_conditional()
    }

    pub(super) fn parse_required_constant_assignment(
        &mut self,
    ) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_assignment()
    }

    pub(super) fn validate_asm_goto_labels(&mut self, body: &CHirStmt<'_>) -> Option<()> {
        let mut defined = HashSet::new();
        let mut referenced = Vec::new();
        collect_asm_goto_labels(body, &mut defined, &mut referenced);
        if let Some(label) = referenced
            .into_iter()
            .find(|label| !defined.contains(&label.spelling))
        {
            self.report("E2914", "malformed-asm", label.span);
            return None;
        }
        Some(())
    }

    pub(super) fn validate_statement_expression_jumps(
        &mut self,
        body: &CHirStmt<'_>,
    ) -> Option<()> {
        let mut labels = HashMap::new();
        let mut gotos = Vec::new();
        let mut regions = Vec::new();
        let is_variable = |ty| self.type_is_variably_modified(ty);
        let mut next_region = JumpRegionContext {
            is_variable: &is_variable,
            next: 0,
            variable: HashSet::new(),
            switches: Vec::new(),
            invalid_switch_entries: Vec::new(),
            indirect: Vec::new(),
        };
        collect_jump_regions_stmt(
            body,
            &mut regions,
            &mut next_region,
            &mut labels,
            &mut gotos,
        );
        let address_taken = gotos
            .iter()
            .filter(|reference| !reference.checks_region_entry)
            .map(|reference| reference.name.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for (span, regions) in &next_region.indirect {
            gotos.extend(address_taken.iter().map(|name| JumpRegionReference {
                name: name.clone(),
                span: *span,
                regions: regions.clone(),
                checks_region_entry: true,
            }));
        }
        if let Some(span) = next_region.invalid_switch_entries.first().copied() {
            self.report("E2945", "jump-into-variable-type-scope", span);
            return None;
        }
        if let Some(reference) = gotos
            .iter()
            .find(|reference| !labels.contains_key(&reference.name))
        {
            self.report("E2920", "undeclared-label", reference.span);
            return None;
        }
        if let Some(reference) = gotos.into_iter().find(|reference| {
            reference.checks_region_entry
                && labels
                    .get(&reference.name)
                    .is_some_and(|target| !reference.regions.starts_with(target))
        }) {
            if labels[&reference.name].iter().any(|region| {
                next_region.variable.contains(region) && !reference.regions.contains(region)
            }) {
                self.report("E2945", "jump-into-variable-type-scope", reference.span);
                return None;
            }
            self.report("E2918", "jump-into-statement-expression", reference.span);
            return None;
        }
        Some(())
    }

    /// Does a function body follow the current position once any attribute
    /// specifiers are skipped? Decides the placement label of those attributes
    /// without consuming them.
    pub(super) fn declarator_is_followed_by_body(&self) -> bool {
        let mut index = self.pos;
        while self.attribute_starts_at(index) {
            let Some(end) = self.attribute_specifier_end(index) else {
                return false;
            };
            index = end;
        }
        self.tokens.get(index).map(|token| token.kind) == Some(TokenKind::LBrace)
    }

    pub(super) fn declaration_end(&mut self, fallback: SourceSpan) -> SourceSpan {
        if self.peek_kind() == Some(TokenKind::Semicolon) {
            return self.advance().expect("peeked semicolon").span;
        }
        let span = self.span();
        if self.peek_kind() == Some(TokenKind::Identifier) {
            self.report("E1004", "trailing-token", span);
        } else {
            self.report("E1002", "expected-token: ;", span);
        }
        fallback
    }

    pub(super) fn finish_tag_event(
        &self,
        event: PendingTag<'arena>,
        declaration_span: SourceSpan,
        gnu_extension_spans: Vec<SourceSpan>,
    ) -> &'arena CHirDecl<'arena> {
        self.alloc_node(CHirDecl::Tag(TagDecl {
            tag: event.tag,
            scope: event.scope,
            name: event.name,
            members: event.members,
            span: declaration_span,
            specifier_span: event.specifier_span,
            gnu_extension_spans,
            attributes: event.attributes,
        }))
    }

    // ── type specifier ─────────────────────────────────────────────────
}
