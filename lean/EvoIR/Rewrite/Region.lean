import EvoIR.Rewrite.Machine

/-!
The link between one accepted region and the machine. At the root of the
region, the runtime values of the seeds give one interpretation of the
classes. The witness instructions that already ran hold the values of their
classes. The replacement instruction then computes the result of the old root.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR.Rewrite

open EvoIR.Core.IR

def junk : Value := .poison (.bits 0)

def classValue (vals : List Value) (S : Nat) (nodes : List RNode) (c : Nat) : Value :=
  if c < S then vals.getD c junk else
  match nodes[c - S]? with
  | some nd => nodeValue nd.op (nd.children.map fun ch =>
      if ch < c then classValue vals S nodes ch else junk)
  | none => junk
termination_by c
decreasing_by assumption

theorem pure_owner {o o' : Nat} {op : Operation} (hp : isPure op = true) (args : List Value) :
    evalOperation o op args = evalOperation o' op args := by
  cases op <;> simp [isPure] at hp
  · cases args <;> rfl
  · rfl

theorem pairs_get {α β : Type} {P : α → β → Prop} :
    ∀ {xs : List α} {ys : List β}, Pairs P xs ys → ∀ (i : Nat) x, xs[i]? = some x → ∃ y, ys[i]? = some y ∧ P x y
  | _, _, .nil, _, _, h => by simp at h
  | _, _, .cons hp t, 0, _, h => by cases h; exact ⟨_, rfl, hp⟩
  | _, _, .cons _ t, i+1, x, h => pairs_get t i x h

theorem pairs_map {α β : Type} {xs : List α} {ys : List β} (g : α → β) (P : α → β → Prop)
    (hp : Pairs P xs ys) (h : ∀ (i : Nat) x y, xs[i]? = some x → ys[i]? = some y → P x y → y = g x) :
    ys = xs.map g := by
  induction hp with
  | nil => rfl
  | cons hxy _ ih =>
    rw [List.map_cons, h 0 _ _ rfl rfl hxy, ih fun i x y hx hy hp => h (i+1) x y hx hy hp]

theorem typed_args (o : Op) {args : List Value}
    (h : Pairs (fun v w => ∃ x, v = .bits w x) args (opInputs o)) : ∃ d, args = opValues o d := by
  cases o <;> simp only [opInputs] at h
  case cast _ w _ =>
    cases h with | cons h1 t => cases t; obtain ⟨x, rfl⟩ := h1; exact ⟨x, rfl⟩
  case nonzero w =>
    cases h with | cons h1 t => cases t; obtain ⟨x, rfl⟩ := h1; exact ⟨x, rfl⟩
  all_goals
    cases h with
    | cons h1 t =>
      cases t with
      | cons h2 t2 =>
        cases t2
        obtain ⟨x, rfl⟩ := h1
        obtain ⟨y, rfl⟩ := h2
        exact ⟨(x, y), rfl⟩

/-- The facts that `shapeOk` gives about node `k`. -/
theorem shape_node {S : Nat} {st : St} (h : shapeOk S st = true) {k : Nat} {nd : RNode}
    (hk : st.nodes[k]? = some nd) :
    nd.owner = S + k ∧ (∀ ch ∈ nd.children, ch < S + k) ∧
      (k < st.witnessCount ↔ nd.result.isSome) ∧
      ((∃ w v, nd.op = .constant w v ∧ nd.children = [] ∧ w ≠ 0 ∧ v < 2 ^ w ∧ st.sort (S + k) = w) ∨
       (∃ o, nd.op = .scalar o ∧ opInputs o = nd.children.map st.sort ∧ st.sort (S + k) = o.res)) := by
  simp only [shapeOk, Bool.and_eq_true, List.all_eq_true] at h
  have := h.2 (nd, k) (List.mem_zipIdx_iff_getElem?.mpr hk)
  simp only [Bool.and_eq_true, beq_iff_eq, List.all_eq_true, decide_eq_true_eq] at this
  obtain ⟨⟨⟨ho, hc⟩, hw⟩, hs⟩ := this
  refine ⟨ho, hc, ⟨fun hk => hw ▸ decide_eq_true hk, fun hs => of_decide_eq_true (hw.trans hs)⟩, ?_⟩
  split at hs
  · rename_i w v hop
    simp only [Bool.and_eq_true, List.isEmpty_iff, bne_iff_ne, ne_eq, decide_eq_true_eq,
      beq_iff_eq] at hs
    exact .inl ⟨w, v, hop, hs.1.1.1, hs.1.1.2, hs.1.2, hs.2⟩
  · rename_i o hop
    simp only [Bool.and_eq_true, beq_iff_eq] at hs
    exact .inr ⟨o, hop, hs.1, hs.2⟩
  · cases hs

theorem classValue_node {vals : List Value} {S : Nat} {nodes : List RNode} {k : Nat} {nd : RNode}
    (hk : nodes[k]? = some nd) (hc : ∀ ch ∈ nd.children, ch < S + k) :
    classValue vals S nodes (S + k) = nodeValue nd.op (nd.children.map (classValue vals S nodes)) := by
  rw [classValue, if_neg (by omega), Nat.add_sub_cancel_left, hk]
  simp only
  congr 1
  exact List.map_congr_left fun ch hm => by rw [if_pos (hc ch hm)]

/-! ## The region at its root -/

structure Accepted (before : Program) (T : Table) (rules : List CoveredRule) (guards : Bool)
    (region : Region) (root : Root) where
  f : Function
  b : Block
  old : Instruction
  st : St
  hf : before.functions.find? (·.name == region.function) = some f
  hb : f.blocks.find? (·.name == region.block) = some b
  hold : b.instructions[region.index]? = some old
  clean : cleanRegion (badValues f (valueCount f + 1) []) b region guards = true
  st0 : St
  hst0 : start region b = some st0
  hreplay : replay ⟨b, region, rules, guards, seedFacts T f.name b region⟩ st0 region.events = some st
  op : Operation
  operands : List Nat
  hfinish : finish ⟨b, region, rules, guards, seedFacts T f.name b region⟩ st = some (op, operands)
  pureOp : isPure op = true
  pureOld : isPure old.op = true
  hroot : root = ⟨f.name, b.name, region.index, ⟨old.results, op, operands⟩⟩

theorem accepted_of {before : Program} {T : Table} {rules : List CoveredRule} {guards : Bool}
    {region : Region} {root : Root} (h : regionRoot before T rules guards region = some root) :
    Nonempty (Accepted before T rules guards region root) := by
  unfold regionRoot at h
  simp only [bind_some, guard_unit, Bool.and_eq_true, exists_and_left, exists_const] at h
  obtain ⟨f, hf, b, hb, old, hold, clean, st0, hst0, st, hr, ⟨op, operands⟩, hfin, ⟨p1, p2⟩, e⟩ := h
  exact ⟨⟨f, b, old, st, hf, hb, hold, clean, st0, hst0, hr, op, operands, hfin, p1, p2,
    (Option.some.inj e).symm⟩⟩

/-! ## Seeds -/

/-- The value of a seed: its runtime value when that is a bit vector of the
seed's width, and otherwise the value of its fact. -/
def seedValue (vals : Bindings) (s : SeedRec) (af : RFact) : Value :=
  match vals.lookup s.value with
  | some (.bits n y) => if n = s.width then .bits n y else .bits s.width (toFact s.width af.mask af.value).value
  | _ => .bits s.width (toFact s.width af.mask af.value).value

theorem gamma_value {n : Nat} (K : Fact n) : gamma K K.value := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  have := norm_bit K hi
  simp only [BitVec.getLsbD_and]
  revert this
  cases K.value.getLsbD i <;> cases K.mask.getLsbD i <;> simp

theorem seedValue_spec (vals : Bindings) (s : SeedRec) (af : RFact)
    (h : ∀ v, vals.lookup s.value = some v → Holds af v) :
    (∃ x, seedValue vals s af = .bits s.width x) ∧ Holds af (seedValue vals s af) ∧
      ∀ y, vals.lookup s.value = some (.bits s.width y) → seedValue vals s af = .bits s.width y := by
  have dflt : Holds af (.bits s.width (toFact s.width af.mask af.value).value) :=
    fun _ => gamma_value _
  unfold seedValue
  split
  · rename_i n y hl
    split
    · rename_i hn
      subst hn
      exact ⟨⟨y, rfl⟩, h _ hl, fun y' h' => by rw [hl] at h'; cases h'; rfl⟩
    · rename_i hn
      exact ⟨⟨_, rfl⟩, dflt, fun y' h' => by rw [hl] at h'; cases h'; exact absurd rfl hn⟩
  · rename_i hl
    exact ⟨⟨_, rfl⟩, dflt, fun y' h' => absurd h' (hl _ _)⟩

theorem args_eq {vals : Bindings} {I : Nat → Value} :
    ∀ {ops : List Nat} {args : List Value} {cs : List Nat}, Bound vals ops args →
      ops.length = cs.length →
      (∀ (p : Nat) v c, ops[p]? = some v → cs[p]? = some c → vals.lookup v = some (I c)) →
      args = cs.map I
  | _, _, [], .nil, _, _ => rfl
  | _, _, c :: cs, .cons hv t, hl, h => by
    rw [List.map_cons, ← args_eq t (by simpa using hl) (fun p v c hv hc => h (p+1) v c hv hc)]
    have := h 0 _ c rfl rfl
    rw [hv] at this
    rw [Option.some.inj this]

theorem eval_node {owner : Nat} {op : Operation} {args : List Value} {v : Value}
    (hp : isPure op = true) (h : evalOperation owner op args = .ok [v]) : nodeValue op args = v := by
  rw [pure_owner (o' := 0) hp] at h
  simp [nodeValue, h]

theorem opValues_bits (o : Op) (d : o.dom) : ∀ u ∈ opValues o d, ∃ n y, u = .bits n y := by
  cases o with
  | cast _ _ _ => intro u hu; simp only [opValues, List.mem_singleton] at hu; exact ⟨_, _, hu⟩
  | nonzero _ => intro u hu; simp only [opValues, List.mem_singleton] at hu; exact ⟨_, _, hu⟩
  | _ =>
    obtain ⟨x, y⟩ := d
    intro u hu
    simp only [opValues, List.mem_cons, List.not_mem_nil, or_false] at hu
    rcases hu with rfl | rfl <;> exact ⟨_, _, rfl⟩

theorem operand_bits {owner : Nat} {o : Op} {ops cs : List Nat} {args rs : List Value}
    {sort : Nat → Nat} {vals : Bindings} (hin : opInputs o = cs.map sort)
    (hb : Bound vals ops args) (he : evalOperation owner (.scalar o) args = .ok rs)
    {p : Nat} {v c : Nat} (hv : ops[p]? = some v) (hc : cs[p]? = some c) :
    ∃ y, vals.lookup v = some (.bits (sort c) y) := by
  obtain ⟨r, hr, -⟩ := evalOperation_scalar he
  obtain ⟨d, rfl, -⟩ := evalScalar_ok hr
  obtain ⟨u, hu, hl⟩ := pairs_get hb p v hv
  have ht := congrArg (·[p]?) (opValues_widths o d)
  simp only [List.getElem?_map, hu, hin, hc, Option.map_some, Option.some.injEq] at ht
  obtain ⟨n, y, rfl⟩ := opValues_bits o d u (List.mem_of_getElem? hu)
  cases ht
  exact ⟨y, hl⟩

/-! ## Final checks -/

theorem start_spec {region : Region} {b : Block} {st0 : St} (h : start region b = some st0) :
    st0.parent = List.range region.seeds.length ∧ st0.sorts = region.seeds.map (·.width) := by
  unfold start at h
  simp only [bind_some, guard_unit, exists_and_left, exists_const] at h
  obtain ⟨-, h⟩ := h
  cases h
  exact ⟨rfl, rfl⟩

theorem witness_spec {ctx : Ctx} {st : St} (h : witnessOk ctx st = true) {k : Nat}
    (hk : k < ctx.region.witnesses.length) :
    ∃ j inst nd, ctx.region.witnesses[k]? = some j ∧ ctx.block.instructions[j]? = some inst ∧
      st.nodes[k]? = some nd ∧ inst.op = nd.op ∧ inst.results.map (·.id) = nd.result.toList ∧
      inst.operands.length = nd.children.length ∧
      ∀ (p : Nat) v c, inst.operands[p]? = some v → nd.children[p]? = some c →
        ∃ oc, operandClass ctx st k v = some oc ∧ st.find oc = st.find c ∧ st.sort oc = st.sort c := by
  simp only [witnessOk, Bool.and_eq_true, List.all_eq_true, List.mem_range] at h
  have hk' := h.2 k hk
  split at hk'
  · rename_i inst nd hi hn
    obtain ⟨j, hj, hji⟩ := Option.bind_eq_some_iff.mp hi
    simp only [Bool.and_eq_true, beq_iff_eq, List.all_eq_true] at hk'
    obtain ⟨⟨⟨hop, hres⟩, hlen⟩, hall⟩ := hk'
    refine ⟨j, inst, nd, hj, hji, hn, hop, hres, hlen, fun p v c hv hc => ?_⟩
    have hm : (v, c) ∈ inst.operands.zip nd.children :=
      List.mem_iff_getElem?.mpr ⟨p, List.getElem?_zip_eq_some.mpr ⟨hv, hc⟩⟩
    have := hall _ hm
    split at this
    · rename_i oc hoc
      simp only [Bool.and_eq_true, beq_iff_eq] at this
      exact ⟨oc, hoc, this.1, this.2⟩
    · cases this
  · cases hk'

theorem operandClass_spec {ctx : Ctx} {st : St} {k v oc : Nat}
    (h : operandClass ctx st k v = some oc) :
    (∃ s, ctx.region.seeds[oc]? = some s ∧ s.value = v) ∨
      ∃ k', k' < k ∧ oc = ctx.region.seeds.length + k' ∧ (st.nodes[k']?.bind (·.result)) = some v := by
  unfold operandClass at h
  split at h
  · rename_i i hi
    cases h
    obtain ⟨hl, hp, -⟩ := List.findIdx?_eq_some_iff_getElem.mp hi
    exact .inl ⟨_, List.getElem?_eq_getElem hl, by simpa using hp⟩
  · obtain ⟨k', hk', rfl⟩ := Option.map_eq_some_iff.mp h
    have hm := List.mem_of_find?_eq_some hk'
    have hp := List.find?_some hk'
    exact .inr ⟨k', List.mem_range.mp hm, rfl, by simpa using hp⟩

theorem pairs_map2 {α β : Type} {P : β → Nat → Prop} {f : α → β} {g : α → Nat} :
    ∀ {l : List α}, (∀ x ∈ l, P (f x) (g x)) → Pairs P (l.map f) (l.map g)
  | [], _ => .nil
  | x :: _, h => .cons (h x List.mem_cons_self) (pairs_map2 fun y hy => h y (List.mem_cons_of_mem _ hy))

theorem mapM_pairs {α β : Type} {f : α → Option β} :
    ∀ {l : List α} {out : List β}, l.mapM f = some out → Pairs (fun x y => f x = some y) l out
  | [], out, h => by cases h; exact .nil
  | x :: l, out, h => by
    simp only [List.mapM_cons, bind_some] at h
    obtain ⟨y, hy, ys, hys, e⟩ := h
    cases e
    exact .cons hy (mapM_pairs hys)

theorem pairs_flip {R : Nat → Nat → Prop} {vals : Bindings} {I : Nat → Value}
    (f : ∀ ch id, R ch id → vals.lookup id = some (I ch)) :
    ∀ {l ops : List Nat}, Pairs R l ops → Bound vals ops (l.map I)
  | _, _, .nil => .nil
  | _, _, .cons h t => .cons (f _ _ h) (pairs_flip f t)

theorem nodeValue_ok {op : Operation} {args : List Value} {n : Nat} {x : BitVec n}
    (h : nodeValue op args = .bits n x) : evalOperation 0 op args = .ok [.bits n x] := by
  unfold nodeValue at h
  split at h
  · rename_i v hv; rw [hv, h]
  · cases h

theorem sort_get {st : St} {c : Nat} (h : c < st.sorts.length) : st.sorts[c]? = some (st.sort c) := by
  simp [St.sort, List.getD_eq_getElem?_getD, List.getElem?_eq_getElem h]

/-- One accepted region, at its root: whenever the old root ran with the
given operands, the replacement gives the same results. -/
theorem region_root {T : Table} {rules : List CoveredRule} {guards : Bool} {region : Region}
    {b : Block} {fname : String} {old : Instruction} {st0 st : St} {op : Operation}
    {operands : List Nat} {bad : List Nat}
    (hold : b.instructions[region.index]? = some old) (pop : isPure op = true)
    (clean : cleanRegion bad b region guards = true)
    (hst0 : start region b = some st0)
    (hrep : replay ⟨b, region, rules, guards, seedFacts T fname b region⟩ st0 region.events = some st)
    (hfin : finish ⟨b, region, rules, guards, seedFacts T fname b region⟩ st = some (op, operands))
    {vals : Bindings} {owner : Nat}
    (ran : ∀ inst ∈ b.instructions.take region.index, Ran vals owner inst)
    (fi : ∀ k prm, b.parameters[k]? = some prm →
      ∃ v, vals.lookup prm.id = some v ∧ Holds (T.param fname b.name k) v)
    {args results : List Value} (hargs : Bound vals old.operands args)
    (heval : evalOperation owner old.op args = .ok results) :
    ∃ args', Bound vals operands args' ∧ evalOperation owner op args' = .ok results := by
  have hfin' := hfin
  unfold finish at hfin'
  simp only [bind_some, guard_unit, Bool.and_eq_true, exists_and_left, exists_const,
    beq_iff_eq] at hfin'
  obtain ⟨⟨⟨⟨⟨hwc, hlast⟩, hdrop⟩, hshape⟩, hwit⟩, rn, hrn, ⟨⟨-, hrf⟩, -⟩, ⟨rc', ch⟩, -, hrest⟩ := hfin'
  rcases ch with i | k
  · simp at hrest
  have hfin' := hrest
  simp only [bind_some, guard_unit, Bool.and_eq_true, exists_and_left, exists_const, beq_iff_eq,
    bne_iff_ne, ne_eq] at hfin'
  obtain ⟨nd, hnd, ⟨⟨hk, -⟩, hnf⟩, ops, hops, -, -, hfin'⟩ := hfin'
  simp only [Option.some.injEq, Prod.mk.injEq] at hfin'
  obtain ⟨rfl, rfl⟩ := hfin'
  have hlens : st.parent.length = region.seeds.length + st.nodes.length ∧
      st.sorts.length = st.parent.length := by
    simp only [shapeOk, Bool.and_eq_true, beq_iff_eq] at hshape; exact hshape.1
  have hwl : region.witnesses[region.witnesses.length - 1]? = some region.index := by
    rw [← List.getLast?_eq_getElem?]; exact hlast
  have hwpos : region.witnesses.length - 1 < region.witnesses.length := by
    have := (List.getElem?_eq_some_iff.mp hwl).1; omega
  have hbefore : ∀ k', k' < region.witnesses.length - 1 → ∀ j, region.witnesses[k']? = some j →
      j < region.index := by
    intro k' hk' j hj
    have := List.all_eq_true.mp hdrop j
      (List.mem_of_getElem? (by rw [List.getElem?_dropLast, if_pos hk']; exact hj))
    simpa using this
  -- Seeds and the interpretation of classes.
  obtain ⟨hp0, hs0⟩ := start_spec hst0
  obtain ⟨-, ⟨u, hu⟩⟩ := replay_shape hrep
  have seedSort : ∀ i s, region.seeds[i]? = some s → st.sort i = s.width := by
    intro i s hi
    have hl := (List.getElem?_eq_some_iff.mp hi).1
    have : st.sorts[i]? = some s.width := by
      rw [← hu, List.getElem?_append_left (by rw [hs0]; simpa using hl), hs0, List.getElem?_map, hi,
        Option.map_some]
    simp [St.sort, List.getD_eq_getElem?_getD, this]
  have hfs : ∀ i s, region.seeds[i]? = some s → ∀ v, vals.lookup s.value = some v →
      Holds ((seedFacts T fname b region).getD i default) v := by
    intro i s hi v hv
    simp only [seedFacts, List.getD_eq_getElem?_getD, List.getElem?_map, hi, Option.map_some,
      Option.getD_some]
    split
    · rename_i k hk
      obtain ⟨hl, hp, -⟩ := List.findIdx?_eq_some_iff_getElem.mp hk
      obtain ⟨u, hu, hh⟩ := fi k _ (List.getElem?_eq_getElem hl)
      rw [show b.parameters[k].id = s.value by simpa using hp, hv] at hu
      cases hu
      exact hh
    · exact holds_default _
  let I := classValue (List.zipWith (seedValue vals) region.seeds (seedFacts T fname b region))
    region.seeds.length st.nodes
  have hIseed : ∀ i s, region.seeds[i]? = some s →
      I i = seedValue vals s ((seedFacts T fname b region).getD i default) := by
    intro i s hi
    obtain ⟨hl, rfl⟩ := List.getElem?_eq_some_iff.mp hi
    simp [I, classValue, hl, List.getD_eq_getElem?_getD, List.getElem?_zipWith, seedFacts]
  have sem : NodeSem I st.nodes := by
    intro x hx
    obtain ⟨k', hk'⟩ := List.getElem?_of_mem hx
    obtain ⟨ho, hc, -⟩ := shape_node hshape hk'
    rw [ho]
    exact classValue_node hk' hc
  have typedLt : ∀ m c n, c < m → st.sorts[c]? = some n → ∃ x, I c = .bits n x := by
    intro m
    induction m with
    | zero => intro c n h; omega
    | succ m ihm =>
      intro c n hcm hn
      have ih : ∀ c', c' < c → ∀ n, st.sorts[c']? = some n → ∃ x, I c' = .bits n x :=
        fun c' hc' n hn => ihm c' n (by omega) hn
      by_cases hcS : c < region.seeds.length
      · obtain ⟨s, hs⟩ : ∃ s, region.seeds[c]? = some s := ⟨_, List.getElem?_eq_getElem hcS⟩
        have : n = s.width := by
          rw [← seedSort c s hs]; simp [St.sort, List.getD_eq_getElem?_getD, hn]
        rw [hIseed c s hs, this]
        exact (seedValue_spec vals s _ (hfs c s hs)).1
      · have hlen := (List.getElem?_eq_some_iff.mp hn).1
        obtain ⟨k', rfl⟩ : ∃ k', c = region.seeds.length + k' := ⟨c - region.seeds.length, by omega⟩
        obtain ⟨nd', hnd'⟩ : ∃ x, st.nodes[k']? = some x :=
          ⟨_, List.getElem?_eq_getElem (by omega)⟩
        obtain ⟨-, hc, -, hsh⟩ := shape_node hshape hnd'
        have hsort : st.sort (region.seeds.length + k') = n := by
          simp [St.sort, List.getD_eq_getElem?_getD, hn]
        show ∃ x, classValue _ _ _ _ = _
        rw [classValue_node hnd' hc]
        rcases hsh with ⟨w', v', hop, hch, hw, hv, hs⟩ | ⟨o, hop, hin, hs⟩
        · rw [hop, hch, List.map_nil, nodeValue_constant hw hv, ← hsort, hs]
          exact ⟨_, rfl⟩
        · have hargs : Pairs (fun v w => ∃ x, v = .bits w x) (nd'.children.map I) (opInputs o) := by
            rw [hin]
            exact pairs_map2 fun ch hch => ih ch (hc ch hch) _ (sort_get (by have := hc ch hch; omega))
          obtain ⟨d, hd⟩ := typed_args o hargs
          rw [hop, hd, nodeValue_scalar, ← hsort, hs]
          exact ⟨_, rfl⟩
  have typed : Typed I st.sorts := fun c n hn => typedLt (c+1) c n (by omega) hn
  have cons : Cons I st := by
    refine replay_sound (ctx := ⟨b, region, rules, guards, seedFacts T fname b region⟩)
      (fun i => ?_) hrep (fun c p hp => ?_) sem typed
    · by_cases hi : i < region.seeds.length
      · obtain ⟨s, hs⟩ : ∃ s, region.seeds[i]? = some s := ⟨_, List.getElem?_eq_getElem hi⟩
        rw [hIseed i s hs]
        exact (seedValue_spec vals s _ (hfs i s hs)).2.1
      · simp only [List.getD_eq_getElem?_getD, seedFacts,
          List.getElem?_eq_none (by simpa using hi), List.getElem?_map]
        exact holds_default _
    · rw [hp0] at hp
      obtain ⟨_, e⟩ := List.getElem?_eq_some_iff.mp hp
      simp only [List.getElem_range] at e
      rw [e]
  have pureNode : ∀ (k' : Nat) (nd' : RNode), st.nodes[k']? = some nd' → isPure nd'.op = true := by
    intro k' nd' hn
    rcases (shape_node hshape hn).2.2.2 with ⟨_, _, h, _⟩ | ⟨_, h, _⟩ <;> rw [h] <;> rfl
  -- The operands of a witness that ran are the values of its child classes.
  have agree : ∀ k', k' < region.witnesses.length →
      (∀ k'', k'' < k' → ∀ id, (st.nodes[k'']?.bind (·.result)) = some id →
        vals.lookup id = some (I (region.seeds.length + k''))) →
      ∀ j inst nd' args' rs, region.witnesses[k']? = some j → b.instructions[j]? = some inst →
        st.nodes[k']? = some nd' → Bound vals inst.operands args' →
        evalOperation owner inst.op args' = .ok rs → args' = nd'.children.map I := by
    intro k' hk' ih j inst nd' args' rs hj hi hn hb he
    obtain ⟨j0, inst0, nd0, hj0, hi0, hn0, hop, -, hlen, hcls⟩ := witness_spec hwit hk'
    rw [hj, Option.some.injEq] at hj0; subst hj0
    rw [hi, Option.some.injEq] at hi0; subst hi0
    rw [hn, Option.some.injEq] at hn0; subst hn0
    refine args_eq hb hlen fun p v c hv hc => ?_
    obtain ⟨oc, hoc, hfo, hso⟩ := hcls p v c hv hc
    rw [← same_find cons hfo]
    rcases operandClass_spec hoc with ⟨s, hs, rfl⟩ | ⟨k'', hk'', rfl, hr⟩
    · rcases (shape_node hshape hn).2.2.2 with ⟨_, _, _, hch, _⟩ | ⟨o, hop', hin, _⟩
      · rw [hch] at hc; simp at hc
      · rw [hop, hop'] at he
        obtain ⟨y, hy⟩ := operand_bits hin hb he hv hc
        revert y
        rw [← hso, seedSort oc s hs]
        intro y hy
        rw [hIseed oc s hs, (seedValue_spec vals s _ (hfs oc s hs)).2.2 y hy, hy]
    · exact ih k'' hk'' v hr
  have hwv : ∀ m k', k' < m → k' < region.witnesses.length - 1 → ∀ id,
      (st.nodes[k']?.bind (·.result)) = some id →
        vals.lookup id = some (I (region.seeds.length + k')) := by
    intro m
    induction m with
    | zero => intro k' h; omega
    | succ m ihm =>
      intro k' hkm hk' id hid
      obtain ⟨j, inst, nd', hj, hi, hn, hop, hres, -, -⟩ :=
        witness_spec hwit (by omega : k' < region.witnesses.length)
      have hpure : isPure inst.op = true := hop ▸ pureNode k' nd' hn
      obtain ⟨args', v, r, hb, he, hr, hl⟩ := ran inst (List.mem_of_getElem?
        (by rw [List.getElem?_take_of_lt (hbefore k' hk' j hj)]; exact hi)) hpure
      have heq := agree k' (by omega) (fun k'' hk'' => ihm k'' (by omega) (by omega)) j inst nd'
        args' [v] hj hi hn hb he
      rw [hn, Option.bind_some] at hid
      rw [hr, hid] at hres
      simp only [List.map, Option.toList, List.cons.injEq, and_true] at hres
      rw [← hres, hl, ← (shape_node hshape hn).1, sem nd' (List.mem_of_getElem? hn), ← heq, ← hop,
        eval_node hpure he]
  have hall : ∀ k'', k'' < region.witnesses.length - 1 → ∀ id,
      (st.nodes[k'']?.bind (·.result)) = some id →
        vals.lookup id = some (I (region.seeds.length + k'')) :=
    fun k'' h => hwv (k''+1) k'' (by omega) h
  -- The root.
  obtain ⟨j, inst, nd', hj, hi, hn, hop, -, -, -⟩ := witness_spec hwit hwpos
  rw [hwl, Option.some.injEq] at hj
  rw [← hj, hold, Option.some.injEq] at hi
  rw [hrn, Option.some.injEq] at hn
  rw [← hi, ← hn] at hop
  have pold : isPure old.op = true := hop ▸ pureNode _ _ hrn
  have hargsI := agree _ hwpos (fun k'' h => hall k'' h) _ _ _ _ _ hwl hold hrn hargs heval
  obtain ⟨v0, rfl⟩ := pure_result pold heval
  have hv0 : I rn.owner = v0 := by
    rw [sem rn (List.mem_of_getElem? hrn), ← hargsI, ← hop, eval_node pold heval]
  -- Seeds that the extraction selects.
  have seedAgree : ∀ i s, region.seeds[i]? = some s →
      region.seeds.findIdx? (·.value == s.value) = some i → vals.lookup s.value = some (I i) := by
    intro i s hs hcan
    simp only [cleanRegion, Bool.and_eq_true, List.all_eq_true, List.any_eq_true,
      List.mem_filterMap] at clean
    obtain ⟨-, x, ⟨jx, hjx, hix⟩, hcont⟩ := (clean.1.1.2 s (List.mem_of_getElem? hs))
    obtain ⟨k'', hk''⟩ := List.getElem?_of_mem hjx
    obtain ⟨p, hp⟩ := List.getElem?_of_mem (List.contains_iff_mem.mp hcont)
    have hk''l := (List.getElem?_eq_some_iff.mp hk'').1
    obtain ⟨j1, inst1, nd1, hj1, hi1, hn1, hop1, -, hlen1, hcls1⟩ := witness_spec hwit hk''l
    rw [hk'', Option.some.injEq] at hj1; subst hj1
    rw [hix, Option.some.injEq] at hi1; subst hi1
    obtain ⟨c, hc⟩ : ∃ c, nd1.children[p]? = some c :=
      ⟨_, List.getElem?_eq_getElem (by rw [← hlen1]; exact (List.getElem?_eq_some_iff.mp hp).1)⟩
    obtain ⟨oc, hoc, -, hso⟩ := hcls1 p _ c hp hc
    simp only [operandClass, hcan, Option.some.injEq] at hoc
    subst hoc
    obtain ⟨args1, rs1, hb1, he1⟩ : ∃ args1 rs1, Bound vals x.operands args1 ∧
        evalOperation owner x.op args1 = .ok rs1 := by
      by_cases hlt : k'' < region.witnesses.length - 1
      · obtain ⟨a1, v1, -, hb1, he1, -⟩ := ran x (List.mem_of_getElem?
          (by rw [List.getElem?_take_of_lt (hbefore k'' hlt jx hk'')]; exact hix)) (hop1 ▸ pureNode _ _ hn1)
        exact ⟨a1, _, hb1, he1⟩
      · have : k'' = region.witnesses.length - 1 := by omega
        subst this
        rw [hwl, Option.some.injEq] at hk''
        subst hk''
        rw [hold, Option.some.injEq] at hix
        subst hix
        exact ⟨_, _, hargs, heval⟩
    rcases (shape_node hshape hn1).2.2.2 with ⟨_, _, _, hch, _⟩ | ⟨o, hop', hin, _⟩
    · rw [hch] at hc; simp at hc
    · rw [hop1, hop'] at he1
      obtain ⟨y, hy⟩ := operand_bits hin hb1 he1 hp hc
      revert y
      rw [← hso, seedSort i s hs]
      intro y hy
      rw [hIseed i s hs, (seedValue_spec vals s _ (hfs i s hs)).2.2 y hy, hy]
  -- The replacement instruction.
  have hops' : Bound vals ops (nd.children.map I) := by
    refine pairs_flip (fun ch id he => ?_) (mapM_pairs hops)
    unfold existing at he
    simp only [bind_some, guard_unit, Bool.and_eq_true, exists_and_left, exists_const,
      beq_iff_eq, bne_iff_ne, ne_eq] at he
    obtain ⟨⟨c', choice⟩, -, he⟩ := he
    rcases choice with i | k'' <;> simp only [bind_some, guard_unit, Bool.and_eq_true,
      exists_and_left, exists_const, bne_iff_ne, ne_eq] at he
    · obtain ⟨s, hs, ⟨hfi, hcan⟩, hid⟩ := he
      cases hid
      rw [seedAgree i s hs hcan, same_find cons hfi]
    · obtain ⟨nd'', hn'', ⟨hk'', hfo⟩, hid⟩ := he
      have hlt : k'' < region.witnesses.length - 1 := by
        have := ((shape_node hshape hn'').2.2.1).mpr (by rw [hid]; rfl)
        omega
      rw [hall k'' hlt id (by rw [hn'', Option.bind_some, hid]), ← (shape_node hshape hn'').1,
        same_find cons hfo]
  refine ⟨_, hops', ?_⟩
  have hlt : nd.owner < st.sorts.length := by
    have := (List.getElem?_eq_some_iff.mp hnd).1
    rw [(shape_node hshape hnd).1]; omega
  obtain ⟨x, hx⟩ := typed _ _ (sort_get hlt)
  have hev := nodeValue_ok (hx ▸ (sem nd (List.mem_of_getElem? hnd)).symm)
  rw [pure_owner (o' := 0) pop, hev, ← hx, same_find cons (hnf.trans hrf.symm), hv0]

end EvoIR.Rewrite
