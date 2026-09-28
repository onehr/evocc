use evocc_evoir::{parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{MInst, MirLimits, StoreKind};
use evocc_types::{EvoIRType, Region};

fn source(function: bool, volatile: bool) -> String {
    let ty = if function {
        "fnptr<(mem<all>, world)->(mem<all>, world)>"
    } else {
        "ptr<generic>"
    };
    let marker = if volatile { " volatile" } else { "" };
    format!("func @main(%arg0: mem<all>, %arg1: world) -> (bits<1>, mem<all>, world) cc = target_default
    effects {{ reads={{all}}, writes={{all}}, volatile={volatile}, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}
{{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=0, size=8, align=8]
    %v5 = ptr.null : {ty}
    %v6 = access_in_bounds %v3, %v2 [size=8, align=8]
    (%v7, %v8) = store{marker} %v6, %v4, %v2, %v5
    %v9 = access_in_bounds %v7, %v2 [size=8, align=8]
    (%v10, %v11, %v12) = load{marker} %v9, %v8, %v2 : {ty}
    %v13 = icmp.eq %v5, %v10
    ret %v13, %v11, %v12
}}
")
}

#[test]
fn null_pointer_stores_preserve_typed_representation_and_volatile_events() {
    for function in [false, true] {
        for volatile in [false, true] {
            let ir = parse_section9_module(source(function, volatile).as_bytes()).unwrap();
            evocc_evoir::validate_module(&ir).unwrap();
            let args = [
                RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                RuntimeValue::new(EvoIRType::World, 0),
            ];
            let expected =
                evocc_evoir::interpret_module(&ir, "main", &args, InterpreterLimits::new(100))
                    .unwrap();
            let mir = evocc_mir::lower_module(&ir).unwrap();
            let actual = evocc_mir::lane::observe_entry(
                &mir,
                "main",
                &args,
                &[EvoIRType::Bits(1)],
                MirLimits::new(100),
            )
            .unwrap();
            assert_eq!(actual.returns, expected.return_values[..1]);
            assert_eq!(actual.returns[0].raw_bits, 1);
            assert_eq!(
                actual.world.volatile_trace(),
                expected.world.volatile_trace()
            );
        }
    }
}

#[test]
fn nonzero_raw_bits_cannot_be_stored_as_pointer_provenance() {
    for function in [false, true] {
        for volatile in [false, true] {
            let ir = parse_section9_module(source(function, volatile).as_bytes()).unwrap();
            let mut mir = evocc_mir::lower_module(&ir).unwrap();
            let mut mutations = 0;
            for inst in &mut mir.functions[0].blocks[0].insts {
                if let MInst::MovImm { imm, .. } = inst {
                    if *imm == 0 {
                        *imm = 37;
                        mutations += 1;
                    }
                }
            }
            assert_eq!(mutations, 1);
            evocc_mir::validate_module(&mir).unwrap();
            let error =
                evocc_mir::interpret_module(&mir, "main", &[], MirLimits::new(100)).unwrap_err();
            assert!(
                matches!(error, evocc_mir::MirError::Memory { code: "E3013", .. }),
                "{error:?}"
            );
        }
    }
}

#[test]
fn raw_integer_zero_store_does_not_create_a_pointer_fragment() {
    for function in [false, true] {
        let ir = parse_section9_module(source(function, false).as_bytes()).unwrap();
        let mut mir = evocc_mir::lower_module(&ir).unwrap();
        for inst in &mut mir.functions[0].blocks[0].insts {
            if let MInst::Store { kind, .. } = inst {
                assert_eq!(*kind, StoreKind::P);
                *kind = StoreKind::X;
            }
        }
        evocc_mir::validate_module(&mir).unwrap();
        let error =
            evocc_mir::interpret_module(&mir, "main", &[], MirLimits::new(100)).unwrap_err();
        assert!(
            matches!(error, evocc_mir::MirError::Memory { code: "E3013", .. }),
            "{error:?}"
        );
    }
}

fn atomic_source(function: bool, volatile: bool, operation: &str) -> String {
    let text = source(function, volatile).replace("atomic=false", "atomic=true");
    if operation == "store" {
        return text
            .replace(" = store", " = atomic.store release")
            .replace(" = load", " = atomic.load acquire");
    }
    let ty = if function {
        "fnptr<(mem<all>, world)->(mem<all>, world)>"
    } else {
        "ptr<generic>"
    };
    let marker = if volatile { " volatile" } else { "" };
    let operands = if operation == "cmpxchg" {
        "%v5, %v14"
    } else {
        "%v14"
    };
    text.replace(
        "    %v9 = access_in_bounds",
        &format!("    %v14 = ptr.null : {ty}\n    %v9 = access_in_bounds"),
    )
    .replace(
        &format!("load{marker} %v9, %v8, %v2 : {ty}"),
        &format!("atomic.rmw.{operation} seq_cst{marker} %v9, %v8, %v2, {operands} : {ty}"),
    )
}

#[test]
fn atomic_null_pointer_operands_keep_order_status_and_events() {
    for operation in ["store", "exchange", "cmpxchg"] {
        for function in [false, true] {
            for volatile in [false, true] {
                let text = atomic_source(function, volatile, operation);
                let ir = parse_section9_module(text.as_bytes()).unwrap();
                evocc_evoir::validate_module(&ir).unwrap();
                let args = [
                    RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                    RuntimeValue::new(EvoIRType::World, 0),
                ];
                let expected =
                    evocc_evoir::interpret_module(&ir, "main", &args, InterpreterLimits::new(100))
                        .unwrap();
                let mir = evocc_mir::lower_module(&ir).unwrap();
                evocc_mir::validate_module(&mir).unwrap();
                assert_eq!(
                    evocc_mir::parse_module(&evocc_mir::render_module(&mir)).unwrap(),
                    mir
                );
                let actual = evocc_mir::lane::observe_entry(
                    &mir,
                    "main",
                    &args,
                    &[EvoIRType::Bits(1)],
                    MirLimits::new(100),
                )
                .unwrap();
                assert_eq!(actual.returns, expected.return_values[..1], "{text}");
                assert_eq!(actual.returns[0].raw_bits, 1);
                assert_eq!(actual.world, expected.world, "{text}");
                assert_eq!(
                    actual.world.atomic_trace().len(),
                    if operation == "store" { 2 } else { 1 }
                );
            }
        }
    }
}

#[test]
fn atomic_stores_and_rmw_do_not_acquire_provenance_from_nonzero_bits() {
    for operation in ["store", "exchange", "cmpxchg"] {
        for function in [false, true] {
            let ir = parse_section9_module(atomic_source(function, false, operation).as_bytes())
                .unwrap();
            let mut mir = evocc_mir::lower_module(&ir).unwrap();
            let body = &mut mir.functions[0].blocks[0].insts;
            let register = evocc_mir::VReg {
                id: body
                    .iter()
                    .flat_map(evocc_mir::inst_defs)
                    .map(|r| r.id)
                    .max()
                    .unwrap()
                    + 1,
                class: evocc_mir::RegClass::Gpr64,
            };
            let index = body
                .iter()
                .position(|i| {
                    matches!(
                        i,
                        MInst::Atomic { load: false, .. } | MInst::AtomicRmw { .. }
                    )
                })
                .unwrap();
            match &mut body[index] {
                MInst::Atomic { value, .. } | MInst::AtomicRmw { value, .. } => *value = register,
                _ => unreachable!(),
            }
            // Do not mutate a shared constant used by the earlier ordinary
            // store: the failure must occur at this atomic operation itself.
            body.insert(
                index,
                MInst::MovImm {
                    dst: register,
                    imm: 37,
                },
            );
            evocc_mir::validate_module(&mir).unwrap();
            let error =
                evocc_mir::interpret_module(&mir, "main", &[], MirLimits::new(100)).unwrap_err();
            match error {
                evocc_mir::MirError::Memory { code: "E3013", .. } if operation == "store" => {}
                evocc_mir::MirError::Invalid { detail }
                    if operation != "store" && detail == "invalid atomic RMW operand" => {}
                error => panic!("unexpected refusal: {error:?}"),
            }
        }
    }
}
