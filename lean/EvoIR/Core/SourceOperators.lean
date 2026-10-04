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

end EvoIR.Core.Symbolic
