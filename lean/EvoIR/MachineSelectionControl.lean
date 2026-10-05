import EvoIR.MachineSelectionCutpoint
import EvoIR.MachineSelectionBranch

/-!
Productive scalar control-flow leaves over actual paired CFGs. Every open
edge takes one real source step and one real machine step before entering a
checked canonical cutpoint. Thus a cyclic proposal cannot prove itself by an
identity edge. Function calls use their separate suspended-context relation.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def ScalarBoundary {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (ScalarCutpoint sf tf)) (source : IR.State) (target : EvoIR.Machine.State) : Prop :=
  ∃ point ∈ table, ∃ σ owner nextFrame incoming memory,
    source = sourceState σ (fun _ => .world) sf point.sourceBlock 0 point.scope owner nextFrame [] ∧
    target = machineState σ tf point.targetBlock 0 point.bank owner nextFrame incoming [] memory

def ScalarControlDiagram {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (ScalarCutpoint sf tf)) (sb : IR.Block) (scope : Scope Γ)
    (tb : EvoIR.Machine.Block) (bank : Bank Γ) (checks : List (Guard Γ) := []) : Prop :=
  ∀ σ owner nextFrame incoming memory,
    guards σ checks = .ok () →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget type) (EvoIR.Machine.observe type) (ScalarBoundary table)
      (sourceState σ (fun _ => .world) sf sb sb.instructions.length scope owner nextFrame [])
      (machineState σ tf tb tb.insts.length bank owner nextFrame incoming [] memory)

structure MatchedScalarEdge {Γ : Var → Nat} {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (ScalarCutpoint sf tf)) (scope : Scope Γ) (bank : Bank Γ)
    (edge : IR.Edge) (target : Nat) (args : List EvoIR.Machine.VReg)
    (checks : List (Guard Γ) := []) : Type where
  point : ScalarCutpoint sf tf
  member : point ∈ table
  checked : ∀ σ owner incoming sourceBlock targetBlock sourcePC targetPC,
    guards σ checks = .ok () →
    ∃ next : Subst inputWidths,
      IR.jump sf ⟨sf.name, sourceBlock, sourcePC, scope.denote σ (fun _ => .world), owner⟩ edge =
        .ok ⟨sf.name, point.sourceBlock.name, 0, point.scope.denote next (fun _ => .world), owner⟩ ∧
      EvoIR.Machine.jump .ssa tf
        ⟨tf.name, targetBlock, targetPC, bankDenote σ bank, owner, incoming⟩ target args =
        .ok ⟨tf.name, point.targetBlock.id, 0, bankDenote next point.bank, owner, incoming⟩

def matchScalarEdge {Γ : Var → Nat} {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (ScalarCutpoint sf tf)) (scope : Scope Γ) (bank : Bank Γ)
    (edge : IR.Edge) (target : Nat) (args : List EvoIR.Machine.VReg)
    (checks : List (Guard Γ) := []) :
    Option (MatchedScalarEdge table scope bank edge target args checks) := do
  let selected : {point // table.find? (fun point =>
      point.sourceBlock.name == edge.block && point.targetBlock.id == target) = some point} ←
    match found : table.find? (fun point =>
        point.sourceBlock.name == edge.block && point.targetBlock.id == target) with
    | none => none
    | some point => some ⟨point, found⟩
  let pair ← scalarEdge selected.val scope bank edge target args checks
  return ⟨selected.val, List.mem_of_find?_eq_some selected.property, pair.down⟩

def unconditionalControl {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (ScalarCutpoint sf tf)) (sb : IR.Block) (scope : Scope Γ)
    (tb : EvoIR.Machine.Block) (bank : Bank Γ) (checks : List (Guard Γ) := []) :
    Option (PLift (ScalarControlDiagram source target sf tf type table sb scope tb bank checks)) := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          match sourceTerm : sb.terminator, targetTerm : tb.term with
          | .branch edge, .b id args => do
            let matched ← matchScalarEdge table scope bank edge id args checks
            return ⟨by
              intro σ owner nextFrame incoming memory passed
              obtain ⟨next, sourceJump, targetJump⟩ := matched.checked σ owner incoming sb.name tb.id
                sb.instructions.length tb.insts.length passed
              let s := sourceState σ (fun _ => .world) sf sb sb.instructions.length scope owner nextFrame []
              let t := machineState σ tf tb tb.insts.length bank owner nextFrame incoming [] memory
              let ss := sourceState next (fun _ => .world) sf matched.point.sourceBlock 0
                matched.point.scope owner nextFrame []
              let tt := machineState next tf matched.point.targetBlock 0 matched.point.bank
                owner nextFrame incoming [] memory
              have sourceStep : IR.step source s = some ss := by
                simp [s, ss, sourceState, IR.step, IR.execute, sourceFunction, sourceBlock,
                  sourceTerm, sourceJump, bind, Except.bind, pure, Except.pure]
              have targetStep : EvoIR.Machine.step .ssa target t = some tt := by
                exact EvoIR.Machine.Selection.step_branch .ssa target t
                  ⟨tf.name, tb.id, tb.insts.length, bankDenote σ bank, owner, incoming⟩ tf tb id args _
                  rfl targetFunction targetBlock rfl targetTerm targetJump
              apply Transition.OpenCheckpoint.exit
                (boundary := ScalarBoundary table) (s' := ss) (t' := tt)
                ⟨matched.point, matched.member, next, owner, nextFrame, incoming, memory, rfl, rfl⟩
                (.single sourceStep) (.single targetStep)⟩
          | _, _ => none
        else none
      else none
    else none
  else none

private theorem arm_state_rebase {Γ : Var → Nat} {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (ScalarCutpoint sf tf)) (scope : Scope Γ) (bank : Bank Γ)
    (edge : IR.Edge) (arm : BranchArm Γ tf bank)
    (checks : List (Guard Γ))
    (matched : MatchedScalarEdge table scope bank edge arm.target arm.args checks)
    (σ : Subst Γ) (next : Subst inputWidths) (owner nextFrame : Nat)
    (incoming : List EvoIR.Machine.Value) (memory : EvoIR.Machine.Memory)
    (block pc : Nat)
    (jumped : EvoIR.Machine.jump .ssa tf
      ⟨tf.name, block, pc, bankDenote σ bank, owner, incoming⟩ arm.target arm.args =
      .ok ⟨tf.name, matched.point.targetBlock.id, 0, bankDenote next matched.point.bank, owner, incoming⟩) :
    machineState σ tf arm.result.block 0 arm.result.bank owner nextFrame incoming [] memory =
      machineState next tf matched.point.targetBlock 0 matched.point.bank owner nextFrame incoming [] memory := by
  have known := arm.checked.2.2 σ owner incoming block pc
  have same := Except.ok.inj (known.symm.trans jumped)
  have lifted := congrArg (fun activation : EvoIR.Machine.Activation =>
    (⟨.running activation, [], nextFrame, memory⟩ : EvoIR.Machine.State)) same
  simpa only [machineState, arm.checked.2.1] using lifted

private theorem one_bit_zero (value : BitVec 1) :
    (value.toNat == 0) = !value.getLsbD 0 := by
  rw [one_bit_reify value]
  cases value.getLsbD 0 <;> rfl

def conditionalControl {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (ScalarCutpoint sf tf)) (sb : IR.Block) (scope : Scope Γ)
    (tb : EvoIR.Machine.Block) (bank : Bank Γ) (checks : List (Guard Γ) := []) :
    Option (PLift (ScalarControlDiagram source target sf tf type table sb scope tb bank checks)) := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      match sourceTerm : sb.terminator with
      | .conditional id yes no => do
        let condition : {condition : Term Γ 1 // scope.word id = some ⟨1, condition⟩} ←
          match found : scope.word id with
          | some ⟨1, word⟩ => some ⟨word, found⟩
          | _ => none
        let choice ← branchChoiceUnder target tf tb bank condition.val checks
        let yesEdge ← matchScalarEdge table scope bank yes choice.val.yes.target choice.val.yes.args checks
        let noEdge ← matchScalarEdge table scope bank no choice.val.no.target choice.val.no.args checks
        return ⟨by
          intro σ owner nextFrame incoming memory passed
          let s := sourceState σ (fun _ => .world) sf sb sb.instructions.length scope owner nextFrame []
          let t := machineState σ tf tb tb.insts.length bank owner nextFrame incoming [] memory
          have read := word_lookup_sound σ (fun _ => .world) scope id ⟨1, condition.val⟩ condition.property
          have targetStep := choice.property σ owner nextFrame incoming [] memory passed
          cases bit : (denoteTerm σ condition.val).getLsbD 0
          · obtain ⟨next, sourceJump, targetJump⟩ := noEdge.checked σ owner incoming sb.name tb.id
              sb.instructions.length tb.insts.length passed
            let ss := sourceState next (fun _ => .world) sf noEdge.point.sourceBlock 0
              noEdge.point.scope owner nextFrame []
            let tt := machineState next tf noEdge.point.targetBlock 0 noEdge.point.bank
              owner nextFrame incoming [] memory
            have sourceStep : IR.step source s = some ss := by
              have atBit := bit
              rw [BitVec.getLsbD_eq_getElem (by decide : 0 < 1)] at atBit
              simp [s, ss, sourceState, IR.step, IR.execute, sourceFunction, sourceBlock,
                sourceTerm, read, Word.denote, IR.Value.asBits, one_bit_zero, atBit, sourceJump,
                bind, Except.bind, pure, Except.pure]
            have rebased := arm_state_rebase table scope bank no choice.val.no checks noEdge σ next owner
              nextFrame incoming memory tb.id tb.insts.length targetJump
            have machineStep : EvoIR.Machine.step .ssa target t = some tt := by
              simpa only [bit, Bool.false_eq_true, ↓reduceIte, rebased] using targetStep
            exact Transition.OpenCheckpoint.exit
              ⟨noEdge.point, noEdge.member, next, owner, nextFrame, incoming, memory, rfl, rfl⟩
              (.single sourceStep) (.single machineStep)
          · obtain ⟨next, sourceJump, targetJump⟩ := yesEdge.checked σ owner incoming sb.name tb.id
              sb.instructions.length tb.insts.length passed
            let ss := sourceState next (fun _ => .world) sf yesEdge.point.sourceBlock 0
              yesEdge.point.scope owner nextFrame []
            let tt := machineState next tf yesEdge.point.targetBlock 0 yesEdge.point.bank
              owner nextFrame incoming [] memory
            have sourceStep : IR.step source s = some ss := by
              have atBit := bit
              rw [BitVec.getLsbD_eq_getElem (by decide : 0 < 1)] at atBit
              simp [s, ss, sourceState, IR.step, IR.execute, sourceFunction, sourceBlock,
                sourceTerm, read, Word.denote, IR.Value.asBits, one_bit_zero, atBit, sourceJump,
                bind, Except.bind, pure, Except.pure]
            have rebased := arm_state_rebase table scope bank yes choice.val.yes checks yesEdge σ next owner
              nextFrame incoming memory tb.id tb.insts.length targetJump
            have machineStep : EvoIR.Machine.step .ssa target t = some tt := by
              simpa only [bit, ↓reduceIte, rebased] using targetStep
            exact Transition.OpenCheckpoint.exit
              ⟨yesEdge.point, yesEdge.member, next, owner, nextFrame, incoming, memory, rfl, rfl⟩
              (.single sourceStep) (.single machineStep)⟩
      | _ => none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
