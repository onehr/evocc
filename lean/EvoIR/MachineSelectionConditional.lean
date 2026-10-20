import EvoIR.MachineSelectionNormal
import EvoIR.MachineSelectionSourceSpan

/-!
Conditional term checking uses the successful ordered source guard trace.
In particular, hardware-masked shift counts are not identified with arbitrary
source counts. Every such replacement carries the actual passed-count proof.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem guards_member_passed {Γ : Var → Nat} (σ : Subst Γ)
    (checks : List (Guard Γ)) (passed : guards σ checks = .ok ())
    (g : Guard Γ) (member : g ∈ checks) : (denoteTerm σ g.condition).getLsbD 0 = true := by
  induction checks with
  | nil => cases member
  | cons head rest ih =>
    simp only [guards] at passed
    split at passed
    · contradiction
    · rename_i nonzero
      cases List.mem_cons.mp member with
      | inl same =>
        subst g
        have bounded := (denoteTerm σ head.condition).isLt
        have value : (denoteTerm σ head.condition).toNat = 1 := by
          simp only [beq_iff_eq] at nonzero
          omega
        have bits : denoteTerm σ head.condition = 1#1 := BitVec.eq_of_toNat_eq (by simpa using value)
        rw [bits]
        rfl
      | inr member => exact ih passed member

private theorem passed_count_bound {Γ : Var → Nat} (σ : Subst Γ) {n m : Nat}
    (signed : Bool) (value : Term Γ n) (count : Term Γ m)
    (passed : (denoteTerm σ (.count signed value count)).getLsbD 0 = true) :
    (denoteTerm σ count).toNat < n := by
  cases signed
  · simpa only [denoteTerm, denoteOp, BitVec.getLsbD_ofBool, decide_true,
      Bool.true_and, Bool.and_eq_true, decide_eq_true_eq] using passed
  · simp only [denoteTerm, denoteOp, BitVec.getLsbD_ofBool, Bool.and_eq_true,
      decide_eq_true_eq] at passed
    rw [BitVec.toInt_eq_toNat_cond] at passed
    have bounded := (denoteTerm σ count).isLt
    split at passed <;> omega

private abbrev UnmaskedCount {Γ : Var → Nat} (checks : List (Guard Γ))
    {k : Nat} (amount : Term Γ k) :=
  {word : Word Γ // ∀ σ, guards σ checks = .ok () →
    (denoteTerm σ word.2).toNat = (denoteTerm σ amount).toNat}

private def countCandidate {Γ : Var → Nat} (checks : List (Guard Γ)) (bound : Nat)
    {w k : Nat} (op : CastOp) (raw : Term Γ w) (g : Guard Γ) (member : g ∈ checks) :
    Option (UnmaskedCount checks (.cast op k raw)) :=
  if notSigned : op ≠ .sext then
    match shape : g.condition with
    | .count (n := n) (m := m) signed value count =>
      if sameBound : n = bound then
        if small : k ≤ m ∧ m ≤ 64 ∧ bound ≤ 2^k then do
          let equal ← checkTerm count (simplifyCast .trunc m raw)
          return ⟨⟨64, encode count⟩, by
            intro σ successful
            have good := guards_member_passed σ checks successful g member
            rw [shape] at good
            have bounded := passed_count_bound σ signed value count good
            have fits : (denoteTerm σ count).toNat < 2^k := by omega
            have same : (denoteTerm σ raw).setWidth m = denoteTerm σ count := by
              have law := equal.down σ
              simpa only [simplify_cast_sound, denoteTerm, denoteOp] using law.symm
            have low : (denoteTerm σ raw).setWidth k = (denoteTerm σ count).setWidth k := by
              rw [← same, BitVec.setWidth_setWidth_of_le _ small.1]
            have target : denoteTerm σ (.cast op k raw) = (denoteTerm σ raw).setWidth k := by
              cases op <;> simp_all [denoteTerm, denoteOp,
                BitVec.zeroExtend_eq_setWidth, BitVec.truncate_eq_setWidth]
            rw [encode_sound, BitVec.toNat_setWidth_of_le small.2.1, target, low,
              BitVec.toNat_setWidth, Nat.mod_eq_of_lt fits]⟩
        else none
      else none
    | _ => none
  else none

private def findCountCandidate {Γ : Var → Nat} (checks : List (Guard Γ)) (bound : Nat)
    {w k : Nat} (op : CastOp) (raw : Term Γ w) :
    (remaining : List (Guard Γ)) → (included : ∀ g ∈ remaining, g ∈ checks) →
    Option (UnmaskedCount checks (.cast op k raw))
  | [], included => none
  | g :: rest, included =>
    match countCandidate checks bound op raw g (included g (by simp)) with
    | some result => some result
    | none => findCountCandidate checks bound op raw rest
      (by intro g member; exact included g (List.mem_cons_of_mem _ member))

private def countUnder {Γ : Var → Nat} (checks : List (Guard Γ)) (bound : Nat)
    {k : Nat} (amount : Term Γ k) : UnmaskedCount checks amount :=
  let proposed : Option (UnmaskedCount checks amount) := match amount with
    | .cast op k raw => findCountCandidate checks bound op raw checks (by intro g member; exact member)
    | _ => none
  match proposed with
  | some result => result
  | none =>
    if small : k ≤ 64 then ⟨⟨64, encode amount⟩, by
      intro σ successful
      rw [encode_sound, BitVec.toNat_setWidth_of_le small]⟩
    else ⟨⟨k, amount⟩, by intro σ successful; rfl⟩

/-- A width-preserving symbolic normalization with an explicit successful-
trace premise. Count widths can change, but integer operation widths cannot. -/
def termUnder {Γ : Var → Nat} (checks : List (Guard Γ)) :
    {n : Nat} → (term : Term Γ n) →
    {normal : Term Γ n // ∀ σ, guards σ checks = .ok () → denoteTerm σ normal = denoteTerm σ term}
  | _, .var v n h => ⟨.var v n h, by intro σ successful; rfl⟩
  | _, .const n bits => ⟨.const n bits, by intro σ successful; rfl⟩
  | _, .bin op a b =>
    let left := termUnder checks a
    let right := termUnder checks b
    ⟨.bin op left.val right.val, by
      intro σ successful
      simp only [denoteTerm, left.property σ successful, right.property σ successful]⟩
  | _, .cmp op a b =>
    let left := termUnder checks a
    let right := termUnder checks b
    ⟨.cmp op left.val right.val, by
      intro σ successful
      simp only [denoteTerm, left.property σ successful, right.property σ successful]⟩
  | n, .shift op a count =>
    let left := termUnder checks a
    let right := termUnder checks count
    let amount := countUnder checks n right.val
    ⟨.shift op left.val amount.val.2, by
      intro σ successful
      have counts := amount.property σ successful
      rw [right.property σ successful] at counts
      cases op <;> simp only [denoteTerm, denoteOp, Op.res,
        left.property σ successful, counts]⟩
  | _, .cast op n a =>
    let argument := termUnder checks a
    ⟨simplifyCast op n argument.val, by
      intro σ successful
      simp only [simplify_cast_sound, denoteTerm, argument.property σ successful]⟩
  | _, .guard op a b =>
    let left := termUnder checks a
    let right := termUnder checks b
    ⟨.guard op left.val right.val, by
      intro σ successful
      simp only [denoteTerm, left.property σ successful, right.property σ successful]⟩
  | _, .nonzero a =>
    let argument := termUnder checks a
    ⟨.nonzero argument.val, by
      intro σ successful
      simp only [denoteTerm, argument.property σ successful]⟩
  | _, .count signed a b =>
    let left := termUnder checks a
    let right := termUnder checks b
    ⟨.count signed left.val right.val, by
      intro σ successful
      simp only [denoteTerm, left.property σ successful, right.property σ successful]⟩
  | _, .division signed remainder a b =>
    let left := termUnder checks a
    let right := termUnder checks b
    ⟨.division signed remainder left.val right.val, by
      intro σ successful
      simp only [denoteTerm, left.property σ successful, right.property σ successful]⟩

def checkTermUnder {Γ : Var → Nat} (checks : List (Guard Γ)) {n : Nat}
    (source target : Term Γ n) : Option (PLift (∀ σ, guards σ checks = .ok () →
      denoteTerm σ source = denoteTerm σ target)) :=
  match checkTerm source target with
  | some equal => some ⟨by intro σ successful; exact equal.down σ⟩
  | none => do
    let left := termUnder checks source
    let right := termUnder checks target
    let equal ← checkTerm left.val right.val
    return ⟨by
      intro σ successful
      exact (left.property σ successful).symm.trans
        ((equal.down σ).trans (right.property σ successful))⟩

def checkWordUnder {Γ : Var → Nat} (checks : List (Guard Γ))
    (source : Word Γ) (target : WordExpr Γ) :
    Option (PLift (∀ σ, guards σ checks = .ok () →
      target.denote σ = (denoteTerm σ source.2).setWidth 64)) :=
  match checkWord source target with
  | some equal => some ⟨by intro σ successful; exact equal.down σ⟩
  | none => do
    let quoted ← wordDiagram target
    let equal ← checkTermUnder checks quoted.val (encode source.2)
    return ⟨by
      intro σ successful
      exact (quoted.property σ).symm.trans
        ((equal.down σ successful).trans (encode_sound σ source.2))⟩

def checkFlagUnder {Γ : Var → Nat} (checks : List (Guard Γ))
    (source : Term Γ 1) (flags : FlagExpr Γ) (cond : EvoIR.Machine.Cond) :
    Option (PLift (∀ σ, guards σ checks = .ok () →
      EvoIR.Machine.condition (flags.denote σ) cond = .ok ((denoteTerm σ source).getLsbD 0))) := do
  let quoted ← flagDiagram flags cond
  let equal ← checkTermUnder checks source quoted.val
  return ⟨by
    intro σ successful
    rw [flag_truth_sound, ← quoted.property σ, ← equal.down σ successful]⟩

private theorem widened_zero_test (width : EvoIR.Machine.Width) (value : BitVec 64) :
    ((EvoIR.Machine.normalizeAt width value).setWidth 64 == 0) =
      (value.setWidth width.bits == 0) := by
  apply Bool.eq_iff_iff.mpr
  simp only [beq_iff_eq]
  cases width
  · change (value.setWidth 32).setWidth 64 = 0#64 ↔ value.setWidth 32 = 0#32
    constructor
    · intro equal
      have low := congrArg (fun bits : BitVec 64 => bits.setWidth 32) equal
      simpa only [BitVec.setWidth_setWidth_of_le _ (by decide : 32 ≤ 64),
        BitVec.setWidth_eq, BitVec.ofNat_eq_ofNat, BitVec.setWidth_zero] using low
    · intro equal
      rw [equal, BitVec.setWidth_zero]
  · rfl

/-- Try both the native-width comparison and its zero-extended 64-bit form.
Both test exactly the same low bits. This permits a source widening cast
without granting equality between arbitrary 32-bit and 64-bit comparisons. -/
private def checkZeroFlagUnder {Γ : Var → Nat} (checks : List (Guard Γ))
    (source : Term Γ 1) (word : WordExpr Γ) (width : EvoIR.Machine.Width)
    (nonzero : Bool) :=
  let cond := if nonzero then EvoIR.Machine.Cond.ne else .eq
  match checkFlagUnder checks source (.comparison width word (.literal 0)) cond with
  | some equal => some equal
  | none => do
    let equal ← checkFlagUnder checks source
      (.comparison .x (.normalize width.class word) (.literal 0)) cond
    return ⟨by
      intro σ passed
      have truth := equal.down σ passed
      have same := widened_zero_test width (word.denote σ)
      cases width <;> cases nonzero <;>
        simp_all [cond, FlagExpr.denote, WordExpr.denote, EvoIR.Machine.condition,
          EvoIR.Machine.Width.class, EvoIR.Machine.Width.bits, EvoIR.Machine.normalizeAt,
          bne]
      all_goals exact same.symm.trans truth⟩

/-- Check the value actually tested by CBZ/CBNZ, not a producer claim that
it encodes a Boolean. The returned polarity determines the source edge.
The comparison is at the instruction's register width. -/
def zeroTestUnder {Γ : Var → Nat} (checks : List (Guard Γ))
    (source : Term Γ 1) (word : WordExpr Γ) (width : EvoIR.Machine.Width) :
    Option {passWhen : Bool // ∀ σ, guards σ checks = .ok () →
      ((word.denote σ).setWidth width.bits != 0) =
        (if (denoteTerm σ source).getLsbD 0 then passWhen else !passWhen)} :=
  match checkWordUnder checks ⟨1, source⟩ word with
  | some equal => some ⟨true, by
      intro σ passed
      rw [equal.down σ passed, one_bit_reify (denoteTerm σ source)]
      cases (denoteTerm σ source).getLsbD 0 <;> cases width <;> rfl⟩
  | none =>
    match checkZeroFlagUnder checks source word width true with
    | some equal => some ⟨true, by
        intro σ passed
        have truth := equal.down σ passed
        simpa [FlagExpr.denote, WordExpr.denote, EvoIR.Machine.condition] using truth⟩
    | none => do
      let equal ← checkZeroFlagUnder checks source word width false
      return ⟨false, by
        intro σ passed
        have truth := equal.down σ passed
        simp only [FlagExpr.denote, WordExpr.denote, EvoIR.Machine.condition,
          BitVec.setWidth_zero, Except.ok.injEq] at truth
        have opposite := congrArg Bool.not truth
        cases bit : (denoteTerm σ source).getLsbD 0 <;>
          simpa only [BitVec.ofNat_eq_ofNat, bne, bit, Bool.not_true, Bool.not_false, Bool.false_eq_true, ↓reduceIte,
            Bool.not_eq_true, Bool.not_eq_false, beq_iff_eq, beq_eq_false_iff_ne] using opposite⟩

end EvoIR.Core.Symbolic.MachineSelection
