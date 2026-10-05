import EvoIR.MachinePrivateCFG
import EvoIR.MachinePrivateBodyCut

/-!
A private suffix starts at actual instruction cursors. The only guard premise
contains checks already executed on both machines. A source block, a guarded
continuation and a call continuation use this same open boundary. Whole CFG
closure discharges the boundary; a construction limit never limits execution.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def PrivateSuffixDiagram {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (type : Source.IntType) (table : List (PrivateCutpoint sf tf))
    (sb : IR.Block) (spc : Nat) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (bank : PrivateBank Γ) (prior : List (Guard Γ)) : Prop :=
  ∀ σ values incoming owner frame memory regs logicalInputs sourceStack targetStack observationType,
    bank.Models σ values incoming owner frame memory regs → guards σ prior = .ok () →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (fun s t => PrivateBoundary table incoming owner sourceStack targetStack s t ∨
        PrivateReturnBoundary type incoming owner sourceStack targetStack s t)
      (privateState σ values sf sb.name spc owner frame bank.source sourceStack)
      (privateMachineState tf tb tpc regs owner frame logicalInputs targetStack memory)

theorem private_suffix_control {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (type : Source.IntType) (table : List (PrivateCutpoint sf tf))
    (sb : IR.Block) (tb : EvoIR.Machine.Block) (bank : PrivateBank Γ)
    (prior : List (Guard Γ))
    (finish : PrivateControlDiagram source target sf tf type table sb tb bank prior) :
    PrivateSuffixDiagram source target sf tf type table sb sb.instructions.length
      tb tb.insts.length bank prior := finish

theorem private_suffix_cut {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (type : Source.IntType) (table : List (PrivateCutpoint sf tf))
    (sb : IR.Block) (spc : Nat) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (bank : PrivateBank Γ) (prior : List (Guard Γ))
    (cut : PrivateBodyCut source target sf sb spc tf tb tpc bank)
    (finish : PrivateSuffixDiagram source target sf tf type table
      sb (spc + cut.result.sourceCount) tb (tpc + cut.result.targetCount)
      cut.result.next prior) :
    PrivateSuffixDiagram source target sf tf type table sb spc tb tpc bank prior := by
  intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
    observationType models passed
  obtain ⟨newValues, nextMemory, nextRegs, sourcePath, targetPath, nextModels⟩ :=
    cut.sound σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
      models (by simp only [cut.noChecks, guards])
  exact (finish σ newValues incoming owner frame nextMemory nextRegs logicalInputs
    sourceStack targetStack observationType nextModels passed).prepend sourcePath.reaches targetPath.reaches

/-- A store may use only checks that have already executed. This is a paired
effect step, not a new guard: its proof obligations are not re-executed or
silently appended to the observable diagnostic trace. -/
def privateStoreSpan {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (bank : PrivateBank Γ) (prior : List (Guard Γ)) :
    Option {result : PrivateBodySpanResult Γ // result.checks = prior ∧
      PrivateBodySpanDiagram source target sf sb spc tf tb tpc bank result} := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          let inst : {inst // sb.instructions[spc]? = some inst} ←
            match found : sb.instructions[spc]? with
            | none => none
            | some inst => some ⟨inst, found⟩
          let native : {inst // tb.insts[tpc]? = some inst} ←
            match found : tb.insts[tpc]? with
            | none => none
            | some inst => some ⟨inst, found⟩
          if store : inst.val.op = .store then
            let row ← privateStore prior bank inst.val native.val
            if sameChecks : row.checks = prior then
              let fetched : PrivateFetchedRow sb tb spc tpc bank := {
                source := some inst.val, target := some native.val, row,
                sourceFetched := by intro actual same; cases Option.some.inj same; exact inst.property,
                sourceNoncall := by
                  intro actual same name
                  cases Option.some.inj same
                  simp [store],
                targetFetched := by intro actual same; cases Option.some.inj same; exact native.property }
              return ⟨⟨row.checks, row.next, 1, 1⟩, sameChecks,
                fetched.sound source target sf sb spc tf tb tpc bank
                  sourceFunction sourceBlock targetFunction targetBlock⟩
            else none
          else none
        else none
      else none
    else none
  else none

theorem private_suffix_span_under {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (type : Source.IntType) (table : List (PrivateCutpoint sf tf))
    (sb : IR.Block) (spc : Nat) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (bank : PrivateBank Γ) (prior : List (Guard Γ))
    (span : {result : PrivateBodySpanResult Γ // result.checks = prior ∧
      PrivateBodySpanDiagram source target sf sb spc tf tb tpc bank result})
    (finish : PrivateSuffixDiagram source target sf tf type table
      sb (spc + span.val.sourceCount) tb (tpc + span.val.targetCount) span.val.next prior) :
    PrivateSuffixDiagram source target sf tf type table sb spc tb tpc bank prior := by
  intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
    observationType models passed
  obtain ⟨newValues, nextMemory, nextRegs, sourcePath, targetPath, nextModels⟩ :=
    span.property.2 σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
      models (by simpa only [span.property.1] using passed)
  exact (finish σ newValues incoming owner frame nextMemory nextRegs logicalInputs
    sourceStack targetStack observationType nextModels passed).prepend sourcePath.reaches targetPath.reaches

theorem private_suffix_block (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (point : PrivateCutpoint sf tf)
    (suffix : PrivateSuffixDiagram source target sf tf type table
      point.source.block 0 point.targetBlock 0 point.bank []) :
    PrivateBlockDiagram source target sf tf type table point := by
  intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
    observationType models
  exact suffix σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
    observationType models rfl

end EvoIR.Core.Symbolic.MachineSelection
