//! Lowering storage.

use super::*;

pub(super) fn collect_symbols_item(item: &CHirBlockItem<'_>, symbols: &mut BTreeSet<SymbolId>) {
    match item {
        CHirBlockItem::Declaration(declaration) => {
            if let CHirDecl::Object(object) = declaration {
                if let Some(cleanup) = object.cleanup {
                    collect_symbols_expr(cleanup, symbols);
                }
                if let Some(initializer) = &object.initializer {
                    for action in initializer.plan.actions() {
                        if let InitAction::StoreScalar { value, .. }
                        | InitAction::CopyAggregate { value, .. } = action
                        {
                            collect_symbols_expr(value, symbols);
                        }
                    }
                }
            }
        }
        CHirBlockItem::Statement(statement) => collect_symbols_stmt(statement, symbols),
    }
}

pub(super) fn collect_symbols_stmt(statement: &CHirStmt<'_>, symbols: &mut BTreeSet<SymbolId>) {
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_symbols_expr(operand.expression, symbols);
            }
        }
        CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
            collect_symbols_expr(expression, symbols)
        }
        CHirStmt::ComputedGoto(expression, _) => collect_symbols_expr(expression, symbols),
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            collect_symbols_expr(condition, symbols);
            collect_symbols_stmt(then_branch, symbols);
            if let Some(branch) = else_branch {
                collect_symbols_stmt(branch, symbols);
            }
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            collect_symbols_expr(expression, symbols);
            collect_symbols_stmt(body, symbols);
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            collect_symbols_expr(first, symbols);
            collect_symbols_expr(last, symbols);
            collect_symbols_stmt(body, symbols);
        }
        CHirStmt::DoWhile(body, expression, _) => {
            collect_symbols_stmt(body, symbols);
            collect_symbols_expr(expression, symbols);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => collect_symbols_expr(expression, symbols),
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        collect_symbols_item(&CHirBlockItem::Declaration(declaration), symbols);
                    }
                }
            }
            if let Some(condition) = condition {
                collect_symbols_expr(condition, symbols);
            }
            if let Some(step) = step {
                collect_symbols_expr(step, symbols);
            }
            collect_symbols_stmt(body, symbols);
        }
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                collect_symbols_item(item, symbols);
            }
        }
        CHirStmt::Label(_, statement, _) | CHirStmt::Default(statement, _) => {
            collect_symbols_stmt(statement, symbols)
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

pub(super) fn collect_symbols_expr(expression: &CHirExpr<'_>, symbols: &mut BTreeSet<SymbolId>) {
    match expression {
        CHirExpr::Complex(operation) => {
            for operand in &operation.operands {
                collect_symbols_expr(operand, symbols);
            }
        }
        CHirExpr::Floating(operation) => {
            for operand in &operation.operands {
                collect_symbols_expr(operand, symbols);
            }
        }
        CHirExpr::Vector(operation) => {
            for operand in &operation.operands {
                collect_symbols_expr(operand, symbols);
            }
        }
        CHirExpr::Atomic(access) => {
            for operand in access.operands() {
                collect_symbols_expr(operand, symbols);
            }
        }
        CHirExpr::BuiltinConstantP { .. } | CHirExpr::BuiltinObjectSize { .. } => {}
        CHirExpr::Ident { symbol, .. } => {
            symbols.insert(*symbol);
        }
        CHirExpr::Assign {
            place,
            target,
            value,
            ..
        } => {
            symbols.insert(place.symbol());
            collect_symbols_expr(target, symbols);
            collect_symbols_expr(value, symbols);
        }
        CHirExpr::Update { place, target, .. } => {
            symbols.insert(place.symbol());
            collect_symbols_expr(target, symbols);
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            if let Some(symbol) = place.root_symbol() {
                symbols.insert(symbol);
            }
            for source in place.source_expressions() {
                collect_symbols_expr(source, symbols);
            }
            collect_symbols_expr(value, symbols);
        }
        CHirExpr::MemoryRead { place, .. } => {
            if let Some(symbol) = place.root_symbol() {
                symbols.insert(symbol);
            }
            for source in place.source_expressions() {
                collect_symbols_expr(source, symbols);
            }
        }
        CHirExpr::MemoryUpdate { place, .. } => {
            if let Some(symbol) = place.root_symbol() {
                symbols.insert(symbol);
            }
            for source in place.source_expressions() {
                collect_symbols_expr(source, symbols);
            }
        }
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => collect_symbols_expr(operand, symbols),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            collect_symbols_expr(left, symbols);
            collect_symbols_expr(right, symbols);
        }
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            collect_symbols_expr(condition, symbols);
            collect_symbols_expr(then_value, symbols);
            collect_symbols_expr(else_value, symbols);
        }
        CHirExpr::Call(callee, arguments, _) => {
            collect_symbols_expr(callee, symbols);
            for argument in arguments {
                collect_symbols_expr(argument, symbols);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            collect_symbols_expr(lhs, symbols);
            collect_symbols_expr(rhs, symbols);
            collect_symbols_expr(target, symbols);
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            collect_symbols_expr(first, symbols);
            if let Some(second) = second {
                collect_symbols_expr(second, symbols);
            }
        }
        CHirExpr::BuiltinVarArgPack { .. } => {}
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    collect_symbols_expr(value, symbols);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => collect_symbols_stmt(body, symbols),
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

/// Conservative live-ins at indirect entries, closed over direct and indirect
/// backedges. A lexical suffix alone misses a target that jumps to an earlier
/// ordinary label. Values confined to other paths keep their SSA treatment.
pub(super) fn indirect_storage_symbols(
    body: &CHirStmt<'_>,
    names: &BTreeSet<String>,
) -> BTreeSet<SymbolId> {
    type Live = BTreeSet<SymbolId>;
    type Labels = BTreeMap<String, Live>;
    fn expression_edges(
        references: FunctionReferences,
        live: &mut Live,
        previous: &Labels,
        labels: &mut Labels,
        names: &BTreeSet<String>,
    ) {
        for name in references
            .goto_names
            .iter()
            .chain(names.iter().filter(|_| references.has_computed_goto))
        {
            live.extend(previous.get(name).into_iter().flatten());
        }
        // Expressions in declarations, conditions and returns can own labels,
        // even when the corresponding &&label occurs elsewhere in the body.
        for name in references.defined_labels {
            labels.entry(name).or_default().extend(live.iter().copied());
        }
    }
    fn visit(
        statement: &CHirStmt<'_>,
        after: &Live,
        previous: &Labels,
        labels: &mut Labels,
        names: &BTreeSet<String>,
    ) -> Live {
        let mut live = after.clone();
        match statement {
            CHirStmt::Compound(items, _, _) => {
                for item in items.iter().rev() {
                    match item {
                        CHirBlockItem::Statement(statement) => {
                            live = visit(statement, &live, previous, labels, names)
                        }
                        CHirBlockItem::Declaration(declaration) => {
                            collect_symbols_item(
                                &CHirBlockItem::Declaration(declaration),
                                &mut live,
                            );
                            let mut references = FunctionReferences::default();
                            collect_references_decl(declaration, &mut references);
                            expression_edges(references, &mut live, previous, labels, names);
                        }
                    }
                }
            }
            CHirStmt::Label(label, child, _) => {
                live = visit(child, after, previous, labels, names);
                labels
                    .entry(label.spelling.clone())
                    .or_default()
                    .extend(live.iter().copied());
            }
            CHirStmt::Goto(label, _) => {
                live = previous.get(&label.spelling).cloned().unwrap_or_default()
            }
            CHirStmt::ComputedGoto(expression, _) => {
                live.clear();
                for name in names {
                    live.extend(previous.get(name).into_iter().flatten());
                }
                collect_symbols_expr(expression, &mut live);
            }
            CHirStmt::Return(_, _) => {
                live.clear();
                collect_symbols_stmt(statement, &mut live);
                let mut references = FunctionReferences::default();
                collect_references_stmt(statement, &mut references);
                expression_edges(references, &mut live, previous, labels, names);
            }
            CHirStmt::If(condition, yes, no, _) => {
                live = visit(yes, after, previous, labels, names);
                live.extend(no.map_or_else(
                    || after.clone(),
                    |no| visit(no, after, previous, labels, names),
                ));
                collect_symbols_expr(condition, &mut live);
                let mut references = FunctionReferences::default();
                collect_references_expr(condition, &mut references);
                expression_edges(references, &mut live, previous, labels, names);
            }
            CHirStmt::For(_, _, _, child, _)
            | CHirStmt::While(_, child, _)
            | CHirStmt::DoWhile(child, _, _)
            | CHirStmt::Switch(_, child, _)
            | CHirStmt::Case(_, child, _)
            | CHirStmt::CaseRange(_, _, child, _)
            | CHirStmt::Default(child, _) => {
                // Loop/case control can revisit any nested use. This is an
                // over-approximation, not a whole-function memoryization.
                collect_symbols_stmt(statement, &mut live);
                let nested = visit(child, &live, previous, labels, names);
                live.extend(nested);
                let mut references = FunctionReferences::default();
                collect_references_stmt(statement, &mut references);
                expression_edges(references, &mut live, previous, labels, names);
            }
            _ => {
                collect_symbols_stmt(statement, &mut live);
                let mut references = FunctionReferences::default();
                collect_references_stmt(statement, &mut references);
                // A statement-expression can contain labels and transfers;
                // conservatively retain its entire expression continuation.
                expression_edges(references, &mut live, previous, labels, names);
            }
        }
        live
    }
    let mut labels = Labels::new();
    loop {
        let previous = labels.clone();
        visit(body, &Live::new(), &previous, &mut labels, names);
        if labels == previous {
            break;
        }
    }
    names
        .iter()
        .flat_map(|name| labels.get(name).into_iter().flatten().copied())
        .collect()
}
