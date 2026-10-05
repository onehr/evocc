import EvoIR.Core.OpenCallingLoop

/-! Context-parametric function correspondence. An accepted body returns the
same typed value to its actual suspended callers, or produces the same checked
diagnostic. It does not assume that either body terminates. -/
namespace EvoIR.Core.Symbolic

def returnedSource (fid : Nat) (context : Option SourceReturnContext)
    (locals : Source.Locals) (value : Source.Value) : Source.State :=
  match context with
  | none => ⟨.halted (.ok value), locals, [], fid⟩
  | some caller => ⟨.value value, caller.locals, caller.continuation, caller.functionId⟩

def FunctionReturn (fid : Nat) (type : Source.IntType) (tokens : Bool)
    (context : Option SourceReturnContext) (stack : List IR.ReturnFrame)
    (s : Source.State) (t : IR.State) : Prop :=
  ∃ value : Source.Value, value.type = type ∧ ∃ locals frame,
    s = returnedSource fid context locals value ∧ t = cutResume (valueIRs tokens value) frame stack

theorem returned_cursor {Γ : Var → Nat} (σ : Subst Γ) (fid : Nat)
    (context : Option SourceReturnContext) (cursor : SourceCursor Γ) (word : CWord Γ)
    (control : cursor.control = .returned word) (boundary : cursor.continuation = []) :
    cursor.denote σ fid context = returnedSource fid context (cursor.locals.denote σ) (word.denote σ) := by
  cases context <;>
    simp [SourceCursor.denote, control, boundary, returnedSource, CutControl.denote,
      sourceContextFrames, SavedFrames.denote]

def FunctionBoundary (points : List LayoutPoint) (fid : Nat) (target : IR.Function)
    (type : Source.IntType) (tokens : Bool) (context : Option SourceReturnContext)
    (owner : Nat) (stack : List IR.ReturnFrame) (s : Source.State) (t : IR.State) : Prop :=
  CutpointRelated (points.map LayoutPoint.joint) fid target s t context owner stack ∨
    FunctionReturn fid type tokens context stack s t

def ContextOpen (source : Source.Program) (ir : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (target : IR.Function) (type observationType : Source.IntType) (tokens : Bool)
    (context : Option SourceReturnContext) (owner : Nat) (stack : List IR.ReturnFrame) :=
  Transition.OpenCheckpoint (Source.step source) (IR.step ir) observeSource (observeTarget observationType)
    (FunctionBoundary points fid target type tokens context owner stack)

abbrev ContextContinuationBuilder (source : Source.Program) (ir : IR.Program)
    (points : List LayoutPoint) (fid : Nat) (target : IR.Function) (tokens : Bool) (type : Source.IntType) :=
  (path : PathConditions inputWidths) → (cursor : SourceCursor inputWidths) →
    (label : String) → (pc : Nat) → (scope : Scope inputWidths) →
    Option (PLift (∀ σ, path.Holds σ → ∀ context owner nextFrame stack observationType,
      ContextOpen source ir points fid target type observationType tokens context owner stack
        (cursor.denote σ fid context) (cutTargetState σ target label pc owner nextFrame scope stack)))

structure OpenFunctionDiagram (program : Source.Program) (ir : IR.Program)
    (source : Source.Function) (target : IR.Function) where
  tokens : Bool
  scope : Scope inputWidths
  locals : LocalStore inputWidths
  block : IR.Block
  signature : target.parameters = (loopInputSlots tokens (parameterWords source.parameters)).map (Slot.type callTokenTypes)
  parameters : block.parameters.map IR.Binding.type = target.parameters
  binding : bindSlots callTokenTypes block.parameters (loopInputSlots tokens (parameterWords source.parameters)) = some scope
  storage : parameterStore source.parameters (parameterWords source.parameters) = some locals
  head : target.blocks.head? = some block
  returns : target.returns = cutReturnTypes tokens source.returnType.type
  body : ∀ σ context owner frame stack observationType,
    Transition.OpenCheckpoint (Source.step program) (IR.step ir) observeSource (observeTarget observationType)
      (FunctionReturn source.symbol source.returnType.type tokens context stack)
      ((⟨.statement source.body, locals, [.function none source.returnType.type]⟩ :
        SourceCursor inputWidths).denote σ source.symbol context)
      (cutTargetState σ target block.name 0 owner frame scope stack)

abbrev OpenFunctionBuilder (source : Source.Program) (target : IR.Program) :=
  (s : Source.Function) → (t : IR.Function) → Option (OpenFunctionDiagram source target s t)

/-- Entry proof uses actual source parameter binding, actual target argument
binding, and the fresh frame counter. No caller frame or operand is invented. -/
theorem OpenFunctionDiagram.call_open (σ : Subst inputWidths)
    {source : Source.Function} {target : IR.Function} (s : Source.Program) (t : IR.Program)
    (d : OpenFunctionDiagram s t source target) (observationType : Source.IntType)
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
    (results : inst.results.map IR.Binding.type = target.returns) :
    Transition.OpenCheckpoint (Source.step s) (IR.step t) observeSource (observeTarget observationType)
      (FunctionReturn source.symbol source.returnType.type d.tokens (some caller)
        (⟨{a with pc := a.pc+1}, inst.results⟩ :: stack))
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
  have body := d.body σ (some caller) nextFrame (nextFrame+1)
    (⟨{a with pc := a.pc+1}, inst.results⟩ :: stack) observationType
  apply body.prepend
  · simpa only [SourceCursor.denote, CutControl.denote, sourceContextFrames, SavedFrames.denote,
      List.map_cons, List.map_nil, Source.Frame.map, List.cons_append, List.nil_append]
      using Transition.Reaches.next sourceStep (.refl _)
  · cases targetStep with | execute h => exact .next h (.refl _)

theorem context_return_binding {fid : Nat} {target : IR.Function} {returnType : Source.IntType}
    {locals : LocalStore inputWidths} {frames : SavedFrames inputWidths}
    {label : String} {pc : Nat} {scope : Scope inputWidths} {results : List IR.Binding}
    {calleeTokens : Bool} {boundScope : Scope inputWidths}
    (binding : scope.forReturn.appendSlots results
      (cutResults calleeTokens (inputWord returnType 0)) = some boundScope)
    (σ : Subst inputWidths) (value : Source.Value) (typed : value.type = returnType)
    (context : Option SourceReturnContext) (owner frame : Nat) (stack : List IR.ReturnFrame) :
    (⟨.value (inputWord returnType 0), locals.forReturn, frames.forReturn⟩ : SourceCursor inputWidths).denote
      (returnSubst σ value) fid context =
        ⟨.value value, locals.denote σ, sourceContextFrames context (frames.denote σ), fid⟩ ∧
    cutTargetState (returnSubst σ value) target label pc owner frame boundScope stack =
      cutResume (valueIRs calleeTokens value) frame
        (⟨⟨target.name, label, pc, scope.denote σ callTokens, owner⟩, results⟩ :: stack) := by
  have fresh : (inputWord returnType 0).denote (returnSubst σ value) = value := by
    simpa only [typed] using return_subst_fresh σ value
  have resultValues : (cutResults calleeTokens (inputWord returnType 0)).map
      (Slot.denote (returnSubst σ value) callTokens) = valueIRs calleeTokens value := by
    rw [cut_results_values, fresh]
  have bound := append_slots_sound (returnSubst σ value) scope.forReturn boundScope results
    (cutResults calleeTokens (inputWord returnType 0)) binding
  rw [scope_for_return, resultValues] at bound
  constructor
  · cases context <;>
      simp only [SourceCursor.denote, CutControl.denote, fresh, local_store_for_return, saved_frames_for_return,
        sourceContextFrames]
  · simp [cutResume, bound, cutTargetState, targetState]

end EvoIR.Core.Symbolic
