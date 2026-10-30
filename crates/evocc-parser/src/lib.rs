//! C declarator, expression, and statement parser producing C-HIR

mod assembly;
mod assignments;
mod atomic_builtins;
mod attributes;
mod builtin_calls;
mod builtin_expressions;
mod builtin_profiles;
mod builtin_values;
pub mod builtins;
mod complex;
mod configuration;
mod conversions;
mod cursor;
mod declaration_support;
mod declarations;
mod declarators;
mod enums;
mod expression_support;
mod expression_types;
mod floating;
mod functions;
mod initializer_actions;
mod initializer_paths;
mod initializers;
mod jump_labels;
mod lexical_values;
mod neon;
mod operators;
mod primary;
mod records;
mod specifiers;
mod statements;
mod string_constants;
mod symbols;
#[cfg(test)]
mod tests;
mod tgmath;
mod vector;

use builtin_profiles::*;
use declaration_support::*;
use expression_support::*;
use jump_labels::*;
use lexical_values::*;

/// A GNU `__atomic` builtin beyond `__atomic_load_n` and `__atomic_store_n`
/// (EvoIR 10.1.14). Fences are library builtins with an `int` operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GnuAtomic {
    Update(evocc_chir::AtomicRmwOp, evocc_chir::AtomicRmwReturn),
    CompareExchange,
    TestAndSet,
    Clear,
    AlwaysLockFree,
    IsLockFree,
    /// `__atomic_load`, `__atomic_store`, `__atomic_exchange` and
    /// `__atomic_compare_exchange`, whose operands point to buffers.
    Generic(evocc_chir::AtomicCopyOp),
}

/// Clang's `__c11_atomic_*` builtins that the Darwin `<stdatomic.h>` uses
/// (EvoIR 10.1.21), except the two fences, which are library builtins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClangAtomic {
    Init,
    Load,
    Store,
    Exchange,
    /// `compare_exchange_strong` and `compare_exchange_weak`; a weak request
    /// is strong.
    CompareExchange,
    Fetch(evocc_chir::AtomicRmwOp),
    IsLockFree,
}

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;

use evocc_chir::integer::{
    enum_integer_type, eval_binary_integer, eval_bit_builtin, eval_constant_binary_integer,
    eval_constant_unary_integer, floating_literal_to_integer, integer_literal_value,
    integer_type_for_rank, rank_for_integer_type, usual_arithmetic_type, BitBuiltinOp, CInteger,
    CIntegerType,
};
use evocc_chir::{
    attribute_query_target, builtin_constant_p, is_modifiable_object_type, record_member_path,
    statement_expression_value, AppliedAttribute, AsmOperand, AsmStmt, AssignmentOp, Attribute,
    AttributeArgument, AttributePlacement, AttributeQueryTarget, AttributeSyntax, BinaryOp,
    CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, CHirUnit, CharLit, CharPrefix,
    CheckedArithmeticOp, EnumeratorDecl, ExprCategory, ExprFacts, FieldProjection, FloatLit,
    FloatSuffix, FrontEndBuiltinRecord, FunctionDecl, GenericSelectionRecord, IndexProjection,
    InitAction, InitPlan, InitializerDesignation, IntBase, IntLit, IntSuffix, MemoryPlace,
    ObjectDecl, ObjectInitializer, ParameterDecl, ParameterSyntax, PlaceProjection, PlaceRoot,
    PointerTypeDesc, RecordMember, ScalarPlace, ScopeId, ScopeKind, SpannedName, StorageClass,
    StringLit, StringPrefix, SubobjectPath, SubobjectStep, SymbolData, SymbolId, SymbolKind,
    SymbolTable, SymbolTableError, TagDecl, TagMembers, TypeSpecifier, TypedefDecl, TypeofOperand,
    UnaryOp, UpdateOp, VarArgsBuiltinOp, WrittenDeclarator, WrittenDeclaratorLayer,
    WrittenDesignator, WrittenInitializer,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_cpp::availability::{self, Availability};
use evocc_cpp::{
    pragma_ms_struct_action, pragma_pack_action, pragma_visibility_action, PragmaPackAction,
    PragmaVisibilityAction,
};
use evocc_lex::{Token, TokenKind, TokenText};
use evocc_types::{
    ArrayBound, BitFieldSpec, CanonicalType, ComplexBase, EnumConstant, FieldLayoutAttributes,
    FloatKind, FunctionParameters, GnuInt128Signedness, IntRank, Qualifiers, RecordField,
    TagDefinition, TagId, TagKind, TagLayoutAttributes, TagState, TargetBuiltinType,
    TargetDataLayout, TypeId, TypeInterner, TypeKind,
};

#[derive(Debug, Clone)]
pub struct ParseDiagnostic {
    pub span: SourceSpan,
    pub previous_declaration: Option<SourceSpan>,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
enum DerivedDeclarator<'arena> {
    Pointer {
        qualifiers: Qualifiers,
        attributes: Vec<Attribute>,
        span: SourceSpan,
    },
    Array {
        bound: ArrayBound,
        expression: Option<&'arena CHirExpr<'arena>>,
        prototype_star: bool,
        parameter_qualifiers: Qualifiers,
        static_bound: bool,
        span: SourceSpan,
    },
    Function {
        parameters: ParameterSyntax,
        entry_bounds: Vec<&'arena CHirDecl<'arena>>,
        scope: ScopeId,
        span: SourceSpan,
    },
}

#[derive(Debug, Clone)]
struct ParsedDeclarator<'arena> {
    name: SpannedName,
    derived: Vec<DerivedDeclarator<'arena>>,
    attributes: Vec<Attribute>,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct ParsedAbstractDeclarator<'arena> {
    derived: Vec<DerivedDeclarator<'arena>>,
    attributes: Vec<Attribute>,
    span: SourceSpan,
}

struct ParsedParameters<'arena> {
    syntax: ParameterSyntax,
    entry_bounds: Vec<&'arena CHirDecl<'arena>>,
}

struct ParsedSpecifier<'arena> {
    surface: TypeSpecifier,
    tag_event: Option<PendingTag<'arena>>,
    inherited_function_parameters: Option<ParameterSyntax>,
}

struct PendingTag<'arena> {
    tag: TagId,
    scope: ScopeId,
    name: Option<SpannedName>,
    members: TagMembers<'arena>,
    specifier_span: SourceSpan,
    attributes: Vec<Attribute>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeclarationContext {
    File,
    Block,
    ForInitializer,
}

#[derive(Debug, Clone, Copy)]
struct DirectUpdateTarget {
    symbol: SymbolId,
    ty: TypeId,
}

#[derive(Debug, Clone)]
enum TypedUpdateTarget<'arena> {
    Direct(DirectUpdateTarget),
    Memory(MemoryPlace<'arena>),
}

impl TypedUpdateTarget<'_> {
    fn ty(&self) -> TypeId {
        match self {
            Self::Direct(target) => target.ty,
            Self::Memory(place) => place.value_ty(),
        }
    }
}

/// The pointed-to type of a converted pointer expression. `pointee` carries
/// structural identity while `qualifiers` may be the union formed by a
/// conditional expression even when no matching canonical TypeId exists yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InferredPointerType {
    pointee: TypeId,
    qualifiers: Qualifiers,
}

/// A side-effect-free expression-type fact used while validating update
/// operators. Synthetic literal/arithmetic types are kept out of the
/// canonical interner so semantic checking cannot perturb declaration TypeId
/// allocation or source-order fingerprints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InferredType {
    Canonical(TypeId),
    Integer(CIntegerType),
    Real(FloatKind),
    Complex(ComplexBase),
    Pointer(InferredPointerType),
    StringArray(StringPrefix),
    StringPointer(StringPrefix),
}

#[derive(Debug, Clone, Copy)]
struct InferredExpressionType {
    ty: InferredType,
    category: ExprCategory,
    null_pointer_constant: bool,
}

#[derive(Debug, Clone, Copy)]
enum OperandTypeClass {
    Integer(CIntegerType),
    WideInteger {
        ty: TypeId,
        signedness: GnuInt128Signedness,
    },
    Real(FloatKind),
    Complex(ComplexBase),
    Pointer(InferredPointerType),
    StringPointer(StringPrefix),
    Aggregate(TypeId),
    Void,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateConstraint {
    Scalar,
    PointerBoundary,
    AggregateBoundary,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitializerPreflightError {
    TooLarge,
    RecursiveObject,
    InvalidObject,
}

#[derive(Debug, Clone)]
struct ResolvedInitializerTarget {
    path: SubobjectPath,
    ty: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntityLinkage {
    None,
    Internal,
    External,
}

#[derive(Debug, Clone, Copy)]
struct EntityState {
    linkage: EntityLinkage,
    thread_local: bool,
    has_definition: bool,
    gnu_inline_body: bool,
    assembly_named: bool,
    /// The unit defines this function with an identifier list or an empty
    /// parameter list (C11 6.9.1p7).
    old_style_definition: bool,
}

#[derive(Debug)]
struct DeclarationTransaction {
    unprototyped_calls: usize,
    old_style_definitions: usize,
    prototype_scopes: usize,
    standard_allocation_calls: usize,
    entity_undo: usize,
    external_entity_undo: usize,
    inherited_function_parameter_undo: usize,
    entity_attribute_undo: usize,
    member_attribute_undo: usize,
    initialized_const_undo: usize,
    initialized_string_undo: usize,
    string_constant_uses: usize,
    function_uses: usize,
    function_definitions: usize,
    side_declarations: usize,
    builtin_declarations: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConventionState {
    Open,
    Accepted,
    Refused,
}

/// A GNU attribute of the EvoIR 10.1.9 list in a declaration position. Only
/// the declaration that receives it knows whether it declares a function.
#[derive(Debug, Clone, Copy)]
struct ConventionAttribute {
    /// The semantic attribute name, as the list spells it.
    attribute: &'static str,
    /// The complete attribute span, which the declaration receives.
    span: SourceSpan,
    /// The attribute name, where E2952 is reported.
    name: SourceSpan,
    preceding: usize,
    state: ConventionState,
}

/// The attributes that EvoIR 10.1.9 gives an effect on a declaration or a
/// type. After the `(` of a parenthesized declarator, GCC 14 attaches one to
/// the type formed at that point and Apple clang attaches some elsewhere, so
/// this profile refuses them there.
const PARENTHESIZED_EFFECT_ATTRIBUTES: [&str; 24] = [
    "aligned",
    "packed",
    "mode",
    "vector_size",
    "transparent_union",
    "weak",
    "weakref",
    "alias",
    "section",
    "used",
    "retain",
    "visibility",
    "cleanup",
    "tls_model",
    "uninitialized",
    "constructor",
    "destructor",
    "always_inline",
    "gnu_inline",
    "noinline",
    "noipa",
    "noclone",
    "patchable_function_entry",
    "no_stack_protector",
];

/// The ACLE intrinsics that Clang's `<arm_acle.h>` declares as functions.
const ACLE_FUNCTIONS: [&str; 8] = [
    "__dmb", "__dsb", "__isb", "__wfi", "__wfe", "__sev", "__sevl", "__yield",
];

/// The type of each ACLE builtin that `<arm_acle.h>` and `<arm64/mte.h>`
/// call, as Apple clang 21 types it: the result and the parameters. Clang
/// types `ldrex` and `strex` by their operands; the entries take any pointer.
/// A call of one is refused, so the type only keeps the unused header bodies
/// well formed (EvoIR 10.1.14).
const ACLE_BUILTINS: [(&str, &str, &[&str]); 40] = [
    ("__builtin_arm_chkfeat", "u64", &["u64"]),
    ("__builtin_arm_cls", "u32", &["u32"]),
    ("__builtin_arm_cls64", "u32", &["u64"]),
    ("__builtin_arm_clz", "u32", &["u32"]),
    ("__builtin_arm_clz64", "u32", &["u64"]),
    ("__builtin_arm_crc32b", "u32", &["u32", "u8"]),
    ("__builtin_arm_crc32cb", "u32", &["u32", "u8"]),
    ("__builtin_arm_crc32cd", "u32", &["u32", "u64"]),
    ("__builtin_arm_crc32ch", "u32", &["u32", "u16"]),
    ("__builtin_arm_crc32cw", "u32", &["u32", "u32"]),
    ("__builtin_arm_crc32d", "u32", &["u32", "u64"]),
    ("__builtin_arm_crc32h", "u32", &["u32", "u16"]),
    ("__builtin_arm_crc32w", "u32", &["u32", "u32"]),
    ("__builtin_arm_gcspopm", "u64", &["u64"]),
    ("__builtin_arm_gcsss", "void*", &["void*"]),
    ("__builtin_arm_gmi", "u64", &["void*", "u64"]),
    ("__builtin_arm_irg", "void*", &["void*", "u64"]),
    ("__builtin_arm_jcvt", "i32", &["f64"]),
    ("__builtin_arm_ld64b", "void", &["const void*", "u64*"]),
    ("__builtin_arm_ldg", "void*", &["void*"]),
    ("__builtin_arm_ldrex", "u64", &["const volatile void*"]),
    ("__builtin_arm_nop", "void", &[]),
    ("__builtin_arm_rbit", "u32", &["u32"]),
    ("__builtin_arm_rbit64", "u64", &["u64"]),
    ("__builtin_arm_rint32x", "f64", &["f64"]),
    ("__builtin_arm_rint32xf", "f32", &["f32"]),
    ("__builtin_arm_rint32z", "f64", &["f64"]),
    ("__builtin_arm_rint32zf", "f32", &["f32"]),
    ("__builtin_arm_rint64x", "f64", &["f64"]),
    ("__builtin_arm_rint64xf", "f32", &["f32"]),
    ("__builtin_arm_rint64z", "f64", &["f64"]),
    ("__builtin_arm_rint64zf", "f32", &["f32"]),
    ("__builtin_arm_rndr", "i32", &["u64*"]),
    ("__builtin_arm_rndrrs", "i32", &["u64*"]),
    ("__builtin_arm_rsr64", "u64", &["const char*"]),
    ("__builtin_arm_st64b", "void", &["void*", "const u64*"]),
    ("__builtin_arm_st64bv", "u64", &["void*", "const u64*"]),
    ("__builtin_arm_st64bv0", "u64", &["void*", "const u64*"]),
    ("__builtin_arm_stg", "void", &["void*"]),
    ("__builtin_arm_strex", "i32", &["u64", "volatile void*"]),
];

/// What kind of declaration settles a deferred list attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Settlement {
    /// A function declaration that is not a definition.
    Prototype,
    /// A function definition.
    Definition,
    /// Every other declaration, type name and member.
    Other,
}

/// The execution and calling-convention attributes of EvoIR 10.1.9, with the
/// E2952 message of each. The order is the order of the specification list.
/// `regparm` belongs to the list on Darwin only (`ignored_x86_convention`).
const CONVENTION_ATTRIBUTES: [(&str, &str); 22] = [
    ("naked", "unsupported-function-attribute: naked"),
    ("interrupt", "unsupported-function-attribute: interrupt"),
    (
        "interrupt_handler",
        "unsupported-function-attribute: interrupt_handler",
    ),
    ("pcs", "unsupported-function-attribute: pcs"),
    (
        "aarch64_vector_pcs",
        "unsupported-function-attribute: aarch64_vector_pcs",
    ),
    (
        "aarch64_sve_pcs",
        "unsupported-function-attribute: aarch64_sve_pcs",
    ),
    (
        "preserve_all",
        "unsupported-function-attribute: preserve_all",
    ),
    (
        "preserve_most",
        "unsupported-function-attribute: preserve_most",
    ),
    (
        "preserve_none",
        "unsupported-function-attribute: preserve_none",
    ),
    (
        "no_callee_saved_registers",
        "unsupported-function-attribute: no_callee_saved_registers",
    ),
    (
        "no_caller_saved_registers",
        "unsupported-function-attribute: no_caller_saved_registers",
    ),
    ("ms_abi", "unsupported-function-attribute: ms_abi"),
    ("sysv_abi", "unsupported-function-attribute: sysv_abi"),
    ("swiftcall", "unsupported-function-attribute: swiftcall"),
    (
        "swiftasynccall",
        "unsupported-function-attribute: swiftasynccall",
    ),
    ("regparm", "unsupported-function-attribute: regparm"),
    (
        "callee_pop_aggregate_return",
        "unsupported-function-attribute: callee_pop_aggregate_return",
    ),
    (
        "returns_twice",
        "unsupported-function-attribute: returns_twice",
    ),
    ("target", "unsupported-function-attribute: target"),
    (
        "target_clones",
        "unsupported-function-attribute: target_clones",
    ),
    (
        "target_version",
        "unsupported-function-attribute: target_version",
    ),
    ("optimize", "unsupported-function-attribute: optimize"),
];

/// The identity of a span: its file and byte range.
type SpanKey = (u32, u32, u32);

/// The list attributes of a unit after parsing: refusals of unsettled
/// attributes, the spans that a declaration accepted, and the definitions and
/// function designators in encounter order.
struct ConventionRecords {
    unsettled: Vec<(&'static str, SourceSpan, usize)>,
    accepted: HashSet<SpanKey>,
    definitions: Vec<FunctionUse>,
    uses: Vec<FunctionUse>,
}

/// A function designator or a function definition, in parse encounter order.
/// `owner` is the definition whose body contains a designator.
#[derive(Debug, Clone, Copy)]
struct FunctionUse {
    function: SymbolId,
    owner: Option<SymbolId>,
    span: SourceSpan,
    preceding: usize,
}

/// A profile refusal, the diagnostic count before it, and the expression
/// whose consumer can discharge it.
type DeferredRefusal = (
    evocc_chir::ProfileRefusal,
    usize,
    Option<BinaryExpressionIdentity>,
);

type AttributeNames = HashSet<String>;
type MemberAttributeKey = (TagId, usize);
type MemberAttributeUndo = (MemberAttributeKey, Option<AttributeNames>);

#[derive(Debug, Clone)]
struct TemporaryParameterShadow {
    scope: ScopeId,
    spelling: String,
    ty: TypeId,
    storage: StorageClass,
    attributes: Vec<Attribute>,
    span: SourceSpan,
}

#[derive(Debug)]
struct SwitchCaseContext {
    selector_type: Option<CIntegerType>,
    intervals: Vec<(i128, i128)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BinaryExpressionIdentity {
    operator: BinaryOp,
    left: usize,
    right: usize,
}

impl BinaryExpressionIdentity {
    fn of(expression: &CHirExpr<'_>) -> Option<Self> {
        let CHirExpr::Binary(operator, left, right, _) = expression else {
            return None;
        };
        // Parentheses clone the root with a different span, but preserve its
        // operands. Neither root addresses nor macro spans identify this use.
        Some(Self {
            operator: *operator,
            left: *left as *const CHirExpr<'_> as usize,
            right: *right as *const CHirExpr<'_> as usize,
        })
    }
}

/// The availability of one declared entity for a macOS deployment target,
/// merged over its declarations in source order (EvoIR 10.1.9).
#[derive(Debug, Clone, Default)]
struct DeclarationAvailability {
    merged: availability::Merged,
    /// The text of the first `unavailable` attribute's message; empty
    /// without one.
    unavailable: Option<String>,
    weak_import: bool,
}

impl DeclarationAvailability {
    /// Why a use is refused, or `None` when the entity is available.
    fn unavailable(&self, deployment: availability::Version) -> Option<String> {
        if let Some(message) = &self.unavailable {
            return Some(message.clone());
        }
        match self.merged.effect(deployment) {
            availability::Effect::Unavailable(reason) => Some(reason),
            _ => None,
        }
    }

    /// Whether a function or object declaration is a weak import.
    fn weak_import(&self, deployment: availability::Version) -> bool {
        self.weak_import || self.merged.effect(deployment) == availability::Effect::NotYetIntroduced
    }
}

/// A use of an unavailable entity, reported unless its context is
/// unavailable too.
#[derive(Debug, Clone)]
struct UnavailableUse {
    message: String,
    span: SourceSpan,
    preceding: usize,
}

/// A context of the availability rule: a declaration, a declarator, a
/// record or a function body. A use in a context whose entity is
/// unavailable is not refused, as in Apple clang.
#[derive(Debug, Clone)]
struct AvailabilityFrame {
    /// Whether the entity is unavailable; `None` until its attributes are
    /// complete.
    exempt: Option<bool>,
    /// The enclosing context of the rule. A record's is the enclosing
    /// function body, because a C record does not nest in another record.
    parent: Option<usize>,
    /// A function definition, whose body encloses local records.
    function: bool,
    pending: Vec<UnavailableUse>,
}

pub struct Parser<'src, 'arena> {
    target_layout: TargetDataLayout,
    general_registers_only: bool,
    /// The macOS deployment target of `aarch64-apple-darwin`. It gives the
    /// availability, `unavailable` and `weak_import` attributes their
    /// meaning; without it they are preserved without effect.
    availability_target: Option<availability::Version>,
    symbol_availability: HashMap<SymbolId, DeclarationAvailability>,
    tag_availability: HashMap<TagId, DeclarationAvailability>,
    member_availability: HashMap<MemberAttributeKey, DeclarationAvailability>,
    availability_frames: Vec<AvailabilityFrame>,
    standard_builtins: bool,
    /// `-fno-builtin-NAME`: ordinary names whose recognition is disabled.
    disabled_builtins: HashSet<String>,
    legacy_declarations: bool,
    lint_declarations: bool,
    lint_uses: HashSet<SymbolId>,
    lint_findings: Vec<evocc_chir::lints::Finding>,
    pointer_lint_findings: std::cell::RefCell<Vec<evocc_chir::lints::Finding>>,
    gnu_inline: bool,
    unprototyped_calls: Vec<(SymbolId, Vec<TypeId>, SourceSpan)>,
    /// Each old-style definition with its promoted parameter types.
    old_style_definitions: Vec<(SymbolId, Vec<TypeId>)>,
    /// Each declaration of a function with a parameter type list, with the
    /// scope that its prototype is visible in (C11 6.2.1p4).
    prototype_scopes: Vec<(SymbolId, ScopeId)>,
    standard_allocation_calls: Vec<(SymbolId, SourceSpan)>,
    tokens: Vec<Token>,
    pos: usize,
    file: FileId,
    _source_lifetime: std::marker::PhantomData<&'src str>,
    had_error: bool,
    error_count: usize,
    error_limit: Option<NonZeroUsize>,
    rollback_error_count: usize,
    diagnostics: Vec<ParseDiagnostic>,
    arena: Option<&'arena Arena>,
    types: TypeInterner,
    symbols: SymbolTable,
    current_scope: ScopeId,
    /// Parameter names whose C point of declaration has been reached while
    /// their prototype is still being parsed. They shadow outer typedef names
    /// immediately, but receive permanent SymbolIds only after the enclosing
    /// function declarator is complete, preserving declaration-point ID order.
    temporary_parameter_shadows: Vec<TemporaryParameterShadow>,
    /// Parameter syntax retained for symbols whose declared type is a
    /// function. Later declarations through a function-type typedef or
    /// `typeof(function-designator)` have no written function suffix of their
    /// own, so the named source declaration is their prototype authority.
    inherited_function_parameters: HashMap<SymbolId, ParameterSyntax>,
    /// Canonical names of attributes written on ordinary declarations. This
    /// is a parser-time index over the lossless C-HIR inventory, not a second
    /// semantic attribute model.
    entity_attributes: HashMap<SymbolId, AttributeNames>,
    /// Canonical names of attributes written on record members, keyed by the
    /// same nominal tag and field index used by `FieldProjection`.
    member_attributes: HashMap<MemberAttributeKey, AttributeNames>,
    entities: HashMap<SymbolId, EntityState>,
    external_entities: HashMap<String, SymbolId>,
    entity_undo: Vec<(SymbolId, Option<EntityState>)>,
    external_entity_undo: Vec<(String, Option<SymbolId>)>,
    inherited_function_parameter_undo: Vec<(SymbolId, Option<ParameterSyntax>)>,
    entity_attribute_undo: Vec<(SymbolId, Option<AttributeNames>)>,
    member_attribute_undo: Vec<MemberAttributeUndo>,
    /// Preceding explicitly initialized const integer objects. Only GNU
    /// static initialization consults this index; it never grants ICE status.
    initialized_const_integers: HashMap<SymbolId, CInteger>,
    initialized_const_undo: Vec<(SymbolId, Option<CInteger>)>,
    /// Written, preceding file-scope initializers. Keep these before the GNU
    /// static-integer substitution: that substitution does not grant ICE status.
    initialized_strings: HashMap<SymbolId, InitPlan<'arena>>,
    initialized_string_undo: Vec<(SymbolId, Option<InitPlan<'arena>>)>,
    string_constant_uses: Vec<(SymbolId, SourceSpan)>,
    declaration_transaction_depth: usize,
    side_declarations: Vec<&'arena CHirDecl<'arena>>,
    builtin_declarations: Vec<&'arena CHirDecl<'arena>>,
    neon_tuple_types: HashMap<String, TypeId>,
    loop_depth: usize,
    current_function_parameters: Option<ParameterSyntax>,
    // The optional expression identity lets a surrounding, precision-invariant
    // consumer discharge only its own pending wide-field refusal. Source spans
    // cannot identify expressions: macro expansions can share the same span.
    profile_refusals: Vec<(
        evocc_chir::ProfileRefusal,
        usize,
        Option<BinaryExpressionIdentity>,
    )>,
    /// Attributes on labels and type names, which have no declaration node.
    detached_attributes: Vec<Attribute>,
    /// Deferred list attributes of EvoIR 10.1.9 until their declarations
    /// settle them.
    convention_attributes: Vec<ConventionAttribute>,
    /// A later declaration can add a list attribute to a function. Its uses
    /// and definitions are therefore checked when the unit is complete.
    function_uses: Vec<FunctionUse>,
    function_definitions: Vec<FunctionUse>,
    generic_selections: Vec<GenericSelectionRecord>,
    front_end_builtins: Vec<FrontEndBuiltinRecord<'arena>>,
    switch_depth: usize,
    switch_cases: Vec<SwitchCaseContext>,
    local_label_scopes: Vec<HashMap<String, String>>,
    next_local_label: u64,
    pack_alignment: Option<u32>,
    pack_stack: Vec<Option<u32>>,
    visibility_stack: Vec<(&'static str, SourceSpan)>,
    fenv_access: bool,
    /// True only while normalizing an initializer attached to a file-scope
    /// object.  It admits the final flexible member without changing the
    /// record type itself.
    flexible_initializer_allowed: bool,
    /// Root flexible member and its highest visited element. Empty aggregate
    /// initializers consume storage even though the sparse plan has no store.
    flexible_initializer_extent: Option<(usize, u64)>,
}

impl<'src, 'arena> Parser<'src, 'arena> {
    /// Infer the type and value category represented by an existing C-HIR
    /// expression. This is intentionally read-only over the type interner:
    /// literal and usual-arithmetic result types use `InferredType` values so
    /// validating an expression never changes later declaration identities.
    fn expression_type(&self, expression: &CHirExpr<'_>) -> Option<InferredExpressionType> {
        let (ty, category) = match expression {
            CHirExpr::Complex(operation) => (
                InferredType::Canonical(operation.facts.ty()),
                ExprCategory::Value,
            ),
            CHirExpr::Floating(operation) => (
                InferredType::Canonical(operation.facts.ty()),
                ExprCategory::Value,
            ),
            CHirExpr::Vector(operation) => (
                InferredType::Canonical(operation.facts.ty()),
                ExprCategory::Value,
            ),
            CHirExpr::Atomic(access) => (
                InferredType::Canonical(access.facts.ty()),
                ExprCategory::Value,
            ),
            CHirExpr::Lit(literal, _) => (
                InferredType::Integer(integer_literal_value(*literal).ok()?.ty()),
                ExprCategory::Value,
            ),
            CHirExpr::FloatLit(literal, _) => (
                InferredType::Real(literal.suffix.type_kind()),
                ExprCategory::Value,
            ),
            CHirExpr::CharLit(literal, _) => (
                InferredType::Integer(literal.integer_value().ty()),
                ExprCategory::Value,
            ),
            CHirExpr::Ident { facts, .. } => {
                (InferredType::Canonical(facts.ty()), facts.category())
            }
            // The place is authority for the designated subobject's type, and
            // this node already represents the converted value.
            CHirExpr::MemoryRead { facts, .. } => {
                (InferredType::Canonical(facts.ty()), ExprCategory::Value)
            }
            CHirExpr::Unary(op, operand, _) => {
                let operand = match op {
                    UnaryOp::AddrOf => self.expression_type(operand)?,
                    UnaryOp::Plus
                    | UnaryOp::Neg
                    | UnaryOp::Abs
                    | UnaryOp::Not
                    | UnaryOp::BitNot => self.arithmetic_operand_type(operand)?,
                    UnaryOp::Deref => self.converted_expression_type(operand)?,
                };
                match op {
                    UnaryOp::Plus | UnaryOp::Neg | UnaryOp::Abs => (
                        self.promoted_arithmetic_type(operand.ty)?,
                        ExprCategory::Value,
                    ),
                    UnaryOp::Not => {
                        if !is_scalar_class(self.operand_type_class(operand.ty)) {
                            return None;
                        }
                        (
                            InferredType::Integer(CIntegerType::INT),
                            ExprCategory::Value,
                        )
                    }
                    UnaryOp::BitNot => {
                        let promoted = match self.operand_type_class(operand.ty) {
                            OperandTypeClass::Integer(integer) => {
                                InferredType::Integer(CInteger::new(0, integer).promoted().ty())
                            }
                            OperandTypeClass::WideInteger { ty, .. } => InferredType::Canonical(ty),
                            _ => return None,
                        };
                        (promoted, ExprCategory::Value)
                    }
                    UnaryOp::AddrOf => {
                        let InferredType::Canonical(pointee) = operand.ty else {
                            return None;
                        };
                        (
                            InferredType::Pointer(self.inferred_pointer_type(pointee)?),
                            ExprCategory::Value,
                        )
                    }
                    UnaryOp::Deref => {
                        let OperandTypeClass::Pointer(pointee) =
                            self.operand_type_class(operand.ty)
                        else {
                            return None;
                        };
                        let category = if matches!(
                            self.types.get(pointee.pointee).map(|ty| &ty.kind),
                            Some(TypeKind::Function { .. })
                        ) {
                            ExprCategory::FunctionDesignator
                        } else {
                            ExprCategory::LValue
                        };
                        (InferredType::Canonical(pointee.pointee), category)
                    }
                }
            }
            CHirExpr::EvaluateOnce(operand, _) => {
                let inner = self.expression_type(operand)?;
                (inner.ty, inner.category)
            }
            CHirExpr::BuiltinExpect(..) => (
                InferredType::Integer(CIntegerType::LONG),
                ExprCategory::Value,
            ),
            CHirExpr::Binary(op, left, right, _) => {
                let left = self.arithmetic_operand_type(left)?;
                let right = self.arithmetic_operand_type(right)?;
                let ty = self.binary_expression_type(*op, left, right)?;
                (ty, ExprCategory::Value)
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => {
                let condition = self.converted_expression_type(condition)?;
                if !is_scalar_class(self.operand_type_class(condition.ty)) {
                    return None;
                }
                let when_true = self.arithmetic_operand_type(when_true)?;
                let when_false = self.arithmetic_operand_type(when_false)?;
                let ty = self.conditional_expression_type(when_true, when_false)?;
                (ty, ExprCategory::Value)
            }
            CHirExpr::Elvis(operand, fallback, _) => {
                let operand = self.arithmetic_operand_type(operand)?;
                if !is_scalar_class(self.operand_type_class(operand.ty)) {
                    return None;
                }
                let fallback = self.arithmetic_operand_type(fallback)?;
                let ty = self.conditional_expression_type(operand, fallback)?;
                (ty, ExprCategory::Value)
            }
            CHirExpr::Call(callee, _, _) => {
                let callee = self.converted_expression_type(callee)?;
                let OperandTypeClass::Pointer(function) = self.operand_type_class(callee.ty) else {
                    return None;
                };
                let TypeKind::Function { return_type, .. } =
                    self.types.get(function.pointee).map(|ty| &ty.kind)?
                else {
                    return None;
                };
                (InferredType::Canonical(*return_type), ExprCategory::Value)
            }
            CHirExpr::CheckedArithmetic { facts, .. } => {
                (InferredType::Canonical(facts.ty()), ExprCategory::Value)
            }
            CHirExpr::BitBuiltin { facts, .. }
            | CHirExpr::BuiltinConstantP { facts, .. }
            | CHirExpr::BuiltinObjectSize { facts, .. }
            | CHirExpr::BuiltinTrap { facts, .. }
            | CHirExpr::BuiltinUnreachable { facts, .. }
            | CHirExpr::BuiltinReturnAddress { facts, .. }
            | CHirExpr::BuiltinFrameAddress { facts, .. }
            | CHirExpr::BuiltinVarArgs { facts, .. }
            | CHirExpr::BuiltinVarArgPack { facts, .. }
            | CHirExpr::LabelAddress { facts, .. } => {
                (InferredType::Canonical(facts.ty()), ExprCategory::Value)
            }
            CHirExpr::Cast(ty, _, _) => (InferredType::Canonical(*ty), ExprCategory::Value),
            CHirExpr::SizeOf(_, _) | CHirExpr::SizeOfExpr(_, _, _) | CHirExpr::AlignOf(_, _) => (
                InferredType::Integer(CIntegerType::ULONG),
                ExprCategory::Value,
            ),
            CHirExpr::Assign { facts, .. }
            | CHirExpr::MemoryAssign { facts, .. }
            | CHirExpr::Update { facts, .. }
            | CHirExpr::MemoryUpdate { facts, .. } => {
                (InferredType::Canonical(facts.ty()), facts.category())
            }
            CHirExpr::StatementExpr { facts, .. } => {
                (InferredType::Canonical(facts.ty()), ExprCategory::Value)
            }
            CHirExpr::CompoundLit(ty, _, _) => (InferredType::Canonical(*ty), ExprCategory::LValue),
            CHirExpr::Comma(_, right, _) => {
                let right = self.converted_expression_type(right)?;
                (right.ty, ExprCategory::Value)
            }
            CHirExpr::StringLit(literal, _) => (
                InferredType::StringArray(literal.prefix),
                ExprCategory::LValue,
            ),
            // A projected lvalue's type comes from the member or element it
            // designates. Qualifier propagation is deliberately not modelled
            // here: `MemoryPlace` accumulates it along the whole chain, which
            // is the authority every consumer already consults.
            CHirExpr::Member(base, name, _) => {
                let inferred = self.expression_type(base)?;
                let InferredType::Canonical(base_ty) = inferred.ty else {
                    return None;
                };
                (
                    InferredType::Canonical(
                        self.record_member_path(base_ty, name)?.last()?.field_ty(),
                    ),
                    if matches!(base, CHirExpr::MemoryRead { .. }) {
                        ExprCategory::LValue
                    } else {
                        inferred.category
                    },
                )
            }
            CHirExpr::PtrMember(pointer, name, _) => {
                let OperandTypeClass::Pointer(pointee) =
                    self.operand_type_class(self.converted_expression_type(pointer)?.ty)
                else {
                    return None;
                };
                (
                    InferredType::Canonical(
                        self.record_member_path(pointee.pointee, name)?
                            .last()?
                            .field_ty(),
                    ),
                    ExprCategory::LValue,
                )
            }
            CHirExpr::Index(left, right, _) => {
                let (base, index) = self.subscript_operands(left, right)?;
                if !matches!(
                    self.operand_type_class(self.converted_expression_type(index)?.ty),
                    OperandTypeClass::Integer(_)
                ) {
                    return None;
                }
                let element = if let Some((_, element, _)) = self.vector_expression_type(base) {
                    InferredType::Canonical(element)
                } else {
                    match self.operand_type_class(self.converted_expression_type(base)?.ty) {
                        OperandTypeClass::Pointer(pointer) => {
                            InferredType::Canonical(pointer.pointee)
                        }
                        OperandTypeClass::StringPointer(prefix) => {
                            InferredType::Integer(integer_type_for_rank(prefix.element_rank()))
                        }
                        _ => return None,
                    }
                };
                (element, ExprCategory::LValue)
            }
        };
        Some(InferredExpressionType {
            ty,
            category,
            null_pointer_constant: self.is_null_pointer_constant(expression),
        })
    }

    fn parse_builtin_atomic(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        let name = self.token_text(&keyword);
        if let Some(form) = clang_atomic(name) {
            return self.parse_clang_atomic(keyword, form);
        }
        if let Some(form) = gnu_atomic(name) {
            return self.parse_gnu_atomic(keyword, form);
        }
        let load = name == "__atomic_load_n";
        let legacy = legacy_atomic(name);
        if !self.match_kind(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        let mut operands = Vec::new();
        if self.peek_kind() != Some(TokenKind::RParen) {
            loop {
                operands.push(self.parse_argument_expression()?);
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
            }
        }
        let arity = match legacy {
            Some((None, _, _)) => 1,
            Some((Some(evocc_chir::AtomicRmwOp::CompareExchange), _, _)) => 3,
            Some(_) => 2,
            None => {
                if load {
                    2
                } else {
                    3
                }
            }
        };
        if operands.len() != arity || self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance()?;
        let address = operands[0];
        let inferred = self.converted_expression_type(address)?;
        let OperandTypeClass::Pointer(pointer) = self.operand_type_class(inferred.ty) else {
            self.report("E2913", "builtin-operand-not-convertible", address.span());
            return None;
        };
        let canonical = self.types.get(pointer.pointee)?.clone();
        let valid_type = self
            .canonical_integer_type(pointer.pointee)
            .is_ok_and(|integer| integer.bits() <= 64)
            || matches!(canonical.kind, TypeKind::Pointer(_));
        // A GNU `__atomic` builtin on an `_Atomic` object means its
        // non-atomic type (EvoIR 10.1.14); the legacy `__sync` set does not
        // admit one.
        if !valid_type
            || (legacy.is_some() && canonical.qualifiers.is_atomic)
            || (!load && canonical.qualifiers.is_const)
            || (legacy.is_some() && matches!(canonical.kind, TypeKind::Int(IntRank::Bool)))
        {
            self.report("E2913", "builtin-operand-not-convertible", address.span());
            return None;
        }
        let volatile = canonical.qualifiers.is_volatile;
        let access_ty = self.intern(canonical.kind, Qualifiers::default());
        let zero = self.alloc_node(CHirExpr::Lit(
            IntLit {
                value: 0,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            keyword.span,
        ));
        let rmw = legacy
            .and_then(|(operation, result, _)| operation.map(|operation| (operation, result)));
        let expected = if rmw.is_some_and(|(op, _)| op == evocc_chir::AtomicRmwOp::CompareExchange)
        {
            let expected = operands[1];
            if !matches!(
                self.assignment_constraint(AssignmentOp::Simple, access_ty, expected),
                UpdateConstraint::Scalar
            ) {
                self.report("E2913", "builtin-operand-not-convertible", expected.span());
                return None;
            }
            Some(self.alloc_node(CHirExpr::Cast(access_ty, expected, expected.span())))
        } else {
            None
        };
        if rmw.is_some_and(|(op, _)| {
            !matches!(
                op,
                evocc_chir::AtomicRmwOp::Exchange | evocc_chir::AtomicRmwOp::CompareExchange
            )
        }) && matches!(self.types.get(access_ty)?.kind, TypeKind::Pointer(_))
        {
            self.report("E2913", "builtin-operand-not-convertible", address.span());
            return None;
        }
        let value = if load {
            None
        } else {
            let value = if legacy.is_some_and(|(op, _, _)| op.is_none()) {
                zero
            } else {
                operands[if expected.is_some() { 2 } else { 1 }]
            };
            if !matches!(
                self.assignment_constraint(AssignmentOp::Simple, access_ty, value),
                UpdateConstraint::Scalar
            ) {
                self.report("E2913", "builtin-operand-not-convertible", value.span());
                return None;
            }
            Some(self.alloc_node(CHirExpr::Cast(access_ty, value, value.span())))
        };
        let order = if let Some((_, _, order)) = legacy {
            let raw = match order {
                evocc_chir::AtomicOrder::Acquire => 2,
                evocc_chir::AtomicOrder::Release => 3,
                _ => 5,
            };
            self.alloc_node(CHirExpr::Lit(
                IntLit {
                    value: raw,
                    suffix: IntSuffix::None,
                    base: IntBase::Dec,
                },
                keyword.span,
            ))
        } else {
            *operands.last()?
        };
        let inferred = self.converted_expression_type(order)?;
        if !is_integer_class(self.operand_type_class(inferred.ty)) {
            self.report("E2913", "builtin-operand-not-convertible", order.span());
            return None;
        }
        let constant_order = if let Ok(value) = self.eval_integer_constant_expression(order) {
            let chosen = value
                .try_mathematical_value()
                .and_then(evocc_chir::AtomicOrder::from_c);
            let Some(chosen) = chosen.filter(|order| rmw.is_some() || order.valid_for(load)) else {
                self.report("E2947", "invalid-atomic-memory-order", order.span());
                return None;
            };
            Some(chosen)
        } else {
            None
        };
        let int_ty = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
        let order = self.alloc_node(CHirExpr::Cast(int_ty, order, order.span()));
        let result_ty =
            if rmw.is_some_and(|(_, result)| result == evocc_chir::AtomicRmwReturn::Success) {
                self.intern(TypeKind::Int(IntRank::Bool), Qualifiers::default())
            } else if load || rmw.is_some() {
                access_ty
            } else {
                self.intern(TypeKind::Void, Qualifiers::default())
            };
        Some(
            self.alloc_node(CHirExpr::Atomic(evocc_chir::CHirAtomicAccess {
                address,
                value,
                expected,
                rmw,
                order,
                constant_order,
                access_ty,
                volatile,
                writeback: false,
                weak: None,
                failure: None,
                memory: false,
                result: None,
                facts: ExprFacts::value(result_ty),
                span: self.merge_span(keyword.span, close.span),
            })),
        )
    }
}

#[derive(Debug)]
struct JumpRegionReference {
    name: String,
    span: SourceSpan,
    regions: Vec<u32>,
    checks_region_entry: bool,
}

struct JumpRegionContext<'a> {
    is_variable: &'a dyn Fn(TypeId) -> bool,
    next: u32,
    variable: HashSet<u32>,
    switches: Vec<Vec<u32>>,
    invalid_switch_entries: Vec<SourceSpan>,
    indirect: Vec<(SourceSpan, Vec<u32>)>,
}

impl JumpRegionContext<'_> {
    fn fresh(&mut self, variable: bool) -> u32 {
        let id = self.next;
        self.next += 1;
        if variable {
            self.variable.insert(id);
        }
        id
    }

    fn check_switch_entry(&mut self, regions: &[u32], span: SourceSpan) {
        if self.switches.last().is_some_and(|entry| {
            regions
                .iter()
                .any(|region| self.variable.contains(region) && !entry.contains(region))
        }) {
            self.invalid_switch_entries.push(span);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BuiltinTypeWord {
    kind: TokenKind,
    forced_int128_signedness: Option<GnuInt128Signedness>,
    target_builtin: Option<TargetBuiltinType>,
    target_vector: Option<evocc_types::neon::BuiltinType>,
}

/// When a parenthesized expression appears, preserve its original span.
fn respan_expr<'arena>(inner: &CHirExpr<'arena>, paren_span: SourceSpan) -> CHirExpr<'arena> {
    match inner {
        CHirExpr::Complex(operation) => {
            let mut operation = operation.clone();
            operation.span = paren_span;
            CHirExpr::Complex(operation)
        }
        CHirExpr::Floating(operation) => {
            let mut operation = operation.clone();
            operation.span = paren_span;
            CHirExpr::Floating(operation)
        }
        CHirExpr::Vector(operation) => {
            let mut operation = operation.clone();
            operation.span = paren_span;
            CHirExpr::Vector(operation)
        }
        CHirExpr::Atomic(access) => {
            let mut access = access.clone();
            access.span = paren_span;
            CHirExpr::Atomic(access)
        }
        CHirExpr::BuiltinObjectSize {
            operand,
            mode,
            dynamic,
            facts,
            ..
        } => CHirExpr::BuiltinObjectSize {
            operand,
            mode: *mode,
            dynamic: *dynamic,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BuiltinConstantP { operand, facts, .. } => CHirExpr::BuiltinConstantP {
            operand,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::Lit(literal, _) => CHirExpr::Lit(*literal, paren_span),
        CHirExpr::MemoryRead {
            place,
            target_span,
            facts,
            ..
        } => CHirExpr::MemoryRead {
            place: place.clone(),
            target_span: *target_span,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::FloatLit(literal, _) => CHirExpr::FloatLit(*literal, paren_span),
        CHirExpr::CharLit(literal, _) => CHirExpr::CharLit(*literal, paren_span),
        CHirExpr::StringLit(literal, _) => CHirExpr::StringLit(literal.clone(), paren_span),
        CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            ..
        } => CHirExpr::Ident {
            symbol: *symbol,
            spelling: spelling.clone(),
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::Unary(op, operand, _) => CHirExpr::Unary(*op, operand, paren_span),
        CHirExpr::EvaluateOnce(operand, _) => CHirExpr::EvaluateOnce(operand, paren_span),
        CHirExpr::Binary(op, lhs, rhs, _) => CHirExpr::Binary(*op, lhs, rhs, paren_span),
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            CHirExpr::Ternary(condition, when_true, when_false, paren_span)
        }
        CHirExpr::Elvis(operand, fallback, _) => CHirExpr::Elvis(operand, fallback, paren_span),
        CHirExpr::BuiltinExpect(value, expected, _) => {
            CHirExpr::BuiltinExpect(value, expected, paren_span)
        }
        CHirExpr::Call(callee, arguments, _) => {
            CHirExpr::Call(callee, arguments.clone(), paren_span)
        }
        CHirExpr::CheckedArithmetic {
            op,
            lhs,
            rhs,
            target,
            target_ty,
            facts,
            ..
        } => CHirExpr::CheckedArithmetic {
            op: *op,
            lhs,
            rhs,
            target,
            target_ty: *target_ty,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BitBuiltin {
            op, operand, facts, ..
        } => CHirExpr::BitBuiltin {
            op: *op,
            operand,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BuiltinUnreachable { facts, .. } => CHirExpr::BuiltinUnreachable {
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BuiltinTrap { facts, .. } => CHirExpr::BuiltinTrap {
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BuiltinReturnAddress { depth, facts, .. } => CHirExpr::BuiltinReturnAddress {
            depth: *depth,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BuiltinFrameAddress { depth, facts, .. } => CHirExpr::BuiltinFrameAddress {
            depth: *depth,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BuiltinVarArgPack { length, facts, .. } => CHirExpr::BuiltinVarArgPack {
            length: *length,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BuiltinVarArgs {
            op,
            first,
            second,
            requested_ty,
            facts,
            ..
        } => CHirExpr::BuiltinVarArgs {
            op: *op,
            first,
            second: *second,
            requested_ty: *requested_ty,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::LabelAddress { label, facts, .. } => CHirExpr::LabelAddress {
            label: label.clone(),
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::Cast(ty, operand, _) => CHirExpr::Cast(*ty, operand, paren_span),
        CHirExpr::SizeOf(ty, _) => CHirExpr::SizeOf(*ty, paren_span),
        CHirExpr::SizeOfExpr(operand, ty, _) => CHirExpr::SizeOfExpr(operand, *ty, paren_span),
        CHirExpr::AlignOf(ty, _) => CHirExpr::AlignOf(*ty, paren_span),
        CHirExpr::Member(object, name, _) => CHirExpr::Member(object, name.clone(), paren_span),
        CHirExpr::PtrMember(object, name, _) => {
            CHirExpr::PtrMember(object, name.clone(), paren_span)
        }
        CHirExpr::Index(object, index, _) => CHirExpr::Index(object, index, paren_span),
        CHirExpr::Assign {
            op,
            place,
            target,
            value,
            facts,
            ..
        } => CHirExpr::Assign {
            op: *op,
            place: *place,
            target,
            value,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::MemoryAssign {
            op,
            place,
            target_span,
            value,
            facts,
            ..
        } => CHirExpr::MemoryAssign {
            op: *op,
            place: place.clone(),
            target_span: *target_span,
            value,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::Update {
            op,
            place,
            target,
            facts,
            ..
        } => CHirExpr::Update {
            op: *op,
            place: *place,
            target,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::MemoryUpdate {
            op,
            place,
            target_span,
            facts,
            ..
        } => CHirExpr::MemoryUpdate {
            op: *op,
            place: place.clone(),
            target_span: *target_span,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::CompoundLit(ty, values, _) => {
            CHirExpr::CompoundLit(*ty, values.clone(), paren_span)
        }
        CHirExpr::StatementExpr {
            body, value, facts, ..
        } => CHirExpr::StatementExpr {
            body,
            value: *value,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::Comma(lhs, rhs, _) => CHirExpr::Comma(lhs, rhs, paren_span),
    }
}
