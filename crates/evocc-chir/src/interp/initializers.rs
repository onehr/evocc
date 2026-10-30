//! C-HIR interpreter initializers.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    /// Resolve a designated subobject to a byte offset within its object.
    ///
    /// Field offsets come from the target record layout and element offsets
    /// from the element size, so this is the same address arithmetic a
    /// projected place performs, driven by constant steps instead of by
    /// evaluated subscripts.
    pub(super) fn subobject_offset(
        &self,
        object_ty: TypeId,
        path: &SubobjectPath,
        span: SourceSpan,
    ) -> Result<(i64, TypeId, Option<BitFieldLayout>), InterpreterError> {
        let mut current = object_ty;
        let mut bit_field = None;
        let mut offset: i64 = 0;
        for step in path.steps() {
            match step {
                SubobjectStep::Field { field_index } => {
                    let tag = match self.unit.types().get(current).map(|ty| &ty.kind) {
                        Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => *tag,
                        _ => return Err(InterpreterError::UnsupportedType { ty: current, span }),
                    };
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), tag)
                        .map_err(|_| InterpreterError::UnsupportedType { ty: current, span })?;
                    let field = layout
                        .fields
                        .iter()
                        .find(|field| field.field_index == *field_index)
                        .ok_or(InterpreterError::UnsupportedType { ty: current, span })?;
                    bit_field = field.bit_field;
                    offset =
                        offset
                            .checked_add(i64::try_from(field.byte_offset).map_err(|_| {
                                InterpreterError::UnsupportedType { ty: current, span }
                            })?)
                            .ok_or(InterpreterError::UnsupportedType { ty: current, span })?;
                    current = self
                        .unit
                        .types()
                        .tag(tag)
                        .and_then(|definition| match &definition.state {
                            TagState::Complete(
                                TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                            ) => fields.get(*field_index).map(|field| field.ty),
                            _ => None,
                        })
                        .ok_or(InterpreterError::UnsupportedType { ty: current, span })?;
                }
                SubobjectStep::Index { index } => {
                    let element = match self.unit.types().get(current).map(|ty| &ty.kind) {
                        Some(
                            TypeKind::Array { element, .. } | TypeKind::Vector { element, .. },
                        ) => *element,
                        _ => return Err(InterpreterError::UnsupportedType { ty: current, span }),
                    };
                    let size = i64::try_from(self.access_layout(element, span)?.size_bytes)
                        .map_err(|_| InterpreterError::UnsupportedType { ty: element, span })?;
                    let step = i64::try_from(*index)
                        .ok()
                        .and_then(|index| index.checked_mul(size))
                        .ok_or(InterpreterError::UnsupportedType { ty: element, span })?;
                    offset = offset
                        .checked_add(step)
                        .ok_or(InterpreterError::UnsupportedType { ty: element, span })?;
                    current = element;
                }
            }
        }
        Ok((offset, current, bit_field))
    }

    /// Address of one designated subobject of an already-bound object.
    pub(super) fn subobject_address(
        &self,
        binding: MemoryBinding,
        path: &SubobjectPath,
        span: SourceSpan,
    ) -> Result<(Pointer, TypeId, Option<BitFieldLayout>), InterpreterError> {
        let (offset, ty, bit_field) = self.subobject_offset(binding.ty, path, span)?;
        let address = binding
            .address
            .checked_offset(offset)
            .map_err(|error| self.memory_error(error, span))?;
        Ok((address, ty, bit_field))
    }

    /// Schedule the next action of an initializer plan, if any remain.
    pub(super) fn continue_memory_initializer(
        &mut self,
        target: InitializerTarget<'arena>,
        index: usize,
        tasks: &mut Vec<MachineTask<'arena>>,
        values: &mut Vec<CValue>,
    ) -> Result<(), InterpreterError> {
        let initializer = target.initializer;
        if index == 0 {
            self.initializer_values.remove(&target.binding.address);
        }
        let Some(action) = initializer.plan().actions().get(index) else {
            self.initializer_values.remove(&target.binding.address);
            return Ok(());
        };
        if let InitAction::StoreScalar {
            path,
            destination_type,
            value,
            span,
        } = action
        {
            if let Some(bits) = crate::constant::floating_constant_representation(
                value,
                *destination_type,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            )
            .filter(|_| !self.unit.runtime_floating_environment())
            {
                let (address, actual_ty, bit_field) =
                    self.subobject_address(target.binding, path, *span)?;
                if actual_ty != *destination_type || bit_field.is_some() {
                    return Err(invalid_initializer_target(target, *span));
                }
                let volatile = self
                    .unit
                    .types()
                    .get(actual_ty)
                    .is_some_and(|ty| ty.qualifiers.is_volatile);
                self.store_constant_representation(address, actual_ty, bits, volatile, *span)?;
                tasks.push(MachineTask::ContinueMemoryInitializer {
                    target,
                    index: index + 1,
                });
                return Ok(());
            }
        }
        match action {
            InitAction::StoreScalar {
                path, span, value, ..
            }
            | InitAction::CopyAggregate {
                path, span, value, ..
            } => {
                // Reject before evaluating, so a rejected destination cannot
                // run the initializer's side effects first.
                self.subobject_address(target.binding, path, *span)?;
                tasks.push(MachineTask::ContinueMemoryInitializer {
                    target,
                    index: index + 1,
                });
                tasks.push(MachineTask::FinishMemorySubobject { target, index });
                if let Some(cached) = initializer
                    .plan()
                    .shared_value(index)
                    .and_then(|shared| {
                        self.initializer_values
                            .get(&target.binding.address)?
                            .get(&shared)
                    })
                    .copied()
                {
                    values.push(cached);
                    return Ok(());
                }
                tasks.push(MachineTask::EvalExpression {
                    expression: value,
                    allow_root_mutation: true,
                });
                Ok(())
            }
            InitAction::ZeroScalar { path, .. } => {
                let span = initializer.span();
                let (address, ty, bit_field) =
                    self.subobject_address(target.binding, path, span)?;
                if matches!(
                    self.unit.types().get(ty).map(|ty| &ty.kind),
                    Some(TypeKind::Float(
                        evocc_types::FloatKind::F32
                            | evocc_types::FloatKind::F64
                            | evocc_types::FloatKind::F128
                    ))
                ) {
                    self.store_constant_representation(
                        address,
                        ty,
                        CInteger::new(0, CIntegerType::ULONG),
                        false,
                        span,
                    )?;
                    tasks.push(MachineTask::ContinueMemoryInitializer {
                        target,
                        index: index + 1,
                    });
                    return Ok(());
                }
                let value = self.zero_value(ty, span)?;
                if let Some(field) = bit_field {
                    self.store_bit_field(address, ty, field, value, false, span)?;
                } else {
                    self.store_initialized_value(address, ty, value, span)?;
                }
                tasks.push(MachineTask::ContinueMemoryInitializer {
                    target,
                    index: index + 1,
                });
                Ok(())
            }
            InitAction::StoreString {
                path,
                destination_type,
                literal,
                span,
            } => {
                let (address, actual_type, _) =
                    self.subobject_address(target.binding, path, *span)?;
                if actual_type != *destination_type {
                    return Err(invalid_initializer_target(target, *span));
                }
                let (layout, value) = self
                    .string_initializer_bytes(*destination_type, literal, *span)
                    .map_err(|_| invalid_initializer_target(target, *span))?;
                self.store_initializer_bytes(address, layout, &value, *span)?;
                tasks.push(MachineTask::ContinueMemoryInitializer {
                    target,
                    index: index + 1,
                });
                Ok(())
            }
        }
    }

    pub(super) fn unreachable_initializer_shapes(
        &mut self,
        declaration: &'arena ObjectDecl<'arena>,
        initializer: &crate::ObjectInitializer<'arena>,
    ) -> Result<(), InterpreterError> {
        let action = initializer.plan().actions().first().ok_or(
            InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            },
        )?;
        match action {
            InitAction::StoreScalar { span, .. }
            | InitAction::StoreString { span, .. }
            | InitAction::CopyAggregate { span, .. } => {
                Err(InterpreterError::InvalidInitializerPlan {
                    symbol: declaration.symbol,
                    span: *span,
                })
            }
            InitAction::ZeroScalar { .. } => Err(InterpreterError::InvalidInitializerPlan {
                symbol: declaration.symbol,
                span: initializer.span(),
            }),
        }
    }
}
