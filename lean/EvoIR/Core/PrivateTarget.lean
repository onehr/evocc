import EvoIR.Core.PrivateTargetTypes

/-! Actual target CFG spans with private storage. The inspection budget counts
syntax between boundaries; successor execution is separately quantified. -/
namespace EvoIR.Core.Symbolic

def privateIsCall : IR.Operation → Bool
  | .call _ => true
  | _ => false

theorem private_not_call (op : IR.Operation) (checked : privateIsCall op = false) :
    ∀ name, op ≠ .call name := by
  cases op <;> simp_all [privateIsCall]

theorem private_cut_boundary {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens : Bool) (label : String) (pc : Nat) (config : PrivateConfig Γ) :
    PrivateCutRuns p f tokens false label pc config (.leaf (.next label pc config)) := by
  intro σ values incoming owner frame stack related
  exact ⟨_, .refl _, by simp, values, frame, related, rfl⟩

theorem private_cut_instruction {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens advance : Bool) (block : IR.Block) (pc : Nat) (config : PrivateConfig Γ)
    (inst : IR.Instruction) (checked : CheckedPrivateInstruction config inst)
    (tree : CutTree Γ (PrivateExit Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (found : f.blocks.find? (·.name == block.name) = some block)
    (atPc : block.instructions[pc]? = some inst)
    (noncall : ∀ name, inst.op ≠ .call name)
    (tail : PrivateCutRuns p f tokens false block.name (pc+1) checked.next tree) :
    PrivateCutRuns p f tokens advance block.name pc config (tree.prefix checked.checks) := by
  intro σ values incoming owner frame stack related
  obtain ⟨nextValues, code, realized⟩ := checked.sound σ values incoming owner frame related
  let start := privateState σ values f block.name pc owner frame config stack
  have execute := execute_instruction (p := p) (s := start)
    (a := ⟨f.name, block.name, pc, config.scope.denote σ (privateToken values), owner⟩)
    function found atPc noncall
  dsimp only [start, privateState] at execute
  rw [CutTree.prefix_sound]
  cases guardsResult : guards σ checked.checks with
  | error fault =>
    have actual : IR.step p start = some {start with control := .halted (.error fault)} := by
      simp only [IR.step, start, privateState]
      rw [execute, code, guardsResult]
      rfl
    exact ⟨_, .next (.execute actual) (.refl _), fun _ => .single actual, rfl⟩
  | ok unit =>
    have accepted : guards σ checked.checks = .ok () := by cases unit; exact guardsResult
    have actual : IR.step p start =
        some (privateState σ nextValues f block.name (pc+1) owner frame checked.next stack) := by
      simp only [IR.step, start, privateState]
      rw [execute, code, accepted]
      rfl
    obtain ⟨last, path, _, outcome⟩ := tail σ nextValues incoming owner frame stack (realized accepted)
    exact ⟨last, .next (.execute actual) path,
      fun _ => path.toReaches.after_step actual, outcome⟩

theorem private_cut_branch {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens advance : Bool) (block : IR.Block) (config : PrivateConfig Γ)
    (scope : Scope Γ) (successor : IR.Edge) (tree : CutTree Γ (PrivateExit Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (found : f.blocks.find? (·.name == block.name) = some block)
    (terminator : block.terminator = .branch successor)
    (binding : edge (privateTokenType config.tokens) f config.scope successor = some scope)
    (shape : ({config with scope} : PrivateConfig Γ).WellShaped = true)
    (tail : PrivateCutRuns p f tokens false successor.block 0 {config with scope} tree) :
    PrivateCutRuns p f tokens advance block.name block.instructions.length config tree := by
  intro σ values incoming owner frame stack related
  have actual := branch_execution_sound σ (privateToken values) (privateTokenType config.tokens)
    (private_token_typed values config.tokens related.2.1) p
    (privateState σ values f block.name block.instructions.length owner frame config stack)
    ⟨f.name, block.name, block.instructions.length, config.scope.denote σ (privateToken values), owner⟩
    f block config.scope scope successor rfl function found rfl terminator rfl
    (by simp [checkEdge, binding])
  obtain ⟨last, path, _, outcome⟩ := tail σ values incoming owner frame stack
    (private_rescope_realizes σ values incoming owner frame config scope related shape)
  exact ⟨last, .next (.execute actual) path, fun _ => path.toReaches.after_step actual, outcome⟩

theorem private_cut_choice {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens advance : Bool) (block : IR.Block) (config : PrivateConfig Γ)
    (yesScope noScope : Scope Γ) (id : Nat) (condition : Term Γ 1) (yes no : IR.Edge)
    (a b : CutTree Γ (PrivateExit Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (found : f.blocks.find? (·.name == block.name) = some block)
    (terminator : block.terminator = .conditional id yes no)
    (test : config.scope.word id = some ⟨1, condition⟩)
    (firstEdge : edge (privateTokenType config.tokens) f config.scope yes = some yesScope)
    (secondEdge : edge (privateTokenType config.tokens) f config.scope no = some noScope)
    (yesShape : ({config with scope := yesScope} : PrivateConfig Γ).WellShaped = true)
    (noShape : ({config with scope := noScope} : PrivateConfig Γ).WellShaped = true)
    (first : PrivateCutRuns p f tokens false yes.block 0 {config with scope := yesScope} a)
    (second : PrivateCutRuns p f tokens false no.block 0 {config with scope := noScope} b) :
    PrivateCutRuns p f tokens advance block.name block.instructions.length config
      (.branch condition a b) := by
  intro σ values incoming owner frame stack related
  have actual := conditional_execution_sound σ (privateToken values) (privateTokenType config.tokens)
    (private_token_typed values config.tokens related.2.1) p
    (privateState σ values f block.name block.instructions.length owner frame config stack)
    ⟨f.name, block.name, block.instructions.length, config.scope.denote σ (privateToken values), owner⟩
    f block config.scope yesScope noScope id condition yes no rfl function found rfl terminator rfl test
    (by simp [checkEdge, firstEdge]) (by simp [checkEdge, secondEdge])
  by_cases zero : (denoteTerm σ condition).toNat == 0
  · obtain ⟨last, path, _, outcome⟩ := second σ values incoming owner frame stack
      (private_rescope_realizes σ values incoming owner frame config noScope related noShape)
    simp only [zero] at actual
    exact ⟨last, .next (.execute actual) path, fun _ => path.toReaches.after_step actual,
      by simpa [CutTree.eval, bne, zero] using outcome⟩
  · obtain ⟨last, path, _, outcome⟩ := first σ values incoming owner frame stack
      (private_rescope_realizes σ values incoming owner frame config yesScope related yesShape)
    simp only [zero] at actual
    exact ⟨last, .next (.execute actual) path, fun _ => path.toReaches.after_step actual,
      by simpa [CutTree.eval, bne, zero] using outcome⟩

end EvoIR.Core.Symbolic
