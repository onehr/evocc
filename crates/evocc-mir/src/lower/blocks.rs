//! Per-block instruction-selection state and obligation edges.

use super::*;

pub(crate) struct BlockCursor {
    pub(crate) id: u32,
    pub(crate) args: Vec<VReg>,
    pub(crate) insts: Vec<MInst>,
    // An icmp fused into this block's conditional terminator.
    fused: Option<(VReg, Cond)>,
    // A checked arithmetic result transported through an explicit block argument.
    computed_result: Option<usize>,
    // Pure selection chains have no externally used intermediate to emit again.
    selected_until: usize,
}

impl BlockCursor {
    pub(crate) fn new(
        lowerer: &mut Lowerer,
        entry: bool,
        id: u32,
        args: Vec<VReg>,
        prologue: Vec<MInst>,
        indirect_result: bool,
    ) -> Self {
        let mut insts: Vec<MInst> = Vec::new();
        // Section 6 (revision 0.18): the marker allocation reads is the first
        // thing in the entry block, before the register save area.
        if indirect_result {
            insts.push(MInst::IndirectResult);
        }
        // Section 6: the register save area is filled "in the prologue", before
        // anything writes an argument register. The marker is the first thing in
        // the entry block, which is where the frame layout finds it.
        if entry {
            if let Some((named, _, _)) = lowerer.named_arguments {
                insts.push(MInst::VaSaveArea { named });
            }
        }
        insts.extend(prologue);
        Self {
            id,
            args,
            insts,
            fused: None,
            computed_result: None,
            selected_until: 0,
        }
    }

    pub(crate) fn should_skip(&mut self, index: usize) -> bool {
        if index < self.selected_until {
            return true;
        }
        if self.computed_result == Some(index) {
            self.computed_result = None;
            return true;
        }
        false
    }

    pub(crate) fn lower_obligation(
        &mut self,
        lowerer: &mut Lowerer,
        block: &Block,
        use_counts: &HashMap<u32, usize>,
        index: usize,
        inst: &Inst,
        canonical_true: &HashSet<u32>,
    ) -> Result<bool, String> {
        if let Inst::UbAssume(condition, reason) = inst {
            if canonical_true.contains(&condition.id) {
                return Ok(true);
            }
            if lowerer.diagnostic_true_predicates.contains(&condition.id) {
                return Ok(true);
            }
            // EvoMIR 1.1 and 5.2: both profiles lower the obligation of a
            // call without a prototype of an old-style definition to
            // nothing. EvoCC compiles that call as GCC 14 does; only the
            // interpreters and `--check` report its undefined behavior.
            if lowerer.ub_checks == UbCheckProfile::Unchecked || *reason == UbReason::OldStyleCall {
                return Ok(true);
            }
            // Section 5: the guard still branches to its reason's trap. An
            // adjacent, single-use CSET can give its flags directly to that
            // branch; otherwise test the materialized boolean's low bit.
            // Instructions after the obligation enter a continuation through
            // explicit block arguments, whichever branch spelling is selected.
            let checked_flags = take_checked_flags(
                &mut self.insts,
                lowerer,
                block,
                use_counts,
                index,
                condition,
            )?;
            let condition = lowerer.reg(condition)?;
            let imm = ub_immediate(*reason);
            let trap = lowerer.trap_block(imm);
            // If the next pure arithmetic instruction has already been
            // computed by its predicate, transport its result, not operands
            // used only by that otherwise-repeated instruction.
            let from = index + 1 + usize::from(self.computed_result == Some(index + 1));
            let carried = live_across(lowerer, block, from);
            let continuation = lowerer.fresh_block();
            let mut continuation_args = Vec::new();
            let mut rebind = HashMap::new();
            for reg in &carried {
                let fresh = lowerer.fresh(reg.class);
                continuation_args.push(fresh);
                rebind.insert(reg.id, fresh);
            }
            let term = if let Some((flags, valid_when)) = checked_flags {
                MTerm::BCond {
                    flags,
                    cond: valid_when.inverted(),
                    taken: trap,
                    taken_args: Vec::new(),
                    untaken: continuation,
                    untaken_args: carried,
                }
            } else {
                MTerm::Tb {
                    nonzero: false,
                    src: condition,
                    bit: 0,
                    taken: trap,
                    taken_args: Vec::new(),
                    untaken: continuation,
                    untaken_args: carried,
                }
            };
            lowerer.blocks.push(MBlock {
                id: self.id,
                args: std::mem::take(&mut self.args),
                insts: std::mem::take(&mut self.insts),
                term,
            });
            // Both limbs are local SSA values. A checked operation splits
            // the block, so the continuation must use both new bindings.
            for reg in lowerer.map.values_mut().chain(lowerer.high.values_mut()) {
                if let Some(fresh) = rebind.get(&reg.id) {
                    *reg = *fresh;
                }
            }
            self.id = continuation;
            self.args = continuation_args;
            lowerer.address_preparation = AddressPreparation::default();
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn select_darwin_product(
        &mut self,
        target: Target,
        lowerer: &mut Lowerer,
        block: &Block,
        index: usize,
    ) -> Result<bool, String> {
        // A Darwin complex product branches to its helper, so it is written
        // here, where a block can be split.
        if let Some(length) = lower_darwin_product(
            target,
            lowerer,
            &mut self.insts,
            (&mut self.id, &mut self.args),
            block,
            index,
        )? {
            self.selected_until = index + length;
            lowerer.address_preparation = AddressPreparation::default();
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn fuse_compare(
        &mut self,
        lowerer: &mut Lowerer,
        block: &Block,
        use_counts: &HashMap<u32, usize>,
        index: usize,
        inst: &Inst,
    ) -> Result<bool, String> {
        // Section 5: an `icmp` "when the only consumer is a cond_br in the same
        // block" becomes `cmp` then `b.cond` with no `cset`. The fusion is
        // taken only when the comparison is the block's last instruction,
        // because Section 2 requires the `cmp` and its consumer to be adjacent.
        if let Inst::ICmp(result, operation, lhs, rhs) = inst {
            if index + 1 == block.insts.len() {
                if let Inst::CondBr(condition, ..) = &block.terminator {
                    if condition.id == result.id && use_counts.get(&result.id) == Some(&1) {
                        let (flags, cond) =
                            lower_compare(lowerer, &mut self.insts, *operation, lhs, rhs)?;
                        self.fused = Some((flags, cond));
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_selected(
        &mut self,
        target: Target,
        signatures: &Signatures<'_>,
        lowerer: &mut Lowerer,
        block: &Block,
        use_counts: &HashMap<u32, usize>,
        index: usize,
        inst: &Inst,
        external: &dyn Fn(&str) -> bool,
    ) -> Result<(), String> {
        if let Some(value) = evocc_evoir::inst_results(inst)
            .first()
            .filter(|v| lowerer.diagnostic_true_predicates.contains(&v.id))
        {
            // Diagnostic only: retain every other data user of the predicate.
            let result = lowerer.fresh(RegClass::Gpr32);
            lowerer.bind(value, result);
            self.insts.push(MInst::MovImm {
                dst: result,
                imm: 1,
            });
        } else if let Some(length) =
            lower_compared_selection(lowerer, &mut self.insts, block, use_counts, index)?
        {
            self.selected_until = index + length;
        } else if lowerer.ub_checks == UbCheckProfile::Checked
            && lower_checked_result(lowerer, &mut self.insts, block, index)?
        {
            self.computed_result = Some(index + 2);
        } else if let Some(length) =
            lower_complex_parts(target, lowerer, &mut self.insts, block, index)?
        {
            self.selected_until = index + length;
        } else {
            lower_inst(target, signatures, lowerer, &mut self.insts, inst, external)?;
        }
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
        lowerer: &mut Lowerer,
        function: &Function,
        block: &Block,
        ids: &HashMap<u32, u32>,
    ) -> Result<(), String> {
        let term = lower_terminator(
            lowerer,
            &mut self.insts,
            function,
            block,
            self.id,
            ids,
            self.fused,
        )?;
        lowerer.blocks.push(MBlock {
            id: self.id,
            args: self.args,
            insts: self.insts,
            term,
        });
        Ok(())
    }
}
