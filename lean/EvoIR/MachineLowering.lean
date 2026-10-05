import EvoIR.MachineSelectionModule
import EvoIR.MachineSelectionDemandModule
import EvoIR.MachineTransportSound
import EvoIR.MachineTransportProposal
import EvoIR.IRPoisonProposal
import EvoIR.IRPoisonSound

/-!
Actual EvoIR to actual postSSA MIR admission. Both component proofs refer to
the same supplied SSA program and initial state. The DCE progress premise is
obtained from the selection theorem, not from finite execution or a producer
annotation. No allocation, physical-frame or emission result is claimed.
-/
namespace EvoIR.Machine

open Core
open Core.Symbolic
open Core.Symbolic.MachineSelection

def loweringArguments (tokens : Bool) (values : List Source.Value) : List IR.Value :=
  values.map sourceValueIR ++ if tokens then [.memory ⟨1, []⟩, .world] else []

structure CheckedLowering (source : IR.Program) (ssa target : Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) where
  tokens : Bool
  checked : ∀ values, values.map Source.Value.type = parameters.map (fun p => p.type.type) →
    ∃ sourceEntry targetEntry,
      IR.initial source entry (loweringArguments tokens values) = .ok sourceEntry ∧
      initial target entry (values.map sourceArgument) = .ok targetEntry ∧
      IRToMachineRefines source target type sourceEntry targetEntry

def lowerScalarPair (source : IR.Program) (ssa target : Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (selection : ScalarSelectionPair source ssa entry parameters type)
    (plan : Transport.ProgramPlan) (accepted : Transport.checkProgram ssa target plan = true) :
    CheckedLowering source ssa target entry parameters type := ⟨false, by
  intro values typed
  obtain ⟨sourceEntry, ssaEntry, sourceStarted, ssaStarted, selected⟩ := selection.checked values typed
  obtain ⟨targetStarted, transported⟩ := Transport.sound ssa target plan accepted type entry
    (values.map sourceArgument) ⟨1, []⟩ ssaEntry ssaStarted selected.progress
  refine ⟨sourceEntry, ssaEntry, ?_, targetStarted, selection_transport_refines selected transported⟩
  simpa only [loweringArguments, Bool.false_eq_true, ↓reduceIte, List.append_nil] using sourceStarted⟩

def lowerTokenPair (source : IR.Program) (ssa target : Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (selection : TokenSelectionPair source ssa entry parameters type)
    (plan : Transport.ProgramPlan) (accepted : Transport.checkProgram ssa target plan = true) :
    CheckedLowering source ssa target entry parameters type := ⟨true, by
  intro values typed
  obtain ⟨sourceEntry, ssaEntry, sourceStarted, ssaStarted, selected⟩ := selection.checked values typed
  obtain ⟨targetStarted, transported⟩ := Transport.sound ssa target plan accepted type entry
    (values.map sourceArgument) ⟨1, []⟩ ssaEntry ssaStarted selected.progress
  exact ⟨sourceEntry, ssaEntry, sourceStarted, targetStarted,
    selection_transport_refines selected transported⟩⟩

private def checkSelection (source : IR.Program) (ssa target : Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (plan : Transport.ProgramPlan) (accepted : Transport.checkProgram ssa target plan = true) :
    Option (CheckedLowering source ssa target entry parameters type) := do
  let function ← source.functions.find? (·.name == entry)
  match scalarFunctionPair source ssa entry parameters type (tokenBlockProposal function) with
  | some selected => return lowerScalarPair source ssa target entry parameters type selected plan accepted
  | none =>
    match tokenModulePair source ssa entry parameters type with
    | some selected => return lowerTokenPair source ssa target entry parameters type selected plan accepted
    | none =>
      let selected ← tokenDemandModulePair source ssa entry parameters type
      return lowerTokenPair source ssa target entry parameters type selected plan accepted

/-- A proved normalization leg changes only independently checked dead poison
definitions. The final theorem still starts from the supplied original IR. -/
def lowerNormalizedPair (source : IR.Program) (ssa target : Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (proposal : IR.Poison.CheckedProposal source)
    (lowered : CheckedLowering proposal.target ssa target entry parameters type) :
    CheckedLowering source ssa target entry parameters type := ⟨lowered.tokens, by
  intro values typed
  obtain ⟨sourceEntry, targetEntry, sourceStarted, targetStarted, refines⟩ := lowered.checked values typed
  obtain ⟨originalStarted, complete⟩ := IR.Poison.sound source proposal.target proposal.plan
    proposal.checked type entry (loweringArguments lowered.tokens values) sourceEntry sourceStarted
    (step .postSSA target) (observe type) targetEntry refines
  exact ⟨sourceEntry, targetEntry, originalStarted, targetStarted, complete⟩⟩

/-- An explicitly supplied transport plan is checked or refused. A failed
plan is never replaced. Optional poison normalization is a separately checked
IR-to-IR proof, composed with the same actual SSA/postSSA pair and plan. -/
def checkLoweringWithPlan (source : IR.Program) (ssa target : Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (plan : Transport.ProgramPlan) :
    Option (CheckedLowering source ssa target entry parameters type) := do
  if accepted : Transport.checkProgram ssa target plan = true then
    match checkSelection source ssa target entry parameters type plan accepted with
    | some checked => return checked
    | none =>
      let proposal ← IR.Poison.propose source
      if proposal.target = source then none else do
        let lowered ← checkSelection proposal.target ssa target entry parameters type plan accepted
        return lowerNormalizedPair source ssa target entry parameters type proposal lowered
  else none

/-- Proposal discovery is untrusted. Universal admission uses the exact
two-input transport check and the independent selection result above. -/
def checkLowering (source : IR.Program) (ssa target : Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) :
    Option (CheckedLowering source ssa target entry parameters type) := do
  let plan ← Transport.proposeProgram ssa target
  checkLoweringWithPlan source ssa target entry parameters type plan

end EvoIR.Machine
