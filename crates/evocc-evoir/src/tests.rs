//! EvoIR shared unit-test builders.

use super::*;
use evocc_memory::{AccessLayout, InitialState};
use evocc_types::{AddressSpace, Region};

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

fn bits(id: u32, width: u16) -> Value {
    value(id, EvoIRType::Bits(width))
}

fn argument(name: &str, value: Value) -> BlockArg {
    BlockArg {
        name: name.to_owned(),
        value,
    }
}

fn overly_nested_type() -> EvoIRType {
    let mut ty = EvoIRType::Bits(1);
    for _ in 0..=MAX_TYPE_NESTING {
        ty = EvoIRType::Tuple(vec![ty]);
    }
    ty
}

fn reject(function: &Function, fragment: &str) {
    let error = validate_ssa(function).expect_err("forged IR must be rejected");
    assert!(
        error.contains(fragment),
        "expected {fragment:?} in validator error, got {error:?}"
    );
}

fn threaded_function() -> Function {
    let produced = bits(0, 32);
    let incoming = bits(1, 32);
    let one = bits(2, 32);
    let result = bits(3, 32);
    Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "threaded".to_owned(),
        params: vec![],
        ret_tys: vec![EvoIRType::Bits(32)],
        effects: Default::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![],
                insts: vec![Inst::Const(produced.clone(), 41)],
                terminator: Inst::Br(1, vec![produced]),
            },
            Block {
                id: 1,
                args: vec![argument("incoming", incoming.clone())],
                insts: vec![
                    Inst::Const(one.clone(), 1),
                    Inst::Add(result.clone(), incoming, one),
                ],
                terminator: Inst::Ret(vec![result]),
            },
        ],
        param_marks: Vec::new(),
    }
}

fn token_call_function() -> Function {
    let mem_global = EvoIRType::Mem(Region::Global);
    let mem_stack = EvoIRType::Mem(Region::Stack);
    let world = EvoIRType::World;
    let global_in = value(0, mem_global.clone());
    let stack_in = value(1, mem_stack.clone());
    let world_in = value(2, world.clone());
    let global_out = value(3, mem_global.clone());
    let world_out = value(4, world.clone());
    Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "tokens".to_owned(),
        params: vec![
            FunctionParam {
                name: "global".to_owned(),
                ty: mem_global.clone(),
            },
            FunctionParam {
                name: "stack".to_owned(),
                ty: mem_stack.clone(),
            },
            FunctionParam {
                name: "world".to_owned(),
                ty: world.clone(),
            },
        ],
        ret_tys: vec![mem_global, mem_stack, world],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![
                argument("global", global_in.clone()),
                argument("stack", stack_in.clone()),
                argument("world", world_in.clone()),
            ],
            insts: vec![Inst::Call {
                returns_twice: false,
                arg_marks: Vec::new(),
                results: vec![],
                callee: "side_effect".to_owned(),
                args: vec![],
                mem_in: global_in,
                world_in,
                mem_out: global_out.clone(),
                world_out: world_out.clone(),
            }],
            terminator: Inst::Ret(vec![global_out, stack_in, world_out]),
        }],
        param_marks: Vec::new(),
    }
}

fn identity_callee(name: &str, ordinary_ty: EvoIRType, region: Region) -> Function {
    let memory_ty = EvoIRType::Mem(region);
    let input = value(0, ordinary_ty.clone());
    let memory = value(1, memory_ty.clone());
    let world = value(2, EvoIRType::World);
    Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: name.to_owned(),
        params: vec![
            FunctionParam {
                name: "input".to_owned(),
                ty: ordinary_ty.clone(),
            },
            FunctionParam {
                name: "memory".to_owned(),
                ty: memory_ty.clone(),
            },
            FunctionParam {
                name: "world".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![ordinary_ty, memory_ty, EvoIRType::World],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![
                argument("input", input.clone()),
                argument("memory", memory.clone()),
                argument("world", world.clone()),
            ],
            insts: vec![],
            terminator: Inst::Ret(vec![input, memory, world]),
        }],
        param_marks: Vec::new(),
    }
}

fn direct_call_module() -> Module {
    let memory_ty = EvoIRType::Mem(Region::All);
    let memory_in = value(0, memory_ty.clone());
    let world_in = value(1, EvoIRType::World);
    let argument_value = bits(2, 32);
    let result = bits(3, 32);
    let memory_out = value(4, memory_ty.clone());
    let world_out = value(5, EvoIRType::World);
    let main = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "memory".to_owned(),
                ty: memory_ty.clone(),
            },
            FunctionParam {
                name: "world".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![
                argument("memory", memory_in.clone()),
                argument("world", world_in.clone()),
            ],
            insts: vec![
                Inst::Const(argument_value.clone(), 41),
                Inst::Call {
                    returns_twice: false,
                    arg_marks: Vec::new(),
                    results: vec![result.clone()],
                    callee: "identity".to_owned(),
                    args: vec![argument_value],
                    mem_in: memory_in,
                    world_in,
                    mem_out: memory_out.clone(),
                    world_out: world_out.clone(),
                },
            ],
            terminator: Inst::Ret(vec![result, memory_out, world_out]),
        }],
        param_marks: Vec::new(),
    };
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![
            identity_callee("identity", EvoIRType::Bits(32), Region::All),
            main,
        ],
    };
    module.recompute_effect_summaries();
    module
}

fn indirect_call_module(null: bool) -> Module {
    let memory_ty = EvoIRType::Mem(Region::All);
    let signature = EvoIRType::FnPtr {
        variadic: false,
        params: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
        results: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
        marks: Vec::new(),
    };
    let memory_in = value(0, memory_ty.clone());
    let world_in = value(1, EvoIRType::World);
    let callee = value(2, signature);
    let argument_value = bits(3, 32);
    let result = bits(4, 32);
    let memory_out = value(5, memory_ty.clone());
    let world_out = value(6, EvoIRType::World);
    let address = if null {
        Inst::PtrNull(callee.clone())
    } else {
        Inst::FuncAddr {
            result: callee.clone(),
            symbol: "identity".to_owned(),
        }
    };
    let main = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "memory".to_owned(),
                ty: memory_ty.clone(),
            },
            FunctionParam {
                name: "world".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![
                argument("memory", memory_in.clone()),
                argument("world", world_in.clone()),
            ],
            insts: vec![
                address,
                Inst::Const(argument_value.clone(), 41),
                Inst::IndirectCall {
                    arg_marks: Vec::new(),
                    results: vec![result.clone()],
                    callee,
                    args: vec![argument_value],
                    mem_in: memory_in,
                    world_in,
                    mem_out: memory_out.clone(),
                    world_out: world_out.clone(),
                },
            ],
            terminator: Inst::Ret(vec![result, memory_out, world_out]),
        }],
        param_marks: Vec::new(),
    };
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![
            identity_callee("identity", EvoIRType::Bits(32), Region::All),
            main,
        ],
    };
    module.recompute_effect_summaries();
    module
}

fn recursive_countdown_module() -> Module {
    let memory_ty = EvoIRType::Mem(Region::All);
    let n = bits(0, 32);
    let memory = value(1, memory_ty.clone());
    let world = value(2, EvoIRType::World);
    let zero = bits(3, 32);
    let condition = bits(4, 1);
    let base_n = bits(5, 32);
    let base_memory = value(6, memory_ty.clone());
    let base_world = value(7, EvoIRType::World);
    let recur_n = bits(8, 32);
    let recur_memory = value(9, memory_ty.clone());
    let recur_world = value(10, EvoIRType::World);
    let one = bits(11, 32);
    let next_n = bits(12, 32);
    let result = bits(13, 32);
    let memory_out = value(14, memory_ty.clone());
    let world_out = value(15, EvoIRType::World);
    let countdown = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: crate::Linkage::Strong,
        visibility: crate::GlobalVisibility::External,
        variadic: false,
        name: "countdown".to_owned(),
        params: vec![
            FunctionParam {
                name: "n".to_owned(),
                ty: EvoIRType::Bits(32),
            },
            FunctionParam {
                name: "memory".to_owned(),
                ty: memory_ty.clone(),
            },
            FunctionParam {
                name: "world".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![EvoIRType::Bits(32), memory_ty.clone(), EvoIRType::World],
        effects: Default::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![
                    argument("n", n.clone()),
                    argument("memory", memory.clone()),
                    argument("world", world.clone()),
                ],
                insts: vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::ICmp(condition.clone(), CmpOp::Eq, n.clone(), zero),
                ],
                terminator: Inst::CondBr(
                    condition,
                    1,
                    vec![n.clone(), memory.clone(), world.clone()],
                    2,
                    vec![n, memory, world],
                ),
            },
            Block {
                id: 1,
                args: vec![
                    argument("n", base_n.clone()),
                    argument("memory", base_memory.clone()),
                    argument("world", base_world.clone()),
                ],
                insts: vec![],
                terminator: Inst::Ret(vec![base_n, base_memory, base_world]),
            },
            Block {
                id: 2,
                args: vec![
                    argument("n", recur_n.clone()),
                    argument("memory", recur_memory.clone()),
                    argument("world", recur_world.clone()),
                ],
                insts: vec![
                    Inst::Const(one.clone(), 1),
                    Inst::Sub(next_n.clone(), recur_n, one),
                    Inst::Call {
                        returns_twice: false,
                        arg_marks: Vec::new(),
                        results: vec![result.clone()],
                        callee: "countdown".to_owned(),
                        args: vec![next_n],
                        mem_in: recur_memory,
                        world_in: recur_world,
                        mem_out: memory_out.clone(),
                        world_out: world_out.clone(),
                    },
                ],
                terminator: Inst::Ret(vec![result, memory_out, world_out]),
            },
        ],
        param_marks: Vec::new(),
    };

    let mut module = direct_call_module();
    let main = module.functions.pop().expect("fixture main");
    let mut main = main;
    let Inst::Const(_, immediate) = &mut main.blocks[0].insts[0] else {
        panic!("fixture constant");
    };
    *immediate = 3;
    let Inst::Call { callee, .. } = &mut main.blocks[0].insts[1] else {
        panic!("fixture call");
    };
    *callee = "countdown".to_owned();
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![countdown, main],
    };
    module.recompute_effect_summaries();
    module
}

mod arithmetic;
mod execution;
mod uninitialized;
mod validation;
