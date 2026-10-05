import EvoIR.MachineSelectionFunction

/-! Productive actual-CFG controls. Acceptance returns a refinement for every
canonical input valuation, not a successful run at a selected fuel bound. -/
namespace EvoIR.Core.Symbolic.MachineSelection.Tests

private def effects : IR.Effects := ⟨false, false, false, false, false, false, false, false⟩
private def r (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr32⟩
private def flags : EvoIR.Machine.VReg := ⟨10, .flags⟩

private def sourceFunction : IR.Function :=
  ⟨"countdown", [.bits 32], [.bits 32], false, effects, [
    ⟨"entry", [⟨0, .bits 32⟩], [], .branch ⟨"loop", [0]⟩⟩,
    ⟨"loop", [⟨1, .bits 32⟩], [
      ⟨[⟨2, .bits 32⟩], .constant 32 0, []⟩,
      ⟨[⟨3, .bits 1⟩], .scalar (.cmp .ne 32), [1, 2]⟩],
      .conditional 3 ⟨"step", [1]⟩ ⟨"exit", [1]⟩⟩,
    ⟨"step", [⟨4, .bits 32⟩], [
      ⟨[⟨5, .bits 32⟩], .constant 32 1, []⟩,
      ⟨[⟨6, .bits 32⟩], .scalar (.bin .sub 32), [4, 5]⟩], .branch ⟨"loop", [6]⟩⟩,
    ⟨"exit", [⟨7, .bits 32⟩], [], .return [7]⟩]⟩

private def targetFunction (swapped := false) (op : EvoIR.Machine.BinOp := .sub) : EvoIR.Machine.Function :=
  ⟨"countdown", [
    ⟨0, [r 0], [], .b 1 [r 0]⟩,
    ⟨1, [r 1], [.cmpImm flags (r 1) 0 .w],
      .bCond flags .ne (if swapped then 3 else 2) [r 1] (if swapped then 2 else 3) [r 1]⟩,
    ⟨2, [r 4], [.binaryImm op (r 6) (r 4) 1 .w], .b 1 [r 6]⟩,
    ⟨3, [r 7], [], .ret [r 7]⟩]⟩

private def table (tf : EvoIR.Machine.Function) : Option (List (ScalarCutpoint sourceFunction tf)) :=
  [("entry", 0), ("loop", 1), ("step", 2), ("exit", 3)].mapM fun (label, id) =>
    scalarCutpoint sourceFunction tf label id

private def admitted (tf : EvoIR.Machine.Function) : Bool :=
  match table tf with
  | none => false
  | some points => (scalarCFG ⟨[sourceFunction]⟩ ⟨[tf]⟩ sourceFunction tf .uint points).isSome

example : admitted (targetFunction false .sub) = true := by decide
example : admitted (targetFunction true .sub) = false := by decide
example : admitted (targetFunction false .add) = false := by decide

private def endlessSource : IR.Function :=
  ⟨"endless", [], [.bits 32], false, effects,
    [⟨"entry", [], [], .branch ⟨"entry", []⟩⟩]⟩
private def endlessTarget : EvoIR.Machine.Function :=
  ⟨"endless", [⟨0, [], [], .b 0 []⟩]⟩

private def endlessAccepted : Bool := (do
  let point ← scalarCutpoint endlessSource endlessTarget "entry" 0
  let _ ← scalarCFG ⟨[endlessSource]⟩ ⟨[endlessTarget]⟩ endlessSource endlessTarget .uint [point]
  return ()).isSome

example : endlessAccepted = true := by decide

private def booleanSource : IR.Function :=
  ⟨"boolean_edge", [], [.bits 32], false, effects, [
    ⟨"loop", [⟨0, .bits 1⟩], [], .branch ⟨"loop", [0]⟩⟩]⟩
private def booleanTarget : EvoIR.Machine.Function :=
  ⟨"boolean_edge", [⟨0, [r 0], [], .b 0 [r 0]⟩]⟩

private def booleanAccepted : Bool := (do
  let point ← scalarCutpoint booleanSource booleanTarget "loop" 0
  let _ ← scalarCFG ⟨[booleanSource]⟩ ⟨[booleanTarget]⟩ booleanSource booleanTarget .uint [point]
  return ()).isSome

example : booleanAccepted = true := by decide

private def parameters : List Source.Parameter := [⟨0, ⟨.uint, false⟩, .none⟩]
private def proposal : List (String × Nat) :=
  [("entry", 0), ("loop", 1), ("step", 2), ("exit", 3)]

private def functionAccepted (tf : EvoIR.Machine.Function)
    (points : List (String × Nat) := proposal) : Bool :=
  (scalarFunctionPair ⟨[sourceFunction]⟩ ⟨[tf]⟩ "countdown" parameters .uint points).isSome

example : functionAccepted (targetFunction false .sub) = true := by decide
example : functionAccepted (targetFunction true .sub) = false := by decide
example : functionAccepted (targetFunction false .add) = false := by decide
example : functionAccepted (targetFunction false .sub) [] = false := by decide
example : functionAccepted (targetFunction false .sub)
    [("loop", 1), ("step", 2), ("exit", 3)] = false := by decide
example : functionAccepted (targetFunction false .sub)
    [("entry", 0), ("loop", 1), ("step", 2)] = false := by decide

/-- The checked cyclic graph gives an entry-level proof for every argument;
the proposition has neither an iteration cap nor a concrete input sample. -/
theorem countdown_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      IR.initial ⟨[sourceFunction]⟩ "countdown" (values.map sourceValueIR) = .ok sourceEntry ∧
      EvoIR.Machine.initial ⟨[targetFunction false .sub]⟩ "countdown"
        (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines ⟨[sourceFunction]⟩ ⟨[targetFunction false .sub]⟩
        .uint sourceEntry targetEntry := by
  have accepted : functionAccepted (targetFunction false .sub) = true := by decide
  cases result : scalarFunctionPair ⟨[sourceFunction]⟩ ⟨[targetFunction false .sub]⟩
      "countdown" parameters .uint proposal with
  | none => simp [functionAccepted, result] at accepted
  | some proof => exact proof.checked values typed

example : (scalarFunctionPair ⟨[endlessSource]⟩ ⟨[endlessTarget]⟩ "endless" [] .uint
    [("entry", 0)]).isSome = true := by decide

end EvoIR.Core.Symbolic.MachineSelection.Tests
