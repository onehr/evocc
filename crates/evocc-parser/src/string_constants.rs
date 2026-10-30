//! Source-level string constants. This query uses only written initializers
//! and integer constant expressions, never optimization or inliner facts.

use std::collections::BTreeMap;

use super::*;

struct StringView {
    extent: u64,
    offset: u64,
    // Missing entries are known zero bytes, not unknown bytes. The array's
    // declared extent can be very large; work is bounded by source entries.
    nonzero: BTreeMap<u64, u8>,
    dependencies: Vec<SymbolId>,
}

impl StringView {
    fn shifted(mut self, count: i128) -> Option<Self> {
        self.offset = u64::try_from(i128::from(self.offset).checked_add(count)?).ok()?;
        (self.offset < self.extent).then_some(self)
    }

    fn length(self) -> Option<(u64, Vec<SymbolId>)> {
        let mut end = self.offset;
        for (&index, _) in self.nonzero.range(self.offset..) {
            if index != end {
                break;
            }
            end = end.checked_add(1)?;
        }
        (end < self.extent).then_some((end - self.offset, self.dependencies))
    }
}

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn string_object_binding_is_fixed(&self, symbol: SymbolId) -> bool {
        !self.entity_attributes.get(&symbol).is_some_and(|names| {
            names.contains("weak") || names.contains("weakref") || names.contains("alias")
        }) && !self
            .entities
            .get(&symbol)
            .is_some_and(|state| state.assembly_named)
    }

    pub(super) fn constant_string_length(
        &self,
        expression: &CHirExpr<'arena>,
    ) -> Option<(u64, Vec<SymbolId>)> {
        self.string_pointer(expression, true)?.length()
    }

    fn string_byte_type(&self, ty: TypeId) -> bool {
        matches!(
            self.types.get(ty).map(|t| &t.kind),
            Some(TypeKind::Int(
                IntRank::Char | IntRank::SignedPlainChar | IntRank::SChar | IntRank::UChar
            ))
        )
    }

    fn string_clean_type(&self, mut ty: TypeId) -> bool {
        loop {
            let Some(t) = self.types.get(ty) else {
                return false;
            };
            if t.qualifiers.is_volatile || t.qualifiers.is_atomic {
                return false;
            }
            if let TypeKind::Array { element, .. } = t.kind {
                ty = element;
            } else {
                return true;
            }
        }
    }

    fn string_object(&self, symbol: SymbolId) -> Option<&InitPlan<'arena>> {
        let declaration = self.symbols.symbol(symbol).ok()?;
        if declaration.scope() != self.symbols.file_scope()
            || declaration
                .storage_class()
                .is_some_and(StorageClass::is_thread_local)
            || !self.string_object_binding_is_fixed(symbol)
        {
            return None;
        }
        let plan = self.initialized_strings.get(&symbol)?;
        self.string_clean_type(plan.target_type()).then_some(plan)
    }

    fn string_index(&self, expression: &CHirExpr<'_>) -> Option<i128> {
        self.eval_integer_constant_expression(expression)
            .ok()?
            .try_mathematical_value()
    }

    fn literal_string(&self, literal: &StringLit) -> Option<StringView> {
        if !matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) {
            return None;
        }
        let (bytes, _) = literal.object_bytes(&self.target_layout)?;
        Some(StringView {
            extent: bytes.len().try_into().ok()?,
            offset: 0,
            dependencies: Vec::new(),
            nonzero: bytes
                .into_iter()
                .enumerate()
                .filter_map(|(i, byte)| (byte != 0).then_some((i as u64, byte)))
                .collect(),
        })
    }

    fn string_pointer(
        &self,
        expression: &CHirExpr<'arena>,
        read_pointer: bool,
    ) -> Option<StringView> {
        match expression {
            CHirExpr::StringLit(literal, _) => self.literal_string(literal),
            CHirExpr::Ident { symbol, .. } => {
                let plan = self.string_object(*symbol)?;
                if let TypeKind::Pointer(pointee) = self.types.get(plan.target_type())?.kind {
                    if !read_pointer || !self.string_byte_type(pointee) {
                        return None;
                    }
                    let [InitAction::StoreScalar { path, value, .. }] = plan.actions() else {
                        return None;
                    };
                    if !path.is_root() {
                        return None;
                    }
                    let mut view = self.string_pointer(value, false)?;
                    view.dependencies.push(*symbol);
                    Some(view)
                } else {
                    self.string_array(*symbol, plan, &SubobjectPath::root(), plan.target_type())
                }
            }
            CHirExpr::Cast(ty, value, _) => {
                let TypeKind::Pointer(pointee) = self.types.get(*ty)?.kind else {
                    return None;
                };
                self.string_byte_type(pointee).then_some(())?;
                self.string_pointer(value, read_pointer)
            }
            CHirExpr::Unary(UnaryOp::AddrOf, value, _) => self.string_lvalue(value, read_pointer),
            CHirExpr::MemoryRead { place, .. }
                if matches!(
                    self.types.get(place.value_ty())?.kind,
                    TypeKind::Array { .. }
                ) =>
            {
                self.string_place(place, read_pointer)
            }
            CHirExpr::Binary(BinaryOp::Add, a, b, _) => {
                let (pointer, count) = if let Some(count) = self.string_index(b) {
                    (*a, count)
                } else {
                    (*b, self.string_index(a)?)
                };
                // A cast outside this addition cannot change its stride.
                // In particular, &array + 1 is not its second character.
                let pointee = PointerTypeDesc::from_expression_with_target(
                    &self.symbols,
                    &self.types,
                    pointer,
                    &self.target_layout,
                )
                .ok()?
                .pointee();
                if !self.string_byte_type(pointee) {
                    return None;
                }
                self.string_pointer(pointer, read_pointer)?.shifted(count)
            }
            CHirExpr::Ternary(condition, yes, no, _) => self.string_pointer(
                if self.string_index(condition)? == 0 {
                    no
                } else {
                    yes
                },
                read_pointer,
            ),
            _ => None,
        }
    }

    fn string_lvalue(
        &self,
        expression: &CHirExpr<'arena>,
        read_pointer: bool,
    ) -> Option<StringView> {
        match expression {
            CHirExpr::StringLit(literal, _) => self.literal_string(literal),
            CHirExpr::MemoryRead { place, .. } => self.string_place(place, read_pointer),
            // Addressing an array never reads a pointer object or its representation.
            CHirExpr::Ident { symbol, .. } => {
                let plan = self.string_object(*symbol)?;
                self.string_array(*symbol, plan, &SubobjectPath::root(), plan.target_type())
            }
            _ => None,
        }
    }

    fn string_place(&self, place: &MemoryPlace<'arena>, read_pointer: bool) -> Option<StringView> {
        if place.access_qualifiers().is_volatile || place.access_qualifiers().is_atomic {
            return None;
        }
        match place.root() {
            PlaceRoot::Object { symbol } => {
                let plan = self.string_object(*symbol)?;
                let mut ty = plan.target_type();
                let mut path = SubobjectPath::root();
                for (position, projection) in place.projections().iter().enumerate() {
                    match projection {
                        PlaceProjection::Field(field) => {
                            if let TypeKind::Union(tag) = self.types.get(ty)?.kind {
                                let selected = plan
                                    .actions()
                                    .iter()
                                    .find_map(|action| {
                                        match action
                                            .path()
                                            .steps()
                                            .strip_prefix(path.steps())?
                                            .first()?
                                        {
                                            SubobjectStep::Field { field_index } => {
                                                Some(*field_index)
                                            }
                                            _ => None,
                                        }
                                    })
                                    .or_else(|| {
                                        let TagState::Complete(TagDefinition::Union(fields)) =
                                            &self.types.tag(tag)?.state
                                        else {
                                            return None;
                                        };
                                        fields.iter().position(|field| field.name.is_some())
                                    })?;
                                if selected != field.field_index() {
                                    return None;
                                }
                            }
                            path = path.with_field(field.field_index());
                            ty = field.field_ty();
                        }
                        PlaceProjection::Index(index) => {
                            let TypeKind::Array {
                                element,
                                bound: ArrayBound::Fixed(count),
                            } = self.types.get(ty)?.kind
                            else {
                                return None;
                            };
                            let offset = u64::try_from(self.string_index(index.index())?).ok()?;
                            if offset >= count {
                                return None;
                            }
                            if self.string_byte_type(element) {
                                if position + 1 != place.projections().len() {
                                    return None;
                                }
                                return self
                                    .string_array(*symbol, plan, &path, ty)?
                                    .shifted(i128::from(offset));
                            }
                            path = path.with_index(offset);
                            ty = element;
                        }
                    }
                    if !self.string_clean_type(ty) {
                        return None;
                    }
                }
                self.string_array(*symbol, plan, &path, ty)
            }
            PlaceRoot::PointerExpression { pointer, .. } => {
                let mut view = self.string_pointer(pointer, read_pointer)?;
                for projection in place.projections() {
                    let PlaceProjection::Index(index) = projection else {
                        return None;
                    };
                    if !self.string_byte_type(index.element_ty()) {
                        return None;
                    }
                    view = view.shifted(self.string_index(index.index())?)?;
                }
                Some(view)
            }
            PlaceRoot::CompoundLiteral { .. } => None,
        }
    }

    fn string_array(
        &self,
        symbol: SymbolId,
        plan: &InitPlan<'arena>,
        path: &SubobjectPath,
        ty: TypeId,
    ) -> Option<StringView> {
        let TypeKind::Array {
            element,
            bound: ArrayBound::Fixed(extent),
        } = self.types.get(ty)?.kind
        else {
            return None;
        };
        if extent == 0 || !self.string_byte_type(element) || !self.string_clean_type(ty) {
            return None;
        }
        let mut nonzero = BTreeMap::new();
        let mut whole = plan.zero_fills_unwritten();
        let mut defined = 0u64;
        for action in plan.actions() {
            let Some(suffix) = action.path().steps().strip_prefix(path.steps()) else {
                // An enclosing aggregate copy is not byte knowledge.
                if path.steps().starts_with(action.path().steps()) {
                    return None;
                }
                continue;
            };
            match (suffix, action) {
                ([], InitAction::StoreString { literal, .. }) => {
                    let view = self.literal_string(literal)?;
                    nonzero = view
                        .nonzero
                        .into_iter()
                        .filter(|(i, _)| *i < extent)
                        .collect();
                    whole = true;
                }
                ([SubobjectStep::Index { index }], InitAction::StoreScalar { value, .. }) => {
                    if *index >= extent {
                        return None;
                    }
                    let byte = self
                        .eval_integer_constant_expression(value)
                        .ok()?
                        .raw_bits() as u8;
                    defined = defined.checked_add(1)?;
                    if byte != 0 {
                        nonzero.insert(*index, byte);
                    } else {
                        nonzero.remove(index);
                    }
                }
                ([SubobjectStep::Index { index }], InitAction::ZeroScalar { .. })
                    if *index < extent =>
                {
                    defined = defined.checked_add(1)?;
                    nonzero.remove(index);
                }
                _ => return None,
            }
        }
        (whole || defined == extent).then_some(StringView {
            extent,
            offset: 0,
            nonzero,
            dependencies: vec![symbol],
        })
    }
}
