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

use evocc_mir::{
    allocate, out_of_ssa, render_allocated, BinOp, MBlock, MFunction, MInst, MTerm, RegClass, VReg,
    Width, CALLEE_SAVED, CALLER_SAVED,
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
                    dst: w(2),
                    base: g(0),
                    offset: 0,
                    kind: evocc_mir::LoadKind::W,
                    volatile: false,
                },
                MInst::Load {
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
