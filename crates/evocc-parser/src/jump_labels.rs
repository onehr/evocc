//! Parser jump labels.

use super::*;

/// Return symbolic references in a GNU asm template.
///
/// Besides `%[name]`, GCC permits a one-letter modifier such as `%l[name]`
/// for goto labels. `%%` is a quoted percent and cannot start a reference.
pub(super) fn asm_template_symbolic_names(template: &str) -> Vec<&str> {
    let bytes = template.as_bytes();
    let mut names = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index] != b'%' || index + 1 >= bytes.len() {
            index += 1;
            continue;
        }
        if bytes[index + 1] == b'%' {
            index += 2;
            continue;
        }
        let mut open = index + 1;
        if bytes[open].is_ascii_alphabetic()
            && bytes.get(open + 1).is_some_and(|byte| *byte == b'[')
        {
            open += 1;
        }
        if bytes[open] != b'[' {
            index += 1;
            continue;
        }
        let name_start = open + 1;
        let Some(relative_end) = bytes[name_start..].iter().position(|byte| *byte == b']') else {
            break;
        };
        let name_end = name_start + relative_end;
        if let Some(name) = template.get(name_start..name_end) {
            names.push(name);
        }
        index = name_end + 1;
    }
    names
}

pub(super) fn collect_asm_goto_labels(
    statement: &CHirStmt<'_>,
    defined: &mut HashSet<String>,
    referenced: &mut Vec<SpannedName>,
) {
    match statement {
        CHirStmt::Asm(assembly) => referenced.extend(assembly.labels.iter().cloned()),
        CHirStmt::If(_, then_statement, else_statement, _) => {
            collect_asm_goto_labels(then_statement, defined, referenced);
            if let Some(else_statement) = else_statement {
                collect_asm_goto_labels(else_statement, defined, referenced);
            }
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::Default(body, _) => collect_asm_goto_labels(body, defined, referenced),
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    collect_asm_goto_labels(statement, defined, referenced);
                }
            }
        }
        CHirStmt::Label(label, body, _) => {
            defined.insert(label.spelling.clone());
            collect_asm_goto_labels(body, defined, referenced);
        }
        CHirStmt::Expr(_, _)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::ComputedGoto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

pub(super) fn collect_jump_regions_decl(
    declaration: &CHirDecl<'_>,
    regions: &mut Vec<u32>,
    next_region: &mut JumpRegionContext<'_>,
    labels: &mut HashMap<String, Vec<u32>>,
    gotos: &mut Vec<JumpRegionReference>,
) {
    match declaration {
        CHirDecl::Object(object) => {
            if let Some(initializer) = &object.initializer {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. } = action {
                        collect_jump_regions_expr(value, regions, next_region, labels, gotos);
                    }
                }
            }
        }
        CHirDecl::Asm(_) | CHirDecl::Tag(_) | CHirDecl::Typedef(_) | CHirDecl::Function(_) => {}
    }
    let ty = match declaration {
        CHirDecl::Object(object) => Some(object.ty),
        CHirDecl::Typedef(declaration) => Some(declaration.ty),
        _ => None,
    };
    if ty.is_some_and(next_region.is_variable) {
        regions.push(next_region.fresh(true));
    }
}

pub(super) fn collect_jump_regions_stmt(
    statement: &CHirStmt<'_>,
    regions: &mut Vec<u32>,
    next_region: &mut JumpRegionContext<'_>,
    labels: &mut HashMap<String, Vec<u32>>,
    gotos: &mut Vec<JumpRegionReference>,
) {
    match statement {
        CHirStmt::Asm(assembly) => {
            gotos.extend(assembly.labels.iter().map(|label| JumpRegionReference {
                name: label.spelling.clone(),
                span: label.span,
                regions: regions.clone(),
                checks_region_entry: true,
            }));
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_jump_regions_expr(operand.expression, regions, next_region, labels, gotos);
            }
        }
        CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
            collect_jump_regions_expr(expression, regions, next_region, labels, gotos);
        }
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            collect_jump_regions_expr(condition, regions, next_region, labels, gotos);
            collect_jump_regions_stmt(then_statement, regions, next_region, labels, gotos);
            if let Some(else_statement) = else_statement {
                collect_jump_regions_stmt(else_statement, regions, next_region, labels, gotos);
            }
        }
        CHirStmt::Switch(expression, body, _) => {
            collect_jump_regions_expr(expression, regions, next_region, labels, gotos);
            next_region.switches.push(regions.clone());
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
            next_region.switches.pop();
        }
        CHirStmt::While(expression, body, _) => {
            collect_jump_regions_expr(expression, regions, next_region, labels, gotos);
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::Case(expression, body, span) => {
            next_region.check_switch_entry(regions, *span);
            collect_jump_regions_expr(expression, regions, next_region, labels, gotos);
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::CaseRange(first, last, body, span) => {
            next_region.check_switch_entry(regions, *span);
            collect_jump_regions_expr(first, regions, next_region, labels, gotos);
            collect_jump_regions_expr(last, regions, next_region, labels, gotos);
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::DoWhile(body, condition, _) => {
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
            collect_jump_regions_expr(condition, regions, next_region, labels, gotos);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            let depth = regions.len();
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => {
                    collect_jump_regions_expr(expression, regions, next_region, labels, gotos);
                }
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        collect_jump_regions_decl(declaration, regions, next_region, labels, gotos);
                    }
                }
            }
            if let Some(condition) = condition {
                collect_jump_regions_expr(condition, regions, next_region, labels, gotos);
            }
            if let Some(step) = step {
                collect_jump_regions_expr(step, regions, next_region, labels, gotos);
            }
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
            regions.truncate(depth);
        }
        CHirStmt::Compound(items, _, _) => {
            let depth = regions.len();
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        collect_jump_regions_decl(declaration, regions, next_region, labels, gotos)
                    }
                    CHirBlockItem::Statement(statement) => {
                        collect_jump_regions_stmt(statement, regions, next_region, labels, gotos)
                    }
                }
            }
            regions.truncate(depth);
        }
        CHirStmt::Label(label, body, _) => {
            labels.insert(label.spelling.clone(), regions.clone());
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::Default(body, span) => {
            next_region.check_switch_entry(regions, *span);
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::Goto(label, span) => gotos.push(JumpRegionReference {
            name: label.spelling.clone(),
            span: *span,
            regions: regions.clone(),
            checks_region_entry: true,
        }),
        CHirStmt::ComputedGoto(target, span) => {
            next_region.indirect.push((*span, regions.clone()));
            collect_jump_regions_expr(target, regions, next_region, labels, gotos);
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

pub(super) fn collect_jump_regions_expr(
    expression: &CHirExpr<'_>,
    regions: &mut Vec<u32>,
    next_region: &mut JumpRegionContext<'_>,
    labels: &mut HashMap<String, Vec<u32>>,
    gotos: &mut Vec<JumpRegionReference>,
) {
    match expression {
        CHirExpr::Complex(operation) => {
            for operand in &operation.operands {
                collect_jump_regions_expr(operand, regions, next_region, labels, gotos);
            }
        }
        CHirExpr::Floating(operation) => {
            for operand in &operation.operands {
                collect_jump_regions_expr(operand, regions, next_region, labels, gotos);
            }
        }
        CHirExpr::Vector(operation) => {
            for operand in &operation.operands {
                collect_jump_regions_expr(operand, regions, next_region, labels, gotos);
            }
        }
        CHirExpr::Atomic(access) => {
            for operand in access.operands() {
                collect_jump_regions_expr(operand, regions, next_region, labels, gotos);
            }
        }
        CHirExpr::BuiltinConstantP { operand, .. }
        | CHirExpr::BuiltinObjectSize { operand, .. } => {
            collect_jump_regions_expr(operand, regions, next_region, labels, gotos);
        }
        CHirExpr::LabelAddress { label, .. } => gotos.push(JumpRegionReference {
            name: label.spelling.clone(),
            span: label.span,
            regions: regions.clone(),
            checks_region_entry: false,
        }),
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::SizeOfExpr(operand, _, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => {
            collect_jump_regions_expr(operand, regions, next_region, labels, gotos);
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            collect_jump_regions_expr(left, regions, next_region, labels, gotos);
            collect_jump_regions_expr(right, regions, next_region, labels, gotos);
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            collect_jump_regions_expr(condition, regions, next_region, labels, gotos);
            collect_jump_regions_expr(when_true, regions, next_region, labels, gotos);
            collect_jump_regions_expr(when_false, regions, next_region, labels, gotos);
        }
        CHirExpr::Call(callee, arguments, _) => {
            collect_jump_regions_expr(callee, regions, next_region, labels, gotos);
            for argument in arguments {
                collect_jump_regions_expr(argument, regions, next_region, labels, gotos);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            collect_jump_regions_expr(lhs, regions, next_region, labels, gotos);
            collect_jump_regions_expr(rhs, regions, next_region, labels, gotos);
            collect_jump_regions_expr(target, regions, next_region, labels, gotos);
        }
        CHirExpr::BuiltinVarArgPack { .. } => {}
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            collect_jump_regions_expr(first, regions, next_region, labels, gotos);
            if let Some(second) = second {
                collect_jump_regions_expr(second, regions, next_region, labels, gotos);
            }
        }
        CHirExpr::Assign { target, value, .. } => {
            collect_jump_regions_expr(target, regions, next_region, labels, gotos);
            collect_jump_regions_expr(value, regions, next_region, labels, gotos);
        }
        CHirExpr::Update { target, .. } => {
            collect_jump_regions_expr(target, regions, next_region, labels, gotos);
        }
        CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
            for source in place.source_expressions() {
                collect_jump_regions_expr(source, regions, next_region, labels, gotos);
            }
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            for source in place.source_expressions() {
                collect_jump_regions_expr(source, regions, next_region, labels, gotos);
            }
            collect_jump_regions_expr(value, regions, next_region, labels, gotos);
        }
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. } = action {
                    collect_jump_regions_expr(value, regions, next_region, labels, gotos);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => {
            let region = next_region.fresh(false);
            regions.push(region);
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
            regions.pop();
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::Ident { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => {}
    }
}
