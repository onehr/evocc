//! Parser primary.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_postfix(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let expression = self.parse_primary()?;
        self.parse_postfix_tail(expression)
    }

    pub(super) fn function_has_incomplete_enum_return(
        &self,
        expression: &CHirExpr<'arena>,
    ) -> bool {
        let Some(inferred) = self.converted_expression_type(expression) else {
            return false;
        };
        let OperandTypeClass::Pointer(pointer) = self.operand_type_class(inferred.ty) else {
            return false;
        };
        let Some(TypeKind::Function { return_type, .. }) =
            self.types.get(pointer.pointee).map(|ty| &ty.kind)
        else {
            return false;
        };
        let Some(TypeKind::Enum(tag)) = self.types.get(*return_type).map(|ty| &ty.kind) else {
            return false;
        };
        self.types
            .tag(*tag)
            .is_some_and(|tag| matches!(tag.state, TagState::Incomplete))
    }

    pub(super) fn parse_postfix_tail(
        &mut self,
        mut expression: &'arena CHirExpr<'arena>,
    ) -> Option<&'arena CHirExpr<'arena>> {
        loop {
            match self.peek_kind() {
                Some(TokenKind::LParen) => {
                    if self.function_has_incomplete_enum_return(expression) {
                        self.report(
                            "E2930",
                            "use-with-incomplete-enumeration",
                            expression.span(),
                        );
                        return None;
                    }
                    if let Some(designator) = self.old_style_call_designator(expression) {
                        expression = designator;
                    }
                    let parameters =
                        self.converted_expression_type(expression)
                            .and_then(|inferred| {
                                let OperandTypeClass::Pointer(pointer) =
                                    self.operand_type_class(inferred.ty)
                                else {
                                    return None;
                                };
                                match self.types.get(pointer.pointee).map(|ty| &ty.kind) {
                                    Some(TypeKind::Function { parameters, .. }) => {
                                        Some(parameters.clone())
                                    }
                                    _ => None,
                                }
                            });
                    self.advance();
                    let (mut arguments, pack) = self.parse_call_arguments()?;
                    let end = self
                        .peek()
                        .map(|token| token.span)
                        .unwrap_or_else(|| self.span());
                    self.expect(TokenKind::RParen)?;
                    let span = self.merge_span(expression.span(), end);
                    if let Some(pack) = pack {
                        // GNU forwards the pack only to a variadic prototype,
                        // in the position of its anonymous arguments.
                        let forwards = matches!(&parameters,
                            Some(FunctionParameters::Prototype { types, variadic: true })
                                if types.len() <= arguments.len());
                        if !forwards {
                            self.report("E2950", "invalid-va-arg-pack", pack.span());
                            return None;
                        }
                    }
                    if let Some(symbol) = self.standard_allocation_target(expression) {
                        self.standard_allocation_calls
                            .push((symbol, expression.span()));
                        expression = self.resolve_builtin_library_call(
                            "__builtin_alloca",
                            arguments,
                            span,
                            expression.span(),
                        )?;
                        continue;
                    }
                    let Some(parameters) = parameters else {
                        self.report("E2937", "invalid-call-target", expression.span());
                        return None;
                    };
                    self.validate_neon_call(expression, &arguments, &parameters)?;
                    if let FunctionParameters::Prototype { types, .. } = &parameters {
                        for (argument, parameter) in arguments.iter_mut().zip(types) {
                            *argument = self.resolve_union_argument(*parameter, argument)?;
                            if self.incompatible_pointer_conversion(*parameter, argument)
                                && self.record_incompatible_pointers(argument.span())
                            {
                                *argument = self.incompatible_pointer_cast(*parameter, argument);
                            }
                            if self.assignment_constraint(
                                AssignmentOp::Simple,
                                *parameter,
                                argument,
                            ) == UpdateConstraint::Invalid
                            {
                                self.report("E2202", "invalid-initializer-type", argument.span());
                                return None;
                            }
                            *argument =
                                self.explicit_floating_integer_conversion(*parameter, argument);
                        }
                    }
                    let default_promoted_from = match parameters {
                        FunctionParameters::Prototype {
                            types,
                            variadic: true,
                        } => Some(types.len()),
                        FunctionParameters::Unspecified => Some(0),
                        FunctionParameters::Prototype { .. } => None,
                    };
                    let promoted_parameters = match default_promoted_from {
                        Some(fixed) if fixed <= arguments.len() => {
                            self.promote_call_arguments(&mut arguments[fixed..])?
                        }
                        _ => Vec::new(),
                    };
                    if let CHirExpr::Ident { symbol, facts, .. } = expression {
                        if matches!(
                            self.types.get(facts.ty()).map(|ty| &ty.kind),
                            Some(TypeKind::Function {
                                parameters: FunctionParameters::Unspecified,
                                ..
                            })
                        ) {
                            self.unprototyped_calls
                                .push((*symbol, promoted_parameters, span));
                        }
                    }
                    expression = if let Some(ty) = self.standard_absolute_type(expression) {
                        self.resolve_absolute_builtin(ty, &arguments, span)?
                    } else if let Some(name) = self.standard_floating_builtin(expression) {
                        self.resolve_floating_builtin(name, arguments, span)?
                    } else {
                        arguments.extend(pack);
                        self.alloc_node(CHirExpr::Call(expression, arguments, span))
                    };
                }
                Some(TokenKind::LBracket) => {
                    self.advance();
                    let index = self.parse_expression()?;
                    let end = self
                        .peek()
                        .map(|token| token.span)
                        .unwrap_or_else(|| self.span());
                    self.expect(TokenKind::RBracket)?;
                    let span = self.merge_span(expression.span(), end);
                    if let Some((_, element, _)) = self.vector_expression_type(expression) {
                        let source_is_lvalue = matches!(expression, CHirExpr::MemoryRead { .. })
                            || self
                                .expression_type(expression)
                                .is_some_and(|ty| ty.category == ExprCategory::LValue);
                        if !source_is_lvalue {
                            if self.promoted_subscript_type(index).is_none() {
                                self.report("E2102", "invalid-vector-subscript", index.span());
                                return None;
                            }
                            expression = self.vector_node(
                                evocc_chir::vector::VectorOp::Extract,
                                vec![expression, index],
                                element,
                                span,
                            );
                            continue;
                        }
                    }
                    self.note_pointer_arithmetic_operands(expression, index, span);
                    let subscript = self.alloc_node(CHirExpr::Index(expression, index, span));
                    expression = self.convert_projected_lvalue(subscript);
                }
                Some(TokenKind::Dot) | Some(TokenKind::Arrow) => {
                    let pointer = self.peek_kind() == Some(TokenKind::Arrow);
                    self.advance();
                    let Some(member) = self.advance() else {
                        self.report("E1003", "expected-expression", self.span());
                        break;
                    };
                    if member.kind != TokenKind::Identifier {
                        self.report("E1003", "expected-expression", member.span);
                        break;
                    }
                    let name = self.token_text(&member).to_owned();
                    let record_ty = if pointer {
                        self.converted_expression_type(expression)
                            .and_then(|inferred| match self.operand_type_class(inferred.ty) {
                                OperandTypeClass::Pointer(pointer) => Some(pointer.pointee),
                                _ => None,
                            })
                    } else {
                        self.expression_type(expression)
                            .and_then(|inferred| match inferred.ty {
                                InferredType::Canonical(ty) => Some(ty),
                                _ => None,
                            })
                    };
                    // Resolve the name while its own span is available. This
                    // also checks members of record-valued expressions, which
                    // do not pass through the lvalue projection path. An
                    // incomplete tag has no member inventory yet; leave its
                    // existing incomplete-projection diagnostic unchanged.
                    if record_ty.is_some_and(|ty| {
                        let tag = match self.types.get(ty).map(|ty| &ty.kind) {
                            Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => *tag,
                            _ => return false,
                        };
                        self.types
                            .tag(tag)
                            .is_some_and(|tag| matches!(tag.state, TagState::Complete(_)))
                            && self.record_member_path(ty, &name).is_none()
                    }) {
                        self.report("E2906", "designator-outside-object", member.span);
                        return None;
                    }
                    let unavailable = self.availability_target.and_then(|deployment| {
                        let last = *record_ty
                            .and_then(|ty| self.record_member_path(ty, &name))?
                            .last()?;
                        self.member_availability
                            .get(&(last.tag(), last.field_index()))?
                            .unavailable(deployment)
                    });
                    if let Some(reason) = unavailable {
                        self.use_unavailable(&name, &reason, member.span);
                    }
                    let span = self.merge_span(expression.span(), member.span);
                    let projected = if pointer {
                        self.alloc_node(CHirExpr::PtrMember(expression, name, span))
                    } else {
                        self.alloc_node(CHirExpr::Member(expression, name, span))
                    };
                    expression = self.convert_projected_lvalue(projected);
                }
                Some(TokenKind::PlusPlus) | Some(TokenKind::MinusMinus) => {
                    let token = self.advance().unwrap();
                    let target = expression;
                    self.require_supported_update_promotion(target)?;
                    let typed_target = self.typed_update_target(target)?;
                    self.require_admitted_atomic_update(&typed_target, true, target.span())?;
                    if let OperandTypeClass::Pointer(pointer) =
                        self.operand_type_class(InferredType::Canonical(typed_target.ty()))
                    {
                        self.note_pointer_arithmetic_pointee(pointer.pointee, target.span());
                    }
                    let op = if token.kind == TokenKind::PlusPlus {
                        UpdateOp::PostIncrement
                    } else {
                        UpdateOp::PostDecrement
                    };
                    let span = self.merge_span(target.span(), token.span);
                    match self.update_constraint(typed_target.ty()) {
                        UpdateConstraint::Scalar | UpdateConstraint::PointerBoundary => {}
                        UpdateConstraint::AggregateBoundary | UpdateConstraint::Invalid => {
                            self.report("E2102", "invalid-update-operands", span);
                            return None;
                        }
                    }
                    expression = match typed_target {
                        TypedUpdateTarget::Direct(target_identity) => {
                            let place =
                                self.finish_scalar_update_place(target_identity, target.span())?;
                            self.alloc_node(CHirExpr::Update {
                                op,
                                place,
                                target,
                                facts: ExprFacts::value(place.ty()),
                                span,
                            })
                        }
                        TypedUpdateTarget::Memory(place) => {
                            let facts = ExprFacts::value(place.value_ty());
                            self.alloc_node(CHirExpr::MemoryUpdate {
                                op,
                                place,
                                target_span: target.span(),
                                facts,
                                span,
                            })
                        }
                    };
                }
                _ => break,
            }
        }
        Some(expression)
    }

    pub(super) fn parse_primary(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let Some(tok) = self.advance() else {
            self.report("E1003", "expected-expression", self.span());
            return None;
        };
        let expression = match tok.kind {
            TokenKind::ImaginaryIntLiteral => {
                let text = self.token_text(&tok);
                let real = evocc_lex::imaginary_integer_spelling(text)
                    .expect("the lexer classified an imaginary integer constant");
                let literal = parse_int_literal(&real)?;
                let rank = rank_for_integer_type(integer_literal_value(literal).ok()?.ty())?;
                let ty = self.intern(
                    TypeKind::Complex(ComplexBase::Integer(rank)),
                    Qualifiers::default(),
                );
                let value = self.alloc_node(CHirExpr::Lit(literal, tok.span));
                return Some(self.complex_node(
                    evocc_chir::complex::ComplexOp::Imaginary,
                    vec![value],
                    ty,
                    tok.span,
                ));
            }
            TokenKind::IntLiteral => {
                let text = self.token_text(&tok);
                let Some(literal) = parse_int_literal(text) else {
                    self.report("E1006", "invalid-integer-literal", tok.span);
                    return None;
                };
                CHirExpr::Lit(literal, tok.span)
            }
            TokenKind::Identifier => {
                let spelling = self.token_text(&tok).to_owned();
                if spelling == "__builtin_bit_cast" && self.target_layout.names_clang_extensions() {
                    return self.parse_neon_bit_cast(tok);
                }
                if matches!(
                    spelling.as_str(),
                    "__builtin_shuffle" | "__builtin_shufflevector" | "__builtin_convertvector"
                ) {
                    return self.parse_vector_builtin(&spelling, tok);
                }
                if matches!(
                    spelling.as_str(),
                    "__builtin_va_arg_pack" | "__builtin_va_arg_pack_len"
                ) {
                    return self.parse_builtin_va_arg_pack_length(&spelling, tok);
                }
                let resolved_spelling = implicit_function_name(&spelling).unwrap_or(&spelling);
                let resolved = self
                    .resolve_expression_symbol(resolved_spelling)
                    .or_else(|| self.resolve_neon_builtin(resolved_spelling, tok.span));
                // Where Clang's `<stdatomic.h>` is the header, Clang's other
                // `__c11_atomic_*` spellings are builtins EvoCC lacks.
                let reserved = reserved_builtin_spelling(resolved_spelling)
                    || (self.target_layout.provides_clang_atomic_builtins()
                        && resolved_spelling.starts_with("__c11_atomic_"));
                let clang_extensions = self.target_layout.names_clang_extensions();
                if resolved.is_none()
                    && clang_extensions
                    && self.peek_kind() == Some(TokenKind::LParen)
                {
                    // Clang's `__builtin_os_log_format` lays out the buffer
                    // of `os_log`; EvoCC has no model of it (EvoIR 10.1.14).
                    if matches!(
                        resolved_spelling,
                        "__builtin_os_log_format" | "__builtin_os_log_format_buffer_size"
                    ) {
                        self.report(
                            "E2965",
                            format!("unsupported-os-log-builtin: {spelling}"),
                            tok.span,
                        );
                        return None;
                    }
                    // Clang's ACLE builtins of `<arm_acle.h>`.
                    if resolved_spelling.starts_with("__builtin_arm_") {
                        return self.parse_acle_builtin(tok);
                    }
                }
                if resolved.is_none() && reserved && self.peek_kind() == Some(TokenKind::LParen) {
                    // An implicit declaration cannot supply a builtin. Name
                    // the builtin where it is spelled, before any lint about
                    // the call that it would have declared; `-w` and the
                    // language mode do not change this.
                    self.report("E2949", format!("unknown-builtin: {spelling}"), tok.span);
                    return None;
                }
                let Some(symbol) = resolved.or_else(|| {
                    ((self.legacy_declarations || self.lint_declarations)
                        && self.peek_kind() == Some(TokenKind::LParen))
                    .then(|| self.declare_implicit_called_function(resolved_spelling, tok.span))
                }) else {
                    self.report(
                        "E2001",
                        format!("undeclared-identifier: {spelling}"),
                        tok.span,
                    );
                    return None;
                };
                if self
                    .symbols
                    .symbol(symbol)
                    .expect("resolved symbol belongs to the same table")
                    .kind()
                    == SymbolKind::Typedef
                {
                    self.report(
                        "E2003",
                        format!("typedef-name-not-expression: {spelling}"),
                        tok.span,
                    );
                    return None;
                }
                self.lint_uses.insert(symbol);
                self.note_function_use(symbol, tok.span);
                if let Some(reason) = self.symbol_unavailable(symbol) {
                    self.use_unavailable(resolved_spelling, &reason, tok.span);
                }
                // `<arm_acle.h>` declares these ACLE intrinsics, which Clang
                // compiles to instructions; there is no library function.
                if clang_extensions
                    && ACLE_FUNCTIONS.contains(&resolved_spelling)
                    && self
                        .symbols
                        .symbol(symbol)
                        .is_ok_and(|entry| entry.kind() == SymbolKind::Function)
                {
                    self.defer_profile_refusal("E2966", "unsupported-acle-builtin", tok.span);
                }
                let facts = self.facts_for_symbol(symbol);
                CHirExpr::Ident {
                    symbol,
                    spelling: resolved_spelling.to_owned(),
                    facts,
                    span: tok.span,
                }
            }
            TokenKind::FloatLiteral => {
                let Some(literal) = parse_float_literal(self.token_text(&tok), self.target_layout)
                else {
                    self.report("E1006", "invalid-floating-literal", tok.span);
                    return None;
                };
                CHirExpr::FloatLit(literal, tok.span)
            }
            TokenKind::ImaginaryFloatLiteral => {
                // GNU `2.5i`: the real part is +0 and the imaginary part is
                // the literal, rounded once to its format.
                let literal = evocc_lex::imaginary_floating_spelling(self.token_text(&tok))
                    .and_then(|text| parse_float_literal(&text, self.target_layout));
                let Some(literal) = literal else {
                    self.report("E1006", "invalid-floating-literal", tok.span);
                    return None;
                };
                let kind = match literal.suffix {
                    FloatSuffix::F => FloatKind::F32,
                    FloatSuffix::None => FloatKind::F64,
                    FloatSuffix::L => FloatKind::F128,
                    FloatSuffix::F16 | FloatSuffix::BF16 => {
                        self.report("E1006", "invalid-floating-literal", tok.span);
                        return None;
                    }
                };
                let ty = self.intern(
                    TypeKind::Complex(complex_base_for_float(kind)),
                    Qualifiers::default(),
                );
                let value = self.alloc_node(CHirExpr::FloatLit(literal, tok.span));
                return Some(self.complex_node(
                    evocc_chir::complex::ComplexOp::Imaginary,
                    vec![value],
                    ty,
                    tok.span,
                ));
            }
            TokenKind::CharLiteral => {
                let Some(literal) = parse_char_literal(self.token_text(&tok), self.target_layout)
                else {
                    self.report("E1006", "invalid-character-literal", tok.span);
                    return None;
                };
                CHirExpr::CharLit(literal, tok.span)
            }
            TokenKind::StringLiteral => {
                let Some(mut literal) = parse_string_literal(self.token_text(&tok)) else {
                    self.report("E1006", "invalid-string-literal", tok.span);
                    return None;
                };
                let mut end = tok.span;
                let mut raw_bytes = tok.text.has_raw_bytes();
                while self.peek_kind() == Some(TokenKind::StringLiteral) {
                    let next = self.advance().expect("peeked adjacent string literal");
                    raw_bytes |= next.text.has_raw_bytes();
                    let Some(fragment) = parse_string_literal(self.token_text(&next)) else {
                        self.report("E1006", "invalid-string-literal", next.span);
                        return None;
                    };
                    let Some(prefix) = concatenate_string_prefix(literal.prefix, fragment.prefix)
                    else {
                        self.report("E1006", "incompatible-string-literal-prefixes", next.span);
                        return None;
                    };
                    literal.prefix = prefix;
                    if raw_bytes && prefix != StringPrefix::None {
                        self.report(
                            "E0002",
                            "invalid encoding in prefixed string literal",
                            next.span,
                        );
                        return None;
                    }
                    literal.value.push_str(&fragment.value);
                    literal.fragments.extend(fragment.fragments);
                    end = next.span;
                }
                let span = self.merge_span(tok.span, end);
                if literal.prefix == StringPrefix::L {
                    literal.prefix = match self.target_layout.wide_character_rank() {
                        IntRank::UShort => StringPrefix::L16,
                        IntRank::Int => StringPrefix::LInt,
                        _ => StringPrefix::L,
                    };
                }
                if literal.explicit_unit_count().is_none() {
                    self.report("E1006", "invalid-string-literal", span);
                    return None;
                }
                let element = self.string_element_type(literal.prefix);
                self.intern(
                    TypeKind::Array {
                        element,
                        bound: ArrayBound::Fixed(literal.explicit_unit_count()?.checked_add(1)?),
                    },
                    Qualifiers::default(),
                );
                return Some(self.alloc_node(CHirExpr::StringLit(literal, span)));
            }
            TokenKind::LParen => {
                if self.peek_kind() == Some(TokenKind::LBrace) {
                    if !self.in_function_scope() {
                        self.report("E2905", "statement-expression-outside-function", tok.span);
                        return None;
                    }
                    let body = self.parse_compound_statement(true);
                    let close = self.expect(TokenKind::RParen)?;
                    let value = match body {
                        CHirStmt::Compound(items, _, _) => {
                            items.last().and_then(|item| match item {
                                CHirBlockItem::Statement(statement) => {
                                    statement_expression_value(statement)
                                }
                                CHirBlockItem::Declaration(_) => None,
                            })
                        }
                        _ => unreachable!("statement expression parses one compound statement"),
                    };
                    let ty = if let Some(expression) = value {
                        let inferred =
                            self.converted_expression_type(expression).or_else(|| {
                                self.report("E1003", "expected-expression", expression.span());
                                None
                            })?;
                        self.materialize_inferred_type(inferred.ty, Some(expression))?
                    } else {
                        self.intern(TypeKind::Void, Qualifiers::default())
                    };
                    let span = self.merge_span(tok.span, close.span);
                    return Some(self.alloc_node(CHirExpr::StatementExpr {
                        body,
                        value,
                        facts: ExprFacts::value(ty),
                        span,
                    }));
                }
                let expr = self.parse_expression()?;
                let rparen_span = self.peek().map(|t| t.span).unwrap_or(tok.span);
                self.expect(TokenKind::RParen)?;
                let span = self.merge_span(tok.span, rparen_span);
                return Some(self.alloc_node(respan_expr(expr, span)));
            }
            TokenKind::Generic => return self.parse_generic_selection(tok),
            TokenKind::BuiltinExpect => return self.parse_builtin_expect(tok),
            TokenKind::BuiltinAtomic => return self.parse_builtin_atomic(tok),
            TokenKind::BuiltinConstantP => return self.parse_builtin_constant_p(tok),
            TokenKind::BuiltinChooseExpr => return self.parse_builtin_choose_expr(tok),
            TokenKind::BuiltinTypesCompatibleP => {
                return self.parse_builtin_types_compatible_p(tok);
            }
            TokenKind::BuiltinClassifyType => return self.parse_builtin_classify_type(tok),
            TokenKind::BuiltinOffsetof => return self.parse_builtin_offsetof(tok),
            TokenKind::BuiltinObjectSize => return self.parse_builtin_object_size(tok),
            TokenKind::BuiltinHasAttribute => return self.parse_builtin_has_attribute(tok),
            TokenKind::BuiltinStrlen => return self.parse_builtin_strlen(tok),
            TokenKind::BuiltinLibrary | TokenKind::BuiltinCtype => {
                return self.parse_builtin_library_call(tok);
            }
            TokenKind::BuiltinVaStart => {
                return self.parse_builtin_varargs(tok, VarArgsBuiltinOp::Start);
            }
            TokenKind::BuiltinVaEnd => {
                return self.parse_builtin_varargs(tok, VarArgsBuiltinOp::End);
            }
            TokenKind::BuiltinVaCopy => {
                return self.parse_builtin_varargs(tok, VarArgsBuiltinOp::Copy);
            }
            TokenKind::BuiltinVaArg => return self.parse_builtin_va_arg(tok),
            TokenKind::BuiltinBit => return self.parse_builtin_bit(tok),
            TokenKind::BuiltinUnreachable | TokenKind::BuiltinTrap => {
                return self.parse_builtin_termination(tok);
            }
            TokenKind::BuiltinReturnAddress => return self.parse_builtin_return_address(tok),
            TokenKind::BuiltinFrameAddress => return self.parse_builtin_frame_address(tok),
            TokenKind::BuiltinAddOverflow => {
                return self.parse_checked_arithmetic(tok, CheckedArithmeticOp::Add);
            }
            TokenKind::BuiltinSubOverflow => {
                return self.parse_checked_arithmetic(tok, CheckedArithmeticOp::Sub);
            }
            TokenKind::BuiltinMulOverflow => {
                return self.parse_checked_arithmetic(tok, CheckedArithmeticOp::Mul);
            }
            _ => {
                self.report("E1003", "expected-expression", tok.span);
                return None;
            }
        };
        Some(self.alloc_node(expression))
    }
}
