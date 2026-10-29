//! Lowering references.

use super::*;

pub(super) fn collect_references_stmt(statement: &CHirStmt<'_>, calls: &mut FunctionReferences) {
    let case_label = matches!(
        statement,
        CHirStmt::Case(..) | CHirStmt::CaseRange(..) | CHirStmt::Default(..)
    );
    calls.has_case_label |= case_label;
    calls.has_unbound_case_label |= case_label && calls.switch_depth == 0;
    let parent = calls.scope;
    if matches!(
        statement,
        CHirStmt::Compound(..) | CHirStmt::For(CHirForInit::Declaration(_), ..)
    ) {
        calls.scope = statement as *const CHirStmt<'_> as usize;
    }
    match statement {
        CHirStmt::Asm(assembly) => {
            calls.any = true;
            calls
                .goto_names
                .extend(assembly.labels.iter().map(|label| label.spelling.clone()));
            for label in &assembly.labels {
                *calls.goto_counts.entry(label.spelling.clone()).or_default() += 1;
            }
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_symbols_expr(operand.expression, &mut calls.unstable_objects);
                collect_references_expr(operand.expression, calls);
            }
        }
        CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
            collect_references_expr(expression, calls)
        }
        CHirStmt::ComputedGoto(expression, _) => {
            calls.has_computed_goto = true;
            collect_references_expr(expression, calls);
        }
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            collect_references_expr(condition, calls);
            collect_references_stmt(then_branch, calls);
            if let Some(branch) = else_branch {
                collect_references_stmt(branch, calls);
            }
        }
        CHirStmt::Switch(expression, body, _) => {
            collect_references_expr(expression, calls);
            calls.switch_depth += 1;
            collect_references_stmt(body, calls);
            calls.switch_depth -= 1;
        }
        CHirStmt::While(expression, body, _) | CHirStmt::Case(expression, body, _) => {
            collect_references_expr(expression, calls);
            collect_references_stmt(body, calls);
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            collect_references_expr(first, calls);
            collect_references_expr(last, calls);
            collect_references_stmt(body, calls);
        }
        CHirStmt::DoWhile(body, expression, _) => {
            collect_references_stmt(body, calls);
            collect_references_expr(expression, calls);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => collect_references_expr(expression, calls),
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        collect_references_decl(declaration, calls);
                    }
                }
            }
            if let Some(condition) = condition {
                collect_references_expr(condition, calls);
            }
            if let Some(step) = step {
                collect_references_expr(step, calls);
            }
            collect_references_stmt(body, calls);
        }
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        collect_references_decl(declaration, calls)
                    }
                    CHirBlockItem::Statement(statement) => {
                        collect_references_stmt(statement, calls)
                    }
                }
            }
        }
        CHirStmt::Label(label, statement, _) => {
            calls.defined_labels.insert(label.spelling.clone());
            collect_references_stmt(statement, calls)
        }
        CHirStmt::Default(statement, _) => collect_references_stmt(statement, calls),
        CHirStmt::Goto(label, _) => {
            calls.goto_names.insert(label.spelling.clone());
            *calls.goto_counts.entry(label.spelling.clone()).or_default() += 1;
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
    calls.scope = parent;
}

pub(super) fn collect_references_decl(declaration: &CHirDecl<'_>, calls: &mut FunctionReferences) {
    if let CHirDecl::Object(object) = declaration {
        if let Some(cleanup) = object.cleanup {
            collect_references_expr(cleanup, calls);
        }
        if !matches!(object.storage, StorageClass::Static | StorageClass::Extern) {
            calls.objects.insert(object.symbol, object.ty);
        }
        if object.storage == StorageClass::Register {
            if let Some(register) = &object.asm_label {
                calls
                    .local_registers
                    .insert(object.symbol, register.clone());
            }
        }
        if let Some(initializer) = &object.initializer {
            for action in initializer.plan.actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    collect_references_expr(value, calls);
                    collect_symbols_expr(value, &mut calls.unstable_objects);
                }
            }
        }
    }
}

pub(super) fn collect_references_expr(expression: &CHirExpr<'_>, calls: &mut FunctionReferences) {
    if let CHirExpr::Unary(UnaryOp::AddrOf, operand, _) = expression {
        collect_symbols_expr(operand, &mut calls.unstable_objects);
        let mut root = *operand;
        while let CHirExpr::Member(base, _, _) = root {
            root = base;
        }
        if let CHirExpr::Ident { symbol, .. } = root {
            calls.frame_addresses.insert(*symbol);
        }
        calls.addressed_compound |= matches!(root, CHirExpr::CompoundLit(..));
    }
    match expression {
        CHirExpr::Floating(operation) => {
            for operand in &operation.operands {
                collect_references_expr(operand, calls);
            }
        }
        CHirExpr::Vector(operation) => {
            for operand in &operation.operands {
                collect_references_expr(operand, calls);
            }
        }
        CHirExpr::Complex(operation) => {
            for operand in &operation.operands {
                collect_references_expr(operand, calls);
            }
        }
        CHirExpr::Atomic(access) => {
            for operand in access.operands() {
                collect_symbols_expr(operand, &mut calls.unstable_objects);
                collect_references_expr(operand, calls);
            }
        }
        CHirExpr::BuiltinConstantP { .. } | CHirExpr::BuiltinObjectSize { .. } => {}
        CHirExpr::LabelAddress { label, .. } => {
            calls.labels.insert(
                expression as *const CHirExpr<'_> as usize,
                label.spelling.clone(),
            );
        }
        CHirExpr::Call(callee, arguments, _) => {
            calls.any = true;
            if let CHirExpr::Ident { symbol, facts, .. } = callee {
                if facts.category() == ExprCategory::FunctionDesignator {
                    calls.direct.insert(*symbol);
                }
            }
            collect_references_expr(callee, calls);
            for argument in arguments {
                collect_symbols_expr(argument, &mut calls.unstable_objects);
                collect_references_expr(argument, calls);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            collect_references_expr(lhs, calls);
            collect_references_expr(rhs, calls);
            collect_references_expr(target, calls);
        }
        CHirExpr::BuiltinVarArgPack { .. } => {}
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            if let CHirExpr::Ident { symbol, .. } = first {
                calls.frame_addresses.insert(*symbol);
            }
            collect_references_expr(first, calls);
            if let Some(second) = second {
                collect_references_expr(second, calls);
            }
        }
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => collect_references_expr(operand, calls),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            collect_references_expr(left, calls);
            collect_references_expr(right, calls);
        }
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            collect_references_expr(condition, calls);
            collect_references_expr(then_value, calls);
            collect_references_expr(else_value, calls);
        }
        CHirExpr::Assign { target, value, .. } => {
            collect_symbols_expr(target, &mut calls.unstable_objects);
            collect_symbols_expr(value, &mut calls.unstable_objects);
            collect_references_expr(target, calls);
            collect_references_expr(value, calls);
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            calls.unstable_objects.extend(place.root_symbol());
            collect_symbols_expr(value, &mut calls.unstable_objects);
            for source in place.source_expressions() {
                collect_references_expr(source, calls);
            }
            collect_references_expr(value, calls);
        }
        CHirExpr::MemoryRead { place, .. } => {
            for source in place.source_expressions() {
                collect_references_expr(source, calls);
            }
        }
        CHirExpr::Update { target, .. } => {
            collect_symbols_expr(target, &mut calls.unstable_objects);
            collect_references_expr(target, calls);
        }
        CHirExpr::MemoryUpdate { place, .. } => {
            calls.unstable_objects.extend(place.root_symbol());
            for source in place.source_expressions() {
                collect_references_expr(source, calls);
            }
        }
        CHirExpr::CompoundLit(ty, initializer, span) => {
            calls
                .compound_scopes
                .entry(calls.scope)
                .or_default()
                .insert(expression as *const CHirExpr<'_> as usize, (*ty, *span));
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    collect_references_expr(value, calls);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => collect_references_stmt(body, calls),
        CHirExpr::Ident { symbol, facts, .. } => {
            if facts.category() == ExprCategory::FunctionDesignator {
                calls.direct.insert(*symbol);
            }
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::SizeOf(..)
        | CHirExpr::SizeOfExpr(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => {}
    }
}
