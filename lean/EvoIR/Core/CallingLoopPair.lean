import EvoIR.Core.OpenCallSpan

/-! Close a caller's cyclic table after checking every call/return fragment.
Root and callee proposals remain separate, untrusted associations. This entry
uses the same source/signature validation and actual entry binding as the
finite-caller path. No bounded execution is an admission premise. -/
namespace EvoIR.Core.Symbolic

def openCallerTable (source : Source.Program) (target : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (f : IR.Function) (tokens : Bool) (type : Source.IntType)
    (proposals : CalleeProposals) (budget : Nat) :
    (todo : List (JointPoint inputWidths)) → Option (PLift (∀ point ∈ todo,
      ∀ σ owner frame, OpenCaller source target points fid f type (point.source.denote σ fid)
        (point.targetState σ f frame owner [])))
  | [] => some ⟨by intro point member; cases member⟩
  | point :: rest => do
    let head ← openCallerSuffix source target points fid f tokens type proposals budget
      [] point.source point.label 0 point.scope
    let tail ← openCallerTable source target points fid f tokens type proposals budget rest
    return ⟨by
      intro selected member σ owner frame
      rcases List.mem_cons.mp member with rfl | member
      · exact head.down σ trivial owner frame
      · exact tail.down selected member σ owner frame⟩

def callingLoopContinuation (source : Source.Program) (target : IR.Program)
    (fid : Nat) (f : IR.Function) (tokens : Bool) (type : Source.IntType)
    (proposals : CalleeProposals) (layouts : List CutpointLayout) (budget : Nat) :
    ContinuationBuilder source target fid f type := fun _ cursor label pc scope => do
  if start : pc = 0 then
    let points ← layouts.mapM layoutPoint
    let spans ← openCallerTable source target points fid f tokens type proposals budget
      (points.map LayoutPoint.joint)
    let joined ← checkExit points fid f tokens type cursor (.next label scope)
    return ⟨by
      intro σ _ owner frame
      cases joined.down with
      | successor entry =>
        have atEntry : CallerBoundary points fid f (cursor.denote σ fid)
            (cutTargetState σ f label pc owner frame scope []) :=
          ⟨owner, by simpa only [TargetExit.callState, start] using entry σ frame none owner []⟩
        apply Transition.OpenCheckpoint.close (boundary := CallerBoundary points fid f) _ atEntry
        rintro a b ⟨savedOwner, point, member, ρ, nextFrame, rfl, rfl⟩
        exact spans.down point member ρ savedOwner nextFrame⟩
  else none

def callingLoopPair (source : Source.Program) (target : IR.Program)
    (proposals : CalleeProposals) (budget : Nat) : Option (CallFrontierPair source target) :=
  callFrontierPairWith source target fun fid f tokens type =>
    match proposals fid with
    | none => checkedSuffix source target fid f tokens type proposals budget
    | some (fuel, layouts) => callingLoopContinuation source target fid f tokens type proposals layouts fuel

end EvoIR.Core.Symbolic
