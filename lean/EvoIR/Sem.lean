import Std.Tactic.BVDecide

/-!
# P0 scalar semantics

One `denoteOp` for every operator the Section 8.5 allowlist admits, written
directly from EvoIR specification sections 3.3 to 3.6.  Widths live in the
operator, so a comparison consuming `bits<N>` and producing `bits<1>`, or a
shift consuming an amount of an independent width `M`, is expressed in the
type rather than by truncating one width into another.
-/

namespace EvoIR

/-- A pattern variable.  The registry writes them `?x`, `?a`, `?b`, `?m`,
`?z`, `?k`; the leading `?` is not part of the name. -/
abbrev Var := String

/-- The two-operand arithmetic and bitwise operators of 3.3 and 3.4. -/
inductive BinOp where
  | add | sub | mul | and | or | xor
  deriving DecidableEq, Repr, Inhabited

/-- The six comparisons of 3.5. -/
inductive CmpOp where
  | eq | ne | slt | sle | ult | ule
  deriving DecidableEq, Repr, Inhabited

/-- The three shifts of 3.4.  The amount has its own width. -/
inductive ShOp where
  | shl | lshr | ashr
  deriving DecidableEq, Repr, Inhabited

/-- The width changes of 3.6. -/
inductive CastOp where
  | zext | sext | trunc
  deriving DecidableEq, Repr, Inhabited

/-- Total signed integer predicates from Sections 3.3.1 and 6.1. -/
inductive GuardOp where
  | add | sub | mul | divRem
  deriving DecidableEq, Repr, Inhabited

def signedMin (n : Nat) : Int := -(2 ^ (n - 1) : Int)
def signedMax (n : Nat) : Int := (2 ^ (n - 1) : Int) - 1
def signedRepresentable (n : Nat) (z : Int) : Prop :=
  signedMin n ≤ z ∧ z ≤ signedMax n

instance (n : Nat) (z : Int) : Decidable (signedRepresentable n z) := by
  unfold signedRepresentable; infer_instance

def denoteNonzero {n : Nat} (y : BitVec n) : BitVec 1 :=
  BitVec.ofBool (y != 0#n)

/-- An operator together with the widths its signature fixes. -/
inductive Op where
  | bin (o : BinOp) (n : Nat)
  | cmp (o : CmpOp) (n : Nat)
  | shift (o : ShOp) (n m : Nat)
  | cast (o : CastOp) (w n : Nat)
  | guard (o : GuardOp) (n : Nat)
  | nonzero (n : Nat)
  | count (isSigned : Bool) (n m : Nat)
  deriving DecidableEq, Repr, Inhabited

/-- The operand tuple an operator consumes. -/
def Op.dom : Op → Type
  | .bin _ n => BitVec n × BitVec n
  | .cmp _ n => BitVec n × BitVec n
  | .shift _ n m => BitVec n × BitVec m
  | .cast _ w _ => BitVec w
  | .guard _ n => BitVec n × BitVec n
  | .nonzero n => BitVec n
  | .count _ n m => BitVec n × BitVec m

/-- The width an operator produces. -/
def Op.res : Op → Nat
  | .bin _ n => n
  | .cmp _ _ => 1
  | .shift _ n _ => n
  | .cast _ _ n => n
  | .guard _ _ => 1
  | .nonzero _ => 1
  | .count _ _ _ => 1

/-- The single semantics.  Everything else in this development -- the rule
theorems, the known-bits transfers, and the executable evaluator -- is stated
about this function and never about a second copy of it.

Shifts decode the amount at its own width through `k.toNat` (3.4: "`%b` is
decoded at its own `M`-bit width"), so an amount of `N` or more gives zero for
`shl` and `lshr` and a sign fill for `ashr`.  Comparisons produce `BitVec 1`
(3.5).  The width changes are `zeroExtend`, `signExtend` and `truncate`
(3.6). -/
def denoteOp : (o : Op) → o.dom → BitVec o.res
  | .bin .add _, (x, y) => x + y
  | .bin .sub _, (x, y) => x - y
  | .bin .mul _, (x, y) => x * y
  | .bin .and _, (x, y) => x &&& y
  | .bin .or _, (x, y) => x ||| y
  | .bin .xor _, (x, y) => x ^^^ y
  | .cmp .eq _, (x, y) => BitVec.ofBool (x == y)
  | .cmp .ne _, (x, y) => BitVec.ofBool (x != y)
  | .cmp .slt _, (x, y) => BitVec.ofBool (x.slt y)
  | .cmp .sle _, (x, y) => BitVec.ofBool (x.sle y)
  | .cmp .ult _, (x, y) => BitVec.ofBool (x.ult y)
  | .cmp .ule _, (x, y) => BitVec.ofBool (x.ule y)
  | .shift .shl _ _, (x, k) => x <<< k.toNat
  | .shift .lshr _ _, (x, k) => x >>> k.toNat
  | .shift .ashr _ _, (x, k) => x.sshiftRight k.toNat
  | .cast .zext _w n, x => BitVec.zeroExtend n x
  | .cast .sext _w n, x => BitVec.signExtend n x
  | .cast .trunc _w n, x => BitVec.truncate n x
  -- Section 6.1: the mathematical signed result must be representable.
  | .guard .add n, (x, y) => BitVec.ofBool (decide (signedRepresentable n (x.toInt + y.toInt)))
  | .guard .sub n, (x, y) => BitVec.ofBool (decide (signedRepresentable n (x.toInt - y.toInt)))
  | .guard .mul n, (x, y) => BitVec.ofBool (decide (signedRepresentable n (x.toInt * y.toInt)))
  -- Section 3.3.1: zero divisor is deliberately not an overflow failure.
  | .guard .divRem n, (x, y) =>
      BitVec.ofBool (decide (¬ (x.toInt = signedMin n ∧ y.toInt = -1)))
  | .nonzero _, y => denoteNonzero y
  -- Decode the count at M, against the mathematical bound N; do not mask it.
  | .count false n _, (_, k) => BitVec.ofBool (decide (k.toNat < n))
  | .count true n _, (_, k) => BitVec.ofBool (decide (0 ≤ k.toInt ∧ k.toInt < (n : Int)))


/-! ## Evaluating a shift by an amount too large to shift by

A `bits<128>` amount is inside Section 8.5's shape, and `2^64` is a value a
machine cannot shift by even though the answer is defined.  `capAmount`
replaces such an amount by the width, and `denoteOp_capAmount` proves the two
give `denoteOp` the same answer, so the evaluator computes the semantics of
this file rather than a second one. -/

def capAmount (n : Nat) {m : Nat} (k : BitVec m) : BitVec m :=
  if k.toNat < n then k else BitVec.ofNat m n

theorem capAmount_toNat {n m : Nat} (k : BitVec m) (h : ¬ k.toNat < n) :
    (capAmount n k).toNat = n := by
  have hk : k.toNat < 2 ^ m := k.isLt
  simp only [capAmount, h, if_false, BitVec.toNat_ofNat]
  exact Nat.mod_eq_of_lt (by omega)

theorem shiftLeft_ge {n : Nat} (x : BitVec n) (s : Nat) (h : n ≤ s) : x <<< s = 0#n := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp [BitVec.getLsbD_shiftLeft, show i < s by omega]

theorem ushiftRight_ge {n : Nat} (x : BitVec n) (s : Nat) (h : n ≤ s) : x >>> s = 0#n := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp [BitVec.getLsbD_ushiftRight, BitVec.getLsbD_of_ge x (s + i) (by omega)]

theorem sshiftRight_ge {n : Nat} (x : BitVec n) (s t : Nat) (hs : n ≤ s) (ht : n ≤ t) :
    x.sshiftRight s = x.sshiftRight t := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp [BitVec.getLsbD_sshiftRight, show ¬ (s + i < n) by omega, show ¬ (t + i < n) by omega,
    show ¬ (n ≤ i) by omega]

theorem denoteOp_capAmount (o : ShOp) (n m : Nat) (x : BitVec n) (k : BitVec m) :
    denoteOp (.shift o n m) (x, capAmount n k) = denoteOp (.shift o n m) (x, k) := by
  by_cases h : k.toNat < n
  · simp [capAmount, h]
  · have hc : (capAmount n k).toNat = n := capAmount_toNat k h
    cases o <;> simp only [denoteOp, hc]
    · rw [shiftLeft_ge x n (by omega), shiftLeft_ge x k.toNat (by omega)]
    · rw [ushiftRight_ge x n (by omega), ushiftRight_ge x k.toNat (by omega)]
    · exact sshiftRight_ge x n k.toNat (by omega) (by omega)


/-- The spelling Section 11.3's operator table uses, for rendering registry
data back to its canonical text. -/
def BinOp.name : BinOp → String
  | .add => "add" | .sub => "sub" | .mul => "mul"
  | .and => "and" | .or => "or" | .xor => "xor"

def CmpOp.name : CmpOp → String
  | .eq => "icmp.eq" | .ne => "icmp.ne" | .slt => "icmp.slt"
  | .sle => "icmp.sle" | .ult => "icmp.ult" | .ule => "icmp.ule"

def ShOp.name : ShOp → String
  | .shl => "shl" | .lshr => "lshr" | .ashr => "ashr"

def CastOp.name : CastOp → String
  | .zext => "zext" | .sext => "sext" | .trunc => "trunc"

def GuardOp.name : GuardOp → String
  | .add => "signed_add_defined" | .sub => "signed_sub_defined"
  | .mul => "signed_mul_defined" | .divRem => "signed_div_rem_no_overflow"

end EvoIR
