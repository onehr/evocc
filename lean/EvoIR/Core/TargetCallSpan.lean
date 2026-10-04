import EvoIR.Core.CallTargetDiagram
import EvoIR.Core.CutTree

/-! Finite instruction spans with actual interprocedural execution. Successful
calls restore the caller stack and owner but retain the advanced frame counter.
The endpoint is the actual block terminator, not a synthetic return. -/
namespace EvoIR.Core.Symbolic

def ValueTree.cutTree {Γ : Var → Nat} : ValueTree Γ → CutTree Γ (CWord Γ)
  | .value word => .leaf word
  | .checked guard tail => .checked guard tail.cutTree
  | .branch condition yes no => .branch condition yes.cutTree no.cutTree

theorem ValueTree.cut_eval {Γ : Var → Nat} (σ : Subst Γ) (tree : ValueTree Γ) :
    (tree.cutTree.eval σ).map (CWord.denote σ) = tree.denote σ := by
  induction tree with
  | value word => rfl
  | checked guard tail ih =>
    simp only [cutTree, CutTree.eval, ValueTree.denote]
    cases guards σ [guard] with
    | error fault => rfl
    | ok u => exact ih
  | branch condition yes no first second =>
    simp only [cutTree, CutTree.eval, ValueTree.denote]
    split <;> assumption

def TargetInstructionSpan {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (b : IR.Block) (pc : Nat) (scope : Scope Γ) (tree : CutTree Γ (Scope Γ)) : Prop :=
  ∀ σ owner nextFrame stack, ∃ last,
    IR.Steps p (callState σ f b pc owner nextFrame scope stack) last ∧
    match tree.eval σ with
    | .error fault => last.control = .halted (.error fault)
    | .ok next => ∃ afterCalls,
        last = callState σ f b b.instructions.length owner afterCalls next stack

theorem target_span_instruction {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (b : IR.Block) (pc : Nat) (scope next : Scope Γ) (inst : IR.Instruction)
    (checks : List (Guard Γ)) (tree : CutTree Γ (Scope Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (atPc : b.instructions[pc]? = some inst)
    (accepted : instruction scope inst = some (checks, next))
    (tail : TargetInstructionSpan p f b (pc+1) next tree) :
    TargetInstructionSpan p f b pc scope (tree.prefix checks) := by
  intro σ owner nextFrame stack
  let start := callState σ f b pc owner nextFrame scope stack
  have execute := instruction_execution_sound σ callTokens scope next checks function block atPc
    rfl accepted (s := start) (a := ⟨f.name, b.name, pc, scope.denote σ callTokens, owner⟩)
  dsimp only [start, callState] at execute
  rw [CutTree.prefix_sound]
  cases checked : guards σ checks with
  | error fault =>
    refine ⟨{start with control := .halted (.error fault)}, .next (.execute ?_) (.refl _), rfl⟩
    simp only [IR.step, start, callState]
    rw [execute, checked]
    rfl
  | ok unitValue =>
    obtain ⟨last, steps, outcome⟩ := tail σ owner nextFrame stack
    refine ⟨last, .next (.execute ?_) steps, outcome⟩
    simp only [IR.step, callState]
    rw [execute, checked]
    rfl

theorem target_span_call {Γ : Var → Nat} (p : IR.Program) (f callee : IR.Function)
    (b : IR.Block) (pc : Nat) (scope : Scope Γ) (inst : IR.Instruction)
    (args : List (Slot Γ)) (called : ValueTree Γ) (tree : CutTree Γ (Scope Γ))
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (atPc : b.instructions[pc]? = some inst)
    (call : inst.op = .call callee.name)
    (found : p.functions.find? (·.name == callee.name) = some callee)
    (operands : inst.operands.mapM (fun id => scope.lookup id) = some args)
    (results : inst.results.map IR.Binding.type = callee.returns)
    (body : CallFunctionDiagram p callee args called)
    (continued : CutBind (fun word out => ∃ next,
      scope.appendSlots inst.results (callResults word) = some next ∧
      TargetInstructionSpan p f b (pc+1) next out) called.cutTree tree) :
    TargetInstructionSpan p f b pc scope tree := by
  intro σ owner nextFrame stack
  let caller : IR.Activation := ⟨f.name, b.name, pc, scope.denote σ callTokens, owner⟩
  let frame : IR.ReturnFrame := ⟨{caller with pc := pc+1}, inst.results⟩
  have entered := IR.call_enters p (callState σ f b pc owner nextFrame scope stack) caller
    ⟨callee.name, body.block.name, 0, body.scope.denote σ callTokens, nextFrame⟩
    f callee b inst (args.map (Slot.denote σ callTokens)) rfl function block atPc call found
    (slots_lookup_sound σ callTokens scope inst.operands args operands) results (body.enters σ nextFrame)
  obtain ⟨last, path, outcome⟩ := body.body σ nextFrame (nextFrame+1) (frame :: stack)
  have prefixPath := IR.Steps.next entered path
  rcases continued.outcome σ with ⟨fault, checked, output⟩ |
    ⟨word, out, checked, ⟨next, binding, tail⟩, output⟩
  · have value : called.denote σ = .error fault := by
      rw [← called.cut_eval σ, checked]; rfl
    rw [output]
    exact ⟨last, prefixPath, by simpa only [value] using outcome⟩
  · have value : called.denote σ = .ok (word.denote σ) := by
      rw [← called.cut_eval σ, checked]; rfl
    rw [output]
    simp only [value] at outcome
    obtain ⟨finalBlock, values, afterCalls, ids, finalFound, state, terminator, returned⟩ := outcome
    subst last
    have types : ((callResults word).map (Slot.denote σ callTokens)).map IR.Value.type =
        callee.returns := by
      rw [call_results_denote, body.returns]
      simp only [List.map_cons, List.map_nil, sourceValueIR, IR.Value.type, body.typed σ _ value]
    have endFrame : IR.returnedValues nextFrame
        ((callResults word).map (Slot.denote σ callTokens)) =
        (callResults word).map (Slot.denote σ callTokens) := rfl
    have bindings := append_slots_sound σ scope next inst.results (callResults word) binding
    have resumed := IR.call_returns p
      ⟨.running ⟨callee.name, finalBlock.name, finalBlock.instructions.length, values, nextFrame⟩,
        frame :: stack, afterCalls⟩
      ⟨callee.name, finalBlock.name, finalBlock.instructions.length, values, nextFrame⟩
      callee finalBlock ids ((callResults word).map (Slot.denote σ callTokens)) frame stack
      (next.denote σ callTokens) rfl found finalFound rfl terminator returned types rfl
      (by simpa only [endFrame] using bindings)
    obtain ⟨final, rest, resultOutcome⟩ := tail σ owner afterCalls stack
    exact ⟨final, prefixPath.trans (.next resumed rest), resultOutcome⟩

/-- The bound is syntactic work within one block and its finite callees. It
does not bound execution of the surrounding cyclic control-flow graph. -/
def targetInstructionSpan {Γ : Var → Nat} (p : IR.Program) (f : IR.Function) (b : IR.Block) :
    (fuel pc : Nat) → (scope : Scope Γ) →
    Option {tree // TargetInstructionSpan p f b pc scope tree}
  | 0, _, _ => none
  | fuel+1, pc, scope => do
    if function : p.functions.find? (·.name == f.name) = some f then
      if block : f.blocks.find? (·.name == b.name) = some b then
        match atPc : b.instructions[pc]? with
        | none =>
          if atEnd : pc = b.instructions.length then
            return ⟨.leaf scope, by
              intro σ owner nextFrame stack
              exact ⟨_, .refl _, nextFrame, by rw [atEnd]⟩⟩
          else none
        | some inst =>
          match code : instruction scope inst with
          | some (checks, next) => do
            let tail ← targetInstructionSpan p f b fuel (pc+1) next
            return ⟨tail.val.prefix checks, target_span_instruction p f b pc scope next inst
              checks tail.val function block atPc code tail.property⟩
          | none =>
            match call : inst.op with
            | .call name => do
              let callee : {callee // p.functions.find? (·.name == name) = some callee} ←
                match found : p.functions.find? (·.name == name) with
                | none => none
                | some callee => some ⟨callee, found⟩
              have nameEq : callee.val.name = name := by simpa using List.find?_some callee.property
              let args : {args // inst.operands.mapM (fun id => scope.lookup id) = some args} ←
                match operands : inst.operands.mapM (fun id => scope.lookup id) with
                | none => none
                | some args => some ⟨args, operands⟩
              let calleeType ← match callee.val.returns with
                | [.bits 32, .memory, .world] => some Source.IntType.uint
                | [.bits 64, .memory, .world] => some Source.IntType.ulong
                | _ => none
              if results : inst.results.map IR.Binding.type = callee.val.returns then
                let body ← callTargetFunction p fuel callee.val calleeType args.val
                let tail ← body.val.cutTree.bindChecked (next := fun word tree => ∃ next,
                    scope.appendSlots inst.results (callResults word) = some next ∧
                    TargetInstructionSpan p f b (pc+1) next tree) (fun word => do
                  let next : {next // scope.appendSlots inst.results (callResults word) = some next} ←
                    match bound : scope.appendSlots inst.results (callResults word) with
                    | none => none
                    | some next => some ⟨next, bound⟩
                  let rest ← targetInstructionSpan p f b fuel (pc+1) next.val
                  return ⟨rest.val, next.val, next.property, rest.property⟩)
                return ⟨tail.val, target_span_call p f callee.val b pc scope inst args.val body.val
                  tail.val function block atPc (by simpa only [nameEq] using call)
                  (by simpa only [nameEq] using callee.property) args.property results body.property tail.property⟩
              else none
            | _ => none
      else none
    else none

end EvoIR.Core.Symbolic
