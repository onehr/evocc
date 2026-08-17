//! Deterministic, fuel-bounded execution for the Day-5 scalar C-HIR subset.
//!
//! This interpreter deliberately consumes typed C-HIR and its normalized
//! initializer plans. It does not reuse the parser's constant-expression
//! evaluator: every accepted operation goes through the shared target integer
//! kernel and every operation outside the scalar contract fails closed.

use std::{
    collections::{BTreeMap, HashMap},
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
    ExprCategory, InitAction, ObjectDecl, ScalarPlace, StorageClass, SymbolId, SymbolKind,
    UpdateOp,
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

/// Interpret one zero-parameter scalar C-HIR function under a deterministic
/// target-fixed budget.
pub fn interpret_scalar_function(
    unit: &CHirUnit<'_>,
    function: SymbolId,
    limits: InterpreterLimits,
) -> Result<InterpretOutcome, InterpreterError> {
    Interpreter::new(unit, limits).run(function)
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

        let mut labels = BTreeMap::new();
        validate_function_labels(body, &mut labels)?;
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
        let mut selected = None;
        let mut default = None;
        for (index, item) in items.iter().enumerate() {
            let CHirBlockItem::Statement(statement) = item else {
                continue;
            };
            match statement {
                CHirStmt::Case(value, _, case_span) if selected.is_none() => {
                    let case_value = self.eval_expr(value, false)?;
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
            CHirExpr::SizeOf(_, _) | CHirExpr::SizeOfExpr(_, _) | CHirExpr::AlignOf(_, _) => {
                Ok(CIntegerType::ULONG)
            }
            CHirExpr::Comma(_, right, _) => self.infer_integer_type(right),
            CHirExpr::FloatLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::Call(_, _, _)
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
            || declaration.kind() != SymbolKind::Object
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
                    span: target.span(),
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
