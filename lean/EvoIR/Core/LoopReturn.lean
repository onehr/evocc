import EvoIR.Core.CutpointInvariant
import EvoIR.Core.CallCorrespondence

/-! Whole-function cyclic result checking. Cutpoint layouts are untrusted
proposals. Admission checks every span, every successor, and the actual ABI
entry before returning a proof for all typed inputs and unbounded executions. -/
namespace EvoIR.Core.Symbolic

def loopInputSlots {Γ : Var → Nat} (tokens : Bool) (args : List (CWord Γ)) : List (Slot Γ) :=
  if tokens then callInputSlots args else args.map (fun word => .word word.word)

def loopInputValues (tokens : Bool) (args : List Source.Value) : List IR.Value :=
  args.map sourceValueIR ++ if tokens then [.memory ⟨1, []⟩, .world] else []

theorem loop_input_values {Γ : Var → Nat} (σ : Subst Γ) (tokens : Bool) (args : List (CWord Γ)) :
    (loopInputSlots tokens args).map (Slot.denote σ callTokens) =
      loopInputValues tokens (args.map (CWord.denote σ)) := by
  cases tokens
  · simp [loopInputSlots, loopInputValues, List.map_map, Slot.denote,
      Word.denote, CWord.word, sourceValueIR, CWord.denote]
  · simpa [loopInputSlots, loopInputValues, List.map_map, Function.comp_def] using call_input_values σ args

structure LoopDiagram (program : Source.Program) (ir : IR.Program)
    (source : Source.Function) (target : IR.Function) where
  tokens : Bool
  scope : Scope inputWidths
  locals : LocalStore inputWidths
  block : IR.Block
  points : List LayoutPoint
  signature : target.parameters = (loopInputSlots tokens (parameterWords source.parameters)).map (Slot.type callTokenTypes)
  parameters : block.parameters.map IR.Binding.type = target.parameters
  binding : bindSlots callTokenTypes block.parameters (loopInputSlots tokens (parameterWords source.parameters)) = some scope
  storage : parameterStore source.parameters (parameterWords source.parameters) = some locals
  head : target.blocks.head? = some block
  returns : target.returns = cutReturnTypes tokens source.returnType.type
  spans : SpanList program ir points source.symbol target tokens source.returnType.type (points.map LayoutPoint.joint)
  initialRelated : ∀ σ context owner nextFrame stack,
    CutpointRelated (points.map LayoutPoint.joint) source.symbol target
    ((⟨.statement source.body, locals, [.function none source.returnType.type]⟩ :
      SourceCursor inputWidths).denote σ source.symbol context)
    (cutTargetState σ target block.name 0 owner nextFrame scope stack) context owner stack

def loopDiagram (program : Source.Program) (ir : IR.Program) (source : Source.Function) (target : IR.Function)
    (layouts : List CutpointLayout) (budget : Nat) : Option (LoopDiagram program ir source target) := do
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
        let spans ← checkedSpans program ir points source.symbol target tokens source.returnType.type budget
          (points.map LayoutPoint.joint)
        let joined ← checkExit points source.symbol target tokens source.returnType.type
          ⟨.statement source.body, locals.val, [.function none source.returnType.type]⟩
          (.next block.val.name scope.val)
        return {
          tokens, scope := scope.val, locals := locals.val, block := block.val,
          points, signature, parameters, binding := scope.property,
          storage := locals.property, head := block.property,
          returns := returnsEq, spans,
          initialRelated := by
            cases joined.down with
            | successor closed =>
              exact fun σ context owner nextFrame stack => closed σ nextFrame context owner stack }
      else none
    else none
  else none

theorem LoopDiagram.refines (σ : Subst inputWidths)
    {source : Source.Function} {target : IR.Function} (s : Source.Program) (t : IR.Program)
    (d : LoopDiagram s t source target)
    (selected : s.entries.contains source.symbol = true)
    (sourceFound : s.functions.find? (·.symbol == source.symbol) = some source)
    (targetFound : t.functions.find? (·.name == target.name) = some target) :
    ∃ sourceEntry targetEntry,
      Source.initial s source.symbol ((parameterWords source.parameters).map (CWord.denote σ)) =
        .ok sourceEntry ∧
      IR.initial t target.name (loopInputValues d.tokens
        ((parameterWords source.parameters).map (CWord.denote σ))) = .ok targetEntry ∧
      LoweringRefines s t source.returnType.type sourceEntry targetEntry := by
  have stored := parameter_store_sound σ source.parameters (parameterWords source.parameters)
    d.locals d.storage
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
  let te := targetState σ callTokens target d.block.name 0 1 2 d.scope
  refine ⟨se, te, ?_, ?_, ?_⟩
  · have chosen : source.symbol ∈ s.entries := by simpa using selected
    simp [Source.initial, chosen, sourceFound, stored, bind, Except.bind,
      pure, Except.pure, se]
  · simp [IR.initial, targetFound, boundTarget, bind, Except.bind, pure, Except.pure, te, targetState]
  apply checked_table_refines d.points source.symbol source.returnType.type s t target
    d.tokens d.spans se te
  simpa only [se, te, SourceCursor.denote, CutControl.denote, cutTargetState, targetState]
    using d.initialRelated σ none 1 2 []

structure LoopPair (source : Source.Program) (target : IR.Program) where
  sourceFunction : Source.Function
  targetFunction : IR.Function
  selected : source.entries.contains sourceFunction.symbol = true
  sourceFound : source.functions.find? (·.symbol == sourceFunction.symbol) = some sourceFunction
  targetFound : target.functions.find? (·.name == targetFunction.name) = some targetFunction
  diagram : LoopDiagram source target sourceFunction targetFunction

def loopPair (source : Source.Program) (target : IR.Program)
    (layouts : List CutpointLayout) (budget : Nat) : Option (LoopPair source target) := do
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
      let d ← loopDiagram source target sf.val tf.val layouts budget
      return {
        sourceFunction := sf.val, targetFunction := tf.val,
        selected := chosen, sourceFound := by simpa only [id] using sf.property,
        targetFound := by simpa only [name] using tf.property, diagram := d }
  else none

theorem LoopPair.refines {source : Source.Program} {target : IR.Program}
    (pair : LoopPair source target) (args : List Source.Value)
    (typed : args.map Source.Value.type =
      pair.sourceFunction.parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      Source.initial source pair.sourceFunction.symbol args = .ok sourceEntry ∧
      IR.initial target pair.targetFunction.name (loopInputValues pair.diagram.tokens args) = .ok targetEntry ∧
      LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry := by
  have coverage := parameter_words_cover pair.sourceFunction.parameters args typed
  have path := LoopDiagram.refines (inputSubst args) source target pair.diagram
    pair.selected pair.sourceFound pair.targetFound
  simpa only [coverage] using path

end EvoIR.Core.Symbolic
