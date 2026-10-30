//! Target-aware C-HIR to block-argument SSA EvoIR lowering.
//!
//! Day 6 keeps scalar locals in SSA values.  Memory and world tokens are
//! nevertheless present on every call edge so Day 7 can add real storage
//! semantics without changing the call instruction ABI.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

mod allocation;
mod assembly;
mod assignments;
mod atomic;
mod binary;
mod bit_fields;
mod blocks;
mod calls;
mod cfg;
mod checked_arithmetic;
mod complex;
mod conditional_values;
mod conditions;
mod constant_values;
mod constants;
mod effects;
mod expressions;
mod floating;
mod function_body;
mod globals;
mod initialization;
mod lifetimes;
mod liveness;
mod loops;
mod memory_access;
mod memory_updates;
mod neon;
mod objects;
mod predicates;
mod private_memory;
mod references;
mod reuse;
mod selection;
mod signatures;
mod statements;
mod storage;
mod switches;
mod symbols;
#[cfg(test)]
mod tests;
mod tls;
mod unit;
mod validation;
mod value_ids;
mod value_types;
mod vector;

use constant_values::*;
use globals::*;
use liveness::*;
use references::*;
use signatures::*;
use storage::*;
use switches::*;
use symbols::*;
use unit::*;
use validation::*;
use value_ids::*;

use evocc_chir::integer::{
    eval_binary_integer, integer_literal_value, integer_type_for_rank, usual_arithmetic_type,
    CInteger, CIntegerType,
};
use evocc_chir::old_style_calls::Verdict as OldStyleVerdict;
use evocc_chir::{
    statement_expression_value, AllocationSiteManifest, AsmStmt, AssignmentOp, BinaryOp,
    CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, CHirUnit, ExprCategory, ExprFacts,
    FunctionDecl, InitAction, MemoryPlace, ObjectDecl, ObjectInitializer, ParameterSyntax,
    PlaceProjection, PlaceRoot, ScalarPlace, ScopeKind, StorageClass, SubobjectPath, SubobjectStep,
    SymbolId, SymbolKind, UnaryOp, UpdateOp, WrittenDeclarator, WrittenDeclaratorLayer,
};
use evocc_common::SourceSpan;
use evocc_evoir::{
    inst_results, validate_module, AccessLayout, AssemblyEdge, AssemblyInput, AssemblyLabel,
    AssemblyOutput, Block, BlockArg, BlockId, CmpOp, CopyVolatility, ExternalObject, Function,
    FunctionDeclaration, FunctionParam, GlobalKind, GlobalObject, GlobalVisibility,
    IndexSignedness, InlineAssembly, Inst, Module, OffsetDirection, TrapReason, UbReason, Value,
};
use evocc_types::{
    canonical_marks, enum_integer_rank, AddressSpace, ArrayBound, BitFieldLayout, EvoIRType,
    FunctionParameters, LayoutError, ParamMark, RecordShape, Region, TagDefinition, TagState,
    TargetDataLayout, TypeId, TypeInterner, TypeKind, TypeLayout,
};

const BREAK_TARGET_BASE: BlockId = u32::MAX - 1_000_000;

/// GNU source numbers a read-write output's implicit input as well as the
/// explicit operands before its labels. Resolve those source numbers once;
/// EvoIR keeps only explicit operand numbers and ordinary symbolic labels.
fn source_assembly_template(source: &AsmStmt<'_>) -> Result<String, LowerError> {
    if source.goto_span.is_none() {
        return Ok(source.template.clone());
    }
    let first_label = source.outputs.len()
        + source.inputs.len()
        + source
            .outputs
            .iter()
            .filter(|output| output.constraint.starts_with('+'))
            .count();
    let mut template = String::with_capacity(source.template.len());
    let mut rest = source.template.as_str();
    while let Some(at) = rest.find('%') {
        template.push_str(&rest[..at]);
        rest = &rest[at..];
        if rest.starts_with("%%") {
            template.push_str("%%");
            rest = &rest[2..];
        } else if let Some(reference) = rest
            .strip_prefix("%l")
            .filter(|reference| reference.as_bytes().first().is_some_and(u8::is_ascii_digit))
        {
            let end = reference.bytes().take_while(u8::is_ascii_digit).count();
            let label = reference[..end]
                .parse::<usize>()
                .ok()
                .and_then(|index| index.checked_sub(first_label))
                .and_then(|index| source.labels.get(index))
                .ok_or_else(|| LowerError::AsmOperand {
                    span: source.span,
                    reason: format!("unknown assembly label index {}", &reference[..end]),
                })?;
            template.push_str("%l[");
            template.push_str(&label.spelling);
            template.push(']');
            rest = &reference[end..];
        } else {
            template.push('%');
            rest = &rest[1..];
        }
    }
    template.push_str(rest);
    Ok(template)
}

/// A fail-closed lowering diagnostic.  Every unsupported source construct is
/// reported rather than replaced with a placeholder value or terminator.
#[derive(Debug, Clone, PartialEq)]
pub enum LowerError {
    UnknownBuiltin {
        name: String,
        span: SourceSpan,
    },
    TargetBuiltin {
        name: String,
        span: SourceSpan,
    },
    Profile(evocc_chir::ProfileRefusal),
    Linkage(evocc_chir::linkage::LinkageError),
    Inline {
        span: SourceSpan,
        cycle: bool,
        reason: String,
    },
    UnknownSymbol {
        symbol: SymbolId,
        span: SourceSpan,
    },
    UnknownType {
        ty: TypeId,
        span: SourceSpan,
    },
    UnsupportedType {
        ty: TypeId,
        span: SourceSpan,
        feature: &'static str,
    },
    UnsupportedExpression {
        span: SourceSpan,
        feature: &'static str,
    },
    UnsupportedStatement {
        span: SourceSpan,
        feature: &'static str,
    },
    FloatingCallInterface {
        span: SourceSpan,
    },
    FloatingValue {
        span: SourceSpan,
    },
    CallArity {
        function: String,
        span: SourceSpan,
    },
    ReturnAddress {
        span: SourceSpan,
    },
    FrameAddress {
        span: SourceSpan,
    },
    FunctionPointerArithmetic {
        span: SourceSpan,
    },
    VolatileAggregate {
        span: SourceSpan,
    },
    WideInteger {
        span: SourceSpan,
    },
    StaticRelocation {
        span: SourceSpan,
    },
    ComputedGoto {
        span: SourceSpan,
    },
    Asm {
        span: SourceSpan,
    },
    AsmOperand {
        span: SourceSpan,
        reason: String,
    },
    VarArgs {
        span: SourceSpan,
    },
    /// A body that forwards the GNU argument pack was not substituted.
    VarArgPack {
        span: SourceSpan,
    },
    MalformedFunction {
        function: String,
        span: SourceSpan,
        reason: &'static str,
    },
    InvalidControlFlow {
        span: SourceSpan,
        reason: &'static str,
    },
    Layout {
        ty: TypeId,
        span: SourceSpan,
        source: LayoutError,
    },
    InvalidGeneratedSsa {
        message: String,
    },
}

impl fmt::Display for LowerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownBuiltin { name, .. } => {
                write!(formatter, "unknown-builtin: {name}")
            }
            Self::TargetBuiltin { name, .. } => {
                write!(formatter, "not-lowered: target-builtin: {name}")
            }
            Self::Profile(error) => formatter.write_str(error.message),
            Self::Linkage(error) => formatter.write_str(error.message),
            Self::Inline { reason, .. } => formatter.write_str(reason),
            Self::UnknownSymbol { symbol, .. } => {
                write!(formatter, "unknown C-HIR symbol {}", symbol.raw())
            }
            Self::UnknownType { ty, .. } => write!(formatter, "unknown C-HIR type {}", ty.raw()),
            Self::UnsupportedType { feature, .. }
            | Self::UnsupportedExpression { feature, .. }
            | Self::UnsupportedStatement { feature, .. } => formatter.write_str(feature),
            Self::FloatingCallInterface { .. } => {
                formatter.write_str("not-lowered: floating-call-interface")
            }
            Self::FloatingValue { .. } => formatter.write_str("not-lowered: floating-value"),
            Self::CallArity { function, .. } => write!(
                formatter,
                "malformed function {function}: call arity disagrees with the canonical prototype"
            ),
            Self::ReturnAddress { .. } => formatter.write_str("not-lowered: return-address"),
            Self::FrameAddress { .. } => formatter.write_str("not-lowered: frame-address"),
            Self::FunctionPointerArithmetic { .. } => {
                formatter.write_str("not-lowered: function-pointer-arithmetic")
            }
            Self::VolatileAggregate { .. } => {
                formatter.write_str("volatile-aggregate-copy-unsupported")
            }
            Self::WideInteger { .. } => formatter.write_str("not-executed: wide-integer"),
            Self::StaticRelocation { .. } => formatter.write_str("not-lowered: static-relocation"),
            Self::ComputedGoto { .. } => formatter.write_str("not-lowered: computed-goto"),
            Self::Asm { .. } => formatter.write_str("not-lowered: asm"),
            Self::AsmOperand { reason, .. } => write!(formatter, "not-lowered: asm: {reason}"),
            Self::VarArgs { .. } => formatter.write_str("not-lowered: varargs"),
            Self::VarArgPack { .. } => formatter.write_str("invalid-va-arg-pack"),
            Self::MalformedFunction {
                function, reason, ..
            } => write!(formatter, "malformed function {function}: {reason}"),
            Self::InvalidControlFlow { reason, .. } => formatter.write_str(reason),
            Self::Layout { ty, source, .. } => {
                write!(formatter, "cannot lay out type {}: {source:?}", ty.raw())
            }
            Self::InvalidGeneratedSsa { message } => {
                write!(formatter, "generated invalid EvoIR: {message}")
            }
        }
    }
}

impl std::error::Error for LowerError {}

impl LowerError {
    /// Stable diagnostic code for the C-HIR to EvoIR lowering boundary.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownBuiltin { .. } => "E2949",
            Self::TargetBuiltin { .. } => "E3052",
            Self::Profile(error) => error.code,
            Self::Linkage(error) => error.code,
            Self::Inline { cycle: true, .. } => "E2932",
            Self::Asm { .. } => "E3021",
            Self::AsmOperand { .. } => "E3021",
            Self::ReturnAddress { .. } => "E3025",
            Self::StaticRelocation { .. } => "E3026",
            Self::ComputedGoto { .. } => "E3029",
            Self::VarArgs { .. } => "E3030",
            Self::VarArgPack { .. } => "E2950",
            Self::FrameAddress { .. } => "E3031",
            Self::WideInteger { .. } => "E3033",
            Self::FunctionPointerArithmetic { .. } => "E3034",
            Self::VolatileAggregate { .. } => "E2922",
            Self::FloatingCallInterface { .. } => "E3049",
            Self::FloatingValue { .. } => "E3050",
            Self::CallArity { .. } => "E3051",
            _ => "E3101",
        }
    }

    /// Source location associated with a source-derived lowering failure.
    ///
    /// Generated-SSA validation failures have no single C source location and
    /// are therefore reported at the driver's stable `1:1` fallback.
    #[must_use]
    pub const fn span(&self) -> Option<SourceSpan> {
        match self {
            Self::UnknownBuiltin { span, .. } => Some(*span),
            Self::TargetBuiltin { span, .. } => Some(*span),
            Self::Profile(error) => Some(error.span),
            Self::Linkage(error) => Some(error.span),
            Self::Inline { span, .. }
            | Self::UnknownSymbol { span, .. }
            | Self::UnknownType { span, .. }
            | Self::UnsupportedType { span, .. }
            | Self::UnsupportedExpression { span, .. }
            | Self::UnsupportedStatement { span, .. }
            | Self::FloatingCallInterface { span }
            | Self::FloatingValue { span }
            | Self::CallArity { span, .. }
            | Self::ReturnAddress { span }
            | Self::FrameAddress { span }
            | Self::FunctionPointerArithmetic { span }
            | Self::VolatileAggregate { span }
            | Self::WideInteger { span }
            | Self::StaticRelocation { span }
            | Self::ComputedGoto { span }
            | Self::Asm { span }
            | Self::AsmOperand { span, .. }
            | Self::VarArgs { span }
            | Self::VarArgPack { span }
            | Self::MalformedFunction { span, .. }
            | Self::InvalidControlFlow { span, .. }
            | Self::Layout { span, .. } => Some(*span),
            Self::InvalidGeneratedSsa { .. } => None,
        }
    }

    /// Stable driver-facing diagnostic message.
    #[must_use]
    pub fn message(&self) -> String {
        format!("evoir-lowering-failed: {self}")
    }
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    section: Option<String>,
    patchable_entry: Option<(u16, u16)>,
    name: String,
    variadic: bool,
    ordinal: u32,
    parameter_symbols: Vec<Option<SymbolId>>,
    parameter_types: Vec<FunctionValueType>,
    return_type: Option<FunctionValueType>,
    constant_result: Option<CInteger>,
    participant: bool,
    /// EvoIR 10.1.9: a direct call of this function returns twice.
    returns_twice: bool,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct FunctionValueType {
    source: TypeId,
    ir: EvoIRType,
    integer: CIntegerType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum EnvKey {
    Local(SymbolId),
    Address(SymbolId),
    Compound(usize),
    ReturnDestination,
    Temporary(u32),
    TemporaryAddress(u32),
    Memory,
    World,
}

#[derive(Debug, Clone)]
struct BoundValue {
    value: Value,
    integer: Option<CIntegerType>,
}

type Environment = BTreeMap<EnvKey, BoundValue>;

#[derive(Debug, Clone)]
struct State {
    block: BlockId,
    env: Environment,
    locals: BTreeSet<SymbolId>,
    /// Reached declarations, in order; reserving storage is not activation.
    cleanups: Vec<SymbolId>,
}

#[derive(Debug, Default)]
struct Flow {
    normal: Vec<State>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockRole {
    Ordinary,
    SwitchExit,
    LoopStep,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ZeroInitializer {
    Values,
    /// Whole-object bytes (including padding) are already zero. Only typed
    /// pointer stores remain necessary, to establish Null provenance.
    PointerProvenance,
}

#[derive(Debug)]
struct DraftBlock {
    id: BlockId,
    args: Vec<(EnvKey, BlockArg)>,
    insts: Vec<Inst>,
    terminator: Option<Inst>,
    role: BlockRole,
}

#[derive(Debug, Clone)]
struct JumpTarget {
    id: BlockId,
    keys: Vec<EnvKey>,
}

#[derive(Debug, Clone, Default)]
struct ControlTargets {
    break_target: Option<JumpTarget>,
    continue_target: Option<JumpTarget>,
}

#[derive(Debug, Clone)]
struct ExprValue {
    value: Value,
    integer: CIntegerType,
}

enum AssemblyConstant {
    Integer(CInteger),
    Address(String, i64),
}

enum AssemblyInputSource {
    Constant(AssemblyConstant),
    Temporary(EnvKey),
}

enum AssemblyWriteback {
    Local(SymbolId),
    Memory {
        address: EnvKey,
        layout: AccessLayout,
        volatile: bool,
    },
}

struct FunctionLowerer<'a> {
    unit: &'a CHirUnit<'a>,
    target: &'a TargetDataLayout,
    functions: &'a BTreeMap<SymbolId, FunctionInfo>,
    info: &'a FunctionInfo,
    memory_symbols: &'a BTreeSet<SymbolId>,
    global_registers: &'a BTreeMap<SymbolId, String>,
    local_registers: BTreeMap<SymbolId, String>,
    allocation_origins: &'a BTreeMap<SymbolId, u64>,
    compound_origins: BTreeMap<usize, (u64, bool)>,
    dynamic_origins: BTreeMap<usize, u64>,
    compound_scopes: BTreeMap<usize, BTreeMap<usize, (TypeId, SourceSpan)>>,
    string_origins: BTreeMap<usize, u64>,
    immutable_globals: &'a BTreeMap<String, GlobalObject>,
    blocks: Vec<DraftBlock>,
    next_value: u32,
    next_block: BlockId,
    next_temporary: u32,
    next_synthetic_origin: u32,
    next_placeholder: BlockId,
    goto_names: BTreeSet<String>,
    has_computed_goto: bool,
    indirect_branches: Vec<(State, Value)>,
    goto_counts: BTreeMap<String, usize>,
    labels: BTreeMap<String, JumpTarget>,
    case_labels: BTreeMap<usize, String>,
    address_labels: BTreeMap<String, BlockId>,
    pending_gotos: BTreeMap<String, Vec<State>>,
    object_declarations: BTreeMap<SymbolId, &'a ObjectDecl<'a>>,
    constant_objects: BTreeSet<SymbolId>,
    unstable_objects: BTreeSet<SymbolId>,
    optimize: bool,
    size_symbols: &'a BTreeSet<SymbolId>,
    active_control: ControlTargets,
    fenv_access: bool,
    source_span: SourceSpan,
    source_values: Option<BTreeMap<u32, SourceSpan>>,
}

/// Diagnostic origins are not part of the serialized or executable IR.
#[derive(Default)]
pub struct SourceLocations {
    functions: BTreeMap<String, (SourceSpan, BTreeMap<u32, SourceSpan>)>,
}

impl SourceLocations {
    /// Resolve a result-producing instruction, or the predicate of an obligation.
    /// Synthetic control flow has no expression origin and uses the declaration.
    pub fn instruction(&self, function: &str, instruction: &Inst) -> Option<SourceSpan> {
        let (_, values) = self.functions.get(function)?;
        let value = match instruction {
            Inst::UbAssume(condition, _) => Some(condition),
            _ => inst_results(instruction).first().copied(),
        }?;
        values.get(&value.id).copied()
    }

    pub fn declaration(&self, function: &str) -> Option<SourceSpan> {
        self.functions.get(function).map(|(span, _)| *span)
    }
}

/// Lower a complete C-HIR translation unit into one validated EvoIR module.
///
/// Function order is the C-HIR declaration order.  The target layout is an
/// explicit input and is consulted for every scalar type admitted by this
/// Day-6 lowering slice.
pub fn lower_unit(unit: &CHirUnit<'_>, target: &TargetDataLayout) -> Result<Module, LowerError> {
    lower_unit_with_optimization(unit, target, false)
}

/// Enable optional integer-flow and dead-internal-body simplification for
/// optimized builds. Source semantics and required inline substitution are
/// identical in either mode; unoptimized clients keep their original CFG.
pub fn lower_unit_with_optimization(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    optimize: bool,
) -> Result<Module, LowerError> {
    lower_unit_with_source(unit, target, optimize, |_, module| module)
}

/// Consume the module together with the exact normalized source inventory
/// that produced it. Debug consumers must not match lifted inline statics
/// against pre-substitution names or repeat the substitution pass themselves.
pub fn lower_unit_with_source<R>(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    optimize: bool,
    consume: impl FnOnce(&CHirUnit<'_>, Module) -> R,
) -> Result<R, LowerError> {
    lower_unit_with_origins(unit, target, optimize, false, &[], |source, module, _| {
        consume(source, module)
    })
}

/// Lower selected externally callable entries without deriving their parameter
/// values from the closed set of C callers. The original source stays intact.
pub fn lower_unit_with_entries<R>(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    optimize: bool,
    entries: &[&str],
    consume: impl FnOnce(&CHirUnit<'_>, Module) -> R,
) -> Result<R, LowerError> {
    lower_unit_with_origins(
        unit,
        target,
        optimize,
        false,
        entries,
        |source, module, _| consume(source, module),
    )
}

/// Keep source origins for diagnostic execution. Ordinary compilation does not
/// allocate this map. Value origins follow final SSA renumbering.
pub fn lower_unit_with_locations<R>(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    optimize: bool,
    consume: impl FnOnce(&CHirUnit<'_>, Module, SourceLocations) -> R,
) -> Result<R, LowerError> {
    lower_unit_with_origins(unit, target, optimize, true, &[], consume)
}

fn lower_unit_with_origins<R>(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    optimize: bool,
    record: bool,
    entries: &[&str],
    consume: impl FnOnce(&CHirUnit<'_>, Module, SourceLocations) -> R,
) -> Result<R, LowerError> {
    let mut locations = record.then(SourceLocations::default);
    unit.symbol_linkage().map_err(LowerError::Linkage)?;
    // Check the entire source inventory before selecting emitted bodies. Its
    // allocation identities must remain stable for cross-interpreter traces.
    let source_manifest = AllocationSiteManifest::for_target(unit, target).map_err(|error| {
        LowerError::InvalidGeneratedSsa {
            message: error.to_string(),
        }
    })?;
    let retained = evocc_chir::inline::without_unreferenced_functions_with_entries(unit, entries)
        .map_err(LowerError::Linkage)?;
    let unit = retained.as_ref();
    if let Some(refusal) = unit.profile_refusals().iter().find(|refusal| {
        unit.iter().any(|declaration| {
            matches!(declaration, CHirDecl::Function(f)
            if f.body.is_some() && f.symbol == refusal.function)
        })
    }) {
        return Err(LowerError::Profile(refusal.clone()));
    }
    if evocc_chir::inline::needs_substitution(unit, target) {
        let arena = evocc_common::Arena::default();
        let rewritten = evocc_chir::inline::substitute_with_entries(unit, &arena, target, entries)
            .map_err(|e| LowerError::Inline {
                span: e.span,
                cycle: e.cycle,
                reason: e.message,
            })?;
        let module = lower_resolved_unit(&rewritten, target, None, optimize, &mut locations)?;
        return Ok(consume(&rewritten, module, locations.unwrap_or_default()));
    }
    let module = lower_resolved_unit(
        unit,
        target,
        Some(&source_manifest),
        optimize,
        &mut locations,
    )?;
    Ok(consume(unit, module, locations.unwrap_or_default()))
}

#[derive(Debug)]
struct SwitchSegment<'a> {
    case: Option<SwitchCase<'a>>,
    items: Vec<SwitchSegmentItem<'a>>,
}

#[derive(Debug, Clone, Copy)]
enum SwitchCase<'a> {
    Single(&'a CHirExpr<'a>),
    Range(&'a CHirExpr<'a>, &'a CHirExpr<'a>),
}

#[derive(Debug, Clone, Copy)]
enum SwitchSegmentItem<'a> {
    Declaration(&'a CHirDecl<'a>),
    Statement(&'a CHirStmt<'a>),
}

#[derive(Default)]
struct FunctionReferences {
    objects: BTreeMap<SymbolId, TypeId>,
    unstable_objects: BTreeSet<SymbolId>,
    frame_addresses: BTreeSet<SymbolId>,
    addressed_compound: bool,
    has_computed_goto: bool,
    local_registers: BTreeMap<SymbolId, String>,
    direct: BTreeSet<SymbolId>,
    any: bool,
    labels: BTreeMap<usize, String>,
    defined_labels: BTreeSet<String>,
    // Expression evaluation can transfer control too (notably scoped_guard's
    // for-step statement expression). Use the same complete reference walk;
    // the discarded sizeof operand is intentionally not an execution root.
    goto_names: BTreeSet<String>,
    goto_counts: BTreeMap<String, usize>,
    has_case_label: bool,
    has_unbound_case_label: bool,
    switch_depth: usize,
    scope: usize,
    compound_scopes: BTreeMap<usize, BTreeMap<usize, (TypeId, SourceSpan)>>,
}
