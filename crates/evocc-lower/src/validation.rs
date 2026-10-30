//! Lowering validation.

use super::*;

pub(super) fn prevalidate_unit_for_lowering(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    global_register_symbols: &BTreeSet<SymbolId>,
) -> Result<(), LowerError> {
    evocc_chir::validate_front_end_builtin_records(unit, target).map_err(|span| {
        LowerError::UnsupportedExpression {
            span,
            feature: "inconsistent front-end builtin record",
        }
    })?;
    for declaration in unit.iter() {
        match declaration {
            CHirDecl::Asm(_) => {}
            CHirDecl::Function(function) => {
                if let Some(body) = function.body {
                    let mut labels = BTreeMap::new();
                    prevalidate_stmt_for_lowering(
                        unit,
                        target,
                        body,
                        &mut labels,
                        global_register_symbols,
                    )?;
                    let mut references = FunctionReferences::default();
                    collect_references_stmt(body, &mut references);
                    if !references.goto_names.is_subset(&references.defined_labels) {
                        return Err(LowerError::InvalidControlFlow {
                            span: function.span,
                            reason: "goto targets a label absent from its function",
                        });
                    }
                }
            }
            CHirDecl::Object(object) => {
                validate_object_declaration_for_lowering(unit, target, object)?;
            }
            CHirDecl::Tag(_) | CHirDecl::Typedef(_) => {}
        }
    }
    Ok(())
}

pub(super) fn prevalidate_decl_for_lowering(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    declaration: &CHirDecl<'_>,
    global_register_symbols: &BTreeSet<SymbolId>,
) -> Result<(), LowerError> {
    match declaration {
        CHirDecl::Asm(assembly) => Err(LowerError::Asm {
            span: assembly.span,
        }),
        CHirDecl::Object(object) => {
            validate_object_declaration_for_lowering(unit, target, object)?;
            if let Some(cleanup) = object.cleanup {
                prevalidate_expr_for_lowering(unit, target, cleanup, global_register_symbols)?;
            }
            if let Some(initializer) = &object.initializer {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } = action
                    {
                        prevalidate_expr_for_lowering(
                            unit,
                            target,
                            value,
                            global_register_symbols,
                        )?;
                    }
                }
            }
            Ok(())
        }
        CHirDecl::Tag(_) | CHirDecl::Typedef(_) => Ok(()),
        CHirDecl::Function(function) if function.body.is_none() => Ok(()),
        CHirDecl::Function(function) => Err(LowerError::UnsupportedStatement {
            span: function.span,
            feature: "block function declaration is outside scalar lowering",
        }),
    }
}

pub(super) fn prevalidate_stmt_for_lowering(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    statement: &CHirStmt<'_>,
    labels: &mut BTreeMap<String, SourceSpan>,
    global_register_symbols: &BTreeSet<SymbolId>,
) -> Result<(), LowerError> {
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                prevalidate_expr_for_lowering(
                    unit,
                    target,
                    operand.expression,
                    global_register_symbols,
                )?;
            }
            Ok(())
        }
        CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
            prevalidate_expr_for_lowering(unit, target, expression, global_register_symbols)
        }
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            prevalidate_expr_for_lowering(unit, target, condition, global_register_symbols)?;
            prevalidate_stmt_for_lowering(
                unit,
                target,
                then_branch,
                labels,
                global_register_symbols,
            )?;
            if let Some(else_branch) = else_branch {
                prevalidate_stmt_for_lowering(
                    unit,
                    target,
                    else_branch,
                    labels,
                    global_register_symbols,
                )?;
            }
            Ok(())
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            prevalidate_expr_for_lowering(unit, target, expression, global_register_symbols)?;
            prevalidate_stmt_for_lowering(unit, target, body, labels, global_register_symbols)
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            prevalidate_expr_for_lowering(unit, target, first, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, target, last, global_register_symbols)?;
            prevalidate_stmt_for_lowering(unit, target, body, labels, global_register_symbols)
        }
        CHirStmt::DoWhile(body, expression, _) => {
            prevalidate_stmt_for_lowering(unit, target, body, labels, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, target, expression, global_register_symbols)
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => {
                    prevalidate_expr_for_lowering(
                        unit,
                        target,
                        expression,
                        global_register_symbols,
                    )?;
                }
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        prevalidate_decl_for_lowering(
                            unit,
                            target,
                            declaration,
                            global_register_symbols,
                        )?;
                    }
                }
            }
            if let Some(condition) = condition {
                prevalidate_expr_for_lowering(unit, target, condition, global_register_symbols)?;
            }
            if let Some(step) = step {
                prevalidate_expr_for_lowering(unit, target, step, global_register_symbols)?;
            }
            prevalidate_stmt_for_lowering(unit, target, body, labels, global_register_symbols)
        }
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        prevalidate_decl_for_lowering(
                            unit,
                            target,
                            declaration,
                            global_register_symbols,
                        )?;
                    }
                    CHirBlockItem::Statement(statement) => {
                        prevalidate_stmt_for_lowering(
                            unit,
                            target,
                            statement,
                            labels,
                            global_register_symbols,
                        )?;
                    }
                }
            }
            Ok(())
        }
        CHirStmt::Label(label, nested, span) => {
            if labels.insert(label.spelling.clone(), *span).is_some() {
                return Err(LowerError::InvalidControlFlow {
                    span: *span,
                    reason: "duplicate label in function",
                });
            }
            prevalidate_stmt_for_lowering(unit, target, nested, labels, global_register_symbols)
        }
        CHirStmt::Default(nested, _) => {
            prevalidate_stmt_for_lowering(unit, target, nested, labels, global_register_symbols)
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => Ok(()),
        CHirStmt::ComputedGoto(expression, _) => {
            prevalidate_expr_for_lowering(unit, target, expression, global_register_symbols)
        }
    }
}

pub(super) fn prevalidate_expr_for_lowering(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
    global_register_symbols: &BTreeSet<SymbolId>,
) -> Result<(), LowerError> {
    match expression {
        CHirExpr::Complex(operation) => {
            for operand in &operation.operands {
                prevalidate_expr_for_lowering(unit, target, operand, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::Floating(operation) => {
            for operand in &operation.operands {
                prevalidate_expr_for_lowering(unit, target, operand, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::Vector(operation) => {
            for operand in &operation.operands {
                prevalidate_expr_for_lowering(unit, target, operand, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::Atomic(access) => {
            for operand in access.operands() {
                prevalidate_expr_for_lowering(unit, target, operand, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::BuiltinConstantP { .. } | CHirExpr::BuiltinObjectSize { .. } => Ok(()),
        CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span,
        } => {
            let declaration =
                unit.symbols()
                    .symbol(*symbol)
                    .map_err(|_| LowerError::UnknownSymbol {
                        symbol: *symbol,
                        span: *span,
                    })?;
            let category_matches = match declaration.kind() {
                SymbolKind::Function => facts.category() == ExprCategory::FunctionDesignator,
                SymbolKind::Enumerator => facts.category() == ExprCategory::Value,
                SymbolKind::Object | SymbolKind::Parameter => {
                    facts.category() == ExprCategory::LValue
                }
                SymbolKind::Typedef => false,
            };
            if declaration.spelling() != spelling
                || !evocc_chir::type_at_use_matches_binding(
                    unit.types(),
                    facts.ty(),
                    declaration.ty(),
                )
                || !category_matches
            {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "identifier disagrees with its semantic identity",
                });
            }
            Ok(())
        }
        CHirExpr::Assign {
            place,
            target: destination,
            value,
            facts,
            span,
            ..
        } => {
            validate_place_identity(unit, *place, destination, *span)?;
            if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "assignment result facts disagree with its scalar place",
                });
            }
            prevalidate_expr_for_lowering(unit, target, destination, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, target, value, global_register_symbols)
        }
        CHirExpr::Update {
            place,
            target: destination,
            facts,
            span,
            ..
        } => {
            validate_place_identity(unit, *place, destination, *span)?;
            if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "update result facts disagree with its scalar place",
                });
            }
            prevalidate_expr_for_lowering(unit, target, destination, global_register_symbols)
        }
        CHirExpr::MemoryRead { place, span, .. } => {
            place
                .validate_against_with_target(unit.symbols(), unit.types(), target)
                .map_err(|_| LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory read place is not authoritative in this C-HIR unit",
                })?;
            for source in place.source_expressions() {
                prevalidate_expr_for_lowering(unit, target, source, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::MemoryAssign {
            place,
            value,
            facts,
            span,
            ..
        } => {
            place
                .validate_against_with_target(unit.symbols(), unit.types(), target)
                .map_err(|_| LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory assignment place is not authoritative in this C-HIR unit",
                })?;
            if facts.ty() != place.value_ty() || facts.category() != ExprCategory::Value {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory assignment result facts disagree with its place",
                });
            }
            for source in place.source_expressions() {
                prevalidate_expr_for_lowering(unit, target, source, global_register_symbols)?;
            }
            prevalidate_expr_for_lowering(unit, target, value, global_register_symbols)
        }
        CHirExpr::MemoryUpdate {
            place, facts, span, ..
        } => {
            place
                .validate_against_with_target(unit.symbols(), unit.types(), target)
                .map_err(|_| LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory update place is not authoritative in this C-HIR unit",
                })?;
            if facts.ty() != place.value_ty() || facts.category() != ExprCategory::Value {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory update result facts disagree with its place",
                });
            }
            for source in place.source_expressions() {
                prevalidate_expr_for_lowering(unit, target, source, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::Cast(ty, operand, span) => {
            if unit.types().get(*ty).is_none() {
                return Err(LowerError::UnknownType {
                    ty: *ty,
                    span: *span,
                });
            }
            prevalidate_expr_for_lowering(unit, target, operand, global_register_symbols)
        }
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => {
            prevalidate_expr_for_lowering(unit, target, operand, global_register_symbols)
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            prevalidate_expr_for_lowering(unit, target, left, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, target, right, global_register_symbols)
        }
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            prevalidate_expr_for_lowering(unit, target, condition, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, target, then_value, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, target, else_value, global_register_symbols)
        }
        CHirExpr::Call(callee, arguments, _) => {
            prevalidate_expr_for_lowering(unit, target, callee, global_register_symbols)?;
            for argument in arguments {
                prevalidate_expr_for_lowering(unit, target, argument, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::CheckedArithmetic {
            lhs,
            rhs,
            target: destination,
            ..
        } => {
            prevalidate_expr_for_lowering(unit, target, lhs, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, target, rhs, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, target, destination, global_register_symbols)
        }
        CHirExpr::BitBuiltin { operand, .. } => {
            prevalidate_expr_for_lowering(unit, target, operand, global_register_symbols)
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            prevalidate_expr_for_lowering(unit, target, first, global_register_symbols)?;
            if let Some(second) = second {
                prevalidate_expr_for_lowering(unit, target, second, global_register_symbols)?;
            }
            Ok(())
        }
        // Substitution replaces the pack at each call site. A body that
        // keeps it has no anonymous arguments to forward.
        CHirExpr::BuiltinVarArgPack { span, .. } => Err(LowerError::VarArgPack { span: *span }),
        CHirExpr::BuiltinReturnAddress { depth, .. }
        | CHirExpr::BuiltinFrameAddress { depth, .. }
            if u16::try_from(*depth).is_ok() =>
        {
            Ok(())
        }
        CHirExpr::BuiltinReturnAddress { span, .. }
        | CHirExpr::BuiltinFrameAddress { span, .. } => {
            Err(LowerError::FrameAddress { span: *span })
        }
        CHirExpr::LabelAddress { .. } => Ok(()),
        CHirExpr::BuiltinUnreachable { .. } | CHirExpr::BuiltinTrap { .. } => Ok(()),
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    prevalidate_expr_for_lowering(unit, target, value, global_register_symbols)?;
                }
            }
            Ok(())
        }
        CHirExpr::StatementExpr { body, .. } => {
            let mut labels = BTreeMap::new();
            prevalidate_stmt_for_lowering(unit, target, body, &mut labels, global_register_symbols)
        }
        CHirExpr::SizeOf(ty, span)
        | CHirExpr::SizeOfExpr(_, ty, span)
        | CHirExpr::AlignOf(ty, span) => {
            if unit.types().get(*ty).is_none() {
                Err(LowerError::UnknownType {
                    ty: *ty,
                    span: *span,
                })
            } else {
                Ok(())
            }
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..) => Ok(()),
    }
}

pub(super) fn validate_place_identity(
    unit: &CHirUnit<'_>,
    place: ScalarPlace,
    target: &CHirExpr<'_>,
    span: SourceSpan,
) -> Result<(), LowerError> {
    let CHirExpr::Ident {
        symbol,
        spelling,
        facts,
        span: target_span,
    } = target
    else {
        return Err(LowerError::UnsupportedExpression {
            span,
            feature: "assignment target is not a scalar identifier place",
        });
    };
    let declaration = unit
        .symbols()
        .symbol(*symbol)
        .map_err(|_| LowerError::UnknownSymbol {
            symbol: *symbol,
            span: *target_span,
        })?;
    if *symbol != place.symbol()
        || !matches!(
            declaration.kind(),
            SymbolKind::Object | SymbolKind::Parameter
        )
        || declaration.spelling() != spelling
        || declaration.ty() != place.ty()
        || facts.ty() != place.ty()
        || facts.category() != ExprCategory::LValue
        || !facts.is_modifiable_lvalue()
    {
        return Err(LowerError::UnsupportedExpression {
            span: *target_span,
            feature: "scalar place disagrees with its semantic identity",
        });
    }
    Ok(())
}

pub(super) fn validate_object_declaration_for_lowering(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    object: &ObjectDecl<'_>,
) -> Result<(), LowerError> {
    let symbol = unit
        .symbols()
        .symbol(object.symbol)
        .map_err(|_| LowerError::UnknownSymbol {
            symbol: object.symbol,
            span: object.span,
        })?;
    if !matches!(
        object.storage.base(),
        StorageClass::None
            | StorageClass::Auto
            | StorageClass::Register
            | StorageClass::Extern
            | StorageClass::Static
    ) {
        return Err(LowerError::UnsupportedStatement {
            span: object.span,
            feature: "object declaration with the typedef storage class",
        });
    }
    if symbol.kind() != SymbolKind::Object
        || symbol.spelling() != object.name.spelling
        || !unit.types().compatible(symbol.ty(), object.ty)
        // AllocationSiteManifest has already checked storage compatibility
        // with lexical context, including an extern followed by a definition.
        || !written_declarator_matches(
            unit.types(),
            &object.written,
            object.ty,
            object.specifier.type_id(),
            None,
            object.initializer.is_some(),
            false,
            (target, &object.attributes),
        )
    {
        return Err(LowerError::UnsupportedStatement {
            span: object.span,
            feature: "object declaration disagrees with its semantic identity",
        });
    }
    if let Some(initializer) = &object.initializer {
        let plan = initializer.plan();
        if plan.target_type() != object.ty || plan.validate(unit.types()).is_err() {
            return Err(LowerError::UnsupportedStatement {
                span: initializer.span(),
                feature: "initializer plan disagrees with its semantic identity",
            });
        }
    }
    Ok(())
}
