//! Exact contracts of the pinned hosted Linux numeric runtime. Helper names
//! belong to MIR only, never to the language IR.
use super::*;
use evocc_types::floating::{FpResult, FpRounding};
use std::cmp::Ordering;

#[derive(Clone, Copy)]
enum Comparison {
    Equality,
    Less,
    Greater,
    Unordered,
}
#[derive(Clone)]
pub(crate) struct Helper {
    instruction: FloatingInst,
    comparison: Option<Comparison>,
    /// A complex helper returns both parts of its product or quotient;
    /// `instruction` names its real part.
    complex: bool,
}
fn registers(ty: &EvoIRType) -> Vec<RegClass> {
    match ty {
        EvoIRType::Bits(128) => vec![RegClass::Gpr64; 2],
        EvoIRType::Bits(1..=32) => vec![RegClass::Gpr32],
        EvoIRType::Bits(33..=64) => vec![RegClass::Gpr64],
        _ => vec![class(evocc_evoir::floating_kind(ty).unwrap())],
    }
}
impl Helper {
    pub(crate) fn classes(&self) -> (Vec<RegClass>, Vec<RegClass>) {
        let mut results = registers(&self.instruction.result.ty);
        if self.complex {
            results.extend(registers(&self.instruction.result.ty));
        }
        (
            self.instruction
                .arguments
                .iter()
                .flat_map(|a| registers(&a.ty))
                .collect(),
            results,
        )
    }
    pub(crate) fn is_complex(&self) -> bool {
        self.complex
    }
    /// Both parts of a complex helper's result and the status of the call.
    pub(crate) fn evaluate_parts(
        &self,
        bits: &[u128],
        rounding: FpRounding,
    ) -> FpResult<[u128; 2]> {
        let real = self.instruction.evaluate_strict(bits, rounding);
        let mut imaginary = self.instruction.clone();
        imaginary.op = match self.instruction.op {
            FloatingOp::ComplexMul { .. } => FloatingOp::ComplexMul { imaginary: true },
            _ => FloatingOp::ComplexDiv { imaginary: true },
        };
        let imaginary = imaginary.evaluate_strict(bits, rounding);
        FpResult {
            value: [real.value, imaginary.value],
            status: real.status.union(imaginary.status),
        }
    }
    pub(crate) fn argument_types(&self) -> Vec<EvoIRType> {
        self.instruction
            .arguments
            .iter()
            .map(|a| a.ty.clone())
            .collect()
    }
    pub(crate) fn result_type(&self) -> &EvoIRType {
        &self.instruction.result.ty
    }
    pub(crate) fn inputs_defined(&self, bits: &[u128]) -> bool {
        if matches!(
            self.instruction.op,
            FloatingOp::ToSigned | FloatingOp::ToUnsigned
        ) {
            let EvoIRType::Bits(width) = self.instruction.result.ty else {
                unreachable!()
            };
            FloatBits::from_bits(
                evocc_evoir::floating_kind(&self.instruction.arguments[0].ty).unwrap(),
                bits[0],
            )
            .unwrap()
            .to_integer(width, self.instruction.op == FloatingOp::ToSigned)
            .is_some()
        } else {
            true
        }
    }
    pub(crate) fn evaluate(&self, bits: &[u128], rounding: FpRounding) -> FpResult<u128> {
        if let Some(op) = self.comparison {
            let a = FloatBits::from_bits(FloatKind::F128, bits[0]).unwrap();
            let b = FloatBits::from_bits(FloatKind::F128, bits[1]).unwrap();
            let result = a
                .target_compare(b, matches!(op, Comparison::Less | Comparison::Greater))
                .unwrap();
            let value = match op {
                Comparison::Equality => i32::from(result.value != Some(Ordering::Equal)),
                Comparison::Unordered => i32::from(result.value.is_none()),
                _ => match result.value {
                    Some(Ordering::Less) => -1,
                    Some(Ordering::Equal) => 0,
                    Some(Ordering::Greater) => 1,
                    None if matches!(op, Comparison::Less) => 2,
                    None => -2,
                },
            };
            FpResult {
                value: u128::from(value as u32),
                status: result.status,
            }
        } else {
            self.instruction.evaluate_strict(bits, rounding)
        }
    }
}

pub(crate) fn qualified(symbol: &str) -> Option<Helper> {
    use EvoIRType::{Bits, F128, F32, F64};
    use FloatingOp::*;
    let mut comparison = None;
    let mut complex = false;
    let (op, arguments, result) = if let Some((from, signed, kind)) = hosted_conversion(symbol) {
        let fp = ty(kind);
        if from {
            (
                if signed { FromSigned } else { FromUnsigned },
                vec![Bits(128)],
                fp,
            )
        } else {
            (
                if signed { ToSigned } else { ToUnsigned },
                vec![fp],
                Bits(128),
            )
        }
    } else {
        match symbol {
            "__addtf3" => (Add, vec![F128; 2], F128),
            "__subtf3" => (Sub, vec![F128; 2], F128),
            "__multf3" => (Mul, vec![F128; 2], F128),
            "__divtf3" => (Div, vec![F128; 2], F128),
            "fmal" => (Fma, vec![F128; 3], F128),
            "__extendsftf2" => (Convert, vec![F32], F128),
            "__extenddftf2" => (Convert, vec![F64], F128),
            "__trunctfsf2" => (Convert, vec![F128], F32),
            "__trunctfdf2" => (Convert, vec![F128], F64),
            "__floatsitf" => (FromSigned, vec![Bits(32)], F128),
            "__floatditf" => (FromSigned, vec![Bits(64)], F128),
            "__floattitf" => (FromSigned, vec![Bits(128)], F128),
            "__floatunsitf" => (FromUnsigned, vec![Bits(32)], F128),
            "__floatunditf" => (FromUnsigned, vec![Bits(64)], F128),
            "__floatuntitf" => (FromUnsigned, vec![Bits(128)], F128),
            "__fixtfsi" => (ToSigned, vec![F128], Bits(32)),
            "__fixtfdi" => (ToSigned, vec![F128], Bits(64)),
            "__fixtfti" => (ToSigned, vec![F128], Bits(128)),
            "__fixunstfsi" => (ToUnsigned, vec![F128], Bits(32)),
            "__fixunstfdi" => (ToUnsigned, vec![F128], Bits(64)),
            "__fixunstfti" => (ToUnsigned, vec![F128], Bits(128)),
            "__eqtf2" | "__netf2" | "__letf2" | "__lttf2" | "__getf2" | "__gttf2"
            | "__unordtf2" => {
                comparison = Some(match symbol {
                    "__eqtf2" | "__netf2" => Comparison::Equality,
                    "__letf2" | "__lttf2" => Comparison::Less,
                    "__getf2" | "__gttf2" => Comparison::Greater,
                    _ => Comparison::Unordered,
                });
                (Oeq, vec![F128; 2], Bits(32))
            }
            "__mulsc3" | "__muldc3" | "__multc3" | "__divsc3" | "__divdc3" | "__divtc3" => {
                complex = true;
                let ty = match &symbol[5..] {
                    "sc3" => F32,
                    "dc3" => F64,
                    _ => F128,
                };
                let op = if symbol.starts_with("__mul") {
                    ComplexMul { imaginary: false }
                } else {
                    ComplexDiv { imaginary: false }
                };
                (op, vec![ty.clone(); 4], ty)
            }
            _ => return None,
        }
    };
    Some(Helper {
        instruction: FloatingInst {
            result: Value { id: 0, ty: result },
            op,
            arguments: arguments
                .into_iter()
                .enumerate()
                .map(|(i, ty)| Value {
                    id: i as u32 + 1,
                    ty,
                })
                .collect(),
        },
        comparison,
        complex,
    })
}
