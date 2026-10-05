import EvoIR.MachinePrivateGuarded
import EvoIR.MachinePrivateCall
import EvoIR.MachinePrivateProposal

/-!
Private storage, guards and calls share one instruction-cursor boundary.
A call is admitted only through a previously proved contextual callee and
the checked demand plan. The resumed suffix retains the actual caller's
storage, pointer identities and executed guards. Construction depth counts
source instructions; cyclic execution closes through the same CFG table.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def privateCallingSuffix (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (callees : List (PrivateCallee source target))
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    (constructionDepth : Nat) → (sb : IR.Block) → (spc : Nat) →
    (tb : EvoIR.Machine.Block) → (tpc : Nat) → (bank : PrivateBank inputWidths) →
    (prior : List (Guard inputWidths)) →
    Option (PLift (PrivateSuffixDiagram source target sf tf type table sb spc tb tpc bank prior))
  | 0, _, _, _, _, _, _ => none
  | depth+1, sb, spc, tb, tpc, bank, prior => do
    let cut ← privateBodyCut source target sf sb spc tf tb tpc bank
    let finish : PLift (PrivateSuffixDiagram source target sf tf type table
        sb (spc + cut.result.sourceCount) tb (tpc + cut.result.targetCount) cut.result.next prior) ←
      if sourceEnd : spc + cut.result.sourceCount = sb.instructions.length then
        if targetEnd : tpc + cut.result.targetCount = tb.insts.length then do
          let control ← privateControl source target sf tf type table sb tb cut.result.next prior
          pure ⟨by
            simpa only [sourceEnd, targetEnd] using
              private_suffix_control source target sf tf type table sb tb cut.result.next prior control.down⟩
        else none
      else do
        let inst ← sb.instructions[spc + cut.result.sourceCount]?
        match inst.op with
        | .call name => do
          let callee ← callees.find? (·.name == name)
          let call ← privateCall source target sf tf type table sb (spc + cut.result.sourceCount)
            tb (tpc + cut.result.targetCount) cut.result.next prior callee plan demand
          let rest ← privateCallingSuffix source target sf tf type table callees plan demand depth
            sb (spc + cut.result.sourceCount + 1) tb (tpc + cut.result.targetCount + 1)
            call.next (guardsForReturn prior)
          pure ⟨call.finish rest.down⟩
        | _ =>
          match privateStoreSpan source target sf sb (spc + cut.result.sourceCount)
              tf tb (tpc + cut.result.targetCount) cut.result.next prior with
          | some stored => do
            let rest ← privateCallingSuffix source target sf tf type table callees plan demand depth
              sb (spc + cut.result.sourceCount + stored.val.sourceCount)
              tb (tpc + cut.result.targetCount + stored.val.targetCount) stored.val.next prior
            pure ⟨private_suffix_span_under source target sf tf type table sb
              (spc + cut.result.sourceCount) tb (tpc + cut.result.targetCount)
              cut.result.next prior stored rest.down⟩
          | none => do
            let guarded ← privateGuardStep source target sf sb (spc + cut.result.sourceCount)
              tf tb (tpc + cut.result.targetCount) cut.result.next prior
            let rest ← privateCallingSuffix source target sf tf type table callees plan demand depth
              sb (spc + cut.result.sourceCount + 1) guarded.val.targetBlock guarded.val.targetPC
              guarded.val.next (prior ++ guarded.val.checks)
            pure ⟨private_suffix_guard source target sf tf type table sb
              (spc + cut.result.sourceCount) tb (tpc + cut.result.targetCount)
              cut.result.next prior guarded.val guarded.property rest.down⟩
    return ⟨private_suffix_cut source target sf tf type table sb spc tb tpc bank prior cut finish.down⟩

def privateCallingBlock (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (callees : List (PrivateCallee source target))
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan)
    (point : PrivateCutpoint sf tf) :
    Option (PLift (PrivateBlockDiagram source target sf tf type table point)) := do
  let checked ← privateCallingSuffix source target sf tf type table callees plan demand
    (point.source.block.instructions.length+1) point.source.block 0 point.targetBlock 0 point.bank []
  return ⟨private_suffix_block source target sf tf type table point checked.down⟩

def privateCallingFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) (callees : List (PrivateCallee source target))
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    Option (CheckedPrivateFunction source target entry type) := do
  let function ← source.functions.find? (·.name == entry)
  let proposal ← PrivateProposal.propose function
  privateFunctionUsing source target entry type
    (fun sf sb tf tb bank => privateCFGBodyUsing source target sf sb tf tb bank type proposal
      (fun table => privateCallingBlock source target sf tf type table callees plan demand))

end EvoIR.Core.Symbolic.MachineSelection
