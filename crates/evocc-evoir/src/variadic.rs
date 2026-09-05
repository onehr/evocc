use crate::{EvoIRType, Value};

/// Access to the abstract anonymous-argument sequence, not a machine ABI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariadicOp {
    Start,
    Arg(Value),
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariadicAccess {
    pub op: VariadicOp,
    pub list: Value,
    pub mem_in: Value,
    pub world_in: Value,
    pub mem_out: Value,
    pub world_out: Value,
}

impl VariadicAccess {
    pub fn results(&self) -> Vec<&Value> {
        let mut results = match &self.op {
            VariadicOp::Arg(value) => vec![value],
            VariadicOp::Start | VariadicOp::End => Vec::new(),
        };
        results.extend([&self.mem_out, &self.world_out]);
        results
    }

    pub fn validate(&self) -> Result<(), String> {
        let valid = matches!(self.list.ty, EvoIRType::Ptr(_))
            && matches!(self.mem_in.ty, EvoIRType::Mem(_))
            && self.mem_in.ty == self.mem_out.ty
            && self.world_in.ty == EvoIRType::World
            && self.world_out.ty == EvoIRType::World
            && match &self.op {
                VariadicOp::Arg(value) => promoted_type(&value.ty),
                VariadicOp::Start | VariadicOp::End => true,
            };
        if valid {
            Ok(())
        } else {
            Err("invalid variadic access operand, result or effect-token type".into())
        }
    }
}

pub(crate) fn promoted_type(ty: &EvoIRType) -> bool {
    matches!(
        ty,
        EvoIRType::Bits(32 | 64 | 128)
            | EvoIRType::F64
            | EvoIRType::Ptr(_)
            | EvoIRType::FnPtr { .. }
    )
}
