import EvoIR.MachineSelectionEdges
import EvoIR.MachineSelectionPair
import EvoIR.Core.SymbolicEdge

/-!
Canonical scalar cutpoints use a fresh valuation of the actual simultaneous
edge arguments. Backedges do not retain a symbolic expression history or
facts from just one predecessor. Internal Boolean arguments retain their
one-bit type; they are not widened to a C integer parameter.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def cutpointWord (width index : Nat) : Word inputWidths :=
  ⟨width, .cast .trunc width (.var (inputName index) 64 rfl)⟩

def cutpointWords (widths : List Nat) : List (Word inputWidths) :=
  widths.mapIdx fun index width => cutpointWord width index

def rebaseWords {Γ : Var → Nat} (σ : Subst Γ) (words : List (Word Γ)) : Subst inputWidths :=
  fun name => ((words[name.length]?).map fun word =>
    (denoteTerm σ word.2).setWidth 64).getD 0

theorem cutpoint_word_rebase {Γ : Var → Nat} (σ : Subst Γ)
    (words : List (Word Γ)) (index : Nat) (word : Word Γ)
    (found : words[index]? = some word) (bounded : word.1 ≤ 64) :
    (cutpointWord word.1 index).denote (rebaseWords σ words) = word.denote σ := by
  have selected : rebaseWords σ words (inputName index) =
      (denoteTerm σ word.2).setWidth 64 := by
    simp [rebaseWords, inputName, found]
  simp only [cutpointWord, Word.denote, denoteTerm, selected, denoteOp,
    BitVec.truncate_eq_setWidth]
  rw [BitVec.setWidth_setWidth_of_le _ bounded, BitVec.setWidth_eq]

theorem cutpoint_words_rebase {Γ : Var → Nat} (σ : Subst Γ)
    (words : List (Word Γ)) (bounded : ∀ word ∈ words, word.1 ≤ 64) :
    (cutpointWords (words.map Sigma.fst)).map (Word.denote (rebaseWords σ words)) =
      words.map (Word.denote σ) := by
  apply List.ext_getElem
  · simp [cutpointWords]
  · intro index leftBound rightBound
    have inside : index < words.length := by simpa using rightBound
    simpa only [List.getElem_map, cutpointWords, List.getElem_mapIdx] using
      cutpoint_word_rebase σ words index words[index]
        (List.getElem?_eq_getElem inside) (bounded _ (List.getElem_mem inside))

private def scalarEncoding : IR.Value → EvoIR.Machine.Value
  | .bits _ bits => .int (bits.setWidth 64)
  | _ => .int 0

theorem cutpoint_machine_rebase {Γ : Var → Nat} (σ : Subst Γ)
    (words : List (Word Γ)) (bounded : ∀ word ∈ words, word.1 ≤ 64) :
    machineInputs (rebaseWords σ words) (cutpointWords (words.map Sigma.fst)) =
      machineInputs σ words := by
  have equality := congrArg (List.map scalarEncoding) (cutpoint_words_rebase σ words bounded)
  simpa only [machineInputs, List.map_map, Function.comp_def, Word.denote, scalarEncoding]
    using equality

structure ScalarCutpoint (sf : IR.Function) (tf : EvoIR.Machine.Function) where
  sourceBlock : IR.Block
  targetBlock : EvoIR.Machine.Block
  widths : List Nat
  bounded : ∀ width ∈ widths, width ≤ 64
  scope : Scope inputWidths
  bank : Bank inputWidths
  sourceFound : sf.blocks.find? (·.name == sourceBlock.name) = some sourceBlock
  targetFound : tf.blocks.find? (·.id == targetBlock.id) = some targetBlock
  sourceBinding : ∀ σ,
    IR.bind sourceBlock.parameters ((cutpointWords widths).map (Word.denote σ)) =
      .ok (scope.denote σ (fun _ => .world))
  targetBinding : ∀ σ,
    EvoIR.Machine.bind [] targetBlock.args (machineInputs σ (cutpointWords widths)) =
      .ok (bankDenote σ bank)

private def scalarWidths : List IR.Binding → Option (List Nat)
  | [] => some []
  | binding :: rest => do
    let .bits width := binding.type | none
    let tail ← scalarWidths rest
    return width :: tail

def scalarCutpoint (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (sourceLabel : String) (targetId : Nat) : Option (ScalarCutpoint sf tf) := do
  let sb : {b // sf.blocks.find? (·.name == sourceLabel) = some b} ←
    match found : sf.blocks.find? (·.name == sourceLabel) with
    | none => none
    | some b => some ⟨b, found⟩
  let tb : {b // tf.blocks.find? (·.id == targetId) = some b} ←
    match found : tf.blocks.find? (·.id == targetId) with
    | none => none
    | some b => some ⟨b, found⟩
  let widths ← scalarWidths sb.val.parameters
  if bounded : ∀ width ∈ widths, width ≤ 64 then
    let words := cutpointWords widths
    let scope : {scope // bindSlots (fun _ => .world) sb.val.parameters
        (words.map Slot.word) = some scope} ←
      match found : bindSlots (fun _ => .world) sb.val.parameters (words.map Slot.word) with
      | none => none
      | some scope => some ⟨scope, found⟩
    let bank ← bindWords tb.val.args (words.map WordExpr.input)
    have sourceName : sb.val.name = sourceLabel := by simpa using List.find?_some sb.property
    have targetName : tb.val.id = targetId := by simpa using List.find?_some tb.property
    return {
      sourceBlock := sb.val, targetBlock := tb.val, widths, bounded,
      scope := scope.val, bank := bank.val
      sourceFound := by simpa only [sourceName] using sb.property
      targetFound := by simpa only [targetName] using tb.property
      sourceBinding := by
        intro σ
        have exactBind := bind_slots_sound σ (fun _ => .world) (fun _ => .world) (by intro id; rfl)
          sb.val.parameters (words.map Slot.word) scope.val scope.property
        simpa only [List.map_map, Function.comp_def, Slot.denote] using exactBind
      targetBinding := by
        intro σ
        simpa only [machineInputs, List.map_map, Function.comp_def, WordExpr.denote]
          using bank.property σ }
  else none

def readSourceWords {Γ : Var → Nat} (scope : Scope Γ) : (ids : List Nat) →
    Option {words : List (Word Γ) // ∀ σ,
      ids.mapM (IR.lookup (scope.denote σ (fun _ => .world))) =
        .ok (words.map (Word.denote σ))}
  | [] => some ⟨[], by intro σ; rfl⟩
  | id :: rest => do
    let word : {word // scope.word id = some word} ←
      match found : scope.word id with
      | none => none
      | some word => some ⟨word, found⟩
    let tail ← readSourceWords scope rest
    return ⟨word.val :: tail.val, by
      intro σ
      have read := word_lookup_sound σ (fun _ => .world) scope id word.val word.property
      simp only [List.mapM_cons, read, tail.property σ, List.map_cons,
        bind, Except.bind, pure, Except.pure]⟩

def matchWordVectors {Γ : Var → Nat} (checks : List (Guard Γ)) : (source : List (Word Γ)) →
    (target : List (WordExpr Γ)) → Option (PLift (∀ σ,
      guards σ checks = .ok () →
      target.map (fun word => EvoIR.Machine.Value.int (word.denote σ)) = machineInputs σ source))
  | [], [] => some ⟨by intro σ passed; rfl⟩
  | first :: rest, candidate :: tail => do
    let equal ← checkWordUnder checks first candidate
    let suffix ← matchWordVectors checks rest tail
    return ⟨by intro σ passed
               simp only [List.map_cons, machineInputs, equal.down σ passed, suffix.down σ passed]⟩
  | _, _ => none

/-- Both real edge evaluators bind one captured vector. The successor gets a
fresh valuation, so a backedge cannot inherit an earlier iteration's terms. -/
def scalarEdge {Γ : Var → Nat} {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (point : ScalarCutpoint sf tf) (scope : Scope Γ) (bank : Bank Γ)
    (sourceEdge : IR.Edge) (targetId : Nat) (targetArgs : List EvoIR.Machine.VReg)
    (checks : List (Guard Γ) := []) :
    Option (PLift (∀ σ owner incoming sourceBlock targetBlock sourcePC targetPC,
      guards σ checks = .ok () →
      ∃ next : Subst inputWidths,
        IR.jump sf ⟨sf.name, sourceBlock, sourcePC, scope.denote σ (fun _ => .world), owner⟩ sourceEdge =
          .ok ⟨sf.name, point.sourceBlock.name, 0, point.scope.denote next (fun _ => .world), owner⟩ ∧
        EvoIR.Machine.jump .ssa tf
          ⟨tf.name, targetBlock, targetPC, bankDenote σ bank, owner, incoming⟩ targetId targetArgs =
          .ok ⟨tf.name, point.targetBlock.id, 0, bankDenote next point.bank, owner, incoming⟩)) := do
  if sourceName : sourceEdge.block = point.sourceBlock.name then
    if targetName : targetId = point.targetBlock.id then
      let sourceWords ← readSourceWords scope sourceEdge.arguments
      let targetWords ← readWords bank targetArgs
      let equal ← matchWordVectors checks sourceWords.val targetWords.val
      if sameWidths : sourceWords.val.map Sigma.fst = point.widths then
        return ⟨by
          intro σ owner incoming sourceBlock targetBlock sourcePC targetPC passed
          let next := rebaseWords σ sourceWords.val
          have bounded : ∀ word ∈ sourceWords.val, word.1 ≤ 64 := by
            intro word member
            apply point.bounded word.1
            rw [← sameWidths]
            exact List.mem_map.mpr ⟨word, member, rfl⟩
          have sourceBind := point.sourceBinding next
          rw [← sameWidths, cutpoint_words_rebase σ sourceWords.val bounded] at sourceBind
          have targetBind := point.targetBinding next
          rw [← sameWidths, cutpoint_machine_rebase σ sourceWords.val bounded] at targetBind
          refine ⟨next, ?_, ?_⟩
          · simp only [IR.jump, sourceName, point.sourceFound, sourceWords.property σ,
              bind, Except.bind, sourceBind, pure, Except.pure]
          · simp only [EvoIR.Machine.jump, targetName, point.targetFound, targetWords.property σ,
              bind, Except.bind, equal.down σ passed, targetBind, pure, Except.pure]⟩
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
