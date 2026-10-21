//! Boundary checks for the allocated instruction matcher, independent of C lowering.
use evocc_mir::isa::aarch64::local::{Selection, Selector};
use evocc_mir::{
    Allocated, BinOp, ExtOp, LoadKind, MBlock, MInst, MTerm, RegClass, StoreKind, VReg, Width,
};
use std::collections::BTreeMap;

fn r(id: u32, width: Width) -> VReg {
    VReg::new(id, width.class())
}

fn function(insts: Vec<MInst>) -> Allocated {
    Allocated {
        input: evocc_mir::MFunction {
            name: "probe".into(),
            blocks: vec![],
        },
        name: "probe".into(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![],
            insts,
            term: MTerm::Ret { values: vec![] },
        }],
        assignment: (0..12).map(|i| (i, i as u8)).collect(),
        spilled: 0,
        frame_slots: 0,
        used_callee_saved: vec![],
        parameters: 0,
        outgoing: 0,
        indirect_result: false,
        internal: false,
    }
}

fn shift_pair(width: Width, shift: BinOp, amount: u64, op: BinOp) -> Vec<MInst> {
    vec![
        MInst::BinaryImm {
            op: shift,
            dst: r(2, width),
            lhs: r(1, width),
            imm: amount,
            width,
        },
        MInst::Binary {
            op,
            dst: r(3, width),
            lhs: r(0, width),
            rhs: r(2, width),
            width,
        },
    ]
}

fn selected(f: &Allocated, strict: bool) -> Option<Selection> {
    Selector::new(f).select(&f.blocks[0].insts, strict)
}

fn text(s: &Selection) -> String {
    let mut text = String::new();
    s.emit(&mut text, &|reg, class| {
        evocc_mir::linscan::physical(class, reg.id as u8)
    });
    text
}

#[test]
fn every_shift_family_width_and_boundary_is_explicit() {
    for width in [Width::W, Width::X] {
        for shift in [BinOp::Lsl, BinOp::Lsr, BinOp::Asr] {
            for op in [BinOp::Add, BinOp::Sub, BinOp::And, BinOp::Orr, BinOp::Eor] {
                for amount in [0, u64::from(width.bits() - 1)] {
                    let f = function(shift_pair(width, shift, amount, op));
                    let s = selected(&f, false).unwrap();
                    assert_eq!(s.consumed, 2);
                    assert!(text(&s).contains(&format!(", {} #{amount}", shift.mnemonic())));
                }
                for amount in [u64::from(width.bits()), u64::MAX] {
                    assert!(
                        selected(&function(shift_pair(width, shift, amount, op)), false).is_none()
                    );
                }
            }
        }
    }
}

#[test]
fn all_definition_use_and_physical_alias_boundaries_decline() {
    let original = function(shift_pair(Width::X, BinOp::Lsl, 3, BinOp::Add));
    let mut f = original.clone();
    f.blocks[0].term = MTerm::Ret {
        values: vec![r(2, Width::X)],
    };
    assert!(selected(&f, false).is_none(), "live in terminator");
    let mut f = original.clone();
    f.blocks.push(MBlock {
        id: 1,
        args: vec![],
        insts: vec![],
        term: MTerm::Ret {
            values: vec![r(2, Width::X)],
        },
    });
    assert!(selected(&f, false).is_none(), "live in another block");
    let mut f = original.clone();
    f.blocks[0].insts.push(MInst::MovImm {
        dst: r(2, Width::X),
        imm: 7,
    });
    assert!(selected(&f, false).is_none(), "multiple definitions");
    let mut f = original.clone();
    f.blocks[0].insts.push(MInst::SpillStore {
        src: r(2, Width::X),
        slot: 0,
    });
    assert!(selected(&f, false).is_none(), "spill use");
    let mut f = original.clone();
    f.blocks[0].insts.push(MInst::Asm(evocc_mir::AsmBody {
        template: "// %[live]".into(),
        volatile: true,
        outputs: vec![],
        inputs: vec![evocc_mir::AsmOperand::reg(
            evocc_mir::AsmConstraint::Register(evocc_mir::AsmDirection::Read),
            r(2, Width::X),
        )],
        clobbers: vec![],
    }));
    assert!(selected(&f, false).is_none(), "assembly use");
    let mut f = original.clone();
    f.assignment.insert(2, 0);
    assert!(
        selected(&f, false).is_none(),
        "omitted write overwrites surviving input"
    );
    let mut f = original.clone();
    f.assignment.insert(2, 1);
    assert!(
        selected(&f, false).is_some(),
        "in-place shift source remains legal"
    );
    let mut f = original.clone();
    f.blocks[0].insts.insert(
        1,
        MInst::MovImm {
            dst: r(5, Width::X),
            imm: 1,
        },
    );
    assert!(selected(&f, false).is_none(), "no instruction motion");
    let mut f = original;
    if let MInst::Binary { width, .. } = &mut f.blocks[0].insts[1] {
        *width = Width::W;
    }
    assert!(selected(&f, false).is_none(), "width change");
}

#[test]
fn noncommutative_order_and_compare_flags_are_not_guessed() {
    for width in [Width::W, Width::X] {
        let mut f = function(shift_pair(width, BinOp::Lsr, 2, BinOp::Sub));
        if let MInst::Binary { lhs, rhs, .. } = &mut f.blocks[0].insts[1] {
            std::mem::swap(lhs, rhs);
        }
        assert!(selected(&f, false).is_none());
        f.blocks[0].insts[1] = MInst::Cmp {
            dst: VReg::new(9, RegClass::Flags),
            lhs: r(0, width),
            rhs: r(2, width),
            width,
        };
        assert!(text(&selected(&f, false).unwrap()).starts_with("    cmp "));
        if let MInst::Cmp { lhs, rhs, .. } = &mut f.blocks[0].insts[1] {
            std::mem::swap(lhs, rhs);
        }
        assert!(selected(&f, false).is_none());
    }
}

#[test]
fn extensions_and_products_cover_both_widths() {
    for width in [Width::W, Width::X] {
        for extension in [
            ExtOp::Sxtb,
            ExtOp::Sxth,
            ExtOp::Sxtw,
            ExtOp::Uxtb,
            ExtOp::Uxth,
        ] {
            let mut f = function(shift_pair(width, BinOp::Lsl, 2, BinOp::Add));
            f.blocks[0].insts[0] = MInst::Extend {
                op: extension,
                dst: r(2, width),
                src: r(1, Width::W),
            };
            if width == Width::W && extension == ExtOp::Sxtw {
                assert!(selected(&f, false).is_none());
            } else {
                assert!(text(&selected(&f, false).unwrap())
                    .contains(&format!(", {}", extension.mnemonic())));
                f.blocks[0].insts[1] = MInst::Cmp {
                    dst: VReg::new(9, RegClass::Flags),
                    lhs: r(0, width),
                    rhs: r(2, width),
                    width,
                };
                assert_eq!(selected(&f, false).unwrap().pattern, "local_extend_compare");
            }
        }
        for op in [BinOp::Add, BinOp::Sub] {
            let mut f = function(shift_pair(width, BinOp::Lsl, 2, op));
            f.blocks[0].insts[0] = MInst::Binary {
                op: BinOp::Mul,
                dst: r(2, width),
                lhs: r(1, width),
                rhs: r(4, width),
                width,
            };
            assert!(
                text(&selected(&f, false).unwrap()).contains(if op == BinOp::Add {
                    "madd"
                } else {
                    "msub"
                })
            );
        }
    }
}

fn indexed() -> Allocated {
    function(vec![
        MInst::BinaryImm {
            op: BinOp::Lsl,
            dst: r(2, Width::X),
            lhs: r(1, Width::X),
            imm: 3,
            width: Width::X,
        },
        MInst::Binary {
            op: BinOp::Add,
            dst: r(3, Width::X),
            lhs: r(0, Width::X),
            rhs: r(2, Width::X),
            width: Width::X,
        },
        MInst::Load {
            dst: r(4, Width::X),
            base: r(3, Width::X),
            offset: 0,
            kind: LoadKind::X,
            align: 8,
            volatile: false,
            mask: None,
        },
    ])
}

#[test]
fn indexed_memory_checks_alignment_width_offset_and_liveness() {
    let original = indexed();
    let choice = selected(&original, false).unwrap();
    assert_eq!(choice.consumed, 3);
    assert_eq!(text(&choice), "    ldr x4, [x0, x1, lsl #3]\n");
    for variation in 0..6 {
        let mut f = original.clone();
        match variation {
            0 => {
                if let MInst::Load { volatile, .. } = &mut f.blocks[0].insts[2] {
                    *volatile = true;
                }
            }
            1 => {
                if let MInst::Load { offset, .. } = &mut f.blocks[0].insts[2] {
                    *offset = 8;
                }
            }
            2 => {
                if let MInst::Load { align, .. } = &mut f.blocks[0].insts[2] {
                    *align = 1;
                }
            }
            3 => {
                if let MInst::BinaryImm { imm, .. } = &mut f.blocks[0].insts[0] {
                    *imm = 4;
                }
            }
            4 => {
                f.assignment.insert(2, 0);
            }
            _ => {
                f.blocks[0].term = MTerm::Ret {
                    values: vec![r(3, Width::X)],
                };
            }
        }
        let choice = selected(&f, true);
        assert!(
            choice.is_none_or(|s| s.pattern != "local_indexed_memory"),
            "variation {variation}"
        );
    }
    let mut f = original;
    f.blocks[0].insts[2] = MInst::Store {
        src: r(4, Width::X),
        base: r(3, Width::X),
        offset: 0,
        kind: StoreKind::X,
        align: 8,
        volatile: false,
        mask: None,
    };
    assert_eq!(selected(&f, false).unwrap().consumed, 3);
    for aliased in [2, 3] {
        f.assignment.insert(4, aliased);
        assert!(selected(&f, false).is_none_or(|s| s.pattern != "local_indexed_memory"));
    }
}

#[test]
fn scalar_memory_forms_and_opaque_accesses_stay_separate() {
    for (kind, class) in [
        (LoadKind::B, RegClass::Gpr32),
        (LoadKind::H, RegClass::Gpr32),
        (LoadKind::W, RegClass::Gpr32),
        (LoadKind::X, RegClass::Gpr64),
        (LoadKind::P, RegClass::Gpr64),
        (LoadKind::Pf, RegClass::Gpr64),
        (LoadKind::Sb, RegClass::Gpr32),
        (LoadKind::Sb, RegClass::Gpr64),
        (LoadKind::Sh, RegClass::Gpr32),
        (LoadKind::Sh, RegClass::Gpr64),
        (LoadKind::Sw, RegClass::Gpr64),
    ] {
        let mut f = indexed();
        if let MInst::Load {
            kind: k,
            dst,
            align,
            ..
        } = &mut f.blocks[0].insts[2]
        {
            *k = kind;
            *dst = VReg::new(4, class);
            *align = kind.size_bytes() as u32;
        }
        if let MInst::BinaryImm { imm, .. } = &mut f.blocks[0].insts[0] {
            *imm = u64::from(kind.size_bytes().trailing_zeros());
        }
        assert_eq!(selected(&f, true).unwrap().consumed, 3);
        f.blocks[0].insts.remove(0);
        assert_eq!(selected(&f, true).unwrap().consumed, 2);
    }
    for access in [
        MInst::Atomic {
            load: true,
            value: r(4, Width::X),
            base: r(3, Width::X),
            kind: LoadKind::X,
            order: evocc_memory::AtomicOrder::Relaxed,
            volatile: false,
        },
        MInst::Load {
            dst: VReg::new(4, RegClass::Simd128),
            base: r(3, Width::X),
            offset: 0,
            kind: LoadKind::Q,
            align: 16,
            volatile: false,
            mask: None,
        },
    ] {
        let mut f = indexed();
        f.blocks[0].insts[2] = access;
        assert!(selected(&f, false).is_none_or(|s| s.pattern != "local_indexed_memory"));
    }
}

#[test]
fn counters_distinguish_real_candidates_from_nonmatches() {
    let mut f = function(shift_pair(Width::X, BinOp::Lsl, 3, BinOp::Add));
    let mut selector = Selector::new(&f);
    selector.select(&f.blocks[0].insts, false).unwrap();
    assert_eq!(
        selector.counts.selected,
        BTreeMap::from([("local_shift_arithmetic", 1)])
    );
    assert_eq!(selector.counts.removed_instructions, 1);
    f.blocks[0].term = MTerm::Ret {
        values: vec![r(2, Width::X)],
    };
    let mut selector = Selector::new(&f);
    assert!(selector.select(&f.blocks[0].insts, false).is_none());
    assert!(selector.counts.selected.is_empty());
    assert_eq!(selector.counts.declined.values().sum::<usize>(), 1);
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_fused_forms_match_values_and_all_nzcv_bits() {
    use std::process::Command;
    let mut cases = Vec::new();
    for width in [Width::W, Width::X] {
        for shift in [BinOp::Lsl, BinOp::Lsr, BinOp::Asr] {
            for amount in [0, 1, u64::from(width.bits() - 1)] {
                for op in [BinOp::Add, BinOp::Sub, BinOp::And, BinOp::Orr, BinOp::Eor] {
                    cases.push(shift_pair(width, shift, amount, op));
                }
                let mut pair = shift_pair(width, shift, amount, BinOp::Add);
                pair[1] = MInst::Cmp {
                    dst: VReg::new(9, RegClass::Flags),
                    lhs: r(0, width),
                    rhs: r(2, width),
                    width,
                };
                cases.push(pair);
            }
        }
        for extension in [
            ExtOp::Sxtb,
            ExtOp::Sxth,
            ExtOp::Sxtw,
            ExtOp::Uxtb,
            ExtOp::Uxth,
        ] {
            if width == Width::W && extension == ExtOp::Sxtw {
                continue;
            }
            for op in [BinOp::Add, BinOp::Sub] {
                let mut pair = shift_pair(width, BinOp::Lsl, 1, op);
                pair[0] = MInst::Extend {
                    op: extension,
                    dst: r(2, width),
                    src: r(1, Width::W),
                };
                cases.push(pair);
            }
            cases.push(vec![
                MInst::Extend {
                    op: extension,
                    dst: r(2, width),
                    src: r(1, Width::W),
                },
                MInst::Cmp {
                    dst: VReg::new(9, RegClass::Flags),
                    lhs: r(0, width),
                    rhs: r(2, width),
                    width,
                },
            ]);
        }
        for op in [BinOp::Add, BinOp::Sub] {
            let mut pair = shift_pair(width, BinOp::Lsl, 1, op);
            pair[0] = MInst::Binary {
                op: BinOp::Mul,
                dst: r(2, width),
                lhs: r(1, width),
                rhs: r(0, width),
                width,
            };
            cases.push(pair);
        }
    }
    assert_eq!(cases.len(), 139);
    // Disposable test files must not be removed from retained evidence.
    let dir = std::env::temp_dir().join(format!("evocc-native-local-flags-{}", std::process::id()));
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::create_dir(&dir).unwrap();
    let mut assembly = String::new();
    let mut declarations = String::from("typedef struct { unsigned long value, flags; } Pair;\n");
    let mut checks = String::new();
    for (index, insts) in cases.into_iter().enumerate() {
        let mut f = function(insts);
        f.name = format!("off_{index}");
        f.assignment.insert(2, 4);
        f.assignment.insert(3, 0);
        let off = evocc_mir::emit_module_for(evocc_mir::Target::Darwin, &[f.clone()], &[])
            .replace(
                &format!("_off_{index}:\n"),
                &format!("_off_{index}:\n    msr nzcv, x2\n"),
            )
            .replace("    ret\n", "    mrs x1, nzcv\n    ret\n");
        assembly.push_str(&off);
        assembly.push_str(&format!(
            ".text\n.p2align 2\n.globl _on_{index}\n_on_{index}:\n    msr nzcv, x2\n"
        ));
        let selected = selected(&f, false).unwrap();
        selected.emit(&mut assembly, &|reg, class| {
            evocc_mir::linscan::physical(class, f.assignment[&reg.id])
        });
        assembly.push_str("    mrs x1, nzcv\n    ret\n");
        declarations.push_str(&format!("extern Pair off_{index}(unsigned long,unsigned long,unsigned long);\nextern Pair on_{index}(unsigned long,unsigned long,unsigned long);\n"));
        checks.push_str(&format!("{{ Pair a=off_{index}(x,y,f<<28), b=on_{index}(x,y,f<<28); if(a.value!=b.value || a.flags!=b.flags) return {}; }}\n", index + 1));
    }
    let harness = format!("{declarations}\nint main(void) {{ const unsigned long v[]={{0,1,0x7fffffffUL,0x80000000UL,0xffffffffUL,0x8000000000000000UL,0xffffffffffffffffUL,0xa597ac32dc8f0102UL}}; for(unsigned i=0;i<8;++i)for(unsigned j=0;j<8;++j)for(unsigned long f=0;f<16;++f){{unsigned long x=v[i],y=v[j];{checks}}} return 0; }}\n");
    std::fs::write(dir.join("subject.s"), assembly).unwrap();
    std::fs::write(dir.join("harness.c"), harness).unwrap();
    let result = Command::new("clang")
        .args(["-O2", "subject.s", "harness.c", "-o", "probe"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "retained {}: {}",
        dir.display(),
        String::from_utf8_lossy(&result.stderr)
    );
    let result = Command::new(dir.join("probe")).output().unwrap();
    assert!(
        result.status.success(),
        "exit {:?}; retained {}",
        result.status,
        dir.display()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
