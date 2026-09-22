//! EvoIR: typed block-argument SSA for optimization
//!
//! Values carry an identity (`id`) and a type so that the SSA validator can
//! verify single-static-definition form before any optimization transforms.

use std::collections::{HashMap, HashSet};
use std::fmt;

pub use evocc_memory::{AccessLayout, CopyVolatility, IndexSignedness, OffsetDirection};
use evocc_memory::{
    ByteSequence, FrameId, InitialState, LibraryValue, MemoryConfig, MemoryFault, MemoryLimits,
    MemoryState, Pointer, PointerValue, ScopeId, WorldLimits, WorldState,
};
pub use evocc_types::{canonical_marks, param_mark, EvoIRType, ParamMark, RecordShape};
use evocc_types::{AddressSpace, Region, TargetDataLayout};

mod assembly;
mod atomic;
mod floating;
pub use atomic::{AtomicAccess, AtomicOp, AtomicOrder, AtomicRmwOp};
pub use floating::{floating_kind, FloatingInst, FloatingOp};
mod text;
mod text_parse;
mod variadic;
mod vector;
pub use variadic::{call_argument_marks, VariadicAccess, VariadicOp};
pub use vector::{integer_vector_shape, VectorInst, VectorOp};

/// A terminator edge: positional SSA bindings, independent of its producer.
pub use assembly::AssemblyEdge as Edge;
pub use assembly::{
    assembly_address_type, assembly_clobber, assembly_memory_constraint, assembly_register_binding,
    assembly_register_type, AssemblyEdge, AssemblyInput, AssemblyLabel, AssemblyOutput,
    InlineAssembly,
};
pub use text::{render_global, render_module, validate_section9_bytes};
pub use text_parse::{parse_global_text, parse_section9_module};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitOp {
    LeadingZeros,
    TrailingZeros,
    Population,
    ByteSwap,
}

impl BitOp {
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::LeadingZeros => "ctlz",
            Self::TrailingZeros => "cttz",
            Self::Population => "ctpop",
            Self::ByteSwap => "bswap",
        }
    }
}

/// The source-level undefined-behavior class guarded by [`Inst::UbAssume`].
///
/// The condition alone is deliberately insufficient to recover this
/// information: two arbitrary `bits<1>` values have identical EvoIR types but
/// may represent different C runtime diagnostics.  Lowering therefore records
/// the reason explicitly and the interpreter preserves it on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UbReason {
    BitBuiltinZeroOperand,
    DivisionByZero,
    RemainderByZero,
    SignedOverflow,
    InvalidShiftCount,
    InvalidSignedLeftShift,
    FloatingToInteger,
}

impl UbReason {
    /// Stable diagnostic code shared with the C-HIR scalar interpreter.
    #[must_use]
    pub const fn diagnostic(self) -> &'static str {
        match self {
            Self::BitBuiltinZeroOperand => "E3022",
            Self::DivisionByZero => "E3002",
            Self::RemainderByZero => "E3003",
            Self::SignedOverflow => "E3004",
            Self::InvalidShiftCount => "E3005",
            Self::InvalidSignedLeftShift => "E3006",
            Self::FloatingToInteger => "E3063",
        }
    }

    /// Stable diagnostic message shared with the C-HIR scalar interpreter.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::BitBuiltinZeroOperand => "bit-builtin-zero-operand",
            Self::DivisionByZero => "integer-division-by-zero",
            Self::RemainderByZero => "integer-remainder-by-zero",
            Self::SignedOverflow => "signed-integer-overflow",
            Self::InvalidShiftCount => "invalid-shift-count",
            Self::InvalidSignedLeftShift => "invalid-signed-left-shift",
            Self::FloatingToInteger => "floating-to-integer-not-representable",
        }
    }

    /// Stable suffix used by the Section 9 textual syntax.
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::BitBuiltinZeroOperand => "bit_builtin_zero_operand",
            Self::DivisionByZero => "division_by_zero",
            Self::RemainderByZero => "remainder_by_zero",
            Self::SignedOverflow => "signed_overflow",
            Self::InvalidShiftCount => "invalid_shift_count",
            Self::InvalidSignedLeftShift => "invalid_signed_left_shift",
            Self::FloatingToInteger => "floating_to_integer",
        }
    }
}

/// A typed, fail-closed reason for terminating execution without returning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrapReason {
    ScalarFunctionFellOffEnd,
    UnreachableReached,
    BuiltinTrap,
}

impl TrapReason {
    /// Stable diagnostic code shared with the C-HIR scalar interpreter.
    #[must_use]
    pub const fn diagnostic(self) -> &'static str {
        match self {
            Self::ScalarFunctionFellOffEnd => "E3007",
            Self::UnreachableReached => "E3024",
            Self::BuiltinTrap => "E3047",
        }
    }

    /// Stable diagnostic message shared with the C-HIR scalar interpreter.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::ScalarFunctionFellOffEnd => "scalar-function-fell-off-end",
            Self::UnreachableReached => "unreachable-reached",
            Self::BuiltinTrap => "builtin-trap-reached",
        }
    }

    /// Stable suffix used by the Section 9 textual syntax.
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::ScalarFunctionFellOffEnd => "scalar_function_fell_off_end",
            Self::UnreachableReached => "unreachable_reached",
            Self::BuiltinTrap => "builtin_trap",
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
    Floating(FloatingInst),
    TlsAddr(TlsAddress),
    Vector(VectorInst),
    /// One naturally aligned atomic access with an explicit memory order.
    Atomic(AtomicAccess),
    /// Select one complete, explicitly bound successor by its code address.
    IndirectBr {
        target: Value,
        edges: Vec<Edge>,
    },
    BitOperation {
        result: Value,
        operand: Value,
        op: BitOp,
    },
    /// Typed access to an anonymous argument list with linear effect tokens.
    VaAccess(VariadicAccess),
    /// Opaque assembly with explicit register results and effect tokens.
    Asm(InlineAssembly),
    /// Opaque assembly terminator; results are available on every edge.
    AsmGoto {
        assembly: InlineAssembly,
        labels: Vec<AssemblyLabel>,
        fallthrough: AssemblyEdge,
    },
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
    /// Integer addresses and pointer provenance are bridged only by these forms.
    PointerCast {
        result: Value,
        operand: Value,
    },

    /// Return the stable address of one module global.
    GlobalAddr {
        result: Value,
        symbol: String,
    },

    /// Return one function's stable provenance with its complete signature.
    FuncAddr {
        result: Value,
        symbol: String,
    },

    /// Return the address of the first instruction of one block of the
    /// current function, with *code* provenance (revision 1.9.68).
    ///
    /// Two of the same block are equal, of different blocks unequal, and a
    /// load, store or `access_in_bounds` through one is `E3013`. It exists
    /// because the kernel writes `({ __label__ __here; __here: (unsigned
    /// long)&&__here; })` to record an instruction address; the value is only
    /// ever compared, stored and printed.
    BlockAddr {
        result: Value,
        block: BlockId,
    },

    /// Return the address the current activation will return to, with *code*
    /// provenance (revision 1.9.72).
    ///
    /// Section 3.7: "equal to itself within one activation, comparable,
    /// storable, `ptr.to_int`-able, and `E3013` when dereferenced"; the
    /// interpreters give each activation a fresh value. It exists because the
    /// kernel's `_RET_IP_` is `(unsigned long)__builtin_return_address(0)`.
    ReturnAddress {
        result: Value,
    },

    /// Return the address of this activation's frame record (depth zero), with
    /// *stack* provenance of an object no load or store may touch (revision
    /// 1.9.72).
    ///
    /// Section 3.7 gives it the same fresh-per-activation identity as
    /// `return_address`; only `E3013` separates it from an ordinary object.
    /// A nonzero depth follows saved caller frame pointers on the machine;
    /// interpreters preserve their explicit physical-stack boundary.
    FrameAddress {
        result: Value,
        depth: u16,
    },

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

    /// Function-lifetime automatic storage whose byte size is an SSA value.
    DynamicAlloca {
        result: Value,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
        origin: u64,
        size: Value,
        align: u32,
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

    /// Copy a complete object representation, including initializedness and
    /// pointer provenance, between identical or disjoint ranges.
    Copy {
        destination: Value,
        source: Value,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
        layout: AccessLayout,
        volatile: CopyVolatility,
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
        /// Call-site byval marks for anonymous records; fixed marks live on
        /// the callee signature. Empty, or one mark per ordinary argument.
        arg_marks: Vec<ParamMark>,
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
        arg_marks: Vec<ParamMark>,
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
        Inst::TlsAddr(access) => access.results(),
        Inst::Vector(operation) => vec![&operation.result],
        Inst::Floating(operation) => vec![&operation.result],
        Inst::IndirectBr { .. } => vec![],
        Inst::PointerCast { result, .. } => vec![result],
        Inst::BitOperation { result, .. } => vec![result],
        Inst::VaAccess(access) => access.results(),
        Inst::Atomic(access) => access.results(),
        Inst::Asm(assembly) | Inst::AsmGoto { assembly, .. } => assembly.results(),
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
        | Inst::BlockAddr { result, .. }
        | Inst::ReturnAddress { result }
        | Inst::FrameAddress { result, .. }
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
        }
        | Inst::Copy {
            mem_out, world_out, ..
        } => vec![mem_out, world_out],
        Inst::Alloca {
            result,
            mem_out,
            world_out,
            ..
        }
        | Inst::DynamicAlloca {
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
        Inst::TlsAddr(access) => access.operands(),
        Inst::Vector(operation) => operation.arguments.iter().collect(),
        Inst::Floating(operation) => operation.arguments.iter().collect(),
        Inst::IndirectBr { target, edges } => std::iter::once(target)
            .chain(edges.iter().flat_map(|edge| &edge.arguments))
            .collect(),
        Inst::PointerCast { operand, .. } => vec![operand],
        Inst::BitOperation { operand, .. } => vec![operand],
        Inst::VaAccess(access) => access.operands(),
        Inst::Atomic(access) => access.operands(),
        Inst::Asm(assembly) => assembly.operands(),
        Inst::AsmGoto {
            assembly,
            labels,
            fallthrough,
        } => {
            let mut uses = assembly.operands();
            for label in labels {
                uses.extend(&label.edge.arguments);
            }
            uses.extend(&fallthrough.arguments);
            uses
        }
        Inst::Const(..)
        | Inst::Uninitialized(..)
        | Inst::Trap(..)
        | Inst::PtrNull(..)
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. }
        | Inst::BlockAddr { .. }
        | Inst::ReturnAddress { .. }
        | Inst::FrameAddress { .. } => vec![],

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
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            ..
        } => vec![destination, source, mem_in, world_in],
        Inst::Alloca {
            mem_in, world_in, ..
        } => vec![mem_in, world_in],
        Inst::DynamicAlloca {
            mem_in,
            world_in,
            size,
            ..
        } => vec![mem_in, world_in, size],
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
    /// Explicit native code placement; not part of its callable signature.
    pub section: Option<String>,
    /// Native entry reservation: total NOPs and those before the callable symbol.
    /// `Some((0, 0))` explicitly overrides a translation-unit default.
    pub patchable_entry: Option<(u16, u16)>,
    /// Source-selected strong stack protection; native guard policy is a target option.
    pub stack_protect: bool,
    /// Alignment of the first instruction; absent from text when it is four.
    pub alignment: u32,
    pub linkage: Linkage,
    pub name: String,
    pub params: Vec<FunctionParam>,
    /// The record marks of Section 10.1.3 (revision 1.9.73), one per
    /// parameter, in the canonical form [`ParamMark`] describes: empty when no
    /// parameter carries one.
    pub param_marks: Vec<ParamMark>,
    pub variadic: bool,
    pub ret_tys: Vec<EvoIRType>,
    pub effects: EffectSummary,
    /// Link visibility (revision 1.9.70).
    ///
    /// `Internal` is the linkage of a C definition whose storage class is
    /// `static`, including `static inline`; other functions are `External`
    /// unless a hidden-visibility attribute binds them to the linked image.
    /// The canonical text omits the default `External` field. The machine
    /// emits no global symbol for an internal function
    /// and drops one nothing in the module references; the interpreters are
    /// indifferent to it, since a module is executed whole.
    pub visibility: GlobalVisibility,
    pub blocks: Vec<Block>,
}

/// A collection of mutually callable EvoIR functions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Module {
    pub tls: Vec<TlsSymbol>,
    /// ELF registration order is semantic. Entries are roots even without calls.
    pub lifecycle: Vec<LifecycleRegistration>,
    pub aliases: Vec<SymbolAlias>,
    /// File-scope assembler text, retained in source order and not interpreted.
    pub assembly: Vec<String>,
    pub globals: Vec<GlobalObject>,
    pub extern_objects: Vec<ExternalObject>,
    pub declares: Vec<FunctionDeclaration>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecyclePhase {
    Constructor,
    Destructor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleRegistration {
    pub phase: LifecyclePhase,
    pub function: String,
    pub priority: u16,
}

mod lifecycle;
pub use lifecycle::interpret_program;
mod tls;
pub use tls::{TlsAddress, TlsModel, TlsSymbol};

/// Storage supplied by a linker, never allocated or initialized by this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalObject {
    pub visibility: GlobalVisibility,
    pub linkage: Linkage,
    pub symbol: String,
    pub size: Option<u64>,
    pub align: u32,
}

/// A callable interface without an implementation or a producer-supplied effect summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDeclaration {
    pub visibility: GlobalVisibility,
    pub linkage: Linkage,
    pub name: String,
    pub params: Vec<EvoIRType>,
    /// The record marks of Section 10.1.3, one per parameter, canonical as on
    /// [`Function`].
    pub param_marks: Vec<ParamMark>,
    pub variadic: bool,
    pub ret_tys: Vec<EvoIRType>,
}

impl FunctionDeclaration {
    fn signature(&self) -> Function {
        Function {
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            name: self.name.clone(),
            variadic: self.variadic,
            params: self
                .params
                .iter()
                .enumerate()
                .map(|(index, ty)| FunctionParam {
                    name: format!("arg{index}"),
                    ty: ty.clone(),
                })
                .collect(),
            ret_tys: self.ret_tys.clone(),
            effects: EffectSummary::top(),
            linkage: self.linkage,
            visibility: self.visibility,
            blocks: Vec::new(),
            param_marks: self.param_marks.clone(),
        }
    }
}

impl Module {
    /// Alias records are flattened and validated before consumers use them.
    pub fn symbol_target<'a>(&'a self, name: &'a str) -> &'a str {
        self.aliases
            .iter()
            .find(|alias| alias.name == name)
            .map_or(name, |alias| alias.target.as_str())
    }

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
    let mut positions = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.as_str(), index))
        .collect::<HashMap<_, _>>();
    for alias in &module.aliases {
        if let Some(index) = positions.get(alias.target.as_str()).copied() {
            positions.insert(alias.name.as_str(), index);
        }
    }
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
        Inst::Floating(_) => EffectSummary::default(),
        Inst::TlsAddr(_) => EffectSummary {
            reads_all: true,
            writes_all: true,
            may_trap: true,
            ..EffectSummary::default()
        },
        Inst::Vector(operation) => EffectSummary {
            may_trap: operation.op == VectorOp::Extract,
            ..EffectSummary::default()
        },
        Inst::Atomic(access) => EffectSummary {
            reads_all: !matches!(access.op, AtomicOp::Store(_)),
            writes_all: !access.is_load(),
            volatile: access.volatile,
            atomic: true,
            may_trap: true,
            ..EffectSummary::default()
        },
        Inst::IndirectBr { .. } => EffectSummary {
            may_trap: true,
            ..EffectSummary::default()
        },
        Inst::PointerCast { .. } => EffectSummary::default(),
        Inst::BitOperation { .. } => EffectSummary::default(),
        Inst::Asm(_) | Inst::AsmGoto { .. } => EffectSummary {
            reads_all: true,
            writes_all: true,
            volatile: true,
            io: true,
            may_trap: true,
            ..EffectSummary::default()
        },
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
        Inst::Alloca { .. } | Inst::DynamicAlloca { .. } | Inst::LifetimeEnd { .. } => {
            EffectSummary {
                reads_all: true,
                writes_all: true,
                may_trap: true,
                ..EffectSummary::default()
            }
        }
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
        Inst::Copy { volatile, .. } => EffectSummary {
            reads_all: true,
            writes_all: true,
            volatile: *volatile != CopyVolatility::None,
            may_trap: true,
            ..EffectSummary::default()
        },
        Inst::VaAccess(_) => EffectSummary {
            reads_all: true,
            writes_all: true,
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
        | Inst::BlockAddr { .. }
        | Inst::ReturnAddress { .. }
        | Inst::FrameAddress { .. }
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
    CompoundLiteral,
}

/// Compact encoding of C linkage and ELF visibility for a symbol.
///
/// `External` is the default because the canonical Section 9 text omits the
/// field for an external function (revision 1.9.70).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GlobalVisibility {
    #[default]
    External,
    /// C internal linkage (static), not GNU visibility("internal").
    Internal,
    /// External linkage without interposition from outside the linked image.
    Hidden,
    /// Externally exported, with references bound inside its final ELF component.
    Protected,
}

impl GlobalVisibility {
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::External => "",
            Self::Internal => " visibility=internal",
            Self::Hidden => " visibility=hidden",
            Self::Protected => " visibility=protected",
        }
    }

    pub const fn binds_locally(self) -> bool {
        !matches!(self, Self::External)
    }
}

/// Symbol strength is independent of visibility and of the value's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Linkage {
    #[default]
    Strong,
    Weak,
}

impl Linkage {
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Strong => "",
            Self::Weak => " linkage=weak",
        }
    }
}

/// A second symbol for one definition, with no allocation or forwarding body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolAlias {
    pub name: String,
    pub target: String,
    pub visibility: GlobalVisibility,
    pub linkage: Linkage,
}

/// One eagerly allocated module object and its byte initialization state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalObject {
    /// Stores are forbidden after initialization and relocation. Independent
    /// of source origin, linkage, and an explicitly selected native section.
    pub read_only: bool,
    /// Explicit native object placement, independent of storage and type identity.
    pub section: Option<String>,
    pub linkage: Linkage,
    pub relocs: Vec<Relocation>,
    pub symbol: String,
    pub origin: u64,
    pub kind: GlobalKind,
    pub visibility: GlobalVisibility,
    pub layout: AccessLayout,
    pub bytes: Vec<u8>,
    pub initialized: Vec<bool>,
}

impl GlobalObject {
    pub fn is_read_only(&self) -> bool {
        self.read_only || self.kind == GlobalKind::StringLiteral
    }

    pub fn has_bss_section(&self) -> bool {
        self.section
            .as_deref()
            .is_some_and(|name| name == ".bss" || name.starts_with(".bss."))
    }

    fn validate_section(&self) -> Result<(), String> {
        if self
            .section
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.chars().any(char::is_control))
        {
            return Err("invalid object section name".into());
        }
        if self.has_bss_section()
            && (!self.relocs.is_empty() || self.bytes.iter().any(|byte| *byte != 0))
        {
            return Err(
                "a NOBITS object section cannot contain initialized nonzero bytes or relocations"
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relocation {
    pub offset: u64,
    pub symbol: String,
    pub addend: i64,
    /// The static initializer converted this address to an integer.
    pub exposed: bool,
    /// A same-function label difference, stored as integer bytes rather than
    /// an address with provenance. Absolute relocations always occupy 8 bytes.
    pub difference: Option<BlockDifference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDifference {
    pub base: String,
    pub bits: u16,
}

impl Relocation {
    pub fn size_bytes(&self) -> u64 {
        self.difference
            .as_ref()
            .map_or(8, |difference| u64::from(difference.bits / 8))
    }

    pub fn symbols(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.symbol.as_str()).chain(
            self.difference
                .as_ref()
                .map(|difference| difference.base.as_str()),
        )
    }
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
    if func
        .section
        .as_ref()
        .is_some_and(|name| name.is_empty() || name.chars().any(char::is_control))
    {
        return Err(format!("function '{}' has invalid section name", func.name));
    }
    if func
        .patchable_entry
        .is_some_and(|(total, before)| before > total)
    {
        return Err(format!(
            "function '{}' has invalid patchable entry",
            func.name
        ));
    }
    if func.alignment < 4 || !func.alignment.is_power_of_two() {
        return Err(format!(
            "function '{}' has invalid alignment {}",
            func.name, func.alignment
        ));
    }
    if func.variadic {
        callable_signature(func)?;
    }
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
            if matches!(
                inst,
                Inst::VaAccess(VariadicAccess {
                    op: VariadicOp::Start,
                    ..
                })
            ) && !func.variadic
            {
                return Err("E3037: va-start-outside-variadic-signature".into());
            }
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
        if let Inst::AsmGoto {
            assembly, labels, ..
        } = &block.terminator
        {
            for used in assembly.operands() {
                ensure_value_type(&used.ty, "assembly input")?;
                require_local_value(&local, used, block.id, "assembly input")?;
            }
            assembly.validate(labels)?;
            effects.advance(&block.terminator, block.id)?;
            for result in assembly.results() {
                ensure_value_type(&result.ty, "assembly result")?;
                register_definition(
                    &mut definitions,
                    result,
                    format!("assembly terminator in block {}", block.id),
                )?;
                local.insert(result.id, result.ty.clone());
            }
        }
        for used in inst_uses(&block.terminator) {
            ensure_value_type(
                &used.ty,
                &format!("terminator use of v{} in block {}", used.id, block.id),
            )?;
            require_local_value(&local, used, block.id, "terminator")?;
        }

        match &block.terminator {
            Inst::IndirectBr { target, edges } => {
                if target.ty != EvoIRType::Ptr(AddressSpace::Generic) {
                    return Err("indirect branch target must be ptr<generic>".into());
                }
                if edges.is_empty()
                    || edges
                        .windows(2)
                        .any(|pair| pair[0].target >= pair[1].target)
                {
                    return Err("indirect successors must be nonempty, unique and sorted".into());
                }
                for edge in edges {
                    validate_edge(&blocks, block.id, edge.target, &edge.arguments)?;
                    validate_effect_edge(
                        &blocks,
                        block.id,
                        edge.target,
                        &edge.arguments,
                        &effects,
                    )?;
                }
            }
            Inst::AsmGoto {
                labels,
                fallthrough,
                ..
            } => {
                for edge in labels.iter().map(|label| &label.edge).chain([fallthrough]) {
                    validate_edge(&blocks, block.id, edge.target, &edge.arguments)?;
                    validate_effect_edge(
                        &blocks,
                        block.id,
                        edge.target,
                        &edge.arguments,
                        &effects,
                    )?;
                }
            }
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

    validate_indirect_destinations(func, &address_taken_blocks(func), false)?;
    validate_reachability(func, entry.id)?;

    Ok(())
}

fn address_taken_blocks(function: &Function) -> HashSet<BlockId> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter_map(|inst| {
            if let Inst::BlockAddr { block, .. } = inst {
                Some(*block)
            } else {
                None
            }
        })
        .collect()
}

fn validate_indirect_destinations(
    function: &Function,
    required: &HashSet<BlockId>,
    exact: bool,
) -> Result<(), String> {
    for block in &function.blocks {
        if let Inst::IndirectBr { edges, .. } = &block.terminator {
            let actual: HashSet<_> = edges.iter().map(|edge| edge.target).collect();
            if !required.is_subset(&actual) || (exact && &actual != required) {
                return Err(format!("indirect branch in block {} has an incomplete or foreign address-taken successor set", block.id));
            }
        }
    }
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
        Inst::Atomic(access) => {
            let layout = access.layout()?;
            if access_guards.get(&access.mem_in.id) != Some(&(access.address.id, layout)) {
                return Err(format!(
                    "E3062: invalid-atomic-access: natural alignment guard in block {block}"
                ));
            }
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
            if *pointer == address.id && expected.is_none_or(|expected|
                layout.size_bytes == expected.size_bytes
                && layout.align_bytes.is_power_of_two()
                && layout.align_bytes <= expected.align_bytes) =>
        {
            Ok(())
        }
        _ => Err(format!(
            "{operation} in block {block} must consume a matching access_in_bounds token for v{} with {expected:?}",
            address.id
        )),
    }
}

/// Target-profile scalar representation layout, also used for compiler-created storage.
pub fn evoir_access_layout(ty: &EvoIRType) -> Result<Option<AccessLayout>, String> {
    let size = match ty {
        EvoIRType::Vec(..) => {
            let (lanes, width) =
                integer_vector_shape(ty).ok_or("unsupported vector memory type")?;
            u64::from(lanes) * u64::from(width / 8)
        }
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
    // EvoIR 10.1.19: a `bits<128>` is accessed "at 16 bytes and alignment 16",
    // so the natural alignment is capped at the widest executed scalar rather
    // than at a register. Every narrower width is unchanged, its size being at
    // most eight bytes already.
    let align = u32::try_from(size.min(16)).map_err(|_| "invalid access alignment".to_owned())?;
    Ok(Some(AccessLayout::new(size, align)))
}

/// Validate every function and all inter-function call signatures.
/// Section 10.1.3 (revision 1.9.73): a `byval` or `sret` mark rides on a
/// `ptr<generic>`, a signature carries at most one `sret`, and the mark list is
/// canonical, so it is empty or exactly as long as the parameter list.
///
/// The size and alignment are a record type's; size may be zero, while the
/// alignment is a power of two. Fixed arguments execute the pointer form;
/// anonymous arguments also retain the mark as the abstract va_arg type.
pub fn validate_record_marks(params: &[EvoIRType], marks: &[ParamMark]) -> Result<(), String> {
    if marks.is_empty() {
        return Ok(());
    }
    if marks.len() != params.len() {
        return Err("a record mark list must be as long as the parameter list".to_owned());
    }
    if marks.iter().all(|mark| matches!(mark, ParamMark::None)) {
        return Err("a record mark list with no mark is spelled by its absence".to_owned());
    }
    let mut destinations = 0_usize;
    for (index, mark) in marks.iter().enumerate() {
        let Some(shape) = mark.shape() else {
            continue;
        };
        if params[index] != EvoIRType::Ptr(AddressSpace::Generic) {
            return Err(format!(
                "parameter {index} carries a record mark but is not ptr<generic>"
            ));
        }
        if !shape.valid() {
            return Err(format!(
                "parameter {index} marks a record whose alignment {} is not a power of two",
                shape.align
            ));
        }
        if mark.is_sret() {
            destinations += 1;
        }
    }
    if destinations > 1 {
        return Err("a signature carries at most one sret parameter".to_owned());
    }
    Ok(())
}

pub fn validate_module(module: &Module) -> Result<(), String> {
    if module
        .extern_objects
        .iter()
        .any(|object| object.visibility == GlobalVisibility::Internal)
        || module
            .declares
            .iter()
            .any(|function| function.visibility == GlobalVisibility::Internal)
    {
        return Err("an external declaration cannot have internal linkage".into());
    }
    let mut functions = HashMap::new();
    let mut symbols = HashSet::new();
    for name in module
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .chain(module.declares.iter().map(|f| f.name.as_str()))
        .chain(module.globals.iter().map(|g| g.symbol.as_str()))
        .chain(module.extern_objects.iter().map(|g| g.symbol.as_str()))
        .chain(module.aliases.iter().map(|alias| alias.name.as_str()))
    {
        if !symbols.insert(name) {
            return Err(format!("E3041: symbol-declared-and-defined: '{name}'"));
        }
    }
    for (name, visibility, linkage) in module
        .functions
        .iter()
        .map(|f| (&f.name, f.visibility, f.linkage))
        .chain(
            module
                .globals
                .iter()
                .map(|g| (&g.symbol, g.visibility, g.linkage)),
        )
        .chain(
            module
                .aliases
                .iter()
                .map(|a| (&a.name, a.visibility, a.linkage)),
        )
    {
        if visibility == GlobalVisibility::Internal && linkage == Linkage::Weak {
            return Err(format!("E3060: weak-internal-symbol: '{name}'"));
        }
    }
    let definitions = module
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .chain(module.globals.iter().map(|g| g.symbol.as_str()))
        .collect::<HashSet<_>>();
    for alias in &module.aliases {
        if !definitions.contains(alias.target.as_str()) {
            return Err(format!(
                "E3061: alias-target-not-defined: '{}'",
                alias.target
            ));
        }
    }
    for function in &module.functions {
        if functions.insert(function.name.as_str(), function).is_some() {
            return Err(format!("duplicate function symbol '{}'", function.name));
        }
        validate_record_marks(
            &function
                .params
                .iter()
                .map(|parameter| parameter.ty.clone())
                .collect::<Vec<_>>(),
            &function.param_marks,
        )
        .map_err(|message| format!("function '{}': {message}", function.name))?;
        validate_ssa(function)
            .map_err(|message| format!("function '{}': {message}", function.name))?;
        let mut taken = address_taken_blocks(function);
        for symbol in module
            .globals
            .iter()
            .flat_map(|global| &global.relocs)
            .flat_map(Relocation::symbols)
        {
            if let Some((owner, label)) = symbol.split_once("^b") {
                if owner == function.name {
                    if let Ok(block) = label.parse::<BlockId>() {
                        taken.insert(block);
                    }
                }
            }
        }
        validate_indirect_destinations(function, &taken, true)?;
    }
    for declaration in &module.declares {
        validate_record_marks(&declaration.params, &declaration.param_marks)
            .map_err(|message| format!("declare '{}': {message}", declaration.name))?;
    }
    let mut registrations = HashSet::new();
    for r in &module.lifecycle {
        let function = functions.get(r.function.as_str()).ok_or_else(|| {
            format!(
                "lifecycle registration names undefined function '{}'",
                r.function
            )
        })?;
        let effects = [EvoIRType::Mem(Region::All), EvoIRType::World];
        if r.priority < 101
            || function.variadic
            || function.params.iter().map(|p| &p.ty).ne(effects.iter())
            || function.ret_tys != effects
            || !registrations.insert((r.function.as_str(), r.phase == LifecyclePhase::Constructor))
        {
            return Err(format!(
                "invalid lifecycle registration for '{}'",
                r.function
            ));
        }
    }
    let declarations = module
        .declares
        .iter()
        .map(FunctionDeclaration::signature)
        .collect::<Vec<_>>();
    for declaration in &declarations {
        for ty in declaration
            .params
            .iter()
            .map(|parameter| &parameter.ty)
            .chain(&declaration.ret_tys)
        {
            validate_value_type(ty)?;
        }
        callable_signature(declaration)?;
        functions.insert(declaration.name.as_str(), declaration);
    }
    for alias in &module.aliases {
        if let Some(function) = functions.get(alias.target.as_str()).copied() {
            functions.insert(alias.name.as_str(), function);
        }
    }
    let mut externs = HashSet::new();
    for object in &module.extern_objects {
        if !object.align.is_power_of_two() {
            return Err(format!(
                "external object '{}' has invalid alignment",
                object.symbol
            ));
        }
        externs.insert(object.symbol.as_str());
    }

    let mut globals = HashMap::new();
    let mut tls_symbols = HashSet::new();
    let declared_tls: HashSet<_> = module.tls.iter().map(|t| t.symbol.as_str()).collect();
    for symbol in &module.tls {
        if !tls_symbols.insert(symbol.symbol.as_str())
            || !(module.globals.iter().any(|g| {
                g.symbol == symbol.symbol
                    && g.kind != GlobalKind::StringLiteral
                    && g.initialized.iter().all(|b| *b)
            }) || externs.contains(symbol.symbol.as_str())
                || module
                    .aliases
                    .iter()
                    .any(|a| a.name == symbol.symbol && declared_tls.contains(a.target.as_str())))
        {
            return Err(format!("invalid TLS symbol '{}'", symbol.symbol));
        }
    }
    let mut origins = HashSet::new();
    for global in &module.globals {
        global.validate_section()?;
        validate_layout(global.layout, "module global", 0)?;
        let size = usize::try_from(global.layout.size_bytes)
            .map_err(|_| format!("global '{}' size exceeds host limits", global.symbol))?;
        if global.bytes.len() != size || global.initialized.len() != size {
            return Err(format!(
                "global '{}' bytes/initialized lengths must equal size {}",
                global.symbol, global.layout.size_bytes
            ));
        }
        let mut previous_end = 0;
        for relocation in &global.relocs {
            if tls_symbols.contains(relocation.symbol.as_str()) {
                return Err("a TLS address is not a static relocation".into());
            }
            let code_target = relocation
                .symbol
                .split_once("^b")
                .and_then(|(name, block)| {
                    let id = block.parse::<BlockId>().ok()?;
                    (id.to_string() == block).then_some((name, id))
                });
            let valid_code_target = code_target.is_some_and(|(name, id)| {
                module.functions.iter().any(|function| {
                    function.name == name && function.blocks.iter().any(|block| block.id == id)
                })
            });
            if let Some(difference) = &relocation.difference {
                let valid_base = difference
                    .base
                    .split_once("^b")
                    .is_some_and(|(owner, label)| {
                        code_target.is_some_and(|(function, _)| function == owner)
                            && module.functions.iter().any(|function| {
                                function.name == owner
                                    && function
                                        .blocks
                                        .iter()
                                        .any(|block| block.id.to_string() == label)
                            })
                    });
                if !valid_code_target
                    || !valid_base
                    || relocation.exposed
                    || !matches!(difference.bits, 8 | 16 | 32 | 64)
                {
                    return Err("malformed relative block relocation".into());
                }
            }
            let end = relocation
                .offset
                .checked_add(relocation.size_bytes())
                .filter(|end| *end <= global.layout.size_bytes)
                .ok_or_else(|| {
                    format!(
                        "global '{}' relocation is outside its object",
                        global.symbol
                    )
                })?;
            if relocation.offset < previous_end
                || !(symbols.contains(relocation.symbol.as_str()) || valid_code_target)
                || (functions.contains_key(relocation.symbol.as_str()) && relocation.addend != 0)
                || global.bytes[relocation.offset as usize..end as usize]
                    .iter()
                    .any(|b| *b != 0)
                || global.initialized[relocation.offset as usize..end as usize]
                    .iter()
                    .any(|b| !*b)
            {
                return Err(format!(
                    "global '{}' has a malformed relocation",
                    global.symbol
                ));
            }
            previous_end = end;
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
            GlobalKind::CompoundLiteral => {
                let expected = format!("__evocc.compound.{}", global.origin);
                if global.symbol != expected || global.visibility != GlobalVisibility::Internal {
                    return Err(format!(
                        "compound-literal global '{}' must be internal symbol '{expected}'",
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

    for alias in &module.aliases {
        if let Some(global) = globals.get(alias.target.as_str()).copied() {
            globals.insert(alias.name.as_str(), global);
        }
    }
    let mut effect_participants = HashSet::new();
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.insts {
                match instruction {
                    Inst::Call { callee, .. } => {
                        effect_participants.insert(function.name.as_str());
                        effect_participants.insert(module.symbol_target(callee));
                    }
                    Inst::IndirectCall { .. } => {
                        effect_participants.insert(function.name.as_str());
                    }
                    Inst::FuncAddr { symbol, .. } => {
                        effect_participants.insert(module.symbol_target(symbol));
                    }
                    Inst::Load { .. }
                    | Inst::VaAccess(_)
                    | Inst::Atomic(_)
                    | Inst::TlsAddr(_)
                    | Inst::Store { .. }
                    | Inst::Copy { .. }
                    | Inst::Alloca { .. }
                    | Inst::DynamicAlloca { .. }
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
                    Inst::TlsAddr(a) if !tls_symbols.contains(a.symbol.as_str()) => {
                        return Err(format!("tls_addr names non-TLS object '{}'", a.symbol));
                    }
                    Inst::GlobalAddr { symbol, .. } if tls_symbols.contains(symbol.as_str()) => {
                        return Err(format!("global_addr cannot name TLS object '{symbol}'"));
                    }
                    Inst::GlobalAddr { symbol, .. }
                        if !globals.contains_key(symbol.as_str())
                            && !externs.contains(symbol.as_str()) =>
                    {
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
                    Inst::BlockAddr { block: target, .. }
                        if !caller
                            .blocks
                            .iter()
                            .any(|candidate| candidate.id == *target) =>
                    {
                        return Err(format!(
                            "function '{}' block {} instruction {} takes the address of absent block {}",
                            caller.name, block.id, index, target
                        ));
                    }
                    Inst::Alloca { origin, .. } | Inst::DynamicAlloca { origin, .. }
                        if !origins.insert(*origin) =>
                    {
                        return Err(format!("duplicate allocation origin {origin}"));
                    }
                    _ => {}
                }
                let Inst::Call {
                    results,
                    callee,
                    args,
                    arg_marks,
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
                variadic::validate_anonymous_marks(
                    args,
                    arg_marks,
                    target.params.len().saturating_sub(2),
                )?;
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
        variadic: function.variadic,
        params: function
            .params
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect(),
        results: function.ret_tys.clone(),
        // Revision 1.9.73: "the attribute is part of the signature for
        // equality", so the pointer a function's address produces carries the
        // function's marks and compares unequal to an unmarked signature.
        marks: function.param_marks.clone(),
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
    if args.len() < signature.ordinary_params.len()
        || (!callee.variadic && args.len() != signature.ordinary_params.len())
    {
        return Err(format!(
            "call from '{}' to '{}' has {} ordinary arguments, expected {}",
            caller.name,
            callee.name,
            args.len(),
            signature.ordinary_params.len()
        ));
    }
    for argument in &args[signature.ordinary_params.len()..] {
        if !variadic::promoted_type(&argument.ty) {
            return Err("anonymous argument is not a promoted scalar type".into());
        }
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
        Inst::Br(..)
            | Inst::CondBr(..)
            | Inst::IndirectBr { .. }
            | Inst::Ret(..)
            | Inst::Trap(..)
            | Inst::AsmGoto { .. }
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
            Inst::IndirectBr { edges, .. } => pending.extend(edges.iter().map(|edge| edge.target)),
            Inst::AsmGoto {
                labels,
                fallthrough,
                ..
            } => {
                pending.extend(labels.iter().map(|label| label.edge.target));
                pending.push(fallthrough.target);
            }
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
            Inst::TlsAddr(a) => Some((&a.mem_in, &a.world_in, &a.mem_out, &a.world_out)),
            Inst::Atomic(access) => Some((
                &access.mem_in,
                &access.world_in,
                &access.mem_out,
                &access.world_out,
            )),
            Inst::VaAccess(access) => Some((
                &access.mem_in,
                &access.world_in,
                &access.mem_out,
                &access.world_out,
            )),
            Inst::Asm(assembly) | Inst::AsmGoto { assembly, .. } => Some((
                &assembly.mem_in,
                &assembly.world_in,
                &assembly.mem_out,
                &assembly.world_out,
            )),
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
            | Inst::Copy {
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
            | Inst::DynamicAlloca {
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
        Inst::BitOperation {
            result,
            operand,
            op,
        } => {
            expect_bits(&result.ty, "bit operation result", block)?;
            if result.ty != operand.ty
                || (*op == BitOp::ByteSwap && !matches!(result.ty, EvoIRType::Bits(16 | 32 | 64)))
            {
                return Err(
                    "bit operation operand/result type mismatch or invalid byte-swap width".into(),
                );
            }
            Ok(())
        }
        Inst::Vector(operation) => operation.validate(),
        Inst::Floating(operation) => operation.validate(),
        Inst::VaAccess(access) => access.validate(),
        Inst::Atomic(access) => access.validate(),
        Inst::TlsAddr(access) => access.validate(),
        Inst::Asm(assembly) => assembly.validate(&[]),
        Inst::AsmGoto {
            assembly, labels, ..
        } => assembly.validate(labels),
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
        Inst::PointerCast { result, operand } => {
            let valid = (result.ty == EvoIRType::Bits(64)
                && matches!(operand.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }))
                || (operand.ty == EvoIRType::Bits(64)
                    && matches!(result.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }));
            if valid {
                Ok(())
            } else {
                Err(format!(
                    "invalid pointer-integer conversion in block {block}"
                ))
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
        // Section 3.7: "`%p = block_addr ^label : ptr<generic>`". The one
        // address space is the form's, not a choice the writer has.
        Inst::BlockAddr { result, .. } => match result.ty {
            EvoIRType::Ptr(evocc_types::AddressSpace::Generic) => Ok(()),
            _ => Err(format!(
                "block-address result in block {block} has type {:?}, expected ptr<generic>",
                result.ty
            )),
        },
        // Section 3.7 (revision 1.9.72): "`%p = return_address : ptr<generic>`"
        // and "`%p = frame_address : ptr<generic>`". Neither takes an operand,
        // and the one address space is the form's.
        Inst::ReturnAddress { result } => match result.ty {
            EvoIRType::Ptr(evocc_types::AddressSpace::Generic) => Ok(()),
            _ => Err(format!(
                "return-address result in block {block} has type {:?}, expected ptr<generic>",
                result.ty
            )),
        },
        Inst::FrameAddress { result, .. } => match result.ty {
            EvoIRType::Ptr(evocc_types::AddressSpace::Generic) => Ok(()),
            _ => Err(format!(
                "frame-address result in block {block} has type {:?}, expected ptr<generic>",
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
        Inst::DynamicAlloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            size,
            align,
            ..
        } => {
            expect_pointer(&result.ty, "alloca.dynamic result", block)?;
            if size.ty != EvoIRType::Bits(64) {
                return Err(format!(
                    "alloca.dynamic size in block {block} must be bits<64>"
                ));
            }
            validate_layout(AccessLayout::new(0, *align), "alloca.dynamic", block)?;
            expect_day7_effect_tokens(
                mem_in,
                world_in,
                mem_out,
                world_out,
                "alloca.dynamic",
                block,
            )
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
            if layout.size_bytes == 0 {
                return Err(format!(
                    "access_in_bounds in block {block} has invalid size/alignment {layout:?}"
                ));
            }
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
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            layout,
            volatile,
        } => {
            expect_pointer(&destination.ty, "copy destination", block)?;
            expect_pointer(&source.ty, "copy source", block)?;
            if layout.align_bytes == 0 || !layout.align_bytes.is_power_of_two() {
                return Err(format!(
                    "copy in block {block} has invalid alignment {}",
                    layout.align_bytes
                ));
            }
            if !volatile.supports(*layout) {
                return Err(format!("volatile copy in block {block} requires one naturally aligned 1/2/4/8-byte access"));
            }
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "copy", block)
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
            arg_marks,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            let EvoIRType::FnPtr {
                params,
                results: returns,
                variadic,
                // The marks ride inside the type, so the equality below already
                // compares them; nothing else here reads one.
                marks: _,
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
            let fixed = params.len().saturating_sub(2);
            variadic::validate_anonymous_marks(args, arg_marks, fixed)?;
            let parameters_match = if *variadic {
                args.len() >= fixed
                    && actual_params[..fixed] == params[..fixed]
                    && actual_params[actual_params.len() - 2..] == params[fixed..]
            } else {
                actual_params == *params
            };
            if !parameters_match || actual_returns != *returns {
                return Err(format!(
                    "E3017: indirect-call-signature-mismatch in block {block}"
                ));
            }
            if *variadic
                && args[fixed..]
                    .iter()
                    .any(|argument| !variadic::promoted_type(&argument.ty))
            {
                return Err("anonymous argument is not a promoted scalar type".into());
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
        Inst::Br(..)
        | Inst::CondBr(..)
        | Inst::IndirectBr { .. }
        | Inst::Ret(..)
        | Inst::Trap(..) => Err(format!(
            "terminator found in instruction list of block {}",
            block
        )),
    }
}

fn validate_layout(layout: AccessLayout, operation: &str, block: BlockId) -> Result<(), String> {
    if layout.align_bytes == 0 || !layout.align_bytes.is_power_of_two() {
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
            EvoIRType::FnPtr {
                params, results, ..
            } => {
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
    /// The bits above 64 of a `bits<128>` value, zero for every narrower type.
    ///
    /// EvoIR 10.1.19 (revision 1.9.61) makes `bits<128>` an executed scalar
    /// type, so the boundary carries 128 bits. It carries them as two halves
    /// rather than one `u128` so that every reader of `raw_bits` keeps
    /// reading exactly the low 64 bits it always read.
    pub high_bits: u64,
}

impl RuntimeValue {
    #[must_use]
    pub const fn new(ty: EvoIRType, raw_bits: u64) -> Self {
        Self {
            ty,
            raw_bits,
            high_bits: 0,
        }
    }

    #[must_use]
    pub const fn bits(width: u16, raw_bits: u64) -> Self {
        Self::new(EvoIRType::Bits(width), raw_bits)
    }

    /// One value of any executed width, split into its two halves.
    #[must_use]
    pub const fn wide(ty: EvoIRType, value: u128) -> Self {
        Self {
            ty,
            raw_bits: value as u64,
            high_bits: (value >> 64) as u64,
        }
    }

    /// The whole bit pattern this value carries.
    #[must_use]
    pub const fn value(&self) -> u128 {
        ((self.high_bits as u128) << 64) | self.raw_bits as u128
    }
}

/// Private runtime state for an SSA value. Public entry arguments and
/// successful results are always initialized; poison exists only while an
/// interpreter is executing validated EvoIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalValue {
    Vector(usize),
    Bits(u128),
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
    tls_templates: HashMap<String, GlobalObject>,
    vectors: Vec<evocc_memory::ObjectRepresentation>,
    argument_lists: evocc_memory::ArgumentLists<(EvoIRType, ParamMark, InternalValue)>,
    memory: MemoryState,
    world: WorldState,
    globals: HashMap<String, Pointer>,
    functions: HashMap<String, u64>,
    /// The code provenance of every block a `block_addr` can name, numbered
    /// past the function identities so the two never collide.
    blocks: HashMap<(String, BlockId), u64>,
    access_guards: HashMap<u128, (Pointer, AccessLayout)>,
    next_token: u128,
    /// The next unused activation-address identity, numbered past the function
    /// and block identities so no activation address ever equals one of those.
    next_activation: u64,
}

/// The two addresses Section 3.7 gives one activation, minted when it begins.
///
/// "The interpreters give each activation a fresh value for each": the identity
/// is the memory model's opaque non-object provenance, which compares equal to
/// itself, exposes to a distinct address under `ptr.to_int`, and denies every
/// load and store (`E3013`). One activation holds both for its whole life, so
/// two readings inside it agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActivationAddresses {
    return_address: u64,
    frame_address: u64,
}

impl ExecutionState {
    fn for_target(
        target: &TargetDataLayout,
        function_alignment: u32,
    ) -> Result<Self, InterpreterError> {
        let config = MemoryConfig::for_target(
            target,
            MemoryLimits {
                max_objects: MAX_MEMORY_OBJECTS,
                max_object_bytes: MAX_MEMORY_OBJECT_BYTES,
                max_total_bytes: MAX_MEMORY_TOTAL_BYTES,
            },
        )
        .and_then(|config| config.with_function_alignment(function_alignment))
        .map_err(|error| InterpreterError::MemorySetup { error })?;
        let memory =
            MemoryState::new(config).map_err(|error| InterpreterError::MemorySetup { error })?;
        let world = WorldState::new(WorldLimits {
            max_volatile_events: MAX_VOLATILE_EVENTS,
        });
        Ok(Self {
            tls_templates: HashMap::new(),
            vectors: Vec::new(),
            argument_lists: evocc_memory::ArgumentLists::default(),
            memory,
            world,
            globals: HashMap::new(),
            functions: HashMap::new(),
            blocks: HashMap::new(),
            access_guards: HashMap::new(),
            next_token: 1,
            next_activation: 1,
        })
    }

    fn fresh_token(&mut self) -> Result<u128, InterpreterError> {
        let token = self.next_token;
        self.next_token = token
            .checked_add(1)
            .ok_or_else(|| InterpreterError::InvalidModule {
                message: "runtime effect-token identity exhausted".to_owned(),
            })?;
        Ok(token)
    }

    /// Mint the two addresses of one beginning activation (Section 3.7).
    fn begin_activation(&mut self) -> Result<ActivationAddresses, InterpreterError> {
        let return_address = self.next_activation;
        let frame_address =
            return_address
                .checked_add(1)
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "activation address identity exhausted".to_owned(),
                })?;
        self.next_activation =
            frame_address
                .checked_add(1)
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "activation address identity exhausted".to_owned(),
                })?;
        Ok(ActivationAddresses {
            return_address,
            frame_address,
        })
    }
}

/// A defined failure from EvoIR validation or execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpreterError {
    InvalidIndirectBranchTarget {
        block: BlockId,
    },
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
    /// The two types are boxed so that the error stays small: an `EvoIRType`
    /// carries a whole signature, including its record marks (revision
    /// 1.9.73), and a `Result` pays for the largest variant at every call.
    EntryArgumentType {
        index: usize,
        expected: Box<EvoIRType>,
        actual: Box<EvoIRType>,
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
    InvalidPointerRepresentation {
        block: BlockId,
    },
    MissingReturn {
        block: BlockId,
    },
    BuiltinTrapReached {
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
    FrameAddressUnavailable {
        block: BlockId,
        depth: u16,
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
            Self::InvalidIndirectBranchTarget { .. } => "E3048",
            Self::MemorySetup { .. } => "E3007",
            Self::MemoryAccess { error, .. } => match error.runtime_diagnostic() {
                Some(diagnostic) => diagnostic.code,
                None => "E3007",
            },
            Self::UninitializedRead { .. } => "E3001",
            Self::InvalidPointerRepresentation { .. } => "E3013",
            Self::MissingReturn { .. } => "E3007",
            Self::BuiltinTrapReached { .. } => TrapReason::BuiltinTrap.diagnostic(),
            Self::DivisionByZero { .. } => "E3002",
            Self::RemainderByZero { .. } => "E3003",
            Self::SignedDivisionOverflow { .. } | Self::SignedRemainderOverflow { .. } => "E3004",
            Self::UndefinedBehavior { diagnostic, .. } => diagnostic,
            Self::StepBudgetExceeded { .. } => "E3008",
            Self::CallThroughNullFunctionPointer { .. } => "E3015",
            Self::CallThroughNonFunctionPointer { .. } => "E3016",
            Self::IndirectCallDynamicSignatureMismatch { .. } => "E3019",
            Self::UnsupportedBitWidth { .. } => "E3033",
            Self::FrameAddressUnavailable { .. } => "E3031",
            _ => "E3007",
        }
    }

    /// Stable diagnostic message aligned with the C-HIR scalar interpreter.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::InvalidIndirectBranchTarget { .. } => "invalid-indirect-branch-target".into(),
            Self::MemorySetup { error } => {
                format!("interpreter-memory-setup-failed: {error}")
            }
            Self::MemoryAccess { error, .. } => error.runtime_diagnostic().map_or_else(
                || format!("interpreter-memory-failed: {error}"),
                |diagnostic| diagnostic.message.to_owned(),
            ),
            Self::UninitializedRead { .. } => "read-of-uninitialized-object".to_owned(),
            Self::InvalidPointerRepresentation { .. } => {
                "invalid-pointer-representation".to_owned()
            }
            Self::MissingReturn { .. } => "scalar-function-fell-off-end".to_owned(),
            Self::BuiltinTrapReached { .. } => TrapReason::BuiltinTrap.message().to_owned(),
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
            Self::UnsupportedBitWidth { .. } => "not-executed: wide-integer".to_owned(),
            Self::FrameAddressUnavailable { .. } => "not-executed: frame-address".to_owned(),
            _ => self.to_string(),
        }
    }
}

impl fmt::Display for InterpreterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIndirectBranchTarget { block } => {
                write!(formatter, "invalid indirect branch target in block {block}")
            }
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
            Self::InvalidPointerRepresentation { block } => write!(
                formatter,
                "E3013: invalid-pointer-representation (pointer poison in block {block})"
            ),
            Self::MissingReturn { block } => write!(
                formatter,
                "E3007: scalar-function-fell-off-end (trap in block {block})"
            ),
            Self::BuiltinTrapReached { block } => write!(
                formatter,
                "{}: {} (trap in block {block})",
                TrapReason::BuiltinTrap.diagnostic(),
                TrapReason::BuiltinTrap.message()
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
            Self::FrameAddressUnavailable { block, depth } => write!(
                formatter,
                "not-executed: frame-address (depth {depth} in block {block})"
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

fn assembly_error(block: BlockId) -> InterpreterError {
    InterpreterError::UndefinedBehavior {
        block,
        diagnostic: "E3021",
        message: "not-lowered: asm",
    }
}

fn trap_error(reason: TrapReason, block: BlockId) -> InterpreterError {
    match reason {
        TrapReason::ScalarFunctionFellOffEnd => InterpreterError::MissingReturn { block },
        TrapReason::BuiltinTrap => InterpreterError::BuiltinTrapReached { block },
        TrapReason::UnreachableReached => InterpreterError::UndefinedBehavior {
            block,
            diagnostic: reason.diagnostic(),
            message: reason.message(),
        },
    }
}

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
    let mut state = ExecutionState::for_target(target, func.alignment)?;
    state.functions.insert(func.name.clone(), 1);
    for (index, block) in func.blocks.iter().enumerate() {
        state
            .blocks
            .insert((func.name.clone(), block.id), index as u64 + 2);
    }
    state.next_activation = func.blocks.len() as u64 + 2;
    interpret_function_in_state(func, entry_args, limits, state)
}

/// One provenance identity per block, numbered after the function identities.
///
/// Section 3.7 asks only that two `block_addr` of the same block be equal and
/// of different blocks unequal; a dense numbering in declaration order is the
/// deterministic way to say that, and starting past the function identities
/// keeps a block address from ever equalling a function address.
fn block_identities(
    module: &Module,
    functions: usize,
) -> Result<HashMap<(String, BlockId), u64>, InterpreterError> {
    let mut identities = HashMap::new();
    let mut next = u64::try_from(functions)
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "block provenance identity exhausted".to_owned(),
        })?;
    for function in &module.functions {
        for block in &function.blocks {
            identities.insert((function.name.clone(), block.id), next);
            next = next
                .checked_add(1)
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "block provenance identity exhausted".to_owned(),
                })?;
        }
    }
    Ok(identities)
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
    let activation = state.begin_activation()?;

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
                &func.name,
                block.id,
                &mut values,
                &mut state,
                memory_frame,
                memory_scope,
                activation,
            )?;
        }

        charge_step(&mut steps, limits, block.id)?;
        match &block.terminator {
            Inst::IndirectBr { target, edges } => {
                let edge =
                    select_indirect_edge(&state, &func.name, block.id, target, edges, &values)?;
                incoming = copy_values(&edge.arguments, block.id, &values)?;
                program_counter = edge.target;
            }
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
            Inst::AsmGoto { .. } => return Err(assembly_error(block.id)),
            Inst::Trap(reason) => return Err(trap_error(*reason, block.id)),
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
    anonymous: Vec<(EvoIRType, ParamMark, InternalValue)>,
    function_index: usize,
    block: BlockId,
    incoming: Vec<InternalValue>,
    values: HashMap<u32, InternalValue>,
    next_instruction: usize,
    entered: bool,
    return_binding: Option<CallReturnBinding>,
    memory_frame: Option<FrameId>,
    memory_scope: Option<ScopeId>,
    /// Section 3.7: one activation, one pair of addresses, for its whole life.
    activation: ActivationAddresses,
}

impl ModuleFrame {
    #[allow(clippy::too_many_arguments)]
    fn new(
        function_index: usize,
        block: BlockId,
        incoming: Vec<InternalValue>,
        anonymous: Vec<(EvoIRType, ParamMark, InternalValue)>,
        return_binding: Option<CallReturnBinding>,
        memory_frame: Option<FrameId>,
        memory_scope: Option<ScopeId>,
        activation: ActivationAddresses,
    ) -> Self {
        Self {
            anonymous,
            function_index,
            block,
            incoming,
            values: HashMap::new(),
            next_instruction: 0,
            entered: false,
            return_binding,
            memory_frame,
            memory_scope,
            activation,
        }
    }
}

fn function_uses_automatic_memory(function: &Function) -> bool {
    function.blocks.iter().any(|block| {
        block.insts.iter().any(|instruction| {
            matches!(
                instruction,
                Inst::Alloca { .. } | Inst::DynamicAlloca { .. } | Inst::LifetimeEnd { .. }
            )
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
    let alignment = module
        .functions
        .iter()
        .map(|function| function.alignment)
        .max()
        .unwrap_or(4);
    let state = ExecutionState::for_target(target, alignment)?;
    interpret_module_in_state(module, entry_name, entry_args, limits, state)
}

fn interpret_module_in_state(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
    state: ExecutionState,
) -> Result<InterpreterOutcome, InterpreterError> {
    interpret_module_execution(module, entry_name, entry_args, limits, state, false)
}

fn interpret_module_execution(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
    mut state: ExecutionState,
    program: bool,
) -> Result<InterpreterOutcome, InterpreterError> {
    validate_module(module).map_err(|message| InterpreterError::InvalidModule { message })?;
    let mut function_indices: HashMap<&str, usize> = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.as_str(), index))
        .collect();
    for alias in &module.aliases {
        if let Some(index) = function_indices.get(alias.target.as_str()).copied() {
            function_indices.insert(alias.name.as_str(), index);
        }
    }
    state.functions = module
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .chain(
            module
                .declares
                .iter()
                .map(|function| function.name.as_str()),
        )
        .enumerate()
        .map(|(index, name)| {
            let identity = u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "function provenance identity exhausted".to_owned(),
                })?;
            Ok((name.to_owned(), identity))
        })
        .collect::<Result<_, _>>()?;
    state.blocks = block_identities(module, state.functions.len())?;
    // Section 3.7: an activation address is fresh, and a fresh identity is one
    // no function and no block already holds.
    state.next_activation = u64::try_from(state.functions.len() + state.blocks.len())
        .ok()
        .and_then(|used| used.checked_add(1))
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "activation address identity exhausted".to_owned(),
        })?;
    for alias in &module.aliases {
        if let Some(identity) = state.functions.get(&alias.target).copied() {
            state.functions.insert(alias.name.clone(), identity);
        }
    }
    initialize_globals(module, &mut state)?;
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
    let mut roots = lifecycle::program_entries(module, entry_index, program);
    let mut root_kind = lifecycle::EntryKind::Main;
    let mut program_result = Vec::new();
    let mut frames = Vec::new();
    let mut steps = 0;

    loop {
        if frames.is_empty() {
            let Some((index, kind)) = roots.pop_front() else {
                return Ok(InterpreterOutcome {
                    return_values: program_result,
                    steps,
                    memory: state.memory,
                    world: state.world,
                });
            };
            root_kind = kind;
            let function = &module.functions[index];
            let block = function.blocks[0].id;
            let values = if kind == lifecycle::EntryKind::Main {
                incoming.clone()
            } else {
                vec![InternalValue::Bits(0), InternalValue::Bits(0)]
            };
            let (frame, scope) = begin_function_memory(&mut state, function, block)?;
            let activation = state.begin_activation()?;
            frames.push(ModuleFrame::new(
                index,
                block,
                values,
                Vec::new(),
                None,
                frame,
                scope,
                activation,
            ));
        }
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
                    arg_marks,
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
                    if module
                        .declares
                        .iter()
                        .any(|declaration| declaration.name == callee)
                    {
                        if program
                            && matches!(callee.as_str(), "exit" | "_Exit" | "_exit" | "quick_exit")
                        {
                            let status =
                                args.first()
                                    .ok_or_else(|| InterpreterError::InvalidModule {
                                        message: "termination needs an exit status".into(),
                                    })?;
                            let code = read_value(&frames[frame_index].values, status, block_id)?;
                            program_result = entry_function
                                .ret_tys
                                .iter()
                                .map(|ty| {
                                    RuntimeValue::wide(
                                        ty.clone(),
                                        if matches!(ty, EvoIRType::Bits(_)) {
                                            code
                                        } else {
                                            0
                                        },
                                    )
                                })
                                .collect();
                            roots.retain(|(_, kind)| {
                                callee == "exit" && *kind == lifecycle::EntryKind::Finalizer
                            });
                            for frame in frames.drain(..).rev() {
                                if let Some(frame) = frame.memory_frame {
                                    state.memory =
                                        state.memory.end_frame(frame).map_err(|error| {
                                            InterpreterError::MemoryAccess {
                                                block: block_id,
                                                error,
                                            }
                                        })?;
                                }
                            }
                            continue;
                        }
                        let returned = execute_external_call(
                            &mut state,
                            &callee,
                            call_arguments,
                            &results,
                            block_id,
                        )?;
                        bind_call_return(
                            &mut frames,
                            CallReturnBinding {
                                results,
                                mem_out,
                                world_out,
                            },
                            returned,
                        )?;
                        continue;
                    }
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
                    let anonymous = separate_anonymous_arguments(
                        &module.functions[callee_index],
                        &args,
                        &arg_marks,
                        &mut call_arguments,
                    );
                    let callee_activation = state.begin_activation()?;
                    frames.push(ModuleFrame::new(
                        callee_index,
                        callee_entry.id,
                        call_arguments,
                        anonymous,
                        Some(CallReturnBinding {
                            results,
                            mem_out,
                            world_out,
                        }),
                        memory_frame,
                        memory_scope,
                        callee_activation,
                    ));
                } else if let Inst::IndirectCall {
                    results,
                    callee,
                    args,
                    arg_marks,
                    mem_in,
                    world_in,
                    mem_out,
                    world_out,
                } = instruction
                {
                    let callee_value =
                        read_internal(&frames[frame_index].values, &callee, block_id)?;
                    let callee_value = if let InternalValue::FunctionPointer(pointer) = callee_value
                    {
                        let pointer = if let PointerValue::Exposed(address) = pointer {
                            state
                                .functions
                                .values()
                                .copied()
                                .map(PointerValue::Function)
                                .find(|candidate| {
                                    state.memory.pointer_address(*candidate) == Ok(address)
                                })
                                .unwrap_or(pointer)
                        } else {
                            pointer
                        };
                        InternalValue::FunctionPointer(
                            state
                                .memory
                                .resolve_function_pointer(pointer)
                                .map_err(|_| InterpreterError::CallThroughNonFunctionPointer {
                                    block: block_id,
                                })?,
                        )
                    } else {
                        callee_value
                    };
                    let identity = match callee_value {
                        InternalValue::FunctionPointer(PointerValue::Function(identity)) => {
                            identity
                        }
                        InternalValue::FunctionPointer(PointerValue::Null) => {
                            return Err(InterpreterError::CallThroughNullFunctionPointer {
                                block: block_id,
                            });
                        }
                        InternalValue::FunctionPointer(PointerValue::Object(_))
                        | InternalValue::FunctionPointer(PointerValue::Exposed(_))
                        | InternalValue::Pointer(_)
                        | InternalValue::Vector(_)
                        | InternalValue::Bits(_) => {
                            return Err(InterpreterError::CallThroughNonFunctionPointer {
                                block: block_id,
                            });
                        }
                        InternalValue::Uninitialized => {
                            return Err(InterpreterError::InvalidPointerRepresentation {
                                block: block_id,
                            });
                        }
                    };
                    let identity_index = identity
                        .checked_sub(1)
                        .and_then(|identity| usize::try_from(identity).ok());
                    if let Some(declaration) = identity_index
                        .and_then(|index| index.checked_sub(module.functions.len()))
                        .and_then(|index| module.declares.get(index))
                    {
                        if function_pointer_type(&declaration.signature())
                            .map_err(|message| InterpreterError::InvalidModule { message })?
                            != callee.ty
                        {
                            return Err(InterpreterError::IndirectCallDynamicSignatureMismatch {
                                block: block_id,
                            });
                        }
                        let mut arguments =
                            read_call_values(&args, block_id, &frames[frame_index].values)?;
                        arguments.push(read_internal(
                            &frames[frame_index].values,
                            &mem_in,
                            block_id,
                        )?);
                        arguments.push(read_internal(
                            &frames[frame_index].values,
                            &world_in,
                            block_id,
                        )?);
                        let returned = execute_external_call(
                            &mut state,
                            &declaration.name,
                            arguments,
                            &results,
                            block_id,
                        )?;
                        bind_call_return(
                            &mut frames,
                            CallReturnBinding {
                                results,
                                mem_out,
                                world_out,
                            },
                            returned,
                        )?;
                        continue;
                    }
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
                    let anonymous = separate_anonymous_arguments(
                        &module.functions[callee_index],
                        &args,
                        &arg_marks,
                        &mut call_arguments,
                    );
                    let callee_activation = state.begin_activation()?;
                    frames.push(ModuleFrame::new(
                        callee_index,
                        callee_entry.id,
                        call_arguments,
                        anonymous,
                        Some(CallReturnBinding {
                            results,
                            mem_out,
                            world_out,
                        }),
                        memory_frame,
                        memory_scope,
                        callee_activation,
                    ));
                } else if let Inst::VaAccess(access) = &instruction {
                    let frame = &mut frames[frame_index];
                    execute_variadic_access(
                        access,
                        &frame.anonymous,
                        block_id,
                        &mut frame.values,
                        &mut state,
                    )?;
                } else {
                    let memory_frame = frames[frame_index].memory_frame;
                    let memory_scope = frames[frame_index].memory_scope;
                    let activation = frames[frame_index].activation;
                    let name = module.functions[frames[frame_index].function_index]
                        .name
                        .clone();
                    execute_instruction(
                        &instruction,
                        &name,
                        block_id,
                        &mut frames[frame_index].values,
                        &mut state,
                        memory_frame,
                        memory_scope,
                        activation,
                    )?;
                }
            }
            FrameAction::Terminator(terminator) => match terminator {
                Inst::IndirectBr { target, edges } => {
                    let function = &module.functions[frames[frame_index].function_index].name;
                    let edge = select_indirect_edge(
                        &state,
                        function,
                        block_id,
                        &target,
                        &edges,
                        &frames[frame_index].values,
                    )?;
                    let incoming =
                        copy_values(&edge.arguments, block_id, &frames[frame_index].values)?;
                    set_module_frame_block(&mut frames[frame_index], edge.target, incoming);
                }
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
                    // A callee may fall off its end and return poison. Binding
                    // that result is transport, not a read: a discarding caller
                    // must still observe the callee's effects without a trap.
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
                        if root_kind == lifecycle::EntryKind::Main {
                            program_result =
                                public_return_values(returned_values, returned_types, block_id)?;
                        }
                    }
                }
                Inst::AsmGoto { .. } => return Err(assembly_error(block_id)),
                Inst::Trap(reason) => return Err(trap_error(reason, block_id)),
                _ => {
                    return Err(InterpreterError::InvalidModule {
                        message: format!("block {block_id} has an invalid terminator"),
                    });
                }
            },
        }
    }
}

fn execute_external_call(
    state: &mut ExecutionState,
    name: &str,
    mut arguments: Vec<InternalValue>,
    results: &[Value],
    block: BlockId,
) -> Result<Vec<InternalValue>, InterpreterError> {
    let _world = arguments
        .pop()
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "external call lacks world".into(),
        })?;
    let _memory = arguments
        .pop()
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "external call lacks memory".into(),
        })?;
    let arguments =
        arguments
            .into_iter()
            .map(|value| match value {
                // The library boundary of 10.1.14 takes C `int` and `size_t`
                // arguments only; a wider value cannot name a library argument.
                InternalValue::Bits(value) => u64::try_from(value)
                    .map(LibraryValue::Integer)
                    .map_err(|_| InterpreterError::MemoryAccess {
                        block,
                        error: MemoryFault::ExternalFunction { name: name.into() },
                    }),
                InternalValue::Pointer(value) => Ok(LibraryValue::Pointer(value)),
                _ => Err(InterpreterError::MemoryAccess {
                    block,
                    error: MemoryFault::ExternalFunction { name: name.into() },
                }),
            })
            .collect::<Result<Vec<_>, _>>()?;
    let (next, result) = state
        .memory
        .call_library(name, &arguments)
        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
    state.memory = next;
    let result = match result {
        LibraryValue::Integer(value) => match results {
            [result @ Value {
                ty: EvoIRType::Bits(_),
                ..
            }] => InternalValue::Bits(
                IntegerKernel::for_value(result, block)?.normalize(u128::from(value)),
            ),
            _ => {
                return Err(InterpreterError::MemoryAccess {
                    block,
                    error: MemoryFault::ExternalFunction { name: name.into() },
                })
            }
        },
        LibraryValue::Pointer(value) => InternalValue::Pointer(value),
    };
    Ok(vec![
        result,
        InternalValue::Bits(state.fresh_token()?),
        InternalValue::Bits(state.fresh_token()?),
    ])
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
            InternalValue::Bits(raw_bits) => Ok(RuntimeValue::wide(ty, raw_bits)),
            InternalValue::Pointer(_)
            | InternalValue::FunctionPointer(_)
            | InternalValue::Vector(_) => {
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

fn write_value(values: &mut HashMap<u32, InternalValue>, result: &Value, raw_bits: u128) {
    values.insert(result.id, InternalValue::Bits(raw_bits));
}

fn write_internal(values: &mut HashMap<u32, InternalValue>, result: &Value, value: InternalValue) {
    values.insert(result.id, value);
}

fn separate_anonymous_arguments(
    function: &Function,
    arguments: &[Value],
    marks: &[ParamMark],
    incoming: &mut Vec<InternalValue>,
) -> Vec<(EvoIRType, ParamMark, InternalValue)> {
    let named = function.params.len().saturating_sub(2);
    if !function.variadic {
        return Vec::new();
    }
    arguments[named..]
        .iter()
        .zip(incoming.drain(named..arguments.len()))
        .enumerate()
        .map(|(index, (argument, value))| {
            (
                argument.ty.clone(),
                marks.get(named + index).copied().unwrap_or(ParamMark::None),
                value,
            )
        })
        .collect()
}

fn execute_variadic_access(
    access: &VariadicAccess,
    anonymous: &[(EvoIRType, ParamMark, InternalValue)],
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    let fault = |error| InterpreterError::MemoryAccess { block, error };
    let _ = read_value(values, &access.mem_in, block)?;
    let _ = read_value(values, &access.world_in, block)?;
    let list = read_pointer(values, &access.list, block)?;
    let identity = match &access.op {
        VariadicOp::Start => Some(state.argument_lists.start(anonymous.to_vec())),
        VariadicOp::End => None,
        VariadicOp::Arg(result) => {
            let identity = state.memory.argument_list(list).map_err(fault)?;
            let (ty, mark, value) = state.argument_lists.read(identity).map_err(fault)?;
            if ty != &result.ty || *mark != ParamMark::None {
                return Err(fault(MemoryFault::InvalidArgumentList));
            }
            write_internal(values, result, *value);
            Some(state.argument_lists.advance(identity).map_err(fault)?)
        }
        VariadicOp::ArgRecord { destination, shape } => {
            let identity = state.memory.argument_list(list).map_err(fault)?;
            let (_, mark, value) = state.argument_lists.read(identity).map_err(fault)?;
            if *mark != ParamMark::ByVal(*shape) {
                return Err(fault(MemoryFault::InvalidArgumentList));
            }
            let InternalValue::Pointer(source) = *value else {
                return Err(fault(MemoryFault::InvalidArgumentList));
            };
            let destination = read_pointer(values, destination, block)?;
            state.memory = state
                .memory
                .copy_bytes(
                    destination,
                    source,
                    AccessLayout::new(shape.size, shape.align),
                )
                .map_err(fault)?;
            Some(state.argument_lists.advance(identity).map_err(fault)?)
        }
    };
    state.memory = state
        .memory
        .with_argument_list(list, identity)
        .map_err(fault)?;
    let mem = state.fresh_token()?;
    let world = state.fresh_token()?;
    write_value(values, &access.mem_out, mem);
    write_value(values, &access.world_out, world);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_instruction(
    inst: &Inst,
    function: &str,
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
    state: &mut ExecutionState,
    memory_frame: Option<FrameId>,
    memory_scope: Option<ScopeId>,
    activation: ActivationAddresses,
) -> Result<(), InterpreterError> {
    match inst {
        Inst::Vector(operation) => vector::execute(operation, block, values, state)?,
        Inst::Floating(operation) => floating::execute(operation, block, values)?,
        Inst::VaAccess(access) => execute_variadic_access(access, &[], block, values, state)?,
        Inst::Atomic(access) => atomic::execute_atomic(access, block, values, state)?,
        Inst::TlsAddr(access) => tls::execute(access, block, values, state)?,
        Inst::BitOperation {
            result,
            operand,
            op,
        } => {
            let raw = read_value(values, operand, block)?;
            let EvoIRType::Bits(width) = result.ty else {
                unreachable!("validated bit type")
            };
            // The value model is 128 bits wide (revision 1.9.61); the counts
            // are answered at the instruction's own width.
            let result_bits: u128 = match op {
                BitOp::LeadingZeros => u128::from(raw.leading_zeros() - (128 - u32::from(width))),
                BitOp::TrailingZeros => u128::from(raw.trailing_zeros().min(u32::from(width))),
                BitOp::Population => u128::from(raw.count_ones()),
                BitOp::ByteSwap => raw.swap_bytes() >> (128 - width),
            };
            write_value(values, result, result_bits);
        }
        Inst::Const(result, immediate) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            write_value(values, result, kernel.normalize(u128::from(*immediate)));
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
                u128::from(kernel.signed_add_defined(left, right)),
            );
        }
        Inst::SignedSubDefined(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u128::from(kernel.signed_sub_defined(left, right)),
            );
        }
        Inst::SignedMulDefined(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u128::from(kernel.signed_mul_defined(left, right)),
            );
        }
        Inst::DivisorNonZero(result, divisor) => {
            let kernel = IntegerKernel::for_value(divisor, block)?;
            let divisor = read_value(values, divisor, block)?;
            write_value(values, result, u128::from(kernel.normalize(divisor) != 0));
        }
        Inst::SignedDivRemNoOverflow(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u128::from(kernel.signed_div_rem_no_overflow(left, right)),
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
                u128::from(shifted_kernel.shift_count_defined(
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
                u128::from(shifted_kernel.signed_shl_defined(
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
                let left = read_pointer_value(values, lhs, block)?;
                let right = read_pointer_value(values, rhs, block)?;
                let left = state
                    .memory
                    .pointer_address(left)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                let right = state
                    .memory
                    .pointer_address(right)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                let comparison = match operation {
                    CmpOp::Eq => left == right,
                    CmpOp::Ne => left != right,
                    _ => {
                        return Err(InterpreterError::InvalidFunction {
                            message: format!("block {block} uses ordered comparison on pointers"),
                        });
                    }
                };
                write_value(values, result, u128::from(comparison));
                return Ok(());
            }
            if matches!(lhs.ty, EvoIRType::FnPtr { .. }) {
                let left = read_pointer_value(values, lhs, block)?;
                let right = read_pointer_value(values, rhs, block)?;
                let left = state
                    .memory
                    .pointer_address(left)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                let right = state
                    .memory
                    .pointer_address(right)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
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
                write_value(values, result, u128::from(comparison));
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
            write_value(values, result, u128::from(comparison));
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
        Inst::PointerCast { result, operand } => {
            let value = if result.ty == EvoIRType::Bits(64) {
                let pointer = match read_internal(values, operand, block)? {
                    InternalValue::Pointer(pointer) => PointerValue::object(pointer),
                    InternalValue::FunctionPointer(pointer) => pointer,
                    _ => return Err(InterpreterError::InvalidPointerRepresentation { block }),
                };
                let (memory, address) = state
                    .memory
                    .expose_pointer(pointer)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                state.memory = memory;
                InternalValue::Bits(u128::from(address))
            } else {
                // `ptr.from_int` takes a `bits<64>` (Section 3.7); the value model is 128 bits wide.
                let address = u64::try_from(read_value(values, operand, block)?)
                    .map_err(|_| InterpreterError::InvalidPointerRepresentation { block })?;
                if matches!(result.ty, EvoIRType::FnPtr { .. }) {
                    InternalValue::FunctionPointer(if address == 0 {
                        PointerValue::Null
                    } else {
                        PointerValue::Exposed(address)
                    })
                } else {
                    InternalValue::Pointer(Pointer::from_address(address))
                }
            };
            write_internal(values, result, value);
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
        Inst::BlockAddr {
            result,
            block: target,
        } => {
            let identity = state
                .blocks
                .get(&(function.to_owned(), *target))
                .copied()
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: format!(
                        "block_addr refers to missing block {target} of function '{function}'"
                    ),
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
        // Section 3.7 (revision 1.9.72): "the interpreters give each activation
        // a fresh value for each". The identity is the memory model's opaque
        // non-object provenance: it compares equal to itself, `ptr.to_int`
        // exposes it to a distinct address that names no object, and every load
        // and store through it is `E3013`.
        Inst::ReturnAddress { result } => {
            write_internal(
                values,
                result,
                InternalValue::FunctionPointer(
                    PointerValue::function(activation.return_address)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?,
                ),
            );
        }
        Inst::FrameAddress { result, depth } => {
            if *depth != 0 {
                return Err(InterpreterError::FrameAddressUnavailable {
                    block,
                    depth: *depth,
                });
            }
            write_internal(
                values,
                result,
                InternalValue::FunctionPointer(
                    PointerValue::function(activation.frame_address)
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
            ..
        }
        | Inst::DynamicAlloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            origin,
            ..
        } => {
            let layout = match inst {
                Inst::Alloca { layout, .. } => *layout,
                Inst::DynamicAlloca { size, align, .. } => {
                    AccessLayout::new(read_value(values, size, block)? as u64, *align)
                }
                _ => unreachable!("allocation instruction"),
            };
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
                .allocate_automatic(memory_frame, memory_scope, *origin, layout)
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
                EvoIRType::Vec(..) => {
                    let (representation, world) = state
                        .memory
                        .load_representation(&state.world, address, layout, *volatile)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    state.world = world;
                    state.save_vector(representation)
                }
                EvoIRType::Bits(_) | EvoIRType::F32 | EvoIRType::F64 => {
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
                    InternalValue::Bits(raw)
                }
                EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => {
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
                    match (&result.ty, pointer) {
                        (EvoIRType::Ptr(_), PointerValue::Null) => {
                            InternalValue::Pointer(Pointer::NULL)
                        }
                        (EvoIRType::Ptr(_), PointerValue::Object(pointer)) => {
                            InternalValue::Pointer(pointer)
                        }
                        (EvoIRType::Ptr(_), PointerValue::Exposed(address)) => {
                            InternalValue::Pointer(Pointer::from_address(address))
                        }
                        _ => InternalValue::FunctionPointer(pointer),
                    }
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
                InternalValue::Vector(identity) => {
                    let (memory, world) = state
                        .memory
                        .store_representation(
                            &state.world,
                            address,
                            layout,
                            &state.vectors[identity],
                            *volatile,
                        )
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    state.memory = memory;
                    state.world = world;
                }
                InternalValue::Bits(raw) => {
                    if *volatile {
                        let (memory, world) = state
                            .memory
                            .volatile_store_integer(&state.world, address, layout, raw)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.memory = memory;
                        state.world = world;
                    } else {
                        state.memory = state
                            .memory
                            .store_integer(address, layout, raw)
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
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            layout,
            volatile,
        } => {
            let _ = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let destination = read_pointer(values, destination, block)?;
            let source = read_pointer(values, source, block)?;
            let (memory, world) = state
                .memory
                .copy_with_volatile(&state.world, destination, source, *layout, *volatile)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            state.memory = memory;
            state.world = world;
            let next_memory_token = state.fresh_token()?;
            let next_world_token = state.fresh_token()?;
            write_value(values, mem_out, next_memory_token);
            write_value(values, world_out, next_world_token);
        }
        Inst::Asm(_) | Inst::AsmGoto { .. } => return Err(assembly_error(block)),
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
                .checked_scaled_offset(raw, width, *signedness, *direction, *scale)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            write_internal(values, result, InternalValue::Pointer(pointer));
        }
        Inst::Br(..)
        | Inst::CondBr(..)
        | Inst::IndirectBr { .. }
        | Inst::Ret(..)
        | Inst::Trap(..) => {
            return Err(InterpreterError::InvalidFunction {
                message: format!("terminator appears in instructions of block {block}"),
            });
        }
    }
    Ok(())
}

fn select_indirect_edge<'a>(
    state: &ExecutionState,
    function: &str,
    block: BlockId,
    target: &Value,
    edges: &'a [Edge],
    values: &HashMap<u32, InternalValue>,
) -> Result<&'a Edge, InterpreterError> {
    let error = || InterpreterError::InvalidIndirectBranchTarget { block };
    let pointer = match read_internal(values, target, block)? {
        InternalValue::FunctionPointer(pointer) => pointer,
        InternalValue::Pointer(pointer) => PointerValue::Exposed(
            state
                .memory
                .pointer_address(PointerValue::object(pointer))
                .map_err(|_| error())?,
        ),
        InternalValue::Uninitialized => {
            return Err(InterpreterError::InvalidPointerRepresentation { block })
        }
        InternalValue::Bits(_) | InternalValue::Vector(_) => return Err(error()),
    };
    let pointer = state
        .memory
        .resolve_function_pointer(pointer)
        .map_err(|_| error())?;
    let PointerValue::Function(identity) = pointer else {
        return Err(error());
    };
    edges
        .iter()
        .find(|edge| state.blocks.get(&(function.to_owned(), edge.target)) == Some(&identity))
        .ok_or_else(error)
}

fn read_value(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<u128, InterpreterError> {
    match values
        .get(&value.id)
        .copied()
        .ok_or(InterpreterError::MissingValue {
            block,
            value: value.id,
        })? {
        InternalValue::Bits(raw_bits) => Ok(raw_bits),
        InternalValue::Pointer(_)
        | InternalValue::FunctionPointer(_)
        | InternalValue::Vector(_) => Err(InterpreterError::InvalidFunction {
            message: format!(
                "block {block} consumes pointer v{} as integer bits",
                value.id
            ),
        }),
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
        InternalValue::Uninitialized => {
            Err(InterpreterError::InvalidPointerRepresentation { block })
        }
        InternalValue::FunctionPointer(_) => {
            Err(InterpreterError::InvalidPointerRepresentation { block })
        }
        InternalValue::Bits(_) | InternalValue::Vector(_) => {
            Err(InterpreterError::InvalidFunction {
                message: format!(
                    "block {block} consumes non-pointer v{} as pointer",
                    value.id
                ),
            })
        }
    }
}

fn read_pointer_value(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<PointerValue, InterpreterError> {
    match read_internal(values, value, block)? {
        InternalValue::FunctionPointer(pointer) => Ok(pointer),
        InternalValue::Pointer(pointer) => Ok(PointerValue::object(pointer)),
        InternalValue::Uninitialized => {
            Err(InterpreterError::InvalidPointerRepresentation { block })
        }
        InternalValue::Bits(_) | InternalValue::Vector(_) => {
            Err(InterpreterError::InvalidFunction {
                message: format!(
                    "block {block} consumes non-pointer v{} as a pointer value",
                    value.id
                ),
            })
        }
    }
}

fn matching_guard(
    state: &ExecutionState,
    memory_token: u128,
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
    for (index, object) in module.extern_objects.iter().enumerate() {
        let (memory, pointer) = state
            .memory
            .register_external(u64::MAX - index as u64, object.size, object.align)
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        state.memory = memory;
        state.globals.insert(object.symbol.clone(), pointer);
    }
    for global in &module.globals {
        if module.tls.iter().any(|t| t.symbol == global.symbol) {
            state
                .tls_templates
                .insert(global.symbol.clone(), global.clone());
            continue;
        }
        let (memory, pointer) = match global.kind {
            GlobalKind::StringLiteral => {
                let bytes = ByteSequence::try_from_bytes(global.bytes.clone())
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                state
                    .memory
                    .allocate_string_literal(global.origin, global.layout, &bytes)
                    .map_err(|error| InterpreterError::MemorySetup { error })?
            }
            GlobalKind::File | GlobalKind::FunctionStatic | GlobalKind::CompoundLiteral => state
                .memory
                .allocate_static(global.origin, global.layout, InitialState::Uninitialized)
                .map_err(|error| InterpreterError::MemorySetup { error })?,
        };
        state.memory = memory;
        if global.kind != GlobalKind::StringLiteral {
            initialize_global_bytes(global, pointer, state)?;
        }
        state.globals.insert(global.symbol.clone(), pointer);
    }
    for alias in &module.aliases {
        if let Some(template) = state.tls_templates.get(&alias.target).cloned() {
            state.tls_templates.insert(alias.name.clone(), template);
        }
        if let Some(pointer) = state.globals.get(&alias.target).copied() {
            state.globals.insert(alias.name.clone(), pointer);
        }
    }
    for global in &module.globals {
        if state.tls_templates.contains_key(&global.symbol) {
            continue;
        }
        initialize_global_relocations(global, state.globals[&global.symbol], state)?;
        if global.read_only && global.kind != GlobalKind::StringLiteral {
            state.memory = state
                .memory
                .freeze_static(state.globals[&global.symbol])
                .map_err(|error| InterpreterError::MemorySetup { error })?;
        }
    }
    Ok(())
}

fn initialize_global_bytes(
    global: &GlobalObject,
    pointer: Pointer,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
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
        let size = u64::try_from(end - start).map_err(|_| InterpreterError::InvalidModule {
            message: format!("global '{}' range exceeds u64", global.symbol),
        })?;
        let bytes = ByteSequence::try_from_static_initializer(global.bytes[start..end].to_vec())
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        state.memory = state
            .memory
            .store_bytes(address, AccessLayout::new(size, 1), &bytes)
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        start = end;
    }
    Ok(())
}

fn initialize_global_relocations(
    global: &GlobalObject,
    base: Pointer,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    for relocation in &global.relocs {
        if let Some(difference) = &relocation.difference {
            let mut address = |symbol: &str| -> Result<u64, InterpreterError> {
                let identity = symbol
                    .split_once("^b")
                    .and_then(|(name, block)| {
                        block
                            .parse::<BlockId>()
                            .ok()
                            .and_then(|block| state.blocks.get(&(name.to_owned(), block)))
                    })
                    .copied()
                    .ok_or_else(|| InterpreterError::InvalidModule {
                        message: "relative relocation names an absent block".into(),
                    })?;
                let (memory, address) = state
                    .memory
                    .expose_pointer(PointerValue::Function(identity))
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                state.memory = memory;
                Ok(address)
            };
            let raw = address(&relocation.symbol)?
                .wrapping_sub(address(&difference.base)?)
                .wrapping_add_signed(relocation.addend);
            let destination = base
                .checked_offset(relocation.offset as i64)
                .map_err(|error| InterpreterError::MemorySetup { error })?;
            state.memory = state
                .memory
                .store_integer(
                    destination,
                    AccessLayout::new(relocation.size_bytes(), 1),
                    u128::from(raw),
                )
                .map_err(|error| InterpreterError::MemorySetup { error })?;
            continue;
        }
        let pointer = if let Some(pointer) = state.globals.get(&relocation.symbol) {
            PointerValue::object(
                pointer
                    .checked_offset(relocation.addend)
                    .map_err(|error| InterpreterError::MemorySetup { error })?,
            )
        } else if let Some(identity) = state.functions.get(&relocation.symbol) {
            if relocation.addend != 0 {
                return Err(InterpreterError::InvalidModule {
                    message: "function relocation has a nonzero addend".into(),
                });
            }
            PointerValue::Function(*identity)
        } else if let Some((name, block)) = relocation.symbol.split_once("^b") {
            let identity = block
                .parse::<BlockId>()
                .ok()
                .and_then(|block| state.blocks.get(&(name.to_owned(), block)))
                .copied()
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "relocation names an absent block".into(),
                })?;
            let value = PointerValue::Function(identity);
            if relocation.addend == 0 {
                value
            } else {
                // A nonzero code addend is an integer address, not the
                // identity of another block or an object allocation.
                let address = state
                    .memory
                    .pointer_address(value)
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                PointerValue::object(Pointer::from_address(
                    address.wrapping_add_signed(relocation.addend),
                ))
            }
        } else {
            return Err(InterpreterError::InvalidModule {
                message: "relocation names an absent symbol".into(),
            });
        };
        let destination = base
            .checked_offset(relocation.offset as i64)
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        if relocation.exposed {
            (state.memory, _) = state
                .memory
                .expose_pointer(pointer)
                .map_err(|error| InterpreterError::MemorySetup { error })?;
        }
        state.memory = state
            .memory
            // A data relocation patches bytes, including packed fields;
            // the subsequent load/store carries its own access alignment.
            .store_pointer_value(destination, AccessLayout::new(8, 1), pointer)
            .map_err(|error| InterpreterError::MemorySetup { error })?;
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
            expected: Box::new(expected.clone()),
            actual: Box::new(value.ty.clone()),
        });
    }
    match expected {
        EvoIRType::F32 | EvoIRType::F64 => {
            let kind = floating_kind(expected).unwrap();
            evocc_types::floating::FloatBits::from_bits(kind, value.value()).ok_or_else(|| {
                InterpreterError::InvalidEntryArgument {
                    index,
                    message: "floating representation exceeds format width".into(),
                }
            })?;
            Ok(InternalValue::Bits(value.value()))
        }
        EvoIRType::Bits(_) => Ok(InternalValue::Bits(
            IntegerKernel::for_type(expected, block)?.normalize(value.value()),
        )),
        EvoIRType::Mem(_) | EvoIRType::World => Ok(InternalValue::Bits(value.value())),
        _ => Err(InterpreterError::UnsupportedRuntimeType {
            index,
            ty: expected.clone(),
        }),
    }
}

/// One width-aware bit-vector kernel shared by every executed integer opcode.
///
/// The carrier is `u128` because EvoIR 10.1.19 (revision 1.9.61) makes
/// `bits<128>` an executed scalar type: "Both interpreters extend their
/// scalar value model to 128 bits for these forms." Every narrower width
/// still answers exactly what it answered before, because the mask is the
/// only thing the width decides.
#[derive(Clone, Copy)]
struct IntegerKernel {
    width: u16,
    mask: u128,
}

impl IntegerKernel {
    fn for_value(value: &Value, block: BlockId) -> Result<Self, InterpreterError> {
        Self::for_type(&value.ty, block)
    }

    /// The executed widths are 1 to 64 and 128, and nothing between.
    ///
    /// Section 1.1 admits every positive width; execution admits the two
    /// ranges a value model exists for. A width outside them is reported as
    /// `not-executed: wide-integer`, which is the only diagnostic the
    /// specification names for the case.
    fn for_type(ty: &EvoIRType, block: BlockId) -> Result<Self, InterpreterError> {
        let EvoIRType::Bits(width @ (1..=64 | 128)) = ty else {
            let width = match ty {
                EvoIRType::Bits(width) => *width,
                _ => 0,
            };
            return Err(InterpreterError::UnsupportedBitWidth { block, width });
        };
        let mask = if *width >= 128 {
            u128::MAX
        } else {
            (1_u128 << width) - 1
        };
        Ok(Self {
            width: *width,
            mask,
        })
    }

    fn normalize(self, value: u128) -> u128 {
        value & self.mask
    }

    fn add(self, left: u128, right: u128) -> u128 {
        self.normalize(left.wrapping_add(right))
    }

    fn sub(self, left: u128, right: u128) -> u128 {
        self.normalize(left.wrapping_sub(right))
    }

    fn mul(self, left: u128, right: u128) -> u128 {
        self.normalize(left.wrapping_mul(right))
    }

    fn shl(self, left: u128, amount: u128) -> u128 {
        if amount >= u128::from(self.width) {
            0
        } else {
            self.normalize(self.normalize(left) << amount)
        }
    }

    fn lshr(self, left: u128, amount: u128) -> u128 {
        if amount >= u128::from(self.width) {
            0
        } else {
            self.normalize(left) >> amount
        }
    }

    fn ashr(self, left: u128, amount: u128) -> u128 {
        if amount >= u128::from(self.width) {
            if self.signed(left) < 0 {
                self.mask
            } else {
                0
            }
        } else {
            self.encode_signed(self.signed(left) >> amount)
        }
    }

    fn signed_add_defined(self, left: u128, right: u128) -> bool {
        self.signed(left)
            .checked_add(self.signed(right))
            .is_some_and(|sum| self.signed_fits(sum))
    }

    fn signed_sub_defined(self, left: u128, right: u128) -> bool {
        self.signed(left)
            .checked_sub(self.signed(right))
            .is_some_and(|difference| self.signed_fits(difference))
    }

    fn signed_mul_defined(self, left: u128, right: u128) -> bool {
        self.signed(left)
            .checked_mul(self.signed(right))
            .is_some_and(|product| self.signed_fits(product))
    }

    fn signed_minimum(self) -> i128 {
        if self.width >= 128 {
            i128::MIN
        } else {
            -(1_i128 << (self.width - 1))
        }
    }

    fn signed_div_rem_no_overflow(self, left: u128, right: u128) -> bool {
        !(self.signed(left) == self.signed_minimum() && self.signed(right) == -1)
    }

    fn decoded_shift_amount(
        self,
        amount_kernel: Self,
        amount: u128,
        amount_signed: bool,
    ) -> Option<u64> {
        let mathematical = if amount_signed {
            amount_kernel.signed(amount)
        } else {
            i128::try_from(amount_kernel.normalize(amount)).ok()?
        };
        if mathematical < 0 || mathematical >= i128::from(self.width) {
            None
        } else {
            u64::try_from(mathematical).ok()
        }
    }

    fn shift_count_defined(self, amount_kernel: Self, amount: u128, amount_signed: bool) -> bool {
        self.decoded_shift_amount(amount_kernel, amount, amount_signed)
            .is_some()
    }

    /// EvoIR 3.3.1: a valid count, a non-negative left operand, and a shifted
    /// result representable in signed `bits<N>`.
    ///
    /// The representability test is the round trip rather than a widened
    /// product, because at `N = 128` there is no wider signed type to compute
    /// the product in. For a non-negative operand the two agree exactly: the
    /// shift loses no bit and leaves the sign clear precisely when the value
    /// is below `2^(N - 1 - amount)`.
    fn signed_shl_defined(
        self,
        left: u128,
        amount_kernel: Self,
        amount: u128,
        amount_signed: bool,
    ) -> bool {
        let Some(amount) = self.decoded_shift_amount(amount_kernel, amount, amount_signed) else {
            return false;
        };
        if self.signed(left) < 0 {
            return false;
        }
        let amount = u128::from(amount);
        let shifted = self.shl(left, amount);
        self.signed(shifted) >= 0 && self.lshr(shifted, amount) == self.normalize(left)
    }

    fn signed_fits(self, value: i128) -> bool {
        if self.width >= 128 {
            return true;
        }
        let minimum = -(1_i128 << (self.width - 1));
        let maximum = (1_i128 << (self.width - 1)) - 1;
        (minimum..=maximum).contains(&value)
    }

    fn signed(self, value: u128) -> i128 {
        let value = self.normalize(value);
        if self.width >= 128 {
            return value as i128;
        }
        let sign_bit = 1_u128 << (self.width - 1);
        if value & sign_bit == 0 {
            value as i128
        } else {
            (value as i128) - (1_i128 << self.width)
        }
    }

    fn encode_signed(self, value: i128) -> u128 {
        self.normalize(value as u128)
    }

    fn sdiv(self, left: u128, right: u128, block: BlockId) -> Result<u128, InterpreterError> {
        let left = self.signed(left);
        let right = self.signed(right);
        if right == 0 {
            return Err(InterpreterError::DivisionByZero {
                block,
                operation: "signed division",
            });
        }
        if left == self.signed_minimum() && right == -1 {
            return Err(InterpreterError::SignedDivisionOverflow {
                block,
                width: self.width,
            });
        }
        Ok(self.encode_signed(left / right))
    }

    fn udiv(self, left: u128, right: u128, block: BlockId) -> Result<u128, InterpreterError> {
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

    fn srem(self, left: u128, right: u128, block: BlockId) -> Result<u128, InterpreterError> {
        let left = self.signed(left);
        let right = self.signed(right);
        if right == 0 {
            return Err(InterpreterError::RemainderByZero {
                block,
                operation: "signed remainder",
            });
        }
        if left == self.signed_minimum() && right == -1 {
            return Err(InterpreterError::SignedRemainderOverflow {
                block,
                width: self.width,
            });
        }
        Ok(self.encode_signed(left % right))
    }

    fn urem(self, left: u128, right: u128, block: BlockId) -> Result<u128, InterpreterError> {
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

// ---------------------------------------------------------------------------
// Operand rewriting
// ---------------------------------------------------------------------------

/// Follow an alias chain to the value it ends at.
///
/// The visited set is what makes a cyclic alias table terminate rather than
/// hang; a cycle is a caller's defect and resolving it to the value the walk
/// re-entered is the answer that keeps every other operand usable.
pub fn resolve_alias(value: &Value, aliases: &HashMap<u32, Value>) -> Value {
    let mut value = value.clone();
    let mut visited = HashSet::new();
    while visited.insert(value.id) {
        let Some(next) = aliases.get(&value.id) else {
            break;
        };
        value = next.clone();
    }
    value
}

fn remap(value: &mut Value, aliases: &HashMap<u32, Value>) {
    *value = resolve_alias(value, aliases);
}

#[allow(clippy::too_many_lines)]
pub fn remap_inst_uses(inst: &mut Inst, aliases: &HashMap<u32, Value>) {
    match inst {
        Inst::Floating(operation) => {
            for value in &mut operation.arguments {
                remap(value, aliases);
            }
        }
        Inst::TlsAddr(a) => {
            remap(&mut a.mem_in, aliases);
            remap(&mut a.world_in, aliases);
        }
        Inst::Vector(operation) => {
            for value in &mut operation.arguments {
                remap(value, aliases);
            }
        }
        Inst::Atomic(access) => {
            for value in access.operands_mut() {
                remap(value, aliases);
            }
        }
        Inst::IndirectBr { target, edges } => {
            remap(target, aliases);
            for value in edges.iter_mut().flat_map(|edge| &mut edge.arguments) {
                remap(value, aliases);
            }
        }
        Inst::PointerCast { operand, .. } => remap(operand, aliases),
        Inst::BitOperation { operand, .. } => remap(operand, aliases),
        Inst::VaAccess(access) => {
            remap(&mut access.list, aliases);
            remap(&mut access.mem_in, aliases);
            remap(&mut access.world_in, aliases);
            if let VariadicOp::ArgRecord { destination, .. } = &mut access.op {
                remap(destination, aliases);
            }
        }
        Inst::Asm(assembly) => {
            for value in assembly.operands_mut() {
                remap(value, aliases);
            }
        }
        Inst::AsmGoto {
            assembly,
            labels,
            fallthrough,
        } => {
            for value in assembly.operands_mut() {
                remap(value, aliases);
            }
            for edge in labels
                .iter_mut()
                .map(|label| &mut label.edge)
                .chain([fallthrough])
            {
                for value in &mut edge.arguments {
                    remap(value, aliases);
                }
            }
        }
        Inst::Const(..)
        | Inst::Uninitialized(..)
        | Inst::Trap(..)
        | Inst::PtrNull(..)
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. }
        | Inst::BlockAddr { .. }
        | Inst::ReturnAddress { .. }
        | Inst::FrameAddress { .. } => {}
        Inst::Add(_, left, right)
        | Inst::Sub(_, left, right)
        | Inst::Mul(_, left, right)
        | Inst::SRem(_, left, right)
        | Inst::URem(_, left, right)
        | Inst::Shl(_, left, right)
        | Inst::AShr(_, left, right)
        | Inst::LShr(_, left, right)
        | Inst::And(_, left, right)
        | Inst::Or(_, left, right)
        | Inst::Xor(_, left, right)
        | Inst::SignedAddDefined(_, left, right)
        | Inst::SignedSubDefined(_, left, right)
        | Inst::SignedMulDefined(_, left, right)
        | Inst::SignedDivRemNoOverflow(_, left, right)
        | Inst::SDiv(_, left, right)
        | Inst::UDiv(_, left, right)
        | Inst::ICmp(_, _, left, right) => {
            remap(left, aliases);
            remap(right, aliases);
        }
        Inst::DivisorNonZero(_, divisor) | Inst::UbAssume(divisor, _) => remap(divisor, aliases),
        Inst::ShiftCountDefined {
            shifted, amount, ..
        }
        | Inst::SignedShlDefined {
            shifted, amount, ..
        } => {
            remap(shifted, aliases);
            remap(amount, aliases);
        }
        Inst::Br(_, arguments) | Inst::Ret(arguments) => {
            for argument in arguments {
                remap(argument, aliases);
            }
        }
        Inst::CondBr(condition, _, true_arguments, _, false_arguments) => {
            remap(condition, aliases);
            for argument in true_arguments.iter_mut().chain(false_arguments) {
                remap(argument, aliases);
            }
        }
        Inst::Load {
            address,
            mem_in,
            world_in,
            ..
        } => {
            remap(address, aliases);
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            ..
        } => {
            remap(address, aliases);
            remap(value, aliases);
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            ..
        } => {
            remap(destination, aliases);
            remap(source, aliases);
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::Alloca {
            mem_in, world_in, ..
        } => {
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::DynamicAlloca {
            mem_in,
            world_in,
            size,
            ..
        } => {
            remap(mem_in, aliases);
            remap(world_in, aliases);
            remap(size, aliases);
        }
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            ..
        } => {
            remap(mem_in, aliases);
            remap(world_in, aliases);
            remap(pointer, aliases);
        }
        Inst::PointerValid {
            mem_in, pointer, ..
        }
        | Inst::AccessInBounds {
            mem_in, pointer, ..
        } => {
            remap(mem_in, aliases);
            remap(pointer, aliases);
        }
        Inst::PtrOffset { base, index, .. } => {
            remap(base, aliases);
            remap(index, aliases);
        }
        Inst::Call {
            args,
            mem_in,
            world_in,
            ..
        } => {
            for argument in args {
                remap(argument, aliases);
            }
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::IndirectCall {
            callee,
            args,
            mem_in,
            world_in,
            ..
        } => {
            remap(callee, aliases);
            for argument in args {
                remap(argument, aliases);
            }
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::SExt(_, source, _) | Inst::ZExt(_, source, _) | Inst::Trunc(_, source, _) => {
            remap(source, aliases);
        }
    }
}

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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
                    arg_marks: Vec::new(),
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
            param_marks: Vec::new(),
        }
    }

    #[test]
    fn entry_interface_is_non_ssa_and_interpreter_accepts_explicit_arguments() {
        let input = bits(0, 32);
        let one = bits(1, 32);
        let result = bits(2, 32);
        let function = Function {
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            arg_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
                        arg_marks: Vec::new(),
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
            param_marks: Vec::new(),
        };
        let mut module = Module {
            tls: Vec::new(),
            lifecycle: Vec::new(),
            aliases: Vec::new(),
            assembly: Vec::new(),
            extern_objects: Vec::new(),
            declares: Vec::new(),
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
            variadic: false,
            params: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
            results: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
            marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
                        arg_marks: Vec::new(),
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
            param_marks: Vec::new(),
        };
        let mut module = Module {
            tls: Vec::new(),
            lifecycle: Vec::new(),
            aliases: Vec::new(),
            assembly: Vec::new(),
            extern_objects: Vec::new(),
            declares: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
                            arg_marks: Vec::new(),
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
            param_marks: Vec::new(),
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
            tls: Vec::new(),
            lifecycle: Vec::new(),
            aliases: Vec::new(),
            assembly: Vec::new(),
            extern_objects: Vec::new(),
            declares: Vec::new(),
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
            .contains("E3041"));

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
        let text = render_module(&module);
        let parsed = parse_section9_module(text.as_bytes())
            .expect("function pointer text parses through the shared Section 9 reader");
        assert_eq!(render_module(&parsed), text);
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
        let mut state = ExecutionState::for_target(&target, 4).expect("valid execution state");
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
        let text = render_module(&module);
        let parsed = parse_section9_module(text.as_bytes())
            .expect("direct-call CFG text parses through the shared Section 9 reader");
        assert_eq!(render_module(&parsed), text);
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
                section: None,
                patchable_entry: None,
                stack_protect: false,
                alignment: 4,
                linkage: crate::Linkage::Strong,
                visibility: crate::GlobalVisibility::External,
                variadic: false,
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
                param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
        };
        validate_ssa(&function).expect("ordinary typed uninitialized values are valid SSA");

        let pointer = value(71, EvoIRType::Ptr(AddressSpace::Stack));
        let pointer_function = Function {
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
                section: None,
                patchable_entry: None,
                stack_protect: false,
                alignment: 4,
                linkage: crate::Linkage::Strong,
                visibility: crate::GlobalVisibility::External,
                variadic: false,
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
                param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
        };
        reject(&invalid_void, "void is not an SSA value type");
    }

    #[test]
    fn uninitialized_propagates_across_block_arguments_without_being_read() {
        let poison = bits(100, 32);
        let carried = bits(101, 32);
        let replacement = bits(102, 32);
        let function = Function {
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
        };

        let outcome = interpret_function(&function, &[], InterpreterLimits::new(4))
            .expect("copying poison through a block argument is not a semantic read");
        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 7)]);
        assert_eq!(outcome.steps, 4);

        let module_outcome = interpret_module(
            &Module {
                tls: Vec::new(),
                lifecycle: Vec::new(),
                aliases: Vec::new(),
                assembly: Vec::new(),
                extern_objects: Vec::new(),
                declares: Vec::new(),
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
                    tls: Vec::new(),
                    lifecycle: Vec::new(),
                    aliases: Vec::new(),
                    assembly: Vec::new(),
                    extern_objects: Vec::new(),
                    declares: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
        };

        let outcome = interpret_function(&function, &[], InterpreterLimits::new(14))
            .expect("the loop assigns x before the only semantic read at return");
        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 7)]);
        assert_eq!(outcome.steps, 14);

        let module_outcome = interpret_module(
            &Module {
                tls: Vec::new(),
                lifecycle: Vec::new(),
                aliases: Vec::new(),
                assembly: Vec::new(),
                extern_objects: Vec::new(),
                declares: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            tls: Vec::new(),
            lifecycle: Vec::new(),
            aliases: Vec::new(),
            assembly: Vec::new(),
            extern_objects: Vec::new(),
            declares: Vec::new(),
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
                    section: None,
                    patchable_entry: None,
                    stack_protect: false,
                    alignment: 4,
                    linkage: crate::Linkage::Strong,
                    visibility: crate::GlobalVisibility::External,
                    variadic: false,
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
                    param_marks: Vec::new(),
                },
                3,
            ),
            (
                Function {
                    section: None,
                    patchable_entry: None,
                    stack_protect: false,
                    alignment: 4,
                    linkage: crate::Linkage::Strong,
                    visibility: crate::GlobalVisibility::External,
                    variadic: false,
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
                    param_marks: Vec::new(),
                },
                3,
            ),
            (
                Function {
                    section: None,
                    patchable_entry: None,
                    stack_protect: false,
                    alignment: 4,
                    linkage: crate::Linkage::Strong,
                    visibility: crate::GlobalVisibility::External,
                    variadic: false,
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
                    param_marks: Vec::new(),
                },
                2,
            ),
            (
                Function {
                    section: None,
                    patchable_entry: None,
                    stack_protect: false,
                    alignment: 4,
                    linkage: crate::Linkage::Strong,
                    visibility: crate::GlobalVisibility::External,
                    variadic: false,
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
                    param_marks: Vec::new(),
                },
                2,
            ),
            (
                Function {
                    section: None,
                    patchable_entry: None,
                    stack_protect: false,
                    alignment: 4,
                    linkage: crate::Linkage::Strong,
                    visibility: crate::GlobalVisibility::External,
                    variadic: false,
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
                    param_marks: Vec::new(),
                },
                2,
            ),
            (
                Function {
                    section: None,
                    patchable_entry: None,
                    stack_protect: false,
                    alignment: 4,
                    linkage: crate::Linkage::Strong,
                    visibility: crate::GlobalVisibility::External,
                    variadic: false,
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
                    param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            InterpreterLimits::new(5),
        )
        .expect_err("observing the caller's result consumes the transported poison");
        assert_eq!(error, InterpreterError::UninitializedRead { block: 0 });
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
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
            param_marks: Vec::new(),
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
                tls: Vec::new(),
                lifecycle: Vec::new(),
                aliases: Vec::new(),
                assembly: Vec::new(),
                extern_objects: Vec::new(),
                declares: Vec::new(),
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
