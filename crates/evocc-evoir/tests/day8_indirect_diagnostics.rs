use evocc_evoir::{
    interpret_module, validate_module, AccessLayout, Block, BlockArg, EffectSummary, Function,
    FunctionParam, GlobalKind, GlobalObject, GlobalVisibility, Inst, InterpreterLimits, Module,
    RuntimeValue, Value,
};
use evocc_types::{AddressSpace, EvoIRType, Region};

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

fn bits(id: u32, width: u16) -> Value {
    value(id, EvoIRType::Bits(width))
}

fn mem(id: u32) -> Value {
    value(id, EvoIRType::Mem(Region::All))
}

fn world(id: u32) -> Value {
    value(id, EvoIRType::World)
}

fn argument(name: &str, value: Value) -> BlockArg {
    BlockArg {
        name: name.to_owned(),
        value,
    }
}

fn signature(width: u16) -> EvoIRType {
    EvoIRType::FnPtr {
        variadic: false,
        params: vec![
            EvoIRType::Bits(width),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        results: vec![
            EvoIRType::Bits(width),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        marks: Vec::new(),
    }
}

fn identity(width: u16) -> Function {
    let input = bits(0, width);
    let memory = mem(1);
    let ordered = world(2);
    Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: format!("identity{width}"),
        params: vec![
            FunctionParam {
                name: "input".to_owned(),
                ty: EvoIRType::Bits(width),
            },
            FunctionParam {
                name: "memory".to_owned(),
                ty: EvoIRType::Mem(Region::All),
            },
            FunctionParam {
                name: "world".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![
            EvoIRType::Bits(width),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![
                argument("input", input.clone()),
                argument("memory", memory.clone()),
                argument("world", ordered.clone()),
            ],
            insts: vec![],
            terminator: Inst::Ret(vec![input, memory, ordered]),
        }],
        param_marks: Vec::new(),
    }
}

fn caller(insts: Vec<Inst>, result: Value, memory: Value, ordered: Value) -> Function {
    let memory_in = mem(0);
    let world_in = world(1);
    Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "memory".to_owned(),
                ty: EvoIRType::Mem(Region::All),
            },
            FunctionParam {
                name: "world".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![
            result.ty.clone(),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument("memory", memory_in), argument("world", world_in)],
            insts,
            terminator: Inst::Ret(vec![result, memory, ordered]),
        }],
        param_marks: Vec::new(),
    }
}

fn null_module() -> Module {
    let callee = value(2, signature(64));
    let argument_value = bits(3, 64);
    let result = bits(4, 64);
    let memory_out = mem(5);
    let world_out = world(6);
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![
            identity(64),
            caller(
                vec![
                    Inst::PtrNull(callee.clone()),
                    Inst::Const(argument_value.clone(), 41),
                    Inst::IndirectCall {
                        arg_marks: Vec::new(),
                        results: vec![result.clone()],
                        callee,
                        args: vec![argument_value],
                        mem_in: mem(0),
                        world_in: world(1),
                        mem_out: memory_out.clone(),
                        world_out: world_out.clone(),
                    },
                ],
                result,
                memory_out,
                world_out,
            ),
        ],
    };
    module.recompute_effect_summaries();
    module
}

fn static_signature_mismatch_module() -> Module {
    let callee = value(2, signature(32));
    let argument_value = bits(3, 64);
    let result = bits(4, 64);
    let memory_out = mem(5);
    let world_out = world(6);
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![
            identity(32),
            caller(
                vec![
                    Inst::FuncAddr {
                        result: callee.clone(),
                        symbol: "identity32".to_owned(),
                    },
                    Inst::Const(argument_value.clone(), 41),
                    Inst::IndirectCall {
                        arg_marks: Vec::new(),
                        results: vec![result.clone()],
                        callee,
                        args: vec![argument_value],
                        mem_in: mem(0),
                        world_in: world(1),
                        mem_out: memory_out.clone(),
                        world_out: world_out.clone(),
                    },
                ],
                result,
                memory_out,
                world_out,
            ),
        ],
    };
    module.recompute_effect_summaries();
    module
}

enum StoredProvenance {
    Object,
    Function32,
}

fn roundtrip_module(stored: StoredProvenance) -> Module {
    let address = value(2, EvoIRType::Ptr(AddressSpace::Stack));
    let allocated_memory = mem(3);
    let allocated_world = world(4);
    let stored_value = match stored {
        // global_addr has ptr<generic>; the stored provenance is still an
        // object, which must remain the distinct runtime E3016 failure.
        StoredProvenance::Object => value(5, EvoIRType::Ptr(AddressSpace::Generic)),
        StoredProvenance::Function32 => value(5, signature(32)),
    };
    let checked_store = mem(6);
    let bounded_store = mem(7);
    let stored_memory = mem(8);
    let stored_world = world(9);
    let checked_load = mem(10);
    let bounded_load = mem(11);
    let loaded_callee = value(12, signature(64));
    let loaded_memory = mem(13);
    let loaded_world = world(14);
    let argument_value = bits(15, 64);
    let result = bits(16, 64);
    let call_memory = mem(17);
    let call_world = world(18);
    let source = match stored {
        StoredProvenance::Object => Inst::GlobalAddr {
            result: stored_value.clone(),
            symbol: "object_source".to_owned(),
        },
        StoredProvenance::Function32 => Inst::FuncAddr {
            result: stored_value.clone(),
            symbol: "identity32".to_owned(),
        },
    };
    let main = caller(
        vec![
            Inst::Alloca {
                result: address.clone(),
                mem_in: mem(0),
                world_in: world(1),
                mem_out: allocated_memory.clone(),
                world_out: allocated_world.clone(),
                origin: 8100,
                layout: AccessLayout::new(8, 8),
            },
            source,
            Inst::PointerValid {
                mem_in: allocated_memory,
                pointer: address.clone(),
                mem_out: checked_store.clone(),
            },
            Inst::AccessInBounds {
                mem_in: checked_store,
                pointer: address.clone(),
                mem_out: bounded_store.clone(),
                layout: AccessLayout::new(8, 8),
            },
            Inst::Store {
                address: address.clone(),
                value: stored_value,
                mem_in: bounded_store,
                world_in: allocated_world,
                mem_out: stored_memory.clone(),
                world_out: stored_world.clone(),
                volatile: false,
                mask: None,
            },
            Inst::PointerValid {
                mem_in: stored_memory,
                pointer: address.clone(),
                mem_out: checked_load.clone(),
            },
            Inst::AccessInBounds {
                mem_in: checked_load,
                pointer: address.clone(),
                mem_out: bounded_load.clone(),
                layout: AccessLayout::new(8, 8),
            },
            Inst::Load {
                result: loaded_callee.clone(),
                address,
                mem_in: bounded_load,
                world_in: stored_world,
                mem_out: loaded_memory.clone(),
                world_out: loaded_world.clone(),
                volatile: false,
                mask: None,
            },
            Inst::Const(argument_value.clone(), 41),
            Inst::IndirectCall {
                arg_marks: Vec::new(),
                results: vec![result.clone()],
                callee: loaded_callee,
                args: vec![argument_value],
                mem_in: loaded_memory,
                world_in: loaded_world,
                mem_out: call_memory.clone(),
                world_out: call_world.clone(),
            },
        ],
        result,
        call_memory,
        call_world,
    );
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: match stored {
            StoredProvenance::Object => vec![GlobalObject {
                read_only: false,
                section: None,
                relocs: Vec::new(),
                symbol: "object_source".to_owned(),
                origin: 8001,
                kind: GlobalKind::File,
                linkage: evocc_evoir::Linkage::Strong,
                visibility: GlobalVisibility::Internal,
                layout: AccessLayout::new(8, 8),
                bytes: vec![0; 8],
                initialized: vec![true; 8],
            }],
            StoredProvenance::Function32 => vec![],
        },
        functions: vec![identity(32), identity(64), main],
    };
    module.recompute_effect_summaries();
    module
}

fn execute(module: &Module) -> evocc_evoir::InterpreterError {
    interpret_module(
        module,
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 17),
            RuntimeValue::new(EvoIRType::World, 19),
        ],
        InterpreterLimits::new(64),
    )
    .expect_err("invalid indirect call must fail")
}

#[test]
fn four_indirect_call_failures_remain_distinct_and_reachable() {
    let null = null_module();
    validate_module(&null).expect("null callee is statically well typed");
    assert_eq!(execute(&null).code(), "E3015");

    let object = roundtrip_module(StoredProvenance::Object);
    validate_module(&object).expect("non-function provenance is statically well typed");
    assert_eq!(execute(&object).code(), "E3016");

    let static_mismatch = validate_module(&static_signature_mismatch_module())
        .expect_err("static call-site mismatch must fail validation");
    assert!(static_mismatch.contains("E3017"), "{static_mismatch}");

    let dynamic = roundtrip_module(StoredProvenance::Function32);
    validate_module(&dynamic).expect("dynamic selected target is not statically knowable");
    assert_eq!(execute(&dynamic).code(), "E3019");

    assert_eq!(dynamic.functions[2].effects, EffectSummary::top());
    eprintln!("day8IndirectDiagnostics distinctCodes=4/4, unknownIndirectTop=1/1");
}
