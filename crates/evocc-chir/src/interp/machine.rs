//! C-HIR interpreter machine.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn run_with_calls(
        mut self,
        function: SymbolId,
    ) -> Result<InterpretOutcome, InterpreterError> {
        let outcome = self
            .run_root(function)?
            .expect("function invocation has a result");
        Ok(InterpretOutcome {
            return_value: outcome.return_value,
            steps: self.consumed,
            memory: self.memory,
            world: self.world,
        })
    }

    pub(super) fn run_root(
        &mut self,
        function: SymbolId,
    ) -> Result<Option<MachineOutcome>, InterpreterError> {
        self.run_root_arguments(function, Vec::new())
    }

    pub(super) fn run_root_arguments(
        &mut self,
        function: SymbolId,
        arguments: Vec<CValue>,
    ) -> Result<Option<MachineOutcome>, InterpreterError> {
        let mut tasks = Vec::new();
        let root_span = self
            .unit
            .symbols()
            .symbol(function)
            .map(|symbol| symbol.span())
            .map_err(|_| InterpreterError::UnknownFunction { symbol: function })?;
        self.enter_function(function, arguments, None, None, None, false, 0, &mut tasks)?;
        let (outcome, _) = self.run_tasks(tasks)?;
        if self.termination.is_some() {
            return Ok(None);
        }
        let outcome = outcome.ok_or(InterpreterError::UnsupportedStatement {
            feature: "interpreter call stack ended without a root result",
            span: root_span,
        })?;
        Ok(Some(outcome))
    }

    pub(super) fn run_tasks(
        &mut self,
        mut tasks: Vec<MachineTask<'arena>>,
    ) -> Result<(Option<MachineOutcome>, Vec<CValue>), InterpreterError> {
        let mut values = Vec::new();
        while let Some(task) = tasks.pop() {
            if self.termination.is_some() {
                return Ok((None, values));
            }
            let outcome = match task {
                MachineTask::BeginConstantRepresentationStore { place, bits, span } => {
                    self.tick(span)?;
                    let completion = tasks.len();
                    self.schedule_place_operands(&place, span, &mut tasks, &mut values)?;
                    // Operand tasks must run before the store. Insert the
                    // completion below them, using the same scheduling order
                    // as every other projected memory access.
                    tasks.insert(
                        completion,
                        MachineTask::FinishConstantRepresentationStore { place, bits, span },
                    );
                    None
                }
                MachineTask::FinishConstantRepresentationStore { place, bits, span } => {
                    let subscripts = place
                        .projections()
                        .iter()
                        .filter(|projection| projection.source_expression().is_some())
                        .count();
                    let address =
                        self.resolve_projected_address(&place, subscripts, &mut values, span)?;
                    let layout = self.place_access_layout(&place, span)?;
                    if place.access_qualifiers().is_volatile {
                        let (memory, world) = self
                            .memory
                            .volatile_store_integer(&self.world, address, layout, bits.raw_bits())
                            .map_err(|e| self.memory_error(e, span))?;
                        self.memory = memory;
                        self.world = world;
                    } else {
                        self.memory = self
                            .memory
                            .store_integer(address, layout, bits.raw_bits())
                            .map_err(|e| self.memory_error(e, span))?;
                    }
                    None
                }
                MachineTask::IndirectGoto(span) => {
                    let value = pop_machine_value(&mut values, span)?;
                    let error = || InterpreterError::InvalidIndirectBranchTarget { span };
                    let address = match value {
                        CValue::Pointer { value, .. } => {
                            self.memory.pointer_address(PointerValue::object(value))
                        }
                        CValue::FunctionPointer { value, .. } => self.memory.pointer_address(value),
                        CValue::Uninitialized(_) => {
                            return Err(InterpreterError::UninitializedValue { span })
                        }
                        _ => return Err(error()),
                    }
                    .map_err(|_| error())?;
                    let PointerValue::Function(identity) = self
                        .memory
                        .resolve_function_pointer(PointerValue::Exposed(address))
                        .map_err(|_| error())?
                    else {
                        return Err(error());
                    };
                    let current = tasks
                        .iter()
                        .rev()
                        .find_map(|task| match task {
                            MachineTask::FunctionEnd(end) => Some(end.symbol),
                            _ => None,
                        })
                        .ok_or_else(error)?;
                    let label = self
                        .symbol_order()
                        .labels
                        .values()
                        .find(|(owner, _, id)| *owner == current && *id == identity)
                        .map(|(_, label, _)| label.clone())
                        .ok_or_else(error)?;
                    self.unwind_goto(&mut tasks, &label, span)?;
                    None
                }
                MachineTask::ApplyVarArgs {
                    op,
                    requested_ty,
                    span,
                } => {
                    self.apply_varargs(op, requested_ty, span, &mut values)?;
                    None
                }
                MachineTask::FunctionEnd(end) => {
                    self.finish_function(end, FunctionExit::FellOffEnd, &mut values)?
                }
                MachineTask::RestoreSlots(saved) => {
                    self.restore_slots(saved);
                    None
                }
                MachineTask::ExitMemoryScope(exit) => {
                    self.exit_memory_scope(exit)?;
                    None
                }
                MachineTask::RestoreFuelAnchor(previous) => {
                    self.fuel_anchor = previous;
                    None
                }
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                    None
                }
                MachineTask::RestoreFenvAccess(previous) => {
                    self.fenv_access = previous;
                    None
                }
                MachineTask::ExecDeclaration(declaration) => {
                    self.schedule_machine_declaration(declaration, &mut tasks)?;
                    None
                }
                MachineTask::ExecStatement(statement) => {
                    self.exec_machine_statement(statement, &mut tasks, &mut values)?
                }
                MachineTask::SwitchEnd => None,
                MachineTask::CompoundContinue { items, index } => {
                    if let Some(item) = items.get(index) {
                        tasks.push(MachineTask::CompoundContinue {
                            items,
                            index: index + 1,
                        });
                        tasks.push(match item {
                            CHirBlockItem::Declaration(declaration) => {
                                MachineTask::ExecDeclaration(declaration)
                            }
                            CHirBlockItem::Statement(statement) => {
                                MachineTask::ExecStatement(statement)
                            }
                        });
                    }
                    None
                }
                MachineTask::PreserveStatementExpressionValue { parent, span } => {
                    // A complete-object rvalue must survive the closing brace;
                    // a pointer value still points at the original object and
                    // must not acquire an extended lifetime here.
                    if let Some(value @ CValue::Aggregate { ty, .. }) = values.last().copied() {
                        let layout = self.access_layout(ty, span)?;
                        let frame = self
                            .memory_frame
                            .as_ref()
                            .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                        let (memory, destination) = self
                            .memory
                            .allocate_automatic(
                                frame.id,
                                parent,
                                self.temporary_origin_site,
                                layout,
                            )
                            .map_err(|error| self.memory_error(error, span))?;
                        self.memory = memory;
                        let preserved = self.store_memory_value(
                            destination,
                            ty,
                            Qualifiers::default(),
                            value,
                            span,
                        )?;
                        *values
                            .last_mut()
                            .expect("statement expression value checked") = preserved;
                    }
                    None
                }
                MachineTask::StatementExpressionContinue {
                    items,
                    index,
                    value,
                    span,
                } => {
                    if let Some(item) = items.get(index) {
                        let item_value = match item {
                            CHirBlockItem::Statement(statement) => {
                                statement_expression_value(statement)
                            }
                            CHirBlockItem::Declaration(_) => None,
                        };
                        let is_value = matches!((item_value, value), (Some(item), Some(value)) if std::ptr::eq(item, value));
                        if index + 1 < items.len() {
                            tasks.push(MachineTask::StatementExpressionContinue {
                                items,
                                index: index + 1,
                                value,
                                span,
                            });
                        }
                        if is_value {
                            let expression = item_value.expect(
                                "the statement-expression value predicate checked this shape",
                            );
                            if crate::expression_is_void(
                                expression,
                                self.unit.symbols(),
                                self.unit.types(),
                            ) {
                                self.schedule_discarded_expression(expression, &mut tasks)?;
                            } else {
                                tasks.push(MachineTask::EvalExpression {
                                    expression,
                                    allow_root_mutation: true,
                                });
                            }
                        } else {
                            tasks.push(match item {
                                CHirBlockItem::Declaration(declaration) => {
                                    MachineTask::ExecDeclaration(declaration)
                                }
                                CHirBlockItem::Statement(statement) => {
                                    MachineTask::ExecStatement(statement)
                                }
                            });
                        }
                    } else if value.is_some() {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "statement expression result is not its final statement",
                            span,
                        });
                    }
                    None
                }
                MachineTask::WhileCheck { condition, body } => {
                    tasks.push(MachineTask::AfterWhileCondition {
                        condition,
                        body,
                        span: condition.span(),
                    });
                    tasks.push(MachineTask::EvalExpression {
                        expression: condition,
                        allow_root_mutation: true,
                    });
                    None
                }
                MachineTask::AfterWhileCondition {
                    condition,
                    body,
                    span,
                } => {
                    if self.complex_truth(pop_machine_value(&mut values, span)?, span)? {
                        tasks.push(MachineTask::WhileCheck { condition, body });
                        tasks.push(MachineTask::ExecStatement(body));
                    }
                    None
                }
                MachineTask::DoWhileCondition { condition, body } => {
                    tasks.push(MachineTask::AfterDoWhileCondition {
                        condition,
                        body,
                        span: condition.span(),
                    });
                    tasks.push(MachineTask::EvalExpression {
                        expression: condition,
                        allow_root_mutation: true,
                    });
                    None
                }
                MachineTask::AfterDoWhileCondition {
                    condition,
                    body,
                    span,
                } => {
                    if self.complex_truth(pop_machine_value(&mut values, span)?, span)? {
                        tasks.push(MachineTask::DoWhileCondition { condition, body });
                        tasks.push(MachineTask::ExecStatement(body));
                    }
                    None
                }
                MachineTask::ForCheck {
                    condition,
                    step,
                    body,
                    span,
                } => {
                    if let Some(condition) = condition {
                        tasks.push(MachineTask::AfterForCondition {
                            condition: Some(condition),
                            step,
                            body,
                            span,
                        });
                        tasks.push(MachineTask::EvalExpression {
                            expression: condition,
                            allow_root_mutation: true,
                        });
                    } else {
                        tasks.push(MachineTask::ForStep {
                            condition,
                            step,
                            body,
                            span,
                        });
                        tasks.push(MachineTask::ExecStatement(body));
                    }
                    None
                }
                MachineTask::AfterForCondition {
                    condition,
                    step,
                    body,
                    span,
                } => {
                    if self.complex_truth(pop_machine_value(&mut values, span)?, span)? {
                        tasks.push(MachineTask::ForStep {
                            condition,
                            step,
                            body,
                            span,
                        });
                        tasks.push(MachineTask::ExecStatement(body));
                    }
                    None
                }
                MachineTask::ForStep {
                    condition,
                    step,
                    body,
                    span,
                } => {
                    if let Some(step) = step {
                        tasks.push(MachineTask::ForCheck {
                            condition,
                            step: Some(step),
                            body,
                            span,
                        });
                        self.schedule_discarded_expression(step, &mut tasks)?;
                    } else {
                        tasks.push(MachineTask::ForCheck {
                            condition,
                            step,
                            body,
                            span,
                        });
                    }
                    None
                }
                MachineTask::DiscardExpression(span) => {
                    pop_machine_value(&mut values, span)?;
                    None
                }
                MachineTask::FinishInitializer { symbol, ty, span } => {
                    let value =
                        expect_machine_integer(pop_machine_value(&mut values, span)?, span)?
                            .cast(ty);
                    let slot = self
                        .slots
                        .get_mut(&symbol)
                        .ok_or(InterpreterError::MalformedObjectDeclaration { symbol, span })?;
                    if slot.ty != ty || !slot.active {
                        return Err(InterpreterError::MalformedObjectDeclaration { symbol, span });
                    }
                    slot.value = Some(value);
                    None
                }
                MachineTask::FinishMemoryInitializer { symbol, span } => {
                    let value = pop_machine_value(&mut values, span)?;
                    self.store_bound_value(symbol, value, span)?;
                    None
                }
                MachineTask::ContinueMemoryInitializer { target, index } => {
                    self.continue_memory_initializer(target, index, &mut tasks, &mut values)?;
                    None
                }
                MachineTask::FinishMemorySubobject { target, index } => {
                    let action = target
                        .initializer
                        .plan()
                        .actions()
                        .get(index)
                        .ok_or_else(|| invalid_initializer_target(target, target.span))?;
                    let (path, span) = match action {
                        InitAction::StoreScalar { path, span, .. }
                        | InitAction::CopyAggregate { path, span, .. } => (path, span),
                        InitAction::StoreString { .. } | InitAction::ZeroScalar { .. } => {
                            return Err(invalid_initializer_target(target, target.span));
                        }
                    };
                    let value = pop_machine_value(&mut values, *span)?;
                    let (address, ty, bit_field) =
                        self.subobject_address(target.binding, path, *span)?;
                    let value =
                        self.cast_runtime_value(value, self.value_type(ty, *span)?, *span)?;
                    let value = if let Some(shared) = target.initializer.plan().shared_value(index)
                    {
                        if let Some(cached) = self
                            .initializer_values
                            .get(&target.binding.address)
                            .and_then(|values| values.get(&shared))
                            .copied()
                        {
                            cached
                        } else {
                            // An aggregate value must not remain a view of
                            // mutable source storage between range stores.
                            let value = if let CValue::Aggregate { ty, .. } = value {
                                let snapshot = self.vector_temporary(ty, *span)?;
                                self.store_memory_value(
                                    snapshot,
                                    ty,
                                    Qualifiers::default(),
                                    value,
                                    *span,
                                )?
                            } else {
                                value
                            };
                            self.initializer_values
                                .entry(target.binding.address)
                                .or_default()
                                .insert(shared, value);
                            value
                        }
                    } else {
                        value
                    };
                    if let Some(field) = bit_field {
                        self.store_bit_field(address, ty, field, value, false, *span)?;
                    } else {
                        self.store_initialized_value(address, ty, value, *span)?;
                    }
                    None
                }
                MachineTask::YieldCompoundLiteral {
                    binding,
                    as_address,
                    span,
                } => {
                    let pointer_type = PointerTypeDesc::to_object(self.unit.types(), binding.ty)
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: binding.ty,
                            span,
                        })?;
                    if as_address
                        || matches!(
                            self.unit.types().get(binding.ty).map(|ty| &ty.kind),
                            Some(TypeKind::Array { .. })
                        )
                    {
                        values.push(CValue::Pointer {
                            value: binding.address,
                            ty: if as_address {
                                pointer_type
                            } else {
                                let TypeKind::Array { element, .. } = self
                                    .unit
                                    .types()
                                    .get(binding.ty)
                                    .expect("compound type checked above")
                                    .kind
                                else {
                                    unreachable!()
                                };
                                PointerTypeDesc::to_object(self.unit.types(), element).ok_or(
                                    InterpreterError::UnsupportedType { ty: element, span },
                                )?
                            },
                        });
                    } else {
                        let qualifiers = self
                            .unit
                            .types()
                            .get(binding.ty)
                            .ok_or(InterpreterError::UnsupportedType {
                                ty: binding.ty,
                                span,
                            })?
                            .qualifiers;
                        values.push(self.load_memory_value(
                            binding.address,
                            binding.ty,
                            qualifiers,
                            span,
                        )?);
                    }
                    None
                }
                MachineTask::FinishReturnValue(span) => {
                    let value = pop_machine_value(&mut values, span)?;
                    let target = tasks
                        .iter()
                        .rev()
                        .find_map(|task| match task {
                            MachineTask::FunctionEnd(end) => Some(end.return_value_type),
                            _ => None,
                        })
                        .ok_or(InterpreterError::UnsupportedStatement {
                            feature: "missing return continuation",
                            span,
                        })?;
                    // Convert under the return statement's policy before unwinding.
                    let value = match value {
                        CValue::Uninitialized(source) if source == target => value,
                        _ => self.cast_runtime_value(value, target, span)?,
                    };
                    let end = self.pop_function_end(&mut tasks, span)?;
                    self.finish_function(end, FunctionExit::Explicit(Some(value)), &mut values)?
                }
                MachineTask::FinishVoidReturn(span) => {
                    let end = self.pop_function_end(&mut tasks, span)?;
                    self.finish_function(end, FunctionExit::Explicit(None), &mut values)?
                }
                MachineTask::DiscardSelectedExpression {
                    then_expression,
                    else_expression,
                } => {
                    let selected = if self.complex_truth(
                        pop_machine_value(&mut values, then_expression.span())?,
                        then_expression.span(),
                    )? {
                        then_expression
                    } else {
                        else_expression
                    };
                    self.schedule_discarded_expression(selected, &mut tasks)?;
                    None
                }
                MachineTask::SelectIf {
                    then_statement,
                    else_statement,
                } => {
                    let condition = self.complex_truth(
                        pop_machine_value(&mut values, then_statement.span())?,
                        then_statement.span(),
                    )?;
                    if condition {
                        tasks.push(MachineTask::ExecStatement(then_statement));
                    } else if let Some(else_statement) = else_statement {
                        tasks.push(MachineTask::ExecStatement(else_statement));
                    }
                    None
                }
                MachineTask::EvalExpression {
                    expression,
                    allow_root_mutation,
                } => {
                    self.schedule_expression(
                        expression,
                        allow_root_mutation,
                        &mut tasks,
                        &mut values,
                    )?;
                    None
                }
                MachineTask::ApplyFloating(access) => {
                    let start = values.len().checked_sub(access.operands.len()).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "floating operand stack",
                            span: access.span,
                        },
                    )?;
                    let arguments = values.split_off(start);
                    let strict = self.strict_floating();
                    values.push(floating::builtin(
                        access,
                        &arguments,
                        &mut self.world,
                        strict,
                    )?);
                    None
                }
                MachineTask::ApplyVector(access) => {
                    let start = values.len().checked_sub(access.operands.len()).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "vector operand stack",
                            span: access.span,
                        },
                    )?;
                    let arguments = values.split_off(start);
                    values.push(self.apply_vector(access, &arguments)?);
                    None
                }
                MachineTask::ApplyComplex(access) => {
                    let start = values.len().checked_sub(access.operands.len()).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "complex operand stack",
                            span: access.span,
                        },
                    )?;
                    let arguments = values.split_off(start);
                    values.push(self.apply_complex(access, &arguments)?);
                    None
                }
                MachineTask::ApplyAtomic(access) => {
                    // Operands were evaluated once each, left to right; a
                    // dynamic order only selects seq_cst.
                    if access.failure.is_some() {
                        expect_machine_integer(
                            pop_machine_value(&mut values, access.span)?,
                            access.span,
                        )?;
                    }
                    let _order = expect_machine_integer(
                        pop_machine_value(&mut values, access.span)?,
                        access.span,
                    )?;
                    if access.weak.is_some() {
                        pop_machine_value(&mut values, access.span)?;
                    }
                    let result = if access.result.is_some() {
                        Some(pop_machine_value(&mut values, access.span)?)
                    } else {
                        None
                    };
                    let stored = if access.value.is_some() {
                        Some(pop_machine_value(&mut values, access.span)?)
                    } else {
                        None
                    };
                    let expected = if access.expected.is_some() {
                        Some(pop_machine_value(&mut values, access.span)?)
                    } else {
                        None
                    };
                    let address = pop_machine_value(&mut values, access.span)?;
                    values.push(if access.memory {
                        self.apply_atomic_copy(access, address, stored, expected, result)?
                    } else {
                        self.apply_atomic(access, address, stored, expected)?
                    });
                    None
                }
                MachineTask::ApplyUnary {
                    operator,
                    expression_span,
                    operand_span,
                } => {
                    let operand = pop_machine_value(&mut values, expression_span)?;
                    if operator == UnaryOp::Not {
                        values.push(CValue::Integer(CInteger::int(i32::from(
                            !self.complex_truth(operand, expression_span)?,
                        ))));
                        continue;
                    }
                    if let CValue::Floating(value) = operand {
                        values.push(CValue::Floating(match operator {
                            UnaryOp::Plus => value,
                            // Negation of a half operand evaluates in
                            // binary32, which quiets a signaling NaN.
                            UnaryOp::Neg if value.kind().is_half() => {
                                let strict = self.strict_floating();
                                floating::convert_runtime(
                                    operand,
                                    evocc_types::FloatKind::F32,
                                    expression_span,
                                    &mut self.world,
                                    strict,
                                )?
                                .negated()
                            }
                            UnaryOp::Neg => value.negated(),
                            _ => {
                                return Err(InterpreterError::UnsupportedExpression {
                                    feature: "invalid floating unary operator",
                                    span: expression_span,
                                })
                            }
                        }));
                        continue;
                    }
                    let operand = expect_machine_integer(operand, expression_span)?;
                    let value = if self.static_integer_evaluation {
                        eval_constant_unary_integer(operator, operand)
                    } else {
                        self.runtime_integer_unary(operator, operand)
                    }
                    .map_err(|error| {
                        let error_span = if operator == crate::UnaryOp::Neg {
                            unary_operator_span(expression_span, operand_span)
                        } else {
                            expression_span
                        };
                        self.integer_error(error, error_span)
                    })?;
                    values.push(CValue::Integer(value));
                    None
                }
                MachineTask::ContinueBinary {
                    operator,
                    right,
                    span,
                    allow_root_mutation,
                } => {
                    let left = pop_machine_value(&mut values, span)?;
                    match operator {
                        BinaryOp::And if !self.complex_truth(left, span)? => {
                            values.push(CValue::Integer(CInteger::int(0)));
                        }
                        BinaryOp::Or if self.complex_truth(left, span)? => {
                            values.push(CValue::Integer(CInteger::int(1)));
                        }
                        BinaryOp::Comma => {
                            tasks.push(MachineTask::EvalExpression {
                                expression: right,
                                allow_root_mutation,
                            });
                        }
                        _ => {
                            tasks.push(MachineTask::ApplyBinary {
                                operator,
                                left,
                                span,
                            });
                            tasks.push(MachineTask::EvalExpression {
                                expression: right,
                                allow_root_mutation: matches!(
                                    operator,
                                    BinaryOp::And | BinaryOp::Or
                                ),
                            });
                        }
                    }
                    None
                }
                MachineTask::ApplyBinary {
                    operator,
                    left,
                    span,
                } => {
                    let right = pop_machine_value(&mut values, span)?;
                    values.push(self.apply_machine_binary(operator, left, right, span)?);
                    None
                }
                MachineTask::ApplyCheckedArithmetic {
                    operator,
                    target_type,
                    span,
                } => {
                    let (address, pointer_type) =
                        expect_machine_pointer(pop_machine_value(&mut values, span)?, span)?;
                    if pointer_type.pointee() != target_type {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "inconsistent checked-arithmetic target type",
                            span,
                        });
                    }
                    let right =
                        expect_machine_integer(pop_machine_value(&mut values, span)?, span)?;
                    let left = expect_machine_integer(pop_machine_value(&mut values, span)?, span)?;
                    let target_integer = self.integer_type(target_type, span)?;
                    let (stored, overflowed) =
                        eval_checked_arithmetic(operator, left, right, target_integer);
                    let qualifiers = self
                        .unit
                        .types()
                        .get(target_type)
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: target_type,
                            span,
                        })?
                        .qualifiers;
                    self.store_memory_value(
                        address,
                        target_type,
                        qualifiers,
                        CValue::Integer(stored),
                        span,
                    )?;
                    values.push(CValue::Integer(CInteger::int(i32::from(overflowed))));
                    None
                }
                MachineTask::ApplyBitBuiltin { operator, span } => {
                    let operand =
                        expect_machine_integer(pop_machine_value(&mut values, span)?, span)?;
                    let value = eval_bit_builtin(operator, operand)
                        .ok_or(InterpreterError::BitBuiltinZeroOperand { span })?;
                    values.push(CValue::Integer(value));
                    None
                }
                MachineTask::SelectTernary {
                    then_expression,
                    else_expression,
                    result_type,
                } => {
                    let condition = self.complex_truth(
                        pop_machine_value(&mut values, then_expression.span())?,
                        then_expression.span(),
                    )?;
                    tasks.push(MachineTask::CastValue {
                        target: result_type,
                        span: then_expression.span(),
                    });
                    tasks.push(MachineTask::EvalExpression {
                        expression: if condition {
                            then_expression
                        } else {
                            else_expression
                        },
                        allow_root_mutation: true,
                    });
                    None
                }
                MachineTask::SelectElvis {
                    else_expression,
                    result_type,
                    span,
                } => {
                    let operand = pop_machine_value(&mut values, span)?;
                    if self.complex_truth(operand, span)? {
                        values.push(self.cast_runtime_value(operand, result_type, span)?);
                    } else {
                        tasks.push(MachineTask::CastValue {
                            target: result_type,
                            span,
                        });
                        tasks.push(MachineTask::EvalExpression {
                            expression: else_expression,
                            allow_root_mutation: true,
                        });
                    }
                    None
                }
                MachineTask::CastValue { target, span } => {
                    let value = pop_machine_value(&mut values, span)?;
                    values.push(self.cast_runtime_value(value, target, span)?);
                    None
                }
                MachineTask::ApplyAssignment {
                    operator,
                    symbol,
                    target_span,
                    span,
                } => {
                    let right = pop_machine_value(&mut values, span)?;
                    if self.memory_mode {
                        values.push(self.apply_bound_assignment(
                            operator,
                            symbol,
                            right,
                            target_span,
                            span,
                        )?);
                    } else {
                        let right = expect_machine_integer(right, span)?;
                        values.push(CValue::Integer(self.apply_assignment_value(
                            operator,
                            symbol,
                            right,
                            target_span,
                            span,
                        )?));
                    }
                    None
                }
                MachineTask::LoadDereference { pointer_type, span } => {
                    let (address, actual_type) =
                        expect_machine_pointer(pop_machine_value(&mut values, span)?, span)?;
                    if actual_type != pointer_type {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "inconsistent pointer type at dereference",
                            span,
                        });
                    }
                    values.push(self.load_memory_value(
                        address,
                        pointer_type.pointee(),
                        pointer_type.pointee_qualifiers(),
                        span,
                    )?);
                    None
                }
                MachineTask::ApplyScalarUpdate {
                    operator,
                    symbol,
                    target_span,
                    span,
                } => {
                    values.push(self.apply_scalar_update(operator, symbol, target_span, span)?);
                    None
                }
                MachineTask::ApplyIndirectUpdate {
                    operator,
                    bit_field,
                    layout,
                    value_type,
                    qualifiers,
                    target_span,
                    span,
                } => {
                    let (address, _) =
                        expect_machine_pointer(pop_machine_value(&mut values, span)?, target_span)?;
                    if qualifiers.is_atomic {
                        let (old, new) = self.update_atomic_object(
                            address,
                            value_type,
                            qualifiers.is_volatile,
                            layout,
                            span,
                            |this, old| this.step_update_value(old, operator, span),
                        )?;
                        values.push(match operator {
                            UpdateOp::PreIncrement | UpdateOp::PreDecrement => new,
                            UpdateOp::PostIncrement | UpdateOp::PostDecrement => old,
                        });
                        continue;
                    }
                    let old = if let Some(field) = bit_field {
                        self.load_bit_field(
                            address,
                            value_type,
                            field,
                            qualifiers.is_volatile,
                            target_span,
                        )?
                    } else {
                        self.load_memory_value_with_layout(
                            address,
                            value_type,
                            qualifiers,
                            target_span,
                            layout,
                        )?
                    };
                    let stepped = self.step_update_value(old, operator, span)?;
                    let stepped = if let Some(field) = bit_field {
                        self.store_bit_field(
                            address,
                            value_type,
                            field,
                            stepped,
                            qualifiers.is_volatile,
                            span,
                        )?
                    } else {
                        self.store_memory_value_with_layout(
                            address, value_type, qualifiers, stepped, span, layout,
                        )?
                    };
                    // A prefix form yields the updated object; a postfix form
                    // yields the value it held before the update.
                    values.push(match operator {
                        UpdateOp::PreIncrement | UpdateOp::PreDecrement => stepped,
                        UpdateOp::PostIncrement | UpdateOp::PostDecrement => old,
                    });
                    None
                }
                MachineTask::FoldPlaceAddress { place, span } => {
                    let subscripts = place
                        .projections()
                        .iter()
                        .filter(|projection| projection.source_expression().is_some())
                        .count();
                    let address =
                        self.resolve_projected_address(place, subscripts, &mut values, span)?;
                    values.push(CValue::Pointer {
                        value: address,
                        ty: PointerTypeDesc::from_addressed_place(place),
                    });
                    None
                }
                MachineTask::LoadProjectedPlace { place, span } => {
                    let subscripts = place
                        .projections()
                        .iter()
                        .filter(|projection| projection.source_expression().is_some())
                        .count();
                    let address =
                        self.resolve_projected_address(place, subscripts, &mut values, span)?;
                    let value = if let Some(field) = self.place_bit_field(place, span)? {
                        self.load_bit_field(
                            address,
                            place.value_ty(),
                            field,
                            place.access_qualifiers().is_volatile,
                            span,
                        )?
                    } else {
                        self.load_memory_value_with_layout(
                            address,
                            place.value_ty(),
                            place.access_qualifiers(),
                            span,
                            self.place_access_layout(place, span)?,
                        )?
                    };
                    values.push(value);
                    None
                }
                MachineTask::LoadAggregateMember { name, span } => {
                    let aggregate = pop_machine_value(&mut values, span)?;
                    let (address, ty, qualifiers) =
                        self.aggregate_member_address(aggregate, name, span)?;
                    values.push(self.load_memory_value(address, ty, qualifiers, span)?);
                    None
                }
                MachineTask::ApplyIndirectAssignment {
                    operator,
                    bit_field,
                    layout,
                    pointer_type,
                    value_type,
                    qualifiers,
                    target_span,
                    span,
                } => {
                    let right = pop_machine_value(&mut values, span)?;
                    let (address, actual_type) =
                        expect_machine_pointer(pop_machine_value(&mut values, span)?, target_span)?;
                    if actual_type != pointer_type || pointer_type.pointee() != value_type {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "inconsistent pointer type at memory assignment",
                            span: target_span,
                        });
                    }
                    if let Some(field) = bit_field {
                        let right = expect_machine_integer(right, span)?;
                        let result = if operator == AssignmentOp::Simple {
                            right
                        } else {
                            let left = expect_machine_integer(
                                self.load_bit_field(
                                    address,
                                    value_type,
                                    field,
                                    qualifiers.is_volatile,
                                    target_span,
                                )?,
                                target_span,
                            )?;
                            let binary = assignment_binary(operator).ok_or(
                                InterpreterError::UnsupportedExpression {
                                    feature: "invalid bit-field assignment",
                                    span,
                                },
                            )?;
                            self.runtime_integer_binary(binary, left, right, span)?
                        };
                        values.push(self.store_bit_field(
                            address,
                            value_type,
                            field,
                            CValue::Integer(result),
                            qualifiers.is_volatile,
                            span,
                        )?);
                    } else {
                        values.push(self.apply_memory_assignment(
                            operator,
                            address,
                            value_type,
                            qualifiers,
                            right,
                            target_span,
                            span,
                            layout,
                        )?);
                    }
                    None
                }
                MachineTask::AllocateDynamic { expression } => {
                    let span = expression.span();
                    let size = expect_machine_integer(pop_machine_value(&mut values, span)?, span)?
                        .cast(CIntegerType::ULONG)
                        .raw_bits() as u64;
                    let origin = self
                        .manifest
                        .as_ref()
                        .ok_or(InterpreterError::MissingAllocationManifest)?
                        .dynamic_stack_site(expression)
                        .ok_or(InterpreterError::UnsupportedExpression {
                            feature: "dynamic allocation has no site",
                            span,
                        })?
                        .ordinal()
                        .raw();
                    let frame = self.memory_frame.as_ref().ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "dynamic allocation has no frame",
                            span,
                        },
                    )?;
                    let (memory, value) = self
                        .memory
                        .allocate_automatic(
                            frame.id,
                            frame.root_scope,
                            origin,
                            AccessLayout::new(size, 16),
                        )
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    let ty = PointerTypeDesc::from_expression(
                        self.unit.symbols(),
                        self.unit.types(),
                        expression,
                    )
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "dynamic allocation has no pointer type",
                        span,
                    })?;
                    values.push(CValue::Pointer { value, ty });
                    None
                }
                MachineTask::SelectSwitch { body, span } => {
                    let controlling =
                        expect_machine_integer(pop_machine_value(&mut values, span)?, span)?;
                    self.schedule_switch(controlling, body, &mut tasks)?;
                    None
                }
                MachineTask::EvalCallArguments {
                    callee,
                    arguments,
                    index,
                    values: arguments_so_far,
                    span,
                    expects_value,
                } => {
                    if index == 0 {
                        self.check_neon_immediates(callee, arguments, span)?;
                    }
                    if index == arguments.len() {
                        if matches!(
                            callee,
                            CHirExpr::Ident {
                                facts,
                                ..
                            } if facts.category() == ExprCategory::FunctionDesignator
                        ) {
                            self.check_old_style_call(callee, arguments, &arguments_so_far, span)?;
                            let value_base = values.len();
                            if let Some(result) = self.start_call(
                                callee,
                                arguments_so_far,
                                span,
                                expects_value,
                                &mut tasks,
                                value_base,
                            )? {
                                if expects_value {
                                    values.push(result);
                                }
                            }
                        } else {
                            if self.call_designator_function_type(callee).is_none() {
                                return Err(InterpreterError::UnsupportedExpression {
                                    feature: "indirect scalar function call",
                                    span: callee.span(),
                                });
                            }
                            tasks.push(MachineTask::StartIndirectCall {
                                arguments: arguments_so_far,
                                span,
                                expects_value,
                                value_base: values.len(),
                            });
                            tasks.push(MachineTask::EvalExpression {
                                expression: callee,
                                allow_root_mutation: false,
                            });
                        }
                    } else {
                        tasks.push(MachineTask::CollectCallArgument {
                            callee,
                            arguments,
                            index,
                            values: arguments_so_far,
                            span,
                            expects_value,
                        });
                        tasks.push(MachineTask::EvalExpression {
                            expression: arguments[index],
                            allow_root_mutation: false,
                        });
                    }
                    None
                }
                MachineTask::CollectCallArgument {
                    callee,
                    arguments,
                    index,
                    values: mut arguments_so_far,
                    span,
                    expects_value,
                } => {
                    arguments_so_far.push(pop_machine_value(&mut values, span)?);
                    tasks.push(MachineTask::EvalCallArguments {
                        callee,
                        arguments,
                        index: index + 1,
                        values: arguments_so_far,
                        span,
                        expects_value,
                    });
                    None
                }
                MachineTask::StartIndirectCall {
                    arguments,
                    span,
                    expects_value,
                    value_base,
                } => {
                    let callee = pop_machine_value(&mut values, span)?;
                    if let Some(result) = self.start_indirect_call(
                        callee,
                        arguments,
                        span,
                        expects_value,
                        &mut tasks,
                        value_base,
                    )? {
                        if expects_value {
                            values.push(result);
                        }
                    }
                    None
                }
            };
            if let Some(outcome) = outcome {
                return Ok((Some(outcome), values));
            }
        }
        Ok((None, values))
    }
}
