import EvoIR.Core.SourceSpan
import EvoIR.Core.TargetCutpoint
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
    (point : JointPoint Γ) : IR.State :=
  Symbolic.targetState σ (fun _ => .world) f point.label 0 1 2 point.scope

def CutpointRelated {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (target : IR.Function) (s : Source.State) (t : IR.State) : Prop :=
  ∃ point ∈ entries, ∃ σ, s = point.source.denote σ fid ∧ t = point.targetState σ target

inductive CutExitAgreement {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (target : IR.Function) (type : Source.IntType) :
    SourceCursor Γ → TargetExit Γ → Prop where
  | returned {s word} (control : s.control = .returned word) (typed : word.type = type) :
      CutExitAgreement entries fid target type s (.returned word)
  | successor {s t} (closed : ∀ σ,
      CutpointRelated entries fid target (s.denote σ fid)
        (t.state σ (fun _ => .world) target 1 2)) :
      CutExitAgreement entries fid target type s t

structure CheckedSpan {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (target : IR.Function) (type : Source.IntType) (point : JointPoint Γ) where
  sourceTree : CutTree Γ (SourceCursor Γ)
  targetTree : CutTree Γ (TargetExit Γ)
  source : SourceCutRuns true point.source sourceTree
  targetDiagram : TargetCutDiagram (fun _ => .world) target type true point.scope point.label targetTree
  agreement : CutMatch (CutExitAgreement entries fid target type)
    sourceTree.normalizeBranches targetTree.normalizeBranches

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
    (target : IR.Function) (found : program.functions.find? (·.name == target.name) = some target)
    (returns : target.returns = [.bits type.width])
    (spans : ∀ point ∈ entries, CheckedSpan entries fid target type point) :
    Transition.CheckpointSimulation (Source.step source) (IR.step program)
      observeSource (observeTarget type) (CutpointRelated entries fid target) := by
  constructor
  intro s t related
  obtain ⟨point, member, σ, rfl, rfl⟩ := related
  let span := spans point member
  obtain ⟨sf, sp, positiveSource, sourceOutcome⟩ := span.source σ source fid
  obtain ⟨tf, tp, positiveTarget, targetOutcome⟩ := span.targetDiagram.sound σ
    (fun _ => .world) (by intro id; rfl) program found returns 1 2
  have agreement := span.agreement.sound σ
  rw [CutTree.normalize_branches_sound, CutTree.normalize_branches_sound] at agreement
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
    have th : tf = te.state σ (fun _ => .world) target 1 2 := by
      simpa [targetSuccess] using targetOutcome
    subst sf
    subst tf
    cases matched with
    | returned control typed =>
      rename_i word
      refine .inl ⟨_, _, .returned type (word.denote σ).bits.toNat,
        sp.toReaches, tp.toReaches, ?_, rfl, ?_, ?_⟩
      · simp [Source.step, SourceCursor.denote, control, CutControl.denote]
      · simp [observeSource, SourceCursor.denote, control, CutControl.denote, CWord.denote, typed]
      · simp [observeTarget, TargetExit.state, sourceValueIR, CWord.denote, typed]
    | successor closed =>
      exact .inr ⟨_, _, positiveSource rfl, positiveTarget rfl, closed σ⟩

theorem checked_cutpoint_refines {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (type : Source.IntType) (source : Source.Program) (program : IR.Program)
    (target : IR.Function) (found : program.functions.find? (·.name == target.name) = some target)
    (returns : target.returns = [.bits type.width])
    (spans : ∀ point ∈ entries, CheckedSpan entries fid target type point)
    (s : Source.State) (t : IR.State) (entry : CutpointRelated entries fid target s t) :
    LoweringRefines source program type s t :=
  (checked_cutpoint_simulation entries fid type source program target found returns spans).sound entry

end EvoIR.Core.Symbolic
