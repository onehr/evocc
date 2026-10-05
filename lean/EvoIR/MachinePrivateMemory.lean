import EvoIR.MachineExecution

/-!
The machine retains storage for static frame slots after an EvoIR lifetime
ends. The relation below preserves identity and exact byte contents but does
not require such retired machine slots to have their live bit cleared early.
Access correspondence therefore requires a live source access. It does not
turn use-after-lifetime into a successful machine access.
-/
namespace EvoIR.Machine.PrivateMemory

open EvoIR.Core

structure ObjectRelated (source target : IR.Object) : Prop where
  id : source.id = target.id
  origin : source.origin = target.origin
  owner : source.owner = target.owner
  align : source.align = target.align
  bytes : source.bytes = target.bytes
  live : source.live = true → target.live = true

inductive ObjectRowsRelated : List IR.Object → List IR.Object → Prop where
  | nil : ObjectRowsRelated [] []
  | cons {source target : IR.Object} {sources targets : List IR.Object} :
      ObjectRelated source target → ObjectRowsRelated sources targets →
      ObjectRowsRelated (source :: sources) (target :: targets)

structure Related (source target : IR.Memory) : Prop where
  nextId : source.nextId = target.nextId
  objects : ObjectRowsRelated source.objects target.objects

theorem ObjectRelated.refl (object : IR.Object) : ObjectRelated object object :=
  ⟨rfl, rfl, rfl, rfl, rfl, fun live => live⟩

theorem ObjectRelated.trans {a b c : IR.Object}
    (ab : ObjectRelated a b) (bc : ObjectRelated b c) : ObjectRelated a c :=
  ⟨ab.id.trans bc.id, ab.origin.trans bc.origin, ab.owner.trans bc.owner,
    ab.align.trans bc.align, ab.bytes.trans bc.bytes, fun live => bc.live (ab.live live)⟩

theorem Related.refl (memory : IR.Memory) : Related memory memory := by
  refine ⟨rfl, ?_⟩
  induction memory.objects with
  | nil => exact .nil
  | cons object rest ih => exact .cons (.refl object) ih

theorem object_endFrame (frame : Nat) {source target : IR.Object}
    (related : ObjectRelated source target) :
    ObjectRelated
      (if source.owner == frame then {source with live := false} else source)
      (if target.owner == frame then {target with live := false} else target) := by
  by_cases ended : source.owner == frame
  · have targetEnded : target.owner == frame := by simpa only [← related.owner] using ended
    simp only [ended, targetEnded, ↓reduceIte]
    exact ⟨related.id, related.origin, related.owner, related.align, related.bytes,
      by intro impossible; cases impossible⟩
  · have targetEnded : ¬target.owner == frame := by simpa only [← related.owner] using ended
    simp only [ended, targetEnded, ↓reduceIte]
    exact related

theorem endFrame {source target : IR.Memory} (related : Related source target) (frame : Nat) :
    Related (source.endFrame frame) (target.endFrame frame) := by
  rcases source with ⟨sourceNext, sourceObjects⟩
  rcases target with ⟨targetNext, targetObjects⟩
  rcases related with ⟨next, rows⟩
  change ObjectRowsRelated sourceObjects targetObjects at rows
  refine ⟨next, ?_⟩
  change ObjectRowsRelated (sourceObjects.map _) (targetObjects.map _)
  clear next
  induction rows with
  | nil => exact .nil
  | cons objects rest ih => exact .cons (object_endFrame frame objects) ih

theorem withBytes {source target : IR.Object} (related : ObjectRelated source target)
    (bytes : List (Option (BitVec 8))) :
    ObjectRelated {source with bytes} {target with bytes} :=
  ⟨related.id, related.origin, related.owner, related.align, rfl, related.live⟩

theorem retired {source target : IR.Object} (related : ObjectRelated source target) :
    ObjectRelated {source with live := false} target :=
  ⟨related.id, related.origin, related.owner, related.align, related.bytes,
    by intro impossible; cases impossible⟩

theorem replace {source target : IR.Memory} (related : Related source target)
    {left right : IR.Object} (objects : ObjectRelated left right) :
    Related (source.replace left) (target.replace right) := by
  rcases source with ⟨sourceNext, sourceObjects⟩
  rcases target with ⟨targetNext, targetObjects⟩
  rcases related with ⟨next, rows⟩
  change ObjectRowsRelated sourceObjects targetObjects at rows
  refine ⟨next, ?_⟩
  change ObjectRowsRelated (sourceObjects.map _) (targetObjects.map _)
  clear next
  induction rows with
  | nil => exact .nil
  | @cons sourceHead targetHead sourceTail targetTail head tail ih =>
    apply ObjectRowsRelated.cons _ ih
    dsimp only
    rw [head.id, objects.id]
    split
    · exact objects
    · exact head

theorem allocate {source target : IR.Memory} (related : Related source target)
    (origin owner align size : Nat) :
    Related
      ⟨source.nextId + 1,
        ⟨source.nextId, origin, owner, align, true, List.replicate size none⟩ :: source.objects⟩
      ⟨target.nextId + 1,
        ⟨target.nextId, origin, owner, align, true, List.replicate size none⟩ :: target.objects⟩ := by
  refine ⟨by rw [related.nextId], .cons ?_ related.objects⟩
  exact ⟨related.nextId, rfl, rfl, rfl, rfl, id⟩

private theorem findObject {sources targets : List IR.Object}
    (related : ObjectRowsRelated sources targets) (id : Nat) (source : IR.Object)
    (found : sources.find? (·.id == id) = some source) :
    ∃ target, targets.find? (·.id == id) = some target ∧ ObjectRelated source target := by
  induction related with
  | nil => simp at found
  | @cons left right lefts rights head tail ih =>
    by_cases hit : left.id == id
    · have same : left = source := by simpa [List.find?_cons, hit] using found
      subst source
      exact ⟨right, by simp [List.find?_cons, ← head.id, hit], head⟩
    · have rest : lefts.find? (·.id == id) = some source := by
        simpa [List.find?_cons, hit] using found
      obtain ⟨target, next, same⟩ := ih rest
      exact ⟨target, by simpa [List.find?_cons, ← head.id, hit] using next, same⟩

theorem liveObject {source target : IR.Memory} (related : Related source target)
    (id : Nat) (object : IR.Object) (live : IR.liveObject source id = .ok object) :
    ∃ other, IR.liveObject target id = .ok other ∧ ObjectRelated object other := by
  by_cases zero : id = 0
  · simp [IR.liveObject, zero, Bind.bind, Except.bind, throw, throwThe, MonadExceptOf.throw] at live
  · cases found : source.objects.find? (·.id == id) with
    | none => simp [IR.liveObject, zero, found, Bind.bind, Except.bind,
        throw, throwThe, MonadExceptOf.throw] at live
    | some sourceObject =>
      by_cases alive : sourceObject.live = true
      · have objectEq : sourceObject = object := by
          simpa [IR.liveObject, zero, found, alive, Bind.bind, Except.bind,
            Pure.pure, Except.pure] using live
        subst object
        obtain ⟨other, targetFound, objects⟩ := findObject related.objects id sourceObject found
        refine ⟨other, ?_, objects⟩
        simp [IR.liveObject, zero, targetFound, objects.live alive,
          Bind.bind, Except.bind, Pure.pure, Except.pure]
      · have dead : sourceObject.live = false := Bool.eq_false_iff.mpr alive
        simp [IR.liveObject, zero, found, dead, Bind.bind, Except.bind,
          throw, throwThe, MonadExceptOf.throw] at live

theorem accessObject {source target : IR.Memory} (related : Related source target)
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
          obtain ⟨other, targetLive, objects⟩ := liveObject related id sourceObject alive
          refine ⟨other, ?_, objects⟩
          simp [EvoIR.Machine.accessObject, asPointer, IR.accessObject, IR.pointer,
            targetLive, layout, ← objects.bytes, ← objects.align, bounds, alignment,
            Bind.bind, Except.bind, Pure.pure, Except.pure]

end EvoIR.Machine.PrivateMemory
