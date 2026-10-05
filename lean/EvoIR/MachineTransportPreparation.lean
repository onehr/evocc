import EvoIR.MachineTransportDCE
import EvoIR.MachineTransportReads

/-! Preparation of the next actual retained instruction or terminator from
an accepted remaining checkBodyAux suffix. The source performs only proved
pure deletions; the target performs only its actual validated moves. Neither
preparation changes memory, call frames, ownership, or logical call inputs. -/
namespace EvoIR.Machine.Transport

def atState (state : State) (activation : Activation) (pc : Nat) (regs : RegisterFile) : State :=
  {state with control := .running {activation with pc, regs}}

theorem atState_self (state : State) (activation : Activation)
    (running : state.control = .running activation) :
    atState state activation activation.pc activation.regs = state := by
  cases state
  simp_all [atState]

theorem containsSpan_drop (block : Block) (pc : Nat) : ContainsSpan block pc (block.insts.drop pc) := by
  intro index _
  exact List.getElem?_drop.symm

theorem containsSpan_dropTake (block : Block) (pc amount : Nat) :
    ContainsSpan block pc ((block.insts.drop pc).take amount) := by
  intro index less
  have below : index < amount := Nat.lt_of_lt_of_le less (List.length_take_le amount _)
  rw [List.getElem?_take_of_lt below]
  exact List.getElem?_drop.symm

/-- This is a derived state of the existing checker, not an additional
annotation or a producer-supplied semantic obligation. -/
def BodyReady (source target : Block) (needed : List Nat) (final : BodyResult)
    (sourcePC targetPC : Nat) (env : BodyResult) : Prop :=
  (sourcePC = source.insts.length ∧ targetPC = target.insts.length ∧ env = final) ∨
  ∃ inst rest positions candidate,
    source.insts.drop sourcePC = inst :: rest ∧ target.insts[targetPC]? = some candidate ∧
    matchOrigins inst candidate env.source env.target = true ∧
    checkBodyAux source.id needed target.insts (sourcePC+1) (targetPC+1) rest positions
      {source := resultOrigins source.id sourcePC 0 inst.defs env.source,
       target := resultOrigins source.id sourcePC 0 candidate.defs env.target,
       cuts := ⟨sourcePC+1, targetPC+1,
          resultOrigins source.id sourcePC 0 inst.defs env.source,
          resultOrigins source.id sourcePC 0 candidate.defs env.target⟩ :: env.cuts} = some final

/-- Accepted actual suffixes have finite concrete preparation paths for all
input values, even when their surrounding original CFG has unbounded loops.
Every subsequent effect and call remains at the next retained constructor.
This theorem consumes the actual source progress established by selection. -/
theorem checkBodyAux_prepares (sourceProgram targetProgram : Program)
    (type : Core.Source.IntType) (sourceEntry : State)
    (sourceFunction targetFunction : Function) (sourceBlock targetBlock : Block)
    (needed : List Nat) (final : BodyResult)
    (remaining : List Instruction) (positions : List (Option Nat))
    (sourceState targetState : State) (sourceActivation targetActivation : Activation)
    (env : BodyResult) (valuation : Origin → Slot)
    (accepted : checkBodyAux sourceBlock.id needed targetBlock.insts sourceActivation.pc
      targetActivation.pc remaining positions env = some final)
    (suffix : sourceBlock.insts.drop sourceActivation.pc = remaining)
    (sourceBound : sourceActivation.pc ≤ sourceBlock.insts.length)
    (targetBound : targetActivation.pc ≤ targetBlock.insts.length)
    (classes : remaining.all instructionShapeValid = true)
    (runningSource : sourceState.control = .running sourceActivation)
    (runningTarget : targetState.control = .running targetActivation)
    (foundSourceFunction : sourceProgram.functions.find? (·.name == sourceActivation.function) = some sourceFunction)
    (foundTargetFunction : targetProgram.functions.find? (·.name == targetActivation.function) = some targetFunction)
    (foundSourceBlock : sourceFunction.blocks.find? (·.id == sourceActivation.block) = some sourceBlock)
    (foundTargetBlock : targetFunction.blocks.find? (·.id == targetActivation.block) = some targetBlock)
    (sourceReal : Realizes env.source valuation sourceActivation.regs)
    (targetReal : Realizes env.target valuation targetActivation.regs)
    (sourceBefore : KnownBefore sourceBlock.id sourceActivation.pc env.source)
    (targetBefore : KnownBefore sourceBlock.id sourceActivation.pc env.target)
    (normalized : OriginsNormalized valuation)
    (progress : ObservedProgress .ssa sourceProgram type sourceEntry)
    (reachable : Core.Transition.Reaches (step .ssa sourceProgram) sourceEntry sourceState) :
    ∃ sourcePC targetPC sourceRegs targetRegs nextValuation nextEnv,
      Core.Transition.Reaches (step .ssa sourceProgram) sourceState
        (atState sourceState sourceActivation sourcePC sourceRegs) ∧
      Core.Transition.Reaches (step .postSSA targetProgram) targetState
        (atState targetState targetActivation targetPC targetRegs) ∧
      Realizes nextEnv.source nextValuation sourceRegs ∧
      Realizes nextEnv.target nextValuation targetRegs ∧ OriginsNormalized nextValuation ∧
      KnownBefore sourceBlock.id sourcePC nextEnv.source ∧
      KnownBefore sourceBlock.id sourcePC nextEnv.target ∧
      BodyReady sourceBlock targetBlock needed final sourcePC targetPC nextEnv := by
  induction remaining generalizing positions sourceState sourceActivation env valuation with
  | nil =>
    cases positions with
    | cons _ _ => simp [checkBodyAux] at accepted
    | nil =>
      cases copied : copyOrigins (targetBlock.insts.drop targetActivation.pc) env.target with
      | none => simp [checkBodyAux, copied] at accepted
      | some after =>
        have result : {env with target := after} = final := by
          simpa [checkBodyAux, copied] using accepted
        obtain ⟨targetRegs, targetPath, targetFinal⟩ := copyOrigins_steps .postSSA targetProgram
          targetState targetActivation targetFunction targetBlock
          (targetBlock.insts.drop targetActivation.pc) env.target after copied valuation
          runningTarget foundTargetFunction foundTargetBlock (containsSpan_drop _ _) targetReal normalized
        have sourceEnd : sourceActivation.pc = sourceBlock.insts.length := by
          have afterEnd := List.drop_eq_nil_iff.mp suffix
          omega
        have targetEnd : targetActivation.pc + (targetBlock.insts.drop targetActivation.pc).length =
            targetBlock.insts.length := by simp only [List.length_drop]; omega
        refine ⟨sourceActivation.pc, targetBlock.insts.length, sourceActivation.regs, targetRegs,
          valuation, final, ?_, ?_, ?_, ?_, normalized, ?_, ?_, Or.inl ⟨sourceEnd, rfl, rfl⟩⟩
        · simpa only [atState_self sourceState sourceActivation runningSource] using
            (Core.Transition.Reaches.refl sourceState : Core.Transition.Reaches
              (step .ssa sourceProgram) sourceState sourceState)
        · simpa only [atState, targetEnd] using targetPath.reaches
        · simpa only [← result] using sourceReal
        · simpa only [← result] using targetFinal
        · simpa only [← result] using sourceBefore
        · simpa only [← result] using knownBefore_copies sourceBlock.id sourceActivation.pc
            _ env.target after copied targetBefore
  | cons inst rest ih =>
    cases positions with
    | nil => simp [checkBodyAux] at accepted
    | cons position positions =>
      have sourceInstruction : sourceBlock.insts[sourceActivation.pc]? = some inst := by
        have first := congrArg (fun values : List Instruction => values[0]?) suffix
        simpa only [List.getElem?_drop, Nat.add_zero, List.getElem?_cons_zero] using first
      have sourceInside : sourceActivation.pc < sourceBlock.insts.length :=
        (List.getElem?_eq_some_iff.mp sourceInstruction).1
      have restSuffix : sourceBlock.insts.drop (sourceActivation.pc+1) = rest := by
        rw [List.drop_add_one_eq_tail_drop, suffix]
        rfl
      have shapes : instructionShapeValid inst = true ∧ rest.all instructionShapeValid = true := by
        simpa only [List.all_cons, Bool.and_eq_true] using classes
      cases position with
      | none =>
        by_cases deleted : checkDCE needed inst = true
        · let nextEnv : BodyResult := {env with source := resultOrigins sourceBlock.id sourceActivation.pc 0 inst.defs env.source}
          have tailAccepted : checkBodyAux sourceBlock.id needed targetBlock.insts (sourceActivation.pc+1)
              targetActivation.pc rest positions nextEnv = some final := by
            simpa [checkBodyAux, deleted, nextEnv] using accepted
          obtain ⟨regs, nextValuation, edge, sourceNext, targetNext, normalizedNext, sourceHistory, targetHistory⟩ :=
            deletedOrigin_step sourceProgram type sourceEntry sourceState needed inst deleted shapes.1
              progress reachable sourceActivation sourceFunction sourceBlock runningSource foundSourceFunction
              foundSourceBlock sourceInstruction env.source env.target valuation targetActivation.regs
              sourceReal targetReal sourceBefore targetBefore normalized
          let nextActivation := {sourceActivation with pc := sourceActivation.pc+1, regs}
          let nextState := {sourceState with control := .running nextActivation}
          have reachedNext : Core.Transition.Reaches (step .ssa sourceProgram) sourceEntry nextState :=
            reachable.trans (.next edge (.refl _))
          obtain ⟨sourcePC, targetPC, sourceRegs, targetRegs, finalValuation, finalEnv,
            sourcePath, targetPath, sourceFinal, targetFinal, normalizedFinal,
            sourceBeforeFinal, targetBeforeFinal, ready⟩ :=
            ih positions nextState nextActivation nextEnv nextValuation tailAccepted restSuffix
              (by dsimp [nextActivation]; omega) shapes.2 rfl foundSourceFunction foundSourceBlock
              sourceNext targetNext sourceHistory targetHistory normalizedNext reachedNext
          refine ⟨sourcePC, targetPC, sourceRegs, targetRegs, finalValuation, finalEnv,
            ?_, targetPath, sourceFinal, targetFinal, normalizedFinal, sourceBeforeFinal, targetBeforeFinal, ready⟩
          have fullPath := Core.Transition.Reaches.next edge sourcePath
          simpa only [nextState, nextActivation, atState] using fullPath
        · simp [checkBodyAux, deleted] at accepted
      | some targetIndex =>
        by_cases backwards : targetIndex < targetActivation.pc
        · simp [checkBodyAux, backwards] at accepted
        · cases copied : copyOrigins
            ((targetBlock.insts.drop targetActivation.pc).take (targetIndex-targetActivation.pc)) env.target with
          | none => simp [checkBodyAux, backwards, copied] at accepted
          | some targetEnv =>
            cases candidateAt : targetBlock.insts[targetIndex]? with
            | none => simp [checkBodyAux, backwards, copied, candidateAt] at accepted
            | some candidate =>
              by_cases matched : matchOrigins inst candidate env.source targetEnv = true
              · have tailAccepted : checkBodyAux sourceBlock.id needed targetBlock.insts
                    (sourceActivation.pc+1) (targetIndex+1) rest positions
                    {source := resultOrigins sourceBlock.id sourceActivation.pc 0 inst.defs env.source,
                     target := resultOrigins sourceBlock.id sourceActivation.pc 0 candidate.defs targetEnv,
                     cuts := ⟨sourceActivation.pc+1, targetIndex+1,
                        resultOrigins sourceBlock.id sourceActivation.pc 0 inst.defs env.source,
                        resultOrigins sourceBlock.id sourceActivation.pc 0 candidate.defs targetEnv⟩ :: env.cuts} = some final := by
                  simpa [checkBodyAux, backwards, copied, candidateAt, matched] using accepted
                have targetInside : targetIndex < targetBlock.insts.length :=
                  (List.getElem?_eq_some_iff.mp candidateAt).1
                have copiedLength : ((targetBlock.insts.drop targetActivation.pc).take
                    (targetIndex-targetActivation.pc)).length = targetIndex-targetActivation.pc := by
                  apply List.length_take_of_le
                  simp only [List.length_drop]
                  omega
                have targetPC : targetActivation.pc + (targetIndex-targetActivation.pc) = targetIndex := by omega
                obtain ⟨targetRegs, targetPath, targetFinal⟩ := copyOrigins_steps .postSSA targetProgram
                  targetState targetActivation targetFunction targetBlock
                  ((targetBlock.insts.drop targetActivation.pc).take (targetIndex-targetActivation.pc))
                  env.target targetEnv copied valuation runningTarget foundTargetFunction foundTargetBlock
                  (containsSpan_dropTake _ _ _) targetReal normalized
                refine ⟨sourceActivation.pc, targetIndex, sourceActivation.regs, targetRegs,
                  valuation, {env with target := targetEnv}, ?_, ?_, sourceReal, targetFinal,
                  normalized, sourceBefore, ?_, Or.inr ⟨inst, rest, positions, candidate,
                    suffix, candidateAt, matched, tailAccepted⟩⟩
                · simpa only [atState_self sourceState sourceActivation runningSource] using
                    (Core.Transition.Reaches.refl sourceState : Core.Transition.Reaches
                      (step .ssa sourceProgram) sourceState sourceState)
                · simpa only [atState, copiedLength, targetPC] using targetPath.reaches
                · exact knownBefore_copies _ _ _ _ _ copied targetBefore
              · simp [checkBodyAux, backwards, copied, candidateAt, matched] at accepted

end EvoIR.Machine.Transport
