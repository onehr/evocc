//! Section 7, linear scan.
//!
//! "Instructions are numbered in block order; a virtual register's live
//! interval is from its definition to its last use, extended across any block
//! in which it is live on entry to cover the whole block. The allocator walks
//! intervals by start point, assigns a free physical register of the class,
//! and on exhaustion spills the interval with the furthest next use to a frame
//! slot ... Fixed operands (argument and result registers at calls and at the
//! function boundary, `x16`/`x17` in emitted sequences) are pre-assigned
//! intervals the allocator must respect; a call clobbers every caller-saved
//! register, so an interval live across a call is assigned a callee-saved
//! register or spilled. Ties are broken by virtual register number, so the
//! allocation is deterministic."

use evocc_evoir::parse_section9_module;
use evocc_mir::{
    allocate, lower_module, out_of_ssa, render_allocated, verify_allocation, BinOp, Cond, MBlock,
    MFunction, MInst, MTerm, RegClass, VReg, Width, CALLEE_SAVED, CALLER_SAVED,
};

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

fn g(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr64)
}

/// A chain of adds over `count` values that are all defined before any is
/// read, so the pressure is exactly `count` simultaneously live registers.
fn pressure(count: u32) -> MFunction {
    let mut insts = Vec::new();
    for index in 0..count {
        insts.push(MInst::MovImm {
            dst: w(index),
            imm: u64::from(index),
        });
    }
    let mut sum = w(0);
    for index in 1..count {
        let next = w(count + index);
        insts.push(MInst::Binary {
            op: BinOp::Add,
            dst: next,
            lhs: sum,
            rhs: w(index),
            width: Width::W,
        });
        sum = next;
    }
    MFunction {
        name: "main".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: Vec::new(),
            insts,
            term: MTerm::Ret { values: vec![sum] },
        }],
    }
}

#[test]
fn the_boundary_registers_are_fixed() {
    let function = MFunction {
        name: "main".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![w(0), w(1)],
            insts: vec![MInst::Binary {
                op: BinOp::Add,
                dst: w(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            }],
            term: MTerm::Ret { values: vec![w(2)] },
        }],
    };
    let allocated = allocate(&out_of_ssa(&function)).expect("allocation");
    let text = render_allocated(&allocated);
    assert!(text.contains("w0"), "{text}");
    assert!(text.contains("w1"), "{text}");
    assert!(
        text.contains("    ret\n"),
        "a return names no register: {text}"
    );
    assert!(
        allocated.blocks.iter().all(|block| block.args.is_empty()),
        "block arguments are gone after allocation"
    );
}

#[test]
fn a_value_live_across_a_call_is_callee_saved_or_spilled() {
    let function = MFunction {
        name: "main".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![w(0)],
            insts: vec![
                MInst::Bl {
                    returns_twice: false,
                    fp_mode: None,
                    symbol: "other".to_owned(),
                    args: Vec::new(),
                    results: vec![w(1)],
                },
                MInst::Binary {
                    op: BinOp::Add,
                    dst: w(2),
                    lhs: w(0),
                    rhs: w(1),
                    width: Width::W,
                },
            ],
            term: MTerm::Ret { values: vec![w(2)] },
        }],
    };
    let allocated = allocate(&out_of_ssa(&function)).expect("allocation");
    let text = render_allocated(&allocated);
    let saved = CALLEE_SAVED
        .iter()
        .any(|number| text.contains(&format!("w{number}")) || text.contains(&format!("x{number}")));
    assert!(
        saved || allocated.spilled > 0,
        "nothing callee-saved and nothing spilled: {text}"
    );
}

#[test]
fn exhausting_the_class_spills_to_a_frame_slot() {
    let allocated = allocate(&out_of_ssa(&pressure(40))).expect("allocation");
    assert!(allocated.spilled > 0, "forty live values fit in no class");
    assert_eq!(
        allocated.frame_slots as usize, allocated.spilled,
        "one eight-byte slot per spilled register"
    );
}

#[test]
fn disjoint_pressure_regions_reuse_spill_storage() {
    let mut insts = Vec::new();
    for group in 0..32 {
        let base = group * 100;
        for index in 0..40 {
            insts.push(MInst::MovImm {
                dst: w(base + index),
                imm: u64::from(index),
            });
        }
        let mut sum = w(base);
        for index in 1..40 {
            let dst = w(base + 40 + index);
            insts.push(MInst::Binary {
                op: BinOp::Add,
                dst,
                lhs: sum,
                rhs: w(base + index),
                width: Width::W,
            });
            sum = dst;
        }
        insts.push(MInst::Bl {
            returns_twice: false,
            fp_mode: None,
            symbol: "consume".into(),
            args: vec![sum],
            results: Vec::new(),
        });
    }
    let function = MFunction {
        name: "separate_pressure".into(),
        blocks: vec![MBlock {
            id: 0,
            args: Vec::new(),
            insts,
            term: MTerm::Ret { values: Vec::new() },
        }],
    };
    let allocated = allocate(&function).expect("each region can spill");
    assert!(
        allocated.spilled > 40,
        "the fixture must create many distinct spilled values"
    );
    assert!(
        allocated.frame_slots <= 40,
        "disjoint regions retained {} physical spill slots",
        allocated.frame_slots
    );
    verify_allocation(&allocated).expect("a reload sees the correct stored value after slot reuse");
}

#[test]
fn mutually_exclusive_spills_do_not_depend_on_block_layout() {
    // Definitions in all arms precede all their consuming blocks in the
    // layout, as happens when out-of-SSA appends edge-copy blocks. Their
    // convex textual intervals overlap; their executed live ranges do not.
    let mut blocks = Vec::new();
    for arm in 0..24 {
        let flags = VReg::new(10 + arm, RegClass::Flags);
        blocks.push(MBlock {
            id: arm,
            args: if arm == 0 { vec![w(0)] } else { Vec::new() },
            insts: vec![MInst::CmpImm {
                dst: flags,
                lhs: w(0),
                imm: u64::from(arm),
                width: Width::W,
            }],
            term: MTerm::BCond {
                flags,
                cond: Cond::Eq,
                taken: 100 + arm,
                taken_args: Vec::new(),
                untaken: if arm == 23 { 100 + arm } else { arm + 1 },
                untaken_args: Vec::new(),
            },
        });
    }
    for arm in 0..24 {
        let base = 100 + arm * 100;
        blocks.push(MBlock {
            id: 100 + arm,
            args: Vec::new(),
            insts: (0..40)
                .map(|index| MInst::MovImm {
                    dst: w(base + index),
                    imm: u64::from(index + arm),
                })
                .collect(),
            term: MTerm::B {
                target: 200 + arm,
                args: Vec::new(),
            },
        });
    }
    for arm in 0..24 {
        let base = 100 + arm * 100;
        let mut sum = w(base);
        let mut insts = Vec::new();
        for index in 1..40 {
            let dst = w(base + 40 + index);
            insts.push(MInst::Binary {
                op: BinOp::Add,
                dst,
                lhs: sum,
                rhs: w(base + index),
                width: Width::W,
            });
            sum = dst;
        }
        blocks.push(MBlock {
            id: 200 + arm,
            args: Vec::new(),
            insts,
            term: MTerm::Ret { values: vec![sum] },
        });
    }
    let function = MFunction {
        name: "branch_pressure".into(),
        blocks,
    };
    let allocated = allocate(&function).expect("each arm can spill");
    verify_allocation(&allocated).expect("every branch reloads its own values");
    // Each arm has forty simultaneously live values. Do not require the
    // extra spills caused by overlapping interval hulls of different arms.
    let minimum_per_arm = 40 - evocc_mir::isa::aarch64::regfile::GPR_ALLOCATABLE.len();
    assert!(allocated.spilled >= 24 * minimum_per_arm);
    for arm in 0..24 {
        let block = allocated
            .blocks
            .iter()
            .find(|block| block.id == 100 + arm)
            .unwrap();
        let stores = block
            .insts
            .iter()
            .filter(|inst| matches!(inst, MInst::SpillStore { .. }))
            .count();
        assert!(
            stores >= minimum_per_arm,
            "arm {arm} must exercise spill storage"
        );
    }
    assert!(
        allocated.frame_slots <= 40,
        "mutually exclusive arms retained {} physical spill slots",
        allocated.frame_slots
    );
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let mut insts = vec![MInst::MovImm { dst: w(0), imm: 0 }];
        let mut errors = w(0);
        for arm in 0..24 {
            let base = 10 + arm * 10;
            insts.push(MInst::MovImm {
                dst: w(base),
                imm: u64::from(arm),
            });
            insts.push(MInst::Bl {
                returns_twice: false,
                fp_mode: None,
                symbol: "branch_pressure".into(),
                args: vec![w(base)],
                results: vec![w(base + 1)],
            });
            insts.push(MInst::MovImm {
                dst: w(base + 2),
                imm: u64::from(780 + 40 * arm),
            });
            insts.push(MInst::Binary {
                op: BinOp::Eor,
                dst: w(base + 3),
                lhs: w(base + 1),
                rhs: w(base + 2),
                width: Width::W,
            });
            insts.push(MInst::Binary {
                op: BinOp::Orr,
                dst: w(base + 4),
                lhs: errors,
                rhs: w(base + 3),
                width: Width::W,
            });
            errors = w(base + 4);
        }
        let main = MFunction {
            name: "main".into(),
            blocks: vec![MBlock {
                id: 0,
                args: Vec::new(),
                insts,
                term: MTerm::Ret {
                    values: vec![errors],
                },
            }],
        };
        let main = allocate(&main).expect("native checker allocation");
        verify_allocation(&main).expect("native checker registers");
        let text = evocc_mir::emit_module(&[allocated, main], &[]);
        assert_eq!(
            evocc_mir::Toolchain::locate()
                .unwrap()
                .exit_status("branch-spill-reuse", &text)
                .unwrap(),
            0
        );
    }
}

#[test]
fn live_spill_contents_survive_backedges_and_dead_stores() {
    let function = MFunction {
        name: "main".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: Vec::new(),
                insts: vec![
                    MInst::MovImm { dst: w(0), imm: 7 },
                    MInst::SpillStore { src: w(0), slot: 0 },
                    MInst::MovImm { dst: w(1), imm: 3 },
                    MInst::SpillStore { src: w(1), slot: 1 },
                ],
                term: MTerm::B {
                    target: 1,
                    args: Vec::new(),
                },
            },
            MBlock {
                id: 1,
                args: Vec::new(),
                insts: vec![
                    MInst::SpillReload { dst: w(2), slot: 1 },
                    MInst::MovImm { dst: w(3), imm: 1 },
                    MInst::Binary {
                        op: BinOp::Sub,
                        dst: w(4),
                        lhs: w(2),
                        rhs: w(3),
                        width: Width::W,
                    },
                    MInst::SpillStore { src: w(4), slot: 1 },
                    MInst::MovImm { dst: w(5), imm: 99 },
                    // Dead does not mean removed: it must not overwrite
                    // either the loop counter or the value carried through.
                    MInst::SpillStore { src: w(5), slot: 2 },
                ],
                term: MTerm::Cb {
                    nonzero: true,
                    src: w(4),
                    taken: 1,
                    taken_args: Vec::new(),
                    untaken: 2,
                    untaken_args: Vec::new(),
                },
            },
            MBlock {
                id: 2,
                args: Vec::new(),
                insts: vec![
                    MInst::SpillReload { dst: w(6), slot: 0 },
                    MInst::MovImm { dst: w(7), imm: 7 },
                    MInst::Binary {
                        op: BinOp::Sub,
                        dst: w(8),
                        lhs: w(6),
                        rhs: w(7),
                        width: Width::W,
                    },
                ],
                term: MTerm::Ret { values: vec![w(8)] },
            },
        ],
    };
    let allocated = allocate(&function).expect("loop allocation");
    verify_allocation(&allocated).expect("loop registers");
    assert_eq!(
        allocated.frame_slots, 3,
        "all three stores interfere on the loop edge"
    );
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let text = evocc_mir::emit_module(&[allocated], &[]);
        assert_eq!(
            evocc_mir::Toolchain::locate()
                .unwrap()
                .exit_status("loop-spill-reuse", &text)
                .unwrap(),
            0
        );
    }
}

#[test]
fn a_reserved_register_is_never_assigned() {
    let allocated = allocate(&out_of_ssa(&pressure(40))).expect("allocation");
    for number in allocated.assignment.values() {
        assert!(
            CALLER_SAVED.contains(number) || CALLEE_SAVED.contains(number) || *number == 16,
            "register {number} is outside the allocatable set and is not the scratch"
        );
        assert!(*number != 18, "x18 is reserved by the platform");
        assert!(*number != 29 && *number != 30, "the frame pair is reserved");
    }
}

#[test]
fn the_allocation_is_a_function_of_its_input() {
    for count in [3, 12, 40] {
        let input = out_of_ssa(&pressure(count));
        let one = render_allocated(&allocate(&input).expect("allocation"));
        let two = render_allocated(&allocate(&input).expect("allocation"));
        assert_eq!(one, two, "two runs differ at pressure {count}");
    }
}

#[test]
fn short_lived_values_do_not_accumulate_interference_history() {
    let mut insts = Vec::new();
    for index in 0..16_384 {
        insts.push(MInst::MovImm {
            dst: w(index),
            imm: u64::from(index),
        });
        insts.push(MInst::Bl {
            returns_twice: false,
            fp_mode: None,
            symbol: "consume".to_owned(),
            args: vec![w(index)],
            results: Vec::new(),
        });
    }
    let function = MFunction {
        name: "sequential_calls".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: Vec::new(),
            insts,
            term: MTerm::Ret { values: Vec::new() },
        }],
    };
    let started = std::time::Instant::now();
    let allocated = allocate(&function).expect("one live value requires no spills");
    let elapsed = started.elapsed();
    assert_eq!(allocated.spilled, 0);
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "allocation retained expired or scanned all future fixed intervals: {elapsed:?}"
    );
    verify_allocation(&allocated).expect("fixed call operands remain valid");
}

#[test]
fn the_cycle_scratch_is_pinned_to_x16() {
    let function = MFunction {
        name: "main".to_owned(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![g(0), g(1)],
                insts: Vec::new(),
                term: MTerm::B {
                    target: 1,
                    args: vec![g(0), g(1)],
                },
            },
            MBlock {
                id: 1,
                args: vec![g(2), g(3)],
                insts: Vec::new(),
                term: MTerm::Tb {
                    nonzero: true,
                    src: g(2),
                    bit: 0,
                    taken: 1,
                    taken_args: vec![g(3), g(2)],
                    untaken: 2,
                    untaken_args: vec![g(2)],
                },
            },
            MBlock {
                id: 2,
                args: vec![g(4)],
                insts: Vec::new(),
                term: MTerm::Ret { values: vec![g(4)] },
            },
        ],
    };
    let allocated = allocate(&out_of_ssa(&function)).expect("allocation");
    let text = render_allocated(&allocated);
    assert!(text.contains("x16"), "{text}");
}

#[test]
fn the_allocated_form_names_physical_registers_and_no_block_arguments() {
    // Appendix A's second example: labels without argument lists, physical
    // registers, and a bare return.
    let function = MFunction {
        name: "dot".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![g(0), g(1)],
            insts: vec![
                MInst::Load {
                    align: 0,
                    dst: w(2),
                    base: g(0),
                    offset: 0,
                    kind: evocc_mir::LoadKind::W,
                    volatile: false,
                },
                MInst::Load {
                    align: 0,
                    dst: w(3),
                    base: g(1),
                    offset: 0,
                    kind: evocc_mir::LoadKind::W,
                    volatile: false,
                },
                MInst::Binary {
                    op: BinOp::Mul,
                    dst: w(4),
                    lhs: w(2),
                    rhs: w(3),
                    width: Width::W,
                },
            ],
            term: MTerm::Ret { values: vec![w(4)] },
        }],
    };
    let text = render_allocated(&allocate(&out_of_ssa(&function)).expect("allocation"));
    assert!(text.starts_with("func @dot {\nentry:\n"), "{text}");
    assert!(text.contains("ldr.w [x0, #0]"), "{text}");
    assert!(text.ends_with("    ret\n}\n"), "{text}");
}

/// The reduced program the campaign refused: a loop whose back edge leaves a
/// join block.
///
/// `^b6` reads none of `^b1`'s arguments and writes all of them, so after the
/// out-of-SSA pass those values leave the block without entering it. An
/// interval was stretched to the end of a block only for a value that entered
/// it, so each of them expired at the copy that wrote it, and under pressure
/// the register went to the spill store standing right after the copies while
/// the loop still needed it. The allocation verifier refused a hundred and
/// twenty of the first two hundred and fifty-six generated programs on this;
/// this is the reduction of the first of them.
const CARRIED_AROUND_A_LOOP: &str = "\
func @h0(%arg0: mem<all>, %arg1: world) -> (bits<32>, bits<64>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=0, size=8, align=8]
    %v5 = const.bits<64> 1
    %v6 = access_in_bounds %v3, %v2 [size=8, align=8]
    (%v7, %v8) = store %v6, %v4, %v2, %v5
    %v12 = const.bits<17> 0
    %v13 = const.bits<1> 0
    %v18 = const.bits<64> 0
    %v27 = const.bits<64> 0
    %v28 = const.bits<64> 2
    %v29 = const.bits<32> 0
    %v30 = const.bits<47> 0
    br ^b1(%v27, %v28, %v2, %v13, %v12, %v29, %v30, %v18, %v7, %v8)

^b1(%v31: bits<64>, %v32: bits<64>, %v33: ptr<generic>, %v34: bits<1>, %v35: bits<17>, %v36: bits<32>, %v37: bits<47>, %v38: bits<64>, %v39: mem<all>, %v40: world):
    %v54 = const.bits<17> 63
    %v55 = const.bits<17> 0
    %v56 = const.bits<1> 0
    %v57 = const.bits<64> 0
    %v58 = const.bits<1> 0
    %v60 = const.bits<32> 4231669572
    cond_br %v58, ^b4(%v31, %v32, %v33, %v34, %v54, %v36, %v37, %v32, %v39, %v40), ^b5(%v31, %v32, %v33, %v56, %v55, %v60, %v37, %v57, %v39, %v40)

^b4(%v61: bits<64>, %v62: bits<64>, %v63: ptr<generic>, %v64: bits<1>, %v65: bits<17>, %v66: bits<32>, %v67: bits<47>, %v68: bits<64>, %v69: mem<all>, %v70: world):
    %v76 = const.bits<47> 0
    %v79 = const.bits<64> 0
    br ^b6(%v61, %v62, %v63, %v64, %v65, %v66, %v76, %v79, %v69, %v70)

^b5(%v80: bits<64>, %v81: bits<64>, %v82: ptr<generic>, %v83: bits<1>, %v84: bits<17>, %v85: bits<32>, %v86: bits<47>, %v87: bits<64>, %v88: mem<all>, %v89: world):
    %v94 = signed_shl_defined.unsigned %v84, %v84
    (%v95, %v96, %v97) = alloca %v88, %v89 [origin=1, size=4, align=2]
    %v98 = const.bits<64> 1
    %v99 = ptr.offset.add.unsigned %v95, %v98 [scale=2]
    %v100 = pointer_valid %v96, %v99
    %v101 = const.bits<16> 11692
    %v102 = access_in_bounds %v100, %v95 [size=2, align=2]
    (%v103, %v104) = store %v102, %v97, %v95, %v101
    %v105 = access_in_bounds %v103, %v99 [size=2, align=2]
    (%v106, %v107) = store %v105, %v104, %v99, %v101
    (%v108, %v109, %v110) = alloca %v106, %v107 [origin=2, size=2, align=2]
    %v111 = access_in_bounds %v109, %v108 [size=2, align=2]
    (%v112, %v113) = store %v111, %v110, %v108, %v101
    (%v114, %v115) = copy %v112, %v113, %v95, %v108 [size=2, align=2]
    %v117 = const.bits<64> 0
    %v119 = ashr.bits<47> %v86, %v117
    %v120 = signed_div_rem_no_overflow %v84, %v84
    %v121 = const.bits<17> 0
    %v126 = const.bits<1> 0
    br ^b6(%v80, %v81, %v82, %v126, %v121, %v85, %v119, %v98, %v114, %v115)

^b6(%v129: bits<64>, %v130: bits<64>, %v131: ptr<generic>, %v132: bits<1>, %v133: bits<17>, %v134: bits<32>, %v135: bits<47>, %v136: bits<64>, %v137: mem<all>, %v138: world):
    %v140 = const.bits<17> 0
    %v143 = const.bits<32> 0
    %v155 = const.bits<64> 0
    br ^b1(%v155, %v130, %v131, %v132, %v140, %v143, %v135, %v155, %v137, %v138)
}
";

#[test]
fn a_value_carried_around_a_loop_keeps_its_register() {
    let module = parse_section9_module(CARRIED_AROUND_A_LOOP.as_bytes()).expect("canonical EvoIR");
    let lowered = lower_module(&module).expect("every form is in the Section 5 table");
    for function in &lowered.functions {
        let mut function = function.clone();
        // Keep the historical pressure observable: unused transport is now
        // eliminated, so relying on dead parameters would no longer exercise
        // the spill store overwriting a value defined on the back edge.
        let first = function
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .args
                    .iter()
                    .copied()
                    .chain(block.insts.iter().flat_map(evocc_mir::inst_defs))
            })
            .map(|reg| reg.id)
            .max()
            .unwrap()
            + 1;
        let carried: Vec<_> = (first..first + 32).map(g).collect();
        function.blocks[0].insts.extend(
            carried
                .iter()
                .enumerate()
                .map(|(i, &dst)| MInst::MovImm { dst, imm: i as u64 }),
        );
        let header = function
            .blocks
            .iter_mut()
            .find(|block| block.id == 1)
            .unwrap();
        header.insts.push(MInst::Bl {
            returns_twice: false,
            fp_mode: None,
            symbol: "observe".into(),
            args: header.args.clone(),
            results: Vec::new(),
        });
        for args in carried.chunks(8) {
            header.insts.push(MInst::Bl {
                returns_twice: false,
                fp_mode: None,
                symbol: "observe".into(),
                args: args.to_vec(),
                results: Vec::new(),
            });
        }
        let allocated = allocate(&out_of_ssa(&function)).expect("allocation");
        assert!(
            allocated.spilled > 0,
            "the reproducer must still spill to reach the defect"
        );
        verify_allocation(&allocated).expect("the loop-carried values keep their registers");
    }
}

/// A value that lives through a block laid out before any block that mentions
/// it keeps its register there.
///
/// The interval extension ran inside the pass that discovers definitions and
/// uses, so it reached only the values some earlier block had already
/// mentioned. A loop whose blocks are laid out starting in the middle produces
/// the value it cannot reach: `^b1` is entered only from `^b3`, and the value
/// `^b3` defines and `^b2` reads lives across `^b1` while its first mention
/// comes after it. The register then went to a dead flag-setting result inside
/// `^b1`. Thirty-two of the 128 Day 7 programs were refused for this.
#[test]
fn a_value_living_through_an_earlier_block_keeps_its_register() {
    let function = MFunction {
        name: "midloop".to_owned(),
        blocks: vec![
            // The entry enters the cycle at its last block.
            MBlock {
                id: 0,
                args: Vec::new(),
                insts: vec![MInst::FrameSlot {
                    dst: g(100),
                    origin: 0,
                    size: 8,
                    align: 8,
                }],
                term: MTerm::B {
                    target: 3,
                    args: Vec::new(),
                },
            },
            // Laid out first of the cycle and entered only from its last block.
            // The flag-setting form defines a register nothing reads, which is
            // what took the carried value's register.
            MBlock {
                id: 1,
                args: Vec::new(),
                insts: vec![
                    MInst::MovImm { dst: w(10), imm: 1 },
                    MInst::FlagBinary {
                        op: evocc_mir::FlagOp::Adds,
                        dst: w(11),
                        flags: VReg::new(12, RegClass::Flags),
                        lhs: w(10),
                        rhs: w(10),
                        width: Width::W,
                    },
                    MInst::Cset {
                        dst: w(13),
                        flags: VReg::new(12, RegClass::Flags),
                        cond: evocc_mir::Cond::Vc,
                    },
                ],
                term: MTerm::Tb {
                    nonzero: true,
                    src: w(13),
                    bit: 0,
                    taken: 2,
                    taken_args: Vec::new(),
                    untaken: 4,
                    untaken_args: Vec::new(),
                },
            },
            // Reads the carried value, whose only definition is below.
            MBlock {
                id: 2,
                args: Vec::new(),
                insts: vec![MInst::Mov {
                    dst: g(20),
                    src: g(50),
                }],
                term: MTerm::B {
                    target: 3,
                    args: Vec::new(),
                },
            },
            // Defines it, after both blocks that need it.
            MBlock {
                id: 3,
                args: Vec::new(),
                insts: vec![MInst::Mov {
                    dst: g(50),
                    src: g(100),
                }],
                term: MTerm::B {
                    target: 1,
                    args: Vec::new(),
                },
            },
            MBlock {
                id: 4,
                args: Vec::new(),
                insts: Vec::new(),
                term: MTerm::Ret { values: Vec::new() },
            },
        ],
    };

    let allocated = allocate(&out_of_ssa(&function)).expect("allocation");
    verify_allocation(&allocated)
        .expect("the value carried across the first block keeps its register");
}
