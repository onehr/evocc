import EvoIR.Rewrite.Invariant
import EvoIR.Core.Correspondence
import EvoIR.Core.SourceTargetCheck

/-!
The rewrite theorem. When `checkRewrite before after cert` is true, the
program after the rewrite refines the program before the rewrite, with the
`Transition.Refines` relation and the `observeTarget` observations that the
source-to-IR and machine stages use.

The theorem has one explicit hypothesis, `safe`: every state that the program
before the rewrite reaches from the entry state can step or has an
observation. This is the `progress` field of the source-to-IR theorem, so
`source_rewrite_refines` discharges it.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR.Rewrite

open EvoIR.Core EvoIR.Core.IR

/-! ## Faults that are not observations -/

def Silent (e : Source.Fault) : Prop := ∀ c, e = .diagnostic c → c = 3001

theorem except_bind_error {α β ε : Type} {x : Except ε α} {f : α → Except ε β} {e : ε}
    (h : (x >>= f) = .error e) : x = .error e ∨ ∃ a, x = .ok a ∧ f a = .error e := by
  cases x with
  | error e' => left; simpa [Bind.bind, Except.bind] using h
  | ok a => right; exact ⟨a, rfl, h⟩

theorem mapM_silent {vals : Bindings} :
    ∀ {ids : List Nat} {e : Source.Fault}, ids.mapM (lookup vals) = .error e → Silent e
  | [], _, h => by simp [Pure.pure, Except.pure] at h
  | id :: ids, e, h => by
    simp only [List.mapM_cons] at h
    rcases except_bind_error h with h | ⟨_, -, h⟩
    · unfold lookup at h; split at h <;> cases h; intro c hc; cases hc
    · rcases except_bind_error h with h | ⟨_, -, h⟩
      · exact mapM_silent h
      · cases h

theorem asBits_silent {v : Value} {n : Nat} {e : Source.Fault} (h : v.asBits n = .error e) :
    Silent e := by
  intro c hc
  subst hc
  cases v <;> simp [Value.asBits] at h
  · split at h <;> cases h
  · exact h.symm

theorem evalScalar_silent {o : Op} {args : List Value} {e : Source.Fault}
    (h : evalScalar o args = .error e) : Silent e := by
  unfold evalScalar at h
  split at h
  all_goals first
    | (intro c hc; subst hc; cases h)
    | (rcases except_bind_error h with h | ⟨_, -, h⟩
       · exact asBits_silent h
       · first
         | (rcases except_bind_error h with h | ⟨_, -, h⟩
            · exact asBits_silent h
            · cases h)
         | cases h)

theorem evalOperation_silent {o : Nat} {op : Operation} {args : List Value} {e : Source.Fault}
    (hp : isPure op = true) (h : evalOperation o op args = .error e) : Silent e := by
  cases op <;> simp [isPure] at hp
  · match args, h with
    | [], h =>
      simp only [evalOperation] at h
      split at h
      · simp [throw, throwThe, MonadExceptOf.throw, Functor.map, Except.map] at h
        subst h; intro c hc; cases hc
      · cases h
    | _ :: _, h => simp [evalOperation, throw, throwThe, MonadExceptOf.throw] at h; subst h
                   intro c hc; cases hc
  · simp only [evalOperation] at h
    rcases except_bind_error h with h | ⟨_, -, h⟩
    · exact evalScalar_silent h
    · cases h

theorem silent_unobserved {e : Source.Fault} (h : Silent e) (rt : Source.IntType) (s : State) :
    observeTarget rt {s with control := .halted (.error e)} = none := by
  cases e with
  | diagnostic c => simp [observeTarget, h c rfl, checkedDiagnostic]
  | malformed m => rfl

/-! ## The reassembled program -/

@[simp] theorem replaceFunction_returns (R : List Root) (f : Function) :
    (replaceFunction R f).returns = f.returns := rfl
@[simp] theorem replaceBlock_terminator (R : List Root) (f : String) (b : Block) :
    (replaceBlock R f b).terminator = b.terminator := rfl
@[simp] theorem replaceBlock_length (R : List Root) (f : String) (b : Block) :
    (replaceBlock R f b).instructions.length = b.instructions.length := by simp [replaceBlock]

theorem find_fun (p : Program) (R : List Root) (n : String) :
    (replaceAll p R).functions.find? (·.name == n) =
      (p.functions.find? (·.name == n)).map (replaceFunction R) := by
  simp [replaceAll, List.find?_map, Function.comp_def, replaceFunction]

theorem find_block (R : List Root) (f : Function) (n : String) :
    (replaceFunction R f).blocks.find? (·.name == n) =
      (f.blocks.find? (·.name == n)).map (replaceBlock R f.name) := by
  simp [replaceFunction, List.find?_map, Function.comp_def, replaceBlock]

theorem enter_replace (R : List Root) (f : Function) (args : List Value) (o : Nat) :
    enter (replaceFunction R f) args o = enter f args o := by
  simp only [enter, replaceFunction, List.head?_map]
  cases f.blocks.head? <;> rfl

theorem jump_replace (R : List Root) (f : Function) (a : Activation) (e : Edge) :
    jump (replaceFunction R f) a e = jump f a e := by
  simp only [jump, find_block]
  cases f.blocks.find? (·.name == e.block) <;> rfl

theorem fetch_replace (R : List Root) (f : String) (b : Block) (j : Nat) :
    (replaceBlock R f b).instructions[j]? = b.instructions[j]?.map fun i => (rootAt R f b.name j).getD i := by
  simp [replaceBlock, List.getElem?_mapIdx]

theorem initial_replace (p : Program) (R : List Root) (fn : String) (args : List Value) :
    IR.initial (replaceAll p R) fn args = IR.initial p fn args := by
  simp only [IR.initial, find_fun]
  cases p.functions.find? (·.name == fn) <;> simp [enter_replace]

theorem execute_replace (p : Program) (R : List Root) (s : State) (a : Activation)
    (h : rootAt R a.function a.block a.pc = none) :
    execute (replaceAll p R) s a = execute p s a := by
  unfold execute
  rw [find_fun]
  cases hf : p.functions.find? (·.name == a.function) with
  | none => rfl
  | some f =>
    simp only [Option.map_some]
    rw [find_block]
    cases hb : f.blocks.find? (·.name == a.block) with
    | none => rfl
    | some b =>
      have hfn : f.name = a.function := by simpa using List.find?_some hf
      have hbn : b.name = a.block := by simpa using List.find?_some hb
      simp only [Option.map_some, fetch_replace, hfn, hbn, h, Option.getD_none, Option.map_id',
        find_fun, jump_replace, enter_replace]
      cases hi : b.instructions[a.pc]? with
      | some inst =>
        simp only
        cases hop : inst.op
        case call name =>
          simp only
          cases p.functions.find? (·.name == name) <;>
            simp [enter_replace, Bind.bind, Except.bind, throw, throwThe, MonadExceptOf.throw]
        all_goals rfl
      | none => simp

theorem execute_pure {p : Program} {s : State} {a : Activation} {f : Function} {b : Block}
    {inst : Instruction} (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.name == a.block) = some b) (hi : b.instructions[a.pc]? = some inst)
    (hp : isPure inst.op = true) :
    execute p s a = (do
      let args ← inst.operands.mapM (lookup a.values)
      let results ← evalOperation a.owner inst.op args
      let values ← appendResults a.values inst.results results
      return {s with control := .running {a with pc := a.pc + 1, values}}) := by
  unfold execute
  simp only [hf, hb, hi]
  congr 1
  funext args
  split
  · rename_i name hop; rw [hop] at hp; cases hp
  · rfl

/-! ## One region gives a sound root -/

def RootOK (p : Program) (T : Table) (r : Root) : Prop :=
  ∃ f b old, p.functions.find? (·.name == r.function) = some f ∧
    f.blocks.find? (·.name == r.block) = some b ∧ b.instructions[r.index]? = some old ∧
    isPure old.op = true ∧ isPure r.inst.op = true ∧ r.inst.results = old.results ∧
    ∀ (a : Activation), a.function = r.function → a.block = r.block → a.pc = r.index →
      ActOK p T a → ∀ args results, Bound a.values old.operands args →
        evalOperation a.owner old.op args = .ok results →
        ∃ args', Bound a.values r.inst.operands args' ∧ evalOperation a.owner r.inst.op args' = .ok results

theorem regionRoot_ok {p : Program} {T : Table} {rules : List CoveredRule} {guards : Bool}
    {region : Region} {root : Root} (h : regionRoot p T rules guards region = some root) :
    RootOK p T root := by
  obtain ⟨⟨f, b, old, st, hf, hb, hold, clean, st0, hst0, hrep, op, operands, hfin, pop, pold,
    rfl⟩⟩ := accepted_of h
  have hfn : f.name = region.function := by simpa using List.find?_some hf
  have hbn : b.name = region.block := by simpa using List.find?_some hb
  refine ⟨f, b, old, by rw [hfn]; exact hf, by rw [hbn]; exact hb, hold, pold, pop, rfl, ?_⟩
  intro a ha hba hpc ok args results hargs he
  obtain ⟨ran, fi⟩ := ok f b (by rw [ha, hfn]; exact hf) (by rw [hba, hbn]; exact hb)
  rw [hpc] at ran
  rw [ha, hba] at fi
  exact region_root hold pop clean hst0 hrep hfin ran fi hargs he

/-! ## The theorem -/

theorem stateOK_step {p : Program} {T : Table} (tab : tableOk p T = true) {s s' : State}
    (h : StateOK p T s) (hs : IR.step p s = some s') : StateOK p T s' := by
  unfold IR.step at hs
  split at hs
  · cases hs
  · rename_i a ha
    simp only [Option.some.injEq] at hs
    subst hs
    split
    · rename_i n hn; exact stateOK_execute tab h ha hn
    · exact ⟨(fun x hx => nomatch hx), h.2⟩

theorem stateOK_initial {p : Program} {T : Table} (tab : tableOk p T = true) {fn : String}
    {args : List Value} {s : State} (h : IR.initial p fn args = .ok s) : StateOK p T s := by
  unfold IR.initial at h
  cases hf : p.functions.find? (·.name == fn) with
  | none => simp [hf, throw, throwThe, MonadExceptOf.throw] at h
  | some f =>
    cases he : enter f args 1 with
    | error _ => simp [hf, he, Bind.bind, Except.bind] at h
    | ok act =>
      simp [hf, he, Bind.bind, Except.bind, Pure.pure, Except.pure] at h
      subst h
      exact ⟨fun x hx => by cases hx; exact actOK_enter tab hf he, fun _ h => nomatch h⟩

theorem rootAt_mem {R : List Root} {f b : String} {j : Nat} {inst : Instruction}
    (h : rootAt R f b j = some inst) : ∃ r ∈ R, r.function = f ∧ r.block = b ∧ r.index = j ∧ r.inst = inst := by
  obtain ⟨r, hr, rfl⟩ := Option.map_eq_some_iff.mp h
  have := List.find?_some hr
  simp only [Bool.and_eq_true, beq_iff_eq] at this
  exact ⟨r, List.mem_of_find?_eq_some hr, this.1.1, this.1.2, this.2, rfl⟩

/-- In every state that the program before the rewrite reaches, one step of
the reassembled program is the same step. -/
theorem step_same {p : Program} {T : Table} {R : List Root}
    (roots : ∀ r ∈ R, RootOK p T r) {rt : Source.IntType} {entry s : State}
    (reach : Transition.Reaches (IR.step p) entry s) (ok : StateOK p T s)
    (safe : ∀ s, Transition.Reaches (IR.step p) entry s →
      (∃ s', IR.step p s = some s') ∨ ∃ o, observeTarget rt s = some o) :
    IR.step (replaceAll p R) s = IR.step p s := by
  unfold IR.step
  split
  · rfl
  · rename_i a ha
    cases hr : rootAt R a.function a.block a.pc with
    | none => rw [execute_replace p R s a hr]
    | some new =>
      obtain ⟨r, hrm, hfn, hbn, hpc, rfl⟩ := rootAt_mem hr
      obtain ⟨f, b, old, hf, hb, hold, pold, pnew, hres, sound⟩ := roots r hrm
      rw [hfn] at hf; rw [hbn] at hb; rw [hpc] at hold
      have hfn' : f.name = a.function := by simpa using List.find?_some hf
      have hbn' : b.name = a.block := by simpa using List.find?_some hb
      have hnew : (replaceBlock R f.name b).instructions[a.pc]? = some r.inst := by
        rw [fetch_replace, hold, hfn', hbn', hr]; rfl
      have hf' := find_fun p R a.function
      rw [hf, Option.map_some] at hf'
      have hb' := find_block R f a.block
      rw [hb, Option.map_some] at hb'
      rw [execute_pure hf' hb' hnew pnew, execute_pure hf hb hold pold]
      -- The old root does not fault, since a fault is not an observation.
      have noFault : ∀ e, ¬ Silent e ∨ IR.step p s ≠ some {s with control := .halted (.error e)} := by
        intro e
        by_cases hs : Silent e
        · right
          intro hstep
          rcases safe _ (reach.trans (.next hstep (.refl _))) with ⟨_, h⟩ | ⟨o, h⟩
          · simp [IR.step] at h
          · rw [silent_unobserved hs] at h; cases h
        · exact .inl hs
      have stepErr : ∀ e, execute p s a = .error e → Silent e → False := by
        intro e he hs
        rcases noFault e with h | h
        · exact h hs
        · exact h (by simp [IR.step, ha, he])
      rw [execute_pure hf hb hold pold] at stepErr
      cases hargs : old.operands.mapM (lookup a.values) with
      | error e => exact (stepErr e (by simp [hargs, Bind.bind, Except.bind]) (mapM_silent hargs)).elim
      | ok args =>
        cases hev : evalOperation a.owner old.op args with
        | error e =>
          exact (stepErr e (by simp [hargs, hev, Bind.bind, Except.bind])
            (evalOperation_silent pold hev)).elim
        | ok results =>
          obtain ⟨args', hb'', he'⟩ := sound a hfn.symm hbn.symm hpc.symm (ok.1 a ha) args results
            (mapM_lookup.mp hargs) hev
          simp only [Bind.bind, Except.bind, hargs, hev, mapM_lookup.mpr hb'', he', hres]

/-- **Rewrite soundness.** An accepted rewrite preserves the entry state and
refines the program before the rewrite. -/
theorem checkRewrite_refines {before after : Program} {cert : Certificate}
    (accepted : checkRewrite before after cert = true) {rt : Source.IntType} {fn : String}
    {args : List Value} {entry : State} (init : IR.initial before fn args = .ok entry)
    (safe : ∀ s, Transition.Reaches (IR.step before) entry s →
      (∃ s', IR.step before s = some s') ∨ ∃ o, observeTarget rt s = some o) :
    IR.initial after fn args = .ok entry ∧
      Transition.Refines (IR.step before) (IR.step after) (observeTarget rt) (observeTarget rt)
        entry entry := by
  unfold checkRewrite at accepted
  split at accepted
  · rename_i T R hroots
    have hafter : after = replaceAll before R := by simpa using accepted
    subst hafter
    unfold rewriteRoots at hroots
    simp only [bind_some, guard_unit, exists_and_left, exists_const] at hroots
    obtain ⟨rules, -, tab, rs, hrs, -, he⟩ := hroots
    simp only [Option.some.injEq, Prod.mk.injEq] at he
    obtain ⟨hT, rfl⟩ := he
    rw [hT] at tab
    have roots : ∀ r ∈ rs, RootOK before T r := by
      intro r hr
      obtain ⟨i, hi⟩ := List.getElem?_of_mem hr
      obtain ⟨region, -, hreg⟩ := pairs_get' (mapM_pairs hrs) i r hi
      rw [← hT]
      exact regionRoot_ok hreg
    have okAll : ∀ s t, StateOK before T s → Transition.Reaches (IR.step before) s t →
        StateOK before T t := by
      intro s t h hs
      induction hs with
      | refl => exact h
      | next hstep _ ih => exact ih (stateOK_step tab h hstep)
    have okEntry := stateOK_initial tab init
    have same : ∀ s, Transition.Reaches (IR.step before) entry s →
        IR.step (replaceAll before rs) s = IR.step before s :=
      fun s hr => step_same roots hr (okAll _ _ okEntry hr) safe
    refine ⟨by rw [initial_replace]; exact init, ?_⟩
    refine Transition.RankedSimulation.sound
      (related := fun s t => Transition.Reaches (IR.step before) entry s ∧ t = s)
      (rank := fun _ _ => 0) ⟨?_, ?_, ?_⟩ ⟨.refl _, rfl⟩
    · intro s t t' h hstep
      obtain ⟨hr, ht⟩ := h
      rw [ht, same s hr] at hstep
      exact .inl ⟨t', .single hstep, hr.trans (.next hstep (.refl _)), rfl⟩
    · intro s t h hstop
      obtain ⟨hr, ht⟩ := h
      rw [ht, same s hr] at hstop
      rw [ht]
      rcases safe s hr with ⟨_, h⟩ | ⟨o, h⟩
      · rw [hstop] at h; cases h
      · exact ⟨s, o, .refl _, hstop, h, h⟩
    · intro s t h
      obtain ⟨hr, ht⟩ := h
      rw [ht, same s hr]
      exact safe s hr
  · cases accepted

/-- Composition with the source-to-IR stage. The `progress` field of the
source-to-IR refinement discharges the `safe` hypothesis. -/
theorem source_rewrite_refines {source : Source.Program} {before after : Program}
    {cert : Certificate} (accepted : checkRewrite before after cert = true) {rt : Source.IntType}
    {fn : String} {args : List Value} {sourceEntry : Source.State} {entry : State}
    (init : IR.initial before fn args = .ok entry)
    (front : LoweringRefines source before rt sourceEntry entry) :
    IR.initial after fn args = .ok entry ∧ LoweringRefines source after rt sourceEntry entry := by
  obtain ⟨hi, hr⟩ := checkRewrite_refines accepted init front.progress
  exact ⟨hi, Transition.Refines.trans front hr⟩

/-- The checked source-to-IR pair, then the checked rewrite: typed Core source
refines the program after the rewrite for every typed argument list. -/
theorem check_source_rewrite_refines {source : Source.Program} {before after : Program}
    {cert : Certificate} {request : Symbolic.SourceTargetRequest} {route : Symbolic.SourceTargetRoute}
    {pair : Symbolic.CallFrontierPair source before}
    (checked : Symbolic.checkSourceTargetPair source before request = some (route, pair))
    (accepted : checkRewrite before after cert = true) (args : List Source.Value)
    (typed : args.map Source.Value.type = pair.sourceFunction.parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      Source.initial source pair.sourceFunction.symbol args = .ok sourceEntry ∧
      IR.initial after pair.targetFunction.name (Symbolic.loopInputValues pair.tokens args) =
        .ok targetEntry ∧
      LoweringRefines source after pair.sourceFunction.returnType.type sourceEntry targetEntry := by
  obtain ⟨se, te, hs, ht, hr⟩ :=
    Symbolic.check_source_target_refines request route pair checked args typed
  obtain ⟨ht', hr'⟩ := source_rewrite_refines accepted ht hr
  exact ⟨se, te, hs, ht', hr'⟩

end EvoIR.Rewrite
