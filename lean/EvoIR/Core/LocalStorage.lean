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
