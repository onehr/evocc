import EvoIR.MachineSelectionBank
import EvoIR.MachineMultiply

/-!
Proof-valued quotation of exact machine expressions into the shared scalar
term language. Register words are always 64 bits. Native-width arithmetic
uses explicit low views, and shifts retain the actual five/six-bit mask.
Quotation is deliberately partial: refusing a form grants no semantic fact.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def view {Γ : Var → Nat} (n : Nat) (term : Term Γ 64) : Term Γ n :=
  simplifyCast .trunc n term

def encode {Γ : Var → Nat} {n : Nat} (term : Term Γ n) : Term Γ 64 :=
  simplifyCast .zext 64 term

@[simp] theorem view_sound {Γ : Var → Nat} (σ : Subst Γ) (n : Nat) (term : Term Γ 64) :
    denoteTerm σ (view n term) = (denoteTerm σ term).setWidth n := by
  rw [view, simplify_cast_sound]
  rfl

@[simp] theorem encode_sound {Γ : Var → Nat} (σ : Subst Γ) {n : Nat} (term : Term Γ n) :
    denoteTerm σ (encode term) = (denoteTerm σ term).setWidth 64 := by
  rw [encode, simplify_cast_sound]
  rfl

abbrev WordDiagram {Γ : Var → Nat} (expr : WordExpr Γ) :=
  {term : Term Γ 64 // ∀ σ, denoteTerm σ term = expr.denote σ}

abbrev FlagDiagram {Γ : Var → Nat} (flags : FlagExpr Γ) (cond : EvoIR.Machine.Cond) :=
  {term : Term Γ 1 // ∀ σ,
    (denoteTerm σ term).getLsbD 0 = flagTruth (flags.denote σ) cond}

def machineCountBits : EvoIR.Machine.Width → Nat
  | .w => 5 | .x => 6

theorem machine_count_exact (width : EvoIR.Machine.Width) (raw : BitVec 64) :
    (raw.setWidth (machineCountBits width)).toNat =
      (raw.setWidth width.bits).toNat % width.bits := by
  cases width <;>
    simp [machineCountBits, EvoIR.Machine.Width.bits, BitVec.toNat_setWidth,
      Nat.mod_mod_of_dvd]

def binaryTerm {Γ : Var → Nat} (op : EvoIR.Machine.BinOp) (width : EvoIR.Machine.Width)
    (lhs rhs : Term Γ 64) : Term Γ 64 :=
  let x := view width.bits lhs
  let y := view width.bits rhs
  let k := view (machineCountBits width) rhs
  encode (match op with
    | .add => .bin .add x y | .sub => .bin .sub x y | .mul => .bin .mul x y
    | .and => .bin .and x y | .orr => .bin .or x y | .eor => .bin .xor x y
    | .udiv => .division false false x y | .sdiv => .division true false x y
    | .lsl => .shift .shl x k | .lsr => .shift .lshr x k | .asr => .shift .ashr x k)

theorem binary_term_sound {Γ : Var → Nat} (σ : Subst Γ)
    (op : EvoIR.Machine.BinOp) (width : EvoIR.Machine.Width) (lhs rhs : Term Γ 64) :
    denoteTerm σ (binaryTerm op width lhs rhs) =
      EvoIR.Machine.evalBinary op width (denoteTerm σ lhs) (denoteTerm σ rhs) := by
  cases op <;>
    simp only [binaryTerm, encode_sound, denoteTerm, view_sound, denoteOp, Op.res,
      EvoIR.Machine.evalBinary, machine_count_exact, divisionBits]
  · by_cases zero : (denoteTerm σ rhs).setWidth width.bits = 0
    · simp [zero]
    · have nonzero : ((denoteTerm σ rhs).setWidth width.bits).toNat ≠ 0 := by
        intro h
        exact zero (BitVec.eq_of_toNat_eq h)
      simp only [BitVec.toNat_setWidth] at nonzero
      simp [nonzero]
  · by_cases zero : ((denoteTerm σ rhs).setWidth width.bits).toNat = 0 <;>
      simp only [BitVec.toNat_setWidth] at zero <;> simp [zero]

def comparisonTerm {Γ : Var → Nat} (cond : EvoIR.Machine.Cond) (width : EvoIR.Machine.Width)
    (lhs rhs : Term Γ 64) : Option (Term Γ 1) :=
  let x := view width.bits lhs
  let y := view width.bits rhs
  match cond with
  | .eq => some (.cmp .eq x y) | .ne => some (.cmp .ne x y)
  | .lt => some (.cmp .slt x y) | .le => some (.cmp .sle x y)
  | .gt => some (.cmp .slt y x) | .ge => some (.cmp .sle y x)
  | .lo | .cc => some (.cmp .ult x y) | .ls => some (.cmp .ule x y)
  | .hi => some (.cmp .ult y x) | .hs | .cs => some (.cmp .ule y x)
  | .vs | .vc => none

theorem comparison_term_sound {Γ : Var → Nat} (σ : Subst Γ)
    (cond : EvoIR.Machine.Cond) (width : EvoIR.Machine.Width)
    (lhs rhs : Term Γ 64) (term : Term Γ 1)
    (found : comparisonTerm cond width lhs rhs = some term) :
    (denoteTerm σ term).getLsbD 0 =
      flagTruth (.integer (denoteTerm σ lhs) (denoteTerm σ rhs) width) cond := by
  cases cond <;>
    simp only [comparisonTerm, Option.some.injEq, reduceCtorEq] at found
  all_goals first | contradiction | subst term
  all_goals simp [denoteTerm, view_sound, flagTruth, EvoIR.Machine.condition,
    denoteOp, Op.res, Except.toOption, Option.getD, BitVec.getLsbD_ofBool,
    BitVec.slt_eq_decide, BitVec.sle_eq_decide, BitVec.ult_eq_decide, BitVec.ule_eq_decide]

def arithmeticTerm {Γ : Var → Nat} (cond : EvoIR.Machine.Cond)
    (op : EvoIR.Machine.FlagOp) (width : EvoIR.Machine.Width) (lhs rhs : Term Γ 64) :
    Option (Term Γ 1) :=
  let term := Term.guard (if op == .subs then .sub else .add)
    (view width.bits lhs) (view width.bits rhs)
  match cond with
  | .vc => some term
  | .vs => some (.bin .xor term (.const 1 1))
  | _ => none

theorem arithmetic_term_sound {Γ : Var → Nat} (σ : Subst Γ)
    (cond : EvoIR.Machine.Cond) (op : EvoIR.Machine.FlagOp) (width : EvoIR.Machine.Width)
    (lhs rhs : Term Γ 64) (term : Term Γ 1)
    (found : arithmeticTerm cond op width lhs rhs = some term) :
    (denoteTerm σ term).getLsbD 0 =
      flagTruth (EvoIR.Machine.arithmeticFlags op width
        (denoteTerm σ lhs) (denoteTerm σ rhs)).2 cond := by
  cases cond <;> simp only [arithmeticTerm, Option.some.injEq, reduceCtorEq] at found
  all_goals first | contradiction | subst term
  all_goals cases op <;>
    simp [denoteTerm, view_sound, flagTruth, EvoIR.Machine.arithmeticFlags,
      EvoIR.Machine.condition, denoteOp, Op.res, Except.toOption, Option.getD,
      signedRepresentable, signedMin, signedMax, BitVec.getLsbD_ofBool,
      BitVec.getLsbD_xor]

def normalizeTerm {Γ : Var → Nat} (regClass : EvoIR.Machine.RegClass) (term : Term Γ 64) :
    Term Γ 64 :=
  if regClass == .gpr32 then encode (view 32 term) else term

theorem normalize_term_sound {Γ : Var → Nat} (σ : Subst Γ)
    (regClass : EvoIR.Machine.RegClass) (term : Term Γ 64) :
    denoteTerm σ (normalizeTerm regClass term) =
      if regClass == .gpr32 then EvoIR.Machine.normalizeAt .w (denoteTerm σ term)
      else denoteTerm σ term := by
  cases regClass <;>
    simp [normalizeTerm, EvoIR.Machine.normalizeAt, EvoIR.Machine.Width.bits]

theorem one_bit_reify (value : BitVec 1) : value = BitVec.ofBool (value.getLsbD 0) := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  have zero : i = 0 := by omega
  subst i
  simp

def smullTerm {Γ : Var → Nat} (lhs rhs : Term Γ 64) : Term Γ 64 :=
  .bin .mul (simplifyCast .sext 64 (view 32 lhs)) (simplifyCast .sext 64 (view 32 rhs))

theorem smull_term_sound {Γ : Var → Nat} (σ : Subst Γ) (lhs rhs : Term Γ 64) :
    denoteTerm σ (smullTerm lhs rhs) =
      EvoIR.Machine.evalWideMultiply .smull (denoteTerm σ lhs) (denoteTerm σ rhs) := by
  simp only [smullTerm, denoteTerm_bin, simplify_cast_sound, denoteTerm_cast, view_sound,
    denoteOp, Op.res, EvoIR.Machine.evalWideMultiply, BitVec.signExtend, BitVec.ofInt_mul]

def bfxTerm {Γ : Var → Nat} (signed : Bool) (width : EvoIR.Machine.Width)
    (input : Term Γ 64) (lsb bits : Nat) : Term Γ 64 :=
  let shifted := Term.shift .lshr (encode (view width.bits input))
    (.const 64 (BitVec.ofNat 64 lsb))
  let extended := simplifyCast (if signed then .sext else .zext) 64 (view bits shifted)
  encode (view width.bits extended)

theorem bfx_term_sound {Γ : Var → Nat} (σ : Subst Γ) (signed : Bool)
    (width : EvoIR.Machine.Width) (input : Term Γ 64) (lsb bits : Nat)
    (fits : lsb < 2^64) :
    denoteTerm σ (bfxTerm signed width input lsb bits) =
      EvoIR.Machine.evalBfx signed width (denoteTerm σ input) lsb bits := by
  cases signed <;>
    simp [bfxTerm, encode_sound, view_sound, simplify_cast_sound,
      denoteTerm, denoteOp, Op.res, EvoIR.Machine.evalBfx, EvoIR.Machine.normalizeAt,
      BitVec.toNat_ofNat, Nat.mod_eq_of_lt fits]

def selectTerm {Γ : Var → Nat} (condition : Term Γ 1) (width : EvoIR.Machine.Width)
    (yes no : Term Γ 64) : Term Γ 64 :=
  let x := view width.bits yes
  let y := view width.bits no
  let mask := Term.bin .sub (.const width.bits 0) (simplifyCast .zext width.bits condition)
  encode (.bin .xor y (.bin .and (.bin .xor x y) mask))

theorem select_term_sound {Γ : Var → Nat} (σ : Subst Γ) (condition : Term Γ 1)
    (width : EvoIR.Machine.Width) (yes no : Term Γ 64) :
    denoteTerm σ (selectTerm condition width yes no) =
      EvoIR.Machine.normalizeAt width
        (if (denoteTerm σ condition).getLsbD 0 then denoteTerm σ yes else denoteTerm σ no) := by
  simp only [selectTerm, encode_sound, denoteTerm_bin, view_sound, simplify_cast_sound,
    denoteTerm_cast, denoteTerm_const, denoteOp, Op.res,
    EvoIR.Machine.Selection.selection_mask_identity]
  rw [one_bit_reify (denoteTerm σ condition)]
  cases value : (denoteTerm σ condition).getLsbD 0 <;>
    simp [value, EvoIR.Machine.normalizeAt]

def logicalTerm {Γ : Var → Nat} (cond : EvoIR.Machine.Cond) (width : EvoIR.Machine.Width)
    (value : Term Γ 64) : Term Γ 1 :=
  let raw := view width.bits value
  let zero := Term.const width.bits 0
  match cond with
  | .eq => .cmp .eq raw zero | .ne => .cmp .ne raw zero
  | .lt => .cmp .slt raw zero | .ge => .cmp .sle zero raw
  | .le => .cmp .sle raw zero | .gt => .cmp .slt zero raw
  | .lo | .cc | .ls | .vc => .const 1 1
  | .hi | .hs | .cs | .vs => .const 1 0

theorem logical_term_sound {Γ : Var → Nat} (σ : Subst Γ) (cond : EvoIR.Machine.Cond)
    (width : EvoIR.Machine.Width) (value : Term Γ 64) :
    (denoteTerm σ (logicalTerm cond width value)).getLsbD 0 =
      flagTruth (.arithmetic (decide (((denoteTerm σ value).setWidth width.bits).toInt < 0))
        ((denoteTerm σ value).setWidth width.bits == 0) false false) cond := by
  have zero : ((denoteTerm σ value).setWidth width.bits == 0#width.bits) =
      decide ((↑(denoteTerm σ value).toNat : Int).bmod (2^width.bits) = 0) := by
    apply Bool.eq_iff_iff.mpr
    simp only [beq_iff_eq, decide_eq_true_eq]
    rw [← BitVec.toInt_inj]
    simp
  have zeroProp : (denoteTerm σ value).setWidth width.bits = 0#width.bits ↔
      (↑(denoteTerm σ value).toNat : Int).bmod (2^width.bits) = 0 := by
    have equality := congrArg (fun b : Bool => b = true) zero
    simpa only [beq_iff_eq, decide_eq_true_eq] using Eq.to_iff equality
  cases cond <;>
    simp [logicalTerm, denoteTerm, view_sound, flagTruth, EvoIR.Machine.condition,
      denoteOp, Op.res, Except.toOption, Option.getD, BitVec.getLsbD_ofBool,
      BitVec.slt_eq_decide, BitVec.sle_eq_decide, bne, zero]
  all_goals apply Bool.eq_iff_iff.mpr
  all_goals simp only [decide_eq_true_eq, Bool.or_eq_true, Bool.and_eq_true, beq_iff_eq,
    Bool.not_eq_true', Bool.eq_false_iff, not_decide_eq_true, zero, zeroProp]
  all_goals try simp only [ne_eq, decide_eq_true_eq]
  all_goals omega

mutual
def wordDiagram {Γ : Var → Nat} : (expr : WordExpr Γ) → Option (WordDiagram expr)
  | .input word => some ⟨encode word.2, by intro σ; exact encode_sound σ word.2⟩
  | .literal bits => some ⟨.const 64 (BitVec.ofNat 64 bits), by intro σ; rfl⟩
  | .binary op width lhs rhs => do
    let left ← wordDiagram lhs
    let right ← wordDiagram rhs
    return ⟨binaryTerm op width left.val right.val, by
      intro σ
      rw [binary_term_sound, left.property σ, right.property σ]
      rfl⟩
  | .wide .smull lhs rhs => do
    let left ← wordDiagram lhs
    let right ← wordDiagram rhs
    return ⟨smullTerm left.val right.val, by
      intro σ
      rw [smull_term_sound, left.property σ, right.property σ]
      rfl⟩
  | .bfx signed width input lsb bits => do
    if fits : lsb < 2^64 then
      let body ← wordDiagram input
      return ⟨bfxTerm signed width body.val lsb bits, by
        intro σ
        rw [bfx_term_sound σ signed width body.val lsb bits fits, body.property σ]
        rfl⟩
    else none
  | .extend op input => do
    let body ← wordDiagram input
    let term := simplifyCast (if op.signed then .sext else .zext) 64 (view op.bits body.val)
    return ⟨term, by
      intro σ
      simp only [term, simplify_cast_sound, denoteTerm_cast, view_sound, body.property σ]
      cases op <;> rfl⟩
  | .cset flags cond => do
    let condition ← flagDiagram flags cond
    return ⟨encode condition.val, by
      intro σ
      rw [encode_sound]
      have boolean : denoteTerm σ condition.val = BitVec.ofBool (flagTruth (flags.denote σ) cond) := by
        apply BitVec.eq_of_getLsbD_eq
        intro i hi
        have zero : i = 0 := by omega
        subst i
        simpa using condition.property σ
      rw [boolean]
      exact (EvoIR.Machine.Selection.bool_encode _).symm⟩
  | .normalize regClass input => do
    let body ← wordDiagram input
    return ⟨normalizeTerm regClass body.val, by
      intro σ
      rw [normalize_term_sound, body.property σ]
      rfl⟩
  | .csel flags cond width yes no => do
    let condition ← flagDiagram flags cond
    let first ← wordDiagram yes
    let second ← wordDiagram no
    return ⟨selectTerm condition.val width first.val second.val, by
      intro σ
      rw [select_term_sound, condition.property σ, first.property σ, second.property σ]
      rfl⟩
  | .wide .smulh .. | .wide .umulh .. => none

def flagDiagram {Γ : Var → Nat} (flags : FlagExpr Γ) (cond : EvoIR.Machine.Cond) :
    Option (FlagDiagram flags cond) :=
  match flags with
  | .comparison width lhs rhs => do
    let left ← wordDiagram lhs
    let right ← wordDiagram rhs
    let term : {term // comparisonTerm cond width left.val right.val = some term} ←
      match found : comparisonTerm cond width left.val right.val with
      | none => none
      | some term => some ⟨term, found⟩
    return ⟨term.val, by
      intro σ
      rw [comparison_term_sound σ cond width left.val right.val term.val term.property,
        left.property σ, right.property σ]
      rfl⟩
  | .arithmetic op width lhs rhs => do
    let left ← wordDiagram lhs
    let right ← wordDiagram rhs
    let term : {term // arithmeticTerm cond op width left.val right.val = some term} ←
      match found : arithmeticTerm cond op width left.val right.val with
      | none => none
      | some term => some ⟨term, found⟩
    return ⟨term.val, by
      intro σ
      rw [arithmetic_term_sound σ cond op width left.val right.val term.val term.property,
        left.property σ, right.property σ]
      rfl⟩
  | .logical width value => do
    let body ← wordDiagram value
    return ⟨logicalTerm cond width body.val, by
      intro σ
      rw [logical_term_sound, body.property σ]
      rfl⟩
end

end EvoIR.Core.Symbolic.MachineSelection
