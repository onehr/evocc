use evocc_types::{
    floating::{FiniteOp, FloatBits, FpEnvironment, FpRounding, FpStatus},
    FloatKind,
};

fn f64(bits: u128) -> FloatBits {
    FloatBits::from_bits(FloatKind::F64, bits).unwrap()
}
fn f32(bits: u128) -> FloatBits {
    FloatBits::from_bits(FloatKind::F32, bits).unwrap()
}

#[test]
fn all_rounding_modes_and_sticky_state_are_explicit() {
    let original = FpEnvironment::default();
    for (rounding, bits) in [
        (FpRounding::NearestEven, 0x3ff0000000000000),
        (FpRounding::Upward, 0x3ff0000000000001),
        (FpRounding::Downward, 0x3ff0000000000000),
        (FpRounding::TowardZero, 0x3ff0000000000000),
    ] {
        let state = FpEnvironment {
            rounding,
            ..original
        }
        .raised(FpStatus::DIV_BY_ZERO);
        let result = f64(0x3ff0000000000000)
            .target_binary_round(f64(0x3ca0000000000000), FiniteOp::Add, state.rounding)
            .unwrap();
        assert_eq!(result.value.bits(), bits);
        assert_eq!(result.status, FpStatus::INEXACT);
        let after = state.raised(result.status);
        assert_eq!(after.flags, FpStatus::DIV_BY_ZERO.union(FpStatus::INEXACT));
        assert_eq!(
            after.cleared(FpStatus::INEXACT).flags,
            FpStatus::DIV_BY_ZERO
        );
        assert_eq!(original, FpEnvironment::default());
    }
}

#[test]
fn target_tininess_is_before_rounding_and_exact_subnormal_is_not_underflow() {
    let tiny = f32(0x00800000)
        .target_binary_round(f32(0x3f7fffff), FiniteOp::Mul, FpRounding::NearestEven)
        .unwrap();
    assert_eq!(tiny.value.bits(), 0x00800000);
    assert_eq!(tiny.status, FpStatus::UNDERFLOW.union(FpStatus::INEXACT));
    let exact = f32(0x00800000)
        .target_binary_round(f32(0x40000000), FiniteOp::Div, FpRounding::NearestEven)
        .unwrap();
    assert_eq!(exact.value.bits(), 0x00400000);
    assert_eq!(exact.status, FpStatus::NONE);
    let tiny = f32(1)
        .target_binary_round(f32(0x40000000), FiniteOp::Div, FpRounding::Upward)
        .unwrap();
    assert_eq!(tiny.value.bits(), 1);
    assert_eq!(tiny.status, FpStatus::UNDERFLOW.union(FpStatus::INEXACT));
}

#[test]
fn directed_overflow_status_is_not_lost_when_result_is_finite() {
    let result = f32(0x3f7fffff)
        .target_binary_round(f32(0x80000001), FiniteOp::Div, FpRounding::Upward)
        .unwrap();
    assert_eq!(result.value.bits(), 0xff7fffff);
    assert_eq!(result.status, FpStatus::OVERFLOW.union(FpStatus::INEXACT));
    // Max-finite plus a small increment is inexact, but downward does not
    // overflow until its unbounded-exponent rounded value exceeds the range.
    let result = f32(0x7f7fffff)
        .target_binary_round(f32(0x3f800000), FiniteOp::Add, FpRounding::TowardZero)
        .unwrap();
    assert_eq!(result.value.bits(), 0x7f7fffff);
    assert_eq!(result.status, FpStatus::INEXACT);
}

#[test]
fn fused_nan_order_and_one_rounding_are_not_binary_composition() {
    let result = f32(0x3f800001)
        .target_fma_round(f32(0x3f7ffffe), f32(0xbf800000), FpRounding::NearestEven)
        .unwrap();
    assert_eq!(result.value.bits(), 0xa8800000);
    assert_eq!(result.status, FpStatus::NONE);
    let result = f32(0x7fc00011)
        .target_fma_round(f32(0x7fc00022), f32(0x7fc00033), FpRounding::NearestEven)
        .unwrap();
    assert_eq!(result.value.bits(), 0x7fc00033);
    let result = f32(0)
        .target_fma_round(f32(0x7f800000), f32(0x7fc00055), FpRounding::NearestEven)
        .unwrap();
    assert_eq!(result.value.bits(), 0x7fc00000);
    assert_eq!(result.status, FpStatus::INVALID);
    let result = f32(0)
        .target_fma_round(f32(0x7f800000), f32(0xff800055), FpRounding::NearestEven)
        .unwrap();
    assert_eq!(result.value.bits(), 0xffc00055);
    assert_eq!(result.status, FpStatus::INVALID);
}

#[test]
fn representation_and_quiet_vs_signaling_comparisons_are_distinct() {
    let q = f32(0x7fc00011);
    let s = f32(0x7f800022);
    assert_eq!(q.target_compare(q, false).unwrap().status, FpStatus::NONE);
    assert_eq!(q.target_compare(q, true).unwrap().status, FpStatus::INVALID);
    assert_eq!(
        s.target_compare(q, false).unwrap().status,
        FpStatus::INVALID
    );
    assert_eq!(
        s.target_convert_round(FloatKind::F32, FpRounding::Upward)
            .unwrap()
            .value,
        s
    );
    assert_eq!(
        s.target_convert_round(FloatKind::F64, FpRounding::Upward)
            .unwrap()
            .status,
        FpStatus::INVALID
    );
    assert_eq!(s.negated().bits(), 0xff800022);
}

#[test]
fn conversions_keep_status_without_admitting_invalid_c_values() {
    let result = FloatBits::from_unsigned_round(FloatKind::F32, 16777217, FpRounding::Upward);
    assert_eq!(result.value.bits(), 0x4b800001);
    assert_eq!(result.status, FpStatus::INEXACT);
    let result = f64(0x3ff8000000000000).to_integer_status(32, true).unwrap();
    assert_eq!(result.value, Some(1));
    assert_eq!(result.status, FpStatus::INEXACT);
    let result = f64(0x7ff0000000000000)
        .to_integer_status(128, true)
        .unwrap();
    assert_eq!(result.value, None);
    assert_eq!(result.status, FpStatus::INVALID);
    assert!(f64(0).to_integer_status(129, false).is_none());
    assert!(FpStatus::from_bits(32).is_none());
}
#[test]
fn signed_one_bit_conversion_uses_the_real_domain_without_engine_panic() {
    use evocc_types::{
        floating::{FloatBits, FpStatus},
        FloatKind,
    };
    for kind in [FloatKind::F32, FloatKind::F64, FloatKind::F128] {
        for (text, value, flags) in [
            ("0", Some(0), FpStatus::NONE),
            ("-1", Some(u128::MAX), FpStatus::NONE),
            ("0.5", Some(0), FpStatus::INEXACT),
            ("-1.5", Some(u128::MAX), FpStatus::INEXACT),
            ("1", None, FpStatus::INVALID),
            ("-2", None, FpStatus::INVALID),
            ("1.5", None, FpStatus::INVALID),
        ] {
            let input = FloatBits::parse(kind, text).unwrap();
            let actual = input.to_integer_status(1, true).unwrap();
            assert_eq!(
                (actual.value, actual.status),
                (value, flags),
                "{kind:?} {text}"
            );
            assert_eq!(input.to_integer(1, true), value);
        }
    }
}
