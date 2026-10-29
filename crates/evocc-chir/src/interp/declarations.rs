//! C-HIR interpreter declarations.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn predeclare_stmt(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
    ) -> Result<(), InterpreterError> {
        match statement {
            CHirStmt::Asm(assembly) => {
                for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                    self.predeclare_expr(operand.expression)?;
                }
            }
            CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
                self.predeclare_expr(expression)?;
            }
            CHirStmt::ComputedGoto(expression, _) => {
                self.predeclare_expr(expression)?;
            }
            CHirStmt::If(condition, then_statement, else_statement, _) => {
                self.predeclare_expr(condition)?;
                self.predeclare_stmt(then_statement)?;
                if let Some(else_statement) = else_statement {
                    self.predeclare_stmt(else_statement)?;
                }
            }
            CHirStmt::Switch(expression, body, _)
            | CHirStmt::While(expression, body, _)
            | CHirStmt::Case(expression, body, _) => {
                self.predeclare_expr(expression)?;
                self.predeclare_stmt(body)?;
            }
            CHirStmt::CaseRange(first, last, body, _) => {
                self.predeclare_expr(first)?;
                self.predeclare_expr(last)?;
                self.predeclare_stmt(body)?;
            }
            CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => {
                self.predeclare_stmt(body)?;
            }
            CHirStmt::DoWhile(body, condition, _) => {
                self.predeclare_stmt(body)?;
                self.predeclare_expr(condition)?;
            }
            CHirStmt::For(initializer, condition, step, body, _) => {
                match initializer {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(expression) => self.predeclare_expr(expression)?,
                    CHirForInit::Declaration(declarations) => {
                        for declaration in declarations {
                            self.predeclare_decl(declaration)?;
                        }
                    }
                }
                if let Some(condition) = condition {
                    self.predeclare_expr(condition)?;
                }
                if let Some(step) = step {
                    self.predeclare_expr(step)?;
                }
                self.predeclare_stmt(body)?;
            }
            CHirStmt::Compound(items, _, _) => {
                for item in items {
                    match item {
                        CHirBlockItem::Declaration(declaration) => {
                            self.predeclare_decl(declaration)?;
                        }
                        CHirBlockItem::Statement(statement) => {
                            self.predeclare_stmt(statement)?;
                        }
                    }
                }
            }
            CHirStmt::Return(None, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::AttributedNull(_, _)
            | CHirStmt::Null(_) => {}
        }
        Ok(())
    }

    pub(super) fn predeclare_expr(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<(), InterpreterError> {
        // Declaration discovery is structural, not expression execution. Keep
        // source order with an explicit stack rather than consuming the host
        // stack for every operator in a preprocessed arithmetic chain.
        let mut pending = vec![expression];
        while let Some(expression) = pending.pop() {
            match expression {
                CHirExpr::Complex(access) => {
                    pending.extend(access.operands.iter().rev().copied());
                }
                CHirExpr::Floating(access) => {
                    pending.extend(access.operands.iter().rev().copied());
                }
                CHirExpr::Vector(access) => {
                    pending.extend(access.operands.iter().rev().copied());
                }
                CHirExpr::Atomic(access) => {
                    pending.extend(access.operands().rev());
                }
                CHirExpr::BuiltinConstantP { .. } | CHirExpr::BuiltinObjectSize { .. } => {}
                CHirExpr::Unary(_, operand, _)
                | CHirExpr::EvaluateOnce(operand, _)
                | CHirExpr::Cast(_, operand, _)
                | CHirExpr::Member(operand, _, _)
                | CHirExpr::PtrMember(operand, _, _) => pending.push(operand),
                CHirExpr::Binary(_, left, right, _)
                | CHirExpr::Index(left, right, _)
                | CHirExpr::Comma(left, right, _)
                | CHirExpr::Elvis(left, right, _)
                | CHirExpr::BuiltinExpect(left, right, _) => {
                    pending.push(right);
                    pending.push(left);
                }
                CHirExpr::Ternary(condition, when_true, when_false, _) => {
                    pending.push(when_false);
                    pending.push(when_true);
                    pending.push(condition);
                }
                CHirExpr::Call(callee, arguments, _) => {
                    pending.extend(arguments.iter().rev().copied());
                    pending.push(callee);
                }
                CHirExpr::CheckedArithmetic {
                    lhs, rhs, target, ..
                } => {
                    pending.push(target);
                    pending.push(rhs);
                    pending.push(lhs);
                }
                CHirExpr::BitBuiltin { operand, .. } => pending.push(operand),
                CHirExpr::BuiltinVarArgs { first, second, .. } => {
                    if let Some(second) = second {
                        pending.push(second);
                    }
                    pending.push(first);
                }
                CHirExpr::Assign { target, value, .. } => {
                    pending.push(value);
                    pending.push(target);
                }
                CHirExpr::Update { target, .. } => pending.push(target),
                CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
                    pending.extend(place.source_expressions().rev());
                }
                CHirExpr::MemoryAssign { place, value, .. } => {
                    pending.push(value);
                    pending.extend(place.source_expressions().rev());
                }
                CHirExpr::CompoundLit(_, initializer, _) => {
                    for action in initializer.plan().actions().iter().rev() {
                        if let InitAction::StoreScalar { value, .. } = action {
                            pending.push(value);
                        }
                    }
                }
                CHirExpr::StatementExpr { body, .. } => self.predeclare_stmt(body)?,
                CHirExpr::Lit(..)
                | CHirExpr::FloatLit(..)
                | CHirExpr::CharLit(..)
                | CHirExpr::StringLit(..)
                | CHirExpr::Ident { .. }
                | CHirExpr::LabelAddress { .. }
                | CHirExpr::SizeOf(..)
                | CHirExpr::SizeOfExpr(..)
                | CHirExpr::AlignOf(..)
                | CHirExpr::BuiltinTrap { .. }
                | CHirExpr::BuiltinUnreachable { .. }
                | CHirExpr::BuiltinReturnAddress { .. }
                | CHirExpr::BuiltinFrameAddress { .. }
                | CHirExpr::BuiltinVarArgPack { .. } => {}
            }
        }
        Ok(())
    }

    pub(super) fn predeclare_decl(
        &mut self,
        declaration: &'arena CHirDecl<'arena>,
    ) -> Result<(), InterpreterError> {
        match declaration {
            CHirDecl::Object(declaration) => {
                let ty = self.validate_object_declaration(declaration)?;
                self.validate_initializer_plan(declaration)?;
                if self
                    .slots
                    .insert(
                        declaration.symbol,
                        Slot {
                            ty,
                            value: None,
                            active: false,
                        },
                    )
                    .is_some()
                {
                    return Err(InterpreterError::MalformedObjectDeclaration {
                        symbol: declaration.symbol,
                        span: declaration.span,
                    });
                }
                if let Some(initializer) = &declaration.initializer {
                    for action in initializer.plan().actions() {
                        if let InitAction::StoreScalar { value, .. } = action {
                            self.predeclare_expr(value)?;
                        }
                    }
                }
                Ok(())
            }
            CHirDecl::Asm(_) | CHirDecl::Tag(_) | CHirDecl::Typedef(_) => Ok(()),
            CHirDecl::Function(function) if function.body.is_none() => Ok(()),
            CHirDecl::Function(declaration) => Err(InterpreterError::UnsupportedDeclaration {
                feature: "block function declaration",
                span: declaration.span,
            }),
        }
    }

    pub(super) fn validate_object_declaration(
        &self,
        declaration: &ObjectDecl<'arena>,
    ) -> Result<CIntegerType, InterpreterError> {
        if !matches!(
            declaration.storage,
            StorageClass::None | StorageClass::Auto | StorageClass::Register
        ) {
            return Err(InterpreterError::UnsupportedDeclaration {
                feature: "non-automatic scalar object",
                span: declaration.span,
            });
        }
        let symbol = self
            .unit
            .symbols()
            .symbol(declaration.symbol)
            .map_err(|_| InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            })?;
        if symbol.kind() != SymbolKind::Object
            || symbol.spelling() != declaration.name.spelling
            || symbol.ty() != declaration.ty
            || symbol.storage_class() != Some(declaration.storage)
        {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        }
        self.integer_type(declaration.ty, declaration.span)
    }

    pub(super) fn validate_initializer_plan(
        &self,
        declaration: &ObjectDecl<'arena>,
    ) -> Result<(), InterpreterError> {
        let Some(initializer) = &declaration.initializer else {
            return Ok(());
        };
        let plan = initializer.plan();
        if plan.target_type() != declaration.ty
            || plan.validate(self.unit.types()).is_err()
            || (plan.actions().is_empty() && !plan.zero_fills_unwritten())
        {
            return Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            });
        }
        // Each action must designate a real subobject of this declaration and
        // agree with that subobject's type. `SubobjectPath` resolution is the
        // same authority the initializer execution uses, so a plan that passes
        // here cannot fail to address its destination later.
        let valid = plan.actions().iter().all(|action| match action {
            InitAction::StoreScalar {
                path,
                destination_type,
                span,
                ..
            } => self
                .subobject_offset(declaration.ty, path, *span)
                .is_ok_and(|(_, ty, _)| ty == *destination_type),
            InitAction::CopyAggregate {
                path,
                destination_type,
                span,
                ..
            } => self
                .subobject_offset(declaration.ty, path, *span)
                .is_ok_and(|(_, ty, _)| ty == *destination_type),
            InitAction::ZeroScalar {
                path,
                destination_type,
            } => self
                .subobject_offset(declaration.ty, path, declaration.span)
                .is_ok_and(|(_, ty, _)| ty == *destination_type),
            InitAction::StoreString {
                path,
                destination_type,
                literal,
                span,
            } => {
                literal.decoded_units().is_some()
                    && self
                        .subobject_offset(declaration.ty, path, *span)
                        .is_ok_and(|(_, ty, _)| ty == *destination_type)
            }
        });
        if !valid {
            return Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: plan
                    .actions()
                    .first()
                    .and_then(InitAction::span)
                    .unwrap_or_else(|| initializer.span()),
            });
        }
        Ok(())
    }

    pub(super) fn activate_declaration(
        &mut self,
        declaration: &'arena CHirDecl<'arena>,
        saved: &mut Vec<(SymbolId, Slot)>,
    ) -> Result<(), InterpreterError> {
        if self.memory_mode {
            return Ok(());
        }
        let CHirDecl::Object(declaration) = declaration else {
            return Ok(());
        };
        let Some(slot) = self.slots.get_mut(&declaration.symbol) else {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        };
        if slot.active {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        }
        saved.push((declaration.symbol, *slot));
        slot.active = true;
        slot.value = None;
        Ok(())
    }

    pub(super) fn activate_block_items(
        &mut self,
        items: &[CHirBlockItem<'arena>],
    ) -> Result<Vec<(SymbolId, Slot)>, InterpreterError> {
        let mut saved = Vec::new();
        for item in items {
            if let CHirBlockItem::Declaration(declaration) = item {
                self.activate_declaration(declaration, &mut saved)?;
            }
        }
        Ok(saved)
    }

    pub(super) fn activate_for_declarations(
        &mut self,
        declarations: &[&'arena CHirDecl<'arena>],
    ) -> Result<Vec<(SymbolId, Slot)>, InterpreterError> {
        let mut saved = Vec::new();
        for declaration in declarations {
            self.activate_declaration(declaration, &mut saved)?;
        }
        Ok(saved)
    }

    pub(super) fn restore_slots(&mut self, saved: Vec<(SymbolId, Slot)>) {
        for (symbol, slot) in saved {
            self.slots.insert(symbol, slot);
        }
    }
}
