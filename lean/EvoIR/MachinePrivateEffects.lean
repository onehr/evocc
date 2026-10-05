import EvoIR.MachinePrivateBytes
import EvoIR.Core.PrivateBytes

/-! Actual private effect transfer. Source and target memories are distinct:
an erased static lifetime retirement changes only the source snapshot. Every
read, width, alignment, lifetime, and initialized-byte premise below is an
equation or property of the concrete evaluators, not a lowering annotation. -/
namespace EvoIR.Machine.PrivateMemory

open EvoIR.Core

theorem frameSlot_eval (owner origin size align : Nat) (dst : VReg)
    (get : OperandReader) (memory : Memory) (safe : memory.WellAllocated)
    (sizePositive : size ≠ 0) (alignPositive : align ≠ 0) :
    evalInstructionOperands owner (.frameSlot dst origin size align) get memory =
      .ok (valueWrite dst (.pointer memory.nextId 0),
        IR.allocatedMemory memory owner origin size align) := by
  have nonzero : memory.nextId ≠ 0 := Nat.ne_of_gt safe.1
  simp [evalInstructionOperands, sizePositive, alignPositive, nonzero,
    memory.well_allocated_fresh safe, IR.allocatedMemory,
    Pure.pure, Except.pure]

theorem allocate_transfer {source target : Memory} (related : LiveRelated source target)
    (owner origin size align : Nat) (dst : VReg) (get : OperandReader)
    (sizePositive : size ≠ 0) (alignPositive : align ≠ 0) :
    IR.evalOperation owner (.allocate origin size align) [.memory source, .world] =
        .ok [.pointer source.nextId 0,
          .memory (IR.allocatedMemory source owner origin size align), .world] ∧
    evalInstructionOperands owner (.frameSlot dst origin size align) get target =
        .ok (valueWrite dst (.pointer source.nextId 0),
          IR.allocatedMemory target owner origin size align) ∧
    LiveRelated (IR.allocatedMemory source owner origin size align)
      (IR.allocatedMemory target owner origin size align) := by
  refine ⟨IR.allocate_memory source owner origin size align related.sourceAllocated
    sizePositive alignPositive, ?_, related.allocate owner origin size align⟩
  simpa only [related.nextId] using frameSlot_eval owner origin size align dst get target
    related.targetAllocated sizePositive alignPositive

theorem load_eval (owner : Nat) (memory : Memory) (align : Nat) (dst base : VReg)
    (kind : LoadKind) (get : OperandReader) (id width : Nat) (bits : BitVec 64)
    (address : get base = .ok (.value (.pointer id 0)))
    (matching : kind.bits = width)
    (alignment : (if align == 0 then kind.bits/8 else align) = width/8)
    (loaded : loadPrivate memory (.pointer id 0) width (width/8) = .ok bits) :
    evalInstructionOperands owner (.load align dst base 0 kind false) get memory =
      .ok (valueWrite dst (.int bits), memory) := by
  have actualAlignment : (if align = 0 then width/8 else align) = width/8 := by
    simpa only [matching, beq_iff_eq] using alignment
  simp [evalInstructionOperands, operandValue, address, offsetPointer, asPointer,
    actualAlignment, matching, loaded, Bind.bind, Except.bind, Pure.pure, Except.pure]

theorem load_transfer {source target : Memory} (related : LiveRelated source target)
    (owner : Nat) (object : IR.Object) (width align : Nat) (value : BitVec width)
    (dst base : VReg) (kind : LoadKind) (get : OperandReader)
    (admitted : width = 32 ∨ width = 64)
    (matching : kind.bits = width)
    (alignment : (if align == 0 then kind.bits/8 else align) = width/8)
    (access : IR.accessObject source (.pointer object.id 0) (width/8) (width/8) = .ok (object, 0))
    (bytes : object.bytes = IR.scalarBytes width value)
    (address : get base = .ok (.value (.pointer object.id 0))) :
    IR.evalOperation owner (.load width) [.memory source, .world, .pointer object.id 0] =
        .ok [.bits width value, .memory source, .world] ∧
    evalInstructionOperands owner (.load align dst base 0 kind false) get target =
        .ok (valueWrite dst (.int (value.setWidth 64)), target) ∧
    LiveRelated source target := by
  obtain ⟨other, nativeAccess, objects⟩ := related.accessObject object.id 0 (width/8) (width/8)
    object 0 access
  have otherAccess : EvoIR.Machine.accessObject target (.pointer other.id 0) (width/8) (width/8) =
      .ok (other, 0) := by simpa only [objects.id] using nativeAccess
  have otherBytes : other.bytes = IR.scalarBytes width value := objects.bytes.symm.trans bytes
  have loaded : loadPrivate target (.pointer object.id 0) width (width/8) = .ok (value.setWidth 64) := by
    simpa only [objects.id] using load_scalar_bytes target other width (width/8) value admitted
      otherAccess otherBytes
  exact ⟨IR.load_scalar_bytes owner source object width value admitted access bytes,
    load_eval owner target align dst base kind get object.id width (value.setWidth 64)
      address matching alignment loaded, related⟩

theorem store_eval (owner : Nat) (memory after : Memory) (align : Nat) (src base : VReg)
    (kind : StoreKind) (get : OperandReader) (id width : Nat) (bits : BitVec 64)
    (address : get base = .ok (.value (.pointer id 0)))
    (value : get src = .ok (.value (.int bits)))
    (matching : kind.bits = width)
    (alignment : (if align == 0 then kind.bits/8 else align) = width/8)
    (stored : storePrivate memory (.pointer id 0) width (width/8) bits = .ok after) :
    evalInstructionOperands owner (.store align src base 0 kind false) get memory =
      .ok ([], after) := by
  have actualAlignment : (if align = 0 then width/8 else align) = width/8 := by
    simpa only [matching, beq_iff_eq] using alignment
  simp [evalInstructionOperands, operandValue, operandBits, address, value, offsetPointer,
    asPointer, actualAlignment, matching, stored, Bind.bind, Except.bind, Pure.pure, Except.pure]

theorem store_transfer {source target : Memory} (related : LiveRelated source target)
    (owner : Nat) (object : IR.Object) (width align : Nat) (value : BitVec width)
    (src base : VReg) (kind : StoreKind) (get : OperandReader)
    (admitted : width = 32 ∨ width = 64)
    (matching : kind.bits = width)
    (alignment : (if align == 0 then kind.bits/8 else align) = width/8)
    (alive : IR.liveObject source object.id = .ok object)
    (access : IR.accessObject source (.pointer object.id 0) (width/8) (width/8) = .ok (object, 0))
    (length : object.bytes.length = width/8)
    (address : get base = .ok (.value (.pointer object.id 0)))
    (readValue : get src = .ok (.value (.int (value.setWidth 64)))) :
    ∃ other,
      ObjectRelated object other ∧
      IR.evalOperation owner .store [.memory source, .world, .pointer object.id 0, .bits width value] =
        .ok [.memory (source.replace {object with bytes := IR.scalarBytes width value}), .world] ∧
      evalInstructionOperands owner (.store align src base 0 kind false) get target =
        .ok ([], target.replace {other with bytes := IR.scalarBytes width value}) ∧
      LiveRelated (source.replace {object with bytes := IR.scalarBytes width value})
        (target.replace {other with bytes := IR.scalarBytes width value}) := by
  obtain ⟨other, nativeAccess, objects⟩ := related.accessObject object.id 0 (width/8) (width/8)
    object 0 access
  have otherAccess : EvoIR.Machine.accessObject target (.pointer other.id 0) (width/8) (width/8) =
      .ok (other, 0) := by simpa only [objects.id] using nativeAccess
  have otherLength : other.bytes.length = width/8 := by simpa only [← objects.bytes] using length
  have stored : storePrivate target (.pointer object.id 0) width (width/8) (value.setWidth 64) =
      .ok (target.replace {other with bytes := IR.scalarBytes width value}) := by
    simpa only [objects.id] using store_scalar_bytes target other width (width/8) value admitted
      otherAccess otherLength
  refine ⟨other, objects, IR.store_scalar_bytes owner source object width value admitted access length,
    store_eval owner target _ align src base kind get object.id width (value.setWidth 64)
      address readValue matching alignment stored, ?_⟩
  exact related.replace _ _ (withBytes objects (IR.scalarBytes width value)) ⟨object, alive⟩

/-- The lowerer erases an admitted access marker. This changes neither actual
snapshot; it does not manufacture a machine load or an address-validity axiom. -/
theorem erased_access_transfer {source target : Memory} (related : LiveRelated source target)
    (owner id size align : Nat) (offset : Int) (object : IR.Object) (index : Nat)
    (access : IR.accessObject source (.pointer id offset) size align = .ok (object, index)) :
    IR.evalOperation owner (.access size align) [.memory source, .pointer id offset] =
      .ok [.memory source] ∧ LiveRelated source target := by
  exact ⟨by simp [IR.evalOperation, IR.memory, access, Bind.bind, Except.bind, Pure.pure, Except.pure], related⟩

theorem lifetime_eval (owner : Nat) (memory : Memory) (object : IR.Object)
    (alive : IR.liveObject memory object.id = .ok object) (owned : object.owner = owner) :
    IR.evalOperation owner .endLifetime [.memory memory, .world, .pointer object.id 0] =
      .ok [.memory (memory.replace {object with live := false}), .world] := by
  simp [IR.evalOperation, IR.memory, IR.world, IR.pointer, alive, owned,
    Bind.bind, Except.bind, Pure.pure, Except.pure]

/-- A static lifetime end has zero machine instructions. The target slot
remains live until frame exit; the relation stops licensing source accesses. -/
theorem erased_lifetime_transfer {source target : Memory} (related : LiveRelated source target)
    (owner : Nat) (object : IR.Object)
    (alive : IR.liveObject source object.id = .ok object) (owned : object.owner = owner) :
    IR.evalOperation owner .endLifetime [.memory source, .world, .pointer object.id 0] =
      .ok [.memory (source.replace {object with live := false}), .world] ∧
    LiveRelated (source.replace {object with live := false}) target := by
  obtain ⟨_, found, _⟩ := liveObject_info source object.id object alive
  exact ⟨lifetime_eval owner source object alive owned, related.retire object found⟩

theorem stackRelease_eval (owner : Nat) (memory : Memory) (address : VReg)
    (get : OperandReader) (objectId : Nat) (object : IR.Object)
    (readAddress : get address = .ok (.value (.pointer objectId 0)))
    (alive : IR.liveObject memory objectId = .ok object) (owned : object.owner = owner) :
    evalInstructionOperands owner (.stackRelease address) get memory =
      .ok ([], memory.replace {object with live := false}) := by
  simp [evalInstructionOperands, operandValue, readAddress, asPointer, alive, owned,
    Bind.bind, Except.bind, Pure.pure, Except.pure]

theorem release_transfer {source target : Memory} (related : LiveRelated source target)
    (owner : Nat) (object : IR.Object) (address : VReg) (get : OperandReader)
    (readAddress : get address = .ok (.value (.pointer object.id 0)))
    (alive : IR.liveObject source object.id = .ok object) (owned : object.owner = owner) :
    ∃ other : IR.Object,
      IR.evalOperation owner .endLifetime [.memory source, .world, .pointer object.id 0] =
        .ok [.memory (source.replace {object with live := false}), .world] ∧
      evalInstructionOperands owner (.stackRelease address) get target =
        .ok ([], target.replace {other with live := false}) ∧
      LiveRelated (source.replace {object with live := false})
        (target.replace {other with live := false}) := by
  obtain ⟨other, nativeAlive, objects⟩ := related.live object.id object alive
  have nativeOwned : other.owner = owner := objects.owner.symm.trans owned
  have retiredObjects : ObjectRelated {object with live := false} {other with live := false} :=
    ⟨objects.id, objects.origin, objects.owner, objects.align, objects.bytes,
      by intro impossible; cases impossible⟩
  exact ⟨other, lifetime_eval owner source object alive owned,
    stackRelease_eval owner target address get object.id other readAddress nativeAlive nativeOwned,
    related.replace _ _ retiredObjects ⟨object, alive⟩⟩

end EvoIR.Machine.PrivateMemory
