//! Parser declarators.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_type_name(&mut self) -> Option<(TypeId, Option<PendingTag<'arena>>)> {
        self.parse_type_name_with_vla_error(None)
    }

    pub(super) fn parse_unevaluated_type_name(
        &mut self,
    ) -> Option<(TypeId, Option<PendingTag<'arena>>)> {
        let side_start = self.side_declarations.len();
        let result = self.parse_type_name()?;
        // Keep bindings needed by the resolved type, but do not execute a
        // nested typeof operand in a query that only inspects that type.
        self.take_extent_evaluations(side_start);
        Some(result)
    }

    pub(super) fn parse_type_name_with_vla_error(
        &mut self,
        vla_error_span: Option<SourceSpan>,
    ) -> Option<(TypeId, Option<PendingTag<'arena>>)> {
        self.parse_type_name_details(vla_error_span, false, false)
            .map(|(ty, tag, _)| (ty, tag))
    }

    pub(super) fn parse_type_name_details(
        &mut self,
        vla_error_span: Option<SourceSpan>,
        size_query: bool,
        capture_cast: bool,
    ) -> Option<(
        TypeId,
        Option<PendingTag<'arena>>,
        Option<&'arena CHirExpr<'arena>>,
    )> {
        let mut type_attributes = Vec::new();
        let specifier = self.parse_type_specifier_with_attributes(&mut type_attributes)?;
        let capture_typeof = vla_error_span.is_some()
            && self.in_function_scope()
            && self
                .symbols
                .scope(self.current_scope)
                .is_ok_and(|scope| scope.kind() != ScopeKind::FunctionPrototype);
        let mut derived = if matches!(
            self.peek_kind(),
            Some(TokenKind::Star | TokenKind::LBracket | TokenKind::LParen)
        ) {
            let declarator = self.parse_abstract_declarator_with_vla_error(
                vla_error_span,
                size_query || capture_typeof || capture_cast,
            )?;
            type_attributes.extend(declarator.attributes);
            declarator.derived
        } else {
            Vec::new()
        };
        self.validate_array_parameter_modifiers(&derived, false);
        if capture_typeof || capture_cast {
            let captures = self.capture_array_bounds(&mut derived)?;
            self.side_declarations.extend(captures);
        }
        // GCC 14 and Apple clang refuse an attribute after an abstract
        // declarator; an x86 convention keeps E2952 there (EvoIR 10.1.9).
        let suffix = !derived.is_empty();
        type_attributes
            .extend(self.parse_attribute_specifiers_in(AttributePlacement::TypeName, suffix));
        self.settle_convention_attributes(&type_attributes, Settlement::Other);
        for attribute in &mut type_attributes {
            attribute.placement = AttributePlacement::TypeName;
        }
        let ty = self.apply_declarator(specifier.surface.type_id(), &derived);
        self.validate_alignment_specifiers(ty, &type_attributes, false);
        let ty = self.apply_attributes_to_type(ty, &mut type_attributes);
        // A type name has no declaration carrying its written pointer layers.
        // Preserve those attributes separately, without applying their layout.
        for layer in &derived {
            if let DerivedDeclarator::Pointer { attributes, .. } = layer {
                self.detached_attributes.extend(attributes.iter().cloned());
            }
        }
        self.detached_attributes.extend(type_attributes);
        let dynamic_size = if size_query && self.target_layout.sizeof_type(&self.types, ty).is_err()
        {
            let span = specifier.surface.span();
            let mut size = self.captured_size_expression(specifier.surface.type_id(), span);
            for layer in derived.iter().rev() {
                size = match layer {
                    DerivedDeclarator::Pointer { .. } => Some(self.size_literal(8, span)),
                    DerivedDeclarator::Function { .. } => Some(self.size_literal(1, span)),
                    DerivedDeclarator::Array {
                        expression: Some(count),
                        bound,
                        ..
                    } => {
                        if matches!(bound, ArrayBound::Variable)
                            && !self
                                .converted_expression_type(count)
                                .is_some_and(|inferred| {
                                    is_integer_class(self.operand_type_class(inferred.ty))
                                })
                        {
                            self.report("E2923", "invalid-array-bound", count.span());
                            return None;
                        }
                        let integer =
                            self.intern(TypeKind::Int(IntRank::ULong), Qualifiers::default());
                        let count = self.alloc_node(CHirExpr::Cast(integer, count, count.span()));
                        size.map(|size| {
                            self.alloc_node(CHirExpr::Binary(BinaryOp::Mul, size, count, span))
                        })
                    }
                    _ => None,
                };
            }
            size
        } else {
            None
        };
        Some((ty, specifier.tag_event, dynamic_size))
    }

    /// A cast's bounds execute with the cast, not when the surrounding
    /// statement is entered. Declare the ordinary capture objects in that
    /// scope, but sequence their assignments on the actual expression path.
    pub(super) fn parse_cast_type_name(
        &mut self,
    ) -> Option<(
        TypeId,
        Option<PendingTag<'arena>>,
        Vec<&'arena CHirExpr<'arena>>,
    )> {
        let side_start = self.side_declarations.len();
        let (ty, tag, _) = self.parse_type_name_details(None, false, self.in_function_scope())?;
        let evaluations = self.take_extent_evaluations(side_start);
        Some((ty, tag, evaluations))
    }

    /// Move hidden capture initializers onto the expression that needs them.
    /// Keeping an initializer in side_declarations would execute it even if
    /// sizeof, a conditional, or short-circuit evaluation skips that expression.
    pub(super) fn take_extent_evaluations(
        &mut self,
        side_start: usize,
    ) -> Vec<&'arena CHirExpr<'arena>> {
        let mut evaluations = Vec::new();
        for declaration in self.side_declarations.split_off(side_start) {
            if let CHirDecl::Object(object) = declaration {
                if object.name.spelling.starts_with("__evocc.extent.") {
                    if let Some(value) = object
                        .initializer
                        .as_ref()
                        .and_then(|i| i.written.expression())
                    {
                        let target = self.alloc_node(CHirExpr::Ident {
                            symbol: object.symbol,
                            spelling: object.name.spelling.clone(),
                            facts: self.facts_for_symbol(object.symbol),
                            span: object.span,
                        });
                        let place = ScalarPlace::bind(&self.symbols, &self.types, object.symbol)
                            .expect("a bound capture is an automatic unsigned integer");
                        evaluations.push(self.alloc_node(CHirExpr::Assign {
                            op: AssignmentOp::Simple,
                            place,
                            target,
                            value,
                            facts: ExprFacts::value(object.ty),
                            span: object.span,
                        }));
                        let mut uninitialized = object.clone();
                        uninitialized.initializer = None;
                        let declaration = self.alloc_node(CHirDecl::Object(uninitialized));
                        self.side_declarations.push(declaration);
                        continue;
                    }
                }
            }
            self.side_declarations.push(declaration);
        }
        evaluations
    }

    // ── declarators ────────────────────────────────────────────────────

    pub(super) fn capture_array_bounds(
        &mut self,
        derived: &mut [DerivedDeclarator<'arena>],
    ) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        let mut captures = Vec::new();
        for layer in derived {
            let DerivedDeclarator::Array {
                bound: bound @ ArrayBound::Variable,
                expression: Some(expression),
                span,
                ..
            } = layer
            else {
                continue;
            };
            if !self
                .converted_expression_type(expression)
                .is_some_and(|inferred| is_integer_class(self.operand_type_class(inferred.ty)))
            {
                self.report("E2923", "invalid-array-bound", expression.span());
                return None;
            }
            let ty = self.intern(TypeKind::Int(IntRank::ULong), Qualifiers::default());
            let name = SpannedName {
                spelling: format!("__evocc.extent.{}", self.symbols.len()),
                span: *span,
            };
            let symbol = self.declare_symbol(
                &name,
                SymbolData::Object {
                    storage: StorageClass::Auto,
                },
                ty,
            );
            let value = self.alloc_node(CHirExpr::Cast(ty, expression, *span));
            let written = WrittenInitializer::Expression(value);
            let plan = self.normalize_initializer_plan(ty, &written, false)?;
            captures.push(self.alloc_node(CHirDecl::Object(ObjectDecl {
                symbol,
                name,
                ty,
                specifier: TypeSpecifier::Builtin { ty, span: *span },
                written: WrittenDeclarator {
                    layers: Vec::new(),
                    span: *span,
                },
                storage: StorageClass::Auto,
                initializer: Some(ObjectInitializer::new(written, plan)),
                cleanup: None,
                asm_label: None,
                asm_label_span: None,
                gnu_extension_spans: Vec::new(),
                attributes: Vec::new(),
                span: *span,
            })));
            // The array type consumes this captured extent through its symbol,
            // not through a parsed identifier. It is not an unused source local.
            self.lint_uses.insert(symbol);
            *bound = ArrayBound::Captured(symbol.raw());
        }
        Some(captures)
    }

    /// Elaborate a size query to reads of the declaration's captured bounds,
    /// never to reevaluation of their source expressions.
    pub(super) fn captured_size_expression(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let (fixed, captures) = self
            .target_layout
            .array_size_factors(&self.types, ty)
            .ok()?;
        let mut size = self.size_literal(fixed, span);
        for raw in captures {
            let symbol = self.symbols.symbols_in_source_order().get(raw as usize)?;
            if symbol.kind() != SymbolKind::Object
                || !symbol.spelling().starts_with("__evocc.extent.")
            {
                return None;
            }
            let count = self.alloc_node(CHirExpr::Ident {
                symbol: symbol.id(),
                spelling: symbol.spelling().to_owned(),
                facts: self.facts_for_symbol(symbol.id()),
                span,
            });
            size = self.alloc_node(CHirExpr::Binary(BinaryOp::Mul, size, count, span));
        }
        Some(size)
    }

    pub(super) fn classify_array_bound(&mut self, expression: &CHirExpr<'_>) -> ArrayBound {
        // Check the expression's type before classifying an integer bound as
        // constant or variable. A noninteger bound is invalid in every storage
        // context; it must not become a VLA and get a different diagnostic.
        if self
            .converted_expression_type(expression)
            .is_some_and(|inferred| !is_integer_class(self.operand_type_class(inferred.ty)))
        {
            self.report("E2923", "invalid-array-bound", expression.span());
            return ArrayBound::Fixed(0);
        }
        match self.eval_integer_constant_expression(expression) {
            Ok(value)
                if value
                    .try_mathematical_value()
                    .is_some_and(|value| value < 0) =>
            {
                self.report("E2923", "invalid-array-bound", expression.span());
                // Preserve a concrete, non-VLA layer after the diagnostic so
                // the declaration context cannot replace E2923 with E2203.
                ArrayBound::Fixed(0)
            }
            Ok(value) => match value
                .try_mathematical_value()
                .and_then(|value| u64::try_from(value).ok())
            {
                Some(value) => ArrayBound::Fixed(value),
                None => {
                    self.report("E2923", "invalid-array-bound", expression.span());
                    ArrayBound::Fixed(0)
                }
            },
            Err(()) => ArrayBound::Variable,
        }
    }

    pub(super) fn validate_variable_array_bounds(
        &mut self,
        derived: &[DerivedDeclarator<'arena>],
        array_parameter: bool,
    ) {
        if array_parameter {
            for layer in derived {
                if let DerivedDeclarator::Array {
                    bound: ArrayBound::Variable,
                    expression: Some(expression),
                    ..
                } = layer
                {
                    if !self
                        .converted_expression_type(expression)
                        .is_some_and(|inferred| {
                            is_integer_class(self.operand_type_class(inferred.ty))
                        })
                    {
                        self.report("E2923", "invalid-array-bound", expression.span());
                    }
                }
            }
            return;
        }
        if let Some(span) = derived.iter().find_map(|layer| match layer {
            DerivedDeclarator::Array {
                bound: ArrayBound::Variable,
                span,
                ..
            } => Some(*span),
            _ => None,
        }) {
            self.report("E2203", "variable-length-object", span);
        }
    }

    pub(super) fn parse_array_parameter_modifiers(&mut self) -> (Qualifiers, bool) {
        let before_static = self.parse_qualifiers();
        let static_bound = self.match_kind(TokenKind::Static);
        let after_static = self.parse_qualifiers();
        (merge_qualifiers(before_static, after_static), static_bound)
    }

    pub(super) fn parse_prototype_array_star(&mut self, static_bound: bool) -> bool {
        if self.peek_kind() != Some(TokenKind::Star) {
            return false;
        }
        let star = self.advance().expect("peeked array asterisk");
        let scope = self
            .symbols
            .scope(self.current_scope)
            .expect("current scope exists");
        if static_bound || scope.kind() != ScopeKind::FunctionPrototype {
            self.report("E1005", "invalid-declarator", star.span);
        }
        true
    }

    pub(super) fn validate_array_parameter_modifiers(
        &mut self,
        derived: &[DerivedDeclarator<'arena>],
        allowed: bool,
    ) {
        if let Some(span) = derived
            .iter()
            .enumerate()
            .find_map(|(index, layer)| match layer {
                DerivedDeclarator::Array {
                    parameter_qualifiers,
                    static_bound,
                    prototype_star,
                    span,
                    ..
                } if ((!allowed || index != 0)
                    && (*static_bound || *parameter_qualifiers != Qualifiers::default()))
                    || (!allowed && *prototype_star) =>
                {
                    Some(*span)
                }
                _ => None,
            })
        {
            self.report("E1005", "invalid-declarator", span);
        }
    }

    pub(super) fn parse_abstract_declarator(&mut self) -> Option<ParsedAbstractDeclarator<'arena>> {
        self.parse_abstract_declarator_with_vla_error(None, true)
    }

    pub(super) fn parse_abstract_declarator_with_vla_error(
        &mut self,
        vla_error_span: Option<SourceSpan>,
        array_parameter: bool,
    ) -> Option<ParsedAbstractDeclarator<'arena>> {
        let mut attributes = self.parse_attribute_specifiers(AttributePlacement::TypeName);
        let mut pointers = Vec::new();
        while self.peek_kind() == Some(TokenKind::Star) {
            let star = self.advance().expect("peeked star");
            let mut qualifiers = self.parse_qualifiers();
            if self.target_layout.names_clang_extensions() {
                qualifiers = self.parse_darwin_pointer_qualifiers(qualifiers);
            }
            let attributes = self.parse_attribute_specifiers(AttributePlacement::PointerDeclarator);
            self.validate_pointer_attributes(&attributes);
            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map_or(star.span, |token| token.span);
            pointers.push(DerivedDeclarator::Pointer {
                qualifiers,
                attributes,
                span: self.merge_span(star.span, end),
            });
        }

        if self.peek_kind() == Some(TokenKind::LParen)
            && self.tokens.get(self.pos + 1).map(|token| token.kind) == Some(TokenKind::Caret)
            && self.target_layout.names_clang_extensions()
        {
            self.advance();
            self.refuse_block_caret();
            return None;
        }
        let grouped = self.peek_kind() == Some(TokenKind::LParen)
            && matches!(
                self.tokens.get(self.pos + 1).map(|token| token.kind),
                Some(
                    TokenKind::Star
                        | TokenKind::LParen
                        | TokenKind::LBracket
                        | TokenKind::Attribute
                )
            );
        let mut declarator = if grouped {
            let open = self.advance().expect("peeked abstract grouping");
            let mut inner =
                self.parse_abstract_declarator_with_vla_error(vla_error_span, array_parameter)?;
            let close = self.expect(TokenKind::RParen)?;
            attributes.append(&mut inner.attributes);
            inner.attributes = attributes;
            inner.span = self.merge_span(open.span, close.span);
            inner
        } else {
            let span = pointers
                .first()
                .map(derived_declarator_span)
                .unwrap_or_else(|| self.span());
            ParsedAbstractDeclarator {
                derived: Vec::new(),
                attributes,
                span,
            }
        };

        loop {
            match self.peek_kind() {
                Some(TokenKind::LBracket) if !self.bracket_attribute_starts_at(self.pos) => {
                    let open = self.advance().expect("peeked abstract array");
                    if matches!(
                        declarator.derived.last(),
                        Some(DerivedDeclarator::Function { .. })
                    ) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let (parameter_qualifiers, static_bound) =
                        self.parse_array_parameter_modifiers();
                    let prototype_star = self.parse_prototype_array_star(static_bound);
                    let mut bound_expression = None;
                    let bound = if prototype_star {
                        ArrayBound::Variable
                    } else if self.peek_kind() == Some(TokenKind::RBracket) {
                        if static_bound {
                            self.report("E1005", "invalid-declarator", open.span);
                        }
                        ArrayBound::Unbounded
                    } else {
                        let expression = self.parse_assignment()?;
                        bound_expression = Some(expression);
                        let bound = self.classify_array_bound(expression);
                        if matches!(bound, ArrayBound::Variable) && !array_parameter {
                            if let Some(span) = vla_error_span {
                                self.report("E2904", "not-covered: variably-modified-typeof", span);
                            } else {
                                self.report(
                                    "E1005",
                                    "unsupported-abstract-declarator",
                                    expression.span(),
                                );
                            }
                        }
                        bound
                    };
                    let close = self.expect(TokenKind::RBracket)?;
                    let layer_span = self.merge_span(open.span, close.span);
                    declarator.derived.push(DerivedDeclarator::Array {
                        bound,
                        expression: bound_expression,
                        prototype_star,
                        parameter_qualifiers,
                        static_bound,
                        span: layer_span,
                    });
                    declarator
                        .attributes
                        .extend(self.parse_bracket_attributes(AttributePlacement::ArrayType));
                    declarator.span = self.merge_span(declarator.span, layer_span);
                }
                Some(TokenKind::LParen) => {
                    let open = self.advance().expect("peeked abstract function");
                    if matches!(
                        declarator.derived.last(),
                        Some(DerivedDeclarator::Array { .. })
                    ) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let outer_scope = self.current_scope;
                    let parameter_scope =
                        self.create_scope(ScopeKind::FunctionPrototype, Some(open.span));
                    self.current_scope = parameter_scope;
                    let parameters = self.parse_parameter_list(false);
                    self.current_scope = outer_scope;
                    let close = self.expect(TokenKind::RParen)?;
                    let layer_span = self.merge_span(open.span, close.span);
                    declarator.derived.push(DerivedDeclarator::Function {
                        parameters: parameters.syntax,
                        entry_bounds: parameters.entry_bounds,
                        scope: parameter_scope,
                        span: layer_span,
                    });
                    declarator
                        .attributes
                        .extend(self.parse_bracket_attributes(AttributePlacement::FunctionType));
                    declarator.span = self.merge_span(declarator.span, layer_span);
                }
                _ => break,
            }
        }

        if let Some(pointer) = pointers.first() {
            declarator.span = self.merge_span(derived_declarator_span(pointer), declarator.span);
        }
        declarator.derived.extend(pointers.into_iter().rev());
        if declarator.derived.is_empty() {
            self.report("E1005", "unsupported-abstract-declarator", self.span());
            None
        } else {
            Some(declarator)
        }
    }

    /// Clang's Blocks extension: `^` in a declarator or as a block literal
    /// is `E2963` on a target whose headers are written for Clang. EvoCC
    /// defines no `__BLOCKS__`, as Apple clang with `-fno-blocks`.
    pub(super) fn refuse_block_caret(&mut self) -> bool {
        if self.target_layout.names_clang_extensions() && self.peek_kind() == Some(TokenKind::Caret)
        {
            // A declaration that is parsed again meets the same `^`.
            let span = self.span();
            if !self
                .diagnostics
                .iter()
                .any(|earlier| earlier.code == "E2963" && earlier.span == span)
            {
                self.report("E2963", "unsupported-blocks", span);
            }
            return true;
        }
        false
    }

    pub(super) fn parse_declarator(
        &mut self,
        function_definition_possible: bool,
    ) -> Option<ParsedDeclarator<'arena>> {
        if self.refuse_block_caret() {
            return None;
        }
        let mut pointers = Vec::new();
        let mut symbol_attributes = Vec::new();
        while self.peek_kind() == Some(TokenKind::Star) {
            let star = self.advance().expect("peeked star");
            let mut qualifiers = self.parse_qualifiers();
            if self.target_layout.names_clang_extensions() {
                qualifiers = self.parse_darwin_pointer_qualifiers(qualifiers);
            }
            let attributes = self.parse_attribute_specifiers(AttributePlacement::PointerDeclarator);
            self.validate_pointer_attributes(&attributes);
            // Linkage, placement and retention marks describe the declared symbol,
            // never its pointer type. Keep their written position/span, but put
            // them in the declaration inventory shared by linkage and attribute
            // queries. Other pointer attributes remain on the written pointer layer.
            let darwin = self.target_layout.names_clang_extensions();
            let (linkage, attributes): (Vec<_>, Vec<_>) =
                attributes.into_iter().partition(|attribute| {
                    matches!(
                        attribute.semantic_name(),
                        Some("weak" | "alias" | "section" | "used" | "retain")
                    ) || (darwin
                        && attribute
                            .semantic_name()
                            .and_then(convention_attribute)
                            .is_some())
                });
            symbol_attributes.extend(linkage);
            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map_or(star.span, |t| t.span);
            pointers.push(DerivedDeclarator::Pointer {
                qualifiers,
                attributes,
                span: self.merge_span(star.span, end),
            });
        }

        let mut declarator = if self.peek_kind() == Some(TokenKind::Identifier) {
            let token = self.advance().unwrap();
            ParsedDeclarator {
                name: SpannedName {
                    spelling: self.token_text(&token).to_owned(),
                    span: token.span,
                },
                derived: Vec::new(),
                attributes: Vec::new(),
                span: token.span,
            }
        } else if self.peek_kind() == Some(TokenKind::LParen) {
            let open = self.advance().expect("peeked left parenthesis");
            // GNU attributes may open a parenthesized declarator (10.1.9).
            // A bracketed attribute may not: GCC 14 and Apple clang refuse it.
            let mut grouped = if self.peek_kind() == Some(TokenKind::Attribute) {
                self.parse_attribute_specifiers(AttributePlacement::ParenthesizedDeclarator)
            } else {
                Vec::new()
            };
            // An abstract parameter declarator returns here and is parsed
            // again, so the attributes are checked only for a named one.
            let mut inner = self.parse_declarator(function_definition_possible)?;
            let close = self.expect(TokenKind::RParen)?;
            for attribute in &grouped {
                if attribute
                    .semantic_name()
                    .is_some_and(|name| PARENTHESIZED_EFFECT_ATTRIBUTES.contains(&name))
                {
                    self.report("E2903", "malformed-attribute", attribute.span);
                }
            }
            grouped.append(&mut inner.attributes);
            inner.attributes = grouped;
            inner.span = self.merge_span(open.span, close.span);
            inner
        } else {
            return None;
        };
        symbol_attributes.append(&mut declarator.attributes);
        declarator.attributes = symbol_attributes;

        loop {
            match self.peek_kind() {
                Some(TokenKind::LBracket) if !self.bracket_attribute_starts_at(self.pos) => {
                    let open = self.advance().unwrap();
                    if matches!(
                        declarator.derived.last(),
                        Some(DerivedDeclarator::Function { .. })
                    ) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let (parameter_qualifiers, static_bound) =
                        self.parse_array_parameter_modifiers();
                    let prototype_star = self.parse_prototype_array_star(static_bound);
                    let mut bound_expression = None;
                    let bound = if prototype_star {
                        ArrayBound::Variable
                    } else if self.peek_kind() == Some(TokenKind::RBracket) {
                        if static_bound {
                            self.report("E1005", "invalid-declarator", open.span);
                        }
                        ArrayBound::Unbounded
                    } else {
                        let expression = self.parse_assignment()?;
                        bound_expression = Some(expression);
                        self.classify_array_bound(expression)
                    };
                    let close = self.expect(TokenKind::RBracket)?;
                    declarator.derived.push(DerivedDeclarator::Array {
                        bound,
                        expression: bound_expression,
                        prototype_star,
                        parameter_qualifiers,
                        static_bound,
                        span: self.merge_span(open.span, close.span),
                    });
                    declarator
                        .attributes
                        .extend(self.parse_bracket_attributes(AttributePlacement::ArrayType));
                    declarator.span = self.merge_span(declarator.span, close.span);
                }
                Some(TokenKind::LParen) => {
                    let open = self.advance().unwrap();
                    if matches!(
                        declarator.derived.last(),
                        Some(DerivedDeclarator::Array { .. })
                    ) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let parameter_scope_kind = if function_definition_possible {
                        self.upcoming_function_scope_kind()
                    } else {
                        ScopeKind::FunctionPrototype
                    };
                    let outer_scope = self.current_scope;
                    let parameter_scope = self.create_scope(parameter_scope_kind, Some(open.span));
                    self.current_scope = parameter_scope;
                    let parameters = self.parse_parameter_list(function_definition_possible);
                    self.current_scope = outer_scope;
                    let close = self.expect(TokenKind::RParen)?;
                    declarator.derived.push(DerivedDeclarator::Function {
                        parameters: parameters.syntax,
                        entry_bounds: parameters.entry_bounds,
                        scope: parameter_scope,
                        span: self.merge_span(open.span, close.span),
                    });
                    declarator
                        .attributes
                        .extend(self.parse_bracket_attributes(AttributePlacement::FunctionType));
                    declarator.span = self.merge_span(declarator.span, close.span);
                }
                _ => break,
            }
        }
        if let Some(pointer) = pointers.first() {
            let pointer_span = match pointer {
                DerivedDeclarator::Pointer { span, .. } => *span,
                _ => unreachable!(),
            };
            declarator.span = self.merge_span(pointer_span, declarator.span);
        }
        declarator.derived.extend(pointers.into_iter().rev());
        Some(declarator)
    }

    pub(super) fn upcoming_function_scope_kind(&self) -> ScopeKind {
        if self.peek().is_some_and(|token| {
            token.kind == TokenKind::Identifier && !self.token_starts_type_name(token)
        }) {
            return ScopeKind::Function;
        }
        let mut depth = 1_usize;
        let mut index = self.pos;
        while let Some(token) = self.tokens.get(index) {
            match token.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        index += 1;
                        // Parenthesized declarators may contribute closing
                        // parentheses between the parameter list and body.
                        loop {
                            if self.attribute_starts_at(index) {
                                let Some(end) = self.attribute_specifier_end(index) else {
                                    break;
                                };
                                index = end;
                                continue;
                            }
                            match self.tokens.get(index).map(|token| token.kind) {
                                Some(TokenKind::RParen) => index += 1,
                                Some(TokenKind::Asm) => {
                                    index += 1;
                                    if self.tokens.get(index).map(|token| token.kind)
                                        != Some(TokenKind::LParen)
                                    {
                                        break;
                                    }
                                    let mut attribute_depth = 0_usize;
                                    while let Some(token) = self.tokens.get(index) {
                                        index += 1;
                                        match token.kind {
                                            TokenKind::LParen => attribute_depth += 1,
                                            TokenKind::RParen => {
                                                attribute_depth -= 1;
                                                if attribute_depth == 0 {
                                                    break;
                                                }
                                            }
                                            TokenKind::Eof => break,
                                            _ => {}
                                        }
                                    }
                                }
                                _ => break,
                            }
                        }
                        return if self.tokens.get(index).map(|token| token.kind)
                            == Some(TokenKind::LBrace)
                        {
                            ScopeKind::Function
                        } else {
                            ScopeKind::FunctionPrototype
                        };
                    }
                }
                TokenKind::Eof => break,
                _ => {}
            }
            index += 1;
        }
        ScopeKind::FunctionPrototype
    }

    fn parse_parameter_list(&mut self, definition_possible: bool) -> ParsedParameters<'arena> {
        let mut params = Vec::new();
        let mut entry_bounds = Vec::new();
        let mut variadic = false;
        if self.peek_kind() == Some(TokenKind::RParen) {
            return ParsedParameters {
                syntax: ParameterSyntax::Unspecified,
                entry_bounds,
            };
        }
        if self.peek().is_some_and(|token| {
            token.kind == TokenKind::Identifier && !self.token_starts_type_name(token)
        }) {
            if !definition_possible {
                self.report("E1001", "expected-declaration", self.span());
            }
            let ty = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
            while let Some(token) = self.expect(TokenKind::Identifier) {
                let name = SpannedName {
                    spelling: self.token_text(&token).to_owned(),
                    span: token.span,
                };
                if params.iter().any(|parameter: &ParameterDecl| {
                    parameter
                        .name
                        .as_ref()
                        .is_some_and(|prior| prior.spelling == name.spelling)
                }) {
                    self.report("E2002", "incompatible-redeclaration", name.span);
                }
                params.push(ParameterDecl {
                    symbol: None,
                    name: Some(name),
                    ty,
                    storage: StorageClass::None,
                    specifier: TypeSpecifier::Builtin {
                        ty,
                        span: token.span,
                    },
                    written: WrittenDeclarator {
                        layers: Vec::new(),
                        span: token.span,
                    },
                    attributes: Vec::new(),
                    span: token.span,
                });
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
            }
            return ParsedParameters {
                syntax: ParameterSyntax::IdentifierList { parameters: params },
                entry_bounds,
            };
        }
        if self.peek_kind() == Some(TokenKind::Void)
            && self.tokens.get(self.pos + 1).map(|token| token.kind) == Some(TokenKind::RParen)
        {
            self.advance();
            return ParsedParameters {
                syntax: ParameterSyntax::Prototype {
                    parameters: params,
                    variadic,
                },
                entry_bounds,
            };
        }
        let shadow_checkpoint = self.temporary_parameter_shadows.len();
        loop {
            if self.match_kind(TokenKind::Ellipsis) {
                variadic = true;
                break;
            }
            let side_start = self.side_declarations.len();
            let storage_span = self.peek().map(|token| token.span);
            let storage = self.parse_storage_class();
            if !matches!(storage, StorageClass::None | StorageClass::Register) {
                self.report(
                    "E1001",
                    "expected-declaration",
                    storage_span.unwrap_or_else(|| self.span()),
                );
            }
            while self.peek_kind() == Some(TokenKind::Inline) {
                let token = self.advance().expect("peeked inline specifier");
                self.report(
                    "E1401",
                    "inline-specifier-on-parameter-declaration",
                    token.span,
                );
            }
            // Attributes among a parameter's specifiers, before or after the
            // type, sit in the declaration-specifiers position.
            let mut specifier_attributes =
                self.parse_attribute_specifiers(AttributePlacement::DeclarationSpecifiers);
            let Some(specifier) =
                self.parse_type_specifier_with_attributes(&mut specifier_attributes)
            else {
                self.report("E1001", "expected-declaration", self.span());
                break;
            };
            while self.peek_kind() == Some(TokenKind::Inline) {
                let token = self.advance().expect("peeked inline specifier");
                self.report(
                    "E1401",
                    "inline-specifier-on-parameter-declaration",
                    token.span,
                );
            }
            specifier_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::DeclarationSpecifiers));
            let start = specifier.surface.span();
            let (name, mut derived, mut written, end) =
                if matches!(self.peek_kind(), Some(TokenKind::Comma | TokenKind::RParen)) {
                    (
                        None,
                        Vec::new(),
                        WrittenDeclarator {
                            layers: Vec::new(),
                            span: start,
                        },
                        start,
                    )
                } else if self.parameter_declarator_prefers_abstract() {
                    match self.parse_abstract_declarator() {
                        Some(declarator) => {
                            let written = self.written_abstract_declarator(&declarator);
                            specifier_attributes.extend(declarator.attributes);
                            (None, declarator.derived, written, declarator.span)
                        }
                        None => (
                            None,
                            Vec::new(),
                            WrittenDeclarator {
                                layers: Vec::new(),
                                span: start,
                            },
                            start,
                        ),
                    }
                } else {
                    let declarator_start = self.pos;
                    let error_checkpoint = self.error_count;
                    let frames = self.availability_frames.len();
                    let pending = frames
                        .checked_sub(1)
                        .map(|frame| self.availability_frames[frame].pending.len());
                    match self.parse_declarator(false) {
                        Some(declarator) => {
                            let written = self.written_declarator(&declarator);
                            specifier_attributes.extend(declarator.attributes);
                            (
                                Some(declarator.name),
                                declarator.derived,
                                written,
                                declarator.span,
                            )
                        }
                        None if self.error_count == error_checkpoint => {
                            // A name-less declarator begins with the same pointer
                            // and parenthesis tokens as a named declarator. Retry
                            // from the exact token checkpoint with the bounded
                            // abstract grammar instead of silently dropping the
                            // consumed layers. Its uses are read again.
                            self.pos = declarator_start;
                            self.availability_frames.truncate(frames);
                            if let Some(pending) = pending {
                                self.availability_frames[frames - 1]
                                    .pending
                                    .truncate(pending);
                            }
                            if let Some(declarator) = self.parse_abstract_declarator() {
                                let written = self.written_abstract_declarator(&declarator);
                                specifier_attributes.extend(declarator.attributes);
                                (None, declarator.derived, written, declarator.span)
                            } else {
                                (
                                    None,
                                    Vec::new(),
                                    WrittenDeclarator {
                                        layers: Vec::new(),
                                        span: start,
                                    },
                                    start,
                                )
                            }
                        }
                        None => (
                            None,
                            Vec::new(),
                            WrittenDeclarator {
                                layers: Vec::new(),
                                span: start,
                            },
                            start,
                        ),
                    }
                };
            self.validate_array_parameter_modifiers(&derived, true);
            self.validate_variable_array_bounds(&derived, true);
            let definition = self
                .symbols
                .scope(self.current_scope)
                .is_ok_and(|scope| scope.kind() == ScopeKind::Function);
            // A VM typeof specifier can create captures before the written
            // parameter declarator does. They belong to this definition's
            // entry, in source order, not to the enclosing declaration list.
            // Prototypes (including nested callback prototypes) have no
            // runtime entry and must not execute these captures.
            for declaration in self.side_declarations.split_off(side_start) {
                if matches!(declaration, CHirDecl::Object(object)
                    if object.name.spelling.starts_with("__evocc.extent."))
                {
                    if definition {
                        entry_bounds.push(declaration);
                    }
                } else {
                    self.side_declarations.push(declaration);
                }
            }
            if definition {
                if let Some(captures) = self.capture_array_bounds(&mut derived) {
                    entry_bounds.extend(captures);
                    written = self.written_derived(&derived, written.span);
                }
            }
            let unadjusted = self.apply_declarator(specifier.surface.type_id(), &derived);
            let ty = self.adjust_parameter_type(unadjusted, &derived);
            let span = self.merge_span(start, end);
            if let Some(event) = specifier.tag_event {
                let declaration = self.finish_tag_event(event, span, Vec::new());
                self.side_declarations.push(declaration);
            }
            let declared_name = name.as_ref().map(|name| (name.spelling.clone(), name.span));
            let mut parameter_attributes = specifier_attributes;
            parameter_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::Parameter));
            self.settle_convention_attributes(&parameter_attributes, Settlement::Other);
            self.validate_alignment_specifiers(ty, &parameter_attributes, false);
            let ty = self.apply_declaration_attributes(ty, &mut parameter_attributes, false);
            params.push(ParameterDecl {
                attributes: parameter_attributes.clone(),
                symbol: None,
                name,
                ty,
                storage,
                specifier: specifier.surface,
                written,
                span,
            });
            if let Some((spelling, span)) = declared_name {
                self.temporary_parameter_shadows
                    .push(TemporaryParameterShadow {
                        scope: self.current_scope,
                        spelling,
                        ty,
                        storage,
                        attributes: parameter_attributes,
                        span,
                    });
            }
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        self.temporary_parameter_shadows.truncate(shadow_checkpoint);
        ParsedParameters {
            syntax: ParameterSyntax::Prototype {
                parameters: params,
                variadic,
            },
            entry_bounds,
        }
    }

    pub(super) fn parameter_declarator_prefers_abstract(&self) -> bool {
        let mut index = self.pos;
        if self.tokens.get(index).map(|token| token.kind) != Some(TokenKind::LParen) {
            return false;
        }
        while self.tokens.get(index).map(|token| token.kind) == Some(TokenKind::LParen) {
            index += 1;
        }
        self.tokens
            .get(index)
            .is_some_and(|token| self.typedef_name(token).is_some())
    }

    pub(super) fn adjust_parameter_type(
        &mut self,
        ty: TypeId,
        derived: &[DerivedDeclarator<'arena>],
    ) -> TypeId {
        match self.types.get(ty).map(|ty| ty.kind.clone()) {
            Some(TypeKind::Array { element, .. }) => {
                let qualifiers = derived
                    .first()
                    .and_then(|layer| match layer {
                        DerivedDeclarator::Array {
                            parameter_qualifiers,
                            ..
                        } => Some(*parameter_qualifiers),
                        _ => None,
                    })
                    .unwrap_or_default();
                self.intern(TypeKind::Pointer(element), qualifiers)
            }
            Some(TypeKind::Function { .. }) => {
                self.intern(TypeKind::Pointer(ty), Qualifiers::default())
            }
            _ => ty,
        }
    }

    pub(super) fn apply_declarator(
        &mut self,
        mut ty: TypeId,
        derived: &[DerivedDeclarator<'arena>],
    ) -> TypeId {
        for layer in derived.iter().rev() {
            ty = match layer {
                DerivedDeclarator::Pointer { qualifiers, .. } => {
                    self.intern(TypeKind::Pointer(ty), *qualifiers)
                }
                DerivedDeclarator::Array { bound, .. } => self.intern(
                    TypeKind::Array {
                        element: ty,
                        bound: bound.clone(),
                    },
                    Qualifiers::default(),
                ),
                DerivedDeclarator::Function { parameters, .. } => {
                    let parameters = match parameters {
                        ParameterSyntax::Unspecified => FunctionParameters::Unspecified,
                        ParameterSyntax::IdentifierList { parameters } => {
                            FunctionParameters::Prototype {
                                types: parameters
                                    .iter()
                                    .map(|parameter| {
                                        let kind = evocc_chir::default_promoted_parameter_kind(
                                            &self.types,
                                            parameter.ty,
                                        )
                                        .expect("parsed parameter type exists");
                                        self.intern(kind, Qualifiers::default())
                                    })
                                    .collect(),
                                variadic: false,
                            }
                        }
                        ParameterSyntax::Prototype {
                            parameters,
                            variadic,
                        } => FunctionParameters::Prototype {
                            types: parameters.iter().map(|parameter| parameter.ty).collect(),
                            variadic: *variadic,
                        },
                    };
                    self.intern(
                        TypeKind::Function {
                            return_type: ty,
                            parameters,
                        },
                        Qualifiers::default(),
                    )
                }
            };
        }
        ty
    }

    pub(super) fn written_declarator(
        &self,
        declarator: &ParsedDeclarator<'arena>,
    ) -> WrittenDeclarator {
        self.written_derived(&declarator.derived, declarator.span)
    }

    pub(super) fn written_abstract_declarator(
        &self,
        declarator: &ParsedAbstractDeclarator<'arena>,
    ) -> WrittenDeclarator {
        self.written_derived(&declarator.derived, declarator.span)
    }

    pub(super) fn written_derived(
        &self,
        derived: &[DerivedDeclarator<'arena>],
        span: SourceSpan,
    ) -> WrittenDeclarator {
        WrittenDeclarator {
            layers: derived
                .iter()
                .map(|layer| match layer {
                    DerivedDeclarator::Pointer {
                        qualifiers,
                        attributes,
                        span,
                    } => WrittenDeclaratorLayer::Pointer {
                        qualifiers: *qualifiers,
                        attributes: attributes.clone(),
                        span: *span,
                    },
                    DerivedDeclarator::Array {
                        bound,
                        parameter_qualifiers,
                        static_bound,
                        span,
                        ..
                    } => WrittenDeclaratorLayer::Array {
                        bound: bound.clone(),
                        parameter_qualifiers: *parameter_qualifiers,
                        static_bound: *static_bound,
                        span: *span,
                    },
                    DerivedDeclarator::Function {
                        parameters, span, ..
                    } => WrittenDeclaratorLayer::Function {
                        parameters: Some(parameters.clone()),
                        span: *span,
                    },
                })
                .collect(),
            span,
        }
    }
}
