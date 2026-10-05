import EvoIR.MachineLowering

/-! The public proof-producing path checks both actual machine boundaries.
The same tests exercise scalar-only and memory/world-bearing signatures.
Mutation refusal is not counted as a successful program proof. -/
namespace EvoIR.Machine.LoweringTests

private def r (id : Nat) : VReg := ⟨id, .gpr32⟩
private def effects : Core.IR.Effects := ⟨false, false, false, false, false, false, false, false⟩
private def parameters : List Core.Source.Parameter := [⟨0, ⟨.uint, false⟩, .none⟩]

private def source (tokens : Bool) : Core.IR.Program := ⟨[
  ⟨"identity", [.bits 32] ++ if tokens then [.memory, .world] else [],
    [.bits 32] ++ if tokens then [.memory, .world] else [], false, effects,
    [⟨"entry", [⟨0, .bits 32⟩] ++ if tokens then [⟨3, .memory⟩, ⟨4, .world⟩] else [],
      [⟨[⟨1, .bits 32⟩], .constant 32 0, []⟩,
       ⟨[⟨2, .bits 32⟩], .scalar (.bin .add 32), [0, 1]⟩],
      .return ([2] ++ if tokens then [3, 4] else [])⟩]⟩]⟩

private def before : Program := ⟨[⟨"identity", [⟨0, [r 0],
  [.movImm (r 1) 0, .binary .add (r 2) (r 0) (r 1) .w, .movImm (r 9) 42],
  .ret [r 2]⟩]⟩]⟩

private def after (op : BinOp := .add) (immediate : Nat := 0) : Program := ⟨[⟨"identity", [⟨0, [r 0],
  [.movImm (r 1) immediate, .binary op (r 2) (r 0) (r 1) .w], .ret [r 2]⟩]⟩]⟩

private def accepted (tokens : Bool) : Option (CheckedLowering (source tokens) before (after)
    "identity" parameters .uint) :=
  checkLowering (source tokens) before (after) "identity" parameters .uint

example : (accepted false).isSome = true := by decide
example : (accepted true).isSome = true := by decide
example : (checkLowering (source true) before (after .mul)
    "identity" parameters .uint).isSome = false := by decide
example : (checkLowering (source true) before (after .add 1)
    "identity" parameters .uint).isSome = false := by decide
example : (checkLowering (source true) (after .add 1) (after .add 1)
    "identity" parameters .uint).isSome = false := by decide
example : (checkLoweringWithPlan (source true) before (after)
    "identity" parameters .uint ⟨[]⟩).isSome = false := by decide

/-- No concrete argument list is enumerated. The executable checker returns
a kernel-proved relation covering every typed argument and unbounded runs. -/
theorem identity_all_inputs (tokens : Bool) (values : List Core.Source.Value)
    (typed : values.map Core.Source.Value.type = [.uint]) :
    ∃ sourceEntry targetEntry,
      Core.IR.initial (source tokens) "identity" (loweringArguments tokens values) = .ok sourceEntry ∧
      initial (after) "identity" (values.map sourceArgument) = .ok targetEntry ∧
      IRToMachineRefines (source tokens) (after) .uint sourceEntry targetEntry := by
  cases tokens with
  | false =>
    have found : (accepted false).isSome = true := by decide
    cases result : accepted false with
    | none => simp [result] at found
    | some proof =>
      have noTokens : proof.tokens = false := by
        have checked : (accepted false).map CheckedLowering.tokens = some false := by decide
        simpa [result] using checked
      simpa only [noTokens] using proof.checked values typed
  | true =>
    have found : (accepted true).isSome = true := by decide
    cases result : accepted true with
    | none => simp [result] at found
    | some proof =>
      have withTokens : proof.tokens = true := by
        have checked : (accepted true).map CheckedLowering.tokens = some true := by decide
        simpa [result] using checked
      simpa only [withTokens] using proof.checked values typed

#print axioms identity_all_inputs
#print axioms lowerScalarPair
#print axioms lowerTokenPair

private def deadSource (returnDead := false) : Core.IR.Program := ⟨[
  ⟨"carry", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, effects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩],
      [⟨[⟨3, .bits 32⟩], .uninitialized (.bits 32), []⟩],
      .branch ⟨"next", [0, 3, 1, 2]⟩⟩,
     ⟨"next", [⟨4, .bits 32⟩, ⟨5, .bits 32⟩, ⟨6, .memory⟩, ⟨7, .world⟩], [],
      .return [if returnDead then 5 else 4, 6, 7]⟩]⟩]⟩

private def deadBefore : Program := ⟨[⟨"carry", [
  ⟨0, [r 0], [.movImm (r 1) 0], .b 1 [r 0, r 1]⟩,
  ⟨1, [r 2, r 3], [], .ret [r 2]⟩]⟩]⟩

private def deadAfter (wrongReturn := false) : Program := ⟨[⟨"carry", [
  ⟨0, [r 0], (if wrongReturn then [.movImm (r 4) 0] else []), .b 1 []⟩,
  ⟨1, [], [], .ret [if wrongReturn then r 4 else r 0]⟩]⟩]⟩

private def deadChecked : Option (CheckedLowering (deadSource) deadBefore (deadAfter)
    "carry" parameters .uint) :=
  checkLowering (deadSource) deadBefore (deadAfter) "carry" parameters .uint

example : deadChecked.isSome = true := by decide
example : (checkLowering (deadSource true) deadBefore (deadAfter)
    "carry" parameters .uint).isNone = true := by decide
example : (checkLowering (deadSource) deadBefore (deadAfter true)
    "carry" parameters .uint).isNone = true := by decide

/-- Poison is not equated to zero. The original entry is retained, and the
normalization proof establishes that the complete edge vector carries a value
which cannot affect any executed operand or observation. -/
theorem dead_placeholder_all_inputs (values : List Core.Source.Value)
    (typed : values.map Core.Source.Value.type = [.uint]) :
    ∃ sourceEntry targetEntry,
      Core.IR.initial (deadSource) "carry" (loweringArguments true values) = .ok sourceEntry ∧
      initial (deadAfter) "carry" (values.map sourceArgument) = .ok targetEntry ∧
      IRToMachineRefines (deadSource) (deadAfter) .uint sourceEntry targetEntry := by
  have accepted : deadChecked.isSome = true := by decide
  cases found : deadChecked with
  | none => simp [found] at accepted
  | some proof =>
    have tokens : proof.tokens = true := by
      have checked : deadChecked.map CheckedLowering.tokens = some true := by decide
      simpa [found] using checked
    simpa only [tokens] using proof.checked values typed

#print axioms lowerNormalizedPair
#print axioms dead_placeholder_all_inputs

end EvoIR.Machine.LoweringTests
