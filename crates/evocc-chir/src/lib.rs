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
pub mod initializer;
pub mod integer;
pub mod interp;
pub mod place;
pub mod semantic;

pub use allocation::{
    AllocationSite, AllocationSiteKind, AllocationSiteManifest, AllocationSiteManifestError,
    AllocationSiteOrdinal,
};
pub use initializer::{
    InitAction, InitPlan, InitPlanError, ObjectInitializer, SubobjectPath, SubobjectStep,
    WrittenInitializer,
};
pub use place::{
    FieldProjection, IndexProjection, MemoryPlace, MemoryPlaceError, PlaceProjection, PlaceRoot,
    PointerTypeDesc,
};
pub use semantic::{
    AssignmentOp, ExprCategory, ExprFacts, ExprFactsError, NotAssignmentOperator, ScalarPlace,
    ScalarPlaceError, Scope, ScopeId, ScopeKind, Symbol, SymbolData, SymbolId, SymbolKind,
    SymbolTable, SymbolTableCheckpoint, SymbolTableError, TagBinding, UpdateEffect, UpdateOp,
    UpdatePlan,
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
        StringPrefix::L | StringPrefix::U => output.push(u32::from(character)),
    }
}

fn push_numeric_unit(output: &mut Vec<u32>, value: u32, prefix: StringPrefix) -> Option<()> {
    let maximum = match prefix {
        StringPrefix::None | StringPrefix::U8 => u32::from(u8::MAX),
        StringPrefix::U16 => u32::from(u16::MAX),
        StringPrefix::L | StringPrefix::U => u32::MAX,
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
            | Self::Assign { span, .. }
            | Self::MemoryAssign { span, .. }
            | Self::MemoryRead { span, .. }
            | Self::Update { span, .. }
            | Self::MemoryUpdate { span, .. }
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
    /// `typeof(...)`, resolved completely by the front end while retaining
    /// whether the source operand was a type name or an expression.
    Typeof {
        resolved: TypeId,
        operand: TypeofOperand,
        span: SourceSpan,
    },
}

impl TypeSpecifier {
    #[must_use]
    pub fn type_id(&self) -> TypeId {
        match self {
            Self::Builtin { ty, .. } | Self::TypedefName { ty, .. } | Self::Tag { ty, .. } => *ty,
            Self::Typeof { resolved, .. } => *resolved,
        }
    }

    #[must_use]
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Builtin { span, .. }
            | Self::TypedefName { span, .. }
            | Self::Tag { span, .. }
            | Self::Typeof { span, .. } => *span,
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
    /// After an enumerator.
    Enumerator,
    /// In a type name.
    TypeName,
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
            Self::Enumerator => "enumerator",
            Self::TypeName => "type-name",
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
    pub value: i64,
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
    detached_attributes: Vec<Attribute>,
    generic_selections: Vec<GenericSelectionRecord>,
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
        }
    }

    /// Attach the attributes that had no declaration node: labels and type
    /// names. Called once by the parser after the unit is assembled.
    pub fn set_detached_attributes(&mut self, attributes: Vec<Attribute>) {
        self.detached_attributes = attributes;
    }

    pub fn set_generic_selections(&mut self, selections: Vec<GenericSelectionRecord>) {
        self.generic_selections = selections;
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
        CHirDecl::Function(function) => {
            for attribute in &function.attributes {
                inventory.record(attribute);
            }
            if let ParameterSyntax::Prototype { parameters, .. } = &function.parameters {
                for parameter in parameters {
                    for attribute in &parameter.attributes {
                        inventory.record(attribute);
                    }
                }
            }
        }
        CHirDecl::Object(object) => {
            for attribute in &object.attributes {
                inventory.record(attribute);
            }
        }
        CHirDecl::Typedef(typedef) => {
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
                        for attribute in &member.attributes {
                            inventory.record(attribute);
                        }
                    }
                }
                TagMembers::Enum(enumerators) => {
                    for enumerator in enumerators {
                        for attribute in &enumerator.attributes {
                            inventory.record(attribute);
                        }
                    }
                }
                TagMembers::Incomplete => {}
            }
        }
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
    let mut out = String::new();
    let attribute_lines = |out: &mut String, attributes: &[Attribute]| {
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
    };
    for declaration in unit.iter() {
        match declaration {
            CHirDecl::Function(function) => {
                let _ = writeln!(out, "function {}", function.name.spelling);
                attribute_lines(&mut out, &function.attributes);
                if let ParameterSyntax::Prototype { parameters, .. } = &function.parameters {
                    for parameter in parameters {
                        if let Some(name) = &parameter.name {
                            let _ = writeln!(out, "  parameter {}", name.spelling);
                        }
                        attribute_lines(&mut out, &parameter.attributes);
                    }
                }
            }
            CHirDecl::Object(object) => {
                let _ = writeln!(out, "object {}", object.name.spelling);
                attribute_lines(&mut out, &object.attributes);
            }
            CHirDecl::Typedef(typedef) => {
                let _ = writeln!(out, "typedef {}", typedef.name.spelling);
                attribute_lines(&mut out, &typedef.attributes);
            }
            CHirDecl::Tag(tag) => {
                let name = tag
                    .name
                    .as_ref()
                    .map_or("<anonymous>", |name| name.spelling.as_str());
                let _ = writeln!(out, "tag {name}");
                attribute_lines(&mut out, &tag.attributes);
                match &tag.members {
                    TagMembers::Struct(members) | TagMembers::Union(members) => {
                        for member in members {
                            let name = member
                                .name
                                .as_ref()
                                .map_or("<anonymous>", |name| name.spelling.as_str());
                            let _ = writeln!(out, "  member {name}");
                            attribute_lines(&mut out, &member.attributes);
                        }
                    }
                    TagMembers::Enum(enumerators) => {
                        for enumerator in enumerators {
                            let _ = writeln!(out, "  enumerator {}", enumerator.name.spelling);
                            attribute_lines(&mut out, &enumerator.attributes);
                        }
                    }
                    TagMembers::Incomplete => {}
                }
            }
        }
    }
    for attribute in unit.detached_attributes() {
        let _ = writeln!(out, "detached");
        attribute_lines(&mut out, std::slice::from_ref(attribute));
    }
    for selection in unit.generic_selections() {
        let _ = writeln!(
            out,
            "generic-selection selected={} associations={}",
            selection.selected_index, selection.association_count
        );
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
            arena_expr(&arena, CHirExpr::SizeOfExpr(id, s)),
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
