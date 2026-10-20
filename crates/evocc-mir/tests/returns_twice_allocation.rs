//! Section 7.4, "Calls that return twice" (revision 1.6.0), and the
//! allocation verifier of Section 9.2.
//!
//! A value live across `bl.returns_twice` gets no register. It lives in a
//! private stack slot, which no other value writes at any point that the call
//! reaches, and each read after the call reloads it. The verifier checks both
//! halves without the allocator's analyses: it refuses an allocation that
//! keeps such a value only in a register, and one in which another value
//! overwrites its slot.

use std::collections::{BTreeMap, BTreeSet};

use evocc_mir::{
    allocate, out_of_ssa, parse_module, render_module, verify_allocation, Allocated, BinOp, MBlock,
    MFunction, MInst, MModule, MTerm, RegClass, VReg, Width,
};

fn x(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr64)
}

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

const KEPT: u32 = 4;
const PRESSURE: u32 = 20;

/// Entry: `KEPT` values, then `setjmp(env)`. The first return computes
/// `PRESSURE` values that live across a call, then jumps. The second return
/// reads only the kept values, so they are dead on the first-return path.
fn hazard(returns_twice: bool) -> MFunction {
    let env = x(0);
    let mut entry = Vec::new();
    for index in 0..KEPT {
        entry.push(MInst::MovImm {
            dst: w(10 + index),
            imm: u64::from(1000 + index),
        });
    }
    entry.push(MInst::Bl {
        symbol: "setjmp".into(),
        args: vec![env],
        results: vec![w(1)],
        fp_mode: None,
        returns_twice,
    });
    let mut first = Vec::new();
    for index in 0..PRESSURE {
        first.push(MInst::MovImm {
            dst: w(100 + index),
            imm: u64::from(index + 1),
        });
    }
    first.push(MInst::Bl {
        symbol: "pressure".into(),
        args: Vec::new(),
        results: Vec::new(),
        fp_mode: None,
        returns_twice: false,
    });
    let mut total = w(100);
    for index in 1..PRESSURE {
        first.push(MInst::Binary {
            op: BinOp::Add,
            dst: w(200 + index),
            lhs: total,
            rhs: w(100 + index),
            width: Width::W,
        });
        total = w(200 + index);
    }
    first.push(MInst::Bl {
        symbol: "jump".into(),
        args: vec![env, total],
        results: Vec::new(),
        fp_mode: None,
        returns_twice: false,
    });
    first.push(MInst::MovImm { dst: w(2), imm: 0 });
    let mut second = Vec::new();
    let mut sum = w(10);
    for index in 1..KEPT {
        second.push(MInst::Binary {
            op: BinOp::Add,
            dst: w(300 + index),
            lhs: sum,
            rhs: w(10 + index),
            width: Width::W,
        });
        sum = w(300 + index);
    }
    MFunction {
        name: "hazard".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![env],
                insts: entry,
                term: MTerm::Cb {
                    nonzero: true,
                    src: w(1),
                    taken: 2,
                    taken_args: Vec::new(),
                    untaken: 1,
                    untaken_args: Vec::new(),
                },
            },
            MBlock {
                id: 1,
                args: Vec::new(),
                insts: first,
                term: MTerm::Ret { values: vec![w(2)] },
            },
            MBlock {
                id: 2,
                args: Vec::new(),
                insts: second,
                term: MTerm::Ret { values: vec![sum] },
            },
        ],
    }
}

fn block(allocated: &Allocated, id: u32) -> &MBlock {
    allocated
        .blocks
        .iter()
        .find(|block| block.id == id)
        .expect("block")
}

/// Spill slots that `insts` stores into.
fn stored(insts: &[MInst]) -> BTreeSet<u32> {
    insts
        .iter()
        .filter_map(|inst| match inst {
            MInst::SpillStore { slot, .. } => Some(*slot),
            _ => None,
        })
        .collect()
}

fn set_returns_twice(insts: &mut [MInst]) {
    for inst in insts {
        if let MInst::Bl {
            symbol,
            returns_twice,
            ..
        } = inst
        {
            if symbol == "setjmp" {
                *returns_twice = true;
            }
        }
    }
}

#[test]
fn the_text_form_spells_the_mark() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![hazard(true)],
    };
    let text = render_module(&module);
    assert!(text.contains("bl.returns_twice @setjmp("), "{text}");
    assert_eq!(parse_module(&text).expect("the text parses"), module);
}

#[test]
fn values_live_across_the_call_get_private_slots() {
    let allocated = allocate(&out_of_ssa(&hazard(true))).expect("allocation");
    verify_allocation(&allocated).expect("the allocation is sound");
    let entry = block(&allocated, 0);
    let call = entry
        .insts
        .iter()
        .position(|inst| {
            matches!(
                inst,
                MInst::Bl {
                    returns_twice: true,
                    ..
                }
            )
        })
        .expect("the returns-twice call");
    // Every kept value and the jump buffer pointer are stored before the call.
    let private = stored(&entry.insts[..call]);
    assert!(private.len() > KEPT as usize, "{allocated:#?}");
    // The first-return path spills its own values, but never into those slots.
    let first = block(&allocated, 1);
    assert!(!stored(&first.insts).is_empty(), "{allocated:#?}");
    assert!(stored(&first.insts).is_disjoint(&private), "{allocated:#?}");
    // The second-return path reloads every value it reads.
    let second = block(&allocated, 2);
    let reloads = second
        .insts
        .iter()
        .filter(|inst| matches!(inst, MInst::SpillReload { .. }))
        .count();
    assert_eq!(reloads, KEPT as usize, "{allocated:#?}");
}

#[test]
fn an_ordinary_call_keeps_the_same_values_in_registers() {
    // The control: without the mark, the kept values stay in callee-saved
    // registers, so the second-return path reloads nothing.
    let allocated = allocate(&out_of_ssa(&hazard(false))).expect("allocation");
    verify_allocation(&allocated).expect("the allocation is sound");
    let second = block(&allocated, 2);
    assert!(
        !second
            .insts
            .iter()
            .any(|inst| matches!(inst, MInst::SpillReload { .. })),
        "{allocated:#?}"
    );
}

#[test]
fn a_value_kept_only_in_a_register_is_refused() {
    // Forge the allocation of the ordinary call for the returns-twice call.
    let mut allocated = allocate(&out_of_ssa(&hazard(false))).expect("allocation");
    for block in &mut allocated.input.blocks {
        set_returns_twice(&mut block.insts);
    }
    for block in &mut allocated.blocks {
        set_returns_twice(&mut block.insts);
    }
    let refusal = verify_allocation(&allocated).expect_err("no slot holds the kept values");
    assert_eq!(refusal.code, "E4020");
    assert!(
        refusal.detail.contains("is read before it is written")
            || refusal.detail.contains("no stack slot holds it"),
        "{refusal:?}"
    );
}

#[test]
fn a_slot_that_another_value_overwrites_is_refused() {
    let sound = allocate(&out_of_ssa(&hazard(true))).expect("allocation");
    let entry = block(&sound, 0);
    let call = entry
        .insts
        .iter()
        .position(|inst| {
            matches!(
                inst,
                MInst::Bl {
                    returns_twice: true,
                    ..
                }
            )
        })
        .unwrap();
    let private = stored(&entry.insts[..call]);
    // Share the private slot of a kept value with a first-return value: its
    // spill store and reload use a slot that the second return reads. The
    // first-return path itself never reads that slot for a kept value.
    let slot = block(&sound, 2)
        .insts
        .iter()
        .find_map(|inst| match inst {
            MInst::SpillReload { slot, .. } => Some(*slot),
            _ => None,
        })
        .expect("the second return reloads a kept value");
    assert!(private.contains(&slot), "{sound:#?}");
    let victim = block(&sound, 1)
        .insts
        .iter()
        .find_map(|inst| match inst {
            MInst::SpillStore { slot, .. } => Some(*slot),
            _ => None,
        })
        .expect("a first-return spill");
    let mut shared = sound.clone();
    for block in &mut shared.blocks {
        for inst in &mut block.insts {
            if let MInst::SpillStore { slot: used, .. } | MInst::SpillReload { slot: used, .. } =
                inst
            {
                if *used == victim && block.id == 1 {
                    *used = slot;
                }
            }
        }
    }
    let refusal = verify_allocation(&shared).expect_err("the shared slot loses a kept value");
    assert_eq!(refusal.code, "E4020");
    assert!(
        refusal.detail.contains("live across a returns-twice call")
            && refusal.detail.contains("overwrites it"),
        "{refusal:?}"
    );
    // A store of a different value anywhere that the call reaches is refused
    // too, even when no later read observes it.
    let mut forged = sound.clone();
    let first = forged
        .blocks
        .iter_mut()
        .find(|block| block.id == 1)
        .unwrap();
    let source = first
        .insts
        .iter()
        .find_map(|inst| match inst {
            MInst::MovImm { dst, .. } => Some(*dst),
            _ => None,
        })
        .unwrap();
    let at = first
        .insts
        .iter()
        .position(|inst| matches!(inst, MInst::MovImm { .. }))
        .unwrap();
    first
        .insts
        .insert(at + 1, MInst::SpillStore { src: source, slot });
    let refusal = verify_allocation(&forged).expect_err("another value writes the slot");
    assert!(refusal.detail.contains("overwrites it"), "{refusal:?}");
}

/// The pattern of musl's `timer_create.c`: a copy `w := v` of a value `v`
/// that lives across `bl.returns_twice`. Before the call, `w` spills to a
/// slot that therefore also holds `v`. `w` is dead after the call, so its
/// slot is shared, and a first-return value writes it later. The second
/// return reads `v` from its own private slot, not from the copy's slot.
fn alias(read_copy_slot: bool) -> (Allocated, u32) {
    let env = x(0);
    let kept = w(1);
    let copy = w(2);
    let mut entry = vec![
        MInst::MovImm {
            dst: kept,
            imm: 4242,
        },
        MInst::Mov {
            dst: copy,
            src: kept,
        },
    ];
    // Twelve values and the copy live across a call: three of them spill.
    // The copy has the furthest use, so it is one of the three.
    for index in 0..12 {
        entry.push(MInst::MovImm {
            dst: w(10 + index),
            imm: u64::from(index + 1),
        });
    }
    entry.push(MInst::Bl {
        symbol: "pressure".into(),
        args: Vec::new(),
        results: Vec::new(),
        fp_mode: None,
        returns_twice: false,
    });
    let mut total = w(10);
    for index in 1..12 {
        entry.push(MInst::Binary {
            op: BinOp::Add,
            dst: w(40 + index),
            lhs: total,
            rhs: w(10 + index),
            width: Width::W,
        });
        total = w(40 + index);
    }
    entry.push(MInst::Binary {
        op: BinOp::Add,
        dst: w(60),
        lhs: total,
        rhs: copy,
        width: Width::W,
    });
    entry.push(MInst::Bl {
        symbol: "consume".into(),
        args: vec![w(60)],
        results: Vec::new(),
        fp_mode: None,
        returns_twice: false,
    });
    entry.push(MInst::Bl {
        symbol: "setjmp".into(),
        args: vec![env],
        results: vec![w(3)],
        fp_mode: None,
        returns_twice: true,
    });
    // The first return: fourteen values live across a call, so four spill
    // into slots that the values of the entry block no longer need.
    let mut first = Vec::new();
    for index in 0..14 {
        first.push(MInst::MovImm {
            dst: w(100 + index),
            imm: u64::from(100 + index),
        });
    }
    first.push(MInst::Bl {
        symbol: "pressure".into(),
        args: Vec::new(),
        results: Vec::new(),
        fp_mode: None,
        returns_twice: false,
    });
    let mut sum = w(100);
    for index in 1..14 {
        first.push(MInst::Binary {
            op: BinOp::Add,
            dst: w(200 + index),
            lhs: sum,
            rhs: w(100 + index),
            width: Width::W,
        });
        sum = w(200 + index);
    }
    first.push(MInst::Bl {
        symbol: "jump".into(),
        args: vec![env, sum],
        results: Vec::new(),
        fp_mode: None,
        returns_twice: false,
    });
    first.push(MInst::MovImm { dst: w(4), imm: 0 });
    let function = MFunction {
        name: "alias".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![env],
                insts: entry,
                term: MTerm::Cb {
                    nonzero: true,
                    src: w(3),
                    taken: 2,
                    taken_args: Vec::new(),
                    untaken: 1,
                    untaken_args: Vec::new(),
                },
            },
            MBlock {
                id: 1,
                args: Vec::new(),
                insts: first,
                term: MTerm::Ret { values: vec![w(4)] },
            },
            MBlock {
                id: 2,
                args: Vec::new(),
                insts: Vec::new(),
                term: MTerm::Ret { values: vec![kept] },
            },
        ],
    };
    let mut allocated = allocate(&out_of_ssa(&function)).expect("allocation");
    let shared = copy_slot(&allocated);
    assert!(
        stored(&block(&allocated, 1).insts).contains(&shared),
        "a first-return value reuses the copy's slot: {allocated:#?}"
    );
    if read_copy_slot {
        // Read the kept value from the copy's slot after the second return.
        let second = allocated
            .blocks
            .iter_mut()
            .find(|block| block.id == 2)
            .unwrap();
        let MInst::SpillReload { slot, .. } = second
            .insts
            .iter_mut()
            .find(|inst| matches!(inst, MInst::SpillReload { .. }))
            .expect("the second return reloads the kept value")
        else {
            unreachable!()
        };
        *slot = shared;
    }
    (allocated, shared)
}

/// The slot of the copy: the entry block stores the kept value's constant
/// into its private slot, reloads it, and stores the reloaded register into
/// a second slot.
fn copy_slot(allocated: &Allocated) -> u32 {
    let register = |reg: VReg| allocated.assignment[&reg.id];
    let mut kept = None;
    let mut constant = None;
    let mut reloaded = BTreeMap::<u8, u32>::new();
    for inst in &block(allocated, 0).insts {
        match inst {
            MInst::MovImm { dst, imm } => {
                reloaded.remove(&register(*dst));
                constant = (*imm == 4242).then_some(*dst);
            }
            MInst::SpillStore { src, slot } => {
                if kept.is_none() && constant == Some(*src) {
                    kept = Some(*slot);
                } else if kept.is_some()
                    && reloaded.get(&register(*src)) == kept.as_ref()
                    && kept != Some(*slot)
                {
                    return *slot;
                }
            }
            MInst::SpillReload { dst, slot } => {
                reloaded.insert(register(*dst), *slot);
            }
            MInst::Mov { dst, src } => match reloaded.get(&register(*src)).copied() {
                Some(slot) => {
                    reloaded.insert(register(*dst), slot);
                }
                None => {
                    reloaded.remove(&register(*dst));
                }
            },
            other => {
                for reg in evocc_mir::inst_defs(other) {
                    if let Some(number) = allocated.assignment.get(&reg.id) {
                        reloaded.remove(number);
                    }
                }
            }
        }
    }
    panic!("no slot holds a copy of the kept value: {allocated:#?}")
}

#[test]
fn a_shared_slot_that_holds_only_a_copy_is_not_guarded() {
    let (allocated, _) = alias(false);
    verify_allocation(&allocated).expect("the second return reads the private slot");
}

#[test]
fn reading_a_kept_value_from_a_reused_copy_slot_is_refused() {
    let (allocated, shared) = alias(true);
    let refusal = verify_allocation(&allocated).expect_err("the copy's slot is overwritten");
    assert_eq!(refusal.code, "E4020");
    assert!(
        refusal.detail.contains(&format!("slot {shared} holds"))
            && refusal.detail.contains("overwrites it"),
        "{refusal:?}"
    );
}

/// The loop of musl's `timer_create.c`: each iteration copies a kept value
/// before `bl.returns_twice`, and the first-return path loops back. The copy
/// lives across the call, so it has no register: the allocator reloads the
/// copied value and stores it directly into the copy's private slot, before
/// the copy in source order. With `tick`, a call precedes the copy.
fn copy_in_loop(tick: bool) -> MFunction {
    let env = x(0);
    let mut header = Vec::new();
    if tick {
        header.push(MInst::Bl {
            symbol: "tick".into(),
            args: Vec::new(),
            results: Vec::new(),
            fp_mode: None,
            returns_twice: false,
        });
    }
    header.push(MInst::Mov {
        dst: w(2),
        src: w(1),
    });
    header.push(MInst::Bl {
        symbol: "setjmp".into(),
        args: vec![env],
        results: vec![w(3)],
        fp_mode: None,
        returns_twice: true,
    });
    MFunction {
        name: "copy_in_loop".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![env],
                insts: vec![MInst::MovImm { dst: w(1), imm: 7 }],
                term: MTerm::B {
                    target: 1,
                    args: Vec::new(),
                },
            },
            MBlock {
                id: 1,
                args: Vec::new(),
                insts: header,
                term: MTerm::Cb {
                    nonzero: true,
                    src: w(3),
                    taken: 3,
                    taken_args: Vec::new(),
                    untaken: 2,
                    untaken_args: Vec::new(),
                },
            },
            MBlock {
                id: 2,
                args: Vec::new(),
                insts: vec![MInst::Bl {
                    symbol: "consume".into(),
                    args: vec![w(2)],
                    results: Vec::new(),
                    fp_mode: None,
                    returns_twice: false,
                }],
                term: MTerm::B {
                    target: 1,
                    args: Vec::new(),
                },
            },
            MBlock {
                id: 3,
                args: Vec::new(),
                insts: Vec::new(),
                term: MTerm::Ret { values: vec![w(2)] },
            },
        ],
    }
}

/// The reload of the copied value and the store into the copy's slot, in
/// the loop block, before the returns-twice call.
fn copy_store(allocated: &Allocated) -> (usize, usize) {
    let header = block(allocated, 1);
    let call = header
        .insts
        .iter()
        .position(|inst| {
            matches!(
                inst,
                MInst::Bl {
                    returns_twice: true,
                    ..
                }
            )
        })
        .expect("the returns-twice call");
    let store = header.insts[..call]
        .iter()
        .position(|inst| matches!(inst, MInst::SpillStore { .. }))
        .expect("the copy's store precedes the call");
    assert!(
        store > 0 && matches!(header.insts[store - 1], MInst::SpillReload { .. }),
        "{allocated:#?}"
    );
    assert!(
        !header
            .insts
            .iter()
            .any(|inst| matches!(inst, MInst::Mov { .. })),
        "the store is the copy: {allocated:#?}"
    );
    (store - 1, store)
}

#[test]
fn a_store_that_is_a_pending_copy_keeps_the_guard() {
    // Around the loop, the store writes the guarded slot of the copy while
    // the copied value still has its old name: the copy follows the store.
    let allocated = allocate(&out_of_ssa(&copy_in_loop(false))).expect("allocation");
    copy_store(&allocated);
    verify_allocation(&allocated).expect("the store is the copy");
}

#[test]
fn a_store_before_an_intervening_call_is_not_the_copy() {
    let sound = allocate(&out_of_ssa(&copy_in_loop(true))).expect("allocation");
    verify_allocation(&sound).expect("the allocation is sound");
    // Move the reload and the store before `tick`. The copy now follows a
    // call that can jump back, so the store overwrites the guarded slot of
    // the copy with another value.
    let (reload, store) = copy_store(&sound);
    let mut forged = sound.clone();
    let header = forged
        .blocks
        .iter_mut()
        .find(|block| block.id == 1)
        .unwrap();
    let tick = header
        .insts
        .iter()
        .position(|inst| matches!(inst, MInst::Bl { symbol, .. } if symbol == "tick"))
        .unwrap();
    assert!(tick < reload, "{sound:#?}");
    let moved: Vec<_> = header.insts.drain(reload..=store).collect();
    header.insts.splice(tick..tick, moved);
    let refusal = verify_allocation(&forged).expect_err("the store precedes the call");
    assert_eq!(refusal.code, "E4020");
    assert!(
        refusal.detail.contains("live across a returns-twice call")
            && refusal.detail.contains("overwrites it"),
        "{refusal:?}"
    );
}
