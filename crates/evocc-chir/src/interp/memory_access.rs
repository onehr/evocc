//! C-HIR interpreter memory access.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn load_memory_value(
        &mut self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        // Decay of an incomplete array does not require its size.
        let layout = if matches!(
            self.unit.types().get(ty).map(|ty| &ty.kind),
            Some(TypeKind::Array { .. })
        ) {
            AccessLayout::new(0, 1)
        } else {
            self.access_layout(ty, span)?
        };
        self.load_memory_value_with_layout(address, ty, qualifiers, span, layout)
    }

    pub(super) fn place_access_layout(
        &self,
        place: &MemoryPlace<'_>,
        span: SourceSpan,
    ) -> Result<AccessLayout, InterpreterError> {
        if matches!(
            self.unit.types().get(place.value_ty()).map(|ty| &ty.kind),
            Some(TypeKind::Array { .. })
        ) {
            return Ok(AccessLayout::new(0, 1));
        }
        place
            .access_layout(self.unit.types(), &self.target)
            .map(AccessLayout::from_type_layout)
            .ok_or(InterpreterError::UnsupportedType {
                ty: place.value_ty(),
                span,
            })
    }

    pub(super) fn load_memory_value_with_layout(
        &mut self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        span: SourceSpan,
        layout: AccessLayout,
    ) -> Result<CValue, InterpreterError> {
        // Array-to-pointer conversion does not access the array object or
        // produce a volatile event, even when it is a projected subobject.
        if let Some(TypeKind::Array { element, .. }) = self.unit.types().get(ty).map(|ty| &ty.kind)
        {
            let pointer_type = PointerTypeDesc::to_object(self.unit.types(), *element)
                .ok_or(InterpreterError::UnsupportedType { ty, span })?;
            return Ok(CValue::Pointer {
                value: address,
                ty: pointer_type,
            });
        }
        if qualifiers.is_atomic {
            return self.load_atomic_object(address, ty, qualifiers.is_volatile, layout, span);
        }
        match self.value_type(ty, span)? {
            ValueType::Floating(kind) => {
                let raw = if qualifiers.is_volatile {
                    let (raw, world) = self
                        .memory
                        .volatile_load_integer(&self.world, address, layout)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.world = world;
                    raw
                } else {
                    self.memory
                        .load_integer(address, layout)
                        .map_err(|error| self.memory_error(error, span))?
                };
                Ok(CValue::Floating(
                    evocc_types::floating::FloatBits::from_bits(kind, raw).unwrap(),
                ))
            }
            ValueType::Integer(integer_type) => {
                let raw = if qualifiers.is_volatile {
                    let (raw, world) = self
                        .memory
                        .volatile_load_integer(&self.world, address, layout)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.world = world;
                    raw
                } else {
                    self.memory
                        .load_integer(address, layout)
                        .map_err(|error| self.memory_error(error, span))?
                };
                Ok(CValue::Integer(CInteger::from_raw(raw, integer_type)))
            }
            ValueType::Pointer(pointer_type) => {
                let is_function = matches!(
                    self.unit
                        .types()
                        .get(pointer_type.pointee())
                        .map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                );
                if is_function {
                    let value = if qualifiers.is_volatile {
                        let (value, world) = self
                            .memory
                            .volatile_load_pointer_value(&self.world, address, layout)
                            .map_err(|error| {
                                Self::pointer_load_error(error, address, layout, span)
                            })?;
                        self.world = world;
                        value
                    } else {
                        self.memory
                            .load_pointer_value(address, layout)
                            .map_err(|error| {
                                Self::pointer_load_error(error, address, layout, span)
                            })?
                    };
                    return Ok(CValue::FunctionPointer {
                        value,
                        ty: pointer_type,
                    });
                }
                let value = if qualifiers.is_volatile {
                    let (value, world) = self
                        .memory
                        .volatile_load_pointer(&self.world, address, layout)
                        .map_err(|error| Self::pointer_load_error(error, address, layout, span))?;
                    self.world = world;
                    value
                } else {
                    self.memory
                        .load_pointer(address, layout)
                        .map_err(|error| Self::pointer_load_error(error, address, layout, span))?
                };
                Ok(CValue::Pointer {
                    value,
                    ty: pointer_type,
                })
            }
            ValueType::Aggregate(aggregate_type) => {
                if matches!(
                    self.unit.types().get(aggregate_type).map(|ty| &ty.kind),
                    Some(TypeKind::Vector { .. })
                ) || crate::complex::admitted(self.unit.types(), aggregate_type)
                {
                    let snapshot = self.vector_temporary(aggregate_type, span)?;
                    if qualifiers.is_volatile {
                        // A volatile complex object is read as its two
                        // parts, each one volatile access; a volatile vector
                        // as one access, or lane by lane when it is wide.
                        self.copy_volatile_aggregate(
                            snapshot,
                            aggregate_type,
                            Qualifiers::default(),
                            address,
                            aggregate_type,
                            qualifiers,
                            span,
                        )?;
                        return Ok(CValue::Aggregate {
                            address: snapshot,
                            ty: aggregate_type,
                            qualifiers: Qualifiers::default(),
                        });
                    }
                    let (memory, world) = self
                        .memory
                        .copy_with_volatile(
                            &self.world,
                            snapshot,
                            address,
                            layout,
                            CopyVolatility::new(qualifiers.is_volatile, false),
                        )
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                    return Ok(CValue::Aggregate {
                        address: snapshot,
                        ty: aggregate_type,
                        qualifiers: Qualifiers::default(),
                    });
                }
                Ok(CValue::Aggregate {
                    address,
                    ty: aggregate_type,
                    qualifiers,
                })
            }
            ValueType::Void => Err(InterpreterError::UnsupportedType { ty, span }),
        }
    }

    pub(super) fn store_memory_value(
        &mut self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        value: CValue,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let layout = self.access_layout(ty, span)?;
        self.store_memory_value_with_layout(address, ty, qualifiers, value, span, layout)
    }

    pub(super) fn store_static_scalar(
        &mut self,
        address: Pointer,
        ty: TypeId,
        value: CValue,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        self.store_initialized_value(address, ty, value, span)
    }

    pub(super) fn store_constant_representation(
        &mut self,
        address: Pointer,
        ty: TypeId,
        bits: CInteger,
        volatile: bool,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let mut layout = self.access_layout(ty, span)?;
        let object = self
            .memory
            .object_info(address.object())
            .map_err(|e| self.memory_error(e, span))?;
        layout.align_bytes = layout.align_bytes.min(object.layout.align_bytes);
        let offset = address.byte_offset().unsigned_abs();
        if offset != 0 {
            layout.align_bytes = layout
                .align_bytes
                .min(1_u32 << offset.trailing_zeros().min(31));
        }
        if volatile {
            let (memory, world) = self
                .memory
                .volatile_store_integer(&self.world, address, layout, bits.raw_bits())
                .map_err(|e| self.memory_error(e, span))?;
            self.memory = memory;
            self.world = world;
        } else {
            self.memory = self
                .memory
                .store_integer(address, layout, bits.raw_bits())
                .map_err(|e| self.memory_error(e, span))?;
        }
        Ok(())
    }

    /// The initializer path has already verified the subobject's identity.
    /// Its alignment follows the containing allocation and byte offset, not
    /// an arbitrary pointer cast to the scalar type.
    pub(super) fn store_initialized_value(
        &mut self,
        address: Pointer,
        ty: TypeId,
        value: CValue,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let mut layout = self.access_layout(ty, span)?;
        let object = self
            .memory
            .object_info(address.object())
            .map_err(|error| self.memory_error(error, span))?;
        layout.align_bytes = layout.align_bytes.min(object.layout.align_bytes);
        let offset = address.byte_offset().unsigned_abs();
        if offset != 0 {
            layout.align_bytes = layout
                .align_bytes
                .min(1_u32 << offset.trailing_zeros().min(31));
        }
        self.store_memory_value_with_layout(address, ty, Qualifiers::default(), value, span, layout)
    }

    pub(super) fn store_memory_value_with_layout(
        &mut self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        value: CValue,
        span: SourceSpan,
        layout: AccessLayout,
    ) -> Result<CValue, InterpreterError> {
        if qualifiers.is_atomic {
            return self.store_atomic_object(
                address,
                ty,
                qualifiers.is_volatile,
                value,
                span,
                layout,
            );
        }
        match (self.value_type(ty, span)?, value) {
            (ValueType::Floating(kind), value) => {
                let strict = self.strict_floating();
                let value = floating::convert_runtime(value, kind, span, &mut self.world, strict)?;
                if qualifiers.is_volatile {
                    let (memory, world) = self
                        .memory
                        .volatile_store_integer(&self.world, address, layout, value.bits())
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                } else {
                    self.memory = self
                        .memory
                        .store_integer(address, layout, value.bits())
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::Floating(value))
            }
            (_, CValue::Uninitialized(_)) => Err(InterpreterError::UninitializedValue { span }),
            (ValueType::Integer(integer_type), CValue::Integer(value)) => {
                let value = value.cast(integer_type);
                if qualifiers.is_volatile {
                    let (memory, world) = self
                        .memory
                        .volatile_store_integer(&self.world, address, layout, value.raw_bits())
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                } else {
                    self.memory = self
                        .memory
                        .store_integer(address, layout, value.raw_bits())
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::Integer(value))
            }
            (ValueType::Pointer(expected), CValue::Pointer { value, ty: actual })
                if expected == actual =>
            {
                if qualifiers.is_volatile {
                    let (memory, world) = self
                        .memory
                        .volatile_store_pointer(&self.world, address, layout, value)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                } else {
                    self.memory = self
                        .memory
                        .store_pointer(address, layout, value)
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::Pointer {
                    value,
                    ty: expected,
                })
            }
            (ValueType::Pointer(expected), CValue::FunctionPointer { value, ty: actual })
                if expected == actual
                    && matches!(
                        self.unit.types().get(expected.pointee()).map(|ty| &ty.kind),
                        Some(TypeKind::Function { .. })
                    ) =>
            {
                if qualifiers.is_volatile {
                    let (memory, world) = self
                        .memory
                        .volatile_store_pointer_value(&self.world, address, layout, value)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.memory = memory;
                    self.world = world;
                } else {
                    self.memory = self
                        .memory
                        .store_pointer_value(address, layout, value)
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::FunctionPointer {
                    value,
                    ty: expected,
                })
            }
            (
                ValueType::Aggregate(expected),
                CValue::Aggregate {
                    address: source,
                    ty: actual,
                    qualifiers: source_qualifiers,
                },
            ) if self.same_aggregate_type(expected, actual) => {
                if qualifiers.is_volatile || source_qualifiers.is_volatile {
                    self.copy_volatile_aggregate(
                        address,
                        expected,
                        qualifiers,
                        source,
                        actual,
                        source_qualifiers,
                        span,
                    )?;
                } else {
                    self.memory = self
                        .memory
                        .copy_bytes(address, source, layout)
                        .map_err(|error| self.memory_error(error, span))?;
                }
                Ok(CValue::Aggregate {
                    address,
                    ty: expected,
                    qualifiers,
                })
            }
            _ => Err(InterpreterError::UnsupportedExpression {
                feature: "incompatible memory store value",
                span,
            }),
        }
    }
}
