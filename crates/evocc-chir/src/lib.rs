#![allow(dead_code)]
#![allow(clippy::collapsible_else_if, clippy::approx_constant)]
//! C High-Level IR: typed AST preserving C source semantics.
//!
//! Every node in the C-HIR carries a [`SourceSpan`] so that downstream passes
//! (type-checking, lowering, diagnostics) can attach source locations to
//! their results without threading a side table.
//!
//! The IR is comprehensive but stubbed: no parser or semantic analysis lives
//! here. This crate is purely the **type definitions** that the parser and
//! semantic analyser will populate.

#![doc = concat!("evocc-chir v", env!("CARGO_PKG_VERSION"))]

pub mod initializer;
pub mod semantic;

pub use initializer::{
    InitAction, InitPlan, InitPlanError, SubobjectPath, SubobjectStep, WrittenInitializer,
};
pub use semantic::{
    AssignmentOp, ExprCategory, ExprFacts, ExprFactsError, NotAssignmentOperator, ScalarPlace,
    ScalarPlaceError, Scope, ScopeId, ScopeKind, Symbol, SymbolData, SymbolId, SymbolKind,
    SymbolTable, SymbolTableCheckpoint, SymbolTableError, TagBinding,
};

use evocc_common::SourceSpan;
use evocc_types::{ArrayBound, Qualifiers, TagId, TagKind, TypeId, TypeInterner};

// ---------------------------------------------------------------------------
// Literal helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntLit {
    pub value: u64,
    pub suffix: IntSuffix,
    pub base: IntBase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntSuffix {
    None,
    U,
    L,
    UL,
    LL,
    ULL,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntBase {
    Dec,
    Hex,
    Oct,
    Bin,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatLit {
    pub value: f64,
    pub suffix: FloatSuffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatSuffix {
    None,
    F,
    L,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CharLit {
    pub value: u32,
    pub prefix: CharPrefix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharPrefix {
    None,
    L,
    U,
    U8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StringLit {
    pub value: String,
    pub prefix: StringPrefix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringPrefix {
    None,
    L,
    U,
    U8,
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
    AddrOf,
    Deref,
    PreInc,
    PreDec,
    PostInc,
    PostDec,
    Plus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Comma,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    ShlAssign,
    ShrAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
}

// ---------------------------------------------------------------------------
// CHirExpr — C expression nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CHirExpr<'arena> {
    Lit(IntLit, SourceSpan),
    FloatLit(FloatLit, SourceSpan),
    CharLit(CharLit, SourceSpan),
    StringLit(StringLit, SourceSpan),
    Ident {
        symbol: SymbolId,
        spelling: String,
        facts: ExprFacts,
        span: SourceSpan,
    },
    Unary(UnaryOp, &'arena CHirExpr<'arena>, SourceSpan),
    Binary(
        BinaryOp,
        &'arena CHirExpr<'arena>,
        &'arena CHirExpr<'arena>,
        SourceSpan,
    ),
    Ternary(
        &'arena CHirExpr<'arena>,
        &'arena CHirExpr<'arena>,
        &'arena CHirExpr<'arena>,
        SourceSpan,
    ),
    Call(
        &'arena CHirExpr<'arena>,
        Vec<&'arena CHirExpr<'arena>>,
        SourceSpan,
    ),
    Cast(TypeId, &'arena CHirExpr<'arena>, SourceSpan),
    SizeOf(TypeId, SourceSpan),
    SizeOfExpr(&'arena CHirExpr<'arena>, SourceSpan),
    AlignOf(TypeId, SourceSpan),
    Member(&'arena CHirExpr<'arena>, String, SourceSpan),
    PtrMember(&'arena CHirExpr<'arena>, String, SourceSpan),
    Index(
        &'arena CHirExpr<'arena>,
        &'arena CHirExpr<'arena>,
        SourceSpan,
    ),
    Assign(
        &'arena CHirExpr<'arena>,
        &'arena CHirExpr<'arena>,
        SourceSpan,
    ),
    CompoundLit(TypeId, Vec<&'arena CHirExpr<'arena>>, SourceSpan),
    Comma(
        &'arena CHirExpr<'arena>,
        &'arena CHirExpr<'arena>,
        SourceSpan,
    ),
}

impl CHirExpr<'_> {
    #[must_use]
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Lit(_, span)
            | Self::FloatLit(_, span)
            | Self::CharLit(_, span)
            | Self::StringLit(_, span)
            | Self::Ident { span, .. }
            | Self::Unary(_, _, span)
            | Self::Binary(_, _, _, span)
            | Self::Ternary(_, _, _, span)
            | Self::Call(_, _, span)
            | Self::Cast(_, _, span)
            | Self::SizeOf(_, span)
            | Self::SizeOfExpr(_, span)
            | Self::AlignOf(_, span)
            | Self::Member(_, _, span)
            | Self::PtrMember(_, _, span)
            | Self::Index(_, _, span)
            | Self::Assign(_, _, span)
            | Self::CompoundLit(_, _, span)
            | Self::Comma(_, _, span) => *span,
        }
    }
}

// ---------------------------------------------------------------------------
// CHirStmt — C statement nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CHirBlockItem<'arena> {
    Declaration(&'arena CHirDecl<'arena>),
    Statement(&'arena CHirStmt<'arena>),
}

#[derive(Debug, Clone)]
pub enum CHirForInit<'arena> {
    Empty,
    Expression(&'arena CHirExpr<'arena>),
    Declaration(Vec<&'arena CHirDecl<'arena>>),
}

#[derive(Debug, Clone)]
pub enum CHirStmt<'arena> {
    Expr(&'arena CHirExpr<'arena>, SourceSpan),
    If(
        &'arena CHirExpr<'arena>,
        &'arena CHirStmt<'arena>,
        Option<&'arena CHirStmt<'arena>>,
        SourceSpan,
    ),
    Switch(
        &'arena CHirExpr<'arena>,
        &'arena CHirStmt<'arena>,
        SourceSpan,
    ),
    While(
        &'arena CHirExpr<'arena>,
        &'arena CHirStmt<'arena>,
        SourceSpan,
    ),
    DoWhile(
        &'arena CHirStmt<'arena>,
        &'arena CHirExpr<'arena>,
        SourceSpan,
    ),
    For(
        CHirForInit<'arena>,
        Option<&'arena CHirExpr<'arena>>,
        Option<&'arena CHirExpr<'arena>>,
        &'arena CHirStmt<'arena>,
        SourceSpan,
    ),
    Return(Option<&'arena CHirExpr<'arena>>, SourceSpan),
    Compound(Vec<CHirBlockItem<'arena>>, SourceSpan),
    Break(SourceSpan),
    Continue(SourceSpan),
    Goto(SpannedName, SourceSpan),
    Label(SpannedName, &'arena CHirStmt<'arena>, SourceSpan),
    Case(
        &'arena CHirExpr<'arena>,
        &'arena CHirStmt<'arena>,
        SourceSpan,
    ),
    Default(&'arena CHirStmt<'arena>, SourceSpan),
    Null(SourceSpan),
}

impl CHirStmt<'_> {
    #[must_use]
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Expr(_, span)
            | Self::If(_, _, _, span)
            | Self::Switch(_, _, span)
            | Self::While(_, _, span)
            | Self::DoWhile(_, _, span)
            | Self::For(_, _, _, _, span)
            | Self::Return(_, span)
            | Self::Compound(_, span)
            | Self::Break(span)
            | Self::Continue(span)
            | Self::Goto(_, span)
            | Self::Label(_, _, span)
            | Self::Case(_, _, span)
            | Self::Default(_, span)
            | Self::Null(span) => *span,
        }
    }
}

// ---------------------------------------------------------------------------
// CHirDecl — C declaration nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct SpannedName {
    pub spelling: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageClass {
    None,
    Typedef,
    Extern,
    Static,
    Auto,
    Register,
    ThreadLocal,
}

#[derive(Debug, Clone)]
pub enum TypeSpecifier {
    Builtin {
        ty: TypeId,
        span: SourceSpan,
    },
    Tag {
        ty: TypeId,
        tag: TagId,
        kind: TagKind,
        name: Option<SpannedName>,
        span: SourceSpan,
    },
}

impl TypeSpecifier {
    #[must_use]
    pub fn type_id(&self) -> TypeId {
        match self {
            Self::Builtin { ty, .. } | Self::Tag { ty, .. } => *ty,
        }
    }

    #[must_use]
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Builtin { span, .. } | Self::Tag { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WrittenDeclaratorLayer {
    Pointer {
        qualifiers: Qualifiers,
        span: SourceSpan,
    },
    Array {
        bound: ArrayBound,
        span: SourceSpan,
    },
    Function {
        span: SourceSpan,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct WrittenDeclarator {
    /// Layers are ordered exactly like the semantic derived-declarator list:
    /// from the identifier outward through suffixes and pointer prefixes.
    pub layers: Vec<WrittenDeclaratorLayer>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ParameterDecl {
    /// Named parameters have an ordinary-identifier binding. Unnamed
    /// prototype parameters deliberately have no symbol.
    pub symbol: Option<SymbolId>,
    pub name: Option<SpannedName>,
    pub ty: TypeId,
    pub specifier: TypeSpecifier,
    pub written: WrittenDeclarator,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum ParameterSyntax {
    Unspecified,
    Prototype {
        parameters: Vec<ParameterDecl>,
        variadic: bool,
    },
}

#[derive(Debug, Clone)]
pub struct RecordMember<'arena> {
    pub name: Option<SpannedName>,
    pub ty: TypeId,
    pub specifier: TypeSpecifier,
    pub written: WrittenDeclarator,
    pub bit_width: Option<&'arena CHirExpr<'arena>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct EnumeratorDecl<'arena> {
    pub symbol: SymbolId,
    pub name: SpannedName,
    pub value: i64,
    pub written_value: Option<&'arena CHirExpr<'arena>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum TagMembers<'arena> {
    Incomplete,
    Struct(Vec<RecordMember<'arena>>),
    Union(Vec<RecordMember<'arena>>),
    Enum(Vec<EnumeratorDecl<'arena>>),
}

#[derive(Debug, Clone)]
pub struct TagDecl<'arena> {
    pub tag: TagId,
    pub scope: ScopeId,
    pub name: Option<SpannedName>,
    pub members: TagMembers<'arena>,
    /// The containing declaration, including its terminating semicolon.
    pub span: SourceSpan,
    /// Only the `struct`/`union`/`enum` specifier, excluding the semicolon and
    /// any enclosing object or function declarator.
    pub specifier_span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ObjectDecl<'arena> {
    pub symbol: SymbolId,
    pub name: SpannedName,
    pub ty: TypeId,
    pub specifier: TypeSpecifier,
    pub written: WrittenDeclarator,
    pub storage: StorageClass,
    pub initializer: Option<&'arena CHirExpr<'arena>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl<'arena> {
    pub symbol: SymbolId,
    pub name: SpannedName,
    /// The complete canonical function type, not merely its return type.
    pub ty: TypeId,
    pub return_specifier: TypeSpecifier,
    pub written: WrittenDeclarator,
    pub parameters: ParameterSyntax,
    pub storage: StorageClass,
    pub body: Option<&'arena CHirStmt<'arena>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct TypedefDecl {
    pub symbol: SymbolId,
    pub name: SpannedName,
    pub ty: TypeId,
    pub specifier: TypeSpecifier,
    pub written: WrittenDeclarator,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum CHirDecl<'arena> {
    Object(ObjectDecl<'arena>),
    Function(FunctionDecl<'arena>),
    Tag(TagDecl<'arena>),
    Typedef(TypedefDecl),
}

impl CHirDecl<'_> {
    #[must_use]
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Object(declaration) => declaration.span,
            Self::Function(declaration) => declaration.span,
            Self::Tag(declaration) => declaration.span,
            Self::Typedef(declaration) => declaration.span,
        }
    }
}

// ---------------------------------------------------------------------------
// CHirUnit — top-level translation unit
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CHirUnit<'arena> {
    declarations: Vec<&'arena CHirDecl<'arena>>,
    types: TypeInterner,
    symbols: SymbolTable,
}

impl<'arena> CHirUnit<'arena> {
    #[must_use]
    pub fn new(
        declarations: Vec<&'arena CHirDecl<'arena>>,
        types: TypeInterner,
        symbols: SymbolTable,
    ) -> Self {
        Self {
            declarations,
            types,
            symbols,
        }
    }

    #[must_use]
    pub fn declarations(&self) -> &[&'arena CHirDecl<'arena>] {
        &self.declarations
    }

    #[must_use]
    pub fn types(&self) -> &TypeInterner {
        &self.types
    }

    #[must_use]
    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.declarations.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &'arena CHirDecl<'arena>> + '_ {
        self.declarations.iter().copied()
    }
}

impl<'arena> std::ops::Deref for CHirUnit<'arena> {
    type Target = [&'arena CHirDecl<'arena>];

    fn deref(&self) -> &Self::Target {
        self.declarations()
    }
}

impl<'unit, 'arena> IntoIterator for &'unit CHirUnit<'arena> {
    type Item = &'arena CHirDecl<'arena>;
    type IntoIter = std::iter::Copied<std::slice::Iter<'unit, &'arena CHirDecl<'arena>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.declarations.iter().copied()
    }
}

// ---------------------------------------------------------------------------
// C-HIR Interpreter
// ---------------------------------------------------------------------------

use std::collections::HashMap;

pub fn eval_expr(expr: &CHirExpr<'_>, vars: &mut HashMap<String, i64>) -> i64 {
    match expr {
        CHirExpr::Lit(lit, _) => lit.value as i64,
        CHirExpr::Ident { spelling, .. } => *vars.get(spelling).unwrap_or(&0),
        CHirExpr::Unary(op, inner, _) => {
            let v = eval_expr(inner, vars);
            match op {
                UnaryOp::Neg => -v,
                UnaryOp::Not => {
                    if v == 0 {
                        1
                    } else {
                        0
                    }
                }
                UnaryOp::BitNot => !v,
                UnaryOp::Plus => v,
                _ => v,
            }
        }
        CHirExpr::Binary(op, lhs, rhs, _) => {
            let l = eval_expr(lhs, vars);
            let r = eval_expr(rhs, vars);
            match op {
                BinaryOp::Add => l.wrapping_add(r),
                BinaryOp::Sub => l.wrapping_sub(r),
                BinaryOp::Mul => l.wrapping_mul(r),
                BinaryOp::Div => {
                    if r != 0 {
                        l / r
                    } else {
                        0
                    }
                }
                BinaryOp::Lt => i64::from(l < r),
                BinaryOp::Gt => i64::from(l > r),
                BinaryOp::Le => i64::from(l <= r),
                BinaryOp::Ge => i64::from(l >= r),
                BinaryOp::Eq => i64::from(l == r),
                BinaryOp::Ne => i64::from(l != r),
                _ => 0,
            }
        }
        CHirExpr::Ternary(cond, t, f, _) => {
            if eval_expr(cond, vars) != 0 {
                eval_expr(t, vars)
            } else {
                eval_expr(f, vars)
            }
        }
        CHirExpr::Assign(lhs, rhs, _) => {
            let r = eval_expr(rhs, vars);
            if let CHirExpr::Ident { spelling, .. } = &**lhs {
                vars.insert(spelling.clone(), r);
            }
            r
        }
        CHirExpr::Call(_, _, _) => 0,
        _ => 0,
    }
}

pub fn eval_stmt(stmt: &CHirStmt<'_>, vars: &mut HashMap<String, i64>) -> Option<i64> {
    match stmt {
        CHirStmt::Expr(expr, _) => {
            eval_expr(expr, vars);
            None
        }
        CHirStmt::Return(expr, _) => Some(expr.as_ref().map_or(0, |e| eval_expr(e, vars))),
        CHirStmt::Compound(stmts, _) => {
            for item in stmts {
                let CHirBlockItem::Statement(statement) = item else {
                    continue;
                };
                if let Some(v) = eval_stmt(statement, vars) {
                    return Some(v);
                }
            }
            None
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_common::{Arena, FileId};
    use evocc_types::{CanonicalType, FunctionParameters, IntRank, TypeKind};

    fn test_span() -> SourceSpan {
        SourceSpan::new(FileId::new(1), 0, 10, 1, 1)
    }

    fn int_type_id(types: &mut TypeInterner) -> TypeId {
        types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        })
    }

    fn function_type_id(types: &mut TypeInterner, return_type: TypeId) -> TypeId {
        types.intern(CanonicalType {
            kind: TypeKind::Function {
                return_type,
                parameters: FunctionParameters::Unspecified,
            },
            qualifiers: Qualifiers::default(),
        })
    }

    fn builtin_specifier(ty: TypeId) -> TypeSpecifier {
        TypeSpecifier::Builtin {
            ty,
            span: test_span(),
        }
    }

    fn written_declarator() -> WrittenDeclarator {
        WrittenDeclarator {
            layers: Vec::new(),
            span: test_span(),
        }
    }

    fn name(spelling: &str) -> SpannedName {
        SpannedName {
            spelling: spelling.into(),
            span: test_span(),
        }
    }

    fn arena_alloc<'arena, T: 'arena>(arena: &'arena Arena, value: T) -> &'arena T {
        // Safety: these tests allocate only C-HIR values. C-HIR nodes contain
        // no Arena handle, and their transitive destructors only release owned
        // source-fact buffers, so teardown cannot access this Arena.
        unsafe { arena.alloc_shared(value) }
    }

    fn arena_expr<'arena>(
        arena: &'arena Arena,
        expression: CHirExpr<'arena>,
    ) -> &'arena CHirExpr<'arena> {
        arena_alloc(arena, expression)
    }

    fn arena_stmt<'arena>(
        arena: &'arena Arena,
        statement: CHirStmt<'arena>,
    ) -> &'arena CHirStmt<'arena> {
        arena_alloc(arena, statement)
    }

    fn arena_decl<'arena>(
        arena: &'arena Arena,
        declaration: CHirDecl<'arena>,
    ) -> &'arena CHirDecl<'arena> {
        arena_alloc(arena, declaration)
    }

    fn test_symbol(spelling: &str, ty: TypeId, data: SymbolData) -> SymbolId {
        let mut symbols = SymbolTable::new();
        symbols
            .declare(symbols.file_scope(), spelling, data, ty, test_span())
            .unwrap()
    }

    fn ident<'arena>(spelling: &str, ty: TypeId) -> CHirExpr<'arena> {
        CHirExpr::Ident {
            symbol: test_symbol(
                spelling,
                ty,
                SymbolData::Object {
                    storage: StorageClass::None,
                },
            ),
            spelling: spelling.into(),
            facts: ExprFacts::lvalue(ty, true, true).unwrap(),
            span: test_span(),
        }
    }

    fn object_decl<'arena>(
        name_spelling: &str,
        ty: TypeId,
        initializer: Option<&'arena CHirExpr<'arena>>,
    ) -> CHirDecl<'arena> {
        CHirDecl::Object(ObjectDecl {
            symbol: test_symbol(
                name_spelling,
                ty,
                SymbolData::Object {
                    storage: StorageClass::None,
                },
            ),
            name: name(name_spelling),
            ty,
            specifier: builtin_specifier(ty),
            written: written_declarator(),
            storage: StorageClass::None,
            initializer,
            span: test_span(),
        })
    }

    #[test]
    fn c_hir_every_node_has_source_span() {
        let arena = Arena::default();
        let s = test_span();
        let d = FileId::DUMMY;
        let mut types = TypeInterner::new();
        let int_id = int_type_id(&mut types);
        let id = arena_expr(&arena, ident("x", int_id));
        let st = arena_stmt(&arena, CHirStmt::Null(s));
        let function_id = function_type_id(&mut types, int_id);
        let tag_id = types.declare_tag(TagKind::Struct, Some("S".into()));
        let dc = arena_decl(&arena, object_decl("local", int_id, None));
        let il = IntLit {
            value: 42,
            suffix: IntSuffix::None,
            base: IntBase::Dec,
        };

        // --- CHirExpr variants ---

        let exprs = vec![
            arena_expr(&arena, CHirExpr::Lit(il, s)),
            arena_expr(
                &arena,
                CHirExpr::FloatLit(
                    FloatLit {
                        value: 3.14,
                        suffix: FloatSuffix::None,
                    },
                    s,
                ),
            ),
            arena_expr(
                &arena,
                CHirExpr::CharLit(
                    CharLit {
                        value: 65,
                        prefix: CharPrefix::None,
                    },
                    s,
                ),
            ),
            arena_expr(
                &arena,
                CHirExpr::StringLit(
                    StringLit {
                        value: "hi".into(),
                        prefix: StringPrefix::None,
                    },
                    s,
                ),
            ),
            arena_expr(&arena, ident("a", int_id)),
            arena_expr(&arena, CHirExpr::Unary(UnaryOp::Neg, id, s)),
            arena_expr(&arena, CHirExpr::Binary(BinaryOp::Add, id, id, s)),
            arena_expr(&arena, CHirExpr::Ternary(id, id, id, s)),
            arena_expr(&arena, CHirExpr::Call(id, vec![id], s)),
            arena_expr(&arena, CHirExpr::Cast(int_id, id, s)),
            arena_expr(&arena, CHirExpr::SizeOf(int_id, s)),
            arena_expr(&arena, CHirExpr::SizeOfExpr(id, s)),
            arena_expr(&arena, CHirExpr::AlignOf(int_id, s)),
            arena_expr(&arena, CHirExpr::Member(id, "f".into(), s)),
            arena_expr(&arena, CHirExpr::PtrMember(id, "f".into(), s)),
            arena_expr(&arena, CHirExpr::Index(id, id, s)),
            arena_expr(&arena, CHirExpr::Assign(id, id, s)),
            arena_expr(&arena, CHirExpr::CompoundLit(int_id, vec![id], s)),
            arena_expr(&arena, CHirExpr::Comma(id, id, s)),
        ];

        for expression in exprs {
            let span = expression.span();
            assert_ne!(span.file, d, "CHirExpr variant has dummy FileId");
            assert!(span.is_valid(), "CHirExpr variant has invalid span");
        }

        // --- CHirStmt variants ---

        let stmts = vec![
            arena_stmt(&arena, CHirStmt::Expr(id, s)),
            arena_stmt(&arena, CHirStmt::If(id, st, Some(st), s)),
            arena_stmt(&arena, CHirStmt::Switch(id, st, s)),
            arena_stmt(&arena, CHirStmt::While(id, st, s)),
            arena_stmt(&arena, CHirStmt::DoWhile(st, id, s)),
            arena_stmt(
                &arena,
                CHirStmt::For(
                    CHirForInit::Declaration(vec![dc]),
                    Some(id),
                    Some(id),
                    st,
                    s,
                ),
            ),
            arena_stmt(&arena, CHirStmt::Return(Some(id), s)),
            arena_stmt(
                &arena,
                CHirStmt::Compound(
                    vec![CHirBlockItem::Declaration(dc), CHirBlockItem::Statement(st)],
                    s,
                ),
            ),
            arena_stmt(&arena, CHirStmt::Break(s)),
            arena_stmt(&arena, CHirStmt::Continue(s)),
            arena_stmt(&arena, CHirStmt::Goto(name("L"), s)),
            arena_stmt(&arena, CHirStmt::Label(name("L"), st, s)),
            arena_stmt(&arena, CHirStmt::Case(id, st, s)),
            arena_stmt(&arena, CHirStmt::Default(st, s)),
            st,
        ];

        for statement in stmts {
            let span = statement.span();
            assert_ne!(span.file, d, "CHirStmt variant has dummy FileId");
            assert!(span.is_valid(), "CHirStmt variant has invalid span");
        }

        // --- CHirDecl variants ---

        let func_body = arena_stmt(&arena, CHirStmt::Return(Some(id), s));
        let decls = vec![
            arena_decl(&arena, object_decl("x", int_id, Some(id))),
            arena_decl(
                &arena,
                CHirDecl::Function(FunctionDecl {
                    symbol: test_symbol(
                        "f",
                        function_id,
                        SymbolData::Function {
                            storage: StorageClass::None,
                        },
                    ),
                    name: name("f"),
                    ty: function_id,
                    return_specifier: builtin_specifier(int_id),
                    written: WrittenDeclarator {
                        layers: vec![WrittenDeclaratorLayer::Function { span: s }],
                        span: s,
                    },
                    parameters: ParameterSyntax::Unspecified,
                    storage: StorageClass::None,
                    body: Some(func_body),
                    span: s,
                }),
            ),
            arena_decl(
                &arena,
                CHirDecl::Tag(TagDecl {
                    tag: tag_id,
                    scope: SymbolTable::new().file_scope(),
                    name: Some(name("S")),
                    members: TagMembers::Incomplete,
                    span: s,
                    specifier_span: s,
                }),
            ),
            arena_decl(
                &arena,
                CHirDecl::Typedef(TypedefDecl {
                    symbol: test_symbol("T", int_id, SymbolData::Typedef),
                    name: name("T"),
                    ty: int_id,
                    specifier: builtin_specifier(int_id),
                    written: written_declarator(),
                    span: s,
                }),
            ),
        ];

        for declaration in decls {
            let span = declaration.span();
            assert_ne!(span.file, d, "CHirDecl variant has dummy FileId");
            assert!(span.is_valid(), "CHirDecl variant has invalid span");
        }
    }

    #[test]
    fn chir_unit_is_vec_of_decls() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let int_id = int_type_id(&mut types);
        let first = arena_decl(&arena, object_decl("a", int_id, None));
        let second = arena_decl(&arena, object_decl("b", int_id, None));

        let unit = CHirUnit::new(vec![first, second], types, SymbolTable::new());

        assert_eq!(unit.len(), 2);
        assert!(std::ptr::eq(unit.declarations()[0], first));
        assert!(std::ptr::eq(unit.declarations()[1], second));
    }

    #[test]
    fn nested_exprs_compile() {
        let arena = Arena::default();
        let s = test_span();
        let mut types = TypeInterner::new();
        let int_id = int_type_id(&mut types);
        let id = arena_expr(&arena, ident("x", int_id));
        let unary = arena_expr(&arena, CHirExpr::Unary(UnaryOp::Neg, id, s));
        let call = arena_expr(&arena, CHirExpr::Call(id, vec![id], s));
        let nested = arena_expr(&arena, CHirExpr::Binary(BinaryOp::Add, unary, call, s));
        assert_eq!(nested.span(), s);
    }

    #[test]
    fn nested_stmts_compile() {
        let arena = Arena::default();
        let s = test_span();
        let mut types = TypeInterner::new();
        let int_id = int_type_id(&mut types);
        let id = arena_expr(&arena, ident("x", int_id));
        let st = arena_stmt(&arena, CHirStmt::Expr(id, s));
        let compound = arena_stmt(
            &arena,
            CHirStmt::Compound(vec![CHirBlockItem::Statement(st)], s),
        );
        let return_stmt = arena_stmt(&arena, CHirStmt::Return(None, s));
        let nested = arena_stmt(&arena, CHirStmt::If(id, compound, Some(return_stmt), s));
        assert_eq!(nested.span(), s);
    }

    #[test]
    fn for_loop_all_parts_optional() {
        let arena = Arena::default();
        let s = test_span();
        let st = arena_stmt(&arena, CHirStmt::Null(s));
        let mut types = TypeInterner::new();
        let int_id = int_type_id(&mut types);
        let id = arena_expr(&arena, ident("x", int_id));

        let for_all = arena_stmt(
            &arena,
            CHirStmt::For(CHirForInit::Expression(id), Some(id), Some(id), st, s),
        );
        let for_none = arena_stmt(&arena, CHirStmt::For(CHirForInit::Empty, None, None, st, s));
        assert_eq!(for_all.span(), s);
        assert_eq!(for_none.span(), s);
    }

    #[test]
    fn int_lit_suffixes() {
        assert_eq!(IntSuffix::None, IntSuffix::None);
        assert_ne!(IntSuffix::U, IntSuffix::L);
        assert_ne!(IntSuffix::LL, IntSuffix::None);
    }

    #[test]
    fn int_lit_bases() {
        assert_eq!(IntBase::Dec, IntBase::Dec);
        assert_ne!(IntBase::Hex, IntBase::Oct);
        assert_ne!(IntBase::Bin, IntBase::Dec);
    }

    fn int_lit_expr<'arena>(arena: &'arena Arena, v: i64) -> &'arena CHirExpr<'arena> {
        arena_expr(
            arena,
            CHirExpr::Lit(
                IntLit {
                    value: v as u64,
                    suffix: IntSuffix::None,
                    base: IntBase::Dec,
                },
                test_span(),
            ),
        )
    }

    #[test]
    fn chir_interpreter_executes_integer_expressions() {
        let arena = Arena::default();
        let mut vars = HashMap::new();

        // 1 + 2 * 3 → 7
        let mul = arena_expr(
            &arena,
            CHirExpr::Binary(
                BinaryOp::Mul,
                int_lit_expr(&arena, 2),
                int_lit_expr(&arena, 3),
                test_span(),
            ),
        );
        let add = arena_expr(
            &arena,
            CHirExpr::Binary(BinaryOp::Add, int_lit_expr(&arena, 1), mul, test_span()),
        );
        assert_eq!(eval_expr(add, &mut vars), 7);

        // 1 < 2 → 1
        let lt = arena_expr(
            &arena,
            CHirExpr::Binary(
                BinaryOp::Lt,
                int_lit_expr(&arena, 1),
                int_lit_expr(&arena, 2),
                test_span(),
            ),
        );
        assert_eq!(eval_expr(lt, &mut vars), 1);

        // 3 > 5 → 0
        let gt = arena_expr(
            &arena,
            CHirExpr::Binary(
                BinaryOp::Gt,
                int_lit_expr(&arena, 3),
                int_lit_expr(&arena, 5),
                test_span(),
            ),
        );
        assert_eq!(eval_expr(gt, &mut vars), 0);

        // 1 ? 10 : 20 → 10
        let ternary = arena_expr(
            &arena,
            CHirExpr::Ternary(
                int_lit_expr(&arena, 1),
                int_lit_expr(&arena, 10),
                int_lit_expr(&arena, 20),
                test_span(),
            ),
        );
        assert_eq!(eval_expr(ternary, &mut vars), 10);
    }
}
