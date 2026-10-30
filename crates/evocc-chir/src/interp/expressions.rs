//! C-HIR interpreter expressions.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn schedule_expression(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        allow_root_mutation: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
        values: &mut Vec<CValue>,
    ) -> Result<(), InterpreterError> {
        if !self.unit.runtime_floating_environment() || self.static_integer_evaluation {
            if let Some(value) = crate::constant::floating_constant_cast(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            ) {
                values.push(CValue::Floating(value));
                return Ok(());
            }
            if let Some(value) = crate::constant::fold_floating_integer_arithmetic(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            ) {
                values.push(CValue::Integer(value));
                return Ok(());
            }
        }
        if !self.static_integer_evaluation
            && !self.memory_mode
            // Composite scalar expressions must use the same explicit task
            // machine as call-bearing/object expressions. The old no-call
            // shortcut recursively evaluated arbitrarily deep arithmetic.
            && matches!(
                expression,
                CHirExpr::Lit(..)
                    | CHirExpr::CharLit(..)
                    | CHirExpr::Ident { .. }
                    | CHirExpr::SizeOf(..)
                    | CHirExpr::SizeOfExpr(..)
                    | CHirExpr::AlignOf(..)
                    | CHirExpr::BuiltinTrap { .. }
                    | CHirExpr::BuiltinUnreachable { .. }
            )
        {
            values.push(CValue::Integer(
                self.eval_expr(expression, allow_root_mutation)?,
            ));
            return Ok(());
        }

        let span = expression.span();
        self.tick(span)?;
        if !self.static_integer_evaluation {
            if let Some(value) = crate::constant::fold_wrapping_integer_arithmetic(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            ) {
                values.push(CValue::Integer(value));
                return Ok(());
            }
        }
        if self.static_integer_evaluation {
            if matches!(expression, CHirExpr::Unary(UnaryOp::AddrOf, ..)) {
                if let Some(value) = self.numeric_static_address(expression)? {
                    values.push(value);
                    return Ok(());
                }
            }
            // Reusing the execution machine must not admit calls, reads of
            // objects, or mutations into constant initialization. Inspect only
            // the evaluated path: sizeof and unselected arms stay unevaluated.
            let admitted = match expression {
                CHirExpr::Lit(..)
                | CHirExpr::CharLit(..)
                | CHirExpr::Unary(
                    UnaryOp::Plus | UnaryOp::Neg | UnaryOp::Abs | UnaryOp::Not | UnaryOp::BitNot,
                    ..,
                )
                | CHirExpr::EvaluateOnce(..)
                | CHirExpr::Binary(..)
                | CHirExpr::Ternary(..)
                | CHirExpr::Elvis(..)
                | CHirExpr::BuiltinConstantP { .. }
                | CHirExpr::BuiltinObjectSize { .. }
                | CHirExpr::BitBuiltin { .. }
                | CHirExpr::SizeOf(..)
                | CHirExpr::SizeOfExpr(..)
                | CHirExpr::AlignOf(..)
                | CHirExpr::Comma(..) => true,
                CHirExpr::Ident { symbol, .. } => self
                    .unit
                    .symbols()
                    .symbol(*symbol)
                    .is_ok_and(|symbol| symbol.enumerator_value().is_some()),
                // Numeric pointer round trips use the same iterative casts
                // and fuel as other constants. The operand whitelist still
                // excludes object reads, symbolic addresses and calls.
                CHirExpr::Cast(ty, ..) => matches!(
                    self.value_type(*ty, span),
                    Ok(ValueType::Integer(_) | ValueType::Pointer(_))
                ),
                _ => false,
            };
            if !admitted {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "nonconstant expression in static integer initializer",
                    span,
                });
            }
        }
        match expression {
            CHirExpr::Atomic(access) => {
                tasks.push(MachineTask::ApplyAtomic(access));
                for expression in access.operands().rev() {
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
            }
            CHirExpr::Floating(access) => {
                tasks.push(MachineTask::ApplyFloating(access));
                for &expression in access.operands.iter().rev() {
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
            }
            CHirExpr::Vector(access) => {
                tasks.push(MachineTask::ApplyVector(access));
                for &expression in access.operands.iter().rev() {
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
            }
            CHirExpr::Complex(access) => {
                tasks.push(MachineTask::ApplyComplex(access));
                for &expression in access.operands.iter().rev() {
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
            }
            CHirExpr::Lit(literal, _) => values.push(CValue::Integer(
                integer_literal_value(*literal).map_err(|error| self.integer_error(error, span))?,
            )),
            CHirExpr::CharLit(literal, _) => values.push(CValue::Integer(literal.integer_value())),
            CHirExpr::BuiltinConstantP { operand, .. } => values.push(CValue::Integer(
                CInteger::int(i32::from(crate::builtin_constant_p(
                    operand,
                    self.unit.types(),
                    self.unit.symbols(),
                    &self.target,
                ))),
            )),
            CHirExpr::BuiltinObjectSize { .. } => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "unresolved object extent query",
                    span,
                })
            }
            CHirExpr::Ident {
                symbol,
                spelling,
                facts,
                span,
            } => values.push(self.load_identifier(*symbol, spelling, *facts, *span)?),
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) if self.memory_mode => {
                if matches!(operand, CHirExpr::CompoundLit(_, _, _)) {
                    self.schedule_compound_literal(operand, true, tasks)?;
                } else if let CHirExpr::MemoryRead {
                    place, target_span, ..
                } = operand
                {
                    self.validate_memory_place(place).map_err(|_| {
                        InterpreterError::UnsupportedExpression {
                            feature: "invalid addressed memory place",
                            span,
                        }
                    })?;
                    // A function designator is the pointer itself, not an
                    // object address on which to perform a memory access.
                    if !matches!(
                        self.unit.types().get(place.value_ty()).map(|ty| &ty.kind),
                        Some(TypeKind::Function { .. })
                    ) {
                        tasks.push(MachineTask::FoldPlaceAddress {
                            place,
                            span: *target_span,
                        });
                    }
                    self.schedule_place_operands(place, *target_span, tasks, values)?;
                } else if let CHirExpr::Unary(UnaryOp::Deref, pointer, _) = operand {
                    tasks.push(MachineTask::EvalExpression {
                        expression: pointer,
                        allow_root_mutation: true,
                    });
                } else {
                    values.push(self.address_of_expression(operand, span)?);
                }
            }
            CHirExpr::Unary(UnaryOp::Deref, operand, _) if self.memory_mode => {
                let pointer_type = self.pointer_type_of_expression(operand)?;
                if !matches!(
                    self.unit
                        .types()
                        .get(pointer_type.pointee())
                        .map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) {
                    tasks.push(MachineTask::LoadDereference { pointer_type, span });
                }
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    // A postfix or prefix update is part of the pointer
                    // expression in `*p++` / `*++p`; the dereference must not
                    // turn that legitimate single mutation into a boundary.
                    allow_root_mutation: true,
                });
            }
            CHirExpr::Unary(operator, operand, _) => {
                tasks.push(MachineTask::ApplyUnary {
                    operator: *operator,
                    expression_span: span,
                    operand_span: operand.span(),
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    allow_root_mutation: false,
                });
            }
            CHirExpr::EvaluateOnce(operand, _) => {
                let previous = self.nested_mutation_depth;
                self.nested_mutation_depth = previous.saturating_add(1);
                tasks.push(MachineTask::RestoreNestedMutationDepth(previous));
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::BuiltinExpect(value, expected, _) => {
                // The saved first value stays below the second on the value
                // stack. A nonreturning second operand never yields a result.
                tasks.push(MachineTask::DiscardExpression(expected.span()));
                tasks.push(MachineTask::EvalExpression {
                    expression: expected,
                    allow_root_mutation: true,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::Comma(left, right, _) | CHirExpr::Binary(BinaryOp::Comma, left, right, _) => {
                if self.static_integer_evaluation {
                    tasks.push(MachineTask::ContinueBinary {
                        operator: BinaryOp::Comma,
                        right,
                        span,
                        allow_root_mutation: false,
                    });
                    tasks.push(MachineTask::EvalExpression {
                        expression: left,
                        allow_root_mutation: false,
                    });
                } else {
                    tasks.push(MachineTask::EvalExpression {
                        expression: right,
                        allow_root_mutation,
                    });
                    self.schedule_discarded_expression(left, tasks)?;
                }
            }
            CHirExpr::Binary(operator, left, right, _) => {
                tasks.push(MachineTask::ContinueBinary {
                    operator: *operator,
                    right,
                    span,
                    allow_root_mutation,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: left,
                    allow_root_mutation: matches!(operator, BinaryOp::And | BinaryOp::Or)
                        || (*operator == BinaryOp::Comma && allow_root_mutation),
                });
            }
            CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
                let result_type = self.conditional_value_type(expression)?;
                tasks.push(MachineTask::SelectTernary {
                    then_expression,
                    else_expression,
                    result_type,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: condition,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::Elvis(operand, else_expression, _) => {
                let result_type = self.conditional_value_type(expression)?;
                tasks.push(MachineTask::SelectElvis {
                    else_expression,
                    result_type,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    // GNU `a ?: b` evaluates `a` as a full expression.  In
                    // particular, an update such as `value++` is permitted
                    // here and its pre-update value is the true result.
                    allow_root_mutation: true,
                });
            }
            CHirExpr::Call(callee, arguments, _) => {
                if let Some(size) = self.unit.stack_allocation_size(expression) {
                    tasks.push(MachineTask::AllocateDynamic { expression });
                    tasks.push(MachineTask::EvalExpression {
                        expression: size,
                        allow_root_mutation: true,
                    });
                    return Ok(());
                }
                tasks.push(MachineTask::EvalCallArguments {
                    callee,
                    arguments,
                    index: 0,
                    values: Vec::new(),
                    span,
                    expects_value: true,
                });
            }
            CHirExpr::CheckedArithmetic {
                op,
                lhs,
                rhs,
                target,
                target_ty,
                ..
            } => {
                tasks.push(MachineTask::ApplyCheckedArithmetic {
                    operator: *op,
                    target_type: *target_ty,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: target,
                    allow_root_mutation: true,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: rhs,
                    allow_root_mutation: true,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: lhs,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::BitBuiltin { op, operand, .. } => {
                tasks.push(MachineTask::ApplyBitBuiltin {
                    operator: *op,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::BuiltinUnreachable { .. } => {
                return Err(InterpreterError::UnreachableReached { span });
            }
            CHirExpr::BuiltinTrap { .. } => {
                return Err(InterpreterError::BuiltinTrapReached { span });
            }
            CHirExpr::BuiltinReturnAddress { depth: 0, .. }
            | CHirExpr::BuiltinFrameAddress { depth: 0, .. } => {
                let frame =
                    self.memory_frame
                        .as_ref()
                        .ok_or(InterpreterError::UnsupportedExpression {
                            feature: "activation address outside a call frame",
                            span,
                        })?;
                // Function identities occupy the initial range. Frame IDs are
                // monotonic across calls, including recursion and returned frames.
                let identity = self.symbol_order().code_identities
                    + frame.id.raw() * 2
                    + u64::from(matches!(expression, CHirExpr::BuiltinFrameAddress { .. }));
                let pointer = PointerValue::function(identity)
                    .map_err(|error| self.memory_error(error, span))?;
                let (memory, address) = self
                    .memory
                    .expose_pointer(pointer)
                    .map_err(|error| self.memory_error(error, span))?;
                self.memory = memory;
                let ty = self.pointer_descriptor(expression).map_err(|_| {
                    InterpreterError::UnsupportedExpression {
                        feature: "activation address has no pointer type",
                        span,
                    }
                })?;
                values.push(CValue::Pointer {
                    value: Pointer::from_address(address),
                    ty,
                });
            }
            CHirExpr::BuiltinReturnAddress { depth, .. }
            | CHirExpr::BuiltinFrameAddress { depth, .. }
                if u16::try_from(*depth).is_ok() =>
            {
                let ty = self.pointer_descriptor(expression).map_err(|_| {
                    InterpreterError::UnsupportedExpression {
                        feature: "activation address has no pointer type",
                        span,
                    }
                })?;
                values.push(CValue::Pointer {
                    value: Pointer::NULL,
                    ty,
                });
            }
            CHirExpr::BuiltinReturnAddress { .. } | CHirExpr::BuiltinFrameAddress { .. } => {
                return Err(InterpreterError::FrameAddressUnavailable { span });
            }
            CHirExpr::BuiltinVarArgs { .. } => self.schedule_varargs(expression, tasks)?,
            CHirExpr::BuiltinVarArgPack { .. } => {
                return Err(InterpreterError::VarArgPack { span });
            }
            CHirExpr::LabelAddress { .. } => {
                values.push(self.label_address_value(expression)?);
            }
            CHirExpr::Cast(ty, value, _) => {
                let target = self.value_type(*ty, span)?;
                tasks.push(MachineTask::CastValue { target, span });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    // A C cast converts the result; it is not an evaluation
                    // barrier for a side effect in its operand.
                    allow_root_mutation,
                });
            }
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                facts,
                ..
            } if self.mutation_allowed(allow_root_mutation) => {
                if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                    return Err(InterpreterError::InvalidScalarPlace {
                        symbol: place.symbol(),
                        span,
                    });
                }
                let symbol = if self.memory_mode {
                    self.validate_memory_scalar_place(*place, target, span)?
                } else {
                    self.validate_place(*place, target, span)?
                };
                tasks.push(MachineTask::ApplyAssignment {
                    operator: *op,
                    symbol,
                    target_span: target.span(),
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: false,
                });
            }
            CHirExpr::MemoryRead {
                place,
                target_span,
                facts,
                ..
            } if self.memory_mode => {
                self.validate_readable_place(place, *facts, *target_span)?;
                if !matches!(
                    self.unit.types().get(place.value_ty()).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) {
                    tasks.push(MachineTask::LoadProjectedPlace {
                        place,
                        span: *target_span,
                    });
                }
                self.schedule_place_operands(place, *target_span, tasks, values)?;
            }
            CHirExpr::Update {
                op,
                place,
                target,
                facts,
                ..
            } if self.mutation_allowed(allow_root_mutation) => {
                if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                    return Err(InterpreterError::InvalidScalarPlace {
                        symbol: place.symbol(),
                        span,
                    });
                }
                let symbol = if self.memory_mode {
                    self.validate_memory_scalar_place(*place, target, span)?
                } else {
                    self.validate_place(*place, target, span)?
                };
                tasks.push(MachineTask::ApplyScalarUpdate {
                    operator: *op,
                    symbol,
                    target_span: target.span(),
                    span,
                });
            }
            CHirExpr::MemoryUpdate {
                op,
                place,
                target_span,
                facts,
                ..
            } if self.memory_mode && self.mutation_allowed(allow_root_mutation) => {
                self.validate_mutable_place(place, *facts, *target_span)?;
                tasks.push(MachineTask::ApplyIndirectUpdate {
                    operator: *op,
                    bit_field: self.place_bit_field(place, *target_span)?,
                    layout: self.place_access_layout(place, *target_span)?,
                    value_type: place.value_ty(),
                    qualifiers: place.access_qualifiers(),
                    target_span: *target_span,
                    span,
                });
                tasks.push(MachineTask::FoldPlaceAddress {
                    place,
                    span: *target_span,
                });
                self.schedule_place_operands(place, *target_span, tasks, values)?;
            }
            CHirExpr::MemoryAssign {
                op,
                place,
                target_span,
                value,
                facts,
                ..
            } if self.memory_mode && self.mutation_allowed(allow_root_mutation) => {
                self.validate_memory_place(place).map_err(|_| {
                    InterpreterError::UnsupportedExpression {
                        feature: "invalid typed memory place",
                        span: *target_span,
                    }
                })?;
                if facts.ty() != place.value_ty()
                    || facts.category() != ExprCategory::Value
                    || !place.is_modifiable()
                {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "unsupported typed memory place",
                        span: *target_span,
                    });
                }
                let pointer_type = PointerTypeDesc::from_addressed_place(place);
                tasks.push(MachineTask::ApplyIndirectAssignment {
                    operator: *op,
                    bit_field: self.place_bit_field(place, *target_span)?,
                    layout: self.place_access_layout(place, *target_span)?,
                    pointer_type,
                    value_type: place.value_ty(),
                    qualifiers: place.access_qualifiers(),
                    target_span: *target_span,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: false,
                });
                tasks.push(MachineTask::FoldPlaceAddress {
                    place,
                    span: *target_span,
                });
                self.schedule_place_operands(place, *target_span, tasks, values)?;
            }
            CHirExpr::Member(base, name, _) if self.memory_mode => {
                tasks.push(MachineTask::LoadAggregateMember {
                    name: name.as_str(),
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: base,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::SizeOf(ty, _) | CHirExpr::SizeOfExpr(_, ty, _) => {
                let size = self
                    .target
                    .sizeof_type(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "sizeof incomplete or unsupported type",
                        span,
                    })?;
                values.push(CValue::Integer(CInteger::new(size, CIntegerType::ULONG)));
            }
            CHirExpr::AlignOf(ty, _) => {
                let alignment = self
                    .target
                    .alignof_type(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "alignof incomplete or unsupported type",
                        span,
                    })?;
                values.push(CValue::Integer(CInteger::new(
                    u64::from(alignment),
                    CIntegerType::ULONG,
                )));
            }
            CHirExpr::StatementExpr {
                body,
                value,
                facts,
                span,
            } => {
                // The binary32 value of a half operation rounds once to the
                // statement expression's half type (EvoIR 3.13.6).
                let round = match self.value_type(facts.ty(), *span)? {
                    target @ ValueType::Floating(kind) if kind.is_half() => Some(target),
                    _ => None,
                };
                self.schedule_statement_expression(body, *value, round, *span, tasks)?;
            }
            CHirExpr::CompoundLit(_, _, _) if self.memory_mode => {
                self.schedule_compound_literal(expression, false, tasks)?;
            }
            CHirExpr::CompoundLit(_, _, _) => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "compound literal outside the object-memory machine",
                    span,
                });
            }
            // Reaching this means requirement analysis concluded the program
            // needs no object memory while its C-HIR designates a subobject.
            // The two must agree, so this is reported rather than papered over
            // by silently enabling memory here.
            CHirExpr::MemoryRead { target_span, .. } => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "memory read outside the object-memory machine",
                    span: *target_span,
                });
            }
            CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. } => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "function call nested in scalar mutation",
                    span,
                });
            }
            CHirExpr::Member(_, _, _) | CHirExpr::PtrMember(_, _, _) | CHirExpr::Index(_, _, _) => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "function call nested outside scalar expression subset",
                    span,
                });
            }
            CHirExpr::StringLit(_, _) => values.push(self.string_pointer(expression, true)?),
            CHirExpr::FloatLit(literal, _) => {
                // A half constant's value is binary32 (C23 H.3).
                let kind = self.target.float_format(literal.suffix.value_kind());
                let value = match literal.value {
                    crate::FloatConstant::Real(value) => value.target_convert(kind).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "deferred floating format",
                            span,
                        },
                    )?,
                    crate::FloatConstant::QuietNaN(payload) => {
                        evocc_types::floating::FloatBits::from_bits(
                            kind,
                            if kind == evocc_types::FloatKind::F32 {
                                0x7fc00000 | (payload & 0x3fffff)
                            } else if kind == evocc_types::FloatKind::F128 {
                                (0x7fff8u128 << 108) | (payload & ((1 << 111) - 1))
                            } else {
                                0x7ff8000000000000 | (payload & 0x7ffffffffffff)
                            },
                        )
                        .unwrap()
                    }
                };
                values.push(CValue::Floating(value));
            }
        }
        Ok(())
    }
}
