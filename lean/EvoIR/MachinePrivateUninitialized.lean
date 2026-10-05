import EvoIR.MachinePrivateEffects

/-!
Uninitialized private bytes remain diagnostic 3001 in both actual evaluators.
This is a refusal control, not an extension of the admitted Core observation
set. In particular, the checker may not turn a missing store into a zero.
-/
namespace EvoIR.Machine.PrivateMemory

open EvoIR.Core

theorem load_uninitialized_bytes (memory : IR.Memory) (object : IR.Object)
    (width align : Nat) (admitted : width = 32 ∨ width = 64)
    (access : EvoIR.Machine.accessObject memory (.pointer object.id 0) (width/8) align =
      .ok (object, 0))
    (bytes : object.bytes = List.replicate (width/8) none) :
    loadPrivate memory (.pointer object.id 0) width align = .error (.diagnostic 3001) := by
  rcases admitted with rfl | rfl
  all_goals
    simp only [loadPrivate, access, Bind.bind, Except.bind, bytes]
    rfl

theorem load_uninitialized_transfer {source target : IR.Memory}
    (related : LiveRelated source target) (owner : Nat) (object : IR.Object)
    (width align : Nat) (dst base : VReg) (kind : LoadKind) (get : OperandReader)
    (admitted : width = 32 ∨ width = 64)
    (matching : kind.bits = width)
    (alignment : (if align == 0 then kind.bits/8 else align) = width/8)
    (access : IR.accessObject source (.pointer object.id 0) (width/8) (width/8) =
      .ok (object, 0))
    (bytes : object.bytes = List.replicate (width/8) none)
    (address : get base = .ok (.value (.pointer object.id 0))) :
    IR.evalOperation owner (.load width) [.memory source, .world, .pointer object.id 0] =
        .error (.diagnostic 3001) ∧
    evalInstructionOperands owner (.load align dst base 0 kind false) get target =
        .error (.diagnostic 3001) := by
  obtain ⟨other, nativeAccess, objects⟩ := related.accessObject object.id 0
    (width/8) (width/8) object 0 access
  have otherAccess : EvoIR.Machine.accessObject target (.pointer other.id 0)
      (width/8) (width/8) = .ok (other, 0) := by
    simpa only [objects.id] using nativeAccess
  have loaded : loadPrivate target (.pointer object.id 0) width (width/8) =
      .error (.diagnostic 3001) := by
    simpa only [objects.id] using load_uninitialized_bytes target other width (width/8)
      admitted otherAccess (objects.bytes.symm.trans bytes)
  have actualAlignment : (if align = 0 then width/8 else align) = width/8 := by
    simpa only [matching, beq_iff_eq] using alignment
  refine ⟨IR.load_uninitialized_bytes owner source object width admitted access bytes, ?_⟩
  simp [evalInstructionOperands, operandValue, address, offsetPointer, asPointer,
    actualAlignment, matching, loaded, Bind.bind, Except.bind, Pure.pure, Except.pure]

end EvoIR.Machine.PrivateMemory
