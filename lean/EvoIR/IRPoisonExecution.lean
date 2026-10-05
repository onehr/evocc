import EvoIR.IRPoisonDispatch

/-! Checked dead-poison replacement lifts through the actual IR machine.
All dispatcher faults remain identical, including malformed bindings; no
external totality, initialized-state or producer-equivalence premise is used. -/
namespace EvoIR.Core.IR.Poison

theorem execute_related (plan : DemandPlan) (program : Program)
    (valid : programValid plan program = true) (source target : State)
    (states : StatesRelated plan source target) (before after : Activation)
    (active : ActivationsRelated plan before after) :
    ExceptRelated (StatesRelated plan) (execute program source before)
      (execute (rewriteProgram plan program) target after) := by
  have targetFunction :
      (rewriteProgram plan program).functions.find? (·.name == after.function) =
      (program.functions.find? (·.name == before.function)).map (rewriteFunction plan) := by
    rw [find_function, ← active.function]
  cases foundFunction : program.functions.find? (·.name == before.function) with
  | none =>
    simp only [foundFunction, Option.map_none] at targetFunction
    simp only [execute, foundFunction, targetFunction, ExceptRelated]
    rfl
  | some f =>
    simp only [foundFunction, Option.map_some] at targetFunction
    have targetBlock :
        (rewriteFunction plan f).blocks.find? (·.name == after.block) =
        (f.blocks.find? (·.name == before.block)).map (rewriteBlock (neededFor plan f.name)) := by
      rw [find_block, ← active.block]
    cases foundBlock : f.blocks.find? (·.name == before.block) with
    | none =>
      simp only [foundBlock, Option.map_none] at targetBlock
      simp only [execute, foundFunction, targetFunction, foundBlock, targetBlock, ExceptRelated]
      rfl
    | some block =>
      simp only [foundBlock, Option.map_some] at targetBlock
      have name : before.function = f.name := by
        have name : f.name = before.function := by simpa using List.find?_some foundFunction
        exact name.symm
      have functionChecked := programValid_function plan program valid f (List.mem_of_find?_eq_some foundFunction)
      have blockChecked := functionValid_block (neededFor plan f.name) f functionChecked block
        (List.mem_of_find?_eq_some foundBlock)
      obtain ⟨used, tested, edgeChecked⟩ := blockValid_data _ _ _ blockChecked
      have targetInstruction : (rewriteBlock (neededFor plan f.name) block).instructions[after.pc]? =
          (block.instructions[before.pc]?).map (rewriteInstruction (neededFor plan f.name)) := by
        simp only [rewriteBlock, List.getElem?_map, ← active.pc]
      cases foundInstruction : block.instructions[before.pc]? with
      | none =>
        simp only [foundInstruction, Option.map_none] at targetInstruction
        by_cases atEnd : before.pc = block.instructions.length
        · have targetAtEnd : after.pc = (rewriteBlock (neededFor plan f.name) block).instructions.length := by
            simpa only [rewriteBlock, List.length_map, ← active.pc] using atEnd
          rw [execute_term program source before f block foundFunction foundBlock foundInstruction atEnd,
            execute_term (rewriteProgram plan program) target after (rewriteFunction plan f)
              (rewriteBlock (neededFor plan f.name) block) targetFunction targetBlock targetInstruction targetAtEnd]
          exact termDispatch_related plan f source target states before after active name block.terminator tested edgeChecked
        · simp only [execute, foundFunction, targetFunction, foundBlock, targetBlock,
            ← active.pc, rewriteBlock, List.getElem?_map, foundInstruction, Option.map_none,
            List.length_map]
          simp [atEnd, Bind.bind, Except.bind, ExceptRelated]
          rfl
      | some inst =>
        simp only [foundInstruction, Option.map_some] at targetInstruction
        have demanded : ∀ id ∈ inst.operands, id ∈ neededFor plan before.function := by
          simpa only [name] using used inst (List.mem_of_getElem? foundInstruction)
        have instructionRewrite : rewriteInstruction (neededFor plan f.name) inst =
            rewriteInstruction (neededFor plan before.function) inst := by rw [name]
        by_cases call : ∃ callee, inst.op = .call callee
        · obtain ⟨callee, isCall⟩ := call
          have same := rewrite_call (neededFor plan f.name) inst callee isCall
          rw [same] at targetInstruction
          rw [execute_call program source before f block inst callee foundFunction foundBlock foundInstruction isCall,
            execute_call (rewriteProgram plan program) target after (rewriteFunction plan f)
              (rewriteBlock (neededFor plan f.name) block) inst callee targetFunction targetBlock targetInstruction isCall]
          exact callDispatch_related plan program source target states before after active inst callee demanded
        · have notCall : ∀ callee, inst.op ≠ .call callee := by
            intro callee equal
            exact call ⟨callee, equal⟩
          have targetNotCall := rewrite_notCall (neededFor plan f.name) inst notCall
          rw [execute_noncall program source before f block inst foundFunction foundBlock foundInstruction notCall,
            execute_noncall (rewriteProgram plan program) target after (rewriteFunction plan f)
              (rewriteBlock (neededFor plan f.name) block) (rewriteInstruction (neededFor plan f.name) inst)
              targetFunction targetBlock targetInstruction targetNotCall]
          have evaluations : ExceptRelated (BanksRelated (neededFor plan before.function))
              (evalAppend before.owner inst before.values)
              (evalAppend after.owner (rewriteInstruction (neededFor plan f.name) inst) after.values) := by
            rw [← active.owner, instructionRewrite]
            exact evalAppend_related _ _ _ _ _ active.values demanded
          apply except_bind_related evaluations
          intro sourceBank targetBank banks
          exact states.replaceRunning (active.advance.replaceValues banks)

theorem step_related (plan : DemandPlan) (program : Program)
    (valid : programValid plan program = true) (source target targetAfter : State)
    (states : StatesRelated plan source target)
    (targetStep : step (rewriteProgram plan program) target = some targetAfter) :
    ∃ sourceAfter, step program source = some sourceAfter ∧ StatesRelated plan sourceAfter targetAfter := by
  have controls := states.control
  cases sourceControl : source.control with
  | halted result =>
    cases targetControl : target.control with
    | running _ => simp [sourceControl, targetControl, ControlsRelated] at controls
    | halted _ => simp [step, targetControl] at targetStep
  | running before =>
    cases targetControl : target.control with
    | halted _ => simp [sourceControl, targetControl, ControlsRelated] at controls
    | running after =>
      have active : ActivationsRelated plan before after := by
        simpa only [sourceControl, targetControl, ControlsRelated] using controls
      have evaluated := execute_related plan program valid source target states before after active
      cases sourceEval : execute program source before with
      | error fault =>
        cases targetEval : execute (rewriteProgram plan program) target after with
        | ok _ => simp [ExceptRelated, sourceEval, targetEval] at evaluated
        | error other =>
          have same : fault = other := by simpa only [ExceptRelated, sourceEval, targetEval] using evaluated
          simp only [step, targetControl, targetEval, Option.some.injEq] at targetStep
          subst targetAfter
          refine ⟨{source with control := .halted (.error fault)}, ?_, ?_⟩
          · simp only [step, sourceControl, sourceEval]
          · rw [same]
            exact states.replaceHalted (.error other)
      | ok sourceAfter =>
        cases targetEval : execute (rewriteProgram plan program) target after with
        | error _ => simp [ExceptRelated, sourceEval, targetEval] at evaluated
        | ok targetNext =>
          simp only [step, targetControl, targetEval, Option.some.injEq] at targetStep
          subst targetAfter
          refine ⟨sourceAfter, ?_, ?_⟩
          · simp only [step, sourceControl, sourceEval]
          · simpa only [ExceptRelated, sourceEval, targetEval] using evaluated

end EvoIR.Core.IR.Poison
