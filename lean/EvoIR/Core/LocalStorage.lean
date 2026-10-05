import EvoIR.Core.Execution

/-! Source-local storage invariants used by the source/IR relation. These
lemmas concern the actual execution functions, including shadowing and const
initialization; they are not a second object model. -/
namespace EvoIR.Core.Source

/-- Successful initialization/assignment makes the selected object readable
at precisely its declared type. No prior initializedness assumption is used. -/
theorem read_written_slot (locals updated : Locals) (symbol : Nat)
    (destination : QualifiedType) (value : Value) (initialization : Bool)
    (written : writeSlot locals symbol destination value initialization = .ok updated) :
    readSlot updated symbol destination.type = .ok value := by
  induction locals generalizing updated with
  | nil => simp [writeSlot] at written
  | cons slot rest ih =>
    by_cases found : slot.symbol = symbol
    · simp only [writeSlot, found, BEq.rfl, ↓reduceIte] at written
      split at written
      · simp [throw, throwThe, MonadExceptOf.throw, bind, Except.bind] at written
      · rename_i types
        have slotType : slot.type = destination := by simp_all
        have valueType : value.type = destination.type := by simp_all
        cases initialization <;> simp_all only [Bool.false_eq_true, ↓reduceIte]
        all_goals split at written
        all_goals simp only [throw, throwThe, MonadExceptOf.throw, bind, Except.bind,
          pure, Except.pure, Except.ok.injEq] at written
        all_goals first | contradiction | subst updated
        all_goals simp [readSlot, List.find?, valueType, pure, Except.pure]
    · simp only [writeSlot, beq_iff_eq, found, ↓reduceIte] at written
      cases result : writeSlot rest symbol destination value initialization with
      | error error => simp [result, Functor.map, Except.map] at written
      | ok tail =>
        simp only [result, bind, Except.bind, pure, Except.pure, Except.ok.injEq] at written
        subst updated
        have notFound : (slot.symbol == symbol) = false := beq_eq_false_iff_ne.mpr found
        simpa [readSlot, List.find?, notFound] using ih tail result

/-- A local write does not change scope depth. Scope exit may therefore use
the captured incoming depth even after assignments inside a loop/call body. -/
theorem write_slot_length (locals updated : Locals) (symbol : Nat)
    (destination : QualifiedType) (value : Value) (initialization : Bool)
    (written : writeSlot locals symbol destination value initialization = .ok updated) :
    updated.length = locals.length := by
  induction locals generalizing updated with
  | nil => simp [writeSlot] at written
  | cons slot rest ih =>
    simp only [writeSlot] at written
    split at written
    · split at written
      · simp [throw, throwThe, MonadExceptOf.throw, bind, Except.bind] at written
      · cases initialization <;> simp_all only [Bool.false_eq_true, ↓reduceIte]
        all_goals split at written
        all_goals simp only [throw, throwThe, MonadExceptOf.throw, bind, Except.bind,
          pure, Except.pure, Except.ok.injEq] at written
        all_goals first | contradiction | subst updated
        all_goals rfl
    · cases result : writeSlot rest symbol destination value initialization with
      | error error => simp [result, Functor.map, Except.map] at written
      | ok tail =>
        simp only [result, bind, Except.bind, pure, Except.pure, Except.ok.injEq] at written
        subst updated
        simpa using ih tail result

/-- A successful write cannot supply or overwrite another identifier's value,
including an uninitialized binding or a differently typed shadowed object. -/
theorem read_other_slot (locals updated : Locals) (symbol other : Nat)
    (destination : QualifiedType) (type : IntType) (value : Value) (initialization : Bool)
    (distinct : other ≠ symbol)
    (written : writeSlot locals symbol destination value initialization = .ok updated) :
    readSlot updated other type = readSlot locals other type := by
  induction locals generalizing updated with
  | nil => simp [writeSlot] at written
  | cons slot rest ih =>
    by_cases found : slot.symbol = symbol
    · simp only [writeSlot, found, BEq.rfl, ↓reduceIte] at written
      split at written
      · simp [throw, throwThe, MonadExceptOf.throw, bind, Except.bind] at written
      · cases initialization <;> simp_all only [Bool.false_eq_true, ↓reduceIte]
        all_goals split at written
        all_goals simp only [throw, throwThe, MonadExceptOf.throw, bind, Except.bind,
          pure, Except.pure, Except.ok.injEq] at written
        all_goals first | contradiction | subst updated
        all_goals have notOther : (symbol == other) = false :=
          beq_eq_false_iff_ne.mpr (Ne.symm distinct)
        all_goals simp only [readSlot, List.find?, found, notOther]
    · simp only [writeSlot, beq_iff_eq, found, ↓reduceIte] at written
      cases result : writeSlot rest symbol destination value initialization with
      | error error => simp [result, Functor.map, Except.map] at written
      | ok tail =>
        simp only [result, bind, Except.bind, pure, Except.pure, Except.ok.injEq] at written
        subst updated
        by_cases isOther : slot.symbol == other
        · simp [readSlot, List.find?, isOther]
        · have notOther : (slot.symbol == other) = false := Bool.eq_false_iff.mpr isOther
          simpa [readSlot, List.find?, notOther] using ih tail result

/-- A successful actual write changes exactly the first selected slot. This
decomposition retains its type/qualifier and every other slot and does not
assume a producer-provided local-state annotation. -/
theorem write_slot_decompose (locals updated : Locals) (symbol : Nat)
    (destination : QualifiedType) (value : Value) (initialization : Bool)
    (written : writeSlot locals symbol destination value initialization = .ok updated) :
    ∃ pre slot suffix,
      locals = pre ++ slot :: suffix ∧
      updated = pre ++ {slot with value := some value} :: suffix ∧
      slot.symbol = symbol ∧ slot.type = destination ∧ value.type = destination.type ∧
      ∀ before ∈ pre, before.symbol ≠ symbol := by
  induction locals generalizing updated with
  | nil => simp [writeSlot] at written
  | cons head rest ih =>
    by_cases selected : head.symbol = symbol
    · simp only [writeSlot, selected, BEq.rfl, ↓reduceIte] at written
      split at written
      · simp [throw, throwThe, MonadExceptOf.throw, bind, Except.bind] at written
      · rename_i types
        have slotType : head.type = destination := by simp_all
        have valueType : value.type = destination.type := by simp_all
        cases initialization <;> simp_all only [Bool.false_eq_true, ↓reduceIte]
        all_goals split at written
        all_goals simp only [throw, throwThe, MonadExceptOf.throw, bind, Except.bind,
          pure, Except.pure, Except.ok.injEq] at written
        all_goals first | contradiction | subst updated
        all_goals refine ⟨[], head, rest, ?_⟩
        all_goals simp_all
    · simp only [writeSlot, beq_iff_eq, selected, ↓reduceIte] at written
      cases result : writeSlot rest symbol destination value initialization with
      | error fault => simp [result, Functor.map, Except.map] at written
      | ok tail =>
        simp only [result, bind, Except.bind, pure, Except.pure, Except.ok.injEq] at written
        subst updated
        obtain ⟨pre, slot, suffix, before, after, same, slotType, valueType, first⟩ := ih tail result
        refine ⟨head :: pre, slot, suffix, ?_, ?_, same, slotType, valueType, ?_⟩
        · simp [before]
        · simp [after]
        · intro slot member
          rcases List.mem_cons.mp member with rfl | member
          · exact selected
          · exact first slot member

theorem write_slot_binding_types (locals updated : Locals) (symbol : Nat)
    (destination : QualifiedType) (value : Value) (initialization : Bool)
    (written : writeSlot locals symbol destination value initialization = .ok updated) :
    updated.map (fun slot => (slot.symbol, slot.type)) =
      locals.map (fun slot => (slot.symbol, slot.type)) := by
  obtain ⟨pre, slot, suffix, rfl, rfl, _⟩ :=
    write_slot_decompose locals updated symbol destination value initialization written
  simp

theorem write_slot_value_types (locals updated : Locals) (symbol : Nat)
    (destination : QualifiedType) (value : Value) (initialization : Bool)
    (written : writeSlot locals symbol destination value initialization = .ok updated)
    (typed : ∀ slot ∈ locals, ∀ stored, slot.value = some stored → stored.type = slot.type.type) :
    ∀ slot ∈ updated, ∀ stored, slot.value = some stored → stored.type = slot.type.type := by
  obtain ⟨pre, changed, suffix, rfl, rfl, _, changedType, valueType, _⟩ :=
    write_slot_decompose locals updated symbol destination value initialization written
  intro slot member stored hasValue
  rcases List.mem_append.mp member with member | member
  · exact typed slot (List.mem_append_left _ member) stored hasValue
  · rcases List.mem_cons.mp member with rfl | member
    · have same : value = stored := Option.some.inj hasValue
      simpa [← same, changedType] using valueType
    · exact typed slot (List.mem_append_right _ (List.mem_cons_of_mem _ member)) stored hasValue

theorem write_slot_lookup_type (locals updated : Locals) (symbol id : Nat)
    (destination : QualifiedType) (value : Value) (initialization : Bool)
    (written : writeSlot locals symbol destination value initialization = .ok updated) :
    (updated.find? (·.symbol == id)).map Slot.type =
      (locals.find? (·.symbol == id)).map Slot.type := by
  have shapes := write_slot_binding_types locals updated symbol destination value initialization written
  have found := congrArg (fun bindings : List (Nat × QualifiedType) =>
    (bindings.find? (fun pair => pair.1 == id)).map Prod.snd) shapes
  simpa [List.find?_map, Function.comp_def, Option.map_map] using found

/-- Static binding/permission facts suffice for the actual write to succeed;
success is not an assumed premise of assignment safety. -/
theorem write_slot_exists (locals : Locals) (symbol : Nat) (slot : Slot)
    (destination : QualifiedType) (value : Value) (initialization : Bool)
    (found : locals.find? (·.symbol == symbol) = some slot)
    (slotType : slot.type = destination) (valueType : value.type = destination.type)
    (fresh : initialization = true → destination.isConst = true → slot.value = none)
    (writable : initialization = false → destination.isConst = false) :
    ∃ updated, writeSlot locals symbol destination value initialization = .ok updated := by
  induction locals with
  | nil => simp at found
  | cons head rest ih =>
    by_cases selected : head.symbol = symbol
    · have same : head = slot := by simpa [List.find?, selected] using found
      subst slot
      cases initialization with
      | false =>
        refine ⟨{head with value := some value} :: rest, ?_⟩
        simp [writeSlot, selected, slotType, valueType, writable rfl, pure, Except.pure]
      | true =>
        refine ⟨{head with value := some value} :: rest, ?_⟩
        cases permission : destination.isConst with
        | false => simp [writeSlot, selected, slotType, valueType, permission, pure, Except.pure]
        | true => simp [writeSlot, selected, slotType, valueType, permission, fresh rfl permission,
            pure, Except.pure]
    · have notSelected : (head.symbol == symbol) = false := beq_eq_false_iff_ne.mpr selected
      have tail : rest.find? (·.symbol == symbol) = some slot := by
        simpa [List.find?, notSelected] using found
      obtain ⟨updated, written⟩ := ih tail
      exact ⟨head :: updated, by simp [writeSlot, selected, written, bind, Except.bind, pure, Except.pure]⟩

theorem read_slot_witness (locals : Locals) (symbol : Nat) (type : IntType) (value : Value)
    (read : readSlot locals symbol type = .ok value) :
    ∃ slot, locals.find? (·.symbol == symbol) = some slot ∧ slot.value = some value := by
  cases found : locals.find? (·.symbol == symbol) with
  | none => simp [readSlot, found, throw, throwThe, MonadExceptOf.throw] at read
  | some slot =>
    by_cases typed : slot.type.type = type
    · cases initialized : slot.value with
      | none => simp [readSlot, found, typed, initialized, throw, throwThe, MonadExceptOf.throw] at read
      | some stored =>
        by_cases represented : stored.type = type
        · have same : stored = value := by simpa [readSlot, found, typed, initialized,
            represented, pure, Except.pure] using read
          exact ⟨slot, rfl, same ▸ initialized⟩
        · simp [readSlot, found, typed, initialized, represented, throw, throwThe,
            MonadExceptOf.throw, Functor.map, Except.map] at read
    · simp [readSlot, found, typed, throw, throwThe, MonadExceptOf.throw,
        bind, Except.bind] at read

-- The first matching binding is selected; a hidden same-ID slot stays hidden.
example :
    (writeSlot [⟨1, ⟨.int, false⟩, some (.ofNat .int 2)⟩,
                ⟨1, ⟨.int, false⟩, some (.ofNat .int 9)⟩]
      1 ⟨.int, false⟩ (.ofNat .int 4) false).map (fun l => readSlot l 1 .int) =
      .ok (.ok (.ofNat .int 4)) := by rfl

-- Const initialization is permitted once; assignment does not acquire that right.
example :
    (writeSlot [⟨1, ⟨.int, true⟩, none⟩] 1 ⟨.int, true⟩ (.ofNat .int 4) true).map
      (fun l => readSlot l 1 .int) = .ok (.ok (.ofNat .int 4)) := by rfl
example :
    (writeSlot [⟨1, ⟨.int, true⟩, none⟩] 1 ⟨.int, true⟩ (.ofNat .int 4) false).map
      (fun l => readSlot l 1 .int) = .error (.malformed "const assignment") := by rfl

end EvoIR.Core.Source
