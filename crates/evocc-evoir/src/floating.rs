//! Exact binary32/binary64 operations, excluded from the scalar EqSat allowlist.
use crate::*;
use evocc_types::floating::{FiniteOp, FloatBits};
use evocc_types::FloatKind;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatingOp {
    Constant(u64),
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Oeq,
    Une,
    Olt,
    Ole,
    Ogt,
    Oge,
    Convert,
    FromSigned,
    FromUnsigned,
    ToSigned,
    ToUnsigned,
    ToIntDefined { width: u16, signed: bool },
    ToBits,
    FromBits,
}

impl FloatingOp {
    pub fn mnemonic(self) -> String {
        match self {
            Self::Constant(_) => "const",
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Neg => "neg",
            Self::Oeq => "oeq",
            Self::Une => "une",
            Self::Olt => "olt",
            Self::Ole => "ole",
            Self::Ogt => "ogt",
            Self::Oge => "oge",
            Self::Convert => "convert",
            Self::FromSigned => "from_signed",
            Self::FromUnsigned => "from_unsigned",
            Self::ToSigned => "to_signed",
            Self::ToUnsigned => "to_unsigned",
            Self::ToBits => "to_bits",
            Self::FromBits => "from_bits",
            Self::ToIntDefined { width, signed } => {
                return format!("to_int_defined.{}{width}", if signed { "s" } else { "u" })
            }
        }
        .into()
    }
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "add" => Self::Add,
            "sub" => Self::Sub,
            "mul" => Self::Mul,
            "div" => Self::Div,
            "neg" => Self::Neg,
            "oeq" => Self::Oeq,
            "une" => Self::Une,
            "olt" => Self::Olt,
            "ole" => Self::Ole,
            "ogt" => Self::Ogt,
            "oge" => Self::Oge,
            "convert" => Self::Convert,
            "from_signed" => Self::FromSigned,
            "from_unsigned" => Self::FromUnsigned,
            "to_signed" => Self::ToSigned,
            "to_unsigned" => Self::ToUnsigned,
            "to_bits" => Self::ToBits,
            "from_bits" => Self::FromBits,
            _ => {
                let rest = name.strip_prefix("to_int_defined.")?;
                let (signed, width) = if let Some(n) = rest.strip_prefix('s') {
                    (true, n)
                } else {
                    (false, rest.strip_prefix('u')?)
                };
                Self::ToIntDefined {
                    width: width.parse().ok()?,
                    signed,
                }
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatingInst {
    pub result: Value,
    pub op: FloatingOp,
    pub arguments: Vec<Value>,
}

pub fn floating_kind(ty: &EvoIRType) -> Option<FloatKind> {
    match ty {
        EvoIRType::F32 => Some(FloatKind::F32),
        EvoIRType::F64 => Some(FloatKind::F64),
        _ => None,
    }
}

impl FloatingInst {
    pub fn values_mut(&mut self) -> Vec<&mut Value> {
        std::iter::once(&mut self.result)
            .chain(self.arguments.iter_mut())
            .collect()
    }
    pub fn validate(&self) -> Result<(), String> {
        use FloatingOp::*;
        let result = &self.result.ty;
        let args = &self.arguments;
        let fp_result = floating_kind(result).is_some();
        let unary = args.len() == 1;
        let fp_unary = unary && floating_kind(&args[0].ty).is_some();
        let fp_binary =
            args.len() == 2 && floating_kind(&args[0].ty).is_some() && args[0].ty == args[1].ty;
        let integer = |ty: &EvoIRType| matches!(ty, EvoIRType::Bits(1..=128));
        let bits = |ty: &EvoIRType| match ty {
            EvoIRType::F32 => EvoIRType::Bits(32),
            _ => EvoIRType::Bits(64),
        };
        let valid = match self.op {
            Constant(raw) => {
                fp_result
                    && args.is_empty()
                    && (*result != EvoIRType::F32 || raw <= u64::from(u32::MAX))
            }
            Add | Sub | Mul | Div => fp_binary && *result == args[0].ty,
            Neg => fp_unary && *result == args[0].ty,
            Oeq | Une | Olt | Ole | Ogt | Oge => fp_binary && *result == EvoIRType::Bits(1),
            Convert => fp_unary && fp_result,
            FromSigned | FromUnsigned => unary && integer(&args[0].ty) && fp_result,
            ToSigned | ToUnsigned => fp_unary && integer(result),
            ToIntDefined { width, .. } => {
                fp_unary && (1..=128).contains(&width) && *result == EvoIRType::Bits(1)
            }
            ToBits => fp_unary && *result == bits(&args[0].ty),
            FromBits => unary && fp_result && args[0].ty == bits(result),
        };
        if valid {
            Ok(())
        } else {
            Err("invalid floating operation type, width or arity".into())
        }
    }

    /// Semantic kernel shared by interpreters; inputs/results are exact bits.
    /// Invalid raw conversions are total (zero); C must emit the distinct UB
    /// predicate and assumption. This fallback never authorizes a C conversion.
    pub fn evaluate(&self, inputs: &[u128]) -> u128 {
        use FloatingOp::*;
        if let Constant(bits) = self.op {
            return u128::from(bits);
        }
        let float = |i: usize| {
            FloatBits::from_bits(floating_kind(&self.arguments[i].ty).unwrap(), inputs[i]).unwrap()
        };
        let target = || floating_kind(&self.result.ty).unwrap();
        match self.op {
            Add | Sub | Mul | Div => float(0)
                .target_binary(
                    float(1),
                    match self.op {
                        Add => FiniteOp::Add,
                        Sub => FiniteOp::Sub,
                        Mul => FiniteOp::Mul,
                        _ => FiniteOp::Div,
                    },
                )
                .unwrap()
                .bits(),
            Neg => float(0).negated().bits(),
            Convert => float(0).target_convert(target()).unwrap().bits(),
            FromSigned => {
                let EvoIRType::Bits(width) = self.arguments[0].ty else {
                    unreachable!()
                };
                let value = ((inputs[0] << (128 - width)) as i128) >> (128 - width);
                FloatBits::from_signed(target(), value).bits()
            }
            FromUnsigned => FloatBits::from_unsigned(target(), inputs[0]).bits(),
            ToSigned | ToUnsigned => {
                let EvoIRType::Bits(width) = self.result.ty else {
                    unreachable!()
                };
                float(0).to_integer(width, self.op == ToSigned).unwrap_or(0)
                    & (u128::MAX >> (128 - width))
            }
            ToIntDefined { width, signed } => {
                u128::from(float(0).to_integer(width, signed).is_some())
            }
            ToBits | FromBits => inputs[0],
            Oeq | Une | Olt | Ole | Ogt | Oge => {
                let order = float(0).compare(float(1));
                u128::from(match self.op {
                    Oeq => order == Some(Ordering::Equal),
                    Une => order != Some(Ordering::Equal),
                    Olt => order == Some(Ordering::Less),
                    Ole => matches!(order, Some(Ordering::Less | Ordering::Equal)),
                    Ogt => order == Some(Ordering::Greater),
                    Oge => matches!(order, Some(Ordering::Greater | Ordering::Equal)),
                    _ => unreachable!(),
                })
            }
            Constant(_) => unreachable!(),
        }
    }
}

pub(super) fn execute(
    inst: &FloatingInst,
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
) -> Result<(), InterpreterError> {
    let args = inst
        .arguments
        .iter()
        .map(|v| read_value(values, v, block))
        .collect::<Result<Vec<_>, _>>()?;
    write_value(values, &inst.result, inst.evaluate(&args));
    Ok(())
}
