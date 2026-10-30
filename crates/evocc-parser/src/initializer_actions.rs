//! Parser initializer actions.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn initializer_action_count(
        &self,
        target: TypeId,
        written: &WrittenInitializer<'arena>,
    ) -> Result<u64, InitializerPreflightError> {
        let kind = self
            .types
            .get(target)
            .ok_or(InitializerPreflightError::InvalidObject)?
            .kind
            .clone();
        if matches!(kind, TypeKind::Array { .. })
            && match written {
                WrittenInitializer::Expression(CHirExpr::StringLit(literal, _)) => {
                    self.string_initializes_array_directly(target, literal)
                }
                WrittenInitializer::List { elements, .. } => {
                    elements.first().is_some_and(|element| match element {
                        WrittenInitializer::Expression(CHirExpr::StringLit(literal, _)) => {
                            self.string_initializes_array_directly(target, literal)
                        }
                        WrittenInitializer::Expression(_) | WrittenInitializer::List { .. } => {
                            false
                        }
                    })
                }
                WrittenInitializer::Expression(_) => false,
            }
        {
            return Ok(1);
        }
        match (kind, written) {
            (
                TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_),
                WrittenInitializer::Expression(_),
            ) => Ok(1),
            (TypeKind::TargetBuiltin(_), WrittenInitializer::Expression(_)) => Ok(1),
            (TypeKind::TargetBuiltin(_), WrittenInitializer::List { elements, .. }) => elements
                .first()
                .map_or(Ok(1), |first| self.initializer_action_count(target, first)),
            (
                TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_),
                WrittenInitializer::List { elements, .. },
            ) => elements
                .first()
                .map_or(Ok(1), |first| self.initializer_action_count(target, first)),
            (
                TypeKind::Array {
                    element,
                    bound: ArrayBound::Unbounded,
                },
                WrittenInitializer::List { elements, .. },
            ) if self.flexible_initializer_allowed => {
                elements.iter().try_fold(0_u64, |count, written| {
                    count
                        .checked_add(self.initializer_action_count(element, written)?)
                        .ok_or(InitializerPreflightError::TooLarge)
                })
            }
            (
                TypeKind::Array { .. }
                | TypeKind::Struct(_)
                | TypeKind::Union(_)
                | TypeKind::Vector { .. },
                WrittenInitializer::List { elements, .. },
            ) => self
                .initializer_aggregate_action_count(target, elements, 0)
                .map(|(_, count)| count),
            (
                TypeKind::Array { .. }
                | TypeKind::Struct(_)
                | TypeKind::Union(_)
                | TypeKind::Vector { .. },
                WrittenInitializer::Expression(_),
            ) => Ok(0),
            (TypeKind::Void | TypeKind::BuiltinVaList | TypeKind::Function { .. }, _) => {
                Err(InitializerPreflightError::InvalidObject)
            }
        }
    }

    pub(super) fn initializer_aggregate_action_count(
        &self,
        target: TypeId,
        elements: &[WrittenInitializer<'arena>],
        mut cursor: usize,
    ) -> Result<(usize, u64), InitializerPreflightError> {
        let kind = self
            .types
            .get(target)
            .ok_or(InitializerPreflightError::InvalidObject)?
            .kind
            .clone();
        let mut count = 0_u64;
        match kind {
            TypeKind::Vector { element, lanes, .. } => {
                for _ in 0..lanes {
                    if cursor < elements.len() {
                        let (next, n) =
                            self.initializer_stream_action_count(element, elements, cursor)?;
                        cursor = next;
                        count = checked_initializer_action_add(count, n)?;
                    } else {
                        count = checked_initializer_action_add(count, 1)?;
                    }
                }
            }
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(bound),
            } => {
                let mut index = 0_u64;
                while index < bound && cursor < elements.len() {
                    let (next, child_count) =
                        self.initializer_stream_action_count(element, elements, cursor)?;
                    cursor = next;
                    count = checked_initializer_action_add(count, child_count)?;
                    index += 1;
                }
                if index < bound {
                    let child_count =
                        self.initializer_subobject_count(element, &mut HashSet::new())?;
                    let omitted = bound
                        .checked_sub(index)
                        .and_then(|remaining| remaining.checked_mul(child_count))
                        .ok_or(InitializerPreflightError::TooLarge)?;
                    count = checked_initializer_action_add(count, omitted)?;
                }
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => {}
            TypeKind::Array {
                bound: ArrayBound::Variable | ArrayBound::Captured(_),
                ..
            } => return Err(InitializerPreflightError::InvalidObject),
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                let tag_data = self
                    .types
                    .tag(tag)
                    .ok_or(InitializerPreflightError::InvalidObject)?;
                let (fields, is_union) = match &tag_data.state {
                    TagState::Complete(TagDefinition::Struct(fields)) => (fields, false),
                    TagState::Complete(TagDefinition::Union(fields)) => (fields, true),
                    TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => {
                        return Err(InitializerPreflightError::InvalidObject);
                    }
                };
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
                    {
                        continue;
                    }
                    if cursor < elements.len() {
                        let (next, child_count) =
                            self.initializer_stream_action_count(field.ty, elements, cursor)?;
                        cursor = next;
                        count = checked_initializer_action_add(count, child_count)?;
                    } else {
                        let child_count =
                            self.initializer_subobject_count(field.ty, &mut HashSet::new())?;
                        count = checked_initializer_action_add(count, child_count)?;
                    }
                    if is_union {
                        break;
                    }
                }
            }
            TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Enum(_)
            | TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::TargetBuiltin(_)
            | TypeKind::GnuInt128(_)
            | TypeKind::Function { .. } => {
                return Err(InitializerPreflightError::InvalidObject);
            }
        }
        Ok((cursor, count))
    }

    pub(super) fn initializer_stream_action_count(
        &self,
        target: TypeId,
        elements: &[WrittenInitializer<'arena>],
        cursor: usize,
    ) -> Result<(usize, u64), InitializerPreflightError> {
        let initializer = elements
            .get(cursor)
            .ok_or(InitializerPreflightError::InvalidObject)?;
        if matches!(initializer, WrittenInitializer::List { .. })
            || self.direct_initializer_expression(target, initializer)
        {
            return Ok((
                cursor + 1,
                self.initializer_action_count(target, initializer)?,
            ));
        }
        self.initializer_aggregate_action_count(target, elements, cursor)
    }

    pub(super) fn initializer_subobject_count(
        &self,
        target: TypeId,
        visiting: &mut HashSet<TypeId>,
    ) -> Result<u64, InitializerPreflightError> {
        let kind = self
            .types
            .get(target)
            .ok_or(InitializerPreflightError::InvalidObject)?
            .kind
            .clone();
        match kind {
            TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_) => Ok(1),
            TypeKind::TargetBuiltin(kind) => kind
                .layout()
                .map(|_| 1)
                .ok_or(InitializerPreflightError::InvalidObject),
            TypeKind::Vector { lanes, .. } => Ok(u64::from(lanes)),
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(bound),
            } => {
                let element_count = self.initializer_subobject_count(element, visiting)?;
                bound
                    .checked_mul(element_count)
                    .ok_or(InitializerPreflightError::TooLarge)
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Ok(0),
            TypeKind::Array {
                bound: ArrayBound::Variable | ArrayBound::Captured(_),
                ..
            } => Err(InitializerPreflightError::InvalidObject),
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                if !visiting.insert(target) {
                    return Err(InitializerPreflightError::RecursiveObject);
                }
                let tag = self
                    .types
                    .tag(tag)
                    .ok_or(InitializerPreflightError::InvalidObject)?;
                let (fields, is_union) = match &tag.state {
                    TagState::Complete(TagDefinition::Struct(fields)) => (fields, false),
                    TagState::Complete(TagDefinition::Union(fields)) => (fields, true),
                    TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => {
                        visiting.remove(&target);
                        return Err(InitializerPreflightError::InvalidObject);
                    }
                };
                let mut count = 0_u64;
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
                    {
                        continue;
                    }
                    count = count
                        .checked_add(self.initializer_subobject_count(field.ty, visiting)?)
                        .ok_or(InitializerPreflightError::TooLarge)?;
                    if is_union {
                        break;
                    }
                }
                visiting.remove(&target);
                Ok(count)
            }
            TypeKind::Void | TypeKind::BuiltinVaList | TypeKind::Function { .. } => {
                Err(InitializerPreflightError::InvalidObject)
            }
        }
    }

    pub(super) fn normalize_initializer_value(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        written: &WrittenInitializer<'arena>,
        actions: &mut Vec<InitAction<'arena>>,
    ) -> Option<()> {
        let kind = self.types.get(target)?.kind.clone();
        if let (
            Some((member, count)),
            [SubobjectStep::Field { field_index }, SubobjectStep::Index { index }, ..],
        ) = (&mut self.flexible_initializer_extent, path.steps())
        {
            if member == field_index {
                *count = (*count).max(index.saturating_add(1));
            }
        }
        if self.flexible_initializer_allowed
            && matches!(
                kind,
                TypeKind::Array {
                    bound: ArrayBound::Unbounded,
                    ..
                }
            )
            && !matches!(path.steps(), [SubobjectStep::Field { .. }])
            && !matches!(written, WrittenInitializer::List { elements, .. } if elements.is_empty())
        {
            self.report("E2202", "invalid-initializer-type", written.span());
            return None;
        }
        match (&kind, written) {
            (
                TypeKind::Array { .. },
                WrittenInitializer::Expression(CHirExpr::StringLit(literal, span)),
            ) if self.string_initializes_array_directly(target, literal) => {
                self.normalize_string_initializer(target, path, literal, *span, actions)
            }
            (
                TypeKind::Array { .. },
                WrittenInitializer::List {
                    elements,
                    designations,
                    ..
                },
            ) if designations.first().is_none_or(Option::is_none)
                && elements.first().is_some_and(|element| {
                    matches!(
                        element,
                        WrittenInitializer::Expression(CHirExpr::StringLit(literal, _))
                            if self.string_initializes_array_directly(target, literal)
                    )
                }) =>
            {
                let WrittenInitializer::Expression(CHirExpr::StringLit(literal, span)) =
                    &elements[0]
                else {
                    unreachable!("guard established a string expression")
                };
                self.normalize_string_initializer(target, path, literal, *span, actions)?;
                if let Some(excess) = elements.get(1) {
                    self.report("E2201", "excess-initializer", excess.span());
                    return None;
                }
                Some(())
            }
            (
                TypeKind::Array {
                    bound: ArrayBound::Unbounded,
                    ..
                },
                WrittenInitializer::List {
                    elements,
                    designations,
                    ..
                },
            ) if self.flexible_initializer_allowed => self.normalize_designated_aggregate_elements(
                target,
                path,
                elements,
                designations,
                actions,
            ),
            (
                TypeKind::Array { .. }
                | TypeKind::Struct(_)
                | TypeKind::Union(_)
                | TypeKind::Vector { .. },
                WrittenInitializer::List {
                    elements,
                    designations,
                    ..
                },
            ) => self.normalize_aggregate_elements(
                target,
                path,
                elements,
                designations,
                actions,
                true,
            ),
            (
                TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::Vector { .. },
                WrittenInitializer::Expression(expression),
            ) => {
                if self.assignment_constraint(AssignmentOp::Simple, target, expression)
                    != UpdateConstraint::AggregateBoundary
                {
                    self.report("E2202", "invalid-initializer-type", expression.span());
                    return None;
                }
                // A resolved member cast remains an rvalue in the written
                // expression, but its checked copy plan may directly name
                // the temporary's initializer. The identity cast adds no
                // evaluation and must not hide static initializer actions.
                let expression = match expression {
                    CHirExpr::Cast(cast, literal @ CHirExpr::CompoundLit(ty, _, _), _)
                        if cast == ty =>
                    {
                        literal
                    }
                    _ => expression,
                };
                actions.push(InitAction::copy_aggregate(
                    path,
                    target,
                    expression,
                    expression.span(),
                ));
                Some(())
            }
            (TypeKind::Array { .. }, WrittenInitializer::Expression(expression)) => {
                self.report("E2202", "invalid-initializer-type", expression.span());
                None
            }
            (
                TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_)
                | TypeKind::TargetBuiltin(_),
                WrittenInitializer::Expression(expression),
            ) => {
                let mut expression: &'arena CHirExpr<'arena> = expression;
                if self.incompatible_pointer_conversion(target, expression)
                    && self.record_incompatible_pointers(expression.span())
                {
                    expression = self.incompatible_pointer_cast(target, expression);
                }
                if self.assignment_constraint(AssignmentOp::Simple, target, expression)
                    != UpdateConstraint::Scalar
                {
                    self.report("E2202", "invalid-initializer-type", expression.span());
                    return None;
                }
                let expression = self.explicit_floating_integer_conversion(target, expression);
                actions.push(InitAction::store_scalar(
                    path,
                    target,
                    expression,
                    expression.span(),
                ));
                Some(())
            }
            (
                TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_)
                | TypeKind::TargetBuiltin(_),
                WrittenInitializer::List {
                    elements,
                    designations,
                    ..
                },
            ) => {
                if let Some(designation) = designations.iter().flatten().next() {
                    self.report("E2906", "designator-outside-object", designation.span);
                    return None;
                }
                let Some(first) = elements.first() else {
                    actions.push(InitAction::zero_scalar(path, target));
                    return Some(());
                };
                self.normalize_initializer_value(target, path, first, actions)?;
                if let Some(excess) = elements.get(1) {
                    self.report("E2201", "excess-initializer", excess.span());
                    return None;
                }
                Some(())
            }
            _ => {
                self.report("E2202", "invalid-initializer-type", written.span());
                None
            }
        }
    }

    pub(super) fn normalize_aggregate_elements(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        elements: &[WrittenInitializer<'arena>],
        designations: &[Option<InitializerDesignation<'arena>>],
        actions: &mut Vec<InitAction<'arena>>,
        diagnose_excess: bool,
    ) -> Option<()> {
        if self.aggregate_first_initializable_member_is_unbounded(target)
            && self.final_flexible_array_member(target).is_none()
        {
            if let Some(first) = elements.first() {
                self.report(
                    "E2706",
                    "unsupported-unbounded-array-initializer",
                    first.span(),
                );
                return None;
            }
        }
        if designations.iter().any(Option::is_some) {
            return self.normalize_designated_aggregate_elements(
                target,
                path,
                elements,
                designations,
                actions,
            );
        }
        let Some(children) = self.aggregate_initializer_children(target) else {
            self.report(
                "E2202",
                "invalid-initializer-type",
                elements
                    .first()
                    .map_or_else(|| self.span(), WrittenInitializer::span),
            );
            return None;
        };
        if !self.flexible_initializer_allowed
            && self.final_flexible_array_member(target).is_some()
            && elements.len() > children.len()
        {
            self.report(
                "E2919",
                "flexible-array-initializer-not-static",
                elements[children.len()].span(),
            );
            return None;
        }
        let mut cursor = 0;
        for (step, child) in children {
            let child_path = extend_subobject_path(path.clone(), step);
            if cursor < elements.len() {
                self.normalize_stream_subobject(child, child_path, elements, &mut cursor, actions)?;
            }
        }
        if diagnose_excess {
            if let Some(excess) = elements.get(cursor) {
                self.report("E2201", "excess-initializer", excess.span());
                return None;
            }
        }
        Some(())
    }

    pub(super) fn normalize_designated_aggregate_elements(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        elements: &[WrittenInitializer<'arena>],
        designations: &[Option<InitializerDesignation<'arena>>],
        actions: &mut Vec<InitAction<'arena>>,
    ) -> Option<()> {
        if elements.len() != designations.len() {
            unreachable!("initializer elements and designations are positional peers");
        }
        if !self.flexible_initializer_allowed
            && self.final_flexible_array_member(target).is_some()
            && designations.iter().flatten().any(|designation| {
                designation.designators.iter().any(|designator| {
                    matches!(designator, WrittenDesignator::Field(name) if self.final_flexible_array_member(target).is_some_and(|(_, field, _)| name.spelling == field))
                })
            })
        {
            let span = designations
                .iter()
                .flatten()
                .find_map(|designation| {
                    designation.designators.iter().find_map(|designator| match designator {
                        WrittenDesignator::Field(name) => Some(name.span),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| self.span());
            self.report("E2919", "flexible-array-initializer-not-static", span);
            return None;
        }
        if !matches!(
            self.types.get(target)?.kind,
            TypeKind::Array { .. } | TypeKind::Struct(_) | TypeKind::Union(_)
        ) {
            let span = designations
                .iter()
                .flatten()
                .next()
                .map_or_else(|| self.span(), |designation| designation.span);
            self.report("E2906", "designator-outside-object", span);
            return None;
        }

        let mut cursor = self.next_initializer_child(target, &path, None);
        for (element, designation) in elements.iter().zip(designations) {
            let range = designation.as_ref().is_some_and(|designation| {
                designation
                    .designators
                    .iter()
                    .any(|designator| matches!(designator, WrittenDesignator::Range { .. }))
            });
            let destinations = self.initializer_element_destinations(
                target,
                &path,
                element,
                designation.as_ref(),
                &mut cursor,
            )?;
            if range {
                let array_value = destinations.first().is_some_and(|destination| {
                    matches!(
                        self.types.get(destination.ty).map(|ty| &ty.kind),
                        Some(TypeKind::Array { .. })
                    )
                });
                self.require_admitted_range_initializer(element, array_value)?;
            }
            let mut first_values = Vec::new();
            for (index, destination) in destinations.into_iter().enumerate() {
                let start = actions.len();
                self.normalize_initializer_value(
                    destination.ty,
                    destination.path,
                    element,
                    actions,
                )?;
                if range {
                    // A range names many destinations, but each leaf value is
                    // one source occurrence. Keep its converted identity even
                    // when conversion allocated a new expression node.
                    let mut value_index = 0;
                    for action in &mut actions[start..] {
                        if let InitAction::StoreScalar { value, .. }
                        | InitAction::CopyAggregate { value, .. } = action
                        {
                            if index == 0 {
                                first_values.push(*value);
                            } else {
                                *value = first_values[value_index];
                            }
                            value_index += 1;
                        }
                    }
                }
            }
        }
        Some(())
    }

    /// EvoIR 10.1.12: a nonconstant range value that must be constant is
    /// `E2969` at the value. On Darwin, each range value must be constant.
    /// On Linux, an array value must be constant, as in GCC 14; the shared
    /// Linux plan takes the scalar and record values.
    pub(super) fn require_admitted_range_initializer(
        &mut self,
        written: &WrittenInitializer<'arena>,
        array_value: bool,
    ) -> Option<()> {
        match written {
            WrittenInitializer::Expression(expression) => {
                if !self.static_initializer_expression_is_constant(expression)
                    && (array_value || self.target_layout.names_clang_extensions())
                {
                    self.report(
                        "E2969",
                        "range-designator-value-not-constant",
                        expression.span(),
                    );
                    return None;
                }
            }
            WrittenInitializer::List { elements, .. } => {
                for element in elements {
                    self.require_admitted_range_initializer(element, array_value)?;
                }
            }
        }
        Some(())
    }

    /// A brace-list cursor is a complete subobject path, not an index into
    /// only the outer aggregate. Bound inference and normalization consume
    /// the same destinations so nested continuations cannot disagree.
    pub(super) fn initializer_element_destinations(
        &mut self,
        target: TypeId,
        base_path: &SubobjectPath,
        written: &WrittenInitializer<'arena>,
        designation: Option<&InitializerDesignation<'arena>>,
        cursor: &mut Option<ResolvedInitializerTarget>,
    ) -> Option<Vec<ResolvedInitializerTarget>> {
        let mut destinations = if let Some(designation) = designation {
            self.resolve_initializer_designation(target, base_path, designation)?
        } else if let Some(destination) = cursor.clone() {
            vec![destination]
        } else {
            self.report("E2201", "excess-initializer", written.span());
            return None;
        };
        for destination in &mut destinations {
            while !matches!(written, WrittenInitializer::List { .. })
                && !self.direct_initializer_expression(destination.ty, written)
                // Keep the existing flexible-member checks in the normalizer;
                // descending past an unbounded member would bypass them.
                && !matches!(
                    self.types.get(destination.ty)?.kind,
                    TypeKind::Array { bound: ArrayBound::Unbounded, .. }
                )
            {
                let Some(child) =
                    self.next_initializer_child(destination.ty, &destination.path, None)
                else {
                    break;
                };
                *destination = child;
            }
            *cursor = self.initializer_successor(target, base_path, &destination.path);
        }
        Some(destinations)
    }
}
