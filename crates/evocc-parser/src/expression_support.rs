//! Parser expression support.

use super::*;

pub(super) fn merge_qualifiers(left: Qualifiers, right: Qualifiers) -> Qualifiers {
    Qualifiers {
        is_const: left.is_const || right.is_const,
        is_volatile: left.is_volatile || right.is_volatile,
        is_restrict: left.is_restrict || right.is_restrict,
        is_atomic: left.is_atomic || right.is_atomic,
        aligned: match (left.aligned, right.aligned) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        },
    }
}

pub(super) fn enum_range_is_representable(minimum: i128, maximum: i128) -> bool {
    minimum >= i128::from(i64::MIN)
        && if minimum < 0 {
            maximum <= i128::from(i64::MAX)
        } else {
            maximum <= i128::from(u64::MAX)
        }
}

pub(super) fn extend_subobject_path(path: SubobjectPath, step: SubobjectStep) -> SubobjectPath {
    match step {
        SubobjectStep::Field { field_index } => path.with_field(field_index),
        SubobjectStep::Index { index } => path.with_index(index),
    }
}

pub(super) fn is_integer_class(class: OperandTypeClass) -> bool {
    matches!(
        class,
        OperandTypeClass::Integer(_) | OperandTypeClass::WideInteger { .. }
    )
}

pub(super) fn is_arithmetic_class(class: OperandTypeClass) -> bool {
    matches!(
        class,
        OperandTypeClass::Integer(_)
            | OperandTypeClass::WideInteger { .. }
            | OperandTypeClass::Real(_)
            | OperandTypeClass::Complex(_)
    )
}

pub(super) fn is_scalar_class(class: OperandTypeClass) -> bool {
    is_arithmetic_class(class)
        || matches!(
            class,
            OperandTypeClass::Pointer(_) | OperandTypeClass::StringPointer(_)
        )
}

pub(super) fn floating_kind(class: OperandTypeClass) -> Option<FloatKind> {
    match class {
        OperandTypeClass::Real(kind) => Some(kind),
        OperandTypeClass::Complex(base) => Some(match base {
            ComplexBase::Integer(_) | ComplexBase::Interchange(_) => return None,
            ComplexBase::CF16 => FloatKind::F16,
            ComplexBase::CF32 => FloatKind::F32,
            ComplexBase::CF64 => FloatKind::F64,
            ComplexBase::CF128 => FloatKind::F128,
        }),
        OperandTypeClass::Integer(_)
        | OperandTypeClass::WideInteger { .. }
        | OperandTypeClass::Pointer(_)
        | OperandTypeClass::StringPointer(_)
        | OperandTypeClass::Aggregate(_)
        | OperandTypeClass::Void
        | OperandTypeClass::Other => None,
    }
}

/// The GNU builtin namespaces. A call of an undeclared name in them is
/// never an implicit declaration.
pub(super) fn reserved_builtin_spelling(spelling: &str) -> bool {
    ["__builtin_", "__atomic_", "__sync_"]
        .iter()
        .any(|prefix| spelling.starts_with(prefix))
}

/// Whether a constant operand keeps its value through substitution: it
/// contains no object-size or constant query that the front end deferred.
pub(super) fn settled_constant_operand(expression: &CHirExpr<'_>) -> bool {
    match expression {
        CHirExpr::Lit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::SizeOf(..)
        | CHirExpr::SizeOfExpr(..)
        | CHirExpr::AlignOf(..) => true,
        CHirExpr::Cast(_, operand, _)
        | CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _) => settled_constant_operand(operand),
        CHirExpr::Binary(_, left, right, _) | CHirExpr::Comma(left, right, _) => {
            settled_constant_operand(left) && settled_constant_operand(right)
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            settled_constant_operand(condition)
                && settled_constant_operand(when_true)
                && settled_constant_operand(when_false)
        }
        _ => false,
    }
}

pub(super) fn implicit_function_name(spelling: &str) -> Option<&'static str> {
    matches!(
        spelling,
        "__func__" | "__FUNCTION__" | "__PRETTY_FUNCTION__"
    )
    .then_some("__func__")
}

pub(super) fn floating_rank(kind: FloatKind) -> u8 {
    match kind {
        FloatKind::F16 | FloatKind::BF16 => 0,
        FloatKind::F32 => 1,
        FloatKind::F64 => 2,
        FloatKind::F128 => 3,
    }
}

/// The arithmetic class of a half type (EvoIR 3.13.6). `_Float16` and
/// `__bf16` keep their own type, which ranks below `float`. An `__fp16`
/// operand promotes to `float` before every operator, as ACLE requires.
pub(super) fn half_operand_class(kind: TargetBuiltinType) -> Option<OperandTypeClass> {
    match kind {
        TargetBuiltinType::GnuFloat16 => Some(OperandTypeClass::Real(FloatKind::F16)),
        TargetBuiltinType::ArmBf16 => Some(OperandTypeClass::Real(FloatKind::BF16)),
        TargetBuiltinType::ArmFp16 => Some(OperandTypeClass::Real(FloatKind::F32)),
        _ => None,
    }
}

/// Whether two operand classes mix the two half formats. Neither value set
/// is a subset of the other, so they have no common type (C23 H.4.2 makes
/// the analogous mixture undefined).
pub(super) fn mixes_half_formats(left: OperandTypeClass, right: OperandTypeClass) -> bool {
    matches!(
        (left, right),
        (
            OperandTypeClass::Real(FloatKind::F16),
            OperandTypeClass::Real(FloatKind::BF16)
        ) | (
            OperandTypeClass::Real(FloatKind::BF16),
            OperandTypeClass::Real(FloatKind::F16)
        )
    )
}

pub(super) fn widest_floating_kind(
    left: OperandTypeClass,
    right: OperandTypeClass,
) -> Option<FloatKind> {
    if mixes_half_formats(left, right) {
        return None;
    }
    match (floating_kind(left), floating_kind(right)) {
        (Some(left), Some(right)) => Some(if floating_rank(left) >= floating_rank(right) {
            left
        } else {
            right
        }),
        (Some(kind), None) | (None, Some(kind)) => Some(kind),
        (None, None) => None,
    }
}

/// The operand class of a complex type. Complex types over `_FloatN`,
/// `_FloatNx` and half formats have no admitted value operation, as their
/// real types have none.
pub(super) fn complex_class(complex: ComplexBase) -> OperandTypeClass {
    match complex {
        ComplexBase::Interchange(_) | ComplexBase::CF16 => OperandTypeClass::Other,
        complex => OperandTypeClass::Complex(complex),
    }
}

pub(super) fn complex_base_for_float(kind: FloatKind) -> ComplexBase {
    match kind {
        FloatKind::F16 | FloatKind::BF16 => ComplexBase::CF16,
        FloatKind::F32 => ComplexBase::CF32,
        FloatKind::F64 => ComplexBase::CF64,
        FloatKind::F128 => ComplexBase::CF128,
    }
}

pub(super) fn drops_qualifiers(source: Qualifiers, target: Qualifiers) -> bool {
    (source.is_const && !target.is_const)
        || (source.is_volatile && !target.is_volatile)
        || (source.is_restrict && !target.is_restrict)
}

pub(super) fn qualifiers_include(target: Qualifiers, source: Qualifiers) -> bool {
    (!source.is_const || target.is_const)
        && (!source.is_volatile || target.is_volatile)
        && (!source.is_restrict || target.is_restrict)
        && (!source.is_atomic || target.is_atomic)
}

pub(super) fn is_object_or_function_type_kind(kind: &TypeKind) -> bool {
    !matches!(kind, TypeKind::Void)
}

// ── precedence table ───────────────────────────────────────────────────────

pub(super) fn prec(kind: TokenKind) -> u8 {
    match kind {
        TokenKind::Star | TokenKind::Slash | TokenKind::Percent => 11,
        TokenKind::Plus | TokenKind::Minus => 10,
        TokenKind::ShiftLeft | TokenKind::ShiftRight => 9,
        TokenKind::Less | TokenKind::LessEq | TokenKind::Greater | TokenKind::GreaterEq => 8,
        TokenKind::EqEq | TokenKind::NotEq => 7,
        TokenKind::Amp => 6,
        TokenKind::Caret => 5,
        TokenKind::Pipe => 4,
        TokenKind::AmpAmp => 3,
        TokenKind::PipePipe => 2,
        _ => 0,
    }
}

pub(super) fn binary_op(kind: TokenKind) -> Option<BinaryOp> {
    match kind {
        TokenKind::Plus => Some(BinaryOp::Add),
        TokenKind::Minus => Some(BinaryOp::Sub),
        TokenKind::Star => Some(BinaryOp::Mul),
        TokenKind::Slash => Some(BinaryOp::Div),
        TokenKind::Percent => Some(BinaryOp::Mod),
        TokenKind::Less => Some(BinaryOp::Lt),
        TokenKind::LessEq => Some(BinaryOp::Le),
        TokenKind::Greater => Some(BinaryOp::Gt),
        TokenKind::GreaterEq => Some(BinaryOp::Ge),
        TokenKind::EqEq => Some(BinaryOp::Eq),
        TokenKind::NotEq => Some(BinaryOp::Ne),
        TokenKind::ShiftLeft => Some(BinaryOp::Shl),
        TokenKind::ShiftRight => Some(BinaryOp::Shr),
        TokenKind::Amp => Some(BinaryOp::BitAnd),
        TokenKind::Caret => Some(BinaryOp::BitXor),
        TokenKind::Pipe => Some(BinaryOp::BitOr),
        TokenKind::AmpAmp => Some(BinaryOp::And),
        TokenKind::PipePipe => Some(BinaryOp::Or),
        _ => None,
    }
}

pub(super) fn assignment_op(kind: TokenKind) -> Option<AssignmentOp> {
    match kind {
        TokenKind::Assign => Some(AssignmentOp::Simple),
        TokenKind::PlusEq => Some(AssignmentOp::Add),
        TokenKind::MinusEq => Some(AssignmentOp::Subtract),
        TokenKind::StarEq => Some(AssignmentOp::Multiply),
        TokenKind::SlashEq => Some(AssignmentOp::Divide),
        TokenKind::PercentEq => Some(AssignmentOp::Remainder),
        TokenKind::ShiftLeftEq => Some(AssignmentOp::ShiftLeft),
        TokenKind::ShiftRightEq => Some(AssignmentOp::ShiftRight),
        TokenKind::AmpEq => Some(AssignmentOp::BitAnd),
        TokenKind::CaretEq => Some(AssignmentOp::BitXor),
        TokenKind::PipeEq => Some(AssignmentOp::BitOr),
        _ => None,
    }
}

pub(super) fn token_spelling(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Identifier => "identifier",
        TokenKind::While => "while",
        TokenKind::Semicolon => ";",
        TokenKind::RParen => ")",
        TokenKind::RBracket => "]",
        TokenKind::RBrace => "}",
        TokenKind::LParen => "(",
        TokenKind::Colon => ":",
        _ => "token",
    }
}

pub(super) fn misplaced_statement_diagnostic(
    kind: Option<TokenKind>,
) -> Option<(&'static str, &'static str)> {
    match kind {
        Some(TokenKind::Break) => Some(("E1201", "break-outside-loop-or-switch")),
        Some(TokenKind::Continue) => Some(("E1202", "continue-outside-loop")),
        Some(TokenKind::Case) => Some(("E1203", "case-outside-switch")),
        Some(TokenKind::Default) => Some(("E1204", "default-outside-switch")),
        _ => None,
    }
}

pub(super) fn can_start_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::GnuExtension
            | TokenKind::IntLiteral
            | TokenKind::ImaginaryIntLiteral
            | TokenKind::FloatLiteral
            | TokenKind::ImaginaryFloatLiteral
            | TokenKind::CharLiteral
            | TokenKind::StringLiteral
            | TokenKind::LParen
            | TokenKind::Minus
            | TokenKind::Plus
            | TokenKind::Bang
            | TokenKind::Tilde
            | TokenKind::Amp
            | TokenKind::AmpAmp
            | TokenKind::Star
            | TokenKind::PlusPlus
            | TokenKind::MinusMinus
            | TokenKind::Sizeof
            | TokenKind::Alignof
            | TokenKind::GnuAlignof
            | TokenKind::BuiltinExpect
            | TokenKind::BuiltinAtomic
            | TokenKind::BuiltinConstantP
            | TokenKind::BuiltinChooseExpr
            | TokenKind::BuiltinTypesCompatibleP
            | TokenKind::BuiltinClassifyType
            | TokenKind::BuiltinOffsetof
            | TokenKind::BuiltinObjectSize
            | TokenKind::BuiltinHasAttribute
            | TokenKind::BuiltinStrlen
            | TokenKind::BuiltinLibrary
            | TokenKind::BuiltinCtype
            | TokenKind::BuiltinVaStart
            | TokenKind::BuiltinVaEnd
            | TokenKind::BuiltinVaCopy
            | TokenKind::BuiltinVaArg
            | TokenKind::BuiltinBit
            | TokenKind::BuiltinUnreachable
            | TokenKind::BuiltinTrap
            | TokenKind::BuiltinReturnAddress
            | TokenKind::BuiltinFrameAddress
            | TokenKind::BuiltinAddOverflow
            | TokenKind::BuiltinSubOverflow
            | TokenKind::BuiltinMulOverflow
            | TokenKind::Generic
    )
}

pub(super) fn written_initializer_references_symbol(
    initializer: &WrittenInitializer<'_>,
    symbol: SymbolId,
) -> bool {
    match initializer {
        WrittenInitializer::Expression(expression) => {
            expression_references_symbol(expression, symbol)
        }
        WrittenInitializer::List {
            elements,
            designations,
            ..
        } => {
            elements
                .iter()
                .any(|element| written_initializer_references_symbol(element, symbol))
                || designations.iter().flatten().any(|designation| {
                    designation
                        .designators
                        .iter()
                        .any(|designator| match designator {
                            WrittenDesignator::Field(_) => false,
                            WrittenDesignator::Index { expression, .. } => {
                                expression_references_symbol(expression, symbol)
                            }
                            WrittenDesignator::Range { first, last, .. } => {
                                expression_references_symbol(first, symbol)
                                    || expression_references_symbol(last, symbol)
                            }
                        })
                })
        }
    }
}

pub(super) fn declaration_references_symbol(declaration: &CHirDecl<'_>, symbol: SymbolId) -> bool {
    match declaration {
        CHirDecl::Asm(assembly) => assembly
            .outputs
            .iter()
            .chain(&assembly.inputs)
            .any(|operand| expression_references_symbol(operand.expression, symbol)),
        CHirDecl::Object(object) => object.initializer.as_ref().is_some_and(|initializer| {
            written_initializer_references_symbol(initializer.written(), symbol)
        }),
        CHirDecl::Function(function) => function
            .body
            .is_some_and(|body| statement_references_symbol(body, symbol)),
        CHirDecl::Tag(tag) => match &tag.members {
            TagMembers::Incomplete => false,
            TagMembers::Struct(members) | TagMembers::Union(members) => {
                members.iter().any(|member| {
                    member
                        .bit_width
                        .is_some_and(|width| expression_references_symbol(width, symbol))
                })
            }
            TagMembers::Enum(enumerators) => enumerators.iter().any(|enumerator| {
                enumerator
                    .written_value
                    .is_some_and(|value| expression_references_symbol(value, symbol))
            }),
        },
        CHirDecl::Typedef(_) => false,
    }
}

pub(super) fn statement_references_symbol(statement: &CHirStmt<'_>, symbol: SymbolId) -> bool {
    match statement {
        CHirStmt::Asm(assembly) => assembly
            .outputs
            .iter()
            .chain(&assembly.inputs)
            .any(|operand| expression_references_symbol(operand.expression, symbol)),
        CHirStmt::Expr(expression, _) => expression_references_symbol(expression, symbol),
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            expression_references_symbol(condition, symbol)
                || statement_references_symbol(then_statement, symbol)
                || else_statement
                    .is_some_and(|statement| statement_references_symbol(statement, symbol))
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            expression_references_symbol(expression, symbol)
                || statement_references_symbol(body, symbol)
        }
        CHirStmt::DoWhile(body, condition, _) => {
            statement_references_symbol(body, symbol)
                || expression_references_symbol(condition, symbol)
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            let initializer_references = match initializer {
                CHirForInit::Empty => false,
                CHirForInit::Expression(expression) => {
                    expression_references_symbol(expression, symbol)
                }
                CHirForInit::Declaration(declarations) => declarations
                    .iter()
                    .any(|declaration| declaration_references_symbol(declaration, symbol)),
            };
            initializer_references
                || condition
                    .is_some_and(|expression| expression_references_symbol(expression, symbol))
                || step.is_some_and(|expression| expression_references_symbol(expression, symbol))
                || statement_references_symbol(body, symbol)
        }
        CHirStmt::Return(expression, _) => {
            expression.is_some_and(|expression| expression_references_symbol(expression, symbol))
        }
        CHirStmt::ComputedGoto(target, _) => expression_references_symbol(target, symbol),
        CHirStmt::Compound(items, _, _) => items.iter().any(|item| match item {
            CHirBlockItem::Declaration(declaration) => {
                declaration_references_symbol(declaration, symbol)
            }
            CHirBlockItem::Statement(statement) => statement_references_symbol(statement, symbol),
        }),
        CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => {
            statement_references_symbol(body, symbol)
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            expression_references_symbol(first, symbol)
                || expression_references_symbol(last, symbol)
                || statement_references_symbol(body, symbol)
        }
        CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => false,
    }
}

pub(super) fn place_references_symbol(place: &MemoryPlace<'_>, symbol: SymbolId) -> bool {
    place.root_symbol() == Some(symbol)
        || matches!(
            place.root(),
            PlaceRoot::CompoundLiteral { expression, .. }
                if expression_references_symbol(expression, symbol)
        )
        || place
            .source_expressions()
            .any(|expression| expression_references_symbol(expression, symbol))
}

pub(super) fn expression_references_symbol(expression: &CHirExpr<'_>, symbol: SymbolId) -> bool {
    match expression {
        CHirExpr::Complex(operation) => operation
            .operands
            .iter()
            .any(|operand| expression_references_symbol(operand, symbol)),
        CHirExpr::Floating(operation) => operation
            .operands
            .iter()
            .any(|operand| expression_references_symbol(operand, symbol)),
        CHirExpr::Vector(operation) => operation
            .operands
            .iter()
            .any(|operand| expression_references_symbol(operand, symbol)),
        CHirExpr::Atomic(access) => access
            .operands()
            .any(|operand| expression_references_symbol(operand, symbol)),
        CHirExpr::BuiltinConstantP { operand, .. }
        | CHirExpr::BuiltinObjectSize { operand, .. } => {
            expression_references_symbol(operand, symbol)
        }
        CHirExpr::Ident {
            symbol: referenced, ..
        } => *referenced == symbol,
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::SizeOfExpr(operand, _, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => expression_references_symbol(operand, symbol),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            expression_references_symbol(left, symbol)
                || expression_references_symbol(right, symbol)
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            expression_references_symbol(condition, symbol)
                || expression_references_symbol(when_true, symbol)
                || expression_references_symbol(when_false, symbol)
        }
        CHirExpr::Call(callee, arguments, _) => {
            expression_references_symbol(callee, symbol)
                || arguments
                    .iter()
                    .any(|argument| expression_references_symbol(argument, symbol))
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            expression_references_symbol(lhs, symbol)
                || expression_references_symbol(rhs, symbol)
                || expression_references_symbol(target, symbol)
        }
        CHirExpr::BuiltinVarArgPack { .. } => false,
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            expression_references_symbol(first, symbol)
                || second.is_some_and(|second| expression_references_symbol(second, symbol))
        }
        CHirExpr::Assign { target, value, .. } => {
            expression_references_symbol(target, symbol)
                || expression_references_symbol(value, symbol)
        }
        CHirExpr::Update { target, .. } => expression_references_symbol(target, symbol),
        CHirExpr::MemoryAssign { place, value, .. } => {
            place_references_symbol(place, symbol) || expression_references_symbol(value, symbol)
        }
        CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
            place_references_symbol(place, symbol)
        }
        CHirExpr::StatementExpr { body, .. } => statement_references_symbol(body, symbol),
        CHirExpr::CompoundLit(_, initializer, _) => {
            written_initializer_references_symbol(initializer.written(), symbol)
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => false,
    }
}

// ── tests ──────────────────────────────────────────────────────────────────
