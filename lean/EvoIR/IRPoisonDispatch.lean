import EvoIR.IRPoisonState

/-! Factorizations of the actual EvoIR dispatcher and its checked term/call
cases. These equations include lookup failures, private frame closure, every
logical call argument and the actual caller result bindings. -/
namespace EvoIR.Core.IR.Poison

def resume (state : State) (next : Except Source.Fault Activation) : Except Source.Fault State := do
  return {state with control := .running (← next)}

theorem resume_related {plan : DemandPlan} {source target : State}
    (states : StatesRelated plan source target) {before after : Except Source.Fault Activation}
    (active : ExceptRelated (ActivationsRelated plan) before after) :
    ExceptRelated (StatesRelated plan) (resume source before) (resume target after) :=
  except_bind_related active (fun _ _ related => states.replaceRunning related)

def termDispatch (f : Function) (state : State) (activation : Activation) :
    Terminator → Except Source.Fault State
  | .branch edge => resume state (jump f activation edge)
  | .conditional id yes no => do
    let condition ← (← IR.lookup activation.values id).asBits 1
    resume state (jump f activation (if condition.toNat == 0 then no else yes))
  | .return ids => do
    let values ← ids.mapM (IR.lookup activation.values)
    if values.map Value.type != f.returns then throw (.malformed "IR return types")
    let values := values.map fun value => match value with
      | .memory memory => Value.memory (memory.endFrame activation.owner)
      | _ => value
    match state.stack with
    | [] =>
      if values.any (fun value => match value with | .poison _ => true | _ => false) then
        throw (.diagnostic 3001)
      return {state with control := .halted (.ok values)}
    | frame :: rest => do
      let bound ← appendResults frame.caller.values frame.results values
      return {state with control := .running {frame.caller with values := bound}, stack := rest}

theorem termDispatch_related (plan : DemandPlan) (f : Function) (source target : State)
    (states : StatesRelated plan source target) (before after : Activation)
    (active : ActivationsRelated plan before after) (name : before.function = f.name)
    (term : Terminator) (uses : ∀ id ∈ testUses term, id ∈ neededFor plan f.name)
    (checked : ∀ edge ∈ edges term, edgeValid (neededFor plan f.name) f edge = true) :
    ExceptRelated (StatesRelated plan) (termDispatch f source before term)
      (termDispatch (rewriteFunction plan f) target after term) := by
  have banks : BanksRelated (neededFor plan f.name) before.values after.values := name ▸ active.values
  cases term with
  | branch edge =>
    exact resume_related states (jump_related plan f edge (checked edge (by simp [edges]))
      before after active name)
  | conditional id yes no =>
    have read := banks.needed_read id (uses id (by simp [testUses]))
    simp only [termDispatch, read]
    apply except_bind_related (relation := Eq)
    · cases IR.lookup after.values id <;> simp [ExceptRelated]
    · intro beforeValue afterValue equal
      subst afterValue
      apply except_bind_related (relation := Eq)
      · cases beforeValue.asBits 1 <;> simp [ExceptRelated]
      · intro beforeBits afterBits equal
        subst afterBits
        apply resume_related states
        apply jump_related plan f _ _ before after active name
        split <;> apply checked <;> simp [edges]
  | «return» ids =>
    have read := banks.reads ids (by simpa only [testUses] using uses)
    simp only [termDispatch, read, rewriteFunction]
    apply except_bind_related (relation := Eq)
    · cases ids.mapM (IR.lookup after.values) <;> simp [ExceptRelated]
    · intro beforeValues afterValues equal
      subst afterValues
      split
      · rfl
      · simp only [Bind.bind, Except.bind, Pure.pure, Except.pure, active.owner]
        exact return_related plan source target states _

theorem execute_term (program : Program) (state : State) (activation : Activation)
    (f : Function) (block : Block)
    (functionAt : program.functions.find? (·.name == activation.function) = some f)
    (blockAt : f.blocks.find? (·.name == activation.block) = some block)
    (instructionAt : block.instructions[activation.pc]? = none)
    (atEnd : activation.pc = block.instructions.length) :
    execute program state activation = termDispatch f state activation block.terminator := by
  simp only [execute, functionAt, blockAt, instructionAt]
  simp only [atEnd, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
  cases block.terminator <;> rfl

theorem execute_noncall (program : Program) (state : State) (activation : Activation)
    (f : Function) (block : Block) (inst : Instruction)
    (functionAt : program.functions.find? (·.name == activation.function) = some f)
    (blockAt : f.blocks.find? (·.name == activation.block) = some block)
    (instructionAt : block.instructions[activation.pc]? = some inst)
    (notCall : ∀ name, inst.op ≠ .call name) :
    execute program state activation = do
      let values ← evalAppend activation.owner inst activation.values
      return {state with control := .running {activation with pc := activation.pc+1, values}} := by
  simp only [execute, functionAt, blockAt, instructionAt, evalAppend]
  cases operation : inst.op <;> try (rename_i name; exact False.elim (notCall name operation))
  all_goals cases arguments : inst.operands.mapM (IR.lookup activation.values) with
    | error _ => simp [arguments, Bind.bind, Except.bind, Pure.pure, Except.pure]
    | ok args =>
      cases evaluated : evalOperation activation.owner inst.op args <;>
        simp only [operation] at evaluated <;>
        simp [arguments, operation, evaluated, Bind.bind, Except.bind, Pure.pure, Except.pure]

def callDispatch (program : Program) (state : State) (activation : Activation)
    (inst : Instruction) (name : String) : Except Source.Fault State := do
  let args ← inst.operands.mapM (IR.lookup activation.values)
  let some callee := program.functions.find? (·.name == name) | throw (.malformed "IR external call")
  if inst.results.map Binding.type != callee.returns then throw (.malformed "IR call results")
  return ⟨.running (← enter callee args state.nextFrame),
    ⟨{activation with pc := activation.pc+1}, inst.results⟩ :: state.stack, state.nextFrame+1⟩

theorem execute_call (program : Program) (state : State) (activation : Activation)
    (f : Function) (block : Block) (inst : Instruction) (name : String)
    (functionAt : program.functions.find? (·.name == activation.function) = some f)
    (blockAt : f.blocks.find? (·.name == activation.block) = some block)
    (instructionAt : block.instructions[activation.pc]? = some inst)
    (call : inst.op = .call name) :
    execute program state activation = callDispatch program state activation inst name := by
  simp only [execute, functionAt, blockAt, instructionAt, call, callDispatch]
  cases arguments : inst.operands.mapM (IR.lookup activation.values) <;>
    simp only [arguments, Bind.bind, Except.bind]
  all_goals rfl

theorem callDispatch_related (plan : DemandPlan) (program : Program) (source target : State)
    (states : StatesRelated plan source target) (before after : Activation)
    (active : ActivationsRelated plan before after) (inst : Instruction) (name : String)
    (demanded : ∀ id ∈ inst.operands, id ∈ neededFor plan before.function) :
    ExceptRelated (StatesRelated plan) (callDispatch program source before inst name)
      (callDispatch (rewriteProgram plan program) target after inst name) := by
  simp only [callDispatch, active.values.reads inst.operands demanded]
  apply except_bind_related (relation := Eq)
  · cases inst.operands.mapM (IR.lookup after.values) <;> simp [ExceptRelated]
  · intro sourceArgs targetArgs equal
    subst targetArgs
    simp only [find_function]
    cases found : program.functions.find? (·.name == name) with
    | none => rfl
    | some callee =>
      simp only [Option.map_some, rewriteFunction]
      split
      · rfl
      · simp only [Bind.bind, Except.bind, Pure.pure, Except.pure]
        change ExceptRelated (StatesRelated plan)
          ((enter callee sourceArgs source.nextFrame).bind _) 
          ((enter (rewriteFunction plan callee) sourceArgs target.nextFrame).bind _)
        rw [enter_rewrite, states.nextFrame]
        apply except_bind_related (relation := Eq)
        · cases enter callee sourceArgs target.nextFrame <;> simp [ExceptRelated]
        · intro sourceEntered targetEntered equal
          subst targetEntered
          exact ⟨ActivationsRelated.refl plan sourceEntered, .cons active.advance states.stack,
            rfl⟩

end EvoIR.Core.IR.Poison
