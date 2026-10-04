import EvoIR.Core.LoopReturn
import EvoIR.Core.ReturnContinuation

/-! A cyclic callee is an open simulation: its return rejoins a proved caller
continuation instead of becoming a terminal observation. Both sides execute
positive actual paths to each cutpoint. Neither termination nor an iteration
bound is assumed. The caller relation remains an explicit proof obligation. -/
namespace EvoIR.Core.Symbolic

def CallingLoopRelated {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (target : IR.Function) (caller : SourceReturnContext)
    (owner : Nat) (stack : List IR.ReturnFrame)
    (resumed : Source.State → IR.State → Prop) (s : Source.State) (t : IR.State) : Prop :=
  CutpointRelated entries fid target s t (some caller) owner stack ∨ resumed s t

theorem checked_call_context_simulation {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (type observationType : Source.IntType)
    (source : Source.Program) (program : IR.Program) (target : IR.Function) (tokens : Bool)
    (spans : ∀ point ∈ entries, CheckedSpan source program entries fid target tokens type point)
    (caller : SourceReturnContext) (owner : Nat) (stack : List IR.ReturnFrame)
    (resumed : Source.State → IR.State → Prop)
    (continuation : Transition.CheckpointSimulation (Source.step source) (IR.step program)
      observeSource (observeTarget observationType) resumed)
    (returns : ∀ (σ : Subst Γ) (word : CWord Γ) afterCalls, word.type = type →
      resumed ⟨.value (word.denote σ), caller.locals, caller.continuation, caller.functionId⟩
        ((TargetExit.returned word).callState σ tokens target owner afterCalls stack)) :
    Transition.CheckpointSimulation (Source.step source) (IR.step program)
      observeSource (observeTarget observationType)
      (CallingLoopRelated entries fid target caller owner stack resumed) := by
  constructor
  intro s t related
  rcases related with related | continued
  · obtain ⟨point, member, σ, nextFrame, rfl, rfl⟩ := related
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
      | returned control typed boundary =>
        apply Or.inr
        simpa only [SourceCursor.denote, control, boundary, sourceContextFrames,
          List.map_nil, List.nil_append] using returns σ _ afterCalls typed
      | successor closed => exact .inl (closed σ afterCalls (some caller) owner stack)
  · rcases continuation.advance s t continued with terminal | ⟨sf, tf, sp, tp, related⟩
    · exact .inl terminal
    · exact .inr ⟨sf, tf, sp, tp, Or.inr related⟩

theorem checked_call_context_refines {Γ : Var → Nat} (entries : List (JointPoint Γ))
    (fid : Nat) (type observationType : Source.IntType)
    (source : Source.Program) (program : IR.Program) (target : IR.Function) (tokens : Bool)
    (spans : ∀ point ∈ entries, CheckedSpan source program entries fid target tokens type point)
    (caller : SourceReturnContext) (owner : Nat) (stack : List IR.ReturnFrame)
    (resumed : Source.State → IR.State → Prop)
    (continuation : Transition.CheckpointSimulation (Source.step source) (IR.step program)
      observeSource (observeTarget observationType) resumed)
    (returns : ∀ (σ : Subst Γ) (word : CWord Γ) afterCalls, word.type = type →
      resumed ⟨.value (word.denote σ), caller.locals, caller.continuation, caller.functionId⟩
        ((TargetExit.returned word).callState σ tokens target owner afterCalls stack))
    (s : Source.State) (t : IR.State)
    (entry : CutpointRelated entries fid target s t (some caller) owner stack) :
    LoweringRefines source program observationType s t :=
  (checked_call_context_simulation entries fid type observationType source program target tokens
    spans caller owner stack resumed continuation returns).sound (Or.inl entry)

/-- The accepted entry association is reusable in a real suspended caller,
for every typed argument valuation and every incoming frame counter. -/
theorem LoopDiagram.context_refines (σ : Subst inputWidths)
    {source : Source.Function} {target : IR.Function} (s : Source.Program) (t : IR.Program)
    (d : LoopDiagram s t source target) (observationType : Source.IntType)
    (caller : SourceReturnContext) (owner nextFrame : Nat) (stack : List IR.ReturnFrame)
    (resumed : Source.State → IR.State → Prop)
    (continuation : Transition.CheckpointSimulation (Source.step s) (IR.step t)
      observeSource (observeTarget observationType) resumed)
    (returns : ∀ (τ : Subst inputWidths) (word : CWord inputWidths) afterCalls,
      word.type = source.returnType.type →
      resumed ⟨.value (word.denote τ), caller.locals, caller.continuation, caller.functionId⟩
        ((TargetExit.returned word).callState τ d.tokens target owner afterCalls stack)) :
    LoweringRefines s t observationType
      ⟨.statement source.body, d.locals.denote σ,
        .function (some (caller.functionId, caller.locals)) source.returnType.type ::
          caller.continuation, source.symbol⟩
      (cutTargetState σ target d.block.name 0 owner nextFrame d.scope stack) := by
  apply checked_call_context_refines (d.points.map LayoutPoint.joint) source.symbol
    source.returnType.type observationType s t target d.tokens
    (fun point member => d.spans.get point member) caller owner stack resumed continuation returns
  simpa only [SourceCursor.denote, CutControl.denote, sourceContextFrames,
    List.map_cons, List.map_nil, List.cons_append, List.nil_append]
    using d.initialRelated σ (some caller) owner nextFrame stack

/-- Connect the open cyclic diagram to both actual call-entry transitions.
Argument evaluation and the post-return relation are separate obligations;
the callee body is neither replaced nor finitely unfolded. -/
theorem LoopDiagram.call_refines (σ : Subst inputWidths)
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
    (continuation : Transition.CheckpointSimulation (Source.step s) (IR.step t)
      observeSource (observeTarget observationType) resumed)
    (returns : ∀ (τ : Subst inputWidths) (word : CWord inputWidths) afterCalls,
      word.type = source.returnType.type →
      resumed ⟨.value (word.denote τ), caller.locals, caller.continuation, caller.functionId⟩
        ((TargetExit.returned word).callState τ d.tokens target nextFrame afterCalls
          (⟨{a with pc := a.pc+1}, inst.results⟩ :: stack))) :
    LoweringRefines s t observationType
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
  let simulation := checked_call_context_simulation (d.points.map LayoutPoint.joint) source.symbol
    source.returnType.type observationType s t target d.tokens (fun point member => d.spans.get point member)
    caller nextFrame (⟨{a with pc := a.pc+1}, inst.results⟩ :: stack) resumed continuation returns
  apply simulation.prepend (Or.inl (d.initialRelated σ (some caller) nextFrame (nextFrame+1) _))
  · exact .single sourceStep
  · cases targetStep with | execute h => exact .single h

/-- The caller suffix premise is discharged by the executable return checker.
This is an actual call-site theorem, not yet a source-entry admission theorem:
the argument/frontier construction must still supply the real call operands. -/
theorem LoopDiagram.call_checked_return (argumentsσ callerσ : Subst inputWidths)
    {source : Source.Function} {target : IR.Function} (s : Source.Program) (t : IR.Program)
    (d : LoopDiagram s t source target) (observationType : Source.IntType)
    (fid : Nat) (locals : LocalStore inputWidths) (frames : List Source.Frame)
    (scope : Scope inputWidths) (owner nextFrame pc : Nat)
    (f : IR.Function) (b : IR.Block) (inst : IR.Instruction) (callerTokens : Bool)
    (sourceFound : s.functions.find? (·.symbol == source.symbol) = some source)
    (function : t.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (atPc : b.instructions[pc]? = some inst)
    (call : inst.op = .call target.name)
    (targetFound : t.functions.find? (·.name == target.name) = some target)
    (arguments : inst.operands.mapM (IR.lookup (scope.denote callerσ callTokens)) =
      .ok (loopInputValues d.tokens ((parameterWords source.parameters).map (CWord.denote argumentsσ))))
    (results : inst.results.map IR.Binding.type = target.returns)
    (resumption : CheckedReturn s t fid f callerTokens observationType source.returnType.type
      locals frames b.name (pc+1) scope inst.results d.tokens) :
    LoweringRefines s t observationType
      ⟨.call source.symbol source.returnType.type
          ((parameterWords source.parameters).map (CWord.denote argumentsσ)),
        locals.denote callerσ, frames, fid⟩
      (cutTargetState callerσ f b.name pc owner nextFrame scope []) := by
  apply d.call_refines argumentsσ s t observationType
    ⟨fid, locals.denote callerσ, frames⟩
    ⟨f.name, b.name, pc, scope.denote callerσ callTokens, owner⟩ nextFrame []
    f b inst sourceFound function block atPc call targetFound arguments results
    _ resumption.diagram.simulation
  intro τ word afterCalls typed
  have resumed := resumption.related callerσ (word.denote τ) typed owner afterCalls
  simpa only [TargetExit.callState, cut_results_values] using resumed

end EvoIR.Core.Symbolic
