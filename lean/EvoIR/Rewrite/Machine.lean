import EvoIR.Rewrite.Check
import EvoIR.Rewrite.ReplaySound

/-!
Facts about the EvoIR machine of `Core/IRExecution.lean` that the rewrite
proof needs. `ActOK` is the invariant of one activation of the program before
the rewrite: each pure instruction that already ran in the block gave the
value that its binding now holds, and each block parameter holds a value that
satisfies its table fact.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR.Rewrite

open EvoIR.Core.IR

def Extends (vals vals' : Bindings) : Prop := ∀ id v, vals.lookup id = some v → vals'.lookup id = some v

inductive Pairs {α β : Type} (P : α → β → Prop) : List α → List β → Prop
  | nil : Pairs P [] []
  | cons {x : α} {y : β} {xs : List α} {ys : List β} : P x y → Pairs P xs ys → Pairs P (x :: xs) (y :: ys)

theorem Pairs.imp {α β : Type} {P Q : α → β → Prop} (f : ∀ {x y}, P x y → Q x y) :
    ∀ {xs : List α} {ys : List β}, Pairs P xs ys → Pairs Q xs ys
  | _, _, .nil => .nil
  | _, _, .cons h t => .cons (f h) (Pairs.imp f t)

abbrev Bound (vals : Bindings) (ids : List Nat) (args : List Value) : Prop :=
  Pairs (fun id v => vals.lookup id = some v) ids args

theorem mapM_lookup {vals : Bindings} :
    ∀ {ids : List Nat} {args : List Value}, ids.mapM (lookup vals) = .ok args ↔ Bound vals ids args
  | [], args => by
    cases args with
    | nil => exact ⟨fun _ => .nil, fun _ => rfl⟩
    | cons => exact ⟨fun h => by simp [Pure.pure, Except.pure] at h, fun h => by cases h⟩
  | id :: ids, args => by
    simp only [List.mapM_cons, lookup]
    cases h : vals.lookup id with
    | none =>
      refine ⟨fun h' => by simp [Bind.bind, Except.bind] at h', fun hf => ?_⟩
      cases hf with
      | cons hv _ => rw [h] at hv; cases hv
    | some v =>
      cases t : ids.mapM (lookup vals) with
      | error e =>
        simp only [Bind.bind, Except.bind, t]
        constructor
        · intro h'; cases h'
        · intro hf
          cases hf with
          | cons _ rest => have := (mapM_lookup (ids := ids)).mpr rest; rw [t] at this; cases this
      | ok rest =>
        simp only [Bind.bind, Except.bind, t, Pure.pure, Except.pure, Except.ok.injEq]
        have ih := (mapM_lookup (ids := ids) (args := rest)).mp t
        constructor
        · intro e; subst e; exact .cons h ih
        · intro hf
          cases hf with
          | cons hv rest' =>
            have := (mapM_lookup (ids := ids)).mpr rest'
            rw [t] at this
            cases this
            cases hv.symm.trans h
            rfl

theorem bound_extends {vals vals' : Bindings} (e : Extends vals vals') {ids : List Nat}
    {args : List Value} (h : Bound vals ids args) : Bound vals' ids args :=
  h.imp fun hv => e _ _ hv

theorem lookup_append (l l' : Bindings) (id : Nat) :
    (l ++ l').lookup id = (l.lookup id).or (l'.lookup id) := by
  induction l with
  | nil => rfl
  | cons x xs ih =>
    obtain ⟨k, v⟩ := x
    show (match id == k with | true => some v | false => (xs ++ l').lookup id) =
      (match id == k with | true => some v | false => xs.lookup id).or (l'.lookup id)
    cases id == k
    · exact ih
    · rfl

theorem pairs_single {α β : Type} {P : α → β → Prop} {xs : List α} {y : β}
    (h : Pairs P xs [y]) : ∃ x, xs = [x] ∧ P x y := by
  cases h with
  | cons hp t => cases t; exact ⟨_, rfl, hp⟩

theorem lookup_key {l : Bindings} {id : Nat} {v : Value} (h : l.lookup id = some v) :
    l.any (fun e => e.1 == id) = true := by
  induction l with
  | nil => simp at h
  | cons x xs ih =>
    obtain ⟨k, w⟩ := x
    simp only [List.lookup] at h
    simp only [List.any_cons, Bool.or_eq_true]
    split at h
    · rename_i hk
      left
      simp only [beq_iff_eq] at hk ⊢
      exact hk.symm
    · exact .inr (ih h)

/-! ## Binding -/

private def bindingStep (pair : Binding × Value) (values : Bindings) :
    Except EvoIR.Core.Source.Fault (ForInStep Bindings) := do
  let (p, v) := pair
  if p.type != v.type then throw (.malformed "IR binding type")
  if values.any (fun entry => entry.1 == p.id) then throw (.malformed "IR duplicate binding")
  return .yield ((p.id, v) :: values)

private theorem bindLoop : ∀ (l : List (Binding × Value)) (acc out : Bindings),
    forIn l acc bindingStep = .ok out →
      Extends acc out ∧ ∀ e ∈ l, out.lookup e.1.id = some e.2
  | [], acc, out, h => by cases h; exact ⟨fun _ _ h => h, by simp⟩
  | (p, v) :: l, acc, out, h => by
    simp only [List.forIn_cons, bindingStep] at h
    split at h
    · simp [Bind.bind, Except.bind] at h
    · split at h
      · simp [Bind.bind, Except.bind] at h
      · rename_i hd
        simp only [Bind.bind, Except.bind, Pure.pure, Except.pure] at h
        obtain ⟨e, rest⟩ := bindLoop l _ out h
        refine ⟨fun id w hw => e id w ?_, ?_⟩
        · simp only [List.lookup]
          split
          · rename_i hid
            exfalso
            simp only [beq_iff_eq] at hid
            subst hid
            have := lookup_key hw
            simp only [List.any_eq_true, beq_iff_eq] at this hd
            exact hd this
          · exact hw
        · intro x hx
          rcases List.mem_cons.mp hx with rfl | hx
          · exact e _ _ (by simp [List.lookup])
          · exact rest x hx

theorem forall2_of_zip {α β : Type} {P : α → β → Prop} :
    ∀ {xs : List α} {ys : List β}, xs.length = ys.length → (∀ e ∈ xs.zip ys, P e.1 e.2) →
      Pairs P xs ys
  | [], [], _, _ => .nil
  | x :: xs, y :: ys, hl, h =>
    .cons (h (x, y) List.mem_cons_self)
      (forall2_of_zip (by simpa using hl) fun e he => h e (List.mem_cons_of_mem _ he))

theorem bind_ok {ps : List Binding} {args : List Value} {vals : Bindings}
    (h : EvoIR.Core.IR.bind ps args = .ok vals) : Pairs (fun p v => vals.lookup p.id = some v) ps args := by
  simp only [EvoIR.Core.IR.bind] at h
  split at h
  · simp [Bind.bind, Except.bind] at h
  · rename_i hl
    have hl : ps.length = args.length := by simpa using hl
    change Except.bind (forIn (ps.zip args) [] bindingStep) (fun r => Except.ok r) = _ at h
    cases hf : forIn (ps.zip args) [] bindingStep with
    | error e => simp [hf, Except.bind] at h
    | ok out =>
      simp only [hf, Except.bind, Except.ok.injEq] at h
      subst h
      exact forall2_of_zip hl (bindLoop _ _ _ hf).2

theorem appendResults_ok {vals vals' : Bindings} {rs : List Binding} {args : List Value}
    (h : appendResults vals rs args = .ok vals') :
    Extends vals vals' ∧ Pairs (fun r v => vals'.lookup r.id = some v) rs args := by
  unfold appendResults at h
  cases hb : EvoIR.Core.IR.bind rs args with
  | error e => rw [hb] at h; cases h
  | ok fresh =>
    rw [hb] at h
    change (if fresh.any (fun (id, _) => (vals.lookup id).isSome) then
      (Except.error (.malformed "IR result already bound") : Except EvoIR.Core.Source.Fault Bindings)
      else .ok (fresh ++ vals)) = .ok vals' at h
    split at h
    · cases h
    · rename_i hfresh
      cases h
      refine ⟨fun id v hv => ?_, (bind_ok hb).imp fun hv => by simp [lookup_append, hv]⟩
      rw [lookup_append]
      cases hl : fresh.lookup id with
      | none => simpa using hv
      | some u =>
        exfalso
        have hk := lookup_key hl
        simp only [List.any_eq_true, beq_iff_eq] at hk
        obtain ⟨⟨k, w⟩, hm, rfl⟩ := hk
        apply hfresh
        simp only [List.any_eq_true]
        exact ⟨(k, w), hm, by simp [hv]⟩

/-! ## The activation invariant -/

def Ran (vals : Bindings) (owner : Nat) (inst : Instruction) : Prop :=
  isPure inst.op = true → ∃ args v r, Bound vals inst.operands args ∧
    evalOperation owner inst.op args = .ok [v] ∧ inst.results = [r] ∧ vals.lookup r.id = some v

def ActOK (p : Program) (T : Table) (a : Activation) : Prop :=
  ∀ f b, p.functions.find? (·.name == a.function) = some f → f.blocks.find? (·.name == a.block) = some b →
    (∀ inst ∈ b.instructions.take a.pc, Ran a.values a.owner inst) ∧
    ∀ k prm, b.parameters[k]? = some prm →
      ∃ v, a.values.lookup prm.id = some v ∧ Holds (T.param a.function a.block k) v

def StateOK (p : Program) (T : Table) (s : State) : Prop :=
  (∀ a, s.control = .running a → ActOK p T a) ∧ ∀ fr ∈ s.stack, ActOK p T fr.caller

theorem ran_extends {vals vals' : Bindings} (e : Extends vals vals') {o : Nat} {inst : Instruction}
    (h : Ran vals o inst) : Ran vals' o inst := by
  intro hp
  obtain ⟨args, v, r, hb, he, hr, hl⟩ := h hp
  exact ⟨args, v, r, bound_extends e hb, he, hr, e _ _ hl⟩

theorem actOK_extends {p : Program} {T : Table} {a : Activation} {vals' : Bindings}
    (h : ActOK p T a) (e : Extends a.values vals') : ActOK p T {a with values := vals'} := by
  intro f b hf hb
  obtain ⟨ran, fi⟩ := h f b hf hb
  refine ⟨fun inst hi => ran_extends e (ran inst hi), fun k prm hk => ?_⟩
  obtain ⟨v, hv, hh⟩ := fi k prm hk
  exact ⟨v, e _ _ hv, hh⟩

theorem pure_result {o : Nat} {op : Operation} {args rs : List Value} (hp : isPure op = true)
    (h : evalOperation o op args = .ok rs) : ∃ v, rs = [v] := by
  cases op with
  | constant w x => exact ⟨_, (evalOperation_constant h).2⟩
  | scalar s => obtain ⟨r, -, e⟩ := evalOperation_scalar h; exact ⟨r, e⟩
  | _ => simp [isPure] at hp

/-- One non-call instruction keeps the invariant. -/
theorem actOK_instruction {p : Program} {T : Table} {a : Activation} {f : Function} {b : Block}
    {inst : Instruction} {args results : List Value} {vals' : Bindings}
    (h : ActOK p T a) (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.name == a.block) = some b) (hi : b.instructions[a.pc]? = some inst)
    (hargs : inst.operands.mapM (lookup a.values) = .ok args)
    (he : evalOperation a.owner inst.op args = .ok results)
    (hv : appendResults a.values inst.results results = .ok vals') :
    ActOK p T {a with pc := a.pc + 1, values := vals'} := by
  obtain ⟨e, fresh⟩ := appendResults_ok hv
  have h' := actOK_extends h e
  intro f' b' hf' hb'
  simp only [hf, Option.some.injEq] at hf'
  subst hf'
  simp only [hb, Option.some.injEq] at hb'
  subst hb'
  obtain ⟨ran, fi⟩ := h' f b hf hb
  refine ⟨fun x hx => ?_, fi⟩
  rw [List.take_add_one, hi, Option.toList_some, List.mem_append, List.mem_singleton] at hx
  rcases hx with hx | rfl
  · exact ran x hx
  · intro hp
    obtain ⟨v, rfl⟩ := pure_result hp he
    obtain ⟨r, hr, hl⟩ := pairs_single fresh
    exact ⟨args, v, r, bound_extends e (mapM_lookup.mp hargs), he, hr, hl⟩

end EvoIR.Rewrite
