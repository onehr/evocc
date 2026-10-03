//! Exact repeated extension selection before allocation. No range inference.
use std::collections::{BTreeMap, BTreeSet};

use super::local::Counts;
use super::patterns::{ExtensionKind, EXTENSION_PATTERNS};
use crate::generic::allocation::AllocTerm;
use crate::generic::machine::{MachInst, MachTerm};
use crate::{BinOp, ExtOp, MFunction, MInst, RegClass, VReg, Width};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Expression {
    Extend(ExtOp, VReg, RegClass),
    Extract(bool, VReg, RegClass, u16, Width),
    Mask(VReg, RegClass, u64, Width),
}

fn gpr_width(class: RegClass) -> Option<u32> {
    match class {
        RegClass::Gpr32 => Some(32),
        RegClass::Gpr64 => Some(64),
        _ => None,
    }
}

fn candidate(
    inst: &MInst,
    aliases: &BTreeMap<u32, VReg>,
) -> Option<(&'static str, Expression, VReg, VReg)> {
    let resolve = |reg: VReg| aliases.get(&reg.id).copied().unwrap_or(reg);
    let (kind, key, src, dst) = match *inst {
        MInst::Extend { op, src, dst } => {
            let src = resolve(src);
            (
                ExtensionKind::Extend,
                Expression::Extend(op, src, dst.class),
                src,
                dst,
            )
        }
        MInst::Bfx {
            signed,
            src,
            dst,
            lsb: 0,
            bits,
            width,
        } if bits != 0 && bits < width.bits() && dst.class == width.class() => {
            let src = resolve(src);
            (
                ExtensionKind::LowExtract,
                Expression::Extract(signed, src, dst.class, bits, width),
                src,
                dst,
            )
        }
        MInst::BinaryImm {
            op: BinOp::And,
            dst,
            lhs,
            imm,
            width,
        } if imm != 0
            && imm & imm.wrapping_add(1) == 0
            && imm < width.mask()
            && dst.class == width.class() =>
        {
            let src = resolve(lhs);
            (
                ExtensionKind::LowMask,
                Expression::Mask(src, dst.class, imm, width),
                src,
                dst,
            )
        }
        _ => return None,
    };
    gpr_width(src.class)?;
    let width = gpr_width(dst.class)?;
    let pattern = EXTENSION_PATTERNS
        .iter()
        .find(|p| p.kind == kind && p.widths.contains(&width))?;
    Some((pattern.id, key, src, dst))
}

/// Return a changed virtual function only if an exact repeated selection exists.
/// The caller still performs ordinary SSA destruction, allocation and validation.
pub fn select(function: &MFunction) -> (Option<MFunction>, Counts) {
    let mut counts = Counts::default();
    let empty = BTreeMap::new();
    if !function
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .any(|i| candidate(i, &empty).is_some())
    {
        return (None, counts);
    }
    let mut definitions = BTreeMap::<u32, usize>::new();
    let mut use_block = BTreeMap::<u32, Option<u32>>::new();
    for block in &function.blocks {
        for reg in block
            .args
            .iter()
            .copied()
            .chain(block.insts.iter().flat_map(MachInst::defs))
            .chain(block.term.defs())
        {
            *definitions.entry(reg.id).or_default() += 1;
        }
        for reg in block
            .insts
            .iter()
            .flat_map(MachInst::uses)
            .chain(block.term.uses())
        {
            use_block
                .entry(reg.id)
                .and_modify(|owner| {
                    if *owner != Some(block.id) {
                        *owner = None;
                    }
                })
                .or_insert(Some(block.id));
        }
        for (_, args) in block.term.edges() {
            for reg in args {
                use_block.insert(reg.id, None);
            }
        }
    }
    let mut changes = Vec::new();
    for (index, block) in function.blocks.iter().enumerate() {
        let mut available = BTreeMap::<Expression, VReg>::new();
        let mut aliases = BTreeMap::<u32, VReg>::new();
        let mut removed = BTreeSet::new();
        for (position, inst) in block.insts.iter().enumerate() {
            if !inst.discardable() {
                available.clear();
            }
            let Some((pattern, key, src, dst)) = candidate(inst, &aliases) else {
                continue;
            };
            let reason =
                if definitions.get(&src.id) != Some(&1) || definitions.get(&dst.id) != Some(&1) {
                    "nonunique-definition"
                } else if use_block.get(&dst.id) != Some(&Some(block.id)) {
                    "nonlocal-destination-use"
                } else if let Some(prior) = available.get(&key).copied() {
                    // Only retained definitions enter available, so no alias chase.
                    aliases.insert(dst.id, prior);
                    removed.insert(position);
                    *counts.selected.entry(pattern).or_default() += 1;
                    counts.removed_instructions += 1;
                    continue;
                } else {
                    available.insert(key, dst);
                    "not-repeated-in-segment"
                };
            *counts.declined.entry((pattern, reason)).or_default() += 1;
        }
        if !removed.is_empty() {
            changes.push((index, aliases, removed));
        }
    }
    if changes.is_empty() {
        return (None, counts);
    }
    let mut result = function.clone();
    for (index, aliases, removed) in changes {
        let block = &mut result.blocks[index];
        block.insts = std::mem::take(&mut block.insts)
            .into_iter()
            .enumerate()
            .filter_map(|(position, mut inst)| {
                if removed.contains(&position) {
                    return None;
                }
                for reg in inst.operand_slots(false) {
                    if let Some(retained) = aliases.get(&reg.id) {
                        *reg = *retained;
                    }
                }
                Some(inst)
            })
            .collect();
        for reg in block.term.operand_slots() {
            if let Some(retained) = aliases.get(&reg.id) {
                *reg = *retained;
            }
        }
    }
    (Some(result), counts)
}
