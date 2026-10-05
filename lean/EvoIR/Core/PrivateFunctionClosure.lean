import EvoIR.Core.PrivateFunctionContext
import EvoIR.Core.FunctionClosure

/-! Memory-aware whole-function admission. The finite body route is the first
consumer of the general private span relation; cyclic/call closure uses the
same context-parametric return contract. -/
namespace EvoIR.Core.Symbolic

def privateInputValues (tokens : Bool) (args : List Source.Value) (memory : IR.Memory) : List IR.Value :=
  args.map sourceValueIR ++ if tokens then [.memory memory, .world] else []

theorem private_input_values (σ : Subst inputWidths) (tokens : Bool) (args : List (CWord inputWidths))
    (memory : IR.Memory) :
    (loopInputSlots tokens args).map (Slot.denote σ (privateToken [.memory memory, .world])) =
      privateInputValues tokens (args.map (CWord.denote σ)) memory := by
  cases tokens <;> simp [loopInputSlots, callInputSlots, privateInputValues, privateToken, List.map_append,
    Slot.denote, Word.denote, CWord.word, CWord.denote, sourceValueIR, List.map_map, Function.comp_def]

structure PrivateFunctionDiagram (program : Source.Program) (ir : IR.Program)
    (source : Source.Function) (target : IR.Function) where
  tokens : Bool
  scope : Scope inputWidths
  locals : LocalStore inputWidths
  block : IR.Block
  signature : target.parameters = (loopInputSlots tokens (parameterWords source.parameters)).map
    (Slot.type (privateTokenType [.memory, .world]))
  parameters : block.parameters.map IR.Binding.type = target.parameters
  binding : bindSlots (privateTokenType [.memory, .world]) block.parameters
    (loopInputSlots tokens (parameterWords source.parameters)) = some scope
  bounded : scope.inBank 2 = true
  storage : parameterStore source.parameters (parameterWords source.parameters) = some locals
  head : target.blocks.head? = some block
  returns : target.returns = cutReturnTypes tokens source.returnType.type
  body : ∀ σ incoming context owner frame stack observationType,
    incoming.WellAllocated → incoming.BeforeFrame owner → owner < frame →
    Transition.OpenCheckpoint (Source.step program) (IR.step ir) observeSource (observeTarget observationType)
      (PrivateFunctionReturn incoming owner source.symbol source.returnType.type tokens context stack)
      ((⟨.statement source.body, locals, [.function none source.returnType.type]⟩ :
        SourceCursor inputWidths).denote σ source.symbol context)
      (privateState σ [.memory incoming, .world] target block.name 0 owner frame (privateEntryConfig scope) stack)

def privateFiniteFunction (program : Source.Program) (ir : IR.Program)
    (source : Source.Function) (target : IR.Function) (budget : Nat) :
    Option (PrivateFunctionDiagram program ir source target) := do
  if source.name != target.name || source.internal != target.internal then none else
  if target.effects != ((IR.deriveEffects ir (ir.functions.length+1)).lookup target.name).getD
      IR.Reader.emptyEffects then none else
  let block : {b // target.blocks.head? = some b} ←
    match h : target.blocks.head? with
    | none => none
    | some b => some ⟨b, h⟩
  let locals : {ls // parameterStore source.parameters (parameterWords source.parameters) = some ls} ←
    match h : parameterStore source.parameters (parameterWords source.parameters) with
    | none => none
    | some ls => some ⟨ls, h⟩
  let tokens := target.returns != [.bits source.returnType.type.width]
  if returnsEq : target.returns = cutReturnTypes tokens source.returnType.type then
    if signature : target.parameters = (loopInputSlots tokens (parameterWords source.parameters)).map
        (Slot.type (privateTokenType [.memory, .world])) then
      if parameters : block.val.parameters.map IR.Binding.type = target.parameters then
        let scope : {scope // bindSlots (privateTokenType [.memory, .world]) block.val.parameters
            (loopInputSlots tokens (parameterWords source.parameters)) = some scope} ←
          match h : bindSlots (privateTokenType [.memory, .world]) block.val.parameters
              (loopInputSlots tokens (parameterWords source.parameters)) with
          | none => none
          | some scope => some ⟨scope, h⟩
        if bounded : scope.val.inBank 2 = true then
          let body ← privateFiniteSpan program ir source.symbol target tokens source.returnType.type budget []
            ⟨.statement source.body, locals.val, [.function none source.returnType.type]⟩
            block.val.name 0 (privateEntryConfig scope.val)
          return {
            tokens, scope := scope.val, locals := locals.val, block := block.val,
            signature, parameters, binding := scope.property, bounded, storage := locals.property,
            head := block.property, returns := returnsEq,
            body := by
              intro σ incoming context owner frame stack observationType safe fresh future
              exact body.down σ trivial [.memory incoming, .world] incoming context owner frame stack observationType
                (private_entry_realizes σ scope.val incoming owner frame bounded safe fresh future) }
        else none
      else none
    else none
  else none

theorem PrivateFunctionDiagram.refines (σ : Subst inputWidths)
    {source : Source.Function} {target : IR.Function} (s : Source.Program) (t : IR.Program)
    (d : PrivateFunctionDiagram s t source target)
    (selected : s.entries.contains source.symbol = true)
    (sourceFound : s.functions.find? (·.symbol == source.symbol) = some source)
    (targetFound : t.functions.find? (·.name == target.name) = some target) :
    ∃ sourceEntry targetEntry,
      Source.initial s source.symbol ((parameterWords source.parameters).map (CWord.denote σ)) = .ok sourceEntry ∧
      IR.initial t target.name (loopInputValues d.tokens
        ((parameterWords source.parameters).map (CWord.denote σ))) = .ok targetEntry ∧
      LoweringRefines s t source.returnType.type sourceEntry targetEntry := by
  let memory : IR.Memory := ⟨1, []⟩
  let values : List IR.Value := [.memory memory, .world]
  have stored := parameter_store_sound σ source.parameters (parameterWords source.parameters) d.locals d.storage
  have tokenTypes : ∀ id, (privateToken values id).type = privateTokenType [.memory, .world] id :=
    private_token_typed values [.memory, .world] rfl
  have binding := bind_slots_sound σ (privateToken values) (privateTokenType [.memory, .world]) tokenTypes _ _ _ d.binding
  have types : ((loopInputSlots d.tokens (parameterWords source.parameters)).map
      (Slot.denote σ (privateToken values))).map IR.Value.type = target.parameters := by
    rw [d.signature, List.map_map]
    apply List.map_congr_left
    intro slot _
    exact slot_type_sound σ (privateToken values) (privateTokenType [.memory, .world]) tokenTypes slot
  have inputValues : privateInputValues d.tokens ((parameterWords source.parameters).map (CWord.denote σ)) memory =
      loopInputValues d.tokens ((parameterWords source.parameters).map (CWord.denote σ)) := by
    cases d.tokens <;> rfl
  have boundTarget : IR.enter target (loopInputValues d.tokens
      ((parameterWords source.parameters).map (CWord.denote σ))) 1 =
      .ok ⟨target.name, d.block.name, 0, d.scope.denote σ (privateToken values), 1⟩ := by
    rw [← inputValues, ← private_input_values]
    simp [IR.enter, types, d.head, d.parameters, binding, Bind.bind, Except.bind, pure, Except.pure, values]
  let se : Source.State :=
    ⟨.statement source.body, d.locals.denote σ, [.function none source.returnType.type], source.symbol⟩
  let te := privateState σ values target d.block.name 0 1 2 (privateEntryConfig d.scope) []
  refine ⟨se, te, ?_, ?_, ?_⟩
  · have chosen : source.symbol ∈ s.entries := by simpa using selected
    simp [Source.initial, chosen, sourceFound, stored, Bind.bind, Except.bind, pure, Except.pure, se]
  · simp [IR.initial, targetFound, boundTarget, Bind.bind, Except.bind, pure, Except.pure, te, privateState, privateEntryConfig]
  have body := d.body σ memory none 1 2 [] source.returnType.type (by simp [IR.Memory.WellAllocated, memory])
    (by simp [IR.Memory.BeforeFrame, memory]) (by decide)
  apply body.finish
  rintro a b ⟨value, typed, locals, returnedMemory, frame, rfl, _, _, _, _, _, rfl⟩
  apply Transition.refines_of_terminals (.refl _) (.refl _) rfl rfl
    (o := Observation.returned source.returnType.type value.bits.toNat)
  · simp [observeSource, returnedSource, typed]
  · cases d.tokens <;> simp [observeTarget, cutResume, sourceValueIR, typed]

def privateFinitePair (source : Source.Program) (target : IR.Program) (budget : Nat) :
    Option (CallFrontierPair source target) := do
  if !(Source.checkWellFormed source).isOk || !(IR.checkProgram target).isOk then none else
  let [entry] := source.entries | none
  let sf : {f // source.functions.find? (·.symbol == entry) = some f} ←
    match found : source.functions.find? (·.symbol == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  if chosen : source.entries.contains sf.val.symbol = true then
    let tf : {f // target.functions.find? (·.name == sf.val.name) = some f} ←
      match found : target.functions.find? (·.name == sf.val.name) with
      | none => none
      | some f => some ⟨f, found⟩
    have name : tf.val.name = sf.val.name := by simpa using List.find?_some tf.property
    have id : sf.val.symbol = entry := by simpa using List.find?_some sf.property
    let d ← privateFiniteFunction source target sf.val tf.val budget
    return {
      sourceFunction := sf.val, targetFunction := tf.val, tokens := d.tokens,
      checked := by
        intro args typed
        have coverage := parameter_words_cover sf.val.parameters args typed
        have proof := d.refines (inputSubst args) source target chosen
          (by simpa only [id] using sf.property) (by simpa only [name] using tf.property)
        simpa only [coverage] using proof }
  else none

end EvoIR.Core.Symbolic
