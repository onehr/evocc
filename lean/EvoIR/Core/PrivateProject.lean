import EvoIR.Core.PrivateRebind
import EvoIR.Core.CutpointInvariant

/-! A source local may be represented by an SSA scalar or by a private cell.
The actual target parameter type selects the route; proposals supply only the
association. Physical pointers are rebound separately, never fabricated. -/
namespace EvoIR.Core.Symbolic

def privateLocalValue {Γ : Var → Nat} (locals : LocalStore Γ) (symbol : Nat)
    (type : Source.IntType) : Option (Option (Term Γ type.width)) := do
  let slot ← locals.find? (·.symbol == symbol)
  if same : slot.type.type = type then
    return (congrArg Source.IntType.width same) ▸ slot.value
  else none

structure PrivateProjection (Γ : Var → Nat) where
  scope : Scope Γ
  cells : List (PrivateCell Γ)
  types : List IR.Ty

def privateProjectLinks {Γ : Var → Nat} (parameters : List IR.Binding) (locals : LocalStore Γ) :
    List LocalLink → Nat → Option (PrivateProjection Γ)
  | [], _ => some ⟨[], [], []⟩
  | (id, symbol, type) :: rest, key => do
    let binding ← parameters.find? (·.id == id)
    let tail ← privateProjectLinks parameters locals rest (key+1)
    match binding.type with
    | .bits width =>
      if width = type.width then
        let word ← locals.read symbol type
        return ⟨(id, .word word.word) :: tail.scope, tail.cells, .bits width :: tail.types⟩
      else none
    | .pointer =>
      let value ← privateLocalValue locals symbol type
      return ⟨(id, .opaque key) :: tail.scope,
        ⟨key, type.width, true, value⟩ :: tail.cells, .pointer :: tail.types⟩
    | _ => none

def privateProject {Γ : Var → Nat} (parameters : List IR.Binding) (layout : CutpointLayout)
    (locals : LocalStore Γ) : Option (PrivateConfig Γ) := do
  if layout.tokens.any (fun token => token.2 > 1) then none else
  let links ← privateProjectLinks parameters locals layout.links 2
  return ⟨layout.tokens.map (fun (id, token) => (id, .opaque token)) ++ links.scope,
    [.memory, .world] ++ links.types, 0, 1, links.cells⟩

theorem private_local_value_sound {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ)
    (symbol : Nat) (type : Source.IntType) (value : Option (Term Γ type.width))
    (accepted : privateLocalValue locals symbol type = some value) :
    ((locals.denote σ).find? (·.symbol == symbol)).bind Source.Slot.value =
      value.map (fun term => Source.Value.mk type (denoteTerm σ term)) := by
  unfold privateLocalValue at accepted
  cases found : locals.find? (·.symbol == symbol) with
  | none => simp [found] at accepted
  | some slot =>
    by_cases same : slot.type.type = type
    · subst type
      simp only [found, Bind.bind, Option.bind, dif_pos rfl, pure] at accepted
      cases Option.some.inj accepted
      simp only [local_find_sound, found, Option.map_some, Option.bind_some, LocalSlot.denote]
    · simp [found, same] at accepted

private theorem private_local_cell_bytes {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (type : Source.IntType) (left : Option (Term Γ type.width)) (right : Option (Term Δ type.width))
    (a b : Nat)
    (same : left.map (fun term => Source.Value.mk type (denoteTerm σ term)) =
      right.map (fun term => Source.Value.mk type (denoteTerm τ term))) :
    (PrivateCell.mk a type.width true left).bytes σ =
      (PrivateCell.mk b type.width true right).bytes τ := by
  cases left with
  | none => cases right <;> cases same <;> rfl
  | some left =>
    cases right with
    | none => cases same
    | some right =>
      have values := Option.some.inj same
      have bits : denoteTerm σ left = denoteTerm τ right :=
        BitVec.eq_of_toNat_eq (congrArg (fun v : Source.Value => v.bits.toNat) values)
      change IR.scalarBytes type.width (denoteTerm σ left) =
        IR.scalarBytes type.width (denoteTerm τ right)
      rw [bits]

private def privateCellShape {Γ : Var → Nat} (cell : PrivateCell Γ) : Nat × Nat × Bool :=
  (cell.pointer, cell.width, cell.live)

private theorem private_project_links_congr {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (ρ : Nat → IR.Value) (parameters : List IR.Binding) (a : LocalStore Γ) (b : LocalStore Δ)
    (links : List LocalLink) (key : Nat) (first : PrivateProjection Γ) (second : PrivateProjection Δ)
    (left : privateProjectLinks parameters a links key = some first)
    (right : privateProjectLinks parameters b links key = some second)
    (same : a.denote σ = b.denote τ) :
    first.types = second.types ∧
      (∀ size, first.scope.inBank size = second.scope.inBank size) ∧
      first.cells.map privateCellShape = second.cells.map privateCellShape ∧
      first.scope.denote σ ρ = second.scope.denote τ ρ ∧
      ∀ cell ∈ second.cells, ∃ old ∈ first.cells,
        cell.pointer = old.pointer ∧ cell.width = old.width ∧ cell.live = old.live ∧
        cell.bytes τ = old.bytes σ := by
  induction links generalizing key first second with
  | nil =>
    cases Option.some.inj left
    cases Option.some.inj right
    exact ⟨rfl, fun _ => rfl, rfl, rfl, by intro cell member; cases member⟩
  | cons link links ih =>
    rcases link with ⟨id, symbol, type⟩
    cases selected : parameters.find? (·.id == id) with
    | none => simp [privateProjectLinks, selected] at left
    | some parameter =>
      cases firstTail : privateProjectLinks parameters a links (key+1) with
      | none => simp [privateProjectLinks, selected, firstTail] at left
      | some firstRest =>
        cases secondTail : privateProjectLinks parameters b links (key+1) with
        | none => simp [privateProjectLinks, selected, secondTail] at right
        | some secondRest =>
          have tail := ih (key+1) firstRest secondRest firstTail secondTail
          cases targetType : parameter.type with
          | bits width =>
            by_cases typed : width = type.width
            · cases firstWord : a.read symbol type with
              | none => simp [privateProjectLinks, selected, firstTail, targetType, typed, firstWord] at left
              | some av =>
                cases secondWord : b.read symbol type with
                | none => simp [privateProjectLinks, selected, secondTail, targetType, typed, secondWord] at right
                | some bv =>
                  simp only [privateProjectLinks, selected, firstTail, secondTail, Bind.bind,
                    Option.bind, targetType, typed, ↓reduceIte, firstWord, secondWord, pure,
                    Option.some.injEq] at left right
                  subst first; subst second
                  have ax := (local_read_sound σ a symbol type av firstWord).1
                  have bx := (local_read_sound τ b symbol type bv secondWord).1
                  rw [same] at ax
                  have equal := Except.ok.inj (ax.symm.trans bx)
                  have word : av.word.denote σ = bv.word.denote τ := congrArg sourceValueIR equal
                  refine ⟨congrArg (List.cons (.bits type.width)) tail.1, ?_, tail.2.2.1, ?_, tail.2.2.2.2⟩
                  · intro size
                    simpa [Scope.inBank, Slot.inBank] using tail.2.1 size
                  · simp only [Scope.denote, List.map_cons, Slot.denote, word]
                    exact congrArg (List.cons _) tail.2.2.2.1
            · simp [privateProjectLinks, selected, firstTail, targetType, typed] at left
          | pointer =>
            cases firstValue : privateLocalValue a symbol type with
            | none => simp [privateProjectLinks, selected, firstTail, targetType, firstValue] at left
            | some av =>
              cases secondValue : privateLocalValue b symbol type with
              | none => simp [privateProjectLinks, selected, secondTail, targetType, secondValue] at right
              | some bv =>
                simp only [privateProjectLinks, selected, firstTail, secondTail, Bind.bind,
                  Option.bind, targetType, firstValue, secondValue, pure, Option.some.injEq] at left right
                subst first; subst second
                have ax := private_local_value_sound σ a symbol type av firstValue
                have bx := private_local_value_sound τ b symbol type bv secondValue
                rw [same] at ax
                have byteEq := private_local_cell_bytes σ τ type av bv key key (ax.symm.trans bx)
                refine ⟨congrArg (List.cons .pointer) tail.1, ?_, ?_, ?_, ?_⟩
                · intro size
                  simp only [Scope.inBank, List.all_cons, Slot.inBank]
                  exact congrArg (Bool.and _) (tail.2.1 size)
                · exact congrArg (List.cons (key, type.width, true)) tail.2.2.1
                · simp only [Scope.denote, List.map_cons, Slot.denote]
                  exact congrArg (List.cons _) tail.2.2.2.1
                · intro cell member
                  rcases List.mem_cons.mp member with rfl | member
                  · exact ⟨⟨key, type.width, true, av⟩, by simp, rfl, rfl, rfl, byteEq.symm⟩
                  · obtain ⟨old, member, fields⟩ := tail.2.2.2.2 cell member
                    exact ⟨old, List.mem_cons_of_mem _ member, fields⟩
          | memory | world => simp [privateProjectLinks, selected, firstTail, targetType] at left

theorem private_project_congr {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (ρ : Nat → IR.Value) (parameters : List IR.Binding) (layout : CutpointLayout)
    (a : LocalStore Γ) (b : LocalStore Δ) (ca : PrivateConfig Γ) (cb : PrivateConfig Δ)
    (left : privateProject parameters layout a = some ca)
    (right : privateProject parameters layout b = some cb)
    (same : a.denote σ = b.denote τ) :
    ca.tokens = cb.tokens ∧ ca.memory = cb.memory ∧ ca.world = cb.world ∧
      ca.WellShaped = cb.WellShaped ∧ ca.scope.denote σ ρ = cb.scope.denote τ ρ ∧
      ∀ cell ∈ cb.cells, ∃ old ∈ ca.cells,
        cell.pointer = old.pointer ∧ cell.width = old.width ∧ cell.live = old.live ∧
        cell.bytes τ = old.bytes σ := by
  unfold privateProject at left right
  split at left
  · contradiction
  · rename_i validTokens
    simp only [validTokens, Bool.false_eq_true, ↓reduceIte] at right
    cases firstLinks : privateProjectLinks parameters a layout.links 2 with
    | none => simp [firstLinks] at left
    | some first =>
      cases secondLinks : privateProjectLinks parameters b layout.links 2 with
      | none => simp [secondLinks] at right
      | some second =>
        simp only [firstLinks, secondLinks, Bind.bind, Option.bind, pure, Option.some.injEq] at left right
        subst ca; subst cb
        obtain ⟨types, scopeBound, cellShapes, scopeValues, cells⟩ :=
          private_project_links_congr σ τ ρ parameters a b layout.links 2 first second firstLinks secondLinks same
        have pointers : first.cells.map PrivateCell.pointer = second.cells.map PrivateCell.pointer := by
          simpa only [List.map_map, Function.comp_def, privateCellShape] using
            congrArg (List.map Prod.fst) cellShapes
        have cellChecks : first.cells.all (fun cell => cell.pointer < 2+second.types.length &&
            privateTokenType ([.memory, .world] ++ second.types) cell.pointer == .pointer &&
              (cell.width == 32 || cell.width == 64)) =
            second.cells.all (fun cell => cell.pointer < 2+second.types.length &&
              privateTokenType ([.memory, .world] ++ second.types) cell.pointer == .pointer &&
                (cell.width == 32 || cell.width == 64)) := by
          let check : Nat × Nat × Bool → Bool := fun cell =>
            cell.1 < 2+second.types.length &&
              privateTokenType ([.memory, .world] ++ second.types) cell.1 == .pointer &&
                (cell.2.1 == 32 || cell.2.1 == 64)
          calc
            _ = (first.cells.map privateCellShape).all check := by
              rw [List.all_map]
              rfl
            _ = (second.cells.map privateCellShape).all check :=
              congrArg (fun shapes => shapes.all check) cellShapes
            _ = _ := by
              rw [List.all_map]
              rfl
        refine ⟨congrArg (List.append [.memory, .world]) types, rfl, rfl, ?_, ?_, cells⟩
        · simp only [PrivateConfig.WellShaped, Scope.inBank, List.all_append, types,
            List.length_append, List.length_cons, List.length_nil, Nat.add_zero, pointers,
            cellChecks]
          rw [show first.scope.all (fun entry => entry.2.inBank (2+second.types.length)) =
              second.scope.all (fun entry => entry.2.inBank (2+second.types.length)) from scopeBound _]
          simp only [List.all_map, Function.comp_def, Slot.inBank]
        · simp only [Scope.denote, List.map_append]
          have tokenPrefix : Scope.denote σ ρ (layout.tokens.map (fun (id, token) => (id, Slot.opaque token))) =
              Scope.denote τ ρ (layout.tokens.map (fun (id, token) => (id, Slot.opaque token))) := by
            simp [Scope.denote, List.map_map, Function.comp_def, Slot.denote]
          exact congrArg (fun front => front ++ first.scope.denote σ ρ) tokenPrefix |>.trans
            (congrArg (List.append _) scopeValues)

/-- Projection depends on the current source values, not on their symbolic
history. Replacing those terms with an equal fresh substitution preserves the
actual token bank, every cell identity and the caller's memory snapshot. -/
theorem private_project_realizes {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (values : List IR.Value) (incoming : IR.Memory) (owner frame : Nat)
    (parameters : List IR.Binding) (layout : CutpointLayout)
    (a : LocalStore Γ) (b : LocalStore Δ) (ca : PrivateConfig Γ) (cb : PrivateConfig Δ)
    (left : privateProject parameters layout a = some ca)
    (right : privateProject parameters layout b = some cb)
    (same : a.denote σ = b.denote τ)
    (related : ca.Realizes σ values incoming owner frame) :
    cb.Realizes τ values incoming owner frame ∧
      ca.scope.denote σ (privateToken values) = cb.scope.denote τ (privateToken values) := by
  obtain ⟨types, memory, world, shape, scope, cellMap⟩ :=
    private_project_congr σ τ (privateToken values) parameters layout a b ca cb left right same
  obtain ⟨shaped, typed, worldValue, incomingSafe, m, current, safe, frameBound,
    ownerBound, privateFrame, distinct, cells⟩ := related
  have current' : cb.memoryValue values = some m := by
    simpa only [PrivateConfig.memoryValue, memory] using current
  refine ⟨⟨shape ▸ shaped, typed.trans types, world ▸ worldValue, incomingSafe,
    m, current', safe, frameBound, ownerBound, privateFrame, ?_, ?_⟩, scope⟩
  · intro first firstMember second secondMember different
    obtain ⟨oldFirst, oldFirstMember, firstPointer, _, _, _⟩ := cellMap first firstMember
    obtain ⟨oldSecond, oldSecondMember, secondPointer, _, _, _⟩ := cellMap second secondMember
    have different' : oldFirst.pointer ≠ oldSecond.pointer := by
      simpa only [firstPointer, secondPointer] using different
    simpa only [PrivateCell.id, firstPointer, secondPointer] using
      distinct oldFirst oldFirstMember oldSecond oldSecondMember different'
  · intro cell member
    obtain ⟨old, oldMember, pointer, width, live, bytes⟩ := cellMap cell member
    have identity : cell.id values = old.id values := by simp only [PrivateCell.id, pointer]
    obtain ⟨oldPointer, cutoff, object, found, objectId, owned, alive, stored, aligned⟩ :=
      cells old oldMember
    refine ⟨?_, ?_, object, ?_, ?_, owned, ?_, ?_, ?_⟩
    · simpa only [pointer, identity] using oldPointer
    · simpa only [identity] using cutoff
    · simpa only [identity] using found
    · exact objectId.trans identity.symm
    · exact alive.trans live.symm
    · exact stored.trans bytes.symm
    · simpa only [width] using aligned

structure PrivateLayoutPoint (target : IR.Function) where
  layout : CutpointLayout
  block : IR.Block
  found : target.blocks.find? (·.name == layout.label) = some block
  config : PrivateConfig inputWidths
  projected : privateProject block.parameters layout (canonicalStore layout.shape) = some config
  shaped : config.WellShaped = true

def privateLayoutPoint (target : IR.Function) (layout : CutpointLayout) : Option (PrivateLayoutPoint target) := do
  let block : {b // target.blocks.find? (·.name == layout.label) = some b} ←
    match h : target.blocks.find? (·.name == layout.label) with
    | none => none
    | some b => some ⟨b, h⟩
  match projected : privateProject block.val.parameters layout (canonicalStore layout.shape) with
  | none => none
  | some config =>
    if shaped : config.WellShaped = true then
      return ⟨layout, block.val, block.property, config, projected, shaped⟩
    else none

end EvoIR.Core.Symbolic
