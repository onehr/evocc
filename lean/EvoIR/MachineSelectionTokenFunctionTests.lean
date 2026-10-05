import EvoIR.MachineSelectionTokenFunction

namespace EvoIR.Core.Symbolic.MachineSelection.TokenFunctionTests

private def parameters : List Source.Parameter := [⟨0, ⟨.uint, false⟩, .none⟩]
private def effects : IR.Effects := ⟨false, false, false, false, false, false, false, false⟩

private def source : IR.Program := ⟨[
  ⟨"countdown", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, effects, [
    ⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], [], .branch ⟨"head", [0, 1, 2]⟩⟩,
    ⟨"head", [⟨10, .bits 32⟩, ⟨11, .memory⟩, ⟨12, .world⟩], [
      ⟨[⟨13, .bits 32⟩], .constant 32 0, []⟩,
      ⟨[⟨14, .bits 1⟩], .scalar (.cmp .ne 32), [10, 13]⟩],
      .conditional 14 ⟨"body", [10, 11, 12]⟩ ⟨"exit", [10, 11, 12]⟩⟩,
    ⟨"body", [⟨20, .bits 32⟩, ⟨21, .memory⟩, ⟨22, .world⟩], [
      ⟨[⟨23, .bits 32⟩], .constant 32 1, []⟩,
      ⟨[⟨24, .bits 32⟩], .scalar (.bin .sub 32), [20, 23]⟩], .branch ⟨"head", [24, 21, 22]⟩⟩,
    ⟨"exit", [⟨30, .bits 32⟩, ⟨31, .memory⟩, ⟨32, .world⟩], [], .return [30, 31, 32]⟩]⟩]⟩

private def target (operation : EvoIR.Machine.BinOp := .sub)
    (condition : EvoIR.Machine.Cond := .ne) : EvoIR.Machine.Program := ⟨[
  ⟨"countdown", [
    ⟨0, [⟨0, .gpr32⟩], [], .b 1 [⟨0, .gpr32⟩]⟩,
    ⟨1, [⟨10, .gpr32⟩], [.movImm ⟨13, .gpr32⟩ 0, .cmp ⟨14, .flags⟩ ⟨10, .gpr32⟩ ⟨13, .gpr32⟩ .w],
      .bCond ⟨14, .flags⟩ condition 2 [⟨10, .gpr32⟩] 3 [⟨10, .gpr32⟩]⟩,
    ⟨2, [⟨20, .gpr32⟩], [.movImm ⟨23, .gpr32⟩ 1,
      .binary operation ⟨24, .gpr32⟩ ⟨20, .gpr32⟩ ⟨23, .gpr32⟩ .w], .b 1 [⟨24, .gpr32⟩]⟩,
    ⟨3, [⟨30, .gpr32⟩], [], .ret [⟨30, .gpr32⟩]⟩]⟩]⟩

private def points : List (String × Nat) := [("entry", 0), ("head", 1), ("body", 2), ("exit", 3)]

private def admitted (machine : EvoIR.Machine.Program) (proposal : List (String × Nat) := points) : Bool :=
  (tokenFunctionPair source machine "countdown" parameters .uint proposal).isSome

def tokenFunctionSelfTest : IO UInt32 := do
  let controls := [
    ("complete token-carrying countdown", admitted (target)),
    ("changed arithmetic refuses", !admitted (target .add)),
    ("changed condition refuses", !admitted (target .sub .eq)),
    ("empty table refuses", !admitted (target) []),
    ("missing exit refuses", !admitted (target) (points.take 3)),
    ("missing entry refuses", !admitted (target) (points.drop 1)),
    ("wrong entry arity refuses", (tokenFunctionPair source (target) "countdown" [] .uint points).isNone),
    ("scalar-only wrapper does not erase source tokens",
      (scalarFunctionPair source (target) "countdown" parameters .uint points).isNone)]
  let mut failures : UInt32 := 0
  for (name, passed) in controls do
    if !passed then
      IO.eprintln s!"token function control failed: {name}"
      failures := failures+1
  if failures == 0 then IO.println s!"{controls.length} token function controls pass"
  return failures

/-- No trip count occurs in this proposition or in the checked body. The
actual source effect tokens and machine entry arguments remain distinct. -/
theorem countdown_with_tokens_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = parameters.map (fun p => p.type.type))
    (accepted : (tokenFunctionPair source (target) "countdown" parameters .uint points).isSome = true) :
    ∃ sourceEntry targetEntry,
      IR.initial source "countdown" (values.map sourceValueIR ++ [.memory ⟨1, []⟩, .world]) = .ok sourceEntry ∧
      EvoIR.Machine.initial (target) "countdown" (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines source (target) .uint sourceEntry targetEntry := by
  cases result : tokenFunctionPair source (target) "countdown" parameters .uint points with
  | none => simp [result] at accepted
  | some proof => exact proof.checked values typed

end EvoIR.Core.Symbolic.MachineSelection.TokenFunctionTests
