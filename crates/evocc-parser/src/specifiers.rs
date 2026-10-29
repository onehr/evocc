//! Parser specifiers.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_type_specifier_with_attributes(
        &mut self,
        attributes: &mut Vec<Attribute>,
    ) -> Option<ParsedSpecifier<'arena>> {
        self.parse_type_specifier_inner(None, None, Some(attributes), false)
    }

    pub(super) fn parse_declaration_type_specifier(
        &mut self,
        storage: &mut Option<StorageClass>,
        inline_spans: &mut Vec<SourceSpan>,
        attributes: &mut Vec<Attribute>,
    ) -> Option<ParsedSpecifier<'arena>> {
        if self.peek_kind() == Some(TokenKind::AutoType) {
            let keyword = self.advance().expect("peeked __auto_type");
            // The real type is installed transactionally after the one
            // required initializer has been parsed. This placeholder is
            // never allowed to escape a completed declaration.
            let placeholder = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
            return Some(ParsedSpecifier {
                surface: TypeSpecifier::AutoType {
                    resolved: placeholder,
                    span: keyword.span,
                },
                tag_event: None,
                inherited_function_parameters: None,
            });
        }
        self.parse_type_specifier_inner(Some(storage), Some(inline_spans), Some(attributes), true)
    }

    pub(super) fn parse_type_specifier_inner(
        &mut self,
        mut storage: Option<&mut Option<StorageClass>>,
        mut inline_spans: Option<&mut Vec<SourceSpan>>,
        mut specifier_attributes: Option<&mut Vec<Attribute>>,
        allow_implicit_integer: bool,
    ) -> Option<ParsedSpecifier<'arena>> {
        let start = self.span();
        let mut qualifiers = self.parse_qualifiers_with_inline(
            &mut storage,
            &mut inline_spans,
            &mut specifier_attributes,
        );
        if matches!(
            self.peek_kind(),
            Some(TokenKind::Struct | TokenKind::Union | TokenKind::Enum)
        ) {
            let token = self.advance().expect("peeked tag keyword");
            let kind = match token.kind {
                TokenKind::Struct => TagKind::Struct,
                TokenKind::Union => TagKind::Union,
                TokenKind::Enum => TagKind::Enum,
                _ => unreachable!("tag keyword checked above"),
            };
            // `enum E : T;` admits no storage class and no qualifier (C23 6.7.2.2).
            let bare = matches!(
                storage.as_deref(),
                None | Some(None | Some(StorageClass::None))
            ) && qualifiers == Qualifiers::default();
            let mut parsed = self.parse_tag_specifier(kind, qualifiers, token.span, bare)?;
            let trailing = self.parse_qualifiers_with_inline(
                &mut storage,
                &mut inline_spans,
                &mut specifier_attributes,
            );
            if trailing != Qualifiers::default() {
                let TypeSpecifier::Tag {
                    ty,
                    tag,
                    kind,
                    name,
                    span,
                } = parsed.surface
                else {
                    unreachable!("tag parser returns a tag specifier")
                };
                parsed.surface = TypeSpecifier::Tag {
                    ty: self.qualify_typedef_type(ty, trailing),
                    tag,
                    kind,
                    name,
                    span,
                };
            }
            return Some(parsed);
        }

        if self.at_atomic_type_specifier() {
            // C11 6.7.2.4: `_Atomic ( type-name )` names the atomic version of
            // a type that is not an array, function, atomic or qualified type.
            let keyword = self.advance().expect("peeked _Atomic specifier");
            self.expect(TokenKind::LParen)?;
            let (named, tag_event) = self.parse_type_name()?;
            let close = self.expect(TokenKind::RParen)?;
            let span = self.merge_span(keyword.span, close.span);
            let canonical = self.types.get(named)?.clone();
            let qualified = canonical.qualifiers.compatibility() != Qualifiers::default();
            if qualified
                || matches!(
                    canonical.kind,
                    TypeKind::Array { .. } | TypeKind::Function { .. }
                )
            {
                self.report("E2955", "invalid-atomic-type", span);
                return None;
            }
            let trailing = self.parse_qualifiers_with_inline(
                &mut storage,
                &mut inline_spans,
                &mut specifier_attributes,
            );
            qualifiers = merge_qualifiers(qualifiers, trailing);
            qualifiers.is_atomic = true;
            let resolved = self.qualify_typedef_type(named, qualifiers);
            return Some(ParsedSpecifier {
                surface: TypeSpecifier::Typeof {
                    resolved,
                    operand: TypeofOperand::TypeName,
                    span: self.merge_span(start, close.span),
                },
                tag_event,
                inherited_function_parameters: None,
            });
        }

        if matches!(
            self.peek_kind(),
            Some(TokenKind::Typeof | TokenKind::TypeofUnqual)
        ) {
            let keyword = self.advance().expect("peeked typeof keyword");
            let strip_top_level = keyword.kind == TokenKind::TypeofUnqual;
            self.expect(TokenKind::LParen)?;
            let (base, operand, tag_event, inherited_function_parameters) = if self
                .peek()
                .is_some_and(|token| self.token_starts_type_name(token))
            {
                let (ty, tag_event) = self.parse_type_name_with_vla_error(Some(keyword.span))?;
                (ty, TypeofOperand::TypeName, tag_event, None)
            } else {
                // Prior parameters belong to the enclosing declarator, not
                // to typeof's speculative scope. Keep their identities alive
                // when rolling back declarations inside the operand.
                self.install_temporary_parameter_shadows();
                let transaction = self.symbols.begin_transaction();
                let declaration_transaction = self.begin_declaration_transaction();
                let enclosing_scope = self.current_scope;
                self.current_scope = self.create_scope(ScopeKind::Block, Some(keyword.span));
                let expression = self.parse_expression();
                let inferred = expression.and_then(|expression| {
                    let inherited = match expression {
                        CHirExpr::Ident { symbol, .. } => {
                            self.inherited_function_parameters.get(symbol).cloned()
                        }
                        _ => None,
                    };
                    self.expression_type(expression)
                        .map(|inferred| (expression, inferred.ty, inherited))
                });
                let resolved = inferred.and_then(|(expression, inferred, inherited)| {
                    self.materialize_inferred_type(inferred, Some(expression))
                        .map(|ty| (expression, ty, inherited))
                });
                self.current_scope = enclosing_scope;
                let evaluated = resolved
                    .as_ref()
                    .is_some_and(|(_, ty, _)| self.type_is_variably_modified(*ty));
                if evaluated {
                    self.symbols
                        .commit_transaction(&transaction)
                        .expect("typeof transaction is table-local and LIFO");
                } else {
                    self.symbols
                        .rollback_transaction(&transaction)
                        .expect("temporary typeof bindings are table-local and LIFO");
                    self.rollback_declaration_transaction(&declaration_transaction);
                }
                let Some((expression, ty, inherited)) = resolved else {
                    if self.diagnostics.is_empty() {
                        self.report("E1003", "expected-expression", self.span());
                    }
                    return None;
                };
                if evaluated {
                    // A VM expression operand executes once. Reuse a hidden
                    // bound capture to sequence it before the declaration;
                    // its operand-local declarations remain in their own scope.
                    let one = self.size_literal(1, keyword.span);
                    let value = self.alloc_node(CHirExpr::Comma(expression, one, keyword.span));
                    let mut layer = [DerivedDeclarator::Array {
                        bound: ArrayBound::Variable,
                        expression: Some(value),
                        prototype_star: false,
                        parameter_qualifiers: Qualifiers::default(),
                        static_bound: false,
                        span: keyword.span,
                    }];
                    let captures = self.capture_array_bounds(&mut layer)?;
                    self.side_declarations.extend(captures);
                }
                (ty, TypeofOperand::Expression, None, inherited)
            };
            let close = self.expect(TokenKind::RParen)?;
            let base = if strip_top_level {
                self.strip_top_level_qualifiers(base)
            } else {
                base
            };
            let trailing = self.parse_qualifiers_with_inline(
                &mut storage,
                &mut inline_spans,
                &mut specifier_attributes,
            );
            qualifiers = merge_qualifiers(qualifiers, trailing);
            if qualifiers.is_atomic {
                self.require_atomic_object_type(base, self.merge_span(start, close.span))?;
            }
            let resolved = self.qualify_typedef_type(base, qualifiers);
            return Some(ParsedSpecifier {
                surface: TypeSpecifier::Typeof {
                    resolved,
                    operand,
                    span: self.merge_span(start, close.span),
                },
                tag_event,
                inherited_function_parameters,
            });
        }

        if let Some((symbol, alias_ty)) = self.peek_typedef_name() {
            let token = self.advance().expect("peeked typedef name");
            let name = SpannedName {
                spelling: self.token_text(&token).to_owned(),
                span: token.span,
            };
            if let Some(reason) = self.symbol_unavailable(symbol) {
                self.use_unavailable(&name.spelling, &reason, token.span);
            }
            let trailing = self.parse_qualifiers_with_inline(
                &mut storage,
                &mut inline_spans,
                &mut specifier_attributes,
            );
            qualifiers = merge_qualifiers(qualifiers, trailing);
            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map_or(token.span, |token| token.span);
            if qualifiers.is_atomic {
                self.require_atomic_object_type(alias_ty, self.merge_span(start, end))?;
            }
            let ty = self.qualify_typedef_type(alias_ty, qualifiers);
            return Some(ParsedSpecifier {
                surface: TypeSpecifier::TypedefName {
                    ty,
                    symbol,
                    name,
                    span: self.merge_span(start, end),
                },
                tag_event: None,
                inherited_function_parameters: self
                    .inherited_function_parameters
                    .get(&symbol)
                    .cloned(),
            });
        }

        let mut words = Vec::new();
        let mut end = None;
        while let Some(kind) = self.peek_kind() {
            if self.capture_storage_specifier(&mut storage) {
                continue;
            }
            if self.capture_inline_specifier(&mut inline_spans) {
                continue;
            }
            if self.capture_specifier_attribute(&mut specifier_attributes) {
                continue;
            }
            if is_builtin_type_word(kind) {
                let token = self.advance().expect("peeked builtin type word");
                if self.token_text(&token) == "__mfp8"
                    && !self.target_layout.names_clang_extensions()
                {
                    self.report(
                        "E1007",
                        "invalid-type-specifiers: __mfp8 is unavailable on this target",
                        token.span,
                    );
                    return None;
                }
                let forced_int128_signedness = match (kind, self.token_text(&token)) {
                    (TokenKind::GnuInt128, "__int128_t") => Some(GnuInt128Signedness::Signed),
                    (TokenKind::GnuInt128, "__uint128_t") => Some(GnuInt128Signedness::Unsigned),
                    _ => None,
                };
                words.push(BuiltinTypeWord {
                    kind,
                    forced_int128_signedness,
                    target_builtin: TargetBuiltinType::from_spelling(self.token_text(&token)),
                    target_vector: evocc_types::neon::BuiltinType::from_spelling(
                        self.token_text(&token),
                    ),
                });
                end = Some(token.span);
                continue;
            }
            let qualifier = match kind {
                TokenKind::Const => Some((&mut qualifiers.is_const, true)),
                TokenKind::Volatile => Some((&mut qualifiers.is_volatile, true)),
                TokenKind::Restrict => Some((&mut qualifiers.is_restrict, true)),
                TokenKind::Atomic
                    if self.tokens.get(self.pos + 1).map(|token| token.kind)
                        == Some(TokenKind::LParen) =>
                {
                    None
                }
                TokenKind::Atomic => Some((&mut qualifiers.is_atomic, true)),
                _ => None,
            };
            let Some((slot, value)) = qualifier else {
                break;
            };
            *slot = value;
            end = self.advance().map(|token| token.span);
        }
        if words.is_empty() {
            if allow_implicit_integer
                && (self.legacy_declarations || self.lint_declarations)
                && matches!(
                    self.peek_kind(),
                    Some(TokenKind::Identifier | TokenKind::Star | TokenKind::LParen)
                )
            {
                self.lint_findings.push(evocc_chir::lints::Finding {
                    lint: 3,
                    span: self.span(),
                });
                let ty = self.intern(TypeKind::Int(IntRank::Int), qualifiers);
                return Some(ParsedSpecifier {
                    surface: TypeSpecifier::Builtin { ty, span: start },
                    tag_event: None,
                    inherited_function_parameters: None,
                });
            }
            return None;
        }
        let end = end.expect("a builtin type word established an end span");
        let resolved = if let [BuiltinTypeWord {
            target_vector: Some(target),
            ..
        }] = words.as_slice()
        {
            Some(target.kind(&mut self.types))
        } else {
            resolve_builtin_type(&words)
        };
        let Some(kind) = resolved else {
            self.report(
                "E1007",
                "invalid-type-specifier-combination",
                self.merge_span(start, end),
            );
            return None;
        };
        let ty = self.intern(kind, qualifiers);
        Some(ParsedSpecifier {
            surface: TypeSpecifier::Builtin {
                ty,
                span: self.merge_span(start, end),
            },
            tag_event: None,
            inherited_function_parameters: None,
        })
    }

    pub(super) fn capture_storage_specifier(
        &mut self,
        storage: &mut Option<&mut Option<StorageClass>>,
    ) -> bool {
        let Some(slot) = storage.as_deref_mut() else {
            return false;
        };
        if !matches!(
            self.peek_kind(),
            Some(
                TokenKind::Typedef
                    | TokenKind::Extern
                    | TokenKind::Static
                    | TokenKind::Auto
                    | TokenKind::Register
                    | TokenKind::ThreadLocal
            )
        ) {
            return false;
        }
        let next_kind = self.peek_kind();
        if let Some(prior) = *slot {
            let combined = if next_kind == Some(TokenKind::ThreadLocal) {
                prior.with_thread_local()
            } else if prior == StorageClass::ThreadLocal {
                match next_kind {
                    Some(TokenKind::Static) => Some(StorageClass::StaticThreadLocal),
                    Some(TokenKind::Extern) => Some(StorageClass::ExternThreadLocal),
                    _ => None,
                }
            } else {
                None
            };
            let Some(combined) = combined else {
                return false;
            };
            self.advance();
            *slot = Some(combined);
        } else {
            *slot = Some(self.parse_storage_class());
        }
        true
    }

    pub(super) fn capture_inline_specifier(
        &mut self,
        inline_spans: &mut Option<&mut Vec<SourceSpan>>,
    ) -> bool {
        if inline_spans.is_none() || self.peek_kind() != Some(TokenKind::Inline) {
            return false;
        }
        let span = self.advance().expect("peeked inline specifier").span;
        inline_spans
            .as_mut()
            .expect("inline capture was enabled")
            .push(span);
        true
    }

    pub(super) fn parse_qualifiers_with_inline(
        &mut self,
        storage: &mut Option<&mut Option<StorageClass>>,
        inline_spans: &mut Option<&mut Vec<SourceSpan>>,
        specifier_attributes: &mut Option<&mut Vec<Attribute>>,
    ) -> Qualifiers {
        let mut qualifiers = Qualifiers::default();
        loop {
            if self.capture_storage_specifier(storage) {
                continue;
            }
            if self.capture_inline_specifier(inline_spans) {
                continue;
            }
            if self.capture_specifier_attribute(specifier_attributes) {
                continue;
            }
            match self.peek_kind() {
                Some(TokenKind::Const) => qualifiers.is_const = true,
                Some(TokenKind::Volatile) => qualifiers.is_volatile = true,
                Some(TokenKind::Restrict) => qualifiers.is_restrict = true,
                // C11 6.7.2.4p4: `_Atomic (` begins the type specifier.
                Some(TokenKind::Atomic) if self.at_atomic_type_specifier() => break,
                Some(TokenKind::Atomic) => qualifiers.is_atomic = true,
                _ => break,
            }
            self.advance();
        }
        qualifiers
    }

    /// Whether the next tokens are `_Atomic (`, which C11 6.7.2.4p4 makes
    /// the atomic type specifier rather than the qualifier.
    pub(super) fn at_atomic_type_specifier(&self) -> bool {
        self.peek_kind() == Some(TokenKind::Atomic)
            && self.tokens.get(self.pos + 1).map(|token| token.kind) == Some(TokenKind::LParen)
    }

    /// Report `_Atomic` applied to an array or function type, which C11
    /// 6.7.3p3 forbids, as `E2955`.
    pub(super) fn require_atomic_object_type(
        &mut self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Option<()> {
        let canonical = self.types.get(ty)?;
        if matches!(
            canonical.kind,
            TypeKind::Array { .. } | TypeKind::Function { .. }
        ) {
            self.report("E2955", "invalid-atomic-type", span);
            return None;
        }
        Some(())
    }

    pub(super) fn capture_specifier_attribute(
        &mut self,
        attributes: &mut Option<&mut Vec<Attribute>>,
    ) -> bool {
        if attributes.is_none() {
            return false;
        }
        if self.peek_kind() == Some(TokenKind::Alignas) {
            if let Some(attribute) = self.parse_alignment_specifier() {
                attributes
                    .as_mut()
                    .expect("attribute capture enabled")
                    .push(attribute);
            }
            return true;
        }
        if let Some(attribute) = self.parse_noreturn_specifier() {
            attributes
                .as_mut()
                .expect("attribute capture enabled")
                .push(attribute);
            return true;
        }
        if !self.attribute_starts_at(self.pos) {
            return false;
        }
        let bracketed = self.bracket_attribute_starts_at(self.pos);
        let previous = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|token| token.kind);
        if bracketed
            && previous.is_some_and(|kind| {
                matches!(
                    kind,
                    TokenKind::Static
                        | TokenKind::Extern
                        | TokenKind::Typedef
                        | TokenKind::Register
                        | TokenKind::Auto
                        | TokenKind::Inline
                        | TokenKind::ThreadLocal
                        | TokenKind::Const
                        | TokenKind::Volatile
                        | TokenKind::Restrict
                        | TokenKind::Atomic
                )
            })
        {
            self.report("E2903", "malformed-attribute", self.span());
        }
        let placement = if bracketed && previous.is_some_and(is_builtin_type_word) {
            AttributePlacement::TypeName
        } else {
            AttributePlacement::DeclarationSpecifiers
        };
        let parsed = self.parse_attribute_specifiers(placement);
        attributes
            .as_mut()
            .expect("specifier attribute capture was enabled")
            .extend(parsed);
        true
    }

    pub(super) fn parse_noreturn_specifier(&mut self) -> Option<Attribute> {
        if self.peek_kind() != Some(TokenKind::Noreturn) {
            return None;
        }
        let token = self.advance().expect("peeked function specifier");
        Some(Attribute {
            name: "noreturn".into(),
            written: "_Noreturn".into(),
            syntax: AttributeSyntax::Gnu,
            arguments: Vec::new(),
            placement: AttributePlacement::DeclarationSpecifiers,
            span: token.span,
            applied: None,
        })
    }

    pub(super) fn parse_alignment_specifier(&mut self) -> Option<Attribute> {
        let keyword = self.expect(TokenKind::Alignas)?;
        self.expect(TokenKind::LParen)?;
        let argument_start = self.pos;
        let alignment = if self
            .peek()
            .is_some_and(|token| self.token_starts_type_name(token))
        {
            let (ty, tag_event) = self.parse_unevaluated_type_name()?;
            if let Some(event) = tag_event {
                let declaration = self.finish_tag_event(event, keyword.span, Vec::new());
                self.side_declarations.push(declaration);
            }
            self.target_layout
                .alignof_type(&self.types, ty)
                .ok()
                .map(u64::from)
        } else {
            let expression = self.parse_conditional()?;
            self.eval_integer_constant_expression(expression)
                .ok()
                .and_then(CInteger::try_mathematical_value)
                .and_then(|value| u64::try_from(value).ok())
        };
        let argument_end = self.pos;
        let close = self.expect(TokenKind::RParen)?;
        let Some(alignment) =
            alignment.filter(|value| *value == 0 || (value.is_power_of_two() && *value <= 1 << 28))
        else {
            self.report("E2938", "invalid-alignment-specifier", keyword.span);
            return None;
        };
        let mut argument = self.attribute_argument(&self.tokens[argument_start..argument_end]);
        argument.integer = Some(alignment);
        Some(Attribute {
            name: "aligned".into(),
            written: "_Alignas".into(),
            syntax: AttributeSyntax::Gnu,
            arguments: vec![argument],
            placement: AttributePlacement::DeclarationSpecifiers,
            span: self.merge_span(keyword.span, close.span),
            applied: None,
        })
    }

    pub(super) fn validate_alignment_specifiers(
        &mut self,
        ty: TypeId,
        attributes: &[Attribute],
        allowed: bool,
    ) -> bool {
        let mut specifiers = attributes
            .iter()
            .filter(|attribute| attribute.written == "_Alignas");
        let Some(first) = specifiers.next() else {
            return true;
        };
        let requested = std::iter::once(first)
            .chain(specifiers)
            .filter_map(|attribute| attribute.arguments.first()?.integer)
            .max()
            .unwrap_or(0);
        let target = self.target_layout;
        let natural = match self.types.get(ty) {
            Some(CanonicalType {
                kind:
                    TypeKind::Array {
                        element,
                        bound: ArrayBound::Unbounded,
                    },
                qualifiers,
            }) => qualifiers.aligned.or_else(|| {
                target
                    .layout_of(&self.types, *element)
                    .ok()
                    .map(|layout| layout.align_bytes)
            }),
            _ => target
                .layout_of(&self.types, ty)
                .ok()
                .map(|layout| layout.align_bytes),
        }
        .map(u64::from);
        if !allowed || (requested != 0 && natural.is_some_and(|natural| requested < natural)) {
            self.report("E2938", "invalid-alignment-specifier", first.span);
            return false;
        }
        true
    }

    pub(super) fn parse_qualifiers(&mut self) -> Qualifiers {
        let mut qualifiers = Qualifiers::default();
        loop {
            match self.peek_kind() {
                Some(TokenKind::Const) => qualifiers.is_const = true,
                Some(TokenKind::Volatile) => qualifiers.is_volatile = true,
                Some(TokenKind::Restrict) => qualifiers.is_restrict = true,
                Some(TokenKind::Atomic) => qualifiers.is_atomic = true,
                _ => break,
            }
            self.advance();
        }
        qualifiers
    }

    /// Darwin accepts nullability keywords between a pointer `*` and its
    /// qualifiers. Qualifiers after the keyword still apply. A second
    /// nullability keyword at the same pointer level conflicts.
    pub(super) fn parse_darwin_pointer_qualifiers(
        &mut self,
        mut qualifiers: Qualifiers,
    ) -> Qualifiers {
        let mut saw_nullability = false;
        loop {
            if !self
                .peek()
                .is_some_and(|token| nullability_keyword(self.token_text(token)))
            {
                break;
            }
            let keyword = self.advance().expect("peeked nullability keyword");
            if saw_nullability {
                self.report("E1001", "expected-declaration", keyword.span);
            }
            saw_nullability = true;
            let trailing = self.parse_qualifiers();
            qualifiers.is_const |= trailing.is_const;
            qualifiers.is_volatile |= trailing.is_volatile;
            qualifiers.is_restrict |= trailing.is_restrict;
            qualifiers.is_atomic |= trailing.is_atomic;
            qualifiers.aligned = qualifiers.aligned.or(trailing.aligned);
        }
        qualifiers
    }
}
