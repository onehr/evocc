import EvoIR.MachinePrivateBody
import EvoIR.MachineSelectionTokenReturnOpen

/-!
Actual private return leaves keep source and target memory separate. A source
lifetime may already have retired an object while the static machine slot is
still live. The return step performs each semantics' own endFrame operation.
The boundary also records the source's checked caller-storage extension; no
caller continuation or equality of the two memories is assumed here.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def privateReturnValue {Γ : Var → Nat} (σ : Subst Γ) (type : Source.IntType)
    (word : Word Γ) (typed : word.1 = type.width) : Source.Value :=
  ⟨type, typed ▸ denoteTerm σ word.2⟩

theorem private_return_value_ir {Γ : Var → Nat} (σ : Subst Γ) (type : Source.IntType)
    (word : Word Γ) (typed : word.1 = type.width) :
    sourceValueIR (privateReturnValue σ type word typed) = word.denote σ := by
  cases word with
  | mk width term => cases typed; rfl

theorem private_return_value_machine {Γ : Var → Nat} (σ : Subst Γ) (type : Source.IntType)
    (word : Word Γ) (typed : word.1 = type.width) :
    EvoIR.Machine.sourceArgument (privateReturnValue σ type word typed) =
      .int ((denoteTerm σ word.2).setWidth 64) := by
  cases word with
  | mk width term => cases typed; rfl

def PrivateReturnBoundary (type : Source.IntType) (incoming : IR.Memory) (owner : Nat)
    (sourceStack : List IR.ReturnFrame) (targetStack : List EvoIR.Machine.ReturnFrame)
    (source : IR.State) (target : EvoIR.Machine.State) : Prop :=
  ∃ value : Source.Value, ∃ sourceMemory targetMemory nextFrame,
    value.type = type ∧
    EvoIR.Machine.PrivateMemory.LiveRelated sourceMemory targetMemory ∧
    incoming.ClosedExtension (sourceMemory.endFrame owner) ∧
    (sourceMemory.endFrame owner).WellAllocated ∧
    (sourceMemory.endFrame owner).BeforeFrame nextFrame ∧
    owner < nextFrame ∧
    (∀ object ∈ (sourceMemory.endFrame owner).objects,
      object ∉ incoming.objects → owner ≤ object.owner) ∧
    source = cutResume [sourceValueIR value, .memory (sourceMemory.endFrame owner), .world]
      nextFrame sourceStack ∧
    target = machineReturnState [EvoIR.Machine.sourceArgument value]
      targetMemory owner nextFrame targetStack

def PrivateReturnDiagram {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block)
    (bank : PrivateBank Γ) (type : Source.IntType) (checks : List (Guard Γ)) : Prop :=
  ∀ σ values incoming owner nextFrame memory regs logicalInputs sourceStack targetStack observationType,
    bank.Models σ values incoming owner nextFrame memory regs →
    guards σ checks = .ok () →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (PrivateReturnBoundary type incoming owner sourceStack targetStack)
      (privateState σ values sf sb.name sb.instructions.length owner nextFrame bank.source sourceStack)
      (privateMachineState tf tb tb.insts.length regs owner nextFrame logicalInputs targetStack memory)

/-- An independently inspected actual return. The words/flags overlay does
not prescribe pointer values or the rest of the concrete register file. -/
def privateReturn {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block)
    (bank : PrivateBank Γ) (type : Source.IntType) (checks : List (Guard Γ) := []) :
    Option (PLift (PrivateReturnDiagram source target sf sb tf tb bank type checks)) := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          if returns : sf.returns = [.bits type.width, .memory, .world] then
            match sourceReturn : sb.terminator, targetReturn : tb.term with
            | .return [id, memoryId, worldId], .ret [reg] => do
              if memoryToken : bank.source.scope.lookup memoryId = some (.opaque bank.source.memory) then
                if worldToken : bank.source.scope.lookup worldId = some (.opaque bank.source.world) then
                  let word : {word // bank.source.scope.word id = some word} ←
                    match found : bank.source.scope.word id with
                    | none => none
                    | some word => some ⟨word, found⟩
                  if typed : word.val.1 = type.width then
                    if present : (bank.words.lookup reg.id).isSome = true then
                      let actual ← wordOperand bank.words reg
                      let equal ← checkWordUnder checks word.val actual.val
                      return ⟨by
                        intro σ values incoming owner nextFrame memory regs logicalInputs
                          sourceStack targetStack observationType models passed
                        obtain ⟨sourceMemory, current, memoryRelated⟩ := models.memory
                        have sourceStep := token_source_return_step source sf sb bank.source.scope word.val
                          id memoryId worldId bank.source.memory bank.source.world σ (privateToken values)
                          sourceMemory owner nextFrame sourceStack sourceFunction sourceBlock sourceReturn
                          word.property memoryToken worldToken (bank.source.current_token current)
                          models.source.2.2.1 (by simpa only [typed] using returns)
                        have targetRead : EvoIR.Machine.read regs reg =
                            .ok (.int ((denoteTerm σ word.val.2).setWidth 64)) := by
                          have read := word_operand_value bank.words reg actual.val actual.property σ
                          have agrees : EvoIR.Machine.operandValue (EvoIR.Machine.readOperand regs) reg =
                              EvoIR.Machine.operandValue (EvoIR.Machine.readOperand (bankDenote σ bank.words)) reg := by
                            simp only [EvoIR.Machine.operandValue]
                            rw [bank_models_operand σ bank.words regs models.words reg present]
                          rw [← agrees, EvoIR.Machine.operandValue_readOperand,
                            equal.down σ passed] at read
                          exact read
                        have targetStep := machine_return_step .ssa target
                          (privateMachineState tf tb tb.insts.length regs owner nextFrame logicalInputs targetStack memory)
                          ⟨tf.name, tb.id, tb.insts.length, regs, owner, logicalInputs⟩ tf tb [reg]
                          [.int ((denoteTerm σ word.val.2).setWidth 64)] rfl
                          targetFunction targetBlock rfl targetReturn
                          (by simp [targetRead, Bind.bind, Except.bind, Pure.pure, Except.pure])
                        let value : Source.Value := privateReturnValue σ type word.val typed
                        have sourceValue : sourceValueIR value = word.val.denote σ := by
                          exact private_return_value_ir σ type word.val typed
                        have targetValue : EvoIR.Machine.sourceArgument value =
                            .int ((denoteTerm σ word.val.2).setWidth 64) := by
                          exact private_return_value_machine σ type word.val typed
                        have storage := private_return_storage σ values incoming sourceMemory owner nextFrame
                          bank.source models.source current
                        have boundary : PrivateReturnBoundary type incoming owner sourceStack targetStack
                            (tokenSourceReturnState σ word.val sourceMemory owner nextFrame sourceStack)
                            (machineReturnState [.int ((denoteTerm σ word.val.2).setWidth 64)]
                              memory owner nextFrame targetStack) := by
                          refine ⟨value, sourceMemory, memory, nextFrame, rfl, memoryRelated,
                            storage.1, storage.2.1, storage.2.2.1, ?_, storage.2.2.2, ?_, ?_⟩
                          · obtain ⟨_, _, _, _, future, _⟩ := models.source.2.2.2.2
                            exact future
                          · simp only [tokenSourceReturnState, sourceValue]
                          · rw [targetValue]
                        apply Transition.OpenCheckpoint.exit boundary (.single ?_) (.single targetStep)
                        simpa only [Transition.Predecessor, privateState, sourceState] using sourceStep⟩
                    else none
                  else none
                else none
              else none
            | _, _ => none
          else none
        else none
      else none
    else none
  else none

/-- Closing the empty caller stack forgets memory only at the observation
boundary. The contextual certificate above still retains both memories. -/
theorem private_return_boundary_refines (source : IR.Program) (target : EvoIR.Machine.Program)
    (type : Source.IntType) (incoming : IR.Memory) (owner : Nat)
    (supported : type.width = 32 ∨ type.width = 64) (s : IR.State) (t : EvoIR.Machine.State)
    (boundary : PrivateReturnBoundary type incoming owner [] [] s t) :
    EvoIR.Machine.SelectionRefines source target type s t := by
  obtain ⟨value, sourceMemory, targetMemory, nextFrame, typed, _, _, _, _, _, _, rfl, rfl⟩ := boundary
  subst type
  apply Transition.refines_of_terminals (.refl _) (.refl _) rfl rfl
    (o := Observation.returned value.type value.bits.toNat)
  · simp [observeTarget, cutResume, sourceValueIR]
  · have bounded : value.type.width ≤ 64 := by omega
    have admitted : (value.type.width == 32 || value.type.width == 64) = true := by
      rcases supported with width | width <;> simp [width]
    simp [machineReturnState, EvoIR.Machine.observe, EvoIR.Machine.sourceArgument,
      admitted, BitVec.setWidth_setWidth_of_le _ bounded]

end EvoIR.Core.Symbolic.MachineSelection
