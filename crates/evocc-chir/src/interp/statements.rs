//! C-HIR interpreter statements.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn enter_fenv_scope(
        &mut self,
        policy: Option<bool>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) {
        if let Some(enabled) = policy {
            tasks.push(MachineTask::RestoreFenvAccess(std::mem::replace(
                &mut self.fenv_access,
                enabled,
            )));
        }
    }

    pub(super) fn exec_machine_statement(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
        values: &mut Vec<CValue>,
    ) -> Result<Option<MachineOutcome>, InterpreterError> {
        self.tick(statement.span())?;
        match statement {
            CHirStmt::Asm(assembly) => Err(InterpreterError::Asm {
                span: assembly.span,
            }),
            CHirStmt::Expr(expression, _) => {
                self.schedule_discarded_expression(expression, tasks)?;
                Ok(None)
            }
            CHirStmt::Return(Some(expression), span) => {
                if self.expression_is_void(expression) {
                    tasks.push(MachineTask::FinishVoidReturn(*span));
                    self.schedule_discarded_expression(expression, tasks)?;
                } else {
                    tasks.push(MachineTask::FinishReturnValue(*span));
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
                Ok(None)
            }
            CHirStmt::Return(None, span) => {
                let end = self.pop_function_end(tasks, *span)?;
                self.finish_function(end, FunctionExit::Explicit(None), values)
            }
            CHirStmt::Compound(items, _, policy) => {
                self.enter_fenv_scope(*policy, tasks);
                let saved = self.activate_block_items(items)?;
                tasks.push(MachineTask::RestoreSlots(saved));
                if let Some(exit) = self.enter_memory_scope(statement.span())? {
                    tasks.push(MachineTask::ExitMemoryScope(exit));
                }
                tasks.push(MachineTask::CompoundContinue { items, index: 0 });
                Ok(None)
            }
            CHirStmt::If(condition, then_statement, else_statement, _) => {
                tasks.push(MachineTask::SelectIf {
                    then_statement,
                    else_statement: *else_statement,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: condition,
                    allow_root_mutation: true,
                });
                Ok(None)
            }
            CHirStmt::Switch(expression, body, span) => {
                tasks.push(MachineTask::SelectSwitch { body, span: *span });
                tasks.push(MachineTask::EvalExpression {
                    expression,
                    allow_root_mutation: true,
                });
                Ok(None)
            }
            CHirStmt::While(condition, body, span) => {
                let previous = self.fuel_anchor.replace(*span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::WhileCheck { condition, body });
                Ok(None)
            }
            CHirStmt::DoWhile(body, condition, span) => {
                let previous = self.fuel_anchor.replace(*span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::DoWhileCondition { condition, body });
                tasks.push(MachineTask::ExecStatement(body));
                Ok(None)
            }
            CHirStmt::For(initializer, condition, step, body, span) => {
                let previous = self.fuel_anchor.replace(*span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                if let CHirForInit::Declaration(declarations) = initializer {
                    let saved = self.activate_for_declarations(declarations)?;
                    tasks.push(MachineTask::RestoreSlots(saved));
                    if let Some(exit) = self.enter_memory_scope(*span)? {
                        tasks.push(MachineTask::ExitMemoryScope(exit));
                    }
                }
                tasks.push(MachineTask::ForCheck {
                    condition: *condition,
                    step: *step,
                    body,
                    span: *span,
                });
                match initializer {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(expression) => {
                        self.schedule_discarded_expression(expression, tasks)?;
                    }
                    CHirForInit::Declaration(declarations) => {
                        for declaration in declarations.iter().rev() {
                            tasks.push(MachineTask::ExecDeclaration(declaration));
                        }
                    }
                }
                Ok(None)
            }
            CHirStmt::Break(span) => {
                self.unwind_break(tasks, *span)?;
                Ok(None)
            }
            CHirStmt::Continue(span) => {
                self.unwind_continue(tasks, *span)?;
                Ok(None)
            }
            CHirStmt::Goto(name, span) => {
                self.unwind_goto(tasks, &name.spelling, *span)?;
                Ok(None)
            }
            CHirStmt::ComputedGoto(expression, span) => {
                tasks.push(MachineTask::IndirectGoto(*span));
                tasks.push(MachineTask::EvalExpression {
                    expression,
                    allow_root_mutation: true,
                });
                Ok(None)
            }
            CHirStmt::Label(_, child, _) => {
                tasks.push(MachineTask::ExecStatement(child));
                Ok(None)
            }
            CHirStmt::Case(_, child, _)
            | CHirStmt::CaseRange(_, _, child, _)
            | CHirStmt::Default(child, _) => {
                if !tasks
                    .iter()
                    .rev()
                    .take_while(|task| !matches!(task, MachineTask::FunctionEnd(_)))
                    .any(|task| matches!(task, MachineTask::SwitchEnd))
                {
                    return Err(InterpreterError::InvalidControlFlow {
                        flow: "case outside switch",
                        span: statement.span(),
                    });
                }
                tasks.push(MachineTask::ExecStatement(child));
                Ok(None)
            }
            CHirStmt::AttributedNull(_, _) | CHirStmt::Null(_) => Ok(None),
        }
    }

    pub(super) fn schedule_machine_declaration(
        &mut self,
        declaration: &'arena CHirDecl<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        self.tick(declaration.span())?;
        let declaration = match declaration {
            CHirDecl::Object(declaration) => declaration,
            CHirDecl::Asm(_) | CHirDecl::Tag(_) | CHirDecl::Typedef(_) => return Ok(()),
            CHirDecl::Function(function) if function.body.is_none() => return Ok(()),
            CHirDecl::Function(declaration) => {
                return Err(InterpreterError::UnsupportedDeclaration {
                    feature: "block function declaration",
                    span: declaration.span,
                });
            }
        };
        if self.memory_mode {
            return self.schedule_memory_declaration(declaration, tasks);
        }
        let ty = self.validate_object_declaration(declaration)?;
        self.validate_initializer_plan(declaration)?;
        let slot = self.slots.get_mut(&declaration.symbol).ok_or(
            InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            },
        )?;
        if slot.ty != ty || !slot.active {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        }
        slot.value = None;
        let Some(initializer) = &declaration.initializer else {
            if self.unit.zero_initialize_automatic(declaration) {
                slot.value = Some(CInteger::new(0, ty));
            }
            return Ok(());
        };
        let action = initializer.plan().actions().first().ok_or(
            InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            },
        )?;
        match action {
            InitAction::StoreScalar {
                path,
                destination_type,
                value,
                span,
            } if path.is_root() && *destination_type == declaration.ty => {
                tasks.push(MachineTask::FinishInitializer {
                    symbol: declaration.symbol,
                    ty,
                    span: *span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: true,
                });
                Ok(())
            }
            InitAction::ZeroScalar {
                path,
                destination_type,
            } if path.is_root() && *destination_type == declaration.ty => {
                let slot = self.slots.get_mut(&declaration.symbol).ok_or(
                    InterpreterError::MalformedObjectDeclaration {
                        symbol: declaration.symbol,
                        span: declaration.span,
                    },
                )?;
                slot.value = Some(CInteger::new(0, ty));
                Ok(())
            }
            InitAction::StoreScalar { span, .. }
            | InitAction::StoreString { span, .. }
            | InitAction::CopyAggregate { span, .. } => {
                Err(InterpreterError::InvalidInitializerPlan {
                    symbol: declaration.symbol,
                    span: *span,
                })
            }
            InitAction::ZeroScalar { .. } => Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            }),
        }
    }

    pub(super) fn schedule_memory_declaration(
        &mut self,
        declaration: &'arena ObjectDecl<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        self.validate_memory_object_declaration(declaration)?;
        self.validate_initializer_plan(declaration)?;
        if matches!(
            declaration.storage.base(),
            StorageClass::Static | StorageClass::Extern
        ) {
            if self.memory_binding(declaration.symbol).is_none() {
                return Err(InterpreterError::MalformedObjectDeclaration {
                    symbol: declaration.symbol,
                    span: declaration.span,
                });
            }
            return Ok(());
        }
        if self.memory_binding(declaration.symbol).is_none() {
            self.allocate_automatic_object(
                declaration.symbol,
                declaration.ty,
                declaration.initializer.as_ref(),
                declaration.span,
            )?;
        }
        if let Some(cleanup) = declaration.cleanup {
            let frame = self
                .memory_frame
                .as_mut()
                .expect("automatic object has a frame");
            frame.cleanups.push((frame.current_scope, cleanup));
        }
        let Some(initializer) = &declaration.initializer else {
            if self.unit.zero_initialize_automatic(declaration) {
                let binding = self.require_memory_binding(declaration.symbol, declaration.span)?;
                let layout = self.object_access_layout(declaration.ty, None, declaration.span)?;
                if layout.size_bytes != 0 {
                    let bytes = ByteSequence::try_from_bytes(vec![0; layout.size_bytes as usize])
                        .map_err(|error| self.memory_error(error, declaration.span))?;
                    self.memory = self
                        .memory
                        .store_bytes(binding.address, layout, &bytes)
                        .map_err(|error| self.memory_error(error, declaration.span))?;
                }
                self.zero_initialize_subobjects(
                    binding.address,
                    declaration.ty,
                    SubobjectPath::root(),
                    &[],
                    declaration.span,
                )?;
            }
            return Ok(());
        };
        if self.unit.zero_initialize_automatic_padding(declaration) {
            let binding = self.require_memory_binding(declaration.symbol, declaration.span)?;
            self.zero_constructor_padding(binding, initializer, declaration.span)?;
        }
        if initializer.plan().zero_fills_unwritten() {
            let binding = self.require_memory_binding(declaration.symbol, declaration.span)?;
            self.zero_initialize_subobjects(
                binding.address,
                declaration.ty,
                SubobjectPath::root(),
                initializer.plan().actions(),
                declaration.span,
            )?;
        }
        if initializer.plan().actions().is_empty() && !initializer.plan().zero_fills_unwritten() {
            return Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            });
        }
        let binding = self.require_memory_binding(declaration.symbol, declaration.span)?;
        tasks.push(MachineTask::ContinueMemoryInitializer {
            target: InitializerTarget {
                binding,
                initializer,
                symbol: Some(declaration.symbol),
                span: declaration.span,
            },
            index: 0,
        });
        Ok(())
    }
}
