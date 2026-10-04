import EvoIR.Core.SourceSpan
import EvoIR.Core.CallingCutpoint
import EvoIR.Core.ReturnCorrespondence

/-! Composition of a checked cyclic cutpoint table. Every successor must
instantiate a member of the same table for every incoming valuation; finite
span agreement alone is not a loop invariant or whole-function admission. -/
namespace EvoIR.Core.Symbolic

structure JointPoint (Γ : Var → Nat) where
  source : SourceCursor Γ
  label : String
  scope : Scope Γ

def JointPoint.targetState {Γ : Var → Nat} (σ : Subst Γ) (f : IR.Function)
    (nextFrame : Nat) (point : JointPoint Γ) (owner : Nat := 1)
    (stack : List IR.ReturnFrame := []) : IR.State :=
  cutTargetState σ f point.label 0 owner nextFrame point.scope stack

def CutpointRelated {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (target : IR.Function) (s : Source.State) (t : IR.State)
    (context : Option SourceReturnContext := none) (owner : Nat := 1)
    (stack : List IR.ReturnFrame := []) : Prop :=
  ∃ point ∈ entries, ∃ σ nextFrame,
    s = point.source.denote σ fid context ∧ t = point.targetState σ target nextFrame owner stack

inductive CutExitAgreement {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (target : IR.Function) (tokens : Bool) (type : Source.IntType) :
    SourceCursor Γ → TargetExit Γ → Prop where
  | returned {s word} (control : s.control = .returned word) (typed : word.type = type)
      (boundary : s.continuation = []) :
      CutExitAgreement entries fid target tokens type s (.returned word)
  | successor {s t} (closed : ∀ σ nextFrame context owner stack,
      CutpointRelated entries fid target (s.denote σ fid context)
        (t.callState σ tokens target owner nextFrame stack) context owner stack) :
      CutExitAgreement entries fid target tokens type s t

structure CheckedSpan {Γ : Var → Nat} (program : Source.Program) (ir : IR.Program)
    (entries : List (JointPoint Γ)) (fid : Nat) (target : IR.Function)
    (tokens : Bool) (type : Source.IntType) (point : JointPoint Γ) where
  sourceTree : CutTree Γ (SourceCursor Γ)
  targetTree : CutTree Γ (TargetExit Γ)
  source : SourceCutRuns program true point.source sourceTree
  targetDiagram : CallingCutRuns ir target tokens true point.label 0 point.scope targetTree
  agreement : CutMatch (CutExitAgreement entries fid target tokens type)
    sourceTree.normalizeTrace targetTree.normalizeTrace

theorem CutTree.fault_observation {Γ : Var → Nat} {α : Type} (σ : Subst Γ)
    (tree : CutTree Γ α) (fault : Source.Fault) (failed : tree.eval σ = .error fault) :
    ∃ code, fault = .diagnostic code ∧ checkedDiagnostic code = some (.diagnostic code) := by
  induction tree with
  | leaf a => simp [eval] at failed
  | checked guard next ih =>
    simp only [eval] at failed
    cases check : guards σ [guard] with
    | error f =>
      simp only [check, bind, Except.bind, Except.error.injEq] at failed
      subst f
      exact guards_fault_observation σ [guard] fault check
    | ok u =>
      simp only [check, bind, Except.bind] at failed
      exact ih failed
  | branch condition yes no first second =>
    simp only [eval] at failed
    split at failed
    · exact first failed
    · exact second failed

theorem checked_cutpoint_simulation {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (type : Source.IntType) (source : Source.Program) (program : IR.Program)
    (target : IR.Function) (tokens : Bool)
    (spans : ∀ point ∈ entries, CheckedSpan source program entries fid target tokens type point) :
    Transition.CheckpointSimulation (Source.step source) (IR.step program)
      observeSource (observeTarget type) (CutpointRelated entries fid target) := by
  constructor
  intro s t related
  obtain ⟨point, member, σ, nextFrame, rfl, rfl⟩ := related
  let span := spans point member
  obtain ⟨sf, sp, positiveSource, sourceOutcome⟩ := span.source σ fid none
  obtain ⟨tf, tp, positiveTarget, targetOutcome⟩ := span.targetDiagram σ 1 nextFrame []
  have agreement := span.agreement.sound σ
  rw [CutTree.normalize_trace_sound, CutTree.normalize_trace_sound] at agreement
  rcases agreement with ⟨fault, sourceFailed, targetFailed⟩ |
    ⟨se, te, sourceSuccess, targetSuccess, matched⟩
  · have sh : sf.control = .halted (.error fault) := by simpa [sourceFailed] using sourceOutcome
    have th : tf.control = .halted (.error fault) := by simpa [targetFailed] using targetOutcome
    obtain ⟨code, rfl, observed⟩ := span.sourceTree.fault_observation σ fault sourceFailed
    exact .inl ⟨sf, tf, .diagnostic code, sp.toReaches, tp.toReaches,
      by simp [Source.step, sh], by simp [IR.step, th],
      by simpa [observeSource, sh] using observed,
      by simpa [observeTarget, th] using observed⟩
  · have sh : sf = se.denote σ fid := by simpa [sourceSuccess] using sourceOutcome
    obtain ⟨afterCalls, th⟩ := (by simpa [targetSuccess] using targetOutcome :
      ∃ afterCalls, tf = te.callState σ tokens target 1 afterCalls)
    subst sf
    subst tf
    cases matched with
    | returned control typed boundary =>
      rename_i word
      refine .inl ⟨_, _, .returned type (word.denote σ).bits.toNat,
        sp.toReaches, tp.toReaches, ?_, rfl, ?_, ?_⟩
      · simp [Source.step, SourceCursor.denote, control, CutControl.denote]
      · simp [observeSource, SourceCursor.denote, control, CutControl.denote, CWord.denote, typed]
      · cases tokens <;>
          simp [observeTarget, TargetExit.callState, cutResume, cutResults, callResults,
            Slot.denote, Word.denote, CWord.word, CWord.denote, typed, callTokens]
    | successor closed =>
      exact .inr ⟨_, _, positiveSource rfl, positiveTarget rfl, closed σ afterCalls none 1 []⟩

theorem checked_cutpoint_refines {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (type : Source.IntType) (source : Source.Program) (program : IR.Program)
    (target : IR.Function) (tokens : Bool)
    (spans : ∀ point ∈ entries, CheckedSpan source program entries fid target tokens type point)
    (s : Source.State) (t : IR.State) (entry : CutpointRelated entries fid target s t) :
    LoweringRefines source program type s t :=
  (checked_cutpoint_simulation entries fid type source program target tokens spans).sound entry

end EvoIR.Core.Symbolic
