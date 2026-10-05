import EvoIR.MachineSelectionCutpoint
import EvoIR.MachineSelectionTokens
import EvoIR.MachineSelectionConditional
import EvoIR.MachineSelectionCallEntry

/-! Canonical SSA cutpoints retain every actual source memory/world position.
Only scalar words occupy MIR arguments. Each edge captures its full source
vector before rebinding, and rebases private memory without retaining dead
token histories from earlier iterations. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def scalarTokenWidths : List IR.Ty → Option (List Nat)
  | [] => some []
  | .bits width :: rest => do return width :: (← scalarTokenWidths rest)
  | .memory :: rest | .world :: rest => scalarTokenWidths rest
  | _ => none

def weaveTokenSlots {Γ : Var → Nat} (memoryKey worldKey : Nat) :
    List IR.Ty → List (Word Γ) → Option (List (Slot Γ))
  | [], [] => some []
  | .bits _ :: rest, word :: words => do
    return .word word :: (← weaveTokenSlots memoryKey worldKey rest words)
  | .memory :: rest, words => do
    return .opaque memoryKey :: (← weaveTokenSlots memoryKey worldKey rest words)
  | .world :: rest, words => do
    return .opaque worldKey :: (← weaveTokenSlots memoryKey worldKey rest words)
  | _, _ => none

def weaveTokenValues (memory : IR.Memory) : List IR.Ty → List IR.Value → Option (List IR.Value)
  | [], [] => some []
  | .bits _ :: rest, value :: values => do return value :: (← weaveTokenValues memory rest values)
  | .memory :: rest, values => do return .memory memory :: (← weaveTokenValues memory rest values)
  | .world :: rest, values => do return .world :: (← weaveTokenValues memory rest values)
  | _, _ => none

theorem weave_token_slots_denote {Γ : Var → Nat} (memoryKey worldKey : Nat)
    (types : List IR.Ty) (words : List (Word Γ)) (slots : List (Slot Γ))
    (woven : weaveTokenSlots memoryKey worldKey types words = some slots)
    (σ : Subst Γ) (ρ : Nat → IR.Value) (memory : IR.Memory)
    (current : ρ memoryKey = .memory memory) (world : ρ worldKey = .world) :
    weaveTokenValues memory types (words.map (Word.denote σ)) = some (slots.map (Slot.denote σ ρ)) := by
  induction types generalizing words slots with
  | nil =>
    cases words <;> simp [weaveTokenSlots] at woven
    subst slots
    rfl
  | cons type rest ih =>
    cases type <;> try simp [weaveTokenSlots] at woven
    all_goals
      cases words <;> simp only [weaveTokenSlots, bind, Option.bind] at woven
      all_goals
        first
        | contradiction
        | split at woven
          · contradiction
          · rename_i tail found
            cases Option.some.inj woven
            have tailLaw := ih _ _ found
            try simp only [List.map_cons, List.map_nil] at tailLaw
            simp only [List.map_cons, List.map_nil, Slot.denote, current, world, weaveTokenValues,
              tailLaw, bind, Option.bind, pure]

def readMixedWords {Γ : Var → Nat} (memoryKey worldKey : Nat) :
    (types : List IR.Ty) → (slots : List (Slot Γ)) →
    Option {words : List (Word Γ) // weaveTokenSlots memoryKey worldKey types words = some slots}
  | [], [] => some ⟨[], rfl⟩
  | .bits _ :: types, .word word :: slots => do
    let tail ← readMixedWords memoryKey worldKey types slots
    return ⟨word :: tail.val, by simp only [weaveTokenSlots, tail.property, bind, Option.bind, pure]⟩
  | .memory :: types, .opaque key :: slots => do
    if same : key = memoryKey then
      let tail ← readMixedWords memoryKey worldKey types slots
      return ⟨tail.val, by simp only [weaveTokenSlots, tail.property, same, bind, Option.bind, pure]⟩
    else none
  | .world :: types, .opaque key :: slots => do
    if same : key = worldKey then
      let tail ← readMixedWords memoryKey worldKey types slots
      return ⟨tail.val, by simp only [weaveTokenSlots, tail.property, same, bind, Option.bind, pure]⟩
    else none
  | _, _ => none

structure TokenCutpoint (sf : IR.Function) (tf : EvoIR.Machine.Function) where
  sourceBlock : IR.Block
  targetBlock : EvoIR.Machine.Block
  widths : List Nat
  bounded : ∀ width ∈ widths, width ≤ 64
  slots : List (Slot inputWidths)
  woven : weaveTokenSlots 0 1 (sourceBlock.parameters.map IR.Binding.type) (cutpointWords widths) = some slots
  scope : Scope inputWidths
  bank : Bank inputWidths
  sourceFound : sf.blocks.find? (·.name == sourceBlock.name) = some sourceBlock
  targetFound : tf.blocks.find? (·.id == targetBlock.id) = some targetBlock
  sourceBinding : ∀ σ memory,
    IR.bind sourceBlock.parameters (slots.map (Slot.denote σ (tokenEnvironment memory))) =
      .ok (scope.denote σ (tokenEnvironment memory))
  targetBinding : ∀ σ,
    EvoIR.Machine.bind [] targetBlock.args (machineInputs σ (cutpointWords widths)) =
      .ok (bankDenote σ bank)

def tokenCutpoint (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (sourceLabel : String) (targetId : Nat) : Option (TokenCutpoint sf tf) := do
  let sb : {b // sf.blocks.find? (·.name == sourceLabel) = some b} ←
    match found : sf.blocks.find? (·.name == sourceLabel) with
    | none => none
    | some b => some ⟨b, found⟩
  let tb : {b // tf.blocks.find? (·.id == targetId) = some b} ←
    match found : tf.blocks.find? (·.id == targetId) with
    | none => none
    | some b => some ⟨b, found⟩
  let widths ← scalarTokenWidths (sb.val.parameters.map IR.Binding.type)
  if bounded : ∀ width ∈ widths, width ≤ 64 then
    let slots : {slots // weaveTokenSlots 0 1 (sb.val.parameters.map IR.Binding.type)
        (cutpointWords widths) = some slots} ←
      match woven : weaveTokenSlots 0 1 (sb.val.parameters.map IR.Binding.type) (cutpointWords widths) with
      | none => none
      | some slots => some ⟨slots, woven⟩
    let scope : {scope // bindSlots tokenTypes sb.val.parameters slots.val = some scope} ←
      match bound : bindSlots tokenTypes sb.val.parameters slots.val with
      | none => none
      | some scope => some ⟨scope, bound⟩
    let bank ← bindWords tb.val.args ((cutpointWords widths).map WordExpr.input)
    have sourceName : sb.val.name = sourceLabel := by simpa using List.find?_some sb.property
    have targetName : tb.val.id = targetId := by simpa using List.find?_some tb.property
    return {
      sourceBlock := sb.val, targetBlock := tb.val, widths, bounded,
      slots := slots.val, woven := slots.property, scope := scope.val, bank := bank.val
      sourceFound := by simpa only [sourceName] using sb.property
      targetFound := by simpa only [targetName] using tb.property
      sourceBinding := by
        intro σ memory
        exact bind_slots_sound σ (tokenEnvironment memory) tokenTypes
          (token_environment_typed memory) sb.val.parameters slots.val scope.val scope.property
      targetBinding := by
        intro σ
        simpa only [machineInputs, List.map_map, Function.comp_def, WordExpr.denote] using bank.property σ }
  else none

def tokenEdge {Γ : Var → Nat} {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (point : TokenCutpoint sf tf) (scope : Scope Γ) (bank : Bank Γ)
    (sourceEdge : IR.Edge) (targetId : Nat) (targetArgs : List EvoIR.Machine.VReg)
    (checks : List (Guard Γ) := []) (memoryKey : Nat := 0) (worldKey : Nat := 1) :
    Option (PLift (∀ σ (ρ : Nat → IR.Value) memory owner incoming sourceBlock targetBlock sourcePC targetPC,
      guards σ checks = .ok () → ρ memoryKey = .memory memory → ρ worldKey = .world →
      ∃ next : Subst inputWidths,
        IR.jump sf ⟨sf.name, sourceBlock, sourcePC, scope.denote σ ρ, owner⟩ sourceEdge =
          .ok ⟨sf.name, point.sourceBlock.name, 0, point.scope.denote next (tokenEnvironment memory), owner⟩ ∧
        EvoIR.Machine.jump .ssa tf
          ⟨tf.name, targetBlock, targetPC, bankDenote σ bank, owner, incoming⟩ targetId targetArgs =
          .ok ⟨tf.name, point.targetBlock.id, 0, bankDenote next point.bank, owner, incoming⟩)) := do
  if sourceName : sourceEdge.block = point.sourceBlock.name then
    if targetName : targetId = point.targetBlock.id then
      let captured : {slots // sourceEdge.arguments.mapM (fun id => scope.lookup id) = some slots} ←
        match found : sourceEdge.arguments.mapM (fun id => scope.lookup id) with
        | none => none
        | some slots => some ⟨slots, found⟩
      let words ← readMixedWords memoryKey worldKey (point.sourceBlock.parameters.map IR.Binding.type) captured.val
      let targetWords ← readWords bank targetArgs
      let equal ← checkArgumentWords checks words.val targetWords.val
      if sameWidths : words.val.map Sigma.fst = point.widths then
        return ⟨by
          intro σ ρ memory owner incoming sourceBlock targetBlock sourcePC targetPC passed current world
          let next := rebaseWords σ words.val
          have bounded : ∀ word ∈ words.val, word.1 ≤ 64 := by
            intro word member
            apply point.bounded word.1
            rw [← sameWidths]
            exact List.mem_map.mpr ⟨word, member, rfl⟩
          have canonicalValues := weave_token_slots_denote 0 1
            (point.sourceBlock.parameters.map IR.Binding.type) (cutpointWords point.widths)
            point.slots point.woven next (tokenEnvironment memory) memory (by rfl) (by rfl)
          rw [← sameWidths, cutpoint_words_rebase σ words.val bounded] at canonicalValues
          have edgeValues := weave_token_slots_denote memoryKey worldKey
            (point.sourceBlock.parameters.map IR.Binding.type) words.val captured.val words.property σ ρ memory current world
          have valuesEqual := Option.some.inj (edgeValues.symm.trans canonicalValues)
          have sourceRead := slots_lookup_sound σ ρ scope sourceEdge.arguments captured.val captured.property
          rw [valuesEqual] at sourceRead
          have targetBind := point.targetBinding next
          rw [← sameWidths, cutpoint_machine_rebase σ words.val bounded] at targetBind
          refine ⟨next, ?_, ?_⟩
          · simp only [IR.jump, sourceName, point.sourceFound, sourceRead, bind, Except.bind,
              point.sourceBinding next memory, pure, Except.pure]
          · simp only [EvoIR.Machine.jump, targetName, point.targetFound, targetWords.property σ,
              bind, Except.bind, equal.down σ passed, targetBind, pure, Except.pure]⟩
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
