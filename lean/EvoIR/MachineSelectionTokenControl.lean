import EvoIR.MachineSelectionTokenCutpoint
import EvoIR.MachineSelectionBranch
import EvoIR.MachineSelectionTokenReturnOpen

namespace EvoIR.Core.Symbolic.MachineSelection

def TokenBoundary {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (TokenCutpoint sf tf)) (owner : Nat)
    (sourceStack : List IR.ReturnFrame) (targetStack : List EvoIR.Machine.ReturnFrame)
    (source : IR.State) (target : EvoIR.Machine.State) : Prop :=
  ∃ point ∈ table, ∃ σ nextFrame incoming memory,
    source = sourceState σ (tokenEnvironment memory) sf point.sourceBlock 0 point.scope owner nextFrame sourceStack ∧
    target = machineState σ tf point.targetBlock 0 point.bank owner nextFrame incoming targetStack memory

def TokenReturnBoundary (type : Source.IntType) (owner : Nat)
    (sourceStack : List IR.ReturnFrame) (targetStack : List EvoIR.Machine.ReturnFrame)
    (source : IR.State) (target : EvoIR.Machine.State) : Prop :=
  ∃ (σ : Subst inputWidths) (word : Word inputWidths) (memory : IR.Memory) (nextFrame : Nat),
    word.1 = type.width ∧
    source = tokenSourceReturnState σ word memory owner nextFrame sourceStack ∧
    target = machineReturnState (machineInputs σ [word]) memory owner nextFrame targetStack

def TokenControlDiagram (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (bank : Bank inputWidths)
    (checks : List (Guard inputWidths)) (memoryKey worldKey : Nat) : Prop :=
  ∀ σ (ρ : Nat → IR.Value) memory owner nextFrame incoming sourceStack targetStack observationType,
    guards σ checks = .ok () → ρ memoryKey = .memory memory → ρ worldKey = .world →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (fun s t => TokenBoundary table owner sourceStack targetStack s t ∨
        TokenReturnBoundary type owner sourceStack targetStack s t)
      (sourceState σ ρ sf sb sb.instructions.length scope owner nextFrame sourceStack)
      (machineState σ tf tb tb.insts.length bank owner nextFrame incoming targetStack memory)

structure MatchedTokenEdge {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (TokenCutpoint sf tf)) (scope : Scope inputWidths) (bank : Bank inputWidths)
    (edge : IR.Edge) (target : Nat) (args : List EvoIR.Machine.VReg)
    (checks : List (Guard inputWidths)) (memoryKey worldKey : Nat) where
  point : TokenCutpoint sf tf
  member : point ∈ table
  checked : ∀ σ (ρ : Nat → IR.Value) memory owner incoming sourceBlock targetBlock sourcePC targetPC,
    guards σ checks = .ok () → ρ memoryKey = .memory memory → ρ worldKey = .world →
    ∃ next : Subst inputWidths,
      IR.jump sf ⟨sf.name, sourceBlock, sourcePC, scope.denote σ ρ, owner⟩ edge =
        .ok ⟨sf.name, point.sourceBlock.name, 0, point.scope.denote next (tokenEnvironment memory), owner⟩ ∧
      EvoIR.Machine.jump .ssa tf
        ⟨tf.name, targetBlock, targetPC, bankDenote σ bank, owner, incoming⟩ target args =
        .ok ⟨tf.name, point.targetBlock.id, 0, bankDenote next point.bank, owner, incoming⟩

def matchTokenEdge {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (TokenCutpoint sf tf)) (scope : Scope inputWidths) (bank : Bank inputWidths)
    (edge : IR.Edge) (target : Nat) (args : List EvoIR.Machine.VReg)
    (checks : List (Guard inputWidths)) (memoryKey worldKey : Nat) :
    Option (MatchedTokenEdge table scope bank edge target args checks memoryKey worldKey) := do
  let selected : {point // table.find? (fun point =>
      point.sourceBlock.name == edge.block && point.targetBlock.id == target) = some point} ←
    match found : table.find? (fun point =>
        point.sourceBlock.name == edge.block && point.targetBlock.id == target) with
    | none => none
    | some point => some ⟨point, found⟩
  let pair ← tokenEdge selected.val scope bank edge target args checks memoryKey worldKey
  return ⟨selected.val, List.mem_of_find?_eq_some selected.property, pair.down⟩

private theorem token_arm_rebase {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (TokenCutpoint sf tf)) (scope : Scope inputWidths) (bank : Bank inputWidths)
    (edge : IR.Edge) (arm : BranchArm inputWidths tf bank)
    (checks : List (Guard inputWidths)) (memoryKey worldKey : Nat)
    (matched : MatchedTokenEdge table scope bank edge arm.target arm.args checks memoryKey worldKey)
    (σ next : Subst inputWidths) (owner nextFrame : Nat) (incoming : List EvoIR.Machine.Value)
    (memory : IR.Memory) (stack : List EvoIR.Machine.ReturnFrame) (block pc : Nat)
    (jumped : EvoIR.Machine.jump .ssa tf
      ⟨tf.name, block, pc, bankDenote σ bank, owner, incoming⟩ arm.target arm.args =
      .ok ⟨tf.name, matched.point.targetBlock.id, 0, bankDenote next matched.point.bank, owner, incoming⟩) :
    machineState σ tf arm.result.block 0 arm.result.bank owner nextFrame incoming stack memory =
      machineState next tf matched.point.targetBlock 0 matched.point.bank owner nextFrame incoming stack memory := by
  have known := arm.checked.2.2 σ owner incoming block pc
  have same := Except.ok.inj (known.symm.trans jumped)
  have lifted := congrArg (fun activation : EvoIR.Machine.Activation =>
    (⟨.running activation, stack, nextFrame, memory⟩ : EvoIR.Machine.State)) same
  simpa only [machineState, arm.checked.2.1] using lifted

private theorem token_one_bit_zero (value : BitVec 1) : (value.toNat == 0) = !value.getLsbD 0 := by
  rw [one_bit_reify value]
  cases value.getLsbD 0 <;> rfl

def tokenBranchControl (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (bank : Bank inputWidths)
    (checks : List (Guard inputWidths) := []) (memoryKey : Nat := 0) (worldKey : Nat := 1) :
    Option (PLift (TokenControlDiagram source target sf tf type table sb scope tb bank checks memoryKey worldKey)) := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          match sourceTerm : sb.terminator, targetTerm : tb.term with
          | .branch edge, .b id args => do
            let matched ← matchTokenEdge table scope bank edge id args checks memoryKey worldKey
            return ⟨by
              intro σ ρ memory owner nextFrame incoming sourceStack targetStack observationType passed current world
              obtain ⟨next, sourceJump, targetJump⟩ := matched.checked σ ρ memory owner incoming
                sb.name tb.id sb.instructions.length tb.insts.length passed current world
              let s := sourceState σ ρ sf sb sb.instructions.length scope owner nextFrame sourceStack
              let t := machineState σ tf tb tb.insts.length bank owner nextFrame incoming targetStack memory
              let ss := sourceState next (tokenEnvironment memory) sf matched.point.sourceBlock 0
                matched.point.scope owner nextFrame sourceStack
              let tt := machineState next tf matched.point.targetBlock 0 matched.point.bank
                owner nextFrame incoming targetStack memory
              have sourceStep : IR.step source s = some ss := by
                simp [s, ss, sourceState, IR.step, IR.execute, sourceFunction, sourceBlock,
                  sourceTerm, sourceJump, bind, Except.bind, pure, Except.pure]
              have targetStep : EvoIR.Machine.step .ssa target t = some tt := by
                exact EvoIR.Machine.Selection.step_branch .ssa target t
                  ⟨tf.name, tb.id, tb.insts.length, bankDenote σ bank, owner, incoming⟩ tf tb id args _
                  rfl targetFunction targetBlock rfl targetTerm targetJump
              exact Transition.OpenCheckpoint.exit
                (.inl ⟨matched.point, matched.member, next, nextFrame, incoming, memory, rfl, rfl⟩)
                (.single sourceStep) (.single targetStep)⟩
          | _, _ => none
        else none
      else none
    else none
  else none

def tokenConditionalControl (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (bank : Bank inputWidths)
    (checks : List (Guard inputWidths) := []) (memoryKey : Nat := 0) (worldKey : Nat := 1) :
    Option (PLift (TokenControlDiagram source target sf tf type table sb scope tb bank checks memoryKey worldKey)) := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      match sourceTerm : sb.terminator with
      | .conditional id yes no => do
        let condition : {condition : Term inputWidths 1 // scope.word id = some ⟨1, condition⟩} ←
          match found : scope.word id with
          | some ⟨1, word⟩ => some ⟨word, found⟩
          | _ => none
        let choice ← branchChoiceUnder target tf tb bank condition.val checks
        let yesEdge ← matchTokenEdge table scope bank yes choice.val.yes.target choice.val.yes.args checks memoryKey worldKey
        let noEdge ← matchTokenEdge table scope bank no choice.val.no.target choice.val.no.args checks memoryKey worldKey
        return ⟨by
          intro σ ρ memory owner nextFrame incoming sourceStack targetStack observationType passed current world
          let s := sourceState σ ρ sf sb sb.instructions.length scope owner nextFrame sourceStack
          let t := machineState σ tf tb tb.insts.length bank owner nextFrame incoming targetStack memory
          have read := word_lookup_sound σ ρ scope id ⟨1, condition.val⟩ condition.property
          have targetStep := choice.property σ owner nextFrame incoming targetStack memory passed
          cases bit : (denoteTerm σ condition.val).getLsbD 0
          · obtain ⟨next, sourceJump, targetJump⟩ := noEdge.checked σ ρ memory owner incoming sb.name tb.id
              sb.instructions.length tb.insts.length passed current world
            let ss := sourceState next (tokenEnvironment memory) sf noEdge.point.sourceBlock 0
              noEdge.point.scope owner nextFrame sourceStack
            let tt := machineState next tf noEdge.point.targetBlock 0 noEdge.point.bank
              owner nextFrame incoming targetStack memory
            have sourceStep : IR.step source s = some ss := by
              have atBit := bit
              rw [BitVec.getLsbD_eq_getElem (by decide : 0 < 1)] at atBit
              simp [s, ss, sourceState, IR.step, IR.execute, sourceFunction, sourceBlock,
                sourceTerm, read, Word.denote, IR.Value.asBits, token_one_bit_zero, atBit, sourceJump,
                bind, Except.bind, pure, Except.pure]
            have rebased := token_arm_rebase table scope bank no choice.val.no checks memoryKey worldKey noEdge σ next
              owner nextFrame incoming memory targetStack tb.id tb.insts.length targetJump
            have machineStep : EvoIR.Machine.step .ssa target t = some tt := by
              simpa only [bit, Bool.false_eq_true, ↓reduceIte, rebased] using targetStep
            exact Transition.OpenCheckpoint.exit
              (.inl ⟨noEdge.point, noEdge.member, next, nextFrame, incoming, memory, rfl, rfl⟩)
              (.single sourceStep) (.single machineStep)
          · obtain ⟨next, sourceJump, targetJump⟩ := yesEdge.checked σ ρ memory owner incoming sb.name tb.id
              sb.instructions.length tb.insts.length passed current world
            let ss := sourceState next (tokenEnvironment memory) sf yesEdge.point.sourceBlock 0
              yesEdge.point.scope owner nextFrame sourceStack
            let tt := machineState next tf yesEdge.point.targetBlock 0 yesEdge.point.bank
              owner nextFrame incoming targetStack memory
            have sourceStep : IR.step source s = some ss := by
              have atBit := bit
              rw [BitVec.getLsbD_eq_getElem (by decide : 0 < 1)] at atBit
              simp [s, ss, sourceState, IR.step, IR.execute, sourceFunction, sourceBlock,
                sourceTerm, read, Word.denote, IR.Value.asBits, token_one_bit_zero, atBit, sourceJump,
                bind, Except.bind, pure, Except.pure]
            have rebased := token_arm_rebase table scope bank yes choice.val.yes checks memoryKey worldKey yesEdge σ next
              owner nextFrame incoming memory targetStack tb.id tb.insts.length targetJump
            have machineStep : EvoIR.Machine.step .ssa target t = some tt := by
              simpa only [bit, ↓reduceIte, rebased] using targetStep
            exact Transition.OpenCheckpoint.exit
              (.inl ⟨yesEdge.point, yesEdge.member, next, nextFrame, incoming, memory, rfl, rfl⟩)
              (.single sourceStep) (.single machineStep)⟩
      | _ => none
    else none
  else none

def tokenControl (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (bank : Bank inputWidths)
    (checks : List (Guard inputWidths) := []) (memoryKey : Nat := 0) (worldKey : Nat := 1) :
    Option (PLift (TokenControlDiagram source target sf tf type table sb scope tb bank checks memoryKey worldKey)) :=
  match tokenReturnOpen source target sf sb scope tf tb bank type checks memoryKey worldKey with
  | some returned => some ⟨by
      intro σ ρ memory owner nextFrame incoming sourceStack targetStack observationType passed current world
      -- The actual return transfer is independent of the enclosing result
      -- observer. Recheck its certificate using that caller observer.
      have result := returned.property.2 σ ρ memory owner nextFrame incoming sourceStack targetStack
        observationType
        (fun s t => TokenBoundary table owner sourceStack targetStack s t ∨
          TokenReturnBoundary type owner sourceStack targetStack s t)
        passed current world
        (.inr ⟨σ, returned.val, memory, nextFrame, returned.property.1, rfl, rfl⟩)
      exact result⟩
  | none => match tokenBranchControl source target sf tf type table sb scope tb bank checks memoryKey worldKey with
    | some proof => some proof
    | none => tokenConditionalControl source target sf tf type table sb scope tb bank checks memoryKey worldKey

end EvoIR.Core.Symbolic.MachineSelection
