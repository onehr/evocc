//! Bounded control idioms after demand DCE/SSA destruction, before allocation.
//! No instruction motion, new flag lifetime, CFG inference or range facts.
use std::collections::BTreeMap;

use super::encodings::add_sub_immediate;
use super::local::Counts;
use super::patterns::{ControlKind, CONTROL_PATTERNS};
use crate::generic::allocation::AllocTerm;
use crate::generic::machine::{MachInst, MachTerm};
use crate::{BinOp, Cond, MFunction, MInst, MTerm, RegClass, VReg, Width};

type Occurrences = BTreeMap<u32, (usize, usize)>;

fn occurrences(function: &MFunction) -> Occurrences {
    let mut counts = Occurrences::new();
    for block in &function.blocks {
        for reg in block
            .args
            .iter()
            .copied()
            .chain(block.insts.iter().flat_map(MachInst::defs))
            .chain(block.term.defs())
        {
            counts.entry(reg.id).or_default().0 += 1;
        }
        // MachTerm::uses includes every outgoing edge argument.
        for reg in block
            .insts
            .iter()
            .flat_map(MachInst::uses)
            .chain(block.term.uses())
        {
            counts.entry(reg.id).or_default().1 += 1;
        }
    }
    counts
}

fn private(counts: &Occurrences, reg: VReg) -> bool {
    counts.get(&reg.id) == Some(&(1, 1))
}

fn pattern(kind: ControlKind, width: Width) -> Option<&'static str> {
    CONTROL_PATTERNS
        .iter()
        .find(|p| p.kind == kind && p.widths.contains(&(width.bits() as u32)))
        .map(|p| p.id)
}

fn interval(insts: &[MInst], uses: &Occurrences) -> Result<Vec<MInst>, &'static str> {
    let [MInst::CmpImm {
        dst: f1,
        lhs: x,
        imm: lo,
        width,
    }, MInst::Cset {
        dst: a,
        flags: af,
        cond: ac,
    }, MInst::CmpImm {
        dst: f2,
        lhs: y,
        imm: hi,
        width: yw,
    }, MInst::Cset {
        dst: b,
        flags: bf,
        cond: bc,
    }, MInst::Binary {
        op: BinOp::And,
        dst,
        lhs,
        rhs,
        width: bw,
    }] = insts
    else {
        return Err("shape");
    };
    if x != y
        || width != yw
        || x.class != width.class()
        || a.class != bw.class()
        || b.class != bw.class()
        || dst.class != bw.class()
        || af != f1
        || bf != f2
        || f1.class != RegClass::Flags
        || f2.class != RegClass::Flags
        || !((*lhs == *a && *rhs == *b) || (*lhs == *b && *rhs == *a))
    {
        return Err("register-or-width");
    }
    if !matches!((ac, bc), (Cond::Hs, Cond::Ls) | (Cond::Ge, Cond::Le)) {
        return Err("not-inclusive-interval");
    }
    if lo > hi || !add_sub_immediate(*lo) || !add_sub_immediate(*hi) {
        return Err("endpoints");
    }
    if [*f1, *f2, *a, *b]
        .into_iter()
        .any(|reg| !private(uses, reg))
    {
        return Err("nonprivate-intermediate");
    }
    // The only old reference to a is the removed AND. Its unique identity can
    // carry the subtraction at the input width without introducing a new ID.
    let tmp = VReg::new(a.id, width.class());
    let mut replacement = Vec::with_capacity(3);
    if *lo != 0 {
        replacement.push(MInst::BinaryImm {
            op: BinOp::Sub,
            dst: tmp,
            lhs: *x,
            imm: *lo,
            width: *width,
        });
    }
    replacement.push(MInst::CmpImm {
        dst: *f1,
        lhs: if *lo == 0 { *x } else { tmp },
        imm: hi - lo,
        width: *width,
    });
    replacement.push(MInst::Cset {
        dst: *dst,
        flags: *f1,
        cond: Cond::Ls,
    });
    Ok(replacement)
}

fn branch(insts: &[MInst], term: &MTerm, uses: &Occurrences) -> Result<MTerm, &'static str> {
    let [compare, MInst::Cset { dst, flags, cond }] = insts else {
        return Err("shape");
    };
    let (compare_flags, width) = match compare {
        MInst::Cmp {
            dst,
            lhs,
            rhs,
            width,
        } if lhs.class == width.class() && rhs.class == width.class() => (*dst, *width),
        MInst::CmpImm {
            dst,
            lhs,
            imm,
            width,
        } if lhs.class == width.class() && add_sub_immediate(*imm) => (*dst, *width),
        _ => return Err("not-integer-compare"),
    };
    if compare_flags != *flags
        || flags.class != RegClass::Flags
        || !matches!(dst.class, RegClass::Gpr32 | RegClass::Gpr64)
        || pattern(ControlKind::BooleanBranch, width).is_none()
    {
        return Err("register-or-width");
    }
    let (nonzero, src, taken, taken_args, untaken, untaken_args) = match term {
        MTerm::Cb {
            nonzero,
            src,
            taken,
            taken_args,
            untaken,
            untaken_args,
        }
        | MTerm::Tb {
            nonzero,
            src,
            bit: 0,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } => (*nonzero, *src, *taken, taken_args, *untaken, untaken_args),
        _ => return Err("not-boolean-branch"),
    };
    if src != *dst {
        return Err("register-or-width");
    }
    if !private(uses, *dst) || !private(uses, *flags) {
        return Err("nonprivate-intermediate");
    }
    Ok(MTerm::BCond {
        flags: *flags,
        cond: if nonzero { *cond } else { cond.inverted() },
        taken,
        taken_args: taken_args.clone(),
        untaken,
        untaken_args: untaken_args.clone(),
    })
}

/// Selected results still go through ordinary allocation and its verifier.
/// Global occurrence counts also reject post-SSA multiple definitions and
/// nonlocal/edge users of the intermediates removed here.
pub fn select(function: &MFunction) -> (Option<MFunction>, Counts) {
    let mut counts = Counts::default();
    if !function
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .any(|i| matches!(i, MInst::Cset { .. }))
    {
        return (None, counts);
    }
    let uses = occurrences(function);
    let mut changes = Vec::new();
    for (index, block) in function.blocks.iter().enumerate() {
        let mut windows = Vec::new();
        let mut position = 0;
        while position + 5 <= block.insts.len() {
            let window = &block.insts[position..position + 5];
            if let [MInst::CmpImm { width, .. }, MInst::Cset { .. }, MInst::CmpImm { .. }, MInst::Cset { .. }, MInst::Binary { op: BinOp::And, .. }] =
                window
            {
                if let Some(id) = pattern(ControlKind::ConstantInterval, *width) {
                    match interval(window, &uses) {
                        Ok(replacement) => {
                            counts.removed_instructions += 5 - replacement.len();
                            *counts.selected.entry(id).or_default() += 1;
                            windows.push((position, replacement));
                            position += 5;
                            continue;
                        }
                        Err(reason) => *counts.declined.entry((id, reason)).or_default() += 1,
                    }
                }
            }
            position += 1;
        }
        if !windows.is_empty() {
            changes.push((index, windows));
        }
    }
    let mut result = if changes.is_empty() {
        None
    } else {
        Some(function.clone())
    };
    if let Some(result) = &mut result {
        for (index, windows) in changes {
            let original = std::mem::take(&mut result.blocks[index].insts);
            let mut start = 0;
            for (position, replacement) in windows {
                result.blocks[index]
                    .insts
                    .extend_from_slice(&original[start..position]);
                result.blocks[index].insts.extend(replacement);
                start = position + 5;
            }
            result.blocks[index]
                .insts
                .extend_from_slice(&original[start..]);
        }
    }
    // An interval's result may be the terminal Boolean. Recount its new
    // internal flags/use graph rather than consult stale pre-rewrite facts.
    let updated;
    let current = result.as_ref().unwrap_or(function);
    let uses = if result.is_some() {
        updated = occurrences(current);
        &updated
    } else {
        &uses
    };
    let mut branches = Vec::new();
    for (index, block) in current.blocks.iter().enumerate() {
        let n = block.insts.len();
        if n < 2 || !matches!(block.insts[n - 1], MInst::Cset { .. }) {
            continue;
        }
        let width = match block.insts[n - 2] {
            MInst::Cmp { width, .. } | MInst::CmpImm { width, .. } => width,
            _ => continue,
        };
        let Some(id) = pattern(ControlKind::BooleanBranch, width) else {
            continue;
        };
        match branch(&block.insts[n - 2..], &block.term, uses) {
            Ok(term) => {
                branches.push((index, term));
                *counts.selected.entry(id).or_default() += 1;
                counts.removed_instructions += 1;
            }
            Err(reason) => *counts.declined.entry((id, reason)).or_default() += 1,
        }
    }
    if !branches.is_empty() {
        let result = result.get_or_insert_with(|| function.clone());
        for (index, term) in branches {
            result.blocks[index].insts.pop();
            result.blocks[index].term = term;
        }
    }
    (result, counts)
}
