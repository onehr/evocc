import EvoIR.IRPoisonCheck

/-! Full-vector SSA jumps, including dead poisoned arguments and cycles.
Reads are performed in the old bank; the actual bind loop installs the new
bank only afterwards. No edge component is dropped or sampled. -/
namespace EvoIR.Core.IR.Poison

theorem edge_reads_related (needed : List Nat) (parameters : List Binding) (ids : List Nat)
    (checked : edgeDemand needed parameters ids = true) {source target : Bindings}
    (values : BanksRelated needed source target) :
    ExceptRelated (ArgumentsRelated needed parameters)
      (ids.mapM (IR.lookup source)) (ids.mapM (IR.lookup target)) := by
  induction parameters generalizing ids with
  | nil =>
    cases ids with
    | nil => exact .nil
    | cons _ _ => simp [edgeDemand] at checked
  | cons parameter parameters ih =>
    cases ids with
    | nil => simp [edgeDemand] at checked
    | cons id ids =>
      have data : (!needed.contains parameter.id || needed.contains id) = true ∧
          edgeDemand needed parameters ids = true := by
        simpa only [edgeDemand, Bool.and_eq_true] using checked
      simp only [List.mapM_cons]
      apply except_bind_related (values.read id)
      intro before after related
      apply except_bind_related (ih ids data.2)
      intro sourceArgs targetArgs args
      apply ArgumentsRelated.cons
      · refine ⟨related.1, ?_⟩
        intro demanded
        have : id ∈ needed := by
          simpa [List.contains_iff_mem, demanded] using data.1
        exact related.2 this
      · exact args

structure ActivationsRelated (plan : DemandPlan) (source target : Activation) : Prop where
  function : source.function = target.function
  block : source.block = target.block
  pc : source.pc = target.pc
  owner : source.owner = target.owner
  values : BanksRelated (neededFor plan source.function) source.values target.values

theorem ActivationsRelated.refl (plan : DemandPlan) (activation : Activation) :
    ActivationsRelated plan activation activation :=
  ⟨rfl, rfl, rfl, rfl, BanksRelated.refl _ _⟩

theorem jump_related (plan : DemandPlan) (f : Function) (edge : Edge)
    (checked : edgeValid (neededFor plan f.name) f edge = true)
    (source target : Activation) (related : ActivationsRelated plan source target)
    (functionName : source.function = f.name) :
    ExceptRelated (ActivationsRelated plan) (jump f source edge)
      (jump (rewriteFunction plan f) target edge) := by
  cases found : f.blocks.find? (·.name == edge.block) with
  | none => simp [edgeValid, found] at checked
  | some block =>
    have demand : edgeDemand (neededFor plan f.name) block.parameters edge.arguments = true := by
      simpa only [edgeValid, found] using checked
    have bank : BanksRelated (neededFor plan f.name) source.values target.values :=
      functionName ▸ related.values
    simp only [jump, found, find_block, Option.map_some, rewrite_block_parameters,
      Except.bind, Bind.bind]
    apply except_bind_related (edge_reads_related _ _ _ demand bank)
    intro before after args
    apply except_bind_related args.bind
    intro sourceBank targetBank banks
    exact ⟨related.function, rfl, rfl, related.owner, functionName.symm ▸ banks⟩

theorem enter_rewrite (plan : DemandPlan) (f : Function) (args : List Value) (owner : Nat) :
    enter (rewriteFunction plan f) args owner = enter f args owner := by
  cases found : f.blocks.head? <;>
    simp only [enter, rewriteFunction, List.head?_map, found, Option.map,
      rewriteBlock] <;> rfl

theorem initial_rewrite (plan : DemandPlan) (source : Program) (entry : String) (args : List Value) :
    initial (rewriteProgram plan source) entry args = initial source entry args := by
  simp only [initial, find_function]
  cases source.functions.find? (·.name == entry) <;>
    simp [enter_rewrite, Bind.bind, Except.bind, Pure.pure, Except.pure]

end EvoIR.Core.IR.Poison
