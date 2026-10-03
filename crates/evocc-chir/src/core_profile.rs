//! Conservative source inventory for the first compilation-proof pilot.
//!
//! This is NOT a proof checker. It neither transforms C-HIR nor grants an
//! output a verification label. The Lean checker must independently establish
//! well-formedness of the exported source and its relation to actual output.

use std::collections::{BTreeMap, BTreeSet};

use evocc_common::SourceSpan;
use evocc_types::{TargetDataLayout, TypeId, TypeKind};

use crate::initializer::InitAction;
use crate::integer::CIntegerType;
use crate::{
    AssignmentOp, BinaryOp, CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, CHirUnit,
    FunctionDecl, ParameterSyntax, StorageClass, SymbolId, UnaryOp,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreRefusal {
    Profile,
    MissingEntry,
    Signature,
    Attributes,
    Type,
    Statement,
    Expression,
    SignedLeftShift,
    NonlocalObject,
    Uninitialized,
    Initializer,
    ExternalCall,
    Recursion,
    Fallthrough,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoreProfileError {
    pub reason: CoreRefusal,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreFunctionInventory {
    pub symbol: SymbolId,
    pub name: String,
    pub parameters: usize,
    pub local_objects: usize,
    pub loops: usize,
    pub direct_calls: BTreeSet<SymbolId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreInventory {
    /// Callees precede callers. No unrelated function is implicitly admitted.
    pub functions: Vec<CoreFunctionInventory>,
}

type Result<T> = std::result::Result<T, CoreProfileError>;

fn error(reason: CoreRefusal, span: SourceSpan) -> CoreProfileError {
    CoreProfileError {
        reason,
        span: Some(span),
    }
}

/// Inventory selected entry functions and their entire direct-call closure.
/// Unsupported constructs are visited even in dead branches; there is no
/// semantic normalization, constant folding, or projection to a smaller body.
pub fn inspect_core(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    entries: &[&str],
) -> Result<CoreInventory> {
    if *target != TargetDataLayout::aarch64_unknown_linux_gnu()
        || unit.signed_wrapping()
        || unit.runtime_floating_environment()
        || unit.stack_protector()
        || !unit.profile_refusals().is_empty()
    {
        return Err(CoreProfileError {
            reason: CoreRefusal::Profile,
            span: None,
        });
    }
    let mut walker = Closure {
        unit,
        declarations: BTreeMap::new(),
        active: BTreeSet::new(),
        done: BTreeSet::new(),
        functions: Vec::new(),
    };
    for declaration in unit.declarations() {
        if let CHirDecl::Function(function) = declaration {
            walker
                .declarations
                .entry(function.symbol)
                .or_default()
                .push(function);
        }
    }
    if entries.is_empty() {
        return Err(CoreProfileError {
            reason: CoreRefusal::MissingEntry,
            span: None,
        });
    }
    for entry in entries {
        let symbol = walker
            .declarations
            .iter()
            .find_map(|(symbol, chain)| {
                chain
                    .iter()
                    .any(|f| f.name.spelling == *entry)
                    .then_some(*symbol)
            })
            .ok_or(CoreProfileError {
                reason: CoreRefusal::MissingEntry,
                span: None,
            })?;
        walker.function(symbol)?;
    }
    Ok(CoreInventory {
        functions: walker.functions,
    })
}

struct Closure<'u, 'a> {
    unit: &'u CHirUnit<'a>,
    declarations: BTreeMap<SymbolId, Vec<&'u FunctionDecl<'a>>>,
    active: BTreeSet<SymbolId>,
    done: BTreeSet<SymbolId>,
    functions: Vec<CoreFunctionInventory>,
}

impl Closure<'_, '_> {
    fn function(&mut self, symbol: SymbolId) -> Result<()> {
        if self.done.contains(&symbol) {
            return Ok(());
        }
        if !self.active.insert(symbol) {
            return Err(CoreProfileError {
                reason: CoreRefusal::Recursion,
                span: None,
            });
        }
        let chain = self.declarations.get(&symbol).ok_or(CoreProfileError {
            reason: CoreRefusal::ExternalCall,
            span: None,
        })?;
        for declaration in chain {
            if !declaration.attributes.is_empty()
                || declaration.asm_label.is_some()
                || !declaration.inline_spans.is_empty()
            {
                return Err(error(CoreRefusal::Attributes, declaration.span));
            }
        }
        let definitions = chain
            .iter()
            .filter(|f| f.body.is_some())
            .collect::<Vec<_>>();
        if definitions.len() != 1 {
            return Err(CoreProfileError {
                reason: CoreRefusal::ExternalCall,
                span: None,
            });
        }
        let definition = definitions[0];
        let ParameterSyntax::Prototype {
            parameters,
            variadic: false,
        } = &definition.parameters
        else {
            return Err(error(CoreRefusal::Signature, definition.span));
        };
        let Some(TypeKind::Function { return_type, .. }) =
            self.unit.types().get(definition.ty).map(|t| &t.kind)
        else {
            return Err(error(CoreRefusal::Signature, definition.span));
        };
        let mut body = Body {
            unit: self.unit,
            locals: BTreeMap::new(),
            initialized: BTreeSet::new(),
            loop_depth: 0,
            inventory: CoreFunctionInventory {
                symbol,
                name: definition.name.spelling.clone(),
                parameters: parameters.len(),
                local_objects: 0,
                loops: 0,
                direct_calls: BTreeSet::new(),
            },
        };
        body.ty(*return_type, definition.span)?;
        for parameter in parameters {
            if !parameter.attributes.is_empty() {
                return Err(error(CoreRefusal::Attributes, parameter.span));
            }
            body.ty(parameter.ty, parameter.span)?;
            let symbol = parameter
                .symbol
                .ok_or(error(CoreRefusal::Signature, parameter.span))?;
            body.locals.insert(symbol, parameter.ty);
            body.initialized.insert(symbol);
        }
        if body.statement(definition.body.expect("one definition"))? {
            return Err(error(CoreRefusal::Fallthrough, definition.span));
        }
        for callee in &body.inventory.direct_calls {
            self.function(*callee)?;
        }
        self.active.remove(&symbol);
        self.done.insert(symbol);
        self.functions.push(body.inventory);
        Ok(())
    }
}

struct Body<'u, 'a> {
    unit: &'u CHirUnit<'a>,
    locals: BTreeMap<SymbolId, TypeId>,
    initialized: BTreeSet<SymbolId>,
    loop_depth: usize,
    inventory: CoreFunctionInventory,
}

impl Body<'_, '_> {
    fn ty(&self, ty: TypeId, span: SourceSpan) -> Result<CIntegerType> {
        let Some(canonical) = self.unit.types().get(ty) else {
            return Err(error(CoreRefusal::Type, span));
        };
        if canonical.qualifiers.is_volatile
            || canonical.qualifiers.is_atomic
            || canonical.qualifiers.aligned.is_some()
        {
            return Err(error(CoreRefusal::Type, span));
        }
        let TypeKind::Int(rank) = canonical.kind else {
            return Err(error(CoreRefusal::Type, span));
        };
        let integer = crate::integer::integer_type_for_rank(rank);
        if !matches!(integer.bits(), 32 | 64) {
            return Err(error(CoreRefusal::Type, span));
        }
        Ok(integer)
    }

    fn local(&self, symbol: SymbolId, span: SourceSpan, read: bool) -> Result<()> {
        if !self.locals.contains_key(&symbol) {
            return Err(error(CoreRefusal::NonlocalObject, span));
        }
        if read && !self.initialized.contains(&symbol) {
            return Err(error(CoreRefusal::Uninitialized, span));
        }
        Ok(())
    }

    fn expression(&mut self, expression: &CHirExpr<'_>) -> Result<()> {
        let span = expression.span();
        match expression {
            CHirExpr::Lit(..) => {}
            CHirExpr::Ident { symbol, facts, .. } => {
                self.local(*symbol, span, true)?;
                self.ty(facts.ty(), span)?;
            }
            CHirExpr::Unary(
                UnaryOp::Plus | UnaryOp::Neg | UnaryOp::BitNot | UnaryOp::Not,
                operand,
                _,
            ) => self.expression(operand)?,
            CHirExpr::Binary(operator, left, right, _) => {
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
                    return Err(error(CoreRefusal::Expression, span));
                }
                if *operator == BinaryOp::Shl && self.integer_type(left)?.is_signed() {
                    return Err(error(CoreRefusal::SignedLeftShift, span));
                }
                self.expression(left)?;
                let before_right = self.initialized.clone();
                self.expression(right)?;
                if matches!(operator, BinaryOp::And | BinaryOp::Or) {
                    self.initialized = before_right;
                }
            }
            CHirExpr::Ternary(condition, left, right, _) => {
                self.expression(condition)?;
                let incoming = self.initialized.clone();
                self.expression(left)?;
                let left_out = self.initialized.clone();
                self.initialized = incoming;
                self.expression(right)?;
                self.initialized.retain(|s| left_out.contains(s));
            }
            CHirExpr::Cast(ty, value, _) => {
                self.ty(*ty, span)?;
                self.expression(value)?;
            }
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                ..
            } => {
                let CHirExpr::Ident { symbol, .. } = target else {
                    return Err(error(CoreRefusal::Expression, span));
                };
                if *symbol != place.symbol() {
                    return Err(error(CoreRefusal::Expression, span));
                }
                self.local(*symbol, span, *op != AssignmentOp::Simple)?;
                let ty = self.ty(place.ty(), span)?;
                if *op == AssignmentOp::ShiftLeft && ty.is_signed() {
                    return Err(error(CoreRefusal::SignedLeftShift, span));
                }
                self.expression(value)?;
                self.initialized.insert(*symbol);
            }
            CHirExpr::Update { place, target, .. } => {
                let CHirExpr::Ident { symbol, .. } = target else {
                    return Err(error(CoreRefusal::Expression, span));
                };
                if *symbol != place.symbol() {
                    return Err(error(CoreRefusal::Expression, span));
                }
                self.ty(place.ty(), span)?;
                self.local(*symbol, span, true)?;
            }
            CHirExpr::Comma(left, right, _) => {
                self.expression(left)?;
                self.expression(right)?;
            }
            CHirExpr::Call(callee, arguments, _) => {
                let CHirExpr::Ident { symbol, .. } = callee else {
                    return Err(error(CoreRefusal::ExternalCall, span));
                };
                if self.locals.contains_key(symbol) {
                    return Err(error(CoreRefusal::ExternalCall, span));
                }
                for argument in arguments {
                    self.expression(argument)?;
                }
                self.inventory.direct_calls.insert(*symbol);
            }
            _ => return Err(error(CoreRefusal::Expression, span)),
        }
        self.integer_type(expression)?;
        Ok(())
    }

    fn integer_type(&self, expression: &CHirExpr<'_>) -> Result<CIntegerType> {
        crate::constant::integer_expression_type(expression, self.unit.types(), self.unit.symbols())
            .filter(|ty| matches!(ty.bits(), 32 | 64))
            .ok_or(error(CoreRefusal::Type, expression.span()))
    }

    fn declaration(&mut self, declaration: &CHirDecl<'_>) -> Result<()> {
        let CHirDecl::Object(object) = declaration else {
            return Err(CoreProfileError {
                reason: CoreRefusal::Statement,
                span: None,
            });
        };
        self.ty(object.ty, object.span)?;
        if !matches!(
            object.storage,
            StorageClass::None | StorageClass::Auto | StorageClass::Register
        ) {
            return Err(error(CoreRefusal::NonlocalObject, object.span));
        }
        if object.cleanup.is_some() || object.asm_label.is_some() || !object.attributes.is_empty() {
            return Err(error(CoreRefusal::Attributes, object.span));
        }
        if self.unit.zero_initialize_automatic(object) {
            return Err(error(CoreRefusal::Profile, object.span));
        }
        self.locals.insert(object.symbol, object.ty);
        self.initialized.remove(&object.symbol);
        self.inventory.local_objects += 1;
        if let Some(initializer) = &object.initializer {
            let plan = initializer.plan();
            let [InitAction::StoreScalar {
                path,
                destination_type,
                value,
                ..
            }] = plan.actions()
            else {
                return Err(error(CoreRefusal::Initializer, object.span));
            };
            if !path.is_root() || *destination_type != object.ty || plan.target_type() != object.ty
            {
                return Err(error(CoreRefusal::Initializer, object.span));
            }
            self.expression(value)?;
            self.initialized.insert(object.symbol);
        }
        Ok(())
    }

    /// Returns whether control may fall through. Loop exits conservatively
    /// retain only incoming definite initialization, including continue edges.
    fn statement(&mut self, statement: &CHirStmt<'_>) -> Result<bool> {
        match statement {
            CHirStmt::Null(_) => {}
            CHirStmt::Expr(value, _) => self.expression(value)?,
            CHirStmt::Return(Some(value), _) => {
                self.expression(value)?;
                return Ok(false);
            }
            CHirStmt::Compound(items, span, fenv) => {
                if *fenv == Some(true) {
                    return Err(error(CoreRefusal::Profile, *span));
                }
                let outer = self.locals.clone();
                let mut falls = true;
                for item in items {
                    match item {
                        CHirBlockItem::Declaration(decl) => self.declaration(decl)?,
                        CHirBlockItem::Statement(stmt) => {
                            falls &= self.statement(stmt)?;
                        }
                    }
                }
                self.locals = outer;
                self.initialized.retain(|s| self.locals.contains_key(s));
                return Ok(falls);
            }
            CHirStmt::If(condition, yes, no, _) => {
                self.expression(condition)?;
                let incoming = self.initialized.clone();
                let yes_falls = self.statement(yes)?;
                let yes_out = self.initialized.clone();
                self.initialized = incoming;
                let no_falls = match no {
                    Some(no) => self.statement(no)?,
                    None => true,
                };
                match (yes_falls, no_falls) {
                    (true, true) => self.initialized.retain(|s| yes_out.contains(s)),
                    (true, false) => self.initialized = yes_out,
                    _ => {}
                }
                return Ok(yes_falls || no_falls);
            }
            CHirStmt::While(condition, body, _) => {
                self.expression(condition)?;
                let incoming = self.initialized.clone();
                self.loop_depth += 1;
                self.inventory.loops += 1;
                self.statement(body)?;
                self.loop_depth -= 1;
                self.initialized = incoming;
            }
            CHirStmt::DoWhile(body, condition, _) => {
                let incoming = self.initialized.clone();
                self.loop_depth += 1;
                self.inventory.loops += 1;
                self.statement(body)?;
                self.loop_depth -= 1;
                self.initialized = incoming.clone();
                self.expression(condition)?;
                self.initialized = incoming;
            }
            CHirStmt::For(init, condition, increment, body, _) => {
                let outer = self.locals.clone();
                match init {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(value) => self.expression(value)?,
                    CHirForInit::Declaration(declarations) => {
                        for declaration in declarations {
                            self.declaration(declaration)?;
                        }
                    }
                }
                if let Some(condition) = condition {
                    self.expression(condition)?;
                }
                let incoming = self.initialized.clone();
                self.loop_depth += 1;
                self.inventory.loops += 1;
                self.statement(body)?;
                self.loop_depth -= 1;
                self.initialized = incoming.clone();
                if let Some(increment) = increment {
                    self.expression(increment)?;
                }
                self.initialized = incoming;
                self.locals = outer;
                self.initialized.retain(|s| self.locals.contains_key(s));
            }
            CHirStmt::Break(_) | CHirStmt::Continue(_) if self.loop_depth > 0 => return Ok(false),
            _ => return Err(error(CoreRefusal::Statement, statement.span())),
        }
        Ok(true)
    }
}
