//! Target compatibility entry points and allocated AArch64 text rendering.
//!
//! Allocation policy and spill rewriting live in generic::allocation; argument
//! placement uses the shared cursor of generic::abi over the selected ABI.

#[cfg(test)]
use std::cmp::Reverse;
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::isa::aarch64::{allocation::Aarch64Allocation, regfile};
use crate::{inst_defs, inst_uses, term_uses, MFunction, MInst, MTerm, RegClass, Target, VReg};
#[cfg(test)]
use crate::{term_edges, MBlock};

/// Section 6: "`x0` to `x17` are caller-saved", less the two intra-procedure
/// scratch registers, which Section 2 keeps for emitted sequences.
pub use crate::isa::aarch64::regfile::CALLER_SAVED;

/// Section 6: "`x19` to `x28` ... are callee-saved".
pub use crate::isa::aarch64::regfile::CALLEE_SAVED;

/// Section 2: "`x16` and `x17` are the intra-procedure-call scratch
/// registers, used only by emitted sequences".
pub use crate::isa::aarch64::regfile::SCRATCH;

/// SIMD numbers occupy a disjoint allocator bank. v30/v31 are emission
/// scratch; v8-v15 are deliberately not allocated because only their low
/// halves are callee saved. Live-across-call vectors spill at full width.
pub use crate::isa::aarch64::regfile::{SIMD_BASE, SIMD_CALLER_SAVED, SIMD_SCRATCH};

/// The existing public allocated representation, specialized without conversion.
pub type Allocated = crate::generic::allocation::Allocated<MInst, MTerm, RegClass>;

pub fn allocate(function: &MFunction) -> Result<Allocated, String> {
    allocate_for(Target::Darwin, function)
}

/// Platform-specific source admission still occurs before the shared allocator,
/// which places boundary values with the target's own ABI table.
pub fn allocate_for(target: Target, function: &MFunction) -> Result<Allocated, String> {
    crate::generic::allocation::allocate(&Aarch64Allocation::for_target(target), function)
}

#[cfg(test)]
fn scan(function: &MFunction, pins: &BTreeMap<u32, u8>) -> Result<BTreeMap<u32, u8>, Vec<u32>> {
    crate::generic::allocation::scan(
        &Aarch64Allocation::for_target(Target::Darwin),
        function,
        pins,
    )
}

#[cfg(test)]
fn live_on_entry(
    function: &MFunction,
    uses: &BTreeMap<u32, BTreeSet<u32>>,
    defs: &BTreeMap<u32, BTreeSet<u32>>,
) -> BTreeMap<u32, BTreeSet<u32>> {
    crate::generic::allocation::live_on_entry(function, uses, defs)
}

#[cfg(test)]
fn compact_spill_slots(function: &mut MFunction) -> u32 {
    crate::generic::allocation::compact_spill_slots(function)
}

#[cfg(test)]
fn insert_spill_code(
    function: &MFunction,
    slots: &BTreeMap<u32, u32>,
    victims: &[u32],
    next: &mut u32,
) -> MFunction {
    crate::generic::allocation::insert_spill_code(function, slots, victims, next)
}

/// The physical spelling of one assigned virtual register.
#[must_use]
pub fn physical(class: RegClass, number: u8) -> String {
    match class {
        RegClass::Gpr32 => regfile::GPR32_VIEW.spell(number),
        RegClass::Gpr64 => regfile::GPR64_VIEW.spell(number),
        RegClass::Simd32 => regfile::SIMD32_VIEW.spell(number),
        RegClass::Simd64 => regfile::SIMD64_VIEW.spell(number),
        RegClass::Simd128 => regfile::SIMD128_VIEW.spell(number),
        RegClass::Flags => regfile::FLAGS_SPELLING.to_owned(),
    }
}

/// Render an allocated function in the post-allocation spelling of Appendix A.
#[must_use]
pub fn render_allocated(allocated: &Allocated) -> String {
    // A use is written without its class, so the physical spelling of one is
    // recovered from the definition that owns it.
    let mut classes: BTreeMap<u32, RegClass> = BTreeMap::new();
    for block in &allocated.blocks {
        for reg in block
            .insts
            .iter()
            .flat_map(inst_defs)
            .chain(block.insts.iter().flat_map(inst_uses))
            .chain(term_uses(&block.term))
        {
            classes.insert(reg.id, reg.class);
        }
    }
    let name = |reg: VReg| -> String {
        let class = classes.get(&reg.id).copied().unwrap_or(reg.class);
        allocated.assignment.get(&reg.id).map_or_else(
            || format!("%v{}", reg.id),
            |number| physical(class, *number),
        )
    };
    let mut text = String::new();
    let _ = writeln!(text, "func @{} {{", allocated.name);
    for (index, block) in allocated.blocks.iter().enumerate() {
        if index != 0 {
            text.push('\n');
        }
        let label = if index == 0 {
            "entry".to_owned()
        } else {
            format!("b{}", block.id)
        };
        let _ = writeln!(text, "{label}:");
        for inst in &block.insts {
            let _ = writeln!(text, "    {}", render_inst(inst, &name));
        }
        let _ = writeln!(text, "    {}", render_term(allocated, &block.term, &name));
    }
    text.push_str("}\n");
    text
}

fn render_inst(inst: &MInst, name: &impl Fn(VReg) -> String) -> String {
    // The pre-allocation renderer already spells every form; the only
    // difference here is that an operand is a physical register, that a
    // definition carries no class, and that the two spill pseudo-forms name a
    // frame slot rather than an address.
    match inst {
        MInst::SpillStore { src, slot } => format!("str.x {}, [slot #{slot}]", name(*src)),
        MInst::SpillReload { dst, slot } => format!("{} = ldr.x [slot #{slot}]", name(*dst)),
        other => {
            let pre = crate::text::render_inst(other);
            rename(&pre, name)
        }
    }
}

fn render_term(allocated: &Allocated, term: &MTerm, name: &impl Fn(VReg) -> String) -> String {
    match term {
        // Section 6 returns in x0 onwards, so the registers are already fixed
        // and Appendix A prints a bare `ret`.
        MTerm::Ret { .. } => "ret".to_owned(),
        other => {
            let function = MFunction {
                name: allocated.name.clone(),
                blocks: allocated.blocks.clone(),
            };
            let pre = crate::text::render_term(&function, other);
            rename(&pre, name)
        }
    }
}

/// Replace every `%vN` in a pre-allocation rendering with its physical name.
fn rename(text: &str, name: &impl Fn(VReg) -> String) -> String {
    let mut out = String::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == '%' && bytes.get(index + 1) == Some(&'v') {
            let mut end = index + 2;
            let mut id = 0_u32;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                id = id * 10 + (bytes[end] as u32 - '0' as u32);
                end += 1;
            }
            // A definition carries its class after a colon; a use does not.
            // A definition carries `:class`, which the physical spelling
            // replaces; a use carries nothing and is looked up by identity.
            let skip = if bytes.get(end) == Some(&':') {
                let mut tail = end + 1;
                while tail < bytes.len() && bytes[tail].is_ascii_alphanumeric() {
                    tail += 1;
                }
                tail
            } else {
                end
            };
            out.push_str(&name(VReg::new(id, RegClass::Gpr32)));
            index = skip;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

#[cfg(test)]
mod spill_flag_tests {
    use super::*;
    use crate::{CarryOp, Cond, FlagOp, Width};

    fn word(id: u32) -> VReg {
        VReg::new(id, RegClass::Gpr64)
    }
    fn flags(id: u32) -> VReg {
        VReg::new(id, RegClass::Flags)
    }

    #[test]
    fn spill_victim_uses_next_read_instead_of_interval_end() {
        let count = (CALLER_SAVED.len() + CALLEE_SAVED.len()) as u32;
        let mut insts: Vec<_> = (0..=count)
            .map(|id| MInst::MovImm {
                dst: word(id),
                imm: u64::from(id),
            })
            .collect();
        // All registers are occupied when `count` is defined. Value zero
        // has the latest LAST read, but must be used immediately. Value one
        // has the most distant NEXT read and is the Section 7 spill victim.
        for id in [0, count].into_iter().chain(2..count).chain([1, 0]) {
            insts.push(MInst::SpillStore {
                src: word(id),
                slot: id,
            });
        }
        let function = MFunction {
            name: "next_read_pressure".into(),
            blocks: vec![MBlock {
                id: 0,
                args: Vec::new(),
                insts,
                term: MTerm::Ret { values: Vec::new() },
            }],
        };
        assert_eq!(scan(&function, &BTreeMap::new()), Err(vec![1]));
    }

    #[test]
    fn next_read_victim_cannot_lend_a_register_with_a_future_pin() {
        let count = (CALLER_SAVED.len() + CALLEE_SAVED.len()) as u32;
        let mut insts: Vec<_> = (0..=count)
            .map(|id| MInst::MovImm {
                dst: word(id),
                imm: u64::from(id),
            })
            .collect();
        // Zero occupies x0 until just before its future fixed use. Although
        // zero's next read is furthest, `count` remains live beyond that pin
        // and cannot take x0. One is the furthest eligible victim instead.
        for id in [count].into_iter().chain(2..count).chain([1, 0]) {
            insts.push(MInst::SpillStore {
                src: word(id),
                slot: id,
            });
        }
        insts.extend([
            MInst::MovImm {
                dst: word(count + 1),
                imm: 0,
            },
            MInst::SpillStore {
                src: word(count + 1),
                slot: count + 1,
            },
            MInst::SpillStore {
                src: word(count),
                slot: count,
            },
        ]);
        let function = MFunction {
            name: "next_read_fixed_pressure".into(),
            blocks: vec![MBlock {
                id: 0,
                args: Vec::new(),
                insts,
                term: MTerm::Ret { values: Vec::new() },
            }],
        };
        assert_eq!(
            scan(&function, &BTreeMap::from([(count + 1, 0)])),
            Err(vec![1])
        );
    }

    // Independent set-based reference for the packed conflict representation.
    // Keep the deliberately small oracle separate from production bitsets.
    fn reference_spill_mapping(function: &MFunction) -> (BTreeMap<u32, u32>, u32) {
        let mut sizes = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let mut defs = BTreeMap::new();
        for block in &function.blocks {
            let mut read = BTreeSet::new();
            let mut written = BTreeSet::new();
            for inst in &block.insts {
                match inst {
                    MInst::SpillReload { dst, slot } => {
                        sizes.insert(*slot, if dst.class == RegClass::Simd128 { 2 } else { 1 });
                        if !written.contains(slot) {
                            read.insert(*slot);
                        }
                    }
                    MInst::SpillStore { src, slot } => {
                        sizes.insert(*slot, if src.class == RegClass::Simd128 { 2 } else { 1 });
                        written.insert(*slot);
                    }
                    _ => {}
                }
            }
            uses.insert(block.id, read);
            defs.insert(block.id, written);
        }
        let live_in = live_on_entry(function, &uses, &defs);
        let mut graph: BTreeMap<u32, BTreeSet<u32>> =
            sizes.keys().map(|slot| (*slot, BTreeSet::new())).collect();
        for block in &function.blocks {
            let mut live: BTreeSet<_> = term_edges(&block.term)
                .iter()
                .flat_map(|(target, _)| live_in[target].iter().copied())
                .collect();
            for inst in block.insts.iter().rev() {
                let slot = match inst {
                    MInst::SpillReload { slot, .. } | MInst::SpillStore { slot, .. } => *slot,
                    _ => continue,
                };
                for other in live.iter().copied().filter(|other| *other != slot) {
                    graph.get_mut(&slot).unwrap().insert(other);
                    graph.get_mut(&other).unwrap().insert(slot);
                }
                if matches!(inst, MInst::SpillStore { .. }) {
                    live.remove(&slot);
                } else {
                    live.insert(slot);
                }
            }
        }
        let mut order: Vec<_> = sizes.keys().copied().collect();
        order.sort_unstable_by_key(|slot| (Reverse(graph[slot].len()), *slot));
        let mut mapping = BTreeMap::new();
        let mut count = 0;
        for slot in order {
            let occupied: BTreeSet<_> = graph[&slot]
                .iter()
                .flat_map(|other| {
                    mapping
                        .get(other)
                        .into_iter()
                        .flat_map(|start| *start..*start + sizes[other])
                })
                .collect();
            let size = sizes[&slot];
            let start = (0_u32..)
                .find(|start| {
                    start.is_multiple_of(size)
                        && (*start..*start + size).all(|at| !occupied.contains(&at))
                })
                .unwrap();
            count = count.max(start + size);
            mapping.insert(slot, start);
        }
        (mapping, count)
    }

    #[test]
    fn packed_spill_conflicts_match_set_graph_across_backedges_and_dead_stores() {
        let mut random = 13_u64;
        for case in 0..128_u32 {
            let mut blocks = Vec::new();
            for id in 0..3 {
                let mut insts = Vec::new();
                for _ in 0..128 {
                    random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let slot = (random >> 32) as u32 % (case % 31 + 1);
                    let reg = VReg::new(
                        slot,
                        if slot.is_multiple_of(3) {
                            RegClass::Simd128
                        } else {
                            RegClass::Gpr64
                        },
                    );
                    let slot = 17 + slot * 3;
                    insts.push(if random & 1 == 0 {
                        MInst::SpillStore { src: reg, slot }
                    } else {
                        MInst::SpillReload { dst: reg, slot }
                    });
                }
                blocks.push(MBlock {
                    id,
                    args: vec![],
                    insts,
                    term: if id == 2 {
                        MTerm::BCond {
                            flags: flags(100),
                            cond: Cond::Ne,
                            taken: 1,
                            taken_args: vec![],
                            untaken: 0,
                            untaken_args: vec![],
                        }
                    } else {
                        MTerm::B {
                            target: id + 1,
                            args: vec![],
                        }
                    },
                });
            }
            let mut function = MFunction {
                name: "spill_graph".into(),
                blocks,
            };
            let (mapping, count) = reference_spill_mapping(&function);
            let mut expected = function.clone();
            for inst in expected
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.insts)
            {
                if let MInst::SpillReload { slot, .. } | MInst::SpillStore { slot, .. } = inst {
                    *slot = mapping[slot];
                }
            }
            assert_eq!(compact_spill_slots(&mut function), count);
            assert_eq!(function, expected);
        }
    }

    #[test]
    fn repeated_dense_vector_spills_keep_every_live_slot_distinct() {
        let mut insts = Vec::new();
        for slot in 0..256 {
            insts.push(MInst::SpillStore {
                src: VReg::new(slot, RegClass::Simd128),
                slot,
            });
        }
        for _ in 0..256 {
            for slot in 0..256 {
                insts.push(MInst::SpillReload {
                    dst: VReg::new(slot, RegClass::Simd128),
                    slot,
                });
            }
        }
        let mut function = MFunction {
            name: "dense_spills".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![],
                insts,
                term: MTerm::Ret { values: vec![] },
            }],
        };
        assert_eq!(compact_spill_slots(&mut function), 512);
        for inst in &function.blocks[0].insts {
            match inst {
                MInst::SpillStore { src, slot } | MInst::SpillReload { dst: src, slot } => {
                    assert_eq!(*slot, 2 * src.id)
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn disjoint_vector_spills_reuse_one_slot_despite_many_sparse_identities() {
        let insts = (0..16_384)
            .flat_map(|id| {
                let reg = VReg::new(id, RegClass::Simd128);
                let slot = 29 + id * 17;
                [
                    MInst::SpillStore { src: reg, slot },
                    MInst::SpillReload { dst: reg, slot },
                ]
            })
            .collect();
        let mut function = MFunction {
            name: "disjoint_spills".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![],
                insts,
                term: MTerm::Ret { values: vec![] },
            }],
        };
        assert_eq!(compact_spill_slots(&mut function), 2);
        for inst in &function.blocks[0].insts {
            match inst {
                MInst::SpillStore { slot, .. } | MInst::SpillReload { slot, .. } => {
                    assert_eq!(*slot, 0);
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn spill_reload_and_store_preserve_carry_adjacency() {
        for chain in [false, true] {
            let mut insts = vec![
                MInst::MovImm {
                    dst: word(0),
                    imm: 1,
                },
                MInst::MovImm {
                    dst: word(1),
                    imm: u64::MAX,
                },
                MInst::MovImm {
                    dst: word(2),
                    imm: 7,
                },
                MInst::FlagBinary {
                    op: FlagOp::Adds,
                    dst: word(3),
                    flags: flags(4),
                    lhs: word(0),
                    rhs: word(1),
                    width: Width::X,
                },
            ];
            if chain {
                insts.push(MInst::SubCarryFlags {
                    dst: flags(5),
                    flags_in: flags(4),
                    lhs: word(3),
                    rhs: word(2),
                    width: Width::X,
                });
                insts.push(MInst::Cset {
                    dst: word(6),
                    flags: flags(5),
                    cond: Cond::Hi,
                });
            } else {
                insts.push(MInst::CarryBinary {
                    op: CarryOp::Adc,
                    dst: word(6),
                    flags: flags(4),
                    lhs: word(3),
                    rhs: word(2),
                    width: Width::X,
                });
            }
            let function = MFunction {
                name: "carry_spill".into(),
                blocks: vec![MBlock {
                    id: 0,
                    args: vec![],
                    insts,
                    term: MTerm::Ret {
                        values: vec![word(6)],
                    },
                }],
            };
            for victims in [vec![2], vec![3], vec![2, 3, 6]] {
                let slots = victims
                    .iter()
                    .enumerate()
                    .map(|(slot, reg)| (*reg, slot as u32))
                    .collect();
                let rewritten = insert_spill_code(&function, &slots, &victims, &mut 100);
                crate::validate::validate_function(&rewritten).unwrap();
                let mut allocated = allocate(&rewritten).unwrap();
                allocated.frame_slots = slots.len() as u32;
                crate::verify_allocation(&allocated).unwrap();
            }
        }
    }

    #[test]
    fn spill_stores_after_a_flag_setting_branch_execute_on_each_edge() {
        let function = MFunction {
            name: "branch_spill".into(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        MInst::MovImm {
                            dst: word(0),
                            imm: 1,
                        },
                        MInst::MovImm {
                            dst: word(1),
                            imm: 2,
                        },
                        MInst::FlagBinary {
                            op: FlagOp::Adds,
                            dst: word(3),
                            flags: flags(4),
                            lhs: word(0),
                            rhs: word(1),
                            width: Width::X,
                        },
                    ],
                    term: MTerm::BCond {
                        flags: flags(4),
                        cond: Cond::Eq,
                        taken: 1,
                        taken_args: vec![],
                        untaken: 2,
                        untaken_args: vec![],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![],
                    insts: vec![],
                    term: MTerm::Ret {
                        values: vec![word(3)],
                    },
                },
                MBlock {
                    id: 2,
                    args: vec![],
                    insts: vec![],
                    term: MTerm::Ret {
                        values: vec![word(3)],
                    },
                },
            ],
        };
        let rewritten = insert_spill_code(&function, &BTreeMap::from([(3, 0)]), &[3], &mut 100);
        let mut allocated = allocate(&rewritten).unwrap();
        allocated.frame_slots = 1;
        crate::verify_allocation(&allocated).unwrap();
        assert_eq!(
            rewritten
                .blocks
                .iter()
                .flat_map(|b| &b.insts)
                .filter(|i| matches!(i, MInst::SpillStore { .. }))
                .count(),
            2
        );
    }
}
