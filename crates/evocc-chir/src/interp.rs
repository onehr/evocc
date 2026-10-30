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
    ArrayBound, BitFieldLayout, FunctionParameters, Qualifiers, TagDefinition, TagState,
    TargetDataLayout, TypeId, TypeInterner, TypeKind,
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

mod atomic;
mod atomic_values;
mod bit_fields;
mod calls;
mod complex;
mod construction;
mod control_flow;
mod declarations;
mod discarded;
mod expressions;
mod floating;
mod functions;
mod identifier_values;
mod initializers;
mod lifecycle;
mod machine;
mod memory_access;
mod neon;
mod objects;
mod places;
mod pointers;
mod requirements;
mod runtime_values;
mod scalar_evaluation;
mod scalar_updates;
mod statements;
mod static_storage;
mod symbols;
#[cfg(test)]
mod tests;
mod value_types;
mod varargs;
mod vector;
mod volatile_copies;

use control_flow::*;
pub use lifecycle::{interpret_program, interpret_program_with_arguments};
use requirements::*;
use runtime_values::*;

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
    /// A call interface with a half type that has no admitted convention
    /// (EvoIR 3.13.6): the same refusal as the lowering's.
    FloatingCallInterface {
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
    /// A call without a prototype of an old-style definition whose
    /// argument count or promoted argument types do not agree with the
    /// definition (C11 6.5.2.2p6, EvoIR 10.1.7).
    OldStyleCallMismatch {
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
    /// The GNU argument pack of a variadic body that was not substituted.
    VarArgPack {
        span: SourceSpan,
    },
    WideInteger {
        span: SourceSpan,
    },
    FrameAddressUnavailable {
        span: SourceSpan,
    },
    /// A complex multiplication or division whose target helper has no
    /// execution model: Darwin's compiler-rt sequence.
    ComplexHelper {
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
            Self::OldStyleCallMismatch { .. } => "E3068",
            Self::CallThroughNullFunctionPointer { .. } => "E3015",
            Self::CallThroughNonFunctionPointer { .. } => "E3016",
            Self::Asm { .. } => "E3021",
            Self::BitBuiltinZeroOperand { .. } => "E3022",
            Self::UnreachableReached { .. } => "E3024",
            Self::BuiltinTrapReached { .. } => "E3047",
            Self::ComputedGoto { .. } => "E3029",
            Self::InvalidIndirectBranchTarget { .. } => "E3048",
            Self::VarArgs { .. } => "E3030",
            Self::FloatingCallInterface { .. } => "E3049",
            Self::VarArgPack { .. } => "E2950",
            Self::WideInteger { .. } => "E3033",
            Self::FrameAddressUnavailable { .. } => "E3031",
            Self::ComplexHelper { .. } => "E3067",
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
            Self::OldStyleCallMismatch { .. } => "old-style-call-mismatch".to_owned(),
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
            Self::FloatingCallInterface { .. } => "not-lowered: floating-call-interface".to_owned(),
            Self::VarArgPack { .. } => "invalid-va-arg-pack".to_owned(),
            Self::WideInteger { .. } => "not-executed: wide-integer".to_owned(),
            Self::FrameAddressUnavailable { .. } => "not-executed: frame-address".to_owned(),
            Self::ComplexHelper { .. } => "not-executed: complex-arithmetic".to_owned(),
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
            | Self::FloatingCallInterface { span }
            | Self::VarArgPack { span }
            | Self::WideInteger { span }
            | Self::FrameAddressUnavailable { span }
            | Self::ComplexHelper { span }
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
            | Self::OldStyleCallMismatch { span }
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
    let requirements = reachable_memory_requirements(unit, function, linkage, target);
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
    /// Pending range values belong to one destination allocation and one
    /// initializer execution. Recursive calls cannot reuse a caller's cache.
    initializer_values: HashMap<Pointer, HashMap<usize, CValue>>,
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

/// Memory obligations of everything reachable from one entry function.
///
/// Reachability is transitive: a global touched only by a callee still forces
/// memory mode and still has to be allocated before execution starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MemoryRequirements {
    /// Some reachable body or call signature needs the typed object-memory machine.
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

/// Whether execution needs object state, including places rooted at integer casts.
pub fn statement_requires_memory(
    unit: &CHirUnit<'_>,
    statement: &CHirStmt<'_>,
    target: &TargetDataLayout,
) -> bool {
    match statement {
        CHirStmt::Asm(_) => true,
        CHirStmt::Expr(expression, _) => expression_requires_memory(unit, expression, target),
        CHirStmt::ComputedGoto(_, _) => true,
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            expression_requires_memory(unit, condition, target)
                || statement_requires_memory(unit, then_statement, target)
                || else_statement
                    .is_some_and(|statement| statement_requires_memory(unit, statement, target))
        }
        CHirStmt::Switch(expression, body, _) | CHirStmt::While(expression, body, _) => {
            expression_requires_memory(unit, expression, target)
                || statement_requires_memory(unit, body, target)
        }
        CHirStmt::DoWhile(body, condition, _) => {
            statement_requires_memory(unit, body, target)
                || expression_requires_memory(unit, condition, target)
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            let initializer_requires_memory = match initializer {
                CHirForInit::Empty => false,
                CHirForInit::Expression(expression) => {
                    expression_requires_memory(unit, expression, target)
                }
                CHirForInit::Declaration(declarations) => declarations
                    .iter()
                    .any(|declaration| declaration_requires_memory(unit, declaration, target)),
            };
            initializer_requires_memory
                || condition
                    .is_some_and(|expression| expression_requires_memory(unit, expression, target))
                || step
                    .is_some_and(|expression| expression_requires_memory(unit, expression, target))
                || statement_requires_memory(unit, body, target)
        }
        CHirStmt::Return(expression, _) => expression
            .is_some_and(|expression| expression_requires_memory(unit, expression, target)),
        CHirStmt::Compound(items, _, _) => items.iter().any(|item| match item {
            CHirBlockItem::Declaration(declaration) => {
                declaration_requires_memory(unit, declaration, target)
            }
            CHirBlockItem::Statement(statement) => {
                statement_requires_memory(unit, statement, target)
            }
        }),
        CHirStmt::Label(_, child, _)
        | CHirStmt::Case(_, child, _)
        | CHirStmt::CaseRange(_, _, child, _)
        | CHirStmt::Default(child, _) => statement_requires_memory(unit, child, target),
        CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => false,
    }
}
