//! Select a zero branch through a private, zero-preserving integer copy.
use std::collections::BTreeMap;

use crate::{inst_defs, inst_uses, term_uses, Cond, MBlock, MInst, MTerm, RegClass, VReg};

pub(super) fn select(blocks: &mut [MBlock]) {
    let mut counts: BTreeMap<VReg, (usize, usize)> = BTreeMap::new();
    for block in blocks.iter() {
        for reg in block
            .args
            .iter()
            .copied()
            .chain(block.insts.iter().flat_map(inst_defs))
        {
            counts.entry(reg).or_default().0 += 1;
        }
        // Include outgoing arguments: a copied value or flags passed to a
        // successor are not private, even when that successor ignores them.
        for reg in block
            .insts
            .iter()
            .flat_map(inst_uses)
            .chain(term_uses(&block.term))
        {
            counts.entry(reg).or_default().1 += 1;
        }
    }
    for block in blocks {
        let MTerm::BCond {
            flags,
            cond,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } = &block.term
        else {
            continue;
        };
        if !matches!(cond, Cond::Eq | Cond::Ne) || counts.get(flags) != Some(&(1, 1)) {
            continue;
        }
        let Some(MInst::CmpImm {
            dst,
            lhs,
            imm: 0,
            width,
        }) = block.insts.last()
        else {
            continue;
        };
        if dst != flags || lhs.class != width.class() || counts.get(lhs) != Some(&(1, 1)) {
            continue;
        }
        let Some((index, src)) = block.insts.iter().enumerate().find_map(|(index, inst)| {
            let MInst::Mov { dst, src } = inst else {
                return None;
            };
            // Narrowing X to W can change nonzero to zero. Do not reinterpret
            // a virtual register's class to evade that distinction. Same-width
            // copies and W-to-X zero extension preserve exactly the zero test.
            (*dst == *lhs
                && matches!(
                    (src.class, dst.class),
                    (RegClass::Gpr32, RegClass::Gpr32 | RegClass::Gpr64)
                        | (RegClass::Gpr64, RegClass::Gpr64)
                ))
            .then_some((index, *src))
        }) else {
            continue;
        };
        block.term = MTerm::Cb {
            nonzero: *cond == Cond::Ne,
            src,
            taken: *taken,
            taken_args: taken_args.clone(),
            untaken: *untaken,
            untaken_args: untaken_args.clone(),
        };
        block.insts.pop();
        block.insts.remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Width;

    fn copied_test(source: RegClass, destination: RegClass) -> MBlock {
        let src = VReg::new(0, source);
        let dst = VReg::new(1, destination);
        let flags = VReg::new(2, RegClass::Flags);
        MBlock {
            id: 0,
            args: vec![src],
            insts: vec![
                MInst::Mov { dst, src },
                MInst::CmpImm {
                    dst: flags,
                    lhs: dst,
                    imm: 0,
                    width: if destination == RegClass::Gpr32 {
                        Width::W
                    } else {
                        Width::X
                    },
                },
            ],
            term: MTerm::BCond {
                flags,
                cond: Cond::Eq,
                taken: 1,
                taken_args: vec![],
                untaken: 2,
                untaken_args: vec![],
            },
        }
    }

    #[test]
    fn narrowing_keeps_the_copy_and_comparison() {
        let before = copied_test(RegClass::Gpr64, RegClass::Gpr32);
        let mut after = [before.clone()];
        select(&mut after);
        assert_eq!(after[0], before);
    }

    #[test]
    fn shared_operands_keep_the_copy_and_comparison() {
        for original in [
            copied_test(RegClass::Gpr32, RegClass::Gpr32),
            copied_test(RegClass::Gpr32, RegClass::Gpr64),
            copied_test(RegClass::Gpr64, RegClass::Gpr64),
        ] {
            let MInst::Mov { dst, .. } = original.insts[0] else {
                unreachable!()
            };
            for reg in [dst, VReg::new(2, RegClass::Flags)] {
                let mut before = original.clone();
                let MTerm::BCond { taken_args, .. } = &mut before.term else {
                    unreachable!()
                };
                taken_args.push(reg);
                let mut after = [before.clone()];
                select(&mut after);
                assert_eq!(after[0], before);
            }
        }
    }
}
