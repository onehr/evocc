//! Lowering symbols.

use super::*;

/// A valid compile-time shift count is the canonical true predicate. Keeping
/// the obligation lets checked MIR omit the branch under 8.12/9.4 without
/// changing the EvoIR interface for a constant invalid or variable count.
pub(super) fn fold_constant_shift_counts(module: &mut Module) {
    for function in &mut module.functions {
        for block in &mut function.blocks {
            let mut constants: HashMap<u32, u64> = HashMap::new();
            for instruction in &mut block.insts {
                match instruction {
                    Inst::Const(value, raw) => {
                        constants.insert(value.id, *raw);
                    }
                    Inst::ShiftCountDefined {
                        result,
                        shifted,
                        amount,
                        amount_signed,
                    } => {
                        let Some(raw) = constants.get(&amount.id).copied() else {
                            continue;
                        };
                        let EvoIRType::Bits(width) = shifted.ty else {
                            continue;
                        };
                        let EvoIRType::Bits(amount_width) = amount.ty else {
                            continue;
                        };
                        let negative = *amount_signed
                            && amount_width > 0
                            && raw & (1_u64 << (amount_width - 1)) != 0;
                        if !negative && raw < u64::from(width) {
                            *instruction = Inst::Const(result.clone(), 1);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

pub(super) fn collect_global_register_symbols(unit: &CHirUnit<'_>) -> BTreeSet<SymbolId> {
    unit.iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object)
                if object.storage == StorageClass::Register && object.asm_label.is_some() =>
            {
                Some(object.symbol)
            }
            _ => None,
        })
        .collect()
}

pub(super) fn has_linked_object_storage(unit: &CHirUnit<'_>, symbol: SymbolId) -> bool {
    unit.symbols().symbol(symbol).is_ok_and(|entry| {
        entry.kind() == SymbolKind::Object
            && (entry.storage_class().map(StorageClass::base) == Some(StorageClass::Extern)
                || unit
                    .symbols()
                    .is_bound_in_scope(symbol, unit.symbols().file_scope())
                    .unwrap_or(false))
    })
}

pub(super) fn collect_memory_symbols(unit: &CHirUnit<'_>) -> BTreeSet<SymbolId> {
    fn declaration(unit: &CHirUnit<'_>, declaration: &CHirDecl<'_>, out: &mut BTreeSet<SymbolId>) {
        if let CHirDecl::Object(object) = declaration {
            if let Some(cleanup) = object.cleanup {
                expression(unit, cleanup, out);
            }
            // Deferred formats retain storage-only use (sizeof/address-of).
            // Executable scalar FP can use the ordinary local SSA environment;
            // address-taking and qualified/linked storage are collected below.
            let needs_object_storage = match unit.types().get(object.ty).map(|ty| &ty.kind) {
                Some(TypeKind::Float(kind)) => floating::ir_type(*kind).is_none(),
                Some(
                    TypeKind::Array { .. }
                    | TypeKind::Vector { .. }
                    | TypeKind::Struct(_)
                    | TypeKind::Union(_)
                    | TypeKind::BuiltinVaList
                    | TypeKind::Complex(_),
                ) => true,
                _ => false,
            };
            let is_qualified_memory = unit
                .types()
                .get(object.ty)
                .is_some_and(|ty| ty.qualifiers.is_volatile || ty.qualifiers.is_atomic);
            if needs_object_storage
                || object.cleanup.is_some()
                || has_linked_object_storage(unit, object.symbol)
                || is_qualified_memory
                || object.storage.base() == StorageClass::Static
            {
                out.insert(object.symbol);
            }
            if let Some(initializer) = &object.initializer {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } = action
                    {
                        expression(unit, value, out);
                    }
                }
            }
        } else if let CHirDecl::Function(function) = declaration {
            if let ParameterSyntax::Prototype { parameters, .. }
            | ParameterSyntax::IdentifierList { parameters } = &function.parameters
            {
                for parameter in parameters {
                    if unit.types().get(parameter.ty).is_some_and(|ty| {
                        ty.qualifiers.is_volatile
                            || ty.qualifiers.is_atomic
                            || is_aggregate_type(unit.types(), parameter.ty)
                    }) {
                        if let Some(symbol) = parameter.symbol {
                            out.insert(symbol);
                        }
                    }
                }
            }
        }
    }

    fn place(unit: &CHirUnit<'_>, place: &MemoryPlace<'_>, out: &mut BTreeSet<SymbolId>) {
        if let Some(symbol) = place.root_symbol() {
            out.insert(symbol);
        }
        for source in place.source_expressions() {
            expression(unit, source, out);
        }
    }

    fn expression(unit: &CHirUnit<'_>, expr: &CHirExpr<'_>, out: &mut BTreeSet<SymbolId>) {
        match expr {
            CHirExpr::Complex(operation) => {
                for operand in &operation.operands {
                    expression(unit, operand, out);
                }
            }
            CHirExpr::Floating(operation) => {
                for operand in &operation.operands {
                    expression(unit, operand, out);
                }
            }
            CHirExpr::Vector(operation) => {
                for operand in &operation.operands {
                    expression(unit, operand, out);
                }
            }
            CHirExpr::Atomic(access) => {
                for operand in access.operands() {
                    expression(unit, operand, out);
                }
            }
            CHirExpr::Ident { symbol, .. } => {
                if has_linked_object_storage(unit, *symbol) {
                    out.insert(*symbol);
                }
            }
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => {
                if let CHirExpr::Ident { symbol, .. } = operand {
                    out.insert(*symbol);
                }
                expression(unit, operand, out);
            }
            CHirExpr::Unary(_, operand, _)
            | CHirExpr::EvaluateOnce(operand, _)
            | CHirExpr::Cast(_, operand, _)
            | CHirExpr::BitBuiltin { operand, .. }
            | CHirExpr::Member(operand, _, _)
            | CHirExpr::PtrMember(operand, _, _) => expression(unit, operand, out),
            CHirExpr::Binary(_, left, right, _)
            | CHirExpr::Index(left, right, _)
            | CHirExpr::Comma(left, right, _)
            | CHirExpr::Elvis(left, right, _)
            | CHirExpr::BuiltinExpect(left, right, _) => {
                expression(unit, left, out);
                expression(unit, right, out);
            }
            CHirExpr::Ternary(condition, left, right, _) => {
                expression(unit, condition, out);
                expression(unit, left, out);
                expression(unit, right, out);
            }
            CHirExpr::Call(callee, arguments, _) => {
                expression(unit, callee, out);
                for argument in arguments {
                    expression(unit, argument, out);
                }
            }
            CHirExpr::CheckedArithmetic {
                lhs, rhs, target, ..
            } => {
                expression(unit, lhs, out);
                expression(unit, rhs, out);
                expression(unit, target, out);
            }
            CHirExpr::BuiltinVarArgs { first, second, .. } => {
                expression(unit, first, out);
                if let Some(second) = second {
                    expression(unit, second, out);
                }
            }
            CHirExpr::BuiltinVarArgPack { .. } => {}
            CHirExpr::Assign { target, value, .. } => {
                expression(unit, target, out);
                expression(unit, value, out);
            }
            CHirExpr::Update { target, .. } => expression(unit, target, out),
            CHirExpr::MemoryRead { place: memory, .. }
            | CHirExpr::MemoryUpdate { place: memory, .. } => place(unit, memory, out),
            CHirExpr::MemoryAssign {
                place: memory,
                value,
                ..
            } => {
                place(unit, memory, out);
                expression(unit, value, out);
            }
            CHirExpr::CompoundLit(_, initializer, _) => {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } = action
                    {
                        expression(unit, value, out);
                    }
                }
            }
            CHirExpr::StatementExpr { body, .. } => statement(unit, body, out),
            CHirExpr::Lit(..)
            | CHirExpr::FloatLit(..)
            | CHirExpr::CharLit(..)
            | CHirExpr::StringLit(..)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::SizeOf(..)
            | CHirExpr::SizeOfExpr(..)
            | CHirExpr::BuiltinConstantP { .. }
            | CHirExpr::BuiltinObjectSize { .. }
            | CHirExpr::AlignOf(..)
            | CHirExpr::BuiltinTrap { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { .. } => {}
        }
    }

    fn statement(unit: &CHirUnit<'_>, stmt: &CHirStmt<'_>, out: &mut BTreeSet<SymbolId>) {
        match stmt {
            CHirStmt::Asm(assembly) => {
                // A register output does not take the variable's address.
                // Addressed, qualified and aggregate locals are collected by
                // their ordinary uses/declarations; only memory constraints
                // independently require storage at the asm boundary.
                for operand in assembly
                    .outputs
                    .iter()
                    .chain(&assembly.inputs)
                    .filter(|operand| evocc_evoir::assembly_memory_constraint(&operand.constraint))
                {
                    if let CHirExpr::Ident { symbol, .. } = operand.expression {
                        out.insert(*symbol);
                    }
                }
                for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                    expression(unit, operand.expression, out);
                }
            }
            CHirStmt::Expr(expr, _) | CHirStmt::Return(Some(expr), _) => {
                expression(unit, expr, out)
            }
            CHirStmt::ComputedGoto(expr, _) => expression(unit, expr, out),
            CHirStmt::If(condition, left, right, _) => {
                expression(unit, condition, out);
                statement(unit, left, out);
                if let Some(right) = right {
                    statement(unit, right, out);
                }
            }
            CHirStmt::Switch(expr, body, _)
            | CHirStmt::While(expr, body, _)
            | CHirStmt::Case(expr, body, _) => {
                expression(unit, expr, out);
                statement(unit, body, out);
            }
            CHirStmt::CaseRange(first, last, body, _) => {
                expression(unit, first, out);
                expression(unit, last, out);
                statement(unit, body, out);
            }
            CHirStmt::DoWhile(body, expr, _) => {
                statement(unit, body, out);
                expression(unit, expr, out);
            }
            CHirStmt::For(initializer, condition, step, body, _) => {
                match initializer {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(expr) => expression(unit, expr, out),
                    CHirForInit::Declaration(declarations) => {
                        for item in declarations {
                            declaration(unit, item, out);
                        }
                    }
                }
                if let Some(expr) = condition {
                    expression(unit, expr, out);
                }
                if let Some(expr) = step {
                    expression(unit, expr, out);
                }
                statement(unit, body, out);
            }
            CHirStmt::Compound(items, _, _) => {
                for item in items {
                    match item {
                        CHirBlockItem::Declaration(item) => declaration(unit, item, out),
                        CHirBlockItem::Statement(item) => statement(unit, item, out),
                    }
                }
            }
            CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => statement(unit, body, out),
            CHirStmt::Return(None, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::AttributedNull(_, _)
            | CHirStmt::Null(_) => {}
        }
    }

    let mut captures_by_function: BTreeMap<SymbolId, Vec<SymbolId>> = BTreeMap::new();
    for raw in unit.types().captured_array_bounds() {
        let Some(symbol) = unit.symbols().symbols_in_source_order().get(raw as usize) else {
            continue;
        };
        let mut current = Some(symbol.scope());
        while let Some(scope) = current.and_then(|id| unit.symbols().scope(id).ok()) {
            if let Some(owner) = scope.function_owner() {
                captures_by_function
                    .entry(owner)
                    .or_default()
                    .push(symbol.id());
                break;
            }
            current = scope.parent();
        }
    }
    let mut symbols = BTreeSet::new();
    for declaration_node in unit.iter() {
        declaration(unit, declaration_node, &mut symbols);
        if let CHirDecl::Function(function) = declaration_node {
            if let Some(body) = function.body {
                statement(unit, body, &mut symbols);
                let mut references = FunctionReferences::default();
                collect_references_stmt(body, &mut references);
                if references.has_computed_goto {
                    // Captured dimensions remain live with their type even
                    // when no ordinary expression names their hidden object.
                    symbols.extend(
                        captures_by_function
                            .get(&function.symbol)
                            .into_iter()
                            .flatten(),
                    );
                    let names = references.labels.into_values().collect();
                    let indirect = indirect_storage_symbols(body, &names);
                    symbols.extend(indirect.into_iter().filter(|symbol| {
                        unit.symbols().symbol(*symbol).is_ok_and(|symbol| {
                            matches!(symbol.kind(), SymbolKind::Object | SymbolKind::Parameter)
                        })
                    }));
                }
            }
        }
    }
    symbols
}
