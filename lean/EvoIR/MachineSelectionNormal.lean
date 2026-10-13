import EvoIR.MachineSelectionConvert
import EvoIR.Core.PathCondition

/-!
Semantic normalization of proof diagrams only. The input/output snapshots
stay frozen. Total division values are normalized independently from their
ordered diagnostic guards; count checks are not dropped or globally assumed.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem mask_low_bits {n : Nat} (value : BitVec n) (count : Nat) :
    value &&& (BitVec.allOnes count).setWidth n = (value.setWidth count).setWidth n := by
  apply BitVec.eq_of_getLsbD_eq
  intro bit bound
  simp [BitVec.getLsbD_and, BitVec.getLsbD_setWidth, BitVec.getLsbD_allOnes,
    bound, Bool.and_comm]

/-- A low-bit mask is a representation conversion, not an assumed range
fact. Search only proposes its width; the exact bit-vector equality is
checked before applying the universally proved conversion law. -/
def lowMaskWidth {n : Nat} (mask : BitVec n) :
    Option {count : Nat // mask = (BitVec.allOnes count).setWidth n} := do
  let found : {count // (List.range (n+1)).find?
      (fun count => mask == (BitVec.allOnes count).setWidth n) = some count} ←
    match found : (List.range (n+1)).find?
        (fun count => mask == (BitVec.allOnes count).setWidth n) with
    | none => none
    | some count => some ⟨count, found⟩
  return ⟨found.val, by simpa only [beq_iff_eq] using List.find?_some found.property⟩

def simplifyBinary {Γ : Var → Nat} {n : Nat} (op : BinOp) (left right : Term Γ n) : Term Γ n :=
  match op, right with
  | .and, .const _ mask =>
    match lowMaskWidth mask with
    | some count => simplifyCast .zext n (simplifyCast .trunc count.val left)
    | none => .bin op left right
  | _, _ => .bin op left right

theorem simplify_binary_sound {Γ : Var → Nat} {n : Nat} (σ : Subst Γ)
    (op : BinOp) (left right : Term Γ n) :
    denoteTerm σ (simplifyBinary op left right) = denoteTerm σ (.bin op left right) := by
  cases op <;> cases right <;> try rfl
  rename_i mask
  simp only [simplifyBinary]
  split
  · rename_i count found
    simp only [simplify_cast_sound, denoteTerm, denoteOp, Op.res,
      BitVec.zeroExtend_eq_setWidth, BitVec.truncate_eq_setWidth]
    exact (mask_low_bits (denoteTerm σ left) count.val).symm.trans
      (congrArg (fun mask => denoteTerm σ left &&& mask) count.property.symm)
  · rfl

def scalarNormal {Γ : Var → Nat} : {n : Nat} → Term Γ n → Term Γ n
  | _, .var v n h => .var v n h
  | _, .const n c => .const n c
  | _, .bin op a b => simplifyBinary op (scalarNormal a) (scalarNormal b)
  | _, .cmp op a b => .cmp op (scalarNormal a) (scalarNormal b)
  | _, .shift op a b => .shift op (scalarNormal a) (scalarNormal b)
  | _, .cast op n a => simplifyCast op n (scalarNormal a)
  | _, .guard (n := n) op a b =>
    let x := scalarNormal a
    let y := scalarNormal b
    match op with
    | .divRem =>
      if 0 < n then .bin .or (.cmp .ne x (.const n (BitVec.intMin n)))
        (.cmp .ne y (.const n (BitVec.allOnes n)))
      else .guard op x y
    | _ => .guard op x y
  | _, .nonzero (n := n) a => .cmp .ne (scalarNormal a) (.const n 0)
  | _, .count (n := n) (m := m) signed a b =>
    let x := scalarNormal a
    let y := scalarNormal b
    if signed then
      if 0 < m ∧ n ≤ 2^(m-1) then .cmp .ult y (.const m (BitVec.ofNat m n))
      else .count signed x y
    else if n < 2^m then .cmp .ult y (.const m (BitVec.ofNat m n))
      else .count signed x y
  | _, .division signed remainder a b =>
    let x := scalarNormal a
    let y := scalarNormal b
    if remainder then .bin .sub x (.bin .mul (.division signed false x y) y)
    else .division signed false x y

theorem scalar_normal_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat} (term : Term Γ n) :
    denoteTerm σ (scalarNormal term) = denoteTerm σ term := by
  induction term with
  | var | const => rfl
  | bin => simp_all only [scalarNormal, simplify_binary_sound, denoteTerm]
  | cmp | shift => simp_all only [scalarNormal, denoteTerm]
  | cast op width a ih => simp only [scalarNormal, simplify_cast_sound, denoteTerm, ih]
  | nonzero a ih =>
    simp only [scalarNormal, denoteTerm, ih]
    exact (EvoIR.Machine.Selection.nonzero_as_comparison _).symm
  | @guard op n a b first second =>
    cases op <;> simp only [scalarNormal, denoteTerm, first, second]
    split
    · rename_i positive
      simp only [denoteTerm, first, second]
      exact (EvoIR.Machine.Selection.division_guard_as_disjunction _ _ positive).symm
    · simp only [denoteTerm, first, second]
  | @count signed n m a b first second =>
    cases signed <;> simp only [scalarNormal, Bool.false_eq_true, ↓reduceIte]
    · split
      · rename_i bound
        simp only [denoteTerm, second]
        exact (EvoIR.Machine.Selection.count_unsigned_as_comparison
          (denoteTerm σ a) (denoteTerm σ b) bound).symm
      · simp only [denoteTerm, first, second]
    · split
      · rename_i bound
        simp only [denoteTerm, second]
        exact (EvoIR.Machine.Selection.count_signed_as_comparison
          (denoteTerm σ a) (denoteTerm σ b) bound.1 bound.2).symm
      · simp only [denoteTerm, first, second]
  | @division signed remainder n a b first second =>
    cases remainder <;> simp only [scalarNormal, Bool.false_eq_true, ↓reduceIte,
      denoteTerm, first, second]
    cases signed
    · exact (EvoIR.Machine.Selection.unsigned_remainder_identity _ _).symm
    · exact (EvoIR.Machine.Selection.signed_remainder_identity _ _).symm

def canonical {Γ : Var → Nat} {n : Nat} (term : Term Γ n) : Term Γ n :=
  (scalarNormal term).normalizeValue

theorem canonical_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat} (term : Term Γ n) :
    denoteTerm σ (canonical term) = denoteTerm σ term := by
  rw [canonical, Term.normalize_value_sound, scalar_normal_sound]

/-- The bitwise operators. Each result bit reads only the same bit of the
two operands. -/
def bitwiseOp : BinOp → Bool
  | .and | .or | .xor => true
  | .add | .sub | .mul => false

/-- Zero extension and truncation both denote `setWidth`. -/
theorem cast_set_width (op : CastOp) (notSigned : op ≠ .sext) {w : Nat} (n : Nat)
    (value : BitVec w) : denoteOp (.cast op w n) value = value.setWidth n := by
  cases op <;> simp_all [denoteOp, BitVec.truncate_eq_setWidth]

/-- A width change commutes with a bitwise operator. The law holds for every
pair of widths, so it needs no width fact. -/
theorem set_width_bitwise (op : BinOp) (bitwise : bitwiseOp op = true)
    {k : Nat} (n : Nat) (a b : BitVec k) :
    denoteOp (.bin op n) (a.setWidth n, b.setWidth n) =
      (denoteOp (.bin op k) (a, b)).setWidth n := by
  cases op <;> simp only [bitwiseOp, Bool.false_eq_true] at bitwise <;>
    simp only [denoteOp, Op.res, BitVec.setWidth_and, BitVec.setWidth_or, BitVec.setWidth_xor]

/-- A bitwise operator over two unsigned width changes from one width becomes
one unsigned width change of the operator at that width. For example, the
32-bit OR of two zero-extended one-bit comparisons becomes the zero-extended
one-bit OR. Other shapes stay unchanged. -/
def liftBitwise {Γ : Var → Nat} {n : Nat} (op : BinOp) (left right : Term Γ n) : Term Γ n :=
  match left, right with
  | .cast (w := k) first _ a, .cast (w := m) second _ b =>
    if same : bitwiseOp op = true ∧ first ≠ .sext ∧ second ≠ .sext ∧ m = k then
      simplifyCast .zext n (.bin op a (same.2.2.2 ▸ b))
    else .bin op left right
  | _, _ => .bin op left right

theorem lift_bitwise_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat} (op : BinOp)
    (left right : Term Γ n) :
    denoteTerm σ (liftBitwise op left right) = denoteTerm σ (.bin op left right) := by
  unfold liftBitwise
  split
  · rename_i first k a second m b
    split
    · rename_i same
      obtain ⟨bitwise, firstUnsigned, secondUnsigned, rfl⟩ := same
      rw [simplify_cast_sound]
      simp only [denoteTerm, cast_set_width first firstUnsigned,
        cast_set_width second secondUnsigned, set_width_bitwise op bitwise,
        cast_set_width CastOp.zext (by decide), Op.res]
    · rfl
  · rfl

/-- Apply `liftBitwise` at every operator node and compose each width change
again. -/
def liftCasts {Γ : Var → Nat} : {n : Nat} → Term Γ n → Term Γ n
  | _, .var v n h => .var v n h
  | _, .const n c => .const n c
  | _, .bin op a b => liftBitwise op (liftCasts a) (liftCasts b)
  | _, .cmp op a b => .cmp op (liftCasts a) (liftCasts b)
  | _, .shift op a b => .shift op (liftCasts a) (liftCasts b)
  | _, .cast op n a => simplifyCast op n (liftCasts a)
  | _, .guard op a b => .guard op (liftCasts a) (liftCasts b)
  | _, .nonzero a => .nonzero (liftCasts a)
  | _, .count s a b => .count s (liftCasts a) (liftCasts b)
  | _, .division s r a b => .division s r (liftCasts a) (liftCasts b)

theorem lift_casts_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat} (term : Term Γ n) :
    denoteTerm σ (liftCasts term) = denoteTerm σ term := by
  induction term <;> simp_all only [liftCasts, lift_bitwise_sound, simplify_cast_sound, denoteTerm]

/-- Equal canonical terms are equal. If the canonical terms differ, a second
proved pass lifts width changes out of bitwise operators and compares again.
The second pass runs only after the first comparison fails, so every pair
that the first comparison accepts stays accepted. -/
def checkTerm {Γ : Var → Nat} {n : Nat} (source target : Term Γ n) :
    Option (PLift (∀ σ, denoteTerm σ source = denoteTerm σ target)) :=
  if same : canonical source = canonical target then some ⟨by
    intro σ
    exact (canonical_sound σ source).symm.trans
      ((congrArg (denoteTerm σ) same).trans (canonical_sound σ target))⟩
  else if lifted : liftCasts (canonical source) = liftCasts (canonical target) then some ⟨by
    intro σ
    have equal := congrArg (denoteTerm σ) lifted
    rw [lift_casts_sound, lift_casts_sound, canonical_sound, canonical_sound] at equal
    exact equal⟩
  else none

def checkWord {Γ : Var → Nat} (source : Word Γ) (target : WordExpr Γ) :
    Option (PLift (∀ σ, target.denote σ = (denoteTerm σ source.2).setWidth 64)) := do
  let quoted ← wordDiagram target
  let matched ← checkTerm quoted.val (encode source.2)
  return ⟨by
    intro σ
    exact (quoted.property σ).symm.trans ((matched.down σ).trans (encode_sound σ source.2))⟩

def checkFlag {Γ : Var → Nat} (source : Term Γ 1) (flags : FlagExpr Γ)
    (cond : EvoIR.Machine.Cond) : Option (PLift (∀ σ,
      EvoIR.Machine.condition (flags.denote σ) cond = .ok ((denoteTerm σ source).getLsbD 0))) := do
  let quoted ← flagDiagram flags cond
  let matched ← checkTerm source quoted.val
  return ⟨by
    intro σ
    rw [flag_truth_sound, ← quoted.property σ, ← matched.down σ]⟩

end EvoIR.Core.Symbolic.MachineSelection
