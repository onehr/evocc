import EvoIR.Core.ArgumentFrontier

/-! Construct real source prefixes ending at a suspended direct call. The
budget limits proof search only: no successful result invokes or unrolls the
stopped callee. Ordinary expressions and administrative steps reuse the same
source semantics as cyclic cutpoint checking. -/
namespace EvoIR.Core.Symbolic

def cursorDescend {Γ : Var → Nat} (locals : LocalStore Γ) (expr : Source.Expr)
    (k : List Source.Frame) : Option {tree // CursorStep ⟨.expression expr, locals, k⟩ tree} :=
  if noWrap : Source.wrappingConstant expr = none then
    match expr with
    | .binary type op left right => some ⟨.leaf ⟨.expression left, locals, .binaryLeft type op right :: k⟩, by
        intro σ p fid context
        cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
          SourceCursor.denote, CutControl.denote, sourceContextFrames, noWrap]⟩
    | .unary type op operand => some ⟨.leaf ⟨.expression operand, locals, .unary type op :: k⟩, by
        intro σ p fid context
        cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
          SourceCursor.denote, CutControl.denote, sourceContextFrames, noWrap]⟩
    | .conditional type test yes no => some ⟨.leaf ⟨.expression test, locals, .choice type yes no :: k⟩, by
        intro σ p fid context
        cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
          SourceCursor.denote, CutControl.denote, sourceContextFrames, noWrap]⟩
    | .cast type destination operand =>
      if typed : type = destination.type then
        some ⟨.leaf ⟨.expression operand, locals, .convert type :: k⟩, by
          intro σ p fid context
          subst type
          cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
            SourceCursor.denote, CutControl.denote, sourceContextFrames, noWrap]⟩
      else none
    | .comma type left right => some ⟨.leaf ⟨.expression left, locals, .binaryLeft type .comma right :: k⟩, by
        intro σ p fid context
        cases context <;> simp [CutTree.eval, Source.Step, Source.step, Source.enterExpression,
          SourceCursor.denote, CutControl.denote, sourceContextFrames, noWrap]⟩
    | _ => none
  else none

def sourceFrontierChunk {Γ : Var → Nat} (p : Source.Program) (budget : Nat) :
    (cursor : SourceCursor Γ) → Option {tree // SourceCutRuns p true cursor tree}
  | ⟨.expression expr, locals, k⟩ =>
    match cursorDescend locals expr k with
    | some step => some ⟨step.val, step.property.runs p⟩
    | none => sourceChunk p budget ⟨.expression expr, locals, k⟩
  | cursor => sourceChunk p budget cursor

def sourceCallFrontier {Γ : Var → Nat} (p : Source.Program) : (budget : Nat) →
    (cursor : SourceCursor Γ) → Option {tree // SourceCutRuns p false cursor tree}
  | 0, _ => none
  | budget+1, cursor => match cursor with
    | ⟨.call callee type args, locals, k⟩ => some ⟨.leaf ⟨.call callee type args, locals, k⟩, by
        intro σ fid context
        exact ⟨_, .refl _, by simp, rfl⟩⟩
    | ⟨.expression (.call type callee args), locals, k⟩ => do
      let built ← argumentFrontier p (budget+1) locals type callee args []
      return ⟨argumentCallTree type callee k built.val,
        (call_frontier_sound p locals type callee args k built.val built.property).relax false⟩
    | cursor => do
      let chunk ← sourceFrontierChunk p (budget+1) cursor
      let tail ← chunk.val.bindChecked (next := SourceCutRuns p false)
        (fun next => sourceCallFrontier p budget next)
      return ⟨tail.val, source_cut_compose (chunk.property.relax false) tail.property⟩

end EvoIR.Core.Symbolic
