import EvoIR.Core.SourceExpression
import EvoIR.Core.LocalStorage

/-! Exact symbolic local storage for stateful source diagrams. Qualifiers,
uninitialized slots and lexical ordering remain explicit. Symbolic writes use
the same first-binding rule as the actual machine, not an SSA-name heuristic. -/
namespace EvoIR.Core.Symbolic

structure LocalSlot (Γ : Var → Nat) where
  symbol : Nat
  type : Source.QualifiedType
  value : Option (Term Γ type.type.width)

abbrev LocalStore (Γ : Var → Nat) := List (LocalSlot Γ)

def LocalSlot.denote {Γ : Var → Nat} (σ : Subst Γ) (slot : LocalSlot Γ) : Source.Slot :=
  ⟨slot.symbol, slot.type, slot.value.map (fun term => ⟨slot.type.type, denoteTerm σ term⟩)⟩

def LocalStore.denote {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ) : Source.Locals :=
  locals.map (LocalSlot.denote σ)

def LocalStore.read {Γ : Var → Nat} (locals : LocalStore Γ) (symbol : Nat)
    (type : Source.IntType) : Option (CWord Γ) := do
  let slot ← locals.find? (·.symbol == symbol)
  if slot.type.type != type then none else
  let term ← slot.value
  return ⟨slot.type.type, term⟩

theorem local_find_sound {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ) (id : Nat) :
    (locals.denote σ).find? (·.symbol == id) =
      (locals.find? (·.symbol == id)).map (LocalSlot.denote σ) := by
  simp [LocalStore.denote, List.find?_map, Function.comp_def, LocalSlot.denote]

theorem local_read_sound {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ)
    (symbol : Nat) (type : Source.IntType) (value : CWord Γ)
    (accepted : locals.read symbol type = some value) :
    Source.readSlot (locals.denote σ) symbol type = .ok (value.denote σ) ∧ value.type = type := by
  simp only [LocalStore.read, bind, Option.bind] at accepted
  split at accepted
  · contradiction
  · rename_i slot found
    dsimp only at accepted
    split at accepted
    · contradiction
    · rename_i same
      have typed : slot.type.type = type := by simpa using same
      cases term : slot.value with
      | none => simp [term] at accepted
      | some termValue =>
        simp only [term, pure, Option.some.injEq] at accepted
        subst value
        constructor
        · simp [Source.readSlot, local_find_sound, found, LocalSlot.denote, term,
            typed, CWord.denote, pure, Except.pure]
        · exact typed

def LocalStore.write {Γ : Var → Nat} : LocalStore Γ → Nat → Source.QualifiedType →
    CWord Γ → Bool → Option (LocalStore Γ)
  | [], _, _, _, _ => none
  | slot :: rest, symbol, destination, value, initialization =>
    if slot.symbol == symbol then
      if slot.type = destination ∧ value.type = destination.type then
        if initialization && slot.value.isSome || !initialization && slot.type.isConst then none
        else some ({slot with value := some (value.cast slot.type.type).term} :: rest)
      else none
    else return slot :: (← LocalStore.write rest symbol destination value initialization)

theorem local_write_sound {Γ : Var → Nat} (σ : Subst Γ)
    (locals updated : LocalStore Γ) (symbol : Nat) (destination : Source.QualifiedType)
    (value : CWord Γ) (initialization : Bool)
    (accepted : locals.write symbol destination value initialization = some updated) :
    Source.writeSlot (locals.denote σ) symbol destination (value.denote σ) initialization =
      .ok (updated.denote σ) := by
  induction locals generalizing updated with
  | nil => simp [LocalStore.write] at accepted
  | cons slot rest ih =>
    simp only [LocalStore.write] at accepted
    split at accepted
    · rename_i id
      split at accepted
      · rename_i same
        split at accepted
        · contradiction
        · rename_i allowed
          cases Option.some.inj accepted
          have castSelf : (value.cast slot.type.type).denote σ = value.denote σ := by
            rw [cast_sound, same.1, ← same.2]
            exact Source.value_cast_self (value.denote σ)
          have represented : Source.Value.mk slot.type.type
              (denoteTerm σ (value.cast slot.type.type).term) = value.denote σ := castSelf
          cases initialization <;>
            simp_all [LocalStore.denote, LocalSlot.denote, Source.writeSlot,
              CWord.denote, pure, Except.pure]
          all_goals exact HEq.symm represented
      · contradiction
    · rename_i other
      cases tail : LocalStore.write rest symbol destination value initialization with
      | none => simp [tail] at accepted
      | some result =>
        simp only [tail, bind, Option.bind, pure, Option.some.injEq] at accepted
        subst updated
        have different : (slot.symbol == symbol) = false := Bool.eq_false_iff.mpr other
        have recursive := ih result tail
        simp only [LocalStore.denote] at recursive
        simp [LocalStore.denote, LocalSlot.denote, Source.writeSlot, different,
          recursive, pure, Except.pure, bind, Except.bind]

private def without {Γ : Var → Nat} (symbol : Nat) : CBindings Γ → CBindings Γ
  | [] => []
  | (key, value) :: rest =>
    if key == symbol then without symbol rest else (key, value) :: without symbol rest

private theorem lookup_without {Γ : Var → Nat} (symbol id : Nat) (env : CBindings Γ) :
    (without symbol env).lookup id = if id == symbol then none else env.lookup id := by
  induction env with
  | nil => simp [without]
  | cons entry rest ih =>
    rcases entry with ⟨key, value⟩
    by_cases remove : key = symbol
    · subst key
      by_cases same : id = symbol
      · subst id; simpa [without, List.lookup_cons] using ih
      · simpa only [without, beq_self_eq_true, ↓reduceIte, List.lookup_cons,
          beq_eq_false_iff_ne.mpr same, Bool.false_eq_true, Bool.true_eq_false] using ih
    · by_cases same : id = symbol
      · subst id
        simpa only [without, beq_eq_false_iff_ne.mpr remove,
          beq_eq_false_iff_ne.mpr (Ne.symm remove), Bool.false_eq_true, ↓reduceIte,
          List.lookup_cons, beq_self_eq_true] using ih
      · simp only [without, beq_eq_false_iff_ne.mpr remove, Bool.false_eq_true,
          ↓reduceIte, List.lookup_cons, ih, beq_eq_false_iff_ne.mpr same]

/-- An uninitialized first binding masks any hidden binding with the same ID.
The source validator rejects duplicate active IDs, but this semantic adapter
does not rely on that unproved validator property. -/
def LocalStore.bindings {Γ : Var → Nat} : LocalStore Γ → CBindings Γ
  | [] => []
  | slot :: rest => match slot.value with
    | some term => (slot.symbol, ⟨slot.type.type, term⟩) :: LocalStore.bindings rest
    | none => without slot.symbol (LocalStore.bindings rest)

theorem local_bindings_read {Γ : Var → Nat} (locals : LocalStore Γ) (id : Nat) (value : CWord Γ)
    (found : locals.bindings.lookup id = some value) : locals.read id value.type = some value := by
  induction locals with
  | nil => simp [LocalStore.bindings] at found
  | cons slot rest ih =>
    cases initialized : slot.value with
    | none =>
      simp only [LocalStore.bindings, initialized, lookup_without] at found
      split at found
      · contradiction
      · rename_i different
        have notId : (slot.symbol == id) = false := by
          have distinct : id ≠ slot.symbol := by simpa using different
          exact beq_eq_false_iff_ne.mpr distinct.symm
        simpa [LocalStore.read, List.find?, notId] using ih found
    | some term =>
      simp only [LocalStore.bindings, initialized, List.lookup] at found
      split at found
      · rename_i same
        cases Option.some.inj found
        have hit : slot.symbol = id := (beq_iff_eq.mp same).symm
        simp [LocalStore.read, List.find?, hit, initialized]
      · rename_i different
        have notId : (slot.symbol == id) = false := by
          have distinct : id ≠ slot.symbol := by simpa using different
          exact beq_eq_false_iff_ne.mpr distinct.symm
        simpa [LocalStore.read, List.find?, notId] using ih found

theorem local_bindings_related {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ) :
    locals.bindings.Related σ (locals.denote σ) := by
  intro id value found
  exact (local_read_sound σ locals id value.type value (local_bindings_read locals id value found)).1

def LocalStore.writable {Γ : Var → Nat} (locals : LocalStore Γ) (symbol : Nat)
    (destination : Source.QualifiedType) : Bool :=
  match locals.find? (·.symbol == symbol) with
  | none => false
  | some slot => slot.type == destination && !slot.type.isConst

theorem local_writable_sound {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ)
    (symbol : Nat) (destination : Source.QualifiedType)
    (accepted : locals.writable symbol destination = true)
    (s : Source.State) (values : s.locals = locals.denote σ) :
    Source.place s symbol destination (.ident destination.type symbol) = true := by
  unfold LocalStore.writable at accepted
  cases found : locals.find? (·.symbol == symbol) with
  | none => simp [found] at accepted
  | some slot =>
    simpa [Source.place, values, local_find_sound, found, LocalSlot.denote] using accepted

def LocalStore.declare {Γ : Var → Nat} (locals : LocalStore Γ) (symbol : Nat)
    (type : Source.QualifiedType) : Option (LocalStore Γ) :=
  if locals.any (·.symbol == symbol) then none else some (⟨symbol, type, none⟩ :: locals)

theorem local_declare_sound {Γ : Var → Nat} (σ : Subst Γ) (locals updated : LocalStore Γ)
    (object : Source.Object) (uninitialized : object.initializer = none)
    (accepted : locals.declare object.symbol object.type = some updated)
    (s : Source.State) (values : s.locals = locals.denote σ) :
    Source.enterObject s object = {s with locals := updated.denote σ, control := .flow .normal} := by
  unfold LocalStore.declare at accepted
  split at accepted
  · contradiction
  · rename_i fresh
    cases Option.some.inj accepted
    have noDuplicate : (locals.denote σ).any (·.symbol == object.symbol) = false := by
      simpa [LocalStore.denote, List.any_map, Function.comp_def, LocalSlot.denote] using
        Bool.eq_false_iff.mpr fresh
    simp only [Source.enterObject, values, noDuplicate, Bool.false_eq_true, ↓reduceIte, uninitialized]
    rfl

def LocalStore.exitScope {Γ : Var → Nat} (locals : LocalStore Γ) (incoming : Nat) :
    Option (LocalStore Γ) :=
  if incoming ≤ locals.length then some (locals.drop (locals.length-incoming)) else none

theorem local_scope_exit_sound {Γ : Var → Nat} (σ : Subst Γ) (locals updated : LocalStore Γ)
    (incoming : Nat) (accepted : locals.exitScope incoming = some updated)
    (s : Source.State) (values : s.locals = locals.denote σ) (flow : Source.Flow) :
    Source.resumeFlow s (.scope incoming) flow =
      {s with locals := updated.denote σ, control := .flow flow} := by
  unfold LocalStore.exitScope at accepted
  split at accepted
  · rename_i length
    cases Option.some.inj accepted
    simp [Source.resumeFlow, values, LocalStore.denote, List.map_drop, Nat.not_lt.mpr length]
  · contradiction

end EvoIR.Core.Symbolic
