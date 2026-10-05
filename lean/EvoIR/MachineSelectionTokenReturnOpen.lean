import EvoIR.MachineSelectionTokens
import EvoIR.Core.CallingCutpoint

/-! A return leaf for suspended callers. The boundary contains both actual
resumed states, including the finalized memory. It does not claim that either
caller continuation progresses, and it retains real result-binding failures. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def tokenSourceReturnState {Γ : Var → Nat} (σ : Subst Γ) (word : Word Γ)
    (memory : IR.Memory) (owner nextFrame : Nat) (stack : List IR.ReturnFrame) : IR.State :=
  cutResume [word.denote σ, .memory (memory.endFrame owner), .world] nextFrame stack

def machineReturnState (values : List EvoIR.Machine.Value) (memory : IR.Memory)
    (owner nextFrame : Nat) (stack : List EvoIR.Machine.ReturnFrame) : EvoIR.Machine.State :=
  match stack with
  | [] => ⟨.halted (.ok values), [], nextFrame, memory.endFrame owner⟩
  | frame :: rest =>
    match EvoIR.Machine.bind frame.caller.regs frame.results values with
    | .ok regs => ⟨.running {frame.caller with regs}, rest, nextFrame, memory.endFrame owner⟩
    | .error fault => ⟨.halted (.error fault), stack, nextFrame, memory⟩

theorem token_source_return_step {Γ : Var → Nat} (source : IR.Program)
    (sf : IR.Function) (sb : IR.Block) (scope : Scope Γ) (word : Word Γ)
    (id memoryId worldId memoryKey worldKey : Nat) (σ : Subst Γ)
    (ρ : Nat → IR.Value) (memory : IR.Memory)
    (owner nextFrame : Nat) (stack : List IR.ReturnFrame)
    (function : source.functions.find? (·.name == sf.name) = some sf)
    (block : sf.blocks.find? (·.name == sb.name) = some sb)
    (terminator : sb.terminator = .return [id, memoryId, worldId])
    (lookupWord : scope.word id = some word)
    (lookupMemory : scope.lookup memoryId = some (.opaque memoryKey))
    (lookupWorld : scope.lookup worldId = some (.opaque worldKey))
    (current : ρ memoryKey = .memory memory) (world : ρ worldKey = .world)
    (returns : sf.returns = [.bits word.1, .memory, .world]) :
    IR.step source (sourceState σ ρ sf sb sb.instructions.length
      scope owner nextFrame stack) =
      some (tokenSourceReturnState σ word memory owner nextFrame stack) := by
  have scalarRead := word_lookup_sound σ ρ scope id word lookupWord
  have memoryRead : IR.lookup (scope.denote σ ρ) memoryId =
      .ok (.memory memory) := by
    simp [IR.lookup, lookup_denote, lookupMemory, Slot.denote, current]
  have worldRead : IR.lookup (scope.denote σ ρ) worldId = .ok .world := by
    simp [IR.lookup, lookup_denote, lookupWorld, Slot.denote, world]
  cases stack <;>
    simp [IR.step, IR.execute, sourceState, function, block, terminator,
      scalarRead, memoryRead, worldRead, returns, Word.denote, IR.Value.type,
      tokenSourceReturnState, cutResume, Bind.bind, Except.bind, Pure.pure, Except.pure]
  all_goals split <;> split at * <;> simp_all

theorem machine_return_step (mode : EvoIR.Machine.Mode) (target : EvoIR.Machine.Program)
    (state : EvoIR.Machine.State) (activation : EvoIR.Machine.Activation)
    (function : EvoIR.Machine.Function) (block : EvoIR.Machine.Block)
    (ids : List EvoIR.Machine.VReg) (values : List EvoIR.Machine.Value)
    (running : state.control = .running activation)
    (foundFunction : target.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (atTerminator : activation.pc = block.insts.length)
    (terminator : block.term = .ret ids)
    (returned : ids.mapM (EvoIR.Machine.read activation.regs) = .ok values) :
    EvoIR.Machine.step mode target state = some
      (machineReturnState values state.memory activation.owner state.nextFrame state.stack) := by
  have noInstruction : block.insts[activation.pc]? = none := by
    rw [atTerminator]
    exact List.getElem?_eq_none (Nat.le_refl _)
  simp only [EvoIR.Machine.step, running, EvoIR.Machine.execute,
    foundFunction, foundBlock, noInstruction, atTerminator, bne_self_eq_false,
    Bool.false_eq_true, ↓reduceIte, terminator, returned]
  cases stack : state.stack with
  | nil => simp [stack, machineReturnState]; rfl
  | cons frame rest =>
    simp only [stack, Bind.bind, Except.bind, machineReturnState]
    cases bound : EvoIR.Machine.bind frame.caller.regs frame.results values <;>
      simp [bound, Pure.pure, Except.pure]
    all_goals cases state <;> simp_all

def TokenReturnOpenDiagram {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : Bank Γ)
    (type : Source.IntType) (word : Word Γ) (checks : List (Guard Γ) := [])
    (memoryKey : Nat := 0) (worldKey : Nat := 1) : Prop :=
  word.1 = type.width ∧ ∀ σ (ρ : Nat → IR.Value) memory owner nextFrame incoming sourceStack targetStack
    (observationType : Source.IntType)
    (boundary : IR.State → EvoIR.Machine.State → Prop),
    guards σ checks = .ok () →
    ρ memoryKey = .memory memory → ρ worldKey = .world →
    boundary (tokenSourceReturnState σ word memory owner nextFrame sourceStack)
      (machineReturnState (machineInputs σ [word]) memory owner nextFrame targetStack) →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType) boundary
      (sourceState σ ρ sf sb sb.instructions.length scope owner nextFrame sourceStack)
      (machineState σ tf tb tb.insts.length bank owner nextFrame incoming targetStack memory)

def tokenReturnOpen {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : Bank Γ)
    (type : Source.IntType) (checks : List (Guard Γ) := [])
    (memoryKey : Nat := 0) (worldKey : Nat := 1) :
    Option {word : Word Γ // TokenReturnOpenDiagram source target sf sb scope tf tb bank type word checks memoryKey worldKey} := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          if supported : type.width = 32 ∨ type.width = 64 then
            if returns : sf.returns = [.bits type.width, .memory, .world] then
              match sourceReturn : sb.terminator, targetReturn : tb.term with
              | .return [id, memoryId, worldId], .ret [reg] => do
                if memoryToken : scope.lookup memoryId = some (.opaque memoryKey) then
                  if worldToken : scope.lookup worldId = some (.opaque worldKey) then
                    let word : {word // scope.word id = some word} ←
                      match found : scope.word id with
                      | none => none
                      | some word => some ⟨word, found⟩
                    if typed : word.val.1 = type.width then
                      let machine ← wordOperand bank reg
                      let equal ← checkWordUnder checks word.val machine.val
                      return ⟨word.val, typed, by
                        intro σ ρ memory owner nextFrame incoming sourceStack targetStack observationType boundary passed current world atBoundary
                        have sourceStep := token_source_return_step source sf sb scope word.val
                          id memoryId worldId memoryKey worldKey σ ρ memory owner nextFrame sourceStack
                          sourceFunction sourceBlock sourceReturn word.property memoryToken worldToken current world
                          (by simpa only [typed] using returns)
                        have targetRead : EvoIR.Machine.read (bankDenote σ bank) reg =
                            .ok (.int ((denoteTerm σ word.val.2).setWidth 64)) := by
                          have actual := word_operand_value bank reg machine.val machine.property σ
                          rw [EvoIR.Machine.operandValue_readOperand, equal.down σ passed] at actual
                          exact actual
                        have targetStep := machine_return_step .ssa target
                          (machineState σ tf tb tb.insts.length bank owner nextFrame incoming targetStack memory)
                          ⟨tf.name, tb.id, tb.insts.length, bankDenote σ bank, owner, incoming⟩ tf tb
                          [reg] (machineInputs σ [word.val]) rfl targetFunction targetBlock rfl targetReturn
                          (by simp [targetRead, machineInputs, Bind.bind, Except.bind, Pure.pure, Except.pure])
                        exact Transition.OpenCheckpoint.exit atBoundary (.single sourceStep) (.single targetStep)⟩
                    else none
                  else none
                else none
              | _, _ => none
            else none
          else none
        else none
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
