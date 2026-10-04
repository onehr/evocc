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

theorem Term.of_p0_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat} (term : EvoIR.Term Γ n) :
    denoteTerm σ (ofP0 term) = EvoIR.denoteTerm σ term := by
  induction term <;> simp_all [ofP0, denoteTerm, EvoIR.denoteTerm]

end EvoIR.Core.Symbolic
