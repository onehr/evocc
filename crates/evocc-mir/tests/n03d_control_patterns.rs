//! C4-derived integer/control idioms. Existing MIR semantics is the oracle.
use evocc_mir::isa::aarch64::controls::select;
use evocc_mir::{
    BinOp, Cond, MBlock, MFunction, MInst, MModule, MTerm, MirLimits, MirValue, RegClass, Target,
    VReg, Width,
};

fn r(id: u32, width: Width) -> VReg {
    VReg::new(id, width.class())
}
fn flags(id: u32) -> VReg {
    VReg::new(id, RegClass::Flags)
}
fn module(f: MFunction) -> MModule {
    MModule {
        globals: vec![],
        functions: vec![f],
    }
}

fn branch(width: Width, boolean: Width, cond: Cond, nonzero: bool, bit: bool) -> MFunction {
    let (src, f) = (r(11, boolean), flags(10));
    let term = if bit {
        MTerm::Tb {
            nonzero,
            src,
            bit: 0,
            taken: 1,
            taken_args: vec![r(0, width)],
            untaken: 2,
            untaken_args: vec![r(1, width)],
        }
    } else {
        MTerm::Cb {
            nonzero,
            src,
            taken: 1,
            taken_args: vec![r(0, width)],
            untaken: 2,
            untaken_args: vec![r(1, width)],
        }
    };
    let mut blocks = vec![MBlock {
        id: 0,
        args: vec![r(0, width), r(1, width)],
        insts: vec![
            MInst::Cmp {
                dst: f,
                lhs: r(0, width),
                rhs: r(1, width),
                width,
            },
            MInst::Cset {
                dst: src,
                flags: f,
                cond,
            },
        ],
        term,
    }];
    for (id, arg, imm) in [(1, 20, 7), (2, 30, 3)] {
        blocks.push(MBlock {
            id,
            args: vec![r(arg, width)],
            insts: vec![MInst::BinaryImm {
                op: BinOp::Add,
                dst: r(arg + 1, width),
                lhs: r(arg, width),
                imm,
                width,
            }],
            term: MTerm::Ret {
                values: vec![r(arg + 1, width)],
            },
        });
    }
    MFunction {
        name: "main".into(),
        blocks,
    }
}

fn interval(
    width: Width,
    boolean: Width,
    signed: bool,
    lo: u64,
    hi: u64,
    reverse: bool,
) -> MFunction {
    let (a, b, dst) = (r(11, boolean), r(13, boolean), r(14, boolean));
    MFunction {
        name: "main".into(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![r(0, width)],
            insts: vec![
                MInst::CmpImm {
                    dst: flags(10),
                    lhs: r(0, width),
                    imm: lo,
                    width,
                },
                MInst::Cset {
                    dst: a,
                    flags: flags(10),
                    cond: if signed { Cond::Ge } else { Cond::Hs },
                },
                MInst::CmpImm {
                    dst: flags(12),
                    lhs: r(0, width),
                    imm: hi,
                    width,
                },
                MInst::Cset {
                    dst: b,
                    flags: flags(12),
                    cond: if signed { Cond::Le } else { Cond::Ls },
                },
                MInst::Binary {
                    op: BinOp::And,
                    dst,
                    lhs: if reverse { b } else { a },
                    rhs: if reverse { a } else { b },
                    width: boolean,
                },
            ],
            term: MTerm::Ret { values: vec![dst] },
        }],
    }
}

fn check(f: MFunction, inputs: &[Vec<MirValue>], removed: usize) {
    let (after, counts) = select(&f);
    assert_eq!(counts.removed_instructions, removed);
    let after = after.unwrap();
    assert_eq!(select(&after).0, None, "idempotent");
    let before = module(f);
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
    for input in inputs {
        let a = evocc_mir::interpret_module(&before, "main", input, MirLimits::new(100)).unwrap();
        let b = evocc_mir::interpret_module(&after, "main", input, MirLimits::new(100)).unwrap();
        assert_eq!(a.results, b.results, "{input:?}");
        assert_eq!(a.memory, b.memory);
        assert_eq!(a.world, b.world);
    }
}

const CONDITIONS: [Cond; 14] = [
    Cond::Eq,
    Cond::Ne,
    Cond::Lt,
    Cond::Le,
    Cond::Gt,
    Cond::Ge,
    Cond::Lo,
    Cond::Ls,
    Cond::Hi,
    Cond::Hs,
    Cond::Vs,
    Cond::Vc,
    Cond::Cs,
    Cond::Cc,
];

#[test]
fn branch_conditions_senses_and_register_classes_preserve_edges() {
    for width in [Width::W, Width::X] {
        let mut values = vec![
            0,
            1,
            7,
            u64::MAX,
            1 << (width.bits() - 1),
            (1 << (width.bits() - 1)) - 1,
        ];
        let mut x = 17_u64;
        for _ in 0..32 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            values.push(x);
        }
        let inputs: Vec<_> = values
            .iter()
            .flat_map(|x| {
                values.iter().take(6).map(move |y| {
                    vec![
                        MirValue::Int(*x & width.mask()),
                        MirValue::Int(*y & width.mask()),
                    ]
                })
            })
            .collect();
        for boolean in [Width::W, Width::X] {
            for cond in CONDITIONS {
                for nonzero in [false, true] {
                    for bit in [false, true] {
                        check(branch(width, boolean, cond, nonzero, bit), &inputs, 1);
                    }
                }
            }
        }
    }
}

#[test]
fn inclusive_signed_and_unsigned_intervals_cover_wrap_and_endpoint_boundaries() {
    for width in [Width::W, Width::X] {
        for boolean in [Width::W, Width::X] {
            for signed in [false, true] {
                for reverse in [false, true] {
                    for (lo, hi) in [(0_u64, 0), (0, 4095), (1, 2), (48, 57), (4095, 4095)] {
                        let inputs: Vec<_> = [
                            0,
                            1,
                            lo.wrapping_sub(1),
                            lo,
                            hi,
                            hi + 1,
                            4096,
                            u64::MAX,
                            1 << (width.bits() - 1),
                            (1 << (width.bits() - 1)) - 1,
                        ]
                        .into_iter()
                        .map(|x| vec![MirValue::Int(x & width.mask())])
                        .collect();
                        check(
                            interval(width, boolean, signed, lo, hi, reverse),
                            &inputs,
                            if lo == 0 { 3 } else { 2 },
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn interval_then_branch_has_separate_counts_and_is_idempotent() {
    let mut f = branch(Width::X, Width::W, Cond::Eq, true, true);
    f.blocks[0].insts = interval(Width::X, Width::W, false, 48, 57, false)
        .blocks
        .remove(0)
        .insts;
    if let MTerm::Tb { src, .. } = &mut f.blocks[0].term {
        *src = r(14, Width::W);
    }
    let (after, counts) = select(&f);
    assert!(after.is_some());
    assert_eq!(counts.selected.get("control_boolean_branch"), Some(&1));
    assert_eq!(counts.selected.get("control_constant_interval"), Some(&1));
    let inputs: Vec<_> = [0, 47, 48, 57, 58, u64::MAX]
        .into_iter()
        .map(|x| vec![MirValue::Int(x), MirValue::Int(100)])
        .collect();
    check(f, &inputs, 3);
}

#[test]
fn nonprivate_branch_values_flags_and_other_bits_decline() {
    let original = branch(Width::X, Width::W, Cond::Ne, true, true);
    for case in 0..7 {
        let mut f = original.clone();
        match case {
            0 => {
                if let MTerm::Tb { bit, .. } = &mut f.blocks[0].term {
                    *bit = 1;
                }
            }
            1 => {
                if let MTerm::Tb { src, .. } = &mut f.blocks[0].term {
                    *src = r(0, Width::X);
                }
            }
            2 => f.blocks[1].insts.push(MInst::Mov {
                dst: r(40, Width::W),
                src: r(11, Width::W),
            }),
            3 => {
                if let MTerm::Tb { taken_args, .. } = &mut f.blocks[0].term {
                    taken_args.push(r(11, Width::W));
                }
            }
            4 => f.blocks[1].insts.push(MInst::MovImm {
                dst: r(11, Width::W),
                imm: 0,
            }),
            5 => f.blocks[1].insts.push(MInst::Cset {
                dst: r(41, Width::W),
                flags: flags(10),
                cond: Cond::Eq,
            }),
            _ => f.blocks[0].insts.insert(
                1,
                MInst::Bl {
                    returns_twice: false,
                    symbol: "opaque".into(),
                    args: vec![],
                    results: vec![],
                    fp_mode: None,
                },
            ),
        }
        assert!(select(&f).0.is_none(), "case {case}");
    }
}

#[test]
fn interval_domains_and_observable_intermediates_do_not_expand() {
    let original = interval(Width::X, Width::W, true, 48, 57, false);
    for case in 0..12 {
        let mut f = original.clone();
        match case {
            0 => {
                if let MInst::CmpImm { imm, .. } = &mut f.blocks[0].insts[0] {
                    *imm = 58;
                }
            }
            1 => {
                if let MInst::CmpImm { imm, .. } = &mut f.blocks[0].insts[2] {
                    *imm = 4096;
                }
            }
            2 => {
                if let MInst::CmpImm { lhs, .. } = &mut f.blocks[0].insts[2] {
                    *lhs = r(1, Width::X);
                }
            }
            3 => {
                if let MInst::Cset { cond, .. } = &mut f.blocks[0].insts[1] {
                    *cond = Cond::Gt;
                }
            }
            4 => {
                if let MInst::Cset { cond, .. } = &mut f.blocks[0].insts[3] {
                    *cond = Cond::Ls;
                }
            }
            5 => {
                if let MInst::Binary { op, .. } = &mut f.blocks[0].insts[4] {
                    *op = BinOp::Orr;
                }
            }
            6 => {
                if let MInst::Binary { width, .. } = &mut f.blocks[0].insts[4] {
                    *width = Width::X;
                }
            }
            7 => {
                if let MTerm::Ret { values } = &mut f.blocks[0].term {
                    values.push(r(11, Width::W));
                }
            }
            8 => f.blocks[0].insts.push(MInst::MovImm {
                dst: r(13, Width::W),
                imm: 0,
            }),
            9 => f.blocks[0].insts.push(MInst::Cset {
                dst: r(20, Width::W),
                flags: flags(10),
                cond: Cond::Eq,
            }),
            10 => f.blocks[0].insts.insert(
                2,
                MInst::Bl {
                    returns_twice: false,
                    symbol: "opaque".into(),
                    args: vec![],
                    results: vec![],
                    fp_mode: None,
                },
            ),
            _ => {
                if let MInst::CmpImm { imm, .. } = &mut f.blocks[0].insts[0] {
                    *imm = u64::MAX;
                }
            }
        }
        assert!(select(&f).0.is_none(), "case {case}");
    }
}

#[test]
fn interval_selection_under_register_pressure_keeps_spills_verified() {
    let w = Width::X;
    let mut f = interval(w, w, false, 48, 57, false);
    let prefix = (0..40)
        .map(|i| MInst::BinaryImm {
            op: BinOp::Add,
            dst: r(100 + i, w),
            lhs: r(0, w),
            imm: u64::from(i + 1),
            width: w,
        })
        .collect::<Vec<_>>();
    f.blocks[0].insts.splice(0..0, prefix);
    for i in 0..40 {
        f.blocks[0].insts.push(MInst::Binary {
            op: BinOp::Add,
            dst: r(200 + i, w),
            lhs: r(if i == 0 { 14 } else { 199 + i }, w),
            rhs: r(100 + i, w),
            width: w,
        });
    }
    f.blocks[0].term = MTerm::Ret {
        values: vec![r(239, w)],
    };
    for target in [Target::Darwin, Target::Linux] {
        let destroyed = evocc_mir::out_of_ssa(&f);
        let after = select(&destroyed).0.unwrap();
        let allocated = evocc_mir::allocate_for(target, &after).unwrap();
        assert!(allocated.spilled > 0, "must exercise real spills");
        evocc_mir::verify_allocation_for(target, &allocated).unwrap();
    }
    let inputs = [0, 47, 48, 57, 58, u64::MAX].map(|x| vec![MirValue::Int(x)]);
    check(f, &inputs, 2);
}

#[test]
fn intervening_effects_and_floating_producers_are_opaque() {
    use evocc_evoir::FloatingOp;
    use evocc_types::FloatKind;
    let floating = MInst::Floating {
        strict: true,
        integer_bits: 0,
        dst: VReg::new(50, RegClass::Simd64),
        op: FloatingOp::Add,
        kind: FloatKind::F64,
        args: vec![
            VReg::new(51, RegClass::Simd64),
            VReg::new(52, RegClass::Simd64),
        ],
    };
    let effects = [
        MInst::Load {
            align: 8,
            dst: r(50, Width::X),
            base: r(51, Width::X),
            offset: 0,
            kind: evocc_mir::LoadKind::X,
            volatile: true,
            mask: None,
            transport: false,
        },
        MInst::Bl {
            returns_twice: false,
            symbol: "opaque".into(),
            args: vec![],
            results: vec![],
            fp_mode: None,
        },
        floating.clone(),
    ];
    for effect in effects {
        let mut f = interval(Width::X, Width::W, false, 1, 2, false);
        f.blocks[0].insts.insert(2, effect.clone());
        assert!(select(&f).0.is_none());
        let mut f = branch(Width::X, Width::W, Cond::Ne, true, false);
        f.blocks[0].insts.insert(1, effect);
        assert!(select(&f).0.is_none());
    }
    let mut f = branch(Width::X, Width::W, Cond::Ne, true, false);
    f.blocks[0].insts[0] = floating;
    assert!(select(&f).0.is_none());
}

#[test]
fn selection_option_lists_have_stable_identity_and_refuse_duplicates() {
    use evocc_mir::emit::SelectionVariants;
    let names = ["constants", "local", "extensions", "controls"];
    for mask in 1..16 {
        let list: Vec<_> = names
            .iter()
            .enumerate()
            .filter_map(|(bit, n)| (mask & (1 << bit) != 0).then_some(*n))
            .collect();
        let canonical = list.join(",");
        let reverse = list.into_iter().rev().collect::<Vec<_>>().join(",");
        assert_eq!(
            SelectionVariants::parse(&canonical).unwrap(),
            SelectionVariants::parse(&reverse).unwrap()
        );
        assert_eq!(
            SelectionVariants::parse(&reverse).unwrap().name(),
            Some(canonical.as_str())
        );
    }
    for list in ["controls,controls", "controls,", "unknown,controls", ""] {
        assert!(SelectionVariants::parse(list).is_err());
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_control_families_match_independent_integer_expressions() {
    use std::process::Command;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join(format!(
        ".evidence/codegen/n03d-c5/native-mir-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::create_dir(&dir).unwrap();
    let mut functions = Vec::new();
    let mut declarations = String::new();
    let mut checks = String::new();
    let mut add =
        |f: MFunction, input_width: Width, width: Width, arity: usize, expected: String| {
            let ctype = if width == Width::W {
                "unsigned"
            } else {
                "unsigned long"
            };
            let argtype = if input_width == Width::W {
                "unsigned"
            } else {
                "unsigned long"
            };
            let index = functions.len();
            let after = select(&f).0.unwrap();
            for (tag, mut function) in [("before", f), ("after", after)] {
                function.name = format!("control_{index}_{tag}");
                let args = if arity == 1 {
                    argtype.to_string()
                } else {
                    format!("{argtype},{argtype}")
                };
                let call = if arity == 1 {
                    "x".to_string()
                } else {
                    "x,y".to_string()
                };
                declarations.push_str(&format!("extern {ctype} {}({args});\n", function.name));
                checks.push_str(&format!(
                    "if({}({call})!=({ctype})({expected}))return 1;\n",
                    function.name
                ));
                functions.push(function);
            }
        };
    for width in [Width::W, Width::X] {
        let (a, b, sa, sb) = if width == Width::W {
            ("(unsigned)x", "(unsigned)y", "(int)x", "(int)y")
        } else {
            ("x", "y", "(long)x", "(long)y")
        };
        for boolean in [Width::W, Width::X] {
            for cond in CONDITIONS {
                let test = match cond {
                    Cond::Eq => format!("{a}=={b}"),
                    Cond::Ne => format!("{a}!={b}"),
                    Cond::Lt => format!("{sa}<{sb}"),
                    Cond::Le => format!("{sa}<={sb}"),
                    Cond::Gt => format!("{sa}>{sb}"),
                    Cond::Ge => format!("{sa}>={sb}"),
                    Cond::Lo | Cond::Cc => format!("{a}<{b}"),
                    Cond::Ls => format!("{a}<={b}"),
                    Cond::Hi => format!("{a}>{b}"),
                    Cond::Hs | Cond::Cs => format!("{a}>={b}"),
                    Cond::Vs | Cond::Vc => format!(
                        "(((({a})^({b}))&(({a})^(({a})-({b}))))&{}UL){}0",
                        1_u64 << (width.bits() - 1),
                        if cond == Cond::Vs { "!=" } else { "==" }
                    ),
                };
                for nonzero in [false, true] {
                    for bit in [false, true] {
                        let test = if nonzero {
                            test.clone()
                        } else {
                            format!("!({test})")
                        };
                        add(
                            branch(width, boolean, cond, nonzero, bit),
                            width,
                            width,
                            2,
                            format!("({test})?({a})+7UL:({b})+3UL"),
                        );
                    }
                }
            }
        }
        for boolean in [Width::W, Width::X] {
            for signed in [false, true] {
                for reverse in [false, true] {
                    for (lo, hi) in [(0, 0), (0, 4095), (1, 2), (48, 57), (4095, 4095)] {
                        let x = if signed { sa } else { a };
                        add(
                            interval(width, boolean, signed, lo, hi, reverse),
                            width,
                            boolean,
                            1,
                            format!("({x})>={lo}L && ({x})<={hi}L"),
                        );
                    }
                }
            }
        }
    }
    assert_eq!(functions.len(), 608);
    for (target, triple, file) in [
        (Target::Darwin, "arm64-apple-macosx11.0.0", "darwin"),
        (Target::Linux, "aarch64-linux-gnu", "linux"),
    ] {
        let allocated: Vec<_> = functions
            .iter()
            .map(|f| {
                let allocated = evocc_mir::allocate_for(target, &evocc_mir::out_of_ssa(f)).unwrap();
                evocc_mir::verify_allocation_for(target, &allocated).unwrap();
                allocated
            })
            .collect();
        std::fs::write(
            dir.join(format!("{file}.s")),
            evocc_mir::emit::emit_module_for(target, &allocated, &[]),
        )
        .unwrap();
        let output = Command::new("clang")
            .args([
                "-target",
                triple,
                "-c",
                &format!("{file}.s"),
                "-o",
                &format!("{file}.o"),
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
    let harness = format!("{declarations}\nint main(void){{unsigned long values[]={{0,1,47,48,57,58,101,102,107,108,4095,4096,~0UL,0x80000000UL,0x7fffffffUL,0x8000000000000000UL,0x7fffffffffffffffUL,0x100000030UL,0xffffffff00000030UL}};unsigned long random=~0UL,y=0;for(unsigned n=0;n<1024;++n){{unsigned long x=n<sizeof(values)/sizeof(*values)?values[n]:random;{checks}\nrandom=random*6364136223846793005UL+1UL;y=y*1442695040888963407UL+1UL;}}return 0;}}\n");
    std::fs::write(dir.join("harness.c"), harness).unwrap();
    let output = Command::new("clang")
        .args(["-O2", "darwin.o", "harness.c", "-o", "probe"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(dir.join("probe")).output().unwrap();
    assert!(output.status.success(), "{}: {output:?}", dir.display());
    println!("304 control shapes / 608 before-after functions, 1024 Darwin inputs each; all assemble for Linux");
    std::fs::remove_dir_all(dir).unwrap();
}
