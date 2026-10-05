import EvoIR.Core.ContextCallSpan

/-! One body builder for the entire admitted nonrecursive call closure.
Construction depth bounds inspection of the finite call graph, never program
iterations. Every function is checked with arbitrary caller contexts. -/
namespace EvoIR.Core.Symbolic

def contextCallerTable (source : Source.Program) (target : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (f : IR.Function) (tokens : Bool) (type : Source.IntType)
    (callee : OpenFunctionBuilder source target) (budget : Nat) :
    (todo : List (JointPoint inputWidths)) → Option (PLift (∀ point ∈ todo,
      ∀ σ context owner frame stack observationType,
        ContextOpen source target points fid f type observationType tokens context owner stack
          (point.source.denote σ fid context) (point.targetState σ f frame owner stack)))
  | [] => some ⟨by intro point member; cases member⟩
  | point :: rest => do
    let head ← contextCallerSuffix source target points fid f tokens type callee budget
      [] point.source point.label 0 point.scope
    let tail ← contextCallerTable source target points fid f tokens type callee budget rest
    return ⟨by
      intro selected member σ context owner frame stack observationType
      rcases List.mem_cons.mp member with rfl | member
      · exact head.down σ trivial context owner frame stack observationType
      · exact tail.down selected member σ context owner frame stack observationType⟩

def openFunctionDiagramWith (program : Source.Program) (ir : IR.Program)
    (source : Source.Function) (target : IR.Function) (layouts : List CutpointLayout) (budget : Nat)
    (callee : OpenFunctionBuilder program ir) : Option (OpenFunctionDiagram program ir source target) := do
  if source.name != target.name || source.internal != target.internal then none else
  if target.effects != ((IR.deriveEffects ir (ir.functions.length+1)).lookup target.name).getD
      IR.Reader.emptyEffects then none else
  let block : {b // target.blocks.head? = some b} ←
    match h : target.blocks.head? with
    | none => none
    | some b => some ⟨b, h⟩
  let locals : {ls // parameterStore source.parameters (parameterWords source.parameters) = some ls} ←
    match h : parameterStore source.parameters (parameterWords source.parameters) with
    | none => none
    | some ls => some ⟨ls, h⟩
  let points ← layouts.mapM layoutPoint
  let tokens := target.returns != [.bits source.returnType.type.width]
  if returnsEq : target.returns = cutReturnTypes tokens source.returnType.type then
    if signature : target.parameters = (loopInputSlots tokens (parameterWords source.parameters)).map (Slot.type callTokenTypes) then
      if parameters : block.val.parameters.map IR.Binding.type = target.parameters then
        let scope : {scope // bindSlots callTokenTypes block.val.parameters
            (loopInputSlots tokens (parameterWords source.parameters)) = some scope} ←
          match h : bindSlots callTokenTypes block.val.parameters (loopInputSlots tokens (parameterWords source.parameters)) with
          | none => none
          | some scope => some ⟨scope, h⟩
        let spans ← contextCallerTable program ir points source.symbol target tokens source.returnType.type
          callee budget (points.map LayoutPoint.joint)
        let joined ← checkExit points source.symbol target tokens source.returnType.type
          ⟨.statement source.body, locals.val, [.function none source.returnType.type]⟩ (.next block.val.name scope.val)
        return {
          tokens, scope := scope.val, locals := locals.val, block := block.val,
          signature, parameters, binding := scope.property, storage := locals.property,
          head := block.property, returns := returnsEq,
          body := by
            intro σ context owner frame stack observationType
            cases joined.down with
            | successor entry =>
              apply Transition.OpenCheckpoint.closeWithin
                (boundary := fun a b => CutpointRelated (points.map LayoutPoint.joint) source.symbol target a b context owner stack)
                (nextBoundary := FunctionReturn source.symbol source.returnType.type tokens context stack)
                _ (entry σ frame context owner stack)
              rintro a b ⟨point, member, ρ, nextFrame, rfl, rfl⟩
              exact spans.down point member ρ context owner nextFrame stack observationType }
      else none
    else none
  else none

def openFunctionClosure (source : Source.Program) (target : IR.Program)
    (proposals : CalleeProposals) (budget : Nat) : Nat → OpenFunctionBuilder source target
  | 0, _, _ => none
  | depth+1, sf, tf => do
    let (fuel, layouts) ← match proposals sf.symbol with
      | some plan => some plan
      | none => (entryCutpoint sf tf).map fun layout => (budget, [layout])
    openFunctionDiagramWith source target sf tf layouts fuel (openFunctionClosure source target proposals budget depth)

theorem OpenFunctionDiagram.refines (σ : Subst inputWidths)
    {source : Source.Function} {target : IR.Function} (s : Source.Program) (t : IR.Program)
    (d : OpenFunctionDiagram s t source target)
    (selected : s.entries.contains source.symbol = true)
    (sourceFound : s.functions.find? (·.symbol == source.symbol) = some source)
    (targetFound : t.functions.find? (·.name == target.name) = some target) :
    ∃ sourceEntry targetEntry,
      Source.initial s source.symbol ((parameterWords source.parameters).map (CWord.denote σ)) = .ok sourceEntry ∧
      IR.initial t target.name (loopInputValues d.tokens
        ((parameterWords source.parameters).map (CWord.denote σ))) = .ok targetEntry ∧
      LoweringRefines s t source.returnType.type sourceEntry targetEntry := by
  have stored := parameter_store_sound σ source.parameters (parameterWords source.parameters) d.locals d.storage
  have binding := bind_slots_sound σ callTokens callTokenTypes (by intro id; rfl) _ _ _ d.binding
  have types : ((loopInputSlots d.tokens (parameterWords source.parameters)).map
      (Slot.denote σ callTokens)).map IR.Value.type = target.parameters := by
    rw [d.signature, List.map_map]
    apply List.map_congr_left
    intro slot _
    exact slot_type_sound σ callTokens callTokenTypes (by intro id; rfl) slot
  have boundTarget : IR.enter target (loopInputValues d.tokens
      ((parameterWords source.parameters).map (CWord.denote σ))) 1 =
      .ok ⟨target.name, d.block.name, 0, d.scope.denote σ callTokens, 1⟩ := by
    rw [← loop_input_values]
    simp [IR.enter, types, d.head, d.parameters, binding, Bind.bind, Except.bind, pure, Except.pure]
  let se : Source.State :=
    ⟨.statement source.body, d.locals.denote σ, [.function none source.returnType.type], source.symbol⟩
  let te := cutTargetState σ target d.block.name 0 1 2 d.scope []
  refine ⟨se, te, ?_, ?_, ?_⟩
  · have chosen : source.symbol ∈ s.entries := by simpa using selected
    simp [Source.initial, chosen, sourceFound, stored, bind, Except.bind, pure, Except.pure, se]
  · simp [IR.initial, targetFound, boundTarget, bind, Except.bind, pure, Except.pure, te, cutTargetState, targetState]
  have body := d.body σ none 1 2 [] source.returnType.type
  apply body.finish
  rintro a b ⟨value, typed, locals, frame, rfl, rfl⟩
  apply Transition.refines_of_terminals (.refl _) (.refl _) rfl rfl
    (o := Observation.returned source.returnType.type value.bits.toNat)
  · simp [observeSource, returnedSource, typed]
  · cases tokens : d.tokens <;>
      simp [observeTarget, cutResume, valueIRs, sourceValueIR, typed]

def openCallPair (source : Source.Program) (target : IR.Program)
    (proposals : CalleeProposals) (budget : Nat) : Option (CallFrontierPair source target) := do
  if !(Source.checkWellFormed source).isOk || !(IR.checkProgram target).isOk then none else
  let [entry] := source.entries | none
  let sf : {f // source.functions.find? (·.symbol == entry) = some f} ←
    match found : source.functions.find? (·.symbol == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  if chosen : source.entries.contains sf.val.symbol = true then
    let tf : {f // target.functions.find? (·.name == sf.val.name) = some f} ←
      match found : target.functions.find? (·.name == sf.val.name) with
      | none => none
      | some f => some ⟨f, found⟩
    have name : tf.val.name = sf.val.name := by simpa using List.find?_some tf.property
    have id : sf.val.symbol = entry := by simpa using List.find?_some sf.property
    let d ← openFunctionClosure source target proposals budget (source.functions.length+1) sf.val tf.val
    return {
      sourceFunction := sf.val, targetFunction := tf.val, tokens := d.tokens,
      checked := by
        intro args typed
        have coverage := parameter_words_cover sf.val.parameters args typed
        have proof := d.refines (inputSubst args) source target chosen
          (by simpa only [id] using sf.property) (by simpa only [name] using tf.property)
        simpa only [coverage] using proof }
  else none

end EvoIR.Core.Symbolic
