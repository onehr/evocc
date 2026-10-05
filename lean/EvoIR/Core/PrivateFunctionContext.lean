import EvoIR.Core.PrivateDiagram
import EvoIR.Core.FunctionContext
import EvoIR.Core.PathCondition

/-! Private function fragments leave the actual caller return boundary open.
The return summary retains the caller snapshot and fresh ownership bounds. -/
namespace EvoIR.Core.Symbolic

def PrivateFunctionReturn (incoming : IR.Memory) (owner fid : Nat) (type : Source.IntType)
    (tokens : Bool) (context : Option SourceReturnContext) (stack : List IR.ReturnFrame)
    (s : Source.State) (t : IR.State) : Prop :=
  ∃ value : Source.Value, value.type = type ∧ ∃ locals memory frame,
    s = returnedSource fid context locals value ∧
    incoming.ClosedExtension memory ∧ memory.WellAllocated ∧ memory.BeforeFrame frame ∧
    owner < frame ∧ (∀ object ∈ memory.objects, object ∉ incoming.objects → owner ≤ object.owner) ∧
    t = cutResume ([sourceValueIR value] ++ if tokens then [.memory memory, .world] else []) frame stack

inductive PrivateReturnAgreement {Γ : Var → Nat} (type : Source.IntType) :
    SourceCursor Γ → PrivateExit Γ → Prop where
  | returned {s word targetWord config} (control : s.control = .returned word)
      (typed : word.type = type) (boundary : s.continuation = [])
      (same : ∀ σ, targetWord.denote σ = word.denote σ) :
      PrivateReturnAgreement type s (.returned targetWord config)

def checkPrivateReturn {Γ : Var → Nat} (type : Source.IntType)
    (s : SourceCursor Γ) (t : PrivateExit Γ) : Option (PLift (PrivateReturnAgreement type s t)) :=
  match control : s.control, t with
  | .returned a, .returned b _ =>
    if same : (CWord.mk a.type a.term.normalizeValue) = (CWord.mk b.type b.term.normalizeValue) then
      if typed : a.type = type then
        if boundary : s.continuation = [] then
          some ⟨.returned control typed boundary (by
            intro σ
            have equal := congrArg (CWord.denote σ) same.symm
            simpa only [CWord.denote, Term.normalize_value_sound] using equal)⟩
        else none
      else none
    else none
  | _, _ => none

abbrev PrivateFiniteContinuation (source : Source.Program) (ir : IR.Program)
    (fid : Nat) (target : IR.Function) (tokens : Bool) (type : Source.IntType) :=
  (path : PathConditions inputWidths) → (cursor : SourceCursor inputWidths) →
    (label : String) → (pc : Nat) → (config : PrivateConfig inputWidths) →
    Option (PLift (∀ σ, path.Holds σ → ∀ values incoming context owner frame stack observationType,
      config.Realizes σ values incoming owner frame →
      Transition.OpenCheckpoint (Source.step source) (IR.step ir) observeSource (observeTarget observationType)
        (PrivateFunctionReturn incoming owner fid type tokens context stack)
        (cursor.denote σ fid context) (privateState σ values target label pc owner frame config stack)))

def privateFiniteSpan (source : Source.Program) (ir : IR.Program) (fid : Nat)
    (target : IR.Function) (tokens : Bool) (type : Source.IntType) (budget : Nat) :
    PrivateFiniteContinuation source ir fid target tokens type := fun path cursor label pc config => do
  let s ← sourceCutDiagram source [] budget true cursor
  let t ← privateCutDiagram ir target tokens type [] false budget true config label pc
  let same ← cutMatchUnder (fun _ a b => do
    let proof ← checkPrivateReturn type a b
    return ⟨fun _ _ => proof.down⟩) budget path s.val.normalizeTrace t.val.normalizeTrace
  return ⟨by
    intro σ holds values incoming context owner frame stack observationType related
    obtain ⟨sf, sp, positiveSource, sourceOutcome⟩ := s.property σ fid context
    obtain ⟨tf, tp, positiveTarget, targetOutcome⟩ := t.property σ values incoming owner frame stack related
    have agreement := same.down σ holds
    rw [CutTree.normalize_trace_sound, CutTree.normalize_trace_sound] at agreement
    rcases agreement with ⟨fault, sourceFailed, targetFailed⟩ |
      ⟨se, te, sourceSuccess, targetSuccess, matched⟩
    · have sh : sf.control = .halted (.error fault) := by simpa [sourceFailed] using sourceOutcome
      have th : tf.control = .halted (.error fault) := by simpa [targetFailed] using targetOutcome
      obtain ⟨code, rfl, observed⟩ := s.val.fault_observation σ fault sourceFailed
      apply Transition.OpenCheckpoint.of_refines
      apply Transition.refines_of_terminals sp.toReaches tp.toReaches
        (by simp [Source.step, sh]) (by simp [IR.step, th]) (o := Observation.diagnostic code)
      · simpa [observeSource, sh] using observed
      · simpa [observeTarget, th] using observed
    · have sh : sf = se.denote σ fid context := by simpa [sourceSuccess] using sourceOutcome
      have result : te.Realizes σ incoming tokens target owner stack tf := by
        simpa [targetSuccess] using targetOutcome
      cases matched with
      | returned control typed boundary equal =>
        rename_i word targetWord outConfig
        obtain ⟨outValues, memory, afterFrame, outRelated, current, th⟩ := result
        have summary := private_return_storage σ outValues incoming memory owner afterFrame outConfig outRelated current
        obtain ⟨_, _, _, _, ownerBound, _⟩ := outRelated.2.2.2.2
        have exit : PrivateFunctionReturn incoming owner fid type tokens context stack sf tf := by
          refine ⟨word.denote σ, typed, se.locals.denote σ, memory.endFrame owner, afterFrame,
            sh.trans (returned_cursor σ fid context se word control boundary),
            summary.1, summary.2.1, summary.2.2.1, ownerBound, summary.2.2.2, ?_⟩
          simpa only [privateReturnValues, equal σ] using th
        exact Transition.OpenCheckpoint.exit exit (positiveSource rfl) (positiveTarget rfl)⟩

end EvoIR.Core.Symbolic
