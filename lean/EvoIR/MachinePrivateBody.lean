import EvoIR.MachinePrivateEffects
import EvoIR.MachinePrivateBank
import EvoIR.MachineSelectionConditional
import EvoIR.Core.PrivateChecker
import EvoIR.Core.PrivateBoundary

/-! Checked private bodies operate on the two actual instruction streams.
The source retains its complete token bank and private-cell assertions. The
machine separately models scalar/flag slots and Nat-identity pointer slots.
Current source and machine memory snapshots are related, never identified. -/
namespace EvoIR.Core.Symbolic.MachineSelection

structure PrivatePointer where
  source : Nat
  target : EvoIR.Machine.VReg
  deriving DecidableEq, Repr

def PrivatePointer.Models (pointers : List PrivatePointer) (source : IR.Bindings)
    (target : EvoIR.Machine.RegisterFile) : Prop :=
  ∀ pointer ∈ pointers, ∃ id,
    IR.lookup source pointer.source = .ok (.pointer id 0) ∧
    EvoIR.Machine.readOperand target pointer.target = .ok (.value (.pointer id 0))

structure PrivateBank (Γ : Var → Nat) where
  source : PrivateConfig Γ
  words : Bank Γ
  pointers : List PrivatePointer
  deriving DecidableEq

structure PrivateBank.Models {Γ : Var → Nat} (bank : PrivateBank Γ) (σ : Subst Γ)
    (values : List IR.Value) (incoming : IR.Memory) (owner frame : Nat)
    (memory : IR.Memory) (regs : EvoIR.Machine.RegisterFile) : Prop where
  source : bank.source.Realizes σ values incoming owner frame
  words : bank.words.Models σ regs
  pointers : PrivatePointer.Models bank.pointers (bank.source.scope.denote σ (privateToken values)) regs
  memory : ∃ actual, bank.source.memoryValue values = some actual ∧
    EvoIR.Machine.PrivateMemory.LiveRelated actual memory

def PrivateSourceInstruction {Γ : Var → Nat} (owner : Nat) (before : PrivateBank Γ)
    (σ : Subst Γ) (values : List IR.Value) : Option IR.Instruction → Except Source.Fault IR.Bindings
  | none => .ok (before.source.scope.denote σ (privateToken values))
  | some inst => concreteInstruction owner (before.source.scope.denote σ (privateToken values)) inst

def PrivateTargetInstruction (owner : Nat) (regs : EvoIR.Machine.RegisterFile)
    (memory : IR.Memory) : Option EvoIR.Machine.Instruction →
    Except Source.Fault (EvoIR.Machine.RegisterFile × IR.Memory)
  | none => .ok (regs, memory)
  | some inst => EvoIR.Machine.evalInstruction owner inst regs memory

def PrivateRowDiagram {Γ : Var → Nat} (before after : PrivateBank Γ)
    (source : Option IR.Instruction) (target : Option EvoIR.Machine.Instruction)
    (checks : List (Guard Γ)) : Prop :=
  ∀ σ values incoming owner frame memory regs, before.Models σ values incoming owner frame memory regs →
    guards σ checks = .ok () →
    ∃ nextValues nextMemory nextRegs,
      PrivateSourceInstruction owner before σ values source =
        .ok (after.source.scope.denote σ (privateToken nextValues)) ∧
      PrivateTargetInstruction owner regs memory target = .ok (nextRegs, nextMemory) ∧
      after.Models σ nextValues incoming owner frame nextMemory nextRegs

structure CheckedPrivateRow {Γ : Var → Nat} (before : PrivateBank Γ)
    (source : Option IR.Instruction) (target : Option EvoIR.Machine.Instruction) where
  checks : List (Guard Γ)
  next : PrivateBank Γ
  sound : PrivateRowDiagram before next source target checks

def opaqueOutputs {Γ : Var → Nat} (count : Nat) : List (Slot Γ) :=
  (List.range count).map Slot.opaque

structure PrivateOutputs {Γ : Var → Nat} (before : PrivateBank Γ)
    (after : PrivateConfig Γ) (inst : IR.Instruction) (types : List IR.Ty) (memoryPosition : Nat) where
  fresh : Scope Γ
  binding : bindSlots (privateTokenType types) inst.results (opaqueOutputs types.length) = some fresh
  pointerClear : ∀ pointer ∈ before.pointers, fresh.lookup pointer.source = none
  memoryName : Nat
  freshMemory : fresh.lookup memoryName = some (.opaque memoryPosition)
  afterMemory : after.scope.lookup memoryName = some (.opaque after.memory)

def privateOutputs {Γ : Var → Nat} (before : PrivateBank Γ) (after : PrivateConfig Γ)
    (inst : IR.Instruction) (types : List IR.Ty) (memoryPosition : Nat) :
    Option (PrivateOutputs before after inst types memoryPosition) := do
  let fresh : {scope // bindSlots (privateTokenType types) inst.results
      (opaqueOutputs types.length) = some scope} ←
    match bound : bindSlots (privateTokenType types) inst.results (opaqueOutputs types.length) with
    | none => none
    | some scope => some ⟨scope, bound⟩
  if clear : before.pointers.all (fun pointer => decide (fresh.val.lookup pointer.source = none)) then
    let memoryResult ← inst.results[memoryPosition]?
    if freshMemory : fresh.val.lookup memoryResult.id = some (.opaque memoryPosition) then
      if afterMemory : after.scope.lookup memoryResult.id = some (.opaque after.memory) then
        return ⟨fresh.val, fresh.property,
          by intro pointer member; exact of_decide_eq_true (List.all_eq_true.mp clear pointer member),
          memoryResult.id, freshMemory, afterMemory⟩
      else none
    else none
  else none

theorem lookup_append_some {α : Type} (first rest : List (Nat × α)) (id : Nat) (value : α)
    (found : first.lookup id = some value) : (first ++ rest).lookup id = some value := by
  induction first with
  | nil => simp at found
  | cons entry tail ih =>
    cases entry with
    | mk key slot =>
      by_cases same : id = key
      · subst key
        simpa only [List.cons_append, List.lookup_cons, beq_self_eq_true, ↓reduceIte] using found
      · simpa only [List.cons_append, List.lookup_cons, beq_eq_false_iff_ne.mpr same,
          Bool.false_eq_true, ↓reduceIte] using ih (by
            simpa only [List.lookup_cons, beq_eq_false_iff_ne.mpr same,
              Bool.false_eq_true, ↓reduceIte] using found)

theorem lookup_append_none {α : Type} (first rest : List (Nat × α)) (id : Nat)
    (missing : first.lookup id = none) : (first ++ rest).lookup id = rest.lookup id := by
  induction first with
  | nil => rfl
  | cons entry tail ih =>
    cases entry with
    | mk key slot =>
      by_cases same : id = key
      · subst key
        simp only [List.lookup_cons, beq_self_eq_true] at missing
        contradiction
      · simpa only [List.cons_append, List.lookup_cons, beq_eq_false_iff_ne.mpr same,
          Bool.false_eq_true, ↓reduceIte] using ih (by
            simpa only [List.lookup_cons, beq_eq_false_iff_ne.mpr same,
              Bool.false_eq_true, ↓reduceIte] using missing)

theorem append_effect_values (before after fresh : IR.Bindings)
    (results : List IR.Binding) (outputs : List IR.Value)
    (bound : IR.bind results outputs = .ok fresh)
    (appended : IR.appendResults before results outputs = .ok after) : after = fresh ++ before := by
  simp only [IR.appendResults, bound, Bind.bind, Except.bind] at appended
  split at appended
  · simp [MonadExcept.throw, throwThe, MonadExceptOf.throw] at appended
  · exact (Except.ok.inj appended).symm

theorem PrivateOutputs.concrete {Γ : Var → Nat} (before : PrivateBank Γ)
    (after : PrivateConfig Γ) (inst : IR.Instruction) (types : List IR.Ty) (memoryPosition : Nat)
    (receipt : PrivateOutputs before after inst types memoryPosition)
    (σ : Subst Γ) (values outputs nextValues : List IR.Value)
    (typed : outputs.map IR.Value.type = types)
    (denoted : (opaqueOutputs types.length).map (Slot.denote σ (privateToken outputs)) = outputs)
    (appended : IR.appendResults (before.source.scope.denote σ (privateToken values)) inst.results outputs =
      .ok (after.scope.denote σ (privateToken nextValues))) :
    after.scope.denote σ (privateToken nextValues) =
      receipt.fresh.denote σ (privateToken outputs) ++ before.source.scope.denote σ (privateToken values) := by
  have bound := bind_slots_sound σ (privateToken outputs) (privateTokenType types)
    (private_token_typed outputs types typed) inst.results (opaqueOutputs types.length) receipt.fresh receipt.binding
  rw [denoted] at bound
  exact append_effect_values _ _ _ inst.results outputs bound appended

theorem PrivateOutputs.memory_eq {Γ : Var → Nat} (before : PrivateBank Γ)
    (after : PrivateConfig Γ) (inst : IR.Instruction) (types : List IR.Ty) (memoryPosition : Nat)
    (receipt : PrivateOutputs before after inst types memoryPosition)
    (σ : Subst Γ) (values outputs nextValues : List IR.Value) (memory : IR.Memory)
    (typed : outputs.map IR.Value.type = types)
    (denoted : (opaqueOutputs types.length).map (Slot.denote σ (privateToken outputs)) = outputs)
    (outputMemory : privateToken outputs memoryPosition = .memory memory)
    (appended : IR.appendResults (before.source.scope.denote σ (privateToken values)) inst.results outputs =
      .ok (after.scope.denote σ (privateToken nextValues))) : after.memoryValue nextValues = some memory := by
  have concrete := PrivateOutputs.concrete before after inst types memoryPosition receipt
    σ values outputs nextValues typed denoted appended
  have fresh : (receipt.fresh.denote σ (privateToken outputs)).lookup receipt.memoryName = some (.memory memory) := by
    simp only [lookup_denote, receipt.freshMemory, Option.map_some, Slot.denote, outputMemory]
  have found := lookup_append_some _ (before.source.scope.denote σ (privateToken values))
    receipt.memoryName (IR.Value.memory memory) fresh
  rw [← concrete, lookup_denote, receipt.afterMemory] at found
  have same : privateToken nextValues after.memory = .memory memory := by
    simpa only [Option.map_some, Slot.denote, Option.some.injEq] using found
  simp only [PrivateConfig.memoryValue, same]

theorem PrivateOutputs.pointers {Γ : Var → Nat} (before : PrivateBank Γ)
    (after : PrivateConfig Γ) (inst : IR.Instruction) (types : List IR.Ty) (memoryPosition : Nat)
    (receipt : PrivateOutputs before after inst types memoryPosition)
    (σ : Subst Γ) (values outputs nextValues : List IR.Value) (regs : EvoIR.Machine.RegisterFile)
    (typed : outputs.map IR.Value.type = types)
    (denoted : (opaqueOutputs types.length).map (Slot.denote σ (privateToken outputs)) = outputs)
    (appended : IR.appendResults (before.source.scope.denote σ (privateToken values)) inst.results outputs =
      .ok (after.scope.denote σ (privateToken nextValues)))
    (models : PrivatePointer.Models before.pointers (before.source.scope.denote σ (privateToken values)) regs) :
    PrivatePointer.Models before.pointers (after.scope.denote σ (privateToken nextValues)) regs := by
  have concrete := PrivateOutputs.concrete before after inst types memoryPosition receipt
    σ values outputs nextValues typed denoted appended
  intro pointer member
  obtain ⟨id, source, target⟩ := models pointer member
  refine ⟨id, ?_, target⟩
  rw [IR.lookup, concrete, lookup_append_none]
  · exact source
  · simp only [lookup_denote, receipt.pointerClear pointer member, Option.map_none]

theorem pointer_source_rebind {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (before after : Scope Γ) (pointers : List PrivatePointer) (regs : EvoIR.Machine.RegisterFile)
    (models : PrivatePointer.Models pointers (before.denote σ (privateToken values)) regs)
    (same : ∀ pointer ∈ pointers, after.lookup pointer.source = before.lookup pointer.source) :
    PrivatePointer.Models pointers (after.denote σ (privateToken values)) regs := by
  intro pointer member
  obtain ⟨id, source, target⟩ := models pointer member
  refine ⟨id, ?_, target⟩
  simpa only [IR.lookup, lookup_denote, same pointer member] using source

theorem pointer_target_preserved (pointers : List PrivatePointer) (source : IR.Bindings)
    (before after : EvoIR.Machine.RegisterFile) (models : PrivatePointer.Models pointers source before)
    (same : ∀ pointer ∈ pointers,
      EvoIR.Machine.readOperand after pointer.target = EvoIR.Machine.readOperand before pointer.target) :
    PrivatePointer.Models pointers source after := by
  intro pointer member
  obtain ⟨id, original, native⟩ := models pointer member
  exact ⟨id, original, (same pointer member).trans native⟩

theorem eval_pointer_preserved (owner : Nat) (inst : EvoIR.Machine.Instruction)
    (regs next : EvoIR.Machine.RegisterFile) (memory after : IR.Memory)
    (evaluated : EvoIR.Machine.evalInstruction owner inst regs memory = .ok (next, after))
    (reg : EvoIR.Machine.VReg) (outside : ∀ dst ∈ inst.defs, reg.id ≠ dst.id) :
    EvoIR.Machine.readOperand next reg = EvoIR.Machine.readOperand regs reg := by
  cases operands : EvoIR.Machine.evalInstructionOperands owner inst
      (EvoIR.Machine.readOperand regs) memory with
  | error fault => simp [EvoIR.Machine.evalInstruction, operands, Functor.map, Except.map] at evaluated
  | ok result =>
    rcases result with ⟨writes, finalMemory⟩
    have footprint := EvoIR.Machine.evalInstructionOperands_resultRegs owner inst
      (EvoIR.Machine.readOperand regs) memory writes finalMemory operands
    have same : next = EvoIR.Machine.applyWrites regs writes := by
      have outputs : EvoIR.Machine.applyWrites regs writes = next ∧ finalMemory = after := by
        simpa only [EvoIR.Machine.evalInstruction, operands, Bind.bind, Except.bind,
          Pure.pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] using evaluated
      exact outputs.1.symm
    have nonalias : ∀ write ∈ writes, reg.id ≠ write.1.id := by
      intro write member
      apply outside write.1
      rw [← footprint]
      exact List.mem_map.mpr ⟨write, member, rfl⟩
    simp only [same, EvoIR.Machine.readOperand,
      EvoIR.Machine.applyWrites_readSlot_other regs writes reg nonalias]

/-- Source-only scalar rows inspect the actual IR instruction. Target scalar
execution is independent and may occur before the next private effect. -/
def privateSourceScalar {Γ : Var → Nat} (before : PrivateBank Γ) (inst : IR.Instruction) :
    Option (CheckedPrivateRow before (some inst) none) := do
  let computed : {result // instruction before.source.scope inst = some result} ←
    match found : instruction before.source.scope inst with
    | none => none
    | some result => some ⟨result, found⟩
  let nextSource := {before.source with scope := computed.val.2}
  if shaped : nextSource.WellShaped = true then
    if pointerScopes : before.pointers.all (fun pointer =>
        decide (nextSource.scope.lookup pointer.source = before.source.scope.lookup pointer.source)) then
      return ⟨computed.val.1, {before with source := nextSource}, by
        intro σ values incoming owner frame memory regs models passed
        have source := instruction_sound σ (privateToken values) owner before.source.scope
          computed.val.2 inst computed.val.1 computed.property
        rw [passed] at source
        have sourceModels : nextSource.Realizes σ values incoming owner frame := by
          rcases models.source with ⟨_, typed, world, incomingSafe, actual⟩
          exact ⟨shaped, typed, world, incomingSafe, actual⟩
        refine ⟨values, memory, regs, source, rfl, sourceModels, models.words, ?_, models.memory⟩
        exact pointer_source_rebind σ values before.source.scope nextSource.scope before.pointers regs
          models.pointers (by
            intro pointer member
            exact of_decide_eq_true (List.all_eq_true.mp pointerScopes pointer member))⟩
    else none
  else none

/-- Native scalar rows execute against the real mutable bank, including
unmodeled private pointers. Actual result footprints prevent pointer clobbers. -/
def privateTargetScalar {Γ : Var → Nat} (before : PrivateBank Γ) (inst : EvoIR.Machine.Instruction) :
    Option (CheckedPrivateRow before none (some inst)) := do
  let native ← modeledInstruction before.words inst
  if separate : before.pointers.all (fun pointer =>
      inst.defs.all (fun dst => decide (pointer.target.id ≠ dst.id))) then
    return ⟨[], {before with words := native.val}, by
      intro σ values incoming owner frame memory regs models passed
      obtain ⟨nextRegs, evaluated, nextWords⟩ := native.property σ regs owner memory models.words
      refine ⟨values, memory, nextRegs, rfl, evaluated,
        models.source, nextWords, ?_, models.memory⟩
      apply pointer_target_preserved before.pointers _ regs nextRegs models.pointers
      intro pointer member
      apply eval_pointer_preserved owner inst regs nextRegs memory memory evaluated pointer.target
      intro dst defined
      exact of_decide_eq_true (List.all_eq_true.mp
        (List.all_eq_true.mp separate pointer member) dst defined)⟩
  else none

def privateMachineState (function : EvoIR.Machine.Function) (block : EvoIR.Machine.Block)
    (pc : Nat) (regs : EvoIR.Machine.RegisterFile) (owner nextFrame : Nat)
    (incoming : List EvoIR.Machine.Value) (stack : List EvoIR.Machine.ReturnFrame)
    (memory : IR.Memory) : EvoIR.Machine.State :=
  ⟨.running ⟨function.name, block.id, pc, regs, owner, incoming⟩, stack, nextFrame, memory⟩

end EvoIR.Core.Symbolic.MachineSelection
