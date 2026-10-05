import EvoIR.MachinePrivateSuffix
import EvoIR.MachinePrivateGuardStep

/-!
Close private blocks with ordered checked-arithmetic failures. Each recursive
construction step consumes one actual guarded source instruction. It does not
unroll a program loop: real control edges leave through the checked CFG table.
Calls are a separate continuation case and are refused by this builder.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem private_suffix_guard {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (type : Source.IntType) (table : List (PrivateCutpoint sf tf))
    (sb : IR.Block) (spc : Nat) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (bank : PrivateBank Γ) (prior : List (Guard Γ))
    (result : PrivateGuardStepResult Γ)
    (step : PrivateGuardStepDiagram source target sf sb spc tf tb tpc bank prior result)
    (finish : PrivateSuffixDiagram source target sf tf type table sb (spc+1)
      result.targetBlock result.targetPC result.next (prior ++ result.checks)) :
    PrivateSuffixDiagram source target sf tf type table sb spc tb tpc bank prior := by
  intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
    observationType models passed
  obtain ⟨sourceLast, targetLast, sourcePath, targetPath, outcome⟩ :=
    step σ values incoming owner frame memory regs logicalInputs sourceStack targetStack models passed
  cases checked : guards σ result.checks with
  | error fault =>
    have halted := outcome
    simp only [checked] at halted
    obtain ⟨code, faultCode, observed⟩ := guards_fault_observation σ result.checks fault checked
    subst fault
    apply Transition.OpenCheckpoint.of_refines
    apply Transition.refines_of_terminals sourcePath targetPath
      (by simp [IR.step, halted.1]) (by simp [EvoIR.Machine.step, halted.2])
      (o := Observation.diagnostic code)
    · simpa only [observeTarget, halted.1] using observed
    · simpa only [EvoIR.Machine.observe, halted.2] using observed
  | ok unit =>
    cases unit
    have succeeded := outcome
    simp only [checked] at succeeded
    obtain ⟨newValues, nextRegs, sourceAt, targetAt, nextModels, allPassed⟩ := succeeded
    rw [sourceAt] at sourcePath
    rw [targetAt] at targetPath
    exact (finish σ newValues incoming owner frame memory nextRegs logicalInputs
      sourceStack targetStack observationType nextModels allPassed).prepend sourcePath targetPath

def privateGuardedSuffix {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (type : Source.IntType) (table : List (PrivateCutpoint sf tf)) :
    (constructionDepth : Nat) → (sb : IR.Block) → (spc : Nat) →
    (tb : EvoIR.Machine.Block) → (tpc : Nat) → (bank : PrivateBank Γ) →
    (prior : List (Guard Γ)) →
    Option (PLift (PrivateSuffixDiagram source target sf tf type table sb spc tb tpc bank prior))
  | 0, _, _, _, _, _, _ => none
  | depth+1, sb, spc, tb, tpc, bank, prior => do
    let cut ← privateBodyCut source target sf sb spc tf tb tpc bank
    let finish : PLift (PrivateSuffixDiagram source target sf tf type table
        sb (spc + cut.result.sourceCount) tb (tpc + cut.result.targetCount) cut.result.next prior) ←
      if sourceEnd : spc + cut.result.sourceCount = sb.instructions.length then
        if targetEnd : tpc + cut.result.targetCount = tb.insts.length then do
          let control ← privateControl source target sf tf type table sb tb cut.result.next prior
          pure ⟨by
            simpa only [sourceEnd, targetEnd] using
              private_suffix_control source target sf tf type table sb tb cut.result.next prior control.down⟩
        else none
      else
        match privateStoreSpan source target sf sb (spc + cut.result.sourceCount)
            tf tb (tpc + cut.result.targetCount) cut.result.next prior with
        | some stored => do
          let rest ← privateGuardedSuffix source target sf tf type table depth
            sb (spc + cut.result.sourceCount + stored.val.sourceCount)
            tb (tpc + cut.result.targetCount + stored.val.targetCount) stored.val.next prior
          pure ⟨private_suffix_span_under source target sf tf type table sb
            (spc + cut.result.sourceCount) tb (tpc + cut.result.targetCount)
            cut.result.next prior stored rest.down⟩
        | none => do
          let guarded ← privateGuardStep source target sf sb (spc + cut.result.sourceCount)
            tf tb (tpc + cut.result.targetCount) cut.result.next prior
          let rest ← privateGuardedSuffix source target sf tf type table depth
            sb (spc + cut.result.sourceCount + 1) guarded.val.targetBlock guarded.val.targetPC
            guarded.val.next (prior ++ guarded.val.checks)
          pure ⟨private_suffix_guard source target sf tf type table sb
            (spc + cut.result.sourceCount) tb (tpc + cut.result.targetCount)
            cut.result.next prior guarded.val guarded.property rest.down⟩
    return ⟨private_suffix_cut source target sf tf type table sb spc tb tpc bank prior cut finish.down⟩

def privateGuardedBlock (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (point : PrivateCutpoint sf tf) :
    Option (PLift (PrivateBlockDiagram source target sf tf type table point)) := do
  let checked ← privateGuardedSuffix source target sf tf type table
    (point.source.block.instructions.length+1) point.source.block 0 point.targetBlock 0 point.bank []
  return ⟨private_suffix_block source target sf tf type table point checked.down⟩

end EvoIR.Core.Symbolic.MachineSelection
