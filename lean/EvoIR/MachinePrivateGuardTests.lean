import EvoIR.MachinePrivateGuardedFunction

/-! Runtime mutation controls for the actual private guarded pair checker.
The source and machine constructors reproduce the storage/overflow shape of
the frozen private_checked fixture. Runtime acceptance is not a theorem;
the conditional all-input theorem consumes the returned proof certificate. -/
namespace EvoIR.Core.Symbolic.MachineSelection.PrivateGuardTests

private def effects : IR.Effects := ⟨true, true, false, false, false, false, true, false⟩
private def parameters : List Source.Parameter := [⟨0, ⟨.int, false⟩, .none⟩]
private def w (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr32⟩
private def x (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr64⟩
private def f (id : Nat) : EvoIR.Machine.VReg := ⟨id, .flags⟩

private def source : IR.Program := ⟨[
  ⟨"private_checked", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, effects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], [
      ⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 0 4 4, [1, 2]⟩,
      ⟨[⟨6, .memory⟩], .access 4 4, [4, 3]⟩,
      ⟨[⟨7, .memory⟩, ⟨8, .world⟩], .store, [6, 5, 3, 0]⟩,
      ⟨[⟨9, .memory⟩], .access 4 4, [7, 3]⟩,
      ⟨[⟨10, .bits 32⟩, ⟨11, .memory⟩, ⟨12, .world⟩], .load 32, [9, 8, 3]⟩,
      ⟨[⟨13, .bits 32⟩], .constant 32 1, []⟩,
      ⟨[⟨14, .bits 1⟩], .scalar (.guard .add 32), [10, 13]⟩,
      ⟨[], .assume .signedOverflow, [14]⟩,
      ⟨[⟨15, .bits 32⟩], .scalar (.bin .add 32), [10, 13]⟩,
      ⟨[⟨16, .memory⟩], .access 4 4, [11, 3]⟩,
      ⟨[⟨17, .memory⟩, ⟨18, .world⟩], .store, [16, 12, 3, 15]⟩,
      ⟨[⟨19, .memory⟩], .access 4 4, [17, 3]⟩,
      ⟨[⟨20, .bits 32⟩, ⟨21, .memory⟩, ⟨22, .world⟩], .load 32, [19, 18, 3]⟩,
      ⟨[⟨23, .memory⟩, ⟨24, .world⟩], .endLifetime, [21, 22, 3]⟩],
      .return [20, 23, 24]⟩]⟩]⟩

private structure Mutation where
  trapCode : Nat := 3004
  cond : EvoIR.Machine.Cond := .vs
  guardFlags : Nat := 5
  guardWidth : EvoIR.Machine.Width := .w
  missingPointer : Bool := false
  swappedArguments : Bool := false
  wrongPointer : Bool := false
  wrongStoredValue : Bool := false
  wrongIncrement : Bool := false
  trapInstruction : Bool := false
  wrongReturn : Bool := false

private def target (mutation : Mutation := {}) : EvoIR.Machine.Program := ⟨[
  ⟨"private_checked", [
    ⟨0, [w 0], [.frameSlot (x 1) 0 4 4, .store 0 (w 0) (x 1) 0 .w false,
      .load 0 (w 2) (x 1) 0 .w false, .movImm (w 3) (if mutation.wrongIncrement then 2 else 1),
      .flagBinary .adds (w 4) (f 5) (w 2) (w 3) mutation.guardWidth],
      .bCond (f mutation.guardFlags) mutation.cond 1 [] 2
        (if mutation.missingPointer then [w 4]
         else if mutation.swappedArguments then [w 4, x 1]
         else [if mutation.wrongPointer then x 99 else x 1, w 4])⟩,
    ⟨1, [], if mutation.trapInstruction then [.movImm (w 40) 0] else [], .brk mutation.trapCode⟩,
    ⟨2, [x 7, w 8], [.store 0 (if mutation.wrongStoredValue then w 0 else w 8) (x 7) 0 .w false,
      .load 0 (w 9) (x 7) 0 .w false], .ret [if mutation.wrongReturn then w 99 else w 9]⟩]⟩]⟩

private def admitted (machine : EvoIR.Machine.Program) : Bool :=
  (privateGuardedFunctionPair source machine "private_checked" parameters .int).isSome

def controls : List (String × Bool) := [
  ("private checked arithmetic all-input certificate", admitted (target)),
  ("private guard wrong BRK refusal", !admitted (target {trapCode := 3005})),
  ("private guard reversed overflow branch refusal", !admitted (target {cond := .vc})),
  ("private guard undefined flags refusal", !admitted (target {guardFlags := 99})),
  ("private guard wrong arithmetic width refusal", !admitted (target {guardWidth := .x})),
  ("private guard omitted pointer argument refusal", !admitted (target {missingPointer := true})),
  ("private guard swapped pointer-word arguments refusal", !admitted (target {swappedArguments := true})),
  ("private guard undefined pointer argument refusal", !admitted (target {wrongPointer := true})),
  ("private guard stale stored register refusal", !admitted (target {wrongStoredValue := true})),
  ("private guard changed arithmetic input refusal", !admitted (target {wrongIncrement := true})),
  ("private guard nonempty immediate trap refusal", !admitted (target {trapInstruction := true})),
  ("private guard undefined return register refusal", !admitted (target {wrongReturn := true}))]

def privateGuardSelfTest : IO UInt32 := do
  let mut failures : Nat := 0
  for (label, passed) in controls do
    if !passed then
      failures := failures + 1
      IO.eprintln s!"FAIL {label}"
  IO.println s!"machine-private-guard selftest: {controls.length - failures}/{controls.length} passed"
  return if failures = 0 then 0 else 1

theorem private_checked_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = parameters.map (fun p => p.type.type))
    (accepted : admitted (target) = true) :
    ∃ sourceEntry targetEntry,
      IR.initial source "private_checked"
        (values.map sourceValueIR ++ [.memory ⟨1, []⟩, .world]) = .ok sourceEntry ∧
      EvoIR.Machine.initial (target) "private_checked"
        (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines source (target) .int sourceEntry targetEntry := by
  cases checked : privateGuardedFunctionPair source (target) "private_checked" parameters .int with
  | none => simp [admitted, checked] at accepted
  | some certificate => exact certificate.checked values typed

#print axioms private_checked_all_inputs

end EvoIR.Core.Symbolic.MachineSelection.PrivateGuardTests
