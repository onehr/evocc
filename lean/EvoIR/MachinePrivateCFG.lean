import EvoIR.MachinePrivateControl
import EvoIR.MachinePrivateFinite

/-!
Finite tables of productive private fragments cover unbounded execution.
Proposal search is bounded; actual execution is not. The first ordinary body
builder independently requires an empty computed guard trace. Guarded bodies
and calling bodies may supply their own checked block builder at the same
context-parametric interface.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def PrivateBlockDiagram (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (point : PrivateCutpoint sf tf) : Prop :=
  ∀ σ values incoming owner frame memory regs logicalInputs sourceStack targetStack observationType,
    point.bank.Models σ values incoming owner frame memory regs →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (fun s t => PrivateBoundary table incoming owner sourceStack targetStack s t ∨
        PrivateReturnBoundary type incoming owner sourceStack targetStack s t)
      (privateState σ values sf point.source.block.name 0 owner frame point.bank.source sourceStack)
      (privateMachineState tf point.targetBlock 0 regs owner frame logicalInputs targetStack memory)

def privateBlock (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (point : PrivateCutpoint sf tf) :
    Option (PLift (PrivateBlockDiagram source target sf tf type table point)) := do
  let body ← privateBodySpan source target sf point.source.block 0 tf point.targetBlock 0 point.bank
  if noChecks : body.val.checks = [] then
    if sourceEnd : body.val.sourceCount = point.source.block.instructions.length then
      if targetEnd : body.val.targetCount = point.targetBlock.insts.length then
        let finish ← privateControl source target sf tf type table point.source.block
          point.targetBlock body.val.next []
        return ⟨by
          intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
            observationType models
          obtain ⟨newValues, nextMemory, nextRegs, sourcePath, targetPath, nextModels⟩ :=
            body.property σ values incoming owner frame memory regs logicalInputs
              sourceStack targetStack models (by simp only [noChecks, guards])
          have sourcePrefix := sourcePath.reaches
          have targetPrefix := targetPath.reaches
          simp only [Nat.zero_add, sourceEnd] at sourcePrefix
          simp only [Nat.zero_add, targetEnd] at targetPrefix
          exact (finish.down σ newValues incoming owner frame nextMemory nextRegs logicalInputs
            sourceStack targetStack observationType nextModels rfl).prepend sourcePrefix targetPrefix⟩
      else none
    else none
  else none

private def checkPrivateBlocks (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf))
    (buildBlock : (point : PrivateCutpoint sf tf) →
      Option (PLift (PrivateBlockDiagram source target sf tf type table point))) :
    (remaining : List (PrivateCutpoint sf tf)) →
    Option (PLift (∀ point ∈ remaining, PrivateBlockDiagram source target sf tf type table point))
  | [] => some ⟨by intro point member; cases member⟩
  | point :: rest => do
    let first ← buildBlock point
    let tail ← checkPrivateBlocks source target sf tf type table buildBlock rest
    return ⟨by
      intro selected member
      rcases List.mem_cons.mp member with equal | later
      · subst selected; exact first.down
      · exact tail.down selected later⟩

structure CheckedPrivateCFG (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) : Type where
  openBody : ∀ incoming owner sourceStack targetStack observationType s t,
    PrivateBoundary table incoming owner sourceStack targetStack s t →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (PrivateReturnBoundary type incoming owner sourceStack targetStack) s t

def privateCFGUsing (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf))
    (buildBlock : (point : PrivateCutpoint sf tf) →
      Option (PLift (PrivateBlockDiagram source target sf tf type table point))) :
    Option (CheckedPrivateCFG source target sf tf type table) := do
  let blocks ← checkPrivateBlocks source target sf tf type table buildBlock table
  return ⟨by
    intro incoming owner sourceStack targetStack observationType s t related
    apply Transition.OpenCheckpoint.closeWithin (entry := related)
    rintro _ _ ⟨point, member, σ, values, frame, memory, regs, logicalInputs, models, rfl, rfl⟩
    exact blocks.down point member σ values incoming owner frame memory regs logicalInputs
      sourceStack targetStack observationType models⟩

def privateCFG (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) :
    Option (CheckedPrivateCFG source target sf tf type table) :=
  privateCFGUsing source target sf tf type table
    (privateBlock source target sf tf type table)

/-- A finite equality reader for whole scalar entry banks. This does not
equate arbitrary raw register files or discard inactive pointer slots. -/
def privateWordBanksEqual {Γ : Var → Nat} : (left right : Bank Γ) →
    Option (PLift (∀ σ, bankDenote σ left = bankDenote σ right))
  | [], [] => some ⟨by intro σ; rfl⟩
  | (id, .word a) :: left, (other, .word b) :: right => do
    if same : id = other then
      let quoted ← wordDiagram a
      let equal ← checkWord ⟨64, quoted.val⟩ b
      let tail ← privateWordBanksEqual left right
      return ⟨by
        intro σ
        have values : a.denote σ = b.denote σ := by
          have checked := equal.down σ
          simp only [BitVec.setWidth_eq] at checked
          exact (quoted.property σ).symm.trans checked.symm
        simp only [bankDenote, List.map_cons, BankSlot.denote, same, values]
        exact congrArg (List.cons _) (tail.down σ)⟩
    else none
  | _, _ => none

def PrivateEntryBoundaryDiagram (sf : IR.Function) (sb : IR.Block)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : PrivateBank inputWidths)
    (table : List (PrivateCutpoint sf tf)) : Prop :=
  ∀ σ values incoming owner frame memory logicalInputs sourceStack targetStack,
    bank.Models σ values incoming owner frame memory (bankDenote σ bank.words) →
    PrivateBoundary table incoming owner sourceStack targetStack
      (privateState σ values sf sb.name 0 owner frame bank.source sourceStack)
      (privateMachineState tf tb 0 (bankDenote σ bank.words) owner frame logicalInputs targetStack memory)

/-- The synthetic self-edge is used only to verify entry rebasing. Both
actual jump results are independently checked to be the original entries;
no extra semantic step is inserted into either execution. -/
def privateEntryBoundary (sf : IR.Function) (sb : IR.Block)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (bank : PrivateBank inputWidths)
    (table : List (PrivateCutpoint sf tf)) :
    Option (PLift (PrivateEntryBoundaryDiagram sf sb tf tb bank table)) := do
  let self : IR.Edge := ⟨sb.name, sb.parameters.map IR.Binding.id⟩
  if sourceIdentity : edge (privateTokenType bank.source.tokens) sf bank.source.scope self =
      some bank.source.scope then
    let native ← jumpBank tf bank.words tb.id tb.args
    let nativeIdentity ← privateWordBanksEqual native.val.bank bank.words
    let matched ← matchPrivateEdge table bank self tb.id tb.args []
    return ⟨by
      intro σ values incoming owner frame memory logicalInputs sourceStack targetStack models
      have sourceJump := edge_sound σ (privateToken values) (privateTokenType bank.source.tokens)
        (private_token_typed values bank.source.tokens models.source.2.1) sf
        ⟨sf.name, sb.name, 0, bank.source.scope.denote σ (privateToken values), owner⟩
        bank.source.scope bank.source.scope self rfl sourceIdentity
      have targetJump := native.property.2.2 σ owner logicalInputs tb.id 0
      rw [nativeIdentity.down σ] at targetJump
      obtain ⟨next, newValues, nextRegs, nextModels, actualSource, actualTarget⟩ :=
        matched.checked σ values incoming owner frame memory (bankDenote σ bank.words) logicalInputs
          sb.name 0 tb.id 0 models rfl
      have sourceSame := Except.ok.inj (actualSource.symm.trans sourceJump)
      have targetSame := Except.ok.inj (actualTarget.symm.trans targetJump)
      refine ⟨matched.point, matched.member, next, newValues, frame, memory, nextRegs,
        logicalInputs, nextModels, ?_, ?_⟩
      · exact congrArg (fun activation : IR.Activation =>
          (⟨.running activation, sourceStack, frame⟩ : IR.State)) sourceSame.symm
      · exact congrArg (fun activation : EvoIR.Machine.Activation =>
          (⟨.running activation, targetStack, frame, memory⟩ : EvoIR.Machine.State)) targetSame.symm⟩
  else none

def privateCFGBodyUsing (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block)
    (bank : PrivateBank inputWidths) (type : Source.IntType) (proposal : List PrivatePointProposal)
    (buildBlock : (table : List (PrivateCutpoint sf tf)) → (point : PrivateCutpoint sf tf) →
      Option (PLift (PrivateBlockDiagram source target sf tf type table point))) :
    Option (PLift (PrivateEntryBodyDiagram source target sf sb tf tb bank type)) := do
  let initialized ← privateInitialBank bank
  let table ← proposal.mapM (privateCutpoint sf tf)
  let entry ← privateEntryBoundary sf sb tf tb bank table
  let body ← privateCFGUsing source target sf tf type table (buildBlock table)
  return ⟨by
    intro σ sourceMemory targetMemory owner frame logicalInputs sourceStack targetStack observationType
      fresh future related
    have models := initialized.down σ sourceMemory targetMemory owner frame fresh future related
    exact body.openBody sourceMemory owner sourceStack targetStack observationType _ _
      (entry.down σ [.memory sourceMemory, .world] sourceMemory owner frame targetMemory
        logicalInputs sourceStack targetStack models)⟩

def privateCFGBody (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block)
    (bank : PrivateBank inputWidths) (type : Source.IntType) (proposal : List PrivatePointProposal) :
    Option (PLift (PrivateEntryBodyDiagram source target sf sb tf tb bank type)) :=
  privateCFGBodyUsing source target sf sb tf tb bank type proposal
    (fun table => privateBlock source target sf tf type table)

def privateCFGFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) (proposal : List PrivatePointProposal) :
    Option (CheckedPrivateFunction source target entry type) :=
  privateFunctionUsing source target entry type
    (fun sf sb tf tb bank => privateCFGBody source target sf sb tf tb bank type proposal)

def privateCFGFunctionPair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (proposal : List PrivatePointProposal) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let function ← privateCFGFunction source target entry type proposal
  privateFunctionPairUsing source target entry parameters type function

end EvoIR.Core.Symbolic.MachineSelection
