//! C-HIR interpreter discarded.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn schedule_discarded_expression(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        if self.memory_mode {
            if let Some((place, bits, span)) = crate::constant::floating_representation_store(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            )
            .filter(|_| !self.unit.runtime_floating_environment())
            {
                tasks.push(MachineTask::BeginConstantRepresentationStore { place, bits, span });
                return Ok(());
            }
        }
        if let CHirExpr::Comma(left, right, _) = expression {
            self.schedule_discarded_expression(right, tasks)?;
            self.schedule_discarded_expression(left, tasks)?;
            return Ok(());
        }
        if let CHirExpr::Ternary(condition, then_expression, else_expression, _) = expression {
            tasks.push(MachineTask::DiscardSelectedExpression {
                then_expression,
                else_expression,
            });
            tasks.push(MachineTask::EvalExpression {
                expression: condition,
                allow_root_mutation: true,
            });
            return Ok(());
        }
        if let CHirExpr::Cast(ty, operand, _) = expression {
            if self
                .unit
                .types()
                .get(*ty)
                .is_some_and(|ty| matches!(ty.kind, TypeKind::Void))
            {
                return self.schedule_discarded_expression(operand, tasks);
            }
        }
        if let CHirExpr::BuiltinVarArgs { op, .. } = expression {
            if *op != crate::VarArgsBuiltinOp::Arg {
                self.schedule_varargs(expression, tasks)?;
                return Ok(());
            }
        }
        if let CHirExpr::Call(callee, arguments, call_span) = expression {
            if self.call_designator_returns_void(callee) {
                tasks.push(MachineTask::EvalCallArguments {
                    callee,
                    arguments,
                    index: 0,
                    values: Vec::new(),
                    span: *call_span,
                    expects_value: false,
                });
                return Ok(());
            }
        }
        if let CHirExpr::StatementExpr {
            body, value, span, ..
        } = expression
        {
            if crate::expression_is_void(expression, self.unit.symbols(), self.unit.types()) {
                self.schedule_statement_expression(body, *value, None, *span, tasks)?;
                return Ok(());
            }
        }
        tasks.push(MachineTask::DiscardExpression(expression.span()));
        tasks.push(MachineTask::EvalExpression {
            expression,
            allow_root_mutation: true,
        });
        Ok(())
    }

    pub(super) fn schedule_statement_expression(
        &mut self,
        body: &'arena CHirStmt<'arena>,
        value: Option<&'arena CHirExpr<'arena>>,
        round: Option<ValueType>,
        span: SourceSpan,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let CHirStmt::Compound(items, _, policy) = body else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "statement expression body is not compound",
                span,
            });
        };
        self.enter_fenv_scope(*policy, tasks);
        if let Some(target) = round {
            tasks.push(MachineTask::CastValue { target, span });
        }
        let result_matches = match (value, items.last()) {
            (None, _) => true,
            (Some(value), Some(CHirBlockItem::Statement(statement))) => {
                statement_expression_value(statement)
                    .is_some_and(|expression| std::ptr::eq(value, expression))
            }
            (Some(_), _) => false,
        };
        if !result_matches {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "statement expression result is not its final statement",
                span,
            });
        }
        let saved = self.activate_block_items(items)?;
        tasks.push(MachineTask::RestoreSlots(saved));
        if let Some(exit) = self.enter_memory_scope(body.span())? {
            let parent = exit.parent;
            tasks.push(MachineTask::ExitMemoryScope(exit));
            if value.is_some_and(|value| {
                !crate::expression_is_void(value, self.unit.symbols(), self.unit.types())
            }) {
                tasks.push(MachineTask::PreserveStatementExpressionValue { parent, span });
            }
        }
        tasks.push(MachineTask::StatementExpressionContinue {
            items,
            index: 0,
            value,
            span,
        });
        Ok(())
    }
}
