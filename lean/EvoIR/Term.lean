import EvoIR.Sem

/-!
# Typed terms

The small term language a P0 rule's two sides are written in.  A term carries
its result width in its type, and a variable carries the proof that the width
environment gives it that width, so no two widths ever share one `BitVec n`.

`denoteTerm` recurses over the term and then calls `denoteOp`; it defines no
operator semantics of its own.
-/

namespace EvoIR

/-- Terms of result width `n` over the width environment `Γ`. -/
inductive Term (Γ : Var → Nat) : Nat → Type where
  /-- A pattern variable, at the width `Γ` gives it. -/
  | var (v : Var) (n : Nat) (h : Γ v = n) : Term Γ n
  /-- A constant of the stated width. -/
  | const (n : Nat) (c : BitVec n) : Term Γ n
  /-- Arithmetic and bitwise: both operands and the result share one width. -/
  | bin (o : BinOp) {n : Nat} (a b : Term Γ n) : Term Γ n
  /-- Comparison: one common operand width, result `bits<1>`. -/
  | cmp (o : CmpOp) {n : Nat} (a b : Term Γ n) : Term Γ 1
  /-- Shift: the amount has its own, unconstrained width `m`. -/
  | shift (o : ShOp) {n m : Nat} (a : Term Γ n) (k : Term Γ m) : Term Γ n
  /-- Width change: source width `w`, target width `n`. -/
  | cast (o : CastOp) {w : Nat} (n : Nat) (a : Term Γ w) : Term Γ n
  | guard (o : GuardOp) {n : Nat} (a b : Term Γ n) : Term Γ 1
  | nonzero {n : Nat} (a : Term Γ n) : Term Γ 1
  | count (isSigned : Bool) {n m : Nat} (a : Term Γ n) (k : Term Γ m) : Term Γ 1

/-- A valuation of the pattern variables, each at its own width. -/
abbrev Subst (Γ : Var → Nat) := (v : Var) → BitVec (Γ v)

/-- Interpretation of a term.  The only operator semantics is `denoteOp`. -/
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

@[simp] theorem denoteTerm_var {Γ : Var → Nat} (σ : Subst Γ) (v : Var) (n : Nat) (h : Γ v = n) :
    denoteTerm σ (.var v n h) = h ▸ σ v := rfl

@[simp] theorem denoteTerm_const {Γ : Var → Nat} (σ : Subst Γ) (n : Nat) (c : BitVec n) :
    denoteTerm σ (Term.const n c) = c := rfl

@[simp] theorem denoteTerm_bin {Γ : Var → Nat} (σ : Subst Γ) (o : BinOp) {n : Nat}
    (a b : Term Γ n) :
    denoteTerm σ (.bin o a b) = denoteOp (.bin o n) (denoteTerm σ a, denoteTerm σ b) := rfl

@[simp] theorem denoteTerm_cmp {Γ : Var → Nat} (σ : Subst Γ) (o : CmpOp) {n : Nat}
    (a b : Term Γ n) :
    denoteTerm σ (.cmp o a b) = denoteOp (.cmp o n) (denoteTerm σ a, denoteTerm σ b) := rfl

@[simp] theorem denoteTerm_shift {Γ : Var → Nat} (σ : Subst Γ) (o : ShOp) {n m : Nat}
    (a : Term Γ n) (k : Term Γ m) :
    denoteTerm σ (.shift o a k) = denoteOp (.shift o n m) (denoteTerm σ a, denoteTerm σ k) := rfl

@[simp] theorem denoteTerm_cast {Γ : Var → Nat} (σ : Subst Γ) (o : CastOp) {w : Nat} (n : Nat)
    (a : Term Γ w) :
    denoteTerm σ (.cast o n a) = denoteOp (.cast o w n) (denoteTerm σ a) := rfl

end EvoIR
