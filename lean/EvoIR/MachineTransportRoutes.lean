import EvoIR.MachineTransportTerms

/-! Concrete finite paths through actual critical-edge blocks. The bound in
routeOrigins concerns only inserted administrative blocks; original CFG
backedges and call executions remain unbounded. -/
namespace EvoIR.Machine.Transport

theorem routeOrigins_found (source target : Function) (plans : FunctionPlan)
    (expected fuel current : Nat) (env after : Origins)
    (accepted : routeOrigins source target plans expected fuel current env = some after) :
    ∃ block, findBlock target current = some block := by
  cases fuel with
  | zero => simp [routeOrigins] at accepted
  | succ fuel =>
    by_cases arrived : current = expected
    · cases found : findBlock target current with
      | none => simp [routeOrigins, if_pos arrived, found] at accepted
      | some block => exact ⟨block, rfl⟩
    · cases original : findBlock source current with
      | some block => simp [routeOrigins, arrived, original] at accepted
      | none =>
        cases found : findBlock target current with
        | none => simp [routeOrigins, arrived, original, found] at accepted
        | some block => exact ⟨block, rfl⟩

theorem step_postSSA_b (program : Program) (state : State) (activation : Activation)
    (function : Function) (block successor : Block) (destination : Nat)
    (running : state.control = .running activation)
    (foundFunction : program.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (atTerminator : activation.pc = block.insts.length)
    (branch : block.term = .b destination [])
    (foundSuccessor : function.blocks.find? (·.id == destination) = some successor) :
    step .postSSA program state = some {state with control := .running {activation with block := destination, pc := 0}} := by
  have noInstruction : block.insts[activation.pc]? = none := by simp [atTerminator]
  have executed : execute .postSSA program state activation = .ok {state with control := .running {activation with block := destination, pc := 0}} := by
    rw [execute_terminator .postSSA program state activation function block foundFunction foundBlock
      noInstruction atTerminator]
    simp [branch, Demand.evalTerminator, finishTerminator, Demand.resume, jump, foundSuccessor,
      Bind.bind, Except.bind, Pure.pure, Except.pure]
  simp only [step, running, executed]

theorem routeOrigins_steps (source target : Function) (plans : FunctionPlan)
    (expected fuel current : Nat) (env after : Origins)
    (accepted : routeOrigins source target plans expected fuel current env = some after)
    (program : Program) (state : State) (activation : Activation)
    (running : state.control = .running activation)
    (foundFunction : program.functions.find? (·.name == activation.function) = some target)
    (atBlock : activation.block = current) (atStart : activation.pc = 0)
    (valuation : Origin → Slot) (related : Realizes env valuation activation.regs)
    (normalized : OriginsNormalized valuation) :
    ∃ block plan regs,
      findBlock target expected = some block ∧ findPlan plans expected = some plan ∧
      plan.prefixLength ≤ block.insts.length ∧
      Core.Transition.Reaches (step .postSSA program) state
        {state with control := .running {activation with block := expected, pc := plan.prefixLength, regs}} ∧
      Realizes after valuation regs := by
  induction fuel generalizing current env state activation with
  | zero => simp [routeOrigins] at accepted
  | succ fuel ih =>
    by_cases arrived : current = expected
    · cases foundBlock : findBlock target current with
      | none => simp [routeOrigins, if_pos arrived, foundBlock] at accepted
      | some block =>
        cases foundPlan : findPlan plans current with
        | none => simp [routeOrigins, if_pos arrived, foundBlock, foundPlan] at accepted
        | some plan =>
          by_cases tooLong : plan.prefixLength > block.insts.length
          · simp [routeOrigins, if_pos arrived, foundBlock, foundPlan, tooLong] at accepted
          · have copied : copyOrigins (block.insts.take plan.prefixLength) env = some after := by
              simpa [routeOrigins, if_pos arrived, foundBlock, foundPlan, tooLong] using accepted
            have prefixBound : plan.prefixLength ≤ block.insts.length := by omega
            have actualBlock : target.blocks.find? (·.id == activation.block) = some block := by
              simpa only [atBlock, findBlock] using foundBlock
            have located : ContainsSpan block activation.pc (block.insts.take plan.prefixLength) := by
              simpa only [atStart, List.drop_zero] using containsSpan_dropTake block 0 plan.prefixLength
            obtain ⟨regs, path, finalReal⟩ := copyOrigins_steps .postSSA program state activation target
              block (block.insts.take plan.prefixLength) env after copied valuation running foundFunction
              actualBlock located related normalized
            refine ⟨block, plan, regs, ?_, ?_, prefixBound, ?_, finalReal⟩
            · simpa only [← arrived] using foundBlock
            · simpa only [← arrived] using foundPlan
            · have length := List.length_take_of_le prefixBound
              have sameBlock : activation.block = expected := atBlock.trans arrived
              simpa only [atStart, length, Nat.zero_add, ← sameBlock] using path.reaches
    · cases original : findBlock source current with
      | some block => simp [routeOrigins, arrived, original] at accepted
      | none =>
        cases foundBlock : findBlock target current with
        | none => simp [routeOrigins, arrived, original, foundBlock] at accepted
        | some block =>
          by_cases noArgs : block.args = []
          · cases copied : copyOrigins block.insts env with
            | none => simp [routeOrigins, arrived, original, foundBlock, noArgs, copied] at accepted
            | some middleEnv =>
              cases branch : block.term <;>
                try simp [routeOrigins, arrived, original, foundBlock, noArgs, copied, branch] at accepted
              case b destination args =>
                cases args with
                | cons _ _ => simp [routeOrigins, arrived, original, foundBlock, noArgs, copied, branch] at accepted
                | nil =>
                  have tailAccepted : routeOrigins source target plans expected fuel destination middleEnv = some after := by
                    simpa [routeOrigins, arrived, original, foundBlock, noArgs, copied, branch] using accepted
                  obtain ⟨successor, foundSuccessor⟩ := routeOrigins_found source target plans expected fuel destination
                    middleEnv after tailAccepted
                  have actualBlock : target.blocks.find? (·.id == activation.block) = some block := by
                    simpa only [atBlock, findBlock] using foundBlock
                  have located : ContainsSpan block activation.pc block.insts := by
                    simpa only [atStart, List.drop_zero] using containsSpan_drop block 0
                  obtain ⟨middleRegs, copiesPath, middleReal⟩ := copyOrigins_steps .postSSA program state activation
                    target block block.insts env middleEnv copied valuation running foundFunction actualBlock
                    located related normalized
                  let middleActivation := {activation with pc := block.insts.length, regs := middleRegs}
                  let middleState := {state with control := .running middleActivation}
                  have copiesReach : Core.Transition.Reaches (step .postSSA program) state middleState := by
                    simpa only [atStart, Nat.zero_add, middleState, middleActivation] using copiesPath.reaches
                  have edge := step_postSSA_b program middleState middleActivation target block successor destination
                    rfl foundFunction actualBlock rfl branch foundSuccessor
                  let nextActivation := {activation with block := destination, pc := 0, regs := middleRegs}
                  let nextState := {state with control := .running nextActivation}
                  obtain ⟨finalBlock, finalPlan, regs, blockAtEnd, planAtEnd, prefixBound, tailPath, finalReal⟩ :=
                    ih destination middleEnv tailAccepted nextState nextActivation rfl foundFunction rfl rfl middleReal
                  refine ⟨finalBlock, finalPlan, regs, blockAtEnd, planAtEnd, prefixBound, ?_, finalReal⟩
                  have fullPath := copiesReach.trans (.next edge tailPath)
                  simpa only [middleState, middleActivation, nextState, nextActivation] using fullPath
          · simp [routeOrigins, arrived, original, foundBlock, noArgs] at accepted

end EvoIR.Machine.Transport
