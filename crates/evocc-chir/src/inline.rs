//! Source-level substitution of always-inline calls and constant inline leaves.
//!
//! This pass produces ordinary C-HIR objects, statements and expressions.
//! It does not add a second execution path or relax an assembly constraint.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use evocc_common::{Arena, SourceSpan};
use evocc_types::{TargetDataLayout, TypeId, TypeKind};

use crate::*;

#[derive(Debug)]
pub struct InlineError {
    pub span: SourceSpan,
    pub cycle: bool,
    pub message: String,
}

enum Node<'a> {
    Expr(&'a CHirExpr<'a>, bool),
    Stmt(&'a CHirStmt<'a>),
    Decl(&'a CHirDecl<'a>),
}

#[derive(Default)]
struct Uses<'a> {
    nodes: usize,
    references: BTreeSet<SymbolId>,
    assembly: Vec<&'a str>,
    addresses: BTreeSet<SymbolId>,
    mutable: BTreeSet<SymbolId>,
    statics: BTreeMap<SymbolId, &'a ObjectDecl<'a>>,
    owns_statements: bool,
    calls: Vec<(SymbolId, &'a [&'a CHirExpr<'a>])>,
}

/// Replaying an immutable parameter's argument must not read mutable storage
/// or repeat effects. This is an inlining property, not builtin_constant_p:
/// taking a stable object's address is repeatable even though that builtin
/// deliberately does not claim address constants as integer constants.
fn repeatable_argument(
    expression: &CHirExpr<'_>,
    types: &evocc_types::TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> bool {
    let repeat = |expression| repeatable_argument(expression, types, symbols, target);
    if crate::constant::eval_integer_constant(expression, types, symbols, target).is_some() {
        return true;
    }
    match expression {
        CHirExpr::StringLit(..) | CHirExpr::LabelAddress { .. } => true,
        CHirExpr::Ident { symbol, facts, .. } => symbols.symbol(*symbol).is_ok_and(|symbol| {
            symbol.kind() == SymbolKind::Function
                || matches!(
                    types.get(facts.ty()).map(|ty| &ty.kind),
                    Some(TypeKind::Array { .. })
                )
        }),
        CHirExpr::Cast(_, operand, _) => repeat(operand),
        CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => {
            repeatable_place(operand, types, symbols, target)
        }
        CHirExpr::MemoryRead { facts, .. }
            if matches!(
                types.get(facts.ty()).map(|ty| &ty.kind),
                Some(TypeKind::Array { .. })
            ) =>
        {
            repeatable_place(expression, types, symbols, target)
        }
        CHirExpr::Binary(BinaryOp::Add | BinaryOp::Sub, left, right, _) => {
            repeat(left) && repeat(right)
        }
        CHirExpr::Ternary(condition, yes, no, _) => {
            crate::constant::eval_integer_constant(condition, types, symbols, target)
                .is_some_and(|condition| repeat(if condition.raw_bits() == 0 { no } else { yes }))
        }
        _ => false,
    }
}

fn repeatable_place(
    expression: &CHirExpr<'_>,
    types: &evocc_types::TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> bool {
    let repeat = |expression| repeatable_argument(expression, types, symbols, target);
    match expression {
        CHirExpr::Ident { .. } | CHirExpr::StringLit(..) => true,
        CHirExpr::Unary(UnaryOp::Deref, pointer, _) => repeat(pointer),
        CHirExpr::MemoryRead { place, .. } => {
            let base = match place.root() {
                PlaceRoot::Object { .. } => true,
                PlaceRoot::PointerExpression { pointer, .. } => repeat(pointer),
                PlaceRoot::CompoundLiteral { .. } => false,
            };
            base && place
                .projections()
                .iter()
                .all(|projection| match projection {
                    PlaceProjection::Field(_) => true,
                    PlaceProjection::Index(index) => repeat(index.index()),
                })
        }
        CHirExpr::Member(base, _, _) => repeatable_place(base, types, symbols, target),
        CHirExpr::Index(base, index, _) => repeat(base) && repeat(index),
        _ => false,
    }
}

// Inspect semantic expression roots only. In particular, an initializer's
// written form and a statement expression's value are not second executions.
fn inspect<'a>(nodes: impl IntoIterator<Item = Node<'a>>) -> Uses<'a> {
    inspect_with_declarations(nodes, &mut |_| {})
}

pub(crate) fn for_each_declaration<'a>(
    unit: &CHirUnit<'a>,
    mut visit: impl FnMut(&'a CHirDecl<'a>),
) {
    inspect_with_declarations(unit.iter().map(Node::Decl), &mut visit);
}

fn inspect_with_declarations<'a>(
    nodes: impl IntoIterator<Item = Node<'a>>,
    visit: &mut impl FnMut(&'a CHirDecl<'a>),
) -> Uses<'a> {
    let mut work: Vec<_> = nodes.into_iter().collect();
    let mut uses = Uses::default();
    while let Some(node) = work.pop() {
        uses.nodes += 1;
        match node {
            Node::Decl(decl) => {
                visit(decl);
                match decl {
                    CHirDecl::Object(o) => {
                        if o.storage == StorageClass::Static {
                            uses.statics.insert(o.symbol, o);
                        }
                        if let Some(i) = &o.initializer {
                            for a in i.plan.actions() {
                                if let InitAction::StoreScalar { value, .. }
                                | InitAction::CopyAggregate { value, .. } = a
                                {
                                    work.push(Node::Expr(value, false));
                                }
                            }
                        }
                    }
                    CHirDecl::Function(f) => {
                        if let Some(b) = f.body {
                            work.push(Node::Stmt(b));
                        }
                    }
                    CHirDecl::Asm(a) => {
                        uses.assembly.push(&a.template);
                        for o in a.inputs.iter().chain(&a.outputs) {
                            work.push(Node::Expr(o.expression, false));
                        }
                    }
                    CHirDecl::Tag(_) | CHirDecl::Typedef(_) => {}
                }
            }
            Node::Stmt(s) => match s {
                CHirStmt::Compound(items, _) => {
                    for i in items {
                        work.push(match i {
                            CHirBlockItem::Declaration(d) => Node::Decl(d),
                            CHirBlockItem::Statement(s) => Node::Stmt(s),
                        });
                    }
                }
                CHirStmt::Expr(e, _) | CHirStmt::ComputedGoto(e, _) => {
                    work.push(Node::Expr(e, false))
                }
                CHirStmt::Return(e, _) => {
                    if let Some(e) = e {
                        work.push(Node::Expr(e, false));
                    }
                }
                CHirStmt::If(e, a, b, _) => {
                    work.push(Node::Expr(e, false));
                    work.push(Node::Stmt(a));
                    if let Some(b) = b {
                        work.push(Node::Stmt(b));
                    }
                }
                CHirStmt::Switch(e, b, _) | CHirStmt::While(e, b, _) | CHirStmt::Case(e, b, _) => {
                    work.push(Node::Expr(e, false));
                    work.push(Node::Stmt(b));
                }
                CHirStmt::DoWhile(b, e, _) => {
                    work.push(Node::Expr(e, false));
                    work.push(Node::Stmt(b));
                }
                CHirStmt::For(i, c, u, b, _) => {
                    match i {
                        CHirForInit::Empty => {}
                        CHirForInit::Expression(e) => work.push(Node::Expr(e, false)),
                        CHirForInit::Declaration(ds) => {
                            for d in ds {
                                work.push(Node::Decl(d));
                            }
                        }
                    }
                    for e in c.iter().chain(u) {
                        work.push(Node::Expr(e, false));
                    }
                    work.push(Node::Stmt(b));
                }
                CHirStmt::Label(_, b, _) | CHirStmt::Default(b, _) => work.push(Node::Stmt(b)),
                CHirStmt::CaseRange(a, b, s, _) => {
                    work.push(Node::Expr(a, false));
                    work.push(Node::Expr(b, false));
                    work.push(Node::Stmt(s));
                }
                CHirStmt::Asm(a) => {
                    uses.assembly.push(&a.template);
                    for o in &a.outputs {
                        if let CHirExpr::Ident { symbol, .. } = o.expression {
                            uses.mutable.insert(*symbol);
                        }
                    }
                    for o in &a.inputs {
                        // Memory constraints implicitly take the operand's
                        // address. Replacing its object with a value is invalid
                        // even when the initializer or argument is constant.
                        if matches!(o.constraint.as_str(), "m" | "Q" | "Qo") {
                            if let CHirExpr::Ident { symbol, .. } = o.expression {
                                uses.mutable.insert(*symbol);
                            }
                        }
                    }
                    for o in a.inputs.iter().chain(&a.outputs) {
                        work.push(Node::Expr(o.expression, false));
                    }
                }
                CHirStmt::Break(_)
                | CHirStmt::Continue(_)
                | CHirStmt::Goto(_, _)
                | CHirStmt::Null(_)
                | CHirStmt::AttributedNull(..) => {}
            },
            Node::Expr(e, callee) => {
                uses.owns_statements |= matches!(e, CHirExpr::StatementExpr { .. });
                if let CHirExpr::Ident { symbol, .. } = e {
                    uses.references.insert(*symbol);
                }
                match e {
                    CHirExpr::Ident { symbol, .. } if !callee => {
                        uses.addresses.insert(*symbol);
                    }
                    CHirExpr::Assign { place, .. } | CHirExpr::Update { place, .. } => {
                        uses.mutable.insert(place.symbol());
                    }
                    CHirExpr::Unary(UnaryOp::AddrOf, CHirExpr::Ident { symbol, .. }, _) => {
                        uses.mutable.insert(*symbol);
                    }
                    _ => {}
                }
                match e {
                    CHirExpr::Call(f, args, _) => {
                        if let CHirExpr::Ident { symbol, .. } = f {
                            uses.calls.push((*symbol, args));
                        }
                        work.push(Node::Expr(f, true));
                        for a in args {
                            work.push(Node::Expr(a, false));
                        }
                    }
                    CHirExpr::Unary(_, e, _)
                    | CHirExpr::Cast(_, e, _)
                    | CHirExpr::EvaluateOnce(e, _)
                    | CHirExpr::Member(e, _, _)
                    | CHirExpr::PtrMember(e, _, _)
                    | CHirExpr::BitBuiltin { operand: e, .. } => work.push(Node::Expr(e, false)),
                    CHirExpr::Binary(_, a, b, _)
                    | CHirExpr::Index(a, b, _)
                    | CHirExpr::Comma(a, b, _)
                    | CHirExpr::Elvis(a, b, _) => {
                        work.push(Node::Expr(a, false));
                        work.push(Node::Expr(b, false));
                    }
                    CHirExpr::Ternary(c, a, b, _) => {
                        for e in [c, a, b] {
                            work.push(Node::Expr(e, false));
                        }
                    }
                    CHirExpr::Assign { target, value, .. } => {
                        work.push(Node::Expr(target, false));
                        work.push(Node::Expr(value, false));
                    }
                    CHirExpr::Update { target, .. } => work.push(Node::Expr(target, false)),
                    CHirExpr::MemoryRead { place, .. }
                    | CHirExpr::MemoryUpdate { place, .. }
                    | CHirExpr::MemoryAssign { place, .. } => {
                        if let Some(s) = place.root_symbol() {
                            uses.mutable.insert(s);
                        }
                        for e in place.source_expressions() {
                            work.push(Node::Expr(e, false));
                        }
                        if let CHirExpr::MemoryAssign { value, .. } = e {
                            work.push(Node::Expr(value, false));
                        }
                    }
                    CHirExpr::CheckedArithmetic {
                        lhs, rhs, target, ..
                    } => {
                        for e in [lhs, rhs, target] {
                            work.push(Node::Expr(e, false));
                        }
                    }
                    CHirExpr::BuiltinVarArgs { first, second, .. } => {
                        work.push(Node::Expr(first, false));
                        if let Some(e) = second {
                            work.push(Node::Expr(e, false));
                        }
                    }
                    CHirExpr::StatementExpr { body, .. } => work.push(Node::Stmt(body)),
                    CHirExpr::CompoundLit(_, i, _) => {
                        for a in i.plan.actions() {
                            if let InitAction::StoreScalar { value, .. }
                            | InitAction::CopyAggregate { value, .. } = a
                            {
                                work.push(Node::Expr(value, false));
                            }
                        }
                    }
                    CHirExpr::Lit(..)
                    | CHirExpr::FloatLit(..)
                    | CHirExpr::CharLit(..)
                    | CHirExpr::StringLit(..)
                    | CHirExpr::Ident { .. }
                    | CHirExpr::LabelAddress { .. }
                    | CHirExpr::SizeOf(..)
                    | CHirExpr::SizeOfExpr(..)
                    | CHirExpr::BuiltinConstantP { .. }
                    | CHirExpr::AlignOf(..)
                    | CHirExpr::BuiltinTrap { .. }
                    | CHirExpr::BuiltinUnreachable { .. }
                    | CHirExpr::BuiltinReturnAddress { .. }
                    | CHirExpr::BuiltinFrameAddress { .. } => {}
                }
            }
        }
    }
    uses
}

/// Source-reference closure, without branch pruning or assumptions about
/// traps. The same graph controls deferred profile diagnostics and lowering.
pub fn retained_functions(
    unit: &CHirUnit<'_>,
) -> Result<BTreeSet<SymbolId>, crate::linkage::LinkageError> {
    let linkage = unit.symbol_linkage()?;
    let definitions: BTreeMap<_, _> = unit
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.body.is_some() => {
                Some((function.symbol, function))
            }
            _ => None,
        })
        .collect();
    let mut assembly_names = BTreeMap::new();
    let mut pending = Vec::new();
    for (&symbol, function) in &definitions {
        if !function.has_internal_linkage(unit.symbols()) {
            pending.push(symbol);
        }
        let name = linkage.function_name(symbol, &function.name.spelling);
        assembly_names.insert(name.to_owned(), symbol);
        // Darwin assembly spells an ordinary C symbol with this prefix.
        assembly_names.insert(format!("_{name}"), symbol);
    }
    pending.extend(linkage.aliases.values().copied());
    let add_uses = |uses: Uses<'_>, pending: &mut Vec<SymbolId>| {
        pending.extend(
            uses.references
                .into_iter()
                .map(|symbol| linkage.target(symbol)),
        );
        for assembly in uses.assembly {
            for name in assembly
                .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$')))
            {
                if let Some(&symbol) = assembly_names.get(name) {
                    pending.push(symbol);
                }
            }
        }
    };
    add_uses(
        inspect(
            unit.iter()
                .filter(|d| !matches!(d, CHirDecl::Function(_)))
                .map(Node::Decl),
        ),
        &mut pending,
    );
    let mut retained = BTreeSet::new();
    while let Some(symbol) = pending.pop() {
        let Some(function) = definitions.get(&symbol) else {
            continue;
        };
        if retained.insert(symbol) {
            add_uses(inspect([Node::Stmt(function.body.unwrap())]), &mut pending);
        }
    }
    Ok(retained)
}

/// Keep the original unit intact for interpretation and source diagnostics.
/// Only declaration references change; surviving object/node identities do not.
pub fn without_unreferenced_functions<'u, 'a>(
    unit: &'u CHirUnit<'a>,
) -> Result<std::borrow::Cow<'u, CHirUnit<'a>>, crate::linkage::LinkageError> {
    let retained = retained_functions(unit)?;
    let keep = |declaration: &&CHirDecl<'_>| {
        !matches!(declaration, CHirDecl::Function(function)
            if function.body.is_some() && !retained.contains(&function.symbol))
    };
    if unit.declarations.iter().all(keep) {
        return Ok(std::borrow::Cow::Borrowed(unit));
    }
    let mut rewritten = unit.clone();
    rewritten.declarations.retain(keep);
    rewritten.resolved_linkage.take();
    Ok(std::borrow::Cow::Owned(rewritten))
}

/// A no-argument internal inline leaf has no parameter initialization, local
/// lifetime, or body effects to preserve. Its literal return can expose a
/// configuration condition before unreachable external references are lowered.
/// This is an optimization fact, not a source-language constant expression.
fn constant_inline_return<'a>(
    unit: &CHirUnit<'a>,
    function: &FunctionDecl<'a>,
    target: &TargetDataLayout,
) -> Option<&'a CHirExpr<'a>> {
    if !matches!(&function.parameters,
            ParameterSyntax::Prototype { parameters, variadic: false }
            if parameters.is_empty())
    {
        return None;
    }
    crate::constant::inline_constant_result(unit, function, target)
        .map(|(expression, _)| expression)
}

pub fn needs_substitution(unit: &CHirUnit<'_>, target: &TargetDataLayout) -> bool {
    unit.iter().any(|d| {
        matches!(d, CHirDecl::Function(f) if f.body.is_some()
        && (f.attributes.iter().any(|a| a.semantic_name() == Some("always_inline"))
            || constant_inline_return(unit, f, target).is_some()
            || f.has_internal_linkage(unit.symbols())))
    }) || !static_integer_values(unit, target).is_empty()
}

type ParameterConstants<'a> = BTreeMap<SymbolId, BTreeMap<SymbolId, (&'a CHirExpr<'a>, TypeId)>>;

// Closed-world facts only: every direct caller must pass the same converted
// value, and neither address-taking nor assembly may supply another caller.
// Keep calls (including noinline calls) and all argument evaluations intact.
fn common_parameter_values<'a>(
    unit: &CHirUnit<'a>,
    target: &TargetDataLayout,
    uses: &Uses<'a>,
    linkage: &crate::linkage::SymbolLinkage,
) -> ParameterConstants<'a> {
    let mut calls = BTreeMap::<_, Vec<_>>::new();
    for &(symbol, args) in &uses.calls {
        calls.entry(symbol).or_default().push(args);
    }
    let assembly_names: BTreeSet<_> = uses
        .assembly
        .iter()
        .flat_map(|assembly| {
            assembly.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$')))
        })
        .collect();
    let mut result = BTreeMap::new();
    for declaration in unit.iter() {
        let CHirDecl::Function(function) = declaration else {
            continue;
        };
        let Some(body) = function.body else { continue };
        let ParameterSyntax::Prototype {
            parameters,
            variadic: false,
        } = &function.parameters
        else {
            continue;
        };
        let name = linkage.function_name(function.symbol, &function.name.spelling);
        if !function.has_internal_linkage(unit.symbols())
            || uses.addresses.contains(&function.symbol)
            || assembly_names.contains(name)
            || assembly_names.contains(format!("_{name}").as_str())
        {
            continue;
        }
        let Some(callers) = calls.get(&function.symbol) else {
            continue;
        };
        if callers.iter().any(|args| args.len() != parameters.len()) {
            continue;
        }
        let mutable = inspect([Node::Stmt(body)]).mutable;
        for (index, parameter) in parameters.iter().enumerate() {
            let Some(symbol) = parameter.symbol else {
                continue;
            };
            let Some(integer) = crate::constant::canonical_integer_type(unit.types(), parameter.ty)
            else {
                continue;
            };
            let qualifiers = unit.types.get(parameter.ty).unwrap().qualifiers;
            if mutable.contains(&symbol) || qualifiers.is_volatile || qualifiers.is_atomic {
                continue;
            }
            let mut values = callers.iter().map(|args| {
                crate::constant::eval_executable_integer_constant(
                    args[index],
                    unit.types(),
                    unit.symbols(),
                    target,
                )
                .map(|value| value.cast(integer))
            });
            let Some(Some(first)) = values.next() else {
                continue;
            };
            if values.all(|value| value == Some(first)) {
                result
                    .entry(function.symbol)
                    .or_insert_with(BTreeMap::new)
                    .insert(symbol, (callers[0][index], parameter.ty));
            }
        }
    }
    result
}

// This is an execution optimization, not an extension of integer constant
// expressions or constant_p. An address escape or volatile access prevents it.
fn static_integer_values<'a>(
    unit: &CHirUnit<'a>,
    target: &TargetDataLayout,
) -> BTreeMap<SymbolId, (&'a CHirExpr<'a>, TypeId)> {
    let uses = inspect(unit.iter().map(Node::Decl));
    uses.statics
        .iter()
        .filter_map(|(&symbol, object)| {
            let canonical = unit.types.get(object.ty)?;
            if !canonical.qualifiers.is_const
                || canonical.qualifiers.is_volatile
                || canonical.qualifiers.is_atomic
                || uses.mutable.contains(&symbol)
                || object.asm_label.is_some()
                || !matches!(
                    canonical.kind,
                    TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_)
                )
            {
                return None;
            }
            let [InitAction::StoreScalar { path, value, .. }] =
                object.initializer.as_ref()?.plan.actions()
            else {
                return None;
            };
            (path.is_root()
                && crate::constant::eval_integer_constant(
                    value,
                    &unit.types,
                    &unit.symbols,
                    target,
                )
                .is_some())
            .then_some((symbol, (*value, object.ty)))
        })
        .collect()
}

/// Rewrite into the caller-owned arena. Original nodes and symbol tables are
/// left untouched, so the original interpreter remains a differential oracle.
pub fn substitute<'a>(
    unit: &CHirUnit<'a>,
    arena: &'a Arena,
    target: &TargetDataLayout,
) -> Result<CHirUnit<'a>, InlineError> {
    let mut uses = inspect(unit.iter().map(Node::Decl));
    let linkage = crate::linkage::SymbolLinkage::derive(unit).map_err(|e| InlineError {
        span: e.span,
        cycle: false,
        message: e.message.to_owned(),
    })?;
    uses.addresses.extend(linkage.aliases.values());
    let parameter_values = common_parameter_values(unit, target, &uses, &linkage);
    // Honor ordinary inline hints for small, acyclic private helpers too.
    // The bound limits code growth; mandatory always_inline remains separate.
    let bodies: BTreeMap<_, _> = unit
        .iter()
        .filter_map(|d| match d {
            CHirDecl::Function(f) => f.body.map(|body| (f.symbol, inspect([Node::Stmt(body)]))),
            _ => None,
        })
        .collect();
    let recursive = |symbol| {
        let mut pending = bodies[&symbol]
            .references
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let mut visited = BTreeSet::new();
        while let Some(next) = pending.pop() {
            if next == symbol {
                return true;
            }
            if visited.insert(next) {
                if let Some(body) = bodies.get(&next) {
                    pending.extend(body.references.iter().copied());
                }
            }
        }
        false
    };
    let eligible: BTreeMap<_, _> = unit
        .iter()
        .filter_map(|d| match d {
            CHirDecl::Function(f)
                if f.body.is_some()
                    && (f
                        .attributes
                        .iter()
                        .any(|a| a.semantic_name() == Some("always_inline"))
                        || constant_inline_return(unit, f, target).is_some()
                        || (!f.inline_spans.is_empty()
                            && f.has_internal_linkage(unit.symbols())
                            && !f
                                .attributes
                                .iter()
                                .any(|a| a.semantic_name() == Some("noinline"))
                            && bodies[&f.symbol].nodes <= 128
                            && !recursive(f.symbol)))
                    && !uses.addresses.contains(&f.symbol)
                    && !matches!(
                        f.parameters,
                        ParameterSyntax::Prototype { variadic: true, .. }
                    ) =>
            {
                Some((f.symbol, f))
            }
            _ => None,
        })
        .collect();
    let static_objects: BTreeMap<_, _> = eligible
        .values()
        .flat_map(|f| inspect([Node::Stmt(f.body.unwrap())]).statics)
        .collect();
    let mut pass = Substitution {
        unit: unit.clone(),
        arena,
        target,
        eligible,
        globals: BTreeMap::new(),
        static_values: BTreeMap::new(),
        serial: 0,
        active: Vec::new(),
        temporaries: Vec::new(),
    };
    for (symbol, (value, ty)) in static_integer_values(unit, target) {
        let converted = pass.alloc(CHirExpr::Cast(ty, value, value.span()));
        if let Some(value) = pass.repeatable_value(converted, ty) {
            pass.static_values.insert(symbol, value);
        }
    }
    // Lift each function-static to one internal object, not one per clone.
    // Rewriting every body (including retained out-of-line bodies) uses this
    // same symbol. Source names and __func__ literal contents are retained.
    let mut lifted = Vec::new();
    for (s, o) in static_objects {
        if unit
            .symbols
            .symbol(s)
            .map_err(|e| pass.error(o.span, e))?
            .scope()
            == unit.symbols.file_scope()
        {
            continue;
        }
        let fresh = pass.fresh(
            pass.unit.symbols.file_scope(),
            o.ty,
            StorageClass::Static,
            o.span,
        )?;
        pass.globals.insert(s, fresh);
        lifted.push(o);
    }
    let mut declarations = Vec::new();
    let mut context = Context::new(pass.unit.symbols.file_scope());
    for o in lifted {
        let mut object = o.clone();
        object.symbol = pass.globals[&o.symbol];
        object.name.spelling = pass
            .unit
            .symbols
            .symbol(object.symbol)
            .unwrap()
            .spelling()
            .to_owned();
        object.initializer = o
            .initializer
            .as_ref()
            .map(|i| pass.initializer(i, &mut context, false))
            .transpose()?;
        declarations.push(pass.alloc(CHirDecl::Object(object)));
    }
    for declaration in unit.iter() {
        if let CHirDecl::Function(f) = declaration {
            if let Some(body) = f.body {
                let scope = unit
                    .symbols
                    .scopes_in_source_order()
                    .iter()
                    .find(|s| s.function_owner() == Some(f.symbol))
                    .ok_or_else(|| pass.error(f.span, "function scope missing"))?
                    .id();
                let mut c = Context::new(scope);
                if let Some(values) = parameter_values.get(&f.symbol) {
                    for (&symbol, &(value, ty)) in values {
                        let converted = pass.alloc(CHirExpr::Cast(ty, value, value.span()));
                        if let Some(value) = pass.repeatable_value(converted, ty) {
                            c.optimized_constants.insert(symbol, value);
                        }
                    }
                }
                pass.temporaries.clear();
                pass.active.push(f.symbol);
                let mut rewritten = pass.stmt(body, &mut c)?;
                pass.active.pop();
                if f.has_internal_linkage(pass.unit.symbols())
                    && pass.eligible.contains_key(&f.symbol)
                {
                    continue;
                }
                if !pass.temporaries.is_empty() {
                    let mut items: Vec<_> = std::mem::take(&mut pass.temporaries)
                        .into_iter()
                        .map(CHirBlockItem::Declaration)
                        .collect();
                    let CHirStmt::Compound(body_items, span) = rewritten else {
                        return Err(pass.error(f.span, "function body is not compound"));
                    };
                    items.extend(body_items.iter().cloned());
                    rewritten = pass.alloc(CHirStmt::Compound(items, *span));
                }
                let mut f = f.clone();
                f.body = Some(rewritten);
                declarations.push(pass.alloc(CHirDecl::Function(f)));
                continue;
            }
        }
        declarations.extend(pass.decl(declaration, &mut context)?);
    }
    pass.unit.declarations = declarations;
    pass.unit.resolved_linkage.take();
    Ok(pass.unit)
}

struct Context<'a> {
    scope: ScopeId,
    replacements: BTreeMap<SymbolId, SymbolId>,
    constants: BTreeMap<SymbolId, &'a CHirExpr<'a>>,
    optimized_constants: BTreeMap<SymbolId, &'a CHirExpr<'a>>,
    mutable: BTreeSet<SymbolId>,
    declaration_prefix: bool,
    memo: HashMap<(usize, bool), &'a CHirExpr<'a>>,
    expand_calls: bool,
    prefix: Option<u64>,
    result: Option<SymbolId>,
    end: Option<SpannedName>,
}

impl Context<'_> {
    fn new(scope: ScopeId) -> Self {
        Self {
            scope,
            replacements: BTreeMap::new(),
            constants: BTreeMap::new(),
            optimized_constants: BTreeMap::new(),
            mutable: BTreeSet::new(),
            declaration_prefix: false,
            memo: HashMap::new(),
            expand_calls: true,
            prefix: None,
            result: None,
            end: None,
        }
    }
}

struct Substitution<'a, 't> {
    unit: CHirUnit<'a>,
    arena: &'a Arena,
    target: &'t TargetDataLayout,
    eligible: BTreeMap<SymbolId, &'a FunctionDecl<'a>>,
    globals: BTreeMap<SymbolId, SymbolId>,
    static_values: BTreeMap<SymbolId, &'a CHirExpr<'a>>,
    serial: u64,
    active: Vec<SymbolId>,
    temporaries: Vec<&'a CHirDecl<'a>>,
}

impl<'a> Substitution<'a, '_> {
    fn alloc<T: 'a>(&self, value: T) -> &'a T {
        // SAFETY: nodes only contain earlier nodes or original arena borrows;
        // their destructors never dereference those borrows or access Arena.
        unsafe { self.arena.alloc_shared(value) }
    }
    fn error(&self, span: SourceSpan, message: impl ToString) -> InlineError {
        InlineError {
            span,
            cycle: false,
            message: message.to_string(),
        }
    }
    fn next(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }
    fn fresh(
        &mut self,
        scope: ScopeId,
        ty: TypeId,
        storage: StorageClass,
        span: SourceSpan,
    ) -> Result<SymbolId, InlineError> {
        let name = format!("__evocc_inline_{}", self.next());
        self.unit
            .symbols
            .declare(scope, name, SymbolData::Object { storage }, ty, span)
            .map_err(|e| self.error(span, e))
    }
    fn symbol(&self, s: SymbolId, c: &Context<'a>) -> SymbolId {
        c.replacements
            .get(&s)
            .or_else(|| self.globals.get(&s))
            .copied()
            .unwrap_or(s)
    }
    fn label(&self, n: &SpannedName, c: &Context<'a>) -> SpannedName {
        let mut n = n.clone();
        if let Some(p) = c.prefix {
            n.spelling = format!("__evocc_inline_{p}_{}", n.spelling);
        }
        n
    }
    fn ident(&self, s: SymbolId, span: SourceSpan) -> &'a CHirExpr<'a> {
        let symbol = self.unit.symbols.symbol(s).unwrap();
        self.alloc(CHirExpr::Ident {
            symbol: s,
            spelling: symbol.spelling().to_owned(),
            facts: ExprFacts::lvalue(
                symbol.ty(),
                is_modifiable_object_type(&self.unit.types, symbol.ty()),
                true,
            )
            .unwrap(),
            span,
        })
    }
    /// Propagate a value, never a second copy of its owned statements/labels.
    /// The original object initializer remains in place, so argument effects
    /// and checked-arithmetic obligations are still executed exactly once.
    fn repeatable_value(
        &mut self,
        expression: &'a CHirExpr<'a>,
        ty: TypeId,
    ) -> Option<&'a CHirExpr<'a>> {
        if let Some(value) = crate::constant::eval_integer_constant(
            expression,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        ) {
            let span = expression.span();
            let suffix = match (value.ty().rank(), value.ty().is_signed()) {
                (0..=3, true) => IntSuffix::None,
                (0..=3, false) => IntSuffix::U,
                (4, true) => IntSuffix::L,
                (4, false) => IntSuffix::UL,
                (5, true) => IntSuffix::LL,
                _ => IntSuffix::ULL,
            };
            let literal = |bits| {
                self.alloc(CHirExpr::Lit(
                    IntLit {
                        value: bits,
                        suffix,
                        base: IntBase::Hex,
                    },
                    span,
                ))
            };
            let low = self.alloc(CHirExpr::Cast(ty, literal(value.raw_bits() as u64), span));
            if value.raw_bits() >> 64 == 0 {
                return Some(low);
            }
            let high = self.alloc(CHirExpr::Cast(
                ty,
                literal((value.raw_bits() >> 64) as u64),
                span,
            ));
            let high = self.alloc(CHirExpr::Binary(BinaryOp::Shl, high, literal(64), span));
            return Some(self.alloc(CHirExpr::Binary(BinaryOp::BitOr, high, low, span)));
        }
        if !repeatable_argument(
            expression,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        ) {
            return None;
        }
        self.reference_value(expression)
    }

    /// Remove unselected statement ownership from an already-proven stable
    /// reference. Keep each intermediate pointer type: it determines scaling.
    fn reference_value(&mut self, expression: &'a CHirExpr<'a>) -> Option<&'a CHirExpr<'a>> {
        if !inspect([Node::Expr(expression, false)]).owns_statements {
            return Some(expression);
        }
        if let Some(value) = crate::constant::eval_integer_constant(
            expression,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        ) {
            let kind = if let Some(rank) = crate::integer::rank_for_integer_type(value.ty()) {
                TypeKind::Int(rank)
            } else if value.ty().bits() == 128 {
                TypeKind::GnuInt128(if value.ty().is_signed() {
                    evocc_types::GnuInt128Signedness::Signed
                } else {
                    evocc_types::GnuInt128Signedness::Unsigned
                })
            } else {
                return None;
            };
            let ty = self.unit.types.intern(evocc_types::CanonicalType {
                kind,
                qualifiers: Default::default(),
            });
            return self.repeatable_value(expression, ty);
        }
        let span = expression.span();
        let value = match expression {
            CHirExpr::Cast(ty, operand, _) => {
                CHirExpr::Cast(*ty, self.reference_value(operand)?, span)
            }
            CHirExpr::Ternary(condition, yes, no, _) => {
                let condition = crate::constant::eval_integer_constant(
                    condition,
                    &self.unit.types,
                    &self.unit.symbols,
                    self.target,
                )?;
                let pointer = PointerTypeDesc::from_expression(
                    &self.unit.symbols,
                    &self.unit.types,
                    expression,
                )
                .ok()?;
                let mut pointee = self.unit.types.get(pointer.pointee())?.clone();
                pointee.qualifiers = pointer.pointee_qualifiers();
                let pointee = self.unit.types.intern(pointee);
                let ty = self.unit.types.intern(evocc_types::CanonicalType {
                    kind: TypeKind::Pointer(pointee),
                    qualifiers: Default::default(),
                });
                let selected = self.reference_value(if condition.truth() { yes } else { no })?;
                CHirExpr::Cast(ty, selected, span)
            }
            CHirExpr::Binary(op @ (BinaryOp::Add | BinaryOp::Sub), left, right, _) => {
                let left = self.reference_value(left)?;
                let right = self.reference_value(right)?;
                CHirExpr::Binary(*op, left, right, span)
            }
            CHirExpr::Unary(op @ (UnaryOp::AddrOf | UnaryOp::Deref), operand, _) => {
                CHirExpr::Unary(*op, self.reference_value(operand)?, span)
            }
            CHirExpr::MemoryRead {
                place,
                facts,
                target_span,
                ..
            } => {
                let root = match place.root() {
                    PlaceRoot::Object { symbol } => PlaceRoot::Object { symbol: *symbol },
                    PlaceRoot::PointerExpression {
                        pointer,
                        pointer_type,
                    } => PlaceRoot::PointerExpression {
                        pointer: self.reference_value(pointer)?,
                        pointer_type: *pointer_type,
                    },
                    PlaceRoot::CompoundLiteral { .. } => return None,
                };
                let mut projections = Vec::new();
                for projection in place.projections() {
                    projections.push(match projection {
                        PlaceProjection::Field(field) => PlaceProjection::Field(*field),
                        PlaceProjection::Index(index) => {
                            PlaceProjection::Index(IndexProjection::new(
                                self.reference_value(index.index())?,
                                index.element_ty(),
                                index.promoted_index(),
                            ))
                        }
                    });
                }
                CHirExpr::MemoryRead {
                    place: MemoryPlace::derive(
                        &self.unit.symbols,
                        &self.unit.types,
                        root,
                        projections,
                        place.value_ty(),
                    )
                    .ok()?,
                    facts: *facts,
                    target_span: *target_span,
                    span,
                }
            }
            _ => return None,
        };
        Some(self.alloc(value))
    }

    fn object(
        &self,
        s: SymbolId,
        value: Option<&'a CHirExpr<'a>>,
        span: SourceSpan,
    ) -> &'a CHirDecl<'a> {
        let symbol = self.unit.symbols.symbol(s).unwrap();
        let ty = symbol.ty();
        let initializer = value.map(|v| {
            // A list parameter is the same complete-object copy as an
            // ordinary record parameter, never a scalar store of its address.
            let action = if matches!(
                self.unit.types.get(ty).unwrap().kind,
                TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList
            ) {
                InitAction::CopyAggregate {
                    path: SubobjectPath::root(),
                    destination_type: ty,
                    value: v,
                    span,
                }
            } else {
                InitAction::StoreScalar {
                    path: SubobjectPath::root(),
                    destination_type: ty,
                    value: v,
                    span,
                }
            };
            ObjectInitializer::new(
                WrittenInitializer::Expression(v),
                InitPlan::new(ty, vec![action]),
            )
        });
        self.alloc(CHirDecl::Object(ObjectDecl {
            symbol: s,
            name: SpannedName {
                spelling: symbol.spelling().to_owned(),
                span,
            },
            ty,
            specifier: TypeSpecifier::Builtin { ty, span },
            written: WrittenDeclarator {
                layers: Vec::new(),
                span,
            },
            storage: StorageClass::None,
            initializer,
            asm_label: None,
            asm_label_span: None,
            gnu_extension_spans: Vec::new(),
            attributes: Vec::new(),
            span,
        }))
    }
    fn object_root(
        &mut self,
        symbol: SymbolId,
        c: &Context<'a>,
        span: SourceSpan,
    ) -> Result<PlaceRoot<'a>, InlineError> {
        let lifted = self.globals.contains_key(&symbol);
        let symbol = self.symbol(symbol, c);
        if !lifted || c.prefix.is_none() {
            return Ok(PlaceRoot::Object { symbol });
        }
        let storage_ty = self
            .unit
            .symbols
            .symbol(symbol)
            .map_err(|e| self.error(span, e))?
            .ty();
        let view_ty = self.substituted_type(storage_ty, c);
        if view_ty == storage_ty {
            return Ok(PlaceRoot::Object { symbol });
        }
        // Lifted statics keep one storage identity, but a pointer-to-VLA
        // view uses this invocation's captures. Express the view as a checked
        // address cast, not different facts on the shared symbol.
        let address = self.alloc(CHirExpr::Unary(
            UnaryOp::AddrOf,
            self.ident(symbol, span),
            span,
        ));
        let pointer_ty = self.unit.types.intern(evocc_types::CanonicalType {
            kind: TypeKind::Pointer(view_ty),
            qualifiers: Default::default(),
        });
        Ok(PlaceRoot::PointerExpression {
            pointer: self.alloc(CHirExpr::Cast(pointer_ty, address, span)),
            pointer_type: PointerTypeDesc::from_canonical_pointer(&self.unit.types, pointer_ty)
                .map_err(|e| self.error(span, e))?,
        })
    }

    fn place(
        &mut self,
        p: &MemoryPlace<'a>,
        c: &mut Context<'a>,
        span: SourceSpan,
    ) -> Result<MemoryPlace<'a>, InlineError> {
        let root = match p.root() {
            PlaceRoot::Object { symbol } => self.object_root(*symbol, c, span)?,
            PlaceRoot::PointerExpression {
                pointer,
                pointer_type,
            } => PlaceRoot::PointerExpression {
                pointer: self.expr(pointer, c)?,
                pointer_type: pointer_type
                    .with_pointee(self.substituted_type(pointer_type.pointee(), c)),
            },
            PlaceRoot::CompoundLiteral { expression, ty } => PlaceRoot::CompoundLiteral {
                expression: self.expr(expression, c)?,
                ty: self.substituted_type(*ty, c),
            },
        };
        let mut projections = Vec::new();
        for projection in p.projections() {
            projections.push(match projection {
                PlaceProjection::Field(f) => PlaceProjection::Field(*f),
                PlaceProjection::Index(i) => PlaceProjection::Index(IndexProjection::new(
                    self.expr(i.index(), c)?,
                    self.substituted_type(i.element_ty(), c),
                    i.promoted_index(),
                )),
            });
        }
        let value_ty = self.substituted_type(p.value_ty(), c);
        MemoryPlace::derive(
            &self.unit.symbols,
            &self.unit.types,
            root,
            projections,
            value_ty,
        )
        .map_err(|e| self.error(span, e))
    }
    fn initializer(
        &mut self,
        i: &ObjectInitializer<'a>,
        c: &mut Context<'a>,
        expand_calls: bool,
    ) -> Result<ObjectInitializer<'a>, InlineError> {
        let previous = c.expand_calls;
        c.expand_calls &= expand_calls;
        let mut actions = i.plan.actions().to_vec();
        for a in &mut actions {
            let (InitAction::StoreScalar {
                destination_type, ..
            }
            | InitAction::CopyAggregate {
                destination_type, ..
            }
            | InitAction::StoreString {
                destination_type, ..
            }
            | InitAction::ZeroScalar {
                destination_type, ..
            }) = a;
            *destination_type = self.substituted_type(*destination_type, c);
            if let InitAction::StoreScalar { value, .. } | InitAction::CopyAggregate { value, .. } =
                a
            {
                *value = self.expr(value, c)?;
            }
        }
        let mut plan = InitPlan::new(self.substituted_type(i.plan.target_type(), c), actions)
            .with_flexible_array_elements(i.plan.flexible_array_elements());
        if i.plan.zero_fills_unwritten() {
            plan = plan.with_zero_fill_unwritten();
        }
        // The manifest visits the written form too. Preserve the shared
        // expression identities between that form and its normalized plan.
        let written = self.written(&i.written, c)?;
        c.expand_calls = previous;
        Ok(ObjectInitializer::new(written, plan))
    }
    fn written(
        &mut self,
        written: &WrittenInitializer<'a>,
        c: &mut Context<'a>,
    ) -> Result<WrittenInitializer<'a>, InlineError> {
        let mut written = written.clone();
        match &mut written {
            WrittenInitializer::Expression(e) => *e = self.expr(e, c)?,
            WrittenInitializer::List {
                elements,
                designations,
                ..
            } => {
                for element in elements {
                    *element = self.written(element, c)?;
                }
                // Designators are already checked integer constant expressions.
                // A call in an unselected arm is syntax, not an execution site.
                let previous = c.expand_calls;
                c.expand_calls = false;
                for designation in designations.iter_mut().flatten() {
                    for designator in &mut designation.designators {
                        match designator {
                            WrittenDesignator::Field(_) => {}
                            WrittenDesignator::Index { expression, .. } => {
                                *expression = self.expr(expression, c)?
                            }
                            WrittenDesignator::Range { first, last, .. } => {
                                *first = self.expr(first, c)?;
                                *last = self.expr(last, c)?;
                            }
                        }
                    }
                }
                c.expand_calls = previous;
            }
        }
        Ok(written)
    }
    fn decl(
        &mut self,
        d: &'a CHirDecl<'a>,
        c: &mut Context<'a>,
    ) -> Result<Vec<&'a CHirDecl<'a>>, InlineError> {
        if let CHirDecl::Function(f) = d {
            if c.prefix.is_some() && !self.unit.symbols.is_builtin_function(f.symbol) {
                self.unit
                    .symbols
                    .bind_existing(c.scope, f.symbol)
                    .map_err(|e| self.error(f.span, e))?;
            }
        }
        let CHirDecl::Object(o) = d else {
            // Types and linkage identities are shared, structural parents are
            // not: each substituted declaration needs its own C-HIR node.
            return Ok(vec![self.alloc(d.clone())]);
        };
        if self.globals.contains_key(&o.symbol) {
            return Ok(Vec::new());
        }
        let original = o.symbol;
        let mut o = o.clone();
        if c.prefix.is_some() && o.storage == StorageClass::Extern {
            self.unit
                .symbols
                .bind_existing(c.scope, o.symbol)
                .map_err(|e| self.error(o.span, e))?;
        }
        if c.prefix.is_some() && o.storage != StorageClass::Extern {
            o.ty = self.substituted_type(o.ty, c);
            let (TypeSpecifier::Builtin { ty, .. }
            | TypeSpecifier::TypedefName { ty, .. }
            | TypeSpecifier::Tag { ty, .. }
            | TypeSpecifier::Typeof { resolved: ty, .. }
            | TypeSpecifier::AutoType { resolved: ty, .. }) = &mut o.specifier;
            *ty = self.substituted_type(*ty, c);
            for layer in &mut o.written.layers {
                if let WrittenDeclaratorLayer::Array { bound, .. } = layer {
                    *bound = self.substituted_bound(bound, c);
                }
            }
            let s = if let Some(symbol) = c.replacements.get(&o.symbol) {
                // A parameter's type may name this entry-bound capture
                // before its initializer is substituted below.
                *symbol
            } else {
                let symbol = self.fresh(c.scope, o.ty, o.storage, o.span)?;
                c.replacements.insert(o.symbol, symbol);
                symbol
            };
            o.symbol = s;
            o.name.spelling = self.unit.symbols.symbol(s).unwrap().spelling().to_owned();
        }
        if let Some(i) = &o.initializer {
            let runtime =
                o.storage != StorageClass::Static && c.scope != self.unit.symbols.file_scope();
            o.initializer = Some(self.initializer(i, c, runtime)?);
        }
        if c.declaration_prefix && o.storage != StorageClass::Extern {
            if let Some(initializer) = &o.initializer {
                let value = match initializer.plan.actions() {
                    [InitAction::StoreScalar { path, value, .. }] if path.is_root() => {
                        Some(self.alloc(CHirExpr::Cast(o.ty, value, o.span)))
                    }
                    _ => None,
                };
                let replacement = value.and_then(|value| self.repeatable_value(value, o.ty));
                let repeatable = replacement.is_some();
                let qualifiers = self.unit.types.get(o.ty).unwrap().qualifiers;
                if repeatable
                    && !c.mutable.contains(&original)
                    && !qualifiers.is_volatile
                    && !qualifiers.is_atomic
                    && o.asm_label.is_none()
                {
                    c.constants.insert(original, replacement.unwrap());
                }
                // Only a straight-line, repeatable declaration prefix is
                // known to dominate every later use. A statement-expression
                // initializer can jump past a subsequent declaration.
                if !repeatable {
                    c.declaration_prefix = false;
                }
            }
        }
        Ok(vec![self.alloc(CHirDecl::Object(o))])
    }
    /// Instantiating a local type must rename its captured sizes as well as
    /// its object uses. Tags retain their nominal identity; VM record fields
    /// are not admitted by C, so only pointer/array layers need rebuilding.
    fn substituted_type(&mut self, ty: TypeId, c: &Context<'a>) -> TypeId {
        if c.replacements.is_empty() {
            return ty;
        }
        let Some(mut canonical) = self.unit.types.get(ty).cloned() else {
            return ty;
        };
        match &mut canonical.kind {
            TypeKind::Pointer(pointee) => *pointee = self.substituted_type(*pointee, c),
            TypeKind::Array { element, bound } => {
                *element = self.substituted_type(*element, c);
                *bound = self.substituted_bound(bound, c);
            }
            _ => return ty,
        }
        self.unit.types.intern(canonical)
    }

    fn substituted_bound(
        &self,
        bound: &evocc_types::ArrayBound,
        c: &Context<'a>,
    ) -> evocc_types::ArrayBound {
        let evocc_types::ArrayBound::Captured(raw) = bound else {
            return bound.clone();
        };
        self.unit
            .symbols
            .symbols_in_source_order()
            .get(*raw as usize)
            .map(|symbol| evocc_types::ArrayBound::Captured(self.symbol(symbol.id(), c).raw()))
            .unwrap_or_else(|| bound.clone())
    }

    fn expr(
        &mut self,
        e: &'a CHirExpr<'a>,
        c: &mut Context<'a>,
    ) -> Result<&'a CHirExpr<'a>, InlineError> {
        let key = (e as *const _ as usize, c.expand_calls);
        if let Some(e) = c.memo.get(&key) {
            return Ok(e);
        }
        if let CHirExpr::Ident { symbol, facts, .. } = e {
            if let Some(v) = c.constants.get(symbol) {
                return Ok(v);
            }
            if c.expand_calls {
                if let Some(v) = c.optimized_constants.get(symbol) {
                    return Ok(v);
                }
                if let Some(v) = self.static_values.get(symbol) {
                    return Ok(v);
                }
            }
            let root = self.object_root(*symbol, c, e.span())?;
            if matches!(root, PlaceRoot::PointerExpression { .. }) {
                let ty = self.substituted_type(facts.ty(), c);
                let place =
                    MemoryPlace::derive(&self.unit.symbols, &self.unit.types, root, Vec::new(), ty)
                        .map_err(|err| self.error(e.span(), err))?;
                let value = self.alloc(CHirExpr::MemoryRead {
                    place,
                    facts: ExprFacts::value(ty),
                    target_span: e.span(),
                    span: e.span(),
                });
                c.memo.insert(key, value);
                return Ok(value);
            }
        }
        let mut out = e.clone();
        match &mut out {
            CHirExpr::Ident {
                symbol,
                spelling,
                facts,
                ..
            } => {
                *symbol = self.symbol(*symbol, c);
                *facts = facts.with_type(self.substituted_type(facts.ty(), c));
                *spelling = self
                    .unit
                    .symbols
                    .symbol(*symbol)
                    .map_err(|err| self.error(e.span(), err))?
                    .spelling()
                    .to_owned();
            }
            CHirExpr::LabelAddress { label, .. } => *label = self.label(label, c),
            CHirExpr::Cast(ty, operand, _) => {
                *ty = self.substituted_type(*ty, c);
                *operand = self.expr(operand, c)?;
            }
            CHirExpr::Unary(_, e, _)
            | CHirExpr::EvaluateOnce(e, _)
            | CHirExpr::Member(e, _, _)
            | CHirExpr::PtrMember(e, _, _)
            | CHirExpr::BitBuiltin { operand: e, .. } => *e = self.expr(e, c)?,
            CHirExpr::BuiltinConstantP { operand, .. } => {
                // The operand is unevaluated. Substitute object bindings, but
                // do not inline calls or create executed argument temporaries.
                let expand = c.expand_calls;
                c.expand_calls = false;
                let rewritten = self.expr(operand, c);
                c.expand_calls = expand;
                *operand = rewritten?;
            }
            CHirExpr::Binary(_, a, b, _)
            | CHirExpr::Index(a, b, _)
            | CHirExpr::Comma(a, b, _)
            | CHirExpr::Elvis(a, b, _) => {
                *a = self.expr(a, c)?;
                *b = self.expr(b, c)?;
            }
            CHirExpr::Ternary(a, b, d, _) => {
                *a = self.expr(a, c)?;
                *b = self.expr(b, c)?;
                *d = self.expr(d, c)?;
            }
            CHirExpr::Call(f, args, span) => {
                for a in args.iter_mut() {
                    *a = self.expr(a, c)?;
                }
                if let CHirExpr::Ident { symbol, .. } = f {
                    if let Some(callee) = self
                        .eligible
                        .get(symbol)
                        .copied()
                        .filter(|_| c.expand_calls)
                    {
                        let result = self.call(callee, args, *span, c.scope)?;
                        c.memo.insert(key, result);
                        return Ok(result);
                    }
                }
                *f = self.expr(f, c)?;
            }
            CHirExpr::CheckedArithmetic {
                lhs, rhs, target, ..
            } => {
                *lhs = self.expr(lhs, c)?;
                *rhs = self.expr(rhs, c)?;
                *target = self.expr(target, c)?;
            }
            CHirExpr::BuiltinVarArgs {
                first,
                second,
                requested_ty,
                facts,
                ..
            } => {
                *first = self.expr(first, c)?;
                *second = second.map(|e| self.expr(e, c)).transpose()?;
                *requested_ty = requested_ty.map(|ty| self.substituted_type(ty, c));
                *facts = facts.with_type(self.substituted_type(facts.ty(), c));
            }
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                facts,
                ..
            } => {
                *place = ScalarPlace::bind(
                    &self.unit.symbols,
                    &self.unit.types,
                    self.symbol(place.symbol(), c),
                )
                .map_err(|err| self.error(e.span(), err))?;
                *target = self.expr(target, c)?;
                *value = self.expr(value, c)?;
                if let CHirExpr::MemoryRead { place, .. } = target {
                    out = CHirExpr::MemoryAssign {
                        op: *op,
                        place: place.clone(),
                        target_span: target.span(),
                        value,
                        facts: ExprFacts::value(place.value_ty()),
                        span: e.span(),
                    };
                } else {
                    *facts = facts.with_type(place.ty());
                }
            }
            CHirExpr::Update {
                op,
                place,
                target,
                facts,
                ..
            } => {
                *place = ScalarPlace::bind(
                    &self.unit.symbols,
                    &self.unit.types,
                    self.symbol(place.symbol(), c),
                )
                .map_err(|err| self.error(e.span(), err))?;
                *target = self.expr(target, c)?;
                if let CHirExpr::MemoryRead { place, .. } = target {
                    out = CHirExpr::MemoryUpdate {
                        op: *op,
                        place: place.clone(),
                        target_span: target.span(),
                        facts: ExprFacts::value(place.value_ty()),
                        span: e.span(),
                    };
                } else {
                    *facts = facts.with_type(place.ty());
                }
            }
            CHirExpr::MemoryAssign {
                place,
                value,
                facts,
                ..
            } => {
                *place = self.place(place, c, e.span())?;
                *facts = facts.with_type(self.substituted_type(facts.ty(), c));
                *value = self.expr(value, c)?;
            }
            CHirExpr::MemoryRead { place, facts, .. }
            | CHirExpr::MemoryUpdate { place, facts, .. } => {
                *place = self.place(place, c, e.span())?;
                *facts = facts.with_type(self.substituted_type(facts.ty(), c));
            }
            CHirExpr::StatementExpr {
                body, value, facts, ..
            } => {
                *body = self.stmt(body, c)?;
                *facts = facts.with_type(self.substituted_type(facts.ty(), c));
                *value = match body {
                    CHirStmt::Compound(items, _) => items.last().and_then(|item| match item {
                        CHirBlockItem::Statement(statement) => {
                            statement_expression_value(statement)
                        }
                        CHirBlockItem::Declaration(_) => None,
                    }),
                    _ => None,
                };
            }
            CHirExpr::CompoundLit(ty, i, _) => {
                *ty = self.substituted_type(*ty, c);
                *i = self.initializer(i, c, c.expand_calls)?;
            }
            CHirExpr::Lit(..)
            | CHirExpr::FloatLit(..)
            | CHirExpr::CharLit(..)
            | CHirExpr::StringLit(..)
            | CHirExpr::SizeOf(..)
            | CHirExpr::SizeOfExpr(..)
            | CHirExpr::AlignOf(..)
            | CHirExpr::BuiltinTrap { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { .. } => {}
        }
        let out = self.alloc(out);
        c.memo.insert(key, out);
        Ok(out)
    }
    fn stmt(
        &mut self,
        s: &'a CHirStmt<'a>,
        c: &mut Context<'a>,
    ) -> Result<&'a CHirStmt<'a>, InlineError> {
        let mut out = s.clone();
        match &mut out {
            CHirStmt::Compound(items, _) => {
                let mut result = Vec::new();
                for i in items.iter() {
                    match i {
                        CHirBlockItem::Declaration(d) => result
                            .extend(self.decl(d, c)?.into_iter().map(CHirBlockItem::Declaration)),
                        CHirBlockItem::Statement(s) => {
                            c.declaration_prefix = false;
                            result.push(CHirBlockItem::Statement(self.stmt(s, c)?))
                        }
                    }
                }
                *items = result;
            }
            CHirStmt::Return(e, span) if c.end.is_some() => {
                let mut items = Vec::new();
                if let Some(e) = e {
                    let value = self.expr(e, c)?;
                    let expression = if let Some(result) = c.result {
                        let target = self.ident(result, *span);
                        let ty = self.unit.symbols.symbol(result).unwrap().ty();
                        let value = if matches!(
                            self.unit.types.get(ty).unwrap().kind,
                            TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList
                        ) {
                            value
                        } else {
                            self.alloc(CHirExpr::Cast(ty, value, *span))
                        };
                        if let Ok(place) =
                            ScalarPlace::bind(&self.unit.symbols, &self.unit.types, result)
                        {
                            self.alloc(CHirExpr::Assign {
                                op: AssignmentOp::Simple,
                                place,
                                target,
                                value,
                                facts: ExprFacts::value(ty),
                                span: *span,
                            })
                        } else {
                            let place = MemoryPlace::derive(
                                &self.unit.symbols,
                                &self.unit.types,
                                PlaceRoot::Object { symbol: result },
                                Vec::new(),
                                ty,
                            )
                            .map_err(|err| self.error(*span, err))?;
                            self.alloc(CHirExpr::MemoryAssign {
                                op: AssignmentOp::Simple,
                                place,
                                target_span: *span,
                                value,
                                facts: ExprFacts::value(ty),
                                span: *span,
                            })
                        }
                    } else {
                        value
                    };
                    items.push(CHirBlockItem::Statement(
                        self.alloc(CHirStmt::Expr(expression, *span)),
                    ));
                }
                items.push(CHirBlockItem::Statement(
                    self.alloc(CHirStmt::Goto(c.end.clone().unwrap(), *span)),
                ));
                return Ok(self.alloc(CHirStmt::Compound(items, *span)));
            }
            CHirStmt::Expr(e, _) | CHirStmt::ComputedGoto(e, _) => *e = self.expr(e, c)?,
            CHirStmt::Return(e, _) => *e = e.map(|e| self.expr(e, c)).transpose()?,
            CHirStmt::If(e, a, b, _) => {
                *e = self.expr(e, c)?;
                *a = self.stmt(a, c)?;
                *b = b.map(|b| self.stmt(b, c)).transpose()?;
            }
            CHirStmt::Switch(e, b, _) | CHirStmt::While(e, b, _) | CHirStmt::Case(e, b, _) => {
                *e = self.expr(e, c)?;
                *b = self.stmt(b, c)?;
            }
            CHirStmt::DoWhile(b, e, _) => {
                *b = self.stmt(b, c)?;
                *e = self.expr(e, c)?;
            }
            CHirStmt::For(i, test, update, body, _) => {
                match i {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(e) => *e = self.expr(e, c)?,
                    CHirForInit::Declaration(ds) => {
                        let mut result = Vec::new();
                        for d in ds.iter() {
                            result.extend(self.decl(d, c)?);
                        }
                        *ds = result;
                    }
                }
                *test = test.map(|e| self.expr(e, c)).transpose()?;
                *update = update.map(|e| self.expr(e, c)).transpose()?;
                *body = self.stmt(body, c)?;
            }
            CHirStmt::Label(label, b, _) => {
                *label = self.label(label, c);
                *b = self.stmt(b, c)?;
            }
            CHirStmt::Goto(label, _) => *label = self.label(label, c),
            CHirStmt::Default(b, _) => *b = self.stmt(b, c)?,
            CHirStmt::CaseRange(a, b, s, _) => {
                *a = self.expr(a, c)?;
                *b = self.expr(b, c)?;
                *s = self.stmt(s, c)?;
            }
            CHirStmt::Asm(a) => {
                for o in a.inputs.iter_mut().chain(&mut a.outputs) {
                    o.expression = self.expr(o.expression, c)?;
                }
                if c.prefix.is_some() {
                    let labels: BTreeMap<_, _> = a
                        .labels
                        .iter()
                        .map(|l| (l.spelling.clone(), self.label(l, c).spelling))
                        .collect();
                    a.template = rename_assembly_labels(&a.template, &labels);
                }
                for l in &mut a.labels {
                    *l = self.label(l, c);
                }
            }
            CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Null(_)
            | CHirStmt::AttributedNull(..) => {}
        }
        Ok(self.alloc(out))
    }
    fn call(
        &mut self,
        f: &'a FunctionDecl<'a>,
        args: &[&'a CHirExpr<'a>],
        span: SourceSpan,
        scope: ScopeId,
    ) -> Result<&'a CHirExpr<'a>, InlineError> {
        if self.active.contains(&f.symbol) {
            return Err(InlineError {
                span,
                cycle: true,
                message: "always-inline-cycle".into(),
            });
        }
        let TypeKind::Function { return_type, .. } = self.unit.types.get(f.ty).unwrap().kind else {
            return Err(self.error(span, "callee is not a function"));
        };
        let parameters = match &f.parameters {
            ParameterSyntax::Prototype { parameters, .. }
            | ParameterSyntax::IdentifierList { parameters } => parameters.as_slice(),
            ParameterSyntax::Unspecified => &[],
        };
        if parameters.len() != args.len() {
            return Err(self.error(span, "inline argument arity mismatch"));
        }
        // A literal call to a single-return leaf has no parameter addresses,
        // local lifetime or body effects. Fold its substituted return value
        // without introducing a result object that hides the constant again.
        if let CHirStmt::Compound(items, _) = f.body.unwrap() {
            if let [CHirBlockItem::Statement(CHirStmt::Return(Some(value), _))] = items.as_slice() {
                let mut constants = Context::new(scope);
                constants.expand_calls = false;
                let mut all_constant = true;
                for (parameter, argument) in parameters.iter().zip(args) {
                    let qualifiers = self.unit.types.get(parameter.ty).unwrap().qualifiers;
                    if qualifiers.is_volatile || qualifiers.is_atomic {
                        all_constant = false;
                        break;
                    }
                    let converted = self.alloc(CHirExpr::Cast(parameter.ty, argument, span));
                    if crate::constant::eval_executable_integer_constant(
                        converted,
                        &self.unit.types,
                        &self.unit.symbols,
                        self.target,
                    )
                    .is_none()
                    {
                        all_constant = false;
                        break;
                    }
                    if let Some(symbol) = parameter.symbol {
                        if let Some(value) = self.repeatable_value(converted, parameter.ty) {
                            constants.constants.insert(symbol, value);
                        } else {
                            all_constant = false;
                        }
                    }
                }
                if all_constant && !inspect([Node::Expr(value, false)]).owns_statements {
                    let value = self.expr(value, &mut constants)?;
                    let converted = self.alloc(CHirExpr::Cast(return_type, value, span));
                    if crate::constant::eval_executable_integer_constant(
                        converted,
                        &self.unit.types,
                        &self.unit.symbols,
                        self.target,
                    )
                    .is_some()
                    {
                        if let Some(value) = self.repeatable_value(converted, return_type) {
                            return Ok(value);
                        }
                    }
                }
            }
        }
        if args.is_empty() {
            if let Some(value) = constant_inline_return(&self.unit, f, self.target) {
                // Retain the conversion at the return boundary. In particular,
                // a nonzero integer returned as _Bool must become one.
                let converted = self.alloc(CHirExpr::Cast(return_type, value, span));
                // Copy only the value. The source expression can contain an
                // unselected statement expression with callee-owned objects.
                return self
                    .repeatable_value(converted, return_type)
                    .ok_or_else(|| self.error(span, "constant inline result was not repeatable"));
            }
        }
        let body = f.body.unwrap();
        let uses = inspect([Node::Stmt(body)]);
        let inline_scope = self
            .unit
            .symbols
            .create_scope(scope, ScopeKind::Block, Some(span))
            .map_err(|e| self.error(span, e))?;
        let mut c = Context::new(inline_scope);
        c.mutable = uses.mutable;
        c.declaration_prefix = true;
        let serial = self.next();
        c.prefix = Some(serial);
        c.end = Some(SpannedName {
            spelling: format!("__evocc_inline_{serial}_end"),
            span,
        });
        // Parameter types can refer forward to captures initialized at body
        // entry. Give these identities a home before copying the parameters;
        // their ordinary declarations still evaluate the bounds afterward.
        let mut captures = BTreeSet::new();
        for parameter in parameters {
            let mut ty = parameter.ty;
            loop {
                match &self.unit.types.get(ty).unwrap().kind {
                    TypeKind::Pointer(pointee) => ty = *pointee,
                    TypeKind::Array { element, bound } => {
                        if let evocc_types::ArrayBound::Captured(raw) = bound {
                            captures.insert(*raw);
                        }
                        ty = *element;
                    }
                    _ => break,
                }
            }
        }
        for raw in captures {
            let original = self
                .unit
                .symbols
                .symbols_in_source_order()
                .get(raw as usize)
                .ok_or_else(|| self.error(span, "parameter size capture has no symbol"))?;
            let (symbol, ty) = (original.id(), original.ty());
            let fresh = self.fresh(inline_scope, ty, StorageClass::Auto, span)?;
            c.replacements.insert(symbol, fresh);
        }
        let mut items = Vec::new();
        if !matches!(
            self.unit.types.get(return_type).unwrap().kind,
            TypeKind::Void
        ) {
            let s = self.fresh(scope, return_type, StorageClass::None, span)?;
            c.result = Some(s);
            let object = self.object(s, None, span);
            if matches!(
                self.unit.types.get(return_type).unwrap().kind,
                TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList
            ) {
                // Like the existing call-result buffer, an aggregate result
                // belongs to the caller, not the callee's ended block.
                self.temporaries.push(object);
            } else {
                items.push(CHirBlockItem::Declaration(object));
            }
        }
        for (parameter, arg) in parameters.iter().zip(args) {
            let ty = self.substituted_type(parameter.ty, &c);
            let fresh = self.fresh(inline_scope, ty, StorageClass::None, span)?;
            let arg = if matches!(
                self.unit.types.get(ty).unwrap().kind,
                TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList
            ) {
                *arg
            } else {
                self.alloc(CHirExpr::Cast(ty, arg, span))
            };
            items.push(CHirBlockItem::Declaration(self.object(
                fresh,
                Some(arg),
                span,
            )));
            if let Some(original) = parameter.symbol {
                c.replacements.insert(original, fresh);
                let qualifiers = self.unit.types.get(ty).unwrap().qualifiers;
                if !c.mutable.contains(&original)
                    && !qualifiers.is_volatile
                    && !qualifiers.is_atomic
                {
                    if let Some(replacement) = self.repeatable_value(arg, ty) {
                        c.constants.insert(original, replacement);
                    }
                }
            }
        }
        self.active.push(f.symbol);
        let body = self.stmt(body, &mut c)?;
        self.active.pop();
        items.push(CHirBlockItem::Statement(body));
        let end = self.alloc(CHirStmt::Null(span));
        items.push(CHirBlockItem::Statement(self.alloc(CHirStmt::Label(
            c.end.unwrap(),
            end,
            span,
        ))));
        let value = c.result.map(|s| self.ident(s, span));
        if let Some(value) = value {
            items.push(CHirBlockItem::Statement(
                self.alloc(CHirStmt::Expr(value, span)),
            ));
        }
        let body = self.alloc(CHirStmt::Compound(items, span));
        Ok(self.alloc(CHirExpr::StatementExpr {
            body,
            value,
            facts: ExprFacts::value(return_type),
            span,
        }))
    }
}

fn rename_assembly_labels(template: &str, labels: &BTreeMap<String, String>) -> String {
    let mut remaining = template;
    let mut output = String::new();
    while let Some(index) = remaining.find('%') {
        output.push_str(&remaining[..index]);
        remaining = &remaining[index..];
        if let Some(rest) = remaining.strip_prefix("%%") {
            output.push_str("%%");
            remaining = rest;
        } else if let Some((name, rest)) = remaining
            .strip_prefix("%l[")
            .and_then(|s| s.split_once(']'))
        {
            output.push_str("%l[");
            output.push_str(labels.get(name).map_or(name, String::as_str));
            output.push(']');
            remaining = rest;
        } else {
            output.push('%');
            remaining = &remaining[1..];
        }
    }
    output.push_str(remaining);
    output
}
