import EvoIR.Core.ArgumentFrontier

/-! Construct real source prefixes ending at a suspended direct call. The
budget limits proof search only: no successful result invokes or unrolls the
stopped callee. Ordinary expressions and administrative steps reuse the same
source semantics as cyclic cutpoint checking. -/
namespace EvoIR.Core.Symbolic

def cursorDescend {Γ : Var → Nat} (locals : LocalStore Γ) (expr : Source.Expr)
    (k : SavedFrames Γ) : Option {tree // CursorStep ⟨.expression expr, locals, k⟩ tree} :=
  if noWrap : Source.wrappingConstant expr = none then
    match expr with
    | .call type callee args =>
      some ⟨.leaf (cursorArguments locals type callee args [] k), by
        intro σ p fid context
        cases context <;> cases args <;>
          simp [CutTree.eval, cursorArguments, Source.Step, Source.step,
            Source.enterExpression, Source.beginArguments, SourceCursor.denote,
            CutControl.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map, noWrap]⟩
    | .binary type op left right => some ⟨.leaf ⟨.expression left, locals, .binaryLeft type op right :: k⟩, by
        intro σ p fid context
        cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
          SourceCursor.denote, CutControl.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map, noWrap]⟩
    | .unary type op operand => some ⟨.leaf ⟨.expression operand, locals, .unary type op :: k⟩, by
        intro σ p fid context
        cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
          SourceCursor.denote, CutControl.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map, noWrap]⟩
    | .conditional type test yes no => some ⟨.leaf ⟨.expression test, locals, .choice type yes no :: k⟩, by
        intro σ p fid context
        cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
          SourceCursor.denote, CutControl.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map, noWrap]⟩
    | .cast type destination operand =>
      if typed : type = destination.type then
        some ⟨.leaf ⟨.expression operand, locals, .convert type :: k⟩, by
          intro σ p fid context
          subst type
          cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
            SourceCursor.denote, CutControl.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map, noWrap]⟩
      else none
    | .comma type left right => some ⟨.leaf ⟨.expression left, locals, .binaryLeft type .comma right :: k⟩, by
        intro σ p fid context
        cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
          SourceCursor.denote, CutControl.denote, sourceContextFrames, SavedFrames.denote, Source.Frame.map, noWrap]⟩
    | .assign type op id destination (.ident targetType targetId) rhs =>
      if typed : type = destination.type ∧ targetType = destination.type ∧ targetId = id then
        if writable : locals.writable id destination = true then
          some ⟨.leaf ⟨.expression rhs, locals, .assign type op id destination :: k⟩, by
            intro σ p fid context
            rcases typed with ⟨rfl, rfl, rfl⟩
            have valid := local_writable_sound σ locals targetId destination writable
              (SourceCursor.mk (.expression (.assign destination.type op targetId destination
                (.ident destination.type targetId) rhs)) locals k |>.denote σ fid context)
              (by cases context <;> rfl)
            cases context <;>
              simp only [SourceCursor.denote, CutControl.denote, sourceContextFrames,
                SavedFrames.denote, List.map_map] at valid <;>
              simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
                SourceCursor.denote, CutControl.denote, sourceContextFrames,
                Source.wrappingConstant, SavedFrames.denote, Source.Frame.map, valid]⟩
        else none
      else none
    | _ => none
  else none

def sourceFrontierChunk {Γ : Var → Nat} (p : Source.Program) (budget : Nat) :
    (cursor : SourceCursor Γ) → Option {tree // SourceCutRuns p true cursor tree}
  | ⟨.expression expr, locals, k⟩ =>
    match cursorDescend locals expr k with
    | some step => some ⟨step.val, step.property.runs p⟩
    | none => sourceChunk p budget ⟨.expression expr, locals, k⟩
  | ⟨.value word, locals, k⟩ =>
    match cursorValue locals k word with
    | some step => some ⟨step.val, step.property.runs p⟩
    | none => sourceChunk p budget ⟨.value word, locals, k⟩
  | cursor => sourceChunk p budget cursor

def sourceCallFrontier {Γ : Var → Nat} (p : Source.Program) : (budget : Nat) →
    (cursor : SourceCursor Γ) → Option {tree // SourceCutRuns p false cursor tree}
  | 0, _ => none
  | budget+1, cursor => match cursor with
    | ⟨.call callee type args, locals, k⟩ => some ⟨.leaf ⟨.call callee type args, locals, k⟩, by
        intro σ fid context
        exact ⟨_, .refl _, by simp, rfl⟩⟩
    | ⟨.expression (.call type callee args), locals, k⟩ =>
      match argumentFrontier p (budget+1) locals type callee args [] with
      | some built => some ⟨argumentCallTree type callee k built.val,
          (call_frontier_sound p locals type callee args k built.val built.property).relax false⟩
      | none => do
        let step ← cursorDescend locals (.call type callee args) k
        let tail ← step.val.bindChecked (next := SourceCutRuns p false)
          (fun next => sourceCallFrontier p budget next)
        return ⟨tail.val, source_cut_compose ((step.property.runs p).relax false) tail.property⟩
    | cursor => do
      let chunk ← sourceFrontierChunk p (budget+1) cursor
      let tail ← chunk.val.bindChecked (next := SourceCutRuns p false)
        (fun next => sourceCallFrontier p budget next)
      return ⟨tail.val, source_cut_compose (chunk.property.relax false) tail.property⟩

end EvoIR.Core.Symbolic
