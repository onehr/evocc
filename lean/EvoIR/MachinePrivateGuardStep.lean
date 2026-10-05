import EvoIR.MachinePrivateBodyCut
import EvoIR.MachinePrivateGuardChain

/-! One actual guarded IR instruction against its ordered native guard chain.
The present checks are outcomes, not premises. Only earlier checks that the
caller has already matched may justify conditional scalar normalization. -/
namespace EvoIR.Core.Symbolic.MachineSelection

structure PrivateGuardStepResult (Γ : Var → Nat) where
  checks : List (Guard Γ)
  next : PrivateBank Γ
  targetBlock : EvoIR.Machine.Block
  targetPC : Nat

def PrivateGuardStepDiagram {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) (prior : List (Guard Γ)) (result : PrivateGuardStepResult Γ) : Prop :=
  ∀ σ values incoming owner frame memory regs logicalInputs sourceStack targetStack,
    before.Models σ values incoming owner frame memory regs → guards σ prior = .ok () →
    ∃ sourceLast targetLast,
      Transition.Reaches (IR.step source)
        (privateState σ values sf sb.name spc owner frame before.source sourceStack) sourceLast ∧
      Transition.Reaches (EvoIR.Machine.step .ssa target)
        (privateMachineState tf tb tpc regs owner frame logicalInputs targetStack memory) targetLast ∧
      match guards σ result.checks with
      | .error fault => sourceLast.control = .halted (.error fault) ∧ targetLast.control = .halted (.error fault)
      | .ok _ => ∃ newValues nextRegs,
          sourceLast = privateState σ newValues sf sb.name (spc+1) owner frame result.next.source sourceStack ∧
          targetLast = privateMachineState tf result.targetBlock result.targetPC nextRegs owner frame
            logicalInputs targetStack memory ∧
          result.next.Models σ newValues incoming owner frame memory nextRegs ∧
          guards σ (prior++result.checks) = .ok ()

theorem guards_fault_observation {Γ : Var → Nat} (σ : Subst Γ) (checks : List (Guard Γ))
    (fault : Source.Fault) (failed : guards σ checks = .error fault) :
    ∃ code, fault = .diagnostic code ∧ checkedDiagnostic code = some (.diagnostic code) := by
  induction checks with
  | nil => simp [guards] at failed
  | cons g rest ih =>
    simp only [guards] at failed
    split at failed
    · have same := Except.error.inj failed
      refine ⟨g.reason.code, same.symm, ?_⟩
      cases g.reason <;> simp [IR.Reason.code, checkedDiagnostic]
    · exact ih failed

private def privateGuardSourceNoncall (inst : IR.Instruction) : Bool :=
  match inst.op with
  | .call _ => false
  | _ => true

private theorem private_guard_source_noncall (inst : IR.Instruction)
    (checked : privateGuardSourceNoncall inst = true) : ∀ name, inst.op ≠ .call name := by
  intro name called
  simp [privateGuardSourceNoncall, called] at checked

def privateGuardStep {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) (prior : List (Guard Γ)) :
    Option {result : PrivateGuardStepResult Γ //
      PrivateGuardStepDiagram source target sf sb spc tf tb tpc before prior result} := do
  if foundFunction : source.functions.find? (·.name == sf.name) = some sf then
    if foundBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      let inst : {inst // sb.instructions[spc]? = some inst} ←
        match found : sb.instructions[spc]? with
        | none => none
        | some inst => some ⟨inst, found⟩
      if noncall : privateGuardSourceNoncall inst.val = true then
        let sourceRow ← privateSourceGuardRow before inst.val
        if nonempty : sourceRow.row.checks ≠ [] then
          let chain ← privateNativeGuardChain target tf sourceRow.row.checks prior tb tpc before
          let resumed ← privateSourceGuardRow chain.val.bank inst.val
          if sameChecks : resumed.row.checks = sourceRow.row.checks then
            return ⟨⟨sourceRow.row.checks, resumed.row.next, chain.val.block, chain.val.pc⟩, by
              intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack models earlier
              obtain ⟨targetLast, targetPath, targetOutcome⟩ := chain.property.2 σ values incoming owner frame memory
                regs logicalInputs targetStack models earlier
              cases checked : guards σ sourceRow.row.checks with
              | error fault =>
                have targetFault : targetLast.control = .halted (.error fault) := by
                  simpa only [checked] using targetOutcome
                have sourceFault := sourceRow.fault σ values owner fault checked
                have stepped := private_source_instruction_fault source sf sb spc owner frame before.source
                  σ values sourceStack inst.val fault foundFunction foundBlock inst.property
                  (private_guard_source_noncall inst.val noncall) sourceFault
                refine ⟨⟨.halted (.error fault), sourceStack, frame⟩, targetLast,
                  .next stepped (.refl _), targetPath, ?_⟩
                simp only [checked]
                exact ⟨True.intro, targetFault⟩
              | ok unit =>
                cases unit
                obtain ⟨nextRegs, targetAt, nativeModels⟩ := (by
                  simpa only [checked] using targetOutcome :
                    ∃ nextRegs,
                      targetLast = privateMachineState tf chain.val.block chain.val.pc nextRegs owner frame
                        logicalInputs targetStack memory ∧
                      chain.val.bank.Models σ values incoming owner frame memory nextRegs)
                have resumedPassed : guards σ resumed.row.checks = .ok () := by rw [sameChecks]; exact checked
                obtain ⟨newValues, nextMemory, finalRegs, sourceRun, targetRun, nextModels⟩ :=
                  resumed.row.sound σ values incoming owner frame memory nextRegs nativeModels resumedPassed
                have unchanged : nextRegs = finalRegs ∧ memory = nextMemory := by
                  simpa only [PrivateTargetInstruction, Except.ok.injEq, Prod.mk.injEq] using targetRun
                rcases unchanged with ⟨sameRegs, sameMemory⟩
                subst finalRegs
                subst nextMemory
                have evaluated : concreteInstruction owner (before.source.scope.denote σ (privateToken values)) inst.val =
                    .ok (resumed.row.next.source.scope.denote σ (privateToken newValues)) := by
                  simpa only [PrivateSourceInstruction, chain.property.1] using sourceRun
                have stepped := private_source_instruction_step source sf sb spc owner frame before.source
                  resumed.row.next.source σ values newValues sourceStack inst.val foundFunction foundBlock inst.property
                  (private_guard_source_noncall inst.val noncall) evaluated
                refine ⟨privateState σ newValues sf sb.name (spc+1) owner frame resumed.row.next.source sourceStack,
                  targetLast, .next stepped (.refl _), targetPath, ?_⟩
                simp only [checked]
                refine ⟨newValues, nextRegs, rfl, targetAt, nextModels, ?_⟩
                rw [guards_append, earlier, checked]
                rfl⟩
          else none
        else none
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
