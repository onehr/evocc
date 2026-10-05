import EvoIR.MachineSelectionConvert
import EvoIR.MachineStep
import EvoIR.Core.CheckpointSimulation

/-!
Actual finite-prefix lifting. Every instruction is fetched from the supplied
MIR function and block; no producer's instruction list or equivalence premise
is accepted. The path counts real `Machine.step .ssa` transitions and therefore
provides positive progress whenever the checked count is nonzero.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def machineState {Γ : Var → Nat} (σ : Subst Γ) (f : EvoIR.Machine.Function)
    (b : EvoIR.Machine.Block) (pc : Nat) (bank : Bank Γ) (owner nextFrame : Nat)
    (incoming : List EvoIR.Machine.Value) (stack : List EvoIR.Machine.ReturnFrame)
    (memory : EvoIR.Machine.Memory) : EvoIR.Machine.State :=
  ⟨.running ⟨f.name, b.id, pc, bankDenote σ bank, owner, incoming⟩,
    stack, nextFrame, memory⟩

def SpanDiagram {Γ : Var → Nat} (p : EvoIR.Machine.Program) (f : EvoIR.Machine.Function)
    (b : EvoIR.Machine.Block) (pc count : Nat) (bank next : Bank Γ) : Prop :=
  ∀ σ owner nextFrame incoming stack memory,
    Transition.Path (EvoIR.Machine.step .ssa p) count
      (machineState σ f b pc bank owner nextFrame incoming stack memory)
      (machineState σ f b (pc+count) next owner nextFrame incoming stack memory)

private theorem eval_instruction_not_call (owner : Nat) (inst : EvoIR.Machine.Instruction)
    (regs : EvoIR.Machine.RegisterFile) (memory : EvoIR.Machine.Memory)
    (nextRegs : EvoIR.Machine.RegisterFile) (nextMemory : EvoIR.Machine.Memory)
    (evaluated : EvoIR.Machine.evalInstruction owner inst regs memory = .ok (nextRegs, nextMemory)) :
    ∀ symbol args results, inst ≠ .bl symbol args results := by
  intro symbol args results equal
  subst inst
  simp [EvoIR.Machine.evalInstruction, EvoIR.Machine.evalInstructionOperands,
    Functor.map, Except.map] at evaluated

/-- The finite bound is a span length in a proposed checkpoint diagram,
not a bound on execution of a cyclic program. Calls and private effects must
be separately checked boundaries; this pure span checker refuses them. -/
def machineSpan {Γ : Var → Nat} (p : EvoIR.Machine.Program) (f : EvoIR.Machine.Function)
    (b : EvoIR.Machine.Block) : (count pc : Nat) → (bank : Bank Γ) →
    Option {next : Bank Γ // SpanDiagram p f b pc count bank next}
  | 0, pc, bank =>
    if function : p.functions.find? (·.name == f.name) = some f then
      if block : f.blocks.find? (·.id == b.id) = some b then
        if valid : pc ≤ b.insts.length then
          some ⟨bank, by
            intro σ owner nextFrame incoming stack memory
            exact .refl _⟩
        else none
      else none
    else none
  | count+1, pc, bank => do
    if function : p.functions.find? (·.name == f.name) = some f then
      if block : f.blocks.find? (·.id == b.id) = some b then
        let instruction : {inst // b.insts[pc]? = some inst} ←
          match atPc : b.insts[pc]? with
          | none => none
          | some inst => some ⟨inst, atPc⟩
        let first ← machineInstruction bank instruction.val
        let rest ← machineSpan p f b count (pc+1) first.val
        return ⟨rest.val, by
          intro σ owner nextFrame incoming stack memory
          have evaluated := first.property σ owner memory
          have notCall := eval_instruction_not_call owner instruction.val
            (bankDenote σ bank) memory (bankDenote σ first.val) memory evaluated
          have stepped := EvoIR.Machine.step_evalInstruction .ssa p
            (machineState σ f b pc bank owner nextFrame incoming stack memory)
            ⟨f.name, b.id, pc, bankDenote σ bank, owner, incoming⟩ f b instruction.val
            (bankDenote σ first.val) memory rfl function block instruction.property
            notCall evaluated
          have tail := rest.property σ owner nextFrame incoming stack memory
          have path := Transition.Path.next stepped tail
          simpa [machineState, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm] using path⟩
      else none
    else none

theorem span_positive {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (pc count : Nat)
    (bank next : Bank Γ) (proof : SpanDiagram p f b pc count bank next)
    (nonempty : 0 < count) (σ : Subst Γ) (owner nextFrame : Nat)
    (incoming : List EvoIR.Machine.Value) (stack : List EvoIR.Machine.ReturnFrame)
    (memory : EvoIR.Machine.Memory) :
    Relation.TransGen (Transition.Predecessor (EvoIR.Machine.step .ssa p))
      (machineState σ f b (pc+count) next owner nextFrame incoming stack memory)
      (machineState σ f b pc bank owner nextFrame incoming stack memory) :=
  (proof σ owner nextFrame incoming stack memory).positive nonempty

end EvoIR.Core.Symbolic.MachineSelection
