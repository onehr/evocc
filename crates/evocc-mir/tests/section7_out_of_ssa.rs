//! Section 7, out-of-SSA.
//!
//! "Block arguments are removed by placing, on each edge, the parallel copy
//! from the values the terminator passes to the target's arguments, resolved
//! into a sequence of moves with at most one scratch register (`x16`) for
//! cycles. Critical edges are split by inserting a block. This happens before
//! allocation, on virtual registers, as its own pass; the lowering of Section
//! 5 does not do it."

use evocc_mir::{
    out_of_ssa, render_module, term_edges, BinOp, MBlock, MFunction, MInst, MModule, MTerm,
    RegClass, VReg, Width,
};

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

fn moves(block: &MBlock) -> Vec<(u32, u32)> {
    block
        .insts
        .iter()
        .filter_map(|inst| match inst {
            MInst::Mov { dst, src } => Some((dst.id, src.id)),
            _ => None,
        })
        .collect()
}

#[test]
fn dead_fixed_addresses_do_not_reserve_storage_but_used_addresses_do() {
    let address = VReg::new(0, RegClass::Gpr64);
    for live in [false, true] {
        let function = MFunction {
            name: "fixed_address".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![],
                insts: vec![MInst::FrameSlot {
                    dst: address,
                    origin: 0,
                    size: 8,
                    align: 8,
                }],
                term: MTerm::Ret {
                    values: if live { vec![address] } else { vec![] },
                },
            }],
        };
        let after = evocc_mir::regalloc::checked_out_of_ssa(&function, true).unwrap();
        assert_eq!(after.blocks[0].insts.len(), usize::from(live));
        let allocated = evocc_mir::allocate(&after).unwrap();
        evocc_mir::verify_allocation(&allocated).unwrap();
        assert_eq!(
            evocc_mir::emit::frame_of_for(evocc_mir::Target::Linux, &allocated)
                .slots
                .len(),
            usize::from(live)
        );
    }
}

#[test]
fn dce_attribution_retains_the_same_dead_witnesses_only_in_the_off_control() {
    let function = MFunction {
        name: "dce_control".into(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![w(0), VReg::new(1, RegClass::Gpr64)],
            insts: vec![
                MInst::MovImm { dst: w(2), imm: 7 },
                MInst::Binary {
                    op: BinOp::Add,
                    dst: w(3),
                    lhs: w(0),
                    rhs: w(2),
                    width: Width::W,
                },
                MInst::Binary {
                    op: BinOp::Eor,
                    dst: w(4),
                    lhs: w(3),
                    rhs: w(2),
                    width: Width::W,
                },
                MInst::Load {
                    dst: w(5),
                    base: VReg::new(1, RegClass::Gpr64),
                    offset: 0,
                    kind: evocc_mir::LoadKind::W,
                    volatile: false,
                    align: 4,
                    mask: None,
                    transport: false,
                },
            ],
            term: MTerm::Ret { values: vec![w(0)] },
        }],
    };
    assert_eq!(evocc_mir::regalloc::dead_computation_count(&function), 3);
    let on = out_of_ssa(&function);
    let off = evocc_mir::regalloc::out_of_ssa_with_dce(&function, false);
    assert_eq!(on.blocks[0].insts.len(), 1);
    assert_eq!(off.blocks[0].insts.len(), 4);
    assert_eq!(evocc_mir::regalloc::dead_computation_count(&on), 0);
    assert_eq!(evocc_mir::regalloc::dead_computation_count(&off), 3);
    for lowered in [on, off] {
        let allocated = evocc_mir::allocate(&lowered).unwrap();
        evocc_mir::verify_allocation(&allocated).unwrap();
        assert!(matches!(
            lowered.blocks[0].insts.last(),
            Some(MInst::Load { .. })
        ));
    }
}

#[test]
fn unused_loop_carried_parameters_do_not_create_copy_pressure() {
    let width = 64;
    let mut blocks = vec![MBlock {
        id: 0,
        args: Vec::new(),
        insts: (0..width)
            .map(|id| MInst::MovImm {
                dst: w(id),
                imm: u64::from(id),
            })
            .collect(),
        term: MTerm::B {
            target: 1,
            args: (0..width).map(w).collect(),
        },
    }];
    for block in 1..=32 {
        let args: Vec<_> = (block * width..(block + 1) * width).map(w).collect();
        let term = if block == 32 {
            MTerm::Tb {
                nonzero: true,
                src: args[0],
                bit: 0,
                taken: 1,
                taken_args: args.clone(),
                untaken: 33,
                untaken_args: vec![args[0]],
            }
        } else {
            MTerm::B {
                target: block + 1,
                args: args.clone(),
            }
        };
        blocks.push(MBlock {
            id: block,
            args,
            insts: Vec::new(),
            term,
        });
    }
    blocks.push(MBlock {
        id: 33,
        args: vec![w(33 * width)],
        insts: Vec::new(),
        term: MTerm::Ret {
            values: vec![w(33 * width)],
        },
    });
    let lowered = out_of_ssa(&MFunction {
        name: "transport".into(),
        blocks,
    });
    let copies = lowered.blocks.iter().flat_map(moves).count();
    assert!(
        copies <= 34,
        "unused transport cycle generated {copies} copies"
    );
    let allocated = evocc_mir::allocate(&lowered).expect("one live value");
    assert_eq!(
        allocated.spilled, 0,
        "unused edge arguments must not cause spills"
    );
    evocc_mir::verify_allocation(&allocated).expect("the live loop value remains defined");
}

#[test]
fn unused_symbol_addresses_do_not_become_linker_dependencies() {
    let x = |id| VReg::new(id, RegClass::Gpr64);
    let function = MFunction {
        name: "metadata".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![x(0)],
                insts: vec![
                    MInst::Adrp {
                        dst: x(1),
                        symbol: "discarded_marker".into(),
                        kind: evocc_mir::SymbolKind::Global,
                    },
                    MInst::AddPageOff {
                        dst: x(2),
                        src: x(1),
                        symbol: "discarded_marker".into(),
                        kind: evocc_mir::SymbolKind::Global,
                    },
                    MInst::MovImm { dst: x(3), imm: 7 },
                    MInst::Load {
                        dst: x(4),
                        base: x(0),
                        offset: 0,
                        kind: evocc_mir::LoadKind::X,
                        volatile: true,
                        align: 8,
                        mask: None,
                        transport: false,
                    },
                ],
                term: MTerm::B {
                    target: 1,
                    args: vec![x(2), x(3)],
                },
            },
            MBlock {
                id: 1,
                args: vec![x(5), x(6)],
                insts: vec![],
                term: MTerm::Ret { values: vec![x(6)] },
            },
        ],
    };
    let after = out_of_ssa(&function);
    assert!(
        !after
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .any(|i| matches!(i, MInst::Adrp { .. } | MInst::AddPageOff { .. })),
        "unused marker address still produces a relocation"
    );
    assert!(
        after
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .any(|i| matches!(i, MInst::Load { volatile: true, .. })),
        "unused result does not authorize deleting an actual access"
    );
    let allocated = evocc_mir::allocate(&after).unwrap();
    evocc_mir::verify_allocation(&allocated).unwrap();
}

#[test]
fn live_arithmetic_keeps_its_flags_consumer_but_dead_bundles_disappear() {
    let flags = |id| VReg::new(id, RegClass::Flags);
    for carry_chain in [false, true] {
        for returned in [None, Some(w(2)), Some(w(5))] {
            let mut insts = vec![MInst::FlagBinary {
                op: evocc_mir::FlagOp::Adds,
                dst: w(2),
                flags: flags(3),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            }];
            if carry_chain {
                insts.push(MInst::AddCarryFlags {
                    dst: flags(4),
                    flags_in: flags(3),
                    lhs: w(0),
                    rhs: w(1),
                    width: Width::W,
                });
            }
            insts.push(MInst::Cset {
                dst: w(5),
                flags: flags(if carry_chain { 4 } else { 3 }),
                cond: evocc_mir::Cond::Cs,
            });
            let function = MFunction {
                name: "arithmetic".into(),
                blocks: vec![MBlock {
                    id: 0,
                    args: vec![w(0), w(1)],
                    insts,
                    term: MTerm::Ret {
                        values: returned.into_iter().collect(),
                    },
                }],
            };
            evocc_mir::validate_module(&MModule {
                functions: vec![function.clone()],
                ..MModule::default()
            })
            .unwrap();
            let after = out_of_ssa(&function);
            evocc_mir::validate_module(&MModule {
                functions: vec![after.clone()],
                ..MModule::default()
            })
            .unwrap();
            if returned.is_none() {
                assert!(
                    after.blocks[0].insts.is_empty(),
                    "dead flags bundle retained"
                );
            }
            let allocated = evocc_mir::allocate(&after).unwrap();
            evocc_mir::verify_allocation(&allocated).unwrap();
        }
    }
}

#[test]
fn an_indirect_copy_cycle_preserves_the_branch_register_and_original_labels() {
    let x = |id| VReg::new(id, RegClass::Gpr64);
    let function = MFunction {
        name: "dispatch".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![x(0), x(1)],
                insts: vec![],
                term: MTerm::B {
                    target: 1,
                    args: vec![x(0), x(1)],
                },
            },
            MBlock {
                id: 1,
                args: vec![x(2), x(3)],
                insts: vec![],
                term: MTerm::Indirect {
                    target: x(2),
                    edges: vec![(1, vec![x(3), x(2)]), (2, vec![x(2), x(3)])],
                },
            },
            MBlock {
                id: 2,
                args: vec![x(4), x(5)],
                insts: vec![],
                // Both bindings checked below must be observable, rather
                // than requiring a copy to an unused parameter to survive.
                term: MTerm::Ret {
                    values: vec![x(4), x(5)],
                },
            },
        ],
    };
    let after = out_of_ssa(&function);
    let block = find(&after, 1);
    let mut values = std::collections::BTreeMap::from([(2, 101), (3, 202)]);
    for (dst, src) in moves(block) {
        values.insert(dst, values[&src]);
    }
    let MTerm::Indirect { target, edges } = &block.term else {
        panic!("indirect terminator disappeared");
    };
    assert_eq!(
        values[&target.id], 101,
        "parallel copies must not overwrite the computed target"
    );
    assert_eq!(
        (values[&2], values[&3], values[&4], values[&5]),
        (202, 101, 101, 202)
    );
    assert_eq!(
        edges.iter().map(|(target, _)| *target).collect::<Vec<_>>(),
        vec![1, 2]
    );
    verify_with_copy_hints(&after);
}

fn verify_with_copy_hints(function: &MFunction) {
    for target in [evocc_mir::Target::Darwin, evocc_mir::Target::Linux] {
        for copy_hints in [false, true] {
            let allocated = evocc_mir::allocate_for_with_options(
                target,
                function,
                evocc_mir::generic::allocation::AllocationOptions {
                    copy_hints,
                    ..Default::default()
                },
            )
            .unwrap();
            evocc_mir::verify_allocation_for(target, &allocated).unwrap();
        }
    }
}

#[test]
fn copy_hints_preserve_selected_swap_and_rotation_edges() {
    for function in [swap(), rotation()] {
        verify_with_copy_hints(&out_of_ssa(&function));
    }
}

fn find(function: &MFunction, id: u32) -> &MBlock {
    function
        .blocks
        .iter()
        .find(|block| block.id == id)
        .expect("block")
}

/// `^entry` falls into `^b1(%v2, %v3)`; the target reads them as `%v4, %v5`.
fn straight_edge() -> MFunction {
    MFunction {
        name: "f".to_owned(),
        blocks: vec![
            MBlock {
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
                    MInst::Binary {
                        op: BinOp::Sub,
                        dst: w(3),
                        lhs: w(0),
                        rhs: w(1),
                        width: Width::W,
                    },
                ],
                term: MTerm::B {
                    target: 1,
                    args: vec![w(2), w(3)],
                },
            },
            MBlock {
                id: 1,
                args: vec![w(4), w(5)],
                insts: Vec::new(),
                term: MTerm::Ret {
                    values: vec![w(4), w(5)],
                },
            },
        ],
    }
}

#[test]
fn every_block_argument_and_edge_argument_is_gone() {
    let before = straight_edge();
    let after = out_of_ssa(&before);
    for block in &after.blocks {
        for (target, args) in term_edges(&block.term) {
            assert!(args.is_empty(), "edge to {target} keeps arguments");
        }
        if block.id == 0 {
            continue;
        }
        assert!(block.args.is_empty(), "block {} keeps arguments", block.id);
    }
    // The entry block's arguments are the function's parameters: no edge binds
    // them, so no copy can remove them, and they are the boundary's fixed
    // operands until the allocator assigns them.
    assert_eq!(find(&after, 0).args, before.blocks[0].args);
}

#[test]
fn a_single_reaching_definition_needs_no_transport_copy() {
    let after = out_of_ssa(&straight_edge());
    assert!(after.blocks.iter().all(|block| moves(block).is_empty()));
    assert_eq!(
        find(&after, 1).term,
        MTerm::Ret {
            values: vec![w(2), w(3)]
        }
    );
}

/// A self-loop that passes its two arguments back crossed over: the parallel
/// copy is a two-cycle and needs the scratch.
fn swap() -> MFunction {
    MFunction {
        name: "f".to_owned(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![w(0), w(1)],
                insts: Vec::new(),
                term: MTerm::B {
                    target: 1,
                    args: vec![w(0), w(1)],
                },
            },
            MBlock {
                id: 1,
                args: vec![w(2), w(3)],
                insts: Vec::new(),
                term: MTerm::Tb {
                    nonzero: true,
                    src: w(2),
                    bit: 0,
                    taken: 1,
                    taken_args: vec![w(3), w(2)],
                    untaken: 2,
                    untaken_args: vec![w(2)],
                },
            },
            MBlock {
                id: 2,
                args: vec![w(4)],
                insts: Vec::new(),
                term: MTerm::Ret { values: vec![w(4)] },
            },
        ],
    }
}

#[test]
fn a_swap_uses_one_scratch_register() {
    let after = out_of_ssa(&swap());
    // The crossing edge is critical, so its copy lands in a block of its own.
    let scratch: Vec<u32> = after
        .blocks
        .iter()
        .flat_map(moves)
        .filter(|(dst, _)| *dst >= 100)
        .map(|(dst, _)| dst)
        .collect();
    assert_eq!(scratch.len(), 1, "one scratch definition: {after:?}");

    let sequence: Vec<(u32, u32)> = after.blocks.iter().flat_map(moves).collect();
    assert_eq!(
        sequence.len(),
        5,
        "two on the entry edge, three on the swap; the exit just forwards: {sequence:?}"
    );
}

/// Three registers rotated on one edge.
fn rotation() -> MFunction {
    MFunction {
        name: "f".to_owned(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![w(0), w(1), w(2)],
                insts: Vec::new(),
                term: MTerm::B {
                    target: 1,
                    args: vec![w(0), w(1), w(2)],
                },
            },
            MBlock {
                id: 1,
                args: vec![w(3), w(4), w(5)],
                insts: Vec::new(),
                term: MTerm::Tb {
                    nonzero: true,
                    src: w(3),
                    bit: 0,
                    taken: 1,
                    taken_args: vec![w(4), w(5), w(3)],
                    untaken: 2,
                    untaken_args: vec![w(3)],
                },
            },
            MBlock {
                id: 2,
                args: vec![w(6)],
                insts: Vec::new(),
                term: MTerm::Ret { values: vec![w(6)] },
            },
        ],
    }
}

#[test]
fn a_cycle_of_three_uses_one_scratch_and_four_moves() {
    let after = out_of_ssa(&rotation());
    let critical: Vec<&MBlock> = after.blocks.iter().filter(|block| block.id >= 3).collect();
    let rotate: Vec<(u32, u32)> = critical.iter().copied().flat_map(moves).collect();
    assert_eq!(
        rotate.len(),
        4,
        "a three-cycle costs one scratch move and three moves: {rotate:?}"
    );
    assert_eq!(
        rotate.iter().filter(|(dst, _)| *dst >= 100).count(),
        1,
        "exactly one scratch: {rotate:?}"
    );
}

#[test]
fn a_critical_edge_is_split() {
    // Block 1 has two successors and block 1 has two predecessors, so the back
    // edge is critical and its copy cannot go in either endpoint.
    let before = swap();
    let after = out_of_ssa(&before);
    assert!(
        after.blocks.len() > before.blocks.len(),
        "a block was inserted: {after:?}"
    );
    for block in &after.blocks {
        let successors = term_edges(&block.term);
        for (target, _) in &successors {
            let predecessors = after
                .blocks
                .iter()
                .filter(|other| term_edges(&other.term).iter().any(|(to, _)| to == target))
                .count();
            assert!(
                successors.len() < 2 || predecessors < 2,
                "block {} still has a critical edge to {target}",
                block.id
            );
        }
    }
}

#[test]
fn the_pass_is_a_function_of_its_input() {
    for function in [straight_edge(), swap(), rotation()] {
        let one = MModule {
            globals: Vec::new(),
            functions: vec![out_of_ssa(&function)],
        };
        let two = MModule {
            globals: Vec::new(),
            functions: vec![out_of_ssa(&function)],
        };
        assert_eq!(render_module(&one), render_module(&two));
    }
}

#[test]
fn a_move_whose_source_and_destination_agree_is_not_emitted() {
    let identity = MFunction {
        name: "f".to_owned(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![w(0)],
                insts: Vec::new(),
                term: MTerm::B {
                    target: 1,
                    args: vec![w(0)],
                },
            },
            MBlock {
                id: 1,
                args: vec![w(0)],
                insts: Vec::new(),
                term: MTerm::Ret { values: vec![w(0)] },
            },
        ],
    };
    let after = out_of_ssa(&identity);
    assert!(
        after.blocks.iter().all(|block| moves(block).is_empty()),
        "{after:?}"
    );
}
