import EvoIR.Core.SymbolicStorage
import EvoIR.Core.BranchExpression
import EvoIR.Core.SourceEquality
import EvoIR.Core.ScalarOperation

/-! Stateful expression diagrams retain the exact local store at every
diagnostic and successful exit. A failed later operand cannot erase an earlier
assignment. Branch trees describe expression syntax, not loop unrolling. -/
namespace EvoIR.Core.Source

abbrev StateOutcome := Locals × Result

mutual
  /-- Executable syntax depth; unlike the automatically derived `sizeOf` for
  nested syntax, this is available in the compiled checker. -/
  def expressionDepth : Expr → Nat
    | .literal .. | .ident .. => 1
    | .unary _ _ e | .cast _ _ e => 1 + expressionDepth e
    | .binary _ _ a b | .comma _ a b => 1 + max (expressionDepth a) (expressionDepth b)
    | .conditional _ c a b => 1 + max (expressionDepth c) (max (expressionDepth a) (expressionDepth b))
    | .assign _ _ _ _ place rhs => 1 + max (expressionDepth place) (expressionDepth rhs)
    | .update _ _ _ _ place => 1 + expressionDepth place
    | .call _ _ args => 1 + argumentsDepth args
  def argumentsDepth : List (QualifiedType × Expr) → Nat
    | [] => 0
    | (_, e) :: rest => max (expressionDepth e) (argumentsDepth rest)
end

def EvaluatesState (p : Program) (locals : Locals) (fid : Nat)
    (expr : Expr) (outcome : StateOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok value => Steps p ⟨.expression expr, locals, k, fid⟩
      ⟨.value value, outcome.1, k, fid⟩
  | .error fault => ∃ failedK, Steps p ⟨.expression expr, locals, k, fid⟩
      ⟨.halted (.error fault), outcome.1, failedK, fid⟩

theorem Evaluates.stateful {p : Program} {locals : Locals} {fid : Nat}
    {expr : Expr} {result : Result} (path : Evaluates p locals fid expr result) :
    EvaluatesState p locals fid expr (locals, result) := by
  cases result <;> exact path

theorem state_cast {p : Program} {locals : Locals} {fid : Nat}
    (destination : QualifiedType) (expr : Expr) (outcome : StateOutcome)
    (path : EvaluatesState p locals fid expr outcome) :
    EvaluatesState p locals fid (.cast destination.type destination expr)
      (outcome.1, outcome.2.map (Value.cast destination.type)) := by
  intro k
  have first : Step p
      ⟨.expression (.cast destination.type destination expr), locals, k, fid⟩
      ⟨.expression expr, locals, .convert destination.type :: k, fid⟩ := by
    simp [Step, step, enterExpression, wrappingConstant]
  cases result : outcome.2 with
  | error fault =>
    simp only [EvaluatesState, result] at path
    obtain ⟨failedK, steps⟩ := path (.convert destination.type :: k)
    exact ⟨failedK, .next first steps⟩
  | ok value =>
    simp only [EvaluatesState, result] at path
    exact .next first ((path (.convert destination.type :: k)).trans
      (.next rfl (.refl _)))

theorem state_comma_error {p : Program} {locals finalLocals : Locals} {fid : Nat}
    (type : IntType) (left right : Expr) (fault : Fault)
    (path : EvaluatesState p locals fid left (finalLocals, .error fault)) :
    EvaluatesState p locals fid (.comma type left right) (finalLocals, .error fault) := by
  intro k
  obtain ⟨failedK, steps⟩ := path (.binaryLeft type .comma right :: k)
  exact ⟨failedK, .next rfl steps⟩

theorem state_comma_ok {p : Program} {locals middle : Locals} {fid : Nat}
    (type : IntType) (left right : Expr) (value : Value) (outcome : StateOutcome)
    (first : EvaluatesState p locals fid left (middle, .ok value))
    (second : EvaluatesState p middle fid right outcome)
    (typed : ∀ v, outcome.2 = .ok v → v.type = type) :
    EvaluatesState p locals fid (.comma type left right) outcome := by
  intro k
  have start : Step p ⟨.expression (.comma type left right), locals, k, fid⟩
      ⟨.expression left, locals, .binaryLeft type .comma right :: k, fid⟩ := rfl
  have next : Step p ⟨.value value, middle, .binaryLeft type .comma right :: k, fid⟩
      ⟨.expression right, middle, .binaryRight type .comma value :: k, fid⟩ := rfl
  have path := (Steps.next start (first (.binaryLeft type .comma right :: k))).trans
    (.next next (.refl _))
  cases result : outcome.2 with
  | error fault =>
    simp only [EvaluatesState, result] at second ⊢
    obtain ⟨failedK, steps⟩ := second (.binaryRight type .comma value :: k)
    exact ⟨failedK, path.trans steps⟩
  | ok v =>
    simp only [EvaluatesState, result] at second ⊢
    exact path.trans ((second (.binaryRight type .comma value :: k)).trans
      (.next (by simp [Step, step, resumeValue, binaryValue, valueResult,
        typed v result, pure, Except.pure]) (.refl _)))

theorem state_assign_error {p : Program} {locals finalLocals : Locals} {fid : Nat}
    (type : IntType) (op : Assignment) (id : Nat) (destination : QualifiedType)
    (rhs : Expr) (fault : Fault)
    (valid : ∀ k, place ⟨.expression rhs, locals, k, fid⟩ id destination
      (.ident destination.type id) = true)
    (path : EvaluatesState p locals fid rhs (finalLocals, .error fault)) :
    EvaluatesState p locals fid (.assign type op id destination
      (.ident destination.type id) rhs) (finalLocals, .error fault) := by
  intro k
  obtain ⟨failedK, steps⟩ := path (.assign type op id destination :: k)
  refine ⟨failedK, .next ?_ steps⟩
  have good : place ⟨.expression (.assign type op id destination (.ident destination.type id) rhs),
      locals, k, fid⟩ id destination (.ident destination.type id) = true := valid k
  simp [Step, step, enterExpression, wrappingConstant, good]

/-- The numeric part of the existing assignment continuation, before its
destination conversion and write. This is not a new source evaluation order. -/
def assignmentCalculation (locals : Locals) (op : Assignment) (id : Nat)
    (destination : QualifiedType) (rhs : Value) : Result :=
  match op.binary with
  | none => .ok rhs
  | some binary => do binaryValue binary (← readSlot locals id destination.type) rhs

def assignmentOutcome (locals : Locals) (op : Assignment) (id : Nat)
    (destination : QualifiedType) (rhs : Value) : StateOutcome :=
  match assignmentCalculation locals op id destination rhs with
  | .error fault => (locals, .error fault)
  | .ok raw =>
    let value := raw.cast destination.type
    match writeSlot locals id destination value false with
    | .error fault => (locals, .error fault)
    | .ok updated => (updated, .ok value)

theorem state_assign_finish {p : Program} {locals middle : Locals} {fid : Nat}
    (op : Assignment) (id : Nat) (destination : QualifiedType) (rhs : Expr) (value : Value)
    (valid : ∀ k, place ⟨.expression rhs, locals, k, fid⟩ id destination
      (.ident destination.type id) = true)
    (path : EvaluatesState p locals fid rhs (middle, .ok value)) :
    EvaluatesState p locals fid (.assign destination.type op id destination
      (.ident destination.type id) rhs) (assignmentOutcome middle op id destination value) := by
  intro k
  have first : Step p
      ⟨.expression (.assign destination.type op id destination (.ident destination.type id) rhs),
        locals, k, fid⟩
      ⟨.expression rhs, locals, .assign destination.type op id destination :: k, fid⟩ := by
    have good : place ⟨.expression (.assign destination.type op id destination
        (.ident destination.type id) rhs), locals, k, fid⟩ id destination
        (.ident destination.type id) = true := valid k
    simp [Step, step, enterExpression, wrappingConstant, good]
  have steps := Steps.next first (path (.assign destination.type op id destination :: k))
  have resume : resumeValue ⟨.value value, middle, k, fid⟩ (.assign destination.type op id destination) value =
      match assignmentOutcome middle op id destination value with
      | (ls, .error fault) => ⟨.halted (.error fault), ls, k, fid⟩
      | (ls, .ok v) => ⟨.value v, ls, k, fid⟩ := by
    simp only [resumeValue, assignmentOutcome, assignmentCalculation]
    cases selected : op.binary with
    | none =>
      simp only [bind, Except.bind, pure, Except.pure]
      cases written : writeSlot middle id destination (value.cast destination.type) false <;>
        simp [fail, valueResult, Value.cast, Value.ofInt]
    | some binary =>
      cases read : readSlot middle id destination.type with
      | error fault => simp [bind, Except.bind, fail]
      | ok old =>
        simp only [bind, Except.bind]
        cases calculated : binaryValue binary old value with
        | error fault => simp [fail]
        | ok raw =>
          dsimp only
          cases written : writeSlot middle id destination (raw.cast destination.type) false <;>
            simp [pure, Except.pure, fail, valueResult, Value.cast, Value.ofInt]
  cases result : assignmentOutcome middle op id destination value with
  | mk ls answer =>
    cases answer with
    | error fault => exact ⟨k, steps.trans (.next (by simpa only [Step, step, result] using congrArg some resume) (.refl _))⟩
    | ok v => exact steps.trans (.next (by simpa only [Step, step, result] using congrArg some resume) (.refl _))

theorem state_conditional_error {p : Program} {locals finalLocals : Locals} {fid : Nat}
    (type : IntType) (condition yes no : Expr) (fault : Fault)
    (path : EvaluatesState p locals fid condition (finalLocals, .error fault)) :
    EvaluatesState p locals fid (.conditional type condition yes no)
      (finalLocals, .error fault) := by
  intro k
  obtain ⟨failedK, steps⟩ := path (.choice type yes no :: k)
  exact ⟨failedK, .next rfl steps⟩

theorem state_conditional_ok {p : Program} {locals middle : Locals} {fid : Nat}
    (type : IntType) (condition yes no : Expr) (value : Value) (outcome : StateOutcome)
    (test : EvaluatesState p locals fid condition (middle, .ok value))
    (arm : EvaluatesState p middle fid (if value.truth then yes else no) outcome) :
    EvaluatesState p locals fid (.conditional type condition yes no)
      (outcome.1, outcome.2.map (Value.cast type)) := by
  intro k
  have start : Step p ⟨.expression (.conditional type condition yes no), locals, k, fid⟩
      ⟨.expression condition, locals, .choice type yes no :: k, fid⟩ := rfl
  have next : Step p ⟨.value value, middle, .choice type yes no :: k, fid⟩
      ⟨.expression (if value.truth then yes else no), middle, .convert type :: k, fid⟩ := rfl
  have path := (Steps.next start (test (.choice type yes no :: k))).trans (.next next (.refl _))
  cases result : outcome.2 with
  | error fault =>
    simp only [EvaluatesState, result] at arm
    obtain ⟨failedK, steps⟩ := arm (.convert type :: k)
    exact ⟨failedK, path.trans steps⟩
  | ok v =>
    simp only [EvaluatesState, result] at arm
    exact path.trans ((arm (.convert type :: k)).trans (.next rfl (.refl _)))

theorem state_binary_error {p : Program} {locals finalLocals : Locals} {fid : Nat}
    (type : IntType) (op : Binary) (left right : Expr) (fault : Fault)
    (noWrap : wrappingConstant (.binary type op left right) = none)
    (path : EvaluatesState p locals fid left (finalLocals, .error fault)) :
    EvaluatesState p locals fid (.binary type op left right) (finalLocals, .error fault) := by
  intro k
  obtain ⟨failedK, steps⟩ := path (.binaryLeft type op right :: k)
  exact ⟨failedK, .next (by simp [Step, step, enterExpression, noWrap]) steps⟩

theorem state_binary_ok {p : Program} {locals middle : Locals} {fid : Nat}
    (type : IntType) (op : Binary) (left right : Expr) (value : Value) (outcome : StateOutcome)
    (noWrap : wrappingConstant (.binary type op left right) = none)
    (continueRight : (op == .and && !value.truth) = false ∧ (op == .or && value.truth) = false)
    (first : EvaluatesState p locals fid left (middle, .ok value))
    (second : EvaluatesState p middle fid right outcome)
    (typed : ∀ v result, outcome.2 = .ok v → binaryValue op value v = .ok result → result.type = type) :
    EvaluatesState p locals fid (.binary type op left right)
      (outcome.1, do let v ← outcome.2; binaryValue op value v) := by
  intro k
  have start : Step p ⟨.expression (.binary type op left right), locals, k, fid⟩
      ⟨.expression left, locals, .binaryLeft type op right :: k, fid⟩ := by
    simp [Step, step, enterExpression, noWrap]
  have next : Step p ⟨.value value, middle, .binaryLeft type op right :: k, fid⟩
      ⟨.expression right, middle, .binaryRight type op value :: k, fid⟩ := by
    simp [Step, step, resumeValue, continueRight.1, continueRight.2]
  have path := (Steps.next start (first (.binaryLeft type op right :: k))).trans (.next next (.refl _))
  cases result : outcome.2 with
  | error fault =>
    simp only [EvaluatesState, result] at second
    obtain ⟨failedK, steps⟩ := second (.binaryRight type op value :: k)
    exact ⟨failedK, path.trans steps⟩
  | ok v =>
    simp only [EvaluatesState, result] at second
    have path := path.trans (second (.binaryRight type op value :: k))
    cases calculated : binaryValue op value v with
    | error fault =>
      simp only [bind, Except.bind, calculated]
      exact ⟨k, path.trans (.next (by simp [Step, step, resumeValue, valueResult,
        calculated, fail]) (.refl _))⟩
    | ok answer =>
      simp only [bind, Except.bind, calculated]
      exact path.trans (.next (by simp [Step, step, resumeValue, valueResult,
        calculated, typed v answer result calculated]) (.refl _))

theorem state_unary_error {p : Program} {locals finalLocals : Locals} {fid : Nat}
    (type : IntType) (op : Unary) (expr : Expr) (fault : Fault)
    (noWrap : wrappingConstant (.unary type op expr) = none)
    (path : EvaluatesState p locals fid expr (finalLocals, .error fault)) :
    EvaluatesState p locals fid (.unary type op expr) (finalLocals, .error fault) := by
  intro k
  obtain ⟨failedK, steps⟩ := path (.unary type op :: k)
  exact ⟨failedK, .next (by simp [Step, step, enterExpression, noWrap]) steps⟩

theorem state_unary_ok {p : Program} {locals finalLocals : Locals} {fid : Nat}
    (type : IntType) (op : Unary) (expr : Expr) (value : Value)
    (noWrap : wrappingConstant (.unary type op expr) = none)
    (path : EvaluatesState p locals fid expr (finalLocals, .ok value))
    (typed : ∀ result, unaryValue op value = .ok result → result.type = type) :
    EvaluatesState p locals fid (.unary type op expr) (finalLocals, unaryValue op value) := by
  intro k
  have start : Step p ⟨.expression (.unary type op expr), locals, k, fid⟩
      ⟨.expression expr, locals, .unary type op :: k, fid⟩ := by
    simp [Step, step, enterExpression, noWrap]
  have steps := Steps.next start (path (.unary type op :: k))
  cases result : unaryValue op value with
  | error fault =>
    exact ⟨k, steps.trans (.next (by simp [Step, step, resumeValue, valueResult, result, fail]) (.refl _))⟩
  | ok answer =>
    exact steps.trans (.next (by simp [Step, step, resumeValue, valueResult, result,
      typed answer result]) (.refl _))

theorem state_short_skip {p : Program} {locals middle : Locals} {fid : Nat}
    (isOr : Bool) (left right : Expr) (value : Value)
    (truth : value.truth = isOr)
    (path : EvaluatesState p locals fid left (middle, .ok value)) :
    EvaluatesState p locals fid (.binary .int (if isOr then .or else .and) left right)
      (middle, .ok (.boolean isOr)) := by
  intro k
  have first : Step p
      ⟨.expression (.binary .int (if isOr then .or else .and) left right), locals, k, fid⟩
      ⟨.expression left, locals, .binaryLeft .int (if isOr then .or else .and) right :: k, fid⟩ := by
    cases isOr <;> rfl
  exact .next first ((path (.binaryLeft .int (if isOr then .or else .and) right :: k)).trans
    (.next (by cases isOr <;> simp [Step, step, resumeValue, truth, valueResult,
      Value.boolean, Value.ofNat]) (.refl _)))

theorem state_short_continue {p : Program} {locals middle : Locals} {fid : Nat}
    (isOr : Bool) (left right : Expr) (value : Value) (outcome : StateOutcome)
    (truth : value.truth = !isOr)
    (first : EvaluatesState p locals fid left (middle, .ok value))
    (second : EvaluatesState p middle fid right outcome) :
    EvaluatesState p locals fid (.binary .int (if isOr then .or else .and) left right)
      (outcome.1, outcome.2.map (fun v => Value.boolean v.truth)) := by
  have result (v : Value) : binaryValue (if isOr then .or else .and) value v =
      .ok (.boolean v.truth) := by
    cases isOr <;> simp [binaryValue, truth, pure, Except.pure]
  have calculated := state_binary_ok .int (if isOr then .or else .and) left right value outcome
    (by cases isOr <;> rfl) (by cases isOr <;> simp [truth]) first second
    (by intro v answer _ h; rw [result] at h; cases Except.ok.inj h; rfl)
  have same : (do let v ← outcome.2; binaryValue (if isOr then .or else .and) value v) =
      outcome.2.map (fun v => Value.boolean v.truth) := by
    cases outcome.2 <;> simp [bind, Except.bind, result, Except.map]
  simpa only [same] using calculated

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

inductive StateTree (Γ : Var → Nat) where
  | value (locals : LocalStore Γ) (word : CWord Γ)
  | checked (locals : LocalStore Γ) (guard : Guard Γ) (next : StateTree Γ)
  | branch (condition : Term Γ 1) (yes no : StateTree Γ)

def StateTree.denote {Γ : Var → Nat} (σ : Subst Γ) : StateTree Γ → Source.StateOutcome
  | .value locals word => (locals.denote σ, .ok (word.denote σ))
  | .checked locals guard next => match guards σ [guard] with
    | .error fault => (locals.denote σ, .error fault)
    | .ok _ => next.denote σ
  | .branch condition yes no =>
    if (denoteTerm σ condition).toNat != 0 then yes.denote σ else no.denote σ

def StateTree.prefix {Γ : Var → Nat} (locals : LocalStore Γ)
    (checks : List (Guard Γ)) (tree : StateTree Γ) : StateTree Γ :=
  checks.foldr (.checked locals) tree

theorem StateTree.prefix_sound {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ)
    (checks : List (Guard Γ)) (tree : StateTree Γ) :
    (tree.prefix locals checks).denote σ = match guards σ checks with
      | .error fault => (locals.denote σ, .error fault)
      | .ok _ => tree.denote σ := by
  induction checks with
  | nil => rfl
  | cons guard rest ih =>
    simp only [StateTree.prefix, List.foldr_cons, denote, guards]
    by_cases failed : (denoteTerm σ guard.condition).toNat == 0
    · simp only [failed, ↓reduceIte]
    · simpa only [StateTree.prefix, failed, Bool.false_eq_true, ↓reduceIte] using ih

def StateTree.linear {Γ : Var → Nat} (locals : LocalStore Γ)
    (result : ExpressionResult Γ) : StateTree Γ :=
  (StateTree.value locals result.2).prefix locals result.1

theorem StateTree.linear_sound {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ)
    (result : ExpressionResult Γ) :
    (linear locals result).denote σ = (locals.denote σ, result.denote σ) := by
  rw [linear, prefix_sound]
  unfold ExpressionResult.denote
  cases guards σ result.1 <;> rfl

/-- Every successful leaf has its own continuation; failed paths retain their
original store and never run that continuation. -/
inductive TreeBind {Γ : Var → Nat} (next : LocalStore Γ → CWord Γ → StateTree Γ → Prop) :
    StateTree Γ → StateTree Γ → Prop where
  | value {locals word output} (continued : next locals word output) :
      TreeBind next (.value locals word) output
  | checked {locals guard before after} (inner : TreeBind next before after) :
      TreeBind next (.checked locals guard before) (.checked locals guard after)
  | branch {condition yes no yes' no'}
      (first : TreeBind next yes yes') (second : TreeBind next no no') :
      TreeBind next (.branch condition yes no) (.branch condition yes' no')

theorem TreeBind.outcome {Γ : Var → Nat}
    {next : LocalStore Γ → CWord Γ → StateTree Γ → Prop} {before after : StateTree Γ}
    (diagram : TreeBind next before after) (σ : Subst Γ) :
    (∃ locals fault, before.denote σ = (locals, .error fault) ∧
      after.denote σ = (locals, .error fault)) ∨
    (∃ locals word output, next locals word output ∧
      before.denote σ = (locals.denote σ, .ok (word.denote σ)) ∧
      after.denote σ = output.denote σ) := by
  induction diagram with
  | value continued => exact .inr ⟨_, _, _, continued, rfl, rfl⟩
  | checked inner ih =>
    simp only [StateTree.denote]
    cases h : guards σ [_] with
    | error fault => exact .inl ⟨_, fault, rfl, rfl⟩
    | ok _ => exact ih
  | branch first second ih ih' =>
    simp only [StateTree.denote]
    split
    · exact ih
    · exact ih'

def StateTree.bind? {Γ : Var → Nat}
    {next : LocalStore Γ → CWord Γ → StateTree Γ → Prop}
    (build : ∀ locals word, Option {output // next locals word output})
    (tree : StateTree Γ) : Option {output // TreeBind next tree output} :=
  match tree with
  | .value locals word => do
    let r ← build locals word
    return ⟨r.val, .value r.property⟩
  | .checked locals guard inner => do
    let r ← inner.bind? build
    return ⟨.checked locals guard r.val, .checked r.property⟩
  | .branch condition yes no => do
    let a ← yes.bind? build
    let b ← no.bind? build
    return ⟨.branch condition a.val b.val, .branch a.property b.property⟩

def StateTree.HasType {Γ : Var → Nat} (type : Source.IntType) (tree : StateTree Γ) : Prop :=
  ∀ σ v, (tree.denote σ).2 = .ok v → v.type = type

def StateTree.Typed {Γ : Var → Nat} (type : Source.IntType) : StateTree Γ → Bool
  | .value _ word => word.type == type
  | .checked _ _ next => next.Typed type
  | .branch _ yes no => yes.Typed type && no.Typed type

theorem StateTree.typed_sound {Γ : Var → Nat} {type : Source.IntType} {tree : StateTree Γ}
    (accepted : tree.Typed type = true) : tree.HasType type := by
  intro σ v value
  induction tree with
  | value locals word =>
    simp only [Typed, beq_iff_eq] at accepted
    simp only [denote, Except.ok.injEq] at value
    subst v
    exact accepted
  | checked locals guard next ih =>
    simp only [denote] at value
    cases h : guards σ [guard] with
    | error fault => simp [h] at value
    | ok _ => exact ih accepted (by simpa [h] using value)
  | branch condition yes no ih ih' =>
    simp only [Typed, Bool.and_eq_true] at accepted
    simp only [denote] at value
    split at value
    · exact ih accepted.1 value
    · exact ih' accepted.2 value

def StateTree.cast {Γ : Var → Nat} (type : Source.IntType) : StateTree Γ → StateTree Γ
  | .value locals word => .value locals (word.cast type)
  | .checked locals guard next => .checked locals guard (next.cast type)
  | .branch condition yes no => .branch condition (yes.cast type) (no.cast type)

theorem StateTree.cast_sound {Γ : Var → Nat} (σ : Subst Γ)
    (type : Source.IntType) (tree : StateTree Γ) :
    (tree.cast type).denote σ =
      ((tree.denote σ).1, (tree.denote σ).2.map (Source.Value.cast type)) := by
  induction tree with
  | value locals word => simp [cast, denote, EvoIR.Core.Symbolic.cast_sound, Except.map]
  | checked locals guard next ih =>
    simp only [cast, denote]
    cases guards σ [guard] with
    | error fault => rfl
    | ok _ => exact ih
  | branch condition yes no ih ih' =>
    simp only [cast, denote]
    split <;> assumption

def StateDiagram {Γ : Var → Nat} (locals : LocalStore Γ) (expr : Source.Expr)
    (tree : StateTree Γ) : Prop :=
  ∀ σ p fid, Source.EvaluatesState p (locals.denote σ) fid expr (tree.denote σ)

def StateTree.booleanize {Γ : Var → Nat} : StateTree Γ → StateTree Γ
  | .value locals word => .value locals word.booleanize
  | .checked locals guard next => .checked locals guard next.booleanize
  | .branch condition yes no => .branch condition yes.booleanize no.booleanize

theorem StateTree.booleanize_sound {Γ : Var → Nat} (σ : Subst Γ) (tree : StateTree Γ) :
    tree.booleanize.denote σ = ((tree.denote σ).1,
      (tree.denote σ).2.map (fun v => Source.Value.boolean v.truth)) := by
  induction tree with
  | value locals word => simp [booleanize, denote, EvoIR.Core.Symbolic.booleanize_sound, Except.map]
  | checked locals guard next ih =>
    simp only [booleanize, denote]
    cases guards σ [guard] with
    | error fault => rfl
    | ok _ => exact ih
  | branch condition yes no ih ih' =>
    simp only [booleanize, denote]
    split <;> assumption

theorem state_linear {Γ : Var → Nat} (locals : LocalStore Γ)
    {expr : Source.Expr} {r : ExpressionResult Γ}
    (diagram : ExpressionDiagram locals.bindings expr r) :
    StateDiagram locals expr (.linear locals r) := by
  intro σ p fid
  rw [StateTree.linear_sound]
  exact (diagram.sound σ p (locals.denote σ) fid (local_bindings_related σ locals)).stateful

theorem state_cast_diagram {Γ : Var → Nat} (locals : LocalStore Γ)
    (destination : Source.QualifiedType) (expr : Source.Expr) (tree : StateTree Γ)
    (diagram : StateDiagram locals expr tree) :
    StateDiagram locals (.cast destination.type destination expr) (tree.cast destination.type) := by
  intro σ p fid
  rw [StateTree.cast_sound]
  exact Source.state_cast destination expr (tree.denote σ) (diagram σ p fid)

theorem state_comma_diagram {Γ : Var → Nat} (locals : LocalStore Γ) (type : Source.IntType)
    (left right : Source.Expr) (first output : StateTree Γ)
    (a : StateDiagram locals left first)
    (b : TreeBind (fun next _ tree => StateDiagram next right tree ∧ tree.HasType type)
      first output) : StateDiagram locals (.comma type left right) output := by
  intro σ p fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨diagram, typed⟩, before, after⟩
  · rw [after]
    exact Source.state_comma_error type left right fault (by simpa only [before] using a σ p fid)
  · rw [after]
    exact Source.state_comma_ok type left right (word.denote σ) (tree.denote σ)
      (by simpa only [before] using a σ p fid) (diagram σ p fid) (typed σ)

def ChoiceContinuation {Γ : Var → Nat} (type : Source.IntType) (yes no : Source.Expr)
    (locals : LocalStore Γ) (word : CWord Γ) (tree : StateTree Γ) : Prop :=
  ∃ a b, StateDiagram locals yes a ∧ StateDiagram locals no b ∧
    tree = .branch word.truthTerm (a.cast type) (b.cast type)

theorem state_choice_diagram {Γ : Var → Nat} (locals : LocalStore Γ)
    (type : Source.IntType) (condition yes no : Source.Expr) (first output : StateTree Γ)
    (a : StateDiagram locals condition first)
    (b : TreeBind (ChoiceContinuation type yes no) first output) :
    StateDiagram locals (.conditional type condition yes no) output := by
  intro σ p fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨yesTree, noTree, firstArm, secondArm, same⟩, before, after⟩
  · rw [after]
    exact Source.state_conditional_error type condition yes no fault
      (by simpa only [before] using a σ p fid)
  · rw [after, same]
    simp only [StateTree.denote, truth_term_sound]
    cases truth : (word.denote σ).truth <;> simp only [Bool.false_eq_true, ↓reduceIte,
      StateTree.cast_sound]
    · apply Source.state_conditional_ok type condition yes no (word.denote σ) (noTree.denote σ)
        (by simpa only [before] using a σ p fid)
      simpa only [truth, Bool.false_eq_true, ↓reduceIte] using secondArm σ p fid
    · apply Source.state_conditional_ok type condition yes no (word.denote σ) (yesTree.denote σ)
        (by simpa only [before] using a σ p fid)
      simpa only [truth, ↓reduceIte] using firstArm σ p fid

def AssignmentResult {Γ : Var → Nat} (locals : LocalStore Γ) (op : Source.Assignment)
    (id : Nat) (destination : Source.QualifiedType) (word : CWord Γ)
    (r : ExpressionResult Γ) : Prop :=
  ∀ σ, r.denote σ = Source.assignmentCalculation (locals.denote σ) op id destination (word.denote σ)

def assignmentResult {Γ : Var → Nat} (locals : LocalStore Γ) (op : Source.Assignment)
    (id : Nat) (destination : Source.QualifiedType) (word : CWord Γ) :
    Option {r // AssignmentResult locals op id destination word r} :=
  match binary : op.binary with
  | none => some ⟨([], word), by intro σ; simp [ExpressionResult.denote, guards,
      Source.assignmentCalculation, binary, bind, Except.bind, pure, Except.pure]⟩
  | some arithmetic => do
    let before : {v // locals.read id destination.type = some v} ←
      match read : locals.read id destination.type with
      | none => none
      | some v => some ⟨v, read⟩
    let r ← binaryResult arithmetic before.val word
    return ⟨r.val, by
      intro σ
      rw [r.property σ]
      simp [Source.assignmentCalculation, binary,
        (local_read_sound σ locals id destination.type before.val before.property).1,
        bind, Except.bind]⟩

def AssignmentContinuation {Γ : Var → Nat} (op : Source.Assignment) (id : Nat)
    (destination : Source.QualifiedType) (locals : LocalStore Γ) (word : CWord Γ)
    (tree : StateTree Γ) : Prop :=
  ∃ r updated, AssignmentResult locals op id destination word r ∧
    locals.write id destination (r.2.cast destination.type) false = some updated ∧
    tree = (StateTree.value updated (r.2.cast destination.type)).prefix locals r.1

theorem assignment_continuation_sound {Γ : Var → Nat} (σ : Subst Γ)
    {op : Source.Assignment} {id : Nat} {destination : Source.QualifiedType}
    {locals : LocalStore Γ} {word : CWord Γ} {tree : StateTree Γ}
    (accepted : AssignmentContinuation op id destination locals word tree) :
    tree.denote σ = Source.assignmentOutcome (locals.denote σ) op id destination (word.denote σ) := by
  obtain ⟨r, updated, calculation, write, rfl⟩ := accepted
  rw [StateTree.prefix_sound, Source.assignmentOutcome, ← calculation σ]
  unfold ExpressionResult.denote
  cases checks : guards σ r.1 with
  | error fault => rfl
  | ok _ =>
    simp only [bind, Except.bind, pure, Except.pure]
    have written := local_write_sound σ locals updated id destination (r.2.cast destination.type) false write
    rw [cast_sound] at written
    rw [written]
    simp only [StateTree.denote, cast_sound]

theorem state_assignment_diagram {Γ : Var → Nat} (locals : LocalStore Γ)
    (op : Source.Assignment) (id : Nat) (destination : Source.QualifiedType)
    (rhs : Source.Expr) (first output : StateTree Γ)
    (place : locals.writable id destination = true)
    (a : StateDiagram locals rhs first)
    (b : TreeBind (AssignmentContinuation op id destination) first output) :
    StateDiagram locals (.assign destination.type op id destination
      (.ident destination.type id) rhs) output := by
  intro σ p fid
  have valid k := local_writable_sound σ locals id destination place
    ⟨.expression rhs, locals.denote σ, k, fid⟩ rfl
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, continuation, before, after⟩
  · rw [after]
    exact Source.state_assign_error destination.type op id destination rhs fault valid
      (by simpa only [before] using a σ p fid)
  · rw [after, assignment_continuation_sound σ continuation]
    exact Source.state_assign_finish op id destination rhs (word.denote σ) valid
      (by simpa only [before] using a σ p fid)

def BinaryContinuation {Γ : Var → Nat} (type : Source.IntType) (op : Source.Binary)
    (word : CWord Γ) (locals : LocalStore Γ) (right : CWord Γ) (tree : StateTree Γ) : Prop :=
  ∃ r : ExpressionResult Γ, BinaryResult op word right r ∧ r.2.type = type ∧
    tree = .linear locals r

theorem binary_continuation_result {Γ : Var → Nat} (σ : Subst Γ)
    (type : Source.IntType) (op : Source.Binary) (word : CWord Γ)
    (before after : StateTree Γ)
    (bound : TreeBind (BinaryContinuation type op word) before after) :
    after.denote σ = ((before.denote σ).1,
      do let v ← (before.denote σ).2; Source.binaryValue op (word.denote σ) v) ∧
    (∀ v result, (before.denote σ).2 = .ok v →
      Source.binaryValue op (word.denote σ) v = .ok result → result.type = type) := by
  rcases bound.outcome σ with ⟨locals, fault, first, last⟩ |
      ⟨locals, right, tree, ⟨r, result, typed, shape⟩, first, last⟩
  · simp [first, last, bind, Except.bind]
  · rw [first, last, shape, StateTree.linear_sound]
    constructor
    · exact congrArg (Prod.mk (locals.denote σ)) (result σ)
    · intro v answer same calculation
      cases Except.ok.inj same
      rw [← result σ] at calculation
      rw [expression_result_value σ r answer calculation]
      exact typed

theorem state_binary_diagram {Γ : Var → Nat} (locals : LocalStore Γ)
    (type : Source.IntType) (op : Source.Binary) (left right : Source.Expr)
    (first output : StateTree Γ)
    (noWrap : Source.wrappingConstant (.binary type op left right) = none)
    (notAnd : op ≠ .and) (notOr : op ≠ .or)
    (a : StateDiagram locals left first)
    (b : TreeBind (fun next word tree => ∃ middle, StateDiagram next right middle ∧
      TreeBind (BinaryContinuation type op word) middle tree) first output) :
    StateDiagram locals (.binary type op left right) output := by
  intro σ p fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨middle, diagram, applied⟩, before, after⟩
  · rw [after]
    exact Source.state_binary_error type op left right fault noWrap
      (by simpa only [before] using a σ p fid)
  · rw [after]
    obtain ⟨result, typed⟩ := binary_continuation_result σ type op word middle tree applied
    rw [result]
    exact Source.state_binary_ok type op left right (word.denote σ) (middle.denote σ) noWrap
      (by simp [notAnd, notOr]) (by simpa only [before] using a σ p fid) (diagram σ p fid) typed

def UnaryContinuation {Γ : Var → Nat} (type : Source.IntType) (op : Source.Unary)
    (locals : LocalStore Γ) (word : CWord Γ) (tree : StateTree Γ) : Prop :=
  (unary op word).2.type = type ∧ tree = .linear locals (unary op word)

theorem state_unary_diagram {Γ : Var → Nat} (locals : LocalStore Γ)
    (type : Source.IntType) (op : Source.Unary) (expr : Source.Expr) (first output : StateTree Γ)
    (noWrap : Source.wrappingConstant (.unary type op expr) = none)
    (a : StateDiagram locals expr first)
    (b : TreeBind (UnaryContinuation type op) first output) :
    StateDiagram locals (.unary type op expr) output := by
  intro σ p fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨typed, shape⟩, before, after⟩
  · rw [after]
    exact Source.state_unary_error type op expr fault noWrap
      (by simpa only [before] using a σ p fid)
  · rw [after, shape, StateTree.linear_sound]
    have result : ExpressionResult.denote σ (unary op word) = Source.unaryValue op (word.denote σ) :=
      unary_sound σ op word
    rw [result]
    apply Source.state_unary_ok type op expr (word.denote σ) noWrap
      (by simpa only [before] using a σ p fid)
    intro answer calculated
    rw [← result] at calculated
    rw [expression_result_value σ (unary op word) answer calculated]
    exact typed

def shortTree {Γ : Var → Nat} (isOr : Bool) (locals : LocalStore Γ)
    (word : CWord Γ) (right : StateTree Γ) : StateTree Γ :=
  let skip := StateTree.value locals (CWord.constant (.boolean isOr))
  if isOr then .branch word.truthTerm skip right.booleanize
  else .branch word.truthTerm right.booleanize skip

def ShortContinuation {Γ : Var → Nat} (isOr : Bool) (right : Source.Expr)
    (locals : LocalStore Γ) (word : CWord Γ) (tree : StateTree Γ) : Prop :=
  ∃ r, StateDiagram locals right r ∧ tree = shortTree isOr locals word r

theorem state_short_diagram {Γ : Var → Nat} (locals : LocalStore Γ) (isOr : Bool)
    (left right : Source.Expr) (first output : StateTree Γ)
    (a : StateDiagram locals left first)
    (b : TreeBind (ShortContinuation isOr right) first output) :
    StateDiagram locals (.binary .int (if isOr then .or else .and) left right) output := by
  intro σ p fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨r, diagram, shape⟩, before, after⟩
  · rw [after]
    exact Source.state_binary_error .int (if isOr then .or else .and) left right fault
      (by cases isOr <;> rfl) (by simpa only [before] using a σ p fid)
  · have initial := a σ p fid
    rw [before] at initial
    rw [after, shape]
    cases isOr <;> cases truth : (word.denote σ).truth <;>
      simp only [shortTree, StateTree.denote, truth_term_sound, truth,
        Bool.false_eq_true, ↓reduceIte, StateTree.booleanize_sound]
    · exact Source.state_short_skip false left right (word.denote σ) truth initial
    · exact Source.state_short_continue false left right (word.denote σ) (r.denote σ)
        truth initial (diagram σ p fid)
    · exact Source.state_short_continue true left right (word.denote σ) (r.denote σ)
        truth initial (diagram σ p fid)
    · exact Source.state_short_skip true left right (word.denote σ) truth initial

def shortExpression {Γ : Var → Nat}
    (build : ∀ locals expr, Option {tree // StateDiagram (Γ := Γ) locals expr tree})
    (locals : LocalStore Γ) (type : Source.IntType) (isOr : Bool) (left right : Source.Expr) :
    Option {tree // StateDiagram locals (.binary type (if isOr then .or else .and) left right) tree} := do
  if typed : type = .int then
    let a ← build locals left
    let b ← a.val.bind? (next := ShortContinuation isOr right) (fun next word => do
      let r ← build next right
      return ⟨shortTree isOr next word r.val, r.val, r.property, rfl⟩)
    return ⟨b.val, by simpa only [typed] using
      state_short_diagram locals isOr left right a.val b.val a.property b.property⟩
  else none

def updateBinary (op : Source.Update) : Source.Binary :=
  if op == .preInc || op == .postInc then .add else .sub

def updatePost (op : Source.Update) : Bool := op == .postInc || op == .postDec

theorem state_update_diagram {Γ : Var → Nat} (locals updated : LocalStore Γ)
    (op : Source.Update) (id : Nat) (destination : Source.QualifiedType)
    (before : CWord Γ) (r : ExpressionResult Γ)
    (place : locals.writable id destination = true)
    (read : locals.read id destination.type = some before)
    (calculation : BinaryResult (updateBinary op) before (CWord.constant (.ofNat .int 1)) r)
    (write : locals.write id destination (r.2.cast destination.type) false = some updated) :
    StateDiagram locals (.update destination.type op id destination (.ident destination.type id))
      ((StateTree.value updated (if updatePost op then before else r.2.cast destination.type)).prefix locals r.1) := by
  intro σ p fid
  have readValue := local_read_sound σ locals id destination.type before read
  have written := local_write_sound σ locals updated id destination (r.2.cast destination.type) false write
  rw [cast_sound] at written
  have computed := calculation σ
  have one : (CWord.constant (Γ := Γ) (.ofNat .int 1)).denote σ = Source.Value.ofNat .int 1 := rfl
  rw [one] at computed
  unfold ExpressionResult.denote at computed
  rw [StateTree.prefix_sound]
  cases checked : guards σ r.1 with
  | error fault =>
    simp only [checked, bind, Except.bind] at computed
    intro k
    have valid := local_writable_sound σ locals id destination place
      ⟨.expression (.update destination.type op id destination (.ident destination.type id)),
        locals.denote σ, k, fid⟩ rfl
    refine ⟨k, .next ?_ (.refl _)⟩
    simp only [Source.Step, Source.step, Source.enterExpression, Source.wrappingConstant,
      valid, Bool.not_true, Bool.false_eq_true, ↓reduceIte, readValue.1, bind, Except.bind]
    change some (match (do
      let value ← Source.binaryValue (updateBinary op) (before.denote σ) (Source.Value.ofNat .int 1)
      let ls ← Source.writeSlot (locals.denote σ) id destination (value.cast destination.type) false
      pure (if updatePost op then before.denote σ else value.cast destination.type, ls)) with
      | .error f => Source.fail ⟨.expression _, locals.denote σ, k, fid⟩ f
      | .ok (v, ls) => Source.valueResult ⟨.expression _, ls, k, fid⟩ destination.type (.ok v)) = _
    rw [← computed]
    rfl
  | ok unitValue =>
    simp only [checked, bind, Except.bind, pure, Except.pure] at computed
    intro k
    have valid := local_writable_sound σ locals id destination place
      ⟨.expression (.update destination.type op id destination (.ident destination.type id)),
        locals.denote σ, k, fid⟩ rfl
    have type : (before.denote σ).type = destination.type := readValue.2
    have step : Source.Step p
        ⟨.expression (.update destination.type op id destination (.ident destination.type id)),
          locals.denote σ, k, fid⟩
        ⟨.value ((if updatePost op then before else r.2.cast destination.type).denote σ),
          updated.denote σ, k, fid⟩ := by
      simp only [Source.Step, Source.step, Source.enterExpression, Source.wrappingConstant,
        valid, Bool.not_true, Bool.false_eq_true, ↓reduceIte, readValue.1, bind, Except.bind]
      have operation : Source.binaryValue (if op == .preInc || op == .postInc then .add else .sub)
          (before.denote σ) (Source.Value.ofNat .int 1) = .ok (r.2.denote σ) := computed.symm
      rw [operation]
      simp only [written, pure, Except.pure, Source.valueResult]
      cases post : updatePost op <;>
        simp_all [updatePost, cast_sound, Source.Value.cast, Source.Value.ofInt]
    exact .next step (.refl _)

def updateExpression {Γ : Var → Nat} (locals : LocalStore Γ) (type : Source.IntType)
    (op : Source.Update) (id : Nat) (destination : Source.QualifiedType)
    (targetType : Source.IntType) (targetId : Nat) :
    Option {tree // StateDiagram locals (.update type op id destination (.ident targetType targetId)) tree} := do
  if typed : type = destination.type ∧ targetType = destination.type ∧ targetId = id then
    if place : locals.writable id destination = true then
      let before : {word // locals.read id destination.type = some word} ←
        match read : locals.read id destination.type with
        | none => none
        | some word => some ⟨word, read⟩
      let r ← binaryResult (updateBinary op) before.val (CWord.constant (.ofNat .int 1))
      match write : locals.write id destination (r.val.2.cast destination.type) false with
      | none => none
      | some updated => return ⟨(StateTree.value updated
          (if updatePost op then before.val else r.val.2.cast destination.type)).prefix locals r.val.1,
          by simpa only [typed.1, typed.2.1, typed.2.2] using
            state_update_diagram locals updated op id destination before.val r.val place before.property r.property write⟩
    else none
  else none

/-- The bound is syntax traversal only. It never accepts a loop by running it
for finitely many steps; loop simulation is a separate CFG obligation. -/
def stateExpression {Γ : Var → Nat} : (depth : Nat) → (locals : LocalStore Γ) →
    (expr : Source.Expr) → Option {tree // StateDiagram locals expr tree}
  | 0, _, _ => none
  | depth+1, locals, expr =>
    match expression locals.bindings expr with
    | some r => some ⟨.linear locals r.val, state_linear locals r.property⟩
    | none => match expr with
      | .update type op id destination (.ident targetType targetId) =>
        updateExpression locals type op id destination targetType targetId
      | .unary type op operand => do
        if noWrap : Source.wrappingConstant (.unary type op operand) = none then
          let a ← stateExpression depth locals operand
          let b ← a.val.bind? (next := UnaryContinuation type op) (fun next word => do
            if typed : (unary op word).2.type = type then
              return ⟨.linear next (unary op word), typed, rfl⟩
            else none)
          return ⟨b.val, state_unary_diagram locals type op operand a.val b.val noWrap
            a.property b.property⟩
        else none
      | .binary type .and left right => shortExpression (stateExpression depth) locals type false left right
      | .binary type .or left right => shortExpression (stateExpression depth) locals type true left right
      | .binary type op left right => do
        if noWrap : Source.wrappingConstant (.binary type op left right) = none then
          if notAnd : op ≠ .and then
            if notOr : op ≠ .or then
              let a ← stateExpression depth locals left
              let b ← a.val.bind? (next := fun next word tree => ∃ middle,
                  StateDiagram next right middle ∧ TreeBind (BinaryContinuation type op word) middle tree)
                  (fun next word => do
                let r ← stateExpression depth next right
                let applied ← r.val.bind? (next := BinaryContinuation type op word) (fun finalLocals v => do
                  let result ← binaryResult op word v
                  if typed : result.val.2.type = type then
                    return ⟨.linear finalLocals result.val, result.val, result.property, typed, rfl⟩
                  else none)
                return ⟨applied.val, r.val, r.property, applied.property⟩)
              return ⟨b.val, state_binary_diagram locals type op left right a.val b.val
                noWrap notAnd notOr a.property b.property⟩
            else none
          else none
        else none
      | .cast type destination operand => do
        if typed : type = destination.type then
          let r ← stateExpression depth locals operand
          return ⟨r.val.cast destination.type, typed ▸ state_cast_diagram locals destination
            operand r.val r.property⟩
        else none
      | .comma type left right => do
        let a ← stateExpression depth locals left
        let b ← a.val.bind? (next := fun next _ tree =>
            StateDiagram next right tree ∧ tree.HasType type) (fun next _ => do
          let r ← stateExpression depth next right
          if typed : r.val.Typed type = true then
            return ⟨r.val, r.property, StateTree.typed_sound typed⟩
          else none)
        return ⟨b.val, state_comma_diagram locals type left right a.val b.val a.property b.property⟩
      | .assign type op id destination (.ident targetType targetId) rhs => do
        if typed : type = destination.type then
          if targetEq : targetType = destination.type ∧ targetId = id then
            if place : locals.writable id destination = true then
              let a ← stateExpression depth locals rhs
              let b ← a.val.bind? (next := AssignmentContinuation op id destination) (fun next word => do
                let r ← assignmentResult next op id destination word
                match written : next.write id destination (r.val.2.cast destination.type) false with
                | none => none
                | some updated => return ⟨(StateTree.value updated (r.val.2.cast destination.type)).prefix next r.val.1,
                    r.val, updated, r.property, written, rfl⟩)
              return ⟨b.val, by
                simpa only [typed, targetEq.1, targetEq.2] using
                  state_assignment_diagram locals op id destination rhs a.val b.val place a.property b.property⟩
            else none
          else none
        else none
      | .conditional type condition yes no => do
        let a ← stateExpression depth locals condition
        let b ← a.val.bind? (next := ChoiceContinuation type yes no) (fun next word => do
          let first ← stateExpression depth next yes
          let second ← stateExpression depth next no
          return ⟨.branch word.truthTerm (first.val.cast type) (second.val.cast type),
            first.val, second.val, first.property, second.property, rfl⟩)
        return ⟨b.val, state_choice_diagram locals type condition yes no a.val b.val a.property b.property⟩
      | _ => none

theorem state_expression_sound {Γ : Var → Nat} (depth : Nat) (locals : LocalStore Γ)
    (expr : Source.Expr) (tree : StateTree Γ)
    (accepted : (stateExpression depth locals expr).map Subtype.val = some tree) :
    StateDiagram locals expr tree := by
  cases built : stateExpression depth locals expr with
  | none => simp [built] at accepted
  | some result =>
    simp only [built, Option.map_some, Option.some.injEq] at accepted
    subst tree
    exact result.property

end EvoIR.Core.Symbolic
