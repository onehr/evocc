use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, AccessLayout, Block, BlockArg,
    Function, FunctionParam, GlobalKind, GlobalObject, GlobalVisibility, Inst, InterpreterLimits,
    Module, RuntimeValue, Value,
};
use evocc_types::{AddressSpace, EvoIRType, Region};

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

fn token_function(name: &str, insts: Vec<Inst>, returned: Vec<Value>) -> Function {
    let mem = value(0, EvoIRType::Mem(Region::All));
    let world = value(1, EvoIRType::World);
    let function = Function {
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: name.to_owned(),
        params: vec![
            FunctionParam {
                name: "mem".to_owned(),
                ty: mem.ty.clone(),
            },
            FunctionParam {
                name: "world".to_owned(),
                ty: world.ty.clone(),
            },
        ],
        ret_tys: returned.iter().map(|value| value.ty.clone()).collect(),
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![
                BlockArg {
                    name: "mem_entry".to_owned(),
                    value: mem,
                },
                BlockArg {
                    name: "world_entry".to_owned(),
                    value: world,
                },
            ],
            insts,
            terminator: Inst::Ret(returned),
        }],
        param_marks: Vec::new(),
    };
    let mut module = Module {
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    module.functions.pop().expect("one function")
}

fn entry_tokens() -> Vec<RuntimeValue> {
    vec![
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ]
}

#[test]
fn automatic_store_load_roundtrip_executes_real_memory() {
    let ptr_ty = EvoIRType::Ptr(AddressSpace::Generic);
    let mem_ty = EvoIRType::Mem(Region::All);
    let world_ty = EvoIRType::World;
    let mem0 = value(0, mem_ty.clone());
    let world0 = value(1, world_ty.clone());
    let ptr = value(2, ptr_ty);
    let mem1 = value(3, mem_ty.clone());
    let world1 = value(4, world_ty.clone());
    let stored = value(5, EvoIRType::Bits(32));
    let guarded1 = value(6, mem_ty.clone());
    let mem2 = value(7, mem_ty.clone());
    let world2 = value(8, world_ty.clone());
    let guarded2 = value(9, mem_ty.clone());
    let loaded = value(10, EvoIRType::Bits(32));
    let mem3 = value(11, mem_ty);
    let world3 = value(12, world_ty);
    let layout = AccessLayout::new(4, 4);

    let function = token_function(
        "main",
        vec![
            Inst::Alloca {
                result: ptr.clone(),
                mem_in: mem0,
                world_in: world0,
                mem_out: mem1.clone(),
                world_out: world1.clone(),
                origin: 10,
                layout,
            },
            Inst::Const(stored.clone(), 42),
            Inst::AccessInBounds {
                mem_in: mem1,
                pointer: ptr.clone(),
                mem_out: guarded1.clone(),
                layout,
            },
            Inst::Store {
                address: ptr.clone(),
                value: stored,
                mem_in: guarded1,
                world_in: world1,
                mem_out: mem2.clone(),
                world_out: world2.clone(),
                volatile: false,
            },
            Inst::AccessInBounds {
                mem_in: mem2,
                pointer: ptr.clone(),
                mem_out: guarded2.clone(),
                layout,
            },
            Inst::Load {
                result: loaded.clone(),
                address: ptr,
                mem_in: guarded2,
                world_in: world2,
                mem_out: mem3.clone(),
                world_out: world3.clone(),
                volatile: false,
            },
        ],
        vec![loaded, mem3, world3],
    );
    let module = Module {
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![function],
    };
    let text = render_module(&module);
    let parsed = parse_section9_module(text.as_bytes()).expect("canonical automatic-memory text");
    assert_eq!(render_module(&parsed), text);
    let outcome = interpret_module(
        &module,
        "main",
        &entry_tokens(),
        InterpreterLimits::new(100),
    )
    .expect("defined object-memory program must execute");

    assert_eq!(outcome.return_values[0], RuntimeValue::bits(32, 42));
    let objects: Vec<_> = outcome.memory.objects().collect();
    assert_eq!(objects.len(), 1);
    assert!(!objects[0].lifetime.is_alive());
}

#[test]
fn initialized_global_and_volatile_trace_are_observable() {
    let ptr_ty = EvoIRType::Ptr(AddressSpace::Generic);
    let mem_ty = EvoIRType::Mem(Region::All);
    let world_ty = EvoIRType::World;
    let mem0 = value(0, mem_ty.clone());
    let world0 = value(1, world_ty.clone());
    let ptr = value(2, ptr_ty);
    let guarded = value(3, mem_ty.clone());
    let loaded = value(4, EvoIRType::Bits(32));
    let mem1 = value(5, mem_ty);
    let world1 = value(6, world_ty);
    let layout = AccessLayout::new(4, 4);
    let function = token_function(
        "main",
        vec![
            Inst::GlobalAddr {
                result: ptr.clone(),
                symbol: "counter".to_owned(),
            },
            Inst::AccessInBounds {
                mem_in: mem0,
                pointer: ptr.clone(),
                mem_out: guarded.clone(),
                layout,
            },
            Inst::Load {
                result: loaded.clone(),
                address: ptr,
                mem_in: guarded,
                world_in: world0,
                mem_out: mem1.clone(),
                world_out: world1.clone(),
                volatile: true,
            },
        ],
        vec![loaded, mem1, world1],
    );
    let module = Module {
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![GlobalObject {
            relocs: Vec::new(),
            symbol: "counter".to_owned(),
            origin: 20,
            kind: GlobalKind::File,
            visibility: GlobalVisibility::External,
            layout,
            bytes: 7_u32.to_le_bytes().to_vec(),
            initialized: vec![true; 4],
        }],
        functions: vec![function],
    };
    let text = render_module(&module);
    let parsed = parse_section9_module(text.as_bytes()).expect("canonical global-memory text");
    assert_eq!(render_module(&parsed), text);
    let outcome = interpret_module(
        &module,
        "main",
        &entry_tokens(),
        InterpreterLimits::new(100),
    )
    .expect("global load must execute");

    assert_eq!(outcome.return_values[0], RuntimeValue::bits(32, 7));
    assert_eq!(outcome.world.volatile_trace().len(), 1);
}
