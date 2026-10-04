import EvoIR.Core.SymbolicEntry
import EvoIR.Core.Correspondence
import EvoIR.Core.IRWellFormed

/-! The straight-line leaf case of source-to-IR checking. Whole original
return bodies, parameter binding, return conversion and the target terminator
are included. This is one case of the required general checker, not a smaller
replacement profile: control, writes and calls must acquire their own diagrams. -/
namespace EvoIR.Core.Source

inductive ReturnShape : Stmt → QualifiedType → Expr → Prop where
  | direct (destination : QualifiedType) (e : Expr) : ReturnShape (.return destination e) destination e
  | block {body : Stmt} {destination : QualifiedType} {e : Expr}
      (inner : ReturnShape body destination e) :
      ReturnShape (.compound [.statement body]) destination e

def returnShape : (body : Stmt) → Option ((d : QualifiedType) × (e : Expr) ×
    { _unit : Unit // ReturnShape body d e })
  | .return d e => some ⟨d, e, ⟨(), .direct d e⟩⟩
  | .compound [.statement body] => do
    let ⟨d, e, proof⟩ ← returnShape body
    return ⟨d, e, ⟨(), .block proof.property⟩⟩
  | _ => none

def Returns (p : Program) (locals : Locals) (fid : Nat) (body : Stmt) (r : Result) : Prop :=
  ∀ k, match r with
  | .ok value => Steps p ⟨.statement body, locals, k, fid⟩
      ⟨.flow (.returned value), locals, k, fid⟩
  | .error fault => ∃ failedK, Steps p ⟨.statement body, locals, k, fid⟩
      ⟨.halted (.error fault), locals, failedK, fid⟩

theorem ReturnShape.sound {body : Stmt} {d : QualifiedType} {e : Expr}
    (shape : ReturnShape body d e) (p : Program) (locals : Locals) (fid : Nat)
    (r : Result) (evaluated : Evaluates p locals fid e r) :
    Returns p locals fid body (r.map (Value.cast d.type)) := by
  induction shape with
  | direct d e =>
    intro k
    have first : Step p ⟨.statement (.return d e), locals, k, fid⟩
        ⟨.expression e, locals, .return d :: k, fid⟩ := rfl
    cases r with
    | error fault =>
      obtain ⟨failedK, path⟩ := evaluated (.return d :: k)
      exact ⟨failedK, .next first path⟩
    | ok value =>
      exact .next first ((evaluated (.return d :: k)).trans (.next rfl (.refl _)))
  | @block body d e inner ih =>
    intro k
    have prefixPath : Steps p ⟨.statement (.compound [.statement body]), locals, k, fid⟩
        ⟨.statement body, locals, .sequence [] :: .scope locals.length :: k, fid⟩ :=
      .next rfl (.next rfl (.refl _))
    cases r with
    | error fault =>
      obtain ⟨failedK, path⟩ := ih evaluated (.sequence [] :: .scope locals.length :: k)
      exact ⟨failedK, prefixPath.trans path⟩
    | ok value =>
      exact prefixPath.trans ((ih evaluated (.sequence [] :: .scope locals.length :: k)).trans
        (.next rfl (.next (by simp [Step, step, resumeFlow]) (.refl _))))

end EvoIR.Core.Source

namespace EvoIR.Core.IR

theorem Steps.trans {p : Program} {a b c : State}
    (first : Steps p a b) (second : Steps p b c) : Steps p a c := by
  induction first with
  | refl => exact second
  | next step _ ih => exact .next step (ih second)

end EvoIR.Core.IR

namespace EvoIR.Core.Symbolic

structure LeafDiagram {Γ : Var → Nat} (source : Source.Function)
    (target : IR.Function) (args : List (CWord Γ)) where
  env : CBindings Γ
  scope : Scope Γ
  block : IR.Block
  expr : Source.Expr
  result : ExpressionResult Γ
  output : Nat
  entry : scalarEntry source target args = some (env, scope)
  body : Source.ReturnShape source.body source.returnType expr
  expression : ExpressionDiagram env expr result
  blocks : target.blocks = [block]
  terminator : block.terminator = .return [output]
  returns : target.returns = [.bits source.returnType.type.width]
  fragment : checkFragment scope block.instructions [output] result.1
    [(result.2.cast source.returnType.type).word] = true

/-- Effects and public identity are admission checks even though the local
execution theorem below only needs the body/entry/return obligations. -/
def leaf {Γ : Var → Nat} (source : Source.Function) (target : IR.Function)
    (args : List (CWord Γ)) : Option (LeafDiagram source target args) := do
  if source.name != target.name || source.internal != target.internal then none else
  if target.effects != ((IR.deriveEffects ⟨[target]⟩ 1).lookup target.name).getD
      IR.Reader.emptyEffects then none else
  let entry : {pair : CBindings Γ × Scope Γ // scalarEntry source target args = some pair} ←
    match h : scalarEntry source target args with
    | none => none
    | some pair => some ⟨pair, h⟩
  let env := entry.val.1
  let scope := entry.val.2
  let ⟨destination, expr, body⟩ ← Source.returnShape source.body
  if destinationEq : destination = source.returnType then
    let r ← expression env expr
    match blocksEq : target.blocks with
    | [block] =>
      match termEq : block.terminator with
      | .return [output] =>
        if returnsEq : target.returns = [.bits source.returnType.type.width] then
          if checked : checkFragment scope block.instructions [output] r.val.1
              [(r.val.2.cast source.returnType.type).word] = true then
            return {
              env := env
              scope := scope
              block := block
              expr := expr
              result := r.val
              output := output
              entry := entry.property
              body := destinationEq ▸ body.property
              expression := r.property
              blocks := blocksEq
              terminator := termEq
              returns := returnsEq
              fragment := checked}
          else none
        else none
      | _ => none
    | _ => none
  else none

theorem guards_fault_observation {Γ : Var → Nat} (σ : Subst Γ)
    (checks : List (Guard Γ)) (fault : Source.Fault)
    (failed : guards σ checks = .error fault) :
    ∃ code, fault = .diagnostic code ∧ checkedDiagnostic code = some (.diagnostic code) := by
  induction checks with
  | nil => simp [guards] at failed
  | cons check rest ih =>
    simp only [guards] at failed
    split at failed
    · cases Except.error.inj failed
      refine ⟨check.reason.code, rfl, ?_⟩
      cases check.reason <;> decide
    · exact ih failed

/-- A complete terminating source/target leaf relation, not merely equality
of the extracted arithmetic terms. The original input programs, entry states,
return conversion, first diagnostic and every target instruction are covered. -/
theorem LeafDiagram.refines {Γ : Var → Nat} (σ : Subst Γ)
    {source : Source.Function} {target : IR.Function} {args : List (CWord Γ)}
    (d : LeafDiagram source target args) (s : Source.Program) (t : IR.Program)
    (selected : s.entries.contains source.symbol = true)
    (sourceFound : s.functions.find? (·.symbol == source.symbol) = some source)
    (targetFound : t.functions.find? (·.name == target.name) = some target) :
    ∃ sourceEntry targetEntry,
      Source.initial s source.symbol (args.map (CWord.denote σ)) = .ok sourceEntry ∧
      IR.initial t target.name (args.map (fun v => sourceValueIR (v.denote σ))) = .ok targetEntry ∧
      LoweringRefines s t source.returnType.type sourceEntry targetEntry := by
  obtain ⟨locals, block, boundSource, related, headBlock, boundTarget⟩ :=
    scalar_entry_sound σ source target args d.env d.scope 1 d.entry
  have sameBlock : block = d.block := by simpa [d.blocks] using headBlock.symm
  subst block
  let se : Source.State :=
    ⟨.statement source.body, locals, [.function none source.returnType.type], source.symbol⟩
  let a : IR.Activation := ⟨target.name, d.block.name, 0,
    d.scope.denote σ (fun _ => .world), 1⟩
  let te : IR.State := ⟨.running a, [], 2⟩
  have selectedMem : source.symbol ∈ s.entries := by simpa using selected
  refine ⟨se, te, ?_, ?_, ?_⟩
  · simp [Source.initial, selectedMem, sourceFound, boundSource, bind, Except.bind,
      pure, Except.pure, se]
  · simp [IR.initial, targetFound, boundTarget, bind, Except.bind, pure, Except.pure, te, a]
  have sourcePath := d.body.sound s locals source.symbol (d.result.denote σ)
    (d.expression.sound σ s locals source.symbol related)
  have blockFound : target.blocks.find? (·.name == d.block.name) = some d.block := by
    simp [d.blocks]
  have fragment := d.fragment
  unfold checkFragment at fragment
  split at fragment
  · contradiction
  · rename_i checks finalScope instructionsChecked
    have agreement := fragment
    simp only [Bool.and_eq_true, decide_eq_true_eq] at agreement
    rcases agreement with ⟨rfl, output⟩
    obtain ⟨tf, targetPath, outcome⟩ := segment_execution_sound σ (fun _ => .world)
      te a target d.block d.scope finalScope d.block.instructions d.result.1
      rfl targetFound blockFound (by intro i inst h; simpa [a] using h) rfl instructionsChecked
    cases checked : guards σ d.result.1 with
    | error fault =>
      have sourceResult : (d.result.denote σ).map (Source.Value.cast source.returnType.type) =
          .error fault := by simp [ExpressionResult.denote, checked, bind, Except.bind, Except.map]
      rw [sourceResult] at sourcePath
      obtain ⟨failedK, path⟩ := sourcePath [.function none source.returnType.type]
      have halted : tf.control = .halted (.error fault) := by simpa [checked] using outcome
      obtain ⟨code, rfl, observed⟩ := guards_fault_observation σ d.result.1 fault checked
      apply Transition.refines_of_terminals path.toReaches targetPath.toReaches
        (by rfl) (by simp [IR.step, halted])
        (o := Observation.diagnostic code)
      · simpa [observeSource] using observed
      · simpa [observeTarget, halted] using observed
    | ok unitValue =>
      let value := (d.result.2.denote σ).cast source.returnType.type
      have sourceResult : (d.result.denote σ).map (Source.Value.cast source.returnType.type) =
          .ok value := by simp [ExpressionResult.denote, checked, bind, Except.bind,
            pure, Except.pure, Except.map, value]
      rw [sourceResult] at sourcePath
      let sf : Source.State := ⟨.halted (.ok value), [], [], source.symbol⟩
      have sourceFinish : Source.Steps s se sf :=
        (sourcePath [.function none source.returnType.type]).trans
          (.next (by simp [Source.Step, Source.step, Source.resumeFlow, value, sf, Source.Value.cast,
            Source.Value.ofInt]) (.refl _))
      have finalState : tf = {te with control := .running {a with
          pc := d.block.instructions.length, values := finalScope.denote σ (fun _ => .world)}} := by
        simpa [checked, a] using outcome
      subst tf
      have resultLookup := operands_sound σ (fun _ => .world) finalScope [d.output]
        [(d.result.2.cast source.returnType.type).word] output
      have returned : [(d.result.2.cast source.returnType.type).word].map (Word.denote σ) =
          [sourceValueIR value] := by
        change [sourceValueIR ((d.result.2.cast source.returnType.type).denote σ)] = _
        rw [cast_sound]
      rw [returned] at resultLookup
      let terminal : IR.State := ⟨.halted (.ok [sourceValueIR value]), [], 2⟩
      have targetFinish : IR.Steps t te terminal := targetPath.trans
        (.next (.execute (by
          simp [IR.step, IR.execute, te, a, targetFound, blockFound, d.terminator,
            resultLookup, d.returns, value, sourceValueIR, Source.Value.cast, Source.Value.ofInt,
            IR.Value.type, bind, Except.bind, pure, Except.pure, terminal])) (.refl _))
      apply Transition.refines_of_terminals sourceFinish.toReaches targetFinish.toReaches
        (by rfl) (by rfl) (o := Observation.returned source.returnType.type value.bits.toNat)
      · rfl
      · simp [observeTarget, terminal, sourceValueIR, value, Source.Value.cast, Source.Value.ofInt]

structure LeafPair (source : Source.Program) (target : IR.Program) where
  sourceFunction : Source.Function
  targetFunction : IR.Function
  selected : source.entries.contains sourceFunction.symbol = true
  sourceFound : source.functions.find? (·.symbol == sourceFunction.symbol) = some sourceFunction
  targetFound : target.functions.find? (·.name == targetFunction.name) = some targetFunction
  diagram : LeafDiagram sourceFunction targetFunction (parameterWords sourceFunction.parameters)

/-- This front door admits the complete single-leaf case and refuses every
unsupported program shape. It shares readers, validation and semantic terms
with the general-checker work; there is no named-function or exact-text allowlist. -/
def leafPair (source : Source.Program) (target : IR.Program) : Option (LeafPair source target) := do
  if !(Source.checkWellFormed source).isOk || !(IR.checkProgram target).isOk then none else
  match hs : source.functions, ht : target.functions with
  | [sf], [tf] =>
    if chosen : source.entries = [sf.symbol] then
      let d ← leaf sf tf (parameterWords sf.parameters)
      return {
        sourceFunction := sf
        targetFunction := tf
        selected := by simp [chosen]
        sourceFound := by simp [hs]
        targetFound := by simp [ht]
        diagram := d }
    else none
  | _, _ => none

/-- Acceptance covers all arguments of the original signature, including
inputs that encounter checked diagnostics. It has no execution-fuel or
caller-supplied symbolic-value correspondence premise. -/
theorem LeafPair.refines {source : Source.Program} {target : IR.Program}
    (pair : LeafPair source target) (args : List Source.Value)
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
