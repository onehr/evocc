import EvoIR.Core.PrivateTargetTypes

/-! Entry and return boundaries for arbitrary caller snapshots. A return
performs the real endFrame operation and the real caller result binding. The
empty top-level snapshot is one instance, not the function summary. -/
namespace EvoIR.Core.Symbolic

def privateEntryConfig {Γ : Var → Nat} (scope : Scope Γ) : PrivateConfig Γ :=
  ⟨scope, [.memory, .world], 0, 1, []⟩

theorem private_entry_realizes {Γ : Var → Nat} (σ : Subst Γ) (scope : Scope Γ)
    (incoming : IR.Memory) (owner frame : Nat)
    (bounded : scope.inBank 2 = true) (safe : incoming.WellAllocated)
    (fresh : incoming.BeforeFrame owner) (future : owner < frame) :
    (privateEntryConfig scope).Realizes σ [.memory incoming, .world] incoming owner frame := by
  have shaped : (privateEntryConfig scope).WellShaped = true := by
    simp [privateEntryConfig, PrivateConfig.WellShaped, privateTokenType, bounded]
  refine ⟨shaped, rfl, rfl, safe, incoming, rfl, safe, ?_, future,
    incoming.private_entry owner fresh, ?_, ?_⟩
  · intro object member
    exact Nat.lt_trans (fresh object member) future
  · intro a member
    cases member
  · intro cell member
    cases member

theorem private_return_step {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (incoming : IR.Memory) (p : IR.Program) (f : IR.Function) (b : IR.Block)
    (tokens : Bool) (owner frame : Nat) (stack : List IR.ReturnFrame)
    (config : PrivateConfig Γ) (ids : List Nat) (word : CWord Γ) (memory : IR.Memory)
    (related : config.Realizes σ values incoming owner frame)
    (current : config.memoryValue values = some memory)
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (terminator : b.terminator = .return ids)
    (lookup : ids.mapM (fun id => config.scope.lookup id) = some (privateResults tokens config word))
    (returns : f.returns = cutReturnTypes tokens word.type) :
    IR.step p (privateState σ values f b.name b.instructions.length owner frame config stack) =
      some (cutResume (privateReturnValues tokens (word.denote σ) memory owner) frame stack) := by
  have actualValues := slots_lookup_sound σ (privateToken values) config.scope ids
    (privateResults tokens config word) lookup
  have world := related.2.2.1
  have memoryToken := config.current_token current
  cases tokens <;> cases stack <;>
    simp [IR.step, IR.execute, privateState, function, block, terminator, actualValues,
      returns, cutReturnTypes, privateResults, privateReturnValues, Slot.denote,
      memoryToken, world, IR.Value.type, Word.denote, CWord.word, CWord.denote,
      sourceValueIR, cutResume, Bind.bind, Except.bind, pure, Except.pure]
  all_goals split <;> split at * <;> simp_all

theorem private_cut_return {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens advance : Bool) (b : IR.Block) (config : PrivateConfig Γ)
    (ids : List Nat) (word : CWord Γ)
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (terminator : b.terminator = .return ids)
    (lookup : ids.mapM (fun id => config.scope.lookup id) = some (privateResults tokens config word))
    (returns : f.returns = cutReturnTypes tokens word.type) :
    PrivateCutRuns p f tokens advance b.name b.instructions.length config
      (.leaf (.returned word config)) := by
  intro σ values incoming owner frame stack related
  obtain ⟨memory, current, _⟩ := related.2.2.2.2
  have step := private_return_step σ values incoming p f b tokens owner frame stack
    config ids word memory related current function block terminator lookup returns
  exact ⟨_, .next (.execute step) (.refl _), fun _ => .single step,
    values, memory, frame, related, current, rfl⟩

/-- The returned snapshot retains every caller object exactly. Fresh objects
remain in history as dead objects; their IDs are never reused after a call. -/
theorem private_return_storage {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (incoming memory : IR.Memory) (owner frame : Nat) (config : PrivateConfig Γ)
    (related : config.Realizes σ values incoming owner frame)
    (current : config.memoryValue values = some memory) :
    incoming.ClosedExtension (memory.endFrame owner) ∧
      (memory.endFrame owner).WellAllocated ∧ (memory.endFrame owner).BeforeFrame frame ∧
      ∀ object ∈ (memory.endFrame owner).objects, object ∉ incoming.objects → owner ≤ object.owner := by
  obtain ⟨actual, actualMemory, safe, frameBound, ownerBound, owned, _⟩ := related.2.2.2.2
  have same : actual = memory := Option.some.inj (actualMemory.symm.trans current)
  subst actual
  exact ⟨IR.Memory.private_return owned, memory.end_frame_safe owner safe,
    memory.end_frame_bound owner frame frameBound, IR.Memory.return_new_owners owned⟩

end EvoIR.Core.Symbolic
