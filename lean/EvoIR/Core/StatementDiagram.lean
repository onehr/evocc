import EvoIR.Core.StatementSource

/-! Proof-producing traversal of finite source statements. Depth bounds syntax
inspection only: loops and calls still require their own cyclic/call diagrams. -/
namespace EvoIR.Core.Source

mutual
  def statementDepth : Stmt → Nat
    | .compound items => 1 + itemsDepth items
    | .ite _ yes none => 1 + max (statementDepth yes) 1
    | .ite _ yes (some no) => 1 + max (statementDepth yes) (statementDepth no)
    | .while _ body | .doWhile body _ | .for _ _ _ body => 1 + statementDepth body
    | _ => 1
  def itemsDepth : List BlockItem → Nat
    | [] => 1
    | .statement stmt :: rest => 1 + max (statementDepth stmt) (itemsDepth rest)
    | .declaration _ :: rest => 1 + max 1 (itemsDepth rest)
end

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

def objectDiagram {Γ : Var → Nat} (locals : LocalStore Γ) (object : Source.Object) :
    Option {tree // FlowDiagram locals (.object object) tree} := do
  let fresh : {ls // locals.declare object.symbol object.type = some ls} ←
    match h : locals.declare object.symbol object.type with
    | none => none
    | some ls => some ⟨ls, h⟩
  match initialized : object.initializer with
  | none =>
    return ⟨.value fresh.val .normal, by
      intro σ p fid k
      exact .next (congrArg some (local_declare_sound σ locals fresh.val object initialized
        fresh.property ⟨.object object, locals.denote σ, k, fid⟩ rfl)) (.refl _)⟩
  | some (destination, expr) =>
    if same : destination = object.type then
      let e ← stateExpression (Source.expressionDepth expr) fresh.val expr
      let t ← e.val.bindFlow? (next := fun ls w t =>
        FrameDiagram ls (.initialize object.symbol object.type) w t) (fun ls w => do
          match written : ls.write object.symbol object.type (w.cast object.type.type) true with
          | none => none
          | some updated => some ⟨.value updated .normal,
              initialize_frame ls updated w object.symbol object.type written⟩)
      return ⟨t.val, initialized_object_diagram locals fresh.val object expr e.val t.val
        (same ▸ initialized) fresh.property e.property t.property⟩
    else none

def flowDiagram {Γ : Var → Nat} (depth : Nat) (locals : LocalStore Γ)
    (control : Source.Control) : Option {tree // FlowDiagram locals control tree} :=
  match depth with
  | 0 => none
  | depth+1 =>
    match control with
    | .object object => objectDiagram locals object
    | .items [] => some ⟨.value locals .normal, by intro σ p fid k; exact .next rfl (.refl _)⟩
    | .items (item :: rest) => do
      let a ← flowDiagram depth locals (itemControl item)
      let b ← a.val.bindNormal? (next := fun ls tree => FlowDiagram ls (.items rest) tree)
        (fun ls => flowDiagram depth ls (.items rest))
      return ⟨b.val, sequence_diagram locals item rest a.val b.val a.property b.property⟩
    | .statement .null => some ⟨.value locals .normal, by intro σ p fid k; exact .next rfl (.refl _)⟩
    | .statement .break => some ⟨.value locals .break, by intro σ p fid k; exact .next rfl (.refl _)⟩
    | .statement .continue => some ⟨.value locals .continue, by intro σ p fid k; exact .next rfl (.refl _)⟩
    | .statement (.expression expr) => do
      let a ← stateExpression (Source.expressionDepth expr) locals expr
      let b ← a.val.bindFlow? (next := fun ls w t => FrameDiagram ls .discard w t)
        (fun ls w => some ⟨.value ls .normal, discard_frame ls w⟩)
      return ⟨b.val, expression_flow_sound locals _ expr .discard a.val b.val
        (by intros; rfl) a.property b.property⟩
    | .statement (.return destination expr) => do
      let a ← stateExpression (Source.expressionDepth expr) locals expr
      let b ← a.val.bindFlow? (next := fun ls w t => FrameDiagram ls (.return destination) w t)
        (fun ls w => some ⟨.value ls (.returned (w.cast destination.type)), return_frame ls w destination⟩)
      return ⟨b.val, expression_flow_sound locals _ expr (.return destination) a.val b.val
        (by intros; rfl) a.property b.property⟩
    | .statement (.ite condition yes no) => do
      let a ← stateExpression (Source.expressionDepth condition) locals condition
      let b ← a.val.bindFlow? (next := fun ls w t => FrameDiagram ls (.branch yes no) w t)
        (fun ls w => do
          let first ← flowDiagram depth ls (.statement yes)
          let second ← flowDiagram depth ls (.statement (no.getD .null))
          return ⟨.branch w.truthTerm first.val second.val,
            branch_frame ls w yes no first.val second.val first.property second.property⟩)
      return ⟨b.val, expression_flow_sound locals _ condition (.branch yes no) a.val b.val
        (by intros; rfl) a.property b.property⟩
    | .statement (.compound items) => do
      let a ← flowDiagram depth locals (.items items)
      let b ← a.val.mapLeaves? (scopeExit locals.length)
      return ⟨b.val, compound_diagram locals items a.val b.val a.property b.property⟩
    | _ => none

theorem flow_diagram_sound {Γ : Var → Nat} (depth : Nat) (locals : LocalStore Γ)
    (control : Source.Control) (tree : FlowTree Γ)
    (accepted : (flowDiagram depth locals control).map Subtype.val = some tree) :
    FlowDiagram locals control tree := by
  cases h : flowDiagram depth locals control with
  | none => simp [h] at accepted
  | some result =>
    simp only [h, Option.map_some, Option.some.injEq] at accepted
    exact accepted ▸ result.property

/-- A complete function may return or diagnose, but may not fall through or
leak a break/continue. Scope exits are proved before discarding local storage. -/
inductive FlowReturns {Γ : Var → Nat} : FlowTree Γ → StateTree Γ → Prop where
  | returned (locals word) : FlowReturns (.value locals (.returned word)) (.value locals word)
  | checked {locals guard before after} (inner : FlowReturns before after) :
      FlowReturns (.checked locals guard before) (.checked locals guard after)
  | branch {condition yes no yes' no'} (first : FlowReturns yes yes') (second : FlowReturns no no') :
      FlowReturns (.branch condition yes no) (.branch condition yes' no')

def FlowTree.returns? {Γ : Var → Nat} (tree : FlowTree Γ) :
    Option {result // FlowReturns tree result} :=
  match tree with
  | .value locals (.returned word) => some ⟨.value locals word, .returned locals word⟩
  | .value _ _ => none
  | .checked locals guard next => do
    let inner ← next.returns?
    return ⟨.checked locals guard inner.val, .checked inner.property⟩
  | .branch condition yes no => do
    let first ← yes.returns?
    let second ← no.returns?
    return ⟨.branch condition first.val second.val, .branch first.property second.property⟩

theorem FlowReturns.sound {Γ : Var → Nat} {before : FlowTree Γ} {after : StateTree Γ}
    (diagram : FlowReturns before after) (σ : Subst Γ) :
    before.denote σ = ((after.denote σ).1, (after.denote σ).2.map Source.Flow.returned) := by
  induction diagram with
  | returned locals word => rfl
  | checked inner ih =>
    simp only [FlowTree.denote, StateTree.denote]
    cases guards σ [_] with
    | error fault => rfl
    | ok _ => exact ih
  | branch first second ih ih' =>
    simp only [FlowTree.denote, StateTree.denote]
    split <;> assumption

end EvoIR.Core.Symbolic
