//! Extent transport uses ordinary C-HIR integer temporaries. No pointer
//! representation is inspected and no query evaluates its source operand.

use super::*;
use evocc_types::{ArrayBound, CanonicalType, FunctionParameters, IntRank, Qualifiers};

type Expr<'a> = &'a CHirExpr<'a>;
// Start, byte size, and whether this extent is known on the executed path.
// The last component matters at an inline return join: an unknown arm must
// not borrow an allocation fact from a different return.
type Extent<'a> = (Expr<'a>, Expr<'a>, Expr<'a>);

#[derive(Clone, Copy)]
pub(super) struct Bounds<'a> {
    offset: Expr<'a>,
    whole: Option<Extent<'a>>,
    member: Option<Extent<'a>>,
}

#[derive(Clone, Copy)]
pub(super) struct ReturnBounds {
    slots: [SymbolId; 7],
}

pub(super) fn allocation_sizes(unit: &CHirUnit<'_>) -> BTreeMap<SymbolId, Vec<usize>> {
    let mut result = BTreeMap::new();
    for_each_declaration(unit, |declaration| {
        let CHirDecl::Function(function) = declaration else {
            return;
        };
        // GNU declarations also place function allocation attributes between
        // the return pointer and the function name (devm_kmalloc's spelling).
        // They remain in the written declarator, not the declaration's list.
        // Do not import attributes from a nested returned function type.
        let return_pointer_attributes = match function.written.layers.as_slice() {
            [WrittenDeclaratorLayer::Function { .. }, WrittenDeclaratorLayer::Pointer { attributes, .. }, ..] => {
                attributes.as_slice()
            }
            _ => &[],
        };
        for attribute in function.attributes.iter().chain(return_pointer_attributes) {
            if attribute.semantic_name() != Some("alloc_size") {
                continue;
            }
            let indices: Option<Vec<_>> = attribute
                .arguments
                .iter()
                .map(|argument| {
                    let index = argument
                        .integer
                        .or_else(|| argument.text.trim().parse().ok())?;
                    usize::try_from(index.checked_sub(1)?).ok()
                })
                .collect();
            if let Some(indices) = indices.filter(|v| matches!(v.len(), 1 | 2)) {
                result.insert(function.symbol, indices);
            }
        }
    });
    result
}

impl<'a> Substitution<'a, '_> {
    /// Private caller-lifetime objects implement the ordinary value join.
    /// They carry no pointer authority, and are written only on the return
    /// path that actually executes, after its pointer expression is evaluated.
    pub(super) fn new_return_bounds(
        &mut self,
        span: SourceSpan,
    ) -> Result<ReturnBounds, InlineError> {
        let ty = self.size_type();
        let mut slots = Vec::new();
        for _ in 0..7 {
            let symbol = self.fresh(self.extent_scope, ty, StorageClass::None, span)?;
            self.temporaries.push(self.object(symbol, None, span));
            slots.push(symbol);
        }
        Ok(ReturnBounds {
            slots: slots.try_into().unwrap(),
        })
    }

    pub(super) fn bind_return_bounds(
        &self,
        destination: ReturnBounds,
        bounds: Option<Bounds<'a>>,
        span: SourceSpan,
    ) -> Result<Vec<CHirBlockItem<'a>>, InlineError> {
        let zero = self.size_value(0, span);
        let values = bounds.map_or([zero; 7], |bounds| {
            let whole = bounds.whole.unwrap_or((zero, zero, zero));
            let member = bounds.member.unwrap_or((zero, zero, zero));
            [
                bounds.offset,
                whole.0,
                whole.1,
                whole.2,
                member.0,
                member.1,
                member.2,
            ]
        });
        destination
            .slots
            .into_iter()
            .zip(values)
            .map(|(symbol, value)| {
                let place = ScalarPlace::bind(&self.unit.symbols, &self.unit.types, symbol)
                    .map_err(|error| self.error(span, error))?;
                let ty = place.ty();
                Ok(CHirBlockItem::Statement(self.alloc(CHirStmt::Expr(
                    self.alloc(CHirExpr::Assign {
                        op: AssignmentOp::Simple,
                        place,
                        target: self.ident(symbol, span),
                        value,
                        facts: ExprFacts::value(ty),
                        span,
                    }),
                    span,
                ))))
            })
            .collect()
    }

    pub(super) fn returned_bounds(&self, slots: ReturnBounds, span: SourceSpan) -> Bounds<'a> {
        let [offset, ws, wn, wk, ms, mn, mk] = slots.slots.map(|s| self.ident(s, span));
        Bounds {
            offset,
            whole: Some((ws, wn, wk)),
            member: Some((ms, mn, mk)),
        }
    }

    /// Analyze the already evaluated body of a statement expression. The
    /// returned facts reference allocation-time captures, never a replay of
    /// the allocation or its argument effects. Unknown control flow drops
    /// facts for its writes; a goto cannot establish a dominating assignment.
    pub(super) fn statement_result_bounds(
        &mut self,
        body: &'a CHirStmt<'a>,
        value: Expr<'a>,
        outer: &Context<'a>,
    ) -> Option<Bounds<'a>> {
        let mut flow = Context::new(outer.scope);
        flow.object_bounds = outer.object_bounds.clone();
        flow.exposed_objects = inspect([Node::Stmt(body)]).exposed_objects;
        self.extent_statement(body, &mut flow, 0)?;
        self.pointer_bounds(value, &flow, 0)
    }

    fn extent_statement(
        &mut self,
        statement: &'a CHirStmt<'a>,
        flow: &mut Context<'a>,
        depth: usize,
    ) -> Option<()> {
        if depth >= 64 {
            return None;
        }
        match statement {
            CHirStmt::Compound(items, _) => {
                for item in items {
                    match item {
                        CHirBlockItem::Declaration(CHirDecl::Object(object)) => {
                            let uses = inspect([Node::Decl(match item {
                                CHirBlockItem::Declaration(declaration) => declaration,
                                _ => unreachable!(),
                            })]);
                            if uses.jumps {
                                return None;
                            }
                            flow.object_bounds
                                .retain(|symbol, _| !uses.mutable.contains(symbol));
                            flow.object_bounds.remove(&object.symbol);
                            let qualifiers = self.unit.types.get(object.ty)?.qualifiers;
                            if !flow.exposed_objects.contains(&object.symbol)
                                && !qualifiers.is_volatile
                                && !qualifiers.is_atomic
                            {
                                if let Some(initializer) = &object.initializer {
                                    if let [InitAction::StoreScalar { value, .. }] =
                                        initializer.plan().actions()
                                    {
                                        if let Some(bounds) = self.pointer_bounds(value, flow, 0) {
                                            flow.object_bounds.insert(object.symbol, bounds);
                                        }
                                    }
                                }
                            }
                        }
                        CHirBlockItem::Declaration(_) => {}
                        CHirBlockItem::Statement(child) => {
                            self.extent_statement(child, flow, depth + 1)?
                        }
                    }
                }
            }
            CHirStmt::Expr(expression, _) => {
                let uses = inspect([Node::Expr(expression, false)]);
                flow.object_bounds
                    .retain(|symbol, _| !uses.mutable.contains(symbol));
                if let CHirExpr::Assign {
                    op: AssignmentOp::Simple,
                    place,
                    value,
                    ..
                } = expression
                {
                    let symbol = place.symbol();
                    let ty = self.unit.types.get(place.ty())?;
                    let entry = self.unit.symbols.symbol(symbol).ok()?;
                    if !flow.exposed_objects.contains(&symbol)
                        && !ty.qualifiers.is_volatile
                        && !ty.qualifiers.is_atomic
                        && entry.scope() != self.unit.symbols.file_scope()
                        && matches!(
                            entry.storage_class(),
                            Some(StorageClass::None | StorageClass::Auto)
                        )
                    {
                        if let Some(bounds) = self.pointer_bounds(value, flow, 0) {
                            flow.object_bounds.insert(symbol, bounds);
                        }
                    }
                }
            }
            CHirStmt::If(test, yes, no, _) => {
                let uses = inspect([Node::Expr(test, false)]);
                if uses.jumps {
                    return None;
                }
                flow.object_bounds
                    .retain(|symbol, _| !uses.mutable.contains(symbol));
                if let Some(value) = crate::constant::eval_executable_integer_constant(
                    test,
                    &self.unit.types,
                    &self.unit.symbols,
                    self.target,
                ) {
                    if value.truth() {
                        self.extent_statement(yes, flow, depth + 1)?;
                    } else if let Some(no) = no {
                        self.extent_statement(no, flow, depth + 1)?;
                    }
                } else {
                    let uses = inspect([Node::Stmt(statement)]);
                    if uses.jumps {
                        return None;
                    }
                    flow.object_bounds
                        .retain(|symbol, _| !uses.mutable.contains(symbol));
                }
            }
            CHirStmt::Label(..)
            | CHirStmt::Goto(..)
            | CHirStmt::ComputedGoto(..)
            | CHirStmt::Return(..) => return None,
            _ => {
                let uses = inspect([Node::Stmt(statement)]);
                if uses.jumps {
                    return None;
                }
                flow.object_bounds
                    .retain(|symbol, _| !uses.mutable.contains(symbol));
            }
        }
        Some(())
    }

    fn size_value(&self, value: u64, span: SourceSpan) -> Expr<'a> {
        self.alloc(CHirExpr::Lit(
            IntLit {
                value,
                suffix: IntSuffix::UL,
                base: IntBase::Hex,
            },
            span,
        ))
    }

    fn size_type(&mut self) -> TypeId {
        self.unit.types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::ULong),
            qualifiers: Qualifiers::default(),
        })
    }

    fn size_binary(&self, op: BinaryOp, a: Expr<'a>, b: Expr<'a>) -> Expr<'a> {
        self.alloc(CHirExpr::Binary(op, a, b, a.span()))
    }

    fn size_choose(&self, condition: Expr<'a>, a: Expr<'a>, b: Expr<'a>) -> Expr<'a> {
        self.alloc(CHirExpr::Ternary(condition, a, b, condition.span()))
    }

    fn complete_bounds(&self, size: Expr<'a>) -> Bounds<'a> {
        let zero = self.size_value(0, size.span());
        let known = self.size_value(1, size.span());
        Bounds {
            offset: zero,
            whole: Some((zero, size, known)),
            member: Some((zero, size, known)),
        }
    }

    /// An assignment in the original evaluated expression captures the value;
    /// the query later reads only the private capture, not the original source.
    pub(super) fn capture_extent_value(
        &mut self,
        value: Expr<'a>,
        _scope: ScopeId,
    ) -> Result<Expr<'a>, InlineError> {
        if crate::constant::eval_integer_constant(
            value,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        )
        .is_some()
        {
            return Ok(value);
        }
        let integer =
            crate::constant::integer_expression_type(value, &self.unit.types, &self.unit.symbols)
                .ok_or_else(|| {
                self.error(value.span(), "object extent capture is not integer-valued")
            })?;
        let rank = [
            IntRank::Bool,
            IntRank::Char,
            IntRank::SChar,
            IntRank::UChar,
            IntRank::Short,
            IntRank::UShort,
            IntRank::Int,
            IntRank::UInt,
            IntRank::Long,
            IntRank::ULong,
            IntRank::LongLong,
            IntRank::ULongLong,
        ]
        .into_iter()
        .find(|rank| crate::integer::integer_type_for_rank(*rank) == integer)
        .ok_or_else(|| self.error(value.span(), "object extent capture exceeds pointer width"))?;
        let ty = self.unit.types.intern(CanonicalType {
            kind: TypeKind::Int(rank),
            qualifiers: Qualifiers::default(),
        });
        let symbol = self.fresh(self.extent_scope, ty, StorageClass::None, value.span())?;
        self.temporaries
            .push(self.object(symbol, None, value.span()));
        let target = self.ident(symbol, value.span());
        let place = ScalarPlace::bind(&self.unit.symbols, &self.unit.types, symbol)
            .map_err(|error| self.error(value.span(), error))?;
        let assignment = self.alloc(CHirExpr::Assign {
            op: AssignmentOp::Simple,
            place,
            target,
            value,
            facts: ExprFacts::value(ty),
            span: value.span(),
        });
        self.extent_captures
            .insert(assignment as *const _ as usize, target);
        Ok(assignment)
    }

    fn captured_value(&self, value: Expr<'a>) -> Expr<'a> {
        self.extent_captures
            .get(&(value as *const _ as usize))
            .copied()
            .unwrap_or(value)
    }

    pub(super) fn capture_allocation(
        &mut self,
        callee: Expr<'a>,
        args: &mut [Expr<'a>],
        scope: ScopeId,
    ) -> Result<Option<Bounds<'a>>, InlineError> {
        let CHirExpr::Ident { symbol, .. } = callee else {
            return Ok(None);
        };
        let Some(indices) = self.allocation_sizes.get(symbol).cloned() else {
            return Ok(None);
        };
        let parameter_types = self
            .unit
            .symbols
            .symbol(*symbol)
            .ok()
            .and_then(|entry| self.unit.types.get(entry.ty()))
            .and_then(|ty| match &ty.kind {
                TypeKind::Function {
                    parameters: FunctionParameters::Prototype { types, .. },
                    return_type,
                } if self
                    .unit
                    .types
                    .get(*return_type)
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Pointer(_))) =>
                {
                    Some(types.clone())
                }
                _ => None,
            });
        let Some(parameter_types) = parameter_types else {
            return Ok(None);
        };
        if indices.iter().any(|index| {
            *index >= args.len()
                || parameter_types.get(*index).is_none_or(|ty| {
                    crate::constant::canonical_integer_type(&self.unit.types, *ty).is_none()
                })
        }) {
            return Ok(None);
        }
        let mut size = self.size_value(1, callee.span());
        let ty = self.size_type();
        for index in indices {
            // Capture at this argument's ordinary evaluation position, including
            // its conversion; size++ must still execute exactly once.
            let converted = self.alloc(CHirExpr::Cast(
                parameter_types[index],
                args[index],
                args[index].span(),
            ));
            args[index] = self.capture_extent_value(converted, scope)?;
            let value = self.captured_value(args[index]);
            let value = self.alloc(CHirExpr::Cast(ty, value, value.span()));
            size = self.size_binary(BinaryOp::Mul, size, value);
        }
        Ok(Some(self.complete_bounds(size)))
    }

    fn type_size(&self, ty: TypeId, span: SourceSpan) -> Option<Expr<'a>> {
        let kind = &self.unit.types.get(ty)?.kind;
        match kind {
            TypeKind::Array {
                bound: ArrayBound::Unbounded | ArrayBound::Variable,
                ..
            } => None,
            TypeKind::Array {
                element,
                bound: ArrayBound::Captured(raw),
            } => {
                let symbol = self
                    .unit
                    .symbols
                    .symbols_in_source_order()
                    .get(*raw as usize)?
                    .id();
                Some(self.size_binary(
                    BinaryOp::Mul,
                    self.ident(symbol, span),
                    self.type_size(*element, span)?,
                ))
            }
            _ => Some(self.size_value(
                self.target.layout_of(&self.unit.types, ty).ok()?.size_bytes,
                span,
            )),
        }
    }

    fn is_array(&self, ty: TypeId) -> bool {
        matches!(
            self.unit.types.get(ty).map(|t| &t.kind),
            Some(TypeKind::Array { .. })
        )
    }

    fn pure_extent_integer(&self, expression: Expr<'a>) -> bool {
        if self
            .extent_captures
            .contains_key(&(expression as *const _ as usize))
        {
            return true;
        }
        if crate::constant::eval_integer_constant(
            expression,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        )
        .is_some()
        {
            return true;
        }
        match expression {
            CHirExpr::Ident { symbol, facts, .. } => self.unit.types.get(facts.ty()).is_some_and(|ty|
                !ty.qualifiers.is_volatile && !ty.qualifiers.is_atomic)
                && (self.unit.symbols.symbol(*symbol).is_ok_and(|entry| entry.kind() == SymbolKind::Parameter)
                    || self.extent_captures.values().any(|capture| matches!(capture, CHirExpr::Ident { symbol: captured, .. } if captured == symbol))),
            CHirExpr::Cast(_, value, _) | CHirExpr::EvaluateOnce(value, _)
            | CHirExpr::Unary(UnaryOp::Plus | UnaryOp::Neg | UnaryOp::Not | UnaryOp::BitNot, value, _) => self.pure_extent_integer(value),
            CHirExpr::Binary(_, a, b, _) => self.pure_extent_integer(a) && self.pure_extent_integer(b),
            _ => false,
        }
    }

    pub(super) fn pointer_bounds(
        &mut self,
        expression: Expr<'a>,
        c: &Context<'a>,
        depth: usize,
    ) -> Option<Bounds<'a>> {
        if depth >= 64 {
            return None;
        }
        if let Some(bounds) = self.extent_calls.get(&(expression as *const _ as usize)) {
            return Some(*bounds);
        }
        match expression {
            CHirExpr::Ident { symbol, facts, .. } => {
                if self.is_array(facts.ty()) {
                    self.lvalue_bounds(expression, c, depth + 1)
                } else {
                    c.object_bounds.get(symbol).copied()
                }
            }
            CHirExpr::StringLit(literal, span) => Some(self.complete_bounds(self.size_value(
                u64::try_from(literal.object_bytes(self.target)?.0.len()).ok()?,
                *span,
            ))),
            CHirExpr::Cast(ty, value, _)
                if matches!(self.unit.types.get(*ty)?.kind, TypeKind::Pointer(_)) =>
            {
                self.pointer_bounds(value, c, depth + 1)
            }
            CHirExpr::EvaluateOnce(value, _) => self.pointer_bounds(value, c, depth + 1),
            CHirExpr::Unary(UnaryOp::AddrOf, value, _) => self.lvalue_bounds(value, c, depth + 1),
            CHirExpr::Unary(UnaryOp::Deref, pointer, _)
                if PointerTypeDesc::from_expression(
                    &self.unit.symbols,
                    &self.unit.types,
                    pointer,
                )
                .is_ok_and(|ty| self.is_array(ty.pointee())) =>
            {
                self.pointer_bounds(pointer, c, depth + 1)
            }
            CHirExpr::MemoryRead { place, facts, .. } if self.is_array(facts.ty()) => {
                self.place_bounds(place, c, depth + 1)
            }
            CHirExpr::Binary(op @ (BinaryOp::Add | BinaryOp::Sub), a, b, _) => {
                let (pointer, index, pointee) = if let Ok(ty) =
                    PointerTypeDesc::from_expression(&self.unit.symbols, &self.unit.types, a)
                {
                    (*a, *b, ty.pointee())
                } else if *op == BinaryOp::Add {
                    (
                        *b,
                        *a,
                        PointerTypeDesc::from_expression(&self.unit.symbols, &self.unit.types, b)
                            .ok()?
                            .pointee(),
                    )
                } else {
                    return None;
                };
                if !self.pure_extent_integer(index) {
                    return None;
                }
                let mut bounds = self.pointer_bounds(pointer, c, depth + 1)?;
                let ty = self.size_type();
                let index =
                    self.alloc(CHirExpr::Cast(ty, self.captured_value(index), index.span()));
                let delta = self.size_binary(
                    BinaryOp::Mul,
                    index,
                    self.type_size(pointee, expression.span())?,
                );
                bounds.offset = self.size_binary(*op, bounds.offset, delta);
                Some(bounds)
            }
            CHirExpr::Ternary(test, yes, no, _) if self.pure_extent_integer(test) => {
                let yes = self.pointer_bounds(yes, c, depth + 1)?;
                let no = self.pointer_bounds(no, c, depth + 1)?;
                let test = self.captured_value(test);
                let choose = |a, b| self.size_choose(test, a, b);
                let merge = |a: Option<Extent<'a>>, b: Option<Extent<'a>>| {
                    let (a, b) = (a?, b?);
                    Some((choose(a.0, b.0), choose(a.1, b.1), choose(a.2, b.2)))
                };
                Some(Bounds {
                    offset: choose(yes.offset, no.offset),
                    whole: merge(yes.whole, no.whole),
                    member: merge(yes.member, no.member),
                })
            }
            CHirExpr::CompoundLit(ty, _, _) if self.is_array(*ty) => {
                self.lvalue_bounds(expression, c, depth + 1)
            }
            _ => None,
        }
    }

    fn lvalue_bounds(
        &mut self,
        expression: Expr<'a>,
        c: &Context<'a>,
        depth: usize,
    ) -> Option<Bounds<'a>> {
        if depth >= 64 {
            return None;
        }
        match expression {
            CHirExpr::Ident { span, .. } | CHirExpr::CompoundLit(_, _, span) => {
                let ty = match expression {
                    CHirExpr::CompoundLit(ty, ..) => *ty,
                    CHirExpr::Ident { facts, .. } => facts.ty(),
                    _ => unreachable!(),
                };
                Some(self.complete_bounds(self.type_size(ty, *span)?))
            }
            CHirExpr::StringLit(..) => self.pointer_bounds(expression, c, depth + 1),
            CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
                self.pointer_bounds(pointer, c, depth + 1)
            }
            CHirExpr::MemoryRead { place, .. } => self.place_bounds(place, c, depth + 1),
            _ => None,
        }
    }

    fn field_bounds(
        &mut self,
        mut bounds: Bounds<'a>,
        field: FieldProjection,
        span: SourceSpan,
    ) -> Option<Bounds<'a>> {
        let record = self
            .target
            .record_layout(&self.unit.types, field.tag())
            .ok()?;
        let member = record
            .fields
            .iter()
            .find(|f| f.field_index == field.field_index())?;
        if member.bit_field.is_some() {
            return None;
        }
        bounds.offset = self.size_binary(
            BinaryOp::Add,
            bounds.offset,
            self.size_value(member.byte_offset, span),
        );
        bounds.member = if matches!(
            self.unit.types.get(field.field_ty())?.kind,
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            }
        ) {
            bounds.whole
        } else {
            Some((
                bounds.offset,
                self.type_size(field.field_ty(), span)?,
                bounds.whole?.2,
            ))
        };
        Some(bounds)
    }

    fn place_bounds(
        &mut self,
        place: &MemoryPlace<'a>,
        c: &Context<'a>,
        depth: usize,
    ) -> Option<Bounds<'a>> {
        if depth >= 64 || place.is_bit_field() {
            return None;
        }
        place
            .validate_against(&self.unit.symbols, &self.unit.types)
            .ok()?;
        let mut bounds = match place.root() {
            PlaceRoot::Object { symbol } => {
                let object = self.unit.symbols.symbol(*symbol).ok()?;
                self.complete_bounds(self.type_size(object.ty(), object.span())?)
            }
            PlaceRoot::PointerExpression { pointer, .. } => {
                self.pointer_bounds(pointer, c, depth + 1)?
            }
            PlaceRoot::CompoundLiteral { expression, .. } => {
                self.lvalue_bounds(expression, c, depth + 1)?
            }
        };
        for projection in place.projections() {
            match projection {
                PlaceProjection::Field(field) => {
                    bounds = self.field_bounds(bounds, *field, bounds.offset.span())?
                }
                PlaceProjection::Index(index) => {
                    if !self.pure_extent_integer(index.index()) {
                        return None;
                    }
                    let size = self.type_size(index.element_ty(), index.index().span())?;
                    let ty = self.size_type();
                    let value = self.alloc(CHirExpr::Cast(
                        ty,
                        self.captured_value(index.index()),
                        index.index().span(),
                    ));
                    bounds.offset = self.size_binary(
                        BinaryOp::Add,
                        bounds.offset,
                        self.size_binary(BinaryOp::Mul, value, size),
                    );
                    if self.is_array(index.element_ty()) {
                        bounds.member = None;
                    }
                }
            }
        }
        Some(bounds)
    }

    fn remaining_extent(&self, bounds: Bounds<'a>, extent: Extent<'a>) -> Expr<'a> {
        let offset = self.size_binary(BinaryOp::Sub, bounds.offset, extent.0);
        let inside = self.size_binary(BinaryOp::Le, offset, extent.1);
        self.size_choose(
            inside,
            self.size_binary(BinaryOp::Sub, extent.1, offset),
            self.size_value(0, offset.span()),
        )
    }

    pub(super) fn object_size_result(
        &mut self,
        operand: Expr<'a>,
        mode: u8,
        dynamic: bool,
        c: &Context<'a>,
    ) -> Expr<'a> {
        let fallback = crate::constant::builtin_object_size(
            operand,
            mode,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        );
        let Some(bounds) = self.pointer_bounds(operand, c, 0) else {
            return self.size_value(fallback, operand.span());
        };
        let Some(extent) = (if mode & 1 == 0 {
            bounds.whole
        } else {
            bounds.member
        }) else {
            return self.size_value(fallback, operand.span());
        };
        let mut remaining = self.remaining_extent(bounds, extent);
        if mode & 1 != 0 {
            if let Some(whole) = bounds.whole {
                let complete = self.remaining_extent(bounds, whole);
                let limited = self.size_choose(
                    self.size_binary(BinaryOp::Lt, remaining, complete),
                    remaining,
                    complete,
                );
                remaining = self.size_choose(whole.2, limited, remaining);
            }
        }
        remaining = self.size_choose(
            extent.2,
            remaining,
            self.size_value(fallback, operand.span()),
        );
        if let Some(value) = crate::constant::eval_integer_constant(
            remaining,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        ) {
            self.size_value(value.raw_bits() as u64, operand.span())
        } else if dynamic {
            remaining
        } else {
            self.size_value(fallback, operand.span())
        }
    }
}
