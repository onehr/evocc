import EvoIR.Core.PrivateOperations

/-! Read-only private-memory instructions preserve the complete incoming
snapshot and the private cell footprint. The concrete token bank receives the
actual evaluator outputs; an initialized load also exposes the stored symbolic
word. No uninitialized byte is admitted as an arbitrary symbolic value. -/
namespace EvoIR.Core.Symbolic

def privateLoadConfig {Γ : Var → Nat} (before : PrivateConfig Γ)
    (scope : Scope Γ) (width : Nat) : PrivateConfig Γ :=
  privatePreservedConfig before scope [.bits width, .memory, .world]
    (before.tokens.length+1) (before.tokens.length+2)

theorem private_load_instruction {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) (nextScope : Scope Γ) (cell : PrivateCell Γ)
    (value : Term Γ cell.width)
    (op : inst.op = .load cell.width) (member : cell ∈ before.cells)
    (active : cell.live = true) (initialized : cell.value = some value)
    (operands : inst.operands.mapM (fun id => before.scope.lookup id) =
      some [.opaque before.memory, .opaque before.world, .opaque cell.pointer])
    (binding : before.scope.appendPrivate
      (before.tokens ++ [.bits cell.width, .memory, .world]) inst.results
      [.word ⟨cell.width, value⟩, .opaque (before.tokens.length+1),
        .opaque (before.tokens.length+2)] = some nextScope)
    (shape : (privateLoadConfig before nextScope cell.width).WellShaped = true) :
    PrivateInstructionRuns before inst [] (privateLoadConfig before nextScope cell.width) := by
  intro σ values incoming owner frame related
  have retainedRelated := related
  obtain ⟨oldShape, typed, world, incomingSafe, m, current, safe, frameBound,
    ownerBound, ownedFrame, distinct, cells⟩ := related
  have admitted := (before.cell_shape oldShape cell member).2
  obtain ⟨o, access, identity, owned, bytes, live, alignment, found⟩ :=
    private_cell_access σ values incoming m owner cell (cells cell member) safe admitted active
  have exactBytes : o.bytes = IR.scalarBytes cell.width (denoteTerm σ value) := by
    simpa only [PrivateCell.bytes, initialized] using bytes
  let added : List IR.Value := [.bits cell.width (denoteTerm σ value), .memory m, .world]
  let bank := values ++ added
  have count := private_values_length typed
  have new1 : privateToken bank (before.tokens.length+1) = .memory m := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 1
  have new2 : privateToken bank (before.tokens.length+2) = .world := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 2
  have pointer := (cells cell member).1
  refine ⟨bank, ?_, fun _ => ?_⟩
  · apply private_effect_binding σ values added before nextScope inst
      [.opaque before.memory, .opaque before.world, .opaque cell.pointer]
      [.word ⟨cell.width, value⟩, .opaque (before.tokens.length+1),
        .opaque (before.tokens.length+2)]
      [.bits cell.width, .memory, .world] owner typed rfl operands
      (by
        have loaded := IR.load_scalar_bytes owner m o cell.width (denoteTerm σ value)
          admitted (by simpa only [← identity] using access) exactBytes
        simpa only [op, List.map_cons, List.map_nil, Slot.denote,
          before.current_token current, world, pointer, ← identity, added] using loaded)
      (by
        change [.bits cell.width (denoteTerm σ value),
          privateToken bank (before.tokens.length+1), privateToken bank (before.tokens.length+2)] = added
        rw [new1, new2]) binding (before.scope_bound oldShape)
  · exact private_preserved_realizes σ values added incoming m owner frame before nextScope
      [.bits cell.width, .memory, .world] (before.tokens.length+1) (before.tokens.length+2)
      retainedRelated current rfl new1 new2 shape

def privateAccessConfig {Γ : Var → Nat} (before : PrivateConfig Γ)
    (scope : Scope Γ) : PrivateConfig Γ :=
  privatePreservedConfig before scope [.memory] before.tokens.length before.world

theorem private_access_instruction {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) (nextScope : Scope Γ) (cell : PrivateCell Γ)
    (size align : Nat) (op : inst.op = .access size align)
    (member : cell ∈ before.cells) (active : cell.live = true)
    (sizePositive : size ≠ 0) (alignPositive : align ≠ 0)
    (sizeBound : size ≤ cell.width/8) (alignBound : align ≤ cell.width/8)
    (operands : inst.operands.mapM (fun id => before.scope.lookup id) =
      some [.opaque before.memory, .opaque cell.pointer])
    (binding : before.scope.appendPrivate (before.tokens ++ [.memory]) inst.results
      [.opaque before.tokens.length] = some nextScope)
    (shape : (privateAccessConfig before nextScope).WellShaped = true) :
    PrivateInstructionRuns before inst [] (privateAccessConfig before nextScope) := by
  intro σ values incoming owner frame related
  have retainedRelated := related
  obtain ⟨oldShape, typed, world, incomingSafe, m, current, safe, frameBound,
    ownerBound, ownedFrame, distinct, cells⟩ := related
  have admitted := (before.cell_shape oldShape cell member).2
  obtain ⟨o, access, identity, owned, bytes, live, alignment, found⟩ :=
    private_cell_access σ values incoming m owner cell (cells cell member) safe admitted active
  have positive := (safe.2 o (List.mem_of_find?_eq_some found)).1
  have exactAccess : IR.accessObject m (.pointer o.id 0) size align = .ok (o, 0) :=
    IR.access_object_zero m o size align (Nat.ne_of_gt positive)
      (by simpa only [identity] using found) (live.trans active)
      sizePositive alignPositive
      (by simpa only [bytes, private_cell_bytes_length] using sizeBound)
      (Nat.le_trans alignBound alignment)
  let added : List IR.Value := [.memory m]
  let bank := values ++ added
  have count := private_values_length typed
  have new0 : privateToken bank before.tokens.length = .memory m := by
    rw [← count]
    simpa [bank, added, privateToken] using private_token_new values added 0
  have worldBound : before.world < values.length := by
    rw [count]
    simp only [PrivateConfig.WellShaped, Bool.and_eq_true] at oldShape
    exact of_decide_eq_true oldShape.1.1.1.2
  have nextWorld : privateToken bank before.world = .world := by
    rw [private_token_old values added before.world worldBound, world]
  have pointer := (cells cell member).1
  have operation : IR.evalOperation owner (.access size align)
      [.memory m, .pointer o.id 0] = .ok added := by
    simp only [IR.evalOperation, IR.memory, Bind.bind, Except.bind, exactAccess,
      pure, Except.pure, added]
  refine ⟨bank, ?_, fun _ => ?_⟩
  · apply private_effect_binding σ values added before nextScope inst
      [.opaque before.memory, .opaque cell.pointer] [.opaque before.tokens.length]
      [.memory] owner typed rfl operands
      (by simpa only [op, List.map_cons, List.map_nil, Slot.denote,
          before.current_token current, pointer, ← identity] using operation)
      (by change [privateToken bank before.tokens.length] = added; rw [new0])
      binding (before.scope_bound oldShape)
  · exact private_preserved_realizes σ values added incoming m owner frame before nextScope
      [.memory] before.tokens.length before.world retainedRelated current rfl new0 nextWorld shape

end EvoIR.Core.Symbolic
