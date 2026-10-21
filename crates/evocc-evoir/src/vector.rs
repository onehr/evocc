//! Representation operations for integer and floating vectors. Arithmetic
//! is lowered using the same checked scalar instructions, independently for
//! every lane.
use crate::*;
use evocc_memory::ObjectRepresentation;
use evocc_types::ByteOrder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorOp {
    Pack,
    Extract,
    Bitcast,
    Aes(evocc_types::neon::AesOperation),
}

impl VectorOp {
    pub fn mnemonic(self) -> &'static str {
        match self {
            Self::Pack => "pack",
            Self::Extract => "extract",
            Self::Bitcast => "bitcast",
            Self::Aes(op) => op.mnemonic(),
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "pack" => Self::Pack,
            "extract" => Self::Extract,
            "bitcast" => Self::Bitcast,
            other => Self::Aes(evocc_types::neon::AesOperation::parse(other)?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorInst {
    pub result: Value,
    pub op: VectorOp,
    pub arguments: Vec<Value>,
}

pub fn integer_vector_shape(ty: &EvoIRType) -> Option<(u16, u16)> {
    let EvoIRType::Vec(lanes, element) = ty else {
        return None;
    };
    let EvoIRType::Bits(width @ (8 | 16 | 32 | 64 | 128)) = element.as_ref() else {
        return None;
    };
    (*lanes != 0 && lanes.is_power_of_two()).then_some((*lanes, *width))
}

/// The lane count and lane format of a vector of `f32` or `f64` lanes
/// (Section 3.8). The lane count is a positive power of two.
pub fn floating_vector_shape(ty: &EvoIRType) -> Option<(u16, evocc_types::FloatKind)> {
    let EvoIRType::Vec(lanes, element) = ty else {
        return None;
    };
    let kind = match element.as_ref() {
        EvoIRType::F32 => evocc_types::FloatKind::F32,
        EvoIRType::F64 => evocc_types::FloatKind::F64,
        _ => return None,
    };
    (*lanes != 0 && lanes.is_power_of_two()).then_some((*lanes, kind))
}

/// The lane count and lane width in bits of an integer or floating vector:
/// the shape that representation operations and the machine use. The lane
/// type itself is the vector's element type.
pub fn vector_shape(ty: &EvoIRType) -> Option<(u16, u16)> {
    integer_vector_shape(ty).or_else(|| {
        floating_vector_shape(ty).map(|(lanes, kind)| {
            (
                lanes,
                if kind == evocc_types::FloatKind::F32 {
                    32
                } else {
                    64
                },
            )
        })
    })
}

fn lane_type(ty: &EvoIRType) -> Option<&EvoIRType> {
    vector_shape(ty)?;
    match ty {
        EvoIRType::Vec(_, element) => Some(element),
        _ => None,
    }
}

fn representation_size(ty: &EvoIRType) -> Option<usize> {
    if let Some((lanes, width)) = vector_shape(ty) {
        return Some(usize::from(lanes) * usize::from(width / 8));
    }
    match ty {
        EvoIRType::Bits(width @ (8 | 16 | 32 | 64 | 128)) => Some(usize::from(*width / 8)),
        _ => None,
    }
}

impl VectorInst {
    pub fn values_mut(&mut self) -> Vec<&mut Value> {
        std::iter::once(&mut self.result)
            .chain(self.arguments.iter_mut())
            .collect()
    }
    pub fn validate(&self) -> Result<(), String> {
        let invalid = || "invalid vector operation".to_owned();
        match self.op {
            VectorOp::Aes(op) => {
                if integer_vector_shape(&self.result.ty) != Some((16, 8))
                    || self.arguments.len() != op.arity()
                    || self.arguments.iter().any(|v| v.ty != self.result.ty)
                {
                    return Err(invalid());
                }
            }
            VectorOp::Pack => {
                let (lanes, _) = vector_shape(&self.result.ty).ok_or_else(invalid)?;
                let lane = lane_type(&self.result.ty).ok_or_else(invalid)?;
                if self.arguments.len() != usize::from(lanes)
                    || self.arguments.iter().any(|value| &value.ty != lane)
                {
                    return Err(invalid());
                }
            }
            VectorOp::Extract => {
                if self.arguments.len() != 2 {
                    return Err(invalid());
                }
                let lane = lane_type(&self.arguments[0].ty).ok_or_else(invalid)?;
                if !matches!(self.arguments[1].ty, EvoIRType::Bits(1..=128)) {
                    return Err(invalid());
                }
                if &self.result.ty != lane {
                    return Err(invalid());
                }
            }
            VectorOp::Bitcast => {
                if self.arguments.len() != 1
                    || representation_size(&self.result.ty).is_none()
                    || representation_size(&self.result.ty)
                        != representation_size(&self.arguments[0].ty)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

impl ExecutionState {
    pub(crate) fn save_vector(&mut self, value: ObjectRepresentation) -> InternalValue {
        let identity = self.vectors.len();
        self.vectors.push(value);
        InternalValue::Vector(identity)
    }
    fn vector_representation(
        &self,
        value: InternalValue,
        ty: &EvoIRType,
        block: BlockId,
    ) -> Result<ObjectRepresentation, InterpreterError> {
        let fail = |error| InterpreterError::MemoryAccess { block, error };
        let size = representation_size(ty).ok_or(InterpreterError::UnsupportedInstruction {
            block,
            operation: "vector representation",
        })?;
        match value {
            InternalValue::Vector(index) => Ok(self.vectors[index].clone()),
            InternalValue::Uninitialized => ObjectRepresentation::uninitialized(size).map_err(fail),
            InternalValue::Bits(bits) => {
                let mut repr = ObjectRepresentation::uninitialized(size).map_err(fail)?;
                repr.write_integer(0, size, Some(bits), ByteOrder::Little)
                    .map_err(fail)?;
                Ok(repr)
            }
            _ => Err(InterpreterError::InvalidFunction {
                message: "non-vector representation".into(),
            }),
        }
    }
}

pub(super) fn execute(
    instruction: &VectorInst,
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    let fail = |error| InterpreterError::MemoryAccess { block, error };
    let arguments = &instruction.arguments;
    let result = match instruction.op {
        VectorOp::Aes(op) => {
            let inputs = arguments
                .iter()
                .map(|value| {
                    let repr = state.vector_representation(
                        read_internal(values, value, block)?,
                        &value.ty,
                        block,
                    )?;
                    Ok(std::array::from_fn(|i| {
                        repr.read_integer(i, 1, ByteOrder::Little).map(|b| b as u8)
                    }))
                })
                .collect::<Result<Vec<_>, InterpreterError>>()?;
            let mut result = ObjectRepresentation::uninitialized(16).map_err(fail)?;
            for (index, byte) in op.evaluate(&inputs).into_iter().enumerate() {
                result
                    .write_integer(index, 1, byte.map(u128::from), ByteOrder::Little)
                    .map_err(fail)?;
            }
            state.save_vector(result)
        }
        VectorOp::Pack => {
            let (lanes, width) = vector_shape(&instruction.result.ty).expect("validated vector");
            let bytes = usize::from(width / 8);
            let mut result =
                ObjectRepresentation::uninitialized(usize::from(lanes) * bytes).map_err(fail)?;
            for (index, value) in arguments.iter().enumerate() {
                let raw = match read_internal(values, value, block)? {
                    InternalValue::Bits(raw) => Some(raw),
                    InternalValue::Uninitialized => None,
                    _ => unreachable!("validated scalar lane"),
                };
                result
                    .write_integer(index * bytes, bytes, raw, ByteOrder::Little)
                    .map_err(fail)?;
            }
            state.save_vector(result)
        }
        VectorOp::Extract => {
            let (lanes, width) = vector_shape(&arguments[0].ty).expect("validated vector");
            let index = read_value(values, &arguments[1], block)?;
            if index >= u128::from(lanes) {
                return Err(InterpreterError::UndefinedBehavior {
                    block,
                    diagnostic: "E3011",
                    message: "vector-index-out-of-bounds",
                });
            }
            let repr = state.vector_representation(
                read_internal(values, &arguments[0], block)?,
                &arguments[0].ty,
                block,
            )?;
            let bytes = usize::from(width / 8);
            repr.read_integer(index as usize * bytes, bytes, ByteOrder::Little)
                .map_or(InternalValue::Uninitialized, InternalValue::Bits)
        }
        VectorOp::Bitcast => {
            let repr = state.vector_representation(
                read_internal(values, &arguments[0], block)?,
                &arguments[0].ty,
                block,
            )?;
            if vector_shape(&instruction.result.ty).is_some() {
                state.save_vector(repr)
            } else {
                repr.read_integer(0, repr.size(), ByteOrder::Little)
                    .map_or(InternalValue::Uninitialized, InternalValue::Bits)
            }
        }
    };
    write_internal(values, &instruction.result, result);
    Ok(())
}
