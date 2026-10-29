//! EvoIR execution unit tests.

use super::*;

#[test]
fn iterative_interpreter_executes_backedges_and_enforces_budget() {
    let initial_n = bits(0, 32);
    let initial_acc = bits(1, 32);
    let n = bits(2, 32);
    let acc = bits(3, 32);
    let zero = bits(4, 32);
    let condition = bits(5, 1);
    let body_n = bits(6, 32);
    let body_acc = bits(7, 32);
    let next_acc = bits(8, 32);
    let one = bits(9, 32);
    let next_n = bits(10, 32);
    let returned = bits(11, 32);
    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "loop_sum".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(initial_n.clone(), 5),
                    Inst::Const(initial_acc.clone(), 0),
                ],
                terminator: Inst::Br(1, vec![initial_n, initial_acc]),
            },
            Block {
                id: 1,
                args: vec![argument("n", n.clone()), argument("acc", acc.clone())],
                insts: vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::ICmp(condition.clone(), CmpOp::SLt, zero, n.clone()),
                ],
                terminator: Inst::CondBr(condition, 2, vec![n, acc.clone()], 3, vec![acc]),
            },
            Block {
                id: 2,
                args: vec![
                    argument("n", body_n.clone()),
                    argument("acc", body_acc.clone()),
                ],
                insts: vec![
                    Inst::Add(next_acc.clone(), body_acc, body_n.clone()),
                    Inst::Const(one.clone(), 1),
                    Inst::Sub(next_n.clone(), body_n, one),
                ],
                terminator: Inst::Br(1, vec![next_n, next_acc]),
            },
            Block {
                id: 3,
                args: vec![argument("result", returned.clone())],
                insts: vec![],
                terminator: Inst::Ret(vec![returned]),
            },
        ],
        param_marks: Vec::new(),
    };
    let outcome = eval_function(&function, &[], InterpreterLimits::new(42))
        .expect("loop must terminate within its exact budget");
    assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 15)]);
    assert_eq!(outcome.steps, 42);
    assert!(matches!(
        eval_function(&function, &[], InterpreterLimits::new(41)),
        Err(InterpreterError::StepBudgetExceeded {
            budget: 41,
            steps: 41,
            ..
        })
    ));
}

#[test]
fn self_loop_arguments_are_resolved_simultaneously_before_rebinding() {
    let a = bits(0, 32);
    let b = bits(1, 32);
    let count = bits(2, 32);
    let zero = bits(3, 32);
    let one = bits(4, 32);
    let next_count = bits(5, 32);
    let condition = bits(6, 1);
    let out_a = bits(7, 32);
    let out_b = bits(8, 32);
    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "swap_once".to_owned(),
        params: vec![
            FunctionParam {
                name: "a".to_owned(),
                ty: EvoIRType::Bits(32),
            },
            FunctionParam {
                name: "b".to_owned(),
                ty: EvoIRType::Bits(32),
            },
            FunctionParam {
                name: "count".to_owned(),
                ty: EvoIRType::Bits(32),
            },
        ],
        ret_tys: vec![EvoIRType::Bits(32), EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![
                    argument("a", a.clone()),
                    argument("b", b.clone()),
                    argument("count", count.clone()),
                ],
                insts: vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::Const(one.clone(), 1),
                    Inst::Sub(next_count.clone(), count.clone(), one),
                    Inst::ICmp(condition.clone(), CmpOp::SLt, zero, count),
                ],
                terminator: Inst::CondBr(
                    condition,
                    0,
                    vec![b, a.clone(), next_count],
                    1,
                    vec![a, bits(1, 32)],
                ),
            },
            Block {
                id: 1,
                args: vec![argument("a", out_a.clone()), argument("b", out_b.clone())],
                insts: vec![],
                terminator: Inst::Ret(vec![out_a, out_b]),
            },
        ],
        param_marks: Vec::new(),
    };
    let outcome = eval_function(
        &function,
        &[
            RuntimeValue::bits(32, 1),
            RuntimeValue::bits(32, 2),
            RuntimeValue::bits(32, 1),
        ],
        InterpreterLimits::new(11),
    )
    .expect("one self-loop iteration must execute");
    assert_eq!(
        outcome.return_values,
        vec![RuntimeValue::bits(32, 2), RuntimeValue::bits(32, 1)]
    );
    assert_eq!(outcome.steps, 11);
}

#[test]
fn integer_kernel_handles_raw_64_bit_patterns_and_equal_width_conversions() {
    let raw = bits(0, 64);
    let same = bits(1, 64);
    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "raw".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(64)],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(raw.clone(), u64::MAX),
                Inst::SExt(same.clone(), raw, EvoIRType::Bits(64)),
            ],
            terminator: Inst::Ret(vec![same]),
        }],
        param_marks: Vec::new(),
    };
    let outcome = eval_function(&function, &[], InterpreterLimits::new(3))
        .expect("raw pattern must not be host-i64 constrained");
    assert_eq!(
        outcome.return_values,
        vec![RuntimeValue::bits(64, u64::MAX)]
    );
}

#[test]
fn interpreter_reports_typed_arithmetic_and_unsupported_effect_errors() {
    let numerator = bits(0, 32);
    let zero = bits(1, 32);
    let quotient = bits(2, 32);
    let division = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "division".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(numerator.clone(), 7),
                Inst::Const(zero.clone(), 0),
                Inst::SDiv(quotient.clone(), numerator, zero),
            ],
            terminator: Inst::Ret(vec![quotient]),
        }],
        param_marks: Vec::new(),
    };
    let division_error = eval_function(&division, &[], InterpreterLimits::new(4))
        .expect_err("zero divisor must fail");
    assert!(matches!(
        &division_error,
        InterpreterError::DivisionByZero {
            operation: "signed division",
            ..
        }
    ));
    assert_eq!(division_error.code(), "E3002");
    assert_eq!(division_error.message(), "integer-division-by-zero");

    let mut remainder = division.clone();
    remainder.name = "remainder".to_owned();
    let Inst::SDiv(result, lhs, rhs) = remainder.blocks[0].insts[2].clone() else {
        panic!("fixture signed division");
    };
    remainder.blocks[0].insts[2] = Inst::SRem(result, lhs, rhs);
    let remainder_error = eval_function(&remainder, &[], InterpreterLimits::new(4))
        .expect_err("zero remainder divisor must fail");
    assert!(matches!(
        &remainder_error,
        InterpreterError::RemainderByZero {
            operation: "signed remainder",
            ..
        }
    ));
    assert_eq!(remainder_error.code(), "E3003");
    assert_eq!(remainder_error.message(), "integer-remainder-by-zero");

    assert!(matches!(
        eval_function(
            &token_call_function(),
            &[
                RuntimeValue::new(EvoIRType::Mem(Region::Global), 10),
                RuntimeValue::new(EvoIRType::Mem(Region::Stack), 20),
                RuntimeValue::new(EvoIRType::World, 30),
            ],
            InterpreterLimits::new(2)
        ),
        Err(InterpreterError::UnsupportedInstruction {
            operation: "call",
            ..
        })
    ));
}

#[test]
fn module_interpreter_executes_calls_and_preserves_tokens() {
    let outcome = interpret_module(
        &direct_call_module(),
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
            RuntimeValue::new(EvoIRType::World, 19),
        ],
        InterpreterLimits::new(4),
    )
    .expect("direct call execution");
    assert_eq!(
        outcome.return_values,
        vec![
            RuntimeValue::bits(32, 41),
            RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
            RuntimeValue::new(EvoIRType::World, 19),
        ]
    );
    assert_eq!(outcome.steps, 4);
}

#[test]
fn indirect_calls_validate_execute_and_report_null_provenance() {
    let arguments = [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
        RuntimeValue::new(EvoIRType::World, 19),
    ];
    let module = indirect_call_module(false);
    validate_module(&module).expect("func_addr and indirect signature are valid");
    let text = render_module(&module);
    let parsed = parse_section9_module(text.as_bytes())
        .expect("function pointer text parses through the shared Section 9 reader");
    assert_eq!(render_module(&parsed), text);
    let outcome = interpret_module(&module, "main", &arguments, InterpreterLimits::new(5))
        .expect("indirect call execution");
    assert_eq!(outcome.return_values[0], RuntimeValue::bits(32, 41));
    assert_eq!(outcome.steps, 5);

    let error = interpret_module(
        &indirect_call_module(true),
        "main",
        &arguments,
        InterpreterLimits::new(3),
    )
    .expect_err("null fnptr must fail at the call");
    assert_eq!(error.code(), "E3015");
    assert_eq!(error.message(), "call-through-null-function-pointer");

    let mut mismatched = indirect_call_module(true);
    let [Inst::PtrNull(defined), _, Inst::IndirectCall { callee, .. }] =
        mismatched.functions[1].blocks[0].insts.as_mut_slice()
    else {
        panic!("fixture indirect call");
    };
    let EvoIRType::FnPtr { params, .. } = &mut defined.ty else {
        panic!("fixture fnptr type");
    };
    params[0] = EvoIRType::Bits(64);
    callee.ty = defined.ty.clone();
    assert!(validate_module(&mismatched)
        .expect_err("indirect signature mismatch must fail")
        .contains("E3017"));
}

#[test]
fn target_aware_function_outcome_owns_one_memory_and_world_state() {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let outcome = interpret_function_with_target(
        &threaded_function(),
        &[],
        &target,
        InterpreterLimits::new(5),
    )
    .expect("target-aware scalar execution");

    assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 42)]);
    assert_eq!(outcome.steps, 5);
    assert_eq!(outcome.memory.config().byte_order(), target.byte_order());
    assert_eq!(
        outcome.memory.config().pointer_layout(),
        AccessLayout::from_type_layout(target.pointer_layout())
    );
    assert!(outcome.world.volatile_trace().is_empty());
}

#[test]
fn direct_call_preserves_the_single_seeded_execution_state_without_reset() {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let mut state = ExecutionState::for_target(&target, 4).expect("valid execution state");
    let (memory, seeded_address) = state
        .memory
        .allocate_static(91, AccessLayout::new(4, 4), InitialState::Zeroed)
        .expect("seed static object");
    state.memory = memory;
    let expected_memory = state.memory.clone();
    let expected_world = state.world.clone();

    let outcome = interpret_module_in_state(
        &direct_call_module(),
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
            RuntimeValue::new(EvoIRType::World, 19),
        ],
        InterpreterLimits::new(4),
        state,
    )
    .expect("direct call execution");

    assert_eq!(outcome.memory, expected_memory);
    assert_eq!(outcome.world, expected_world);
    assert_eq!(
        outcome
            .memory
            .object_info(seeded_address.object())
            .expect("callee must not reset the top-level memory")
            .origin
            .site,
        91
    );
}

#[test]
fn direct_call_fuel_failure_returns_no_fabricated_outcome_state() {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    assert!(matches!(
        interpret_module_with_target(
            &direct_call_module(),
            "main",
            &[
                RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
                RuntimeValue::new(EvoIRType::World, 19),
            ],
            &target,
            InterpreterLimits::new(3),
        ),
        Err(InterpreterError::StepBudgetExceeded {
            budget: 3,
            steps: 3,
            ..
        })
    ));
}

#[test]
fn module_interpreter_recurses_with_one_shared_step_budget() {
    let module = recursive_countdown_module();
    validate_module(&module).expect("recursive module validates");
    let text = render_module(&module);
    let parsed = parse_section9_module(text.as_bytes())
        .expect("direct-call CFG text parses through the shared Section 9 reader");
    assert_eq!(render_module(&parsed), text);
    let arguments = [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 23),
        RuntimeValue::new(EvoIRType::World, 29),
    ];
    let outcome = interpret_module(&module, "main", &arguments, InterpreterLimits::new(28))
        .expect("recursive execution uses explicit frames");
    assert_eq!(outcome.return_values[0], RuntimeValue::bits(32, 0));
    assert_eq!(outcome.steps, 28);
    assert!(matches!(
        interpret_module(&module, "main", &arguments, InterpreterLimits::new(27)),
        Err(InterpreterError::StepBudgetExceeded {
            budget: 27,
            steps: 27,
            ..
        })
    ));
}

#[test]
fn scalar_fallthrough_trap_is_a_typed_zero_operand_terminator() {
    let reason = TrapReason::ScalarFunctionFellOffEnd;
    assert_eq!(reason.diagnostic(), "E3007");
    assert_eq!(reason.message(), "scalar-function-fell-off-end");

    let trap = Inst::Trap(reason);
    assert!(inst_results(&trap).is_empty());
    assert!(inst_uses(&trap).is_empty());
    assert!(is_terminator(&trap));

    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "fallthrough".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![Block {
            id: 7,
            args: vec![],
            insts: vec![],
            terminator: trap.clone(),
        }],
        param_marks: Vec::new(),
    };
    validate_ssa(&function).expect("a typed trap is a valid CFG sink without a fake return");

    let mut misplaced = function;
    misplaced.blocks[0].insts.push(trap);
    reject(&misplaced, "terminator appears in instruction");
}

#[test]
fn scalar_fallthrough_traps_only_on_the_executed_path() {
    let condition = bits(160, 1);
    let safe_value = bits(161, 32);
    let returned = bits(162, 32);
    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "conditional_fallthrough".to_owned(),
        params: vec![FunctionParam {
            name: "take_safe_return".to_owned(),
            ty: EvoIRType::Bits(1),
        }],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: EffectSummary {
            may_trap: true,
            ..EffectSummary::default()
        },
        blocks: vec![
            Block {
                id: 0,
                args: vec![argument("take_safe_return", condition.clone())],
                insts: vec![Inst::Const(safe_value.clone(), 7)],
                terminator: Inst::CondBr(condition, 1, vec![safe_value], 2, vec![]),
            },
            Block {
                id: 1,
                args: vec![argument("returned", returned.clone())],
                insts: vec![],
                terminator: Inst::Ret(vec![returned]),
            },
            Block {
                id: 2,
                args: vec![],
                insts: vec![],
                terminator: Inst::Trap(TrapReason::ScalarFunctionFellOffEnd),
            },
        ],
        param_marks: Vec::new(),
    };
    validate_ssa(&function).expect("return and fallthrough paths are both well formed");

    let safe = interpret_function(
        &function,
        &[RuntimeValue::bits(1, 1)],
        InterpreterLimits::new(3),
    )
    .expect("an unexecuted fallthrough trap must not poison the function");
    assert_eq!(safe.return_values, vec![RuntimeValue::bits(32, 7)]);
    assert_eq!(safe.steps, 3);

    let error = interpret_function(
        &function,
        &[RuntimeValue::bits(1, 0)],
        InterpreterLimits::new(3),
    )
    .expect_err("executed scalar fallthrough must fail closed");
    assert_eq!(error, InterpreterError::MissingReturn { block: 2 });
    assert_eq!(error.code(), "E3007");
    assert_eq!(error.message(), "scalar-function-fell-off-end");

    let module_error = interpret_module(
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
        "conditional_fallthrough",
        &[RuntimeValue::bits(1, 0)],
        InterpreterLimits::new(3),
    )
    .expect_err("module execution must preserve the typed missing-return failure");
    assert_eq!(module_error, InterpreterError::MissingReturn { block: 2 });
    assert_eq!(module_error.code(), "E3007");
    assert_eq!(module_error.message(), "scalar-function-fell-off-end");
}

#[test]
fn called_scalar_fallthrough_needs_no_placeholder_return_or_effect_tokens() {
    let mut module = direct_call_module();
    let callee = module
        .functions
        .iter_mut()
        .find(|function| function.name == "identity")
        .expect("fixture callee");
    callee.blocks[0].id = 9;
    callee.blocks[0].insts.clear();
    callee.blocks[0].terminator = Inst::Trap(TrapReason::ScalarFunctionFellOffEnd);
    module.recompute_effect_summaries();
    validate_module(&module)
        .expect("a trapping callee needs no fabricated ordinary or token return operands");

    let error = interpret_module(
        &module,
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 10),
            RuntimeValue::new(EvoIRType::World, 20),
        ],
        InterpreterLimits::new(3),
    )
    .expect_err("the callee fallthrough trap must fire only after the call executes");
    assert_eq!(error, InterpreterError::MissingReturn { block: 9 });
    assert_eq!(error.code(), "E3007");
    assert_eq!(error.message(), "scalar-function-fell-off-end");
}
