use evocc_types::{
    floating::{FiniteOp, FloatBits, FpRounding, FpStatus},
    FloatKind,
};

fn quad(bits: u128) -> FloatBits {
    FloatBits::from_bits(FloatKind::F128, bits).unwrap()
}

#[test]
fn quad_arithmetic_keeps_all_113_significand_bits_and_status() {
    let one = 0x3fff0000000000000000000000000000;
    let half_ulp = 0x3f8e0000000000000000000000000000;
    for (rounding, expected) in [
        (FpRounding::NearestEven, one),
        (FpRounding::Upward, one + 1),
        (FpRounding::Downward, one),
        (FpRounding::TowardZero, one),
    ] {
        let result = quad(one)
            .target_binary_round(quad(half_ulp), FiniteOp::Add, rounding)
            .expect("binary128 must use the shared target adapter");
        assert_eq!(result.value.bits(), expected);
        assert_eq!(result.status, FpStatus::INEXACT);
    }
}

#[test]
fn quad_narrowing_uses_the_destination_payload_and_one_rounding() {
    let q = quad(0xffff8000000000000000000000000123);
    let result = q
        .target_convert_round(FloatKind::F64, FpRounding::Upward)
        .unwrap();
    assert_eq!(result.value.bits(), 0xfff8000000000000);
    assert_eq!(result.status, FpStatus::NONE);
    let s = quad(0x7fff0000000000000000000000000123);
    assert_eq!(
        s.target_convert_round(FloatKind::F32, FpRounding::NearestEven)
            .unwrap()
            .status,
        FpStatus::INVALID
    );
    let x = quad(0x3fff0000010000000000000000000001);
    assert_eq!(
        x.target_convert_round(FloatKind::F32, FpRounding::NearestEven)
            .unwrap()
            .value
            .bits(),
        0x3f800001
    );
}

#[test]
fn quad_tininess_and_directed_overflow_do_not_use_quad_as_a_wider_type() {
    let one = 0x3fff0000000000000000000000000000;
    let two = 0x40000000000000000000000000000000;
    let minimum = 1u128 << 112;
    let maximum = 0x7ffeffffffffffffffffffffffffffff;
    let result = quad(minimum)
        .target_binary_round(quad(one - 1), FiniteOp::Mul, FpRounding::NearestEven)
        .unwrap();
    assert_eq!(result.value.bits(), minimum);
    assert_eq!(result.status, FpStatus::UNDERFLOW.union(FpStatus::INEXACT));
    let result = quad(maximum)
        .target_binary_round(quad(two), FiniteOp::Mul, FpRounding::TowardZero)
        .unwrap();
    assert_eq!(result.value.bits(), maximum);
    assert_eq!(result.status, FpStatus::OVERFLOW.union(FpStatus::INEXACT));
    let result = quad(maximum)
        .target_binary_round(quad(one), FiniteOp::Add, FpRounding::TowardZero)
        .unwrap();
    assert_eq!(result.value.bits(), maximum);
    assert_eq!(result.status, FpStatus::INEXACT);
    let result = quad(1)
        .target_binary_round(quad(two), FiniteOp::Div, FpRounding::Upward)
        .unwrap();
    assert_eq!(result.value.bits(), 1);
    assert_eq!(result.status, FpStatus::UNDERFLOW.union(FpStatus::INEXACT));
}

#[test]
fn quad_nan_transport_and_runtime_default_nan_are_separate() {
    let snan = 0xffff0000000000000000000000000123;
    let result = quad(0x7fff8000000000000000000000000022)
        .target_binary_round(quad(snan), FiniteOp::Add, FpRounding::NearestEven)
        .unwrap();
    assert_eq!(result.value.bits(), snan | (1u128 << 111));
    assert_eq!(result.status, FpStatus::INVALID);
    let result = quad(0)
        .target_binary_round(quad(0), FiniteOp::Div, FpRounding::NearestEven)
        .unwrap();
    assert_eq!(result.value.bits(), 0x7fffffffffffffffffffffffffffffff);
    assert_eq!(result.status, FpStatus::INVALID);
    assert_eq!(
        quad(snan).target_convert(FloatKind::F128).unwrap().bits(),
        snan
    );
}

#[test]
fn quad_fma_uses_the_qualified_hosted_nan_priority() {
    let q = quad(0x7fff8000000000000000000000000011);
    let s = quad(0x7fff0000000000000000000000000033);
    let result = quad(0)
        .target_fma_round(q, q.negated(), FpRounding::NearestEven)
        .unwrap();
    assert_eq!(result.value, q);
    assert_eq!(result.status, FpStatus::NONE);
    let result = q
        .target_fma_round(s, s.negated(), FpRounding::NearestEven)
        .unwrap();
    assert_eq!(result.value.bits(), 0xffff8000000000000000000000000033);
    assert_eq!(result.status, FpStatus::INVALID);
}
