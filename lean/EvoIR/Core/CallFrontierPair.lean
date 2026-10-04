import EvoIR.Core.SourceFrontier
import EvoIR.Core.TargetFrontier
import EvoIR.Core.CallingLoopContext

/-! Whole-entry composition through a checked cyclic call. Callee proposals
are untrusted, function-scoped inputs. Matching a caller frontier never stands
in for checking the callee's own entry, spans and backedges. -/
namespace EvoIR.Core.Symbolic

abbrev CalleeProposals := Nat → Option (Nat × List CutpointLayout)

def CallFrontierAgreement (source : Source.Program) (target : IR.Program)
    (fid : Nat) (f : IR.Function) (type : Source.IntType)
    (s : SourceCursor inputWidths) (t : TargetCallSite inputWidths) : Prop :=
  ∀ σ owner nextFrame, LoweringRefines source target type (s.denote σ fid)
    (t.denote σ f owner nextFrame [])

def checkCallFrontier (source : Source.Program) (target : IR.Program)
    (fid : Nat) (f : IR.Function) (tokens : Bool) (type : Source.IntType)
    (proposals : CalleeProposals) (budget : Nat)
    (s : SourceCursor inputWidths) (t : TargetCallSite inputWidths) :
    Option (PLift (CallFrontierAgreement source target fid f type s t)) := do
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
        let (fuel, layouts) ← proposals sf.val.symbol
        let diagram ← loopDiagram source target sf.val tf.val layouts fuel
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
            if operands : inst.val.operands.mapM (fun id => t.scope.lookup id) =
                some (loopInputSlots diagram.tokens args) then
              if results : inst.val.results.map IR.Binding.type = tf.val.returns then
                let resumed ← checkedReturn source target fid f tokens type sf.val.returnType.type
                  locals frames b.val.name (t.pc+1) t.scope inst.val.results diagram.tokens budget
                return ⟨by
                  rw [sourceCursor]
                  intro σ owner nextFrame
                  have typed : (args.map (CWord.denote σ)).map Source.Value.type =
                      sf.val.parameters.map (fun p => p.type.type) := by
                    simpa only [List.map_map, Function.comp_def, CWord.denote] using argsTyped
                  have coverage := parameter_words_cover sf.val.parameters (args.map (CWord.denote σ)) typed
                  have actualArgs := slots_lookup_sound σ callTokens t.scope inst.val.operands
                    (loopInputSlots diagram.tokens args) operands
                  rw [loop_input_values] at actualArgs
                  have proof := diagram.call_checked_return (inputSubst (args.map (CWord.denote σ))) σ
                    source target type fid locals frames t.scope owner nextFrame t.pc f b.val inst.val tokens
                    (by simpa only [sourceId] using sf.property) function
                    (by simpa only [blockName] using b.property) inst.property call
                    (by simpa only [targetName] using tf.property)
                    (by simpa only [coverage] using actualArgs) results resumed
                  simpa only [CallFrontierAgreement, SourceCursor.denote, CutControl.denote,
                    TargetCallSite.denote, coverage, returnTyped, sourceId, blockName] using proof⟩
              else none
            else none
          else none
        else none
      else none
    else none
  | _ => none

theorem frontier_prefix_refines (source : Source.Program) (target : IR.Program)
    (fid : Nat) (f : IR.Function) (type : Source.IntType)
    (cursor : SourceCursor inputWidths) (label : String) (pc : Nat) (scope : Scope inputWidths)
    (sourceTree : CutTree inputWidths (SourceCursor inputWidths))
    (targetTree : CutTree inputWidths (TargetCallSite inputWidths))
    (sourcePrefix : SourceCutRuns source false cursor sourceTree)
    (targetPrefix : TargetFrontierRuns target f label pc scope targetTree)
    (matched : CutMatch (CallFrontierAgreement source target fid f type)
      sourceTree.normalizeTrace targetTree.normalizeTrace)
    (σ : Subst inputWidths) (owner nextFrame : Nat) :
    LoweringRefines source target type (cursor.denote σ fid)
      (cutTargetState σ f label pc owner nextFrame scope []) := by
  obtain ⟨sf, sp, _, sourceOutcome⟩ := sourcePrefix σ fid none
  obtain ⟨tf, tp, targetOutcome⟩ := targetPrefix σ owner nextFrame []
  have agreement := matched.sound σ
  rw [CutTree.normalize_trace_sound, CutTree.normalize_trace_sound] at agreement
  rcases agreement with ⟨fault, sourceFailed, targetFailed⟩ |
    ⟨se, te, sourceSuccess, targetSuccess, continued⟩
  · have sh : sf.control = .halted (.error fault) := by simpa [sourceFailed] using sourceOutcome
    have th : tf.control = .halted (.error fault) := by simpa [targetFailed] using targetOutcome
    obtain ⟨code, rfl, observed⟩ := sourceTree.fault_observation σ fault sourceFailed
    apply Transition.refines_of_terminals sp.toReaches tp.toReaches
      (by simp [Source.step, sh]) (by simp [IR.step, th]) (o := Observation.diagnostic code)
    · simpa [observeSource, sh] using observed
    · simpa [observeTarget, th] using observed
  · have sh : sf = se.denote σ fid := by simpa [sourceSuccess] using sourceOutcome
    have th : tf = te.denote σ f owner nextFrame [] := by simpa [targetSuccess] using targetOutcome
    subst sf
    subst tf
    exact (continued σ owner nextFrame).prepend sp.toReaches tp.toReaches

structure CallFrontierPair (source : Source.Program) (target : IR.Program) where
  sourceFunction : Source.Function
  targetFunction : IR.Function
  tokens : Bool
  checked : ∀ args : List Source.Value,
    args.map Source.Value.type = sourceFunction.parameters.map (fun p => p.type.type) →
    ∃ sourceEntry targetEntry,
      Source.initial source sourceFunction.symbol args = .ok sourceEntry ∧
      IR.initial target targetFunction.name (loopInputValues tokens args) = .ok targetEntry ∧
      LoweringRefines source target sourceFunction.returnType.type sourceEntry targetEntry

def callFrontierPair (source : Source.Program) (target : IR.Program)
    (proposals : CalleeProposals) (budget : Nat) : Option (CallFrontierPair source target) := do
  if !(Source.checkWellFormed source).isOk || !(IR.checkProgram target).isOk then none else
  let [entry] := source.entries | none
  let sf : {f // source.functions.find? (·.symbol == entry) = some f} ←
    match found : source.functions.find? (·.symbol == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  if selected : source.entries.contains sf.val.symbol = true then
    let tf : {f // target.functions.find? (·.name == sf.val.name) = some f} ←
      match found : target.functions.find? (·.name == sf.val.name) with
      | none => none
      | some f => some ⟨f, found⟩
    if sf.val.internal != tf.val.internal then none else
    if tf.val.effects != ((IR.deriveEffects target (target.functions.length+1)).lookup tf.val.name).getD
        IR.Reader.emptyEffects then none else
    let block : {b // tf.val.blocks.head? = some b} ←
      match found : tf.val.blocks.head? with
      | none => none
      | some b => some ⟨b, found⟩
    let locals : {ls // parameterStore sf.val.parameters (parameterWords sf.val.parameters) = some ls} ←
      match stored : parameterStore sf.val.parameters (parameterWords sf.val.parameters) with
      | none => none
      | some ls => some ⟨ls, stored⟩
    let tokens := tf.val.returns != [.bits sf.val.returnType.type.width]
    if tf.val.returns != cutReturnTypes tokens sf.val.returnType.type then none else
    if signature : tf.val.parameters =
        (loopInputSlots tokens (parameterWords sf.val.parameters)).map (Slot.type callTokenTypes) then
      if parameters : block.val.parameters.map IR.Binding.type = tf.val.parameters then
        let scope : {scope // bindSlots callTokenTypes block.val.parameters
            (loopInputSlots tokens (parameterWords sf.val.parameters)) = some scope} ←
          match bound : bindSlots callTokenTypes block.val.parameters
              (loopInputSlots tokens (parameterWords sf.val.parameters)) with
          | none => none
          | some scope => some ⟨scope, bound⟩
        let cursor : SourceCursor inputWidths :=
          ⟨.statement sf.val.body, locals.val, [.function none sf.val.returnType.type]⟩
        let sp ← sourceCallFrontier source budget cursor
        let tp ← targetCallFrontier target tf.val budget block.val.name 0 scope.val
        let joined ← cutMatch (checkCallFrontier source target sf.val.symbol tf.val tokens
          sf.val.returnType.type proposals budget) sp.val.normalizeTrace tp.val.normalizeTrace
        return {
          sourceFunction := sf.val, targetFunction := tf.val, tokens := tokens
          checked := by
            intro args typed
            let σ := inputSubst args
            have coverage := parameter_words_cover sf.val.parameters args typed
            have stored := parameter_store_sound σ sf.val.parameters (parameterWords sf.val.parameters)
              locals.val locals.property
            have binding := bind_slots_sound σ callTokens callTokenTypes (by intro id; rfl) _ _ _ scope.property
            have types : ((loopInputSlots tokens (parameterWords sf.val.parameters)).map
                (Slot.denote σ callTokens)).map IR.Value.type = tf.val.parameters := by
              rw [signature, List.map_map]
              apply List.map_congr_left
              intro slot _
              exact slot_type_sound σ callTokens callTokenTypes (by intro id; rfl) slot
            have entered : IR.enter tf.val (loopInputValues tokens args) 1 =
                .ok ⟨tf.val.name, block.val.name, 0, scope.val.denote σ callTokens, 1⟩ := by
              have actual := loop_input_values σ tokens (parameterWords sf.val.parameters)
              rw [coverage] at actual
              rw [← actual]
              simp [IR.enter, types, block.property, parameters, binding, Bind.bind, Except.bind, pure, Except.pure]
            have sourceId : sf.val.symbol = entry := by simpa using List.find?_some sf.property
            have targetName : tf.val.name = sf.val.name := by simpa using List.find?_some tf.property
            have sourceFound : source.functions.find? (·.symbol == sf.val.symbol) = some sf.val := by
              simpa only [sourceId] using sf.property
            have targetFound : target.functions.find? (·.name == tf.val.name) = some tf.val := by
              simpa only [targetName] using tf.property
            have chosen : sf.val.symbol ∈ source.entries := by simpa using selected
            rw [coverage] at stored
            refine ⟨cursor.denote σ sf.val.symbol, cutTargetState σ tf.val block.val.name 0 1 2 scope.val [], ?_, ?_, ?_⟩
            · simp [Source.initial, chosen, sourceFound, stored, Bind.bind, Except.bind,
                pure, Except.pure, cursor, SourceCursor.denote, CutControl.denote]
            · simp [IR.initial, targetFound, entered, Bind.bind, Except.bind, pure, Except.pure,
                cutTargetState, targetState]
            · exact frontier_prefix_refines source target sf.val.symbol tf.val sf.val.returnType.type
                cursor block.val.name 0 scope.val sp.val tp.val sp.property tp.property joined.down σ 1 2 }
      else none
    else none
  else none

theorem CallFrontierPair.refines {source : Source.Program} {target : IR.Program}
    (pair : CallFrontierPair source target) (args : List Source.Value)
    (typed : args.map Source.Value.type = pair.sourceFunction.parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      Source.initial source pair.sourceFunction.symbol args = .ok sourceEntry ∧
      IR.initial target pair.targetFunction.name (loopInputValues pair.tokens args) = .ok targetEntry ∧
      LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry :=
  pair.checked args typed

end EvoIR.Core.Symbolic
