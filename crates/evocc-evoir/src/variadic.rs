use crate::{EvoIRType, ParamMark, RecordShape, Value};

/// Access to the abstract anonymous-argument sequence, not a machine ABI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariadicOp {
    Start,
    Arg(Value),
    ArgRecord {
        destination: Value,
        shape: RecordShape,
    },
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
            VariadicOp::Start | VariadicOp::End | VariadicOp::ArgRecord { .. } => Vec::new(),
        };
        results.extend([&self.mem_out, &self.world_out]);
        results
    }

    pub fn operands(&self) -> Vec<&Value> {
        let mut values = vec![&self.list, &self.mem_in, &self.world_in];
        if let VariadicOp::ArgRecord { destination, .. } = &self.op {
            values.push(destination);
        }
        values
    }

    pub fn validate(&self) -> Result<(), String> {
        let valid = matches!(self.list.ty, EvoIRType::Ptr(_))
            && matches!(self.mem_in.ty, EvoIRType::Mem(_))
            && self.mem_in.ty == self.mem_out.ty
            && self.world_in.ty == EvoIRType::World
            && self.world_out.ty == EvoIRType::World
            && match &self.op {
                VariadicOp::Arg(value) => promoted_type(&value.ty),
                VariadicOp::ArgRecord { destination, shape } => crate::validate_record_marks(
                    std::slice::from_ref(&destination.ty),
                    &[ParamMark::ByVal(*shape)],
                )
                .is_ok(),
                VariadicOp::Start | VariadicOp::End => true,
            };
        if valid {
            Ok(())
        } else {
            Err("invalid variadic access operand, result or effect-token type".into())
        }
    }
}

/// Fixed record marks come from the signature, anonymous marks from the call.
pub fn call_argument_marks(
    named: &[ParamMark],
    anonymous: &[ParamMark],
    count: usize,
) -> Vec<ParamMark> {
    (0..count)
        .map(|index| {
            let supplied = anonymous.get(index).copied().unwrap_or(ParamMark::None);
            if supplied == ParamMark::None {
                named.get(index).copied().unwrap_or(ParamMark::None)
            } else {
                supplied
            }
        })
        .collect()
}

pub(crate) fn validate_anonymous_marks(
    args: &[Value],
    marks: &[ParamMark],
    fixed: usize,
) -> Result<(), String> {
    crate::validate_record_marks(
        &args.iter().map(|arg| arg.ty.clone()).collect::<Vec<_>>(),
        marks,
    )?;
    if marks
        .iter()
        .enumerate()
        .any(|(index, mark)| mark.is_sret() || (index < fixed && *mark != ParamMark::None))
    {
        return Err("call-site byval marks apply only to anonymous arguments".into());
    }
    Ok(())
}

pub(crate) fn promoted_type(ty: &EvoIRType) -> bool {
    crate::integer_vector_shape(ty).is_some()
        || matches!(
            ty,
            EvoIRType::Bits(32 | 64 | 128)
                | EvoIRType::F64
                | EvoIRType::Ptr(_)
                | EvoIRType::FnPtr { .. }
        )
}
