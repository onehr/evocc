import EvoIR.IRPoisonEdges

/-! The only changing instruction rule evaluates actual poison and actual
canonical zero separately. Equal types plus a checked non-demanded result
justify actual appendResults; operand-consuming poison is never accepted. -/
namespace EvoIR.Core.IR.Poison

theorem rewrite_cases (needed : List Nat) (inst : Instruction) :
    rewriteInstruction needed inst = inst ∨
    ∃ width result,
      inst = ⟨[result], .uninitialized (.bits width), []⟩ ∧
      0 < width ∧ result.id ∉ needed ∧
      rewriteInstruction needed inst = ⟨[result], .constant width 0, []⟩ := by
  unfold rewriteInstruction
  split
  · rename_i width result shapeOp shapeResults shapeOperands
    split
    · rename_i changed
      right
      have data : 0 < width ∧ result.id ∉ needed := by
        simpa [List.contains_iff_mem] using changed
      exact ⟨width, result, by cases inst; simp_all, data.1, data.2, by simp_all⟩
    · left; rfl
  · left; rfl

def evalAppend (owner : Nat) (inst : Instruction) (values : Bindings) : Except Source.Fault Bindings := do
  let args ← inst.operands.mapM (IR.lookup values)
  let results ← evalOperation owner inst.op args
  appendResults values inst.results results

theorem evalAppend_related (needed : List Nat) (owner : Nat) (inst : Instruction)
    (source target : Bindings) (related : BanksRelated needed source target)
    (demanded : ∀ id ∈ inst.operands, id ∈ needed) :
    ExceptRelated (BanksRelated needed) (evalAppend owner inst source)
      (evalAppend owner (rewriteInstruction needed inst) target) := by
  rcases rewrite_cases needed inst with same | ⟨width, result, rfl, positive, dead, changed⟩
  · rw [same]
    unfold evalAppend
    rw [related.reads inst.operands demanded]
    apply except_bind_related (relation := Eq)
    · cases inst.operands.mapM (IR.lookup target) <;> simp [ExceptRelated]
    · intro before after equal
      subst after
      apply except_bind_related (relation := Eq)
      · cases evalOperation owner inst.op before <;> simp [ExceptRelated]
      · intro beforeResults afterResults equal
        subst afterResults
        exact related.append_same inst.results beforeResults
  · rw [changed]
    have nonzero : (width == 0) = false := by simp [Nat.ne_of_gt positive]
    have powPositive : 0 < 2 ^ width := Nat.two_pow_pos width
    simp only [evalAppend, List.mapM_nil, Bind.bind, Except.bind, Pure.pure, Except.pure,
      evalOperation, nonzero, Nat.not_le_of_gt powPositive, decide_false,
      Bool.or_self, Bool.false_eq_true, ↓reduceIte]
    apply related.append_related
    exact .cons ⟨rfl, fun demanded => False.elim (dead demanded)⟩ .nil

theorem rewrite_call (needed : List Nat) (inst : Instruction) (name : String)
    (call : inst.op = .call name) : rewriteInstruction needed inst = inst := by
  simp [rewriteInstruction, call]

theorem rewrite_notCall (needed : List Nat) (inst : Instruction)
    (notCall : ∀ name, inst.op ≠ .call name) :
    ∀ name, (rewriteInstruction needed inst).op ≠ .call name := by
  rcases rewrite_cases needed inst with same | ⟨width, result, shape, positive, dead, changed⟩
  · simpa only [same] using notCall
  · simp only [changed]; intro name; intro impossible; cases impossible

end EvoIR.Core.IR.Poison
