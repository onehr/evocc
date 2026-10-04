import EvoIR.Core.TargetCallSpan
import EvoIR.Core.TargetCutpoint

/-! Call-aware target cutpoints. Frame counters are existential at each
successor, not reset to the entry counter. The actual memory/world signature
is checked on every return; scalar-only functions use the same construction. -/
namespace EvoIR.Core.Symbolic

def cutResults {Γ : Var → Nat} (tokens : Bool) (word : CWord Γ) : List (Slot Γ) :=
  if tokens then callResults word else [.word word.word]

def cutReturnTypes (tokens : Bool) (type : Source.IntType) : List IR.Ty :=
  [.bits type.width] ++ if tokens then [.memory, .world] else []

def TargetExit.callState {Γ : Var → Nat} (σ : Subst Γ) (tokens : Bool)
    (f : IR.Function) (owner nextFrame : Nat) : TargetExit Γ → IR.State
  | .returned word => ⟨.halted (.ok ((cutResults tokens word).map (Slot.denote σ callTokens))), [], nextFrame⟩
  | .next label scope => targetState σ callTokens f label 0 owner nextFrame scope

def CallingCutRuns {Γ : Var → Nat} (p : IR.Program) (f : IR.Function) (tokens : Bool)
    (advance : Bool) (label : String) (pc : Nat) (scope : Scope Γ)
    (tree : CutTree Γ (TargetExit Γ)) : Prop :=
  ∀ σ owner nextFrame, ∃ final,
    IR.Steps p (targetState σ callTokens f label pc owner nextFrame scope) final ∧
    (advance = true → Relation.TransGen (Transition.Predecessor (IR.step p)) final
      (targetState σ callTokens f label pc owner nextFrame scope)) ∧
    match tree.eval σ with
    | .error fault => final.control = .halted (.error fault)
    | .ok exit => ∃ afterCalls, final = exit.callState σ tokens f owner afterCalls

theorem calling_cut_span {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens advance : Bool) (b : IR.Block) (scope : Scope Γ)
    (before : CutTree Γ (Scope Γ)) (after : CutTree Γ (TargetExit Γ))
    (span : TargetInstructionSpan p f b 0 scope before)
    (continued : CutBind (CallingCutRuns p f tokens advance b.name b.instructions.length) before after) :
    CallingCutRuns p f tokens advance b.name 0 scope after := by
  intro σ owner nextFrame
  obtain ⟨middle, path, outcome⟩ := span σ owner nextFrame []
  rcases continued.outcome σ with ⟨fault, failed, output⟩ |
    ⟨next, rest, success, resumed, output⟩
  · have halted : middle.control = .halted (.error fault) := by simpa [failed] using outcome
    refine ⟨middle, path, ?_, by simpa [output] using halted⟩
    intro _
    apply path.toReaches.positive_of_ne
    intro same
    rw [← same] at halted
    cases halted
  · obtain ⟨afterCalls, same⟩ := (by simpa [success] using outcome :
      ∃ afterCalls, middle = callState σ f b b.instructions.length owner afterCalls next [])
    subst middle
    obtain ⟨last, tail, positive, final⟩ := resumed σ owner afterCalls
    exact ⟨last, path.trans tail,
      fun h => path.toReaches.then_positive (positive h), by simpa [output] using final⟩

theorem calling_cut_return {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens advance : Bool) (b : IR.Block) (scope : Scope Γ) (ids : List Nat) (word : CWord Γ)
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (terminator : b.terminator = .return ids)
    (lookup : ids.mapM (fun id => scope.lookup id) = some (cutResults tokens word))
    (returns : f.returns = cutReturnTypes tokens word.type) :
    CallingCutRuns p f tokens advance b.name b.instructions.length scope (.leaf (.returned word)) := by
  intro σ owner nextFrame
  have values := slots_lookup_sound σ callTokens scope ids (cutResults tokens word) lookup
  have step : IR.step p (targetState σ callTokens f b.name b.instructions.length owner nextFrame scope) =
      some ((TargetExit.returned word).callState σ tokens f owner nextFrame) := by
    cases tokens <;>
      simp [IR.step, IR.execute, targetState, function, block, terminator, values, returns,
        cutResults, cutReturnTypes, callResults, TargetExit.callState, Slot.denote,
        callTokens, IR.Value.type, Word.denote, CWord.word, IR.Memory.endFrame,
        bind, Except.bind, pure, Except.pure]
  exact ⟨_, .next (.execute step) (.refl _), fun _ => .single step, nextFrame, rfl⟩

theorem calling_cut_branch {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens advance : Bool) (b : IR.Block) (scope next : Scope Γ) (successor : IR.Edge)
    (tree : CutTree Γ (TargetExit Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (terminator : b.terminator = .branch successor)
    (binding : edge callTokenTypes f scope successor = some next)
    (tail : CallingCutRuns p f tokens false successor.block 0 next tree) :
    CallingCutRuns p f tokens advance b.name b.instructions.length scope tree := by
  intro σ owner nextFrame
  have step := branch_execution_sound σ callTokens callTokenTypes (by intro id; rfl) p
    (targetState σ callTokens f b.name b.instructions.length owner nextFrame scope)
    ⟨f.name, b.name, b.instructions.length, scope.denote σ callTokens, owner⟩
    f b scope next successor rfl function block rfl terminator rfl (by simp [checkEdge, binding])
  obtain ⟨last, path, _, outcome⟩ := tail σ owner nextFrame
  exact ⟨last, .next (.execute step) path, fun _ => path.toReaches.after_step step, outcome⟩

theorem calling_cut_choice {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens advance : Bool) (b : IR.Block) (scope yesScope noScope : Scope Γ)
    (id : Nat) (condition : Term Γ 1) (yes no : IR.Edge) (a c : CutTree Γ (TargetExit Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (terminator : b.terminator = .conditional id yes no)
    (test : scope.word id = some ⟨1, condition⟩)
    (firstEdge : edge callTokenTypes f scope yes = some yesScope)
    (secondEdge : edge callTokenTypes f scope no = some noScope)
    (first : CallingCutRuns p f tokens false yes.block 0 yesScope a)
    (second : CallingCutRuns p f tokens false no.block 0 noScope c) :
    CallingCutRuns p f tokens advance b.name b.instructions.length scope (.branch condition a c) := by
  intro σ owner nextFrame
  have step := conditional_execution_sound σ callTokens callTokenTypes (by intro id; rfl) p
    (targetState σ callTokens f b.name b.instructions.length owner nextFrame scope)
    ⟨f.name, b.name, b.instructions.length, scope.denote σ callTokens, owner⟩
    f b scope yesScope noScope id condition yes no rfl function block rfl terminator rfl test
    (by simp [checkEdge, firstEdge]) (by simp [checkEdge, secondEdge])
  by_cases zero : (denoteTerm σ condition).toNat == 0
  · obtain ⟨last, path, _, outcome⟩ := second σ owner nextFrame
    have actual := step
    simp only [zero] at actual
    exact ⟨last, .next (.execute actual) path, fun _ => path.toReaches.after_step actual,
      by simpa [CutTree.eval, bne, zero] using outcome⟩
  · obtain ⟨last, path, _, outcome⟩ := first σ owner nextFrame
    have actual := step
    simp only [zero] at actual
    exact ⟨last, .next (.execute actual) path, fun _ => path.toReaches.after_step actual,
      by simpa [CutTree.eval, bne, zero] using outcome⟩

def callingCutDiagram {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens : Bool) (type : Source.IntType) (cutpoints : List String) (budget : Nat)
    (advance : Bool) (scope : Scope Γ) (label : String) :
    Option {tree // CallingCutRuns p f tokens advance label 0 scope tree} :=
  if stopped : advance = false ∧ label ∈ cutpoints then
    some ⟨.leaf (.next label scope), by
      intro σ owner nextFrame
      exact ⟨_, .refl _, by simp [stopped.1], nextFrame, rfl⟩⟩
  else match budget with
  | 0 => none
  | budget+1 => do
    if function : p.functions.find? (·.name == f.name) = some f then
      let block : {b // f.blocks.find? (·.name == label) = some b} ←
        match h : f.blocks.find? (·.name == label) with
        | none => none
        | some b => some ⟨b, h⟩
      have name : block.val.name = label := by simpa using List.find?_some block.property
      have found : f.blocks.find? (·.name == block.val.name) = some block.val := by
        simpa only [name] using block.property
      let span ← targetInstructionSpan p f block.val (budget+1) 0 scope
      let tail ← span.val.bindChecked
        (next := CallingCutRuns p f tokens advance block.val.name block.val.instructions.length)
        (fun next => do
          match terminator : block.val.terminator with
          | .return ids =>
            let id :: _ := ids | none
            match lookup : next.word id with
            | none => none
            | some ⟨n, term⟩ =>
              if width : n = type.width then
                let word : CWord Γ := ⟨type, width ▸ term⟩
                if operands : ids.mapM (fun id => next.lookup id) = some (cutResults tokens word) then
                  if returns : f.returns = cutReturnTypes tokens type then
                    return ⟨.leaf (.returned word), calling_cut_return p f tokens advance block.val
                      next ids word function found terminator operands returns⟩
                  else none
                else none
              else none
          | .branch successor =>
            match binding : edge callTokenTypes f next successor with
            | none => none
            | some after => do
              let tail ← callingCutDiagram p f tokens type cutpoints budget false after successor.block
              return ⟨tail.val, calling_cut_branch p f tokens advance block.val next after successor
                tail.val function found terminator binding tail.property⟩
          | .conditional id yes no =>
            match lookup : next.word id with
            | some ⟨1, condition⟩ =>
              match a : edge callTokenTypes f next yes, b : edge callTokenTypes f next no with
              | some yesScope, some noScope => do
                let first ← callingCutDiagram p f tokens type cutpoints budget false yesScope yes.block
                let second ← callingCutDiagram p f tokens type cutpoints budget false noScope no.block
                return ⟨.branch condition first.val second.val,
                  calling_cut_choice p f tokens advance block.val next yesScope noScope id condition
                    yes no first.val second.val function found terminator lookup a b first.property second.property⟩
              | _, _ => none
            | _ => none
          )
      return ⟨tail.val, by
        simpa only [name] using
          calling_cut_span p f tokens advance block.val scope span.val tail.val span.property tail.property⟩
    else none

end EvoIR.Core.Symbolic
