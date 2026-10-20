//! Accesses of `_Atomic` objects that the front end refuses (EvoIR 10.1.21).
//! A read of an admitted atomic scalar is a `seq_cst` atomic load in every
//! consumer. A read of any other atomic object is `E2702`, as an update is.
//! A read has no node of its own, because an lvalue that a consumer reads as
//! a value stays an lvalue in C-HIR, so this walk finds each read. It also
//! finds each evaluated member access of an atomic record or union, which
//! C11 6.5.2.3p5 makes undefined.

use evocc_types::{FloatKind, TargetDataLayout, TypeId, TypeInterner, TypeKind};

use crate::{
    CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, ExprCategory, InitAction,
    MemoryPlace, ObjectInitializer, PointerTypeDesc, SourceSpan, SymbolTable, UnaryOp,
};

/// One refused access of an `_Atomic` object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AtomicFinding {
    /// A read of an atomic object outside the admitted scalars (`E2702`).
    UnadmittedRead(SourceSpan),
    /// An evaluated member access of an atomic record or union (`E2956`).
    MemberAccess(SourceSpan),
}

/// Whether loads and stores of an `_Atomic` object of type `ty` are
/// admitted: an integer, `_Bool`, enumeration, pointer, binary32 or binary64
/// object of 1, 2, 4 or 8 bytes at an alignment of at least its size.
#[must_use]
pub fn atomic_access_admitted(types: &TypeInterner, target: &TargetDataLayout, ty: TypeId) -> bool {
    let Some(canonical) = types.get(ty) else {
        return false;
    };
    let scalar = match canonical.kind {
        TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::Pointer(_) => true,
        TypeKind::Float(kind) => {
            matches!(target.float_format(kind), FloatKind::F32 | FloatKind::F64)
        }
        _ => false,
    };
    scalar
        && target.layout_of(types, ty).is_ok_and(|layout| {
            matches!(layout.size_bytes, 1 | 2 | 4 | 8)
                && u64::from(layout.align_bytes) >= layout.size_bytes
        })
}

/// The refused atomic accesses of `statement`, in source order. Operands
/// that C does not evaluate (`sizeof`, the queries of `__builtin_constant_p`
/// and `__builtin_object_size`) are skipped. Operands that C designates
/// without a read (`&`, the target of an assignment or an update, the base
/// of a member) are not reads, but a member access among them is reported.
#[must_use]
pub fn atomic_access_findings(
    statement: &CHirStmt<'_>,
    symbols: &SymbolTable,
    types: &TypeInterner,
    target: &TargetDataLayout,
) -> Vec<AtomicFinding> {
    let mut walk = Walk {
        symbols,
        types,
        target,
        findings: Vec::new(),
    };
    walk.statement(statement);
    walk.findings
}

struct Walk<'t> {
    symbols: &'t SymbolTable,
    types: &'t TypeInterner,
    target: &'t TargetDataLayout,
    findings: Vec<AtomicFinding>,
}

impl Walk<'_> {
    /// An atomic-qualified object type whose read is refused.
    fn unadmitted_atomic_object(&self, ty: TypeId) -> bool {
        self.types.get(ty).is_some_and(|canonical| {
            canonical.qualifiers.is_atomic
                && !matches!(
                    canonical.kind,
                    TypeKind::Array { .. } | TypeKind::Function { .. }
                )
                && !atomic_access_admitted(self.types, self.target, ty)
        })
    }

    fn read(&mut self, span: SourceSpan) {
        self.findings.push(AtomicFinding::UnadmittedRead(span));
    }

    /// Report a member access of an atomic record and walk the place's
    /// source expressions.
    fn place(&mut self, place: &MemoryPlace<'_>, span: SourceSpan) {
        if place.accesses_atomic_member() {
            self.findings.push(AtomicFinding::MemberAccess(span));
        }
        for source in place.source_expressions() {
            self.expression(source, true);
        }
    }

    fn statement(&mut self, statement: &CHirStmt<'_>) {
        match statement {
            CHirStmt::Asm(assembly) => {
                for operand in &assembly.outputs {
                    self.expression(operand.expression, false);
                }
                for operand in &assembly.inputs {
                    self.expression(operand.expression, true);
                }
            }
            CHirStmt::Expr(expression, _)
            | CHirStmt::Return(Some(expression), _)
            | CHirStmt::ComputedGoto(expression, _) => self.expression(expression, true),
            CHirStmt::If(condition, then_branch, else_branch, _) => {
                self.expression(condition, true);
                self.statement(then_branch);
                if let Some(branch) = else_branch {
                    self.statement(branch);
                }
            }
            CHirStmt::Switch(expression, body, _)
            | CHirStmt::While(expression, body, _)
            | CHirStmt::Case(expression, body, _) => {
                self.expression(expression, true);
                self.statement(body);
            }
            CHirStmt::CaseRange(first, last, body, _) => {
                self.expression(first, true);
                self.expression(last, true);
                self.statement(body);
            }
            CHirStmt::DoWhile(body, expression, _) => {
                self.statement(body);
                self.expression(expression, true);
            }
            CHirStmt::For(initializer, condition, step, body, _) => {
                match initializer {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(expression) => self.expression(expression, true),
                    CHirForInit::Declaration(declarations) => {
                        for declaration in declarations {
                            self.declaration(declaration);
                        }
                    }
                }
                if let Some(condition) = condition {
                    self.expression(condition, true);
                }
                if let Some(step) = step {
                    self.expression(step, true);
                }
                self.statement(body);
            }
            CHirStmt::Compound(items, _, _) => {
                for item in items {
                    match item {
                        CHirBlockItem::Declaration(declaration) => self.declaration(declaration),
                        CHirBlockItem::Statement(statement) => self.statement(statement),
                    }
                }
            }
            CHirStmt::Label(_, statement, _) | CHirStmt::Default(statement, _) => {
                self.statement(statement)
            }
            CHirStmt::Return(None, _)
            | CHirStmt::Goto(..)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::AttributedNull(_, _)
            | CHirStmt::Null(_) => {}
        }
    }

    fn declaration(&mut self, declaration: &CHirDecl<'_>) {
        if let CHirDecl::Object(object) = declaration {
            if let Some(initializer) = &object.initializer {
                self.initializer(initializer);
            }
            if let Some(cleanup) = object.cleanup {
                self.expression(cleanup, true);
            }
        }
    }

    fn initializer(&mut self, initializer: &ObjectInitializer<'_>) {
        for action in initializer.plan.actions() {
            if let InitAction::StoreScalar { value, .. } | InitAction::CopyAggregate { value, .. } =
                action
            {
                self.expression(value, true);
            }
        }
    }

    /// Walk `expression`; `read` says whether its value is read.
    fn expression(&mut self, expression: &CHirExpr<'_>, read: bool) {
        // A member or element that stays an aggregate expression is read
        // without a place, so its own type decides.
        if read
            && matches!(
                expression,
                CHirExpr::Member(..) | CHirExpr::PtrMember(..) | CHirExpr::Index(..)
            )
            && crate::vector::expression_type(self.types, expression)
                .is_some_and(|ty| self.unadmitted_atomic_object(ty))
        {
            self.read(expression.span());
        }
        match expression {
            CHirExpr::Ident { facts, span, .. } => {
                if read
                    && facts.category() == ExprCategory::LValue
                    && self.unadmitted_atomic_object(facts.ty())
                {
                    self.read(*span);
                }
            }
            CHirExpr::MemoryRead {
                place, target_span, ..
            } => {
                if read
                    && place.access_qualifiers().is_atomic
                    && !place.accesses_atomic_member()
                    && self.unadmitted_atomic_object(place.value_ty())
                {
                    self.read(*target_span);
                }
                self.place(place, *target_span);
            }
            CHirExpr::MemoryAssign {
                place,
                target_span,
                value,
                ..
            } => {
                self.place(place, *target_span);
                self.expression(value, true);
            }
            CHirExpr::MemoryUpdate {
                place, target_span, ..
            } => self.place(place, *target_span),
            CHirExpr::Assign { value, .. } => self.expression(value, true),
            CHirExpr::Update { .. } => {}
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => self.expression(operand, false),
            CHirExpr::Unary(UnaryOp::Deref, operand, span) => {
                if read
                    && PointerTypeDesc::from_expression(self.symbols, self.types, operand)
                        .is_ok_and(|pointer| self.unadmitted_atomic_object(pointer.pointee()))
                {
                    self.read(*span);
                }
                self.expression(operand, true);
            }
            CHirExpr::PtrMember(operand, _, span) => {
                let atomic = crate::vector::expression_type(self.types, operand)
                    .and_then(|pointer| match self.types.get(pointer)?.kind {
                        TypeKind::Pointer(pointee) => self.types.get(pointee),
                        _ => None,
                    })
                    .is_some_and(|record| record.qualifiers.is_atomic);
                if atomic {
                    self.findings.push(AtomicFinding::MemberAccess(*span));
                }
                self.expression(operand, true);
            }
            // A member read reads the member, not its whole base.
            CHirExpr::Member(base, _, span) => {
                let atomic = crate::vector::expression_type(self.types, base)
                    .and_then(|record| self.types.get(record))
                    .is_some_and(|record| record.qualifiers.is_atomic);
                if atomic {
                    self.findings.push(AtomicFinding::MemberAccess(*span));
                }
                self.expression(base, false);
            }
            CHirExpr::Unary(_, operand, _)
            | CHirExpr::Cast(_, operand, _)
            | CHirExpr::EvaluateOnce(operand, _)
            | CHirExpr::BitBuiltin { operand, .. } => self.expression(operand, true),
            CHirExpr::Index(base, index, _) => {
                let array = match base {
                    CHirExpr::Ident { facts, .. } => self
                        .types
                        .get(facts.ty())
                        .is_some_and(|ty| matches!(ty.kind, TypeKind::Array { .. })),
                    _ => false,
                };
                self.expression(base, !array);
                self.expression(index, true);
            }
            CHirExpr::Binary(_, left, right, _)
            | CHirExpr::Comma(left, right, _)
            | CHirExpr::Elvis(left, right, _)
            | CHirExpr::BuiltinExpect(left, right, _) => {
                self.expression(left, true);
                self.expression(right, true);
            }
            CHirExpr::Ternary(condition, left, right, _) => {
                self.expression(condition, true);
                self.expression(left, true);
                self.expression(right, true);
            }
            CHirExpr::Call(callee, arguments, _) => {
                self.expression(callee, true);
                for argument in arguments {
                    self.expression(argument, true);
                }
            }
            CHirExpr::CheckedArithmetic {
                lhs, rhs, target, ..
            } => {
                self.expression(lhs, true);
                self.expression(rhs, true);
                self.expression(target, true);
            }
            CHirExpr::BuiltinVarArgs { first, second, .. } => {
                self.expression(first, false);
                if let Some(second) = second {
                    self.expression(second, false);
                }
            }
            CHirExpr::Floating(operation) => {
                for operand in &operation.operands {
                    self.expression(operand, true);
                }
            }
            CHirExpr::Complex(operation) => {
                for operand in &operation.operands {
                    self.expression(operand, true);
                }
            }
            CHirExpr::Vector(operation) => {
                for operand in &operation.operands {
                    self.expression(operand, true);
                }
            }
            CHirExpr::Atomic(access) => {
                for operand in access.operands() {
                    self.expression(operand, true);
                }
            }
            CHirExpr::StatementExpr { body, value, .. } => {
                self.statement(body);
                if let Some(value) = value {
                    self.expression(value, read);
                }
            }
            CHirExpr::CompoundLit(_, initializer, _) => self.initializer(initializer),
            // Operands that C does not evaluate.
            CHirExpr::SizeOfExpr(..)
            | CHirExpr::BuiltinConstantP { .. }
            | CHirExpr::BuiltinObjectSize { .. } => {}
            CHirExpr::Lit(..)
            | CHirExpr::FloatLit(..)
            | CHirExpr::CharLit(..)
            | CHirExpr::StringLit(..)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::SizeOf(..)
            | CHirExpr::AlignOf(..)
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinTrap { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { .. }
            | CHirExpr::BuiltinVarArgPack { .. } => {}
        }
    }
}
