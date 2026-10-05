import EvoIR.MachineTransportPreparation
import EvoIR.MachineDemandTerm

/-! The structural check's actual terminator row preserves control tests,
return vectors and the first diagnostic. Edges are related positionally
against the actual source/target edge lists, not by unordered membership. -/
namespace EvoIR.Machine.Transport

def TermEffectsRelated (source target : Terminator) :
    Except Core.Source.Fault Demand.TermEffect → Except Core.Source.Fault Demand.TermEffect → Prop
  | .ok (.edge sourceDst sourceArgs), .ok (.edge targetDst targetArgs) =>
    ((sourceDst, sourceArgs), (targetDst, targetArgs)) ∈ source.edges.zip target.edges
  | .ok (.returned sourceValues), .ok (.returned targetValues) => sourceValues = targetValues
  | .error sourceFault, .error targetFault => sourceFault = targetFault
  | _, _ => False

theorem matchControls_origins (source target : Terminator) (sourceEnv targetEnv : Origins)
    (accepted : matchControls source target sourceEnv targetEnv = true) :
    controlShape source = controlShape target ∧
      ∃ values, operandOrigins sourceEnv (controlUses source) = some values ∧
        operandOrigins targetEnv (controlUses target) = some values := by
  simp only [matchControls, Bool.and_eq_true, decide_eq_true_eq] at accepted
  refine ⟨accepted.1, ?_⟩
  cases left : operandOrigins sourceEnv (controlUses source) with
  | none => simp [left] at accepted
  | some values =>
    cases right : operandOrigins targetEnv (controlUses target) with
    | none => simp [right] at accepted
    | some actual =>
      have same : values = actual := by simpa [left, right] using accepted.2
      exact ⟨values, rfl, congrArg some same.symm⟩

theorem single_origins_reader (sourceEnv targetEnv : Origins) (left right : VReg)
    (values : List Origin)
    (sourceFound : operandOrigins sourceEnv [left] = some values)
    (targetFound : operandOrigins targetEnv [right] = some values)
    (valuation : Origin → Slot) (sourceRegs targetRegs : RegisterFile)
    (sourceReal : Realizes sourceEnv valuation sourceRegs)
    (targetReal : Realizes targetEnv valuation targetRegs) :
    readOperand sourceRegs left = readOperand targetRegs right := by
  cases foundLeft : origin sourceEnv left with
  | none => simp [operandOrigins, foundLeft] at sourceFound
  | some sourceValue =>
    cases foundRight : origin targetEnv right with
    | none => simp [operandOrigins, foundRight] at targetFound
    | some targetValue =>
      have sourceShape : [sourceValue] = values := by simpa [operandOrigins, foundLeft] using sourceFound
      have targetShape : [targetValue] = values := by simpa [operandOrigins, foundRight] using targetFound
      have same := List.singleton_inj.mp (sourceShape.trans targetShape.symm)
      rw [sourceReal left sourceValue foundLeft, targetReal right targetValue foundRight, same]

theorem matchControls_effects (source target : Terminator) (sourceEnv targetEnv : Origins)
    (accepted : matchControls source target sourceEnv targetEnv = true)
    (valuation : Origin → Slot) (sourceRegs targetRegs : RegisterFile)
    (sourceReal : Realizes sourceEnv valuation sourceRegs)
    (targetReal : Realizes targetEnv valuation targetRegs) :
    TermEffectsRelated source target (Demand.evalTerminator sourceRegs source)
      (Demand.evalTerminator targetRegs target) := by
  obtain ⟨shape, values, sourceFound, targetFound⟩ := matchControls_origins source target sourceEnv targetEnv accepted
  have compatible := shape
  cases source <;> cases target <;> simp [controlShape] at compatible
  case b.b => simp [Demand.evalTerminator, TermEffectsRelated, Terminator.edges]
  case brk.brk sourceImm targetImm =>
    have immEq : sourceImm = targetImm := by simpa [controlShape] using shape
    simp [Demand.evalTerminator, TermEffectsRelated, immEq]
  case ret.ret sourceValues targetValues =>
    have reads := operandOrigins_reads_equal sourceEnv targetEnv sourceValues targetValues values
      sourceFound targetFound valuation sourceRegs targetRegs sourceReal targetReal
    simp only [Demand.evalTerminator, reads, Bind.bind, Except.bind, Pure.pure, Except.pure]
    cases actual : targetValues.mapM (read targetRegs) <;> simp [TermEffectsRelated]
  case bCond.bCond sourceFlags sourceCond sourceTaken sourceTakenArgs sourceUntaken sourceUntakenArgs
      targetFlags targetCond targetTaken targetTakenArgs targetUntaken targetUntakenArgs =>
    have sameCond : sourceCond = targetCond := by simpa [controlShape] using congrArg (fun term =>
      match term with | .bCond _ cond _ _ _ _ => cond | _ => .eq) shape
    have reader := single_origins_reader sourceEnv targetEnv sourceFlags targetFlags values
      sourceFound targetFound valuation sourceRegs targetRegs sourceReal targetReal
    have flags := readFlags_eq_of_operand_eq sourceRegs targetRegs sourceFlags targetFlags reader
    simp only [Demand.evalTerminator, sameCond, flags, Bind.bind, Except.bind, Pure.pure, Except.pure]
    repeat' split
    all_goals simp_all [TermEffectsRelated, Terminator.edges]
  case cb.cb sourceNonzero sourceReg sourceTaken sourceTakenArgs sourceUntaken sourceUntakenArgs
      targetNonzero targetReg targetTaken targetTakenArgs targetUntaken targetUntakenArgs =>
    have sameNonzero : sourceNonzero = targetNonzero := by simpa [controlShape] using congrArg (fun term =>
      match term with | .cb nonzero _ _ _ _ _ => nonzero | _ => false) shape
    have sameClass : sourceReg.class = targetReg.class := by simpa [controlShape] using congrArg (fun term =>
      match term with | .cb _ reg _ _ _ _ => reg.class | _ => .flags) shape
    have reader := single_origins_reader sourceEnv targetEnv sourceReg targetReg values
      sourceFound targetFound valuation sourceRegs targetRegs sourceReal targetReal
    have bits := readBits_eq_of_operand_eq sourceRegs targetRegs sourceReg targetReg reader
    simp only [Demand.evalTerminator, sameNonzero, sameClass, bits, Bind.bind, Except.bind,
      Pure.pure, Except.pure, MonadExcept.throw, throwThe, MonadExceptOf.throw]
    repeat' split
    all_goals simp_all [TermEffectsRelated, Terminator.edges]
  case tb.tb sourceNonzero sourceReg sourceBit sourceTaken sourceTakenArgs sourceUntaken sourceUntakenArgs
      targetNonzero targetReg targetBit targetTaken targetTakenArgs targetUntaken targetUntakenArgs =>
    have sameNonzero : sourceNonzero = targetNonzero := by simpa [controlShape] using congrArg (fun term =>
      match term with | .tb nonzero _ _ _ _ _ _ => nonzero | _ => false) shape
    have sameBit : sourceBit = targetBit := by simpa [controlShape] using congrArg (fun term =>
      match term with | .tb _ _ bit _ _ _ _ => bit | _ => 0) shape
    have reader := single_origins_reader sourceEnv targetEnv sourceReg targetReg values
      sourceFound targetFound valuation sourceRegs targetRegs sourceReal targetReal
    have bits := readBits_eq_of_operand_eq sourceRegs targetRegs sourceReg targetReg reader
    simp only [Demand.evalTerminator, sameNonzero, sameBit, bits, Bind.bind, Except.bind,
      Pure.pure, Except.pure, MonadExcept.throw, throwThe, MonadExceptOf.throw]
    repeat' split
    all_goals simp_all [TermEffectsRelated, Terminator.edges]

def finishTerminator (mode : Mode) (function : Function) (state : State) (activation : Activation) :
    Demand.TermEffect → Except Core.Source.Fault State
  | .edge target args => Demand.resume state (jump mode function activation target args)
  | .returned values => Demand.returnValues state activation values

theorem execute_terminator (mode : Mode) (program : Program) (state : State) (activation : Activation)
    (function : Function) (block : Block)
    (foundFunction : program.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (noInstruction : block.insts[activation.pc]? = none)
    (atTerminator : activation.pc = block.insts.length) :
    execute mode program state activation =
      (Demand.evalTerminator activation.regs block.term >>= finishTerminator mode function state activation) := by
  simp only [execute, foundFunction, foundBlock, noInstruction]
  simp only [atTerminator, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
  cases term : block.term <;>
    simp only [Demand.evalTerminator, finishTerminator, Demand.resume, Demand.returnValues,
      Bind.bind, Except.bind, Pure.pure, Except.pure,
      MonadExcept.throw, throwThe, MonadExceptOf.throw]
  all_goals repeat' first | rfl | split
  all_goals simp_all

end EvoIR.Machine.Transport
