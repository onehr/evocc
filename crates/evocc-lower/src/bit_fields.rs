//! Lowering bit fields.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn place_bit_field(
        &self,
        place: &MemoryPlace<'_>,
        span: SourceSpan,
    ) -> Result<Option<BitFieldLayout>, LowerError> {
        if !place.is_bit_field() {
            return Ok(None);
        }
        let Some(PlaceProjection::Field(field)) = place.projections().last() else {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "bit-field place has no final field",
            });
        };
        let layout = self
            .target
            .record_layout(self.unit.types(), field.tag())
            .map_err(|source| LowerError::Layout {
                ty: place.value_ty(),
                span,
                source,
            })?;
        layout
            .fields
            .iter()
            .find(|entry| entry.field_index == field.field_index())
            .and_then(|entry| entry.bit_field)
            .map(Some)
            .ok_or(LowerError::UnsupportedExpression {
                span,
                feature: "bit-field place has no target layout",
            })
    }

    /// One byte access. A `mask` names the bits that a load requires to be
    /// initialized, or the bits that a store initializes (EvoIR 3.8).
    pub(super) fn emit_byte_access(
        &mut self,
        address: Value,
        stored: Option<Value>,
        mask: Option<u128>,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        self.emit_integer_access(address, stored, CIntegerType::UCHAR, false, mask, state)
    }

    pub(super) fn emit_integer_access(
        &mut self,
        address: Value,
        stored: Option<Value>,
        integer: CIntegerType,
        volatile: bool,
        mask: Option<u128>,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let guarded = self.fresh_value(EvoIRType::Mem(Region::All));
        self.emit(
            state,
            Inst::AccessInBounds {
                mem_in,
                pointer: address.clone(),
                mem_out: guarded.clone(),
                layout: AccessLayout {
                    size_bytes: u64::from(integer.bits() / 8),
                    align_bytes: 1,
                },
            },
        )?;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        let result = if let Some(value) = stored {
            self.emit(
                state,
                Inst::Store {
                    address,
                    value: value.clone(),
                    mem_in: guarded,
                    world_in,
                    mem_out: mem_out.clone(),
                    world_out: world_out.clone(),
                    volatile,
                    mask,
                },
            )?;
            value
        } else {
            let result = self.fresh_value(bits(integer));
            self.emit(
                state,
                Inst::Load {
                    result: result.clone(),
                    address,
                    mem_in: guarded,
                    world_in,
                    mem_out: mem_out.clone(),
                    world_out: world_out.clone(),
                    volatile,
                    mask,
                },
            )?;
            result
        };
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(result)
    }

    pub(super) fn normalize_bit_field(
        &mut self,
        value: ExprValue,
        field: BitFieldLayout,
        state: &State,
    ) -> Result<ExprValue, LowerError> {
        let integer = value.integer;
        let width = u16::from(integer.bits());
        if field.width_bits == width {
            return Ok(value);
        }
        let mask = self
            .wide_constant(state, integer, (1u128 << field.width_bits) - 1)?
            .value;
        let masked = self.fresh_value(bits(integer));
        self.emit(state, Inst::And(masked.clone(), value.value, mask))?;
        let result = if integer.is_signed() {
            let shift = self
                .constant(
                    state,
                    CIntegerType::UINT,
                    u64::from(width - field.width_bits),
                )?
                .value;
            let high = self.fresh_value(bits(integer));
            let signed = self.fresh_value(bits(integer));
            self.emit(state, Inst::Shl(high.clone(), masked, shift.clone()))?;
            self.emit(state, Inst::AShr(signed.clone(), high, shift))?;
            signed
        } else {
            masked
        };
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    pub(super) fn emit_bit_field_load(
        &mut self,
        address: Value,
        ty: TypeId,
        field: BitFieldLayout,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let integer = canonical_integer(self.unit.types(), self.target, ty, span)?;
        if volatile {
            return self.emit_volatile_bit_field_load(address, integer, field, state);
        }
        let mut result = self.constant(state, integer, 0)?.value;
        let mut consumed = 0u16;
        while consumed < field.width_bits {
            let position = field.bit_offset % 8 + consumed;
            let within = position % 8;
            let count = (8 - within).min(field.width_bits - consumed);
            let byte_address =
                self.constant_offset_address(address.clone(), u64::from(position / 8), state)?;
            // The read demands only the field's own bits of this byte.
            let own = (count < 8).then(|| ((1u128 << count) - 1) << within);
            let byte = self.emit_byte_access(byte_address, None, own, state)?;
            let shift = self
                .constant(state, CIntegerType::UINT, u64::from(within))?
                .value;
            let shifted = self.fresh_value(EvoIRType::Bits(8));
            self.emit(state, Inst::LShr(shifted.clone(), byte, shift))?;
            let mask = self
                .constant(state, CIntegerType::UCHAR, (1u64 << count) - 1)?
                .value;
            let selected = self.fresh_value(EvoIRType::Bits(8));
            self.emit(state, Inst::And(selected.clone(), shifted, mask))?;
            let selected = self
                .cast(
                    ExprValue {
                        value: selected,
                        integer: CIntegerType::UCHAR,
                    },
                    integer,
                    state,
                )?
                .value;
            let shift = self
                .constant(state, CIntegerType::UINT, u64::from(consumed))?
                .value;
            let shifted = self.fresh_value(bits(integer));
            self.emit(state, Inst::Shl(shifted.clone(), selected, shift))?;
            let joined = self.fresh_value(bits(integer));
            self.emit(state, Inst::Or(joined.clone(), result, shifted))?;
            result = joined;
            consumed += count;
        }
        let result = self.normalize_bit_field(
            ExprValue {
                value: result,
                integer,
            },
            field,
            state,
        )?;
        self.cast(result, integer.bit_field_promoted(field.width_bits), state)
    }

    pub(super) fn emit_bit_field_store(
        &mut self,
        address: Value,
        value: ExprValue,
        field: BitFieldLayout,
        volatile: bool,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let value = self.normalize_bit_field(value, field, state)?;
        if volatile {
            return self.emit_volatile_bit_field_store(address, value, field, state);
        }
        let mut consumed = 0u16;
        while consumed < field.width_bits {
            let position = field.bit_offset % 8 + consumed;
            let within = position % 8;
            let count = (8 - within).min(field.width_bits - consumed);
            let byte_address =
                self.constant_offset_address(address.clone(), u64::from(position / 8), state)?;
            let shift = self
                .constant(state, CIntegerType::UINT, u64::from(consumed))?
                .value;
            let shifted = self.fresh_value(value.value.ty.clone());
            self.emit(
                state,
                Inst::LShr(shifted.clone(), value.value.clone(), shift),
            )?;
            let low = self
                .cast(
                    ExprValue {
                        value: shifted,
                        integer: value.integer,
                    },
                    CIntegerType::UCHAR,
                    state,
                )?
                .value;
            let mask_bits = ((1u64 << count) - 1) << within;
            let mask = self.constant(state, CIntegerType::UCHAR, mask_bits)?.value;
            let shift = self
                .constant(state, CIntegerType::UINT, u64::from(within))?
                .value;
            let shifted = self.fresh_value(EvoIRType::Bits(8));
            self.emit(state, Inst::Shl(shifted.clone(), low, shift))?;
            let selected = self.fresh_value(EvoIRType::Bits(8));
            self.emit(state, Inst::And(selected.clone(), shifted, mask))?;
            // A partial byte keeps the other bits: the load reads none of
            // them, and the store initializes only the field's bits.
            let (stored, written) = if count == 8 {
                (selected, None)
            } else {
                let original = self.emit_byte_access(byte_address.clone(), None, Some(0), state)?;
                let keep = self
                    .constant(state, CIntegerType::UCHAR, !mask_bits & 255)?
                    .value;
                let kept = self.fresh_value(EvoIRType::Bits(8));
                self.emit(state, Inst::And(kept.clone(), original, keep))?;
                let merged = self.fresh_value(EvoIRType::Bits(8));
                self.emit(state, Inst::Or(merged.clone(), kept, selected))?;
                (merged, Some(u128::from(mask_bits)))
            };
            self.emit_byte_access(byte_address, Some(stored), written, state)?;
            consumed += count;
        }
        Ok(value)
    }

    pub(super) fn bit_field_unit_address(
        &mut self,
        address: Value,
        field: BitFieldLayout,
        offset: u32,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let delta = i64::from(offset) - i64::from(field.bit_offset / 8);
        if delta == 0 {
            return Ok(address);
        }
        let index = self.constant(state, CIntegerType::LONG, delta as u64)?;
        self.offset_address(
            address,
            index,
            OffsetDirection::Add,
            IndexSignedness::Signed,
            1,
            state,
        )
    }

    pub(super) fn emit_volatile_bit_field_load(
        &mut self,
        address: Value,
        integer: CIntegerType,
        field: BitFieldLayout,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let mut result = self.constant(state, integer, 0)?.value;
        for (offset, bytes) in field.storage_units() {
            let unit_integer = bit_field_unit_integer(bytes);
            let first = u32::from(field.bit_offset).max(offset * 8);
            let last = (u32::from(field.bit_offset) + u32::from(field.width_bits))
                .min((offset + bytes) * 8);
            let count = last - first;
            let at = self.bit_field_unit_address(address.clone(), field, offset, state)?;
            // One volatile access of the unit that demands only the field's
            // bits of it (D25).
            let own = unit_mask(
                unit_integer,
                (u128::MAX >> (128 - count)) << (first - offset * 8),
            );
            let raw = self.emit_integer_access(at, None, unit_integer, true, own, state)?;
            let shift = self
                .constant(state, CIntegerType::UINT, u64::from(first - offset * 8))?
                .value;
            let selected = self.emit_raw_binary(state, Inst::LShr, &raw, &shift)?;
            let mask = self
                .wide_constant(state, unit_integer, u128::MAX >> (128 - count))?
                .value;
            let selected = self.emit_raw_binary(state, Inst::And, &selected, &mask)?;
            let selected = self
                .cast(
                    ExprValue {
                        value: selected,
                        integer: unit_integer,
                    },
                    integer,
                    state,
                )?
                .value;
            let shift = self
                .constant(
                    state,
                    CIntegerType::UINT,
                    u64::from(first - u32::from(field.bit_offset)),
                )?
                .value;
            let positioned = self.emit_raw_binary(state, Inst::Shl, &selected, &shift)?;
            result = self.emit_raw_binary(state, Inst::Or, &result, &positioned)?;
        }
        let value = self.normalize_bit_field(
            ExprValue {
                value: result,
                integer,
            },
            field,
            state,
        )?;
        self.cast(value, integer.bit_field_promoted(field.width_bits), state)
    }

    pub(super) fn emit_volatile_bit_field_store(
        &mut self,
        address: Value,
        value: ExprValue,
        field: BitFieldLayout,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        for (offset, bytes) in field.storage_units() {
            let unit_integer = bit_field_unit_integer(bytes);
            let first = u32::from(field.bit_offset).max(offset * 8);
            let last = (u32::from(field.bit_offset) + u32::from(field.width_bits))
                .min((offset + bytes) * 8);
            let count = last - first;
            let within = first - offset * 8;
            let shift = self
                .constant(
                    state,
                    CIntegerType::UINT,
                    u64::from(first - u32::from(field.bit_offset)),
                )?
                .value;
            let selected = self.emit_raw_binary(state, Inst::LShr, &value.value, &shift)?;
            let selected = self
                .cast(
                    ExprValue {
                        value: selected,
                        integer: value.integer,
                    },
                    unit_integer,
                    state,
                )?
                .value;
            let shift = self
                .constant(state, CIntegerType::UINT, u64::from(within))?
                .value;
            let positioned = self.emit_raw_binary(state, Inst::Shl, &selected, &shift)?;
            let mask_bits = (u128::MAX >> (128 - count)) << within;
            let mask = self.wide_constant(state, unit_integer, mask_bits)?.value;
            let selected = self.emit_raw_binary(state, Inst::And, &positioned, &mask)?;
            let at = self.bit_field_unit_address(address.clone(), field, offset, state)?;
            // The unit read keeps the other bits but is no C read of them: it
            // demands no bit, and the store initializes only the field's
            // bits (D25).
            let original =
                self.emit_integer_access(at.clone(), None, unit_integer, true, Some(0), state)?;
            let keep = self.wide_constant(state, unit_integer, !mask_bits)?.value;
            let kept = self.emit_raw_binary(state, Inst::And, &original, &keep)?;
            let merged = self.emit_raw_binary(state, Inst::Or, &kept, &selected)?;
            let written = unit_mask(unit_integer, mask_bits);
            self.emit_integer_access(at, Some(merged), unit_integer, true, written, state)?;
        }
        Ok(value)
    }
}
