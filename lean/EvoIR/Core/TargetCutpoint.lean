import EvoIR.Core.BranchTarget
import EvoIR.Core.CutTree
import EvoIR.Core.CheckpointSimulation

/-! Target block diagrams stop at explicit successor cutpoints instead of
unrolling a cycle. A cutpoint is reached only by a checked actual terminator
and simultaneous block-argument binding. Unknown operations still refuse. -/
namespace EvoIR.Core.Symbolic

inductive TargetExit (Γ : Var → Nat) where
  | returned (word : CWord Γ)
  | next (label : String) (scope : Scope Γ)

def TargetExit.state {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (f : IR.Function) (owner nextFrame : Nat) : TargetExit Γ → IR.State
  | .returned word => ⟨.halted (.ok [sourceValueIR (word.denote σ)]), [], nextFrame⟩
  | .next label scope => targetState σ ρ f label 0 owner nextFrame scope

def TargetCutRuns {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (advance : Bool) (label : String) (pc : Nat) (scope : Scope Γ) (tree : CutTree Γ (TargetExit Γ))
    (σ : Subst Γ) (ρ : Nat → IR.Value) (owner nextFrame : Nat) : Prop :=
  ∃ final, IR.Steps p (targetState σ ρ f label pc owner nextFrame scope) final ∧
    (advance = true → Relation.TransGen (Transition.Predecessor (IR.step p)) final
      (targetState σ ρ f label pc owner nextFrame scope)) ∧
    match tree.eval σ with
    | .error fault => final.control = .halted (.error fault)
    | .ok exit => final = exit.state σ ρ f owner nextFrame

theorem target_cut_prefix {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (p : IR.Program) (f : IR.Function) (advance : Bool) (block : IR.Block) (scope next : Scope Γ)
    (checks : List (Guard Γ)) (tree : CutTree Γ (TargetExit Γ)) (owner nextFrame : Nat)
    (function : p.functions.find? (·.name == f.name) = some f)
    (found : f.blocks.find? (·.name == block.name) = some block)
    (code : instructions scope block.instructions = some (checks, next))
    (finish : TargetCutRuns p f advance block.name block.instructions.length next tree σ ρ owner nextFrame) :
    TargetCutRuns p f advance block.name 0 scope (tree.prefix checks) σ ρ owner nextFrame := by
  obtain ⟨last, path, outcome⟩ := segment_execution_sound σ ρ
    (targetState σ ρ f block.name 0 owner nextFrame scope)
    ⟨f.name, block.name, 0, scope.denote σ ρ, owner⟩ f block scope next block.instructions checks
    rfl function found (by intro i inst h; simpa using h) rfl code
  cases checked : guards σ checks with
  | error fault =>
    have halted : last.control = .halted (.error fault) := by simpa [checked] using outcome
    refine ⟨last, path, ?_, ?_⟩
    · intro _
      apply path.toReaches.positive_of_ne
      intro same
      rw [← same] at halted
      cases halted
    · simpa [CutTree.prefix_sound, checked, bind, Except.bind] using halted
  | ok u =>
    have same : last = targetState σ ρ f block.name block.instructions.length owner nextFrame next := by
      simpa [checked, targetState] using outcome
    obtain ⟨final, remaining, positive, exit⟩ := finish
    subst last
    refine ⟨final, path.trans remaining, fun h => path.toReaches.then_positive (positive h), ?_⟩
    simpa [CutTree.prefix_sound, checked, bind, Except.bind] using exit

inductive TargetCutDiagram {Γ : Var → Nat} (types : Nat → IR.Ty)
    (f : IR.Function) (type : Source.IntType) : Bool → Scope Γ → String → CutTree Γ (TargetExit Γ) → Prop where
  | boundary (scope : Scope Γ) (label : String) :
      TargetCutDiagram types f type false scope label (.leaf (.next label scope))
  | returned (advance : Bool) (scope next : Scope Γ) (block : IR.Block) (checks : List (Guard Γ))
      (output : Nat) (term : Term Γ type.width)
      (found : f.blocks.find? (·.name == block.name) = some block)
      (code : instructions scope block.instructions = some (checks, next))
      (terminator : block.terminator = .return [output])
      (lookup : next.word output = some ⟨type.width, term⟩) :
      TargetCutDiagram types f type advance scope block.name
        ((CutTree.leaf (.returned ⟨type, term⟩)).prefix checks)
  | jump (advance : Bool) (scope next after : Scope Γ) (block : IR.Block) (checks : List (Guard Γ))
      (successor : IR.Edge) (tree : CutTree Γ (TargetExit Γ))
      (found : f.blocks.find? (·.name == block.name) = some block)
      (code : instructions scope block.instructions = some (checks, next))
      (terminator : block.terminator = .branch successor)
      (edge : edge types f next successor = some after)
      (tail : TargetCutDiagram types f type false after successor.block tree) :
      TargetCutDiagram types f type advance scope block.name (tree.prefix checks)
  | choice (advance : Bool) (scope next yesScope noScope : Scope Γ) (block : IR.Block) (checks : List (Guard Γ))
      (id : Nat) (condition : Term Γ 1) (yes no : IR.Edge) (a b : CutTree Γ (TargetExit Γ))
      (found : f.blocks.find? (·.name == block.name) = some block)
      (code : instructions scope block.instructions = some (checks, next))
      (terminator : block.terminator = .conditional id yes no)
      (test : next.word id = some ⟨1, condition⟩)
      (firstEdge : edge types f next yes = some yesScope)
      (secondEdge : edge types f next no = some noScope)
      (first : TargetCutDiagram types f type false yesScope yes.block a)
      (second : TargetCutDiagram types f type false noScope no.block b) :
      TargetCutDiagram types f type advance scope block.name
        ((CutTree.branch condition a b).prefix checks)

theorem TargetCutDiagram.sound {Γ : Var → Nat} {types : Nat → IR.Ty}
    {f : IR.Function} {type : Source.IntType} {advance : Bool} {scope : Scope Γ} {label : String}
    {tree : CutTree Γ (TargetExit Γ)} (diagram : TargetCutDiagram types f type advance scope label tree)
    (σ : Subst Γ) (ρ : Nat → IR.Value) (typed : ∀ id, (ρ id).type = types id)
    (p : IR.Program) (function : p.functions.find? (·.name == f.name) = some f)
    (returns : f.returns = [.bits type.width]) (owner nextFrame : Nat) :
    TargetCutRuns p f advance label 0 scope tree σ ρ owner nextFrame := by
  induction diagram with
  | boundary scope label => exact ⟨_, .refl _, by simp, rfl⟩
  | returned advance scope next block checks output term found code terminator lookup =>
    apply target_cut_prefix σ ρ p f advance block scope next checks _ owner nextFrame function found code
    have value := word_lookup_sound σ ρ next output ⟨type.width, term⟩ lookup
    have step : IR.step p (targetState σ ρ f block.name block.instructions.length owner nextFrame next) =
        some ⟨.halted (.ok [.bits type.width (denoteTerm σ term)]), [], nextFrame⟩ := by
      simp [IR.step, IR.execute, targetState, function, found, terminator, value, returns,
        Word.denote, IR.Value.type, bind, Except.bind, pure, Except.pure]
    exact ⟨_, .next (.execute step) (.refl _), fun _ => .single step, rfl⟩
  | jump advance scope next after block checks successor tree found code terminator edge _ ih =>
    apply target_cut_prefix σ ρ p f advance block scope next checks _ owner nextFrame function found code
    have step := branch_execution_sound σ ρ types typed p
      (targetState σ ρ f block.name block.instructions.length owner nextFrame next)
      ⟨f.name, block.name, block.instructions.length, next.denote σ ρ, owner⟩
      f block next after successor rfl function found rfl terminator rfl
      (by simp [checkEdge, edge])
    obtain ⟨final, path, _, exit⟩ := ih
    exact ⟨final, .next (.execute step) path, fun _ => path.toReaches.after_step step, exit⟩
  | choice advance scope next yesScope noScope block checks id condition yes no a b
      found code terminator test firstEdge secondEdge _ _ first second =>
    apply target_cut_prefix σ ρ p f advance block scope next checks _ owner nextFrame function found code
    have step := conditional_execution_sound σ ρ types typed p
      (targetState σ ρ f block.name block.instructions.length owner nextFrame next)
      ⟨f.name, block.name, block.instructions.length, next.denote σ ρ, owner⟩
      f block next yesScope noScope id condition yes no rfl function found rfl terminator rfl test
      (by simp [checkEdge, firstEdge]) (by simp [checkEdge, secondEdge])
    by_cases zero : (denoteTerm σ condition).toNat == 0
    · obtain ⟨final, path, _, exit⟩ := second
      have actual := step
      simp only [zero] at actual
      refine ⟨final, .next (.execute actual) path, fun _ => path.toReaches.after_step actual, ?_⟩
      · simpa [CutTree.eval, bne, zero] using exit
    · obtain ⟨final, path, _, exit⟩ := first
      have actual := step
      simp only [zero] at actual
      refine ⟨final, .next (.execute actual) path, fun _ => path.toReaches.after_step actual, ?_⟩
      · simpa [CutTree.eval, bne, zero] using exit

/-- Explore the actual CFG up to the proposed cutpoints. `advance=true` at
the root prevents a cutpoint from certifying itself with a zero-step path.
The bound limits the finite between-cutpoint syntax, never loop iterations. -/
def targetCutDiagram {Γ : Var → Nat} (types : Nat → IR.Ty) (f : IR.Function)
    (type : Source.IntType) (cutpoints : List String) (budget : Nat)
    (advance : Bool) (scope : Scope Γ) (label : String) :
    Option {tree : CutTree Γ (TargetExit Γ) //
      TargetCutDiagram types f type advance scope label tree} :=
  if stopped : advance = false ∧ label ∈ cutpoints then
    some ⟨.leaf (.next label scope), stopped.1 ▸ TargetCutDiagram.boundary scope label⟩
  else match budget with
  | 0 => none
  | budget+1 => do
    let block : {b : IR.Block // f.blocks.find? (·.name == label) = some b} ←
      match h : f.blocks.find? (·.name == label) with
      | none => none
      | some b => some ⟨b, h⟩
    have name : block.val.name = label := by
      have h := List.find?_some block.property
      simpa using h
    have found : f.blocks.find? (·.name == block.val.name) = some block.val := by
      simpa only [name] using block.property
    let output : {pair : List (Guard Γ) × Scope Γ //
        instructions scope block.val.instructions = some pair} ←
      match h : instructions scope block.val.instructions with
      | none => none
      | some pair => some ⟨pair, h⟩
    let checks := output.val.1
    let next := output.val.2
    match terminator : block.val.terminator with
    | .return [id] =>
      match lookup : next.word id with
      | none => none
      | some ⟨n, term⟩ =>
        if width : n = type.width then
          return ⟨(CutTree.leaf (.returned ⟨type, width ▸ term⟩)).prefix checks, by
            subst n
            exact name ▸ TargetCutDiagram.returned advance scope next block.val checks id term
              found output.property terminator lookup⟩
        else none
    | .branch successor =>
      match edgeProof : edge types f next successor with
      | none => none
      | some after => do
        let tail ← targetCutDiagram types f type cutpoints budget false after successor.block
        return ⟨tail.val.prefix checks, name ▸ TargetCutDiagram.jump advance scope next after block.val
          checks successor tail.val found output.property terminator edgeProof tail.property⟩
    | .conditional id yes no =>
      match lookup : next.word id with
      | none => none
      | some ⟨n, term⟩ =>
        if width : n = 1 then
          match firstEdge : edge types f next yes, secondEdge : edge types f next no with
          | some yesScope, some noScope => do
            let a ← targetCutDiagram types f type cutpoints budget false yesScope yes.block
            let b ← targetCutDiagram types f type cutpoints budget false noScope no.block
            return ⟨(CutTree.branch (width ▸ term) a.val b.val).prefix checks, by
              subst n
              exact name ▸ TargetCutDiagram.choice advance scope next yesScope noScope block.val
                checks id term yes no a.val b.val found output.property terminator lookup
                firstEdge secondEdge a.property b.property⟩
          | _, _ => none
        else none
    | _ => none

theorem target_cut_diagram_sound {Γ : Var → Nat} (types : Nat → IR.Ty) (f : IR.Function)
    (type : Source.IntType) (cutpoints : List String) (budget : Nat) (advance : Bool)
    (scope : Scope Γ) (label : String) (tree : CutTree Γ (TargetExit Γ))
    (accepted : (targetCutDiagram types f type cutpoints budget advance scope label).map
      Subtype.val = some tree) : TargetCutDiagram types f type advance scope label tree := by
  cases h : targetCutDiagram types f type cutpoints budget advance scope label with
  | none => simp [h] at accepted
  | some result =>
    simp only [h, Option.map_some, Option.some.injEq] at accepted
    exact accepted ▸ result.property

end EvoIR.Core.Symbolic
