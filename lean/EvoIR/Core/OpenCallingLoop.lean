import EvoIR.Core.LoopReturn
import EvoIR.Core.ReturnContinuation
import EvoIR.Core.OpenCheckpoint

/-! A cyclic callee's return is an open boundary, not a precondition that its
caller already has a whole-execution proof. The actual call and return frames
are retained; the caller must subsequently close every returned state. -/
namespace EvoIR.Core.Symbolic

theorem checked_call_context_open {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (type observationType : Source.IntType)
    (source : Source.Program) (program : IR.Program) (target : IR.Function) (tokens : Bool)
    (spans : ∀ point ∈ entries, CheckedSpan source program entries fid target tokens type point)
    (caller : SourceReturnContext) (owner : Nat) (stack : List IR.ReturnFrame)
    (resumed : Source.State → IR.State → Prop)
    (returns : ∀ (σ : Subst Γ) (word : CWord Γ) afterCalls, word.type = type →
      resumed ⟨.value (word.denote σ), caller.locals, caller.continuation, caller.functionId⟩
        ((TargetExit.returned word).callState σ tokens target owner afterCalls stack))
    (s : Source.State) (t : IR.State)
    (entry : CutpointRelated entries fid target s t (some caller) owner stack) :
    Transition.OpenCheckpoint (Source.step source) (IR.step program)
      observeSource (observeTarget observationType) resumed s t := by
  refine ⟨fun a b => CutpointRelated entries fid target a b (some caller) owner stack, entry, ?_⟩
  intro a b related
  obtain ⟨point, member, σ, nextFrame, rfl, rfl⟩ := related
  let span := spans point member
  obtain ⟨sf, sp, positiveSource, sourceOutcome⟩ := span.source σ fid (some caller)
  obtain ⟨tf, tp, positiveTarget, targetOutcome⟩ := span.targetDiagram σ owner nextFrame stack
  have agreement := span.agreement.sound σ
  rw [CutTree.normalize_trace_sound, CutTree.normalize_trace_sound] at agreement
  rcases agreement with ⟨fault, sourceFailed, targetFailed⟩ |
    ⟨se, te, sourceSuccess, targetSuccess, matched⟩
  · have sh : sf.control = .halted (.error fault) := by simpa [sourceFailed] using sourceOutcome
    have th : tf.control = .halted (.error fault) := by simpa [targetFailed] using targetOutcome
    obtain ⟨code, rfl, observed⟩ := span.sourceTree.fault_observation σ fault sourceFailed
    exact .inl ⟨sf, tf, .diagnostic code, sp.toReaches, tp.toReaches,
      by simp [Source.step, sh], by simp [IR.step, th],
      by simpa [observeSource, sh] using observed,
      by simpa [observeTarget, th] using observed⟩
  · have sh : sf = se.denote σ fid (some caller) := by simpa [sourceSuccess] using sourceOutcome
    obtain ⟨afterCalls, th⟩ := (by simpa [targetSuccess] using targetOutcome :
      ∃ afterCalls, tf = te.callState σ tokens target owner afterCalls stack)
    subst sf
    subst tf
    refine .inr ⟨_, _, positiveSource rfl, positiveTarget rfl, ?_⟩
    cases matched with
    | returned control typed boundary same =>
      rename_i word targetWord
      rw [TargetExit.returned_congr σ tokens target owner afterCalls stack targetWord word (same σ)]
      apply Or.inr
      simpa only [SourceCursor.denote, control, boundary, sourceContextFrames,
        SavedFrames.denote, List.map_nil, List.nil_append] using returns σ _ afterCalls typed
    | successor closed => exact .inl (closed σ afterCalls (some caller) owner stack)

theorem LoopDiagram.call_open (σ : Subst inputWidths)
    {source : Source.Function} {target : IR.Function} (s : Source.Program) (t : IR.Program)
    (d : LoopDiagram s t source target) (observationType : Source.IntType)
    (caller : SourceReturnContext) (a : IR.Activation) (nextFrame : Nat) (stack : List IR.ReturnFrame)
    (f : IR.Function) (b : IR.Block) (inst : IR.Instruction)
    (sourceFound : s.functions.find? (·.symbol == source.symbol) = some source)
    (function : t.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.name == a.block) = some b)
    (atPc : b.instructions[a.pc]? = some inst)
    (call : inst.op = .call target.name)
    (targetFound : t.functions.find? (·.name == target.name) = some target)
    (arguments : inst.operands.mapM (IR.lookup a.values) =
      .ok (loopInputValues d.tokens ((parameterWords source.parameters).map (CWord.denote σ))))
    (results : inst.results.map IR.Binding.type = target.returns)
    (resumed : Source.State → IR.State → Prop)
    (returns : ∀ (τ : Subst inputWidths) (word : CWord inputWidths) afterCalls,
      word.type = source.returnType.type →
      resumed ⟨.value (word.denote τ), caller.locals, caller.continuation, caller.functionId⟩
        ((TargetExit.returned word).callState τ d.tokens target nextFrame afterCalls
          (⟨{a with pc := a.pc+1}, inst.results⟩ :: stack))) :
    Transition.OpenCheckpoint (Source.step s) (IR.step t) observeSource (observeTarget observationType)
      resumed
      ⟨.call source.symbol source.returnType.type
          ((parameterWords source.parameters).map (CWord.denote σ)),
        caller.locals, caller.continuation, caller.functionId⟩
      ⟨.running a, stack, nextFrame⟩ := by
  have stored := parameter_store_sound σ source.parameters (parameterWords source.parameters)
    d.locals d.storage
  have sourceStep : Source.Step s
      ⟨.call source.symbol source.returnType.type
          ((parameterWords source.parameters).map (CWord.denote σ)),
        caller.locals, caller.continuation, caller.functionId⟩
      ⟨.statement source.body, d.locals.denote σ,
        .function (some (caller.functionId, caller.locals)) source.returnType.type ::
          caller.continuation, source.symbol⟩ := by
    simp [Source.Step, Source.step, Source.enterFunction, sourceFound, stored]
  have binding := bind_slots_sound σ callTokens callTokenTypes (by intro id; rfl) _ _ _ d.binding
  have types : ((loopInputSlots d.tokens (parameterWords source.parameters)).map
      (Slot.denote σ callTokens)).map IR.Value.type = target.parameters := by
    rw [d.signature, List.map_map]
    apply List.map_congr_left
    intro slot _
    exact slot_type_sound σ callTokens callTokenTypes (by intro id; rfl) slot
  have entered : IR.enter target (loopInputValues d.tokens
      ((parameterWords source.parameters).map (CWord.denote σ))) nextFrame =
      .ok ⟨target.name, d.block.name, 0, d.scope.denote σ callTokens, nextFrame⟩ := by
    rw [← loop_input_values]
    simp [IR.enter, types, d.head, d.parameters, binding, Bind.bind, Except.bind, pure, Except.pure]
  have targetStep := IR.call_enters t ⟨.running a, stack, nextFrame⟩ a
    ⟨target.name, d.block.name, 0, d.scope.denote σ callTokens, nextFrame⟩
    f target b inst _ rfl function block atPc call targetFound arguments results entered
  have body := checked_call_context_open (d.points.map LayoutPoint.joint) source.symbol
    source.returnType.type observationType s t target d.tokens (fun point member => d.spans.get point member)
    caller nextFrame (⟨{a with pc := a.pc+1}, inst.results⟩ :: stack) resumed returns _ _
    (d.initialRelated σ (some caller) nextFrame (nextFrame+1) _)
  apply body.prepend
  · exact .next sourceStep (.refl _)
  · cases targetStep with | execute h => exact .next h (.refl _)

end EvoIR.Core.Symbolic
