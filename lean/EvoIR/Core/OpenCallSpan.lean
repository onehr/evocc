import EvoIR.Core.OpenCallingLoop
import EvoIR.Core.CallFrontierPair

/-! Checked caller fragments can end at their own loop cutpoints. Finite
fragments, complete suffixes and cyclic callees share the same open simulation
contract. Only the final table closure produces whole-execution refinement. -/
namespace EvoIR.Core.Symbolic

def CallerBoundary (points : List LayoutPoint) (fid : Nat) (target : IR.Function)
    (s : Source.State) (t : IR.State) : Prop :=
  ∃ owner, CutpointRelated (points.map LayoutPoint.joint) fid target s t none owner []

def OpenCaller (source : Source.Program) (ir : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (target : IR.Function) (type : Source.IntType) : Source.State → IR.State → Prop :=
  Transition.OpenCheckpoint (Source.step source) (IR.step ir) observeSource (observeTarget type)
    (CallerBoundary points fid target)

abbrev OpenContinuationBuilder (source : Source.Program) (ir : IR.Program)
    (points : List LayoutPoint) (fid : Nat) (target : IR.Function) (type : Source.IntType) :=
  (path : PathConditions inputWidths) → (cursor : SourceCursor inputWidths) →
    (label : String) → (pc : Nat) → (scope : Scope inputWidths) →
    Option (PLift (∀ σ, path.Holds σ → ∀ owner nextFrame,
      OpenCaller source ir points fid target type (cursor.denote σ fid)
        (cutTargetState σ target label pc owner nextFrame scope [])))

def openFiniteSpan (source : Source.Program) (ir : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (target : IR.Function) (tokens : Bool) (type : Source.IntType) (budget : Nat) :
    OpenContinuationBuilder source ir points fid target type := fun path cursor label pc scope => do
  let s ← sourceCutDiagram source (points.map (·.layout.site)) budget true cursor
  let t ← callingCutDiagram ir target tokens type (points.map (·.layout.label)) budget true scope label pc
  let same ← cutMatchUnder (fun _ a b => do
    let proof ← checkExit points fid target tokens type a b
    return ⟨fun _ _ => proof.down⟩) budget path s.val.normalizeTrace t.val.normalizeTrace
  return ⟨by
    intro σ holds owner nextFrame
    obtain ⟨sf, sp, positiveSource, sourceOutcome⟩ := s.property σ fid none
    obtain ⟨tf, tp, positiveTarget, targetOutcome⟩ := t.property σ owner nextFrame []
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
    · have sh : sf = se.denote σ fid := by simpa [sourceSuccess] using sourceOutcome
      obtain ⟨afterCalls, th⟩ := (by simpa [targetSuccess] using targetOutcome :
        ∃ afterCalls, tf = te.callState σ tokens target owner afterCalls [])
      subst sf
      subst tf
      cases matched with
      | returned control typed boundary equal =>
        rename_i word targetWord
        have identical := TargetExit.returned_congr σ tokens target owner afterCalls [] targetWord word (equal σ)
        rw [identical] at tp
        apply Transition.OpenCheckpoint.of_refines
        apply Transition.refines_of_terminals sp.toReaches tp.toReaches
          (by simp [Source.step, SourceCursor.denote, control, CutControl.denote]) rfl
          (o := Observation.returned type (word.denote σ).bits.toNat)
        · simp [observeSource, SourceCursor.denote, control, CutControl.denote, CWord.denote, typed]
        · cases tokens <;>
            simp [observeTarget, TargetExit.callState, cutResume, cutResults, callResults,
              Slot.denote, Word.denote, CWord.word, CWord.denote, typed, callTokens]
      | successor closed =>
        exact Transition.OpenCheckpoint.exit ⟨owner, closed σ afterCalls none owner []⟩
          (positiveSource rfl) (positiveTarget rfl)⟩

theorem open_frontier_prefix (source : Source.Program) (ir : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (f : IR.Function) (type : Source.IntType)
    (cursor : SourceCursor inputWidths) (label : String) (pc : Nat) (scope : Scope inputWidths)
    (st : CutTree inputWidths (SourceCursor inputWidths))
    (tt : CutTree inputWidths (TargetCallSite inputWidths))
    (sp : SourceCutRuns source false cursor st) (tp : TargetFrontierRuns ir f label pc scope tt)
    (path : PathConditions inputWidths)
    (matched : CutMatchUnder (fun σ s t => ∀ owner nextFrame,
      OpenCaller source ir points fid f type (s.denote σ fid) (t.denote σ f owner nextFrame []))
      path st.normalizeTrace tt.normalizeTrace)
    (σ : Subst inputWidths) (holds : path.Holds σ) (owner nextFrame : Nat) :
    OpenCaller source ir points fid f type (cursor.denote σ fid)
      (cutTargetState σ f label pc owner nextFrame scope []) := by
  obtain ⟨sf, ss, _, so⟩ := sp σ fid none
  obtain ⟨tf, ts, to⟩ := tp σ owner nextFrame []
  have agreement := matched σ holds
  rw [CutTree.normalize_trace_sound, CutTree.normalize_trace_sound] at agreement
  rcases agreement with ⟨fault, sh, th⟩ | ⟨se, te, sh, th, continued⟩
  · have sh' : sf.control = .halted (.error fault) := by simpa [sh] using so
    have th' : tf.control = .halted (.error fault) := by simpa [th] using to
    obtain ⟨code, rfl, observed⟩ := st.fault_observation σ fault sh
    apply Transition.OpenCheckpoint.of_refines
    apply Transition.refines_of_terminals ss.toReaches ts.toReaches
      (by simp [Source.step, sh']) (by simp [IR.step, th']) (o := Observation.diagnostic code)
    · simpa [observeSource, sh'] using observed
    · simpa [observeTarget, th'] using observed
  · have sh' : sf = se.denote σ fid := by simpa [sh] using so
    have th' : tf = te.denote σ f owner nextFrame [] := by simpa [th] using to
    subst sf
    subst tf
    exact (continued owner nextFrame).prepend ss.toReaches ts.toReaches

def checkOpenCall (source : Source.Program) (target : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (f : IR.Function) (type : Source.IntType)
    (proposals : CalleeProposals) (budget : Nat)
    (continuation : OpenContinuationBuilder source target points fid f type)
    (path : PathConditions inputWidths)
    (s : SourceCursor inputWidths) (t : TargetCallSite inputWidths) :
    Option (PLift (∀ σ, path.Holds σ → ∀ owner nextFrame,
      OpenCaller source target points fid f type (s.denote σ fid) (t.denote σ f owner nextFrame []))) := do
  match sourceCursor : s with
  | ⟨.call id returnType args, locals, frames⟩ =>
    let sf : {g // source.functions.find? (·.symbol == id) = some g} ←
      match found : source.functions.find? (·.symbol == id) with
      | none => none
      | some g => some ⟨g, found⟩
    have sourceId : sf.val.symbol = id := by simpa using List.find?_some sf.property
    if returnTyped : returnType = sf.val.returnType.type then
      if argsTyped : args.map CWord.type = sf.val.parameters.map (fun p => p.type.type) then
        let tf : {g // target.functions.find? (·.name == sf.val.name) = some g} ←
          match found : target.functions.find? (·.name == sf.val.name) with
          | none => none
          | some g => some ⟨g, found⟩
        have targetName : tf.val.name = sf.val.name := by simpa using List.find?_some tf.property
        let (fuel, layouts) ← match proposals sf.val.symbol with
          | some plan => some plan
          | none => (entryCutpoint sf.val tf.val).map fun layout => (budget, [layout])
        let d ← loopDiagram source target sf.val tf.val layouts fuel
        if function : target.functions.find? (·.name == f.name) = some f then
          let b : {b // f.blocks.find? (·.name == t.label) = some b} ←
            match found : f.blocks.find? (·.name == t.label) with
            | none => none
            | some b => some ⟨b, found⟩
          have blockName : b.val.name = t.label := by simpa using List.find?_some b.property
          let inst : {i // b.val.instructions[t.pc]? = some i} ←
            match found : b.val.instructions[t.pc]? with
            | none => none
            | some i => some ⟨i, found⟩
          if call : inst.val.op = .call tf.val.name then
            if operands : inst.val.operands.mapM (fun id => t.scope.lookup id) = some (loopInputSlots d.tokens args) then
              if results : inst.val.results.map IR.Binding.type = tf.val.returns then
                let bound : {out // t.scope.forReturn.appendSlots inst.val.results
                    (cutResults d.tokens (inputWord sf.val.returnType.type 0)) = some out} ←
                  match binding : t.scope.forReturn.appendSlots inst.val.results
                      (cutResults d.tokens (inputWord sf.val.returnType.type 0)) with
                  | none => none
                  | some out => some ⟨out, binding⟩
                let resumed ← continuation path.forReturn
                  ⟨.value (inputWord sf.val.returnType.type 0), locals.forReturn, frames.forReturn⟩
                  b.val.name (t.pc+1) bound.val
                return ⟨by
                  rw [sourceCursor]
                  intro σ holds owner nextFrame
                  have typed : (args.map (CWord.denote σ)).map Source.Value.type =
                      sf.val.parameters.map (fun p => p.type.type) := by
                    simpa only [List.map_map, Function.comp_def, CWord.denote] using argsTyped
                  have coverage := parameter_words_cover sf.val.parameters (args.map (CWord.denote σ)) typed
                  have actualArgs := slots_lookup_sound σ callTokens t.scope inst.val.operands
                    (loopInputSlots d.tokens args) operands
                  rw [loop_input_values] at actualArgs
                  let relation := fun a c => ResumedAt fid f
                    ⟨.value (inputWord sf.val.returnType.type 0), locals.forReturn, frames.forReturn⟩
                    b.val.name (t.pc+1) bound.val a c path.forReturn
                  have openCall := d.call_open (inputSubst (args.map (CWord.denote σ))) source target type
                    ⟨fid, locals.denote σ, frames.denote σ⟩
                    ⟨f.name, b.val.name, t.pc, t.scope.denote σ callTokens, owner⟩ nextFrame []
                    f b.val inst.val (by simpa only [sourceId] using sf.property) function
                    (by simpa only [blockName] using b.property) inst.property call
                    (by simpa only [targetName] using tf.property)
                    (by simpa only [coverage] using actualArgs) results relation (by
                      intro τ word afterCalls wordTyped
                      simpa only [TargetExit.callState, cut_results_values, relation] using
                        (return_binding_related (fid := fid) (target := f) (locals := locals) (frames := frames)
                          (label := b.val.name) (pc := t.pc+1) (path := path)
                          bound.property σ holds (word.denote τ) wordTyped owner afterCalls))
                  have complete := openCall.bind (nextBoundary := CallerBoundary points fid f) (by
                    rintro a c ⟨ρ, savedOwner, savedFrame, savedHolds, rfl, rfl⟩
                    exact .inr (resumed.down ρ savedHolds savedOwner savedFrame))
                  simpa only [OpenCaller, SourceCursor.denote, CutControl.denote, TargetCallSite.denote,
                    cutTargetState, targetState, coverage, returnTyped, sourceId, blockName] using complete⟩
              else none
            else none
          else none
        else none
      else none
    else none
  | _ => continuation path s t.label t.pc t.scope

def openCallerSuffix (source : Source.Program) (target : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (f : IR.Function) (tokens : Bool) (type : Source.IntType) (proposals : CalleeProposals) :
    Nat → OpenContinuationBuilder source target points fid f type
  | 0, _, _, _, _, _ => none
  | budget+1, path, cursor, label, pc, scope => do
    if let some span := openFiniteSpan source target points fid f tokens type (budget+1) path cursor label pc scope then
      return span
    if let some finite := returnContinuation source target fid f tokens type cursor label pc scope (budget+1) path then
      return ⟨fun σ holds owner frame => .of_refines (finite.refines σ holds owner frame)⟩
    let sp ← sourceCallFrontier source (budget+1) cursor
    let tp ← targetCallFrontier target f (budget+1) label pc scope
    let joined ← cutMatchUnder (checkOpenCall source target points fid f type proposals (budget+1)
      (openCallerSuffix source target points fid f tokens type proposals budget)) (budget+1) path
      sp.val.normalizeTrace tp.val.normalizeTrace
    return ⟨fun σ holds owner nextFrame => open_frontier_prefix source target points fid f type
      cursor label pc scope sp.val tp.val sp.property tp.property path joined.down σ holds owner nextFrame⟩

end EvoIR.Core.Symbolic
