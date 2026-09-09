//! Conservative front-end constant knowledge shared by every C-HIR consumer.

use evocc_common::SourceSpan;
use evocc_types::{
    enum_integer_rank, TagDefinition, TagId, TagState, TargetDataLayout, TypeId, TypeInterner,
    TypeKind,
};

use crate::integer::{
    eval_bit_builtin, eval_constant_binary_integer, eval_constant_unary_integer,
    floating_literal_to_integer, integer_literal_value, integer_type_for_rank,
    usual_arithmetic_type, CInteger, CIntegerType,
};
use crate::{
    record_member_path, BinaryOp, CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt,
    CHirUnit, FrontEndBuiltinRecord, InitAction, PlaceProjection, SymbolId, SymbolKind,
    SymbolTable, TagMembers, UnaryOp,
};

/// Declaration identity designated by a `__builtin_has_attribute` operand.
///
/// This is derived from typed C-HIR rather than retained in the builtin record,
/// so consumers can independently check the producer's constant result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeQueryTarget {
    Entity(SymbolId),
    Member { tag: TagId, field_index: usize },
}

/// Resolve the declaration designated by an attribute-query expression.
#[must_use]
pub fn attribute_query_target(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
) -> Option<AttributeQueryTarget> {
    match expression {
        CHirExpr::Ident { symbol, .. } => Some(AttributeQueryTarget::Entity(*symbol)),
        CHirExpr::Member(base, name, _) => {
            let base_ty = attribute_query_expression_type(base, types)?;
            let field = record_member_path(types, base_ty, name)?.last().copied()?;
            Some(AttributeQueryTarget::Member {
                tag: field.tag(),
                field_index: field.field_index(),
            })
        }
        CHirExpr::PtrMember(pointer, name, _) => {
            let pointer_ty = attribute_query_expression_type(pointer, types)?;
            let TypeKind::Pointer(pointee) = types.get(pointer_ty).map(|ty| &ty.kind)? else {
                return None;
            };
            let field = record_member_path(types, *pointee, name)?.last().copied()?;
            Some(AttributeQueryTarget::Member {
                tag: field.tag(),
                field_index: field.field_index(),
            })
        }
        CHirExpr::MemoryRead { place, .. } => place
            .projections()
            .iter()
            .rev()
            .find_map(|projection| match projection {
                PlaceProjection::Field(field) => Some(AttributeQueryTarget::Member {
                    tag: field.tag(),
                    field_index: field.field_index(),
                }),
                PlaceProjection::Index(_) => None,
            })
            .or_else(|| place.root_symbol().map(AttributeQueryTarget::Entity)),
        CHirExpr::Cast(_, operand, _) | CHirExpr::EvaluateOnce(operand, _) => {
            attribute_query_target(operand, types)
        }
        CHirExpr::Comma(_, right, _) => attribute_query_target(right, types),
        _ => None,
    }
}

/// Recompute one front-end-resolved attribute query from the declarations
/// preserved in C-HIR.
#[must_use]
fn builtin_has_attribute(
    unit: &CHirUnit<'_>,
    operand: Option<&CHirExpr<'_>>,
    attribute: &str,
) -> bool {
    operand
        .and_then(|expression| attribute_query_target(expression, unit.types()))
        .is_some_and(|target| {
            unit.iter()
                .any(|declaration| declaration_has_attribute(declaration, target, attribute))
        })
}

/// The exact conservative predicate frozen for `__builtin_constant_p`.
///
/// This function deliberately knows only source-language integer constant
/// expressions and string literals. Object values, address constants, calls,
/// side effects, and optimizer facts all fail closed.
#[must_use]
pub fn builtin_constant_p(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> bool {
    matches!(expression, CHirExpr::StringLit(_, _))
        || eval_integer_constant(expression, types, symbols, target).is_some()
}

/// Recompute every retained constant-knowledge decision. Consumers call this
/// before execution or lowering instead of trusting a producer-supplied bit.
pub fn validate_front_end_builtin_records(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
) -> Result<(), SourceSpan> {
    for record in unit.front_end_builtins() {
        match record {
            FrontEndBuiltinRecord::Expect { .. } => {}
            FrontEndBuiltinRecord::ConstantP {
                span,
                operand,
                result,
            } => {
                if builtin_constant_p(operand, unit.types(), unit.symbols(), target) != *result {
                    return Err(*span);
                }
            }
            FrontEndBuiltinRecord::ObjectSize {
                span, mode, result, ..
            } => {
                let expected = if *mode <= 1 { u64::MAX } else { 0 };
                if *mode > 3 || *result != expected {
                    return Err(*span);
                }
            }
            FrontEndBuiltinRecord::HasAttribute {
                span,
                operand,
                attribute,
                result,
            } => {
                if builtin_has_attribute(unit, *operand, attribute) != *result {
                    return Err(*span);
                }
            }
        }
    }
    Ok(())
}

fn attribute_query_expression_type(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
) -> Option<TypeId> {
    match expression {
        CHirExpr::Ident { facts, .. }
        | CHirExpr::CheckedArithmetic { facts, .. }
        | CHirExpr::BitBuiltin { facts, .. }
        | CHirExpr::BuiltinUnreachable { facts, .. }
        | CHirExpr::BuiltinReturnAddress { facts, .. }
        | CHirExpr::BuiltinFrameAddress { facts, .. }
        | CHirExpr::BuiltinVarArgs { facts, .. }
        | CHirExpr::Assign { facts, .. }
        | CHirExpr::MemoryAssign { facts, .. }
        | CHirExpr::MemoryRead { facts, .. }
        | CHirExpr::Update { facts, .. }
        | CHirExpr::MemoryUpdate { facts, .. }
        | CHirExpr::StatementExpr { facts, .. } => Some(facts.ty()),
        CHirExpr::Cast(ty, _, _) | CHirExpr::CompoundLit(ty, _, _) => Some(*ty),
        CHirExpr::EvaluateOnce(operand, _) => attribute_query_expression_type(operand, types),
        CHirExpr::Member(base, name, _) => {
            let base_ty = attribute_query_expression_type(base, types)?;
            record_member_path(types, base_ty, name)
                .and_then(|path| path.last().map(|field| field.field_ty()))
        }
        CHirExpr::PtrMember(pointer, name, _) => {
            let pointer_ty = attribute_query_expression_type(pointer, types)?;
            let TypeKind::Pointer(pointee) = types.get(pointer_ty).map(|ty| &ty.kind)? else {
                return None;
            };
            record_member_path(types, *pointee, name)
                .and_then(|path| path.last().map(|field| field.field_ty()))
        }
        CHirExpr::Index(base, _, _) => {
            let base_ty = attribute_query_expression_type(base, types)?;
            match types.get(base_ty).map(|ty| &ty.kind)? {
                TypeKind::Array { element, .. } | TypeKind::Pointer(element) => Some(*element),
                _ => None,
            }
        }
        CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
            let pointer_ty = attribute_query_expression_type(pointer, types)?;
            match types.get(pointer_ty).map(|ty| &ty.kind)? {
                TypeKind::Pointer(pointee) => Some(*pointee),
                _ => None,
            }
        }
        CHirExpr::Comma(_, right, _) => attribute_query_expression_type(right, types),
        _ => None,
    }
}

fn declaration_has_attribute(
    declaration: &CHirDecl<'_>,
    target: AttributeQueryTarget,
    attribute: &str,
) -> bool {
    match declaration {
        CHirDecl::Asm(assembly) => assembly
            .outputs
            .iter()
            .chain(&assembly.inputs)
            .any(|operand| expression_contains_declaration(operand.expression, target, attribute)),
        CHirDecl::Object(object) => {
            (target == AttributeQueryTarget::Entity(object.symbol)
                && has_attribute(&object.attributes, attribute))
                || object.initializer.as_ref().is_some_and(|initializer| {
                    initializer
                        .plan()
                        .actions()
                        .iter()
                        .any(|action| match action {
                            InitAction::StoreScalar { value, .. }
                            | InitAction::CopyAggregate { value, .. } => {
                                expression_contains_declaration(value, target, attribute)
                            }
                            InitAction::StoreString { .. } | InitAction::ZeroScalar { .. } => false,
                        })
                })
        }
        CHirDecl::Function(function) => {
            (target == AttributeQueryTarget::Entity(function.symbol)
                && has_attribute(&function.attributes, attribute))
                || match &function.parameters {
                    crate::ParameterSyntax::Prototype { parameters, .. }
                    | crate::ParameterSyntax::IdentifierList { parameters } => {
                        parameters.iter().any(|parameter| {
                            parameter.symbol == entity_target(target)
                                && has_attribute(&parameter.attributes, attribute)
                        })
                    }
                    crate::ParameterSyntax::Unspecified => false,
                }
                || function
                    .body
                    .is_some_and(|body| statement_contains_declaration(body, target, attribute))
        }
        CHirDecl::Typedef(typedef) => {
            target == AttributeQueryTarget::Entity(typedef.symbol)
                && has_attribute(&typedef.attributes, attribute)
        }
        CHirDecl::Tag(tag) => match &tag.members {
            TagMembers::Struct(members) | TagMembers::Union(members) => {
                let own_member = match target {
                    AttributeQueryTarget::Member {
                        tag: target_tag,
                        field_index,
                    } if target_tag == tag.tag => members
                        .get(field_index)
                        .is_some_and(|member| has_attribute(&member.attributes, attribute)),
                    AttributeQueryTarget::Entity(_) | AttributeQueryTarget::Member { .. } => false,
                };
                own_member
                    || members.iter().any(|member| {
                        member.bit_width.is_some_and(|width| {
                            expression_contains_declaration(width, target, attribute)
                        })
                    })
            }
            TagMembers::Enum(enumerators) => enumerators.iter().any(|enumerator| {
                (target == AttributeQueryTarget::Entity(enumerator.symbol)
                    && has_attribute(&enumerator.attributes, attribute))
                    || enumerator.written_value.is_some_and(|value| {
                        expression_contains_declaration(value, target, attribute)
                    })
            }),
            TagMembers::Incomplete => false,
        },
    }
}

fn entity_target(target: AttributeQueryTarget) -> Option<SymbolId> {
    match target {
        AttributeQueryTarget::Entity(symbol) => Some(symbol),
        AttributeQueryTarget::Member { .. } => None,
    }
}

fn has_attribute(attributes: &[crate::Attribute], name: &str) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.query_name() == Some(name))
}

fn statement_contains_declaration(
    statement: &CHirStmt<'_>,
    target: AttributeQueryTarget,
    attribute: &str,
) -> bool {
    match statement {
        CHirStmt::Asm(assembly) => assembly
            .outputs
            .iter()
            .chain(&assembly.inputs)
            .any(|operand| expression_contains_declaration(operand.expression, target, attribute)),
        CHirStmt::Expr(expression, _)
        | CHirStmt::Return(Some(expression), _)
        | CHirStmt::ComputedGoto(expression, _) => {
            expression_contains_declaration(expression, target, attribute)
        }
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            expression_contains_declaration(condition, target, attribute)
                || statement_contains_declaration(then_statement, target, attribute)
                || else_statement.is_some_and(|statement| {
                    statement_contains_declaration(statement, target, attribute)
                })
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            expression_contains_declaration(expression, target, attribute)
                || statement_contains_declaration(body, target, attribute)
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            expression_contains_declaration(first, target, attribute)
                || expression_contains_declaration(last, target, attribute)
                || statement_contains_declaration(body, target, attribute)
        }
        CHirStmt::DoWhile(body, condition, _) => {
            statement_contains_declaration(body, target, attribute)
                || expression_contains_declaration(condition, target, attribute)
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            let initializer_contains = match initializer {
                CHirForInit::Empty => false,
                CHirForInit::Expression(expression) => {
                    expression_contains_declaration(expression, target, attribute)
                }
                CHirForInit::Declaration(declarations) => declarations
                    .iter()
                    .any(|declaration| declaration_has_attribute(declaration, target, attribute)),
            };
            initializer_contains
                || condition.is_some_and(|expression| {
                    expression_contains_declaration(expression, target, attribute)
                })
                || step.is_some_and(|expression| {
                    expression_contains_declaration(expression, target, attribute)
                })
                || statement_contains_declaration(body, target, attribute)
        }
        CHirStmt::Compound(items, _) => items.iter().any(|item| match item {
            CHirBlockItem::Declaration(declaration) => {
                declaration_has_attribute(declaration, target, attribute)
            }
            CHirBlockItem::Statement(statement) => {
                statement_contains_declaration(statement, target, attribute)
            }
        }),
        CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => {
            statement_contains_declaration(body, target, attribute)
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => false,
    }
}

fn expression_contains_declaration(
    expression: &CHirExpr<'_>,
    target: AttributeQueryTarget,
    attribute: &str,
) -> bool {
    match expression {
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::SizeOfExpr(operand, _, _)
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Update {
            target: operand, ..
        } => expression_contains_declaration(operand, target, attribute),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::Assign {
            target: left,
            value: right,
            ..
        } => {
            expression_contains_declaration(left, target, attribute)
                || expression_contains_declaration(right, target, attribute)
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            expression_contains_declaration(condition, target, attribute)
                || expression_contains_declaration(when_true, target, attribute)
                || expression_contains_declaration(when_false, target, attribute)
        }
        CHirExpr::Call(callee, arguments, _) => {
            expression_contains_declaration(callee, target, attribute)
                || arguments
                    .iter()
                    .any(|argument| expression_contains_declaration(argument, target, attribute))
        }
        CHirExpr::CheckedArithmetic {
            lhs,
            rhs,
            target: destination,
            ..
        } => {
            expression_contains_declaration(lhs, target, attribute)
                || expression_contains_declaration(rhs, target, attribute)
                || expression_contains_declaration(destination, target, attribute)
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            expression_contains_declaration(first, target, attribute)
                || second.is_some_and(|second| {
                    expression_contains_declaration(second, target, attribute)
                })
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            place
                .source_expressions()
                .any(|source| expression_contains_declaration(source, target, attribute))
                || expression_contains_declaration(value, target, attribute)
        }
        CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => place
            .source_expressions()
            .any(|source| expression_contains_declaration(source, target, attribute)),
        CHirExpr::StatementExpr { body, .. } => {
            statement_contains_declaration(body, target, attribute)
        }
        CHirExpr::CompoundLit(_, initializer, _) => {
            initializer
                .plan()
                .actions()
                .iter()
                .any(|action| match action {
                    InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } => {
                        expression_contains_declaration(value, target, attribute)
                    }
                    InitAction::StoreString { .. } | InitAction::ZeroScalar { .. } => false,
                })
        }
        CHirExpr::Lit(_, _)
        | CHirExpr::FloatLit(_, _)
        | CHirExpr::CharLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::Ident { .. }
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(_, _)
        | CHirExpr::AlignOf(_, _)
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => false,
    }
}

pub(crate) fn eval_integer_constant(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> Option<CInteger> {
    match expression {
        CHirExpr::Lit(literal, _) => integer_literal_value(*literal).ok(),
        CHirExpr::CharLit(literal, _) => Some(literal.integer_value()),
        CHirExpr::Ident { symbol, .. } => {
            let symbol = symbols.symbol(*symbol).ok()?;
            if symbol.kind() != SymbolKind::Enumerator {
                return None;
            }
            Some(CInteger::from_raw(
                symbol.enumerator_value()? as u128,
                canonical_integer_type(types, symbol.ty())?,
            ))
        }
        CHirExpr::Unary(operator, operand, _) => eval_constant_unary_integer(
            *operator,
            eval_integer_constant(operand, types, symbols, target)?,
        )
        .ok(),
        CHirExpr::EvaluateOnce(operand, _) => {
            eval_integer_constant(operand, types, symbols, target)
        }
        CHirExpr::Binary(operator, left, right, _) => {
            let left = eval_integer_constant(left, types, symbols, target)?;
            if *operator == BinaryOp::And && !left.truth() {
                return Some(CInteger::int(0));
            }
            if *operator == BinaryOp::Or && left.truth() {
                return Some(CInteger::int(1));
            }
            let right = eval_integer_constant(right, types, symbols, target)?;
            eval_constant_binary_integer(*operator, left, right).ok()
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            let selected = if eval_integer_constant(condition, types, symbols, target)?.truth() {
                when_true
            } else {
                when_false
            };
            eval_integer_constant(selected, types, symbols, target)
        }
        CHirExpr::Elvis(operand, fallback, _) => {
            let operand_value = eval_integer_constant(operand, types, symbols, target)?;
            let selected = if operand_value.truth() {
                operand_value
            } else {
                eval_integer_constant(fallback, types, symbols, target)?
            };
            let operand_type = CInteger::new(0, integer_expression_type(operand, types, symbols)?)
                .promoted()
                .ty();
            let fallback_type =
                CInteger::new(0, integer_expression_type(fallback, types, symbols)?)
                    .promoted()
                    .ty();
            Some(selected.cast(usual_arithmetic_type(operand_type, fallback_type)))
        }
        CHirExpr::Cast(ty, operand, _) => {
            let target_ty = canonical_integer_type(types, *ty)?;
            if let CHirExpr::FloatLit(literal, _) = operand {
                return floating_literal_to_integer(*literal, target_ty).ok();
            }
            Some(eval_integer_constant(operand, types, symbols, target)?.cast(target_ty))
        }
        CHirExpr::SizeOf(ty, _) | CHirExpr::SizeOfExpr(_, ty, _) => Some(CInteger::new(
            target.sizeof_type(types, *ty).ok()?,
            CIntegerType::ULONG,
        )),
        CHirExpr::AlignOf(ty, _) => Some(CInteger::new(
            u64::from(target.alignof_type(types, *ty).ok()?),
            CIntegerType::ULONG,
        )),
        CHirExpr::BitBuiltin { op, operand, .. } => {
            eval_bit_builtin(*op, eval_integer_constant(operand, types, symbols, target)?)
        }
        CHirExpr::FloatLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::Call(_, _, _)
        | CHirExpr::CheckedArithmetic { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. }
        | CHirExpr::BuiltinVarArgs { .. }
        | CHirExpr::Member(_, _, _)
        | CHirExpr::PtrMember(_, _, _)
        | CHirExpr::Index(_, _, _)
        | CHirExpr::Assign { .. }
        | CHirExpr::MemoryAssign { .. }
        | CHirExpr::MemoryRead { .. }
        | CHirExpr::Update { .. }
        | CHirExpr::MemoryUpdate { .. }
        | CHirExpr::StatementExpr { .. }
        | CHirExpr::CompoundLit(_, _, _)
        | CHirExpr::Comma(_, _, _) => None,
    }
}

pub(crate) fn canonical_integer_type(types: &TypeInterner, ty: TypeId) -> Option<CIntegerType> {
    match types.get(ty)?.kind {
        TypeKind::Int(rank) => Some(integer_type_for_rank(rank)),
        TypeKind::GnuInt128(evocc_types::GnuInt128Signedness::Signed) => Some(CIntegerType::INT128),
        TypeKind::GnuInt128(evocc_types::GnuInt128Signedness::Unsigned) => {
            Some(CIntegerType::UINT128)
        }
        TypeKind::Enum(tag) => {
            let definition = types.tag(tag)?;
            let TagState::Complete(TagDefinition::Enum(constants)) = &definition.state else {
                return None;
            };
            let minimum = constants
                .iter()
                .map(|constant| constant.value)
                .min()
                .unwrap_or(0);
            let maximum = constants
                .iter()
                .map(|constant| constant.value)
                .max()
                .unwrap_or(0);
            Some(integer_type_for_rank(enum_integer_rank(
                minimum,
                maximum,
                definition.layout.packed,
            )))
        }
        _ => None,
    }
}

fn integer_expression_type(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
) -> Option<CIntegerType> {
    match expression {
        CHirExpr::Lit(literal, _) => Some(integer_literal_value(*literal).ok()?.ty()),
        CHirExpr::CharLit(literal, _) => Some(literal.integer_value().ty()),
        CHirExpr::Ident { symbol, facts, .. } => {
            let symbol = symbols.symbol(*symbol).ok()?;
            canonical_integer_type(types, facts.ty()).or_else(|| {
                (symbol.kind() == SymbolKind::Enumerator)
                    .then(|| canonical_integer_type(types, symbol.ty()))
                    .flatten()
            })
        }
        CHirExpr::MemoryRead { facts, .. }
        | CHirExpr::CheckedArithmetic { facts, .. }
        | CHirExpr::BitBuiltin { facts, .. }
        | CHirExpr::Assign { facts, .. }
        | CHirExpr::MemoryAssign { facts, .. }
        | CHirExpr::Update { facts, .. }
        | CHirExpr::MemoryUpdate { facts, .. }
        | CHirExpr::StatementExpr { facts, .. } => canonical_integer_type(types, facts.ty()),
        CHirExpr::Unary(operator, operand, _) => match operator {
            crate::UnaryOp::Not => Some(CIntegerType::INT),
            crate::UnaryOp::Plus
            | crate::UnaryOp::Neg
            | crate::UnaryOp::Abs
            | crate::UnaryOp::BitNot => Some(
                CInteger::new(0, integer_expression_type(operand, types, symbols)?)
                    .promoted()
                    .ty(),
            ),
            crate::UnaryOp::AddrOf | crate::UnaryOp::Deref => None,
        },
        CHirExpr::EvaluateOnce(operand, _) => integer_expression_type(operand, types, symbols),
        CHirExpr::Binary(operator, left, right, _) => {
            let left = CInteger::new(0, integer_expression_type(left, types, symbols)?)
                .promoted()
                .ty();
            let right = CInteger::new(0, integer_expression_type(right, types, symbols)?)
                .promoted()
                .ty();
            Some(match operator {
                BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::And
                | BinaryOp::Or => CIntegerType::INT,
                BinaryOp::Shl | BinaryOp::Shr => left,
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Mod
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor => usual_arithmetic_type(left, right),
                BinaryOp::Comma => right,
                BinaryOp::Assign
                | BinaryOp::AddAssign
                | BinaryOp::SubAssign
                | BinaryOp::MulAssign
                | BinaryOp::DivAssign
                | BinaryOp::ModAssign
                | BinaryOp::ShlAssign
                | BinaryOp::ShrAssign
                | BinaryOp::BitAndAssign
                | BinaryOp::BitOrAssign
                | BinaryOp::BitXorAssign => return None,
            })
        }
        CHirExpr::Ternary(_, when_true, when_false, _) => {
            let when_true = CInteger::new(0, integer_expression_type(when_true, types, symbols)?)
                .promoted()
                .ty();
            let when_false = CInteger::new(0, integer_expression_type(when_false, types, symbols)?)
                .promoted()
                .ty();
            Some(usual_arithmetic_type(when_true, when_false))
        }
        CHirExpr::Elvis(operand, fallback, _) => {
            let operand = CInteger::new(0, integer_expression_type(operand, types, symbols)?)
                .promoted()
                .ty();
            let fallback = CInteger::new(0, integer_expression_type(fallback, types, symbols)?)
                .promoted()
                .ty();
            Some(usual_arithmetic_type(operand, fallback))
        }
        CHirExpr::Cast(ty, _, _) => canonical_integer_type(types, *ty),
        CHirExpr::SizeOf(_, _) | CHirExpr::SizeOfExpr(_, _, _) | CHirExpr::AlignOf(_, _) => {
            Some(CIntegerType::ULONG)
        }
        CHirExpr::Comma(_, right, _) => integer_expression_type(right, types, symbols),
        CHirExpr::FloatLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::Call(_, _, _)
        | CHirExpr::Member(_, _, _)
        | CHirExpr::PtrMember(_, _, _)
        | CHirExpr::Index(_, _, _)
        | CHirExpr::CompoundLit(_, _, _) => None,
        CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. }
        | CHirExpr::BuiltinVarArgs { .. } => None,
    }
}
