import EvoIR.Core.SourceExpression

/-! Source-directed branching expression diagrams. Branches remain explicit:
the unselected arm is not executed, and each guard retains its position and
diagnostic. The decision tree is finite syntax, not bounded execution fuel. -/
namespace EvoIR.Core.Source

theorem evaluates_conditional (p : Program) (locals : Locals) (fid : Nat)
    (type : IntType) (condition yes no : Expr) (c a b : Result)
    (test : Evaluates p locals fid condition c)
    (first : Evaluates p locals fid yes a) (second : Evaluates p locals fid no b) :
    Evaluates p locals fid (.conditional type condition yes no)
      (do let value ← c; (if value.truth then a else b).map (Value.cast type)) := by
  intro k
  have start : Step p ⟨.expression (.conditional type condition yes no), locals, k, fid⟩
      ⟨.expression condition, locals, .choice type yes no :: k, fid⟩ := rfl
  cases c with
  | error fault =>
    obtain ⟨failedK, path⟩ := test (.choice type yes no :: k)
    exact ⟨failedK, .next start path⟩
  | ok value =>
    have prefixPath := Steps.next start (test (.choice type yes no :: k))
    cases truth : value.truth <;> simp only [bind, Except.bind, truth, Bool.false_eq_true,
      ↓reduceIte]
    · cases b with
      | error fault =>
        obtain ⟨failedK, path⟩ := second (.convert type :: k)
        exact ⟨failedK, prefixPath.trans (.next (by simp [Step, step, resumeValue, truth]) path)⟩
      | ok result =>
        exact prefixPath.trans (.next (by simp [Step, step, resumeValue, truth])
          ((second (.convert type :: k)).trans (.next rfl (.refl _))))
    · cases a with
      | error fault =>
        obtain ⟨failedK, path⟩ := first (.convert type :: k)
        exact ⟨failedK, prefixPath.trans (.next (by simp [Step, step, resumeValue, truth]) path)⟩
      | ok result =>
        exact prefixPath.trans (.next (by simp [Step, step, resumeValue, truth])
          ((first (.convert type :: k)).trans (.next rfl (.refl _))))

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

deriving instance DecidableEq for CWord

inductive ValueTree (Γ : Var → Nat) where
  | value (word : CWord Γ)
  | checked (guard : Guard Γ) (next : ValueTree Γ)
  | branch (condition : Term Γ 1) (yes no : ValueTree Γ)
  deriving DecidableEq

def ValueTree.denote {Γ : Var → Nat} (σ : Subst Γ) : ValueTree Γ → Source.Result
  | .value word => .ok (word.denote σ)
  | .checked guard next => do guards σ [guard]; next.denote σ
  | .branch condition yes no =>
    if (denoteTerm σ condition).toNat != 0 then yes.denote σ else no.denote σ

def ValueTree.prefix {Γ : Var → Nat} (checks : List (Guard Γ)) (tree : ValueTree Γ) :
    ValueTree Γ := checks.foldr .checked tree

theorem ValueTree.prefix_sound {Γ : Var → Nat} (σ : Subst Γ)
    (checks : List (Guard Γ)) (tree : ValueTree Γ) :
    (tree.prefix checks).denote σ = (do guards σ checks; tree.denote σ) := by
  induction checks with
  | nil => rfl
  | cons g gs ih =>
    simp only [ValueTree.prefix, List.foldr_cons, denote, guards]
    split
    · rfl
    · exact ih

def ValueTree.cast {Γ : Var → Nat} (type : Source.IntType) : ValueTree Γ → ValueTree Γ
  | .value word => .value (word.cast type)
  | .checked guard next => .checked guard (next.cast type)
  | .branch condition yes no => .branch condition (yes.cast type) (no.cast type)

theorem ValueTree.cast_sound {Γ : Var → Nat} (σ : Subst Γ)
    (type : Source.IntType) (tree : ValueTree Γ) :
    (tree.cast type).denote σ = (tree.denote σ).map (Source.Value.cast type) := by
  induction tree with
  | value word => simp [cast, denote, EvoIR.Core.Symbolic.cast_sound, Except.map]
  | checked guard next ih =>
    simp only [cast, denote, ih]
    cases guards σ [guard] <;> rfl
  | branch condition yes no ih ih' =>
    simp only [cast, denote]
    split <;> assumption

def ExpressionResult.tree {Γ : Var → Nat} (result : ExpressionResult Γ) : ValueTree Γ :=
  (ValueTree.value result.2).prefix result.1

theorem ExpressionResult.tree_sound {Γ : Var → Nat} (σ : Subst Γ)
    (result : ExpressionResult Γ) : result.tree.denote σ = result.denote σ := by
  rw [tree, ValueTree.prefix_sound]
  rfl

inductive BranchExpressionDiagram {Γ : Var → Nat} (env : CBindings Γ) :
    Source.Expr → ValueTree Γ → Prop where
  | linear {e : Source.Expr} {r : ExpressionResult Γ} (diagram : ExpressionDiagram env e r) :
      BranchExpressionDiagram env e r.tree
  | conditional (type : Source.IntType) (condition yes no : Source.Expr)
      (c : ExpressionResult Γ) (a b : ValueTree Γ)
      (test : ExpressionDiagram env condition c)
      (first : BranchExpressionDiagram env yes a) (second : BranchExpressionDiagram env no b) :
      BranchExpressionDiagram env (.conditional type condition yes no)
        ((ValueTree.branch c.2.truthTerm (a.cast type) (b.cast type)).prefix c.1)

theorem BranchExpressionDiagram.sound {Γ : Var → Nat} {env : CBindings Γ}
    {e : Source.Expr} {tree : ValueTree Γ} (diagram : BranchExpressionDiagram env e tree)
    (σ : Subst Γ) (p : Source.Program) (locals : Source.Locals) (fid : Nat)
    (related : env.Related σ locals) :
    Source.Evaluates p locals fid e (tree.denote σ) := by
  induction diagram with
  | linear diagram =>
    rw [ExpressionResult.tree_sound]
    exact diagram.sound σ p locals fid related
  | conditional type condition yes no c a b test _ _ first second =>
    have path := Source.evaluates_conditional p locals fid type condition yes no
      (c.denote σ) (a.denote σ) (b.denote σ)
      (test.sound σ p locals fid related) first second
    have same : ((ValueTree.branch c.2.truthTerm (a.cast type) (b.cast type)).prefix c.1).denote σ =
        (do let v ← c.denote σ;
            (if v.truth then a.denote σ else b.denote σ).map (Source.Value.cast type)) := by
      rw [ValueTree.prefix_sound]
      simp only [ExpressionResult.denote, ValueTree.denote, truth_term_sound, ValueTree.cast_sound]
      cases guards σ c.1 with
      | error fault => rfl
      | ok u =>
        cases u
        simp only [bind, Except.bind, pure, Except.pure]
        cases (c.2.denote σ).truth <;> rfl
    rw [same]
    exact path

def branchExpression {Γ : Var → Nat} (env : CBindings Γ) (e : Source.Expr) :
    Option {tree : ValueTree Γ // BranchExpressionDiagram env e tree} :=
  match expression env e with
  | some r => some ⟨r.val.tree, .linear r.property⟩
  | none => match e with
    | .conditional type condition yes no => do
      let c ← expression env condition
      let a ← branchExpression env yes
      let b ← branchExpression env no
      return ⟨((ValueTree.branch c.val.2.truthTerm (a.val.cast type) (b.val.cast type)).prefix c.val.1),
        .conditional type condition yes no c.val a.val b.val c.property a.property b.property⟩
    | _ => none

end EvoIR.Core.Symbolic
