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

use evocc_common::SourceSpan;
use evocc_types::CType;

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
pub enum CHirExpr {
    Lit(IntLit, SourceSpan),
    FloatLit(FloatLit, SourceSpan),
    CharLit(CharLit, SourceSpan),
    StringLit(StringLit, SourceSpan),
    Ident(String, SourceSpan),
    Unary(UnaryOp, Box<CHirExpr>, SourceSpan),
    Binary(BinaryOp, Box<CHirExpr>, Box<CHirExpr>, SourceSpan),
    Ternary(Box<CHirExpr>, Box<CHirExpr>, Box<CHirExpr>, SourceSpan),
    Call(Box<CHirExpr>, Vec<CHirExpr>, SourceSpan),
    Cast(Box<CType>, Box<CHirExpr>, SourceSpan),
    SizeOf(Box<CType>, SourceSpan),
    SizeOfExpr(Box<CHirExpr>, SourceSpan),
    AlignOf(Box<CType>, SourceSpan),
    Member(Box<CHirExpr>, String, SourceSpan),
    PtrMember(Box<CHirExpr>, String, SourceSpan),
    Index(Box<CHirExpr>, Box<CHirExpr>, SourceSpan),
    Assign(Box<CHirExpr>, Box<CHirExpr>, SourceSpan),
    CompoundLit(Box<CType>, Vec<CHirExpr>, SourceSpan),
    Comma(Box<CHirExpr>, Box<CHirExpr>, SourceSpan),
}

// ---------------------------------------------------------------------------
// CHirStmt — C statement nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CHirStmt {
    Expr(CHirExpr, SourceSpan),
    If(CHirExpr, Box<CHirStmt>, Option<Box<CHirStmt>>, SourceSpan),
    Switch(CHirExpr, Box<CHirStmt>, SourceSpan),
    While(CHirExpr, Box<CHirStmt>, SourceSpan),
    DoWhile(Box<CHirStmt>, CHirExpr, SourceSpan),
    For(
        Option<Box<CHirExpr>>,
        Option<Box<CHirExpr>>,
        Option<Box<CHirExpr>>,
        Box<CHirStmt>,
        SourceSpan,
    ),
    Return(Option<CHirExpr>, SourceSpan),
    Compound(Vec<CHirStmt>, SourceSpan),
    Break(SourceSpan),
    Continue(SourceSpan),
    Goto(String, SourceSpan),
    Label(String, SourceSpan),
    Case(CHirExpr, SourceSpan),
    Default(SourceSpan),
    Decl(Box<CHirDecl>, SourceSpan),
    Null(SourceSpan),
}

// ---------------------------------------------------------------------------
// CHirDecl — C declaration nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CHirDecl {
    VarDecl(String, CType, Option<CHirExpr>, SourceSpan),
    FuncDecl(
        String,
        Vec<(String, CType)>,
        CType,
        Option<Box<CHirStmt>>,
        SourceSpan,
    ),
    TypedefDecl(String, CType, SourceSpan),
    StructDecl(String, Vec<(String, CType)>, SourceSpan),
    UnionDecl(String, Vec<(String, CType)>, SourceSpan),
    EnumDecl(String, Vec<(String, i64)>, SourceSpan),
}

// ---------------------------------------------------------------------------
// CHirUnit — top-level translation unit
// ---------------------------------------------------------------------------

pub type CHirUnit = Vec<CHirDecl>;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_common::FileId;
    use evocc_types::IntRank;

    fn test_span() -> SourceSpan {
        SourceSpan::new(FileId::new(1), 0, 10, 1, 1)
    }

    fn int_type() -> CType {
        CType::Int(IntRank::Int)
    }

    fn void_type() -> CType {
        CType::Void
    }

    fn dummy_ident() -> CHirExpr {
        CHirExpr::Ident("x".into(), test_span())
    }

    fn int_lit() -> IntLit {
        IntLit { value: 42, suffix: IntSuffix::None, base: IntBase::Dec }
    }

    fn dummy_stmt() -> CHirStmt {
        CHirStmt::Null(test_span())
    }

    fn dummy_decl() -> CHirDecl {
        CHirDecl::VarDecl("x".into(), int_type(), None, test_span())
    }

    fn extract_span(e: &CHirExpr) -> &SourceSpan {
        match e {
            CHirExpr::Lit(_, s) => s,
            CHirExpr::FloatLit(_, s) => s,
            CHirExpr::CharLit(_, s) => s,
            CHirExpr::StringLit(_, s) => s,
            CHirExpr::Ident(_, s) => s,
            CHirExpr::Unary(_, _, s) => s,
            CHirExpr::Binary(_, _, _, s) => s,
            CHirExpr::Ternary(_, _, _, s) => s,
            CHirExpr::Call(_, _, s) => s,
            CHirExpr::Cast(_, _, s) => s,
            CHirExpr::SizeOf(_, s) => s,
            CHirExpr::SizeOfExpr(_, s) => s,
            CHirExpr::AlignOf(_, s) => s,
            CHirExpr::Member(_, _, s) => s,
            CHirExpr::PtrMember(_, _, s) => s,
            CHirExpr::Index(_, _, s) => s,
            CHirExpr::Assign(_, _, s) => s,
            CHirExpr::CompoundLit(_, _, s) => s,
            CHirExpr::Comma(_, _, s) => s,
        }
    }

    fn extract_stmt_span(s: &CHirStmt) -> &SourceSpan {
        match s {
            CHirStmt::Expr(_, s) => s,
            CHirStmt::If(_, _, _, s) => s,
            CHirStmt::Switch(_, _, s) => s,
            CHirStmt::While(_, _, s) => s,
            CHirStmt::DoWhile(_, _, s) => s,
            CHirStmt::For(_, _, _, _, s) => s,
            CHirStmt::Return(_, s) => s,
            CHirStmt::Compound(_, s) => s,
            CHirStmt::Break(s) => s,
            CHirStmt::Continue(s) => s,
            CHirStmt::Goto(_, s) => s,
            CHirStmt::Label(_, s) => s,
            CHirStmt::Case(_, s) => s,
            CHirStmt::Default(s) => s,
            CHirStmt::Decl(_, s) => s,
            CHirStmt::Null(s) => s,
        }
    }

    fn extract_decl_span(d: &CHirDecl) -> &SourceSpan {
        match d {
            CHirDecl::VarDecl(_, _, _, s) => s,
            CHirDecl::FuncDecl(_, _, _, _, s) => s,
            CHirDecl::TypedefDecl(_, _, s) => s,
            CHirDecl::StructDecl(_, _, s) => s,
            CHirDecl::UnionDecl(_, _, s) => s,
            CHirDecl::EnumDecl(_, _, s) => s,
        }
    }

    #[test]
    fn c_hir_every_node_has_source_span() {
        let s = test_span();
        let d = FileId::DUMMY;
        let id = dummy_ident();
        let st = dummy_stmt();
        let dc = dummy_decl();
        let ty = int_type();
        let vty = void_type();
        let il = int_lit();

        // --- CHirExpr variants ---

        let exprs: Vec<CHirExpr> = vec![
            CHirExpr::Lit(il, s),
            CHirExpr::FloatLit(FloatLit { value: 3.14, suffix: FloatSuffix::None }, s),
            CHirExpr::CharLit(CharLit { value: 65, prefix: CharPrefix::None }, s),
            CHirExpr::StringLit(StringLit { value: "hi".into(), prefix: StringPrefix::None }, s),
            CHirExpr::Ident("a".into(), s),
            CHirExpr::Unary(UnaryOp::Neg, Box::new(id.clone()), s),
            CHirExpr::Binary(BinaryOp::Add, Box::new(id.clone()), Box::new(id.clone()), s),
            CHirExpr::Ternary(Box::new(id.clone()), Box::new(id.clone()), Box::new(id.clone()), s),
            CHirExpr::Call(Box::new(id.clone()), vec![id.clone()], s),
            CHirExpr::Cast(Box::new(ty.clone()), Box::new(id.clone()), s),
            CHirExpr::SizeOf(Box::new(ty.clone()), s),
            CHirExpr::SizeOfExpr(Box::new(id.clone()), s),
            CHirExpr::AlignOf(Box::new(ty.clone()), s),
            CHirExpr::Member(Box::new(id.clone()), "f".into(), s),
            CHirExpr::PtrMember(Box::new(id.clone()), "f".into(), s),
            CHirExpr::Index(Box::new(id.clone()), Box::new(id.clone()), s),
            CHirExpr::Assign(Box::new(id.clone()), Box::new(id.clone()), s),
            CHirExpr::CompoundLit(Box::new(ty.clone()), vec![id.clone()], s),
            CHirExpr::Comma(Box::new(id.clone()), Box::new(id.clone()), s),
        ];

        for e in &exprs {
            let sp = extract_span(e);
            assert_ne!(sp.file, d, "CHirExpr variant has dummy FileId");
            assert!(sp.is_valid(), "CHirExpr variant has invalid span");
        }

        // --- CHirStmt variants ---

        let stmts: Vec<CHirStmt> = vec![
            CHirStmt::Expr(id.clone(), s),
            CHirStmt::If(id.clone(), Box::new(st.clone()), Some(Box::new(st.clone())), s),
            CHirStmt::Switch(id.clone(), Box::new(st.clone()), s),
            CHirStmt::While(id.clone(), Box::new(st.clone()), s),
            CHirStmt::DoWhile(Box::new(st.clone()), id.clone(), s),
            CHirStmt::For(Some(Box::new(id.clone())), Some(Box::new(id.clone())), Some(Box::new(id.clone())), Box::new(st.clone()), s),
            CHirStmt::Return(Some(id.clone()), s),
            CHirStmt::Compound(vec![st.clone()], s),
            CHirStmt::Break(s),
            CHirStmt::Continue(s),
            CHirStmt::Goto("L".into(), s),
            CHirStmt::Label("L".into(), s),
            CHirStmt::Case(id.clone(), s),
            CHirStmt::Default(s),
            CHirStmt::Decl(Box::new(dc.clone()), s),
            CHirStmt::Null(s),
        ];

        for stmt in &stmts {
            let sp = extract_stmt_span(stmt);
            assert_ne!(sp.file, d, "CHirStmt variant has dummy FileId");
            assert!(sp.is_valid(), "CHirStmt variant has invalid span");
        }

        // --- CHirDecl variants ---

        let func_body = Box::new(CHirStmt::Return(Some(id.clone()), s));
        let decls: Vec<CHirDecl> = vec![
            CHirDecl::VarDecl("x".into(), ty.clone(), Some(id.clone()), s),
            CHirDecl::FuncDecl("f".into(), vec![("a".into(), ty.clone())], vty.clone(), Some(func_body), s),
            CHirDecl::TypedefDecl("T".into(), ty.clone(), s),
            CHirDecl::StructDecl("S".into(), vec![("x".into(), ty.clone())], s),
            CHirDecl::UnionDecl("U".into(), vec![("x".into(), ty.clone())], s),
            CHirDecl::EnumDecl("E".into(), vec![("A".into(), 0)], s),
        ];

        for decl in &decls {
            let sp = extract_decl_span(decl);
            assert_ne!(sp.file, d, "CHirDecl variant has dummy FileId");
            assert!(sp.is_valid(), "CHirDecl variant has invalid span");
        }
    }

    #[test]
    fn chir_unit_is_vec_of_decls() {
        let s = test_span();
        let ty = int_type();

        let unit: CHirUnit = vec![
            CHirDecl::VarDecl("a".into(), ty.clone(), None, s),
            CHirDecl::VarDecl("b".into(), ty.clone(), None, s),
        ];

        assert_eq!(unit.len(), 2);
    }

    #[test]
    fn nested_exprs_compile() {
        let s = test_span();
        let id = dummy_ident();

        let _nested = CHirExpr::Binary(
            BinaryOp::Add,
            Box::new(CHirExpr::Unary(UnaryOp::Neg, Box::new(id.clone()), s)),
            Box::new(CHirExpr::Call(Box::new(id.clone()), vec![id.clone()], s)),
            s,
        );
    }

    #[test]
    fn nested_stmts_compile() {
        let s = test_span();
        let id = dummy_ident();
        let st = CHirStmt::Expr(id.clone(), s);

        let _nested = CHirStmt::If(
            id.clone(),
            Box::new(CHirStmt::Compound(vec![st], s)),
            Some(Box::new(CHirStmt::Return(None, s))),
            s,
        );
    }

    #[test]
    fn for_loop_all_parts_optional() {
        let s = test_span();
        let st = dummy_stmt();

        let _for_all = CHirStmt::For(
            Some(Box::new(dummy_ident())),
            Some(Box::new(dummy_ident())),
            Some(Box::new(dummy_ident())),
            Box::new(st.clone()),
            s,
        );

        let _for_none = CHirStmt::For(None, None, None, Box::new(st), s);
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
}
