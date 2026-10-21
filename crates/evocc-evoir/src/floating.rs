//! Exact binary32/binary64 operations, excluded from the scalar EqSat allowlist.
use crate::*;
use evocc_types::floating::{
    libgcc_complex, ComplexArithmetic, FiniteOp, FloatBits, FpResult, FpRounding,
};
use evocc_types::FloatKind;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatingOp {
    Constant(u128),
    Add,
    Sub,
    Mul,
    Div,
    Fma,
    Neg,
    Oeq,
    Une,
    Olt,
    Ole,
    Ogt,
    Oge,
    QuietOlt,
    QuietOle,
    QuietOgt,
    QuietOge,
    Convert,
    FromSigned,
    FromUnsigned,
    ToSigned,
    ToUnsigned,
    ToIntDefined {
        width: u16,
        signed: bool,
    },
    ToBits,
    FromBits,
    /// One part of the product of `a + bi` and `c + di`, as the target's
    /// complex multiplication helper computes it (3.13.5).
    ComplexMul {
        imaginary: bool,
    },
    /// One part of the quotient of `a + bi` by `c + di`, as the target's
    /// complex division helper computes it (3.13.5).
    ComplexDiv {
        imaginary: bool,
    },
}

impl FloatingOp {
    pub fn mnemonic(self) -> String {
        match self {
            Self::Constant(_) => "const",
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Fma => "fma",
            Self::Neg => "neg",
            Self::Oeq => "oeq",
            Self::Une => "une",
            Self::Olt => "olt",
            Self::Ole => "ole",
            Self::Ogt => "ogt",
            Self::Oge => "oge",
            Self::QuietOlt => "quiet_olt",
            Self::QuietOle => "quiet_ole",
            Self::QuietOgt => "quiet_ogt",
            Self::QuietOge => "quiet_oge",
            Self::Convert => "convert",
            Self::FromSigned => "from_signed",
            Self::FromUnsigned => "from_unsigned",
            Self::ToSigned => "to_signed",
            Self::ToUnsigned => "to_unsigned",
            Self::ToBits => "to_bits",
            Self::FromBits => "from_bits",
            Self::ComplexMul { imaginary: false } => "cmul.real",
            Self::ComplexMul { imaginary: true } => "cmul.imag",
            Self::ComplexDiv { imaginary: false } => "cdiv.real",
            Self::ComplexDiv { imaginary: true } => "cdiv.imag",
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
            "fma" => Self::Fma,
            "neg" => Self::Neg,
            "oeq" => Self::Oeq,
            "une" => Self::Une,
            "olt" => Self::Olt,
            "ole" => Self::Ole,
            "ogt" => Self::Ogt,
            "oge" => Self::Oge,
            "quiet_olt" => Self::QuietOlt,
            "quiet_ole" => Self::QuietOle,
            "quiet_ogt" => Self::QuietOgt,
            "quiet_oge" => Self::QuietOge,
            "convert" => Self::Convert,
            "from_signed" => Self::FromSigned,
            "from_unsigned" => Self::FromUnsigned,
            "to_signed" => Self::ToSigned,
            "to_unsigned" => Self::ToUnsigned,
            "to_bits" => Self::ToBits,
            "from_bits" => Self::FromBits,
            "cmul.real" => Self::ComplexMul { imaginary: false },
            "cmul.imag" => Self::ComplexMul { imaginary: true },
            "cdiv.real" => Self::ComplexDiv { imaginary: false },
            "cdiv.imag" => Self::ComplexDiv { imaginary: true },
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
        EvoIRType::F16 => Some(FloatKind::F16),
        EvoIRType::BF16 => Some(FloatKind::BF16),
        EvoIRType::F32 => Some(FloatKind::F32),
        EvoIRType::F64 => Some(FloatKind::F64),
        EvoIRType::F128 => Some(FloatKind::F128),
        _ => None,
    }
}

/// Whether `ty` is a half storage format (EvoIR 3.13.6).
pub fn is_half_type(ty: &EvoIRType) -> bool {
    matches!(ty, EvoIRType::F16 | EvoIRType::BF16)
}

/// The representation type of a floating type: `bits<N>` of its width.
pub fn floating_bits_type(ty: &EvoIRType) -> EvoIRType {
    match ty {
        EvoIRType::F16 | EvoIRType::BF16 => EvoIRType::Bits(16),
        EvoIRType::F32 => EvoIRType::Bits(32),
        EvoIRType::F128 => EvoIRType::Bits(128),
        _ => EvoIRType::Bits(64),
    }
}

impl FloatingInst {
    /// Exact representation transport is independent of FP controls; numeric
    /// operations retain the fixed-profile admission rule when not strict.
    pub fn needs_environment(&self) -> bool {
        !matches!(
            self.op,
            FloatingOp::Constant(_)
                | FloatingOp::Neg
                | FloatingOp::ToBits
                | FloatingOp::FromBits
                | FloatingOp::ToIntDefined { .. }
        )
    }
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
        let bits = floating_bits_type;
        // A half format is a storage format. Constants, conversions and
        // representation moves are its only operations (3.13.6).
        let half = is_half_type(result) || args.iter().any(|a| is_half_type(&a.ty));
        if half
            && !matches!(
                self.op,
                Constant(_) | Convert | FromSigned | FromUnsigned | ToBits | FromBits
            )
        {
            return Err("a half format admits only constants, conversions and bits".into());
        }
        let valid = match self.op {
            Constant(raw) => {
                fp_result
                    && args.is_empty()
                    && (!is_half_type(result) || raw <= u128::from(u16::MAX))
                    && (*result != EvoIRType::F32 || raw <= u128::from(u32::MAX))
                    && (*result != EvoIRType::F64 || raw <= u128::from(u64::MAX))
            }
            Add | Sub | Mul | Div => fp_binary && *result == args[0].ty,
            Fma => args.len() == 3 && fp_result && args.iter().all(|a| a.ty == *result),
            Neg => fp_unary && *result == args[0].ty,
            Oeq | Une | Olt | Ole | Ogt | Oge | QuietOlt | QuietOle | QuietOgt | QuietOge => {
                fp_binary && *result == EvoIRType::Bits(1)
            }
            Convert => fp_unary && fp_result,
            FromSigned | FromUnsigned => unary && integer(&args[0].ty) && fp_result,
            ToSigned | ToUnsigned => fp_unary && integer(result),
            ToIntDefined { width, .. } => {
                fp_unary && (1..=128).contains(&width) && *result == EvoIRType::Bits(1)
            }
            ToBits => fp_unary && *result == bits(&args[0].ty),
            FromBits => unary && fp_result && args[0].ty == bits(result),
            ComplexMul { .. } | ComplexDiv { .. } => {
                args.len() == 4 && fp_result && args.iter().all(|a| a.ty == *result)
            }
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
            return bits;
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
            Fma => float(0)
                .target_fma_round(
                    float(1),
                    float(2),
                    evocc_types::floating::FpRounding::NearestEven,
                )
                .unwrap()
                .value
                .bits(),
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
            ComplexMul { .. } | ComplexDiv { .. } => {
                self.evaluate_complex(inputs, FpRounding::NearestEven).value
            }
            Oeq | Une | Olt | Ole | Ogt | Oge | QuietOlt | QuietOle | QuietOgt | QuietOge => {
                let order = float(0).compare(float(1));
                u128::from(match self.op {
                    Oeq => order == Some(Ordering::Equal),
                    Une => order != Some(Ordering::Equal),
                    Olt | QuietOlt => order == Some(Ordering::Less),
                    Ole | QuietOle => matches!(order, Some(Ordering::Less | Ordering::Equal)),
                    Ogt | QuietOgt => order == Some(Ordering::Greater),
                    Oge | QuietOge => matches!(order, Some(Ordering::Greater | Ordering::Equal)),
                    _ => unreachable!(),
                })
            }
            Constant(_) => unreachable!(),
        }
    }
}

impl FloatingInst {
    /// Whether this is a part of a complex product or quotient, whose value
    /// the target's runtime helper defines.
    pub fn is_complex_helper(&self) -> bool {
        matches!(
            self.op,
            FloatingOp::ComplexMul { .. } | FloatingOp::ComplexDiv { .. }
        )
    }

    /// The part of the qualified Linux helper's result that this
    /// instruction names, in `rounding`, with the status of the whole helper.
    pub(crate) fn evaluate_complex(&self, inputs: &[u128], rounding: FpRounding) -> FpResult<u128> {
        let kind = floating_kind(&self.result.ty).expect("validated complex part format");
        let part = |index: usize| FloatBits::from_bits(kind, inputs[index]).expect("operand bits");
        let (op, imaginary) = match self.op {
            FloatingOp::ComplexMul { imaginary } => (ComplexArithmetic::Multiply, imaginary),
            FloatingOp::ComplexDiv { imaginary } => (ComplexArithmetic::Divide, imaginary),
            _ => unreachable!("validated complex part"),
        };
        let result = libgcc_complex(op, [part(0), part(1), part(2), part(3)], rounding)
            .expect("validated complex operands share one executable format");
        FpResult {
            value: result.value[usize::from(imaginary)].bits(),
            status: result.status,
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
