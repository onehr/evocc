import EvoIR.MachinePrivateFunction
import EvoIR.MachineProgress

/-!
Standalone machine progress follows from a checked context-parametric private
callee. Both actual incoming memories are retained. Demand replacement can use
this theorem for each complete argument vector at the same target snapshot;
it need not assume caller progress or replace the caller's storage by empty
memory. Closing empty continuation stacks is not clearing either memory.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem private_callee_progress (function : CheckedPrivateFunction source target name type)
    (args : List (Word inputWidths)) (body : PrivateCalleeArgumentsDiagram function args)
    (σ : Subst inputWidths) (sourceMemory targetMemory : IR.Memory) (owner frame : Nat)
    (sourceActivation : IR.Activation) (targetActivation : EvoIR.Machine.Activation)
    (fresh : sourceMemory.BeforeFrame owner) (future : owner < frame)
    (related : EvoIR.Machine.PrivateMemory.LiveRelated sourceMemory targetMemory)
    (sourceEntered : IR.enter function.sourceFunction (tokenInputValues σ args sourceMemory) owner =
      .ok sourceActivation)
    (targetEntered : EvoIR.Machine.enter function.targetFunction (machineInputs σ args) owner =
      .ok targetActivation) :
    EvoIR.Machine.ObservedProgress .ssa target type
      ⟨.running targetActivation, [], frame, targetMemory⟩ := by
  have supported : type.width = 32 ∨ type.width = 64 := by cases type <;> decide
  exact ((body σ sourceMemory targetMemory owner frame [] [] type
    sourceActivation targetActivation fresh future related sourceEntered targetEntered).finish
    (private_return_boundary_refines source target type sourceMemory owner supported)).progress

end EvoIR.Core.Symbolic.MachineSelection
