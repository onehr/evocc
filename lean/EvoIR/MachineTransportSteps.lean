import EvoIR.MachineTransportCopies
import EvoIR.MachineStep

/-! Finite administrative spans inside actual blocks, lifted to the concrete
unbounded Machine.step relation. Span lengths count constructor steps; they
are not sampled interpreter fuel or a program termination bound. -/
namespace EvoIR.Machine.Transport

def ContainsSpan (block : Block) (pc : Nat) (insts : List Instruction) : Prop :=
  ∀ index, index < insts.length → block.insts[pc+index]? = insts[index]?

def NoCalls (insts : List Instruction) : Prop :=
  ∀ inst ∈ insts, ∀ symbol args results, inst ≠ .bl symbol args results

theorem evalSpan_steps (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (insts : List Instruction)
    (nextRegs : RegisterFile) (nextMemory : Memory)
    (running : s.control = .running a)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (located : ContainsSpan block a.pc insts) (notCalls : NoCalls insts)
    (evaluated : evalCopySpan a.owner insts a.regs s.memory = .ok (nextRegs, nextMemory)) :
    Core.Transition.Path (step mode p) insts.length s
      {s with control := .running {a with pc := a.pc+insts.length, regs := nextRegs}, memory := nextMemory} := by
  induction insts generalizing s a nextRegs nextMemory with
  | nil =>
    simp only [evalCopySpan] at evaluated
    cases evaluated
    have unchanged : {s with control := .running {a with pc := a.pc+0, regs := a.regs}, memory := s.memory} = s := by
      cases s
      simp_all
    simpa only [List.length_nil, unchanged] using
      (Core.Transition.Path.refl s : Core.Transition.Path (step mode p) 0 s s)
  | cons inst rest ih =>
    have foundInstruction : block.insts[a.pc]? = some inst := by
      have first := located 0 (by simp)
      simpa using first
    have notCall : ∀ symbol args results, inst ≠ .bl symbol args results :=
      notCalls inst (by simp)
    cases first : evalInstruction a.owner inst a.regs s.memory with
    | error fault => simp [evalCopySpan, first, Bind.bind, Except.bind] at evaluated
    | ok pair =>
      rcases pair with ⟨middleRegs, middleMemory⟩
      have firstStep := step_evalInstruction mode p s a function block inst
        middleRegs middleMemory running foundFunction foundBlock foundInstruction notCall first
      let middleActivation := {a with pc := a.pc+1, regs := middleRegs}
      let middleState := {s with control := .running middleActivation, memory := middleMemory}
      have tailLocated : ContainsSpan block middleActivation.pc rest := by
        intro index less
        have atIndex := located (index+1) (by simp only [List.length_cons]; omega)
        simpa [middleActivation, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using atIndex
      have tailNotCalls : NoCalls rest := by
        intro next member
        exact notCalls next (List.mem_cons_of_mem _ member)
      have tailEvaluated : evalCopySpan middleActivation.owner rest middleActivation.regs
          middleState.memory = .ok (nextRegs, nextMemory) := by
        simpa [evalCopySpan, first, middleActivation, middleState, Bind.bind, Except.bind] using evaluated
      have tailPath := ih middleState middleActivation nextRegs nextMemory rfl
        foundFunction foundBlock tailLocated tailNotCalls tailEvaluated
      have fullPath := Core.Transition.Path.next firstStep tailPath
      simpa [middleState, middleActivation, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using fullPath

theorem copyOrigins_noCalls (insts : List Instruction) (env after : Origins)
    (accepted : copyOrigins insts env = some after) : NoCalls insts := by
  induction insts generalizing env after with
  | nil => simp [NoCalls]
  | cons inst rest ih =>
    cases inst <;> try simp [copyOrigins] at accepted
    case mov dst src =>
      have tail : ∃ nextEnv, copyOrigins rest nextEnv = some after := by
        by_cases safe : dst.class = src.class ∧ dst.class ≠ .flags
        · cases known : origin env src with
          | none => simp [copyOrigins, safe, known] at accepted
          | some value =>
            refine ⟨originWrite env dst value, ?_⟩
            have sourceGeneral : src.class ≠ .flags := by rw [← safe.1]; exact safe.2
            simpa [copyOrigins, safe, known, sourceGeneral] using accepted
        · simp [copyOrigins, safe] at accepted
      obtain ⟨nextEnv, tailAccepted⟩ := tail
      have tailNoCalls := ih _ _ tailAccepted
      intro candidate member symbol args results
      rcases List.mem_cons.mp member with head | tailMember
      · rw [head]; intro equal; cases equal
      · exact tailNoCalls candidate tailMember symbol args results

/-- The actual accepted moves have exactly this finite concrete path. This
supplies progress for administrative copies even inside an otherwise infinite
loop and does not introduce a hidden interpreter or a producer rerun. -/
theorem copyOrigins_steps (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (insts : List Instruction) (env after : Origins)
    (accepted : copyOrigins insts env = some after) (valuation : Origin → Slot)
    (running : s.control = .running a)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (located : ContainsSpan block a.pc insts)
    (related : Realizes env valuation a.regs)
    (normalized : OriginsNormalized valuation) :
    ∃ nextRegs, Core.Transition.Path (step mode p) insts.length s
      {s with control := .running {a with pc := a.pc+insts.length, regs := nextRegs}} ∧
      Realizes after valuation nextRegs := by
  obtain ⟨nextRegs, evaluated, finalReal⟩ := copyOrigins_sound_normalized insts env after accepted
    valuation a.owner a.regs s.memory related normalized
  refine ⟨nextRegs, ?_, finalReal⟩
  simpa using evalSpan_steps mode p s a function block insts nextRegs s.memory
    running foundFunction foundBlock located (copyOrigins_noCalls insts env after accepted) evaluated

def checkDeletedSpan (live : List Nat) (insts : List Instruction) : Bool :=
  insts.all (checkDeleted live)

theorem checkDeletedSpan_noCalls (live : List Nat) (insts : List Instruction)
    (accepted : checkDeletedSpan live insts = true) : NoCalls insts := by
  intro inst member symbol args results equal
  have each : ∀ candidate ∈ insts, checkDeleted live candidate = true := by
    simpa only [checkDeletedSpan, List.all_eq_true] using accepted
  have h := each inst member
  rw [equal] at h
  simp [checkDeleted] at h

theorem checkDeletedSpan_eval (live : List Nat) (insts : List Instruction)
    (accepted : checkDeletedSpan live insts = true)
    (owner : Nat) (regs : RegisterFile) (memory : Memory) :
    ∃ nextRegs, evalCopySpan owner insts regs memory = .ok (nextRegs, memory) ∧
      ∀ reg ∈ live, ∀ regClass,
        readSlot nextRegs ⟨reg, regClass⟩ = readSlot regs ⟨reg, regClass⟩ := by
  induction insts generalizing regs with
  | nil => exact ⟨regs, rfl, by intros; rfl⟩
  | cons inst rest ih =>
    simp only [checkDeletedSpan, List.all_cons, Bool.and_eq_true] at accepted
    obtain ⟨middle, evaluated, unchanged⟩ := checkDeleted_sound live inst accepted.1 owner regs memory
    obtain ⟨next, tail, finalUnchanged⟩ := ih accepted.2 middle
    refine ⟨next, ?_, ?_⟩
    · simpa [evalCopySpan, evaluated, Bind.bind, Except.bind] using tail
    · intro reg member regClass
      exact (finalUnchanged reg member regClass).trans (unchanged reg member regClass)

/-- Accepted actual deleted-constant prefixes also have concrete finite
source paths. Every checked live slot and memory is preserved for all stores,
including stores whose dead registers are missing or contain other values. -/
theorem checkDeletedSpan_steps (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (insts : List Instruction) (live : List Nat)
    (accepted : checkDeletedSpan live insts = true)
    (running : s.control = .running a)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (located : ContainsSpan block a.pc insts) :
    ∃ nextRegs, Core.Transition.Path (step mode p) insts.length s
      {s with control := .running {a with pc := a.pc+insts.length, regs := nextRegs}} ∧
      ∀ reg ∈ live, ∀ regClass,
        readSlot nextRegs ⟨reg, regClass⟩ = readSlot a.regs ⟨reg, regClass⟩ := by
  obtain ⟨nextRegs, evaluated, unchanged⟩ := checkDeletedSpan_eval live insts accepted
    a.owner a.regs s.memory
  refine ⟨nextRegs, ?_, unchanged⟩
  simpa using evalSpan_steps mode p s a function block insts nextRegs s.memory
    running foundFunction foundBlock located (checkDeletedSpan_noCalls live insts accepted) evaluated

end EvoIR.Machine.Transport
