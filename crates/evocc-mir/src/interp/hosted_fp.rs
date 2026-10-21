//! Executable models of qualified numeric helper calls; all state is explicit.
use super::*;
use evocc_types::{floating::FpRounding, EvoIRType};

pub(super) fn execute(
    symbol: &str,
    args: &[MirValue],
    world: &mut WorldState,
    fp_mode: Option<crate::FpCallMode>,
) -> Result<Option<Vec<MirValue>>, MirError> {
    let Some(helper) = crate::floating::helpers::qualified(symbol) else {
        return Ok(None);
    };
    // Ordinary runtime calls use and update the actual environment. Only an
    // explicitly marked fixed-profile compiler helper has pure-IR semantics.
    let strict = fp_mode != Some(crate::FpCallMode::Fixed);
    if !strict {
        world
            .check_fixed_fp_environment()
            .map_err(|e| memory_error(&e))?;
    }
    let invalid = || MirError::Invalid {
        detail: format!("invalid @{symbol} arguments"),
    };
    let mut remaining = args;
    let mut values = Vec::new();
    for ty in helper.argument_types() {
        let bits = match (&ty, remaining) {
            (EvoIRType::Bits(128), [MirValue::Int(lo), MirValue::Int(hi), rest @ ..]) => {
                remaining = rest;
                u128::from(*lo) | (u128::from(*hi) << 64)
            }
            (EvoIRType::Bits(width), [MirValue::Int(value), rest @ ..]) => {
                remaining = rest;
                u128::from(*value) & (u128::MAX >> (128 - width))
            }
            (_, [MirValue::Vector(value), rest @ ..]) => {
                remaining = rest;
                let kind = evocc_evoir::floating_kind(&ty).ok_or_else(invalid)?;
                let bytes = match kind {
                    evocc_types::FloatKind::F32 => 4,
                    evocc_types::FloatKind::F64 => 8,
                    _ => 16,
                };
                value
                    .read_integer(0, bytes, evocc_memory::ByteOrder::Little)
                    .ok_or_else(invalid)?
            }
            _ => return Err(invalid()),
        };
        values.push(bits);
    }
    if !remaining.is_empty() {
        return Err(invalid());
    }
    if !helper.inputs_defined(&values) {
        return Err(invalid());
    }
    let environment = world.fp_environment();
    let rounding = if strict {
        environment.rounding
    } else {
        FpRounding::NearestEven
    };
    if helper.is_complex() {
        // Both parts return in two SIMD registers of the part format.
        let result = helper.evaluate_parts(&values, rounding);
        if strict {
            *world = world.with_fp_environment(environment.raised(result.status));
        }
        let bytes = match helper.result_type() {
            EvoIRType::F32 => 4,
            EvoIRType::F64 => 8,
            _ => 16,
        };
        let mut outputs = Vec::with_capacity(2);
        for part in result.value {
            let mut repr = evocc_memory::ObjectRepresentation::uninitialized(bytes)
                .map_err(|e| memory_error(&e))?;
            repr.write_integer(0, bytes, Some(part), evocc_memory::ByteOrder::Little)
                .map_err(|e| memory_error(&e))?;
            outputs.push(MirValue::Vector(repr));
        }
        return Ok(Some(outputs));
    }
    let result = helper.evaluate(&values, rounding);
    if strict {
        *world = world.with_fp_environment(environment.raised(result.status));
    }
    let outputs = match helper.result_type() {
        EvoIRType::Bits(128) => vec![
            MirValue::Int(result.value as u64),
            MirValue::Int((result.value >> 64) as u64),
        ],
        EvoIRType::Bits(_) => vec![MirValue::Int(result.value as u64)],
        ty => {
            let bytes = match ty {
                EvoIRType::F32 => 4,
                EvoIRType::F64 => 8,
                _ => 16,
            };
            let mut repr = evocc_memory::ObjectRepresentation::uninitialized(bytes)
                .map_err(|e| memory_error(&e))?;
            repr.write_integer(
                0,
                bytes,
                Some(result.value),
                evocc_memory::ByteOrder::Little,
            )
            .map_err(|e| memory_error(&e))?;
            vec![MirValue::Vector(repr)]
        }
    };
    Ok(Some(outputs))
}
