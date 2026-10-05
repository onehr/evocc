import EvoIR.Rewrite.Region

/-!
Every state that the program before the rewrite reaches satisfies `StateOK`,
when the block-argument table passes `tableOk`. The edge check of the table
gives the fact of each block parameter after a jump.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR.Rewrite

open EvoIR.Core.IR

def EnvOK (env : List (Nat × RFact)) (vals : Bindings) : Prop :=
  ∀ id af, env.lookup id = some af → ∀ v, vals.lookup id = some v → Holds af v

theorem holds_mask {a : RFact} (h : a.mask = 0) (v : Value) : Holds a v := by
  obtain ⟨w, m, x⟩ := a
  cases h
  cases v with
  | bits n y => intro _; exact gamma_mask_zero _ y
  | _ => trivial

theorem lookup_mem' {α β : Type} [BEq α] [LawfulBEq α] {l : List (α × β)} {a : α} {b : β}
    (h : l.lookup a = some b) : (a, b) ∈ l := by
  induction l with
  | nil => simp at h
  | cons x xs ih =>
    obtain ⟨k, v⟩ := x
    simp only [List.lookup] at h
    split at h
    · rename_i hk; cases h; rw [show a = k by simpa using hk]; exact List.mem_cons_self
    · exact List.mem_cons_of_mem _ (ih h)

theorem pairs_get' {α β : Type} {P : α → β → Prop} :
    ∀ {xs : List α} {ys : List β}, Pairs P xs ys → ∀ (i : Nat) y, ys[i]? = some y →
      ∃ x, xs[i]? = some x ∧ P x y
  | _, _, .nil, _, _, h => by simp at h
  | _, _, .cons hp _, 0, _, h => by cases h; exact ⟨_, rfl, hp⟩
  | _, _, .cons _ t, i+1, y, h => pairs_get' t i y h

theorem addFacts_sound {vals : Bindings} {owner : Nat} :
    ∀ (l : List Instruction) (env : List (Nat × RFact)), EnvOK env vals →
      (∀ inst ∈ l, Ran vals owner inst) → EnvOK (addFacts env l) vals
  | [], _, h, _ => h
  | inst :: l, env, h, ran => by
    refine addFacts_sound l _ ?_ fun i hi => ran i (List.mem_cons_of_mem _ hi)
    cases hf : instFact env inst with
    | none => simpa [hf] using h
    | some e =>
      obtain ⟨rid, af⟩ := e
      simp only [hf, Option.elim]
      intro id bf hl v hv
      simp only [List.lookup] at hl
      split at hl
      · rename_i hid
        cases hl
        have hid : id = rid := by simpa using hid
        subst hid
        unfold instFact at hf
        split at hf
        · simp only [Option.some.injEq, Prod.mk.injEq] at hf
          obtain ⟨rfl, rfl⟩ := hf
          rename_i w x r hop hr
          obtain ⟨args, v0, r', hb, he, hr', hl'⟩ := ran inst List.mem_cons_self (by rw [hop]; rfl)
          rw [hr, List.cons.injEq] at hr'
          rw [hr'.1, hl'] at hv
          cases hv
          rw [hop] at he
          obtain ⟨-, e⟩ := evalOperation_constant he
          cases e
          exact holds_const w x
        · simp only [Option.some.injEq, Prod.mk.injEq] at hf
          obtain ⟨rfl, rfl⟩ := hf
          rename_i o r hop hr
          obtain ⟨args, v0, r', hb, he, hr', hl'⟩ := ran inst List.mem_cons_self (by rw [hop]; rfl)
          rw [hr, List.cons.injEq] at hr'
          rw [hr'.1, hl'] at hv
          cases hv
          rw [hop] at he
          obtain ⟨r0, hr0, e⟩ := evalOperation_scalar he
          cases e
          refine rawTransfer_sound hr0 fun i u hu => ?_
          obtain ⟨id', hid', hl⟩ := pairs_get' hb i u hu
          simp only [List.getD_eq_getElem?_getD, List.getElem?_map, hid', Option.map_some,
            Option.getD_some]
          cases he' : env.lookup id' with
          | none => exact holds_default u
          | some cf => exact h id' cf he' u hl
        · cases hf
      · exact h id bf hl v hv

/-- The facts of the values in the current block hold of their bindings. -/
theorem blockEnv_sound {p : Program} {T : Table} {a : Activation} {f : Function} {b : Block}
    (ok : ActOK p T a) (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.name == a.block) = some b) {k : Nat} (hk : k ≤ a.pc) :
    EnvOK (blockEnv T f.name b k) a.values := by
  obtain ⟨ran, fi⟩ := ok f b hf hb
  have hfn : f.name = a.function := by simpa using List.find?_some hf
  have hbn : b.name = a.block := by simpa using List.find?_some hb
  refine addFacts_sound _ _ ?_ fun i hi => ran i ?_
  · intro id af hl v hv
    obtain ⟨⟨prm, j⟩, hm, he⟩ := List.mem_map.mp (lookup_mem' hl)
    simp only [Prod.mk.injEq] at he
    obtain ⟨rfl, rfl⟩ := he
    obtain ⟨u, hu, hh⟩ := fi j prm (List.mem_zipIdx_iff_getElem?.mp hm)
    rw [hv] at hu
    cases hu
    rw [hfn, hbn]
    exact hh
  · obtain ⟨n, hn⟩ := List.getElem?_of_mem hi
    rw [List.getElem?_take] at hn
    split at hn
    · rename_i hnk
      have : (b.instructions.take a.pc)[n]? = some i := by
        rw [List.getElem?_take_of_lt (by omega : n < a.pc)]; exact hn
      exact List.mem_of_getElem? this
    · cases hn

theorem actOK_jump {p : Program} {T : Table} (tab : tableOk p T = true) {a a' : Activation}
    {f : Function} {b : Block} {e : Edge} (ok : ActOK p T a)
    (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.name == a.block) = some b) (hpc : a.pc = b.instructions.length)
    (he : e ∈ edges b.terminator) (hj : jump f a e = .ok a') : ActOK p T a' := by
  unfold jump at hj
  cases ht : f.blocks.find? (·.name == e.block) with
  | none => simp [ht] at hj
  | some tb =>
    simp only [ht, Bind.bind, Except.bind] at hj
    cases hargs : e.arguments.mapM (lookup a.values) with
    | error _ => simp [hargs] at hj
    | ok args =>
      simp only [hargs] at hj
      cases hbind : EvoIR.Core.IR.bind tb.parameters args with
      | error _ => simp [hbind] at hj
      | ok vals =>
        simp only [hbind, Pure.pure, Except.pure, Except.ok.injEq] at hj
        subst hj
        have htn : tb.name = e.block := by simpa using List.find?_some ht
        intro f' b' hf' hb'
        rw [hf, Option.some.injEq] at hf'
        subst hf'
        simp only [htn] at hb'
        rw [ht, Option.some.injEq] at hb'
        subst hb'
        refine ⟨by simp, fun k prm hk => ?_⟩
        obtain ⟨u, hu, hl⟩ := pairs_get (bind_ok hbind) k prm hk
        refine ⟨u, hl, ?_⟩
        obtain ⟨id, hid, hlu⟩ := pairs_get' (mapM_lookup.mp hargs) k u hu
        simp only [tableOk, List.all_eq_true, Bool.and_eq_true] at tab
        have hedge := (tab f (List.mem_of_find?_eq_some hf)).2 b (List.mem_of_find?_eq_some hb) e he
        simp only [edgeOk, ht, List.all_eq_true] at hedge
        have hle := hedge (((blockEnv T f.name b b.instructions.length).lookup id).getD default, k)
          (List.mem_zipIdx_iff_getElem?.mpr (by simp [passed, hid]))
        rw [← show f.name = a.function by simpa using List.find?_some hf]
        refine holds_le ?_ hle
        cases hen : (blockEnv T f.name b b.instructions.length).lookup id with
        | none => exact holds_default u
        | some af => exact blockEnv_sound ok hf hb (by omega) id af hen u hlu

theorem actOK_skip {p : Program} {T : Table} {a : Activation} {f : Function} {b : Block}
    {inst : Instruction} (ok : ActOK p T a) (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.name == a.block) = some b) (hi : b.instructions[a.pc]? = some inst)
    (hp : isPure inst.op = false) : ActOK p T {a with pc := a.pc + 1} := by
  intro f' b' hf' hb'
  rw [hf, Option.some.injEq] at hf'; subst hf'
  rw [hb, Option.some.injEq] at hb'; subst hb'
  obtain ⟨ran, fi⟩ := ok f b hf hb
  refine ⟨fun x hx => ?_, fi⟩
  rw [List.take_add_one, hi, Option.toList_some, List.mem_append, List.mem_singleton] at hx
  rcases hx with hx | rfl
  · exact ran x hx
  · intro h; rw [hp] at h; cases h

theorem actOK_enter {p : Program} {T : Table} (tab : tableOk p T = true) {callee : Function}
    {name : String} {args : List Value} {o : Nat} {act : Activation}
    (hc : p.functions.find? (·.name == name) = some callee) (he : enter callee args o = .ok act) :
    ActOK p T act := by
  have hcn : callee.name = name := by simpa using List.find?_some hc
  cases hh : callee.blocks.head? with
  | none =>
    simp [enter, hh, throw, throwThe, MonadExceptOf.throw, Bind.bind, Except.bind] at he <;>
      split at he <;> cases he
  | some b0 =>
  cases hbind : EvoIR.Core.IR.bind b0.parameters args with
  | error _ =>
    simp [enter, hh, hbind, throw, throwThe, MonadExceptOf.throw, Bind.bind, Except.bind] at he
    split at he <;> (try split at he) <;> cases he
  | ok vals =>
          simp [enter, hh, hbind, throw, throwThe, MonadExceptOf.throw, Bind.bind, Except.bind,
            Pure.pure, Except.pure] at he
          have hact : act = ⟨callee.name, b0.name, 0, vals, o⟩ := by
            split at he <;> (try split at he) <;> cases he; rfl
          subst hact
          obtain ⟨rest, hrest⟩ := List.head?_eq_some_iff.mp hh
          intro f' b' hf' hb'
          simp only [hcn, hc, Option.some.injEq] at hf'
          subst hf'
          rw [hrest, List.find?_cons] at hb'
          simp only [beq_self_eq_true, Option.some.injEq] at hb'
          subst hb'
          refine ⟨by simp, fun k prm hk => ?_⟩
          obtain ⟨u, -, hl⟩ := pairs_get (bind_ok hbind) k prm hk
          refine ⟨u, hl, holds_mask ?_ u⟩
          simp only [tableOk, List.all_eq_true, Bool.and_eq_true] at tab
          have := (tab callee (List.mem_of_find?_eq_some hc)).1
          simp only [entryOk, hh, List.all_eq_true, List.mem_range, beq_iff_eq] at this
          exact this k (List.getElem?_eq_some_iff.mp hk).1

theorem stateOK_execute {p : Program} {T : Table} (tab : tableOk p T = true) {s s' : State}
    {a : Activation} (h : StateOK p T s) (ha : s.control = .running a)
    (he : execute p s a = .ok s') : StateOK p T s' := by
  have ok := h.1 a ha
  unfold execute at he
  cases hf : p.functions.find? (·.name == a.function) with
  | none => simp [hf, throw, throwThe, MonadExceptOf.throw] at he
  | some f =>
  cases hb : f.blocks.find? (·.name == a.block) with
  | none => simp [hf, hb, throw, throwThe, MonadExceptOf.throw] at he
  | some b =>
  simp only [hf, hb] at he
  cases hi : b.instructions[a.pc]? with
  | some inst =>
    simp only [hi] at he
    cases hargs : inst.operands.mapM (lookup a.values) with
    | error _ => simp [hargs, Bind.bind, Except.bind] at he
    | ok args =>
    simp only [hargs, Bind.bind, Except.bind] at he
    split at he
    · rename_i name hop
      cases hc : p.functions.find? (·.name == name) with
      | none => simp [hc, throw, throwThe, MonadExceptOf.throw] at he
      | some callee =>
        simp only [hc] at he
        split at he
        · simp [throw, throwThe, MonadExceptOf.throw, Bind.bind, Except.bind] at he
        · cases hen : enter callee args s.nextFrame with
          | error _ => simp [hen] at he
          | ok act =>
            simp only [hen, Pure.pure, Except.pure, Except.ok.injEq] at he
            subst he
            refine ⟨fun a' ha' => ?_, fun fr hfr => ?_⟩
            · cases ha'; exact actOK_enter tab hc hen
            · rcases List.mem_cons.mp hfr with rfl | hfr
              · exact actOK_skip ok hf hb hi (by rw [hop]; rfl)
              · exact h.2 fr hfr
    · cases hres : evalOperation a.owner inst.op args with
      | error _ => simp [hres] at he
      | ok results =>
        cases hv : appendResults a.values inst.results results with
        | error _ => simp [hres, hv] at he
        | ok vals =>
          simp only [hres, hv, Pure.pure, Except.pure, Except.ok.injEq] at he
          subst he
          exact ⟨fun a' ha' => by cases ha'; exact actOK_instruction ok hf hb hi hargs hres hv, h.2⟩
  | none =>
    simp only [hi] at he
    split at he
    · simp [throw, throwThe, MonadExceptOf.throw, Bind.bind, Except.bind] at he
    · rename_i hpc
      have hpc : a.pc = b.instructions.length := by simpa using hpc
      have jumpOK : ∀ e a', e ∈ edges b.terminator → jump f a e = .ok a' →
          StateOK p T {s with control := .running a'} :=
        fun e a' hm hj => ⟨fun x hx => by cases hx; exact actOK_jump tab ok hf hb hpc hm hj, h.2⟩
      split at he
      · rename_i e ht
        cases hj : jump f a e with
        | error _ => simp [hj, Bind.bind, Except.bind] at he
        | ok a' =>
          simp only [hj, Bind.bind, Except.bind, Pure.pure, Except.pure, Except.ok.injEq] at he
          subst he
          exact jumpOK e a' (by simp [edges, ht]) hj
      · rename_i c yes no ht
        simp only [Bind.bind, Except.bind] at he
        split at he
        · cases he
        · split at he
          · cases he
          · split at he
            · cases he
            · rename_i a' hj
              simp only [Pure.pure, Except.pure, Except.ok.injEq] at he
              subst he
              exact jumpOK _ a' (by split <;> simp [edges, ht]) hj
      · rename_i ids ht
        simp only [Bind.bind, Except.bind] at he
        split at he
        · cases he
        · split at he
          · simp [throw, throwThe, MonadExceptOf.throw, Bind.bind, Except.bind] at he
          · split at he
            · rename_i hst
              split at he
              · simp [throw, throwThe, MonadExceptOf.throw, Bind.bind, Except.bind] at he
              · simp only [Pure.pure, Except.pure, Except.ok.injEq] at he
                subst he
                exact ⟨(fun x hx => nomatch hx), (fun g hg => by rw [hst] at hg; cases hg)⟩
            · rename_i fr rest hst
              split at he
              · cases he
              · rename_i vals hv
                simp only [Pure.pure, Except.pure, Except.ok.injEq] at he
                subst he
                refine ⟨fun x hx => ?_, fun g hg => h.2 g (by rw [hst]; exact List.mem_cons_of_mem _ hg)⟩
                cases hx
                exact actOK_extends (h.2 fr (by rw [hst]; exact List.mem_cons_self))
                  (appendResults_ok hv).1

end EvoIR.Rewrite
