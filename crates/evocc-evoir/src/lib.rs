//! EvoIR: typed block-argument SSA for optimization
//!
//! Values carry an identity (`id`) and a type so that the SSA validator can
//! verify single-static-definition form before any optimization transforms.

use std::collections::{HashMap, HashSet};
use std::fmt;

pub use evocc_memory::{AccessLayout, IndexSignedness, OffsetDirection};
use evocc_memory::{
    ByteSequence, FrameId, InitialState, MemoryConfig, MemoryFault, MemoryLimits, MemoryState,
    Pointer, PointerValue, ScopeId, WorldLimits, WorldState,
};
use evocc_types::{EvoIRType, Region, TargetDataLayout};

mod text;

pub use text::{render_module, validate_section9_bytes};

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// A value in the IR with a unique identifier and its type.
///
/// Every instruction that produces a result carries a `Value` as its first
/// field.  Operands and block-argument bindings also use `Value`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Value {
    pub id: u32,
    pub ty: EvoIRType,
}

/// A non-SSA function-interface binder.
///
/// Parameters deliberately have no [`Value`] identity.  The entry block owns
/// the SSA values corresponding to the interface, one argument per parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParam {
    pub name: String,
    pub ty: EvoIRType,
}

/// A named SSA value bound at block entry by the incoming edge (or by the
/// function interface for the entry block).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockArg {
    pub name: String,
    pub value: Value,
}

/// Opaque identifier for a basic block.
pub type BlockId = u32;

// ---------------------------------------------------------------------------
// Comparison operators
// ---------------------------------------------------------------------------

/// Integer comparison operators for `ICmp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmpOp {
    Eq,
    Ne,
    SLt,
    SLe,
    ULt,
    ULe,
}

/// The source-level undefined-behavior class guarded by [`Inst::UbAssume`].
///
/// The condition alone is deliberately insufficient to recover this
/// information: two arbitrary `bits<1>` values have identical EvoIR types but
/// may represent different C runtime diagnostics.  Lowering therefore records
/// the reason explicitly and the interpreter preserves it on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UbReason {
    DivisionByZero,
    RemainderByZero,
    SignedOverflow,
    InvalidShiftCount,
    InvalidSignedLeftShift,
}

impl UbReason {
    /// Stable diagnostic code shared with the C-HIR scalar interpreter.
    #[must_use]
    pub const fn diagnostic(self) -> &'static str {
        match self {
            Self::DivisionByZero => "E3002",
            Self::RemainderByZero => "E3003",
            Self::SignedOverflow => "E3004",
            Self::InvalidShiftCount => "E3005",
            Self::InvalidSignedLeftShift => "E3006",
        }
    }

    /// Stable diagnostic message shared with the C-HIR scalar interpreter.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::DivisionByZero => "integer-division-by-zero",
            Self::RemainderByZero => "integer-remainder-by-zero",
            Self::SignedOverflow => "signed-integer-overflow",
            Self::InvalidShiftCount => "invalid-shift-count",
            Self::InvalidSignedLeftShift => "invalid-signed-left-shift",
        }
    }

    /// Stable suffix used by the Section 9 textual syntax.
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::DivisionByZero => "division_by_zero",
            Self::RemainderByZero => "remainder_by_zero",
            Self::SignedOverflow => "signed_overflow",
            Self::InvalidShiftCount => "invalid_shift_count",
            Self::InvalidSignedLeftShift => "invalid_signed_left_shift",
        }
    }
}

/// A typed, fail-closed reason for terminating execution without returning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrapReason {
    ScalarFunctionFellOffEnd,
}

impl TrapReason {
    /// Stable diagnostic code shared with the C-HIR scalar interpreter.
    #[must_use]
    pub const fn diagnostic(self) -> &'static str {
        match self {
            Self::ScalarFunctionFellOffEnd => "E3007",
        }
    }

    /// Stable diagnostic message shared with the C-HIR scalar interpreter.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::ScalarFunctionFellOffEnd => "scalar-function-fell-off-end",
        }
    }

    /// Stable suffix used by the Section 9 textual syntax.
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::ScalarFunctionFellOffEnd => "scalar_function_fell_off_end",
        }
    }
}

// ---------------------------------------------------------------------------
// Instructions
// ---------------------------------------------------------------------------

/// An SSA instruction.
///
/// Pure instructions carry their result `Value` as their first field.
/// Effectful operations use named fields so their memory and world token flow
/// cannot be confused with ordinary call arguments or results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    /// `Const(result, bits)`: define `result` from a non-negative raw bit
    /// pattern. The result type determines how many low bits are retained.
    Const(Value, u64),

    /// Define an ordinary typed SSA value whose source object has not been
    /// initialized. This instruction has no operands and creates poison that
    /// may be copied through block arguments. Any semantic read traps with the
    /// typed uninitialized-read diagnostic.
    Uninitialized(Value),

    /// `Add(result, lhs, rhs)`: integer addition.
    Add(Value, Value, Value),
    /// `Sub(result, lhs, rhs)`: integer subtraction.
    Sub(Value, Value, Value),
    /// `Mul(result, lhs, rhs)`: integer multiplication.
    Mul(Value, Value, Value),
    /// Signed remainder. Defined executions truncate the quotient toward zero.
    SRem(Value, Value, Value),
    /// Unsigned remainder.
    URem(Value, Value, Value),
    /// Total bit-vector left shift; C shift validity is carried separately.
    Shl(Value, Value, Value),
    /// Total arithmetic right shift.
    AShr(Value, Value, Value),
    /// Total logical right shift.
    LShr(Value, Value, Value),
    /// Bitwise conjunction.
    And(Value, Value, Value),
    /// Bitwise disjunction.
    Or(Value, Value, Value),
    /// Bitwise exclusive-or.
    Xor(Value, Value, Value),

    /// True when signed addition of `lhs` and `rhs` is representable.
    SignedAddDefined(Value, Value, Value),
    /// True when signed subtraction of `lhs` and `rhs` is representable.
    SignedSubDefined(Value, Value, Value),
    /// True when signed multiplication of `lhs` and `rhs` is representable.
    SignedMulDefined(Value, Value, Value),
    /// True exactly when `divisor` is not zero.
    DivisorNonZero(Value, Value),
    /// True unless signed division/remainder would compute `MIN / -1`.
    /// A zero divisor is deliberately handled by a separate predicate.
    SignedDivRemNoOverflow(Value, Value, Value),
    /// True when `amount`, interpreted with its source signedness, is in the
    /// half-open range `[0, width(shifted))`.
    ShiftCountDefined {
        result: Value,
        shifted: Value,
        amount: Value,
        amount_signed: bool,
    },
    /// True when a signed left shift is representable. Invalid counts return
    /// false rather than relying on a preceding shift-count assumption.
    SignedShlDefined {
        result: Value,
        shifted: Value,
        amount: Value,
        amount_signed: bool,
    },

    /// Assert a typed `bits<1>` UB obligation. A false condition reports the
    /// exact source-level diagnostic identified by `UbReason`.
    UbAssume(Value, UbReason),
    /// `SDiv(result, lhs, rhs)`: signed integer division.
    SDiv(Value, Value, Value),
    /// `UDiv(result, lhs, rhs)`: unsigned integer division.
    UDiv(Value, Value, Value),

    /// `ICmp(result, op, lhs, rhs)`: integer comparison.
    ICmp(Value, CmpOp, Value, Value),

    /// `Br(target, args)`: unconditional branch, passing `args` to the
    /// destination block's block arguments.
    Br(BlockId, Vec<Value>),

    /// `CondBr(cond, true_block, true_args, false_block, false_args)`
    /// conditional branch.
    CondBr(Value, BlockId, Vec<Value>, BlockId, Vec<Value>),

    /// `Ret(values)`: return from function with the given values.
    Ret(Vec<Value>),

    /// Terminate execution with an explicit typed failure. Unlike `Ret`, a
    /// trap has no ordinary results or effect-token operands.
    Trap(TrapReason),

    /// Construct the canonical null pointer.
    PtrNull(Value),

    /// Return the stable address of one module global.
    GlobalAddr { result: Value, symbol: String },

    /// Return one function's stable provenance with its complete signature.
    FuncAddr { result: Value, symbol: String },

    /// Allocate one fresh automatic object in the current invocation.
    Alloca {
        result: Value,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
        origin: u64,
        layout: AccessLayout,
    },

    /// Explicitly end one automatic object's lifetime.
    LifetimeEnd {
        mem_in: Value,
        world_in: Value,
        pointer: Value,
        mem_out: Value,
        world_out: Value,
    },

    /// Validate provenance, lifetime, and the closed pointer range.
    PointerValid {
        mem_in: Value,
        pointer: Value,
        mem_out: Value,
    },

    /// Validate a complete aligned non-empty memory access.
    AccessInBounds {
        mem_in: Value,
        pointer: Value,
        mem_out: Value,
        layout: AccessLayout,
    },

    /// Load through `address`, consuming and producing explicit effect tokens.
    /// The loaded type is owned by `result`.
    Load {
        result: Value,
        address: Value,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
        volatile: bool,
    },

    /// Store `value` through `address`, consuming and producing effect tokens.
    Store {
        address: Value,
        value: Value,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
        volatile: bool,
    },

    /// Target-scaled, provenance-preserving pointer arithmetic.
    PtrOffset {
        result: Value,
        base: Value,
        index: Value,
        direction: OffsetDirection,
        signedness: IndexSignedness,
        scale: u64,
    },

    /// Call `callee`. Ordinary `args` and `results` exclude the separately
    /// threaded memory and world tokens.
    Call {
        results: Vec<Value>,
        callee: String,
        args: Vec<Value>,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
    },

    /// Call through a signature-carrying function pointer.
    IndirectCall {
        results: Vec<Value>,
        callee: Value,
        args: Vec<Value>,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
    },

    /// `SExt(result, src, target_ty)`: sign-extend `src` to `target_ty`.
    SExt(Value, Value, EvoIRType),

    /// `ZExt(result, src, target_ty)`: zero-extend `src` to `target_ty`.
    ZExt(Value, Value, EvoIRType),

    /// `Trunc(result, src, target_ty)`: truncate `src` to `target_ty`.
    Trunc(Value, Value, EvoIRType),
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns all SSA values defined by an instruction, in textual result order.
#[must_use]
pub fn inst_results(inst: &Inst) -> Vec<&Value> {
    match inst {
        Inst::Const(v, _)
        | Inst::Uninitialized(v)
        | Inst::Add(v, ..)
        | Inst::Sub(v, ..)
        | Inst::Mul(v, ..)
        | Inst::SRem(v, ..)
        | Inst::URem(v, ..)
        | Inst::Shl(v, ..)
        | Inst::AShr(v, ..)
        | Inst::LShr(v, ..)
        | Inst::And(v, ..)
        | Inst::Or(v, ..)
        | Inst::Xor(v, ..)
        | Inst::SignedAddDefined(v, ..)
        | Inst::SignedSubDefined(v, ..)
        | Inst::SignedMulDefined(v, ..)
        | Inst::DivisorNonZero(v, ..)
        | Inst::SignedDivRemNoOverflow(v, ..)
        | Inst::SDiv(v, ..)
        | Inst::UDiv(v, ..)
        | Inst::ICmp(v, ..)
        | Inst::PtrNull(v)
        | Inst::SExt(v, ..)
        | Inst::ZExt(v, ..)
        | Inst::Trunc(v, ..) => vec![v],
        Inst::GlobalAddr { result, .. }
        | Inst::FuncAddr { result, .. }
        | Inst::PtrOffset { result, .. } => vec![result],
        Inst::ShiftCountDefined { result, .. } | Inst::SignedShlDefined { result, .. } => {
            vec![result]
        }
        Inst::Load {
            result,
            mem_out,
            world_out,
            ..
        } => vec![result, mem_out, world_out],
        Inst::Store {
            mem_out, world_out, ..
        } => vec![mem_out, world_out],
        Inst::Alloca {
            result,
            mem_out,
            world_out,
            ..
        } => vec![result, mem_out, world_out],
        Inst::LifetimeEnd {
            mem_out, world_out, ..
        } => vec![mem_out, world_out],
        Inst::PointerValid { mem_out, .. } | Inst::AccessInBounds { mem_out, .. } => vec![mem_out],
        Inst::Call {
            results,
            mem_out,
            world_out,
            ..
        }
        | Inst::IndirectCall {
            results,
            mem_out,
            world_out,
            ..
        } => {
            let mut definitions: Vec<&Value> = results.iter().collect();
            definitions.push(mem_out);
            definitions.push(world_out);
            definitions
        }
        Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) | Inst::Trap(..) | Inst::UbAssume(..) => {
            vec![]
        }
    }
}

/// Returns all `Value` operands that this instruction *uses* (reads).
#[must_use]
pub fn inst_uses(inst: &Inst) -> Vec<&Value> {
    match inst {
        Inst::Const(..)
        | Inst::Uninitialized(..)
        | Inst::Trap(..)
        | Inst::PtrNull(..)
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. } => vec![],

        Inst::Add(_, lhs, rhs)
        | Inst::Sub(_, lhs, rhs)
        | Inst::Mul(_, lhs, rhs)
        | Inst::SRem(_, lhs, rhs)
        | Inst::URem(_, lhs, rhs)
        | Inst::Shl(_, lhs, rhs)
        | Inst::AShr(_, lhs, rhs)
        | Inst::LShr(_, lhs, rhs)
        | Inst::And(_, lhs, rhs)
        | Inst::Or(_, lhs, rhs)
        | Inst::Xor(_, lhs, rhs)
        | Inst::SignedAddDefined(_, lhs, rhs)
        | Inst::SignedSubDefined(_, lhs, rhs)
        | Inst::SignedMulDefined(_, lhs, rhs)
        | Inst::SignedDivRemNoOverflow(_, lhs, rhs)
        | Inst::SDiv(_, lhs, rhs)
        | Inst::UDiv(_, lhs, rhs) => vec![lhs, rhs],

        Inst::DivisorNonZero(_, divisor) => vec![divisor],
        Inst::ShiftCountDefined {
            shifted, amount, ..
        }
        | Inst::SignedShlDefined {
            shifted, amount, ..
        } => vec![shifted, amount],

        Inst::UbAssume(condition, _) => vec![condition],

        Inst::ICmp(_, _, lhs, rhs) => vec![lhs, rhs],

        Inst::Br(_, args) => args.iter().collect(),
        Inst::CondBr(cond, _, targs, _, fargs) => {
            let mut uses: Vec<&Value> = vec![cond];
            uses.extend(targs);
            uses.extend(fargs);
            uses
        }
        Inst::Ret(values) => values.iter().collect(),

        Inst::Load {
            address,
            mem_in,
            world_in,
            ..
        } => vec![address, mem_in, world_in],
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            ..
        } => vec![address, value, mem_in, world_in],
        Inst::Alloca {
            mem_in, world_in, ..
        } => vec![mem_in, world_in],
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            ..
        } => vec![mem_in, world_in, pointer],
        Inst::PointerValid {
            mem_in, pointer, ..
        }
        | Inst::AccessInBounds {
            mem_in, pointer, ..
        } => vec![mem_in, pointer],
        Inst::PtrOffset { base, index, .. } => vec![base, index],
        Inst::Call {
            args,
            mem_in,
            world_in,
            ..
        } => {
            let mut uses: Vec<&Value> = args.iter().collect();
            uses.push(mem_in);
            uses.push(world_in);
            uses
        }
        Inst::IndirectCall {
            callee,
            args,
            mem_in,
            world_in,
            ..
        } => {
            let mut uses: Vec<&Value> = vec![callee];
            uses.extend(args);
            uses.push(mem_in);
            uses.push(world_in);
            uses
        }
        Inst::SExt(_, src, _) | Inst::ZExt(_, src, _) | Inst::Trunc(_, src, _) => vec![src],
    }
}

// ---------------------------------------------------------------------------
// Blocks and functions
// ---------------------------------------------------------------------------

/// A basic block in the control-flow graph.
///
/// `args` are block arguments: values bound at block entry.  The entry
/// block's args semantically include the function's `params`.
///
/// `insts` is the list of non-terminator instructions; `terminator` is
/// always present and must be one of `Br`, `CondBr`, or `Ret`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: BlockId,
    pub args: Vec<BlockArg>,
    pub insts: Vec<Inst>,
    pub terminator: Inst,
}

/// The exact observable effects implied by one function and its direct callees.
///
/// Day 8 has a single memory region, so the two region sets are represented by
/// their `all` membership. The remaining components follow specification
/// Section 4.6 and participate in the same componentwise join.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EffectSummary {
    pub reads_all: bool,
    pub writes_all: bool,
    pub volatile: bool,
    pub atomic: bool,
    pub io: bool,
    pub may_trap: bool,
    pub may_unwind: bool,
}

impl EffectSummary {
    /// The conservative summary assigned to an indirect call.
    #[must_use]
    pub const fn top() -> Self {
        Self {
            reads_all: true,
            writes_all: true,
            volatile: true,
            atomic: true,
            io: true,
            may_trap: true,
            may_unwind: true,
        }
    }

    fn join(&mut self, other: Self) {
        self.reads_all |= other.reads_all;
        self.writes_all |= other.writes_all;
        self.volatile |= other.volatile;
        self.atomic |= other.atomic;
        self.io |= other.io;
        self.may_trap |= other.may_trap;
        self.may_unwind |= other.may_unwind;
    }
}

/// A function in EvoIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<FunctionParam>,
    pub ret_tys: Vec<EvoIRType>,
    pub effects: EffectSummary,
    pub blocks: Vec<Block>,
}

/// A collection of mutually callable EvoIR functions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Module {
    pub globals: Vec<GlobalObject>,
    pub functions: Vec<Function>,
}

impl Module {
    /// Replace every stored summary with the least fixed point implied by this
    /// module's instructions and direct-call graph.
    pub fn recompute_effect_summaries(&mut self) {
        let summaries = derive_effect_summaries(self);
        for (function, summary) in self.functions.iter_mut().zip(summaries) {
            function.effects = summary;
        }
    }
}

/// Compute the least fixed point of Section 4.6 without trusting any recorded
/// summary already present in the module.
#[must_use]
pub fn derive_effect_summaries(module: &Module) -> Vec<EffectSummary> {
    let positions = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.as_str(), index))
        .collect::<HashMap<_, _>>();
    let local = module
        .functions
        .iter()
        .map(local_effect_summary)
        .collect::<Vec<_>>();
    let mut summaries = local.clone();
    loop {
        let previous = summaries.clone();
        for (index, function) in module.functions.iter().enumerate() {
            let mut summary = local[index];
            for inst in function.blocks.iter().flat_map(|block| &block.insts) {
                match inst {
                    Inst::Call { callee, .. } => summary.join(
                        positions
                            .get(callee.as_str())
                            .map_or(EffectSummary::top(), |callee| previous[*callee]),
                    ),
                    Inst::IndirectCall { .. } => summary.join(EffectSummary::top()),
                    _ => {}
                }
            }
            summaries[index] = summary;
        }
        if summaries == previous {
            return summaries;
        }
    }
}

fn local_effect_summary(function: &Function) -> EffectSummary {
    let mut summary = EffectSummary::default();
    for inst in function
        .blocks
        .iter()
        .flat_map(|block| block.insts.iter().chain(std::iter::once(&block.terminator)))
    {
        summary.join(inst_effect_summary(inst));
    }
    summary
}

#[allow(clippy::too_many_lines)]
fn inst_effect_summary(inst: &Inst) -> EffectSummary {
    match inst {
        Inst::SDiv(..)
        | Inst::UDiv(..)
        | Inst::SRem(..)
        | Inst::URem(..)
        | Inst::UbAssume(..)
        | Inst::PtrOffset { .. }
        | Inst::Trap(..) => EffectSummary {
            may_trap: true,
            ..EffectSummary::default()
        },
        Inst::Alloca { .. } | Inst::LifetimeEnd { .. } => EffectSummary {
            reads_all: true,
            writes_all: true,
            may_trap: true,
            ..EffectSummary::default()
        },
        Inst::PointerValid { .. } | Inst::AccessInBounds { .. } => EffectSummary {
            reads_all: true,
            may_trap: true,
            ..EffectSummary::default()
        },
        Inst::Load { volatile, .. } => EffectSummary {
            reads_all: true,
            volatile: *volatile,
            may_trap: true,
            ..EffectSummary::default()
        },
        Inst::Store { volatile, .. } => EffectSummary {
            writes_all: true,
            volatile: *volatile,
            may_trap: true,
            ..EffectSummary::default()
        },
        Inst::IndirectCall { .. } => EffectSummary::top(),
        Inst::Const(..)
        | Inst::Uninitialized(..)
        | Inst::Add(..)
        | Inst::Sub(..)
        | Inst::Mul(..)
        | Inst::Shl(..)
        | Inst::AShr(..)
        | Inst::LShr(..)
        | Inst::And(..)
        | Inst::Or(..)
        | Inst::Xor(..)
        | Inst::SignedAddDefined(..)
        | Inst::SignedSubDefined(..)
        | Inst::SignedMulDefined(..)
        | Inst::DivisorNonZero(..)
        | Inst::SignedDivRemNoOverflow(..)
        | Inst::ShiftCountDefined { .. }
        | Inst::SignedShlDefined { .. }
        | Inst::ICmp(..)
        | Inst::Br(..)
        | Inst::CondBr(..)
        | Inst::Ret(..)
        | Inst::PtrNull(..)
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. }
        | Inst::Call { .. }
        | Inst::SExt(..)
        | Inst::ZExt(..)
        | Inst::Trunc(..) => EffectSummary::default(),
    }
}

/// Source category encoded by the textual module-global table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalKind {
    File,
    FunctionStatic,
    StringLiteral,
}

/// Link visibility of one module global.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalVisibility {
    External,
    Internal,
}

/// One eagerly allocated module object and its byte initialization state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalObject {
    pub symbol: String,
    pub origin: u64,
    pub kind: GlobalKind,
    pub visibility: GlobalVisibility,
    pub layout: AccessLayout,
    pub bytes: Vec<u8>,
    pub initialized: Vec<bool>,
}

// ---------------------------------------------------------------------------
// SSA validator
// ---------------------------------------------------------------------------

/// Validate block-argument SSA without inferring cross-block dominance.
///
/// Function parameters are interface binders, not SSA definitions.  The first
/// block must therefore declare one block argument of the same type for every
/// parameter.  Within every block, an operand may name only a block argument
/// or an instruction result defined earlier in that same block.  Values that
/// cross a CFG edge must be passed explicitly to the destination arguments.
pub fn validate_ssa(func: &Function) -> Result<(), String> {
    let Some(entry) = func.blocks.first() else {
        return Err(format!("function '{}' has no entry block", func.name));
    };

    let mut blocks = HashMap::new();
    for block in &func.blocks {
        if blocks.insert(block.id, block).is_some() {
            return Err(format!("duplicate block id {}", block.id));
        }
    }

    if entry.args.len() != func.params.len() {
        return Err(format!(
            "entry block {} has {} arguments but function '{}' has {} parameters",
            entry.id,
            entry.args.len(),
            func.name,
            func.params.len()
        ));
    }
    for (index, (param, arg)) in func.params.iter().zip(&entry.args).enumerate() {
        ensure_value_type(&param.ty, &format!("parameter {index}"))?;
        ensure_value_type(&arg.value.ty, &format!("entry argument {index}"))?;
        if param.ty != arg.value.ty {
            return Err(format!(
                "entry argument {} has type {:?}, expected parameter type {:?}",
                index, arg.value.ty, param.ty
            ));
        }
    }
    for (index, ty) in func.ret_tys.iter().enumerate() {
        ensure_value_type(ty, &format!("return type {index}"))?;
    }

    let mut definitions: HashMap<u32, String> = HashMap::new();
    for block in &func.blocks {
        let mut names = HashSet::new();
        for arg in &block.args {
            ensure_value_type(
                &arg.value.ty,
                &format!("argument '{}' in block {}", arg.name, block.id),
            )?;
            if !names.insert(arg.name.as_str()) {
                return Err(format!(
                    "duplicate block argument name '{}' in block {}",
                    arg.name, block.id
                ));
            }
            register_definition(
                &mut definitions,
                &arg.value,
                format!("block argument '{}' in block {}", arg.name, block.id),
            )?;
        }
    }

    for block in &func.blocks {
        let mut local: HashMap<u32, EvoIRType> = block
            .args
            .iter()
            .map(|arg| (arg.value.id, arg.value.ty.clone()))
            .collect();
        let mut effects = EffectState::from_block_args(block)?;
        let mut access_guards: HashMap<u32, (u32, AccessLayout)> = HashMap::new();

        for (index, inst) in block.insts.iter().enumerate() {
            if is_terminator(inst) {
                return Err(format!(
                    "terminator appears in instruction {} of block {}",
                    index, block.id
                ));
            }
            for result in inst_results(inst) {
                ensure_value_type(
                    &result.ty,
                    &format!(
                        "result v{} of instruction {} in block {}",
                        result.id, index, block.id
                    ),
                )?;
            }
            validate_embedded_instruction_types(inst, index, block.id)?;
            for used in inst_uses(inst) {
                ensure_value_type(
                    &used.ty,
                    &format!(
                        "use of v{} in instruction {} of block {}",
                        used.id, index, block.id
                    ),
                )?;
                require_local_value(&local, used, block.id, "instruction")?;
            }
            validate_instruction_types(inst, block.id)?;
            validate_memory_contract(
                inst,
                block.insts.get(index + 1),
                block.id,
                &effects,
                &mut access_guards,
            )?;
            effects.advance(inst, block.id)?;
            for result in inst_results(inst) {
                register_definition(
                    &mut definitions,
                    result,
                    format!("instruction {} in block {}", index, block.id),
                )?;
                local.insert(result.id, result.ty.clone());
            }
        }

        if !is_terminator(&block.terminator) {
            return Err(format!("block {} ends with a non-terminator", block.id));
        }
        for used in inst_uses(&block.terminator) {
            ensure_value_type(
                &used.ty,
                &format!("terminator use of v{} in block {}", used.id, block.id),
            )?;
            require_local_value(&local, used, block.id, "terminator")?;
        }

        match &block.terminator {
            Inst::Br(target, args) => {
                validate_edge(&blocks, block.id, *target, args)?;
                validate_effect_edge(&blocks, block.id, *target, args, &effects)?;
            }
            Inst::CondBr(cond, true_block, true_args, false_block, false_args) => {
                if cond.ty != EvoIRType::Bits(1) {
                    return Err(format!(
                        "conditional branch in block {} uses {:?}; expected bits<1>",
                        block.id, cond.ty
                    ));
                }
                validate_edge(&blocks, block.id, *true_block, true_args)?;
                validate_edge(&blocks, block.id, *false_block, false_args)?;
                validate_effect_edge(&blocks, block.id, *true_block, true_args, &effects)?;
                validate_effect_edge(&blocks, block.id, *false_block, false_args, &effects)?;
            }
            Inst::Ret(values) => {
                if values.len() != func.ret_tys.len() {
                    return Err(format!(
                        "return in block {} has {} values, expected {}",
                        block.id,
                        values.len(),
                        func.ret_tys.len()
                    ));
                }
                for (index, (value, expected)) in values.iter().zip(&func.ret_tys).enumerate() {
                    if &value.ty != expected {
                        return Err(format!(
                            "return value {} in block {} has type {:?}, expected {:?}",
                            index, block.id, value.ty, expected
                        ));
                    }
                }
                validate_effect_boundary(
                    &func.ret_tys,
                    values,
                    &effects,
                    &format!("return in block {}", block.id),
                )?;
            }
            Inst::Trap(_) => {}
            _ => return Err(format!("block {} has an invalid terminator", block.id)),
        }
    }

    validate_reachability(func, entry.id)?;

    Ok(())
}

fn validate_memory_contract(
    inst: &Inst,
    next: Option<&Inst>,
    block: BlockId,
    effects: &EffectState,
    access_guards: &mut HashMap<u32, (u32, AccessLayout)>,
) -> Result<(), String> {
    match inst {
        Inst::PtrOffset { result, .. } => {
            let Some(Inst::PointerValid {
                mem_in, pointer, ..
            }) = next
            else {
                return Err(format!(
                    "ptr.offset result v{} in block {block} must be immediately followed by pointer_valid",
                    result.id
                ));
            };
            if pointer.id != result.id || effects.mem[region_index(Region::All)] != Some(mem_in.id)
            {
                return Err(format!(
                    "ptr.offset result v{} in block {block} has a mismatched pointer_valid obligation",
                    result.id
                ));
            }
        }
        Inst::AccessInBounds {
            pointer,
            mem_out,
            layout,
            ..
        } => {
            access_guards.insert(mem_out.id, (pointer.id, *layout));
        }
        Inst::Load {
            result,
            address,
            mem_in,
            ..
        } => validate_matching_access_guard(
            access_guards,
            mem_in,
            address,
            evoir_access_layout(&result.ty)?,
            "load",
            block,
        )?,
        Inst::Store {
            address,
            value,
            mem_in,
            ..
        } => validate_matching_access_guard(
            access_guards,
            mem_in,
            address,
            evoir_access_layout(&value.ty)?,
            "store",
            block,
        )?,
        _ => {}
    }
    Ok(())
}

fn validate_matching_access_guard(
    access_guards: &HashMap<u32, (u32, AccessLayout)>,
    mem_in: &Value,
    address: &Value,
    expected: Option<AccessLayout>,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    match access_guards.get(&mem_in.id) {
        Some((pointer, layout))
            if *pointer == address.id && expected.is_none_or(|expected| *layout == expected) =>
        {
            Ok(())
        }
        _ => Err(format!(
            "{operation} in block {block} must consume a matching access_in_bounds token for v{} with {expected:?}",
            address.id
        )),
    }
}

fn evoir_access_layout(ty: &EvoIRType) -> Result<Option<AccessLayout>, String> {
    let size = match ty {
        EvoIRType::Bits(width) if *width > 0 => u64::from(width.div_ceil(8)),
        EvoIRType::F32 => 4,
        EvoIRType::F64 => 8,
        // Pointer size/alignment is target data, so target-independent module
        // validation checks only pointer identity. Lowering and interpretation
        // use the explicit guard layout produced from TargetDataLayout.
        EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => return Ok(None),
        _ => {
            return Err(format!(
                "Day 7 memory type {ty:?} has no scalar access layout"
            ))
        }
    };
    let align = u32::try_from(size.min(8)).map_err(|_| "invalid access alignment".to_owned())?;
    Ok(Some(AccessLayout::new(size, align)))
}

/// Validate every function and all inter-function call signatures.
pub fn validate_module(module: &Module) -> Result<(), String> {
    let mut functions = HashMap::new();
    for function in &module.functions {
        if functions.insert(function.name.as_str(), function).is_some() {
            return Err(format!("duplicate function symbol '{}'", function.name));
        }
        validate_ssa(function)
            .map_err(|message| format!("function '{}': {message}", function.name))?;
    }

    let mut globals = HashMap::new();
    let mut origins = HashSet::new();
    for global in &module.globals {
        validate_layout(global.layout, "module global", 0)?;
        let size = usize::try_from(global.layout.size_bytes)
            .map_err(|_| format!("global '{}' size exceeds host limits", global.symbol))?;
        if global.bytes.len() != size || global.initialized.len() != size {
            return Err(format!(
                "global '{}' bytes/initialized lengths must equal size {}",
                global.symbol, global.layout.size_bytes
            ));
        }
        if global.kind == GlobalKind::StringLiteral && global.initialized.iter().any(|bit| !*bit) {
            return Err(format!(
                "string-literal global '{}' must be fully initialized",
                global.symbol
            ));
        }
        match global.kind {
            GlobalKind::File => {
                if global.symbol.contains('.') {
                    return Err(format!(
                        "file global '{}' must use a source C identifier",
                        global.symbol
                    ));
                }
            }
            GlobalKind::FunctionStatic => {
                let expected = format!("__evocc.static.{}", global.origin);
                if global.symbol != expected || global.visibility != GlobalVisibility::Internal {
                    return Err(format!(
                        "function-static global '{}' must be internal symbol '{expected}'",
                        global.symbol
                    ));
                }
            }
            GlobalKind::StringLiteral => {
                let expected = format!("__evocc.string.{}", global.origin);
                if global.symbol != expected || global.visibility != GlobalVisibility::Internal {
                    return Err(format!(
                        "string-literal global '{}' must be internal symbol '{expected}'",
                        global.symbol
                    ));
                }
            }
        }
        if functions.contains_key(global.symbol.as_str()) {
            return Err(format!(
                "symbol '{}' is both a function and global",
                global.symbol
            ));
        }
        if globals.insert(global.symbol.as_str(), global).is_some() {
            return Err(format!("duplicate global symbol '{}'", global.symbol));
        }
        if !origins.insert(global.origin) {
            return Err(format!("duplicate allocation origin {}", global.origin));
        }
    }

    let mut effect_participants = HashSet::new();
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.insts {
                match instruction {
                    Inst::Call { callee, .. } => {
                        effect_participants.insert(function.name.as_str());
                        effect_participants.insert(callee.as_str());
                    }
                    Inst::IndirectCall { .. } => {
                        effect_participants.insert(function.name.as_str());
                    }
                    Inst::FuncAddr { symbol, .. } => {
                        effect_participants.insert(symbol.as_str());
                    }
                    Inst::Load { .. }
                    | Inst::Store { .. }
                    | Inst::Alloca { .. }
                    | Inst::LifetimeEnd { .. }
                    | Inst::PointerValid { .. }
                    | Inst::AccessInBounds { .. } => {
                        effect_participants.insert(function.name.as_str());
                    }
                    _ => {}
                }
            }
        }
    }
    for function in &module.functions {
        if effect_participants.contains(function.name.as_str()) {
            callable_signature(function)
                .map_err(|message| format!("call/effect participant: {message}"))?;
        }
    }

    for caller in &module.functions {
        for block in &caller.blocks {
            for (index, instruction) in block.insts.iter().enumerate() {
                match instruction {
                    Inst::GlobalAddr { symbol, .. } if !globals.contains_key(symbol.as_str()) => {
                        return Err(format!(
                            "function '{}' block {} instruction {} refers to missing global '{}'",
                            caller.name, block.id, index, symbol
                        ));
                    }
                    Inst::GlobalAddr { symbol, .. } if functions.contains_key(symbol.as_str()) => {
                        return Err(format!(
                            "function '{}' block {} instruction {} uses function '{}' as global",
                            caller.name, block.id, index, symbol
                        ));
                    }
                    Inst::FuncAddr { result, symbol } => {
                        let target = functions.get(symbol.as_str()).ok_or_else(|| {
                            format!(
                                "function '{}' block {} instruction {} refers to missing function '{}'",
                                caller.name, block.id, index, symbol
                            )
                        })?;
                        let expected = function_pointer_type(target)?;
                        if result.ty != expected {
                            return Err(format!(
                                "function '{}' block {} instruction {} func_addr type {:?} disagrees with '{}' signature {:?}",
                                caller.name, block.id, index, result.ty, symbol, expected
                            ));
                        }
                    }
                    Inst::Alloca { origin, .. } if !origins.insert(*origin) => {
                        return Err(format!("duplicate allocation origin {origin}"));
                    }
                    _ => {}
                }
                let Inst::Call {
                    results,
                    callee,
                    args,
                    mem_in,
                    world_in,
                    mem_out,
                    world_out,
                } = instruction
                else {
                    continue;
                };
                let target = functions.get(callee.as_str()).ok_or_else(|| {
                    format!(
                        "function '{}' block {} instruction {} calls missing callee '{}'",
                        caller.name, block.id, index, callee
                    )
                })?;
                validate_call_signature(
                    caller, block.id, index, target, results, args, mem_in, world_in, mem_out,
                    world_out,
                )?;
            }
        }
    }
    for (function, expected) in module.functions.iter().zip(derive_effect_summaries(module)) {
        if function.effects != expected {
            return Err(format!(
                "function '{}' recorded effect summary {:?}, expected exact Section 4.6 summary {:?}",
                function.name, function.effects, expected
            ));
        }
    }
    Ok(())
}

fn function_pointer_type(function: &Function) -> Result<EvoIRType, String> {
    callable_signature(function)?;
    Ok(EvoIRType::FnPtr {
        params: function
            .params
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect(),
        results: function.ret_tys.clone(),
    })
}

struct CallableSignature<'a> {
    ordinary_params: &'a [FunctionParam],
    ordinary_returns: &'a [EvoIRType],
    memory_region: Region,
}

fn callable_signature(function: &Function) -> Result<CallableSignature<'_>, String> {
    if function.params.len() < 2 || function.ret_tys.len() < 2 {
        return Err(format!(
            "function '{}' lacks trailing mem<R>, world parameters and returns",
            function.name
        ));
    }
    let parameter_split = function.params.len() - 2;
    let return_split = function.ret_tys.len() - 2;
    let memory_region = match (
        &function.params[parameter_split].ty,
        &function.params[parameter_split + 1].ty,
    ) {
        (EvoIRType::Mem(region), EvoIRType::World) => *region,
        _ => {
            return Err(format!(
                "function '{}' must end parameters with mem<R>, world",
                function.name
            ));
        }
    };
    match (
        &function.ret_tys[return_split],
        &function.ret_tys[return_split + 1],
    ) {
        (EvoIRType::Mem(region), EvoIRType::World) if *region == memory_region => {}
        _ => {
            return Err(format!(
                "function '{}' must end returns with mem<{:?}>, world",
                function.name, memory_region
            ));
        }
    }
    for parameter in &function.params[..parameter_split] {
        if contains_effect_token(&parameter.ty)? {
            return Err(format!(
                "function '{}' ordinary parameter '{}' contains an effect token",
                function.name, parameter.name
            ));
        }
    }
    for (index, ty) in function.ret_tys[..return_split].iter().enumerate() {
        if contains_effect_token(ty)? {
            return Err(format!(
                "function '{}' ordinary return {} contains an effect token",
                function.name, index
            ));
        }
    }
    Ok(CallableSignature {
        ordinary_params: &function.params[..parameter_split],
        ordinary_returns: &function.ret_tys[..return_split],
        memory_region,
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_call_signature(
    caller: &Function,
    block: BlockId,
    instruction: usize,
    callee: &Function,
    results: &[Value],
    args: &[Value],
    mem_in: &Value,
    world_in: &Value,
    mem_out: &Value,
    world_out: &Value,
) -> Result<(), String> {
    let signature = callable_signature(callee).map_err(|message| {
        format!(
            "function '{}' block {} instruction {}: {message}",
            caller.name, block, instruction
        )
    })?;
    if args.len() != signature.ordinary_params.len() {
        return Err(format!(
            "call from '{}' to '{}' has {} ordinary arguments, expected {}",
            caller.name,
            callee.name,
            args.len(),
            signature.ordinary_params.len()
        ));
    }
    for (index, (argument, parameter)) in args.iter().zip(signature.ordinary_params).enumerate() {
        if argument.ty != parameter.ty {
            return Err(format!(
                "call from '{}' to '{}' argument {} has type {:?}, expected {:?}",
                caller.name, callee.name, index, argument.ty, parameter.ty
            ));
        }
    }
    if results.len() != signature.ordinary_returns.len() {
        return Err(format!(
            "call from '{}' to '{}' has {} ordinary results, expected {}",
            caller.name,
            callee.name,
            results.len(),
            signature.ordinary_returns.len()
        ));
    }
    for (index, (result, expected)) in results.iter().zip(signature.ordinary_returns).enumerate() {
        if &result.ty != expected {
            return Err(format!(
                "call from '{}' to '{}' result {} has type {:?}, expected {:?}",
                caller.name, callee.name, index, result.ty, expected
            ));
        }
    }
    let expected_memory = EvoIRType::Mem(signature.memory_region);
    if mem_in.ty != expected_memory || mem_out.ty != expected_memory {
        return Err(format!(
            "call from '{}' to '{}' must thread memory token {:?}",
            caller.name, callee.name, expected_memory
        ));
    }
    if world_in.ty != EvoIRType::World || world_out.ty != EvoIRType::World {
        return Err(format!(
            "call from '{}' to '{}' must thread a world token",
            caller.name, callee.name
        ));
    }
    Ok(())
}

fn validate_embedded_instruction_types(
    inst: &Inst,
    index: usize,
    block: BlockId,
) -> Result<(), String> {
    let target = match inst {
        Inst::SExt(_, _, target) | Inst::ZExt(_, _, target) | Inst::Trunc(_, _, target) => {
            Some(target)
        }
        _ => None,
    };
    if let Some(target) = target {
        ensure_value_type(
            target,
            &format!("target type of instruction {index} in block {block}"),
        )?;
    }
    Ok(())
}

fn register_definition(
    definitions: &mut HashMap<u32, String>,
    value: &Value,
    site: String,
) -> Result<(), String> {
    if let Some(previous) = definitions.insert(value.id, site.clone()) {
        return Err(format!(
            "value v{} defined more than once (previous: {}; duplicate: {})",
            value.id, previous, site
        ));
    }
    Ok(())
}

fn require_local_value(
    local: &HashMap<u32, EvoIRType>,
    value: &Value,
    block: BlockId,
    context: &str,
) -> Result<(), String> {
    let Some(defined_ty) = local.get(&value.id) else {
        return Err(format!(
            "value v{} used by {} in block {} is not a prior local definition; thread it through a block argument",
            value.id, context, block
        ));
    };
    if defined_ty != &value.ty {
        return Err(format!(
            "value v{} used by {} in block {} has type {:?}, but its local definition has type {:?}",
            value.id, context, block, value.ty, defined_ty
        ));
    }
    Ok(())
}

fn is_terminator(inst: &Inst) -> bool {
    matches!(
        inst,
        Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) | Inst::Trap(..)
    )
}

fn validate_edge(
    blocks: &HashMap<BlockId, &Block>,
    source: BlockId,
    target: BlockId,
    values: &[Value],
) -> Result<(), String> {
    let Some(destination) = blocks.get(&target) else {
        return Err(format!(
            "branch in block {} targets missing block {}",
            source, target
        ));
    };
    if values.len() != destination.args.len() {
        return Err(format!(
            "edge {} -> {} passes {} values, expected {}",
            source,
            target,
            values.len(),
            destination.args.len()
        ));
    }
    for (index, (value, arg)) in values.iter().zip(&destination.args).enumerate() {
        if value.ty != arg.value.ty {
            return Err(format!(
                "edge {} -> {} value {} has type {:?}, expected {:?}",
                source, target, index, value.ty, arg.value.ty
            ));
        }
    }
    Ok(())
}

fn validate_effect_edge(
    blocks: &HashMap<BlockId, &Block>,
    source: BlockId,
    target: BlockId,
    values: &[Value],
    effects: &EffectState,
) -> Result<(), String> {
    let destination = blocks.get(&target).ok_or_else(|| {
        format!(
            "branch in block {} targets missing block {}",
            source, target
        )
    })?;
    let types: Vec<EvoIRType> = destination
        .args
        .iter()
        .map(|arg| arg.value.ty.clone())
        .collect();
    validate_effect_boundary(
        &types,
        values,
        effects,
        &format!("edge {} -> {}", source, target),
    )
}

fn validate_effect_boundary(
    types: &[EvoIRType],
    values: &[Value],
    effects: &EffectState,
    context: &str,
) -> Result<(), String> {
    let mut memory_positions: [Vec<usize>; REGION_ORDER.len()] = std::array::from_fn(|_| vec![]);
    for (index, ty) in types.iter().enumerate() {
        if let EvoIRType::Mem(region) = ty {
            memory_positions[region_index(*region)].push(index);
        }
    }
    for (region_index, region) in REGION_ORDER.iter().copied().enumerate() {
        let positions = &memory_positions[region_index];
        if positions.len() > 1 {
            return Err(format!(
                "{} passes mem<{:?}> more than once",
                context, region
            ));
        }
        match (effects.mem[region_index], positions.as_slice()) {
            (None, []) => {}
            (None, _) => {
                return Err(format!(
                    "{} introduces mem<{:?}> without a current token",
                    context, region
                ));
            }
            (Some(_), []) => {
                return Err(format!("{} drops current mem<{:?}> token", context, region));
            }
            (Some(expected), [position]) if values[*position].id == expected => {}
            (Some(expected), [position]) => {
                return Err(format!(
                    "{} passes stale mem<{:?}> token v{}; current token is v{}",
                    context, region, values[*position].id, expected
                ));
            }
            (Some(_), _) => {
                return Err(format!(
                    "{} passes mem<{:?}> more than once",
                    context, region
                ));
            }
        }
    }
    validate_one_effect_boundary(
        types,
        values,
        &EvoIRType::World,
        effects.world,
        "world",
        context,
    )
}

fn validate_one_effect_boundary(
    types: &[EvoIRType],
    values: &[Value],
    token_type: &EvoIRType,
    current: Option<u32>,
    token_name: &str,
    context: &str,
) -> Result<(), String> {
    let positions: Vec<usize> = types
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| (ty == token_type).then_some(index))
        .collect();
    match (current, positions.as_slice()) {
        (None, []) => Ok(()),
        (None, _) => Err(format!(
            "{} introduces a {} token without a current token",
            context, token_name
        )),
        (Some(_), []) => Err(format!(
            "{} drops the current {} token",
            context, token_name
        )),
        (Some(expected), [position]) if values[*position].id == expected => Ok(()),
        (Some(expected), [position]) => Err(format!(
            "{} passes stale {} token v{}; current token is v{}",
            context, token_name, values[*position].id, expected
        )),
        (Some(_), _) => Err(format!(
            "{} passes the current {} token more than once",
            context, token_name
        )),
    }
}

fn validate_reachability(func: &Function, entry: BlockId) -> Result<(), String> {
    let by_id: HashMap<BlockId, &Block> =
        func.blocks.iter().map(|block| (block.id, block)).collect();
    let mut reachable = HashSet::new();
    let mut pending = vec![entry];
    while let Some(block_id) = pending.pop() {
        if !reachable.insert(block_id) {
            continue;
        }
        let Some(block) = by_id.get(&block_id) else {
            return Err(format!("reachable block {} is missing", block_id));
        };
        match &block.terminator {
            Inst::Br(target, _) => pending.push(*target),
            Inst::CondBr(_, true_block, _, false_block, _) => {
                pending.push(*true_block);
                pending.push(*false_block);
            }
            Inst::Ret(_) | Inst::Trap(_) => {}
            _ => {
                return Err(format!("block {} has an invalid terminator", block_id));
            }
        }
    }
    if let Some(block) = func
        .blocks
        .iter()
        .find(|block| !reachable.contains(&block.id))
    {
        return Err(format!(
            "block {} is unreachable from entry block {}",
            block.id, entry
        ));
    }
    Ok(())
}

const REGION_ORDER: [Region; 6] = [
    Region::All,
    Region::Unknown,
    Region::Global,
    Region::Stack,
    Region::Heap,
    Region::Escaped,
];

fn region_index(region: Region) -> usize {
    match region {
        Region::All => 0,
        Region::Unknown => 1,
        Region::Global => 2,
        Region::Stack => 3,
        Region::Heap => 4,
        Region::Escaped => 5,
    }
}

#[derive(Clone, Copy, Default)]
struct EffectState {
    mem: [Option<u32>; REGION_ORDER.len()],
    world: Option<u32>,
}

impl EffectState {
    fn from_block_args(block: &Block) -> Result<Self, String> {
        let mut state = Self::default();
        for arg in &block.args {
            match &arg.value.ty {
                EvoIRType::Mem(region) => {
                    let slot = &mut state.mem[region_index(*region)];
                    if slot.replace(arg.value.id).is_some() {
                        return Err(format!(
                            "block {} has more than one mem<{:?}> argument",
                            block.id, region
                        ));
                    }
                }
                EvoIRType::World => {
                    if let Some(previous) = state.world.replace(arg.value.id) {
                        return Err(format!(
                            "block {} has more than one world argument (previous token v{})",
                            block.id, previous
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(state)
    }

    fn advance(&mut self, inst: &Inst, block: BlockId) -> Result<(), String> {
        let tokens = match inst {
            Inst::Load {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::Store {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::Alloca {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::LifetimeEnd {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::Call {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::IndirectCall {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            } => Some((mem_in, world_in, mem_out, world_out)),
            Inst::PointerValid {
                mem_in, mem_out, ..
            }
            | Inst::AccessInBounds {
                mem_in, mem_out, ..
            } => {
                if mem_in.ty != EvoIRType::Mem(Region::All)
                    || mem_out.ty != EvoIRType::Mem(Region::All)
                {
                    return Err(format!(
                        "memory guard in block {block} must thread mem<all>"
                    ));
                }
                let current = self.mem[region_index(Region::All)];
                if current != Some(mem_in.id) {
                    return Err(format!(
                        "memory guard in block {block} consumes stale mem<all> token v{}",
                        mem_in.id
                    ));
                }
                self.mem[region_index(Region::All)] = Some(mem_out.id);
                return Ok(());
            }
            _ => None,
        };
        let Some((mem_in, world_in, mem_out, world_out)) = tokens else {
            return Ok(());
        };
        let region = match (&mem_in.ty, &mem_out.ty) {
            (EvoIRType::Mem(input), EvoIRType::Mem(output)) if input == output => *input,
            _ => {
                return Err(format!(
                    "effect in block {} has invalid memory token types",
                    block
                ));
            }
        };
        let current_mem = self.mem[region_index(region)];
        if current_mem != Some(mem_in.id) {
            return Err(format!(
                "effect in block {} consumes stale mem<{:?}> token v{}; current token is {}",
                block,
                region,
                mem_in.id,
                current_mem.map_or_else(|| "absent".to_owned(), |id| format!("v{id}"))
            ));
        }
        if self.world != Some(world_in.id) {
            return Err(format!(
                "effect in block {} consumes stale world token v{}; current token is {}",
                block,
                world_in.id,
                self.world
                    .map_or_else(|| "absent".to_owned(), |id| format!("v{id}"))
            ));
        }
        self.mem[region_index(region)] = Some(mem_out.id);
        self.world = Some(world_out.id);
        Ok(())
    }
}

fn validate_instruction_types(inst: &Inst, block: BlockId) -> Result<(), String> {
    match inst {
        Inst::Const(result, _) => expect_bits(&result.ty, "const result", block),
        Inst::Uninitialized(result) => expect_data_type(&result.ty, "uninitialized result", block),
        Inst::Add(result, lhs, rhs)
        | Inst::Sub(result, lhs, rhs)
        | Inst::Mul(result, lhs, rhs)
        | Inst::SRem(result, lhs, rhs)
        | Inst::URem(result, lhs, rhs)
        | Inst::And(result, lhs, rhs)
        | Inst::Or(result, lhs, rhs)
        | Inst::Xor(result, lhs, rhs)
        | Inst::SDiv(result, lhs, rhs)
        | Inst::UDiv(result, lhs, rhs) => {
            expect_same_bits(result, lhs, rhs, "integer arithmetic", block)
        }
        Inst::Shl(result, shifted, amount)
        | Inst::AShr(result, shifted, amount)
        | Inst::LShr(result, shifted, amount) => {
            expect_shift_bits(result, shifted, amount, "shift", block)
        }
        Inst::SignedAddDefined(result, lhs, rhs)
        | Inst::SignedSubDefined(result, lhs, rhs)
        | Inst::SignedMulDefined(result, lhs, rhs) => {
            expect_binary_bits_predicate(result, lhs, rhs, "signed-definedness", block)
        }
        Inst::DivisorNonZero(result, divisor) => {
            expect_unary_bits_predicate(result, divisor, "nonzero-divisor", block)
        }
        Inst::SignedDivRemNoOverflow(result, lhs, rhs) => {
            expect_binary_bits_predicate(result, lhs, rhs, "signed-div-rem-overflow", block)
        }
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            ..
        } => expect_shift_predicate(result, shifted, amount, "shift-count", block),
        Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            ..
        } => expect_shift_predicate(
            result,
            shifted,
            amount,
            "signed-left-shift-definedness",
            block,
        ),
        Inst::UbAssume(condition, _) => {
            if condition.ty == EvoIRType::Bits(1) {
                Ok(())
            } else {
                Err(format!(
                    "ub.assume in block {} uses {:?}, expected bits<1>",
                    block, condition.ty
                ))
            }
        }
        Inst::ICmp(result, operation, lhs, rhs) => {
            if result.ty != EvoIRType::Bits(1) {
                return Err(format!(
                    "icmp result in block {} has type {:?}, expected bits<1>",
                    block, result.ty
                ));
            }
            if lhs.ty != rhs.ty {
                return Err(format!(
                    "icmp operands in block {} have different types {:?} and {:?}",
                    block, lhs.ty, rhs.ty
                ));
            }
            match &lhs.ty {
                EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
                    if matches!(operation, CmpOp::Eq | CmpOp::Ne) =>
                {
                    Ok(())
                }
                _ => expect_bits(&lhs.ty, "icmp operand", block),
            }
        }
        Inst::PtrNull(result) => expect_pointer_value(&result.ty, "null-pointer result", block),
        Inst::GlobalAddr { result, .. } => {
            expect_pointer(&result.ty, "global-address result", block)
        }
        Inst::FuncAddr { result, .. } => match result.ty {
            EvoIRType::FnPtr { .. } => Ok(()),
            _ => Err(format!(
                "function-address result in block {block} has type {:?}, expected fnptr<Sig>",
                result.ty
            )),
        },
        Inst::Alloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            layout,
            ..
        } => {
            expect_pointer(&result.ty, "alloca result", block)?;
            validate_layout(*layout, "alloca", block)?;
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "alloca", block)
        }
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            mem_out,
            world_out,
        } => {
            expect_pointer(&pointer.ty, "lifetime.end pointer", block)?;
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "lifetime.end", block)
        }
        Inst::PointerValid {
            mem_in,
            pointer,
            mem_out,
        } => {
            expect_pointer(&pointer.ty, "pointer_valid pointer", block)?;
            expect_day7_memory_tokens(mem_in, mem_out, "pointer_valid", block)
        }
        Inst::AccessInBounds {
            mem_in,
            pointer,
            mem_out,
            layout,
        } => {
            expect_pointer(&pointer.ty, "access_in_bounds pointer", block)?;
            validate_layout(*layout, "access_in_bounds", block)?;
            expect_day7_memory_tokens(mem_in, mem_out, "access_in_bounds", block)
        }
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            expect_pointer(&address.ty, "load address", block)?;
            expect_data_type(&result.ty, "load result", block)?;
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "load", block)
        }
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            expect_pointer(&address.ty, "store address", block)?;
            expect_data_type(&value.ty, "stored value", block)?;
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "store", block)
        }
        Inst::Call {
            results,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            for argument in args {
                expect_data_type(&argument.ty, "ordinary call argument", block)?;
            }
            for result in results {
                expect_data_type(&result.ty, "call result", block)?;
            }
            expect_effect_tokens(mem_in, world_in, mem_out, world_out, "call", block)
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
            let EvoIRType::FnPtr {
                params,
                results: returns,
            } = &callee.ty
            else {
                return Err(format!(
                    "indirect call callee in block {block} has type {:?}, expected fnptr<Sig>",
                    callee.ty
                ));
            };
            let mut actual_params = args
                .iter()
                .map(|value| value.ty.clone())
                .collect::<Vec<_>>();
            actual_params.push(mem_in.ty.clone());
            actual_params.push(world_in.ty.clone());
            let mut actual_returns = results
                .iter()
                .map(|value| value.ty.clone())
                .collect::<Vec<_>>();
            actual_returns.push(mem_out.ty.clone());
            actual_returns.push(world_out.ty.clone());
            if actual_params != *params || actual_returns != *returns {
                return Err(format!(
                    "E3017: indirect-call-signature-mismatch in block {block}"
                ));
            }
            expect_effect_tokens(mem_in, world_in, mem_out, world_out, "indirect call", block)
        }
        Inst::PtrOffset {
            result,
            base,
            index,
            scale,
            ..
        } => {
            expect_pointer(&result.ty, "pointer-offset result", block)?;
            expect_pointer(&base.ty, "pointer-offset base", block)?;
            if result.ty != base.ty {
                return Err(format!(
                    "pointer-offset result and base in block {} have different types {:?} and {:?}",
                    block, result.ty, base.ty
                ));
            }
            if *scale == 0 {
                return Err(format!(
                    "pointer-offset scale in block {block} must be positive"
                ));
            }
            expect_bits(&index.ty, "pointer-offset index", block)
        }
        Inst::SExt(result, source, target) => {
            validate_conversion(result, source, target, Conversion::SignExtend, block)
        }
        Inst::ZExt(result, source, target) => {
            validate_conversion(result, source, target, Conversion::ZeroExtend, block)
        }
        Inst::Trunc(result, source, target) => {
            validate_conversion(result, source, target, Conversion::Truncate, block)
        }
        Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) | Inst::Trap(..) => Err(format!(
            "terminator found in instruction list of block {}",
            block
        )),
    }
}

fn validate_layout(layout: AccessLayout, operation: &str, block: BlockId) -> Result<(), String> {
    if layout.size_bytes == 0 || layout.align_bytes == 0 || !layout.align_bytes.is_power_of_two() {
        return Err(format!(
            "{operation} in block {block} has invalid size/alignment {layout:?}"
        ));
    }
    Ok(())
}

fn expect_day7_memory_tokens(
    mem_in: &Value,
    mem_out: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if mem_in.ty == EvoIRType::Mem(Region::All) && mem_out.ty == EvoIRType::Mem(Region::All) {
        Ok(())
    } else {
        Err(format!("{operation} in block {block} must thread mem<all>"))
    }
}

fn expect_day7_effect_tokens(
    mem_in: &Value,
    world_in: &Value,
    mem_out: &Value,
    world_out: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    expect_effect_tokens(mem_in, world_in, mem_out, world_out, operation, block)?;
    expect_day7_memory_tokens(mem_in, mem_out, operation, block)
}

fn expect_same_bits(
    result: &Value,
    lhs: &Value,
    rhs: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != lhs.ty || lhs.ty != rhs.ty {
        return Err(format!(
            "{} in block {} has inconsistent types: result {:?}, lhs {:?}, rhs {:?}",
            operation, block, result.ty, lhs.ty, rhs.ty
        ));
    }
    expect_bits(&result.ty, operation, block)
}

fn expect_shift_bits(
    result: &Value,
    shifted: &Value,
    amount: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != shifted.ty {
        return Err(format!(
            "{} in block {} has different result and shifted types {:?} and {:?}",
            operation, block, result.ty, shifted.ty
        ));
    }
    expect_bits(&shifted.ty, &format!("{operation} shifted operand"), block)?;
    expect_bits(&amount.ty, &format!("{operation} amount"), block)
}

fn expect_unary_bits_predicate(
    result: &Value,
    operand: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != EvoIRType::Bits(1) {
        return Err(format!(
            "{} result in block {} has type {:?}, expected bits<1>",
            operation, block, result.ty
        ));
    }
    expect_bits(&operand.ty, &format!("{operation} operand"), block)
}

fn expect_binary_bits_predicate(
    result: &Value,
    lhs: &Value,
    rhs: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != EvoIRType::Bits(1) {
        return Err(format!(
            "{} result in block {} has type {:?}, expected bits<1>",
            operation, block, result.ty
        ));
    }
    if lhs.ty != rhs.ty {
        return Err(format!(
            "{} operands in block {} have different types {:?} and {:?}",
            operation, block, lhs.ty, rhs.ty
        ));
    }
    expect_bits(&lhs.ty, &format!("{operation} operand"), block)
}

fn expect_shift_predicate(
    result: &Value,
    shifted: &Value,
    amount: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != EvoIRType::Bits(1) {
        return Err(format!(
            "{} result in block {} has type {:?}, expected bits<1>",
            operation, block, result.ty
        ));
    }
    expect_bits(&shifted.ty, &format!("{operation} shifted operand"), block)?;
    expect_bits(&amount.ty, &format!("{operation} amount"), block)
}

fn expect_bits(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    match ty {
        EvoIRType::Bits(width) if *width > 0 => Ok(()),
        _ => Err(format!(
            "{} in block {} has type {:?}, expected non-empty bits<N>",
            role, block, ty
        )),
    }
}

fn expect_pointer(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    if matches!(ty, EvoIRType::Ptr(_)) {
        Ok(())
    } else {
        Err(format!(
            "{} in block {} has type {:?}, expected pointer",
            role, block, ty
        ))
    }
}

fn expect_pointer_value(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    if matches!(ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }) {
        Ok(())
    } else {
        Err(format!(
            "{} in block {} has type {:?}, expected pointer or fnptr",
            role, block, ty
        ))
    }
}

fn expect_data_type(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    ensure_value_type(ty, &format!("{role} in block {block}"))?;
    if contains_effect_token(ty)? {
        Err(format!(
            "{} in block {} contains a memory/world effect token",
            role, block
        ))
    } else {
        Ok(())
    }
}

/// Maximum accepted nesting for an attacker-provided EvoIR type.
pub const MAX_TYPE_NESTING: usize = 256;

/// Validate the recursive shape of one EvoIR type without recursive descent.
///
/// `Void` and the empty tuple are well-formed type spellings, although
/// [`validate_value_type`] rejects both when a concrete SSA value is required.
pub fn validate_type_well_formed(ty: &EvoIRType) -> Result<(), String> {
    let mut pending = vec![(ty, 0_usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_TYPE_NESTING {
            return Err(format!(
                "type nesting exceeds the limit of {MAX_TYPE_NESTING}"
            ));
        }
        match current {
            EvoIRType::Bits(0) => return Err("zero-width bits type is invalid".to_owned()),
            EvoIRType::Vec(0, _) => return Err("zero-lane vector type is invalid".to_owned()),
            EvoIRType::Vec(_, element) => {
                let forbidden_element = matches!(
                    element.as_ref(),
                    EvoIRType::Void | EvoIRType::Mem(_) | EvoIRType::World
                ) || matches!(element.as_ref(), EvoIRType::Tuple(elements) if elements.is_empty());
                if forbidden_element {
                    return Err("vector element is not a data value type".to_owned());
                }
                pending.push((element, depth + 1));
            }
            EvoIRType::Tuple(elements) => {
                pending.extend(elements.iter().map(|element| (element, depth + 1)));
            }
            EvoIRType::FnPtr { params, results } => {
                if params.len() < 2 || results.len() < 2 {
                    return Err("fnptr signature lacks trailing mem<R>, world interface".to_owned());
                }
                let parameter_split = params.len() - 2;
                let result_split = results.len() - 2;
                let region = match (&params[parameter_split], &params[parameter_split + 1]) {
                    (EvoIRType::Mem(region), EvoIRType::World) => *region,
                    _ => return Err("fnptr parameters must end with mem<R>, world".to_owned()),
                };
                if !matches!(
                    (&results[result_split], &results[result_split + 1]),
                    (EvoIRType::Mem(output), EvoIRType::World) if *output == region
                ) {
                    return Err("fnptr results must end with the same mem<R>, world".to_owned());
                }
                if params[..parameter_split]
                    .iter()
                    .chain(&results[..result_split])
                    .any(|ty| matches!(ty, EvoIRType::Mem(_) | EvoIRType::World | EvoIRType::Void))
                {
                    return Err(
                        "fnptr ordinary parameters/results cannot be effect tokens or void"
                            .to_owned(),
                    );
                }
                pending.extend(
                    params
                        .iter()
                        .chain(results)
                        .map(|element| (element, depth + 1)),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validate a type at a concrete SSA value/interface position.
pub fn validate_value_type(ty: &EvoIRType) -> Result<(), String> {
    validate_type_well_formed(ty)?;
    let mut pending = vec![(ty, 0_usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_TYPE_NESTING {
            return Err(format!(
                "type nesting exceeds the limit of {MAX_TYPE_NESTING}"
            ));
        }
        match current {
            EvoIRType::Void => return Err("void is not an SSA value type".to_owned()),
            EvoIRType::Tuple(elements) if elements.is_empty() => {
                return Err(
                    "the empty tuple is void-equivalent and not an SSA value type".to_owned(),
                );
            }
            EvoIRType::Tuple(elements) => {
                pending.extend(elements.iter().map(|element| (element, depth + 1)));
            }
            EvoIRType::Vec(_, element) => pending.push((element, depth + 1)),
            EvoIRType::FnPtr { .. } => {}
            _ => {}
        }
    }
    Ok(())
}

fn ensure_value_type(ty: &EvoIRType, role: &str) -> Result<(), String> {
    validate_value_type(ty).map_err(|message| format!("{role}: {message}"))
}

fn contains_effect_token(ty: &EvoIRType) -> Result<bool, String> {
    validate_type_well_formed(ty)?;
    let mut pending = vec![(ty, 0_usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_TYPE_NESTING {
            return Err(format!(
                "type nesting exceeds the limit of {MAX_TYPE_NESTING}"
            ));
        }
        match current {
            EvoIRType::Mem(_) | EvoIRType::World => return Ok(true),
            EvoIRType::Tuple(elements) => {
                pending.extend(elements.iter().map(|element| (element, depth + 1)));
            }
            EvoIRType::Vec(_, element) => pending.push((element, depth + 1)),
            _ => {}
        }
    }
    Ok(false)
}

fn expect_effect_tokens(
    mem_in: &Value,
    world_in: &Value,
    mem_out: &Value,
    world_out: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    let input_region = match mem_in.ty {
        EvoIRType::Mem(region) => region,
        _ => {
            return Err(format!(
                "{} input memory token in block {} has type {:?}, expected mem<R>",
                operation, block, mem_in.ty
            ));
        }
    };
    let output_region = match mem_out.ty {
        EvoIRType::Mem(region) => region,
        _ => {
            return Err(format!(
                "{} output memory token in block {} has type {:?}, expected mem<R>",
                operation, block, mem_out.ty
            ));
        }
    };
    if input_region != output_region {
        return Err(format!(
            "{} in block {} changes memory region from {:?} to {:?}",
            operation, block, input_region, output_region
        ));
    }
    for (direction, token) in [("input", world_in), ("output", world_out)] {
        if token.ty != EvoIRType::World {
            return Err(format!(
                "{} {} world token in block {} has type {:?}, expected world",
                operation, direction, block, token.ty
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Conversion {
    SignExtend,
    ZeroExtend,
    Truncate,
}

fn validate_conversion(
    result: &Value,
    source: &Value,
    target: &EvoIRType,
    conversion: Conversion,
    block: BlockId,
) -> Result<(), String> {
    if &result.ty != target {
        return Err(format!(
            "conversion result in block {} has type {:?}, but target is {:?}",
            block, result.ty, target
        ));
    }
    let EvoIRType::Bits(source_width) = source.ty else {
        return Err(format!(
            "conversion source in block {} has non-integer type {:?}",
            block, source.ty
        ));
    };
    let EvoIRType::Bits(target_width) = target else {
        return Err(format!(
            "conversion target in block {} has non-integer type {:?}",
            block, target
        ));
    };
    let widths_valid = match conversion {
        Conversion::SignExtend | Conversion::ZeroExtend => target_width >= &source_width,
        Conversion::Truncate => target_width <= &source_width,
    };
    if !widths_valid {
        return Err(format!(
            "invalid conversion width in block {}: bits<{}> to bits<{}>",
            block, source_width, target_width
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------------

/// Resource limits for deterministic EvoIR execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterpreterLimits {
    pub step_budget: u64,
}

// Keep the two interpreter frontends on the same explicit process-resource
// policy until that policy gains a shared configuration authority. These are
// host-safety limits, not properties of the source program or EvoIR module.
const MAX_MEMORY_OBJECTS: u64 = 65_536;
const MAX_MEMORY_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MEMORY_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_VOLATILE_EVENTS: u64 = 1_048_576;

impl InterpreterLimits {
    #[must_use]
    pub const fn new(step_budget: u64) -> Self {
        Self { step_budget }
    }
}

/// A typed runtime value. `raw_bits` preserves the signless EvoIR bit pattern;
/// memory and world tokens use it as an opaque host-side identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeValue {
    pub ty: EvoIRType,
    pub raw_bits: u64,
}

impl RuntimeValue {
    #[must_use]
    pub const fn new(ty: EvoIRType, raw_bits: u64) -> Self {
        Self { ty, raw_bits }
    }

    #[must_use]
    pub const fn bits(width: u16, raw_bits: u64) -> Self {
        Self::new(EvoIRType::Bits(width), raw_bits)
    }
}

/// Private runtime state for an SSA value. Public entry arguments and
/// successful results are always initialized; poison exists only while an
/// interpreter is executing validated EvoIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalValue {
    Bits(u64),
    Pointer(Pointer),
    FunctionPointer(PointerValue),
    Uninitialized,
}

/// Successful EvoIR execution together with the exact charged step count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpreterOutcome {
    pub return_values: Vec<RuntimeValue>,
    pub steps: u64,
    pub memory: MemoryState,
    pub world: WorldState,
}

struct ExecutionState {
    memory: MemoryState,
    world: WorldState,
    globals: HashMap<String, Pointer>,
    functions: HashMap<String, u64>,
    access_guards: HashMap<u64, (Pointer, AccessLayout)>,
    next_token: u64,
}

impl ExecutionState {
    fn for_target(target: &TargetDataLayout) -> Result<Self, InterpreterError> {
        let config = MemoryConfig::for_target(
            target,
            MemoryLimits {
                max_objects: MAX_MEMORY_OBJECTS,
                max_object_bytes: MAX_MEMORY_OBJECT_BYTES,
                max_total_bytes: MAX_MEMORY_TOTAL_BYTES,
            },
        )
        .map_err(|error| InterpreterError::MemorySetup { error })?;
        let memory =
            MemoryState::new(config).map_err(|error| InterpreterError::MemorySetup { error })?;
        let world = WorldState::new(WorldLimits {
            max_volatile_events: MAX_VOLATILE_EVENTS,
        });
        Ok(Self {
            memory,
            world,
            globals: HashMap::new(),
            functions: HashMap::new(),
            access_guards: HashMap::new(),
            next_token: 1,
        })
    }

    fn fresh_token(&mut self) -> Result<u64, InterpreterError> {
        let token = self.next_token;
        self.next_token = token
            .checked_add(1)
            .ok_or_else(|| InterpreterError::InvalidModule {
                message: "runtime effect-token identity exhausted".to_owned(),
            })?;
        Ok(token)
    }
}

/// A defined failure from EvoIR validation or execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpreterError {
    MemorySetup {
        error: MemoryFault,
    },
    MemoryAccess {
        block: BlockId,
        error: MemoryFault,
    },
    InvalidModule {
        message: String,
    },
    InvalidFunction {
        message: String,
    },
    MissingEntryFunction {
        name: String,
    },
    EntryArgumentCount {
        expected: usize,
        actual: usize,
    },
    EntryArgumentType {
        index: usize,
        expected: EvoIRType,
        actual: EvoIRType,
    },
    InvalidEntryArgument {
        index: usize,
        message: String,
    },
    UnsupportedRuntimeType {
        index: usize,
        ty: EvoIRType,
    },
    StepBudgetExceeded {
        budget: u64,
        steps: u64,
        block: BlockId,
    },
    MissingBlock {
        block: BlockId,
    },
    MissingValue {
        block: BlockId,
        value: u32,
    },
    UninitializedRead {
        block: BlockId,
    },
    MissingReturn {
        block: BlockId,
    },
    UnsupportedBitWidth {
        block: BlockId,
        width: u16,
    },
    DivisionByZero {
        block: BlockId,
        operation: &'static str,
    },
    RemainderByZero {
        block: BlockId,
        operation: &'static str,
    },
    SignedDivisionOverflow {
        block: BlockId,
        width: u16,
    },
    SignedRemainderOverflow {
        block: BlockId,
        width: u16,
    },
    UndefinedBehavior {
        block: BlockId,
        diagnostic: &'static str,
        message: &'static str,
    },
    UnsupportedInstruction {
        block: BlockId,
        operation: &'static str,
    },
    CallThroughNullFunctionPointer {
        block: BlockId,
    },
    CallThroughNonFunctionPointer {
        block: BlockId,
    },
    IndirectCallDynamicSignatureMismatch {
        block: BlockId,
    },
}

impl InterpreterError {
    /// Stable diagnostic code aligned with the C-HIR scalar interpreter.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MemorySetup { .. } => "E3007",
            Self::MemoryAccess { error, .. } => match error.runtime_diagnostic() {
                Some(diagnostic) => diagnostic.code,
                None => "E3007",
            },
            Self::UninitializedRead { .. } => "E3001",
            Self::MissingReturn { .. } => "E3007",
            Self::DivisionByZero { .. } => "E3002",
            Self::RemainderByZero { .. } => "E3003",
            Self::SignedDivisionOverflow { .. } | Self::SignedRemainderOverflow { .. } => "E3004",
            Self::UndefinedBehavior { diagnostic, .. } => diagnostic,
            Self::StepBudgetExceeded { .. } => "E3008",
            Self::CallThroughNullFunctionPointer { .. } => "E3015",
            Self::CallThroughNonFunctionPointer { .. } => "E3016",
            Self::IndirectCallDynamicSignatureMismatch { .. } => "E3019",
            _ => "E3007",
        }
    }

    /// Stable diagnostic message aligned with the C-HIR scalar interpreter.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::MemorySetup { error } => {
                format!("interpreter-memory-setup-failed: {error}")
            }
            Self::MemoryAccess { error, .. } => error.runtime_diagnostic().map_or_else(
                || format!("interpreter-memory-failed: {error}"),
                |diagnostic| diagnostic.message.to_owned(),
            ),
            Self::UninitializedRead { .. } => "read-of-uninitialized-object".to_owned(),
            Self::MissingReturn { .. } => "scalar-function-fell-off-end".to_owned(),
            Self::DivisionByZero { .. } => "integer-division-by-zero".to_owned(),
            Self::RemainderByZero { .. } => "integer-remainder-by-zero".to_owned(),
            Self::SignedDivisionOverflow { .. } | Self::SignedRemainderOverflow { .. } => {
                "signed-integer-overflow".to_owned()
            }
            Self::UndefinedBehavior { message, .. } => (*message).to_owned(),
            Self::StepBudgetExceeded { .. } => "interpreter-fuel-exhausted".to_owned(),
            Self::CallThroughNullFunctionPointer { .. } => {
                "call-through-null-function-pointer".to_owned()
            }
            Self::CallThroughNonFunctionPointer { .. } => {
                "call-through-non-function-pointer".to_owned()
            }
            Self::IndirectCallDynamicSignatureMismatch { .. } => {
                "indirect-call-dynamic-signature-mismatch".to_owned()
            }
            _ => self.to_string(),
        }
    }
}

impl fmt::Display for InterpreterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MemorySetup { error } => {
                write!(formatter, "EvoIR memory setup failed: {error}")
            }
            Self::MemoryAccess { block, error } => {
                write!(
                    formatter,
                    "EvoIR memory access failed in block {block}: {error}"
                )
            }
            Self::InvalidModule { message } => write!(formatter, "invalid EvoIR module: {message}"),
            Self::InvalidFunction { message } => write!(formatter, "invalid EvoIR: {message}"),
            Self::MissingEntryFunction { name } => {
                write!(formatter, "missing EvoIR entry function '{name}'")
            }
            Self::EntryArgumentCount { expected, actual } => write!(
                formatter,
                "entry argument count mismatch: expected {expected}, got {actual}"
            ),
            Self::EntryArgumentType {
                index,
                expected,
                actual,
            } => write!(
                formatter,
                "entry argument {index} has type {actual:?}, expected {expected:?}"
            ),
            Self::InvalidEntryArgument { index, message } => {
                write!(formatter, "entry argument {index} is invalid: {message}")
            }
            Self::UnsupportedRuntimeType { index, ty } => write!(
                formatter,
                "entry argument {index} has unsupported runtime type {ty:?}"
            ),
            Self::StepBudgetExceeded {
                budget,
                steps,
                block,
            } => write!(
                formatter,
                "EvoIR step budget {budget} exceeded after {steps} steps in block {block}"
            ),
            Self::MissingBlock { block } => write!(formatter, "missing EvoIR block {block}"),
            Self::MissingValue { block, value } => {
                write!(formatter, "missing EvoIR value v{value} in block {block}")
            }
            Self::UninitializedRead { block } => write!(
                formatter,
                "E3001: read-of-uninitialized-object (uninitialized value in block {block})"
            ),
            Self::MissingReturn { block } => write!(
                formatter,
                "E3007: scalar-function-fell-off-end (trap in block {block})"
            ),
            Self::UnsupportedBitWidth { block, width } => write!(
                formatter,
                "EvoIR interpreter does not support bits<{width}> in block {block}"
            ),
            Self::DivisionByZero { block, operation } => {
                write!(formatter, "{operation} by zero in block {block}")
            }
            Self::RemainderByZero { block, operation } => {
                write!(formatter, "{operation} by zero in block {block}")
            }
            Self::SignedDivisionOverflow { block, width } => write!(
                formatter,
                "signed division overflow for bits<{width}> in block {block}"
            ),
            Self::SignedRemainderOverflow { block, width } => write!(
                formatter,
                "signed remainder overflow for bits<{width}> in block {block}"
            ),
            Self::UndefinedBehavior {
                block,
                diagnostic,
                message,
            } => write!(
                formatter,
                "{diagnostic}: {message} (ub.assume failed in block {block})"
            ),
            Self::UnsupportedInstruction { block, operation } => write!(
                formatter,
                "EvoIR interpreter does not support {operation} in block {block}"
            ),
            Self::CallThroughNullFunctionPointer { block } => write!(
                formatter,
                "E3015: call-through-null-function-pointer in block {block}"
            ),
            Self::CallThroughNonFunctionPointer { block } => write!(
                formatter,
                "E3016: call-through-non-function-pointer in block {block}"
            ),
            Self::IndirectCallDynamicSignatureMismatch { block } => write!(
                formatter,
                "E3019: indirect-call-dynamic-signature-mismatch in block {block}"
            ),
        }
    }
}

impl std::error::Error for InterpreterError {}

/// Interpret a validated function with explicit entry arguments and limits.
///
/// Each non-terminator instruction and each terminator costs one step. The
/// implementation advances an explicit block program counter, so loops and
/// backedges consume bounded host stack.
pub fn interpret_function(
    func: &Function,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    interpret_function_with_target(func, entry_args, &target, limits)
}

/// Interpret one function against an explicit target data layout.
///
/// This is the authoritative target-aware entry point. It constructs exactly
/// one memory/world state pair for the complete top-level invocation.
pub fn interpret_function_with_target(
    func: &Function,
    entry_args: &[RuntimeValue],
    target: &TargetDataLayout,
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    let state = ExecutionState::for_target(target)?;
    interpret_function_in_state(func, entry_args, limits, state)
}

fn interpret_function_in_state(
    func: &Function,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
    mut state: ExecutionState,
) -> Result<InterpreterOutcome, InterpreterError> {
    validate_ssa(func).map_err(|message| InterpreterError::InvalidFunction { message })?;
    let entry = func
        .blocks
        .first()
        .ok_or_else(|| InterpreterError::InvalidFunction {
            message: format!("function '{}' has no entry block", func.name),
        })?;
    if entry_args.len() != entry.args.len() {
        return Err(InterpreterError::EntryArgumentCount {
            expected: entry.args.len(),
            actual: entry_args.len(),
        });
    }

    let blocks: HashMap<BlockId, &Block> =
        func.blocks.iter().map(|block| (block.id, block)).collect();
    let mut values = HashMap::new();
    let mut incoming = entry_args
        .iter()
        .zip(&entry.args)
        .enumerate()
        .map(|(index, (value, argument))| {
            normalize_runtime_input(value, &argument.value.ty, index, entry.id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut program_counter = entry.id;
    let mut steps = 0;
    let (memory_frame, memory_scope) = begin_function_memory(&mut state, func, entry.id)?;

    loop {
        let block =
            blocks
                .get(&program_counter)
                .copied()
                .ok_or(InterpreterError::MissingBlock {
                    block: program_counter,
                })?;
        if incoming.len() != block.args.len() {
            return Err(InterpreterError::EntryArgumentCount {
                expected: block.args.len(),
                actual: incoming.len(),
            });
        }
        for (argument, runtime_value) in block.args.iter().zip(incoming.drain(..)) {
            values.insert(argument.value.id, runtime_value);
        }

        for inst in &block.insts {
            charge_step(&mut steps, limits, block.id)?;
            execute_instruction(
                inst,
                block.id,
                &mut values,
                &mut state,
                memory_frame,
                memory_scope,
            )?;
        }

        charge_step(&mut steps, limits, block.id)?;
        match &block.terminator {
            Inst::Br(target, arguments) => {
                incoming = copy_values(arguments, block.id, &values)?;
                program_counter = *target;
            }
            Inst::CondBr(condition, true_block, true_arguments, false_block, false_arguments) => {
                let condition_value = read_value(&values, condition, block.id)?;
                if condition_value != 0 {
                    incoming = copy_values(true_arguments, block.id, &values)?;
                    program_counter = *true_block;
                } else {
                    incoming = copy_values(false_arguments, block.id, &values)?;
                    program_counter = *false_block;
                }
            }
            Inst::Ret(returned) => {
                let internal = copy_values(returned, block.id, &values)?;
                if let Some(memory_frame) = memory_frame {
                    state.memory = state.memory.end_frame(memory_frame).map_err(|error| {
                        InterpreterError::MemoryAccess {
                            block: block.id,
                            error,
                        }
                    })?;
                }
                let return_values = public_return_values(
                    internal,
                    returned.iter().map(|value| value.ty.clone()).collect(),
                    block.id,
                )?;
                return Ok(InterpreterOutcome {
                    return_values,
                    steps,
                    memory: state.memory,
                    world: state.world,
                });
            }
            Inst::Trap(TrapReason::ScalarFunctionFellOffEnd) => {
                return Err(InterpreterError::MissingReturn { block: block.id });
            }
            _ => {
                return Err(InterpreterError::InvalidFunction {
                    message: format!("block {} has a non-terminator", block.id),
                });
            }
        }
    }
}

#[derive(Clone)]
struct CallReturnBinding {
    results: Vec<Value>,
    mem_out: Value,
    world_out: Value,
}

struct ModuleFrame {
    function_index: usize,
    block: BlockId,
    incoming: Vec<InternalValue>,
    values: HashMap<u32, InternalValue>,
    next_instruction: usize,
    entered: bool,
    return_binding: Option<CallReturnBinding>,
    memory_frame: Option<FrameId>,
    memory_scope: Option<ScopeId>,
}

impl ModuleFrame {
    fn new(
        function_index: usize,
        block: BlockId,
        incoming: Vec<InternalValue>,
        return_binding: Option<CallReturnBinding>,
        memory_frame: Option<FrameId>,
        memory_scope: Option<ScopeId>,
    ) -> Self {
        Self {
            function_index,
            block,
            incoming,
            values: HashMap::new(),
            next_instruction: 0,
            entered: false,
            return_binding,
            memory_frame,
            memory_scope,
        }
    }
}

fn function_uses_automatic_memory(function: &Function) -> bool {
    function.blocks.iter().any(|block| {
        block.insts.iter().any(|instruction| {
            matches!(instruction, Inst::Alloca { .. } | Inst::LifetimeEnd { .. })
        })
    })
}

fn begin_function_memory(
    state: &mut ExecutionState,
    function: &Function,
    block: BlockId,
) -> Result<(Option<FrameId>, Option<ScopeId>), InterpreterError> {
    if !function_uses_automatic_memory(function) {
        return Ok((None, None));
    }
    let (memory, frame, scope) = state
        .memory
        .begin_frame()
        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
    state.memory = memory;
    Ok((Some(frame), Some(scope)))
}

enum FrameAction {
    Instruction(Inst),
    Terminator(Inst),
}

/// Interpret one entry function in a validated module.
///
/// Function calls push explicit heap-backed frames. All instructions and
/// terminators in every frame debit the same step budget.
pub fn interpret_module(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    interpret_module_with_target(module, entry_name, entry_args, &target, limits)
}

/// Interpret one module entry against an explicit target data layout.
///
/// The state is created once here. Explicit call frames share the single state
/// owned by the module execution core; frames never construct or clone it.
pub fn interpret_module_with_target(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    target: &TargetDataLayout,
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    let state = ExecutionState::for_target(target)?;
    interpret_module_in_state(module, entry_name, entry_args, limits, state)
}

fn interpret_module_in_state(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
    mut state: ExecutionState,
) -> Result<InterpreterOutcome, InterpreterError> {
    validate_module(module).map_err(|message| InterpreterError::InvalidModule { message })?;
    initialize_globals(module, &mut state)?;
    let function_indices: HashMap<&str, usize> = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.as_str(), index))
        .collect();
    state.functions = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| {
            let identity = u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "function provenance identity exhausted".to_owned(),
                })?;
            Ok((function.name.clone(), identity))
        })
        .collect::<Result<_, _>>()?;
    let entry_index = function_indices.get(entry_name).copied().ok_or_else(|| {
        InterpreterError::MissingEntryFunction {
            name: entry_name.to_owned(),
        }
    })?;
    let entry_function = &module.functions[entry_index];
    let entry_block =
        entry_function
            .blocks
            .first()
            .ok_or_else(|| InterpreterError::InvalidModule {
                message: format!("function '{}' has no entry block", entry_function.name),
            })?;
    if entry_args.len() != entry_block.args.len() {
        return Err(InterpreterError::EntryArgumentCount {
            expected: entry_block.args.len(),
            actual: entry_args.len(),
        });
    }
    let incoming = entry_args
        .iter()
        .zip(&entry_block.args)
        .enumerate()
        .map(|(index, (value, argument))| {
            normalize_runtime_input(value, &argument.value.ty, index, entry_block.id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (entry_frame, entry_scope) =
        begin_function_memory(&mut state, entry_function, entry_block.id)?;
    let mut frames = vec![ModuleFrame::new(
        entry_index,
        entry_block.id,
        incoming,
        None,
        entry_frame,
        entry_scope,
    )];
    let mut steps = 0;

    loop {
        let frame_index =
            frames
                .len()
                .checked_sub(1)
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "interpreter frame stack became empty".to_owned(),
                })?;
        if !frames[frame_index].entered {
            enter_module_frame(module, &mut frames[frame_index])?;
        }
        let action = module_frame_action(module, &frames[frame_index])?;
        let block_id = frames[frame_index].block;
        charge_step(&mut steps, limits, block_id)?;

        match action {
            FrameAction::Instruction(instruction) => {
                frames[frame_index].next_instruction += 1;
                if let Inst::Call {
                    results,
                    callee,
                    args,
                    mem_in,
                    world_in,
                    mem_out,
                    world_out,
                } = instruction
                {
                    let mut call_arguments =
                        read_call_values(&args, block_id, &frames[frame_index].values)?;
                    call_arguments.push(read_internal(
                        &frames[frame_index].values,
                        &mem_in,
                        block_id,
                    )?);
                    call_arguments.push(read_internal(
                        &frames[frame_index].values,
                        &world_in,
                        block_id,
                    )?);
                    let callee_index =
                        function_indices
                            .get(callee.as_str())
                            .copied()
                            .ok_or_else(|| InterpreterError::InvalidModule {
                                message: format!("call targets missing function '{callee}'"),
                            })?;
                    let callee_entry =
                        module.functions[callee_index]
                            .blocks
                            .first()
                            .ok_or_else(|| InterpreterError::InvalidModule {
                                message: format!("callee '{callee}' has no entry block"),
                            })?;
                    let (memory_frame, memory_scope) = begin_function_memory(
                        &mut state,
                        &module.functions[callee_index],
                        block_id,
                    )?;
                    frames.push(ModuleFrame::new(
                        callee_index,
                        callee_entry.id,
                        call_arguments,
                        Some(CallReturnBinding {
                            results,
                            mem_out,
                            world_out,
                        }),
                        memory_frame,
                        memory_scope,
                    ));
                } else if let Inst::IndirectCall {
                    results,
                    callee,
                    args,
                    mem_in,
                    world_in,
                    mem_out,
                    world_out,
                } = instruction
                {
                    let identity =
                        match read_internal(&frames[frame_index].values, &callee, block_id)? {
                            InternalValue::FunctionPointer(PointerValue::Function(identity)) => {
                                identity
                            }
                            InternalValue::FunctionPointer(PointerValue::Null) => {
                                return Err(InterpreterError::CallThroughNullFunctionPointer {
                                    block: block_id,
                                });
                            }
                            InternalValue::FunctionPointer(PointerValue::Object(_))
                            | InternalValue::Pointer(_)
                            | InternalValue::Bits(_) => {
                                return Err(InterpreterError::CallThroughNonFunctionPointer {
                                    block: block_id,
                                });
                            }
                            InternalValue::Uninitialized => {
                                return Err(InterpreterError::UninitializedRead {
                                    block: block_id,
                                });
                            }
                        };
                    let callee_index = identity
                        .checked_sub(1)
                        .and_then(|identity| usize::try_from(identity).ok())
                        .filter(|index| *index < module.functions.len())
                        .ok_or(InterpreterError::CallThroughNonFunctionPointer {
                            block: block_id,
                        })?;
                    let dynamic_type = function_pointer_type(&module.functions[callee_index])
                        .map_err(|message| InterpreterError::InvalidModule { message })?;
                    if dynamic_type != callee.ty {
                        return Err(InterpreterError::IndirectCallDynamicSignatureMismatch {
                            block: block_id,
                        });
                    }
                    let mut call_arguments =
                        read_call_values(&args, block_id, &frames[frame_index].values)?;
                    call_arguments.push(read_internal(
                        &frames[frame_index].values,
                        &mem_in,
                        block_id,
                    )?);
                    call_arguments.push(read_internal(
                        &frames[frame_index].values,
                        &world_in,
                        block_id,
                    )?);
                    let callee_entry =
                        module.functions[callee_index]
                            .blocks
                            .first()
                            .ok_or_else(|| InterpreterError::InvalidModule {
                                message: format!(
                                    "indirect callee '{}' has no entry block",
                                    module.functions[callee_index].name
                                ),
                            })?;
                    let (memory_frame, memory_scope) = begin_function_memory(
                        &mut state,
                        &module.functions[callee_index],
                        block_id,
                    )?;
                    frames.push(ModuleFrame::new(
                        callee_index,
                        callee_entry.id,
                        call_arguments,
                        Some(CallReturnBinding {
                            results,
                            mem_out,
                            world_out,
                        }),
                        memory_frame,
                        memory_scope,
                    ));
                } else {
                    let memory_frame = frames[frame_index].memory_frame;
                    let memory_scope = frames[frame_index].memory_scope;
                    execute_instruction(
                        &instruction,
                        block_id,
                        &mut frames[frame_index].values,
                        &mut state,
                        memory_frame,
                        memory_scope,
                    )?;
                }
            }
            FrameAction::Terminator(terminator) => match terminator {
                Inst::Br(target, arguments) => {
                    let incoming = copy_values(&arguments, block_id, &frames[frame_index].values)?;
                    set_module_frame_block(&mut frames[frame_index], target, incoming);
                }
                Inst::CondBr(
                    condition,
                    true_block,
                    true_arguments,
                    false_block,
                    false_arguments,
                ) => {
                    let condition = read_value(&frames[frame_index].values, &condition, block_id)?;
                    let (target, arguments) = if condition != 0 {
                        (true_block, true_arguments)
                    } else {
                        (false_block, false_arguments)
                    };
                    let incoming = copy_values(&arguments, block_id, &frames[frame_index].values)?;
                    set_module_frame_block(&mut frames[frame_index], target, incoming);
                }
                Inst::Ret(returned) => {
                    let returned_values =
                        copy_values(&returned, block_id, &frames[frame_index].values)?;
                    if returned_values
                        .iter()
                        .any(|value| matches!(value, InternalValue::Uninitialized))
                    {
                        return Err(InterpreterError::UninitializedRead { block: block_id });
                    }
                    let returned_types = module.functions[frames[frame_index].function_index]
                        .ret_tys
                        .clone();
                    let completed =
                        frames
                            .pop()
                            .ok_or_else(|| InterpreterError::InvalidModule {
                                message: "return observed with no active frame".to_owned(),
                            })?;
                    if let Some(memory_frame) = completed.memory_frame {
                        state.memory = state.memory.end_frame(memory_frame).map_err(|error| {
                            InterpreterError::MemoryAccess {
                                block: block_id,
                                error,
                            }
                        })?;
                    }
                    if let Some(binding) = completed.return_binding {
                        bind_call_return(&mut frames, binding, returned_values)?;
                    } else {
                        let return_values =
                            public_return_values(returned_values, returned_types, block_id)?;
                        return Ok(InterpreterOutcome {
                            return_values,
                            steps,
                            memory: state.memory,
                            world: state.world,
                        });
                    }
                }
                Inst::Trap(TrapReason::ScalarFunctionFellOffEnd) => {
                    return Err(InterpreterError::MissingReturn { block: block_id });
                }
                _ => {
                    return Err(InterpreterError::InvalidModule {
                        message: format!("block {block_id} has an invalid terminator"),
                    });
                }
            },
        }
    }
}

fn enter_module_frame(module: &Module, frame: &mut ModuleFrame) -> Result<(), InterpreterError> {
    let function = module.functions.get(frame.function_index).ok_or_else(|| {
        InterpreterError::InvalidModule {
            message: format!("missing function index {}", frame.function_index),
        }
    })?;
    let block = function
        .blocks
        .iter()
        .find(|block| block.id == frame.block)
        .ok_or(InterpreterError::MissingBlock { block: frame.block })?;
    if frame.incoming.len() != block.args.len() {
        return Err(InterpreterError::EntryArgumentCount {
            expected: block.args.len(),
            actual: frame.incoming.len(),
        });
    }
    for (argument, runtime_value) in block.args.iter().zip(frame.incoming.drain(..)) {
        frame.values.insert(argument.value.id, runtime_value);
    }
    frame.next_instruction = 0;
    frame.entered = true;
    Ok(())
}

fn module_frame_action(
    module: &Module,
    frame: &ModuleFrame,
) -> Result<FrameAction, InterpreterError> {
    let function = module.functions.get(frame.function_index).ok_or_else(|| {
        InterpreterError::InvalidModule {
            message: format!("missing function index {}", frame.function_index),
        }
    })?;
    let block = function
        .blocks
        .iter()
        .find(|block| block.id == frame.block)
        .ok_or(InterpreterError::MissingBlock { block: frame.block })?;
    Ok(if frame.next_instruction < block.insts.len() {
        FrameAction::Instruction(block.insts[frame.next_instruction].clone())
    } else {
        FrameAction::Terminator(block.terminator.clone())
    })
}

fn set_module_frame_block(frame: &mut ModuleFrame, block: BlockId, incoming: Vec<InternalValue>) {
    frame.block = block;
    frame.incoming = incoming;
    frame.entered = false;
}

fn bind_call_return(
    frames: &mut [ModuleFrame],
    binding: CallReturnBinding,
    returned: Vec<InternalValue>,
) -> Result<(), InterpreterError> {
    let expected = binding.results.len() + 2;
    if returned.len() != expected {
        return Err(InterpreterError::InvalidModule {
            message: format!(
                "callee returned {} values to a call expecting {expected}",
                returned.len()
            ),
        });
    }
    let caller = frames
        .last_mut()
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "callee return has no caller frame".to_owned(),
        })?;
    for (result, value) in binding.results.iter().zip(&returned) {
        caller.values.insert(result.id, *value);
    }
    caller
        .values
        .insert(binding.mem_out.id, returned[binding.results.len()]);
    caller
        .values
        .insert(binding.world_out.id, returned[binding.results.len() + 1]);
    Ok(())
}

fn public_return_values(
    values: Vec<InternalValue>,
    types: Vec<EvoIRType>,
    block: BlockId,
) -> Result<Vec<RuntimeValue>, InterpreterError> {
    values
        .into_iter()
        .zip(types)
        .enumerate()
        .map(|(index, (value, ty))| match value {
            InternalValue::Bits(raw_bits) => Ok(RuntimeValue { ty, raw_bits }),
            InternalValue::Pointer(_) | InternalValue::FunctionPointer(_) => {
                Err(InterpreterError::UnsupportedRuntimeType { index, ty })
            }
            InternalValue::Uninitialized => Err(InterpreterError::UninitializedRead { block }),
        })
        .collect()
}

/// Explicit compatibility spelling for [`interpret_function`].
pub fn eval_function(
    func: &Function,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    interpret_function(func, entry_args, limits)
}

fn charge_step(
    steps: &mut u64,
    limits: InterpreterLimits,
    block: BlockId,
) -> Result<(), InterpreterError> {
    if *steps >= limits.step_budget {
        return Err(InterpreterError::StepBudgetExceeded {
            budget: limits.step_budget,
            steps: *steps,
            block,
        });
    }
    *steps += 1;
    Ok(())
}

fn write_value(values: &mut HashMap<u32, InternalValue>, result: &Value, raw_bits: u64) {
    values.insert(result.id, InternalValue::Bits(raw_bits));
}

fn write_internal(values: &mut HashMap<u32, InternalValue>, result: &Value, value: InternalValue) {
    values.insert(result.id, value);
}

fn execute_instruction(
    inst: &Inst,
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
    state: &mut ExecutionState,
    memory_frame: Option<FrameId>,
    memory_scope: Option<ScopeId>,
) -> Result<(), InterpreterError> {
    match inst {
        Inst::Const(result, immediate) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            write_value(values, result, kernel.normalize(*immediate));
        }
        Inst::Uninitialized(result) => {
            values.insert(result.id, InternalValue::Uninitialized);
        }
        Inst::Add(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.add(left, right));
        }
        Inst::Sub(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.sub(left, right));
        }
        Inst::Mul(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.mul(left, right));
        }
        Inst::SRem(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.srem(left, right, block)?);
        }
        Inst::URem(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.urem(left, right, block)?);
        }
        Inst::Shl(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.shl(left, right));
        }
        Inst::AShr(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.ashr(left, right));
        }
        Inst::LShr(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.lshr(left, right));
        }
        Inst::And(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.normalize(left & right));
        }
        Inst::Or(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.normalize(left | right));
        }
        Inst::Xor(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.normalize(left ^ right));
        }
        Inst::SignedAddDefined(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u64::from(kernel.signed_add_defined(left, right)),
            );
        }
        Inst::SignedSubDefined(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u64::from(kernel.signed_sub_defined(left, right)),
            );
        }
        Inst::SignedMulDefined(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u64::from(kernel.signed_mul_defined(left, right)),
            );
        }
        Inst::DivisorNonZero(result, divisor) => {
            let kernel = IntegerKernel::for_value(divisor, block)?;
            let divisor = read_value(values, divisor, block)?;
            write_value(values, result, u64::from(kernel.normalize(divisor) != 0));
        }
        Inst::SignedDivRemNoOverflow(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u64::from(kernel.signed_div_rem_no_overflow(left, right)),
            );
        }
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let _shifted = read_value(values, shifted, block)?;
            let amount_raw = read_value(values, amount, block)?;
            let shifted_kernel = IntegerKernel::for_value(shifted, block)?;
            let amount_kernel = IntegerKernel::for_value(amount, block)?;
            write_value(
                values,
                result,
                u64::from(shifted_kernel.shift_count_defined(
                    amount_kernel,
                    amount_raw,
                    *amount_signed,
                )),
            );
        }
        Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let shifted_kernel = IntegerKernel::for_value(shifted, block)?;
            let amount_kernel = IntegerKernel::for_value(amount, block)?;
            let shifted = read_value(values, shifted, block)?;
            let amount = read_value(values, amount, block)?;
            write_value(
                values,
                result,
                u64::from(shifted_kernel.signed_shl_defined(
                    shifted,
                    amount_kernel,
                    amount,
                    *amount_signed,
                )),
            );
        }
        Inst::UbAssume(condition, reason) => {
            if read_value(values, condition, block)? == 0 {
                return Err(InterpreterError::UndefinedBehavior {
                    block,
                    diagnostic: reason.diagnostic(),
                    message: reason.message(),
                });
            }
        }
        Inst::SDiv(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            let quotient = kernel.sdiv(left, right, block)?;
            write_value(values, result, quotient);
        }
        Inst::UDiv(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            let quotient = kernel.udiv(left, right, block)?;
            write_value(values, result, quotient);
        }
        Inst::ICmp(result, operation, lhs, rhs) => {
            if matches!(lhs.ty, EvoIRType::Ptr(_)) {
                let left = read_pointer(values, lhs, block)?;
                let right = read_pointer(values, rhs, block)?;
                let comparison = match operation {
                    CmpOp::Eq => left == right,
                    CmpOp::Ne => left != right,
                    _ => {
                        return Err(InterpreterError::InvalidFunction {
                            message: format!("block {block} uses ordered comparison on pointers"),
                        });
                    }
                };
                write_value(values, result, u64::from(comparison));
                return Ok(());
            }
            if matches!(lhs.ty, EvoIRType::FnPtr { .. }) {
                let left = read_function_pointer(values, lhs, block)?;
                let right = read_function_pointer(values, rhs, block)?;
                let comparison = match operation {
                    CmpOp::Eq => left == right,
                    CmpOp::Ne => left != right,
                    _ => {
                        return Err(InterpreterError::InvalidFunction {
                            message: format!(
                                "block {block} uses ordered comparison on function pointers"
                            ),
                        });
                    }
                };
                write_value(values, result, u64::from(comparison));
                return Ok(());
            }
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            let comparison = match operation {
                CmpOp::Eq => kernel.normalize(left) == kernel.normalize(right),
                CmpOp::Ne => kernel.normalize(left) != kernel.normalize(right),
                CmpOp::SLt => kernel.signed(left) < kernel.signed(right),
                CmpOp::SLe => kernel.signed(left) <= kernel.signed(right),
                CmpOp::ULt => kernel.normalize(left) < kernel.normalize(right),
                CmpOp::ULe => kernel.normalize(left) <= kernel.normalize(right),
            };
            write_value(values, result, u64::from(comparison));
        }
        Inst::SExt(result, source, _) => {
            let source_kernel = IntegerKernel::for_value(source, block)?;
            let target_kernel = IntegerKernel::for_value(result, block)?;
            let raw = read_value(values, source, block)?;
            write_value(
                values,
                result,
                target_kernel.encode_signed(source_kernel.signed(raw)),
            );
        }
        Inst::ZExt(result, source, _) | Inst::Trunc(result, source, _) => {
            let source_kernel = IntegerKernel::for_value(source, block)?;
            let target_kernel = IntegerKernel::for_value(result, block)?;
            let raw = read_value(values, source, block)?;
            write_value(
                values,
                result,
                target_kernel.normalize(source_kernel.normalize(raw)),
            );
        }
        Inst::PtrNull(result) => match result.ty {
            EvoIRType::FnPtr { .. } => write_internal(
                values,
                result,
                InternalValue::FunctionPointer(PointerValue::Null),
            ),
            _ => write_internal(values, result, InternalValue::Pointer(Pointer::NULL)),
        },
        Inst::GlobalAddr { result, symbol } => {
            let pointer = state.globals.get(symbol).copied().ok_or_else(|| {
                InterpreterError::InvalidModule {
                    message: format!("global_addr refers to missing global '{symbol}'"),
                }
            })?;
            write_internal(values, result, InternalValue::Pointer(pointer));
        }
        Inst::FuncAddr { result, symbol } => {
            let identity = state.functions.get(symbol).copied().ok_or_else(|| {
                InterpreterError::InvalidModule {
                    message: format!("func_addr refers to missing function '{symbol}'"),
                }
            })?;
            write_internal(
                values,
                result,
                InternalValue::FunctionPointer(
                    PointerValue::function(identity)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?,
                ),
            );
        }
        Inst::Alloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            origin,
            layout,
        } => {
            let _ = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let memory_frame = memory_frame.ok_or_else(|| InterpreterError::InvalidModule {
                message: format!("alloca in block {block} lacks a function memory frame"),
            })?;
            let memory_scope = memory_scope.ok_or_else(|| InterpreterError::InvalidModule {
                message: format!("alloca in block {block} lacks a function memory scope"),
            })?;
            let (memory, pointer) = state
                .memory
                .allocate_automatic(memory_frame, memory_scope, *origin, *layout)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            state.memory = memory;
            let memory_token = state.fresh_token()?;
            let world_token = state.fresh_token()?;
            write_internal(values, result, InternalValue::Pointer(pointer));
            write_value(values, mem_out, memory_token);
            write_value(values, world_out, world_token);
        }
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            mem_out,
            world_out,
        } => {
            let _ = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let memory_frame = memory_frame.ok_or_else(|| InterpreterError::InvalidModule {
                message: format!("lifetime.end in block {block} lacks a function memory frame"),
            })?;
            let pointer = read_pointer(values, pointer, block)?;
            state.memory = state
                .memory
                .end_object_lifetime(memory_frame, pointer)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            let memory_token = state.fresh_token()?;
            let world_token = state.fresh_token()?;
            write_value(values, mem_out, memory_token);
            write_value(values, world_out, world_token);
        }
        Inst::PointerValid {
            mem_in,
            pointer,
            mem_out,
        } => {
            let _ = read_value(values, mem_in, block)?;
            let pointer = read_pointer(values, pointer, block)?;
            state
                .memory
                .check_pointer(pointer)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            let memory_token = state.fresh_token()?;
            write_value(values, mem_out, memory_token);
        }
        Inst::AccessInBounds {
            mem_in,
            pointer,
            mem_out,
            layout,
        } => {
            let _ = read_value(values, mem_in, block)?;
            let pointer = read_pointer(values, pointer, block)?;
            state
                .memory
                .check_access(pointer, *layout)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            let memory_token = state.fresh_token()?;
            state.access_guards.insert(memory_token, (pointer, *layout));
            write_value(values, mem_out, memory_token);
        }
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile,
        } => {
            let memory_token = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let address = read_pointer(values, address, block)?;
            let layout = matching_guard(state, memory_token, address, block)?;
            let loaded = match &result.ty {
                EvoIRType::Bits(_) => {
                    let raw = if *volatile {
                        let (raw, world) = state
                            .memory
                            .volatile_load_integer(&state.world, address, layout)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.world = world;
                        raw
                    } else {
                        state
                            .memory
                            .load_integer(address, layout)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?
                    };
                    let raw = u64::try_from(raw)
                        .map_err(|_| InterpreterError::UnsupportedBitWidth { block, width: 128 })?;
                    InternalValue::Bits(raw)
                }
                EvoIRType::Ptr(_) => {
                    let pointer = if *volatile {
                        let (pointer, world) = state
                            .memory
                            .volatile_load_pointer(&state.world, address, layout)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.world = world;
                        pointer
                    } else {
                        state
                            .memory
                            .load_pointer(address, layout)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?
                    };
                    InternalValue::Pointer(pointer)
                }
                EvoIRType::FnPtr { .. } => {
                    let pointer = if *volatile {
                        let (pointer, world) = state
                            .memory
                            .volatile_load_pointer_value(&state.world, address, layout)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.world = world;
                        pointer
                    } else {
                        state
                            .memory
                            .load_pointer_value(address, layout)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?
                    };
                    InternalValue::FunctionPointer(pointer)
                }
                _ => {
                    return Err(InterpreterError::UnsupportedInstruction {
                        block,
                        operation: "load type",
                    });
                }
            };
            let next_memory_token = state.fresh_token()?;
            let next_world_token = state.fresh_token()?;
            write_internal(values, result, loaded);
            write_value(values, mem_out, next_memory_token);
            write_value(values, world_out, next_world_token);
        }
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile,
        } => {
            let memory_token = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let address = read_pointer(values, address, block)?;
            let layout = matching_guard(state, memory_token, address, block)?;
            match read_internal(values, value, block)? {
                InternalValue::Bits(raw) => {
                    if *volatile {
                        let (memory, world) = state
                            .memory
                            .volatile_store_integer(&state.world, address, layout, u128::from(raw))
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.memory = memory;
                        state.world = world;
                    } else {
                        state.memory = state
                            .memory
                            .store_integer(address, layout, u128::from(raw))
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    }
                }
                InternalValue::Pointer(pointer) => {
                    if *volatile {
                        let (memory, world) = state
                            .memory
                            .volatile_store_pointer(&state.world, address, layout, pointer)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.memory = memory;
                        state.world = world;
                    } else {
                        state.memory = state
                            .memory
                            .store_pointer(address, layout, pointer)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    }
                }
                InternalValue::FunctionPointer(pointer) => {
                    if *volatile {
                        let (memory, world) = state
                            .memory
                            .volatile_store_pointer_value(&state.world, address, layout, pointer)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.memory = memory;
                        state.world = world;
                    } else {
                        state.memory =
                            state
                                .memory
                                .store_pointer_value(address, layout, pointer)
                                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    }
                }
                InternalValue::Uninitialized => {
                    return Err(InterpreterError::UninitializedRead { block });
                }
            }
            let next_memory_token = state.fresh_token()?;
            let next_world_token = state.fresh_token()?;
            write_value(values, mem_out, next_memory_token);
            write_value(values, world_out, next_world_token);
        }
        Inst::Call { .. } | Inst::IndirectCall { .. } => {
            return Err(InterpreterError::UnsupportedInstruction {
                block,
                operation: "call",
            });
        }
        Inst::PtrOffset {
            result,
            base,
            index,
            direction,
            signedness,
            scale,
        } => {
            let base = read_pointer(values, base, block)?;
            let raw = read_value(values, index, block)?;
            let EvoIRType::Bits(width) = index.ty else {
                return Err(InterpreterError::InvalidFunction {
                    message: format!("pointer offset index in block {block} is not bits"),
                });
            };
            let pointer = base
                .checked_scaled_offset(u128::from(raw), width, *signedness, *direction, *scale)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            write_internal(values, result, InternalValue::Pointer(pointer));
        }
        Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) | Inst::Trap(..) => {
            return Err(InterpreterError::InvalidFunction {
                message: format!("terminator appears in instructions of block {block}"),
            });
        }
    }
    Ok(())
}

fn read_value(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<u64, InterpreterError> {
    match values
        .get(&value.id)
        .copied()
        .ok_or(InterpreterError::MissingValue {
            block,
            value: value.id,
        })? {
        InternalValue::Bits(raw_bits) => Ok(raw_bits),
        InternalValue::Pointer(_) | InternalValue::FunctionPointer(_) => {
            Err(InterpreterError::InvalidFunction {
                message: format!(
                    "block {block} consumes pointer v{} as integer bits",
                    value.id
                ),
            })
        }
        InternalValue::Uninitialized => Err(InterpreterError::UninitializedRead { block }),
    }
}

fn read_internal(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<InternalValue, InterpreterError> {
    values
        .get(&value.id)
        .copied()
        .ok_or(InterpreterError::MissingValue {
            block,
            value: value.id,
        })
}

fn read_pointer(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<Pointer, InterpreterError> {
    match read_internal(values, value, block)? {
        InternalValue::Pointer(pointer) => Ok(pointer),
        InternalValue::Uninitialized => Err(InterpreterError::UninitializedRead { block }),
        InternalValue::Bits(_) | InternalValue::FunctionPointer(_) => {
            Err(InterpreterError::InvalidFunction {
                message: format!(
                    "block {block} consumes non-pointer v{} as pointer",
                    value.id
                ),
            })
        }
    }
}

fn read_function_pointer(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<PointerValue, InterpreterError> {
    match read_internal(values, value, block)? {
        InternalValue::FunctionPointer(pointer) => Ok(pointer),
        InternalValue::Uninitialized => Err(InterpreterError::UninitializedRead { block }),
        InternalValue::Bits(_) | InternalValue::Pointer(_) => {
            Err(InterpreterError::InvalidFunction {
                message: format!(
                    "block {block} consumes non-function-pointer v{} as fnptr",
                    value.id
                ),
            })
        }
    }
}

fn matching_guard(
    state: &ExecutionState,
    memory_token: u64,
    pointer: Pointer,
    block: BlockId,
) -> Result<AccessLayout, InterpreterError> {
    let (guarded_pointer, layout) =
        state
            .access_guards
            .get(&memory_token)
            .copied()
            .ok_or_else(|| InterpreterError::InvalidModule {
                message: format!(
                    "block {block} load/store consumes memory token without access_in_bounds"
                ),
            })?;
    if guarded_pointer != pointer {
        return Err(InterpreterError::InvalidModule {
            message: format!("block {block} load/store pointer does not match its access guard"),
        });
    }
    Ok(layout)
}

fn initialize_globals(module: &Module, state: &mut ExecutionState) -> Result<(), InterpreterError> {
    for global in &module.globals {
        let (memory, pointer) = match global.kind {
            GlobalKind::StringLiteral => {
                let bytes = ByteSequence::try_from_bytes(global.bytes.clone())
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                state
                    .memory
                    .allocate_string_literal(global.origin, global.layout, &bytes)
                    .map_err(|error| InterpreterError::MemorySetup { error })?
            }
            GlobalKind::File | GlobalKind::FunctionStatic => state
                .memory
                .allocate_static(global.origin, global.layout, InitialState::Uninitialized)
                .map_err(|error| InterpreterError::MemorySetup { error })?,
        };
        state.memory = memory;
        if global.kind != GlobalKind::StringLiteral {
            let mut start = 0_usize;
            while start < global.initialized.len() {
                if !global.initialized[start] {
                    start += 1;
                    continue;
                }
                let mut end = start + 1;
                while end < global.initialized.len() && global.initialized[end] {
                    end += 1;
                }
                let offset = i64::try_from(start).map_err(|_| InterpreterError::InvalidModule {
                    message: format!("global '{}' offset exceeds i64", global.symbol),
                })?;
                let address = pointer
                    .checked_offset(offset)
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                let size =
                    u64::try_from(end - start).map_err(|_| InterpreterError::InvalidModule {
                        message: format!("global '{}' range exceeds u64", global.symbol),
                    })?;
                let bytes = ByteSequence::try_from_bytes(global.bytes[start..end].to_vec())
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                state.memory = state
                    .memory
                    .store_bytes(address, AccessLayout::new(size, 1), &bytes)
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                start = end;
            }
        }
        state.globals.insert(global.symbol.clone(), pointer);
    }
    Ok(())
}

fn copy_values(
    arguments: &[Value],
    block: BlockId,
    values: &HashMap<u32, InternalValue>,
) -> Result<Vec<InternalValue>, InterpreterError> {
    arguments
        .iter()
        .map(|argument| {
            values
                .get(&argument.id)
                .copied()
                .ok_or(InterpreterError::MissingValue {
                    block,
                    value: argument.id,
                })
        })
        .collect()
}

fn read_call_values(
    arguments: &[Value],
    block: BlockId,
    values: &HashMap<u32, InternalValue>,
) -> Result<Vec<InternalValue>, InterpreterError> {
    arguments
        .iter()
        .map(|argument| match read_internal(values, argument, block)? {
            InternalValue::Uninitialized => Err(InterpreterError::UninitializedRead { block }),
            value => Ok(value),
        })
        .collect()
}

fn normalize_runtime_input(
    value: &RuntimeValue,
    expected: &EvoIRType,
    index: usize,
    block: BlockId,
) -> Result<InternalValue, InterpreterError> {
    validate_value_type(&value.ty)
        .map_err(|message| InterpreterError::InvalidEntryArgument { index, message })?;
    if &value.ty != expected {
        return Err(InterpreterError::EntryArgumentType {
            index,
            expected: expected.clone(),
            actual: value.ty.clone(),
        });
    }
    match expected {
        EvoIRType::Bits(_) => Ok(InternalValue::Bits(
            IntegerKernel::for_type(expected, block)?.normalize(value.raw_bits),
        )),
        EvoIRType::Mem(_) | EvoIRType::World => Ok(InternalValue::Bits(value.raw_bits)),
        _ => Err(InterpreterError::UnsupportedRuntimeType {
            index,
            ty: expected.clone(),
        }),
    }
}

/// One width-aware bit-vector kernel shared by every executed integer opcode.
#[derive(Clone, Copy)]
struct IntegerKernel {
    width: u16,
    mask: u64,
}

impl IntegerKernel {
    fn for_value(value: &Value, block: BlockId) -> Result<Self, InterpreterError> {
        Self::for_type(&value.ty, block)
    }

    fn for_type(ty: &EvoIRType, block: BlockId) -> Result<Self, InterpreterError> {
        let EvoIRType::Bits(width @ 1..=64) = ty else {
            let width = match ty {
                EvoIRType::Bits(width) => *width,
                _ => 0,
            };
            return Err(InterpreterError::UnsupportedBitWidth { block, width });
        };
        let mask = if *width == 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        Ok(Self {
            width: *width,
            mask,
        })
    }

    fn normalize(self, value: u64) -> u64 {
        value & self.mask
    }

    fn add(self, left: u64, right: u64) -> u64 {
        self.normalize(left.wrapping_add(right))
    }

    fn sub(self, left: u64, right: u64) -> u64 {
        self.normalize(left.wrapping_sub(right))
    }

    fn mul(self, left: u64, right: u64) -> u64 {
        self.normalize(left.wrapping_mul(right))
    }

    fn shl(self, left: u64, amount: u64) -> u64 {
        if amount >= u64::from(self.width) {
            0
        } else {
            self.normalize(self.normalize(left) << amount)
        }
    }

    fn lshr(self, left: u64, amount: u64) -> u64 {
        if amount >= u64::from(self.width) {
            0
        } else {
            self.normalize(left) >> amount
        }
    }

    fn ashr(self, left: u64, amount: u64) -> u64 {
        if amount >= u64::from(self.width) {
            if self.signed(left) < 0 {
                self.mask
            } else {
                0
            }
        } else {
            self.encode_signed(self.signed(left) >> amount)
        }
    }

    fn signed_add_defined(self, left: u64, right: u64) -> bool {
        self.signed_fits(self.signed(left) + self.signed(right))
    }

    fn signed_sub_defined(self, left: u64, right: u64) -> bool {
        self.signed_fits(self.signed(left) - self.signed(right))
    }

    fn signed_mul_defined(self, left: u64, right: u64) -> bool {
        self.signed_fits(self.signed(left) * self.signed(right))
    }

    fn signed_div_rem_no_overflow(self, left: u64, right: u64) -> bool {
        let minimum = -(1_i128 << (self.width - 1));
        !(self.signed(left) == minimum && self.signed(right) == -1)
    }

    fn decoded_shift_amount(
        self,
        amount_kernel: Self,
        amount: u64,
        amount_signed: bool,
    ) -> Option<u64> {
        let mathematical = if amount_signed {
            amount_kernel.signed(amount)
        } else {
            i128::from(amount_kernel.normalize(amount))
        };
        if mathematical < 0 || mathematical >= i128::from(self.width) {
            None
        } else {
            u64::try_from(mathematical).ok()
        }
    }

    fn shift_count_defined(self, amount_kernel: Self, amount: u64, amount_signed: bool) -> bool {
        self.decoded_shift_amount(amount_kernel, amount, amount_signed)
            .is_some()
    }

    fn signed_shl_defined(
        self,
        left: u64,
        amount_kernel: Self,
        amount: u64,
        amount_signed: bool,
    ) -> bool {
        let Some(amount) = self.decoded_shift_amount(amount_kernel, amount, amount_signed) else {
            return false;
        };
        let left = self.signed(left);
        left >= 0 && self.signed_fits(left << amount)
    }

    fn signed_fits(self, value: i128) -> bool {
        let minimum = -(1_i128 << (self.width - 1));
        let maximum = (1_i128 << (self.width - 1)) - 1;
        (minimum..=maximum).contains(&value)
    }

    fn signed(self, value: u64) -> i128 {
        let value = self.normalize(value);
        let sign_bit = 1_u64 << (self.width - 1);
        if value & sign_bit == 0 {
            i128::from(value)
        } else {
            i128::from(value) - (1_i128 << self.width)
        }
    }

    fn encode_signed(self, value: i128) -> u64 {
        self.normalize(value as u64)
    }

    fn sdiv(self, left: u64, right: u64, block: BlockId) -> Result<u64, InterpreterError> {
        let left = self.signed(left);
        let right = self.signed(right);
        if right == 0 {
            return Err(InterpreterError::DivisionByZero {
                block,
                operation: "signed division",
            });
        }
        let minimum = -(1_i128 << (self.width - 1));
        if left == minimum && right == -1 {
            return Err(InterpreterError::SignedDivisionOverflow {
                block,
                width: self.width,
            });
        }
        Ok(self.encode_signed(left / right))
    }

    fn udiv(self, left: u64, right: u64, block: BlockId) -> Result<u64, InterpreterError> {
        let left = self.normalize(left);
        let right = self.normalize(right);
        if right == 0 {
            return Err(InterpreterError::DivisionByZero {
                block,
                operation: "unsigned division",
            });
        }
        Ok(left / right)
    }

    fn srem(self, left: u64, right: u64, block: BlockId) -> Result<u64, InterpreterError> {
        let left = self.signed(left);
        let right = self.signed(right);
        if right == 0 {
            return Err(InterpreterError::RemainderByZero {
                block,
                operation: "signed remainder",
            });
        }
        let minimum = -(1_i128 << (self.width - 1));
        if left == minimum && right == -1 {
            return Err(InterpreterError::SignedRemainderOverflow {
                block,
                width: self.width,
            });
        }
        Ok(self.encode_signed(left % right))
    }

    fn urem(self, left: u64, right: u64, block: BlockId) -> Result<u64, InterpreterError> {
        let left = self.normalize(left);
        let right = self.normalize(right);
        if right == 0 {
            return Err(InterpreterError::RemainderByZero {
                block,
                operation: "unsigned remainder",
            });
        }
        Ok(left % right)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_memory::{AccessLayout, InitialState};
    use evocc_types::{AddressSpace, Region};

    fn value(id: u32, ty: EvoIRType) -> Value {
        Value { id, ty }
    }

    fn bits(id: u32, width: u16) -> Value {
        value(id, EvoIRType::Bits(width))
    }

    fn argument(name: &str, value: Value) -> BlockArg {
        BlockArg {
            name: name.to_owned(),
            value,
        }
    }

    fn overly_nested_type() -> EvoIRType {
        let mut ty = EvoIRType::Bits(1);
        for _ in 0..=MAX_TYPE_NESTING {
            ty = EvoIRType::Tuple(vec![ty]);
        }
        ty
    }

    fn reject(function: &Function, fragment: &str) {
        let error = validate_ssa(function).expect_err("forged IR must be rejected");
        assert!(
            error.contains(fragment),
            "expected {fragment:?} in validator error, got {error:?}"
        );
    }

    fn threaded_function() -> Function {
        let produced = bits(0, 32);
        let incoming = bits(1, 32);
        let one = bits(2, 32);
        let result = bits(3, 32);
        Function {
            name: "threaded".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![],
                    insts: vec![Inst::Const(produced.clone(), 41)],
                    terminator: Inst::Br(1, vec![produced]),
                },
                Block {
                    id: 1,
                    args: vec![argument("incoming", incoming.clone())],
                    insts: vec![
                        Inst::Const(one.clone(), 1),
                        Inst::Add(result.clone(), incoming, one),
                    ],
                    terminator: Inst::Ret(vec![result]),
                },
            ],
        }
    }

    fn token_call_function() -> Function {
        let mem_global = EvoIRType::Mem(Region::Global);
        let mem_stack = EvoIRType::Mem(Region::Stack);
        let world = EvoIRType::World;
        let global_in = value(0, mem_global.clone());
        let stack_in = value(1, mem_stack.clone());
        let world_in = value(2, world.clone());
        let global_out = value(3, mem_global.clone());
        let world_out = value(4, world.clone());
        Function {
            name: "tokens".to_owned(),
            params: vec![
                FunctionParam {
                    name: "global".to_owned(),
                    ty: mem_global.clone(),
                },
                FunctionParam {
                    name: "stack".to_owned(),
                    ty: mem_stack.clone(),
                },
                FunctionParam {
                    name: "world".to_owned(),
                    ty: world.clone(),
                },
            ],
            ret_tys: vec![mem_global, mem_stack, world],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![
                    argument("global", global_in.clone()),
                    argument("stack", stack_in.clone()),
                    argument("world", world_in.clone()),
                ],
                insts: vec![Inst::Call {
                    results: vec![],
                    callee: "side_effect".to_owned(),
                    args: vec![],
                    mem_in: global_in,
                    world_in,
                    mem_out: global_out.clone(),
                    world_out: world_out.clone(),
                }],
                terminator: Inst::Ret(vec![global_out, stack_in, world_out]),
            }],
        }
    }

    #[test]
    fn entry_interface_is_non_ssa_and_interpreter_accepts_explicit_arguments() {
        let input = bits(0, 32);
        let one = bits(1, 32);
        let result = bits(2, 32);
        let function = Function {
            name: "increment".to_owned(),
            params: vec![FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(32),
            }],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![argument("x", input.clone())],
                insts: vec![
                    Inst::Const(one.clone(), 1),
                    Inst::Add(result.clone(), input, one),
                ],
                terminator: Inst::Ret(vec![result]),
            }],
        };
        validate_ssa(&function).expect("valid entry correspondence");
        let outcome = eval_function(
            &function,
            &[RuntimeValue::bits(32, 41)],
            InterpreterLimits::new(3),
        )
        .expect("bounded execution");
        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 42)]);
        assert_eq!(outcome.steps, 3);

        let mut missing_entry_arg = function.clone();
        missing_entry_arg.blocks[0].args.clear();
        reject(&missing_entry_arg, "has 0 arguments");
        assert!(matches!(
            eval_function(&function, &[], InterpreterLimits::new(3)),
            Err(InterpreterError::EntryArgumentCount {
                expected: 1,
                actual: 0
            })
        ));
        assert!(matches!(
            eval_function(
                &function,
                &[RuntimeValue::bits(64, 41)],
                InterpreterLimits::new(3)
            ),
            Err(InterpreterError::EntryArgumentType { index: 0, .. })
        ));
        assert!(matches!(
            eval_function(
                &function,
                &[RuntimeValue::new(overly_nested_type(), 41)],
                InterpreterLimits::new(3)
            ),
            Err(InterpreterError::InvalidEntryArgument { index: 0, .. })
        ));

        let pointer_type = EvoIRType::Ptr(AddressSpace::Stack);
        let pointer = value(10, pointer_type.clone());
        let pointer_function = Function {
            name: "pointer_passthrough".to_owned(),
            params: vec![FunctionParam {
                name: "pointer".to_owned(),
                ty: pointer_type.clone(),
            }],
            ret_tys: vec![pointer_type.clone()],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![argument("pointer", pointer.clone())],
                insts: vec![],
                terminator: Inst::Ret(vec![pointer]),
            }],
        };
        validate_ssa(&pointer_function).expect("pointer IR is structurally valid");
        assert!(matches!(
            eval_function(
                &pointer_function,
                &[RuntimeValue::new(pointer_type, 0x1000)],
                InterpreterLimits::new(1)
            ),
            Err(InterpreterError::UnsupportedRuntimeType { index: 0, .. })
        ));
    }

    #[test]
    fn real_block_argument_ids_are_unique_and_cross_block_values_are_threaded() {
        let function = threaded_function();
        validate_ssa(&function).expect("explicit block-argument threading is valid");

        let mut duplicate = function.clone();
        duplicate.blocks[1].args[0].value.id = 0;
        reject(&duplicate, "defined more than once");

        let mut cross_block = function.clone();
        cross_block.blocks[1].terminator = Inst::Ret(vec![bits(0, 32)]);
        reject(&cross_block, "not a prior local definition");

        let mut use_before_definition = function.clone();
        let late = bits(9, 32);
        use_before_definition.blocks[0].insts = vec![
            Inst::Add(bits(10, 32), late.clone(), late.clone()),
            Inst::Const(late, 1),
        ];
        use_before_definition.blocks[0].terminator = Inst::Br(1, vec![bits(10, 32)]);
        reject(&use_before_definition, "not a prior local definition");
    }

    #[test]
    fn edge_shape_targets_conditions_returns_and_reachability_are_checked() {
        let function = threaded_function();

        let mut arity = function.clone();
        arity.blocks[0].terminator = Inst::Br(1, vec![]);
        reject(&arity, "passes 0 values");

        let mut edge_type = function.clone();
        edge_type.blocks[1].args[0].value.ty = EvoIRType::Bits(64);
        reject(&edge_type, "edge 0 -> 1 value 0");

        let mut missing_target = function.clone();
        missing_target.blocks[0].terminator = Inst::Br(99, vec![bits(0, 32)]);
        reject(&missing_target, "targets missing block 99");

        let mut unreachable = function.clone();
        let unused = bits(20, 32);
        unreachable.blocks.push(Block {
            id: 20,
            args: vec![],
            insts: vec![Inst::Const(unused.clone(), 0)],
            terminator: Inst::Ret(vec![unused]),
        });
        reject(&unreachable, "is unreachable");

        let condition = bits(30, 32);
        let left = bits(31, 32);
        let right = bits(32, 32);
        let bad_condition = Function {
            name: "bad_condition".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![],
                    insts: vec![Inst::Const(condition.clone(), 1)],
                    terminator: Inst::CondBr(condition, 1, vec![], 2, vec![]),
                },
                Block {
                    id: 1,
                    args: vec![],
                    insts: vec![Inst::Const(left.clone(), 1)],
                    terminator: Inst::Ret(vec![left]),
                },
                Block {
                    id: 2,
                    args: vec![],
                    insts: vec![Inst::Const(right.clone(), 2)],
                    terminator: Inst::Ret(vec![right]),
                },
            ],
        };
        reject(&bad_condition, "expected bits<1>");

        let mut bad_return = function;
        bad_return.ret_tys = vec![EvoIRType::Bits(64)];
        reject(&bad_return, "return value 0");
    }

    #[test]
    fn value_type_validation_is_recursive_and_void_is_not_a_value() {
        assert!(validate_type_well_formed(&EvoIRType::Void).is_ok());
        assert!(validate_value_type(&EvoIRType::Void).is_err());
        assert!(validate_value_type(&EvoIRType::Tuple(vec![])).is_err());
        assert!(validate_value_type(&EvoIRType::Bits(0)).is_err());
        assert!(validate_value_type(&EvoIRType::Vec(4, Box::new(EvoIRType::World))).is_err());
        assert!(
            validate_value_type(&EvoIRType::Vec(4, Box::new(EvoIRType::Tuple(vec![])))).is_err()
        );
        assert!(validate_value_type(&EvoIRType::Ptr(AddressSpace::Stack)).is_ok());
        assert!(validate_value_type(&EvoIRType::Mem(Region::Heap)).is_ok());
        let nested_effect = EvoIRType::Vec(
            4,
            Box::new(EvoIRType::Tuple(vec![EvoIRType::Mem(Region::Heap)])),
        );
        assert!(validate_value_type(&nested_effect).is_ok());
        assert_eq!(contains_effect_token(&nested_effect), Ok(true));

        let too_deep = overly_nested_type();
        let error = validate_value_type(&too_deep).expect_err("deep forged type must be bounded");
        assert!(error.contains("nesting exceeds"));
    }

    #[test]
    fn instruction_result_and_embedded_types_are_bounded_before_comparison() {
        let left = bits(0, 32);
        let right = bits(1, 32);
        let mut deep_result = Function {
            name: "deep_result".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(left.clone(), 1),
                    Inst::Const(right.clone(), 2),
                    Inst::Add(value(2, overly_nested_type()), left.clone(), right),
                ],
                terminator: Inst::Ret(vec![left.clone()]),
            }],
        };
        reject(&deep_result, "nesting exceeds");

        deep_result.blocks[0].insts = vec![
            Inst::Const(left.clone(), 1),
            Inst::SExt(bits(3, 32), left.clone(), overly_nested_type()),
        ];
        deep_result.blocks[0].terminator = Inst::Ret(vec![bits(3, 32)]);
        reject(&deep_result, "nesting exceeds");
    }

    #[test]
    fn token_regions_are_linear_and_ordinary_call_values_exclude_tokens() {
        let function = token_call_function();
        validate_ssa(&function).expect("distinct memory regions thread independently");

        let mut stale = function.clone();
        stale.blocks[0].insts.push(Inst::Call {
            results: vec![],
            callee: "stale".to_owned(),
            args: vec![],
            mem_in: value(0, EvoIRType::Mem(Region::Global)),
            world_in: value(2, EvoIRType::World),
            mem_out: value(5, EvoIRType::Mem(Region::Global)),
            world_out: value(6, EvoIRType::World),
        });
        stale.blocks[0].terminator = Inst::Ret(vec![
            value(5, EvoIRType::Mem(Region::Global)),
            value(1, EvoIRType::Mem(Region::Stack)),
            value(6, EvoIRType::World),
        ]);
        reject(&stale, "consumes stale");

        let mut dropped = function.clone();
        dropped.ret_tys = vec![EvoIRType::Mem(Region::Global), EvoIRType::World];
        dropped.blocks[0].terminator = Inst::Ret(vec![
            value(3, EvoIRType::Mem(Region::Global)),
            value(4, EvoIRType::World),
        ]);
        reject(&dropped, "drops current mem<Stack>");

        let mut duplicate_region = function.clone();
        duplicate_region.params[1].ty = EvoIRType::Mem(Region::Global);
        duplicate_region.blocks[0].args[1].value.ty = EvoIRType::Mem(Region::Global);
        duplicate_region.ret_tys[1] = EvoIRType::Mem(Region::Global);
        duplicate_region.blocks[0].terminator = Inst::Ret(vec![
            value(3, EvoIRType::Mem(Region::Global)),
            value(1, EvoIRType::Mem(Region::Global)),
            value(4, EvoIRType::World),
        ]);
        reject(&duplicate_region, "more than one mem<Global>");

        let mut ordinary_token = function.clone();
        let Inst::Call { args, .. } = &mut ordinary_token.blocks[0].insts[0] else {
            panic!("fixture must contain a call");
        };
        args.push(value(1, EvoIRType::Mem(Region::Stack)));
        reject(&ordinary_token, "ordinary call argument");

        let mut nested_token = function.clone();
        let tuple_token = EvoIRType::Tuple(vec![EvoIRType::Mem(Region::Heap)]);
        nested_token.params.push(FunctionParam {
            name: "hidden".to_owned(),
            ty: tuple_token.clone(),
        });
        nested_token.blocks[0]
            .args
            .push(argument("hidden", value(5, tuple_token.clone())));
        let Inst::Call { args, .. } = &mut nested_token.blocks[0].insts[0] else {
            panic!("fixture must contain a call");
        };
        args.push(value(5, tuple_token));
        reject(&nested_token, "contains a memory/world effect token");

        let mut changed_region = function;
        let Inst::Call { mem_out, .. } = &mut changed_region.blocks[0].insts[0] else {
            panic!("fixture must contain a call");
        };
        mem_out.ty = EvoIRType::Mem(Region::Heap);
        reject(&changed_region, "changes memory region");
    }

    #[test]
    fn iterative_interpreter_executes_backedges_and_enforces_budget() {
        let initial_n = bits(0, 32);
        let initial_acc = bits(1, 32);
        let n = bits(2, 32);
        let acc = bits(3, 32);
        let zero = bits(4, 32);
        let condition = bits(5, 1);
        let body_n = bits(6, 32);
        let body_acc = bits(7, 32);
        let next_acc = bits(8, 32);
        let one = bits(9, 32);
        let next_n = bits(10, 32);
        let returned = bits(11, 32);
        let function = Function {
            name: "loop_sum".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        Inst::Const(initial_n.clone(), 5),
                        Inst::Const(initial_acc.clone(), 0),
                    ],
                    terminator: Inst::Br(1, vec![initial_n, initial_acc]),
                },
                Block {
                    id: 1,
                    args: vec![argument("n", n.clone()), argument("acc", acc.clone())],
                    insts: vec![
                        Inst::Const(zero.clone(), 0),
                        Inst::ICmp(condition.clone(), CmpOp::SLt, zero, n.clone()),
                    ],
                    terminator: Inst::CondBr(condition, 2, vec![n, acc.clone()], 3, vec![acc]),
                },
                Block {
                    id: 2,
                    args: vec![
                        argument("n", body_n.clone()),
                        argument("acc", body_acc.clone()),
                    ],
                    insts: vec![
                        Inst::Add(next_acc.clone(), body_acc, body_n.clone()),
                        Inst::Const(one.clone(), 1),
                        Inst::Sub(next_n.clone(), body_n, one),
                    ],
                    terminator: Inst::Br(1, vec![next_n, next_acc]),
                },
                Block {
                    id: 3,
                    args: vec![argument("result", returned.clone())],
                    insts: vec![],
                    terminator: Inst::Ret(vec![returned]),
                },
            ],
        };
        let outcome = eval_function(&function, &[], InterpreterLimits::new(42))
            .expect("loop must terminate within its exact budget");
        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 15)]);
        assert_eq!(outcome.steps, 42);
        assert!(matches!(
            eval_function(&function, &[], InterpreterLimits::new(41)),
            Err(InterpreterError::StepBudgetExceeded {
                budget: 41,
                steps: 41,
                ..
            })
        ));
    }

    #[test]
    fn self_loop_arguments_are_resolved_simultaneously_before_rebinding() {
        let a = bits(0, 32);
        let b = bits(1, 32);
        let count = bits(2, 32);
        let zero = bits(3, 32);
        let one = bits(4, 32);
        let next_count = bits(5, 32);
        let condition = bits(6, 1);
        let out_a = bits(7, 32);
        let out_b = bits(8, 32);
        let function = Function {
            name: "swap_once".to_owned(),
            params: vec![
                FunctionParam {
                    name: "a".to_owned(),
                    ty: EvoIRType::Bits(32),
                },
                FunctionParam {
                    name: "b".to_owned(),
                    ty: EvoIRType::Bits(32),
                },
                FunctionParam {
                    name: "count".to_owned(),
                    ty: EvoIRType::Bits(32),
                },
            ],
            ret_tys: vec![EvoIRType::Bits(32), EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![
                        argument("a", a.clone()),
                        argument("b", b.clone()),
                        argument("count", count.clone()),
                    ],
                    insts: vec![
                        Inst::Const(zero.clone(), 0),
                        Inst::Const(one.clone(), 1),
                        Inst::Sub(next_count.clone(), count.clone(), one),
                        Inst::ICmp(condition.clone(), CmpOp::SLt, zero, count),
                    ],
                    terminator: Inst::CondBr(
                        condition,
                        0,
                        vec![b, a.clone(), next_count],
                        1,
                        vec![a, bits(1, 32)],
                    ),
                },
                Block {
                    id: 1,
                    args: vec![argument("a", out_a.clone()), argument("b", out_b.clone())],
                    insts: vec![],
                    terminator: Inst::Ret(vec![out_a, out_b]),
                },
            ],
        };
        let outcome = eval_function(
            &function,
            &[
                RuntimeValue::bits(32, 1),
                RuntimeValue::bits(32, 2),
                RuntimeValue::bits(32, 1),
            ],
            InterpreterLimits::new(11),
        )
        .expect("one self-loop iteration must execute");
        assert_eq!(
            outcome.return_values,
            vec![RuntimeValue::bits(32, 2), RuntimeValue::bits(32, 1)]
        );
        assert_eq!(outcome.steps, 11);
    }

    #[test]
    fn integer_kernel_handles_raw_64_bit_patterns_and_equal_width_conversions() {
        let raw = bits(0, 64);
        let same = bits(1, 64);
        let function = Function {
            name: "raw".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(64)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(raw.clone(), u64::MAX),
                    Inst::SExt(same.clone(), raw, EvoIRType::Bits(64)),
                ],
                terminator: Inst::Ret(vec![same]),
            }],
        };
        let outcome = eval_function(&function, &[], InterpreterLimits::new(3))
            .expect("raw pattern must not be host-i64 constrained");
        assert_eq!(
            outcome.return_values,
            vec![RuntimeValue::bits(64, u64::MAX)]
        );
    }

    #[test]
    fn interpreter_reports_typed_arithmetic_and_unsupported_effect_errors() {
        let numerator = bits(0, 32);
        let zero = bits(1, 32);
        let quotient = bits(2, 32);
        let division = Function {
            name: "division".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(numerator.clone(), 7),
                    Inst::Const(zero.clone(), 0),
                    Inst::SDiv(quotient.clone(), numerator, zero),
                ],
                terminator: Inst::Ret(vec![quotient]),
            }],
        };
        let division_error = eval_function(&division, &[], InterpreterLimits::new(4))
            .expect_err("zero divisor must fail");
        assert!(matches!(
            &division_error,
            InterpreterError::DivisionByZero {
                operation: "signed division",
                ..
            }
        ));
        assert_eq!(division_error.code(), "E3002");
        assert_eq!(division_error.message(), "integer-division-by-zero");

        let mut remainder = division.clone();
        remainder.name = "remainder".to_owned();
        let Inst::SDiv(result, lhs, rhs) = remainder.blocks[0].insts[2].clone() else {
            panic!("fixture signed division");
        };
        remainder.blocks[0].insts[2] = Inst::SRem(result, lhs, rhs);
        let remainder_error = eval_function(&remainder, &[], InterpreterLimits::new(4))
            .expect_err("zero remainder divisor must fail");
        assert!(matches!(
            &remainder_error,
            InterpreterError::RemainderByZero {
                operation: "signed remainder",
                ..
            }
        ));
        assert_eq!(remainder_error.code(), "E3003");
        assert_eq!(remainder_error.message(), "integer-remainder-by-zero");

        assert!(matches!(
            eval_function(
                &token_call_function(),
                &[
                    RuntimeValue::new(EvoIRType::Mem(Region::Global), 10),
                    RuntimeValue::new(EvoIRType::Mem(Region::Stack), 20),
                    RuntimeValue::new(EvoIRType::World, 30),
                ],
                InterpreterLimits::new(2)
            ),
            Err(InterpreterError::UnsupportedInstruction {
                operation: "call",
                ..
            })
        ));
    }

    fn identity_callee(name: &str, ordinary_ty: EvoIRType, region: Region) -> Function {
        let memory_ty = EvoIRType::Mem(region);
        let input = value(0, ordinary_ty.clone());
        let memory = value(1, memory_ty.clone());
        let world = value(2, EvoIRType::World);
        Function {
            name: name.to_owned(),
            params: vec![
                FunctionParam {
                    name: "input".to_owned(),
                    ty: ordinary_ty.clone(),
                },
                FunctionParam {
                    name: "memory".to_owned(),
                    ty: memory_ty.clone(),
                },
                FunctionParam {
                    name: "world".to_owned(),
                    ty: EvoIRType::World,
                },
            ],
            ret_tys: vec![ordinary_ty, memory_ty, EvoIRType::World],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![
                    argument("input", input.clone()),
                    argument("memory", memory.clone()),
                    argument("world", world.clone()),
                ],
                insts: vec![],
                terminator: Inst::Ret(vec![input, memory, world]),
            }],
        }
    }

    fn direct_call_module() -> Module {
        let memory_ty = EvoIRType::Mem(Region::All);
        let memory_in = value(0, memory_ty.clone());
        let world_in = value(1, EvoIRType::World);
        let argument_value = bits(2, 32);
        let result = bits(3, 32);
        let memory_out = value(4, memory_ty.clone());
        let world_out = value(5, EvoIRType::World);
        let main = Function {
            name: "main".to_owned(),
            params: vec![
                FunctionParam {
                    name: "memory".to_owned(),
                    ty: memory_ty.clone(),
                },
                FunctionParam {
                    name: "world".to_owned(),
                    ty: EvoIRType::World,
                },
            ],
            ret_tys: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![
                    argument("memory", memory_in.clone()),
                    argument("world", world_in.clone()),
                ],
                insts: vec![
                    Inst::Const(argument_value.clone(), 41),
                    Inst::Call {
                        results: vec![result.clone()],
                        callee: "identity".to_owned(),
                        args: vec![argument_value],
                        mem_in: memory_in,
                        world_in,
                        mem_out: memory_out.clone(),
                        world_out: world_out.clone(),
                    },
                ],
                terminator: Inst::Ret(vec![result, memory_out, world_out]),
            }],
        };
        let mut module = Module {
            globals: vec![],
            functions: vec![
                identity_callee("identity", EvoIRType::Bits(32), Region::All),
                main,
            ],
        };
        module.recompute_effect_summaries();
        module
    }

    fn indirect_call_module(null: bool) -> Module {
        let memory_ty = EvoIRType::Mem(Region::All);
        let signature = EvoIRType::FnPtr {
            params: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
            results: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
        };
        let memory_in = value(0, memory_ty.clone());
        let world_in = value(1, EvoIRType::World);
        let callee = value(2, signature);
        let argument_value = bits(3, 32);
        let result = bits(4, 32);
        let memory_out = value(5, memory_ty.clone());
        let world_out = value(6, EvoIRType::World);
        let address = if null {
            Inst::PtrNull(callee.clone())
        } else {
            Inst::FuncAddr {
                result: callee.clone(),
                symbol: "identity".to_owned(),
            }
        };
        let main = Function {
            name: "main".to_owned(),
            params: vec![
                FunctionParam {
                    name: "memory".to_owned(),
                    ty: memory_ty.clone(),
                },
                FunctionParam {
                    name: "world".to_owned(),
                    ty: EvoIRType::World,
                },
            ],
            ret_tys: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![
                    argument("memory", memory_in.clone()),
                    argument("world", world_in.clone()),
                ],
                insts: vec![
                    address,
                    Inst::Const(argument_value.clone(), 41),
                    Inst::IndirectCall {
                        results: vec![result.clone()],
                        callee,
                        args: vec![argument_value],
                        mem_in: memory_in,
                        world_in,
                        mem_out: memory_out.clone(),
                        world_out: world_out.clone(),
                    },
                ],
                terminator: Inst::Ret(vec![result, memory_out, world_out]),
            }],
        };
        let mut module = Module {
            globals: vec![],
            functions: vec![
                identity_callee("identity", EvoIRType::Bits(32), Region::All),
                main,
            ],
        };
        module.recompute_effect_summaries();
        module
    }

    fn recursive_countdown_module() -> Module {
        let memory_ty = EvoIRType::Mem(Region::All);
        let n = bits(0, 32);
        let memory = value(1, memory_ty.clone());
        let world = value(2, EvoIRType::World);
        let zero = bits(3, 32);
        let condition = bits(4, 1);
        let base_n = bits(5, 32);
        let base_memory = value(6, memory_ty.clone());
        let base_world = value(7, EvoIRType::World);
        let recur_n = bits(8, 32);
        let recur_memory = value(9, memory_ty.clone());
        let recur_world = value(10, EvoIRType::World);
        let one = bits(11, 32);
        let next_n = bits(12, 32);
        let result = bits(13, 32);
        let memory_out = value(14, memory_ty.clone());
        let world_out = value(15, EvoIRType::World);
        let countdown = Function {
            name: "countdown".to_owned(),
            params: vec![
                FunctionParam {
                    name: "n".to_owned(),
                    ty: EvoIRType::Bits(32),
                },
                FunctionParam {
                    name: "memory".to_owned(),
                    ty: memory_ty.clone(),
                },
                FunctionParam {
                    name: "world".to_owned(),
                    ty: EvoIRType::World,
                },
            ],
            ret_tys: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
            effects: Default::default(),
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![
                        argument("n", n.clone()),
                        argument("memory", memory.clone()),
                        argument("world", world.clone()),
                    ],
                    insts: vec![
                        Inst::Const(zero.clone(), 0),
                        Inst::ICmp(condition.clone(), CmpOp::Eq, n.clone(), zero),
                    ],
                    terminator: Inst::CondBr(
                        condition,
                        1,
                        vec![n.clone(), memory.clone(), world.clone()],
                        2,
                        vec![n, memory, world],
                    ),
                },
                Block {
                    id: 1,
                    args: vec![
                        argument("n", base_n.clone()),
                        argument("memory", base_memory.clone()),
                        argument("world", base_world.clone()),
                    ],
                    insts: vec![],
                    terminator: Inst::Ret(vec![base_n, base_memory, base_world]),
                },
                Block {
                    id: 2,
                    args: vec![
                        argument("n", recur_n.clone()),
                        argument("memory", recur_memory.clone()),
                        argument("world", recur_world.clone()),
                    ],
                    insts: vec![
                        Inst::Const(one.clone(), 1),
                        Inst::Sub(next_n.clone(), recur_n, one),
                        Inst::Call {
                            results: vec![result.clone()],
                            callee: "countdown".to_owned(),
                            args: vec![next_n],
                            mem_in: recur_memory,
                            world_in: recur_world,
                            mem_out: memory_out.clone(),
                            world_out: world_out.clone(),
                        },
                    ],
                    terminator: Inst::Ret(vec![result, memory_out, world_out]),
                },
            ],
        };

        let mut module = direct_call_module();
        let main = module.functions.pop().expect("fixture main");
        let mut main = main;
        let Inst::Const(_, immediate) = &mut main.blocks[0].insts[0] else {
            panic!("fixture constant");
        };
        *immediate = 3;
        let Inst::Call { callee, .. } = &mut main.blocks[0].insts[1] else {
            panic!("fixture call");
        };
        *callee = "countdown".to_owned();
        let mut module = Module {
            globals: vec![],
            functions: vec![countdown, main],
        };
        module.recompute_effect_summaries();
        module
    }

    #[test]
    fn module_validation_checks_symbols_ordinary_signature_and_effect_interface() {
        let module = direct_call_module();
        validate_module(&module).expect("well-typed direct call module");

        let mut duplicate = module.clone();
        duplicate.functions[1].name = "identity".to_owned();
        assert!(validate_module(&duplicate)
            .expect_err("duplicate symbols must fail")
            .contains("duplicate function"));

        let mut missing = module.clone();
        let Inst::Call { callee, .. } = &mut missing.functions[1].blocks[0].insts[1] else {
            panic!("fixture call");
        };
        *callee = "missing".to_owned();
        assert!(validate_module(&missing)
            .expect_err("missing callee must fail")
            .contains("missing callee"));

        let mut wrong_arity = module.clone();
        let Inst::Call { args, .. } = &mut wrong_arity.functions[1].blocks[0].insts[1] else {
            panic!("fixture call");
        };
        args.clear();
        assert!(validate_module(&wrong_arity)
            .expect_err("ordinary argument arity must fail")
            .contains("ordinary arguments"));

        let mut wrong_result_arity = module.clone();
        let caller = &mut wrong_result_arity.functions[1];
        caller.ret_tys.remove(0);
        let Inst::Call { results, .. } = &mut caller.blocks[0].insts[1] else {
            panic!("fixture call");
        };
        results.clear();
        let Inst::Ret(returned) = &mut caller.blocks[0].terminator else {
            panic!("fixture return");
        };
        returned.remove(0);
        assert!(validate_module(&wrong_result_arity)
            .expect_err("ordinary result arity must fail")
            .contains("ordinary results"));

        let mut wrong_argument_type = module.clone();
        let callee = &mut wrong_argument_type.functions[0];
        callee.params[0].ty = EvoIRType::Bits(64);
        callee.ret_tys[0] = EvoIRType::Bits(64);
        callee.blocks[0].args[0].value.ty = EvoIRType::Bits(64);
        let Inst::Ret(returned) = &mut callee.blocks[0].terminator else {
            panic!("fixture return");
        };
        returned[0].ty = EvoIRType::Bits(64);
        assert!(validate_module(&wrong_argument_type)
            .expect_err("ordinary argument type must fail")
            .contains("argument 0"));

        let mut wrong_result_type = module.clone();
        let caller = &mut wrong_result_type.functions[1];
        caller.ret_tys[0] = EvoIRType::Bits(64);
        let Inst::Call { results, .. } = &mut caller.blocks[0].insts[1] else {
            panic!("fixture call");
        };
        results[0].ty = EvoIRType::Bits(64);
        let Inst::Ret(returned) = &mut caller.blocks[0].terminator else {
            panic!("fixture return");
        };
        returned[0].ty = EvoIRType::Bits(64);
        assert!(validate_module(&wrong_result_type)
            .expect_err("ordinary result type must fail")
            .contains("result 0"));

        let mut missing_effect_interface = module.clone();
        let callee = &mut missing_effect_interface.functions[0];
        callee.params.truncate(1);
        callee.ret_tys.truncate(1);
        callee.blocks[0].args.truncate(1);
        let Inst::Ret(returned) = &mut callee.blocks[0].terminator else {
            panic!("fixture return");
        };
        returned.truncate(1);
        assert!(validate_module(&missing_effect_interface)
            .expect_err("callee effect interface must be explicit")
            .contains("lacks trailing mem<R>, world"));

        let mut reordered_caller_parameters = module.clone();
        let caller = &mut reordered_caller_parameters.functions[1];
        caller.params.swap(0, 1);
        caller.blocks[0].args.swap(0, 1);
        validate_ssa(caller).expect("reordered caller remains function-local SSA valid");
        assert!(validate_module(&reordered_caller_parameters)
            .expect_err("a call-bearing caller must have canonical trailing parameters")
            .contains("function 'main' must end parameters with mem<R>, world"));

        let mut reordered_caller_returns = module.clone();
        let caller = &mut reordered_caller_returns.functions[1];
        caller.ret_tys.swap(1, 2);
        let Inst::Ret(returned) = &mut caller.blocks[0].terminator else {
            panic!("fixture return");
        };
        returned.swap(1, 2);
        validate_ssa(caller).expect("reordered caller remains function-local SSA valid");
        assert!(validate_module(&reordered_caller_returns)
            .expect_err("a call-bearing caller must have canonical trailing returns")
            .contains("function 'main' must end returns with mem<All>, world"));

        let mut wrong_region = module;
        let callee = &mut wrong_region.functions[0];
        callee.params[1].ty = EvoIRType::Mem(Region::Stack);
        callee.ret_tys[1] = EvoIRType::Mem(Region::Stack);
        callee.blocks[0].args[1].value.ty = EvoIRType::Mem(Region::Stack);
        let Inst::Ret(returned) = &mut callee.blocks[0].terminator else {
            panic!("fixture return");
        };
        returned[1].ty = EvoIRType::Mem(Region::Stack);
        assert!(validate_module(&wrong_region)
            .expect_err("memory region mismatch must fail")
            .contains("memory token"));
    }

    #[test]
    fn module_validation_requires_the_exact_recorded_effect_fixed_point() {
        let mut module = direct_call_module();
        module.recompute_effect_summaries();
        validate_module(&module).expect("freshly derived summaries are exact");

        module.functions[0].effects.may_trap = true;
        let error = validate_module(&module).expect_err("an over-strong summary must fail");
        assert!(error.contains("recorded effect summary"), "{error}");

        module.recompute_effect_summaries();
        module.functions[0].blocks[0].terminator = Inst::Trap(TrapReason::ScalarFunctionFellOffEnd);
        let error = validate_module(&module).expect_err("a stale weak summary must fail");
        assert!(error.contains("recorded effect summary"), "{error}");
    }

    #[test]
    fn recursive_effect_derivation_reaches_the_least_shared_fixed_point() {
        let mut module = recursive_countdown_module();
        module.functions[0].blocks[1].terminator = Inst::Trap(TrapReason::ScalarFunctionFellOffEnd);
        module.recompute_effect_summaries();

        assert!(module.functions[0].effects.may_trap);
        assert!(module.functions[1].effects.may_trap);
        validate_module(&module).expect("the recursive least fixed point validates");
    }

    #[test]
    fn module_interpreter_executes_calls_and_preserves_tokens() {
        let outcome = interpret_module(
            &direct_call_module(),
            "main",
            &[
                RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
                RuntimeValue::new(EvoIRType::World, 19),
            ],
            InterpreterLimits::new(4),
        )
        .expect("direct call execution");
        assert_eq!(
            outcome.return_values,
            vec![
                RuntimeValue::bits(32, 41),
                RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
                RuntimeValue::new(EvoIRType::World, 19),
            ]
        );
        assert_eq!(outcome.steps, 4);
    }

    #[test]
    fn indirect_calls_validate_execute_and_report_null_provenance() {
        let arguments = [
            RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
            RuntimeValue::new(EvoIRType::World, 19),
        ];
        let module = indirect_call_module(false);
        validate_module(&module).expect("func_addr and indirect signature are valid");
        let outcome = interpret_module(&module, "main", &arguments, InterpreterLimits::new(5))
            .expect("indirect call execution");
        assert_eq!(outcome.return_values[0], RuntimeValue::bits(32, 41));
        assert_eq!(outcome.steps, 5);

        let error = interpret_module(
            &indirect_call_module(true),
            "main",
            &arguments,
            InterpreterLimits::new(3),
        )
        .expect_err("null fnptr must fail at the call");
        assert_eq!(error.code(), "E3015");
        assert_eq!(error.message(), "call-through-null-function-pointer");

        let mut mismatched = indirect_call_module(true);
        let [Inst::PtrNull(defined), _, Inst::IndirectCall { callee, .. }] =
            mismatched.functions[1].blocks[0].insts.as_mut_slice()
        else {
            panic!("fixture indirect call");
        };
        let EvoIRType::FnPtr { params, .. } = &mut defined.ty else {
            panic!("fixture fnptr type");
        };
        params[0] = EvoIRType::Bits(64);
        callee.ty = defined.ty.clone();
        assert!(validate_module(&mismatched)
            .expect_err("indirect signature mismatch must fail")
            .contains("E3017"));
    }

    #[test]
    fn target_aware_function_outcome_owns_one_memory_and_world_state() {
        let target = TargetDataLayout::aarch64_unknown_linux_gnu();
        let outcome = interpret_function_with_target(
            &threaded_function(),
            &[],
            &target,
            InterpreterLimits::new(5),
        )
        .expect("target-aware scalar execution");

        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 42)]);
        assert_eq!(outcome.steps, 5);
        assert_eq!(outcome.memory.config().byte_order(), target.byte_order());
        assert_eq!(
            outcome.memory.config().pointer_layout(),
            AccessLayout::from_type_layout(target.pointer_layout())
        );
        assert!(outcome.world.volatile_trace().is_empty());
    }

    #[test]
    fn direct_call_preserves_the_single_seeded_execution_state_without_reset() {
        let target = TargetDataLayout::aarch64_unknown_linux_gnu();
        let mut state = ExecutionState::for_target(&target).expect("valid execution state");
        let (memory, seeded_address) = state
            .memory
            .allocate_static(91, AccessLayout::new(4, 4), InitialState::Zeroed)
            .expect("seed static object");
        state.memory = memory;
        let expected_memory = state.memory.clone();
        let expected_world = state.world.clone();

        let outcome = interpret_module_in_state(
            &direct_call_module(),
            "main",
            &[
                RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
                RuntimeValue::new(EvoIRType::World, 19),
            ],
            InterpreterLimits::new(4),
            state,
        )
        .expect("direct call execution");

        assert_eq!(outcome.memory, expected_memory);
        assert_eq!(outcome.world, expected_world);
        assert_eq!(
            outcome
                .memory
                .object_info(seeded_address.object())
                .expect("callee must not reset the top-level memory")
                .origin
                .site,
            91
        );
    }

    #[test]
    fn direct_call_fuel_failure_returns_no_fabricated_outcome_state() {
        let target = TargetDataLayout::aarch64_unknown_linux_gnu();
        assert!(matches!(
            interpret_module_with_target(
                &direct_call_module(),
                "main",
                &[
                    RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
                    RuntimeValue::new(EvoIRType::World, 19),
                ],
                &target,
                InterpreterLimits::new(3),
            ),
            Err(InterpreterError::StepBudgetExceeded {
                budget: 3,
                steps: 3,
                ..
            })
        ));
    }

    #[test]
    fn module_interpreter_recurses_with_one_shared_step_budget() {
        let module = recursive_countdown_module();
        validate_module(&module).expect("recursive module validates");
        let arguments = [
            RuntimeValue::new(EvoIRType::Mem(Region::All), 23),
            RuntimeValue::new(EvoIRType::World, 29),
        ];
        let outcome = interpret_module(&module, "main", &arguments, InterpreterLimits::new(28))
            .expect("recursive execution uses explicit frames");
        assert_eq!(outcome.return_values[0], RuntimeValue::bits(32, 0));
        assert_eq!(outcome.steps, 28);
        assert!(matches!(
            interpret_module(&module, "main", &arguments, InterpreterLimits::new(27)),
            Err(InterpreterError::StepBudgetExceeded {
                budget: 27,
                steps: 27,
                ..
            })
        ));
    }

    #[test]
    fn signed_definedness_and_ub_assume_have_typed_runtime_semantics() {
        let ten = bits(0, 8);
        let one = bits(1, 8);
        let add_ok = bits(2, 1);
        let sum = bits(3, 8);
        let function = Function {
            name: "defined_add".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(8)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(ten.clone(), 10),
                    Inst::Const(one.clone(), 1),
                    Inst::SignedAddDefined(add_ok.clone(), ten.clone(), one.clone()),
                    Inst::UbAssume(add_ok, UbReason::SignedOverflow),
                    Inst::Add(sum.clone(), ten, one),
                ],
                terminator: Inst::Ret(vec![sum]),
            }],
        };
        let outcome = eval_function(&function, &[], InterpreterLimits::new(6))
            .expect("defined signed addition");
        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(8, 11)]);

        let mut wrong_predicate_type = function.clone();
        let Inst::SignedAddDefined(result, ..) = &mut wrong_predicate_type.blocks[0].insts[2]
        else {
            panic!("fixture definedness predicate");
        };
        result.ty = EvoIRType::Bits(2);
        let Inst::UbAssume(condition, _) = &mut wrong_predicate_type.blocks[0].insts[3] else {
            panic!("fixture ub.assume");
        };
        condition.ty = EvoIRType::Bits(2);
        assert!(validate_ssa(&wrong_predicate_type)
            .expect_err("definedness result must be bits<1>")
            .contains("signed-definedness result"));

        let mut wrong_operand_type = function.clone();
        let Inst::Const(right, _) = &mut wrong_operand_type.blocks[0].insts[1] else {
            panic!("fixture constant");
        };
        right.ty = EvoIRType::Bits(16);
        let Inst::SignedAddDefined(_, _, right) = &mut wrong_operand_type.blocks[0].insts[2] else {
            panic!("fixture definedness predicate");
        };
        right.ty = EvoIRType::Bits(16);
        assert!(validate_ssa(&wrong_operand_type)
            .expect_err("definedness operands must have equal width")
            .contains("different types"));

        let mut wrong_assumption_type = function.clone();
        let Inst::Const(condition, _) = &wrong_assumption_type.blocks[0].insts[0] else {
            panic!("fixture constant");
        };
        wrong_assumption_type.blocks[0].insts[3] =
            Inst::UbAssume(condition.clone(), UbReason::SignedOverflow);
        assert!(validate_ssa(&wrong_assumption_type)
            .expect_err("ub.assume condition must be bits<1>")
            .contains("ub.assume"));

        let max = bits(10, 8);
        let one = bits(11, 8);
        let add_ok = bits(12, 1);
        let overflow = Function {
            name: "overflow".to_owned(),
            params: vec![],
            ret_tys: vec![],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(max.clone(), 0x7f),
                    Inst::Const(one.clone(), 1),
                    Inst::SignedAddDefined(add_ok.clone(), max, one),
                    Inst::UbAssume(add_ok, UbReason::SignedOverflow),
                ],
                terminator: Inst::Ret(vec![]),
            }],
        };
        assert!(matches!(
            eval_function(&overflow, &[], InterpreterLimits::new(5)),
            Err(InterpreterError::UndefinedBehavior {
                diagnostic: "E3004",
                ..
            })
        ));

        let left = bits(20, 8);
        let right = bits(21, 8);
        let sub_ok = bits(22, 1);
        let mul_ok = bits(23, 1);
        let predicates = Function {
            name: "predicates".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(1), EvoIRType::Bits(1)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(left.clone(), 0x80),
                    Inst::Const(right.clone(), 2),
                    Inst::SignedSubDefined(sub_ok.clone(), left.clone(), right.clone()),
                    Inst::SignedMulDefined(mul_ok.clone(), left, right),
                ],
                terminator: Inst::Ret(vec![sub_ok, mul_ok]),
            }],
        };
        let outcome = eval_function(&predicates, &[], InterpreterLimits::new(5))
            .expect("definedness predicates execute");
        assert_eq!(
            outcome.return_values,
            vec![RuntimeValue::bits(1, 0), RuntimeValue::bits(1, 0)]
        );
    }

    #[test]
    fn extended_integer_ops_and_separated_ub_predicates_execute() {
        let neg_seven = bits(0, 8);
        let three = bits(1, 8);
        let seven = bits(2, 8);
        let two = bits(3, 8);
        let neg_four = bits(4, 8);
        let five = bits(5, 8);
        let wide_invalid_count = bits(6, 16);
        let srem = bits(7, 8);
        let urem = bits(8, 8);
        let shl = bits(9, 8);
        let ashr = bits(10, 8);
        let lshr = bits(11, 8);
        let and = bits(12, 8);
        let or = bits(13, 8);
        let xor = bits(14, 8);
        let invalid_shl = bits(15, 8);
        let operations = Function {
            name: "extended_integer_ops".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(8); 9],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(neg_seven.clone(), 0xf9),
                    Inst::Const(three.clone(), 3),
                    Inst::Const(seven.clone(), 7),
                    Inst::Const(two.clone(), 2),
                    Inst::Const(neg_four.clone(), 0xfc),
                    Inst::Const(five.clone(), 5),
                    Inst::Const(wide_invalid_count.clone(), 256),
                    Inst::SRem(srem.clone(), neg_seven, three.clone()),
                    Inst::URem(urem.clone(), seven, three.clone()),
                    Inst::Shl(shl.clone(), three.clone(), two.clone()),
                    Inst::AShr(ashr.clone(), neg_four.clone(), two.clone()),
                    Inst::LShr(lshr.clone(), neg_four, two),
                    Inst::And(and.clone(), five.clone(), three.clone()),
                    Inst::Or(or.clone(), five.clone(), three.clone()),
                    Inst::Xor(xor.clone(), five, three),
                    Inst::Shl(invalid_shl.clone(), shl.clone(), wide_invalid_count),
                ],
                terminator: Inst::Ret(vec![srem, urem, shl, ashr, lshr, and, or, xor, invalid_shl]),
            }],
        };
        let outcome = eval_function(&operations, &[], InterpreterLimits::new(17))
            .expect("extended integer operations execute");
        assert_eq!(
            outcome.return_values,
            vec![
                RuntimeValue::bits(8, 0xff),
                RuntimeValue::bits(8, 1),
                RuntimeValue::bits(8, 12),
                RuntimeValue::bits(8, 0xff),
                RuntimeValue::bits(8, 0x3f),
                RuntimeValue::bits(8, 1),
                RuntimeValue::bits(8, 7),
                RuntimeValue::bits(8, 6),
                RuntimeValue::bits(8, 0),
            ]
        );

        let zero = bits(20, 8);
        let one = bits(21, 8);
        let minimum = bits(22, 8);
        let minus_one = bits(23, 8);
        let minus_one_count = bits(24, 8);
        let width_count = bits(25, 8);
        let six = bits(26, 8);
        let seven = bits(27, 8);
        let predicates: Vec<Value> = (30..=40).map(|id| bits(id, 1)).collect();
        let predicate_function = Function {
            name: "separated_predicates".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(1); predicates.len()],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::Const(one.clone(), 1),
                    Inst::Const(minimum.clone(), 0x80),
                    Inst::Const(minus_one.clone(), 0xff),
                    Inst::Const(minus_one_count.clone(), 0xff),
                    Inst::Const(width_count.clone(), 8),
                    Inst::Const(six.clone(), 6),
                    Inst::Const(seven.clone(), 7),
                    Inst::DivisorNonZero(predicates[0].clone(), zero),
                    Inst::DivisorNonZero(predicates[1].clone(), one.clone()),
                    Inst::SignedDivRemNoOverflow(
                        predicates[2].clone(),
                        minimum.clone(),
                        minus_one.clone(),
                    ),
                    Inst::SignedDivRemNoOverflow(predicates[3].clone(), minimum, one.clone()),
                    Inst::ShiftCountDefined {
                        result: predicates[4].clone(),
                        shifted: one.clone(),
                        amount: minus_one_count,
                        amount_signed: true,
                    },
                    Inst::ShiftCountDefined {
                        result: predicates[5].clone(),
                        shifted: one.clone(),
                        amount: width_count.clone(),
                        amount_signed: false,
                    },
                    Inst::ShiftCountDefined {
                        result: predicates[6].clone(),
                        shifted: one.clone(),
                        amount: six.clone(),
                        amount_signed: true,
                    },
                    Inst::SignedShlDefined {
                        result: predicates[7].clone(),
                        shifted: minus_one,
                        amount: one.clone(),
                        amount_signed: true,
                    },
                    Inst::SignedShlDefined {
                        result: predicates[8].clone(),
                        shifted: one.clone(),
                        amount: seven,
                        amount_signed: true,
                    },
                    Inst::SignedShlDefined {
                        result: predicates[9].clone(),
                        shifted: one.clone(),
                        amount: six,
                        amount_signed: true,
                    },
                    Inst::SignedShlDefined {
                        result: predicates[10].clone(),
                        shifted: one,
                        amount: width_count,
                        amount_signed: true,
                    },
                ],
                terminator: Inst::Ret(predicates.clone()),
            }],
        };
        let outcome = eval_function(&predicate_function, &[], InterpreterLimits::new(20))
            .expect("separated UB predicates execute without host panics");
        assert_eq!(
            outcome.return_values,
            [false, true, false, true, false, false, true, false, false, true, false]
                .into_iter()
                .map(|value| RuntimeValue::bits(1, u64::from(value)))
                .collect::<Vec<_>>()
        );

        let mut mixed_shift_width = predicate_function;
        let Inst::Const(amount, _) = &mut mixed_shift_width.blocks[0].insts[4] else {
            panic!("fixture shift amount");
        };
        amount.ty = EvoIRType::Bits(16);
        let Inst::ShiftCountDefined { amount, .. } = &mut mixed_shift_width.blocks[0].insts[12]
        else {
            panic!("fixture shift predicate");
        };
        amount.ty = EvoIRType::Bits(16);
        validate_ssa(&mixed_shift_width)
            .expect("shifted value and promoted amount may have different widths");

        let shifted = bits(50, 64);
        let negative_narrow_count = bits(51, 1);
        let sixty_two = bits(52, 8);
        let sixty_three = bits(53, 8);
        let mixed_predicates: Vec<Value> = (54..=58).map(|id| bits(id, 1)).collect();
        let shifted_result = bits(59, 64);
        let mixed_width_shifts = Function {
            name: "mixed_width_shifts".to_owned(),
            params: vec![],
            ret_tys: vec![
                EvoIRType::Bits(64),
                EvoIRType::Bits(1),
                EvoIRType::Bits(1),
                EvoIRType::Bits(1),
                EvoIRType::Bits(1),
                EvoIRType::Bits(1),
            ],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(shifted.clone(), 1),
                    Inst::Const(negative_narrow_count.clone(), 1),
                    Inst::Const(sixty_two.clone(), 62),
                    Inst::Const(sixty_three.clone(), 63),
                    Inst::ShiftCountDefined {
                        result: mixed_predicates[0].clone(),
                        shifted: shifted.clone(),
                        amount: negative_narrow_count.clone(),
                        amount_signed: true,
                    },
                    Inst::ShiftCountDefined {
                        result: mixed_predicates[1].clone(),
                        shifted: shifted.clone(),
                        amount: sixty_three.clone(),
                        amount_signed: false,
                    },
                    Inst::SignedShlDefined {
                        result: mixed_predicates[2].clone(),
                        shifted: shifted.clone(),
                        amount: negative_narrow_count,
                        amount_signed: true,
                    },
                    Inst::SignedShlDefined {
                        result: mixed_predicates[3].clone(),
                        shifted: shifted.clone(),
                        amount: sixty_two.clone(),
                        amount_signed: true,
                    },
                    Inst::SignedShlDefined {
                        result: mixed_predicates[4].clone(),
                        shifted: shifted.clone(),
                        amount: sixty_three,
                        amount_signed: true,
                    },
                    Inst::Shl(shifted_result.clone(), shifted, sixty_two),
                ],
                terminator: Inst::Ret(
                    std::iter::once(shifted_result.clone())
                        .chain(mixed_predicates.iter().cloned())
                        .collect(),
                ),
            }],
        };
        let outcome = eval_function(&mixed_width_shifts, &[], InterpreterLimits::new(11))
            .expect("mixed-width shift predicates and operation execute");
        assert_eq!(
            outcome.return_values,
            vec![
                RuntimeValue::bits(64, 1_u64 << 62),
                RuntimeValue::bits(1, 0),
                RuntimeValue::bits(1, 1),
                RuntimeValue::bits(1, 0),
                RuntimeValue::bits(1, 1),
                RuntimeValue::bits(1, 0),
            ]
        );
    }

    #[test]
    fn ub_assume_preserves_each_typed_integer_diagnostic() {
        let zero = bits(0, 8);
        let division_condition = bits(1, 1);
        let remainder_condition = bits(2, 1);
        let minimum = bits(3, 8);
        let minus_one = bits(4, 8);
        let overflow_condition = bits(5, 1);
        let shifted_wide = bits(6, 64);
        let negative_narrow_count = bits(7, 1);
        let count_condition = bits(8, 1);
        let shifted_narrow = bits(9, 8);
        let seven = bits(10, 8);
        let signed_shift_condition = bits(11, 1);
        let cases = vec![
            (
                vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::DivisorNonZero(division_condition.clone(), zero.clone()),
                ],
                division_condition,
                UbReason::DivisionByZero,
                "division_by_zero",
                "E3002",
                "integer-division-by-zero",
            ),
            (
                vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::DivisorNonZero(remainder_condition.clone(), zero),
                ],
                remainder_condition,
                UbReason::RemainderByZero,
                "remainder_by_zero",
                "E3003",
                "integer-remainder-by-zero",
            ),
            (
                vec![
                    Inst::Const(minimum.clone(), 0x80),
                    Inst::Const(minus_one.clone(), 0xff),
                    Inst::SignedDivRemNoOverflow(overflow_condition.clone(), minimum, minus_one),
                ],
                overflow_condition,
                UbReason::SignedOverflow,
                "signed_overflow",
                "E3004",
                "signed-integer-overflow",
            ),
            (
                vec![
                    Inst::Const(shifted_wide.clone(), 1),
                    Inst::Const(negative_narrow_count.clone(), 1),
                    Inst::ShiftCountDefined {
                        result: count_condition.clone(),
                        shifted: shifted_wide,
                        amount: negative_narrow_count,
                        amount_signed: true,
                    },
                ],
                count_condition,
                UbReason::InvalidShiftCount,
                "invalid_shift_count",
                "E3005",
                "invalid-shift-count",
            ),
            (
                vec![
                    Inst::Const(shifted_narrow.clone(), 1),
                    Inst::Const(seven.clone(), 7),
                    Inst::SignedShlDefined {
                        result: signed_shift_condition.clone(),
                        shifted: shifted_narrow,
                        amount: seven,
                        amount_signed: true,
                    },
                ],
                signed_shift_condition,
                UbReason::InvalidSignedLeftShift,
                "invalid_signed_left_shift",
                "E3006",
                "invalid-signed-left-shift",
            ),
        ];

        for (
            mut instructions,
            condition,
            reason,
            expected_mnemonic,
            expected_diagnostic,
            expected_message,
        ) in cases
        {
            assert_eq!(reason.mnemonic(), expected_mnemonic);
            instructions.push(Inst::UbAssume(condition, reason));
            let budget = instructions.len() as u64 + 1;
            let function = Function {
                name: "typed_ub".to_owned(),
                params: vec![],
                ret_tys: vec![],
                effects: Default::default(),
                blocks: vec![Block {
                    id: 0,
                    args: vec![],
                    insts: instructions,
                    terminator: Inst::Ret(vec![]),
                }],
            };
            let error = eval_function(&function, &[], InterpreterLimits::new(budget))
                .expect_err("a false typed UB obligation must fail");
            assert_eq!(error.code(), expected_diagnostic);
            assert_eq!(error.message(), expected_message);
            let InterpreterError::UndefinedBehavior {
                block,
                diagnostic,
                message,
            } = error
            else {
                panic!("unexpected typed UB failure: {error:?}");
            };
            assert_eq!(block, 0);
            assert_eq!(diagnostic, expected_diagnostic);
            assert_eq!(message, expected_message);
        }
    }

    #[test]
    fn uninitialized_is_an_ordinary_typed_definition_with_no_operands() {
        let result = bits(70, 32);
        let instruction = Inst::Uninitialized(result.clone());
        assert_eq!(inst_results(&instruction), vec![&result]);
        assert!(inst_uses(&instruction).is_empty());

        let function = Function {
            name: "uninitialized_scalar".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 37,
                args: vec![],
                insts: vec![instruction],
                terminator: Inst::Ret(vec![result]),
            }],
        };
        validate_ssa(&function).expect("ordinary typed uninitialized values are valid SSA");

        let pointer = value(71, EvoIRType::Ptr(AddressSpace::Stack));
        let pointer_function = Function {
            name: "uninitialized_pointer".to_owned(),
            params: vec![],
            ret_tys: vec![],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![Inst::Uninitialized(pointer)],
                terminator: Inst::Ret(vec![]),
            }],
        };
        validate_ssa(&pointer_function)
            .expect("an ordinary pointer is a supported uninitialized value type");

        for forbidden in [
            EvoIRType::Mem(Region::Stack),
            EvoIRType::World,
            EvoIRType::Tuple(vec![
                EvoIRType::Bits(32),
                EvoIRType::Tuple(vec![EvoIRType::Mem(Region::Heap)]),
            ]),
        ] {
            let invalid = Function {
                name: "uninitialized_effect_token".to_owned(),
                params: vec![],
                ret_tys: vec![],
                effects: Default::default(),
                blocks: vec![Block {
                    id: 0,
                    args: vec![],
                    insts: vec![Inst::Uninitialized(value(80, forbidden))],
                    terminator: Inst::Ret(vec![]),
                }],
            };
            let error = validate_ssa(&invalid)
                .expect_err("effect tokens cannot masquerade as uninitialized ordinary data");
            assert!(
                error.contains("uninitialized result")
                    && error.contains("memory/world effect token"),
                "unexpected validator error: {error}"
            );
        }

        let invalid_void = Function {
            name: "uninitialized_void".to_owned(),
            params: vec![],
            ret_tys: vec![],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![Inst::Uninitialized(value(90, EvoIRType::Void))],
                terminator: Inst::Ret(vec![]),
            }],
        };
        reject(&invalid_void, "void is not an SSA value type");
    }

    #[test]
    fn uninitialized_propagates_across_block_arguments_without_being_read() {
        let poison = bits(100, 32);
        let carried = bits(101, 32);
        let replacement = bits(102, 32);
        let function = Function {
            name: "propagate_uninitialized".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![],
                    insts: vec![Inst::Uninitialized(poison.clone())],
                    terminator: Inst::Br(1, vec![poison]),
                },
                Block {
                    id: 1,
                    args: vec![argument("carried", carried)],
                    insts: vec![Inst::Const(replacement.clone(), 7)],
                    terminator: Inst::Ret(vec![replacement]),
                },
            ],
        };

        let outcome = interpret_function(&function, &[], InterpreterLimits::new(4))
            .expect("copying poison through a block argument is not a semantic read");
        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 7)]);
        assert_eq!(outcome.steps, 4);

        let module_outcome = interpret_module(
            &Module {
                globals: vec![],
                functions: vec![function.clone()],
            },
            "propagate_uninitialized",
            &[],
            InterpreterLimits::new(4),
        )
        .expect("module block-argument transfer preserves poison without reading it");
        assert_eq!(
            module_outcome.return_values,
            vec![RuntimeValue::bits(32, 7)]
        );
        assert_eq!(module_outcome.steps, 4);

        let mut consumed = function;
        consumed.name = "consume_propagated_uninitialized".to_owned();
        let carried = consumed.blocks[1].args[0].value.clone();
        consumed.blocks[1].terminator = Inst::Ret(vec![carried]);
        assert_eq!(
            interpret_function(&consumed, &[], InterpreterLimits::new(4)),
            Err(InterpreterError::UninitializedRead { block: 1 })
        );
        assert_eq!(
            interpret_module(
                &Module {
                    globals: vec![],
                    functions: vec![consumed],
                },
                "consume_propagated_uninitialized",
                &[],
                InterpreterLimits::new(4),
            ),
            Err(InterpreterError::UninitializedRead { block: 1 })
        );
    }

    #[test]
    fn loop_carried_uninitialized_value_can_be_definitely_assigned_before_read() {
        let initial_x = bits(110, 32);
        let initial_i = bits(111, 32);
        let header_x = bits(112, 32);
        let header_i = bits(113, 32);
        let limit = bits(114, 32);
        let condition = bits(115, 1);
        let body_x = bits(116, 32);
        let body_i = bits(117, 32);
        let assigned_x = bits(118, 32);
        let one = bits(119, 32);
        let next_i = bits(120, 32);
        let exit_x = bits(121, 32);
        let function = Function {
            name: "definite_assignment_loop".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        Inst::Uninitialized(initial_x.clone()),
                        Inst::Const(initial_i.clone(), 0),
                    ],
                    terminator: Inst::Br(1, vec![initial_x, initial_i]),
                },
                Block {
                    id: 1,
                    args: vec![
                        argument("x", header_x.clone()),
                        argument("i", header_i.clone()),
                    ],
                    insts: vec![
                        Inst::Const(limit.clone(), 1),
                        Inst::ICmp(condition.clone(), CmpOp::ULt, header_i.clone(), limit),
                    ],
                    terminator: Inst::CondBr(
                        condition,
                        2,
                        vec![header_x.clone(), header_i],
                        3,
                        vec![header_x],
                    ),
                },
                Block {
                    id: 2,
                    args: vec![argument("x", body_x), argument("i", body_i.clone())],
                    insts: vec![
                        Inst::Const(assigned_x.clone(), 7),
                        Inst::Const(one.clone(), 1),
                        Inst::Add(next_i.clone(), body_i, one),
                    ],
                    terminator: Inst::Br(1, vec![assigned_x, next_i]),
                },
                Block {
                    id: 3,
                    args: vec![argument("x", exit_x.clone())],
                    insts: vec![],
                    terminator: Inst::Ret(vec![exit_x]),
                },
            ],
        };

        let outcome = interpret_function(&function, &[], InterpreterLimits::new(14))
            .expect("the loop assigns x before the only semantic read at return");
        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 7)]);
        assert_eq!(outcome.steps, 14);

        let module_outcome = interpret_module(
            &Module {
                globals: vec![],
                functions: vec![function],
            },
            "definite_assignment_loop",
            &[],
            InterpreterLimits::new(14),
        )
        .expect("module loop execution overwrites poison before the return read");
        assert_eq!(
            module_outcome.return_values,
            vec![RuntimeValue::bits(32, 7)]
        );
        assert_eq!(module_outcome.steps, 14);
    }

    #[test]
    fn uninitialized_return_traps_with_e3001_in_function_and_module_interpreters() {
        let result = bits(100, 32);
        let function = Function {
            name: "uninitialized_read".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 37,
                args: vec![],
                insts: vec![Inst::Uninitialized(result.clone())],
                terminator: Inst::Ret(vec![result]),
            }],
        };

        assert!(matches!(
            interpret_function(&function, &[], InterpreterLimits::new(1)),
            Err(InterpreterError::StepBudgetExceeded {
                budget: 1,
                steps: 1,
                block: 37,
            })
        ));
        let function_error = interpret_function(&function, &[], InterpreterLimits::new(2))
            .expect_err("returning an uninitialized ordinary result is a semantic read");
        assert_eq!(
            function_error,
            InterpreterError::UninitializedRead { block: 37 }
        );
        assert_eq!(function_error.code(), "E3001");
        assert_eq!(function_error.message(), "read-of-uninitialized-object");

        let module = Module {
            globals: vec![],
            functions: vec![function],
        };
        assert!(matches!(
            interpret_module(
                &module,
                "uninitialized_read",
                &[],
                InterpreterLimits::new(1),
            ),
            Err(InterpreterError::StepBudgetExceeded {
                budget: 1,
                steps: 1,
                block: 37,
            })
        ));
        let module_error = interpret_module(
            &module,
            "uninitialized_read",
            &[],
            InterpreterLimits::new(2),
        )
        .expect_err("module execution must preserve the typed uninitialized-read trap");
        assert_eq!(
            module_error,
            InterpreterError::UninitializedRead { block: 37 }
        );
        assert_eq!(module_error.code(), "E3001");
        assert_eq!(module_error.message(), "read-of-uninitialized-object");
    }

    #[test]
    fn arithmetic_comparison_cast_condition_and_call_consume_uninitialized_values() {
        let poison = bits(130, 32);
        let one = bits(131, 32);
        let arithmetic_result = bits(132, 32);
        let comparison_result = bits(133, 1);
        let cast_result = bits(134, 64);
        let zext_source = bits(135, 8);
        let zext_result = bits(136, 32);
        let trunc_source = bits(137, 32);
        let trunc_result = bits(138, 8);
        let shifted_poison = bits(139, 32);
        let shift_amount = bits(140, 32);
        let shift_predicate = bits(141, 1);
        let cases = vec![
            (
                Function {
                    name: "uninitialized_arithmetic".to_owned(),
                    params: vec![],
                    ret_tys: vec![EvoIRType::Bits(32)],
                    effects: Default::default(),
                    blocks: vec![Block {
                        id: 0,
                        args: vec![],
                        insts: vec![
                            Inst::Uninitialized(poison.clone()),
                            Inst::Const(one.clone(), 1),
                            Inst::Add(arithmetic_result.clone(), poison.clone(), one.clone()),
                        ],
                        terminator: Inst::Ret(vec![arithmetic_result]),
                    }],
                },
                3,
            ),
            (
                Function {
                    name: "uninitialized_comparison".to_owned(),
                    params: vec![],
                    ret_tys: vec![EvoIRType::Bits(1)],
                    effects: Default::default(),
                    blocks: vec![Block {
                        id: 0,
                        args: vec![],
                        insts: vec![
                            Inst::Uninitialized(poison.clone()),
                            Inst::Const(one.clone(), 1),
                            Inst::ICmp(comparison_result.clone(), CmpOp::Eq, poison.clone(), one),
                        ],
                        terminator: Inst::Ret(vec![comparison_result]),
                    }],
                },
                3,
            ),
            (
                Function {
                    name: "uninitialized_cast".to_owned(),
                    params: vec![],
                    ret_tys: vec![EvoIRType::Bits(64)],
                    effects: Default::default(),
                    blocks: vec![Block {
                        id: 0,
                        args: vec![],
                        insts: vec![
                            Inst::Uninitialized(poison.clone()),
                            Inst::SExt(cast_result.clone(), poison, EvoIRType::Bits(64)),
                        ],
                        terminator: Inst::Ret(vec![cast_result]),
                    }],
                },
                2,
            ),
            (
                Function {
                    name: "uninitialized_zero_extend".to_owned(),
                    params: vec![],
                    ret_tys: vec![EvoIRType::Bits(32)],
                    effects: Default::default(),
                    blocks: vec![Block {
                        id: 0,
                        args: vec![],
                        insts: vec![
                            Inst::Uninitialized(zext_source.clone()),
                            Inst::ZExt(zext_result.clone(), zext_source, EvoIRType::Bits(32)),
                        ],
                        terminator: Inst::Ret(vec![zext_result]),
                    }],
                },
                2,
            ),
            (
                Function {
                    name: "uninitialized_truncate".to_owned(),
                    params: vec![],
                    ret_tys: vec![EvoIRType::Bits(8)],
                    effects: Default::default(),
                    blocks: vec![Block {
                        id: 0,
                        args: vec![],
                        insts: vec![
                            Inst::Uninitialized(trunc_source.clone()),
                            Inst::Trunc(trunc_result.clone(), trunc_source, EvoIRType::Bits(8)),
                        ],
                        terminator: Inst::Ret(vec![trunc_result]),
                    }],
                },
                2,
            ),
            (
                Function {
                    name: "uninitialized_shifted_predicate_operand".to_owned(),
                    params: vec![],
                    ret_tys: vec![EvoIRType::Bits(1)],
                    effects: Default::default(),
                    blocks: vec![Block {
                        id: 0,
                        args: vec![],
                        insts: vec![
                            Inst::Uninitialized(shifted_poison.clone()),
                            Inst::Const(shift_amount.clone(), 1),
                            Inst::ShiftCountDefined {
                                result: shift_predicate.clone(),
                                shifted: shifted_poison,
                                amount: shift_amount,
                                amount_signed: false,
                            },
                        ],
                        terminator: Inst::Ret(vec![shift_predicate]),
                    }],
                },
                3,
            ),
        ];
        for (function, consumer_step) in &cases {
            assert!(matches!(
                interpret_function(function, &[], InterpreterLimits::new(consumer_step - 1),),
                Err(InterpreterError::StepBudgetExceeded { .. })
            ));
            let error = interpret_function(function, &[], InterpreterLimits::new(*consumer_step))
                .expect_err("semantic instruction operands must reject uninitialized values");
            assert_eq!(error, InterpreterError::UninitializedRead { block: 0 });
            assert_eq!(error.code(), "E3001");
            assert_eq!(error.message(), "read-of-uninitialized-object");
        }

        let poison_condition = bits(150, 1);
        let condition_function = Function {
            name: "uninitialized_condition".to_owned(),
            params: vec![],
            ret_tys: vec![],
            effects: Default::default(),
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![],
                    insts: vec![Inst::Uninitialized(poison_condition.clone())],
                    terminator: Inst::CondBr(poison_condition, 1, vec![], 2, vec![]),
                },
                Block {
                    id: 1,
                    args: vec![],
                    insts: vec![],
                    terminator: Inst::Ret(vec![]),
                },
                Block {
                    id: 2,
                    args: vec![],
                    insts: vec![],
                    terminator: Inst::Ret(vec![]),
                },
            ],
        };
        assert!(matches!(
            interpret_function(&condition_function, &[], InterpreterLimits::new(1)),
            Err(InterpreterError::StepBudgetExceeded { .. })
        ));
        assert_eq!(
            interpret_function(&condition_function, &[], InterpreterLimits::new(2)),
            Err(InterpreterError::UninitializedRead { block: 0 })
        );

        let mut call_module = direct_call_module();
        let main = call_module
            .functions
            .iter_mut()
            .find(|function| function.name == "main")
            .expect("fixture main");
        let argument = match &main.blocks[0].insts[0] {
            Inst::Const(argument, _) => argument.clone(),
            _ => panic!("fixture ordinary call argument"),
        };
        main.blocks[0].insts[0] = Inst::Uninitialized(argument);
        let entry_args = [
            RuntimeValue::new(EvoIRType::Mem(Region::All), 10),
            RuntimeValue::new(EvoIRType::World, 20),
        ];
        assert!(matches!(
            interpret_module(&call_module, "main", &entry_args, InterpreterLimits::new(1),),
            Err(InterpreterError::StepBudgetExceeded { .. })
        ));
        let error = interpret_module(&call_module, "main", &entry_args, InterpreterLimits::new(2))
            .expect_err("an ordinary call argument semantically consumes its value");
        assert_eq!(error, InterpreterError::UninitializedRead { block: 0 });
        assert_eq!(error.code(), "E3001");
        assert_eq!(error.message(), "read-of-uninitialized-object");

        let mut callee_return_module = direct_call_module();
        let callee = callee_return_module
            .functions
            .iter_mut()
            .find(|function| function.name == "identity")
            .expect("fixture callee");
        let poison_result = bits(6, 32);
        let memory = callee.blocks[0].args[1].value.clone();
        let world = callee.blocks[0].args[2].value.clone();
        callee.blocks[0].id = 9;
        callee.blocks[0].insts = vec![Inst::Uninitialized(poison_result.clone())];
        callee.blocks[0].terminator = Inst::Ret(vec![poison_result, memory, world]);
        assert!(matches!(
            interpret_module(
                &callee_return_module,
                "main",
                &entry_args,
                InterpreterLimits::new(3),
            ),
            Err(InterpreterError::StepBudgetExceeded { .. })
        ));
        let error = interpret_module(
            &callee_return_module,
            "main",
            &entry_args,
            InterpreterLimits::new(4),
        )
        .expect_err("callee ordinary returns consume poison before caller binding");
        assert_eq!(error, InterpreterError::UninitializedRead { block: 9 });
        assert_eq!(error.code(), "E3001");
        assert_eq!(error.message(), "read-of-uninitialized-object");
    }

    #[test]
    fn scalar_fallthrough_trap_is_a_typed_zero_operand_terminator() {
        let reason = TrapReason::ScalarFunctionFellOffEnd;
        assert_eq!(reason.diagnostic(), "E3007");
        assert_eq!(reason.message(), "scalar-function-fell-off-end");

        let trap = Inst::Trap(reason);
        assert!(inst_results(&trap).is_empty());
        assert!(inst_uses(&trap).is_empty());
        assert!(is_terminator(&trap));

        let function = Function {
            name: "fallthrough".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 7,
                args: vec![],
                insts: vec![],
                terminator: trap.clone(),
            }],
        };
        validate_ssa(&function).expect("a typed trap is a valid CFG sink without a fake return");

        let mut misplaced = function;
        misplaced.blocks[0].insts.push(trap);
        reject(&misplaced, "terminator appears in instruction");
    }

    #[test]
    fn scalar_fallthrough_traps_only_on_the_executed_path() {
        let condition = bits(160, 1);
        let safe_value = bits(161, 32);
        let returned = bits(162, 32);
        let function = Function {
            name: "conditional_fallthrough".to_owned(),
            params: vec![FunctionParam {
                name: "take_safe_return".to_owned(),
                ty: EvoIRType::Bits(1),
            }],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: EffectSummary {
                may_trap: true,
                ..EffectSummary::default()
            },
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![argument("take_safe_return", condition.clone())],
                    insts: vec![Inst::Const(safe_value.clone(), 7)],
                    terminator: Inst::CondBr(condition, 1, vec![safe_value], 2, vec![]),
                },
                Block {
                    id: 1,
                    args: vec![argument("returned", returned.clone())],
                    insts: vec![],
                    terminator: Inst::Ret(vec![returned]),
                },
                Block {
                    id: 2,
                    args: vec![],
                    insts: vec![],
                    terminator: Inst::Trap(TrapReason::ScalarFunctionFellOffEnd),
                },
            ],
        };
        validate_ssa(&function).expect("return and fallthrough paths are both well formed");

        let safe = interpret_function(
            &function,
            &[RuntimeValue::bits(1, 1)],
            InterpreterLimits::new(3),
        )
        .expect("an unexecuted fallthrough trap must not poison the function");
        assert_eq!(safe.return_values, vec![RuntimeValue::bits(32, 7)]);
        assert_eq!(safe.steps, 3);

        let error = interpret_function(
            &function,
            &[RuntimeValue::bits(1, 0)],
            InterpreterLimits::new(3),
        )
        .expect_err("executed scalar fallthrough must fail closed");
        assert_eq!(error, InterpreterError::MissingReturn { block: 2 });
        assert_eq!(error.code(), "E3007");
        assert_eq!(error.message(), "scalar-function-fell-off-end");

        let module_error = interpret_module(
            &Module {
                globals: vec![],
                functions: vec![function],
            },
            "conditional_fallthrough",
            &[RuntimeValue::bits(1, 0)],
            InterpreterLimits::new(3),
        )
        .expect_err("module execution must preserve the typed missing-return failure");
        assert_eq!(module_error, InterpreterError::MissingReturn { block: 2 });
        assert_eq!(module_error.code(), "E3007");
        assert_eq!(module_error.message(), "scalar-function-fell-off-end");
    }

    #[test]
    fn called_scalar_fallthrough_needs_no_placeholder_return_or_effect_tokens() {
        let mut module = direct_call_module();
        let callee = module
            .functions
            .iter_mut()
            .find(|function| function.name == "identity")
            .expect("fixture callee");
        callee.blocks[0].id = 9;
        callee.blocks[0].insts.clear();
        callee.blocks[0].terminator = Inst::Trap(TrapReason::ScalarFunctionFellOffEnd);
        module.recompute_effect_summaries();
        validate_module(&module)
            .expect("a trapping callee needs no fabricated ordinary or token return operands");

        let error = interpret_module(
            &module,
            "main",
            &[
                RuntimeValue::new(EvoIRType::Mem(Region::All), 10),
                RuntimeValue::new(EvoIRType::World, 20),
            ],
            InterpreterLimits::new(3),
        )
        .expect_err("the callee fallthrough trap must fire only after the call executes");
        assert_eq!(error, InterpreterError::MissingReturn { block: 9 });
        assert_eq!(error.code(), "E3007");
        assert_eq!(error.message(), "scalar-function-fell-off-end");
    }
}
