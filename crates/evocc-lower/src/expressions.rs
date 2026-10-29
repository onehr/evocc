//! Lowering expressions.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_expr(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let previous_span = std::mem::replace(&mut self.source_span, expression.span());
        let result = self.lower_expr_with_origin(expression, state);
        self.source_span = previous_span;
        result
    }

    pub(super) fn lower_expr_with_origin(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        // An arithmetic continuation does not need a native stack frame.
        // Preserve preorder constant admission and left-before-right effects;
        // the existing binary tail threads the saved left value through any
        // CFG opened by the right operand.
        let mut pending = Vec::new();
        let mut next = expression;
        let mut value = loop {
            self.source_span = next.span();
            if let Some(value) = self.fold_expression(next, state) {
                break value?;
            }
            if let CHirExpr::Binary(
                op @ (BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Mod
                | BinaryOp::Shl
                | BinaryOp::Shr
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne),
                left,
                right,
                span,
            ) = next
            {
                pending.push((*op, *left, *right, *span));
                next = left;
            } else {
                break self.lower_unfolded_expr(next, state)?;
            }
        };
        while let Some((operator, left, right, span)) = pending.pop() {
            self.source_span = span;
            value = self.lower_binary_after_left(operator, (left, right), value, span, state)?;
        }
        Ok(value)
    }

    pub(super) fn fold_expression(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Option<Result<ExprValue, LowerError>> {
        if self.optimize
            && !self.unit.runtime_floating_environment()
            && matches!(expression, CHirExpr::Binary(..) | CHirExpr::Unary(..))
        {
            if let Some(value) = evocc_chir::constant::eval_executable_integer_constant(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                self.target,
            ) {
                // Use executed-expression rules, not ICE rules: invalid
                // division and shifts must still reach their UB obligations.
                // A numerically unselected arm can nevertheless contain a
                // label entered by a goto or switch outside the expression.
                if !self.expression_has_external_entry(expression) {
                    return Some(self.wide_constant(state, value.ty(), value.raw_bits()));
                }
            }
        }
        if !self.unit.runtime_floating_environment() {
            if let Some(value) = evocc_chir::constant::floating_constant_cast(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                self.target,
            ) {
                return Some(self.floating_constant(value, state));
            }
            if let Some(value) = evocc_chir::constant::fold_floating_integer_arithmetic(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                self.target,
            ) {
                return Some(self.wide_constant(state, value.ty(), value.raw_bits()));
            }
        }
        if let Some(value) = evocc_chir::constant::fold_wrapping_integer_arithmetic(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            self.target,
        ) {
            return Some(self.wide_constant(state, value.ty(), value.raw_bits()));
        }
        None
    }

    pub(super) fn lower_unfolded_expr(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        match expression {
            CHirExpr::Floating(operation) => self.lower_floating_builtin(operation, state),
            CHirExpr::Vector(operation) => self.lower_vector(operation, state),
            CHirExpr::Complex(operation) => self.lower_complex(operation, state),
            CHirExpr::Lit(literal, _) => {
                let integer = integer_literal_value(*literal).map_err(|_| {
                    LowerError::UnsupportedExpression {
                        span: expression.span(),
                        feature: "integer literal has no Day-6 target representation",
                    }
                })?;
                self.wide_constant(state, integer.ty(), integer.raw_bits())
            }
            CHirExpr::BuiltinObjectSize { span, .. } => Err(LowerError::UnsupportedExpression {
                span: *span,
                feature: "unresolved object extent query",
            }),
            CHirExpr::BuiltinConstantP { operand, .. } => {
                let value = evocc_chir::builtin_constant_p(
                    operand,
                    self.unit.types(),
                    self.unit.symbols(),
                    self.target,
                );
                self.wide_constant(state, CIntegerType::INT, u128::from(value))
            }
            CHirExpr::CharLit(literal, _) => {
                let integer = literal.integer_value();
                self.wide_constant(state, integer.ty(), integer.raw_bits())
            }
            CHirExpr::Ident {
                symbol,
                spelling,
                facts,
                span,
                ..
            } => {
                let declaration =
                    self.unit
                        .symbols()
                        .symbol(*symbol)
                        .map_err(|_| LowerError::UnknownSymbol {
                            symbol: *symbol,
                            span: *span,
                        })?;
                if declaration.kind() == SymbolKind::Enumerator {
                    if declaration.spelling() != spelling
                        || declaration.ty() != facts.ty()
                        || facts.category() != ExprCategory::Value
                    {
                        return Err(LowerError::UnsupportedExpression {
                            span: *span,
                            feature: "enumerator identifier disagrees with its semantic identity",
                        });
                    }
                    let integer =
                        canonical_integer(self.unit.types(), self.target, facts.ty(), *span)?;
                    let value = declaration.enumerator_value().ok_or(
                        LowerError::UnsupportedExpression {
                            span: *span,
                            feature: "enumerator symbol is missing its canonical value",
                        },
                    )?;
                    // The symbol stores a signed host integer. Materialize its
                    // C-width bit pattern, not its sign-extended host bits.
                    return self.wide_constant(state, integer, value as u128);
                }
                if declaration.kind() == SymbolKind::Function
                    && declaration.spelling() == spelling
                    && evocc_chir::type_at_use_matches_binding(
                        self.unit.types(),
                        facts.ty(),
                        declaration.ty(),
                    )
                    && facts.category() == ExprCategory::FunctionDesignator
                {
                    return self.function_address(*symbol, *span, state);
                }
                if !matches!(
                    declaration.kind(),
                    SymbolKind::Object | SymbolKind::Parameter
                ) || declaration.spelling() != spelling
                    || !evocc_chir::type_at_use_matches_binding(
                        self.unit.types(),
                        facts.ty(),
                        declaration.ty(),
                    )
                    || facts.category() != ExprCategory::LValue
                {
                    return Err(LowerError::UnsupportedExpression {
                        span: *span,
                        feature: "object identifier disagrees with its semantic identity",
                    });
                }
                if self.memory_symbols.contains(symbol) || self.is_aggregate_type(facts.ty()) {
                    let address = self.object_address(*symbol, *span, state)?;
                    if self.is_aggregate_type(facts.ty())
                        || matches!(
                            self.unit.types().get(facts.ty()).map(|ty| &ty.kind),
                            Some(
                                TypeKind::Array { .. }
                                    | TypeKind::Struct(_)
                                    | TypeKind::Union(_)
                                    | TypeKind::BuiltinVaList
                            )
                        )
                    {
                        return Ok(ExprValue {
                            value: address,
                            integer: CIntegerType::ULONG,
                        });
                    }
                    let volatile = self
                        .unit
                        .types()
                        .get(facts.ty())
                        .is_some_and(|ty| ty.qualifiers.is_volatile);
                    return self.emit_memory_load(address, facts.ty(), volatile, *span, state);
                }
                self.local(state, *symbol, *span)
            }
            CHirExpr::Unary(operator, operand, span) => {
                self.lower_unary(*operator, operand, *span, state)
            }
            CHirExpr::EvaluateOnce(operand, _) => self.lower_expr(operand, state),
            CHirExpr::Atomic(access) => self.lower_atomic(access, state),
            CHirExpr::BuiltinExpect(value, expected, span) => {
                let value = self.lower_expr(value, state)?;
                let key = EnvKey::Temporary(self.next_temporary);
                self.next_temporary += 1;
                state.env.insert(
                    key,
                    BoundValue {
                        value: value.value,
                        integer: Some(value.integer),
                    },
                );
                // Keep the first result in the ordinary edge environment if
                // the hint contains control flow, just like a binary operand.
                if self.unit.runtime_floating_environment()
                    || evocc_chir::constant::eval_executable_integer_constant(
                        expected,
                        self.unit.types(),
                        self.unit.symbols(),
                        self.target,
                    )
                    .is_none()
                {
                    self.lower_expr(expected, state)?;
                }
                let saved = state
                    .env
                    .remove(&key)
                    .ok_or(LowerError::InvalidControlFlow {
                        span: *span,
                        reason: "expectation hint lost its evaluated value",
                    })?;
                Ok(ExprValue {
                    value: saved.value,
                    integer: value.integer,
                })
            }
            CHirExpr::Binary(operator, left, right, span) => {
                self.lower_binary(*operator, left, right, *span, state)
            }
            CHirExpr::Ternary(condition, then_value, else_value, _) => {
                self.lower_ternary(condition, then_value, else_value, state)
            }
            CHirExpr::Elvis(operand, fallback, _) => self.lower_elvis(operand, fallback, state),
            CHirExpr::Call(_, _, span) => {
                self.emit_call(expression, state)?
                    .ok_or(LowerError::UnsupportedExpression {
                        span: *span,
                        feature: "void call used as a scalar expression",
                    })
            }
            CHirExpr::CheckedArithmetic {
                op,
                lhs,
                rhs,
                target,
                target_ty,
                span,
                ..
            } => self.lower_checked(*op, lhs, rhs, target, *target_ty, *span, state),
            CHirExpr::BitBuiltin { op, operand, .. } => self.lower_bit(*op, operand, state),
            CHirExpr::BuiltinVarArgPack { span, .. } => Err(LowerError::VarArgPack { span: *span }),
            CHirExpr::BuiltinVarArgs {
                op,
                first,
                second,
                requested_ty,
                span,
                ..
            } => self
                .lower_varargs(*op, first, *second, *requested_ty, *span, state)?
                .ok_or(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "void variadic operation used as a value",
                }),
            CHirExpr::BuiltinReturnAddress { depth, .. }
            | CHirExpr::BuiltinFrameAddress { depth, .. } => {
                let result = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
                let instruction = if *depth != 0 {
                    // This is a runtime null value. The front end must
                    // retain the builtin's non-constant-expression status.
                    Inst::PtrNull(result.clone())
                } else if matches!(expression, CHirExpr::BuiltinFrameAddress { .. }) {
                    Inst::FrameAddress {
                        result: result.clone(),
                        depth: 0,
                    }
                } else {
                    Inst::ReturnAddress {
                        result: result.clone(),
                    }
                };
                self.emit(state, instruction)?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                })
            }
            CHirExpr::LabelAddress { label, span, .. } => {
                let block = *self.address_labels.get(&label.spelling).ok_or(
                    LowerError::InvalidControlFlow {
                        span: *span,
                        reason: "label address has no function-local identity",
                    },
                )?;
                let result = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
                self.emit(
                    state,
                    Inst::BlockAddr {
                        result: result.clone(),
                        block,
                    },
                )?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                })
            }
            CHirExpr::BuiltinUnreachable { span, .. } => Err(LowerError::UnsupportedExpression {
                span: *span,
                feature: "unreachable builtin must be a discarded expression",
            }),
            CHirExpr::BuiltinTrap { span, .. } => Err(LowerError::UnsupportedExpression {
                span: *span,
                feature: "void trap builtin used where a value is required",
            }),
            CHirExpr::StatementExpr {
                body,
                value,
                facts,
                span,
            } => self
                .lower_statement_expression(body, *value, *facts, *span, state)?
                .ok_or(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "void statement expression used where a value is required",
                }),
            CHirExpr::CompoundLit(ty, _, span) => {
                let address = self.lower_compound_literal_address(expression, state)?;
                if self.is_aggregate_type(*ty)
                    || matches!(
                        self.unit.types().get(*ty).map(|canonical| &canonical.kind),
                        Some(
                            TypeKind::Array { .. }
                                | TypeKind::Struct(_)
                                | TypeKind::Union(_)
                                | TypeKind::BuiltinVaList
                        )
                    )
                {
                    Ok(ExprValue {
                        value: address,
                        integer: CIntegerType::ULONG,
                    })
                } else {
                    let volatile = self
                        .unit
                        .types()
                        .get(*ty)
                        .is_some_and(|canonical| canonical.qualifiers.is_volatile);
                    self.emit_memory_load(address, *ty, volatile, *span, state)
                }
            }
            CHirExpr::Cast(target, operand, span) => {
                // Default argument conversions preserve complete composite
                // values, including integer complex and wide vectors. Their
                // numeric conversions have explicit Complex/Vector nodes.
                if self.is_aggregate_type(*target) {
                    let source = self
                        .record_value_type(operand)
                        .and_then(|ty| self.unit.types().get(ty));
                    if !source
                        .zip(self.unit.types().get(*target))
                        .is_some_and(|(source, target)| source.kind == target.kind)
                    {
                        return Err(LowerError::UnsupportedExpression {
                            span: *span,
                            feature: "record cast changes the aggregate identity",
                        });
                    }
                    return self.lower_expr(operand, state);
                }
                if matches!(
                    self.unit.types().get(*target).map(|ty| &ty.kind),
                    Some(TypeKind::Pointer(_) | TypeKind::Vector { .. } | TypeKind::Float(_))
                ) || floating::half_format(self.unit.types(), *target).is_some()
                {
                    let value = self.lower_expr(operand, state)?;
                    return self.coerce_to_object_type(value, Some(operand), *target, *span, state);
                }
                let target = canonical_integer(self.unit.types(), self.target, *target, *span)?;
                if let CHirExpr::Lit(literal, _) = operand {
                    let converted = integer_literal_value(*literal)
                        .map_err(|_| LowerError::UnsupportedExpression {
                            span: operand.span(),
                            feature: "integer literal has no target representation",
                        })?
                        .cast(target);
                    return self.wide_constant(state, target, converted.raw_bits());
                }
                let value = self.lower_expr(operand, state)?;
                self.cast(value, target, state)
            }
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                span,
                ..
            } => self.lower_assignment(*op, *place, target, value, *span, state),
            CHirExpr::Update {
                op,
                place,
                target,
                span,
                ..
            } => self.lower_update(*op, *place, target, *span, state),
            CHirExpr::MemoryAssign {
                op,
                place,
                target_span,
                value,
                span,
                ..
            } => self.lower_memory_assignment(*op, place, *target_span, value, *span, state),
            CHirExpr::MemoryUpdate {
                op,
                place,
                target_span,
                span,
                ..
            } => self.lower_memory_update(*op, place, *target_span, *span, state),
            CHirExpr::Comma(left, right, _) => {
                self.lower_discarded_expr(left, state)?;
                self.lower_expr(right, state)
            }
            CHirExpr::SizeOf(ty, span) | CHirExpr::SizeOfExpr(_, ty, span) => {
                let size = self
                    .target
                    .sizeof_type(self.unit.types(), *ty)
                    .map_err(|source| LowerError::Layout {
                        ty: *ty,
                        span: *span,
                        source,
                    })?;
                let result = self.fresh_value(bits(CIntegerType::ULONG));
                self.emit(state, Inst::Const(result.clone(), size))?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                })
            }
            CHirExpr::AlignOf(ty, span) => {
                let alignment =
                    self.target
                        .alignof_type(self.unit.types(), *ty)
                        .map_err(|source| LowerError::Layout {
                            ty: *ty,
                            span: *span,
                            source,
                        })?;
                let result = self.fresh_value(bits(CIntegerType::ULONG));
                self.emit(state, Inst::Const(result.clone(), u64::from(alignment)))?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                })
            }
            CHirExpr::StringLit(_, span) => {
                let origin = self
                    .string_origins
                    .get(&(expression as *const CHirExpr<'_> as usize))
                    .ok_or(LowerError::UnsupportedExpression {
                        span: *span,
                        feature: "string has no allocation site",
                    })?;
                let symbol = format!("__evocc.string.{origin}");
                let result = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
                self.emit(
                    state,
                    Inst::GlobalAddr {
                        result: result.clone(),
                        symbol,
                    },
                )?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                })
            }
            CHirExpr::Member(base, name, span) => {
                let record =
                    self.record_value_type(base)
                        .ok_or(LowerError::UnsupportedExpression {
                            span: *span,
                            feature: "member base has no record value type",
                        })?;
                let path = evocc_chir::record_member_path(self.unit.types(), record, name).ok_or(
                    LowerError::UnsupportedExpression {
                        span: *span,
                        feature: "record value has no unambiguous member",
                    },
                )?;
                let mut address = self.lower_expr(base, state)?.value;
                let mut field_type = record;
                let mut bit_field = None;
                for projection in path {
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), projection.tag())
                        .map_err(|source| LowerError::Layout {
                            ty: record,
                            span: *span,
                            source,
                        })?;
                    let field = layout
                        .fields
                        .iter()
                        .find(|f| f.field_index == projection.field_index())
                        .ok_or(LowerError::UnsupportedExpression {
                            span: *span,
                            feature: "record value member has no layout",
                        })?;
                    address = self.constant_offset_address(address, field.byte_offset, state)?;
                    field_type = projection.field_ty();
                    bit_field = field.bit_field;
                }
                if let Some(field) = bit_field {
                    self.emit_bit_field_load(address, field_type, field, false, *span, state)
                } else if self.is_aggregate_type(field_type)
                    || matches!(
                        self.unit.types().get(field_type).map(|t| &t.kind),
                        Some(TypeKind::Array { .. })
                    )
                {
                    Ok(ExprValue {
                        value: address,
                        integer: CIntegerType::ULONG,
                    })
                } else {
                    self.emit_memory_load(address, field_type, false, *span, state)
                }
            }
            CHirExpr::FloatLit(literal, span) => self.floating_literal(literal, *span, state),
            CHirExpr::PtrMember(_, _, span) | CHirExpr::Index(_, _, span) => {
                Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "expression requires the Day-7 object or non-integer model",
                })
            }
            CHirExpr::MemoryRead {
                place, target_span, ..
            } => {
                let address = self.lower_place_address(place, *target_span, state)?;
                if let Some(field) = self.place_bit_field(place, *target_span)? {
                    return self.emit_bit_field_load(
                        address,
                        place.value_ty(),
                        field,
                        place.access_qualifiers().is_volatile,
                        *target_span,
                        state,
                    );
                }
                if self.is_aggregate_type(place.value_ty())
                    || matches!(
                        self.unit.types().get(place.value_ty()).map(|ty| &ty.kind),
                        Some(TypeKind::Array { .. } | TypeKind::Function { .. })
                    )
                {
                    return Ok(ExprValue {
                        value: address,
                        integer: CIntegerType::ULONG,
                    });
                }
                self.emit_memory_load_with_layout(
                    address,
                    place.value_ty(),
                    place.access_qualifiers().is_volatile,
                    *target_span,
                    state,
                    self.place_access_layout(place, *target_span)?,
                )
            }
        }
    }

    pub(super) fn lower_statement_expression(
        &mut self,
        body: &'a CHirStmt<'a>,
        value: Option<&'a CHirExpr<'a>>,
        facts: ExprFacts,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        let previous = self.fenv_access;
        if let CHirStmt::Compound(_, _, Some(enabled)) = body {
            self.fenv_access = *enabled;
        }
        let result = self.lower_statement_expression_active(body, value, facts, span, state);
        self.fenv_access = previous;
        result
    }

    pub(super) fn lower_statement_expression_active(
        &mut self,
        body: &'a CHirStmt<'a>,
        value: Option<&'a CHirExpr<'a>>,
        facts: ExprFacts,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        self.record_constant_prefix(body);
        let CHirStmt::Compound(items, _, _) = body else {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "statement expression body is not a compound statement",
            });
        };
        let mut temporary_storage = Vec::new();
        if self.has_computed_goto
            && contains_targeted_label(
                body,
                &self.address_labels.keys().cloned().collect(),
                &BTreeMap::new(),
            )
        {
            let retained = state
                .env
                .iter()
                .filter_map(|(key, bound)| {
                    if let EnvKey::Temporary(id) = key {
                        (!state.env.contains_key(&EnvKey::TemporaryAddress(*id)))
                            .then_some((*id, bound.clone()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for (id, bound) in retained {
                let layout = evocc_evoir::evoir_access_layout(&bound.value.ty)
                    .map_err(|message| LowerError::InvalidGeneratedSsa { message })?
                    .ok_or(LowerError::InvalidControlFlow {
                        span,
                        reason: "indirect expression temporary has no storage layout",
                    })?;
                let origin = self.synthetic_origin()?;
                let address = self.emit_alloca_with_layout(layout, origin, state)?;
                self.emit_memory_store_with_layout(
                    address.clone(),
                    ExprValue {
                        value: bound.value.clone(),
                        integer: bound.integer.unwrap_or(CIntegerType::ULONG),
                    },
                    layout,
                    false,
                    state,
                )?;
                let key = EnvKey::TemporaryAddress(id);
                state.env.insert(
                    key,
                    BoundValue {
                        value: address,
                        integer: None,
                    },
                );
                temporary_storage.push((key, bound, layout));
            }
        }
        let prefix = if let Some(value) = value {
            let Some(CHirBlockItem::Statement(final_statement)) = items.last() else {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "statement expression value is not its final expression statement",
                });
            };
            let Some(final_value) = statement_expression_value(final_statement) else {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "statement expression value is not its final expression statement",
                });
            };
            if !std::ptr::eq(final_value, value) {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "statement expression value identity is not authoritative",
                });
            }
            &items[..items.len() - 1]
        } else {
            items.as_ref()
        };

        let mut live = state.locals.clone();
        if let Some(value) = value {
            collect_symbols_expr(value, &mut live);
        }
        let control = self.active_control.clone();
        let storage = self.begin_compound_storage(body as *const CHirStmt<'a> as usize, state)?;
        let flow = self.lower_compound_items(prefix, state.clone(), &live, &control, false)?;
        let mut continued = self.expression_continuation(flow, state, &live)?;

        // The final statement supplies the value, but its label prefixes
        // still establish distinct code identities before that value runs.
        if value.is_some() {
            if let Some(CHirBlockItem::Statement(statement)) = items.last() {
                let mut statement = *statement;
                while let CHirStmt::Label(label, nested, _) = statement {
                    continued = self
                        .enter_label(&label.spelling, nested, vec![continued], None, &live)?
                        .ok_or(LowerError::InvalidControlFlow {
                            span,
                            reason: "terminal label has no value path",
                        })?;
                    statement = nested;
                }
            }
        }
        let mut result = match value {
            Some(value)
                if evocc_chir::expression_is_void(
                    value,
                    self.unit.symbols(),
                    self.unit.types(),
                ) =>
            {
                self.lower_discarded_expr(value, &mut continued)?;
                None
            }
            Some(value) => Some(self.lower_expr(value, &mut continued)?),
            None => None,
        };
        if let Some(result) = &mut result {
            let expected = if self.is_aggregate_type(facts.ty()) {
                EvoIRType::Ptr(AddressSpace::Generic)
            } else {
                self.c_value_type(facts.ty(), span)?.0
            };
            if matches!(expected, EvoIRType::Bits(_))
                && matches!(result.value.ty, EvoIRType::Bits(_))
            {
                // Truth values are internally bits<1>, but the block
                // expression still exposes its declared C integer type.
                *result = self.cast(
                    result.clone(),
                    canonical_integer(self.unit.types(), self.target, facts.ty(), span)?,
                    &mut continued,
                )?;
            }
            // The binary32 value of a half operation rounds once to the
            // statement expression's half type, as in GCC 14 (EvoIR 3.13.6).
            if let Some(kind) = floating::half_format(self.unit.types(), facts.ty()) {
                *result = self.cast_floating(result.clone(), kind, &mut continued)?;
            }
            if result.value.ty != expected {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "statement expression result disagrees with its semantic type",
                });
            }
        }

        if let Some(result) = &mut result {
            if self.is_aggregate_type(facts.ty()) {
                // The copy reads the source object before its lifetime ends.
                // A volatile source object keeps its volatile accesses.
                let copy_is_volatile =
                    value.is_some_and(|value| self.aggregate_source_is_volatile(value));
                let origin = self.synthetic_origin()?;
                let temporary = self.emit_alloca(facts.ty(), span, origin, &mut continued)?;
                self.emit_aggregate_transfer(
                    temporary.clone(),
                    result.value.clone(),
                    facts.ty(),
                    CopyVolatility::new(copy_is_volatile, false),
                    span,
                    &mut continued,
                )?;
                result.value = temporary;
            }
        }
        let result_key = result
            .as_ref()
            .map(|result| self.retain_temporary(result.value.clone(), &mut continued));
        for item in items.iter().rev() {
            let CHirBlockItem::Declaration(CHirDecl::Object(object)) = item else {
                continue;
            };
            if self.memory_symbols.contains(&object.symbol)
                && continued.env.contains_key(&EnvKey::Address(object.symbol))
                && object.storage.base() != StorageClass::Static
            {
                self.end_object_lifetime(object.symbol, &mut continued)?;
            }
            continued.env.remove(&EnvKey::Local(object.symbol));
            continued.env.remove(&EnvKey::Address(object.symbol));
            continued.locals.remove(&object.symbol);
        }
        self.end_compound_storage(&storage, &mut continued)?;
        if let (Some(result), Some(key)) = (&mut result, result_key) {
            result.value = self.token(&continued, key)?.value;
            continued.env.remove(&key);
        }
        // Resuming the enclosing expression needs its saved operands. Binding
        // an indirect edge does not: a jump out of this expression must not
        // read an operand which that path never uses.
        for (key, bound, layout) in &temporary_storage {
            let EnvKey::TemporaryAddress(id) = key else {
                unreachable!("only expression storage is retained here");
            };
            let address = self.token(&continued, *key)?.value;
            let loaded = self.emit_typed_memory_load(
                address,
                bound.value.ty.clone(),
                bound.integer.unwrap_or(CIntegerType::ULONG),
                false,
                &mut continued,
                *layout,
            )?;
            continued.env.insert(
                EnvKey::Temporary(*id),
                BoundValue {
                    value: loaded.value,
                    integer: bound.integer,
                },
            );
        }
        let temporary_keys = temporary_storage
            .iter()
            .map(|(key, _, _)| *key)
            .collect::<Vec<_>>();
        self.end_compound_storage(&temporary_keys, &mut continued)?;
        *state = continued;
        Ok(result)
    }

    pub(super) fn lower_unary(
        &mut self,
        operator: UnaryOp,
        operand: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if self.optimize && operator == UnaryOp::Not {
            if let CHirExpr::Unary(UnaryOp::Not, inner, _) = operand {
                // !!x is its truth conversion, not x itself. Evaluate x once
                // through the ordinary condition path so volatile reads,
                // calls, poison checks and full-width/FP truth are retained.
                // Kernel likely/unlikely wrappers frequently nest these pairs.
                return Ok(ExprValue {
                    value: self.lower_condition(inner, state)?,
                    integer: CIntegerType::INT,
                });
            }
        }
        if operator == UnaryOp::AddrOf {
            let address = match operand {
                CHirExpr::Ident { symbol, .. }
                    if self
                        .unit
                        .symbols()
                        .symbol(*symbol)
                        .is_ok_and(|symbol| symbol.kind() == SymbolKind::Function) =>
                {
                    return self.lower_expr(operand, state);
                }
                CHirExpr::Ident { symbol, .. } => self.object_address(*symbol, span, state)?,
                CHirExpr::MemoryRead { place, .. } => {
                    self.lower_place_address(place, span, state)?
                }
                CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
                    let pointer = self.lower_expr(pointer, state)?;
                    if !matches!(
                        pointer.value.ty,
                        EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
                    ) {
                        return Err(LowerError::UnsupportedExpression {
                            span,
                            feature: "address-of dereference operand is not a pointer",
                        });
                    }
                    pointer.value
                }
                CHirExpr::CompoundLit(_, _, _) => {
                    self.lower_compound_literal_address(operand, state)?
                }
                CHirExpr::StringLit(..) => self.lower_expr(operand, state)?.value,
                _ => {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature: "address-of operand has no authoritative memory place",
                    });
                }
            };
            return Ok(ExprValue {
                value: address,
                integer: CIntegerType::ULONG,
            });
        }
        if operator == UnaryOp::Deref {
            let pointer = self.lower_expr(operand, state)?;
            if !matches!(
                pointer.value.ty,
                EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
            ) {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "dereference operand did not lower to a pointer",
                });
            }
            let pointee = self.pointer_pointee_type(operand, span)?;
            if matches!(
                self.unit.types().get(pointee).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. } | TypeKind::Array { .. })
            ) {
                // Arrays decay and function designators keep their address;
                // neither conversion loads an object value.
                return Ok(pointer);
            }
            if self.is_aggregate_type(pointee) {
                return Ok(ExprValue {
                    value: pointer.value,
                    integer: CIntegerType::ULONG,
                });
            }
            let volatile = self
                .unit
                .types()
                .get(pointee)
                .is_some_and(|ty| ty.qualifiers.is_volatile);
            return self.emit_memory_load(pointer.value, pointee, volatile, span, state);
        }
        let operand = self.lower_expr(operand, state)?;
        if evocc_evoir::floating_kind(&operand.value.ty).is_some()
            && matches!(operator, UnaryOp::Plus | UnaryOp::Neg)
        {
            return self.floating_unary(operator, operand, state);
        }
        match operator {
            UnaryOp::Plus => {
                let promoted = integer_promotion(operand.integer);
                self.cast(operand, promoted, state)
            }
            UnaryOp::Not => {
                // !x is x == 0 at x's full width. Materializing x != 0
                // first adds a second comparison and hides the branch from
                // the existing compare/cond_br machine lowering. Evaluate x
                // once above; keep floating unordered truth on its own path.
                let value = if evocc_evoir::floating_kind(&operand.value.ty).is_some() {
                    self.floating_truth(operand, state)?
                } else {
                    operand.value
                };
                let result = self.compare_with_zero(value, CmpOp::Eq, state)?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::INT,
                })
            }
            UnaryOp::Neg => {
                let integer = integer_promotion(operand.integer);
                let operand = self.cast(operand, integer, state)?;
                let zero = self.constant(state, integer, 0)?;
                self.emit_signed_arithmetic(state, BinaryOp::Sub, zero, operand, integer)
            }
            UnaryOp::Abs => {
                let integer = integer_promotion(operand.integer);
                let operand = self.cast(operand, integer, state)?;
                let amount = self
                    .constant(state, CIntegerType::UINT, u64::from(integer.bits() - 1))?
                    .value;
                let sign = self.fresh_value(bits(integer));
                self.emit(
                    state,
                    Inst::AShr(sign.clone(), operand.value.clone(), amount),
                )?;
                let magnitude = self.fresh_value(bits(integer));
                self.emit(
                    state,
                    Inst::Xor(magnitude.clone(), operand.value, sign.clone()),
                )?;
                // The builtin's signed-minimum domain error is independent of
                // the source arithmetic wrapping option.
                let defined = self.fresh_value(EvoIRType::Bits(1));
                self.emit(
                    state,
                    Inst::SignedSubDefined(defined.clone(), magnitude.clone(), sign.clone()),
                )?;
                self.emit(state, Inst::UbAssume(defined, UbReason::SignedOverflow))?;
                let result = self.fresh_value(bits(integer));
                self.emit(state, Inst::Sub(result.clone(), magnitude, sign))?;
                Ok(ExprValue {
                    value: result,
                    integer,
                })
            }
            UnaryOp::BitNot => {
                let integer = integer_promotion(operand.integer);
                let operand = self.cast(operand, integer, state)?;
                let mask = self.wide_constant(state, integer, integer.mask())?;
                let result = self.fresh_value(bits(integer));
                self.emit(state, Inst::Xor(result.clone(), operand.value, mask.value))?;
                Ok(ExprValue {
                    value: result,
                    integer,
                })
            }
            UnaryOp::AddrOf | UnaryOp::Deref => unreachable!("handled before operand conversion"),
        }
    }
}
