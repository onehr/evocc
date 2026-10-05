import EvoIR.MachinePrivateEdges
import EvoIR.MachinePrivateBranch

/-!
Actual private control-flow leaves. Each accepted edge performs one real IR
step and one real SSA MIR step, then enters a separately checked canonical
private cutpoint. Source and target memories remain independently modeled.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def PrivateControlDiagram {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (type : Source.IntType) (table : List (PrivateCutpoint sf tf))
    (sb : IR.Block) (tb : EvoIR.Machine.Block) (bank : PrivateBank Γ)
    (checks : List (Guard Γ)) : Prop :=
  ∀ σ values incoming owner frame memory regs logicalInputs sourceStack targetStack observationType,
    bank.Models σ values incoming owner frame memory regs → guards σ checks = .ok () →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (fun s t => PrivateBoundary table incoming owner sourceStack targetStack s t ∨
        PrivateReturnBoundary type incoming owner sourceStack targetStack s t)
      (privateState σ values sf sb.name sb.instructions.length owner frame bank.source sourceStack)
      (privateMachineState tf tb tb.insts.length regs owner frame logicalInputs targetStack memory)

structure MatchedPrivateEdge {Γ : Var → Nat} {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (PrivateCutpoint sf tf)) (bank : PrivateBank Γ)
    (edge : IR.Edge) (target : Nat) (args : List EvoIR.Machine.VReg)
    (checks : List (Guard Γ)) where
  point : PrivateCutpoint sf tf
  member : point ∈ table
  checked : PrivateEdgeDiagram sf tf bank point edge target args checks

def matchPrivateEdge {Γ : Var → Nat} {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (PrivateCutpoint sf tf)) (bank : PrivateBank Γ)
    (edge : IR.Edge) (target : Nat) (args : List EvoIR.Machine.VReg)
    (checks : List (Guard Γ)) :
    Option (MatchedPrivateEdge table bank edge target args checks) := do
  let selected : {point // table.find? (fun point =>
      point.source.block.name == edge.block && point.targetBlock.id == target) = some point} ←
    match found : table.find? (fun point =>
        point.source.block.name == edge.block && point.targetBlock.id == target) with
    | none => none
    | some point => some ⟨point, found⟩
  let checked ← privateEdge sf tf bank selected.val edge target args checks
  return ⟨selected.val, List.mem_of_find?_eq_some selected.property, checked.down⟩

def privateBranchControl {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (sb : IR.Block) (tb : EvoIR.Machine.Block)
    (bank : PrivateBank Γ) (checks : List (Guard Γ)) :
    Option (PLift (PrivateControlDiagram source target sf tf type table sb tb bank checks)) := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          match sourceTerm : sb.terminator, targetTerm : tb.term with
          | .branch edge, .b id args => do
            let matched ← matchPrivateEdge table bank edge id args checks
            return ⟨by
              intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
                observationType models passed
              obtain ⟨next, newValues, nextRegs, nextModels, sourceJump, targetJump⟩ :=
                matched.checked σ values incoming owner frame memory regs logicalInputs
                  sb.name sb.instructions.length tb.id tb.insts.length models passed
              let s := privateState σ values sf sb.name sb.instructions.length owner frame bank.source sourceStack
              let t := privateMachineState tf tb tb.insts.length regs owner frame logicalInputs targetStack memory
              let ss := privateState next newValues sf matched.point.source.block.name 0 owner frame
                matched.point.bank.source sourceStack
              let tt := privateMachineState tf matched.point.targetBlock 0 nextRegs owner frame
                logicalInputs targetStack memory
              have sourceStep : IR.step source s = some ss := by
                simp [s, ss, privateState, IR.step, IR.execute, sourceFunction, sourceBlock,
                  sourceTerm, sourceJump, Bind.bind, Except.bind, Pure.pure, Except.pure]
              have targetStep : EvoIR.Machine.step .ssa target t = some tt := by
                exact EvoIR.Machine.Selection.step_branch .ssa target t
                  ⟨tf.name, tb.id, tb.insts.length, regs, owner, logicalInputs⟩ tf tb id args _
                  rfl targetFunction targetBlock rfl targetTerm targetJump
              exact Transition.OpenCheckpoint.exit
                (.inl ⟨matched.point, matched.member, next, newValues, frame, memory, nextRegs,
                  logicalInputs, nextModels, rfl, rfl⟩)
                (.single sourceStep) (.single targetStep)⟩
          | _, _ => none
        else none
      else none
    else none
  else none

private theorem private_one_bit_zero (value : BitVec 1) :
    (value.toNat == 0) = !value.getLsbD 0 := by
  rw [one_bit_reify value]
  cases value.getLsbD 0 <;> rfl

def privateConditionalControl {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (type : Source.IntType) (table : List (PrivateCutpoint sf tf))
    (sb : IR.Block) (tb : EvoIR.Machine.Block) (bank : PrivateBank Γ)
    (checks : List (Guard Γ)) :
    Option (PLift (PrivateControlDiagram source target sf tf type table sb tb bank checks)) := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      match sourceTerm : sb.terminator with
      | .conditional id yes no => do
        let condition : {condition : Term Γ 1 // bank.source.scope.word id = some ⟨1, condition⟩} ←
          match found : bank.source.scope.word id with
          | some ⟨1, word⟩ => some ⟨word, found⟩
          | _ => none
        let choice ← privateBranchChoiceUnder target tf tb bank.words condition.val checks
        let yesEdge ← matchPrivateEdge table bank yes choice.val.yes.target choice.val.yes.args checks
        let noEdge ← matchPrivateEdge table bank no choice.val.no.target choice.val.no.args checks
        return ⟨by
          intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
            observationType models passed
          let s := privateState σ values sf sb.name sb.instructions.length owner frame bank.source sourceStack
          let t := privateMachineState tf tb tb.insts.length regs owner frame logicalInputs targetStack memory
          have read := word_lookup_sound σ (privateToken values) bank.source.scope id
            ⟨1, condition.val⟩ condition.property
          cases bit : (denoteTerm σ condition.val).getLsbD 0
          · obtain ⟨next, newValues, nextRegs, nextModels, sourceJump, targetJump⟩ :=
              noEdge.checked σ values incoming owner frame memory regs logicalInputs
                sb.name sb.instructions.length tb.id tb.insts.length models passed
            let ss := privateState next newValues sf noEdge.point.source.block.name 0 owner frame
              noEdge.point.bank.source sourceStack
            let tt := privateMachineState tf noEdge.point.targetBlock 0 nextRegs owner frame
              logicalInputs targetStack memory
            have sourceStep : IR.step source s = some ss := by
              have atBit := bit
              rw [BitVec.getLsbD_eq_getElem (by decide : 0 < 1)] at atBit
              simp [s, ss, privateState, IR.step, IR.execute, sourceFunction, sourceBlock,
                sourceTerm, read, Word.denote, IR.Value.asBits, private_one_bit_zero,
                atBit, sourceJump, Bind.bind, Except.bind, Pure.pure, Except.pure]
            have targetStep : EvoIR.Machine.step .ssa target t = some tt := by
              apply choice.property σ regs owner frame logicalInputs targetStack memory _
                models.words passed
              simpa only [bit, Bool.false_eq_true, ↓reduceIte] using targetJump
            exact Transition.OpenCheckpoint.exit
              (.inl ⟨noEdge.point, noEdge.member, next, newValues, frame, memory, nextRegs,
                logicalInputs, nextModels, rfl, rfl⟩)
              (.single sourceStep) (.single targetStep)
          · obtain ⟨next, newValues, nextRegs, nextModels, sourceJump, targetJump⟩ :=
              yesEdge.checked σ values incoming owner frame memory regs logicalInputs
                sb.name sb.instructions.length tb.id tb.insts.length models passed
            let ss := privateState next newValues sf yesEdge.point.source.block.name 0 owner frame
              yesEdge.point.bank.source sourceStack
            let tt := privateMachineState tf yesEdge.point.targetBlock 0 nextRegs owner frame
              logicalInputs targetStack memory
            have sourceStep : IR.step source s = some ss := by
              have atBit := bit
              rw [BitVec.getLsbD_eq_getElem (by decide : 0 < 1)] at atBit
              simp [s, ss, privateState, IR.step, IR.execute, sourceFunction, sourceBlock,
                sourceTerm, read, Word.denote, IR.Value.asBits, private_one_bit_zero,
                atBit, sourceJump, Bind.bind, Except.bind, Pure.pure, Except.pure]
            have targetStep : EvoIR.Machine.step .ssa target t = some tt := by
              apply choice.property σ regs owner frame logicalInputs targetStack memory _
                models.words passed
              simpa only [bit, ↓reduceIte] using targetJump
            exact Transition.OpenCheckpoint.exit
              (.inl ⟨yesEdge.point, yesEdge.member, next, newValues, frame, memory, nextRegs,
                logicalInputs, nextModels, rfl, rfl⟩)
              (.single sourceStep) (.single targetStep)⟩
      | _ => none
    else none
  else none

def privateControl {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (sb : IR.Block) (tb : EvoIR.Machine.Block)
    (bank : PrivateBank Γ) (checks : List (Guard Γ)) :
    Option (PLift (PrivateControlDiagram source target sf tf type table sb tb bank checks)) :=
  match privateReturn source target sf sb tf tb bank type checks with
  | some returned => some ⟨by
      intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
        observationType models passed
      exact (returned.down σ values incoming owner frame memory regs logicalInputs
        sourceStack targetStack observationType models passed).bind
          (fun _ _ atReturn => .inl (.inr atReturn))⟩
  | none => match privateBranchControl source target sf tf type table sb tb bank checks with
    | some branched => some branched
    | none => privateConditionalControl source target sf tf type table sb tb bank checks

end EvoIR.Core.Symbolic.MachineSelection
