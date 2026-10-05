import EvoIR.Core.PrivateRebind

/-! Executable span-bank rebinding. Static normalized expressions may change
names, but memory snapshots and physical pointer values are transported from
the actual incoming bank. No runtime value is chosen by the checker. -/
namespace EvoIR.Core.Symbolic

def privateNormalizeSlot {Γ : Var → Nat} : Slot Γ → Slot Γ
  | .word ⟨width, term⟩ => .word ⟨width, term.normalizeValue⟩
  | .opaque key => .opaque key

def privateNormalizeScope {Γ : Var → Nat} (scope : Scope Γ) : Scope Γ :=
  scope.map (fun (id, slot) => (id, privateNormalizeSlot slot))

def privateNormalizeCell {Γ : Var → Nat} (cell : PrivateCell Γ) : PrivateCell Γ :=
  {cell with value := cell.value.map Term.normalizeValue}

theorem private_normalize_slot_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (slot : Slot Γ) : (privateNormalizeSlot slot).denote σ ρ = slot.denote σ ρ := by
  cases slot with
  | word word => cases word; simp [privateNormalizeSlot, Slot.denote, Word.denote, Term.normalize_value_sound]
  | «opaque» key => rfl

theorem private_normalize_scope_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (scope : Scope Γ) : (privateNormalizeScope scope).denote σ ρ = scope.denote σ ρ := by
  simp only [privateNormalizeScope, Scope.denote, List.map_map]
  apply List.map_congr_left
  intro entry _
  exact congrArg (Prod.mk entry.1) (private_normalize_slot_sound σ ρ entry.2)

theorem private_normalize_cell_bytes {Γ : Var → Nat} (σ : Subst Γ) (cell : PrivateCell Γ) :
    (privateNormalizeCell cell).bytes σ = cell.bytes σ := by
  cases cell with
  | mk pointer width live value =>
    cases value <;> simp [privateNormalizeCell, PrivateCell.bytes, Term.normalize_value_sound]

theorem private_cell_bytes_pointer {Γ : Var → Nat} (σ : Subst Γ) (cell : PrivateCell Γ) (pointer : Nat) :
    ({cell with pointer} : PrivateCell Γ).bytes σ = cell.bytes σ := by
  cases cell with
  | mk old width live value => cases value <;> rfl

theorem private_bank_lookup {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (bank : List (Slot Γ)) (key : Nat) (slot : Slot Γ) (found : bank[key]? = some slot) :
    privateToken (bank.map (Slot.denote σ (privateToken values))) key = slot.denote σ (privateToken values) := by
  simp [privateToken, List.getElem?_map, found]

def privateRenameScope {Γ : Var → Nat} (bank : List (Slot Γ)) : Scope Γ → Option (Scope Γ)
  | [] => some []
  | (id, slot) :: rest => do
    let newSlot ← match slot with
      | .word word => some (.word word)
      | .opaque key => bank[key]?
    let tail ← privateRenameScope bank rest
    return (id, newSlot) :: tail

theorem private_rename_scope_sound {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (bank : List (Slot Γ)) (scope renamed : Scope Γ)
    (accepted : privateRenameScope bank scope = some renamed) :
    scope.denote σ (privateToken (bank.map (Slot.denote σ (privateToken values)))) =
      renamed.denote σ (privateToken values) := by
  induction scope generalizing renamed with
  | nil => simp [privateRenameScope] at accepted; subst renamed; rfl
  | cons entry rest ih =>
    rcases entry with ⟨id, slot⟩
    cases slot with
    | word word =>
      cases tail : privateRenameScope bank rest with
      | none => simp [privateRenameScope, tail] at accepted
      | some next =>
        simp only [privateRenameScope, tail, Bind.bind, Option.bind, pure, Option.some.injEq] at accepted
        subst renamed
        exact congrArg (List.cons (id, word.denote σ)) (ih next tail)
    | «opaque» key =>
      cases found : bank[key]? with
      | none => simp [privateRenameScope, found] at accepted
      | some value =>
        cases tail : privateRenameScope bank rest with
        | none => simp [privateRenameScope, found, tail] at accepted
        | some next =>
          simp only [privateRenameScope, found, tail, Bind.bind, Option.bind, pure, Option.some.injEq] at accepted
          subst renamed
          change (id, privateToken (bank.map (Slot.denote σ (privateToken values))) key) ::
              Scope.denote σ (privateToken (bank.map (Slot.denote σ (privateToken values)))) rest =
            (id, value.denote σ (privateToken values)) :: next.denote σ (privateToken values)
          rw [private_bank_lookup σ values bank key value found, ih next tail]

def privateCellRoute {Γ : Var → Nat} (before : PrivateConfig Γ) (bank : List (Slot Γ))
    (cell : PrivateCell Γ) : PrivateCell Γ :=
  match bank[cell.pointer]? with
  | some (.opaque key) => (before.cells.find? (·.pointer == key)).getD cell
  | _ => cell

def PrivateRouteChecked {Γ : Var → Nat} (before : PrivateConfig Γ) (bank : List (Slot Γ))
    (cell : PrivateCell Γ) : Prop :=
  privateCellRoute before bank cell ∈ before.cells ∧
    bank[cell.pointer]? = some (.opaque (privateCellRoute before bank cell).pointer) ∧
    privateNormalizeCell {cell with pointer := (privateCellRoute before bank cell).pointer} =
      privateNormalizeCell (privateCellRoute before bank cell)

instance {Γ : Var → Nat} (before : PrivateConfig Γ) (bank : List (Slot Γ)) (cell : PrivateCell Γ) :
    Decidable (PrivateRouteChecked before bank cell) := by
  unfold PrivateRouteChecked
  infer_instance

def privateRoutes {Γ : Var → Nat} (before : PrivateConfig Γ) (bank : List (Slot Γ)) :
    (cells : List (PrivateCell Γ)) → Option (PLift (∀ cell ∈ cells, PrivateRouteChecked before bank cell))
  | [] => some ⟨by intro cell member; cases member⟩
  | cell :: rest => do
    if checked : PrivateRouteChecked before bank cell then
      let tail ← privateRoutes before bank rest
      return ⟨by
        intro selected member
        rcases List.mem_cons.mp member with rfl | member
        · exact checked
        · exact tail.down selected member⟩
    else none

set_option maxHeartbeats 1000000 in
def privateRebindingCheck {Γ : Var → Nat} (before after : PrivateConfig Γ) (bank : List (Slot Γ)) :
    Option (PLift (∀ σ values,
      values.map IR.Value.type = before.tokens →
      privateToken values before.world = .world →
      Nonempty (PrivateRebinding σ σ values (bank.map (Slot.denote σ (privateToken values))) before after))) := do
  if shape : after.WellShaped = true then
    if types : bank.map (Slot.type (privateTokenType before.tokens)) = after.tokens then
      if memory : bank[after.memory]? = some (.opaque before.memory) then
        if world : bank[after.world]? = some (.opaque before.world) then
          match renamed : privateRenameScope bank after.scope with
          | none => none
          | some scope =>
            if same : privateNormalizeScope scope = privateNormalizeScope before.scope then
              let routes ← privateRoutes before bank after.cells
              if injective : ∀ a ∈ after.cells, ∀ b ∈ after.cells, a.pointer ≠ b.pointer →
                  (privateCellRoute before bank a).pointer ≠ (privateCellRoute before bank b).pointer then
                return ⟨by
                  intro σ values typed actualWorld
                  refine ⟨{
                    shape, typed := ?_, world := ?_, memory := ?_, scope := ?_,
                    route := privateCellRoute before bank,
                    member := fun cell member => (routes.down cell member).1,
                    injective, pointer := ?_, width := ?_, live := ?_, bytes := ?_ }⟩
                  · rw [← types, List.map_map]
                    apply List.map_congr_left
                    intro slot _
                    exact slot_type_sound σ (privateToken values) (privateTokenType before.tokens)
                      (private_token_typed values before.tokens typed) slot
                  · rw [private_bank_lookup σ values bank after.world _ world]
                    exact actualWorld
                  · unfold PrivateConfig.memoryValue
                    rw [private_bank_lookup σ values bank after.memory _ memory]
                    rfl
                  · have scopeEqual := congrArg (Scope.denote σ (privateToken values)) same
                    rw [private_normalize_scope_sound, private_normalize_scope_sound] at scopeEqual
                    exact (private_rename_scope_sound σ values bank after.scope scope renamed).trans scopeEqual
                  · intro cell member
                    exact private_bank_lookup σ values bank cell.pointer _ (routes.down cell member).2.1
                  · intro cell member
                    have widths := congrArg (fun c : PrivateCell Γ => c.width) (routes.down cell member).2.2
                    exact widths
                  · intro cell member
                    have lives := congrArg (fun c : PrivateCell Γ => c.live) (routes.down cell member).2.2
                    exact lives
                  · intro cell member
                    have bytes := congrArg (PrivateCell.bytes σ) (routes.down cell member).2.2
                    simpa only [private_normalize_cell_bytes, private_cell_bytes_pointer] using bytes⟩
              else none
            else none
        else none
      else none
    else none
  else none

end EvoIR.Core.Symbolic
