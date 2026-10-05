import EvoIR.MachineTransportEdges

/-! Extraction of the already executable structural checker. These records
are kernel-derived facts about the two actual input tables, not a new
certificate language or caller-supplied semantic obligations. -/
namespace EvoIR.Machine.Transport

theorem checkBody_data (source target : Block) (plan : BlockPlan) (needed : List Nat)
    (final : BodyResult) (accepted : checkBody source target plan needed = some final) :
    source.id = plan.block ∧ source.id = target.id ∧ plan.prefixLength ≤ target.insts.length ∧
      bindingsValid source plan needed = true ∧
      checkBodyAux source.id needed target.insts 0 plan.prefixLength source.insts plan.alignment
        {source := sourceEntryOrigins source.args plan.bindings,
         target := targetEntryOrigins plan.bindings,
         cuts := [⟨0, plan.prefixLength, sourceEntryOrigins source.args plan.bindings,
           targetEntryOrigins plan.bindings⟩]} = some final := by
  by_cases bad : source.id ≠ plan.block ∨ source.id ≠ target.id ∨ plan.prefixLength > target.insts.length
  · simp only [checkBody, Bool.or_eq_true, decide_eq_true_eq] at accepted
    rcases bad with left | left | right <;> simp_all
  · have ids : source.id = plan.block ∧ source.id = target.id ∧ plan.prefixLength ≤ target.insts.length := by
      constructor
      · exact Classical.byContradiction (fun h => bad (Or.inl h))
      constructor
      · exact Classical.byContradiction (fun h => bad (Or.inr (Or.inl h)))
      · omega
    by_cases binding : bindingsValid source plan needed = true
    · have targetId : plan.block = target.id := ids.1.symm.trans ids.2.1
      exact ⟨ids.1, ids.2.1, ids.2.2, binding, by simpa [checkBody, ids.1, targetId, binding, Nat.not_lt.mpr ids.2.2] using accepted⟩
    · simp [checkBody, ids.1, ids.2.1, binding, Nat.not_lt.mpr ids.2.2] at accepted

theorem originalBlockValid_data (source target : Function) (plans : FunctionPlan) (block : Block)
    (accepted : originalBlockValid source target plans block = true) :
    ∃ candidate plan final,
      findBlock target block.id = some candidate ∧ findPlan plans block.id = some plan ∧
      checkBody block candidate plan plans.needed = some final ∧
      matchControls block.term candidate.term final.source final.target = true ∧
      (∀ pair ∈ block.term.edges.zip candidate.term.edges,
        edgeValid source target plans final pair.1 pair.2 = true) ∧
      (block.id = 0 → candidate.args = block.args ∧ plan.prefixLength = 0 ∧
        ∀ entry ∈ plan.bindings, entry.1 = entry.2.id) := by
  cases targetAt : findBlock target block.id with
  | none => simp [originalBlockValid, targetAt] at accepted
  | some candidate =>
    cases planAt : findPlan plans block.id with
    | none => simp [originalBlockValid, targetAt, planAt] at accepted
    | some plan =>
      have unfolded : (if block.id = 0 then
          if candidate.args ≠ block.args || plan.prefixLength ≠ 0 ||
              !plan.bindings.all (fun (id, reg) => decide (id = reg.id)) then false
          else originalBlockValid.finish source target plans block candidate plan
        else if !candidate.args.isEmpty then false
        else originalBlockValid.finish source target plans block candidate plan) = true := by
        simpa only [originalBlockValid, targetAt, planAt] using accepted
      have finish : originalBlockValid.finish source target plans block candidate plan = true := by
        by_cases entry : block.id = 0
        · by_cases good : candidate.args = block.args ∧ plan.prefixLength = 0 ∧
              plan.bindings.all (fun (id, reg) => decide (id = reg.id)) = true
          · simpa [if_pos entry, good.1, good.2.1, good.2.2] using unfolded
          · simp [if_pos entry, Bool.or_eq_true] at unfolded
            grind
        · by_cases empty : candidate.args = []
          · simpa [if_neg entry, empty] using unfolded
          · simp [if_neg entry, empty] at unfolded
      have entryShape : block.id = 0 → candidate.args = block.args ∧ plan.prefixLength = 0 ∧
          ∀ row ∈ plan.bindings, row.1 = row.2.id := by
        intro entry
        simp only [if_pos entry] at unfolded
        split at unfolded <;> try contradiction
        next good =>
          simp only [Bool.or_eq_true, decide_eq_true_eq, Bool.not_eq_true] at good
          have argsEq : candidate.args = block.args := Classical.byContradiction (fun h => good (Or.inl (Or.inl h)))
          have prefixBound : plan.prefixLength = 0 := Classical.byContradiction (fun h => good (Or.inl (Or.inr h)))
          have identity : plan.bindings.all (fun (id, reg) => decide (id = reg.id)) = true := by
            cases found : plan.bindings.all (fun (id, reg) => decide (id = reg.id)) <;> simp_all
          exact ⟨argsEq, prefixBound, by simpa only [List.all_eq_true, decide_eq_true_eq] using identity⟩
      cases checkedBody : checkBody block candidate plan plans.needed with
      | none => simp [originalBlockValid.finish, checkedBody] at finish
      | some final =>
        have rows : matchControls block.term candidate.term final.source final.target = true ∧
            block.term.edges.length = candidate.term.edges.length ∧
            ∀ pair ∈ block.term.edges.zip candidate.term.edges,
              edgeValid source target plans final pair.1 pair.2 = true := by
          simpa only [originalBlockValid.finish, checkedBody, Bool.and_eq_true,
            decide_eq_true_eq, List.all_eq_true, and_assoc] using finish
        exact ⟨candidate, plan, final, rfl, rfl, checkedBody, rows.1, rows.2.2, entryShape⟩

theorem checkStructure_name (source target : Function) (plans : FunctionPlan)
    (accepted : checkStructure source target plans = true) : source.name = target.name := by
  simp only [checkStructure, Bool.and_eq_true, decide_eq_true_eq] at accepted
  grind

theorem checkStructure_block (source target : Function) (plans : FunctionPlan)
    (accepted : checkStructure source target plans = true) (block : Block)
    (member : block ∈ source.blocks) :
    block.insts.all instructionShapeValid = true ∧ originalBlockValid source target plans block = true := by
  simp only [checkStructure, sourceWellFormed, Bool.and_eq_true, List.all_eq_true] at accepted
  have known : sourceBlockKnown block = true := by grind
  have original : originalBlockValid source target plans block = true := by grind
  simp only [sourceBlockKnown, Bool.and_eq_true] at known
  exact ⟨by grind, original⟩

structure ProgramPlan where
  functions : List (String × FunctionPlan)
  deriving DecidableEq, Repr

def functionPlan (plans : ProgramPlan) (name : String) : Option FunctionPlan := plans.functions.lookup name

def checkProgram (source target : Program) (plans : ProgramPlan) : Bool :=
  decide (source.functions.map Function.name = target.functions.map Function.name) &&
  source.functions.all fun function =>
    match target.functions.find? (·.name == function.name), functionPlan plans function.name with
    | some candidate, some plan => checkStructure function candidate plan
    | _, _ => false

theorem checkProgram_function (source target : Program) (plans : ProgramPlan)
    (accepted : checkProgram source target plans = true) (name : String) (function : Function)
    (found : source.functions.find? (·.name == name) = some function) :
    ∃ candidate plan, target.functions.find? (·.name == name) = some candidate ∧
      functionPlan plans name = some plan ∧ checkStructure function candidate plan = true := by
  have nameEq : function.name = name := by simpa using List.find?_some found
  have member := List.mem_of_find?_eq_some found
  simp only [checkProgram, Bool.and_eq_true, List.all_eq_true] at accepted
  have row := accepted.2 function member
  rw [nameEq] at row
  cases candidateAt : target.functions.find? (·.name == name) with
  | none => simp [candidateAt] at row
  | some candidate =>
    cases planAt : functionPlan plans name with
    | none => simp [candidateAt, planAt] at row
    | some plan =>
      exact ⟨candidate, plan, rfl, rfl, by simpa only [candidateAt, planAt] using row⟩

end EvoIR.Machine.Transport
