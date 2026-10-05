import EvoIR.Term

/-! The compilation checker needs division even though the frozen P0 rewrite
language deliberately excludes it. Keep this separate width-indexed language;
reuse denoteOp for every existing primitive. No P0 operator, rule, eligibility
or proof domain changes. Division's total value is distinct from the ordered
source/target diagnostic guards carried by the surrounding diagram. -/
namespace EvoIR.Core.Symbolic

inductive Term (Γ : Var → Nat) : Nat → Type where
  | var (v : Var) (n : Nat) (h : Γ v = n) : Term Γ n
  | const (n : Nat) (c : BitVec n) : Term Γ n
  | bin (o : BinOp) {n : Nat} (a b : Term Γ n) : Term Γ n
  | cmp (o : CmpOp) {n : Nat} (a b : Term Γ n) : Term Γ 1
  | shift (o : ShOp) {n m : Nat} (a : Term Γ n) (k : Term Γ m) : Term Γ n
  | cast (o : CastOp) {w : Nat} (n : Nat) (a : Term Γ w) : Term Γ n
  | guard (o : GuardOp) {n : Nat} (a b : Term Γ n) : Term Γ 1
  | nonzero {n : Nat} (a : Term Γ n) : Term Γ 1
  | count (signed : Bool) {n m : Nat} (a : Term Γ n) (k : Term Γ m) : Term Γ 1
  | division (signed remainder : Bool) {n : Nat} (a b : Term Γ n) : Term Γ n
  deriving DecidableEq

def divisionBits {n : Nat} (signed remainder : Bool) (a b : BitVec n) : BitVec n :=
  if signed then BitVec.ofInt n (if remainder then a.toInt.tmod b.toInt else a.toInt.tdiv b.toInt)
  else BitVec.ofNat n (if remainder then a.toNat % b.toNat else a.toNat / b.toNat)

def denoteTerm {Γ : Var → Nat} (σ : Subst Γ) : {n : Nat} → Term Γ n → BitVec n
  | _, .var v _ h => h ▸ σ v
  | _, .const _ c => c
  | n, .bin o a b => denoteOp (.bin o n) (denoteTerm σ a, denoteTerm σ b)
  | _, .cmp (n := n) o a b => denoteOp (.cmp o n) (denoteTerm σ a, denoteTerm σ b)
  | n, .shift (m := m) o a k => denoteOp (.shift o n m) (denoteTerm σ a, denoteTerm σ k)
  | n, .cast (w := w) o _ a => denoteOp (.cast o w n) (denoteTerm σ a)
  | _, .guard (n := n) o a b => denoteOp (.guard o n) (denoteTerm σ a, denoteTerm σ b)
  | _, .nonzero (n := n) a => denoteOp (.nonzero n) (denoteTerm σ a)
  | _, .count (n := n) (m := m) s a k => denoteOp (.count s n m) (denoteTerm σ a, denoteTerm σ k)
  | _, .division signed remainder a b => divisionBits signed remainder (denoteTerm σ a) (denoteTerm σ b)

def Term.ofP0 {Γ : Var → Nat} : {n : Nat} → EvoIR.Term Γ n → Term Γ n
  | _, .var v n h => .var v n h
  | _, .const n c => .const n c
  | _, .bin o a b => .bin o (ofP0 a) (ofP0 b)
  | _, .cmp o a b => .cmp o (ofP0 a) (ofP0 b)
  | _, .shift o a k => .shift o (ofP0 a) (ofP0 k)
  | _, .cast o n a => .cast o n (ofP0 a)
  | _, .guard o a b => .guard o (ofP0 a) (ofP0 b)
  | _, .nonzero a => .nonzero (ofP0 a)
  | _, .count s a k => .count s (ofP0 a) (ofP0 k)

/-- Canonicalize literal width conversions in proof diagrams, not input IR. -/
def Term.constantCast {Γ : Var → Nat} (op : CastOp) {w : Nat} (n : Nat) :
    Term Γ w → Term Γ n
  | .const _ bits => .const n (denoteOp (.cast op w n) bits)
  | term => .cast op n term

theorem Term.constant_cast_sound {Γ : Var → Nat} (σ : Subst Γ)
    (op : CastOp) {w : Nat} (n : Nat) (term : Term Γ w) :
    denoteTerm σ (constantCast op n term) = denoteTerm σ (.cast op n term) := by
  cases term <;> rfl

def Term.normalizeCasts {Γ : Var → Nat} : {n : Nat} → Term Γ n → Term Γ n
  | _, .var v n h => .var v n h
  | _, .const n c => .const n c
  | _, .bin op a b => .bin op a.normalizeCasts b.normalizeCasts
  | _, .cmp op a b => .cmp op a.normalizeCasts b.normalizeCasts
  | _, .shift op a k => .shift op a.normalizeCasts k.normalizeCasts
  | _, .cast op n a => constantCast op n a.normalizeCasts
  | _, .guard op a b => .guard op a.normalizeCasts b.normalizeCasts
  | _, .nonzero a => .nonzero a.normalizeCasts
  | _, .count s a k => .count s a.normalizeCasts k.normalizeCasts
  | _, .division s r a b => .division s r a.normalizeCasts b.normalizeCasts

theorem Term.normalize_casts_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat}
    (term : Term Γ n) : denoteTerm σ term.normalizeCasts = denoteTerm σ term := by
  induction term <;> simp_all only [normalizeCasts, constant_cast_sound, denoteTerm]

/-- Evaluate only closed diagram terms. No valuation, path fact, or runtime
sample is used; a variable makes the result unknown. This is proof-diagram
normalization, not a change to the source/IR or to the EqSat registry. -/
def Term.constantValue {Γ : Var → Nat} : {n : Nat} → Term Γ n → Option (BitVec n)
  | _, .var .. => none
  | _, .const _ c => some c
  | n, .bin op a b => do return denoteOp (.bin op n) (← a.constantValue, ← b.constantValue)
  | _, .cmp (n := n) op a b => do return denoteOp (.cmp op n) (← a.constantValue, ← b.constantValue)
  | n, .shift (m := m) op a b => do return denoteOp (.shift op n m) (← a.constantValue, ← b.constantValue)
  | n, .cast (w := w) op _ a => do return denoteOp (.cast op w n) (← a.constantValue)
  | _, .guard (n := n) op a b => do return denoteOp (.guard op n) (← a.constantValue, ← b.constantValue)
  | _, .nonzero (n := n) a => do return denoteOp (.nonzero n) (← a.constantValue)
  | _, .count (n := n) (m := m) signed a b => do return denoteOp (.count signed n m) (← a.constantValue, ← b.constantValue)
  | _, .division signed remainder a b => do return divisionBits signed remainder (← a.constantValue) (← b.constantValue)

theorem Term.constant_value_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat}
    (term : Term Γ n) (value : BitVec n) (accepted : term.constantValue = some value) :
    denoteTerm σ term = value := by
  induction term with
  | var => simp [constantValue] at accepted
  | const => exact Option.some.inj accepted
  | bin op a b first second | cmp op a b first second | shift op a b first second
  | guard op a b first second | count signed a b first second | division signed remainder a b first second =>
    cases ha : a.constantValue <;> cases hb : b.constantValue <;>
      simp_all [constantValue, denoteTerm] <;> exact Option.some.inj accepted
  | cast op width a ih | nonzero a ih =>
    cases ha : a.constantValue <;> simp_all [constantValue, denoteTerm] <;> exact Option.some.inj accepted

def Term.normalizeValue {Γ : Var → Nat} {n : Nat} (term : Term Γ n) : Term Γ n :=
  match term.constantValue with
  | some value => .const n value
  | none => term.normalizeCasts

theorem Term.normalize_value_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat} (term : Term Γ n) :
    denoteTerm σ term.normalizeValue = denoteTerm σ term := by
  cases known : term.constantValue with
  | none => simp [normalizeValue, known, normalize_casts_sound]
  | some value => simpa [normalizeValue, known, denoteTerm] using (term.constant_value_sound σ value known).symm

theorem Term.of_p0_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat} (term : EvoIR.Term Γ n) :
    denoteTerm σ (ofP0 term) = EvoIR.denoteTerm σ term := by
  induction term <;> simp_all [ofP0, denoteTerm, EvoIR.denoteTerm]

end EvoIR.Core.Symbolic
