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

use crate::{interpret_module, MModule, MirError, MirLimits, MirValue};

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

/// Execute `entry` in a lowered module on the EvoIR entry arguments.
///
/// `result_types` are the entry function's ordinary EvoIR result types, memory
/// and world excluded. They are needed because Section 6 passes and returns a
/// `bits<N>` with `N < 64` "in the low bits of its register with the upper bits
/// unspecified": the register alone does not say how much of it is the value.
pub fn observe_entry(
    module: &MModule,
    entry: &str,
    entry_args: &[RuntimeValue],
    result_types: &[EvoIRType],
    limits: MirLimits,
) -> Result<MirRun, MirError> {
    let mut arguments = Vec::new();
    for value in entry_args
        .iter()
        .filter(|value| is_register_type(&value.ty))
    {
        match value.ty {
            EvoIRType::Bits(_) => arguments.push(MirValue::Int(value.raw_bits)),
            ref other => {
                return Err(MirError::Invalid {
                    detail: format!("no register spelling for an entry argument of type {other:?}"),
                })
            }
        }
    }
    let outcome = interpret_module(module, entry, &arguments, limits)?;

    let mut returns = Vec::new();
    match (result_types.len(), outcome.result) {
        (0, _) => {}
        (1, Some(MirValue::Int(raw))) => {
            let ty = result_types[0].clone();
            let raw_bits = match ty {
                EvoIRType::Bits(width) => raw & mask(width),
                _ => {
                    return Err(MirError::Invalid {
                        detail: "no register spelling for this result type".to_owned(),
                    })
                }
            };
            returns.push(RuntimeValue { ty, raw_bits });
        }
        (1, other) => {
            return Err(MirError::Invalid {
                detail: format!("the entry returned {other:?} where one integer was declared"),
            })
        }
        (count, _) => {
            return Err(MirError::Invalid {
                detail: format!("{count} ordinary results are not lowered"),
            })
        }
    }

    Ok(MirRun {
        returns,
        memory: outcome.memory,
        world: outcome.world,
        steps: outcome.steps,
    })
}
