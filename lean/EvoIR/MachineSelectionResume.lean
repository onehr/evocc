import EvoIR.MachineSelectionTokens
import EvoIR.MachineSelectionEdges
import EvoIR.Core.FreshReturn

/-!
Fresh unknown call results preserve all suspended caller inputs. The source
token environment is extended, not overwritten: old memory tokens remain
the exact values saved in the actual activation. Machine result registers
are rebound by the actual normalization and duplicate/arity checks.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

mutual
def WordExpr.renameInputs (name : Var → Var) : WordExpr inputWidths → WordExpr inputWidths
  | .input ⟨n, term⟩ => .input ⟨n, term.renameInputs name⟩
  | .literal bits => .literal bits
  | .binary op width lhs rhs => .binary op width (lhs.renameInputs name) (rhs.renameInputs name)
  | .wide op lhs rhs => .wide op (lhs.renameInputs name) (rhs.renameInputs name)
  | .extend op input => .extend op (input.renameInputs name)
  | .bfx signed width input lsb bits => .bfx signed width (input.renameInputs name) lsb bits
  | .cset flags cond => .cset (flags.renameInputs name) cond
  | .csel flags cond width yes no =>
    .csel (flags.renameInputs name) cond width (yes.renameInputs name) (no.renameInputs name)
  | .normalize regClass input => .normalize regClass (input.renameInputs name)

def FlagExpr.renameInputs (name : Var → Var) : FlagExpr inputWidths → FlagExpr inputWidths
  | .comparison width lhs rhs => .comparison width (lhs.renameInputs name) (rhs.renameInputs name)
  | .arithmetic op width lhs rhs => .arithmetic op width (lhs.renameInputs name) (rhs.renameInputs name)
  | .logical width input => .logical width (input.renameInputs name)
end

mutual
theorem WordExpr.rename_inputs_sound (name : Var → Var) (σ : Subst inputWidths)
    (word : WordExpr inputWidths) :
    (word.renameInputs name).denote σ = word.denote (fun v => σ (name v)) := by
  cases word with
  | input word => cases word; simp only [WordExpr.renameInputs, WordExpr.denote, Term.rename_inputs_sound]
  | literal => rfl
  | binary op width lhs rhs | wide op lhs rhs =>
    simp only [WordExpr.renameInputs, WordExpr.denote, WordExpr.rename_inputs_sound name σ lhs,
      WordExpr.rename_inputs_sound name σ rhs]
  | extend op input | bfx signed width input lsb bits | normalize regClass input =>
    simp only [WordExpr.renameInputs, WordExpr.denote, WordExpr.rename_inputs_sound name σ input]
  | cset flags cond => simp only [WordExpr.renameInputs, WordExpr.denote, FlagExpr.rename_inputs_sound name σ flags]
  | csel flags cond width yes no =>
    simp only [WordExpr.renameInputs, WordExpr.denote, FlagExpr.rename_inputs_sound name σ flags,
      WordExpr.rename_inputs_sound name σ yes, WordExpr.rename_inputs_sound name σ no]

theorem FlagExpr.rename_inputs_sound (name : Var → Var) (σ : Subst inputWidths)
    (flags : FlagExpr inputWidths) :
    (flags.renameInputs name).denote σ = flags.denote (fun v => σ (name v)) := by
  cases flags with
  | comparison width lhs rhs | arithmetic op width lhs rhs =>
    simp only [FlagExpr.renameInputs, FlagExpr.denote, WordExpr.rename_inputs_sound name σ lhs,
      WordExpr.rename_inputs_sound name σ rhs]
  | logical width input =>
    simp only [FlagExpr.renameInputs, FlagExpr.denote, WordExpr.rename_inputs_sound name σ input]
end

def Bank.forReturn (bank : Bank inputWidths) : Bank inputWidths :=
  bank.map fun (id, slot) => (id, match slot with
    | .word word => .word (word.renameInputs returnName)
    | .flags flags => .flags (flags.renameInputs returnName))

theorem bank_for_return (σ : Subst inputWidths) (value : Source.Value) (bank : Bank inputWidths) :
    bankDenote (returnSubst σ value) bank.forReturn = bankDenote σ bank := by
  have old : (fun v => returnSubst σ value (returnName v)) = σ := by
    funext v
    exact return_subst_old σ value v
  simp only [Bank.forReturn, bankDenote, List.map_map, Function.comp_def]
  apply List.map_congr_left
  rintro ⟨id, slot⟩ member
  cases slot <;> simp only [BankSlot.denote, WordExpr.rename_inputs_sound,
    FlagExpr.rename_inputs_sound, old] <;> rfl

def resumeSlots (tokenCount : Nat) (type : Source.IntType) : List (Slot inputWidths) :=
  [.word (inputWord type 0).word, .opaque tokenCount, .opaque (tokenCount+1)]

private theorem resume_slots_denote (σ : Subst inputWidths) (value : Source.Value)
    (type : Source.IntType) (typed : value.type = type) (values : List IR.Value)
    (memory : IR.Memory) :
    (resumeSlots values.length type).map
        (Slot.denote (returnSubst σ value) (privateToken (values ++ [.memory memory, .world]))) =
      [sourceValueIR value, .memory memory, .world] := by
  have fresh : (inputWord type 0).denote (returnSubst σ value) = value := by
    simpa only [typed] using return_subst_fresh σ value
  have word := congrArg sourceValueIR fresh
  have memoryToken : privateToken (values ++ [.memory memory, .world]) values.length = .memory memory := by
    simpa only [Nat.add_zero, privateToken, List.getElem?_cons_zero, Option.getD_some] using
      private_token_new values [.memory memory, .world] 0
  have worldToken : privateToken (values ++ [.memory memory, .world]) (values.length+1) = .world := by
    simpa only [privateToken, List.getElem?_cons_succ, List.getElem?_cons_zero, Option.getD_some] using
      private_token_new values [.memory memory, .world] 1
  simpa only [resumeSlots, List.map_cons, List.map_nil, Slot.denote,
    memoryToken, worldToken, Word.denote, CWord.word, CWord.denote, sourceValueIR] using
    congrArg (fun returned => [returned, IR.Value.memory memory, IR.Value.world]) word

structure ResumeBindings (types : List IR.Ty) (oldScope : Scope inputWidths) (oldBank : Bank inputWidths)
    (sourceResults : List IR.Binding) (targetResults : List EvoIR.Machine.VReg)
    (type : Source.IntType) where
  scope : Scope inputWidths
  bank : Bank inputWidths
  targetArity : targetResults.length = 1
  sound : ∀ σ value, value.type = type → ∀ values : List IR.Value,
    values.map IR.Value.type = types → ∀ memory,
      IR.appendResults (oldScope.denote σ (privateToken values)) sourceResults
          [sourceValueIR value, .memory memory, .world] =
        .ok (scope.denote (returnSubst σ value)
          (privateToken (values ++ [.memory memory, .world]))) ∧
      EvoIR.Machine.bind (bankDenote σ oldBank) targetResults [.int (value.bits.setWidth 64)] =
        .ok (bankDenote (returnSubst σ value) bank)

def resumeBindings (types : List IR.Ty) (scope : Scope inputWidths) (bank : Bank inputWidths)
    (sourceResults : List IR.Binding) (targetResults : List EvoIR.Machine.VReg)
    (type : Source.IntType) : Option (ResumeBindings types scope bank sourceResults targetResults type) := do
  if bounded : scope.inBank types.length = true then
    let nextScope : {next // scope.forReturn.appendPrivate (types ++ [.memory, .world])
        sourceResults (resumeSlots types.length type) = some next} ←
      match bound : scope.forReturn.appendPrivate (types ++ [.memory, .world]) sourceResults
          (resumeSlots types.length type) with
      | none => none
      | some next => some ⟨next, bound⟩
    let freshWord := WordExpr.input (inputWord type 0).word
    if arity : targetResults.length = 1 then
      if unique : (targetResults.map EvoIR.Machine.VReg.id).eraseDups.length = targetResults.length then
        let nextBank := bindWordValues bank.forReturn (targetResults.zip [freshWord])
        return {
          scope := nextScope.val, bank := nextBank
          targetArity := arity
          sound := by
            intro σ value typed values valueTypes memory
            have length : values.length = types.length := by
              have same := congrArg List.length valueTypes
              simpa only [List.length_map] using same
            have sourceBinding := append_private_sound (returnSubst σ value)
              (values ++ [.memory memory, .world]) scope.forReturn nextScope.val
              (types ++ [.memory, .world]) sourceResults (resumeSlots types.length type)
              (by simp only [List.map_append, valueTypes, List.map_cons, List.map_nil, IR.Value.type])
              nextScope.property
            have oldSource : scope.forReturn.denote (returnSubst σ value)
                (privateToken (values ++ [.memory memory, .world])) = scope.denote σ (privateToken values) := by
              rw [scope_for_return]
              exact scope_private_extension σ scope values [.memory memory, .world] (by simpa only [length] using bounded)
            have slots := resume_slots_denote σ value type typed values memory
            rw [length] at slots
            rw [oldSource, slots] at sourceBinding
            have fresh : (inputWord type 0).denote (returnSubst σ value) = value := by
              simpa only [typed] using return_subst_fresh σ value
            have returned : freshWord.denote (returnSubst σ value) = value.bits.setWidth 64 := by
              exact congrArg (fun result : Source.Value => result.bits.setWidth 64) fresh
            refine ⟨sourceBinding, ?_⟩
            rw [EvoIR.Machine.bind_exact _ targetResults _ (by simpa using arity) unique]
            rw [bind_word_values_denote, bank_for_return]
            apply congrArg Except.ok
            apply congrArg (EvoIR.Machine.bindValues (bankDenote σ bank))
            have resultValues : [EvoIR.Machine.Value.int (value.bits.setWidth 64)] =
                [freshWord].map (fun word => EvoIR.Machine.Value.int (word.denote (returnSubst σ value))) := by
              simp only [List.map_cons, List.map_nil, returned]
            rw [resultValues, List.zip_map_right]
            rfl }
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
