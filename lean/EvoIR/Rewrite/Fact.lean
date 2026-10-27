import EvoIR.Core.IRExecution
import EvoIR.Proofs.Transfer
import EvoIR.Proofs.GuardTransfer
import EvoIR.Proofs.Lattice

/-!
Known-bits facts for the rewrite checker. A fact is kept as a width and two
natural numbers. The typed `Fact` of Section 8.10 is made again at each use.

The checker uses only the transfer rows that have a kernel-only proof:
`const`, `and`, `or`, `xor`, the three shifts, the three width changes and
the seven guard rows. The rows for `add`, `sub`, `mul` and the comparisons
are proved with `bv_decide` per width, in other modules. This file does not
import or use them. Their result is top, which is always sound.
-/

namespace EvoIR.Rewrite

open EvoIR.Core.IR

structure RFact where
  width : Nat
  mask : Nat
  value : Nat
  deriving DecidableEq, Repr, Inhabited

def toFact (n mask value : Nat) : Fact n :=
  Fact.mk' (BitVec.ofNat n mask) (BitVec.ofNat n value)

def ofFact {n : Nat} (K : Fact n) : RFact := ⟨n, K.mask.toNat, K.value.toNat⟩

def RFact.top (n : Nat) : RFact := ⟨n, 0, 0⟩

/-- The typed fact at width `n`. A fact of another width gives top. -/
def RFact.at (a : RFact) (n : Nat) : Fact n :=
  if a.width = n then toFact n a.mask a.value else Fact.top n

/-- A fact holds of a value when the widths agree and the known bits agree.
A value of another shape or width gets no claim. -/
def Holds (a : RFact) : Value → Prop
  | .bits n x => a.width = n → gamma (toFact n a.mask a.value) x
  | _ => True

theorem fact_ofFact {n : Nat} (K : Fact n) : toFact n K.mask.toNat K.value.toNat = K := by
  apply Fact.ext
  · simp [toFact, Fact.mk']
  · simp only [toFact, Fact.mk', BitVec.ofNat_toNat, BitVec.setWidth_eq]
    apply BitVec.eq_of_getLsbD_eq
    intro i hi
    have h := norm_bit K hi
    simp only [BitVec.getLsbD_and]
    revert h
    cases K.value.getLsbD i <;> cases K.mask.getLsbD i <;> simp

theorem gamma_mask_zero {n : Nat} (value : Nat) (x : BitVec n) : gamma (toFact n 0 value) x := by
  simp [gamma, toFact, Fact.mk']

theorem holds_top (n : Nat) (v : Value) : Holds (RFact.top n) v := by
  cases v <;> simp [Holds, RFact.top, gamma_mask_zero]

theorem holds_ofFact {n : Nat} {K : Fact n} {x : BitVec n} (h : gamma K x) :
    Holds (ofFact K) (.bits n x) := by
  intro _
  show gamma (toFact n K.mask.toNat K.value.toNat) x
  rw [fact_ofFact]
  exact h

theorem holds_at {a : RFact} {n : Nat} {x : BitVec n} (h : Holds a (.bits n x)) :
    gamma (a.at n) x := by
  unfold RFact.at
  split
  · exact h (by assumption)
  · simp [gamma, Fact.top]

/-! ## Order and merge -/

theorem gamma_le {n : Nat} {K K' : Fact n} {x : BitVec n} (h : gamma K x) (le : K.le K') :
    gamma K' x := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  have e := gamma_bit h i
  have l1 := congrArg (fun z => BitVec.getLsbD z i) le.1
  have l2 := congrArg (fun z => BitVec.getLsbD z i) le.2
  have n' := norm_bit K' hi
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_not, BitVec.getLsbD_zero, hi, decide_true,
    Bool.true_and] at l1 l2 ⊢
  revert e l1 l2 n'
  cases K.mask.getLsbD i <;> cases K.value.getLsbD i <;> cases K'.mask.getLsbD i <;>
    cases K'.value.getLsbD i <;> cases x.getLsbD i <;> simp

/-- Section 8.10 order: every bit that `b` claims, `a` knows with the same
value. A fact with no known bit is above every fact of any width. -/
def RFact.le (a b : RFact) : Bool :=
  b.mask == 0 || (a.width == b.width &&
    decide ((toFact a.width a.mask a.value).le (toFact a.width b.mask b.value)))

theorem holds_le {a b : RFact} {v : Value} (h : Holds a v) (le : a.le b = true) : Holds b v := by
  obtain ⟨w, m, x⟩ := a
  obtain ⟨w', m', x'⟩ := b
  simp only [RFact.le, Bool.or_eq_true, beq_iff_eq, Bool.and_eq_true, decide_eq_true_eq] at le
  rcases le with hm | ⟨hw, hle⟩
  · subst hm
    cases v with
    | bits n y => intro _; exact gamma_mask_zero _ y
    | _ => trivial
  · subst hw
    cases v with
    | bits n y =>
      intro hn
      subst hn
      exact gamma_le (h rfl) hle
    | _ => trivial

/-- Section 8.10 merge. A conflict and a width mismatch give no fact. -/
def RFact.meet (a b : RFact) : Option RFact :=
  if a.width = b.width then
    ((toFact a.width a.mask a.value).meet (toFact a.width b.mask b.value)).map ofFact
  else none

theorem holds_meet {a b c : RFact} {v : Value} (ha : Holds a v) (hb : Holds b v)
    (h : a.meet b = some c) : Holds c v := by
  obtain ⟨w, m, x⟩ := a
  obtain ⟨w', m', x'⟩ := b
  unfold RFact.meet at h
  split at h
  · rename_i hw
    simp only at hw
    subst hw
    cases v with
    | bits n y =>
      by_cases hn : w = n
      · subst hn
        obtain ⟨K, hK, hg⟩ := MeetSound _ _ y (ha rfl) (hb rfl)
        simp only [hK, Option.map_some, Option.some.injEq] at h
        subst h
        exact holds_ofFact hg
      · cases h' : (toFact w m x).meet (toFact w m' x') with
        | none => simp [h'] at h
        | some K =>
          simp only [h', Option.map_some, Option.some.injEq] at h
          subst h
          intro hw'
          exact absurd hw' hn
    | _ => trivial
  · contradiction

/-- Section 8.10 join, used only to propose a block-argument table. The
proposal is checked; this function carries no proof obligation. -/
def RFact.join (a b : RFact) : RFact :=
  if a.width = b.width then
    ofFact ((toFact a.width a.mask a.value).join (toFact a.width b.mask b.value))
  else RFact.top a.width

/-! ## Transfer -/

/-- The operand list an operator consumes, as IR values. -/
def opValues : (o : Op) → o.dom → List Value
  | .bin _ n, (x, y) => [.bits n x, .bits n y]
  | .cmp _ n, (x, y) => [.bits n x, .bits n y]
  | .shift _ n m, (x, k) => [.bits n x, .bits m k]
  | .cast _ w _, x => [.bits w x]
  | .guard _ n, (x, y) => [.bits n x, .bits n y]
  | .nonzero n, x => [.bits n x]
  | .count _ n m, (x, k) => [.bits n x, .bits m k]

/-- The widths of the operands an operator consumes. -/
def opInputs : Op → List Nat
  | .bin _ n | .cmp _ n | .guard _ n => [n, n]
  | .shift _ n m | .count _ n m => [n, m]
  | .cast _ w _ | .nonzero w => [w]

def opFacts (o : Op) (fs : List RFact) : o.factDom :=
  match o with
  | .bin _ n => ((fs.getD 0 default).at n, (fs.getD 1 default).at n)
  | .cmp _ n => ((fs.getD 0 default).at n, (fs.getD 1 default).at n)
  | .shift _ n m => ((fs.getD 0 default).at n, (fs.getD 1 default).at m)
  | .cast _ w _ => (fs.getD 0 default).at w
  | .guard _ n => ((fs.getD 0 default).at n, (fs.getD 1 default).at n)
  | .nonzero n => (fs.getD 0 default).at n
  | .count _ n m => ((fs.getD 0 default).at n, (fs.getD 1 default).at m)

theorem asBits_ok {n : Nat} {v : Value} {x : BitVec n} (h : v.asBits n = .ok x) :
    v = .bits n x := by
  cases v with
  | bits m value =>
    simp only [Value.asBits] at h
    split at h
    · rename_i hm
      have hm : m = n := by simpa using hm
      subst hm
      simp only [BitVec.setWidth_eq, Except.ok.injEq] at h
      subst h
      rfl
    · cases h
  | poison => cases h
  | pointer => cases h
  | memory => cases h
  | world => cases h

theorem bind_asBits {n : Nat} {α : Type} {v : Value}
    {k : BitVec n → Except EvoIR.Core.Source.Fault α} {r : α}
    (h : (v.asBits n >>= k) = .ok r) : ∃ x, v = .bits n x ∧ k x = .ok r := by
  cases hv : v.asBits n with
  | error e => simp [hv, Bind.bind, Except.bind] at h
  | ok x => exact ⟨x, asBits_ok hv, by simpa [hv, Bind.bind, Except.bind] using h⟩

/-- A successful scalar evaluation consumed bit vectors of the operator's
widths and produced the result of `denoteOp`. -/
theorem evalScalar_ok {o : Op} {args : List Value} {r : Value} (h : evalScalar o args = .ok r) :
    ∃ d : o.dom, args = opValues o d ∧ r = .bits o.res (denoteOp o d) := by
  cases o with
  | bin op n =>
    match args, h with
    | [a, b], h =>
      simp only [evalScalar] at h
      obtain ⟨x, rfl, h1⟩ := bind_asBits h
      obtain ⟨y, rfl, h2⟩ := bind_asBits h1
      exact ⟨(x, y), rfl, (Except.ok.inj h2).symm⟩
  | cmp op n =>
    match args, h with
    | [a, b], h =>
      simp only [evalScalar] at h
      obtain ⟨x, rfl, h1⟩ := bind_asBits h
      obtain ⟨y, rfl, h2⟩ := bind_asBits h1
      exact ⟨(x, y), rfl, (Except.ok.inj h2).symm⟩
  | shift op n m =>
    match args, h with
    | [a, b], h =>
      simp only [evalScalar] at h
      obtain ⟨x, rfl, h1⟩ := bind_asBits h
      obtain ⟨y, rfl, h2⟩ := bind_asBits h1
      exact ⟨(x, y), rfl, (Except.ok.inj h2).symm⟩
  | cast op w n =>
    match args, h with
    | [a], h =>
      simp only [evalScalar] at h
      obtain ⟨x, rfl, h1⟩ := bind_asBits h
      exact ⟨x, rfl, (Except.ok.inj h1).symm⟩
  | guard op n =>
    match args, h with
    | [a, b], h =>
      simp only [evalScalar] at h
      obtain ⟨x, rfl, h1⟩ := bind_asBits h
      obtain ⟨y, rfl, h2⟩ := bind_asBits h1
      exact ⟨(x, y), rfl, (Except.ok.inj h2).symm⟩
  | nonzero n =>
    match args, h with
    | [a], h =>
      simp only [evalScalar] at h
      obtain ⟨x, rfl, h1⟩ := bind_asBits h
      exact ⟨x, rfl, (Except.ok.inj h1).symm⟩
  | count s n m =>
    match args, h with
    | [a, b], h =>
      simp only [evalScalar] at h
      obtain ⟨x, rfl, h1⟩ := bind_asBits h
      obtain ⟨y, rfl, h2⟩ := bind_asBits h1
      exact ⟨(x, y), rfl, (Except.ok.inj h2).symm⟩

/-- Every operand of the operator's widths gives a successful evaluation. -/
@[simp] theorem asBits_self {n : Nat} (x : BitVec n) : (Value.bits n x).asBits n = .ok x := by
  simp [Value.asBits]

theorem evalScalar_typed (o : Op) (d : o.dom) :
    evalScalar o (opValues o d) = .ok (.bits o.res (denoteOp o d)) := by
  cases o with
  | cast _ _ _ => simp [evalScalar, opValues, Bind.bind, Except.bind, Pure.pure, Except.pure, Op.res]
  | nonzero _ => simp [evalScalar, opValues, Bind.bind, Except.bind, Pure.pure, Except.pure, Op.res]
  | _ =>
    obtain ⟨x, y⟩ := d
    simp [evalScalar, opValues, Bind.bind, Except.bind, Pure.pure, Except.pure, Op.res]

theorem opValues_widths (o : Op) (d : o.dom) :
    (opValues o d).map Value.type = (opInputs o).map Ty.bits := by
  cases o <;> rfl

/-- The rows with a kernel-only `TransferRowSound` theorem. -/
def kernelRow : Op → Bool
  | .bin .and _ | .bin .or _ | .bin .xor _ => true
  | .shift _ _ _ | .cast _ _ _ | .guard _ _ | .nonzero _ | .count _ _ _ => true
  | _ => false

theorem kernelRow_sound : ∀ o, kernelRow o = true → TransferRowSound o
  | .bin .and n, _ => transfer_and n
  | .bin .or n, _ => transfer_or n
  | .bin .xor n, _ => transfer_xor n
  | .shift .shl n m, _ => transfer_shl n m
  | .shift .lshr n m, _ => transfer_lshr n m
  | .shift .ashr n m, _ => transfer_ashr n m
  | .cast .zext w n, _ => transfer_zext w n
  | .cast .sext w n, _ => transfer_sext w n
  | .cast .trunc w n, _ => transfer_trunc w n
  | .guard .add n, _ => transfer_guard_add n
  | .guard .sub n, _ => transfer_guard_sub n
  | .guard .mul n, _ => transfer_guard_mul n
  | .guard .divRem n, _ => transfer_guard_div_rem n
  | .nonzero n, _ => transfer_guard_nonzero n
  | .count false n m, _ => transfer_guard_unsigned_count n m
  | .count true n m, _ => transfer_guard_signed_count n m
  | .bin .add _, h | .bin .sub _, h | .bin .mul _, h | .cmp _ _, h => by simp [kernelRow] at h

def rawTransfer (o : Op) (fs : List RFact) : RFact :=
  if kernelRow o then ofFact (transfer o (opFacts o fs)) else RFact.top o.res

def constFact (w v : Nat) : RFact := ofFact (Fact.const (BitVec.ofNat w v))

theorem holds_const (w v : Nat) : Holds (constFact w v) (.bits w (BitVec.ofNat w v)) :=
  holds_ofFact (transfer_const w _)

/-- Facts that hold of each operand position. -/
def HoldsAll (fs : List RFact) (args : List Value) : Prop :=
  ∀ i v, args[i]? = some v → Holds (fs.getD i default) v

theorem opFacts_sound (o : Op) (fs : List RFact) (d : o.dom)
    (h : HoldsAll fs (opValues o d)) : gammaDom o (opFacts o fs) d := by
  cases o with
  | bin op n => exact ⟨holds_at (h 0 _ rfl), holds_at (h 1 _ rfl)⟩
  | cmp op n => exact ⟨holds_at (h 0 _ rfl), holds_at (h 1 _ rfl)⟩
  | shift op n m => exact ⟨holds_at (h 0 _ rfl), holds_at (h 1 _ rfl)⟩
  | cast op w n => exact holds_at (h 0 _ rfl)
  | guard op n => exact ⟨holds_at (h 0 _ rfl), holds_at (h 1 _ rfl)⟩
  | nonzero n => exact holds_at (h 0 _ rfl)
  | count s n m => exact ⟨holds_at (h 0 _ rfl), holds_at (h 1 _ rfl)⟩

theorem rawTransfer_sound {o : Op} {fs : List RFact} {args : List Value} {r : Value}
    (eval : evalScalar o args = .ok r) (inputs : HoldsAll fs args) :
    Holds (rawTransfer o fs) r := by
  obtain ⟨d, rfl, rfl⟩ := evalScalar_ok eval
  unfold rawTransfer
  split
  · rename_i hk
    exact holds_ofFact (kernelRow_sound o hk (opFacts o fs) d (opFacts_sound o fs d inputs))
  · exact holds_top _ _

end EvoIR.Rewrite
