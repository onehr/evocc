import EvoIR.Core.SymbolicInstruction

/-!
Proof-diagram normalization for instruction selection. It changes neither
decoded input. In particular, the low-width view of a machine register is
not identified with its 64-bit value without a proved width-conversion law.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem cast_same_width (op : CastOp) {n : Nat} (value : BitVec n) :
    denoteOp (.cast op n n) value = value := by
  cases op <;> simp [denoteOp, BitVec.zeroExtend_eq_setWidth,
    BitVec.truncate_eq_setWidth]

theorem nonsigned_cast_compose (outer inner : CastOp) {a b c : Nat}
    (notSignedOuter : outer ≠ .sext) (notSignedInner : inner ≠ .sext)
    (shorter : c ≤ b) (value : BitVec a) :
    denoteOp (.cast outer b c) (denoteOp (.cast inner a b) value) =
      denoteOp (.cast .zext a c) value := by
  cases outer <;> cases inner <;>
    simp_all [denoteOp, BitVec.zeroExtend_eq_setWidth,
      BitVec.truncate_eq_setWidth, BitVec.setWidth_setWidth_of_le]

/-- A wider destination also permits composition when the intermediate view
did not truncate the original value. This is needed for one-bit block
arguments carried by a 32-bit machine register and then read as 64 bits. -/
theorem nonsigned_cast_compose_widen (outer inner : CastOp) {a b c : Nat}
    (notSignedOuter : outer ≠ .sext) (notSignedInner : inner ≠ .sext)
    (preserved : a ≤ b) (value : BitVec a) :
    denoteOp (.cast outer b c) (denoteOp (.cast inner a b) value) =
      denoteOp (.cast .zext a c) value := by
  have safe : ¬(b < a ∧ b < c) := by omega
  cases outer <;> cases inner <;>
    simp_all [denoteOp, BitVec.zeroExtend_eq_setWidth,
      BitVec.truncate_eq_setWidth, BitVec.setWidth_setWidth safe]

theorem truncate_signed_cast (inner : CastOp) {a b c : Nat}
    (shorter : c ≤ b) (original : c ≤ a) (value : BitVec a) :
    denoteOp (.cast .trunc b c) (denoteOp (.cast inner a b) value) =
      denoteOp (.cast .trunc a c) value := by
  cases inner with
  | zext | trunc =>
    simp [denoteOp, BitVec.zeroExtend_eq_setWidth, BitVec.truncate_eq_setWidth,
      BitVec.setWidth_setWidth_of_le, shorter]
  | sext =>
    change (value.signExtend b).setWidth c = value.setWidth c
    apply BitVec.eq_of_getLsbD_eq
    intro i hi
    simp only [BitVec.getLsbD_setWidth, BitVec.getLsbD_signExtend]
    simp [show i < b by omega, show i < a by omega, hi]

def simplifyCast {Γ : Var → Nat} (op : CastOp) {w : Nat} (n : Nat)
    (term : Term Γ w) : Term Γ n :=
  if same : w = n then same ▸ term
  else match term with
  | .cast (w := originalWidth) inner _ body =>
    if op ≠ .sext ∧ inner ≠ .sext ∧ n ≤ w then
      Term.constantCast .zext n body
    else if op ≠ .sext ∧ inner ≠ .sext ∧ originalWidth ≤ w then
      Term.constantCast .zext n body
    else if op = .trunc ∧ n ≤ w ∧ n ≤ originalWidth then
      Term.constantCast .trunc n body
    else Term.constantCast op n term
  | _ => Term.constantCast op n term

theorem simplify_cast_sound {Γ : Var → Nat} (σ : Subst Γ) (op : CastOp)
    {w : Nat} (n : Nat) (term : Term Γ w) :
    denoteTerm σ (simplifyCast op n term) = denoteTerm σ (.cast op n term) := by
  cases term <;> dsimp only [simplifyCast]
  all_goals split
  all_goals
    first
    | rename_i same
      subst same
      exact (cast_same_width op _).symm
    | skip
  all_goals try exact Term.constant_cast_sound σ op n _
  rename_i inner a body different
  split
  · rename_i h
    rw [Term.constant_cast_sound]
    exact (nonsigned_cast_compose op inner h.1 h.2.1 h.2.2 (denoteTerm σ body)).symm
  · split
    · rename_i h
      rw [Term.constant_cast_sound]
      exact (nonsigned_cast_compose_widen op inner h.1 h.2.1 h.2.2 (denoteTerm σ body)).symm
    · split
      · rename_i h
        rw [Term.constant_cast_sound]
        rcases h with ⟨rfl, shorter, original⟩
        exact (truncate_signed_cast inner shorter original (denoteTerm σ body)).symm
      · exact Term.constant_cast_sound σ op n _

def normalize {Γ : Var → Nat} : {n : Nat} → Term Γ n → Term Γ n
  | _, .var v n h => .var v n h
  | _, .const n c => .const n c
  | _, .bin op a b => .bin op (normalize a) (normalize b)
  | _, .cmp op a b => .cmp op (normalize a) (normalize b)
  | _, .shift op a b => .shift op (normalize a) (normalize b)
  | _, .cast op n a => simplifyCast op n (normalize a)
  | _, .guard op a b => .guard op (normalize a) (normalize b)
  | _, .nonzero a => .nonzero (normalize a)
  | _, .count s a b => .count s (normalize a) (normalize b)
  | _, .division s r a b => .division s r (normalize a) (normalize b)

theorem normalize_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat}
    (term : Term Γ n) : denoteTerm σ (normalize term) = denoteTerm σ term := by
  induction term <;> simp_all only [normalize, simplify_cast_sound, denoteTerm]

def sameTerm {Γ : Var → Nat} {n : Nat} (source target : Term Γ n) : Bool :=
  decide ((normalize source).normalizeValue = (normalize target).normalizeValue)

theorem same_term_sound {Γ : Var → Nat} {n : Nat} (σ : Subst Γ)
    (source target : Term Γ n) (accepted : sameTerm source target = true) :
    denoteTerm σ source = denoteTerm σ target := by
  have equal : (normalize source).normalizeValue = (normalize target).normalizeValue :=
    of_decide_eq_true accepted
  have sem := congrArg (denoteTerm σ) equal
  simpa only [Term.normalize_value_sound, normalize_sound] using sem

end EvoIR.Core.Symbolic.MachineSelection
