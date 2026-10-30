//! C-HIR interpreter control flow.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn schedule_switch(
        &mut self,
        controlling: CInteger,
        body: &'arena CHirStmt<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let controlling = controlling.promoted();
        let mut labels = Vec::new();
        collect_switch_labels(body, &mut labels);
        let mut selected = None;
        let mut default = None;
        for statement in labels {
            let matches = match statement {
                CHirStmt::Case(value, _, case_span) => {
                    let value = self
                        .eval_static_integer_expression(value)?
                        .cast(controlling.ty());
                    eval_binary_integer(BinaryOp::Eq, controlling, value)
                        .map_err(|error| self.integer_error(error, *case_span))?
                        .truth()
                }
                CHirStmt::CaseRange(first, last, _, _) => {
                    let first = self
                        .eval_static_integer_expression(first)?
                        .cast(controlling.ty());
                    let last = self
                        .eval_static_integer_expression(last)?
                        .cast(controlling.ty());
                    eval_binary_integer(BinaryOp::Le, first, controlling)
                        .map_err(|error| self.integer_error(error, statement.span()))?
                        .truth()
                        && eval_binary_integer(BinaryOp::Le, controlling, last)
                            .map_err(|error| self.integer_error(error, statement.span()))?
                            .truth()
                }
                CHirStmt::Default(_, _) => {
                    default = Some(statement);
                    false
                }
                _ => unreachable!("only labels of this switch are collected"),
            };
            if matches {
                selected = Some(statement);
                break;
            }
        }
        if let Some(statement) = selected.or(default) {
            // Dispatch is a jump to the label, not execution from the body's
            // first statement. Reuse goto's scope and loop continuations.
            tasks.push(MachineTask::SwitchEnd);
            self.schedule_goto_in_statement(
                body,
                StatementDestination::Case(statement as *const CHirStmt<'_> as usize),
                tasks,
            )?;
        }
        Ok(())
    }

    pub(super) fn pop_function_end(
        &mut self,
        tasks: &mut Vec<MachineTask<'arena>>,
        span: SourceSpan,
    ) -> Result<FunctionEnd<'arena>, InterpreterError> {
        while let Some(task) = tasks.pop() {
            match task {
                MachineTask::FunctionEnd(end) => return Ok(end),
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                // Return copies an aggregate while its source is still alive.
                // finish_function ends the entire frame after that copy, so
                // individual lexical exits must not destroy it first.
                MachineTask::ExitMemoryScope(_) => {}
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
                MachineTask::RestoreFenvAccess(previous) => self.fenv_access = previous,
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                }
                _ => {}
            }
        }
        Err(InterpreterError::UnsupportedStatement {
            feature: "malformed interpreter call continuation",
            span,
        })
    }

    pub(super) fn unwind_break(
        &mut self,
        tasks: &mut Vec<MachineTask<'arena>>,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        while let Some(task) = tasks.pop() {
            match task {
                MachineTask::WhileCheck { .. }
                | MachineTask::DoWhileCondition { .. }
                | MachineTask::ForStep { .. }
                | MachineTask::SwitchEnd => return Ok(()),
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                MachineTask::ExitMemoryScope(exit) => self.exit_memory_scope(exit)?,
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
                MachineTask::RestoreFenvAccess(previous) => self.fenv_access = previous,
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                }
                MachineTask::FunctionEnd(end) => {
                    tasks.push(MachineTask::FunctionEnd(end));
                    break;
                }
                _ => {}
            }
        }
        Err(InterpreterError::InvalidControlFlow {
            flow: "break outside loop or switch",
            span,
        })
    }

    pub(super) fn unwind_continue(
        &mut self,
        tasks: &mut Vec<MachineTask<'arena>>,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        while let Some(task) = tasks.pop() {
            match task {
                task @ (MachineTask::WhileCheck { .. }
                | MachineTask::DoWhileCondition { .. }
                | MachineTask::ForStep { .. }) => {
                    tasks.push(task);
                    return Ok(());
                }
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                MachineTask::ExitMemoryScope(exit) => self.exit_memory_scope(exit)?,
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
                MachineTask::RestoreFenvAccess(previous) => self.fenv_access = previous,
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                }
                MachineTask::FunctionEnd(end) => {
                    tasks.push(MachineTask::FunctionEnd(end));
                    break;
                }
                _ => {}
            }
        }
        Err(InterpreterError::InvalidControlFlow {
            flow: "continue outside loop",
            span,
        })
    }

    pub(super) fn unwind_goto(
        &mut self,
        tasks: &mut Vec<MachineTask<'arena>>,
        spelling: &str,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let destination = StatementDestination::Label(spelling);
        while let Some(task) = tasks.pop() {
            match task {
                MachineTask::StatementExpressionContinue {
                    items, value, span, ..
                } => {
                    if self.schedule_goto_in_active_items(
                        items,
                        destination,
                        MachineBlockMode::StatementExpression { value, span },
                        tasks,
                    )? {
                        return Ok(());
                    }
                }
                MachineTask::CompoundContinue { items, .. } => {
                    if self.schedule_goto_in_active_items(
                        items,
                        destination,
                        MachineBlockMode::Compound,
                        tasks,
                    )? {
                        return Ok(());
                    }
                }
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                MachineTask::ExitMemoryScope(exit) => self.exit_memory_scope(exit)?,
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
                MachineTask::RestoreFenvAccess(previous) => self.fenv_access = previous,
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                }
                MachineTask::FunctionEnd(end) => {
                    let body = self.function_definition(end.symbol)?.body.ok_or(
                        InterpreterError::FunctionHasNoBody {
                            symbol: end.symbol,
                            span: end.body_span,
                        },
                    )?;
                    tasks.push(MachineTask::FunctionEnd(end));
                    if self.schedule_goto_in_statement(body, destination, tasks)? {
                        return Ok(());
                    }
                    break;
                }
                _ => {}
            }
        }
        Err(InterpreterError::UnknownLabel {
            spelling: spelling.to_owned(),
            span,
        })
    }

    pub(super) fn schedule_goto_in_active_items(
        &mut self,
        items: &'arena [CHirBlockItem<'arena>],
        destination: StatementDestination<'_>,
        mode: MachineBlockMode<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<bool, InterpreterError> {
        for (index, item) in items.iter().enumerate() {
            let CHirBlockItem::Statement(statement) = item else {
                continue;
            };
            if !statement_contains_destination(statement, destination) {
                continue;
            }
            if self.memory_mode {
                // A backward jump before a VLA declaration leaves that
                // object's declaration scope even within the same compound.
                // Fixed automatic objects keep their scope-wide identity.
                for skipped in items[index..].iter().rev() {
                    let CHirBlockItem::Declaration(CHirDecl::Object(object)) = skipped else {
                        continue;
                    };
                    if self
                        .target
                        .array_size_factors(self.unit.types(), object.ty)
                        .is_ok_and(|(_, captures)| !captures.is_empty())
                    {
                        if let Some(binding) = self.memory_binding(object.symbol) {
                            let frame = self.memory_frame.as_mut().expect("active memory scope");
                            frame.bindings.remove(&object.symbol);
                            let id = frame.id;
                            self.memory = self
                                .memory
                                .end_object_lifetime(id, binding.address)
                                .map_err(|error| self.memory_error(error, object.span))?;
                        }
                    }
                }
                // A jump can enter a block after automatic declarations.
                // Their storage exists, but their initializers did not run.
                // Existing bindings belong to this active scope and must not
                // be replaced on a backward jump.
                for skipped in &items[..index] {
                    let CHirBlockItem::Declaration(CHirDecl::Object(object)) = skipped else {
                        continue;
                    };
                    if matches!(
                        object.storage,
                        StorageClass::None | StorageClass::Auto | StorageClass::Register
                    ) && self.memory_binding(object.symbol).is_none()
                    {
                        self.validate_memory_object_declaration(object)?;
                        self.allocate_automatic_object(
                            object.symbol,
                            object.ty,
                            object.initializer.as_ref(),
                            object.span,
                        )?;
                    }
                }
            }
            if let MachineBlockMode::StatementExpression { value, span } = mode {
                // Re-enter a directly labelled item through the expression
                // continuation so a labelled final expression still yields
                // its value rather than being evaluated and discarded.
                if destination.matches(statement) {
                    tasks.push(MachineTask::StatementExpressionContinue {
                        items,
                        index,
                        value,
                        span,
                    });
                    return Ok(true);
                }
            }
            tasks.push(match mode {
                MachineBlockMode::Compound => MachineTask::CompoundContinue {
                    items,
                    index: index + 1,
                },
                MachineBlockMode::StatementExpression { value, span } => {
                    MachineTask::StatementExpressionContinue {
                        items,
                        index: index + 1,
                        value,
                        span,
                    }
                }
            });
            return self.schedule_goto_in_statement(statement, destination, tasks);
        }
        Ok(false)
    }

    pub(super) fn schedule_goto_in_statement(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
        destination: StatementDestination<'_>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<bool, InterpreterError> {
        if !statement_contains_destination(statement, destination) {
            return Ok(false);
        }
        if destination.matches(statement) {
            tasks.push(MachineTask::ExecStatement(statement));
            return Ok(true);
        }
        match statement {
            CHirStmt::Label(_, child, _) => {
                self.schedule_goto_in_statement(child, destination, tasks)
            }
            CHirStmt::Compound(items, _, policy) => {
                self.enter_fenv_scope(*policy, tasks);
                let saved = self.activate_block_items(items)?;
                tasks.push(MachineTask::RestoreSlots(saved));
                if let Some(exit) = self.enter_memory_scope(statement.span())? {
                    tasks.push(MachineTask::ExitMemoryScope(exit));
                }
                self.schedule_goto_in_active_items(
                    items,
                    destination,
                    MachineBlockMode::Compound,
                    tasks,
                )
            }
            CHirStmt::If(_, then_statement, else_statement, _) => {
                if statement_contains_destination(then_statement, destination) {
                    self.schedule_goto_in_statement(then_statement, destination, tasks)
                } else if let Some(else_statement) = else_statement {
                    self.schedule_goto_in_statement(else_statement, destination, tasks)
                } else {
                    Ok(false)
                }
            }
            CHirStmt::Switch(_, body, _) => {
                tasks.push(MachineTask::SwitchEnd);
                self.schedule_goto_in_statement(body, destination, tasks)
            }
            CHirStmt::While(condition, body, loop_span) => {
                let previous = self.fuel_anchor.replace(*loop_span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::WhileCheck { condition, body });
                self.schedule_goto_in_statement(body, destination, tasks)
            }
            CHirStmt::DoWhile(body, condition, loop_span) => {
                let previous = self.fuel_anchor.replace(*loop_span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::DoWhileCondition { condition, body });
                self.schedule_goto_in_statement(body, destination, tasks)
            }
            CHirStmt::For(initializer, condition, step, body, loop_span) => {
                let previous = self.fuel_anchor.replace(*loop_span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                if let CHirForInit::Declaration(declarations) = initializer {
                    let saved = self.activate_for_declarations(declarations)?;
                    tasks.push(MachineTask::RestoreSlots(saved));
                    if let Some(exit) = self.enter_memory_scope(*loop_span)? {
                        tasks.push(MachineTask::ExitMemoryScope(exit));
                    }
                }
                tasks.push(MachineTask::ForStep {
                    condition: *condition,
                    step: *step,
                    body,
                    span: *loop_span,
                });
                self.schedule_goto_in_statement(body, destination, tasks)
            }
            CHirStmt::Case(_, child, _)
            | CHirStmt::CaseRange(_, _, child, _)
            | CHirStmt::Default(child, _) => {
                self.schedule_goto_in_statement(child, destination, tasks)
            }
            CHirStmt::Expr(_, _)
            | CHirStmt::Asm(_)
            | CHirStmt::Return(_, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::ComputedGoto(_, _)
            | CHirStmt::AttributedNull(_, _)
            | CHirStmt::Null(_) => Ok(false),
        }
    }
}

pub(super) fn expression_operand_span(expression: &CHirExpr<'_>) -> SourceSpan {
    match expression {
        CHirExpr::Unary(_, operand, _) => operand.span(),
        _ => expression.span(),
    }
}

pub(super) fn unary_operator_span(outer: SourceSpan, operand: SourceSpan) -> SourceSpan {
    if outer.file == operand.file
        && outer.line_start == operand.line_start
        && outer.byte_start < operand.byte_start
        && outer.byte_end >= operand.byte_end
    {
        SourceSpan::new(
            operand.file,
            operand.byte_start - 1,
            operand.byte_end,
            operand.line_start,
            operand.column_start.saturating_sub(1),
        )
    } else {
        outer
    }
}

pub(super) fn collect_switch_labels<'a>(
    statement: &'a CHirStmt<'a>,
    labels: &mut Vec<&'a CHirStmt<'a>>,
) {
    match statement {
        CHirStmt::Case(_, child, _)
        | CHirStmt::CaseRange(_, _, child, _)
        | CHirStmt::Default(child, _) => {
            labels.push(statement);
            collect_switch_labels(child, labels);
        }
        CHirStmt::If(_, then_branch, else_branch, _) => {
            collect_switch_labels(then_branch, labels);
            if let Some(branch) = else_branch {
                collect_switch_labels(branch, labels);
            }
        }
        CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Label(_, body, _) => collect_switch_labels(body, labels),
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    collect_switch_labels(statement, labels);
                }
            }
        }
        // A nested switch owns its own case labels.
        _ => {}
    }
}

pub(super) fn statement_contains_destination(
    statement: &CHirStmt<'_>,
    destination: StatementDestination<'_>,
) -> bool {
    if destination.matches(statement) {
        return true;
    }
    match statement {
        CHirStmt::Label(_, child, _) => statement_contains_destination(child, destination),
        CHirStmt::If(_, then_statement, else_statement, _) => {
            statement_contains_destination(then_statement, destination)
                || else_statement
                    .is_some_and(|statement| statement_contains_destination(statement, destination))
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::Default(body, _) => statement_contains_destination(body, destination),
        CHirStmt::DoWhile(body, _, _) => statement_contains_destination(body, destination),
        CHirStmt::Compound(items, _, _) => items.iter().any(|item| {
            matches!(
                item,
                CHirBlockItem::Statement(statement)
                    if statement_contains_destination(statement, destination)
            )
        }),
        CHirStmt::Expr(_, _)
        | CHirStmt::Asm(_)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::ComputedGoto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => false,
    }
}

pub(super) fn validate_function_labels(
    statement: &CHirStmt<'_>,
    labels: &mut BTreeMap<String, SourceSpan>,
) -> Result<(), InterpreterError> {
    match statement {
        CHirStmt::If(_, then_statement, else_statement, _) => {
            validate_function_labels(then_statement, labels)?;
            if let Some(else_statement) = else_statement {
                validate_function_labels(else_statement, labels)?;
            }
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::Default(body, _) => validate_function_labels(body, labels)?,
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    validate_function_labels(statement, labels)?;
                }
            }
        }
        CHirStmt::Label(name, body, span) => {
            if labels.insert(name.spelling.clone(), *span).is_some() {
                return Err(InterpreterError::DuplicateLabel {
                    spelling: name.spelling.clone(),
                    span: *span,
                });
            }
            validate_function_labels(body, labels)?;
        }
        CHirStmt::Expr(_, _)
        | CHirStmt::Asm(_)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::ComputedGoto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
    Ok(())
}

pub(super) fn validate_function_control_flow(
    statement: &CHirStmt<'_>,
) -> Result<(), InterpreterError> {
    let mut labels = BTreeMap::new();
    validate_function_labels(statement, &mut labels)?;
    validate_function_gotos(statement, &labels)
}

pub(super) fn validate_function_gotos(
    statement: &CHirStmt<'_>,
    labels: &BTreeMap<String, SourceSpan>,
) -> Result<(), InterpreterError> {
    match statement {
        CHirStmt::If(_, then_statement, else_statement, _) => {
            validate_function_gotos(then_statement, labels)?;
            if let Some(else_statement) = else_statement {
                validate_function_gotos(else_statement, labels)?;
            }
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::Default(body, _)
        | CHirStmt::Label(_, body, _) => validate_function_gotos(body, labels)?,
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    validate_function_gotos(statement, labels)?;
                }
            }
        }
        CHirStmt::Goto(name, span) if !labels.contains_key(&name.spelling) => {
            return Err(InterpreterError::UnknownLabel {
                spelling: name.spelling.clone(),
                span: *span,
            });
        }
        CHirStmt::Expr(_, _)
        | CHirStmt::Asm(_)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::ComputedGoto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
    Ok(())
}
