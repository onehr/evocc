import EvoIR.Core.SourceExpression
import EvoIR.Core.SourceDivision

/-! Checked numeric operation selection for stateful source diagrams. The
operands are the already-evaluated source values, not rewritten source syntax;
in particular they never acquire the literal-overflow exception. -/
namespace EvoIR.Core.Symbolic

def BinaryResult {Γ : Var → Nat} (op : Source.Binary) (a b : CWord Γ)
    (r : ExpressionResult Γ) : Prop :=
  ∀ σ, r.denote σ = Source.binaryValue op (a.denote σ) (b.denote σ)

def binaryResult {Γ : Var → Nat} (op : Source.Binary) (a b : CWord Γ) :
    Option {r // BinaryResult op a b r} :=
  match op with
  | .add => some ⟨arithmetic .add a b, fun σ => arithmetic_sound σ .add a b⟩
  | .sub => some ⟨arithmetic .sub a b, fun σ => arithmetic_sound σ .sub a b⟩
  | .mul => some ⟨arithmetic .mul a b, fun σ => arithmetic_sound σ .mul a b⟩
  | .div => some ⟨division false a b, fun σ => division_sound σ false a b⟩
  | .rem => some ⟨division true a b, fun σ => division_sound σ true a b⟩
  | .bitAnd => some ⟨([], bitwise .and a b), fun σ => (bitwise_sound σ .and a b).symm⟩
  | .bitOr => some ⟨([], bitwise .or a b), fun σ => (bitwise_sound σ .or a b).symm⟩
  | .bitXor => some ⟨([], bitwise .xor a b), fun σ => (bitwise_sound σ .xor a b).symm⟩
  | .eq => some ⟨([], comparison .eq a b), fun σ => (comparison_sound σ .eq a b).symm⟩
  | .ne => some ⟨([], comparison .ne a b), fun σ => (comparison_sound σ .ne a b).symm⟩
  | .lt => some ⟨([], comparison .lt a b), fun σ => (comparison_sound σ .lt a b).symm⟩
  | .le => some ⟨([], comparison .le a b), fun σ => (comparison_sound σ .le a b).symm⟩
  | .gt => some ⟨([], comparison .gt a b), fun σ => (comparison_sound σ .gt a b).symm⟩
  | .ge => some ⟨([], comparison .ge a b), fun σ => (comparison_sound σ .ge a b).symm⟩
  | .shr => some ⟨shift false a b, fun σ => shift_sound σ false a b (.inl rfl)⟩
  | .shl =>
    if allowed : a.type.signed = false then
      some ⟨shift true a b, fun σ => shift_sound σ true a b (.inr allowed)⟩
    else none
  | .comma => some ⟨([], b), fun _ => rfl⟩
  | _ => none

def CWord.booleanize {Γ : Var → Nat} (word : CWord Γ) : CWord Γ :=
  ⟨.int, .cast .zext 32 word.truthTerm⟩

theorem booleanize_sound {Γ : Var → Nat} (σ : Subst Γ) (word : CWord Γ) :
    word.booleanize.denote σ = Source.Value.boolean (word.denote σ).truth := by
  rw [← truth_term_sound]
  change Source.Value.mk .int ((denoteTerm σ word.truthTerm).setWidth 32) = _
  have bound := (denoteTerm σ word.truthTerm).isLt
  have alternatives : denoteTerm σ word.truthTerm = 0#1 ∨ denoteTerm σ word.truthTerm = 1#1 := by
    have : (denoteTerm σ word.truthTerm).toNat = 0 ∨ (denoteTerm σ word.truthTerm).toNat = 1 := by omega
    rcases this with h | h
    · exact .inl (BitVec.eq_of_toNat_eq h)
    · exact .inr (BitVec.eq_of_toNat_eq h)
  rcases alternatives with h | h <;> rw [h] <;> rfl

end EvoIR.Core.Symbolic
