//! Section 9, instrument two: "A separate pass over the allocated function
//! checks the three properties of Section 7 by dataflow; the allocator does
//! not certify itself."
//!
//! Those properties are: "every physical register read is defined on every
//! path, no two simultaneously live values share a register, and every `flags`
//! producer is adjacent to its consumer."

use evocc_mir::{
    allocate, out_of_ssa, verify_allocation, AsmBody, AsmClobber, BinOp, Cond, MBlock, MFunction,
    MInst, MTerm, RegClass, VReg, Width,
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
                    fp_mode: None,
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
        assert_eq!(
            verify_allocation(&allocated),
            Ok(()),
            "{}: {allocated:#?}",
            function.name
        );
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
fn the_verifier_needs_no_allocator_analysis() {
    // The pair includes the unchanged input, not intervals, pins, liveness or
    // the allocator's spill decisions. Cloning it preserves every check input.
    let allocated = allocate(&out_of_ssa(&straight())).expect("allocation");
    let copy = allocated.clone();
    assert_eq!(verify_allocation(&copy), Ok(()));
}

/// One `asm` in the middle of a block that writes `register`, with a value
/// that the block defines before it and reads after it.
fn clobbering(register: u8) -> MFunction {
    MFunction {
        name: "main".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: Vec::new(),
            insts: vec![
                MInst::MovImm { dst: w(0), imm: 7 },
                MInst::Asm(AsmBody {
                    template: String::new(),
                    volatile: true,
                    outputs: Vec::new(),
                    inputs: Vec::new(),
                    clobbers: vec![AsmClobber::Register(register)],
                }),
                MInst::Binary {
                    op: BinOp::Add,
                    dst: w(1),
                    lhs: w(0),
                    rhs: w(0),
                    width: Width::W,
                },
            ],
            term: MTerm::Ret { values: vec![w(1)] },
        }],
    }
}

/// Section 7.5: an `asm` writes every register that its clobber list names,
/// wherever it stands in its block (T03g).
#[test]
fn a_value_kept_in_an_assembly_clobber_is_refused() {
    let allocated = allocate(&out_of_ssa(&clobbering(9))).expect("allocation");
    assert_eq!(verify_allocation(&allocated), Ok(()));
    let mut forged = allocated.clone();
    // The value read after the `asm` now waits in the clobbered register.
    let across = forged.blocks[0]
        .insts
        .iter()
        .find_map(|inst| match inst {
            MInst::Binary { lhs, .. } => Some(lhs.id),
            _ => None,
        })
        .expect("the read after the asm");
    forged.assignment.insert(across, 9);
    let refusal = verify_allocation(&forged).expect_err("the asm writes x9");
    assert_eq!(refusal.code, "E4020");
}

/// Section 6.7 and `spec/target.md` B4: a callee-saved register that an `asm`
/// clobbers is in the saved set, and the verifier checks that it is (T03g).
#[test]
fn a_callee_saved_assembly_clobber_is_saved() {
    let allocated = allocate(&out_of_ssa(&clobbering(19))).expect("allocation");
    assert!(
        allocated.used_callee_saved.contains(&19),
        "{:?}",
        allocated.used_callee_saved
    );
    assert_eq!(verify_allocation(&allocated), Ok(()));
    let mut forged = allocated.clone();
    forged.used_callee_saved.retain(|number| *number != 19);
    let refusal = verify_allocation(&forged).expect_err("x19 is written and not saved");
    assert_eq!(refusal.code, "E4020");
}

#[test]
fn swapped_entry_arguments_are_refused() {
    let mut allocated = allocate(&out_of_ssa(&straight())).unwrap();
    // Both inputs are still defined in legal argument registers. Only their
    // relation to the ordered source parameters distinguishes this mutation.
    for number in allocated.assignment.values_mut() {
        if *number < 2 {
            *number = 1 - *number;
        }
    }
    assert_eq!(verify_allocation(&allocated).unwrap_err().code, "E4020");
}

fn spilled_values() -> evocc_mir::Allocated {
    let count = 40;
    let mut insts: Vec<_> = (0..count)
        .map(|id| MInst::MovImm {
            dst: w(id),
            imm: u64::from(id + 1),
        })
        .collect();
    let mut sum = w(0);
    for id in 1..count {
        let dst = w(count + id);
        insts.push(MInst::Binary {
            op: BinOp::Add,
            dst,
            lhs: sum,
            rhs: w(id),
            width: Width::W,
        });
        sum = dst;
    }
    let allocated = allocate(&MFunction {
        name: "pressure".into(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![],
            insts,
            term: MTerm::Ret { values: vec![sum] },
        }],
    })
    .unwrap();
    assert!(allocated.frame_slots >= 2);
    verify_allocation(&allocated).unwrap();
    allocated
}

#[test]
fn reload_from_another_initialized_slot_is_refused() {
    let mut allocated = spilled_values();
    let reload = allocated.blocks[0]
        .insts
        .iter_mut()
        .find_map(|inst| match inst {
            MInst::SpillReload { slot, .. } => Some(slot),
            _ => None,
        })
        .unwrap();
    *reload = (*reload + 1) % allocated.frame_slots;
    assert_eq!(verify_allocation(&allocated).unwrap_err().code, "E4020");
}

#[test]
fn merging_simultaneously_live_spill_slots_is_refused() {
    let mut allocated = spilled_values();
    for inst in &mut allocated.blocks[0].insts {
        if let MInst::SpillStore { slot, .. } | MInst::SpillReload { slot, .. } = inst {
            if *slot == 1 {
                *slot = 0;
            }
        }
    }
    assert_eq!(verify_allocation(&allocated).unwrap_err().code, "E4020");
}

#[test]
fn early_clobber_and_symbol_constraint_forgery_are_refused() {
    use evocc_mir::{AsmConstraint, AsmDirection, AsmOperand, AsmValue};
    let input = MFunction {
        name: "operand_constraints".into(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![w(0)],
            insts: vec![MInst::Asm(AsmBody {
                template: "// %0 %1".into(),
                volatile: true,
                outputs: vec![AsmOperand::reg(
                    AsmConstraint::EarlyClobber(AsmDirection::Write),
                    w(1),
                )],
                inputs: vec![AsmOperand::reg(
                    AsmConstraint::Register(AsmDirection::Read),
                    w(0),
                )],
                clobbers: vec![],
            })],
            term: MTerm::Ret { values: vec![w(1)] },
        }],
    };
    let valid = allocate(&input).unwrap();
    verify_allocation(&valid).unwrap();
    for symbolic in [false, true] {
        let mut forged = valid.clone();
        let body = forged.blocks[0]
            .insts
            .iter_mut()
            .find_map(|inst| match inst {
                MInst::Asm(body) => Some(body),
                _ => None,
            })
            .unwrap();
        if symbolic {
            body.inputs[0].value = AsmValue::Symbol {
                name: "object".into(),
                addend: 0,
            };
        } else {
            let src = body.inputs[0].register().unwrap();
            let dst = body.outputs[0].register().unwrap();
            forged.assignment.insert(dst.id, forged.assignment[&src.id]);
        }
        assert_eq!(verify_allocation(&forged).unwrap_err().code, "E4020");
    }
}

#[test]
fn the_input_operation_and_spill_extent_cannot_be_forged() {
    let valid = spilled_values();
    for out_of_frame in [false, true] {
        let mut forged = valid.clone();
        if out_of_frame {
            forged.frame_slots = 0;
        } else {
            let immediate = forged.blocks[0]
                .insts
                .iter_mut()
                .find_map(|inst| match inst {
                    MInst::MovImm { imm, .. } => Some(imm),
                    _ => None,
                })
                .unwrap();
            *immediate ^= 1;
        }
        assert_eq!(verify_allocation(&forged).unwrap_err().code, "E4020");
    }
}

#[test]
fn fixed_inputs_do_not_add_constraints_to_other_uses_of_the_same_value() {
    use evocc_mir::{AsmConstraint, AsmDirection, AsmOperand};
    let address = VReg::new(0, RegClass::Gpr64);
    for memory_output in [false, true] {
        let mut fixed = AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), address);
        fixed.fixed_register = Some(8);
        let source = MFunction {
            name: "repeated_assembly_value".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![address],
                insts: vec![MInst::Asm(AsmBody {
                    template: String::new(),
                    volatile: true,
                    outputs: if memory_output {
                        vec![AsmOperand::reg(
                            AsmConstraint::Memory {
                                direction: AsmDirection::ReadWrite,
                                offsettable: false,
                            },
                            address,
                        )]
                    } else {
                        vec![]
                    },
                    inputs: vec![
                        fixed,
                        AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), address),
                    ],
                    clobbers: vec![],
                })],
                term: MTerm::Ret {
                    values: vec![address],
                },
            }],
        };
        let mut allocated = allocate(&source).unwrap();
        verify_allocation(&allocated).unwrap();
        let body = allocated.blocks[0]
            .insts
            .iter_mut()
            .find_map(|inst| match inst {
                MInst::Asm(body) => Some(body),
                _ => None,
            })
            .unwrap();
        assert_eq!(body.inputs[0].fixed_register, Some(8));
        assert_eq!(body.inputs[1].fixed_register, None);
        assert_eq!(body.inputs[0].register(), body.inputs[1].register());
        if memory_output {
            assert_eq!(body.outputs[0].fixed_register, None);
            assert_eq!(body.outputs[0].register(), body.inputs[0].register());
        }
        // Sharing the physical value does not make the source's fixed binding optional.
        body.inputs[0].fixed_register = None;
        assert_eq!(verify_allocation(&allocated).unwrap_err().code, "E4020");
    }
}

#[test]
fn tied_fixed_bindings_can_be_repeated_but_not_removed() {
    use evocc_mir::{AsmConstraint, AsmDirection, AsmOperand};
    for bound_output in [false, true] {
        let mut output = AsmOperand::reg(AsmConstraint::Register(AsmDirection::Write), w(1));
        let mut input = AsmOperand::reg(AsmConstraint::Tied(0), w(0));
        if bound_output {
            output.fixed_register = Some(8);
        } else {
            input.fixed_register = Some(8);
        }
        let source = MFunction {
            name: "tied_binding".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![w(0)],
                insts: vec![MInst::Asm(AsmBody {
                    template: "add %w0, %w1, #3".into(),
                    volatile: true,
                    outputs: vec![output],
                    inputs: vec![input],
                    clobbers: vec![],
                })],
                term: MTerm::Ret { values: vec![w(1)] },
            }],
        };
        let mut allocated = allocate(&source).unwrap();
        verify_allocation(&allocated).unwrap();
        for inst in &mut allocated.blocks[0].insts {
            if let MInst::Asm(body) = inst {
                assert_eq!(body.outputs[0].fixed_register, Some(8));
                assert_eq!(body.inputs[0].fixed_register, Some(8));
                body.outputs[0].fixed_register = None;
                body.inputs[0].fixed_register = None;
            }
        }
        assert_eq!(verify_allocation(&allocated).unwrap_err().code, "E4020");
    }
}

#[test]
fn replacing_both_sides_cannot_change_the_callers_program() {
    let valid = spilled_values();
    let mut forged = valid.clone();
    for instructions in [
        &mut forged.input.blocks[0].insts,
        &mut forged.blocks[0].insts,
    ] {
        let immediate = instructions
            .iter_mut()
            .find_map(|inst| match inst {
                MInst::MovImm { imm, .. } => Some(imm),
                _ => None,
            })
            .unwrap();
        *immediate ^= 1;
    }
    // The altered pair is self-consistent, but it is not the caller's input.
    verify_allocation(&forged).unwrap();
    let error =
        evocc_mir::verify_allocation_against(evocc_mir::Target::Darwin, &valid.input, &forged)
            .unwrap_err();
    assert_eq!(error.code, "E4020");
    assert!(error.detail.contains("caller"));
}

#[test]
fn distinct_pair_load_results_cannot_be_collapsed() {
    let x = |id| VReg::new(id, RegClass::Gpr64);
    let source = MFunction {
        name: "pair_results".into(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![x(0)],
            insts: vec![
                MInst::Ldp {
                    rt1: x(1),
                    rt2: x(2),
                    base: x(0),
                    offset: 0,
                    writeback: false,
                    volatile: false,
                },
                MInst::Binary {
                    op: BinOp::Add,
                    dst: x(3),
                    lhs: x(1),
                    rhs: x(2),
                    width: Width::X,
                },
            ],
            term: MTerm::Ret { values: vec![x(3)] },
        }],
    };
    let mut allocated = allocate(&source).unwrap();
    verify_allocation(&allocated).unwrap();
    // Renaming both the second definition and its use defeats a checker that
    // only tracks allocated register IDs. The original results are distinct.
    for inst in &mut allocated.blocks[0].insts {
        match inst {
            MInst::Ldp { rt1, rt2, .. } => *rt2 = *rt1,
            MInst::Binary { lhs, rhs, .. } => *rhs = *lhs,
            _ => {}
        }
    }
    assert_eq!(verify_allocation(&allocated).unwrap_err().code, "E4020");
}
