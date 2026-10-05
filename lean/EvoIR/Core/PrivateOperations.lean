import EvoIR.Core.PrivateBindings
import EvoIR.Core.SymbolicInstruction

/-! Certified instruction spans for real private-memory operands. Each
transfer extends the concrete token bank with the real evaluator's outputs;
older SSA snapshots remain available and unchanged. -/
namespace EvoIR.Core.Symbolic

def PrivateInstructionRuns {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) (checks : List (Guard Γ)) (after : PrivateConfig Γ) : Prop :=
  ∀ σ values incoming owner frame, before.Realizes σ values incoming owner frame →
    ∃ nextValues,
      concreteInstruction owner (before.scope.denote σ (privateToken values)) inst =
        (do guards σ checks; return after.scope.denote σ (privateToken nextValues)) ∧
      (guards σ checks = .ok () → after.Realizes σ nextValues incoming owner frame)

structure CheckedPrivateInstruction {Γ : Var → Nat}
    (before : PrivateConfig Γ) (inst : IR.Instruction) where
  checks : List (Guard Γ)
  next : PrivateConfig Γ
  sound : PrivateInstructionRuns before inst checks next

theorem private_scalar_instruction {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) (checks : List (Guard Γ)) (nextScope : Scope Γ)
    (accepted : instruction before.scope inst = some (checks, nextScope))
    (shape : ({before with scope := nextScope} : PrivateConfig Γ).WellShaped = true) :
    PrivateInstructionRuns before inst checks {before with scope := nextScope} := by
  intro σ values incoming owner frame related
  refine ⟨values, instruction_sound σ (privateToken values) owner before.scope nextScope inst checks accepted, ?_⟩
  intro _
  rcases related with ⟨_, typed, world, incomingSafe, memory⟩
  exact ⟨shape, typed, world, incomingSafe, memory⟩

theorem private_values_length {Γ : Var → Nat} {before : PrivateConfig Γ}
    {values : List IR.Value} (typed : values.map IR.Value.type = before.tokens) :
    values.length = before.tokens.length := by
  simpa using congrArg List.length typed

theorem private_cell_pointer_bound {Γ : Var → Nat} {before : PrivateConfig Γ}
    {values : List IR.Value} (shape : before.WellShaped = true)
    (typed : values.map IR.Value.type = before.tokens) (cell : PrivateCell Γ)
    (member : cell ∈ before.cells) : cell.pointer < values.length := by
  rw [private_values_length typed]
  exact (before.cell_shape shape cell member).1

theorem private_cell_access {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (incoming m : IR.Memory) (owner : Nat) (cell : PrivateCell Γ)
    (realized : cell.Realizes σ values incoming owner m) (safe : m.WellAllocated)
    (admitted : cell.width = 32 ∨ cell.width = 64) (live : cell.live = true) :
    ∃ o, IR.accessObject m (.pointer (cell.id values) 0) (cell.width/8) (cell.width/8) =
        .ok (o, 0) ∧ o.id = cell.id values ∧ o.owner = owner ∧
      o.bytes = cell.bytes σ ∧ o.live = cell.live ∧ cell.width/8 ≤ o.align ∧
      m.objects.find? (·.id == cell.id values) = some o := by
  rcases realized with ⟨_, _, o, found, identity, owned, active, bytes, aligned⟩
  have member := List.mem_of_find?_eq_some found
  have positive := (safe.2 o member).1
  have sizePositive : cell.width/8 ≠ 0 := by rcases admitted with h | h <;> simp [h]
  have bound : cell.width/8 ≤ o.bytes.length := by
    rw [bytes, private_cell_bytes_length]
    exact Nat.le_refl _
  refine ⟨o, ?_, identity, owned, bytes, active, aligned, found⟩
  rw [← identity]
  exact IR.access_object_zero m o _ _ (Nat.ne_of_gt positive)
    (by simpa only [identity] using found) (active.trans live)
    sizePositive sizePositive bound aligned

theorem private_effect_binding {Γ : Var → Nat} (σ : Subst Γ) (values added : List IR.Value)
    (before : PrivateConfig Γ) (nextScope : Scope Γ) (inst : IR.Instruction)
    (args outputs : List (Slot Γ)) (outputTypes : List IR.Ty) (owner : Nat)
    (typed : values.map IR.Value.type = before.tokens)
    (addedTypes : added.map IR.Value.type = outputTypes)
    (operands : inst.operands.mapM (fun id => before.scope.lookup id) = some args)
    (operation : IR.evalOperation owner inst.op (args.map (Slot.denote σ (privateToken values))) = .ok added)
    (denotedOutputs : outputs.map (Slot.denote σ (privateToken (values ++ added))) = added)
    (binding : before.scope.appendPrivate (before.tokens ++ outputTypes) inst.results outputs = some nextScope)
    (bounded : before.scope.inBank before.tokens.length = true) :
    concreteInstruction owner (before.scope.denote σ (privateToken values)) inst =
      .ok (nextScope.denote σ (privateToken (values ++ added))) := by
  have bankTypes : (values ++ added).map IR.Value.type = before.tokens ++ outputTypes := by
    simp [typed, addedTypes]
  have bound : before.scope.inBank values.length = true := by
    simpa only [private_values_length typed] using bounded
  have appended := append_private_sound σ (values ++ added) before.scope nextScope
    (before.tokens ++ outputTypes) inst.results outputs bankTypes binding
  rw [scope_private_extension σ before.scope values added bound, denotedOutputs] at appended
  simp only [concreteInstruction, slots_lookup_sound σ (privateToken values) before.scope
    inst.operands args operands, Bind.bind, Except.bind, operation, appended]

def privateAllocationConfig {Γ : Var → Nat} (before : PrivateConfig Γ)
    (scope : Scope Γ) (width : Nat) : PrivateConfig Γ :=
  ⟨scope, before.tokens ++ [.pointer, .memory, .world], before.tokens.length+1,
    before.tokens.length+2, ⟨before.tokens.length, width, true, none⟩ :: before.cells⟩

theorem private_allocate_instruction {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) (nextScope : Scope Γ) (origin size align : Nat)
    (op : inst.op = .allocate origin size align)
    (admitted : size = 4 ∨ size = 8) (aligned : size ≤ align)
    (operands : inst.operands.mapM (fun id => before.scope.lookup id) =
      some [.opaque before.memory, .opaque before.world])
    (binding : before.scope.appendPrivate (before.tokens ++ [.pointer, .memory, .world]) inst.results
      [.opaque before.tokens.length, .opaque (before.tokens.length+1), .opaque (before.tokens.length+2)] =
        some nextScope)
    (shape : (privateAllocationConfig before nextScope (8*size)).WellShaped = true) :
    PrivateInstructionRuns before inst [] (privateAllocationConfig before nextScope (8*size)) := by
  intro σ values incoming owner frame related
  rcases related with ⟨oldShape, typed, world, incomingSafe, m, current, safe, beforeFrame,
    ownerFresh, ownedFrame, distinct, cells⟩
  have count := private_values_length typed
  have mv : privateToken values before.memory = .memory m := by
    unfold PrivateConfig.memoryValue at current
    split at current <;> simp_all
  have sizePositive : size ≠ 0 := by rcases admitted with h | h <;> simp [h]
  have alignPositive : align ≠ 0 := by omega
  let fresh := IR.allocatedMemory m owner origin size align
  let added : List IR.Value := [.pointer m.nextId 0, .memory fresh, .world]
  let bank := values ++ added
  let cell : PrivateCell Γ := ⟨before.tokens.length, 8*size, true, none⟩
  have new0 : privateToken bank before.tokens.length = .pointer m.nextId 0 := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 0
  have new1 : privateToken bank (before.tokens.length+1) = .memory fresh := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 1
  have new2 : privateToken bank (before.tokens.length+2) = .world := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 2
  have freshId : cell.id bank = m.nextId := by simp [cell, PrivateCell.id, new0]
  have oldId : ∀ old ∈ before.cells, old.id bank = old.id values := by
    intro old member
    exact private_cell_id_old values added old (private_cell_pointer_bound oldShape typed old member)
  have oldPointer : ∀ old ∈ before.cells,
      privateToken bank old.pointer = privateToken values old.pointer := by
    intro old member
    exact private_token_old values added old.pointer (private_cell_pointer_bound oldShape typed old member)
  have oldBound : ∀ old ∈ before.cells, old.id values < m.nextId := by
    intro old member
    obtain ⟨_, _, o, found, same, _⟩ := cells old member
    simpa only [same] using (safe.2 o (List.mem_of_find?_eq_some found)).2
  refine ⟨bank, ?_, ?_⟩
  · apply private_effect_binding σ values added before nextScope inst
      [.opaque before.memory, .opaque before.world]
      [.opaque before.tokens.length, .opaque (before.tokens.length+1), .opaque (before.tokens.length+2)]
      [.pointer, .memory, .world] owner typed rfl operands
      (by simpa only [op, List.map_cons, List.map_nil, Slot.denote, mv, world]
        using IR.allocate_memory m owner origin size align safe sizePositive alignPositive)
      (by
        change [privateToken bank before.tokens.length, privateToken bank (before.tokens.length+1),
          privateToken bank (before.tokens.length+2)] = added
        rw [new0, new1, new2]) binding (before.scope_bound oldShape)
  · intro _
    refine ⟨shape, by simp [privateAllocationConfig, bank, added, typed, IR.Value.type], new2, incomingSafe,
      fresh, ?_, m.allocated_safe owner origin size align safe,
      m.allocated_before owner frame origin size align beforeFrame ownerFresh, ownerFresh,
      IR.Memory.allocated_private ownedFrame origin size align, ?_, ?_⟩
    · simp [privateAllocationConfig, PrivateConfig.memoryValue, new1]
    · change ∀ a ∈ cell :: before.cells, ∀ b ∈ cell :: before.cells,
        a.pointer ≠ b.pointer → a.id bank ≠ b.id bank
      intro a am b bm different
      rcases List.mem_cons.mp am with rfl | am
      · rcases List.mem_cons.mp bm with rfl | bm
        · exact False.elim (different rfl)
        · rw [freshId, oldId b bm]
          exact Nat.ne_of_gt (oldBound b bm)
      · rcases List.mem_cons.mp bm with rfl | bm
        · rw [oldId a am, freshId]
          exact Nat.ne_of_lt (oldBound a am)
        · rw [oldId a am, oldId b bm]
          exact distinct a am b bm different
    · change ∀ old ∈ cell :: before.cells, old.Realizes σ bank incoming owner fresh
      intro old member
      rcases List.mem_cons.mp member with rfl | member
      · unfold PrivateCell.Realizes
        rw [freshId]
        refine ⟨new0, ownedFrame.1, ⟨m.nextId, origin, owner, align, true, List.replicate size none⟩,
          ?_, rfl, rfl, rfl, ?_, ?_⟩
        · exact m.find_allocated_new owner origin size align
        · simp [cell, PrivateCell.bytes]
        · simpa [cell] using aligned
      · rcases cells old member with ⟨pointer, cutoff, o, found, identity, owned, live, bytes, alignment⟩
        refine ⟨?_, ?_, o, ?_, ?_, owned, live, bytes, alignment⟩
        · simpa only [oldPointer old member, oldId old member] using pointer
        · simpa only [oldId old member] using cutoff
        · rw [oldId old member, m.find_allocated_old owner origin size align _ (oldBound old member)]
          exact found
        · simpa only [oldId old member] using identity

def privateMutationCells {Γ : Var → Nat} (before : PrivateConfig Γ)
    (selected updated : PrivateCell Γ) : List (PrivateCell Γ) :=
  before.cells.map fun old => if old.pointer == selected.pointer then updated else old

def privateMutationConfig {Γ : Var → Nat} (before : PrivateConfig Γ)
    (scope : Scope Γ) (selected updated : PrivateCell Γ) : PrivateConfig Γ :=
  ⟨scope, before.tokens ++ [.memory, .world], before.tokens.length,
    before.tokens.length+1, privateMutationCells before selected updated⟩

theorem private_mutation_realizes {Γ : Var → Nat} (σ : Subst Γ)
    (values : List IR.Value) (incoming m : IR.Memory) (owner frame : Nat)
    (before : PrivateConfig Γ) (nextScope : Scope Γ) (selected updated : PrivateCell Γ)
    (replacement : IR.Object)
    (related : before.Realizes σ values incoming owner frame)
    (current : before.memoryValue values = some m)
    (selectedMember : selected ∈ before.cells)
    (samePointer : updated.pointer = selected.pointer)
    (identity : replacement.id = selected.id values)
    (owned : replacement.owner = owner) (live : replacement.live = updated.live)
    (bytes : replacement.bytes = updated.bytes σ) (alignment : updated.width/8 ≤ replacement.align)
    (shape : (privateMutationConfig before nextScope selected updated).WellShaped = true) :
    (privateMutationConfig before nextScope selected updated).Realizes σ
      (values ++ [.memory (m.replace replacement), .world]) incoming owner frame := by
  rcases related with ⟨oldShape, typed, world, incomingSafe, actual, actualMemory, safe,
    frameBound, ownerBound, ownedFrame, distinct, cells⟩
  have sameMemory : actual = m := Option.some.inj (actualMemory.symm.trans current)
  subst actual
  let added : List IR.Value := [.memory (m.replace replacement), .world]
  let bank := values ++ added
  have count := private_values_length typed
  have new0 : privateToken bank before.tokens.length = .memory (m.replace replacement) := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 0
  have new1 : privateToken bank (before.tokens.length+1) = .world := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 1
  have oldId : ∀ old ∈ before.cells, old.id bank = old.id values := by
    intro old member
    exact private_cell_id_old values added old (private_cell_pointer_bound oldShape typed old member)
  have oldPointer : ∀ old ∈ before.cells,
      privateToken bank old.pointer = privateToken values old.pointer := by
    intro old member
    exact private_token_old values added old.pointer (private_cell_pointer_bound oldShape typed old member)
  have changedPointer : ∀ old ∈ before.cells,
      (if old.pointer == selected.pointer then updated else old).pointer = old.pointer := by
    intro old _
    split
    · rename_i same
      exact samePointer.trans (beq_iff_eq.mp same).symm
    · rfl
  have changedId : ∀ old ∈ before.cells,
      (if old.pointer == selected.pointer then updated else old).id bank = old.id values := by
    intro old member
    rw [private_cell_id_pointer bank _ old (changedPointer old member), oldId old member]
  rcases cells selected selectedMember with ⟨selectedPointer, cutoff, original, found, originalId, _⟩
  have positive : 0 < replacement.id := by
    rw [identity, ← originalId]
    exact (safe.2 original (List.mem_of_find?_eq_some found)).1
  have bound : replacement.id < m.nextId := by
    rw [identity, ← originalId]
    exact (safe.2 original (List.mem_of_find?_eq_some found)).2
  refine ⟨shape, by simp [privateMutationConfig, List.map_append, typed, IR.Value.type], new1, incomingSafe,
    m.replace replacement, ?_, m.replace_safe replacement safe positive bound,
    m.replace_before replacement frame frameBound (owned ▸ ownerBound), ownerBound,
    IR.Memory.replace_private replacement ownedFrame incomingSafe (by simpa only [identity] using cutoff) owned, ?_, ?_⟩
  · change (match privateToken bank before.tokens.length with
      | .memory snapshot => some snapshot | _ => none) = some (m.replace replacement)
    rw [new0]
  · change ∀ a ∈ privateMutationCells before selected updated,
      ∀ b ∈ privateMutationCells before selected updated,
      a.pointer ≠ b.pointer → a.id bank ≠ b.id bank
    intro a am b bm different
    unfold privateMutationCells at am bm
    obtain ⟨oldA, oldAMember, rfl⟩ := List.mem_map.mp am
    obtain ⟨oldB, oldBMember, rfl⟩ := List.mem_map.mp bm
    rw [changedId oldA oldAMember, changedId oldB oldBMember]
    apply distinct oldA oldAMember oldB oldBMember
    simpa only [changedPointer oldA oldAMember, changedPointer oldB oldBMember] using different
  · change ∀ cell ∈ privateMutationCells before selected updated,
      cell.Realizes σ bank incoming owner (m.replace replacement)
    intro cell member
    unfold privateMutationCells at member
    obtain ⟨old, oldMember, rfl⟩ := List.mem_map.mp member
    split
    · rename_i same
      have updatedId : updated.id bank = selected.id values := by
        rw [private_cell_id_pointer bank updated selected samePointer, oldId selected selectedMember]
      refine ⟨?_, by simpa only [updatedId] using cutoff, replacement, ?_, ?_, owned, live, bytes, alignment⟩
      · rw [samePointer, oldPointer selected selectedMember, updatedId]
        exact selectedPointer
      · rw [updatedId, m.find_replace replacement (selected.id values), found]
        simp [originalId, identity]
      · exact identity.trans updatedId.symm
    · rename_i other
      have unequalKeys : old.pointer ≠ selected.pointer := by simpa using other
      have unequalIds := distinct old oldMember selected selectedMember unequalKeys
      rcases cells old oldMember with ⟨pointer, oldCutoff, o, oldFound, oldIdentity,
        oldOwned, oldLive, oldBytes, oldAlignment⟩
      refine ⟨?_, ?_, o, ?_, ?_, oldOwned, oldLive, oldBytes, oldAlignment⟩
      · simpa only [oldPointer old oldMember, oldId old oldMember] using pointer
      · simpa only [oldId old oldMember] using oldCutoff
      · rw [oldId old oldMember, m.find_replace replacement (old.id values), oldFound]
        simp only [oldIdentity, identity, beq_eq_false_iff_ne.mpr unequalIds, Bool.false_eq_true,
          ↓reduceIte, Option.map_some]
      · simpa only [oldId old oldMember] using oldIdentity

theorem private_store_instruction {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) (nextScope : Scope Γ) (cell : PrivateCell Γ)
    (value : Term Γ cell.width)
    (op : inst.op = .store) (member : cell ∈ before.cells) (active : cell.live = true)
    (operands : inst.operands.mapM (fun id => before.scope.lookup id) =
      some [.opaque before.memory, .opaque before.world, .opaque cell.pointer, .word ⟨cell.width, value⟩])
    (binding : before.scope.appendPrivate (before.tokens ++ [.memory, .world]) inst.results
      [.opaque before.tokens.length, .opaque (before.tokens.length+1)] = some nextScope)
    (shape : (privateMutationConfig before nextScope cell {cell with value := some value}).WellShaped = true) :
    PrivateInstructionRuns before inst []
      (privateMutationConfig before nextScope cell {cell with value := some value}) := by
  intro σ values incoming owner frame related
  have retainedRelated := related
  obtain ⟨oldShape, typed, world, incomingSafe, m, current, safe, frameBound, ownerBound,
    ownedFrame, distinct, cells⟩ := related
  have admitted := (before.cell_shape oldShape cell member).2
  obtain ⟨o, access, identity, owned, bytes, live, alignment, found⟩ :=
    private_cell_access σ values incoming m owner cell (cells cell member) safe admitted active
  have length : o.bytes.length = cell.width/8 := by rw [bytes, private_cell_bytes_length]
  let replacement := {o with bytes := IR.scalarBytes cell.width (denoteTerm σ value)}
  let added : List IR.Value := [.memory (m.replace replacement), .world]
  let bank := values ++ added
  have count := private_values_length typed
  have new0 : privateToken bank before.tokens.length = .memory (m.replace replacement) := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 0
  have new1 : privateToken bank (before.tokens.length+1) = .world := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 1
  have pointer := (cells cell member).1
  refine ⟨bank, ?_, fun _ => ?_⟩
  · apply private_effect_binding σ values added before nextScope inst
      [.opaque before.memory, .opaque before.world, .opaque cell.pointer, .word ⟨cell.width, value⟩]
      [.opaque before.tokens.length, .opaque (before.tokens.length+1)]
      [.memory, .world] owner typed rfl operands
      (by
        have stored := IR.store_scalar_bytes owner m o cell.width (denoteTerm σ value) admitted
          (by simpa only [← identity] using access) length
        simpa only [op, List.map_cons, List.map_nil, Slot.denote,
          before.current_token current, world, pointer, Word.denote, ← identity, added, replacement]
          using stored)
      (by
        change [privateToken bank before.tokens.length, privateToken bank (before.tokens.length+1)] = added
        rw [new0, new1]) binding (before.scope_bound oldShape)
  · apply private_mutation_realizes σ values incoming m owner frame before nextScope cell
      {cell with value := some value} replacement retainedRelated current member rfl identity owned
      (by simpa [replacement] using live)
      (by simp [replacement, PrivateCell.bytes])
      (by simpa [replacement] using alignment)
      shape

theorem private_lifetime_instruction {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) (nextScope : Scope Γ) (cell : PrivateCell Γ)
    (op : inst.op = .endLifetime) (member : cell ∈ before.cells) (active : cell.live = true)
    (operands : inst.operands.mapM (fun id => before.scope.lookup id) =
      some [.opaque before.memory, .opaque before.world, .opaque cell.pointer])
    (binding : before.scope.appendPrivate (before.tokens ++ [.memory, .world]) inst.results
      [.opaque before.tokens.length, .opaque (before.tokens.length+1)] = some nextScope)
    (shape : (privateMutationConfig before nextScope cell {cell with live := false}).WellShaped = true) :
    PrivateInstructionRuns before inst [] (privateMutationConfig before nextScope cell {cell with live := false}) := by
  intro σ values incoming owner frame related
  have retainedRelated := related
  obtain ⟨oldShape, typed, world, incomingSafe, m, current, safe, frameBound, ownerBound,
    ownedFrame, distinct, cells⟩ := related
  have admitted := (before.cell_shape oldShape cell member).2
  obtain ⟨o, access, identity, owned, bytes, live, alignment, found⟩ :=
    private_cell_access σ values incoming m owner cell (cells cell member) safe admitted active
  let replacement := {o with live := false}
  let added : List IR.Value := [.memory (m.replace replacement), .world]
  let bank := values ++ added
  have count := private_values_length typed
  have new0 : privateToken bank before.tokens.length = .memory (m.replace replacement) := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 0
  have new1 : privateToken bank (before.tokens.length+1) = .world := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 1
  have pointer := (cells cell member).1
  have positive := (safe.2 o (List.mem_of_find?_eq_some found)).1
  have nonzero : cell.id values ≠ 0 := by simpa only [← identity] using Nat.ne_of_gt positive
  have operation : IR.evalOperation owner .endLifetime [.memory m, .world, .pointer o.id 0] = .ok added := by
    simp [IR.evalOperation, IR.memory, IR.world, IR.pointer, IR.liveObject,
      Nat.ne_of_gt positive, identity, nonzero, found, live, active, owned, added, replacement,
      Bind.bind, Except.bind, pure, Except.pure]
  refine ⟨bank, ?_, fun _ => ?_⟩
  · apply private_effect_binding σ values added before nextScope inst
      [.opaque before.memory, .opaque before.world, .opaque cell.pointer]
      [.opaque before.tokens.length, .opaque (before.tokens.length+1)]
      [.memory, .world] owner typed rfl operands
      (by simpa only [op, List.map_cons, List.map_nil, Slot.denote,
          before.current_token current, world, pointer, ← identity] using operation)
      (by
        change [privateToken bank before.tokens.length, privateToken bank (before.tokens.length+1)] = added
        rw [new0, new1]) binding (before.scope_bound oldShape)
  · apply private_mutation_realizes σ values incoming m owner frame before nextScope cell
      {cell with live := false} replacement retainedRelated current member rfl identity owned rfl
      (by
        change o.bytes = ({cell with live := false} : PrivateCell Γ).bytes σ
        rw [private_cell_bytes_live]
        exact bytes)
      (by simpa [replacement] using alignment) shape

def privatePreservedConfig {Γ : Var → Nat} (before : PrivateConfig Γ) (scope : Scope Γ)
    (addedTypes : List IR.Ty) (memory world : Nat) : PrivateConfig Γ :=
  ⟨scope, before.tokens ++ addedTypes, memory, world, before.cells⟩

theorem private_preserved_realizes {Γ : Var → Nat} (σ : Subst Γ) (values added : List IR.Value)
    (incoming m : IR.Memory) (owner frame : Nat) (before : PrivateConfig Γ) (scope : Scope Γ)
    (addedTypes : List IR.Ty) (memory world : Nat)
    (related : before.Realizes σ values incoming owner frame)
    (current : before.memoryValue values = some m)
    (typedAdded : added.map IR.Value.type = addedTypes)
    (nextMemory : privateToken (values ++ added) memory = .memory m)
    (nextWorld : privateToken (values ++ added) world = .world)
    (shape : (privatePreservedConfig before scope addedTypes memory world).WellShaped = true) :
    (privatePreservedConfig before scope addedTypes memory world).Realizes σ (values ++ added)
      incoming owner frame := by
  rcases related with ⟨oldShape, typed, _, incomingSafe, actual, actualCurrent, safe, frameBound,
    ownerBound, ownedFrame, distinct, cells⟩
  have sameMemory : actual = m := Option.some.inj (actualCurrent.symm.trans current)
  subst actual
  have oldId : ∀ cell ∈ before.cells, cell.id (values ++ added) = cell.id values := by
    intro cell member
    exact private_cell_id_old values added cell (private_cell_pointer_bound oldShape typed cell member)
  have oldPointer : ∀ cell ∈ before.cells,
      privateToken (values ++ added) cell.pointer = privateToken values cell.pointer := by
    intro cell member
    exact private_token_old values added cell.pointer (private_cell_pointer_bound oldShape typed cell member)
  refine ⟨shape, by simp [privatePreservedConfig, typed, typedAdded], nextWorld, incomingSafe,
    m, ?_, safe, frameBound, ownerBound, ownedFrame, ?_, ?_⟩
  · simp only [PrivateConfig.memoryValue, privatePreservedConfig, nextMemory]
  · intro a am b bm different
    rw [oldId a am, oldId b bm]
    exact distinct a am b bm different
  · intro cell member
    unfold PrivateCell.Realizes
    rw [oldPointer cell member, oldId cell member]
    exact cells cell member

theorem private_poison_instruction {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) (nextScope : Scope Γ) (width : Nat)
    (op : inst.op = .uninitialized (.bits width)) (operands : inst.operands = [])
    (binding : before.scope.appendPrivate (before.tokens ++ [.bits width]) inst.results
      [.opaque before.tokens.length] = some nextScope)
    (shape : (privatePreservedConfig before nextScope [.bits width] before.memory before.world).WellShaped = true) :
    PrivateInstructionRuns before inst []
      (privatePreservedConfig before nextScope [.bits width] before.memory before.world) := by
  intro σ values incoming owner frame related
  have retainedRelated := related
  rcases related with ⟨oldShape, typed, world, incomingSafe, m, current, safe, frameBound,
    ownerBound, ownedFrame, distinct, cells⟩
  let added : List IR.Value := [.poison (.bits width)]
  have count := private_values_length typed
  have new0 : privateToken (values ++ added) before.tokens.length = .poison (.bits width) := by
    rw [← count]
    simpa [added, privateToken] using private_token_new values added 0
  have memoryBound : before.memory < values.length := by
    rw [count]
    simp only [PrivateConfig.WellShaped, Bool.and_eq_true] at oldShape
    exact of_decide_eq_true oldShape.1.1.1.1.1.2
  have worldBound : before.world < values.length := by
    rw [count]
    simp only [PrivateConfig.WellShaped, Bool.and_eq_true] at oldShape
    exact of_decide_eq_true oldShape.1.1.1.2
  have nextMemory : privateToken (values ++ added) before.memory = .memory m := by
    rw [private_token_old values added before.memory memoryBound]
    exact before.current_token current
  have nextWorld : privateToken (values ++ added) before.world = .world := by
    rw [private_token_old values added before.world worldBound, world]
  refine ⟨values ++ added, ?_, fun _ => ?_⟩
  · apply private_effect_binding σ values added before nextScope inst [] [.opaque before.tokens.length]
      [.bits width] owner typed rfl (by simp [operands])
      (by simp [op, IR.evalOperation, added, pure, Except.pure])
      (by simp only [List.map_cons, List.map_nil, Slot.denote, new0]; rfl)
      binding (before.scope_bound oldShape)
  · exact private_preserved_realizes σ values added incoming m owner frame before nextScope
      [.bits width] before.memory before.world retainedRelated current rfl nextMemory nextWorld shape

end EvoIR.Core.Symbolic
