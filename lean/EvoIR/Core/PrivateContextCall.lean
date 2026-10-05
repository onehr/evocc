import EvoIR.Core.PrivateCyclic
import EvoIR.Core.PrivateCallBindings
import EvoIR.Core.SourceFrontier

/-! Private interprocedural fragments enter actual callees with the caller's
current memory, and compose independently checked callee and caller bodies. -/
namespace EvoIR.Core.Symbolic

abbrev PrivateFunctionBuilder (source : Source.Program) (target : IR.Program) :=
  (sf : Source.Function) → (tf : IR.Function) → Option (PrivateFunctionDiagram source target sf tf)

theorem PrivateFunctionDiagram.call_open (σ : Subst inputWidths)
    {source : Source.Function} {target : IR.Function} (s : Source.Program) (t : IR.Program)
    (d : PrivateFunctionDiagram s t source target) (observationType : Source.IntType)
    (caller : SourceReturnContext) (a : IR.Activation) (nextFrame : Nat) (stack : List IR.ReturnFrame)
    (f : IR.Function) (b : IR.Block) (inst : IR.Instruction) (memory : IR.Memory)
    (safe : memory.WellAllocated) (fresh : memory.BeforeFrame nextFrame)
    (sourceFound : s.functions.find? (·.symbol == source.symbol) = some source)
    (function : t.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.name == a.block) = some b)
    (atPc : b.instructions[a.pc]? = some inst)
    (call : inst.op = .call target.name)
    (targetFound : t.functions.find? (·.name == target.name) = some target)
    (arguments : inst.operands.mapM (IR.lookup a.values) =
      .ok (privateInputValues d.tokens ((parameterWords source.parameters).map (CWord.denote σ)) memory))
    (results : inst.results.map IR.Binding.type = target.returns) :
    Transition.OpenCheckpoint (Source.step s) (IR.step t) observeSource (observeTarget observationType)
      (PrivateFunctionReturn memory nextFrame source.symbol source.returnType.type d.tokens (some caller)
        (⟨{a with pc := a.pc+1}, inst.results⟩ :: stack))
      ⟨.call source.symbol source.returnType.type
          ((parameterWords source.parameters).map (CWord.denote σ)),
        caller.locals, caller.continuation, caller.functionId⟩
      ⟨.running a, stack, nextFrame⟩ := by
  have stored := parameter_store_sound σ source.parameters (parameterWords source.parameters) d.locals d.storage
  have sourceStep : Source.Step s
      ⟨.call source.symbol source.returnType.type
          ((parameterWords source.parameters).map (CWord.denote σ)),
        caller.locals, caller.continuation, caller.functionId⟩
      ⟨.statement source.body, d.locals.denote σ,
        .function (some (caller.functionId, caller.locals)) source.returnType.type ::
          caller.continuation, source.symbol⟩ := by
    simp [Source.Step, Source.step, Source.enterFunction, sourceFound, stored]
  let values : List IR.Value := [.memory memory, .world]
  have typedTokens := private_token_typed values [.memory, .world] rfl
  have binding := bind_slots_sound σ (privateToken values) (privateTokenType [.memory, .world]) typedTokens _ _ _ d.binding
  have types : ((loopInputSlots d.tokens (parameterWords source.parameters)).map
      (Slot.denote σ (privateToken values))).map IR.Value.type = target.parameters := by
    rw [d.signature, List.map_map]
    apply List.map_congr_left
    intro slot _
    exact slot_type_sound σ (privateToken values) (privateTokenType [.memory, .world]) typedTokens slot
  have entered : IR.enter target (privateInputValues d.tokens
      ((parameterWords source.parameters).map (CWord.denote σ)) memory) nextFrame =
      .ok ⟨target.name, d.block.name, 0, d.scope.denote σ (privateToken values), nextFrame⟩ := by
    rw [← private_input_values]
    simp [IR.enter, types, d.head, d.parameters, binding, Bind.bind, Except.bind, pure, Except.pure, values]
  have targetStep := IR.call_enters t ⟨.running a, stack, nextFrame⟩ a
    ⟨target.name, d.block.name, 0, d.scope.denote σ (privateToken values), nextFrame⟩
    f target b inst _ rfl function block atPc call targetFound arguments results entered
  have body := d.body σ memory (some caller) nextFrame (nextFrame+1)
    (⟨{a with pc := a.pc+1}, inst.results⟩ :: stack) observationType safe fresh (Nat.lt_succ_self _)
  apply body.prepend
  · simpa only [SourceCursor.denote, CutControl.denote, sourceContextFrames, SavedFrames.denote,
      List.map_cons, List.map_nil, Source.Frame.map, List.cons_append, List.nil_append]
      using Transition.Reaches.next sourceStep (.refl _)
  · cases targetStep with | execute h => exact .next h (.refl _)

theorem private_context_frontier_prefix (source : Source.Program) (ir : IR.Program) (target : IR.Function)
    (points : List (PrivateLayoutPoint target)) (fid : Nat) (tokens : Bool) (type : Source.IntType)
    (cursor : SourceCursor inputWidths) (label : String) (pc : Nat) (config : PrivateConfig inputWidths)
    (st : CutTree inputWidths (SourceCursor inputWidths)) (tt : CutTree inputWidths (PrivateExit inputWidths))
    (sp : SourceCutRuns source false cursor st) (tp : PrivateCutRuns ir target tokens false label pc config tt)
    (path : PathConditions inputWidths)
    (matched : CutMatchUnder (fun σ s t => ∀ incoming context owner stack final observationType,
      t.Realizes σ incoming tokens target owner stack final →
      PrivateContextOpen source ir target points incoming fid type observationType tokens context owner stack
        (s.denote σ fid context) final) path st.normalizeTrace tt.normalizeTrace)
    (σ : Subst inputWidths) (holds : path.Holds σ) (values : List IR.Value) (incoming : IR.Memory)
    (context : Option SourceReturnContext) (owner frame : Nat) (stack : List IR.ReturnFrame)
    (observationType : Source.IntType) (related : config.Realizes σ values incoming owner frame) :
    PrivateContextOpen source ir target points incoming fid type observationType tokens context owner stack
      (cursor.denote σ fid context) (privateState σ values target label pc owner frame config stack) := by
  obtain ⟨sf, ss, _, so⟩ := sp σ fid context
  obtain ⟨tf, ts, _, to⟩ := tp σ values incoming owner frame stack related
  have agreement := matched σ holds
  rw [CutTree.normalize_trace_sound, CutTree.normalize_trace_sound] at agreement
  rcases agreement with ⟨fault, sh, th⟩ | ⟨se, te, sh, th, continued⟩
  · have sh' : sf.control = .halted (.error fault) := by simpa [sh] using so
    have th' : tf.control = .halted (.error fault) := by simpa [th] using to
    obtain ⟨code, rfl, observed⟩ := st.fault_observation σ fault sh
    apply Transition.OpenCheckpoint.of_refines
    apply Transition.refines_of_terminals ss.toReaches ts.toReaches
      (by simp [Source.step, sh']) (by simp [IR.step, th']) (o := Observation.diagnostic code)
    · simpa [observeSource, sh'] using observed
    · simpa [observeTarget, th'] using observed
  · have sh' : sf = se.denote σ fid context := by simpa [sh] using so
    have outcome : te.Realizes σ incoming tokens target owner stack tf := by simpa [th] using to
    rw [sh'] at ss
    exact (continued incoming context owner stack tf observationType outcome).prepend ss.toReaches ts.toReaches

theorem private_call_input_values (σ : Subst inputWidths) (values : List IR.Value)
    (config : PrivateConfig inputWidths) (tokens : Bool) (args : List (CWord inputWidths))
    (memory : IR.Memory) (current : config.memoryValue values = some memory)
    (world : privateToken values config.world = .world) :
    (privateCallInputSlots tokens config args).map (Slot.denote σ (privateToken values)) =
      privateInputValues tokens (args.map (CWord.denote σ)) memory := by
  have memoryToken := config.current_token current
  cases tokens <;> simp [privateCallInputSlots, privateInputValues, Slot.denote, memoryToken, world,
    Word.denote, CWord.word, CWord.denote, sourceValueIR, List.map_map, Function.comp_def]

set_option maxHeartbeats 1000000 in
def checkPrivateContextCall (source : Source.Program) (ir : IR.Program) (target : IR.Function)
    (points : List (PrivateLayoutPoint target)) (fid : Nat) (tokens : Bool) (type : Source.IntType)
    (callee : PrivateFunctionBuilder source ir)
    (continuation : PrivateContextContinuation source ir target points fid tokens type)
    (path : PathConditions inputWidths) (s : SourceCursor inputWidths)
    (label : String) (pc : Nat) (config : PrivateConfig inputWidths) :
    Option (PLift (∀ σ, path.Holds σ → ∀ values incoming context owner frame stack observationType,
      config.Realizes σ values incoming owner frame →
      PrivateContextOpen source ir target points incoming fid type observationType tokens context owner stack
        (s.denote σ fid context) (privateState σ values target label pc owner frame config stack))) := do
  match sourceCursor : s with
  | ⟨.call id returnType args, locals, frames⟩ =>
    let sf : {g // source.functions.find? (·.symbol == id) = some g} ←
      match found : source.functions.find? (·.symbol == id) with
      | none => none
      | some g => some ⟨g, found⟩
    have sourceId : sf.val.symbol = id := by simpa using List.find?_some sf.property
    if returnTyped : returnType = sf.val.returnType.type then
      if argsTyped : args.map CWord.type = sf.val.parameters.map (fun p => p.type.type) then
        let tf : {g // ir.functions.find? (·.name == sf.val.name) = some g} ←
          match found : ir.functions.find? (·.name == sf.val.name) with
          | none => none
          | some g => some ⟨g, found⟩
        have targetName : tf.val.name = sf.val.name := by simpa using List.find?_some tf.property
        let d ← callee sf.val tf.val
        if function : ir.functions.find? (·.name == target.name) = some target then
          let b : {b // target.blocks.find? (·.name == label) = some b} ←
            match found : target.blocks.find? (·.name == label) with
            | none => none
            | some b => some ⟨b, found⟩
          have blockName : b.val.name = label := by simpa using List.find?_some b.property
          let inst : {i // b.val.instructions[pc]? = some i} ←
            match found : b.val.instructions[pc]? with
            | none => none
            | some i => some ⟨i, found⟩
          if call : inst.val.op = .call tf.val.name then
            if operands : inst.val.operands.mapM (fun id => config.scope.lookup id) =
                some (privateCallInputSlots d.tokens config args) then
              if results : inst.val.results.map IR.Binding.type = tf.val.returns then
                let bound : {out // config.scope.forReturn.appendPrivate
                    (config.tokens ++ privateCallOutputTypes d.tokens sf.val.returnType.type) inst.val.results
                    (privateCallOutputSlots d.tokens config sf.val.returnType.type) = some out} ←
                  match binding : config.scope.forReturn.appendPrivate
                      (config.tokens ++ privateCallOutputTypes d.tokens sf.val.returnType.type) inst.val.results
                      (privateCallOutputSlots d.tokens config sf.val.returnType.type) with
                  | none => none
                  | some out => some ⟨out, binding⟩
                if shape : (privateCallConfig d.tokens config bound.val sf.val.returnType.type).WellShaped = true then
                  let resumed ← continuation path.forReturn
                    ⟨.value (inputWord sf.val.returnType.type 0), locals.forReturn, frames.forReturn⟩
                    b.val.name (pc+1) (privateCallConfig d.tokens config bound.val sf.val.returnType.type)
                  return ⟨by
                    rw [sourceCursor]
                    intro σ holds values incoming context owner frame stack observationType related
                    obtain ⟨memory, current, safe, memoryFrame, _, _⟩ := related.2.2.2.2
                    have typed : (args.map (CWord.denote σ)).map Source.Value.type =
                        sf.val.parameters.map (fun p => p.type.type) := by
                      simpa only [List.map_map, Function.comp_def, CWord.denote] using argsTyped
                    have coverage := parameter_words_cover sf.val.parameters (args.map (CWord.denote σ)) typed
                    have actualArgs := slots_lookup_sound σ (privateToken values) config.scope inst.val.operands
                      (privateCallInputSlots d.tokens config args) operands
                    rw [private_call_input_values σ values config d.tokens args memory current related.2.2.1] at actualArgs
                    let caller : SourceReturnContext :=
                      ⟨fid, locals.denote σ, sourceContextFrames context (frames.denote σ)⟩
                    have openCall := d.call_open (inputSubst (args.map (CWord.denote σ))) source ir observationType
                      caller ⟨target.name, b.val.name, pc, config.scope.denote σ (privateToken values), owner⟩
                      frame stack target b.val inst.val memory safe memoryFrame
                      (by simpa only [sourceId] using sf.property) function
                      (by simpa only [blockName] using b.property) inst.property call
                      (by simpa only [targetName] using tf.property)
                      (by simpa only [coverage] using actualArgs) results
                    have complete := openCall.bind
                      (nextBoundary := fun a b => PrivateJointRelated target points incoming fid context owner stack a b ∨
                        PrivateFunctionReturn incoming owner fid type tokens context stack a b) (by
                        rintro a c ⟨value, valueTyped, _, returnedMemory, afterFrame, sourceState,
                          returned, returnedSafe, returnedFrame, future, newOwners, targetState⟩
                        subst a
                        subst c
                        let newValues := values ++ privateCallValues d.tokens value returnedMemory
                        have nextRelated := private_call_realizes σ values incoming memory returnedMemory owner frame afterFrame
                          config bound.val d.tokens sf.val.returnType.type value related current valueTyped
                          returned returnedSafe returnedFrame newOwners future shape
                        have suffix := resumed.down (returnSubst σ value)
                          ((path.for_return_sound σ value).mpr holds) newValues incoming context owner afterFrame
                          stack observationType nextRelated
                        have fresh : (inputWord sf.val.returnType.type 0).denote (returnSubst σ value) = value := by
                          simpa only [valueTyped] using return_subst_fresh σ value
                        have sourceEqual :
                            (⟨.value (inputWord sf.val.returnType.type 0), locals.forReturn, frames.forReturn⟩ :
                              SourceCursor inputWidths).denote (returnSubst σ value) fid context =
                            returnedSource sf.val.symbol (some caller) ([]) value := by
                          cases context <;>
                            simp [SourceCursor.denote, CutControl.denote, fresh, local_store_for_return,
                              saved_frames_for_return, sourceContextFrames, caller, returnedSource]
                        have binding := private_call_binding σ values config bound.val sf.val.returnType.type value
                          returnedMemory d.tokens inst.val.results related.2.1 valueTyped related.1 bound.property
                        have targetEqual :
                            privateState (returnSubst σ value) newValues target b.val.name (pc+1) owner afterFrame
                              (privateCallConfig d.tokens config bound.val sf.val.returnType.type) stack =
                            cutResume (privateCallValues d.tokens value returnedMemory) afterFrame
                              (⟨⟨target.name, b.val.name, pc+1, config.scope.denote σ (privateToken values), owner⟩,
                                inst.val.results⟩ :: stack) := by
                          simp [privateState, privateCallConfig, cutResume, binding, newValues]
                        rw [sourceEqual, targetEqual] at suffix
                        exact .inr (by simpa only [PrivateContextOpen, returnedSource, privateCallValues] using suffix))
                    cases context <;>
                      simpa only [PrivateContextOpen, sourceContextFrames, SourceCursor.denote, CutControl.denote,
                        privateState, caller, coverage, returnTyped, sourceId, blockName] using complete⟩
                else none
              else none
            else none
          else none
        else none
      else none
    else none
  | _ => continuation path s label pc config

set_option maxHeartbeats 1000000 in
def privateContextCallerSuffix (source : Source.Program) (ir : IR.Program) (target : IR.Function)
    (points : List (PrivateLayoutPoint target)) (fid : Nat) (tokens : Bool) (type : Source.IntType)
    (callee : PrivateFunctionBuilder source ir) :
    Nat → PrivateContextContinuation source ir target points fid tokens type
  | 0, _, _, _, _, _ => none
  | budget+1, path, cursor, label, pc, config => do
    if let some span := privateContextFiniteSpan source ir target points fid tokens type (budget+1) path cursor label pc config then
      return span
    let sp ← sourceOpenFrontier source (budget+1) cursor
    let tp ← privateCutDiagram ir target tokens type [] true (budget+1) false config label pc
    let joined ← cutMatchUnder (fun path s t =>
      match t with
      | .returned _ _ => none
      | .next label pc config => do
        let proof ← checkPrivateContextCall source ir target points fid tokens type callee
          (privateContextCallerSuffix source ir target points fid tokens type callee budget) path s label pc config
        return ⟨by
          intro σ holds incoming context owner stack final observationType outcome
          obtain ⟨values, frame, related, rfl⟩ := outcome
          exact proof.down σ holds values incoming context owner frame stack observationType related⟩)
      (budget+1) path sp.val.normalizeTrace tp.val.normalizeTrace
    return ⟨fun σ holds values incoming context owner frame stack observationType related =>
      private_context_frontier_prefix source ir target points fid tokens type cursor label pc config
        sp.val tp.val sp.property tp.property path joined.down σ holds values incoming context owner frame
        stack observationType related⟩

end EvoIR.Core.Symbolic
