import EvoIR.MachineSelectionCaller

/-! Full-argument actual call controls. These retain the actual token
signature and use unknown arguments/return values in the proved endpoint. -/
namespace EvoIR.Core.Symbolic.MachineSelection.CallerTests

private def effects : IR.Effects := ⟨false, false, false, false, false, false, false, false⟩
private def r (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr32⟩

private def source : IR.Program := ⟨[
  ⟨"sum", [.bits 32, .bits 32, .memory, .world], [.bits 32, .memory, .world], true, effects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .bits 32⟩, ⟨2, .memory⟩, ⟨3, .world⟩],
      [⟨[⟨4, .bits 32⟩], .scalar (.bin .add 32), [0, 1]⟩], .return [4, 2, 3]⟩]⟩,
  ⟨"caller", [.bits 32, .bits 32, .memory, .world], [.bits 32, .memory, .world], false, effects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .bits 32⟩, ⟨2, .memory⟩, ⟨3, .world⟩],
      [⟨[⟨4, .bits 32⟩, ⟨5, .memory⟩, ⟨6, .world⟩], .call "sum", [0, 1, 2, 3]⟩,
       ⟨[⟨7, .bits 32⟩], .scalar (.bin .sub 32), [4, 0]⟩], .return [7, 5, 6]⟩]⟩]⟩

private def target (op : EvoIR.Machine.BinOp := .sub) (swapped := false) (returned : Nat := 7) :
    EvoIR.Machine.Program := ⟨[
  ⟨"sum", [⟨0, [r 0, r 1], [.binary .add (r 4) (r 0) (r 1) .w], .ret [r 4]⟩]⟩,
  ⟨"caller", [⟨0, [r 0, r 1],
    [.bl "sum" (if swapped then [r 1, r 0] else [r 0, r 1]) [r 4],
      .binary op (r 7) (r 4) (r 0) .w], .ret [r returned]⟩]⟩]⟩

private def parameters : List Source.Parameter :=
  [⟨0, ⟨.uint, false⟩, .none⟩, ⟨1, ⟨.uint, false⟩, .none⟩]

private def pair (machine : EvoIR.Machine.Program) (includeCallee := true) :
    Option (TokenSelectionPair source machine "caller" parameters .uint) := do
  let callee ← tokenFunction source machine "sum" .uint [("entry", 0)]
  let caller ← tokenCallerFunction source machine "caller" .uint [("entry", 0)]
    (if includeCallee then [⟨"sum", .uint, callee⟩] else [])
  tokenFunctionPairUsing source machine "caller" parameters .uint caller

example : (pair (target .sub)).isSome = true := by decide
example : (pair (target .add)).isSome = false := by decide
example : (pair (target .sub true)).isSome = false := by decide
example : (pair (target .sub false 0)).isSome = false := by decide
example : (pair (target .sub) false).isSome = false := by decide

theorem caller_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      IR.initial source "caller" (values.map sourceValueIR ++ [.memory ⟨1, []⟩, .world]) = .ok sourceEntry ∧
      EvoIR.Machine.initial (target .sub) "caller" (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines source (target .sub) .uint sourceEntry targetEntry := by
  have accepted : (pair (target .sub)).isSome = true := by decide
  cases checked : pair (target .sub) with
  | none => simp [checked] at accepted
  | some proof => exact proof.checked values typed

end EvoIR.Core.Symbolic.MachineSelection.CallerTests
