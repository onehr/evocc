//! Target-aware C-HIR to block-argument SSA EvoIR lowering.
//!
//! Day 6 keeps scalar locals in SSA values.  Memory and world tokens are
//! nevertheless present on every call edge so Day 7 can add real storage
//! semantics without changing the call instruction ABI.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use evocc_chir::integer::{
    eval_binary_integer, eval_unary_integer, integer_literal_value, integer_type_for_rank,
    usual_arithmetic_type, CInteger, CIntegerType,
};
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
    AssemblyOutput, Block, BlockArg, BlockId, CmpOp, ExternalObject, Function, FunctionDeclaration,
    FunctionParam, GlobalKind, GlobalObject, GlobalVisibility, IndexSignedness, InlineAssembly,
    Inst, Module, OffsetDirection, TrapReason, UbReason, Value,
};
use evocc_types::{
    enum_integer_rank, AddressSpace, ArrayBound, EvoIRType, FunctionParameters, LayoutError,
    Region, TagDefinition, TagState, TargetDataLayout, TypeId, TypeInterner, TypeKind,
};

const BREAK_TARGET_BASE: BlockId = u32::MAX - 1_000_000;

/// A fail-closed lowering diagnostic.  Every unsupported source construct is
/// reported rather than replaced with a placeholder value or terminator.
#[derive(Debug, Clone, PartialEq)]
pub enum LowerError {
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
    ReturnAddress {
        span: SourceSpan,
    },
    FrameAddress {
        span: SourceSpan,
    },
    PointerDifference {
        span: SourceSpan,
    },
    FunctionPointerArithmetic {
        span: SourceSpan,
    },
    GlobalRegisterVariable {
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
            Self::UnknownSymbol { symbol, .. } => {
                write!(formatter, "unknown C-HIR symbol {}", symbol.raw())
            }
            Self::UnknownType { ty, .. } => write!(formatter, "unknown C-HIR type {}", ty.raw()),
            Self::UnsupportedType { feature, .. }
            | Self::UnsupportedExpression { feature, .. }
            | Self::UnsupportedStatement { feature, .. } => formatter.write_str(feature),
            Self::ReturnAddress { .. } => formatter.write_str("not-lowered: return-address"),
            Self::FrameAddress { .. } => formatter.write_str("not-lowered: frame-address"),
            Self::PointerDifference { .. } => {
                formatter.write_str("not-lowered: pointer-difference")
            }
            Self::FunctionPointerArithmetic { .. } => {
                formatter.write_str("not-lowered: function-pointer-arithmetic")
            }
            Self::GlobalRegisterVariable { .. } => {
                formatter.write_str("not-lowered: global-register-variable")
            }
            Self::WideInteger { .. } => formatter.write_str("not-executed: wide-integer"),
            Self::StaticRelocation { .. } => formatter.write_str("not-lowered: static-relocation"),
            Self::ComputedGoto { .. } => formatter.write_str("not-lowered: computed-goto"),
            Self::Asm { .. } => formatter.write_str("not-lowered: asm"),
            Self::AsmOperand { reason, .. } => write!(formatter, "not-lowered: asm: {reason}"),
            Self::VarArgs { .. } => formatter.write_str("not-lowered: varargs"),
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
            Self::Asm { .. } => "E3021",
            Self::AsmOperand { .. } => "E3021",
            Self::ReturnAddress { .. } => "E3025",
            Self::StaticRelocation { .. } => "E3026",
            Self::ComputedGoto { .. } => "E3029",
            Self::VarArgs { .. } => "E3030",
            Self::FrameAddress { .. } => "E3031",
            Self::PointerDifference { .. } => "E3032",
            Self::WideInteger { .. } => "E3033",
            Self::FunctionPointerArithmetic { .. } => "E3034",
            Self::GlobalRegisterVariable { .. } => "E3035",
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
            Self::UnknownSymbol { span, .. }
            | Self::UnknownType { span, .. }
            | Self::UnsupportedType { span, .. }
            | Self::UnsupportedExpression { span, .. }
            | Self::UnsupportedStatement { span, .. }
            | Self::ReturnAddress { span }
            | Self::FrameAddress { span }
            | Self::PointerDifference { span }
            | Self::FunctionPointerArithmetic { span }
            | Self::GlobalRegisterVariable { span }
            | Self::WideInteger { span }
            | Self::StaticRelocation { span }
            | Self::ComputedGoto { span }
            | Self::Asm { span }
            | Self::AsmOperand { span, .. }
            | Self::VarArgs { span }
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
    name: String,
    variadic: bool,
    ordinal: u32,
    parameter_symbols: Vec<SymbolId>,
    parameter_types: Vec<FunctionValueType>,
    return_type: Option<FunctionValueType>,
    participant: bool,
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
    ReturnDestination,
    Temporary(u32),
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

struct FunctionLowerer<'a> {
    unit: &'a CHirUnit<'a>,
    target: &'a TargetDataLayout,
    functions: &'a BTreeMap<SymbolId, FunctionInfo>,
    info: &'a FunctionInfo,
    memory_symbols: &'a BTreeSet<SymbolId>,
    allocation_origins: &'a BTreeMap<SymbolId, u64>,
    compound_origins: BTreeMap<usize, (u64, bool)>,
    string_origins: BTreeMap<usize, u64>,
    blocks: Vec<DraftBlock>,
    next_value: u32,
    next_block: BlockId,
    next_temporary: u32,
    next_synthetic_origin: u32,
    next_placeholder: BlockId,
    goto_names: BTreeSet<String>,
    labels: BTreeMap<String, JumpTarget>,
    pending_gotos: BTreeMap<String, Vec<State>>,
    active_control: ControlTargets,
}

/// Lower a complete C-HIR translation unit into one validated EvoIR module.
///
/// Function order is the C-HIR declaration order.  The target layout is an
/// explicit input and is consulted for every scalar type admitted by this
/// Day-6 lowering slice.
pub fn lower_unit(unit: &CHirUnit<'_>, target: &TargetDataLayout) -> Result<Module, LowerError> {
    let global_register_symbols = collect_global_register_symbols(unit);
    let mut memory_symbols = collect_memory_symbols(unit);
    memory_symbols.retain(|symbol| !global_register_symbols.contains(symbol));
    let manifest =
        AllocationSiteManifest::derive(unit).map_err(|_| LowerError::InvalidGeneratedSsa {
            message: "cannot derive the authoritative allocation-site manifest".to_owned(),
        })?;
    let allocation_origins = manifest
        .sites()
        .iter()
        .filter_map(|site| {
            site.kind()
                .object_symbol()
                .map(|symbol| (symbol, site.ordinal().raw()))
        })
        .collect::<BTreeMap<_, _>>();
    let compound_origins = manifest
        .sites()
        .iter()
        .filter_map(|site| {
            site.kind()
                .compound_literal()
                .map(|(expression, static_storage)| {
                    (
                        expression as *const CHirExpr<'_> as usize,
                        (site.ordinal().raw(), static_storage),
                    )
                })
        })
        .collect::<BTreeMap<_, _>>();
    let memory_profile = !memory_symbols.is_empty()
        || unit.iter().any(|declaration| match declaration {
            CHirDecl::Function(function) => function
                .body
                .is_some_and(|body| evocc_chir::interp::statement_requires_memory(unit, body)),
            _ => false,
        })
        || manifest.sites().iter().any(|site| {
            site.kind().object_symbol().is_some_and(|symbol| {
                unit.symbols()
                    .symbol(symbol)
                    .ok()
                    .and_then(|entry| unit.types().get(entry.ty()))
                    .is_some_and(|ty| {
                        matches!(
                            ty.kind,
                            TypeKind::Pointer(_)
                                | TypeKind::Array { .. }
                                | TypeKind::Struct(_)
                                | TypeKind::Union(_)
                        ) || ty.qualifiers.is_volatile
                    })
            })
        });
    let functions = index_functions(unit, target, memory_profile)?;
    prevalidate_unit_for_lowering(unit, target, &global_register_symbols)?;
    let mut globals = Vec::new();
    let mut string_origins = BTreeMap::new();
    for site in manifest.sites() {
        let Some(expression) = site.kind().string_literal() else {
            continue;
        };
        let CHirExpr::StringLit(literal, span) = expression else {
            unreachable!("manifest string site")
        };
        let (bytes, align) =
            literal
                .object_bytes(target)
                .ok_or(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "invalid string representation",
                })?;
        let origin = site.ordinal().raw();
        string_origins.insert(expression as *const CHirExpr<'_> as usize, origin);
        globals.push(GlobalObject {
            relocs: Vec::new(),
            symbol: format!("__evocc.string.{origin}"),
            origin,
            kind: GlobalKind::StringLiteral,
            visibility: GlobalVisibility::Internal,
            layout: AccessLayout::new(bytes.len() as u64, align),
            initialized: vec![true; bytes.len()],
            bytes,
        });
    }
    globals.extend(lower_globals(
        unit,
        target,
        &memory_symbols,
        &allocation_origins,
        &string_origins,
    )?);
    let mut lowered = Vec::new();
    for declaration in unit.iter() {
        let CHirDecl::Function(function) = declaration else {
            continue;
        };
        if function.body.is_none() {
            continue;
        }
        let info =
            functions
                .get(&function.symbol)
                .ok_or_else(|| LowerError::MalformedFunction {
                    function: function.name.spelling.clone(),
                    span: function.span,
                    reason: "definition is absent from the function index",
                })?;
        lowered.push(
            FunctionLowerer::new(
                unit,
                target,
                &functions,
                info,
                &memory_symbols,
                &allocation_origins,
                &compound_origins,
                &string_origins,
            )
            .lower(function)?,
        );
    }
    let defined = lowered
        .iter()
        .map(|function| function.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut declares = BTreeMap::new();
    for info in functions
        .values()
        .filter(|info| !defined.contains(info.name.as_str()))
    {
        let aggregate_return = info.return_type.as_ref().is_some_and(|value| {
            matches!(
                unit.types().get(value.source).map(|ty| &ty.kind),
                Some(TypeKind::Struct(_) | TypeKind::Union(_))
            )
        });
        let mut params = info
            .parameter_types
            .iter()
            .map(|value| value.ir.clone())
            .collect::<Vec<_>>();
        if aggregate_return {
            params.push(EvoIRType::Ptr(AddressSpace::Generic));
        }
        params.extend([EvoIRType::Mem(Region::All), EvoIRType::World]);
        let mut ret_tys = info
            .return_type
            .iter()
            .filter(|_| !aggregate_return)
            .map(|value| value.ir.clone())
            .collect::<Vec<_>>();
        ret_tys.extend([EvoIRType::Mem(Region::All), EvoIRType::World]);
        declares.insert(
            info.name.clone(),
            FunctionDeclaration {
                name: info.name.clone(),
                variadic: info.variadic,
                params,
                ret_tys,
            },
        );
    }
    let global_names = globals
        .iter()
        .map(|global| global.symbol.as_str())
        .collect::<BTreeSet<_>>();
    let referenced_objects = lowered
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.insts)
        .filter_map(|instruction| match instruction {
            Inst::GlobalAddr { symbol, .. } if !global_names.contains(symbol.as_str()) => {
                Some(symbol.as_str())
            }
            _ => None,
        })
        .chain(
            globals
                .iter()
                .flat_map(|global| global.relocs.iter())
                .filter_map(|relocation| {
                    (!global_names.contains(relocation.symbol.as_str()))
                        .then_some(relocation.symbol.as_str())
                }),
        )
        .collect::<BTreeSet<_>>();
    let mut extern_objects = BTreeMap::new();
    for symbol in &memory_symbols {
        let entry =
            unit.symbols()
                .symbol(*symbol)
                .map_err(|_| LowerError::InvalidGeneratedSsa {
                    message: "external object has no symbol".into(),
                })?;
        if !referenced_objects.contains(entry.spelling())
            || matches!(
                unit.types().get(entry.ty()).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. })
            )
        {
            continue;
        }
        let (size, align) = match target.layout_of(unit.types(), entry.ty()) {
            Ok(layout) => (Some(layout.size_bytes), layout.align_bytes),
            Err(_) => match unit.types().get(entry.ty()).map(|ty| &ty.kind) {
                Some(TypeKind::Array {
                    element,
                    bound: ArrayBound::Unbounded,
                }) => {
                    let layout = target.layout_of(unit.types(), *element).map_err(|source| {
                        LowerError::Layout {
                            ty: *element,
                            span: entry.span(),
                            source,
                        }
                    })?;
                    (None, layout.align_bytes)
                }
                _ => {
                    return Err(LowerError::UnsupportedType {
                        ty: entry.ty(),
                        span: entry.span(),
                        feature: "external object has no object layout",
                    })
                }
            },
        };
        extern_objects.insert(
            entry.spelling().to_owned(),
            ExternalObject {
                symbol: entry.spelling().to_owned(),
                size,
                align: align.max(entry.declared_alignment()),
            },
        );
    }
    let mut module = Module {
        assembly: unit
            .iter()
            .filter_map(|decl| match decl {
                CHirDecl::Asm(assembly) => Some(assembly.template.clone()),
                _ => None,
            })
            .collect(),
        globals,
        extern_objects: extern_objects.into_values().collect(),
        declares: declares.into_values().collect(),
        functions: lowered,
    };
    module.recompute_effect_summaries();
    validate_module(&module).map_err(|message| LowerError::InvalidGeneratedSsa { message })?;
    Ok(module)
}

fn collect_global_register_symbols(unit: &CHirUnit<'_>) -> BTreeSet<SymbolId> {
    unit.iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object)
                if object.storage == StorageClass::Register && object.asm_label.is_some() =>
            {
                Some(object.symbol)
            }
            _ => None,
        })
        .collect()
}

fn collect_memory_symbols(unit: &CHirUnit<'_>) -> BTreeSet<SymbolId> {
    fn declaration(unit: &CHirUnit<'_>, declaration: &CHirDecl<'_>, out: &mut BTreeSet<SymbolId>) {
        if let CHirDecl::Object(object) = declaration {
            let is_aggregate = matches!(
                unit.types().get(object.ty).map(|ty| &ty.kind),
                Some(
                    TypeKind::Array { .. }
                        | TypeKind::Struct(_)
                        | TypeKind::Union(_)
                        | TypeKind::BuiltinVaList
                )
            );
            let is_qualified_memory = unit
                .types()
                .get(object.ty)
                .is_some_and(|ty| ty.qualifiers.is_volatile || ty.qualifiers.is_atomic);
            let is_file = unit
                .symbols()
                .symbol(object.symbol)
                .is_ok_and(|symbol| symbol.scope() == unit.symbols().file_scope());
            if is_aggregate
                || is_file
                || is_qualified_memory
                || object.storage == StorageClass::Static
            {
                out.insert(object.symbol);
            }
            if let Some(initializer) = &object.initializer {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } = action
                    {
                        expression(unit, value, out);
                    }
                }
            }
        } else if let CHirDecl::Function(function) = declaration {
            if let ParameterSyntax::Prototype { parameters, .. } = &function.parameters {
                for parameter in parameters {
                    if matches!(
                        unit.types().get(parameter.ty).map(|ty| &ty.kind),
                        Some(TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList)
                    ) {
                        if let Some(symbol) = parameter.symbol {
                            out.insert(symbol);
                        }
                    }
                }
            }
        }
    }

    fn place(unit: &CHirUnit<'_>, place: &MemoryPlace<'_>, out: &mut BTreeSet<SymbolId>) {
        if let Some(symbol) = place.root_symbol() {
            out.insert(symbol);
        }
        for source in place.source_expressions() {
            expression(unit, source, out);
        }
    }

    fn expression(unit: &CHirUnit<'_>, expr: &CHirExpr<'_>, out: &mut BTreeSet<SymbolId>) {
        match expr {
            CHirExpr::Ident { symbol, .. } => {
                if unit.symbols().symbol(*symbol).is_ok_and(|entry| {
                    entry.kind() == SymbolKind::Object
                        && entry.scope() == unit.symbols().file_scope()
                }) {
                    out.insert(*symbol);
                }
            }
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => {
                if let CHirExpr::Ident { symbol, .. } = operand {
                    out.insert(*symbol);
                }
                expression(unit, operand, out);
            }
            CHirExpr::Unary(_, operand, _)
            | CHirExpr::EvaluateOnce(operand, _)
            | CHirExpr::Cast(_, operand, _)
            | CHirExpr::BitBuiltin { operand, .. }
            | CHirExpr::Member(operand, _, _)
            | CHirExpr::PtrMember(operand, _, _) => expression(unit, operand, out),
            CHirExpr::Binary(_, left, right, _)
            | CHirExpr::Index(left, right, _)
            | CHirExpr::Comma(left, right, _)
            | CHirExpr::Elvis(left, right, _) => {
                expression(unit, left, out);
                expression(unit, right, out);
            }
            CHirExpr::Ternary(condition, left, right, _) => {
                expression(unit, condition, out);
                expression(unit, left, out);
                expression(unit, right, out);
            }
            CHirExpr::Call(callee, arguments, _) => {
                expression(unit, callee, out);
                for argument in arguments {
                    expression(unit, argument, out);
                }
            }
            CHirExpr::CheckedArithmetic {
                lhs, rhs, target, ..
            } => {
                expression(unit, lhs, out);
                expression(unit, rhs, out);
                expression(unit, target, out);
            }
            CHirExpr::BuiltinVarArgs { first, second, .. } => {
                expression(unit, first, out);
                if let Some(second) = second {
                    expression(unit, second, out);
                }
            }
            CHirExpr::Assign { target, value, .. } => {
                expression(unit, target, out);
                expression(unit, value, out);
            }
            CHirExpr::Update { target, .. } => expression(unit, target, out),
            CHirExpr::MemoryRead { place: memory, .. }
            | CHirExpr::MemoryUpdate { place: memory, .. } => place(unit, memory, out),
            CHirExpr::MemoryAssign {
                place: memory,
                value,
                ..
            } => {
                place(unit, memory, out);
                expression(unit, value, out);
            }
            CHirExpr::CompoundLit(_, initializer, _) => {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } = action
                    {
                        expression(unit, value, out);
                    }
                }
            }
            CHirExpr::StatementExpr { body, .. } => statement(unit, body, out),
            CHirExpr::Lit(..)
            | CHirExpr::FloatLit(..)
            | CHirExpr::CharLit(..)
            | CHirExpr::StringLit(..)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::SizeOf(..)
            | CHirExpr::SizeOfExpr(..)
            | CHirExpr::AlignOf(..)
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { .. } => {}
        }
    }

    fn statement(unit: &CHirUnit<'_>, stmt: &CHirStmt<'_>, out: &mut BTreeSet<SymbolId>) {
        match stmt {
            CHirStmt::Asm(assembly) => {
                for operand in
                    assembly
                        .outputs
                        .iter()
                        .chain(assembly.inputs.iter().filter(|operand| {
                            evocc_evoir::assembly_memory_constraint(&operand.constraint)
                        }))
                {
                    if let CHirExpr::Ident { symbol, .. } = operand.expression {
                        out.insert(*symbol);
                    }
                }
                for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                    expression(unit, operand.expression, out);
                }
            }
            CHirStmt::Expr(expr, _) | CHirStmt::Return(Some(expr), _) => {
                expression(unit, expr, out)
            }
            CHirStmt::ComputedGoto(expr, _) => expression(unit, expr, out),
            CHirStmt::If(condition, left, right, _) => {
                expression(unit, condition, out);
                statement(unit, left, out);
                if let Some(right) = right {
                    statement(unit, right, out);
                }
            }
            CHirStmt::Switch(expr, body, _)
            | CHirStmt::While(expr, body, _)
            | CHirStmt::Case(expr, body, _) => {
                expression(unit, expr, out);
                statement(unit, body, out);
            }
            CHirStmt::CaseRange(first, last, body, _) => {
                expression(unit, first, out);
                expression(unit, last, out);
                statement(unit, body, out);
            }
            CHirStmt::DoWhile(body, expr, _) => {
                statement(unit, body, out);
                expression(unit, expr, out);
            }
            CHirStmt::For(initializer, condition, step, body, _) => {
                match initializer {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(expr) => expression(unit, expr, out),
                    CHirForInit::Declaration(declarations) => {
                        for item in declarations {
                            declaration(unit, item, out);
                        }
                    }
                }
                if let Some(expr) = condition {
                    expression(unit, expr, out);
                }
                if let Some(expr) = step {
                    expression(unit, expr, out);
                }
                statement(unit, body, out);
            }
            CHirStmt::Compound(items, _) => {
                for item in items {
                    match item {
                        CHirBlockItem::Declaration(item) => declaration(unit, item, out),
                        CHirBlockItem::Statement(item) => statement(unit, item, out),
                    }
                }
            }
            CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => statement(unit, body, out),
            CHirStmt::Return(None, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::AttributedNull(_, _)
            | CHirStmt::Null(_) => {}
        }
    }

    let mut symbols = BTreeSet::new();
    for declaration_node in unit.iter() {
        declaration(unit, declaration_node, &mut symbols);
        if let CHirDecl::Function(function) = declaration_node {
            if let Some(body) = function.body {
                statement(unit, body, &mut symbols);
            }
        }
    }
    symbols
}

fn lower_globals(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    memory_symbols: &BTreeSet<SymbolId>,
    origins: &BTreeMap<SymbolId, u64>,
    strings: &BTreeMap<usize, u64>,
) -> Result<Vec<GlobalObject>, LowerError> {
    fn collect_nested<'a>(
        statement: &'a CHirStmt<'a>,
        out: &mut BTreeMap<SymbolId, &'a ObjectDecl<'a>>,
    ) {
        match statement {
            CHirStmt::Compound(items, _) => {
                for item in items {
                    match item {
                        CHirBlockItem::Declaration(CHirDecl::Object(object)) => {
                            if object.storage == StorageClass::Static {
                                out.insert(object.symbol, object);
                            }
                        }
                        CHirBlockItem::Declaration(_) => {}
                        CHirBlockItem::Statement(statement) => collect_nested(statement, out),
                    }
                }
            }
            CHirStmt::If(_, left, right, _) => {
                collect_nested(left, out);
                if let Some(right) = right {
                    collect_nested(right, out);
                }
            }
            CHirStmt::Switch(_, body, _)
            | CHirStmt::While(_, body, _)
            | CHirStmt::DoWhile(body, _, _)
            | CHirStmt::For(_, _, _, body, _)
            | CHirStmt::Case(_, body, _)
            | CHirStmt::CaseRange(_, _, body, _)
            | CHirStmt::Default(body, _)
            | CHirStmt::Label(_, body, _) => collect_nested(body, out),
            CHirStmt::Expr(..)
            | CHirStmt::Asm(_)
            | CHirStmt::Return(..)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(..)
            | CHirStmt::ComputedGoto(..)
            | CHirStmt::AttributedNull(_, _)
            | CHirStmt::Null(_) => {}
        }
    }

    let mut declarations = BTreeMap::<SymbolId, &ObjectDecl<'_>>::new();
    for declaration in unit.iter() {
        match declaration {
            CHirDecl::Object(object)
                if memory_symbols.contains(&object.symbol)
                    && origins.contains_key(&object.symbol) =>
            {
                let canonical_type = unit
                    .symbols()
                    .symbol(object.symbol)
                    .ok()
                    .map(|symbol| symbol.ty());
                declarations
                    .entry(object.symbol)
                    .and_modify(|selected| {
                        if object.initializer.is_some()
                            || (selected.initializer.is_none()
                                && canonical_type == Some(object.ty)
                                && canonical_type != Some(selected.ty))
                        {
                            *selected = object;
                        }
                    })
                    .or_insert(object);
            }
            CHirDecl::Function(function) => {
                if let Some(body) = function.body {
                    collect_nested(body, &mut declarations);
                }
            }
            _ => {}
        }
    }
    declarations
        .into_values()
        .map(|object| {
            let symbol =
                unit.symbols()
                    .symbol(object.symbol)
                    .map_err(|_| LowerError::UnknownSymbol {
                        symbol: object.symbol,
                        span: object.span,
                    })?;
            let layout = target
                .layout_of(unit.types(), object.ty)
                .map_err(|source| LowerError::Layout {
                    ty: object.ty,
                    span: object.span,
                    source,
                })?;
            let mut object_size = layout.size_bytes;
            if let Some(initializer) = &object.initializer {
                if initializer.plan().flexible_array_elements() != 0 {
                    for action in initializer.plan().actions() {
                        let action_span = action.span().unwrap_or(object.span);
                        let (offset, actual_ty) =
                            subobject_offset(unit, target, object.ty, action.path(), action_span)?;
                        if actual_ty != action.destination_type() || offset < 0 {
                            return Err(LowerError::UnsupportedStatement {
                                span: action_span,
                                feature: "flexible initializer path disagrees with its object",
                            });
                        }
                        let destination =
                            target
                                .layout_of(unit.types(), actual_ty)
                                .map_err(|source| LowerError::Layout {
                                    ty: actual_ty,
                                    span: action_span,
                                    source,
                                })?;
                        object_size = object_size.max(
                            u64::try_from(offset)
                                .ok()
                                .and_then(|offset| offset.checked_add(destination.size_bytes))
                                .ok_or(LowerError::UnsupportedStatement {
                                    span: action_span,
                                    feature: "flexible initializer range overflow",
                                })?,
                        );
                    }
                }
            }
            let size = usize::try_from(object_size).map_err(|_| LowerError::UnsupportedType {
                ty: object.ty,
                span: object.span,
                feature: "global object exceeds host representation",
            })?;
            let mut bytes = vec![0_u8; size];
            let mut relocs = Vec::new();
            if let Some(initializer) = &object.initializer {
                for action in initializer.plan().actions() {
                    apply_static_initializer(
                        unit,
                        target,
                        object.ty,
                        action,
                        &mut bytes,
                        &mut relocs,
                        origins,
                        strings,
                    )?;
                }
            }
            relocs.sort_by_key(|relocation: &evocc_evoir::Relocation| relocation.offset);
            let origin = *origins
                .get(&object.symbol)
                .ok_or(LowerError::InvalidGeneratedSsa {
                    message: format!("global {} has no allocation origin", object.symbol.raw()),
                })?;
            let file_scope = symbol.scope() == unit.symbols().file_scope();
            Ok(GlobalObject {
                relocs,
                symbol: if file_scope {
                    symbol.spelling().to_owned()
                } else {
                    format!("__evocc.static.{origin}")
                },
                origin,
                kind: if file_scope {
                    GlobalKind::File
                } else {
                    GlobalKind::FunctionStatic
                },
                visibility: if file_scope && object.storage != StorageClass::Static {
                    GlobalVisibility::External
                } else {
                    GlobalVisibility::Internal
                },
                layout: AccessLayout::new(
                    object_size,
                    layout.align_bytes.max(symbol.declared_alignment()),
                ),
                bytes,
                initialized: vec![true; size],
            })
        })
        .collect()
}

/// Resolve an admitted static address without inventing a runtime object.
fn symbolic_address(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
    address_of: bool,
    origins: &BTreeMap<SymbolId, u64>,
    strings: &BTreeMap<usize, u64>,
) -> Result<(String, i64), LowerError> {
    let span = expression.span();
    let error = || LowerError::StaticRelocation { span };
    let symbol_address = |id: SymbolId| -> Result<(String, i64), LowerError> {
        let symbol = unit.symbols().symbol(id).map_err(|_| error())?;
        if symbol.kind() == SymbolKind::Function || symbol.scope() == unit.symbols().file_scope() {
            Ok((symbol.spelling().to_owned(), 0))
        } else if symbol.storage_class() == Some(StorageClass::Static) {
            Ok((
                format!("__evocc.static.{}", origins.get(&id).ok_or_else(error)?),
                0,
            ))
        } else {
            Err(error())
        }
    };
    match expression {
        CHirExpr::Cast(ty, operand, _) => {
            let canonical = unit.types().get(*ty).ok_or_else(error)?;
            if !matches!(canonical.kind, TypeKind::Pointer(_))
                && target
                    .layout_of(unit.types(), *ty)
                    .map_err(|_| error())?
                    .size_bytes
                    < 8
            {
                return Err(error());
            }
            symbolic_address(unit, target, operand, false, origins, strings)
        }
        CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => {
            symbolic_address(unit, target, operand, true, origins, strings)
        }
        CHirExpr::Unary(UnaryOp::Deref, operand, _) if address_of => {
            symbolic_address(unit, target, operand, false, origins, strings)
        }
        CHirExpr::Ident { symbol, facts, .. }
            if address_of
                || unit.types().get(facts.ty()).is_some_and(|ty| {
                    matches!(ty.kind, TypeKind::Array { .. } | TypeKind::Function { .. })
                }) =>
        {
            symbol_address(*symbol)
        }
        CHirExpr::StringLit(..) => {
            let origin = strings
                .get(&(expression as *const CHirExpr<'_> as usize))
                .ok_or_else(error)?;
            Ok((format!("__evocc.string.{origin}"), 0))
        }
        CHirExpr::MemoryRead { place, .. }
            if address_of
                || unit
                    .types()
                    .get(place.value_ty())
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Array { .. })) =>
        {
            let (symbol, mut offset) = match place.root() {
                PlaceRoot::Object { symbol } => symbol_address(*symbol)?,
                PlaceRoot::PointerExpression { pointer, .. } => {
                    symbolic_address(unit, target, pointer, false, origins, strings)?
                }
                PlaceRoot::CompoundLiteral { .. } => return Err(error()),
            };
            for projection in place.projections() {
                let delta = match projection {
                    PlaceProjection::Field(field) => {
                        let layout = target
                            .record_layout(unit.types(), field.tag())
                            .map_err(|_| error())?;
                        i64::try_from(
                            layout
                                .fields
                                .iter()
                                .find(|entry| entry.field_index == field.field_index())
                                .ok_or_else(error)?
                                .byte_offset,
                        )
                        .map_err(|_| error())?
                    }
                    PlaceProjection::Index(index) => {
                        let value = evocc_chir::interp::interpret_static_integer_expression(
                            unit,
                            index.index(),
                            target,
                        )
                        .map_err(|_| error())?;
                        let signed = value.cast(CIntegerType::LONG).raw_bits() as i64;
                        let size = target
                            .layout_of(unit.types(), index.element_ty())
                            .map_err(|_| error())?
                            .size_bytes;
                        signed
                            .checked_mul(i64::try_from(size).map_err(|_| error())?)
                            .ok_or_else(error)?
                    }
                };
                offset = offset.checked_add(delta).ok_or_else(error)?;
            }
            Ok((symbol, offset))
        }
        CHirExpr::Binary(op @ (BinaryOp::Add | BinaryOp::Sub), pointer, index, _) => {
            let (symbol, offset) =
                symbolic_address(unit, target, pointer, false, origins, strings)?;
            let value =
                evocc_chir::interp::interpret_static_integer_expression(unit, index, target)
                    .map_err(|_| error())?;
            let scale =
                evocc_chir::PointerTypeDesc::from_expression(unit.symbols(), unit.types(), pointer)
                    .ok()
                    .and_then(|ty| target.layout_of(unit.types(), ty.pointee()).ok())
                    .map_or(1, |layout| layout.size_bytes);
            let delta = (value.cast(CIntegerType::LONG).raw_bits() as i64)
                .checked_mul(i64::try_from(scale).map_err(|_| error())?)
                .ok_or_else(error)?;
            let offset = if *op == BinaryOp::Add {
                offset.checked_add(delta)
            } else {
                offset.checked_sub(delta)
            }
            .ok_or_else(error)?;
            Ok((symbol, offset))
        }
        CHirExpr::Ternary(condition, left, right, _) => {
            let condition = if let Ok(value) =
                evocc_chir::interp::interpret_static_integer_expression(unit, condition, target)
            {
                value.raw_bits() != 0
            } else if let CHirExpr::Binary(op @ (BinaryOp::Eq | BinaryOp::Ne), a, b, _) = condition
            {
                let a = if null_pointer_constant(unit, target, a) {
                    None
                } else {
                    Some(symbolic_address(unit, target, a, false, origins, strings)?)
                };
                let b = if null_pointer_constant(unit, target, b) {
                    None
                } else {
                    Some(symbolic_address(unit, target, b, false, origins, strings)?)
                };
                (a == b) == (*op == BinaryOp::Eq)
            } else {
                symbolic_address(unit, target, condition, false, origins, strings)?;
                true
            };
            symbolic_address(
                unit,
                target,
                if condition { left } else { right },
                address_of,
                origins,
                strings,
            )
        }
        _ => Err(error()),
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_static_initializer(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    object_ty: TypeId,
    action: &InitAction<'_>,
    bytes: &mut [u8],
    relocs: &mut Vec<evocc_evoir::Relocation>,
    origins: &BTreeMap<SymbolId, u64>,
    strings: &BTreeMap<usize, u64>,
) -> Result<(), LowerError> {
    let span = action
        .span()
        .unwrap_or_else(|| SourceSpan::new(evocc_common::FileId::new(0), 0, 0, 1, 1));
    let (offset, _) = subobject_offset(unit, target, object_ty, action.path(), span)?;
    let offset = usize::try_from(offset).map_err(|_| LowerError::UnsupportedStatement {
        span,
        feature: "negative static initializer offset",
    })?;
    let destination = action.destination_type();
    let layout = target
        .layout_of(unit.types(), destination)
        .map_err(|source| LowerError::Layout {
            ty: destination,
            span,
            source,
        })?;
    let end = offset
        .checked_add(usize::try_from(layout.size_bytes).map_err(|_| {
            LowerError::UnsupportedType {
                ty: destination,
                span,
                feature: "initializer destination exceeds host representation",
            }
        })?)
        .ok_or(LowerError::UnsupportedStatement {
            span,
            feature: "initializer range overflow",
        })?;
    let target_bytes = bytes
        .get_mut(offset..end)
        .ok_or(LowerError::UnsupportedStatement {
            span,
            feature: "initializer range lies outside global object",
        })?;
    relocs
        .retain(|relocation| relocation.offset < offset as u64 || relocation.offset >= end as u64);
    match action {
        InitAction::ZeroScalar { .. } => target_bytes.fill(0),
        InitAction::StoreString { literal, .. } => {
            let TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(count),
            } = unit
                .types()
                .get(destination)
                .ok_or(LowerError::UnknownType {
                    ty: destination,
                    span,
                })?
                .kind
            else {
                return Err(LowerError::UnsupportedType {
                    ty: destination,
                    span,
                    feature: "string initializer destination is not a fixed array",
                });
            };
            let width = usize::try_from(
                target
                    .layout_of(unit.types(), element)
                    .map_err(|source| LowerError::Layout {
                        ty: element,
                        span,
                        source,
                    })?
                    .size_bytes,
            )
            .map_err(|_| LowerError::UnsupportedType {
                ty: element,
                span,
                feature: "string code unit exceeds host representation",
            })?;
            let count = usize::try_from(count).map_err(|_| LowerError::UnsupportedType {
                ty: destination,
                span,
                feature: "string destination length exceeds host representation",
            })?;
            let mut decoded = literal
                .decoded_units()
                .ok_or(LowerError::UnsupportedStatement {
                    span,
                    feature: "invalid string initializer",
                })?;
            decoded.resize(count, 0);
            if width == 0 || width > 4 || target_bytes.len() != count.saturating_mul(width) {
                return Err(LowerError::UnsupportedType {
                    ty: destination,
                    span,
                    feature: "string code unit layout is unsupported",
                });
            }
            for (unit, bytes) in decoded
                .into_iter()
                .zip(target_bytes.chunks_exact_mut(width))
            {
                let encoded = match target.byte_order() {
                    evocc_types::ByteOrder::Little => unit.to_le_bytes(),
                    evocc_types::ByteOrder::Big => unit.to_be_bytes(),
                };
                let source = match target.byte_order() {
                    evocc_types::ByteOrder::Little => &encoded[..width],
                    evocc_types::ByteOrder::Big => &encoded[4 - width..],
                };
                bytes.copy_from_slice(source);
            }
        }
        InitAction::StoreScalar { value, .. } => {
            // A numeric cast is not a pointer relocation. Section 2.1 keeps
            // pointer-valued static initializers outside the byte-only schema.
            if matches!(
                unit.types().get(destination).map(|ty| &ty.kind),
                Some(TypeKind::Pointer(_))
            ) {
                if null_pointer_constant(unit, target, value) {
                    target_bytes.fill(0);
                    return Ok(());
                }
                let (symbol, addend) =
                    symbolic_address(unit, target, value, false, origins, strings)?;
                if target_bytes.len() != 8 {
                    return Err(LowerError::StaticRelocation { span });
                }
                target_bytes.fill(0);
                relocs.push(evocc_evoir::Relocation {
                    offset: offset as u64,
                    symbol,
                    addend,
                });
                return Ok(());
            }
            let integer =
                evocc_chir::interp::interpret_static_integer_expression(unit, value, target)
                    .map_err(|_| LowerError::UnsupportedExpression {
                        span,
                        feature: "global initializer is not an admitted integer constant",
                    })?;
            let destination_integer = canonical_integer(unit.types(), target, destination, span)?;
            let raw = integer.cast(destination_integer).raw_bits();
            let byte_len = target_bytes.len();
            for (index, byte) in target_bytes.iter_mut().enumerate() {
                let shift = match target.byte_order() {
                    evocc_types::ByteOrder::Little => index * 8,
                    evocc_types::ByteOrder::Big => (byte_len - index - 1) * 8,
                };
                *byte = raw
                    .checked_shr(u32::try_from(shift).unwrap_or(u32::MAX))
                    .unwrap_or(0) as u8;
            }
        }
        InitAction::CopyAggregate { span, .. } => {
            return Err(LowerError::UnsupportedExpression {
                span: *span,
                feature: "global aggregate copy initializer",
            });
        }
    }
    Ok(())
}

fn subobject_offset(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    object_ty: TypeId,
    path: &SubobjectPath,
    span: SourceSpan,
) -> Result<(i64, TypeId), LowerError> {
    let mut current = object_ty;
    let mut offset = 0_i64;
    for step in path.steps() {
        match step {
            SubobjectStep::Field { field_index } => {
                let tag = match unit.types().get(current).map(|ty| &ty.kind) {
                    Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => *tag,
                    _ => {
                        return Err(LowerError::UnsupportedType {
                            ty: current,
                            span,
                            feature: "initializer field path crosses a non-record",
                        });
                    }
                };
                let layout = target.record_layout(unit.types(), tag).map_err(|source| {
                    LowerError::Layout {
                        ty: current,
                        span,
                        source,
                    }
                })?;
                let field = layout
                    .fields
                    .iter()
                    .find(|field| field.field_index == *field_index)
                    .ok_or(LowerError::UnsupportedType {
                        ty: current,
                        span,
                        feature: "initializer field path has no target field",
                    })?;
                if field.bit_field.is_some() {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature: "bit-field initializer is outside Day 7",
                    });
                }
                offset = offset
                    .checked_add(i64::try_from(field.byte_offset).map_err(|_| {
                        LowerError::UnsupportedType {
                            ty: current,
                            span,
                            feature: "record field offset exceeds i64",
                        }
                    })?)
                    .ok_or(LowerError::UnsupportedType {
                        ty: current,
                        span,
                        feature: "record field offset overflow",
                    })?;
                current = unit
                    .types()
                    .tag(tag)
                    .and_then(|definition| match &definition.state {
                        TagState::Complete(
                            TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                        ) => fields.get(*field_index).map(|field| field.ty),
                        _ => None,
                    })
                    .ok_or(LowerError::UnsupportedType {
                        ty: current,
                        span,
                        feature: "initializer field type is unavailable",
                    })?;
            }
            SubobjectStep::Index { index } => {
                let element = match unit.types().get(current).map(|ty| &ty.kind) {
                    Some(TypeKind::Array { element, .. }) => *element,
                    _ => {
                        return Err(LowerError::UnsupportedType {
                            ty: current,
                            span,
                            feature: "initializer index path crosses a non-array",
                        });
                    }
                };
                let layout = target.layout_of(unit.types(), element).map_err(|source| {
                    LowerError::Layout {
                        ty: element,
                        span,
                        source,
                    }
                })?;
                let delta = index
                    .checked_mul(layout.size_bytes)
                    .and_then(|value| i64::try_from(value).ok())
                    .ok_or(LowerError::UnsupportedType {
                        ty: element,
                        span,
                        feature: "initializer array offset overflow",
                    })?;
                offset = offset
                    .checked_add(delta)
                    .ok_or(LowerError::UnsupportedType {
                        ty: element,
                        span,
                        feature: "initializer array offset overflow",
                    })?;
                current = element;
            }
        }
    }
    Ok((offset, current))
}

fn prevalidate_unit_for_lowering(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    global_register_symbols: &BTreeSet<SymbolId>,
) -> Result<(), LowerError> {
    evocc_chir::validate_front_end_builtin_records(unit, target).map_err(|span| {
        LowerError::UnsupportedExpression {
            span,
            feature: "inconsistent front-end builtin record",
        }
    })?;
    for declaration in unit.iter() {
        match declaration {
            CHirDecl::Asm(_) => {}
            CHirDecl::Function(function) => {
                if let Some(body) = function.body {
                    let mut labels = BTreeMap::new();
                    prevalidate_stmt_for_lowering(
                        unit,
                        body,
                        &mut labels,
                        global_register_symbols,
                    )?;
                    let mut goto_names = BTreeSet::new();
                    collect_goto_names(body, &mut goto_names);
                    if goto_names.iter().any(|name| !labels.contains_key(name)) {
                        return Err(LowerError::InvalidControlFlow {
                            span: function.span,
                            reason: "goto targets a label absent from its function",
                        });
                    }
                }
            }
            CHirDecl::Object(object) => {
                validate_object_declaration_for_lowering(unit, object)?;
            }
            CHirDecl::Tag(_) | CHirDecl::Typedef(_) => {}
        }
    }
    Ok(())
}

fn prevalidate_decl_for_lowering(
    unit: &CHirUnit<'_>,
    declaration: &CHirDecl<'_>,
    global_register_symbols: &BTreeSet<SymbolId>,
) -> Result<(), LowerError> {
    match declaration {
        CHirDecl::Asm(assembly) => Err(LowerError::Asm {
            span: assembly.span,
        }),
        CHirDecl::Object(object) => {
            validate_object_declaration_for_lowering(unit, object)?;
            if let Some(initializer) = &object.initializer {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } = action
                    {
                        prevalidate_expr_for_lowering(unit, value, global_register_symbols)?;
                    }
                }
            }
            Ok(())
        }
        CHirDecl::Tag(_) | CHirDecl::Typedef(_) => Ok(()),
        CHirDecl::Function(function) => Err(LowerError::UnsupportedStatement {
            span: function.span,
            feature: "block function declaration is outside scalar lowering",
        }),
    }
}

fn prevalidate_stmt_for_lowering(
    unit: &CHirUnit<'_>,
    statement: &CHirStmt<'_>,
    labels: &mut BTreeMap<String, SourceSpan>,
    global_register_symbols: &BTreeSet<SymbolId>,
) -> Result<(), LowerError> {
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                prevalidate_expr_for_lowering(unit, operand.expression, global_register_symbols)?;
            }
            Ok(())
        }
        CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
            prevalidate_expr_for_lowering(unit, expression, global_register_symbols)
        }
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            prevalidate_expr_for_lowering(unit, condition, global_register_symbols)?;
            prevalidate_stmt_for_lowering(unit, then_branch, labels, global_register_symbols)?;
            if let Some(else_branch) = else_branch {
                prevalidate_stmt_for_lowering(unit, else_branch, labels, global_register_symbols)?;
            }
            Ok(())
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            prevalidate_expr_for_lowering(unit, expression, global_register_symbols)?;
            prevalidate_stmt_for_lowering(unit, body, labels, global_register_symbols)
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            prevalidate_expr_for_lowering(unit, first, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, last, global_register_symbols)?;
            prevalidate_stmt_for_lowering(unit, body, labels, global_register_symbols)
        }
        CHirStmt::DoWhile(body, expression, _) => {
            prevalidate_stmt_for_lowering(unit, body, labels, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, expression, global_register_symbols)
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => {
                    prevalidate_expr_for_lowering(unit, expression, global_register_symbols)?;
                }
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        prevalidate_decl_for_lowering(unit, declaration, global_register_symbols)?;
                    }
                }
            }
            if let Some(condition) = condition {
                prevalidate_expr_for_lowering(unit, condition, global_register_symbols)?;
            }
            if let Some(step) = step {
                prevalidate_expr_for_lowering(unit, step, global_register_symbols)?;
            }
            prevalidate_stmt_for_lowering(unit, body, labels, global_register_symbols)
        }
        CHirStmt::Compound(items, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        prevalidate_decl_for_lowering(unit, declaration, global_register_symbols)?;
                    }
                    CHirBlockItem::Statement(statement) => {
                        prevalidate_stmt_for_lowering(
                            unit,
                            statement,
                            labels,
                            global_register_symbols,
                        )?;
                    }
                }
            }
            Ok(())
        }
        CHirStmt::Label(label, nested, span) => {
            if labels.insert(label.spelling.clone(), *span).is_some() {
                return Err(LowerError::InvalidControlFlow {
                    span: *span,
                    reason: "duplicate label in function",
                });
            }
            prevalidate_stmt_for_lowering(unit, nested, labels, global_register_symbols)
        }
        CHirStmt::Default(nested, _) => {
            prevalidate_stmt_for_lowering(unit, nested, labels, global_register_symbols)
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => Ok(()),
        CHirStmt::ComputedGoto(_, span) => Err(LowerError::ComputedGoto { span: *span }),
    }
}

fn prevalidate_expr_for_lowering(
    unit: &CHirUnit<'_>,
    expression: &CHirExpr<'_>,
    global_register_symbols: &BTreeSet<SymbolId>,
) -> Result<(), LowerError> {
    match expression {
        CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span,
        } => {
            if global_register_symbols.contains(symbol) {
                return Err(LowerError::GlobalRegisterVariable { span: *span });
            }
            let declaration =
                unit.symbols()
                    .symbol(*symbol)
                    .map_err(|_| LowerError::UnknownSymbol {
                        symbol: *symbol,
                        span: *span,
                    })?;
            let category_matches = match declaration.kind() {
                SymbolKind::Function => facts.category() == ExprCategory::FunctionDesignator,
                SymbolKind::Enumerator => facts.category() == ExprCategory::Value,
                SymbolKind::Object | SymbolKind::Parameter => {
                    facts.category() == ExprCategory::LValue
                }
                SymbolKind::Typedef => false,
            };
            if declaration.spelling() != spelling
                || declaration.ty() != facts.ty()
                || !category_matches
            {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "identifier disagrees with its semantic identity",
                });
            }
            Ok(())
        }
        CHirExpr::Assign {
            place,
            target,
            value,
            facts,
            span,
            ..
        } => {
            validate_place_identity(unit, *place, target, *span)?;
            if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "assignment result facts disagree with its scalar place",
                });
            }
            prevalidate_expr_for_lowering(unit, target, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, value, global_register_symbols)
        }
        CHirExpr::Update {
            place,
            target,
            facts,
            span,
            ..
        } => {
            validate_place_identity(unit, *place, target, *span)?;
            if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "update result facts disagree with its scalar place",
                });
            }
            prevalidate_expr_for_lowering(unit, target, global_register_symbols)
        }
        CHirExpr::MemoryRead { place, span, .. } => {
            place
                .validate_against(unit.symbols(), unit.types())
                .map_err(|_| LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory read place is not authoritative in this C-HIR unit",
                })?;
            for source in place.source_expressions() {
                prevalidate_expr_for_lowering(unit, source, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::MemoryAssign {
            place,
            value,
            facts,
            span,
            ..
        } => {
            place
                .validate_against(unit.symbols(), unit.types())
                .map_err(|_| LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory assignment place is not authoritative in this C-HIR unit",
                })?;
            if facts.ty() != place.value_ty() || facts.category() != ExprCategory::Value {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory assignment result facts disagree with its place",
                });
            }
            for source in place.source_expressions() {
                prevalidate_expr_for_lowering(unit, source, global_register_symbols)?;
            }
            prevalidate_expr_for_lowering(unit, value, global_register_symbols)
        }
        CHirExpr::MemoryUpdate {
            place, facts, span, ..
        } => {
            place
                .validate_against(unit.symbols(), unit.types())
                .map_err(|_| LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory update place is not authoritative in this C-HIR unit",
                })?;
            if facts.ty() != place.value_ty() || facts.category() != ExprCategory::Value {
                return Err(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "memory update result facts disagree with its place",
                });
            }
            for source in place.source_expressions() {
                prevalidate_expr_for_lowering(unit, source, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::Cast(ty, operand, span) => {
            if unit.types().get(*ty).is_none() {
                return Err(LowerError::UnknownType {
                    ty: *ty,
                    span: *span,
                });
            }
            prevalidate_expr_for_lowering(unit, operand, global_register_symbols)
        }
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => {
            prevalidate_expr_for_lowering(unit, operand, global_register_symbols)
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _) => {
            prevalidate_expr_for_lowering(unit, left, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, right, global_register_symbols)
        }
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            prevalidate_expr_for_lowering(unit, condition, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, then_value, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, else_value, global_register_symbols)
        }
        CHirExpr::Call(callee, arguments, _) => {
            prevalidate_expr_for_lowering(unit, callee, global_register_symbols)?;
            for argument in arguments {
                prevalidate_expr_for_lowering(unit, argument, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            prevalidate_expr_for_lowering(unit, lhs, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, rhs, global_register_symbols)?;
            prevalidate_expr_for_lowering(unit, target, global_register_symbols)
        }
        CHirExpr::BitBuiltin { operand, .. } => {
            prevalidate_expr_for_lowering(unit, operand, global_register_symbols)
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            prevalidate_expr_for_lowering(unit, first, global_register_symbols)?;
            if let Some(second) = second {
                prevalidate_expr_for_lowering(unit, second, global_register_symbols)?;
            }
            Ok(())
        }
        CHirExpr::BuiltinReturnAddress { span, .. } => {
            Err(LowerError::ReturnAddress { span: *span })
        }
        CHirExpr::BuiltinFrameAddress { span, .. } => Err(LowerError::FrameAddress { span: *span }),
        CHirExpr::LabelAddress { span, .. } => Err(LowerError::ComputedGoto { span: *span }),
        CHirExpr::BuiltinUnreachable { .. } => Ok(()),
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    prevalidate_expr_for_lowering(unit, value, global_register_symbols)?;
                }
            }
            Ok(())
        }
        CHirExpr::StatementExpr { body, .. } => {
            let mut labels = BTreeMap::new();
            prevalidate_stmt_for_lowering(unit, body, &mut labels, global_register_symbols)
        }
        CHirExpr::SizeOf(ty, span)
        | CHirExpr::SizeOfExpr(_, ty, span)
        | CHirExpr::AlignOf(ty, span) => {
            if unit.types().get(*ty).is_none() {
                Err(LowerError::UnknownType {
                    ty: *ty,
                    span: *span,
                })
            } else {
                Ok(())
            }
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..) => Ok(()),
    }
}

fn validate_place_identity(
    unit: &CHirUnit<'_>,
    place: ScalarPlace,
    target: &CHirExpr<'_>,
    span: SourceSpan,
) -> Result<(), LowerError> {
    let CHirExpr::Ident {
        symbol,
        spelling,
        facts,
        span: target_span,
    } = target
    else {
        return Err(LowerError::UnsupportedExpression {
            span,
            feature: "assignment target is not a scalar identifier place",
        });
    };
    let declaration = unit
        .symbols()
        .symbol(*symbol)
        .map_err(|_| LowerError::UnknownSymbol {
            symbol: *symbol,
            span: *target_span,
        })?;
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
    {
        return Err(LowerError::UnsupportedExpression {
            span: *target_span,
            feature: "scalar place disagrees with its semantic identity",
        });
    }
    Ok(())
}

fn index_functions(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    memory_participant: bool,
) -> Result<BTreeMap<SymbolId, FunctionInfo>, LowerError> {
    let mut required = BTreeSet::new();
    for declaration in unit.iter() {
        let mut references = CallSites::default();
        collect_calls_decl(declaration, &mut references);
        if let CHirDecl::Function(function) = declaration {
            if let Some(body) = function.body {
                required.insert(function.symbol);
                collect_calls_stmt(body, &mut references);
            }
        }
        required.extend(references.direct);
    }
    let mut definitions = BTreeMap::new();
    for declaration in unit.iter() {
        let CHirDecl::Function(function) = declaration else {
            continue;
        };
        if !required.contains(&function.symbol) {
            continue;
        }
        let info = function_info(unit, target, function)?;
        definitions
            .entry(function.symbol)
            .and_modify(|existing: &mut FunctionInfo| {
                if function.body.is_some() {
                    *existing = info.clone();
                }
            })
            .or_insert(info);
    }

    let mut participants = BTreeSet::new();
    for declaration in unit.iter() {
        let CHirDecl::Function(function) = declaration else {
            continue;
        };
        let Some(body) = function.body else {
            continue;
        };
        let mut calls = CallSites::default();
        collect_calls_stmt(body, &mut calls);
        if calls.any {
            participants.insert(function.symbol);
        }
        participants.extend(calls.direct);
    }
    for symbol in participants {
        let Some(info) = definitions.get_mut(&symbol) else {
            let span = unit
                .symbols()
                .symbol(symbol)
                .map(|entry| entry.span())
                .unwrap_or_else(|_| SourceSpan::new(evocc_common::FileId::new(0), 0, 0, 0, 0));
            return Err(LowerError::UnknownSymbol { symbol, span });
        };
        info.participant = true;
    }
    if memory_participant {
        for info in definitions.values_mut() {
            info.participant = true;
        }
    }
    for (ordinal, info) in definitions.values_mut().enumerate() {
        info.ordinal = u32::try_from(ordinal).map_err(|_| LowerError::InvalidGeneratedSsa {
            message: "function count exceeds the aggregate temporary origin namespace".to_owned(),
        })?;
    }
    Ok(definitions)
}

fn function_info(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    function: &FunctionDecl<'_>,
) -> Result<FunctionInfo, LowerError> {
    let symbol = unit
        .symbols()
        .symbol(function.symbol)
        .map_err(|_| LowerError::UnknownSymbol {
            symbol: function.symbol,
            span: function.span,
        })?;
    if symbol.kind() != SymbolKind::Function
        || symbol.spelling() != function.name.spelling
        || !unit.types().compatible(symbol.ty(), function.ty)
        || !symbol
            .storage_class()
            .is_some_and(|storage| function_storage_compatible(storage, function.storage))
        || !written_declarator_matches(
            unit.types(),
            &function.written,
            function.ty,
            function.return_specifier.type_id(),
            Some(&function.parameters),
            false,
            false,
        )
    {
        return Err(LowerError::MalformedFunction {
            function: function.name.spelling.clone(),
            span: function.span,
            reason: "function declaration disagrees with its semantic identity",
        });
    }
    let canonical = unit
        .types()
        .get(function.ty)
        .ok_or(LowerError::UnknownType {
            ty: function.ty,
            span: function.span,
        })?;
    let TypeKind::Function {
        return_type,
        parameters,
    } = &canonical.kind
    else {
        return Err(LowerError::MalformedFunction {
            function: function.name.spelling.clone(),
            span: function.span,
            reason: "declaration does not have a canonical function type",
        });
    };
    let return_type = match &unit
        .types()
        .get(*return_type)
        .ok_or(LowerError::UnknownType {
            ty: *return_type,
            span: function.span,
        })?
        .kind
    {
        TypeKind::Void => None,
        _ => Some(function_value_type(
            unit.types(),
            target,
            *return_type,
            function.span,
        )?),
    };
    let types: &[TypeId] = match parameters {
        FunctionParameters::Prototype { types, .. } => types,
        FunctionParameters::Unspecified if function.body.is_none() => {
            unit.unprototyped_parameters(function.symbol).unwrap_or(&[])
        }
        FunctionParameters::Unspecified => {
            return Err(LowerError::MalformedFunction {
                function: function.name.spelling.clone(),
                span: function.span,
                reason: "function definition requires a prototype parameter list",
            });
        }
    };
    let parameter_types = types
        .iter()
        .map(|ty| function_value_type(unit.types(), target, *ty, function.span))
        .collect::<Result<Vec<_>, _>>()?;
    let parameter_symbols = match &function.parameters {
        ParameterSyntax::Prototype { parameters, .. }
            if function.body.is_some() && parameters.len() == parameter_types.len() =>
        {
            let mut ids = BTreeSet::new();
            let mut spellings = BTreeSet::new();
            let mut function_scope = None;
            parameters
                .iter()
                .zip(types)
                .map(|(parameter, canonical_type)| {
                    let (Some(parameter_symbol), Some(parameter_name)) =
                        (parameter.symbol, &parameter.name)
                    else {
                        return Err(LowerError::MalformedFunction {
                            function: function.name.spelling.clone(),
                            span: parameter.span,
                            reason: "function definition has an unnamed parameter",
                        });
                    };
                    let symbol = unit.symbols().symbol(parameter_symbol).map_err(|_| {
                        LowerError::MalformedFunction {
                            function: function.name.spelling.clone(),
                            span: parameter.span,
                            reason: "parameter symbol is absent from the symbol table",
                        }
                    })?;
                    let scope = unit.symbols().scope(symbol.scope()).map_err(|_| {
                        LowerError::MalformedFunction {
                            function: function.name.spelling.clone(),
                            span: parameter.span,
                            reason: "parameter scope is absent from the symbol table",
                        }
                    })?;
                    let scope_is_valid = scope.kind() == ScopeKind::Function
                        && scope.parent() == Some(unit.symbols().file_scope())
                        && function_scope.is_none_or(|expected| expected == symbol.scope());
                    if *canonical_type != parameter.ty
                        || !written_declarator_matches(
                            unit.types(),
                            &parameter.written,
                            parameter.ty,
                            parameter.specifier.type_id(),
                            None,
                            false,
                            true,
                        )
                        || symbol.kind() != SymbolKind::Parameter
                        || symbol.spelling() != parameter_name.spelling
                        || symbol.ty() != parameter.ty
                        || !matches!(
                            symbol.storage_class(),
                            Some(StorageClass::None | StorageClass::Register)
                        )
                        || symbol.storage_class() != Some(parameter.storage)
                        || !scope_is_valid
                        || !ids.insert(parameter_symbol)
                        || !spellings.insert(parameter_name.spelling.clone())
                    {
                        return Err(LowerError::MalformedFunction {
                            function: function.name.spelling.clone(),
                            span: parameter.span,
                            reason: "parameter declaration disagrees with its semantic identity",
                        });
                    }
                    function_scope = Some(symbol.scope());
                    Ok(parameter_symbol)
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        ParameterSyntax::Prototype {
            parameters,
            variadic: false,
        } if function.body.is_none() && parameters.len() == parameter_types.len() => Vec::new(),
        _ if function.body.is_none() => Vec::new(),
        _ => {
            return Err(LowerError::MalformedFunction {
                function: function.name.spelling.clone(),
                span: function.span,
                reason: "written parameters do not match the canonical prototype",
            });
        }
    };
    Ok(FunctionInfo {
        name: function.name.spelling.clone(),
        variadic: matches!(
            parameters,
            FunctionParameters::Prototype { variadic: true, .. }
        ),
        ordinal: 0,
        parameter_symbols,
        parameter_types,
        return_type,
        participant: matches!(
            parameters,
            FunctionParameters::Prototype { variadic: true, .. }
        ),
        span: function.span,
    })
}

fn function_value_type(
    types: &TypeInterner,
    target: &TargetDataLayout,
    ty: TypeId,
    span: SourceSpan,
) -> Result<FunctionValueType, LowerError> {
    match types.get(ty).map(|canonical| &canonical.kind) {
        Some(TypeKind::Pointer(pointee))
            if matches!(
                types.get(*pointee).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. })
            ) =>
        {
            Ok(FunctionValueType {
                source: ty,
                ir: function_pointer_ir_type(types, target, *pointee, span)?,
                integer: CIntegerType::ULONG,
            })
        }
        Some(TypeKind::Pointer(pointee))
            if !matches!(
                types.get(*pointee).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. })
            ) =>
        {
            Ok(FunctionValueType {
                source: ty,
                ir: EvoIRType::Ptr(AddressSpace::Generic),
                integer: CIntegerType::ULONG,
            })
        }
        Some(TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_)) => {
            let integer = canonical_integer(types, target, ty, span)?;
            Ok(FunctionValueType {
                source: ty,
                ir: bits(integer),
                integer,
            })
        }
        Some(TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList) => {
            Ok(FunctionValueType {
                source: ty,
                ir: EvoIRType::Ptr(AddressSpace::Generic),
                integer: CIntegerType::ULONG,
            })
        }
        _ => Err(LowerError::UnsupportedType {
            ty,
            span,
            feature: "function value type is neither an integer nor object pointer",
        }),
    }
}

fn function_pointer_ir_type(
    types: &TypeInterner,
    target: &TargetDataLayout,
    function_ty: TypeId,
    span: SourceSpan,
) -> Result<EvoIRType, LowerError> {
    let TypeKind::Function {
        return_type,
        parameters,
    } = &types
        .get(function_ty)
        .ok_or(LowerError::UnknownType {
            ty: function_ty,
            span,
        })?
        .kind
    else {
        return Err(LowerError::UnsupportedType {
            ty: function_ty,
            span,
            feature: "fnptr pointee is not a function",
        });
    };
    let parameter_types = match parameters {
        FunctionParameters::Prototype { types, .. } => types,
        _ => {
            return Err(LowerError::UnsupportedType {
                ty: function_ty,
                span,
                feature: "fnptr requires a prototype",
            });
        }
    };
    let mut params = parameter_types
        .iter()
        .map(|ty| function_value_type(types, target, *ty, span).map(|value_type| value_type.ir))
        .collect::<Result<Vec<_>, _>>()?;
    let returned = match types.get(*return_type).map(|ty| &ty.kind) {
        Some(TypeKind::Void) => None,
        Some(_) => Some(function_value_type(types, target, *return_type, span)?),
        None => {
            return Err(LowerError::UnknownType {
                ty: *return_type,
                span,
            });
        }
    };
    if returned
        .as_ref()
        .is_some_and(|value_type| is_aggregate_type(types, value_type.source))
    {
        params.push(EvoIRType::Ptr(AddressSpace::Generic));
    }
    params.push(EvoIRType::Mem(Region::All));
    params.push(EvoIRType::World);
    let mut results = returned
        .filter(|value_type| !is_aggregate_type(types, value_type.source))
        .map(|value_type| vec![value_type.ir])
        .unwrap_or_default();
    results.push(EvoIRType::Mem(Region::All));
    results.push(EvoIRType::World);
    Ok(EvoIRType::FnPtr {
        params,
        results,
        variadic: matches!(
            parameters,
            FunctionParameters::Prototype { variadic: true, .. }
        ),
    })
}

fn is_aggregate_type(types: &TypeInterner, ty: TypeId) -> bool {
    matches!(
        types.get(ty).map(|canonical| &canonical.kind),
        Some(TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList)
    )
}

fn canonical_integer(
    types: &TypeInterner,
    target: &TargetDataLayout,
    ty: TypeId,
    span: SourceSpan,
) -> Result<CIntegerType, LowerError> {
    let canonical = types.get(ty).ok_or(LowerError::UnknownType { ty, span })?;
    let integer = match &canonical.kind {
        TypeKind::Int(rank) => integer_type_for_rank(*rank),
        TypeKind::GnuInt128(evocc_types::GnuInt128Signedness::Signed) => CIntegerType::INT128,
        TypeKind::GnuInt128(evocc_types::GnuInt128Signedness::Unsigned) => CIntegerType::UINT128,
        TypeKind::Enum(tag) => {
            let tag = types.tag(*tag).ok_or(LowerError::UnsupportedType {
                ty,
                span,
                feature: "unknown enum type",
            })?;
            let TagState::Complete(TagDefinition::Enum(constants)) = &tag.state else {
                return Err(LowerError::UnsupportedType {
                    ty,
                    span,
                    feature: "incomplete or non-enum scalar type",
                });
            };
            let minimum = constants
                .iter()
                .map(|constant| constant.value)
                .min()
                .ok_or(LowerError::UnsupportedType {
                    ty,
                    span,
                    feature: "empty enum has no canonical integer representation",
                })?;
            let maximum = constants
                .iter()
                .map(|constant| constant.value)
                .max()
                .ok_or(LowerError::UnsupportedType {
                    ty,
                    span,
                    feature: "empty enum has no canonical integer representation",
                })?;
            integer_type_for_rank(enum_integer_rank(minimum, maximum, tag.layout.packed))
        }
        _ => {
            return Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "non-integer scalar type is outside Day 6",
            });
        }
    };
    let layout = target
        .layout_of(types, ty)
        .map_err(|source| LowerError::Layout { ty, span, source })?;
    let expected_storage_bits = u64::from(integer.bits().max(8));
    if layout.size_bytes.saturating_mul(8) != expected_storage_bits {
        return Err(LowerError::UnsupportedType {
            ty,
            span,
            feature: "integer value width disagrees with target data layout",
        });
    }
    Ok(integer)
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
        | StorageClass::ThreadLocal => false,
    }
}

fn function_parameters_match(
    interner: &TypeInterner,
    canonical: &FunctionParameters,
    written: &ParameterSyntax,
) -> bool {
    match (canonical, written) {
        (FunctionParameters::Unspecified, ParameterSyntax::Unspecified) => true,
        (
            FunctionParameters::Prototype { types, variadic },
            ParameterSyntax::Prototype {
                parameters,
                variadic: written_variadic,
            },
        ) => {
            variadic == written_variadic
                && types.len() == parameters.len()
                && types.iter().zip(parameters).all(|(canonical, parameter)| {
                    interner.compatible_ignoring_top_qualifiers(*canonical, parameter.ty)
                })
        }
        _ => false,
    }
}

fn written_declarator_matches(
    types: &TypeInterner,
    written: &WrittenDeclarator,
    mut current: TypeId,
    terminal: TypeId,
    function_syntax: Option<&ParameterSyntax>,
    initialized_object: bool,
    adjusted_parameter: bool,
) -> bool {
    // Undo only the outer function-to-pointer parameter adjustment.
    if adjusted_parameter
        && (matches!(
            written.layers.first(),
            Some(WrittenDeclaratorLayer::Function { .. })
        ) || (written.layers.is_empty()
            && types
                .get(terminal)
                .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))))
    {
        if let Some(canonical) = types.get(current) {
            if let TypeKind::Pointer(pointee) = canonical.kind {
                if canonical.qualifiers == evocc_types::Qualifiers::default()
                    && types
                        .get(pointee)
                        .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))
                {
                    current = pointee;
                }
            }
        }
    }
    if let Some(syntax) = function_syntax {
        if !matches!(
            written.layers.first(),
            Some(WrittenDeclaratorLayer::Function { .. })
        ) {
            return written.layers.is_empty()
                && current == terminal
                && types.get(current).is_some_and(|canonical| {
                    matches!(
                        &canonical.kind,
                        TypeKind::Function { parameters, .. }
                            if function_parameters_match(types, parameters, syntax)
                    )
                });
        }
    }
    for (index, layer) in written.layers.iter().enumerate() {
        let Some(canonical) = types.get(current) else {
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
                    && function_syntax
                        .is_some_and(|syntax| !function_parameters_match(types, parameters, syntax))
                {
                    return false;
                }
                *return_type
            }
            (WrittenDeclaratorLayer::Pointer { qualifiers, .. }, TypeKind::Pointer(pointee))
                if canonical.qualifiers == *qualifiers =>
            {
                *pointee
            }
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
            ) if bound == canonical_bound
                || (initialized_object
                    && index == 0
                    && *bound == ArrayBound::Unbounded
                    && matches!(canonical_bound, ArrayBound::Fixed(_))) =>
            {
                *element
            }
            _ => return false,
        };
    }
    current == terminal
        || (adjusted_parameter
            && written.layers.is_empty()
            && matches!(
                (types.get(current).map(|ty| &ty.kind), types.get(terminal).map(|ty| &ty.kind)),
                (Some(TypeKind::Pointer(pointee)), Some(TypeKind::Array { element, .. })) if pointee == element
            ))
}

fn validate_object_declaration_for_lowering(
    unit: &CHirUnit<'_>,
    object: &ObjectDecl<'_>,
) -> Result<(), LowerError> {
    let symbol = unit
        .symbols()
        .symbol(object.symbol)
        .map_err(|_| LowerError::UnknownSymbol {
            symbol: object.symbol,
            span: object.span,
        })?;
    if !matches!(
        object.storage,
        StorageClass::None
            | StorageClass::Auto
            | StorageClass::Register
            | StorageClass::Extern
            | StorageClass::Static
    ) {
        return Err(LowerError::UnsupportedStatement {
            span: object.span,
            feature: "non-automatic scalar object is outside Day 6 lowering",
        });
    }
    if symbol.kind() != SymbolKind::Object
        || symbol.spelling() != object.name.spelling
        || !unit.types().compatible(symbol.ty(), object.ty)
        // AllocationSiteManifest has already checked storage compatibility
        // with lexical context, including an extern followed by a definition.
        || !written_declarator_matches(
            unit.types(),
            &object.written,
            object.ty,
            object.specifier.type_id(),
            None,
            object.initializer.is_some(),
            false,
        )
    {
        return Err(LowerError::UnsupportedStatement {
            span: object.span,
            feature: "object declaration disagrees with its semantic identity",
        });
    }
    if let Some(initializer) = &object.initializer {
        let plan = initializer.plan();
        if plan.target_type() != object.ty || plan.validate(unit.types()).is_err() {
            return Err(LowerError::UnsupportedStatement {
                span: initializer.span(),
                feature: "initializer plan disagrees with its semantic identity",
            });
        }
    }
    Ok(())
}

impl<'a> FunctionLowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        unit: &'a CHirUnit<'a>,
        target: &'a TargetDataLayout,
        functions: &'a BTreeMap<SymbolId, FunctionInfo>,
        info: &'a FunctionInfo,
        memory_symbols: &'a BTreeSet<SymbolId>,
        allocation_origins: &'a BTreeMap<SymbolId, u64>,
        compound_origins: &BTreeMap<usize, (u64, bool)>,
        string_origins: &BTreeMap<usize, u64>,
    ) -> Self {
        Self {
            unit,
            target,
            functions,
            info,
            memory_symbols,
            allocation_origins,
            compound_origins: compound_origins.clone(),
            string_origins: string_origins.clone(),
            blocks: Vec::new(),
            next_value: 0,
            next_block: 0,
            next_temporary: 0,
            next_synthetic_origin: 0,
            next_placeholder: BREAK_TARGET_BASE,
            goto_names: BTreeSet::new(),
            labels: BTreeMap::new(),
            pending_gotos: BTreeMap::new(),
            active_control: ControlTargets::default(),
        }
    }

    fn lower(mut self, declaration: &FunctionDecl<'a>) -> Result<Function, LowerError> {
        let body = declaration
            .body
            .ok_or_else(|| LowerError::MalformedFunction {
                function: declaration.name.spelling.clone(),
                span: declaration.span,
                reason: "attempted to lower a declaration without a body",
            })?;
        collect_goto_names(body, &mut self.goto_names);
        let mut params = Vec::new();
        let mut env = Environment::new();
        let mut entry_args = Vec::new();
        for (index, (symbol, value_type)) in self
            .info
            .parameter_symbols
            .iter()
            .copied()
            .zip(self.info.parameter_types.iter())
            .enumerate()
        {
            let ty = value_type.ir.clone();
            params.push(FunctionParam {
                name: format!("arg{index}"),
                ty: ty.clone(),
            });
            let value = self.fresh_value(ty);
            let key = if self.is_aggregate_type(value_type.source) {
                EnvKey::Address(symbol)
            } else {
                EnvKey::Local(symbol)
            };
            entry_args.push((
                key,
                BlockArg {
                    name: format!("param{index}"),
                    value: value.clone(),
                },
            ));
            env.insert(
                key,
                BoundValue {
                    value,
                    integer: (!self.is_aggregate_type(value_type.source))
                        .then_some(value_type.integer),
                },
            );
        }
        if self
            .info
            .return_type
            .as_ref()
            .is_some_and(|returned| self.is_aggregate_type(returned.source))
        {
            let index = params.len();
            let ty = EvoIRType::Ptr(AddressSpace::Generic);
            params.push(FunctionParam {
                name: format!("arg{index}"),
                ty: ty.clone(),
            });
            let value = self.fresh_value(ty);
            entry_args.push((
                EnvKey::ReturnDestination,
                BlockArg {
                    name: "return_destination".into(),
                    value: value.clone(),
                },
            ));
            env.insert(
                EnvKey::ReturnDestination,
                BoundValue {
                    value,
                    integer: None,
                },
            );
        }
        if self.info.participant {
            let mem = EvoIRType::Mem(Region::All);
            let world = EvoIRType::World;
            let mem_index = params.len();
            params.push(FunctionParam {
                name: format!("arg{mem_index}"),
                ty: mem.clone(),
            });
            params.push(FunctionParam {
                name: format!("arg{}", mem_index + 1),
                ty: world.clone(),
            });
            let mem_value = self.fresh_value(mem);
            let world_value = self.fresh_value(world);
            entry_args.push((
                EnvKey::Memory,
                BlockArg {
                    name: "memory".into(),
                    value: mem_value.clone(),
                },
            ));
            entry_args.push((
                EnvKey::World,
                BlockArg {
                    name: "world".into(),
                    value: world_value.clone(),
                },
            ));
            env.insert(
                EnvKey::Memory,
                BoundValue {
                    value: mem_value,
                    integer: None,
                },
            );
            env.insert(
                EnvKey::World,
                BoundValue {
                    value: world_value,
                    integer: None,
                },
            );
        }
        self.blocks.push(DraftBlock {
            id: 0,
            args: entry_args,
            insts: Vec::new(),
            terminator: None,
            role: BlockRole::Ordinary,
        });
        self.next_block = 1;

        let locals = self.info.parameter_symbols.iter().copied().collect();
        let flow = self.lower_stmt(
            body,
            State {
                block: 0,
                env,
                locals,
            },
            &BTreeSet::new(),
            &ControlTargets::default(),
        )?;
        if !self.pending_gotos.is_empty() {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "goto targets a label that was not lowered",
            });
        }
        for state in &flow.normal {
            if self.info.return_type.is_none() || self.info.name == "main" {
                let mut returned = Vec::new();
                if let Some(value_type) = self.info.return_type.as_ref() {
                    if matches!(value_type.ir, EvoIRType::Ptr(_)) {
                        let value = self.fresh_value(value_type.ir.clone());
                        self.emit(state, Inst::PtrNull(value.clone()))?;
                        returned.push(value);
                    } else {
                        returned.push(self.constant(state, value_type.integer, 0)?.value);
                    }
                }
                if self.info.participant {
                    returned.push(self.token(state, EnvKey::Memory)?.value);
                    returned.push(self.token(state, EnvKey::World)?.value);
                }
                self.terminate(state, Inst::Ret(returned))?;
            } else {
                self.terminate(state, Inst::Trap(TrapReason::ScalarFunctionFellOffEnd))?;
            }
        }
        self.prune_unreachable_blocks()?;
        if self.blocks.iter().any(|block| block.terminator.is_none()) {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "lowered block has no terminator",
            });
        }
        self.eliminate_switch_forwarders()?;
        let mut function = Function {
            name: self.info.name.clone(),
            variadic: self.info.variadic,
            params,
            ret_tys: self.return_types(),
            effects: Default::default(),
            blocks: self.finish_blocks()?,
        };
        renumber_function(&mut function)?;
        Ok(function)
    }

    fn return_types(&self) -> Vec<EvoIRType> {
        let mut returns = self
            .info
            .return_type
            .as_ref()
            .filter(|value_type| !self.is_aggregate_type(value_type.source))
            .map(|value_type| vec![value_type.ir.clone()])
            .unwrap_or_default();
        if self.info.participant {
            returns.push(EvoIRType::Mem(Region::All));
            returns.push(EvoIRType::World);
        }
        returns
    }

    fn fresh_value(&mut self, ty: EvoIRType) -> Value {
        let value = Value {
            id: self.next_value,
            ty,
        };
        self.next_value += 1;
        value
    }

    fn fresh_placeholder(&mut self) -> BlockId {
        let id = self.next_placeholder;
        self.next_placeholder += 1;
        id
    }

    fn prune_unreachable_blocks(&mut self) -> Result<(), LowerError> {
        let mut reachable = BTreeSet::from([0]);
        let mut pending = vec![0];
        while let Some(id) = pending.pop() {
            let block = self.blocks.iter().find(|block| block.id == id).ok_or(
                LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "reachable edge targets an unknown draft block",
                },
            )?;
            let Some(terminator) = &block.terminator else {
                continue;
            };
            let targets = match terminator {
                Inst::AsmGoto {
                    labels,
                    fallthrough,
                    ..
                } => labels
                    .iter()
                    .map(|label| label.edge.target)
                    .chain([fallthrough.target])
                    .collect(),
                Inst::Br(target, _) => vec![*target],
                Inst::CondBr(_, true_target, _, false_target, _) => {
                    vec![*true_target, *false_target]
                }
                _ => vec![],
            };
            for target in targets {
                if reachable.insert(target) {
                    pending.push(target);
                }
            }
        }
        self.blocks.retain(|block| reachable.contains(&block.id));
        Ok(())
    }

    fn block_mut(&mut self, id: BlockId) -> Result<&mut DraftBlock, LowerError> {
        self.blocks
            .iter_mut()
            .find(|block| block.id == id)
            .ok_or(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "unknown draft block",
            })
    }

    fn emit(&mut self, state: &State, instruction: Inst) -> Result<(), LowerError> {
        let block = self.block_mut(state.block)?;
        if block.terminator.is_some() {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "instruction emitted after a terminator",
            });
        }
        block.insts.push(instruction);
        Ok(())
    }

    fn terminate(&mut self, state: &State, terminator: Inst) -> Result<(), LowerError> {
        let block = self.block_mut(state.block)?;
        if block.terminator.replace(terminator).is_some() {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "block received more than one terminator",
            });
        }
        Ok(())
    }

    fn branch(
        &mut self,
        state: &State,
        target: BlockId,
        keys: &[EnvKey],
    ) -> Result<(), LowerError> {
        let arguments = self.edge_values(state, keys)?;
        self.terminate(state, Inst::Br(target, arguments))
    }

    fn keys_for(
        &self,
        state: &State,
        symbols: &BTreeSet<SymbolId>,
        preferred: &[SymbolId],
    ) -> Vec<EnvKey> {
        let mut keys = Vec::new();
        let mut seen = BTreeSet::new();
        for symbol in preferred {
            let key = if self.memory_symbols.contains(symbol) {
                EnvKey::Address(*symbol)
            } else {
                EnvKey::Local(*symbol)
            };
            if symbols.contains(symbol) && state.locals.contains(symbol) && seen.insert(key) {
                keys.push(key);
            }
        }
        let mut locals = state
            .env
            .iter()
            .filter_map(|(key, bound)| match key {
                EnvKey::Local(symbol) | EnvKey::Address(symbol) => {
                    Some((bound.value.id, *symbol, *key))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        locals.sort_by_key(|(value, _, _)| *value);
        for (_, symbol, key) in locals {
            if (symbols.contains(&symbol) || matches!(key, EnvKey::Address(_))) && seen.insert(key)
            {
                keys.push(key);
            }
        }
        for symbol in symbols {
            let key = if self.memory_symbols.contains(symbol) {
                EnvKey::Address(*symbol)
            } else {
                EnvKey::Local(*symbol)
            };
            if seen.contains(&key) {
                continue;
            }
            let is_scalar_local = state.locals.contains(symbol)
                && self.unit.symbols().symbol(*symbol).is_ok_and(|symbol| {
                    matches!(symbol.kind(), SymbolKind::Object | SymbolKind::Parameter)
                });
            if is_scalar_local && seen.insert(key) {
                keys.push(key);
            }
        }
        let mut temporaries = state
            .env
            .iter()
            .filter_map(|(key, bound)| match key {
                EnvKey::Temporary(_) => Some((bound.value.id, *key)),
                _ => None,
            })
            .collect::<Vec<_>>();
        temporaries.sort_by_key(|(value, _)| *value);
        for (_, key) in temporaries {
            if seen.insert(key) {
                keys.push(key);
            }
        }
        if state.env.contains_key(&EnvKey::ReturnDestination) {
            keys.push(EnvKey::ReturnDestination);
        }
        if self.info.participant {
            if state.env.contains_key(&EnvKey::Memory) {
                keys.push(EnvKey::Memory);
            }
            if state.env.contains_key(&EnvKey::World) {
                keys.push(EnvKey::World);
            }
        }
        keys
    }

    fn new_block_from(
        &mut self,
        template: &State,
        keys: &[EnvKey],
        role: BlockRole,
    ) -> Result<State, LowerError> {
        let id = self.next_block;
        self.next_block += 1;
        let mut args = Vec::new();
        let mut env = Environment::new();
        for (index, key) in keys.iter().copied().enumerate() {
            let (ty, integer) = if let Some(bound) = template.env.get(&key) {
                (bound.value.ty.clone(), bound.integer)
            } else if let EnvKey::Local(symbol) = key {
                let declaration =
                    self.unit
                        .symbols()
                        .symbol(symbol)
                        .map_err(|_| LowerError::UnknownSymbol {
                            symbol,
                            span: self.info.span,
                        })?;
                let (ty, integer) = self.c_value_type(declaration.ty(), self.info.span)?;
                (ty, Some(integer))
            } else if let EnvKey::Address(_) = key {
                (EvoIRType::Ptr(AddressSpace::Generic), None)
            } else {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "effect or temporary edge key is absent from its template environment",
                });
            };
            let value = self.fresh_value(ty);
            args.push((
                key,
                BlockArg {
                    name: format!("phi{index}"),
                    value: value.clone(),
                },
            ));
            env.insert(key, BoundValue { value, integer });
        }
        self.blocks.push(DraftBlock {
            id,
            args,
            insts: Vec::new(),
            terminator: None,
            role,
        });
        Ok(State {
            block: id,
            env,
            locals: template.locals.clone(),
        })
    }

    fn edge_values(&mut self, state: &State, keys: &[EnvKey]) -> Result<Vec<Value>, LowerError> {
        let mut values = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(bound) = state.env.get(key) {
                values.push(bound.value.clone());
                continue;
            }
            let EnvKey::Local(symbol) = key else {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "effect or temporary edge uses an unavailable environment value",
                });
            };
            let declaration =
                self.unit
                    .symbols()
                    .symbol(*symbol)
                    .map_err(|_| LowerError::UnknownSymbol {
                        symbol: *symbol,
                        span: self.info.span,
                    })?;
            let (ty, _) = self.c_value_type(declaration.ty(), self.info.span)?;
            let value = self.fresh_value(ty);
            self.emit(state, Inst::Uninitialized(value.clone()))?;
            values.push(value);
        }
        Ok(values)
    }

    fn merge_states(
        &mut self,
        states: Vec<State>,
        live: &BTreeSet<SymbolId>,
    ) -> Result<Option<State>, LowerError> {
        let mut states = states.into_iter();
        let Some(first) = states.next() else {
            return Ok(None);
        };
        let rest = states.collect::<Vec<_>>();
        if rest.is_empty() {
            return Ok(Some(first));
        }
        let keys = self.keys_for(&first, live, &[]);
        let target = self.new_block_from(&first, &keys, BlockRole::Ordinary)?;
        self.branch(&first, target.block, &keys)?;
        for state in rest {
            self.branch(&state, target.block, &keys)?;
        }
        Ok(Some(target))
    }

    fn retain_temporary(&mut self, value: Value, state: &mut State) -> EnvKey {
        let key = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        state.env.insert(
            key,
            BoundValue {
                value,
                integer: None,
            },
        );
        key
    }

    fn lower_assembly(
        &mut self,
        source: &'a AsmStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
    ) -> Result<Flow, LowerError> {
        let refuse = |reason: String| LowerError::AsmOperand {
            span: source.span,
            reason,
        };
        let mut outputs = Vec::new();
        let mut inputs = Vec::new();
        let mut result_types = Vec::new();
        let mut writebacks = Vec::new();
        let mut temporary_keys = Vec::new();
        let mut incoming_keys = Vec::new();
        let mut input_keys = Vec::new();
        for operand in &source.outputs {
            if !matches!(
                operand.constraint.as_str(),
                "=r" | "+r" | "=&r" | "+&r" | "=Q" | "+Q"
            ) {
                return Err(refuse(operand.constraint.clone()));
            }
            let address = self
                .lower_unary(
                    UnaryOp::AddrOf,
                    operand.expression,
                    operand.span,
                    &mut state,
                )?
                .value;
            let incoming = if evocc_evoir::assembly_memory_constraint(&operand.constraint) {
                Some(address.clone())
            } else {
                let ty = match operand.expression {
                    CHirExpr::Ident { facts, .. } | CHirExpr::MemoryRead { facts, .. } => {
                        facts.ty()
                    }
                    CHirExpr::Unary(UnaryOp::Deref, pointer, span) => {
                        self.pointer_pointee_type(pointer, *span)?
                    }
                    _ => return Err(refuse(format!("{} output place", operand.constraint))),
                };
                let (ir, integer) = self.c_value_type(ty, operand.span)?;
                let volatile = self
                    .unit
                    .types()
                    .get(ty)
                    .is_some_and(|ty| ty.qualifiers.is_volatile);
                let address_key = self.retain_temporary(address.clone(), &mut state);
                temporary_keys.push(address_key);
                result_types.push(ir);
                writebacks.push((address_key, ty, integer, volatile));
                if operand.constraint.starts_with('+') {
                    Some(
                        self.emit_memory_load(address, ty, volatile, operand.span, &mut state)?
                            .value,
                    )
                } else {
                    None
                }
            };
            let ty = if evocc_evoir::assembly_memory_constraint(&operand.constraint) {
                incoming.as_ref().expect("memory address").ty.clone()
            } else {
                result_types.last().expect("register output").clone()
            };
            let key = incoming
                .as_ref()
                .map(|value| self.retain_temporary(value.clone(), &mut state));
            temporary_keys.extend(key);
            incoming_keys.push(key);
            outputs.push(AssemblyOutput {
                constraint: operand.constraint.clone(),
                name: operand.symbolic.clone().unwrap_or_default(),
                ty,
                incoming,
            });
        }
        for operand in &source.inputs {
            let immediate = matches!(
                operand.constraint.as_str(),
                "I" | "J" | "K" | "L" | "i" | "n"
            );
            if immediate
                && !evocc_chir::builtin_constant_p(
                    operand.expression,
                    self.unit.types(),
                    self.unit.symbols(),
                    self.target,
                )
            {
                return Err(refuse(operand.constraint.clone()));
            }
            let value = if immediate {
                let integer = evocc_chir::interp::interpret_static_integer_expression(
                    self.unit,
                    operand.expression,
                    self.target,
                )
                .map_err(|_| refuse(operand.constraint.clone()))?;
                if integer.ty().bits() > 64 {
                    return Err(LowerError::WideInteger { span: operand.span });
                }
                self.constant(&state, integer.ty(), integer.raw_bits() as u64)?
                    .value
            } else if evocc_evoir::assembly_memory_constraint(&operand.constraint) {
                self.lower_unary(
                    UnaryOp::AddrOf,
                    operand.expression,
                    operand.span,
                    &mut state,
                )?
                .value
            } else {
                self.lower_expr(operand.expression, &mut state)?.value
            };
            let key = self.retain_temporary(value.clone(), &mut state);
            temporary_keys.push(key);
            input_keys.push(key);
            inputs.push(AssemblyInput {
                constraint: operand.constraint.clone(),
                name: operand.symbolic.clone().unwrap_or_default(),
                value,
            });
        }
        for (output, key) in outputs.iter_mut().zip(incoming_keys) {
            if let Some(key) = key {
                output.incoming = Some(self.token(&state, key)?.value);
            }
        }
        for (input, key) in inputs.iter_mut().zip(input_keys) {
            input.value = self.token(&state, key)?.value;
        }
        let results = result_types
            .into_iter()
            .map(|ty| self.fresh_value(ty))
            .collect::<Vec<_>>();
        let assembly = InlineAssembly {
            template: source.template.clone(),
            volatile: source.volatile_span.is_some(),
            outputs,
            inputs,
            clobbers: source.clobbers.clone(),
            results,
            mem_in: self.token(&state, EnvKey::Memory)?.value,
            world_in: self.token(&state, EnvKey::World)?.value,
            mem_out: self.fresh_value(EvoIRType::Mem(Region::All)),
            world_out: self.fresh_value(EvoIRType::World),
        };
        let labels = source
            .labels
            .iter()
            .map(|label| AssemblyLabel {
                name: label.spelling.clone(),
                edge: AssemblyEdge {
                    target: 0,
                    arguments: vec![],
                },
            })
            .collect::<Vec<_>>();
        assembly.validate(&labels).map_err(refuse)?;
        self.replace_effect_tokens(
            &mut state,
            assembly.mem_out.clone(),
            Some(assembly.world_out.clone()),
        );
        let mut result_keys = Vec::new();
        for result in &assembly.results {
            let key = self.retain_temporary(result.clone(), &mut state);
            temporary_keys.push(key);
            result_keys.push(key);
        }
        let mut successors = Vec::new();
        if source.goto_span.is_some() {
            let keys = self.keys_for(&state, &state.locals, &[]);
            let mut labels = Vec::new();
            for label in &source.labels {
                let successor = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
                labels.push(AssemblyLabel {
                    name: label.spelling.clone(),
                    edge: AssemblyEdge {
                        target: successor.block,
                        arguments: self.edge_values(&state, &keys)?,
                    },
                });
                successors.push((Some(label.spelling.clone()), successor));
            }
            let fallthrough = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
            let edge = AssemblyEdge {
                target: fallthrough.block,
                arguments: self.edge_values(&state, &keys)?,
            };
            self.terminate(
                &state,
                Inst::AsmGoto {
                    assembly,
                    labels,
                    fallthrough: edge,
                },
            )?;
            successors.push((None, fallthrough));
        } else {
            self.emit(&state, Inst::Asm(assembly))?;
            successors.push((None, state));
        }
        let mut normal = Vec::new();
        for (label, mut successor) in successors {
            for ((address, ty, integer, volatile), result) in writebacks.iter().zip(&result_keys) {
                let address = self.token(&successor, *address)?.value;
                let value = ExprValue {
                    value: self.token(&successor, *result)?.value,
                    integer: *integer,
                };
                self.emit_memory_store(
                    address,
                    value,
                    *ty,
                    *volatile,
                    source.span,
                    &mut successor,
                )?;
            }
            for key in &temporary_keys {
                successor.env.remove(key);
            }
            if let Some(label) = label {
                if let Some(target) = self.labels.get(&label).cloned() {
                    self.branch(&successor, target.id, &target.keys)?;
                } else {
                    self.pending_gotos.entry(label).or_default().push(successor);
                }
            } else {
                normal.push(successor);
            }
        }
        let _ = live_after;
        Ok(Flow { normal })
    }

    fn lower_stmt(
        &mut self,
        statement: &'a CHirStmt<'a>,
        state: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let previous = std::mem::replace(&mut self.active_control, control.clone());
        let result = self.lower_stmt_active(statement, state, live_after, control);
        self.active_control = previous;
        result
    }

    fn lower_stmt_active(
        &mut self,
        statement: &'a CHirStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        match statement {
            CHirStmt::Asm(assembly) => self.lower_assembly(assembly, state, live_after),
            CHirStmt::Expr(CHirExpr::BuiltinUnreachable { .. }, _) => {
                self.terminate(&state, Inst::Trap(TrapReason::UnreachableReached))?;
                Ok(Flow::default())
            }
            CHirStmt::Expr(expression, _) => {
                self.lower_discarded_expr(expression, &mut state)?;
                Ok(Flow {
                    normal: vec![state],
                })
            }
            CHirStmt::Return(expression, span) => {
                let mut returned = Vec::new();
                match (self.info.return_type.as_ref(), expression) {
                    (Some(expected), Some(expression)) => {
                        if self.is_aggregate_type(expected.source) {
                            let source = self.lower_expr(expression, &mut state)?;
                            let destination = self.token(&state, EnvKey::ReturnDestination)?.value;
                            self.emit_aggregate_transfer(
                                destination,
                                source.value,
                                expected.source,
                                self.aggregate_source_is_volatile(expression),
                                *span,
                                &mut state,
                            )?;
                        } else {
                            let lowered = self.lower_expr(expression, &mut state)?;
                            let lowered = if matches!(
                                expected.ir,
                                EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
                            ) {
                                self.coerce_to_object_type(
                                    lowered,
                                    Some(expression),
                                    expected.source,
                                    *span,
                                    &state,
                                )?
                            } else {
                                self.cast(lowered, expected.integer, &state)?
                            };
                            returned.push(lowered.value);
                        }
                    }
                    (None, None) => {}
                    _ => {
                        return Err(LowerError::MalformedFunction {
                            function: self.info.name.clone(),
                            span: *span,
                            reason: "return expression disagrees with the function result type",
                        });
                    }
                }
                if self.info.participant {
                    returned.push(self.token(&state, EnvKey::Memory)?.value);
                    returned.push(self.token(&state, EnvKey::World)?.value);
                }
                self.terminate(&state, Inst::Ret(returned))?;
                Ok(Flow::default())
            }
            CHirStmt::Compound(items, _) => self.lower_compound(items, state, live_after, control),
            CHirStmt::If(condition, then_branch, else_branch, _) => self.lower_if(
                condition,
                then_branch,
                *else_branch,
                state,
                live_after,
                control,
            ),
            CHirStmt::While(condition, body, _) => {
                self.lower_while(condition, body, state, live_after, control)
            }
            CHirStmt::For(initializer, condition, step, body, _) => self.lower_for(
                initializer,
                *condition,
                *step,
                body,
                state,
                live_after,
                control,
            ),
            CHirStmt::Switch(expression, body, _) => {
                self.lower_switch(expression, body, state, live_after, control)
            }
            CHirStmt::Break(span) => {
                let target =
                    control
                        .break_target
                        .as_ref()
                        .ok_or(LowerError::InvalidControlFlow {
                            span: *span,
                            reason: "break has no enclosing target",
                        })?;
                self.branch(&state, target.id, &target.keys)?;
                Ok(Flow::default())
            }
            CHirStmt::Continue(span) => {
                let target =
                    control
                        .continue_target
                        .as_ref()
                        .ok_or(LowerError::InvalidControlFlow {
                            span: *span,
                            reason: "continue has no enclosing loop",
                        })?;
                self.branch(&state, target.id, &target.keys)?;
                Ok(Flow::default())
            }
            CHirStmt::AttributedNull(_, _) | CHirStmt::Null(_) => Ok(Flow {
                normal: vec![state],
            }),
            CHirStmt::DoWhile(body, condition, _) => {
                self.lower_do_while(body, condition, state, live_after, control)
            }
            CHirStmt::Goto(label, _) => {
                if let Some(target) = self.labels.get(&label.spelling).cloned() {
                    self.branch(&state, target.id, &target.keys)?;
                } else {
                    self.pending_gotos
                        .entry(label.spelling.clone())
                        .or_default()
                        .push(state);
                }
                Ok(Flow::default())
            }
            CHirStmt::ComputedGoto(_, span) => Err(LowerError::ComputedGoto { span: *span }),
            CHirStmt::Label(label, nested, _) => self.lower_label(
                label.spelling.as_str(),
                nested,
                vec![state],
                live_after,
                control,
            ),
            CHirStmt::Case(_, _, span)
            | CHirStmt::CaseRange(_, _, _, span)
            | CHirStmt::Default(_, span) => Err(LowerError::UnsupportedStatement {
                span: *span,
                feature: "unstructured label outside a switch lowering context",
            }),
        }
    }

    fn lower_compound(
        &mut self,
        items: &'a [CHirBlockItem<'a>],
        initial: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        self.lower_compound_items(items, initial, live_after, control, true)
    }

    fn lower_compound_items(
        &mut self,
        items: &'a [CHirBlockItem<'a>],
        mut initial: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
        leave_scope: bool,
    ) -> Result<Flow, LowerError> {
        for item in items {
            if let CHirBlockItem::Declaration(CHirDecl::Object(object)) = item {
                initial.locals.insert(object.symbol);
            }
        }
        let mut normal = vec![initial];
        let mut last_template = normal.first().cloned();
        let mut declared = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let mut future = live_after.clone();
            for later in &items[index + 1..] {
                collect_symbols_item(later, &mut future);
            }
            let mut needed_now = future.clone();
            collect_symbols_item(item, &mut needed_now);
            if let CHirBlockItem::Statement(CHirStmt::Label(label, nested, _)) = item {
                let incoming = std::mem::take(&mut normal);
                let template = incoming.first().cloned().or_else(|| last_template.clone());
                normal = self
                    .lower_label_with_template(
                        label.spelling.as_str(),
                        nested,
                        incoming,
                        template,
                        &future,
                        control,
                    )?
                    .normal;
                if let Some(state) = normal.first().cloned() {
                    last_template = Some(state);
                }
                continue;
            }
            if normal.is_empty() {
                let CHirBlockItem::Statement(statement) = item else {
                    continue;
                };
                if !contains_targeted_label(statement, &self.goto_names) {
                    continue;
                }
                let template = last_template
                    .clone()
                    .ok_or(LowerError::InvalidControlFlow {
                        span: statement.span(),
                        reason: "targeted nested label has no environment template",
                    })?;
                let keys = self.keys_for(&template, &needed_now, &[]);
                let detached = self.new_block_from(&template, &keys, BlockRole::Ordinary)?;
                normal = self
                    .lower_stmt(statement, detached, &future, control)?
                    .normal;
                if let Some(state) = normal.first().cloned() {
                    last_template = Some(state);
                }
                continue;
            }
            let pending = std::mem::take(&mut normal);
            let Some(mut state) = self.merge_states(pending, &needed_now)? else {
                continue;
            };
            last_template = Some(state.clone());
            normal = match item {
                CHirBlockItem::Declaration(declaration) => {
                    if let CHirDecl::Object(object) = declaration {
                        declared.push(object.symbol);
                    }
                    self.lower_declaration(declaration, &mut state)?;
                    vec![state]
                }
                CHirBlockItem::Statement(statement) => {
                    self.lower_stmt(statement, state, &future, control)?.normal
                }
            };
        }
        if leave_scope {
            for state in &mut normal {
                for symbol in declared.iter().rev() {
                    if self.memory_symbols.contains(symbol)
                        && state.env.contains_key(&EnvKey::Address(*symbol))
                        && self
                            .unit
                            .symbols()
                            .symbol(*symbol)
                            .is_ok_and(|entry| entry.storage_class() != Some(StorageClass::Static))
                    {
                        self.end_object_lifetime(*symbol, state)?;
                    }
                    state.env.remove(&EnvKey::Local(*symbol));
                    state.env.remove(&EnvKey::Address(*symbol));
                    state.locals.remove(symbol);
                }
            }
        }
        Ok(Flow { normal })
    }

    fn lower_label(
        &mut self,
        name: &str,
        nested: &'a CHirStmt<'a>,
        incoming: Vec<State>,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let template = incoming.first().cloned();
        self.lower_label_with_template(name, nested, incoming, template, live_after, control)
    }

    fn lower_label_with_template(
        &mut self,
        name: &str,
        nested: &'a CHirStmt<'a>,
        mut incoming: Vec<State>,
        template: Option<State>,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        if self.labels.contains_key(name) {
            return Err(LowerError::InvalidControlFlow {
                span: nested.span(),
                reason: "duplicate label reached lowering",
            });
        }
        incoming.extend(self.pending_gotos.remove(name).unwrap_or_default());
        let Some(template) = incoming.first().cloned().or(template) else {
            if self.goto_names.contains(name) {
                return Err(LowerError::InvalidControlFlow {
                    span: nested.span(),
                    reason: "targeted label has no environment template",
                });
            }
            return Ok(Flow::default());
        };
        let symbols = live_before_stmt(nested, live_after.clone());
        let keys = self.keys_for(&template, &symbols, &[]);
        let label_state = self.new_block_from(&template, &keys, BlockRole::Ordinary)?;
        let target = JumpTarget {
            id: label_state.block,
            keys: keys.clone(),
        };
        self.labels.insert(name.to_owned(), target);
        for predecessor in incoming {
            self.branch(&predecessor, label_state.block, &keys)?;
        }
        self.lower_stmt(nested, label_state, live_after, control)
    }

    fn lower_declaration(
        &mut self,
        declaration: &'a CHirDecl<'a>,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let object = match declaration {
            CHirDecl::Object(object) => object,
            CHirDecl::Asm(assembly) => {
                return Err(LowerError::Asm {
                    span: assembly.span,
                });
            }
            CHirDecl::Tag(_) | CHirDecl::Typedef(_) => return Ok(()),
            CHirDecl::Function(_) => {
                return Err(LowerError::UnsupportedStatement {
                    span: declaration.span(),
                    feature: "block function declaration is outside scalar lowering",
                });
            }
        };
        validate_object_declaration_for_lowering(self.unit, object)?;
        if self.memory_symbols.contains(&object.symbol) {
            return self.lower_memory_declaration(object, state);
        }
        let (_, integer) = self.c_value_type(object.ty, object.span)?;
        state.locals.insert(object.symbol);
        let Some(initializer) = &object.initializer else {
            state.env.remove(&EnvKey::Local(object.symbol));
            return Ok(());
        };
        let stored = match initializer.plan.actions() {
            [InitAction::StoreScalar { value, .. }] => {
                let lowered = self.lower_expr(value, state)?;
                self.coerce_to_object_type(lowered, Some(value), object.ty, object.span, state)?
            }
            [InitAction::ZeroScalar { .. }] => {
                if matches!(
                    self.unit.types().get(object.ty).map(|ty| &ty.kind),
                    Some(TypeKind::Pointer(_))
                ) {
                    let pointer = self.fresh_value(self.c_value_type(object.ty, object.span)?.0);
                    self.emit(state, Inst::PtrNull(pointer.clone()))?;
                    ExprValue {
                        value: pointer,
                        integer,
                    }
                } else {
                    self.constant(state, integer, 0)?
                }
            }
            _ => {
                return Err(LowerError::UnsupportedStatement {
                    span: object.span,
                    feature: "aggregate or multi-action initializer is outside scalar lowering",
                });
            }
        };
        state.env.insert(
            EnvKey::Local(object.symbol),
            BoundValue {
                value: stored.value,
                integer: Some(integer),
            },
        );
        Ok(())
    }

    fn c_value_type(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<(EvoIRType, CIntegerType), LowerError> {
        match self.unit.types().get(ty).map(|canonical| &canonical.kind) {
            Some(TypeKind::Pointer(pointee))
                if matches!(
                    self.unit.types().get(*pointee).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) =>
            {
                Ok((self.fnptr_type(*pointee, span)?, CIntegerType::ULONG))
            }
            Some(TypeKind::Pointer(_)) => {
                Ok((EvoIRType::Ptr(AddressSpace::Generic), CIntegerType::ULONG))
            }
            Some(TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_)) => {
                let integer = canonical_integer(self.unit.types(), self.target, ty, span)?;
                Ok((bits(integer), integer))
            }
            _ => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "object subvalue is neither an integer nor pointer",
            }),
        }
    }

    fn fnptr_type(&self, function_ty: TypeId, span: SourceSpan) -> Result<EvoIRType, LowerError> {
        function_pointer_ir_type(self.unit.types(), self.target, function_ty, span)
    }

    fn call_designator_function_type(&self, callee: &CHirExpr<'_>) -> Option<TypeId> {
        let candidate = match callee {
            CHirExpr::Ident { facts, .. }
            | CHirExpr::MemoryRead { facts, .. }
            | CHirExpr::Assign { facts, .. }
            | CHirExpr::MemoryAssign { facts, .. }
            | CHirExpr::Update { facts, .. }
            | CHirExpr::MemoryUpdate { facts, .. } => facts.ty(),
            CHirExpr::Cast(ty, _, _) => *ty,
            _ => return None,
        };
        match self.unit.types().get(candidate).map(|ty| &ty.kind) {
            Some(TypeKind::Function { .. }) => Some(candidate),
            Some(TypeKind::Pointer(pointee))
                if matches!(
                    self.unit.types().get(*pointee).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) =>
            {
                Some(*pointee)
            }
            _ => None,
        }
    }

    fn scalar_call_signature(
        &self,
        function_ty: TypeId,
        span: SourceSpan,
    ) -> Result<(Vec<FunctionValueType>, Option<FunctionValueType>), LowerError> {
        let TypeKind::Function {
            return_type,
            parameters,
        } = &self
            .unit
            .types()
            .get(function_ty)
            .ok_or(LowerError::UnknownType {
                ty: function_ty,
                span,
            })?
            .kind
        else {
            return Err(LowerError::UnsupportedType {
                ty: function_ty,
                span,
                feature: "call designator does not name a function type",
            });
        };
        let parameter_types = match parameters {
            FunctionParameters::Prototype { types, .. } => types
                .iter()
                .map(|ty| function_value_type(self.unit.types(), self.target, *ty, span))
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(LowerError::UnsupportedType {
                    ty: function_ty,
                    span,
                    feature: "call requires a prototype",
                });
            }
        };
        let return_type = match self.unit.types().get(*return_type).map(|ty| &ty.kind) {
            Some(TypeKind::Void) => None,
            Some(_) => Some(function_value_type(
                self.unit.types(),
                self.target,
                *return_type,
                span,
            )?),
            None => {
                return Err(LowerError::UnknownType {
                    ty: *return_type,
                    span,
                });
            }
        };
        Ok((parameter_types, return_type))
    }

    fn access_layout(&self, ty: TypeId, span: SourceSpan) -> Result<AccessLayout, LowerError> {
        let layout = self
            .target
            .layout_of(self.unit.types(), ty)
            .map_err(|source| LowerError::Layout { ty, span, source })?;
        Ok(AccessLayout::new(layout.size_bytes, layout.align_bytes))
    }

    fn pointer_arithmetic_scale(
        &self,
        pointee: TypeId,
        span: SourceSpan,
    ) -> Result<u64, LowerError> {
        if matches!(
            self.unit
                .types()
                .get(pointee)
                .map(|canonical| &canonical.kind),
            Some(TypeKind::Void)
        ) {
            return Ok(1);
        }
        Ok(self.access_layout(pointee, span)?.size_bytes)
    }

    fn is_aggregate_type(&self, ty: TypeId) -> bool {
        is_aggregate_type(self.unit.types(), ty)
    }

    fn is_function_pointer_type(&self, ty: TypeId) -> bool {
        let Some(TypeKind::Pointer(pointee)) =
            self.unit.types().get(ty).map(|canonical| &canonical.kind)
        else {
            return false;
        };
        matches!(
            self.unit
                .types()
                .get(*pointee)
                .map(|canonical| &canonical.kind),
            Some(TypeKind::Function { .. })
        )
    }

    fn synthetic_origin(&mut self) -> Result<u64, LowerError> {
        let local = self.next_synthetic_origin;
        self.next_synthetic_origin =
            self.next_synthetic_origin
                .checked_add(1)
                .ok_or(LowerError::InvalidGeneratedSsa {
                    message: "aggregate temporary origin counter overflow".to_owned(),
                })?;
        Ok((1_u64 << 63) | (u64::from(self.info.ordinal) << 32) | u64::from(local))
    }

    fn emit_alloca(
        &mut self,
        ty: TypeId,
        span: SourceSpan,
        origin: u64,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let layout = self.access_layout(ty, span)?;
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Alloca {
                result: address.clone(),
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                origin,
                layout,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(address)
    }

    fn emit_copy(
        &mut self,
        destination: Value,
        source: Value,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        if !matches!(destination.ty, EvoIRType::Ptr(_)) || !matches!(source.ty, EvoIRType::Ptr(_)) {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "aggregate copy operands are not object addresses",
            });
        }
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        let layout = self.access_layout(ty, span)?;
        self.emit(
            state,
            Inst::Copy {
                destination,
                source,
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                layout,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(())
    }

    fn aggregate_source_is_volatile(&self, expression: &CHirExpr<'_>) -> bool {
        match expression {
            CHirExpr::MemoryRead { place, .. } => place.access_qualifiers().is_volatile,
            CHirExpr::Ident { facts, .. }
            | CHirExpr::MemoryAssign { facts, .. }
            | CHirExpr::StatementExpr { facts, .. } => self
                .unit
                .types()
                .get(facts.ty())
                .is_some_and(|ty| ty.qualifiers.is_volatile),
            CHirExpr::CompoundLit(ty, _, _) => self
                .unit
                .types()
                .get(*ty)
                .is_some_and(|ty| ty.qualifiers.is_volatile),
            _ => false,
        }
    }

    fn emit_aggregate_transfer(
        &mut self,
        destination: Value,
        source: Value,
        ty: TypeId,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        if !volatile {
            return self.emit_copy(destination, source, ty, span, state);
        }
        let kind = self
            .unit
            .types()
            .get(ty)
            .ok_or(LowerError::UnknownType { ty, span })?
            .kind
            .clone();
        match kind {
            TypeKind::Struct(tag) => {
                let fields = match &self
                    .unit
                    .types()
                    .tag(tag)
                    .ok_or(LowerError::UnsupportedType {
                        ty,
                        span,
                        feature: "volatile aggregate has an unknown record tag",
                    })?
                    .state
                {
                    TagState::Complete(TagDefinition::Struct(fields)) => fields.clone(),
                    _ => {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "volatile aggregate is not a complete struct",
                        });
                    }
                };
                let layout = self
                    .target
                    .record_layout(self.unit.types(), tag)
                    .map_err(|source| LowerError::Layout { ty, span, source })?;
                for (field_index, field) in fields.into_iter().enumerate() {
                    let Some(field_layout) = layout
                        .fields
                        .iter()
                        .find(|layout| layout.field_index == field_index)
                    else {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "volatile member has no target layout",
                        });
                    };
                    if field.bit_field.is_some() {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "volatile aggregate bit-field copy is not specified",
                        });
                    }
                    let destination = self.constant_offset_address(
                        destination.clone(),
                        field_layout.byte_offset,
                        state,
                    )?;
                    let source = self.constant_offset_address(
                        source.clone(),
                        field_layout.byte_offset,
                        state,
                    )?;
                    self.emit_aggregate_transfer(destination, source, field.ty, true, span, state)?;
                }
                Ok(())
            }
            TypeKind::Union(_) => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "volatile union copy has no selected-member authority",
            }),
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(count),
            } => {
                let element_layout = self.access_layout(element, span)?;
                for index in 0..count {
                    let offset = index.checked_mul(element_layout.size_bytes).ok_or(
                        LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "volatile array member offset overflow",
                        },
                    )?;
                    let destination =
                        self.constant_offset_address(destination.clone(), offset, state)?;
                    let source = self.constant_offset_address(source.clone(), offset, state)?;
                    self.emit_aggregate_transfer(destination, source, element, true, span, state)?;
                }
                Ok(())
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Ok(()),
            TypeKind::Pointer(_) | TypeKind::Int(_) | TypeKind::Enum(_) => {
                let value = self.emit_memory_load(source, ty, true, span, state)?;
                self.emit_memory_store(destination, value, ty, true, span, state)?;
                Ok(())
            }
            _ => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "volatile aggregate member has no executable scalar representation",
            }),
        }
    }

    fn replace_effect_tokens(&self, state: &mut State, memory: Value, world: Option<Value>) {
        state.env.insert(
            EnvKey::Memory,
            BoundValue {
                value: memory,
                integer: None,
            },
        );
        if let Some(world) = world {
            state.env.insert(
                EnvKey::World,
                BoundValue {
                    value: world,
                    integer: None,
                },
            );
        }
    }

    fn object_address(
        &mut self,
        symbol: SymbolId,
        span: SourceSpan,
        state: &State,
    ) -> Result<Value, LowerError> {
        if let Some(address) = state.env.get(&EnvKey::Address(symbol)) {
            return Ok(address.value.clone());
        }
        let declaration = self
            .unit
            .symbols()
            .symbol(symbol)
            .map_err(|_| LowerError::UnknownSymbol { symbol, span })?;
        if declaration.scope() != self.unit.symbols().file_scope() {
            return Err(LowerError::InvalidGeneratedSsa {
                message: format!(
                    "automatic memory object '{}' ({}) has no live address binding",
                    declaration.spelling(),
                    symbol.raw()
                ),
            });
        }
        let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        self.emit(
            state,
            Inst::GlobalAddr {
                result: address.clone(),
                symbol: declaration.spelling().to_owned(),
            },
        )?;
        Ok(address)
    }

    fn offset_address(
        &mut self,
        base: Value,
        index: ExprValue,
        direction: OffsetDirection,
        signedness: IndexSignedness,
        scale: u64,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let result = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        self.emit(
            state,
            Inst::PtrOffset {
                result: result.clone(),
                base,
                index: index.value,
                direction,
                signedness,
                scale,
            },
        )?;
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        self.emit(
            state,
            Inst::PointerValid {
                mem_in,
                pointer: result.clone(),
                mem_out: mem_out.clone(),
            },
        )?;
        self.replace_effect_tokens(state, mem_out, None);
        Ok(result)
    }

    fn constant_offset_address(
        &mut self,
        base: Value,
        offset: u64,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        if offset == 0 {
            return Ok(base);
        }
        let raw = self.fresh_value(EvoIRType::Bits(64));
        self.emit(state, Inst::Const(raw.clone(), offset))?;
        self.offset_address(
            base,
            ExprValue {
                value: raw,
                integer: CIntegerType::ULONG,
            },
            OffsetDirection::Add,
            IndexSignedness::Unsigned,
            1,
            state,
        )
    }

    fn lower_place_address(
        &mut self,
        place: &'a MemoryPlace<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let mut address = match place.root() {
            PlaceRoot::Object { symbol } => self.object_address(*symbol, span, state)?,
            PlaceRoot::PointerExpression { pointer, .. } => {
                let lowered = self.lower_expr(pointer, state)?;
                if !matches!(lowered.value.ty, EvoIRType::Ptr(_)) {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature: "memory place pointer root did not lower to a pointer",
                    });
                }
                lowered.value
            }
            PlaceRoot::CompoundLiteral { expression, .. } => {
                self.lower_compound_literal_address(expression, state)?
            }
        };
        for projection in place.projections() {
            match projection {
                PlaceProjection::Index(projection) => {
                    let index = self.lower_expr(projection.index(), state)?;
                    let promoted = self.cast(index, projection.promoted_index(), state)?;
                    let layout = self.access_layout(projection.element_ty(), span)?;
                    address = self.offset_address(
                        address,
                        promoted,
                        OffsetDirection::Add,
                        if projection.promoted_index().is_signed() {
                            IndexSignedness::Signed
                        } else {
                            IndexSignedness::Unsigned
                        },
                        layout.size_bytes,
                        state,
                    )?;
                }
                PlaceProjection::Field(projection) => {
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), projection.tag())
                        .map_err(|source| LowerError::Layout {
                            ty: projection.field_ty(),
                            span,
                            source,
                        })?;
                    let field = layout
                        .fields
                        .iter()
                        .find(|field| field.field_index == projection.field_index())
                        .ok_or(LowerError::UnsupportedExpression {
                            span,
                            feature: "record projection has no target field layout",
                        })?;
                    address = self.constant_offset_address(address, field.byte_offset, state)?;
                }
            }
        }
        Ok(address)
    }

    fn emit_memory_load(
        &mut self,
        address: Value,
        ty: TypeId,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let (result_ty, integer) = self.c_value_type(ty, span)?;
        let layout = self.access_layout(ty, span)?;
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let guarded = self.fresh_value(EvoIRType::Mem(Region::All));
        self.emit(
            state,
            Inst::AccessInBounds {
                mem_in,
                pointer: address.clone(),
                mem_out: guarded.clone(),
                layout,
            },
        )?;
        self.replace_effect_tokens(state, guarded.clone(), None);
        let world_in = self.token(state, EnvKey::World)?.value;
        let result = self.fresh_value(result_ty);
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Load {
                result: result.clone(),
                address,
                mem_in: guarded,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                volatile,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    fn emit_memory_store(
        &mut self,
        address: Value,
        value: ExprValue,
        ty: TypeId,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let layout = self.access_layout(ty, span)?;
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let guarded = self.fresh_value(EvoIRType::Mem(Region::All));
        self.emit(
            state,
            Inst::AccessInBounds {
                mem_in,
                pointer: address.clone(),
                mem_out: guarded.clone(),
                layout,
            },
        )?;
        self.replace_effect_tokens(state, guarded.clone(), None);
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Store {
                address,
                value: value.value.clone(),
                mem_in: guarded,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                volatile,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(value)
    }

    fn coerce_to_object_type(
        &mut self,
        value: ExprValue,
        source: Option<&CHirExpr<'_>>,
        ty: TypeId,
        span: SourceSpan,
        state: &State,
    ) -> Result<ExprValue, LowerError> {
        match self.unit.types().get(ty).map(|canonical| &canonical.kind) {
            Some(TypeKind::Pointer(_)) => {
                let expected = self.c_value_type(ty, span)?.0;
                if value.value.ty == expected {
                    return Ok(value);
                }
                if source
                    .is_some_and(|source| null_pointer_constant(self.unit, self.target, source))
                {
                    let pointer = self.fresh_value(expected);
                    self.emit(state, Inst::PtrNull(pointer.clone()))?;
                    return Ok(ExprValue {
                        value: pointer,
                        integer: CIntegerType::ULONG,
                    });
                }
                if matches!(value.value.ty, EvoIRType::Bits(_)) {
                    let address = self.cast(value, CIntegerType::ULONG, state)?;
                    let result = self.fresh_value(expected);
                    self.emit(
                        state,
                        Inst::PointerCast {
                            result: result.clone(),
                            operand: address.value,
                        },
                    )?;
                    return Ok(ExprValue {
                        value: result,
                        integer: CIntegerType::ULONG,
                    });
                }
                Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "integer-to-pointer conversion is not a null pointer constant",
                })
            }
            Some(TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_)) => {
                let integer = canonical_integer(self.unit.types(), self.target, ty, span)?;
                self.cast(value, integer, state)
            }
            _ => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "initializer/store destination is not scalar",
            }),
        }
    }

    fn emit_zero_initializer_at(
        &mut self,
        address: Value,
        ty: TypeId,
        path: SubobjectPath,
        actions: &[InitAction<'a>],
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
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
            .ok_or(LowerError::UnknownType { ty, span })?
            .kind
            .clone();
        match kind {
            TypeKind::Int(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_) => {
                let zero = match kind {
                    TypeKind::Pointer(_) => {
                        let pointer = self.fresh_value(self.c_value_type(ty, span)?.0);
                        self.emit(state, Inst::PtrNull(pointer.clone()))?;
                        ExprValue {
                            value: pointer,
                            integer: CIntegerType::ULONG,
                        }
                    }
                    _ => {
                        let integer = canonical_integer(self.unit.types(), self.target, ty, span)?;
                        self.constant(state, integer, 0)?
                    }
                };
                self.emit_memory_store(address, zero, ty, false, span, state)?;
                Ok(())
            }
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(count),
            } => {
                let element_layout = self.access_layout(element, span)?;
                for index in 0..count {
                    let offset = index.checked_mul(element_layout.size_bytes).ok_or(
                        LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "zero initializer element offset overflow",
                        },
                    )?;
                    let element_address =
                        self.constant_offset_address(address.clone(), offset, state)?;
                    self.emit_zero_initializer_at(
                        element_address,
                        element,
                        path.clone().with_index(index),
                        actions,
                        span,
                        state,
                    )?;
                }
                Ok(())
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Ok(()),
            TypeKind::Array {
                bound: ArrayBound::Variable,
                ..
            }
            | TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::Function { .. } => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "object cannot be zero-initialized",
            }),
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                let is_union = matches!(kind, TypeKind::Union(_));
                let definition = self
                    .unit
                    .types()
                    .tag(tag)
                    .ok_or(LowerError::UnsupportedType {
                        ty,
                        span,
                        feature: "zero initializer record tag is absent",
                    })?;
                let fields = match &definition.state {
                    TagState::Complete(
                        TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                    ) => fields.clone(),
                    _ => {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "zero initializer record is incomplete",
                        });
                    }
                };
                let record_layout = self
                    .target
                    .record_layout(self.unit.types(), tag)
                    .map_err(|source| LowerError::Layout { ty, span, source })?;
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
                    let Some(layout) = record_layout
                        .fields
                        .iter()
                        .find(|layout| layout.field_index == field_index)
                    else {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "zero initializer field has no layout",
                        });
                    };
                    if layout.bit_field.is_some() {
                        return Err(LowerError::UnsupportedType {
                            ty: field.ty,
                            span,
                            feature: "bit-field zero initialization is outside lowering",
                        });
                    }
                    let field_address =
                        self.constant_offset_address(address.clone(), layout.byte_offset, state)?;
                    self.emit_zero_initializer_at(
                        field_address,
                        field.ty,
                        path.clone().with_field(field_index),
                        actions,
                        span,
                        state,
                    )?;
                    if is_union {
                        break;
                    }
                }
                Ok(())
            }
        }
    }

    fn lower_memory_declaration(
        &mut self,
        object: &'a ObjectDecl<'a>,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let mut layout = self.access_layout(object.ty, object.span)?;
        layout.align_bytes = layout.align_bytes.max(
            self.unit
                .symbols()
                .symbol(object.symbol)
                .map_err(|_| LowerError::UnknownSymbol {
                    symbol: object.symbol,
                    span: object.span,
                })?
                .declared_alignment(),
        );
        let origin = *self.allocation_origins.get(&object.symbol).ok_or(
            LowerError::InvalidGeneratedSsa {
                message: format!(
                    "automatic object {} has no allocation origin",
                    object.symbol.raw()
                ),
            },
        )?;
        if object.storage == StorageClass::Static {
            let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
            self.emit(
                state,
                Inst::GlobalAddr {
                    result: address.clone(),
                    symbol: format!("__evocc.static.{origin}"),
                },
            )?;
            state.env.insert(
                EnvKey::Address(object.symbol),
                BoundValue {
                    value: address,
                    integer: None,
                },
            );
            state.locals.insert(object.symbol);
            return Ok(());
        }
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Alloca {
                result: address.clone(),
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                origin,
                layout,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        state.env.insert(
            EnvKey::Address(object.symbol),
            BoundValue {
                value: address.clone(),
                integer: None,
            },
        );
        state.locals.insert(object.symbol);

        let Some(initializer) = &object.initializer else {
            return Ok(());
        };
        if initializer.plan().zero_fills_unwritten() {
            self.emit_zero_initializer_at(
                address.clone(),
                object.ty,
                SubobjectPath::root(),
                initializer.plan().actions(),
                object.span,
                state,
            )?;
        }
        for action in initializer.plan().actions() {
            let action_span = action.span().unwrap_or(object.span);
            let (offset, actual_ty) = subobject_offset(
                self.unit,
                self.target,
                object.ty,
                action.path(),
                action_span,
            )?;
            if actual_ty != action.destination_type() {
                return Err(LowerError::UnsupportedStatement {
                    span: action_span,
                    feature: "initializer destination type does not match subobject path",
                });
            }
            let action_address = self.constant_offset_address(
                address.clone(),
                u64::try_from(offset).map_err(|_| LowerError::UnsupportedStatement {
                    span: action_span,
                    feature: "initializer has a negative object offset",
                })?,
                state,
            )?;
            match action {
                InitAction::StoreScalar {
                    destination_type,
                    value,
                    ..
                } => {
                    let lowered = self.lower_expr(value, state)?;
                    let lowered = self.coerce_to_object_type(
                        lowered,
                        Some(value),
                        *destination_type,
                        action_span,
                        state,
                    )?;
                    let volatile = self
                        .unit
                        .types()
                        .get(*destination_type)
                        .is_some_and(|ty| ty.qualifiers.is_volatile);
                    self.emit_memory_store(
                        action_address,
                        lowered,
                        *destination_type,
                        volatile,
                        action_span,
                        state,
                    )?;
                }
                InitAction::ZeroScalar {
                    destination_type, ..
                } => {
                    let zero = match self.unit.types().get(*destination_type).map(|ty| &ty.kind) {
                        Some(TypeKind::Pointer(_)) => {
                            let pointer = self
                                .fresh_value(self.c_value_type(*destination_type, action_span)?.0);
                            self.emit(state, Inst::PtrNull(pointer.clone()))?;
                            ExprValue {
                                value: pointer,
                                integer: CIntegerType::ULONG,
                            }
                        }
                        _ => {
                            let integer = canonical_integer(
                                self.unit.types(),
                                self.target,
                                *destination_type,
                                action_span,
                            )?;
                            self.constant(state, integer, 0)?
                        }
                    };
                    self.emit_memory_store(
                        action_address,
                        zero,
                        *destination_type,
                        false,
                        action_span,
                        state,
                    )?;
                }
                InitAction::StoreString {
                    destination_type,
                    literal,
                    ..
                } => {
                    let (element, count) =
                        match self.unit.types().get(*destination_type).map(|ty| &ty.kind) {
                            Some(TypeKind::Array {
                                element,
                                bound: ArrayBound::Fixed(count),
                            }) => (*element, *count),
                            _ => {
                                return Err(LowerError::UnsupportedStatement {
                                    span: action_span,
                                    feature: "string initializer destination is not a fixed array",
                                });
                            }
                        };
                    let mut units =
                        literal
                            .decoded_units()
                            .ok_or(LowerError::UnsupportedStatement {
                                span: action_span,
                                feature: "string initializer decoding failed",
                            })?;
                    let count =
                        usize::try_from(count).map_err(|_| LowerError::UnsupportedType {
                            ty: *destination_type,
                            span: action_span,
                            feature: "string destination length exceeds host representation",
                        })?;
                    units.resize(count, 0);
                    let element_layout = self.access_layout(element, action_span)?;
                    for (index, unit) in units.into_iter().take(count).enumerate() {
                        let address = self.constant_offset_address(
                            action_address.clone(),
                            u64::try_from(index)
                                .ok()
                                .and_then(|index| index.checked_mul(element_layout.size_bytes))
                                .ok_or(LowerError::UnsupportedType {
                                    ty: element,
                                    span: action_span,
                                    feature: "string element offset overflow",
                                })?,
                            state,
                        )?;
                        let integer = canonical_integer(
                            self.unit.types(),
                            self.target,
                            element,
                            action_span,
                        )?;
                        let value = self.constant(state, integer, u64::from(unit))?;
                        self.emit_memory_store(address, value, element, false, action_span, state)?;
                    }
                }
                InitAction::CopyAggregate {
                    destination_type,
                    value,
                    span,
                    ..
                } => {
                    let source = self.lower_expr(value, state)?;
                    let volatile = self
                        .unit
                        .types()
                        .get(*destination_type)
                        .is_some_and(|ty| ty.qualifiers.is_volatile)
                        || self.aggregate_source_is_volatile(value);
                    self.emit_aggregate_transfer(
                        action_address,
                        source.value,
                        *destination_type,
                        volatile,
                        *span,
                        state,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn lower_initializer_at(
        &mut self,
        address: Value,
        object_ty: TypeId,
        initializer: &ObjectInitializer<'a>,
        object_span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        if initializer.plan().zero_fills_unwritten() {
            self.emit_zero_initializer_at(
                address.clone(),
                object_ty,
                SubobjectPath::root(),
                initializer.plan().actions(),
                object_span,
                state,
            )?;
        }
        for action in initializer.plan().actions() {
            let action_span = action.span().unwrap_or(object_span);
            let (offset, actual_ty) = subobject_offset(
                self.unit,
                self.target,
                object_ty,
                action.path(),
                action_span,
            )?;
            if actual_ty != action.destination_type() {
                return Err(LowerError::UnsupportedStatement {
                    span: action_span,
                    feature: "initializer destination type does not match subobject path",
                });
            }
            let action_address = self.constant_offset_address(
                address.clone(),
                u64::try_from(offset).map_err(|_| LowerError::UnsupportedStatement {
                    span: action_span,
                    feature: "initializer has a negative object offset",
                })?,
                state,
            )?;
            match action {
                InitAction::StoreScalar {
                    destination_type,
                    value,
                    ..
                } => {
                    let lowered = self.lower_expr(value, state)?;
                    let lowered = self.coerce_to_object_type(
                        lowered,
                        Some(value),
                        *destination_type,
                        action_span,
                        state,
                    )?;
                    let volatile = self
                        .unit
                        .types()
                        .get(*destination_type)
                        .is_some_and(|ty| ty.qualifiers.is_volatile);
                    self.emit_memory_store(
                        action_address,
                        lowered,
                        *destination_type,
                        volatile,
                        action_span,
                        state,
                    )?;
                }
                InitAction::ZeroScalar {
                    destination_type, ..
                } => {
                    let zero = match self.unit.types().get(*destination_type).map(|ty| &ty.kind) {
                        Some(TypeKind::Pointer(_)) => {
                            let pointer = self
                                .fresh_value(self.c_value_type(*destination_type, action_span)?.0);
                            self.emit(state, Inst::PtrNull(pointer.clone()))?;
                            ExprValue {
                                value: pointer,
                                integer: CIntegerType::ULONG,
                            }
                        }
                        _ => {
                            let integer = canonical_integer(
                                self.unit.types(),
                                self.target,
                                *destination_type,
                                action_span,
                            )?;
                            self.constant(state, integer, 0)?
                        }
                    };
                    self.emit_memory_store(
                        action_address,
                        zero,
                        *destination_type,
                        false,
                        action_span,
                        state,
                    )?;
                }
                InitAction::StoreString {
                    destination_type,
                    literal,
                    ..
                } => {
                    let (element, count) =
                        match self.unit.types().get(*destination_type).map(|ty| &ty.kind) {
                            Some(TypeKind::Array {
                                element,
                                bound: ArrayBound::Fixed(count),
                            }) => (*element, *count),
                            _ => {
                                return Err(LowerError::UnsupportedStatement {
                                    span: action_span,
                                    feature: "string initializer destination is not a fixed array",
                                });
                            }
                        };
                    let mut units =
                        literal
                            .decoded_units()
                            .ok_or(LowerError::UnsupportedStatement {
                                span: action_span,
                                feature: "string initializer decoding failed",
                            })?;
                    let count =
                        usize::try_from(count).map_err(|_| LowerError::UnsupportedType {
                            ty: *destination_type,
                            span: action_span,
                            feature: "string destination length exceeds host representation",
                        })?;
                    units.resize(count, 0);
                    let element_layout = self.access_layout(element, action_span)?;
                    for (index, unit) in units.into_iter().take(count).enumerate() {
                        let address = self.constant_offset_address(
                            action_address.clone(),
                            u64::try_from(index)
                                .ok()
                                .and_then(|index| index.checked_mul(element_layout.size_bytes))
                                .ok_or(LowerError::UnsupportedType {
                                    ty: element,
                                    span: action_span,
                                    feature: "string element offset overflow",
                                })?,
                            state,
                        )?;
                        let integer = canonical_integer(
                            self.unit.types(),
                            self.target,
                            element,
                            action_span,
                        )?;
                        let value = self.constant(state, integer, u64::from(unit))?;
                        self.emit_memory_store(address, value, element, false, action_span, state)?;
                    }
                }
                InitAction::CopyAggregate {
                    destination_type,
                    value,
                    span,
                    ..
                } => {
                    let source = self.lower_expr(value, state)?;
                    let volatile = self
                        .unit
                        .types()
                        .get(*destination_type)
                        .is_some_and(|ty| ty.qualifiers.is_volatile)
                        || self.aggregate_source_is_volatile(value);
                    self.emit_aggregate_transfer(
                        action_address,
                        source.value,
                        *destination_type,
                        volatile,
                        *span,
                        state,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn lower_compound_literal_address(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let CHirExpr::CompoundLit(ty, initializer, span) = expression else {
            return Err(LowerError::UnsupportedExpression {
                span: expression.span(),
                feature: "compound-literal allocation requested for another expression",
            });
        };
        if initializer.plan().target_type() != *ty
            || initializer.plan().validate(self.unit.types()).is_err()
        {
            return Err(LowerError::UnsupportedExpression {
                span: *span,
                feature: "compound-literal initializer plan is invalid",
            });
        }
        let identity = expression as *const CHirExpr<'a> as usize;
        let (origin, static_storage) = self.compound_origins.get(&identity).copied().ok_or(
            LowerError::InvalidGeneratedSsa {
                message: "compound literal has no allocation origin".to_owned(),
            },
        )?;
        let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        if static_storage {
            return Err(LowerError::UnsupportedExpression {
                span: *span,
                feature:
                    "file-scope compound-literal relocation has no specified EvoIR global identity",
            });
        }
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Alloca {
                result: address.clone(),
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                origin,
                layout: self.access_layout(*ty, *span)?,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        self.lower_initializer_at(address.clone(), *ty, initializer, *span, state)?;
        Ok(address)
    }

    fn end_object_lifetime(
        &mut self,
        symbol: SymbolId,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let pointer = state
            .env
            .get(&EnvKey::Address(symbol))
            .map(|bound| bound.value.clone())
            .ok_or(LowerError::InvalidGeneratedSsa {
                message: format!("lifetime end has no address for object {}", symbol.raw()),
            })?;
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::LifetimeEnd {
                mem_in,
                world_in,
                pointer,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(())
    }

    fn lower_if(
        &mut self,
        condition: &'a CHirExpr<'a>,
        then_branch: &'a CHirStmt<'a>,
        else_branch: Option<&'a CHirStmt<'a>>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let condition = self.lower_condition(condition, &mut state)?;
        let mut then_symbols = live_after.clone();
        collect_symbols_stmt(then_branch, &mut then_symbols);
        let then_keys = self.keys_for(&state, &then_symbols, &[]);
        let direct_then = self.direct_jump(then_branch, control);
        let then_state = if direct_then.is_none() {
            Some(self.new_block_from(&state, &then_keys, BlockRole::Ordinary)?)
        } else {
            None
        };

        let mut else_symbols = live_after.clone();
        if let Some(branch) = else_branch {
            collect_symbols_stmt(branch, &mut else_symbols);
        }
        let else_keys = self.keys_for(&state, &else_symbols, &[]);
        let direct_else = else_branch.and_then(|branch| self.direct_jump(branch, control));
        let else_state = if direct_else.is_none() {
            Some(self.new_block_from(&state, &else_keys, BlockRole::Ordinary)?)
        } else {
            None
        };

        let (then_target, then_args) = if let Some(target) = direct_then {
            (target.id, self.edge_values(&state, &target.keys)?)
        } else {
            let target = then_state.as_ref().ok_or(LowerError::InvalidControlFlow {
                span: then_branch.span(),
                reason: "if-then block was not constructed",
            })?;
            (target.block, self.edge_values(&state, &then_keys)?)
        };
        let (else_target, else_args) = if let Some(target) = direct_else {
            (target.id, self.edge_values(&state, &target.keys)?)
        } else {
            let target = else_state.as_ref().ok_or(LowerError::InvalidControlFlow {
                span: else_branch.map_or(then_branch.span(), CHirStmt::span),
                reason: "if-else block was not constructed",
            })?;
            (target.block, self.edge_values(&state, &else_keys)?)
        };
        self.terminate(
            &state,
            Inst::CondBr(condition, then_target, then_args, else_target, else_args),
        )?;

        let mut normal = Vec::new();
        if let Some(then_state) = then_state {
            normal.extend(
                self.lower_stmt(then_branch, then_state, live_after, control)?
                    .normal,
            );
        }
        if let Some(else_state) = else_state {
            if let Some(branch) = else_branch {
                normal.extend(
                    self.lower_stmt(branch, else_state, live_after, control)?
                        .normal,
                );
            } else {
                normal.push(else_state);
            }
        }
        Ok(Flow { normal })
    }

    fn direct_jump<'b>(
        &self,
        statement: &'b CHirStmt<'b>,
        control: &'b ControlTargets,
    ) -> Option<&'b JumpTarget> {
        match statement {
            CHirStmt::Break(_) => control.break_target.as_ref(),
            CHirStmt::Continue(_) => control.continue_target.as_ref(),
            _ => None,
        }
    }

    fn lower_while(
        &mut self,
        condition: &'a CHirExpr<'a>,
        body: &'a CHirStmt<'a>,
        state: State,
        live_after: &BTreeSet<SymbolId>,
        outer: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let mut carried = live_after.clone();
        collect_symbols_expr(condition, &mut carried);
        collect_symbols_stmt(body, &mut carried);
        let carried_keys = self.keys_for(&state, &carried, &[]);
        let header = self.new_block_from(&state, &carried_keys, BlockRole::Ordinary)?;
        self.branch(&state, header.block, &carried_keys)?;
        let mut header_after_condition = header.clone();
        let known_truth = constant_truth(self.unit, self.target, condition);
        let predicate = if known_truth.is_none() {
            Some(self.lower_condition(condition, &mut header_after_condition)?)
        } else {
            None
        };
        let body_keys = self.keys_for(&header_after_condition, &carried, &[]);
        let body_state =
            self.new_block_from(&header_after_condition, &body_keys, BlockRole::Ordinary)?;
        let break_placeholder = self.fresh_placeholder();
        let break_keys = self.keys_for(&header_after_condition, live_after, &[]);
        let body_arguments = self.edge_values(&header_after_condition, &body_keys)?;
        let break_arguments = self.edge_values(&header_after_condition, &break_keys)?;
        match (known_truth, predicate) {
            (Some(true), None) => self.terminate(
                &header_after_condition,
                Inst::Br(body_state.block, body_arguments),
            )?,
            (Some(false), None) => self.terminate(
                &header_after_condition,
                Inst::Br(break_placeholder, break_arguments),
            )?,
            (None, Some(predicate)) => self.terminate(
                &header_after_condition,
                Inst::CondBr(
                    predicate,
                    body_state.block,
                    body_arguments,
                    break_placeholder,
                    break_arguments,
                ),
            )?,
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: condition.span(),
                    reason: "while condition classification is inconsistent",
                });
            }
        }
        let control = ControlTargets {
            break_target: Some(JumpTarget {
                id: break_placeholder,
                keys: break_keys.clone(),
            }),
            continue_target: Some(JumpTarget {
                id: header.block,
                keys: carried_keys.clone(),
            }),
        };
        let flow = self.lower_stmt(body, body_state, &carried, &control)?;
        for normal in flow.normal {
            self.branch(&normal, header.block, &carried_keys)?;
        }
        let exit =
            self.new_block_from(&header_after_condition, &break_keys, BlockRole::Ordinary)?;
        self.replace_target(break_placeholder, exit.block);
        let _ = outer;
        Ok(Flow { normal: vec![exit] })
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_for(
        &mut self,
        initializer: &'a CHirForInit<'a>,
        condition: Option<&'a CHirExpr<'a>>,
        step: Option<&'a CHirExpr<'a>>,
        body: &'a CHirStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        _outer: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let mut init_symbols = Vec::new();
        match initializer {
            CHirForInit::Empty => {}
            CHirForInit::Expression(expression) => {
                self.lower_discarded_expr(expression, &mut state)?;
            }
            CHirForInit::Declaration(declarations) => {
                for declaration in declarations {
                    if let CHirDecl::Object(object) = declaration {
                        init_symbols.push(object.symbol);
                    }
                    self.lower_declaration(declaration, &mut state)?;
                }
            }
        }
        let mut carried = live_after.clone();
        if let Some(condition) = condition {
            collect_symbols_expr(condition, &mut carried);
        }
        if let Some(step) = step {
            collect_symbols_expr(step, &mut carried);
        }
        collect_symbols_stmt(body, &mut carried);
        let carried_keys = self.keys_for(&state, &carried, &init_symbols);
        let header = self.new_block_from(&state, &carried_keys, BlockRole::Ordinary)?;
        self.branch(&state, header.block, &carried_keys)?;
        let mut header_condition = header.clone();
        let known_truth =
            condition.and_then(|condition| constant_truth(self.unit, self.target, condition));
        let predicate = if condition.is_some() && known_truth.is_none() {
            condition
                .map(|condition| self.lower_condition(condition, &mut header_condition))
                .transpose()?
        } else {
            None
        };
        let body_keys = self.keys_for(&header_condition, &carried, &init_symbols);
        let body_state = self.new_block_from(&header_condition, &body_keys, BlockRole::Ordinary)?;
        let break_placeholder = self.fresh_placeholder();
        let continue_placeholder = self.fresh_placeholder();
        let break_keys = self.keys_for(&header_condition, live_after, &[]);
        let body_arguments = self.edge_values(&header_condition, &body_keys)?;
        match (condition, known_truth, predicate) {
            (None, None, None) | (Some(_), Some(true), None) => {
                self.terminate(
                    &header_condition,
                    Inst::Br(body_state.block, body_arguments),
                )?;
            }
            (Some(_), Some(false), None) => {
                let break_arguments = self.edge_values(&header_condition, &break_keys)?;
                self.terminate(
                    &header_condition,
                    Inst::Br(break_placeholder, break_arguments),
                )?;
            }
            (Some(_), None, Some(predicate)) => {
                let break_arguments = self.edge_values(&header_condition, &break_keys)?;
                self.terminate(
                    &header_condition,
                    Inst::CondBr(
                        predicate,
                        body_state.block,
                        body_arguments,
                        break_placeholder,
                        break_arguments,
                    ),
                )?;
            }
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: condition.map_or(self.info.span, CHirExpr::span),
                    reason: "for condition classification is inconsistent",
                });
            }
        }
        let control = ControlTargets {
            break_target: Some(JumpTarget {
                id: break_placeholder,
                keys: break_keys.clone(),
            }),
            continue_target: Some(JumpTarget {
                id: continue_placeholder,
                keys: carried_keys.clone(),
            }),
        };
        let body_flow = self.lower_stmt(body, body_state, &carried, &control)?;
        let step_template = body_flow
            .normal
            .first()
            .cloned()
            .unwrap_or_else(|| header.clone());
        let mut step_state =
            self.new_block_from(&step_template, &carried_keys, BlockRole::LoopStep)?;
        self.replace_target(continue_placeholder, step_state.block);
        for normal in &body_flow.normal {
            self.branch(normal, step_state.block, &carried_keys)?;
        }
        let normal_predecessor_positions = body_flow
            .normal
            .iter()
            .filter_map(|normal| {
                self.blocks
                    .iter()
                    .position(|block| block.id == normal.block)
            })
            .collect::<Vec<_>>();
        if let Some(step) = step {
            self.lower_discarded_expr(step, &mut step_state)?;
        }
        self.branch(&step_state, header.block, &carried_keys)?;
        if let Some(position) = normal_predecessor_positions.into_iter().min() {
            self.move_block_before(step_state.block, position)?;
        }
        let exit = self.new_block_from(&header_condition, &break_keys, BlockRole::Ordinary)?;
        self.replace_target(break_placeholder, exit.block);
        let mut exit = exit;
        for symbol in init_symbols {
            exit.env.remove(&EnvKey::Local(symbol));
            exit.locals.remove(&symbol);
        }
        Ok(Flow { normal: vec![exit] })
    }

    fn lower_do_while(
        &mut self,
        body: &'a CHirStmt<'a>,
        condition: &'a CHirExpr<'a>,
        state: State,
        live_after: &BTreeSet<SymbolId>,
        _outer: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let mut carried = live_after.clone();
        collect_symbols_stmt(body, &mut carried);
        collect_symbols_expr(condition, &mut carried);
        let carried_keys = self.keys_for(&state, &carried, &[]);
        let body_state = self.new_block_from(&state, &carried_keys, BlockRole::Ordinary)?;
        let body_block = body_state.block;
        self.branch(&state, body_state.block, &carried_keys)?;
        let condition_placeholder = self.fresh_placeholder();
        let break_placeholder = self.fresh_placeholder();
        let break_keys = self.keys_for(&body_state, live_after, &[]);
        let control = ControlTargets {
            break_target: Some(JumpTarget {
                id: break_placeholder,
                keys: break_keys.clone(),
            }),
            continue_target: Some(JumpTarget {
                id: condition_placeholder,
                keys: carried_keys.clone(),
            }),
        };
        let body_flow = self.lower_stmt(body, body_state.clone(), &carried, &control)?;
        let condition_template = body_flow.normal.first().cloned().unwrap_or(body_state);
        let mut condition_state =
            self.new_block_from(&condition_template, &carried_keys, BlockRole::Ordinary)?;
        self.replace_target(condition_placeholder, condition_state.block);
        for normal in body_flow.normal {
            self.branch(&normal, condition_state.block, &carried_keys)?;
        }
        match constant_truth(self.unit, self.target, condition) {
            Some(true) => {
                let body_arguments = self.edge_values(&condition_state, &carried_keys)?;
                self.terminate(&condition_state, Inst::Br(body_block, body_arguments))?;
            }
            Some(false) => {
                let break_arguments = self.edge_values(&condition_state, &break_keys)?;
                self.terminate(
                    &condition_state,
                    Inst::Br(break_placeholder, break_arguments),
                )?;
            }
            None => {
                let predicate = self.lower_condition(condition, &mut condition_state)?;
                let body_arguments = self.edge_values(&condition_state, &carried_keys)?;
                let break_arguments = self.edge_values(&condition_state, &break_keys)?;
                self.terminate(
                    &condition_state,
                    Inst::CondBr(
                        predicate,
                        body_block,
                        body_arguments,
                        break_placeholder,
                        break_arguments,
                    ),
                )?;
            }
        }
        let exit = self.new_block_from(&condition_state, &break_keys, BlockRole::Ordinary)?;
        self.replace_target(break_placeholder, exit.block);
        Ok(Flow { normal: vec![exit] })
    }

    fn lower_switch(
        &mut self,
        expression: &'a CHirExpr<'a>,
        body: &'a CHirStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        outer: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let selector = self.lower_expr(expression, &mut state)?;
        let selector_type = integer_promotion(selector.integer);
        let selector = self.cast(selector, selector_type, &state)?;
        let selector_temporary = if matches!(expression, CHirExpr::Ident { .. }) {
            None
        } else {
            let key = EnvKey::Temporary(self.next_temporary);
            self.next_temporary += 1;
            state.env.insert(
                key,
                BoundValue {
                    value: selector.value.clone(),
                    integer: Some(selector.integer),
                },
            );
            Some(key)
        };
        let (prefix, segments) = switch_segments(body, self.unit, self.target, selector_type)?;
        let mut switch_locals = state.locals.clone();
        for item in prefix {
            if let CHirBlockItem::Declaration(CHirDecl::Object(object)) = item {
                switch_locals.insert(object.symbol);
            }
        }
        for segment in &segments {
            for item in &segment.items {
                if let SwitchSegmentItem::Declaration(CHirDecl::Object(object)) = item {
                    switch_locals.insert(object.symbol);
                }
            }
        }
        let prefix_is_targeted = prefix.iter().any(|item| {
            matches!(
                item,
                CHirBlockItem::Statement(statement)
                    if contains_targeted_label(statement, &self.goto_names)
            )
        });
        if segments.is_empty() {
            if !prefix_is_targeted {
                if let Some(selector_temporary) = selector_temporary {
                    state.env.remove(&selector_temporary);
                }
                return Ok(Flow {
                    normal: vec![state],
                });
            }
            let mut prefix_symbols = live_after.clone();
            for item in prefix {
                collect_symbols_item(item, &mut prefix_symbols);
            }
            let mut prefix_template = state.clone();
            prefix_template.locals.clone_from(&switch_locals);
            let mut prefix_keys = self.keys_for(&prefix_template, &prefix_symbols, &[]);
            if let Some(selector_temporary) = selector_temporary {
                prefix_keys.retain(|key| *key != selector_temporary);
            }
            let detached =
                self.new_block_from(&prefix_template, &prefix_keys, BlockRole::Ordinary)?;
            let exit_keys = self.keys_for(&state, live_after, &[]);
            let exit = self.new_block_from(&state, &exit_keys, BlockRole::SwitchExit)?;
            let control = ControlTargets {
                break_target: Some(JumpTarget {
                    id: exit.block,
                    keys: exit_keys.clone(),
                }),
                continue_target: outer.continue_target.clone(),
            };
            let prefix_flow =
                self.lower_compound_items(prefix, detached, live_after, &control, false)?;
            self.branch(&state, exit.block, &exit_keys)?;
            for normal in prefix_flow.normal {
                self.branch(&normal, exit.block, &exit_keys)?;
            }
            return Ok(Flow { normal: vec![exit] });
        }
        let break_placeholder = self.fresh_placeholder();
        let mut break_keys = self.keys_for(&state, live_after, &[]);
        if let Some(selector_temporary) = selector_temporary {
            break_keys.retain(|key| *key != selector_temporary);
        }
        let mut entries = Vec::new();
        let mut dispatch_states = Vec::new();
        let mut current_dispatch = state.clone();
        for (index, segment) in segments.iter().enumerate() {
            let mut symbols = live_after.clone();
            for later in &segments[index..] {
                for item in &later.items {
                    collect_symbols_switch_item(item, &mut symbols);
                }
            }
            let mut entry_template = current_dispatch.clone();
            entry_template.locals.clone_from(&switch_locals);
            let mut keys = self.keys_for(&entry_template, &symbols, &[]);
            if let Some(selector_temporary) = selector_temporary {
                keys.retain(|key| *key != selector_temporary);
            }
            let entry = self.new_block_from(&entry_template, &keys, BlockRole::Ordinary)?;
            entries.push((entry, keys));
            if segment.case.is_some() {
                let next_case = segments[index + 1..]
                    .iter()
                    .position(|candidate| candidate.case.is_some())
                    .map(|offset| index + 1 + offset);
                if next_case.is_some() {
                    let mut remaining = live_after.clone();
                    collect_symbols_expr(expression, &mut remaining);
                    for later in &segments[index + 1..] {
                        if let Some(case) = later.case {
                            match case {
                                SwitchCase::Single(expression) => {
                                    collect_symbols_expr(expression, &mut remaining);
                                }
                                SwitchCase::Range(first, last) => {
                                    collect_symbols_expr(first, &mut remaining);
                                    collect_symbols_expr(last, &mut remaining);
                                }
                            }
                        }
                        for item in &later.items {
                            collect_symbols_switch_item(item, &mut remaining);
                        }
                    }
                    let next_keys = self.keys_for(&current_dispatch, &remaining, &[]);
                    let next =
                        self.new_block_from(&current_dispatch, &next_keys, BlockRole::Ordinary)?;
                    dispatch_states.push((index, current_dispatch, next.clone(), next_keys));
                    current_dispatch = next;
                } else {
                    dispatch_states.push((
                        index,
                        current_dispatch.clone(),
                        entries[index].0.clone(),
                        Vec::new(),
                    ));
                }
            }
        }

        let default_index = segments.iter().position(|segment| segment.case.is_none());
        if dispatch_states.is_empty() {
            let default = default_index.ok_or(LowerError::InvalidControlFlow {
                span: body.span(),
                reason: "switch has no case or default dispatch target",
            })?;
            self.branch(&state, entries[default].0.block, &entries[default].1)?;
        }
        for (position, (index, mut dispatch, next, next_keys)) in
            dispatch_states.into_iter().enumerate()
        {
            let case_matcher = segments[index].case.ok_or(LowerError::InvalidControlFlow {
                span: body.span(),
                reason: "switch dispatch does not reference a case",
            })?;
            let current_selector = if let CHirExpr::Ident { symbol, span, .. } = expression {
                self.local(&dispatch, *symbol, *span)?
            } else {
                let key = selector_temporary.ok_or(LowerError::InvalidControlFlow {
                    span: expression.span(),
                    reason: "switch selector temporary is absent",
                })?;
                let bound = dispatch
                    .env
                    .get(&key)
                    .ok_or(LowerError::InvalidControlFlow {
                        span: expression.span(),
                        reason: "switch selector was not threaded to a dispatch block",
                    })?;
                ExprValue {
                    value: bound.value.clone(),
                    integer: bound.integer.ok_or(LowerError::InvalidControlFlow {
                        span: expression.span(),
                        reason: "switch selector temporary is not an integer",
                    })?,
                }
            };
            let compared_type = current_selector.integer;
            let selector = self.cast(current_selector, compared_type, &dispatch)?;
            let condition = match case_matcher {
                SwitchCase::Single(expression) => {
                    let case_value = self.lower_expr(expression, &mut dispatch)?;
                    let case_value = self.cast(case_value, compared_type, &dispatch)?;
                    let condition = self.fresh_value(EvoIRType::Bits(1));
                    self.emit(
                        &dispatch,
                        Inst::ICmp(
                            condition.clone(),
                            CmpOp::Eq,
                            selector.value,
                            case_value.value,
                        ),
                    )?;
                    condition
                }
                SwitchCase::Range(first, last) => {
                    let first = self.lower_expr(first, &mut dispatch)?;
                    let last = self.lower_expr(last, &mut dispatch)?;
                    let first = self.cast(first, compared_type, &dispatch)?;
                    let last = self.cast(last, compared_type, &dispatch)?;
                    let comparison = if compared_type.is_signed() {
                        CmpOp::SLe
                    } else {
                        CmpOp::ULe
                    };
                    let above_first = self.fresh_value(EvoIRType::Bits(1));
                    let below_last = self.fresh_value(EvoIRType::Bits(1));
                    let condition = self.fresh_value(EvoIRType::Bits(1));
                    self.emit(
                        &dispatch,
                        Inst::ICmp(
                            above_first.clone(),
                            comparison,
                            first.value,
                            selector.value.clone(),
                        ),
                    )?;
                    self.emit(
                        &dispatch,
                        Inst::ICmp(below_last.clone(), comparison, selector.value, last.value),
                    )?;
                    self.emit(
                        &dispatch,
                        Inst::And(condition.clone(), above_first, below_last),
                    )?;
                    condition
                }
            };
            let true_entry = &entries[index];
            let (false_target, false_args) =
                if position + 1 < segments.len() && !next_keys.is_empty() {
                    (next.block, self.edge_values(&dispatch, &next_keys)?)
                } else if let Some(default) = default_index {
                    let default_entry = &entries[default];
                    (
                        default_entry.0.block,
                        self.edge_values(&dispatch, &default_entry.1)?,
                    )
                } else {
                    (break_placeholder, self.edge_values(&dispatch, &break_keys)?)
                };
            let true_args = self.edge_values(&dispatch, &true_entry.1)?;
            self.terminate(
                &dispatch,
                Inst::CondBr(
                    condition,
                    true_entry.0.block,
                    true_args,
                    false_target,
                    false_args,
                ),
            )?;
        }

        let control = ControlTargets {
            break_target: Some(JumpTarget {
                id: break_placeholder,
                keys: break_keys.clone(),
            }),
            continue_target: outer.continue_target.clone(),
        };
        if prefix_is_targeted {
            let mut prefix_live = live_after.clone();
            for segment in &segments {
                for item in &segment.items {
                    collect_symbols_switch_item(item, &mut prefix_live);
                }
            }
            let mut prefix_template = state.clone();
            prefix_template.locals.clone_from(&switch_locals);
            let mut prefix_keys = self.keys_for(&prefix_template, &prefix_live, &[]);
            if let Some(selector_temporary) = selector_temporary {
                prefix_keys.retain(|key| *key != selector_temporary);
            }
            let detached =
                self.new_block_from(&prefix_template, &prefix_keys, BlockRole::Ordinary)?;
            let prefix_flow =
                self.lower_compound_items(prefix, detached, &prefix_live, &control, false)?;
            for normal in prefix_flow.normal {
                self.branch(&normal, entries[0].0.block, &entries[0].1)?;
            }
        }
        for index in 0..segments.len() {
            let mut normal = vec![entries[index].0.clone()];
            for (item_index, item) in segments[index].items.iter().enumerate() {
                let mut future = live_after.clone();
                for later_item in &segments[index].items[item_index + 1..] {
                    collect_symbols_switch_item(later_item, &mut future);
                }
                for later in &segments[index + 1..] {
                    for later_item in &later.items {
                        collect_symbols_switch_item(later_item, &mut future);
                    }
                }
                let mut needed_now = future.clone();
                collect_symbols_switch_item(item, &mut needed_now);
                let pending = std::mem::take(&mut normal);
                let Some(mut current) = self.merge_states(pending, &needed_now)? else {
                    break;
                };
                normal = match item {
                    SwitchSegmentItem::Declaration(declaration) => {
                        self.lower_declaration(declaration, &mut current)?;
                        vec![current]
                    }
                    SwitchSegmentItem::Statement(statement) => {
                        self.lower_stmt(statement, current, &future, &control)?
                            .normal
                    }
                };
            }
            if index + 1 < segments.len() {
                let next = &entries[index + 1];
                for current in normal {
                    self.branch(&current, next.0.block, &next.1)?;
                }
            } else {
                for current in normal {
                    self.branch(&current, break_placeholder, &break_keys)?;
                }
            }
        }
        let exit = self.new_block_from(&state, &break_keys, BlockRole::SwitchExit)?;
        self.replace_target(break_placeholder, exit.block);
        Ok(Flow { normal: vec![exit] })
    }

    fn lower_expr(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        match expression {
            CHirExpr::Lit(literal, _) => {
                let integer = integer_literal_value(*literal).map_err(|_| {
                    LowerError::UnsupportedExpression {
                        span: expression.span(),
                        feature: "integer literal has no Day-6 target representation",
                    }
                })?;
                self.wide_constant(state, integer.ty(), integer.raw_bits())
            }
            CHirExpr::CharLit(literal, _) => {
                let result = self.fresh_value(bits(CIntegerType::INT));
                self.emit(state, Inst::Const(result.clone(), u64::from(literal.value)))?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::INT,
                })
            }
            CHirExpr::Ident {
                symbol,
                spelling,
                facts,
                span,
                ..
            } => {
                let declaration =
                    self.unit
                        .symbols()
                        .symbol(*symbol)
                        .map_err(|_| LowerError::UnknownSymbol {
                            symbol: *symbol,
                            span: *span,
                        })?;
                if declaration.kind() == SymbolKind::Enumerator {
                    if declaration.spelling() != spelling
                        || declaration.ty() != facts.ty()
                        || facts.category() != ExprCategory::Value
                    {
                        return Err(LowerError::UnsupportedExpression {
                            span: *span,
                            feature: "enumerator identifier disagrees with its semantic identity",
                        });
                    }
                    let integer =
                        canonical_integer(self.unit.types(), self.target, facts.ty(), *span)?;
                    let result = self.fresh_value(bits(integer));
                    let value = declaration.enumerator_value().ok_or(
                        LowerError::UnsupportedExpression {
                            span: *span,
                            feature: "enumerator symbol is missing its canonical value",
                        },
                    )?;
                    self.emit(state, Inst::Const(result.clone(), value as u64))?;
                    return Ok(ExprValue {
                        value: result,
                        integer,
                    });
                }
                if declaration.kind() == SymbolKind::Function
                    && declaration.spelling() == spelling
                    && declaration.ty() == facts.ty()
                    && facts.category() == ExprCategory::FunctionDesignator
                {
                    let result = self.fresh_value(self.fnptr_type(facts.ty(), *span)?);
                    self.emit(
                        state,
                        Inst::FuncAddr {
                            result: result.clone(),
                            symbol: spelling.clone(),
                        },
                    )?;
                    return Ok(ExprValue {
                        value: result,
                        integer: CIntegerType::ULONG,
                    });
                }
                if !matches!(
                    declaration.kind(),
                    SymbolKind::Object | SymbolKind::Parameter
                ) || declaration.spelling() != spelling
                    || declaration.ty() != facts.ty()
                    || facts.category() != ExprCategory::LValue
                {
                    return Err(LowerError::UnsupportedExpression {
                        span: *span,
                        feature: "object identifier disagrees with its semantic identity",
                    });
                }
                if self.memory_symbols.contains(symbol) {
                    let address = self.object_address(*symbol, *span, state)?;
                    if matches!(
                        self.unit.types().get(facts.ty()).map(|ty| &ty.kind),
                        Some(
                            TypeKind::Array { .. }
                                | TypeKind::Struct(_)
                                | TypeKind::Union(_)
                                | TypeKind::BuiltinVaList
                        )
                    ) {
                        return Ok(ExprValue {
                            value: address,
                            integer: CIntegerType::ULONG,
                        });
                    }
                    let volatile = self
                        .unit
                        .types()
                        .get(facts.ty())
                        .is_some_and(|ty| ty.qualifiers.is_volatile);
                    return self.emit_memory_load(address, facts.ty(), volatile, *span, state);
                }
                self.local(state, *symbol, *span)
            }
            CHirExpr::Unary(operator, operand, span) => {
                self.lower_unary(*operator, operand, *span, state)
            }
            CHirExpr::EvaluateOnce(operand, _) => self.lower_expr(operand, state),
            CHirExpr::Binary(operator, left, right, span) => {
                self.lower_binary(*operator, left, right, *span, state)
            }
            CHirExpr::Ternary(condition, then_value, else_value, _) => {
                self.lower_ternary(condition, then_value, else_value, state)
            }
            CHirExpr::Elvis(operand, fallback, _) => self.lower_elvis(operand, fallback, state),
            CHirExpr::Call(callee, arguments, span) => self
                .emit_call(callee, arguments, *span, state)?
                .ok_or(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "void call used as a scalar expression",
                }),
            CHirExpr::CheckedArithmetic {
                op,
                lhs,
                rhs,
                target,
                target_ty,
                span,
                ..
            } => self.lower_checked(*op, lhs, rhs, target, *target_ty, *span, state),
            CHirExpr::BitBuiltin { op, operand, .. } => self.lower_bit(*op, operand, state),
            CHirExpr::BuiltinVarArgs {
                op,
                first,
                second,
                requested_ty,
                span,
                ..
            } => self
                .lower_varargs(*op, first, *second, *requested_ty, *span, state)?
                .ok_or(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "void variadic operation used as a value",
                }),
            CHirExpr::BuiltinReturnAddress { span, .. } => {
                Err(LowerError::ReturnAddress { span: *span })
            }
            CHirExpr::BuiltinFrameAddress { span, .. } => {
                Err(LowerError::FrameAddress { span: *span })
            }
            CHirExpr::LabelAddress { span, .. } => Err(LowerError::ComputedGoto { span: *span }),
            CHirExpr::BuiltinUnreachable { span, .. } => Err(LowerError::UnsupportedExpression {
                span: *span,
                feature: "unreachable builtin must be a discarded expression",
            }),
            CHirExpr::StatementExpr {
                body,
                value,
                facts,
                span,
            } => self
                .lower_statement_expression(body, *value, *facts, *span, state)?
                .ok_or(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "void statement expression used where a value is required",
                }),
            CHirExpr::CompoundLit(ty, _, span) => {
                let address = self.lower_compound_literal_address(expression, state)?;
                if matches!(
                    self.unit.types().get(*ty).map(|canonical| &canonical.kind),
                    Some(
                        TypeKind::Array { .. }
                            | TypeKind::Struct(_)
                            | TypeKind::Union(_)
                            | TypeKind::BuiltinVaList
                    )
                ) {
                    Ok(ExprValue {
                        value: address,
                        integer: CIntegerType::ULONG,
                    })
                } else {
                    let volatile = self
                        .unit
                        .types()
                        .get(*ty)
                        .is_some_and(|canonical| canonical.qualifiers.is_volatile);
                    self.emit_memory_load(address, *ty, volatile, *span, state)
                }
            }
            CHirExpr::Cast(target, operand, span) => {
                if matches!(
                    self.unit.types().get(*target).map(|ty| &ty.kind),
                    Some(TypeKind::Pointer(_))
                ) {
                    let value = self.lower_expr(operand, state)?;
                    return self.coerce_to_object_type(value, Some(operand), *target, *span, state);
                }
                let target = canonical_integer(self.unit.types(), self.target, *target, *span)?;
                if let CHirExpr::Lit(literal, _) = operand {
                    let converted = integer_literal_value(*literal)
                        .map_err(|_| LowerError::UnsupportedExpression {
                            span: operand.span(),
                            feature: "integer literal has no target representation",
                        })?
                        .cast(target);
                    return self.wide_constant(state, target, converted.raw_bits());
                }
                let value = self.lower_expr(operand, state)?;
                self.cast(value, target, state)
            }
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                span,
                ..
            } => self.lower_assignment(*op, *place, target, value, *span, state),
            CHirExpr::Update {
                op,
                place,
                target,
                span,
                ..
            } => self.lower_update(*op, *place, target, *span, state),
            CHirExpr::MemoryAssign {
                op,
                place,
                target_span,
                value,
                span,
                ..
            } => self.lower_memory_assignment(*op, place, *target_span, value, *span, state),
            CHirExpr::MemoryUpdate {
                op,
                place,
                target_span,
                span,
                ..
            } => self.lower_memory_update(*op, place, *target_span, *span, state),
            CHirExpr::Comma(left, right, _) => {
                self.lower_discarded_expr(left, state)?;
                self.lower_expr(right, state)
            }
            CHirExpr::SizeOf(ty, span) | CHirExpr::SizeOfExpr(_, ty, span) => {
                let size = self
                    .target
                    .sizeof_type(self.unit.types(), *ty)
                    .map_err(|source| LowerError::Layout {
                        ty: *ty,
                        span: *span,
                        source,
                    })?;
                let result = self.fresh_value(bits(CIntegerType::ULONG));
                self.emit(state, Inst::Const(result.clone(), size))?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                })
            }
            CHirExpr::AlignOf(ty, span) => {
                let layout = self
                    .target
                    .layout_of(self.unit.types(), *ty)
                    .map_err(|source| LowerError::Layout {
                        ty: *ty,
                        span: *span,
                        source,
                    })?;
                let result = self.fresh_value(bits(CIntegerType::ULONG));
                self.emit(
                    state,
                    Inst::Const(result.clone(), u64::from(layout.align_bytes)),
                )?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                })
            }
            CHirExpr::StringLit(_, span) => {
                let origin = self
                    .string_origins
                    .get(&(expression as *const CHirExpr<'_> as usize))
                    .ok_or(LowerError::UnsupportedExpression {
                        span: *span,
                        feature: "string has no allocation site",
                    })?;
                let symbol = format!("__evocc.string.{origin}");
                let result = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
                self.emit(
                    state,
                    Inst::GlobalAddr {
                        result: result.clone(),
                        symbol,
                    },
                )?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                })
            }
            CHirExpr::FloatLit(_, span)
            | CHirExpr::Member(_, _, span)
            | CHirExpr::PtrMember(_, _, span)
            | CHirExpr::Index(_, _, span) => Err(LowerError::UnsupportedExpression {
                span: *span,
                feature: "expression requires the Day-7 object or non-integer model",
            }),
            CHirExpr::MemoryRead {
                place, target_span, ..
            } => {
                let address = self.lower_place_address(place, *target_span, state)?;
                if self.is_aggregate_type(place.value_ty()) {
                    return Ok(ExprValue {
                        value: address,
                        integer: CIntegerType::ULONG,
                    });
                }
                self.emit_memory_load(
                    address,
                    place.value_ty(),
                    place.access_qualifiers().is_volatile,
                    *target_span,
                    state,
                )
            }
        }
    }

    fn lower_statement_expression(
        &mut self,
        body: &'a CHirStmt<'a>,
        value: Option<&'a CHirExpr<'a>>,
        facts: ExprFacts,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        let CHirStmt::Compound(items, _) = body else {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "statement expression body is not a compound statement",
            });
        };
        let prefix = if let Some(value) = value {
            let Some(CHirBlockItem::Statement(final_statement)) = items.last() else {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "statement expression value is not its final expression statement",
                });
            };
            let Some(final_value) = statement_expression_value(final_statement) else {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "statement expression value is not its final expression statement",
                });
            };
            if !std::ptr::eq(final_value, value) {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "statement expression value identity is not authoritative",
                });
            }
            &items[..items.len() - 1]
        } else {
            items.as_ref()
        };

        let mut live = state.locals.clone();
        if let Some(value) = value {
            collect_symbols_expr(value, &mut live);
        }
        let control = self.active_control.clone();
        let flow = self.lower_compound_items(prefix, state.clone(), &live, &control, false)?;
        let Some(mut continued) = self.merge_states(flow.normal, &live)? else {
            return Err(LowerError::InvalidControlFlow {
                span,
                reason: "statement expression has no normal path to its value",
            });
        };

        let result = value
            .map(|value| self.lower_expr(value, &mut continued))
            .transpose()?;
        if let Some(result) = &result {
            let expected = if self.is_aggregate_type(facts.ty()) {
                EvoIRType::Ptr(AddressSpace::Generic)
            } else {
                self.c_value_type(facts.ty(), span)?.0
            };
            if result.value.ty != expected {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "statement expression result disagrees with its semantic type",
                });
            }
        }

        for item in items {
            let CHirBlockItem::Declaration(CHirDecl::Object(object)) = item else {
                continue;
            };
            if self.memory_symbols.contains(&object.symbol)
                && continued.env.contains_key(&EnvKey::Address(object.symbol))
                && object.storage != StorageClass::Static
            {
                self.end_object_lifetime(object.symbol, &mut continued)?;
            }
            continued.env.remove(&EnvKey::Local(object.symbol));
            continued.env.remove(&EnvKey::Address(object.symbol));
            continued.locals.remove(&object.symbol);
        }
        *state = continued;
        Ok(result)
    }

    fn lower_unary(
        &mut self,
        operator: UnaryOp,
        operand: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if operator == UnaryOp::AddrOf {
            let address = match operand {
                CHirExpr::Ident { symbol, .. } => self.object_address(*symbol, span, state)?,
                CHirExpr::MemoryRead { place, .. } => {
                    self.lower_place_address(place, span, state)?
                }
                CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
                    let pointer = self.lower_expr(pointer, state)?;
                    if !matches!(pointer.value.ty, EvoIRType::Ptr(_)) {
                        return Err(LowerError::UnsupportedExpression {
                            span,
                            feature: "address-of dereference operand is not a pointer",
                        });
                    }
                    pointer.value
                }
                CHirExpr::CompoundLit(_, _, _) => {
                    self.lower_compound_literal_address(operand, state)?
                }
                _ => {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature: "address-of operand has no authoritative memory place",
                    });
                }
            };
            return Ok(ExprValue {
                value: address,
                integer: CIntegerType::ULONG,
            });
        }
        if operator == UnaryOp::Deref {
            let pointer = self.lower_expr(operand, state)?;
            if !matches!(pointer.value.ty, EvoIRType::Ptr(_)) {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "dereference operand did not lower to a pointer",
                });
            }
            let pointee = self.pointer_pointee_type(operand, span)?;
            if self.is_aggregate_type(pointee) {
                return Ok(ExprValue {
                    value: pointer.value,
                    integer: CIntegerType::ULONG,
                });
            }
            let volatile = self
                .unit
                .types()
                .get(pointee)
                .is_some_and(|ty| ty.qualifiers.is_volatile);
            return self.emit_memory_load(pointer.value, pointee, volatile, span, state);
        }
        let operand = self.lower_expr(operand, state)?;
        match operator {
            UnaryOp::Plus => {
                let promoted = integer_promotion(operand.integer);
                self.cast(operand, promoted, state)
            }
            UnaryOp::Not => {
                let truth = self.lower_condition_value(operand, state)?;
                let zero = self.fresh_value(EvoIRType::Bits(1));
                self.emit(state, Inst::Const(zero.clone(), 0))?;
                let result = self.fresh_value(EvoIRType::Bits(1));
                self.emit(state, Inst::ICmp(result.clone(), CmpOp::Eq, truth, zero))?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::INT,
                })
            }
            UnaryOp::Neg => {
                let integer = integer_promotion(operand.integer);
                let operand = self.cast(operand, integer, state)?;
                let zero = self.constant(state, integer, 0)?;
                self.emit_signed_arithmetic(state, BinaryOp::Sub, zero, operand, integer)
            }
            UnaryOp::BitNot => {
                let integer = integer_promotion(operand.integer);
                let operand = self.cast(operand, integer, state)?;
                let mask = self.wide_constant(state, integer, integer.mask())?;
                let result = self.fresh_value(bits(integer));
                self.emit(state, Inst::Xor(result.clone(), operand.value, mask.value))?;
                Ok(ExprValue {
                    value: result,
                    integer,
                })
            }
            UnaryOp::AddrOf | UnaryOp::Deref => unreachable!("handled before operand conversion"),
        }
    }

    fn pointer_pointee_type(
        &self,
        expression: &CHirExpr<'_>,
        span: SourceSpan,
    ) -> Result<TypeId, LowerError> {
        evocc_chir::PointerTypeDesc::from_expression(
            self.unit.symbols(),
            self.unit.types(),
            expression,
        )
        .map(|pointer| pointer.pointee())
        .map_err(|_| LowerError::UnsupportedExpression {
            span,
            feature: "pointer expression type is unavailable",
        })
    }

    fn lower_binary(
        &mut self,
        operator: BinaryOp,
        left: &'a CHirExpr<'a>,
        right: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if matches!(operator, BinaryOp::And | BinaryOp::Or) {
            return self.lower_logical(operator, left, right, state);
        }
        if operator == BinaryOp::Comma {
            self.lower_discarded_expr(left, state)?;
            return self.lower_expr(right, state);
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
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "assignment must be represented by CHirExpr::Assign",
            });
        }
        let left_expression = left;
        let right_expression = right;
        let left = self.lower_expr(left, state)?;
        let left_key = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        state.env.insert(
            left_key,
            BoundValue {
                value: left.value,
                integer: Some(left.integer),
            },
        );
        let right = self.lower_expr(right, state)?;
        let left = state
            .env
            .remove(&left_key)
            .ok_or(LowerError::InvalidControlFlow {
                span,
                reason: "right operand control flow lost the evaluated left operand",
            })?;
        let left = ExprValue {
            value: left.value,
            integer: left.integer.ok_or(LowerError::InvalidControlFlow {
                span,
                reason: "threaded left operand lost its integer type",
            })?,
        };
        if matches!(left.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. })
            || matches!(right.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. })
        {
            return self.lower_pointer_binary(
                operator,
                left_expression,
                right_expression,
                left,
                right,
                span,
                state,
            );
        }
        if matches!(operator, BinaryOp::Shl | BinaryOp::Shr) {
            let shifted_type = integer_promotion(left.integer);
            let amount_type = integer_promotion(right.integer);
            let left = self.cast(left, shifted_type, state)?;
            let right = self.cast(right, amount_type, state)?;
            return self.emit_shift(state, operator, left, right, shifted_type);
        }
        let common = arithmetic_common_type(left.integer, right.integer);
        let left = self.cast(left, common, state)?;
        let right = self.cast(right, common, state)?;
        match operator {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                self.emit_signed_arithmetic(state, operator, left, right, common)
            }
            BinaryOp::Div | BinaryOp::Mod => {
                self.emit_div_rem(state, operator, left, right, common)
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                let result = self.fresh_value(bits(common));
                let instruction = match operator {
                    BinaryOp::BitAnd => Inst::And(result.clone(), left.value, right.value),
                    BinaryOp::BitOr => Inst::Or(result.clone(), left.value, right.value),
                    BinaryOp::BitXor => Inst::Xor(result.clone(), left.value, right.value),
                    _ => {
                        return Err(LowerError::InvalidControlFlow {
                            span,
                            reason: "non-bitwise operator reached bitwise emission",
                        });
                    }
                };
                self.emit(state, instruction)?;
                Ok(ExprValue {
                    value: result,
                    integer: common,
                })
            }
            BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne => {
                let result = self.fresh_value(EvoIRType::Bits(1));
                let (comparison, lhs, rhs) = match operator {
                    BinaryOp::Eq => (CmpOp::Eq, left.value, right.value),
                    BinaryOp::Ne => (CmpOp::Ne, left.value, right.value),
                    BinaryOp::Lt if common.is_signed() => (CmpOp::SLt, left.value, right.value),
                    BinaryOp::Lt => (CmpOp::ULt, left.value, right.value),
                    BinaryOp::Le if common.is_signed() => (CmpOp::SLe, left.value, right.value),
                    BinaryOp::Le => (CmpOp::ULe, left.value, right.value),
                    BinaryOp::Gt if common.is_signed() => (CmpOp::SLt, right.value, left.value),
                    BinaryOp::Gt => (CmpOp::ULt, right.value, left.value),
                    BinaryOp::Ge if common.is_signed() => (CmpOp::SLe, right.value, left.value),
                    BinaryOp::Ge => (CmpOp::ULe, right.value, left.value),
                    _ => {
                        return Err(LowerError::InvalidControlFlow {
                            span,
                            reason: "non-comparison operator reached comparison emission",
                        });
                    }
                };
                self.emit(state, Inst::ICmp(result.clone(), comparison, lhs, rhs))?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::INT,
                })
            }
            _ => Err(LowerError::UnsupportedExpression {
                span,
                feature: "unsupported binary operator",
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_pointer_binary(
        &mut self,
        operator: BinaryOp,
        left_expression: &'a CHirExpr<'a>,
        right_expression: &'a CHirExpr<'a>,
        left: ExprValue,
        right: ExprValue,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let left_pointer = matches!(left.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. });
        let right_pointer = matches!(right.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. });
        let left_object_pointer = matches!(left.value.ty, EvoIRType::Ptr(_));
        let right_object_pointer = matches!(right.value.ty, EvoIRType::Ptr(_));
        if matches!(operator, BinaryOp::Add | BinaryOp::Sub)
            && (matches!(left.value.ty, EvoIRType::FnPtr { .. })
                || matches!(right.value.ty, EvoIRType::FnPtr { .. }))
        {
            return Err(LowerError::FunctionPointerArithmetic { span });
        }
        if matches!(operator, BinaryOp::Add | BinaryOp::Sub)
            && left_object_pointer
            && !right_pointer
        {
            let pointee = self.pointer_pointee_type(left_expression, span)?;
            let scale_bytes = self.pointer_arithmetic_scale(pointee, span)?;
            let promoted_type = integer_promotion(right.integer);
            let index = self.cast(right, promoted_type, state)?;
            let value = self.offset_address(
                left.value,
                index,
                if operator == BinaryOp::Add {
                    OffsetDirection::Add
                } else {
                    OffsetDirection::Subtract
                },
                if promoted_type.is_signed() {
                    IndexSignedness::Signed
                } else {
                    IndexSignedness::Unsigned
                },
                scale_bytes,
                state,
            )?;
            return Ok(ExprValue {
                value,
                integer: CIntegerType::ULONG,
            });
        }
        if operator == BinaryOp::Add && !left_pointer && right_object_pointer {
            let pointee = self.pointer_pointee_type(right_expression, span)?;
            let scale_bytes = self.pointer_arithmetic_scale(pointee, span)?;
            let promoted_type = integer_promotion(left.integer);
            let index = self.cast(left, promoted_type, state)?;
            let value = self.offset_address(
                right.value,
                index,
                OffsetDirection::Add,
                if promoted_type.is_signed() {
                    IndexSignedness::Signed
                } else {
                    IndexSignedness::Unsigned
                },
                scale_bytes,
                state,
            )?;
            return Ok(ExprValue {
                value,
                integer: CIntegerType::ULONG,
            });
        }
        if operator == BinaryOp::Sub && left_object_pointer && right_object_pointer {
            return Err(LowerError::PointerDifference { span });
        }
        if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) {
            let (left, right) = match (left_pointer, right_pointer) {
                (true, true) if left.value.ty == right.value.ty => (left.value, right.value),
                (true, true) if null_pointer_constant(self.unit, self.target, left_expression) => {
                    let null = self.fresh_value(right.value.ty.clone());
                    self.emit(state, Inst::PtrNull(null.clone()))?;
                    (null, right.value)
                }
                (true, true) if null_pointer_constant(self.unit, self.target, right_expression) => {
                    let null = self.fresh_value(left.value.ty.clone());
                    self.emit(state, Inst::PtrNull(null.clone()))?;
                    (left.value, null)
                }
                (true, false)
                    if null_pointer_constant(self.unit, self.target, right_expression) =>
                {
                    let null = self.fresh_value(left.value.ty.clone());
                    self.emit(state, Inst::PtrNull(null.clone()))?;
                    (left.value, null)
                }
                (false, true) if null_pointer_constant(self.unit, self.target, left_expression) => {
                    let null = self.fresh_value(right.value.ty.clone());
                    self.emit(state, Inst::PtrNull(null.clone()))?;
                    (null, right.value)
                }
                _ => {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature:
                            "pointer comparison requires a compatible pointer or null constant",
                    });
                }
            };
            let result = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::ICmp(
                    result.clone(),
                    if operator == BinaryOp::Eq {
                        CmpOp::Eq
                    } else {
                        CmpOp::Ne
                    },
                    left,
                    right,
                ),
            )?;
            return Ok(ExprValue {
                value: result,
                integer: CIntegerType::INT,
            });
        }
        Err(LowerError::UnsupportedExpression {
            span,
            feature: "unsupported pointer binary operation",
        })
    }

    fn emit_signed_arithmetic(
        &mut self,
        state: &State,
        operator: BinaryOp,
        left: ExprValue,
        right: ExprValue,
        integer: CIntegerType,
    ) -> Result<ExprValue, LowerError> {
        let result = self.fresh_value(bits(integer));
        if integer.is_signed() {
            let defined = self.fresh_value(EvoIRType::Bits(1));
            let predicate = match operator {
                BinaryOp::Add => {
                    Inst::SignedAddDefined(defined.clone(), left.value.clone(), right.value.clone())
                }
                BinaryOp::Sub => {
                    Inst::SignedSubDefined(defined.clone(), left.value.clone(), right.value.clone())
                }
                BinaryOp::Mul => {
                    Inst::SignedMulDefined(defined.clone(), left.value.clone(), right.value.clone())
                }
                _ => {
                    return Err(LowerError::InvalidControlFlow {
                        span: self.info.span,
                        reason: "non-arithmetic operator reached signed arithmetic emission",
                    });
                }
            };
            self.emit(state, predicate)?;
            self.emit(state, Inst::UbAssume(defined, UbReason::SignedOverflow))?;
        }
        let instruction = match operator {
            BinaryOp::Add => Inst::Add(result.clone(), left.value, right.value),
            BinaryOp::Sub => Inst::Sub(result.clone(), left.value, right.value),
            BinaryOp::Mul => Inst::Mul(result.clone(), left.value, right.value),
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "unsupported arithmetic emission",
                });
            }
        };
        self.emit(state, instruction)?;
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    fn emit_div_rem(
        &mut self,
        state: &State,
        operator: BinaryOp,
        left: ExprValue,
        right: ExprValue,
        integer: CIntegerType,
    ) -> Result<ExprValue, LowerError> {
        if integer.bits() == 128 {
            return Err(LowerError::WideInteger {
                span: self.info.span,
            });
        }
        let nonzero = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::DivisorNonZero(nonzero.clone(), right.value.clone()),
        )?;
        self.emit(
            state,
            Inst::UbAssume(
                nonzero,
                if operator == BinaryOp::Div {
                    UbReason::DivisionByZero
                } else {
                    UbReason::RemainderByZero
                },
            ),
        )?;
        if integer.is_signed() {
            let defined = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::SignedDivRemNoOverflow(
                    defined.clone(),
                    left.value.clone(),
                    right.value.clone(),
                ),
            )?;
            self.emit(state, Inst::UbAssume(defined, UbReason::SignedOverflow))?;
        }
        let result = self.fresh_value(bits(integer));
        let instruction = match (operator, integer.is_signed()) {
            (BinaryOp::Div, true) => Inst::SDiv(result.clone(), left.value, right.value),
            (BinaryOp::Div, false) => Inst::UDiv(result.clone(), left.value, right.value),
            (BinaryOp::Mod, true) => Inst::SRem(result.clone(), left.value, right.value),
            (BinaryOp::Mod, false) => Inst::URem(result.clone(), left.value, right.value),
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "unsupported division/remainder emission",
                });
            }
        };
        self.emit(state, instruction)?;
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    fn emit_shift(
        &mut self,
        state: &State,
        operator: BinaryOp,
        shifted: ExprValue,
        amount: ExprValue,
        integer: CIntegerType,
    ) -> Result<ExprValue, LowerError> {
        let count_defined = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::ShiftCountDefined {
                result: count_defined.clone(),
                shifted: shifted.value.clone(),
                amount: amount.value.clone(),
                amount_signed: amount.integer.is_signed(),
            },
        )?;
        self.emit(
            state,
            Inst::UbAssume(count_defined, UbReason::InvalidShiftCount),
        )?;
        if operator == BinaryOp::Shl && integer.is_signed() {
            let defined = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::SignedShlDefined {
                    result: defined.clone(),
                    shifted: shifted.value.clone(),
                    amount: amount.value.clone(),
                    amount_signed: amount.integer.is_signed(),
                },
            )?;
            self.emit(
                state,
                Inst::UbAssume(defined, UbReason::InvalidSignedLeftShift),
            )?;
        }
        let result = self.fresh_value(bits(integer));
        let instruction = match (operator, integer.is_signed()) {
            (BinaryOp::Shl, _) => Inst::Shl(result.clone(), shifted.value, amount.value),
            (BinaryOp::Shr, true) => Inst::AShr(result.clone(), shifted.value, amount.value),
            (BinaryOp::Shr, false) => Inst::LShr(result.clone(), shifted.value, amount.value),
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "unsupported shift emission",
                });
            }
        };
        self.emit(state, instruction)?;
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    fn lower_logical(
        &mut self,
        operator: BinaryOp,
        left: &'a CHirExpr<'a>,
        right: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let left = self.lower_condition(left, state)?;
        let symbols = state
            .env
            .keys()
            .filter_map(|key| match key {
                EnvKey::Local(symbol) => Some(*symbol),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let keys = self.keys_for(state, &symbols, &[]);
        let mut right_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let skip_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let (true_target, true_args, false_target, false_args) = if operator == BinaryOp::And {
            (
                right_state.block,
                self.edge_values(state, &keys)?,
                skip_state.block,
                self.edge_values(state, &keys)?,
            )
        } else {
            (
                skip_state.block,
                self.edge_values(state, &keys)?,
                right_state.block,
                self.edge_values(state, &keys)?,
            )
        };
        self.terminate(
            state,
            Inst::CondBr(left, true_target, true_args, false_target, false_args),
        )?;
        let right_value = self.lower_expr(right, &mut right_state)?;
        let right_result = self.lower_condition_value(right_value, &right_state)?;
        let mut skip_state = skip_state;
        let skip_result = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            &skip_state,
            Inst::Const(skip_result.clone(), u64::from(operator == BinaryOp::Or)),
        )?;
        let temporary = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        right_state.env.insert(
            temporary,
            BoundValue {
                value: right_result,
                integer: Some(CIntegerType::INT),
            },
        );
        skip_state.env.insert(
            temporary,
            BoundValue {
                value: skip_result,
                integer: Some(CIntegerType::INT),
            },
        );
        let mut merge_keys = keys;
        merge_keys.push(temporary);
        let merge = self.new_block_from(&right_state, &merge_keys, BlockRole::Ordinary)?;
        self.branch(&right_state, merge.block, &merge_keys)?;
        self.branch(&skip_state, merge.block, &merge_keys)?;
        *state = merge;
        let result = state
            .env
            .remove(&temporary)
            .ok_or(LowerError::InvalidControlFlow {
                span: right.span(),
                reason: "logical short-circuit merge lost its result",
            })?;
        Ok(ExprValue {
            value: result.value,
            integer: CIntegerType::INT,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_checked(
        &mut self,
        op: evocc_chir::CheckedArithmeticOp,
        lhs: &'a CHirExpr<'a>,
        rhs: &'a CHirExpr<'a>,
        target: &'a CHirExpr<'a>,
        target_ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let left = self.lower_expr(lhs, state)?;
        let left_key = self.retain_temporary(left.value, state);
        let right = self.lower_expr(rhs, state)?;
        let right_key = self.retain_temporary(right.value, state);
        let address = self.lower_expr(target, state)?.value;
        let left_width = u16::from(left.integer.bits()) + u16::from(!left.integer.is_signed());
        let right_width = u16::from(right.integer.bits()) + u16::from(!right.integer.is_signed());
        let required = match op {
            evocc_chir::CheckedArithmeticOp::Mul => left_width + right_width,
            _ => left_width.max(right_width) + 1,
        };
        let exact_type = match required {
            0..=32 => CIntegerType::INT,
            33..=64 => CIntegerType::LONG,
            65..=128 => CIntegerType::INT128,
            _ => {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "checked arithmetic requires more than 128 exact bits",
                })
            }
        };
        let left_value = state
            .env
            .remove(&left_key)
            .expect("retained checked operand")
            .value;
        let right_value = state
            .env
            .remove(&right_key)
            .expect("retained checked operand")
            .value;
        let left = self.cast(
            ExprValue {
                value: left_value,
                integer: left.integer,
            },
            exact_type,
            state,
        )?;
        let right = self.cast(
            ExprValue {
                value: right_value,
                integer: right.integer,
            },
            exact_type,
            state,
        )?;
        let exact = self.fresh_value(bits(exact_type));
        self.emit(
            state,
            match op {
                evocc_chir::CheckedArithmeticOp::Add => {
                    Inst::Add(exact.clone(), left.value, right.value)
                }
                evocc_chir::CheckedArithmeticOp::Sub => {
                    Inst::Sub(exact.clone(), left.value, right.value)
                }
                evocc_chir::CheckedArithmeticOp::Mul => {
                    Inst::Mul(exact.clone(), left.value, right.value)
                }
            },
        )?;
        let destination = canonical_integer(self.unit.types(), self.target, target_ty, span)?;
        let stored = self.cast(
            ExprValue {
                value: exact.clone(),
                integer: exact_type,
            },
            destination,
            state,
        )?;
        let round_trip = self.cast(stored.clone(), exact_type, state)?;
        let mut result = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::ICmp(result.clone(), CmpOp::Ne, exact.clone(), round_trip.value),
        )?;
        if !destination.is_signed() {
            let zero = self.constant(state, exact_type, 0)?;
            let negative = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::ICmp(negative.clone(), CmpOp::SLt, exact, zero.value),
            )?;
            let overflow = self.fresh_value(EvoIRType::Bits(1));
            self.emit(state, Inst::Or(overflow.clone(), result, negative))?;
            result = overflow;
        }
        let volatile = self
            .unit
            .types()
            .get(target_ty)
            .is_some_and(|ty| ty.qualifiers.is_volatile);
        self.emit_memory_store(address, stored, target_ty, volatile, span, state)?;
        Ok(ExprValue {
            value: result,
            integer: CIntegerType::BOOL,
        })
    }

    fn lower_bit(
        &mut self,
        op: evocc_chir::integer::BitBuiltinOp,
        operand: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        use evocc_chir::integer::BitBuiltinOp;
        let input = self.lower_expr(operand, state)?;
        let total_op = match op {
            BitBuiltinOp::CountLeadingZeros => evocc_evoir::BitOp::LeadingZeros,
            BitBuiltinOp::CountTrailingZeros | BitBuiltinOp::FindFirstSet => {
                evocc_evoir::BitOp::TrailingZeros
            }
            BitBuiltinOp::PopulationCount => evocc_evoir::BitOp::Population,
            BitBuiltinOp::ByteSwap => evocc_evoir::BitOp::ByteSwap,
        };
        let zero = self.constant(state, input.integer, 0)?;
        let nonzero = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::ICmp(nonzero.clone(), CmpOp::Ne, input.value.clone(), zero.value),
        )?;
        if matches!(
            op,
            BitBuiltinOp::CountLeadingZeros | BitBuiltinOp::CountTrailingZeros
        ) {
            self.emit(
                state,
                Inst::UbAssume(nonzero.clone(), UbReason::BitBuiltinZeroOperand),
            )?;
        }
        let counted = self.fresh_value(input.value.ty.clone());
        self.emit(
            state,
            Inst::BitOperation {
                result: counted.clone(),
                operand: input.value,
                op: total_op,
            },
        )?;
        let mut counted = ExprValue {
            value: counted,
            integer: input.integer,
        };
        if op == BitBuiltinOp::FindFirstSet {
            let one = self.constant(state, input.integer, 1)?;
            let incremented = self.fresh_value(counted.value.ty.clone());
            self.emit(
                state,
                Inst::Add(incremented.clone(), counted.value, one.value),
            )?;
            counted.value = incremented;
        }
        let result_type = if op == BitBuiltinOp::ByteSwap {
            input.integer
        } else {
            CIntegerType::INT
        };
        let counted = self.cast(counted, result_type, state)?;
        if op != BitBuiltinOp::FindFirstSet {
            return Ok(counted);
        }
        let zero = self.constant(state, CIntegerType::INT, 0)?;
        let temporary = self.retain_temporary(counted.value, state);
        let symbols = state
            .env
            .keys()
            .filter_map(|key| match key {
                EnvKey::Local(symbol) => Some(*symbol),
                _ => None,
            })
            .collect();
        let keys = self.keys_for(state, &symbols, &[]);
        let merge = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let nonzero_args = self.edge_values(state, &keys)?;
        let mut zero_args = nonzero_args.clone();
        zero_args[keys
            .iter()
            .position(|key| *key == temporary)
            .expect("result edge parameter")] = zero.value;
        self.terminate(
            state,
            Inst::CondBr(nonzero, merge.block, nonzero_args, merge.block, zero_args),
        )?;
        *state = merge;
        Ok(ExprValue {
            value: state
                .env
                .remove(&temporary)
                .expect("merged bit result")
                .value,
            integer: CIntegerType::INT,
        })
    }

    fn common_branch_values(
        &mut self,
        left: ExprValue,
        right: ExprValue,
        left_state: &State,
        right_state: &State,
    ) -> Result<(ExprValue, ExprValue, CIntegerType), LowerError> {
        if left.value.ty == right.value.ty
            && matches!(left.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. })
        {
            return Ok((left, right, CIntegerType::ULONG));
        }
        let common = arithmetic_common_type(left.integer, right.integer);
        Ok((
            self.cast(left, common, left_state)?,
            self.cast(right, common, right_state)?,
            common,
        ))
    }

    fn lower_ternary(
        &mut self,
        condition: &'a CHirExpr<'a>,
        then_expression: &'a CHirExpr<'a>,
        else_expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let predicate = self.lower_condition(condition, state)?;
        let all_symbols = state
            .env
            .keys()
            .filter_map(|key| match key {
                EnvKey::Local(symbol) => Some(*symbol),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let keys = self.keys_for(state, &all_symbols, &[]);
        let mut then_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let mut else_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let then_arguments = self.edge_values(state, &keys)?;
        let else_arguments = self.edge_values(state, &keys)?;
        self.terminate(
            state,
            Inst::CondBr(
                predicate,
                then_state.block,
                then_arguments,
                else_state.block,
                else_arguments,
            ),
        )?;
        let then_value = self.lower_expr(then_expression, &mut then_state)?;
        let else_value = self.lower_expr(else_expression, &mut else_state)?;
        let (then_value, else_value, common) =
            self.common_branch_values(then_value, else_value, &then_state, &else_state)?;
        let temporary = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        then_state.env.insert(
            temporary,
            BoundValue {
                value: then_value.value,
                integer: Some(common),
            },
        );
        else_state.env.insert(
            temporary,
            BoundValue {
                value: else_value.value,
                integer: Some(common),
            },
        );
        let mut merge_keys = keys;
        merge_keys.push(temporary);
        let merge = self.new_block_from(&then_state, &merge_keys, BlockRole::Ordinary)?;
        self.branch(&then_state, merge.block, &merge_keys)?;
        self.branch(&else_state, merge.block, &merge_keys)?;
        *state = merge;
        let result = state
            .env
            .remove(&temporary)
            .ok_or(LowerError::InvalidControlFlow {
                span: condition.span(),
                reason: "ternary merge lost its result",
            })?;
        Ok(ExprValue {
            value: result.value,
            integer: result.integer.ok_or(LowerError::InvalidControlFlow {
                span: condition.span(),
                reason: "ternary result has no integer type",
            })?,
        })
    }

    fn lower_elvis(
        &mut self,
        operand: &'a CHirExpr<'a>,
        else_expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        // Retain the one evaluated operand as an explicit edge value. The
        // true successor consumes that value directly; it never lowers the
        // source expression a second time.
        let operand_value = self.lower_expr(operand, state)?;
        let retained = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        state.env.insert(
            retained,
            BoundValue {
                value: operand_value.value.clone(),
                integer: Some(operand_value.integer),
            },
        );
        let predicate = self.lower_condition_value(operand_value, state)?;
        let all_symbols = state
            .env
            .keys()
            .filter_map(|key| match key {
                EnvKey::Local(symbol) => Some(*symbol),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let mut keys = self.keys_for(state, &all_symbols, &[]);
        // Normalize the retained value exactly where an explicit scalar local
        // initialized from the same SSA value would appear. `keys_for`
        // otherwise groups all temporaries after locals, which would make the
        // GNU shorthand observably different in Section 9 text despite
        // carrying the same edge values.
        keys.retain(|key| *key != retained);
        let retained_id = state
            .env
            .get(&retained)
            .ok_or(LowerError::InvalidControlFlow {
                span: operand.span(),
                reason: "elvis lowering lost its retained operand",
            })?
            .value
            .id;
        let insertion = keys
            .iter()
            .position(|key| {
                state
                    .env
                    .get(key)
                    .is_none_or(|bound| bound.value.id > retained_id)
            })
            .unwrap_or(keys.len());
        keys.insert(insertion, retained);
        let mut then_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let mut else_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let then_arguments = self.edge_values(state, &keys)?;
        let else_arguments = self.edge_values(state, &keys)?;
        self.terminate(
            state,
            Inst::CondBr(
                predicate,
                then_state.block,
                then_arguments,
                else_state.block,
                else_arguments,
            ),
        )?;
        let retained_value =
            then_state
                .env
                .get(&retained)
                .cloned()
                .ok_or(LowerError::InvalidControlFlow {
                    span: operand.span(),
                    reason: "elvis true edge lost its retained operand",
                })?;
        let then_value = ExprValue {
            value: retained_value.value,
            integer: retained_value
                .integer
                .ok_or(LowerError::InvalidControlFlow {
                    span: operand.span(),
                    reason: "elvis retained operand has no integer type",
                })?,
        };
        let else_value = self.lower_expr(else_expression, &mut else_state)?;
        let (then_value, else_value, common) =
            self.common_branch_values(then_value, else_value, &then_state, &else_state)?;
        let result_key = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        then_state.env.insert(
            result_key,
            BoundValue {
                value: then_value.value,
                integer: Some(common),
            },
        );
        else_state.env.insert(
            result_key,
            BoundValue {
                value: else_value.value,
                integer: Some(common),
            },
        );
        let mut merge_keys = keys;
        merge_keys.push(result_key);
        let mut merge = self.new_block_from(&then_state, &merge_keys, BlockRole::Ordinary)?;
        self.branch(&then_state, merge.block, &merge_keys)?;
        self.branch(&else_state, merge.block, &merge_keys)?;
        let result = merge
            .env
            .remove(&result_key)
            .ok_or(LowerError::InvalidControlFlow {
                span: operand.span(),
                reason: "elvis merge lost its result",
            })?;
        merge.env.remove(&retained);
        *state = merge;
        Ok(ExprValue {
            value: result.value,
            integer: result.integer.ok_or(LowerError::InvalidControlFlow {
                span: operand.span(),
                reason: "elvis result has no integer type",
            })?,
        })
    }

    fn emit_call(
        &mut self,
        callee: &'a CHirExpr<'a>,
        arguments: &'a [&'a CHirExpr<'a>],
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        let direct =
            match callee {
                CHirExpr::Ident {
                    symbol,
                    spelling,
                    facts,
                    span: designator_span,
                } if facts.category() == ExprCategory::FunctionDesignator => {
                    let designator = self.unit.symbols().symbol(*symbol).map_err(|_| {
                        LowerError::UnknownSymbol {
                            symbol: *symbol,
                            span: *designator_span,
                        }
                    })?;
                    if designator.kind() != SymbolKind::Function
                        || designator.spelling() != spelling
                        || designator.ty() != facts.ty()
                    {
                        return Err(LowerError::UnsupportedExpression {
                            span: *designator_span,
                            feature: "function designator disagrees with its semantic identity",
                        });
                    }
                    Some(
                        self.functions
                            .get(symbol)
                            .ok_or(LowerError::UnknownSymbol {
                                symbol: *symbol,
                                span,
                            })?,
                    )
                }
                _ => None,
            };
        let function_ty = self.call_designator_function_type(callee).ok_or(
            LowerError::UnsupportedExpression {
                span: callee.span(),
                feature: "indirect call designator has no function signature",
            },
        )?;
        let (mut parameter_types, return_type, direct_name) = if let Some(function) = direct {
            (
                function.parameter_types.clone(),
                function.return_type.clone(),
                Some(function.name.clone()),
            )
        } else {
            let (parameters, returned) = self.scalar_call_signature(function_ty, span)?;
            (parameters, returned, None)
        };
        let variadic = matches!(
            self.unit.types().get(function_ty).map(|ty| &ty.kind),
            Some(TypeKind::Function {
                parameters: FunctionParameters::Prototype { variadic: true, .. },
                ..
            })
        );
        if variadic {
            for argument in arguments.iter().skip(parameter_types.len()) {
                let CHirExpr::Cast(ty, _, _) = argument else {
                    return Err(LowerError::UnsupportedExpression {
                        span: argument.span(),
                        feature: "anonymous argument lacks default promotion",
                    });
                };
                if self.is_aggregate_type(*ty) {
                    return Err(LowerError::VarArgs {
                        span: argument.span(),
                    });
                }
                parameter_types.push(function_value_type(
                    self.unit.types(),
                    self.target,
                    *ty,
                    argument.span(),
                )?);
            }
        }
        if arguments.len() != parameter_types.len() {
            return Err(LowerError::MalformedFunction {
                function: direct_name
                    .clone()
                    .unwrap_or_else(|| "<indirect>".to_owned()),
                span,
                reason: "call arity disagrees with the canonical prototype",
            });
        }
        let callee_key = if direct_name.is_none() {
            let lowered = self.lower_expr(callee, state)?;
            let expected = self.fnptr_type(function_ty, span)?;
            if lowered.value.ty != expected {
                return Err(LowerError::UnsupportedExpression {
                    span: callee.span(),
                    feature: "indirect callee value disagrees with its fnptr signature",
                });
            }
            let key = EnvKey::Temporary(self.next_temporary);
            self.next_temporary += 1;
            state.env.insert(
                key,
                BoundValue {
                    value: lowered.value,
                    integer: None,
                },
            );
            Some(key)
        } else {
            None
        };
        let mut argument_keys = Vec::new();
        for (argument, expected) in arguments.iter().zip(&parameter_types) {
            let value = if self.is_aggregate_type(expected.source) {
                let source = self.lower_expr(argument, state)?;
                let origin = self.synthetic_origin()?;
                let destination =
                    self.emit_alloca(expected.source, argument.span(), origin, state)?;
                self.emit_aggregate_transfer(
                    destination.clone(),
                    source.value,
                    expected.source,
                    self.aggregate_source_is_volatile(argument),
                    argument.span(),
                    state,
                )?;
                ExprValue {
                    value: destination,
                    integer: CIntegerType::ULONG,
                }
            } else {
                let value = self.lower_expr(argument, state)?;
                if matches!(expected.ir, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }) {
                    self.coerce_to_object_type(
                        value,
                        Some(argument),
                        expected.source,
                        argument.span(),
                        state,
                    )?
                } else {
                    self.cast(value, expected.integer, state)?
                }
            };
            let key = EnvKey::Temporary(self.next_temporary);
            self.next_temporary += 1;
            state.env.insert(
                key,
                BoundValue {
                    value: value.value,
                    integer: (!self.is_aggregate_type(expected.source)).then_some(expected.integer),
                },
            );
            argument_keys.push(key);
        }
        let mut args = argument_keys
            .into_iter()
            .map(|key| {
                state.env.remove(&key).map(|bound| bound.value).ok_or(
                    LowerError::InvalidControlFlow {
                        span,
                        reason: "call argument control flow lost an evaluated argument",
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let aggregate_return = if return_type
            .as_ref()
            .is_some_and(|value_type| self.is_aggregate_type(value_type.source))
        {
            let value_type = return_type
                .as_ref()
                .expect("aggregate return predicate requires a return type");
            let origin = self.synthetic_origin()?;
            let destination = self.emit_alloca(value_type.source, span, origin, state)?;
            args.push(destination.clone());
            Some(destination)
        } else {
            None
        };
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mut results = Vec::new();
        if let Some(value_type) = return_type
            .as_ref()
            .filter(|value_type| !self.is_aggregate_type(value_type.source))
        {
            results.push(self.fresh_value(value_type.ir.clone()));
        }
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        let call = if let Some(callee) = direct_name {
            Inst::Call {
                results: results.clone(),
                callee,
                args,
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            }
        } else {
            let callee = state
                .env
                .remove(&callee_key.expect("indirect call inserted its callee temporary"))
                .ok_or(LowerError::InvalidControlFlow {
                    span,
                    reason: "indirect call control flow lost its callee",
                })?
                .value;
            Inst::IndirectCall {
                results: results.clone(),
                callee,
                args,
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            }
        };
        self.emit(state, call)?;
        state.env.insert(
            EnvKey::Memory,
            BoundValue {
                value: mem_out,
                integer: None,
            },
        );
        state.env.insert(
            EnvKey::World,
            BoundValue {
                value: world_out,
                integer: None,
            },
        );
        Ok(return_type.map(|value_type| {
            if self.is_aggregate_type(value_type.source) {
                ExprValue {
                    value: aggregate_return
                        .expect("aggregate-returning call allocated its result object"),
                    integer: CIntegerType::ULONG,
                }
            } else {
                ExprValue {
                    value: results.remove(0),
                    integer: value_type.integer,
                }
            }
        }))
    }

    fn lower_discarded_expr(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<(), LowerError> {
        match expression {
            CHirExpr::BuiltinVarArgs {
                op,
                first,
                second,
                requested_ty,
                span,
                ..
            } => {
                self.lower_varargs(*op, first, *second, *requested_ty, *span, state)?;
            }
            CHirExpr::Call(callee, arguments, span) => {
                let _ = self.emit_call(callee, arguments, *span, state)?;
            }
            CHirExpr::Comma(left, right, _) => {
                self.lower_discarded_expr(left, state)?;
                self.lower_discarded_expr(right, state)?;
            }
            CHirExpr::StatementExpr {
                body,
                value: None,
                facts,
                span,
            } => {
                let result = self.lower_statement_expression(body, None, *facts, *span, state)?;
                debug_assert!(result.is_none());
            }
            _ => {
                let _ = self.lower_expr(expression, state)?;
            }
        }
        Ok(())
    }

    fn lower_varargs(
        &mut self,
        op: evocc_chir::VarArgsBuiltinOp,
        first: &'a CHirExpr<'a>,
        second: Option<&'a CHirExpr<'a>>,
        requested_ty: Option<TypeId>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        use evocc_chir::VarArgsBuiltinOp;
        use evocc_evoir::{VariadicAccess, VariadicOp};
        let list = self.lower_expr(first, state)?.value;
        if op == VarArgsBuiltinOp::Copy {
            let source = second.ok_or(LowerError::VarArgs { span })?;
            let source = self.lower_expr(source, state)?.value;
            let mem_in = self.token(state, EnvKey::Memory)?.value;
            let world_in = self.token(state, EnvKey::World)?.value;
            let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
            let world_out = self.fresh_value(EvoIRType::World);
            self.emit(
                state,
                Inst::Copy {
                    destination: list,
                    source,
                    mem_in,
                    world_in,
                    mem_out: mem_out.clone(),
                    world_out: world_out.clone(),
                    layout: AccessLayout {
                        size_bytes: 32,
                        align_bytes: 8,
                    },
                },
            )?;
            self.replace_effect_tokens(state, mem_out, Some(world_out));
            return Ok(None);
        }
        let result = if op == VarArgsBuiltinOp::Arg {
            let ty = requested_ty.ok_or(LowerError::VarArgs { span })?;
            if self.is_aggregate_type(ty) {
                return Err(LowerError::VarArgs { span });
            }
            let value_type = function_value_type(self.unit.types(), self.target, ty, span)?;
            Some(ExprValue {
                value: self.fresh_value(value_type.ir),
                integer: value_type.integer,
            })
        } else {
            None
        };
        let op = match op {
            VarArgsBuiltinOp::Start => VariadicOp::Start,
            VarArgsBuiltinOp::End => VariadicOp::End,
            VarArgsBuiltinOp::Arg => VariadicOp::Arg(result.as_ref().unwrap().value.clone()),
            VarArgsBuiltinOp::Copy => unreachable!("copy returned above"),
        };
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::VaAccess(VariadicAccess {
                op,
                list,
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            }),
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(result)
    }

    fn lower_assignment(
        &mut self,
        operator: AssignmentOp,
        place: ScalarPlace,
        target: &'a CHirExpr<'a>,
        assigned: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        self.validate_place(place, target, span)?;
        if operator != AssignmentOp::Simple && self.is_function_pointer_type(place.ty()) {
            return Err(LowerError::FunctionPointerArithmetic { span });
        }
        if self.memory_symbols.contains(&place.symbol()) {
            let address = self.object_address(place.symbol(), span, state)?;
            let volatile = self
                .unit
                .types()
                .get(place.ty())
                .is_some_and(|ty| ty.qualifiers.is_volatile);
            let original = if operator == AssignmentOp::Simple {
                None
            } else {
                Some(self.emit_memory_load(address.clone(), place.ty(), volatile, span, state)?)
            };
            let right = self.lower_expr(assigned, state)?;
            let result = if operator == AssignmentOp::Simple {
                self.coerce_to_object_type(right, Some(assigned), place.ty(), span, state)?
            } else {
                let original = original.expect("compound assignment loaded its object");
                if matches!(original.value.ty, EvoIRType::Ptr(_)) {
                    self.compute_pointer_assignment(
                        operator,
                        original,
                        right,
                        place.ty(),
                        span,
                        state,
                    )?
                } else {
                    let destination =
                        canonical_integer(self.unit.types(), self.target, place.ty(), span)?;
                    self.compute_compound_assignment(
                        operator,
                        original,
                        right,
                        destination,
                        span,
                        state,
                    )?
                }
            };
            return self.emit_memory_store(address, result, place.ty(), volatile, span, state);
        }
        if matches!(
            self.unit.types().get(place.ty()).map(|ty| &ty.kind),
            Some(TypeKind::Pointer(_))
        ) {
            let right = self.lower_expr(assigned, state)?;
            let result = if operator == AssignmentOp::Simple {
                self.coerce_to_object_type(right, Some(assigned), place.ty(), span, state)?
            } else {
                let original = self.local(state, place.symbol(), span)?;
                self.compute_pointer_assignment(operator, original, right, place.ty(), span, state)?
            };
            state.env.insert(
                EnvKey::Local(place.symbol()),
                BoundValue {
                    value: result.value.clone(),
                    integer: Some(CIntegerType::ULONG),
                },
            );
            return Ok(result);
        }
        let destination = canonical_integer(self.unit.types(), self.target, place.ty(), span)?;
        let right = self.lower_expr(assigned, state)?;
        let result = if operator == AssignmentOp::Simple {
            self.cast(right, destination, state)?
        } else {
            let left = self.local(state, place.symbol(), span)?;
            let computed = if matches!(operator, AssignmentOp::ShiftLeft | AssignmentOp::ShiftRight)
            {
                let shifted_type = integer_promotion(left.integer);
                let amount_type = integer_promotion(right.integer);
                let left = self.cast(left, shifted_type, state)?;
                let right = self.cast(right, amount_type, state)?;
                self.emit_shift(
                    state,
                    if operator == AssignmentOp::ShiftLeft {
                        BinaryOp::Shl
                    } else {
                        BinaryOp::Shr
                    },
                    left,
                    right,
                    shifted_type,
                )?
            } else {
                let common = arithmetic_common_type(left.integer, right.integer);
                let left = self.cast(left, common, state)?;
                let right = self.cast(right, common, state)?;
                match operator {
                    AssignmentOp::Add => {
                        self.emit_signed_arithmetic(state, BinaryOp::Add, left, right, common)?
                    }
                    AssignmentOp::Subtract => {
                        self.emit_signed_arithmetic(state, BinaryOp::Sub, left, right, common)?
                    }
                    AssignmentOp::Multiply => {
                        self.emit_signed_arithmetic(state, BinaryOp::Mul, left, right, common)?
                    }
                    AssignmentOp::Divide => {
                        self.emit_div_rem(state, BinaryOp::Div, left, right, common)?
                    }
                    AssignmentOp::Remainder => {
                        self.emit_div_rem(state, BinaryOp::Mod, left, right, common)?
                    }
                    AssignmentOp::BitAnd | AssignmentOp::BitOr | AssignmentOp::BitXor => {
                        let result = self.fresh_value(bits(common));
                        let instruction = match operator {
                            AssignmentOp::BitAnd => {
                                Inst::And(result.clone(), left.value, right.value)
                            }
                            AssignmentOp::BitOr => {
                                Inst::Or(result.clone(), left.value, right.value)
                            }
                            AssignmentOp::BitXor => {
                                Inst::Xor(result.clone(), left.value, right.value)
                            }
                            _ => {
                                return Err(LowerError::InvalidControlFlow {
                                    span,
                                    reason:
                                        "non-bitwise compound assignment reached bitwise emission",
                                });
                            }
                        };
                        self.emit(state, instruction)?;
                        ExprValue {
                            value: result,
                            integer: common,
                        }
                    }
                    AssignmentOp::ShiftLeft | AssignmentOp::ShiftRight => {
                        return Err(LowerError::InvalidControlFlow {
                            span,
                            reason: "shift assignment bypassed shift emission",
                        });
                    }
                    AssignmentOp::Simple => {
                        return Err(LowerError::InvalidControlFlow {
                            span,
                            reason: "simple assignment reached compound-assignment emission",
                        });
                    }
                }
            };
            self.cast(computed, destination, state)?
        };
        state.env.insert(
            EnvKey::Local(place.symbol()),
            BoundValue {
                value: result.value.clone(),
                integer: Some(destination),
            },
        );
        Ok(result)
    }

    fn lower_update(
        &mut self,
        operator: UpdateOp,
        place: ScalarPlace,
        target: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        self.validate_place(place, target, span)?;
        if self.is_function_pointer_type(place.ty()) {
            return Err(LowerError::FunctionPointerArithmetic { span });
        }
        let original = self.local(state, place.symbol(), span)?;
        if matches!(original.value.ty, EvoIRType::Ptr(_)) {
            let one = self.constant(state, CIntegerType::INT, 1)?;
            let updated = self.compute_pointer_assignment(
                if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement) {
                    AssignmentOp::Add
                } else {
                    AssignmentOp::Subtract
                },
                original.clone(),
                one,
                place.ty(),
                span,
                state,
            )?;
            state.env.insert(
                EnvKey::Local(place.symbol()),
                BoundValue {
                    value: updated.value.clone(),
                    integer: Some(CIntegerType::ULONG),
                },
            );
            return Ok(
                if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                    updated
                } else {
                    original
                },
            );
        }
        let promoted_type = integer_promotion(original.integer);
        let promoted = self.cast(original.clone(), promoted_type, state)?;
        let one = self.constant(state, promoted_type, 1)?;
        let computed = self.emit_signed_arithmetic(
            state,
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            },
            promoted,
            one,
            promoted_type,
        )?;
        let updated = self.cast(computed, original.integer, state)?;
        state.env.insert(
            EnvKey::Local(place.symbol()),
            BoundValue {
                value: updated.value.clone(),
                integer: Some(updated.integer),
            },
        );
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                updated
            } else {
                original
            },
        )
    }

    fn compute_compound_assignment(
        &mut self,
        operator: AssignmentOp,
        left: ExprValue,
        right: ExprValue,
        destination: CIntegerType,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let computed = if matches!(operator, AssignmentOp::ShiftLeft | AssignmentOp::ShiftRight) {
            let shifted_type = integer_promotion(left.integer);
            let amount_type = integer_promotion(right.integer);
            let left = self.cast(left, shifted_type, state)?;
            let right = self.cast(right, amount_type, state)?;
            self.emit_shift(
                state,
                if operator == AssignmentOp::ShiftLeft {
                    BinaryOp::Shl
                } else {
                    BinaryOp::Shr
                },
                left,
                right,
                shifted_type,
            )?
        } else {
            let common = arithmetic_common_type(left.integer, right.integer);
            let left = self.cast(left, common, state)?;
            let right = self.cast(right, common, state)?;
            match operator {
                AssignmentOp::Add => {
                    self.emit_signed_arithmetic(state, BinaryOp::Add, left, right, common)?
                }
                AssignmentOp::Subtract => {
                    self.emit_signed_arithmetic(state, BinaryOp::Sub, left, right, common)?
                }
                AssignmentOp::Multiply => {
                    self.emit_signed_arithmetic(state, BinaryOp::Mul, left, right, common)?
                }
                AssignmentOp::Divide => {
                    self.emit_div_rem(state, BinaryOp::Div, left, right, common)?
                }
                AssignmentOp::Remainder => {
                    self.emit_div_rem(state, BinaryOp::Mod, left, right, common)?
                }
                AssignmentOp::BitAnd | AssignmentOp::BitOr | AssignmentOp::BitXor => {
                    let result = self.fresh_value(bits(common));
                    let instruction = match operator {
                        AssignmentOp::BitAnd => Inst::And(result.clone(), left.value, right.value),
                        AssignmentOp::BitOr => Inst::Or(result.clone(), left.value, right.value),
                        AssignmentOp::BitXor => Inst::Xor(result.clone(), left.value, right.value),
                        _ => unreachable!(),
                    };
                    self.emit(state, instruction)?;
                    ExprValue {
                        value: result,
                        integer: common,
                    }
                }
                AssignmentOp::Simple | AssignmentOp::ShiftLeft | AssignmentOp::ShiftRight => {
                    return Err(LowerError::InvalidControlFlow {
                        span,
                        reason: "invalid assignment operator reached compound computation",
                    });
                }
            }
        };
        self.cast(computed, destination, state)
    }

    fn compute_pointer_assignment(
        &mut self,
        operator: AssignmentOp,
        pointer: ExprValue,
        index: ExprValue,
        pointer_ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if self.is_function_pointer_type(pointer_ty) {
            return Err(LowerError::FunctionPointerArithmetic { span });
        }
        if !matches!(pointer.value.ty, EvoIRType::Ptr(_))
            || !matches!(index.value.ty, EvoIRType::Bits(_))
        {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "pointer assignment operands disagree with C-HIR types",
            });
        }
        let pointee = match self.unit.types().get(pointer_ty).map(|ty| &ty.kind) {
            Some(TypeKind::Pointer(pointee)) => *pointee,
            _ => {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "pointer assignment target is not a canonical pointer",
                });
            }
        };
        let direction = match operator {
            AssignmentOp::Add => OffsetDirection::Add,
            AssignmentOp::Subtract => OffsetDirection::Subtract,
            _ => {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "invalid compound pointer assignment operator",
                });
            }
        };
        let scale_bytes = self.pointer_arithmetic_scale(pointee, span)?;
        let promoted_type = integer_promotion(index.integer);
        let index = self.cast(index, promoted_type, state)?;
        let value = self.offset_address(
            pointer.value,
            index,
            direction,
            if promoted_type.is_signed() {
                IndexSignedness::Signed
            } else {
                IndexSignedness::Unsigned
            },
            scale_bytes,
            state,
        )?;
        Ok(ExprValue {
            value,
            integer: CIntegerType::ULONG,
        })
    }

    fn lower_memory_assignment(
        &mut self,
        operator: AssignmentOp,
        place: &'a MemoryPlace<'a>,
        target_span: SourceSpan,
        assigned: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if operator != AssignmentOp::Simple && self.is_function_pointer_type(place.value_ty()) {
            return Err(LowerError::FunctionPointerArithmetic { span });
        }
        let address = self.lower_place_address(place, target_span, state)?;
        let volatile = place.access_qualifiers().is_volatile;
        if self.is_aggregate_type(place.value_ty()) {
            if operator != AssignmentOp::Simple {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "aggregate compound assignment reached lowering",
                });
            }
            let source = self.lower_expr(assigned, state)?;
            self.emit_aggregate_transfer(
                address.clone(),
                source.value,
                place.value_ty(),
                volatile || self.aggregate_source_is_volatile(assigned),
                span,
                state,
            )?;
            return Ok(ExprValue {
                value: address,
                integer: CIntegerType::ULONG,
            });
        }
        let original = if operator == AssignmentOp::Simple {
            None
        } else {
            Some(self.emit_memory_load(
                address.clone(),
                place.value_ty(),
                volatile,
                target_span,
                state,
            )?)
        };
        let right = self.lower_expr(assigned, state)?;
        let result = if operator == AssignmentOp::Simple {
            self.coerce_to_object_type(right, Some(assigned), place.value_ty(), span, state)?
        } else {
            let original = original.expect("memory compound assignment loaded its left value");
            if matches!(original.value.ty, EvoIRType::Ptr(_)) {
                self.compute_pointer_assignment(
                    operator,
                    original,
                    right,
                    place.value_ty(),
                    span,
                    state,
                )?
            } else {
                let destination =
                    canonical_integer(self.unit.types(), self.target, place.value_ty(), span)?;
                self.compute_compound_assignment(
                    operator,
                    original,
                    right,
                    destination,
                    span,
                    state,
                )?
            }
        };
        self.emit_memory_store(
            address,
            result,
            place.value_ty(),
            volatile,
            target_span,
            state,
        )
    }

    fn lower_memory_update(
        &mut self,
        operator: UpdateOp,
        place: &'a MemoryPlace<'a>,
        target_span: SourceSpan,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if self.is_function_pointer_type(place.value_ty()) {
            return Err(LowerError::FunctionPointerArithmetic { span });
        }
        let address = self.lower_place_address(place, target_span, state)?;
        let original = self.emit_memory_load(
            address.clone(),
            place.value_ty(),
            place.access_qualifiers().is_volatile,
            target_span,
            state,
        )?;
        if matches!(original.value.ty, EvoIRType::Ptr(_)) {
            let one = self.constant(state, CIntegerType::INT, 1)?;
            let updated = self.compute_pointer_assignment(
                if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement) {
                    AssignmentOp::Add
                } else {
                    AssignmentOp::Subtract
                },
                original.clone(),
                one,
                place.value_ty(),
                span,
                state,
            )?;
            self.emit_memory_store(
                address,
                updated.clone(),
                place.value_ty(),
                place.access_qualifiers().is_volatile,
                target_span,
                state,
            )?;
            return Ok(
                if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                    updated
                } else {
                    original
                },
            );
        }
        let promoted_type = integer_promotion(original.integer);
        let promoted = self.cast(original.clone(), promoted_type, state)?;
        let one = self.constant(state, promoted_type, 1)?;
        let computed = self.emit_signed_arithmetic(
            state,
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            },
            promoted,
            one,
            promoted_type,
        )?;
        let destination =
            canonical_integer(self.unit.types(), self.target, place.value_ty(), span)?;
        let updated = self.cast(computed, destination, state)?;
        self.emit_memory_store(
            address,
            updated.clone(),
            place.value_ty(),
            place.access_qualifiers().is_volatile,
            target_span,
            state,
        )?;
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                updated
            } else {
                original
            },
        )
    }

    fn lower_condition(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let value = self.lower_expr(expression, state)?;
        self.lower_condition_value(value, state)
    }

    fn lower_condition_value(
        &mut self,
        value: ExprValue,
        state: &State,
    ) -> Result<Value, LowerError> {
        if value.value.ty == EvoIRType::Bits(1) {
            return Ok(value.value);
        }
        if matches!(value.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }) {
            let null = self.fresh_value(value.value.ty.clone());
            self.emit(state, Inst::PtrNull(null.clone()))?;
            let condition = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::ICmp(condition.clone(), CmpOp::Ne, value.value, null),
            )?;
            return Ok(condition);
        }
        let zero = self.constant(state, value.integer, 0)?;
        let condition = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::ICmp(condition.clone(), CmpOp::Ne, value.value, zero.value),
        )?;
        Ok(condition)
    }

    fn cast(
        &mut self,
        value: ExprValue,
        target: CIntegerType,
        state: &State,
    ) -> Result<ExprValue, LowerError> {
        if target == CIntegerType::BOOL {
            let condition = self.lower_condition_value(value, state)?;
            return Ok(ExprValue {
                value: condition,
                integer: target,
            });
        }
        if matches!(value.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }) {
            let result = self.fresh_value(EvoIRType::Bits(64));
            self.emit(
                state,
                Inst::PointerCast {
                    result: result.clone(),
                    operand: value.value,
                },
            )?;
            return self.cast(
                ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                },
                target,
                state,
            );
        }
        let EvoIRType::Bits(source_width) = value.value.ty else {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "integer expression has a non-bit-vector EvoIR value",
            });
        };
        let target_width = u16::from(target.bits());
        if source_width == target_width {
            return Ok(ExprValue {
                value: value.value,
                integer: target,
            });
        }
        let result = self.fresh_value(bits(target));
        let instruction = if source_width < target_width {
            // C comparison, logical, and `!` expressions have type `int`, but
            // EvoIR keeps their canonical truth value in `bits<1>`.  That
            // representation is always zero-extended; treating it as a
            // one-bit signed integer would turn true into -1.
            if value.integer.is_signed()
                && !(source_width == 1 && value.integer == CIntegerType::INT)
            {
                Inst::SExt(result.clone(), value.value, bits(target))
            } else {
                Inst::ZExt(result.clone(), value.value, bits(target))
            }
        } else {
            Inst::Trunc(result.clone(), value.value, bits(target))
        };
        self.emit(state, instruction)?;
        Ok(ExprValue {
            value: result,
            integer: target,
        })
    }

    fn constant(
        &mut self,
        state: &State,
        integer: CIntegerType,
        raw: u64,
    ) -> Result<ExprValue, LowerError> {
        self.wide_constant(state, integer, u128::from(raw))
    }

    /// Keep the instruction's existing 64-bit immediate representation. A wide
    /// constant is composed from its two halves using ordinary bit operations.
    fn wide_constant(
        &mut self,
        state: &State,
        integer: CIntegerType,
        raw: u128,
    ) -> Result<ExprValue, LowerError> {
        let raw = raw & integer.mask();
        let result = self.fresh_value(bits(integer));
        self.emit(state, Inst::Const(result.clone(), raw as u64))?;
        let result = if raw >> 64 != 0 {
            let high = self.fresh_value(bits(integer));
            let amount = self.fresh_value(EvoIRType::Bits(32));
            let shifted = self.fresh_value(bits(integer));
            let joined = self.fresh_value(bits(integer));
            self.emit(state, Inst::Const(high.clone(), (raw >> 64) as u64))?;
            self.emit(state, Inst::Const(amount.clone(), 64))?;
            self.emit(state, Inst::Shl(shifted.clone(), high, amount))?;
            self.emit(state, Inst::Or(joined.clone(), result, shifted))?;
            joined
        } else {
            result
        };
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    fn validate_place(
        &self,
        place: ScalarPlace,
        target: &CHirExpr<'a>,
        span: SourceSpan,
    ) -> Result<(), LowerError> {
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span: target_span,
        } = target
        else {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "assignment target is not a scalar identifier place",
            });
        };
        let declaration =
            self.unit
                .symbols()
                .symbol(*symbol)
                .map_err(|_| LowerError::UnknownSymbol {
                    symbol: *symbol,
                    span: *target_span,
                })?;
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
        {
            return Err(LowerError::UnsupportedExpression {
                span: *target_span,
                feature: "scalar place disagrees with its semantic identity",
            });
        }
        Ok(())
    }

    fn local(
        &mut self,
        state: &State,
        symbol: SymbolId,
        span: SourceSpan,
    ) -> Result<ExprValue, LowerError> {
        let Some(bound) = state.env.get(&EnvKey::Local(symbol)) else {
            let declaration = self
                .unit
                .symbols()
                .symbol(symbol)
                .map_err(|_| LowerError::UnknownSymbol { symbol, span })?;
            let (ty, integer) = self.c_value_type(declaration.ty(), span)?;
            let value = self.fresh_value(ty);
            self.emit(state, Inst::Uninitialized(value.clone()))?;
            return Ok(ExprValue { value, integer });
        };
        Ok(ExprValue {
            value: bound.value.clone(),
            integer: bound.integer.ok_or(LowerError::UnsupportedExpression {
                span,
                feature: "local does not carry an integer type",
            })?,
        })
    }

    fn token(&self, state: &State, key: EnvKey) -> Result<BoundValue, LowerError> {
        state
            .env
            .get(&key)
            .cloned()
            .ok_or(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "call participant has no current effect token",
            })
    }

    fn replace_target(&mut self, old: BlockId, new: BlockId) {
        for block in &mut self.blocks {
            if let Some(terminator) = &mut block.terminator {
                replace_inst_target(terminator, old, new);
            }
        }
    }

    fn move_block_before(&mut self, block_id: BlockId, before: usize) -> Result<(), LowerError> {
        let position = self
            .blocks
            .iter()
            .position(|block| block.id == block_id)
            .ok_or(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "loop step block disappeared",
            })?;
        if before < position {
            let block = self.blocks.remove(position);
            self.blocks.insert(before, block);
        }
        Ok(())
    }

    fn eliminate_switch_forwarders(&mut self) -> Result<(), LowerError> {
        while let Some(index) = self.blocks.iter().position(|block| {
            if block.role != BlockRole::SwitchExit || !block.insts.is_empty() {
                return false;
            }
            let Some(Inst::Br(_, arguments)) = &block.terminator else {
                return false;
            };
            block.args.len() == arguments.len()
                && block
                    .args
                    .iter()
                    .zip(arguments)
                    .all(|((_, argument), value)| argument.value == *value)
        }) {
            let block = &self.blocks[index];
            let target = match &block.terminator {
                Some(Inst::Br(target, _)) => *target,
                _ => {
                    return Err(LowerError::InvalidControlFlow {
                        span: self.info.span,
                        reason: "switch forwarding block changed during elimination",
                    });
                }
            };
            let old = block.id;
            for predecessor in &mut self.blocks {
                let Some(terminator) = &mut predecessor.terminator else {
                    continue;
                };
                replace_inst_target(terminator, old, target);
            }
            self.blocks.remove(index);
        }
        Ok(())
    }

    fn finish_blocks(self) -> Result<Vec<Block>, LowerError> {
        self.blocks
            .into_iter()
            .map(|block| {
                Ok(Block {
                    id: block.id,
                    args: block
                        .args
                        .into_iter()
                        .map(|(_, argument)| argument)
                        .collect(),
                    insts: block.insts,
                    terminator: block.terminator.ok_or(LowerError::InvalidControlFlow {
                        span: self.info.span,
                        reason: "draft block lacks a terminator",
                    })?,
                })
            })
            .collect()
    }
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

fn switch_segments<'a>(
    body: &'a CHirStmt<'a>,
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    selector_type: CIntegerType,
) -> Result<(&'a [CHirBlockItem<'a>], Vec<SwitchSegment<'a>>), LowerError> {
    let CHirStmt::Compound(items, _) = body else {
        return Err(LowerError::UnsupportedStatement {
            span: body.span(),
            feature: "switch body must be a compound statement",
        });
    };
    let mut segments: Vec<SwitchSegment<'a>> = Vec::new();
    let mut prefix_end = items.len();
    for (index, item) in items.iter().enumerate() {
        let CHirBlockItem::Statement(statement) = item else {
            let Some(segment) = segments.last_mut() else {
                // Ordinary switch dispatch jumps directly to a case/default;
                // declarations before the first label establish scope but do
                // not execute their initializer.
                continue;
            };
            let CHirBlockItem::Declaration(declaration) = item else {
                return Err(LowerError::InvalidControlFlow {
                    span: body.span(),
                    reason: "switch block item has an unknown form",
                });
            };
            segment
                .items
                .push(SwitchSegmentItem::Declaration(declaration));
            continue;
        };
        match statement {
            CHirStmt::Case(..) | CHirStmt::CaseRange(..) | CHirStmt::Default(..) => {
                if segments.is_empty() {
                    prefix_end = index;
                }
                let (labels, nested) = switch_label_chain(statement, unit, target, selector_type);
                let label_count = labels.len();
                for (label_index, case) in labels.into_iter().enumerate() {
                    let items = if label_index + 1 == label_count {
                        vec![SwitchSegmentItem::Statement(nested)]
                    } else {
                        Vec::new()
                    };
                    segments.push(SwitchSegment { case, items });
                }
            }
            other => {
                let Some(segment) = segments.last_mut() else {
                    // Statements before the first case/default are bypassed by
                    // normal switch dispatch. A separately targeted ordinary
                    // label is materialized by the detached-goto path.
                    continue;
                };
                segment.items.push(SwitchSegmentItem::Statement(other));
            }
        }
    }
    Ok((&items[..prefix_end], segments))
}

fn switch_label_chain<'a>(
    statement: &'a CHirStmt<'a>,
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    selector_type: CIntegerType,
) -> (Vec<Option<SwitchCase<'a>>>, &'a CHirStmt<'a>) {
    let mut labels = Vec::new();
    let mut nested = statement;
    loop {
        match nested {
            CHirStmt::Case(value, body, _) => {
                labels.push(Some(SwitchCase::Single(value)));
                nested = body;
            }
            CHirStmt::CaseRange(first, last, body, _) => {
                labels.push(Some(SwitchCase::Range(first, last)));
                nested = body;
            }
            CHirStmt::Default(body, _) => {
                labels.push(None);
                nested = body;
            }
            _ => break,
        }
    }

    let mut coalesced = Vec::with_capacity(labels.len());
    let mut index = 0;
    while index < labels.len() {
        let Some(SwitchCase::Single(first)) = labels[index] else {
            coalesced.push(labels[index]);
            index += 1;
            continue;
        };
        let mut last = first;
        let mut last_value = normalized_case_value(unit, target, first, selector_type);
        let mut end = index + 1;
        while end < labels.len() {
            let Some(SwitchCase::Single(candidate)) = labels[end] else {
                break;
            };
            let candidate_value = normalized_case_value(unit, target, candidate, selector_type);
            if last_value.and_then(|value| value.checked_add(1)) != candidate_value {
                break;
            }
            last = candidate;
            last_value = candidate_value;
            end += 1;
        }
        if end == index + 1 {
            coalesced.push(Some(SwitchCase::Single(first)));
        } else {
            coalesced.push(Some(SwitchCase::Range(first, last)));
        }
        index = end;
    }
    (coalesced, nested)
}

fn normalized_case_value(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
    selector_type: CIntegerType,
) -> Option<i128> {
    constant_integer(unit, target, expression)
        .map(|value| value.cast(selector_type).mathematical_value())
}

fn collect_symbols_switch_item(item: &SwitchSegmentItem<'_>, symbols: &mut BTreeSet<SymbolId>) {
    match item {
        SwitchSegmentItem::Declaration(declaration) => {
            collect_symbols_item(&CHirBlockItem::Declaration(declaration), symbols);
        }
        SwitchSegmentItem::Statement(statement) => collect_symbols_stmt(statement, symbols),
    }
}

fn bits(integer: CIntegerType) -> EvoIRType {
    EvoIRType::Bits(u16::from(integer.bits()))
}

fn constant_truth(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
) -> Option<bool> {
    constant_integer(unit, target, expression).map(CInteger::truth)
}

fn null_pointer_constant(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
) -> bool {
    if constant_integer(unit, target, expression).is_some_and(|value| !value.truth()) {
        return true;
    }
    let CHirExpr::Cast(pointer, operand, _) = expression else {
        return false;
    };
    let Some(TypeKind::Pointer(pointee)) = unit.types().get(*pointer).map(|ty| &ty.kind) else {
        return false;
    };
    if !matches!(
        unit.types().get(*pointee).map(|ty| &ty.kind),
        Some(TypeKind::Void)
    ) {
        return false;
    }
    constant_integer(unit, target, operand).is_some_and(|value| !value.truth())
}

fn constant_integer(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
) -> Option<CInteger> {
    match expression {
        CHirExpr::Lit(literal, _) => integer_literal_value(*literal).ok(),
        CHirExpr::CharLit(literal, _) => {
            Some(CInteger::new(u64::from(literal.value), CIntegerType::INT))
        }
        CHirExpr::Ident {
            symbol,
            facts,
            span,
            ..
        } => {
            let declaration = unit.symbols().symbol(*symbol).ok()?;
            if declaration.kind() != SymbolKind::Enumerator {
                return None;
            }
            let integer = canonical_integer(unit.types(), target, facts.ty(), *span).ok()?;
            Some(CInteger::new(
                declaration.enumerator_value()? as u64,
                integer,
            ))
        }
        CHirExpr::Unary(operator, operand, _) => {
            eval_unary_integer(*operator, constant_integer(unit, target, operand)?).ok()
        }
        CHirExpr::Binary(BinaryOp::And, left, right, _) => {
            let left = constant_integer(unit, target, left)?;
            if !left.truth() {
                Some(CInteger::int(0))
            } else {
                Some(CInteger::int(i32::from(
                    constant_integer(unit, target, right)?.truth(),
                )))
            }
        }
        CHirExpr::Binary(BinaryOp::Or, left, right, _) => {
            let left = constant_integer(unit, target, left)?;
            if left.truth() {
                Some(CInteger::int(1))
            } else {
                Some(CInteger::int(i32::from(
                    constant_integer(unit, target, right)?.truth(),
                )))
            }
        }
        CHirExpr::Binary(operator, left, right, _) => eval_binary_integer(
            *operator,
            constant_integer(unit, target, left)?,
            constant_integer(unit, target, right)?,
        )
        .ok(),
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            if constant_integer(unit, target, condition)?.truth() {
                constant_integer(unit, target, then_value)
            } else {
                constant_integer(unit, target, else_value)
            }
        }
        CHirExpr::Cast(ty, operand, span) => {
            let target_integer = canonical_integer(unit.types(), target, *ty, *span).ok()?;
            Some(constant_integer(unit, target, operand)?.cast(target_integer))
        }
        CHirExpr::Comma(left, right, _) => {
            constant_integer(unit, target, left)?;
            constant_integer(unit, target, right)
        }
        _ => None,
    }
}

fn integer_promotion(integer: CIntegerType) -> CIntegerType {
    if integer.rank() < CIntegerType::INT.rank() {
        CIntegerType::INT
    } else {
        integer
    }
}

fn arithmetic_common_type(left: CIntegerType, right: CIntegerType) -> CIntegerType {
    usual_arithmetic_type(integer_promotion(left), integer_promotion(right))
}

fn contains_targeted_label(statement: &CHirStmt<'_>, names: &BTreeSet<String>) -> bool {
    match statement {
        CHirStmt::Label(label, nested, _) => {
            names.contains(&label.spelling) || contains_targeted_label(nested, names)
        }
        CHirStmt::If(_, then_branch, else_branch, _) => {
            contains_targeted_label(then_branch, names)
                || else_branch
                    .is_some_and(|branch| contains_targeted_label(branch, names))
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::Default(body, _) => contains_targeted_label(body, names),
        CHirStmt::For(_, _, _, body, _) => contains_targeted_label(body, names),
        CHirStmt::Compound(items, _) => items.iter().any(|item| {
            matches!(item, CHirBlockItem::Statement(statement) if contains_targeted_label(statement, names))
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

fn collect_goto_names(statement: &CHirStmt<'_>, names: &mut BTreeSet<String>) {
    match statement {
        CHirStmt::Asm(assembly) => {
            names.extend(assembly.labels.iter().map(|label| label.spelling.clone()));
        }
        CHirStmt::Goto(label, _) => {
            names.insert(label.spelling.clone());
        }
        CHirStmt::ComputedGoto(_, _) => {}
        CHirStmt::If(_, then_branch, else_branch, _) => {
            collect_goto_names(then_branch, names);
            if let Some(else_branch) = else_branch {
                collect_goto_names(else_branch, names);
            }
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::Label(_, body, _)
        | CHirStmt::Default(body, _) => collect_goto_names(body, names),
        CHirStmt::For(_, _, _, body, _) => collect_goto_names(body, names),
        CHirStmt::Compound(items, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    collect_goto_names(statement, names);
                }
            }
        }
        CHirStmt::Expr(_, _)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

#[derive(Default)]
struct CallSites {
    direct: BTreeSet<SymbolId>,
    any: bool,
}

fn collect_calls_stmt(statement: &CHirStmt<'_>, calls: &mut CallSites) {
    match statement {
        CHirStmt::Asm(assembly) => {
            calls.any = true;
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_calls_expr(operand.expression, calls);
            }
        }
        CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
            collect_calls_expr(expression, calls)
        }
        CHirStmt::ComputedGoto(expression, _) => collect_calls_expr(expression, calls),
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            collect_calls_expr(condition, calls);
            collect_calls_stmt(then_branch, calls);
            if let Some(branch) = else_branch {
                collect_calls_stmt(branch, calls);
            }
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            collect_calls_expr(expression, calls);
            collect_calls_stmt(body, calls);
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            collect_calls_expr(first, calls);
            collect_calls_expr(last, calls);
            collect_calls_stmt(body, calls);
        }
        CHirStmt::DoWhile(body, expression, _) => {
            collect_calls_stmt(body, calls);
            collect_calls_expr(expression, calls);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => collect_calls_expr(expression, calls),
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        collect_calls_decl(declaration, calls);
                    }
                }
            }
            if let Some(condition) = condition {
                collect_calls_expr(condition, calls);
            }
            if let Some(step) = step {
                collect_calls_expr(step, calls);
            }
            collect_calls_stmt(body, calls);
        }
        CHirStmt::Compound(items, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        collect_calls_decl(declaration, calls)
                    }
                    CHirBlockItem::Statement(statement) => collect_calls_stmt(statement, calls),
                }
            }
        }
        CHirStmt::Label(_, statement, _) | CHirStmt::Default(statement, _) => {
            collect_calls_stmt(statement, calls)
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

fn collect_calls_decl(declaration: &CHirDecl<'_>, calls: &mut CallSites) {
    if let CHirDecl::Object(object) = declaration {
        if let Some(initializer) = &object.initializer {
            for action in initializer.plan.actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    collect_calls_expr(value, calls);
                }
            }
        }
    }
}

fn collect_calls_expr(expression: &CHirExpr<'_>, calls: &mut CallSites) {
    match expression {
        CHirExpr::Call(callee, arguments, _) => {
            calls.any = true;
            if let CHirExpr::Ident { symbol, facts, .. } = callee {
                if facts.category() == ExprCategory::FunctionDesignator {
                    calls.direct.insert(*symbol);
                }
            }
            collect_calls_expr(callee, calls);
            for argument in arguments {
                collect_calls_expr(argument, calls);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            collect_calls_expr(lhs, calls);
            collect_calls_expr(rhs, calls);
            collect_calls_expr(target, calls);
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            collect_calls_expr(first, calls);
            if let Some(second) = second {
                collect_calls_expr(second, calls);
            }
        }
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => collect_calls_expr(operand, calls),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _) => {
            collect_calls_expr(left, calls);
            collect_calls_expr(right, calls);
        }
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            collect_calls_expr(condition, calls);
            collect_calls_expr(then_value, calls);
            collect_calls_expr(else_value, calls);
        }
        CHirExpr::Assign { target, value, .. } => {
            collect_calls_expr(target, calls);
            collect_calls_expr(value, calls);
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            for source in place.source_expressions() {
                collect_calls_expr(source, calls);
            }
            collect_calls_expr(value, calls);
        }
        CHirExpr::MemoryRead { place, .. } => {
            for source in place.source_expressions() {
                collect_calls_expr(source, calls);
            }
        }
        CHirExpr::Update { target, .. } => {
            collect_calls_expr(target, calls);
        }
        CHirExpr::MemoryUpdate { place, .. } => {
            for source in place.source_expressions() {
                collect_calls_expr(source, calls);
            }
        }
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    collect_calls_expr(value, calls);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => collect_calls_stmt(body, calls),
        CHirExpr::Ident { symbol, facts, .. } => {
            if facts.category() == ExprCategory::FunctionDesignator {
                calls.direct.insert(*symbol);
            }
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::SizeOfExpr(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => {}
    }
}

fn live_before_stmt(
    statement: &CHirStmt<'_>,
    mut live_after: BTreeSet<SymbolId>,
) -> BTreeSet<SymbolId> {
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly
                .inputs
                .iter()
                .rev()
                .chain(assembly.outputs.iter().rev())
            {
                live_before_expr(operand.expression, &mut live_after);
            }
        }
        CHirStmt::Expr(expression, _) => {
            live_before_expr(expression, &mut live_after);
        }
        CHirStmt::Return(expression, _) => {
            live_after.clear();
            if let Some(expression) = expression {
                live_before_expr(expression, &mut live_after);
            }
        }
        CHirStmt::ComputedGoto(expression, _) => {
            live_after.clear();
            live_before_expr(expression, &mut live_after);
        }
        CHirStmt::Compound(items, _) => {
            for item in items.iter().rev() {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        if let CHirDecl::Object(object) = declaration {
                            live_after.remove(&object.symbol);
                            if let Some(initializer) = &object.initializer {
                                for action in initializer.plan().actions() {
                                    if let InitAction::StoreScalar { value, .. }
                                    | InitAction::CopyAggregate { value, .. } = action
                                    {
                                        live_before_expr(value, &mut live_after);
                                    }
                                }
                            }
                        }
                    }
                    CHirBlockItem::Statement(statement) => {
                        live_after = live_before_stmt(statement, live_after);
                    }
                }
            }
        }
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            let then_live = live_before_stmt(then_branch, live_after.clone());
            let else_live = if let Some(branch) = else_branch {
                live_before_stmt(branch, live_after.clone())
            } else {
                live_after.clone()
            };
            live_after = then_live.union(&else_live).copied().collect();
            live_before_expr(condition, &mut live_after);
        }
        CHirStmt::While(condition, body, _) => {
            let exit_live = live_after.clone();
            loop {
                let mut before = live_before_stmt(body, live_after.clone());
                live_before_expr(condition, &mut before);
                before.extend(exit_live.iter().copied());
                if before == live_after {
                    break;
                }
                live_after = before;
            }
        }
        CHirStmt::DoWhile(body, condition, _) => {
            let exit_live = live_after.clone();
            loop {
                let mut condition_live = live_after.clone();
                live_before_expr(condition, &mut condition_live);
                let mut before = live_before_stmt(body, condition_live);
                before.extend(exit_live.iter().copied());
                if before == live_after {
                    break;
                }
                live_after = before;
            }
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            let exit_live = live_after.clone();
            loop {
                let mut iteration_live = live_after.clone();
                if let Some(step) = step {
                    live_before_expr(step, &mut iteration_live);
                }
                iteration_live = live_before_stmt(body, iteration_live);
                if let Some(condition) = condition {
                    live_before_expr(condition, &mut iteration_live);
                }
                iteration_live.extend(exit_live.iter().copied());
                if iteration_live == live_after {
                    break;
                }
                live_after = iteration_live;
            }
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => {
                    live_before_expr(expression, &mut live_after);
                }
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations.iter().rev() {
                        if let CHirDecl::Object(object) = declaration {
                            live_after.remove(&object.symbol);
                            if let Some(initializer) = &object.initializer {
                                for action in initializer.plan().actions() {
                                    if let InitAction::StoreScalar { value, .. }
                                    | InitAction::CopyAggregate { value, .. } = action
                                    {
                                        live_before_expr(value, &mut live_after);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        CHirStmt::Switch(expression, body, _) | CHirStmt::Case(expression, body, _) => {
            live_after = live_before_stmt(body, live_after);
            live_before_expr(expression, &mut live_after);
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            live_after = live_before_stmt(body, live_after);
            live_before_expr(last, &mut live_after);
            live_before_expr(first, &mut live_after);
        }
        CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => {
            live_after = live_before_stmt(body, live_after);
        }
        CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
    live_after
}

fn live_before_expr(expression: &CHirExpr<'_>, live: &mut BTreeSet<SymbolId>) {
    match expression {
        CHirExpr::Ident { symbol, .. } => {
            live.insert(*symbol);
        }
        CHirExpr::Assign {
            op, place, value, ..
        } => {
            if *op == AssignmentOp::Simple {
                live.remove(&place.symbol());
            } else {
                live.insert(place.symbol());
            }
            live_before_expr(value, live);
        }
        CHirExpr::Update { place, .. } => {
            live.insert(place.symbol());
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            live_before_expr(value, live);
            for source in place.source_expressions().rev() {
                live_before_expr(source, live);
            }
            if let Some(symbol) = place.root_symbol() {
                live.insert(symbol);
            }
        }
        CHirExpr::MemoryRead { place, .. } => {
            for source in place.source_expressions().rev() {
                live_before_expr(source, live);
            }
            if let Some(symbol) = place.root_symbol() {
                live.insert(symbol);
            }
        }
        CHirExpr::MemoryUpdate { place, .. } => {
            for source in place.source_expressions().rev() {
                live_before_expr(source, live);
            }
            if let Some(symbol) = place.root_symbol() {
                live.insert(symbol);
            }
        }
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => live_before_expr(operand, live),
        CHirExpr::Binary(BinaryOp::And | BinaryOp::Or, left, right, _) => {
            let mut right_live = live.clone();
            live_before_expr(right, &mut right_live);
            live.extend(right_live);
            live_before_expr(left, live);
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _) => {
            live_before_expr(right, live);
            live_before_expr(left, live);
        }
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            let mut then_live = live.clone();
            live_before_expr(then_value, &mut then_live);
            let mut else_live = live.clone();
            live_before_expr(else_value, &mut else_live);
            *live = then_live.union(&else_live).copied().collect();
            live_before_expr(condition, live);
        }
        CHirExpr::Call(callee, arguments, _) => {
            for argument in arguments.iter().rev() {
                live_before_expr(argument, live);
            }
            live_before_expr(callee, live);
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            live_before_expr(target, live);
            live_before_expr(rhs, live);
            live_before_expr(lhs, live);
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            if let Some(second) = second {
                live_before_expr(second, live);
            }
            live_before_expr(first, live);
        }
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions().iter().rev() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    live_before_expr(value, live);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => {
            *live = live_before_stmt(body, std::mem::take(live));
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::SizeOfExpr(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => {}
    }
}

fn collect_symbols_item(item: &CHirBlockItem<'_>, symbols: &mut BTreeSet<SymbolId>) {
    match item {
        CHirBlockItem::Declaration(declaration) => {
            if let CHirDecl::Object(object) = declaration {
                if let Some(initializer) = &object.initializer {
                    for action in initializer.plan.actions() {
                        if let InitAction::StoreScalar { value, .. }
                        | InitAction::CopyAggregate { value, .. } = action
                        {
                            collect_symbols_expr(value, symbols);
                        }
                    }
                }
            }
        }
        CHirBlockItem::Statement(statement) => collect_symbols_stmt(statement, symbols),
    }
}

fn collect_symbols_stmt(statement: &CHirStmt<'_>, symbols: &mut BTreeSet<SymbolId>) {
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_symbols_expr(operand.expression, symbols);
            }
        }
        CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
            collect_symbols_expr(expression, symbols)
        }
        CHirStmt::ComputedGoto(expression, _) => collect_symbols_expr(expression, symbols),
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            collect_symbols_expr(condition, symbols);
            collect_symbols_stmt(then_branch, symbols);
            if let Some(branch) = else_branch {
                collect_symbols_stmt(branch, symbols);
            }
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            collect_symbols_expr(expression, symbols);
            collect_symbols_stmt(body, symbols);
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            collect_symbols_expr(first, symbols);
            collect_symbols_expr(last, symbols);
            collect_symbols_stmt(body, symbols);
        }
        CHirStmt::DoWhile(body, expression, _) => {
            collect_symbols_stmt(body, symbols);
            collect_symbols_expr(expression, symbols);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => collect_symbols_expr(expression, symbols),
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        collect_symbols_item(&CHirBlockItem::Declaration(declaration), symbols);
                    }
                }
            }
            if let Some(condition) = condition {
                collect_symbols_expr(condition, symbols);
            }
            if let Some(step) = step {
                collect_symbols_expr(step, symbols);
            }
            collect_symbols_stmt(body, symbols);
        }
        CHirStmt::Compound(items, _) => {
            for item in items {
                collect_symbols_item(item, symbols);
            }
        }
        CHirStmt::Label(_, statement, _) | CHirStmt::Default(statement, _) => {
            collect_symbols_stmt(statement, symbols)
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

fn collect_symbols_expr(expression: &CHirExpr<'_>, symbols: &mut BTreeSet<SymbolId>) {
    match expression {
        CHirExpr::Ident { symbol, .. } => {
            symbols.insert(*symbol);
        }
        CHirExpr::Assign {
            place,
            target,
            value,
            ..
        } => {
            symbols.insert(place.symbol());
            collect_symbols_expr(target, symbols);
            collect_symbols_expr(value, symbols);
        }
        CHirExpr::Update { place, target, .. } => {
            symbols.insert(place.symbol());
            collect_symbols_expr(target, symbols);
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            if let Some(symbol) = place.root_symbol() {
                symbols.insert(symbol);
            }
            for source in place.source_expressions() {
                collect_symbols_expr(source, symbols);
            }
            collect_symbols_expr(value, symbols);
        }
        CHirExpr::MemoryRead { place, .. } => {
            if let Some(symbol) = place.root_symbol() {
                symbols.insert(symbol);
            }
            for source in place.source_expressions() {
                collect_symbols_expr(source, symbols);
            }
        }
        CHirExpr::MemoryUpdate { place, .. } => {
            if let Some(symbol) = place.root_symbol() {
                symbols.insert(symbol);
            }
            for source in place.source_expressions() {
                collect_symbols_expr(source, symbols);
            }
        }
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => collect_symbols_expr(operand, symbols),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _) => {
            collect_symbols_expr(left, symbols);
            collect_symbols_expr(right, symbols);
        }
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            collect_symbols_expr(condition, symbols);
            collect_symbols_expr(then_value, symbols);
            collect_symbols_expr(else_value, symbols);
        }
        CHirExpr::Call(callee, arguments, _) => {
            collect_symbols_expr(callee, symbols);
            for argument in arguments {
                collect_symbols_expr(argument, symbols);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            collect_symbols_expr(lhs, symbols);
            collect_symbols_expr(rhs, symbols);
            collect_symbols_expr(target, symbols);
        }
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            collect_symbols_expr(first, symbols);
            if let Some(second) = second {
                collect_symbols_expr(second, symbols);
            }
        }
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. }
                | InitAction::CopyAggregate { value, .. } = action
                {
                    collect_symbols_expr(value, symbols);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => collect_symbols_stmt(body, symbols),
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::SizeOfExpr(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => {}
    }
}

fn replace_inst_target(instruction: &mut Inst, old: BlockId, new: BlockId) {
    match instruction {
        Inst::Br(target, _) => {
            if *target == old {
                *target = new;
            }
        }
        Inst::CondBr(_, true_target, _, false_target, _) => {
            if *true_target == old {
                *true_target = new;
            }
            if *false_target == old {
                *false_target = new;
            }
        }
        _ => {}
    }
}

fn renumber_function(function: &mut Function) -> Result<(), LowerError> {
    let block_map = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            u32::try_from(index)
                .map(|new| (block.id, new))
                .map_err(|_| LowerError::InvalidGeneratedSsa {
                    message: "too many blocks to number".into(),
                })
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    let mut value_map = HashMap::new();
    let mut next = 0_u32;
    for block in &function.blocks {
        for argument in &block.args {
            value_map.insert(argument.value.id, next);
            next = next
                .checked_add(1)
                .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                    message: "too many values to number".into(),
                })?;
        }
        for instruction in block.insts.iter().chain([&block.terminator]) {
            for result in inst_results(instruction) {
                value_map.insert(result.id, next);
                next = next
                    .checked_add(1)
                    .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                        message: "too many values to number".into(),
                    })?;
            }
        }
    }
    for block in &mut function.blocks {
        block.id = *block_map
            .get(&block.id)
            .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                message: "block remap is incomplete".into(),
            })?;
        for argument in &mut block.args {
            remap_value(&mut argument.value, &value_map)?;
        }
        for instruction in &mut block.insts {
            remap_inst(instruction, &block_map, &value_map)?;
        }
        remap_inst(&mut block.terminator, &block_map, &value_map)?;
    }
    Ok(())
}

fn remap_value(value: &mut Value, value_map: &HashMap<u32, u32>) -> Result<(), LowerError> {
    value.id = *value_map
        .get(&value.id)
        .ok_or_else(|| LowerError::InvalidGeneratedSsa {
            message: format!("value {} has no final identity", value.id),
        })?;
    Ok(())
}

fn remap_inst(
    instruction: &mut Inst,
    block_map: &HashMap<BlockId, BlockId>,
    value_map: &HashMap<u32, u32>,
) -> Result<(), LowerError> {
    for value in inst_values_mut(instruction) {
        remap_value(value, value_map)?;
    }
    match instruction {
        Inst::AsmGoto {
            labels,
            fallthrough,
            ..
        } => {
            for edge in labels
                .iter_mut()
                .map(|label| &mut label.edge)
                .chain([fallthrough])
            {
                edge.target = *block_map.get(&edge.target).ok_or_else(|| {
                    LowerError::InvalidGeneratedSsa {
                        message: "assembly edge has no final identity".into(),
                    }
                })?;
            }
        }
        Inst::Br(target, _) => {
            *target = *block_map
                .get(target)
                .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                    message: format!("branch target {target} has no final identity"),
                })?;
        }
        Inst::CondBr(_, true_target, _, false_target, _) => {
            *true_target =
                *block_map
                    .get(true_target)
                    .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                        message: "true branch has no final identity".into(),
                    })?;
            *false_target =
                *block_map
                    .get(false_target)
                    .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                        message: "false branch has no final identity".into(),
                    })?;
        }
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn inst_values_mut(instruction: &mut Inst) -> Vec<&mut Value> {
    match instruction {
        Inst::PointerCast { result, operand } => vec![result, operand],
        Inst::BitOperation {
            result, operand, ..
        } => vec![result, operand],
        Inst::VaAccess(access) => {
            let mut values = vec![
                &mut access.list,
                &mut access.mem_in,
                &mut access.world_in,
                &mut access.mem_out,
                &mut access.world_out,
            ];
            if let evocc_evoir::VariadicOp::Arg(result) = &mut access.op {
                values.push(result);
            }
            values
        }
        Inst::Asm(assembly) => assembly.values_mut(),
        Inst::AsmGoto {
            assembly,
            labels,
            fallthrough,
        } => {
            let mut values = assembly.values_mut();
            for edge in labels
                .iter_mut()
                .map(|label| &mut label.edge)
                .chain([fallthrough])
            {
                values.extend(edge.arguments.iter_mut());
            }
            values
        }
        Inst::Const(result, _) | Inst::Uninitialized(result) => vec![result],
        Inst::Add(result, left, right)
        | Inst::Sub(result, left, right)
        | Inst::Mul(result, left, right)
        | Inst::SRem(result, left, right)
        | Inst::URem(result, left, right)
        | Inst::Shl(result, left, right)
        | Inst::AShr(result, left, right)
        | Inst::LShr(result, left, right)
        | Inst::And(result, left, right)
        | Inst::Or(result, left, right)
        | Inst::Xor(result, left, right)
        | Inst::SignedAddDefined(result, left, right)
        | Inst::SignedSubDefined(result, left, right)
        | Inst::SignedMulDefined(result, left, right)
        | Inst::SignedDivRemNoOverflow(result, left, right)
        | Inst::SDiv(result, left, right)
        | Inst::UDiv(result, left, right)
        | Inst::ICmp(result, _, left, right) => vec![result, left, right],
        Inst::PtrOffset {
            result,
            base,
            index,
            ..
        } => vec![result, base, index],
        Inst::PtrNull(result) | Inst::GlobalAddr { result, .. } | Inst::FuncAddr { result, .. } => {
            vec![result]
        }
        Inst::Alloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![result, mem_in, world_in, mem_out, world_out],
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            mem_out,
            world_out,
        } => vec![mem_in, world_in, pointer, mem_out, world_out],
        Inst::PointerValid {
            mem_in,
            pointer,
            mem_out,
        }
        | Inst::AccessInBounds {
            mem_in,
            pointer,
            mem_out,
            ..
        } => vec![mem_in, pointer, mem_out],
        Inst::DivisorNonZero(result, divisor) => vec![result, divisor],
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            ..
        }
        | Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            ..
        } => vec![result, shifted, amount],
        Inst::UbAssume(condition, _) => vec![condition],
        Inst::Trap(_) => Vec::new(),
        Inst::Br(_, arguments) | Inst::Ret(arguments) => arguments.iter_mut().collect(),
        Inst::CondBr(condition, _, true_args, _, false_args) => {
            let mut values = vec![condition];
            values.extend(true_args.iter_mut());
            values.extend(false_args.iter_mut());
            values
        }
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![result, address, mem_in, world_in, mem_out, world_out],
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![address, value, mem_in, world_in, mem_out, world_out],
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![destination, source, mem_in, world_in, mem_out, world_out],
        Inst::Call {
            results,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            let mut values = results.iter_mut().collect::<Vec<_>>();
            values.extend(args.iter_mut());
            values.push(mem_in);
            values.push(world_in);
            values.push(mem_out);
            values.push(world_out);
            values
        }
        Inst::IndirectCall {
            results,
            callee,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            let mut values = results.iter_mut().collect::<Vec<_>>();
            values.push(callee);
            values.extend(args.iter_mut());
            values.push(mem_in);
            values.push(world_in);
            values.push(mem_out);
            values.push(world_out);
            values
        }
        Inst::SExt(result, source, _)
        | Inst::ZExt(result, source, _)
        | Inst::Trunc(result, source, _) => {
            vec![result, source]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_completion_only_relaxes_the_initialized_outermost_bound() {
        use evocc_types::{CanonicalType, IntRank, Qualifiers};

        let mut types = TypeInterner::new();
        let int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let array = types.intern(CanonicalType {
            kind: TypeKind::Array {
                element: int,
                bound: ArrayBound::Fixed(4),
            },
            qualifiers: Qualifiers::default(),
        });
        let span = SourceSpan::new(evocc_common::FileId::new(0), 0, 1, 1, 1);
        let layer = |bound| WrittenDeclaratorLayer::Array {
            bound,
            parameter_qualifiers: Qualifiers::default(),
            static_bound: false,
            span,
        };
        let mut written = WrittenDeclarator {
            layers: vec![layer(ArrayBound::Unbounded)],
            span,
        };
        assert!(written_declarator_matches(
            &types, &written, array, int, None, true, false
        ));
        assert!(!written_declarator_matches(
            &types, &written, array, int, None, false, false
        ));
        written.layers[0] = layer(ArrayBound::Fixed(3));
        assert!(!written_declarator_matches(
            &types, &written, array, int, None, true, false
        ));
        let nested = types.intern(CanonicalType {
            kind: TypeKind::Array {
                element: array,
                bound: ArrayBound::Fixed(2),
            },
            qualifiers: Qualifiers::default(),
        });
        written.layers = vec![layer(ArrayBound::Fixed(2)), layer(ArrayBound::Unbounded)];
        assert!(!written_declarator_matches(
            &types, &written, nested, int, None, true, false
        ));
    }

    #[test]
    fn lower_error_is_stable_and_nonempty() {
        let error = LowerError::InvalidGeneratedSsa {
            message: "bad edge".into(),
        };
        assert_eq!(error.to_string(), "generated invalid EvoIR: bad edge");
    }
}
