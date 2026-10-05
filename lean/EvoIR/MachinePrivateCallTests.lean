import EvoIR.MachinePrivateModule

/-! Actual private caller shape controls. A caller cell is preserved while its
callee allocates, loads and retires a separate cell. Runtime controls are
separate from kernel soundness. No execution bound occurs in the all-input
certificate theorem. The full call-loop is covered by actual backend replay,
not by a duplicated Lean fixture here. -/
namespace EvoIR.Core.Symbolic.MachineSelection.PrivateCallTests

private def effects : IR.Effects := ⟨true, true, false, false, false, false, true, false⟩
private def parameters (count : Nat) : List Source.Parameter :=
  (List.range count).map fun id => ⟨id, ⟨.uint, false⟩, .none⟩
private def w (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr32⟩
private def x (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr64⟩

private def cell (name : String) (secondParameter := false) : IR.Function :=
  let m := if secondParameter then 2 else 1
  let p := m+2
  ⟨name, (if secondParameter then [.bits 32, .bits 32] else [.bits 32]) ++ [.memory, .world],
    [.bits 32, .memory, .world], secondParameter, effects,
    [⟨"entry", (if secondParameter then [⟨0, .bits 32⟩, ⟨1, .bits 32⟩] else [⟨0, .bits 32⟩]) ++
      [⟨m, .memory⟩, ⟨m+1, .world⟩], [
      ⟨[⟨p, .pointer⟩, ⟨p+1, .memory⟩, ⟨p+2, .world⟩], .allocate 0 4 4, [m, m+1]⟩,
      ⟨[⟨p+3, .memory⟩], .access 4 4, [p+1, p]⟩,
      ⟨[⟨p+4, .memory⟩, ⟨p+5, .world⟩], .store, [p+3, p+2, p, 0]⟩,
      ⟨[⟨p+6, .memory⟩], .access 4 4, [p+4, p]⟩,
      ⟨[⟨p+7, .bits 32⟩, ⟨p+8, .memory⟩, ⟨p+9, .world⟩], .load 32, [p+6, p+5, p]⟩,
      ⟨[⟨p+10, .memory⟩, ⟨p+11, .world⟩], .endLifetime, [p+8, p+9, p]⟩],
      .return [p+7, p+10, p+11]⟩]⟩

private def caller : IR.Function :=
  ⟨"private_caller", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, effects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], [
      ⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 2 4 4, [1, 2]⟩,
      ⟨[⟨6, .memory⟩], .access 4 4, [4, 3]⟩,
      ⟨[⟨7, .memory⟩, ⟨8, .world⟩], .store, [6, 5, 3, 0]⟩,
      ⟨[⟨9, .bits 32⟩, ⟨10, .memory⟩, ⟨11, .world⟩], .call "private_word", [0, 7, 8]⟩,
      ⟨[⟨12, .memory⟩], .access 4 4, [10, 3]⟩,
      ⟨[⟨13, .bits 32⟩, ⟨14, .memory⟩, ⟨15, .world⟩], .load 32, [12, 11, 3]⟩,
      ⟨[⟨16, .bits 32⟩], .scalar (.bin .add 32), [9, 13]⟩,
      ⟨[⟨17, .memory⟩, ⟨18, .world⟩], .endLifetime, [14, 15, 3]⟩],
      .return [16, 17, 18]⟩]⟩

private def nativeCell (name : String) (secondParameter := false) (badReturn := false) :
    EvoIR.Machine.Function :=
  let ptr := if secondParameter then 2 else 1
  ⟨name, [⟨0, if secondParameter then [w 0, w 1] else [w 0],
    [.frameSlot (x ptr) 0 4 4, .store 0 (w 0) (x ptr) 0 .w false,
      .load 0 (w (ptr+1)) (x ptr) 0 .w false], .ret [w (if badReturn then 99 else ptr+1)]⟩]⟩

private structure CallerMutation where
  badArgument : Bool := false
  pointerResult : Bool := false
  missingResult : Bool := false
  badCalleeReturn : Bool := false
  overwriteCell : Bool := false
  wrongSum : Bool := false

private def callerTarget (change : CallerMutation := {}) : EvoIR.Machine.Program := ⟨[
  nativeCell "private_word" false change.badCalleeReturn,
  ⟨"private_caller", [⟨0, [w 0],
    [.frameSlot (x 1) 2 4 4, .store 0 (w 0) (x 1) 0 .w false] ++
    (if change.badArgument then [.movImm (w 5) 0] else []) ++
    [.bl "private_word" [if change.badArgument then w 5 else w 0]
      (if change.missingResult then [] else [if change.pointerResult then x 1 else w 2])] ++
    (if change.overwriteCell then [.frameSlot (x 1) 99 4 4] else []) ++
    [.load 0 (w 3) (x 1) 0 .w false,
      .binary (if change.wrongSum then .sub else .add) (w 4) (w 2) (w 3) .w], .ret [w 4]⟩]⟩]⟩

private def callerSource : IR.Program := ⟨[cell "private_word", caller]⟩
private def callerAdmitted (machine : EvoIR.Machine.Program) : Bool :=
  (privateModulePair callerSource machine "private_caller" (parameters 1) .uint).isSome

private def largeId : Nat := 2^64+11
private def sentinel : IR.Object := ⟨largeId, 99, 0, 4, true,
  IR.scalarBytes 32 (0x11223344#32)⟩
private def incoming : IR.Memory := ⟨largeId+1, [sentinel]⟩

private def preservesIncoming (machine : EvoIR.Machine.Program) (entry : String)
    (args : List Nat) (expected : Nat) : Bool :=
  match EvoIR.Machine.initial machine entry (args.map fun n => .int (BitVec.ofNat 64 n)) incoming with
  | .error _ => false
  | .ok state =>
    let last := EvoIR.Machine.runSteps .ssa machine 250 state
    decide (EvoIR.Machine.observe .uint last = some (.returned .uint expected) ∧
      last.memory.objects.find? (·.id == largeId) = some sentinel) &&
      last.memory.objects.all (fun object => object.owner == 0 || !object.live)

def controls : List (String × Bool) := [
  ("private caller all-input certificate", callerAdmitted (callerTarget)),
  ("private caller changed used call argument refusal", !callerAdmitted (callerTarget {badArgument := true})),
  ("private caller pointer overwritten by return refusal", !callerAdmitted (callerTarget {pointerResult := true})),
  ("private caller missing result register refusal", !callerAdmitted (callerTarget {missingResult := true})),
  ("private caller undefined callee return refusal", !callerAdmitted (callerTarget {badCalleeReturn := true})),
  ("private caller replaced saved cell refusal", !callerAdmitted (callerTarget {overwriteCell := true})),
  ("private caller wrong returned sum refusal", !callerAdmitted (callerTarget {wrongSum := true})),
  ("private caller Nat pointers and incoming frame retained", preservesIncoming (callerTarget) "private_caller" [7] 14),
  ("private caller width and incoming Nat pointer retained", preservesIncoming (callerTarget) "private_caller"
    [0xffffffff] 0xfffffffe)]

def privateCallSelfTest : IO UInt32 := do
  let mut failures : Nat := 0
  for (label, passed) in controls do
    if !passed then
      failures := failures + 1
      IO.eprintln s!"FAIL {label}"
  IO.println s!"machine-private-call selftest: {controls.length - failures}/{controls.length} passed"
  return if failures = 0 then 0 else 1

theorem private_caller_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = (parameters 1).map (fun p => p.type.type))
    (accepted : callerAdmitted (callerTarget) = true) :
    ∃ sourceEntry targetEntry,
      IR.initial callerSource "private_caller"
        (values.map sourceValueIR ++ [.memory ⟨1, []⟩, .world]) = .ok sourceEntry ∧
      EvoIR.Machine.initial (callerTarget) "private_caller"
        (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines callerSource (callerTarget) .uint sourceEntry targetEntry := by
  cases checked : privateModulePair callerSource (callerTarget) "private_caller" (parameters 1) .uint with
  | none => simp [callerAdmitted, checked] at accepted
  | some certificate => exact certificate.checked values typed

#print axioms private_caller_all_inputs

end EvoIR.Core.Symbolic.MachineSelection.PrivateCallTests
