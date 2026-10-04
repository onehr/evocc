import EvoIR.Core.CallSourceDiagram
import EvoIR.Core.TargetCall

/-! Interprocedural target diagrams for scalar blocks carrying the actual
memory/world arguments. This component does not admit allocation or erase
state operands. Its initial memory is the core entry's empty private memory;
all admitted instructions and callees preserve it. -/
namespace EvoIR.Core.Symbolic

def callTokens (id : Nat) : IR.Value :=
  if id == 0 then .memory ⟨1, []⟩ else if id == 1 then .world else .poison (.bits (id-2))

theorem call_poison (width : Nat) : callTokens (width+2) = .poison (.bits width) := by
  have zero : (width+2 == 0) = false := by simp
  have one : (width+2 == 1) = false := by simp only [beq_eq_false_iff_ne]; omega
  simp [callTokens, zero, one]

def callTokenTypes (id : Nat) : IR.Ty := (callTokens id).type

def callResults {Γ : Var → Nat} (word : CWord Γ) : List (Slot Γ) :=
  [.word word.word, .opaque 0, .opaque 1]

theorem call_results_denote {Γ : Var → Nat} (σ : Subst Γ) (word : CWord Γ) :
    (callResults word).map (Slot.denote σ callTokens) =
      [sourceValueIR (word.denote σ), .memory ⟨1, []⟩, .world] := rfl

def Scope.appendSlots {Γ : Var → Nat} (scope : Scope Γ) (results : List IR.Binding)
    (args : List (Slot Γ)) : Option (Scope Γ) := do
  let fresh ← bindSlots callTokenTypes results args
  if fresh.any (fun (id, _) => (scope.lookup id).isSome) then none else some (fresh ++ scope)

theorem append_slots_sound {Γ : Var → Nat} (σ : Subst Γ) (scope next : Scope Γ)
    (results : List IR.Binding) (args : List (Slot Γ))
    (accepted : scope.appendSlots results args = some next) :
    IR.appendResults (scope.denote σ callTokens) results (args.map (Slot.denote σ callTokens)) =
      .ok (next.denote σ callTokens) := by
  unfold Scope.appendSlots at accepted
  simp only [bind, Option.bind] at accepted
  split at accepted
  · contradiction
  · rename_i fresh bound
    dsimp only at accepted
    split at accepted
    · contradiction
    · rename_i disjoint
      cases Option.some.inj accepted
      have binding := bind_slots_sound σ callTokens callTokenTypes (by intro id; rfl) _ _ _ bound
      have distinct : ((fresh.denote σ callTokens).any
          (fun (id, _) => ((scope.denote σ callTokens).lookup id).isSome)) = false := by
        change (fresh.map (fun x => (x.1, x.2.denote σ callTokens))).any _ = false
        rw [List.any_map]
        simpa only [Function.comp_def, lookup_denote, Option.isSome_map]
          using Bool.eq_false_iff.mpr disjoint
      dsimp only at distinct
      rw [IR.appendResults, binding]
      simp only [Bind.bind, Except.bind]
      rw [distinct]
      simp [Scope.denote, List.map_append, pure, Except.pure]

def callState {Γ : Var → Nat} (σ : Subst Γ) (f : IR.Function) (b : IR.Block)
    (pc owner nextFrame : Nat) (scope : Scope Γ) (stack : List IR.ReturnFrame) : IR.State :=
  ⟨.running ⟨f.name, b.name, pc, scope.denote σ callTokens, owner⟩, stack, nextFrame⟩

/-- The endpoint is immediately before the real return instruction. Its
operands are checked, and the actual return still performs frame finalization. -/
def CallTargetDiagram {Γ : Var → Nat} (p : IR.Program) (f : IR.Function) (b : IR.Block)
    (pc : Nat) (scope : Scope Γ) (result : ExpressionResult Γ) : Prop :=
  ∀ σ owner nextFrame stack, ∃ last, IR.Steps p (callState σ f b pc owner nextFrame scope stack) last ∧
    match guards σ result.1 with
    | .error fault => last.control = .halted (.error fault)
    | .ok _ => ∃ finalBlock values afterCalls ids,
        f.blocks.find? (·.name == finalBlock.name) = some finalBlock ∧
        last = ⟨.running ⟨f.name, finalBlock.name, finalBlock.instructions.length, values, owner⟩,
          stack, afterCalls⟩ ∧
        finalBlock.terminator = .return ids ∧ ids.mapM (IR.lookup values) =
          .ok ((callResults result.2).map (Slot.denote σ callTokens))

theorem call_target_return {Γ : Var → Nat} (p : IR.Program) (f : IR.Function) (b : IR.Block)
    (scope : Scope Γ) (ids : List Nat) (word : CWord Γ)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (returned : b.terminator = .return ids)
    (lookup : ids.mapM (fun id => scope.lookup id) = some (callResults word)) :
    CallTargetDiagram p f b b.instructions.length scope ([], word) := by
  intro σ owner nextFrame stack
  exact ⟨_, .refl _, b, scope.denote σ callTokens, nextFrame, ids, block, rfl, returned,
    slots_lookup_sound σ callTokens scope ids (callResults word) lookup⟩

theorem call_target_instruction {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (b : IR.Block) (pc : Nat) (scope next : Scope Γ) (inst : IR.Instruction)
    (checks : List (Guard Γ)) (result : ExpressionResult Γ)
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (atPc : b.instructions[pc]? = some inst)
    (accepted : instruction scope inst = some (checks, next))
    (tail : CallTargetDiagram p f b (pc+1) next result) :
    CallTargetDiagram p f b pc scope (checks ++ result.1, result.2) := by
  intro σ owner nextFrame stack
  let start := callState σ f b pc owner nextFrame scope stack
  have execute := instruction_execution_sound σ callTokens scope next checks function block atPc
    rfl accepted (s := start) (a := ⟨f.name, b.name, pc, scope.denote σ callTokens, owner⟩)
  dsimp only [start, callState] at execute
  rw [guards_append]
  cases checked : guards σ checks with
  | error fault =>
    refine ⟨{start with control := .halted (.error fault)}, .next (.execute ?_) (.refl _), rfl⟩
    simp only [IR.step, start, callState]
    rw [execute, checked]
    rfl
  | ok unitValue =>
    obtain ⟨last, steps, outcome⟩ := tail σ owner nextFrame stack
    refine ⟨last, .next (.execute ?_) steps, ?_⟩
    · simp only [IR.step, callState]
      rw [execute, checked]
      rfl
    · exact outcome

/-- Uninitialized data is transported as poison, never made into a symbolic
integer or admitted as a numeric operand. Unused lowered placeholders may cross
edges, but a read, arithmetic use, guard or return still refuses the diagram. -/
theorem call_target_uninitialized {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (b : IR.Block) (pc width : Nat) (scope next : Scope Γ) (inst : IR.Instruction)
    (result : ExpressionResult Γ)
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (atPc : b.instructions[pc]? = some inst)
    (op : inst.op = .uninitialized (.bits width)) (args : inst.operands = [])
    (binding : scope.appendSlots inst.results [.opaque (width+2)] = some next)
    (tail : CallTargetDiagram p f b (pc+1) next result) :
    CallTargetDiagram p f b pc scope result := by
  intro σ owner nextFrame stack
  have bindings := append_slots_sound σ scope next inst.results [.opaque (width+2)] binding
  simp only [List.map_cons, List.map_nil, Slot.denote, call_poison] at bindings
  have execute := execute_instruction (p := p)
    (s := callState σ f b pc owner nextFrame scope stack)
    (a := ⟨f.name, b.name, pc, scope.denote σ callTokens, owner⟩)
    function block atPc (by intro name; simp [op])
  dsimp only [callState] at execute
  obtain ⟨last, steps, outcome⟩ := tail σ owner nextFrame stack
  refine ⟨last, .next (.execute ?_) steps, outcome⟩
  simp only [IR.step, callState]
  rw [execute]
  simp [concreteInstruction, args, op, IR.evalOperation, bindings,
    Bind.bind, Except.bind, pure, Except.pure]

theorem call_target_branch {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (b nextBlock : IR.Block) (scope next : Scope Γ) (successor : IR.Edge)
    (result : ExpressionResult Γ)
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (terminator : b.terminator = .branch successor)
    (name : nextBlock.name = successor.block)
    (binding : edge callTokenTypes f scope successor = some next)
    (tail : CallTargetDiagram p f nextBlock 0 next result) :
    CallTargetDiagram p f b b.instructions.length scope result := by
  intro σ owner nextFrame stack
  have step := branch_execution_sound σ callTokens callTokenTypes (by intro id; rfl) p
    (callState σ f b b.instructions.length owner nextFrame scope stack)
    ⟨f.name, b.name, b.instructions.length, scope.denote σ callTokens, owner⟩
    f b scope next successor rfl function block rfl terminator rfl
    (by simp [checkEdge, binding])
  obtain ⟨last, path, outcome⟩ := tail σ owner nextFrame stack
  exact ⟨last, .next (.execute (by simpa only [callState, name] using step)) path, outcome⟩

structure CallFunctionDiagram {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (args : List (Slot Γ)) (result : ExpressionResult Γ) where
  block : IR.Block
  scope : Scope Γ
  head : f.blocks.head? = some block
  found : f.blocks.find? (·.name == block.name) = some block
  signature : f.parameters = args.map (Slot.type callTokenTypes)
  parameters : block.parameters.map IR.Binding.type = f.parameters
  binding : bindSlots callTokenTypes block.parameters args = some scope
  returns : f.returns = [.bits result.2.type.width, .memory, .world]
  body : CallTargetDiagram p f block 0 scope result

theorem CallFunctionDiagram.enters {Γ : Var → Nat} (σ : Subst Γ)
    {p : IR.Program} {f : IR.Function} {args : List (Slot Γ)} {result : ExpressionResult Γ}
    (diagram : CallFunctionDiagram p f args result) (owner : Nat) :
    IR.enter f (args.map (Slot.denote σ callTokens)) owner =
      .ok ⟨f.name, diagram.block.name, 0, diagram.scope.denote σ callTokens, owner⟩ := by
  have binding := bind_slots_sound σ callTokens callTokenTypes (by intro id; rfl)
    _ _ _ diagram.binding
  have types : (args.map (Slot.denote σ callTokens)).map IR.Value.type =
      args.map (Slot.type callTokenTypes) := by
    simp only [List.map_map]
    apply List.map_congr_left
    intro slot _
    exact slot_type_sound σ callTokens callTokenTypes (by intro id; rfl) slot
  simp [IR.enter, types, diagram.signature, diagram.head, diagram.parameters,
    binding, Bind.bind, Except.bind, pure, Except.pure]

theorem call_target_call {Γ : Var → Nat} (p : IR.Program) (f callee : IR.Function)
    (b : IR.Block) (pc : Nat) (scope next : Scope Γ) (inst : IR.Instruction)
    (args : List (Slot Γ)) (called result : ExpressionResult Γ)
    (function : p.functions.find? (·.name == f.name) = some f)
    (block : f.blocks.find? (·.name == b.name) = some b)
    (atPc : b.instructions[pc]? = some inst)
    (call : inst.op = .call callee.name)
    (found : p.functions.find? (·.name == callee.name) = some callee)
    (operands : inst.operands.mapM (fun id => scope.lookup id) = some args)
    (results : inst.results.map IR.Binding.type = callee.returns)
    (body : CallFunctionDiagram p callee args called)
    (binding : scope.appendSlots inst.results (callResults called.2) = some next)
    (tail : CallTargetDiagram p f b (pc+1) next result) :
    CallTargetDiagram p f b pc scope (called.1 ++ result.1, result.2) := by
  intro σ owner nextFrame stack
  let caller : IR.Activation := ⟨f.name, b.name, pc, scope.denote σ callTokens, owner⟩
  let frame : IR.ReturnFrame := ⟨{caller with pc := pc+1}, inst.results⟩
  have entered := IR.call_enters p (callState σ f b pc owner nextFrame scope stack) caller
    ⟨callee.name, body.block.name, 0, body.scope.denote σ callTokens, nextFrame⟩
    f callee b inst (args.map (Slot.denote σ callTokens)) rfl function block atPc call found
    (slots_lookup_sound σ callTokens scope inst.operands args operands) results (body.enters σ nextFrame)
  obtain ⟨last, path, outcome⟩ := body.body σ nextFrame (nextFrame+1) (frame :: stack)
  have prefixPath := IR.Steps.next entered path
  rw [guards_append]
  cases checked : guards σ called.1 with
  | error fault =>
    exact ⟨last, prefixPath, by simpa only [checked, Bind.bind, Except.bind] using outcome⟩
  | ok unitValue =>
    simp only [checked] at outcome
    obtain ⟨finalBlock, values, afterCalls, ids, finalFound, state, terminator, returned⟩ := outcome
    subst last
    have types : ((callResults called.2).map (Slot.denote σ callTokens)).map IR.Value.type =
        callee.returns := by
      rw [call_results_denote, body.returns]
      rfl
    have endFrame : IR.returnedValues nextFrame
        ((callResults called.2).map (Slot.denote σ callTokens)) =
        (callResults called.2).map (Slot.denote σ callTokens) := rfl
    have bindings := append_slots_sound σ scope next inst.results (callResults called.2) binding
    have resumed := IR.call_returns p
      ⟨.running ⟨callee.name, finalBlock.name, finalBlock.instructions.length, values, nextFrame⟩,
        frame :: stack, afterCalls⟩
      ⟨callee.name, finalBlock.name, finalBlock.instructions.length, values, nextFrame⟩
      callee finalBlock ids ((callResults called.2).map (Slot.denote σ callTokens)) frame stack
      (next.denote σ callTokens) rfl found finalFound rfl terminator returned types rfl
      (by simpa only [endFrame] using bindings)
    obtain ⟨final, rest, resultOutcome⟩ := tail σ owner afterCalls stack
    exact ⟨final, prefixPath.trans (.next resumed rest), resultOutcome⟩

structure CheckedTargetCall {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (args : List (Slot Γ)) where
  val : ExpressionResult Γ
  property : CallFunctionDiagram p f args val

mutual
  def callTargetBlock {Γ : Var → Nat} (p : IR.Program) (f : IR.Function) (b : IR.Block)
      (type : Source.IntType) : (fuel pc : Nat) → (scope : Scope Γ) →
      Option {result // CallTargetDiagram p f b pc scope result}
    | 0, _, _ => none
    | fuel+1, pc, scope => do
      if function : p.functions.find? (·.name == f.name) = some f then
        if block : f.blocks.find? (·.name == b.name) = some b then
          match atPc : b.instructions[pc]? with
          | none =>
            if atEnd : pc = b.instructions.length then
              match returned : b.terminator with
              | .return [id, mem, world] =>
                match word : scope.word id with
                | none => none
                | some ⟨n, term⟩ =>
                  if width : n = type.width then
                    let result : CWord Γ := ⟨type, width ▸ term⟩
                    if lookup : [id, mem, world].mapM (fun id => scope.lookup id) =
                        some (callResults result) then
                      return ⟨([], result), atEnd ▸ call_target_return p f b scope _ result block returned lookup⟩
                    else none
                  else none
              | .branch successor => do
                let nextBlock : {nextBlock // f.blocks.find? (·.name == successor.block) = some nextBlock} ←
                  match found : f.blocks.find? (·.name == successor.block) with
                  | none => none
                  | some nextBlock => some ⟨nextBlock, found⟩
                let next : {next // edge callTokenTypes f scope successor = some next} ←
                  match bound : edge callTokenTypes f scope successor with
                  | none => none
                  | some next => some ⟨next, bound⟩
                let tail ← callTargetBlock p f nextBlock.val type fuel 0 next.val
                return ⟨tail.val, atEnd ▸ call_target_branch p f b nextBlock.val scope next.val
                  successor tail.val function block returned
                  (by simpa using List.find?_some nextBlock.property) next.property tail.property⟩
              | _ => none
            else none
          | some inst =>
            match code : instruction scope inst with
            | some (checks, next) => do
              let tail ← callTargetBlock p f b type fuel (pc+1) next
              return ⟨(checks ++ tail.val.1, tail.val.2),
                call_target_instruction p f b pc scope next inst checks tail.val
                  function block atPc code tail.property⟩
            | none =>
              match call : inst.op with
              | .uninitialized (.bits width) => do
                if args : inst.operands = [] then
                  let next : {next // scope.appendSlots inst.results [.opaque (width+2)] = some next} ←
                    match bound : scope.appendSlots inst.results [.opaque (width+2)] with
                    | none => none
                    | some next => some ⟨next, bound⟩
                  let tail ← callTargetBlock p f b type fuel (pc+1) next.val
                  return ⟨tail.val, call_target_uninitialized p f b pc width scope next.val inst
                    tail.val function block atPc call args next.property tail.property⟩
                else none
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
                  let next : {next // scope.appendSlots inst.results (callResults body.val.2) = some next} ←
                    match bound : scope.appendSlots inst.results (callResults body.val.2) with
                    | none => none
                    | some next => some ⟨next, bound⟩
                  let tail ← callTargetBlock p f b type fuel (pc+1) next.val
                  return ⟨(body.val.1 ++ tail.val.1, tail.val.2),
                    call_target_call p f callee.val b pc scope next.val inst args.val body.val tail.val
                      function block atPc (by simpa only [nameEq] using call)
                      (by simpa only [nameEq] using callee.property) args.property results body.property
                      next.property tail.property⟩
                else none
              | _ => none
        else none
      else none

  def callTargetFunction {Γ : Var → Nat} (p : IR.Program) : (fuel : Nat) →
      (f : IR.Function) → (type : Source.IntType) → (args : List (Slot Γ)) →
      Option (CheckedTargetCall p f args)
    | 0, _, _, _ => none
    | fuel+1, f, type, args => do
      let block : {block // f.blocks.head? = some block} ←
        match head : f.blocks.head? with
        | none => none
        | some block => some ⟨block, head⟩
      if found : f.blocks.find? (·.name == block.val.name) = some block.val then
        if signature : f.parameters = args.map (Slot.type callTokenTypes) then
          if parameters : block.val.parameters.map IR.Binding.type = f.parameters then
            let scope : {scope // bindSlots callTokenTypes block.val.parameters args = some scope} ←
              match bound : bindSlots callTokenTypes block.val.parameters args with
              | none => none
              | some scope => some ⟨scope, bound⟩
            let body ← callTargetBlock p f block.val type fuel 0 scope.val
            if returns : f.returns = [.bits body.val.2.type.width, .memory, .world] then
              return ⟨body.val, ⟨block.val, scope.val, block.property, found,
                signature, parameters, scope.property, returns, body.property⟩⟩
            else none
          else none
        else none
      else none
end

theorem CallFunctionDiagram.completes {Γ : Var → Nat} (σ : Subst Γ)
    {p : IR.Program} {f : IR.Function} {args : List (Slot Γ)} {result : ExpressionResult Γ}
    (diagram : CallFunctionDiagram p f args result)
    (function : p.functions.find? (·.name == f.name) = some f) :
    ∃ entry last, IR.initial p f.name (args.map (Slot.denote σ callTokens)) = .ok entry ∧
      IR.Steps p entry last ∧ last.control = .halted
        ((result.denote σ).map (fun value => [sourceValueIR value, .memory ⟨1, []⟩, .world])) := by
  let entry := callState σ f diagram.block 0 1 2 diagram.scope []
  have initial : IR.initial p f.name (args.map (Slot.denote σ callTokens)) = .ok entry := by
    simp [IR.initial, function, diagram.enters σ 1, Bind.bind, Except.bind, pure,
      Except.pure, entry, callState]
  obtain ⟨last, path, outcome⟩ := diagram.body σ 1 2 []
  cases checked : guards σ result.1 with
  | error fault =>
    refine ⟨entry, last, initial, path, ?_⟩
    simpa only [ExpressionResult.denote, checked, Bind.bind, Except.bind, Except.map] using outcome
  | ok unitValue =>
    simp only [checked] at outcome
    obtain ⟨finalBlock, values, afterCalls, ids, finalFound, state, terminator, returned⟩ := outcome
    subst last
    rw [call_results_denote] at returned
    let final : IR.State := ⟨.halted (.ok
      [sourceValueIR (result.2.denote σ), .memory ⟨1, []⟩, .world]), [], afterCalls⟩
    refine ⟨entry, final, initial, path.trans (.next (.execute ?_) (.refl _)), ?_⟩
    · simp [IR.step, IR.execute, function, finalFound, terminator, returned,
        diagram.returns, sourceValueIR, CWord.denote, IR.Value.type, IR.Memory.endFrame,
        Bind.bind, Except.bind, pure, Except.pure, final]
    · simp [final, ExpressionResult.denote, checked, Bind.bind, Except.bind, Except.map, pure, Except.pure]

end EvoIR.Core.Symbolic
