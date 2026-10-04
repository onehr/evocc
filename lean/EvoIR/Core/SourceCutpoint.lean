import EvoIR.Core.StatementDiagram
import EvoIR.Core.SourceEquality
import EvoIR.Core.CutTree
import EvoIR.Core.CheckpointSimulation

/-! Exact source states at cyclic cutpoints. Administrative transitions use
the original source machine, including scope and loop-control continuations.
Symbolic expressions retain their ordered diagnostics and local writes. -/
namespace EvoIR.Core.Source
deriving instance DecidableEq for Slot, Frame
end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

inductive CutControl (Γ : Var → Nat) where
  | expression (expr : Source.Expr)
  | statement (stmt : Source.Stmt)
  | items (items : List Source.BlockItem)
  | object (object : Source.Object)
  | value (word : CWord Γ)
  | flow (flow : SymbolicFlow Γ)
  | returned (word : CWord Γ)

def CutControl.denote {Γ : Var → Nat} (σ : Subst Γ) : CutControl Γ → Source.Control
  | .expression e => .expression e
  | .statement s => .statement s
  | .items xs => .items xs
  | .object o => .object o
  | .value w => .value (w.denote σ)
  | .flow f => .flow (f.denote σ)
  | .returned w => .halted (.ok (w.denote σ))

structure SourceCursor (Γ : Var → Nat) where
  control : CutControl Γ
  locals : LocalStore Γ
  continuation : List Source.Frame

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
  | none => ⟨cursor.control.denote σ, cursor.locals.denote σ, cursor.continuation, fid⟩
  | some caller =>
    match cursor.control with
    | .returned word => ⟨.value (word.denote σ), caller.locals,
        sourceContextFrames context cursor.continuation, caller.functionId⟩
    | _ => ⟨cursor.control.denote σ, cursor.locals.denote σ,
        sourceContextFrames context cursor.continuation, fid⟩

inductive SourceSite where
  | expression (expr : Source.Expr) (continuation : List Source.Frame)
  | statement (stmt : Source.Stmt) (continuation : List Source.Frame)
  deriving DecidableEq

def SourceCursor.site {Γ : Var → Nat} (cursor : SourceCursor Γ) : Option SourceSite :=
  match cursor.control with
  | .expression e => some (.expression e cursor.continuation)
  | .statement s => some (.statement s cursor.continuation)
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

def cursorStatement {Γ : Var → Nat} (locals : LocalStore Γ) (k : List Source.Frame)
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
    (k : List Source.Frame) (stmt : Source.Stmt) :
    CursorStep ⟨.statement stmt, locals, k⟩ (.leaf (cursorStatement locals k stmt)) := by
  intro σ p fid context
  cases context <;> cases stmt <;> simp [CutTree.eval, cursorStatement, Source.Step,
    Source.step, Source.enterStatement, SourceCursor.denote, CutControl.denote,
    SymbolicFlow.denote, LocalStore.denote, sourceContextFrames]
  all_goals split <;> rfl

def cursorFlow {Γ : Var → Nat} (locals : LocalStore Γ) (k : List Source.Frame)
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
            ⟨.flow (flow.denote σ), locals.denote σ, rest, fid⟩ rfl (flow.denote σ))
        | some caller => exact congrArg some (local_scope_exit_sound σ locals updated n h
            ⟨.flow (flow.denote σ), locals.denote σ,
              sourceContextFrames (some caller) rest, fid⟩ rfl (flow.denote σ))⟩
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
            CWord.denote, typed, LocalStore.denote, sourceContextFrames]⟩
      else none
    | _ => none
  | _ => none

def cursorValue {Γ : Var → Nat} (locals : LocalStore Γ) (k : List Source.Frame)
    (word : CWord Γ) :
    Option {tree // CursorStep ⟨.value word, locals, k⟩ tree} :=
  match k with
  | .discard :: rest => some ⟨.leaf ⟨.flow .normal, locals, rest⟩, by intro σ p fid context; cases context <;> rfl⟩
  | .return destination :: rest =>
    some ⟨.leaf ⟨.flow (.returned (word.cast destination.type)), locals, rest⟩, by
      intro σ p fid context
      cases context <;>
        simp [CutTree.eval, Source.Step, Source.step, Source.resumeValue, SourceCursor.denote,
          CutControl.denote, SymbolicFlow.denote, cast_sound, sourceContextFrames]⟩
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
            CutControl.denote, SymbolicFlow.denote, write, sourceContextFrames]⟩
  | .branch yes no :: rest =>
    some ⟨.branch word.truthTerm (.leaf ⟨.statement yes, locals, rest⟩)
      (.leaf ⟨.statement (no.getD .null), locals, rest⟩), by
        intro σ p fid context
        simp only [CutTree.eval, truth_term_sound]
        cases context <;> cases truth : (word.denote σ).truth <;>
          simp [truth, Source.Step, Source.step, Source.resumeValue, SourceCursor.denote,
            CutControl.denote, sourceContextFrames]⟩
  | .loopTest condition increment body :: rest =>
    some ⟨.branch word.truthTerm
      (.leaf ⟨.statement body, locals, .loopBody condition increment body :: rest⟩)
      (.leaf ⟨.flow .normal, locals, rest⟩), by
        intro σ p fid context
        simp only [CutTree.eval, truth_term_sound]
        cases context <;> cases truth : (word.denote σ).truth <;>
          simp [truth, Source.Step, Source.step, Source.resumeValue, SourceCursor.denote,
            CutControl.denote, SymbolicFlow.denote, sourceContextFrames]⟩
  | .loopIncrement condition increment body :: rest =>
    some ⟨.leaf ⟨.expression condition, locals, .loopTest condition (some increment) body :: rest⟩,
      by intro σ p fid context; cases context <;> rfl⟩
  | _ => none

end EvoIR.Core.Symbolic
