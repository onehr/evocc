import EvoIR.Core.CallExpression

/-! Expression composition around actual calls. Successful operands restore
the caller; a fault in any operand retains the callee's halted activation. -/
namespace EvoIR.Core.Source

theorem context_frame_error {p : Program} {locals finalLocals : Locals} {fid : Nat}
    (outer inner : Expr) (frame : Frame) (fault : Fault)
    (start : ∀ k, Step p ⟨.expression outer, locals, k, fid⟩
      ⟨.expression inner, locals, frame :: k, fid⟩)
    (path : EvaluatesContext p locals fid inner (finalLocals, .error fault)) :
    EvaluatesContext p locals fid outer (finalLocals, .error fault) := by
  intro k
  obtain ⟨ls, failedK, failedFid, steps⟩ := path (frame :: k)
  exact ⟨ls, failedK, failedFid, .next (start k) steps⟩

theorem context_comma_ok {p : Program} {locals middle : Locals} {fid : Nat}
    (type : IntType) (left right : Expr) (value : Value) (outcome : StateOutcome)
    (first : EvaluatesContext p locals fid left (middle, .ok value))
    (second : EvaluatesContext p middle fid right outcome)
    (typed : ∀ v, outcome.2 = .ok v → v.type = type) :
    EvaluatesContext p locals fid (.comma type left right) outcome := by
  cases result : outcome.2 with
  | ok v =>
    have old : EvaluatesState p middle fid right (outcome.1, .ok v) := by
      simpa only [EvaluatesState, EvaluatesContext, result] using second
    have same : outcome = (outcome.1, .ok v) := Prod.ext rfl result
    rw [same]
    exact (state_comma_ok type left right value _ first old
      (by simpa only [result] using typed)).inContext
  | error fault =>
    intro k
    simp only [EvaluatesContext, result] at second ⊢
    obtain ⟨ls, failedK, failedFid, tail⟩ := second (.binaryRight type .comma value :: k)
    exact ⟨ls, failedK, failedFid, .next rfl
      ((first (.binaryLeft type .comma right :: k)).trans (.next rfl tail))⟩

theorem context_conditional_ok {p : Program} {locals middle : Locals} {fid : Nat}
    (type : IntType) (condition yes no : Expr) (value : Value) (outcome : StateOutcome)
    (test : EvaluatesContext p locals fid condition (middle, .ok value))
    (arm : EvaluatesContext p middle fid (if value.truth then yes else no) outcome) :
    EvaluatesContext p locals fid (.conditional type condition yes no)
      (outcome.1, outcome.2.map (Value.cast type)) := by
  cases result : outcome.2 with
  | ok v =>
    have old : EvaluatesState p middle fid (if value.truth then yes else no) (outcome.1, .ok v) := by
      simpa only [EvaluatesState, EvaluatesContext, result] using arm
    simpa only [result] using (state_conditional_ok type condition yes no value _ test old).inContext
  | error fault =>
    intro k
    simp only [EvaluatesContext, result] at arm
    obtain ⟨ls, failedK, failedFid, tail⟩ := arm (.convert type :: k)
    exact ⟨ls, failedK, failedFid, .next rfl
      ((test (.choice type yes no :: k)).trans (.next rfl tail))⟩

theorem context_short_continue {p : Program} {locals middle : Locals} {fid : Nat}
    (isOr : Bool) (left right : Expr) (value : Value) (outcome : StateOutcome)
    (truth : value.truth = !isOr)
    (first : EvaluatesContext p locals fid left (middle, .ok value))
    (second : EvaluatesContext p middle fid right outcome) :
    EvaluatesContext p locals fid (.binary .int (if isOr then .or else .and) left right)
      (outcome.1, outcome.2.map (fun v => Value.boolean v.truth)) := by
  have result (v : Value) : binaryValue (if isOr then .or else .and) value v =
      .ok (.boolean v.truth) := by
    cases isOr <;> simp [binaryValue, truth, pure, Except.pure]
  have calculated := context_binary_ok .int (if isOr then .or else .and) left right value outcome
    (by cases isOr <;> rfl) (by cases isOr <;> simp [truth]) first second
    (by intro v answer _ h; rw [result] at h; cases Except.ok.inj h; rfl)
  have same : (do let v ← outcome.2; binaryValue (if isOr then .or else .and) value v) =
      outcome.2.map (fun v => Value.boolean v.truth) := by
    cases outcome.2 <;> simp [bind, Except.bind, result, Except.map]
  simpa only [same] using calculated

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

theorem context_comma_diagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (type : Source.IntType) (left right : Source.Expr) (first output : StateTree Γ)
    (a : ContextDiagram p locals left first)
    (b : TreeBind (fun next _ tree => ContextDiagram p next right tree ∧ tree.HasType type)
      first output) : ContextDiagram p locals (.comma type left right) output := by
  intro σ fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨diagram, typed⟩, before, after⟩
  · rw [after]
    exact Source.context_frame_error _ left (.binaryLeft type .comma right) fault
      (by intros; rfl) (by simpa only [before] using a σ fid)
  · rw [after]
    exact Source.context_comma_ok type left right (word.denote σ) (tree.denote σ)
      (by simpa only [before] using a σ fid) (diagram σ fid) (typed σ)

theorem context_unary_diagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (type : Source.IntType) (op : Source.Unary) (expr : Source.Expr) (first output : StateTree Γ)
    (noWrap : Source.wrappingConstant (.unary type op expr) = none)
    (a : ContextDiagram p locals expr first)
    (b : TreeBind (UnaryContinuation type op) first output) :
    ContextDiagram p locals (.unary type op expr) output := by
  intro σ fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨typed, shape⟩, before, after⟩
  · rw [after]
    exact Source.context_frame_error _ expr (.unary type op) fault
      (by intros; simp [Source.Step, Source.step, Source.enterExpression, noWrap])
      (by simpa only [before] using a σ fid)
  · rw [after, shape, StateTree.linear_sound]
    have result : ExpressionResult.denote σ (unary op word) = Source.unaryValue op (word.denote σ) :=
      unary_sound σ op word
    rw [result]
    apply Source.EvaluatesState.inContext
    apply Source.state_unary_ok type op expr (word.denote σ) noWrap
      (by simpa only [before, Source.EvaluatesContext, Source.EvaluatesState] using a σ fid)
    intro answer calculated
    rw [← result] at calculated
    rw [expression_result_value σ (unary op word) answer calculated]
    exact typed

theorem context_assignment_diagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (op : Source.Assignment) (id : Nat) (destination : Source.QualifiedType)
    (rhs : Source.Expr) (first output : StateTree Γ)
    (place : locals.writable id destination = true)
    (a : ContextDiagram p locals rhs first)
    (b : TreeBind (AssignmentContinuation op id destination) first output) :
    ContextDiagram p locals (.assign destination.type op id destination
      (.ident destination.type id) rhs) output := by
  intro σ fid
  have valid k := local_writable_sound σ locals id destination place
    ⟨.expression rhs, locals.denote σ, k, fid⟩ rfl
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, continuation, before, after⟩
  · rw [after]
    apply Source.context_frame_error _ rhs (.assign destination.type op id destination) fault
      ?_ (by simpa only [before] using a σ fid)
    intro k
    have good : Source.place ⟨.expression (.assign destination.type op id destination
        (.ident destination.type id) rhs), locals.denote σ, k, fid⟩
        id destination (.ident destination.type id) = true := valid k
    simp [Source.Step, Source.step, Source.enterExpression, Source.wrappingConstant, good]
  · rw [after, assignment_continuation_sound σ continuation]
    exact (Source.state_assign_finish op id destination rhs (word.denote σ) valid
      (by simpa only [before, Source.EvaluatesContext, Source.EvaluatesState] using a σ fid)).inContext

def ContextChoice {Γ : Var → Nat} (p : Source.Program) (type : Source.IntType)
    (yes no : Source.Expr) (locals : LocalStore Γ) (word : CWord Γ) (tree : StateTree Γ) : Prop :=
  ∃ a b, ContextDiagram p locals yes a ∧ ContextDiagram p locals no b ∧
    tree = .branch word.truthTerm (a.cast type) (b.cast type)

theorem context_choice_diagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (type : Source.IntType) (condition yes no : Source.Expr) (first output : StateTree Γ)
    (a : ContextDiagram p locals condition first)
    (b : TreeBind (ContextChoice p type yes no) first output) :
    ContextDiagram p locals (.conditional type condition yes no) output := by
  intro σ fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨yesTree, noTree, firstArm, secondArm, same⟩, before, after⟩
  · rw [after]
    exact Source.context_frame_error _ condition (.choice type yes no) fault
      (by intros; rfl) (by simpa only [before] using a σ fid)
  · rw [after, same]
    simp only [StateTree.denote, truth_term_sound]
    cases truth : (word.denote σ).truth <;> simp only [Bool.false_eq_true, ↓reduceIte,
      StateTree.cast_sound]
    · apply Source.context_conditional_ok type condition yes no (word.denote σ) (noTree.denote σ)
        (by simpa only [before] using a σ fid)
      simpa only [truth, Bool.false_eq_true, ↓reduceIte] using secondArm σ fid
    · apply Source.context_conditional_ok type condition yes no (word.denote σ) (yesTree.denote σ)
        (by simpa only [before] using a σ fid)
      simpa only [truth, ↓reduceIte] using firstArm σ fid

def ContextShort {Γ : Var → Nat} (p : Source.Program) (isOr : Bool) (right : Source.Expr)
    (locals : LocalStore Γ) (word : CWord Γ) (tree : StateTree Γ) : Prop :=
  ∃ r, ContextDiagram p locals right r ∧ tree = shortTree isOr locals word r

theorem context_short_diagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (isOr : Bool) (left right : Source.Expr) (first output : StateTree Γ)
    (a : ContextDiagram p locals left first)
    (b : TreeBind (ContextShort p isOr right) first output) :
    ContextDiagram p locals (.binary .int (if isOr then .or else .and) left right) output := by
  intro σ fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨r, diagram, shape⟩, before, after⟩
  · rw [after]
    exact Source.context_binary_error .int (if isOr then .or else .and) left right fault
      (by cases isOr <;> rfl) (by simpa only [before] using a σ fid)
  · have initial := a σ fid
    rw [before] at initial
    rw [after, shape]
    cases isOr <;> cases truth : (word.denote σ).truth <;>
      simp only [shortTree, StateTree.denote, truth_term_sound, truth,
        Bool.false_eq_true, ↓reduceIte, StateTree.booleanize_sound]
    · exact (Source.state_short_skip false left right (word.denote σ) truth initial).inContext
    · exact Source.context_short_continue false left right (word.denote σ) (r.denote σ)
        truth initial (diagram σ fid)
    · exact Source.context_short_continue true left right (word.denote σ) (r.denote σ)
        truth initial (diagram σ fid)
    · exact (Source.state_short_skip true left right (word.denote σ) truth initial).inContext

def contextShortExpression {Γ : Var → Nat} (p : Source.Program)
    (build : ∀ locals expr, Option {tree // ContextDiagram (Γ := Γ) p locals expr tree})
    (locals : LocalStore Γ) (type : Source.IntType) (isOr : Bool) (left right : Source.Expr) :
    Option {tree // ContextDiagram p locals (.binary type (if isOr then .or else .and) left right) tree} := do
  if typed : type = .int then
    let a ← build locals left
    let b ← a.val.bind? (next := ContextShort p isOr right) (fun next word => do
      let r ← build next right
      return ⟨shortTree isOr next word r.val, r.val, r.property, rfl⟩)
    return ⟨b.val, by simpa only [typed] using
      context_short_diagram p locals isOr left right a.val b.val a.property b.property⟩
  else none

end EvoIR.Core.Symbolic
