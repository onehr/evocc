import EvoIR.MachineTransportSteps
import EvoIR.MachineTransportWrites
import EvoIR.MachineDemandExecution

/-! Baseline DCE on actual reachable SSA instructions. Initialized operands
are obtained from established whole-source progress: a malformed pure
instruction would otherwise reach an unobserved terminal. Memory preservation
and the write footprint are proved by actual opcode dispatch. -/
namespace EvoIR.Machine.Transport

theorem checkDCE_pure (needed : List Nat) (inst : Instruction)
    (accepted : checkDCE needed inst = true) : inst.pure = true := by
  simp only [checkDCE, Bool.and_eq_true] at accepted
  exact accepted.1

theorem checkDCE_outside (needed : List Nat) (inst : Instruction)
    (accepted : checkDCE needed inst = true) (id : Nat) (live : id ∈ needed) :
    ∀ dst ∈ inst.defs, id ≠ dst.id := by
  intro dst member
  simp only [checkDCE, Bool.and_eq_true] at accepted
  have definitions := accepted.2
  exact of_decide_eq_true ((List.all_eq_true.mp ((List.all_eq_true.mp definitions) dst member)) id live)

theorem checkDCE_noCall (needed : List Nat) (inst : Instruction)
    (accepted : checkDCE needed inst = true) :
    ∀ symbol args results, inst ≠ .bl symbol args results := by
  intro symbol args results equal
  have pure := checkDCE_pure needed inst accepted
  rw [equal] at pure
  simp [Instruction.pure] at pure

/-- A general accepted deletion executes successfully on the actual source
path, does not change memory, and does not write any checked live register.
This is not an arbitrary-register-file totality claim or a producer type
assertion. The progress premise is discharged by the preceding exact SSA
selection refinement. -/
theorem checkDCE_reachable (p : Program) (type : Core.Source.IntType)
    (entry state : State) (needed : List Nat) (inst : Instruction)
    (accepted : checkDCE needed inst = true)
    (progress : ObservedProgress .ssa p type entry)
    (reachable : Core.Transition.Reaches (step .ssa p) entry state)
    (activation : Activation) (function : Function) (block : Block)
    (running : state.control = .running activation)
    (foundFunction : p.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (foundInstruction : block.insts[activation.pc]? = some inst) :
    ∃ regs,
      evalInstruction activation.owner inst activation.regs state.memory = .ok (regs, state.memory) ∧
      step .ssa p state = some {state with control := .running {activation with pc := activation.pc+1, regs}} ∧
      ∀ id ∈ needed, ∀ regClass,
        readSlot regs ⟨id, regClass⟩ = readSlot activation.regs ⟨id, regClass⟩ := by
  obtain ⟨regs, evaluated⟩ := reachable_pure_success .ssa p type entry state progress reachable
    activation function block inst running foundFunction foundBlock foundInstruction
    (checkDCE_pure needed inst accepted)
  refine ⟨regs, evaluated, ?_, ?_⟩
  · simpa using step_evalInstruction .ssa p state activation function block inst regs state.memory
      running foundFunction foundBlock foundInstruction (checkDCE_noCall needed inst accepted) evaluated
  · intro id member regClass
    exact Demand.evalInstruction_lookup_other activation.owner inst activation.regs state.memory regs
      state.memory evaluated ⟨id, regClass⟩ (checkDCE_outside needed inst accepted id member)

theorem deletedOrigin_step (p : Program) (type : Core.Source.IntType)
    (entry state : State) (needed : List Nat) (inst : Instruction)
    (accepted : checkDCE needed inst = true) (classes : instructionShapeValid inst = true)
    (progress : ObservedProgress .ssa p type entry)
    (reachable : Core.Transition.Reaches (step .ssa p) entry state)
    (activation : Activation) (function : Function) (block : Block)
    (running : state.control = .running activation)
    (foundFunction : p.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (foundInstruction : block.insts[activation.pc]? = some inst)
    (sourceEnv targetEnv : Origins) (valuation : Origin → Slot) (targetRegs : RegisterFile)
    (sourceReal : Realizes sourceEnv valuation activation.regs)
    (targetReal : Realizes targetEnv valuation targetRegs)
    (sourceBefore : KnownBefore block.id activation.pc sourceEnv)
    (targetBefore : KnownBefore block.id activation.pc targetEnv)
    (normalized : OriginsNormalized valuation) :
    ∃ regs nextValuation,
      step .ssa p state = some {state with control := .running {activation with pc := activation.pc+1, regs}} ∧
      Realizes (resultOrigins block.id activation.pc 0 inst.defs sourceEnv) nextValuation regs ∧
      Realizes targetEnv nextValuation targetRegs ∧ OriginsNormalized nextValuation ∧
      KnownBefore block.id (activation.pc+1) (resultOrigins block.id activation.pc 0 inst.defs sourceEnv) ∧
      KnownBefore block.id (activation.pc+1) targetEnv := by
  obtain ⟨regs, evaluated, edge, _⟩ := checkDCE_reachable p type entry state needed inst accepted
    progress reachable activation function block running foundFunction foundBlock foundInstruction
  obtain ⟨writes, effects, regsEqual⟩ := evalInstruction_success_writes activation.owner inst
    activation.regs state.memory regs state.memory evaluated
  let nextValuation := writeResultValues block.id activation.pc 0 writes valuation
  obtain ⟨sourceAfter, targetAfter⟩ := deleted_results block.id activation.pc inst sourceEnv targetEnv
    valuation activation.regs targetRegs activation.owner state.memory sourceReal targetReal
    (futureFresh_of_before _ _ _ _ sourceBefore) (futureFresh_of_before _ _ _ _ targetBefore)
    writes effects
  refine ⟨regs, nextValuation, edge, ?_, targetAfter, ?_,
    knownBefore_results _ _ _ _ _ sourceBefore, knownBefore_mono _ _ _ _ targetBefore (by omega)⟩
  · simpa only [regsEqual] using sourceAfter
  · exact writeResultValues_normalized _ _ _ _ _ normalized
      (checked_writes_normalized activation.owner inst (readOperand activation.regs)
        state.memory writes state.memory classes effects)

def checkDCESpan (needed : List Nat) (insts : List Instruction) : Bool :=
  insts.all (checkDCE needed)

/-- The induction follows actual source constructor steps. Its finite span
length is a local block prefix, not a bound on a loop or on total execution. -/
theorem checkDCESpan_reachable (p : Program) (type : Core.Source.IntType)
    (entry state : State) (needed : List Nat) (insts : List Instruction)
    (accepted : checkDCESpan needed insts = true)
    (progress : ObservedProgress .ssa p type entry)
    (reachable : Core.Transition.Reaches (step .ssa p) entry state)
    (activation : Activation) (function : Function) (block : Block)
    (running : state.control = .running activation)
    (foundFunction : p.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (located : ContainsSpan block activation.pc insts) :
    ∃ regs, Core.Transition.Path (step .ssa p) insts.length state
      {state with control := .running {activation with pc := activation.pc+insts.length, regs}} ∧
      ∀ id ∈ needed, ∀ regClass,
        readSlot regs ⟨id, regClass⟩ = readSlot activation.regs ⟨id, regClass⟩ := by
  induction insts generalizing state activation with
  | nil =>
    refine ⟨activation.regs, ?_, by intros; rfl⟩
    have unchanged : {state with control := .running {activation with pc := activation.pc+0, regs := activation.regs}} = state := by
      cases state
      simp_all
    simpa only [List.length_nil, unchanged] using
      (Core.Transition.Path.refl state : Core.Transition.Path (step .ssa p) 0 state state)
  | cons inst rest ih =>
    have checks : checkDCE needed inst = true ∧ checkDCESpan needed rest = true := by
      simpa only [checkDCESpan, List.all_cons, Bool.and_eq_true] using accepted
    have foundInstruction : block.insts[activation.pc]? = some inst := by
      simpa using located 0 (by simp)
    obtain ⟨middleRegs, _, firstStep, firstUnchanged⟩ := checkDCE_reachable p type entry state needed inst
      checks.1 progress reachable activation function block running foundFunction foundBlock foundInstruction
    let middleActivation := {activation with pc := activation.pc+1, regs := middleRegs}
    let middleState := {state with control := .running middleActivation}
    have middleReachable : Core.Transition.Reaches (step .ssa p) entry middleState :=
      reachable.trans (.next firstStep (.refl _))
    have tailLocated : ContainsSpan block middleActivation.pc rest := by
      intro index less
      have atIndex := located (index+1) (by simp only [List.length_cons]; omega)
      simpa [middleActivation, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using atIndex
    obtain ⟨regs, tailPath, tailUnchanged⟩ := ih middleState checks.2 middleReachable middleActivation
      rfl foundFunction foundBlock tailLocated
    refine ⟨regs, ?_, ?_⟩
    · have fullPath := Core.Transition.Path.next firstStep tailPath
      simpa [middleState, middleActivation, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using fullPath
    · intro id member regClass
      exact (tailUnchanged id member regClass).trans (firstUnchanged id member regClass)

end EvoIR.Machine.Transport
