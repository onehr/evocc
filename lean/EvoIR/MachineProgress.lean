import EvoIR.MachineStep
import EvoIR.MachineCorrespondence

/-!
Use established semantic progress, not a producer's type claim, to justify
deletion of a reachable pure machine instruction. Pure MIR instructions can
fail on malformed operands but cannot raise a checked program diagnostic.
Thus an already proved, well-observed execution cannot take that failure arm.
This is the bridge from instruction selection to baseline DCE.
-/
namespace EvoIR.Machine

private def MalformedOr (property : α → Prop) : Except Core.Source.Fault α → Prop
  | .ok value => property value
  | .error (.malformed _) => True
  | .error (.diagnostic _) => False

private theorem malformedOr_bind (result : Except Core.Source.Fault α)
    (next : α → Except Core.Source.Fault β) (property : β → Prop)
    (valid : MalformedOr (fun _ => True) result)
    (following : ∀ value, MalformedOr property (next value)) :
    MalformedOr property (result >>= next) := by
  cases result with
  | ok value => exact following value
  | error fault => cases fault <;> exact valid

private theorem operand_value_malformed (regs : RegisterFile) (reg : VReg) :
    MalformedOr (fun _ => True) (operandValue (readOperand regs) reg) := by
  simp only [operandValue, readOperand, readSlot]
  cases regs.lookup reg.id with
  | none => trivial
  | some slot => cases slot <;> trivial

private theorem operand_bits_malformed (regs : RegisterFile) (reg : VReg) :
    MalformedOr (fun _ => True) (operandBits (readOperand regs) reg) := by
  apply malformedOr_bind _ _ _ (operand_value_malformed regs reg)
  intro value
  cases value <;> trivial

private theorem operand_flags_malformed (regs : RegisterFile) (reg : VReg) :
    MalformedOr (fun _ => True) (operandFlags (readOperand regs) reg) := by
  simp only [operandFlags, readOperand, readSlot]
  cases regs.lookup reg.id with
  | none => trivial
  | some slot => cases slot <;> trivial

private theorem binary_value_malformed (op : BinOp) (width : Width) (lhs rhs : Value) :
    MalformedOr (fun _ => True) (evalBinaryValue op width lhs rhs) := by
  cases op <;> cases lhs <;> cases rhs <;> trivial

private theorem condition_malformed (flags : Flags) (cond : Cond) :
    MalformedOr (fun _ => True) (condition flags cond) := by
  cases flags <;> cases cond <;> trivial

theorem pure_instruction_outcome (owner : Nat) (inst : Instruction)
    (regs : RegisterFile) (memory : Memory) (pure : inst.pure = true) :
    (∃ next, evalInstruction owner inst regs memory = .ok (next, memory)) ∨
    (∃ message, evalInstruction owner inst regs memory = .error (.malformed message)) := by
  have safe : MalformedOr (fun result => result.2 = memory)
      (evalInstructionOperands owner inst (readOperand regs) memory) := by
    cases inst <;> try simp [Instruction.pure] at pure
    all_goals simp only [evalInstructionOperands]
    all_goals repeat' first
      | exact operand_value_malformed _ _
      | exact operand_bits_malformed _ _
      | exact operand_flags_malformed _ _
      | exact binary_value_malformed _ _ _ _
      | exact condition_malformed _ _
      | rfl
      | apply malformedOr_bind
      | intro
  unfold evalInstruction
  cases outcome : evalInstructionOperands owner inst (readOperand regs) memory with
  | ok pair =>
    simp only [outcome, MalformedOr] at safe
    refine Or.inl ⟨applyWrites regs pair.1, ?_⟩
    change Except.ok (applyWrites regs pair.1, pair.2) = _
    rw [safe]
  | error fault =>
    cases fault with
    | diagnostic code => simp [outcome, MalformedOr] at safe
    | malformed message => exact Or.inr ⟨message, rfl⟩

def ObservedProgress (mode : Mode) (p : Program) (type : Core.Source.IntType)
    (entry : State) : Prop :=
  ∀ final, Core.Transition.Reaches (step mode p) entry final →
    (∃ next, step mode p final = some next) ∨ ∃ value, observe type final = some value

theorem reachable_pure_success (mode : Mode) (p : Program) (type : Core.Source.IntType)
    (entry state : State) (progress : ObservedProgress mode p type entry)
    (reachable : Core.Transition.Reaches (step mode p) entry state)
    (activation : Activation) (function : Function) (block : Block) (inst : Instruction)
    (running : state.control = .running activation)
    (foundFunction : p.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (foundInstruction : block.insts[activation.pc]? = some inst)
    (pure : inst.pure = true) :
    ∃ regs, evalInstruction activation.owner inst activation.regs state.memory =
      .ok (regs, state.memory) := by
  rcases pure_instruction_outcome activation.owner inst activation.regs state.memory pure with
    success | ⟨message, failed⟩
  · exact success
  · have notCall : ∀ name args results, inst ≠ .bl name args results := by
      intro name args results same
      subst inst
      simp [Instruction.pure] at pure
    have edge := step_evalInstruction_error mode p state activation function block inst
      (.malformed message) running foundFunction foundBlock foundInstruction notCall failed
    have reached : Core.Transition.Reaches (step mode p) entry
        {state with control := .halted (.error (.malformed message))} :=
      reachable.trans (.next edge (.refl _))
    rcases progress _ reached with ⟨next, continues⟩ | ⟨value, observed⟩
    · simp [step] at continues
    · simp [observe] at observed

end EvoIR.Machine
