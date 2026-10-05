import EvoIR.Core.FunctionContext
import EvoIR.Core.CallFrontierPair

/-! Function-body fragments quantified over the suspended caller and target
return stack. All calls use independently checked body diagrams from the same
builder; no finite-only summary is used for a cyclic callee. -/
namespace EvoIR.Core.Symbolic

def contextFiniteSpan (source : Source.Program) (ir : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (target : IR.Function) (tokens : Bool) (type : Source.IntType) (budget : Nat) :
    ContextContinuationBuilder source ir points fid target tokens type := fun path cursor label pc scope => do
  let s ← sourceCutDiagram source (points.map (·.layout.site)) budget true cursor
  let t ← callingCutDiagram ir target tokens type (points.map (·.layout.label)) budget true scope label pc
  let same ← cutMatchUnder (fun _ a b => do
    let proof ← checkExit points fid target tokens type a b
    return ⟨fun _ _ => proof.down⟩) budget path s.val.normalizeTrace t.val.normalizeTrace
  return ⟨by
    intro σ holds context owner nextFrame stack observationType
    obtain ⟨sf, sp, positiveSource, sourceOutcome⟩ := s.property σ fid context
    obtain ⟨tf, tp, positiveTarget, targetOutcome⟩ := t.property σ owner nextFrame stack
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
      obtain ⟨afterCalls, th⟩ := (by simpa [targetSuccess] using targetOutcome :
        ∃ afterCalls, tf = te.callState σ tokens target owner afterCalls stack)
      subst sf
      subst tf
      cases matched with
      | returned control typed boundary equal =>
        rename_i word targetWord
        have identical := TargetExit.returned_congr σ tokens target owner afterCalls stack targetWord word (equal σ)
        rw [identical] at positiveTarget
        refine Transition.OpenCheckpoint.exit (boundary := FunctionBoundary points fid target type tokens context owner stack)
          (.inr ⟨word.denote σ, typed, se.locals.denote σ, afterCalls, ?_, ?_⟩)
          (positiveSource rfl) (positiveTarget rfl)
        · exact returned_cursor σ fid context se word control boundary
        · simp only [TargetExit.callState, cut_results_values]
      | successor closed =>
        exact Transition.OpenCheckpoint.exit (.inl (closed σ afterCalls context owner stack))
          (positiveSource rfl) (positiveTarget rfl)⟩

theorem context_frontier_prefix (source : Source.Program) (ir : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (f : IR.Function) (tokens : Bool) (type : Source.IntType)
    (cursor : SourceCursor inputWidths) (label : String) (pc : Nat) (scope : Scope inputWidths)
    (st : CutTree inputWidths (SourceCursor inputWidths))
    (tt : CutTree inputWidths (TargetCallSite inputWidths))
    (sp : SourceCutRuns source false cursor st) (tp : TargetFrontierRuns ir f label pc scope tt)
    (path : PathConditions inputWidths)
    (matched : CutMatchUnder (fun σ s t => ∀ context owner nextFrame stack observationType,
      ContextOpen source ir points fid f type observationType tokens context owner stack
        (s.denote σ fid context) (t.denote σ f owner nextFrame stack))
      path st.normalizeTrace tt.normalizeTrace)
    (σ : Subst inputWidths) (holds : path.Holds σ) (context : Option SourceReturnContext)
    (owner nextFrame : Nat) (stack : List IR.ReturnFrame) (observationType : Source.IntType) :
    ContextOpen source ir points fid f type observationType tokens context owner stack (cursor.denote σ fid context)
      (cutTargetState σ f label pc owner nextFrame scope stack) := by
  obtain ⟨sf, ss, _, so⟩ := sp σ fid context
  obtain ⟨tf, ts, to⟩ := tp σ owner nextFrame stack
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
  · have sh' : sf = se.denote σ fid context := by simpa [sh] using so
    have th' : tf = te.denote σ f owner nextFrame stack := by simpa [th] using to
    subst sf
    subst tf
    exact (continued context owner nextFrame stack observationType).prepend ss.toReaches ts.toReaches

def checkContextCall (source : Source.Program) (target : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (f : IR.Function) (tokens : Bool) (type : Source.IntType)
    (callee : OpenFunctionBuilder source target)
    (continuation : ContextContinuationBuilder source target points fid f tokens type)
    (path : PathConditions inputWidths)
    (s : SourceCursor inputWidths) (t : TargetCallSite inputWidths) :
    Option (PLift (∀ σ, path.Holds σ → ∀ context owner nextFrame stack observationType,
      ContextOpen source target points fid f type observationType tokens context owner stack
        (s.denote σ fid context) (t.denote σ f owner nextFrame stack))) := do
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
        let d ← callee sf.val tf.val
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
                  intro σ holds context owner nextFrame stack observationType
                  have typed : (args.map (CWord.denote σ)).map Source.Value.type =
                      sf.val.parameters.map (fun p => p.type.type) := by
                    simpa only [List.map_map, Function.comp_def, CWord.denote] using argsTyped
                  have coverage := parameter_words_cover sf.val.parameters (args.map (CWord.denote σ)) typed
                  have actualArgs := slots_lookup_sound σ callTokens t.scope inst.val.operands
                    (loopInputSlots d.tokens args) operands
                  rw [loop_input_values] at actualArgs

                  have openCall := d.call_open (inputSubst (args.map (CWord.denote σ))) source target observationType
                    ⟨fid, locals.denote σ, sourceContextFrames context (frames.denote σ)⟩
                    ⟨f.name, b.val.name, t.pc, t.scope.denote σ callTokens, owner⟩ nextFrame stack
                    f b.val inst.val (by simpa only [sourceId] using sf.property) function
                    (by simpa only [blockName] using b.property) inst.property call
                    (by simpa only [targetName] using tf.property)
                    (by simpa only [coverage] using actualArgs) results
                  have complete := openCall.bind
                    (nextBoundary := FunctionBoundary points fid f type tokens context owner stack) (by
                      rintro a c ⟨value, valueTyped, _, frame, rfl, rfl⟩
                      have equal := context_return_binding (fid := fid) (target := f) (locals := locals) (frames := frames)
                        (label := b.val.name) (pc := t.pc+1) bound.property σ value valueTyped context owner frame stack
                      have suffix := resumed.down (returnSubst σ value)
                        ((path.for_return_sound σ value).mpr holds) context owner frame stack observationType
                      rw [equal.1, equal.2] at suffix
                      exact .inr suffix)
                  cases context <;>
                  simpa only [ContextOpen, sourceContextFrames, SourceCursor.denote, CutControl.denote, TargetCallSite.denote,
                    cutTargetState, targetState, coverage, returnTyped, sourceId, blockName] using complete⟩
              else none
            else none
          else none
        else none
      else none
    else none
  | _ => continuation path s t.label t.pc t.scope


def contextCallerSuffix (source : Source.Program) (target : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (f : IR.Function) (tokens : Bool) (type : Source.IntType)
    (callee : OpenFunctionBuilder source target) :
    Nat → ContextContinuationBuilder source target points fid f tokens type
  | 0, _, _, _, _, _ => none
  | budget+1, path, cursor, label, pc, scope => do
    if let some span := contextFiniteSpan source target points fid f tokens type (budget+1) path cursor label pc scope then
      return span
    let sp ← sourceOpenFrontier source (budget+1) cursor
    let tp ← targetCallFrontier target f (budget+1) label pc scope
    let joined ← cutMatchUnder (checkContextCall source target points fid f tokens type callee
      (contextCallerSuffix source target points fid f tokens type callee budget)) (budget+1) path
      sp.val.normalizeTrace tp.val.normalizeTrace
    return ⟨fun σ holds context owner nextFrame stack observationType =>
      context_frontier_prefix source target points fid f tokens type cursor label pc scope
        sp.val tp.val sp.property tp.property path joined.down σ holds context owner nextFrame stack observationType⟩

end EvoIR.Core.Symbolic

