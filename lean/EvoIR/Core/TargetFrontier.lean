import EvoIR.Core.CallingCutpoint

/-! Actual IR prefixes ending before a call or return. Ordinary instructions and CFG
edges are checked against their execution semantics. A zero-length prefix is
valid when the original entry already contains the call. Return leaves still
require independent result, token and actual return-step checking. -/
namespace EvoIR.Core.Symbolic

structure TargetCallSite (Γ : Var → Nat) where
  label : String
  pc : Nat
  scope : Scope Γ

def TargetCallSite.denote {Γ : Var → Nat} (σ : Subst Γ) (f : IR.Function)
    (owner nextFrame : Nat) (stack : List IR.ReturnFrame) (site : TargetCallSite Γ) : IR.State :=
  cutTargetState σ f site.label site.pc owner nextFrame site.scope stack

def TargetFrontierRuns {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (label : String) (pc : Nat) (scope : Scope Γ) (tree : CutTree Γ (TargetCallSite Γ)) : Prop :=
  ∀ σ owner nextFrame stack, ∃ last,
    IR.Steps p (cutTargetState σ f label pc owner nextFrame scope stack) last ∧
    match tree.eval σ with
    | .error fault => last.control = .halted (.error fault)
    | .ok site => last = site.denote σ f owner nextFrame stack

theorem target_frontier_instruction {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (b : IR.Block) (pc : Nat) (scope next : Scope Γ) (inst : IR.Instruction)
    (checks : List (Guard Γ)) (tree : CutTree Γ (TargetCallSite Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (atPc : b.instructions[pc]? = some inst)
    (accepted : instruction scope inst = some (checks, next))
    (tail : TargetFrontierRuns p f b.name (pc+1) next tree) :
    TargetFrontierRuns p f b.name pc scope (tree.prefix checks) := by
  intro σ owner nextFrame stack
  let start := cutTargetState σ f b.name pc owner nextFrame scope stack
  have execute := instruction_execution_sound σ callTokens scope next checks function block atPc
    rfl accepted (s := start) (a := ⟨f.name, b.name, pc, scope.denote σ callTokens, owner⟩)
  dsimp only [start, cutTargetState, targetState] at execute
  rw [CutTree.prefix_sound]
  cases checked : guards σ checks with
  | error fault =>
    refine ⟨{start with control := .halted (.error fault)}, .next (.execute ?_) (.refl _), rfl⟩
    simp only [IR.step, start, cutTargetState, targetState]
    rw [execute, checked]
    rfl
  | ok unitValue =>
    obtain ⟨last, steps, outcome⟩ := tail σ owner nextFrame stack
    refine ⟨last, .next (.execute ?_) steps, outcome⟩
    simp only [IR.step, cutTargetState, targetState]
    rw [execute, checked]
    rfl

theorem target_frontier_branch {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (b : IR.Block) (scope next : Scope Γ) (successor : IR.Edge)
    (tree : CutTree Γ (TargetCallSite Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (terminator : b.terminator = .branch successor)
    (binding : edge callTokenTypes f scope successor = some next)
    (tail : TargetFrontierRuns p f successor.block 0 next tree) :
    TargetFrontierRuns p f b.name b.instructions.length scope tree := by
  intro σ owner nextFrame stack
  have step := branch_execution_sound σ callTokens callTokenTypes (by intro id; rfl) p
    (cutTargetState σ f b.name b.instructions.length owner nextFrame scope stack)
    ⟨f.name, b.name, b.instructions.length, scope.denote σ callTokens, owner⟩
    f b scope next successor rfl function block rfl terminator rfl (by simp [checkEdge, binding])
  obtain ⟨last, path, outcome⟩ := tail σ owner nextFrame stack
  exact ⟨last, .next (.execute step) path, outcome⟩

theorem target_frontier_choice {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (b : IR.Block) (scope yesScope noScope : Scope Γ)
    (id : Nat) (condition : Term Γ 1) (yes no : IR.Edge)
    (a c : CutTree Γ (TargetCallSite Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (terminator : b.terminator = .conditional id yes no)
    (test : scope.word id = some ⟨1, condition⟩)
    (firstEdge : edge callTokenTypes f scope yes = some yesScope)
    (secondEdge : edge callTokenTypes f scope no = some noScope)
    (first : TargetFrontierRuns p f yes.block 0 yesScope a)
    (second : TargetFrontierRuns p f no.block 0 noScope c) :
    TargetFrontierRuns p f b.name b.instructions.length scope (.branch condition a c) := by
  intro σ owner nextFrame stack
  have step := conditional_execution_sound σ callTokens callTokenTypes (by intro id; rfl) p
    (cutTargetState σ f b.name b.instructions.length owner nextFrame scope stack)
    ⟨f.name, b.name, b.instructions.length, scope.denote σ callTokens, owner⟩
    f b scope yesScope noScope id condition yes no rfl function block rfl terminator rfl test
    (by simp [checkEdge, firstEdge]) (by simp [checkEdge, secondEdge])
  by_cases zero : (denoteTerm σ condition).toNat == 0
  · obtain ⟨last, path, outcome⟩ := second σ owner nextFrame stack
    have actual := step
    simp only [zero] at actual
    exact ⟨last, .next (.execute actual) path, by simpa [CutTree.eval, bne, zero] using outcome⟩
  · obtain ⟨last, path, outcome⟩ := first σ owner nextFrame stack
    have actual := step
    simp only [zero] at actual
    exact ⟨last, .next (.execute actual) path, by simpa [CutTree.eval, bne, zero] using outcome⟩

def targetCallFrontier {Γ : Var → Nat} (p : IR.Program) (f : IR.Function) :
    (budget : Nat) → (label : String) → (pc : Nat) → (scope : Scope Γ) →
    Option {tree // TargetFrontierRuns p f label pc scope tree}
  | 0, _, _, _ => none
  | budget+1, label, pc, scope => do
    if function : p.functions.find? (·.name == f.name) = some f then
      let b : {b // f.blocks.find? (·.name == label) = some b} ←
        match found : f.blocks.find? (·.name == label) with
        | none => none
        | some b => some ⟨b, found⟩
      have name : b.val.name = label := by simpa using List.find?_some b.property
      have block : f.blocks.find? (·.name == b.val.name) = some b.val := by
        simpa only [name] using b.property
      match atPc : b.val.instructions[pc]? with
      | some inst => match op : inst.op with
        | .call _ => return ⟨.leaf ⟨label, pc, scope⟩, by
            intro σ owner nextFrame stack
            exact ⟨_, .refl _, rfl⟩⟩
        | .uninitialized (.bits width) => do
          if args : inst.operands = [] then
            let next : {next // scope.appendSlots inst.results [.opaque (width+2)] = some next} ←
              match bound : scope.appendSlots inst.results [.opaque (width+2)] with
              | none => none
              | some next => some ⟨next, bound⟩
            let tail ← targetCallFrontier p f budget label (pc+1) next.val
            return ⟨tail.val, by
              intro σ owner nextFrame stack
              obtain ⟨last, steps, outcome⟩ := tail.property σ owner nextFrame stack
              refine ⟨last, .next ?_ steps, outcome⟩
              simpa only [callState, cutTargetState, targetState, name] using
                call_uninitialized_step p f b.val pc width scope next.val inst
                  function block atPc op args next.property σ owner nextFrame stack⟩
          else none
        | _ => do
          let code : {pair // instruction scope inst = some pair} ←
            match accepted : instruction scope inst with
            | none => none
            | some pair => some ⟨pair, accepted⟩
          let tail ← targetCallFrontier p f budget label (pc+1) code.val.2
          return ⟨tail.val.prefix code.val.1, by
            simpa only [name] using target_frontier_instruction p f b.val pc scope code.val.2 inst
              code.val.1 tail.val function block atPc code.property (by simpa only [name] using tail.property)⟩
      | none =>
        if atEnd : pc = b.val.instructions.length then do
          match terminator : b.val.terminator with
          | .branch successor =>
            match binding : edge callTokenTypes f scope successor with
            | none => none
            | some next => do
              let tail ← targetCallFrontier p f budget successor.block 0 next
              return ⟨tail.val, by
                simpa only [name, atEnd] using
                  target_frontier_branch p f b.val scope next successor tail.val function block
                    terminator binding tail.property⟩
          | .conditional id yes no =>
            match lookup : scope.word id with
            | some ⟨1, condition⟩ =>
              match a : edge callTokenTypes f scope yes, b' : edge callTokenTypes f scope no with
              | some yesScope, some noScope => do
                let first ← targetCallFrontier p f budget yes.block 0 yesScope
                let second ← targetCallFrontier p f budget no.block 0 noScope
                return ⟨.branch condition first.val second.val, by
                  simpa only [name, atEnd] using
                    target_frontier_choice p f b.val scope yesScope noScope id condition yes no first.val second.val
                      function block terminator lookup a b' first.property second.property⟩
              | _, _ => none
            | _ => none
          | .return _ => return ⟨.leaf ⟨label, pc, scope⟩, by
              intro σ owner nextFrame stack
              exact ⟨_, .refl _, rfl⟩⟩
        else none
    else none

end EvoIR.Core.Symbolic
