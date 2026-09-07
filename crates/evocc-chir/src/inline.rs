//! Source-level substitution of explicitly always-inline calls.
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
    addresses: BTreeSet<SymbolId>,
    mutable: BTreeSet<SymbolId>,
    statics: BTreeMap<SymbolId, &'a ObjectDecl<'a>>,
}

// Inspect semantic expression roots only. In particular, an initializer's
// written form and a statement expression's value are not second executions.
fn inspect<'a>(nodes: impl IntoIterator<Item = Node<'a>>) -> Uses<'a> {
    let mut work: Vec<_> = nodes.into_iter().collect();
    let mut uses = Uses::default();
    while let Some(node) = work.pop() {
        match node {
            Node::Decl(decl) => match decl {
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
                    for o in a.inputs.iter().chain(&a.outputs) {
                        work.push(Node::Expr(o.expression, false));
                    }
                }
                CHirDecl::Tag(_) | CHirDecl::Typedef(_) => {}
            },
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
                    for o in &a.outputs {
                        if let CHirExpr::Ident { symbol, .. } = o.expression {
                            uses.mutable.insert(*symbol);
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
                    | CHirExpr::AlignOf(..)
                    | CHirExpr::BuiltinUnreachable { .. }
                    | CHirExpr::BuiltinReturnAddress { .. }
                    | CHirExpr::BuiltinFrameAddress { .. } => {}
                }
            }
        }
    }
    uses
}

pub fn has_inline_definitions(unit: &CHirUnit<'_>) -> bool {
    unit.iter().any(|d| matches!(d, CHirDecl::Function(f) if f.body.is_some() && f.attributes.iter().any(|a| a.name == "always_inline")))
}

/// Rewrite into the caller-owned arena. Original nodes and symbol tables are
/// left untouched, so the original interpreter remains a differential oracle.
pub fn substitute<'a>(
    unit: &CHirUnit<'a>,
    arena: &'a Arena,
    target: &TargetDataLayout,
) -> Result<CHirUnit<'a>, InlineError> {
    let uses = inspect(unit.iter().map(Node::Decl));
    let eligible: BTreeMap<_, _> = unit
        .iter()
        .filter_map(|d| match d {
            CHirDecl::Function(f)
                if f.body.is_some()
                    && f.attributes.iter().any(|a| a.name == "always_inline")
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
        serial: 0,
        active: Vec::new(),
        temporaries: Vec::new(),
    };
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
                pass.temporaries.clear();
                pass.active.push(f.symbol);
                let mut rewritten = pass.stmt(body, &mut c)?;
                pass.active.pop();
                if f.storage == StorageClass::Static && pass.eligible.contains_key(&f.symbol) {
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
    Ok(pass.unit)
}

struct Context<'a> {
    scope: ScopeId,
    replacements: BTreeMap<SymbolId, SymbolId>,
    constants: BTreeMap<SymbolId, &'a CHirExpr<'a>>,
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
    fn object(
        &self,
        s: SymbolId,
        value: Option<&'a CHirExpr<'a>>,
        span: SourceSpan,
    ) -> &'a CHirDecl<'a> {
        let symbol = self.unit.symbols.symbol(s).unwrap();
        let ty = symbol.ty();
        let initializer = value.map(|v| {
            let action = if matches!(
                self.unit.types.get(ty).unwrap().kind,
                TypeKind::Struct(_) | TypeKind::Union(_)
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
    fn place(
        &mut self,
        p: &MemoryPlace<'a>,
        c: &mut Context<'a>,
        span: SourceSpan,
    ) -> Result<MemoryPlace<'a>, InlineError> {
        let root = match p.root() {
            PlaceRoot::Object { symbol } => PlaceRoot::Object {
                symbol: self.symbol(*symbol, c),
            },
            PlaceRoot::PointerExpression {
                pointer,
                pointer_type,
            } => PlaceRoot::PointerExpression {
                pointer: self.expr(pointer, c)?,
                pointer_type: *pointer_type,
            },
            PlaceRoot::CompoundLiteral { expression, ty } => PlaceRoot::CompoundLiteral {
                expression: self.expr(expression, c)?,
                ty: *ty,
            },
        };
        let mut projections = Vec::new();
        for projection in p.projections() {
            projections.push(match projection {
                PlaceProjection::Field(f) => PlaceProjection::Field(*f),
                PlaceProjection::Index(i) => PlaceProjection::Index(IndexProjection::new(
                    self.expr(i.index(), c)?,
                    i.element_ty(),
                    i.promoted_index(),
                )),
            });
        }
        MemoryPlace::derive(
            &self.unit.symbols,
            &self.unit.types,
            root,
            projections,
            p.value_ty(),
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
            if let InitAction::StoreScalar { value, .. } | InitAction::CopyAggregate { value, .. } =
                a
            {
                *value = self.expr(value, c)?;
            }
        }
        let mut plan = InitPlan::new(i.plan.target_type(), actions)
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
        let mut o = o.clone();
        if c.prefix.is_some() && o.storage == StorageClass::Extern {
            self.unit
                .symbols
                .bind_existing(c.scope, o.symbol)
                .map_err(|e| self.error(o.span, e))?;
        }
        if c.prefix.is_some() && o.storage != StorageClass::Extern {
            let s = self.fresh(c.scope, o.ty, o.storage, o.span)?;
            c.replacements.insert(o.symbol, s);
            o.symbol = s;
            o.name.spelling = self.unit.symbols.symbol(s).unwrap().spelling().to_owned();
        }
        if let Some(i) = &o.initializer {
            let runtime =
                o.storage != StorageClass::Static && c.scope != self.unit.symbols.file_scope();
            o.initializer = Some(self.initializer(i, c, runtime)?);
        }
        Ok(vec![self.alloc(CHirDecl::Object(o))])
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
        if let CHirExpr::Ident { symbol, .. } = e {
            if let Some(v) = c.constants.get(symbol) {
                return Ok(v);
            }
        }
        let mut out = e.clone();
        match &mut out {
            CHirExpr::Ident {
                symbol, spelling, ..
            } => {
                *symbol = self.symbol(*symbol, c);
                *spelling = self
                    .unit
                    .symbols
                    .symbol(*symbol)
                    .map_err(|err| self.error(e.span(), err))?
                    .spelling()
                    .to_owned();
            }
            CHirExpr::LabelAddress { label, .. } => *label = self.label(label, c),
            CHirExpr::Unary(_, e, _)
            | CHirExpr::Cast(_, e, _)
            | CHirExpr::EvaluateOnce(e, _)
            | CHirExpr::Member(e, _, _)
            | CHirExpr::PtrMember(e, _, _)
            | CHirExpr::BitBuiltin { operand: e, .. } => *e = self.expr(e, c)?,
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
            CHirExpr::BuiltinVarArgs { first, second, .. } => {
                *first = self.expr(first, c)?;
                *second = second.map(|e| self.expr(e, c)).transpose()?;
            }
            CHirExpr::Assign {
                place,
                target,
                value,
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
            }
            CHirExpr::Update { place, target, .. } => {
                *place = ScalarPlace::bind(
                    &self.unit.symbols,
                    &self.unit.types,
                    self.symbol(place.symbol(), c),
                )
                .map_err(|err| self.error(e.span(), err))?;
                *target = self.expr(target, c)?;
            }
            CHirExpr::MemoryAssign { place, value, .. } => {
                *place = self.place(place, c, e.span())?;
                *value = self.expr(value, c)?;
            }
            CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
                *place = self.place(place, c, e.span())?
            }
            CHirExpr::StatementExpr { body, value, .. } => {
                *body = self.stmt(body, c)?;
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
            CHirExpr::CompoundLit(_, i, _) => *i = self.initializer(i, c, c.expand_calls)?,
            CHirExpr::Lit(..)
            | CHirExpr::FloatLit(..)
            | CHirExpr::CharLit(..)
            | CHirExpr::StringLit(..)
            | CHirExpr::SizeOf(..)
            | CHirExpr::SizeOfExpr(..)
            | CHirExpr::AlignOf(..)
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
                            TypeKind::Struct(_) | TypeKind::Union(_)
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
            ParameterSyntax::Prototype { parameters, .. } => parameters.as_slice(),
            ParameterSyntax::Unspecified => &[],
        };
        if parameters.len() != args.len() {
            return Err(self.error(span, "inline argument arity mismatch"));
        }
        let body = f.body.unwrap();
        let uses = inspect([Node::Stmt(body)]);
        let inline_scope = self
            .unit
            .symbols
            .create_scope(scope, ScopeKind::Block, Some(span))
            .map_err(|e| self.error(span, e))?;
        let mut c = Context::new(inline_scope);
        let serial = self.next();
        c.prefix = Some(serial);
        c.end = Some(SpannedName {
            spelling: format!("__evocc_inline_{serial}_end"),
            span,
        });
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
                TypeKind::Struct(_) | TypeKind::Union(_)
            ) {
                // Like the existing call-result buffer, an aggregate result
                // belongs to the caller, not the callee's ended block.
                self.temporaries.push(object);
            } else {
                items.push(CHirBlockItem::Declaration(object));
            }
        }
        for (parameter, arg) in parameters.iter().zip(args) {
            let fresh = self.fresh(inline_scope, parameter.ty, StorageClass::None, span)?;
            let arg = if matches!(
                self.unit.types.get(parameter.ty).unwrap().kind,
                TypeKind::Struct(_) | TypeKind::Union(_)
            ) {
                *arg
            } else {
                self.alloc(CHirExpr::Cast(parameter.ty, arg, span))
            };
            items.push(CHirBlockItem::Declaration(self.object(
                fresh,
                Some(arg),
                span,
            )));
            if let Some(original) = parameter.symbol {
                c.replacements.insert(original, fresh);
                let qualifiers = self.unit.types.get(parameter.ty).unwrap().qualifiers;
                if !uses.mutable.contains(&original)
                    && !qualifiers.is_volatile
                    && !qualifiers.is_atomic
                    && crate::constant::eval_integer_constant(
                        arg,
                        &self.unit.types,
                        &self.unit.symbols,
                        self.target,
                    )
                    .is_some()
                {
                    c.constants.insert(original, arg);
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
