import EvoIR.Core.Execution
import EvoIR.Core.ScalarCorrectness
import EvoIR.Core.IRExecution
import EvoIR.Core.IRReader
import EvoIR.Core.SourceEquality
import EvoIR.Core.Correspondence

/-!
Whole-body correspondence work for the frozen original musl function. The
trees below retain the original compound/return/conditional and the emitted
four-block CFG; neither machine executes an extracted arithmetic expression.
This is a proof development, not the general two-input result checker.
-/
namespace EvoIR.Core.AbsCorrespondence

def sourceBody : Source.Stmt :=
    .compound [.statement (.return ⟨.int, false⟩
      (.conditional .int
        (.binary .int .gt (.ident .int 1) (.literal .int 0))
        (.ident .int 1) (.unary .int .neg (.ident .int 1))))]

def source : Source.Program := ⟨[0], [
  ⟨0, "abs", ⟨.int, false⟩, false, [⟨1, ⟨.int, false⟩, .none⟩], sourceBody⟩]⟩

def target : IR.Program := ⟨[
  ⟨"abs", [.bits 32], [.bits 32], false,
    {IR.Reader.emptyEffects with mayTrap := true}, [
    ⟨"entry", [⟨0, .bits 32⟩], [
      ⟨[⟨1, .bits 32⟩], .constant 32 0, []⟩,
      ⟨[⟨2, .bits 1⟩], .scalar (.cmp .slt 32), [1, 0]⟩],
      .conditional 2 ⟨"b1", [0]⟩ ⟨"b2", [0]⟩⟩,
    ⟨"b1", [⟨3, .bits 32⟩], [], .branch ⟨"b3", [3, 3]⟩⟩,
    ⟨"b2", [⟨4, .bits 32⟩], [
      ⟨[⟨5, .bits 32⟩], .constant 32 0, []⟩,
      ⟨[⟨6, .bits 1⟩], .scalar (.guard .sub 32), [5, 4]⟩,
      ⟨[], .assume .signedOverflow, [6]⟩,
      ⟨[⟨7, .bits 32⟩], .scalar (.bin .sub 32), [5, 4]⟩],
      .branch ⟨"b3", [4, 7]⟩⟩,
    ⟨"b3", [⟨8, .bits 32⟩, ⟨9, .bits 32⟩], [], .return [9]⟩]⟩]⟩

def sourceEntry (x : BitVec 32) : Source.State :=
  ⟨.statement sourceBody,
    [⟨1, ⟨.int, false⟩, some ⟨.int, x⟩⟩], [.function none .int], 0⟩

def targetEntry (x : BitVec 32) : IR.State :=
  ⟨.running ⟨"abs", "entry", 0, [(0, .bits 32 x)], 1⟩, [], 2⟩

theorem source_initial (x : BitVec 32) :
    Source.initial source 0 [⟨.int, x⟩] = .ok (sourceEntry x) := by rfl

theorem target_initial (x : BitVec 32) :
    IR.initial target "abs" [.bits 32 x] = .ok (targetEntry x) := by rfl

def sourceObservation (r : CheckedValue 32) : Source.Control :=
  .halted (Source.liftChecked .int r)

def targetObservation (r : CheckedValue 32) : IR.Control :=
  .halted (match r with
    | .ok v => .ok [.bits 32 v]
    | .error code => .error (.diagnostic code))

theorem source_stopped (p : Source.Program) (n : Nat) (s : Source.State)
    (h : Source.step p s = none) : Source.runSteps p n s = s := by
  induction n with
  | zero => rfl
  | succ n ih => simp [Source.runSteps, h]

theorem source_steps_add (p : Source.Program) (m n : Nat) (s : Source.State) :
    Source.runSteps p (m+n) s = Source.runSteps p n (Source.runSteps p m s) := by
  induction m generalizing s with
  | zero => simp [Source.runSteps]
  | succ m ih =>
    cases h : Source.step p s with
    | none => simp [Nat.succ_add, Source.runSteps, h, source_stopped p n s h]
    | some t => simpa [Nat.succ_add, Source.runSteps, h] using ih t

def sourceBeforeChoice (x : BitVec 32) : Source.State :=
  ⟨.value (.boolean (0 < x.toInt)), [⟨1, ⟨.int, false⟩, some ⟨.int, x⟩⟩],
   [.choice .int (.ident .int 1) (.unary .int .neg (.ident .int 1)),
    .return ⟨.int, false⟩, .sequence [], .scope 1, .function none .int], 0⟩

set_option maxRecDepth 4096 in
theorem source_prefix (x : BitVec 32) :
    Source.runSteps source 9 (sourceEntry x) = sourceBeforeChoice x := by
  simp [sourceBody, sourceEntry, sourceBeforeChoice,
    Source.runSteps, Source.step, Source.enterStatement, Source.enterExpression,
    Source.resumeValue, Source.readSlot, Source.valueResult,
    Source.wrappingConstant, Source.binaryValue,
    Source.Value.cast, Source.Value.number, Source.Value.ofInt, Source.Value.ofNat,
    Source.Value.boolean, Source.Value.truth, Source.usualType, Source.IntType.rank,
    Source.IntType.signed, Source.IntType.width, pure, Except.pure]

set_option maxRecDepth 4096 in
theorem source_run (x : BitVec 32) :
    (Source.runSteps source 20 (sourceEntry x)).control =
      sourceObservation (sourceAbs x) := by
  rw [show 20 = 9+11 from rfl, source_steps_add, source_prefix]
  by_cases positive : 0 < x.toInt
  all_goals by_cases safe : signedRepresentable 32 (-x.toInt)
  all_goals simp [sourceBeforeChoice, sourceObservation, sourceAbs, sourceNeg,
    Source.runSteps, Source.step, Source.enterExpression,
    Source.resumeValue, Source.resumeFlow, Source.readSlot, Source.valueResult,
    Source.wrappingConstant, Source.constantValue,
    Source.unaryValue, Source.arithmeticResult,
    Source.Value.cast, Source.Value.number, Source.Value.ofInt, Source.Value.ofNat,
    Source.Value.boolean, Source.Value.truth, Source.IntType.signed, Source.IntType.width,
    Source.liftChecked, Source.fail, pure, Except.pure, positive, safe]
  simpa only [BitVec.toInt_ofInt] using
    (BitVec.ofInt_toInt (x := BitVec.ofInt 32 (-x.toInt)))

def targetBeforeBranch (x : BitVec 32) : IR.State :=
  ⟨.running ⟨"abs", "entry", 2,
    [(2, .bits 1 (denoteOp (.cmp .slt 32) (0#32, x))),
     (1, .bits 32 (0#32)), (0, .bits 32 x)], 1⟩, [], 2⟩

theorem target_prefix (x : BitVec 32) :
    IR.runSteps target 2 (targetEntry x) = targetBeforeBranch x := by rfl

theorem target_stopped (p : IR.Program) (n : Nat) (s : IR.State)
    (h : IR.step p s = none) : IR.runSteps p n s = s := by
  induction n with
  | zero => rfl
  | succ n ih => simp [IR.runSteps, h]

theorem target_steps_add (p : IR.Program) (m n : Nat) (s : IR.State) :
    IR.runSteps p (m+n) s = IR.runSteps p n (IR.runSteps p m s) := by
  induction m generalizing s with
  | zero => simp [IR.runSteps]
  | succ m ih =>
    cases h : IR.step p s with
    | none => simp [Nat.succ_add, IR.runSteps, h, target_stopped p n s h]
    | some t => simpa [Nat.succ_add, IR.runSteps, h] using ih t

def targetArm (x : BitVec 32) (positive : Bool) : IR.State :=
  ⟨.running ⟨"abs", if positive then "b1" else "b2", 0,
    [(if positive then 3 else 4, .bits 32 x)], 1⟩, [], 2⟩

theorem target_branch (x : BitVec 32) :
    IR.runSteps target 1 (targetBeforeBranch x) = targetArm x (0 < x.toInt) := by
  by_cases positive : 0 < x.toInt
  all_goals simp [target, targetBeforeBranch, targetArm,
    IR.runSteps, IR.step, IR.execute, IR.jump, IR.bind, IR.lookup, IR.Value.asBits,
    IR.Value.type, denoteOp, BitVec.slt, positive, pure, bind, Except.pure, Except.bind]
  all_goals rfl

theorem target_positive (x : BitVec 32) :
    (IR.runSteps target 2 (targetArm x true)).control = targetObservation (.ok x) := by rfl

def targetBeforeAssume (x : BitVec 32) : IR.State :=
  ⟨.running ⟨"abs", "b2", 2,
    [(6, .bits 1 (denoteOp (.guard .sub 32) (0#32, x))),
     (5, .bits 32 (0#32)), (4, .bits 32 x)], 1⟩, [], 2⟩

def targetAfterAssume (x : BitVec 32) : IR.State :=
  ⟨.running ⟨"abs", "b2", 3,
    [(6, .bits 1 (denoteOp (.guard .sub 32) (0#32, x))),
     (5, .bits 32 (0#32)), (4, .bits 32 x)], 1⟩, [], 2⟩

def targetOverflow : IR.State := ⟨.halted (.error (.diagnostic 3004)), [], 2⟩

theorem target_negative_prefix (x : BitVec 32) :
    IR.runSteps target 2 (targetArm x false) = targetBeforeAssume x := by rfl

theorem target_guard (x : BitVec 32) :
    IR.runSteps target 1 (targetBeforeAssume x) =
      if signedRepresentable 32 (-x.toInt) then targetAfterAssume x else targetOverflow := by
  by_cases safe : signedRepresentable 32 (-x.toInt)
  all_goals simp [target, targetBeforeAssume, targetAfterAssume, targetOverflow,
    IR.runSteps, IR.step, IR.execute, IR.lookup, List.lookup,
    IR.evalOperation, IR.Value.asBits, IR.Reason.code,
    denoteOp, safe, pure, bind, Except.pure, Except.bind]
  all_goals rfl

theorem target_negative_finish (x : BitVec 32) :
    (IR.runSteps target 3 (targetAfterAssume x)).control =
      targetObservation (.ok ((0#32)-x)) := by rfl

theorem target_negative (x : BitVec 32) :
    (IR.runSteps target 6 (targetArm x false)).control =
      targetObservation (sourceNeg x) := by
  rw [show 6 = 2+4 from rfl, target_steps_add, target_negative_prefix,
      show 4 = 1+3 from rfl, target_steps_add, target_guard]
  by_cases safe : signedRepresentable 32 (-x.toInt)
  · simp [safe, target_negative_finish, sourceNeg, targetObservation, BitVec.ofInt_neg]
  · simp [safe, sourceNeg, targetOverflow, targetObservation, IR.runSteps, IR.step]

theorem target_halted_control (p : IR.Program) (n : Nat) (s : IR.State)
    (result : Except Source.Fault (List IR.Value)) (h : s.control = .halted result) :
    (IR.runSteps p n s).control = .halted result := by
  have stopped : IR.step p s = none := by simp [IR.step, h]
  rw [target_stopped p n s stopped]
  exact h

theorem target_run (x : BitVec 32) :
    (IR.runSteps target 9 (targetEntry x)).control =
      targetObservation (sourceAbs x) := by
  rw [show 9 = 2+7 from rfl, target_steps_add, target_prefix,
      show 7 = 1+6 from rfl, target_steps_add, target_branch]
  by_cases positive : 0 < x.toInt
  · simp only [positive, decide_true, sourceAbs, if_true]
    rw [show 6 = 2+4 from rfl, target_steps_add]
    exact target_halted_control target 4 _ _ (target_positive x)
  · simpa [sourceAbs, positive] using target_negative x

/-- Actual finite execution witnesses for every argument, not sampled tests.
The witnesses include the full source continuation and target CFG states. -/
theorem whole_body (x : BitVec 32) :
    ∃ s t,
      Source.Steps source (sourceEntry x) s ∧
      IR.Steps target (targetEntry x) t ∧
      s.control = sourceObservation (sourceAbs x) ∧
      t.control = targetObservation (sourceAbs x) ∧
      Source.step source s = none ∧ IR.step target t = none := by
  refine ⟨Source.runSteps source 20 (sourceEntry x),
    IR.runSteps target 9 (targetEntry x),
    Source.runSteps_prefix _ _ _, IR.runSteps_prefix _ _ _,
    source_run x, target_run x, ?_, ?_⟩
  · simp only [Source.step, source_run, sourceObservation]
  · simp only [IR.step, target_run, targetObservation]

/-- This is the shared observable result, including the original E3004
failure on INT_MIN, not an unchecked-C exemption. -/
def observation (x : BitVec 32) : Observation :=
  match sourceAbs x with
  | .ok value => .returned .int value.toNat
  | .error code => .diagnostic code

theorem source_observed (x : BitVec 32) (s : Source.State)
    (h : s.control = sourceObservation (sourceAbs x)) :
    observeSource s = some (observation x) := by
  by_cases positive : 0 < x.toInt
  all_goals by_cases safe : signedRepresentable 32 (-x.toInt)
  all_goals simp [observeSource, h, sourceObservation, Source.liftChecked,
    observation, sourceAbs, sourceNeg, positive, safe, checkedDiagnostic]
  all_goals rfl

theorem target_observed (x : BitVec 32) (t : IR.State)
    (h : t.control = targetObservation (sourceAbs x)) :
    observeTarget .int t = some (observation x) := by
  by_cases positive : 0 < x.toInt
  all_goals by_cases safe : signedRepresentable 32 (-x.toInt)
  all_goals simp [observeTarget, h, targetObservation, Source.IntType.width,
    observation, sourceAbs, sourceNeg, positive, safe, checkedDiagnostic]

theorem whole_body_refines (x : BitVec 32) :
    LoweringRefines source target .int (sourceEntry x) (targetEntry x) := by
  obtain ⟨s, t, hs, ht, os, ot, ss, st⟩ := whole_body x
  exact Transition.refines_of_terminals hs.toReaches ht.toReaches ss st
    (source_observed x s os) (target_observed x t ot)

/-- The first frozen-pilot pair check. It compares both complete input trees,
including signatures, effects, all blocks and unused edge arguments. This
exact pair is only one V01 proof case, not the complete core-v1 checker. -/
def checkPilotPair (s : Source.Program) (t : IR.Program) : Bool :=
  decide (s = source ∧ t = target)

theorem pilot_pair_sound (s : Source.Program) (t : IR.Program)
    (accepted : checkPilotPair s t = true) (x : BitVec 32) :
    ∃ s₀ t₀ s₁ t₁,
      Source.initial s 0 [⟨.int, x⟩] = .ok s₀ ∧
      IR.initial t "abs" [.bits 32 x] = .ok t₀ ∧
      Source.Steps s s₀ s₁ ∧ IR.Steps t t₀ t₁ ∧
      s₁.control = sourceObservation (sourceAbs x) ∧
      t₁.control = targetObservation (sourceAbs x) ∧
      Source.step s s₁ = none ∧ IR.step t t₁ = none := by
  have h : s = source ∧ t = target := by simpa [checkPilotPair] using accepted
  rcases h with ⟨rfl, rfl⟩
  obtain ⟨s₁, t₁, hs, ht, os, ot, ss, st⟩ := whole_body x
  exact ⟨sourceEntry x, targetEntry x, s₁, t₁,
    source_initial x, target_initial x, hs, ht, os, ot, ss, st⟩

/-- Admission relates the two supplied programs and their actual entry states.
The conclusion covers every execution, not just one successful bounded run. -/
theorem pilot_pair_refines (s : Source.Program) (t : IR.Program)
    (accepted : checkPilotPair s t = true) (x : BitVec 32) :
    ∃ s₀ t₀,
      Source.initial s 0 [⟨.int, x⟩] = .ok s₀ ∧
      IR.initial t "abs" [.bits 32 x] = .ok t₀ ∧
      LoweringRefines s t .int s₀ t₀ := by
  have h : s = source ∧ t = target := by simpa [checkPilotPair] using accepted
  rcases h with ⟨rfl, rfl⟩
  exact ⟨sourceEntry x, targetEntry x, source_initial x, target_initial x, whole_body_refines x⟩

end EvoIR.Core.AbsCorrespondence
