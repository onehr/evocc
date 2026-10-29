//! C-HIR interpreter volatile copies.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn same_aggregate_type(&self, left: TypeId, right: TypeId) -> bool {
        matches!(
            (self.unit.types().get(left), self.unit.types().get(right)),
            (Some(left), Some(right))
                if (matches!(left.kind, TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList | TypeKind::Vector { .. } | TypeKind::Complex(evocc_types::ComplexBase::Integer(_)))
                    || matches!(left.kind, TypeKind::Complex(base) if base.executable_part().is_some()))
                    && left.kind == right.kind
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn copy_volatile_aggregate(
        &mut self,
        destination: Pointer,
        destination_type: TypeId,
        destination_qualifiers: Qualifiers,
        source: Pointer,
        source_type: TypeId,
        source_qualifiers: Qualifiers,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let destination_kind = self
            .unit
            .types()
            .get(destination_type)
            .ok_or(InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            })?
            .kind
            .clone();
        let source_kind = self
            .unit
            .types()
            .get(source_type)
            .ok_or(InterpreterError::UnsupportedType {
                ty: source_type,
                span,
            })?
            .kind
            .clone();
        if destination_kind != source_kind {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "incompatible volatile aggregate copy",
                span,
            });
        }
        match destination_kind {
            TypeKind::Struct(tag) => {
                let fields = match &self
                    .unit
                    .types()
                    .tag(tag)
                    .ok_or(InterpreterError::UnsupportedType {
                        ty: destination_type,
                        span,
                    })?
                    .state
                {
                    TagState::Complete(TagDefinition::Struct(fields)) => fields.clone(),
                    _ => {
                        return Err(InterpreterError::UnsupportedType {
                            ty: destination_type,
                            span,
                        })
                    }
                };
                let record = self
                    .target
                    .record_layout(self.unit.types(), tag)
                    .map_err(|_| InterpreterError::UnsupportedType {
                        ty: destination_type,
                        span,
                    })?;
                for (field_index, field) in fields.iter().enumerate() {
                    let layout_field = record
                        .fields
                        .iter()
                        .find(|layout| layout.field_index == field_index)
                        .ok_or(InterpreterError::UnsupportedType { ty: field.ty, span })?;
                    if layout_field.bit_field.is_some() {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "volatile aggregate bit-field copy",
                            span,
                        });
                    }
                    if matches!(
                        self.unit.types().get(field.ty).map(|ty| &ty.kind),
                        Some(TypeKind::Array {
                            bound: ArrayBound::Unbounded,
                            ..
                        })
                    ) {
                        continue;
                    }
                    let offset = i64::try_from(layout_field.byte_offset)
                        .map_err(|_| InterpreterError::UnsupportedType { ty: field.ty, span })?;
                    let destination = destination
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    let source = source
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    let field_qualifiers = self
                        .unit
                        .types()
                        .get(field.ty)
                        .ok_or(InterpreterError::UnsupportedType { ty: field.ty, span })?
                        .qualifiers;
                    self.copy_volatile_subobject(
                        destination,
                        field.ty,
                        merge_memory_qualifiers(destination_qualifiers, field_qualifiers),
                        source,
                        field.ty,
                        merge_memory_qualifiers(source_qualifiers, field_qualifiers),
                        span,
                    )?;
                }
                Ok(())
            }
            TypeKind::Complex(_)
                if crate::complex::admitted(self.unit.types(), destination_type) =>
            {
                let element = crate::complex::element(self.unit.types(), destination_type).ok_or(
                    InterpreterError::UnsupportedType {
                        ty: destination_type,
                        span,
                    },
                )?;
                let width = self.access_layout(element, span)?.size_bytes as i64;
                for offset in [0, width] {
                    let destination = destination
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    let source = source
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.copy_volatile_subobject(
                        destination,
                        element,
                        destination_qualifiers,
                        source,
                        element,
                        source_qualifiers,
                        span,
                    )?;
                }
                Ok(())
            }
            TypeKind::Vector { element, lanes, .. } => {
                let layout = self.access_layout(destination_type, span)?;
                if layout.size_bytes > 16 {
                    // A wide vector is an object of lanes: it is accessed
                    // lane by lane, as an array is.
                    let width = self.access_layout(element, span)?.size_bytes;
                    for lane in 0..u64::from(lanes) {
                        let offset = (lane * width) as i64;
                        self.copy_volatile_subobject(
                            destination
                                .checked_offset(offset)
                                .map_err(|error| self.memory_error(error, span))?,
                            element,
                            destination_qualifiers,
                            source
                                .checked_offset(offset)
                                .map_err(|error| self.memory_error(error, span))?,
                            element,
                            source_qualifiers,
                            span,
                        )?;
                    }
                    return Ok(());
                }
                // A short vector is one access of its size, as an EvoIR
                // `load` or `store` of a vector value is.
                let (representation, world) = self
                    .memory
                    .load_representation(&self.world, source, layout, source_qualifiers.is_volatile)
                    .map_err(|error| self.memory_error(error, span))?;
                self.world = world;
                let (memory, world) = self
                    .memory
                    .store_representation(
                        &self.world,
                        destination,
                        layout,
                        &representation,
                        destination_qualifiers.is_volatile,
                    )
                    .map_err(|error| self.memory_error(error, span))?;
                self.memory = memory;
                self.world = world;
                Ok(())
            }
            TypeKind::Union(_) => {
                let layout = self.access_layout(destination_type, span)?;
                let volatile = CopyVolatility::new(
                    source_qualifiers.is_volatile,
                    destination_qualifiers.is_volatile,
                );
                if !volatile.supports(layout) {
                    return Err(InterpreterError::VolatileAggregate { span });
                }
                let (memory, world) = self
                    .memory
                    .copy_with_volatile(&self.world, destination, source, layout, volatile)
                    .map_err(|error| self.memory_error(error, span))?;
                self.memory = memory;
                self.world = world;
                Ok(())
            }
            _ => Err(InterpreterError::UnsupportedExpression {
                feature: "volatile aggregate copy reached non-aggregate",
                span,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn copy_volatile_subobject(
        &mut self,
        destination: Pointer,
        destination_type: TypeId,
        destination_qualifiers: Qualifiers,
        source: Pointer,
        source_type: TypeId,
        source_qualifiers: Qualifiers,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        // A complex member is copied as its two parts.
        let parts = crate::complex::admitted(self.unit.types(), destination_type);
        match self
            .unit
            .types()
            .get(destination_type)
            .map(|ty| ty.kind.clone())
        {
            Some(TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::Vector { .. }) => self
                .copy_volatile_aggregate(
                    destination,
                    destination_type,
                    destination_qualifiers,
                    source,
                    source_type,
                    source_qualifiers,
                    span,
                ),
            Some(TypeKind::Complex(_)) if parts => self.copy_volatile_aggregate(
                destination,
                destination_type,
                destination_qualifiers,
                source,
                source_type,
                source_qualifiers,
                span,
            ),
            Some(TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(count),
            }) => {
                let layout = self.access_layout(element, span)?;
                for index in 0..count {
                    let offset = index
                        .checked_mul(layout.size_bytes)
                        .and_then(|offset| i64::try_from(offset).ok())
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: destination_type,
                            span,
                        })?;
                    self.copy_volatile_subobject(
                        destination
                            .checked_offset(offset)
                            .map_err(|error| self.memory_error(error, span))?,
                        element,
                        destination_qualifiers,
                        source
                            .checked_offset(offset)
                            .map_err(|error| self.memory_error(error, span))?,
                        element,
                        source_qualifiers,
                        span,
                    )?;
                }
                Ok(())
            }
            Some(TypeKind::Array { .. }) => Ok(()),
            Some(_) => {
                let value = self.load_memory_value(source, source_type, source_qualifiers, span)?;
                self.store_memory_value(
                    destination,
                    destination_type,
                    destination_qualifiers,
                    value,
                    span,
                )?;
                Ok(())
            }
            None => Err(InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            }),
        }
    }
}
