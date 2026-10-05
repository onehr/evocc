import EvoIR.MachineSelectionTokenFunction
import EvoIR.MachineSelectionCallEntry
import EvoIR.MachineProgress

/-! Instantiating a previously checked function at the actual call entry.
Canonical cutpoint inputs are rebased from the entire logical argument vector.
The incoming private memory and both suspended caller stacks are arbitrary;
neither termination nor a caller continuation theorem is required. -/
namespace EvoIR.Core.Symbolic.MachineSelection

structure TokenCallee (source : IR.Program) (target : EvoIR.Machine.Program) where
  name : String
  type : Source.IntType
  checked : CheckedTokenFunction source target name type

def TokenCalleeArgumentsDiagram (function : CheckedTokenFunction source target name type)
    (args : List (Word inputWidths)) : Prop :=
  ∀ σ memory owner nextFrame sourceStack targetStack observationType
      (sourceActivation : IR.Activation) (targetActivation : EvoIR.Machine.Activation),
    IR.enter function.sourceFunction (tokenInputValues σ args memory) owner = .ok sourceActivation →
    EvoIR.Machine.enter function.targetFunction (machineInputs σ args) owner = .ok targetActivation →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (TokenReturnBoundary type owner sourceStack targetStack)
      ⟨.running sourceActivation, sourceStack, nextFrame⟩
      ⟨.running targetActivation, targetStack, nextFrame, memory⟩

theorem token_callee_arguments (function : CheckedTokenFunction source target name type)
    (args : List (Word inputWidths)) (widths : function.first.widths = args.map Sigma.fst) :
    TokenCalleeArgumentsDiagram function args := by
  intro σ memory owner nextFrame sourceStack targetStack observationType sourceActivation targetActivation sourceEntered targetEntered
  let next := rebaseWords σ args
  have bounded : ∀ word ∈ args, word.1 ≤ 64 := by
    intro word member
    apply function.first.bounded word.1
    rw [widths]
    exact List.mem_map.mpr ⟨word, member, rfl⟩
  have canonicalSource := function.sourceEntry next memory owner
  have sourceInputs : (cutpointWords function.first.widths).map (Word.denote next) =
      args.map (Word.denote σ) := by
    dsimp only [next]
    rw [widths, cutpoint_words_rebase σ args bounded]
  simp only [tokenInputValues, sourceInputs] at canonicalSource
  have actualSource := Except.ok.inj (sourceEntered.symm.trans canonicalSource)
  have canonicalTarget := function.targetEntry next owner
  have targetInputs : machineInputs next (cutpointWords function.first.widths) =
      machineInputs σ args := by
    dsimp only [next]
    rw [widths, cutpoint_machine_rebase σ args bounded]
  rw [targetInputs] at canonicalTarget
  have actualTarget := Except.ok.inj (targetEntered.symm.trans canonicalTarget)
  rw [actualSource, actualTarget]
  exact function.body.openBody owner sourceStack targetStack observationType _ _
    ⟨function.first, function.member, next, nextFrame, machineInputs σ args, memory, rfl, rfl⟩

def tokenCalleeArguments (function : CheckedTokenFunction source target name type)
    (args : List (Word inputWidths)) : Option (PLift (TokenCalleeArgumentsDiagram function args)) :=
  if widths : function.first.widths = args.map Sigma.fst then
    some ⟨token_callee_arguments function args widths⟩
  else none

/-- Both auxiliary argument vectors receive actual standalone machine
progress from the all-argument callee certificate, at arbitrary memory and
frame state. This never assumes progress for the suspended caller. -/
theorem token_callee_progress (function : CheckedTokenFunction source target name type)
    (args : List (Word inputWidths)) (body : TokenCalleeArgumentsDiagram function args)
    (σ : Subst inputWidths) (memory : IR.Memory) (owner nextFrame : Nat)
    (sourceActivation : IR.Activation) (targetActivation : EvoIR.Machine.Activation)
    (sourceEntered : IR.enter function.sourceFunction (tokenInputValues σ args memory) owner = .ok sourceActivation)
    (targetEntered : EvoIR.Machine.enter function.targetFunction (machineInputs σ args) owner = .ok targetActivation) :
    EvoIR.Machine.ObservedProgress .ssa target type ⟨.running targetActivation, [], nextFrame, memory⟩ := by
  have supported : type.width = 32 ∨ type.width = 64 := by cases type <;> decide
  exact ((body σ memory owner nextFrame [] [] type sourceActivation targetActivation sourceEntered targetEntered).finish
    (token_return_boundary_refines source target type owner supported)).progress

end EvoIR.Core.Symbolic.MachineSelection
