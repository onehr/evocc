import EvoIR.MachineSelectionTokens
import EvoIR.MachineSelectionEdges
import EvoIR.Core.TargetCall

/-! Exact token-erasing call entry. Both actual operand vectors are read and
checked; no ABI prefix, invented arguments, or callee body is substituted. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def effectInputSlots {Γ : Var → Nat} (words : List (Word Γ)) (memoryKey worldKey : Nat) : List (Slot Γ) :=
  words.map Slot.word ++ [.opaque memoryKey, .opaque worldKey]

def stripTokenSlots {Γ : Var → Nat} (memoryKey worldKey : Nat) : (slots : List (Slot Γ)) →
    Option {words : List (Word Γ) // slots = effectInputSlots words memoryKey worldKey}
  | [.opaque actualMemory, .opaque actualWorld] =>
    if same : actualMemory = memoryKey ∧ actualWorld = worldKey then
      some ⟨[], by simp [effectInputSlots, same.1, same.2]⟩ else none
  | .word first :: rest => do
    let tail ← stripTokenSlots memoryKey worldKey rest
    return ⟨first :: tail.val, by
      simp only [effectInputSlots, List.map_cons, List.cons_append]
      exact congrArg (Slot.word first :: ·) tail.property⟩
  | _ => none

/-- Read the complete logical call vector, including the current memory/world
tail. Physical argument-register limits never truncate this vector. -/
def readTokenArguments {Γ : Var → Nat} (scope : Scope Γ) (ids : List Nat) (memoryKey worldKey : Nat) :
    Option {words : List (Word Γ) // ∀ σ (ρ : Nat → IR.Value) memory,
      ρ memoryKey = .memory memory → ρ worldKey = .world →
      ids.mapM (IR.lookup (scope.denote σ ρ)) =
        .ok (tokenInputValues σ words memory)} := do
  let slots : {slots // ids.mapM (fun id => scope.lookup id) = some slots} ←
    match found : ids.mapM (fun id => scope.lookup id) with
    | none => none
    | some slots => some ⟨slots, found⟩
  let words ← stripTokenSlots memoryKey worldKey slots.val
  return ⟨words.val, by
    intro σ ρ memory current world
    have actual := slots_lookup_sound σ ρ scope ids slots.val slots.property
    rw [words.property] at actual
    simpa only [effectInputSlots, List.map_append, List.map_map, Function.comp_def,
      List.map_cons, List.map_nil, Slot.denote, current, world, tokenInputValues] using actual⟩

def checkArgumentWords {Γ : Var → Nat} (checks : List (Guard Γ)) : (source : List (Word Γ)) →
    (target : List (WordExpr Γ)) → Option (PLift (∀ σ,
      guards σ checks = .ok () →
      target.map (fun word => EvoIR.Machine.Value.int (word.denote σ)) = machineInputs σ source))
  | [], [] => some ⟨by intro σ passed; rfl⟩
  | first :: rest, actual :: tail => do
    let head ← checkWordUnder checks first actual
    let suffix ← checkArgumentWords checks rest tail
    return ⟨by
      intro σ passed
      simp only [List.map_cons, machineInputs, head.down σ passed]
      exact congrArg (EvoIR.Machine.Value.int ((denoteTerm σ first.2).setWidth 64) :: ·)
        (suffix.down σ passed)⟩
  | _, _ => none

structure TokenCallEntryResult (Γ : Var → Nat) (source : IR.Function)
    (target : EvoIR.Machine.Function) where
  arguments : List (Word Γ)
  sourceBlock : IR.Block
  sourceScope : Scope Γ
  targetBlock : EvoIR.Machine.Block
  targetBank : Bank Γ
  sourceEntry : ∀ σ memory owner, IR.enter source (tokenInputValues σ arguments memory) owner =
    .ok ⟨source.name, sourceBlock.name, 0, sourceScope.denote σ (tokenEnvironment memory), owner⟩
  targetEntry : ∀ σ owner, EvoIR.Machine.enter target (machineInputs σ arguments) owner =
    .ok ⟨target.name, targetBlock.id, 0, bankDenote σ targetBank, owner, machineInputs σ arguments⟩

def TokenCallEntryDiagram {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (scope : Scope Γ) (inst : IR.Instruction)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank Γ)
    (targetResults : List EvoIR.Machine.VReg) (sc : IR.Function) (tc : EvoIR.Machine.Function)
    (entry : TokenCallEntryResult Γ sc tc) (checks : List (Guard Γ) := [])
    (memoryKey : Nat := 0) (worldKey : Nat := 1) : Prop :=
  ∀ σ (ρ : Nat → IR.Value) memory owner nextFrame incoming sourceStack targetStack,
    guards σ checks = .ok () →
    ρ memoryKey = .memory memory → ρ worldKey = .world →
    IR.step source (sourceState σ ρ sf sb spc scope owner nextFrame sourceStack) =
      some ⟨.running ⟨sc.name, entry.sourceBlock.name, 0,
        entry.sourceScope.denote σ (tokenEnvironment memory), nextFrame⟩,
        ⟨⟨sf.name, sb.name, spc+1, scope.denote σ ρ, owner⟩,
          inst.results⟩ :: sourceStack, nextFrame+1⟩ ∧
    EvoIR.Machine.step .ssa target
      (machineState σ tf tb tpc bank owner nextFrame incoming targetStack memory) =
      some ⟨.running ⟨tc.name, entry.targetBlock.id, 0, bankDenote σ entry.targetBank,
        nextFrame, machineInputs σ entry.arguments⟩,
        ⟨⟨tf.name, tb.id, tpc+1, bankDenote σ bank, owner, incoming⟩,
          targetResults⟩ :: targetStack, nextFrame+1, memory⟩

def tokenCallEntry {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank Γ)
    (sc : IR.Function) (tc : EvoIR.Machine.Function) (checks : List (Guard Γ) := [])
    (memoryKey : Nat := 0) (worldKey : Nat := 1) :
    Option {pair : IR.Instruction × List EvoIR.Machine.VReg × TokenCallEntryResult Γ sc tc //
      TokenCallEntryDiagram source target sf sb spc scope pair.1 tf tb tpc bank pair.2.1 sc tc pair.2.2 checks memoryKey worldKey} := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          if sourceCallee : source.functions.find? (·.name == sc.name) = some sc then
            if targetCallee : target.functions.find? (·.name == tc.name) = some tc then
              if sameName : sc.name = tc.name then
                let inst : {inst // sb.instructions[spc]? = some inst} ←
                  match found : sb.instructions[spc]? with
                  | none => none
                  | some inst => some ⟨inst, found⟩
                if call : inst.val.op = .call sc.name then
                  if resultTypes : inst.val.results.map IR.Binding.type = sc.returns then
                    match instruction : tb.insts[tpc]? with
                    | some (.bl symbol args results) => do
                      if symbolName : symbol = tc.name then
                        let sourceArgs ← readTokenArguments scope inst.val.operands memoryKey worldKey
                        let targetArgs ← readWords bank args
                        let sameArgs ← checkArgumentWords checks sourceArgs.val targetArgs.val
                        let sourceEntry ← sourceTokenEntry sc sourceArgs.val
                        let targetEntry ← machineEntry tc sourceArgs.val
                        let result : TokenCallEntryResult Γ sc tc :=
                          ⟨sourceArgs.val, sourceEntry.val.1, sourceEntry.val.2,
                            targetEntry.val.1, targetEntry.val.2, sourceEntry.property, targetEntry.property⟩
                        return ⟨(inst.val, results, result), by
                          intro σ ρ memory owner nextFrame incoming sourceStack targetStack passed current world
                          constructor
                          · have step := IR.call_enters source
                              (sourceState σ ρ sf sb spc scope owner nextFrame sourceStack)
                              ⟨sf.name, sb.name, spc, scope.denote σ ρ, owner⟩
                              ⟨sc.name, sourceEntry.val.1.name, 0,
                                sourceEntry.val.2.denote σ (tokenEnvironment memory), nextFrame⟩
                              sf sc sb inst.val (tokenInputValues σ sourceArgs.val memory)
                              rfl sourceFunction sourceBlock inst.property call sourceCallee
                              (sourceArgs.property σ ρ memory current world) resultTypes (sourceEntry.property σ memory nextFrame)
                            cases step with
                            | execute equation => exact equation
                          · have readArgs := targetArgs.property σ
                            rw [sameArgs.down σ passed] at readArgs
                            have step := EvoIR.Machine.step_bl .ssa target
                              (machineState σ tf tb tpc bank owner nextFrame incoming targetStack memory)
                              ⟨tf.name, tb.id, tpc, bankDenote σ bank, owner, incoming⟩
                              tf tb symbol args results (machineInputs σ sourceArgs.val) tc
                              ⟨tc.name, targetEntry.val.1.id, 0, bankDenote σ targetEntry.val.2,
                                nextFrame, machineInputs σ sourceArgs.val⟩ rfl
                              targetFunction targetBlock instruction readArgs
                              (by simpa only [symbolName] using targetCallee)
                              (targetEntry.property σ nextFrame)
                            exact step⟩
                      else none
                    | _ => none
                  else none
                else none
              else none
            else none
          else none
        else none
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
