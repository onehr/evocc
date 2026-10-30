//! Running a lowered function on the inputs the EvoIR interpreter was given.
//!
//! Section 9.1's first instrument compares two interpreters on the same
//! program, so the argument and result shapes have to cross the boundary. This
//! module is the only place that translates between an EvoIR `RuntimeValue`
//! and an EvoMIR register, and it is shared by the hand-written fixtures and
//! by the semantic gate's third lane so the two cannot drift.

use evocc_evoir::RuntimeValue;
use evocc_memory::{MemoryState, WorldState};
use evocc_types::EvoIRType;

use crate::{MModule, MirError, MirLimits, MirValue, Target};

/// One defined EvoMIR execution, shaped so the Section 7 equality criterion
/// can be applied to it directly.
#[derive(Debug, Clone)]
pub struct MirRun {
    pub returns: Vec<RuntimeValue>,
    pub memory: MemoryState,
    pub world: WorldState,
    pub steps: u64,
}

/// Whether a type occupies a machine register at all.
///
/// Section 5: "EvoIR's `mem` and `world` tokens are consumed here, and the
/// order they imposed becomes program order." A consumed token is not an
/// argument, a result, or a register.
#[must_use]
pub fn is_register_type(ty: &EvoIRType) -> bool {
    !matches!(ty, EvoIRType::Mem(_) | EvoIRType::World)
}

fn mask(width: u16) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

/// The bits above a narrow argument's value.
///
/// Section 6: "a `bits<N>` argument with `N < 64` is passed in the low bits of
/// its register with the upper bits unspecified, and the callee applies
/// Section 3 before reading it as anything but a low-`N`-bit value." Passing
/// zeroes there would make an unspecified bit read like a specified one and
/// would hide exactly the defect Section 9.1's comparison exists to find, so
/// the bits above `N` are filled with a pattern derived from the argument's
/// position and width: deterministic, so the lane is reproducible, and rarely
/// zero, so a lowering that reads them answers differently from EvoIR.
fn unspecified_fill(index: usize, bits: u16) -> u64 {
    let register: u16 = if bits <= 32 { 32 } else { 64 };
    if bits >= register {
        return 0;
    }
    let mut mixed = (index as u64)
        .wrapping_add(1)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ u64::from(bits).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed ^= mixed >> 31;
    mixed |= 1;
    (mixed & mask(register - bits)) << bits
}

/// Execute `entry` in a lowered module on the EvoIR entry arguments.
///
/// `result_types` are the entry function's ordinary EvoIR result types, memory
/// and world excluded. They are needed because Section 6 passes and returns a
/// `bits<N>` with `N < 64` "in the low bits of its register with the upper bits
/// unspecified": the register alone does not say how much of it is the value.
///
/// The argument registers follow the Darwin profile, as `lower_module` does.
/// Use [`observe_entry_for`] for a module lowered for another target.
pub fn observe_entry(
    module: &MModule,
    entry: &str,
    entry_args: &[RuntimeValue],
    result_types: &[EvoIRType],
    limits: MirLimits,
) -> Result<MirRun, MirError> {
    observe_entry_for(
        Target::Darwin,
        module,
        entry,
        entry_args,
        result_types,
        limits,
    )
}

/// The same, for a module lowered for `target`.
///
/// The target decides where a `bits<128>` argument starts. The entry block
/// reads the registers that the target's ABI table assigns, so the lane must
/// pass the arguments in the same registers.
pub fn observe_entry_for(
    target: Target,
    module: &MModule,
    entry: &str,
    entry_args: &[RuntimeValue],
    result_types: &[EvoIRType],
    limits: MirLimits,
) -> Result<MirRun, MirError> {
    let pair_rounding = target.aarch64_abi().aligned_record_pair_rounding;
    let mut arguments = Vec::new();
    for (index, value) in entry_args
        .iter()
        .filter(|value| is_register_type(&value.ty))
        .enumerate()
    {
        match value.ty {
            // Section 2: "lowering represents it as two `gpr64` virtual
            // registers, the low half first", so the boundary passes two.
            // Section 6.5 rounds the position up to even before them where
            // the ABI table rounds a 16-byte-aligned argument: Linux does,
            // Darwin does not.
            EvoIRType::Bits(128) => {
                if pair_rounding && arguments.len() % 2 == 1 {
                    arguments.push(MirValue::Int(0));
                }
                arguments.push(MirValue::Int(value.raw_bits));
                arguments.push(MirValue::Int(value.high_bits));
            }
            EvoIRType::Bits(width) => arguments.push(MirValue::Int(
                (value.raw_bits & mask(width)) | unspecified_fill(index, width),
            )),
            ref other => {
                return Err(MirError::Invalid {
                    detail: format!("no register spelling for an entry argument of type {other:?}"),
                })
            }
        }
    }
    let outcome = crate::interp::interpret_module_with_target(
        module,
        entry,
        &arguments,
        &target.platform().layout(),
        limits,
    )?;

    // Section 6 returns up to eight results in x0 to x7, in order, each a
    // low-N-bit value in a register whose upper bits are unspecified.
    let expected: usize = result_types
        .iter()
        .map(|ty| usize::from(matches!(ty, EvoIRType::Bits(128))) + 1)
        .sum();
    if outcome.results.len() != expected {
        return Err(MirError::Invalid {
            detail: format!(
                "the entry returned {} registers where {expected} were declared",
                outcome.results.len()
            ),
        });
    }
    let mut returns = Vec::new();
    let mut registers = outcome.results.iter();
    for ty in result_types {
        let EvoIRType::Bits(width) = ty else {
            return Err(MirError::Invalid {
                detail: format!("no register spelling for a result of type {ty:?}"),
            });
        };
        let mut next = || match registers.next() {
            Some(MirValue::Int(raw)) => Ok(*raw),
            other => Err(MirError::Invalid {
                detail: format!("an integer result came back as {other:?}"),
            }),
        };
        if *width == 128 {
            // Section 6: "A `bits<128>` result is `x0`:`x1`, low half in `x0`."
            let low = next()?;
            let high = next()?;
            returns.push(RuntimeValue {
                ty: ty.clone(),
                raw_bits: low,
                high_bits: high,
            });
        } else {
            let raw = next()?;
            returns.push(RuntimeValue {
                ty: ty.clone(),
                raw_bits: raw & mask(*width),
                high_bits: 0,
            });
        }
    }

    Ok(MirRun {
        returns,
        memory: outcome.memory,
        world: outcome.world,
        steps: outcome.steps,
    })
}
