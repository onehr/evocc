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
  tokens : List (Nat × Nat) := []

/-- Token links select only the admitted empty-memory/world values. Actual
edge binding and exact scope equality must justify each proposed token. -/
def CutpointLayout.project {Γ : Var → Nat} (layout : CutpointLayout)
    (locals : LocalStore Γ) : Option (Scope Γ) := do
  let words ← projectLocals locals layout.links
  if layout.tokens.any (fun token => token.2 > 1) then none
  else some (layout.tokens.map (fun (id, token) => (id, Slot.opaque token)) ++ words)

theorem project_cutpoint_congr {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (layout : CutpointLayout) (a : LocalStore Γ) (b : LocalStore Δ)
    (first : Scope Γ) (second : Scope Δ)
    (left : layout.project a = some first) (right : layout.project b = some second)
    (same : a.denote σ = b.denote τ) :
    first.denote σ callTokens = second.denote τ callTokens := by
  unfold CutpointLayout.project at left right
  cases ha : projectLocals a layout.links with
  | none => simp [ha] at left
  | some ascope =>
    cases hb : projectLocals b layout.links with
    | none => simp [hb] at right
    | some bscope =>
      simp only [ha, hb, Bind.bind, Option.bind] at left right
      split at left
      · contradiction
      · rename_i admitted
        simp only [admitted] at right
        cases Option.some.inj left
        cases right
        have words := project_locals_congr σ τ callTokens callTokens a b layout.links
          ascope bscope ha hb same
        simp only [Scope.denote, List.map_append, List.map_map, Function.comp_def, Slot.denote]
        exact congrArg (List.append _) words

structure LayoutPoint where
  layout : CutpointLayout
  scope : Scope inputWidths
  projected : layout.project (canonicalStore layout.shape) = some scope

def layoutPoint (layout : CutpointLayout) : Option LayoutPoint :=
  match h : layout.project (canonicalStore layout.shape) with
  | none => none
  | some scope => some ⟨layout, scope, h⟩

def LayoutPoint.joint (point : LayoutPoint) : JointPoint inputWidths :=
  ⟨point.layout.site.cursor (canonicalStore point.layout.shape), point.layout.label, point.scope⟩

def checkSuccessor (points : List LayoutPoint) (candidate : {p // p ∈ points})
    (fid : Nat) (target : IR.Function) (tokens : Bool) (type : Source.IntType)
    (s : SourceCursor inputWidths) (t : TargetExit inputWidths) :
    Option (PLift (CutExitAgreement (points.map LayoutPoint.joint) fid target tokens type s t)) :=
  match t with
  | .returned _ => none
  | .next label scope => do
    let point := candidate.val
    if atSite : s.site = some point.layout.site then
      if shape : s.locals.map LocalSlot.shape = point.layout.shape then
        if atLabel : label = point.layout.label then
          if projection : point.layout.project s.locals = some scope then
            return ⟨.successor (by
              intro σ nextFrame
              obtain ⟨τ, stored⟩ := canonical_store_of_shape σ s.locals point.layout.shape shape
              have source := source_site_sound σ τ s point.layout.site
                (canonicalStore point.layout.shape) fid atSite stored.symm
              have targetScope := project_cutpoint_congr σ τ point.layout
                s.locals (canonicalStore point.layout.shape) scope point.scope
                projection point.projected stored.symm
              refine ⟨point.joint, List.mem_map.mpr ⟨point, candidate.property, rfl⟩, τ, nextFrame, source, ?_⟩
              simp only [TargetExit.callState, LayoutPoint.joint, JointPoint.targetState, targetState]
              rw [atLabel, targetScope])⟩
          else none
        else none
      else none
    else none

def checkExit (points : List LayoutPoint) (fid : Nat) (target : IR.Function)
    (tokens : Bool) (type : Source.IntType) (s : SourceCursor inputWidths) (t : TargetExit inputWidths) :
    Option (PLift (CutExitAgreement (points.map LayoutPoint.joint) fid target tokens type s t)) :=
  match control : s.control, t with
  | .returned a, .returned b =>
    if same : a = b then
      if typed : a.type = type then
        some ⟨by subst b; exact .returned control typed⟩
      else none
    else none
  | _, out => points.attach.findSome? (fun candidate => checkSuccessor points candidate fid target tokens type s out)

def checkedSpan (program : Source.Program) (ir : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (target : IR.Function) (tokens : Bool)
    (type : Source.IntType) (budget : Nat) (point : JointPoint inputWidths) :
    Option (CheckedSpan program ir (points.map LayoutPoint.joint) fid target tokens type point) := do
  let source ← sourceCutDiagram program (points.map (·.layout.site)) budget true point.source
  let targetTree ← callingCutDiagram ir target tokens type (points.map (·.layout.label))
    budget true point.scope point.label
  let same ← cutMatch (checkExit points fid target tokens type)
    source.val.normalizeTrace targetTree.val.normalizeTrace
  return ⟨source.val, targetTree.val, source.property, targetTree.property, same.down⟩

inductive SpanList (program : Source.Program) (ir : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (target : IR.Function) (tokens : Bool) (type : Source.IntType) : List (JointPoint inputWidths) → Type where
  | nil : SpanList program ir points fid target tokens type []
  | cons {point rest} : CheckedSpan program ir (points.map LayoutPoint.joint) fid target tokens type point →
      SpanList program ir points fid target tokens type rest → SpanList program ir points fid target tokens type (point :: rest)

def checkedSpans (program : Source.Program) (ir : IR.Program) (points : List LayoutPoint)
    (fid : Nat) (target : IR.Function) (tokens : Bool)
    (type : Source.IntType) (budget : Nat) :
    (remaining : List (JointPoint inputWidths)) → Option (SpanList program ir points fid target tokens type remaining)
  | [] => some .nil
  | point :: rest => do
    let head ← checkedSpan program ir points fid target tokens type budget point
    let tail ← checkedSpans program ir points fid target tokens type budget rest
    return .cons head tail

/-- Only the proof uses classical equality to select the already checked
member. Executable admission constructs every entry, not a sampled subset. -/
noncomputable def SpanList.get {program : Source.Program} {ir : IR.Program} {points : List LayoutPoint}
    {fid : Nat} {target : IR.Function} {tokens : Bool}
    {type : Source.IntType} {remaining : List (JointPoint inputWidths)}
    (checked : SpanList program ir points fid target tokens type remaining) (point : JointPoint inputWidths)
    (member : point ∈ remaining) : CheckedSpan program ir (points.map LayoutPoint.joint) fid target tokens type point := by
  classical
  induction checked with
  | nil => simp at member
  | @cons first rest head tail ih =>
    if same : point = first then exact same ▸ head
    else exact ih ((List.mem_cons.mp member).resolve_left same)

theorem checked_table_refines (points : List LayoutPoint) (fid : Nat) (type : Source.IntType)
    (source : Source.Program) (program : IR.Program) (target : IR.Function)
    (tokens : Bool)
    (table : SpanList source program points fid target tokens type (points.map LayoutPoint.joint))
    (s : Source.State) (t : IR.State)
    (entry : CutpointRelated (points.map LayoutPoint.joint) fid target s t) :
    LoweringRefines source program type s t :=
  checked_cutpoint_refines (points.map LayoutPoint.joint) fid type source program target tokens
    (fun point member => table.get point member) s t entry

end EvoIR.Core.Symbolic
