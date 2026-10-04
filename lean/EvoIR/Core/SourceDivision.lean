import EvoIR.Core.SymbolicSource

/-! C division and remainder retain usual arithmetic conversions, truncation
toward zero and first-failure diagnostics. This bridge does not add a rewrite
rule or assume that the source operands are defined. -/
namespace EvoIR.Core.Symbolic

def division {Γ : Var → Nat} (remainder : Bool) (a b : CWord Γ) :
    List (Guard Γ) × CWord Γ :=
  let type := Source.usualType a.type b.type
  let x := (a.cast type).term
  let y := (b.cast type).term
  (divisionGuards type.signed remainder x y, ⟨type, .division type.signed remainder x y⟩)

theorem typed_division_trace {Γ : Var → Nat} (σ : Subst Γ)
    (remainder : Bool) (type : Source.IntType) (a b : Term Γ type.width) :
    Source.binaryValue (if remainder then .rem else .div)
        ⟨type, denoteTerm σ a⟩ ⟨type, denoteTerm σ b⟩ =
      (do guards σ (divisionGuards type.signed remainder a b)
          return Source.Value.mk type (divisionBits type.signed remainder (denoteTerm σ a) (denoteTerm σ b))) := by
  have self (bits : BitVec type.width) : Source.Value.cast type ⟨type, bits⟩ = ⟨type, bits⟩ :=
    Source.value_cast_self ⟨type, bits⟩
  have zero : (denoteTerm σ b).toInt = 0 ↔ (denoteTerm σ b).toNat = 0 := by
    have eqz := BitVec.toInt_inj (x := denoteTerm σ b) (y := 0)
    simpa [BitVec.toNat_eq] using eqz
  have divNat (a b : Nat) : (a : Int).tdiv b = (a / b : Nat) := rfl
  have remNat (a b : Nat) : (a : Int).tmod b = (a % b : Nat) := rfl
  have moduloNat (q n : Nat) : ((q : Int) % (2 ^ n : Int)).toNat = q % 2 ^ n := by
    rfl
  cases remainder <;>
    simp only [Bool.false_eq_true, ↓reduceIte, Source.binaryValue,
      Source.usualType_self, self]
  all_goals cases signed : type.signed <;>
    by_cases nonzero : (denoteTerm σ b).toNat = 0 <;>
    by_cases overflow : (denoteTerm σ a).toInt = signedMin type.width ∧ (denoteTerm σ b).toInt = -1 <;>
    simp [Source.Value.number, signed, Source.Value.ofInt, divisionGuards, divisionBits,
      guards, denoteTerm, denoteOp, denoteNonzero, IR.Reason.code,
      bind, Except.bind, pure, Except.pure, throw, throwThe,
      MonadExceptOf.throw, zero, nonzero, overflow, divNat, remNat, BitVec.toNat_eq]
  all_goals first
    | omega
    | simpa only [← Int.natCast_ediv, ← Int.natCast_emod] using moduloNat _ _

theorem division_promote (remainder : Bool) (a b : Source.Value) :
    Source.binaryValue (if remainder then .rem else .div) a b =
      Source.binaryValue (if remainder then .rem else .div)
        (a.cast (Source.usualType a.type b.type)) (b.cast (Source.usualType a.type b.type)) := by
  have castType (v : Source.Value) (t : Source.IntType) : (v.cast t).type = t := rfl
  have castAgain (v : Source.Value) (t : Source.IntType) : (v.cast t).cast t = v.cast t :=
    Source.value_cast_self (v.cast t)
  cases remainder <;>
    simp only [Bool.false_eq_true, ↓reduceIte, Source.binaryValue,
      castType, Source.usualType_self, castAgain]

theorem division_sound {Γ : Var → Nat} (σ : Subst Γ)
    (remainder : Bool) (a b : CWord Γ) :
    (do guards σ (division remainder a b).1; return (division remainder a b).2.denote σ) =
      Source.binaryValue (if remainder then .rem else .div) (a.denote σ) (b.denote σ) := by
  let type := Source.usualType a.type b.type
  rw [division_promote remainder (a.denote σ) (b.denote σ)]
  change _ = Source.binaryValue (if remainder then .rem else .div)
    ((a.denote σ).cast type) ((b.denote σ).cast type)
  rw [← cast_sound σ type a, ← cast_sound σ type b]
  exact (typed_division_trace σ remainder type (a.cast type).term (b.cast type).term).symm

end EvoIR.Core.Symbolic
