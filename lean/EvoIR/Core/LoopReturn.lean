import EvoIR.Core.CutpointInvariant

/-! Whole-function cyclic result checking. Cutpoint layouts are untrusted
proposals. Admission checks every span, every successor, and the actual ABI
entry before returning a proof for all typed inputs and unbounded executions. -/
namespace EvoIR.Core.Symbolic

structure LoopDiagram (source : Source.Function) (target : IR.Function) where
  env : CBindings inputWidths
  scope : Scope inputWidths
  locals : LocalStore inputWidths
  block : IR.Block
  points : List LayoutPoint
  entry : scalarEntry source target (parameterWords source.parameters) = some (env, scope)
  storage : parameterStore source.parameters (parameterWords source.parameters) = some locals
  head : target.blocks.head? = some block
  returns : target.returns = [.bits source.returnType.type.width]
  spans : SpanList points source.symbol target source.returnType.type (points.map LayoutPoint.joint)
  initialRelated : ∀ σ, CutpointRelated (points.map LayoutPoint.joint) source.symbol target
    ((⟨.statement source.body, locals, [.function none source.returnType.type]⟩ :
      SourceCursor inputWidths).denote σ source.symbol)
    (targetState σ (fun _ => .world) target block.name 0 1 2 scope)

def loopDiagram (source : Source.Function) (target : IR.Function)
    (layouts : List CutpointLayout) (budget : Nat) : Option (LoopDiagram source target) := do
  if source.name != target.name || source.internal != target.internal then none else
  if target.effects != ((IR.deriveEffects ⟨[target]⟩ 1).lookup target.name).getD
      IR.Reader.emptyEffects then none else
  let entry : {pair // scalarEntry source target (parameterWords source.parameters) = some pair} ←
    match h : scalarEntry source target (parameterWords source.parameters) with
    | none => none
    | some pair => some ⟨pair, h⟩
  let block : {b // target.blocks.head? = some b} ←
    match h : target.blocks.head? with
    | none => none
    | some b => some ⟨b, h⟩
  let locals : {ls // parameterStore source.parameters (parameterWords source.parameters) = some ls} ←
    match h : parameterStore source.parameters (parameterWords source.parameters) with
    | none => none
    | some ls => some ⟨ls, h⟩
  let points ← layouts.mapM layoutPoint
  if returnsEq : target.returns = [.bits source.returnType.type.width] then
    let spans ← checkedSpans points source.symbol target source.returnType.type budget
      (points.map LayoutPoint.joint)
    let joined ← checkExit points source.symbol target source.returnType.type
      ⟨.statement source.body, locals.val, [.function none source.returnType.type]⟩
      (.next block.val.name entry.val.2)
    return {
      env := entry.val.1, scope := entry.val.2, locals := locals.val, block := block.val,
      points, entry := entry.property, storage := locals.property, head := block.property,
      returns := returnsEq, spans,
      initialRelated := by cases joined.down with | successor closed => exact closed }
  else none

theorem LoopDiagram.refines (σ : Subst inputWidths)
    {source : Source.Function} {target : IR.Function} (d : LoopDiagram source target)
    (s : Source.Program) (t : IR.Program)
    (selected : s.entries.contains source.symbol = true)
    (sourceFound : s.functions.find? (·.symbol == source.symbol) = some source)
    (targetFound : t.functions.find? (·.name == target.name) = some target) :
    ∃ sourceEntry targetEntry,
      Source.initial s source.symbol ((parameterWords source.parameters).map (CWord.denote σ)) =
        .ok sourceEntry ∧
      IR.initial t target.name ((parameterWords source.parameters).map
        (fun v => sourceValueIR (v.denote σ))) = .ok targetEntry ∧
      LoweringRefines s t source.returnType.type sourceEntry targetEntry := by
  obtain ⟨locals, block, boundSource, related, headBlock, boundTarget⟩ :=
    scalar_entry_sound σ source target (parameterWords source.parameters) d.env d.scope 1 d.entry
  have sameBlock : block = d.block := Option.some.inj (headBlock.symm.trans d.head)
  subst block
  have stored := parameter_store_sound σ source.parameters (parameterWords source.parameters)
    d.locals d.storage
  have localsEq : locals = d.locals.denote σ := Except.ok.inj (boundSource.symm.trans stored)
  let se : Source.State :=
    ⟨.statement source.body, locals, [.function none source.returnType.type], source.symbol⟩
  let te := targetState σ (fun _ => .world) target d.block.name 0 1 2 d.scope
  refine ⟨se, te, ?_, ?_, ?_⟩
  · have chosen : source.symbol ∈ s.entries := by simpa using selected
    simp [Source.initial, chosen, sourceFound, boundSource, bind, Except.bind,
      pure, Except.pure, se]
  · simp [IR.initial, targetFound, boundTarget, bind, Except.bind, pure, Except.pure, te, targetState]
  apply checked_table_refines d.points source.symbol source.returnType.type s t target
    targetFound d.returns d.spans se te
  simpa only [se, te, localsEq, SourceCursor.denote, CutControl.denote] using d.initialRelated σ

structure LoopPair (source : Source.Program) (target : IR.Program) where
  sourceFunction : Source.Function
  targetFunction : IR.Function
  selected : source.entries.contains sourceFunction.symbol = true
  sourceFound : source.functions.find? (·.symbol == sourceFunction.symbol) = some sourceFunction
  targetFound : target.functions.find? (·.name == targetFunction.name) = some targetFunction
  diagram : LoopDiagram sourceFunction targetFunction

def loopPair (source : Source.Program) (target : IR.Program)
    (layouts : List CutpointLayout) (budget : Nat) : Option (LoopPair source target) := do
  if !(Source.checkWellFormed source).isOk || !(IR.checkProgram target).isOk then none else
  match hs : source.functions with
  | [sf] =>
    if chosen : source.entries = [sf.symbol] then
      let tf : {f // target.functions.find? (·.name == sf.name) = some f} ←
        match found : target.functions.find? (·.name == sf.name) with
        | none => none
        | some f => some ⟨f, found⟩
      have name : tf.val.name = sf.name := by simpa using List.find?_some tf.property
      let d ← loopDiagram sf tf.val layouts budget
      return {
        sourceFunction := sf, targetFunction := tf.val,
        selected := by simp [chosen], sourceFound := by simp [hs],
        targetFound := by simpa only [name] using tf.property, diagram := d }
    else none
  | _ => none

theorem LoopPair.refines {source : Source.Program} {target : IR.Program}
    (pair : LoopPair source target) (args : List Source.Value)
    (typed : args.map Source.Value.type =
      pair.sourceFunction.parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      Source.initial source pair.sourceFunction.symbol args = .ok sourceEntry ∧
      IR.initial target pair.targetFunction.name (args.map sourceValueIR) = .ok targetEntry ∧
      LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry := by
  have coverage := parameter_words_cover pair.sourceFunction.parameters args typed
  have path := pair.diagram.refines (inputSubst args) source target
    pair.selected pair.sourceFound pair.targetFound
  have targetArgs : (parameterWords pair.sourceFunction.parameters).map
      (fun v => sourceValueIR (v.denote (inputSubst args))) = args.map sourceValueIR := by
    simpa only [List.map_map, Function.comp_def] using congrArg (List.map sourceValueIR) coverage
  simpa only [coverage, targetArgs] using path

end EvoIR.Core.Symbolic
