import EvoIR.Core.PrivateFunctionContext
import EvoIR.Core.PrivateCheckpointBank
import EvoIR.Core.PrivateProject

/-! Private checkpoint instantiation. Every backedge transports its actual
current memory and pointers into a fresh, bounded symbolic bank. -/
namespace EvoIR.Core.Symbolic

def PrivateJointRelated (target : IR.Function) (points : List (PrivateLayoutPoint target)) (incoming : IR.Memory)
    (fid : Nat) (context : Option SourceReturnContext)
    (owner : Nat) (stack : List IR.ReturnFrame) (s : Source.State) (t : IR.State) : Prop :=
  ∃ point ∈ points, ∃ σ values frame,
    point.config.Realizes σ values incoming owner frame ∧
    s = (point.layout.site.cursor (canonicalStore point.layout.shape)).denote σ fid context ∧
    t = privateState σ values target point.layout.label 0 owner frame point.config stack

inductive PrivateExitAgreement (target : IR.Function) (points : List (PrivateLayoutPoint target)) (fid : Nat)
    (type : Source.IntType) :
    SourceCursor inputWidths → PrivateExit inputWidths → Prop where
  | returned {s word targetWord config} (control : s.control = .returned word)
      (typed : word.type = type) (boundary : s.continuation = [])
      (same : ∀ σ, targetWord.denote σ = word.denote σ) :
      PrivateExitAgreement target points fid type s (.returned targetWord config)
  | successor {s label pc config} (closed : ∀ σ values incoming context owner frame stack,
      config.Realizes σ values incoming owner frame →
      PrivateJointRelated target points incoming fid context owner stack
        (s.denote σ fid context) (privateState σ values target label pc owner frame config stack)) :
      PrivateExitAgreement target points fid type s (.next label pc config)

def privateProjectBank {Γ : Var → Nat} (config : PrivateConfig Γ) (layout : CutpointLayout) :
    Option (List (Slot Γ)) := do
  let slots ← layout.links.mapM (fun (id, _, _) => config.scope.lookup id)
  return [.opaque config.memory, .opaque config.world] ++ slots

def checkPrivateSuccessor (target : IR.Function) (points : List (PrivateLayoutPoint target))
    (candidate : {point // point ∈ points}) (fid : Nat)
    (type : Source.IntType) (s : SourceCursor inputWidths) (t : PrivateExit inputWidths) :
    Option (PLift (PrivateExitAgreement target points fid type s t)) :=
  match t with
  | .returned _ _ => none
  | .next label pc config => do
    let point := candidate.val
    if atSite : s.site = some point.layout.site then
      if shape : s.locals.map LocalSlot.shape = point.layout.shape then
        if atLabel : label = point.layout.label then
          if atStart : pc = 0 then
            let template : {config // privateProject point.block.parameters point.layout s.locals = some config} ←
              match h : privateProject point.block.parameters point.layout s.locals with
              | none => none
              | some config => some ⟨config, h⟩
            let bank ← privateProjectBank config point.layout
            let rebind ← privateRebindingCheck config template.val bank
            return ⟨.successor (by
              intro σ values incoming context owner frame stack related
              let newValues := bank.map (Slot.denote σ (privateToken values))
              obtain ⟨rebinding⟩ := rebind.down σ values related.2.1 related.2.2.1
              have intermediate := private_rebind_realizes σ σ values newValues incoming owner frame
                config template.val related rebinding
              obtain ⟨τ, stored⟩ := canonical_store_of_shape σ s.locals point.layout.shape shape
              have canonical := private_project_realizes σ τ newValues incoming owner frame
                point.block.parameters point.layout s.locals (canonicalStore point.layout.shape)
                template.val point.config template.property point.projected stored.symm intermediate
              refine ⟨point, candidate.property, τ, newValues, frame, canonical.1,
                source_site_sound σ τ s point.layout.site (canonicalStore point.layout.shape)
                  fid atSite stored.symm context, ?_⟩
              simp only [privateState]
              rw [atLabel, atStart, ← canonical.2, rebinding.scope])⟩
          else none
        else none
      else none
    else none

def checkPrivateExit (target : IR.Function) (points : List (PrivateLayoutPoint target)) (fid : Nat)
    (type : Source.IntType)
    (s : SourceCursor inputWidths) (t : PrivateExit inputWidths) :
    Option (PLift (PrivateExitAgreement target points fid type s t)) := do
  match t with
  | .returned _ _ =>
    let proof ← checkPrivateReturn type s t
    return ⟨by cases proof.down with | returned control typed boundary same => exact .returned control typed boundary same⟩
  | .next _ _ _ =>
    points.attach.findSome? (fun candidate => checkPrivateSuccessor target points candidate fid type s t)

def PrivateContextOpen (source : Source.Program) (ir : IR.Program) (target : IR.Function)
    (points : List (PrivateLayoutPoint target)) (incoming : IR.Memory) (fid : Nat) (type observationType : Source.IntType)
    (tokens : Bool) (context : Option SourceReturnContext) (owner : Nat) (stack : List IR.ReturnFrame) :=
  Transition.OpenCheckpoint (Source.step source) (IR.step ir) observeSource (observeTarget observationType)
    (fun s t => PrivateJointRelated target points incoming fid context owner stack s t ∨
      PrivateFunctionReturn incoming owner fid type tokens context stack s t)

abbrev PrivateContextContinuation (source : Source.Program) (ir : IR.Program) (target : IR.Function)
    (points : List (PrivateLayoutPoint target)) (fid : Nat)
    (tokens : Bool) (type : Source.IntType) :=
  (path : PathConditions inputWidths) → (cursor : SourceCursor inputWidths) →
    (label : String) → (pc : Nat) → (config : PrivateConfig inputWidths) →
    Option (PLift (∀ σ, path.Holds σ → ∀ values incoming context owner frame stack observationType,
      config.Realizes σ values incoming owner frame →
      PrivateContextOpen source ir target points incoming fid type observationType tokens context owner stack
        (cursor.denote σ fid context) (privateState σ values target label pc owner frame config stack)))

def privateContextFiniteSpan (source : Source.Program) (ir : IR.Program) (target : IR.Function)
    (points : List (PrivateLayoutPoint target)) (fid : Nat)
    (tokens : Bool) (type : Source.IntType) (budget : Nat) :
    PrivateContextContinuation source ir target points fid tokens type := fun path cursor label pc config => do
  let s ← sourceCutDiagram source (points.map (·.layout.site)) budget true cursor
  let t ← privateCutDiagram ir target tokens type (points.map (·.layout.label)) false budget true config label pc
  let same ← cutMatchUnder (fun _ a b => do
    let proof ← checkPrivateExit target points fid type a b
    return ⟨fun _ _ => proof.down⟩) budget path s.val.normalizeTrace t.val.normalizeTrace
  return ⟨by
    intro σ holds values incoming context owner frame stack observationType related
    obtain ⟨sf, sp, positiveSource, sourceOutcome⟩ := s.property σ fid context
    obtain ⟨tf, tp, positiveTarget, targetOutcome⟩ := t.property σ values incoming owner frame stack related
    have agreement := same.down σ holds
    rw [CutTree.normalize_trace_sound, CutTree.normalize_trace_sound] at agreement
    rcases agreement with ⟨fault, sourceFailed, targetFailed⟩ |
      ⟨se, te, sourceSuccess, targetSuccess, matched⟩
    · have sh : sf.control = .halted (.error fault) := by simpa [sourceFailed] using sourceOutcome
      have th : tf.control = .halted (.error fault) := by simpa [targetFailed] using targetOutcome
      obtain ⟨code, rfl, observed⟩ := s.val.fault_observation σ fault sourceFailed
      apply Transition.OpenCheckpoint.of_refines
      apply Transition.refines_of_terminals sp.toReaches tp.toReaches
        (by simp [Source.step, sh]) (by simp [IR.step, th]) (o := Observation.diagnostic code)
      · simpa [observeSource, sh] using observed
      · simpa [observeTarget, th] using observed
    · have sh : sf = se.denote σ fid context := by simpa [sourceSuccess] using sourceOutcome
      have result : te.Realizes σ incoming tokens target owner stack tf := by
        simpa [targetSuccess] using targetOutcome
      cases matched with
      | returned control typed boundary equal =>
        rename_i word targetWord outConfig
        obtain ⟨outValues, memory, afterFrame, outRelated, current, th⟩ := result
        have summary := private_return_storage σ outValues incoming memory owner afterFrame outConfig outRelated current
        obtain ⟨_, _, _, _, ownerBound, _⟩ := outRelated.2.2.2.2
        have exit : PrivateFunctionReturn incoming owner fid type tokens context stack sf tf := by
          refine ⟨word.denote σ, typed, se.locals.denote σ, memory.endFrame owner, afterFrame,
            sh.trans (returned_cursor σ fid context se word control boundary),
            summary.1, summary.2.1, summary.2.2.1, ownerBound, summary.2.2.2, ?_⟩
          simpa only [privateReturnValues, equal σ] using th
        exact Transition.OpenCheckpoint.exit (.inr exit) (positiveSource rfl) (positiveTarget rfl)
      | successor closed =>
        obtain ⟨outValues, afterFrame, outRelated, th⟩ := result
        exact Transition.OpenCheckpoint.exit
          (.inl (by simpa only [← sh, ← th] using closed σ outValues incoming context owner afterFrame stack outRelated))
          (positiveSource rfl) (positiveTarget rfl)⟩

end EvoIR.Core.Symbolic
