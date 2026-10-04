import EvoIR.Core.BranchTarget
import EvoIR.Core.StatementDiagram
import EvoIR.Core.ConstantBranch

/-! Whole-function source/target checking for finite statements and conditional
CFGs. Cyclic control and direct calls remain separate V01 obligations. -/
namespace EvoIR.Core.Source

def ReturnsState (p : Program) (locals : Locals) (fid : Nat) (body : Stmt)
    (outcome : StateOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok value => Steps p ⟨.statement body, locals, k, fid⟩
      ⟨.flow (.returned value), outcome.1, k, fid⟩
  | .error fault => ∃ failedK, Steps p ⟨.statement body, locals, k, fid⟩
      ⟨.halted (.error fault), outcome.1, failedK, fid⟩

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
  tree : StateTree Γ
  entry : scalarEntry source target args = some (env, scope)
  storage : parameterStore source.parameters args = some locals
  head : target.blocks.head? = some block
  body : ∀ σ p fid, Source.ReturnsState p (locals.denote σ) fid source.body (tree.denote σ)
  typed : tree.HasType source.returnType.type
  returns : target.returns = [.bits source.returnType.type.width]
  targetTree : ValueTree Γ
  sameResult : ∀ σ, targetTree.denote σ = tree.erase.denote σ
  targetDiagram : TargetDiagram (fun _ => .world) target source.returnType.type scope block.name
    targetTree

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
  let locals : {ls // parameterStore source.parameters args = some ls} ←
    match stored : parameterStore source.parameters args with
    | none => none
    | some ls => some ⟨ls, stored⟩
  let body ← flowDiagram (Source.statementDepth source.body) locals.val (.statement source.body)
  let s ← body.val.returns?
  if typed : s.val.Typed source.returnType.type = true then
    let t ← targetDiagram (fun _ => .world) target source.returnType.type
      (target.blocks.length+1) entry.val.2 block.val.name
    if returnsEq : target.returns = [.bits source.returnType.type.width] then
      if same : t.val.normalizeBranches = s.val.erase.normalizeBranches then
        return {
          env := entry.val.1, scope := entry.val.2, locals := locals.val,
          block := block.val, tree := s.val
          entry := entry.property, head := block.property,
          body := by
            intro σ p fid k
            have path := body.property σ p fid k
            rw [s.property.sound σ] at path
            cases outcome : (s.val.denote σ).2 <;>
              simpa [Source.ReturnsState, outcome, Except.map] using path
          storage := locals.property, typed := StateTree.typed_sound typed,
          returns := returnsEq, targetTree := t.val,
          sameResult := normalized_trees_equal t.val s.val.erase same,
          targetDiagram := t.property }
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
  have sourcePath : Source.ReturnsState s locals source.symbol source.body (d.tree.denote σ) :=
    by simpa only [localsEq] using d.body σ s source.symbol
  obtain ⟨tf, targetPath, targetOutcome⟩ := d.targetDiagram.sound σ (fun _ => .world)
    (by intro id; rfl) t targetFound d.returns 1 2
  have targetResult : d.targetTree.irResult σ =
      ((d.tree.denote σ).2).map
        (fun v => [sourceValueIR v]) := by
    rw [ValueTree.irResult, d.sameResult, StateTree.erase_sound]
  rw [targetResult] at targetOutcome
  cases result : (d.tree.denote σ).2 with
  | error fault =>
    simp only [Source.ReturnsState, result, Except.map] at sourcePath targetOutcome
    obtain ⟨failedK, path⟩ := sourcePath [.function none source.returnType.type]
    obtain ⟨code, rfl, observed⟩ := d.tree.erase.fault_observation σ fault
      (by simpa only [StateTree.erase_sound] using result)
    apply Transition.refines_of_terminals path.toReaches targetPath.toReaches
      (by rfl) (by simp [IR.step, targetOutcome]) (o := Observation.diagnostic code)
    · simpa [observeSource] using observed
    · simpa [observeTarget, targetOutcome] using observed
  | ok raw =>
    let value := raw
    have valueTyped : raw.type = source.returnType.type := d.typed σ raw result
    simp only [Source.ReturnsState, result, Except.map] at sourcePath targetOutcome
    let sf : Source.State := ⟨.halted (.ok value), [], [], source.symbol⟩
    have sourceFinish : Source.Steps s se sf :=
      (sourcePath [.function none source.returnType.type]).trans
        (.next (by simp [Source.Step, Source.step, Source.resumeFlow, value, sf,
          valueTyped]) (.refl _))
    apply Transition.refines_of_terminals sourceFinish.toReaches targetPath.toReaches
      (by rfl) (by simp [IR.step, targetOutcome])
      (o := Observation.returned source.returnType.type value.bits.toNat)
    · simp [observeSource, sf, value, valueTyped]
    · simp [observeTarget, targetOutcome, sourceValueIR, value, valueTyped]

structure ReturnPair (source : Source.Program) (target : IR.Program) where
  sourceFunction : Source.Function
  targetFunction : IR.Function
  selected : source.entries.contains sourceFunction.symbol = true
  sourceFound : source.functions.find? (·.symbol == sourceFunction.symbol) = some sourceFunction
  targetFound : target.functions.find? (·.name == targetFunction.name) = some targetFunction
  diagram : ReturnDiagram sourceFunction targetFunction (parameterWords sourceFunction.parameters)

def returnPair (source : Source.Program) (target : IR.Program) : Option (ReturnPair source target) := do
  if !(Source.checkWellFormed source).isOk || !(IR.checkProgram target).isOk then none else
  match hs : source.functions with
  | [sf] =>
    if chosen : source.entries = [sf.symbol] then
      let tf : {f // target.functions.find? (·.name == sf.name) = some f} ←
        match found : target.functions.find? (·.name == sf.name) with
        | none => none
        | some f => some ⟨f, found⟩
      have name : tf.val.name = sf.name := by
        simpa using List.find?_some tf.property
      let d ← returnDiagram sf tf.val (parameterWords sf.parameters)
      return {
        sourceFunction := sf, targetFunction := tf.val
        selected := by simp [chosen]
        sourceFound := by simp [hs]
        targetFound := by simpa only [name] using tf.property
        diagram := d }
    else none
  | _ => none

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
