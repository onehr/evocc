import EvoIR.MachinePrivateBodyRows
import EvoIR.MachinePrivateSourceStep
import EvoIR.MachineStep
import EvoIR.Core.CheckpointSimulation

/-! Actual paired private-body prefixes. The finite recursion budget counts
the supplied instruction streams, not program execution. Every consumed row
is fetched at its real PC; unsupported calls/control remain explicit suffix
boundaries. Whole-block users must check the returned endpoints. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def privateRowCount {α : Type} : Option α → Nat
  | none => 0
  | some _ => 1

structure PrivateFetchedRow {Γ : Var → Nat} (sb : IR.Block)
    (tb : EvoIR.Machine.Block) (spc tpc : Nat) (before : PrivateBank Γ) where
  source : Option IR.Instruction
  target : Option EvoIR.Machine.Instruction
  row : CheckedPrivateRow before source target
  sourceFetched : ∀ inst, source = some inst → sb.instructions[spc]? = some inst
  sourceNoncall : ∀ inst, source = some inst → ∀ name, inst.op ≠ .call name
  targetFetched : ∀ inst, target = some inst → tb.insts[tpc]? = some inst

private def privateSourceNoncall (inst : IR.Instruction) : Bool :=
  match inst.op with
  | .call _ => false
  | _ => true

private theorem private_source_noncall (inst : IR.Instruction)
    (checked : privateSourceNoncall inst = true) : ∀ name, inst.op ≠ .call name := by
  intro name called
  simp [privateSourceNoncall, called] at checked

private def privateSourceOnly {Γ : Var → Nat} (before : PrivateBank Γ)
    (inst : IR.Instruction) : Option (CheckedPrivateRow before (some inst) none) :=
  (privateSourceScalar before inst).orElse fun _ =>
    (privateErasedAccess before inst).orElse fun _ => privateErasedLifetime before inst

private def privatePaired {Γ : Var → Nat} (before : PrivateBank Γ)
    (inst : IR.Instruction) (native : EvoIR.Machine.Instruction) :
    Option (CheckedPrivateRow before (some inst) (some native)) :=
  (privateAllocate before inst native).orElse fun _ =>
    (privateLoad before inst native).orElse fun _ => privateStore [] before inst native

private def fetchedSource {Γ : Var → Nat} (sb : IR.Block) (tb : EvoIR.Machine.Block)
    (spc tpc : Nat) (before : PrivateBank Γ) (inst : IR.Instruction)
    (found : sb.instructions[spc]? = some inst) (noncall : ∀ name, inst.op ≠ .call name)
    (row : CheckedPrivateRow before (some inst) none) :
    PrivateFetchedRow sb tb spc tpc before :=
  ⟨some inst, none, row,
    by intro actual same; cases Option.some.inj same; exact found,
    by intro actual same; cases Option.some.inj same; exact noncall,
    by intro actual impossible; cases impossible⟩

private def fetchedTarget {Γ : Var → Nat} (sb : IR.Block) (tb : EvoIR.Machine.Block)
    (spc tpc : Nat) (before : PrivateBank Γ) (inst : EvoIR.Machine.Instruction)
    (found : tb.insts[tpc]? = some inst) (row : CheckedPrivateRow before none (some inst)) :
    PrivateFetchedRow sb tb spc tpc before :=
  ⟨none, some inst, row,
    (by intro actual impossible; cases impossible),
    (by intro actual impossible; cases impossible),
    by intro actual same; cases Option.some.inj same; exact found⟩

private def fetchedPair {Γ : Var → Nat} (sb : IR.Block) (tb : EvoIR.Machine.Block)
    (spc tpc : Nat) (before : PrivateBank Γ) (inst : IR.Instruction)
    (native : EvoIR.Machine.Instruction) (sourceFound : sb.instructions[spc]? = some inst)
    (targetFound : tb.insts[tpc]? = some native) (noncall : ∀ name, inst.op ≠ .call name)
    (row : CheckedPrivateRow before (some inst) (some native)) :
    PrivateFetchedRow sb tb spc tpc before :=
  ⟨some inst, some native, row,
    by intro actual same; cases Option.some.inj same; exact sourceFound,
    by intro actual same; cases Option.some.inj same; exact noncall,
    by intro actual same; cases Option.some.inj same; exact targetFound⟩

def privateNextRow {Γ : Var → Nat} (sb : IR.Block) (tb : EvoIR.Machine.Block)
    (spc tpc : Nat) (before : PrivateBank Γ) :
    Option (PrivateFetchedRow sb tb spc tpc before) :=
  let targetOnly := fun _ =>
    match targetFound : tb.insts[tpc]? with
    | none => none
    | some native => (privateTargetScalar before native).map
        (fetchedTarget sb tb spc tpc before native targetFound)
  match sourceFound : sb.instructions[spc]? with
  | none => targetOnly ()
  | some inst =>
    if noncall : privateSourceNoncall inst = true then
      match privateSourceOnly before inst with
      | some row => some (fetchedSource sb tb spc tpc before inst sourceFound
          (private_source_noncall inst noncall) row)
      | none =>
        match targetFound : tb.insts[tpc]? with
        | none => none
        | some native =>
          match privatePaired before inst native with
          | some row => some (fetchedPair sb tb spc tpc before inst native sourceFound
              targetFound (private_source_noncall inst noncall) row)
          | none => targetOnly ()
    else targetOnly ()

structure PrivateBodySpanResult (Γ : Var → Nat) where
  checks : List (Guard Γ)
  next : PrivateBank Γ
  sourceCount : Nat
  targetCount : Nat

def PrivateBodySpanDiagram {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) (result : PrivateBodySpanResult Γ) : Prop :=
  ∀ σ values incoming owner frame memory regs logicalInputs sourceStack targetStack,
    before.Models σ values incoming owner frame memory regs →
    guards σ result.checks = .ok () →
    ∃ nextValues nextMemory nextRegs,
      Transition.Path (IR.step source) result.sourceCount
        (privateState σ values sf sb.name spc owner frame before.source sourceStack)
        (privateState σ nextValues sf sb.name (spc+result.sourceCount) owner frame
          result.next.source sourceStack) ∧
      Transition.Path (EvoIR.Machine.step .ssa target) result.targetCount
        (privateMachineState tf tb tpc regs owner frame logicalInputs targetStack memory)
        (privateMachineState tf tb (tpc+result.targetCount) nextRegs owner frame
          logicalInputs targetStack nextMemory) ∧
      result.next.Models σ nextValues incoming owner frame nextMemory nextRegs

private theorem private_target_notcall (owner : Nat) (inst : EvoIR.Machine.Instruction)
    (regs : EvoIR.Machine.RegisterFile) (memory : IR.Memory)
    (nextRegs : EvoIR.Machine.RegisterFile) (nextMemory : IR.Memory)
    (evaluated : EvoIR.Machine.evalInstruction owner inst regs memory = .ok (nextRegs, nextMemory)) :
    ∀ name args results, inst ≠ .bl name args results := by
  intro name args results called
  subst inst
  simp [EvoIR.Machine.evalInstruction, EvoIR.Machine.evalInstructionOperands,
    Functor.map, Except.map] at evaluated

private theorem private_source_row_path {Γ : Var → Nat} (source : IR.Program)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (before after : PrivateBank Γ)
    (selected : Option IR.Instruction) (σ : Subst Γ) (values nextValues : List IR.Value)
    (owner frame : Nat) (sourceStack : List IR.ReturnFrame)
    (sourceFunction : source.functions.find? (·.name == sf.name) = some sf)
    (sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb)
    (sourceFetched : ∀ inst, selected = some inst → sb.instructions[spc]? = some inst)
    (sourceNoncall : ∀ inst, selected = some inst → ∀ name, inst.op ≠ .call name)
    (sourceRun : PrivateSourceInstruction owner before σ values selected =
      .ok (after.source.scope.denote σ (privateToken nextValues))) :
    Transition.Path (IR.step source) (privateRowCount selected)
      (privateState σ values sf sb.name spc owner frame before.source sourceStack)
      (privateState σ nextValues sf sb.name (spc+privateRowCount selected) owner frame after.source sourceStack) := by
  cases selected with
  | none =>
    have same := Except.ok.inj sourceRun
    simp only [privateRowCount, Nat.add_zero]
    have states : privateState σ values sf sb.name spc owner frame before.source sourceStack =
        privateState σ nextValues sf sb.name spc owner frame after.source sourceStack := by
      simp only [privateState, PrivateSourceInstruction] at same ⊢
      rw [same]
    rw [← states]
    exact .refl _
  | some inst =>
    have step := private_source_instruction_step source sf sb spc owner frame before.source
      after.source σ values nextValues sourceStack inst sourceFunction sourceBlock
      (sourceFetched inst rfl) (sourceNoncall inst rfl) sourceRun
    simpa only [privateRowCount] using Transition.Path.next step (.refl _)

private theorem private_target_row_path (target : EvoIR.Machine.Program)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc owner frame : Nat)
    (selected : Option EvoIR.Machine.Instruction) (memory nextMemory : IR.Memory)
    (regs nextRegs : EvoIR.Machine.RegisterFile) (logicalInputs : List EvoIR.Machine.Value)
    (targetStack : List EvoIR.Machine.ReturnFrame)
    (targetFunction : target.functions.find? (·.name == tf.name) = some tf)
    (targetBlock : tf.blocks.find? (·.id == tb.id) = some tb)
    (targetFetched : ∀ inst, selected = some inst → tb.insts[tpc]? = some inst)
    (targetRun : PrivateTargetInstruction owner regs memory selected = .ok (nextRegs, nextMemory)) :
    Transition.Path (EvoIR.Machine.step .ssa target) (privateRowCount selected)
      (privateMachineState tf tb tpc regs owner frame logicalInputs targetStack memory)
      (privateMachineState tf tb (tpc+privateRowCount selected) nextRegs owner frame logicalInputs targetStack nextMemory) := by
  cases selected with
  | none =>
    have same : regs = nextRegs ∧ memory = nextMemory := by
      simpa only [PrivateTargetInstruction, Except.ok.injEq, Prod.mk.injEq] using targetRun
    simp only [privateRowCount, Nat.add_zero, ← same.1, ← same.2]
    exact .refl _
  | some inst =>
    have actual : EvoIR.Machine.evalInstruction owner inst regs memory = .ok (nextRegs, nextMemory) := targetRun
    have step := EvoIR.Machine.step_evalInstruction .ssa target
      (privateMachineState tf tb tpc regs owner frame logicalInputs targetStack memory)
      ⟨tf.name, tb.id, tpc, regs, owner, logicalInputs⟩ tf tb inst nextRegs nextMemory rfl
      targetFunction targetBlock (targetFetched inst rfl)
      (private_target_notcall owner inst regs memory nextRegs nextMemory actual) actual
    simpa only [privateRowCount, privateMachineState] using Transition.Path.next step (.refl _)

theorem PrivateFetchedRow.sound {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) (fetched : PrivateFetchedRow sb tb spc tpc before)
    (sourceFunction : source.functions.find? (·.name == sf.name) = some sf)
    (sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb)
    (targetFunction : target.functions.find? (·.name == tf.name) = some tf)
    (targetBlock : tf.blocks.find? (·.id == tb.id) = some tb) :
    PrivateBodySpanDiagram source target sf sb spc tf tb tpc before
      ⟨fetched.row.checks, fetched.row.next, privateRowCount fetched.source,
        privateRowCount fetched.target⟩ := by
  intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack models passed
  obtain ⟨nextValues, nextMemory, nextRegs, sourceRun, targetRun, nextModels⟩ :=
    fetched.row.sound σ values incoming owner frame memory regs models passed
  exact ⟨nextValues, nextMemory, nextRegs,
    private_source_row_path source sf sb spc before fetched.row.next fetched.source σ values nextValues
      owner frame sourceStack sourceFunction sourceBlock fetched.sourceFetched fetched.sourceNoncall sourceRun,
    private_target_row_path target tf tb tpc owner frame fetched.target memory nextMemory regs nextRegs
      logicalInputs targetStack targetFunction targetBlock fetched.targetFetched targetRun, nextModels⟩

private theorem private_path_append {α : Type} {step : α → Option α}
    {firstCount lastCount : Nat} {start middle last : α}
    (first : Transition.Path step firstCount start middle)
    (rest : Transition.Path step lastCount middle last) :
    Transition.Path step (firstCount+lastCount) start last := by
  induction first with
  | refl => simpa only [Nat.zero_add] using rest
  | next edge _ ih => simpa only [Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
      using Transition.Path.next edge (ih rest)

private theorem private_guards_pass {Γ : Var → Nat} (σ : Subst Γ)
    (first rest : List (Guard Γ)) (passed : guards σ (first++rest) = .ok ()) :
    guards σ first = .ok () ∧ guards σ rest = .ok () := by
  rw [guards_append] at passed
  cases checked : guards σ first with
  | error fault => simp [checked, Bind.bind, Except.bind] at passed
  | ok unit =>
    cases unit
    exact ⟨rfl, by simpa only [checked, Bind.bind, Except.bind] using passed⟩

private def privateEmptySpan {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) :
    {result : PrivateBodySpanResult Γ // PrivateBodySpanDiagram source target sf sb spc tf tb tpc before result} :=
  ⟨⟨[], before, 0, 0⟩, by
    intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack models passed
    exact ⟨values, memory, regs, .refl _, .refl _, models⟩⟩

private def privatePrependRow {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) (fetched : PrivateFetchedRow sb tb spc tpc before)
    (first : PrivateBodySpanDiagram source target sf sb spc tf tb tpc before
      ⟨fetched.row.checks, fetched.row.next, privateRowCount fetched.source, privateRowCount fetched.target⟩)
    (rest : {result : PrivateBodySpanResult Γ //
      PrivateBodySpanDiagram source target sf sb (spc+privateRowCount fetched.source)
        tf tb (tpc+privateRowCount fetched.target) fetched.row.next result}) :
    {result : PrivateBodySpanResult Γ // PrivateBodySpanDiagram source target sf sb spc tf tb tpc before result} :=
  ⟨⟨fetched.row.checks++rest.val.checks, rest.val.next,
    privateRowCount fetched.source+rest.val.sourceCount, privateRowCount fetched.target+rest.val.targetCount⟩, by
    intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack models passed
    obtain ⟨firstPassed, restPassed⟩ := private_guards_pass σ fetched.row.checks rest.val.checks passed
    obtain ⟨middleValues, middleMemory, middleRegs, sourceFirst, targetFirst, middleModels⟩ :=
      first σ values incoming owner frame memory regs logicalInputs sourceStack targetStack models firstPassed
    obtain ⟨nextValues, nextMemory, nextRegs, sourceRest, targetRest, nextModels⟩ :=
      rest.property σ middleValues incoming owner frame middleMemory middleRegs logicalInputs
        sourceStack targetStack middleModels restPassed
    refine ⟨nextValues, nextMemory, nextRegs, ?_, ?_, nextModels⟩
    · simpa only [Nat.add_assoc] using private_path_append sourceFirst sourceRest
    · simpa only [Nat.add_assoc] using private_path_append targetFirst targetRest⟩

private def privateBodySpanFuel {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block)
    (stopBeforeChecks : Bool)
    (sourceFunction : source.functions.find? (·.name == sf.name) = some sf)
    (sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb)
    (targetFunction : target.functions.find? (·.name == tf.name) = some tf)
    (targetBlock : tf.blocks.find? (·.id == tb.id) = some tb) :
    (fuel spc tpc : Nat) → (before : PrivateBank Γ) →
    {result : PrivateBodySpanResult Γ // PrivateBodySpanDiagram source target sf sb spc tf tb tpc before result}
  | 0, spc, tpc, before => privateEmptySpan source target sf sb spc tf tb tpc before
  | fuel+1, spc, tpc, before =>
    match privateNextRow sb tb spc tpc before with
    | none => privateEmptySpan source target sf sb spc tf tb tpc before
    | some fetched =>
      if stopBeforeChecks && !fetched.row.checks.isEmpty then
        privateEmptySpan source target sf sb spc tf tb tpc before
      else
        let rest := privateBodySpanFuel source target sf sb tf tb stopBeforeChecks
          sourceFunction sourceBlock targetFunction targetBlock
          fuel (spc+privateRowCount fetched.source) (tpc+privateRowCount fetched.target) fetched.row.next
        privatePrependRow source target sf sb spc tf tb tpc before fetched
          (fetched.sound source target sf sb spc tf tb tpc before sourceFunction sourceBlock targetFunction targetBlock) rest

def privateBodySpanStopping {Γ : Var → Nat} (stopBeforeChecks : Bool) (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) :
    Option {result : PrivateBodySpanResult Γ // PrivateBodySpanDiagram source target sf sb spc tf tb tpc before result} := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          if within : spc ≤ sb.instructions.length ∧ tpc ≤ tb.insts.length then
            return privateBodySpanFuel source target sf sb tf tb stopBeforeChecks
              sourceFunction sourceBlock targetFunction targetBlock
              (sb.instructions.length+tb.insts.length+1) spc tpc before
          else none
        else none
      else none
    else none
  else none

/-- The unrestricted component retains its original success-under-checks
contract. Whole admission must discharge those checks, or use the stopping
entry to handle the first guarded row and its actual failure separately. -/
def privateBodySpan {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) :
    Option {result : PrivateBodySpanResult Γ // PrivateBodySpanDiagram source target sf sb spc tf tb tpc before result} :=
  privateBodySpanStopping false source target sf sb spc tf tb tpc before

end EvoIR.Core.Symbolic.MachineSelection
