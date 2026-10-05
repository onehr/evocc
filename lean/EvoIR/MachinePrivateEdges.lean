import EvoIR.MachinePrivateCutpoint
import EvoIR.Core.PrivateCutpoint

/-!
Actual SSA edges capture their entire source vectors before rebinding either
machine. Fresh cutpoint substitutions describe the captured scalar values and
cell bytes; pointer identities and current memory travel in the actual token
bank. The synthetic local bank below is a finite projection of checked slots,
not a source-language execution.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

private def privateEdgeNativeValue : IR.Value → EvoIR.Machine.Value
  | .bits _ bits => .int (bits.setWidth 64)
  | .pointer id offset => .pointer id offset
  | _ => .int 0

private theorem private_input_native_lookup {Γ : Var → Nat} (σ : Subst Γ)
    (values : List IR.Value) (scope : Scope Γ) (input : PrivateInput Γ)
    (follows : input.followsScope scope)
    (pointers : PrivateInputsBound [input] values (scope.denote σ (privateToken values))) :
    (IR.lookup (scope.denote σ (privateToken values)) input.source).map privateEdgeNativeValue =
      .ok (input.value σ values) := by
  cases input with
  | word name reg word =>
    have read := word_lookup_sound σ (privateToken values) scope name word follows
    simp only [PrivateInput.source, read, Except.map, privateEdgeNativeValue,
      Word.denote, PrivateInput.value]
  | pointer name reg key =>
    obtain ⟨id, _, pointer⟩ := pointers name reg key List.mem_cons_self
    simp only [PrivateInput.followsScope] at follows
    simp [PrivateInput.source, IR.lookup, lookup_denote, follows, Slot.denote,
      Except.map, PrivateInput.value, privatePointerValue, privateEdgeNativeValue, pointer]

private theorem private_point_inputs_bound {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (point : PrivateCutpoint sf tf) (σ : Subst inputWidths) (values : List IR.Value)
    (regs : EvoIR.Machine.RegisterFile)
    (models : PrivatePointer.Models point.bank.pointers
      (point.bank.source.scope.denote σ (privateToken values)) regs) :
    PrivateInputsBound point.inputs values (point.source.config.scope.denote σ (privateToken values)) := by
  intro name reg key member
  have included : (⟨name, reg⟩ : PrivatePointer) ∈ privateBindingPointers point.inputs := by
    exact List.mem_filterMap.mpr ⟨.pointer name reg key, member, rfl⟩
  have inBank : (⟨name, reg⟩ : PrivatePointer) ∈ point.bank.pointers := by
    simpa only [point.pointers] using included
  obtain ⟨id, read, _⟩ := models ⟨name, reg⟩ inBank
  have sourceRead : IR.lookup (point.source.config.scope.denote σ (privateToken values)) name =
      .ok (.pointer id 0) := by simpa only [point.sourceBank] using read
  have follows := point.inputMatches (.pointer name reg key) member
  change point.source.config.scope.lookup name = some (.opaque key) at follows
  have known : IR.lookup (point.source.config.scope.denote σ (privateToken values)) name =
      .ok (privateToken values key) := by
    simp [IR.lookup, lookup_denote, follows, Slot.denote]
  exact ⟨id, sourceRead, Except.ok.inj (known.symm.trans sourceRead)⟩

private def privateSnapshotLocals {Γ : Var → Nat} (config : PrivateConfig Γ) :
    List LocalLink → Option (LocalStore Γ)
  | [] => some []
  | (name, symbol, type) :: rest => do
    let value : Option (Term Γ type.width) ←
      match config.scope.lookup name with
      | some (.word ⟨width, term⟩) =>
        if same : width = type.width then some (some (same ▸ term)) else none
      | some (.opaque key) => do
        let cell ← config.cells.find? (·.pointer == key)
        if same : cell.width = type.width then some (same ▸ cell.value) else none
      | _ => none
    let tail ← privateSnapshotLocals config rest
    return ⟨symbol, ⟨type, false⟩, value⟩ :: tail

private def privateEdgeRead {Γ Δ : Var → Nat} (before : PrivateBank Γ)
    (scope : Scope Γ) (input : PrivateInput Δ) (reg : EvoIR.Machine.VReg)
    (checks : List (Guard Γ)) :
    Option {slot : Slot Γ // scope.lookup input.source = some slot ∧
      ∀ σ values incoming owner frame memory regs,
        before.Models σ values incoming owner frame memory regs →
        guards σ checks = .ok () →
        EvoIR.Machine.read regs reg =
          .ok (privateEdgeNativeValue (slot.denote σ (privateToken values)))} :=
  match input with
  | .word name _ _ => do
    let word : {word // scope.lookup name = some (.word word)} ←
      match found : scope.lookup name with
      | some (.word word) => some ⟨word, found⟩
      | _ => none
    let native ← wordOperand before.words reg
    let equal ← checkWordUnder checks word.val native.val
    if present : (before.words.lookup reg.id).isSome = true then
      return ⟨.word word.val, word.property, by
        intro σ values incoming owner frame memory regs models passed
        have agrees := bank_models_operand σ before.words regs models.words reg present
        have agreesValue : EvoIR.Machine.operandValue (EvoIR.Machine.readOperand regs) reg =
            EvoIR.Machine.operandValue (EvoIR.Machine.readOperand (bankDenote σ before.words)) reg := by
          simp only [EvoIR.Machine.operandValue, agrees]
        have read := word_operand_value before.words reg native.val native.property σ
        rw [← agreesValue, EvoIR.Machine.operandValue_readOperand, equal.down σ passed] at read
        simpa only [Slot.denote, Word.denote, privateEdgeNativeValue] using read⟩
    else none
  | .pointer name _ _ => do
    let key : {key // scope.lookup name = some (.opaque key)} ←
      match found : scope.lookup name with
      | some (.opaque key) => some ⟨key, found⟩
      | _ => none
    before.pointers.attach.findSome? fun link =>
      if target : link.val.target = reg then
        if source : before.source.scope.lookup link.val.source = some (.opaque key.val) then
          some ⟨.opaque key.val, key.property, by
            intro σ values incoming owner frame memory regs models passed
            obtain ⟨id, sourceRead, targetRead⟩ := models.pointers link.val link.property
            have known : IR.lookup (before.source.scope.denote σ (privateToken values))
                link.val.source = .ok (privateToken values key.val) := by
              simp [IR.lookup, lookup_denote, source, Slot.denote]
            have pointer : privateToken values key.val = .pointer id 0 :=
              Except.ok.inj (known.symm.trans sourceRead)
            have native : EvoIR.Machine.read regs reg = .ok (.pointer id 0) := by
              rw [← EvoIR.Machine.operandValue_readOperand]
              simp [EvoIR.Machine.operandValue, ← target, targetRead,
                Bind.bind, Except.bind, Pure.pure, Except.pure]
            simpa only [Slot.denote, pointer, privateEdgeNativeValue] using native⟩
        else none
      else none

private def privateEdgeReads {Γ Δ : Var → Nat} (before : PrivateBank Γ)
    (scope : Scope Γ) (checks : List (Guard Γ)) :
    (inputs : List (PrivateInput Δ)) → (args : List EvoIR.Machine.VReg) →
    Option {slots : List (Slot Γ) //
      inputs.mapM (fun input => scope.lookup input.source) = some slots ∧
      ∀ σ values incoming owner frame memory regs,
        before.Models σ values incoming owner frame memory regs →
        guards σ checks = .ok () →
        args.mapM (EvoIR.Machine.read regs) =
          .ok (slots.map fun slot => privateEdgeNativeValue (slot.denote σ (privateToken values)))}
  | [], [] => some ⟨[], rfl, by intro σ values incoming owner frame memory regs models passed; rfl⟩
  | input :: inputs, arg :: args => do
    let first ← privateEdgeRead before scope input arg checks
    let tail ← privateEdgeReads before scope checks inputs args
    return ⟨first.val :: tail.val, by
      simp only [List.mapM_cons, first.property.1, tail.property.1,
        Bind.bind, Option.bind, Pure.pure], by
      intro σ values incoming owner frame memory regs models passed
      simp only [List.mapM_cons, first.property.2 σ values incoming owner frame memory regs models passed,
        tail.property.2 σ values incoming owner frame memory regs models passed,
        List.map_cons, Bind.bind, Except.bind, Pure.pure, Except.pure]⟩
  | _, _ => none

private theorem private_edge_values {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (values newValues : List IR.Value) (scope : Scope Γ) (projected : Scope Δ)
    (inputs : List (PrivateInput Δ)) (slots : List (Slot Γ))
    (found : inputs.mapM (fun input => scope.lookup input.source) = some slots)
    (follows : ∀ input ∈ inputs, input.followsScope projected)
    (pointers : PrivateInputsBound inputs newValues (projected.denote τ (privateToken newValues)))
    (same : scope.denote σ (privateToken values) = projected.denote τ (privateToken newValues)) :
    slots.map (fun slot => privateEdgeNativeValue (slot.denote σ (privateToken values))) =
      inputs.map (PrivateInput.value τ newValues) := by
  induction inputs generalizing slots with
  | nil =>
    have empty : slots = [] := by simpa using found.symm
    subst slots
    rfl
  | cons input inputs ih =>
    cases first : scope.lookup input.source with
    | none => simp [List.mapM_cons, first] at found
    | some slot =>
      cases rest : inputs.mapM (fun input => scope.lookup input.source) with
      | none => simp [List.mapM_cons, first, rest] at found
      | some tail =>
        have slotsEq : slots = slot :: tail := by
          simpa [List.mapM_cons, first, rest] using found.symm
        subst slots
        have actual : (IR.lookup (scope.denote σ (privateToken values)) input.source).map
            privateEdgeNativeValue = .ok (privateEdgeNativeValue (slot.denote σ (privateToken values))) := by
          simp [IR.lookup, lookup_denote, first, Except.map]
        rw [same] at actual
        have canonical := private_input_native_lookup τ newValues projected input
          (follows input List.mem_cons_self) (by
            intro name reg key member
            rcases List.mem_cons.mp member with equal | impossible
            · exact pointers name reg key (List.mem_cons.mpr (.inl equal))
            · cases impossible)
        have equal := Except.ok.inj (actual.symm.trans canonical)
        simp only [List.map_cons, equal]
        exact congrArg (List.cons _) (ih tail rest
          (by intro selected member; exact follows selected (List.mem_cons_of_mem _ member))
          (by intro name reg key member; exact pointers name reg key (List.mem_cons_of_mem _ member)))

def PrivateEdgeDiagram {Γ : Var → Nat} (sf : IR.Function)
    (tf : EvoIR.Machine.Function) (before : PrivateBank Γ) (point : PrivateCutpoint sf tf)
    (sourceEdge : IR.Edge) (targetId : Nat) (targetArgs : List EvoIR.Machine.VReg)
    (checks : List (Guard Γ)) : Prop :=
  ∀ σ values incoming owner frame memory regs logicalInputs sourceBlock sourcePc targetBlock targetPc,
    before.Models σ values incoming owner frame memory regs →
    guards σ checks = .ok () →
    ∃ τ newValues nextRegs,
      point.bank.Models τ newValues incoming owner frame memory nextRegs ∧
      IR.jump sf ⟨sf.name, sourceBlock, sourcePc,
          before.source.scope.denote σ (privateToken values), owner⟩ sourceEdge =
        .ok ⟨sf.name, point.source.block.name, 0,
          point.bank.source.scope.denote τ (privateToken newValues), owner⟩ ∧
      EvoIR.Machine.jump .ssa tf ⟨tf.name, targetBlock, targetPc, regs, owner, logicalInputs⟩
          targetId targetArgs =
        .ok ⟨tf.name, point.targetBlock.id, 0, nextRegs, owner, logicalInputs⟩

/-- This checker uses both complete actual edge vectors. A missing, swapped,
or scalarized pointer argument cannot be supplied by a cutpoint proposal. -/
def privateEdge {Γ : Var → Nat} (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (before : PrivateBank Γ) (point : PrivateCutpoint sf tf) (sourceEdge : IR.Edge)
    (targetId : Nat) (targetArgs : List EvoIR.Machine.VReg) (checks : List (Guard Γ)) :
    Option (PLift (PrivateEdgeDiagram sf tf before point sourceEdge targetId targetArgs checks)) := do
  if sourceLabel : sourceEdge.block = point.source.layout.label then
    if targetLabel : targetId = point.targetBlock.id then
      let scope : {scope // edge (privateTokenType before.source.tokens) sf before.source.scope sourceEdge = some scope} ←
        match accepted : edge (privateTokenType before.source.tokens) sf before.source.scope sourceEdge with
        | none => none
        | some scope => some ⟨scope, accepted⟩
      let current : PrivateConfig Γ := {before.source with scope := scope.val}
      if shaped : current.WellShaped = true then
        let locals ← privateSnapshotLocals current point.source.layout.links
        if shape : locals.map LocalSlot.shape = point.source.layout.shape then
          let template : {config // privateProject point.source.block.parameters point.source.layout locals = some config} ←
            match projected : privateProject point.source.block.parameters point.source.layout locals with
            | none => none
            | some config => some ⟨config, projected⟩
          let bank ← privateProjectBank current point.source.layout
          let rebinding ← privateRebindingCheck current template.val bank
          let arguments ← privateEdgeReads before scope.val checks point.inputs targetArgs
          return ⟨by
            intro σ values incoming owner frame memory regs logicalInputs sourceBlock sourcePc targetBlock targetPc models passed
            let newValues := bank.map (Slot.denote σ (privateToken values))
            obtain ⟨rebind⟩ := rebinding.down σ values models.source.2.1 models.source.2.2.1
            have rescoped := private_rescope_realizes σ values incoming owner frame before.source
              scope.val models.source shaped
            have intermediate := private_rebind_realizes σ σ values newValues incoming owner frame
              current template.val rescoped rebind
            obtain ⟨τ, stored⟩ := canonical_store_of_shape σ locals point.source.layout.shape shape
            have canonical := private_project_realizes σ τ newValues incoming owner frame
              point.source.block.parameters point.source.layout locals
              (canonicalStore point.source.layout.shape) template.val point.source.config
              template.property point.source.projected stored.symm intermediate
            have sourceValues : scope.val.denote σ (privateToken values) =
                point.source.config.scope.denote τ (privateToken newValues) :=
              rebind.scope.symm.trans canonical.2
            obtain ⟨nextRegs, bound, wordModels, pointerModels⟩ :=
              point.targetBinding τ newValues incoming owner frame (by simpa only [point.sourceBank] using canonical.1)
            have captured := arguments.property.2 σ values incoming owner frame memory regs models passed
            rw [private_edge_values σ τ values newValues scope.val point.source.config.scope
              point.inputs arguments.val arguments.property.1 point.inputMatches
              (private_point_inputs_bound point τ newValues nextRegs pointerModels) sourceValues] at captured
            have projected := private_project_congr σ τ (privateToken newValues)
              point.source.block.parameters point.source.layout locals
              (canonicalStore point.source.layout.shape) template.val point.source.config
              template.property point.source.projected stored.symm
            have memorySame : point.bank.source.memoryValue newValues = before.source.memoryValue values := by
              rw [point.sourceBank]
              unfold PrivateConfig.memoryValue
              rw [← projected.2.1]
              exact rebind.memory
            refine ⟨τ, newValues, nextRegs, ⟨by simpa only [point.sourceBank] using canonical.1,
              wordModels, pointerModels, ?_⟩, ?_, ?_⟩
            · obtain ⟨actual, found, related⟩ := models.memory
              exact ⟨actual, memorySame.trans found, related⟩
            · have actual := edge_sound σ (privateToken values) (privateTokenType before.source.tokens)
                (private_token_typed values before.source.tokens models.source.2.1) sf
                ⟨sf.name, sourceBlock, sourcePc,
                  before.source.scope.denote σ (privateToken values), owner⟩
                before.source.scope scope.val sourceEdge rfl scope.property
              have blockName : point.source.block.name = point.source.layout.label := by
                simpa using List.find?_some point.source.found
              simpa only [sourceValues, point.sourceBank, sourceLabel, ← blockName] using actual
            · simpa only [EvoIR.Machine.jump, targetLabel, point.targetFound, captured,
                Bind.bind, Except.bind, bound, Pure.pure, Except.pure]⟩
        else none
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
