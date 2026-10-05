import EvoIR.MachineSelectionPair

namespace EvoIR.Core.Symbolic.MachineSelection

private def parameters : List Source.Parameter :=
  [⟨0, ⟨.uint, false⟩, .none⟩, ⟨1, ⟨.uint, false⟩, .none⟩]

private def source : IR.Program := ⟨[
  ⟨"sum", [.bits 32, .bits 32], [.bits 32], false,
    ⟨false, false, false, false, false, false, false, false⟩, [
      ⟨"entry", [⟨0, .bits 32⟩, ⟨1, .bits 32⟩],
        [⟨[⟨2, .bits 32⟩], .scalar (.bin .add 32), [0, 1]⟩], .return [2]⟩]⟩]⟩

private def machine (op : EvoIR.Machine.BinOp := .add)
    (returned : Nat := 2) : EvoIR.Machine.Program := ⟨[
  ⟨"sum", [⟨0, [⟨0, .gpr32⟩, ⟨1, .gpr32⟩],
    [.binary op ⟨2, .gpr32⟩ ⟨0, .gpr32⟩ ⟨1, .gpr32⟩ .w], .ret [⟨returned, .gpr32⟩]⟩]⟩]⟩

private def admitted (p : EvoIR.Machine.Program) : Bool :=
  (straightLinePair source p "sum" parameters .uint).isSome

example : admitted (machine .add) = true := by decide
example : admitted (machine .sub) = false := by decide
example : admitted (machine .add 0) = false := by decide
example : admitted (machine .add 77) = false := by decide
example : (straightLinePair source (machine .add) "missing" parameters .uint).isNone = true := by decide
example : (straightLinePair source (machine .add) "sum" [] .uint).isNone = true := by decide
example : (straightLinePair source (machine .add) "sum" parameters .ulongLong).isNone = true := by decide

/-- This uses the actual result proof, not merely the Boolean test above. -/
theorem admitted_sum_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      IR.initial source "sum" (values.map sourceValueIR) = .ok sourceEntry ∧
      EvoIR.Machine.initial (machine .add) "sum" (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines source (machine .add) .uint sourceEntry targetEntry := by
  have accepted : (straightLinePair source (machine .add) "sum" parameters .uint).isSome = true := by decide
  cases result : straightLinePair source (machine .add) "sum" parameters .uint with
  | none => simp [result] at accepted
  | some proof => exact proof.checked values typed

end EvoIR.Core.Symbolic.MachineSelection
