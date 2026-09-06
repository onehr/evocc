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

pub mod allocation;
pub mod constant;
pub mod initializer;
pub mod integer;
pub mod interp;
pub mod place;
pub mod semantic;

pub use allocation::{
    AllocationSite, AllocationSiteKind, AllocationSiteManifest, AllocationSiteManifestError,
    AllocationSiteOrdinal,
};
pub use constant::{
    attribute_query_target, builtin_constant_p, validate_front_end_builtin_records,
    AttributeQueryTarget,
};
pub use initializer::{
    InitAction, InitPlan, InitPlanError, InitializerDesignation, ObjectInitializer, SubobjectPath,
    SubobjectStep, WrittenDesignator, WrittenInitializer,
};
pub use place::{
    is_modifiable_object_type, record_member_path, FieldProjection, IndexProjection, MemoryPlace,
    MemoryPlaceError, PlaceProjection, PlaceRoot, PointerTypeDesc,
};
pub use semantic::{
    AssignmentOp, ExprCategory, ExprFacts, ExprFactsError, NotAssignmentOperator, ScalarPlace,
    ScalarPlaceError, Scope, ScopeId, ScopeKind, Symbol, SymbolData, SymbolId, SymbolKind,
    SymbolTable, SymbolTableCheckpoint, SymbolTableError, SymbolTableTransaction, TagBinding,
    UpdateEffect, UpdateOp, UpdatePlan,
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
    /// Raw contents of the full literal sequence, without prefixes or quotes.
    pub value: String,
    pub prefix: StringPrefix,
    /// Raw contents of each preprocessing string-literal token. Boundaries
    /// are semantic: `"\\x1" "2"` has two units, unlike `"\\x12"`.
    pub fragments: Vec<String>,
}

impl StringLit {
    #[must_use]
    pub fn single(value: impl Into<String>, prefix: StringPrefix) -> Self {
        let value = value.into();
        Self {
            fragments: vec![value.clone()],
            value,
            prefix,
        }
    }

    /// Number of explicit code units before the implicit terminating zero.
    ///
    /// `value` contains raw literal contents without a prefix or quotes, so
    /// escape spelling remains distinguishable from directly encoded text.
    #[must_use]
    pub fn explicit_unit_count(&self) -> Option<u64> {
        u64::try_from(self.decoded_units()?.len()).ok()
    }

    /// Decode the explicit execution code units of this literal sequence.
    ///
    /// The terminating zero is deliberately excluded because C omits it when
    /// an exactly-sized destination array has no remaining element. Fragment
    /// boundaries are retained while decoding, so `"\\x1" "2"` cannot be
    /// confused with `"\\x12"`.
    #[must_use]
    pub fn decoded_units(&self) -> Option<Vec<u32>> {
        if self.fragments.is_empty() {
            return decode_c_string_units(&self.value, self.prefix);
        }
        if self.fragments.concat() != self.value {
            return None;
        }
        let mut units = Vec::new();
        for fragment in &self.fragments {
            units.extend(decode_c_string_units(fragment, self.prefix)?);
        }
        Some(units)
    }

    /// A string object's target representation, including its terminating zero.
    #[must_use]
    pub fn object_bytes(&self, target: &evocc_types::TargetDataLayout) -> Option<(Vec<u8>, u32)> {
        let width = match self.prefix {
            StringPrefix::None | StringPrefix::U8 => 1,
            StringPrefix::L | StringPrefix::U16 => 2,
            StringPrefix::U => 4,
        };
        let mut units = self.decoded_units()?;
        units.push(0);
        let mut bytes = Vec::new();
        for unit in units {
            match target.byte_order() {
                evocc_types::ByteOrder::Little => {
                    bytes.extend_from_slice(&unit.to_le_bytes()[..width])
                }
                evocc_types::ByteOrder::Big => {
                    bytes.extend_from_slice(&unit.to_be_bytes()[4 - width..])
                }
            }
        }
        Some((bytes, width as u32))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringPrefix {
    None,
    L,
    /// C11 `u` prefix (`char16_t` code units).
    U16,
    /// C11 `U` prefix (`char32_t` code units).
    U,
    U8,
}

fn decode_c_string_units(contents: &str, prefix: StringPrefix) -> Option<Vec<u32>> {
    let mut chars = contents.chars().peekable();
    let mut units = Vec::new();
    while let Some(character) = chars.next() {
        if character != '\\' {
            push_encoded_character(&mut units, character, prefix);
            continue;
        }

        let escaped = chars.next()?;
        let numeric = match escaped {
            '\n' => continue,
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                continue;
            }
            'x' => {
                let mut value = 0_u32;
                let mut digits = 0_u32;
                while let Some(digit) = chars.peek().and_then(|next| next.to_digit(16)) {
                    value = value.checked_mul(16)?.checked_add(digit)?;
                    chars.next();
                    digits += 1;
                }
                if digits == 0 {
                    return None;
                }
                value
            }
            'u' => {
                let mut value = 0_u32;
                for _ in 0..4 {
                    value = value
                        .checked_mul(16)?
                        .checked_add(chars.next()?.to_digit(16)?)?;
                }
                if value < 0xa0 && !matches!(value, 0x24 | 0x40 | 0x60) {
                    return None;
                }
                let character = char::from_u32(value)?;
                push_encoded_character(&mut units, character, prefix);
                continue;
            }
            'U' => {
                let mut value = 0_u32;
                for _ in 0..8 {
                    value = value
                        .checked_mul(16)?
                        .checked_add(chars.next()?.to_digit(16)?)?;
                }
                if value < 0xa0 && !matches!(value, 0x24 | 0x40 | 0x60) {
                    return None;
                }
                let character = char::from_u32(value)?;
                push_encoded_character(&mut units, character, prefix);
                continue;
            }
            first @ '0'..='7' => {
                let mut value = first.to_digit(8)?;
                for _ in 0..2 {
                    let Some(digit) = chars.peek().and_then(|next| next.to_digit(8)) else {
                        break;
                    };
                    value = value.checked_mul(8)?.checked_add(digit)?;
                    chars.next();
                }
                value
            }
            'a' => 7,
            'b' => 8,
            'e' => 27,
            'f' => 12,
            'n' => 10,
            'r' => 13,
            't' => 9,
            'v' => 11,
            '\\' => u32::from(b'\\'),
            '\'' => u32::from(b'\''),
            '"' => u32::from(b'"'),
            '?' => u32::from(b'?'),
            other => {
                push_encoded_character(&mut units, other, prefix);
                continue;
            }
        };
        push_numeric_unit(&mut units, numeric, prefix)?;
    }
    Some(units)
}

fn push_encoded_character(output: &mut Vec<u32>, character: char, prefix: StringPrefix) {
    match prefix {
        StringPrefix::None | StringPrefix::U8 => {
            let mut bytes = [0_u8; 4];
            output.extend(
                character
                    .encode_utf8(&mut bytes)
                    .as_bytes()
                    .iter()
                    .copied()
                    .map(u32::from),
            );
        }
        StringPrefix::U16 => output.extend(
            character
                .encode_utf16(&mut [0_u16; 2])
                .iter()
                .copied()
                .map(u32::from),
        ),
        StringPrefix::L => output.extend(
            character
                .encode_utf16(&mut [0_u16; 2])
                .iter()
                .copied()
                .map(u32::from),
        ),
        StringPrefix::U => output.push(u32::from(character)),
    }
}

fn push_numeric_unit(output: &mut Vec<u32>, value: u32, prefix: StringPrefix) -> Option<()> {
    let maximum = match prefix {
        StringPrefix::None | StringPrefix::U8 => u32::from(u8::MAX),
        StringPrefix::U16 => u32::from(u16::MAX),
        StringPrefix::L => u32::from(u16::MAX),
        StringPrefix::U => u32::MAX,
    };
    if value > maximum {
        return None;
    }
    output.push(value);
    Some(())
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

/// Infinite-precision arithmetic selected by a checked GNU builtin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CheckedArithmeticOp {
    Add,
    Sub,
    Mul,
}

/// Front-end-complete variable-argument operations. Target ABI semantics are
/// deliberately deferred, so executable consumers must fail closed when one
/// is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VarArgsBuiltinOp {
    Start,
    End,
    Copy,
    Arg,
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
    /// GNU address of a function-local label (`&&name`). The front end keeps
    /// this source value, but revision 1.9.17 deliberately provides no EvoIR
    /// representation for it.
    LabelAddress {
        label: SpannedName,
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
    /// GNU conditional with an omitted middle operand (`a ?: b`). Consumers
    /// must evaluate `operand` exactly once and use that same value as both
    /// the condition and the selected true result.
    Elvis(
        &'arena CHirExpr<'arena>,
        &'arena CHirExpr<'arena>,
        SourceSpan,
    ),
    Call(
        &'arena CHirExpr<'arena>,
        Vec<&'arena CHirExpr<'arena>>,
        SourceSpan,
    ),
    /// Front-end-complete GNU checked arithmetic. The first two operands and
    /// the destination pointer are evaluation roots, in that source order.
    CheckedArithmetic {
        op: CheckedArithmeticOp,
        lhs: &'arena CHirExpr<'arena>,
        rhs: &'arena CHirExpr<'arena>,
        target: &'arena CHirExpr<'arena>,
        target_ty: TypeId,
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// Front-end-complete GNU bit counting whose operand has already been
    /// converted to the builtin signature selected by its suffix.
    BitBuiltin {
        op: integer::BitBuiltinOp,
        operand: &'arena CHirExpr<'arena>,
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// Reaching this expression reports the typed unreachable-code trap.
    BuiltinUnreachable {
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// The current activation's return address.
    BuiltinReturnAddress {
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// The frame-address builtin; only depth zero has executable semantics.
    BuiltinFrameAddress {
        depth: u128,
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// A GNU variable-argument operation retained until the target calling
    /// convention defines its execution. `second` is the last named parameter
    /// for `Start`, the source object for `Copy`, and absent for `End` and
    /// `Arg`. `requested_ty` is present only for `Arg`.
    BuiltinVarArgs {
        op: VarArgsBuiltinOp,
        first: &'arena CHirExpr<'arena>,
        second: Option<&'arena CHirExpr<'arena>>,
        requested_ty: Option<TypeId>,
        facts: ExprFacts,
        span: SourceSpan,
    },
    Cast(TypeId, &'arena CHirExpr<'arena>, SourceSpan),
    /// A front-end boundary that evaluates its operand exactly once even when
    /// nested inside generated arithmetic. It is erased before EvoIR.
    EvaluateOnce(&'arena CHirExpr<'arena>, SourceSpan),
    SizeOf(TypeId, SourceSpan),
    /// `sizeof` of an expression retains the unevaluated source operand and
    /// the front end's resolved operand type. Runtime consumers use only the
    /// type; retaining the operand keeps source structure reviewable without
    /// accidentally evaluating it.
    SizeOfExpr(&'arena CHirExpr<'arena>, TypeId, SourceSpan),
    AlignOf(TypeId, SourceSpan),
    Member(&'arena CHirExpr<'arena>, String, SourceSpan),
    PtrMember(&'arena CHirExpr<'arena>, String, SourceSpan),
    Index(
        &'arena CHirExpr<'arena>,
        &'arena CHirExpr<'arena>,
        SourceSpan,
    ),
    Assign {
        op: AssignmentOp,
        place: ScalarPlace,
        target: &'arena CHirExpr<'arena>,
        value: &'arena CHirExpr<'arena>,
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// Assignment through an indirect or projected C lvalue. `place` is the
    /// sole evaluation authority; `target_span` is diagnostic provenance only.
    MemoryAssign {
        op: AssignmentOp,
        place: MemoryPlace<'arena>,
        target_span: SourceSpan,
        value: &'arena CHirExpr<'arena>,
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// The value of an indirect or projected C lvalue, after lvalue conversion.
    ///
    /// `place` is the sole evaluation authority and `target_span` is diagnostic
    /// provenance only. The designating syntax is deliberately not retained:
    /// a consumer that could walk both the syntax and the place would evaluate
    /// subscript expressions twice, which C forbids. Contexts that must not
    /// perform lvalue conversion, such as the operand of `&` or the target of
    /// an assignment, recover the place from this node instead of re-deriving
    /// it from syntax.
    MemoryRead {
        place: MemoryPlace<'arena>,
        target_span: SourceSpan,
        facts: ExprFacts,
        span: SourceSpan,
    },
    Update {
        op: UpdateOp,
        place: ScalarPlace,
        target: &'arena CHirExpr<'arena>,
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// Prefix/postfix update through an indirect or projected C lvalue.
    /// `place` is the sole evaluation authority.
    MemoryUpdate {
        op: UpdateOp,
        place: MemoryPlace<'arena>,
        target_span: SourceSpan,
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// A GNU statement expression. `body` is the sole execution authority;
    /// `value` only identifies the final expression statement whose value is
    /// yielded when control reaches the closing brace.
    StatementExpr {
        body: &'arena CHirStmt<'arena>,
        value: Option<&'arena CHirExpr<'arena>>,
        facts: ExprFacts,
        span: SourceSpan,
    },
    /// A C compound-literal object. The normalized initializer is the sole
    /// write authority; allocation duration is derived from the occurrence's
    /// file/function context by the allocation manifest.
    CompoundLit(TypeId, ObjectInitializer<'arena>, SourceSpan),
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
            | Self::LabelAddress { span, .. }
            | Self::Unary(_, _, span)
            | Self::Binary(_, _, _, span)
            | Self::Ternary(_, _, _, span)
            | Self::Elvis(_, _, span)
            | Self::Call(_, _, span)
            | Self::CheckedArithmetic { span, .. }
            | Self::BitBuiltin { span, .. }
            | Self::BuiltinUnreachable { span, .. }
            | Self::BuiltinReturnAddress { span, .. }
            | Self::BuiltinFrameAddress { span, .. }
            | Self::BuiltinVarArgs { span, .. }
            | Self::Cast(_, _, span)
            | Self::EvaluateOnce(_, span)
            | Self::SizeOf(_, span)
            | Self::SizeOfExpr(_, _, span)
            | Self::AlignOf(_, span)
            | Self::Member(_, _, span)
            | Self::PtrMember(_, _, span)
            | Self::Index(_, _, span)
            | Self::Assign { span, .. }
            | Self::MemoryAssign { span, .. }
            | Self::MemoryRead { span, .. }
            | Self::Update { span, .. }
            | Self::MemoryUpdate { span, .. }
            | Self::StatementExpr { span, .. }
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

/// One input or output operand retained from GNU extended `asm`.
#[derive(Debug, Clone)]
pub struct AsmOperand<'arena> {
    pub symbolic: Option<String>,
    pub symbolic_span: Option<SourceSpan>,
    pub constraint: String,
    pub constraint_span: SourceSpan,
    pub expression: &'arena CHirExpr<'arena>,
    pub span: SourceSpan,
}

/// A front-end-complete GNU `asm` construct.
///
/// Boolean qualifier facts are paired with their exact source spans. The
/// spans also recover source order without making that order semantically
/// significant.
#[derive(Debug, Clone)]
pub struct AsmStmt<'arena> {
    pub template: String,
    pub template_span: SourceSpan,
    pub outputs: Vec<AsmOperand<'arena>>,
    pub inputs: Vec<AsmOperand<'arena>>,
    pub clobbers: Vec<String>,
    pub clobber_spans: Vec<SourceSpan>,
    pub labels: Vec<SpannedName>,
    pub volatile: bool,
    pub volatile_span: Option<SourceSpan>,
    pub inline: bool,
    pub inline_span: Option<SourceSpan>,
    pub is_goto: bool,
    pub goto_span: Option<SourceSpan>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum CHirStmt<'arena> {
    Asm(AsmStmt<'arena>),
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
    /// GNU indirect control transfer (`goto *expr`). It is preserved for
    /// diagnostics and fails closed before EvoIR in revision 1.9.17.
    ComputedGoto(&'arena CHirExpr<'arena>, SourceSpan),
    Label(SpannedName, &'arena CHirStmt<'arena>, SourceSpan),
    Case(
        &'arena CHirExpr<'arena>,
        &'arena CHirStmt<'arena>,
        SourceSpan,
    ),
    /// GNU inclusive case range. Both endpoints are retained constant
    /// expressions; consumers compare them after the switch promotion.
    CaseRange(
        &'arena CHirExpr<'arena>,
        &'arena CHirExpr<'arena>,
        &'arena CHirStmt<'arena>,
        SourceSpan,
    ),
    Default(&'arena CHirStmt<'arena>, SourceSpan),
    /// A null statement carrying GNU attributes written immediately before
    /// its semicolon. The attributes are source facts only and have no
    /// executable effect.
    AttributedNull(Vec<Attribute>, SourceSpan),
    Null(SourceSpan),
}

impl CHirStmt<'_> {
    #[must_use]
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Asm(statement) => statement.span,
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
            | Self::ComputedGoto(_, span)
            | Self::Label(_, _, span)
            | Self::Case(_, _, span)
            | Self::CaseRange(_, _, _, span)
            | Self::Default(_, span)
            | Self::AttributedNull(_, span)
            | Self::Null(span) => *span,
        }
    }
}

/// Return the expression that supplies a GNU statement expression's value.
///
/// An ordinary label changes control-flow identity but not the type or value
/// of the statement it prefixes. Keeping this rule beside C-HIR gives every
/// consumer one recursive authority for the terminal-statement shape.
#[must_use]
pub fn statement_expression_value<'arena>(
    statement: &'arena CHirStmt<'arena>,
) -> Option<&'arena CHirExpr<'arena>> {
    match statement {
        CHirStmt::Expr(expression, _) => Some(*expression),
        CHirStmt::Label(_, nested, _) => statement_expression_value(nested),
        _ => None,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeofOperand {
    TypeName,
    Expression,
}

#[derive(Debug, Clone)]
pub enum TypeSpecifier {
    Builtin {
        ty: TypeId,
        span: SourceSpan,
    },
    /// A type spelled through the ordinary-identifier namespace.
    ///
    /// `ty` is the canonical type of this particular use (including any
    /// qualifiers written around the typedef name), while `symbol` preserves
    /// the stable identity of the typedef declaration that was selected by
    /// lexical lookup.
    TypedefName {
        ty: TypeId,
        symbol: SymbolId,
        name: SpannedName,
        span: SourceSpan,
    },
    Tag {
        ty: TypeId,
        tag: TagId,
        kind: TagKind,
        name: Option<SpannedName>,
        span: SourceSpan,
    },
    /// `typeof(...)` or `__typeof_unqual__(...)`, resolved completely by the
    /// front end while retaining whether the source operand was a type name or
    /// an expression.
    Typeof {
        resolved: TypeId,
        operand: TypeofOperand,
        span: SourceSpan,
    },
    /// GNU `__auto_type`, resolved from the initializer after lvalue
    /// conversion. `resolved` is the base type before the written declarator
    /// layers are applied, preserving the same source/type split as every
    /// other declaration specifier.
    AutoType {
        resolved: TypeId,
        span: SourceSpan,
    },
}

impl TypeSpecifier {
    #[must_use]
    pub fn type_id(&self) -> TypeId {
        match self {
            Self::Builtin { ty, .. } | Self::TypedefName { ty, .. } | Self::Tag { ty, .. } => *ty,
            Self::Typeof { resolved, .. } | Self::AutoType { resolved, .. } => *resolved,
        }
    }

    #[must_use]
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Builtin { span, .. }
            | Self::TypedefName { span, .. }
            | Self::Tag { span, .. }
            | Self::Typeof { span, .. }
            | Self::AutoType { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WrittenDeclaratorLayer {
    Pointer {
        qualifiers: Qualifiers,
        /// Attributes written after this pointer's `*`. They belong only to
        /// this layer and are preserved without semantic application.
        attributes: Vec<Attribute>,
        span: SourceSpan,
    },
    Array {
        bound: ArrayBound,
        /// Qualifiers written between the brackets of an array parameter.
        /// They qualify the pointer produced by parameter adjustment, not the
        /// array element type.
        parameter_qualifiers: Qualifiers,
        /// Whether `static` was written between the brackets. This is a
        /// source fact; the canonical parameter type is still the one-time
        /// adjusted pointer type.
        static_bound: bool,
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

/// Where an attribute specifier appeared, which fixes what it attaches to
/// (specification 10.1.9, placement table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributePlacement {
    /// Among the declaration specifiers: attaches to every declarator.
    DeclarationSpecifiers,
    /// Immediately after a declarator: attaches to that declarator alone.
    AfterDeclarator,
    /// Written immediately before a declarator that follows a comma.
    BeforeDeclarator,
    /// After `struct`, `union` or `enum`: attaches to the tag type.
    TagKeyword,
    /// After a tag definition's closing brace: attaches to the tag type.
    TagClosingBrace,
    /// After a member declarator.
    Member,
    /// After a parameter declarator.
    Parameter,
    /// After a function declarator, before its body.
    FunctionDefinition,
    /// After a label.
    Label,
    /// Immediately before a null statement.
    NullStatement,
    /// After an enumerator.
    Enumerator,
    /// In a type name.
    TypeName,
    /// After `*` and its qualifiers, before the rest of the declarator.
    PointerDeclarator,
}

impl AttributePlacement {
    #[must_use]
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::DeclarationSpecifiers => "declaration-specifiers",
            Self::AfterDeclarator => "after-declarator",
            Self::BeforeDeclarator => "before-declarator",
            Self::TagKeyword => "tag-keyword",
            Self::TagClosingBrace => "tag-closing-brace",
            Self::Member => "member",
            Self::Parameter => "parameter",
            Self::FunctionDefinition => "function-definition",
            Self::Label => "label",
            Self::NullStatement => "null-statement",
            Self::Enumerator => "enumerator",
            Self::TypeName => "type-name",
            Self::PointerDeclarator => "pointer-declarator",
        }
    }
}

/// One attribute argument, kept exactly as written.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeArgument {
    pub text: String,
    pub span: SourceSpan,
    /// The value when the argument was read as an integer constant
    /// expression (`aligned(sizeof(void *))`); `None` for every other argument.
    pub integer: Option<u64>,
}

/// The meaning this profile gave an attribute. Only two attributes have one;
/// every other attribute is preserved with `None` here and is listed by name
/// as not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppliedAttribute {
    /// `aligned(N)` or bare `aligned`, resolved to bytes.
    Aligned(u32),
    /// `packed`.
    Packed,
}

/// A GNU `__attribute__` entry, preserved losslessly whether or not it was
/// understood. `name` is canonical (`__unused__` and `unused` both give
/// `unused`); `written` is the spelling in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub written: String,
    pub arguments: Vec<AttributeArgument>,
    pub placement: AttributePlacement,
    pub span: SourceSpan,
    pub applied: Option<AppliedAttribute>,
}

impl Attribute {
    /// Strip one leading and one trailing `__`, which GNU C treats as the
    /// same attribute name. Any other spelling is returned unchanged.
    #[must_use]
    pub fn canonical_name(written: &str) -> String {
        written
            .strip_prefix("__")
            .and_then(|inner| inner.strip_suffix("__"))
            .unwrap_or(written)
            .to_owned()
    }
}

/// Every attribute in a unit, grouped by whether the profile applied it.
///
/// Produced from C-HIR rather than by hand so the receipt's
/// preserved-not-applied list is a measurement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeInventory {
    pub applied: std::collections::BTreeMap<String, usize>,
    pub preserved_not_applied: std::collections::BTreeMap<String, usize>,
}

impl AttributeInventory {
    fn record(&mut self, attribute: &Attribute) {
        let bucket = if attribute.applied.is_some() {
            &mut self.applied
        } else {
            &mut self.preserved_not_applied
        };
        *bucket.entry(attribute.name.clone()).or_insert(0) += 1;
    }
}

#[derive(Debug, Clone)]
pub struct ParameterDecl {
    /// Named parameters have an ordinary-identifier binding. Unnamed
    /// prototype parameters deliberately have no symbol.
    pub symbol: Option<SymbolId>,
    pub name: Option<SpannedName>,
    pub ty: TypeId,
    /// The only storage class admitted on a parameter is `register`.
    pub storage: StorageClass,
    pub specifier: TypeSpecifier,
    pub written: WrittenDeclarator,
    /// GNU attributes attached here, in source order.
    pub attributes: Vec<Attribute>,
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
    /// GNU attributes attached here, in source order.
    pub attributes: Vec<Attribute>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct EnumeratorDecl<'arena> {
    pub symbol: SymbolId,
    pub name: SpannedName,
    pub value: i128,
    pub written_value: Option<&'arena CHirExpr<'arena>>,
    /// GNU attributes attached here, in source order.
    pub attributes: Vec<Attribute>,
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
    /// Every declaration-prefix GNU `__extension__` marker, in source order.
    /// These spans are provenance only and have no semantic effect.
    pub gnu_extension_spans: Vec<SourceSpan>,
    /// GNU attributes attached here, in source order.
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone)]
pub struct ObjectDecl<'arena> {
    pub symbol: SymbolId,
    pub name: SpannedName,
    pub ty: TypeId,
    pub specifier: TypeSpecifier,
    pub written: WrittenDeclarator,
    pub storage: StorageClass,
    pub initializer: Option<ObjectInitializer<'arena>>,
    /// GNU assembler name attached after this declarator, if present.
    pub asm_label: Option<String>,
    pub asm_label_span: Option<SourceSpan>,
    /// Every declaration-prefix GNU `__extension__` marker, in source order.
    pub gnu_extension_spans: Vec<SourceSpan>,
    /// GNU attributes attached here, in source order.
    pub attributes: Vec<Attribute>,
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
    /// GNU assembler name attached after this declarator, if present.
    pub asm_label: Option<String>,
    pub asm_label_span: Option<SourceSpan>,
    /// Every written C11 `inline` function specifier, in source order.
    ///
    /// An empty vector means the declaration is not inline; retaining every
    /// token preserves both the written count and each exact source origin.
    /// Day 5 preserves syntax and provenance only; C11 6.7.4 inline-definition
    /// and linkage semantics are not modeled by this field.
    pub inline_spans: Vec<SourceSpan>,
    /// Every declaration-prefix GNU `__extension__` marker, in source order.
    pub gnu_extension_spans: Vec<SourceSpan>,
    /// GNU attributes attached here, in source order.
    pub attributes: Vec<Attribute>,
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
    /// Every declaration-prefix GNU `__extension__` marker, in source order.
    pub gnu_extension_spans: Vec<SourceSpan>,
    /// GNU attributes attached here, in source order.
    pub attributes: Vec<Attribute>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum CHirDecl<'arena> {
    Asm(AsmStmt<'arena>),
    Object(ObjectDecl<'arena>),
    Function(FunctionDecl<'arena>),
    Tag(TagDecl<'arena>),
    Typedef(TypedefDecl),
}

/// Source-fidelity record for a `_Generic` that was resolved away by the
/// front end. Expressions retain only the selected association.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenericSelectionRecord {
    pub span: SourceSpan,
    pub controlling_span: SourceSpan,
    pub selected_index: usize,
    pub association_count: usize,
}

/// Source-fidelity record for a front-end-resolved GNU builtin. Executable
/// C-HIR contains only the resolved expression; operands retained here are
/// parsed and type-checked source facts, not additional evaluation roots.
#[derive(Debug, Clone)]
pub enum FrontEndBuiltinRecord<'arena> {
    Expect {
        span: SourceSpan,
        value: &'arena CHirExpr<'arena>,
        expected: &'arena CHirExpr<'arena>,
    },
    ConstantP {
        span: SourceSpan,
        operand: &'arena CHirExpr<'arena>,
        result: bool,
    },
    ObjectSize {
        span: SourceSpan,
        operand: &'arena CHirExpr<'arena>,
        mode: u8,
        result: u64,
    },
    HasAttribute {
        span: SourceSpan,
        operand: Option<&'arena CHirExpr<'arena>>,
        attribute: String,
        result: bool,
    },
}

impl CHirDecl<'_> {
    #[must_use]
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Asm(declaration) => declaration.span,
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
    detached_attributes: Vec<Attribute>,
    generic_selections: Vec<GenericSelectionRecord>,
    front_end_builtins: Vec<FrontEndBuiltinRecord<'arena>>,
    unprototyped_parameters: std::collections::HashMap<SymbolId, Vec<TypeId>>,
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
            detached_attributes: Vec::new(),
            generic_selections: Vec::new(),
            front_end_builtins: Vec::new(),
            unprototyped_parameters: std::collections::HashMap::new(),
        }
    }

    pub fn set_unprototyped_parameters(
        &mut self,
        parameters: std::collections::HashMap<SymbolId, Vec<TypeId>>,
    ) {
        self.unprototyped_parameters = parameters;
    }

    #[must_use]
    pub fn unprototyped_parameters(&self, symbol: SymbolId) -> Option<&[TypeId]> {
        self.unprototyped_parameters.get(&symbol).map(Vec::as_slice)
    }

    /// Attach the attributes that had no declaration node: labels and type
    /// names. Called once by the parser after the unit is assembled.
    pub fn set_detached_attributes(&mut self, attributes: Vec<Attribute>) {
        self.detached_attributes = attributes;
    }

    pub fn set_generic_selections(&mut self, selections: Vec<GenericSelectionRecord>) {
        self.generic_selections = selections;
    }

    pub fn set_front_end_builtins(&mut self, builtins: Vec<FrontEndBuiltinRecord<'arena>>) {
        self.front_end_builtins = builtins;
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

    /// Attributes whose placement has no declaration node to live on: labels
    /// and type names. They are preserved here with their placement and span
    /// so the unit still shows every attribute the source had.
    #[must_use]
    pub fn detached_attributes(&self) -> &[Attribute] {
        &self.detached_attributes
    }

    #[must_use]
    pub fn generic_selections(&self) -> &[GenericSelectionRecord] {
        &self.generic_selections
    }

    #[must_use]
    pub fn front_end_builtins(&self) -> &[FrontEndBuiltinRecord<'arena>] {
        &self.front_end_builtins
    }

    /// Every attribute in the unit, grouped by applied versus preserved.
    #[must_use]
    pub fn attribute_inventory(&self) -> AttributeInventory {
        let mut inventory = AttributeInventory::default();
        for declaration in self.iter() {
            inventory_of_declaration(declaration, &mut inventory);
        }
        for attribute in &self.detached_attributes {
            inventory.record(attribute);
        }
        inventory
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
// Tests
// ---------------------------------------------------------------------------

fn inventory_of_declaration(declaration: &CHirDecl<'_>, inventory: &mut AttributeInventory) {
    match declaration {
        CHirDecl::Asm(_) => {}
        CHirDecl::Function(function) => {
            inventory_of_written(&function.written, inventory);
            for attribute in &function.attributes {
                inventory.record(attribute);
            }
            if let ParameterSyntax::Prototype { parameters, .. } = &function.parameters {
                for parameter in parameters {
                    inventory_of_written(&parameter.written, inventory);
                    for attribute in &parameter.attributes {
                        inventory.record(attribute);
                    }
                }
            }
            if let Some(body) = function.body {
                inventory_of_statement(body, inventory);
            }
        }
        CHirDecl::Object(object) => {
            inventory_of_written(&object.written, inventory);
            for attribute in &object.attributes {
                inventory.record(attribute);
            }
            if let Some(initializer) = &object.initializer {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } = action
                    {
                        inventory_of_expression(value, inventory);
                    }
                }
            }
        }
        CHirDecl::Typedef(typedef) => {
            inventory_of_written(&typedef.written, inventory);
            for attribute in &typedef.attributes {
                inventory.record(attribute);
            }
        }
        CHirDecl::Tag(tag) => {
            for attribute in &tag.attributes {
                inventory.record(attribute);
            }
            match &tag.members {
                TagMembers::Struct(members) | TagMembers::Union(members) => {
                    for member in members {
                        inventory_of_written(&member.written, inventory);
                        for attribute in &member.attributes {
                            inventory.record(attribute);
                        }
                        if let Some(width) = member.bit_width {
                            inventory_of_expression(width, inventory);
                        }
                    }
                }
                TagMembers::Enum(enumerators) => {
                    for enumerator in enumerators {
                        for attribute in &enumerator.attributes {
                            inventory.record(attribute);
                        }
                        if let Some(value) = enumerator.written_value {
                            inventory_of_expression(value, inventory);
                        }
                    }
                }
                TagMembers::Incomplete => {}
            }
        }
    }
}

fn inventory_of_statement(statement: &CHirStmt<'_>, inventory: &mut AttributeInventory) {
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                inventory_of_expression(operand.expression, inventory);
            }
        }
        CHirStmt::Expr(expression, _)
        | CHirStmt::Return(Some(expression), _)
        | CHirStmt::ComputedGoto(expression, _) => inventory_of_expression(expression, inventory),
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            inventory_of_expression(condition, inventory);
            inventory_of_statement(then_statement, inventory);
            if let Some(else_statement) = else_statement {
                inventory_of_statement(else_statement, inventory);
            }
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            inventory_of_expression(expression, inventory);
            inventory_of_statement(body, inventory);
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            inventory_of_expression(first, inventory);
            inventory_of_expression(last, inventory);
            inventory_of_statement(body, inventory);
        }
        CHirStmt::DoWhile(body, condition, _) => {
            inventory_of_statement(body, inventory);
            inventory_of_expression(condition, inventory);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => {
                    inventory_of_expression(expression, inventory);
                }
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        inventory_of_declaration(declaration, inventory);
                    }
                }
            }
            if let Some(condition) = condition {
                inventory_of_expression(condition, inventory);
            }
            if let Some(step) = step {
                inventory_of_expression(step, inventory);
            }
            inventory_of_statement(body, inventory);
        }
        CHirStmt::Compound(items, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        inventory_of_declaration(declaration, inventory);
                    }
                    CHirBlockItem::Statement(statement) => {
                        inventory_of_statement(statement, inventory);
                    }
                }
            }
        }
        CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => {
            inventory_of_statement(body, inventory);
        }
        CHirStmt::AttributedNull(attributes, _) => {
            for attribute in attributes {
                inventory.record(attribute);
            }
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::Null(_) => {}
    }
}

fn inventory_of_expression(expression: &CHirExpr<'_>, inventory: &mut AttributeInventory) {
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
        } => inventory_of_expression(operand, inventory),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::Assign {
            target: left,
            value: right,
            ..
        } => {
            inventory_of_expression(left, inventory);
            inventory_of_expression(right, inventory);
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            inventory_of_expression(condition, inventory);
            inventory_of_expression(when_true, inventory);
            inventory_of_expression(when_false, inventory);
        }
        CHirExpr::Call(callee, arguments, _) => {
            inventory_of_expression(callee, inventory);
            for argument in arguments {
                inventory_of_expression(argument, inventory);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            inventory_of_expression(lhs, inventory);
            inventory_of_expression(rhs, inventory);
            inventory_of_expression(target, inventory);
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            inventory_of_expression(first, inventory);
            if let Some(second) = second {
                inventory_of_expression(second, inventory);
            }
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            for source in place.source_expressions() {
                inventory_of_expression(source, inventory);
            }
            inventory_of_expression(value, inventory);
        }
        CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
            for source in place.source_expressions() {
                inventory_of_expression(source, inventory);
            }
        }
        CHirExpr::StatementExpr { body, .. } => inventory_of_statement(body, inventory),
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    inventory_of_expression(value, inventory);
                }
            }
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
        | CHirExpr::BuiltinFrameAddress { .. } => {}
    }
}

fn inventory_of_written(written: &WrittenDeclarator, inventory: &mut AttributeInventory) {
    for layer in &written.layers {
        if let WrittenDeclaratorLayer::Pointer { attributes, .. } = layer {
            for attribute in attributes {
                inventory.record(attribute);
            }
        }
    }
}

fn render_attribute_lines(out: &mut String, attributes: &[Attribute]) {
    use std::fmt::Write as _;

    for attribute in attributes {
        let arguments = attribute
            .arguments
            .iter()
            .map(|argument| argument.text.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let applied = match attribute.applied {
            Some(AppliedAttribute::Aligned(bytes)) => format!("applied aligned={bytes}"),
            Some(AppliedAttribute::Packed) => "applied packed".to_owned(),
            None => "preserved-not-applied".to_owned(),
        };
        let _ = writeln!(
            out,
            "  attribute {}({arguments}) placement={} {applied}",
            attribute.name,
            attribute.placement.spelling()
        );
    }
}

/// A line-oriented summary of a unit for `--emit=c-hir`.
///
/// One line per declaration, one indented line per attribute naming its
/// canonical name, arguments as written, placement and whether the profile
/// applied it, then an inventory block. The point is that a reviewer can
/// confirm from this text alone that no attribute was dropped on the way in.
#[must_use]
pub fn render_unit_summary(unit: &CHirUnit<'_>) -> String {
    use std::fmt::Write as _;

    fn render_assembly(assembly: &AsmStmt<'_>, out: &mut String) {
        let mut qualifiers = [
            assembly
                .volatile_span
                .map(|span| (span.byte_start, "volatile")),
            assembly.inline_span.map(|span| (span.byte_start, "inline")),
            assembly.goto_span.map(|span| (span.byte_start, "goto")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        qualifiers.sort_unstable_by_key(|qualifier| qualifier.0);
        let qualifier_text = qualifiers
            .iter()
            .map(|(_, spelling)| *spelling)
            .collect::<Vec<_>>()
            .join(" ");
        if qualifier_text.is_empty() {
            let _ = writeln!(out, "asm {:?}", assembly.template);
        } else {
            let _ = writeln!(out, "asm {qualifier_text} {:?}", assembly.template);
        }
        for operand in &assembly.outputs {
            let symbolic = operand
                .symbolic
                .as_ref()
                .map_or_else(String::new, |name| format!("[{name}] "));
            let _ = writeln!(
                out,
                "  output {symbolic}{:?} expression={}..{}",
                operand.constraint,
                operand.expression.span().byte_start,
                operand.expression.span().byte_end
            );
        }
        for operand in &assembly.inputs {
            let symbolic = operand
                .symbolic
                .as_ref()
                .map_or_else(String::new, |name| format!("[{name}] "));
            let _ = writeln!(
                out,
                "  input {symbolic}{:?} expression={}..{}",
                operand.constraint,
                operand.expression.span().byte_start,
                operand.expression.span().byte_end
            );
        }
        for clobber in &assembly.clobbers {
            let _ = writeln!(out, "  clobber {clobber:?}");
        }
        for label in &assembly.labels {
            let _ = writeln!(out, "  label {}", label.spelling);
        }
    }

    fn checked_expression(
        expression: &CHirExpr<'_>,
        out: &mut String,
        seen: &mut std::collections::BTreeSet<(u32, u32, u32)>,
    ) {
        if let CHirExpr::CheckedArithmetic {
            op,
            lhs,
            rhs,
            target,
            target_ty,
            span,
            ..
        } = expression
        {
            let identity = (span.file.raw(), span.byte_start, span.byte_end);
            if seen.insert(identity) {
                let operation = match op {
                    CheckedArithmeticOp::Add => "add",
                    CheckedArithmeticOp::Sub => "sub",
                    CheckedArithmeticOp::Mul => "mul",
                };
                let _ = writeln!(
                    out,
                    "checked {operation}({}..{}, {}..{}) -> *{}..{} target-type={}",
                    lhs.span().byte_start,
                    lhs.span().byte_end,
                    rhs.span().byte_start,
                    rhs.span().byte_end,
                    target.span().byte_start,
                    target.span().byte_end,
                    target_ty.raw()
                );
            }
        }
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
            } => checked_expression(operand, out, seen),
            CHirExpr::Binary(_, lhs, rhs, _)
            | CHirExpr::Index(lhs, rhs, _)
            | CHirExpr::Comma(lhs, rhs, _)
            | CHirExpr::Elvis(lhs, rhs, _)
            | CHirExpr::Assign {
                target: lhs,
                value: rhs,
                ..
            } => {
                checked_expression(lhs, out, seen);
                checked_expression(rhs, out, seen);
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => {
                checked_expression(condition, out, seen);
                checked_expression(when_true, out, seen);
                checked_expression(when_false, out, seen);
            }
            CHirExpr::Call(callee, arguments, _) => {
                checked_expression(callee, out, seen);
                for argument in arguments {
                    checked_expression(argument, out, seen);
                }
            }
            CHirExpr::CheckedArithmetic {
                lhs, rhs, target, ..
            } => {
                checked_expression(lhs, out, seen);
                checked_expression(rhs, out, seen);
                checked_expression(target, out, seen);
            }
            CHirExpr::BuiltinVarArgs { first, second, .. } => {
                checked_expression(first, out, seen);
                if let Some(second) = second {
                    checked_expression(second, out, seen);
                }
            }
            CHirExpr::MemoryAssign { place, value, .. } => {
                for source in place.source_expressions() {
                    checked_expression(source, out, seen);
                }
                checked_expression(value, out, seen);
            }
            CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
                for source in place.source_expressions() {
                    checked_expression(source, out, seen);
                }
            }
            CHirExpr::StatementExpr { body, .. } => checked_statement(body, out, seen),
            CHirExpr::CompoundLit(_, initializer, _) => {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. } = action {
                        checked_expression(value, out, seen);
                    }
                }
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
            | CHirExpr::BuiltinFrameAddress { .. } => {}
        }
    }

    fn checked_declaration(
        declaration: &CHirDecl<'_>,
        out: &mut String,
        seen: &mut std::collections::BTreeSet<(u32, u32, u32)>,
    ) {
        match declaration {
            CHirDecl::Asm(assembly) => {
                render_assembly(assembly, out);
                for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                    checked_expression(operand.expression, out, seen);
                }
            }
            CHirDecl::Object(object) => {
                if let Some(initializer) = &object.initializer {
                    for action in initializer.plan().actions() {
                        if let InitAction::StoreScalar { value, .. } = action {
                            checked_expression(value, out, seen);
                        }
                    }
                }
            }
            CHirDecl::Function(function) => {
                if let Some(body) = function.body {
                    checked_statement(body, out, seen);
                }
            }
            CHirDecl::Tag(tag) => match &tag.members {
                TagMembers::Struct(members) | TagMembers::Union(members) => {
                    for member in members {
                        if let Some(width) = member.bit_width {
                            checked_expression(width, out, seen);
                        }
                    }
                }
                TagMembers::Enum(enumerators) => {
                    for enumerator in enumerators {
                        if let Some(value) = enumerator.written_value {
                            checked_expression(value, out, seen);
                        }
                    }
                }
                TagMembers::Incomplete => {}
            },
            CHirDecl::Typedef(_) => {}
        }
    }

    fn checked_statement(
        statement: &CHirStmt<'_>,
        out: &mut String,
        seen: &mut std::collections::BTreeSet<(u32, u32, u32)>,
    ) {
        match statement {
            CHirStmt::Asm(assembly) => {
                render_assembly(assembly, out);
                for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                    checked_expression(operand.expression, out, seen);
                }
            }
            CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
                checked_expression(expression, out, seen);
            }
            CHirStmt::ComputedGoto(expression, _) => {
                checked_expression(expression, out, seen);
            }
            CHirStmt::If(condition, then_statement, else_statement, _) => {
                checked_expression(condition, out, seen);
                checked_statement(then_statement, out, seen);
                if let Some(else_statement) = else_statement {
                    checked_statement(else_statement, out, seen);
                }
            }
            CHirStmt::Switch(expression, body, _)
            | CHirStmt::While(expression, body, _)
            | CHirStmt::Case(expression, body, _) => {
                checked_expression(expression, out, seen);
                checked_statement(body, out, seen);
            }
            CHirStmt::CaseRange(first, last, body, _) => {
                checked_expression(first, out, seen);
                checked_expression(last, out, seen);
                checked_statement(body, out, seen);
            }
            CHirStmt::DoWhile(body, condition, _) => {
                checked_statement(body, out, seen);
                checked_expression(condition, out, seen);
            }
            CHirStmt::For(initializer, condition, step, body, _) => {
                match initializer {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(expression) => {
                        checked_expression(expression, out, seen);
                    }
                    CHirForInit::Declaration(declarations) => {
                        for declaration in declarations {
                            checked_declaration(declaration, out, seen);
                        }
                    }
                }
                if let Some(condition) = condition {
                    checked_expression(condition, out, seen);
                }
                if let Some(step) = step {
                    checked_expression(step, out, seen);
                }
                checked_statement(body, out, seen);
            }
            CHirStmt::Compound(items, _) => {
                for item in items {
                    match item {
                        CHirBlockItem::Declaration(declaration) => {
                            checked_declaration(declaration, out, seen);
                        }
                        CHirBlockItem::Statement(statement) => {
                            checked_statement(statement, out, seen);
                        }
                    }
                }
            }
            CHirStmt::Label(_, child, _) | CHirStmt::Default(child, _) => {
                checked_statement(child, out, seen);
            }
            CHirStmt::AttributedNull(attributes, _) => {
                render_attribute_lines(out, attributes);
            }
            CHirStmt::Return(None, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::Null(_) => {}
        }
    }

    let mut out = String::new();
    let written_attribute_lines = |out: &mut String, written: &WrittenDeclarator| {
        for layer in &written.layers {
            if let WrittenDeclaratorLayer::Pointer { attributes, .. } = layer {
                render_attribute_lines(out, attributes);
            }
        }
    };
    for declaration in unit.iter() {
        match declaration {
            CHirDecl::Asm(_) => {}
            CHirDecl::Function(function) => {
                let _ = writeln!(out, "function {}", function.name.spelling);
                if let Some(label) = &function.asm_label {
                    let _ = writeln!(out, "  asm-label {label:?}");
                }
                written_attribute_lines(&mut out, &function.written);
                render_attribute_lines(&mut out, &function.attributes);
                if let ParameterSyntax::Prototype { parameters, .. } = &function.parameters {
                    for parameter in parameters {
                        if let Some(name) = &parameter.name {
                            let _ = writeln!(out, "  parameter {}", name.spelling);
                        }
                        written_attribute_lines(&mut out, &parameter.written);
                        render_attribute_lines(&mut out, &parameter.attributes);
                    }
                }
            }
            CHirDecl::Object(object) => {
                let _ = writeln!(out, "object {}", object.name.spelling);
                if let Some(label) = &object.asm_label {
                    let _ = writeln!(out, "  asm-label {label:?}");
                }
                written_attribute_lines(&mut out, &object.written);
                render_attribute_lines(&mut out, &object.attributes);
            }
            CHirDecl::Typedef(typedef) => {
                let _ = writeln!(out, "typedef {}", typedef.name.spelling);
                written_attribute_lines(&mut out, &typedef.written);
                render_attribute_lines(&mut out, &typedef.attributes);
            }
            CHirDecl::Tag(tag) => {
                let name = tag
                    .name
                    .as_ref()
                    .map_or("<anonymous>", |name| name.spelling.as_str());
                let _ = writeln!(out, "tag {name}");
                render_attribute_lines(&mut out, &tag.attributes);
                match &tag.members {
                    TagMembers::Struct(members) | TagMembers::Union(members) => {
                        for member in members {
                            let name = member
                                .name
                                .as_ref()
                                .map_or("<anonymous>", |name| name.spelling.as_str());
                            let _ = writeln!(out, "  member {name}");
                            written_attribute_lines(&mut out, &member.written);
                            render_attribute_lines(&mut out, &member.attributes);
                        }
                    }
                    TagMembers::Enum(enumerators) => {
                        for enumerator in enumerators {
                            let _ = writeln!(out, "  enumerator {}", enumerator.name.spelling);
                            render_attribute_lines(&mut out, &enumerator.attributes);
                        }
                    }
                    TagMembers::Incomplete => {}
                }
            }
        }
    }
    for attribute in unit.detached_attributes() {
        let _ = writeln!(out, "detached");
        render_attribute_lines(&mut out, std::slice::from_ref(attribute));
    }
    for selection in unit.generic_selections() {
        let _ = writeln!(
            out,
            "generic-selection selected={} associations={}",
            selection.selected_index, selection.association_count
        );
    }
    for builtin in unit.front_end_builtins() {
        match builtin {
            FrontEndBuiltinRecord::Expect {
                span,
                value,
                expected,
            } => {
                let _ = writeln!(
                    out,
                    "builtin-expect span={}..{} value={}..{} expected={}..{}",
                    span.byte_start,
                    span.byte_end,
                    value.span().byte_start,
                    value.span().byte_end,
                    expected.span().byte_start,
                    expected.span().byte_end
                );
            }
            FrontEndBuiltinRecord::ConstantP {
                span,
                operand,
                result,
            } => {
                let _ = writeln!(
                    out,
                    "builtin-constant-p span={}..{} operand={}..{} result={}",
                    span.byte_start,
                    span.byte_end,
                    operand.span().byte_start,
                    operand.span().byte_end,
                    u8::from(*result)
                );
            }
            FrontEndBuiltinRecord::ObjectSize {
                span,
                operand,
                mode,
                result,
            } => {
                let _ = writeln!(
                    out,
                    "builtin-object-size span={}..{} operand={}..{} mode={mode} result={result}",
                    span.byte_start,
                    span.byte_end,
                    operand.span().byte_start,
                    operand.span().byte_end,
                );
            }
            FrontEndBuiltinRecord::HasAttribute {
                span,
                operand,
                attribute,
                result,
            } => {
                let operand = operand.map_or_else(
                    || "type-name".to_owned(),
                    |operand| format!("{}..{}", operand.span().byte_start, operand.span().byte_end),
                );
                let _ = writeln!(
                    out,
                    "builtin-has-attribute span={}..{} operand={operand} attribute={attribute} result={}",
                    span.byte_start,
                    span.byte_end,
                    u8::from(*result)
                );
            }
        }
    }
    let mut rendered_checked = std::collections::BTreeSet::new();
    for declaration in unit.iter() {
        checked_declaration(declaration, &mut out, &mut rendered_checked);
    }
    let inventory = unit.attribute_inventory();
    for (name, count) in &inventory.applied {
        let _ = writeln!(out, "applied: {name} x{count}");
    }
    for (name, count) in &inventory.preserved_not_applied {
        let _ = writeln!(out, "preserved-not-applied: {name} x{count}");
    }
    out
}

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
        let initializer = initializer.map(|expression| {
            ObjectInitializer::new(
                WrittenInitializer::Expression(expression),
                InitPlan::new(
                    ty,
                    vec![InitAction::store_scalar(
                        SubobjectPath::root(),
                        ty,
                        expression,
                        expression.span(),
                    )],
                ),
            )
        });
        CHirDecl::Object(ObjectDecl {
            attributes: Vec::new(),
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
            asm_label: None,
            asm_label_span: None,
            gnu_extension_spans: Vec::new(),
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
        let mut symbols = SymbolTable::new();
        let place_symbol = symbols
            .declare(
                symbols.file_scope(),
                "x",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                int_id,
                s,
            )
            .unwrap();
        let place = ScalarPlace::bind(&symbols, &types, place_symbol).unwrap();
        let value_facts = ExprFacts::value(int_id);
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
                CHirExpr::StringLit(StringLit::single("hi", StringPrefix::None), s),
            ),
            arena_expr(&arena, ident("a", int_id)),
            arena_expr(&arena, CHirExpr::Unary(UnaryOp::Neg, id, s)),
            arena_expr(&arena, CHirExpr::Binary(BinaryOp::Add, id, id, s)),
            arena_expr(&arena, CHirExpr::Ternary(id, id, id, s)),
            arena_expr(&arena, CHirExpr::Call(id, vec![id], s)),
            arena_expr(&arena, CHirExpr::Cast(int_id, id, s)),
            arena_expr(&arena, CHirExpr::SizeOf(int_id, s)),
            arena_expr(&arena, CHirExpr::SizeOfExpr(id, int_id, s)),
            arena_expr(&arena, CHirExpr::AlignOf(int_id, s)),
            arena_expr(&arena, CHirExpr::Member(id, "f".into(), s)),
            arena_expr(&arena, CHirExpr::PtrMember(id, "f".into(), s)),
            arena_expr(&arena, CHirExpr::Index(id, id, s)),
            arena_expr(
                &arena,
                CHirExpr::Assign {
                    op: AssignmentOp::Simple,
                    place,
                    target: id,
                    value: id,
                    facts: value_facts,
                    span: s,
                },
            ),
            arena_expr(
                &arena,
                CHirExpr::Update {
                    op: UpdateOp::PostIncrement,
                    place,
                    target: id,
                    facts: value_facts,
                    span: s,
                },
            ),
            arena_expr(
                &arena,
                CHirExpr::CompoundLit(
                    int_id,
                    ObjectInitializer::new(
                        WrittenInitializer::Expression(id),
                        InitPlan::new(
                            int_id,
                            vec![InitAction::store_scalar(
                                SubobjectPath::root(),
                                int_id,
                                id,
                                s,
                            )],
                        ),
                    ),
                    s,
                ),
            ),
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
                    attributes: Vec::new(),
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
                    asm_label: None,
                    asm_label_span: None,
                    inline_spans: Vec::new(),
                    gnu_extension_spans: Vec::new(),
                    body: Some(func_body),
                    span: s,
                }),
            ),
            arena_decl(
                &arena,
                CHirDecl::Tag(TagDecl {
                    attributes: Vec::new(),
                    tag: tag_id,
                    scope: SymbolTable::new().file_scope(),
                    name: Some(name("S")),
                    members: TagMembers::Incomplete,
                    span: s,
                    specifier_span: s,
                    gnu_extension_spans: Vec::new(),
                }),
            ),
            arena_decl(
                &arena,
                CHirDecl::Typedef(TypedefDecl {
                    attributes: Vec::new(),
                    symbol: test_symbol("T", int_id, SymbolData::Typedef),
                    name: name("T"),
                    ty: int_id,
                    specifier: builtin_specifier(int_id),
                    written: written_declarator(),
                    gnu_extension_spans: Vec::new(),
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

    #[test]
    fn string_literal_fragment_integrity_fails_closed() {
        let forged = StringLit {
            value: "long".to_owned(),
            prefix: StringPrefix::None,
            fragments: vec!["x".to_owned()],
        };
        assert_eq!(forged.explicit_unit_count(), None);

        let separated = StringLit {
            value: "\\x12".to_owned(),
            prefix: StringPrefix::None,
            fragments: vec!["\\x1".to_owned(), "2".to_owned()],
        };
        assert_eq!(separated.explicit_unit_count(), Some(2));

        assert_eq!(
            StringLit::single("\\u0061", StringPrefix::None).explicit_unit_count(),
            None,
        );
    }
}
