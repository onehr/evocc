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
pub use errors::InterpreterError;
pub use function_execution::{eval_function, interpret_function, interpret_function_with_target};
pub use instruction_types::{validate_type_well_formed, validate_value_type};
pub use instruction_values::{inst_results, inst_uses};
pub use module_execution::{
    interpret_module, interpret_module_with_fp_environment, interpret_module_with_target,
};
pub use module_validation::validate_module;
pub use validation::{evoir_access_layout, validate_record_marks, validate_ssa};
pub use value_aliases::{remap_inst_uses, resolve_alias};

pub use atomic::{AtomicAccess, AtomicCopyOp, AtomicOp, AtomicOrder, AtomicRmwOp};
pub use floating::{floating_bits_type, floating_kind, is_half_type, FloatingInst, FloatingOp};
mod strict_floating;
pub use strict_floating::StrictFloatingInst;
mod errors;
mod function_execution;
mod global_memory;
mod instruction_execution;
mod instruction_types;
mod instruction_values;
mod integer_kernel;
mod module_execution;
mod module_validation;
mod runtime_values;
#[cfg(test)]
mod tests;
mod text;
mod text_parse;
mod validation;
mod value_aliases;
mod variadic;
mod vector;

use errors::*;
use function_execution::*;
use global_memory::*;
use instruction_execution::*;
use instruction_types::*;
use module_execution::*;
use module_validation::*;
use runtime_values::*;
use validation::*;
pub use variadic::{call_argument_marks, VariadicAccess, VariadicOp};
pub use vector::{floating_vector_shape, integer_vector_shape, vector_shape, VectorInst, VectorOp};

/// A terminator edge: positional SSA bindings, independent of its producer.
pub use assembly::AssemblyEdge as Edge;
pub use assembly::{
    assembly_address_type, assembly_clobber, assembly_memory_constraint, assembly_register_binding,
    assembly_register_type, AssemblyEdge, AssemblyInput, AssemblyLabel, AssemblyOutput,
    InlineAssembly,
};
pub use text::{
    render_global, render_module, render_pre_fenv_module, render_target_module,
    validate_section9_bytes, IMPLICIT_TARGET,
};
pub use text_parse::{
    parse_global_text, parse_pre_fenv_module, parse_pre_fenv_module_detailed,
    parse_section9_module, parse_target_module, parse_target_module_detailed, ModuleParseError,
};

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
    /// A call without a prototype of an old-style definition does not
    /// agree with the definition (Section 10.1.7). Native code drops this
    /// obligation in both UB-check profiles (EvoMIR 5.2).
    OldStyleCall,
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
            Self::OldStyleCall => "E3068",
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
            Self::OldStyleCall => "old-style-call-mismatch",
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
            Self::OldStyleCall => "old_style_call",
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
    StrictFloating(StrictFloatingInst),
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
    /// pattern. The complete pattern must fit the declared result width.
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
    /// The loaded type is owned by `result`. A `mask` selects the bits that
    /// must be initialized; the other bits may be uninitialized (3.8). A
    /// volatile masked load is still one volatile event.
    Load {
        result: Value,
        address: Value,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
        volatile: bool,
        mask: Option<u128>,
    },

    /// Store `value` through `address`, consuming and producing effect tokens.
    /// A `mask` selects the bits that become initialized; the other bits keep
    /// their initializedness (3.8).
    Store {
        address: Value,
        value: Value,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
        volatile: bool,
        mask: Option<u128>,
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
        /// `call.returns_twice` (Section 3.10): the callee can resume this
        /// call a second time, as `setjmp` does after a `longjmp`.
        returns_twice: bool,
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
    pub fenv: bool,
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
            fenv: true,
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
        self.fenv |= other.fenv;
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
pub use lifecycle::{
    interpret_program, interpret_program_with_site, interpret_program_with_target_and_site,
};
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
                    // A second return resumes the caller after code that this
                    // call does not bound, so the call takes the top summary.
                    Inst::Call {
                        returns_twice: true,
                        ..
                    } => summary.join(EffectSummary::top()),
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
        Inst::StrictFloating(_) => EffectSummary {
            fenv: true,
            ..EffectSummary::default()
        },
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
            // A form on memory also reads or writes its ordinary buffers.
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
            fenv: true,
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

struct CallableSignature<'a> {
    ordinary_params: &'a [FunctionParam],
    ordinary_returns: &'a [EvoIRType],
    memory_region: Region,
}

const REGION_ORDER: [Region; 6] = [
    Region::All,
    Region::Unknown,
    Region::Global,
    Region::Stack,
    Region::Heap,
    Region::Escaped,
];

#[derive(Clone, Copy, Default)]
struct EffectState {
    mem: [Option<u32>; REGION_ORDER.len()],
    world: Option<u32>,
}

/// Maximum accepted nesting for an attacker-provided EvoIR type.
pub const MAX_TYPE_NESTING: usize = 256;

#[derive(Clone, Copy)]
enum Conversion {
    SignExtend,
    ZeroExtend,
    Truncate,
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
    /// The target's complex helpers are compiler-rt's, which have no model.
    compiler_rt_complex: bool,
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
            compiler_rt_complex: target.compiler_rt_complex(),
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

/// The instruction about to execute. The index after the last ordinary
/// instruction denotes the terminator. No source-language position is guessed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionSite {
    pub function: usize,
    pub block: BlockId,
    pub instruction: usize,
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
