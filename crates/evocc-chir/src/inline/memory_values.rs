//! Bounded, typed local-object facts. These describe values after retained
//! executions, not an alternative interpreter or permission to omit accesses.

use super::*;
use crate::integer::CInteger;

#[derive(Clone, Default)]
pub(super) struct MemoryValues {
    objects: BTreeMap<SymbolId, (TypeId, u64)>,
    cells: BTreeMap<(SymbolId, u64), CInteger>,
}

impl MemoryValues {
    pub(super) fn clear(&mut self) {
        self.cells.clear();
    }

    pub(super) fn retain_objects_from(&mut self, incoming: &Self) {
        self.objects
            .retain(|symbol, _| incoming.objects.contains_key(symbol));
        self.cells
            .retain(|(symbol, _), _| self.objects.contains_key(symbol));
    }

    pub(super) fn intersect(&mut self, other: &Self) {
        self.cells
            .retain(|key, value| other.cells.get(key) == Some(value));
    }
}

impl<'a> Substitution<'a, '_> {
    pub(super) fn memory_effects(expression: &'a CHirExpr<'a>) -> bool {
        let mut writes = false;
        let uses = inspect_with_visitors([Node::Expr(expression, false)], &mut |_| {}, &mut |e| {
            writes |= matches!(
                e,
                CHirExpr::Assign { .. }
                    | CHirExpr::Update { .. }
                    | CHirExpr::MemoryAssign { .. }
                    | CHirExpr::MemoryUpdate { .. }
                    | CHirExpr::Call(..)
                    | CHirExpr::Atomic(_)
                    | CHirExpr::Vector(_)
                    | CHirExpr::Complex(_)
                    | CHirExpr::CheckedArithmetic { .. }
                    | CHirExpr::BuiltinVarArgs { .. }
            );
        });
        writes || uses.jumps || !uses.assembly.is_empty()
    }
    pub(super) fn register_memory_object(&mut self, object: &ObjectDecl<'a>, c: &mut Context<'a>) {
        if !c.expand_calls
            || object.cleanup.is_some()
            || object.asm_label.is_some()
            || !matches!(object.storage, StorageClass::None | StorageClass::Auto)
            || self
                .unit
                .symbols
                .symbol(object.symbol)
                .is_ok_and(|s| s.scope() == self.unit.symbols.file_scope())
        {
            return;
        }
        let Some(canonical) = self.unit.types.get(object.ty) else {
            return;
        };
        if canonical.qualifiers.is_volatile || canonical.qualifiers.is_atomic {
            return;
        }
        let (element, length) = match canonical.kind {
            TypeKind::Array {
                element,
                bound: evocc_types::ArrayBound::Fixed(n),
            } => (element, n),
            _ => (object.ty, 1),
        };
        let Some(element_type) = self.unit.types.get(element) else {
            return;
        };
        if length == 0
            || length > 4096
            || c.memory.objects.len() >= 4096
            || element_type.qualifiers.is_volatile
            || element_type.qualifiers.is_atomic
            || crate::constant::canonical_integer_type(&self.unit.types, element).is_none()
        {
            return;
        }
        c.memory.objects.insert(object.symbol, (element, length));
        c.memory
            .cells
            .retain(|(symbol, _), _| *symbol != object.symbol);
        // Arrays start unknown. Do not infer initialized padding or implicit
        // zeroes from allocation, source declarations, or future stores.
        if element == object.ty {
            if let Some(initializer) = &object.initializer {
                if let [InitAction::StoreScalar { path, value, .. }] = initializer.plan.actions() {
                    if path.is_root() {
                        if let Some(value) = self.memory_integer(value, element) {
                            c.memory.cells.insert((object.symbol, 0), value);
                        }
                    }
                }
            }
        }
        c.memo.clear();
    }

    fn memory_integer(&mut self, value: &'a CHirExpr<'a>, ty: TypeId) -> Option<CInteger> {
        let value = self.value_after_execution(value, ty)?;
        let converted = self.alloc(CHirExpr::Cast(ty, value, value.span()));
        crate::constant::eval_executable_integer_constant(
            converted,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        )
    }

    fn memory_index(&self, e: &CHirExpr<'_>) -> Option<i128> {
        crate::constant::eval_executable_integer_constant(
            e,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        )?
        .try_mathematical_value()
    }

    fn memory_address(
        &self,
        e: &CHirExpr<'a>,
        memory: &MemoryValues,
        depth: u8,
    ) -> Option<(SymbolId, i128)> {
        let depth = depth.checked_sub(1)?;
        match e {
            CHirExpr::Ident { symbol, facts, .. }
                if matches!(
                    self.unit.types.get(facts.ty())?.kind,
                    TypeKind::Array { .. }
                ) =>
            {
                memory.objects.contains_key(symbol).then_some((*symbol, 0))
            }
            CHirExpr::Unary(UnaryOp::AddrOf, CHirExpr::Ident { symbol, .. }, _) => {
                memory.objects.contains_key(symbol).then_some((*symbol, 0))
            }
            CHirExpr::Unary(UnaryOp::AddrOf, CHirExpr::MemoryRead { place, .. }, _) => {
                self.memory_place_address(place, memory, depth)
            }
            CHirExpr::Cast(ty, value, _) => {
                let TypeKind::Pointer(pointee) = self.unit.types.get(*ty)?.kind else {
                    return None;
                };
                let address = self.memory_address(value, memory, depth)?;
                let (element, _) = memory.objects.get(&address.0)?;
                self.unit
                    .types
                    .compatible_ignoring_top_qualifiers(pointee, *element)
                    .then_some(address)
            }
            CHirExpr::Binary(op @ (BinaryOp::Add | BinaryOp::Sub), pointer, index, _) => {
                let (symbol, offset) = self.memory_address(pointer, memory, depth)?;
                let pointer_type = self.pointer_descriptor(pointer).ok()?;
                let (element, _) = memory.objects.get(&symbol)?;
                if !self
                    .unit
                    .types
                    .compatible_ignoring_top_qualifiers(pointer_type.pointee(), *element)
                {
                    return None;
                }
                let index = self.memory_index(index)?;
                Some((
                    symbol,
                    if *op == BinaryOp::Add {
                        offset.checked_add(index)?
                    } else {
                        offset.checked_sub(index)?
                    },
                ))
            }
            _ => None,
        }
    }

    fn memory_place_address(
        &self,
        place: &MemoryPlace<'a>,
        memory: &MemoryValues,
        depth: u8,
    ) -> Option<(SymbolId, i128)> {
        let depth = depth.checked_sub(1)?;
        let (symbol, mut offset) = match place.root() {
            PlaceRoot::Object { symbol } => (*symbol, 0),
            PlaceRoot::PointerExpression { pointer, .. } => {
                self.memory_address(pointer, memory, depth)?
            }
            PlaceRoot::CompoundLiteral { .. } => return None,
        };
        let (element, _) = memory.objects.get(&symbol)?;
        for projection in place.projections() {
            let PlaceProjection::Index(index) = projection else {
                return None;
            };
            if !self
                .unit
                .types
                .compatible_ignoring_top_qualifiers(index.element_ty(), *element)
            {
                return None;
            }
            offset = offset.checked_add(self.memory_index(index.index())?)?;
        }
        Some((symbol, offset))
    }

    fn memory_cell(
        &self,
        place: &MemoryPlace<'a>,
        memory: &MemoryValues,
    ) -> Option<(SymbolId, u64)> {
        if place.access_qualifiers().is_volatile
            || place.access_qualifiers().is_atomic
            || place.is_bit_field()
        {
            return None;
        }
        let (symbol, offset) = self.memory_place_address(place, memory, 32)?;
        let (element, length) = memory.objects.get(&symbol)?;
        let offset = u64::try_from(offset).ok()?;
        (offset < *length
            && self
                .unit
                .types
                .compatible_ignoring_top_qualifiers(*element, place.value_ty()))
        .then_some((symbol, offset))
    }

    pub(super) fn invalidate_memory_write(&self, c: &mut Context<'a>) {
        if c.expand_calls {
            c.memory.clear();
            c.memo.clear();
        }
    }

    pub(super) fn invalidate_memory_place(&self, place: &MemoryPlace<'a>, c: &mut Context<'a>) {
        if !c.expand_calls {
            return;
        }
        if let Some(key) = self.memory_cell(place, &c.memory) {
            c.memory.cells.remove(&key);
            c.memo.clear();
        } else {
            self.invalidate_memory_write(c);
        }
    }

    pub(super) fn invalidate_memory_scalar(&self, symbol: SymbolId, c: &mut Context<'a>) {
        if c.expand_calls {
            c.memory.cells.remove(&(symbol, 0));
            c.memo.clear();
        }
    }

    pub(super) fn remember_memory_read(
        &mut self,
        expression: &'a CHirExpr<'a>,
        c: &mut Context<'a>,
    ) -> Option<&'a CHirExpr<'a>> {
        if let CHirExpr::Binary(op @ (BinaryOp::Eq | BinaryOp::Ne), left, right, span) = expression
        {
            let nonnull = |mut e: &CHirExpr<'a>| {
                if let CHirExpr::Cast(ty, operand, _) = e {
                    if crate::constant::canonical_integer_type(&self.unit.types, *ty)
                        .is_some_and(|ty| ty.bits() == 64)
                    {
                        e = operand;
                    }
                }
                self.memory_address(e, &c.memory, 32)
                    .is_some_and(|(symbol, offset)| {
                        offset >= 0 && offset < i128::from(c.memory.objects[&symbol].1)
                    })
            };
            if (self.memory_index(right) == Some(0) && nonnull(left))
                || (self.memory_index(left) == Some(0) && nonnull(right))
            {
                // Only the relation is known, never the numerical address.
                // Retain the executed comparison and source ICE boundaries.
                let literal = self.alloc(CHirExpr::Lit(
                    IntLit {
                        value: u64::from(*op == BinaryOp::Ne),
                        suffix: IntSuffix::None,
                        base: IntBase::Dec,
                    },
                    *span,
                ));
                self.inline_values
                    .insert(expression as *const _ as usize, literal);
                return Some(literal);
            }
        }
        let (key, ty) = match expression {
            CHirExpr::MemoryRead { place, .. } => {
                (self.memory_cell(place, &c.memory)?, place.value_ty())
            }
            CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
                let pointer_type = self.pointer_descriptor(pointer).ok()?;
                let place = self
                    .derive_memory_place(
                        PlaceRoot::PointerExpression {
                            pointer,
                            pointer_type,
                        },
                        Vec::new(),
                        pointer_type.pointee(),
                    )
                    .ok()?;
                (self.memory_cell(&place, &c.memory)?, place.value_ty())
            }
            CHirExpr::Ident { symbol, facts, .. } => ((*symbol, 0), facts.ty()),
            _ => return None,
        };
        let integer_ty = crate::constant::canonical_integer_type(&self.unit.types, ty)?;
        let value = *c.memory.cells.get(&key)?;
        // Restrict the first implementation to <=64-bit cells. Construction of
        // wider integer constants already has a separate shared path.
        if integer_ty.bits() > 64 {
            return None;
        }
        let literal = self.alloc(CHirExpr::Lit(
            IntLit {
                value: value.raw_bits() as u64,
                suffix: IntSuffix::ULL,
                base: IntBase::Hex,
            },
            expression.span(),
        ));
        let literal = self.alloc(CHirExpr::Cast(ty, literal, expression.span()));
        self.inline_values
            .insert(expression as *const _ as usize, literal);
        Some(literal)
    }

    pub(super) fn remember_memory_assignment(
        &mut self,
        rewritten: &'a CHirStmt<'a>,
        previous: &MemoryValues,
        c: &mut Context<'a>,
    ) {
        if !c.expand_calls {
            return;
        }
        let CHirStmt::Expr(expression, _) = rewritten else {
            return;
        };
        let (op, value, key, ty) = match expression {
            CHirExpr::MemoryAssign {
                op, place, value, ..
            } => (
                *op,
                *value,
                self.memory_cell(place, previous),
                place.value_ty(),
            ),
            CHirExpr::Assign {
                op, place, value, ..
            } => (
                *op,
                *value,
                previous
                    .objects
                    .contains_key(&place.symbol())
                    .then_some((place.symbol(), 0)),
                place.ty(),
            ),
            _ => return,
        };
        let Some(key) = key else { return };
        if Self::memory_effects(value) {
            return;
        }
        let Some(proved) = self.integer_value_after_execution(value) else {
            return;
        };
        let Some(mut value) = crate::constant::eval_executable_integer_constant(
            proved,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        ) else {
            return;
        };
        if let Some(binary) = op.binary_operation() {
            if let Some(old) = previous.cells.get(&key) {
                let Ok(result) = crate::integer::eval_binary_integer(binary, *old, value) else {
                    return;
                };
                value = result;
            } else if !(binary == BinaryOp::BitAnd && value.raw_bits() == 0) {
                return;
            }
        }
        let Some(integer_ty) = crate::constant::canonical_integer_type(&self.unit.types, ty) else {
            return;
        };
        value = value.cast(integer_ty);
        // The original access and its checks remain. Disjoint facts survive
        // exact writes, but nothing here resurrects facts killed by the RHS.
        c.memory.cells.insert(key, value);
        c.memo.clear();
    }
}
