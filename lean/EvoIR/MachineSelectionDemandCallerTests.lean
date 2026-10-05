import EvoIR.MachineSelectionDemandCaller

/-! Actual omitted-private-argument controls retain both logical arguments
and the original memory/world signature. The success theorem uses all typed
input vectors; the second source argument is not assumed to be zero. -/
namespace EvoIR.Core.Symbolic.MachineSelection.DemandCallerTests

private def effects : IR.Effects := ⟨false, false, false, false, false, false, false, false⟩
private def r (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr32⟩

private def source : IR.Program := ⟨[
  ⟨"first", [.bits 32, .bits 32, .memory, .world], [.bits 32, .memory, .world], true, effects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .bits 32⟩, ⟨2, .memory⟩, ⟨3, .world⟩], [], .return [0, 2, 3]⟩]⟩,
  ⟨"caller", [.bits 32, .bits 32, .memory, .world], [.bits 32, .memory, .world], false, effects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .bits 32⟩, ⟨2, .memory⟩, ⟨3, .world⟩],
      [⟨[⟨4, .bits 32⟩, ⟨5, .memory⟩, ⟨6, .world⟩], .call "first", [0, 1, 2, 3]⟩,
       ⟨[⟨7, .bits 32⟩], .scalar (.bin .sub 32), [4, 0]⟩], .return [7, 5, 6]⟩]⟩]⟩

private def target (wrongUsed := false) (firstReturn : Nat := 0) : EvoIR.Machine.Program := ⟨[
  ⟨"first", [⟨0, [r 0, r 1], [], .ret [r firstReturn]⟩]⟩,
  ⟨"caller", [⟨0, [r 0, r 1],
    [.movImm (r 2) 0, .bl "first" (if wrongUsed then [r 2, r 1] else [r 0, r 2]) [r 4],
      .binary .sub (r 7) (r 4) (r 0) .w], .ret [r 7]⟩]⟩]⟩

private def plan (omitUsed := false) : EvoIR.Machine.Demand.Plan :=
  [⟨"first", if omitUsed then [] else [0]⟩, ⟨"caller", [0, 4, 7]⟩]

private def parameters : List Source.Parameter :=
  [⟨0, ⟨.uint, false⟩, .none⟩, ⟨1, ⟨.uint, false⟩, .none⟩]

private def pair (machine : EvoIR.Machine.Program) (proposal : EvoIR.Machine.Demand.Plan) :
    Option (TokenSelectionPair source machine "caller" parameters .uint) := do
  let first ← tokenFunction source machine "first" .uint [("entry", 0)]
  let caller ← tokenDemandCallerFunction source machine "caller" .uint [("entry", 0)]
    [⟨"first", .uint, first⟩] proposal
  tokenFunctionPairUsing source machine "caller" parameters .uint caller

example : (pair (target false 0) (plan false)).isSome = true := by decide
example : (pair (target true 0) (plan false)).isSome = false := by decide
example : (pair (target false 0) (plan true)).isSome = false := by decide
example : (pair (target false 1) (plan false)).isSome = false := by decide

example : (do
  let first ← tokenFunction source (target false 0) "first" .uint [("entry", 0)]
  tokenCallerFunction source (target false 0) "caller" .uint [("entry", 0)] [⟨"first", .uint, first⟩]).isSome = false := by decide

theorem zero_substituted_call_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      IR.initial source "caller" (values.map sourceValueIR ++ [.memory ⟨1, []⟩, .world]) = .ok sourceEntry ∧
      EvoIR.Machine.initial (target false 0) "caller" (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines source (target false 0) .uint sourceEntry targetEntry := by
  have accepted : (pair (target false 0) (plan false)).isSome = true := by decide
  cases checked : pair (target false 0) (plan false) with
  | none => simp [checked] at accepted
  | some proof => exact proof.checked values typed

end EvoIR.Core.Symbolic.MachineSelection.DemandCallerTests
