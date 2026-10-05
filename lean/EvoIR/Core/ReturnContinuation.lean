import EvoIR.Core.CutpointInvariant
import EvoIR.Core.FreshReturn

/-! Construct a checked caller suffix from the actual source continuation and
the actual post-call instruction position. The result covers every symbolic
valuation and frame counter; it is not obtained by running a callee on sample
arguments or replacing its result by a chosen constant. -/
namespace EvoIR.Core.Symbolic

structure ReturnContinuation (source : Source.Program) (ir : IR.Program)
    (fid : Nat) (target : IR.Function) (tokens : Bool) (type : Source.IntType)
    (cursor : SourceCursor inputWidths) (label : String) (pc : Nat) (scope : Scope inputWidths) where
  sourceTree : CutTree inputWidths (SourceCursor inputWidths)
  targetTree : CutTree inputWidths (TargetExit inputWidths)
  -- A return-only frontier is already terminal on the source side. This
  -- finite suffix uses terminal agreement, not a zero-step cyclic backedge.
  sourceDiagram : SourceCutRuns source false cursor sourceTree
  targetDiagram : CallingCutRuns ir target tokens true label pc scope targetTree
  agreement : CutMatch (CutExitAgreement [] fid target tokens type)
    sourceTree.normalizeTrace targetTree.normalizeTrace

def returnContinuation (source : Source.Program) (ir : IR.Program)
    (fid : Nat) (target : IR.Function) (tokens : Bool) (type : Source.IntType)
    (cursor : SourceCursor inputWidths) (label : String) (pc : Nat) (scope : Scope inputWidths)
    (budget : Nat) : Option (ReturnContinuation source ir fid target tokens type cursor label pc scope) := do
  let s ← sourceCutDiagram source [] budget false cursor
  let t ← callingCutDiagram ir target tokens type [] budget true scope label pc
  let same ← cutMatch (checkExit [] fid target tokens type) s.val.normalizeTrace t.val.normalizeTrace
  return ⟨s.val, t.val, s.property, t.property, same.down⟩

def ResumedAt (fid : Nat) (target : IR.Function) (cursor : SourceCursor inputWidths)
    (label : String) (pc : Nat) (scope : Scope inputWidths) (s : Source.State) (t : IR.State) : Prop :=
  ∃ σ owner nextFrame, s = cursor.denote σ fid ∧
    t = cutTargetState σ target label pc owner nextFrame scope []

theorem ReturnContinuation.simulation {source : Source.Program} {ir : IR.Program}
    {fid : Nat} {target : IR.Function} {tokens : Bool} {type : Source.IntType}
    {cursor : SourceCursor inputWidths} {label : String} {pc : Nat} {scope : Scope inputWidths}
    (d : ReturnContinuation source ir fid target tokens type cursor label pc scope) :
    Transition.CheckpointSimulation (Source.step source) (IR.step ir)
      observeSource (observeTarget type) (ResumedAt fid target cursor label pc scope) := by
  constructor
  intro s t related
  obtain ⟨σ, owner, nextFrame, rfl, rfl⟩ := related
  obtain ⟨sf, sp, positiveSource, sourceOutcome⟩ := d.sourceDiagram σ fid none
  obtain ⟨tf, tp, positiveTarget, targetOutcome⟩ := d.targetDiagram σ owner nextFrame []
  have agreement := d.agreement.sound σ
  rw [CutTree.normalize_trace_sound, CutTree.normalize_trace_sound] at agreement
  rcases agreement with ⟨fault, sourceFailed, targetFailed⟩ |
    ⟨se, te, sourceSuccess, targetSuccess, matched⟩
  · have sh : sf.control = .halted (.error fault) := by simpa [sourceFailed] using sourceOutcome
    have th : tf.control = .halted (.error fault) := by simpa [targetFailed] using targetOutcome
    obtain ⟨code, rfl, observed⟩ := d.sourceTree.fault_observation σ fault sourceFailed
    exact .inl ⟨sf, tf, .diagnostic code, sp.toReaches, tp.toReaches,
      by simp [Source.step, sh], by simp [IR.step, th],
      by simpa [observeSource, sh] using observed,
      by simpa [observeTarget, th] using observed⟩
  · have sh : sf = se.denote σ fid := by simpa [sourceSuccess] using sourceOutcome
    obtain ⟨afterCalls, th⟩ := (by simpa [targetSuccess] using targetOutcome :
      ∃ afterCalls, tf = te.callState σ tokens target owner afterCalls [])
    subst sf
    subst tf
    cases matched with
    | returned control typed boundary same =>
      rename_i word targetWord
      have identical := TargetExit.returned_congr σ tokens target owner afterCalls [] targetWord word (same σ)
      rw [identical] at tp
      refine .inl ⟨_, _, .returned type (word.denote σ).bits.toNat,
        sp.toReaches, tp.toReaches, ?_, rfl, ?_, ?_⟩
      · simp [Source.step, SourceCursor.denote, control, CutControl.denote]
      · simp [observeSource, SourceCursor.denote, control, CutControl.denote, CWord.denote, typed]
      · cases tokens <;>
          simp [observeTarget, TargetExit.callState, cutResume, cutResults, callResults,
            Slot.denote, Word.denote, CWord.word, CWord.denote, typed, callTokens]
    | successor closed =>
      obtain ⟨point, member, _⟩ := closed σ afterCalls none owner []
      cases member

theorem ReturnContinuation.refines {source : Source.Program} {ir : IR.Program}
    {fid : Nat} {target : IR.Function} {tokens : Bool} {type : Source.IntType}
    {cursor : SourceCursor inputWidths} {label : String} {pc : Nat} {scope : Scope inputWidths}
    (d : ReturnContinuation source ir fid target tokens type cursor label pc scope)
    (σ : Subst inputWidths) (owner nextFrame : Nat) :
    LoweringRefines source ir type (cursor.denote σ fid)
      (cutTargetState σ target label pc owner nextFrame scope []) :=
  d.simulation.sound ⟨σ, owner, nextFrame, rfl, rfl⟩

def ContinuationRefines (source : Source.Program) (ir : IR.Program) (fid : Nat)
    (target : IR.Function) (type : Source.IntType) (cursor : SourceCursor inputWidths)
    (label : String) (pc : Nat) (scope : Scope inputWidths) : Prop :=
  ∀ σ owner nextFrame, LoweringRefines source ir type (cursor.denote σ fid)
    (cutTargetState σ target label pc owner nextFrame scope [])

abbrev ContinuationBuilder (source : Source.Program) (ir : IR.Program) (fid : Nat)
    (target : IR.Function) (type : Source.IntType) :=
  (cursor : SourceCursor inputWidths) → (label : String) → (pc : Nat) → (scope : Scope inputWidths) →
    Option (PLift (ContinuationRefines source ir fid target type cursor label pc scope))

structure CheckedReturn (source : Source.Program) (ir : IR.Program) (fid : Nat)
    (target : IR.Function) (callerTokens : Bool) (type returnType : Source.IntType)
    (locals : LocalStore inputWidths) (frames : SavedFrames inputWidths)
    (label : String) (pc : Nat) (scope : Scope inputWidths) (results : List IR.Binding)
    (calleeTokens : Bool) where
  boundScope : Scope inputWidths
  binding : scope.forReturn.appendSlots results (cutResults calleeTokens (inputWord returnType 0)) = some boundScope
  refinement : ContinuationRefines source ir fid target type
    ⟨.value (inputWord returnType 0), locals.forReturn, frames.forReturn⟩ label pc boundScope

def checkedReturnWith (source : Source.Program) (ir : IR.Program) (fid : Nat)
    (target : IR.Function) (callerTokens : Bool) (type returnType : Source.IntType)
    (locals : LocalStore inputWidths) (frames : SavedFrames inputWidths)
    (label : String) (pc : Nat) (scope : Scope inputWidths) (results : List IR.Binding)
    (calleeTokens : Bool) (build : ContinuationBuilder source ir fid target type) :
    Option (CheckedReturn source ir fid target callerTokens type returnType locals frames label pc scope results calleeTokens) := do
  let bound : {out // scope.forReturn.appendSlots results
      (cutResults calleeTokens (inputWord returnType 0)) = some out} ←
    match binding : scope.forReturn.appendSlots results (cutResults calleeTokens (inputWord returnType 0)) with
    | none => none
    | some out => some ⟨out, binding⟩
  let proof ← build ⟨.value (inputWord returnType 0), locals.forReturn, frames.forReturn⟩ label pc bound.val
  return ⟨bound.val, bound.property, proof.down⟩

def checkedReturn (source : Source.Program) (ir : IR.Program) (fid : Nat)
    (target : IR.Function) (callerTokens : Bool) (type returnType : Source.IntType)
    (locals : LocalStore inputWidths) (frames : SavedFrames inputWidths)
    (label : String) (pc : Nat) (scope : Scope inputWidths) (results : List IR.Binding)
    (calleeTokens : Bool) (budget : Nat) :
    Option (CheckedReturn source ir fid target callerTokens type returnType locals frames label pc scope results calleeTokens) :=
  checkedReturnWith source ir fid target callerTokens type returnType locals frames label pc scope results calleeTokens
    (fun cursor label pc scope => do
      let diagram ← returnContinuation source ir fid target callerTokens type cursor label pc scope budget
      return ⟨diagram.refines⟩)

def valueIRs (tokens : Bool) (value : Source.Value) : List IR.Value :=
  [sourceValueIR value] ++ if tokens then [.memory ⟨1, []⟩, .world] else []

theorem cut_results_values {Γ : Var → Nat} (σ : Subst Γ) (tokens : Bool) (word : CWord Γ) :
    (cutResults tokens word).map (Slot.denote σ callTokens) = valueIRs tokens (word.denote σ) := by
  cases tokens <;> rfl

/-- Every possible typed callee result instantiates the checked suffix without
changing any saved caller input. This discharges the return-site obligation;
it makes no assumption about the callee's arithmetic or termination. -/
theorem CheckedReturn.related {source : Source.Program} {ir : IR.Program} {fid : Nat}
    {target : IR.Function} {callerTokens : Bool} {type returnType : Source.IntType}
    {locals : LocalStore inputWidths} {frames : SavedFrames inputWidths}
    {label : String} {pc : Nat} {scope : Scope inputWidths} {results : List IR.Binding}
    {calleeTokens : Bool}
    (d : CheckedReturn source ir fid target callerTokens type returnType locals frames label pc scope results calleeTokens)
    (σ : Subst inputWidths) (value : Source.Value) (typed : value.type = returnType)
    (owner nextFrame : Nat) :
    ResumedAt fid target ⟨.value (inputWord returnType 0), locals.forReturn, frames.forReturn⟩ label pc d.boundScope
      ⟨.value value, locals.denote σ, frames.denote σ, fid⟩
      (cutResume (valueIRs calleeTokens value) nextFrame
        [⟨⟨target.name, label, pc, scope.denote σ callTokens, owner⟩, results⟩]) := by
  have fresh : (inputWord returnType 0).denote (returnSubst σ value) = value := by
    simpa only [typed] using return_subst_fresh σ value
  have resultValues : (cutResults calleeTokens (inputWord returnType 0)).map
      (Slot.denote (returnSubst σ value) callTokens) = valueIRs calleeTokens value := by
    rw [cut_results_values, fresh]
  have bound := append_slots_sound (returnSubst σ value) scope.forReturn d.boundScope results
    (cutResults calleeTokens (inputWord returnType 0)) d.binding
  rw [scope_for_return, resultValues] at bound
  refine ⟨returnSubst σ value, owner, nextFrame, ?_, ?_⟩
  · simp only [SourceCursor.denote, CutControl.denote, fresh, local_store_for_return, saved_frames_for_return]
  · simp [cutResume, bound, cutTargetState, targetState]

end EvoIR.Core.Symbolic
