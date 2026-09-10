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
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
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
        aliases: Vec::new(),
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
fn dynamic_stack_size_is_a_typed_ssa_operand_and_preserves_object_lifetime() {
    let mem = EvoIRType::Mem(Region::All);
    let world = EvoIRType::World;
    let pointer = value(3, EvoIRType::Ptr(AddressSpace::Generic));
    let mem_out = value(4, mem.clone());
    let world_out = value(5, world.clone());
    let mut function = token_function(
        "main",
        vec![Inst::Alloca {
            result: pointer.clone(),
            mem_in: value(0, mem),
            world_in: value(1, world),
            mem_out: mem_out.clone(),
            world_out: world_out.clone(),
            origin: 77,
            layout: AccessLayout::new(13, 16),
        }],
        vec![pointer, mem_out, world_out],
    );
    function.params.insert(
        0,
        FunctionParam {
            name: "size".into(),
            ty: EvoIRType::Bits(64),
        },
    );
    function.blocks[0].args.insert(
        0,
        BlockArg {
            name: "size_entry".into(),
            value: value(2, EvoIRType::Bits(64)),
        },
    );
    let mut module = Module {
        aliases: vec![],
        assembly: vec![],
        extern_objects: vec![],
        declares: vec![],
        globals: vec![],
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    let text = render_module(&module).replace(
        "= alloca %v0, %v1 [origin=77, size=13, align=16]",
        "= alloca.dynamic %v0, %v1, %v2 [origin=77, align=16]",
    );
    assert!(text.contains("alloca.dynamic"));
    let parsed = parse_section9_module(text.as_bytes()).expect("dynamic alloca must parse");
    assert_eq!(render_module(&parsed), text);
    for size in [0, 1, 19, 35] {
        let mut arguments = entry_tokens();
        arguments.insert(0, RuntimeValue::bits(64, size));
        let result = interpret_module(&parsed, "main", &arguments, InterpreterLimits::new(20))
            .expect("dynamic allocation must use its actual size");
        let objects = result.memory.objects().collect::<Vec<_>>();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].layout, AccessLayout::new(size as u64, 16));
        assert!(!objects[0].lifetime.is_alive());
    }
    for malformed in [
        text.replace("bits<64>", "bits<32>"),
        text.replace(
            "alloca.dynamic %v0, %v1, %v2",
            "alloca.dynamic %v0, %v1, %v99",
        ),
        text.replace("align=16", "align=3"),
    ] {
        assert!(parse_section9_module(malformed.as_bytes()).is_err());
    }
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
        aliases: Vec::new(),
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

    // Packing can lower the required alignment, but never change the byte
    // extent or detach the guard from its following access.
    for align_bytes in [1, 2, 4] {
        let mut packed = module.clone();
        for inst in &mut packed.functions[0].blocks[0].insts {
            if let Inst::AccessInBounds { layout, .. } = inst {
                layout.align_bytes = align_bytes;
            }
        }
        evocc_evoir::validate_module(&packed).unwrap();
        assert_eq!(
            interpret_module(
                &packed,
                "main",
                &entry_tokens(),
                InterpreterLimits::new(100)
            )
            .unwrap()
            .return_values[0],
            RuntimeValue::bits(32, 42)
        );
    }
    for layout in [
        AccessLayout::new(1, 1),
        AccessLayout::new(8, 4),
        AccessLayout::new(4, 0),
        AccessLayout::new(4, 3),
        AccessLayout::new(4, 8),
    ] {
        let mut invalid = module.clone();
        for inst in &mut invalid.functions[0].blocks[0].insts {
            if let Inst::AccessInBounds {
                layout: guarded, ..
            } = inst
            {
                *guarded = layout;
            }
        }
        assert!(
            evocc_evoir::validate_module(&invalid).is_err(),
            "{layout:?}"
        );
    }
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
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![GlobalObject {
            section: None,
            relocs: Vec::new(),
            symbol: "counter".to_owned(),
            origin: 20,
            kind: GlobalKind::File,
            linkage: evocc_evoir::Linkage::Strong,
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
