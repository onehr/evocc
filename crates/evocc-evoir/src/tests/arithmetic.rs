//! EvoIR arithmetic unit tests.

use super::*;

#[test]
fn signed_definedness_and_ub_assume_have_typed_runtime_semantics() {
    let ten = bits(0, 8);
    let one = bits(1, 8);
    let add_ok = bits(2, 1);
    let sum = bits(3, 8);
    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "defined_add".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(8)],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(ten.clone(), 10),
                Inst::Const(one.clone(), 1),
                Inst::SignedAddDefined(add_ok.clone(), ten.clone(), one.clone()),
                Inst::UbAssume(add_ok, UbReason::SignedOverflow),
                Inst::Add(sum.clone(), ten, one),
            ],
            terminator: Inst::Ret(vec![sum]),
        }],
        param_marks: Vec::new(),
    };
    let outcome =
        eval_function(&function, &[], InterpreterLimits::new(6)).expect("defined signed addition");
    assert_eq!(outcome.return_values, vec![RuntimeValue::bits(8, 11)]);

    let mut wrong_predicate_type = function.clone();
    let Inst::SignedAddDefined(result, ..) = &mut wrong_predicate_type.blocks[0].insts[2] else {
        panic!("fixture definedness predicate");
    };
    result.ty = EvoIRType::Bits(2);
    let Inst::UbAssume(condition, _) = &mut wrong_predicate_type.blocks[0].insts[3] else {
        panic!("fixture ub.assume");
    };
    condition.ty = EvoIRType::Bits(2);
    assert!(validate_ssa(&wrong_predicate_type)
        .expect_err("definedness result must be bits<1>")
        .contains("signed-definedness result"));

    let mut wrong_operand_type = function.clone();
    let Inst::Const(right, _) = &mut wrong_operand_type.blocks[0].insts[1] else {
        panic!("fixture constant");
    };
    right.ty = EvoIRType::Bits(16);
    let Inst::SignedAddDefined(_, _, right) = &mut wrong_operand_type.blocks[0].insts[2] else {
        panic!("fixture definedness predicate");
    };
    right.ty = EvoIRType::Bits(16);
    assert!(validate_ssa(&wrong_operand_type)
        .expect_err("definedness operands must have equal width")
        .contains("different types"));

    let mut wrong_assumption_type = function.clone();
    let Inst::Const(condition, _) = &wrong_assumption_type.blocks[0].insts[0] else {
        panic!("fixture constant");
    };
    wrong_assumption_type.blocks[0].insts[3] =
        Inst::UbAssume(condition.clone(), UbReason::SignedOverflow);
    assert!(validate_ssa(&wrong_assumption_type)
        .expect_err("ub.assume condition must be bits<1>")
        .contains("ub.assume"));

    let max = bits(10, 8);
    let one = bits(11, 8);
    let add_ok = bits(12, 1);
    let overflow = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "overflow".to_owned(),
        params: vec![],
        ret_tys: vec![],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(max.clone(), 0x7f),
                Inst::Const(one.clone(), 1),
                Inst::SignedAddDefined(add_ok.clone(), max, one),
                Inst::UbAssume(add_ok, UbReason::SignedOverflow),
            ],
            terminator: Inst::Ret(vec![]),
        }],
        param_marks: Vec::new(),
    };
    assert!(matches!(
        eval_function(&overflow, &[], InterpreterLimits::new(5)),
        Err(InterpreterError::UndefinedBehavior {
            diagnostic: "E3004",
            ..
        })
    ));

    let left = bits(20, 8);
    let right = bits(21, 8);
    let sub_ok = bits(22, 1);
    let mul_ok = bits(23, 1);
    let predicates = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "predicates".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(1), EvoIRType::Bits(1)],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(left.clone(), 0x80),
                Inst::Const(right.clone(), 2),
                Inst::SignedSubDefined(sub_ok.clone(), left.clone(), right.clone()),
                Inst::SignedMulDefined(mul_ok.clone(), left, right),
            ],
            terminator: Inst::Ret(vec![sub_ok, mul_ok]),
        }],
        param_marks: Vec::new(),
    };
    let outcome = eval_function(&predicates, &[], InterpreterLimits::new(5))
        .expect("definedness predicates execute");
    assert_eq!(
        outcome.return_values,
        vec![RuntimeValue::bits(1, 0), RuntimeValue::bits(1, 0)]
    );
}

#[test]
fn extended_integer_ops_and_separated_ub_predicates_execute() {
    let neg_seven = bits(0, 8);
    let three = bits(1, 8);
    let seven = bits(2, 8);
    let two = bits(3, 8);
    let neg_four = bits(4, 8);
    let five = bits(5, 8);
    let wide_invalid_count = bits(6, 16);
    let srem = bits(7, 8);
    let urem = bits(8, 8);
    let shl = bits(9, 8);
    let ashr = bits(10, 8);
    let lshr = bits(11, 8);
    let and = bits(12, 8);
    let or = bits(13, 8);
    let xor = bits(14, 8);
    let invalid_shl = bits(15, 8);
    let operations = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "extended_integer_ops".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(8); 9],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(neg_seven.clone(), 0xf9),
                Inst::Const(three.clone(), 3),
                Inst::Const(seven.clone(), 7),
                Inst::Const(two.clone(), 2),
                Inst::Const(neg_four.clone(), 0xfc),
                Inst::Const(five.clone(), 5),
                Inst::Const(wide_invalid_count.clone(), 256),
                Inst::SRem(srem.clone(), neg_seven, three.clone()),
                Inst::URem(urem.clone(), seven, three.clone()),
                Inst::Shl(shl.clone(), three.clone(), two.clone()),
                Inst::AShr(ashr.clone(), neg_four.clone(), two.clone()),
                Inst::LShr(lshr.clone(), neg_four, two),
                Inst::And(and.clone(), five.clone(), three.clone()),
                Inst::Or(or.clone(), five.clone(), three.clone()),
                Inst::Xor(xor.clone(), five, three),
                Inst::Shl(invalid_shl.clone(), shl.clone(), wide_invalid_count),
            ],
            terminator: Inst::Ret(vec![srem, urem, shl, ashr, lshr, and, or, xor, invalid_shl]),
        }],
        param_marks: Vec::new(),
    };
    let outcome = eval_function(&operations, &[], InterpreterLimits::new(17))
        .expect("extended integer operations execute");
    assert_eq!(
        outcome.return_values,
        vec![
            RuntimeValue::bits(8, 0xff),
            RuntimeValue::bits(8, 1),
            RuntimeValue::bits(8, 12),
            RuntimeValue::bits(8, 0xff),
            RuntimeValue::bits(8, 0x3f),
            RuntimeValue::bits(8, 1),
            RuntimeValue::bits(8, 7),
            RuntimeValue::bits(8, 6),
            RuntimeValue::bits(8, 0),
        ]
    );

    let zero = bits(20, 8);
    let one = bits(21, 8);
    let minimum = bits(22, 8);
    let minus_one = bits(23, 8);
    let minus_one_count = bits(24, 8);
    let width_count = bits(25, 8);
    let six = bits(26, 8);
    let seven = bits(27, 8);
    let predicates: Vec<Value> = (30..=40).map(|id| bits(id, 1)).collect();
    let predicate_function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "separated_predicates".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(1); predicates.len()],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(zero.clone(), 0),
                Inst::Const(one.clone(), 1),
                Inst::Const(minimum.clone(), 0x80),
                Inst::Const(minus_one.clone(), 0xff),
                Inst::Const(minus_one_count.clone(), 0xff),
                Inst::Const(width_count.clone(), 8),
                Inst::Const(six.clone(), 6),
                Inst::Const(seven.clone(), 7),
                Inst::DivisorNonZero(predicates[0].clone(), zero),
                Inst::DivisorNonZero(predicates[1].clone(), one.clone()),
                Inst::SignedDivRemNoOverflow(
                    predicates[2].clone(),
                    minimum.clone(),
                    minus_one.clone(),
                ),
                Inst::SignedDivRemNoOverflow(predicates[3].clone(), minimum, one.clone()),
                Inst::ShiftCountDefined {
                    result: predicates[4].clone(),
                    shifted: one.clone(),
                    amount: minus_one_count,
                    amount_signed: true,
                },
                Inst::ShiftCountDefined {
                    result: predicates[5].clone(),
                    shifted: one.clone(),
                    amount: width_count.clone(),
                    amount_signed: false,
                },
                Inst::ShiftCountDefined {
                    result: predicates[6].clone(),
                    shifted: one.clone(),
                    amount: six.clone(),
                    amount_signed: true,
                },
                Inst::SignedShlDefined {
                    result: predicates[7].clone(),
                    shifted: minus_one,
                    amount: one.clone(),
                    amount_signed: true,
                },
                Inst::SignedShlDefined {
                    result: predicates[8].clone(),
                    shifted: one.clone(),
                    amount: seven,
                    amount_signed: true,
                },
                Inst::SignedShlDefined {
                    result: predicates[9].clone(),
                    shifted: one.clone(),
                    amount: six,
                    amount_signed: true,
                },
                Inst::SignedShlDefined {
                    result: predicates[10].clone(),
                    shifted: one,
                    amount: width_count,
                    amount_signed: true,
                },
            ],
            terminator: Inst::Ret(predicates.clone()),
        }],
        param_marks: Vec::new(),
    };
    let outcome = eval_function(&predicate_function, &[], InterpreterLimits::new(20))
        .expect("separated UB predicates execute without host panics");
    assert_eq!(
        outcome.return_values,
        [false, true, false, true, false, false, true, false, false, true, false]
            .into_iter()
            .map(|value| RuntimeValue::bits(1, u64::from(value)))
            .collect::<Vec<_>>()
    );

    let mut mixed_shift_width = predicate_function;
    let Inst::Const(amount, _) = &mut mixed_shift_width.blocks[0].insts[4] else {
        panic!("fixture shift amount");
    };
    amount.ty = EvoIRType::Bits(16);
    let Inst::ShiftCountDefined { amount, .. } = &mut mixed_shift_width.blocks[0].insts[12] else {
        panic!("fixture shift predicate");
    };
    amount.ty = EvoIRType::Bits(16);
    validate_ssa(&mixed_shift_width)
        .expect("shifted value and promoted amount may have different widths");

    let shifted = bits(50, 64);
    let negative_narrow_count = bits(51, 1);
    let sixty_two = bits(52, 8);
    let sixty_three = bits(53, 8);
    let mixed_predicates: Vec<Value> = (54..=58).map(|id| bits(id, 1)).collect();
    let shifted_result = bits(59, 64);
    let mixed_width_shifts = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "mixed_width_shifts".to_owned(),
        params: vec![],
        ret_tys: vec![
            EvoIRType::Bits(64),
            EvoIRType::Bits(1),
            EvoIRType::Bits(1),
            EvoIRType::Bits(1),
            EvoIRType::Bits(1),
            EvoIRType::Bits(1),
        ],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(shifted.clone(), 1),
                Inst::Const(negative_narrow_count.clone(), 1),
                Inst::Const(sixty_two.clone(), 62),
                Inst::Const(sixty_three.clone(), 63),
                Inst::ShiftCountDefined {
                    result: mixed_predicates[0].clone(),
                    shifted: shifted.clone(),
                    amount: negative_narrow_count.clone(),
                    amount_signed: true,
                },
                Inst::ShiftCountDefined {
                    result: mixed_predicates[1].clone(),
                    shifted: shifted.clone(),
                    amount: sixty_three.clone(),
                    amount_signed: false,
                },
                Inst::SignedShlDefined {
                    result: mixed_predicates[2].clone(),
                    shifted: shifted.clone(),
                    amount: negative_narrow_count,
                    amount_signed: true,
                },
                Inst::SignedShlDefined {
                    result: mixed_predicates[3].clone(),
                    shifted: shifted.clone(),
                    amount: sixty_two.clone(),
                    amount_signed: true,
                },
                Inst::SignedShlDefined {
                    result: mixed_predicates[4].clone(),
                    shifted: shifted.clone(),
                    amount: sixty_three,
                    amount_signed: true,
                },
                Inst::Shl(shifted_result.clone(), shifted, sixty_two),
            ],
            terminator: Inst::Ret(
                std::iter::once(shifted_result.clone())
                    .chain(mixed_predicates.iter().cloned())
                    .collect(),
            ),
        }],
        param_marks: Vec::new(),
    };
    let outcome = eval_function(&mixed_width_shifts, &[], InterpreterLimits::new(11))
        .expect("mixed-width shift predicates and operation execute");
    assert_eq!(
        outcome.return_values,
        vec![
            RuntimeValue::bits(64, 1_u64 << 62),
            RuntimeValue::bits(1, 0),
            RuntimeValue::bits(1, 1),
            RuntimeValue::bits(1, 0),
            RuntimeValue::bits(1, 1),
            RuntimeValue::bits(1, 0),
        ]
    );
}

#[test]
fn ub_assume_preserves_each_typed_integer_diagnostic() {
    let zero = bits(0, 8);
    let division_condition = bits(1, 1);
    let remainder_condition = bits(2, 1);
    let minimum = bits(3, 8);
    let minus_one = bits(4, 8);
    let overflow_condition = bits(5, 1);
    let shifted_wide = bits(6, 64);
    let negative_narrow_count = bits(7, 1);
    let count_condition = bits(8, 1);
    let shifted_narrow = bits(9, 8);
    let seven = bits(10, 8);
    let signed_shift_condition = bits(11, 1);
    let cases = vec![
        (
            vec![
                Inst::Const(zero.clone(), 0),
                Inst::DivisorNonZero(division_condition.clone(), zero.clone()),
            ],
            division_condition,
            UbReason::DivisionByZero,
            "division_by_zero",
            "E3002",
            "integer-division-by-zero",
        ),
        (
            vec![
                Inst::Const(zero.clone(), 0),
                Inst::DivisorNonZero(remainder_condition.clone(), zero),
            ],
            remainder_condition,
            UbReason::RemainderByZero,
            "remainder_by_zero",
            "E3003",
            "integer-remainder-by-zero",
        ),
        (
            vec![
                Inst::Const(minimum.clone(), 0x80),
                Inst::Const(minus_one.clone(), 0xff),
                Inst::SignedDivRemNoOverflow(overflow_condition.clone(), minimum, minus_one),
            ],
            overflow_condition,
            UbReason::SignedOverflow,
            "signed_overflow",
            "E3004",
            "signed-integer-overflow",
        ),
        (
            vec![
                Inst::Const(shifted_wide.clone(), 1),
                Inst::Const(negative_narrow_count.clone(), 1),
                Inst::ShiftCountDefined {
                    result: count_condition.clone(),
                    shifted: shifted_wide,
                    amount: negative_narrow_count,
                    amount_signed: true,
                },
            ],
            count_condition,
            UbReason::InvalidShiftCount,
            "invalid_shift_count",
            "E3005",
            "invalid-shift-count",
        ),
        (
            vec![
                Inst::Const(shifted_narrow.clone(), 1),
                Inst::Const(seven.clone(), 7),
                Inst::SignedShlDefined {
                    result: signed_shift_condition.clone(),
                    shifted: shifted_narrow,
                    amount: seven,
                    amount_signed: true,
                },
            ],
            signed_shift_condition,
            UbReason::InvalidSignedLeftShift,
            "invalid_signed_left_shift",
            "E3006",
            "invalid-signed-left-shift",
        ),
    ];

    for (
        mut instructions,
        condition,
        reason,
        expected_mnemonic,
        expected_diagnostic,
        expected_message,
    ) in cases
    {
        assert_eq!(reason.mnemonic(), expected_mnemonic);
        instructions.push(Inst::UbAssume(condition, reason));
        let budget = instructions.len() as u64 + 1;
        let function = Function {
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
            name: "typed_ub".to_owned(),
            params: vec![],
            ret_tys: vec![],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: instructions,
                terminator: Inst::Ret(vec![]),
            }],
            param_marks: Vec::new(),
        };
        let error = eval_function(&function, &[], InterpreterLimits::new(budget))
            .expect_err("a false typed UB obligation must fail");
        assert_eq!(error.code(), expected_diagnostic);
        assert_eq!(error.message(), expected_message);
        let InterpreterError::UndefinedBehavior {
            block,
            diagnostic,
            message,
        } = error
        else {
            panic!("unexpected typed UB failure: {error:?}");
        };
        assert_eq!(block, 0);
        assert_eq!(diagnostic, expected_diagnostic);
        assert_eq!(message, expected_message);
    }
}
