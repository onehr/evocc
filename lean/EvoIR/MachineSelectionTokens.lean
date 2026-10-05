import EvoIR.MachineSelectionEntry
import EvoIR.MachineSelectionConditional
import EvoIR.Core.PrivateBindings

/-!
Erase only the actual memory/world SSA tokens, not scalar call arguments.
The environment carries an arbitrary concrete private-memory snapshot. Pure
spans already quantify this environment, so scalar quoting needs no second
integer interpreter or empty-memory substitution.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def tokenEnvironment (memory : IR.Memory) : Nat → IR.Value :=
  privateToken [.memory memory, .world]

def tokenTypes : Nat → IR.Ty := privateTokenType [.memory, .world]

theorem token_environment_typed (memory : IR.Memory) (id : Nat) :
    (tokenEnvironment memory id).type = tokenTypes id :=
  private_token_typed [.memory memory, .world] [.memory, .world] rfl id

def tokenInputSlots (args : List (Word Γ)) : List (Slot Γ) :=
  args.map Slot.word ++ [.opaque 0, .opaque 1]

def tokenInputValues (σ : Subst Γ) (args : List (Word Γ)) (memory : IR.Memory) : List IR.Value :=
  args.map (Word.denote σ) ++ [.memory memory, .world]

def tokenInputTypes (args : List (Word Γ)) : List IR.Ty :=
  args.map (fun word => .bits word.1) ++ [.memory, .world]

theorem token_input_denote (σ : Subst Γ) (args : List (Word Γ)) (memory : IR.Memory) :
    (tokenInputSlots args).map (Slot.denote σ (tokenEnvironment memory)) =
      tokenInputValues σ args memory := by
  simp [tokenInputSlots, tokenInputValues, Slot.denote, tokenEnvironment, privateToken,
    List.map_map, Function.comp_def]

theorem token_input_types (σ : Subst Γ) (args : List (Word Γ)) (memory : IR.Memory) :
    (tokenInputValues σ args memory).map IR.Value.type = tokenInputTypes args := by
  simp [tokenInputValues, tokenInputTypes, List.map_map, Function.comp_def, Word.denote,
    IR.Value.type]

/-- Bind the complete actual source signature, including its two erased
effect tokens. The paired MIR entry still receives every scalar argument. -/
def sourceTokenEntry {Γ : Var → Nat} (function : IR.Function) (args : List (Word Γ)) :
    Option {entry : IR.Block × Scope Γ // ∀ σ memory owner,
      IR.enter function (tokenInputValues σ args memory) owner =
        .ok ⟨function.name, entry.1.name, 0,
          entry.2.denote σ (tokenEnvironment memory), owner⟩} := do
  let block : {block // function.blocks.head? = some block} ←
    match found : function.blocks.head? with
    | none => none
    | some block => some ⟨block, found⟩
  if signature : function.parameters = tokenInputTypes args then
    if blockTypes : block.val.parameters.map IR.Binding.type = function.parameters then
      let scope : {scope // bindSlots tokenTypes block.val.parameters (tokenInputSlots args) = some scope} ←
        match bound : bindSlots tokenTypes block.val.parameters (tokenInputSlots args) with
        | none => none
        | some scope => some ⟨scope, bound⟩
      return ⟨(block.val, scope.val), by
        intro σ memory owner
        have bound := bind_slots_sound σ (tokenEnvironment memory) tokenTypes
          (token_environment_typed memory) block.val.parameters (tokenInputSlots args)
          scope.val scope.property
        rw [token_input_denote] at bound
        have types : (tokenInputValues σ args memory).map IR.Value.type = function.parameters :=
          (token_input_types σ args memory).trans signature.symm
        simp [IR.enter, types, block.property, blockTypes, bound,
          Bind.bind, Except.bind, Pure.pure, Except.pure]⟩
    else none
  else none

theorem token_edge_sound {Γ : Var → Nat} (σ : Subst Γ) (memory : IR.Memory)
    (function : IR.Function) (activation : IR.Activation) (scope next : Scope Γ)
    (successor : IR.Edge) (values : activation.values = scope.denote σ (tokenEnvironment memory))
    (accepted : edge tokenTypes function scope successor = some next) :
    IR.jump function activation successor = .ok {activation with block := successor.block, pc := 0, values := next.denote σ (tokenEnvironment memory)} :=
  edge_sound σ (tokenEnvironment memory) tokenTypes (token_environment_typed memory)
    function activation scope next successor values accepted

def TokenReturnDiagram {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : Bank Γ)
    (type : Source.IntType) (checks : List (Guard Γ) := []) : Prop :=
  ∀ σ memory owner nextFrame incoming,
    guards σ checks = .ok () →
    EvoIR.Machine.SelectionRefines source target type
      (sourceState σ (tokenEnvironment memory) sf sb sb.instructions.length scope owner nextFrame [])
      (machineState σ tf tb tb.insts.length bank owner nextFrame incoming [] memory)

def tokenReturn {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : Bank Γ)
    (type : Source.IntType) (checks : List (Guard Γ) := []) :
    Option (PLift (TokenReturnDiagram source target sf sb scope tf tb bank type checks)) := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          if supported : type.width = 32 ∨ type.width = 64 then
            if returns : sf.returns = [.bits type.width, .memory, .world] then
              match sourceReturn : sb.terminator, targetReturn : tb.term with
              | .return [id, memoryId, worldId], .ret [reg] => do
                if memoryToken : scope.lookup memoryId = some (.opaque 0) then
                  if worldToken : scope.lookup worldId = some (.opaque 1) then
                    let word : {word // scope.word id = some word} ←
                      match found : scope.word id with
                      | none => none
                      | some word => some ⟨word, found⟩
                    if typed : word.val.1 = type.width then
                      let machine ← wordOperand bank reg
                      let equal ← checkWordUnder checks word.val machine.val
                      return ⟨by
                        intro σ memory owner nextFrame incoming passed
                        let bits := denoteTerm σ word.val.2
                        let s := sourceState σ (tokenEnvironment memory) sf sb sb.instructions.length
                          scope owner nextFrame []
                        let t := machineState σ tf tb tb.insts.length bank owner nextFrame incoming [] memory
                        let ss : IR.State := {s with control := .halted (.ok [.bits word.val.1 bits,
                          .memory (memory.endFrame owner), .world])}
                        let tt : EvoIR.Machine.State := {t with control := .halted (.ok [.int (machine.val.denote σ)]), memory := memory.endFrame owner}
                        have sourceRead := word_lookup_sound σ (tokenEnvironment memory) scope id
                          word.val word.property
                        have memoryRead : IR.lookup (scope.denote σ (tokenEnvironment memory)) memoryId =
                            .ok (.memory memory) := by
                          simp [IR.lookup, lookup_denote, memoryToken, Slot.denote,
                            tokenEnvironment, privateToken]
                        have worldRead : IR.lookup (scope.denote σ (tokenEnvironment memory)) worldId =
                            .ok .world := by
                          simp [IR.lookup, lookup_denote, worldToken, Slot.denote,
                            tokenEnvironment, privateToken]
                        have sourceStep : IR.step source s = some ss := by
                          simp [s, ss, sourceState, IR.step, IR.execute, sourceFunction, sourceBlock,
                            sourceReturn, sourceRead, memoryRead, worldRead, Word.denote, IR.Value.type,
                            returns, typed, bits, Bind.bind, Except.bind, Pure.pure, Except.pure]
                        have targetRead : EvoIR.Machine.read (bankDenote σ bank) reg =
                            .ok (.int (machine.val.denote σ)) := by
                          simpa only [EvoIR.Machine.operandValue_readOperand] using
                            word_operand_value bank reg machine.val machine.property σ
                        have executed := EvoIR.Machine.execute_ret_noCaller .ssa target t
                          ⟨tf.name, tb.id, tb.insts.length, bankDenote σ bank, owner, incoming⟩
                          tf tb [reg] [.int (machine.val.denote σ)] targetFunction targetBlock
                          (List.getElem?_eq_none (Nat.le_refl _)) rfl targetReturn
                          (by simp [targetRead, Bind.bind, Except.bind, Pure.pure, Except.pure]) rfl
                        have targetStep : EvoIR.Machine.step .ssa target t = some tt := by
                          change some (match EvoIR.Machine.execute .ssa target t
                              ⟨tf.name, tb.id, tb.insts.length, bankDenote σ bank, owner, incoming⟩ with
                            | .ok state => state
                            | .error fault => {t with control := .halted (.error fault)}) = some tt
                          rw [executed]
                          rfl
                        apply Transition.refines_of_terminals (.next sourceStep (.refl _))
                          (.next targetStep (.refl _)) rfl rfl
                          (o := Observation.returned type bits.toNat)
                        · simp [observeTarget, ss, typed, bits]
                        · have value := equal.down σ passed
                          have width : word.val.1 ≤ 64 := by omega
                          simp only [EvoIR.Machine.observe, tt, value]
                          have retained : ((bits.setWidth 64).setWidth type.width).toNat = bits.toNat := by
                            rw [← typed]
                            simp [BitVec.setWidth_setWidth_of_le bits width]
                          have admitted : (type.width == 32 || type.width == 64) = true := by
                            rcases supported with h | h <;> simp [h]
                          simp only [admitted, ↓reduceIte]
                          change some (Observation.returned type
                            ((bits.setWidth 64).setWidth type.width).toNat) = _
                          rw [retained]⟩
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
