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

end EvoIR.Core.Symbolic.MachineSelection
