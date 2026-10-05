import EvoIR.MachinePrivateMemory
import EvoIR.Core.PrivateStorage

/-!
The selection relation observes live source objects. A static machine slot
can remain allocated after the corresponding IR lifetime has ended. Dead
source objects cannot justify a load, store, or pointer comparison. Fresh
allocation still checks the complete actual object lists on both sides.
-/
namespace EvoIR.Machine.PrivateMemory

open EvoIR.Core

structure LiveRelated (source target : IR.Memory) : Prop where
  nextId : source.nextId = target.nextId
  sourceAllocated : source.WellAllocated
  targetAllocated : target.WellAllocated
  live : ∀ id object, IR.liveObject source id = .ok object →
    ∃ other, IR.liveObject target id = .ok other ∧ ObjectRelated object other

theorem LiveRelated.refl (memory : IR.Memory) (allocated : memory.WellAllocated) :
    LiveRelated memory memory :=
  ⟨rfl, allocated, allocated, fun _ object found => ⟨object, found, ObjectRelated.refl object⟩⟩

theorem liveObject_info (memory : IR.Memory) (id : Nat) (object : IR.Object)
    (found : IR.liveObject memory id = .ok object) :
    id ≠ 0 ∧ memory.objects.find? (·.id == id) = some object ∧ object.live = true := by
  by_cases zero : id = 0
  · simp [IR.liveObject, zero, Bind.bind, Except.bind, throw, throwThe, MonadExceptOf.throw] at found
  · cases lookup : memory.objects.find? (·.id == id) with
    | none => simp [IR.liveObject, zero, lookup, Bind.bind, Except.bind,
        throw, throwThe, MonadExceptOf.throw] at found
    | some selected =>
      by_cases active : selected.live = true
      · have equal : selected = object := by
          simpa [IR.liveObject, zero, lookup, active, Bind.bind, Except.bind,
            Pure.pure, Except.pure] using found
        subst object
        exact ⟨zero, rfl, active⟩
      · have dead : selected.live = false := Bool.eq_false_iff.mpr active
        simp [IR.liveObject, zero, lookup, dead, Bind.bind, Except.bind,
          throw, throwThe, MonadExceptOf.throw] at found

theorem liveObject_of_info (memory : IR.Memory) (id : Nat) (object : IR.Object)
    (nonzero : id ≠ 0) (found : memory.objects.find? (·.id == id) = some object)
    (active : object.live = true) : IR.liveObject memory id = .ok object := by
  simp [IR.liveObject, nonzero, found, active, Bind.bind, Except.bind, Pure.pure, Except.pure]

theorem LiveRelated.allocate {source target : IR.Memory} (related : LiveRelated source target)
    (owner origin size align : Nat) :
    LiveRelated (IR.allocatedMemory source owner origin size align)
      (IR.allocatedMemory target owner origin size align) := by
  refine ⟨by simp [IR.allocatedMemory, related.nextId],
    source.allocated_safe owner origin size align related.sourceAllocated,
    target.allocated_safe owner origin size align related.targetAllocated, ?_⟩
  intro id object found
  obtain ⟨nonzero, lookup, active⟩ := liveObject_info _ id object found
  by_cases fresh : id = source.nextId
  · subst id
    have same : (⟨source.nextId, origin, owner, align, true,
        List.replicate size none⟩ : IR.Object) = object := by
      simpa only [source.find_allocated_new owner origin size align, Option.some.injEq] using lookup
    subst object
    refine ⟨⟨target.nextId, origin, owner, align, true, List.replicate size none⟩,
      liveObject_of_info _ _ _ nonzero ?_ rfl, ?_⟩
    · simpa only [related.nextId] using target.find_allocated_new owner origin size align
    · exact ⟨related.nextId, rfl, rfl, rfl, rfl, fun _ => rfl⟩
  · have original : source.objects.find? (·.id == id) = some object := by
      simpa [IR.allocatedMemory, List.find?_cons, Ne.symm fresh] using lookup
    obtain ⟨other, targetLive, same⟩ := related.live id object
      (liveObject_of_info source id object nonzero original active)
    obtain ⟨_, targetLookup, targetActive⟩ := liveObject_info target id other targetLive
    refine ⟨other, liveObject_of_info _ id other nonzero ?_ targetActive, same⟩
    simpa [IR.allocatedMemory, List.find?_cons, ← related.nextId, Ne.symm fresh] using targetLookup

/-- Source lifetime retirement needs no machine instruction. Only that
object's now-unusable source assertions are removed from the relation. -/
theorem LiveRelated.retire {source target : IR.Memory} (related : LiveRelated source target)
    (object : IR.Object) (found : source.objects.find? (·.id == object.id) = some object) :
    LiveRelated (source.replace {object with live := false}) target := by
  have member := List.mem_of_find?_eq_some found
  refine ⟨related.nextId,
    source.replace_safe {object with live := false} related.sourceAllocated
      (related.sourceAllocated.2 object member).1 (related.sourceAllocated.2 object member).2,
    related.targetAllocated, ?_⟩
  intro id selected live
  obtain ⟨nonzero, lookup, active⟩ := liveObject_info _ id selected live
  rw [IR.Memory.find_replace] at lookup
  cases original : source.objects.find? (·.id == id) with
  | none => simp [original] at lookup
  | some previous =>
    simp only [original, Option.map_some] at lookup
    by_cases replaced : previous.id == object.id
    · have same : {object with live := false} = selected := by simpa [replaced] using lookup
      subst selected
      contradiction
    · have same : previous = selected := by simpa [replaced] using lookup
      subst selected
      exact related.live id previous (liveObject_of_info source id previous nonzero original active)

theorem LiveRelated.replace {source target : IR.Memory} (related : LiveRelated source target)
    (left right : IR.Object) (objects : ObjectRelated left right)
    (available : ∃ previous, IR.liveObject source left.id = .ok previous) :
    LiveRelated (source.replace left) (target.replace right) := by
  obtain ⟨oldObject, previousLive⟩ := available
  obtain ⟨_, present, _⟩ := liveObject_info source left.id oldObject previousLive
  have identity : oldObject.id = left.id := by simpa using List.find?_some present
  have member := List.mem_of_find?_eq_some present
  have positive : 0 < left.id := by simpa only [← identity] using (related.sourceAllocated.2 oldObject member).1
  have bound : left.id < source.nextId := by simpa only [← identity] using (related.sourceAllocated.2 oldObject member).2
  refine ⟨related.nextId,
    source.replace_safe left related.sourceAllocated positive bound,
    target.replace_safe right related.targetAllocated
      (by simpa only [← objects.id] using positive)
      (by simpa only [← objects.id, ← related.nextId] using bound), ?_⟩
  intro id selected live
  obtain ⟨nonzero, lookup, active⟩ := liveObject_info _ id selected live
  rw [IR.Memory.find_replace] at lookup
  cases original : source.objects.find? (·.id == id) with
  | none => simp [original] at lookup
  | some previous =>
    simp only [original, Option.map_some] at lookup
    by_cases replaced : previous.id == left.id
    · have same : left = selected := by simpa [replaced] using lookup
      subst selected
      have idEq : left.id = id := by
        have oldId : previous.id = id := by simpa using List.find?_some original
        exact (beq_iff_eq.mp replaced).symm.trans oldId
      obtain ⟨other, targetLive, oldRelated⟩ := related.live left.id _ previousLive
      have targetAt : IR.liveObject target id = .ok other := by simpa only [idEq] using targetLive
      obtain ⟨_, targetLookup, _⟩ := liveObject_info target id other targetAt
      refine ⟨right, liveObject_of_info _ id right nonzero ?_ (objects.live active), objects⟩
      rw [IR.Memory.find_replace, targetLookup]
      simp only [Option.map_some, ← oldRelated.id, identity, objects.id, beq_self_eq_true, ↓reduceIte]
    · have same : previous = selected := by simpa [replaced] using lookup
      subst selected
      obtain ⟨other, targetLive, oldRelated⟩ := related.live id previous
        (liveObject_of_info source id previous nonzero original active)
      obtain ⟨_, targetLookup, targetActive⟩ := liveObject_info target id other targetLive
      refine ⟨other, liveObject_of_info _ id other nonzero ?_ targetActive, oldRelated⟩
      rw [IR.Memory.find_replace, targetLookup]
      simp only [Option.map_some, ← oldRelated.id, ← objects.id, replaced, Bool.false_eq_true, ↓reduceIte]

theorem find_endFrame (memory : IR.Memory) (owner id : Nat) :
    (memory.endFrame owner).objects.find? (·.id == id) =
      (memory.objects.find? (·.id == id)).map
        (fun object => if object.owner == owner then {object with live := false} else object) := by
  simp only [IR.Memory.endFrame, List.find?_map]
  have predicate : ((fun (object : IR.Object) => object.id == id) ∘
      (fun object => if object.owner == owner then {object with live := false} else object)) =
      (fun object => object.id == id) := by
    funext object
    dsimp only [Function.comp_def]
    split <;> rfl
  rw [predicate]

theorem LiveRelated.endFrame {source target : IR.Memory} (related : LiveRelated source target)
    (owner : Nat) : LiveRelated (source.endFrame owner) (target.endFrame owner) := by
  refine ⟨related.nextId, source.end_frame_safe owner related.sourceAllocated,
    target.end_frame_safe owner related.targetAllocated, ?_⟩
  intro id selected live
  obtain ⟨nonzero, lookup, active⟩ := liveObject_info _ id selected live
  rw [find_endFrame] at lookup
  cases original : source.objects.find? (·.id == id) with
  | none => simp [original] at lookup
  | some previous =>
    simp only [original, Option.map_some] at lookup
    by_cases owned : previous.owner == owner
    · have same : {previous with live := false} = selected := by simpa [owned] using lookup
      subst selected
      contradiction
    · have same : previous = selected := by simpa [owned] using lookup
      subst selected
      obtain ⟨other, targetLive, objects⟩ := related.live id previous
        (liveObject_of_info source id previous nonzero original active)
      obtain ⟨_, targetLookup, targetActive⟩ := liveObject_info target id other targetLive
      refine ⟨other, liveObject_of_info _ id other nonzero ?_ targetActive, objects⟩
      rw [find_endFrame, targetLookup]
      simp only [Option.map_some, ← objects.owner, owned, Bool.false_eq_true, ↓reduceIte]

theorem LiveRelated.accessObject {source target : IR.Memory} (related : LiveRelated source target)
    (id : Nat) (offset : Int) (size align : Nat) (object : IR.Object) (index : Nat)
    (access : IR.accessObject source (.pointer id offset) size align = .ok (object, index)) :
    ∃ other, EvoIR.Machine.accessObject target (.pointer id offset) size align = .ok (other, index) ∧
      ObjectRelated object other := by
  simp only [IR.accessObject, IR.pointer, Bind.bind, Except.bind] at access
  cases alive : IR.liveObject source id with
  | error fault => simp [alive, Except.bind] at access
  | ok sourceObject =>
    simp only [alive, Except.bind] at access
    split at access
    · simp [throw, throwThe, MonadExceptOf.throw] at access
    · rename_i layout
      split at access
      · simp [throw, throwThe, MonadExceptOf.throw] at access
      · rename_i bounds
        split at access
        · simp [throw, throwThe, MonadExceptOf.throw] at access
        · rename_i alignment
          have result : sourceObject = object ∧ offset.toNat = index := by
            simpa [Pure.pure, Except.pure] using access
          rcases result with ⟨rfl, rfl⟩
          obtain ⟨other, targetLive, objects⟩ := related.live id sourceObject alive
          refine ⟨other, ?_, objects⟩
          simp [EvoIR.Machine.accessObject, asPointer, IR.accessObject, IR.pointer,
            targetLive, layout, ← objects.bytes, ← objects.align, bounds, alignment,
            Bind.bind, Except.bind, Pure.pure, Except.pure]

end EvoIR.Machine.PrivateMemory
