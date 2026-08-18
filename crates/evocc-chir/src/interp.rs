//! Deterministic, fuel-bounded execution for the Day-5 scalar C-HIR subset.
//!
//! This interpreter deliberately consumes typed C-HIR and its normalized
//! initializer plans. It does not reuse the parser's constant-expression
//! evaluator: every accepted operation goes through the shared target integer
//! kernel and every operation outside the scalar contract fails closed.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
};

use evocc_common::SourceSpan;
use evocc_types::{
    FunctionParameters, TagDefinition, TagState, TargetDataLayout, TypeId, TypeInterner, TypeKind,
};

use crate::{
    integer::{
        enum_integer_type, eval_binary_integer, eval_unary_integer, integer_literal_value,
        integer_type_for_rank, usual_arithmetic_type, CInteger, CIntegerType, IntegerError,
    },
    AssignmentOp, BinaryOp, CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, CHirUnit,
    ExprCategory, FunctionDecl, InitAction, ObjectDecl, ParameterSyntax, ScalarPlace, ScopeKind,
    StorageClass, SymbolId, SymbolKind, UpdateOp, WrittenDeclarator, WrittenDeclaratorLayer,
};

const DEFAULT_FUEL: u64 = 100_000;

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
    pub raw_bits: u64,
}

impl ScalarValue {
    /// Decode the target bits according to the canonical integer or enum type.
    pub fn mathematical_value(self, types: &TypeInterner) -> Result<i128, ScalarValueError> {
        let ty = canonical_integer_type(types, self.ty)
            .ok_or(ScalarValueError::UnsupportedType { ty: self.ty })?;
        Ok(CInteger::new(self.raw_bits, ty).mathematical_value())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarValueError {
    UnsupportedType { ty: TypeId },
}

impl fmt::Display for ScalarValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedType { ty } => {
                write!(formatter, "unsupported scalar value type ID {}", ty.raw())
            }
        }
    }
}

impl std::error::Error for ScalarValueError {}

/// Successful execution and its deterministic budget witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterpretOutcome {
    pub return_value: Option<ScalarValue>,
    pub steps: u64,
}

/// A fail-closed scalar interpreter error.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpreterError {
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
}

impl InterpreterError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ReadOfUninitializedObject { .. } => "E3001",
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
            Self::UnknownFunction { .. } => None,
            Self::FunctionHasNoBody { span, .. }
            | Self::UnsupportedFunctionSignature { span, .. }
            | Self::UnsupportedType { span, .. }
            | Self::UnsupportedExpression { span, .. }
            | Self::UnsupportedStatement { span, .. }
            | Self::UnsupportedDeclaration { span, .. }
            | Self::MalformedFunctionDeclaration { span, .. }
            | Self::DuplicateFunctionBody { span, .. }
            | Self::MalformedObjectDeclaration { span, .. }
            | Self::InvalidInitializerPlan { span, .. }
            | Self::InvalidScalarPlace { span, .. }
            | Self::ReadOfUninitializedObject { span, .. }
            | Self::OutOfScopeObject { span, .. }
            | Self::Integer { span, .. }
            | Self::InvalidControlFlow { span, .. }
            | Self::UnknownLabel { span, .. }
            | Self::DuplicateLabel { span, .. }
            | Self::MissingReturn { span }
            | Self::FuelExhausted { span, .. } => Some(*span),
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

#[derive(Debug)]
enum Flow {
    Normal,
    Return(Option<CInteger>),
    Break(SourceSpan),
    Continue(SourceSpan),
    Goto { spelling: String, span: SourceSpan },
}

#[derive(Debug)]
struct FunctionEnd {
    symbol: SymbolId,
    return_type: TypeId,
    return_integer: Option<CIntegerType>,
    is_main: bool,
    body_span: SourceSpan,
    caller_slots: Option<HashMap<SymbolId, Slot>>,
    caller_fuel_anchor: Option<SourceSpan>,
    caller_expects_value: bool,
    value_base: usize,
}

#[derive(Debug)]
struct ValidatedFunctionSignature {
    return_type: TypeId,
    return_integer: Option<CIntegerType>,
    parameters: Vec<(SymbolId, CIntegerType)>,
}

#[derive(Debug)]
enum MachineTask<'arena> {
    FunctionEnd(FunctionEnd),
    RestoreSlots(Vec<(SymbolId, Slot)>),
    RestoreFuelAnchor(Option<SourceSpan>),
    ExecDeclaration(&'arena CHirDecl<'arena>),
    ExecStatement(&'arena CHirStmt<'arena>),
    ExecSwitchStatement(&'arena CHirStmt<'arena>),
    CompoundContinue {
        items: &'arena [CHirBlockItem<'arena>],
        index: usize,
    },
    SwitchContinue {
        items: &'arena [CHirBlockItem<'arena>],
        index: usize,
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
    FinishReturnValue(SourceSpan),
    SelectIf {
        then_statement: &'arena CHirStmt<'arena>,
        else_statement: Option<&'arena CHirStmt<'arena>>,
    },
    EvalExpression {
        expression: &'arena CHirExpr<'arena>,
        allow_root_mutation: bool,
    },
    ApplyUnary {
        operator: crate::UnaryOp,
        expression_span: SourceSpan,
        operand_span: SourceSpan,
    },
    ContinueBinary {
        operator: BinaryOp,
        right: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    },
    ApplyBinary {
        operator: BinaryOp,
        left: CInteger,
        span: SourceSpan,
    },
    SelectTernary {
        then_expression: &'arena CHirExpr<'arena>,
        else_expression: &'arena CHirExpr<'arena>,
        result_type: CIntegerType,
    },
    CastValue {
        target: CIntegerType,
        span: SourceSpan,
    },
    ApplyAssignment {
        operator: AssignmentOp,
        symbol: SymbolId,
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
        values: Vec<CInteger>,
        span: SourceSpan,
        expects_value: bool,
    },
    CollectCallArgument {
        callee: &'arena CHirExpr<'arena>,
        arguments: &'arena [&'arena CHirExpr<'arena>],
        index: usize,
        values: Vec<CInteger>,
        span: SourceSpan,
        expects_value: bool,
    },
}

#[derive(Debug, Clone, Copy)]
enum MachineBlockMode {
    Compound,
    Switch,
}

#[derive(Debug, Clone, Copy)]
enum FunctionExit {
    Explicit(Option<CInteger>),
    FellOffEnd,
}

/// Interpret one zero-parameter scalar C-HIR function under a deterministic
/// target-fixed budget.
pub fn interpret_scalar_function(
    unit: &CHirUnit<'_>,
    function: SymbolId,
    limits: InterpreterLimits,
) -> Result<InterpretOutcome, InterpreterError> {
    let interpreter = Interpreter::new(unit, limits);
    interpreter.validate_root_function(function)?;
    if function_has_call(unit, function) || function_has_goto(unit, function) {
        interpreter.run_with_calls(function)
    } else {
        interpreter.run(function)
    }
}

struct Interpreter<'unit, 'arena> {
    unit: &'unit CHirUnit<'arena>,
    slots: HashMap<SymbolId, Slot>,
    fuel_limit: u64,
    consumed: u64,
    fuel_anchor: Option<SourceSpan>,
}

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    fn new(unit: &'unit CHirUnit<'arena>, limits: InterpreterLimits) -> Self {
        Self {
            unit,
            slots: HashMap::new(),
            fuel_limit: limits.fuel(),
            consumed: 0,
            fuel_anchor: None,
        }
    }

    fn run(mut self, function: SymbolId) -> Result<InterpretOutcome, InterpreterError> {
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
            if declaration.name.spelling != symbol.spelling() || declaration.ty != symbol.ty() {
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
        let declaration = definition.ok_or(InterpreterError::FunctionHasNoBody {
            symbol: function,
            span: symbol.span(),
        })?;

        let canonical = self.unit.types().get(declaration.ty).ok_or(
            InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            },
        )?;
        let TypeKind::Function {
            return_type,
            parameters,
        } = &canonical.kind
        else {
            return Err(InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            });
        };
        let zero_parameters = match parameters {
            FunctionParameters::Unspecified => true,
            FunctionParameters::Prototype { types, variadic } => types.is_empty() && !variadic,
        };
        if !zero_parameters {
            return Err(InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            });
        }
        let return_type = *return_type;
        let returns_void = matches!(
            self.unit.types().get(return_type).map(|ty| &ty.kind),
            Some(TypeKind::Void)
        );
        let return_integer = if returns_void {
            None
        } else {
            Some(self.integer_type(return_type, declaration.span)?)
        };
        let body = declaration
            .body
            .ok_or(InterpreterError::FunctionHasNoBody {
                symbol: function,
                span: declaration.span,
            })?;
        let is_main = symbol.spelling() == "main";

        validate_function_control_flow(body)?;
        self.predeclare_stmt(body)?;

        let flow = self.exec_stmt(body)?;
        let return_value = match (flow, return_integer) {
            (Flow::Return(Some(value)), Some(return_integer)) => Some(ScalarValue {
                ty: return_type,
                raw_bits: value.cast(return_integer).raw_bits(),
            }),
            (Flow::Return(None), None) | (Flow::Normal, None) => None,
            (Flow::Normal, Some(return_integer)) if is_main => Some(ScalarValue {
                ty: return_type,
                raw_bits: CInteger::int(0).cast(return_integer).raw_bits(),
            }),
            (Flow::Return(None), Some(_)) => {
                return Err(InterpreterError::UnsupportedStatement {
                    feature: "empty return in scalar function",
                    span: body.span(),
                });
            }
            (Flow::Return(Some(_)), None) => {
                return Err(InterpreterError::UnsupportedStatement {
                    feature: "value return in void function",
                    span: body.span(),
                });
            }
            (Flow::Normal, Some(_)) => {
                return Err(InterpreterError::MissingReturn { span: body.span() });
            }
            (Flow::Break(span), _) => {
                return Err(InterpreterError::InvalidControlFlow {
                    flow: "break outside loop or switch",
                    span,
                });
            }
            (Flow::Continue(span), _) => {
                return Err(InterpreterError::InvalidControlFlow {
                    flow: "continue outside loop",
                    span,
                });
            }
            (Flow::Goto { spelling, span }, _) => {
                return Err(InterpreterError::UnknownLabel { spelling, span });
            }
        };
        Ok(InterpretOutcome {
            return_value,
            steps: self.consumed,
        })
    }

    fn run_with_calls(mut self, function: SymbolId) -> Result<InterpretOutcome, InterpreterError> {
        let mut tasks = Vec::new();
        let mut values = Vec::new();
        let root_span = self
            .unit
            .symbols()
            .symbol(function)
            .map(|symbol| symbol.span())
            .map_err(|_| InterpreterError::UnknownFunction { symbol: function })?;
        self.enter_function(function, Vec::new(), None, None, None, false, 0, &mut tasks)?;

        while let Some(task) = tasks.pop() {
            let outcome = match task {
                MachineTask::FunctionEnd(end) => {
                    self.finish_function(end, FunctionExit::FellOffEnd, &mut values)?
                }
                MachineTask::RestoreSlots(saved) => {
                    self.restore_slots(saved);
                    None
                }
                MachineTask::RestoreFuelAnchor(previous) => {
                    self.fuel_anchor = previous;
                    None
                }
                MachineTask::ExecDeclaration(declaration) => {
                    self.schedule_machine_declaration(declaration, &mut tasks)?;
                    None
                }
                MachineTask::ExecStatement(statement) => {
                    self.exec_machine_statement(statement, &mut tasks, &mut values)?
                }
                MachineTask::ExecSwitchStatement(statement) => {
                    self.schedule_switch_statement(statement, &mut tasks)?;
                    None
                }
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
                MachineTask::SwitchContinue { items, index } => {
                    if let Some(item) = items.get(index) {
                        tasks.push(MachineTask::SwitchContinue {
                            items,
                            index: index + 1,
                        });
                        tasks.push(match item {
                            CHirBlockItem::Declaration(declaration) => {
                                MachineTask::ExecDeclaration(declaration)
                            }
                            CHirBlockItem::Statement(statement) => {
                                MachineTask::ExecSwitchStatement(statement)
                            }
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
                    if pop_machine_value(&mut values, span)?.truth() {
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
                    if pop_machine_value(&mut values, span)?.truth() {
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
                    if pop_machine_value(&mut values, span)?.truth() {
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
                        self.schedule_discarded_expression(step, &mut tasks);
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
                    let value = pop_machine_value(&mut values, span)?.cast(ty);
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
                MachineTask::FinishReturnValue(span) => {
                    let value = pop_machine_value(&mut values, span)?;
                    let end = self.pop_function_end(&mut tasks, span)?;
                    self.finish_function(end, FunctionExit::Explicit(Some(value)), &mut values)?
                }
                MachineTask::SelectIf {
                    then_statement,
                    else_statement,
                } => {
                    let condition = pop_machine_value(&mut values, then_statement.span())?;
                    if condition.truth() {
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
                MachineTask::ApplyUnary {
                    operator,
                    expression_span,
                    operand_span,
                } => {
                    let operand = pop_machine_value(&mut values, expression_span)?;
                    let value = eval_unary_integer(operator, operand).map_err(|error| {
                        let error_span = if operator == crate::UnaryOp::Neg {
                            unary_operator_span(expression_span, operand_span)
                        } else {
                            expression_span
                        };
                        self.integer_error(error, error_span)
                    })?;
                    values.push(value);
                    None
                }
                MachineTask::ContinueBinary {
                    operator,
                    right,
                    span,
                } => {
                    let left = pop_machine_value(&mut values, span)?;
                    match operator {
                        BinaryOp::And if !left.truth() => values.push(CInteger::int(0)),
                        BinaryOp::Or if left.truth() => values.push(CInteger::int(1)),
                        BinaryOp::Comma => {
                            tasks.push(MachineTask::EvalExpression {
                                expression: right,
                                allow_root_mutation: false,
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
                                allow_root_mutation: false,
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
                    let value = match operator {
                        BinaryOp::And => CInteger::int(i32::from(right.truth())),
                        BinaryOp::Or => CInteger::int(i32::from(right.truth())),
                        BinaryOp::Comma => right,
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
                        | BinaryOp::BitXorAssign => {
                            return Err(InterpreterError::UnsupportedExpression {
                                feature: "unlowered assignment binary operator",
                                span,
                            });
                        }
                        _ => eval_binary_integer(operator, left, right)
                            .map_err(|error| self.integer_error(error, span))?,
                    };
                    values.push(value);
                    None
                }
                MachineTask::SelectTernary {
                    then_expression,
                    else_expression,
                    result_type,
                } => {
                    let condition = pop_machine_value(&mut values, then_expression.span())?;
                    tasks.push(MachineTask::CastValue {
                        target: result_type,
                        span: then_expression.span(),
                    });
                    tasks.push(MachineTask::EvalExpression {
                        expression: if condition.truth() {
                            then_expression
                        } else {
                            else_expression
                        },
                        allow_root_mutation: false,
                    });
                    None
                }
                MachineTask::CastValue { target, span } => {
                    let value = pop_machine_value(&mut values, span)?;
                    values.push(value.cast(target));
                    None
                }
                MachineTask::ApplyAssignment {
                    operator,
                    symbol,
                    target_span,
                    span,
                } => {
                    let right = pop_machine_value(&mut values, span)?;
                    values.push(self.apply_assignment_value(
                        operator,
                        symbol,
                        right,
                        target_span,
                        span,
                    )?);
                    None
                }
                MachineTask::SelectSwitch { body, span } => {
                    let controlling = pop_machine_value(&mut values, span)?;
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
                    if index == arguments.len() {
                        self.start_call(
                            callee,
                            arguments_so_far,
                            span,
                            expects_value,
                            &mut tasks,
                            values.len(),
                        )?;
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
            };
            if let Some(outcome) = outcome {
                return Ok(outcome);
            }
        }

        Err(InterpreterError::UnsupportedStatement {
            feature: "interpreter call stack ended without a root result",
            span: root_span,
        })
    }

    fn exec_machine_statement(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
        values: &mut Vec<CInteger>,
    ) -> Result<Option<InterpretOutcome>, InterpreterError> {
        self.tick(statement.span())?;
        match statement {
            CHirStmt::Expr(expression, _) => {
                self.schedule_discarded_expression(expression, tasks);
                Ok(None)
            }
            CHirStmt::Return(Some(expression), span) => {
                tasks.push(MachineTask::FinishReturnValue(*span));
                tasks.push(MachineTask::EvalExpression {
                    expression,
                    allow_root_mutation: true,
                });
                Ok(None)
            }
            CHirStmt::Return(None, span) => {
                let end = self.pop_function_end(tasks, *span)?;
                self.finish_function(end, FunctionExit::Explicit(None), values)
            }
            CHirStmt::Compound(items, _) => {
                let saved = self.activate_block_items(items)?;
                tasks.push(MachineTask::RestoreSlots(saved));
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
                        self.schedule_discarded_expression(expression, tasks);
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
            CHirStmt::Label(_, child, _) => {
                tasks.push(MachineTask::ExecStatement(child));
                Ok(None)
            }
            CHirStmt::Case(_, _, _) => Err(InterpreterError::InvalidControlFlow {
                flow: "case outside switch",
                span: statement.span(),
            }),
            CHirStmt::Default(_, _) => Err(InterpreterError::InvalidControlFlow {
                flow: "default outside switch",
                span: statement.span(),
            }),
            CHirStmt::Null(_) => Ok(None),
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
            CHirDecl::Tag(_) | CHirDecl::Typedef(_) => return Ok(()),
            CHirDecl::Function(declaration) => {
                return Err(InterpreterError::UnsupportedDeclaration {
                    feature: "block function declaration",
                    span: declaration.span,
                });
            }
        };
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
            return Ok(());
        };
        match &initializer.plan().actions()[0] {
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
            InitAction::StoreScalar { span, .. } | InitAction::StoreString { span, .. } => {
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

    fn schedule_switch_statement(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        match statement {
            CHirStmt::Case(_, child, span) | CHirStmt::Default(child, span) => {
                self.tick(*span)?;
                tasks.push(MachineTask::ExecSwitchStatement(child));
            }
            _ => tasks.push(MachineTask::ExecStatement(statement)),
        }
        Ok(())
    }

    fn schedule_switch(
        &mut self,
        controlling: CInteger,
        body: &'arena CHirStmt<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let CHirStmt::Compound(items, _) = body else {
            return Err(InterpreterError::UnsupportedStatement {
                feature: "switch body without a compound statement",
                span: body.span(),
            });
        };
        let saved = self.activate_block_items(items)?;
        tasks.push(MachineTask::RestoreSlots(saved));
        let controlling = controlling.promoted();
        let mut selected = None;
        let mut default = None;
        for (index, item) in items.iter().enumerate() {
            let CHirBlockItem::Statement(statement) = item else {
                continue;
            };
            match statement {
                CHirStmt::Case(value, _, case_span) if selected.is_none() => {
                    let case_value = self.eval_expr(value, false)?.cast(controlling.ty());
                    let equal = eval_binary_integer(BinaryOp::Eq, controlling, case_value)
                        .map_err(|error| self.integer_error(error, *case_span))?;
                    if equal.truth() {
                        selected = Some(index);
                    }
                }
                CHirStmt::Default(_, _) if default.is_none() => default = Some(index),
                _ => {}
            }
        }
        if let Some(index) = selected.or(default) {
            tasks.push(MachineTask::SwitchContinue { items, index });
        }
        Ok(())
    }

    fn pop_function_end(
        &mut self,
        tasks: &mut Vec<MachineTask<'arena>>,
        span: SourceSpan,
    ) -> Result<FunctionEnd, InterpreterError> {
        while let Some(task) = tasks.pop() {
            match task {
                MachineTask::FunctionEnd(end) => return Ok(end),
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
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
                | MachineTask::SwitchContinue { .. } => return Ok(()),
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
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
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
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
        while let Some(task) = tasks.pop() {
            match task {
                MachineTask::CompoundContinue { items, .. } => {
                    if self.schedule_goto_in_active_items(
                        items,
                        spelling,
                        MachineBlockMode::Compound,
                        tasks,
                    )? {
                        return Ok(());
                    }
                }
                MachineTask::SwitchContinue { items, .. } => {
                    if self.schedule_goto_in_active_items(
                        items,
                        spelling,
                        MachineBlockMode::Switch,
                        tasks,
                    )? {
                        return Ok(());
                    }
                }
                MachineTask::RestoreSlots(saved) => self.restore_slots(saved),
                MachineTask::RestoreFuelAnchor(previous) => self.fuel_anchor = previous,
                MachineTask::FunctionEnd(end) => {
                    let body = self.function_definition(end.symbol)?.body.ok_or(
                        InterpreterError::FunctionHasNoBody {
                            symbol: end.symbol,
                            span: end.body_span,
                        },
                    )?;
                    tasks.push(MachineTask::FunctionEnd(end));
                    if self.schedule_goto_in_statement(body, spelling, tasks)? {
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
        spelling: &str,
        mode: MachineBlockMode,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<bool, InterpreterError> {
        for (index, item) in items.iter().enumerate() {
            let CHirBlockItem::Statement(statement) = item else {
                continue;
            };
            if !statement_contains_label(statement, spelling) {
                continue;
            }
            tasks.push(match mode {
                MachineBlockMode::Compound => MachineTask::CompoundContinue {
                    items,
                    index: index + 1,
                },
                MachineBlockMode::Switch => MachineTask::SwitchContinue {
                    items,
                    index: index + 1,
                },
            });
            return self.schedule_goto_in_statement(statement, spelling, tasks);
        }
        Ok(false)
    }

    fn schedule_goto_in_statement(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
        spelling: &str,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<bool, InterpreterError> {
        if !statement_contains_label(statement, spelling) {
            return Ok(false);
        }
        match statement {
            CHirStmt::Label(name, child, _) => {
                if name.spelling == spelling {
                    tasks.push(MachineTask::ExecStatement(statement));
                    Ok(true)
                } else {
                    self.schedule_goto_in_statement(child, spelling, tasks)
                }
            }
            CHirStmt::Compound(items, _) => {
                let saved = self.activate_block_items(items)?;
                tasks.push(MachineTask::RestoreSlots(saved));
                self.schedule_goto_in_active_items(
                    items,
                    spelling,
                    MachineBlockMode::Compound,
                    tasks,
                )
            }
            CHirStmt::If(_, then_statement, else_statement, _) => {
                if statement_contains_label(then_statement, spelling) {
                    self.schedule_goto_in_statement(then_statement, spelling, tasks)
                } else if let Some(else_statement) = else_statement {
                    self.schedule_goto_in_statement(else_statement, spelling, tasks)
                } else {
                    Ok(false)
                }
            }
            CHirStmt::Switch(_, body, _) => {
                let CHirStmt::Compound(items, _) = body else {
                    return Err(InterpreterError::UnsupportedStatement {
                        feature: "switch body without a compound statement",
                        span: body.span(),
                    });
                };
                let saved = self.activate_block_items(items)?;
                tasks.push(MachineTask::RestoreSlots(saved));
                self.schedule_goto_in_active_items(items, spelling, MachineBlockMode::Switch, tasks)
            }
            CHirStmt::While(condition, body, loop_span) => {
                let previous = self.fuel_anchor.replace(*loop_span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::WhileCheck { condition, body });
                self.schedule_goto_in_statement(body, spelling, tasks)
            }
            CHirStmt::DoWhile(body, condition, loop_span) => {
                let previous = self.fuel_anchor.replace(*loop_span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                tasks.push(MachineTask::DoWhileCondition { condition, body });
                self.schedule_goto_in_statement(body, spelling, tasks)
            }
            CHirStmt::For(initializer, condition, step, body, loop_span) => {
                let previous = self.fuel_anchor.replace(*loop_span);
                tasks.push(MachineTask::RestoreFuelAnchor(previous));
                if let CHirForInit::Declaration(declarations) = initializer {
                    let saved = self.activate_for_declarations(declarations)?;
                    tasks.push(MachineTask::RestoreSlots(saved));
                }
                tasks.push(MachineTask::ForStep {
                    condition: *condition,
                    step: *step,
                    body,
                    span: *loop_span,
                });
                self.schedule_goto_in_statement(body, spelling, tasks)
            }
            CHirStmt::Case(_, child, _) | CHirStmt::Default(child, _) => {
                self.schedule_goto_in_statement(child, spelling, tasks)
            }
            CHirStmt::Expr(_, _)
            | CHirStmt::Return(_, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::Null(_) => Ok(false),
        }
    }

    fn call_designator_returns_void(&self, callee: &CHirExpr<'arena>) -> bool {
        let CHirExpr::Ident { facts, .. } = callee else {
            return false;
        };
        let Some(TypeKind::Function { return_type, .. }) =
            self.unit.types().get(facts.ty()).map(|ty| &ty.kind)
        else {
            return false;
        };
        matches!(
            self.unit.types().get(*return_type).map(|ty| &ty.kind),
            Some(TypeKind::Void)
        )
    }

    fn schedule_discarded_expression(
        &self,
        expression: &'arena CHirExpr<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) {
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
                return;
            }
        }
        tasks.push(MachineTask::DiscardExpression(expression.span()));
        tasks.push(MachineTask::EvalExpression {
            expression,
            allow_root_mutation: true,
        });
    }

    fn schedule_expression(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        allow_root_mutation: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
        values: &mut Vec<CInteger>,
    ) -> Result<(), InterpreterError> {
        if !expression_contains_call(expression) {
            values.push(self.eval_expr(expression, allow_root_mutation)?);
            return Ok(());
        }

        let span = expression.span();
        self.tick(span)?;
        match expression {
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
            CHirExpr::Binary(operator, left, right, _) => {
                tasks.push(MachineTask::ContinueBinary {
                    operator: *operator,
                    right,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: left,
                    allow_root_mutation: false,
                });
            }
            CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
                let then_type = self.infer_integer_type(then_expression)?;
                let else_type = self.infer_integer_type(else_expression)?;
                let result_type = usual_arithmetic_type(
                    CInteger::new(0, then_type).promoted().ty(),
                    CInteger::new(0, else_type).promoted().ty(),
                );
                tasks.push(MachineTask::SelectTernary {
                    then_expression,
                    else_expression,
                    result_type,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: condition,
                    allow_root_mutation: false,
                });
            }
            CHirExpr::Call(callee, arguments, _) => {
                tasks.push(MachineTask::EvalCallArguments {
                    callee,
                    arguments,
                    index: 0,
                    values: Vec::new(),
                    span,
                    expects_value: true,
                });
            }
            CHirExpr::Cast(ty, value, _) => {
                let target = self.integer_type(*ty, span)?;
                tasks.push(MachineTask::CastValue { target, span });
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: false,
                });
            }
            CHirExpr::Comma(left, right, _) => {
                tasks.push(MachineTask::ContinueBinary {
                    operator: BinaryOp::Comma,
                    right,
                    span,
                });
                tasks.push(MachineTask::EvalExpression {
                    expression: left,
                    allow_root_mutation: false,
                });
            }
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                facts,
                ..
            } if allow_root_mutation => {
                if facts.ty() != place.ty() || facts.category() != ExprCategory::Value {
                    return Err(InterpreterError::InvalidScalarPlace {
                        symbol: place.symbol(),
                        span,
                    });
                }
                let symbol = self.validate_place(*place, target, span)?;
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
            CHirExpr::Assign { .. } | CHirExpr::Update { .. } => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "function call nested in scalar mutation",
                    span,
                });
            }
            CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _)
            | CHirExpr::CompoundLit(_, _, _) => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "function call nested outside scalar expression subset",
                    span,
                });
            }
            CHirExpr::Lit(_, _)
            | CHirExpr::FloatLit(_, _)
            | CHirExpr::CharLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::Ident { .. }
            | CHirExpr::SizeOf(_, _)
            | CHirExpr::SizeOfExpr(_, _)
            | CHirExpr::AlignOf(_, _) => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "inconsistent call-bearing expression",
                    span,
                });
            }
        }
        Ok(())
    }

    fn start_call(
        &mut self,
        callee: &'arena CHirExpr<'arena>,
        arguments: Vec<CInteger>,
        call_span: SourceSpan,
        expects_value: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
        value_base: usize,
    ) -> Result<(), InterpreterError> {
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
            || declaration.ty() != facts.ty()
            || facts.category() != ExprCategory::FunctionDesignator
        {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "inconsistent function designator",
                span: *span,
            });
        }

        let caller_slots = std::mem::take(&mut self.slots);
        let caller_fuel_anchor = self.fuel_anchor.take();
        self.enter_function(
            *symbol,
            arguments,
            Some(caller_slots),
            caller_fuel_anchor,
            Some(call_span),
            expects_value,
            value_base,
            tasks,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn enter_function(
        &mut self,
        function: SymbolId,
        arguments: Vec<CInteger>,
        caller_slots: Option<HashMap<SymbolId, Slot>>,
        caller_fuel_anchor: Option<SourceSpan>,
        call_span: Option<SourceSpan>,
        caller_expects_value: bool,
        value_base: usize,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let declaration = self.function_definition(function)?;
        let signature = self.validate_function_signature(declaration, arguments.len())?;
        let return_type = signature.return_type;
        let return_integer = signature.return_integer;
        if call_span.is_some() && caller_expects_value && return_integer.is_none() {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "void call in scalar expression",
                span: call_span.unwrap_or(declaration.span),
            });
        }
        if call_span.is_some() && !caller_expects_value && return_integer.is_some() {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "non-void call scheduled as a void expression statement",
                span: call_span.unwrap_or(declaration.span),
            });
        }
        let parameter_bindings = signature
            .parameters
            .into_iter()
            .zip(arguments)
            .map(|((symbol, ty), argument)| (symbol, ty, argument.cast(ty)))
            .collect::<Vec<_>>();

        let body = declaration
            .body
            .ok_or(InterpreterError::FunctionHasNoBody {
                symbol: function,
                span: declaration.span,
            })?;
        let body_span = body.span();
        let is_main = declaration.name.spelling == "main";

        self.slots = HashMap::new();
        for (symbol, ty, value) in parameter_bindings {
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
        validate_function_control_flow(body)?;

        tasks.push(MachineTask::FunctionEnd(FunctionEnd {
            symbol: function,
            return_type,
            return_integer,
            is_main,
            body_span,
            caller_slots,
            caller_fuel_anchor,
            caller_expects_value,
            value_base,
        }));
        tasks.push(MachineTask::ExecStatement(body));
        Ok(())
    }

    fn validate_root_function(&self, function: SymbolId) -> Result<(), InterpreterError> {
        let declaration = self.function_definition(function)?;
        self.validate_function_signature(declaration, 0)?;
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
            || declaration.return_specifier.type_id() != *return_type
            || !self.written_declarator_matches(
                &declaration.written,
                declaration.ty,
                declaration.return_specifier.type_id(),
                Some(&declaration.parameters),
            )
        {
            return Err(InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            });
        }
        let return_integer = match self.unit.types().get(*return_type).map(|ty| &ty.kind) {
            Some(TypeKind::Void) => None,
            _ => Some(self.integer_type(*return_type, declaration.span)?),
        };
        let parameters = match (canonical_parameters, &declaration.parameters) {
            (FunctionParameters::Unspecified, ParameterSyntax::Unspecified)
                if argument_count == 0 =>
            {
                Vec::new()
            }
            (
                FunctionParameters::Prototype {
                    types,
                    variadic: false,
                },
                ParameterSyntax::Prototype {
                    parameters,
                    variadic: false,
                },
            ) if types.len() == parameters.len() && types.len() == argument_count => {
                let mut validated = Vec::with_capacity(parameters.len());
                let mut parameter_ids = BTreeSet::new();
                let mut parameter_spellings = BTreeSet::new();
                let mut function_scope = None;
                for (canonical_type, parameter) in types.iter().zip(parameters) {
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
                    if *canonical_type != parameter.ty
                        || parameter.specifier.type_id() != parameter.ty
                        || !self.written_declarator_matches(
                            &parameter.written,
                            parameter.ty,
                            parameter.specifier.type_id(),
                            None,
                        )
                        || symbol.kind() != SymbolKind::Parameter
                        || symbol.spelling() != parameter_name.spelling
                        || symbol.ty() != parameter.ty
                        || !matches!(
                            symbol.storage_class(),
                            Some(StorageClass::None | StorageClass::Register)
                        )
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
                        parameter_symbol,
                        self.integer_type(parameter.ty, parameter.span)?,
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
            return_integer,
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
        left_return == right_return
            && function_parameters_compatible(self.unit.types(), left_parameters, right_parameters)
    }

    fn written_declarator_matches(
        &self,
        written: &WrittenDeclarator,
        mut current: TypeId,
        terminal: TypeId,
        function_syntax: Option<&ParameterSyntax>,
    ) -> bool {
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
                        && function_syntax
                            .is_some_and(|syntax| !function_parameters_match(parameters, syntax))
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
    }

    fn function_definition(
        &self,
        function: SymbolId,
    ) -> Result<&'arena FunctionDecl<'arena>, InterpreterError> {
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
        end: FunctionEnd,
        exit: FunctionExit,
        values: &mut Vec<CInteger>,
    ) -> Result<Option<InterpretOutcome>, InterpreterError> {
        let return_value = match (exit, end.return_integer) {
            (FunctionExit::Explicit(Some(value)), Some(return_integer)) => {
                Some(value.cast(return_integer))
            }
            (FunctionExit::Explicit(None), None) | (FunctionExit::FellOffEnd, None) => None,
            (FunctionExit::FellOffEnd, Some(return_integer)) if end.is_main => {
                Some(CInteger::int(0).cast(return_integer))
            }
            (FunctionExit::Explicit(None), Some(_)) => {
                return Err(InterpreterError::UnsupportedStatement {
                    feature: "empty return in scalar function",
                    span: end.body_span,
                });
            }
            (FunctionExit::Explicit(Some(_)), None) => {
                return Err(InterpreterError::UnsupportedStatement {
                    feature: "value return in void function",
                    span: end.body_span,
                });
            }
            (FunctionExit::FellOffEnd, Some(_)) => {
                return Err(InterpreterError::MissingReturn {
                    span: end.body_span,
                });
            }
        };

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
            Ok(Some(InterpretOutcome {
                return_value: return_value.map(|value| ScalarValue {
                    ty: end.return_type,
                    raw_bits: value.raw_bits(),
                }),
                steps: self.consumed,
            }))
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
        canonical_integer_type(self.unit.types(), ty)
            .ok_or(InterpreterError::UnsupportedType { ty, span })
    }

    fn integer_error(&self, error: IntegerError, span: SourceSpan) -> InterpreterError {
        InterpreterError::Integer { error, span }
    }

    fn predeclare_stmt(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
    ) -> Result<(), InterpreterError> {
        match statement {
            CHirStmt::If(_, then_statement, else_statement, _) => {
                self.predeclare_stmt(then_statement)?;
                if let Some(else_statement) = else_statement {
                    self.predeclare_stmt(else_statement)?;
                }
            }
            CHirStmt::Switch(_, body, _)
            | CHirStmt::While(_, body, _)
            | CHirStmt::Label(_, body, _)
            | CHirStmt::Case(_, body, _)
            | CHirStmt::Default(body, _) => self.predeclare_stmt(body)?,
            CHirStmt::DoWhile(body, _, _) => self.predeclare_stmt(body)?,
            CHirStmt::For(initializer, _, _, body, _) => {
                if let CHirForInit::Declaration(declarations) = initializer {
                    for declaration in declarations {
                        self.predeclare_decl(declaration)?;
                    }
                }
                self.predeclare_stmt(body)?;
            }
            CHirStmt::Compound(items, _) => {
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
            CHirStmt::Expr(_, _)
            | CHirStmt::Return(_, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::Null(_) => {}
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
                Ok(())
            }
            CHirDecl::Tag(_) | CHirDecl::Typedef(_) => Ok(()),
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
            || plan.actions().len() != 1
        {
            return Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            });
        }
        let valid = match &plan.actions()[0] {
            InitAction::StoreScalar {
                path,
                destination_type,
                ..
            }
            | InitAction::ZeroScalar {
                path,
                destination_type,
            } => path.is_root() && *destination_type == declaration.ty,
            InitAction::StoreString { .. } => false,
        };
        if !valid {
            return Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: plan.actions()[0]
                    .span()
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

    fn exec_decl(&mut self, declaration: &'arena CHirDecl<'arena>) -> Result<(), InterpreterError> {
        self.tick(declaration.span())?;
        match declaration {
            CHirDecl::Object(declaration) => self.exec_object_decl(declaration),
            CHirDecl::Tag(_) | CHirDecl::Typedef(_) => Ok(()),
            CHirDecl::Function(declaration) => Err(InterpreterError::UnsupportedDeclaration {
                feature: "block function declaration",
                span: declaration.span,
            }),
        }
    }

    fn exec_object_decl(
        &mut self,
        declaration: &ObjectDecl<'arena>,
    ) -> Result<(), InterpreterError> {
        let ty = self.validate_object_declaration(declaration)?;
        self.validate_initializer_plan(declaration)?;
        let Some(slot) = self.slots.get_mut(&declaration.symbol) else {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        };
        if slot.ty != ty || !slot.active {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        }
        slot.value = None;
        let Some(initializer) = &declaration.initializer else {
            return Ok(());
        };
        let plan = initializer.plan();
        let value = match &plan.actions()[0] {
            InitAction::StoreScalar {
                path,
                destination_type,
                value,
                span,
            } if path.is_root() && *destination_type == declaration.ty => {
                self.eval_expr(value, true)?.cast(ty)
            }
            InitAction::ZeroScalar {
                path,
                destination_type,
            } if path.is_root() && *destination_type == declaration.ty => CInteger::new(0, ty),
            InitAction::StoreScalar { span, .. } | InitAction::StoreString { span, .. } => {
                return Err(InterpreterError::InvalidInitializerPlan {
                    symbol: declaration.symbol,
                    span: *span,
                });
            }
            InitAction::ZeroScalar { .. } => {
                return Err(InterpreterError::InvalidInitializerPlan {
                    symbol: declaration.symbol,
                    span: initializer.span(),
                });
            }
        };
        self.slots
            .get_mut(&declaration.symbol)
            .expect("slot inserted immediately before initializer execution")
            .value = Some(value);
        Ok(())
    }

    fn exec_stmt(&mut self, statement: &'arena CHirStmt<'arena>) -> Result<Flow, InterpreterError> {
        self.tick(statement.span())?;
        match statement {
            CHirStmt::Expr(expression, _) => {
                self.eval_expr(expression, true)?;
                Ok(Flow::Normal)
            }
            CHirStmt::Return(expression, _) => expression
                .map(|expression| self.eval_expr(expression, true))
                .transpose()
                .map(Flow::Return),
            CHirStmt::Compound(items, _) => self.exec_compound(items),
            CHirStmt::If(condition, then_statement, else_statement, _) => {
                if self.eval_expr(condition, true)?.truth() {
                    self.exec_stmt(then_statement)
                } else if let Some(else_statement) = else_statement {
                    self.exec_stmt(else_statement)
                } else {
                    Ok(Flow::Normal)
                }
            }
            CHirStmt::Switch(expression, body, _) => self.exec_switch(expression, body),
            CHirStmt::While(condition, body, span) => {
                let previous_anchor = self.fuel_anchor.replace(*span);
                let result = self.exec_while(condition, body);
                self.fuel_anchor = previous_anchor;
                result
            }
            CHirStmt::DoWhile(body, condition, span) => {
                let previous_anchor = self.fuel_anchor.replace(*span);
                let result = self.exec_do_while(body, condition);
                self.fuel_anchor = previous_anchor;
                result
            }
            CHirStmt::For(initializer, condition, step, body, span) => {
                let previous_anchor = self.fuel_anchor.replace(*span);
                let result = self.exec_for(initializer, *condition, *step, body);
                self.fuel_anchor = previous_anchor;
                result
            }
            CHirStmt::Break(span) => Ok(Flow::Break(*span)),
            CHirStmt::Continue(span) => Ok(Flow::Continue(*span)),
            CHirStmt::Null(_) => Ok(Flow::Normal),
            CHirStmt::Goto(name, span) => Ok(Flow::Goto {
                spelling: name.spelling.clone(),
                span: *span,
            }),
            CHirStmt::Label(_, child, _) => self.exec_stmt(child),
            CHirStmt::Case(_, _, _) => Err(InterpreterError::InvalidControlFlow {
                flow: "case outside switch",
                span: statement.span(),
            }),
            CHirStmt::Default(_, _) => Err(InterpreterError::InvalidControlFlow {
                flow: "default outside switch",
                span: statement.span(),
            }),
        }
    }

    fn exec_compound(&mut self, items: &[CHirBlockItem<'arena>]) -> Result<Flow, InterpreterError> {
        let saved = self.activate_block_items(items)?;
        let result = self.exec_compound_active(items);
        self.restore_slots(saved);
        result
    }

    fn exec_compound_active(
        &mut self,
        items: &[CHirBlockItem<'arena>],
    ) -> Result<Flow, InterpreterError> {
        let labels = collect_labels(items)?;
        let mut program_counter = 0_usize;
        while program_counter < items.len() {
            let flow = match &items[program_counter] {
                CHirBlockItem::Declaration(declaration) => {
                    self.exec_decl(declaration)?;
                    Flow::Normal
                }
                CHirBlockItem::Statement(statement) => self.exec_stmt(statement)?,
            };
            match flow {
                Flow::Normal => program_counter += 1,
                Flow::Goto { spelling, span } => {
                    let Some(target) = labels.get(&spelling) else {
                        return Ok(Flow::Goto { spelling, span });
                    };
                    program_counter = *target;
                }
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_while(
        &mut self,
        condition: &'arena CHirExpr<'arena>,
        body: &'arena CHirStmt<'arena>,
    ) -> Result<Flow, InterpreterError> {
        while self.eval_expr(condition, true)?.truth() {
            match self.exec_stmt(body)? {
                Flow::Normal | Flow::Continue(_) => {}
                Flow::Break(_) => return Ok(Flow::Normal),
                flow @ (Flow::Return(_) | Flow::Goto { .. }) => return Ok(flow),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_do_while(
        &mut self,
        body: &'arena CHirStmt<'arena>,
        condition: &'arena CHirExpr<'arena>,
    ) -> Result<Flow, InterpreterError> {
        loop {
            match self.exec_stmt(body)? {
                Flow::Normal | Flow::Continue(_) => {}
                Flow::Break(_) => return Ok(Flow::Normal),
                flow @ (Flow::Return(_) | Flow::Goto { .. }) => return Ok(flow),
            }
            if !self.eval_expr(condition, true)?.truth() {
                return Ok(Flow::Normal);
            }
        }
    }

    fn exec_switch(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        body: &'arena CHirStmt<'arena>,
    ) -> Result<Flow, InterpreterError> {
        let controlling = self.eval_expr(expression, true)?;
        let CHirStmt::Compound(items, _) = body else {
            return Err(InterpreterError::UnsupportedStatement {
                feature: "switch body without a compound statement",
                span: body.span(),
            });
        };

        let saved = self.activate_block_items(items)?;
        let result = self.exec_switch_active(controlling, items);
        self.restore_slots(saved);
        result
    }

    fn exec_switch_active(
        &mut self,
        controlling: CInteger,
        items: &[CHirBlockItem<'arena>],
    ) -> Result<Flow, InterpreterError> {
        let controlling = controlling.promoted();
        let mut selected = None;
        let mut default = None;
        for (index, item) in items.iter().enumerate() {
            let CHirBlockItem::Statement(statement) = item else {
                continue;
            };
            match statement {
                CHirStmt::Case(value, _, case_span) if selected.is_none() => {
                    let case_value = self.eval_expr(value, false)?.cast(controlling.ty());
                    let equal = eval_binary_integer(BinaryOp::Eq, controlling, case_value)
                        .map_err(|error| self.integer_error(error, *case_span))?;
                    if equal.truth() {
                        selected = Some(index);
                    }
                }
                CHirStmt::Default(_, _) if default.is_none() => default = Some(index),
                _ => {}
            }
        }
        let Some(mut program_counter) = selected.or(default) else {
            return Ok(Flow::Normal);
        };
        let labels = collect_labels(items)?;
        while program_counter < items.len() {
            let flow = match &items[program_counter] {
                CHirBlockItem::Declaration(declaration) => {
                    self.exec_decl(declaration)?;
                    Flow::Normal
                }
                CHirBlockItem::Statement(statement) => self.exec_switch_item(statement)?,
            };
            match flow {
                Flow::Normal => program_counter += 1,
                Flow::Break(_) => return Ok(Flow::Normal),
                Flow::Goto { spelling, span } => {
                    let Some(target) = labels.get(&spelling) else {
                        return Ok(Flow::Goto { spelling, span });
                    };
                    program_counter = *target;
                }
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_switch_item(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
    ) -> Result<Flow, InterpreterError> {
        match statement {
            CHirStmt::Case(_, child, span) | CHirStmt::Default(child, span) => {
                self.tick(*span)?;
                self.exec_switch_item(child)
            }
            _ => self.exec_stmt(statement),
        }
    }

    fn exec_for(
        &mut self,
        initializer: &CHirForInit<'arena>,
        condition: Option<&'arena CHirExpr<'arena>>,
        step: Option<&'arena CHirExpr<'arena>>,
        body: &'arena CHirStmt<'arena>,
    ) -> Result<Flow, InterpreterError> {
        let saved = match initializer {
            CHirForInit::Declaration(declarations) => {
                self.activate_for_declarations(declarations)?
            }
            CHirForInit::Empty | CHirForInit::Expression(_) => Vec::new(),
        };
        let result = self.exec_for_active(initializer, condition, step, body);
        self.restore_slots(saved);
        result
    }

    fn exec_for_active(
        &mut self,
        initializer: &CHirForInit<'arena>,
        condition: Option<&'arena CHirExpr<'arena>>,
        step: Option<&'arena CHirExpr<'arena>>,
        body: &'arena CHirStmt<'arena>,
    ) -> Result<Flow, InterpreterError> {
        match initializer {
            CHirForInit::Empty => {}
            CHirForInit::Expression(expression) => {
                self.eval_expr(expression, true)?;
            }
            CHirForInit::Declaration(declarations) => {
                for declaration in declarations {
                    self.exec_decl(declaration)?;
                }
            }
        }
        loop {
            if let Some(condition) = condition {
                if !self.eval_expr(condition, true)?.truth() {
                    return Ok(Flow::Normal);
                }
            }
            match self.exec_stmt(body)? {
                Flow::Normal | Flow::Continue(_) => {}
                Flow::Break(_) => return Ok(Flow::Normal),
                flow @ (Flow::Return(_) | Flow::Goto { .. }) => return Ok(flow),
            }
            if let Some(step) = step {
                self.eval_expr(step, true)?;
            }
        }
    }

    fn eval_expr(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        allow_root_mutation: bool,
    ) -> Result<CInteger, InterpreterError> {
        let span = expression.span();
        self.tick(span)?;
        match expression {
            CHirExpr::Lit(literal, _) => {
                integer_literal_value(*literal).map_err(|error| self.integer_error(error, span))
            }
            CHirExpr::Ident {
                symbol,
                spelling,
                facts,
                span,
            } => {
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
                eval_unary_integer(*operator, operand).map_err(|error| {
                    let error_span = if *operator == crate::UnaryOp::Neg {
                        unary_operator_span(span, expression_operand_span(expression))
                    } else {
                        span
                    };
                    self.integer_error(error, error_span)
                })
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
                let selected = if self.eval_expr(condition, false)?.truth() {
                    self.eval_expr(then_expression, false)?
                } else {
                    self.eval_expr(else_expression, false)?
                };
                Ok(selected.cast(result_type))
            }
            CHirExpr::CharLit(literal, _) => {
                Ok(CInteger::new(u64::from(literal.value), CIntegerType::INT))
            }
            CHirExpr::Cast(ty, value, _) => {
                let target = self.integer_type(*ty, span)?;
                Ok(self.eval_expr(value, false)?.cast(target))
            }
            CHirExpr::SizeOf(ty, _) => {
                let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
                    .layout_of(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "sizeof incomplete or unsupported type",
                        span,
                    })?;
                Ok(CInteger::new(layout.size_bytes, CIntegerType::ULONG))
            }
            CHirExpr::SizeOfExpr(operand, _) => {
                let operand_type = self.infer_integer_type(operand)?;
                Ok(CInteger::new(
                    integer_storage_size(operand_type),
                    CIntegerType::ULONG,
                ))
            }
            CHirExpr::AlignOf(ty, _) => {
                let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
                    .layout_of(self.unit.types(), *ty)
                    .map_err(|_| InterpreterError::UnsupportedExpression {
                        feature: "alignof incomplete or unsupported type",
                        span,
                    })?;
                Ok(CInteger::new(
                    u64::from(layout.align_bytes),
                    CIntegerType::ULONG,
                ))
            }
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                facts,
                ..
            } if allow_root_mutation => {
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
            } if allow_root_mutation => {
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
            | CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _)
            | CHirExpr::CompoundLit(_, _, _)
            | CHirExpr::Assign { .. }
            | CHirExpr::Update { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "outside scalar Day-5 expression subset",
                span,
            }),
        }
    }

    fn infer_integer_type(
        &self,
        expression: &CHirExpr<'arena>,
    ) -> Result<CIntegerType, InterpreterError> {
        let span = expression.span();
        match expression {
            CHirExpr::Lit(literal, _) => integer_literal_value(*literal)
                .map(|value| value.ty())
                .map_err(|error| self.integer_error(error, span)),
            CHirExpr::CharLit(_, _) => Ok(CIntegerType::INT),
            CHirExpr::Ident { facts, .. }
            | CHirExpr::Assign { facts, .. }
            | CHirExpr::Update { facts, .. } => self.integer_type(facts.ty(), span),
            CHirExpr::Unary(operator, operand, _) => match operator {
                crate::UnaryOp::Not => Ok(CIntegerType::INT),
                crate::UnaryOp::Plus | crate::UnaryOp::Neg | crate::UnaryOp::BitNot => {
                    Ok(CInteger::new(0, self.infer_integer_type(operand)?)
                        .promoted()
                        .ty())
                }
                crate::UnaryOp::AddrOf | crate::UnaryOp::Deref => {
                    Err(InterpreterError::UnsupportedExpression {
                        feature: "non-integer unary operand",
                        span,
                    })
                }
            },
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
                    Ok(CInteger::new(0, self.infer_integer_type(left)?)
                        .promoted()
                        .ty())
                }
                BinaryOp::Comma => self.infer_integer_type(right),
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Mod
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor => {
                    let left = CInteger::new(0, self.infer_integer_type(left)?)
                        .promoted()
                        .ty();
                    let right = CInteger::new(0, self.infer_integer_type(right)?)
                        .promoted()
                        .ty();
                    Ok(usual_arithmetic_type(left, right))
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
            CHirExpr::Ternary(_, then_expression, else_expression, _) => {
                let then_type = CInteger::new(0, self.infer_integer_type(then_expression)?)
                    .promoted()
                    .ty();
                let else_type = CInteger::new(0, self.infer_integer_type(else_expression)?)
                    .promoted()
                    .ty();
                Ok(usual_arithmetic_type(then_type, else_type))
            }
            CHirExpr::Cast(ty, _, _) => self.integer_type(*ty, span),
            CHirExpr::Call(callee, _, _) => {
                let CHirExpr::Ident { facts, .. } = callee else {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "indirect scalar function call",
                        span: callee.span(),
                    });
                };
                let Some(TypeKind::Function { return_type, .. }) =
                    self.unit.types().get(facts.ty()).map(|ty| &ty.kind)
                else {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "inconsistent function designator",
                        span: callee.span(),
                    });
                };
                self.integer_type(*return_type, span)
            }
            CHirExpr::SizeOf(_, _) | CHirExpr::SizeOfExpr(_, _) | CHirExpr::AlignOf(_, _) => {
                Ok(CIntegerType::ULONG)
            }
            CHirExpr::Comma(_, right, _) => self.infer_integer_type(right),
            CHirExpr::FloatLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _)
            | CHirExpr::CompoundLit(_, _, _) => Err(InterpreterError::UnsupportedExpression {
                feature: "non-integer expression type",
                span,
            }),
        }
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
                let left = self.eval_expr(left, false)?;
                if !left.truth() {
                    Ok(CInteger::int(0))
                } else {
                    Ok(CInteger::int(i32::from(
                        self.eval_expr(right, false)?.truth(),
                    )))
                }
            }
            BinaryOp::Or => {
                let left = self.eval_expr(left, false)?;
                if left.truth() {
                    Ok(CInteger::int(1))
                } else {
                    Ok(CInteger::int(i32::from(
                        self.eval_expr(right, false)?.truth(),
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
                eval_binary_integer(operator, left, right)
                    .map_err(|error| self.integer_error(error, span))
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
            eval_binary_integer(binary, left, right)
                .map_err(|error| self.integer_error(error, span))?
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
        let updated = eval_binary_integer(binary, previous, CInteger::int(1))
            .map_err(|error| self.integer_error(error, span))?
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

fn function_has_call(unit: &CHirUnit<'_>, function: SymbolId) -> bool {
    unit.iter().any(|declaration| {
        matches!(
            declaration,
            CHirDecl::Function(FunctionDecl {
                symbol,
                body: Some(body),
                ..
            }) if *symbol == function && statement_contains_call(body)
        )
    })
}

fn function_has_goto(unit: &CHirUnit<'_>, function: SymbolId) -> bool {
    unit.iter().any(|declaration| {
        matches!(
            declaration,
            CHirDecl::Function(FunctionDecl {
                symbol,
                body: Some(body),
                ..
            }) if *symbol == function && statement_contains_goto(body)
        )
    })
}

fn statement_contains_goto(statement: &CHirStmt<'_>) -> bool {
    match statement {
        CHirStmt::Goto(_, _) => true,
        CHirStmt::If(_, then_statement, else_statement, _) => {
            statement_contains_goto(then_statement)
                || else_statement.is_some_and(statement_contains_goto)
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _) => statement_contains_goto(body),
        CHirStmt::Compound(items, _) => items.iter().any(|item| match item {
            CHirBlockItem::Declaration(_) => false,
            CHirBlockItem::Statement(statement) => statement_contains_goto(statement),
        }),
        CHirStmt::Label(_, child, _)
        | CHirStmt::Case(_, child, _)
        | CHirStmt::Default(child, _) => statement_contains_goto(child),
        CHirStmt::Expr(_, _)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Null(_) => false,
    }
}

fn statement_contains_call(statement: &CHirStmt<'_>) -> bool {
    match statement {
        CHirStmt::Expr(expression, _) => expression_contains_call(expression),
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            expression_contains_call(condition)
                || statement_contains_call(then_statement)
                || else_statement.is_some_and(statement_contains_call)
        }
        CHirStmt::Switch(expression, body, _) | CHirStmt::While(expression, body, _) => {
            expression_contains_call(expression) || statement_contains_call(body)
        }
        CHirStmt::DoWhile(body, expression, _) => {
            statement_contains_call(body) || expression_contains_call(expression)
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            let initializer_has_call = match initializer {
                CHirForInit::Empty => false,
                CHirForInit::Expression(expression) => expression_contains_call(expression),
                CHirForInit::Declaration(declarations) => declarations
                    .iter()
                    .any(|declaration| declaration_contains_call(declaration)),
            };
            initializer_has_call
                || condition.is_some_and(expression_contains_call)
                || step.is_some_and(expression_contains_call)
                || statement_contains_call(body)
        }
        CHirStmt::Return(expression, _) => expression.is_some_and(expression_contains_call),
        CHirStmt::Compound(items, _) => items.iter().any(|item| match item {
            CHirBlockItem::Declaration(declaration) => declaration_contains_call(declaration),
            CHirBlockItem::Statement(statement) => statement_contains_call(statement),
        }),
        CHirStmt::Label(_, child, _)
        | CHirStmt::Case(_, child, _)
        | CHirStmt::Default(child, _) => statement_contains_call(child),
        CHirStmt::Break(_) | CHirStmt::Continue(_) | CHirStmt::Goto(_, _) | CHirStmt::Null(_) => {
            false
        }
    }
}

fn declaration_contains_call(declaration: &CHirDecl<'_>) -> bool {
    let CHirDecl::Object(declaration) = declaration else {
        return false;
    };
    declaration.initializer.as_ref().is_some_and(|initializer| {
        initializer
            .plan()
            .actions()
            .iter()
            .any(|action| match action {
                InitAction::StoreScalar { value, .. } => expression_contains_call(value),
                InitAction::ZeroScalar { .. } | InitAction::StoreString { .. } => false,
            })
    })
}

fn expression_contains_call(expression: &CHirExpr<'_>) -> bool {
    match expression {
        CHirExpr::Call(_, _, _) => true,
        CHirExpr::Unary(_, operand, _) | CHirExpr::Cast(_, operand, _) => {
            expression_contains_call(operand)
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _) => {
            expression_contains_call(left) || expression_contains_call(right)
        }
        CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
            expression_contains_call(condition)
                || expression_contains_call(then_expression)
                || expression_contains_call(else_expression)
        }
        CHirExpr::Member(base, _, _) | CHirExpr::PtrMember(base, _, _) => {
            expression_contains_call(base)
        }
        CHirExpr::Assign { target, value, .. } => {
            expression_contains_call(target) || expression_contains_call(value)
        }
        CHirExpr::Update { target, .. } => expression_contains_call(target),
        CHirExpr::CompoundLit(_, values, _) => {
            values.iter().any(|value| expression_contains_call(value))
        }
        CHirExpr::Lit(_, _)
        | CHirExpr::FloatLit(_, _)
        | CHirExpr::CharLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::Ident { .. }
        | CHirExpr::SizeOf(_, _)
        | CHirExpr::SizeOfExpr(_, _)
        | CHirExpr::AlignOf(_, _) => false,
    }
}

fn pop_machine_value(
    values: &mut Vec<CInteger>,
    span: SourceSpan,
) -> Result<CInteger, InterpreterError> {
    values.pop().ok_or(InterpreterError::UnsupportedExpression {
        feature: "malformed interpreter expression continuation",
        span,
    })
}

fn assignment_binary(operator: AssignmentOp) -> Option<BinaryOp> {
    match operator {
        AssignmentOp::Simple => None,
        AssignmentOp::Add => Some(BinaryOp::Add),
        AssignmentOp::Subtract => Some(BinaryOp::Sub),
        AssignmentOp::Multiply => Some(BinaryOp::Mul),
        AssignmentOp::Divide => Some(BinaryOp::Div),
        AssignmentOp::Remainder => Some(BinaryOp::Mod),
        AssignmentOp::ShiftLeft => Some(BinaryOp::Shl),
        AssignmentOp::ShiftRight => Some(BinaryOp::Shr),
        AssignmentOp::BitAnd => Some(BinaryOp::BitAnd),
        AssignmentOp::BitOr => Some(BinaryOp::BitOr),
        AssignmentOp::BitXor => Some(BinaryOp::BitXor),
    }
}

fn function_parameters_match(canonical: &FunctionParameters, written: &ParameterSyntax) -> bool {
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
                && types
                    .iter()
                    .zip(parameters)
                    .all(|(canonical, parameter)| *canonical == parameter.ty)
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
        ) => left_variadic == right_variadic && left_types == right_types,
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
        TypeKind::Void | TypeKind::Array { .. } | TypeKind::Function { .. } => false,
        TypeKind::BuiltinVaList
        | TypeKind::GnuInt128(_)
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
        | StorageClass::ThreadLocal => false,
    }
}

fn canonical_integer_type(types: &TypeInterner, ty: TypeId) -> Option<CIntegerType> {
    let canonical = types.get(ty)?;
    match canonical.kind {
        TypeKind::Int(rank) => Some(integer_type_for_rank(rank)),
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
            Some(enum_integer_type(minimum, maximum))
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

fn statement_contains_label(statement: &CHirStmt<'_>, spelling: &str) -> bool {
    match statement {
        CHirStmt::Label(name, child, _) => {
            name.spelling == spelling || statement_contains_label(child, spelling)
        }
        CHirStmt::If(_, then_statement, else_statement, _) => {
            statement_contains_label(then_statement, spelling)
                || else_statement
                    .is_some_and(|statement| statement_contains_label(statement, spelling))
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::Default(body, _) => statement_contains_label(body, spelling),
        CHirStmt::DoWhile(body, _, _) => statement_contains_label(body, spelling),
        CHirStmt::Compound(items, _) => items.iter().any(|item| {
            matches!(
                item,
                CHirBlockItem::Statement(statement)
                    if statement_contains_label(statement, spelling)
            )
        }),
        CHirStmt::Expr(_, _)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::Null(_) => false,
    }
}

fn collect_labels(
    items: &[CHirBlockItem<'_>],
) -> Result<BTreeMap<String, usize>, InterpreterError> {
    let mut labels = BTreeMap::new();
    for (index, item) in items.iter().enumerate() {
        let CHirBlockItem::Statement(mut statement) = item else {
            continue;
        };
        while let CHirStmt::Label(name, child, span) = statement {
            if labels.insert(name.spelling.clone(), index).is_some() {
                return Err(InterpreterError::DuplicateLabel {
                    spelling: name.spelling.clone(),
                    span: *span,
                });
            }
            statement = child;
        }
    }
    Ok(labels)
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
        | CHirStmt::Default(body, _) => validate_function_labels(body, labels)?,
        CHirStmt::Compound(items, _) => {
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
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
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
        | CHirStmt::Default(body, _)
        | CHirStmt::Label(_, body, _) => validate_function_gotos(body, labels)?,
        CHirStmt::Compound(items, _) => {
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
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::Null(_) => {}
    }
    Ok(())
}
