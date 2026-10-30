//! C-HIR interpreter scalar evaluation.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn runtime_integer_unary(
        &self,
        operator: UnaryOp,
        operand: CInteger,
    ) -> Result<CInteger, IntegerError> {
        if self.unit.signed_wrapping() && operator != UnaryOp::Abs {
            eval_constant_unary_integer(operator, operand)
        } else {
            eval_unary_integer(operator, operand)
        }
    }

    pub(super) fn runtime_integer_binary(
        &self,
        operator: BinaryOp,
        left: CInteger,
        right: CInteger,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        if self.static_integer_evaluation {
            return eval_constant_binary_integer(operator, left, right)
                .map_err(|error| self.integer_error(error, span));
        }
        let result = if self.unit.signed_wrapping()
            && matches!(operator, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul)
        {
            eval_constant_binary_integer(operator, left, right)
        } else {
            eval_binary_integer(operator, left, right)
        };
        result.map_err(|error| self.integer_error(error, span))
    }

    pub(super) fn apply_machine_binary(
        &mut self,
        operator: BinaryOp,
        left: CValue,
        right: CValue,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        if matches!(operator, BinaryOp::And | BinaryOp::Or) {
            return Ok(CValue::Integer(CInteger::int(i32::from(
                self.complex_truth(right, span)?,
            ))));
        }
        if operator == BinaryOp::Comma {
            return Ok(right);
        }
        if matches!(
            operator,
            BinaryOp::Assign
                | BinaryOp::AddAssign
                | BinaryOp::SubAssign
                | BinaryOp::MulAssign
                | BinaryOp::DivAssign
                | BinaryOp::ModAssign
                | BinaryOp::ShlAssign
                | BinaryOp::ShrAssign
                | BinaryOp::BitAndAssign
                | BinaryOp::BitOrAssign
                | BinaryOp::BitXorAssign
        ) {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "unlowered assignment binary operator",
                span,
            });
        }
        match (left, right) {
            (CValue::Floating(_), _) | (_, CValue::Floating(_)) => {
                let strict = self.strict_floating();
                floating::binary(operator, left, right, span, &mut self.world, strict)
            }
            (CValue::Uninitialized(_), _) | (_, CValue::Uninitialized(_)) => {
                Err(InterpreterError::UninitializedValue { span })
            }
            (CValue::Integer(left), CValue::Integer(right)) => self
                .runtime_integer_binary(operator, left, right, span)
                .map(CValue::Integer),
            (CValue::FunctionPointer { value, ty }, CValue::Integer(index))
                if matches!(operator, BinaryOp::Add | BinaryOp::Sub) =>
            {
                self.offset_function_value(
                    value,
                    ty,
                    index,
                    if operator == BinaryOp::Add {
                        OffsetDirection::Add
                    } else {
                        OffsetDirection::Subtract
                    },
                    span,
                )
            }
            (CValue::Integer(index), CValue::FunctionPointer { value, ty })
                if operator == BinaryOp::Add =>
            {
                self.offset_function_value(value, ty, index, OffsetDirection::Add, span)
            }
            (
                CValue::FunctionPointer { value: left, .. },
                CValue::FunctionPointer { value: right, .. },
            ) if operator == BinaryOp::Sub => {
                let left = self
                    .memory
                    .pointer_address(left)
                    .map_err(|error| self.memory_error(error, span))?;
                let right = self
                    .memory
                    .pointer_address(right)
                    .map_err(|error| self.memory_error(error, span))?;
                let difference = i128::from(left) - i128::from(right);
                let value = i64::try_from(difference)
                    .map_err(|_| self.integer_error(IntegerError::SignedOverflow, span))?;
                Ok(CValue::Integer(CInteger::new(
                    value as u64,
                    CIntegerType::LONG,
                )))
            }
            (CValue::Pointer { value, ty }, CValue::Integer(index))
                if matches!(operator, BinaryOp::Add | BinaryOp::Sub) =>
            {
                self.offset_pointer_value(
                    value,
                    ty,
                    index,
                    if operator == BinaryOp::Add {
                        OffsetDirection::Add
                    } else {
                        OffsetDirection::Subtract
                    },
                    span,
                )
            }
            (CValue::Integer(index), CValue::Pointer { value, ty })
                if operator == BinaryOp::Add =>
            {
                self.offset_pointer_value(value, ty, index, OffsetDirection::Add, span)
            }
            (
                CValue::Pointer {
                    value: left,
                    ty: left_ty,
                },
                CValue::Pointer {
                    value: right,
                    ty: right_ty,
                },
            ) if operator == BinaryOp::Sub
                && self
                    .unit
                    .types()
                    .compatible_ignoring_top_qualifiers(left_ty.pointee(), right_ty.pointee()) =>
            {
                // Distinct declarations can capture the same runtime array
                // bound under different symbol IDs. C requires compatible
                // pointees, not identical interned types; the left capture
                // still supplies the stride and all memory checks remain.
                self.subtract_pointer_values(left, right, left_ty, span)
            }
            (
                CValue::Pointer {
                    value: left,
                    ty: left_ty,
                },
                CValue::Pointer {
                    value: right,
                    ty: right_ty,
                },
            ) if matches!(
                operator,
                BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
            ) && PointerTypeDesc::composite(self.unit.types(), left_ty, right_ty).is_ok() =>
            {
                if self.static_integer_evaluation {
                    // The static-expression whitelist has evaluated numeric
                    // casts through the ordinary task machine and fuel budget.
                    // Comparing their representations does not access objects.
                    let bits = |pointer| {
                        self.memory
                            .pointer_address(PointerValue::object(pointer))
                            .map(|bits| CInteger::new(bits, CIntegerType::ULONG))
                            .map_err(|error| self.memory_error(error, span))
                    };
                    return self
                        .runtime_integer_binary(operator, bits(left)?, bits(right)?, span)
                        .map(CValue::Integer);
                }
                // Ordering compares byte addresses. Unlike subtraction it
                // does not require an exact number of pointee-sized elements.
                self.pointer_byte_difference(left, right, span)?;
                let left = left.byte_offset();
                let right = right.byte_offset();
                let result = match operator {
                    BinaryOp::Lt => left < right,
                    BinaryOp::Le => left <= right,
                    BinaryOp::Gt => left > right,
                    BinaryOp::Ge => left >= right,
                    _ => unreachable!(),
                };
                Ok(CValue::Integer(CInteger::new(
                    u64::from(result),
                    CIntegerType::INT,
                )))
            }
            (
                CValue::Pointer {
                    value: left,
                    ty: left_ty,
                },
                CValue::Pointer {
                    value: right,
                    ty: right_ty,
                },
            ) if matches!(operator, BinaryOp::Eq | BinaryOp::Ne)
                && PointerTypeDesc::equality_compatible(self.unit.types(), left_ty, right_ty) =>
            {
                let left = self
                    .memory
                    .pointer_address(PointerValue::object(left))
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                let right = self
                    .memory
                    .pointer_address(PointerValue::object(right))
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                Ok(machine_equality_result(operator, left == right))
            }
            (CValue::Pointer { value, .. }, CValue::Integer(zero))
            | (CValue::Integer(zero), CValue::Pointer { value, .. })
                if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) && zero.raw_bits() == 0 =>
            {
                Ok(machine_equality_result(operator, value == Pointer::NULL))
            }
            (
                CValue::FunctionPointer {
                    value: left,
                    ty: left_ty,
                },
                CValue::FunctionPointer {
                    value: right,
                    ty: right_ty,
                },
            ) if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) && left_ty == right_ty => {
                let left = self
                    .memory
                    .pointer_address(left)
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                let right = self
                    .memory
                    .pointer_address(right)
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                Ok(machine_equality_result(operator, left == right))
            }
            (CValue::FunctionPointer { value, .. }, CValue::Pointer { value: null, .. })
            | (CValue::Pointer { value: null, .. }, CValue::FunctionPointer { value, .. })
                if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) && null == Pointer::NULL =>
            {
                Ok(machine_equality_result(
                    operator,
                    value == PointerValue::Null,
                ))
            }
            (CValue::FunctionPointer { value, .. }, CValue::Integer(zero))
            | (CValue::Integer(zero), CValue::FunctionPointer { value, .. })
                if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) && zero.raw_bits() == 0 =>
            {
                Ok(machine_equality_result(
                    operator,
                    value == PointerValue::Null,
                ))
            }
            (
                CValue::FunctionPointer {
                    value: function,
                    ty: function_ty,
                },
                CValue::Pointer {
                    value: object,
                    ty: object_ty,
                },
            )
            | (
                CValue::Pointer {
                    value: object,
                    ty: object_ty,
                },
                CValue::FunctionPointer {
                    value: function,
                    ty: function_ty,
                },
            ) if matches!(operator, BinaryOp::Eq | BinaryOp::Ne)
                && PointerTypeDesc::equality_compatible(
                    self.unit.types(),
                    function_ty,
                    object_ty,
                ) =>
            {
                let function = self
                    .memory
                    .pointer_address(function)
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                let object = self
                    .memory
                    .pointer_address(PointerValue::object(object))
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                Ok(machine_equality_result(operator, function == object))
            }
            _ => Err(InterpreterError::UnsupportedExpression {
                feature: "unsupported pointer binary operation",
                span,
            }),
        }
    }

    pub(super) fn eval_expr(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        allow_root_mutation: bool,
    ) -> Result<CInteger, InterpreterError> {
        if let Some(value) = crate::constant::fold_floating_integer_arithmetic(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) {
            return Ok(value);
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
                return Ok(value);
            }
        }
        match expression {
            CHirExpr::Atomic(_)
            | CHirExpr::Vector(_)
            | CHirExpr::Complex(_)
            | CHirExpr::Floating(_) => Err(InterpreterError::UnsupportedExpression {
                feature: "atomic access outside memory execution",
                span,
            }),
            CHirExpr::Lit(literal, _) => {
                integer_literal_value(*literal).map_err(|error| self.integer_error(error, span))
            }
            CHirExpr::BuiltinConstantP { operand, .. } => {
                Ok(CInteger::int(i32::from(crate::builtin_constant_p(
                    operand,
                    self.unit.types(),
                    self.unit.symbols(),
                    &self.target,
                ))))
            }
            CHirExpr::BuiltinObjectSize { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "unresolved object extent query",
                span,
            }),
            CHirExpr::Ident {
                symbol,
                spelling,
                facts,
                span,
            } => {
                if is_global_register(self.unit, *symbol) {
                    return Err(InterpreterError::Asm { span: *span });
                }
                let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
                    InterpreterError::UnsupportedExpression {
                        feature: "unknown identifier",
                        span: *span,
                    }
                })?;
                if declaration.spelling() != spelling || declaration.ty() != facts.ty() {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "inconsistent identifier facts",
                        span: *span,
                    });
                }
                if let Some(value) = declaration.enumerator_value() {
                    let ty = self.integer_type(declaration.ty(), *span)?;
                    return Ok(CInteger::new(value as u64, ty));
                }
                let slot =
                    self.slots
                        .get(symbol)
                        .ok_or(InterpreterError::UnsupportedExpression {
                            feature: "identifier without automatic scalar slot",
                            span: *span,
                        })?;
                if !slot.active {
                    return Err(InterpreterError::OutOfScopeObject {
                        symbol: *symbol,
                        spelling: spelling.clone(),
                        span: *span,
                    });
                }
                slot.value
                    .ok_or_else(|| InterpreterError::ReadOfUninitializedObject {
                        symbol: *symbol,
                        spelling: spelling.clone(),
                        span: *span,
                    })
            }
            CHirExpr::Unary(operator, operand, _) => {
                let operand = self.eval_expr(operand, false)?;
                let result = if self.static_integer_evaluation {
                    eval_constant_unary_integer(*operator, operand)
                } else {
                    self.runtime_integer_unary(*operator, operand)
                };
                result.map_err(|error| {
                    let error_span = if *operator == crate::UnaryOp::Neg {
                        unary_operator_span(span, expression_operand_span(expression))
                    } else {
                        span
                    };
                    self.integer_error(error, error_span)
                })
            }
            CHirExpr::EvaluateOnce(operand, _) => {
                let previous = self.nested_mutation_depth;
                self.nested_mutation_depth = previous.saturating_add(1);
                let result = self.eval_expr(operand, true);
                self.nested_mutation_depth = previous;
                result
            }
            CHirExpr::BuiltinExpect(value, expected, _) => {
                let value = self.eval_expr(value, true)?;
                self.eval_expr(expected, true)?;
                Ok(value)
            }
            CHirExpr::Binary(operator, left, right, _) => {
                self.eval_binary(*operator, left, right, span)
            }
            CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
                let then_type = self.infer_integer_type(then_expression)?;
                let else_type = self.infer_integer_type(else_expression)?;
                let result_type = usual_arithmetic_type(
                    CInteger::new(0, then_type).promoted().ty(),
                    CInteger::new(0, else_type).promoted().ty(),
                );
                let selected = if self.eval_expr(condition, true)?.truth() {
                    self.eval_expr(then_expression, true)?
                } else {
                    self.eval_expr(else_expression, true)?
                };
                Ok(selected.cast(result_type))
            }
            CHirExpr::Elvis(operand, else_expression, _) => {
                let operand_type = self.infer_integer_type(operand)?;
                let else_type = self.infer_integer_type(else_expression)?;
                let result_type = usual_arithmetic_type(
                    CInteger::new(0, operand_type).promoted().ty(),
                    CInteger::new(0, else_type).promoted().ty(),
                );
                // The omitted middle operand does not turn the controlling
                // expression into an unevaluated or mutation-free context.
                let operand_value = self.eval_expr(operand, true)?;
                let selected = if operand_value.truth() {
                    operand_value
                } else {
                    self.eval_expr(else_expression, true)?
                };
                Ok(selected.cast(result_type))
            }
            CHirExpr::BitBuiltin { op, operand, .. } => {
                let operand = self.eval_expr(operand, allow_root_mutation)?;
                eval_bit_builtin(*op, operand)
                    .ok_or(InterpreterError::BitBuiltinZeroOperand { span })
            }
            CHirExpr::BuiltinUnreachable { .. } => {
                Err(InterpreterError::UnreachableReached { span })
            }
            CHirExpr::BuiltinTrap { .. } => Err(InterpreterError::BuiltinTrapReached { span }),
            CHirExpr::BuiltinReturnAddress { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "return-address builtin",
                span,
            }),
            CHirExpr::BuiltinFrameAddress { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "frame-address builtin",
                span,
            }),
            CHirExpr::BuiltinVarArgs { .. } => Err(InterpreterError::VarArgs { span }),
            CHirExpr::BuiltinVarArgPack { .. } => Err(InterpreterError::VarArgPack { span }),
            CHirExpr::LabelAddress { .. } => Err(InterpreterError::ComputedGoto { span }),
            CHirExpr::CharLit(literal, _) => Ok(literal.integer_value()),
            CHirExpr::Cast(ty, value, _) => {
                let target = self.integer_type(*ty, span)?;
                Ok(self.eval_expr(value, allow_root_mutation)?.cast(target))
            }
            CHirExpr::SizeOf(ty, _) | CHirExpr::SizeOfExpr(_, ty, _) => {
                let size = self
                    .target
                    .sizeof_type(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "sizeof incomplete or unsupported type",
                        span,
                    })?;
                Ok(CInteger::new(size, CIntegerType::ULONG))
            }
            CHirExpr::AlignOf(ty, _) => {
                let alignment = self
                    .target
                    .alignof_type(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "alignof incomplete or unsupported type",
                        span,
                    })?;
                Ok(CInteger::new(u64::from(alignment), CIntegerType::ULONG))
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
                self.eval_assignment(*op, *place, target, value, span)
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
                self.eval_update(*op, *place, target, span)
            }
            CHirExpr::Comma(left, right, _) => {
                self.eval_expr(left, false)?;
                self.eval_expr(right, false)
            }
            CHirExpr::FloatLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::Call(_, _, _)
            | CHirExpr::CheckedArithmetic { .. }
            | CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _)
            | CHirExpr::StatementExpr { .. }
            | CHirExpr::CompoundLit(_, _, _)
            | CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::MemoryRead { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "outside scalar Day-5 expression subset",
                span,
            }),
        }
    }

    pub(super) fn infer_integer_type(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<CIntegerType, InterpreterError> {
        enum TypeTask<'a> {
            Infer(&'a CHirExpr<'a>),
            Cache(usize),
            Promote,
            Common,
        }
        // Type inference is unevaluated but still must not consume the host
        // call stack. Cache immutable expression identities so nested selected
        // conditionals do not repeatedly walk their entire remaining subtree.
        let mut pending = vec![TypeTask::Infer(expression)];
        let mut values = Vec::new();
        while let Some(task) = pending.pop() {
            let expression = match task {
                TypeTask::Cache(key) => {
                    self.integer_expression_types
                        .insert(key, *values.last().expect("inferred type"));
                    continue;
                }
                TypeTask::Promote => {
                    let ty = values.pop().expect("unary operand type");
                    values.push(CInteger::new(0, ty).promoted().ty());
                    continue;
                }
                TypeTask::Common => {
                    let right = values.pop().expect("right operand type");
                    let left = values.pop().expect("left operand type");
                    values.push(usual_arithmetic_type(
                        CInteger::new(0, left).promoted().ty(),
                        CInteger::new(0, right).promoted().ty(),
                    ));
                    continue;
                }
                TypeTask::Infer(expression) => expression,
            };
            let key = expression as *const CHirExpr<'arena> as usize;
            if let Some(ty) = self.integer_expression_types.get(&key) {
                values.push(*ty);
                continue;
            }
            pending.push(TypeTask::Cache(key));
            let span = expression.span();
            let immediate = match expression {
                CHirExpr::Atomic(access) => self.integer_type(access.facts.ty(), span),
                CHirExpr::Floating(access) => self.integer_type(access.facts.ty(), span),
                CHirExpr::Vector(access) => self.integer_type(access.facts.ty(), span),
                CHirExpr::Complex(access) => self.integer_type(access.facts.ty(), span),
                CHirExpr::Lit(literal, _) => integer_literal_value(*literal)
                    .map(|value| value.ty())
                    .map_err(|error| self.integer_error(error, span)),
                CHirExpr::CharLit(literal, _) => Ok(literal.integer_value().ty()),
                CHirExpr::Ident { facts, .. }
                | CHirExpr::BuiltinConstantP { facts, .. }
                | CHirExpr::BuiltinObjectSize { facts, .. }
                | CHirExpr::CheckedArithmetic { facts, .. }
                | CHirExpr::BitBuiltin { facts, .. }
                | CHirExpr::Assign { facts, .. }
                | CHirExpr::MemoryAssign { facts, .. }
                | CHirExpr::MemoryRead { facts, .. }
                | CHirExpr::Update { facts, .. }
                | CHirExpr::MemoryUpdate { facts, .. }
                | CHirExpr::StatementExpr { facts, .. } => self.integer_type(facts.ty(), span),
                CHirExpr::Unary(operator, operand, _) => match operator {
                    crate::UnaryOp::Not => Ok(CIntegerType::INT),
                    crate::UnaryOp::Plus
                    | crate::UnaryOp::Neg
                    | crate::UnaryOp::Abs
                    | crate::UnaryOp::BitNot => {
                        pending.push(TypeTask::Promote);
                        pending.push(TypeTask::Infer(operand));
                        continue;
                    }
                    crate::UnaryOp::Deref => {
                        // Conditional conversions inspect both arm types, not
                        // their values. Resolve the pointee without accessing
                        // the object or evaluating the pointer expression.
                        let pointer = self.pointer_type_of_expression(operand)?;
                        self.integer_type(pointer.pointee(), span)
                    }
                    crate::UnaryOp::AddrOf => Err(InterpreterError::UnsupportedExpression {
                        feature: "non-integer unary operand",
                        span,
                    }),
                },
                CHirExpr::EvaluateOnce(operand, _) => {
                    pending.push(TypeTask::Infer(operand));
                    continue;
                }
                CHirExpr::Binary(operator, left, right, _) => match operator {
                    BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::And
                    | BinaryOp::Or => Ok(CIntegerType::INT),
                    BinaryOp::Shl | BinaryOp::Shr => {
                        pending.push(TypeTask::Promote);
                        pending.push(TypeTask::Infer(left));
                        continue;
                    }
                    BinaryOp::Comma => {
                        pending.push(TypeTask::Infer(right));
                        continue;
                    }
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor => {
                        pending.push(TypeTask::Common);
                        pending.push(TypeTask::Infer(right));
                        pending.push(TypeTask::Infer(left));
                        continue;
                    }
                    BinaryOp::Assign
                    | BinaryOp::AddAssign
                    | BinaryOp::SubAssign
                    | BinaryOp::MulAssign
                    | BinaryOp::DivAssign
                    | BinaryOp::ModAssign
                    | BinaryOp::ShlAssign
                    | BinaryOp::ShrAssign
                    | BinaryOp::BitAndAssign
                    | BinaryOp::BitOrAssign
                    | BinaryOp::BitXorAssign => Err(InterpreterError::UnsupportedExpression {
                        feature: "unlowered assignment binary operator",
                        span,
                    }),
                },
                CHirExpr::Ternary(_, left, right, _) | CHirExpr::Elvis(left, right, _) => {
                    pending.push(TypeTask::Common);
                    pending.push(TypeTask::Infer(right));
                    pending.push(TypeTask::Infer(left));
                    continue;
                }
                CHirExpr::Cast(ty, _, _) => self.integer_type(*ty, span),
                CHirExpr::BuiltinExpect(..) => Ok(CIntegerType::LONG),
                CHirExpr::Call(callee, _, _) => {
                    let function_type = self.call_designator_function_type(callee).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "indirect scalar function call",
                            span: callee.span(),
                        },
                    )?;
                    let Some(TypeKind::Function { return_type, .. }) =
                        self.unit.types().get(function_type).map(|ty| &ty.kind)
                    else {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "inconsistent function designator",
                            span: callee.span(),
                        });
                    };
                    self.integer_type(*return_type, span)
                }
                CHirExpr::SizeOf(_, _)
                | CHirExpr::SizeOfExpr(_, _, _)
                | CHirExpr::AlignOf(_, _) => Ok(CIntegerType::ULONG),
                CHirExpr::Comma(_, right, _) => {
                    pending.push(TypeTask::Infer(right));
                    continue;
                }
                CHirExpr::FloatLit(_, _)
                | CHirExpr::StringLit(_, _)
                | CHirExpr::LabelAddress { .. }
                | CHirExpr::BuiltinTrap { .. }
                | CHirExpr::BuiltinUnreachable { .. }
                | CHirExpr::BuiltinReturnAddress { .. }
                | CHirExpr::BuiltinFrameAddress { .. }
                | CHirExpr::BuiltinVarArgs { .. }
                | CHirExpr::Member(_, _, _)
                | CHirExpr::PtrMember(_, _, _)
                | CHirExpr::Index(_, _, _)
                | CHirExpr::CompoundLit(_, _, _) => Err(InterpreterError::UnsupportedExpression {
                    feature: "non-integer expression type",
                    span,
                }),
                CHirExpr::BuiltinVarArgPack { .. } => Err(InterpreterError::VarArgPack { span }),
            }?;
            values.push(immediate);
        }
        Ok(values.pop().expect("root integer expression type"))
    }

    pub(super) fn eval_binary(
        &mut self,
        operator: BinaryOp,
        left: &'arena CHirExpr<'arena>,
        right: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        match operator {
            BinaryOp::And => {
                let left = self.eval_expr(left, true)?;
                if !left.truth() {
                    Ok(CInteger::int(0))
                } else {
                    Ok(CInteger::int(i32::from(
                        self.eval_expr(right, true)?.truth(),
                    )))
                }
            }
            BinaryOp::Or => {
                let left = self.eval_expr(left, true)?;
                if left.truth() {
                    Ok(CInteger::int(1))
                } else {
                    Ok(CInteger::int(i32::from(
                        self.eval_expr(right, true)?.truth(),
                    )))
                }
            }
            BinaryOp::Comma => {
                self.eval_expr(left, false)?;
                self.eval_expr(right, false)
            }
            BinaryOp::Assign
            | BinaryOp::AddAssign
            | BinaryOp::SubAssign
            | BinaryOp::MulAssign
            | BinaryOp::DivAssign
            | BinaryOp::ModAssign
            | BinaryOp::ShlAssign
            | BinaryOp::ShrAssign
            | BinaryOp::BitAndAssign
            | BinaryOp::BitOrAssign
            | BinaryOp::BitXorAssign => Err(InterpreterError::UnsupportedExpression {
                feature: "unlowered assignment binary operator",
                span,
            }),
            _ => {
                let left = self.eval_expr(left, false)?;
                let right = self.eval_expr(right, false)?;
                if self.static_integer_evaluation {
                    eval_constant_binary_integer(operator, left, right)
                        .map_err(|error| self.integer_error(error, span))
                } else {
                    self.runtime_integer_binary(operator, left, right, span)
                }
            }
        }
    }

    pub(super) fn validate_place(
        &self,
        place: ScalarPlace,
        target: &CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<SymbolId, InterpreterError> {
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span: target_span,
            ..
        } = target
        else {
            return Err(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span,
            });
        };
        let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
            InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            }
        })?;
        if is_global_register(self.unit, *symbol) {
            return Err(InterpreterError::Asm { span: *target_span });
        }
        let slot = self
            .slots
            .get(symbol)
            .ok_or(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            })?;
        if !slot.active {
            return Err(InterpreterError::OutOfScopeObject {
                symbol: *symbol,
                spelling: declaration.spelling().to_owned(),
                span: *target_span,
            });
        }
        if *symbol != place.symbol()
            || !matches!(
                declaration.kind(),
                SymbolKind::Object | SymbolKind::Parameter
            )
            || declaration.spelling() != spelling
            || declaration.ty() != place.ty()
            || facts.ty() != place.ty()
            || facts.category() != ExprCategory::LValue
            || !facts.is_modifiable_lvalue()
            || canonical_integer_type(self.unit.types(), place.ty()) != Some(slot.ty)
        {
            return Err(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            });
        }
        Ok(*symbol)
    }

    pub(super) fn eval_assignment(
        &mut self,
        operator: AssignmentOp,
        place: ScalarPlace,
        target: &CHirExpr<'arena>,
        value: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        let symbol = self.validate_place(place, target, span)?;
        let right = self.eval_expr(value, false)?;
        self.apply_assignment_value(operator, symbol, right, target.span(), span)
    }

    pub(super) fn apply_assignment_value(
        &mut self,
        operator: AssignmentOp,
        symbol: SymbolId,
        right: CInteger,
        target_span: SourceSpan,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        let slot = *self
            .slots
            .get(&symbol)
            .ok_or(InterpreterError::InvalidScalarPlace { symbol, span })?;
        let result = if operator == AssignmentOp::Simple {
            right.cast(slot.ty)
        } else {
            let left = slot.value.ok_or_else(|| {
                let spelling = self
                    .unit
                    .symbols()
                    .symbol(symbol)
                    .map(|symbol| symbol.spelling().to_owned())
                    .unwrap_or_else(|_| format!("symbol#{}", symbol.raw()));
                InterpreterError::ReadOfUninitializedObject {
                    symbol,
                    spelling,
                    span: target_span,
                }
            })?;
            let binary =
                assignment_binary(operator).ok_or(InterpreterError::UnsupportedExpression {
                    feature: "invalid compound assignment operator",
                    span,
                })?;
            self.runtime_integer_binary(binary, left, right, span)?
                .cast(slot.ty)
        };
        self.slots
            .get_mut(&symbol)
            .expect("validated scalar place has a slot")
            .value = Some(result);
        Ok(result)
    }

    pub(super) fn eval_update(
        &mut self,
        operator: UpdateOp,
        place: ScalarPlace,
        target: &CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        let symbol = self.validate_place(place, target, span)?;
        let slot = *self
            .slots
            .get(&symbol)
            .ok_or(InterpreterError::InvalidScalarPlace { symbol, span })?;
        let previous = slot.value.ok_or_else(|| {
            let spelling = self
                .unit
                .symbols()
                .symbol(symbol)
                .map(|symbol| symbol.spelling().to_owned())
                .unwrap_or_else(|_| format!("symbol#{}", symbol.raw()));
            InterpreterError::ReadOfUninitializedObject {
                symbol,
                spelling,
                span: target.span(),
            }
        })?;
        let binary = match operator {
            UpdateOp::PreIncrement | UpdateOp::PostIncrement => BinaryOp::Add,
            UpdateOp::PreDecrement | UpdateOp::PostDecrement => BinaryOp::Sub,
        };
        let updated = self
            .runtime_integer_binary(binary, previous, CInteger::int(1), span)?
            .cast(slot.ty);
        self.slots
            .get_mut(&symbol)
            .expect("validated scalar place has a slot")
            .value = Some(updated);
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                updated
            } else {
                previous
            },
        )
    }
}
