import EvoIR.Sem

/-!
# The known-bits lattice of Section 8.10

A fact is the `(mask, value)` pair a certificate premise carries (11.3).  The
normalization invariant is the one the checker's `Fact::from_premise` enforces:
a premise whose value has a one where the mask has a zero is malformed.

The order, the merge, the join and the four side-condition predicates follow
8.10 and 11.6.  The transfer table is the one 8.10 prints, with the shift rows
written as the plan's split: a shift amount that is not fully known, or is
known and not below the width, transfers to top.
-/

namespace EvoIR

/-- What is known about the bits of one `bits<n>` value: bit `i` is known
exactly when `mask` has a one there, and then it equals `value`'s bit. -/
structure Fact (n : Nat) where
  mask : BitVec n
  value : BitVec n
  /-- 11.3: a value bit outside the mask is malformed. -/
  normalized : value &&& ~~~mask = 0#n

theorem Fact.ext {n : Nat} : ∀ {K K' : Fact n}, K.mask = K'.mask → K.value = K'.value → K = K'
  | ⟨_, _, _⟩, ⟨_, _, _⟩, rfl, rfl => rfl

instance {n : Nat} : DecidableEq (Fact n) := fun K K' =>
  if h : K.mask = K'.mask ∧ K.value = K'.value then
    isTrue (Fact.ext h.1 h.2)
  else
    isFalse fun e => h (by cases e; exact ⟨rfl, rfl⟩)

/-- The smart constructor every transfer uses: keep the claimed mask and drop
any value bit outside it, so the invariant holds by one proof rather than one
per row. -/
def Fact.mk' {n : Nat} (m v : BitVec n) : Fact n :=
  ⟨m, v &&& m, by
    rw [BitVec.and_assoc]
    simp⟩

/-- Nothing known. -/
def Fact.top (n : Nat) : Fact n := ⟨0#n, 0#n, by simp⟩

/-- Every bit known and equal to `v`: the `ConstValue` of 8.10. -/
def Fact.const {n : Nat} (v : BitVec n) : Fact n :=
  ⟨BitVec.allOnes n, v &&& BitVec.allOnes n, by
    rw [BitVec.and_assoc]
    simp⟩

/-- Concretization: the values a fact admits. -/
def gamma {n : Nat} (K : Fact n) (x : BitVec n) : Prop := x &&& K.mask = K.value

instance {n : Nat} (K : Fact n) (x : BitVec n) : Decidable (gamma K x) := by
  unfold gamma; infer_instance

/-- 8.10 order: `K ⊑ K'` when every bit known in `K'` is known in `K` with the
same value.  The direction is the one revision 1.9.40 corrected. -/
def Fact.le {n : Nat} (K K' : Fact n) : Prop :=
  K'.mask &&& ~~~K.mask = 0#n ∧ K.value &&& K'.mask = K'.value

instance {n : Nat} (K K' : Fact n) : Decidable (K.le K') := by
  unfold Fact.le; infer_instance

/-- 8.10 merge, with the disagreement check that makes a union invalid.
`none` is the conflict; a conflict is not a fact. -/
def Fact.meet {n : Nat} (K K' : Fact n) : Option (Fact n) :=
  if (K.value ^^^ K'.value) &&& K.mask &&& K'.mask = 0#n then
    some (Fact.mk' (K.mask ||| K'.mask) (K.value ||| K'.value))
  else
    none

/-- 8.10 join: what two facts arriving on different edges agree on. -/
def Fact.join {n : Nat} (K K' : Fact n) : Fact n :=
  Fact.mk' (K.mask &&& K'.mask &&& ~~~(K.value ^^^ K'.value)) K.value

/-- 11.6 `fully_known`. -/
def fullyKnown {n : Nat} (K : Fact n) : Prop := K.mask = BitVec.allOnes n

instance {n : Nat} (K : Fact n) : Decidable (fullyKnown K) := by
  unfold fullyKnown; infer_instance

/-- 11.6 `known_zero(?z)`: every bit known and every value bit zero. -/
def knownZero {n : Nat} (K : Fact n) : Prop := K.mask = BitVec.allOnes n ∧ K.value = 0#n

instance {n : Nat} (K : Fact n) : Decidable (knownZero K) := by
  unfold knownZero; infer_instance

/-- 11.6 `covers(?m,?x)`: every bit that is not known zero in `X` is known one
in `M`.  `X.mask &&& ~~~X.value` is the known-zero set of `X`. -/
def covers {n : Nat} (M X : Fact n) : Prop :=
  ~~~(X.mask &&& ~~~X.value) &&& ~~~M.value = 0#n

instance {n : Nat} (M X : Fact n) : Decidable (covers M X) := by
  unfold covers; infer_instance

/-- `covers` between facts whose widths are not syntactically equal.  The
checker's `covers` answers false when the widths differ; a rule's side
condition is read only after the left pattern has typed, which forces them
equal. -/
def coversAt {a b : Nat} (M : Fact a) (X : Fact b) : Prop :=
  ∀ h : a = b, covers (h ▸ M) X

/-! ## The transfer table of Section 8.10 -/

/-- Known where either input is known zero, or both are known one. -/
def T_and {n : Nat} (A B : Fact n) : Fact n :=
  Fact.mk' ((A.mask &&& ~~~A.value) ||| (B.mask &&& ~~~B.value) ||| (A.mask &&& B.mask))
    (A.value &&& B.value)

/-- Known where either input is known one, or both are known zero. -/
def T_or {n : Nat} (A B : Fact n) : Fact n :=
  Fact.mk' ((A.mask &&& A.value) ||| (B.mask &&& B.value) ||| (A.mask &&& B.mask))
    (A.value ||| B.value)

/-- Known where both inputs are known. -/
def T_xor {n : Nat} (A B : Fact n) : Fact n :=
  Fact.mk' (A.mask &&& B.mask) (A.value ^^^ B.value)

/-- The trailing run of bits known in both inputs: the low bits below the
first bit either input leaves unknown. -/
def carryMask {n : Nat} (A B : Fact n) : BitVec n :=
  (~~~(A.mask &&& B.mask) &&& ((A.mask &&& B.mask) + 1#n)) - 1#n

def T_add {n : Nat} (A B : Fact n) : Fact n := Fact.mk' (carryMask A B) (A.value + B.value)

def T_sub {n : Nat} (A B : Fact n) : Fact n := Fact.mk' (carryMask A B) (A.value - B.value)

/-- One-hot of the lowest clear bit of a known-zero set, or zero when the set
is all ones. -/
def lowbit0 {n : Nat} (kz : BitVec n) : BitVec n := ~~~kz &&& (kz + 1#n)

/-- The trailing known-zero run is the sum of the inputs' trailing known-zero
runs; nothing else is claimed. -/
def T_mul {n : Nat} (A B : Fact n) : Fact n :=
  Fact.mk' (lowbit0 (A.mask &&& ~~~A.value) * lowbit0 (B.mask &&& ~~~B.value) - 1#n) 0#n

/-- The smallest value a fact admits, read as an unsigned `bits<n>`. -/
def Fact.lo {n : Nat} (K : Fact n) : BitVec n := K.value

/-- The largest value a fact admits, read as an unsigned `bits<n>`. -/
def Fact.hi {n : Nat} (K : Fact n) : BitVec n := K.value ||| ~~~K.mask

/-- The `bits<1>` fact a decided comparison produces. -/
def Fact.bit (b : Bool) : Fact 1 := Fact.const (BitVec.ofBool b)

/-- The `icmp` procedure of 8.10.  Equality is decided by a bit known in both
with different values, or by both operands being fully known; the ordered
comparisons by intervals, the signed ones only when both sign bits are
known.  No other case is decided. -/
def T_icmp {n : Nat} (o : CmpOp) (A B : Fact n) : Fact 1 :=
  match o with
  | .eq =>
      if (A.value ^^^ B.value) &&& A.mask &&& B.mask ≠ 0#n then Fact.bit false
      else if fullyKnown A ∧ fullyKnown B then Fact.bit true
      else Fact.top 1
  | .ne =>
      if (A.value ^^^ B.value) &&& A.mask &&& B.mask ≠ 0#n then Fact.bit true
      else if fullyKnown A ∧ fullyKnown B then Fact.bit false
      else Fact.top 1
  | .ult =>
      if A.hi.ult B.lo then Fact.bit true
      else if A.lo.ult B.hi = false then Fact.bit false
      else Fact.top 1
  | .ule =>
      if A.hi.ule B.lo then Fact.bit true
      else if A.lo.ule B.hi = false then Fact.bit false
      else Fact.top 1
  | .slt =>
      if A.mask.msb ∧ B.mask.msb then
        if A.hi.slt B.lo then Fact.bit true
        else if A.lo.slt B.hi = false then Fact.bit false
        else Fact.top 1
      else Fact.top 1
  | .sle =>
      if A.mask.msb ∧ B.mask.msb then
        if A.hi.sle B.lo then Fact.bit true
        else if A.lo.sle B.hi = false then Fact.bit false
        else Fact.top 1
      else Fact.top 1

/-- A shift amount the table admits: fully known and below the width.  The
amount's own width `m` is unconstrained; the comparison is on `Nat`, never on
a width-`m` truncation of `n`. -/
def shiftAmount {n m : Nat} (K : Fact m) : Option Nat :=
  if fullyKnown K ∧ K.value.toNat < n then some K.value.toNat else none

def T_shl {n m : Nat} (A : Fact n) (K : Fact m) : Fact n :=
  match shiftAmount (n := n) K with
  | some s => Fact.mk' ((A.mask <<< s) ||| ((1#n <<< s) - 1#n)) (A.value <<< s)
  | none => Fact.top n

def T_lshr {n m : Nat} (A : Fact n) (K : Fact m) : Fact n :=
  match shiftAmount (n := n) K with
  | some s => Fact.mk' ((A.mask >>> s) ||| ~~~(BitVec.allOnes n >>> s)) (A.value >>> s)
  | none => Fact.top n

def T_ashr {n m : Nat} (A : Fact n) (K : Fact m) : Fact n :=
  match shiftAmount (n := n) K with
  | some s => Fact.mk' (A.mask.sshiftRight s) (A.value.sshiftRight s)
  | none => Fact.top n

def T_zext {w : Nat} (n : Nat) (A : Fact w) : Fact n :=
  Fact.mk' (BitVec.zeroExtend n A.mask ||| ~~~(BitVec.zeroExtend n (BitVec.allOnes w)))
    (BitVec.zeroExtend n A.value)

def T_sext {w : Nat} (n : Nat) (A : Fact w) : Fact n :=
  Fact.mk' (BitVec.signExtend n A.mask) (BitVec.signExtend n A.value)

def T_trunc {w : Nat} (n : Nat) (A : Fact w) : Fact n :=
  Fact.mk' (BitVec.truncate n A.mask) (BitVec.truncate n A.value)

/-! ## The transfer as one function of the operator -/

/-- The fact tuple an operator's transfer consumes. -/
def Op.factDom : Op → Type
  | .bin _ n => Fact n × Fact n
  | .cmp _ n => Fact n × Fact n
  | .shift _ n m => Fact n × Fact m
  | .cast _ w _ => Fact w

/-- Every operand fact holds of the corresponding operand. -/
def gammaDom : (o : Op) → o.factDom → o.dom → Prop
  | .bin _ _, (A, B), (x, y) => gamma A x ∧ gamma B y
  | .cmp _ _, (A, B), (x, y) => gamma A x ∧ gamma B y
  | .shift _ _ _, (A, K), (x, k) => gamma A x ∧ gamma K k
  | .cast _ _ _, A, x => gamma A x

/-- One transfer function, dispatching on the operator, so the evaluator and
the proofs speak about the same rows. -/
def transfer : (o : Op) → o.factDom → Fact o.res
  | .bin .add _, (A, B) => T_add A B
  | .bin .sub _, (A, B) => T_sub A B
  | .bin .mul _, (A, B) => T_mul A B
  | .bin .and _, (A, B) => T_and A B
  | .bin .or _, (A, B) => T_or A B
  | .bin .xor _, (A, B) => T_xor A B
  | .cmp o _, (A, B) => T_icmp o A B
  | .shift .shl _ _, (A, K) => T_shl A K
  | .shift .lshr _ _, (A, K) => T_lshr A K
  | .shift .ashr _ _, (A, K) => T_ashr A K
  | .cast .zext _ n, A => T_zext n A
  | .cast .sext _ n, A => T_sext n A
  | .cast .trunc _ n, A => T_trunc n A

/-- The fixed statement every transfer row is proved against. -/
def TransferSound (o : Op) (T : o.factDom → Fact o.res) : Prop :=
  ∀ (a : o.factDom) (d : o.dom), gammaDom o a d → gamma (T a) (denoteOp o d)

/-- One row of the table, at the widths the operator fixes. -/
def TransferRowSound (o : Op) : Prop := TransferSound o (transfer o)

end EvoIR
