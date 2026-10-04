import EvoIR.Core.SymbolicSource

/-! Source operator bridges required by the control-flow diagrams. These
preserve source ranks and usual arithmetic conversions, rather than using a
target operand width as a substitute for C signedness. -/
namespace EvoIR.Core.Symbolic

inductive Bitwise where
  | and | or | xor
  deriving DecidableEq

def Bitwise.source : Bitwise → Source.Binary
  | .and => .bitAnd
  | .or => .bitOr
  | .xor => .bitXor

def Bitwise.target : Bitwise → BinOp
  | .and => .and
  | .or => .or
  | .xor => .xor

def bitwise {Γ : Var → Nat} (op : Bitwise) (a b : CWord Γ) : CWord Γ :=
  let type := Source.usualType a.type b.type
  ⟨type, .bin op.target (a.cast type).term (b.cast type).term⟩

theorem source_value_representation (v : Source.Value) :
    BitVec.ofInt v.type.width v.number = v.bits := by
  cases v with
  | mk type bits =>
    cases type <;> simp [Source.Value.number, Source.IntType.signed]

theorem bitwise_sound {Γ : Var → Nat} (σ : Subst Γ) (op : Bitwise) (a b : CWord Γ) :
    Source.binaryValue op.source (a.denote σ) (b.denote σ) = .ok ((bitwise op a b).denote σ) := by
  let type := Source.usualType a.type b.type
  have represented (value : CWord Γ) :
      BitVec.ofInt type.width ((value.denote σ).cast type).number =
        denoteTerm σ (value.cast type).term := by
    rw [← cast_sound]
    exact source_value_representation ((value.cast type).denote σ)
  cases op
  · change Except.ok (Source.Value.mk type
        ((BitVec.ofInt type.width ((a.denote σ).cast type).number) &&&
         (BitVec.ofInt type.width ((b.denote σ).cast type).number))) =
      Except.ok (Source.Value.mk type
        ((denoteTerm σ (a.cast type).term : BitVec type.width) &&&
          (denoteTerm σ (b.cast type).term : BitVec type.width)))
    rw [represented, represented]
    rfl
  · change Except.ok (Source.Value.mk type
        ((BitVec.ofInt type.width ((a.denote σ).cast type).number) |||
         (BitVec.ofInt type.width ((b.denote σ).cast type).number))) =
      Except.ok (Source.Value.mk type
        ((denoteTerm σ (a.cast type).term : BitVec type.width) |||
          (denoteTerm σ (b.cast type).term : BitVec type.width)))
    rw [represented, represented]
    rfl
  · change Except.ok (Source.Value.mk type
        ((BitVec.ofInt type.width ((a.denote σ).cast type).number) ^^^
         (BitVec.ofInt type.width ((b.denote σ).cast type).number))) =
      Except.ok (Source.Value.mk type
        ((denoteTerm σ (a.cast type).term : BitVec type.width) ^^^
          (denoteTerm σ (b.cast type).term : BitVec type.width)))
    rw [represented, represented]
    rfl

inductive Comparison where
  | eq | ne | lt | le | gt | ge
  deriving DecidableEq, Repr

def Comparison.source : Comparison → Source.Binary
  | .eq => .eq | .ne => .ne | .lt => .lt
  | .le => .le | .gt => .gt | .ge => .ge

def Comparison.math : Comparison → Int → Int → Bool
  | .eq, x, y => x == y
  | .ne, x, y => x != y
  | .lt, x, y => x < y
  | .le, x, y => x ≤ y
  | .gt, x, y => y < x
  | .ge, x, y => y ≤ x

def Comparison.predicate {Γ : Var → Nat} {n : Nat} (signed : Bool)
    (op : Comparison) (x y : Term Γ n) : Term Γ 1 :=
  match op with
  | .eq => .cmp .eq x y
  | .ne => .cmp .ne x y
  | .lt => .cmp (if signed then .slt else .ult) x y
  | .le => .cmp (if signed then .sle else .ule) x y
  | .gt => .cmp (if signed then .slt else .ult) y x
  | .ge => .cmp (if signed then .sle else .ule) y x

def comparisonTest {Γ : Var → Nat} (op : Comparison) (a b : CWord Γ) : Term Γ 1 :=
  let type := Source.usualType a.type b.type
  op.predicate type.signed (a.cast type).term (b.cast type).term

def comparison {Γ : Var → Nat} (op : Comparison) (a b : CWord Γ) : CWord Γ :=
  ⟨.int, .cast .zext 32 (comparisonTest op a b)⟩

theorem comparison_predicate_sound {Γ : Var → Nat} (σ : Subst Γ)
    (op : Comparison) (type : Source.IntType) (a b : Term Γ type.width) :
    denoteTerm σ (op.predicate type.signed a b) = BitVec.ofBool
      (op.math (Source.Value.mk type (denoteTerm σ a)).number
        (Source.Value.mk type (denoteTerm σ b)).number) := by
  cases op <;> cases signed : type.signed <;>
    simp [Comparison.predicate, Comparison.math, denoteOp, Source.Value.number, signed,
      BitVec.slt_eq_decide, BitVec.sle_eq_decide, BitVec.ult_eq_decide,
      BitVec.ule_eq_decide]
  all_goals apply congrArg BitVec.ofBool
  all_goals apply Bool.eq_iff_iff.mpr
  all_goals simp [bne, BitVec.toInt_inj, Int.natCast_inj, BitVec.toNat_inj]

theorem comparison_test_sound {Γ : Var → Nat} (σ : Subst Γ)
    (op : Comparison) (a b : CWord Γ) :
    Source.binaryValue op.source (a.denote σ) (b.denote σ) = .ok (.boolean
      ((denoteTerm σ (comparisonTest op a b)).toNat != 0)) := by
  have normal (x y : Source.Value) : Source.binaryValue op.source x y =
      .ok (.boolean (op.math (x.cast (Source.usualType x.type y.type)).number
        (y.cast (Source.usualType x.type y.type)).number)) := by cases op <;> rfl
  rw [normal]
  let type := Source.usualType a.type b.type
  change Except.ok (Source.Value.boolean
    (op.math ((a.denote σ).cast type).number ((b.denote σ).cast type).number)) = _
  rw [← cast_sound, ← cast_sound]
  have same := comparison_predicate_sound σ op type
    (a.cast type).term (b.cast type).term
  change denoteTerm σ (comparisonTest op a b) = BitVec.ofBool
    (op.math ((a.cast type).denote σ).number ((b.cast type).denote σ).number) at same
  rw [same]
  generalize op.math ((a.cast type).denote σ).number
    ((b.cast type).denote σ).number = result
  cases result <;> rfl

theorem comparison_sound {Γ : Var → Nat} (σ : Subst Γ)
    (op : Comparison) (a b : CWord Γ) :
    Source.binaryValue op.source (a.denote σ) (b.denote σ) =
      .ok ((comparison op a b).denote σ) := by
  rw [comparison_test_sound]
  have boolean (v : BitVec 1) :
      Source.Value.boolean (v.toNat != 0) = Source.Value.mk .int (v.setWidth 32) := by
    have bound := v.isLt
    have alternatives : v = 0#1 ∨ v = 1#1 := by
      have : v.toNat = 0 ∨ v.toNat = 1 := by omega
      rcases this with h | h
      · left; exact BitVec.eq_of_toNat_eq h
      · right; exact BitVec.eq_of_toNat_eq h
    rcases alternatives with rfl | rfl <;> rfl
  exact congrArg Except.ok (boolean _)

/-- A C comparison is an int-valued expression, but branch lowering consumes
its one-bit predicate directly. Strip only a proved boolean widening; all
other integer expressions use an explicit nonzero test. -/
def CWord.truthTerm {Γ : Var → Nat} (value : CWord Γ) : Term Γ 1 :=
  match value.term with
  | .cast (w := 1) .zext _ condition => condition
  | _ => .cmp .ne value.term (.const value.type.width 0)

theorem truth_term_sound {Γ : Var → Nat} (σ : Subst Γ) (value : CWord Γ) :
    ((denoteTerm σ value.truthTerm).toNat != 0) = (value.denote σ).truth := by
  unfold CWord.truthTerm
  split
  · rename_i condition shape
    have positive : 1 ≤ value.type.width := by cases value.type <;> decide
    simp only [CWord.denote, Source.Value.truth, shape, denoteTerm, denoteOp,
      BitVec.zeroExtend, BitVec.toNat_setWidth_of_le positive]
  · simp only [denoteTerm, denoteOp, BitVec.toNat_ofBool, CWord.denote, Source.Value.truth]
    apply Bool.eq_iff_iff.mpr
    simp [bne, BitVec.toNat_eq]

def unary {Γ : Var → Nat} (op : Source.Unary) (value : CWord Γ) :
    List (Guard Γ) × CWord Γ :=
  match op with
  | .plus => ([], value)
  | .neg => arithmetic .sub ⟨value.type, .const value.type.width 0⟩ value
  | .bitNot => ([], ⟨value.type,
      .bin .xor value.term (.const value.type.width (BitVec.allOnes value.type.width))⟩)
  | .not => ([], ⟨.int, .cast .zext 32 (.cmp .eq value.term (.const value.type.width 0))⟩)

theorem unary_sound {Γ : Var → Nat} (σ : Subst Γ) (op : Source.Unary) (value : CWord Γ) :
    (do guards σ (unary op value).1; return (unary op value).2.denote σ) =
      Source.unaryValue op (value.denote σ) := by
  cases op with
  | plus => rfl
  | neg =>
    change (do guards σ (arithmetic .sub ⟨value.type, .const value.type.width 0⟩ value).1
               return (arithmetic .sub ⟨value.type, .const value.type.width 0⟩ value).2.denote σ) = _
    rw [arithmetic_sound]
    have zero : (Source.Value.mk value.type (0 : BitVec value.type.width)).number = 0 := by
      cases value.type <;> rfl
    have self (x : BitVec value.type.width) :
        Source.Value.cast value.type ⟨value.type, x⟩ = ⟨value.type, x⟩ :=
      Source.value_cast_self ⟨value.type, x⟩
    simp only [Source.arithmeticSourceOp, Source.binaryValue, CWord.denote, denoteTerm,
      Source.usualType_self, self, zero, Int.zero_sub, Source.unaryValue]
  | bitNot =>
    simp [unary, guards, CWord.denote, Source.unaryValue, denoteOp,
      bind, Except.bind, pure, Except.pure]
    rfl
  | not =>
    have zero (x : BitVec value.type.width) : (x == 0) = (x.toNat == 0) := by
      apply Bool.eq_iff_iff.mpr
      simp [BitVec.toNat_eq]
    simp only [unary, guards, CWord.denote, denoteTerm, denoteOp, zero,
      Source.unaryValue, Source.Value.truth, Source.Value.boolean, Source.Value.ofNat,
      bind, Except.bind, pure, Except.pure, bne, Bool.not_not]
    cases (denoteTerm σ value.term).toNat == 0 <;> rfl

end EvoIR.Core.Symbolic
