//! Regressions for the module rules used by both the compiler and checker.
//! Each mutation starts from a legal module and retains its exact effects.
use evocc_evoir::{
    parse_section9_module, parse_target_module_detailed, render_module, validate_module,
    AccessLayout, Block, BlockArg, EvoIRType, Inst, Module, ModuleParseError, Value,
};
use evocc_types::{AddressSpace, Region};

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}
fn mem(id: u32) -> Value {
    value(id, EvoIRType::Mem(Region::All))
}
fn world(id: u32) -> Value {
    value(id, EvoIRType::World)
}
fn ptr(id: u32) -> Value {
    value(id, EvoIRType::Ptr(AddressSpace::Generic))
}
fn bits(id: u32, width: u16) -> Value {
    value(id, EvoIRType::Bits(width))
}

fn module(insts: Vec<Inst>, returned: Vec<Value>) -> Module {
    let mut module = parse_section9_module(b"func @f(%arg0: mem<all>, %arg1: world) -> (mem<all>, world) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: mem<all>, %v1: world):\n    ret %v0, %v1\n}\n").unwrap();
    let function = &mut module.functions[0];
    function.ret_tys = returned.iter().map(|v| v.ty.clone()).collect();
    function.blocks[0].insts = insts;
    function.blocks[0].terminator = Inst::Ret(returned);
    module.recompute_effect_summaries();
    module
}

fn invalid(mut module: Module, message: &str) {
    module.recompute_effect_summaries();
    let error = validate_module(&module).expect_err("invalid module was accepted");
    assert!(error.contains(message), "expected {message:?}: {error}");
    let error = parse_section9_module(render_module(&module).as_bytes()).unwrap_err();
    assert!(error.contains(message), "expected {message:?}: {error}");
}

fn allocation() -> Inst {
    Inst::Alloca {
        result: ptr(2),
        mem_in: mem(0),
        world_in: world(1),
        mem_out: mem(3),
        world_out: world(4),
        origin: 19,
        layout: AccessLayout::new(8, 8),
    }
}

fn pointer_load(ty: EvoIRType) -> Module {
    module(
        vec![
            allocation(),
            Inst::AccessInBounds {
                mem_in: mem(3),
                pointer: ptr(2),
                mem_out: mem(5),
                layout: AccessLayout::new(8, 8),
            },
            Inst::Load {
                result: value(6, ty.clone()),
                address: ptr(2),
                mem_in: mem(5),
                world_in: world(4),
                mem_out: mem(7),
                world_out: world(8),
                volatile: false,
            },
        ],
        vec![value(6, ty), mem(7), world(8)],
    )
}

fn function_pointer(argument: Option<EvoIRType>) -> EvoIRType {
    let mut params: Vec<_> = argument.into_iter().collect();
    params.extend([EvoIRType::Mem(Region::All), EvoIRType::World]);
    EvoIRType::FnPtr {
        params,
        results: vec![EvoIRType::Mem(Region::All), EvoIRType::World],
        variadic: false,
        marks: vec![],
    }
}

#[test]
fn integer_constants_fit_their_declared_width_without_truncation() {
    for width in [1, 8, 31, 63] {
        let limit = 1_u64 << width;
        let good = module(
            vec![Inst::Const(bits(2, width), limit - 1)],
            vec![bits(2, width), mem(0), world(1)],
        );
        validate_module(&good).unwrap();
        let mut bad = good.clone();
        bad.functions[0].blocks[0].insts[0] = Inst::Const(bits(2, width), limit);
        assert!(validate_module(&bad).unwrap_err().contains("does not fit"));
        assert!(matches!(
            parse_target_module_detailed(
                render_module(&bad).as_bytes(),
                "aarch64-unknown-linux-gnu"
            ),
            Err(ModuleParseError::Malformed(_))
        ));
    }
    for width in [64, 128] {
        let good = module(
            vec![Inst::Const(bits(2, width), u64::MAX)],
            vec![bits(2, width), mem(0), world(1)],
        );
        validate_module(&good).unwrap();
        parse_section9_module(render_module(&good).as_bytes()).unwrap();
    }
}

#[test]
fn readable_instruction_and_record_type_errors_have_invalid_status() {
    let bad_float = "func @f(%arg0: bits<32>) -> (bits<32>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: bits<32>):\n    %v1 = fp.add %v0, %v0 : bits<32>\n    ret %v1\n}\n";
    for text in [
        bad_float.to_owned(),
        "declare @f(bits<32> byval(8, 8), mem<all>, world) -> (mem<all>, world) cc = target_default\n".to_owned(),
        "declare @f(ptr<generic> byval(8, 3), mem<all>, world) -> (mem<all>, world) cc = target_default\n".to_owned(),
    ] {
        let error = parse_target_module_detailed(text.as_bytes(), "aarch64-unknown-linux-gnu").unwrap_err();
        assert!(matches!(error, ModuleParseError::Invalid(_)), "{error:?}: {text}");
    }
}

#[test]
fn pointer_accesses_need_the_same_byte_width_and_permitted_alignment() {
    for ty in [ptr(0).ty, function_pointer(None)] {
        let good = pointer_load(ty);
        validate_module(&good).unwrap();
        for layout in [AccessLayout::new(4, 4), AccessLayout::new(8, 16)] {
            let mut bad = good.clone();
            let Inst::AccessInBounds { layout: actual, .. } =
                &mut bad.functions[0].blocks[0].insts[1]
            else {
                unreachable!()
            };
            *actual = layout;
            invalid(bad, "matching access_in_bounds");
        }
        // Packed pointers still have eight bytes; their alignment can be one.
        let mut packed = good.clone();
        let Inst::AccessInBounds { layout, .. } = &mut packed.functions[0].blocks[0].insts[1]
        else {
            unreachable!()
        };
        *layout = AccessLayout::new(8, 1);
        validate_module(&packed).unwrap();
        let mut unguarded = good.clone();
        unguarded.functions[0].blocks[0].insts.remove(1);
        let Inst::Load { mem_in, .. } = &mut unguarded.functions[0].blocks[0].insts[1] else {
            unreachable!()
        };
        *mem_in = mem(3);
        invalid(unguarded, "matching access_in_bounds");
        let mut wrong_pointer = good;
        wrong_pointer.functions[0].blocks[0]
            .insts
            .insert(1, Inst::PtrNull(ptr(9)));
        let Inst::Load { address, .. } = &mut wrong_pointer.functions[0].blocks[0].insts[3] else {
            unreachable!()
        };
        *address = ptr(9);
        invalid(wrong_pointer, "matching access_in_bounds");
    }
}

#[test]
fn pointer_formation_requires_its_immediate_guard() {
    let good = module(
        vec![
            allocation(),
            Inst::Const(bits(5, 64), 1),
            Inst::PtrOffset {
                result: ptr(6),
                base: ptr(2),
                index: bits(5, 64),
                direction: evocc_evoir::OffsetDirection::Add,
                signedness: evocc_evoir::IndexSignedness::Unsigned,
                scale: 1,
            },
            Inst::PointerValid {
                mem_in: mem(3),
                pointer: ptr(6),
                mem_out: mem(7),
            },
        ],
        vec![mem(7), world(4)],
    );
    validate_module(&good).unwrap();
    let mut missing = good.clone();
    missing.functions[0].blocks[0].insts.pop();
    missing.functions[0].blocks[0].terminator = Inst::Ret(vec![mem(3), world(4)]);
    invalid(missing, "immediately followed by pointer_valid");
    let mut wrong = good;
    let Inst::PointerValid { pointer, .. } = &mut wrong.functions[0].blocks[0].insts[3] else {
        unreachable!()
    };
    *pointer = ptr(2);
    invalid(wrong, "mismatched pointer_valid");
}

fn add_global(module: &mut Module) {
    module.globals.push(evocc_evoir::parse_global_text("global @g origin=0 kind=file visibility=external size=8 align=8 bytes=0000000000000000 initialized=11111111").unwrap());
}

#[test]
fn global_addresses_cannot_claim_another_address_space() {
    let mut good = module(
        vec![Inst::GlobalAddr {
            result: ptr(2),
            symbol: "g".into(),
        }],
        vec![mem(0), world(1)],
    );
    add_global(&mut good);
    validate_module(&good).unwrap();
    for space in [
        AddressSpace::Stack,
        AddressSpace::Global,
        AddressSpace::ThreadLocal,
    ] {
        let mut bad = good.clone();
        let Inst::GlobalAddr { result, .. } = &mut bad.functions[0].blocks[0].insts[0] else {
            unreachable!()
        };
        result.ty = EvoIRType::Ptr(space);
        invalid(bad, "expected ptr<generic>");
    }
}

#[test]
fn aliases_do_not_hide_tls_from_static_addresses_or_relocations() {
    let mut good = parse_section9_module(b"global @t origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111\nglobal @p origin=1 kind=file visibility=external size=8 align=8 bytes=0000000000000000 initialized=11111111 relocs=[(0, @a, 0)]\n\nalias @a = @t\n").unwrap();
    validate_module(&good).unwrap();
    good.tls.push(evocc_evoir::TlsSymbol {
        symbol: "t".into(),
        model: evocc_evoir::TlsModel::LocalExec,
    });
    invalid(good.clone(), "TLS address is not a static relocation");
    good.globals.pop();
    good.functions = module(
        vec![Inst::GlobalAddr {
            result: ptr(2),
            symbol: "a".into(),
        }],
        vec![mem(0), world(1)],
    )
    .functions;
    invalid(good, "global_addr cannot name TLS");
}

#[test]
fn lifetime_end_cannot_end_a_named_static_object_or_its_cast() {
    for cast in [false, true] {
        let mut insts = vec![Inst::GlobalAddr {
            result: ptr(2),
            symbol: "g".into(),
        }];
        let pointer = if cast {
            insts.push(Inst::PointerCast {
                result: bits(5, 64),
                operand: ptr(2),
            });
            insts.push(Inst::PointerCast {
                result: ptr(6),
                operand: bits(5, 64),
            });
            ptr(6)
        } else {
            ptr(2)
        };
        insts.push(Inst::LifetimeEnd {
            mem_in: mem(0),
            world_in: world(1),
            pointer,
            mem_out: mem(3),
            world_out: world(4),
        });
        let mut bad = module(insts, vec![mem(3), world(4)]);
        add_global(&mut bad);
        invalid(bad, "lifetime.end");
    }
    let good = module(
        vec![
            allocation(),
            Inst::LifetimeEnd {
                mem_in: mem(3),
                world_in: world(4),
                pointer: ptr(2),
                mem_out: mem(5),
                world_out: world(6),
            },
        ],
        vec![mem(5), world(6)],
    );
    validate_module(&good).unwrap();
}

#[test]
fn access_sizes_cannot_exceed_the_pointer_offset_domain() {
    for size in [i64::MAX as u64 + 1, u64::MAX] {
        let bad = module(
            vec![
                Inst::PtrNull(ptr(2)),
                Inst::AccessInBounds {
                    mem_in: mem(0),
                    pointer: ptr(2),
                    mem_out: mem(3),
                    layout: AccessLayout::new(size, 1),
                },
            ],
            vec![mem(3), world(1)],
        );
        invalid(bad, "invalid size/alignment");
    }
}

#[test]
fn function_pointer_ordinary_values_cannot_hide_effect_tokens() {
    for token in [EvoIRType::Mem(Region::All), EvoIRType::World] {
        for ty in [
            EvoIRType::Tuple(vec![token.clone()]),
            EvoIRType::Vec(2, Box::new(EvoIRType::Tuple(vec![token]))),
        ] {
            let bad = module(
                vec![Inst::Uninitialized(value(2, function_pointer(Some(ty))))],
                vec![mem(0), world(1)],
            );
            invalid(bad, "effect token");
        }
    }
    // A nested function pointer carries its own legal effect interface. That
    // interface is not an effect token stored in the outer ordinary value.
    let good = module(
        vec![Inst::Uninitialized(value(
            2,
            function_pointer(Some(function_pointer(None))),
        ))],
        vec![mem(0), world(1)],
    );
    validate_module(&good).unwrap();
}

fn reordered_interface(mut module: Module) -> Module {
    let function = &mut module.functions[0];
    let end = function.params.len();
    function.params.swap(end - 2, end - 1);
    function.blocks[0].args.swap(end - 2, end - 1);
    let end = function.ret_tys.len();
    function.ret_tys.swap(end - 2, end - 1);
    for block in &mut function.blocks {
        if let Inst::Ret(values) = &mut block.terminator {
            let end = values.len();
            values.swap(end - 2, end - 1);
        }
    }
    module
}

#[test]
fn strict_and_assembly_effects_need_the_canonical_callable_interface() {
    let strict = parse_section9_module(include_bytes!(
        "../../../tests/fixtures/floating-strict-world.evoir"
    ))
    .unwrap();
    invalid(
        reordered_interface(strict),
        "must end parameters with mem<R>, world",
    );
    let ordinary = parse_section9_module(b"func @f(%arg0: mem<all>, %arg1: world) -> (mem<all>, world) cc = target_default\n    effects { reads={all}, writes={all}, volatile=true, atomic=false, io=true, fenv=true, may_trap=true, may_unwind=false }\n{\n^entry(%v0: mem<all>, %v1: world):\n    (%v2, %v3) = asm \"\" %v0, %v1 outputs [] inputs [] clobbers []\n    ret %v2, %v3\n}\n").unwrap();
    invalid(
        reordered_interface(ordinary.clone()),
        "must end parameters with mem<R>, world",
    );
    let mut branching = ordinary;
    let Inst::Asm(assembly) = branching.functions[0].blocks[0].insts.remove(0) else {
        unreachable!()
    };
    let edge = evocc_evoir::AssemblyEdge {
        target: 1,
        arguments: vec![mem(2), world(3)],
    };
    branching.functions[0].blocks[0].terminator = Inst::AsmGoto {
        assembly,
        labels: vec![evocc_evoir::AssemblyLabel {
            name: "taken".into(),
            edge: edge.clone(),
        }],
        fallthrough: edge,
    };
    branching.functions[0].blocks.push(Block {
        id: 1,
        args: vec![
            BlockArg {
                name: "m".into(),
                value: mem(4),
            },
            BlockArg {
                name: "w".into(),
                value: world(5),
            },
        ],
        insts: vec![],
        terminator: Inst::Ret(vec![mem(4), world(5)]),
    });
    branching.recompute_effect_summaries();
    validate_module(&branching).unwrap();
    invalid(
        reordered_interface(branching),
        "must end parameters with mem<R>, world",
    );
}
