//! Structural regressions for the state growth observed in Oniguruma regexec.c.
//! The counter is test-only. No wall-clock threshold or checker bypass is used.

use std::cell::Cell;
use std::collections::BTreeMap;

use crate::isa::aarch64::allocation::Aarch64Allocation;
use crate::{
    Allocated, BinOp, Cond, MBlock, MFunction, MInst, MTerm, RegClass, Target, VReg, Width,
};

thread_local! {
    static MAX_INCOMING: Cell<usize> = const { Cell::new(0) };
    static BLOCK_VISITS: Cell<usize> = const { Cell::new(0) };
    static MAX_STEP: Cell<usize> = const { Cell::new(0) };
    static IDENTITY_PROBES: Cell<usize> = const { Cell::new(0) };
    static PROJECTION_PROBES: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn observe_identity_probe() {
    IDENTITY_PROBES.with(|count| count.set(count.get() + 1));
}

pub(super) fn observe_projection_probe() {
    PROJECTION_PROBES.with(|count| count.set(count.get() + 1));
}

#[test]
fn a_sparse_successor_does_not_copy_all_predecessor_equations() {
    let mut state = super::State::new();
    for id in 0..2048 {
        state.insert((super::Place::Spill(id), w(id).into()));
    }
    PROJECTION_PROBES.with(|count| count.set(0));
    let live = std::collections::BTreeSet::from([super::Identity::Register(1024)]);
    let projected = state.project(&live);
    assert_eq!(projected.len(), 1);
    assert!(
        projected.equations
            == state
                .equations
                .iter()
                .copied()
                .filter(|(_, v)| live.contains(&v.id))
                .collect()
    );
    assert!(
        PROJECTION_PROBES.with(Cell::get) <= 2,
        "an edge must copy only its demanded identities, probes={}",
        PROJECTION_PROBES.with(Cell::get)
    );
}

#[test]
fn projecting_identities_commutes_with_bridge_transport() {
    let places = [
        super::Place::Register(0),
        super::Place::Register(1),
        super::Place::Spill(0),
        super::Place::Spill(1),
    ];
    let mut state = super::State::new();
    for id in 0..4 {
        for class in [RegClass::Gpr32, RegClass::Gpr64] {
            state.insert((places[id as usize], VReg::new(id, class).into()));
        }
    }
    for mask in 0..16 {
        let live = (0..4)
            .filter(|id| mask & (1 << id) != 0)
            .map(super::Identity::Register)
            .collect();
        for src in places {
            for dst in places {
                for class in [RegClass::Gpr32, RegClass::Gpr64] {
                    let mut before = state.project(&live);
                    let mut after = state.clone();
                    let src = super::Location { place: src, class };
                    let dst = super::Location { place: dst, class };
                    super::copy(&mut before, dst, src);
                    super::copy(&mut after, dst, src);
                    assert!(before == after.project(&live));
                }
            }
        }
    }
}

#[test]
fn renaming_one_identity_does_not_scan_unrelated_equations() {
    let mut state = super::State::new();
    for id in 0..2048 {
        state.insert((super::Place::Spill(id), w(id).into()));
    }
    IDENTITY_PROBES.with(|count| count.set(0));
    super::rename(&mut state, w(4096).into(), w(1024).into());
    assert!(
        IDENTITY_PROBES.with(Cell::get) <= 2,
        "a source rename must inspect only its identity, probes={}",
        IDENTITY_PROBES.with(Cell::get)
    );
    assert!(state.iter().any(|(place, value)| {
        *place == super::Place::Spill(1024) && value.id == super::Identity::Register(4096)
    }));
}

#[test]
fn identity_index_matches_equations_after_every_update() {
    fn assert_index(state: &super::State<RegClass>) {
        let mut expected = BTreeMap::<_, std::collections::BTreeSet<_>>::new();
        for (place, value) in state.iter() {
            expected
                .entry(value.id)
                .or_default()
                .insert((*place, value.class));
        }
        assert!(
            state.identities == expected,
            "identity index differs from equations"
        );
    }
    let mut state = super::State::new();
    for id in 0..64 {
        for class in [RegClass::Gpr32, RegClass::Gpr64] {
            let equation = (super::Place::Spill(id % 7), VReg::new(id, class).into());
            state.insert(equation);
            state.insert(equation);
            assert_index(&state);
        }
    }
    super::rename(&mut state, w(0).into(), w(0).into());
    assert_index(&state);
    state.retain(|(_, value)| value.id != super::Identity::Register(3));
    assert_index(&state);
    let snapshot = state.clone();
    for id in 0..64 {
        state.forget_identity(super::Identity::Register(id));
        assert_index(&state);
    }
    state = state.intersection(&snapshot).copied().collect();
    assert_index(&state);
    assert_eq!(state.len(), 0);
    state.extend(snapshot.iter().copied());
    assert_index(&state);
    super::kill(
        &mut state,
        super::Location {
            place: super::Place::Spill(3),
            class: RegClass::Gpr64,
        },
    );
    assert_index(&state);
}

pub(super) fn observe_incoming(size: usize) {
    MAX_INCOMING.with(|peak| peak.set(peak.get().max(size)));
    BLOCK_VISITS.with(|visits| visits.set(visits.get() + 1));
}

pub(super) fn observe_step(size: usize) {
    MAX_STEP.with(|peak| peak.set(peak.get().max(size)));
}

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

fn allocated(input: MFunction, assignment: BTreeMap<u32, u8>) -> Allocated {
    let mut blocks = input.blocks.clone();
    let parameters = blocks[0].args.len();
    blocks[0].args.clear();
    Allocated {
        name: input.name.clone(),
        input,
        blocks,
        assignment,
        spilled: 0,
        frame_slots: 0,
        used_callee_saved: vec![],
        parameters,
        outgoing: 0,
        indirect_result: false,
        internal: false,
    }
}

fn check(program: &Allocated) -> Result<(), String> {
    super::verify(&Aarch64Allocation::for_target(Target::Linux), program)
}

#[test]
fn duplicate_source_blocks_are_rejected_before_dataflow() {
    let mut program = allocated(
        MFunction {
            name: "duplicate_blocks".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![w(0)],
                insts: vec![],
                term: MTerm::Ret { values: vec![w(0)] },
            }],
        },
        BTreeMap::from([(0, 0)]),
    );
    // The two blocks demand different incoming sets for the same ID. A
    // liveness solver must not run on this malformed graph.
    program.input.blocks.push(MBlock {
        id: 0,
        args: vec![],
        insts: vec![MInst::MovImm { dst: w(0), imm: 1 }],
        term: MTerm::Ret { values: vec![w(0)] },
    });
    assert!(check(&program).unwrap_err().contains("duplicated blocks"));
}

fn copy_chain(block_count: u32, copies_per_block: u32) -> Allocated {
    let mut blocks = vec![];
    let mut last = 0;
    for id in 0..block_count {
        let mut insts = vec![];
        for _ in 0..copies_per_block {
            insts.push(MInst::Mov {
                dst: w(last + 1),
                src: w(last),
            });
            last += 1;
        }
        blocks.push(MBlock {
            id,
            args: if id == 0 { vec![w(0)] } else { vec![] },
            insts,
            term: if id + 1 == block_count {
                MTerm::Ret {
                    values: vec![w(last)],
                }
            } else {
                MTerm::B {
                    target: id + 1,
                    args: vec![],
                }
            },
        });
    }
    // All copies can legally name the same physical register. Only the last
    // source identity of a block is live at its successor's entry.
    allocated(
        MFunction {
            name: "copy_chain".into(),
            blocks,
        },
        (0..=last).map(|id| (id, 0)).collect(),
    )
}

#[test]
fn acyclic_blocks_are_not_revisited_without_a_changed_input() {
    let program = copy_chain(32, 3);
    BLOCK_VISITS.with(|visits| visits.set(0));
    check(&program).unwrap();
    assert_eq!(
        BLOCK_VISITS.with(Cell::get),
        32,
        "only changed incoming equations require another block walk"
    );
}

#[test]
fn dead_copy_history_does_not_accumulate_inside_a_block() {
    let program = copy_chain(1, 1024);
    MAX_STEP.with(|peak| peak.set(0));
    check(&program).unwrap();
    assert_eq!(
        MAX_STEP.with(Cell::get),
        1,
        "an expired alias must not remain until the next CFG edge"
    );
}

#[test]
fn a_dead_copy_cycle_does_not_create_source_demand() {
    let program = allocated(
        MFunction {
            name: "dead_copy_loop".into(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: vec![],
                    term: MTerm::B {
                        target: 1,
                        args: vec![],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![],
                    insts: vec![MInst::Mov {
                        dst: w(1),
                        src: w(0),
                    }],
                    term: MTerm::B {
                        target: 2,
                        args: vec![],
                    },
                },
                MBlock {
                    id: 2,
                    args: vec![],
                    insts: vec![MInst::Mov {
                        dst: w(0),
                        src: w(1),
                    }],
                    term: MTerm::B {
                        target: 1,
                        args: vec![],
                    },
                },
            ],
        },
        BTreeMap::from([(0, 0), (1, 0)]),
    );
    MAX_INCOMING.with(|peak| peak.set(0));
    check(&program).unwrap();
    assert_eq!(
        MAX_INCOMING.with(Cell::get),
        0,
        "transport-only cycles have no checked read to justify live identities"
    );
}

#[test]
fn dead_copy_history_does_not_accumulate_at_block_entries() {
    let program = copy_chain(128, 8);
    MAX_INCOMING.with(|peak| peak.set(0));
    check(&program).unwrap();
    assert_eq!(
        MAX_INCOMING.with(Cell::get),
        1,
        "dead copy identities must not enlarge successor states"
    );
}

#[test]
fn flags_used_after_an_edge_remain_in_the_source_relation() {
    let flags = VReg::new(1, RegClass::Flags);
    let program = allocated(
        MFunction {
            name: "edge_flags".into(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: vec![MInst::CmpImm {
                        dst: flags,
                        lhs: w(0),
                        imm: 0,
                        width: Width::W,
                    }],
                    term: MTerm::B {
                        target: 1,
                        args: vec![],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![],
                    insts: vec![MInst::Cset {
                        dst: w(2),
                        flags,
                        cond: Cond::Eq,
                    }],
                    term: MTerm::Ret { values: vec![w(2)] },
                },
            ],
        },
        BTreeMap::from([(0, 0), (2, 0)]),
    );
    check(&program).unwrap();
}

#[test]
fn operands_of_a_dead_computation_are_still_checked() {
    let program = allocated(
        MFunction {
            name: "dead_result".into(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0), w(1)],
                    insts: vec![],
                    term: MTerm::B {
                        target: 1,
                        args: vec![],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![],
                    insts: vec![MInst::Binary {
                        op: BinOp::Add,
                        dst: w(2),
                        lhs: w(0),
                        rhs: w(1),
                        width: Width::W,
                    }],
                    term: MTerm::Ret { values: vec![w(0)] },
                },
            ],
        },
        BTreeMap::from([(0, 0), (1, 1), (2, 2)]),
    );
    check(&program).unwrap();
    let mut forged = program;
    forged.assignment.insert(1, 2);
    assert!(
        check(&forged).is_err(),
        "a dead result does not excuse a wrong input"
    );
}

#[test]
fn source_spill_identities_survive_an_edge() {
    let mut program = allocated(
        MFunction {
            name: "edge_spill".into(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: vec![MInst::SpillStore { src: w(0), slot: 0 }],
                    term: MTerm::B {
                        target: 1,
                        args: vec![],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![],
                    insts: vec![MInst::SpillReload { dst: w(1), slot: 0 }],
                    term: MTerm::Ret { values: vec![w(1)] },
                },
            ],
        },
        BTreeMap::from([(0, 0), (1, 0)]),
    );
    program.frame_slots = 2;
    check(&program).unwrap();
    program.blocks[1].insts[0] = MInst::SpillReload { dst: w(1), slot: 1 };
    assert!(
        check(&program).is_err(),
        "another slot cannot supply the live logical slot"
    );
}

#[test]
fn loop_liveness_keeps_values_used_only_after_exit() {
    let program = allocated(
        MFunction {
            name: "loop_exit".into(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0), w(1)],
                    insts: vec![MInst::MovImm { dst: w(2), imm: 1 }],
                    term: MTerm::B {
                        target: 1,
                        args: vec![],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![],
                    insts: vec![MInst::Binary {
                        op: BinOp::Sub,
                        dst: w(0),
                        lhs: w(0),
                        rhs: w(2),
                        width: Width::W,
                    }],
                    term: MTerm::Cb {
                        nonzero: true,
                        src: w(0),
                        taken: 1,
                        taken_args: vec![],
                        untaken: 2,
                        untaken_args: vec![],
                    },
                },
                MBlock {
                    id: 2,
                    args: vec![],
                    insts: vec![],
                    term: MTerm::Ret { values: vec![w(1)] },
                },
            ],
        },
        BTreeMap::from([(0, 0), (1, 0), (2, 2)]),
    );
    // The return value must be in x0, so transport the second argument there
    // only after the loop. Its entry register is x1.
    let mut program = program;
    program.assignment.insert(1, 1);
    program.blocks[2].insts.push(MInst::Mov {
        dst: w(3),
        src: w(1),
    });
    program.blocks[2].term = MTerm::Ret { values: vec![w(3)] };
    program.assignment.insert(3, 0);
    check(&program).unwrap();
    program.assignment.insert(2, 1);
    assert!(
        check(&program).is_err(),
        "a live-through-loop argument was clobbered"
    );
}
