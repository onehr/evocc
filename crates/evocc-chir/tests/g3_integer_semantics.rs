use evocc_chir::integer::{
    eval_binary_integer, eval_unary_integer, floating_literal_to_integer, integer_literal_value,
    integer_type_for_rank, rank_for_integer_type, usual_arithmetic_type, CInteger, CIntegerType,
    IntegerError,
};
use evocc_chir::{BinaryOp, FloatLit, FloatSuffix, IntBase, IntLit, IntSuffix, UnaryOp};
use evocc_types::IntRank;

fn integer(raw: u64, ty: CIntegerType) -> CInteger {
    CInteger::new(raw, ty)
}

#[test]
fn aarch64_gnu_integer_model_is_explicit() {
    assert_eq!(CIntegerType::BOOL.bits(), 1);
    assert_eq!(CIntegerType::CHAR, CIntegerType::UCHAR);
    assert!(!CIntegerType::CHAR.is_signed());
    assert_eq!(CIntegerType::SHORT.bits(), 16);
    assert_eq!(CIntegerType::INT.bits(), 32);
    assert_eq!(CIntegerType::LONG.bits(), 64);
    assert_eq!(CIntegerType::LONG_LONG.bits(), 64);
    assert!(CIntegerType::LONG.is_signed());
    assert!(!CIntegerType::ULONG.is_signed());

    for rank in [
        IntRank::Bool,
        IntRank::Char,
        IntRank::SChar,
        IntRank::UChar,
        IntRank::Short,
        IntRank::UShort,
        IntRank::Int,
        IntRank::UInt,
        IntRank::Long,
        IntRank::ULong,
        IntRank::LongLong,
        IntRank::ULongLong,
    ] {
        let ty = integer_type_for_rank(rank);
        let canonical_rank = rank_for_integer_type(ty).expect("target type has a C rank");
        if rank == IntRank::Char {
            assert_eq!(canonical_rank, IntRank::UChar);
        } else {
            assert_eq!(canonical_rank, rank);
        }
    }
}

#[test]
fn literal_typing_uses_c_candidate_lists_not_host_integer_rules() {
    let decimal = integer_literal_value(IntLit {
        value: 2_147_483_648,
        suffix: IntSuffix::None,
        base: IntBase::Dec,
    })
    .unwrap();
    assert_eq!(decimal.ty(), CIntegerType::LONG);

    let hexadecimal = integer_literal_value(IntLit {
        value: 0x8000_0000,
        suffix: IntSuffix::None,
        base: IntBase::Hex,
    })
    .unwrap();
    assert_eq!(hexadecimal.ty(), CIntegerType::UINT);

    let max_hex = integer_literal_value(IntLit {
        value: u64::MAX,
        suffix: IntSuffix::None,
        base: IntBase::Hex,
    })
    .unwrap();
    assert_eq!(max_hex.ty(), CIntegerType::ULONG);

    assert_eq!(
        integer_literal_value(IntLit {
            value: u64::MAX,
            suffix: IntSuffix::None,
            base: IntBase::Dec,
        }),
        Err(IntegerError::LiteralOutOfRange)
    );
}

#[test]
fn promotions_usual_arithmetic_conversions_and_casts_are_target_typed() {
    assert_eq!(
        integer(255, CIntegerType::UCHAR).promoted().ty(),
        CIntegerType::INT
    );
    assert_eq!(
        usual_arithmetic_type(CIntegerType::LONG, CIntegerType::UINT),
        CIntegerType::LONG
    );
    assert_eq!(
        usual_arithmetic_type(CIntegerType::LONG_LONG, CIntegerType::ULONG),
        CIntegerType::ULONG_LONG
    );

    let minus_one = CInteger::int(-1);
    assert_eq!(
        minus_one.cast(CIntegerType::UINT).raw_bits(),
        u128::from(u32::MAX)
    );
    assert_eq!(
        integer(257, CIntegerType::UINT)
            .cast(CIntegerType::UCHAR)
            .raw_bits(),
        1
    );
    assert_eq!(minus_one.cast(CIntegerType::BOOL).raw_bits(), 1);
}

#[test]
fn checked_arithmetic_distinguishes_runtime_failure_classes() {
    assert_eq!(
        eval_binary_integer(
            BinaryOp::Add,
            integer(i32::MAX as u64, CIntegerType::INT),
            CInteger::int(1),
        ),
        Err(IntegerError::SignedOverflow)
    );
    assert_eq!(
        eval_binary_integer(
            BinaryOp::Add,
            integer(u32::MAX.into(), CIntegerType::UINT),
            integer(1, CIntegerType::UINT),
        )
        .unwrap()
        .raw_bits(),
        0
    );
    assert_eq!(
        eval_binary_integer(BinaryOp::Div, CInteger::int(1), CInteger::int(0)),
        Err(IntegerError::DivisionByZero)
    );
    assert_eq!(
        eval_binary_integer(BinaryOp::Mod, CInteger::int(1), CInteger::int(0)),
        Err(IntegerError::RemainderByZero)
    );
    assert_eq!(
        eval_binary_integer(
            BinaryOp::Div,
            integer(i32::MIN as u64, CIntegerType::INT),
            CInteger::int(-1),
        ),
        Err(IntegerError::SignedOverflow)
    );
    assert_eq!(
        eval_binary_integer(
            BinaryOp::Mod,
            integer(i32::MIN as u64, CIntegerType::INT),
            CInteger::int(-1),
        ),
        Err(IntegerError::SignedOverflow)
    );
}

#[test]
fn checked_shifts_do_not_inherit_host_shift_or_overflow_behavior() {
    for count in [CInteger::int(-1), CInteger::int(32)] {
        assert_eq!(
            eval_binary_integer(BinaryOp::Shl, CInteger::int(1), count),
            Err(IntegerError::InvalidShiftCount)
        );
    }
    assert_eq!(
        eval_binary_integer(BinaryOp::Shl, CInteger::int(-1), CInteger::int(1)),
        Ok(CInteger::int(-2))
    );
    assert_eq!(
        eval_binary_integer(BinaryOp::Shl, CInteger::int(1), CInteger::int(31)),
        Ok(CInteger::int(i32::MIN))
    );
    assert_eq!(
        eval_binary_integer(
            BinaryOp::Shl,
            integer(0x8000_0000, CIntegerType::UINT),
            CInteger::int(1),
        )
        .unwrap()
        .raw_bits(),
        0
    );
    assert_eq!(
        eval_binary_integer(BinaryOp::Shr, CInteger::int(-4), CInteger::int(1))
            .unwrap()
            .mathematical_value(),
        -2
    );
}

#[test]
fn unary_and_float_cast_edges_are_checked_by_the_same_kernel() {
    assert_eq!(
        eval_unary_integer(UnaryOp::Neg, integer(i32::MIN as u64, CIntegerType::INT)),
        Err(IntegerError::SignedOverflow)
    );
    assert_eq!(
        eval_unary_integer(UnaryOp::Neg, integer(1, CIntegerType::UINT))
            .unwrap()
            .raw_bits(),
        u128::from(u32::MAX)
    );
    assert_eq!(
        floating_literal_to_integer(
            FloatLit {
                value: -0.25,
                suffix: FloatSuffix::None,
            },
            CIntegerType::BOOL,
        )
        .unwrap()
        .raw_bits(),
        1
    );
    assert_eq!(
        floating_literal_to_integer(
            FloatLit {
                value: f64::INFINITY,
                suffix: FloatSuffix::None,
            },
            CIntegerType::INT,
        ),
        Err(IntegerError::ConversionOutOfRange)
    );
}
