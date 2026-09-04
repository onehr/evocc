//! Section 9, instrument two: "A separate pass over the allocated function
//! checks the three properties of Section 7 by dataflow; the allocator does
//! not certify itself."
//!
//! Those properties are: "every physical register read is defined on every
//! path, no two simultaneously live values share a register, and every `flags`
//! producer is adjacent to its consumer."

use evocc_mir::{
    allocate, out_of_ssa, verify_allocation, BinOp, Cond, MBlock, MFunction, MInst, MTerm,
    RegClass, VReg, Width,
};

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

fn f(id: u32) -> VReg {
    VReg::new(id, RegClass::Flags)
}

/// Section 6 says where an argument arrives, not that the body reads it.
///
/// The verifier's first rule required as many values live on entry as the
/// function has arguments, so a function that ignores one was refused with
/// `E4020` -- a hundred and ten of the first two hundred and fifty-six
/// generated programs, whose helpers take arguments they never use.
#[test]
fn a_function_may_ignore_an_argument() {
    let function = MFunction {
        name: "ignores".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![w(0), w(1), w(2)],
            insts: vec![MInst::Binary {
                op: BinOp::Add,
                dst: w(3),
                lhs: w(1),
                rhs: w(1),
                width: Width::W,
            }],
            term: MTerm::Ret { values: vec![w(3)] },
        }],
    };
    let allocated = allocate(&out_of_ssa(&function)).expect("allocation");
    verify_allocation(&allocated).expect("ignoring the first and third argument is not a fault");
}

fn straight() -> MFunction {
    MFunction {
        name: "main".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![w(0), w(1)],
            insts: vec![
                MInst::Binary {
                    op: BinOp::Add,
                    dst: w(2),
                    lhs: w(0),
                    rhs: w(1),
                    width: Width::W,
                },
                MInst::Cmp {
                    dst: f(3),
                    lhs: w(2),
                    rhs: w(0),
                    width: Width::W,
                },
                MInst::Cset {
                    dst: w(4),
                    flags: f(3),
                    cond: Cond::Lo,
                },
            ],
            term: MTerm::Ret { values: vec![w(4)] },
        }],
    }
}

fn branching() -> MFunction {
    MFunction {
        name: "main".to_owned(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![w(0)],
                insts: Vec::new(),
                term: MTerm::Tb {
                    nonzero: true,
                    src: w(0),
                    bit: 0,
                    taken: 1,
                    taken_args: vec![w(0)],
                    untaken: 2,
                    untaken_args: vec![w(0)],
                },
            },
            MBlock {
                id: 1,
                args: vec![w(1)],
                insts: vec![MInst::Binary {
                    op: BinOp::Add,
                    dst: w(2),
                    lhs: w(1),
                    rhs: w(1),
                    width: Width::W,
                }],
                term: MTerm::B {
                    target: 3,
                    args: vec![w(2)],
                },
            },
            MBlock {
                id: 2,
                args: vec![w(3)],
                insts: vec![MInst::Binary {
                    op: BinOp::Sub,
                    dst: w(4),
                    lhs: w(3),
                    rhs: w(3),
                    width: Width::W,
                }],
                term: MTerm::B {
                    target: 3,
                    args: vec![w(4)],
                },
            },
            MBlock {
                id: 3,
                args: vec![w(5)],
                insts: Vec::new(),
                term: MTerm::Ret { values: vec![w(5)] },
            },
        ],
    }
}

fn calling() -> MFunction {
    MFunction {
        name: "main".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![w(0)],
            insts: vec![
                MInst::Bl {
                    symbol: "other".to_owned(),
                    args: vec![w(0)],
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
    }
}

#[test]
fn a_sound_allocation_is_accepted() {
    for function in [straight(), branching(), calling()] {
        let allocated = allocate(&out_of_ssa(&function)).expect("allocation");
        assert_eq!(verify_allocation(&allocated), Ok(()), "{}", function.name);
    }
}

#[test]
fn a_register_read_before_it_is_written_is_refused() {
    let mut allocated = allocate(&out_of_ssa(&straight())).expect("allocation");
    // Read a register nothing writes on the way here.
    allocated.blocks[0].insts.insert(
        0,
        MInst::Binary {
            op: BinOp::Add,
            dst: w(91),
            lhs: w(92),
            rhs: w(92),
            width: Width::W,
        },
    );
    allocated.assignment.insert(91, 6);
    allocated.assignment.insert(92, 7);
    let refusal = verify_allocation(&allocated).expect_err("the read is undefined");
    assert_eq!(refusal.code, "E4020");
    assert_eq!(refusal.name, "allocation: verifier-refused");
}

#[test]
fn two_live_values_in_one_register_are_refused() {
    let mut allocated = allocate(&out_of_ssa(&branching())).expect("allocation");
    // Drop the last instruction of one arm, so the register the join reads
    // holds one value on one path and something else on the other.
    let arm = allocated
        .blocks
        .iter_mut()
        .find(|block| block.id == 1)
        .expect("the first arm");
    arm.insts.pop();
    let refusal = verify_allocation(&allocated).expect_err("the paths disagree about the register");
    assert_eq!(refusal.code, "E4020");
}

#[test]
fn a_clobbered_value_is_refused() {
    let mut allocated = allocate(&out_of_ssa(&calling())).expect("allocation");
    // Put the value that lives across the call into a caller-saved register.
    let across = allocated
        .assignment
        .iter()
        .find(|(_, number)| **number >= 19)
        .map(|(value, _)| *value);
    if let Some(value) = across {
        allocated.assignment.insert(value, 9);
        let refusal = verify_allocation(&allocated).expect_err("the call clobbers it");
        assert_eq!(refusal.code, "E4020");
    }
}

#[test]
fn a_flags_producer_separated_from_its_consumer_is_refused() {
    let mut allocated = allocate(&out_of_ssa(&straight())).expect("allocation");
    let position = allocated.blocks[0]
        .insts
        .iter()
        .position(|inst| matches!(inst, MInst::Cmp { .. }))
        .expect("a comparison");
    allocated.blocks[0]
        .insts
        .insert(position + 1, MInst::MovImm { dst: w(90), imm: 1 });
    allocated.assignment.insert(90, 5);
    let refusal = verify_allocation(&allocated).expect_err("the flags no longer reach the cset");
    assert_eq!(refusal.code, "E4020");
}

#[test]
fn the_verifier_reads_only_the_allocated_function() {
    // The pass takes the allocated form and nothing else: no interval, no
    // spill decision, no record of what the allocator was thinking.
    let allocated = allocate(&out_of_ssa(&straight())).expect("allocation");
    let copy = allocated.clone();
    assert_eq!(verify_allocation(&copy), Ok(()));
}
