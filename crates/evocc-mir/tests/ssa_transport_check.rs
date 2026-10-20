//! Bind the ordinary backend to its actual pre-destruction input.

use evocc_mir::{BinOp, MBlock, MFunction, MInst, MTerm, RegClass, VReg, Width};

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

fn source() -> MFunction {
    MFunction {
        name: "transport".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![w(0), w(1)],
                insts: vec![MInst::MovImm { dst: w(2), imm: 7 }],
                term: MTerm::B {
                    target: 1,
                    args: vec![w(0), w(1)],
                },
            },
            MBlock {
                id: 1,
                args: vec![w(3), w(4)],
                insts: vec![MInst::Binary {
                    op: BinOp::Sub,
                    width: Width::W,
                    dst: w(5),
                    lhs: w(3),
                    rhs: w(4),
                }],
                term: MTerm::Ret { values: vec![w(5)] },
            },
        ],
    }
}

fn accepts(before: &MFunction, after: &MFunction) -> bool {
    let (_, plan) = evocc_mir::generic::ssa::out_of_ssa_with_plan(before, true, true);
    if evocc_mir::generic::ssa_check::verify(before, after, &plan).is_err() {
        return false;
    }
    let allocated = evocc_mir::allocate(after).unwrap();
    evocc_mir::verify_allocation_against(evocc_mir::Target::Darwin, after, &allocated).is_ok()
}

#[test]
fn unchanged_transport_is_accepted() {
    let before = source();
    assert!(accepts(&before, &evocc_mir::out_of_ssa(&before)));
}

#[test]
fn register_classes_and_unique_blocks_are_not_producer_claims() {
    let before = source();
    let (after, plan) = evocc_mir::generic::ssa::out_of_ssa_with_plan(&before, true, true);
    let mut changed = after.clone();
    changed.blocks.push(changed.blocks[1].clone());
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
    let mut changed = after;
    if let MInst::Binary { lhs, .. } = &mut changed.blocks[1].insts[0] {
        lhs.class = RegClass::Gpr64;
    }
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
    let mut malformed = before;
    malformed.blocks[1].args[0] = w(0);
    assert!(evocc_mir::regalloc::checked_out_of_ssa(&malformed, true).is_err());
}

#[test]
fn changing_a_forwarded_operand_is_rejected() {
    let before = source();
    let mut after = evocc_mir::out_of_ssa(&before);
    if let MInst::Binary { lhs, .. } = &mut after.blocks[1].insts[0] {
        *lhs = w(1);
    }
    assert!(
        !accepts(&before, &after),
        "wrong source value survived the backend checks"
    );
}

#[test]
fn changing_a_retained_operation_is_rejected() {
    let before = source();
    let mut after = evocc_mir::out_of_ssa(&before);
    if let MInst::Binary { op, .. } = &mut after.blocks[1].insts[0] {
        *op = BinOp::Add;
    }
    assert!(
        !accepts(&before, &after),
        "changed operation survived the backend checks"
    );
}

#[test]
fn changing_the_result_is_rejected() {
    let before = source();
    let mut after = evocc_mir::out_of_ssa(&before);
    after.blocks[1].term = MTerm::Ret { values: vec![w(1)] };
    assert!(
        !accepts(&before, &after),
        "wrong return value survived the backend checks"
    );
}

#[test]
fn proposals_cannot_supply_missing_demand_or_invent_entry_equalities() {
    let before = source();
    let (after, plan) = evocc_mir::generic::ssa::out_of_ssa_with_plan(&before, true, true);
    let rejects = |plan: &evocc_mir::generic::ssa_check::TransportPlan<RegClass>| {
        assert!(evocc_mir::generic::ssa_check::verify(&before, &after, plan).is_err());
    };
    let mut changed = plan.clone();
    changed.needed.remove(&0);
    rejects(&changed);
    let mut changed = plan.clone();
    changed
        .blocks
        .get_mut(&0)
        .unwrap()
        .bindings
        .insert(w(0), w(1));
    rejects(&changed);
    let mut changed = plan.clone();
    changed
        .blocks
        .get_mut(&1)
        .unwrap()
        .bindings
        .insert(w(3), w(1));
    rejects(&changed);
    let mut changed = plan.clone();
    changed.blocks.get_mut(&1).unwrap().prefix = usize::MAX;
    rejects(&changed);
    let mut changed = plan.clone();
    changed.blocks.get_mut(&1).unwrap().alignment[0] = None;
    rejects(&changed);
    let mut changed = plan.clone();
    changed.blocks.get_mut(&1).unwrap().alignment[0] = Some(usize::MAX);
    rejects(&changed);
    let mut changed = plan;
    changed.blocks.remove(&1);
    rejects(&changed);
}

#[test]
fn calls_cannot_be_deleted_as_dead_results() {
    let mut before = source();
    before.blocks[0].insts.push(MInst::Bl {
        returns_twice: false,
        symbol: "touch".into(),
        args: vec![w(0)],
        results: vec![],
        fp_mode: None,
    });
    let (mut after, mut plan) = evocc_mir::generic::ssa::out_of_ssa_with_plan(&before, true, true);
    assert!(accepts(&before, &after));
    after.blocks[0].insts.clear();
    plan.blocks.get_mut(&0).unwrap().alignment[1] = None;
    assert!(evocc_mir::generic::ssa_check::verify(&before, &after, &plan).is_err());
}

fn rotation() -> MFunction {
    MFunction {
        name: "rotate".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![w(0), w(1), w(2)],
                insts: vec![],
                term: MTerm::B {
                    target: 1,
                    args: vec![w(0), w(1), w(2)],
                },
            },
            MBlock {
                id: 1,
                args: vec![w(3), w(4), w(5)],
                insts: vec![],
                term: MTerm::Cb {
                    nonzero: true,
                    src: w(5),
                    taken: 2,
                    taken_args: vec![w(3), w(4), w(5)],
                    untaken: 3,
                    untaken_args: vec![w(3)],
                },
            },
            MBlock {
                id: 2,
                args: vec![w(6), w(7), w(8)],
                insts: vec![MInst::BinaryImm {
                    op: BinOp::Sub,
                    width: Width::W,
                    dst: w(9),
                    lhs: w(8),
                    imm: 1,
                }],
                term: MTerm::B {
                    target: 1,
                    args: vec![w(7), w(6), w(9)],
                },
            },
            MBlock {
                id: 3,
                args: vec![w(10)],
                insts: vec![],
                term: MTerm::Ret {
                    values: vec![w(10)],
                },
            },
        ],
    }
}

#[test]
fn cyclic_parallel_copies_and_split_edges_are_checked_in_execution_order() {
    let before = rotation();
    evocc_mir::validate::validate_function(&before).unwrap();
    let (after, plan) = evocc_mir::generic::ssa::out_of_ssa_with_plan(&before, true, true);
    evocc_mir::generic::ssa_check::verify(&before, &after, &plan).unwrap();
    assert!(accepts(&before, &after));
    let mut changed = after.clone();
    let moves = &mut changed.blocks[2].insts;
    let saved = moves
        .iter()
        .position(|i| matches!(i, MInst::Mov { dst, .. } if evocc_mir::regalloc::is_scratch(*dst)))
        .unwrap();
    moves.remove(saved);
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
    let mut changed = after.clone();
    changed.blocks[2].term = MTerm::B {
        target: 3,
        args: vec![],
    };
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
    let mut changed = after;
    changed.blocks[0].args.swap(0, 1);
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
}

#[test]
fn transport_cannot_reinterpret_a_value_or_change_its_class() {
    let before = rotation();
    let (after, plan) = evocc_mir::generic::ssa::out_of_ssa_with_plan(&before, true, true);
    let mut changed = after.clone();
    let mov = changed
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.insts)
        .find(|i| matches!(i, MInst::Mov { .. }))
        .unwrap();
    if let MInst::Mov { dst, src } = mov {
        dst.class = RegClass::Gpr64;
        src.class = RegClass::Gpr64;
    }
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
    let mut changed = after;
    let mov = changed
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.insts)
        .find(|i| matches!(i, MInst::Mov { .. }))
        .unwrap();
    if let MInst::Mov { src, .. } = mov {
        src.class = RegClass::Gpr64;
    }
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
}

#[test]
fn administrative_blocks_cannot_hide_effects_or_cycles() {
    let mut before = rotation();
    // The exit edge now has two predecessors and must be split.
    before.blocks[2].term = MTerm::Cb {
        nonzero: true,
        src: w(9),
        taken: 1,
        taken_args: vec![w(7), w(6), w(9)],
        untaken: 3,
        untaken_args: vec![w(6)],
    };
    evocc_mir::validate::validate_function(&before).unwrap();
    let (after, plan) = evocc_mir::generic::ssa::out_of_ssa_with_plan(&before, true, true);
    evocc_mir::generic::ssa_check::verify(&before, &after, &plan).unwrap();
    let index = after.blocks.iter().position(|b| b.id > 3).unwrap();
    let mut changed = after.clone();
    let id = changed.blocks[index].id;
    changed.blocks[index].term = MTerm::B {
        target: id,
        args: vec![],
    };
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
    let mut changed = after;
    changed.blocks[index].insts.push(MInst::Bl {
        returns_twice: false,
        symbol: "extra".into(),
        args: vec![],
        results: vec![],
        fp_mode: None,
    });
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
}

#[test]
fn assembly_goto_outputs_and_all_successors_keep_their_values() {
    use evocc_mir::{AsmBody, AsmConstraint, AsmDirection, AsmOperand};
    let mut before = source();
    before.blocks[0].insts.clear();
    before.blocks[0].term = MTerm::AsmGoto {
        body: AsmBody {
            template: "mov %w0, %w1; cbnz %w1, %l2".into(),
            volatile: true,
            outputs: vec![AsmOperand::reg(
                AsmConstraint::Register(AsmDirection::Write),
                w(2),
            )],
            inputs: vec![AsmOperand::reg(
                AsmConstraint::Register(AsmDirection::Read),
                w(0),
            )],
            clobbers: vec![],
        },
        labels: vec![(1, vec![w(2), w(1)])],
        fallthrough: 2,
        fallthrough_args: vec![w(2)],
    };
    before.blocks.push(MBlock {
        id: 2,
        args: vec![w(6)],
        insts: vec![],
        term: MTerm::Ret { values: vec![w(6)] },
    });
    evocc_mir::validate::validate_function(&before).unwrap();
    let (after, plan) = evocc_mir::generic::ssa::out_of_ssa_with_plan(&before, true, true);
    evocc_mir::generic::ssa_check::verify(&before, &after, &plan).unwrap();
    let mut changed = after.clone();
    if let MTerm::AsmGoto { body, .. } = &mut changed.blocks[0].term {
        body.volatile = false;
    }
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
    let mut changed = after;
    if let MTerm::AsmGoto { body, .. } = &mut changed.blocks[0].term {
        body.outputs.clear();
    }
    assert!(evocc_mir::generic::ssa_check::verify(&before, &changed, &plan).is_err());
}
