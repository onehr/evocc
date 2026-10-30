//! Parser initializer paths.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn next_initializer_child(
        &self,
        target: TypeId,
        path: &SubobjectPath,
        after: Option<SubobjectStep>,
    ) -> Option<ResolvedInitializerTarget> {
        let kind = &self.types.get(target)?.kind;
        let (step, ty) = match kind {
            TypeKind::Array { element, bound } => {
                let index = match after {
                    None => 0,
                    Some(SubobjectStep::Index { index }) => index.checked_add(1)?,
                    Some(SubobjectStep::Field { .. }) => return None,
                };
                if matches!(bound, ArrayBound::Fixed(size) if index >= *size)
                    || matches!(bound, ArrayBound::Variable | ArrayBound::Captured(_))
                {
                    return None;
                }
                (SubobjectStep::Index { index }, *element)
            }
            TypeKind::Union(_) if after.is_some() => return None,
            _ => self
                .aggregate_initializer_children(target)?
                .into_iter()
                .find(|(step, _)| after.is_none_or(|after| *step > after))?,
        };
        Some(ResolvedInitializerTarget {
            path: extend_subobject_path(path.clone(), step),
            ty,
        })
    }

    pub(super) fn initializer_successor(
        &self,
        target: TypeId,
        base_path: &SubobjectPath,
        initialized: &SubobjectPath,
    ) -> Option<ResolvedInitializerTarget> {
        let mut parents = Vec::new();
        let mut ty = target;
        let mut path = base_path.clone();
        for &step in &initialized.steps()[base_path.steps().len()..] {
            parents.push((ty, path.clone(), step));
            ty = match (&self.types.get(ty)?.kind, step) {
                (
                    TypeKind::Array { element, .. } | TypeKind::Vector { element, .. },
                    SubobjectStep::Index { .. },
                ) => *element,
                (
                    TypeKind::Struct(tag) | TypeKind::Union(tag),
                    SubobjectStep::Field { field_index },
                ) => match &self.types.tag(*tag)?.state {
                    TagState::Complete(
                        TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                    ) => fields.get(field_index)?.ty,
                    _ => return None,
                },
                _ => return None,
            };
            path = extend_subobject_path(path, step);
        }
        parents
            .into_iter()
            .rev()
            .find_map(|(ty, path, step)| self.next_initializer_child(ty, &path, Some(step)))
    }

    pub(super) fn resolve_initializer_designation(
        &mut self,
        target: TypeId,
        base_path: &SubobjectPath,
        designation: &InitializerDesignation<'arena>,
    ) -> Option<Vec<ResolvedInitializerTarget>> {
        let mut resolved = vec![ResolvedInitializerTarget {
            path: base_path.clone(),
            ty: target,
        }];
        for designator in &designation.designators {
            let mut next = Vec::new();
            match designator {
                WrittenDesignator::Field(name) => {
                    for current in resolved {
                        let Some(fields) = self.designated_field(current.ty, &name.spelling) else {
                            self.report("E2906", "designator-outside-object", name.span);
                            return None;
                        };
                        let field_type = fields.last()?.field_ty();
                        let path = fields.into_iter().fold(current.path, |path, field| {
                            path.with_field(field.field_index())
                        });
                        next.push(ResolvedInitializerTarget {
                            path,
                            ty: field_type,
                        });
                    }
                }
                WrittenDesignator::Index { expression, span } => {
                    let index = self.designator_constant_index(expression, *span)?;
                    for current in resolved {
                        let element = self.designated_element_type(current.ty, index, *span)?;
                        next.push(ResolvedInitializerTarget {
                            path: current.path.with_index(index),
                            ty: element,
                        });
                    }
                }
                WrittenDesignator::Range { first, last, span } => {
                    let first = self.designator_constant_index(first, *span)?;
                    let last = self.designator_constant_index(last, *span)?;
                    if first > last {
                        self.report("E2917", "designator-index-not-constant", *span);
                        return None;
                    }
                    for current in resolved {
                        for index in first..=last {
                            let element = self.designated_element_type(current.ty, index, *span)?;
                            next.push(ResolvedInitializerTarget {
                                path: current.path.clone().with_index(index),
                                ty: element,
                            });
                        }
                    }
                }
            }
            resolved = next;
        }
        Some(resolved)
    }

    pub(super) fn designated_field(
        &self,
        target: TypeId,
        spelling: &str,
    ) -> Option<Vec<FieldProjection>> {
        self.record_member_path(target, spelling)
    }

    pub(super) fn designator_constant_index(
        &mut self,
        expression: &CHirExpr<'_>,
        span: SourceSpan,
    ) -> Option<u64> {
        let index = self
            .eval_integer_constant_expression(expression)
            .ok()
            .and_then(CInteger::try_mathematical_value)
            .and_then(|value| u64::try_from(value).ok());
        let Some(index) = index else {
            self.report("E2917", "designator-index-not-constant", span);
            return None;
        };
        Some(index)
    }

    pub(super) fn designated_element_type(
        &mut self,
        target: TypeId,
        index: u64,
        span: SourceSpan,
    ) -> Option<TypeId> {
        let Some(TypeKind::Array { element, bound }) =
            self.types.get(target).map(|ty| ty.kind.clone())
        else {
            self.report("E2906", "designator-outside-object", span);
            return None;
        };
        if matches!(bound, ArrayBound::Fixed(size) if index >= size) {
            self.report("E2906", "designator-outside-object", span);
            return None;
        }
        Some(element)
    }

    pub(super) fn final_flexible_array_member(
        &self,
        target: TypeId,
    ) -> Option<(usize, String, TypeId)> {
        let Some(TypeKind::Struct(tag)) = self.types.get(target).map(|ty| &ty.kind) else {
            return None;
        };
        let tag = self.types.tag(*tag)?;
        let fields = match &tag.state {
            TagState::Complete(TagDefinition::Struct(fields)) => fields,
            TagState::Incomplete
            | TagState::Complete(TagDefinition::Union(_) | TagDefinition::Enum(_)) => return None,
        };
        let (field_index, field) = fields.iter().enumerate().next_back()?;
        let Some(TypeKind::Array {
            element,
            bound: ArrayBound::Unbounded,
        }) = self.types.get(field.ty).map(|ty| &ty.kind)
        else {
            return None;
        };
        Some((field_index, field.name.clone()?, *element))
    }

    pub(super) fn aggregate_first_initializable_member_is_unbounded(&self, target: TypeId) -> bool {
        let Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) =
            self.types.get(target).map(|ty| &ty.kind)
        else {
            return false;
        };
        let Some(tag) = self.types.tag(*tag) else {
            return false;
        };
        let fields = match &tag.state {
            TagState::Complete(TagDefinition::Struct(fields))
            | TagState::Complete(TagDefinition::Union(fields)) => fields,
            TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => return false,
        };
        fields
            .iter()
            .find(|field| !(field.name.is_none() && field.bit_field.is_some()))
            .is_some_and(|field| {
                matches!(
                    self.types.get(field.ty).map(|ty| &ty.kind),
                    Some(TypeKind::Array {
                        bound: ArrayBound::Unbounded,
                        ..
                    })
                )
            })
    }

    pub(super) fn flexible_initializer_element_count(
        &self,
        target: TypeId,
        actions: &[InitAction<'arena>],
    ) -> u64 {
        let Some((field_index, _, _)) = self.final_flexible_array_member(target) else {
            return 0;
        };
        actions
            .iter()
            .filter_map(|action| match action.path().steps() {
                [SubobjectStep::Field {
                    field_index: candidate,
                }] if *candidate == field_index => match action {
                    InitAction::StoreString { literal, .. } => {
                        literal.initializer_element_count(&ArrayBound::Unbounded)
                    }
                    _ => None,
                },
                [SubobjectStep::Field {
                    field_index: candidate,
                }, SubobjectStep::Index { index }, ..]
                    if *candidate == field_index =>
                {
                    index.checked_add(1)
                }
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }

    pub(super) fn normalize_stream_subobject(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        elements: &[WrittenInitializer<'arena>],
        cursor: &mut usize,
        actions: &mut Vec<InitAction<'arena>>,
    ) -> Option<()> {
        let initializer = elements.get(*cursor)?;
        if matches!(initializer, WrittenInitializer::List { .. })
            || self.direct_initializer_expression(target, initializer)
        {
            *cursor += 1;
            return self.normalize_initializer_value(target, path, initializer, actions);
        }
        let undesignated = vec![None; elements.len().saturating_sub(*cursor)];
        self.normalize_aggregate_elements(
            target,
            path,
            &elements[*cursor..],
            &undesignated,
            actions,
            false,
        )?;
        *cursor = self.initializer_stream_consumption(target, elements, *cursor)?;
        Some(())
    }

    pub(super) fn normalize_string_initializer(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        literal: &StringLit,
        span: SourceSpan,
        actions: &mut Vec<InitAction<'arena>>,
    ) -> Option<()> {
        if !matches!(
            literal.prefix,
            StringPrefix::None
                | StringPrefix::U8
                | StringPrefix::L
                | StringPrefix::L16
                | StringPrefix::LInt
        ) {
            self.report("E2205", "not-covered-day5: wide-string-initializer", span);
            return None;
        }
        let canonical = self.types.get(target)?;
        let TypeKind::Array { element, bound } = &canonical.kind else {
            self.report("E2202", "invalid-initializer-type", span);
            return None;
        };
        let element_matches = self.string_literal_element_matches(*element, literal.prefix);
        if !element_matches {
            self.report("E2202", "invalid-initializer-type", span);
            return None;
        }
        let units = literal.explicit_unit_count()?;
        if let ArrayBound::Fixed(bound) = bound {
            if units > *bound {
                self.report(
                    "E2204",
                    format!("string-initializer-too-long: {units} > {bound}"),
                    span,
                );
                return None;
            }
        }
        actions.push(InitAction::store_string(
            path,
            target,
            literal.clone(),
            span,
        ));
        Some(())
    }

    pub(super) fn direct_initializer_expression(
        &self,
        target: TypeId,
        initializer: &WrittenInitializer<'arena>,
    ) -> bool {
        let WrittenInitializer::Expression(expression) = initializer else {
            return false;
        };
        let Some(kind) = self.types.get(target).map(|canonical| &canonical.kind) else {
            return false;
        };
        if matches!(kind, TypeKind::Pointer(_)) {
            self.note_pointer_conversion(target, expression);
            return true;
        }
        if matches!(
            kind,
            TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_)
        ) || matches!(kind, TypeKind::TargetBuiltin(kind) if kind.layout().is_some())
        {
            return true;
        }
        if matches!(expression, CHirExpr::StringLit(_, _)) && matches!(kind, TypeKind::Array { .. })
        {
            return true;
        }
        matches!(
            kind,
            TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::Vector { .. }
        ) && self.assignment_constraint(AssignmentOp::Simple, target, expression)
            == UpdateConstraint::AggregateBoundary
    }

    pub(super) fn aggregate_initializer_children(
        &self,
        target: TypeId,
    ) -> Option<Vec<(SubobjectStep, TypeId)>> {
        match self.types.get(target)?.kind.clone() {
            TypeKind::Vector { element, lanes, .. } => Some(
                (0..u64::from(lanes))
                    .map(|index| (SubobjectStep::Index { index }, element))
                    .collect(),
            ),
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(bound),
            } => Some(
                (0..bound)
                    .map(|index| (SubobjectStep::Index { index }, element))
                    .collect(),
            ),
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Some(Vec::new()),
            TypeKind::Array {
                bound: ArrayBound::Variable | ArrayBound::Captured(_),
                ..
            } => None,
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                let tag = self.types.tag(tag)?;
                let fields = match &tag.state {
                    TagState::Complete(TagDefinition::Struct(fields))
                    | TagState::Complete(TagDefinition::Union(fields)) => fields,
                    TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => {
                        return None;
                    }
                };
                let is_union = matches!(self.types.get(target)?.kind, TypeKind::Union(_));
                let mut children = Vec::new();
                for (field_index, field) in fields.iter().enumerate() {
                    if field.name.is_none() && field.bit_field.is_some() {
                        continue;
                    }
                    if !is_union
                        && field_index + 1 == fields.len()
                        && matches!(
                            self.types.get(field.ty).map(|ty| &ty.kind),
                            Some(TypeKind::Array {
                                bound: ArrayBound::Unbounded,
                                ..
                            })
                        )
                        && !self.flexible_initializer_allowed
                    {
                        continue;
                    }
                    children.push((SubobjectStep::Field { field_index }, field.ty));
                    if is_union {
                        break;
                    }
                }
                Some(children)
            }
            TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::TargetBuiltin(_)
            | TypeKind::GnuInt128(_)
            | TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Function { .. }
            | TypeKind::Enum(_) => None,
        }
    }

    pub(super) fn is_narrow_character_type(&self, target: TypeId) -> bool {
        self.types.get(target).is_some_and(|canonical| {
            !canonical.qualifiers.is_atomic
                && matches!(
                    canonical.kind,
                    TypeKind::Int(
                        IntRank::Char | IntRank::SignedPlainChar | IntRank::SChar | IntRank::UChar
                    )
                )
        })
    }

    pub(super) fn string_literal_element_matches(
        &self,
        target: TypeId,
        prefix: StringPrefix,
    ) -> bool {
        match prefix {
            StringPrefix::None | StringPrefix::U8 => self.is_narrow_character_type(target),
            StringPrefix::L | StringPrefix::L16 | StringPrefix::LInt => {
                self.types.get(target).is_some_and(|canonical| {
                    !canonical.qualifiers.is_atomic
                        && canonical.kind == TypeKind::Int(prefix.element_rank())
                })
            }
            StringPrefix::U16 | StringPrefix::U => false,
        }
    }

    pub(super) fn string_initializes_array_directly(
        &self,
        target: TypeId,
        literal: &StringLit,
    ) -> bool {
        if !matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) {
            return true;
        }
        matches!(
            self.types.get(target).map(|ty| &ty.kind),
            Some(TypeKind::Array { element, .. }) if self.is_narrow_character_type(*element)
        )
    }
}
