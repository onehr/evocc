import EvoIR.IRPoisonSound
import EvoIR.MachineSelectionFunction

/-! The changing original IR, not its normalized surrogate, is the endpoint
of these all-input theorems. Controls reject false demand closure, consumed
poison, altered edge vectors, wrong widths and first-diagnostic changes. -/
namespace EvoIR.Core.IR.Poison.SoundTests

private def effects : Effects := ⟨false, false, false, false, false, false, false, false⟩
private def r (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr32⟩
private def parameters : List Source.Parameter := [⟨0, ⟨.uint, false⟩, .none⟩]

private def originalFunction : Function :=
  ⟨"carry", [.bits 32], [.bits 32], false, effects, [
    ⟨"entry", [⟨0, .bits 32⟩],
      [⟨[⟨1, .bits 32⟩], .uninitialized (.bits 32), []⟩], .branch ⟨"next", [0, 1]⟩⟩,
    ⟨"next", [⟨2, .bits 32⟩, ⟨3, .bits 32⟩], [], .return [2]⟩]⟩
private def original : Program := ⟨[originalFunction]⟩
private def plan : DemandPlan := [("carry", [0, 2])]
private def normalized : Program := rewriteProgram plan original
private def target : EvoIR.Machine.Program := ⟨[⟨"carry", [
  ⟨0, [r 0], [.movImm (r 1) 0], .b 1 [r 0, r 1]⟩,
  ⟨1, [r 2, r 3], [], .ret [r 2]⟩]⟩]⟩

example : checkPair original normalized plan = true := by decide
example : decide (original ≠ normalized) = true := by decide
example : programValid [("carry", [2])] original = false := by decide

private def wrongEdge : Program := ⟨[{originalFunction with blocks := [
  ⟨"entry", [⟨0, .bits 32⟩], [⟨[⟨1, .bits 32⟩], .constant 32 0, []⟩],
    .branch ⟨"next", [1, 0]⟩⟩,
  ⟨"next", [⟨2, .bits 32⟩, ⟨3, .bits 32⟩], [], .return [2]⟩]}]⟩
example : checkPair original wrongEdge plan = false := by decide

private def wrongWidth : Program := ⟨[{originalFunction with blocks := [
  ⟨"entry", [⟨0, .bits 32⟩], [⟨[⟨1, .bits 32⟩], .constant 64 0, []⟩],
    .branch ⟨"next", [0, 1]⟩⟩,
  ⟨"next", [⟨2, .bits 32⟩, ⟨3, .bits 32⟩], [], .return [2]⟩]}]⟩
example : checkPair original wrongWidth plan = false := by decide

private def consumed : Program := ⟨[{originalFunction with blocks := [
  ⟨"entry", [⟨0, .bits 32⟩], [⟨[⟨1, .bits 32⟩], .uninitialized (.bits 32), []⟩],
    .branch ⟨"next", [0, 1]⟩⟩,
  ⟨"next", [⟨2, .bits 32⟩, ⟨3, .bits 32⟩],
    [⟨[⟨4, .bits 32⟩], .scalar (.bin .add 32), [2, 3]⟩], .return [2]⟩]}]⟩
example : checkPair consumed (rewriteProgram plan consumed) plan = false := by decide
example : programValid [("carry", [0, 1, 2, 3])] consumed = true := by decide
example : rewriteProgram [("carry", [0, 1, 2, 3])] consumed = consumed := by decide

private def originalDiagnostic : Program := ⟨[⟨"diagnostic", [], [.bits 32], false, effects, [
  ⟨"entry", [], [⟨[⟨0, .bits 1⟩], .constant 1 0, []⟩,
    ⟨[], .assume .divisionByZero, [0]⟩], .return []⟩]⟩]⟩
private def alteredDiagnostic : Program := ⟨[⟨"diagnostic", [], [.bits 32], false, effects, [
  ⟨"entry", [], [⟨[⟨0, .bits 1⟩], .constant 1 0, []⟩,
    ⟨[], .assume .signedOverflow, [0]⟩], .return []⟩]⟩]⟩
example : checkPair originalDiagnostic alteredDiagnostic [("diagnostic", [0])] = false := by decide

private def selected : Bool :=
  (Symbolic.MachineSelection.scalarFunctionPair normalized target "carry" parameters .uint
    [("entry", 0), ("next", 1)]).isSome
example : selected = true := by decide

theorem carry_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = [.uint]) :
    ∃ sourceEntry targetEntry,
      initial original "carry" (values.map Symbolic.sourceValueIR) = .ok sourceEntry ∧
      EvoIR.Machine.initial target "carry" (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines original target .uint sourceEntry targetEntry := by
  have accepted : selected = true := by decide
  cases found : Symbolic.MachineSelection.scalarFunctionPair normalized target "carry" parameters .uint
      [("entry", 0), ("next", 1)] with
  | none => simp [selected, found] at accepted
  | some proof =>
    obtain ⟨sourceEntry, targetEntry, sourceStarted, targetStarted, refinement⟩ := proof.checked values typed
    obtain ⟨originalStarted, composed⟩ := sound original normalized plan (by decide) .uint "carry"
      (values.map Symbolic.sourceValueIR) sourceEntry sourceStarted
      (EvoIR.Machine.step .ssa target) (EvoIR.Machine.observe .uint) targetEntry refinement
    exact ⟨sourceEntry, targetEntry, originalStarted, targetStarted, composed⟩

private def loopOriginal : Program := ⟨[⟨"dead_loop", [.bits 32], [.bits 32], false, effects, [
  ⟨"entry", [⟨0, .bits 32⟩], [⟨[⟨1, .bits 32⟩], .uninitialized (.bits 32), []⟩],
    .branch ⟨"loop", [1]⟩⟩,
  ⟨"loop", [⟨2, .bits 32⟩], [], .branch ⟨"loop", [2]⟩⟩]⟩]⟩
private def loopPlan : DemandPlan := [("dead_loop", [])]
private def loopNormalized : Program := rewriteProgram loopPlan loopOriginal
private def loopTarget : EvoIR.Machine.Program := ⟨[⟨"dead_loop", [
  ⟨0, [r 0], [.movImm (r 1) 0], .b 1 [r 1]⟩,
  ⟨1, [r 2], [], .b 1 [r 2]⟩]⟩]⟩
private def loopSelected : Bool :=
  (Symbolic.MachineSelection.scalarFunctionPair loopNormalized loopTarget "dead_loop" parameters .uint
    [("entry", 0), ("loop", 1)]).isSome
example : checkPair loopOriginal loopNormalized loopPlan = true := by decide
example : loopSelected = true := by decide

/-- An actual poisoned loop-carried value may remain opaque forever. This
proof has no iteration bound, execution sample or poison-to-zero equality. -/
theorem dead_loop_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = [.uint]) :
    ∃ sourceEntry targetEntry,
      initial loopOriginal "dead_loop" (values.map Symbolic.sourceValueIR) = .ok sourceEntry ∧
      EvoIR.Machine.initial loopTarget "dead_loop" (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines loopOriginal loopTarget .uint sourceEntry targetEntry := by
  have accepted : loopSelected = true := by decide
  cases found : Symbolic.MachineSelection.scalarFunctionPair loopNormalized loopTarget "dead_loop" parameters .uint
      [("entry", 0), ("loop", 1)] with
  | none => simp [loopSelected, found] at accepted
  | some proof =>
    obtain ⟨sourceEntry, targetEntry, sourceStarted, targetStarted, refinement⟩ := proof.checked values typed
    obtain ⟨originalStarted, composed⟩ := sound loopOriginal loopNormalized loopPlan (by decide) .uint "dead_loop"
      (values.map Symbolic.sourceValueIR) sourceEntry sourceStarted
      (EvoIR.Machine.step .ssa loopTarget) (EvoIR.Machine.observe .uint) targetEntry refinement
    exact ⟨sourceEntry, targetEntry, originalStarted, targetStarted, composed⟩

#print axioms normalization_refines
#print axioms sound
#print axioms carry_all_inputs
#print axioms dead_loop_all_inputs

end EvoIR.Core.IR.Poison.SoundTests
