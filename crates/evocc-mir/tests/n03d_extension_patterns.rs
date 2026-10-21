//! Exact reuse keeps the old extension, width and local-use semantics.
use evocc_mir::isa::aarch64::extensions::select;
use evocc_mir::{
    BinOp, Cond, ExtOp, LoadKind, MBlock, MFunction, MInst, MModule, MTerm, MirLimits, MirValue,
    RegClass, StoreKind, Target, VReg, Width,
};

fn r(id: u32, width: Width) -> VReg {
    VReg::new(id, width.class())
}

fn function(insts: Vec<MInst>, width: Width) -> MFunction {
    MFunction {
        name: "main".into(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![r(0, width)],
            insts,
            term: MTerm::Ret {
                values: vec![r(1, width), r(2, width)],
            },
        }],
    }
}

fn module(function: MFunction) -> MModule {
    MModule {
        globals: vec![],
        functions: vec![function],
    }
}

fn extension(op: ExtOp, dst: u32, width: Width) -> MInst {
    MInst::Extend {
        op,
        dst: r(dst, width),
        src: r(0, width),
    }
}

fn check_pair(before: MFunction) {
    let (after, counts) = select(&before);
    assert_eq!(counts.removed_instructions, 1);
    let after = after.unwrap();
    assert_eq!(select(&after).0, None, "selection is idempotent");
    let before = module(before);
    let after = module(after);
    for m in [&before, &after] {
        evocc_mir::validate_module(m).unwrap();
        assert_eq!(
            evocc_mir::parse_module(&evocc_mir::render_module(m)).unwrap(),
            *m
        );
        for target in [Target::Darwin, Target::Linux] {
            let allocated =
                evocc_mir::allocate_for(target, &evocc_mir::out_of_ssa(&m.functions[0])).unwrap();
            evocc_mir::verify_allocation_for(target, &allocated).unwrap();
        }
    }
    let mut values = vec![
        0,
        1,
        u64::MAX,
        0x8000_0000_0000_0000,
        0xffff_ffff_8000_0080,
        0xffff_ffff_0000_8000,
        0x1234_5678_9abc_def0,
    ];
    let mut value = 7_u64;
    for _ in 0..64 {
        value = value.wrapping_mul(6364136223846793005).wrapping_add(1);
        values.push(value);
    }
    for value in values {
        let input = [MirValue::Int(value)];
        let a = evocc_mir::interpret_module(&before, "main", &input, MirLimits::new(100)).unwrap();
        let b = evocc_mir::interpret_module(&after, "main", &input, MirLimits::new(100)).unwrap();
        assert_eq!(a.results, b.results, "input={value:x}");
        assert_eq!(a.memory, b.memory);
        assert_eq!(a.world, b.world);
    }
}

#[test]
fn every_extension_and_low_width_keeps_dirty_upper_bits_semantics() {
    for width in [Width::W, Width::X] {
        for op in [
            ExtOp::Sxtb,
            ExtOp::Sxth,
            ExtOp::Sxtw,
            ExtOp::Uxtb,
            ExtOp::Uxth,
        ] {
            if op == ExtOp::Sxtw && width == Width::W {
                continue;
            }
            check_pair(function(
                vec![extension(op, 1, width), extension(op, 2, width)],
                width,
            ));
        }
        for bits in [1, 8, 13, 16, 31, 63]
            .into_iter()
            .filter(|b| *b < width.bits())
        {
            for signed in [false, true] {
                check_pair(function(
                    (1..=2)
                        .map(|id| MInst::Bfx {
                            signed,
                            dst: r(id, width),
                            src: r(0, width),
                            lsb: 0,
                            bits,
                            width,
                        })
                        .collect(),
                    width,
                ));
            }
            check_pair(function(
                (1..=2)
                    .map(|id| MInst::BinaryImm {
                        op: BinOp::And,
                        dst: r(id, width),
                        lhs: r(0, width),
                        imm: (1_u64 << bits) - 1,
                        width,
                    })
                    .collect(),
                width,
            ));
        }
    }
}

#[test]
fn sign_source_class_width_and_mask_are_part_of_the_key() {
    let width = Width::X;
    let base = function(
        vec![
            extension(ExtOp::Uxtb, 1, width),
            extension(ExtOp::Uxtb, 2, width),
        ],
        width,
    );
    for second in [
        extension(ExtOp::Sxtb, 2, width),
        extension(ExtOp::Uxth, 2, width),
        MInst::Extend {
            op: ExtOp::Uxtb,
            dst: r(2, Width::W),
            src: r(0, width),
        },
        MInst::Extend {
            op: ExtOp::Uxtb,
            dst: r(2, width),
            src: r(0, Width::W),
        },
        MInst::Extend {
            op: ExtOp::Uxtb,
            dst: r(2, width),
            src: r(3, width),
        },
    ] {
        let mut f = base.clone();
        f.blocks[0].insts[1] = second;
        assert!(select(&f).0.is_none());
    }
    for (signed, lsb, bits) in [(true, 0, 8), (false, 1, 8), (false, 0, 16), (false, 0, 64)] {
        let f = function(
            vec![
                MInst::Bfx {
                    signed: false,
                    dst: r(1, width),
                    src: r(0, width),
                    lsb: 0,
                    bits: 8,
                    width,
                },
                MInst::Bfx {
                    signed,
                    dst: r(2, width),
                    src: r(0, width),
                    lsb,
                    bits,
                    width,
                },
            ],
            width,
        );
        assert!(select(&f).0.is_none());
    }
    for mask in [0, 0xf0, 0xff00, u64::MAX, 0xffff] {
        let f = function(
            vec![
                MInst::BinaryImm {
                    op: BinOp::And,
                    dst: r(1, width),
                    lhs: r(0, width),
                    imm: 0xff,
                    width,
                },
                MInst::BinaryImm {
                    op: BinOp::And,
                    dst: r(2, width),
                    lhs: r(0, width),
                    imm: mask,
                    width,
                },
            ],
            width,
        );
        assert!(select(&f).0.is_none());
    }
}

#[test]
fn effects_and_fp_admission_end_the_available_segment() {
    let w = Width::X;
    for barrier in [
        MInst::Bl {
            returns_twice: false,
            symbol: "external".into(),
            args: vec![],
            results: vec![],
            fp_mode: None,
        },
        MInst::FpEnvCheck { strict: true },
        MInst::Asm(evocc_mir::AsmBody {
            template: "nop".into(),
            volatile: true,
            outputs: vec![],
            inputs: vec![],
            clobbers: vec![],
        }),
        MInst::Load {
            align: 8,
            dst: r(3, w),
            base: r(4, w),
            offset: 0,
            kind: LoadKind::X,
            volatile: false,
            mask: None,
            transport: false,
        },
        MInst::Store {
            align: 8,
            src: r(3, w),
            base: r(4, w),
            offset: 0,
            kind: StoreKind::X,
            volatile: false,
            mask: None,
            transport: false,
        },
    ] {
        let f = function(
            vec![
                extension(ExtOp::Uxtb, 1, w),
                barrier,
                extension(ExtOp::Uxtb, 2, w),
            ],
            w,
        );
        assert!(select(&f).0.is_none(), "{f:?}");
    }
}

#[test]
fn nonunique_definitions_edges_and_nonlocal_uses_are_not_reused() {
    let w = Width::X;
    let original = function(
        vec![extension(ExtOp::Uxtb, 1, w), extension(ExtOp::Uxtb, 2, w)],
        w,
    );
    for id in [0, 1, 2] {
        let mut f = original.clone();
        f.blocks[0].insts.push(MInst::MovImm {
            dst: r(id, w),
            imm: 3,
        });
        assert!(select(&f).0.is_none());
    }
    for edge in [false, true] {
        let mut f = original.clone();
        f.blocks[0].term = MTerm::B {
            target: 1,
            args: if edge { vec![r(2, w)] } else { vec![] },
        };
        // Retain the first use so the negative specifically exercises the second.
        f.blocks[0].insts.push(MInst::Mov {
            dst: r(4, w),
            src: r(1, w),
        });
        f.blocks.push(MBlock {
            id: 1,
            args: if edge { vec![r(3, w)] } else { vec![] },
            insts: vec![],
            term: MTerm::Ret {
                values: vec![r(if edge { 3 } else { 2 }, w)],
            },
        });
        assert!(select(&f).0.is_none());
    }
    let mut f = original;
    f.blocks[0].insts.pop();
    f.blocks[0].term = MTerm::B {
        target: 1,
        args: vec![r(0, w), r(1, w)],
    };
    f.blocks.push(MBlock {
        id: 1,
        args: vec![r(3, w), r(4, w)],
        insts: vec![MInst::Extend {
            op: ExtOp::Uxtb,
            dst: r(2, w),
            src: r(3, w),
        }],
        term: MTerm::Ret {
            values: vec![r(2, w), r(4, w)],
        },
    });
    assert!(select(&f).0.is_none());
}

#[test]
fn flags_and_long_reuse_chains_keep_their_original_order() {
    let w = Width::X;
    let flags = VReg::new(3, RegClass::Flags);
    let mut f = function(
        vec![
            extension(ExtOp::Uxtb, 1, w),
            extension(ExtOp::Uxtb, 2, w),
            MInst::Cmp {
                dst: flags,
                lhs: r(0, w),
                rhs: r(1, w),
                width: w,
            },
            MInst::Cset {
                dst: r(4, w),
                flags,
                cond: Cond::Eq,
            },
        ],
        w,
    );
    f.blocks[0].term = MTerm::Ret {
        values: vec![r(2, w), r(4, w)],
    };
    check_pair(f);
    let mut f = function(
        (1..=256).map(|id| extension(ExtOp::Uxth, id, w)).collect(),
        w,
    );
    // Every duplicate has a local use, but all reuse the one retained definition.
    for id in 2..=256 {
        f.blocks[0].insts.push(MInst::Mov {
            dst: r(256 + id, w),
            src: r(id, w),
        });
    }
    f.blocks[0].term = MTerm::Ret {
        values: vec![r(1, w), r(512, w)],
    };
    let (after, counts) = select(&f);
    assert_eq!(counts.removed_instructions, 255);
    let after = after.unwrap();
    assert_eq!(select(&after).0, None);
    let a = evocc_mir::interpret_module(
        &module(f),
        "main",
        &[MirValue::Int(u64::MAX)],
        MirLimits::new(2000),
    )
    .unwrap();
    let b = evocc_mir::interpret_module(
        &module(after),
        "main",
        &[MirValue::Int(u64::MAX)],
        MirLimits::new(2000),
    )
    .unwrap();
    assert_eq!(a.results, b.results);
}

#[test]
fn odd_width_evoir_conversions_keep_the_existing_interpreter_observation() {
    use evocc_evoir::{InterpreterLimits, RuntimeValue};
    use evocc_types::{EvoIRType, Region};
    let mut hits = 0;
    for bits in [1, 8, 13, 16, 31, 32, 47, 63] {
        for opcode in ["sext", "zext"] {
            let text = format!("func @main(%arg0: bits<{bits}>, %arg1: mem<all>, %arg2: world) -> (bits<64>, mem<all>, world) cc = target_default\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}\n{{\n^entry(%v0: bits<{bits}>, %v1: mem<all>, %v2: world):\n    %v3 = {opcode}.bits<64> %v0\n    %v4 = {opcode}.bits<64> %v0\n    %v5 = add.bits<64> %v3, %v4\n    ret %v5, %v1, %v2\n}}\n");
            let ir = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
            evocc_evoir::validate_module(&ir).unwrap();
            let mut mir = evocc_mir::lower_module_for(Target::Linux, &ir, &|_| true).unwrap();
            let (selected, counts) = select(&mir.functions[0]);
            hits += counts.removed_instructions;
            if let Some(selected) = selected {
                mir.functions[0] = selected;
            }
            evocc_mir::validate_module(&mir).unwrap();
            for raw in [0, 1, u64::MAX, 1 << (bits - 1), 0xabcdef0123456789] {
                let args = [
                    RuntimeValue::bits(bits, raw),
                    RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                    RuntimeValue::new(EvoIRType::World, 0),
                ];
                let expected =
                    evocc_evoir::interpret_module(&ir, "main", &args, InterpreterLimits::new(100))
                        .unwrap();
                let actual = evocc_mir::lane::observe_entry(
                    &mir,
                    "main",
                    &args,
                    &[EvoIRType::Bits(64)],
                    MirLimits::new(100),
                )
                .unwrap();
                assert_eq!(
                    actual.returns,
                    expected.return_values[..1],
                    "{bits} {opcode} {raw:x}"
                );
            }
        }
    }
    assert!(
        hits > 0,
        "the typed-IR probe must materialize a selected extension"
    );
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn all_selected_families_execute_against_unsigned_c_reference_expressions() {
    use std::process::Command;
    // Disposable test files must not be removed from retained evidence.
    let dir = std::env::temp_dir().join(format!(
        "evocc-native-extension-patterns-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::create_dir(&dir).unwrap();
    let mut functions = Vec::new();
    let mut declarations = String::new();
    let mut checks = String::new();
    for width in [Width::W, Width::X] {
        for family in 0..3 {
            for signed in [false, true] {
                if family == 2 && signed {
                    continue;
                }
                for bits in [1, 8, 13, 16, 31, 32, 47, 63]
                    .into_iter()
                    .filter(|b| *b < width.bits())
                {
                    let ext = match (signed, bits) {
                        (true, 8) => Some(ExtOp::Sxtb),
                        (true, 16) => Some(ExtOp::Sxth),
                        (true, 32) => Some(ExtOp::Sxtw),
                        (false, 8) => Some(ExtOp::Uxtb),
                        (false, 16) => Some(ExtOp::Uxth),
                        _ => None,
                    };
                    if family == 0 && ext.is_none() {
                        continue;
                    }
                    let mask = (1_u64 << bits) - 1;
                    let mut f = function(
                        (1..=2)
                            .map(|id| match family {
                                0 => extension(ext.unwrap(), id, width),
                                1 => MInst::Bfx {
                                    signed,
                                    dst: r(id, width),
                                    src: r(0, width),
                                    lsb: 0,
                                    bits,
                                    width,
                                },
                                _ => MInst::BinaryImm {
                                    op: BinOp::And,
                                    dst: r(id, width),
                                    lhs: r(0, width),
                                    imm: mask,
                                    width,
                                },
                            })
                            .collect(),
                        width,
                    );
                    f.blocks[0].insts.push(MInst::Binary {
                        op: BinOp::Add,
                        dst: r(3, width),
                        lhs: r(1, width),
                        rhs: r(2, width),
                        width,
                    });
                    f.blocks[0].term = MTerm::Ret {
                        values: vec![r(3, width)],
                    };
                    let mut f = select(&f).0.unwrap();
                    f.name = format!("probe_{}", functions.len());
                    let ctype = if width == Width::W {
                        "unsigned"
                    } else {
                        "unsigned long"
                    };
                    let value = if signed {
                        format!(
                            "((x&{mask}UL)^{}UL)-{}UL",
                            1_u64 << (bits - 1),
                            1_u64 << (bits - 1)
                        )
                    } else {
                        format!("x&{mask}UL")
                    };
                    declarations.push_str(&format!("extern {ctype} {}({ctype});\n", f.name));
                    checks.push_str(&format!(
                        "if({}(({ctype})x)!=({ctype})(2UL*({value})))return {};\n",
                        f.name,
                        functions.len() + 1
                    ));
                    functions.push(f);
                }
            }
        }
    }
    for (target, triple, filename) in [
        (Target::Darwin, "arm64-apple-macosx11.0.0", "darwin"),
        (Target::Linux, "aarch64-linux-gnu", "linux"),
    ] {
        let allocated: Vec<_> = functions
            .iter()
            .map(|f| {
                let f = evocc_mir::allocate_for(target, &evocc_mir::out_of_ssa(f)).unwrap();
                evocc_mir::verify_allocation_for(target, &f).unwrap();
                f
            })
            .collect();
        std::fs::write(
            dir.join(format!("{filename}.s")),
            evocc_mir::emit::emit_module_for(target, &allocated, &[]),
        )
        .unwrap();
        let output = Command::new("clang")
            .args([
                "-target",
                triple,
                "-c",
                &format!("{filename}.s"),
                "-o",
                &format!("{filename}.o"),
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let harness=format!("{declarations}\nint main(void){{unsigned long x=~0UL;for(unsigned n=0;n<1024;++n){{{checks}\nx=x*6364136223846793005UL+1UL;}}return 0;}}\n");
    std::fs::write(dir.join("harness.c"), harness).unwrap();
    let output = Command::new("clang")
        .args(["-O2", "darwin.o", "harness.c", "-o", "probe"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(dir.join("probe")).output().unwrap();
    assert!(
        output.status.success(),
        "{}: {:?}",
        dir.display(),
        output.status
    );
    std::fs::remove_dir_all(dir).unwrap();
}
