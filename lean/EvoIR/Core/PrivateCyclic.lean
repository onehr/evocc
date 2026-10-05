import EvoIR.Core.PrivateFunctionClosure
import EvoIR.Core.PrivateCutpoint

/-! Productive private checkpoint-table closure. Each table entry is checked
for every valuation and every actual caller snapshot, independently of entry
reachability. Allocation histories are retained by the concrete memory. -/
namespace EvoIR.Core.Symbolic

def privateCallerTable (source : Source.Program) (ir : IR.Program) (target : IR.Function)
    (points : List (PrivateLayoutPoint target)) (fid : Nat) (tokens : Bool) (type : Source.IntType)
    (build : PrivateContextContinuation source ir target points fid tokens type) :
    (todo : List (PrivateLayoutPoint target)) → Option (PLift (∀ point ∈ todo,
      ∀ σ values incoming context owner frame stack observationType,
      point.config.Realizes σ values incoming owner frame →
      PrivateContextOpen source ir target points incoming fid type observationType tokens context owner stack
        ((point.layout.site.cursor (canonicalStore point.layout.shape)).denote σ fid context)
        (privateState σ values target point.layout.label 0 owner frame point.config stack)))
  | [] => some ⟨by intro point member; cases member⟩
  | point :: rest => do
    let head ← build [] (point.layout.site.cursor (canonicalStore point.layout.shape)) point.layout.label 0 point.config
    let tail ← privateCallerTable source ir target points fid tokens type build rest
    return ⟨by
      intro selected member σ values incoming context owner frame stack observationType related
      rcases List.mem_cons.mp member with rfl | member
      · exact head.down σ trivial values incoming context owner frame stack observationType related
      · exact tail.down selected member σ values incoming context owner frame stack observationType related⟩

def privateFunctionWith (program : Source.Program) (ir : IR.Program)
    (source : Source.Function) (target : IR.Function) (layouts : List CutpointLayout)
    (builder : (points : List (PrivateLayoutPoint target)) →
      (tokens : Bool) → PrivateContextContinuation program ir target points source.symbol tokens source.returnType.type) :
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
  let points ← layouts.mapM (privateLayoutPoint target)
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
          let spans ← privateCallerTable program ir target points source.symbol tokens source.returnType.type
            (builder points tokens) points
          let joined ← checkPrivateExit target points source.symbol source.returnType.type
            ⟨.statement source.body, locals.val, [.function none source.returnType.type]⟩
            (.next block.val.name 0 (privateEntryConfig scope.val))
          return {
            tokens, scope := scope.val, locals := locals.val, block := block.val,
            signature, parameters, binding := scope.property, bounded, storage := locals.property,
            head := block.property, returns := returnsEq,
            body := by
              intro σ incoming context owner frame stack observationType safe fresh future
              cases joined.down with
              | successor entry =>
                apply Transition.OpenCheckpoint.closeWithin
                  (boundary := PrivateJointRelated target points incoming source.symbol context owner stack)
                  (nextBoundary := PrivateFunctionReturn incoming owner source.symbol source.returnType.type tokens context stack)
                  _ (entry σ [.memory incoming, .world] incoming context owner frame stack
                    (private_entry_realizes σ scope.val incoming owner frame bounded safe fresh future))
                rintro a b ⟨point, member, τ, values, nextFrame, related, rfl, rfl⟩
                exact spans.down point member τ values incoming context owner nextFrame stack observationType related }
        else none
      else none
    else none
  else none

def privateCyclicFunction (program : Source.Program) (ir : IR.Program)
    (source : Source.Function) (target : IR.Function) (layouts : List CutpointLayout) (budget : Nat) :
    Option (PrivateFunctionDiagram program ir source target) :=
  privateFunctionWith program ir source target layouts (fun points tokens =>
    privateContextFiniteSpan program ir target points source.symbol tokens source.returnType.type budget)

def privatePairWith (source : Source.Program) (target : IR.Program)
    (build : (sf : Source.Function) → (tf : IR.Function) → Option (PrivateFunctionDiagram source target sf tf)) :
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
    let d ← build sf.val tf.val
    return {
      sourceFunction := sf.val, targetFunction := tf.val, tokens := d.tokens,
      checked := by
        intro args typed
        have coverage := parameter_words_cover sf.val.parameters args typed
        have proof := d.refines (inputSubst args) source target chosen
          (by simpa only [id] using sf.property) (by simpa only [name] using tf.property)
        simpa only [coverage] using proof }
  else none

def privateLoopPair (source : Source.Program) (target : IR.Program)
    (layouts : List CutpointLayout) (budget : Nat) : Option (CallFrontierPair source target) :=
  privatePairWith source target (fun sf tf => privateCyclicFunction source target sf tf layouts budget)

end EvoIR.Core.Symbolic
