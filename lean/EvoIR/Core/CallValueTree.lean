import EvoIR.Core.CallSourceDiagram

/-! Continuations of finite callee result trees. A caller continuation is
executed only at a successful return leaf, never before a callee guard or on
an unselected branch. This reuses the scalar term semantics unchanged. -/
namespace EvoIR.Core.Symbolic

inductive ValueBind {Γ : Var → Nat} (next : CWord Γ → ValueTree Γ → Prop) :
    ValueTree Γ → ValueTree Γ → Prop where
  | value {word output} (continued : next word output) : ValueBind next (.value word) output
  | checked {guard before after} (inner : ValueBind next before after) :
      ValueBind next (.checked guard before) (.checked guard after)
  | branch {condition yes no yes' no'}
      (first : ValueBind next yes yes') (second : ValueBind next no no') :
      ValueBind next (.branch condition yes no) (.branch condition yes' no')

theorem ValueBind.outcome {Γ : Var → Nat} {next : CWord Γ → ValueTree Γ → Prop}
    {before after : ValueTree Γ} (bound : ValueBind next before after) (σ : Subst Γ) :
    (∃ fault, before.denote σ = .error fault ∧ after.denote σ = .error fault) ∨
    (∃ word output, next word output ∧ before.denote σ = .ok (word.denote σ) ∧
      after.denote σ = output.denote σ) := by
  induction bound with
  | value continued => exact .inr ⟨_, _, continued, rfl, rfl⟩
  | checked inner ih =>
    simp only [ValueTree.denote]
    cases guards σ [_] with
    | error fault => exact .inl ⟨fault, rfl, rfl⟩
    | ok _ => exact ih
  | branch first second ih ih' =>
    simp only [ValueTree.denote]
    split <;> assumption

def ValueTree.bind? {Γ : Var → Nat} {next : CWord Γ → ValueTree Γ → Prop}
    (build : ∀ word, Option {tree // next word tree}) (tree : ValueTree Γ) :
    Option {output // ValueBind next tree output} :=
  match tree with
  | .value word => do
    let r ← build word
    return ⟨r.val, .value r.property⟩
  | .checked guard inner => do
    let r ← inner.bind? build
    return ⟨.checked guard r.val, .checked r.property⟩
  | .branch condition yes no => do
    let a ← yes.bind? build
    let b ← no.bind? build
    return ⟨.branch condition a.val b.val, .branch a.property b.property⟩

def ValueTree.Typed {Γ : Var → Nat} (tree : ValueTree Γ) (type : Source.IntType) : Bool :=
  (tree.withLocals []).Typed type

theorem ValueTree.typed_sound {Γ : Var → Nat} {tree : ValueTree Γ} {type : Source.IntType}
    (typed : tree.Typed type = true) (σ : Subst Γ) (value : Source.Value)
    (returned : tree.denote σ = .ok value) : value.type = type := by
  apply StateTree.typed_sound typed σ value
  simpa only [ValueTree.withLocals_sound] using returned

end EvoIR.Core.Symbolic
