import EvoIR.Core.FlowTree

/-! Source-machine proofs for declaration, sequencing, branching and scope
continuations. All steps use the original exported-source semantics. -/
namespace EvoIR.Core.Symbolic

theorem discard_frame {Γ : Var → Nat} (locals : LocalStore Γ) (word : CWord Γ) :
    FrameDiagram locals .discard word (.value locals .normal) := by
  intro σ p fid k
  exact .next rfl (.refl _)

theorem return_frame {Γ : Var → Nat} (locals : LocalStore Γ) (word : CWord Γ)
    (destination : Source.QualifiedType) :
    FrameDiagram locals (.return destination) word
      (.value locals (.returned (word.cast destination.type))) := by
  intro σ p fid k
  exact .next (by simp [Source.Step, Source.step, Source.resumeValue,
    FlowTree.denote, SymbolicFlow.denote, cast_sound]) (.refl _)

theorem initialize_frame {Γ : Var → Nat} (locals updated : LocalStore Γ)
    (word : CWord Γ) (id : Nat) (destination : Source.QualifiedType)
    (written : locals.write id destination (word.cast destination.type) true = some updated) :
    FrameDiagram locals (.initialize id destination) word (.value updated .normal) := by
  intro σ p fid k
  have write := local_write_sound σ locals updated id destination
    (word.cast destination.type) true written
  rw [cast_sound] at write
  exact .next (by simp [Source.Step, Source.step, Source.resumeValue, write,
    FlowTree.denote, SymbolicFlow.denote]) (.refl _)

theorem branch_frame {Γ : Var → Nat} (locals : LocalStore Γ) (word : CWord Γ)
    (yes : Source.Stmt) (no : Option Source.Stmt) (a b : FlowTree Γ)
    (first : FlowDiagram locals (.statement yes) a)
    (second : FlowDiagram locals (.statement (no.getD .null)) b) :
    FrameDiagram locals (.branch yes no) word (.branch word.truthTerm a b) := by
  intro σ p fid k
  simp only [FlowTree.denote, truth_term_sound]
  cases truth : (word.denote σ).truth with
  | false =>
    simp only [Bool.false_eq_true, ↓reduceIte]
    have path := second σ p fid k
    cases result : (b.denote σ).2 with
    | error fault =>
      simp only [result] at path ⊢
      obtain ⟨failedK, tail⟩ := path
      exact ⟨failedK, .next (by simp [Source.Step, Source.step, Source.resumeValue, truth]) tail⟩
    | ok flow =>
      simp only [result] at path ⊢
      exact .next (by simp [Source.Step, Source.step, Source.resumeValue, truth]) path
  | true =>
    simp only [↓reduceIte]
    have path := first σ p fid k
    cases result : (a.denote σ).2 with
    | error fault =>
      simp only [result] at path ⊢
      obtain ⟨failedK, tail⟩ := path
      exact ⟨failedK, .next (by simp [Source.Step, Source.step, Source.resumeValue, truth]) tail⟩
    | ok flow =>
      simp only [result] at path ⊢
      exact .next (by simp [Source.Step, Source.step, Source.resumeValue, truth]) path

def itemControl : Source.BlockItem → Source.Control
  | .statement stmt => .statement stmt
  | .declaration object => .object object

theorem sequence_diagram {Γ : Var → Nat} (locals : LocalStore Γ)
    (item : Source.BlockItem) (rest : List Source.BlockItem) (before after : FlowTree Γ)
    (first : FlowDiagram locals (itemControl item) before)
    (next : NormalBind (fun ls tree => FlowDiagram ls (.items rest) tree) before after) :
    FlowDiagram locals (.items (item :: rest)) after := by
  intro σ p fid k
  have start : Source.Step p ⟨.items (item :: rest), locals.denote σ, k, fid⟩
      ⟨itemControl item, locals.denote σ, .sequence rest :: k, fid⟩ := by
    cases item <;> rfl
  have path := first σ p fid (.sequence rest :: k)
  rcases next.outcome σ with ⟨same, notNormal⟩ | ⟨ls, tree, tail, success, output⟩
  · rw [← same]
    cases result : (before.denote σ).2 with
    | error fault =>
      simp only [result] at path ⊢
      obtain ⟨failedK, steps⟩ := path
      exact ⟨failedK, .next start steps⟩
    | ok flow =>
      simp only [result] at path ⊢
      have notNormalFlow : flow ≠ .normal := by
        intro sameFlow
        apply notNormal (before.denote σ).1
        exact Prod.ext rfl (result.trans (congrArg Except.ok sameFlow))
      exact .next start (path.trans (.next (by
        cases flow <;> simp_all [Source.Step, Source.step, Source.resumeFlow]) (.refl _)))
  · rw [output]
    simp only [success] at path
    have prefixPath := (Source.Steps.next start path).trans (.next rfl (.refl _))
    have finish := tail σ p fid k
    cases result : (tree.denote σ).2 with
    | error fault =>
      simp only [result] at finish ⊢
      obtain ⟨failedK, steps⟩ := finish
      exact ⟨failedK, prefixPath.trans steps⟩
    | ok flow =>
      simp only [result] at finish ⊢
      exact prefixPath.trans finish

def ScopeExit {Γ : Var → Nat} (depth : Nat) (locals : LocalStore Γ)
    (flow : SymbolicFlow Γ) (tree : FlowTree Γ) : Prop :=
  ∃ updated, locals.exitScope depth = some updated ∧ tree = .value updated flow

def scopeExit {Γ : Var → Nat} (depth : Nat) (locals : LocalStore Γ)
    (flow : SymbolicFlow Γ) : Option {tree // ScopeExit depth locals flow tree} := do
  match accepted : locals.exitScope depth with
  | none => none
  | some updated => some ⟨.value updated flow, updated, accepted, rfl⟩

theorem compound_diagram {Γ : Var → Nat} (locals : LocalStore Γ)
    (items : List Source.BlockItem) (before after : FlowTree Γ)
    (body : FlowDiagram locals (.items items) before)
    (scope : FlowMap (ScopeExit locals.length) before after) :
    FlowDiagram locals (.statement (.compound items)) after := by
  intro σ p fid k
  have length : (locals.denote σ).length = locals.length := by simp [LocalStore.denote]
  have start : Source.Step p ⟨.statement (.compound items), locals.denote σ, k, fid⟩
      ⟨.items items, locals.denote σ, .scope locals.length :: k, fid⟩ := by
    simp [Source.Step, Source.step, Source.enterStatement, length]
  have path := body σ p fid (.scope locals.length :: k)
  rcases scope.outcome σ with ⟨ls, fault, failed, output⟩ |
    ⟨ls, flow, tree, ⟨updated, exit, treeEq⟩, success, output⟩
  · rw [output]
    simp only [failed] at path
    obtain ⟨failedK, steps⟩ := path
    exact ⟨failedK, .next start steps⟩
  · rw [output, treeEq]
    simp only [success] at path
    have resume := local_scope_exit_sound σ ls updated locals.length exit
      ⟨.flow (flow.denote σ), ls.denote σ, k, fid⟩ rfl (flow.denote σ)
    exact .next start (path.trans (.next (congrArg some resume) (.refl _)))

theorem initialized_object_diagram {Γ : Var → Nat} (locals fresh : LocalStore Γ)
    (object : Source.Object) (expr : Source.Expr) (before : StateTree Γ) (after : FlowTree Γ)
    (initializer : object.initializer = some (object.type, expr))
    (declared : locals.declare object.symbol object.type = some fresh)
    (value : StateDiagram fresh expr before)
    (continued : ExpressionFlowBind
      (fun ls w t => FrameDiagram ls (.initialize object.symbol object.type) w t) before after) :
    FlowDiagram locals (.object object) after := by
  intro σ p fid k
  have start : Source.Step p ⟨.object object, locals.denote σ, k, fid⟩
      ⟨.expression expr, fresh.denote σ, .initialize object.symbol object.type :: k, fid⟩ := by
    unfold LocalStore.declare at declared
    split at declared
    · contradiction
    · rename_i unique
      cases Option.some.inj declared
      have duplicate : (locals.denote σ).any (·.symbol == object.symbol) = false := by
        simpa [LocalStore.denote, List.any_map, Function.comp_def, LocalSlot.denote]
          using Bool.eq_false_iff.mpr unique
      simp only [Source.Step, Source.step, Source.enterObject, duplicate,
        Bool.false_eq_true, ↓reduceIte, initializer, bne_self_eq_false]
      rfl
  have evaluated := value σ p fid
  rcases continued.outcome σ with ⟨ls, fault, failed, output⟩ |
    ⟨ls, word, tree, resume, success, output⟩
  · rw [output]
    simp only [Source.EvaluatesState, failed] at evaluated
    obtain ⟨failedK, steps⟩ := evaluated (.initialize object.symbol object.type :: k)
    exact ⟨failedK, .next start steps⟩
  · rw [output]
    simp only [Source.EvaluatesState, success] at evaluated
    have path := Source.Steps.next start (evaluated (.initialize object.symbol object.type :: k))
    have tail := resume σ p fid k
    cases result : (tree.denote σ).2 with
    | error fault =>
      simp only [result] at tail ⊢
      obtain ⟨failedK, steps⟩ := tail
      exact ⟨failedK, path.trans steps⟩
    | ok flow =>
      simp only [result] at tail ⊢
      exact path.trans tail

end EvoIR.Core.Symbolic
