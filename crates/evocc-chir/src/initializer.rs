//! Lossless written initializers and checked normalized initialization plans.
//!
//! Parsing preserves braces in [`WrittenInitializer`].  A later semantic pass
//! may normalize that tree into ordered [`InitAction`]s.  This module defines
//! and validates the representation; it does not perform normalization.

use std::fmt;

use evocc_common::SourceSpan;
use evocc_types::{
    ArrayBound, IntRank, LayoutError, TagDefinition, TagId, TagState, TargetDataLayout, TypeId,
    TypeInterner, TypeKind, TypeLayout,
};

use crate::{CHirExpr, SpannedName, StringLit, StringPrefix};

/// The single authoritative representation of one written object
/// initializer and its checked, normalized meaning.
///
/// Keeping these together prevents later consumers from accidentally
/// interpreting the surface brace tree while ignoring the canonical write
/// plan (or vice versa).
#[derive(Debug, Clone)]
pub struct ObjectInitializer<'arena> {
    pub written: WrittenInitializer<'arena>,
    pub plan: InitPlan<'arena>,
}

impl<'arena> ObjectInitializer<'arena> {
    #[must_use]
    pub fn new(written: WrittenInitializer<'arena>, plan: InitPlan<'arena>) -> Self {
        Self { written, plan }
    }

    #[must_use]
    pub fn span(&self) -> SourceSpan {
        self.written.span()
    }

    #[must_use]
    pub fn written(&self) -> &WrittenInitializer<'arena> {
        &self.written
    }

    #[must_use]
    pub fn plan(&self) -> &InitPlan<'arena> {
        &self.plan
    }
}

/// Initializer syntax exactly as written, including nested brace boundaries.
#[derive(Debug, Clone)]
pub enum WrittenInitializer<'arena> {
    Expression(&'arena CHirExpr<'arena>),
    List {
        elements: Vec<WrittenInitializer<'arena>>,
        /// Optional designator list paired positionally with `elements`.
        /// Keeping it beside the unmodified initializer preserves every
        /// written entry even when normalization later applies last-wins.
        designations: Vec<Option<InitializerDesignation<'arena>>>,
        span: SourceSpan,
    },
}

/// A source-level designator list preceding one initializer-list element.
#[derive(Debug, Clone)]
pub struct InitializerDesignation<'arena> {
    pub designators: Vec<WrittenDesignator<'arena>>,
    pub span: SourceSpan,
}

/// One source-level component of an initializer designation.
#[derive(Debug, Clone)]
pub enum WrittenDesignator<'arena> {
    Field(SpannedName),
    Index {
        expression: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    },
    Range {
        first: &'arena CHirExpr<'arena>,
        last: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    },
}

impl<'arena> WrittenInitializer<'arena> {
    #[must_use]
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Expression(expression) => expression.span(),
            Self::List { span, .. } => *span,
        }
    }

    #[must_use]
    pub fn elements(&self) -> Option<&[WrittenInitializer<'_>]> {
        match self {
            Self::Expression(_) => None,
            Self::List { elements, .. } => Some(elements),
        }
    }

    #[must_use]
    pub fn designations(&self) -> Option<&[Option<InitializerDesignation<'_>>]> {
        match self {
            Self::Expression(_) => None,
            Self::List { designations, .. } => Some(designations),
        }
    }

    #[must_use]
    pub fn expression(&self) -> Option<&'arena CHirExpr<'arena>> {
        match self {
            Self::Expression(expression) => Some(expression),
            Self::List { .. } => None,
        }
    }
}

/// One structural step from an initialized object to a subobject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SubobjectStep {
    Field { field_index: usize },
    Index { index: u64 },
}

/// A canonical structural path relative to an [`InitPlan`]'s target object.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubobjectPath {
    steps: Vec<SubobjectStep>,
}

impl SubobjectPath {
    #[must_use]
    pub fn root() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_field(mut self, field_index: usize) -> Self {
        self.steps.push(SubobjectStep::Field { field_index });
        self
    }

    #[must_use]
    pub fn with_index(mut self, index: u64) -> Self {
        self.steps.push(SubobjectStep::Index { index });
        self
    }

    #[must_use]
    pub fn steps(&self) -> &[SubobjectStep] {
        &self.steps
    }

    #[must_use]
    pub fn is_root(&self) -> bool {
        self.steps.is_empty()
    }

    /// Whether one structural path contains the other.  Exact duplicates
    /// therefore overlap as well.  Union-sibling overlap requires type
    /// information and is checked by [`InitPlan::validate`].
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.steps.starts_with(&other.steps) || other.steps.starts_with(&self.steps)
    }
}

/// One ordered, normalized write performed by object initialization.
#[derive(Debug, Clone)]
pub enum InitAction<'arena> {
    StoreScalar {
        path: SubobjectPath,
        destination_type: TypeId,
        value: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    },
    StoreString {
        path: SubobjectPath,
        destination_type: TypeId,
        literal: StringLit,
        span: SourceSpan,
    },
    /// Copy one complete record or union value into the designated
    /// subobject.  Keeping this distinct from a scalar store prevents later
    /// consumers from accidentally expanding the copy member by member.
    CopyAggregate {
        path: SubobjectPath,
        destination_type: TypeId,
        value: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    },
    ZeroScalar {
        path: SubobjectPath,
        destination_type: TypeId,
    },
}

impl<'arena> InitAction<'arena> {
    #[must_use]
    pub fn store_scalar(
        path: SubobjectPath,
        destination_type: TypeId,
        value: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Self {
        Self::StoreScalar {
            path,
            destination_type,
            value,
            span,
        }
    }

    #[must_use]
    /// Store the literal and all required trailing zero elements across the
    /// complete destination array.
    pub fn store_string(
        path: SubobjectPath,
        destination_type: TypeId,
        literal: StringLit,
        span: SourceSpan,
    ) -> Self {
        Self::StoreString {
            path,
            destination_type,
            literal,
            span,
        }
    }

    #[must_use]
    pub fn copy_aggregate(
        path: SubobjectPath,
        destination_type: TypeId,
        value: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Self {
        Self::CopyAggregate {
            path,
            destination_type,
            value,
            span,
        }
    }

    #[must_use]
    pub fn zero_scalar(path: SubobjectPath, destination_type: TypeId) -> Self {
        Self::ZeroScalar {
            path,
            destination_type,
        }
    }

    #[must_use]
    pub fn path(&self) -> &SubobjectPath {
        match self {
            Self::StoreScalar { path, .. }
            | Self::StoreString { path, .. }
            | Self::CopyAggregate { path, .. }
            | Self::ZeroScalar { path, .. } => path,
        }
    }

    #[must_use]
    pub fn destination_type(&self) -> TypeId {
        match self {
            Self::StoreScalar {
                destination_type, ..
            }
            | Self::StoreString {
                destination_type, ..
            }
            | Self::CopyAggregate {
                destination_type, ..
            }
            | Self::ZeroScalar {
                destination_type, ..
            } => *destination_type,
        }
    }

    /// Source span of an explicit write.  Synthetic zero-fill actions have no
    /// written source span.
    #[must_use]
    pub fn span(&self) -> Option<SourceSpan> {
        match self {
            Self::StoreScalar { span, .. }
            | Self::StoreString { span, .. }
            | Self::CopyAggregate { span, .. } => Some(*span),
            Self::ZeroScalar { .. } => None,
        }
    }
}

/// Ordered normalized actions for one object initializer.
#[derive(Debug, Clone)]
pub struct InitPlan<'arena> {
    target_type: TypeId,
    actions: Vec<InitAction<'arena>>,
    /// C initialization zeroes every subobject not named by an explicit
    /// action. Keeping that fact once on the plan makes plan size depend on
    /// source entries rather than on the destination object's extent.
    zero_fills_unwritten: bool,
    flexible_array_elements: u64,
}

impl<'arena> InitPlan<'arena> {
    /// Byte gaps in a constructor's object representation. Named members,
    /// selected union members and copied aggregate representations are kept;
    /// the complement includes inter-member padding and unused union tails.
    /// Bit-field bytes are initialized by the existing bit-field constructor
    /// path, including their unused bits. No value is read by this traversal.
    pub fn padding_ranges(
        &self,
        types: &TypeInterner,
        target: TargetDataLayout,
    ) -> Result<Vec<std::ops::Range<u64>>, LayoutError> {
        let size = target.layout_of(types, self.target_type)?.size_bytes;
        let mut occupied = Vec::new();
        let mut pending = vec![(self.target_type, 0u64, SubobjectPath::root())];
        while let Some((ty, offset, path)) = pending.pop() {
            let kind = &types.get(ty).ok_or(LayoutError::UnknownType(ty))?.kind;
            if matches!(
                kind,
                TypeKind::Array {
                    bound: ArrayBound::Unbounded,
                    ..
                }
            ) {
                continue;
            }
            let extent = target.layout_of(types, ty)?.size_bytes;
            let end = offset
                .checked_add(extent)
                .ok_or(LayoutError::ArithmeticOverflow)?;
            if self.actions.iter().any(|action| {
                matches!(action, InitAction::CopyAggregate { .. }) && action.path() == &path
            }) {
                occupied.push(offset..end);
                continue;
            }
            match kind {
                TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                    let definition = types.tag(*tag).ok_or(LayoutError::UnknownTag(*tag))?;
                    let TagState::Complete(
                        TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                    ) = &definition.state
                    else {
                        return Err(LayoutError::IncompleteTag(*tag));
                    };
                    let layout = target.record_layout(types, *tag)?;
                    let selected = if matches!(kind, TypeKind::Union(_)) {
                        self.actions
                            .iter()
                            .find_map(|action| {
                                match action.path().steps().strip_prefix(path.steps())?.first()? {
                                    SubobjectStep::Field { field_index } => Some(*field_index),
                                    SubobjectStep::Index { .. } => None,
                                }
                            })
                            .or_else(|| {
                                fields.iter().position(|field| {
                                    field.name.is_some() || field.bit_field.is_none()
                                })
                            })
                    } else {
                        None
                    };
                    for field in &layout.fields {
                        if selected.is_some_and(|selected| selected != field.field_index) {
                            continue;
                        }
                        let source = &fields[field.field_index];
                        if source.name.is_none() && source.bit_field.is_some() {
                            continue;
                        }
                        let at = offset
                            .checked_add(field.byte_offset)
                            .ok_or(LayoutError::ArithmeticOverflow)?;
                        if let Some(bits) = field.bit_field {
                            let bytes =
                                u64::from((bits.bit_offset % 8 + bits.width_bits).div_ceil(8));
                            occupied.push(
                                at..at
                                    .checked_add(bytes)
                                    .ok_or(LayoutError::ArithmeticOverflow)?,
                            );
                        } else {
                            pending.push((
                                source.ty,
                                at,
                                path.clone().with_field(field.field_index),
                            ));
                        }
                    }
                }
                TypeKind::Array {
                    element,
                    bound: ArrayBound::Fixed(count),
                } => {
                    if matches!(
                        types.get(*element).map(|ty| &ty.kind),
                        Some(TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::Array { .. })
                    ) {
                        let stride = target.layout_of(types, *element)?.size_bytes;
                        for index in 0..*count {
                            let at = index
                                .checked_mul(stride)
                                .and_then(|n| offset.checked_add(n))
                                .ok_or(LayoutError::ArithmeticOverflow)?;
                            pending.push((*element, at, path.clone().with_index(index)));
                        }
                    } else {
                        occupied.push(offset..end);
                    }
                }
                _ => occupied.push(offset..end),
            }
        }
        occupied.sort_unstable_by_key(|range| range.start);
        let mut padding = Vec::new();
        let mut end = 0;
        for range in occupied {
            if range.end > size {
                return Err(LayoutError::ArithmeticOverflow);
            }
            if end < range.start {
                padding.push(end..range.start);
            }
            end = end.max(range.end);
        }
        if end < size {
            padding.push(end..size);
        }
        Ok(padding)
    }

    #[must_use]
    pub fn new(target_type: TypeId, actions: Vec<InitAction<'arena>>) -> Self {
        Self {
            target_type,
            actions,
            zero_fills_unwritten: false,
            flexible_array_elements: 0,
        }
    }

    /// Build the canonical plan for a written initializer stream. When a
    /// later write overlaps an earlier write, only the later meaning remains;
    /// the lossless [`WrittenInitializer`] continues to retain both entries.
    #[must_use]
    pub fn with_last_write_wins(
        target_type: TypeId,
        actions: Vec<InitAction<'arena>>,
        types: &TypeInterner,
    ) -> Self {
        let mut retained: Vec<InitAction<'arena>> = Vec::with_capacity(actions.len());
        for action in actions {
            retained.retain(|earlier| {
                !paths_overlap(types, target_type, earlier.path(), action.path())
            });
            retained.push(action);
        }
        Self::new(target_type, retained)
    }

    #[must_use]
    pub fn target_type(&self) -> TypeId {
        self.target_type
    }

    #[must_use]
    pub fn actions(&self) -> &[InitAction<'arena>] {
        &self.actions
    }

    #[must_use]
    pub fn zero_fills_unwritten(&self) -> bool {
        self.zero_fills_unwritten
    }

    #[must_use]
    pub fn with_zero_fill_unwritten(mut self) -> Self {
        self.zero_fills_unwritten = true;
        self
    }

    /// Number of explicitly materialized elements in a final flexible array
    /// member.  This extends the declared object's storage without changing
    /// the record type or its `sizeof`.
    #[must_use]
    pub fn flexible_array_elements(&self) -> u64 {
        self.flexible_array_elements
    }

    /// Storage includes complete initialized flexible elements, including
    /// omitted members of the final element. The declared type is unchanged.
    pub fn storage_layout(
        &self,
        types: &TypeInterner,
        target: &TargetDataLayout,
    ) -> Result<TypeLayout, LayoutError> {
        let ty = self.target_type;
        let mut layout = target.layout_of(types, ty)?;
        if self.flexible_array_elements == 0 {
            return Ok(layout);
        }
        let TypeKind::Struct(tag) = types.get(ty).ok_or(LayoutError::UnknownType(ty))?.kind else {
            return Err(LayoutError::NonObjectType(ty));
        };
        let TagState::Complete(TagDefinition::Struct(fields)) =
            &types.tag(tag).ok_or(LayoutError::UnknownTag(tag))?.state
        else {
            return Err(LayoutError::IncompleteTag(tag));
        };
        let field = fields.last().ok_or(LayoutError::NonObjectType(ty))?;
        let TypeKind::Array {
            element,
            bound: ArrayBound::Unbounded,
        } = types
            .get(field.ty)
            .ok_or(LayoutError::UnknownType(field.ty))?
            .kind
        else {
            return Err(LayoutError::NonObjectType(field.ty));
        };
        let element = target.layout_of(types, element)?;
        layout.size_bytes = self
            .flexible_array_elements
            .checked_mul(element.size_bytes)
            .and_then(|extension| layout.size_bytes.checked_add(extension))
            .ok_or(LayoutError::ArithmeticOverflow)?;
        Ok(layout)
    }

    #[must_use]
    pub fn with_flexible_array_elements(mut self, elements: u64) -> Self {
        self.flexible_array_elements = elements;
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Validate path resolution, destination identity, action/type
    /// compatibility, and the single-writer invariant.
    pub fn validate(&self, types: &TypeInterner) -> Result<(), InitPlanError> {
        if types.get(self.target_type).is_none() {
            return Err(InitPlanError::UnknownTargetType {
                target: self.target_type,
            });
        }

        for (action_index, action) in self.actions.iter().enumerate() {
            let path = action.path();
            let resolved = resolve_path(types, self.target_type, path, action_index)?;
            let destination = action.destination_type();
            if types.get(destination).is_none() {
                return Err(InitPlanError::UnknownDestinationType {
                    action: action_index,
                    path: path.clone(),
                    destination,
                });
            }
            if resolved != destination {
                return Err(InitPlanError::DestinationTypeMismatch {
                    action: action_index,
                    path: path.clone(),
                    resolved,
                    destination,
                });
            }

            let destination_kind = &types
                .get(destination)
                .expect("destination existence checked above")
                .kind;
            match action {
                InitAction::StoreScalar { .. } if !is_scalar(destination_kind) => {
                    return Err(InitPlanError::ScalarDestinationIsNotScalar {
                        action: action_index,
                        path: path.clone(),
                        destination,
                    });
                }
                InitAction::StoreString { literal, .. } => {
                    validate_string_destination(
                        types,
                        action_index,
                        path,
                        destination,
                        destination_kind,
                        literal,
                    )?;
                }
                InitAction::CopyAggregate { .. }
                    if !matches!(
                        destination_kind,
                        TypeKind::Struct(_)
                            | TypeKind::Union(_)
                            | TypeKind::BuiltinVaList
                            | TypeKind::Vector { .. }
                    ) =>
                {
                    return Err(InitPlanError::AggregateDestinationIsNotRecord {
                        action: action_index,
                        path: path.clone(),
                        destination,
                    });
                }
                InitAction::ZeroScalar { .. } if !is_scalar(destination_kind) => {
                    return Err(InitPlanError::ZeroDestinationIsNotScalar {
                        action: action_index,
                        path: path.clone(),
                        destination,
                    });
                }
                InitAction::StoreScalar { .. }
                | InitAction::CopyAggregate { .. }
                | InitAction::ZeroScalar { .. } => {}
            }
        }

        let mut order: Vec<_> = (0..self.actions.len()).collect();
        order.sort_unstable_by_key(|index| self.actions[*index].path());
        for pair in order.windows(2) {
            let first_action = pair[0];
            let second_action = pair[1];
            let first_path = self.actions[first_action].path();
            let second_path = self.actions[second_action].path();
            if paths_overlap(types, self.target_type, first_path, second_path) {
                return Err(InitPlanError::OverlappingActions {
                    first_action,
                    second_action,
                    first_path: first_path.clone(),
                    second_path: second_path.clone(),
                });
            }
        }

        Ok(())
    }
}

fn paths_overlap(
    types: &TypeInterner,
    target: TypeId,
    first: &SubobjectPath,
    second: &SubobjectPath,
) -> bool {
    if first.overlaps(second) {
        return true;
    }

    let mut current = target;
    for (first_step, second_step) in first.steps().iter().zip(second.steps()) {
        if first_step != second_step {
            return matches!(
                (
                    types.get(current).map(|ty| &ty.kind),
                    first_step,
                    second_step
                ),
                (
                    Some(TypeKind::Union(_)),
                    SubobjectStep::Field { .. },
                    SubobjectStep::Field { .. }
                )
            );
        }
        let Some(next) = shared_step_type(types, current, *first_step) else {
            return false;
        };
        current = next;
    }
    false
}

fn shared_step_type(types: &TypeInterner, current: TypeId, step: SubobjectStep) -> Option<TypeId> {
    match (step, &types.get(current)?.kind) {
        (
            SubobjectStep::Index { .. },
            TypeKind::Array { element, .. } | TypeKind::Vector { element, .. },
        ) => Some(*element),
        (SubobjectStep::Field { field_index }, TypeKind::Struct(tag) | TypeKind::Union(tag)) => {
            let TagState::Complete(TagDefinition::Struct(fields) | TagDefinition::Union(fields)) =
                &types.tag(*tag)?.state
            else {
                return None;
            };
            fields.get(field_index).map(|field| field.ty)
        }
        _ => None,
    }
}

fn resolve_path(
    types: &TypeInterner,
    target: TypeId,
    path: &SubobjectPath,
    action: usize,
) -> Result<TypeId, InitPlanError> {
    let mut current = target;
    for (step_index, step) in path.steps.iter().copied().enumerate() {
        let kind = types
            .get(current)
            .ok_or_else(|| InitPlanError::UnknownTypeInPath {
                action,
                path: path.clone(),
                step: step_index,
                ty: current,
            })?
            .kind
            .clone();

        current = match (step, kind) {
            (SubobjectStep::Field { field_index }, TypeKind::Struct(tag))
            | (SubobjectStep::Field { field_index }, TypeKind::Union(tag)) => {
                resolve_field(types, tag, field_index, action, path, step_index)?
            }
            (SubobjectStep::Field { .. }, _) => {
                return Err(InitPlanError::FieldStepOnNonRecord {
                    action,
                    path: path.clone(),
                    step: step_index,
                    ty: current,
                });
            }
            (SubobjectStep::Index { index }, TypeKind::Array { element, bound }) => {
                if let ArrayBound::Fixed(bound) = bound {
                    if index >= bound {
                        return Err(InitPlanError::ArrayIndexOutOfBounds {
                            action,
                            path: path.clone(),
                            index,
                            bound,
                        });
                    }
                }
                element
            }
            (SubobjectStep::Index { index }, TypeKind::Vector { element, lanes, .. }) => {
                if index >= u64::from(lanes) {
                    return Err(InitPlanError::ArrayIndexOutOfBounds {
                        action,
                        path: path.clone(),
                        index,
                        bound: u64::from(lanes),
                    });
                }
                element
            }
            (SubobjectStep::Index { .. }, _) => {
                return Err(InitPlanError::IndexStepOnNonArray {
                    action,
                    path: path.clone(),
                    step: step_index,
                    ty: current,
                });
            }
        };
    }

    if types.get(current).is_none() {
        return Err(InitPlanError::UnknownTypeInPath {
            action,
            path: path.clone(),
            step: path.steps.len(),
            ty: current,
        });
    }
    Ok(current)
}

fn resolve_field(
    types: &TypeInterner,
    tag: TagId,
    field_index: usize,
    action: usize,
    path: &SubobjectPath,
    step: usize,
) -> Result<TypeId, InitPlanError> {
    let tag_data = types
        .tag(tag)
        .ok_or_else(|| InitPlanError::UnknownTagInPath {
            action,
            path: path.clone(),
            step,
            tag,
        })?;
    let definition = match &tag_data.state {
        TagState::Incomplete => {
            return Err(InitPlanError::IncompleteTagInPath {
                action,
                path: path.clone(),
                step,
                tag,
            });
        }
        TagState::Complete(definition) => definition,
    };
    let fields = match definition {
        TagDefinition::Struct(fields) | TagDefinition::Union(fields) => fields,
        TagDefinition::Enum(_) => {
            return Err(InitPlanError::TagDefinitionIsNotRecord {
                action,
                path: path.clone(),
                step,
                tag,
            });
        }
    };
    fields
        .get(field_index)
        .map(|field| field.ty)
        .ok_or_else(|| InitPlanError::RecordFieldOutOfBounds {
            action,
            path: path.clone(),
            field_index,
            field_count: fields.len(),
        })
}

fn is_scalar(kind: &TypeKind) -> bool {
    if let TypeKind::TargetBuiltin(kind) = kind {
        return kind.layout().is_some();
    }
    matches!(
        kind,
        TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_)
    )
}

fn validate_string_destination(
    types: &TypeInterner,
    action: usize,
    path: &SubobjectPath,
    destination: TypeId,
    destination_kind: &TypeKind,
    literal: &StringLit,
) -> Result<(), InitPlanError> {
    if !matches!(
        literal.prefix,
        StringPrefix::None | StringPrefix::U8 | StringPrefix::L | StringPrefix::L16
    ) {
        return Err(InitPlanError::UnsupportedStringPrefix {
            action,
            path: path.clone(),
            prefix: literal.prefix,
        });
    }
    let TypeKind::Array { element, bound } = destination_kind else {
        return Err(InitPlanError::StringDestinationIsNotCharacterArray {
            action,
            path: path.clone(),
            destination,
        });
    };
    let is_character = types.get(*element).is_some_and(|element_type| {
        !element_type.qualifiers.is_atomic
            && match literal.prefix {
                StringPrefix::None | StringPrefix::U8 => matches!(
                    element_type.kind,
                    TypeKind::Int(
                        IntRank::Char | IntRank::SignedPlainChar | IntRank::SChar | IntRank::UChar
                    )
                ),
                StringPrefix::L | StringPrefix::L16 => {
                    element_type.kind == TypeKind::Int(literal.prefix.element_rank())
                }
                StringPrefix::U16 | StringPrefix::U => false,
            }
    });
    if !is_character {
        return Err(InitPlanError::StringDestinationIsNotCharacterArray {
            action,
            path: path.clone(),
            destination,
        });
    }

    let Some(units) = literal.explicit_unit_count() else {
        return Err(InitPlanError::InvalidStringLiteral {
            action,
            path: path.clone(),
        });
    };
    if let ArrayBound::Fixed(bound) = bound {
        if units > *bound {
            return Err(InitPlanError::StringInitializerTooLong {
                action,
                path: path.clone(),
                units,
                bound: *bound,
            });
        }
    }
    Ok(())
}

/// A broken normalized-initializer invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitPlanError {
    UnknownTargetType {
        target: TypeId,
    },
    UnknownDestinationType {
        action: usize,
        path: SubobjectPath,
        destination: TypeId,
    },
    UnknownTypeInPath {
        action: usize,
        path: SubobjectPath,
        step: usize,
        ty: TypeId,
    },
    UnknownTagInPath {
        action: usize,
        path: SubobjectPath,
        step: usize,
        tag: TagId,
    },
    IncompleteTagInPath {
        action: usize,
        path: SubobjectPath,
        step: usize,
        tag: TagId,
    },
    TagDefinitionIsNotRecord {
        action: usize,
        path: SubobjectPath,
        step: usize,
        tag: TagId,
    },
    FieldStepOnNonRecord {
        action: usize,
        path: SubobjectPath,
        step: usize,
        ty: TypeId,
    },
    RecordFieldOutOfBounds {
        action: usize,
        path: SubobjectPath,
        field_index: usize,
        field_count: usize,
    },
    IndexStepOnNonArray {
        action: usize,
        path: SubobjectPath,
        step: usize,
        ty: TypeId,
    },
    ArrayIndexOutOfBounds {
        action: usize,
        path: SubobjectPath,
        index: u64,
        bound: u64,
    },
    DestinationTypeMismatch {
        action: usize,
        path: SubobjectPath,
        resolved: TypeId,
        destination: TypeId,
    },
    ScalarDestinationIsNotScalar {
        action: usize,
        path: SubobjectPath,
        destination: TypeId,
    },
    AggregateDestinationIsNotRecord {
        action: usize,
        path: SubobjectPath,
        destination: TypeId,
    },
    StringDestinationIsNotCharacterArray {
        action: usize,
        path: SubobjectPath,
        destination: TypeId,
    },
    UnsupportedStringPrefix {
        action: usize,
        path: SubobjectPath,
        prefix: StringPrefix,
    },
    InvalidStringLiteral {
        action: usize,
        path: SubobjectPath,
    },
    StringInitializerTooLong {
        action: usize,
        path: SubobjectPath,
        units: u64,
        bound: u64,
    },
    ZeroDestinationIsNotScalar {
        action: usize,
        path: SubobjectPath,
        destination: TypeId,
    },
    OverlappingActions {
        first_action: usize,
        second_action: usize,
        first_path: SubobjectPath,
        second_path: SubobjectPath,
    },
}

impl fmt::Display for InitPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTargetType { target } => {
                write!(
                    formatter,
                    "unknown initializer target type {}",
                    target.raw()
                )
            }
            Self::UnknownDestinationType {
                action,
                destination,
                ..
            } => write!(
                formatter,
                "initializer action {action} has unknown destination type {}",
                destination.raw()
            ),
            Self::UnknownTypeInPath {
                action, step, ty, ..
            } => write!(
                formatter,
                "initializer action {action} path step {step} reaches unknown type {}",
                ty.raw()
            ),
            Self::UnknownTagInPath {
                action, step, tag, ..
            } => write!(
                formatter,
                "initializer action {action} path step {step} reaches unknown tag {}",
                tag.raw()
            ),
            Self::IncompleteTagInPath {
                action, step, tag, ..
            } => write!(
                formatter,
                "initializer action {action} path step {step} enters incomplete tag {}",
                tag.raw()
            ),
            Self::TagDefinitionIsNotRecord {
                action, step, tag, ..
            } => write!(
                formatter,
                "initializer action {action} path step {step} enters non-record tag {}",
                tag.raw()
            ),
            Self::FieldStepOnNonRecord {
                action, step, ty, ..
            } => write!(
                formatter,
                "initializer action {action} uses field step {step} on type {}",
                ty.raw()
            ),
            Self::RecordFieldOutOfBounds {
                action,
                field_index,
                field_count,
                ..
            } => write!(
                formatter,
                "initializer action {action} selects field {field_index} from {field_count} fields"
            ),
            Self::IndexStepOnNonArray {
                action, step, ty, ..
            } => write!(
                formatter,
                "initializer action {action} uses index step {step} on type {}",
                ty.raw()
            ),
            Self::ArrayIndexOutOfBounds {
                action,
                index,
                bound,
                ..
            } => write!(
                formatter,
                "initializer action {action} index {index} is outside array bound {bound}"
            ),
            Self::DestinationTypeMismatch {
                action,
                resolved,
                destination,
                ..
            } => write!(
                formatter,
                "initializer action {action} path resolves to type {}, not destination type {}",
                resolved.raw(),
                destination.raw()
            ),
            Self::ScalarDestinationIsNotScalar {
                action,
                destination,
                ..
            } => write!(
                formatter,
                "initializer action {action} scalar store targets non-scalar type {}",
                destination.raw()
            ),
            Self::AggregateDestinationIsNotRecord {
                action,
                destination,
                ..
            } => write!(
                formatter,
                "initializer action {action} aggregate copy targets non-record type {}",
                destination.raw()
            ),
            Self::StringDestinationIsNotCharacterArray {
                action,
                destination,
                ..
            } => write!(
                formatter,
                "initializer action {action} string store targets non-character-array type {}",
                destination.raw()
            ),
            Self::UnsupportedStringPrefix { action, prefix, .. } => write!(
                formatter,
                "initializer action {action} uses unsupported wide string prefix {prefix:?}"
            ),
            Self::InvalidStringLiteral { action, .. } => write!(
                formatter,
                "initializer action {action} has inconsistent or invalid string source facts"
            ),
            Self::StringInitializerTooLong {
                action,
                units,
                bound,
                ..
            } => write!(
                formatter,
                "initializer action {action} string has {units} units for array bound {bound}"
            ),
            Self::ZeroDestinationIsNotScalar {
                action,
                destination,
                ..
            } => write!(
                formatter,
                "initializer action {action} scalar zero targets non-scalar type {}",
                destination.raw()
            ),
            Self::OverlappingActions {
                first_action,
                second_action,
                ..
            } => write!(
                formatter,
                "initializer actions {first_action} and {second_action} overlap"
            ),
        }
    }
}

impl std::error::Error for InitPlanError {}
