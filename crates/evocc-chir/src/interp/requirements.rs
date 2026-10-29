//! C-HIR interpreter requirements.

use super::*;

/// File-scope register bindings execute as assembly, not object accesses.
pub(super) fn is_global_register(unit: &CHirUnit<'_>, symbol: SymbolId) -> bool {
    unit.symbols().symbol(symbol).is_ok_and(|declaration| {
        declaration.storage_class() == Some(StorageClass::Register)
            && unit
                .symbols()
                .scope(declaration.scope())
                .is_ok_and(|scope| scope.kind() == ScopeKind::File)
    })
}

/// Does this symbol name an object that lives for the whole execution?
///
/// Shared by requirement discovery and static allocation.
pub(super) fn has_static_storage_duration(unit: &CHirUnit<'_>, symbol: SymbolId) -> bool {
    let Ok(declaration) = unit.symbols().symbol(symbol) else {
        return false;
    };
    if declaration.kind() != SymbolKind::Object {
        return false;
    }
    let file_scope = unit
        .symbols()
        .is_bound_in_scope(symbol, unit.symbols().file_scope())
        .unwrap_or(false);
    file_scope || declaration.storage_class().map(StorageClass::base) == Some(StorageClass::Static)
}

pub(super) fn reachable_memory_requirements(
    unit: &CHirUnit<'_>,
    entry: SymbolId,
    linkage: &crate::linkage::SymbolLinkage,
) -> MemoryRequirements {
    let mut requires_memory = false;
    let mut visited_functions: BTreeSet<SymbolId> = BTreeSet::new();
    let mut static_objects: BTreeSet<SymbolId> = BTreeSet::new();
    let mut pending_functions: Vec<SymbolId> = vec![linkage.target(entry)];
    let mut pending_objects: Vec<SymbolId> = Vec::new();

    while !pending_functions.is_empty() || !pending_objects.is_empty() {
        while let Some(function) = pending_functions.pop() {
            if !visited_functions.insert(function) {
                continue;
            }
            let Some(definition) = reachable_function_definition(unit, function) else {
                continue;
            };
            let body = definition.body.expect("reachable definition has a body");
            // Calls bind converted arguments even when the body never reads
            // them (or mentions them only in sizeof). The integer-only slot
            // machine cannot hold a null pointer or an ignored FP parameter.
            // Inspect the reached definition, not every prototype in the unit;
            // unsupported unrelated declarations must remain harmless.
            let signature_requires_memory = unit.types().get(definition.ty).is_some_and(|ty| {
                let TypeKind::Function {
                    return_type,
                    parameters,
                } = &ty.kind
                else {
                    return false;
                };
                let result = unit.types().get(*return_type).is_some_and(|ty| {
                    !matches!(ty.kind, TypeKind::Void)
                        && type_requires_memory(unit.types(), *return_type)
                });
                result
                    || match parameters {
                        FunctionParameters::Prototype { types, .. } => types
                            .iter()
                            .any(|ty| type_requires_memory(unit.types(), *ty)),
                        FunctionParameters::Unspecified => false,
                    }
            });
            if signature_requires_memory || statement_requires_memory(unit, body) {
                requires_memory = true;
            }
            let mut referenced = ReferencedSymbols::default();
            collect_statement_symbols(body, &mut referenced);
            classify_referenced_symbols(
                unit,
                linkage,
                referenced,
                &mut pending_functions,
                &mut static_objects,
                &mut pending_objects,
            );
        }

        while let Some(object) = pending_objects.pop() {
            // Function-static initializers were already visited with their
            // declaration by collect_statement_symbols. Only file objects
            // need this separate initializer-closure lookup.
            let Some(declaration) = find_storage_object_declaration(unit, object, &[]) else {
                continue;
            };
            let Some(initializer) = &declaration.initializer else {
                continue;
            };
            let mut referenced = ReferencedSymbols::default();
            collect_initializer_symbols(initializer.plan(), &mut referenced);
            classify_referenced_symbols(
                unit,
                linkage,
                referenced,
                &mut pending_functions,
                &mut static_objects,
                &mut pending_objects,
            );
        }
    }

    // Allocating a static object and running without the memory machine are
    // contradictory, so the two outputs are reconciled here rather than left to
    // agree by accident. A `static` local is exactly the case a purely
    // syntactic scan misses.
    let requires_memory = requires_memory
        || static_objects
            .iter()
            .any(|symbol| has_static_storage_duration(unit, *symbol));

    MemoryRequirements {
        requires_memory,
        static_objects,
    }
}

/// Route each named symbol to the worklist that owns it. Unknown symbols are
/// dropped here; every consumer re-resolves them and fails closed on its own.
pub(super) fn classify_referenced_symbols(
    unit: &CHirUnit<'_>,
    linkage: &crate::linkage::SymbolLinkage,
    referenced: ReferencedSymbols,
    pending_functions: &mut Vec<SymbolId>,
    static_objects: &mut BTreeSet<SymbolId>,
    pending_objects: &mut Vec<SymbolId>,
) {
    for symbol in referenced.named {
        let symbol = linkage.target(symbol);
        let Ok(declaration) = unit.symbols().symbol(symbol) else {
            continue;
        };
        match declaration.kind() {
            SymbolKind::Function => pending_functions.push(symbol),
            SymbolKind::Object => {
                if static_objects.insert(symbol) {
                    pending_objects.push(symbol);
                }
            }
            SymbolKind::Parameter | SymbolKind::Typedef | SymbolKind::Enumerator => {}
        }
    }
}

// Builtin prototypes have their own semantic identity, but an ordinary call
// still links to a compatible definition of the library name in this TU.
pub(super) fn linked_library_definition(unit: &CHirUnit<'_>, function: SymbolId) -> SymbolId {
    if !unit.symbols().is_builtin_function(function) {
        return function;
    }
    let Ok(prototype) = unit.symbols().symbol(function) else {
        return function;
    };
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(definition)
                if definition.body.is_some()
                    && definition.name.spelling == prototype.spelling()
                    && unit.types().compatible(definition.ty, prototype.ty()) =>
            {
                Some(definition.symbol)
            }
            _ => None,
        })
        .unwrap_or(function)
}

pub(super) fn reachable_function_definition<'arena>(
    unit: &CHirUnit<'arena>,
    function: SymbolId,
) -> Option<&'arena FunctionDecl<'arena>> {
    let function = linked_library_definition(unit, function);
    unit.iter().find_map(|declaration| match declaration {
        CHirDecl::Function(definition)
            if definition.symbol == function && definition.body.is_some() =>
        {
            Some(definition)
        }
        CHirDecl::Function(_)
        | CHirDecl::Object(_)
        | CHirDecl::Asm(_)
        | CHirDecl::Tag(_)
        | CHirDecl::Typedef(_) => None,
    })
}

/// Follow only recursive object-initializer edges. Integer expressions are
/// task-machine work, and peeling consecutive casts is an explicit loop.
pub(super) fn push_static_initializer_children<'arena>(
    expression: &'arena CHirExpr<'arena>,
    depth: usize,
    pending: &mut Vec<(&'arena CHirExpr<'arena>, usize)>,
) {
    if let CHirExpr::Cast(_, operand, _) = expression {
        pending.push((operand, depth));
        return;
    }
    let child_depth = depth.saturating_add(1);
    let mut push = |child: &'arena CHirExpr<'arena>| pending.push((child, child_depth));
    match expression {
        CHirExpr::Unary(UnaryOp::AddrOf | UnaryOp::Deref, operand, _)
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => push(operand),
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    push(value);
                }
            }
        }
        CHirExpr::MemoryRead { place, .. } => {
            for source in place.source_expressions() {
                push(source);
            }
        }
        _ => {}
    }
}

pub(super) fn collect_initializer_symbols<'arena>(
    plan: &crate::InitPlan<'arena>,
    output: &mut ReferencedSymbols,
) {
    for action in plan.actions() {
        if let InitAction::StoreScalar { value, .. } | InitAction::CopyAggregate { value, .. } =
            action
        {
            collect_expression_symbols(value, output);
        }
    }
}

pub(super) fn collect_declaration_symbols<'arena>(
    declaration: &'arena CHirDecl<'arena>,
    output: &mut ReferencedSymbols,
) {
    match declaration {
        CHirDecl::Object(object) => {
            output.record(object.symbol);
            if let Some(cleanup) = object.cleanup {
                collect_expression_symbols(cleanup, output);
            }
            if let Some(initializer) = &object.initializer {
                collect_initializer_symbols(initializer.plan(), output);
            }
        }
        CHirDecl::Function(function) => {
            output.record(function.symbol);
            if let Some(body) = function.body {
                collect_statement_symbols(body, output);
            }
        }
        CHirDecl::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_expression_symbols(operand.expression, output);
            }
        }
        CHirDecl::Tag(_) | CHirDecl::Typedef(_) => {}
    }
}

pub(super) fn collect_statement_symbols<'arena>(
    statement: &'arena CHirStmt<'arena>,
    output: &mut ReferencedSymbols,
) {
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_expression_symbols(operand.expression, output);
            }
        }
        CHirStmt::Expr(expression, _) => collect_expression_symbols(expression, output),
        CHirStmt::ComputedGoto(expression, _) => {
            collect_expression_symbols(expression, output);
        }
        CHirStmt::Return(expression, _) => {
            if let Some(expression) = expression {
                collect_expression_symbols(expression, output);
            }
        }
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            collect_expression_symbols(condition, output);
            collect_statement_symbols(then_statement, output);
            if let Some(else_statement) = else_statement {
                collect_statement_symbols(else_statement, output);
            }
        }
        CHirStmt::Switch(expression, body, _) | CHirStmt::While(expression, body, _) => {
            collect_expression_symbols(expression, output);
            collect_statement_symbols(body, output);
        }
        CHirStmt::DoWhile(body, expression, _) => {
            collect_statement_symbols(body, output);
            collect_expression_symbols(expression, output);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => {
                    collect_expression_symbols(expression, output);
                }
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        collect_declaration_symbols(declaration, output);
                    }
                }
            }
            if let Some(condition) = condition {
                collect_expression_symbols(condition, output);
            }
            if let Some(step) = step {
                collect_expression_symbols(step, output);
            }
            collect_statement_symbols(body, output);
        }
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        collect_declaration_symbols(declaration, output);
                    }
                    CHirBlockItem::Statement(statement) => {
                        collect_statement_symbols(statement, output);
                    }
                }
            }
        }
        CHirStmt::Label(_, child, _) | CHirStmt::Default(child, _) => {
            collect_statement_symbols(child, output);
        }
        CHirStmt::Case(expression, child, _) => {
            collect_expression_symbols(expression, output);
            collect_statement_symbols(child, output);
        }
        CHirStmt::CaseRange(first, last, child, _) => {
            collect_expression_symbols(first, output);
            collect_expression_symbols(last, output);
            collect_statement_symbols(child, output);
        }
        CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

pub(super) fn collect_expression_symbols<'arena>(
    expression: &'arena CHirExpr<'arena>,
    output: &mut ReferencedSymbols,
) {
    match expression {
        CHirExpr::Complex(access) => {
            for operand in &access.operands {
                collect_expression_symbols(operand, output);
            }
        }
        CHirExpr::Floating(access) => {
            for operand in &access.operands {
                collect_expression_symbols(operand, output);
            }
        }
        CHirExpr::Vector(access) => {
            for operand in &access.operands {
                collect_expression_symbols(operand, output);
            }
        }
        CHirExpr::Atomic(access) => {
            for operand in access.operands() {
                collect_expression_symbols(operand, output);
            }
        }
        CHirExpr::LabelAddress { label, .. } => {
            output.labels.insert(
                expression as *const CHirExpr<'_> as usize,
                label.spelling.clone(),
            );
        }
        CHirExpr::Ident { symbol, .. } => output.record(*symbol),
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => collect_expression_symbols(operand, output),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            collect_expression_symbols(left, output);
            collect_expression_symbols(right, output);
        }
        CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
            collect_expression_symbols(condition, output);
            collect_expression_symbols(then_expression, output);
            collect_expression_symbols(else_expression, output);
        }
        CHirExpr::Call(callee, arguments, _) => {
            collect_expression_symbols(callee, output);
            for argument in arguments {
                collect_expression_symbols(argument, output);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            collect_expression_symbols(lhs, output);
            collect_expression_symbols(rhs, output);
            collect_expression_symbols(target, output);
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            collect_expression_symbols(first, output);
            if let Some(second) = second {
                collect_expression_symbols(second, output);
            }
        }
        CHirExpr::StatementExpr { body, .. } => collect_statement_symbols(body, output),
        CHirExpr::CompoundLit(_, initializer, _) => {
            collect_initializer_symbols(initializer.plan(), output);
        }
        CHirExpr::Assign { target, value, .. } => {
            collect_expression_symbols(target, output);
            collect_expression_symbols(value, output);
        }
        CHirExpr::Update { target, .. } => collect_expression_symbols(target, output),
        CHirExpr::MemoryAssign { place, value, .. } => {
            collect_place_symbols(place, output);
            collect_expression_symbols(value, output);
        }
        CHirExpr::MemoryUpdate { place, .. } | CHirExpr::MemoryRead { place, .. } => {
            collect_place_symbols(place, output);
        }
        CHirExpr::Lit(_, _)
        | CHirExpr::FloatLit(_, _)
        | CHirExpr::CharLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::SizeOf(_, _)
        | CHirExpr::SizeOfExpr(_, _, _)
        | CHirExpr::BuiltinConstantP { .. }
        | CHirExpr::BuiltinObjectSize { .. }
        | CHirExpr::AlignOf(_, _)
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. }
        | CHirExpr::BuiltinVarArgPack { .. } => {}
    }
}

pub(super) fn collect_place_symbols<'arena>(
    place: &MemoryPlace<'arena>,
    output: &mut ReferencedSymbols,
) {
    if let PlaceRoot::Object { symbol } = place.root() {
        output.record(*symbol);
    }
    for expression in place.source_expressions() {
        collect_expression_symbols(expression, output);
    }
}

pub(super) fn declaration_requires_memory(unit: &CHirUnit<'_>, declaration: &CHirDecl<'_>) -> bool {
    let CHirDecl::Object(declaration) = declaration else {
        return false;
    };
    declaration.cleanup.is_some()
        || type_requires_memory(unit.types(), declaration.ty)
        || declaration.initializer.as_ref().is_some_and(|initializer| {
            initializer
                .plan()
                .actions()
                .iter()
                .any(|action| match action {
                    InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } => {
                        expression_requires_memory(unit, value)
                    }
                    InitAction::StoreString { .. } => true,
                    InitAction::ZeroScalar { .. } => false,
                })
        })
}

pub(super) fn type_requires_memory(types: &TypeInterner, ty: TypeId) -> bool {
    types.get(ty).is_none_or(|canonical| {
        canonical.qualifiers.is_volatile
            || canonical.qualifiers.is_atomic
            || !matches!(canonical.kind, TypeKind::Int(_) | TypeKind::Enum(_))
    })
}

pub(super) fn expression_requires_memory(unit: &CHirUnit<'_>, expression: &CHirExpr<'_>) -> bool {
    if unit.stack_allocation_size(expression).is_some() {
        return true;
    }
    match expression {
        CHirExpr::Atomic(_)
        | CHirExpr::Vector(_)
        | CHirExpr::Complex(_)
        | CHirExpr::Floating(_)
        // The legacy integer-only evaluator cannot execute an unfolderable
        // floating literal/cast. Use the typed task machine even if no object
        // is read, so invalid conversions retain their E3063 obligation.
        | CHirExpr::FloatLit(_, _) => true,
        CHirExpr::BuiltinConstantP { .. }
        | CHirExpr::BuiltinObjectSize { .. }
        | CHirExpr::BuiltinVarArgPack { .. } => false,
        // A file-scope *object* has static storage and therefore lives in
        // memory. A parameter can never legitimately be declared at file scope,
        // so it is left to the signature validator instead of silently forcing
        // memory mode and masking that diagnostic.
        CHirExpr::Ident { symbol, facts, .. } => {
            unit.symbols()
                .symbol(*symbol)
                .ok()
                .is_some_and(|declaration| {
                    declaration.kind() == SymbolKind::Function
                        || matches!(
                            declaration.kind(),
                            SymbolKind::Object | SymbolKind::Parameter
                        ) && (has_static_storage_duration(unit, *symbol)
                            || type_requires_memory(unit.types(), facts.ty()))
                })
        }
        CHirExpr::Unary(UnaryOp::AddrOf | UnaryOp::Deref, _, _)
        | CHirExpr::MemoryAssign { .. }
        | CHirExpr::MemoryRead { .. }
        | CHirExpr::MemoryUpdate { .. }
        | CHirExpr::StringLit(_, _)
        | CHirExpr::Member(_, _, _)
        | CHirExpr::PtrMember(_, _, _)
        | CHirExpr::Index(_, _, _)
        | CHirExpr::CompoundLit(_, _, _)
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => true,
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::BitBuiltin { operand, .. } => expression_requires_memory(unit, operand),
        CHirExpr::Cast(ty, operand, _) => {
            type_requires_memory(unit.types(), *ty) || expression_requires_memory(unit, operand)
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            expression_requires_memory(unit, left) || expression_requires_memory(unit, right)
        }
        CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
            expression_requires_memory(unit, condition)
                || expression_requires_memory(unit, then_expression)
                || expression_requires_memory(unit, else_expression)
        }
        CHirExpr::Call(callee, arguments, _) => {
            (!matches!(callee, CHirExpr::Ident { facts, .. } if facts.category() == ExprCategory::FunctionDesignator)
                && PointerTypeDesc::from_expression(unit.symbols(), unit.types(), callee)
                    .is_ok_and(|p| {
                        matches!(
                            unit.types().get(p.pointee()).map(|t| &t.kind),
                            Some(TypeKind::Function { .. })
                        )
                    })
                && expression_requires_memory(unit, callee))
                || arguments
                    .iter()
                    .any(|argument| expression_requires_memory(unit, argument))
        }
        // The result pointer is written even when the mathematical operation
        // does not overflow, so checked arithmetic always selects the explicit
        // memory task machine.
        CHirExpr::CheckedArithmetic { .. } => true,
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            expression_requires_memory(unit, first)
                || second.is_some_and(|second| expression_requires_memory(unit, second))
        }
        CHirExpr::StatementExpr { body, .. } => statement_requires_memory(unit, body),
        CHirExpr::Assign { target, value, .. } => {
            expression_requires_memory(unit, target) || expression_requires_memory(unit, value)
        }
        CHirExpr::Update { target, .. } => expression_requires_memory(unit, target),
        CHirExpr::Lit(_, _)
        | CHirExpr::CharLit(_, _)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(_, _)
        | CHirExpr::SizeOfExpr(_, _, _)
        | CHirExpr::AlignOf(_, _)
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. } => false,
    }
}

pub(super) fn find_storage_object_declaration<'arena>(
    unit: &CHirUnit<'arena>,
    symbol: SymbolId,
    function_statics: &[&'arena ObjectDecl<'arena>],
) -> Option<ObjectDecl<'arena>> {
    let declarations: Vec<_> = unit
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object) => Some(object),
            _ => None,
        })
        .chain(function_statics.iter().copied())
        .filter(|object| object.symbol == symbol)
        .collect();
    declarations
        .iter()
        .find(|declaration| declaration.initializer.is_some())
        .or_else(|| {
            declarations
                .iter()
                .find(|declaration| declaration.storage.base() != StorageClass::Extern)
        })
        .map(|object| (*object).clone())
}
