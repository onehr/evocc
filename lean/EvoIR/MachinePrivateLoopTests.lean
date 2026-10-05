import EvoIR.MachinePrivateLoop

/-! Whole private-loop admission and mutations. Runtime controls exercise
the proof-valued checker. Its conditional theorem is kernel checked and starts
at both actual entries for every typed input; no execution fuel is supplied. -/
namespace EvoIR.Core.Symbolic.MachineSelection.PrivateLoopTests

private def effects : IR.Effects := ⟨true, true, false, false, false, false, true, false⟩
private def parameters : List Source.Parameter :=
  [⟨0, ⟨.uint, false⟩, .none⟩, ⟨1, ⟨.uint, false⟩, .none⟩]
private def w (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr32⟩
private def x (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr64⟩
private def flags : EvoIR.Machine.VReg := ⟨6, .flags⟩

private def sourceFunction : IR.Function :=
  ⟨"private_loop", [.bits 32, .bits 32, .memory, .world], [.bits 32, .memory, .world],
    false, effects, [
    ⟨"entry", [⟨0, .bits 32⟩, ⟨1, .bits 32⟩, ⟨2, .memory⟩, ⟨3, .world⟩], [
      ⟨[⟨4, .pointer⟩, ⟨5, .memory⟩, ⟨6, .world⟩], .allocate 3 4 4, [2, 3]⟩,
      ⟨[⟨7, .memory⟩], .access 4 4, [5, 4]⟩,
      ⟨[⟨8, .memory⟩, ⟨9, .world⟩], .store, [7, 6, 4, 1]⟩],
      .branch ⟨"b1", [0, 4, 8, 9]⟩⟩,
    ⟨"b1", [⟨10, .bits 32⟩, ⟨11, .pointer⟩, ⟨12, .memory⟩, ⟨13, .world⟩], [
      ⟨[⟨14, .bits 32⟩], .constant 32 0, []⟩,
      ⟨[⟨15, .bits 1⟩], .scalar (.cmp .ne 32), [10, 14]⟩],
      .conditional 15 ⟨"b2", [10, 11, 12, 13]⟩ ⟨"b3", [11, 12, 13]⟩⟩,
    ⟨"b2", [⟨16, .bits 32⟩, ⟨17, .pointer⟩, ⟨18, .memory⟩, ⟨19, .world⟩], [
      ⟨[⟨20, .memory⟩], .access 4 4, [18, 17]⟩,
      ⟨[⟨21, .bits 32⟩, ⟨22, .memory⟩, ⟨23, .world⟩], .load 32, [20, 19, 17]⟩,
      ⟨[⟨24, .bits 32⟩], .constant 32 1, []⟩,
      ⟨[⟨25, .bits 32⟩], .scalar (.bin .add 32), [21, 24]⟩,
      ⟨[⟨26, .bits 32⟩], .scalar (.bin .sub 32), [16, 24]⟩,
      ⟨[⟨27, .memory⟩], .access 4 4, [22, 17]⟩,
      ⟨[⟨28, .memory⟩, ⟨29, .world⟩], .store, [27, 23, 17, 25]⟩],
      .branch ⟨"b1", [26, 17, 28, 29]⟩⟩,
    ⟨"b3", [⟨30, .pointer⟩, ⟨31, .memory⟩, ⟨32, .world⟩], [
      ⟨[⟨33, .memory⟩], .access 4 4, [31, 30]⟩,
      ⟨[⟨34, .bits 32⟩, ⟨35, .memory⟩, ⟨36, .world⟩], .load 32, [33, 32, 30]⟩,
      ⟨[⟨37, .memory⟩, ⟨38, .world⟩], .endLifetime, [35, 36, 30]⟩],
      .return [34, 37, 38]⟩]⟩

private def targetFunction (swapped := false) (decrement := 1)
    (missingPointer := false) (wrongStore := false) : EvoIR.Machine.Function :=
  ⟨"private_loop", [
    ⟨0, [w 0, w 1], [.frameSlot (x 2) 3 4 4, .store 0 (w 1) (x 2) 0 .w false],
      .b 1 (if missingPointer then [w 0] else [w 0, x 2])⟩,
    ⟨1, [w 3, x 4], [.movImm (w 5) 0, .cmpImm flags (w 3) 0 .w],
      .bCond flags (if swapped then .eq else .ne) 2 [w 3, x 4] 3 [x 4]⟩,
    ⟨2, [w 7, x 8], [.load 0 (w 9) (x 8) 0 .w false, .movImm (w 10) 1,
      .binaryImm .add (w 11) (w 9) 1 .w,
      .binaryImm .sub (w 12) (w 7) decrement .w,
      .store 0 (if wrongStore then w 12 else w 11) (x 8) 0 .w false],
      .b 1 [w 12, x 8]⟩,
    ⟨3, [x 13], [.load 0 (w 14) (x 13) 0 .w false], .ret [w 14]⟩]⟩

private def admitted (function : EvoIR.Machine.Function) : Bool :=
  (privateCFGProposedFunctionPair ⟨[sourceFunction]⟩ ⟨[function]⟩
    "private_loop" parameters .uint).isSome

private def missingPoint : Bool := (do
  let proposal ← PrivateProposal.propose sourceFunction
  privateCFGFunctionPair ⟨[sourceFunction]⟩ ⟨[targetFunction]⟩ "private_loop"
    parameters .uint (proposal.filter (·.target != 2))).isSome

theorem private_loop_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = parameters.map (fun p => p.type.type))
    (accepted : admitted (targetFunction) = true) :
    ∃ sourceEntry targetEntry,
      IR.initial ⟨[sourceFunction]⟩ "private_loop"
        (values.map sourceValueIR ++ [.memory ⟨1, []⟩, .world]) = .ok sourceEntry ∧
      EvoIR.Machine.initial ⟨[targetFunction]⟩ "private_loop"
        (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines ⟨[sourceFunction]⟩ ⟨[targetFunction]⟩ .uint
        sourceEntry targetEntry := by
  cases checked : privateCFGProposedFunctionPair ⟨[sourceFunction]⟩ ⟨[targetFunction]⟩
      "private_loop" parameters .uint with
  | none => simp [admitted, checked] at accepted
  | some certificate => exact certificate.checked values typed

private def endlessSource : IR.Function :=
  ⟨"endless", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, effects, [
    ⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], [
      ⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 0 4 4, [1, 2]⟩,
      ⟨[⟨6, .memory⟩, ⟨7, .world⟩], .store, [4, 5, 3, 0]⟩],
      .branch ⟨"loop", [3, 6, 7]⟩⟩,
    ⟨"loop", [⟨8, .pointer⟩, ⟨9, .memory⟩, ⟨10, .world⟩], [],
      .branch ⟨"loop", [8, 9, 10]⟩⟩]⟩

private def endlessTarget : EvoIR.Machine.Function :=
  ⟨"endless", [
    ⟨0, [w 0], [.frameSlot (x 1) 0 4 4, .store 0 (w 0) (x 1) 0 .w false], .b 1 [x 1]⟩,
    ⟨1, [x 2], [], .b 1 [x 2]⟩]⟩

def controls : List (String × Bool) := [
  ("private CFG all-input countdown", admitted (targetFunction)),
  ("private CFG reversed condition refusal", !admitted (targetFunction true)),
  ("private CFG changed decrement refusal", !admitted (targetFunction false 2)),
  ("private CFG missing pointer argument refusal", !admitted (targetFunction false 1 true)),
  ("private CFG changed stored value refusal", !admitted (targetFunction false 1 false true)),
  ("private CFG missing cutpoint refusal", !missingPoint),
  ("private CFG infinite pointer backedge", (privateCFGProposedFunctionPair
    ⟨[endlessSource]⟩ ⟨[endlessTarget]⟩ "endless" [⟨0, ⟨.uint, false⟩, .none⟩] .uint).isSome)]

def privateLoopSelfTest : IO UInt32 := do
  let mut failures : Nat := 0
  for (label, passed) in controls do
    if !passed then
      failures := failures + 1
      IO.eprintln s!"FAIL {label}"
  IO.println s!"machine-private-loop selftest: {controls.length - failures}/{controls.length} passed"
  return if failures = 0 then 0 else 1

#print axioms private_loop_all_inputs

end EvoIR.Core.Symbolic.MachineSelection.PrivateLoopTests
