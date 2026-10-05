import EvoIR.MachinePrivateSuffix
import EvoIR.MachineSelectionCallResume
import EvoIR.Core.PrivateCallBindings

/-! A real callee return extends the source token bank with its full result
vector and updates the machine's actual result register. The two returned
memories remain separate. Saved caller pointers preserve their unbounded
identities; only symbolic scalar history is renamed for the fresh result. -/
namespace EvoIR.Core.Symbolic.MachineSelection

private theorem private_words_for_return (σ : Subst inputWidths) (value : Source.Value)
    (bank : Bank inputWidths) (regs : EvoIR.Machine.RegisterFile) (models : bank.Models σ regs) :
    bank.forReturn.Models (returnSubst σ value) regs := by
  intro id slot found
  have actual : (bankDenote (returnSubst σ value) bank.forReturn).lookup id =
      some (slot.denote (returnSubst σ value)) := by rw [bank_lookup, found]; rfl
  rw [bank_for_return, bank_lookup] at actual
  cases old : bank.lookup id with
  | none => simp [old] at actual
  | some oldSlot =>
    have equal : oldSlot.denote σ = slot.denote (returnSubst σ value) := by
      simpa only [old, Option.map_some, Option.some.injEq] using actual
    rw [← equal]
    exact models id oldSlot old

private theorem private_normalized_word (σ : Subst inputWidths)
    (reg : EvoIR.Machine.VReg) (word : WordExpr inputWidths) :
    (BankSlot.word (.normalize reg.class word)).denote σ =
      .value (EvoIR.Machine.normalize reg.class (.int (word.denote σ))) := by
  cases regClass : reg.class <;>
    simp [BankSlot.denote, WordExpr.denote, EvoIR.Machine.normalize,
      EvoIR.Machine.normalizeAt, EvoIR.Machine.Width.bits, regClass]

def PrivateResumePointers (before : PrivateBank inputWidths) (scope : Scope inputWidths)
    (reg : EvoIR.Machine.VReg) : Prop :=
  ∀ pointer ∈ before.pointers, ∃ key,
    before.source.scope.lookup pointer.source = some (.opaque key) ∧
    scope.lookup pointer.source = some (.opaque key) ∧
    key < before.source.tokens.length ∧ pointer.target.id ≠ reg.id

private def privateResumePointerList (before : PrivateBank inputWidths)
    (scope : Scope inputWidths) (reg : EvoIR.Machine.VReg) :
    (pointers : List PrivatePointer) → Option (PLift (∀ pointer ∈ pointers, ∃ key,
      before.source.scope.lookup pointer.source = some (.opaque key) ∧
      scope.lookup pointer.source = some (.opaque key) ∧
      key < before.source.tokens.length ∧ pointer.target.id ≠ reg.id))
  | [] => some ⟨by intro pointer member; cases member⟩
  | pointer :: pointers => do
    let key : {key // before.source.scope.lookup pointer.source = some (.opaque key)} ←
      match found : before.source.scope.lookup pointer.source with
      | some (.opaque key) => some ⟨key, found⟩
      | _ => none
    if same : scope.lookup pointer.source = some (.opaque key.val) then
      if bounded : key.val < before.source.tokens.length then
        if separate : pointer.target.id ≠ reg.id then do
          let tail ← privateResumePointerList before scope reg pointers
          return ⟨by
            intro selected member
            rcases List.mem_cons.mp member with rfl | member
            · exact ⟨key.val, key.property, same, bounded, separate⟩
            · exact tail.down selected member⟩
        else none
      else none
    else none

private def privateResumePointers (before : PrivateBank inputWidths)
    (scope : Scope inputWidths) (reg : EvoIR.Machine.VReg) :
    Option (PLift (PrivateResumePointers before scope reg)) :=
  privateResumePointerList before scope reg before.pointers

structure PrivateResumeBindings (before : PrivateBank inputWidths)
    (sourceResults : List IR.Binding) (targetResults : List EvoIR.Machine.VReg)
    (type : Source.IntType) where
  next : PrivateBank inputWidths
  targetArity : targetResults.length = 1
  sound : ∀ σ values incoming callerOwner calleeOwner targetBefore regs,
    before.Models σ values incoming callerOwner calleeOwner targetBefore regs →
    ∀ sourceBefore, before.source.memoryValue values = some sourceBefore →
    ∀ value : Source.Value, value.type = type → ∀ sourceAfter targetAfter afterFrame,
    sourceBefore.ClosedExtension sourceAfter → sourceAfter.WellAllocated →
    sourceAfter.BeforeFrame afterFrame → calleeOwner < afterFrame →
    (∀ object ∈ sourceAfter.objects, object ∉ sourceBefore.objects → calleeOwner ≤ object.owner) →
    EvoIR.Machine.PrivateMemory.LiveRelated sourceAfter targetAfter →
    ∃ nextRegs,
      IR.appendResults (before.source.scope.denote σ (privateToken values)) sourceResults
          (privateCallValues true value sourceAfter) =
        .ok (next.source.scope.denote (returnSubst σ value)
          (privateToken (values ++ privateCallValues true value sourceAfter))) ∧
      EvoIR.Machine.bind regs targetResults [EvoIR.Machine.sourceArgument value] = .ok nextRegs ∧
      next.Models (returnSubst σ value) (values ++ privateCallValues true value sourceAfter)
        incoming callerOwner afterFrame targetAfter nextRegs

def privateResumeBindings (before : PrivateBank inputWidths) (sourceResults : List IR.Binding)
    (targetResults : List EvoIR.Machine.VReg) (type : Source.IntType) :
    Option (PrivateResumeBindings before sourceResults targetResults type) := do
  let scope : {scope // before.source.scope.forReturn.appendPrivate
      (before.source.tokens ++ privateCallOutputTypes true type) sourceResults
      (privateCallOutputSlots true before.source type) = some scope} ←
    match bound : before.source.scope.forReturn.appendPrivate
        (before.source.tokens ++ privateCallOutputTypes true type) sourceResults
        (privateCallOutputSlots true before.source type) with
    | none => none
    | some scope => some ⟨scope, bound⟩
  let config := privateCallConfig true before.source scope.val type
  if shaped : config.WellShaped = true then
    match resultShape : targetResults with
    | [reg] => do
      let saved ← privateResumePointers before scope.val reg
      let word : WordExpr inputWidths := .input (inputWord type 0).word
      let words := bankWrite before.words.forReturn reg (.word (.normalize reg.class word))
      let next : PrivateBank inputWidths := ⟨config, words, before.pointers⟩
      return {
        next, targetArity := by simp only [resultShape, List.length_cons, List.length_nil],
        sound := by
          intro σ values incoming callerOwner calleeOwner targetBefore regs models sourceBefore current
            value typed sourceAfter targetAfter afterFrame closed safe frame future newOwners related
          let nextRegs := EvoIR.Machine.write regs reg (EvoIR.Machine.sourceArgument value)
          have sourceBound := private_call_binding σ values before.source scope.val type value sourceAfter
            true sourceResults models.source.2.1 typed models.source.1 scope.property
          have sourceModels := private_call_realizes σ values incoming sourceBefore sourceAfter
            callerOwner calleeOwner afterFrame before.source scope.val true type value models.source
            current typed closed safe frame newOwners future shaped
          have fresh : word.denote (returnSubst σ value) = value.bits.setWidth 64 := by
            have returned := return_subst_fresh σ value
            rw [typed] at returned
            exact congrArg (fun output : Source.Value => output.bits.setWidth 64) returned
          have wordModels := bank_models_write (returnSubst σ value) before.words.forReturn regs
            (private_words_for_return σ value before.words regs models.words) reg
            (.word (.normalize reg.class word))
          rw [private_normalized_word, fresh] at wordModels
          refine ⟨nextRegs, sourceBound, ?_, ⟨sourceModels, wordModels, ?_, ?_⟩⟩
          · rw [resultShape, EvoIR.Machine.bind_exact regs [reg]
              [EvoIR.Machine.sourceArgument value] rfl (by rfl)]
            rfl
          · intro pointer member
            obtain ⟨id, oldSource, oldTarget⟩ := models.pointers pointer member
            obtain ⟨key, oldSlot, newSlot, bounded, separate⟩ := saved.down pointer member
            have count : values.length = before.source.tokens.length := private_values_length models.source.2.1
            have retained : privateToken (values ++ privateCallValues true value sourceAfter) key =
                privateToken values key := by
              apply private_token_old
              simpa only [count] using bounded
            have sourceRead : IR.lookup (scope.val.denote (returnSubst σ value)
                (privateToken (values ++ privateCallValues true value sourceAfter))) pointer.source =
                IR.lookup (before.source.scope.denote σ (privateToken values)) pointer.source := by
              simp [IR.lookup, lookup_denote, newSlot, oldSlot, Slot.denote, retained]
            refine ⟨id, sourceRead.trans oldSource, ?_⟩
            have unchanged := EvoIR.Machine.readSlot_writeSlot_other regs reg pointer.target
              (.value (EvoIR.Machine.normalize reg.class (EvoIR.Machine.sourceArgument value))) separate
            simpa only [nextRegs, EvoIR.Machine.write, EvoIR.Machine.readOperand, unchanged] using oldTarget
          · refine ⟨sourceAfter, ?_, related⟩
            change (match privateToken (values ++ privateCallValues true value sourceAfter)
                (before.source.tokens.length+1) with
              | .memory actual => some actual | _ => none) = some sourceAfter
            rw [← private_values_length models.source.2.1, private_token_new]
            rfl }
    | _ => none
  else none

end EvoIR.Core.Symbolic.MachineSelection
