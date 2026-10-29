//! EvoIR errors.

use super::*;

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
    /// A complex product or quotient whose target helper has no execution
    /// model: Darwin's compiler-rt sequence (3.13.5).
    ComplexHelperUnavailable {
        block: BlockId,
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
            Self::ComplexHelperUnavailable { .. } => "E3067",
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
            Self::ComplexHelperUnavailable { .. } => "not-executed: complex-arithmetic".to_owned(),
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
                "interpreter does not support {operation} in EvoIR block {block}"
            ),
            Self::FrameAddressUnavailable { block, depth } => write!(
                formatter,
                "not-executed: frame-address (depth {depth} in block {block})"
            ),
            Self::ComplexHelperUnavailable { block } => write!(
                formatter,
                "not-executed: complex-arithmetic (the target helper has no model) in block {block}"
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

pub(super) fn assembly_error(block: BlockId) -> InterpreterError {
    InterpreterError::UndefinedBehavior {
        block,
        diagnostic: "E3021",
        message: "not-lowered: asm",
    }
}

pub(super) fn trap_error(reason: TrapReason, block: BlockId) -> InterpreterError {
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
