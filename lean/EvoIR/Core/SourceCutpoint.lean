import EvoIR.Core.StatementDiagram
import EvoIR.Core.SavedFrames
import EvoIR.Core.CutTree
import EvoIR.Core.CheckpointSimulation

/-! Exact source states at cyclic cutpoints. Administrative transitions use
the original source machine, including scope and loop-control continuations.
Symbolic expressions retain their ordered diagnostics and local writes. -/
namespace EvoIR.Core.Symbolic

inductive CutControl (Γ : Var → Nat) where
  | expression (expr : Source.Expr)
  | statement (stmt : Source.Stmt)
  | items (items : List Source.BlockItem)
  | object (object : Source.Object)
  | value (word : CWord Γ)
  | call (callee : Nat) (type : Source.IntType) (arguments : List (CWord Γ))
  | flow (flow : SymbolicFlow Γ)
  | returned (word : CWord Γ)

def CutControl.denote {Γ : Var → Nat} (σ : Subst Γ) : CutControl Γ → Source.Control
  | .expression e => .expression e
  | .statement s => .statement s
  | .items xs => .items xs
  | .object o => .object o
  | .value w => .value (w.denote σ)
  | .call callee type arguments => .call callee type (arguments.map (CWord.denote σ))
  | .flow f => .flow (f.denote σ)
  | .returned w => .halted (.ok (w.denote σ))

structure SourceCursor (Γ : Var → Nat) where
  control : CutControl Γ
  locals : LocalStore Γ
  continuation : SavedFrames Γ

/-- The actual suspended caller, not an observation substituted for a return. -/
structure SourceReturnContext where
  functionId : Nat
  locals : Source.Locals
  continuation : List Source.Frame

def sourceContextFrames (context : Option SourceReturnContext)
    (frames : List Source.Frame) : List Source.Frame :=
  match context with
  | none => frames
  | some caller => frames.map (fun frame => match frame with
      | .function none type => .function (some (caller.functionId, caller.locals)) type
      | frame => frame) ++ caller.continuation

def SourceCursor.denote {Γ : Var → Nat} (σ : Subst Γ) (fid : Nat)
    (cursor : SourceCursor Γ) (context : Option SourceReturnContext := none) : Source.State :=
  match context with
  | none => ⟨cursor.control.denote σ, cursor.locals.denote σ, cursor.continuation.denote σ, fid⟩
  | some caller =>
    match cursor.control with
    | .returned word => ⟨.value (word.denote σ), caller.locals,
        sourceContextFrames context (cursor.continuation.denote σ), caller.functionId⟩
    | _ => ⟨cursor.control.denote σ, cursor.locals.denote σ,
        sourceContextFrames context (cursor.continuation.denote σ), fid⟩

inductive SourceSite where
  | expression (expr : Source.Expr) (continuation : List Source.Frame)
  | statement (stmt : Source.Stmt) (continuation : List Source.Frame)
  deriving DecidableEq

def SourceCursor.site {Γ : Var → Nat} (cursor : SourceCursor Γ) : Option SourceSite :=
  match cursor.control with
  | .expression e => (cursor.continuation.mapM Source.Frame.static?).map (.expression e)
  | .statement s => (cursor.continuation.mapM Source.Frame.static?).map (.statement s)
  | _ => none

def SourceCutRuns {Γ : Var → Nat} (p : Source.Program) (advance : Bool) (cursor : SourceCursor Γ)
    (tree : CutTree Γ (SourceCursor Γ)) : Prop :=
  ∀ σ fid context, ∃ final, Source.Steps p (cursor.denote σ fid context) final ∧
    (advance = true → Relation.TransGen (Transition.Predecessor (Source.step p)) final
      (cursor.denote σ fid context)) ∧
    match tree.eval σ with
    | .error fault => final.control = .halted (.error fault)
    | .ok exit => final = exit.denote σ fid context

/-- One real administrative step. No unsupported source case becomes an
uninterpreted transition. Branch leaves describe the actually selected state. -/
def CursorStep {Γ : Var → Nat} (cursor : SourceCursor Γ)
    (tree : CutTree Γ (SourceCursor Γ)) : Prop :=
  ∀ σ p fid context, match tree.eval σ with
    | .error _ => False
    | .ok exit => Source.Step p (cursor.denote σ fid context) (exit.denote σ fid context)

def cursorStatement {Γ : Var → Nat} (locals : LocalStore Γ) (k : SavedFrames Γ)
    (stmt : Source.Stmt) : SourceCursor Γ :=
  match stmt with
  | .null => ⟨.flow .normal, locals, k⟩
  | .break => ⟨.flow .break, locals, k⟩
  | .continue => ⟨.flow .continue, locals, k⟩
  | .expression e => ⟨.expression e, locals, .discard :: k⟩
  | .return t e => ⟨.expression e, locals, .return t :: k⟩
  | .compound items => ⟨.items items, locals, .scope locals.length :: k⟩
  | .ite e yes no => ⟨.expression e, locals, .branch yes no :: k⟩
  | .while e body => ⟨.expression e, locals, .loopTest e none body :: k⟩
  | .doWhile body e => ⟨.statement body, locals, .loopBody e none body :: k⟩
  | .for init condition increment body =>
    let first := match init with
      | .empty => CutControl.flow .normal
      | .expression e => .statement (.expression e)
      | .declarations objects => .items (objects.map Source.BlockItem.declaration)
    ⟨first, locals, .startLoop (condition.getD (.literal .int 1)) increment body ::
      .scope locals.length :: k⟩

theorem cursor_statement_sound {Γ : Var → Nat} (locals : LocalStore Γ)
    (k : SavedFrames Γ) (stmt : Source.Stmt) :
    CursorStep ⟨.statement stmt, locals, k⟩ (.leaf (cursorStatement locals k stmt)) := by
  intro σ p fid context
  cases context <;> cases stmt <;> simp [CutTree.eval, cursorStatement, Source.Step,
    Source.step, Source.enterStatement, SourceCursor.denote, CutControl.denote,
    SymbolicFlow.denote, LocalStore.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map]
  all_goals split <;> rfl

def cursorObject {Γ : Var → Nat} (locals : LocalStore Γ) (k : SavedFrames Γ)
    (object : Source.Object) :
    Option {tree // CursorStep ⟨.object object, locals, k⟩ tree} := do
  let fresh : {ls // locals.declare object.symbol object.type = some ls} ←
    match declared : locals.declare object.symbol object.type with
    | none => none
    | some ls => some ⟨ls, declared⟩
  match initialized : object.initializer with
  | none =>
    return ⟨.leaf ⟨.flow .normal, fresh.val, k⟩, by
      intro σ p fid context
      have started := local_declare_sound σ locals fresh.val object initialized fresh.property
        ((SourceCursor.mk (.object object) locals k).denote σ fid context)
        (by cases context <;> rfl)
      cases context <;>
        simpa [CutTree.eval, Source.Step, Source.step, SourceCursor.denote,
          CutControl.denote, SymbolicFlow.denote] using congrArg some started⟩
  | some (destination, expr) =>
    if same : destination = object.type then
      return ⟨.leaf ⟨.expression expr, fresh.val, .initialize object.symbol destination :: k⟩, by
        intro σ p fid context
        have declared := fresh.property
        unfold LocalStore.declare at declared
        split at declared
        · contradiction
        · rename_i unique
          have duplicate : (locals.denote σ).any (·.symbol == object.symbol) = false := by
            simpa [LocalStore.denote, List.any_map, Function.comp_def, LocalSlot.denote]
              using Bool.eq_false_iff.mpr unique
          have shape := Option.some.inj declared
          cases context <;>
            simp only [CutTree.eval, Source.Step, Source.step, Source.enterObject,
              SourceCursor.denote, CutControl.denote, duplicate, initialized, same,
              Bool.false_eq_true, ↓reduceIte, bne_self_eq_false, ← shape] <;>
            simp [sourceContextFrames, SavedFrames.denote, Source.Frame.map,
              LocalStore.denote, LocalSlot.denote]⟩
    else none

def cursorFlow {Γ : Var → Nat} (locals : LocalStore Γ) (k : SavedFrames Γ)
    (flow : SymbolicFlow Γ) :
    Option {tree // CursorStep ⟨.flow flow, locals, k⟩ tree} :=
  match k with
  | [] => none
  | .scope n :: rest =>
    match h : locals.exitScope n with
    | none => none
    | some updated => some ⟨.leaf ⟨.flow flow, updated, rest⟩, by
        intro σ p fid context
        cases context with
        | none => exact congrArg some (local_scope_exit_sound σ locals updated n h
            ⟨.flow (flow.denote σ), locals.denote σ, SavedFrames.denote σ rest, fid⟩ rfl (flow.denote σ))
        | some caller => exact congrArg some (local_scope_exit_sound σ locals updated n h
            ⟨.flow (flow.denote σ), locals.denote σ,
              sourceContextFrames (some caller) (SavedFrames.denote σ rest), fid⟩ rfl (flow.denote σ))⟩
  | .sequence items :: rest =>
    match flow with
    | .normal => some ⟨.leaf ⟨.items items, locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
    | .break => some ⟨.leaf ⟨.flow .break, locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
    | .continue => some ⟨.leaf ⟨.flow .continue, locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
    | .returned w => some ⟨.leaf ⟨.flow (.returned w), locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
  | .startLoop condition increment body :: rest =>
    match flow with
    | .normal => some ⟨.leaf ⟨.expression condition, locals,
        .loopTest condition increment body :: rest⟩, by intro σ p fid context; cases context <;> rfl⟩
    | .break => some ⟨.leaf ⟨.flow .break, locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
    | .continue => some ⟨.leaf ⟨.flow .continue, locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
    | .returned w => some ⟨.leaf ⟨.flow (.returned w), locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
  | .loopBody condition increment body :: rest =>
    match flow with
    | .break => some ⟨.leaf ⟨.flow .normal, locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
    | .returned w => some ⟨.leaf ⟨.flow (.returned w), locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
    | .normal | .continue =>
      match increment with
      | none => some ⟨.leaf ⟨.expression condition, locals,
          .loopTest condition none body :: rest⟩, by intro σ p fid context; cases context <;> rfl⟩
      | some e => some ⟨.leaf ⟨.expression e, locals,
          .loopIncrement condition e body :: rest⟩, by intro σ p fid context; cases context <;> rfl⟩
  | .function none expected :: rest =>
    match flow with
    | .returned w =>
      if typed : w.type = expected then
        some ⟨.leaf ⟨.returned w, [], rest⟩, by
          intro σ p fid context
          cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.resumeFlow,
            SourceCursor.denote, CutControl.denote, SymbolicFlow.denote,
            CWord.denote, typed, LocalStore.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map]⟩
      else none
    | _ => none
  | _ => none

def cursorArguments {Γ : Var → Nat} (locals : LocalStore Γ) (type : Source.IntType)
    (callee : Nat) (arguments : List (Source.QualifiedType × Source.Expr))
    (reverseWords : List (CWord Γ)) (k : SavedFrames Γ) : SourceCursor Γ :=
  match arguments with
  | [] => ⟨.call callee type reverseWords.reverse, locals, k⟩
  | (destination, expr) :: rest =>
    ⟨.expression expr, locals, .callArgument type callee destination rest reverseWords :: k⟩

def cursorLogical {Γ : Var → Nat} (locals : LocalStore Γ) (k : SavedFrames Γ)
    (isOr : Bool) (right : Source.Expr) (word : CWord Γ) :
    Option {tree // CursorStep
      ⟨.value word, locals, .binaryLeft .int (if isOr then .or else .and) right :: k⟩ tree} :=
  let skip := CutTree.leaf (SourceCursor.mk (.value (CWord.constant (.boolean isOr))) locals k)
  let next := CutTree.leaf (SourceCursor.mk (.expression right) locals
    (.binaryRight .int (if isOr then .or else .and) word :: k))
  some ⟨if isOr then .branch word.truthTerm skip next else .branch word.truthTerm next skip, by
    intro σ p fid context
    cases isOr <;> cases context <;> cases truth : (word.denote σ).truth <;>
      simp [CutTree.eval, truth_term_sound, truth, skip, next, Source.Step, Source.step,
        Source.resumeValue, Source.valueResult, SourceCursor.denote, CutControl.denote,
        sourceContextFrames, SavedFrames.denote, Source.Frame.map,
        Source.Value.boolean, Source.Value.ofNat] <;> rfl⟩

def cursorValue {Γ : Var → Nat} (locals : LocalStore Γ) (k : SavedFrames Γ)
    (word : CWord Γ) :
    Option {tree // CursorStep ⟨.value word, locals, k⟩ tree} :=
  match k with
  | .binaryLeft .int .and right :: rest => cursorLogical locals rest false right word
  | .binaryLeft .int .or right :: rest => cursorLogical locals rest true right word
  | .callArgument type callee destination remaining reverseWords :: rest =>
    some ⟨.leaf (cursorArguments locals type callee remaining
      (word.cast destination.type :: reverseWords) rest), by
        intro σ p fid context
        cases context <;> cases remaining <;>
          simp [CutTree.eval, cursorArguments, Source.Step, Source.step,
            Source.resumeValue, Source.beginArguments, SourceCursor.denote,
            CutControl.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map,
            cast_sound, List.map_reverse]⟩
  | .binaryLeft type op right :: rest =>
    if notAnd : op ≠ .and then
      if notOr : op ≠ .or then
        some ⟨.leaf ⟨.expression right, locals, .binaryRight type op word :: rest⟩, by
          intro σ p fid context
          cases context <;> simp [CutTree.eval, Source.Step, Source.step,
            Source.resumeValue, SourceCursor.denote, CutControl.denote, sourceContextFrames,
            SavedFrames.denote, Source.Frame.map, notAnd, notOr]⟩
      else none
    else none
  | .discard :: rest => some ⟨.leaf ⟨.flow .normal, locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
  | .return destination :: rest =>
    some ⟨.leaf ⟨.flow (.returned (word.cast destination.type)), locals, rest⟩, by
      intro σ p fid context
      cases context <;>
        simp [CutTree.eval, Source.Step, Source.step, Source.resumeValue, SourceCursor.denote,
          CutControl.denote, SymbolicFlow.denote, cast_sound, sourceContextFrames, SavedFrames.denote, Source.Frame.map]⟩
  | .initialize id destination :: rest =>
    match h : locals.write id destination (word.cast destination.type) true with
    | none => none
    | some updated => some ⟨.leaf ⟨.flow .normal, updated, rest⟩, by
        intro σ p fid context
        have write := local_write_sound σ locals updated id destination
          (word.cast destination.type) true h
        rw [cast_sound] at write
        cases context <;>
          simp [CutTree.eval, Source.Step, Source.step, Source.resumeValue, SourceCursor.denote,
            CutControl.denote, SymbolicFlow.denote, write, sourceContextFrames, SavedFrames.denote, Source.Frame.map]⟩
  | .branch yes no :: rest =>
    some ⟨.branch word.truthTerm (.leaf ⟨.statement yes, locals, rest⟩)
      (.leaf ⟨.statement (no.getD .null), locals, rest⟩), by
        intro σ p fid context
        simp only [CutTree.eval, truth_term_sound]
        cases context <;> cases truth : (word.denote σ).truth <;>
          simp [truth, Source.Step, Source.step, Source.resumeValue, SourceCursor.denote,
            CutControl.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map]⟩
  | .choice type yes no :: rest =>
    some ⟨.branch word.truthTerm
      (.leaf ⟨.expression yes, locals, .convert type :: rest⟩)
      (.leaf ⟨.expression no, locals, .convert type :: rest⟩), by
        intro σ p fid context
        simp only [CutTree.eval, truth_term_sound]
        cases context <;> cases truth : (word.denote σ).truth <;>
          simp [truth, Source.Step, Source.step, Source.resumeValue, SourceCursor.denote,
            CutControl.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map]⟩
  | .loopTest condition increment body :: rest =>
    some ⟨.branch word.truthTerm
      (.leaf ⟨.statement body, locals, .loopBody condition increment body :: rest⟩)
      (.leaf ⟨.flow .normal, locals, rest⟩), by
        intro σ p fid context
        simp only [CutTree.eval, truth_term_sound]
        cases context <;> cases truth : (word.denote σ).truth <;>
          simp [truth, Source.Step, Source.step, Source.resumeValue, SourceCursor.denote,
            CutControl.denote, SymbolicFlow.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map]⟩
  | .loopIncrement condition increment body :: rest =>
    some ⟨.leaf ⟨.expression condition, locals, .loopTest condition (some increment) body :: rest⟩,
      by intro σ p fid context; cases context <;> rfl⟩
  | _ => none

end EvoIR.Core.Symbolic
