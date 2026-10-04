import EvoIR.Core.BranchTarget
import EvoIR.Core.StateEntry

/-! Whole-function source/target checking for branching return expressions.
This extends the structural leaf case to actual conditional CFGs; it does not
replace V01's remaining loop, mutable-storage and direct-call obligations. -/
namespace EvoIR.Core.Source

def ReturnsState (p : Program) (locals : Locals) (fid : Nat) (body : Stmt)
    (outcome : StateOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok value => Steps p ⟨.statement body, locals, k, fid⟩
      ⟨.flow (.returned value), outcome.1, k, fid⟩
  | .error fault => ∃ failedK, Steps p ⟨.statement body, locals, k, fid⟩
      ⟨.halted (.error fault), outcome.1, failedK, fid⟩

theorem ReturnShape.stateful_sound {body : Stmt} {d : QualifiedType} {e : Expr}
    (shape : ReturnShape body d e) (p : Program) (locals : Locals) (fid : Nat)
    (outcome : StateOutcome) (evaluated : EvaluatesState p locals fid e outcome)
    (depth : ∀ v, outcome.2 = .ok v → outcome.1.length = locals.length) :
    ReturnsState p locals fid body (outcome.1, outcome.2.map (Value.cast d.type)) := by
  induction shape with
  | direct d e =>
    intro k
    have first : Step p ⟨.statement (.return d e), locals, k, fid⟩
        ⟨.expression e, locals, .return d :: k, fid⟩ := rfl
    cases result : outcome.2 with
    | error fault =>
      simp only [EvaluatesState, result] at evaluated
      obtain ⟨failedK, path⟩ := evaluated (.return d :: k)
      exact ⟨failedK, .next first path⟩
    | ok value =>
      simp only [EvaluatesState, result] at evaluated
      exact .next first ((evaluated (.return d :: k)).trans (.next rfl (.refl _)))
  | @block body d e inner ih =>
    intro k
    have first : Steps p ⟨.statement (.compound [.statement body]), locals, k, fid⟩
        ⟨.statement body, locals, .sequence [] :: .scope locals.length :: k, fid⟩ :=
      .next rfl (.next rfl (.refl _))
    have middle := ih evaluated
    cases result : outcome.2 with
    | error fault =>
      simp only [ReturnsState, result, Except.map] at middle
      obtain ⟨failedK, path⟩ := middle (.sequence [] :: .scope locals.length :: k)
      exact ⟨failedK, first.trans path⟩
    | ok value =>
      simp only [ReturnsState, result, Except.map] at middle
      exact first.trans ((middle (.sequence [] :: .scope locals.length :: k)).trans
        (.next rfl (.next (by simp [Step, step, resumeFlow, depth value result]) (.refl _))))

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

theorem ValueTree.fault_observation {Γ : Var → Nat} (σ : Subst Γ)
    (tree : ValueTree Γ) (fault : Source.Fault) (failed : tree.denote σ = .error fault) :
    ∃ code, fault = .diagnostic code ∧ checkedDiagnostic code = some (.diagnostic code) := by
  induction tree with
  | value word => simp [denote] at failed
  | checked guard next ih =>
    simp only [denote] at failed
    cases check : guards σ [guard] with
    | error f =>
      simp only [check, bind, Except.bind, Except.error.injEq] at failed
      subst f
      exact guards_fault_observation σ [guard] fault check
    | ok u =>
      simp only [check, bind, Except.bind] at failed
      exact ih failed
  | branch condition yes no first second =>
    simp only [denote] at failed
    split at failed
    · exact first failed
    · exact second failed

structure ReturnDiagram {Γ : Var → Nat} (source : Source.Function)
    (target : IR.Function) (args : List (CWord Γ)) where
  env : CBindings Γ
  scope : Scope Γ
  locals : LocalStore Γ
  block : IR.Block
  expr : Source.Expr
  tree : StateTree Γ
  entry : scalarEntry source target args = some (env, scope)
  storage : parameterStore source.parameters args = some locals
  head : target.blocks.head? = some block
  body : Source.ReturnShape source.body source.returnType expr
  expression : StateDiagram locals expr tree
  depth : tree.sameDepth locals.length = true
  returns : target.returns = [.bits source.returnType.type.width]
  targetDiagram : TargetDiagram (fun _ => .world) target source.returnType.type scope block.name
    (tree.erase.cast source.returnType.type)

def returnDiagram {Γ : Var → Nat} (source : Source.Function) (target : IR.Function)
    (args : List (CWord Γ)) : Option (ReturnDiagram source target args) := do
  if source.name != target.name || source.internal != target.internal then none else
  if target.effects != ((IR.deriveEffects ⟨[target]⟩ 1).lookup target.name).getD
      IR.Reader.emptyEffects then none else
  let entry : {pair : CBindings Γ × Scope Γ // scalarEntry source target args = some pair} ←
    match h : scalarEntry source target args with
    | none => none
    | some pair => some ⟨pair, h⟩
  let block : {b : IR.Block // target.blocks.head? = some b} ←
    match h : target.blocks.head? with
    | none => none
    | some b => some ⟨b, h⟩
  let ⟨destination, expr, body⟩ ← Source.returnShape source.body
  if destinationEq : destination = source.returnType then
    let locals : {ls // parameterStore source.parameters args = some ls} ←
      match stored : parameterStore source.parameters args with
      | none => none
      | some ls => some ⟨ls, stored⟩
    let s ← stateExpression (Source.expressionDepth expr) locals.val expr
    if depth : s.val.sameDepth locals.val.length = true then
    let t ← targetDiagram (fun _ => .world) target source.returnType.type
      (target.blocks.length+1) entry.val.2 block.val.name
    if returnsEq : target.returns = [.bits source.returnType.type.width] then
      if same : t.val = s.val.erase.cast source.returnType.type then
        return {
          env := entry.val.1, scope := entry.val.2, locals := locals.val,
          block := block.val, expr := expr, tree := s.val
          entry := entry.property, head := block.property, body := destinationEq ▸ body.property
          storage := locals.property, depth := depth, expression := s.property,
          returns := returnsEq, targetDiagram := same ▸ t.property }
      else none
    else none
    else none
  else none

theorem ReturnDiagram.refines {Γ : Var → Nat} (σ : Subst Γ)
    {source : Source.Function} {target : IR.Function} {args : List (CWord Γ)}
    (d : ReturnDiagram source target args) (s : Source.Program) (t : IR.Program)
    (selected : s.entries.contains source.symbol = true)
    (sourceFound : s.functions.find? (·.symbol == source.symbol) = some source)
    (targetFound : t.functions.find? (·.name == target.name) = some target) :
    ∃ sourceEntry targetEntry,
      Source.initial s source.symbol (args.map (CWord.denote σ)) = .ok sourceEntry ∧
      IR.initial t target.name (args.map (fun v => sourceValueIR (v.denote σ))) = .ok targetEntry ∧
      LoweringRefines s t source.returnType.type sourceEntry targetEntry := by
  obtain ⟨locals, block, boundSource, related, headBlock, boundTarget⟩ :=
    scalar_entry_sound σ source target args d.env d.scope 1 d.entry
  have sameBlock : block = d.block := Option.some.inj (headBlock.symm.trans d.head)
  subst block
  have stored := parameter_store_sound σ source.parameters args d.locals d.storage
  have localsEq : locals = d.locals.denote σ := Except.ok.inj (boundSource.symm.trans stored)
  let se : Source.State :=
    ⟨.statement source.body, locals, [.function none source.returnType.type], source.symbol⟩
  let te := targetState σ (fun _ => .world) target d.block.name 0 1 2 d.scope
  refine ⟨se, te, ?_, ?_, ?_⟩
  · have chosen : source.symbol ∈ s.entries := by simpa using selected
    simp [Source.initial, chosen, sourceFound, boundSource, bind, Except.bind,
      pure, Except.pure, se]
  · simp [IR.initial, targetFound, boundTarget, bind, Except.bind, pure, Except.pure, te, targetState]
  have sourcePath := d.body.stateful_sound s locals source.symbol (d.tree.denote σ)
    (by simpa only [localsEq] using d.expression σ s source.symbol)
    (by intro v success; simpa [localsEq, LocalStore.denote] using
      StateTree.same_depth_sound d.depth σ v success)
  obtain ⟨tf, targetPath, targetOutcome⟩ := d.targetDiagram.sound σ (fun _ => .world)
    (by intro id; rfl) t targetFound d.returns 1 2
  have targetResult : (d.tree.erase.cast source.returnType.type).irResult σ =
      (((d.tree.denote σ).2).map (Source.Value.cast source.returnType.type)).map
        (fun v => [sourceValueIR v]) := by
    rw [ValueTree.irResult, ValueTree.cast_sound, StateTree.erase_sound]
  rw [targetResult] at targetOutcome
  cases result : (d.tree.denote σ).2 with
  | error fault =>
    simp only [result, Except.map] at sourcePath targetOutcome
    obtain ⟨failedK, path⟩ := sourcePath [.function none source.returnType.type]
    obtain ⟨code, rfl, observed⟩ := d.tree.erase.fault_observation σ fault
      (by simpa only [StateTree.erase_sound] using result)
    apply Transition.refines_of_terminals path.toReaches targetPath.toReaches
      (by rfl) (by simp [IR.step, targetOutcome]) (o := Observation.diagnostic code)
    · simpa [observeSource] using observed
    · simpa [observeTarget, targetOutcome] using observed
  | ok raw =>
    let value := raw.cast source.returnType.type
    simp only [result, Except.map] at sourcePath targetOutcome
    let sf : Source.State := ⟨.halted (.ok value), [], [], source.symbol⟩
    have sourceFinish : Source.Steps s se sf :=
      (sourcePath [.function none source.returnType.type]).trans
        (.next (by simp [Source.Step, Source.step, Source.resumeFlow, value, sf,
          Source.Value.cast, Source.Value.ofInt]) (.refl _))
    apply Transition.refines_of_terminals sourceFinish.toReaches targetPath.toReaches
      (by rfl) (by simp [IR.step, targetOutcome])
      (o := Observation.returned source.returnType.type value.bits.toNat)
    · rfl
    · simp [observeTarget, targetOutcome, sourceValueIR, value, Source.Value.cast, Source.Value.ofInt]

structure ReturnPair (source : Source.Program) (target : IR.Program) where
  sourceFunction : Source.Function
  targetFunction : IR.Function
  selected : source.entries.contains sourceFunction.symbol = true
  sourceFound : source.functions.find? (·.symbol == sourceFunction.symbol) = some sourceFunction
  targetFound : target.functions.find? (·.name == targetFunction.name) = some targetFunction
  diagram : ReturnDiagram sourceFunction targetFunction (parameterWords sourceFunction.parameters)

def returnPair (source : Source.Program) (target : IR.Program) : Option (ReturnPair source target) := do
  if !(Source.checkWellFormed source).isOk || !(IR.checkProgram target).isOk then none else
  match hs : source.functions, ht : target.functions with
  | [sf], [tf] =>
    if chosen : source.entries = [sf.symbol] then
      let d ← returnDiagram sf tf (parameterWords sf.parameters)
      return {
        sourceFunction := sf, targetFunction := tf
        selected := by simp [chosen]
        sourceFound := by simp [hs]
        targetFound := by simp [ht]
        diagram := d }
    else none
  | _, _ => none

theorem ReturnPair.refines {source : Source.Program} {target : IR.Program}
    (pair : ReturnPair source target) (args : List Source.Value)
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
