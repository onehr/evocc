import EvoIR.Core.Source

/-!
Structural equality of the complete source tree. The ordinary deriving handler
does not handle nested recursive lists; these decision procedures recurse on
syntax size and return kernel-checked equality/disequality evidence. They do
not compare pretty printing, hashes, normalized expressions or sampled values.
-/
namespace EvoIR.Core.Source

private def andDec {p q : Prop} (a : Decidable p) (b : Decidable q) :
    Decidable (p ∧ q) := @instDecidableAnd p q a b

mutual
  def exprDecEq (a b : Expr) : Decidable (a = b) := by
    cases a <;> cases b
    all_goals try (solve | apply isFalse; intro h; cases h)
    all_goals simp only [Expr.literal.injEq, Expr.ident.injEq, Expr.unary.injEq,
      Expr.binary.injEq, Expr.conditional.injEq, Expr.cast.injEq, Expr.comma.injEq,
      Expr.assign.injEq, Expr.update.injEq, Expr.call.injEq]
    all_goals repeat first
      | apply andDec
      | exact exprDecEq _ _
      | exact argumentsDecEq _ _
      | infer_instance
  termination_by sizeOf a

  def argumentsDecEq (a b : List (QualifiedType × Expr)) : Decidable (a = b) := by
    cases a with
    | nil => cases b <;> infer_instance
    | cons x xs =>
      cases b with
      | nil => exact isFalse (by intro h; cases h)
      | cons y ys =>
        rcases x with ⟨xt, xe⟩
        rcases y with ⟨yt, ye⟩
        haveI := exprDecEq xe ye
        haveI := argumentsDecEq xs ys
        simp only [List.cons.injEq, Prod.mk.injEq]
        infer_instance
  termination_by sizeOf a
end

instance : DecidableEq Expr := exprDecEq
deriving instance DecidableEq for Object, ForInit

mutual
  def stmtDecEq (a b : Stmt) : Decidable (a = b) := by
    cases a <;> cases b
    all_goals try (solve | apply isFalse; intro h; cases h)
    all_goals try (solve | exact isTrue rfl)
    all_goals simp only [Stmt.expression.injEq, Stmt.return.injEq, Stmt.compound.injEq,
      Stmt.ite.injEq, Stmt.while.injEq, Stmt.doWhile.injEq, Stmt.for.injEq]
    all_goals repeat first
      | apply andDec
      | exact stmtDecEq _ _
      | exact optionalStmtDecEq _ _
      | exact itemsDecEq _ _
      | infer_instance
  termination_by sizeOf a

  def optionalStmtDecEq (a b : Option Stmt) : Decidable (a = b) := by
    cases a with
    | none => cases b <;> infer_instance
    | some x =>
      cases b with
      | none => exact isFalse (by intro h; cases h)
      | some y => simpa using stmtDecEq x y
  termination_by sizeOf a

  def itemDecEq (a b : BlockItem) : Decidable (a = b) := by
    cases a <;> cases b
    all_goals try (solve | apply isFalse; intro h; cases h)
    all_goals simp only [BlockItem.declaration.injEq, BlockItem.statement.injEq]
    · infer_instance
    · exact stmtDecEq _ _
  termination_by sizeOf a

  def itemsDecEq (a b : List BlockItem) : Decidable (a = b) := by
    cases a with
    | nil => cases b <;> infer_instance
    | cons x xs =>
      cases b with
      | nil => exact isFalse (by intro h; cases h)
      | cons y ys =>
        haveI := itemDecEq x y
        haveI := itemsDecEq xs ys
        simp only [List.cons.injEq]
        infer_instance
  termination_by sizeOf a
end

instance : DecidableEq Stmt := stmtDecEq
instance : DecidableEq BlockItem := itemDecEq
deriving instance DecidableEq for Parameter, Function, Program

end EvoIR.Core.Source
