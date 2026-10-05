import EvoIR.Core.PrivateBytes

/-! Private scalar-storage facts about the actual target machine. The frame
relation retains an arbitrary caller snapshot. Object identities and the
frame counter are runtime data; no finite allocation log bounds execution. -/
namespace EvoIR.Core.IR

def Memory.WellAllocated (m : Memory) : Prop :=
  0 < m.nextId ∧ ∀ o ∈ m.objects, 0 < o.id ∧ o.id < m.nextId

def Memory.BeforeFrame (m : Memory) (frame : Nat) : Prop :=
  ∀ o ∈ m.objects, o.owner < frame

/-- Every object added by this activation is distinct from the arbitrary
caller objects. Objects belonging to nested activations are already dead. -/
def Memory.PrivateFrame (incoming : Memory) (owner : Nat) (m : Memory) : Prop :=
  incoming.nextId ≤ m.nextId ∧ incoming.BeforeFrame owner ∧
  ∃ added, m.objects = added ++ incoming.objects ∧
    ∀ o ∈ added, incoming.nextId ≤ o.id ∧ owner ≤ o.owner ∧
      (o.owner ≠ owner → o.live = false)

/-- The actual returned snapshot may contain arbitrarily many dead objects.
It must retain every caller object, including its bytes and live bit. -/
def Memory.ClosedExtension (incoming outgoing : Memory) : Prop :=
  incoming.nextId ≤ outgoing.nextId ∧
  ∃ dead, outgoing.objects = dead ++ incoming.objects ∧
    ∀ o ∈ dead, o.live = false ∧ incoming.nextId ≤ o.id

theorem Memory.well_allocated_fresh (m : Memory) (safe : m.WellAllocated) :
    m.objects.any (·.id == m.nextId) = false := by
  apply List.any_eq_false.mpr
  intro o member
  simpa using Nat.ne_of_lt (safe.2 o member).2

theorem Memory.private_entry (m : Memory) (owner : Nat) (fresh : m.BeforeFrame owner) :
    m.PrivateFrame owner m := by
  exact ⟨Nat.le_refl _, fresh, [], by simp, by simp⟩

theorem Memory.closed_refl (m : Memory) : m.ClosedExtension m :=
  ⟨Nat.le_refl _, [], by simp, by simp⟩

theorem Memory.closed_trans {a b c : Memory}
    (first : a.ClosedExtension b) (second : b.ClosedExtension c) : a.ClosedExtension c := by
  obtain ⟨ab, firstObjects, objects, dead⟩ := first
  obtain ⟨bc, secondObjects, next, nextDead⟩ := second
  refine ⟨Nat.le_trans ab bc, secondObjects ++ firstObjects, ?_, ?_⟩
  · rw [next, objects, List.append_assoc]
  · intro o member
    rcases List.mem_append.mp member with member | member
    · exact ⟨(nextDead o member).1, Nat.le_trans ab (nextDead o member).2⟩
    · exact dead o member

theorem Memory.end_frame_before (m : Memory) (owner : Nat) (before : m.BeforeFrame owner) :
    m.endFrame owner = m := by
  cases m with
  | mk next objects =>
    simp only [endFrame, Memory.mk.injEq, true_and]
    conv => rhs; rw [← List.map_id objects]
    apply List.map_congr_left
    intro o member
    have distinct : o.owner ≠ owner := Nat.ne_of_lt (before o member)
    simp [distinct]

theorem Memory.private_return {incoming m : Memory} {owner : Nat}
    (frame : incoming.PrivateFrame owner m) : incoming.ClosedExtension (m.endFrame owner) := by
  obtain ⟨monotone, before, added, objects, owned⟩ := frame
  refine ⟨monotone, added.map (fun o => if o.owner == owner then {o with live := false} else o), ?_, ?_⟩
  · simp only [endFrame, objects, List.map_append]
    have retained := congrArg Memory.objects (incoming.end_frame_before owner before)
    simp only [endFrame] at retained
    rw [retained]
  · intro o member
    obtain ⟨original, inAdded, rfl⟩ := List.mem_map.mp member
    split
    · exact ⟨rfl, (owned original inAdded).1⟩
    · rename_i other
      exact ⟨(owned original inAdded).2.2 (by simpa using other), (owned original inAdded).1⟩

theorem Memory.private_after_call {incoming before after : Memory} {owner : Nat}
    (frame : incoming.PrivateFrame owner before) (returned : before.ClosedExtension after)
    (allocated : before.WellAllocated)
    (newOwners : ∀ o ∈ after.objects, o ∉ before.objects → owner ≤ o.owner) :
    incoming.PrivateFrame owner after := by
  obtain ⟨monotone, fresh, added, objects, owned⟩ := frame
  obtain ⟨callMonotone, dead, returnedObjects, allDead⟩ := returned
  refine ⟨Nat.le_trans monotone callMonotone, fresh, dead ++ added, ?_, ?_⟩
  · rw [returnedObjects, objects, List.append_assoc]
  · intro o member
    rcases List.mem_append.mp member with member | member
    · constructor
      · exact Nat.le_trans monotone (allDead o member).2
      · constructor
        · apply newOwners o (by simp [returnedObjects, member])
          intro old
          have low := (allocated.2 o old).2
          have high := (allDead o member).2
          omega
        · intro _; exact (allDead o member).1
    · exact owned o member

theorem access_object_zero (m : Memory) (o : Object) (size align : Nat)
    (nonzero : o.id ≠ 0) (found : m.objects.find? (·.id == o.id) = some o)
    (live : o.live = true) (sizePositive : size ≠ 0) (alignPositive : align ≠ 0)
    (bounds : size ≤ o.bytes.length) (aligned : align ≤ o.align) :
    accessObject m (.pointer o.id 0) size align = .ok (o, 0) := by
  simp [accessObject, pointer, liveObject, nonzero, found, live, sizePositive, alignPositive,
    bounds, Nat.not_lt.mpr aligned, Bind.bind, Except.bind, pure, Except.pure]

def allocatedMemory (m : Memory) (owner origin size align : Nat) : Memory :=
  ⟨m.nextId+1, ⟨m.nextId, origin, owner, align, true, List.replicate size none⟩ :: m.objects⟩

theorem allocate_memory (m : Memory) (owner origin size align : Nat)
    (safe : m.WellAllocated) (sizePositive : size ≠ 0) (alignPositive : align ≠ 0) :
    evalOperation owner (.allocate origin size align) [.memory m, .world] =
      .ok [.pointer m.nextId 0, .memory (allocatedMemory m owner origin size align), .world] := by
  have nonzero : m.nextId ≠ 0 := Nat.ne_of_gt safe.1
  simp [evalOperation, memory, world, sizePositive, alignPositive, nonzero,
    m.well_allocated_fresh safe, allocatedMemory, Bind.bind, Except.bind, pure, Except.pure]

theorem Memory.allocated_safe (m : Memory) (owner origin size align : Nat)
    (safe : m.WellAllocated) : (allocatedMemory m owner origin size align).WellAllocated := by
  refine ⟨by simp [allocatedMemory], ?_⟩
  intro o member
  rcases List.mem_cons.mp member with rfl | member
  · exact ⟨safe.1, by simp [allocatedMemory]⟩
  · exact ⟨(safe.2 o member).1, Nat.lt_trans (safe.2 o member).2 (by simp [allocatedMemory])⟩

theorem Memory.allocated_before (m : Memory) (owner frame origin size align : Nat)
    (before : m.BeforeFrame frame) (fresh : owner < frame) :
    (allocatedMemory m owner origin size align).BeforeFrame frame := by
  intro o member
  rcases List.mem_cons.mp member with rfl | member
  · exact fresh
  · exact before o member

theorem Memory.allocated_private {incoming m : Memory} {owner : Nat}
    (frame : incoming.PrivateFrame owner m) (origin size align : Nat) :
    incoming.PrivateFrame owner (allocatedMemory m owner origin size align) := by
  obtain ⟨monotone, before, added, objects, owned⟩ := frame
  refine ⟨by simpa [allocatedMemory] using Nat.le_trans monotone (Nat.le_succ m.nextId),
    before, ⟨m.nextId, origin, owner, align, true, List.replicate size none⟩ :: added, ?_, ?_⟩
  · simp [allocatedMemory, objects]
  · intro o member
    rcases List.mem_cons.mp member with rfl | member
    · exact ⟨monotone, Nat.le_refl _, by simp⟩
    · exact owned o member

theorem Memory.find_allocated_new (m : Memory) (owner origin size align : Nat) :
    (allocatedMemory m owner origin size align).objects.find? (·.id == m.nextId) =
      some ⟨m.nextId, origin, owner, align, true, List.replicate size none⟩ := by
  simp [allocatedMemory]

theorem Memory.find_allocated_old (m : Memory) (owner origin size align id : Nat)
    (old : id < m.nextId) :
    (allocatedMemory m owner origin size align).objects.find? (·.id == id) =
      m.objects.find? (·.id == id) := by
  simp [allocatedMemory, Nat.ne_of_gt old]

theorem Memory.find_replace (m : Memory) (replacement : Object) (id : Nat) :
    (m.replace replacement).objects.find? (·.id == id) =
      (m.objects.find? (·.id == id)).map
        (fun old => if old.id == replacement.id then replacement else old) := by
  simp only [replace, List.find?_map]
  have predicate : ((fun (o : Object) => o.id == id) ∘
      (fun old => if old.id == replacement.id then replacement else old)) =
      (fun o => o.id == id) := by
    funext o
    by_cases same : o.id = replacement.id <;> simp [same]
  rw [predicate]

theorem Memory.replace_safe (m : Memory) (replacement : Object) (safe : m.WellAllocated)
    (positive : 0 < replacement.id) (bound : replacement.id < m.nextId) :
    (m.replace replacement).WellAllocated := by
  refine ⟨safe.1, ?_⟩
  intro o member
  simp only [replace] at member
  obtain ⟨old, inObjects, rfl⟩ := List.mem_map.mp member
  split
  · exact ⟨positive, bound⟩
  · exact safe.2 old inObjects

theorem Memory.replace_before (m : Memory) (replacement : Object) (frame : Nat)
    (before : m.BeforeFrame frame) (fresh : replacement.owner < frame) :
    (m.replace replacement).BeforeFrame frame := by
  intro o member
  simp only [replace] at member
  obtain ⟨old, inObjects, rfl⟩ := List.mem_map.mp member
  split
  · exact fresh
  · exact before old inObjects

theorem Memory.replace_private {incoming m : Memory} {owner : Nat} (replacement : Object)
    (frame : incoming.PrivateFrame owner m) (allocated : incoming.WellAllocated)
    (privateId : incoming.nextId ≤ replacement.id) (owned : replacement.owner = owner) :
    incoming.PrivateFrame owner (m.replace replacement) := by
  obtain ⟨monotone, before, added, objects, ownedObjects⟩ := frame
  refine ⟨monotone, before,
    added.map (fun old => if old.id == replacement.id then replacement else old), ?_, ?_⟩
  · simp only [replace, objects, List.map_append]
    have retained : incoming.objects.map (fun old =>
        if old.id == replacement.id then replacement else old) = incoming.objects := by
      conv => rhs; rw [← List.map_id incoming.objects]
      apply List.map_congr_left
      intro old member
      have different : old.id ≠ replacement.id := by
        have low := (allocated.2 old member).2
        omega
      simp [different]
    rw [retained]
  · intro o member
    obtain ⟨old, inAdded, rfl⟩ := List.mem_map.mp member
    split
    · exact ⟨privateId, by simp [owned], by simp [owned]⟩
    · exact ownedObjects old inAdded

theorem Memory.end_frame_safe (m : Memory) (owner : Nat) (safe : m.WellAllocated) :
    (m.endFrame owner).WellAllocated := by
  refine ⟨safe.1, ?_⟩
  intro o member
  simp only [endFrame] at member
  obtain ⟨old, inObjects, rfl⟩ := List.mem_map.mp member
  split <;> exact safe.2 old inObjects

theorem Memory.end_frame_bound (m : Memory) (owner frame : Nat) (before : m.BeforeFrame frame) :
    (m.endFrame owner).BeforeFrame frame := by
  intro o member
  simp only [endFrame] at member
  obtain ⟨old, inObjects, rfl⟩ := List.mem_map.mp member
  split <;> exact before old inObjects

theorem Memory.return_new_owners {incoming m : Memory} {owner : Nat}
    (frame : incoming.PrivateFrame owner m) :
    ∀ o ∈ (m.endFrame owner).objects, o ∉ incoming.objects → owner ≤ o.owner := by
  obtain ⟨_, before, added, objects, ownedObjects⟩ := frame
  intro o member new
  simp only [endFrame] at member
  obtain ⟨old, inObjects, rfl⟩ := List.mem_map.mp member
  rcases List.mem_append.mp (objects ▸ inObjects) with inAdded | inIncoming
  · split <;> exact (ownedObjects old inAdded).2.1
  · have below := before old inIncoming
    have different : old.owner ≠ owner := Nat.ne_of_lt below
    simp only [beq_eq_false_iff_ne.mpr different, Bool.false_eq_true, ↓reduceIte] at new
    exact False.elim (new inIncoming)

end EvoIR.Core.IR
