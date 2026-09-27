//! Strict arithmetic is a world transformation, not a memory operation.
use crate::*;
use evocc_types::floating::{FiniteOp, FloatBits, FpResult, FpRounding};
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictFloatingInst {
    pub instruction: FloatingInst,
    pub world_in: Value,
    pub world_out: Value,
}

impl StrictFloatingInst {
    pub fn operands(&self) -> Vec<&Value> {
        std::iter::once(&self.world_in)
            .chain(self.instruction.arguments.iter())
            .collect()
    }
    pub fn values_mut(&mut self) -> Vec<&mut Value> {
        let mut values = self.instruction.values_mut();
        values.extend([&mut self.world_in, &mut self.world_out]);
        values
    }
    pub fn validate(&self) -> Result<(), String> {
        self.instruction.validate()?;
        if self.world_in.ty != EvoIRType::World || self.world_out.ty != EvoIRType::World {
            return Err("strict floating operation requires world input/output".into());
        }
        use FloatingOp::*;
        if !matches!(
            self.instruction.op,
            Add | Sub
                | Mul
                | Div
                | Fma
                | Oeq
                | Une
                | Olt
                | Ole
                | Ogt
                | Oge
                | QuietOlt
                | QuietOle
                | QuietOgt
                | QuietOge
                | Convert
                | FromSigned
                | FromUnsigned
                | ToSigned
                | ToUnsigned
        ) {
            return Err("representation-only operation has no strict form".into());
        }
        Ok(())
    }
}

impl FloatingInst {
    /// Invoked only after strict-op validation. Source comparisons distinguish
    /// quiet equality from signaling ordered comparison; raw C conversion
    /// guards remain separate pure operations and preserve E3063.
    pub fn evaluate_strict(&self, inputs: &[u128], rounding: FpRounding) -> FpResult<u128> {
        use FloatingOp::*;
        let float = |i: usize| {
            FloatBits::from_bits(floating_kind(&self.arguments[i].ty).unwrap(), inputs[i]).unwrap()
        };
        let target = || floating_kind(&self.result.ty).unwrap();
        let pack = |value: FpResult<FloatBits>| FpResult {
            value: value.value.bits(),
            status: value.status,
        };
        match self.op {
            Add | Sub | Mul | Div => pack(
                float(0)
                    .target_binary_round(
                        float(1),
                        match self.op {
                            Add => FiniteOp::Add,
                            Sub => FiniteOp::Sub,
                            Mul => FiniteOp::Mul,
                            _ => FiniteOp::Div,
                        },
                        rounding,
                    )
                    .unwrap(),
            ),
            Fma => pack(
                float(0)
                    .target_fma_round(float(1), float(2), rounding)
                    .unwrap(),
            ),
            Convert => pack(float(0).target_convert_round(target(), rounding).unwrap()),
            FromSigned => {
                let EvoIRType::Bits(width) = self.arguments[0].ty else {
                    unreachable!()
                };
                pack(FloatBits::from_signed_round(
                    target(),
                    ((inputs[0] << (128 - width)) as i128) >> (128 - width),
                    rounding,
                ))
            }
            FromUnsigned => pack(FloatBits::from_unsigned_round(
                target(),
                inputs[0],
                rounding,
            )),
            ToSigned | ToUnsigned => {
                let EvoIRType::Bits(width) = self.result.ty else {
                    unreachable!()
                };
                let value = float(0)
                    .to_integer_status(width, self.op == ToSigned)
                    .unwrap();
                FpResult {
                    value: value.value.unwrap_or(0) & (u128::MAX >> (128 - width)),
                    status: value.status,
                }
            }
            Oeq | Une | Olt | Ole | Ogt | Oge | QuietOlt | QuietOle | QuietOgt | QuietOge => {
                let result = float(0)
                    .target_compare(float(1), matches!(self.op, Olt | Ole | Ogt | Oge))
                    .unwrap();
                let yes = match self.op {
                    Oeq => result.value == Some(Ordering::Equal),
                    Une => result.value != Some(Ordering::Equal),
                    Olt | QuietOlt => result.value == Some(Ordering::Less),
                    Ole | QuietOle => {
                        matches!(result.value, Some(Ordering::Less | Ordering::Equal))
                    }
                    Ogt | QuietOgt => result.value == Some(Ordering::Greater),
                    Oge | QuietOge => {
                        matches!(result.value, Some(Ordering::Greater | Ordering::Equal))
                    }
                    _ => unreachable!(),
                };
                FpResult {
                    value: u128::from(yes),
                    status: result.status,
                }
            }
            _ => unreachable!("validated strict floating operation"),
        }
    }
}

pub(super) fn execute(
    operation: &StrictFloatingInst,
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    let _ = read_value(values, &operation.world_in, block)?;
    let inputs = operation
        .instruction
        .arguments
        .iter()
        .map(|value| read_value(values, value, block))
        .collect::<Result<Vec<_>, _>>()?;
    let environment = state.world.fp_environment();
    let result = operation
        .instruction
        .evaluate_strict(&inputs, environment.rounding);
    state.world = state
        .world
        .with_fp_environment(environment.raised(result.status));
    let token = state.fresh_token()?;
    write_value(values, &operation.instruction.result, result.value);
    write_value(values, &operation.world_out, token);
    Ok(())
}
