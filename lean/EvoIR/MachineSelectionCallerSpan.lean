import EvoIR.MachineSelectionCallee
import EvoIR.MachineSelectionCallResume

/-! Actual finite caller prefixes stop at effects and calls. The checked
suffix relation retains a finite opaque token history within one block;
canonical jumps clear that history through the actual simultaneous bind.
The eventual CFG closure, not this finite-prefix scan, handles backedges. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def sourcePurePrefix : List IR.Instruction → Nat
  | [] => 0
  | inst :: rest =>
    match inst.op with
    | .constant _ _ | .scalar _ | .division _ _ _ | .assume _ => sourcePurePrefix rest + 1
    | _ => 0

def TokenCallerBoundary {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (type : Source.IntType) (table : List (TokenCutpoint sf tf)) (owner : Nat)
    (sourceStack : List IR.ReturnFrame) (targetStack : List EvoIR.Machine.ReturnFrame)
    (sourceState : IR.State) (targetState : EvoIR.Machine.State) : Prop :=
  TokenBoundary table owner sourceStack targetStack sourceState targetState ∨
    TokenReturnBoundary type owner sourceStack targetStack sourceState targetState

def TokenSuffixDiagram (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (spc : Nat) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank inputWidths)
    (types : List IR.Ty) (memoryKey worldKey : Nat) (checks : List (Guard inputWidths)) : Prop :=
  ∀ σ (values : List IR.Value), values.map IR.Value.type = types →
    ∀ memory owner nextFrame incoming sourceStack targetStack observationType,
    guards σ checks = .ok () →
    privateToken values memoryKey = .memory memory → privateToken values worldKey = .world →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (TokenCallerBoundary type table owner sourceStack targetStack)
      (sourceState σ (privateToken values) sf sb spc scope owner nextFrame sourceStack)
      (machineState σ tf tb tpc bank owner nextFrame incoming targetStack memory)

theorem token_suffix_control (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (bank : Bank inputWidths) (types : List IR.Ty)
    (memoryKey worldKey : Nat) (checks : List (Guard inputWidths))
    (finish : TokenControlDiagram source target sf tf type table sb scope tb bank checks memoryKey worldKey) :
    TokenSuffixDiagram source target sf tf type table sb sb.instructions.length scope
      tb tb.insts.length bank types memoryKey worldKey checks := by
  intro σ values typed memory owner nextFrame incoming sourceStack targetStack observationType passed current world
  exact finish σ (privateToken values) memory owner nextFrame incoming sourceStack targetStack observationType
    passed current world

theorem token_suffix_prefix (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (spc count : Nat) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank inputWidths) (types : List IR.Ty)
    (memoryKey worldKey : Nat) (checks : List (Guard inputWidths))
    (sourceBody : SourceSpanResult inputWidths) (targetBody : GuardChainResult inputWidths)
    (sourceDiagram : SourceSpanDiagram source sf sb spc count scope sourceBody)
    (targetDiagram : GuardChainUnderDiagram target tf tb tpc bank checks sourceBody.checks targetBody)
    (finish : TokenSuffixDiagram source target sf tf type table sb (spc+count) sourceBody.scope
      targetBody.block targetBody.pc targetBody.bank types memoryKey worldKey (checks ++ sourceBody.checks)) :
    TokenSuffixDiagram source target sf tf type table sb spc scope tb tpc bank types memoryKey worldKey checks := by
  intro σ values typed memory owner nextFrame incoming sourceStack targetStack observationType passed current world
  obtain ⟨sourceLast, sourcePath, sourceOutcome⟩ := sourceDiagram σ (privateToken values) owner nextFrame sourceStack
  obtain ⟨targetLast, targetPath, targetOutcome⟩ := targetDiagram σ owner nextFrame incoming targetStack memory passed
  cases checked : guards σ sourceBody.checks with
  | error fault =>
    have sourceHalt : sourceLast.control = .halted (.error fault) := by
      simpa only [checked] using sourceOutcome
    have targetHalt : targetLast.control = .halted (.error fault) := by
      simpa only [checked] using targetOutcome
    obtain ⟨code, faultCode, observed⟩ := guards_fault_observation σ sourceBody.checks fault checked
    subst fault
    apply Transition.OpenCheckpoint.of_refines
    apply Transition.refines_of_terminals sourcePath.toReaches targetPath.toReaches
      (by simp [IR.step, sourceHalt]) (by simp [EvoIR.Machine.step, targetHalt])
      (o := Observation.diagnostic code)
    · simpa only [observeTarget, sourceHalt] using observed
    · simpa only [EvoIR.Machine.observe, targetHalt] using observed
  | ok value =>
    cases value
    have sourceEnd : sourceLast = sourceState σ (privateToken values) sf sb (spc+count)
        sourceBody.scope owner nextFrame sourceStack := by
      simpa only [checked] using sourceOutcome
    have targetEnd : targetLast = machineState σ tf targetBody.block targetBody.pc targetBody.bank
        owner nextFrame incoming targetStack memory := by
      simpa only [checked] using targetOutcome
    rw [sourceEnd] at sourcePath
    rw [targetEnd] at targetPath
    have allPassed : guards σ (checks ++ sourceBody.checks) = .ok () := by
      simp only [guards_append, passed, checked, Bind.bind, Except.bind]
    exact (finish σ values typed memory owner nextFrame incoming sourceStack targetStack observationType
      allPassed current world).prepend sourcePath.toReaches targetPath.toReaches

end EvoIR.Core.Symbolic.MachineSelection
