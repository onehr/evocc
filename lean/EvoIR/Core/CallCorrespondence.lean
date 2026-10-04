import EvoIR.Core.CallTargetDiagram

/-! Complete original source/IR pairing for the finite call diagrams. Both
programs and the original call closure are inputs; no interpreted sample or
post-inline source tree is substituted for an all-input execution proof. -/
namespace EvoIR.Core.Symbolic

def callInputSlots {Γ : Var → Nat} (args : List (CWord Γ)) : List (Slot Γ) :=
  args.map (fun word => .word word.word) ++ [.opaque 0, .opaque 1]

theorem call_input_values {Γ : Var → Nat} (σ : Subst Γ) (args : List (CWord Γ)) :
    (callInputSlots args).map (Slot.denote σ callTokens) =
      args.map (fun word => sourceValueIR (word.denote σ)) ++ [.memory ⟨1, []⟩, .world] := by
  simp [callInputSlots, List.map_append, List.map_map, callTokens, Slot.denote,
    Word.denote, CWord.word, CWord.denote, sourceValueIR]

structure CallPair (source : Source.Program) (target : IR.Program) where
  sourceFunction : Source.Function
  targetFunction : IR.Function
  selected : source.entries.contains sourceFunction.symbol = true
  sourceFound : source.functions.find? (·.symbol == sourceFunction.symbol) = some sourceFunction
  targetFound : target.functions.find? (·.name == targetFunction.name) = some targetFunction
  locals : LocalStore inputWidths
  storage : parameterStore sourceFunction.parameters (parameterWords sourceFunction.parameters) = some locals
  sourceTree : StateTree inputWidths
  sourceBody : ContextBody source locals sourceFunction.body sourceTree
  typed : sourceTree.HasType sourceFunction.returnType.type
  targetResult : ValueTree inputWidths
  targetDiagram : CallFunctionDiagram target targetFunction
    (callInputSlots (parameterWords sourceFunction.parameters)) targetResult
  sameResult : ∀ σ, targetResult.denote σ = (sourceTree.denote σ).2

def callPair (source : Source.Program) (target : IR.Program) : Option (CallPair source target) := do
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
    let locals : {ls // parameterStore sf.val.parameters (parameterWords sf.val.parameters) = some ls} ←
      match stored : parameterStore sf.val.parameters (parameterWords sf.val.parameters) with
      | none => none
      | some ls => some ⟨ls, stored⟩
    let body ← callBody source 512 locals.val sf.val.body
    if typed : body.val.Typed sf.val.returnType.type = true then
      let targetBody ← callTargetFunction target 512 tf.val sf.val.returnType.type
        (callInputSlots (parameterWords sf.val.parameters))
      if same : targetBody.val.normalizeTrace = body.val.erase.normalizeTrace then
        have id : sf.val.symbol = entry := by simpa using List.find?_some sf.property
        have name : tf.val.name = sf.val.name := by simpa using List.find?_some tf.property
        return {
          sourceFunction := sf.val, targetFunction := tf.val, selected := selected
          sourceFound := by simpa only [id] using sf.property
          targetFound := by simpa only [name] using tf.property
          locals := locals.val, storage := locals.property, sourceTree := body.val
          sourceBody := body.property, typed := StateTree.typed_sound typed
          targetResult := targetBody.val, targetDiagram := targetBody.property
          sameResult := by
            intro σ
            simpa only [StateTree.erase_sound]
              using normalized_traces_equal targetBody.val body.val.erase same σ }
      else none
    else none
  else none

theorem CallPair.refines {source : Source.Program} {target : IR.Program}
    (pair : CallPair source target) (args : List Source.Value)
    (typed : args.map Source.Value.type = pair.sourceFunction.parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      Source.initial source pair.sourceFunction.symbol args = .ok sourceEntry ∧
      IR.initial target pair.targetFunction.name
        (args.map sourceValueIR ++ [.memory ⟨1, []⟩, .world]) = .ok targetEntry ∧
      LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry := by
  let σ := inputSubst args
  have coverage := parameter_words_cover pair.sourceFunction.parameters args typed
  have stored := parameter_store_sound σ pair.sourceFunction.parameters
    (parameterWords pair.sourceFunction.parameters) pair.locals pair.storage
  rw [coverage] at stored
  let se : Source.State := ⟨.statement pair.sourceFunction.body, pair.locals.denote σ,
    [.function none pair.sourceFunction.returnType.type], pair.sourceFunction.symbol⟩
  have initial : Source.initial source pair.sourceFunction.symbol args = .ok se := by
    have chosen : pair.sourceFunction.symbol ∈ source.entries := by simpa using pair.selected
    simp [Source.initial, chosen, pair.sourceFound, stored, Bind.bind, Except.bind,
      pure, Except.pure, se]
  obtain ⟨te, tf, targetEntry, targetPath, targetOutcome⟩ :=
    pair.targetDiagram.completes σ pair.targetFound
  have targetArgs : (callInputSlots (parameterWords pair.sourceFunction.parameters)).map
      (Slot.denote σ callTokens) = args.map sourceValueIR ++ [.memory ⟨1, []⟩, .world] := by
    rw [call_input_values]
    have covered := congrArg (List.map sourceValueIR) coverage
    simpa only [List.map_map, Function.comp_def] using
      congrArg (fun values => values ++ [.memory ⟨1, []⟩, .world]) covered
  rw [targetArgs] at targetEntry
  refine ⟨se, te, initial, targetEntry, ?_⟩
  rw [pair.sameResult] at targetOutcome
  have sourcePath := pair.sourceBody σ pair.sourceFunction.symbol
    [.function none pair.sourceFunction.returnType.type]
  cases result : (pair.sourceTree.denote σ).2 with
  | error fault =>
    simp only [result, Except.map] at sourcePath targetOutcome
    obtain ⟨failedLocals, failedK, failedFid, path⟩ := sourcePath
    obtain ⟨code, rfl, observed⟩ := pair.sourceTree.erase.fault_observation σ fault
      (by simpa only [StateTree.erase_sound] using result)
    apply Transition.refines_of_terminals path.toReaches targetPath.toReaches
      (by rfl) (by simp [IR.step, targetOutcome]) (o := Observation.diagnostic code)
    · simpa [observeSource] using observed
    · simpa [observeTarget, targetOutcome] using observed
  | ok value =>
    have valueTyped := pair.typed σ value result
    simp only [result, Except.map] at sourcePath targetOutcome
    let sf : Source.State := ⟨.halted (.ok value), [], [], pair.sourceFunction.symbol⟩
    have finish : Source.Steps source se sf := sourcePath.trans (.next (by
      simp [Source.Step, Source.step, Source.resumeFlow, valueTyped, sf]) (.refl _))
    apply Transition.refines_of_terminals finish.toReaches targetPath.toReaches
      (by rfl) (by simp [IR.step, targetOutcome])
      (o := Observation.returned pair.sourceFunction.returnType.type value.bits.toNat)
    · simp [observeSource, sf, valueTyped]
    · simp [observeTarget, targetOutcome, sourceValueIR, valueTyped]

end EvoIR.Core.Symbolic
