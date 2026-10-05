import EvoIR.Core.IRExecution

/-! Exact byte transport for the private scalar objects admitted by the core.
These lemmas use the real load/store evaluator. They do not replace object
identity, access, lifetime, or initialization checks with numeric assumptions.
-/
namespace EvoIR.Core.IR

/-- Little-endian initialized bytes, exactly as emitted by `evalOperation.store`. -/
def scalarBytes (width : Nat) (value : BitVec width) : List (Option (BitVec 8)) :=
  (List.range (width / 8)).map fun i =>
    some (BitVec.ofNat 8 (value.toNat / 2^(8*i)))

@[simp] theorem scalarBytes_length (width : Nat) (value : BitVec width) :
    (scalarBytes width value).length = width / 8 := by
  simp [scalarBytes]

/-- Reassembly of any finite prefix of base-256 digits. The statement is
parametric in both the integer and byte count, not a sampled bit-vector test. -/
private def byteSum (value : Nat) : Nat → Nat
  | 0 => 0
  | count + 1 => byteSum value count +
      (value / 2^(8*count) % 256) * 2^(8*count)

private theorem byteSum_eq (value count : Nat) :
    byteSum value count = value % 2^(8*count) := by
  induction count with
  | zero => exact (Nat.mod_one value).symm
  | succ count ih =>
    rw [byteSum, ih, Nat.mul_succ, Nat.pow_add]
    change value % 2^(8*count) + (value / 2^(8*count) % 256) * 2^(8*count) =
      value % (2^(8*count) * 256)
    simpa only [Nat.mul_comm] using
      (Nat.mod_mul (x := value) (a := 2^(8*count)) (b := 256)).symm

/-- The same private bytes can be read into a wider machine word without
changing the stored representation. This is a byte-decoding theorem; it does
not assert that an address is live, aligned, or in bounds. -/
theorem scalar_bytes_read_word (width : Nat) (value : BitVec width)
    (admitted : width = 32 ∨ width = 64) :
    (do
      let mut bits := 0
      for i in List.range (width/8) do
        let some (some byte) := (scalarBytes width value)[i]? | throw (.diagnostic 3001)
        bits := bits + byte.toNat * 2^(8*i)
      return BitVec.ofNat 64 bits : Except Source.Fault (BitVec 64)) =
      .ok (value.setWidth 64) := by
  rcases admitted with rfl | rfl
  · change Except.ok (BitVec.ofNat 64 (byteSum value.toNat 4)) = .ok (value.setWidth 64)
    rw [byteSum_eq, Nat.mod_eq_of_lt value.isLt]
    congr 1
    apply BitVec.eq_of_toNat_eq
    simp [BitVec.toNat_setWidth]
  · change Except.ok (BitVec.ofNat 64 (byteSum value.toNat 8)) = .ok (value.setWidth 64)
    rw [byteSum_eq, Nat.mod_eq_of_lt value.isLt]
    simp

theorem store_scalar_bytes (owner : Nat) (m : Memory) (o : Object)
    (width : Nat) (value : BitVec width)
    (admitted : width = 32 ∨ width = 64)
    (access : accessObject m (.pointer o.id 0) (width/8) (width/8) = .ok (o, 0))
    (length : o.bytes.length = width/8) :
    evalOperation owner .store [.memory m, .world, .pointer o.id 0, .bits width value] =
      .ok [.memory (m.replace {o with bytes := scalarBytes width value}), .world] := by
  have supported : (width != 32 && width != 64) = false := by
    rcases admitted with rfl | rfl <;> decide
  simp only [evalOperation, memory, world, beq_self_eq_true, ↓reduceIte,
    Bind.bind, Except.bind, supported, Bool.false_eq_true, access, pure, Except.pure]
  simp [scalarBytes, ← length]

theorem load_scalar_bytes (owner : Nat) (m : Memory) (o : Object)
    (width : Nat) (value : BitVec width)
    (admitted : width = 32 ∨ width = 64)
    (access : accessObject m (.pointer o.id 0) (width/8) (width/8) = .ok (o, 0))
    (bytes : o.bytes = scalarBytes width value) :
    evalOperation owner (.load width) [.memory m, .world, .pointer o.id 0] =
      .ok [.bits width value, .memory m, .world] := by
  rcases admitted with rfl | rfl
  all_goals
    simp only [evalOperation, memory, world, beq_self_eq_true, ↓reduceIte,
      Bind.bind, Except.bind, access, pure, Except.pure, bytes]
  · change Except.ok [Value.bits 32 (BitVec.ofNat 32 (byteSum value.toNat 4)),
        .memory m, .world] = .ok [.bits 32 value, .memory m, .world]
    rw [byteSum_eq]
    rw [Nat.mod_eq_of_lt value.isLt]
    simp
  · change Except.ok [Value.bits 64 (BitVec.ofNat 64 (byteSum value.toNat 8)),
        .memory m, .world] = .ok [.bits 64 value, .memory m, .world]
    rw [byteSum_eq]
    rw [Nat.mod_eq_of_lt value.isLt]
    simp

theorem load_uninitialized_bytes (owner : Nat) (m : Memory) (o : Object)
    (width : Nat) (admitted : width = 32 ∨ width = 64)
    (access : accessObject m (.pointer o.id 0) (width/8) (width/8) = .ok (o, 0))
    (bytes : o.bytes = List.replicate (width/8) none) :
    evalOperation owner (.load width) [.memory m, .world, .pointer o.id 0] =
      .error (.diagnostic 3001) := by
  rcases admitted with rfl | rfl
  all_goals
    simp only [evalOperation, memory, world, beq_self_eq_true, ↓reduceIte,
      Bind.bind, Except.bind, access, pure, Except.pure, bytes]
    rfl

end EvoIR.Core.IR
