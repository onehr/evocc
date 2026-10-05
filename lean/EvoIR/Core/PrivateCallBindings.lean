import EvoIR.Core.PrivateFunctionClosure
import EvoIR.Core.FreshReturn

/-! Fresh symbolic return coordinates retain caller cell bytes and physical
pointers. Only actual returned target values extend the caller token bank. -/
namespace EvoIR.Core.Symbolic

def PrivateCell.forReturn (cell : PrivateCell inputWidths) : PrivateCell inputWidths :=
  {cell with value := cell.value.map (Term.renameInputs returnName)}

def PrivateConfig.forReturn (config : PrivateConfig inputWidths) : PrivateConfig inputWidths :=
  {config with scope := config.scope.forReturn, cells := config.cells.map PrivateCell.forReturn}

def privateCallInputSlots (tokens : Bool) (config : PrivateConfig inputWidths)
    (args : List (CWord inputWidths)) : List (Slot inputWidths) :=
  args.map (fun word => .word word.word) ++
    if tokens then [.opaque config.memory, .opaque config.world] else []

def privateCallOutputTypes (tokens : Bool) (type : Source.IntType) : List IR.Ty :=
  [.bits type.width] ++ if tokens then [.memory, .world] else []

def privateCallOutputSlots (tokens : Bool) (before : PrivateConfig inputWidths)
    (type : Source.IntType) : List (Slot inputWidths) :=
  [.word (inputWord type 0).word] ++
    if tokens then [.opaque (before.tokens.length+1), .opaque (before.tokens.length+2)] else []

def privateCallConfig (tokens : Bool) (before : PrivateConfig inputWidths)
    (scope : Scope inputWidths) (type : Source.IntType) : PrivateConfig inputWidths :=
  ⟨scope, before.tokens ++ privateCallOutputTypes tokens type,
    (if tokens then before.tokens.length+1 else before.memory),
    (if tokens then before.tokens.length+2 else before.world),
    before.cells.map PrivateCell.forReturn⟩

def privateCallValues (tokens : Bool) (value : Source.Value) (memory : IR.Memory) : List IR.Value :=
  [sourceValueIR value] ++ if tokens then [.memory memory, .world] else []

theorem private_cell_for_return_bytes (σ : Subst inputWidths) (value : Source.Value)
    (cell : PrivateCell inputWidths) :
    cell.forReturn.bytes (returnSubst σ value) = cell.bytes σ := by
  cases cell with
  | mk pointer width live contents =>
    cases contents <;>
      simp [PrivateCell.forReturn, PrivateCell.bytes, Term.rename_inputs_sound, return_subst_old] <;> rfl

/-- A callee can add dead private objects, but its return summary cannot hide
or replace any object already present in the caller snapshot. -/
theorem private_closed_lookup (before after : IR.Memory) (id : Nat)
    (closed : before.ClosedExtension after) (old : id < before.nextId) :
    after.objects.find? (·.id == id) = before.objects.find? (·.id == id) := by
  obtain ⟨_, added, objects, fresh⟩ := closed
  have absent : added.find? (·.id == id) = none := by
    apply List.find?_eq_none.mpr
    intro object member
    have bound := (fresh object member).2
    have distinct : object.id ≠ id := by omega
    simpa using distinct
  simp only [objects, List.find?_append, absent, Option.none_or]

theorem private_call_realizes (σ : Subst inputWidths) (values : List IR.Value)
    (incoming beforeMemory returnedMemory : IR.Memory) (callerOwner oldFrame afterFrame : Nat)
    (before : PrivateConfig inputWidths) (nextScope : Scope inputWidths)
    (calleeTokens : Bool) (returnType : Source.IntType) (value : Source.Value)
    (related : before.Realizes σ values incoming callerOwner oldFrame)
    (current : before.memoryValue values = some beforeMemory)
    (typedValue : value.type = returnType)
    (returned : beforeMemory.ClosedExtension returnedMemory)
    (returnedSafe : returnedMemory.WellAllocated)
    (returnedFrame : returnedMemory.BeforeFrame afterFrame)
    (newOwners : ∀ object ∈ returnedMemory.objects, object ∉ beforeMemory.objects →
      oldFrame ≤ object.owner)
    (future : oldFrame < afterFrame)
    (shape : (privateCallConfig calleeTokens before nextScope returnType).WellShaped = true) :
    (privateCallConfig calleeTokens before nextScope returnType).Realizes
      (returnSubst σ value) (values ++ privateCallValues calleeTokens value returnedMemory)
      incoming callerOwner afterFrame := by
  obtain ⟨oldShape, typedBank, oldWorld, incomingSafe, actual, actualCurrent, safe,
    frameBound, ownerBound, privateFrame, distinct, cells⟩ := related
  have actualMemory : actual = beforeMemory := Option.some.inj (actualCurrent.symm.trans current)
  subst actual
  let added := privateCallValues calleeTokens value returnedMemory
  let nextMemory := if calleeTokens then returnedMemory else beforeMemory
  have count := private_values_length typedBank
  have memoryBound : before.memory < values.length := by
    rw [count]
    simp only [PrivateConfig.WellShaped, Bool.and_eq_true] at oldShape
    exact of_decide_eq_true oldShape.1.1.1.1.1.2
  have worldBound : before.world < values.length := by
    rw [count]
    simp only [PrivateConfig.WellShaped, Bool.and_eq_true] at oldShape
    exact of_decide_eq_true oldShape.1.1.1.2
  have memoryToken : privateToken (values ++ added)
      (privateCallConfig calleeTokens before nextScope returnType).memory = .memory nextMemory := by
    cases calleeTokens with
    | false =>
      change privateToken (values ++ added) before.memory = .memory beforeMemory
      rw [private_token_old values added before.memory memoryBound]
      exact before.current_token current
    | true =>
      change privateToken (values ++ added) (before.tokens.length+1) = .memory returnedMemory
      rw [← count, private_token_new]
      rfl
  have worldToken : privateToken (values ++ added)
      (privateCallConfig calleeTokens before nextScope returnType).world = .world := by
    cases calleeTokens with
    | false =>
      change privateToken (values ++ added) before.world = .world
      rw [private_token_old values added before.world worldBound]
      exact oldWorld
    | true =>
      change privateToken (values ++ added) (before.tokens.length+2) = .world
      rw [← count, private_token_new]
      rfl
  have nextSafe : nextMemory.WellAllocated := by
    cases calleeTokens <;> assumption
  have nextFrame : nextMemory.BeforeFrame afterFrame := by
    cases calleeTokens with
    | false => exact fun object member => Nat.lt_trans (frameBound object member) future
    | true => exact returnedFrame
  have nextPrivate : incoming.PrivateFrame callerOwner nextMemory := by
    cases calleeTokens with
    | false => exact privateFrame
    | true =>
      exact IR.Memory.private_after_call privateFrame returned safe
        (fun object member absent => Nat.le_trans (Nat.le_of_lt ownerBound) (newOwners object member absent))
  have oldLookup : ∀ id, id < beforeMemory.nextId →
      nextMemory.objects.find? (·.id == id) = beforeMemory.objects.find? (·.id == id) := by
    intro id bound
    cases calleeTokens with
    | false => rfl
    | true => exact private_closed_lookup beforeMemory returnedMemory id returned bound
  have oldId : ∀ cell ∈ before.cells,
      cell.forReturn.id (values ++ added) = cell.id values := by
    intro cell member
    change cell.id (values ++ added) = cell.id values
    exact private_cell_id_old values added cell (private_cell_pointer_bound oldShape typedBank cell member)
  refine ⟨shape, ?_, worldToken, incomingSafe, nextMemory, ?_, nextSafe, nextFrame,
    Nat.lt_trans ownerBound future, nextPrivate, ?_, ?_⟩
  · cases calleeTokens <;> simp [privateCallConfig, added, privateCallValues, privateCallOutputTypes,
      sourceValueIR, IR.Value.type, typedBank, typedValue]
  · change (match privateToken (values ++ added)
      (privateCallConfig calleeTokens before nextScope returnType).memory with
      | .memory m => some m | _ => none) = some nextMemory
    rw [memoryToken]
  · intro a am b bm different
    obtain ⟨first, fm, rfl⟩ := List.mem_map.mp am
    obtain ⟨second, sm, rfl⟩ := List.mem_map.mp bm
    rw [oldId first fm, oldId second sm]
    exact distinct first fm second sm different
  · intro cell member
    obtain ⟨old, oldMember, rfl⟩ := List.mem_map.mp member
    obtain ⟨pointer, cutoff, object, found, identity, owned, live, bytes, aligned⟩ := cells old oldMember
    have below : old.id values < beforeMemory.nextId := by
      rw [← identity]
      exact (safe.2 object (List.mem_of_find?_eq_some found)).2
    refine ⟨?_, ?_, object, ?_, ?_, owned, live, ?_, aligned⟩
    · rw [oldId old oldMember]
      change privateToken (values ++ added) old.pointer = .pointer (old.id values) 0
      rw [private_token_old values added old.pointer (private_cell_pointer_bound oldShape typedBank old oldMember)]
      exact pointer
    · change incoming.nextId ≤ old.forReturn.id (values ++ added)
      rw [oldId old oldMember]
      exact cutoff
    · rw [oldId old oldMember, oldLookup _ below]
      exact found
    · exact identity.trans (oldId old oldMember).symm
    · rw [private_cell_for_return_bytes]
      exact bytes

theorem private_call_binding (σ : Subst inputWidths) (values : List IR.Value)
    (before : PrivateConfig inputWidths) (nextScope : Scope inputWidths)
    (returnType : Source.IntType) (value : Source.Value) (returnedMemory : IR.Memory)
    (calleeTokens : Bool) (results : List IR.Binding)
    (typedBank : values.map IR.Value.type = before.tokens)
    (typedValue : value.type = returnType)
    (shaped : before.WellShaped = true)
    (binding : before.scope.forReturn.appendPrivate
      (before.tokens ++ privateCallOutputTypes calleeTokens returnType) results
      (privateCallOutputSlots calleeTokens before returnType) = some nextScope) :
    IR.appendResults (before.scope.denote σ (privateToken values)) results
      (privateCallValues calleeTokens value returnedMemory) =
        .ok (nextScope.denote (returnSubst σ value)
          (privateToken (values ++ privateCallValues calleeTokens value returnedMemory))) := by
  let added := privateCallValues calleeTokens value returnedMemory
  have count := private_values_length typedBank
  have types : (values ++ added).map IR.Value.type =
      before.tokens ++ privateCallOutputTypes calleeTokens returnType := by
    cases calleeTokens <;>
      simp [added, privateCallValues, privateCallOutputTypes, sourceValueIR, IR.Value.type, typedBank, typedValue]
  have fresh : (inputWord returnType 0).denote (returnSubst σ value) = value := by
    simpa only [typedValue] using return_subst_fresh σ value
  have outputs : (privateCallOutputSlots calleeTokens before returnType).map
      (Slot.denote (returnSubst σ value) (privateToken (values ++ added))) = added := by
    cases calleeTokens with
    | false =>
      change [sourceValueIR ((inputWord returnType 0).denote (returnSubst σ value))] = [sourceValueIR value]
      rw [fresh]
    | true =>
      have first : privateToken (values ++ added) (before.tokens.length+1) = .memory returnedMemory := by
        rw [← count, private_token_new]; rfl
      have second : privateToken (values ++ added) (before.tokens.length+2) = .world := by
        rw [← count, private_token_new]; rfl
      change sourceValueIR ((inputWord returnType 0).denote (returnSubst σ value)) ::
        privateToken (values ++ added) (before.tokens.length+1) ::
        privateToken (values ++ added) (before.tokens.length+2) :: [] = _
      rw [fresh, first, second]
      rfl
  have appended := append_private_sound (returnSubst σ value) (values ++ added)
    before.scope.forReturn nextScope (before.tokens ++ privateCallOutputTypes calleeTokens returnType)
    results (privateCallOutputSlots calleeTokens before returnType) types binding
  rw [scope_for_return, outputs] at appended
  have bounded : before.scope.inBank values.length = true := by
    simpa only [count] using before.scope_bound shaped
  rw [scope_private_extension σ before.scope values added bounded] at appended
  exact appended

end EvoIR.Core.Symbolic
