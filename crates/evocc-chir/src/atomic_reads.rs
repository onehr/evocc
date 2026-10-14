//! Reads of `_Atomic` objects. EvoIR Section 10.1.3 keeps `_Atomic` objects
//! outside the profile (`E2702`). An assignment or an update is refused where
//! the parser builds it; a read has no node of its own, because an lvalue
//! that a consumer reads as a value stays an lvalue in C-HIR. This walk finds
//! each such read, so that no read becomes a plain load.

use evocc_types::{TypeInterner, TypeKind};

use crate::{
    CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, ExprCategory, InitAction,
    ObjectInitializer, PointerTypeDesc, SourceSpan, SymbolTable, UnaryOp,
};

/// The spans where `statement` reads an `_Atomic` object as a value, in
/// source order. Operands that C does not evaluate (`sizeof`, the queries of
/// `__builtin_constant_p` and `__builtin_object_size`) and operands that C
/// designates without a read (`&`, the target of an assignment or an
/// update, the base of a member) are not reads.
#[must_use]
pub fn atomic_object_reads(
    statement: &CHirStmt<'_>,
    symbols: &SymbolTable,
    types: &TypeInterner,
) -> Vec<SourceSpan> {
    let mut walk = Walk {
        symbols,
        types,
        reads: Vec::new(),
    };
    walk.statement(statement);
    walk.reads
}

struct Walk<'t> {
    symbols: &'t SymbolTable,
    types: &'t TypeInterner,
    reads: Vec<SourceSpan>,
}

impl Walk<'_> {
    /// An atomic-qualified object type whose lvalue a read converts.
    fn atomic_object(&self, ty: evocc_types::TypeId) -> bool {
        self.types.get(ty).is_some_and(|canonical| {
            canonical.qualifiers.is_atomic
                && !matches!(
                    canonical.kind,
                    TypeKind::Array { .. } | TypeKind::Function { .. }
                )
        })
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
        match expression {
            CHirExpr::Ident { facts, span, .. } => {
                if read
                    && facts.category() == ExprCategory::LValue
                    && self.atomic_object(facts.ty())
                {
                    self.reads.push(*span);
                }
            }
            CHirExpr::MemoryRead {
                place, target_span, ..
            } => {
                if read
                    && place.access_qualifiers().is_atomic
                    && self.atomic_object(place.value_ty())
                {
                    self.reads.push(*target_span);
                }
                for source in place.source_expressions() {
                    self.expression(source, true);
                }
            }
            CHirExpr::MemoryAssign { place, value, .. } => {
                for source in place.source_expressions() {
                    self.expression(source, true);
                }
                self.expression(value, true);
            }
            CHirExpr::MemoryUpdate { place, .. } => {
                for source in place.source_expressions() {
                    self.expression(source, true);
                }
            }
            CHirExpr::Assign { value, .. } => self.expression(value, true),
            CHirExpr::Update { .. } => {}
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => self.expression(operand, false),
            CHirExpr::Unary(UnaryOp::Deref, operand, span) => {
                if read
                    && PointerTypeDesc::from_expression(self.symbols, self.types, operand)
                        .is_ok_and(|pointer| self.atomic_object(pointer.pointee()))
                {
                    self.reads.push(*span);
                }
                self.expression(operand, true);
            }
            CHirExpr::Unary(_, operand, _)
            | CHirExpr::Cast(_, operand, _)
            | CHirExpr::EvaluateOnce(operand, _)
            | CHirExpr::BitBuiltin { operand, .. }
            | CHirExpr::PtrMember(operand, _, _) => self.expression(operand, true),
            // A member read reads the member, not its whole base.
            CHirExpr::Member(base, _, _) => self.expression(base, false),
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
