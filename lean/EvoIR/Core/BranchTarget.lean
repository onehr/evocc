import EvoIR.Core.BranchExpression
import EvoIR.Core.LeafCorrespondence

/-! Finite CFG diagrams for branching return expressions. Every block and
edge is read from the actual target, with simultaneous successor binding.
Cycles refuse this component; loop simulation is a separate V01 obligation. -/
namespace EvoIR.Core.Symbolic

def targetState {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (f : IR.Function) (label : String) (pc owner nextFrame : Nat) (scope : Scope Γ) : IR.State :=
  ⟨.running ⟨f.name, label, pc, scope.denote σ ρ, owner⟩, [], nextFrame⟩

def ValueTree.irResult {Γ : Var → Nat} (σ : Subst Γ) (tree : ValueTree Γ) :
    Except Source.Fault (List IR.Value) :=
  (tree.denote σ).map (fun value => [sourceValueIR value])

def TargetRuns {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (label : String) (pc : Nat) (scope : Scope Γ) (tree : ValueTree Γ)
    (σ : Subst Γ) (ρ : Nat → IR.Value) (owner nextFrame : Nat) : Prop :=
  ∃ finalState, IR.Steps p (targetState σ ρ f label pc owner nextFrame scope) finalState ∧
    finalState.control = .halted (tree.irResult σ)

theorem block_prefix_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (p : IR.Program) (f : IR.Function) (block : IR.Block) (scope next : Scope Γ)
    (checks : List (Guard Γ)) (tree : ValueTree Γ) (owner nextFrame : Nat)
    (function : p.functions.find? (·.name == f.name) = some f)
    (found : f.blocks.find? (·.name == block.name) = some block)
    (code : instructions scope block.instructions = some (checks, next))
    (finish : TargetRuns p f block.name block.instructions.length next tree σ ρ owner nextFrame) :
    TargetRuns p f block.name 0 scope (tree.prefix checks) σ ρ owner nextFrame := by
  obtain ⟨last, path, outcome⟩ := segment_execution_sound σ ρ
    (targetState σ ρ f block.name 0 owner nextFrame scope)
    ⟨f.name, block.name, 0, scope.denote σ ρ, owner⟩ f block scope next block.instructions checks
    rfl function found (by intro i inst h; simpa using h) rfl code
  cases checked : guards σ checks with
  | error fault =>
    refine ⟨last, path, ?_⟩
    simpa [ValueTree.irResult, ValueTree.prefix_sound, checked, bind, Except.bind, Except.map]
      using outcome
  | ok u =>
    have same : last = targetState σ ρ f block.name block.instructions.length owner nextFrame next := by
      simpa [checked, targetState] using outcome
    obtain ⟨finalState, remaining, halted⟩ := finish
    subst last
    refine ⟨finalState, path.trans remaining, ?_⟩
    simpa [ValueTree.irResult, ValueTree.prefix_sound, checked, bind, Except.bind] using halted

inductive TargetDiagram {Γ : Var → Nat} (types : Nat → IR.Ty)
    (f : IR.Function) (type : Source.IntType) : Scope Γ → String → ValueTree Γ → Prop where
  | returned (scope next : Scope Γ) (block : IR.Block) (checks : List (Guard Γ))
      (output : Nat) (term : Term Γ type.width)
      (found : f.blocks.find? (·.name == block.name) = some block)
      (code : instructions scope block.instructions = some (checks, next))
      (terminator : block.terminator = .return [output])
      (lookup : next.word output = some ⟨type.width, term⟩) :
      TargetDiagram types f type scope block.name
        ((ValueTree.value ⟨type, term⟩).prefix checks)
  | jump (scope next after : Scope Γ) (block : IR.Block) (checks : List (Guard Γ))
      (successor : IR.Edge) (tree : ValueTree Γ)
      (found : f.blocks.find? (·.name == block.name) = some block)
      (code : instructions scope block.instructions = some (checks, next))
      (terminator : block.terminator = .branch successor)
      (edge : edge types f next successor = some after)
      (tail : TargetDiagram types f type after successor.block tree) :
      TargetDiagram types f type scope block.name (tree.prefix checks)
  | choice (scope next yesScope noScope : Scope Γ) (block : IR.Block) (checks : List (Guard Γ))
      (id : Nat) (condition : Term Γ 1) (yes no : IR.Edge) (a b : ValueTree Γ)
      (found : f.blocks.find? (·.name == block.name) = some block)
      (code : instructions scope block.instructions = some (checks, next))
      (terminator : block.terminator = .conditional id yes no)
      (test : next.word id = some ⟨1, condition⟩)
      (firstEdge : edge types f next yes = some yesScope)
      (secondEdge : edge types f next no = some noScope)
      (first : TargetDiagram types f type yesScope yes.block a)
      (second : TargetDiagram types f type noScope no.block b) :
      TargetDiagram types f type scope block.name
        ((ValueTree.branch condition a b).prefix checks)

theorem TargetDiagram.sound {Γ : Var → Nat} {types : Nat → IR.Ty}
    {f : IR.Function} {type : Source.IntType} {scope : Scope Γ} {label : String}
    {tree : ValueTree Γ} (d : TargetDiagram types f type scope label tree)
    (σ : Subst Γ) (ρ : Nat → IR.Value) (typed : ∀ id, (ρ id).type = types id)
    (p : IR.Program) (function : p.functions.find? (·.name == f.name) = some f)
    (returns : f.returns = [.bits type.width]) (owner nextFrame : Nat) :
    TargetRuns p f label 0 scope tree σ ρ owner nextFrame := by
  induction d with
  | returned scope next block checks output term found code terminator lookup =>
    apply block_prefix_sound σ ρ p f block scope next checks _ owner nextFrame function found code
    have value := word_lookup_sound σ ρ next output ⟨type.width, term⟩ lookup
    refine ⟨⟨.halted (.ok [.bits type.width (denoteTerm σ term)]), [], nextFrame⟩,
      .next (.execute ?_) (.refl _), rfl⟩
    simp [IR.step, IR.execute, targetState, function, found, terminator, value, returns,
      Word.denote, IR.Value.type, bind, Except.bind, pure, Except.pure]
  | jump scope next after block checks successor tree found code terminator edge _ ih =>
    apply block_prefix_sound σ ρ p f block scope next checks tree owner nextFrame function found code
    have step := branch_execution_sound σ ρ types typed p
      (targetState σ ρ f block.name block.instructions.length owner nextFrame next)
      ⟨f.name, block.name, block.instructions.length, next.denote σ ρ, owner⟩
      f block next after successor rfl function found rfl terminator rfl
      (by simp [checkEdge, edge])
    obtain ⟨finalState, path, halted⟩ := ih
    refine ⟨finalState, .next (.execute ?_) path, halted⟩
    exact step
  | choice scope next yesScope noScope block checks id condition yes no a b
      found code terminator test firstEdge secondEdge _ _ first second =>
    apply block_prefix_sound σ ρ p f block scope next checks _ owner nextFrame function found code
    have step := conditional_execution_sound σ ρ types typed p
      (targetState σ ρ f block.name block.instructions.length owner nextFrame next)
      ⟨f.name, block.name, block.instructions.length, next.denote σ ρ, owner⟩
      f block next yesScope noScope id condition yes no rfl function found rfl terminator rfl test
      (by simp [checkEdge, firstEdge]) (by simp [checkEdge, secondEdge])
    by_cases zero : (denoteTerm σ condition).toNat == 0
    · obtain ⟨finalState, path, halted⟩ := second
      refine ⟨finalState, .next (.execute ?_) path, ?_⟩
      · simpa [zero, targetState] using step
      · simpa [ValueTree.irResult, ValueTree.denote, bne, zero] using halted
    · obtain ⟨finalState, path, halted⟩ := first
      refine ⟨finalState, .next (.execute ?_) path, ?_⟩
      · simpa [zero, targetState] using step
      · simpa [ValueTree.irResult, ValueTree.denote, bne, zero] using halted

/-- The bound limits syntax exploration, not execution acceptance. A backedge
without a completed cyclic diagram refuses; it is never certified by running
the first bounded number of loop iterations. -/
def targetDiagram {Γ : Var → Nat} (types : Nat → IR.Ty) (f : IR.Function)
    (type : Source.IntType) (budget : Nat) (scope : Scope Γ) (label : String) :
    Option {tree : ValueTree Γ // TargetDiagram types f type scope label tree} :=
  match budget with
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
          return ⟨(ValueTree.value ⟨type, width ▸ term⟩).prefix checks, by
            subst n
            exact name ▸ TargetDiagram.returned scope next block.val checks id term
              found output.property terminator lookup⟩
        else none
    | .branch successor =>
      match edgeProof : edge types f next successor with
      | none => none
      | some after => do
        let tail ← targetDiagram types f type budget after successor.block
        return ⟨tail.val.prefix checks, name ▸ TargetDiagram.jump scope next after block.val
          checks successor tail.val found output.property terminator edgeProof tail.property⟩
    | .conditional id yes no =>
      match lookup : next.word id with
      | none => none
      | some ⟨n, term⟩ =>
        if width : n = 1 then
          match firstEdge : edge types f next yes, secondEdge : edge types f next no with
          | some yesScope, some noScope => do
            let a ← targetDiagram types f type budget yesScope yes.block
            let b ← targetDiagram types f type budget noScope no.block
            return ⟨(ValueTree.branch (width ▸ term) a.val b.val).prefix checks, by
              subst n
              exact name ▸ TargetDiagram.choice scope next yesScope noScope block.val
                checks id term yes no a.val b.val found output.property terminator lookup
                firstEdge secondEdge a.property b.property⟩
          | _, _ => none
        else none
    | _ => none

end EvoIR.Core.Symbolic
