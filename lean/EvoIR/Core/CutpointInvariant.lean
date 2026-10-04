import EvoIR.Core.CutpointStorage
import EvoIR.Core.LoopCorrespondence

/-! Independent successor-invariant instantiation. Proposed associations are
checked against both actual symbolic exits. The target may omit dead locals,
but it may not invent a value, change the store shape or rename a source site. -/
namespace EvoIR.Core.Symbolic

def SourceSite.cursor {Γ : Var → Nat} (site : SourceSite) (locals : LocalStore Γ) : SourceCursor Γ :=
  match site with
  | .expression e k => ⟨.expression e, locals, k⟩
  | .statement s k => ⟨.statement s, locals, k⟩

theorem source_site_sound {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (cursor : SourceCursor Γ) (site : SourceSite) (locals : LocalStore Δ) (fid : Nat)
    (atSite : cursor.site = some site) (stored : cursor.locals.denote σ = locals.denote τ) :
    cursor.denote σ fid = (site.cursor locals).denote τ fid := by
  rcases cursor with ⟨control, values, k⟩
  cases control <;> simp only [SourceCursor.site] at atSite
  all_goals first | contradiction | skip
  all_goals cases Option.some.inj atSite
  all_goals simp only [SourceSite.cursor, SourceCursor.denote, CutControl.denote] at stored ⊢
  all_goals rw [stored]

structure CutpointLayout where
  site : SourceSite
  shape : List LocalShape
  label : String
  links : List LocalLink

structure LayoutPoint where
  layout : CutpointLayout
  scope : Scope inputWidths
  projected : projectLocals (canonicalStore layout.shape) layout.links = some scope

def layoutPoint (layout : CutpointLayout) : Option LayoutPoint :=
  match h : projectLocals (canonicalStore layout.shape) layout.links with
  | none => none
  | some scope => some ⟨layout, scope, h⟩

def LayoutPoint.joint (point : LayoutPoint) : JointPoint inputWidths :=
  ⟨point.layout.site.cursor (canonicalStore point.layout.shape), point.layout.label, point.scope⟩

def checkSuccessor (points : List LayoutPoint) (candidate : {p // p ∈ points})
    (fid : Nat) (target : IR.Function) (type : Source.IntType)
    (s : SourceCursor inputWidths) (t : TargetExit inputWidths) :
    Option (PLift (CutExitAgreement (points.map LayoutPoint.joint) fid target type s t)) :=
  match t with
  | .returned _ => none
  | .next label scope => do
    let point := candidate.val
    if atSite : s.site = some point.layout.site then
      if shape : s.locals.map LocalSlot.shape = point.layout.shape then
        if atLabel : label = point.layout.label then
          if projection : projectLocals s.locals point.layout.links = some scope then
            return ⟨.successor (by
              intro σ
              obtain ⟨τ, stored⟩ := canonical_store_of_shape σ s.locals point.layout.shape shape
              have source := source_site_sound σ τ s point.layout.site
                (canonicalStore point.layout.shape) fid atSite stored.symm
              have targetScope := project_locals_congr σ τ (fun _ => .world) (fun _ => .world)
                s.locals (canonicalStore point.layout.shape) point.layout.links scope point.scope
                projection point.projected stored.symm
              refine ⟨point.joint, List.mem_map.mpr ⟨point, candidate.property, rfl⟩, τ, source, ?_⟩
              simp only [TargetExit.state, LayoutPoint.joint, JointPoint.targetState, targetState]
              rw [atLabel, targetScope])⟩
          else none
        else none
      else none
    else none

def checkExit (points : List LayoutPoint) (fid : Nat) (target : IR.Function)
    (type : Source.IntType) (s : SourceCursor inputWidths) (t : TargetExit inputWidths) :
    Option (PLift (CutExitAgreement (points.map LayoutPoint.joint) fid target type s t)) :=
  match control : s.control, t with
  | .returned a, .returned b =>
    if same : a = b then
      if typed : a.type = type then
        some ⟨by subst b; exact .returned control typed⟩
      else none
    else none
  | _, out => points.attach.findSome? (fun candidate => checkSuccessor points candidate fid target type s out)

def checkedSpan (points : List LayoutPoint) (fid : Nat) (target : IR.Function)
    (type : Source.IntType) (budget : Nat) (point : JointPoint inputWidths) :
    Option (CheckedSpan (points.map LayoutPoint.joint) fid target type point) := do
  let source ← sourceCutDiagram (points.map (·.layout.site)) budget true point.source
  let targetTree ← targetCutDiagram (fun _ => .world) target type (points.map (·.layout.label))
    (target.blocks.length+1) true point.scope point.label
  let same ← cutMatch (checkExit points fid target type)
    source.val.normalizeBranches targetTree.val.normalizeBranches
  return ⟨source.val, targetTree.val, source.property, targetTree.property, same.down⟩

inductive SpanList (points : List LayoutPoint) (fid : Nat) (target : IR.Function)
    (type : Source.IntType) : List (JointPoint inputWidths) → Type where
  | nil : SpanList points fid target type []
  | cons {point rest} : CheckedSpan (points.map LayoutPoint.joint) fid target type point →
      SpanList points fid target type rest → SpanList points fid target type (point :: rest)

def checkedSpans (points : List LayoutPoint) (fid : Nat) (target : IR.Function)
    (type : Source.IntType) (budget : Nat) :
    (remaining : List (JointPoint inputWidths)) → Option (SpanList points fid target type remaining)
  | [] => some .nil
  | point :: rest => do
    let head ← checkedSpan points fid target type budget point
    let tail ← checkedSpans points fid target type budget rest
    return .cons head tail

/-- Only the proof uses classical equality to select the already checked
member. Executable admission constructs every entry, not a sampled subset. -/
noncomputable def SpanList.get {points : List LayoutPoint} {fid : Nat} {target : IR.Function}
    {type : Source.IntType} {remaining : List (JointPoint inputWidths)}
    (checked : SpanList points fid target type remaining) (point : JointPoint inputWidths)
    (member : point ∈ remaining) : CheckedSpan (points.map LayoutPoint.joint) fid target type point := by
  classical
  induction checked with
  | nil => simp at member
  | @cons first rest head tail ih =>
    if same : point = first then exact same ▸ head
    else exact ih ((List.mem_cons.mp member).resolve_left same)

theorem checked_table_refines (points : List LayoutPoint) (fid : Nat) (type : Source.IntType)
    (source : Source.Program) (program : IR.Program) (target : IR.Function)
    (found : program.functions.find? (·.name == target.name) = some target)
    (returns : target.returns = [.bits type.width])
    (table : SpanList points fid target type (points.map LayoutPoint.joint))
    (s : Source.State) (t : IR.State)
    (entry : CutpointRelated (points.map LayoutPoint.joint) fid target s t) :
    LoweringRefines source program type s t :=
  checked_cutpoint_refines (points.map LayoutPoint.joint) fid type source program target found
    returns (fun point member => table.get point member) s t entry

end EvoIR.Core.Symbolic
