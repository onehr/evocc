import EvoIR.MachineSelectionSourceSpan
import EvoIR.MachineSelectionNormal
import EvoIR.MachineSelectionConditional
import EvoIR.MachineCorrespondence
import EvoIR.Core.OpenCheckpoint

/-!
Check an actual scalar return boundary. The checker reads both terminators,
the source binding and the machine register. The equality proof concerns the
computed expressions, not sampled arguments. This is the top-level scalar
return leaf; suspended callers and returned memory tokens are separate leaves.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def ScalarReturnUnderDiagram {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : Bank Γ) (type : Source.IntType)
    (checks : List (Guard Γ)) : Prop :=
  ∀ σ ρ owner nextFrame incoming memory,
    guards σ checks = .ok () →
    EvoIR.Machine.SelectionRefines source target type
      (sourceState σ ρ sf sb sb.instructions.length scope owner nextFrame [])
      (machineState σ tf tb tb.insts.length bank owner nextFrame incoming [] memory)

/-- Returning from both actual machines is a closed semantic leaf. It can be
inserted into any productive checkpoint table without adding an open edge. -/
def scalarReturnUnder {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : Bank Γ) (type : Source.IntType)
    (checks : List (Guard Γ)) :
    Option (PLift (ScalarReturnUnderDiagram source target sf sb scope tf tb bank type checks)) := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          if supported : type.width = 32 ∨ type.width = 64 then
            if returns : sf.returns = [.bits type.width] then
              match sourceReturn : sb.terminator, targetReturn : tb.term with
              | .return [id], .ret [reg] => do
                let word : {word // scope.word id = some word} ←
                  match found : scope.word id with
                  | none => none
                  | some word => some ⟨word, found⟩
                if typed : word.val.1 = type.width then
                  let machine ← wordOperand bank reg
                  let equal ← checkWordUnder checks word.val machine.val
                  return ⟨by
                    intro σ ρ owner nextFrame incoming memory passed
                    let bits := denoteTerm σ word.val.2
                    let s := sourceState σ ρ sf sb sb.instructions.length scope owner nextFrame []
                    let t := machineState σ tf tb tb.insts.length bank owner nextFrame incoming [] memory
                    let ss : IR.State := {s with control := .halted (.ok [.bits word.val.1 bits])}
                    let tt : EvoIR.Machine.State :=
                      {t with control := .halted (.ok [.int (machine.val.denote σ)]), memory := memory.endFrame owner}
                    have sourceRead := word_lookup_sound σ ρ scope id word.val word.property
                    have sourceStep : IR.step source s = some ss := by
                      simp [s, ss, sourceState, IR.step, IR.execute, sourceFunction, sourceBlock,
                        sourceReturn, sourceRead, Word.denote, IR.Value.type, returns, typed,
                        bits, Bind.bind, Except.bind, Pure.pure, Except.pure]
                    have targetRead : EvoIR.Machine.read (bankDenote σ bank) reg = .ok (.int (machine.val.denote σ)) := by
                      have read := word_operand_value bank reg machine.val machine.property σ
                      cases actual : EvoIR.Machine.readSlot (bankDenote σ bank) reg with
                      | none =>
                        simp [EvoIR.Machine.operandValue, EvoIR.Machine.readOperand, actual,
                          Bind.bind, Except.bind] at read
                      | some slot =>
                        cases slot <;>
                          simpa [EvoIR.Machine.operandValue, EvoIR.Machine.readOperand,
                            EvoIR.Machine.read, actual, Bind.bind, Except.bind,
                            Pure.pure, Except.pure] using read
                    have executed := EvoIR.Machine.execute_ret_noCaller .ssa target t
                      ⟨tf.name, tb.id, tb.insts.length, bankDenote σ bank, owner, incoming⟩
                      tf tb [reg] [.int (machine.val.denote σ)] targetFunction targetBlock
                      (List.getElem?_eq_none (Nat.le_refl _)) rfl targetReturn
                      (by simp [targetRead, Bind.bind, Except.bind, Pure.pure, Except.pure]) rfl
                    have targetStep : EvoIR.Machine.step .ssa target t = some tt := by
                      change some (match EvoIR.Machine.execute .ssa target t
                          ⟨tf.name, tb.id, tb.insts.length, bankDenote σ bank, owner, incoming⟩ with
                        | .ok state => state
                        | .error fault => {t with control := .halted (.error fault)}) = some tt
                      rw [executed]
                      rfl
                    apply Transition.refines_of_terminals (.next sourceStep (.refl _))
                      (.next targetStep (.refl _)) rfl rfl
                      (o := Observation.returned type bits.toNat)
                    · simp [observeTarget, ss, typed, bits]
                    · have value := equal.down σ passed
                      have width : word.val.1 ≤ 64 := by omega
                      simp only [EvoIR.Machine.observe, tt, value]
                      have retained : ((bits.setWidth 64).setWidth type.width).toNat =
                          bits.toNat := by
                        rw [← typed]
                        simp [BitVec.setWidth_setWidth_of_le bits width]
                      have admitted : (type.width == 32 || type.width == 64) = true := by
                        rcases supported with h | h <;> simp [h]
                      simp only [admitted, ↓reduceIte]
                      change some (Observation.returned type
                        ((bits.setWidth 64).setWidth type.width).toNat) = _
                      rw [retained]⟩
                else none
              | _, _ => none
            else none
          else none
        else none
      else none
    else none
  else none

def ScalarReturnDiagram {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : Bank Γ) (type : Source.IntType) : Prop :=
  ∀ σ ρ owner nextFrame incoming memory,
    EvoIR.Machine.SelectionRefines source target type
      (sourceState σ ρ sf sb sb.instructions.length scope owner nextFrame [])
      (machineState σ tf tb tb.insts.length bank owner nextFrame incoming [] memory)

/-- Empty-trace compatibility for callers which need no guard premise. -/
def scalarReturn {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : Bank Γ) (type : Source.IntType) :
    Option (PLift (ScalarReturnDiagram source target sf sb scope tf tb bank type)) := do
  let checked ← scalarReturnUnder source target sf sb scope tf tb bank type []
  return ⟨by intro σ ρ owner nextFrame incoming memory
             exact checked.down σ ρ owner nextFrame incoming memory rfl⟩

end EvoIR.Core.Symbolic.MachineSelection
