//! The nominal types and typed builtin interface of the GCC 14 AArch64 header.
//! Layout and call checking use these types even in unused inline bodies.

use super::*;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Exact name, return type and parameter types recorded from GCC 14.2.0-19's
/// typed arm_neon.h tree on aarch64-linux. This is interface data, not a rule
/// that guesses a builtin's signature from its containing wrapper.
pub fn signature(name: &str) -> Option<&'static [&'static str]> {
    static SIGNATURES: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    SIGNATURES
        .get_or_init(|| {
            include_str!("neon-signatures.tsv")
                .lines()
                .map(|line| {
                    let mut fields = line.split('\t');
                    let name = fields.next().expect("signature name");
                    (name, fields.collect())
                })
                .collect()
        })
        .get(name)
        .map(Vec::as_slice)
}

/// Apple clang 21's builtin interfaces, measured from its unmodified NEON
/// and ACLE headers with AST dumps. These types let unused header bodies
/// type-check; they do not admit execution of the builtins.
pub fn apple_signature(name: &str) -> Option<&'static [&'static str]> {
    static SIGNATURES: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    SIGNATURES
        .get_or_init(|| {
            include_str!("neon-apple-signatures.tsv")
                .lines()
                .map(|line| {
                    let mut fields = line.split('\t');
                    (fields.next().expect("signature name"), fields.collect())
                })
                .collect()
        })
        .get(name)
        .map(Vec::as_slice)
}

/// Every name that [`signature`] knows, in table order.
pub fn names() -> impl Iterator<Item = &'static str> {
    include_str!("neon-signatures.tsv")
        .lines()
        .filter_map(|line| line.split('\t').next())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Reinterpret,
    PolynomialMultiply,
    PolynomialLong { high: bool },
    ReverseByteBits,
    Combine,
    ExtractHalf { high: bool },
    Aes(AesOperation),
    Load,
    Store,
    ShiftLeft,
    ShiftRight { signed: bool },
    ShiftRightNarrow,
    ShiftInsert { right: bool },
    LoadInterleaved { vectors: u8 },
    Table,
    LaneBounds,
}

/// Only exact registered interfaces can be lowered as target operations. A
/// target builtin outside this set is not an unresolved library function.
pub fn operation(name: &str) -> Option<Operation> {
    signature(name)?;
    let aes = match name {
        "__builtin_aarch64_crypto_aesev16qi_uuu" => Some(AesOperation::Encrypt),
        "__builtin_aarch64_crypto_aesdv16qi_uuu" => Some(AesOperation::Decrypt),
        "__builtin_aarch64_crypto_aesmcv16qi_uu" => Some(AesOperation::Mix),
        "__builtin_aarch64_crypto_aesimcv16qi_uu" => Some(AesOperation::InverseMix),
        _ => None,
    };
    if let Some(op) = aes {
        return Some(Operation::Aes(op));
    }
    match name {
        "__builtin_aarch64_rbitv8qi" | "__builtin_aarch64_rbitv16qi" => {
            return Some(Operation::ReverseByteBits);
        }
        "__builtin_aarch64_crypto_pmulldi_ppp" => {
            return Some(Operation::PolynomialLong { high: false });
        }
        "__builtin_aarch64_crypto_pmullv2di_ppp" => {
            return Some(Operation::PolynomialLong { high: true });
        }
        _ => {}
    }
    if name.starts_with("vreinterpret") {
        return Some(Operation::Reinterpret);
    }
    if matches!(
        name,
        "__builtin_aarch64_pmulv8qi_ppp" | "__builtin_aarch64_pmulv16qi_ppp"
    ) {
        return Some(Operation::PolynomialMultiply);
    }
    let name = name.strip_prefix("__builtin_aarch64_")?;
    for (prefix, high) in [("get_low", false), ("get_high", true)] {
        if name
            .strip_prefix(prefix)
            .is_some_and(|mode| matches!(mode, "v16qi" | "v8hi" | "v4si" | "v2di"))
        {
            return Some(Operation::ExtractHalf { high });
        }
    }
    let (stem, _) = name.split_once('_').unwrap_or((name, ""));
    if stem == "im" && name == "im_lane_boundsi" {
        return Some(Operation::LaneBounds);
    }
    if matches!(
        stem,
        "combinev8qi" | "combinev4hi" | "combinev2si" | "combinedi"
    ) {
        return Some(Operation::Combine);
    }
    let vector_mode = |s: &str| {
        matches!(
            s,
            "v8qi" | "v16qi" | "v4hi" | "v8hi" | "v2si" | "v4si" | "v1di" | "v2di"
        )
    };
    if name
        .strip_prefix("shrn_n")
        .is_some_and(|rest| matches!(rest.split('_').next(), Some("v8hi" | "v4si" | "v2di")))
    {
        return Some(Operation::ShiftRightNarrow);
    }
    for (prefix, right) in [
        ("ssli_n", false),
        ("usli_n", false),
        ("ssri_n", true),
        ("usri_n", true),
    ] {
        if name
            .strip_prefix(prefix)
            .is_some_and(|rest| vector_mode(rest.split('_').next().unwrap_or(rest)))
        {
            return Some(Operation::ShiftInsert { right });
        }
    }
    for (prefix, vectors) in [("ld2", 2), ("ld3", 3), ("ld4", 4)] {
        if stem.strip_prefix(prefix).is_some_and(vector_mode) {
            return Some(Operation::LoadInterleaved { vectors });
        }
    }
    for (prefix, operation) in [
        ("ld1", Operation::Load),
        ("st1", Operation::Store),
        ("ashl", Operation::ShiftLeft),
        ("lshr", Operation::ShiftRight { signed: false }),
        ("ashr", Operation::ShiftRight { signed: true }),
    ] {
        if stem.strip_prefix(prefix).is_some_and(vector_mode) {
            return Some(operation);
        }
    }
    matches!(stem, "qtbl1v16qi" | "qtbl1v8qi" | "tbl1v8qi").then_some(Operation::Table)
}

/// AArch64 AES round operations on sixteen bytes, not floating-point values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AesOperation {
    Encrypt,
    Decrypt,
    Mix,
    InverseMix,
}

impl AesOperation {
    pub fn mnemonic(self) -> &'static str {
        match self {
            Self::Encrypt => "aese",
            Self::Decrypt => "aesd",
            Self::Mix => "aesmc",
            Self::InverseMix => "aesimc",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "aese" => Self::Encrypt,
            "aesd" => Self::Decrypt,
            "aesmc" => Self::Mix,
            "aesimc" => Self::InverseMix,
            _ => return None,
        })
    }
    pub fn arity(self) -> usize {
        if matches!(self, Self::Encrypt | Self::Decrypt) {
            2
        } else {
            1
        }
    }

    /// Preserve byte initializedness through the actual dependencies. No
    /// pointer identity can be manufactured by these representation operations.
    pub fn evaluate(self, inputs: &[[Option<u8>; 16]]) -> [Option<u8>; 16] {
        assert_eq!(inputs.len(), self.arity(), "validated AES operands");
        std::array::from_fn(|index| {
            let row = index % 4;
            let column = index / 4;
            match self {
                Self::Encrypt | Self::Decrypt => {
                    let inverse = self == Self::Decrypt;
                    let column = (column + if inverse { 4 - row } else { row }) % 4;
                    let index = column * 4 + row;
                    let mut value = inputs[0][index]? ^ inputs[1][index]?;
                    if inverse {
                        value = value.rotate_left(1)
                            ^ value.rotate_left(3)
                            ^ value.rotate_left(6)
                            ^ 0x05;
                        Some(aes_inverse(value))
                    } else {
                        value = aes_inverse(value);
                        Some(
                            value
                                ^ value.rotate_left(1)
                                ^ value.rotate_left(2)
                                ^ value.rotate_left(3)
                                ^ value.rotate_left(4)
                                ^ 0x63,
                        )
                    }
                }
                Self::Mix | Self::InverseMix => {
                    let coefficients = if self == Self::Mix {
                        [2, 3, 1, 1]
                    } else {
                        [14, 11, 13, 9]
                    };
                    let mut result = 0;
                    for (offset, coefficient) in coefficients.into_iter().enumerate() {
                        result ^=
                            aes_multiply(inputs[0][column * 4 + (row + offset) % 4]?, coefficient);
                    }
                    Some(result)
                }
            }
        })
    }
}

fn aes_multiply(mut left: u8, mut right: u8) -> u8 {
    let mut result = 0;
    for _ in 0..8 {
        if right & 1 != 0 {
            result ^= left;
        }
        left = (left << 1) ^ if left & 0x80 != 0 { 0x1b } else { 0 };
        right >>= 1;
    }
    result
}

fn aes_inverse(value: u8) -> u8 {
    // a^254 in GF(2^8), including the AES definition inverse(0) = 0.
    let mut power = value;
    let mut result = 1;
    for bit in 0..8 {
        if 254 & (1 << bit) != 0 {
            result = aes_multiply(result, power);
        }
        power = aes_multiply(power, power);
    }
    result
}

pub fn immediate_indices(op: Operation) -> &'static [usize] {
    match op {
        Operation::ShiftLeft | Operation::ShiftRight { .. } | Operation::ShiftRightNarrow => &[1],
        Operation::ShiftInsert { .. } => &[2],
        Operation::LaneBounds => &[0, 1, 2],
        _ => &[],
    }
}

/// The target-header tuple has exactly one array member. Check the canonical
/// record instead of treating an arbitrary same-sized aggregate as a tuple.
pub fn tuple_vector(types: &TypeInterner, ty: TypeId, count: u8) -> Option<TypeId> {
    let TypeKind::Struct(tag) = types.get(ty)?.kind else {
        return None;
    };
    let TagState::Complete(TagDefinition::Struct(fields)) = &types.tag(tag)?.state else {
        return None;
    };
    let [field] = fields.as_slice() else {
        return None;
    };
    if field.name.as_deref() != Some("val") || field.bit_field.is_some() {
        return None;
    }
    let TypeKind::Array {
        element,
        bound: ArrayBound::Fixed(length),
    } = types.get(field.ty)?.kind
    else {
        return None;
    };
    (length == u64::from(count)
        && matches!(
            types.get(element)?.kind,
            TypeKind::Vector {
                family: VectorFamily::ArmNeon,
                ..
            }
        ))
    .then_some(element)
}

#[derive(Clone, Copy, Debug)]
pub enum Scalar {
    Integer(IntRank),
    Floating(FloatKind),
    Target(TargetBuiltinType),
}

impl Scalar {
    pub fn kind(self) -> TypeKind {
        match self {
            Self::Integer(rank) => TypeKind::Int(rank),
            Self::Floating(kind) => TypeKind::Float(kind),
            Self::Target(kind) => TypeKind::TargetBuiltin(kind),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinType {
    pub element: Scalar,
    pub lanes: Option<u16>,
}

impl BuiltinType {
    pub fn from_spelling(name: &str) -> Option<Self> {
        if let Some((rank, lanes)) = arm_integer_vector(name) {
            return Some(Self {
                element: Scalar::Integer(rank),
                lanes: Some(lanes),
            });
        }
        if let Some(mode) = name.strip_prefix("__builtin_aarch64_simd_") {
            let element = match mode {
                "qi" => Scalar::Integer(IntRank::SChar),
                "hi" => Scalar::Integer(IntRank::Short),
                "si" => Scalar::Integer(IntRank::Int),
                "di" => Scalar::Integer(IntRank::Long),
                "hf" => Scalar::Target(TargetBuiltinType::ArmFp16),
                "sf" => Scalar::Floating(FloatKind::F32),
                "df" => Scalar::Floating(FloatKind::F64),
                "bf" => Scalar::Target(TargetBuiltinType::ArmBf16),
                _ => return None,
            };
            return Some(Self {
                element,
                lanes: None,
            });
        }
        let (base, lanes) = name
            .strip_prefix("__")?
            .strip_suffix("_t")?
            .split_once('x')?;
        let lanes = lanes.parse::<u16>().ok()?;
        let (element, bits) = match base {
            "Float16" => (Scalar::Target(TargetBuiltinType::ArmFp16), 16),
            "Float32" => (Scalar::Floating(FloatKind::F32), 32),
            "Float64" => (Scalar::Floating(FloatKind::F64), 64),
            "Bfloat16" => (Scalar::Target(TargetBuiltinType::ArmBf16), 16),
            "Poly8" => (Scalar::Target(TargetBuiltinType::ArmPoly8), 8),
            "Poly16" => (Scalar::Target(TargetBuiltinType::ArmPoly16), 16),
            "Poly64" => (Scalar::Target(TargetBuiltinType::ArmPoly64), 64),
            _ => return None,
        };
        matches!(lanes.checked_mul(bits), Some(64 | 128)).then_some(Self {
            element,
            lanes: Some(lanes),
        })
    }

    pub fn kind(self, types: &mut TypeInterner) -> TypeKind {
        if let Some(lanes) = self.lanes {
            let element = types.intern(CanonicalType {
                kind: self.element.kind(),
                qualifiers: Qualifiers::default(),
            });
            TypeKind::Vector {
                element,
                lanes,
                family: VectorFamily::ArmNeon,
            }
        } else {
            self.element.kind()
        }
    }
}

/// Public tuple names are typedefs introduced by the target-header pragma,
/// not keywords. Each member is an array of nominal vectors.
pub const TUPLE_BASES: &[(&str, &str)] = &[
    ("int8x8", "__Int8x8_t"),
    ("int8x16", "__Int8x16_t"),
    ("int16x4", "__Int16x4_t"),
    ("int16x8", "__Int16x8_t"),
    ("int32x2", "__Int32x2_t"),
    ("int32x4", "__Int32x4_t"),
    ("int64x1", "__Int64x1_t"),
    ("int64x2", "__Int64x2_t"),
    ("uint8x8", "__Uint8x8_t"),
    ("uint8x16", "__Uint8x16_t"),
    ("uint16x4", "__Uint16x4_t"),
    ("uint16x8", "__Uint16x8_t"),
    ("uint32x2", "__Uint32x2_t"),
    ("uint32x4", "__Uint32x4_t"),
    ("uint64x1", "__Uint64x1_t"),
    ("uint64x2", "__Uint64x2_t"),
    ("float16x4", "__Float16x4_t"),
    ("float16x8", "__Float16x8_t"),
    ("float32x2", "__Float32x2_t"),
    ("float32x4", "__Float32x4_t"),
    ("float64x1", "__Float64x1_t"),
    ("float64x2", "__Float64x2_t"),
    ("bfloat16x4", "__Bfloat16x4_t"),
    ("bfloat16x8", "__Bfloat16x8_t"),
    ("poly8x8", "__Poly8x8_t"),
    ("poly8x16", "__Poly8x16_t"),
    ("poly16x4", "__Poly16x4_t"),
    ("poly16x8", "__Poly16x8_t"),
    ("poly64x1", "__Poly64x1_t"),
    ("poly64x2", "__Poly64x2_t"),
];
