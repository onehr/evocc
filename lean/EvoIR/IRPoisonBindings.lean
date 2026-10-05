import EvoIR.Core.IRExecution

/-! Register-bank facts for checked dead-poison replacement. Every binding
keeps its identity and type. Only explicitly demanded identities must keep
their value; in particular this relation never asserts poison equals zero. -/
namespace EvoIR.Core.IR.Poison

def ExceptRelated {α β : Type} (relation : α → β → Prop) :
    Except Source.Fault α → Except Source.Fault β → Prop
  | .error before, .error after => before = after
  | .ok before, .ok after => relation before after
  | _, _ => False

theorem except_bind_related {α β γ δ : Type} {relation : α → β → Prop}
    {next : γ → δ → Prop} {before : Except Source.Fault α} {after : Except Source.Fault β}
    {f : α → Except Source.Fault γ} {g : β → Except Source.Fault δ}
    (related : ExceptRelated relation before after)
    (advance : ∀ a b, relation a b → ExceptRelated next (f a) (g b)) :
    ExceptRelated next (before.bind f) (after.bind g) := by
  cases before with
  | error fault =>
    cases after with
    | error other => exact related
    | ok _ => exact False.elim related
  | ok value =>
    cases after with
    | error _ => exact False.elim related
    | ok other => exact advance value other related

def ValueRelated (needed : List Nat) (id : Nat) (before after : Value) : Prop :=
  before.type = after.type ∧ (id ∈ needed → before = after)

inductive BanksRelated (needed : List Nat) : Bindings → Bindings → Prop where
  | nil : BanksRelated needed [] []
  | cons {id before after source target} : ValueRelated needed id before after →
      BanksRelated needed source target →
      BanksRelated needed ((id, before) :: source) ((id, after) :: target)

theorem BanksRelated.refl (needed : List Nat) (values : Bindings) :
    BanksRelated needed values values := by
  induction values with
  | nil => exact .nil
  | cons pair rest ih => exact .cons ⟨rfl, fun _ => rfl⟩ ih

theorem BanksRelated.append {needed : List Nat} {source target sourceTail targetTail : Bindings}
    (head : BanksRelated needed source target) (tail : BanksRelated needed sourceTail targetTail) :
    BanksRelated needed (source ++ sourceTail) (target ++ targetTail) := by
  induction head with
  | nil => exact tail
  | cons values _ ih => exact .cons values ih

def OptionsRelated (needed : List Nat) (id : Nat) : Option Value → Option Value → Prop
  | none, none => True
  | some before, some after => ValueRelated needed id before after
  | _, _ => False

theorem BanksRelated.lookup {needed : List Nat} {source target : Bindings}
    (related : BanksRelated needed source target) (id : Nat) :
    OptionsRelated needed id (source.lookup id) (target.lookup id) := by
  induction related with
  | nil => trivial
  | @cons key before after source target values _ ih =>
    by_cases same : key = id
    · subst key
      simpa [List.lookup, OptionsRelated] using values
    · have test : (id == key) = false := by simp [Ne.symm same]
      simpa only [List.lookup, test] using ih

theorem BanksRelated.present {needed : List Nat} {source target : Bindings}
    (related : BanksRelated needed source target) (id : Nat) :
    (source.lookup id).isSome = (target.lookup id).isSome := by
  have lookup := related.lookup id
  cases before : source.lookup id <;> cases after : target.lookup id <;>
    simp [OptionsRelated, before, after] at lookup ⊢

theorem BanksRelated.read {needed : List Nat} {source target : Bindings}
    (related : BanksRelated needed source target) (id : Nat) :
    ExceptRelated (ValueRelated needed id) (IR.lookup source id) (IR.lookup target id) := by
  have lookup := related.lookup id
  cases before : source.lookup id <;> cases after : target.lookup id <;>
    simp only [IR.lookup, before, after, OptionsRelated, ExceptRelated] at lookup ⊢ <;> trivial

theorem BanksRelated.needed_read {needed : List Nat} {source target : Bindings}
    (related : BanksRelated needed source target) (id : Nat) (demanded : id ∈ needed) :
    IR.lookup source id = IR.lookup target id := by
  have read := related.read id
  cases before : IR.lookup source id with
  | error fault =>
    cases after : IR.lookup target id with
    | error other =>
      simp only [before, after, ExceptRelated] at read
      exact congrArg Except.error read
    | ok _ => simp [ExceptRelated, before, after] at read
  | ok value =>
    cases after : IR.lookup target id with
    | error _ => simp [ExceptRelated, before, after] at read
    | ok other =>
      simp only [before, after, ExceptRelated] at read
      exact congrArg Except.ok (read.2 demanded)

theorem BanksRelated.reads {needed : List Nat} {source target : Bindings}
    (related : BanksRelated needed source target) (ids : List Nat)
    (demanded : ∀ id ∈ ids, id ∈ needed) :
    ids.mapM (IR.lookup source) = ids.mapM (IR.lookup target) := by
  induction ids with
  | nil => rfl
  | cons id ids ih =>
    rw [List.mapM_cons, List.mapM_cons, related.needed_read id (demanded id (by simp))]
    rw [ih (fun key member => demanded key (by simp [member]))]

theorem BanksRelated.append_same {needed : List Nat} {source target : Bindings}
    (related : BanksRelated needed source target) (results : List Binding) (values : List Value) :
    ExceptRelated (BanksRelated needed) (appendResults source results values)
      (appendResults target results values) := by
  unfold appendResults
  apply except_bind_related (relation := Eq)
  · cases bind results values <;> simp [ExceptRelated]
  · intro fresh same eq
    subst same
    have checks : fresh.any (fun (id, _) => (source.lookup id).isSome) =
        fresh.any (fun (id, _) => (target.lookup id).isSome) := by
      apply congrArg (fun predicate => fresh.any predicate)
      funext pair
      exact related.present pair.1
    rw [checks]
    split
    · rfl
    · exact (BanksRelated.refl needed fresh).append related

inductive ArgumentsRelated (needed : List Nat) : List Binding → List Value → List Value → Prop where
  | nil : ArgumentsRelated needed [] [] []
  | cons {parameter parameters before after source target} :
      ValueRelated needed parameter.id before after →
      ArgumentsRelated needed parameters source target →
      ArgumentsRelated needed (parameter :: parameters) (before :: source) (after :: target)

private def bindingStep (pair : Binding × Value) (values : Bindings) :
    Except Source.Fault (ForInStep Bindings) := do
  let (parameter, value) := pair
  if parameter.type != value.type then throw (.malformed "IR binding type")
  if values.any (fun entry => entry.1 == parameter.id) then throw (.malformed "IR duplicate binding")
  return .yield ((parameter.id, value) :: values)

theorem BanksRelated.any_id {needed : List Nat} {source target : Bindings}
    (related : BanksRelated needed source target) (id : Nat) :
    source.any (fun entry => entry.1 == id) = target.any (fun entry => entry.1 == id) := by
  induction related with
  | nil => rfl
  | cons _ _ ih => simpa only [List.any_cons] using congrArg (fun value => _ || value) ih

private theorem binding_loop_related {needed : List Nat} {parameters : List Binding}
    {sourceArgs targetArgs : List Value} (args : ArgumentsRelated needed parameters sourceArgs targetArgs)
    {source target : Bindings} (values : BanksRelated needed source target) :
    ExceptRelated (BanksRelated needed)
      (forIn (parameters.zip sourceArgs) source bindingStep)
      (forIn (parameters.zip targetArgs) target bindingStep) := by
  induction args generalizing source target with
  | nil => exact values
  | @cons parameter parameters before after sourceArgs targetArgs head tail ih =>
    simp only [List.zip_cons_cons, List.forIn_cons, bindingStep, head.1,
      values.any_id parameter.id]
    split
    · rfl
    · split
      · rfl
      · simp only [Bind.bind, Except.bind, Pure.pure, Except.pure]
        exact ih (.cons head values)

theorem ArgumentsRelated.bind {needed : List Nat} {parameters : List Binding}
    {sourceArgs targetArgs : List Value} (args : ArgumentsRelated needed parameters sourceArgs targetArgs) :
    ExceptRelated (BanksRelated needed) (IR.bind parameters sourceArgs) (IR.bind parameters targetArgs) := by
  have lengths : parameters.length = sourceArgs.length ∧ parameters.length = targetArgs.length := by
    induction args with
    | nil => exact ⟨rfl, rfl⟩
    | cons _ _ ih => exact ⟨congrArg Nat.succ ih.1, congrArg Nat.succ ih.2⟩
  simp only [IR.bind, ← lengths.1, ← lengths.2, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
  change ExceptRelated (BanksRelated needed)
    ((forIn (parameters.zip sourceArgs) [] bindingStep).bind Except.ok)
    ((forIn (parameters.zip targetArgs) [] bindingStep).bind Except.ok)
  exact except_bind_related (binding_loop_related args .nil) (fun _ _ related => related)

theorem BanksRelated.conflicts {needed : List Nat} {source target freshBefore freshAfter : Bindings}
    (values : BanksRelated needed source target) (fresh : BanksRelated needed freshBefore freshAfter) :
    freshBefore.any (fun pair => (source.lookup pair.1).isSome) =
      freshAfter.any (fun pair => (target.lookup pair.1).isSome) := by
  induction fresh with
  | nil => rfl
  | @cons id _ _ _ _ _ _ ih =>
    simp only [List.any_cons, values.present id, ih]

theorem BanksRelated.append_related {needed : List Nat} {source target : Bindings}
    (values : BanksRelated needed source target) {results : List Binding} {before after : List Value}
    (args : ArgumentsRelated needed results before after) :
    ExceptRelated (BanksRelated needed) (appendResults source results before)
      (appendResults target results after) := by
  unfold appendResults
  apply except_bind_related args.bind
  intro freshBefore freshAfter fresh
  rw [values.conflicts fresh]
  split
  · rfl
  · exact fresh.append values

end EvoIR.Core.IR.Poison
