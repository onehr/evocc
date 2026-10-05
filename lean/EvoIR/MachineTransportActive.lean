import EvoIR.MachineTransportRelation

namespace EvoIR.Machine.Transport

theorem activeEntry {source target : Program} (ctx : Context source target)
    (before after : Activation)
    (sourceName : before.function = ctx.sourceFunction.name)
    (targetName : after.function = ctx.sourceFunction.name)
    (sourceId : before.block = ctx.sourceBlock.id)
    (targetId : after.block = ctx.sourceBlock.id)
    (owner : before.owner = after.owner) (incoming : before.incoming = after.incoming)
    (sourcePC : before.pc = 0) (targetPC : after.pc = ctx.plan.prefixLength)
    (ready : EntryReady ctx.sourceBlock.args ctx.plan.bindings before.regs after.regs) :
    Nonempty (Active ctx before after) := by
  obtain ⟨_, _, prefixBound, _, checked⟩ := checkBody_data ctx.sourceBlock ctx.targetBlock
    ctx.plan ctx.plans.needed ctx.final ctx.body
  obtain ⟨sourceReal, targetReal⟩ := entryReady_realizes ctx.sourceBlock.args ctx.plan.bindings
    before.regs after.regs ready
  refine ⟨{
    sourceName := sourceName, targetName, sourceId, targetId, owner, incoming,
    positions := ctx.plan.alignment,
    env := {
      source := sourceEntryOrigins ctx.sourceBlock.args ctx.plan.bindings,
      target := targetEntryOrigins ctx.plan.bindings,
      cuts := [⟨0, ctx.plan.prefixLength, sourceEntryOrigins ctx.sourceBlock.args ctx.plan.bindings,
        targetEntryOrigins ctx.plan.bindings⟩]},
    valuation := inputValuation before.regs after.regs, sourceBound := ?_, targetBound := ?_,
    accepted := ?_, sourceReal, targetReal, sourceBefore := ?_, targetBefore := ?_,
    normalized := inputValuation_normalized before.regs after.regs}⟩
  · simp only [sourcePC]; omega
  · simpa only [targetPC] using prefixBound
  · simpa only [sourcePC, targetPC, List.drop_zero] using checked
  · exact sourceEntryOrigins_before _ _ _ _
  · exact targetEntryOrigins_before _ _ _

theorem activeEntry_sameBank {source target : Program} (ctx : Context source target)
    (values : List Value) (owner : Nat) (entered : Activation)
    (isEntry : ctx.sourceBlock.id = 0)
    (sourceEntered : enter ctx.sourceFunction values owner = .ok entered) :
    enter ctx.targetFunction values owner = .ok entered ∧ Nonempty (Active ctx entered entered) := by
  cases entryAt : findBlock ctx.sourceFunction 0 with
  | none =>
    change ctx.sourceFunction.blocks.find? (·.id == 0) = none at entryAt
    simp [enter, entryAt, Bind.bind, Except.bind, MonadExcept.throw,
      throwThe, MonadExceptOf.throw] at sourceEntered
  | some entry =>
    have sourceEntryId : ctx.sourceBlock.id = 0 → ctx.sourceBlock = entry := by
      intro id
      have same := ctx.sourceBlockAt
      rw [id, entryAt] at same
      exact Option.some.inj same.symm
    have blockEq := sourceEntryId isEntry
    subst entry
    obtain ⟨args, prefixBound, identity⟩ := ctx.entryShape isEntry
    have targetAt : ctx.targetFunction.blocks.find? (·.id == 0) = some ctx.targetBlock := by
      simpa only [isEntry, findBlock] using ctx.targetBlockAt
    have sourceAt : ctx.sourceFunction.blocks.find? (·.id == 0) = some ctx.sourceBlock := by
      simpa only [isEntry, findBlock] using ctx.sourceBlockAt
    cases bound : bind [] ctx.sourceBlock.args values with
    | error fault => simp [enter, sourceAt, bound, Bind.bind, Except.bind] at sourceEntered
    | ok regs =>
      have shape : ({
          function := ctx.sourceFunction.name, block := ctx.sourceBlock.id, pc := 0, regs,
          owner, incoming := values} : Activation) = entered := by
        simpa [enter, sourceAt, bound, Bind.bind, Except.bind, Pure.pure, Except.pure] using sourceEntered
      subst entered
      have ids := (checkBody_data ctx.sourceBlock ctx.targetBlock ctx.plan ctx.plans.needed ctx.final ctx.body).2.1
      refine ⟨?_, activeEntry ctx _ _ rfl rfl rfl rfl rfl rfl rfl prefixBound.symm ?_⟩
      · simp [enter, targetAt, args, bound, ← ctx.names, ← ids, Bind.bind, Except.bind, Pure.pure, Except.pure]
      · obtain ⟨_, _, _, bindings, _⟩ := checkBody_data ctx.sourceBlock ctx.targetBlock ctx.plan ctx.plans.needed ctx.final ctx.body
        exact entryReady_same_bank ctx.sourceBlock ctx.plan ctx.plans.needed bindings identity regs
          (bind_normalized_reads [] ctx.sourceBlock.args values regs bound)

end EvoIR.Machine.Transport
