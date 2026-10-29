//! EvoIR uninitialized unit tests.

use super::*;

#[test]
fn uninitialized_is_an_ordinary_typed_definition_with_no_operands() {
    let result = bits(70, 32);
    let instruction = Inst::Uninitialized(result.clone());
    assert_eq!(inst_results(&instruction), vec![&result]);
    assert!(inst_uses(&instruction).is_empty());

    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "uninitialized_scalar".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![Block {
            id: 37,
            args: vec![],
            insts: vec![instruction],
            terminator: Inst::Ret(vec![result]),
        }],
        param_marks: Vec::new(),
    };
    validate_ssa(&function).expect("ordinary typed uninitialized values are valid SSA");

    let pointer = value(71, EvoIRType::Ptr(AddressSpace::Stack));
    let pointer_function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "uninitialized_pointer".to_owned(),
        params: vec![],
        ret_tys: vec![],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![Inst::Uninitialized(pointer)],
            terminator: Inst::Ret(vec![]),
        }],
        param_marks: Vec::new(),
    };
    validate_ssa(&pointer_function)
        .expect("an ordinary pointer is a supported uninitialized value type");

    for forbidden in [
        EvoIRType::Mem(Region::Stack),
        EvoIRType::World,
        EvoIRType::Tuple(vec![
            EvoIRType::Bits(32),
            EvoIRType::Tuple(vec![EvoIRType::Mem(Region::Heap)]),
        ]),
    ] {
        let invalid = Function {
            section: None,
            patchable_entry: None,
            stack_protect: false,
            alignment: 4,
            linkage: crate::Linkage::Strong,
            visibility: crate::GlobalVisibility::External,
            variadic: false,
            name: "uninitialized_effect_token".to_owned(),
            params: vec![],
            ret_tys: vec![],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![Inst::Uninitialized(value(80, forbidden))],
                terminator: Inst::Ret(vec![]),
            }],
            param_marks: Vec::new(),
        };
        let error = validate_ssa(&invalid)
            .expect_err("effect tokens cannot masquerade as uninitialized ordinary data");
        assert!(
            error.contains("uninitialized result") && error.contains("memory/world effect token"),
            "unexpected validator error: {error}"
        );
    }

    let invalid_void = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "uninitialized_void".to_owned(),
        params: vec![],
        ret_tys: vec![],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![Inst::Uninitialized(value(90, EvoIRType::Void))],
            terminator: Inst::Ret(vec![]),
        }],
        param_marks: Vec::new(),
    };
    reject(&invalid_void, "void is not an SSA value type");
}

#[test]
fn uninitialized_propagates_across_block_arguments_without_being_read() {
    let poison = bits(100, 32);
    let carried = bits(101, 32);
    let replacement = bits(102, 32);
    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "propagate_uninitialized".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![],
                insts: vec![Inst::Uninitialized(poison.clone())],
                terminator: Inst::Br(1, vec![poison]),
            },
            Block {
                id: 1,
                args: vec![argument("carried", carried)],
                insts: vec![Inst::Const(replacement.clone(), 7)],
                terminator: Inst::Ret(vec![replacement]),
            },
        ],
        param_marks: Vec::new(),
    };

    let outcome = interpret_function(&function, &[], InterpreterLimits::new(4))
        .expect("copying poison through a block argument is not a semantic read");
    assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 7)]);
    assert_eq!(outcome.steps, 4);

    let module_outcome = interpret_module(
        &Module {
            tls: Vec::new(),
            lifecycle: Vec::new(),
            aliases: Vec::new(),
            assembly: Vec::new(),
            extern_objects: Vec::new(),
            declares: Vec::new(),
            globals: vec![],
            functions: vec![function.clone()],
        },
        "propagate_uninitialized",
        &[],
        InterpreterLimits::new(4),
    )
    .expect("module block-argument transfer preserves poison without reading it");
    assert_eq!(
        module_outcome.return_values,
        vec![RuntimeValue::bits(32, 7)]
    );
    assert_eq!(module_outcome.steps, 4);

    let mut consumed = function;
    consumed.name = "consume_propagated_uninitialized".to_owned();
    let carried = consumed.blocks[1].args[0].value.clone();
    consumed.blocks[1].terminator = Inst::Ret(vec![carried]);
    assert_eq!(
        interpret_function(&consumed, &[], InterpreterLimits::new(4)),
        Err(InterpreterError::UninitializedRead { block: 1 })
    );
    assert_eq!(
        interpret_module(
            &Module {
                tls: Vec::new(),
                lifecycle: Vec::new(),
                aliases: Vec::new(),
                assembly: Vec::new(),
                extern_objects: Vec::new(),
                declares: Vec::new(),
                globals: vec![],
                functions: vec![consumed],
            },
            "consume_propagated_uninitialized",
            &[],
            InterpreterLimits::new(4),
        ),
        Err(InterpreterError::UninitializedRead { block: 1 })
    );
}

#[test]
fn loop_carried_uninitialized_value_can_be_definitely_assigned_before_read() {
    let initial_x = bits(110, 32);
    let initial_i = bits(111, 32);
    let header_x = bits(112, 32);
    let header_i = bits(113, 32);
    let limit = bits(114, 32);
    let condition = bits(115, 1);
    let body_x = bits(116, 32);
    let body_i = bits(117, 32);
    let assigned_x = bits(118, 32);
    let one = bits(119, 32);
    let next_i = bits(120, 32);
    let exit_x = bits(121, 32);
    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "definite_assignment_loop".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Uninitialized(initial_x.clone()),
                    Inst::Const(initial_i.clone(), 0),
                ],
                terminator: Inst::Br(1, vec![initial_x, initial_i]),
            },
            Block {
                id: 1,
                args: vec![
                    argument("x", header_x.clone()),
                    argument("i", header_i.clone()),
                ],
                insts: vec![
                    Inst::Const(limit.clone(), 1),
                    Inst::ICmp(condition.clone(), CmpOp::ULt, header_i.clone(), limit),
                ],
                terminator: Inst::CondBr(
                    condition,
                    2,
                    vec![header_x.clone(), header_i],
                    3,
                    vec![header_x],
                ),
            },
            Block {
                id: 2,
                args: vec![argument("x", body_x), argument("i", body_i.clone())],
                insts: vec![
                    Inst::Const(assigned_x.clone(), 7),
                    Inst::Const(one.clone(), 1),
                    Inst::Add(next_i.clone(), body_i, one),
                ],
                terminator: Inst::Br(1, vec![assigned_x, next_i]),
            },
            Block {
                id: 3,
                args: vec![argument("x", exit_x.clone())],
                insts: vec![],
                terminator: Inst::Ret(vec![exit_x]),
            },
        ],
        param_marks: Vec::new(),
    };

    let outcome = interpret_function(&function, &[], InterpreterLimits::new(14))
        .expect("the loop assigns x before the only semantic read at return");
    assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 7)]);
    assert_eq!(outcome.steps, 14);

    let module_outcome = interpret_module(
        &Module {
            tls: Vec::new(),
            lifecycle: Vec::new(),
            aliases: Vec::new(),
            assembly: Vec::new(),
            extern_objects: Vec::new(),
            declares: Vec::new(),
            globals: vec![],
            functions: vec![function],
        },
        "definite_assignment_loop",
        &[],
        InterpreterLimits::new(14),
    )
    .expect("module loop execution overwrites poison before the return read");
    assert_eq!(
        module_outcome.return_values,
        vec![RuntimeValue::bits(32, 7)]
    );
    assert_eq!(module_outcome.steps, 14);
}

#[test]
fn uninitialized_return_traps_with_e3001_in_function_and_module_interpreters() {
    let result = bits(100, 32);
    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "uninitialized_read".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![Block {
            id: 37,
            args: vec![],
            insts: vec![Inst::Uninitialized(result.clone())],
            terminator: Inst::Ret(vec![result]),
        }],
        param_marks: Vec::new(),
    };

    assert!(matches!(
        interpret_function(&function, &[], InterpreterLimits::new(1)),
        Err(InterpreterError::StepBudgetExceeded {
            budget: 1,
            steps: 1,
            block: 37,
        })
    ));
    let function_error = interpret_function(&function, &[], InterpreterLimits::new(2))
        .expect_err("returning an uninitialized ordinary result is a semantic read");
    assert_eq!(
        function_error,
        InterpreterError::UninitializedRead { block: 37 }
    );
    assert_eq!(function_error.code(), "E3001");
    assert_eq!(function_error.message(), "read-of-uninitialized-object");

    let module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![function],
    };
    assert!(matches!(
        interpret_module(
            &module,
            "uninitialized_read",
            &[],
            InterpreterLimits::new(1),
        ),
        Err(InterpreterError::StepBudgetExceeded {
            budget: 1,
            steps: 1,
            block: 37,
        })
    ));
    let module_error = interpret_module(
        &module,
        "uninitialized_read",
        &[],
        InterpreterLimits::new(2),
    )
    .expect_err("module execution must preserve the typed uninitialized-read trap");
    assert_eq!(
        module_error,
        InterpreterError::UninitializedRead { block: 37 }
    );
    assert_eq!(module_error.code(), "E3001");
    assert_eq!(module_error.message(), "read-of-uninitialized-object");
}

#[test]
fn arithmetic_comparison_cast_condition_and_call_consume_uninitialized_values() {
    let poison = bits(130, 32);
    let one = bits(131, 32);
    let arithmetic_result = bits(132, 32);
    let comparison_result = bits(133, 1);
    let cast_result = bits(134, 64);
    let zext_source = bits(135, 8);
    let zext_result = bits(136, 32);
    let trunc_source = bits(137, 32);
    let trunc_result = bits(138, 8);
    let shifted_poison = bits(139, 32);
    let shift_amount = bits(140, 32);
    let shift_predicate = bits(141, 1);
    let cases = vec![
        (
            Function {
                section: None,
                patchable_entry: None,
                stack_protect: false,
                alignment: 4,
                linkage: crate::Linkage::Strong,
                visibility: crate::GlobalVisibility::External,
                variadic: false,
                name: "uninitialized_arithmetic".to_owned(),
                params: vec![],
                ret_tys: vec![EvoIRType::Bits(32)],
                effects: Default::default(),
                blocks: vec![Block {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        Inst::Uninitialized(poison.clone()),
                        Inst::Const(one.clone(), 1),
                        Inst::Add(arithmetic_result.clone(), poison.clone(), one.clone()),
                    ],
                    terminator: Inst::Ret(vec![arithmetic_result]),
                }],
                param_marks: Vec::new(),
            },
            3,
        ),
        (
            Function {
                section: None,
                patchable_entry: None,
                stack_protect: false,
                alignment: 4,
                linkage: crate::Linkage::Strong,
                visibility: crate::GlobalVisibility::External,
                variadic: false,
                name: "uninitialized_comparison".to_owned(),
                params: vec![],
                ret_tys: vec![EvoIRType::Bits(1)],
                effects: Default::default(),
                blocks: vec![Block {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        Inst::Uninitialized(poison.clone()),
                        Inst::Const(one.clone(), 1),
                        Inst::ICmp(comparison_result.clone(), CmpOp::Eq, poison.clone(), one),
                    ],
                    terminator: Inst::Ret(vec![comparison_result]),
                }],
                param_marks: Vec::new(),
            },
            3,
        ),
        (
            Function {
                section: None,
                patchable_entry: None,
                stack_protect: false,
                alignment: 4,
                linkage: crate::Linkage::Strong,
                visibility: crate::GlobalVisibility::External,
                variadic: false,
                name: "uninitialized_cast".to_owned(),
                params: vec![],
                ret_tys: vec![EvoIRType::Bits(64)],
                effects: Default::default(),
                blocks: vec![Block {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        Inst::Uninitialized(poison.clone()),
                        Inst::SExt(cast_result.clone(), poison, EvoIRType::Bits(64)),
                    ],
                    terminator: Inst::Ret(vec![cast_result]),
                }],
                param_marks: Vec::new(),
            },
            2,
        ),
        (
            Function {
                section: None,
                patchable_entry: None,
                stack_protect: false,
                alignment: 4,
                linkage: crate::Linkage::Strong,
                visibility: crate::GlobalVisibility::External,
                variadic: false,
                name: "uninitialized_zero_extend".to_owned(),
                params: vec![],
                ret_tys: vec![EvoIRType::Bits(32)],
                effects: Default::default(),
                blocks: vec![Block {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        Inst::Uninitialized(zext_source.clone()),
                        Inst::ZExt(zext_result.clone(), zext_source, EvoIRType::Bits(32)),
                    ],
                    terminator: Inst::Ret(vec![zext_result]),
                }],
                param_marks: Vec::new(),
            },
            2,
        ),
        (
            Function {
                section: None,
                patchable_entry: None,
                stack_protect: false,
                alignment: 4,
                linkage: crate::Linkage::Strong,
                visibility: crate::GlobalVisibility::External,
                variadic: false,
                name: "uninitialized_truncate".to_owned(),
                params: vec![],
                ret_tys: vec![EvoIRType::Bits(8)],
                effects: Default::default(),
                blocks: vec![Block {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        Inst::Uninitialized(trunc_source.clone()),
                        Inst::Trunc(trunc_result.clone(), trunc_source, EvoIRType::Bits(8)),
                    ],
                    terminator: Inst::Ret(vec![trunc_result]),
                }],
                param_marks: Vec::new(),
            },
            2,
        ),
        (
            Function {
                section: None,
                patchable_entry: None,
                stack_protect: false,
                alignment: 4,
                linkage: crate::Linkage::Strong,
                visibility: crate::GlobalVisibility::External,
                variadic: false,
                name: "uninitialized_shifted_predicate_operand".to_owned(),
                params: vec![],
                ret_tys: vec![EvoIRType::Bits(1)],
                effects: Default::default(),
                blocks: vec![Block {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        Inst::Uninitialized(shifted_poison.clone()),
                        Inst::Const(shift_amount.clone(), 1),
                        Inst::ShiftCountDefined {
                            result: shift_predicate.clone(),
                            shifted: shifted_poison,
                            amount: shift_amount,
                            amount_signed: false,
                        },
                    ],
                    terminator: Inst::Ret(vec![shift_predicate]),
                }],
                param_marks: Vec::new(),
            },
            3,
        ),
    ];
    for (function, consumer_step) in &cases {
        assert!(matches!(
            interpret_function(function, &[], InterpreterLimits::new(consumer_step - 1),),
            Err(InterpreterError::StepBudgetExceeded { .. })
        ));
        let error = interpret_function(function, &[], InterpreterLimits::new(*consumer_step))
            .expect_err("semantic instruction operands must reject uninitialized values");
        assert_eq!(error, InterpreterError::UninitializedRead { block: 0 });
        assert_eq!(error.code(), "E3001");
        assert_eq!(error.message(), "read-of-uninitialized-object");
    }

    let poison_condition = bits(150, 1);
    let condition_function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "uninitialized_condition".to_owned(),
        params: vec![],
        ret_tys: vec![],
        effects: Default::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![],
                insts: vec![Inst::Uninitialized(poison_condition.clone())],
                terminator: Inst::CondBr(poison_condition, 1, vec![], 2, vec![]),
            },
            Block {
                id: 1,
                args: vec![],
                insts: vec![],
                terminator: Inst::Ret(vec![]),
            },
            Block {
                id: 2,
                args: vec![],
                insts: vec![],
                terminator: Inst::Ret(vec![]),
            },
        ],
        param_marks: Vec::new(),
    };
    assert!(matches!(
        interpret_function(&condition_function, &[], InterpreterLimits::new(1)),
        Err(InterpreterError::StepBudgetExceeded { .. })
    ));
    assert_eq!(
        interpret_function(&condition_function, &[], InterpreterLimits::new(2)),
        Err(InterpreterError::UninitializedRead { block: 0 })
    );

    let mut call_module = direct_call_module();
    let main = call_module
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .expect("fixture main");
    let argument = match &main.blocks[0].insts[0] {
        Inst::Const(argument, _) => argument.clone(),
        _ => panic!("fixture ordinary call argument"),
    };
    main.blocks[0].insts[0] = Inst::Uninitialized(argument);
    let entry_args = [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 10),
        RuntimeValue::new(EvoIRType::World, 20),
    ];
    assert!(matches!(
        interpret_module(&call_module, "main", &entry_args, InterpreterLimits::new(1),),
        Err(InterpreterError::StepBudgetExceeded { .. })
    ));
    let error = interpret_module(&call_module, "main", &entry_args, InterpreterLimits::new(2))
        .expect_err("an ordinary call argument semantically consumes its value");
    assert_eq!(error, InterpreterError::UninitializedRead { block: 0 });
    assert_eq!(error.code(), "E3001");
    assert_eq!(error.message(), "read-of-uninitialized-object");

    let mut callee_return_module = direct_call_module();
    let callee = callee_return_module
        .functions
        .iter_mut()
        .find(|function| function.name == "identity")
        .expect("fixture callee");
    let poison_result = bits(6, 32);
    let memory = callee.blocks[0].args[1].value.clone();
    let world = callee.blocks[0].args[2].value.clone();
    callee.blocks[0].id = 9;
    callee.blocks[0].insts = vec![Inst::Uninitialized(poison_result.clone())];
    callee.blocks[0].terminator = Inst::Ret(vec![poison_result, memory, world]);
    assert!(matches!(
        interpret_module(
            &callee_return_module,
            "main",
            &entry_args,
            InterpreterLimits::new(3),
        ),
        Err(InterpreterError::StepBudgetExceeded { .. })
    ));
    let error = interpret_module(
        &callee_return_module,
        "main",
        &entry_args,
        InterpreterLimits::new(5),
    )
    .expect_err("observing the caller's result consumes the transported poison");
    assert_eq!(error, InterpreterError::UninitializedRead { block: 0 });
    assert_eq!(error.code(), "E3001");
    assert_eq!(error.message(), "read-of-uninitialized-object");
}
