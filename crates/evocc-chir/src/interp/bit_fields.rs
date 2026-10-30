//! C-HIR interpreter bit fields.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn place_bit_field(
        &self,
        place: &MemoryPlace<'_>,
        span: SourceSpan,
    ) -> Result<Option<BitFieldLayout>, InterpreterError> {
        if !place.is_bit_field() {
            return Ok(None);
        }
        let Some(PlaceProjection::Field(field)) = place.projections().last() else {
            return Err(InterpreterError::UnsupportedExpression {
                span,
                feature: "bit-field place has no final field",
            });
        };
        let layout = self
            .target
            .record_layout(self.unit.types(), field.tag())
            .map_err(|_| InterpreterError::UnsupportedType {
                ty: place.value_ty(),
                span,
            })?;
        layout
            .fields
            .iter()
            .find(|entry| entry.field_index == field.field_index())
            .and_then(|entry| entry.bit_field)
            .map(Some)
            .ok_or(InterpreterError::UnsupportedExpression {
                span,
                feature: "bit-field place has no target layout",
            })
    }

    pub(super) fn normalize_bit_field(
        &self,
        value: CInteger,
        ty: TypeId,
        field: BitFieldLayout,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        let ValueType::Integer(integer) = self.value_type(ty, span)? else {
            return Err(InterpreterError::UnsupportedType { ty, span });
        };
        let mask = u128::MAX >> (128 - field.width_bits);
        let mut raw = value.raw_bits() & mask;
        if integer.is_signed() && raw & (1u128 << (field.width_bits - 1)) != 0 {
            raw |= !mask;
        }
        Ok(CInteger::from_raw(raw, integer))
    }

    pub(super) fn load_bit_field(
        &mut self,
        address: Pointer,
        ty: TypeId,
        field: BitFieldLayout,
        volatile: bool,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let mut raw = 0u128;
        if volatile {
            for (offset, bytes) in field.storage_units() {
                let first = u32::from(field.bit_offset).max(offset * 8);
                let last = (u32::from(field.bit_offset) + u32::from(field.width_bits))
                    .min((offset + bytes) * 8);
                let count = last - first;
                let at = address
                    .checked_offset(i64::from(offset) - i64::from(field.bit_offset / 8))
                    .map_err(|error| self.memory_error(error, span))?;
                // One volatile access of the unit, which demands only the
                // field's own bits of it (EvoIR 10.1.6).
                let own = (u128::MAX >> (128 - count)) << (first - offset * 8);
                let (value, world) = self
                    .memory
                    .volatile_load_integer_bits(
                        &self.world,
                        at,
                        AccessLayout::new(u64::from(bytes), 1),
                        own,
                    )
                    .map_err(|error| self.memory_error(error, span))?;
                self.world = world;
                let selected = (value >> (first - offset * 8)) & (u128::MAX >> (128 - count));
                raw |= selected << (first - u32::from(field.bit_offset));
            }
        } else {
            let mut consumed = 0u16;
            while consumed < field.width_bits {
                let position = field.bit_offset % 8 + consumed;
                let within = position % 8;
                let count = (8 - within).min(field.width_bits - consumed);
                let at = address
                    .checked_offset(i64::from(position / 8))
                    .map_err(|error| self.memory_error(error, span))?;
                // Only the field's own bits of the byte must be initialized.
                let own = ((1u128 << count) - 1) << within;
                let byte = self
                    .memory
                    .load_integer_bits(at, AccessLayout::new(1, 1), own)
                    .map_err(|error| self.memory_error(error, span))?;
                raw |= ((byte >> within) & ((1u128 << count) - 1)) << consumed;
                consumed += count;
            }
        }
        let result = self.normalize_bit_field(
            CInteger::from_raw(raw, CIntegerType::UINT128),
            ty,
            field,
            span,
        )?;
        Ok(CValue::Integer(
            result.cast(result.ty().bit_field_promoted(field.width_bits)),
        ))
    }

    pub(super) fn store_bit_field(
        &mut self,
        address: Pointer,
        ty: TypeId,
        field: BitFieldLayout,
        value: CValue,
        volatile: bool,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let value =
            self.normalize_bit_field(expect_machine_integer(value, span)?, ty, field, span)?;
        if volatile {
            for (offset, bytes) in field.storage_units() {
                let first = u32::from(field.bit_offset).max(offset * 8);
                let last = (u32::from(field.bit_offset) + u32::from(field.width_bits))
                    .min((offset + bytes) * 8);
                let count = last - first;
                let within = first - offset * 8;
                let at = address
                    .checked_offset(i64::from(offset) - i64::from(field.bit_offset / 8))
                    .map_err(|error| self.memory_error(error, span))?;
                let layout = AccessLayout::new(u64::from(bytes), 1);
                // The unit read keeps the other bits and is one volatile
                // access, but it is not a C read of the other members: it
                // demands no bit, and the store initializes only the
                // field's bits (EvoIR 10.1.6).
                let (original, world) = self
                    .memory
                    .volatile_load_integer_bits(&self.world, at, layout, 0)
                    .map_err(|error| self.memory_error(error, span))?;
                let mask = (u128::MAX >> (128 - count)) << within;
                let selected =
                    ((value.raw_bits() >> (first - u32::from(field.bit_offset))) << within) & mask;
                let stored = (original & !mask) | selected;
                let (memory, world) = self
                    .memory
                    .volatile_store_integer_bits(&world, at, layout, stored, mask)
                    .map_err(|error| self.memory_error(error, span))?;
                self.memory = memory;
                self.world = world;
            }
            return Ok(CValue::Integer(value));
        }
        let mut consumed = 0u16;
        while consumed < field.width_bits {
            let position = field.bit_offset % 8 + consumed;
            let within = position % 8;
            let count = (8 - within).min(field.width_bits - consumed);
            let at = address
                .checked_offset(i64::from(position / 8))
                .map_err(|error| self.memory_error(error, span))?;
            let mask = ((1u128 << count) - 1) << within;
            // The store keeps the other bits of a partial byte. It reads none
            // of them, and it initializes only the field's own bits.
            let original = if count == 8 {
                0
            } else {
                self.memory
                    .load_integer_bits(at, AccessLayout::new(1, 1), 0)
                    .map_err(|error| self.memory_error(error, span))?
            };
            let byte = (original & !mask) | (((value.raw_bits() >> consumed) << within) & mask);
            self.memory = self
                .memory
                .store_integer_bits(at, AccessLayout::new(1, 1), byte, mask)
                .map_err(|error| self.memory_error(error, span))?;
            consumed += count;
        }
        Ok(CValue::Integer(value))
    }
}
