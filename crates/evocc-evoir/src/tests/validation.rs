//! EvoIR validation unit tests.

use super::*;

#[test]
fn entry_interface_is_non_ssa_and_interpreter_accepts_explicit_arguments() {
    let input = bits(0, 32);
    let one = bits(1, 32);
    let result = bits(2, 32);
    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "increment".to_owned(),
        params: vec![FunctionParam {
            name: "x".to_owned(),
            ty: EvoIRType::Bits(32),
        }],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument("x", input.clone())],
            insts: vec![
                Inst::Const(one.clone(), 1),
                Inst::Add(result.clone(), input, one),
            ],
            terminator: Inst::Ret(vec![result]),
        }],
        param_marks: Vec::new(),
    };
    validate_ssa(&function).expect("valid entry correspondence");
    let outcome = eval_function(
        &function,
        &[RuntimeValue::bits(32, 41)],
        InterpreterLimits::new(3),
    )
    .expect("bounded execution");
    assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 42)]);
    assert_eq!(outcome.steps, 3);

    let mut missing_entry_arg = function.clone();
    missing_entry_arg.blocks[0].args.clear();
    reject(&missing_entry_arg, "has 0 arguments");
    assert!(matches!(
        eval_function(&function, &[], InterpreterLimits::new(3)),
        Err(InterpreterError::EntryArgumentCount {
            expected: 1,
            actual: 0
        })
    ));
    assert!(matches!(
        eval_function(
            &function,
            &[RuntimeValue::bits(64, 41)],
            InterpreterLimits::new(3)
        ),
        Err(InterpreterError::EntryArgumentType { index: 0, .. })
    ));
    assert!(matches!(
        eval_function(
            &function,
            &[RuntimeValue::new(overly_nested_type(), 41)],
            InterpreterLimits::new(3)
        ),
        Err(InterpreterError::InvalidEntryArgument { index: 0, .. })
    ));

    let pointer_type = EvoIRType::Ptr(AddressSpace::Stack);
    let pointer = value(10, pointer_type.clone());
    let pointer_function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "pointer_passthrough".to_owned(),
        params: vec![FunctionParam {
            name: "pointer".to_owned(),
            ty: pointer_type.clone(),
        }],
        ret_tys: vec![pointer_type.clone()],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument("pointer", pointer.clone())],
            insts: vec![],
            terminator: Inst::Ret(vec![pointer]),
        }],
        param_marks: Vec::new(),
    };
    validate_ssa(&pointer_function).expect("pointer IR is structurally valid");
    assert!(matches!(
        eval_function(
            &pointer_function,
            &[RuntimeValue::new(pointer_type, 0x1000)],
            InterpreterLimits::new(1)
        ),
        Err(InterpreterError::UnsupportedRuntimeType { index: 0, .. })
    ));
}

#[test]
fn real_block_argument_ids_are_unique_and_cross_block_values_are_threaded() {
    let function = threaded_function();
    validate_ssa(&function).expect("explicit block-argument threading is valid");

    let mut duplicate = function.clone();
    duplicate.blocks[1].args[0].value.id = 0;
    reject(&duplicate, "defined more than once");

    let mut cross_block = function.clone();
    cross_block.blocks[1].terminator = Inst::Ret(vec![bits(0, 32)]);
    reject(&cross_block, "not a prior local definition");

    let mut use_before_definition = function.clone();
    let late = bits(9, 32);
    use_before_definition.blocks[0].insts = vec![
        Inst::Add(bits(10, 32), late.clone(), late.clone()),
        Inst::Const(late, 1),
    ];
    use_before_definition.blocks[0].terminator = Inst::Br(1, vec![bits(10, 32)]);
    reject(&use_before_definition, "not a prior local definition");
}

#[test]
fn edge_shape_targets_conditions_returns_and_reachability_are_checked() {
    let function = threaded_function();

    let mut arity = function.clone();
    arity.blocks[0].terminator = Inst::Br(1, vec![]);
    reject(&arity, "passes 0 values");

    let mut edge_type = function.clone();
    edge_type.blocks[1].args[0].value.ty = EvoIRType::Bits(64);
    reject(&edge_type, "edge 0 -> 1 value 0");

    let mut missing_target = function.clone();
    missing_target.blocks[0].terminator = Inst::Br(99, vec![bits(0, 32)]);
    reject(&missing_target, "targets missing block 99");

    let mut unreachable = function.clone();
    let unused = bits(20, 32);
    unreachable.blocks.push(Block {
        id: 20,
        args: vec![],
        insts: vec![Inst::Const(unused.clone(), 0)],
        terminator: Inst::Ret(vec![unused]),
    });
    reject(&unreachable, "is unreachable");

    let condition = bits(30, 32);
    let left = bits(31, 32);
    let right = bits(32, 32);
    let bad_condition = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "bad_condition".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![],
                insts: vec![Inst::Const(condition.clone(), 1)],
                terminator: Inst::CondBr(condition, 1, vec![], 2, vec![]),
            },
            Block {
                id: 1,
                args: vec![],
                insts: vec![Inst::Const(left.clone(), 1)],
                terminator: Inst::Ret(vec![left]),
            },
            Block {
                id: 2,
                args: vec![],
                insts: vec![Inst::Const(right.clone(), 2)],
                terminator: Inst::Ret(vec![right]),
            },
        ],
        param_marks: Vec::new(),
    };
    reject(&bad_condition, "expected bits<1>");

    let mut bad_return = function;
    bad_return.ret_tys = vec![EvoIRType::Bits(64)];
    reject(&bad_return, "return value 0");
}

#[test]
fn value_type_validation_is_recursive_and_void_is_not_a_value() {
    assert!(validate_type_well_formed(&EvoIRType::Void).is_ok());
    assert!(validate_value_type(&EvoIRType::Void).is_err());
    assert!(validate_value_type(&EvoIRType::Tuple(vec![])).is_err());
    assert!(validate_value_type(&EvoIRType::Bits(0)).is_err());
    assert!(validate_value_type(&EvoIRType::Vec(4, Box::new(EvoIRType::World))).is_err());
    assert!(validate_value_type(&EvoIRType::Vec(4, Box::new(EvoIRType::Tuple(vec![])))).is_err());
    assert!(validate_value_type(&EvoIRType::Ptr(AddressSpace::Stack)).is_ok());
    assert!(validate_value_type(&EvoIRType::Mem(Region::Heap)).is_ok());
    let nested_effect = EvoIRType::Vec(
        4,
        Box::new(EvoIRType::Tuple(vec![EvoIRType::Mem(Region::Heap)])),
    );
    assert!(validate_value_type(&nested_effect).is_ok());
    assert_eq!(contains_effect_token(&nested_effect), Ok(true));

    let too_deep = overly_nested_type();
    let error = validate_value_type(&too_deep).expect_err("deep forged type must be bounded");
    assert!(error.contains("nesting exceeds"));
}

#[test]
fn instruction_result_and_embedded_types_are_bounded_before_comparison() {
    let left = bits(0, 32);
    let right = bits(1, 32);
    let mut deep_result = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "deep_result".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(left.clone(), 1),
                Inst::Const(right.clone(), 2),
                Inst::Add(value(2, overly_nested_type()), left.clone(), right),
            ],
            terminator: Inst::Ret(vec![left.clone()]),
        }],
        param_marks: Vec::new(),
    };
    reject(&deep_result, "nesting exceeds");

    deep_result.blocks[0].insts = vec![
        Inst::Const(left.clone(), 1),
        Inst::SExt(bits(3, 32), left.clone(), overly_nested_type()),
    ];
    deep_result.blocks[0].terminator = Inst::Ret(vec![bits(3, 32)]);
    reject(&deep_result, "nesting exceeds");
}

#[test]
fn token_regions_are_linear_and_ordinary_call_values_exclude_tokens() {
    let function = token_call_function();
    validate_ssa(&function).expect("distinct memory regions thread independently");

    let mut stale = function.clone();
    stale.blocks[0].insts.push(Inst::Call {
        returns_twice: false,
        arg_marks: Vec::new(),
        results: vec![],
        callee: "stale".to_owned(),
        args: vec![],
        mem_in: value(0, EvoIRType::Mem(Region::Global)),
        world_in: value(2, EvoIRType::World),
        mem_out: value(5, EvoIRType::Mem(Region::Global)),
        world_out: value(6, EvoIRType::World),
    });
    stale.blocks[0].terminator = Inst::Ret(vec![
        value(5, EvoIRType::Mem(Region::Global)),
        value(1, EvoIRType::Mem(Region::Stack)),
        value(6, EvoIRType::World),
    ]);
    reject(&stale, "consumes stale");

    let mut dropped = function.clone();
    dropped.ret_tys = vec![EvoIRType::Mem(Region::Global), EvoIRType::World];
    dropped.blocks[0].terminator = Inst::Ret(vec![
        value(3, EvoIRType::Mem(Region::Global)),
        value(4, EvoIRType::World),
    ]);
    reject(&dropped, "drops current mem<Stack>");

    let mut duplicate_region = function.clone();
    duplicate_region.params[1].ty = EvoIRType::Mem(Region::Global);
    duplicate_region.blocks[0].args[1].value.ty = EvoIRType::Mem(Region::Global);
    duplicate_region.ret_tys[1] = EvoIRType::Mem(Region::Global);
    duplicate_region.blocks[0].terminator = Inst::Ret(vec![
        value(3, EvoIRType::Mem(Region::Global)),
        value(1, EvoIRType::Mem(Region::Global)),
        value(4, EvoIRType::World),
    ]);
    reject(&duplicate_region, "more than one mem<Global>");

    let mut ordinary_token = function.clone();
    let Inst::Call { args, .. } = &mut ordinary_token.blocks[0].insts[0] else {
        panic!("fixture must contain a call");
    };
    args.push(value(1, EvoIRType::Mem(Region::Stack)));
    reject(&ordinary_token, "ordinary call argument");

    let mut nested_token = function.clone();
    let tuple_token = EvoIRType::Tuple(vec![EvoIRType::Mem(Region::Heap)]);
    nested_token.params.push(FunctionParam {
        name: "hidden".to_owned(),
        ty: tuple_token.clone(),
    });
    nested_token.blocks[0]
        .args
        .push(argument("hidden", value(5, tuple_token.clone())));
    let Inst::Call { args, .. } = &mut nested_token.blocks[0].insts[0] else {
        panic!("fixture must contain a call");
    };
    args.push(value(5, tuple_token));
    reject(&nested_token, "contains a memory/world effect token");

    let mut changed_region = function;
    let Inst::Call { mem_out, .. } = &mut changed_region.blocks[0].insts[0] else {
        panic!("fixture must contain a call");
    };
    mem_out.ty = EvoIRType::Mem(Region::Heap);
    reject(&changed_region, "changes memory region");
}

#[test]
fn module_validation_checks_symbols_ordinary_signature_and_effect_interface() {
    let module = direct_call_module();
    validate_module(&module).expect("well-typed direct call module");

    let mut duplicate = module.clone();
    duplicate.functions[1].name = "identity".to_owned();
    assert!(validate_module(&duplicate)
        .expect_err("duplicate symbols must fail")
        .contains("E3041"));

    let mut missing = module.clone();
    let Inst::Call { callee, .. } = &mut missing.functions[1].blocks[0].insts[1] else {
        panic!("fixture call");
    };
    *callee = "missing".to_owned();
    assert!(validate_module(&missing)
        .expect_err("missing callee must fail")
        .contains("missing callee"));

    let mut wrong_arity = module.clone();
    let Inst::Call { args, .. } = &mut wrong_arity.functions[1].blocks[0].insts[1] else {
        panic!("fixture call");
    };
    args.clear();
    assert!(validate_module(&wrong_arity)
        .expect_err("ordinary argument arity must fail")
        .contains("ordinary arguments"));

    let mut wrong_result_arity = module.clone();
    let caller = &mut wrong_result_arity.functions[1];
    caller.ret_tys.remove(0);
    let Inst::Call { results, .. } = &mut caller.blocks[0].insts[1] else {
        panic!("fixture call");
    };
    results.clear();
    let Inst::Ret(returned) = &mut caller.blocks[0].terminator else {
        panic!("fixture return");
    };
    returned.remove(0);
    assert!(validate_module(&wrong_result_arity)
        .expect_err("ordinary result arity must fail")
        .contains("ordinary results"));

    let mut wrong_argument_type = module.clone();
    let callee = &mut wrong_argument_type.functions[0];
    callee.params[0].ty = EvoIRType::Bits(64);
    callee.ret_tys[0] = EvoIRType::Bits(64);
    callee.blocks[0].args[0].value.ty = EvoIRType::Bits(64);
    let Inst::Ret(returned) = &mut callee.blocks[0].terminator else {
        panic!("fixture return");
    };
    returned[0].ty = EvoIRType::Bits(64);
    assert!(validate_module(&wrong_argument_type)
        .expect_err("ordinary argument type must fail")
        .contains("argument 0"));

    let mut wrong_result_type = module.clone();
    let caller = &mut wrong_result_type.functions[1];
    caller.ret_tys[0] = EvoIRType::Bits(64);
    let Inst::Call { results, .. } = &mut caller.blocks[0].insts[1] else {
        panic!("fixture call");
    };
    results[0].ty = EvoIRType::Bits(64);
    let Inst::Ret(returned) = &mut caller.blocks[0].terminator else {
        panic!("fixture return");
    };
    returned[0].ty = EvoIRType::Bits(64);
    assert!(validate_module(&wrong_result_type)
        .expect_err("ordinary result type must fail")
        .contains("result 0"));

    let mut missing_effect_interface = module.clone();
    let callee = &mut missing_effect_interface.functions[0];
    callee.params.truncate(1);
    callee.ret_tys.truncate(1);
    callee.blocks[0].args.truncate(1);
    let Inst::Ret(returned) = &mut callee.blocks[0].terminator else {
        panic!("fixture return");
    };
    returned.truncate(1);
    assert!(validate_module(&missing_effect_interface)
        .expect_err("callee effect interface must be explicit")
        .contains("lacks trailing mem<R>, world"));

    let mut reordered_caller_parameters = module.clone();
    let caller = &mut reordered_caller_parameters.functions[1];
    caller.params.swap(0, 1);
    caller.blocks[0].args.swap(0, 1);
    validate_ssa(caller).expect("reordered caller remains function-local SSA valid");
    assert!(validate_module(&reordered_caller_parameters)
        .expect_err("a call-bearing caller must have canonical trailing parameters")
        .contains("function 'main' must end parameters with mem<R>, world"));

    let mut reordered_caller_returns = module.clone();
    let caller = &mut reordered_caller_returns.functions[1];
    caller.ret_tys.swap(1, 2);
    let Inst::Ret(returned) = &mut caller.blocks[0].terminator else {
        panic!("fixture return");
    };
    returned.swap(1, 2);
    validate_ssa(caller).expect("reordered caller remains function-local SSA valid");
    assert!(validate_module(&reordered_caller_returns)
        .expect_err("a call-bearing caller must have canonical trailing returns")
        .contains("function 'main' must end returns with mem<All>, world"));

    let mut wrong_region = module;
    let callee = &mut wrong_region.functions[0];
    callee.params[1].ty = EvoIRType::Mem(Region::Stack);
    callee.ret_tys[1] = EvoIRType::Mem(Region::Stack);
    callee.blocks[0].args[1].value.ty = EvoIRType::Mem(Region::Stack);
    let Inst::Ret(returned) = &mut callee.blocks[0].terminator else {
        panic!("fixture return");
    };
    returned[1].ty = EvoIRType::Mem(Region::Stack);
    assert!(validate_module(&wrong_region)
        .expect_err("memory region mismatch must fail")
        .contains("memory token"));
}

#[test]
fn module_validation_requires_the_exact_recorded_effect_fixed_point() {
    let mut module = direct_call_module();
    module.recompute_effect_summaries();
    validate_module(&module).expect("freshly derived summaries are exact");

    module.functions[0].effects.may_trap = true;
    let error = validate_module(&module).expect_err("an over-strong summary must fail");
    assert!(error.contains("recorded effect summary"), "{error}");

    module.recompute_effect_summaries();
    module.functions[0].blocks[0].terminator = Inst::Trap(TrapReason::ScalarFunctionFellOffEnd);
    let error = validate_module(&module).expect_err("a stale weak summary must fail");
    assert!(error.contains("recorded effect summary"), "{error}");
}

#[test]
fn recursive_effect_derivation_reaches_the_least_shared_fixed_point() {
    let mut module = recursive_countdown_module();
    module.functions[0].blocks[1].terminator = Inst::Trap(TrapReason::ScalarFunctionFellOffEnd);
    module.recompute_effect_summaries();

    assert!(module.functions[0].effects.may_trap);
    assert!(module.functions[1].effects.may_trap);
    validate_module(&module).expect("the recursive least fixed point validates");
}
