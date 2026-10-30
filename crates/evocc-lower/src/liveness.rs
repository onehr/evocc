//! Lowering liveness.

use super::*;

pub(super) fn live_before_stmt(
    statement: &CHirStmt<'_>,
    mut live_after: BTreeSet<SymbolId>,
) -> BTreeSet<SymbolId> {
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly
                .inputs
                .iter()
                .rev()
                .chain(assembly.outputs.iter().rev())
            {
                live_before_expr(operand.expression, &mut live_after);
            }
        }
        CHirStmt::Expr(expression, _) => {
            live_before_expr(expression, &mut live_after);
        }
        CHirStmt::Return(expression, _) => {
            live_after.clear();
            if let Some(expression) = expression {
                live_before_expr(expression, &mut live_after);
            }
        }
        CHirStmt::ComputedGoto(expression, _) => {
            live_after.clear();
            live_before_expr(expression, &mut live_after);
        }
        CHirStmt::Compound(items, _, _) => {
            for item in items.iter().rev() {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        if let CHirDecl::Object(object) = declaration {
                            live_after.remove(&object.symbol);
                            if let Some(initializer) = &object.initializer {
                                for action in initializer.plan().actions() {
                                    if let InitAction::StoreScalar { value, .. }
                                    | InitAction::CopyAggregate { value, .. } = action
                                    {
                                        live_before_expr(value, &mut live_after);
                                    }
                                }
                            }
                        }
                    }
                    CHirBlockItem::Statement(statement) => {
                        live_after = live_before_stmt(statement, live_after);
                    }
                }
            }
        }
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            let then_live = live_before_stmt(then_branch, live_after.clone());
            let else_live = if let Some(branch) = else_branch {
                live_before_stmt(branch, live_after.clone())
            } else {
                live_after.clone()
            };
            live_after = then_live.union(&else_live).copied().collect();
            live_before_expr(condition, &mut live_after);
        }
        CHirStmt::While(condition, body, _) => {
            let exit_live = live_after.clone();
            loop {
                let mut before = live_before_stmt(body, live_after.clone());
                live_before_expr(condition, &mut before);
                before.extend(exit_live.iter().copied());
                if before == live_after {
                    break;
                }
                live_after = before;
            }
        }
        CHirStmt::DoWhile(body, condition, _) => {
            let exit_live = live_after.clone();
            loop {
                let mut condition_live = live_after.clone();
                live_before_expr(condition, &mut condition_live);
                let mut before = live_before_stmt(body, condition_live);
                before.extend(exit_live.iter().copied());
                if before == live_after {
                    break;
                }
                live_after = before;
            }
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            let exit_live = live_after.clone();
            loop {
                let mut iteration_live = live_after.clone();
                if let Some(step) = step {
                    live_before_expr(step, &mut iteration_live);
                }
                iteration_live = live_before_stmt(body, iteration_live);
                if let Some(condition) = condition {
                    live_before_expr(condition, &mut iteration_live);
                }
                iteration_live.extend(exit_live.iter().copied());
                if iteration_live == live_after {
                    break;
                }
                live_after = iteration_live;
            }
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => {
                    live_before_expr(expression, &mut live_after);
                }
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations.iter().rev() {
                        if let CHirDecl::Object(object) = declaration {
                            live_after.remove(&object.symbol);
                            if let Some(initializer) = &object.initializer {
                                for action in initializer.plan().actions() {
                                    if let InitAction::StoreScalar { value, .. }
                                    | InitAction::CopyAggregate { value, .. } = action
                                    {
                                        live_before_expr(value, &mut live_after);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        CHirStmt::Switch(expression, body, _) | CHirStmt::Case(expression, body, _) => {
            live_after = live_before_stmt(body, live_after);
            live_before_expr(expression, &mut live_after);
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            live_after = live_before_stmt(body, live_after);
            live_before_expr(last, &mut live_after);
            live_before_expr(first, &mut live_after);
        }
        CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => {
            live_after = live_before_stmt(body, live_after);
        }
        CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
    live_after
}

pub(super) fn live_before_expr(expression: &CHirExpr<'_>, live: &mut BTreeSet<SymbolId>) {
    match expression {
        CHirExpr::Complex(operation) => {
            for operand in operation.operands.iter().rev() {
                live_before_expr(operand, live);
            }
        }
        CHirExpr::Floating(operation) => {
            for operand in operation.operands.iter().rev() {
                live_before_expr(operand, live);
            }
        }
        CHirExpr::Vector(operation) => {
            for operand in operation.operands.iter().rev() {
                live_before_expr(operand, live);
            }
        }
        CHirExpr::Atomic(access) => {
            for operand in access.operands().rev() {
                live_before_expr(operand, live);
            }
        }
        CHirExpr::BuiltinConstantP { .. } | CHirExpr::BuiltinObjectSize { .. } => {}
        CHirExpr::Ident { symbol, .. } => {
            live.insert(*symbol);
        }
        CHirExpr::Assign {
            op, place, value, ..
        } => {
            if *op == AssignmentOp::Simple {
                live.remove(&place.symbol());
            } else {
                live.insert(place.symbol());
            }
            live_before_expr(value, live);
        }
        CHirExpr::Update { place, .. } => {
            live.insert(place.symbol());
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            live_before_expr(value, live);
            for source in place.source_expressions().rev() {
                live_before_expr(source, live);
            }
            if let Some(symbol) = place.root_symbol() {
                live.insert(symbol);
            }
        }
        CHirExpr::MemoryRead { place, .. } => {
            for source in place.source_expressions().rev() {
                live_before_expr(source, live);
            }
            if let Some(symbol) = place.root_symbol() {
                live.insert(symbol);
            }
        }
        CHirExpr::MemoryUpdate { place, .. } => {
            for source in place.source_expressions().rev() {
                live_before_expr(source, live);
            }
            if let Some(symbol) = place.root_symbol() {
                live.insert(symbol);
            }
        }
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => live_before_expr(operand, live),
        CHirExpr::Binary(BinaryOp::And | BinaryOp::Or, left, right, _) => {
            let mut right_live = live.clone();
            live_before_expr(right, &mut right_live);
            live.extend(right_live);
            live_before_expr(left, live);
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            live_before_expr(right, live);
            live_before_expr(left, live);
        }
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            let mut then_live = live.clone();
            live_before_expr(then_value, &mut then_live);
            let mut else_live = live.clone();
            live_before_expr(else_value, &mut else_live);
            *live = then_live.union(&else_live).copied().collect();
            live_before_expr(condition, live);
        }
        CHirExpr::Call(callee, arguments, _) => {
            for argument in arguments.iter().rev() {
                live_before_expr(argument, live);
            }
            live_before_expr(callee, live);
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            live_before_expr(target, live);
            live_before_expr(rhs, live);
            live_before_expr(lhs, live);
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            if let Some(second) = second {
                live_before_expr(second, live);
            }
            live_before_expr(first, live);
        }
        CHirExpr::BuiltinVarArgPack { .. } => {}
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions().iter().rev() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    live_before_expr(value, live);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => {
            *live = live_before_stmt(body, std::mem::take(live));
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::SizeOfExpr(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => {}
    }
}
