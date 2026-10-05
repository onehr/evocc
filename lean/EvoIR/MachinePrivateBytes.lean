import EvoIR.MachinePrivateLive

/-! Exact private byte transport into actual virtual-machine loads and stores.
All access premises are real evaluator equations, not producer annotations. -/
namespace EvoIR.Machine.PrivateMemory

open EvoIR.Core

theorem load_scalar_bytes (memory : IR.Memory) (object : IR.Object)
    (width align : Nat) (value : BitVec width)
    (admitted : width = 32 ∨ width = 64)
    (access : EvoIR.Machine.accessObject memory (.pointer object.id 0) (width/8) align = .ok (object, 0))
    (bytes : object.bytes = IR.scalarBytes width value) :
    loadPrivate memory (.pointer object.id 0) width align = .ok (value.setWidth 64) := by
  simp only [loadPrivate, access, Bind.bind, Except.bind, bytes, Nat.zero_add]
  exact IR.scalar_bytes_read_word width value admitted

theorem store_scalar_bytes (memory : IR.Memory) (object : IR.Object)
    (width align : Nat) (value : BitVec width)
    (admitted : width = 32 ∨ width = 64)
    (access : EvoIR.Machine.accessObject memory (.pointer object.id 0) (width/8) align = .ok (object, 0))
    (length : object.bytes.length = width/8) :
    storePrivate memory (.pointer object.id 0) width align (value.setWidth 64) =
      .ok (memory.replace {object with bytes := IR.scalarBytes width value}) := by
  have bits : (value.setWidth 64).toNat = value.toNat := by
    rcases admitted with rfl | rfl
    · simp [BitVec.toNat_setWidth, Nat.mod_eq_of_lt
        (Nat.lt_trans value.isLt (by decide : 2^32 < 2^64))]
    · simp
  simp only [storePrivate, access, Bind.bind, Except.bind, bits]
  simp [← length, IR.scalarBytes, Pure.pure, Except.pure]

end EvoIR.Machine.PrivateMemory
