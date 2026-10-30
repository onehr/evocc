//! Parser declarations.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_external_declaration(&mut self) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        if self.match_kind(TokenKind::Semicolon) {
            return Some(Vec::new());
        }
        if self.peek_kind() == Some(TokenKind::StaticAssert) {
            self.parse_static_assert_declaration()?;
            return Some(Vec::new());
        }
        if self.peek_kind() == Some(TokenKind::Asm) {
            let mut assembly = self.parse_asm_construct()?;
            let semicolon = self.expect(TokenKind::Semicolon)?;
            assembly.span = self.merge_span(assembly.span, semicolon.span);
            return Some(vec![self.alloc_node(CHirDecl::Asm(assembly))]);
        }
        self.parse_declaration(DeclarationContext::File)
    }

    pub(super) fn parse_declaration(
        &mut self,
        context: DeclarationContext,
    ) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        let scope_checkpoint = self.current_scope;
        let types_transaction = self.types.begin_transaction();
        let symbols_transaction = self.symbols.begin_transaction();
        let declaration_transaction = self.begin_declaration_transaction();
        let generic_selection_checkpoint = self.generic_selections.len();
        let front_end_builtin_checkpoint = self.front_end_builtins.len();
        let rollback_error_checkpoint = self.rollback_error_count;
        let side_start = declaration_transaction.side_declarations;
        let availability_depth = self.availability_frames.len();

        let result = self.parse_declaration_inner(context, side_start);
        if result.is_some() {
            self.pop_availability_frames(availability_depth);
        } else {
            // A failed declaration has reported its error already.
            self.availability_frames.truncate(availability_depth);
        }
        if result.is_none() || self.rollback_error_count != rollback_error_checkpoint {
            self.types
                .rollback_transaction(&types_transaction)
                .expect("parser type transactions are table-local and LIFO");
            self.symbols
                .rollback_transaction(&symbols_transaction)
                .expect("parser declaration transactions are table-local and LIFO");
            self.current_scope = scope_checkpoint;
            self.rollback_declaration_transaction(&declaration_transaction);
            self.generic_selections
                .truncate(generic_selection_checkpoint);
            self.front_end_builtins
                .truncate(front_end_builtin_checkpoint);
            None
        } else {
            self.types
                .commit_transaction(&types_transaction)
                .expect("parser type transactions are table-local and LIFO");
            self.symbols
                .commit_transaction(&symbols_transaction)
                .expect("parser declaration transactions are table-local and LIFO");
            self.commit_declaration_transaction(&declaration_transaction);
            result
        }
    }

    pub(super) fn report_internal_linkage_attributes(
        &mut self,
        attributes: &[Attribute],
        storage: StorageClass,
    ) {
        if !self.target_layout.names_clang_extensions() || storage.base() == StorageClass::Static {
            return;
        }
        for attribute in attributes {
            if attribute.semantic_name() == Some("internal_linkage") {
                self.report("E2967", "unsupported-darwin-attribute", attribute.span);
            }
        }
    }

    pub(super) fn parse_declaration_inner(
        &mut self,
        context: DeclarationContext,
        side_start: usize,
    ) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        let start_span = self.span();
        let visibility = self.visibility_stack.last().copied();
        // EvoIR 10.1.9: at file scope a use in the declaration specifiers
        // takes the context of the first declarator, as in Apple clang. In a
        // block it takes the context of the enclosing function.
        let file_scope = context == DeclarationContext::File;
        let declaration_frame = self.push_availability_frame(
            (!file_scope).then_some(false),
            self.innermost_availability_frame(),
        );
        let gnu_extension_spans = self.parse_extension_prefix();
        let mut storage = None;
        let mut inline_spans = Vec::new();
        let mut attributes = Vec::new();
        self.parse_declaration_modifiers(&mut storage, &mut inline_spans, &mut attributes);
        let Some(specifier) =
            self.parse_declaration_type_specifier(&mut storage, &mut inline_spans, &mut attributes)
        else {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        };
        self.parse_declaration_modifiers(&mut storage, &mut inline_spans, &mut attributes);
        let storage = storage.unwrap_or(StorageClass::None);
        let auto_type_span = match &specifier.surface {
            TypeSpecifier::AutoType { span, .. } => Some(*span),
            _ => None,
        };

        if self.peek_kind() == Some(TokenKind::Semicolon) {
            let semi = self.advance().expect("peeked semicolon");
            if let Some(attribute) = attributes.iter().find(|a| a.written == "_Noreturn") {
                self.report(
                    "E2934",
                    "function-specifier-on-non-function",
                    attribute.span,
                );
                return None;
            }
            if auto_type_span.is_some() {
                self.report("E2915", "auto-type-misuse", semi.span);
                return None;
            }
            let declaration_span = self.merge_span(start_span, semi.span);
            if let Some(span) = inline_spans.first().copied() {
                self.report("E1401", "inline-specifier-on-non-function", span);
                return None;
            }
            let mut declarations = self.side_declarations.split_off(side_start);
            self.settle_convention_attributes(&attributes, Settlement::Other);
            if let Some(event) = specifier.tag_event {
                self.detached_attributes.extend(attributes);
                declarations.push(self.finish_tag_event(
                    event,
                    declaration_span,
                    gnu_extension_spans,
                ));
            } else {
                self.report("E1001", "expected-declaration", semi.span);
            }
            return Some(declarations);
        }

        let mut declarator_frame = if file_scope {
            declaration_frame
        } else {
            self.push_availability_frame(None, Some(declaration_frame))
        };
        let Some(mut declarator) = self.parse_declarator(true) else {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        };
        self.validate_array_parameter_modifiers(&declarator.derived, false);
        if declarator.derived.iter().any(|layer| {
            matches!(
                layer,
                DerivedDeclarator::Function {
                    parameters: ParameterSyntax::IdentifierList { .. },
                    ..
                }
            )
        }) {
            if context != DeclarationContext::File || storage == StorageClass::Typedef {
                self.report("E1001", "expected-declaration", declarator.span);
                return None;
            }
            self.complete_identifier_parameters(&mut declarator)?;
        }
        if auto_type_span.is_some()
            && (storage == StorageClass::Typedef || !declarator.derived.is_empty())
        {
            self.report("E2915", "auto-type-misuse", declarator.span);
            return None;
        }
        let (asm_label, asm_label_span) = self.parse_optional_asm_label()?;
        // Attributes after the declarator attach to it alone. When a function
        // body follows they are in function-definition position; the same
        // tokens, a different row of the placement table.
        attributes.extend(declarator.attributes.iter().cloned());
        let trailing_placement = if self.declarator_is_followed_by_body() {
            AttributePlacement::FunctionDefinition
        } else {
            AttributePlacement::AfterDeclarator
        };
        attributes.extend(self.parse_attribute_specifiers(trailing_placement));
        self.report_internal_linkage_attributes(&attributes, storage);
        let mut full_type = self.apply_declarator(specifier.surface.type_id(), &declarator.derived);
        if let Some(DerivedDeclarator::Function {
            parameters: syntax @ ParameterSyntax::IdentifierList { .. },
            ..
        }) = declarator.derived.first()
        {
            // A prior prototype supplies the call interface when its types
            // match the declared objects or their default promotions (GNU C).
            if let Some(prior) = self
                .symbol_in_current_scope(&declarator.name.spelling)
                .and_then(|id| self.symbols.symbol(id).ok())
            {
                if let (
                    Some(TypeKind::Function {
                        return_type: prior_return,
                        parameters:
                            FunctionParameters::Prototype {
                                types,
                                variadic: false,
                            },
                    }),
                    Some(TypeKind::Function { return_type, .. }),
                ) = (
                    self.types.get(prior.ty()).map(|ty| &ty.kind),
                    self.types.get(full_type).map(|ty| &ty.kind),
                ) {
                    if self.types.compatible(*return_type, *prior_return)
                        && types.len() == syntax.parameters().len()
                        && types
                            .iter()
                            .zip(syntax.parameters())
                            .all(|(ty, parameter)| {
                                syntax.parameter_type_matches(&self.types, *ty, parameter.ty)
                            })
                    {
                        full_type = prior.ty();
                    }
                }
            }
        }
        full_type = self.apply_declaration_attributes(
            full_type,
            &mut attributes,
            storage == StorageClass::Typedef,
        );
        let written = self.written_declarator(&declarator);

        if (context != DeclarationContext::File && storage == StorageClass::ThreadLocal)
            || (context == DeclarationContext::File && storage == StorageClass::Auto)
        {
            self.report_redefinition(&declarator.name);
        }

        if storage != StorageClass::Typedef
            && matches!(
                self.types.get(full_type).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. })
            )
            && self.peek_kind() == Some(TokenKind::LBrace)
        {
            // On Darwin an internal definition settles its list attributes
            // like a prototype, and the definition is refused only when the
            // unit keeps the function (EvoIR 10.1.9).
            let settlement = if self.target_layout.names_clang_extensions()
                && storage.base() == StorageClass::Static
            {
                Settlement::Prototype
            } else {
                Settlement::Definition
            };
            self.settle_convention_attributes(&attributes, settlement);
            let (mut parameters, parameter_scope, inherited_from_specifier) =
                self.function_declaration_parameters(&declarator, &specifier, full_type)?;
            // An unspecified declaration can compose with a prototype, but
            // an empty definition actually defines zero parameters.
            if !inherited_from_specifier
                && matches!(parameters, ParameterSyntax::Unspecified)
                && self
                    .symbol_in_current_scope(&declarator.name.spelling)
                    .and_then(|id| self.symbols.symbol(id).ok())
                    .and_then(|prior| self.types.get(prior.ty()))
                    .is_some_and(|prior| {
                        matches!(
                            &prior.kind,
                            TypeKind::Function {
                                parameters: FunctionParameters::Prototype { types, variadic }, ..
                            } if !types.is_empty() || *variadic
                        )
                    })
            {
                self.report_redefinition(&declarator.name);
            }
            let symbol = self.declare_symbol(
                &declarator.name,
                SymbolData::Function { storage },
                full_type,
            );
            if self.declares_prototype(&declarator, &specifier, full_type) {
                self.record_prototype_scope(symbol);
            }
            if storage != StorageClass::Static {
                self.apply_visibility_default(symbol, &mut attributes, visibility);
            }
            self.add_entity_attributes(symbol, &attributes);
            let unavailable = self.symbol_unavailable(symbol).is_some();
            self.resolve_availability_frame(declarator_frame, unavailable);
            self.availability_frames[declarator_frame].function = true;
            self.record_assembly_name(symbol, &asm_label);
            let has_body = self.peek_kind() == Some(TokenKind::LBrace);
            if has_body && inherited_from_specifier {
                self.report("E1001", "expected-declaration", self.span());
                return None;
            }
            if has_body {
                self.function_definitions.push(FunctionUse {
                    function: symbol,
                    owner: None,
                    span: declarator.name.span,
                    preceding: self.diagnostics.len(),
                });
                // Reject a second ordinary body inside the declaration
                // transaction, preserving rollback and diagnostic encounter
                // order. Final emission still depends on the complete chain.
                if let Some(mut state) = self.entities.get(&symbol).copied() {
                    let gnu_inline_body = storage == StorageClass::Extern
                        && !inline_spans.is_empty()
                        && (self.gnu_inline
                            || attributes
                                .iter()
                                .any(|a| a.semantic_name() == Some("gnu_inline")));
                    if state.has_definition && (!state.gnu_inline_body || gnu_inline_body) {
                        self.report_redefinition(&declarator.name);
                    } else {
                        state.has_definition = true;
                        state.gnu_inline_body = gnu_inline_body;
                        // Set before the body, so that a recursive call
                        // sees the old-style definition.
                        if let Some(promoted) = self.old_style_parameter_types(&parameters) {
                            state.old_style_definition = true;
                            self.old_style_definitions.push((symbol, promoted));
                        }
                        self.set_entity_state(symbol, state);
                    }
                }
                if context == DeclarationContext::File {
                    self.symbols
                        .bind_function_scope(parameter_scope, symbol)
                        .expect("a file-scope definition owns its parameter scope");
                }
            }
            let body = if has_body {
                let open = self.span();
                let outer_scope = self.current_scope;
                self.current_scope = parameter_scope;
                self.bind_parameters(&mut parameters);
                let outer_parameters = self.current_function_parameters.replace(parameters.clone());
                let body = self.parse_function_body(&declarator.name);
                let body = self.prepend_parameter_bounds(body, &declarator);
                self.current_function_parameters = outer_parameters;
                self.validate_asm_goto_labels(body)?;
                self.validate_statement_expression_jumps(body)?;
                self.current_scope = outer_scope;
                if context != DeclarationContext::File {
                    self.report("E1205", "nested-function-definition", open);
                }
                Some(body)
            } else {
                let outer_scope = self.current_scope;
                self.current_scope = parameter_scope;
                self.bind_parameters(&mut parameters);
                self.current_scope = outer_scope;
                None
            };
            self.set_inherited_function_parameters(symbol, parameters.clone());
            let end_span = if let Some(body) = body {
                body.span()
            } else {
                self.declaration_end(declarator.name.span)
            };
            let declaration_span = self.merge_span(start_span, end_span);
            if context == DeclarationContext::ForInitializer {
                self.report(
                    "E1206",
                    "function-declaration-in-for-initializer",
                    declarator.name.span,
                );
                return None;
            }
            if storage == StorageClass::Typedef {
                if let Some(attribute) = attributes.iter().find(|a| a.written == "_Noreturn") {
                    self.report(
                        "E2934",
                        "function-specifier-on-non-function",
                        attribute.span,
                    );
                    return None;
                }
                if let Some(span) = inline_spans.first().copied() {
                    self.report("E1401", "inline-specifier-on-non-function", span);
                    return None;
                }
            }
            let mut declarations = self.side_declarations.split_off(side_start);
            if let Some(event) = specifier.tag_event {
                declarations.push(self.finish_tag_event(
                    event,
                    declaration_span,
                    gnu_extension_spans.clone(),
                ));
            }
            declarations.push(self.alloc_node(CHirDecl::Function(FunctionDecl {
                symbol,
                name: declarator.name,
                ty: full_type,
                return_specifier: specifier.surface,
                written,
                parameters,
                storage,
                asm_label,
                asm_label_span,
                inline_spans,
                gnu_extension_spans,
                attributes,
                body,
                span: declaration_span,
            })));
            return Some(declarations);
        }

        // C11 6.7: one declaration may carry several init-declarators. Each
        // gets its own symbol and initializer in sequence, so a later
        // initializer can refer to an earlier name, and every one receives the
        // attributes written among the specifiers. Attributes written after a
        // particular declarator belong to it alone.
        struct Pending<'arena> {
            declarator: ParsedDeclarator<'arena>,
            captures: Vec<&'arena CHirDecl<'arena>>,
            full_type: TypeId,
            symbol: SymbolId,
            initializer: Option<ObjectInitializer<'arena>>,
            function_parameters: Option<ParameterSyntax>,
            specifier: TypeSpecifier,
            written: WrittenDeclarator,
            attributes: Vec<Attribute>,
            asm_label: Option<String>,
            asm_label_span: Option<SourceSpan>,
        }
        let specifier_attributes: Vec<Attribute> = attributes
            .iter()
            .filter(|attribute| attribute.placement == AttributePlacement::DeclarationSpecifiers)
            .cloned()
            .collect();
        let mut pending: Vec<Pending<'arena>> = Vec::new();
        let mut written = written;
        let mut asm_label = asm_label;
        let mut asm_label_span = asm_label_span;
        loop {
            // An ordinary identifier's scope begins immediately after its
            // declarator, so self-references in an initializer resolve to this
            // declaration rather than an outer shadowed name.
            let has_initializer = self.peek_kind() == Some(TokenKind::Assign);
            let fixed_block_view = context != DeclarationContext::File
                && matches!(
                    storage,
                    StorageClass::None
                        | StorageClass::Auto
                        | StorageClass::Register
                        | StorageClass::Static
                )
                && declarator.derived.iter().any(|layer| {
                    matches!(
                        layer,
                        DerivedDeclarator::Array {
                            bound: ArrayBound::Variable,
                            ..
                        }
                    )
                })
                && (!self.initialized_array_contains_vla(full_type)
                    || (storage != StorageClass::Static && !has_initializer))
                && !matches!(
                    self.types.get(full_type).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                );
            let captures = if context != DeclarationContext::File
                && ((storage == StorageClass::Typedef && !has_initializer) || fixed_block_view)
            {
                let captures = self.capture_array_bounds(&mut declarator.derived)?;
                full_type = self.apply_declarator(specifier.surface.type_id(), &declarator.derived);
                full_type = self.apply_declaration_attributes(
                    full_type,
                    &mut attributes,
                    storage == StorageClass::Typedef,
                );
                written = self.written_declarator(&declarator);
                captures
            } else {
                Vec::new()
            };
            if !has_initializer {
                self.validate_variable_array_bounds(&declarator.derived, false);
                if storage != StorageClass::Typedef
                    && self.initialized_array_contains_vla(full_type)
                    && (context == DeclarationContext::File
                        || !matches!(
                            storage,
                            StorageClass::None | StorageClass::Auto | StorageClass::Register
                        ))
                {
                    self.report("E2203", "variable-length-object", declarator.span);
                }
            }
            if auto_type_span.is_some() && !has_initializer {
                self.report("E2915", "auto-type-misuse", declarator.span);
                return None;
            }
            let is_function = storage != StorageClass::Typedef
                && matches!(
                    self.types.get(full_type).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                );
            if is_function && has_initializer {
                self.report("E2202", "invalid-initializer-type", declarator.span);
                return None;
            }
            self.settle_convention_attributes(
                &attributes,
                if is_function {
                    Settlement::Prototype
                } else {
                    Settlement::Other
                },
            );
            let parsed_function_parameters = if is_function {
                Some(self.function_declaration_parameters(&declarator, &specifier, full_type)?)
            } else {
                None
            };
            let inherited_function_syntax = if storage == StorageClass::Typedef
                && matches!(
                    self.types.get(full_type).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) {
                declarator
                    .derived
                    .iter()
                    .find_map(|layer| match layer {
                        DerivedDeclarator::Function { parameters, .. } => Some(parameters.clone()),
                        _ => None,
                    })
                    .or_else(|| specifier.inherited_function_parameters.clone())
            } else {
                None
            };
            let symbol_data = if storage == StorageClass::Typedef {
                SymbolData::Typedef
            } else if is_function {
                SymbolData::Function { storage }
            } else {
                SymbolData::Object { storage }
            };
            let symbol = self.declare_symbol(&declarator.name, symbol_data, full_type);
            if is_function && self.declares_prototype(&declarator, &specifier, full_type) {
                self.record_prototype_scope(symbol);
            }
            if storage != StorageClass::Typedef
                && storage != StorageClass::Static
                && (is_function
                    || context == DeclarationContext::File
                    || storage == StorageClass::Extern)
            {
                self.apply_visibility_default(symbol, &mut attributes, visibility);
            }
            self.add_entity_attributes(symbol, &attributes);
            let unavailable = self.symbol_unavailable(symbol).is_some();
            self.resolve_availability_frame(declarator_frame, unavailable);
            self.record_assembly_name(symbol, &asm_label);
            full_type = self
                .symbols
                .symbol(symbol)
                .expect("declared symbols belong to the parser symbol table")
                .ty();
            let function_parameters =
                if let Some((mut parameters, parameter_scope, _)) = parsed_function_parameters {
                    let outer_scope = self.current_scope;
                    self.current_scope = parameter_scope;
                    self.bind_parameters(&mut parameters);
                    self.current_scope = outer_scope;
                    self.set_inherited_function_parameters(symbol, parameters.clone());
                    Some(parameters)
                } else {
                    None
                };
            if let Some(parameters) = inherited_function_syntax {
                self.set_inherited_function_parameters(symbol, parameters);
            }
            if has_initializer && storage != StorageClass::Typedef && !is_function {
                self.mark_definition(symbol, &declarator.name);
            }
            let mut declaration_specifier = specifier.surface.clone();
            let initializer = if self.match_kind(TokenKind::Assign) {
                let written_initializer = self.parse_written_initializer()?;
                if auto_type_span.is_some()
                    && written_initializer_references_symbol(&written_initializer, symbol)
                {
                    self.report("E2915", "auto-type-misuse", declarator.span);
                    return None;
                }
                if storage == StorageClass::Typedef
                    || (storage == StorageClass::Extern && context != DeclarationContext::File)
                {
                    self.report(
                        "E2202",
                        "invalid-initializer-type",
                        written_initializer.span(),
                    );
                    return None;
                }
                if let Some(span) = auto_type_span {
                    let base = self.infer_auto_type_base(
                        &written_initializer,
                        &declarator.derived,
                        declarator.span,
                    )?;
                    full_type = self.apply_declarator(base, &declarator.derived);
                    declaration_specifier = TypeSpecifier::AutoType {
                        resolved: base,
                        span,
                    };
                } else {
                    full_type =
                        self.infer_initializer_target_type(full_type, &written_initializer)?;
                }
                self.symbols
                    .update_symbol_type(symbol, full_type)
                    .expect("declared symbols belong to the parser symbol table");
                let mut plan = self.normalize_initializer_plan(
                    full_type,
                    &written_initializer,
                    context == DeclarationContext::File,
                )?;
                if context == DeclarationContext::File
                    && asm_label.is_none()
                    && !storage.is_thread_local()
                    && self.types.is_const_qualified_object(full_type)
                {
                    let previous = self.initialized_strings.insert(symbol, plan.clone());
                    if self.declaration_transaction_depth > 0 {
                        self.initialized_string_undo.push((symbol, previous));
                    }
                }
                if self.initializer_has_static_storage(context, storage) {
                    plan = self.resolve_static_integer_initializers(plan);
                    self.validate_static_initializer_plan(&plan)?;
                }
                self.remember_initialized_const_integer(symbol, full_type, &plan);
                Some(ObjectInitializer::new(written_initializer, plan))
            } else {
                None
            };
            let current_declarator_span = declarator.span;
            if !self.validate_alignment_specifiers(
                full_type,
                &attributes,
                !matches!(storage, StorageClass::Typedef | StorageClass::Register) && !is_function,
            ) {
                return None;
            }
            pending.push(Pending {
                declarator,
                captures,
                full_type,
                symbol,
                initializer,
                function_parameters,
                specifier: declaration_specifier,
                written,
                attributes,
                asm_label,
                asm_label_span,
            });
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
            if auto_type_span.is_some() {
                self.report("E2915", "auto-type-misuse", current_declarator_span);
                return None;
            }
            if declarator_frame != declaration_frame {
                self.pop_availability_frames(declarator_frame);
            }
            declarator_frame = self.push_availability_frame(
                None,
                if file_scope {
                    self.availability_frames[declaration_frame].parent
                } else {
                    Some(declaration_frame)
                },
            );
            let mut next_attributes = specifier_attributes.clone();
            next_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::BeforeDeclarator));
            let Some(next) = self.parse_declarator(false) else {
                self.report("E1001", "expected-declaration", self.span());
                return None;
            };
            self.validate_array_parameter_modifiers(&next.derived, false);
            let (next_asm_label, next_asm_label_span) = self.parse_optional_asm_label()?;
            next_attributes.extend(next.attributes.iter().cloned());
            next_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::AfterDeclarator));
            let mut next_type = self.apply_declarator(specifier.surface.type_id(), &next.derived);
            next_type = self.apply_declaration_attributes(
                next_type,
                &mut next_attributes,
                storage == StorageClass::Typedef,
            );
            written = self.written_declarator(&next);
            declarator = next;
            full_type = next_type;
            attributes = next_attributes;
            asm_label = next_asm_label;
            asm_label_span = next_asm_label_span;
        }

        let last_name_span = pending
            .last()
            .map(|entry| entry.declarator.name.span)
            .expect("at least one declarator was parsed");
        let end_span = self.declaration_end(last_name_span);
        let declaration_span = self.merge_span(start_span, end_span);
        if storage == StorageClass::Typedef
            || pending
                .iter()
                .any(|entry| entry.function_parameters.is_none())
        {
            if let Some(attribute) = pending
                .iter()
                .flat_map(|entry| &entry.attributes)
                .find(|a| a.written == "_Noreturn")
            {
                self.report(
                    "E2934",
                    "function-specifier-on-non-function",
                    attribute.span,
                );
                return None;
            }
        }
        if context == DeclarationContext::ForInitializer {
            if let Some(entry) = pending
                .iter()
                .find(|entry| entry.function_parameters.is_some())
            {
                self.report(
                    "E1206",
                    "function-declaration-in-for-initializer",
                    entry.declarator.name.span,
                );
                return None;
            }
        }
        if let Some(span) = inline_spans.first().copied() {
            if pending
                .iter()
                .any(|entry| entry.function_parameters.is_none())
            {
                self.report("E1401", "inline-specifier-on-non-function", span);
                return None;
            }
        }
        let mut declarations = self.side_declarations.split_off(side_start);
        if let Some(event) = specifier.tag_event {
            declarations.push(self.finish_tag_event(
                event,
                declaration_span,
                gnu_extension_spans.clone(),
            ));
        }
        for mut entry in pending {
            declarations.extend(entry.captures);
            let declaration = if storage == StorageClass::Typedef {
                CHirDecl::Typedef(TypedefDecl {
                    symbol: entry.symbol,
                    name: entry.declarator.name,
                    ty: entry.full_type,
                    specifier: entry.specifier,
                    written: entry.written,
                    gnu_extension_spans: gnu_extension_spans.clone(),
                    attributes: entry.attributes,
                    span: declaration_span,
                })
            } else if let Some(parameters) = entry.function_parameters {
                CHirDecl::Function(FunctionDecl {
                    symbol: entry.symbol,
                    name: entry.declarator.name,
                    ty: entry.full_type,
                    return_specifier: entry.specifier,
                    written: entry.written,
                    parameters,
                    storage,
                    asm_label: entry.asm_label,
                    asm_label_span: entry.asm_label_span,
                    inline_spans: inline_spans.clone(),
                    gnu_extension_spans: gnu_extension_spans.clone(),
                    attributes: entry.attributes,
                    body: None,
                    span: declaration_span,
                })
            } else {
                let cleanup = self.resolve_object_cleanup(
                    entry.symbol,
                    &entry.declarator.name,
                    storage,
                    &mut entry.attributes,
                )?;
                CHirDecl::Object(ObjectDecl {
                    symbol: entry.symbol,
                    name: entry.declarator.name,
                    ty: entry.full_type,
                    specifier: entry.specifier,
                    written: entry.written,
                    storage,
                    initializer: entry.initializer,
                    cleanup,
                    asm_label: entry.asm_label,
                    asm_label_span: entry.asm_label_span,
                    gnu_extension_spans: gnu_extension_spans.clone(),
                    attributes: entry.attributes,
                    span: declaration_span,
                })
            };
            declarations.push(self.alloc_node(declaration));
        }
        Some(declarations)
    }

    pub(super) fn resolve_object_cleanup(
        &mut self,
        object: SymbolId,
        name: &SpannedName,
        storage: StorageClass,
        attributes: &mut [Attribute],
    ) -> Option<Option<&'arena CHirExpr<'arena>>> {
        let mut cleanup = None;
        for attribute in attributes {
            if attribute.semantic_name() != Some("cleanup") {
                continue;
            }
            // GNU cleanup applies only to automatic variables. Attributes on
            // static or external objects remain inventory, not executed calls.
            if self.current_scope == self.symbols.file_scope()
                || matches!(storage, StorageClass::Static | StorageClass::Extern)
            {
                continue;
            }
            let [argument] = attribute.arguments.as_slice() else {
                self.report("E2903", "malformed-attribute", attribute.span);
                return None;
            };
            let spelling = argument.text.trim();
            let Some(function) = self.resolve_expression_symbol(spelling) else {
                self.report("E2001", "undeclared-identifier", argument.span);
                return None;
            };
            if self.symbols.symbol(function).ok()?.kind() != SymbolKind::Function
                || !self.facts_for_symbol(object).is_addressable()
            {
                self.report("E2903", "malformed-attribute", attribute.span);
                return None;
            }
            self.note_function_use(function, argument.span);
            let callee = self.alloc_node(CHirExpr::Ident {
                symbol: function,
                spelling: spelling.to_owned(),
                facts: self.facts_for_symbol(function),
                span: argument.span,
            });
            let local = self.alloc_node(CHirExpr::Ident {
                symbol: object,
                spelling: name.spelling.clone(),
                facts: self.facts_for_symbol(object),
                span: name.span,
            });
            let address = self.alloc_node(CHirExpr::Unary(UnaryOp::AddrOf, local, name.span));
            let parameters = match self
                .types
                .get(self.facts_for_symbol(function).ty())?
                .kind
                .clone()
            {
                TypeKind::Function { parameters, .. } => parameters,
                _ => unreachable!("cleanup resolved a function symbol"),
            };
            let valid = match parameters {
                FunctionParameters::Prototype { types, .. } => {
                    types.len() == 1
                        && self.assignment_constraint(AssignmentOp::Simple, types[0], address)
                            != UpdateConstraint::Invalid
                }
                FunctionParameters::Unspecified => true,
            };
            if !valid {
                self.report("E2903", "malformed-attribute", attribute.span);
                return None;
            }
            cleanup = Some(self.alloc_node(CHirExpr::Call(callee, vec![address], attribute.span)));
            attribute.applied = Some(AppliedAttribute::Cleanup);
        }
        Some(cleanup)
    }

    pub(super) fn function_declaration_parameters(
        &mut self,
        declarator: &ParsedDeclarator<'arena>,
        specifier: &ParsedSpecifier<'arena>,
        function_type: TypeId,
    ) -> Option<(ParameterSyntax, ScopeId, bool)> {
        if let Some(written) = declarator.derived.iter().find_map(|layer| match layer {
            DerivedDeclarator::Function {
                parameters, scope, ..
            } => Some((parameters.clone(), *scope)),
            _ => None,
        }) {
            return Some((written.0, written.1, false));
        }

        let inherited = specifier.inherited_function_parameters.clone().or_else(|| {
            self.parameter_syntax_from_function_type(function_type, specifier.surface.span())
        });
        let Some(mut inherited) = inherited else {
            self.report("E1001", "expected-declaration", declarator.name.span);
            return None;
        };
        if matches!(inherited, ParameterSyntax::IdentifierList { .. }) {
            inherited =
                self.parameter_syntax_from_function_type(function_type, specifier.surface.span())?;
        }
        if let ParameterSyntax::Prototype { parameters, .. } = &mut inherited {
            for parameter in parameters {
                parameter.symbol = None;
                parameter.name = None;
            }
        }
        let scope = self.create_scope(ScopeKind::FunctionPrototype, Some(declarator.name.span));
        Some((inherited, scope, true))
    }

    pub(super) fn parameter_syntax_from_function_type(
        &self,
        function_type: TypeId,
        span: SourceSpan,
    ) -> Option<ParameterSyntax> {
        let TypeKind::Function { parameters, .. } = &self.types.get(function_type)?.kind else {
            return None;
        };
        match parameters {
            FunctionParameters::Unspecified => Some(ParameterSyntax::Unspecified),
            FunctionParameters::Prototype { types, variadic } => {
                let parameters = types
                    .iter()
                    .map(|ty| ParameterDecl {
                        symbol: None,
                        name: None,
                        ty: *ty,
                        storage: StorageClass::None,
                        // A declaration through a semantic function type has
                        // no written parameter list of its own. These nodes
                        // preserve the canonical parameter types only.
                        specifier: TypeSpecifier::Builtin { ty: *ty, span },
                        written: WrittenDeclarator {
                            layers: Vec::new(),
                            span,
                        },
                        attributes: Vec::new(),
                        span,
                    })
                    .collect();
                Some(ParameterSyntax::Prototype {
                    parameters,
                    variadic: *variadic,
                })
            }
        }
    }
}
