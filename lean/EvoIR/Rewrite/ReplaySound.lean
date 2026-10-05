import EvoIR.Rewrite.Replay

/-!
Soundness of the region replay. An interpretation `I` gives a value to each
class. `Cons` says that each class has the value of its parent. A union is
added only when a rule instance or a congruence step proves the two values
equal, so `Cons` stays true through the replay.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR.Rewrite

open EvoIR.Core.IR

/-- The value of a node, from the values of its children. -/
def nodeValue (op : Operation) (args : List Value) : Value :=
  match evalOperation 0 op args with
  | .ok [v] => v
  | _ => .poison (.bits 0)

def Cons (I : Nat → Value) (st : St) : Prop := ∀ c p, st.parent[c]? = some p → I c = I p

def NodeSem (I : Nat → Value) (nodes : List RNode) : Prop :=
  ∀ nd ∈ nodes, I nd.owner = nodeValue nd.op (nd.children.map I)

def Typed (I : Nat → Value) (sorts : List Nat) : Prop :=
  ∀ c n, sorts[c]? = some n → ∃ x, I c = .bits n x

def FSound (I : Nat → Value) (fs : List RFact) : Prop := ∀ c, Holds (fs.getD c default) (I c)

theorem holds_default (v : Value) : Holds default v := by
  cases v with
  | bits n x => intro _; exact gamma_mask_zero _ x
  | _ => trivial

theorem holds_poison (a : RFact) (t : Ty) : Holds a (.poison t) := trivial

/-! ## Partition -/

theorem findF_sound {I : Nat → Value} {parent : List Nat}
    (h : ∀ c p, parent[c]? = some p → I c = I p) : ∀ k c, I (findF parent k c) = I c
  | 0, _ => rfl
  | k+1, c => by
    simp only [findF]
    split
    · rename_i p hp
      split
      · rfl
      · rw [findF_sound h k p, h c p hp]
    · rfl

theorem find_sound {I : Nat → Value} {st : St} (h : Cons I st) (c : Nat) : I (st.find c) = I c :=
  findF_sound h _ c

theorem same_find {I : Nat → Value} {st : St} (h : Cons I st) {a b : Nat}
    (e : st.find a = st.find b) : I a = I b := by
  rw [← find_sound h a, ← find_sound h b, e]

theorem cons_union {I : Nat → Value} {st : St} {l r : Nat} (h : Cons I st) (e : I l = I r) :
    Cons I (st.union l r) := by
  intro c p hp
  simp only [St.union, List.getElem?_set] at hp
  split at hp
  · rename_i hc
    split at hp
    · cases hp
      subst hc
      by_cases hlr : l ≤ r
      · simp [Nat.max_def, Nat.min_def, hlr, e]
      · simp [Nat.max_def, Nat.min_def, hlr, e]
    · cases hp
  · exact h c p hp

theorem union_nodes (st : St) (l r : Nat) : (st.union l r).nodes = st.nodes := rfl
theorem union_sorts (st : St) (l r : Nat) : (st.union l r).sorts = st.sorts := rfl

/-! ## Class facts -/

theorem setFact_sound {I : Nat → Value} {fs fs' : List RFact} {c : Nat} {t : RFact}
    (h : FSound I fs) (ht : Holds t (I c)) (e : setFact fs c t = some fs') : FSound I fs' := by
  unfold setFact at e
  cases hold : fs[c]? with
  | none => simp [hold, Bind.bind, Option.bind] at e
  | some old =>
    cases hm : old.meet t with
    | none => simp [hold, hm, Bind.bind, Option.bind] at e
    | some m =>
      simp only [hold, hm, Bind.bind, Option.bind, Option.some.injEq] at e
      subst e
      intro d
      by_cases hd : c = d
      · subst hd
        have hlen : c < fs.length := (List.getElem?_eq_some_iff.mp hold).1
        have hc := h c
        simp only [List.getD_eq_getElem?_getD, hold, Option.getD_some] at hc
        simp only [List.getD_eq_getElem?_getD, List.getElem?_set_self hlen, Option.getD_some]
        exact holds_meet hc ht hm
      · simp only [List.getD_eq_getElem?_getD, List.getElem?_set_ne hd]
        simpa only [List.getD_eq_getElem?_getD] using h d

theorem evalOperation_constant {o w v : Nat} {args : List Value} {rs : List Value}
    (h : evalOperation o (.constant w v) args = .ok rs) :
    args = [] ∧ rs = [.bits w (BitVec.ofNat w v)] := by
  match args, h with
  | [], h =>
    simp only [evalOperation] at h
    split at h
    · cases h
    · exact ⟨rfl, (Except.ok.inj h).symm⟩

theorem evalOperation_scalar {o : Nat} {op : Op} {args rs : List Value}
    (h : evalOperation o (.scalar op) args = .ok rs) :
    ∃ r, evalScalar op args = .ok r ∧ rs = [r] := by
  simp only [evalOperation] at h
  cases he : evalScalar op args with
  | error e => simp [he, Bind.bind, Except.bind] at h
  | ok r =>
    simp [he, Bind.bind, Except.bind, Pure.pure, Except.pure] at h
    exact ⟨r, rfl, h.symm⟩

theorem nodeValue_scalar (op : Op) (d : op.dom) :
    nodeValue (.scalar op) (opValues op d) = .bits op.res (denoteOp op d) := by
  simp [nodeValue, evalOperation, evalScalar_typed, Functor.map, Except.map]

theorem nodeValue_constant {w v : Nat} (hw : w ≠ 0) (hv : v < 2 ^ w) :
    nodeValue (.constant w v) [] = .bits w (BitVec.ofNat w v) := by
  have : ¬ (w == 0 || decide (v ≥ 2 ^ w)) = true := by simp [hw, hv]
  simp [nodeValue, evalOperation, hw, Nat.not_le.mpr hv, pure, Except.pure]

theorem nodeFact_sound {I : Nat → Value} {st : St} {fs : List RFact} {nd : RNode}
    (cons : Cons I st) (sem : I nd.owner = nodeValue nd.op (nd.children.map I)) (h : FSound I fs) :
    Holds (nodeFact st fs nd) (I nd.owner) := by
  rw [sem]
  cases hop : nd.op
  case constant w v =>
    simp only [nodeFact, hop, nodeValue]
    split
    · rename_i x hx
      obtain ⟨-, hr⟩ := evalOperation_constant hx
      cases hr
      exact holds_const w v
    · trivial
  case scalar op =>
    simp only [nodeFact, hop, nodeValue]
    split
    · rename_i x hx
      obtain ⟨r, hr, e⟩ := evalOperation_scalar hx
      cases e
      apply rawTransfer_sound hr
      intro i v hi
      simp only [List.getElem?_map] at hi
      cases hc : nd.children[i]? with
      | none => simp [hc] at hi
      | some c =>
        simp only [hc, Option.map_some, Option.some.injEq] at hi
        subst hi
        simp only [List.getD_eq_getElem?_getD, List.getElem?_map, hc, Option.map_some,
          Option.getD_some]
        have := h (st.find c)
        rw [find_sound cons] at this
        simpa only [List.getD_eq_getElem?_getD] using this
    · trivial
  all_goals
    simp only [nodeFact, hop, nodeValue]
    split <;> first | trivial | exact holds_default _

theorem foldlM_sound {α β : Type} {P : α → Prop} {f : α → β → Option α} :
    ∀ (l : List β) (a b : α), (∀ a a' x, x ∈ l → P a → f a x = some a' → P a') →
      P a → l.foldlM f a = some b → P b
  | [], a, b, _, ha, e => by cases e; exact ha
  | x :: l, a, b, step, ha, e => by
    simp only [List.foldlM_cons, Bind.bind, Option.bind] at e
    split at e
    · cases e
    · rename_i a' h'
      exact foldlM_sound l a' b (fun a a'' y hy => step a a'' y (List.mem_cons_of_mem _ hy))
        (step a a' x List.mem_cons_self ha h') e

theorem factPass_sound {I : Nat → Value} {st : St} {fs fs' : List RFact}
    (cons : Cons I st) (sem : NodeSem I st.nodes) (h : FSound I fs)
    (e : factPass st fs = some fs') : FSound I fs' := by
  refine foldlM_sound st.nodes fs fs' ?_ h e
  intro a a' nd hnd ha hs
  refine setFact_sound ha ?_ hs
  rw [find_sound cons]
  exact nodeFact_sound cons (sem nd hnd) ha

theorem classFacts_sound {I : Nat → Value} {ctx : Ctx} {st : St} {fs : List RFact}
    (cons : Cons I st) (sem : NodeSem I st.nodes)
    (seeds : ∀ i, Holds (ctx.seedFacts.getD i default) (I i))
    (e : classFacts ctx st = some fs) : FSound I fs := by
  unfold classFacts at e
  simp only [Bind.bind, Option.bind] at e
  split at e
  · cases e
  · rename_i fs0 h0
    have s0 : FSound I fs0 := by
      refine foldlM_sound _ _ _ ?_ ?_ h0
      · rintro a a' ⟨f, i⟩ hx ha hs
        refine setFact_sound ha ?_ hs
        rw [find_sound cons]
        have hi := List.mem_zipIdx_iff_getElem?.mp hx
        have := seeds i
        simpa only [List.getD_eq_getElem?_getD, hi, Option.getD_some] using this
      · intro c
        simp only [List.getD_eq_getElem?_getD, List.getElem?_map]
        cases (st.sorts[c]?) with
        | none => exact holds_default _
        | some w => exact holds_top w _
    exact foldlM_sound _ _ _ (fun a a' _ _ ha hs => factPass_sound cons sem ha hs) s0 e

/-! ## Option helpers -/

theorem bind_some {α β : Type} {o : Option α} {f : α → Option β} {b : β} :
    (o >>= f) = some b ↔ ∃ a, o = some a ∧ f a = some b := Option.bind_eq_some_iff

theorem guard_some {p : Prop} [Decidable p] {β : Type} {f : Unit → Option β} {b : β} :
    (guard p >>= f) = some b ↔ p ∧ f () = some b := by
  by_cases hp : p
  · simp [guard, hp]
  · simp only [guard, hp, if_false, false_and, iff_false]
    exact fun h => by cases h

theorem guard_unit {p : Prop} [Decidable p] {u : Unit} : (guard p : Option Unit) = some u ↔ p := by
  by_cases hp : p <;> simp [guard, hp]

/-! ## Values of classes -/

def bitsAt (v : Value) (n : Nat) : BitVec n :=
  match v with
  | .bits m x => if h : m = n then h ▸ x else 0
  | _ => 0

theorem bitsAt_bits {n : Nat} (x : BitVec n) : bitsAt (.bits n x) n = x := by simp [bitsAt]

theorem bits_cast {m n : Nat} (h : m = n) (x : BitVec m) :
    Value.bits n (h ▸ x) = Value.bits m x := by
  subst h; rfl

theorem bits_inj {n : Nat} {x y : BitVec n} (h : Value.bits n x = Value.bits n y) : x = y := by
  cases h; rfl

/-- The substitution a rule instance is applied under. -/
def sub (I : Nat → Value) (st : St) (σ : List (String × Nat)) : Subst (widths st σ) :=
  fun v => bitsAt (I (bound σ v)) (widths st σ v)

theorem sub_spec {I : Nat → Value} {st : St} {σ : List (String × Nat)} {v : Var}
    (typed : Typed I st.sorts) (hb : bound σ v < st.sorts.length) :
    I (bound σ v) = .bits (widths st σ v) (sub I st σ v) := by
  obtain ⟨n, hn⟩ : ∃ n, st.sorts[bound σ v]? = some n := ⟨_, List.getElem?_eq_getElem hb⟩
  obtain ⟨x, hx⟩ := typed _ _ hn
  have hw : widths st σ v = n := by simp [widths, St.sort, List.getD_eq_getElem?_getD, hn]
  unfold sub
  rw [hx, hw, bitsAt_bits]

theorem class_node {I : Nat → Value} {st : St} {c : Nat} {nd : RNode} (cons : Cons I st)
    (sem : NodeSem I st.nodes) (hm : nd ∈ st.nodes) (hf : st.find nd.owner = st.find c) :
    I c = nodeValue nd.op (nd.children.map I) := by
  rw [← same_find cons hf]
  exact sem nd hm

theorem constNode_spec {I : Nat → Value} {st : St} {c w v : Nat} {nd : RNode} (cons : Cons I st)
    (sem : NodeSem I st.nodes) (hm : nd ∈ st.nodes) (h : constNode st c w nd = some v) :
    I c = .bits w (BitVec.ofNat w v) := by
  unfold constNode at h
  split at h
  · rename_i w' v' hop
    split at h
    · rename_i hc
      cases h
      simp only [Bool.and_eq_true, beq_iff_eq, bne_iff_ne, ne_eq, decide_eq_true_eq,
        List.isEmpty_iff] at hc
      obtain ⟨⟨⟨⟨hw, hz⟩, hv⟩, hch⟩, hf⟩ := hc
      subst hw
      rw [class_node cons sem hm hf, hop, hch]
      exact nodeValue_constant hz hv
    · cases h
  · cases h

theorem hasConst_spec {I : Nat → Value} {st : St} {c w : Nat} {ok : Nat → Bool}
    (cons : Cons I st) (sem : NodeSem I st.nodes) (h : hasConst st c w ok = true) :
    ∃ v, ok v = true ∧ I c = .bits w (BitVec.ofNat w v) := by
  simp only [hasConst, List.any_eq_true] at h
  obtain ⟨nd, hm, hk⟩ := h
  split at hk
  · rename_i v hv
    exact ⟨v, hk, constNode_spec cons sem hm hv⟩
  · cases hk

theorem constIn_spec {I : Nat → Value} {st : St} {c w v : Nat} (cons : Cons I st)
    (sem : NodeSem I st.nodes) (h : constIn st c w = some v) : I c = .bits w (BitVec.ofNat w v) := by
  obtain ⟨nd, hm, hv⟩ := List.exists_of_findSome?_eq_some h
  exact constNode_spec cons sem hm hv

theorem hasOp_spec {st : St} {c : Nat} {o : Op} {k : List Nat → Bool}
    (h : hasOp st c o k = true) :
    ∃ nd ∈ st.nodes, nd.op = .scalar o ∧ st.find nd.owner = st.find c ∧ k nd.children = true := by
  simp only [hasOp, List.any_eq_true, Bool.and_eq_true, beq_iff_eq] at h
  obtain ⟨nd, hm, ⟨hop, hf⟩, hk⟩ := h
  exact ⟨nd, hm, hop, hf, hk⟩

theorem two_spec {k : Nat → Nat → Bool} {l : List Nat} (h : two k l = true) :
    ∃ x y, l = [x, y] ∧ k x y = true := by
  match l, h with
  | [x, y], h => exact ⟨x, y, rfl, h⟩

theorem one_spec {k : Nat → Bool} {l : List Nat} (h : one k l = true) : ∃ x, l = [x] ∧ k x = true := by
  match l, h with
  | [x], h => exact ⟨x, rfl, h⟩

theorem denote_some {Γ : Var → Nat} {σ : Subst Γ} {n : Nat} {p : Pat} {v : Value}
    (h : (denotePat Γ σ n p).map (Value.bits n) = some v) :
    ∃ t, elabPat Γ n p = some t ∧ v = .bits n (denoteTerm σ t) := by
  simp only [denotePat, Option.map_map, Option.map_eq_some_iff, Function.comp] at h
  obtain ⟨t, ht, rfl⟩ := h
  exact ⟨t, ht, rfl⟩

theorem denote_var {Γ : Var → Nat} {σ : Subst Γ} {n : Nat} {v : Var} (h : Γ v = n) :
    (denotePat Γ σ n (.var v)).map (Value.bits n) = some (Value.bits (Γ v) (σ v)) := by
  simp only [denotePat, elabPat, h, dite_true, Option.map_some, denoteTerm_var]
  rw [bits_cast]

/-! ## Constant folds -/

theorem foldConst_sound {I : Nat → Value} {st : St} {σ : List (String × Nat)}
    (cons : Cons I st) (sem : NodeSem I st.nodes) :
    ∀ p n x, foldConst st σ p n = some x →
      (denotePat (widths st σ) (sub I st σ) n p).map (Value.bits n) =
        some (.bits n (BitVec.ofNat n x)) := by
  intro p
  induction p with
  | «var» v =>
    intro n x h
    simp only [foldConst] at h
    split at h
    · rename_i hw
      have hw : widths st σ v = n := by simpa using hw
      rw [denote_var hw]
      have hc := constIn_spec cons sem h
      subst hw
      simp only [sub, hc, bitsAt_bits]
    · cases h
  | constLike v k =>
    intro n x h
    simp only [foldConst] at h
    split at h
    · rename_i hw
      have hw : widths st σ v = n := by simpa using hw
      cases h
      simp only [denotePat, elabPat, hw, if_true, Option.map_some, denoteTerm_const,
        Option.some.injEq, BitVec.ofNat_toNat, BitVec.setWidth_eq]
    · cases h
  | bin o f a b iha ihb =>
    intro n x h
    simp only [foldConst, bind_some] at h
    obtain ⟨xa, ha, xb, hb, hx⟩ := h
    simp only [Option.some.injEq] at hx
    subst hx
    obtain ⟨ta, eta, va⟩ := denote_some (iha n xa ha)
    obtain ⟨tb, etb, vb⟩ := denote_some (ihb n xb hb)
    have va := bits_inj va
    have vb := bits_inj vb
    simp [denotePat, elabPat, eta, etb, ← va, ← vb]
    exact (BitVec.setWidth_eq (n := n) _).symm
  | cmp o f a b iha ihb =>
    intro n x h
    match n, h with
    | 1, h =>
      simp only [foldConst] at h
      split at h
      · rename_i w hw
        simp only [bind_some] at h
        obtain ⟨xa, ha, xb, hb, hx⟩ := h
        simp only [Option.some.injEq] at hx
        subst hx
        obtain ⟨ta, eta, va⟩ := denote_some (iha w xa ha)
        obtain ⟨tb, etb, vb⟩ := denote_some (ihb w xb hb)
        have va := bits_inj va
        have vb := bits_inj vb
        simp [denotePat, elabPat, hw, eta, etb, ← va, ← vb]
        exact (BitVec.setWidth_eq (n := 1) _).symm
      · cases h
  | _ => intro n x h; simp [foldConst] at h

/-! ## Pattern matching -/

theorem node2 {I : Nat → Value} {st : St} {c x y : Nat} {nd : RNode} {o : Op} (d : o.dom)
    (cons : Cons I st) (sem : NodeSem I st.nodes) (hm : nd ∈ st.nodes) (hop : nd.op = .scalar o)
    (hf : st.find nd.owner = st.find c) (hxy : nd.children = [x, y]) (hv : [I x, I y] = opValues o d) :
    I c = .bits o.res (denoteOp o d) := by
  rw [class_node cons sem hm hf, hop, hxy]
  simp only [List.map]
  rw [hv, nodeValue_scalar]

theorem node1 {I : Nat → Value} {st : St} {c x : Nat} {nd : RNode} {o : Op} (d : o.dom)
    (cons : Cons I st) (sem : NodeSem I st.nodes) (hm : nd ∈ st.nodes) (hop : nd.op = .scalar o)
    (hf : st.find nd.owner = st.find c) (hx : nd.children = [x]) (hv : [I x] = opValues o d) :
    I c = .bits o.res (denoteOp o d) := by
  rw [class_node cons sem hm hf, hop, hx]
  simp only [List.map]
  rw [hv, nodeValue_scalar]

theorem matchPat_sound {I : Nat → Value} {st : St} {σ : List (String × Nat)}
    (cons : Cons I st) (sem : NodeSem I st.nodes) (typed : Typed I st.sorts) :
    ∀ p n c, matchPat st σ p n c = true →
      (denotePat (widths st σ) (sub I st σ) n p).map (Value.bits n) = some (I c) := by
  intro p
  induction p with
  | «var» v =>
    intro n c h
    simp only [matchPat, Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] at h
    obtain ⟨⟨⟨_, hb⟩, hf⟩, hw⟩ := h
    rw [denote_var hw, ← sub_spec typed hb, same_find cons hf]
  | constLike v k =>
    intro n c h
    simp only [matchPat, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨hw, hc⟩ := h
    obtain ⟨x, hx, hI⟩ := hasConst_spec cons sem hc
    simp only [beq_iff_eq] at hx
    simp [denotePat, elabPat, hw, hI, hx]
  | boolLit k =>
    intro n c h
    simp only [matchPat, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨rfl, hc⟩ := h
    obtain ⟨x, hx, hI⟩ := hasConst_spec cons sem hc
    simp only [beq_iff_eq] at hx
    simp [denotePat, elabPat, hI, hx]
  | bin o f a b iha ihb =>
    intro n c h
    simp only [matchPat, Bool.or_eq_true] at h
    rcases h with h | h
    · obtain ⟨nd, hm, hop, hf, hk⟩ := hasOp_spec h
      obtain ⟨x, y, hxy, hk⟩ := two_spec hk
      simp only [Bool.and_eq_true] at hk
      obtain ⟨ta, eta, va⟩ := denote_some (iha n x hk.1)
      obtain ⟨tb, etb, vb⟩ := denote_some (ihb n y hk.2)
      rw [node2 (o := .bin o n) (denoteTerm _ ta, denoteTerm _ tb) cons sem hm hop hf hxy
        (by rw [va, vb]; rfl)]
      simp [denotePat, elabPat, eta, etb, Op.res]
    · simp only [Bool.and_eq_true] at h
      obtain ⟨-, h⟩ := h
      split at h
      · rename_i x0 hx0
        obtain ⟨y, hy, hI⟩ := hasConst_spec cons sem h
        simp only [beq_iff_eq] at hy
        rw [foldConst_sound cons sem _ _ _ hx0, hI, hy]
      · cases h
  | cmp o f a b iha ihb =>
    intro n c h
    simp only [matchPat, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨rfl, h⟩ := h
    split at h
    · rename_i w hw
      simp only [Bool.or_eq_true] at h
      rcases h with h | h
      · obtain ⟨nd, hm, hop, hf, hk⟩ := hasOp_spec h
        obtain ⟨x, y, hxy, hk⟩ := two_spec hk
        simp only [Bool.and_eq_true] at hk
        obtain ⟨ta, eta, va⟩ := denote_some (iha w x hk.1)
        obtain ⟨tb, etb, vb⟩ := denote_some (ihb w y hk.2)
        rw [node2 (o := .cmp o w) (denoteTerm _ ta, denoteTerm _ tb) cons sem hm hop hf hxy
          (by rw [va, vb]; rfl)]
        simp [denotePat, elabPat, hw, eta, etb, Op.res]
      · simp only [Bool.and_eq_true] at h
        obtain ⟨-, h⟩ := h
        split at h
        · rename_i x0 hx0
          obtain ⟨y, hy, hI⟩ := hasConst_spec cons sem h
          simp only [beq_iff_eq] at hy
          rw [foldConst_sound cons sem _ _ _ hx0, hI, hy]
        · cases h
    · cases h
  | shift o a k iha ihk =>
    intro n c h
    simp only [matchPat] at h
    split at h
    · rename_i m hmw
      obtain ⟨nd, hm, hop, hf, hk⟩ := hasOp_spec h
      obtain ⟨x, y, hxy, hk⟩ := two_spec hk
      simp only [Bool.and_eq_true] at hk
      obtain ⟨ta, eta, va⟩ := denote_some (iha n x hk.1)
      obtain ⟨tk, etk, vk⟩ := denote_some (ihk m y hk.2)
      rw [node2 (o := .shift o n m) (denoteTerm _ ta, denoteTerm _ tk) cons sem hm hop hf hxy
        (by rw [va, vk]; rfl)]
      simp [denotePat, elabPat, hmw, eta, etk, Op.res]
    · cases h
  | cast o a iha =>
    intro n c h
    simp only [matchPat] at h
    split at h
    · rename_i w hw
      obtain ⟨nd, hm, hop, hf, hk⟩ := hasOp_spec h
      obtain ⟨x, hx, hk⟩ := one_spec hk
      obtain ⟨ta, eta, va⟩ := denote_some (iha w x hk)
      rw [node1 (o := .cast o w n) (denoteTerm _ ta) cons sem hm hop hf hx (by rw [va]; rfl)]
      simp [denotePat, elabPat, hw, eta, Op.res]
    · cases h
  | guard o a b iha ihb =>
    intro n c h
    simp only [matchPat, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨rfl, h⟩ := h
    split at h
    · rename_i w hw
      obtain ⟨nd, hm, hop, hf, hk⟩ := hasOp_spec h
      obtain ⟨x, y, hxy, hk⟩ := two_spec hk
      simp only [Bool.and_eq_true] at hk
      obtain ⟨ta, eta, va⟩ := denote_some (iha w x hk.1)
      obtain ⟨tb, etb, vb⟩ := denote_some (ihb w y hk.2)
      rw [node2 (o := .guard o w) (denoteTerm _ ta, denoteTerm _ tb) cons sem hm hop hf hxy
        (by rw [va, vb]; rfl)]
      simp [denotePat, elabPat, hw, eta, etb, Op.res, denoteTerm]
    · cases h
  | nonzero a iha =>
    intro n c h
    simp only [matchPat, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨rfl, h⟩ := h
    split at h
    · rename_i w hw
      obtain ⟨nd, hm, hop, hf, hk⟩ := hasOp_spec h
      obtain ⟨x, hx, hk⟩ := one_spec hk
      obtain ⟨ta, eta, va⟩ := denote_some (iha w x hk)
      rw [node1 (o := .nonzero w) (denoteTerm _ ta) cons sem hm hop hf hx (by rw [va]; rfl)]
      simp [denotePat, elabPat, hw, eta, Op.res, denoteTerm]
    · cases h
  | count s a k iha ihk =>
    intro n c h
    simp only [matchPat, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨rfl, h⟩ := h
    split at h
    · rename_i w m hw hmw
      obtain ⟨nd, hm, hop, hf, hk⟩ := hasOp_spec h
      obtain ⟨x, y, hxy, hk⟩ := two_spec hk
      simp only [Bool.and_eq_true] at hk
      obtain ⟨ta, eta, va⟩ := denote_some (iha w x hk.1)
      obtain ⟨tk, etk, vk⟩ := denote_some (ihk m y hk.2)
      rw [node2 (o := .count s w m) (denoteTerm _ ta, denoteTerm _ tk) cons sem hm hop hf hxy
        (by rw [va, vk]; rfl)]
      simp [denotePat, elabPat, hw, hmw, eta, etk, Op.res, denoteTerm]
    · cases h

/-! ## Rule instances and congruence -/

theorem condCheck_sound {Γ : Var → Nat} {σ : Subst Γ} {F : FactEnv Γ} {out : Nat} {R : Fact out} :
    ∀ c, condCheck Γ F R c = true → CondHolds Γ σ F R c
  | .tt, _ => trivial
  | .allConst, _ => trivial
  | .fullyKnown, h => by simp only [condCheck] at h; simp only [CondHolds]; exact of_decide_eq_true h
  | .covers m x, h => by
    intro e
    simp only [condCheck] at h
    rw [dif_pos e] at h
    exact of_decide_eq_true h
  | .knownZero _, h => by simp only [condCheck] at h; simp only [CondHolds]; exact of_decide_eq_true h
  | .widthMin _ _, h => by simp only [condCheck] at h; simp only [CondHolds]; exact of_decide_eq_true h

theorem premise_holds {I : Nat → Value} {st : St} {fs : List RFact} {p : Premise}
    (cons : Cons I st) (hf : FSound I fs) (h : premiseOk st fs p = true) :
    Holds (premiseFact st p) (I p.cls) := by
  simp only [premiseOk, Bool.and_eq_true] at h
  have := hf (st.find p.cls)
  rw [find_sound cons] at this
  exact holds_le this h.2

theorem premiseFor_holds {I : Nat → Value} {st : St} {fs : List RFact} {ps : List Premise} {c : Nat}
    (cons : Cons I st) (hf : FSound I fs) (h : ps.all (premiseOk st fs) = true) :
    Holds (premiseFor st ps c) (I c) := by
  unfold premiseFor
  split
  · rename_i p hp
    have hm := List.mem_of_find?_eq_some hp
    have heq := List.find?_some hp
    rw [← same_find cons (beq_iff_eq.mp heq)]
    exact premise_holds cons hf (List.all_eq_true.mp h p hm)
  · exact holds_default _

theorem lookup_mem {σ : List (String × Nat)} {v : String} {c : Nat} (h : σ.lookup v = some c) :
    (v, c) ∈ σ := by
  induction σ with
  | nil => simp at h
  | cons x xs ih =>
    obtain ⟨a, b⟩ := x
    simp only [List.lookup] at h
    split at h
    · rename_i hab
      cases h
      have : v = a := by simpa using hab
      subst this
      exact List.mem_cons_self
    · exact List.mem_cons_of_mem _ (ih h)

theorem factEnv_gamma {I : Nat → Value} {st : St} {σ : List (String × Nat)} {ps : List Premise}
    {fs : List RFact} (cons : Cons I st) (hf : FSound I fs) (typed : Typed I st.sorts)
    (hps : ps.all (premiseOk st fs) = true) (hb : σ.all (·.2 < st.sorts.length) = true) (v : Var) :
    gamma (factEnv st σ ps v) (sub I st σ v) := by
  unfold factEnv
  split
  · rename_i c hc
    have hbv : bound σ v = c := by simp [bound, hc]
    have hlt : bound σ v < st.sorts.length := by
      rw [hbv]
      exact of_decide_eq_true (List.all_eq_true.mp hb _ (lookup_mem hc))
    apply holds_at
    rw [← sub_spec typed hlt, hbv]
    exact premiseFor_holds cons hf hps
  · simp [gamma, Fact.top]

theorem rhsOk_sound {I : Nat → Value} {st : St} {σ : List (String × Nat)} {rhs : Rhs} {out c : Nat}
    {R : Fact out} (cons : Cons I st) (sem : NodeSem I st.nodes) (typed : Typed I st.sorts)
    (h : rhsOk st σ rhs out c R = true) :
    (denoteRhs (widths st σ) (sub I st σ) R rhs).map (Value.bits out) = some (I c) := by
  cases rhs with
  | pat p => exact matchPat_sound cons sem typed p out c h
  | knownConst =>
    obtain ⟨v, hv, hI⟩ := hasConst_spec cons sem h
    simp only [beq_iff_eq] at hv
    simp [denoteRhs, hI, hv]

theorem rewriteStep_sound {I : Nat → Value} {ctx : Ctx} {st st' : St} {rw : RewriteRec}
    (cons : Cons I st) (sem : NodeSem I st.nodes) (typed : Typed I st.sorts)
    (seeds : ∀ i, Holds (ctx.seedFacts.getD i default) (I i))
    (h : rewriteStep ctx st rw = some st') : Cons I st' := by
  unfold rewriteStep at h
  simp only [bind_some, guard_unit, Bool.and_eq_true, exists_and_left, exists_const] at h
  obtain ⟨-, r, -, fs, hfs, ⟨hps, hbs⟩, hl, hrhs, hc, hst⟩ := h
  simp only [Option.some.injEq] at hst
  subst hst
  have hf := classFacts_sound cons sem seeds hfs
  obtain ⟨tl, etl, el⟩ := denote_some (matchPat_sound cons sem typed _ _ _ hl)
  have gR : gamma ((premiseFor st rw.premises rw.lhs).at (st.sort rw.lhs))
      (denoteTerm (sub I st rw.bindings) tl) := by
    apply holds_at
    rw [← el]
    exact premiseFor_holds cons hf hps
  have rule := r.2 (widths st rw.bindings) (st.sort rw.lhs) (sub I st rw.bindings)
    (factEnv st rw.bindings rw.premises) _ (denoteTerm (sub I st rw.bindings) tl)
    (by simp [denotePat, etl]) (factEnv_gamma cons hf typed hps hbs) gR
    (condCheck_sound _ hc)
  have hrv := rhsOk_sound cons sem typed hrhs
  rw [rule, Option.map_some] at hrv
  exact cons_union cons (el.trans (Option.some.inj hrv))

theorem map_find_eq {I : Nat → Value} {st : St} (cons : Cons I st) :
    ∀ {xs ys : List Nat}, xs.length = ys.length →
      (xs.zip ys).all (fun (x, y) => st.find x == st.find y) = true → xs.map I = ys.map I
  | [], [], _, _ => rfl
  | x :: xs, y :: ys, hl, h => by
    simp only [List.zip_cons_cons, List.all_cons, Bool.and_eq_true, beq_iff_eq] at h
    simp only [List.map_cons, same_find cons h.1, map_find_eq cons (by simpa using hl) h.2]

theorem congruenceStep_sound {I : Nat → Value} {st st' : St} {cg : CongruenceRec}
    (cons : Cons I st) (sem : NodeSem I st.nodes) (h : congruenceStep st cg = some st') :
    Cons I st' := by
  unfold congruenceStep at h
  simp only [bind_some, guard_unit, Bool.and_eq_true, beq_iff_eq, exists_and_left,
    exists_const] at h
  obtain ⟨-, a, ha, b, hb, ⟨⟨⟨⟨hfa, hfb⟩, hop⟩, hlen⟩, hall⟩, hst⟩ := h
  simp only [Option.some.injEq] at hst
  subst hst
  have hch := map_find_eq cons hlen hall
  refine cons_union cons ?_
  rw [← hfa, ← hfb, find_sound cons, find_sound cons, sem a (List.mem_of_getElem? ha),
    sem b (List.mem_of_getElem? hb), hop, hch]

/-! ## The whole replay -/

theorem rewriteStep_shape {ctx : Ctx} {st st' : St} {rw : RewriteRec}
    (h : rewriteStep ctx st rw = some st') : st'.nodes = st.nodes ∧ st'.sorts = st.sorts := by
  unfold rewriteStep at h
  simp only [bind_some, guard_unit, Bool.and_eq_true, exists_and_left, exists_const] at h
  obtain ⟨-, r, -, fs, -, -, -, -, -, hst⟩ := h
  simp only [Option.some.injEq] at hst
  subst hst
  exact ⟨rfl, rfl⟩

theorem congruenceStep_shape {st st' : St} {cg : CongruenceRec}
    (h : congruenceStep st cg = some st') : st'.nodes = st.nodes ∧ st'.sorts = st.sorts := by
  unfold congruenceStep at h
  simp only [bind_some, guard_unit, exists_and_left, exists_const] at h
  obtain ⟨-, a, -, b, -, -, hst⟩ := h
  simp only [Option.some.injEq] at hst
  subst hst
  exact ⟨rfl, rfl⟩

theorem insertNode_shape {ctx : Ctx} {st st' : St} {nd : NodeRec}
    (h : insertNode ctx st nd = some st') :
    st'.parent = st.parent ++ [st.parent.length] ∧ (∃ x, st'.nodes = st.nodes ++ [x]) ∧
      st'.sorts = st.sorts ++ [nd.sort] := by
  unfold insertNode at h
  simp only [bind_some, guard_unit, Bool.and_eq_true, beq_iff_eq, exists_and_left,
    exists_const] at h
  obtain ⟨⟨⟨-, ho⟩, -⟩, op, -, h⟩ := h
  split at h
  · simp only [Option.some.injEq] at h
    subst h
    exact ⟨by rw [ho], ⟨_, rfl⟩, rfl⟩
  · simp only [bind_some, guard_unit, exists_and_left, exists_const] at h
    obtain ⟨-, j, -, inst, -, h⟩ := h
    split at h
    · simp only [bind_some, guard_unit, exists_and_left, exists_const] at h
      obtain ⟨-, h⟩ := h
      simp only [Option.some.injEq] at h
      subst h
      exact ⟨by rw [ho], ⟨_, rfl⟩, rfl⟩
    · cases h

theorem insertNode_cons {I : Nat → Value} {ctx : Ctx} {st st' : St} {nd : NodeRec}
    (cons : Cons I st) (h : insertNode ctx st nd = some st') : Cons I st' := by
  intro c p hp
  rw [(insertNode_shape h).1, List.getElem?_append] at hp
  split at hp
  · exact cons c p hp
  · rename_i hc
    simp only [List.getElem?_singleton] at hp
    split at hp
    · cases hp
      congr
      omega
    · cases hp

theorem step_shape {ctx : Ctx} {st st' : St} {e : Event} (h : step ctx st e = some st') :
    (∀ nd ∈ st.nodes, nd ∈ st'.nodes) ∧ st.sorts <+: st'.sorts := by
  cases e with
  | node nd =>
    obtain ⟨-, ⟨x, hx⟩, hs⟩ := insertNode_shape h
    exact ⟨fun n hn => by rw [hx]; exact List.mem_append_left _ hn, by rw [hs]; exact List.prefix_append _ _⟩
  | rewrite rw =>
    obtain ⟨hn, hs⟩ := rewriteStep_shape h
    exact ⟨fun n hm => by rw [hn]; exact hm, by rw [hs]; exact List.prefix_refl _⟩
  | congruence cg =>
    obtain ⟨hn, hs⟩ := congruenceStep_shape h
    exact ⟨fun n hm => by rw [hn]; exact hm, by rw [hs]; exact List.prefix_refl _⟩

theorem replay_shape {ctx : Ctx} :
    ∀ {st st' : St} {es : List Event}, replay ctx st es = some st' →
      (∀ nd ∈ st.nodes, nd ∈ st'.nodes) ∧ st.sorts <+: st'.sorts
  | st, st', [], h => by cases h; exact ⟨fun _ h => h, List.prefix_refl _⟩
  | st, st', e :: es, h => by
    simp only [replay, bind_some] at h
    obtain ⟨st1, h1, h2⟩ := h
    obtain ⟨n1, s1⟩ := step_shape h1
    obtain ⟨n2, s2⟩ := replay_shape h2
    exact ⟨fun nd hn => n2 nd (n1 nd hn), s1.trans s2⟩

theorem typed_prefix {I : Nat → Value} {s s' : List Nat} (h : s <+: s') (t : Typed I s') :
    Typed I s := by
  intro c n hc
  obtain ⟨u, rfl⟩ := h
  have hl : c < s.length := (List.getElem?_eq_some_iff.mp hc).1
  exact t c n (by rw [List.getElem?_append_left hl, hc])

theorem replay_sound {I : Nat → Value} {ctx : Ctx}
    (seeds : ∀ i, Holds (ctx.seedFacts.getD i default) (I i)) :
    ∀ {st st' : St} {es : List Event}, replay ctx st es = some st' → Cons I st →
      NodeSem I st'.nodes → Typed I st'.sorts → Cons I st'
  | st, st', [], h, cons, _, _ => by cases h; exact cons
  | st, st', e :: es, h, cons, sem, typed => by
    simp only [replay, bind_some] at h
    obtain ⟨st1, h1, h2⟩ := h
    obtain ⟨n2, s2⟩ := replay_shape h2
    have sem1 : NodeSem I st1.nodes := fun nd hn => sem nd (n2 nd hn)
    have typed1 := typed_prefix s2 typed
    refine replay_sound seeds h2 ?_ sem typed
    cases e with
    | node nd => exact insertNode_cons cons h1
    | rewrite rw =>
      have := rewriteStep_shape h1
      exact rewriteStep_sound cons (this.1 ▸ sem1) (this.2 ▸ typed1) seeds h1
    | congruence cg =>
      have := congruenceStep_shape h1
      exact congruenceStep_sound cons (this.1 ▸ sem1) h1

end EvoIR.Rewrite
