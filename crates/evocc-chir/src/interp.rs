//! Deterministic, fuel-bounded execution for the Day-5 scalar C-HIR subset.
//!
//! This interpreter deliberately consumes typed C-HIR and its normalized
//! initializer plans. It does not reuse the parser's constant-expression
//! evaluator: every accepted operation goes through the shared target integer
//! kernel and every operation outside the scalar contract fails closed.

use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
};

use evocc_common::SourceSpan;
use evocc_memory::{
    AccessLayout, ByteSequence, CopyVolatility, FrameId, IndexSignedness, InitialState,
    LibraryValue, MemoryConfig, MemoryFault, MemoryLimits, MemoryState, OffsetDirection,
    OffsetOperand, Pointer, PointerValue, ScopeId as MemoryScopeId, WorldLimits, WorldState,
};
use evocc_types::{
    enum_integer_rank, ArrayBound, BitFieldLayout, FunctionParameters, Qualifiers, TagDefinition,
    TagState, TargetDataLayout, TypeId, TypeInterner, TypeKind,
};

use crate::{
    integer::{
        eval_binary_integer, eval_bit_builtin, eval_checked_arithmetic,
        eval_constant_binary_integer, eval_constant_unary_integer, eval_unary_integer,
        integer_literal_value, integer_type_for_rank, usual_arithmetic_type, BitBuiltinOp,
        CInteger, CIntegerType, IntegerError,
    },
    statement_expression_value, AllocationSiteKind, AllocationSiteManifest,
    AllocationSiteManifestError, AssignmentOp, BinaryOp, CHirBlockItem, CHirDecl, CHirExpr,
    CHirForInit, CHirStmt, CHirUnit, CheckedArithmeticOp, ExprCategory, ExprFacts, FunctionDecl,
    InitAction, MemoryPlace, ObjectDecl, ParameterSyntax, PlaceProjection, PlaceRoot,
    PointerTypeDesc, ScalarPlace, ScopeKind, StorageClass, StringLit, SubobjectPath, SubobjectStep,
    SymbolId, SymbolKind, UnaryOp, UpdateOp, WrittenDeclarator, WrittenDeclaratorLayer,
};

mod complex;
mod floating;
mod lifecycle;
mod neon;
mod vector;
pub use lifecycle::interpret_program;

const DEFAULT_FUEL: u64 = 100_000;
const MAX_MEMORY_OBJECTS: u64 = 65_536;
const MAX_MEMORY_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MEMORY_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_VOLATILE_EVENTS: u64 = 1_048_576;
/// Bound for the remaining recursive address/aggregate initializer paths.
/// Integer expressions and cast chains use explicit stacks, so their depth
/// does not consume this allowance. Keep the bound until those remaining
/// object paths are iterative too.
const MAX_STATIC_INITIALIZER_DEPTH: usize = 64;

/// Deterministic execution limits for one interpreter invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterpreterLimits {
    fuel: u64,
}

impl InterpreterLimits {
    #[must_use]
    pub const fn new(fuel: u64) -> Self {
        Self { fuel }
    }

    #[must_use]
    pub const fn fuel(self) -> u64 {
        self.fuel
    }
}

impl Default for InterpreterLimits {
    fn default() -> Self {
        Self::new(DEFAULT_FUEL)
    }
}

/// One scalar result, retaining both canonical C type identity and target bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScalarValue {
    pub ty: TypeId,
    pub raw_bits: u128,
}

impl ScalarValue {
    /// Decode the target bits according to the canonical integer or enum type.
    pub fn mathematical_value(self, types: &TypeInterner) -> Result<i128, ScalarValueError> {
        let ty = canonical_integer_type(types, self.ty)
            .ok_or(ScalarValueError::UnsupportedType { ty: self.ty })?;
        CInteger::from_raw(self.raw_bits, ty)
            .try_mathematical_value()
            .ok_or(ScalarValueError::OutsideSignedCarrier)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarValueError {
    UnsupportedType { ty: TypeId },
    OutsideSignedCarrier,
}

impl fmt::Display for ScalarValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutsideSignedCarrier => {
                formatter.write_str("unsigned value does not fit i128; use raw_bits")
            }
            Self::UnsupportedType { ty } => {
                write!(formatter, "unsupported scalar value type ID {}", ty.raw())
            }
        }
    }
}

impl std::error::Error for ScalarValueError {}

/// Successful execution and its deterministic budget witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpretOutcome {
    pub return_value: Option<ScalarValue>,
    pub steps: u64,
    /// Final immutable object-memory snapshot for this invocation.
    pub memory: MemoryState,
    /// Final externally observable volatile-event snapshot for this invocation.
    pub world: WorldState,
}

/// A fail-closed scalar interpreter error.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpreterError {
    TargetBuiltin {
        name: String,
        span: SourceSpan,
    },
    Profile(crate::ProfileRefusal),
    Linkage(crate::linkage::LinkageError),
    VolatileAggregate {
        span: SourceSpan,
    },
    IndirectCallDynamicSignatureMismatch {
        span: SourceSpan,
    },
    UnknownFunction {
        symbol: SymbolId,
    },
    FunctionHasNoBody {
        symbol: SymbolId,
        span: SourceSpan,
    },
    UnsupportedFunctionSignature {
        symbol: SymbolId,
        span: SourceSpan,
    },
    UnsupportedType {
        ty: TypeId,
        span: SourceSpan,
    },
    UnsupportedExpression {
        feature: &'static str,
        span: SourceSpan,
    },
    UnsupportedStatement {
        feature: &'static str,
        span: SourceSpan,
    },
    Asm {
        span: SourceSpan,
    },
    BitBuiltinZeroOperand {
        span: SourceSpan,
    },
    UnreachableReached {
        span: SourceSpan,
    },
    BuiltinTrapReached {
        span: SourceSpan,
    },
    ComputedGoto {
        span: SourceSpan,
    },
    InvalidIndirectBranchTarget {
        span: SourceSpan,
    },
    VarArgs {
        span: SourceSpan,
    },
    WideInteger {
        span: SourceSpan,
    },
    FrameAddressUnavailable {
        span: SourceSpan,
    },
    UnsupportedDeclaration {
        feature: &'static str,
        span: SourceSpan,
    },
    MalformedFunctionDeclaration {
        symbol: SymbolId,
        span: SourceSpan,
    },
    DuplicateFunctionBody {
        symbol: SymbolId,
        span: SourceSpan,
    },
    MalformedObjectDeclaration {
        symbol: SymbolId,
        span: SourceSpan,
    },
    InvalidInitializerPlan {
        symbol: SymbolId,
        span: SourceSpan,
    },
    InvalidScalarPlace {
        symbol: SymbolId,
        span: SourceSpan,
    },
    ReadOfUninitializedObject {
        symbol: SymbolId,
        spelling: String,
        span: SourceSpan,
    },
    UninitializedValue {
        span: SourceSpan,
    },
    FloatingToInteger {
        span: SourceSpan,
    },
    OutOfScopeObject {
        symbol: SymbolId,
        spelling: String,
        span: SourceSpan,
    },
    Integer {
        error: IntegerError,
        span: SourceSpan,
    },
    InvalidControlFlow {
        flow: &'static str,
        span: SourceSpan,
    },
    UnknownLabel {
        spelling: String,
        span: SourceSpan,
    },
    DuplicateLabel {
        spelling: String,
        span: SourceSpan,
    },
    MissingReturn {
        span: SourceSpan,
    },
    FuelExhausted {
        limit: u64,
        consumed: u64,
        span: SourceSpan,
    },
    MemorySetup {
        error: MemoryFault,
    },
    Memory {
        error: MemoryFault,
        span: SourceSpan,
    },
    InvalidAllocationManifest {
        error: AllocationSiteManifestError,
    },
    MissingAllocationManifest,
    CallThroughNullFunctionPointer {
        span: SourceSpan,
    },
    CallThroughNonFunctionPointer {
        span: SourceSpan,
    },
}

impl InterpreterError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TargetBuiltin { .. } => "E3052",
            Self::Profile(error) => error.code,
            Self::Linkage(error) => error.code,
            Self::VolatileAggregate { .. } => "E2922",
            Self::ReadOfUninitializedObject { .. } | Self::UninitializedValue { .. } => "E3001",
            Self::FloatingToInteger { .. } => "E3063",
            Self::Integer {
                error: IntegerError::DivisionByZero,
                ..
            } => "E3002",
            Self::Integer {
                error: IntegerError::RemainderByZero,
                ..
            } => "E3003",
            Self::Integer {
                error: IntegerError::SignedOverflow,
                ..
            } => "E3004",
            Self::Integer {
                error: IntegerError::InvalidShiftCount,
                ..
            } => "E3005",
            Self::Integer {
                error: IntegerError::InvalidSignedLeftShift,
                ..
            } => "E3006",
            Self::FuelExhausted { .. } => "E3008",
            Self::IndirectCallDynamicSignatureMismatch { .. } => "E3019",
            Self::CallThroughNullFunctionPointer { .. } => "E3015",
            Self::CallThroughNonFunctionPointer { .. } => "E3016",
            Self::Asm { .. } => "E3021",
            Self::BitBuiltinZeroOperand { .. } => "E3022",
            Self::UnreachableReached { .. } => "E3024",
            Self::BuiltinTrapReached { .. } => "E3047",
            Self::ComputedGoto { .. } => "E3029",
            Self::InvalidIndirectBranchTarget { .. } => "E3048",
            Self::VarArgs { .. } => "E3030",
            Self::WideInteger { .. } => "E3033",
            Self::FrameAddressUnavailable { .. } => "E3031",
            Self::Memory { error, .. } => match error.runtime_diagnostic() {
                Some(diagnostic) => diagnostic.code,
                None => "E3007",
            },
            Self::MemorySetup { .. }
            | Self::InvalidAllocationManifest { .. }
            | Self::MissingAllocationManifest => "E3007",
            Self::UnknownFunction { .. }
            | Self::FunctionHasNoBody { .. }
            | Self::UnsupportedFunctionSignature { .. }
            | Self::UnsupportedType { .. }
            | Self::UnsupportedExpression { .. }
            | Self::UnsupportedStatement { .. }
            | Self::UnsupportedDeclaration { .. }
            | Self::MalformedFunctionDeclaration { .. }
            | Self::DuplicateFunctionBody { .. }
            | Self::MalformedObjectDeclaration { .. }
            | Self::InvalidInitializerPlan { .. }
            | Self::InvalidScalarPlace { .. }
            | Self::OutOfScopeObject { .. }
            | Self::Integer { .. }
            | Self::InvalidControlFlow { .. }
            | Self::UnknownLabel { .. }
            | Self::DuplicateLabel { .. }
            | Self::MissingReturn { .. } => "E3007",
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::TargetBuiltin { name, .. } => format!("not-executed: target-builtin: {name}"),
            Self::Profile(error) => error.message.to_owned(),
            Self::Linkage(error) => error.message.to_owned(),
            Self::VolatileAggregate { .. } => "volatile-aggregate-copy-unsupported".to_owned(),
            Self::UninitializedValue { .. } => "read-of-uninitialized-object".to_owned(),
            Self::FloatingToInteger { .. } => "floating-to-integer-not-representable".to_owned(),
            Self::IndirectCallDynamicSignatureMismatch { .. } => {
                "indirect-call-dynamic-signature-mismatch".to_owned()
            }
            Self::ReadOfUninitializedObject { spelling, .. } => {
                format!("read-of-uninitialized-object: {spelling}")
            }
            Self::OutOfScopeObject { spelling, .. } => {
                format!("out-of-scope-interpreter-object: {spelling}")
            }
            Self::Integer {
                error: IntegerError::DivisionByZero,
                ..
            } => "integer-division-by-zero".to_owned(),
            Self::Integer {
                error: IntegerError::RemainderByZero,
                ..
            } => "integer-remainder-by-zero".to_owned(),
            Self::Integer {
                error: IntegerError::SignedOverflow,
                ..
            } => "signed-integer-overflow".to_owned(),
            Self::Integer {
                error: IntegerError::InvalidShiftCount,
                ..
            } => "invalid-shift-count".to_owned(),
            Self::Integer {
                error: IntegerError::InvalidSignedLeftShift,
                ..
            } => "invalid-signed-left-shift".to_owned(),
            Self::FuelExhausted { .. } => "interpreter-fuel-exhausted".to_owned(),
            Self::CallThroughNullFunctionPointer { .. } => {
                "call-through-null-function-pointer".to_owned()
            }
            Self::CallThroughNonFunctionPointer { .. } => {
                "call-through-non-function-pointer".to_owned()
            }
            Self::MemorySetup { error } => {
                format!("interpreter-memory-setup-failed: {error}")
            }
            Self::Memory { error, .. } => error.runtime_diagnostic().map_or_else(
                || format!("interpreter-memory-failed: {error}"),
                |diagnostic| diagnostic.message.to_owned(),
            ),
            Self::InvalidAllocationManifest { error } => {
                format!("invalid-allocation-site-manifest: {error}")
            }
            Self::MissingAllocationManifest => "missing-allocation-site-manifest".to_owned(),
            Self::UnknownFunction { symbol } => {
                format!("unknown-interpreter-function: {}", symbol.raw())
            }
            Self::FunctionHasNoBody { .. } => "interpreter-function-has-no-body".to_owned(),
            Self::UnsupportedFunctionSignature { .. } => {
                "unsupported-interpreter-function-signature".to_owned()
            }
            Self::UnsupportedType { .. } => "unsupported-interpreter-type".to_owned(),
            Self::UnsupportedExpression { feature, .. } => {
                format!("unsupported-interpreter-expression: {feature}")
            }
            Self::UnsupportedStatement { feature, .. } => {
                format!("unsupported-interpreter-statement: {feature}")
            }
            Self::Asm { .. } => "not-lowered: asm".to_owned(),
            Self::BitBuiltinZeroOperand { .. } => "bit-builtin-zero-operand".to_owned(),
            Self::UnreachableReached { .. } => "unreachable-reached".to_owned(),
            Self::BuiltinTrapReached { .. } => "builtin-trap-reached".to_owned(),
            Self::ComputedGoto { .. } => "not-lowered: computed-goto".to_owned(),
            Self::InvalidIndirectBranchTarget { .. } => "invalid-indirect-branch-target".to_owned(),
            Self::VarArgs { .. } => "not-lowered: varargs".to_owned(),
            Self::WideInteger { .. } => "not-executed: wide-integer".to_owned(),
            Self::FrameAddressUnavailable { .. } => "not-executed: frame-address".to_owned(),
            Self::UnsupportedDeclaration { feature, .. } => {
                format!("unsupported-interpreter-declaration: {feature}")
            }
            Self::MalformedFunctionDeclaration { .. } => {
                "malformed-interpreter-function-declaration".to_owned()
            }
            Self::DuplicateFunctionBody { .. } => "duplicate-interpreter-function-body".to_owned(),
            Self::MalformedObjectDeclaration { .. } => {
                "malformed-interpreter-object-declaration".to_owned()
            }
            Self::InvalidInitializerPlan { .. } => "invalid-scalar-initializer-plan".to_owned(),
            Self::InvalidScalarPlace { .. } => "invalid-scalar-place".to_owned(),
            Self::Integer { error, .. } => format!("integer-evaluation-failed: {error:?}"),
            Self::InvalidControlFlow { flow, .. } => {
                format!("invalid-interpreter-control-flow: {flow}")
            }
            Self::UnknownLabel { spelling, .. } => {
                format!("unknown-interpreter-label: {spelling}")
            }
            Self::DuplicateLabel { spelling, .. } => {
                format!("duplicate-interpreter-label: {spelling}")
            }
            Self::MissingReturn { .. } => "scalar-function-fell-off-end".to_owned(),
        }
    }

    #[must_use]
    pub const fn span(&self) -> Option<SourceSpan> {
        match self {
            Self::TargetBuiltin { span, .. } => Some(*span),
            Self::Profile(error) => Some(error.span),
            Self::Linkage(error) => Some(error.span),
            Self::UnknownFunction { .. }
            | Self::MemorySetup { .. }
            | Self::InvalidAllocationManifest { .. }
            | Self::MissingAllocationManifest => None,
            Self::FunctionHasNoBody { span, .. }
            | Self::UnsupportedFunctionSignature { span, .. }
            | Self::UnsupportedType { span, .. }
            | Self::VolatileAggregate { span }
            | Self::UnsupportedExpression { span, .. }
            | Self::UnsupportedStatement { span, .. }
            | Self::Asm { span }
            | Self::BitBuiltinZeroOperand { span }
            | Self::UnreachableReached { span }
            | Self::BuiltinTrapReached { span }
            | Self::ComputedGoto { span }
            | Self::InvalidIndirectBranchTarget { span }
            | Self::VarArgs { span }
            | Self::WideInteger { span }
            | Self::FrameAddressUnavailable { span }
            | Self::UnsupportedDeclaration { span, .. }
            | Self::MalformedFunctionDeclaration { span, .. }
            | Self::DuplicateFunctionBody { span, .. }
            | Self::MalformedObjectDeclaration { span, .. }
            | Self::InvalidInitializerPlan { span, .. }
            | Self::InvalidScalarPlace { span, .. }
            | Self::ReadOfUninitializedObject { span, .. }
            | Self::UninitializedValue { span }
            | Self::FloatingToInteger { span }
            | Self::OutOfScopeObject { span, .. }
            | Self::Integer { span, .. }
            | Self::InvalidControlFlow { span, .. }
            | Self::UnknownLabel { span, .. }
            | Self::DuplicateLabel { span, .. }
            | Self::MissingReturn { span }
            | Self::FuelExhausted { span, .. }
            | Self::Memory { span, .. }
            | Self::IndirectCallDynamicSignatureMismatch { span }
            | Self::CallThroughNullFunctionPointer { span }
            | Self::CallThroughNonFunctionPointer { span } => Some(*span),
        }
    }
}

impl fmt::Display for InterpreterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "error[{}]: {}", self.code(), self.message())
    }
}

impl std::error::Error for InterpreterError {}

#[derive(Debug, Clone, Copy)]
struct Slot {
    ty: CIntegerType,
    value: Option<CInteger>,
    active: bool,
}

/// One task-machine value. The call-free scalar fast path deliberately keeps
/// using `CInteger` directly; this boundary is where Day 7 pointer execution
/// will attach without changing that path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CValue {
    /// A missing function result can be discarded without being read.
    Uninitialized(ValueType),
    Integer(CInteger),
    Floating(evocc_types::floating::FloatBits),
    Pointer {
        value: Pointer,
        ty: PointerTypeDesc,
    },
    FunctionPointer {
        value: PointerValue,
        ty: PointerTypeDesc,
    },
    /// A record or union value remains backed by object storage until copied.
    /// This preserves padding initializedness and pointer provenance without
    /// inventing a second aggregate representation in the interpreter.
    Aggregate {
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
    },
}

/// The value-level part of a function signature accepted by the task machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueType {
    Void,
    Integer(CIntegerType),
    Floating(evocc_types::FloatKind),
    Pointer(PointerTypeDesc),
    Aggregate(TypeId),
}

#[derive(Debug, Clone, Copy)]
struct MemoryBinding {
    address: Pointer,
    ty: TypeId,
}

#[derive(Debug, Clone, Copy)]
struct InitializerTarget<'arena> {
    binding: MemoryBinding,
    initializer: &'arena crate::ObjectInitializer<'arena>,
    symbol: Option<SymbolId>,
    span: SourceSpan,
}

fn invalid_initializer_target(target: InitializerTarget<'_>, span: SourceSpan) -> InterpreterError {
    target.symbol.map_or(
        InterpreterError::UnsupportedExpression {
            feature: "invalid compound-literal initializer plan",
            span,
        },
        |symbol| InterpreterError::InvalidInitializerPlan { symbol, span },
    )
}

#[derive(Debug)]
struct MemoryFrame<'arena> {
    anonymous: Vec<CValue>,
    id: FrameId,
    root_scope: MemoryScopeId,
    current_scope: MemoryScopeId,
    bindings: HashMap<SymbolId, MemoryBinding>,
    compound_bindings: HashMap<(MemoryScopeId, usize), MemoryBinding>,
    cleanups: Vec<(MemoryScopeId, &'arena CHirExpr<'arena>)>,
}

#[derive(Debug)]
struct MemoryScopeExit {
    scope: MemoryScopeId,
    parent: MemoryScopeId,
    bindings: HashMap<SymbolId, MemoryBinding>,
    span: SourceSpan,
}

#[derive(Debug)]
struct FunctionEnd<'arena> {
    symbol: SymbolId,
    return_type: TypeId,
    return_value_type: ValueType,
    is_main: bool,
    body_span: SourceSpan,
    caller_slots: Option<HashMap<SymbolId, Slot>>,
    caller_memory_frame: Option<MemoryFrame<'arena>>,
    caller_fuel_anchor: Option<SourceSpan>,
    caller_expects_value: bool,
    value_base: usize,
}

#[derive(Debug)]
struct ValidatedFunctionSignature {
    return_type: TypeId,
    return_value_type: ValueType,
    parameters: Vec<(Option<SymbolId>, ValueType)>,
}

#[derive(Debug)]
enum MachineTask<'arena> {
    BeginConstantRepresentationStore {
        place: MemoryPlace<'arena>,
        bits: CInteger,
        span: SourceSpan,
    },
    FinishConstantRepresentationStore {
        place: MemoryPlace<'arena>,
        bits: CInteger,
        span: SourceSpan,
    },
    ApplyAtomic(&'arena crate::CHirAtomicAccess<'arena>),
    IndirectGoto(SourceSpan),
    ApplyVarArgs {
        op: crate::VarArgsBuiltinOp,
        requested_ty: Option<TypeId>,
        span: SourceSpan,
    },
    FunctionEnd(FunctionEnd<'arena>),
    RestoreSlots(Vec<(SymbolId, Slot)>),
    ExitMemoryScope(MemoryScopeExit),
    /// Step a named scalar object in place, yielding the value the prefix or
    /// postfix form of the operator produces.
    ApplyScalarUpdate {
        operator: UpdateOp,
        symbol: SymbolId,
        target_span: SourceSpan,
        span: SourceSpan,
    },
    /// Load, step and store one designated scalar, yielding the value the
    /// prefix or postfix form of the operator produces.
    ApplyIndirectUpdate {
        operator: UpdateOp,
        bit_field: Option<BitFieldLayout>,
        layout: AccessLayout,
        value_type: TypeId,
        qualifiers: Qualifiers,
        target_span: SourceSpan,
        span: SourceSpan,
    },
    /// Fold a projected place's root address and subscript values into one
    /// address and leave it on the value stack.
    ///
    /// Separating this from the access itself lets the ordinary indirect store
    /// path consume a projected address unchanged.
    FoldPlaceAddress {
        place: &'arena MemoryPlace<'arena>,
        span: SourceSpan,
    },
    /// Fold a projected place's root address and subscript values into one
    /// address, then load the designated subobject.
    ///
    /// The subscript values are already on the value stack in source order, so
    /// each one has been evaluated exactly once by the time this runs.
    LoadProjectedPlace {
        place: &'arena MemoryPlace<'arena>,
        span: SourceSpan,
    },
    /// Select a scalar member from an aggregate value whose producer must be
    /// evaluated first (notably the value of a record assignment).
    LoadAggregateMember {
        name: &'arena str,
        span: SourceSpan,
    },
    RestoreFuelAnchor(Option<SourceSpan>),
    RestoreNestedMutationDepth(usize),
    RestoreFenvAccess(bool),
    ExecDeclaration(&'arena CHirDecl<'arena>),
    ExecStatement(&'arena CHirStmt<'arena>),
    SwitchEnd,
    CompoundContinue {
        items: &'arena [CHirBlockItem<'arena>],
        index: usize,
    },
    StatementExpressionContinue {
        items: &'arena [CHirBlockItem<'arena>],
        index: usize,
        value: Option<&'arena CHirExpr<'arena>>,
        span: SourceSpan,
    },
    PreserveStatementExpressionValue {
        parent: MemoryScopeId,
        span: SourceSpan,
    },
    WhileCheck {
        condition: &'arena CHirExpr<'arena>,
        body: &'arena CHirStmt<'arena>,
    },
    AfterWhileCondition {
        condition: &'arena CHirExpr<'arena>,
        body: &'arena CHirStmt<'arena>,
        span: SourceSpan,
    },
    DoWhileCondition {
        condition: &'arena CHirExpr<'arena>,
        body: &'arena CHirStmt<'arena>,
    },
    AfterDoWhileCondition {
        condition: &'arena CHirExpr<'arena>,
        body: &'arena CHirStmt<'arena>,
        span: SourceSpan,
    },
    ForCheck {
        condition: Option<&'arena CHirExpr<'arena>>,
        step: Option<&'arena CHirExpr<'arena>>,
        body: &'arena CHirStmt<'arena>,
        span: SourceSpan,
    },
    AfterForCondition {
        condition: Option<&'arena CHirExpr<'arena>>,
        step: Option<&'arena CHirExpr<'arena>>,
        body: &'arena CHirStmt<'arena>,
        span: SourceSpan,
    },
    ForStep {
        condition: Option<&'arena CHirExpr<'arena>>,
        step: Option<&'arena CHirExpr<'arena>>,
        body: &'arena CHirStmt<'arena>,
        span: SourceSpan,
    },
    DiscardExpression(SourceSpan),
    FinishInitializer {
        symbol: SymbolId,
        ty: CIntegerType,
        span: SourceSpan,
    },
    FinishMemoryInitializer {
        symbol: SymbolId,
        span: SourceSpan,
    },
    /// Apply the remaining actions of one object's initializer plan.
    ///
    /// Plans are ordered and may target subobjects, so they are driven one
    /// action at a time rather than assumed to be a single whole-object store.
    ContinueMemoryInitializer {
        target: InitializerTarget<'arena>,
        index: usize,
    },
    /// Store one evaluated initializer value into the subobject its action
    /// designates.
    FinishMemorySubobject {
        target: InitializerTarget<'arena>,
        index: usize,
    },
    YieldCompoundLiteral {
        binding: MemoryBinding,
        as_address: bool,
        span: SourceSpan,
    },
    FinishReturnValue(SourceSpan),
    FinishVoidReturn(SourceSpan),
    DiscardSelectedExpression {
        then_expression: &'arena CHirExpr<'arena>,
        else_expression: &'arena CHirExpr<'arena>,
    },
    SelectIf {
        then_statement: &'arena CHirStmt<'arena>,
        else_statement: Option<&'arena CHirStmt<'arena>>,
    },
    EvalExpression {
        expression: &'arena CHirExpr<'arena>,
        allow_root_mutation: bool,
    },
    ApplyVector(&'arena crate::vector::VectorExpr<'arena>),
    ApplyFloating(&'arena crate::floating::FloatingExpr<'arena>),
    ApplyComplex(&'arena crate::complex::ComplexExpr<'arena>),
    ApplyUnary {
        operator: crate::UnaryOp,
        expression_span: SourceSpan,
        operand_span: SourceSpan,
    },
    ContinueBinary {
        operator: BinaryOp,
        right: &'arena CHirExpr<'arena>,
        span: SourceSpan,
        allow_root_mutation: bool,
    },
    ApplyBinary {
        operator: BinaryOp,
        left: CValue,
        span: SourceSpan,
    },
    ApplyCheckedArithmetic {
        operator: CheckedArithmeticOp,
        target_type: TypeId,
        span: SourceSpan,
    },
    ApplyBitBuiltin {
        operator: BitBuiltinOp,
        span: SourceSpan,
    },
    AllocateDynamic {
        expression: &'arena CHirExpr<'arena>,
    },
    SelectTernary {
        then_expression: &'arena CHirExpr<'arena>,
        else_expression: &'arena CHirExpr<'arena>,
        result_type: ValueType,
    },
    SelectElvis {
        else_expression: &'arena CHirExpr<'arena>,
        result_type: ValueType,
        span: SourceSpan,
    },
    CastValue {
        target: ValueType,
        span: SourceSpan,
    },
    ApplyAssignment {
        operator: AssignmentOp,
        symbol: SymbolId,
        target_span: SourceSpan,
        span: SourceSpan,
    },
    LoadDereference {
        pointer_type: PointerTypeDesc,
        span: SourceSpan,
    },
    ApplyIndirectAssignment {
        operator: AssignmentOp,
        bit_field: Option<BitFieldLayout>,
        layout: AccessLayout,
        pointer_type: PointerTypeDesc,
        value_type: TypeId,
        qualifiers: Qualifiers,
        target_span: SourceSpan,
        span: SourceSpan,
    },
    SelectSwitch {
        body: &'arena CHirStmt<'arena>,
        span: SourceSpan,
    },
    EvalCallArguments {
        callee: &'arena CHirExpr<'arena>,
        arguments: &'arena [&'arena CHirExpr<'arena>],
        index: usize,
        values: Vec<CValue>,
        span: SourceSpan,
        expects_value: bool,
    },
    CollectCallArgument {
        callee: &'arena CHirExpr<'arena>,
        arguments: &'arena [&'arena CHirExpr<'arena>],
        index: usize,
        values: Vec<CValue>,
        span: SourceSpan,
        expects_value: bool,
    },
    StartIndirectCall {
        arguments: Vec<CValue>,
        span: SourceSpan,
        expects_value: bool,
        value_base: usize,
    },
}

#[derive(Debug, Clone, Copy)]
enum MachineBlockMode<'arena> {
    Compound,
    StatementExpression {
        value: Option<&'arena CHirExpr<'arena>>,
        span: SourceSpan,
    },
}

#[derive(Clone, Copy)]
enum StatementDestination<'a> {
    Label(&'a str),
    Case(usize),
}

impl StatementDestination<'_> {
    fn matches(self, statement: &CHirStmt<'_>) -> bool {
        match self {
            Self::Label(spelling) => matches!(statement,
                CHirStmt::Label(name, _, _) if name.spelling == spelling),
            Self::Case(identity) => statement as *const CHirStmt<'_> as usize == identity,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FunctionExit {
    Explicit(Option<CValue>),
    FellOffEnd,
}

#[derive(Debug)]
struct MachineOutcome {
    return_value: Option<ScalarValue>,
}

/// Interpret one zero-parameter scalar C-HIR function using the current GNU
/// AArch64 compatibility target and a deterministic budget.
///
/// New target-aware callers should use [`interpret_scalar_function_with_target`].
pub fn interpret_scalar_function(
    unit: &CHirUnit<'_>,
    function: SymbolId,
    limits: InterpreterLimits,
) -> Result<InterpretOutcome, InterpreterError> {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    interpret_scalar_function_with_target(unit, function, &target, limits)
}

/// Interpret one zero-parameter scalar C-HIR function against an explicit
/// target data layout. This is the authoritative target-aware entry point.
pub fn interpret_scalar_function_with_target(
    unit: &CHirUnit<'_>,
    function: SymbolId,
    target: &TargetDataLayout,
    limits: InterpreterLimits,
) -> Result<InterpretOutcome, InterpreterError> {
    interpret_scalar_function_with_fp_environment(
        unit,
        function,
        target,
        limits,
        Default::default(),
    )
}

/// Explicit invocation environment, owned by this logical execution only.
pub fn interpret_scalar_function_with_fp_environment(
    unit: &CHirUnit<'_>,
    function: SymbolId,
    target: &TargetDataLayout,
    limits: InterpreterLimits,
    environment: evocc_types::floating::FpEnvironment,
) -> Result<InterpretOutcome, InterpreterError> {
    crate::validate_front_end_builtin_records(unit, target).map_err(|span| {
        InterpreterError::UnsupportedExpression {
            feature: "inconsistent front-end builtin record",
            span,
        }
    })?;
    if crate::inline::needs_execution_resolution(unit) {
        let arena = evocc_common::Arena::default();
        let resolved = crate::inline::substitute_for_execution(unit, &arena, target, true)
            .map_err(|error| InterpreterError::UnsupportedExpression {
                feature: "constant-query substitution",
                span: error.span,
            })?;
        return interpret_resolved_function_with_fp_environment(
            &resolved,
            function,
            target,
            limits,
            environment,
        );
    }
    interpret_resolved_function_with_fp_environment(unit, function, target, limits, environment)
}

pub(crate) fn interpret_resolved_function(
    unit: &CHirUnit<'_>,
    function: SymbolId,
    target: &TargetDataLayout,
    limits: InterpreterLimits,
) -> Result<InterpretOutcome, InterpreterError> {
    interpret_resolved_function_with_fp_environment(
        unit,
        function,
        target,
        limits,
        Default::default(),
    )
}

fn interpret_resolved_function_with_fp_environment(
    unit: &CHirUnit<'_>,
    function: SymbolId,
    target: &TargetDataLayout,
    limits: InterpreterLimits,
    environment: evocc_types::floating::FpEnvironment,
) -> Result<InterpretOutcome, InterpreterError> {
    let linkage = unit.symbol_linkage().map_err(InterpreterError::Linkage)?;
    let requirements = reachable_memory_requirements(unit, function, linkage);
    let memory_mode = requirements.requires_memory;
    let mut interpreter = Interpreter::new(unit, target, limits, memory_mode)?;
    // The requested function is validated before any whole-unit work, so a
    // malformed signature is reported as such instead of surfacing incidentally
    // as an allocation-manifest error from an unrelated part of the unit.
    interpreter.validate_root_function(function)?;
    if memory_mode {
        interpreter.derive_allocation_manifest()?;
        interpreter.initialize_static_objects(&requirements.static_objects)?;
    }
    // Static objects were initialized in the translation environment. They
    // must not accrue flags in the caller's runtime environment.
    interpreter.world = interpreter.world.with_fp_environment(environment);
    interpreter.run_with_calls(function)
}

/// Evaluate a static integer initializer through the C-HIR interpreter's
/// existing expression semantics. This is also the lowering entry point for
/// integer object bytes, so lowering must not grow a separate toy evaluator.
/// Runtime expressions continue to use checked integer arithmetic.
pub fn interpret_static_integer_expression<'arena>(
    unit: &CHirUnit<'arena>,
    expression: &'arena CHirExpr<'arena>,
    target: &TargetDataLayout,
) -> Result<CInteger, InterpreterError> {
    let mut interpreter = Interpreter::new(unit, target, InterpreterLimits::default(), false)?;
    interpreter.eval_static_integer_expression(expression)
}

struct SymbolOrder {
    functions: Vec<SymbolId>,
    objects: BTreeMap<String, u64>,
    labels: BTreeMap<usize, (SymbolId, String, u64)>,
    code_identities: u64,
}

struct Interpreter<'unit, 'arena> {
    program: bool,
    termination: Option<(u128, bool)>,
    linkage: &'unit crate::linkage::SymbolLinkage,
    argument_lists: evocc_memory::ArgumentLists<CValue>,
    unit: &'unit CHirUnit<'arena>,
    target: TargetDataLayout,
    slots: HashMap<SymbolId, Slot>,
    manifest: Option<AllocationSiteManifest<'arena>>,
    static_bindings: HashMap<SymbolId, MemoryBinding>,
    static_compound_bindings: HashMap<usize, MemoryBinding>,
    string_bindings: HashMap<usize, Pointer>,
    symbol_order: OnceCell<SymbolOrder>,
    memory_frame: Option<MemoryFrame<'arena>>,
    memory_mode: bool,
    memory: MemoryState,
    world: WorldState,
    fuel_limit: u64,
    consumed: u64,
    fuel_anchor: Option<SourceSpan>,
    nested_mutation_depth: usize,
    static_integer_evaluation: bool,
    fenv_access: bool,
    integer_expression_types: HashMap<usize, CIntegerType>,
    temporary_origin_site: u64,
}

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    fn new(
        unit: &'unit CHirUnit<'arena>,
        target: &TargetDataLayout,
        limits: InterpreterLimits,
        memory_mode: bool,
    ) -> Result<Self, InterpreterError> {
        let memory_limits = MemoryLimits {
            max_objects: MAX_MEMORY_OBJECTS,
            max_object_bytes: MAX_MEMORY_OBJECT_BYTES,
            max_total_bytes: MAX_MEMORY_TOTAL_BYTES,
        };
        let alignment = unit
            .symbols()
            .symbols_in_source_order()
            .iter()
            .filter(|symbol| symbol.kind() == SymbolKind::Function)
            .map(|symbol| symbol.declared_alignment())
            .max()
            .unwrap_or(4)
            .max(unit.minimum_function_alignment());
        let config = MemoryConfig::for_target(target, memory_limits)
            .and_then(|config| config.with_function_alignment(alignment))
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        let memory =
            MemoryState::new(config).map_err(|error| InterpreterError::MemorySetup { error })?;
        let world = WorldState::new(WorldLimits {
            max_volatile_events: MAX_VOLATILE_EVENTS,
        });
        Ok(Self {
            program: false,
            termination: None,
            linkage: unit.symbol_linkage().map_err(InterpreterError::Linkage)?,
            argument_lists: evocc_memory::ArgumentLists::default(),
            unit,
            target: *target,
            slots: HashMap::new(),
            manifest: None,
            static_bindings: HashMap::new(),
            static_compound_bindings: HashMap::new(),
            string_bindings: HashMap::new(),
            symbol_order: OnceCell::new(),
            memory_frame: None,
            memory_mode,
            memory,
            world,
            fuel_limit: limits.fuel(),
            consumed: 0,
            fuel_anchor: None,
            nested_mutation_depth: 0,
            static_integer_evaluation: false,
            fenv_access: false,
            integer_expression_types: HashMap::new(),
            temporary_origin_site: 0,
        })
    }

    /// Derive the canonical allocation-site manifest for the whole unit.
    ///
    /// This is whole-unit work, so it runs only after the requested function has
    /// been validated; otherwise a defect anywhere in the unit would mask the
    /// diagnostic that actually describes the function under execution.
    fn derive_allocation_manifest(&mut self) -> Result<(), InterpreterError> {
        let manifest = AllocationSiteManifest::derive(self.unit)
            .map_err(|error| InterpreterError::InvalidAllocationManifest { error })?;
        self.temporary_origin_site = u64::try_from(manifest.sites().len())
            .map_err(|_| InterpreterError::MissingAllocationManifest)?;
        self.manifest = Some(manifest);
        Ok(())
    }

    /// Bound only the initializer paths that still use recursive object walks.
    /// Pure integer evaluation uses the fuel-bounded task machine instead.
    fn guard_static_initializer_evaluation(
        &mut self,
        value: &'arena CHirExpr<'arena>,
    ) -> Result<(), InterpreterError> {
        let mut pending: Vec<(&'arena CHirExpr<'arena>, usize)> = vec![(value, 1)];
        while let Some((expression, depth)) = pending.pop() {
            if depth > MAX_STATIC_INITIALIZER_DEPTH {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "deeply-nested-static-initializer",
                    span: expression.span(),
                });
            }
            match expression {
                CHirExpr::Ternary(condition, left, right, _)
                    if self.infer_integer_type(expression).is_err() =>
                {
                    pending.extend([condition, left, right].map(|child| (*child, depth + 1)));
                }
                CHirExpr::Binary(BinaryOp::Eq | BinaryOp::Ne, left, right, _)
                    if self.infer_integer_type(left).is_err()
                        || self.infer_integer_type(right).is_err() =>
                {
                    pending.extend([left, right].map(|child| (*child, depth + 1)));
                }
                _ => push_static_initializer_children(expression, depth, &mut pending),
            }
        }
        Ok(())
    }

    fn initialize_static_objects(
        &mut self,
        required: &BTreeSet<SymbolId>,
    ) -> Result<(), InterpreterError> {
        let mut plans = Vec::new();
        let mut compound_plans = Vec::new();
        let manifest = self
            .manifest
            .as_ref()
            .ok_or(InterpreterError::MissingAllocationManifest)?;
        for site in manifest.sites() {
            if let AllocationSiteKind::StringLiteral(expression) = site.kind() {
                let CHirExpr::StringLit(literal, span) = expression else {
                    unreachable!("manifest string site")
                };
                let (bytes, align) = literal.object_bytes(&self.target).ok_or(
                    InterpreterError::UnsupportedExpression {
                        feature: "invalid string representation",
                        span: *span,
                    },
                )?;
                let layout = AccessLayout::new(bytes.len() as u64, align);
                let bytes = ByteSequence::try_from_bytes(bytes)
                    .map_err(|error| self.memory_error(error, *span))?;
                let (memory, pointer) = self
                    .memory
                    .allocate_string_literal(site.ordinal().raw(), layout, &bytes)
                    .map_err(|error| self.memory_error(error, *span))?;
                self.memory = memory;
                self.string_bindings
                    .insert(expression as *const CHirExpr<'arena> as usize, pointer);
                continue;
            }
            if let AllocationSiteKind::CompoundLiteral {
                expression,
                static_storage: true,
            } = site.kind()
            {
                compound_plans.push((expression, site.ordinal().raw()));
                continue;
            }
            let AllocationSiteKind::Object(symbol) = site.kind() else {
                continue;
            };
            if !required.contains(&symbol) {
                continue;
            }
            if !has_static_storage_duration(self.unit, symbol) {
                continue;
            }
            let object = find_storage_object_declaration(
                self.unit,
                symbol,
                manifest.function_static_objects(),
            )
            .ok_or(InterpreterError::MalformedObjectDeclaration {
                symbol,
                span: site.anchor(),
            })?;
            plans.push((object, site.ordinal().raw()));
        }

        for (expression, site) in &compound_plans {
            let CHirExpr::CompoundLit(ty, _, span) = expression else {
                unreachable!("manifest compound site has compound expression authority")
            };
            let layout = self.access_layout(*ty, *span)?;
            let (memory, address) = self
                .memory
                .allocate_static(*site, layout, InitialState::Zeroed)
                .map_err(|error| self.memory_error(error, *span))?;
            self.memory = memory;
            self.static_compound_bindings.insert(
                *expression as *const CHirExpr<'arena> as usize,
                MemoryBinding { address, ty: *ty },
            );
        }

        for (declaration, site) in &plans {
            let mut layout = self.object_access_layout(
                declaration.ty,
                declaration.initializer.as_ref(),
                declaration.span,
            )?;
            layout.align_bytes = layout.align_bytes.max(
                self.unit
                    .symbols()
                    .symbol(declaration.symbol)
                    .map_err(|_| InterpreterError::MalformedObjectDeclaration {
                        symbol: declaration.symbol,
                        span: declaration.span,
                    })?
                    .declared_alignment(),
            );
            let allocation = if declaration.storage.is_thread_local() {
                self.memory
                    .allocate_thread_local(*site, layout, InitialState::Zeroed)
                    .map(|(memory, address, _)| (memory, address))
            } else {
                self.memory
                    .allocate_static(*site, layout, InitialState::Zeroed)
            };
            let (memory, address) =
                allocation.map_err(|error| self.memory_error(error, declaration.span))?;
            self.memory = memory;
            if self
                .static_bindings
                .insert(
                    declaration.symbol,
                    MemoryBinding {
                        address,
                        ty: declaration.ty,
                    },
                )
                .is_some()
            {
                return Err(InterpreterError::MalformedObjectDeclaration {
                    symbol: declaration.symbol,
                    span: declaration.span,
                });
            }
        }

        for symbol in required {
            if self.static_bindings.contains_key(symbol) {
                continue;
            }
            let entry = self.unit.symbols().symbol(*symbol).map_err(|_| {
                InterpreterError::MalformedObjectDeclaration {
                    symbol: *symbol,
                    span: self
                        .fuel_anchor
                        .unwrap_or_else(|| self.unit.iter().next().expect("nonempty unit").span()),
                }
            })?;
            if entry.storage_class().map(StorageClass::base) != Some(StorageClass::Extern) {
                continue;
            }
            let (size, align) = self
                .target
                .external_object_layout(self.unit.types(), entry.ty())
                .map_err(|_| InterpreterError::UnsupportedType {
                    ty: entry.ty(),
                    span: entry.span(),
                })?;
            let (memory, address) = self
                .memory
                .register_external(
                    self.symbol_order().objects[entry.spelling()],
                    size,
                    align.max(entry.declared_alignment()),
                )
                .map_err(|error| self.memory_error(error, entry.span()))?;
            self.memory = memory;
            self.static_bindings.insert(
                *symbol,
                MemoryBinding {
                    address,
                    ty: entry.ty(),
                },
            );
        }

        for (expression, _) in compound_plans {
            let CHirExpr::CompoundLit(ty, initializer, span) = expression else {
                unreachable!("manifest compound site has compound expression authority")
            };
            if initializer.plan().target_type() != *ty
                || initializer.plan().validate(self.unit.types()).is_err()
            {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "invalid static compound-literal initializer plan",
                    span: *span,
                });
            }
            let binding = self
                .static_compound_bindings
                .get(&(expression as *const CHirExpr<'arena> as usize))
                .copied()
                .ok_or(InterpreterError::UnsupportedExpression {
                    feature: "static compound literal has no binding",
                    span: *span,
                })?;
            self.initialize_static_plan(binding, initializer, *span)?;
            self.memory = self
                .memory
                .finish_static_initializer(binding.address)
                .map_err(|error| self.memory_error(error, *span))?;
            if self.unit.types().is_const_qualified_object(*ty) {
                self.memory = self
                    .memory
                    .freeze_static(binding.address)
                    .map_err(|error| self.memory_error(error, *span))?;
            }
        }

        for (declaration, _) in plans {
            self.validate_initializer_plan(&declaration)?;
            let binding = self
                .static_bindings
                .get(&declaration.symbol)
                .copied()
                .ok_or(InterpreterError::MalformedObjectDeclaration {
                    symbol: declaration.symbol,
                    span: declaration.span,
                })?;
            if let Some(initializer) = &declaration.initializer {
                self.initialize_static_plan(binding, initializer, declaration.span)?;
            }
            self.memory = self
                .memory
                .finish_static_initializer(binding.address)
                .map_err(|error| self.memory_error(error, declaration.span))?;
            if self.unit.types().is_const_qualified_object(binding.ty) {
                self.memory = self
                    .memory
                    .freeze_static(binding.address)
                    .map_err(|error| self.memory_error(error, declaration.span))?;
            }
        }
        Ok(())
    }

    fn initialize_static_plan(
        &mut self,
        binding: MemoryBinding,
        initializer: &crate::ObjectInitializer<'arena>,
        object_span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        for action in initializer.plan().actions() {
            let span = action.span().unwrap_or(object_span);
            let (address, actual_ty, bit_field) =
                self.subobject_address(binding, action.path(), span)?;
            if actual_ty != action.destination_type() {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "static initializer path disagrees with its destination type",
                    span,
                });
            }
            match action {
                InitAction::StoreScalar {
                    destination_type,
                    value,
                    ..
                } => {
                    if let Some(bits) = crate::constant::floating_constant_representation(
                        value,
                        *destination_type,
                        self.unit.types(),
                        self.unit.symbols(),
                        &self.target,
                    ) {
                        self.store_constant_representation(
                            address,
                            *destination_type,
                            bits,
                            false,
                            span,
                        )?;
                        continue;
                    }
                    if let Some(element) =
                        crate::complex::element(self.unit.types(), *destination_type)
                    {
                        let pair = crate::complex::constant_components(
                            value,
                            self.unit.types(),
                            self.unit.symbols(),
                            &self.target,
                        )
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: *destination_type,
                            span,
                        })?;
                        let layout = self.access_layout(element, span)?;
                        let integer = self.integer_type(element, span)?;
                        for (index, value) in pair.into_iter().enumerate() {
                            let at = address
                                .checked_offset(index as i64 * layout.size_bytes as i64)
                                .map_err(|e| self.memory_error(e, span))?;
                            self.store_static_scalar(
                                at,
                                element,
                                CValue::Integer(value.cast(integer)),
                                span,
                            )?;
                        }
                        continue;
                    }
                    self.guard_static_initializer_evaluation(value)?;
                    let value = self.eval_static_initializer_value(value)?;
                    let value = cast_machine_value(
                        value,
                        self.value_type(*destination_type, span)?,
                        span,
                        self.unit.types(),
                        &mut self.memory,
                    )?;
                    if let Some(field) = bit_field {
                        self.store_bit_field(
                            address,
                            *destination_type,
                            field,
                            value,
                            false,
                            span,
                        )?;
                    } else {
                        self.store_static_scalar(address, *destination_type, value, span)?;
                    }
                }
                InitAction::ZeroScalar {
                    destination_type, ..
                } => {
                    if matches!(
                        self.unit.types().get(*destination_type).map(|ty| &ty.kind),
                        Some(TypeKind::Float(
                            evocc_types::FloatKind::F32
                                | evocc_types::FloatKind::F64
                                | evocc_types::FloatKind::F128
                        ))
                    ) {
                        self.store_constant_representation(
                            address,
                            *destination_type,
                            CInteger::new(0, CIntegerType::ULONG),
                            false,
                            span,
                        )?;
                        continue;
                    }
                    if crate::complex::rank(self.unit.types(), *destination_type).is_some() {
                        let layout = self.access_layout(*destination_type, span)?;
                        self.memory = self
                            .memory
                            .store_integer(address, layout, 0)
                            .map_err(|e| self.memory_error(e, span))?;
                        continue;
                    }
                    let value = self.zero_value(*destination_type, span)?;
                    if let Some(field) = bit_field {
                        self.store_bit_field(
                            address,
                            *destination_type,
                            field,
                            value,
                            false,
                            span,
                        )?;
                    } else {
                        self.store_static_scalar(address, *destination_type, value, span)?;
                    }
                }
                InitAction::StoreString {
                    destination_type,
                    literal,
                    ..
                } => {
                    let (layout, value) =
                        self.string_initializer_bytes(*destination_type, literal, span)?;
                    self.store_initializer_bytes(address, layout, &value, span)?;
                }
                InitAction::CopyAggregate { value, span, .. } => {
                    let CHirExpr::CompoundLit(ty, initializer, _) = value else {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "nonconstant aggregate copy in static initializer",
                            span: *span,
                        });
                    };
                    self.guard_static_initializer_evaluation(value)?;
                    let layout = self.access_layout(*ty, *span)?;
                    let zero = ByteSequence::try_from_bytes(vec![0; layout.size_bytes as usize])
                        .map_err(|error| self.memory_error(error, *span))?;
                    self.store_initializer_bytes(address, layout, &zero, *span)?;
                    self.initialize_static_plan(
                        MemoryBinding { address, ty: *ty },
                        initializer,
                        *span,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn store_initializer_bytes(
        &mut self,
        address: Pointer,
        layout: AccessLayout,
        bytes: &ByteSequence,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        if layout.size_bytes == 0 && bytes.is_empty() {
            return self
                .memory
                .check_pointer(address)
                .map_err(|error| self.memory_error(error, span));
        }
        self.memory = self
            .memory
            .store_bytes(address, layout, bytes)
            .map_err(|error| self.memory_error(error, span))?;
        Ok(())
    }

    // An admitted numeric address expression computes representation bits,
    // not an access. Named objects still require the symbolic relocation path.
    fn numeric_static_address(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<Option<CValue>, InterpreterError> {
        let Some(integer) = crate::constant::eval_absolute_address_constant(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) else {
            return Ok(None);
        };
        cast_machine_value(
            CValue::Integer(integer),
            ValueType::Pointer(self.pointer_type_of_expression(expression)?),
            expression.span(),
            self.unit.types(),
            &mut self.memory,
        )
        .map(Some)
    }

    fn eval_static_initializer_value(
        &mut self,
        mut expression: &'arena CHirExpr<'arena>,
    ) -> Result<CValue, InterpreterError> {
        if let Some(value) = crate::constant::fold_floating_integer_arithmetic(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) {
            return Ok(CValue::Integer(value));
        }
        if self.pointer_type_of_expression(expression).is_ok() {
            if let Some(integer) = crate::constant::eval_absolute_address_constant(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            ) {
                let span = expression.span();
                self.tick(span)?;
                return cast_machine_value(
                    CValue::Integer(integer),
                    self.expression_value_type(expression)?,
                    span,
                    self.unit.types(),
                    &mut self.memory,
                );
            }
        }
        let mut casts = Vec::new();
        while let CHirExpr::Cast(ty, operand, span) = expression {
            self.tick(*span)?;
            casts.push((*ty, *span));
            expression = operand;
        }
        let mut value = match expression {
            CHirExpr::LabelAddress { .. } => self.label_address_value(expression),
            CHirExpr::Ternary(condition, left, right, span)
                if self.infer_integer_type(expression).is_err() =>
            {
                self.tick(*span)?;
                let condition =
                    machine_truth(self.eval_static_initializer_value(condition)?, *span)?;
                let value =
                    self.eval_static_initializer_value(if condition { left } else { right })?;
                let target = self.expression_value_type(expression)?;
                cast_machine_value(value, target, *span, self.unit.types(), &mut self.memory)
            }
            CHirExpr::Binary(operator @ (BinaryOp::Add | BinaryOp::Sub), left, right, span) => {
                if let Ok(integer) = self.eval_static_integer_expression(expression) {
                    Ok(CValue::Integer(integer))
                } else {
                    self.tick(*span)?;
                    let left = self.eval_static_initializer_value(left)?;
                    let right = self.eval_static_initializer_value(right)?;
                    match (left, right) {
                        (CValue::Integer(left), CValue::Integer(right)) => {
                            eval_constant_binary_integer(*operator, left, right)
                                .map(CValue::Integer)
                                .map_err(|error| self.integer_error(error, *span))
                        }
                        (left, right) => self.apply_machine_binary(*operator, left, right, *span),
                    }
                }
            }
            CHirExpr::Binary(operator @ (BinaryOp::Eq | BinaryOp::Ne), left, right, span)
                if self.infer_integer_type(left).is_err()
                    || self.infer_integer_type(right).is_err() =>
            {
                self.tick(*span)?;
                let left = self.eval_static_initializer_value(left)?;
                let right = self.eval_static_initializer_value(right)?;
                self.apply_machine_binary(*operator, left, right, *span)
            }
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => {
                if let Some(value) = self.numeric_static_address(expression)? {
                    Ok(value)
                } else {
                    self.static_address_value(operand, false)
                }
            }
            CHirExpr::Ident { facts, .. }
                if self.unit.types().get(facts.ty()).is_some_and(|ty| {
                    matches!(ty.kind, TypeKind::Array { .. } | TypeKind::Function { .. })
                }) =>
            {
                self.static_address_value(expression, true)
            }
            CHirExpr::MemoryRead { place, .. }
                if self
                    .unit
                    .types()
                    .get(place.value_ty())
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Array { .. })) =>
            {
                self.static_address_value(expression, true)
            }
            CHirExpr::StringLit(..) => self.static_address_value(expression, true),
            CHirExpr::Member(..) | CHirExpr::PtrMember(..) => {
                self.static_address_value(expression, true)
            }
            CHirExpr::CompoundLit(_, _, span) => {
                self.static_compound_pointer(expression, *span, true)
            }
            _ => self
                .eval_static_integer_expression(expression)
                .map(CValue::Integer),
        }?;
        for (ty, span) in casts.into_iter().rev() {
            value = cast_machine_value(
                value,
                self.value_type(ty, span)?,
                span,
                self.unit.types(),
                &mut self.memory,
            )?;
        }
        Ok(value)
    }

    fn static_address_value(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        decay: bool,
    ) -> Result<CValue, InterpreterError> {
        let span = expression.span();
        let unsupported = || InterpreterError::UnsupportedExpression {
            feature: "static initializer is not an address constant",
            span,
        };
        let pointer_type =
            |ty| PointerTypeDesc::to_object(self.unit.types(), ty).ok_or_else(unsupported);
        match expression {
            CHirExpr::Ident { symbol, facts, .. } => {
                if self
                    .unit
                    .types()
                    .get(facts.ty())
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))
                {
                    return Ok(CValue::FunctionPointer {
                        value: PointerValue::Function(
                            self.function_identity(*symbol).ok_or_else(unsupported)?,
                        ),
                        ty: PointerTypeDesc::from_expression(
                            self.unit.symbols(),
                            self.unit.types(),
                            expression,
                        )
                        .map_err(|_| unsupported())?,
                    });
                }
                let binding = self.require_memory_binding(*symbol, span)?;
                let ty = if decay {
                    match self.unit.types().get(binding.ty).map(|ty| &ty.kind) {
                        Some(TypeKind::Array { element, .. }) => *element,
                        _ => binding.ty,
                    }
                } else {
                    binding.ty
                };
                Ok(CValue::Pointer {
                    value: binding.address,
                    ty: pointer_type(ty)?,
                })
            }
            CHirExpr::MemoryRead { place, .. } => {
                place
                    .validate_against(self.unit.symbols(), self.unit.types())
                    .map_err(|_| unsupported())?;
                let root = match place.root() {
                    PlaceRoot::Object { symbol } => {
                        self.require_memory_binding(*symbol, span)?.address
                    }
                    PlaceRoot::PointerExpression { pointer, .. } => {
                        expect_machine_pointer(self.eval_static_initializer_value(pointer)?, span)?
                            .0
                    }
                    PlaceRoot::CompoundLiteral { expression, .. } => {
                        expect_machine_pointer(
                            self.static_compound_pointer(expression, span, false)?,
                            span,
                        )?
                        .0
                    }
                };
                let mut indices = Vec::new();
                for projection in place.projections() {
                    if let PlaceProjection::Index(index) = projection {
                        indices.push(self.eval_static_integer_expression(index.index())?);
                    }
                }
                let value = self.project_place_address(root, place, &indices, span)?;
                let ty = if decay {
                    match self.unit.types().get(place.value_ty()).map(|ty| &ty.kind) {
                        Some(TypeKind::Array { element, .. }) => *element,
                        _ => place.value_ty(),
                    }
                } else {
                    place.value_ty()
                };
                Ok(CValue::Pointer {
                    value,
                    ty: PointerTypeDesc::to_object(self.unit.types(), ty)
                        .ok_or_else(unsupported)?,
                })
            }
            CHirExpr::StringLit(..) => self.string_pointer(expression, decay),
            CHirExpr::CompoundLit(..) => self.static_compound_pointer(expression, span, decay),
            CHirExpr::Member(base, name, _) | CHirExpr::PtrMember(base, name, _) => {
                let base = if matches!(expression, CHirExpr::Member(..)) {
                    self.static_address_value(base, false)?
                } else {
                    self.eval_static_initializer_value(base)?
                };
                let (mut address, base_ty) = expect_machine_pointer(base, span)?;
                let path = crate::record_member_path(self.unit.types(), base_ty.pointee(), name)
                    .ok_or_else(unsupported)?;
                let mut ty = path.last().ok_or_else(unsupported)?.field_ty();
                for field in path {
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), field.tag())
                        .map_err(|_| unsupported())?;
                    let offset = layout
                        .fields
                        .iter()
                        .find(|entry| entry.field_index == field.field_index())
                        .ok_or_else(unsupported)?
                        .byte_offset;
                    address = address
                        .checked_offset(i64::try_from(offset).map_err(|_| unsupported())?)
                        .map_err(|error| self.memory_error(error, span))?;
                }
                if decay {
                    ty = match self.unit.types().get(ty).map(|ty| &ty.kind) {
                        Some(TypeKind::Array { element, .. }) => *element,
                        _ => return Err(unsupported()),
                    };
                }
                Ok(CValue::Pointer {
                    value: address,
                    ty: pointer_type(ty)?,
                })
            }
            CHirExpr::Unary(UnaryOp::Deref, operand, _) => {
                self.eval_static_initializer_value(operand)
            }
            _ => Err(unsupported()),
        }
    }

    fn eval_static_integer_expression(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<CInteger, InterpreterError> {
        if let Some(value) = crate::constant::fold_floating_integer_arithmetic(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) {
            return Ok(value);
        }
        let previous = std::mem::replace(&mut self.static_integer_evaluation, true);
        let result = self.run_tasks(vec![MachineTask::EvalExpression {
            expression,
            allow_root_mutation: false,
        }]);
        self.static_integer_evaluation = previous;
        let (outcome, mut values) = result?;
        if outcome.is_some() || values.len() != 1 {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "static initializer did not produce one integer value",
                span: expression.span(),
            });
        }
        expect_machine_integer(
            pop_machine_value(&mut values, expression.span())?,
            expression.span(),
        )
    }

    fn static_compound_pointer(
        &self,
        expression: &'arena CHirExpr<'arena>,
        span: SourceSpan,
        decay_array: bool,
    ) -> Result<CValue, InterpreterError> {
        let CHirExpr::CompoundLit(ty, _, _) = expression else {
            unreachable!("static compound pointer requires compound expression")
        };
        let binding = self
            .static_compound_bindings
            .get(&(expression as *const CHirExpr<'arena> as usize))
            .copied()
            .ok_or(InterpreterError::UnsupportedExpression {
                feature: "static compound literal has no binding",
                span,
            })?;
        let pointee = if decay_array {
            match self.unit.types().get(*ty).map(|canonical| &canonical.kind) {
                Some(TypeKind::Array { element, .. }) => *element,
                _ => *ty,
            }
        } else {
            *ty
        };
        let pointer = PointerTypeDesc::to_object(self.unit.types(), pointee)
            .ok_or(InterpreterError::UnsupportedType { ty: pointee, span })?;
        Ok(CValue::Pointer {
            value: binding.address,
            ty: pointer,
        })
    }

    fn run_with_calls(mut self, function: SymbolId) -> Result<InterpretOutcome, InterpreterError> {
        let outcome = self
            .run_root(function)?
            .expect("function invocation has a result");
        Ok(InterpretOutcome {
            return_value: outcome.return_value,
            steps: self.consumed,
            memory: self.memory,
            world: self.world,
        })
    }

    fn run_root(&mut self, function: SymbolId) -> Result<Option<MachineOutcome>, InterpreterError> {
        let mut tasks = Vec::new();
        let root_span = self
            .unit
            .symbols()
            .symbol(function)
            .map(|symbol| symbol.span())
            .map_err(|_| InterpreterError::UnknownFunction { symbol: function })?;
        self.enter_function(function, Vec::new(), None, None, None, false, 0, &mut tasks)?;
        let (outcome, _) = self.run_tasks(tasks)?;
        if self.termination.is_some() {
            return Ok(None);
        }
        let outcome = outcome.ok_or(InterpreterError::UnsupportedStatement {
            feature: "interpreter call stack ended without a root result",
            span: root_span,
        })?;
        Ok(Some(outcome))
    }

    fn run_tasks(
        &mut self,
        mut tasks: Vec<MachineTask<'arena>>,
    ) -> Result<(Option<MachineOutcome>, Vec<CValue>), InterpreterError> {
        let mut values = Vec::new();
        while let Some(task) = tasks.pop() {
            if self.termination.is_some() {
                return Ok((None, values));
            }
            let outcome = match task {
                MachineTask::BeginConstantRepresentationStore { place, bits, span } => {
                    self.tick(span)?;
                    let completion = tasks.len();
                    self.schedule_place_operands(&place, span, &mut tasks, &mut values)?;
                    // Operand tasks must run before the store. Insert the
                    // completion below them, using the same scheduling order
                    // as every other projected memory access.
                    tasks.insert(
                        completion,
                        MachineTask::FinishConstantRepresentationStore { place, bits, span },
                    );
                    None
                }
                MachineTask::FinishConstantRepresentationStore { place, bits, span } => {
                    let subscripts = place
                        .projections()
                        .iter()
                        .filter(|projection| projection.source_expression().is_some())
                        .count();
                    let address =
                        self.resolve_projected_address(&place, subscripts, &mut values, span)?;
                    let layout = self.place_access_layout(&place, span)?;
                    if place.access_qualifiers().is_volatile {
                        let (memory, world) = self
                            .memory
                            .volatile_store_integer(&self.world, address, layout, bits.raw_bits())
                            .map_err(|e| self.memory_error(e, span))?;
                        self.memory = memory;
                        self.world = world;
                    } else {
                        self.memory = self
                            .memory
                            .store_integer(address, layout, bits.raw_bits())
                            .map_err(|e| self.memory_error(e, span))?;
                    }
                    None
                }
                MachineTask::IndirectGoto(span) => {
                    let value = pop_machine_value(&mut values, span)?;
                    let error = || InterpreterError::InvalidIndirectBranchTarget { span };
                    let address = match value {
                        CValue::Pointer { value, .. } => {
                            self.memory.pointer_address(PointerValue::object(value))
                        }
                        CValue::FunctionPointer { value, .. } => self.memory.pointer_address(value),
                        CValue::Uninitialized(_) => {
                            return Err(InterpreterError::UninitializedValue { span })
                        }
                        _ => return Err(error()),
                    }
                    .map_err(|_| error())?;
                    let PointerValue::Function(identity) = self
                        .memory
                        .resolve_function_pointer(PointerValue::Exposed(address))
                        .map_err(|_| error())?
                    else {
                        return Err(error());
                    };
                    let current = tasks
                        .iter()
                        .rev()
                        .find_map(|task| match task {
                            MachineTask::FunctionEnd(end) => Some(end.symbol),
                            _ => None,
                        })
                        .ok_or_else(error)?;
                    let label = self
                        .symbol_order()
                        .labels
                        .values()
                        .find(|(owner, _, id)| *owner == current && *id == identity)
                        .map(|(_, label, _)| label.clone())
                        .ok_or_else(error)?;
                    self.unwind_goto(&mut tasks, &label, span)?;
                    None
                }
                MachineTask::ApplyVarArgs {
                    op,
                    requested_ty,
                    span,
                } => {
                    self.apply_varargs(op, requested_ty, span, &mut values)?;
                    None
                }
                MachineTask::FunctionEnd(end) => {
                    self.finish_function(end, FunctionExit::FellOffEnd, &mut values)?
                }
                MachineTask::RestoreSlots(saved) => {
                    self.restore_slots(saved);
                    None
                }
                MachineTask::ExitMemoryScope(exit) => {
                    self.exit_memory_scope(exit)?;
                    None
                }
                MachineTask::RestoreFuelAnchor(previous) => {
                    self.fuel_anchor = previous;
                    None
                }
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                    None
                }
                MachineTask::RestoreFenvAccess(previous) => {
                    self.fenv_access = previous;
                    None
                }
                MachineTask::ExecDeclaration(declaration) => {
                    self.schedule_machine_declaration(declaration, &mut tasks)?;
                    None
                }
                MachineTask::ExecStatement(statement) => {
                    self.exec_machine_statement(statement, &mut tasks, &mut values)?
                }
                MachineTask::SwitchEnd => None,
                MachineTask::CompoundContinue { items, index } => {
                    if let Some(item) = items.get(index) {
                        tasks.push(MachineTask::CompoundContinue {
                            items,
                            index: index + 1,
                        });
                        tasks.push(match item {
                            CHirBlockItem::Declaration(declaration) => {
                                MachineTask::ExecDeclaration(declaration)
                            }
                            CHirBlockItem::Statement(statement) => {
                                MachineTask::ExecStatement(statement)
                            }
                        });
                    }
                    None
                }
                MachineTask::PreserveStatementExpressionValue { parent, span } => {
                    // A complete-object rvalue must survive the closing brace;
                    // a pointer value still points at the original object and
                    // must not acquire an extended lifetime here.
                    if let Some(value @ CValue::Aggregate { ty, .. }) = values.last().copied() {
                        let layout = self.access_layout(ty, span)?;
                        let frame = self
                            .memory_frame
                            .as_ref()
                            .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                        let (memory, destination) = self
                            .memory
                            .allocate_automatic(
                                frame.id,
                                parent,
                                self.temporary_origin_site,
                                layout,
                            )
                            .map_err(|error| self.memory_error(error, span))?;
                        self.memory = memory;
                        let preserved = self.store_memory_value(
                            destination,
                            ty,
                            Qualifiers::default(),
                            value,
                            span,
                        )?;
                        *values
                            .last_mut()
                            .expect("statement expression value checked") = preserved;
                    }
                    None
                }
                MachineTask::StatementExpressionContinue {
                    items,
                    index,
                    value,
                    span,
                } => {
                    if let Some(item) = items.get(index) {
                        let item_value = match item {
                            CHirBlockItem::Statement(statement) => {
                                statement_expression_value(statement)
                            }
                            CHirBlockItem::Declaration(_) => None,
                        };
                        let is_value = matches!((item_value, value), (Some(item), Some(value)) if std::ptr::eq(item, value));
                        if index + 1 < items.len() {
                            tasks.push(MachineTask::StatementExpressionContinue {
                                items,
                                index: index + 1,
                                value,
                                span,
                            });
                        }
                        if is_value {
                            let expression = item_value.expect(
                                "the statement-expression value predicate checked this shape",
                            );
                            if crate::expression_is_void(
                                expression,
                                self.unit.symbols(),
                                self.unit.types(),
                            ) {
                                self.schedule_discarded_expression(expression, &mut tasks)?;
                            } else {
                                tasks.push(MachineTask::EvalExpression {
                                    expression,
                                    allow_root_mutation: true,
                                });
                            }
                        } else {
                            tasks.push(match item {
                                CHirBlockItem::Declaration(declaration) => {
                                    MachineTask::ExecDeclaration(declaration)
                                }
                                CHirBlockItem::Statement(statement) => {
                                    MachineTask::ExecStatement(statement)
                                }
                            });
                        }
                    } else if value.is_some() {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "statement expression result is not its final statement",
                            span,
                        });
                    }
                    None
                }
                MachineTask::WhileCheck { condition, body } => {
                    tasks.push(MachineTask::AfterWhileCondition {
                        condition,
                        body,
                        span: condition.span(),
                    });
                    tasks.push(MachineTask::EvalExpression {
                        expression: condition,
                        allow_root_mutation: true,
                    });
                    None
                }
                MachineTask::AfterWhileCondition {
                    condition,
                    body,
                    span,
                } => {
                    if self.complex_truth(pop_machine_value(&mut values, span)?, span)? {
                        tasks.push(MachineTask::WhileCheck { condition, body });
                        tasks.push(MachineTask::ExecStatement(body));
                    }
                    None
                }
                MachineTask::DoWhileCondition { condition, body } => {
                    tasks.push(MachineTask::AfterDoWhileCondition {
                        condition,
                        body,
                        span: condition.span(),
                    });
                    tasks.push(MachineTask::EvalExpression {
                        expression: condition,
                        allow_root_mutation: true,
                    });
                    None
                }
                MachineTask::AfterDoWhileCondition {
                    condition,
                    body,
                    span,
                } => {
                    if self.complex_truth(pop_machine_value(&mut values, span)?, span)? {
                        tasks.push(MachineTask::DoWhileCondition { condition, body });
                        tasks.push(MachineTask::ExecStatement(body));
                    }
                    None
                }
                MachineTask::ForCheck {
                    condition,
                    step,
                    body,
                    span,
                } => {
                    if let Some(condition) = condition {
                        tasks.push(MachineTask::AfterForCondition {
                            condition: Some(condition),
                            step,
                            body,
                            span,
                        });
                        tasks.push(MachineTask::EvalExpression {
                            expression: condition,
                            allow_root_mutation: true,
                        });
                    } else {
                        tasks.push(MachineTask::ForStep {
                            condition,
                            step,
                            body,
                            span,
                        });
                        tasks.push(MachineTask::ExecStatement(body));
                    }
                    None
                }
                MachineTask::AfterForCondition {
                    condition,
                    step,
                    body,
                    span,
                } => {
                    if self.complex_truth(pop_machine_value(&mut values, span)?, span)? {
                        tasks.push(MachineTask::ForStep {
                            condition,
                            step,
                            body,
                            span,
                        });
                        tasks.push(MachineTask::ExecStatement(body));
                    }
                    None
                }
                MachineTask::ForStep {
                    condition,
                    step,
                    body,
                    span,
                } => {
                    if let Some(step) = step {
                        tasks.push(MachineTask::ForCheck {
                            condition,
                            step: Some(step),
                            body,
                            span,
                        });
                        self.schedule_discarded_expression(step, &mut tasks)?;
                    } else {
                        tasks.push(MachineTask::ForCheck {
                            condition,
                            step,
                            body,
                            span,
                        });
                    }
                    None
                }
                MachineTask::DiscardExpression(span) => {
                    pop_machine_value(&mut values, span)?;
                    None
                }
                MachineTask::FinishInitializer { symbol, ty, span } => {
                    let value =
                        expect_machine_integer(pop_machine_value(&mut values, span)?, span)?
                            .cast(ty);
                    let slot = self
                        .slots
                        .get_mut(&symbol)
                        .ok_or(InterpreterError::MalformedObjectDeclaration { symbol, span })?;
                    if slot.ty != ty || !slot.active {
                        return Err(InterpreterError::MalformedObjectDeclaration { symbol, span });
                    }
                    slot.value = Some(value);
                    None
                }
                MachineTask::FinishMemoryInitializer { symbol, span } => {
                    let value = pop_machine_value(&mut values, span)?;
                    self.store_bound_value(symbol, value, span)?;
                    None
                }
                MachineTask::ContinueMemoryInitializer { target, index } => {
                    self.continue_memory_initializer(target, index, &mut tasks)?;
                    None
                }
                MachineTask::FinishMemorySubobject { target, index } => {
                    let action = target
                        .initializer
                        .plan()
                        .actions()
                        .get(index)
                        .ok_or_else(|| invalid_initializer_target(target, target.span))?;
                    let (path, span) = match action {
                        InitAction::StoreScalar { path, span, .. }
                        | InitAction::CopyAggregate { path, span, .. } => (path, span),
                        InitAction::StoreString { .. } | InitAction::ZeroScalar { .. } => {
                            return Err(invalid_initializer_target(target, target.span));
                        }
                    };
                    let value = pop_machine_value(&mut values, *span)?;
                    let (address, ty, bit_field) =
                        self.subobject_address(target.binding, path, *span)?;
                    let value =
                        self.cast_runtime_value(value, self.value_type(ty, *span)?, *span)?;
                    if let Some(field) = bit_field {
                        self.store_bit_field(address, ty, field, value, false, *span)?;
                    } else {
                        self.store_initialized_value(address, ty, value, *span)?;
                    }
                    None
                }
                MachineTask::YieldCompoundLiteral {
                    binding,
                    as_address,
                    span,
                } => {
                    let pointer_type = PointerTypeDesc::to_object(self.unit.types(), binding.ty)
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: binding.ty,
                            span,
                        })?;
                    if as_address
                        || matches!(
                            self.unit.types().get(binding.ty).map(|ty| &ty.kind),
                            Some(TypeKind::Array { .. })
                        )
                    {
                        values.push(CValue::Pointer {
                            value: binding.address,
                            ty: if as_address {
                                pointer_type
                            } else {
                                let TypeKind::Array { element, .. } = self
                                    .unit
                                    .types()
                                    .get(binding.ty)
                                    .expect("compound type checked above")
                                    .kind
                                else {
                                    unreachable!()
                                };
                                PointerTypeDesc::to_object(self.unit.types(), element).ok_or(
                                    InterpreterError::UnsupportedType { ty: element, span },
                                )?
                            },
                        });
                    } else {
                        let qualifiers = self
                            .unit
                            .types()
                            .get(binding.ty)
                            .ok_or(InterpreterError::UnsupportedType {
                                ty: binding.ty,
                                span,
                            })?
                            .qualifiers;
                        values.push(self.load_memory_value(
                            binding.address,
                            binding.ty,
                            qualifiers,
                            span,
                        )?);
                    }
                    None
                }
                MachineTask::FinishReturnValue(span) => {
                    let value = pop_machine_value(&mut values, span)?;
                    let target = tasks
                        .iter()
                        .rev()
                        .find_map(|task| match task {
                            MachineTask::FunctionEnd(end) => Some(end.return_value_type),
                            _ => None,
                        })
                        .ok_or(InterpreterError::UnsupportedStatement {
                            feature: "missing return continuation",
                            span,
                        })?;
                    // Convert under the return statement's policy before unwinding.
                    let value = match value {
                        CValue::Uninitialized(source) if source == target => value,
                        _ => self.cast_runtime_value(value, target, span)?,
                    };
                    let end = self.pop_function_end(&mut tasks, span)?;
                    self.finish_function(end, FunctionExit::Explicit(Some(value)), &mut values)?
                }
                MachineTask::FinishVoidReturn(span) => {
                    let end = self.pop_function_end(&mut tasks, span)?;
                    self.finish_function(end, FunctionExit::Explicit(None), &mut values)?
                }
                MachineTask::DiscardSelectedExpression {
                    then_expression,
                    else_expression,
                } => {
                    let selected = if self.complex_truth(
                        pop_machine_value(&mut values, then_expression.span())?,
                        then_expression.span(),
                    )? {
                        then_expression
                    } else {
                        else_expression
                    };
                    self.schedule_discarded_expression(selected, &mut tasks)?;
                    None
                }
                MachineTask::SelectIf {
                    then_statement,
                    else_statement,
                } => {
                    let condition = self.complex_truth(
                        pop_machine_value(&mut values, then_statement.span())?,
                        then_statement.span(),
                    )?;
                    if condition {
                        tasks.push(MachineTask::ExecStatement(then_statement));
                    } else if let Some(else_statement) = else_statement {
                        tasks.push(MachineTask::ExecStatement(else_statement));
                    }
                    None
                }
                MachineTask::EvalExpression {
                    expression,
                    allow_root_mutation,
                } => {
                    self.schedule_expression(
                        expression,
                        allow_root_mutation,
                        &mut tasks,
                        &mut values,
                    )?;
                    None
                }
                MachineTask::ApplyFloating(access) => {
                    let start = values.len().checked_sub(access.operands.len()).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "floating operand stack",
                            span: access.span,
                        },
                    )?;
                    let arguments = values.split_off(start);
                    let strict = self.strict_floating();
                    values.push(floating::builtin(
                        access,
                        &arguments,
                        &mut self.world,
                        strict,
                    )?);
                    None
                }
                MachineTask::ApplyVector(access) => {
                    let start = values.len().checked_sub(access.operands.len()).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "vector operand stack",
                            span: access.span,
                        },
                    )?;
                    let arguments = values.split_off(start);
                    values.push(self.apply_vector(access, &arguments)?);
                    None
                }
                MachineTask::ApplyComplex(access) => {
                    let start = values.len().checked_sub(access.operands.len()).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "complex operand stack",
                            span: access.span,
                        },
                    )?;
                    let arguments = values.split_off(start);
                    values.push(self.apply_complex(access, &arguments)?);
                    None
                }
                MachineTask::ApplyAtomic(access) => {
                    let _order = expect_machine_integer(
                        pop_machine_value(&mut values, access.span)?,
                        access.span,
                    )?;
                    let stored = if access.value.is_some() {
                        Some(pop_machine_value(&mut values, access.span)?)
                    } else {
                        None
                    };
                    let expected = if access.expected.is_some() {
                        Some(pop_machine_value(&mut values, access.span)?)
                    } else {
                        None
                    };
                    let address = pop_machine_value(&mut values, access.span)?;
                    values.push(self.apply_atomic(access, address, stored, expected)?);
                    None
                }
                MachineTask::ApplyUnary {
                    operator,
                    expression_span,
                    operand_span,
                } => {
                    let operand = pop_machine_value(&mut values, expression_span)?;
                    if operator == UnaryOp::Not {
                        values.push(CValue::Integer(CInteger::int(i32::from(
                            !self.complex_truth(operand, expression_span)?,
                        ))));
                        continue;
                    }
                    if let CValue::Floating(value) = operand {
                        values.push(CValue::Floating(match operator {
                            UnaryOp::Plus => value,
                            UnaryOp::Neg => value.negated(),
                            _ => {
                                return Err(InterpreterError::UnsupportedExpression {
                                    feature: "invalid floating unary operator",
                                    span: expression_span,
                                })
                            }
                        }));
                        continue;
                    }
                    let operand = expect_machine_integer(operand, expression_span)?;
                    let value = if self.static_integer_evaluation {
                        eval_constant_unary_integer(operator, operand)
                    } else {
                        self.runtime_integer_unary(operator, operand)
                    }
                    .map_err(|error| {
                        let error_span = if operator == crate::UnaryOp::Neg {
                            unary_operator_span(expression_span, operand_span)
                        } else {
                            expression_span
                        };
                        self.integer_error(error, error_span)
                    })?;
                    values.push(CValue::Integer(value));
                    None
                }
                MachineTask::ContinueBinary {
                    operator,
                    right,
                    span,
                    allow_root_mutation,
                } => {
                    let left = pop_machine_value(&mut values, span)?;
                    match operator {
                        BinaryOp::And if !self.complex_truth(left, span)? => {
                            values.push(CValue::Integer(CInteger::int(0)));
                        }
                        BinaryOp::Or if self.complex_truth(left, span)? => {
                            values.push(CValue::Integer(CInteger::int(1)));
                        }
                        BinaryOp::Comma => {
                            tasks.push(MachineTask::EvalExpression {
                                expression: right,
                                allow_root_mutation,
                            });
                        }
                        _ => {
                            tasks.push(MachineTask::ApplyBinary {
                                operator,
                                left,
                                span,
                            });
                            tasks.push(MachineTask::EvalExpression {
                                expression: right,
                                allow_root_mutation: matches!(
                                    operator,
                                    BinaryOp::And | BinaryOp::Or
                                ),
                            });
                        }
                    }
                    None
                }
                MachineTask::ApplyBinary {
                    operator,
                    left,
                    span,
                } => {
                    let right = pop_machine_value(&mut values, span)?;
                    values.push(self.apply_machine_binary(operator, left, right, span)?);
                    None
                }
                MachineTask::ApplyCheckedArithmetic {
                    operator,
                    target_type,
                    span,
                } => {
                    let (address, pointer_type) =
                        expect_machine_pointer(pop_machine_value(&mut values, span)?, span)?;
                    if pointer_type.pointee() != target_type {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "inconsistent checked-arithmetic target type",
                            span,
                        });
                    }
                    let right =
                        expect_machine_integer(pop_machine_value(&mut values, span)?, span)?;
                    let left = expect_machine_integer(pop_machine_value(&mut values, span)?, span)?;
                    let target_integer = self.integer_type(target_type, span)?;
                    let (stored, overflowed) =
                        eval_checked_arithmetic(operator, left, right, target_integer);
                    let qualifiers = self
                        .unit
                        .types()
                        .get(target_type)
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: target_type,
                            span,
                        })?
                        .qualifiers;
                    self.store_memory_value(
                        address,
                        target_type,
                        qualifiers,
                        CValue::Integer(stored),
                        span,
                    )?;
                    values.push(CValue::Integer(CInteger::int(i32::from(overflowed))));
                    None
                }
                MachineTask::ApplyBitBuiltin { operator, span } => {
                    let operand =
                        expect_machine_integer(pop_machine_value(&mut values, span)?, span)?;
                    let value = eval_bit_builtin(operator, operand)
                        .ok_or(InterpreterError::BitBuiltinZeroOperand { span })?;
                    values.push(CValue::Integer(value));
                    None
                }
                MachineTask::SelectTernary {
                    then_expression,
                    else_expression,
                    result_type,
                } => {
                    let condition = self.complex_truth(
                        pop_machine_value(&mut values, then_expression.span())?,
                        then_expression.span(),
                    )?;
                    tasks.push(MachineTask::CastValue {
                        target: result_type,
                        span: then_expression.span(),
                    });
                    tasks.push(MachineTask::EvalExpression {
                        expression: if condition {
                            then_expression
                        } else {
                            else_expression
                        },
                        allow_root_mutation: true,
                    });
                    None
                }
                MachineTask::SelectElvis {
                    else_expression,
                    result_type,
                    span,
                } => {
                    let operand = pop_machine_value(&mut values, span)?;
                    if self.complex_truth(operand, span)? {
                        values.push(self.cast_runtime_value(operand, result_type, span)?);
                    } else {
                        tasks.push(MachineTask::CastValue {
                            target: result_type,
                            span,
                        });
                        tasks.push(MachineTask::EvalExpression {
                            expression: else_expression,
                            allow_root_mutation: true,
                        });
                    }
                    None
                }
                MachineTask::CastValue { target, span } => {
                    let value = pop_machine_value(&mut values, span)?;
                    values.push(self.cast_runtime_value(value, target, span)?);
                    None
                }
                MachineTask::ApplyAssignment {
                    operator,
                    symbol,
                    target_span,
                    span,
                } => {
                    let right = pop_machine_value(&mut values, span)?;
                    if self.memory_mode {
                        values.push(self.apply_bound_assignment(
                            operator,
                            symbol,
                            right,
                            target_span,
                            span,
                        )?);
                    } else {
                        let right = expect_machine_integer(right, span)?;
                        values.push(CValue::Integer(self.apply_assignment_value(
                            operator,
                            symbol,
                            right,
                            target_span,
                            span,
                        )?));
                    }
                    None
                }
                MachineTask::LoadDereference { pointer_type, span } => {
                    let (address, actual_type) =
                        expect_machine_pointer(pop_machine_value(&mut values, span)?, span)?;
                    if actual_type != pointer_type {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "inconsistent pointer type at dereference",
                            span,
                        });
                    }
                    values.push(self.load_memory_value(
                        address,
                        pointer_type.pointee(),
                        pointer_type.pointee_qualifiers(),
                        span,
                    )?);
                    None
                }
                MachineTask::ApplyScalarUpdate {
                    operator,
                    symbol,
                    target_span,
                    span,
                } => {
                    values.push(self.apply_scalar_update(operator, symbol, target_span, span)?);
                    None
                }
                MachineTask::ApplyIndirectUpdate {
                    operator,
                    bit_field,
                    layout,
                    value_type,
                    qualifiers,
                    target_span,
                    span,
                } => {
                    let (address, _) =
                        expect_machine_pointer(pop_machine_value(&mut values, span)?, target_span)?;
                    let old = if let Some(field) = bit_field {
                        self.load_bit_field(
                            address,
                            value_type,
                            field,
                            qualifiers.is_volatile,
                            target_span,
                        )?
                    } else {
                        self.load_memory_value_with_layout(
                            address,
                            value_type,
                            qualifiers,
                            target_span,
                            layout,
                        )?
                    };
                    let stepped = self.step_update_value(old, operator, span)?;
                    let stepped = if let Some(field) = bit_field {
                        self.store_bit_field(
                            address,
                            value_type,
                            field,
                            stepped,
                            qualifiers.is_volatile,
                            span,
                        )?
                    } else {
                        self.store_memory_value_with_layout(
                            address, value_type, qualifiers, stepped, span, layout,
                        )?
                    };
                    // A prefix form yields the updated object; a postfix form
                    // yields the value it held before the update.
                    values.push(match operator {
                        UpdateOp::PreIncrement | UpdateOp::PreDecrement => stepped,
                        UpdateOp::PostIncrement | UpdateOp::PostDecrement => old,
                    });
                    None
                }
                MachineTask::FoldPlaceAddress { place, span } => {
                    let subscripts = place
                        .projections()
                        .iter()
                        .filter(|projection| projection.source_expression().is_some())
                        .count();
                    let address =
                        self.resolve_projected_address(place, subscripts, &mut values, span)?;
                    values.push(CValue::Pointer {
                        value: address,
                        ty: PointerTypeDesc::from_addressed_place(place),
                    });
                    None
                }
                MachineTask::LoadProjectedPlace { place, span } => {
                    let subscripts = place
                        .projections()
                        .iter()
                        .filter(|projection| projection.source_expression().is_some())
                        .count();
                    let address =
                        self.resolve_projected_address(place, subscripts, &mut values, span)?;
                    let value = if let Some(field) = self.place_bit_field(place, span)? {
                        self.load_bit_field(
                            address,
                            place.value_ty(),
                            field,
                            place.access_qualifiers().is_volatile,
                            span,
                        )?
                    } else {
                        self.load_memory_value_with_layout(
                            address,
                            place.value_ty(),
                            place.access_qualifiers(),
                            span,
                            self.place_access_layout(place, span)?,
                        )?
                    };
                    values.push(value);
                    None
                }
                MachineTask::LoadAggregateMember { name, span } => {
                    let aggregate = pop_machine_value(&mut values, span)?;
                    let (address, ty, qualifiers) =
                        self.aggregate_member_address(aggregate, name, span)?;
                    values.push(self.load_memory_value(address, ty, qualifiers, span)?);
                    None
                }
                MachineTask::ApplyIndirectAssignment {
                    operator,
                    bit_field,
                    layout,
                    pointer_type,
                    value_type,
                    qualifiers,
                    target_span,
                    span,
                } => {
                    let right = pop_machine_value(&mut values, span)?;
                    let (address, actual_type) =
                        expect_machine_pointer(pop_machine_value(&mut values, span)?, target_span)?;
                    if actual_type != pointer_type || pointer_type.pointee() != value_type {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "inconsistent pointer type at memory assignment",
                            span: target_span,
                        });
                    }
                    if let Some(field) = bit_field {
                        let right = expect_machine_integer(right, span)?;
                        let result = if operator == AssignmentOp::Simple {
                            right
                        } else {
                            let left = expect_machine_integer(
                                self.load_bit_field(
                                    address,
                                    value_type,
                                    field,
                                    qualifiers.is_volatile,
                                    target_span,
                                )?,
                                target_span,
                            )?;
                            let binary = assignment_binary(operator).ok_or(
                                InterpreterError::UnsupportedExpression {
                                    feature: "invalid bit-field assignment",
                                    span,
                                },
                            )?;
                            self.runtime_integer_binary(binary, left, right, span)?
                        };
                        values.push(self.store_bit_field(
                            address,
                            value_type,
                            field,
                            CValue::Integer(result),
                            qualifiers.is_volatile,
                            span,
                        )?);
                    } else {
                        values.push(self.apply_memory_assignment(
                            operator,
                            address,
                            value_type,
                            qualifiers,
                            right,
                            target_span,
                            span,
                            layout,
                        )?);
                    }
                    None
                }
                MachineTask::AllocateDynamic { expression } => {
                    let span = expression.span();
                    let size = expect_machine_integer(pop_machine_value(&mut values, span)?, span)?
                        .cast(CIntegerType::ULONG)
                        .raw_bits() as u64;
                    let origin = self
                        .manifest
                        .as_ref()
                        .ok_or(InterpreterError::MissingAllocationManifest)?
                        .dynamic_stack_site(expression)
                        .ok_or(InterpreterError::UnsupportedExpression {
                            feature: "dynamic allocation has no site",
                            span,
                        })?
                        .ordinal()
                        .raw();
                    let frame = self.memory_frame.as_ref().ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "dynamic allocation has no frame",
                            span,
                        },
                    )?;
                    let (memory, value) = self
                        .memory
                        .allocate_automatic(
                            frame.id,
                            frame.root_scope,
                            origin,
                            AccessLayout::new(size, 16),
                        )
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    let ty = PointerTypeDesc::from_expression(
                        self.unit.symbols(),
                        self.unit.types(),
                        expression,
                    )
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "dynamic allocation has no pointer type",
                        span,
                    })?;
                    values.push(CValue::Pointer { value, ty });
                    None
                }
                MachineTask::SelectSwitch { body, span } => {
                    let controlling =
                        expect_machine_integer(pop_machine_value(&mut values, span)?, span)?;
                    self.schedule_switch(controlling, body, &mut tasks)?;
                    None
                }
                MachineTask::EvalCallArguments {
                    callee,
                    arguments,
                    index,
                    values: arguments_so_far,
                    span,
                    expects_value,
                } => {
                    if index == 0 {
                        self.check_neon_immediates(callee, arguments, span)?;
                    }
                    if index == arguments.len() {
                        if matches!(
                            callee,
                            CHirExpr::Ident {
                                facts,
                                ..
                            } if facts.category() == ExprCategory::FunctionDesignator
                        ) {
                            let value_base = values.len();
                            if let Some(result) = self.start_call(
                                callee,
                                arguments_so_far,
                                span,
                                expects_value,
                                &mut tasks,
                                value_base,
                            )? {
                                if expects_value {
                                    values.push(result);
                                }
                            }
                        } else {
                            if self.call_designator_function_type(callee).is_none() {
                                return Err(InterpreterError::UnsupportedExpression {
                                    feature: "indirect scalar function call",
                                    span: callee.span(),
                                });
                            }
                            tasks.push(MachineTask::StartIndirectCall {
                                arguments: arguments_so_far,
                                span,
                                expects_value,
                                value_base: values.len(),
                            });
                            tasks.push(MachineTask::EvalExpression {
                                expression: callee,
                                allow_root_mutation: false,
                            });
                        }
                    } else {
                        tasks.push(MachineTask::CollectCallArgument {
                            callee,
                            arguments,
                            index,
                            values: arguments_so_far,
                            span,
                            expects_value,
                        });
                        tasks.push(MachineTask::EvalExpression {
                            expression: arguments[index],
                            allow_root_mutation: false,
                        });
                    }
                    None
                }
                MachineTask::CollectCallArgument {
                    callee,
                    arguments,
                    index,
                    values: mut arguments_so_far,
                    span,
                    expects_value,
                } => {
                    arguments_so_far.push(pop_machine_value(&mut values, span)?);
                    tasks.push(MachineTask::EvalCallArguments {
                        callee,
                        arguments,
                        index: index + 1,
                        values: arguments_so_far,
                        span,
                        expects_value,
                    });
                    None
                }
                MachineTask::StartIndirectCall {
                    arguments,
                    span,
                    expects_value,
                    value_base,
                } => {
                    let callee = pop_machine_value(&mut values, span)?;
                    if let Some(result) = self.start_indirect_call(
                        callee,
                        arguments,
                        span,
                        expects_value,
                        &mut tasks,
                        value_base,
                    )? {
                        if expects_value {
                            values.push(result);
                        }
                    }
                    None
                }
            };
            if let Some(outcome) = outcome {
                return Ok((Some(outcome), values));
            }
        }
        Ok((None, values))
    }

    fn enter_fenv_scope(&mut self, policy: Option<bool>, tasks: &mut Vec<MachineTask<'arena>>) {
        if let Some(enabled) = policy {
            tasks.push(MachineTask::RestoreFenvAccess(std::mem::replace(
                &mut self.fenv_access,
                enabled,
            )));
        }
    }

    fn exec_machine_statement(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
        values: &mut Vec<CValue>,
    ) -> Result<Option<MachineOutcome>, InterpreterError> {
        self.tick(statement.span())?;
        match statement {
            CHirStmt::Asm(assembly) => Err(InterpreterError::Asm {
                span: assembly.span,
            }),
            CHirStmt::Expr(expression, _) => {
                self.schedule_discarded_expression(expression, tasks)?;
                Ok(None)
            }
            CHirStmt::Return(Some(expression), span) => {
                if crate::expression_is_void(expression, self.unit.symbols(), self.unit.types()) {
                    tasks.push(MachineTask::FinishVoidReturn(*span));
                    self.schedule_discarded_expression(expression, tasks)?;
                } else {
                    tasks.push(MachineTask::FinishReturnValue(*span));
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
                Ok(None)
            }
            CHirStmt::Return(None, span) => {
                let end = self.pop_function_end(tasks, *span)?;
                self.finish_function(end, FunctionExit::Explicit(None), values)
            }
            CHirStmt::Compound(items, _, policy) => {
                self.enter_fenv_scope(*policy, tasks);
                let saved = self.activate_block_items(items)?;
                tasks.push(MachineTask::RestoreSlots(saved));
                if let Some(exit) = self.enter_memory_scope(statement.span())? {
                    tasks.push(MachineTask::ExitMemoryScope(exit));
                }
                tasks.push(MachineTask::CompoundContinue { items, index: 0 });
                Ok(None)
            }
            CHirStmt::If(condition, then_statement, else_statement, _) => {
                tasks.push(MachineTask::SelectIf {
                    then_statement,
                    else_statement: *else_statement,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: condition,
                    allow_root_mutation: true,
                });
                Ok(None)
            }
            CHirStmt::Switch(expression, body, span) => {
                tasks.push(MachineTask::SelectSwitch { body, span: *span });
                tasks.push(MachineTask::EvalExpression {
                    expression,
                    allow_root_mutation: true,
                });
                Ok(None)
            }
            CHirStmt::While(condition, body, span) => {
                let previous = self.fuel_anchor.replace(*span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::WhileCheck { condition, body });
                Ok(None)
            }
            CHirStmt::DoWhile(body, condition, span) => {
                let previous = self.fuel_anchor.replace(*span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::DoWhileCondition { condition, body });
                tasks.push(MachineTask::ExecStatement(body));
                Ok(None)
            }
            CHirStmt::For(initializer, condition, step, body, span) => {
                let previous = self.fuel_anchor.replace(*span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                if let CHirForInit::Declaration(declarations) = initializer {
                    let saved = self.activate_for_declarations(declarations)?;
                    tasks.push(MachineTask::RestoreSlots(saved));
                    if let Some(exit) = self.enter_memory_scope(*span)? {
                        tasks.push(MachineTask::ExitMemoryScope(exit));
                    }
                }
                tasks.push(MachineTask::ForCheck {
                    condition: *condition,
                    step: *step,
                    body,
                    span: *span,
                });
                match initializer {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(expression) => {
                        self.schedule_discarded_expression(expression, tasks)?;
                    }
                    CHirForInit::Declaration(declarations) => {
                        for declaration in declarations.iter().rev() {
                            tasks.push(MachineTask::ExecDeclaration(declaration));
                        }
                    }
                }
                Ok(None)
            }
            CHirStmt::Break(span) => {
                self.unwind_break(tasks, *span)?;
                Ok(None)
            }
            CHirStmt::Continue(span) => {
                self.unwind_continue(tasks, *span)?;
                Ok(None)
            }
            CHirStmt::Goto(name, span) => {
                self.unwind_goto(tasks, &name.spelling, *span)?;
                Ok(None)
            }
            CHirStmt::ComputedGoto(expression, span) => {
                tasks.push(MachineTask::IndirectGoto(*span));
                tasks.push(MachineTask::EvalExpression {
                    expression,
                    allow_root_mutation: true,
                });
                Ok(None)
            }
            CHirStmt::Label(_, child, _) => {
                tasks.push(MachineTask::ExecStatement(child));
                Ok(None)
            }
            CHirStmt::Case(_, child, _)
            | CHirStmt::CaseRange(_, _, child, _)
            | CHirStmt::Default(child, _) => {
                if !tasks
                    .iter()
                    .rev()
                    .take_while(|task| !matches!(task, MachineTask::FunctionEnd(_)))
                    .any(|task| matches!(task, MachineTask::SwitchEnd))
                {
                    return Err(InterpreterError::InvalidControlFlow {
                        flow: "case outside switch",
                        span: statement.span(),
                    });
                }
                tasks.push(MachineTask::ExecStatement(child));
                Ok(None)
            }
            CHirStmt::AttributedNull(_, _) | CHirStmt::Null(_) => Ok(None),
        }
    }

    fn schedule_machine_declaration(
        &mut self,
        declaration: &'arena CHirDecl<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        self.tick(declaration.span())?;
        let declaration = match declaration {
            CHirDecl::Object(declaration) => declaration,
            CHirDecl::Asm(_) | CHirDecl::Tag(_) | CHirDecl::Typedef(_) => return Ok(()),
            CHirDecl::Function(function) if function.body.is_none() => return Ok(()),
            CHirDecl::Function(declaration) => {
                return Err(InterpreterError::UnsupportedDeclaration {
                    feature: "block function declaration",
                    span: declaration.span,
                });
            }
        };
        if self.memory_mode {
            return self.schedule_memory_declaration(declaration, tasks);
        }
        let ty = self.validate_object_declaration(declaration)?;
        self.validate_initializer_plan(declaration)?;
        let slot = self.slots.get_mut(&declaration.symbol).ok_or(
            InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            },
        )?;
        if slot.ty != ty || !slot.active {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        }
        slot.value = None;
        let Some(initializer) = &declaration.initializer else {
            if self.unit.zero_initialize_automatic(declaration) {
                slot.value = Some(CInteger::new(0, ty));
            }
            return Ok(());
        };
        let action = initializer.plan().actions().first().ok_or(
            InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            },
        )?;
        match action {
            InitAction::StoreScalar {
                path,
                destination_type,
                value,
                span,
            } if path.is_root() && *destination_type == declaration.ty => {
                tasks.push(MachineTask::FinishInitializer {
                    symbol: declaration.symbol,
                    ty,
                    span: *span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: true,
                });
                Ok(())
            }
            InitAction::ZeroScalar {
                path,
                destination_type,
            } if path.is_root() && *destination_type == declaration.ty => {
                let slot = self.slots.get_mut(&declaration.symbol).ok_or(
                    InterpreterError::MalformedObjectDeclaration {
                        symbol: declaration.symbol,
                        span: declaration.span,
                    },
                )?;
                slot.value = Some(CInteger::new(0, ty));
                Ok(())
            }
            InitAction::StoreScalar { span, .. }
            | InitAction::StoreString { span, .. }
            | InitAction::CopyAggregate { span, .. } => {
                Err(InterpreterError::InvalidInitializerPlan {
                    symbol: declaration.symbol,
                    span: *span,
                })
            }
            InitAction::ZeroScalar { .. } => Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            }),
        }
    }

    fn schedule_memory_declaration(
        &mut self,
        declaration: &'arena ObjectDecl<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        self.validate_memory_object_declaration(declaration)?;
        self.validate_initializer_plan(declaration)?;
        if matches!(
            declaration.storage.base(),
            StorageClass::Static | StorageClass::Extern
        ) {
            if self.memory_binding(declaration.symbol).is_none() {
                return Err(InterpreterError::MalformedObjectDeclaration {
                    symbol: declaration.symbol,
                    span: declaration.span,
                });
            }
            return Ok(());
        }
        if self.memory_binding(declaration.symbol).is_none() {
            self.allocate_automatic_object(
                declaration.symbol,
                declaration.ty,
                declaration.initializer.as_ref(),
                declaration.span,
            )?;
        }
        if let Some(cleanup) = declaration.cleanup {
            let frame = self
                .memory_frame
                .as_mut()
                .expect("automatic object has a frame");
            frame.cleanups.push((frame.current_scope, cleanup));
        }
        let Some(initializer) = &declaration.initializer else {
            if self.unit.zero_initialize_automatic(declaration) {
                let binding = self.require_memory_binding(declaration.symbol, declaration.span)?;
                let layout = self.object_access_layout(declaration.ty, None, declaration.span)?;
                if layout.size_bytes != 0 {
                    let bytes = ByteSequence::try_from_bytes(vec![0; layout.size_bytes as usize])
                        .map_err(|error| self.memory_error(error, declaration.span))?;
                    self.memory = self
                        .memory
                        .store_bytes(binding.address, layout, &bytes)
                        .map_err(|error| self.memory_error(error, declaration.span))?;
                }
                self.zero_initialize_subobjects(
                    binding.address,
                    declaration.ty,
                    SubobjectPath::root(),
                    &[],
                    declaration.span,
                )?;
            }
            return Ok(());
        };
        if self.unit.zero_initialize_automatic_padding(declaration) {
            let binding = self.require_memory_binding(declaration.symbol, declaration.span)?;
            self.zero_constructor_padding(binding, initializer, declaration.span)?;
        }
        if initializer.plan().zero_fills_unwritten() {
            let binding = self.require_memory_binding(declaration.symbol, declaration.span)?;
            self.zero_initialize_subobjects(
                binding.address,
                declaration.ty,
                SubobjectPath::root(),
                initializer.plan().actions(),
                declaration.span,
            )?;
        }
        if initializer.plan().actions().is_empty() && !initializer.plan().zero_fills_unwritten() {
            return Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            });
        }
        let binding = self.require_memory_binding(declaration.symbol, declaration.span)?;
        tasks.push(MachineTask::ContinueMemoryInitializer {
            target: InitializerTarget {
                binding,
                initializer,
                symbol: Some(declaration.symbol),
                span: declaration.span,
            },
            index: 0,
        });
        Ok(())
    }

    /// Resolve a designated subobject to a byte offset within its object.
    ///
    /// Field offsets come from the target record layout and element offsets
    /// from the element size, so this is the same address arithmetic a
    /// projected place performs, driven by constant steps instead of by
    /// evaluated subscripts.
    fn subobject_offset(
        &self,
        object_ty: TypeId,
        path: &SubobjectPath,
        span: SourceSpan,
    ) -> Result<(i64, TypeId, Option<BitFieldLayout>), InterpreterError> {
        let mut current = object_ty;
        let mut bit_field = None;
        let mut offset: i64 = 0;
        for step in path.steps() {
            match step {
                SubobjectStep::Field { field_index } => {
                    let tag = match self.unit.types().get(current).map(|ty| &ty.kind) {
                        Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => *tag,
                        _ => return Err(InterpreterError::UnsupportedType { ty: current, span }),
                    };
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), tag)
                        .map_err(|_| InterpreterError::UnsupportedType { ty: current, span })?;
                    let field = layout
                        .fields
                        .iter()
                        .find(|field| field.field_index == *field_index)
                        .ok_or(InterpreterError::UnsupportedType { ty: current, span })?;
                    bit_field = field.bit_field;
                    offset =
                        offset
                            .checked_add(i64::try_from(field.byte_offset).map_err(|_| {
                                InterpreterError::UnsupportedType { ty: current, span }
                            })?)
                            .ok_or(InterpreterError::UnsupportedType { ty: current, span })?;
                    current = self
                        .unit
                        .types()
                        .tag(tag)
                        .and_then(|definition| match &definition.state {
                            TagState::Complete(
                                TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                            ) => fields.get(*field_index).map(|field| field.ty),
                            _ => None,
                        })
                        .ok_or(InterpreterError::UnsupportedType { ty: current, span })?;
                }
                SubobjectStep::Index { index } => {
                    let element = match self.unit.types().get(current).map(|ty| &ty.kind) {
                        Some(
                            TypeKind::Array { element, .. } | TypeKind::Vector { element, .. },
                        ) => *element,
                        _ => return Err(InterpreterError::UnsupportedType { ty: current, span }),
                    };
                    let size = i64::try_from(self.access_layout(element, span)?.size_bytes)
                        .map_err(|_| InterpreterError::UnsupportedType { ty: element, span })?;
                    let step = i64::try_from(*index)
                        .ok()
                        .and_then(|index| index.checked_mul(size))
                        .ok_or(InterpreterError::UnsupportedType { ty: element, span })?;
                    offset = offset
                        .checked_add(step)
                        .ok_or(InterpreterError::UnsupportedType { ty: element, span })?;
                    current = element;
                }
            }
        }
        Ok((offset, current, bit_field))
    }

    /// Address of one designated subobject of an already-bound object.
    fn subobject_address(
        &self,
        binding: MemoryBinding,
        path: &SubobjectPath,
        span: SourceSpan,
    ) -> Result<(Pointer, TypeId, Option<BitFieldLayout>), InterpreterError> {
        let (offset, ty, bit_field) = self.subobject_offset(binding.ty, path, span)?;
        let address = binding
            .address
            .checked_offset(offset)
            .map_err(|error| self.memory_error(error, span))?;
        Ok((address, ty, bit_field))
    }

    /// Schedule the next action of an initializer plan, if any remain.
    fn continue_memory_initializer(
        &mut self,
        target: InitializerTarget<'arena>,
        index: usize,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let initializer = target.initializer;
        let Some(action) = initializer.plan().actions().get(index) else {
            return Ok(());
        };
        if let InitAction::StoreScalar {
            path,
            destination_type,
            value,
            span,
        } = action
        {
            if let Some(bits) = crate::constant::floating_constant_representation(
                value,
                *destination_type,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            )
            .filter(|_| !self.unit.runtime_floating_environment())
            {
                let (address, actual_ty, bit_field) =
                    self.subobject_address(target.binding, path, *span)?;
                if actual_ty != *destination_type || bit_field.is_some() {
                    return Err(invalid_initializer_target(target, *span));
                }
                let volatile = self
                    .unit
                    .types()
                    .get(actual_ty)
                    .is_some_and(|ty| ty.qualifiers.is_volatile);
                self.store_constant_representation(address, actual_ty, bits, volatile, *span)?;
                tasks.push(MachineTask::ContinueMemoryInitializer {
                    target,
                    index: index + 1,
                });
                return Ok(());
            }
        }
        match action {
            InitAction::StoreScalar {
                path, span, value, ..
            }
            | InitAction::CopyAggregate {
                path, span, value, ..
            } => {
                // Reject before evaluating, so a rejected destination cannot
                // run the initializer's side effects first.
                self.subobject_address(target.binding, path, *span)?;
                tasks.push(MachineTask::ContinueMemoryInitializer {
                    target,
                    index: index + 1,
                });
                tasks.push(MachineTask::FinishMemorySubobject { target, index });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: true,
                });
                Ok(())
            }
            InitAction::ZeroScalar { path, .. } => {
                let span = initializer.span();
                let (address, ty, bit_field) =
                    self.subobject_address(target.binding, path, span)?;
                if matches!(
                    self.unit.types().get(ty).map(|ty| &ty.kind),
                    Some(TypeKind::Float(
                        evocc_types::FloatKind::F32
                            | evocc_types::FloatKind::F64
                            | evocc_types::FloatKind::F128
                    ))
                ) {
                    self.store_constant_representation(
                        address,
                        ty,
                        CInteger::new(0, CIntegerType::ULONG),
                        false,
                        span,
                    )?;
                    tasks.push(MachineTask::ContinueMemoryInitializer {
                        target,
                        index: index + 1,
                    });
                    return Ok(());
                }
                let value = self.zero_value(ty, span)?;
                if let Some(field) = bit_field {
                    self.store_bit_field(address, ty, field, value, false, span)?;
                } else {
                    self.store_initialized_value(address, ty, value, span)?;
                }
                tasks.push(MachineTask::ContinueMemoryInitializer {
                    target,
                    index: index + 1,
                });
                Ok(())
            }
            InitAction::StoreString {
                path,
                destination_type,
                literal,
                span,
            } => {
                let (address, actual_type, _) =
                    self.subobject_address(target.binding, path, *span)?;
                if actual_type != *destination_type {
                    return Err(invalid_initializer_target(target, *span));
                }
                let (layout, value) = self
                    .string_initializer_bytes(*destination_type, literal, *span)
                    .map_err(|_| invalid_initializer_target(target, *span))?;
                self.store_initializer_bytes(address, layout, &value, *span)?;
                tasks.push(MachineTask::ContinueMemoryInitializer {
                    target,
                    index: index + 1,
                });
                Ok(())
            }
        }
    }

    fn unreachable_initializer_shapes(
        &mut self,
        declaration: &'arena ObjectDecl<'arena>,
        initializer: &crate::ObjectInitializer<'arena>,
    ) -> Result<(), InterpreterError> {
        let action = initializer.plan().actions().first().ok_or(
            InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            },
        )?;
        match action {
            InitAction::StoreScalar { span, .. }
            | InitAction::StoreString { span, .. }
            | InitAction::CopyAggregate { span, .. } => {
                Err(InterpreterError::InvalidInitializerPlan {
                    symbol: declaration.symbol,
                    span: *span,
                })
            }
            InitAction::ZeroScalar { .. } => Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            }),
        }
    }

    fn schedule_switch(
        &mut self,
        controlling: CInteger,
        body: &'arena CHirStmt<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let controlling = controlling.promoted();
        let mut labels = Vec::new();
        collect_switch_labels(body, &mut labels);
        let mut selected = None;
        let mut default = None;
        for statement in labels {
            let matches = match statement {
                CHirStmt::Case(value, _, case_span) => {
                    let value = self
                        .eval_static_integer_expression(value)?
                        .cast(controlling.ty());
                    eval_binary_integer(BinaryOp::Eq, controlling, value)
                        .map_err(|error| self.integer_error(error, *case_span))?
                        .truth()
                }
                CHirStmt::CaseRange(first, last, _, _) => {
                    let first = self
                        .eval_static_integer_expression(first)?
                        .cast(controlling.ty());
                    let last = self
                        .eval_static_integer_expression(last)?
                        .cast(controlling.ty());
                    eval_binary_integer(BinaryOp::Le, first, controlling)
                        .map_err(|error| self.integer_error(error, statement.span()))?
                        .truth()
                        && eval_binary_integer(BinaryOp::Le, controlling, last)
                            .map_err(|error| self.integer_error(error, statement.span()))?
                            .truth()
                }
                CHirStmt::Default(_, _) => {
                    default = Some(statement);
                    false
                }
                _ => unreachable!("only labels of this switch are collected"),
            };
            if matches {
                selected = Some(statement);
                break;
            }
        }
        if let Some(statement) = selected.or(default) {
            // Dispatch is a jump to the label, not execution from the body's
            // first statement. Reuse goto's scope and loop continuations.
            tasks.push(MachineTask::SwitchEnd);
            self.schedule_goto_in_statement(
                body,
                StatementDestination::Case(statement as *const CHirStmt<'_> as usize),
                tasks,
            )?;
        }
        Ok(())
    }

    fn pop_function_end(
        &mut self,
        tasks: &mut Vec<MachineTask<'arena>>,
        span: SourceSpan,
    ) -> Result<FunctionEnd<'arena>, InterpreterError> {
        while let Some(task) = tasks.pop() {
            match task {
                MachineTask::FunctionEnd(end) => return Ok(end),
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                // Return copies an aggregate while its source is still alive.
                // finish_function ends the entire frame after that copy, so
                // individual lexical exits must not destroy it first.
                MachineTask::ExitMemoryScope(_) => {}
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
                MachineTask::RestoreFenvAccess(previous) => self.fenv_access = previous,
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                }
                _ => {}
            }
        }
        Err(InterpreterError::UnsupportedStatement {
            feature: "malformed interpreter call continuation",
            span,
        })
    }

    fn unwind_break(
        &mut self,
        tasks: &mut Vec<MachineTask<'arena>>,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        while let Some(task) = tasks.pop() {
            match task {
                MachineTask::WhileCheck { .. }
                | MachineTask::DoWhileCondition { .. }
                | MachineTask::ForStep { .. }
                | MachineTask::SwitchEnd => return Ok(()),
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                MachineTask::ExitMemoryScope(exit) => self.exit_memory_scope(exit)?,
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
                MachineTask::RestoreFenvAccess(previous) => self.fenv_access = previous,
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                }
                MachineTask::FunctionEnd(end) => {
                    tasks.push(MachineTask::FunctionEnd(end));
                    break;
                }
                _ => {}
            }
        }
        Err(InterpreterError::InvalidControlFlow {
            flow: "break outside loop or switch",
            span,
        })
    }

    fn unwind_continue(
        &mut self,
        tasks: &mut Vec<MachineTask<'arena>>,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        while let Some(task) = tasks.pop() {
            match task {
                task @ (MachineTask::WhileCheck { .. }
                | MachineTask::DoWhileCondition { .. }
                | MachineTask::ForStep { .. }) => {
                    tasks.push(task);
                    return Ok(());
                }
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                MachineTask::ExitMemoryScope(exit) => self.exit_memory_scope(exit)?,
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
                MachineTask::RestoreFenvAccess(previous) => self.fenv_access = previous,
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                }
                MachineTask::FunctionEnd(end) => {
                    tasks.push(MachineTask::FunctionEnd(end));
                    break;
                }
                _ => {}
            }
        }
        Err(InterpreterError::InvalidControlFlow {
            flow: "continue outside loop",
            span,
        })
    }

    fn unwind_goto(
        &mut self,
        tasks: &mut Vec<MachineTask<'arena>>,
        spelling: &str,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let destination = StatementDestination::Label(spelling);
        while let Some(task) = tasks.pop() {
            match task {
                MachineTask::StatementExpressionContinue {
                    items, value, span, ..
                } => {
                    if self.schedule_goto_in_active_items(
                        items,
                        destination,
                        MachineBlockMode::StatementExpression { value, span },
                        tasks,
                    )? {
                        return Ok(());
                    }
                }
                MachineTask::CompoundContinue { items, .. } => {
                    if self.schedule_goto_in_active_items(
                        items,
                        destination,
                        MachineBlockMode::Compound,
                        tasks,
                    )? {
                        return Ok(());
                    }
                }
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                MachineTask::ExitMemoryScope(exit) => self.exit_memory_scope(exit)?,
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
                MachineTask::RestoreFenvAccess(previous) => self.fenv_access = previous,
                MachineTask::RestoreNestedMutationDepth(previous) => {
                    self.nested_mutation_depth = previous;
                }
                MachineTask::FunctionEnd(end) => {
                    let body = self.function_definition(end.symbol)?.body.ok_or(
                        InterpreterError::FunctionHasNoBody {
                            symbol: end.symbol,
                            span: end.body_span,
                        },
                    )?;
                    tasks.push(MachineTask::FunctionEnd(end));
                    if self.schedule_goto_in_statement(body, destination, tasks)? {
                        return Ok(());
                    }
                    break;
                }
                _ => {}
            }
        }
        Err(InterpreterError::UnknownLabel {
            spelling: spelling.to_owned(),
            span,
        })
    }

    fn schedule_goto_in_active_items(
        &mut self,
        items: &'arena [CHirBlockItem<'arena>],
        destination: StatementDestination<'_>,
        mode: MachineBlockMode<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<bool, InterpreterError> {
        for (index, item) in items.iter().enumerate() {
            let CHirBlockItem::Statement(statement) = item else {
                continue;
            };
            if !statement_contains_destination(statement, destination) {
                continue;
            }
            if self.memory_mode {
                // A backward jump before a VLA declaration leaves that
                // object's declaration scope even within the same compound.
                // Fixed automatic objects keep their scope-wide identity.
                for skipped in items[index..].iter().rev() {
                    let CHirBlockItem::Declaration(CHirDecl::Object(object)) = skipped else {
                        continue;
                    };
                    if self
                        .target
                        .array_size_factors(self.unit.types(), object.ty)
                        .is_ok_and(|(_, captures)| !captures.is_empty())
                    {
                        if let Some(binding) = self.memory_binding(object.symbol) {
                            let frame = self.memory_frame.as_mut().expect("active memory scope");
                            frame.bindings.remove(&object.symbol);
                            let id = frame.id;
                            self.memory = self
                                .memory
                                .end_object_lifetime(id, binding.address)
                                .map_err(|error| self.memory_error(error, object.span))?;
                        }
                    }
                }
                // A jump can enter a block after automatic declarations.
                // Their storage exists, but their initializers did not run.
                // Existing bindings belong to this active scope and must not
                // be replaced on a backward jump.
                for skipped in &items[..index] {
                    let CHirBlockItem::Declaration(CHirDecl::Object(object)) = skipped else {
                        continue;
                    };
                    if matches!(
                        object.storage,
                        StorageClass::None | StorageClass::Auto | StorageClass::Register
                    ) && self.memory_binding(object.symbol).is_none()
                    {
                        self.validate_memory_object_declaration(object)?;
                        self.allocate_automatic_object(
                            object.symbol,
                            object.ty,
                            object.initializer.as_ref(),
                            object.span,
                        )?;
                    }
                }
            }
            if let MachineBlockMode::StatementExpression { value, span } = mode {
                // Re-enter a directly labelled item through the expression
                // continuation so a labelled final expression still yields
                // its value rather than being evaluated and discarded.
                if destination.matches(statement) {
                    tasks.push(MachineTask::StatementExpressionContinue {
                        items,
                        index,
                        value,
                        span,
                    });
                    return Ok(true);
                }
            }
            tasks.push(match mode {
                MachineBlockMode::Compound => MachineTask::CompoundContinue {
                    items,
                    index: index + 1,
                },
                MachineBlockMode::StatementExpression { value, span } => {
                    MachineTask::StatementExpressionContinue {
                        items,
                        index: index + 1,
                        value,
                        span,
                    }
                }
            });
            return self.schedule_goto_in_statement(statement, destination, tasks);
        }
        Ok(false)
    }

    fn schedule_goto_in_statement(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
        destination: StatementDestination<'_>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<bool, InterpreterError> {
        if !statement_contains_destination(statement, destination) {
            return Ok(false);
        }
        if destination.matches(statement) {
            tasks.push(MachineTask::ExecStatement(statement));
            return Ok(true);
        }
        match statement {
            CHirStmt::Label(_, child, _) => {
                self.schedule_goto_in_statement(child, destination, tasks)
            }
            CHirStmt::Compound(items, _, policy) => {
                self.enter_fenv_scope(*policy, tasks);
                let saved = self.activate_block_items(items)?;
                tasks.push(MachineTask::RestoreSlots(saved));
                if let Some(exit) = self.enter_memory_scope(statement.span())? {
                    tasks.push(MachineTask::ExitMemoryScope(exit));
                }
                self.schedule_goto_in_active_items(
                    items,
                    destination,
                    MachineBlockMode::Compound,
                    tasks,
                )
            }
            CHirStmt::If(_, then_statement, else_statement, _) => {
                if statement_contains_destination(then_statement, destination) {
                    self.schedule_goto_in_statement(then_statement, destination, tasks)
                } else if let Some(else_statement) = else_statement {
                    self.schedule_goto_in_statement(else_statement, destination, tasks)
                } else {
                    Ok(false)
                }
            }
            CHirStmt::Switch(_, body, _) => {
                tasks.push(MachineTask::SwitchEnd);
                self.schedule_goto_in_statement(body, destination, tasks)
            }
            CHirStmt::While(condition, body, loop_span) => {
                let previous = self.fuel_anchor.replace(*loop_span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::WhileCheck { condition, body });
                self.schedule_goto_in_statement(body, destination, tasks)
            }
            CHirStmt::DoWhile(body, condition, loop_span) => {
                let previous = self.fuel_anchor.replace(*loop_span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::DoWhileCondition { condition, body });
                self.schedule_goto_in_statement(body, destination, tasks)
            }
            CHirStmt::For(initializer, condition, step, body, loop_span) => {
                let previous = self.fuel_anchor.replace(*loop_span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                if let CHirForInit::Declaration(declarations) = initializer {
                    let saved = self.activate_for_declarations(declarations)?;
                    tasks.push(MachineTask::RestoreSlots(saved));
                    if let Some(exit) = self.enter_memory_scope(*loop_span)? {
                        tasks.push(MachineTask::ExitMemoryScope(exit));
                    }
                }
                tasks.push(MachineTask::ForStep {
                    condition: *condition,
                    step: *step,
                    body,
                    span: *loop_span,
                });
                self.schedule_goto_in_statement(body, destination, tasks)
            }
            CHirStmt::Case(_, child, _)
            | CHirStmt::CaseRange(_, _, child, _)
            | CHirStmt::Default(child, _) => {
                self.schedule_goto_in_statement(child, destination, tasks)
            }
            CHirStmt::Expr(_, _)
            | CHirStmt::Asm(_)
            | CHirStmt::Return(_, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::ComputedGoto(_, _)
            | CHirStmt::AttributedNull(_, _)
            | CHirStmt::Null(_) => Ok(false),
        }
    }

    fn call_designator_returns_void(&self, callee: &CHirExpr<'arena>) -> bool {
        let Some(function_type) = self.call_designator_function_type(callee) else {
            return false;
        };
        let Some(TypeKind::Function { return_type, .. }) =
            self.unit.types().get(function_type).map(|ty| &ty.kind)
        else {
            return false;
        };
        matches!(
            self.unit.types().get(*return_type).map(|ty| &ty.kind),
            Some(TypeKind::Void)
        )
    }

    fn call_designator_function_type(&self, callee: &CHirExpr<'arena>) -> Option<TypeId> {
        let candidate =
            PointerTypeDesc::from_expression(self.unit.symbols(), self.unit.types(), callee)
                .ok()?
                .pointee();
        match self.unit.types().get(candidate).map(|ty| &ty.kind) {
            Some(TypeKind::Function { .. }) => Some(candidate),
            _ => None,
        }
    }

    fn schedule_discarded_expression(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        if self.memory_mode {
            if let Some((place, bits, span)) = crate::constant::floating_representation_store(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            )
            .filter(|_| !self.unit.runtime_floating_environment())
            {
                tasks.push(MachineTask::BeginConstantRepresentationStore { place, bits, span });
                return Ok(());
            }
        }
        if let CHirExpr::Comma(left, right, _) = expression {
            self.schedule_discarded_expression(right, tasks)?;
            self.schedule_discarded_expression(left, tasks)?;
            return Ok(());
        }
        if let CHirExpr::Ternary(condition, then_expression, else_expression, _) = expression {
            tasks.push(MachineTask::DiscardSelectedExpression {
                then_expression,
                else_expression,
            });
            tasks.push(MachineTask::EvalExpression {
                expression: condition,
                allow_root_mutation: true,
            });
            return Ok(());
        }
        if let CHirExpr::Cast(ty, operand, _) = expression {
            if self
                .unit
                .types()
                .get(*ty)
                .is_some_and(|ty| matches!(ty.kind, TypeKind::Void))
            {
                return self.schedule_discarded_expression(operand, tasks);
            }
        }
        if let CHirExpr::BuiltinVarArgs { op, .. } = expression {
            if *op != crate::VarArgsBuiltinOp::Arg {
                self.schedule_varargs(expression, tasks)?;
                return Ok(());
            }
        }
        if let CHirExpr::Call(callee, arguments, call_span) = expression {
            if self.call_designator_returns_void(callee) {
                tasks.push(MachineTask::EvalCallArguments {
                    callee,
                    arguments,
                    index: 0,
                    values: Vec::new(),
                    span: *call_span,
                    expects_value: false,
                });
                return Ok(());
            }
        }
        if let CHirExpr::StatementExpr {
            body, value, span, ..
        } = expression
        {
            if crate::expression_is_void(expression, self.unit.symbols(), self.unit.types()) {
                self.schedule_statement_expression(body, *value, *span, tasks)?;
                return Ok(());
            }
        }
        tasks.push(MachineTask::DiscardExpression(expression.span()));
        tasks.push(MachineTask::EvalExpression {
            expression,
            allow_root_mutation: true,
        });
        Ok(())
    }

    fn schedule_statement_expression(
        &mut self,
        body: &'arena CHirStmt<'arena>,
        value: Option<&'arena CHirExpr<'arena>>,
        span: SourceSpan,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let CHirStmt::Compound(items, _, policy) = body else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "statement expression body is not compound",
                span,
            });
        };
        self.enter_fenv_scope(*policy, tasks);
        let result_matches = match (value, items.last()) {
            (None, _) => true,
            (Some(value), Some(CHirBlockItem::Statement(statement))) => {
                statement_expression_value(statement)
                    .is_some_and(|expression| std::ptr::eq(value, expression))
            }
            (Some(_), _) => false,
        };
        if !result_matches {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "statement expression result is not its final statement",
                span,
            });
        }
        let saved = self.activate_block_items(items)?;
        tasks.push(MachineTask::RestoreSlots(saved));
        if let Some(exit) = self.enter_memory_scope(body.span())? {
            let parent = exit.parent;
            tasks.push(MachineTask::ExitMemoryScope(exit));
            if value.is_some_and(|value| {
                !crate::expression_is_void(value, self.unit.symbols(), self.unit.types())
            }) {
                tasks.push(MachineTask::PreserveStatementExpressionValue { parent, span });
            }
        }
        tasks.push(MachineTask::StatementExpressionContinue {
            items,
            index: 0,
            value,
            span,
        });
        Ok(())
    }

    fn schedule_varargs(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let CHirExpr::BuiltinVarArgs {
            op,
            first,
            second,
            requested_ty,
            span,
            ..
        } = expression
        else {
            return Err(InterpreterError::VarArgs {
                span: expression.span(),
            });
        };
        tasks.push(MachineTask::ApplyVarArgs {
            op: *op,
            requested_ty: *requested_ty,
            span: *span,
        });
        if *op == crate::VarArgsBuiltinOp::Copy {
            let source = second.ok_or(InterpreterError::VarArgs { span: *span })?;
            tasks.push(MachineTask::EvalExpression {
                expression: source,
                allow_root_mutation: true,
            });
        }
        tasks.push(MachineTask::EvalExpression {
            expression: first,
            allow_root_mutation: true,
        });
        Ok(())
    }

    fn apply_varargs(
        &mut self,
        op: crate::VarArgsBuiltinOp,
        requested_ty: Option<TypeId>,
        span: SourceSpan,
        values: &mut Vec<CValue>,
    ) -> Result<(), InterpreterError> {
        use crate::VarArgsBuiltinOp;
        let fault = |error| InterpreterError::Memory { error, span };
        let address = |value| match value {
            CValue::Aggregate { address, .. } => Ok(address),
            _ => Err(fault(MemoryFault::InvalidArgumentList)),
        };
        let source = if op == VarArgsBuiltinOp::Copy {
            Some(address(pop_machine_value(values, span)?)?)
        } else {
            None
        };
        let list = address(pop_machine_value(values, span)?)?;
        if let Some(source) = source {
            self.memory = self
                .memory
                .copy_bytes(
                    list,
                    source,
                    AccessLayout {
                        size_bytes: 32,
                        align_bytes: 8,
                    },
                )
                .map_err(fault)?;
            return Ok(());
        }
        let identity = match op {
            VarArgsBuiltinOp::Start => {
                let arguments = self
                    .memory_frame
                    .as_ref()
                    .ok_or_else(|| fault(MemoryFault::InvalidArgumentList))?
                    .anonymous
                    .clone();
                Some(self.argument_lists.start(arguments))
            }
            VarArgsBuiltinOp::End => None,
            VarArgsBuiltinOp::Arg => {
                let target = self.value_type(
                    requested_ty.ok_or(InterpreterError::VarArgs { span })?,
                    span,
                )?;
                if matches!(target, ValueType::Aggregate(ty) if !crate::is_integer_variadic_record(self.unit.types(), ty))
                {
                    return Err(InterpreterError::VarArgs { span });
                }
                let identity = self.memory.argument_list(list).map_err(fault)?;
                let value = *self.argument_lists.read(identity).map_err(fault)?;
                let matches = match (value, target) {
                    (CValue::Integer(value), ValueType::Integer(ty)) => {
                        value.ty().bits() == ty.bits()
                    }
                    (CValue::Floating(value), ValueType::Floating(kind)) => value.kind() == kind,
                    (CValue::Pointer { .. }, ValueType::Pointer(ty)) => !matches!(
                        self.unit.types().get(ty.pointee()).map(|ty| &ty.kind),
                        Some(TypeKind::Function { .. })
                    ),
                    (CValue::FunctionPointer { .. }, ValueType::Pointer(ty)) => matches!(
                        self.unit.types().get(ty.pointee()).map(|ty| &ty.kind),
                        Some(TypeKind::Function { .. })
                    ),
                    (CValue::Aggregate { ty, .. }, ValueType::Aggregate(expected)) => {
                        self.same_aggregate_type(ty, expected)
                    }
                    _ => false,
                };
                if !matches {
                    return Err(fault(MemoryFault::InvalidArgumentList));
                }
                values.push(self.cast_runtime_value(value, target, span)?);
                Some(self.argument_lists.advance(identity).map_err(fault)?)
            }
            VarArgsBuiltinOp::Copy => unreachable!("copy returned above"),
        };
        self.memory = self
            .memory
            .with_argument_list(list, identity)
            .map_err(fault)?;
        Ok(())
    }

    fn schedule_expression(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        allow_root_mutation: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
        values: &mut Vec<CValue>,
    ) -> Result<(), InterpreterError> {
        if !self.unit.runtime_floating_environment() || self.static_integer_evaluation {
            if let Some(value) = crate::constant::floating_constant_cast(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            ) {
                values.push(CValue::Floating(value));
                return Ok(());
            }
            if let Some(value) = crate::constant::fold_floating_integer_arithmetic(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            ) {
                values.push(CValue::Integer(value));
                return Ok(());
            }
        }
        if !self.static_integer_evaluation
            && !self.memory_mode
            // Composite scalar expressions must use the same explicit task
            // machine as call-bearing/object expressions. The old no-call
            // shortcut recursively evaluated arbitrarily deep arithmetic.
            && matches!(
                expression,
                CHirExpr::Lit(..)
                    | CHirExpr::CharLit(..)
                    | CHirExpr::Ident { .. }
                    | CHirExpr::SizeOf(..)
                    | CHirExpr::SizeOfExpr(..)
                    | CHirExpr::AlignOf(..)
                    | CHirExpr::BuiltinTrap { .. }
                    | CHirExpr::BuiltinUnreachable { .. }
            )
        {
            values.push(CValue::Integer(
                self.eval_expr(expression, allow_root_mutation)?,
            ));
            return Ok(());
        }

        let span = expression.span();
        self.tick(span)?;
        if !self.static_integer_evaluation {
            if let Some(value) = crate::constant::fold_wrapping_integer_arithmetic(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            ) {
                values.push(CValue::Integer(value));
                return Ok(());
            }
        }
        if self.static_integer_evaluation {
            if matches!(expression, CHirExpr::Unary(UnaryOp::AddrOf, ..)) {
                if let Some(value) = self.numeric_static_address(expression)? {
                    values.push(value);
                    return Ok(());
                }
            }
            // Reusing the execution machine must not admit calls, reads of
            // objects, or mutations into constant initialization. Inspect only
            // the evaluated path: sizeof and unselected arms stay unevaluated.
            let admitted = match expression {
                CHirExpr::Lit(..)
                | CHirExpr::CharLit(..)
                | CHirExpr::Unary(
                    UnaryOp::Plus | UnaryOp::Neg | UnaryOp::Abs | UnaryOp::Not | UnaryOp::BitNot,
                    ..,
                )
                | CHirExpr::EvaluateOnce(..)
                | CHirExpr::Binary(..)
                | CHirExpr::Ternary(..)
                | CHirExpr::Elvis(..)
                | CHirExpr::BuiltinConstantP { .. }
                | CHirExpr::BuiltinObjectSize { .. }
                | CHirExpr::BitBuiltin { .. }
                | CHirExpr::SizeOf(..)
                | CHirExpr::SizeOfExpr(..)
                | CHirExpr::AlignOf(..)
                | CHirExpr::Comma(..) => true,
                CHirExpr::Ident { symbol, .. } => self
                    .unit
                    .symbols()
                    .symbol(*symbol)
                    .is_ok_and(|symbol| symbol.enumerator_value().is_some()),
                // Numeric pointer round trips use the same iterative casts
                // and fuel as other constants. The operand whitelist still
                // excludes object reads, symbolic addresses and calls.
                CHirExpr::Cast(ty, ..) => matches!(
                    self.value_type(*ty, span),
                    Ok(ValueType::Integer(_) | ValueType::Pointer(_))
                ),
                _ => false,
            };
            if !admitted {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "nonconstant expression in static integer initializer",
                    span,
                });
            }
        }
        match expression {
            CHirExpr::Atomic(access) => {
                tasks.push(MachineTask::ApplyAtomic(access));
                for expression in access.operands().rev() {
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
            }
            CHirExpr::Floating(access) => {
                tasks.push(MachineTask::ApplyFloating(access));
                for &expression in access.operands.iter().rev() {
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
            }
            CHirExpr::Vector(access) => {
                tasks.push(MachineTask::ApplyVector(access));
                for &expression in access.operands.iter().rev() {
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
            }
            CHirExpr::Complex(access) => {
                tasks.push(MachineTask::ApplyComplex(access));
                for &expression in access.operands.iter().rev() {
                    tasks.push(MachineTask::EvalExpression {
                        expression,
                        allow_root_mutation: true,
                    });
                }
            }
            CHirExpr::Lit(literal, _) => values.push(CValue::Integer(
                integer_literal_value(*literal).map_err(|error| self.integer_error(error, span))?,
            )),
            CHirExpr::CharLit(literal, _) => values.push(CValue::Integer(literal.integer_value())),
            CHirExpr::BuiltinConstantP { operand, .. } => values.push(CValue::Integer(
                CInteger::int(i32::from(crate::builtin_constant_p(
                    operand,
                    self.unit.types(),
                    self.unit.symbols(),
                    &self.target,
                ))),
            )),
            CHirExpr::BuiltinObjectSize { .. } => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "unresolved object extent query",
                    span,
                })
            }
            CHirExpr::Ident {
                symbol,
                spelling,
                facts,
                span,
            } => values.push(self.load_identifier(*symbol, spelling, *facts, *span)?),
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) if self.memory_mode => {
                if matches!(operand, CHirExpr::CompoundLit(_, _, _)) {
                    self.schedule_compound_literal(operand, true, tasks)?;
                } else if let CHirExpr::MemoryRead {
                    place, target_span, ..
                } = operand
                {
                    place
                        .validate_against(self.unit.symbols(), self.unit.types())
                        .map_err(|_| InterpreterError::UnsupportedExpression {
                            feature: "invalid addressed memory place",
                            span,
                        })?;
                    // A function designator is the pointer itself, not an
                    // object address on which to perform a memory access.
                    if !matches!(
                        self.unit.types().get(place.value_ty()).map(|ty| &ty.kind),
                        Some(TypeKind::Function { .. })
                    ) {
                        tasks.push(MachineTask::FoldPlaceAddress {
                            place,
                            span: *target_span,
                        });
                    }
                    self.schedule_place_operands(place, *target_span, tasks, values)?;
                } else if let CHirExpr::Unary(UnaryOp::Deref, pointer, _) = operand {
                    tasks.push(MachineTask::EvalExpression {
                        expression: pointer,
                        allow_root_mutation: true,
                    });
                } else {
                    values.push(self.address_of_expression(operand, span)?);
                }
            }
            CHirExpr::Unary(UnaryOp::Deref, operand, _) if self.memory_mode => {
                let pointer_type = self.pointer_type_of_expression(operand)?;
                if !matches!(
                    self.unit
                        .types()
                        .get(pointer_type.pointee())
                        .map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) {
                    tasks.push(MachineTask::LoadDereference { pointer_type, span });
                }
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    // A postfix or prefix update is part of the pointer
                    // expression in `*p++` / `*++p`; the dereference must not
                    // turn that legitimate single mutation into a boundary.
                    allow_root_mutation: true,
                });
            }
            CHirExpr::Unary(operator, operand, _) => {
                tasks.push(MachineTask::ApplyUnary {
                    operator: *operator,
                    expression_span: span,
                    operand_span: operand.span(),
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    allow_root_mutation: false,
                });
            }
            CHirExpr::EvaluateOnce(operand, _) => {
                let previous = self.nested_mutation_depth;
                self.nested_mutation_depth = previous.saturating_add(1);
                tasks.push(MachineTask::RestoreNestedMutationDepth(previous));
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::BuiltinExpect(value, expected, _) => {
                // The saved first value stays below the second on the value
                // stack. A nonreturning second operand never yields a result.
                tasks.push(MachineTask::DiscardExpression(expected.span()));
                tasks.push(MachineTask::EvalExpression {
                    expression: expected,
                    allow_root_mutation: true,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::Comma(left, right, _) | CHirExpr::Binary(BinaryOp::Comma, left, right, _) => {
                if self.static_integer_evaluation {
                    tasks.push(MachineTask::ContinueBinary {
                        operator: BinaryOp::Comma,
                        right,
                        span,
                        allow_root_mutation: false,
                    });
                    tasks.push(MachineTask::EvalExpression {
                        expression: left,
                        allow_root_mutation: false,
                    });
                } else {
                    tasks.push(MachineTask::EvalExpression {
                        expression: right,
                        allow_root_mutation,
                    });
                    self.schedule_discarded_expression(left, tasks)?;
                }
            }
            CHirExpr::Binary(operator, left, right, _) => {
                tasks.push(MachineTask::ContinueBinary {
                    operator: *operator,
                    right,
                    span,
                    allow_root_mutation,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: left,
                    allow_root_mutation: matches!(operator, BinaryOp::And | BinaryOp::Or)
                        || (*operator == BinaryOp::Comma && allow_root_mutation),
                });
            }
            CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
                let result_type = self.expression_value_type(expression)?;
                tasks.push(MachineTask::SelectTernary {
                    then_expression,
                    else_expression,
                    result_type,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: condition,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::Elvis(operand, else_expression, _) => {
                let result_type = self.expression_value_type(expression)?;
                tasks.push(MachineTask::SelectElvis {
                    else_expression,
                    result_type,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    // GNU `a ?: b` evaluates `a` as a full expression.  In
                    // particular, an update such as `value++` is permitted
                    // here and its pre-update value is the true result.
                    allow_root_mutation: true,
                });
            }
            CHirExpr::Call(callee, arguments, _) => {
                if let Some(size) = self.unit.stack_allocation_size(expression) {
                    tasks.push(MachineTask::AllocateDynamic { expression });
                    tasks.push(MachineTask::EvalExpression {
                        expression: size,
                        allow_root_mutation: true,
                    });
                    return Ok(());
                }
                tasks.push(MachineTask::EvalCallArguments {
                    callee,
                    arguments,
                    index: 0,
                    values: Vec::new(),
                    span,
                    expects_value: true,
                });
            }
            CHirExpr::CheckedArithmetic {
                op,
                lhs,
                rhs,
                target,
                target_ty,
                ..
            } => {
                tasks.push(MachineTask::ApplyCheckedArithmetic {
                    operator: *op,
                    target_type: *target_ty,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: target,
                    allow_root_mutation: true,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: rhs,
                    allow_root_mutation: true,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: lhs,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::BitBuiltin { op, operand, .. } => {
                tasks.push(MachineTask::ApplyBitBuiltin {
                    operator: *op,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: operand,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::BuiltinUnreachable { .. } => {
                return Err(InterpreterError::UnreachableReached { span });
            }
            CHirExpr::BuiltinTrap { .. } => {
                return Err(InterpreterError::BuiltinTrapReached { span });
            }
            CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { depth: 0, .. } => {
                let frame =
                    self.memory_frame
                        .as_ref()
                        .ok_or(InterpreterError::UnsupportedExpression {
                            feature: "activation address outside a call frame",
                            span,
                        })?;
                // Function identities occupy the initial range. Frame IDs are
                // monotonic across calls, including recursion and returned frames.
                let identity = self.symbol_order().code_identities
                    + frame.id.raw() * 2
                    + u64::from(matches!(expression, CHirExpr::BuiltinFrameAddress { .. }));
                let pointer = PointerValue::function(identity)
                    .map_err(|error| self.memory_error(error, span))?;
                let (memory, address) = self
                    .memory
                    .expose_pointer(pointer)
                    .map_err(|error| self.memory_error(error, span))?;
                self.memory = memory;
                let ty = PointerTypeDesc::from_expression(
                    self.unit.symbols(),
                    self.unit.types(),
                    expression,
                )
                .map_err(|_| InterpreterError::UnsupportedExpression {
                    feature: "activation address has no pointer type",
                    span,
                })?;
                values.push(CValue::Pointer {
                    value: Pointer::from_address(address),
                    ty,
                });
            }
            CHirExpr::BuiltinFrameAddress { .. } => {
                return Err(InterpreterError::FrameAddressUnavailable { span });
            }
            CHirExpr::BuiltinVarArgs { .. } => self.schedule_varargs(expression, tasks)?,
            CHirExpr::LabelAddress { .. } => {
                values.push(self.label_address_value(expression)?);
            }
            CHirExpr::Cast(ty, value, _) => {
                let target = self.value_type(*ty, span)?;
                tasks.push(MachineTask::CastValue { target, span });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    // A C cast converts the result; it is not an evaluation
                    // barrier for a side effect in its operand.
                    allow_root_mutation,
                });
            }
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                facts,
                ..
            } if self.mutation_allowed(allow_root_mutation) => {
                if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                    return Err(InterpreterError::InvalidScalarPlace {
                        symbol: place.symbol(),
                        span,
                    });
                }
                let symbol = if self.memory_mode {
                    self.validate_memory_scalar_place(*place, target, span)?
                } else {
                    self.validate_place(*place, target, span)?
                };
                tasks.push(MachineTask::ApplyAssignment {
                    operator: *op,
                    symbol,
                    target_span: target.span(),
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: false,
                });
            }
            CHirExpr::MemoryRead {
                place,
                target_span,
                facts,
                ..
            } if self.memory_mode => {
                self.validate_readable_place(place, *facts, *target_span)?;
                if !matches!(
                    self.unit.types().get(place.value_ty()).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) {
                    tasks.push(MachineTask::LoadProjectedPlace {
                        place,
                        span: *target_span,
                    });
                }
                self.schedule_place_operands(place, *target_span, tasks, values)?;
            }
            CHirExpr::Update {
                op,
                place,
                target,
                facts,
                ..
            } if self.mutation_allowed(allow_root_mutation) => {
                if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                    return Err(InterpreterError::InvalidScalarPlace {
                        symbol: place.symbol(),
                        span,
                    });
                }
                let symbol = if self.memory_mode {
                    self.validate_memory_scalar_place(*place, target, span)?
                } else {
                    self.validate_place(*place, target, span)?
                };
                tasks.push(MachineTask::ApplyScalarUpdate {
                    operator: *op,
                    symbol,
                    target_span: target.span(),
                    span,
                });
            }
            CHirExpr::MemoryUpdate {
                op,
                place,
                target_span,
                facts,
                ..
            } if self.memory_mode && self.mutation_allowed(allow_root_mutation) => {
                self.validate_mutable_place(place, *facts, *target_span)?;
                tasks.push(MachineTask::ApplyIndirectUpdate {
                    operator: *op,
                    bit_field: self.place_bit_field(place, *target_span)?,
                    layout: self.place_access_layout(place, *target_span)?,
                    value_type: place.value_ty(),
                    qualifiers: place.access_qualifiers(),
                    target_span: *target_span,
                    span,
                });
                tasks.push(MachineTask::FoldPlaceAddress {
                    place,
                    span: *target_span,
                });
                self.schedule_place_operands(place, *target_span, tasks, values)?;
            }
            CHirExpr::MemoryAssign {
                op,
                place,
                target_span,
                value,
                facts,
                ..
            } if self.memory_mode && self.mutation_allowed(allow_root_mutation) => {
                place
                    .validate_against(self.unit.symbols(), self.unit.types())
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "invalid typed memory place",
                        span: *target_span,
                    })?;
                if facts.ty() != place.value_ty()
                    || facts.category() != ExprCategory::Value
                    || !place.is_modifiable()
                {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "unsupported typed memory place",
                        span: *target_span,
                    });
                }
                let pointer_type = PointerTypeDesc::from_addressed_place(place);
                tasks.push(MachineTask::ApplyIndirectAssignment {
                    operator: *op,
                    bit_field: self.place_bit_field(place, *target_span)?,
                    layout: self.place_access_layout(place, *target_span)?,
                    pointer_type,
                    value_type: place.value_ty(),
                    qualifiers: place.access_qualifiers(),
                    target_span: *target_span,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: false,
                });
                tasks.push(MachineTask::FoldPlaceAddress {
                    place,
                    span: *target_span,
                });
                self.schedule_place_operands(place, *target_span, tasks, values)?;
            }
            CHirExpr::Member(base, name, _) if self.memory_mode => {
                tasks.push(MachineTask::LoadAggregateMember {
                    name: name.as_str(),
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: base,
                    allow_root_mutation: true,
                });
            }
            CHirExpr::SizeOf(ty, _) | CHirExpr::SizeOfExpr(_, ty, _) => {
                let size = self
                    .target
                    .sizeof_type(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "sizeof incomplete or unsupported type",
                        span,
                    })?;
                values.push(CValue::Integer(CInteger::new(size, CIntegerType::ULONG)));
            }
            CHirExpr::AlignOf(ty, _) => {
                let alignment = self
                    .target
                    .alignof_type(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "alignof incomplete or unsupported type",
                        span,
                    })?;
                values.push(CValue::Integer(CInteger::new(
                    u64::from(alignment),
                    CIntegerType::ULONG,
                )));
            }
            CHirExpr::StatementExpr {
                body, value, span, ..
            } => {
                self.schedule_statement_expression(body, *value, *span, tasks)?;
            }
            CHirExpr::CompoundLit(_, _, _) if self.memory_mode => {
                self.schedule_compound_literal(expression, false, tasks)?;
            }
            CHirExpr::CompoundLit(_, _, _) => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "compound literal outside the object-memory machine",
                    span,
                });
            }
            // Reaching this means requirement analysis concluded the program
            // needs no object memory while its C-HIR designates a subobject.
            // The two must agree, so this is reported rather than papered over
            // by silently enabling memory here.
            CHirExpr::MemoryRead { target_span, .. } => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "memory read outside the object-memory machine",
                    span: *target_span,
                });
            }
            CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. } => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "function call nested in scalar mutation",
                    span,
                });
            }
            CHirExpr::Member(_, _, _) | CHirExpr::PtrMember(_, _, _) | CHirExpr::Index(_, _, _) => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "function call nested outside scalar expression subset",
                    span,
                });
            }
            CHirExpr::StringLit(_, _) => values.push(self.string_pointer(expression, true)?),
            CHirExpr::FloatLit(literal, _) => {
                let kind = match literal.suffix {
                    crate::FloatSuffix::F => evocc_types::FloatKind::F32,
                    crate::FloatSuffix::None => evocc_types::FloatKind::F64,
                    crate::FloatSuffix::L => evocc_types::FloatKind::F128,
                };
                let value = match literal.value {
                    crate::FloatConstant::Real(value) => value.target_convert(kind).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "deferred floating format",
                            span,
                        },
                    )?,
                    crate::FloatConstant::QuietNaN(payload) => {
                        evocc_types::floating::FloatBits::from_bits(
                            kind,
                            if kind == evocc_types::FloatKind::F32 {
                                0x7fc00000 | (payload & 0x3fffff)
                            } else if kind == evocc_types::FloatKind::F128 {
                                (0x7fff8u128 << 108) | (payload & ((1 << 111) - 1))
                            } else {
                                0x7ff8000000000000 | (payload & 0x7ffffffffffff)
                            },
                        )
                        .unwrap()
                    }
                };
                values.push(CValue::Floating(value));
            }
        }
        Ok(())
    }

    fn mutation_allowed(&self, _allow_root_mutation: bool) -> bool {
        // An executed operand may itself be an assignment or update. The
        // constant-initializer evaluator is the separate non-mutating path.
        !self.static_integer_evaluation
    }

    fn start_call(
        &mut self,
        callee: &'arena CHirExpr<'arena>,
        arguments: Vec<CValue>,
        call_span: SourceSpan,
        expects_value: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
        value_base: usize,
    ) -> Result<Option<CValue>, InterpreterError> {
        self.tick(callee.span())?;
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span,
        } = callee
        else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "indirect scalar function call",
                span: callee.span(),
            });
        };
        let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
            InterpreterError::UnsupportedExpression {
                feature: "unknown function designator",
                span: *span,
            }
        })?;
        if declaration.kind() != SymbolKind::Function
            || declaration.spelling() != spelling
            || !crate::type_at_use_matches_binding(self.unit.types(), facts.ty(), declaration.ty())
            || facts.category() != ExprCategory::FunctionDesignator
        {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "inconsistent function designator",
                span: *span,
            });
        }

        if self.unit.symbols().is_builtin_function(*symbol)
            && evocc_types::neon::signature(spelling).is_some()
        {
            return self.neon_call(spelling, facts.ty(), &arguments, call_span);
        }
        if spelling == "__sync_synchronize" && self.unit.symbols().is_builtin_function(*symbol) {
            if !arguments.is_empty() {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "full memory barrier arity",
                    span: call_span,
                });
            }
            // Match the lowered, ordered assembly boundary instead of
            // pretending that this interpreter models concurrent execution.
            return Err(InterpreterError::Asm { span: call_span });
        }
        if spelling == "__builtin_prefetch" && self.unit.symbols().is_builtin_function(*symbol) {
            // Argument evaluation has already run through the normal call
            // continuation; the hint itself performs no abstract memory access.
            return Ok(None);
        }
        if spelling == "__builtin_extract_return_addr"
            && self.unit.symbols().is_builtin_function(*symbol)
        {
            let [value] = arguments.as_slice() else {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "return-address extraction arity",
                    span: call_span,
                });
            };
            return Ok(Some(*value));
        }
        let target = self
            .linkage
            .target(linked_library_definition(self.unit, *symbol));
        if !self.unit.iter().any(|declaration| matches!(declaration, CHirDecl::Function(function) if function.symbol == target && function.body.is_some())) {
            let name = self.linkage.function_name(*symbol, spelling);
            let result = self.external_library_call(target, name, &arguments, call_span)?;
            return Ok(Some(result));
        }

        let caller_slots = std::mem::take(&mut self.slots);
        let caller_fuel_anchor = self.fuel_anchor.take();
        self.enter_function(
            target,
            arguments,
            Some(caller_slots),
            caller_fuel_anchor,
            Some(call_span),
            expects_value,
            value_base,
            tasks,
        )?;
        Ok(None)
    }

    fn external_library_call(
        &mut self,
        symbol: SymbolId,
        spelling: &str,
        arguments: &[CValue],
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        if self.program && matches!(spelling, "exit" | "_Exit" | "_exit" | "quick_exit") {
            let [CValue::Integer(value)] = arguments else {
                return Err(InterpreterError::UnsupportedFunctionSignature { symbol, span });
            };
            self.termination = Some((value.raw_bits() & u128::from(u32::MAX), spelling == "exit"));
            return Ok(CValue::Integer(*value));
        }
        let entry = self
            .unit
            .symbols()
            .symbol(symbol)
            .map_err(|_| InterpreterError::UnknownFunction { symbol })?;
        let Some(TypeKind::Function {
            return_type,
            parameters,
        }) = self.unit.types().get(entry.ty()).map(|ty| &ty.kind)
        else {
            return Err(InterpreterError::UnsupportedFunctionSignature { symbol, span });
        };
        let return_type = self.value_type(*return_type, span)?;
        let parameter_types = match parameters {
            FunctionParameters::Prototype { types, .. } => types.as_slice(),
            FunctionParameters::Unspecified => self
                .unit
                .unprototyped_parameters(symbol)
                .unwrap_or_default(),
        };
        let arguments = arguments
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let value = match parameter_types.get(index) {
                    Some(ty) => {
                        self.cast_runtime_value(*value, self.value_type(*ty, span)?, span)?
                    }
                    None => *value,
                };
                match value {
                    CValue::Integer(value) => u64::try_from(value.raw_bits())
                        .map(LibraryValue::Integer)
                        .map_err(|_| InterpreterError::WideInteger { span }),
                    CValue::Pointer { value, .. } => Ok(LibraryValue::Pointer(value)),
                    _ => Err(self.memory_error(
                        MemoryFault::ExternalFunction {
                            name: spelling.into(),
                        },
                        span,
                    )),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (memory, world, result) = self
            .memory
            .call_library_with_world(
                &self.world,
                evocc_memory::FenvRuntime::Aarch64Glibc,
                spelling,
                &arguments,
            )
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        self.world = world;
        match (result, return_type) {
            (LibraryValue::Integer(value), ValueType::Integer(ty)) => {
                Ok(CValue::Integer(CInteger::from_raw(u128::from(value), ty)))
            }
            (LibraryValue::Pointer(value), ValueType::Pointer(ty)) => {
                Ok(CValue::Pointer { value, ty })
            }
            _ => Err(InterpreterError::UnsupportedFunctionSignature { symbol, span }),
        }
    }

    fn start_indirect_call(
        &mut self,
        callee: CValue,
        arguments: Vec<CValue>,
        call_span: SourceSpan,
        expects_value: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
        value_base: usize,
    ) -> Result<Option<CValue>, InterpreterError> {
        let CValue::FunctionPointer { value, ty } = callee else {
            return Err(InterpreterError::CallThroughNonFunctionPointer { span: call_span });
        };
        if !matches!(
            self.unit.types().get(ty.pointee()).map(|ty| &ty.kind),
            Some(TypeKind::Function { .. })
        ) {
            return Err(InterpreterError::CallThroughNonFunctionPointer { span: call_span });
        }
        let value = if let PointerValue::Exposed(address) = value {
            (1..=self.symbol_order().functions.len() as u64)
                .map(PointerValue::Function)
                .find(|candidate| self.memory.pointer_address(*candidate) == Ok(address))
                .unwrap_or(value)
        } else {
            value
        };
        let value = self
            .memory
            .resolve_function_pointer(value)
            .map_err(|_| InterpreterError::CallThroughNonFunctionPointer { span: call_span })?;
        let identity = match value {
            PointerValue::Null => {
                return Err(InterpreterError::CallThroughNullFunctionPointer { span: call_span });
            }
            PointerValue::Object(_) | PointerValue::Exposed(_) => {
                return Err(InterpreterError::CallThroughNonFunctionPointer { span: call_span });
            }
            PointerValue::Function(identity) => identity,
        };
        let symbol = self
            .function_symbol(identity)
            .ok_or(InterpreterError::CallThroughNonFunctionPointer { span: call_span })?;
        let entry = self
            .unit
            .symbols()
            .symbol(symbol)
            .map_err(|_| InterpreterError::UnknownFunction { symbol })?;
        if !self.function_types_compatible(entry.ty(), ty.pointee()) {
            return Err(InterpreterError::IndirectCallDynamicSignatureMismatch { span: call_span });
        }
        if matches!(
            self.unit.types().get(ty.pointee()).map(|ty| &ty.kind),
            Some(TypeKind::Function {
                parameters: FunctionParameters::Unspecified,
                ..
            })
        ) {
            let parameters = match self.unit.types().get(entry.ty()).map(|ty| &ty.kind) {
                Some(TypeKind::Function {
                    parameters: FunctionParameters::Prototype { types, .. },
                    ..
                }) => types.as_slice(),
                _ => &[],
            };
            if arguments.len() != parameters.len() {
                return Err(InterpreterError::IndirectCallDynamicSignatureMismatch {
                    span: call_span,
                });
            }
            for (argument, parameter) in arguments.iter().zip(parameters) {
                let expected = self.value_type(*parameter, call_span)?;
                let compatible = match (argument, expected) {
                    (CValue::Integer(value), ValueType::Integer(integer)) => {
                        value.ty().bits() == integer.bits()
                    }
                    (
                        CValue::Pointer { ty, .. } | CValue::FunctionPointer { ty, .. },
                        ValueType::Pointer(expected),
                    ) => self
                        .unit
                        .types()
                        .compatible_ignoring_top_qualifiers(ty.pointee(), expected.pointee()),
                    (CValue::Aggregate { ty, .. }, ValueType::Aggregate(expected)) => self
                        .unit
                        .types()
                        .compatible_ignoring_top_qualifiers(*ty, expected),
                    (CValue::Uninitialized(_), _) => true,
                    _ => false,
                };
                if !compatible {
                    return Err(InterpreterError::IndirectCallDynamicSignatureMismatch {
                        span: call_span,
                    });
                }
            }
        }
        if !self.unit.iter().any(|declaration| matches!(declaration, CHirDecl::Function(function) if function.symbol == symbol && function.body.is_some())) {
            let name = self.linkage.function_name(symbol, entry.spelling());
            return self.external_library_call(symbol, name, &arguments, call_span).map(Some);
        }
        let caller_slots = std::mem::take(&mut self.slots);
        let caller_fuel_anchor = self.fuel_anchor.take();
        self.enter_function(
            symbol,
            arguments,
            Some(caller_slots),
            caller_fuel_anchor,
            Some(call_span),
            expects_value,
            value_base,
            tasks,
        )?;
        Ok(None)
    }

    fn function_identity(&self, symbol: SymbolId) -> Option<u64> {
        let symbol = self.linkage.target(symbol);
        let name = self.unit.symbols().symbol(symbol).ok()?.spelling();
        self.symbol_order()
            .functions
            .iter()
            .position(|candidate| {
                self.unit
                    .symbols()
                    .symbol(*candidate)
                    .is_ok_and(|entry| entry.spelling() == name)
            })
            .and_then(|index| u64::try_from(index).ok())
            .and_then(|index| index.checked_add(1))
    }

    fn label_address_value(
        &mut self,
        expression: &CHirExpr<'arena>,
    ) -> Result<CValue, InterpreterError> {
        let span = expression.span();
        let identity = self
            .symbol_order()
            .labels
            .get(&(expression as *const CHirExpr<'_> as usize))
            .map(|(_, _, id)| *id)
            .ok_or(InterpreterError::InvalidIndirectBranchTarget { span })?;
        let (memory, address) = self
            .memory
            .expose_pointer(PointerValue::Function(identity))
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        Ok(CValue::Pointer {
            value: Pointer::from_address(address),
            ty: self.pointer_type_of_expression(expression)?,
        })
    }

    fn symbol_order(&self) -> &SymbolOrder {
        self.symbol_order.get_or_init(|| {
            let mut symbols = self
                .unit
                .iter()
                .filter_map(|declaration| match declaration {
                    CHirDecl::Function(function) if function.body.is_some() => {
                        Some(function.symbol)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let defined = symbols.iter().copied().collect::<BTreeSet<_>>();
            let mut referenced = ReferencedSymbols::default();
            for declaration in self.unit.iter() {
                match declaration {
                    CHirDecl::Function(function) => {
                        if let Some(body) = function.body {
                            collect_statement_symbols(body, &mut referenced);
                        }
                    }
                    CHirDecl::Object(object) => {
                        if let Some(initializer) = &object.initializer {
                            for action in initializer.plan().actions() {
                                if let InitAction::StoreScalar { value, .. }
                                | InitAction::CopyAggregate { value, .. } = action
                                {
                                    collect_expression_symbols(value, &mut referenced);
                                }
                            }
                        }
                    }
                    CHirDecl::Asm(_) => collect_declaration_symbols(declaration, &mut referenced),
                    CHirDecl::Tag(_) | CHirDecl::Typedef(_) => {}
                }
            }
            let mut functions = BTreeMap::new();
            let mut objects = BTreeSet::new();
            for symbol in referenced.named {
                let symbol = self
                    .linkage
                    .target(linked_library_definition(self.unit, symbol));
                let Ok(entry) = self.unit.symbols().symbol(symbol) else {
                    continue;
                };
                match entry.kind() {
                    SymbolKind::Function if !defined.contains(&symbol) => {
                        functions.insert(entry.spelling().to_owned(), symbol);
                    }
                    SymbolKind::Object
                        if entry.storage_class().map(StorageClass::base)
                            == Some(StorageClass::Extern)
                            && !self
                                .manifest
                                .as_ref()
                                .is_some_and(|manifest| manifest.object_site(symbol).is_some()) =>
                    {
                        objects.insert(entry.spelling().to_owned());
                    }
                    _ => {}
                }
            }
            symbols.extend(functions.into_values());
            let mut labels = BTreeMap::new();
            let mut identities = BTreeMap::new();
            for declaration in self.unit.iter() {
                if let CHirDecl::Function(function) = declaration {
                    if let Some(body) = function.body {
                        let mut references = ReferencedSymbols::default();
                        collect_statement_symbols(body, &mut references);
                        for (node, label) in references.labels {
                            let next = symbols.len() as u64 + identities.len() as u64 + 1;
                            let identity = *identities
                                .entry((function.symbol, label.clone()))
                                .or_insert(next);
                            labels.insert(node, (function.symbol, label, identity));
                        }
                    }
                }
            }
            let code_identities = symbols.len() as u64 + identities.len() as u64;
            SymbolOrder {
                labels,
                code_identities,
                functions: symbols,
                objects: objects
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, name)| (name, u64::MAX - ordinal as u64))
                    .collect(),
            }
        })
    }

    fn function_symbol(&self, identity: u64) -> Option<SymbolId> {
        let index = identity
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())?;
        self.symbol_order().functions.get(index).copied()
    }

    #[allow(clippy::too_many_arguments)]
    fn enter_function(
        &mut self,
        function: SymbolId,
        arguments: Vec<CValue>,
        caller_slots: Option<HashMap<SymbolId, Slot>>,
        caller_fuel_anchor: Option<SourceSpan>,
        call_span: Option<SourceSpan>,
        caller_expects_value: bool,
        value_base: usize,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let declaration = self.function_definition(function)?;
        if let Some(refusal) = self
            .unit
            .profile_refusals()
            .iter()
            .find(|r| r.function == function)
        {
            return Err(InterpreterError::Profile(refusal.clone()));
        }
        let signature = self.validate_function_signature(declaration, arguments.len())?;
        let return_type = signature.return_type;
        let return_value_type = signature.return_value_type;
        if call_span.is_some() && caller_expects_value && return_value_type == ValueType::Void {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "void call in scalar expression",
                span: call_span.unwrap_or(declaration.span),
            });
        }
        if call_span.is_some() && !caller_expects_value && return_value_type != ValueType::Void {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "non-void call scheduled as a void expression statement",
                span: call_span.unwrap_or(declaration.span),
            });
        }
        let binding_span = call_span.unwrap_or(declaration.span);
        let mut arguments = arguments;
        let anonymous = arguments.split_off(signature.parameters.len());
        if anonymous
            .iter()
            .any(|value| matches!(value, CValue::Aggregate { ty, .. } if !crate::is_integer_variadic_record(self.unit.types(), *ty)))
        {
            return Err(InterpreterError::VarArgs { span: binding_span });
        }
        let mut parameter_bindings = Vec::with_capacity(arguments.len());
        for ((symbol, ty), argument) in signature.parameters.into_iter().zip(arguments) {
            let argument = self.cast_runtime_value(argument, ty, binding_span)?;
            let Some(symbol) = symbol else {
                // Argument evaluation and conversion still happen; there is no
                // source identifier to bind when the parameter name is omitted.
                continue;
            };
            match (ty, argument) {
                (ValueType::Floating(kind), CValue::Floating(value)) => {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Floating(kind),
                        CValue::Floating(value),
                    ));
                }
                (ValueType::Integer(ty), CValue::Integer(value)) => {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Integer(ty),
                        CValue::Integer(value),
                    ));
                }
                (ValueType::Pointer(pointer_type), CValue::Pointer { value, ty })
                    if pointer_type == ty =>
                {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Pointer(pointer_type),
                        CValue::Pointer { value, ty },
                    ));
                }
                (ValueType::Pointer(pointer_type), CValue::FunctionPointer { value, ty })
                    if pointer_type == ty =>
                {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Pointer(pointer_type),
                        CValue::FunctionPointer {
                            value,
                            ty: pointer_type,
                        },
                    ));
                }
                (
                    ValueType::Aggregate(parameter_type),
                    CValue::Aggregate {
                        address,
                        ty,
                        qualifiers,
                    },
                ) if self.same_aggregate_type(parameter_type, ty) => {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Aggregate(parameter_type),
                        CValue::Aggregate {
                            address,
                            ty,
                            qualifiers,
                        },
                    ));
                }
                (ValueType::Void, _) | (_, _) => {
                    return Err(InterpreterError::UnsupportedFunctionSignature {
                        symbol: function,
                        span: declaration.span,
                    });
                }
            }
        }

        let body = declaration
            .body
            .ok_or(InterpreterError::FunctionHasNoBody {
                symbol: function,
                span: declaration.span,
            })?;
        let body_span = body.span();
        let is_main = declaration.name.spelling == "main";

        self.slots = HashMap::new();
        let caller_memory_frame = if self.memory_mode {
            let caller = self.memory_frame.take();
            let (memory, frame, root_scope) = self
                .memory
                .begin_frame()
                .map_err(|error| self.memory_error(error, binding_span))?;
            self.memory = memory;
            // Anonymous records are values just like named record parameters:
            // preserve a copy before executing the callee, including va_copy.
            let mut anonymous_values = Vec::with_capacity(anonymous.len());
            for value in anonymous {
                let value = if let CValue::Aggregate { ty, .. } = value {
                    let layout = self.access_layout(ty, binding_span)?;
                    let (memory, destination) = self
                        .memory
                        .allocate_automatic(frame, root_scope, self.temporary_origin_site, layout)
                        .map_err(|error| self.memory_error(error, binding_span))?;
                    self.memory = memory;
                    self.store_memory_value(
                        destination,
                        ty,
                        Qualifiers::default(),
                        value,
                        binding_span,
                    )?
                } else {
                    value
                };
                anonymous_values.push(value);
            }
            self.memory_frame = Some(MemoryFrame {
                cleanups: Vec::new(),
                anonymous: anonymous_values,
                id: frame,
                root_scope,
                current_scope: root_scope,
                bindings: HashMap::new(),
                compound_bindings: HashMap::new(),
            });
            for (symbol, _, value) in parameter_bindings {
                let ty = self
                    .unit
                    .symbols()
                    .symbol(symbol)
                    .map_err(|_| InterpreterError::UnsupportedFunctionSignature {
                        symbol: function,
                        span: declaration.span,
                    })?
                    .ty();
                self.allocate_automatic_object(symbol, ty, None, binding_span)?;
                self.store_bound_value(symbol, value, binding_span)?;
            }
            caller
        } else {
            for (symbol, ty, value) in parameter_bindings {
                let (ValueType::Integer(ty), CValue::Integer(value)) = (ty, value) else {
                    return Err(InterpreterError::UnsupportedFunctionSignature {
                        symbol: function,
                        span: declaration.span,
                    });
                };
                if self
                    .slots
                    .insert(
                        symbol,
                        Slot {
                            ty,
                            value: Some(value),
                            active: true,
                        },
                    )
                    .is_some()
                {
                    return Err(InterpreterError::UnsupportedFunctionSignature {
                        symbol: function,
                        span: declaration.span,
                    });
                }
            }
            self.predeclare_stmt(body)?;
            None
        };
        validate_function_control_flow(body)?;

        tasks.push(MachineTask::FunctionEnd(FunctionEnd {
            symbol: function,
            return_type,
            return_value_type,
            is_main,
            body_span,
            caller_slots,
            caller_memory_frame,
            caller_fuel_anchor,
            caller_expects_value,
            value_base,
        }));
        tasks.push(MachineTask::ExecStatement(body));
        Ok(())
    }

    fn validate_root_function(&self, function: SymbolId) -> Result<(), InterpreterError> {
        let declaration = self.function_definition(function)?;
        let signature = self.validate_function_signature(declaration, 0)?;
        if matches!(signature.return_value_type, ValueType::Pointer(_)) {
            return Err(InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            });
        }
        Ok(())
    }

    fn validate_function_signature(
        &self,
        declaration: &FunctionDecl<'arena>,
        argument_count: usize,
    ) -> Result<ValidatedFunctionSignature, InterpreterError> {
        let function = declaration.symbol;
        let symbol = self.unit.symbols().symbol(function).map_err(|_| {
            InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            }
        })?;
        let canonical = self.unit.types().get(declaration.ty).ok_or(
            InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            },
        )?;
        let TypeKind::Function {
            return_type,
            parameters: canonical_parameters,
        } = &canonical.kind
        else {
            return Err(InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            });
        };
        if symbol.kind() != SymbolKind::Function
            || symbol.spelling() != declaration.name.spelling
            || !self.function_types_compatible(symbol.ty(), declaration.ty)
            || !symbol
                .storage_class()
                .is_some_and(|storage| function_storage_compatible(storage, declaration.storage))
            || !self.written_declarator_matches(
                &declaration.written,
                declaration.ty,
                declaration.return_specifier.type_id(),
                Some(&declaration.parameters),
                false,
                &declaration.attributes,
            )
        {
            return Err(InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            });
        }
        let return_value_type = self.value_type(*return_type, declaration.span)?;
        let parameters = match (canonical_parameters, &declaration.parameters) {
            (FunctionParameters::Unspecified, ParameterSyntax::Unspecified)
                if argument_count == 0 =>
            {
                Vec::new()
            }
            (FunctionParameters::Prototype { types, variadic }, syntax)
                if !matches!(syntax, ParameterSyntax::Unspecified)
                    && *variadic
                        == matches!(syntax, ParameterSyntax::Prototype { variadic: true, .. })
                    && types.len() == syntax.parameters().len()
                    && (types.len() == argument_count
                        || (*variadic && types.len() <= argument_count)) =>
            {
                let parameters = syntax.parameters();
                let mut validated = Vec::with_capacity(parameters.len());
                let mut parameter_ids = BTreeSet::new();
                let mut parameter_spellings = BTreeSet::new();
                let mut function_scope = None;
                for (canonical_type, parameter) in types.iter().zip(parameters) {
                    if !syntax.parameter_type_matches(
                        self.unit.types(),
                        *canonical_type,
                        parameter.ty,
                    ) || !self.written_declarator_matches(
                        &parameter.written,
                        parameter.ty,
                        parameter.specifier.type_id(),
                        None,
                        true,
                        &parameter.attributes,
                    ) || !matches!(
                        parameter.storage,
                        StorageClass::None | StorageClass::Register
                    ) {
                        return Err(InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        });
                    }
                    if parameter.symbol.is_none() && parameter.name.is_none() {
                        validated.push((None, self.value_type(parameter.ty, parameter.span)?));
                        continue;
                    }
                    let (Some(parameter_symbol), Some(parameter_name)) =
                        (parameter.symbol, &parameter.name)
                    else {
                        return Err(InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        });
                    };
                    let symbol = self.unit.symbols().symbol(parameter_symbol).map_err(|_| {
                        InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        }
                    })?;
                    let scope = self.unit.symbols().scope(symbol.scope()).map_err(|_| {
                        InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        }
                    })?;
                    let scope_is_valid = scope.kind() == ScopeKind::Function
                        && scope.parent() == Some(self.unit.symbols().file_scope())
                        && function_scope.is_none_or(|expected| expected == symbol.scope());
                    if symbol.kind() != SymbolKind::Parameter
                        || symbol.spelling() != parameter_name.spelling
                        || symbol.ty() != parameter.ty
                        || !matches!(
                            symbol.storage_class(),
                            Some(StorageClass::None | StorageClass::Register)
                        )
                        || symbol.storage_class() != Some(parameter.storage)
                        || !scope_is_valid
                        || !parameter_ids.insert(parameter_symbol)
                        || !parameter_spellings.insert(parameter_name.spelling.clone())
                    {
                        return Err(InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        });
                    }
                    function_scope = Some(symbol.scope());
                    validated.push((
                        Some(parameter_symbol),
                        self.value_type(parameter.ty, parameter.span)?,
                    ));
                }
                validated
            }
            _ => {
                return Err(InterpreterError::UnsupportedFunctionSignature {
                    symbol: function,
                    span: declaration.span,
                });
            }
        };
        Ok(ValidatedFunctionSignature {
            return_type: *return_type,
            return_value_type,
            parameters,
        })
    }

    fn function_types_compatible(&self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        let (Some(left), Some(right)) = (self.unit.types().get(left), self.unit.types().get(right))
        else {
            return false;
        };
        let (
            TypeKind::Function {
                return_type: left_return,
                parameters: left_parameters,
            },
            TypeKind::Function {
                return_type: right_return,
                parameters: right_parameters,
            },
        ) = (&left.kind, &right.kind)
        else {
            return false;
        };
        self.unit.types().compatible(*left_return, *right_return)
            && function_parameters_compatible(self.unit.types(), left_parameters, right_parameters)
    }

    fn written_declarator_matches(
        &self,
        written: &WrittenDeclarator,
        mut current: TypeId,
        terminal: TypeId,
        function_syntax: Option<&ParameterSyntax>,
        adjusted_parameter: bool,
        attributes: &[crate::Attribute],
    ) -> bool {
        if adjusted_parameter
            && (matches!(
                written.layers.first(),
                Some(WrittenDeclaratorLayer::Function { .. })
            ) || (written.layers.is_empty()
                && self
                    .unit
                    .types()
                    .get(terminal)
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))))
        {
            if let Some(canonical) = self.unit.types().get(current) {
                if let TypeKind::Pointer(pointee) = canonical.kind {
                    if canonical.qualifiers == Qualifiers::default()
                        && self
                            .unit
                            .types()
                            .get(pointee)
                            .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))
                    {
                        current = pointee;
                    }
                }
            }
        }
        if function_syntax.is_some()
            && !matches!(
                written.layers.first(),
                Some(WrittenDeclaratorLayer::Function { .. })
            )
        {
            return false;
        }
        for (index, layer) in written.layers.iter().enumerate() {
            let Some(canonical) = self.unit.types().get(current) else {
                return false;
            };
            current = match (layer, &canonical.kind) {
                (
                    WrittenDeclaratorLayer::Function { .. },
                    TypeKind::Function {
                        return_type,
                        parameters,
                    },
                ) => {
                    if index == 0
                        && function_syntax.is_some_and(|syntax| {
                            !function_parameters_match(self.unit.types(), parameters, syntax)
                        })
                    {
                        return false;
                    }
                    *return_type
                }
                (
                    WrittenDeclaratorLayer::Pointer { qualifiers, .. },
                    TypeKind::Pointer(pointee),
                ) if canonical.qualifiers == *qualifiers => *pointee,
                (
                    WrittenDeclaratorLayer::Array {
                        parameter_qualifiers,
                        ..
                    },
                    TypeKind::Pointer(pointee),
                ) if adjusted_parameter
                    && index == 0
                    && canonical.qualifiers == *parameter_qualifiers =>
                {
                    *pointee
                }
                (
                    WrittenDeclaratorLayer::Array { bound, .. },
                    TypeKind::Array {
                        element,
                        bound: canonical_bound,
                    },
                ) if bound == canonical_bound => *element,
                _ => return false,
            };
        }
        current == terminal
            || crate::vector::attribute_matches_base(
                self.unit.types(),
                &self.target,
                attributes,
                current,
                terminal,
            )
            || (adjusted_parameter
                && written.layers.is_empty()
                && matches!(
                    (self.unit.types().get(current).map(|ty| &ty.kind), self.unit.types().get(terminal).map(|ty| &ty.kind)),
                    (Some(TypeKind::Pointer(pointee)), Some(TypeKind::Array { element, .. })) if pointee == element
                ))
    }

    fn function_definition(
        &self,
        function: SymbolId,
    ) -> Result<&'arena FunctionDecl<'arena>, InterpreterError> {
        let function = self
            .linkage
            .target(linked_library_definition(self.unit, function));
        let symbol = self
            .unit
            .symbols()
            .symbol(function)
            .map_err(|_| InterpreterError::UnknownFunction { symbol: function })?;
        if symbol.kind() != SymbolKind::Function {
            return Err(InterpreterError::UnknownFunction { symbol: function });
        }

        let mut definition = None;
        for declaration in self.unit.iter() {
            let CHirDecl::Function(declaration) = declaration else {
                continue;
            };
            if declaration.symbol != function {
                continue;
            }
            if declaration.name.spelling != symbol.spelling()
                || !self.function_types_compatible(declaration.ty, symbol.ty())
                || !symbol.storage_class().is_some_and(|storage| {
                    function_storage_compatible(storage, declaration.storage)
                })
            {
                return Err(InterpreterError::MalformedFunctionDeclaration {
                    symbol: function,
                    span: declaration.span,
                });
            }
            if declaration.body.is_some() && definition.replace(declaration).is_some() {
                return Err(InterpreterError::DuplicateFunctionBody {
                    symbol: function,
                    span: declaration.span,
                });
            }
        }
        definition.ok_or(InterpreterError::FunctionHasNoBody {
            symbol: function,
            span: symbol.span(),
        })
    }

    fn finish_function(
        &mut self,
        end: FunctionEnd<'arena>,
        exit: FunctionExit,
        values: &mut Vec<CValue>,
    ) -> Result<Option<MachineOutcome>, InterpreterError> {
        let mut return_value = match (exit, end.return_value_type) {
            (FunctionExit::Explicit(Some(CValue::Uninitialized(source))), target)
                if source == target =>
            {
                Some(CValue::Uninitialized(target))
            }
            (FunctionExit::Explicit(Some(value)), ValueType::Floating(kind)) => {
                Some(self.cast_runtime_value(value, ValueType::Floating(kind), end.body_span)?)
            }
            (FunctionExit::Explicit(Some(value)), ValueType::Integer(return_integer)) => Some(
                self.cast_runtime_value(value, ValueType::Integer(return_integer), end.body_span)?,
            ),
            (FunctionExit::Explicit(Some(value)), ValueType::Pointer(return_type)) => Some(
                self.cast_runtime_value(value, ValueType::Pointer(return_type), end.body_span)?,
            ),
            (FunctionExit::Explicit(Some(value)), ValueType::Aggregate(return_type)) => Some(
                self.cast_runtime_value(value, ValueType::Aggregate(return_type), end.body_span)?,
            ),
            (FunctionExit::Explicit(None), ValueType::Void)
            | (FunctionExit::FellOffEnd, ValueType::Void) => None,
            (FunctionExit::FellOffEnd, ValueType::Integer(return_integer)) if end.is_main => {
                Some(CValue::Integer(CInteger::int(0).cast(return_integer)))
            }
            (
                FunctionExit::Explicit(None),
                ValueType::Integer(_)
                | ValueType::Floating(_)
                | ValueType::Pointer(_)
                | ValueType::Aggregate(_),
            ) => {
                return Err(InterpreterError::UnsupportedStatement {
                    feature: "empty return in scalar function",
                    span: end.body_span,
                });
            }
            (FunctionExit::Explicit(Some(_)), ValueType::Void) => {
                return Err(InterpreterError::UnsupportedStatement {
                    feature: "value return in void function",
                    span: end.body_span,
                });
            }
            (
                FunctionExit::FellOffEnd,
                ValueType::Integer(_)
                | ValueType::Floating(_)
                | ValueType::Pointer(_)
                | ValueType::Aggregate(_),
            ) => Some(CValue::Uninitialized(end.return_value_type)),
        };

        if self.memory_mode {
            if let Some(CValue::Aggregate {
                address: source,
                ty,
                qualifiers,
            }) = return_value
            {
                let caller = end.caller_memory_frame.as_ref().ok_or(
                    InterpreterError::UnsupportedFunctionSignature {
                        symbol: end.symbol,
                        span: end.body_span,
                    },
                )?;
                let layout = self.access_layout(ty, end.body_span)?;
                let (memory, destination) = self
                    .memory
                    .allocate_automatic(
                        caller.id,
                        caller.current_scope,
                        self.temporary_origin_site,
                        layout,
                    )
                    .map_err(|error| self.memory_error(error, end.body_span))?;
                self.memory = memory;
                return_value = Some(self.store_memory_value(
                    destination,
                    ty,
                    Qualifiers::default(),
                    CValue::Aggregate {
                        address: source,
                        ty,
                        qualifiers,
                    },
                    end.body_span,
                )?);
            }
            self.run_scope_cleanups(None)?;
            let frame = self
                .memory_frame
                .take()
                .ok_or(InterpreterError::UnsupportedStatement {
                    feature: "missing interpreter memory frame at function exit",
                    span: end.body_span,
                })?;
            self.memory = self
                .memory
                .end_frame(frame.id)
                .map_err(|error| self.memory_error(error, end.body_span))?;
            self.memory_frame = end.caller_memory_frame;
        }

        values.truncate(end.value_base);
        if let Some(caller_slots) = end.caller_slots {
            self.slots = caller_slots;
            self.fuel_anchor = end.caller_fuel_anchor;
            match (end.caller_expects_value, return_value) {
                (true, Some(value)) => values.push(value),
                (false, None) => {}
                _ => {
                    return Err(InterpreterError::UnsupportedFunctionSignature {
                        symbol: end.symbol,
                        span: end.body_span,
                    });
                }
            }
            Ok(None)
        } else {
            let return_value = match return_value {
                Some(CValue::Uninitialized(_)) => {
                    return Err(InterpreterError::UninitializedValue {
                        span: end.body_span,
                    });
                }
                Some(CValue::Integer(value)) => Some(ScalarValue {
                    ty: end.return_type,
                    raw_bits: value.raw_bits(),
                }),
                Some(CValue::Floating(value)) => Some(ScalarValue {
                    ty: end.return_type,
                    raw_bits: value.bits(),
                }),
                Some(CValue::Pointer { .. } | CValue::FunctionPointer { .. }) => {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "pointer return from scalar interpreter root",
                        span: end.body_span,
                    });
                }
                Some(CValue::Aggregate { .. }) => {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "aggregate return from scalar interpreter root",
                        span: end.body_span,
                    });
                }
                None => None,
            };
            Ok(Some(MachineOutcome { return_value }))
        }
    }

    fn tick(&mut self, span: SourceSpan) -> Result<(), InterpreterError> {
        if self.consumed >= self.fuel_limit {
            return Err(InterpreterError::FuelExhausted {
                limit: self.fuel_limit,
                consumed: self.consumed,
                span: self.fuel_anchor.unwrap_or(span),
            });
        }
        self.consumed += 1;
        Ok(())
    }

    fn integer_type(&self, ty: TypeId, span: SourceSpan) -> Result<CIntegerType, InterpreterError> {
        if let Some(TypeKind::BuiltinVaList) =
            self.unit.types().get(ty).map(|canonical| &canonical.kind)
        {
            return Err(InterpreterError::VarArgs { span });
        }
        canonical_integer_type(self.unit.types(), ty)
            .ok_or(InterpreterError::UnsupportedType { ty, span })
    }

    fn value_type(&self, ty: TypeId, span: SourceSpan) -> Result<ValueType, InterpreterError> {
        match self.unit.types().get(ty).map(|canonical| &canonical.kind) {
            Some(TypeKind::Void) => Ok(ValueType::Void),
            Some(TypeKind::Float(
                kind @ (evocc_types::FloatKind::F32
                | evocc_types::FloatKind::F64
                | evocc_types::FloatKind::F128),
            )) => Ok(ValueType::Floating(*kind)),
            Some(
                TypeKind::Struct(_)
                | TypeKind::Union(_)
                | TypeKind::BuiltinVaList
                | TypeKind::Complex(evocc_types::ComplexBase::Integer(_))
                | TypeKind::Vector { .. },
            ) => Ok(ValueType::Aggregate(ty)),
            Some(TypeKind::Pointer(_)) => {
                PointerTypeDesc::from_canonical_pointer(self.unit.types(), ty)
                    .map(ValueType::Pointer)
                    .map_err(|_| InterpreterError::UnsupportedType { ty, span })
            }
            _ => self.integer_type(ty, span).map(ValueType::Integer),
        }
    }

    fn integer_error(&self, error: IntegerError, span: SourceSpan) -> InterpreterError {
        InterpreterError::Integer { error, span }
    }

    fn memory_error(&self, error: MemoryFault, span: SourceSpan) -> InterpreterError {
        InterpreterError::Memory { error, span }
    }

    fn pointer_load_error(
        error: MemoryFault,
        address: Pointer,
        access: AccessLayout,
        span: SourceSpan,
    ) -> InterpreterError {
        let error = if matches!(error, MemoryFault::UninitializedRead { .. }) {
            MemoryFault::InvalidPointerRepresentation { address, access }
        } else {
            error
        };
        InterpreterError::Memory { error, span }
    }

    fn access_layout(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<AccessLayout, InterpreterError> {
        self.target
            .layout_of(self.unit.types(), ty)
            .map(AccessLayout::from_type_layout)
            .map_err(|_| InterpreterError::UnsupportedType { ty, span })
    }

    fn object_access_layout(
        &self,
        ty: TypeId,
        initializer: Option<&crate::ObjectInitializer<'arena>>,
        span: SourceSpan,
    ) -> Result<AccessLayout, InterpreterError> {
        let Some(initializer) = initializer else {
            if self
                .target
                .array_size_factors(self.unit.types(), ty)
                .is_ok_and(|(_, captures)| !captures.is_empty())
            {
                let size = self.pointer_arithmetic_scale(ty, Pointer::NULL, span)?;
                let align = self
                    .target
                    .alignof_type(self.unit.types(), ty)
                    .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
                return Ok(AccessLayout::new(size, align));
            }
            return self.access_layout(ty, span);
        };
        if initializer.plan().target_type() != ty {
            return Err(InterpreterError::UnsupportedType { ty, span });
        }
        initializer
            .plan()
            .storage_layout(self.unit.types(), &self.target)
            .map(AccessLayout::from_type_layout)
            .map_err(|_| InterpreterError::UnsupportedType { ty, span })
    }

    fn memory_binding(&self, symbol: SymbolId) -> Option<MemoryBinding> {
        let symbol = self.linkage.target(symbol);
        self.memory_frame
            .as_ref()
            .and_then(|frame| frame.bindings.get(&symbol).copied())
            .or_else(|| self.static_bindings.get(&symbol).copied())
    }

    fn require_memory_binding(
        &self,
        symbol: SymbolId,
        span: SourceSpan,
    ) -> Result<MemoryBinding, InterpreterError> {
        self.memory_binding(symbol)
            .ok_or(InterpreterError::OutOfScopeObject {
                symbol,
                spelling: self.unit.symbols().symbol(symbol).map_or_else(
                    |_| format!("symbol#{}", symbol.raw()),
                    |symbol| symbol.spelling().to_owned(),
                ),
                span,
            })
    }

    fn enter_memory_scope(
        &mut self,
        span: SourceSpan,
    ) -> Result<Option<MemoryScopeExit>, InterpreterError> {
        if !self.memory_mode {
            return Ok(None);
        }
        let frame = self
            .memory_frame
            .as_ref()
            .ok_or(InterpreterError::UnsupportedStatement {
                feature: "lexical scope without an interpreter memory frame",
                span,
            })?;
        let parent = frame.current_scope;
        let bindings = frame.bindings.clone();
        let (memory, scope) = self
            .memory
            .begin_scope(parent)
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        self.memory_frame
            .as_mut()
            .expect("memory frame checked immediately before scope creation")
            .current_scope = scope;
        Ok(Some(MemoryScopeExit {
            scope,
            parent,
            bindings,
            span,
        }))
    }

    fn exit_memory_scope(&mut self, exit: MemoryScopeExit) -> Result<(), InterpreterError> {
        let frame = self
            .memory_frame
            .as_ref()
            .ok_or(InterpreterError::UnsupportedStatement {
                feature: "lexical scope exit without an interpreter memory frame",
                span: exit.span,
            })?;
        if frame.current_scope != exit.scope {
            return Err(InterpreterError::UnsupportedStatement {
                feature: "non-LIFO interpreter memory scope exit",
                span: exit.span,
            });
        }
        self.run_scope_cleanups(Some(exit.scope))?;
        self.memory = self
            .memory
            .end_scope(exit.scope)
            .map_err(|error| self.memory_error(error, exit.span))?;
        let frame = self
            .memory_frame
            .as_mut()
            .expect("memory frame checked immediately before scope exit");
        frame.current_scope = exit.parent;
        frame.bindings = exit.bindings;
        frame
            .compound_bindings
            .retain(|(scope, _), _| *scope != exit.scope);
        Ok(())
    }

    fn run_scope_cleanups(&mut self, scope: Option<MemoryScopeId>) -> Result<(), InterpreterError> {
        let frame = self
            .memory_frame
            .as_mut()
            .expect("cleanup has a live frame");
        let mut cleanups = Vec::new();
        frame.cleanups.retain(|(owner, expression)| {
            if scope.is_none_or(|scope| scope == *owner) {
                cleanups.push(*expression);
                false
            } else {
                true
            }
        });
        for expression in cleanups.into_iter().rev() {
            let mut tasks = Vec::new();
            self.schedule_discarded_expression(expression, &mut tasks)?;
            self.run_tasks(tasks)?;
        }
        Ok(())
    }

    fn validate_memory_object_declaration(
        &self,
        declaration: &ObjectDecl<'arena>,
    ) -> Result<(), InterpreterError> {
        let symbol = self
            .unit
            .symbols()
            .symbol(declaration.symbol)
            .map_err(|_| InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            })?;
        if symbol.kind() != SymbolKind::Object
            || symbol.spelling() != declaration.name.spelling
            || !self.unit.types().compatible(symbol.ty(), declaration.ty)
            || !(symbol.storage_class() == Some(declaration.storage)
                || (declaration.storage.base() == StorageClass::Extern
                    && matches!(
                        symbol.storage_class().map(StorageClass::base),
                        Some(StorageClass::None | StorageClass::Static)
                    )))
            || !matches!(
                declaration.storage.base(),
                StorageClass::None
                    | StorageClass::Auto
                    | StorageClass::Register
                    | StorageClass::Static
                    | StorageClass::Extern
            )
        {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        }
        let canonical =
            self.unit
                .types()
                .get(declaration.ty)
                .ok_or(InterpreterError::UnsupportedType {
                    ty: declaration.ty,
                    span: declaration.span,
                })?;
        // Object storage includes nominal target scalar types as well as
        // ordinary scalars and aggregates. Execution checks their value kind
        // separately; an object declaration is not an arithmetic operation.
        if canonical.qualifiers.is_atomic
            || !matches!(
                canonical.kind,
                TypeKind::Int(_)
                    | TypeKind::GnuInt128(_)
                    | TypeKind::Enum(_)
                    | TypeKind::Pointer(_)
                    | TypeKind::Array { .. }
                    | TypeKind::Vector { .. }
                    | TypeKind::Struct(_)
                    | TypeKind::Union(_)
                    | TypeKind::BuiltinVaList
                    | TypeKind::Float(_)
                    | TypeKind::Complex(_)
                    | TypeKind::TargetBuiltin(_)
            )
        {
            return Err(InterpreterError::UnsupportedType {
                ty: declaration.ty,
                span: declaration.span,
            });
        }
        // A declaration does not allocate an object: an earlier extern array
        // may still be incomplete even when its later definition is complete.
        if declaration.storage.base() != StorageClass::Extern {
            self.object_access_layout(
                declaration.ty,
                declaration.initializer.as_ref(),
                declaration.span,
            )?;
        }
        Ok(())
    }

    fn allocate_automatic_object(
        &mut self,
        symbol: SymbolId,
        ty: TypeId,
        initializer: Option<&crate::ObjectInitializer<'arena>>,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let site = self
            .manifest
            .as_ref()
            .ok_or(InterpreterError::MissingAllocationManifest)?
            .object_site(symbol)
            .ok_or(InterpreterError::MalformedObjectDeclaration { symbol, span })?
            .ordinal()
            .raw();
        let mut layout = self.object_access_layout(ty, initializer, span)?;
        layout.align_bytes = layout.align_bytes.max(
            self.unit
                .symbols()
                .symbol(symbol)
                .map_err(|_| InterpreterError::MalformedObjectDeclaration { symbol, span })?
                .declared_alignment(),
        );
        let (frame, scope) = self
            .memory_frame
            .as_ref()
            .map(|frame| (frame.id, frame.current_scope))
            .ok_or(InterpreterError::UnsupportedStatement {
                feature: "automatic object without an interpreter memory frame",
                span,
            })?;
        let (memory, address) = self
            .memory
            .allocate_automatic(frame, scope, site, layout)
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        let bindings = &mut self
            .memory_frame
            .as_mut()
            .ok_or(InterpreterError::UnsupportedStatement {
                feature: "automatic object without an interpreter memory frame",
                span,
            })?
            .bindings;
        // Re-entering a scope allocates a fresh object at the same site, so a
        // previous binding is replaced rather than rejected: the memory model
        // has already issued a new object identity and left the old one as a
        // tombstone, which is what makes a stale pointer stay dangling instead
        // of silently addressing the new object. Two declarations of one symbol
        // within a single scope activation are handled by reusing the binding:
        // a backward jump can execute one declaration's initializer again.
        bindings.insert(symbol, MemoryBinding { address, ty });
        Ok(())
    }

    fn zero_constructor_padding(
        &mut self,
        binding: MemoryBinding,
        initializer: &crate::ObjectInitializer<'arena>,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let ranges = initializer
            .plan()
            .padding_ranges(self.unit.types(), self.target)
            .map_err(|_| InterpreterError::UnsupportedType {
                ty: binding.ty,
                span,
            })?;
        for range in ranges {
            let at = binding
                .address
                .checked_offset(range.start as i64)
                .map_err(|error| self.memory_error(error, span))?;
            let bytes = ByteSequence::try_from_bytes(vec![0; (range.end - range.start) as usize])
                .map_err(|error| self.memory_error(error, span))?;
            self.memory = self
                .memory
                .store_bytes(at, AccessLayout::new(range.end - range.start, 1), &bytes)
                .map_err(|error| self.memory_error(error, span))?;
        }
        Ok(())
    }

    fn zero_initialize_subobjects(
        &mut self,
        address: Pointer,
        ty: TypeId,
        path: SubobjectPath,
        actions: &[InitAction<'arena>],
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        if actions
            .iter()
            .any(|action| path.steps().starts_with(action.path().steps()))
        {
            return Ok(());
        }
        let kind = self
            .unit
            .types()
            .get(ty)
            .ok_or(InterpreterError::UnsupportedType { ty, span })?
            .kind
            .clone();
        match kind {
            TypeKind::Float(
                evocc_types::FloatKind::F32
                | evocc_types::FloatKind::F64
                | evocc_types::FloatKind::F128,
            ) => self.store_constant_representation(
                address,
                ty,
                CInteger::new(0, CIntegerType::ULONG),
                false,
                span,
            ),
            TypeKind::Int(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::TargetBuiltin(_) => {
                let value = self.zero_value(ty, span)?;
                self.store_initialized_value(address, ty, value, span)?;
                Ok(())
            }
            TypeKind::BuiltinVaList => Ok(()),
            TypeKind::Vector { element, lanes, .. } => {
                let layout = self.access_layout(element, span)?;
                for index in 0..u64::from(lanes) {
                    let at = address
                        .checked_offset((index * layout.size_bytes) as i64)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.zero_initialize_subobjects(
                        at,
                        element,
                        path.clone().with_index(index),
                        actions,
                        span,
                    )?;
                }
                Ok(())
            }
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(count),
            } => {
                let element_layout = self.object_access_layout(element, None, span)?;
                for index in 0..count {
                    let offset = index
                        .checked_mul(element_layout.size_bytes)
                        .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                    let offset = i64::try_from(offset)
                        .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
                    let element_address = address
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.zero_initialize_subobjects(
                        element_address,
                        element,
                        path.clone().with_index(index),
                        actions,
                        span,
                    )?;
                }
                Ok(())
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Ok(()),
            TypeKind::Array {
                element,
                bound: ArrayBound::Captured(_),
            } => {
                let layout = self.object_access_layout(ty, None, span)?;
                let element_layout = self.object_access_layout(element, None, span)?;
                if element_layout.size_bytes == 0 {
                    return Ok(());
                }
                for index in 0..layout.size_bytes / element_layout.size_bytes {
                    let at = address
                        .checked_offset((index * element_layout.size_bytes) as i64)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.zero_initialize_subobjects(
                        at,
                        element,
                        path.clone().with_index(index),
                        actions,
                        span,
                    )?;
                }
                Ok(())
            }
            TypeKind::Array {
                bound: ArrayBound::Variable,
                ..
            }
            | TypeKind::Void
            | TypeKind::Function { .. } => Err(InterpreterError::UnsupportedType { ty, span }),
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                let is_union = matches!(kind, TypeKind::Union(_));
                let definition = self
                    .unit
                    .types()
                    .tag(tag)
                    .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                let fields = match &definition.state {
                    TagState::Complete(
                        TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                    ) => fields.clone(),
                    _ => return Err(InterpreterError::UnsupportedType { ty, span }),
                };
                let layout = self
                    .target
                    .record_layout(self.unit.types(), tag)
                    .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
                let selected_union_field = is_union.then(|| {
                    actions.iter().find_map(|action| {
                        let suffix = action.path().steps().strip_prefix(path.steps())?;
                        match suffix.first()? {
                            SubobjectStep::Field { field_index } => Some(*field_index),
                            SubobjectStep::Index { .. } => None,
                        }
                    })
                });
                for (field_index, field) in fields.iter().enumerate() {
                    if selected_union_field
                        .flatten()
                        .is_some_and(|selected| selected != field_index)
                    {
                        continue;
                    }
                    if field.name.is_none() && field.bit_field.is_some() {
                        continue;
                    }
                    let field_layout = layout
                        .fields
                        .iter()
                        .find(|layout| layout.field_index == field_index)
                        .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                    if let Some(field) = field_layout.bit_field {
                        let count = (field.bit_offset % 8 + field.width_bits).div_ceil(8);
                        for byte in 0..count {
                            let at = address
                                .checked_offset((field_layout.byte_offset + u64::from(byte)) as i64)
                                .map_err(|error| self.memory_error(error, span))?;
                            self.memory = self
                                .memory
                                .store_integer(at, AccessLayout::new(1, 1), 0)
                                .map_err(|error| self.memory_error(error, span))?;
                        }
                        continue;
                    }
                    let offset = i64::try_from(field_layout.byte_offset)
                        .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
                    let field_address = address
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.zero_initialize_subobjects(
                        field_address,
                        field.ty,
                        path.clone().with_field(field_index),
                        actions,
                        span,
                    )?;
                    if is_union {
                        break;
                    }
                }
                Ok(())
            }
        }
    }

    fn string_initializer_bytes(
        &self,
        destination_type: TypeId,
        literal: &StringLit,
        span: SourceSpan,
    ) -> Result<(AccessLayout, ByteSequence), InterpreterError> {
        let Some(TypeKind::Array { element, bound }) = self
            .unit
            .types()
            .get(destination_type)
            .map(|canonical| &canonical.kind)
        else {
            return Err(InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            });
        };
        let count =
            literal
                .initializer_element_count(bound)
                .ok_or(InterpreterError::UnsupportedType {
                    ty: destination_type,
                    span,
                })?;
        let element = self.access_layout(*element, span)?;
        let destination = if *bound == ArrayBound::Unbounded {
            AccessLayout::new(
                element
                    .size_bytes
                    .checked_mul(count)
                    .ok_or(InterpreterError::UnsupportedType {
                        ty: destination_type,
                        span,
                    })?,
                element.align_bytes,
            )
        } else {
            self.access_layout(destination_type, span)?
        };
        let width =
            usize::try_from(element.size_bytes).map_err(|_| InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            })?;
        let count = usize::try_from(count).map_err(|_| InterpreterError::UnsupportedType {
            ty: destination_type,
            span,
        })?;
        if width == 0
            || width > 4
            || count.checked_mul(width) != usize::try_from(destination.size_bytes).ok()
        {
            return Err(InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            });
        }
        let mut units = literal
            .decoded_units()
            .ok_or(InterpreterError::UnsupportedExpression {
                feature: "invalid string initializer",
                span,
            })?;
        if units.len() > count {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "string initializer exceeds destination",
                span,
            });
        }
        units.resize(count, 0);
        let mut bytes = Vec::with_capacity(count.saturating_mul(width));
        for unit in units {
            let encoded = match self.target.byte_order() {
                evocc_types::ByteOrder::Little => unit.to_le_bytes(),
                evocc_types::ByteOrder::Big => unit.to_be_bytes(),
            };
            match self.target.byte_order() {
                evocc_types::ByteOrder::Little => bytes.extend_from_slice(&encoded[..width]),
                evocc_types::ByteOrder::Big => bytes.extend_from_slice(&encoded[4 - width..]),
            }
        }
        let value =
            ByteSequence::try_from_bytes(bytes).map_err(|error| self.memory_error(error, span))?;
        Ok((destination, value))
    }

    fn zero_value(&mut self, ty: TypeId, span: SourceSpan) -> Result<CValue, InterpreterError> {
        if crate::complex::rank(self.unit.types(), ty).is_some() {
            let address = self.vector_temporary(ty, span)?;
            let layout = self.access_layout(ty, span)?;
            self.memory = self
                .memory
                .store_integer(address, layout, 0)
                .map_err(|e| self.memory_error(e, span))?;
            return Ok(CValue::Aggregate {
                address,
                ty,
                qualifiers: Qualifiers::default(),
            });
        }
        match self.value_type(ty, span)? {
            ValueType::Integer(integer) => Ok(CValue::Integer(CInteger::new(0, integer))),
            ValueType::Floating(kind) => Ok(CValue::Floating(
                evocc_types::floating::FloatBits::from_bits(kind, 0).unwrap(),
            )),
            ValueType::Pointer(pointer)
                if matches!(
                    self.unit.types().get(pointer.pointee()).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) =>
            {
                Ok(CValue::FunctionPointer {
                    value: PointerValue::Null,
                    ty: pointer,
                })
            }
            ValueType::Pointer(pointer) => Ok(CValue::Pointer {
                value: Pointer::NULL,
                ty: pointer,
            }),
            ValueType::Void | ValueType::Aggregate(_) => {
                Err(InterpreterError::UnsupportedType { ty, span })
            }
        }
    }

    fn load_identifier(
        &mut self,
        symbol: SymbolId,
        spelling: &str,
        facts: ExprFacts,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let declaration = self.unit.symbols().symbol(symbol).map_err(|_| {
            InterpreterError::UnsupportedExpression {
                feature: "unknown identifier",
                span,
            }
        })?;
        if declaration.spelling() != spelling
            || !crate::type_at_use_matches_binding(self.unit.types(), facts.ty(), declaration.ty())
        {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "inconsistent identifier facts",
                span,
            });
        }
        if is_global_register(self.unit, symbol) {
            return Err(InterpreterError::Asm { span });
        }
        if let Some(value) = declaration.enumerator_value() {
            let ty = self.integer_type(declaration.ty(), span)?;
            return Ok(CValue::Integer(CInteger::new(value as u64, ty)));
        }
        if declaration.kind() == SymbolKind::Function
            && facts.category() == ExprCategory::FunctionDesignator
            && matches!(
                self.unit.types().get(facts.ty()).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. })
            )
        {
            let identity =
                self.function_identity(symbol)
                    .ok_or(InterpreterError::UnsupportedExpression {
                        feature: "function designator has no definition identity",
                        span,
                    })?;
            let ty = PointerTypeDesc::to_object(self.unit.types(), facts.ty()).ok_or(
                InterpreterError::UnsupportedType {
                    ty: facts.ty(),
                    span,
                },
            )?;
            return Ok(CValue::FunctionPointer {
                value: PointerValue::function(identity)
                    .map_err(|error| self.memory_error(error, span))?,
                ty,
            });
        }
        // C11 6.3.2.1p3: an array lvalue used as a value becomes a pointer to
        // its first element. The array itself is never loaded, so this is the
        // only way an array-typed identifier can produce one.
        if let Some(TypeKind::Array { element, .. }) = self
            .unit
            .types()
            .get(declaration.ty())
            .map(|canonical| &canonical.kind)
        {
            let element = *element;
            let binding = self.require_memory_binding(symbol, span)?;
            let ty = PointerTypeDesc::to_object(self.unit.types(), element)
                .ok_or(InterpreterError::UnsupportedType { ty: element, span })?;
            return Ok(CValue::Pointer {
                value: binding.address,
                ty,
            });
        }
        if !self.memory_mode {
            let slot = self
                .slots
                .get(&symbol)
                .ok_or(InterpreterError::UnsupportedExpression {
                    feature: "identifier without automatic scalar slot",
                    span,
                })?;
            if !slot.active {
                return Err(InterpreterError::OutOfScopeObject {
                    symbol,
                    spelling: spelling.to_owned(),
                    span,
                });
            }
            return slot.value.map(CValue::Integer).ok_or_else(|| {
                InterpreterError::ReadOfUninitializedObject {
                    symbol,
                    spelling: spelling.to_owned(),
                    span,
                }
            });
        }
        let binding = self.require_memory_binding(symbol, span)?;
        let qualifiers = self
            .unit
            .types()
            .get(binding.ty)
            .ok_or(InterpreterError::UnsupportedType {
                ty: binding.ty,
                span,
            })?
            .qualifiers;
        self.load_memory_value(binding.address, binding.ty, qualifiers, span)
    }

    fn address_of_expression(
        &mut self,
        expression: &CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        if matches!(expression, CHirExpr::StringLit(..)) {
            return self.string_pointer(expression, false);
        }
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span: operand_span,
        } = expression
        else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "address-of outside direct object subset",
                span,
            });
        };
        let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
            InterpreterError::UnsupportedExpression {
                feature: "unknown address-of object",
                span: *operand_span,
            }
        })?;
        if declaration.kind() == SymbolKind::Function {
            return self.load_identifier(*symbol, spelling, *facts, *operand_span);
        }
        if declaration.spelling() != spelling
            || declaration.ty() != facts.ty()
            || facts.category() != ExprCategory::LValue
            || !facts.is_addressable()
        {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "inconsistent address-of object",
                span: *operand_span,
            });
        }
        let place = MemoryPlace::derive(
            self.unit.symbols(),
            self.unit.types(),
            PlaceRoot::Object { symbol: *symbol },
            Vec::new(),
            facts.ty(),
        )
        .map_err(|_| InterpreterError::UnsupportedExpression {
            feature: "invalid address-of place",
            span: *operand_span,
        })?;
        let binding = self.require_memory_binding(*symbol, *operand_span)?;
        Ok(CValue::Pointer {
            value: binding.address,
            ty: PointerTypeDesc::from_addressed_place(&place),
        })
    }

    fn string_pointer(
        &self,
        expression: &CHirExpr<'arena>,
        decay: bool,
    ) -> Result<CValue, InterpreterError> {
        let unsupported = || InterpreterError::UnsupportedExpression {
            feature: "string has no allocation site or complete array type",
            span: expression.span(),
        };
        let CHirExpr::StringLit(literal, _) = expression else {
            return Err(unsupported());
        };
        let value = *self
            .string_bindings
            .get(&(expression as *const CHirExpr<'_> as usize))
            .ok_or_else(unsupported)?;
        let ty = if decay {
            PointerTypeDesc::from_expression(self.unit.symbols(), self.unit.types(), expression)
                .map_err(|_| unsupported())?
        } else {
            let array = literal
                .array_type(self.unit.types())
                .ok_or_else(unsupported)?;
            PointerTypeDesc::to_object(self.unit.types(), array).ok_or_else(unsupported)?
        };
        Ok(CValue::Pointer { value, ty })
    }

    fn pointer_type_of_expression(
        &self,
        expression: &CHirExpr<'arena>,
    ) -> Result<PointerTypeDesc, InterpreterError> {
        let span = expression.span();
        PointerTypeDesc::from_expression(self.unit.symbols(), self.unit.types(), expression)
            .map_err(|_| InterpreterError::UnsupportedExpression {
                feature: "invalid pointer expression type",
                span,
            })
    }

    fn expression_value_type(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<ValueType, InterpreterError> {
        if let Some(ty) =
            crate::vector::expression_type(self.unit.types(), expression).filter(|ty| {
                crate::vector::shape(self.unit.types(), *ty).is_some()
                    || crate::complex::rank(self.unit.types(), *ty).is_some()
            })
        {
            return self.value_type(ty, expression.span());
        }
        if self.static_integer_evaluation {
            return self.infer_integer_type(expression).map(ValueType::Integer);
        }
        if let Some(kind) = crate::constant::floating_expression_kind(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) {
            return Ok(ValueType::Floating(kind));
        }
        if let Ok(ty) =
            crate::place::record_expression_type(self.unit.symbols(), self.unit.types(), expression)
        {
            return self.value_type(ty, expression.span());
        }
        if let Ok(pointer) =
            PointerTypeDesc::from_expression(self.unit.symbols(), self.unit.types(), expression)
        {
            return Ok(ValueType::Pointer(pointer));
        }
        self.infer_integer_type(expression).map(ValueType::Integer)
    }

    /// Recheck a place against the tables of the unit being executed, and
    /// confirm the facts recorded at the use site still describe it.
    ///
    /// A place is checked data supplied by the parser, so this guards against a
    /// place transplanted from an unrelated unit whose numeric identities
    /// happen to overlap, not merely against parser bugs.
    fn validate_readable_place(
        &self,
        place: &MemoryPlace<'arena>,
        facts: ExprFacts,
        target_span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        place
            .validate_against(self.unit.symbols(), self.unit.types())
            .map_err(|_| InterpreterError::UnsupportedExpression {
                feature: "invalid typed memory place",
                span: target_span,
            })?;
        if facts.ty() != place.value_ty() || facts.category() != ExprCategory::Value {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "memory read disagrees with its place",
                span: target_span,
            });
        }
        Ok(())
    }

    /// Recheck a place that is about to be written, with the extra conditions
    /// a store requires beyond a load.
    fn validate_mutable_place(
        &self,
        place: &MemoryPlace<'arena>,
        facts: ExprFacts,
        target_span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        self.validate_readable_place(place, facts, target_span)?;
        if !place.is_modifiable() {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "store to an unmodifiable place",
                span: target_span,
            });
        }
        Ok(())
    }

    /// Schedule evaluation of a projected place: its root address first, then
    /// each subscript expression in source order.
    ///
    /// Returns the number of subscript values the completion task must pop.
    /// Nothing here evaluates a subexpression more than once, which is why the
    /// designating syntax is not retained alongside the place.
    fn schedule_place_operands(
        &mut self,
        place: &MemoryPlace<'arena>,
        target_span: SourceSpan,
        tasks: &mut Vec<MachineTask<'arena>>,
        values: &mut Vec<CValue>,
    ) -> Result<usize, InterpreterError> {
        let subscripts: Vec<&'arena CHirExpr<'arena>> = place
            .projections()
            .iter()
            .filter_map(PlaceProjection::source_expression)
            .collect();
        // The task stack is last-in-first-out, so pushing in reverse source
        // order makes the machine evaluate the root first and then each
        // subscript left to right.
        for subscript in subscripts.iter().rev() {
            tasks.push(MachineTask::EvalExpression {
                expression: subscript,
                allow_root_mutation: false,
            });
        }
        match place.root() {
            PlaceRoot::PointerExpression { pointer, .. } => {
                tasks.push(MachineTask::EvalExpression {
                    expression: pointer,
                    allow_root_mutation: true,
                });
            }
            PlaceRoot::Object { symbol } => {
                let binding = self.require_memory_binding(*symbol, target_span)?;
                values.push(CValue::Pointer {
                    value: binding.address,
                    ty: self.root_pointer_type(place, target_span)?,
                });
            }
            PlaceRoot::CompoundLiteral { expression, .. } => {
                self.schedule_compound_literal(expression, true, tasks)?;
            }
        }
        Ok(subscripts.len())
    }

    fn schedule_compound_literal(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        as_address: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let CHirExpr::CompoundLit(ty, initializer, span) = expression else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "compound-literal allocation requested for another expression",
                span: expression.span(),
            });
        };
        if initializer.plan().target_type() != *ty
            || initializer.plan().validate(self.unit.types()).is_err()
        {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "invalid compound-literal initializer plan",
                span: *span,
            });
        }
        let site = self
            .manifest
            .as_ref()
            .ok_or(InterpreterError::MissingAllocationManifest)?
            .compound_literal_site(expression)
            .ok_or(InterpreterError::UnsupportedExpression {
                feature: "compound literal has no allocation site",
                span: *span,
            })?;
        let static_storage = matches!(
            site.kind(),
            AllocationSiteKind::CompoundLiteral {
                static_storage: true,
                ..
            }
        );
        let identity = expression as *const CHirExpr<'arena> as usize;
        let binding = if static_storage {
            self.static_compound_bindings
                .get(&identity)
                .copied()
                .ok_or(InterpreterError::UnsupportedExpression {
                    feature: "static compound literal was not initialized",
                    span: *span,
                })?
        } else {
            let layout = self.access_layout(*ty, *span)?;
            let (frame, scope) = self
                .memory_frame
                .as_ref()
                .map(|frame| (frame.id, frame.current_scope))
                .ok_or(InterpreterError::UnsupportedExpression {
                    feature: "automatic compound literal has no active scope",
                    span: *span,
                })?;
            if let Some(binding) = self
                .memory_frame
                .as_ref()
                .and_then(|frame| frame.compound_bindings.get(&(scope, identity)))
                .copied()
            {
                binding
            } else {
                let (memory, address) = self
                    .memory
                    .allocate_automatic(frame, scope, site.ordinal().raw(), layout)
                    .map_err(|error| self.memory_error(error, *span))?;
                self.memory = memory;
                let binding = MemoryBinding { address, ty: *ty };
                self.memory_frame
                    .as_mut()
                    .expect("active compound scope")
                    .compound_bindings
                    .insert((scope, identity), binding);
                binding
            }
        };
        if !static_storage && self.unit.zero_initialize_compound_padding(initializer) {
            self.zero_constructor_padding(binding, initializer, *span)?;
        }
        if !static_storage && initializer.plan().zero_fills_unwritten() {
            self.zero_initialize_subobjects(
                binding.address,
                *ty,
                SubobjectPath::root(),
                initializer.plan().actions(),
                *span,
            )?;
        }
        tasks.push(MachineTask::YieldCompoundLiteral {
            binding,
            as_address,
            span: *span,
        });
        if !static_storage {
            tasks.push(MachineTask::ContinueMemoryInitializer {
                target: InitializerTarget {
                    binding,
                    initializer,
                    symbol: None,
                    span: *span,
                },
                index: 0,
            });
        }
        Ok(())
    }

    /// Pointer type of a place's root, which is what address folding starts
    /// from and therefore what the value stack must be carrying.
    fn root_pointer_type(
        &self,
        place: &MemoryPlace<'_>,
        span: SourceSpan,
    ) -> Result<PointerTypeDesc, InterpreterError> {
        place
            .root_pointer_type(self.unit.symbols(), self.unit.types())
            .ok_or(InterpreterError::UnsupportedExpression {
                feature: "place root has no pointer type",
                span,
            })
    }

    /// Fold a root address and already-evaluated subscript values into the
    /// address of the designated subobject.
    ///
    /// Every step goes through the memory model's checked pointer arithmetic,
    /// so an out-of-range subscript or an overflowing offset becomes a typed
    /// diagnostic rather than a wrapped address.
    fn project_place_address(
        &self,
        root: Pointer,
        place: &MemoryPlace<'_>,
        subscripts: &[CInteger],
        span: SourceSpan,
    ) -> Result<Pointer, InterpreterError> {
        let mut address = root;
        let mut consumed = 0;
        for projection in place.projections() {
            address = match projection {
                PlaceProjection::Index(projection) => {
                    let subscript = subscripts.get(consumed).copied().ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "missing subscript value for projected place",
                            span,
                        },
                    )?;
                    consumed += 1;
                    let scale =
                        self.pointer_arithmetic_scale(projection.element_ty(), address, span)?;
                    let signedness = if subscript.ty().is_signed() {
                        IndexSignedness::Signed
                    } else {
                        IndexSignedness::Unsigned
                    };
                    address
                        .checked_scaled_offset(
                            if scale == 0 { 0 } else { subscript.raw_bits() },
                            u16::from(subscript.ty().bits()),
                            signedness,
                            OffsetDirection::Add,
                            scale.max(1),
                        )
                        .map_err(|error| self.memory_error(error, span))?
                }
                PlaceProjection::Field(projection) => {
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), projection.tag())
                        .map_err(|_| InterpreterError::UnsupportedType {
                            ty: projection.field_ty(),
                            span,
                        })?;
                    let field = layout
                        .fields
                        .iter()
                        .find(|field| field.field_index == projection.field_index())
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: projection.field_ty(),
                            span,
                        })?;
                    let offset = i64::try_from(field.byte_offset).map_err(|_| {
                        InterpreterError::UnsupportedType {
                            ty: projection.field_ty(),
                            span,
                        }
                    })?;
                    address
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?
                }
            };
        }
        if consumed != subscripts.len() {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "unconsumed subscript value at projected place",
                span,
            });
        }
        Ok(address)
    }

    /// Pop a projected place's operands and resolve its final address.
    ///
    /// The value stack holds the root address followed by `subscript_count`
    /// subscript values in source order, so they are popped in reverse and
    /// restored to source order before folding.
    fn resolve_projected_address(
        &self,
        place: &MemoryPlace<'_>,
        subscript_count: usize,
        values: &mut Vec<CValue>,
        span: SourceSpan,
    ) -> Result<Pointer, InterpreterError> {
        let mut subscripts = Vec::with_capacity(subscript_count);
        for _ in 0..subscript_count {
            let value = pop_machine_value(values, span)?;
            subscripts.push(expect_machine_integer(value, span)?);
        }
        subscripts.reverse();
        let (root, actual) = expect_machine_pointer(pop_machine_value(values, span)?, span)?;
        let expected = self.root_pointer_type(place, span)?;
        if actual != expected {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "inconsistent pointer type at projected place",
                span,
            });
        }
        self.project_place_address(root, place, &subscripts, span)
    }

    fn aggregate_member_address(
        &self,
        aggregate: CValue,
        name: &str,
        span: SourceSpan,
    ) -> Result<(Pointer, TypeId, Qualifiers), InterpreterError> {
        let CValue::Aggregate {
            address,
            ty,
            qualifiers,
        } = aggregate
        else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "member base is not an aggregate value",
                span,
            });
        };
        self.aggregate_member_address_in(address, ty, qualifiers, name, span)
    }

    fn aggregate_member_address_in(
        &self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        name: &str,
        span: SourceSpan,
    ) -> Result<(Pointer, TypeId, Qualifiers), InterpreterError> {
        let tag = match self.unit.types().get(ty).map(|ty| &ty.kind) {
            Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => *tag,
            _ => return Err(InterpreterError::UnsupportedType { ty, span }),
        };
        let fields = match &self
            .unit
            .types()
            .tag(tag)
            .ok_or(InterpreterError::UnsupportedType { ty, span })?
            .state
        {
            TagState::Complete(TagDefinition::Struct(fields) | TagDefinition::Union(fields)) => {
                fields
            }
            _ => return Err(InterpreterError::UnsupportedType { ty, span }),
        };
        let record = self
            .target
            .record_layout(self.unit.types(), tag)
            .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
        for (field_index, field) in fields.iter().enumerate() {
            let layout = record
                .fields
                .iter()
                .find(|layout| layout.field_index == field_index)
                .ok_or(InterpreterError::UnsupportedType { ty: field.ty, span })?;
            if layout.bit_field.is_some() {
                continue;
            }
            let offset = i64::try_from(layout.byte_offset)
                .map_err(|_| InterpreterError::UnsupportedType { ty: field.ty, span })?;
            let field_address = address
                .checked_offset(offset)
                .map_err(|error| self.memory_error(error, span))?;
            let field_qualifiers = merge_memory_qualifiers(
                qualifiers,
                self.unit
                    .types()
                    .get(field.ty)
                    .ok_or(InterpreterError::UnsupportedType { ty: field.ty, span })?
                    .qualifiers,
            );
            if field.name.as_deref() == Some(name) {
                return Ok((field_address, field.ty, field_qualifiers));
            }
            if field.name.is_none()
                && matches!(
                    self.unit.types().get(field.ty).map(|ty| &ty.kind),
                    Some(TypeKind::Struct(_) | TypeKind::Union(_))
                )
            {
                if let Ok(found) = self.aggregate_member_address_in(
                    field_address,
                    field.ty,
                    field_qualifiers,
                    name,
                    span,
                ) {
                    return Ok(found);
                }
            }
        }
        Err(InterpreterError::UnsupportedExpression {
            feature: "unknown aggregate member",
            span,
        })
    }

    fn place_bit_field(
        &self,
        place: &MemoryPlace<'_>,
        span: SourceSpan,
    ) -> Result<Option<BitFieldLayout>, InterpreterError> {
        if !place.is_bit_field() {
            return Ok(None);
        }
        let Some(PlaceProjection::Field(field)) = place.projections().last() else {
            return Err(InterpreterError::UnsupportedExpression {
                span,
                feature: "bit-field place has no final field",
            });
        };
        let layout = self
            .target
            .record_layout(self.unit.types(), field.tag())
            .map_err(|_| InterpreterError::UnsupportedType {
                ty: place.value_ty(),
                span,
            })?;
        layout
            .fields
            .iter()
            .find(|entry| entry.field_index == field.field_index())
            .and_then(|entry| entry.bit_field)
            .map(Some)
            .ok_or(InterpreterError::UnsupportedExpression {
                span,
                feature: "bit-field place has no target layout",
            })
    }

    fn normalize_bit_field(
        &self,
        value: CInteger,
        ty: TypeId,
        field: BitFieldLayout,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        let ValueType::Integer(integer) = self.value_type(ty, span)? else {
            return Err(InterpreterError::UnsupportedType { ty, span });
        };
        let mask = u128::MAX >> (128 - field.width_bits);
        let mut raw = value.raw_bits() & mask;
        if integer.is_signed() && raw & (1u128 << (field.width_bits - 1)) != 0 {
            raw |= !mask;
        }
        Ok(CInteger::from_raw(raw, integer))
    }

    fn load_bit_field(
        &mut self,
        address: Pointer,
        ty: TypeId,
        field: BitFieldLayout,
        volatile: bool,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let mut raw = 0u128;
        if volatile {
            for (offset, bytes) in field.storage_units() {
                let first = u32::from(field.bit_offset).max(offset * 8);
                let last = (u32::from(field.bit_offset) + u32::from(field.width_bits))
                    .min((offset + bytes) * 8);
                let count = last - first;
                let at = address
                    .checked_offset(i64::from(offset) - i64::from(field.bit_offset / 8))
                    .map_err(|error| self.memory_error(error, span))?;
                let (value, world) = self
                    .memory
                    .volatile_load_integer(&self.world, at, AccessLayout::new(u64::from(bytes), 1))
                    .map_err(|error| self.memory_error(error, span))?;
                self.world = world;
                let selected = (value >> (first - offset * 8)) & (u128::MAX >> (128 - count));
                raw |= selected << (first - u32::from(field.bit_offset));
            }
        } else {
            let mut consumed = 0u16;
            while consumed < field.width_bits {
                let position = field.bit_offset % 8 + consumed;
                let within = position % 8;
                let count = (8 - within).min(field.width_bits - consumed);
                let at = address
                    .checked_offset(i64::from(position / 8))
                    .map_err(|error| self.memory_error(error, span))?;
                let byte = self
                    .memory
                    .load_integer(at, AccessLayout::new(1, 1))
                    .map_err(|error| self.memory_error(error, span))?;
                raw |= ((byte >> within) & ((1u128 << count) - 1)) << consumed;
                consumed += count;
            }
        }
        let result = self.normalize_bit_field(
            CInteger::from_raw(raw, CIntegerType::UINT128),
            ty,
            field,
            span,
        )?;
        Ok(CValue::Integer(
            result.cast(result.ty().bit_field_promoted(field.width_bits)),
        ))
    }

    fn store_bit_field(
        &mut self,
        address: Pointer,
        ty: TypeId,
        field: BitFieldLayout,
        value: CValue,
        volatile: bool,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let value =
            self.normalize_bit_field(expect_machine_integer(value, span)?, ty, field, span)?;
        if volatile {
            for (offset, bytes) in field.storage_units() {
                let first = u32::from(field.bit_offset).max(offset * 8);
                let last = (u32::from(field.bit_offset) + u32::from(field.width_bits))
                    .min((offset + bytes) * 8);
                let count = last - first;
                let within = first - offset * 8;
                let at = address
                    .checked_offset(i64::from(offset) - i64::from(field.bit_offset / 8))
                    .map_err(|error| self.memory_error(error, span))?;
                let layout = AccessLayout::new(u64::from(bytes), 1);
                let (original, world) = self
                    .memory
                    .volatile_load_integer(&self.world, at, layout)
                    .map_err(|error| self.memory_error(error, span))?;
                let mask = (u128::MAX >> (128 - count)) << within;
                let selected =
                    ((value.raw_bits() >> (first - u32::from(field.bit_offset))) << within) & mask;
                let stored = (original & !mask) | selected;
                let (memory, world) = self
                    .memory
                    .volatile_store_integer(&world, at, layout, stored)
                    .map_err(|error| self.memory_error(error, span))?;
                self.memory = memory;
                self.world = world;
            }
            return Ok(CValue::Integer(value));
        }
        let mut consumed = 0u16;
        while consumed < field.width_bits {
            let position = field.bit_offset % 8 + consumed;
            let within = position % 8;
            let count = (8 - within).min(field.width_bits - consumed);
            let at = address
                .checked_offset(i64::from(position / 8))
                .map_err(|error| self.memory_error(error, span))?;
            let mask = ((1u128 << count) - 1) << within;
            let original = if count == 8 {
                0
            } else {
                self.memory
                    .load_integer(at, AccessLayout::new(1, 1))
                    .map_err(|error| self.memory_error(error, span))?
            };
            let byte = (original & !mask) | (((value.raw_bits() >> consumed) << within) & mask);
            self.memory = self
                .memory
                .store_integer(at, AccessLayout::new(1, 1), byte)
                .map_err(|error| self.memory_error(error, span))?;
            consumed += count;
        }
        Ok(CValue::Integer(value))
    }

    fn load_memory_value(
        &mut self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        // Decay of an incomplete array does not require its size.
        let layout = if matches!(
            self.unit.types().get(ty).map(|ty| &ty.kind),
            Some(TypeKind::Array { .. })
        ) {
            AccessLayout::new(0, 1)
        } else {
            self.access_layout(ty, span)?
        };
        self.load_memory_value_with_layout(address, ty, qualifiers, span, layout)
    }

    fn place_access_layout(
        &self,
        place: &MemoryPlace<'_>,
        span: SourceSpan,
    ) -> Result<AccessLayout, InterpreterError> {
        if matches!(
            self.unit.types().get(place.value_ty()).map(|ty| &ty.kind),
            Some(TypeKind::Array { .. })
        ) {
            return Ok(AccessLayout::new(0, 1));
        }
        place
            .access_layout(self.unit.types(), &self.target)
            .map(AccessLayout::from_type_layout)
            .ok_or(InterpreterError::UnsupportedType {
                ty: place.value_ty(),
                span,
            })
    }

    fn apply_atomic(
        &mut self,
        access: &crate::CHirAtomicAccess<'arena>,
        address: CValue,
        stored: Option<CValue>,
        expected: Option<CValue>,
    ) -> Result<CValue, InterpreterError> {
        let span = access.span;
        let (address, _) = expect_machine_pointer(address, span)?;
        let layout = self.access_layout(access.access_ty, span)?;
        let order = access
            .constant_order
            .unwrap_or(evocc_memory::AtomicOrder::SeqCst);
        let ty = self.value_type(access.access_ty, span)?;
        if let Some((operation, result_kind)) = access.rmw {
            let value = self.atomic_operand_value(
                stored.ok_or(InterpreterError::UnsupportedType {
                    ty: access.access_ty,
                    span,
                })?,
                ty,
                span,
            )?;
            let expected = expected
                .map(|value| self.atomic_operand_value(value, ty, span))
                .transpose()?;
            let (memory, world, result) = self
                .memory
                .atomic_rmw(
                    &self.world,
                    address,
                    layout,
                    order,
                    access.volatile,
                    operation,
                    value,
                    expected,
                )
                .map_err(|error| self.memory_error(error, span))?;
            self.memory = memory;
            self.world = world;
            return match result_kind {
                crate::AtomicRmwReturn::Old => self.atomic_result_value(result.old, ty, span),
                crate::AtomicRmwReturn::New => self.atomic_result_value(result.new, ty, span),
                crate::AtomicRmwReturn::Success => Ok(CValue::Integer(CInteger::new(
                    u64::from(result.exchanged),
                    CIntegerType::BOOL,
                ))),
            };
        }
        if let Some(stored) = stored {
            let stored = self.cast_runtime_value(stored, ty, span)?;
            let stored = match stored {
                CValue::Integer(value) => evocc_memory::AtomicValue::Integer(value.raw_bits()),
                CValue::Pointer { value, .. } => {
                    evocc_memory::AtomicValue::Pointer(PointerValue::object(value))
                }
                CValue::FunctionPointer { value, .. } => evocc_memory::AtomicValue::Pointer(value),
                CValue::Uninitialized(_) => {
                    return Err(InterpreterError::UninitializedValue { span })
                }
                _ => {
                    return Err(InterpreterError::UnsupportedType {
                        ty: access.access_ty,
                        span,
                    })
                }
            };
            let (memory, world) = self
                .memory
                .atomic_store(&self.world, address, layout, order, access.volatile, stored)
                .map_err(|error| self.memory_error(error, span))?;
            self.memory = memory;
            self.world = world;
            Ok(CValue::Integer(CInteger::int(0)))
        } else {
            let (value, world) = self
                .memory
                .atomic_load(
                    &self.world,
                    address,
                    layout,
                    order,
                    access.volatile,
                    matches!(ty, ValueType::Pointer(_)),
                )
                .map_err(|error| self.memory_error(error, span))?;
            self.world = world;
            match (value, ty) {
                (evocc_memory::AtomicValue::Integer(raw), ValueType::Integer(ty)) => {
                    Ok(CValue::Integer(CInteger::from_raw(raw, ty)))
                }
                (evocc_memory::AtomicValue::Pointer(value), ValueType::Pointer(ty)) => {
                    if pointer_type_is_function(self.unit.types(), ty) {
                        return Ok(CValue::FunctionPointer { value, ty });
                    }
                    let value = match value {
                        PointerValue::Null => Pointer::NULL,
                        PointerValue::Object(value) => value,
                        PointerValue::Exposed(value) => Pointer::from_address(value),
                        PointerValue::Function(_) => {
                            return Ok(CValue::FunctionPointer { value, ty })
                        }
                    };
                    Ok(CValue::Pointer { value, ty })
                }
                _ => Err(InterpreterError::UnsupportedType {
                    ty: access.access_ty,
                    span,
                }),
            }
        }
    }

    fn atomic_operand_value(
        &mut self,
        value: CValue,
        ty: ValueType,
        span: SourceSpan,
    ) -> Result<evocc_memory::AtomicValue, InterpreterError> {
        match self.cast_runtime_value(value, ty, span)? {
            CValue::Integer(value) => Ok(evocc_memory::AtomicValue::Integer(value.raw_bits())),
            CValue::Pointer { value, .. } => Ok(evocc_memory::AtomicValue::Pointer(
                PointerValue::object(value),
            )),
            CValue::FunctionPointer { value, .. } => Ok(evocc_memory::AtomicValue::Pointer(value)),
            _ => Err(InterpreterError::UninitializedValue { span }),
        }
    }

    fn atomic_result_value(
        &self,
        value: evocc_memory::AtomicValue,
        ty: ValueType,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        match (value, ty) {
            (evocc_memory::AtomicValue::Integer(raw), ValueType::Integer(ty)) => {
                Ok(CValue::Integer(CInteger::from_raw(raw, ty)))
            }
            (evocc_memory::AtomicValue::Pointer(value), ValueType::Pointer(ty)) => {
                if pointer_type_is_function(self.unit.types(), ty) {
                    return Ok(CValue::FunctionPointer { value, ty });
                }
                let value = match value {
                    PointerValue::Null => Pointer::NULL,
                    PointerValue::Object(pointer) => pointer,
                    PointerValue::Exposed(address) => Pointer::from_address(address),
                    PointerValue::Function(_) => return Ok(CValue::FunctionPointer { value, ty }),
                };
                Ok(CValue::Pointer { value, ty })
            }
            _ => Err(InterpreterError::UnsupportedExpression {
                span,
                feature: "atomic result type disagrees with access",
            }),
        }
    }

    fn load_memory_value_with_layout(
        &mut self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        span: SourceSpan,
        layout: AccessLayout,
    ) -> Result<CValue, InterpreterError> {
        // Array-to-pointer conversion does not access the array object or
        // produce a volatile event, even when it is a projected subobject.
        if let Some(TypeKind::Array { element, .. }) = self.unit.types().get(ty).map(|ty| &ty.kind)
        {
            let pointer_type = PointerTypeDesc::to_object(self.unit.types(), *element)
                .ok_or(InterpreterError::UnsupportedType { ty, span })?;
            return Ok(CValue::Pointer {
                value: address,
                ty: pointer_type,
            });
        }
        if qualifiers.is_atomic {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "atomic memory access",
                span,
            });
        }
        match self.value_type(ty, span)? {
            ValueType::Floating(kind) => {
                let raw = if qualifiers.is_volatile {
                    let (raw, world) = self
                        .memory
                        .volatile_load_integer(&self.world, address, layout)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.world = world;
                    raw
                } else {
                    self.memory
                        .load_integer(address, layout)
                        .map_err(|error| self.memory_error(error, span))?
                };
                Ok(CValue::Floating(
                    evocc_types::floating::FloatBits::from_bits(kind, raw).unwrap(),
                ))
            }
            ValueType::Integer(integer_type) => {
                let raw = if qualifiers.is_volatile {
                    let (raw, world) = self
                        .memory
                        .volatile_load_integer(&self.world, address, layout)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.world = world;
                    raw
                } else {
                    self.memory
                        .load_integer(address, layout)
                        .map_err(|error| self.memory_error(error, span))?
                };
                Ok(CValue::Integer(CInteger::from_raw(raw, integer_type)))
            }
            ValueType::Pointer(pointer_type) => {
                let is_function = matches!(
                    self.unit
                        .types()
                        .get(pointer_type.pointee())
                        .map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                );
                if is_function {
                    let value = if qualifiers.is_volatile {
                        let (value, world) = self
                            .memory
                            .volatile_load_pointer_value(&self.world, address, layout)
                            .map_err(|error| {
                                Self::pointer_load_error(error, address, layout, span)
                            })?;
                        self.world = world;
                        value
                    } else {
                        self.memory
                            .load_pointer_value(address, layout)
                            .map_err(|error| {
                                Self::pointer_load_error(error, address, layout, span)
                            })?
                    };
                    return Ok(CValue::FunctionPointer {
                        value,
                        ty: pointer_type,
                    });
                }
                let value = if qualifiers.is_volatile {
                    let (value, world) = self
                        .memory
                        .volatile_load_pointer(&self.world, address, layout)
                        .map_err(|error| Self::pointer_load_error(error, address, layout, span))?;
                    self.world = world;
                    value
                } else {
                    self.memory
                        .load_pointer(address, layout)
                        .map_err(|error| Self::pointer_load_error(error, address, layout, span))?
                };
                Ok(CValue::Pointer {
                    value,
                    ty: pointer_type,
                })
            }
            ValueType::Aggregate(aggregate_type) => {
                if matches!(
                    self.unit.types().get(aggregate_type).map(|ty| &ty.kind),
                    Some(
                        TypeKind::Vector { .. }
                            | TypeKind::Complex(evocc_types::ComplexBase::Integer(_))
                    )
                ) {
                    let snapshot = self.vector_temporary(aggregate_type, span)?;
                    let (memory, world) = self
                        .memory
                        .copy_with_volatile(
                            &self.world,
                            snapshot,
                            address,
                            layout,
                            CopyVolatility::new(qualifiers.is_volatile, false),
                        )
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                    return Ok(CValue::Aggregate {
                        address: snapshot,
                        ty: aggregate_type,
                        qualifiers: Qualifiers::default(),
                    });
                }
                Ok(CValue::Aggregate {
                    address,
                    ty: aggregate_type,
                    qualifiers,
                })
            }
            ValueType::Void => Err(InterpreterError::UnsupportedType { ty, span }),
        }
    }

    fn store_memory_value(
        &mut self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        value: CValue,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        if qualifiers.is_atomic {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "atomic memory access",
                span,
            });
        }
        let layout = self.access_layout(ty, span)?;
        self.store_memory_value_with_layout(address, ty, qualifiers, value, span, layout)
    }

    fn store_static_scalar(
        &mut self,
        address: Pointer,
        ty: TypeId,
        value: CValue,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        self.store_initialized_value(address, ty, value, span)
    }

    fn store_constant_representation(
        &mut self,
        address: Pointer,
        ty: TypeId,
        bits: CInteger,
        volatile: bool,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let mut layout = self.access_layout(ty, span)?;
        let object = self
            .memory
            .object_info(address.object())
            .map_err(|e| self.memory_error(e, span))?;
        layout.align_bytes = layout.align_bytes.min(object.layout.align_bytes);
        let offset = address.byte_offset().unsigned_abs();
        if offset != 0 {
            layout.align_bytes = layout
                .align_bytes
                .min(1_u32 << offset.trailing_zeros().min(31));
        }
        if volatile {
            let (memory, world) = self
                .memory
                .volatile_store_integer(&self.world, address, layout, bits.raw_bits())
                .map_err(|e| self.memory_error(e, span))?;
            self.memory = memory;
            self.world = world;
        } else {
            self.memory = self
                .memory
                .store_integer(address, layout, bits.raw_bits())
                .map_err(|e| self.memory_error(e, span))?;
        }
        Ok(())
    }

    /// The initializer path has already verified the subobject's identity.
    /// Its alignment follows the containing allocation and byte offset, not
    /// an arbitrary pointer cast to the scalar type.
    fn store_initialized_value(
        &mut self,
        address: Pointer,
        ty: TypeId,
        value: CValue,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let mut layout = self.access_layout(ty, span)?;
        let object = self
            .memory
            .object_info(address.object())
            .map_err(|error| self.memory_error(error, span))?;
        layout.align_bytes = layout.align_bytes.min(object.layout.align_bytes);
        let offset = address.byte_offset().unsigned_abs();
        if offset != 0 {
            layout.align_bytes = layout
                .align_bytes
                .min(1_u32 << offset.trailing_zeros().min(31));
        }
        self.store_memory_value_with_layout(address, ty, Qualifiers::default(), value, span, layout)
    }

    fn store_memory_value_with_layout(
        &mut self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        value: CValue,
        span: SourceSpan,
        layout: AccessLayout,
    ) -> Result<CValue, InterpreterError> {
        match (self.value_type(ty, span)?, value) {
            (ValueType::Floating(kind), value) => {
                let strict = self.strict_floating();
                let value = floating::convert_runtime(value, kind, span, &mut self.world, strict)?;
                if qualifiers.is_volatile {
                    let (memory, world) = self
                        .memory
                        .volatile_store_integer(&self.world, address, layout, value.bits())
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                } else {
                    self.memory = self
                        .memory
                        .store_integer(address, layout, value.bits())
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::Floating(value))
            }
            (_, CValue::Uninitialized(_)) => Err(InterpreterError::UninitializedValue { span }),
            (ValueType::Integer(integer_type), CValue::Integer(value)) => {
                let value = value.cast(integer_type);
                if qualifiers.is_volatile {
                    let (memory, world) = self
                        .memory
                        .volatile_store_integer(&self.world, address, layout, value.raw_bits())
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                } else {
                    self.memory = self
                        .memory
                        .store_integer(address, layout, value.raw_bits())
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::Integer(value))
            }
            (ValueType::Pointer(expected), CValue::Pointer { value, ty: actual })
                if expected == actual =>
            {
                if qualifiers.is_volatile {
                    let (memory, world) = self
                        .memory
                        .volatile_store_pointer(&self.world, address, layout, value)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                } else {
                    self.memory = self
                        .memory
                        .store_pointer(address, layout, value)
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::Pointer {
                    value,
                    ty: expected,
                })
            }
            (ValueType::Pointer(expected), CValue::FunctionPointer { value, ty: actual })
                if expected == actual
                    && matches!(
                        self.unit.types().get(expected.pointee()).map(|ty| &ty.kind),
                        Some(TypeKind::Function { .. })
                    ) =>
            {
                if qualifiers.is_volatile {
                    let (memory, world) = self
                        .memory
                        .volatile_store_pointer_value(&self.world, address, layout, value)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                } else {
                    self.memory = self
                        .memory
                        .store_pointer_value(address, layout, value)
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::FunctionPointer {
                    value,
                    ty: expected,
                })
            }
            (
                ValueType::Aggregate(expected),
                CValue::Aggregate {
                    address: source,
                    ty: actual,
                    qualifiers: source_qualifiers,
                },
            ) if self.same_aggregate_type(expected, actual) => {
                if qualifiers.is_volatile || source_qualifiers.is_volatile {
                    self.copy_volatile_aggregate(
                        address,
                        expected,
                        qualifiers,
                        source,
                        actual,
                        source_qualifiers,
                        span,
                    )?;
                } else {
                    self.memory = self
                        .memory
                        .copy_bytes(address, source, layout)
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::Aggregate {
                    address,
                    ty: expected,
                    qualifiers,
                })
            }
            _ => Err(InterpreterError::UnsupportedExpression {
                feature: "incompatible memory store value",
                span,
            }),
        }
    }

    fn same_aggregate_type(&self, left: TypeId, right: TypeId) -> bool {
        matches!(
            (self.unit.types().get(left), self.unit.types().get(right)),
            (Some(left), Some(right))
                if matches!(left.kind, TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList | TypeKind::Vector { .. } | TypeKind::Complex(evocc_types::ComplexBase::Integer(_)))
                    && left.kind == right.kind
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_volatile_aggregate(
        &mut self,
        destination: Pointer,
        destination_type: TypeId,
        destination_qualifiers: Qualifiers,
        source: Pointer,
        source_type: TypeId,
        source_qualifiers: Qualifiers,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let destination_kind = self
            .unit
            .types()
            .get(destination_type)
            .ok_or(InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            })?
            .kind
            .clone();
        let source_kind = self
            .unit
            .types()
            .get(source_type)
            .ok_or(InterpreterError::UnsupportedType {
                ty: source_type,
                span,
            })?
            .kind
            .clone();
        if destination_kind != source_kind {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "incompatible volatile aggregate copy",
                span,
            });
        }
        match destination_kind {
            TypeKind::Struct(tag) => {
                let fields = match &self
                    .unit
                    .types()
                    .tag(tag)
                    .ok_or(InterpreterError::UnsupportedType {
                        ty: destination_type,
                        span,
                    })?
                    .state
                {
                    TagState::Complete(TagDefinition::Struct(fields)) => fields.clone(),
                    _ => {
                        return Err(InterpreterError::UnsupportedType {
                            ty: destination_type,
                            span,
                        })
                    }
                };
                let record = self
                    .target
                    .record_layout(self.unit.types(), tag)
                    .map_err(|_| InterpreterError::UnsupportedType {
                        ty: destination_type,
                        span,
                    })?;
                for (field_index, field) in fields.iter().enumerate() {
                    let layout_field = record
                        .fields
                        .iter()
                        .find(|layout| layout.field_index == field_index)
                        .ok_or(InterpreterError::UnsupportedType { ty: field.ty, span })?;
                    if layout_field.bit_field.is_some() {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "volatile aggregate bit-field copy",
                            span,
                        });
                    }
                    if matches!(
                        self.unit.types().get(field.ty).map(|ty| &ty.kind),
                        Some(TypeKind::Array {
                            bound: ArrayBound::Unbounded,
                            ..
                        })
                    ) {
                        continue;
                    }
                    let offset = i64::try_from(layout_field.byte_offset)
                        .map_err(|_| InterpreterError::UnsupportedType { ty: field.ty, span })?;
                    let destination = destination
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    let source = source
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    let field_qualifiers = self
                        .unit
                        .types()
                        .get(field.ty)
                        .ok_or(InterpreterError::UnsupportedType { ty: field.ty, span })?
                        .qualifiers;
                    self.copy_volatile_subobject(
                        destination,
                        field.ty,
                        merge_memory_qualifiers(destination_qualifiers, field_qualifiers),
                        source,
                        field.ty,
                        merge_memory_qualifiers(source_qualifiers, field_qualifiers),
                        span,
                    )?;
                }
                Ok(())
            }
            TypeKind::Union(_) => {
                let layout = self.access_layout(destination_type, span)?;
                let volatile = CopyVolatility::new(
                    source_qualifiers.is_volatile,
                    destination_qualifiers.is_volatile,
                );
                if !volatile.supports(layout) {
                    return Err(InterpreterError::VolatileAggregate { span });
                }
                let (memory, world) = self
                    .memory
                    .copy_with_volatile(&self.world, destination, source, layout, volatile)
                    .map_err(|error| self.memory_error(error, span))?;
                self.memory = memory;
                self.world = world;
                Ok(())
            }
            _ => Err(InterpreterError::UnsupportedExpression {
                feature: "volatile aggregate copy reached non-aggregate",
                span,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_volatile_subobject(
        &mut self,
        destination: Pointer,
        destination_type: TypeId,
        destination_qualifiers: Qualifiers,
        source: Pointer,
        source_type: TypeId,
        source_qualifiers: Qualifiers,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        match self
            .unit
            .types()
            .get(destination_type)
            .map(|ty| ty.kind.clone())
        {
            Some(TypeKind::Struct(_) | TypeKind::Union(_)) => self.copy_volatile_aggregate(
                destination,
                destination_type,
                destination_qualifiers,
                source,
                source_type,
                source_qualifiers,
                span,
            ),
            Some(TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(count),
            }) => {
                let layout = self.access_layout(element, span)?;
                for index in 0..count {
                    let offset = index
                        .checked_mul(layout.size_bytes)
                        .and_then(|offset| i64::try_from(offset).ok())
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: destination_type,
                            span,
                        })?;
                    self.copy_volatile_subobject(
                        destination
                            .checked_offset(offset)
                            .map_err(|error| self.memory_error(error, span))?,
                        element,
                        destination_qualifiers,
                        source
                            .checked_offset(offset)
                            .map_err(|error| self.memory_error(error, span))?,
                        element,
                        source_qualifiers,
                        span,
                    )?;
                }
                Ok(())
            }
            Some(TypeKind::Array { .. }) => Ok(()),
            Some(_) => {
                let value = self.load_memory_value(source, source_type, source_qualifiers, span)?;
                self.store_memory_value(
                    destination,
                    destination_type,
                    destination_qualifiers,
                    value,
                    span,
                )?;
                Ok(())
            }
            None => Err(InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            }),
        }
    }

    /// Step a named scalar object by one, in whichever storage model is active.
    ///
    /// The task machine runs in both modes, so this cannot assume the scalar
    /// lives in an interpreter slot; in memory mode the object is a real
    /// addressable object like any other.
    fn apply_scalar_update(
        &mut self,
        operator: UpdateOp,
        symbol: SymbolId,
        target_span: SourceSpan,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let previous = if self.memory_mode {
            let binding = self.require_memory_binding(symbol, target_span)?;
            let qualifiers = self
                .unit
                .types()
                .get(binding.ty)
                .ok_or(InterpreterError::UnsupportedType {
                    ty: binding.ty,
                    span: target_span,
                })?
                .qualifiers;
            self.load_memory_value(binding.address, binding.ty, qualifiers, target_span)?
        } else {
            let slot = *self
                .slots
                .get(&symbol)
                .ok_or(InterpreterError::InvalidScalarPlace { symbol, span })?;
            CValue::Integer(
                slot.value
                    .ok_or_else(|| self.uninitialized_read(symbol, target_span))?,
            )
        };
        let updated = self.step_update_value(previous, operator, span)?;
        if self.memory_mode {
            self.store_bound_value(symbol, updated, span)?;
        } else {
            let updated = expect_machine_integer(updated, span)?;
            self.slots
                .get_mut(&symbol)
                .ok_or(InterpreterError::InvalidScalarPlace { symbol, span })?
                .value = Some(updated);
        }
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                updated
            } else {
                previous
            },
        )
    }

    fn step_update_value(
        &mut self,
        previous: CValue,
        operator: UpdateOp,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let direction = match operator {
            UpdateOp::PreIncrement | UpdateOp::PostIncrement => OffsetDirection::Add,
            UpdateOp::PreDecrement | UpdateOp::PostDecrement => OffsetDirection::Subtract,
        };
        let strict = self.strict_floating();
        match previous {
            CValue::Floating(value) => floating::binary(
                if direction == OffsetDirection::Add {
                    BinaryOp::Add
                } else {
                    BinaryOp::Sub
                },
                CValue::Floating(value),
                CValue::Integer(CInteger::int(1)),
                span,
                &mut self.world,
                strict,
            ),
            CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span }),
            CValue::Integer(previous) => {
                let binary = if direction == OffsetDirection::Add {
                    BinaryOp::Add
                } else {
                    BinaryOp::Sub
                };
                self.runtime_integer_binary(binary, previous, CInteger::int(1), span)
                    .map(|updated| CValue::Integer(updated.cast(previous.ty())))
            }
            CValue::Pointer { value, ty } => {
                self.offset_pointer_value(value, ty, CInteger::int(1), direction, span)
            }
            CValue::FunctionPointer { value, ty } => {
                self.offset_function_value(value, ty, CInteger::int(1), direction, span)
            }
            CValue::Aggregate { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "aggregate update",
                span,
            }),
        }
    }

    fn uninitialized_read(&self, symbol: SymbolId, span: SourceSpan) -> InterpreterError {
        let spelling = self
            .unit
            .symbols()
            .symbol(symbol)
            .map(|symbol| symbol.spelling().to_owned())
            .unwrap_or_else(|_| format!("symbol#{}", symbol.raw()));
        InterpreterError::ReadOfUninitializedObject {
            symbol,
            spelling,
            span,
        }
    }

    fn store_bound_value(
        &mut self,
        symbol: SymbolId,
        value: CValue,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let binding = self.require_memory_binding(symbol, span)?;
        let qualifiers = self
            .unit
            .types()
            .get(binding.ty)
            .ok_or(InterpreterError::UnsupportedType {
                ty: binding.ty,
                span,
            })?
            .qualifiers;
        self.store_memory_value(binding.address, binding.ty, qualifiers, value, span)?;
        Ok(())
    }

    fn validate_memory_scalar_place(
        &self,
        place: ScalarPlace,
        target: &CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<SymbolId, InterpreterError> {
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span: target_span,
        } = target
        else {
            return Err(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span,
            });
        };
        let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
            InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            }
        })?;
        if is_global_register(self.unit, *symbol) {
            return Err(InterpreterError::Asm { span: *target_span });
        }
        let binding = self.require_memory_binding(*symbol, *target_span)?;
        if *symbol != place.symbol()
            || !matches!(
                declaration.kind(),
                SymbolKind::Object | SymbolKind::Parameter
            )
            || declaration.spelling() != spelling
            || declaration.ty() != place.ty()
            || binding.ty != place.ty()
            || facts.ty() != place.ty()
            || facts.category() != ExprCategory::LValue
            || !facts.is_modifiable_lvalue()
        {
            return Err(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            });
        }
        Ok(*symbol)
    }

    fn apply_bound_assignment(
        &mut self,
        operator: AssignmentOp,
        symbol: SymbolId,
        right: CValue,
        target_span: SourceSpan,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let binding = self.require_memory_binding(symbol, target_span)?;
        let qualifiers = self
            .unit
            .types()
            .get(binding.ty)
            .ok_or(InterpreterError::UnsupportedType {
                ty: binding.ty,
                span,
            })?
            .qualifiers;
        self.apply_memory_assignment(
            operator,
            binding.address,
            binding.ty,
            qualifiers,
            right,
            target_span,
            span,
            self.access_layout(binding.ty, span)?,
        )
    }

    fn runtime_integer_unary(
        &self,
        operator: UnaryOp,
        operand: CInteger,
    ) -> Result<CInteger, IntegerError> {
        if self.unit.signed_wrapping() && operator != UnaryOp::Abs {
            eval_constant_unary_integer(operator, operand)
        } else {
            eval_unary_integer(operator, operand)
        }
    }

    fn runtime_integer_binary(
        &self,
        operator: BinaryOp,
        left: CInteger,
        right: CInteger,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        if self.static_integer_evaluation {
            return eval_constant_binary_integer(operator, left, right)
                .map_err(|error| self.integer_error(error, span));
        }
        let result = if self.unit.signed_wrapping()
            && matches!(operator, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul)
        {
            eval_constant_binary_integer(operator, left, right)
        } else {
            eval_binary_integer(operator, left, right)
        };
        result.map_err(|error| self.integer_error(error, span))
    }

    fn apply_machine_binary(
        &mut self,
        operator: BinaryOp,
        left: CValue,
        right: CValue,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        if matches!(operator, BinaryOp::And | BinaryOp::Or) {
            return Ok(CValue::Integer(CInteger::int(i32::from(
                self.complex_truth(right, span)?,
            ))));
        }
        if operator == BinaryOp::Comma {
            return Ok(right);
        }
        if matches!(
            operator,
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
                | BinaryOp::BitXorAssign
        ) {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "unlowered assignment binary operator",
                span,
            });
        }
        match (left, right) {
            (CValue::Floating(_), _) | (_, CValue::Floating(_)) => {
                let strict = self.strict_floating();
                floating::binary(operator, left, right, span, &mut self.world, strict)
            }
            (CValue::Uninitialized(_), _) | (_, CValue::Uninitialized(_)) => {
                Err(InterpreterError::UninitializedValue { span })
            }
            (CValue::Integer(left), CValue::Integer(right)) => self
                .runtime_integer_binary(operator, left, right, span)
                .map(CValue::Integer),
            (CValue::FunctionPointer { value, ty }, CValue::Integer(index))
                if matches!(operator, BinaryOp::Add | BinaryOp::Sub) =>
            {
                self.offset_function_value(
                    value,
                    ty,
                    index,
                    if operator == BinaryOp::Add {
                        OffsetDirection::Add
                    } else {
                        OffsetDirection::Subtract
                    },
                    span,
                )
            }
            (CValue::Integer(index), CValue::FunctionPointer { value, ty })
                if operator == BinaryOp::Add =>
            {
                self.offset_function_value(value, ty, index, OffsetDirection::Add, span)
            }
            (
                CValue::FunctionPointer { value: left, .. },
                CValue::FunctionPointer { value: right, .. },
            ) if operator == BinaryOp::Sub => {
                let left = self
                    .memory
                    .pointer_address(left)
                    .map_err(|error| self.memory_error(error, span))?;
                let right = self
                    .memory
                    .pointer_address(right)
                    .map_err(|error| self.memory_error(error, span))?;
                let difference = i128::from(left) - i128::from(right);
                let value = i64::try_from(difference)
                    .map_err(|_| self.integer_error(IntegerError::SignedOverflow, span))?;
                Ok(CValue::Integer(CInteger::new(
                    value as u64,
                    CIntegerType::LONG,
                )))
            }
            (CValue::Pointer { value, ty }, CValue::Integer(index))
                if matches!(operator, BinaryOp::Add | BinaryOp::Sub) =>
            {
                self.offset_pointer_value(
                    value,
                    ty,
                    index,
                    if operator == BinaryOp::Add {
                        OffsetDirection::Add
                    } else {
                        OffsetDirection::Subtract
                    },
                    span,
                )
            }
            (CValue::Integer(index), CValue::Pointer { value, ty })
                if operator == BinaryOp::Add =>
            {
                self.offset_pointer_value(value, ty, index, OffsetDirection::Add, span)
            }
            (
                CValue::Pointer {
                    value: left,
                    ty: left_ty,
                },
                CValue::Pointer {
                    value: right,
                    ty: right_ty,
                },
            ) if operator == BinaryOp::Sub && left_ty == right_ty => {
                self.subtract_pointer_values(left, right, left_ty, span)
            }
            (
                CValue::Pointer {
                    value: left,
                    ty: left_ty,
                },
                CValue::Pointer {
                    value: right,
                    ty: right_ty,
                },
            ) if matches!(
                operator,
                BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
            ) && PointerTypeDesc::composite(self.unit.types(), left_ty, right_ty).is_ok() =>
            {
                if self.static_integer_evaluation {
                    // The static-expression whitelist has evaluated numeric
                    // casts through the ordinary task machine and fuel budget.
                    // Comparing their representations does not access objects.
                    let bits = |pointer| {
                        self.memory
                            .pointer_address(PointerValue::object(pointer))
                            .map(|bits| CInteger::new(bits, CIntegerType::ULONG))
                            .map_err(|error| self.memory_error(error, span))
                    };
                    return self
                        .runtime_integer_binary(operator, bits(left)?, bits(right)?, span)
                        .map(CValue::Integer);
                }
                // Ordering compares byte addresses. Unlike subtraction it
                // does not require an exact number of pointee-sized elements.
                self.pointer_byte_difference(left, right, span)?;
                let left = left.byte_offset();
                let right = right.byte_offset();
                let result = match operator {
                    BinaryOp::Lt => left < right,
                    BinaryOp::Le => left <= right,
                    BinaryOp::Gt => left > right,
                    BinaryOp::Ge => left >= right,
                    _ => unreachable!(),
                };
                Ok(CValue::Integer(CInteger::new(
                    u64::from(result),
                    CIntegerType::INT,
                )))
            }
            (
                CValue::Pointer {
                    value: left,
                    ty: left_ty,
                },
                CValue::Pointer {
                    value: right,
                    ty: right_ty,
                },
            ) if matches!(operator, BinaryOp::Eq | BinaryOp::Ne)
                && PointerTypeDesc::equality_compatible(self.unit.types(), left_ty, right_ty) =>
            {
                let left = self
                    .memory
                    .pointer_address(PointerValue::object(left))
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                let right = self
                    .memory
                    .pointer_address(PointerValue::object(right))
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                Ok(machine_equality_result(operator, left == right))
            }
            (CValue::Pointer { value, .. }, CValue::Integer(zero))
            | (CValue::Integer(zero), CValue::Pointer { value, .. })
                if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) && zero.raw_bits() == 0 =>
            {
                Ok(machine_equality_result(operator, value == Pointer::NULL))
            }
            (
                CValue::FunctionPointer {
                    value: left,
                    ty: left_ty,
                },
                CValue::FunctionPointer {
                    value: right,
                    ty: right_ty,
                },
            ) if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) && left_ty == right_ty => {
                let left = self
                    .memory
                    .pointer_address(left)
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                let right = self
                    .memory
                    .pointer_address(right)
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                Ok(machine_equality_result(operator, left == right))
            }
            (CValue::FunctionPointer { value, .. }, CValue::Pointer { value: null, .. })
            | (CValue::Pointer { value: null, .. }, CValue::FunctionPointer { value, .. })
                if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) && null == Pointer::NULL =>
            {
                Ok(machine_equality_result(
                    operator,
                    value == PointerValue::Null,
                ))
            }
            (CValue::FunctionPointer { value, .. }, CValue::Integer(zero))
            | (CValue::Integer(zero), CValue::FunctionPointer { value, .. })
                if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) && zero.raw_bits() == 0 =>
            {
                Ok(machine_equality_result(
                    operator,
                    value == PointerValue::Null,
                ))
            }
            (
                CValue::FunctionPointer {
                    value: function,
                    ty: function_ty,
                },
                CValue::Pointer {
                    value: object,
                    ty: object_ty,
                },
            )
            | (
                CValue::Pointer {
                    value: object,
                    ty: object_ty,
                },
                CValue::FunctionPointer {
                    value: function,
                    ty: function_ty,
                },
            ) if matches!(operator, BinaryOp::Eq | BinaryOp::Ne)
                && PointerTypeDesc::equality_compatible(
                    self.unit.types(),
                    function_ty,
                    object_ty,
                ) =>
            {
                let function = self
                    .memory
                    .pointer_address(function)
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                let object = self
                    .memory
                    .pointer_address(PointerValue::object(object))
                    .map_err(|error| InterpreterError::Memory { span, error })?;
                Ok(machine_equality_result(operator, function == object))
            }
            _ => Err(InterpreterError::UnsupportedExpression {
                feature: "unsupported pointer binary operation",
                span,
            }),
        }
    }

    fn offset_function_value(
        &self,
        pointer: PointerValue,
        ty: PointerTypeDesc,
        index: CInteger,
        direction: OffsetDirection,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let address = self
            .memory
            .pointer_address(pointer)
            .map_err(|error| self.memory_error(error, span))?;
        let index = index.cast(CIntegerType::ULONG).raw_bits() as u64;
        let address = match direction {
            OffsetDirection::Add => address.wrapping_add(index),
            OffsetDirection::Subtract => address.wrapping_sub(index),
        };
        Ok(CValue::FunctionPointer {
            value: if address == 0 {
                PointerValue::Null
            } else {
                PointerValue::Exposed(address)
            },
            ty,
        })
    }

    fn offset_pointer_value(
        &self,
        pointer: Pointer,
        pointer_type: PointerTypeDesc,
        index: CInteger,
        direction: OffsetDirection,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let index = index.promoted();
        let scale_bytes = self.pointer_arithmetic_scale(pointer_type.pointee(), pointer, span)?;
        let signedness = if index.ty().is_signed() {
            IndexSignedness::Signed
        } else {
            IndexSignedness::Unsigned
        };
        let value = pointer
            .checked_scaled_offset(
                if scale_bytes == 0 {
                    0
                } else {
                    index.raw_bits()
                },
                u16::from(index.ty().bits()),
                signedness,
                direction,
                scale_bytes.max(1),
            )
            .map_err(|error| self.memory_error(error, span))?;
        // Exposed integer/code addresses have no object until an access
        // resolves one. Arithmetic transports those bits; it must not try to
        // dereference a label address as an object. Genuine object pointers
        // retain the ordinary lifetime/one-past checks.
        if !pointer.object().is_null() {
            self.memory
                .check_pointer(value)
                .map_err(|error| self.memory_error(error, span))?;
        }
        Ok(CValue::Pointer {
            value,
            ty: pointer_type,
        })
    }

    fn pointer_arithmetic_scale(
        &self,
        pointee: TypeId,
        pointer: Pointer,
        span: SourceSpan,
    ) -> Result<u64, InterpreterError> {
        let (mut size, captures) = self
            .target
            .array_size_factors(self.unit.types(), pointee)
            .map_err(|_| InterpreterError::UnsupportedType { ty: pointee, span })?;
        let captured = !captures.is_empty();
        for raw in captures {
            let symbol = self
                .unit
                .symbols()
                .symbols_in_source_order()
                .get(raw as usize)
                .ok_or(InterpreterError::UnsupportedExpression {
                    span,
                    feature: "array size capture has no symbol",
                })?;
            if symbol.kind() != SymbolKind::Object
                || self.integer_type(symbol.ty(), span)? != CIntegerType::ULONG
                || self
                    .unit
                    .types()
                    .get(symbol.ty())
                    .is_none_or(|ty| ty.qualifiers.is_volatile || ty.qualifiers.is_atomic)
            {
                return Err(InterpreterError::UnsupportedExpression {
                    span,
                    feature: "array size capture is not size_t",
                });
            }
            let count = if self.memory_mode {
                let binding = self.require_memory_binding(symbol.id(), span)?;
                self.memory
                    .load_integer(binding.address, self.access_layout(symbol.ty(), span)?)
                    .map_err(|error| self.memory_error(error, span))? as u64
            } else {
                self.slots
                    .get(&symbol.id())
                    .filter(|slot| slot.active)
                    .and_then(|slot| slot.value)
                    .ok_or(InterpreterError::UnsupportedExpression {
                        span,
                        feature: "array size capture has no active value",
                    })?
                    .raw_bits() as u64
            };
            size = size
                .checked_mul(count)
                .filter(|size| *size <= i64::MAX as u64)
                .unwrap_or(0);
        }
        if captured && size == 0 {
            return Err(self.memory_error(
                MemoryFault::PointerOffsetOverflow {
                    pointer,
                    operand: OffsetOperand::Scaled {
                        raw_bits: 0,
                        width_bits: 64,
                        signedness: IndexSignedness::Unsigned,
                        direction: OffsetDirection::Add,
                        scale_bytes: size,
                    },
                },
                span,
            ));
        }
        Ok(size)
    }

    fn subtract_pointer_values(
        &self,
        left: Pointer,
        right: Pointer,
        pointer_type: PointerTypeDesc,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let byte_delta = self.pointer_byte_difference(left, right, span)?;
        let scale =
            i64::try_from(self.pointer_arithmetic_scale(pointer_type.pointee(), left, span)?)
                .map_err(|_| InterpreterError::UnsupportedExpression {
                    feature: "pointer subtraction scale is not representable",
                    span,
                })?;
        if byte_delta % scale != 0 {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "pointer subtraction is not an exact element difference",
                span,
            });
        }
        Ok(CValue::Integer(CInteger::new(
            (byte_delta / scale) as u64,
            CIntegerType::LONG,
        )))
    }

    fn pointer_byte_difference(
        &self,
        left: Pointer,
        right: Pointer,
        span: SourceSpan,
    ) -> Result<i64, InterpreterError> {
        let address = |pointer| {
            self.memory
                .pointer_address(PointerValue::object(pointer))
                .map_err(|error| self.memory_error(error, span))
        };
        let left_address = address(left)?;
        let right_address = address(right)?;
        let owner = |address| {
            self.symbol_order()
                .labels
                .values()
                .find_map(|(owner, _, identity)| {
                    (self
                        .memory
                        .pointer_address(PointerValue::Function(*identity))
                        == Ok(address))
                    .then_some(*owner)
                })
        };
        if owner(left_address).is_some_and(|left| Some(left) == owner(right_address)) {
            return i64::try_from(i128::from(left_address) - i128::from(right_address))
                .map_err(|_| self.integer_error(IntegerError::SignedOverflow, span));
        }
        self.memory
            .check_pointer(left)
            .map_err(|error| self.memory_error(error, span))?;
        self.memory
            .check_pointer(right)
            .map_err(|error| self.memory_error(error, span))?;
        if left.object() != right.object() {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "pointer subtraction across distinct objects",
                span,
            });
        }
        left.byte_offset().checked_sub(right.byte_offset()).ok_or(
            InterpreterError::UnsupportedExpression {
                feature: "pointer subtraction overflow",
                span,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_memory_assignment(
        &mut self,
        operator: AssignmentOp,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        right: CValue,
        target_span: SourceSpan,
        span: SourceSpan,
        layout: AccessLayout,
    ) -> Result<CValue, InterpreterError> {
        if operator == AssignmentOp::Simple {
            // Simple assignment converts its right operand to the type of the
            // designated object, which is where a null pointer constant becomes
            // a null pointer and a narrower integer is truncated.
            let right = self.cast_runtime_value(right, self.value_type(ty, span)?, span)?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, right, span, layout);
        }
        let left =
            self.load_memory_value_with_layout(address, ty, qualifiers, target_span, layout)?;
        if matches!(left, CValue::Floating(_)) || matches!(right, CValue::Floating(_)) {
            let op =
                assignment_binary(operator).ok_or(InterpreterError::UnsupportedExpression {
                    feature: "invalid floating compound assignment",
                    span,
                })?;
            let result = self.apply_machine_binary(op, left, right, span)?;
            let result = self.cast_runtime_value(result, self.value_type(ty, span)?, span)?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, result, span, layout);
        }
        if crate::complex::rank(self.unit.types(), ty).is_some() {
            let op = crate::vector::assignment_binary(operator).ok_or(
                InterpreterError::UnsupportedExpression {
                    feature: "invalid complex assignment",
                    span,
                },
            )?;
            let operation = crate::complex::ComplexExpr {
                op: crate::complex::ComplexOp::Binary(op),
                operands: Vec::new(),
                facts: ExprFacts::value(ty),
                span,
            };
            let result = self.apply_complex(&operation, &[left, right])?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, result, span, layout);
        }
        if crate::vector::shape(self.unit.types(), ty).is_some() {
            let op = crate::vector::assignment_binary(operator).ok_or(
                InterpreterError::UnsupportedExpression {
                    span,
                    feature: "invalid vector assignment",
                },
            )?;
            let operation = crate::vector::VectorExpr {
                op: crate::vector::VectorOp::Binary(op),
                operands: Vec::new(),
                facts: ExprFacts::value(ty),
                span,
            };
            let result = self.apply_vector(&operation, &[left, right])?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, result, span, layout);
        }
        if matches!(
            left,
            CValue::Pointer { .. } | CValue::FunctionPointer { .. }
        ) {
            let binary = match operator {
                AssignmentOp::Add => BinaryOp::Add,
                AssignmentOp::Subtract => BinaryOp::Sub,
                _ => {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "invalid compound pointer assignment operator",
                        span,
                    });
                }
            };
            let result = self.apply_machine_binary(binary, left, right, span)?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, result, span, layout);
        }
        let left = expect_machine_integer(left, target_span)?;
        let right = expect_machine_integer(right, span)?;
        let binary =
            assignment_binary(operator).ok_or(InterpreterError::UnsupportedExpression {
                feature: "invalid compound assignment operator",
                span,
            })?;
        let result = self.runtime_integer_binary(binary, left, right, span)?;
        self.store_memory_value_with_layout(
            address,
            ty,
            qualifiers,
            CValue::Integer(result),
            span,
            layout,
        )
    }

    fn predeclare_stmt(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
    ) -> Result<(), InterpreterError> {
        match statement {
            CHirStmt::Asm(assembly) => {
                for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                    self.predeclare_expr(operand.expression)?;
                }
            }
            CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
                self.predeclare_expr(expression)?;
            }
            CHirStmt::ComputedGoto(expression, _) => {
                self.predeclare_expr(expression)?;
            }
            CHirStmt::If(condition, then_statement, else_statement, _) => {
                self.predeclare_expr(condition)?;
                self.predeclare_stmt(then_statement)?;
                if let Some(else_statement) = else_statement {
                    self.predeclare_stmt(else_statement)?;
                }
            }
            CHirStmt::Switch(expression, body, _)
            | CHirStmt::While(expression, body, _)
            | CHirStmt::Case(expression, body, _) => {
                self.predeclare_expr(expression)?;
                self.predeclare_stmt(body)?;
            }
            CHirStmt::CaseRange(first, last, body, _) => {
                self.predeclare_expr(first)?;
                self.predeclare_expr(last)?;
                self.predeclare_stmt(body)?;
            }
            CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => {
                self.predeclare_stmt(body)?;
            }
            CHirStmt::DoWhile(body, condition, _) => {
                self.predeclare_stmt(body)?;
                self.predeclare_expr(condition)?;
            }
            CHirStmt::For(initializer, condition, step, body, _) => {
                match initializer {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(expression) => self.predeclare_expr(expression)?,
                    CHirForInit::Declaration(declarations) => {
                        for declaration in declarations {
                            self.predeclare_decl(declaration)?;
                        }
                    }
                }
                if let Some(condition) = condition {
                    self.predeclare_expr(condition)?;
                }
                if let Some(step) = step {
                    self.predeclare_expr(step)?;
                }
                self.predeclare_stmt(body)?;
            }
            CHirStmt::Compound(items, _, _) => {
                for item in items {
                    match item {
                        CHirBlockItem::Declaration(declaration) => {
                            self.predeclare_decl(declaration)?;
                        }
                        CHirBlockItem::Statement(statement) => {
                            self.predeclare_stmt(statement)?;
                        }
                    }
                }
            }
            CHirStmt::Return(None, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::AttributedNull(_, _)
            | CHirStmt::Null(_) => {}
        }
        Ok(())
    }

    fn predeclare_expr(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<(), InterpreterError> {
        // Declaration discovery is structural, not expression execution. Keep
        // source order with an explicit stack rather than consuming the host
        // stack for every operator in a preprocessed arithmetic chain.
        let mut pending = vec![expression];
        while let Some(expression) = pending.pop() {
            match expression {
                CHirExpr::Complex(access) => {
                    pending.extend(access.operands.iter().rev().copied());
                }
                CHirExpr::Floating(access) => {
                    pending.extend(access.operands.iter().rev().copied());
                }
                CHirExpr::Vector(access) => {
                    pending.extend(access.operands.iter().rev().copied());
                }
                CHirExpr::Atomic(access) => {
                    pending.extend(access.operands().rev());
                }
                CHirExpr::BuiltinConstantP { .. } | CHirExpr::BuiltinObjectSize { .. } => {}
                CHirExpr::Unary(_, operand, _)
                | CHirExpr::EvaluateOnce(operand, _)
                | CHirExpr::Cast(_, operand, _)
                | CHirExpr::Member(operand, _, _)
                | CHirExpr::PtrMember(operand, _, _) => pending.push(operand),
                CHirExpr::Binary(_, left, right, _)
                | CHirExpr::Index(left, right, _)
                | CHirExpr::Comma(left, right, _)
                | CHirExpr::Elvis(left, right, _)
                | CHirExpr::BuiltinExpect(left, right, _) => {
                    pending.push(right);
                    pending.push(left);
                }
                CHirExpr::Ternary(condition, when_true, when_false, _) => {
                    pending.push(when_false);
                    pending.push(when_true);
                    pending.push(condition);
                }
                CHirExpr::Call(callee, arguments, _) => {
                    pending.extend(arguments.iter().rev().copied());
                    pending.push(callee);
                }
                CHirExpr::CheckedArithmetic {
                    lhs, rhs, target, ..
                } => {
                    pending.push(target);
                    pending.push(rhs);
                    pending.push(lhs);
                }
                CHirExpr::BitBuiltin { operand, .. } => pending.push(operand),
                CHirExpr::BuiltinVarArgs { first, second, .. } => {
                    if let Some(second) = second {
                        pending.push(second);
                    }
                    pending.push(first);
                }
                CHirExpr::Assign { target, value, .. } => {
                    pending.push(value);
                    pending.push(target);
                }
                CHirExpr::Update { target, .. } => pending.push(target),
                CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
                    pending.extend(place.source_expressions().rev());
                }
                CHirExpr::MemoryAssign { place, value, .. } => {
                    pending.push(value);
                    pending.extend(place.source_expressions().rev());
                }
                CHirExpr::CompoundLit(_, initializer, _) => {
                    for action in initializer.plan().actions().iter().rev() {
                        if let InitAction::StoreScalar { value, .. } = action {
                            pending.push(value);
                        }
                    }
                }
                CHirExpr::StatementExpr { body, .. } => self.predeclare_stmt(body)?,
                CHirExpr::Lit(..)
                | CHirExpr::FloatLit(..)
                | CHirExpr::CharLit(..)
                | CHirExpr::StringLit(..)
                | CHirExpr::Ident { .. }
                | CHirExpr::LabelAddress { .. }
                | CHirExpr::SizeOf(..)
                | CHirExpr::SizeOfExpr(..)
                | CHirExpr::AlignOf(..)
                | CHirExpr::BuiltinTrap { .. }
                | CHirExpr::BuiltinUnreachable { .. }
                | CHirExpr::BuiltinReturnAddress { .. }
                | CHirExpr::BuiltinFrameAddress { .. } => {}
            }
        }
        Ok(())
    }

    fn predeclare_decl(
        &mut self,
        declaration: &'arena CHirDecl<'arena>,
    ) -> Result<(), InterpreterError> {
        match declaration {
            CHirDecl::Object(declaration) => {
                let ty = self.validate_object_declaration(declaration)?;
                self.validate_initializer_plan(declaration)?;
                if self
                    .slots
                    .insert(
                        declaration.symbol,
                        Slot {
                            ty,
                            value: None,
                            active: false,
                        },
                    )
                    .is_some()
                {
                    return Err(InterpreterError::MalformedObjectDeclaration {
                        symbol: declaration.symbol,
                        span: declaration.span,
                    });
                }
                if let Some(initializer) = &declaration.initializer {
                    for action in initializer.plan().actions() {
                        if let InitAction::StoreScalar { value, .. } = action {
                            self.predeclare_expr(value)?;
                        }
                    }
                }
                Ok(())
            }
            CHirDecl::Asm(_) | CHirDecl::Tag(_) | CHirDecl::Typedef(_) => Ok(()),
            CHirDecl::Function(function) if function.body.is_none() => Ok(()),
            CHirDecl::Function(declaration) => Err(InterpreterError::UnsupportedDeclaration {
                feature: "block function declaration",
                span: declaration.span,
            }),
        }
    }

    fn validate_object_declaration(
        &self,
        declaration: &ObjectDecl<'arena>,
    ) -> Result<CIntegerType, InterpreterError> {
        if !matches!(
            declaration.storage,
            StorageClass::None | StorageClass::Auto | StorageClass::Register
        ) {
            return Err(InterpreterError::UnsupportedDeclaration {
                feature: "non-automatic scalar object",
                span: declaration.span,
            });
        }
        let symbol = self
            .unit
            .symbols()
            .symbol(declaration.symbol)
            .map_err(|_| InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            })?;
        if symbol.kind() != SymbolKind::Object
            || symbol.spelling() != declaration.name.spelling
            || symbol.ty() != declaration.ty
            || symbol.storage_class() != Some(declaration.storage)
        {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        }
        self.integer_type(declaration.ty, declaration.span)
    }

    fn validate_initializer_plan(
        &self,
        declaration: &ObjectDecl<'arena>,
    ) -> Result<(), InterpreterError> {
        let Some(initializer) = &declaration.initializer else {
            return Ok(());
        };
        let plan = initializer.plan();
        if plan.target_type() != declaration.ty
            || plan.validate(self.unit.types()).is_err()
            || (plan.actions().is_empty() && !plan.zero_fills_unwritten())
        {
            return Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            });
        }
        // Each action must designate a real subobject of this declaration and
        // agree with that subobject's type. `SubobjectPath` resolution is the
        // same authority the initializer execution uses, so a plan that passes
        // here cannot fail to address its destination later.
        let valid = plan.actions().iter().all(|action| match action {
            InitAction::StoreScalar {
                path,
                destination_type,
                span,
                ..
            } => self
                .subobject_offset(declaration.ty, path, *span)
                .is_ok_and(|(_, ty, _)| ty == *destination_type),
            InitAction::CopyAggregate {
                path,
                destination_type,
                span,
                ..
            } => self
                .subobject_offset(declaration.ty, path, *span)
                .is_ok_and(|(_, ty, _)| ty == *destination_type),
            InitAction::ZeroScalar {
                path,
                destination_type,
            } => self
                .subobject_offset(declaration.ty, path, declaration.span)
                .is_ok_and(|(_, ty, _)| ty == *destination_type),
            InitAction::StoreString {
                path,
                destination_type,
                literal,
                span,
            } => {
                literal.decoded_units().is_some()
                    && self
                        .subobject_offset(declaration.ty, path, *span)
                        .is_ok_and(|(_, ty, _)| ty == *destination_type)
            }
        });
        if !valid {
            return Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: plan
                    .actions()
                    .first()
                    .and_then(InitAction::span)
                    .unwrap_or_else(|| initializer.span()),
            });
        }
        Ok(())
    }

    fn activate_declaration(
        &mut self,
        declaration: &'arena CHirDecl<'arena>,
        saved: &mut Vec<(SymbolId, Slot)>,
    ) -> Result<(), InterpreterError> {
        if self.memory_mode {
            return Ok(());
        }
        let CHirDecl::Object(declaration) = declaration else {
            return Ok(());
        };
        let Some(slot) = self.slots.get_mut(&declaration.symbol) else {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        };
        if slot.active {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        }
        saved.push((declaration.symbol, *slot));
        slot.active = true;
        slot.value = None;
        Ok(())
    }

    fn activate_block_items(
        &mut self,
        items: &[CHirBlockItem<'arena>],
    ) -> Result<Vec<(SymbolId, Slot)>, InterpreterError> {
        let mut saved = Vec::new();
        for item in items {
            if let CHirBlockItem::Declaration(declaration) = item {
                self.activate_declaration(declaration, &mut saved)?;
            }
        }
        Ok(saved)
    }

    fn activate_for_declarations(
        &mut self,
        declarations: &[&'arena CHirDecl<'arena>],
    ) -> Result<Vec<(SymbolId, Slot)>, InterpreterError> {
        let mut saved = Vec::new();
        for declaration in declarations {
            self.activate_declaration(declaration, &mut saved)?;
        }
        Ok(saved)
    }

    fn restore_slots(&mut self, saved: Vec<(SymbolId, Slot)>) {
        for (symbol, slot) in saved {
            self.slots.insert(symbol, slot);
        }
    }

    fn eval_expr(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        allow_root_mutation: bool,
    ) -> Result<CInteger, InterpreterError> {
        if let Some(value) = crate::constant::fold_floating_integer_arithmetic(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) {
            return Ok(value);
        }
        let span = expression.span();
        self.tick(span)?;
        if !self.static_integer_evaluation {
            if let Some(value) = crate::constant::fold_wrapping_integer_arithmetic(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            ) {
                return Ok(value);
            }
        }
        match expression {
            CHirExpr::Atomic(_)
            | CHirExpr::Vector(_)
            | CHirExpr::Complex(_)
            | CHirExpr::Floating(_) => Err(InterpreterError::UnsupportedExpression {
                feature: "atomic access outside memory execution",
                span,
            }),
            CHirExpr::Lit(literal, _) => {
                integer_literal_value(*literal).map_err(|error| self.integer_error(error, span))
            }
            CHirExpr::BuiltinConstantP { operand, .. } => {
                Ok(CInteger::int(i32::from(crate::builtin_constant_p(
                    operand,
                    self.unit.types(),
                    self.unit.symbols(),
                    &self.target,
                ))))
            }
            CHirExpr::BuiltinObjectSize { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "unresolved object extent query",
                span,
            }),
            CHirExpr::Ident {
                symbol,
                spelling,
                facts,
                span,
            } => {
                if is_global_register(self.unit, *symbol) {
                    return Err(InterpreterError::Asm { span: *span });
                }
                let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
                    InterpreterError::UnsupportedExpression {
                        feature: "unknown identifier",
                        span: *span,
                    }
                })?;
                if declaration.spelling() != spelling || declaration.ty() != facts.ty() {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "inconsistent identifier facts",
                        span: *span,
                    });
                }
                if let Some(value) = declaration.enumerator_value() {
                    let ty = self.integer_type(declaration.ty(), *span)?;
                    return Ok(CInteger::new(value as u64, ty));
                }
                let slot =
                    self.slots
                        .get(symbol)
                        .ok_or(InterpreterError::UnsupportedExpression {
                            feature: "identifier without automatic scalar slot",
                            span: *span,
                        })?;
                if !slot.active {
                    return Err(InterpreterError::OutOfScopeObject {
                        symbol: *symbol,
                        spelling: spelling.clone(),
                        span: *span,
                    });
                }
                slot.value
                    .ok_or_else(|| InterpreterError::ReadOfUninitializedObject {
                        symbol: *symbol,
                        spelling: spelling.clone(),
                        span: *span,
                    })
            }
            CHirExpr::Unary(operator, operand, _) => {
                let operand = self.eval_expr(operand, false)?;
                let result = if self.static_integer_evaluation {
                    eval_constant_unary_integer(*operator, operand)
                } else {
                    self.runtime_integer_unary(*operator, operand)
                };
                result.map_err(|error| {
                    let error_span = if *operator == crate::UnaryOp::Neg {
                        unary_operator_span(span, expression_operand_span(expression))
                    } else {
                        span
                    };
                    self.integer_error(error, error_span)
                })
            }
            CHirExpr::EvaluateOnce(operand, _) => {
                let previous = self.nested_mutation_depth;
                self.nested_mutation_depth = previous.saturating_add(1);
                let result = self.eval_expr(operand, true);
                self.nested_mutation_depth = previous;
                result
            }
            CHirExpr::BuiltinExpect(value, expected, _) => {
                let value = self.eval_expr(value, true)?;
                self.eval_expr(expected, true)?;
                Ok(value)
            }
            CHirExpr::Binary(operator, left, right, _) => {
                self.eval_binary(*operator, left, right, span)
            }
            CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
                let then_type = self.infer_integer_type(then_expression)?;
                let else_type = self.infer_integer_type(else_expression)?;
                let result_type = usual_arithmetic_type(
                    CInteger::new(0, then_type).promoted().ty(),
                    CInteger::new(0, else_type).promoted().ty(),
                );
                let selected = if self.eval_expr(condition, true)?.truth() {
                    self.eval_expr(then_expression, true)?
                } else {
                    self.eval_expr(else_expression, true)?
                };
                Ok(selected.cast(result_type))
            }
            CHirExpr::Elvis(operand, else_expression, _) => {
                let operand_type = self.infer_integer_type(operand)?;
                let else_type = self.infer_integer_type(else_expression)?;
                let result_type = usual_arithmetic_type(
                    CInteger::new(0, operand_type).promoted().ty(),
                    CInteger::new(0, else_type).promoted().ty(),
                );
                // The omitted middle operand does not turn the controlling
                // expression into an unevaluated or mutation-free context.
                let operand_value = self.eval_expr(operand, true)?;
                let selected = if operand_value.truth() {
                    operand_value
                } else {
                    self.eval_expr(else_expression, true)?
                };
                Ok(selected.cast(result_type))
            }
            CHirExpr::BitBuiltin { op, operand, .. } => {
                let operand = self.eval_expr(operand, allow_root_mutation)?;
                eval_bit_builtin(*op, operand)
                    .ok_or(InterpreterError::BitBuiltinZeroOperand { span })
            }
            CHirExpr::BuiltinUnreachable { .. } => {
                Err(InterpreterError::UnreachableReached { span })
            }
            CHirExpr::BuiltinTrap { .. } => Err(InterpreterError::BuiltinTrapReached { span }),
            CHirExpr::BuiltinReturnAddress { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "return-address builtin",
                span,
            }),
            CHirExpr::BuiltinFrameAddress { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "frame-address builtin",
                span,
            }),
            CHirExpr::BuiltinVarArgs { .. } => Err(InterpreterError::VarArgs { span }),
            CHirExpr::LabelAddress { .. } => Err(InterpreterError::ComputedGoto { span }),
            CHirExpr::CharLit(literal, _) => Ok(literal.integer_value()),
            CHirExpr::Cast(ty, value, _) => {
                let target = self.integer_type(*ty, span)?;
                Ok(self.eval_expr(value, allow_root_mutation)?.cast(target))
            }
            CHirExpr::SizeOf(ty, _) | CHirExpr::SizeOfExpr(_, ty, _) => {
                let size = self
                    .target
                    .sizeof_type(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "sizeof incomplete or unsupported type",
                        span,
                    })?;
                Ok(CInteger::new(size, CIntegerType::ULONG))
            }
            CHirExpr::AlignOf(ty, _) => {
                let alignment = self
                    .target
                    .alignof_type(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "alignof incomplete or unsupported type",
                        span,
                    })?;
                Ok(CInteger::new(u64::from(alignment), CIntegerType::ULONG))
            }
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                facts,
                ..
            } if self.mutation_allowed(allow_root_mutation) => {
                if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                    return Err(InterpreterError::InvalidScalarPlace {
                        symbol: place.symbol(),
                        span,
                    });
                }
                self.eval_assignment(*op, *place, target, value, span)
            }
            CHirExpr::Update {
                op,
                place,
                target,
                facts,
                ..
            } if self.mutation_allowed(allow_root_mutation) => {
                if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                    return Err(InterpreterError::InvalidScalarPlace {
                        symbol: place.symbol(),
                        span,
                    });
                }
                self.eval_update(*op, *place, target, span)
            }
            CHirExpr::Comma(left, right, _) => {
                self.eval_expr(left, false)?;
                self.eval_expr(right, false)
            }
            CHirExpr::FloatLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::Call(_, _, _)
            | CHirExpr::CheckedArithmetic { .. }
            | CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _)
            | CHirExpr::StatementExpr { .. }
            | CHirExpr::CompoundLit(_, _, _)
            | CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::MemoryRead { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "outside scalar Day-5 expression subset",
                span,
            }),
        }
    }

    fn infer_integer_type(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<CIntegerType, InterpreterError> {
        enum TypeTask<'a> {
            Infer(&'a CHirExpr<'a>),
            Cache(usize),
            Promote,
            Common,
        }
        // Type inference is unevaluated but still must not consume the host
        // call stack. Cache immutable expression identities so nested selected
        // conditionals do not repeatedly walk their entire remaining subtree.
        let mut pending = vec![TypeTask::Infer(expression)];
        let mut values = Vec::new();
        while let Some(task) = pending.pop() {
            let expression = match task {
                TypeTask::Cache(key) => {
                    self.integer_expression_types
                        .insert(key, *values.last().expect("inferred type"));
                    continue;
                }
                TypeTask::Promote => {
                    let ty = values.pop().expect("unary operand type");
                    values.push(CInteger::new(0, ty).promoted().ty());
                    continue;
                }
                TypeTask::Common => {
                    let right = values.pop().expect("right operand type");
                    let left = values.pop().expect("left operand type");
                    values.push(usual_arithmetic_type(
                        CInteger::new(0, left).promoted().ty(),
                        CInteger::new(0, right).promoted().ty(),
                    ));
                    continue;
                }
                TypeTask::Infer(expression) => expression,
            };
            let key = expression as *const CHirExpr<'arena> as usize;
            if let Some(ty) = self.integer_expression_types.get(&key) {
                values.push(*ty);
                continue;
            }
            pending.push(TypeTask::Cache(key));
            let span = expression.span();
            let immediate = match expression {
                CHirExpr::Atomic(access) => self.integer_type(access.facts.ty(), span),
                CHirExpr::Floating(access) => self.integer_type(access.facts.ty(), span),
                CHirExpr::Vector(access) => self.integer_type(access.facts.ty(), span),
                CHirExpr::Complex(access) => self.integer_type(access.facts.ty(), span),
                CHirExpr::Lit(literal, _) => integer_literal_value(*literal)
                    .map(|value| value.ty())
                    .map_err(|error| self.integer_error(error, span)),
                CHirExpr::CharLit(literal, _) => Ok(literal.integer_value().ty()),
                CHirExpr::Ident { facts, .. }
                | CHirExpr::BuiltinConstantP { facts, .. }
                | CHirExpr::BuiltinObjectSize { facts, .. }
                | CHirExpr::CheckedArithmetic { facts, .. }
                | CHirExpr::BitBuiltin { facts, .. }
                | CHirExpr::Assign { facts, .. }
                | CHirExpr::MemoryAssign { facts, .. }
                | CHirExpr::MemoryRead { facts, .. }
                | CHirExpr::Update { facts, .. }
                | CHirExpr::MemoryUpdate { facts, .. }
                | CHirExpr::StatementExpr { facts, .. } => self.integer_type(facts.ty(), span),
                CHirExpr::Unary(operator, operand, _) => match operator {
                    crate::UnaryOp::Not => Ok(CIntegerType::INT),
                    crate::UnaryOp::Plus
                    | crate::UnaryOp::Neg
                    | crate::UnaryOp::Abs
                    | crate::UnaryOp::BitNot => {
                        pending.push(TypeTask::Promote);
                        pending.push(TypeTask::Infer(operand));
                        continue;
                    }
                    crate::UnaryOp::Deref => {
                        // Conditional conversions inspect both arm types, not
                        // their values. Resolve the pointee without accessing
                        // the object or evaluating the pointer expression.
                        let pointer = self.pointer_type_of_expression(operand)?;
                        self.integer_type(pointer.pointee(), span)
                    }
                    crate::UnaryOp::AddrOf => Err(InterpreterError::UnsupportedExpression {
                        feature: "non-integer unary operand",
                        span,
                    }),
                },
                CHirExpr::EvaluateOnce(operand, _) => {
                    pending.push(TypeTask::Infer(operand));
                    continue;
                }
                CHirExpr::Binary(operator, left, right, _) => match operator {
                    BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::And
                    | BinaryOp::Or => Ok(CIntegerType::INT),
                    BinaryOp::Shl | BinaryOp::Shr => {
                        pending.push(TypeTask::Promote);
                        pending.push(TypeTask::Infer(left));
                        continue;
                    }
                    BinaryOp::Comma => {
                        pending.push(TypeTask::Infer(right));
                        continue;
                    }
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor => {
                        pending.push(TypeTask::Common);
                        pending.push(TypeTask::Infer(right));
                        pending.push(TypeTask::Infer(left));
                        continue;
                    }
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
                    | BinaryOp::BitXorAssign => Err(InterpreterError::UnsupportedExpression {
                        feature: "unlowered assignment binary operator",
                        span,
                    }),
                },
                CHirExpr::Ternary(_, left, right, _) | CHirExpr::Elvis(left, right, _) => {
                    pending.push(TypeTask::Common);
                    pending.push(TypeTask::Infer(right));
                    pending.push(TypeTask::Infer(left));
                    continue;
                }
                CHirExpr::Cast(ty, _, _) => self.integer_type(*ty, span),
                CHirExpr::BuiltinExpect(..) => Ok(CIntegerType::LONG),
                CHirExpr::Call(callee, _, _) => {
                    let function_type = self.call_designator_function_type(callee).ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "indirect scalar function call",
                            span: callee.span(),
                        },
                    )?;
                    let Some(TypeKind::Function { return_type, .. }) =
                        self.unit.types().get(function_type).map(|ty| &ty.kind)
                    else {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "inconsistent function designator",
                            span: callee.span(),
                        });
                    };
                    self.integer_type(*return_type, span)
                }
                CHirExpr::SizeOf(_, _)
                | CHirExpr::SizeOfExpr(_, _, _)
                | CHirExpr::AlignOf(_, _) => Ok(CIntegerType::ULONG),
                CHirExpr::Comma(_, right, _) => {
                    pending.push(TypeTask::Infer(right));
                    continue;
                }
                CHirExpr::FloatLit(_, _)
                | CHirExpr::StringLit(_, _)
                | CHirExpr::LabelAddress { .. }
                | CHirExpr::BuiltinTrap { .. }
                | CHirExpr::BuiltinUnreachable { .. }
                | CHirExpr::BuiltinReturnAddress { .. }
                | CHirExpr::BuiltinFrameAddress { .. }
                | CHirExpr::BuiltinVarArgs { .. }
                | CHirExpr::Member(_, _, _)
                | CHirExpr::PtrMember(_, _, _)
                | CHirExpr::Index(_, _, _)
                | CHirExpr::CompoundLit(_, _, _) => Err(InterpreterError::UnsupportedExpression {
                    feature: "non-integer expression type",
                    span,
                }),
            }?;
            values.push(immediate);
        }
        Ok(values.pop().expect("root integer expression type"))
    }

    fn eval_binary(
        &mut self,
        operator: BinaryOp,
        left: &'arena CHirExpr<'arena>,
        right: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        match operator {
            BinaryOp::And => {
                let left = self.eval_expr(left, true)?;
                if !left.truth() {
                    Ok(CInteger::int(0))
                } else {
                    Ok(CInteger::int(i32::from(
                        self.eval_expr(right, true)?.truth(),
                    )))
                }
            }
            BinaryOp::Or => {
                let left = self.eval_expr(left, true)?;
                if left.truth() {
                    Ok(CInteger::int(1))
                } else {
                    Ok(CInteger::int(i32::from(
                        self.eval_expr(right, true)?.truth(),
                    )))
                }
            }
            BinaryOp::Comma => {
                self.eval_expr(left, false)?;
                self.eval_expr(right, false)
            }
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
            | BinaryOp::BitXorAssign => Err(InterpreterError::UnsupportedExpression {
                feature: "unlowered assignment binary operator",
                span,
            }),
            _ => {
                let left = self.eval_expr(left, false)?;
                let right = self.eval_expr(right, false)?;
                if self.static_integer_evaluation {
                    eval_constant_binary_integer(operator, left, right)
                        .map_err(|error| self.integer_error(error, span))
                } else {
                    self.runtime_integer_binary(operator, left, right, span)
                }
            }
        }
    }

    fn validate_place(
        &self,
        place: ScalarPlace,
        target: &CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<SymbolId, InterpreterError> {
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span: target_span,
            ..
        } = target
        else {
            return Err(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span,
            });
        };
        let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
            InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            }
        })?;
        if is_global_register(self.unit, *symbol) {
            return Err(InterpreterError::Asm { span: *target_span });
        }
        let slot = self
            .slots
            .get(symbol)
            .ok_or(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            })?;
        if !slot.active {
            return Err(InterpreterError::OutOfScopeObject {
                symbol: *symbol,
                spelling: declaration.spelling().to_owned(),
                span: *target_span,
            });
        }
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
            || canonical_integer_type(self.unit.types(), place.ty()) != Some(slot.ty)
        {
            return Err(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            });
        }
        Ok(*symbol)
    }

    fn eval_assignment(
        &mut self,
        operator: AssignmentOp,
        place: ScalarPlace,
        target: &CHirExpr<'arena>,
        value: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        let symbol = self.validate_place(place, target, span)?;
        let right = self.eval_expr(value, false)?;
        self.apply_assignment_value(operator, symbol, right, target.span(), span)
    }

    fn apply_assignment_value(
        &mut self,
        operator: AssignmentOp,
        symbol: SymbolId,
        right: CInteger,
        target_span: SourceSpan,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        let slot = *self
            .slots
            .get(&symbol)
            .ok_or(InterpreterError::InvalidScalarPlace { symbol, span })?;
        let result = if operator == AssignmentOp::Simple {
            right.cast(slot.ty)
        } else {
            let left = slot.value.ok_or_else(|| {
                let spelling = self
                    .unit
                    .symbols()
                    .symbol(symbol)
                    .map(|symbol| symbol.spelling().to_owned())
                    .unwrap_or_else(|_| format!("symbol#{}", symbol.raw()));
                InterpreterError::ReadOfUninitializedObject {
                    symbol,
                    spelling,
                    span: target_span,
                }
            })?;
            let binary =
                assignment_binary(operator).ok_or(InterpreterError::UnsupportedExpression {
                    feature: "invalid compound assignment operator",
                    span,
                })?;
            self.runtime_integer_binary(binary, left, right, span)?
                .cast(slot.ty)
        };
        self.slots
            .get_mut(&symbol)
            .expect("validated scalar place has a slot")
            .value = Some(result);
        Ok(result)
    }

    fn eval_update(
        &mut self,
        operator: UpdateOp,
        place: ScalarPlace,
        target: &CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        let symbol = self.validate_place(place, target, span)?;
        let slot = *self
            .slots
            .get(&symbol)
            .ok_or(InterpreterError::InvalidScalarPlace { symbol, span })?;
        let previous = slot.value.ok_or_else(|| {
            let spelling = self
                .unit
                .symbols()
                .symbol(symbol)
                .map(|symbol| symbol.spelling().to_owned())
                .unwrap_or_else(|_| format!("symbol#{}", symbol.raw()));
            InterpreterError::ReadOfUninitializedObject {
                symbol,
                spelling,
                span: target.span(),
            }
        })?;
        let binary = match operator {
            UpdateOp::PreIncrement | UpdateOp::PostIncrement => BinaryOp::Add,
            UpdateOp::PreDecrement | UpdateOp::PostDecrement => BinaryOp::Sub,
        };
        let updated = self
            .runtime_integer_binary(binary, previous, CInteger::int(1), span)?
            .cast(slot.ty);
        self.slots
            .get_mut(&symbol)
            .expect("validated scalar place has a slot")
            .value = Some(updated);
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                updated
            } else {
                previous
            },
        )
    }
}

/// Memory obligations of everything reachable from one entry function.
///
/// Reachability is transitive: a global touched only by a callee still forces
/// memory mode and still has to be allocated before execution starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MemoryRequirements {
    /// Some reachable body needs the object-memory machine.
    requires_memory: bool,
    /// Every object symbol named by a reachable body, closed over the
    /// initializers of the objects already collected. `initialize_static_objects`
    /// keeps exactly the subset with static storage duration.
    static_objects: BTreeSet<SymbolId>,
}

/// Symbols named by one traversal, before they are classified.
#[derive(Debug, Default)]
struct ReferencedSymbols {
    named: Vec<SymbolId>,
    labels: BTreeMap<usize, String>,
}

impl ReferencedSymbols {
    fn record(&mut self, symbol: SymbolId) {
        self.named.push(symbol);
    }
}

/// File-scope register bindings execute as assembly, not object accesses.
fn is_global_register(unit: &CHirUnit<'_>, symbol: SymbolId) -> bool {
    unit.symbols().symbol(symbol).is_ok_and(|declaration| {
        declaration.storage_class() == Some(StorageClass::Register)
            && unit
                .symbols()
                .scope(declaration.scope())
                .is_ok_and(|scope| scope.kind() == ScopeKind::File)
    })
}

/// Does this symbol name an object that lives for the whole execution?
///
/// Shared by requirement discovery and static allocation.
fn has_static_storage_duration(unit: &CHirUnit<'_>, symbol: SymbolId) -> bool {
    let Ok(declaration) = unit.symbols().symbol(symbol) else {
        return false;
    };
    if declaration.kind() != SymbolKind::Object {
        return false;
    }
    let file_scope = unit
        .symbols()
        .is_bound_in_scope(symbol, unit.symbols().file_scope())
        .unwrap_or(false);
    file_scope || declaration.storage_class().map(StorageClass::base) == Some(StorageClass::Static)
}

fn reachable_memory_requirements(
    unit: &CHirUnit<'_>,
    entry: SymbolId,
    linkage: &crate::linkage::SymbolLinkage,
) -> MemoryRequirements {
    let mut requires_memory = false;
    let mut visited_functions: BTreeSet<SymbolId> = BTreeSet::new();
    let mut static_objects: BTreeSet<SymbolId> = BTreeSet::new();
    let mut pending_functions: Vec<SymbolId> = vec![linkage.target(entry)];
    let mut pending_objects: Vec<SymbolId> = Vec::new();

    while !pending_functions.is_empty() || !pending_objects.is_empty() {
        while let Some(function) = pending_functions.pop() {
            if !visited_functions.insert(function) {
                continue;
            }
            let Some(body) = reachable_function_body(unit, function) else {
                continue;
            };
            if statement_requires_memory(unit, body) {
                requires_memory = true;
            }
            let mut referenced = ReferencedSymbols::default();
            collect_statement_symbols(body, &mut referenced);
            classify_referenced_symbols(
                unit,
                linkage,
                referenced,
                &mut pending_functions,
                &mut static_objects,
                &mut pending_objects,
            );
        }

        while let Some(object) = pending_objects.pop() {
            // Function-static initializers were already visited with their
            // declaration by collect_statement_symbols. Only file objects
            // need this separate initializer-closure lookup.
            let Some(declaration) = find_storage_object_declaration(unit, object, &[]) else {
                continue;
            };
            let Some(initializer) = &declaration.initializer else {
                continue;
            };
            let mut referenced = ReferencedSymbols::default();
            collect_initializer_symbols(initializer.plan(), &mut referenced);
            classify_referenced_symbols(
                unit,
                linkage,
                referenced,
                &mut pending_functions,
                &mut static_objects,
                &mut pending_objects,
            );
        }
    }

    // Allocating a static object and running without the memory machine are
    // contradictory, so the two outputs are reconciled here rather than left to
    // agree by accident. A `static` local is exactly the case a purely
    // syntactic scan misses.
    let requires_memory = requires_memory
        || static_objects
            .iter()
            .any(|symbol| has_static_storage_duration(unit, *symbol));

    MemoryRequirements {
        requires_memory,
        static_objects,
    }
}

/// Route each named symbol to the worklist that owns it. Unknown symbols are
/// dropped here; every consumer re-resolves them and fails closed on its own.
fn classify_referenced_symbols(
    unit: &CHirUnit<'_>,
    linkage: &crate::linkage::SymbolLinkage,
    referenced: ReferencedSymbols,
    pending_functions: &mut Vec<SymbolId>,
    static_objects: &mut BTreeSet<SymbolId>,
    pending_objects: &mut Vec<SymbolId>,
) {
    for symbol in referenced.named {
        let symbol = linkage.target(symbol);
        let Ok(declaration) = unit.symbols().symbol(symbol) else {
            continue;
        };
        match declaration.kind() {
            SymbolKind::Function => pending_functions.push(symbol),
            SymbolKind::Object => {
                if static_objects.insert(symbol) {
                    pending_objects.push(symbol);
                }
            }
            SymbolKind::Parameter | SymbolKind::Typedef | SymbolKind::Enumerator => {}
        }
    }
}

// Builtin prototypes have their own semantic identity, but an ordinary call
// still links to a compatible definition of the library name in this TU.
fn linked_library_definition(unit: &CHirUnit<'_>, function: SymbolId) -> SymbolId {
    if !unit.symbols().is_builtin_function(function) {
        return function;
    }
    let Ok(prototype) = unit.symbols().symbol(function) else {
        return function;
    };
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(definition)
                if definition.body.is_some()
                    && definition.name.spelling == prototype.spelling()
                    && unit.types().compatible(definition.ty, prototype.ty()) =>
            {
                Some(definition.symbol)
            }
            _ => None,
        })
        .unwrap_or(function)
}

fn reachable_function_body<'arena>(
    unit: &CHirUnit<'arena>,
    function: SymbolId,
) -> Option<&'arena CHirStmt<'arena>> {
    let function = linked_library_definition(unit, function);
    unit.iter().find_map(|declaration| match declaration {
        CHirDecl::Function(FunctionDecl { symbol, body, .. }) if *symbol == function => *body,
        CHirDecl::Function(_)
        | CHirDecl::Object(_)
        | CHirDecl::Asm(_)
        | CHirDecl::Tag(_)
        | CHirDecl::Typedef(_) => None,
    })
}

/// Follow only recursive object-initializer edges. Integer expressions are
/// task-machine work, and peeling consecutive casts is an explicit loop.
fn push_static_initializer_children<'arena>(
    expression: &'arena CHirExpr<'arena>,
    depth: usize,
    pending: &mut Vec<(&'arena CHirExpr<'arena>, usize)>,
) {
    if let CHirExpr::Cast(_, operand, _) = expression {
        pending.push((operand, depth));
        return;
    }
    let child_depth = depth.saturating_add(1);
    let mut push = |child: &'arena CHirExpr<'arena>| pending.push((child, child_depth));
    match expression {
        CHirExpr::Unary(UnaryOp::AddrOf | UnaryOp::Deref, operand, _)
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => push(operand),
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    push(value);
                }
            }
        }
        CHirExpr::MemoryRead { place, .. } => {
            for source in place.source_expressions() {
                push(source);
            }
        }
        _ => {}
    }
}

fn collect_initializer_symbols<'arena>(
    plan: &crate::InitPlan<'arena>,
    output: &mut ReferencedSymbols,
) {
    for action in plan.actions() {
        if let InitAction::StoreScalar { value, .. } | InitAction::CopyAggregate { value, .. } =
            action
        {
            collect_expression_symbols(value, output);
        }
    }
}

fn collect_declaration_symbols<'arena>(
    declaration: &'arena CHirDecl<'arena>,
    output: &mut ReferencedSymbols,
) {
    match declaration {
        CHirDecl::Object(object) => {
            output.record(object.symbol);
            if let Some(cleanup) = object.cleanup {
                collect_expression_symbols(cleanup, output);
            }
            if let Some(initializer) = &object.initializer {
                collect_initializer_symbols(initializer.plan(), output);
            }
        }
        CHirDecl::Function(function) => {
            output.record(function.symbol);
            if let Some(body) = function.body {
                collect_statement_symbols(body, output);
            }
        }
        CHirDecl::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_expression_symbols(operand.expression, output);
            }
        }
        CHirDecl::Tag(_) | CHirDecl::Typedef(_) => {}
    }
}

fn collect_statement_symbols<'arena>(
    statement: &'arena CHirStmt<'arena>,
    output: &mut ReferencedSymbols,
) {
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_expression_symbols(operand.expression, output);
            }
        }
        CHirStmt::Expr(expression, _) => collect_expression_symbols(expression, output),
        CHirStmt::ComputedGoto(expression, _) => {
            collect_expression_symbols(expression, output);
        }
        CHirStmt::Return(expression, _) => {
            if let Some(expression) = expression {
                collect_expression_symbols(expression, output);
            }
        }
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            collect_expression_symbols(condition, output);
            collect_statement_symbols(then_statement, output);
            if let Some(else_statement) = else_statement {
                collect_statement_symbols(else_statement, output);
            }
        }
        CHirStmt::Switch(expression, body, _) | CHirStmt::While(expression, body, _) => {
            collect_expression_symbols(expression, output);
            collect_statement_symbols(body, output);
        }
        CHirStmt::DoWhile(body, expression, _) => {
            collect_statement_symbols(body, output);
            collect_expression_symbols(expression, output);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => {
                    collect_expression_symbols(expression, output);
                }
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        collect_declaration_symbols(declaration, output);
                    }
                }
            }
            if let Some(condition) = condition {
                collect_expression_symbols(condition, output);
            }
            if let Some(step) = step {
                collect_expression_symbols(step, output);
            }
            collect_statement_symbols(body, output);
        }
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        collect_declaration_symbols(declaration, output);
                    }
                    CHirBlockItem::Statement(statement) => {
                        collect_statement_symbols(statement, output);
                    }
                }
            }
        }
        CHirStmt::Label(_, child, _) | CHirStmt::Default(child, _) => {
            collect_statement_symbols(child, output);
        }
        CHirStmt::Case(expression, child, _) => {
            collect_expression_symbols(expression, output);
            collect_statement_symbols(child, output);
        }
        CHirStmt::CaseRange(first, last, child, _) => {
            collect_expression_symbols(first, output);
            collect_expression_symbols(last, output);
            collect_statement_symbols(child, output);
        }
        CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

fn collect_expression_symbols<'arena>(
    expression: &'arena CHirExpr<'arena>,
    output: &mut ReferencedSymbols,
) {
    match expression {
        CHirExpr::Complex(access) => {
            for operand in &access.operands {
                collect_expression_symbols(operand, output);
            }
        }
        CHirExpr::Floating(access) => {
            for operand in &access.operands {
                collect_expression_symbols(operand, output);
            }
        }
        CHirExpr::Vector(access) => {
            for operand in &access.operands {
                collect_expression_symbols(operand, output);
            }
        }
        CHirExpr::Atomic(access) => {
            for operand in access.operands() {
                collect_expression_symbols(operand, output);
            }
        }
        CHirExpr::LabelAddress { label, .. } => {
            output.labels.insert(
                expression as *const CHirExpr<'_> as usize,
                label.spelling.clone(),
            );
        }
        CHirExpr::Ident { symbol, .. } => output.record(*symbol),
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => collect_expression_symbols(operand, output),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            collect_expression_symbols(left, output);
            collect_expression_symbols(right, output);
        }
        CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
            collect_expression_symbols(condition, output);
            collect_expression_symbols(then_expression, output);
            collect_expression_symbols(else_expression, output);
        }
        CHirExpr::Call(callee, arguments, _) => {
            collect_expression_symbols(callee, output);
            for argument in arguments {
                collect_expression_symbols(argument, output);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            collect_expression_symbols(lhs, output);
            collect_expression_symbols(rhs, output);
            collect_expression_symbols(target, output);
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            collect_expression_symbols(first, output);
            if let Some(second) = second {
                collect_expression_symbols(second, output);
            }
        }
        CHirExpr::StatementExpr { body, .. } => collect_statement_symbols(body, output),
        CHirExpr::CompoundLit(_, initializer, _) => {
            collect_initializer_symbols(initializer.plan(), output);
        }
        CHirExpr::Assign { target, value, .. } => {
            collect_expression_symbols(target, output);
            collect_expression_symbols(value, output);
        }
        CHirExpr::Update { target, .. } => collect_expression_symbols(target, output),
        CHirExpr::MemoryAssign { place, value, .. } => {
            collect_place_symbols(place, output);
            collect_expression_symbols(value, output);
        }
        CHirExpr::MemoryUpdate { place, .. } | CHirExpr::MemoryRead { place, .. } => {
            collect_place_symbols(place, output);
        }
        CHirExpr::Lit(_, _)
        | CHirExpr::FloatLit(_, _)
        | CHirExpr::CharLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::SizeOf(_, _)
        | CHirExpr::SizeOfExpr(_, _, _)
        | CHirExpr::BuiltinConstantP { .. }
        | CHirExpr::BuiltinObjectSize { .. }
        | CHirExpr::AlignOf(_, _)
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => {}
    }
}

fn collect_place_symbols<'arena>(place: &MemoryPlace<'arena>, output: &mut ReferencedSymbols) {
    if let PlaceRoot::Object { symbol } = place.root() {
        output.record(*symbol);
    }
    for expression in place.source_expressions() {
        collect_expression_symbols(expression, output);
    }
}

/// Whether execution needs object state, including places rooted at integer casts.
pub fn statement_requires_memory(unit: &CHirUnit<'_>, statement: &CHirStmt<'_>) -> bool {
    match statement {
        CHirStmt::Asm(_) => true,
        CHirStmt::Expr(expression, _) => expression_requires_memory(unit, expression),
        CHirStmt::ComputedGoto(_, _) => true,
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            expression_requires_memory(unit, condition)
                || statement_requires_memory(unit, then_statement)
                || else_statement
                    .is_some_and(|statement| statement_requires_memory(unit, statement))
        }
        CHirStmt::Switch(expression, body, _) | CHirStmt::While(expression, body, _) => {
            expression_requires_memory(unit, expression) || statement_requires_memory(unit, body)
        }
        CHirStmt::DoWhile(body, condition, _) => {
            statement_requires_memory(unit, body) || expression_requires_memory(unit, condition)
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            let initializer_requires_memory = match initializer {
                CHirForInit::Empty => false,
                CHirForInit::Expression(expression) => expression_requires_memory(unit, expression),
                CHirForInit::Declaration(declarations) => declarations
                    .iter()
                    .any(|declaration| declaration_requires_memory(unit, declaration)),
            };
            initializer_requires_memory
                || condition.is_some_and(|expression| expression_requires_memory(unit, expression))
                || step.is_some_and(|expression| expression_requires_memory(unit, expression))
                || statement_requires_memory(unit, body)
        }
        CHirStmt::Return(expression, _) => {
            expression.is_some_and(|expression| expression_requires_memory(unit, expression))
        }
        CHirStmt::Compound(items, _, _) => items.iter().any(|item| match item {
            CHirBlockItem::Declaration(declaration) => {
                declaration_requires_memory(unit, declaration)
            }
            CHirBlockItem::Statement(statement) => statement_requires_memory(unit, statement),
        }),
        CHirStmt::Label(_, child, _)
        | CHirStmt::Case(_, child, _)
        | CHirStmt::CaseRange(_, _, child, _)
        | CHirStmt::Default(child, _) => statement_requires_memory(unit, child),
        CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => false,
    }
}

fn declaration_requires_memory(unit: &CHirUnit<'_>, declaration: &CHirDecl<'_>) -> bool {
    let CHirDecl::Object(declaration) = declaration else {
        return false;
    };
    declaration.cleanup.is_some()
        || type_requires_memory(unit.types(), declaration.ty)
        || declaration.initializer.as_ref().is_some_and(|initializer| {
            initializer
                .plan()
                .actions()
                .iter()
                .any(|action| match action {
                    InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } => {
                        expression_requires_memory(unit, value)
                    }
                    InitAction::StoreString { .. } => true,
                    InitAction::ZeroScalar { .. } => false,
                })
        })
}

fn type_requires_memory(types: &TypeInterner, ty: TypeId) -> bool {
    types.get(ty).is_none_or(|canonical| {
        canonical.qualifiers.is_volatile
            || canonical.qualifiers.is_atomic
            || !matches!(canonical.kind, TypeKind::Int(_) | TypeKind::Enum(_))
    })
}

fn expression_requires_memory(unit: &CHirUnit<'_>, expression: &CHirExpr<'_>) -> bool {
    if unit.stack_allocation_size(expression).is_some() {
        return true;
    }
    match expression {
        CHirExpr::Atomic(_)
        | CHirExpr::Vector(_)
        | CHirExpr::Complex(_)
        | CHirExpr::Floating(_)
        // The legacy integer-only evaluator cannot execute an unfolderable
        // floating literal/cast. Use the typed task machine even if no object
        // is read, so invalid conversions retain their E3063 obligation.
        | CHirExpr::FloatLit(_, _) => true,
        CHirExpr::BuiltinConstantP { .. } | CHirExpr::BuiltinObjectSize { .. } => false,
        // A file-scope *object* has static storage and therefore lives in
        // memory. A parameter can never legitimately be declared at file scope,
        // so it is left to the signature validator instead of silently forcing
        // memory mode and masking that diagnostic.
        CHirExpr::Ident { symbol, facts, .. } => {
            unit.symbols()
                .symbol(*symbol)
                .ok()
                .is_some_and(|declaration| {
                    declaration.kind() == SymbolKind::Function
                        || matches!(
                            declaration.kind(),
                            SymbolKind::Object | SymbolKind::Parameter
                        ) && (has_static_storage_duration(unit, *symbol)
                            || type_requires_memory(unit.types(), facts.ty()))
                })
        }
        CHirExpr::Unary(UnaryOp::AddrOf | UnaryOp::Deref, _, _)
        | CHirExpr::MemoryAssign { .. }
        | CHirExpr::MemoryRead { .. }
        | CHirExpr::MemoryUpdate { .. }
        | CHirExpr::StringLit(_, _)
        | CHirExpr::Member(_, _, _)
        | CHirExpr::PtrMember(_, _, _)
        | CHirExpr::Index(_, _, _)
        | CHirExpr::CompoundLit(_, _, _)
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => true,
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::BitBuiltin { operand, .. } => expression_requires_memory(unit, operand),
        CHirExpr::Cast(ty, operand, _) => {
            type_requires_memory(unit.types(), *ty) || expression_requires_memory(unit, operand)
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            expression_requires_memory(unit, left) || expression_requires_memory(unit, right)
        }
        CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
            expression_requires_memory(unit, condition)
                || expression_requires_memory(unit, then_expression)
                || expression_requires_memory(unit, else_expression)
        }
        CHirExpr::Call(callee, arguments, _) => {
            (!matches!(callee, CHirExpr::Ident { facts, .. } if facts.category() == ExprCategory::FunctionDesignator)
                && PointerTypeDesc::from_expression(unit.symbols(), unit.types(), callee)
                    .is_ok_and(|p| {
                        matches!(
                            unit.types().get(p.pointee()).map(|t| &t.kind),
                            Some(TypeKind::Function { .. })
                        )
                    })
                && expression_requires_memory(unit, callee))
                || arguments
                    .iter()
                    .any(|argument| expression_requires_memory(unit, argument))
        }
        // The result pointer is written even when the mathematical operation
        // does not overflow, so checked arithmetic always selects the explicit
        // memory task machine.
        CHirExpr::CheckedArithmetic { .. } => true,
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            expression_requires_memory(unit, first)
                || second.is_some_and(|second| expression_requires_memory(unit, second))
        }
        CHirExpr::StatementExpr { body, .. } => statement_requires_memory(unit, body),
        CHirExpr::Assign { target, value, .. } => {
            expression_requires_memory(unit, target) || expression_requires_memory(unit, value)
        }
        CHirExpr::Update { target, .. } => expression_requires_memory(unit, target),
        CHirExpr::Lit(_, _)
        | CHirExpr::CharLit(_, _)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(_, _)
        | CHirExpr::SizeOfExpr(_, _, _)
        | CHirExpr::AlignOf(_, _)
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. } => false,
    }
}

fn find_storage_object_declaration<'arena>(
    unit: &CHirUnit<'arena>,
    symbol: SymbolId,
    function_statics: &[&'arena ObjectDecl<'arena>],
) -> Option<ObjectDecl<'arena>> {
    let declarations: Vec<_> = unit
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object) => Some(object),
            _ => None,
        })
        .chain(function_statics.iter().copied())
        .filter(|object| object.symbol == symbol)
        .collect();
    declarations
        .iter()
        .find(|declaration| declaration.initializer.is_some())
        .or_else(|| {
            declarations
                .iter()
                .find(|declaration| declaration.storage.base() != StorageClass::Extern)
        })
        .map(|object| (*object).clone())
}

fn pop_machine_value(
    values: &mut Vec<CValue>,
    span: SourceSpan,
) -> Result<CValue, InterpreterError> {
    values.pop().ok_or(InterpreterError::UnsupportedExpression {
        feature: "malformed interpreter expression continuation",
        span,
    })
}

/// Truth of a controlling expression.
///
/// C11 6.5.13 and the statement forms compare the operand against zero, and a
/// pointer compares against the null pointer, so a pointer condition is well
/// formed rather than an integer-only operation.
fn machine_truth(value: CValue, _span: SourceSpan) -> Result<bool, InterpreterError> {
    match value {
        CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span: _span }),
        CValue::Integer(value) => Ok(value.truth()),
        CValue::Floating(value) => Ok(!value.is_zero()),
        CValue::Pointer { value, .. } => Ok(value != Pointer::NULL),
        CValue::FunctionPointer { value, .. } => Ok(value != PointerValue::Null),
        CValue::Aggregate { .. } => Err(InterpreterError::UnsupportedExpression {
            feature: "aggregate controlling expression",
            span: _span,
        }),
    }
}

fn machine_equality_result(operator: BinaryOp, equal: bool) -> CValue {
    CValue::Integer(CInteger::int(i32::from(if operator == BinaryOp::Eq {
        equal
    } else {
        !equal
    })))
}

fn expect_machine_integer(value: CValue, span: SourceSpan) -> Result<CInteger, InterpreterError> {
    match value {
        CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span }),
        CValue::Integer(value) => Ok(value),
        CValue::Floating(_)
        | CValue::Pointer { .. }
        | CValue::FunctionPointer { .. }
        | CValue::Aggregate { .. } => Err(InterpreterError::UnsupportedExpression {
            feature: "pointer value in integer task-machine operation",
            span,
        }),
    }
}

fn expect_machine_pointer(
    value: CValue,
    span: SourceSpan,
) -> Result<(Pointer, PointerTypeDesc), InterpreterError> {
    match value {
        CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span }),
        CValue::Pointer { value, ty } => Ok((value, ty)),
        CValue::Floating(_)
        | CValue::Integer(_)
        | CValue::FunctionPointer { .. }
        | CValue::Aggregate { .. } => Err(InterpreterError::UnsupportedExpression {
            feature: "integer value in pointer task-machine operation",
            span,
        }),
    }
}

fn cast_machine_value(
    value: CValue,
    target: ValueType,
    span: SourceSpan,
    types: &TypeInterner,
    memory: &mut MemoryState,
) -> Result<CValue, InterpreterError> {
    match (value, target) {
        (CValue::Uninitialized(_), _) => Err(InterpreterError::UninitializedValue { span }),
        (value, ValueType::Floating(kind)) => {
            floating::convert(value, kind, span).map(CValue::Floating)
        }
        (CValue::Floating(value), ValueType::Integer(target)) => {
            let raw = if target == CIntegerType::BOOL {
                u128::from(!value.is_zero())
            } else {
                value
                    .to_integer(u16::from(target.bits()), target.is_signed())
                    .ok_or(InterpreterError::FloatingToInteger { span })?
            };
            Ok(CValue::Integer(CInteger::from_raw(raw, target)))
        }
        (CValue::Floating(_), ValueType::Pointer(_) | ValueType::Aggregate(_)) => {
            Err(InterpreterError::UnsupportedExpression {
                feature: "invalid floating conversion",
                span,
            })
        }
        (CValue::Pointer { value, .. }, ValueType::Integer(target)) => {
            if target == CIntegerType::BOOL {
                return Ok(CValue::Integer(CInteger::from_raw(
                    u128::from(value != Pointer::NULL),
                    target,
                )));
            }
            let (next, address) = memory
                .expose_pointer(PointerValue::object(value))
                .map_err(|error| InterpreterError::Memory { error, span })?;
            *memory = next;
            Ok(CValue::Integer(
                CInteger::from_raw(u128::from(address), CIntegerType::ULONG).cast(target),
            ))
        }
        (CValue::FunctionPointer { value, .. }, ValueType::Integer(target)) => {
            if target == CIntegerType::BOOL {
                return Ok(CValue::Integer(CInteger::from_raw(
                    u128::from(value != PointerValue::Null),
                    target,
                )));
            }
            let (next, address) = memory
                .expose_pointer(value)
                .map_err(|error| InterpreterError::Memory { error, span })?;
            *memory = next;
            Ok(CValue::Integer(
                CInteger::from_raw(u128::from(address), CIntegerType::ULONG).cast(target),
            ))
        }
        (CValue::Integer(value), ValueType::Integer(target)) => {
            Ok(CValue::Integer(value.cast(target)))
        }
        (value @ CValue::Pointer { .. }, ValueType::Pointer(target))
            if pointer_type_is_function(types, target) =>
        {
            let address = cast_machine_value(
                value,
                ValueType::Integer(CIntegerType::ULONG),
                span,
                types,
                memory,
            )?;
            cast_machine_value(address, ValueType::Pointer(target), span, types, memory)
        }
        (value @ CValue::FunctionPointer { .. }, ValueType::Pointer(target))
            if !pointer_type_is_function(types, target) =>
        {
            let address = cast_machine_value(
                value,
                ValueType::Integer(CIntegerType::ULONG),
                span,
                types,
                memory,
            )?;
            cast_machine_value(address, ValueType::Pointer(target), span, types, memory)
        }
        (CValue::Pointer { value, ty }, ValueType::Pointer(target)) if ty == target => {
            Ok(CValue::Pointer { value, ty: target })
        }
        (CValue::Pointer { value, .. }, ValueType::Pointer(target))
            if value == Pointer::NULL && pointer_type_is_function(types, target) =>
        {
            Ok(CValue::FunctionPointer {
                value: PointerValue::Null,
                ty: target,
            })
        }
        (CValue::Pointer { value, ty }, ValueType::Pointer(target))
            if pointer_runtime_conversion(types, ty, target) =>
        {
            Ok(CValue::Pointer { value, ty: target })
        }
        (CValue::FunctionPointer { value, .. }, ValueType::Pointer(target))
            if pointer_type_is_function(types, target) =>
        {
            // A cast retags the interface; start_indirect_call checks it
            // against the function identity after evaluating the arguments.
            Ok(CValue::FunctionPointer { value, ty: target })
        }
        (
            CValue::Aggregate {
                address,
                ty,
                qualifiers,
            },
            ValueType::Aggregate(target),
        ) if crate::vector::assignment_compatible(types, ty, target)
            || types
                .get(ty)
                .zip(types.get(target))
                .is_some_and(|(source, target)| {
                    matches!(
                        source.kind,
                        TypeKind::Struct(_)
                            | TypeKind::Union(_)
                            | TypeKind::BuiltinVaList
                            | TypeKind::Complex(evocc_types::ComplexBase::Integer(_))
                            | TypeKind::Vector { .. }
                    ) && source.kind == target.kind
                }) =>
        {
            Ok(CValue::Aggregate {
                address,
                ty: target,
                qualifiers,
            })
        }
        // C11 6.3.2.3p3: a null pointer constant converts to a null pointer of
        // the destination type. Whether the source really was a *constant* is
        // settled where the assignment was type checked; only the conversion
        // itself is executed here.
        (CValue::Integer(value), ValueType::Pointer(target)) if value.raw_bits() == 0 => {
            if pointer_type_is_function(types, target) {
                Ok(CValue::FunctionPointer {
                    value: PointerValue::Null,
                    ty: target,
                })
            } else {
                Ok(CValue::Pointer {
                    value: Pointer::NULL,
                    ty: target,
                })
            }
        }
        (CValue::Integer(value), ValueType::Pointer(target)) => {
            let address = value.cast(CIntegerType::ULONG).raw_bits() as u64;
            Ok(if pointer_type_is_function(types, target) {
                CValue::FunctionPointer {
                    value: PointerValue::Exposed(address),
                    ty: target,
                }
            } else {
                CValue::Pointer {
                    value: Pointer::from_address(address),
                    ty: target,
                }
            })
        }
        (CValue::Pointer { .. }, ValueType::Pointer(_))
        | (CValue::FunctionPointer { .. }, ValueType::Pointer(_))
        | (CValue::Aggregate { .. }, ValueType::Integer(_) | ValueType::Pointer(_))
        | (
            CValue::Integer(_) | CValue::Pointer { .. } | CValue::FunctionPointer { .. },
            ValueType::Aggregate(_),
        )
        | (CValue::Aggregate { .. }, ValueType::Aggregate(_)) => {
            Err(InterpreterError::UnsupportedExpression {
                feature: "pointer cast execution before Day 7 memory bindings",
                span,
            })
        }
        (_, ValueType::Void) => Err(InterpreterError::UnsupportedExpression {
            feature: "value cast to void in task-machine continuation",
            span,
        }),
    }
}

fn pointer_type_is_function(types: &TypeInterner, pointer: PointerTypeDesc) -> bool {
    matches!(
        types.get(pointer.pointee()).map(|ty| &ty.kind),
        Some(TypeKind::Function { .. })
    )
}

fn pointer_runtime_conversion(
    types: &TypeInterner,
    source: PointerTypeDesc,
    target: PointerTypeDesc,
) -> bool {
    let (Some(source_ty), Some(target_ty)) =
        (types.get(source.pointee()), types.get(target.pointee()))
    else {
        return false;
    };
    // The parser has already distinguished implicit qualification conversion
    // from an explicit cast. Runtime pointer values carry provenance, not C
    // qualifiers, so either source-level form preserves the same capability
    // when the unqualified pointee kind is compatible.
    source_ty.kind == target_ty.kind
        || (!matches!(source_ty.kind, TypeKind::Function { .. })
            && !matches!(target_ty.kind, TypeKind::Function { .. }))
}

fn merge_memory_qualifiers(left: Qualifiers, right: Qualifiers) -> Qualifiers {
    Qualifiers {
        is_const: left.is_const || right.is_const,
        is_volatile: left.is_volatile || right.is_volatile,
        is_restrict: left.is_restrict || right.is_restrict,
        is_atomic: left.is_atomic || right.is_atomic,
        aligned: match (left.aligned, right.aligned) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (left, right) => left.or(right),
        },
    }
}

fn assignment_binary(operator: AssignmentOp) -> Option<BinaryOp> {
    operator.binary_operation()
}

fn function_parameters_match(
    interner: &TypeInterner,
    canonical: &FunctionParameters,
    written: &ParameterSyntax,
) -> bool {
    match (canonical, written) {
        (FunctionParameters::Unspecified, ParameterSyntax::Unspecified) => true,
        (FunctionParameters::Prototype { types, variadic }, syntax)
            if !matches!(syntax, ParameterSyntax::Unspecified) =>
        {
            let parameters = syntax.parameters();
            *variadic == matches!(syntax, ParameterSyntax::Prototype { variadic: true, .. })
                && types.len() == parameters.len()
                && types.iter().zip(parameters).all(|(canonical, parameter)| {
                    syntax.parameter_type_matches(interner, *canonical, parameter.ty)
                })
        }
        _ => false,
    }
}

fn function_parameters_compatible(
    types: &TypeInterner,
    left: &FunctionParameters,
    right: &FunctionParameters,
) -> bool {
    match (left, right) {
        (FunctionParameters::Unspecified, FunctionParameters::Unspecified) => true,
        (
            FunctionParameters::Prototype {
                types: left_types,
                variadic: left_variadic,
            },
            FunctionParameters::Prototype {
                types: right_types,
                variadic: right_variadic,
            },
        ) => {
            left_variadic == right_variadic
                && left_types.len() == right_types.len()
                && left_types
                    .iter()
                    .zip(right_types)
                    .all(|(left, right)| types.compatible_ignoring_top_qualifiers(*left, *right))
        }
        (
            FunctionParameters::Unspecified,
            FunctionParameters::Prototype {
                types: prototype,
                variadic,
            },
        )
        | (
            FunctionParameters::Prototype {
                types: prototype,
                variadic,
            },
            FunctionParameters::Unspecified,
        ) => {
            !variadic
                && prototype
                    .iter()
                    .all(|ty| default_argument_promotion_preserves_type(types, *ty))
        }
    }
}

fn default_argument_promotion_preserves_type(types: &TypeInterner, ty: TypeId) -> bool {
    let Some(canonical) = types.get(ty) else {
        return false;
    };
    match canonical.kind {
        TypeKind::Int(_) | TypeKind::Enum(_) => canonical_integer_type(types, ty)
            .is_some_and(|integer| CInteger::new(0, integer).promoted().ty() == integer),
        TypeKind::Float(kind) => matches!(
            kind,
            evocc_types::FloatKind::F64 | evocc_types::FloatKind::F128
        ),
        TypeKind::Void
        | TypeKind::TargetBuiltin(_)
        | TypeKind::Array { .. }
        | TypeKind::Function { .. } => false,
        TypeKind::BuiltinVaList
        | TypeKind::GnuInt128(_)
        | TypeKind::Vector { .. }
        | TypeKind::Complex(_)
        | TypeKind::Pointer(_)
        | TypeKind::Struct(_)
        | TypeKind::Union(_) => true,
    }
}

fn function_storage_compatible(entity: StorageClass, written: StorageClass) -> bool {
    match entity {
        StorageClass::Static => matches!(
            written,
            StorageClass::None | StorageClass::Extern | StorageClass::Static
        ),
        StorageClass::None | StorageClass::Extern => {
            matches!(written, StorageClass::None | StorageClass::Extern)
        }
        StorageClass::Typedef
        | StorageClass::Auto
        | StorageClass::Register
        | StorageClass::ThreadLocal
        | StorageClass::StaticThreadLocal
        | StorageClass::ExternThreadLocal => false,
    }
}

fn canonical_integer_type(types: &TypeInterner, ty: TypeId) -> Option<CIntegerType> {
    let canonical = types.get(ty)?;
    match canonical.kind {
        TypeKind::TargetBuiltin(kind) => crate::integer::integer_type_for_target(kind),
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

fn integer_storage_size(ty: CIntegerType) -> u64 {
    u64::from(ty.bits()).div_ceil(8).max(1)
}

fn expression_operand_span(expression: &CHirExpr<'_>) -> SourceSpan {
    match expression {
        CHirExpr::Unary(_, operand, _) => operand.span(),
        _ => expression.span(),
    }
}

fn unary_operator_span(outer: SourceSpan, operand: SourceSpan) -> SourceSpan {
    if outer.file == operand.file
        && outer.line_start == operand.line_start
        && outer.byte_start < operand.byte_start
        && outer.byte_end >= operand.byte_end
    {
        SourceSpan::new(
            operand.file,
            operand.byte_start - 1,
            operand.byte_end,
            operand.line_start,
            operand.column_start.saturating_sub(1),
        )
    } else {
        outer
    }
}

fn collect_switch_labels<'a>(statement: &'a CHirStmt<'a>, labels: &mut Vec<&'a CHirStmt<'a>>) {
    match statement {
        CHirStmt::Case(_, child, _)
        | CHirStmt::CaseRange(_, _, child, _)
        | CHirStmt::Default(child, _) => {
            labels.push(statement);
            collect_switch_labels(child, labels);
        }
        CHirStmt::If(_, then_branch, else_branch, _) => {
            collect_switch_labels(then_branch, labels);
            if let Some(branch) = else_branch {
                collect_switch_labels(branch, labels);
            }
        }
        CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Label(_, body, _) => collect_switch_labels(body, labels),
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    collect_switch_labels(statement, labels);
                }
            }
        }
        // A nested switch owns its own case labels.
        _ => {}
    }
}

fn statement_contains_destination(
    statement: &CHirStmt<'_>,
    destination: StatementDestination<'_>,
) -> bool {
    if destination.matches(statement) {
        return true;
    }
    match statement {
        CHirStmt::Label(_, child, _) => statement_contains_destination(child, destination),
        CHirStmt::If(_, then_statement, else_statement, _) => {
            statement_contains_destination(then_statement, destination)
                || else_statement
                    .is_some_and(|statement| statement_contains_destination(statement, destination))
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::Default(body, _) => statement_contains_destination(body, destination),
        CHirStmt::DoWhile(body, _, _) => statement_contains_destination(body, destination),
        CHirStmt::Compound(items, _, _) => items.iter().any(|item| {
            matches!(
                item,
                CHirBlockItem::Statement(statement)
                    if statement_contains_destination(statement, destination)
            )
        }),
        CHirStmt::Expr(_, _)
        | CHirStmt::Asm(_)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::ComputedGoto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => false,
    }
}

fn validate_function_labels(
    statement: &CHirStmt<'_>,
    labels: &mut BTreeMap<String, SourceSpan>,
) -> Result<(), InterpreterError> {
    match statement {
        CHirStmt::If(_, then_statement, else_statement, _) => {
            validate_function_labels(then_statement, labels)?;
            if let Some(else_statement) = else_statement {
                validate_function_labels(else_statement, labels)?;
            }
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::Default(body, _) => validate_function_labels(body, labels)?,
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    validate_function_labels(statement, labels)?;
                }
            }
        }
        CHirStmt::Label(name, body, span) => {
            if labels.insert(name.spelling.clone(), *span).is_some() {
                return Err(InterpreterError::DuplicateLabel {
                    spelling: name.spelling.clone(),
                    span: *span,
                });
            }
            validate_function_labels(body, labels)?;
        }
        CHirStmt::Expr(_, _)
        | CHirStmt::Asm(_)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::ComputedGoto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
    Ok(())
}

fn validate_function_control_flow(statement: &CHirStmt<'_>) -> Result<(), InterpreterError> {
    let mut labels = BTreeMap::new();
    validate_function_labels(statement, &mut labels)?;
    validate_function_gotos(statement, &labels)
}

fn validate_function_gotos(
    statement: &CHirStmt<'_>,
    labels: &BTreeMap<String, SourceSpan>,
) -> Result<(), InterpreterError> {
    match statement {
        CHirStmt::If(_, then_statement, else_statement, _) => {
            validate_function_gotos(then_statement, labels)?;
            if let Some(else_statement) = else_statement {
                validate_function_gotos(else_statement, labels)?;
            }
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::Default(body, _)
        | CHirStmt::Label(_, body, _) => validate_function_gotos(body, labels)?,
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    validate_function_gotos(statement, labels)?;
                }
            }
        }
        CHirStmt::Goto(name, span) if !labels.contains_key(&name.spelling) => {
            return Err(InterpreterError::UnknownLabel {
                spelling: name.spelling.clone(),
                span: *span,
            });
        }
        CHirStmt::Expr(_, _)
        | CHirStmt::Asm(_)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::ComputedGoto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        IntBase, IntLit, IntSuffix, SpannedName, TypeSpecifier, WrittenDeclarator,
        WrittenDeclaratorLayer,
    };
    use evocc_common::{Arena, FileId};
    use evocc_memory::{AccessLayout, MemoryFault, ObjectId};
    use evocc_types::{CanonicalType, IntRank, Qualifiers};

    #[test]
    fn repeated_evaluation_shares_the_immutable_units_linkage() {
        let unit = CHirUnit::new(vec![], TypeInterner::new(), crate::SymbolTable::new());
        let target = TargetDataLayout::aarch64_unknown_linux_gnu();
        let first = Interpreter::new(&unit, &target, InterpreterLimits::default(), false).unwrap();
        let second = Interpreter::new(&unit, &target, InterpreterLimits::default(), false).unwrap();
        assert!(
            std::ptr::eq(first.linkage, second.linkage),
            "one immutable unit must not rebuild its declaration inventory for every initializer"
        );
    }

    #[test]
    fn substitution_invalidates_only_the_rewritten_units_linkage() {
        let arena = Arena::default();
        let (mut unit, function, _) = scalar_program(&arena, false);
        let scope = unit
            .symbols
            .create_scope(unit.symbols.file_scope(), crate::ScopeKind::Function, None)
            .unwrap();
        unit.symbols.bind_function_scope(scope, function).unwrap();
        unit.symbol_linkage().unwrap();
        let rewritten = crate::inline::substitute(
            &unit,
            &arena,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
        )
        .unwrap();
        assert!(unit.resolved_linkage.get().is_some());
        assert!(rewritten.resolved_linkage.get().is_none());
        let cached = rewritten.symbol_linkage().unwrap();
        let derived = crate::linkage::SymbolLinkage::derive(&rewritten).unwrap();
        assert_eq!(cached.aliases, derived.aliases);
        assert_eq!(cached.weak, derived.weak);
    }
    fn span(start: u32, end: u32) -> SourceSpan {
        SourceSpan::new(FileId::new(701), start, end, 1, start + 1)
    }

    fn alloc<'arena, T: 'arena>(arena: &'arena Arena, value: T) -> &'arena T {
        // Safety: these C-HIR nodes contain no Arena handle and their
        // destructors only release owned source-fact buffers.
        unsafe { arena.alloc_shared(value) }
    }

    fn scalar_program(arena: &Arena, use_task_machine: bool) -> (CHirUnit<'_>, SymbolId, TypeId) {
        let mut types = TypeInterner::new();
        let int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let function_ty = types.intern(CanonicalType {
            kind: TypeKind::Function {
                return_type: int,
                parameters: FunctionParameters::Prototype {
                    types: Vec::new(),
                    variadic: false,
                },
            },
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = crate::SymbolTable::new();
        let function = symbols
            .declare(
                symbols.file_scope(),
                "main",
                crate::SymbolData::Function {
                    storage: StorageClass::None,
                },
                function_ty,
                span(0, 4),
            )
            .expect("declare test main");

        let value = alloc(
            arena,
            CHirExpr::Lit(
                IntLit {
                    value: 7,
                    suffix: IntSuffix::None,
                    base: IntBase::Dec,
                },
                span(20, 21),
            ),
        );
        let returned = alloc(arena, CHirStmt::Return(Some(value), span(13, 22)));
        let items = if use_task_machine {
            let name = SpannedName {
                spelling: "done".to_owned(),
                span: span(7, 11),
            };
            vec![
                CHirBlockItem::Statement(alloc(arena, CHirStmt::Goto(name.clone(), span(5, 12)))),
                CHirBlockItem::Statement(alloc(
                    arena,
                    CHirStmt::Label(name, returned, span(13, 22)),
                )),
            ]
        } else {
            vec![CHirBlockItem::Statement(returned)]
        };
        let body = alloc(arena, CHirStmt::Compound(items, span(4, 23), None));
        let declaration = alloc(
            arena,
            CHirDecl::Function(FunctionDecl {
                attributes: Vec::new(),
                symbol: function,
                name: SpannedName {
                    spelling: "main".to_owned(),
                    span: span(0, 4),
                },
                ty: function_ty,
                return_specifier: TypeSpecifier::Builtin {
                    ty: int,
                    span: span(0, 3),
                },
                written: WrittenDeclarator {
                    layers: vec![WrittenDeclaratorLayer::Function {
                        parameters: None,
                        span: span(3, 5),
                    }],
                    span: span(0, 5),
                },
                parameters: ParameterSyntax::Prototype {
                    parameters: Vec::new(),
                    variadic: false,
                },
                storage: StorageClass::None,
                asm_label: None,
                asm_label_span: None,
                inline_spans: Vec::new(),
                gnu_extension_spans: Vec::new(),
                body: Some(body),
                span: span(0, 23),
            }),
        );
        (
            CHirUnit::new(vec![declaration], types, symbols),
            function,
            int,
        )
    }

    fn assert_empty_execution_state(outcome: &InterpretOutcome) {
        assert!(outcome.world.volatile_trace().is_empty());
        assert_eq!(
            outcome.memory.config().limits(),
            MemoryLimits {
                max_objects: MAX_MEMORY_OBJECTS,
                max_object_bytes: MAX_MEMORY_OBJECT_BYTES,
                max_total_bytes: MAX_MEMORY_TOTAL_BYTES,
            }
        );
        assert_eq!(
            outcome.world.limits(),
            WorldLimits {
                max_volatile_events: MAX_VOLATILE_EVENTS,
            }
        );
        assert!(matches!(
            outcome.memory.object_info(ObjectId::from_raw(1)),
            Err(MemoryFault::UnknownObject { .. })
        ));
    }

    #[test]
    fn call_free_scalar_execution_keeps_value_and_returns_empty_states() {
        let arena = Arena::default();
        let (unit, function, int) = scalar_program(&arena, false);

        let outcome =
            interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();

        assert_eq!(
            outcome.return_value,
            Some(ScalarValue {
                ty: int,
                raw_bits: 7,
            })
        );
        assert!(outcome.steps > 0);
        assert_empty_execution_state(&outcome);
    }

    #[test]
    fn task_machine_uses_target_aware_states_and_is_deterministic() {
        let arena = Arena::default();
        let (unit, function, int) = scalar_program(&arena, true);
        let target = TargetDataLayout::aarch64_unknown_linux_gnu();

        let first = interpret_scalar_function_with_target(
            &unit,
            function,
            &target,
            InterpreterLimits::default(),
        )
        .unwrap();
        let second = interpret_scalar_function_with_target(
            &unit,
            function,
            &target,
            InterpreterLimits::default(),
        )
        .unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first.return_value,
            Some(ScalarValue {
                ty: int,
                raw_bits: 7,
            })
        );
        assert_eq!(first.memory.config().byte_order(), target.byte_order());
        assert_eq!(
            first.memory.config().pointer_layout().size_bytes,
            target.pointer_layout().size_bytes
        );
        assert_empty_execution_state(&first);
    }

    #[test]
    fn invalid_target_memory_setup_fails_closed_without_a_source_span() {
        let error = InterpreterError::MemorySetup {
            error: MemoryFault::InvalidPointerLayout {
                layout: AccessLayout::new(0, 1),
            },
        };

        assert_eq!(error.code(), "E3007");
        assert_eq!(
            error.message(),
            "interpreter-memory-setup-failed: memory fault: InvalidPointerLayout"
        );
        assert_eq!(error.span(), None);
    }

    /// Build `int counter; int bump(void) { return counter; } int main(void) { return bump(); }`.
    ///
    /// Only `bump` touches the global, so a non-transitive scan of `main` alone
    /// would wrongly conclude that this program needs no memory.
    fn transitive_global_program(arena: &Arena) -> (CHirUnit<'_>, SymbolId, SymbolId, SymbolId) {
        let mut types = TypeInterner::new();
        let int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let function_ty = types.intern(CanonicalType {
            kind: TypeKind::Function {
                return_type: int,
                parameters: FunctionParameters::Prototype {
                    types: Vec::new(),
                    variadic: false,
                },
            },
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = crate::SymbolTable::new();
        let counter = symbols
            .declare(
                symbols.file_scope(),
                "counter",
                crate::SymbolData::Object {
                    storage: StorageClass::None,
                },
                int,
                span(0, 7),
            )
            .expect("declare counter");
        let bump = symbols
            .declare(
                symbols.file_scope(),
                "bump",
                crate::SymbolData::Function {
                    storage: StorageClass::None,
                },
                function_ty,
                span(10, 14),
            )
            .expect("declare bump");
        let main = symbols
            .declare(
                symbols.file_scope(),
                "main",
                crate::SymbolData::Function {
                    storage: StorageClass::None,
                },
                function_ty,
                span(30, 34),
            )
            .expect("declare main");
        symbols
            .create_scope(
                symbols.file_scope(),
                ScopeKind::Function,
                Some(span(10, 29)),
            )
            .expect("create bump function scope");
        symbols
            .create_scope(
                symbols.file_scope(),
                ScopeKind::Function,
                Some(span(30, 48)),
            )
            .expect("create main function scope");

        let object = alloc(
            arena,
            CHirDecl::Object(ObjectDecl {
                attributes: Vec::new(),
                cleanup: None,
                symbol: counter,
                name: SpannedName {
                    spelling: "counter".to_owned(),
                    span: span(0, 7),
                },
                ty: int,
                specifier: TypeSpecifier::Builtin {
                    ty: int,
                    span: span(0, 3),
                },
                written: WrittenDeclarator {
                    layers: Vec::new(),
                    span: span(0, 7),
                },
                storage: StorageClass::None,
                initializer: None,
                asm_label: None,
                asm_label_span: None,
                gnu_extension_spans: Vec::new(),
                span: span(0, 8),
            }),
        );

        let read_counter = alloc(
            arena,
            CHirExpr::Ident {
                symbol: counter,
                spelling: "counter".to_owned(),
                facts: ExprFacts::lvalue(int, true, true).expect("modifiable global lvalue"),
                span: span(20, 27),
            },
        );
        let bump_body = alloc(
            arena,
            CHirStmt::Compound(
                vec![CHirBlockItem::Statement(alloc(
                    arena,
                    CHirStmt::Return(Some(read_counter), span(18, 28)),
                ))],
                span(16, 29),
                None,
            ),
        );
        let bump_decl = alloc(
            arena,
            CHirDecl::Function(test_function_decl(
                bump,
                "bump",
                int,
                function_ty,
                Some(bump_body),
                span(10, 29),
            )),
        );

        let callee = alloc(
            arena,
            CHirExpr::Ident {
                symbol: bump,
                spelling: "bump".to_owned(),
                facts: ExprFacts::function_designator(function_ty),
                span: span(40, 44),
            },
        );
        let call = alloc(arena, CHirExpr::Call(callee, Vec::new(), span(40, 46)));
        let main_body = alloc(
            arena,
            CHirStmt::Compound(
                vec![CHirBlockItem::Statement(alloc(
                    arena,
                    CHirStmt::Return(Some(call), span(38, 47)),
                ))],
                span(36, 48),
                None,
            ),
        );
        let main_decl = alloc(
            arena,
            CHirDecl::Function(test_function_decl(
                main,
                "main",
                int,
                function_ty,
                Some(main_body),
                span(30, 48),
            )),
        );

        (
            CHirUnit::new(vec![object, bump_decl, main_decl], types, symbols),
            main,
            bump,
            counter,
        )
    }

    fn test_function_decl<'arena>(
        symbol: SymbolId,
        spelling: &str,
        result: TypeId,
        function_ty: TypeId,
        body: Option<&'arena CHirStmt<'arena>>,
        at: SourceSpan,
    ) -> FunctionDecl<'arena> {
        FunctionDecl {
            attributes: Vec::new(),
            symbol,
            name: SpannedName {
                spelling: spelling.to_owned(),
                span: at,
            },
            ty: function_ty,
            return_specifier: TypeSpecifier::Builtin {
                ty: result,
                span: at,
            },
            written: WrittenDeclarator {
                layers: vec![WrittenDeclaratorLayer::Function {
                    parameters: None,
                    span: at,
                }],
                span: at,
            },
            parameters: ParameterSyntax::Prototype {
                parameters: Vec::new(),
                variadic: false,
            },
            storage: StorageClass::None,
            asm_label: None,
            asm_label_span: None,
            inline_spans: Vec::new(),
            gnu_extension_spans: Vec::new(),
            body,
            span: at,
        }
    }

    #[test]
    fn reachable_memory_requirements_follow_transitive_calls() {
        let arena = Arena::new(64 * 1024);
        let (unit, main, _bump, counter) = transitive_global_program(&arena);

        let requirements =
            reachable_memory_requirements(&unit, main, &crate::linkage::SymbolLinkage::default());

        assert!(
            requirements.requires_memory,
            "a global read behind one call level still requires memory"
        );
        assert!(
            requirements.static_objects.contains(&counter),
            "the transitively referenced global must be scheduled for allocation"
        );
    }

    #[test]
    fn unreachable_globals_are_not_scheduled_for_allocation() {
        let arena = Arena::new(64 * 1024);
        let (unit, _main, bump, counter) = transitive_global_program(&arena);

        // Entering at `bump` still reaches `counter`; entering nowhere else must not
        // invent objects, so the manifest stays exactly one object wide.
        let requirements =
            reachable_memory_requirements(&unit, bump, &crate::linkage::SymbolLinkage::default());

        assert!(requirements.requires_memory);
        assert_eq!(
            requirements
                .static_objects
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![counter]
        );
    }

    #[test]
    fn deeply_nested_static_initializer_executes_without_recursive_stack_growth() {
        let arena = Arena::new(64 * 1024);
        let (unit, main) = deep_static_initializer_program(&arena, 512);

        let outcome = interpret_scalar_function_with_target(
            &unit,
            main,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            InterpreterLimits::default(),
        )
        .expect("a defined constant initializer is independent of host stack depth");

        assert_eq!(
            outcome
                .return_value
                .unwrap()
                .mathematical_value(unit.types())
                .unwrap(),
            1
        );
    }

    #[test]
    fn static_initializer_iteration_retains_the_execution_budget() {
        let arena = Arena::new(64 * 1024);
        let (unit, main) = deep_static_initializer_program(&arena, 512);
        let error = interpret_scalar_function_with_target(
            &unit,
            main,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            InterpreterLimits::new(32),
        )
        .expect_err("constant initialization consumes the same explicit fuel budget");
        assert!(
            matches!(error, InterpreterError::FuelExhausted { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn odd_negations_in_static_initializers_preserve_the_value() {
        let arena = Arena::new(64 * 1024);
        let negations = 63;
        let (unit, main) = deep_static_initializer_program(&arena, negations);

        let outcome = interpret_scalar_function_with_target(
            &unit,
            main,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            InterpreterLimits::new(1_000_000),
        )
        .expect("a defined static initializer must be evaluated");

        let value = outcome
            .return_value
            .expect("main returns the initialized global")
            .mathematical_value(unit.types())
            .expect("the global is a plain int");
        // An even number of negations leaves the literal 1 unchanged.
        assert_eq!(value, if negations.is_multiple_of(2) { 1 } else { -1 });
    }

    #[test]
    fn static_integer_machine_does_not_execute_calls_or_dead_operands() {
        let arena = Arena::default();
        let (unit, function, int) = scalar_program(&arena, false);
        let target = TargetDataLayout::aarch64_unknown_linux_gnu();
        let callee = alloc(
            &arena,
            CHirExpr::Ident {
                symbol: function,
                spelling: "main".to_owned(),
                facts: ExprFacts::function_designator(
                    unit.symbols().symbol(function).unwrap().ty(),
                ),
                span: span(0, 4),
            },
        );
        let call = alloc(&arena, CHirExpr::Call(callee, Vec::new(), span(0, 6)));
        let zero = alloc(
            &arena,
            CHirExpr::Lit(
                IntLit {
                    value: 0,
                    suffix: IntSuffix::None,
                    base: IntBase::Dec,
                },
                span(7, 8),
            ),
        );
        let comma = alloc(&arena, CHirExpr::Comma(call, zero, span(0, 8)));
        for expression in [call, comma] {
            assert!(matches!(
                interpret_static_integer_expression(&unit, expression, &target),
                Err(InterpreterError::UnsupportedExpression {
                    feature: "nonconstant expression in static integer initializer",
                    ..
                })
            ));
        }
        let selected = alloc(&arena, CHirExpr::Ternary(zero, call, zero, span(0, 8)));
        assert_eq!(
            interpret_static_integer_expression(&unit, selected, &target).unwrap(),
            CInteger::int(0)
        );
        let size = alloc(&arena, CHirExpr::SizeOfExpr(call, int, span(0, 8)));
        assert_eq!(
            interpret_static_integer_expression(&unit, size, &target).unwrap(),
            CInteger::new(4, CIntegerType::ULONG)
        );
    }

    /// Build `int deep = -(-(-...1...)); int main(void) { return deep; }` with
    /// `depth` nested negations.
    fn deep_static_initializer_program(arena: &Arena, depth: usize) -> (CHirUnit<'_>, SymbolId) {
        let mut types = TypeInterner::new();
        let int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let function_ty = types.intern(CanonicalType {
            kind: TypeKind::Function {
                return_type: int,
                parameters: FunctionParameters::Prototype {
                    types: Vec::new(),
                    variadic: false,
                },
            },
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = crate::SymbolTable::new();
        let deep = symbols
            .declare(
                symbols.file_scope(),
                "deep",
                crate::SymbolData::Object {
                    storage: StorageClass::None,
                },
                int,
                span(0, 4),
            )
            .expect("declare deep");
        let main = symbols
            .declare(
                symbols.file_scope(),
                "main",
                crate::SymbolData::Function {
                    storage: StorageClass::None,
                },
                function_ty,
                span(30, 34),
            )
            .expect("declare main");
        symbols
            .create_scope(
                symbols.file_scope(),
                ScopeKind::Function,
                Some(span(30, 46)),
            )
            .expect("create main function scope");

        let mut value: &CHirExpr<'_> = alloc(
            arena,
            CHirExpr::Lit(
                IntLit {
                    value: 1,
                    suffix: IntSuffix::None,
                    base: IntBase::Dec,
                },
                span(10, 11),
            ),
        );
        for _ in 0..depth {
            value = alloc(arena, CHirExpr::Unary(UnaryOp::Neg, value, span(10, 11)));
        }

        let plan = crate::InitPlan::new(
            int,
            vec![crate::InitAction::store_scalar(
                crate::SubobjectPath::root(),
                int,
                value,
                span(10, 11),
            )],
        );
        let object = alloc(
            arena,
            CHirDecl::Object(ObjectDecl {
                attributes: Vec::new(),
                cleanup: None,
                symbol: deep,
                name: SpannedName {
                    spelling: "deep".to_owned(),
                    span: span(0, 4),
                },
                ty: int,
                specifier: TypeSpecifier::Builtin {
                    ty: int,
                    span: span(0, 3),
                },
                written: WrittenDeclarator {
                    layers: Vec::new(),
                    span: span(0, 4),
                },
                storage: StorageClass::None,
                initializer: Some(crate::ObjectInitializer::new(
                    crate::WrittenInitializer::Expression(value),
                    plan,
                )),
                asm_label: None,
                asm_label_span: None,
                gnu_extension_spans: Vec::new(),
                span: span(0, 12),
            }),
        );

        let read_deep = alloc(
            arena,
            CHirExpr::Ident {
                symbol: deep,
                spelling: "deep".to_owned(),
                facts: ExprFacts::lvalue(int, true, true).expect("modifiable global lvalue"),
                span: span(40, 44),
            },
        );
        let main_body = alloc(
            arena,
            CHirStmt::Compound(
                vec![CHirBlockItem::Statement(alloc(
                    arena,
                    CHirStmt::Return(Some(read_deep), span(38, 45)),
                ))],
                span(36, 46),
                None,
            ),
        );
        let main_decl = alloc(
            arena,
            CHirDecl::Function(test_function_decl(
                main,
                "main",
                int,
                function_ty,
                Some(main_body),
                span(30, 46),
            )),
        );

        (CHirUnit::new(vec![object, main_decl], types, symbols), main)
    }
}
