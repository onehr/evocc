import EvoIR.MachineTransportTable
import EvoIR.MachineCorrespondence

/-! The proof relation follows the actual accepted suffix and actual saved
call frames. Its valuations are constructed from real writes and bindings.
No part of this relation is accepted as an annotation or assumed equivalent
execution supplied by the producer. -/
namespace EvoIR.Machine.Transport

structure Context (source target : Program) where
  sourceFunction : Function
  targetFunction : Function
  plans : FunctionPlan
  sourceBlock : Block
  targetBlock : Block
  plan : BlockPlan
  final : BodyResult
  sourceFunctionAt : source.functions.find? (·.name == sourceFunction.name) = some sourceFunction
  targetFunctionAt : target.functions.find? (·.name == sourceFunction.name) = some targetFunction
  names : sourceFunction.name = targetFunction.name
  checked : checkStructure sourceFunction targetFunction plans = true
  sourceBlockAt : findBlock sourceFunction sourceBlock.id = some sourceBlock
  targetBlockAt : findBlock targetFunction sourceBlock.id = some targetBlock
  planAt : findPlan plans sourceBlock.id = some plan
  classes : sourceBlock.insts.all instructionShapeValid = true
  body : checkBody sourceBlock targetBlock plan plans.needed = some final
  controls : matchControls sourceBlock.term targetBlock.term final.source final.target = true
  edges : ∀ pair ∈ sourceBlock.term.edges.zip targetBlock.term.edges,
    edgeValid sourceFunction targetFunction plans final pair.1 pair.2 = true
  entryShape : sourceBlock.id = 0 → targetBlock.args = sourceBlock.args ∧ plan.prefixLength = 0 ∧
    ∀ row ∈ plan.bindings, row.1 = row.2.id

theorem contextForBlock (source target : Program) (annotations : ProgramPlan)
    (accepted : checkProgram source target annotations = true)
    (name : String) (function : Function) (blockId : Nat) (block : Block)
    (functionAt : source.functions.find? (·.name == name) = some function)
    (blockAt : findBlock function blockId = some block) :
    ∃ ctx : Context source target, ctx.sourceFunction = function ∧ ctx.sourceBlock = block := by
  obtain ⟨candidate, plans, candidateAt, _, checked⟩ := checkProgram_function source target annotations
    accepted name function functionAt
  have nameEq : function.name = name := by simpa using List.find?_some functionAt
  have idEq : block.id = blockId := by simpa [findBlock] using List.find?_some blockAt
  obtain ⟨classes, original⟩ := checkStructure_block function candidate plans checked block
    (List.mem_of_find?_eq_some blockAt)
  obtain ⟨targetBlock, plan, final, targetAt, planAt, body, controls, edges, entryShape⟩ :=
    originalBlockValid_data function candidate plans block original
  refine ⟨{
    sourceFunction := function, targetFunction := candidate, plans, sourceBlock := block,
    targetBlock, plan, final, sourceFunctionAt := ?_, targetFunctionAt := ?_,
    names := checkStructure_name function candidate plans checked, checked, sourceBlockAt := ?_,
    targetBlockAt := targetAt, planAt, classes, body, controls, edges, entryShape}, rfl, rfl⟩
  · simpa only [nameEq] using functionAt
  · simpa only [nameEq] using candidateAt
  · simpa only [idEq] using blockAt

theorem contextInFunction {source target : Program} (old : Context source target)
    (blockId : Nat) (block : Block) (blockAt : findBlock old.sourceFunction blockId = some block) :
    ∃ ctx : Context source target, ctx.sourceFunction = old.sourceFunction ∧
      ctx.targetFunction = old.targetFunction ∧ ctx.plans = old.plans ∧ ctx.sourceBlock = block := by
  have idEq : block.id = blockId := by simpa [findBlock] using List.find?_some blockAt
  obtain ⟨classes, original⟩ := checkStructure_block old.sourceFunction old.targetFunction old.plans
    old.checked block (List.mem_of_find?_eq_some blockAt)
  obtain ⟨targetBlock, plan, final, targetAt, planAt, body, controls, edges, entryShape⟩ :=
    originalBlockValid_data old.sourceFunction old.targetFunction old.plans block original
  refine ⟨{
    sourceFunction := old.sourceFunction, targetFunction := old.targetFunction,
    plans := old.plans, sourceBlock := block, targetBlock, plan, final,
    sourceFunctionAt := old.sourceFunctionAt, targetFunctionAt := old.targetFunctionAt,
    names := old.names, checked := old.checked, sourceBlockAt := ?_, targetBlockAt := targetAt,
    planAt, classes, body, controls, edges, entryShape}, rfl, rfl, rfl, rfl⟩
  simpa only [idEq] using blockAt

structure Active {source target : Program} (ctx : Context source target)
    (before after : Activation) where
  sourceName : before.function = ctx.sourceFunction.name
  targetName : after.function = ctx.sourceFunction.name
  sourceId : before.block = ctx.sourceBlock.id
  targetId : after.block = ctx.sourceBlock.id
  owner : before.owner = after.owner
  incoming : before.incoming = after.incoming
  positions : List (Option Nat)
  env : BodyResult
  valuation : Origin → Slot
  sourceBound : before.pc ≤ ctx.sourceBlock.insts.length
  targetBound : after.pc ≤ ctx.targetBlock.insts.length
  accepted : checkBodyAux ctx.sourceBlock.id ctx.plans.needed ctx.targetBlock.insts
    before.pc after.pc (ctx.sourceBlock.insts.drop before.pc) positions env = some ctx.final
  sourceReal : Realizes env.source valuation before.regs
  targetReal : Realizes env.target valuation after.regs
  sourceBefore : KnownBefore ctx.sourceBlock.id before.pc env.source
  targetBefore : KnownBefore ctx.sourceBlock.id before.pc env.target
  normalized : OriginsNormalized valuation

structure Suspended {source target : Program} (ctx : Context source target)
    (before after : ReturnFrame) where
  sourceName : before.caller.function = ctx.sourceFunction.name
  targetName : after.caller.function = ctx.sourceFunction.name
  sourceId : before.caller.block = ctx.sourceBlock.id
  targetId : after.caller.block = ctx.sourceBlock.id
  owner : before.caller.owner = after.caller.owner
  incoming : before.caller.incoming = after.caller.incoming
  results : before.results = after.results
  callPC : Nat
  sourcePC : before.caller.pc = callPC+1
  positions : List (Option Nat)
  env : BodyResult
  valuation : Origin → Slot
  sourceBound : before.caller.pc ≤ ctx.sourceBlock.insts.length
  targetBound : after.caller.pc ≤ ctx.targetBlock.insts.length
  accepted : checkBodyAux ctx.sourceBlock.id ctx.plans.needed ctx.targetBlock.insts
    before.caller.pc after.caller.pc (ctx.sourceBlock.insts.drop before.caller.pc) positions
    {source := resultOrigins ctx.sourceBlock.id callPC 0 before.results env.source,
     target := resultOrigins ctx.sourceBlock.id callPC 0 after.results env.target,
     cuts := ⟨before.caller.pc, after.caller.pc,
       resultOrigins ctx.sourceBlock.id callPC 0 before.results env.source,
       resultOrigins ctx.sourceBlock.id callPC 0 after.results env.target⟩ :: env.cuts} = some ctx.final
  sourceReal : Realizes env.source valuation before.caller.regs
  targetReal : Realizes env.target valuation after.caller.regs
  sourceBefore : KnownBefore ctx.sourceBlock.id callPC env.source
  targetBefore : KnownBefore ctx.sourceBlock.id callPC env.target
  normalized : OriginsNormalized valuation

def FramesRelated (source target : Program) : List ReturnFrame → List ReturnFrame → Prop :=
  Demand.Aligned (fun before after => ∃ ctx : Context source target, Nonempty (Suspended ctx before after))

structure StateRelated (source target : Program) (type : Core.Source.IntType) (entry : State)
    (before after : State) : Prop where
  reached : Core.Transition.Reaches (step .ssa source) entry before
  stack : FramesRelated source target before.stack after.stack
  counter : before.nextFrame = after.nextFrame
  memory : before.memory = after.memory
  active : ∃ a b, before.control = .running a ∧ after.control = .running b ∧
    ∃ ctx : Context source target, Nonempty (Active ctx a b)

theorem Active.foundSourceFunction {source target : Program} {ctx : Context source target}
    {a b : Activation} (active : Active ctx a b) :
    source.functions.find? (·.name == a.function) = some ctx.sourceFunction := by
  simpa only [active.sourceName] using ctx.sourceFunctionAt

theorem Active.foundTargetFunction {source target : Program} {ctx : Context source target}
    {a b : Activation} (active : Active ctx a b) :
    target.functions.find? (·.name == b.function) = some ctx.targetFunction := by
  simpa only [active.targetName] using ctx.targetFunctionAt

theorem Active.foundSourceBlock {source target : Program} {ctx : Context source target}
    {a b : Activation} (active : Active ctx a b) :
    ctx.sourceFunction.blocks.find? (·.id == a.block) = some ctx.sourceBlock := by
  simpa only [active.sourceId, findBlock] using ctx.sourceBlockAt

theorem Active.foundTargetBlock {source target : Program} {ctx : Context source target}
    {a b : Activation} (active : Active ctx a b) :
    ctx.targetFunction.blocks.find? (·.id == b.block) = some ctx.targetBlock := by
  simpa only [active.targetId, findBlock] using ctx.targetBlockAt

theorem Suspended.bound {source target : Program} {ctx : Context source target}
    {before after : ReturnFrame} (frame : Suspended ctx before after)
    (values : List Value) (sourceRegs : RegisterFile)
    (sourceBound : bind before.caller.regs before.results values = .ok sourceRegs) :
    ∃ targetRegs, bind after.caller.regs after.results values = .ok targetRegs ∧
      Nonempty (Active ctx {before.caller with regs := sourceRegs} {after.caller with regs := targetRegs}) := by
  obtain ⟨targetRegs, valuation, targetBound, normalized, sourceReal, targetReal⟩ := bound_results
    ctx.sourceBlock.id frame.callPC before.results values before.caller.regs after.caller.regs sourceRegs
    sourceBound frame.env.source frame.env.target frame.valuation frame.sourceReal frame.targetReal
    (futureFresh_of_before _ _ _ _ frame.sourceBefore)
    (futureFresh_of_before _ _ _ _ frame.targetBefore) frame.normalized
  refine ⟨targetRegs, ?_, ⟨{
    sourceName := frame.sourceName, targetName := frame.targetName,
    sourceId := frame.sourceId, targetId := frame.targetId, owner := frame.owner,
    incoming := frame.incoming, positions := frame.positions,
    env := {
      source := resultOrigins ctx.sourceBlock.id frame.callPC 0 before.results frame.env.source,
      target := resultOrigins ctx.sourceBlock.id frame.callPC 0 after.results frame.env.target,
      cuts := ⟨before.caller.pc, after.caller.pc,
        resultOrigins ctx.sourceBlock.id frame.callPC 0 before.results frame.env.source,
        resultOrigins ctx.sourceBlock.id frame.callPC 0 after.results frame.env.target⟩ :: frame.env.cuts},
    valuation, sourceBound := frame.sourceBound, targetBound := frame.targetBound,
    accepted := frame.accepted, sourceReal, targetReal := ?_, sourceBefore := ?_,
    targetBefore := ?_, normalized}⟩⟩
  · simpa only [← frame.results] using targetBound
  · simpa only [← frame.results] using targetReal
  · simpa only [frame.sourcePC] using knownBefore_results _ _ _ _ _ frame.sourceBefore
  · simpa only [frame.sourcePC] using knownBefore_results ctx.sourceBlock.id frame.callPC 0
      after.results frame.env.target frame.targetBefore

end EvoIR.Machine.Transport
