import EvoIR.Asm.Advance

/-!
Terminators that stay in the function: unconditional and conditional
branches, and `brk`. A branch arrives at the label of its target block; the
label step is the last step of the chunk.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem drop_get {α : Type} {l : List α} {q : Nat} {x : α} {rest : List α}
    (h : l.drop q = x :: rest) : l[q]? = some x := by
  have := congrArg (·[0]?) h
  simpa using this

theorem jump_ok (file : File) (t : Asm.State) (fn : Nat) (g : Asm.Function)
    (hg : file.functions[fn]? = some g) (k L : Nat) (hL : labelIndex g.body k = some L) :
    jump file t fn k = .ok {t with control := .running fn L} := by
  simp [jump, hg, hL]; rfl

/-- The state after a step of a branch instruction at line `q`. -/
theorem step_branch (file : File) (t : Asm.State) (fn q : Nat) (g : Asm.Function)
    (hg : file.functions[fn]? = some g) (hc : t.control = .running fn q) (i : Inst)
    (rest : List Line) (hd : g.body.drop q = .inst i :: rest) (k L : Nat)
    (hL : labelIndex g.body k = some L) (taken : Bool)
    (hi : execute file t fn q i = if taken then jump file t fn k else .ok (t.next fn q)) :
    Asm.step file t = some (if taken then {t with control := .running fn L} else t.next fn q) := by
  rw [step_at file t fn q g i rest hc hg hd, hi]
  cases taken
  · rfl
  · simp only [if_true, jump_ok file t fn g hg k L hL]

/-- The data of the state is unchanged; the control moves to the code of
block `bu`. -/
theorem running_goto {ctx : Ctx} {entry : String} {s : Alloc.State} {t : Asm.State}
    (c : Cursor ctx entry s t) (bu : Alloc.Block) (find : c.f.blocks.find? (·.id == bu.id) = some bu)
    (q : Nat) (at_ : At c.cur.hint c.f c.g.body bu 0 q) :
    Running ctx entry {s with control := .running {c.a with block := bu.id, pc := 0}}
      {t with control := .running c.cur.index q} :=
  running_intro {c.a with block := bu.id, pc := 0} c.cur c.frs q c.f c.g bu rfl rfl c.outer c.ok.at_
    c.name find (Nat.zero_le _) at_ (c.local_.control _) c.need c.chain

/-- From a label line of block `bu`, one step reaches the code of `bu`. -/
theorem label_path (file : File) (t : Asm.State) (fn L : Nat) (g : Asm.Function)
    (hg : file.functions[fn]? = some g) (hc : t.control = .running fn L) (k : Nat)
    (hl : g.body[L]? = some (.label k)) :
    Path (Asm.step file) 1 t {t with control := .running fn (L+1)} := by
  have hlt : L < g.body.length := by
    rcases Nat.lt_or_ge L g.body.length with h | h
    · exact h
    · rw [List.getElem?_eq_none h] at hl; cases hl
  have heq : g.body[L] = .label k := by
    rw [List.getElem?_eq_getElem hlt] at hl; exact Option.some.inj hl
  have hd : g.body.drop L = .label k :: g.body.drop (L+1) := by
    rw [List.drop_eq_getElem_cons hlt, heq]
  exact .next (step_label_at file t fn L g k _ hc hg hd) (.refl _)

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem block_eq (f : Alloc.Function) (unique : keysUnique (f.blocks.map (·.id)) = true)
    (x y : Alloc.Block) (mx : x ∈ f.blocks) (my : y ∈ f.blocks) (e : x.id = y.id) : x = y := by
  have hx := blocks_find f unique x mx
  have hy := blocks_find f unique y my
  rw [e] at hx
  rw [hx] at hy; exact Option.some.inj hy

/-- A jump to the label of block `bu` and the label step. -/
theorem jump_path {ctx : Ctx} {i : Nat} {f : Alloc.Function} {h : FrameHint} {g : Asm.Function}
    (ok : FunOk ctx i f h g) (bu : Alloc.Block) (mbu : bu ∈ f.blocks) :
    ∃ L, labelIndex g.body bu.id = some L ∧ At h f g.body bu 0 (L+1) ∧
      ∀ t : Asm.State, t.control = .running i L →
        Path (Asm.step ctx.asm) 1 t {t with control := .running i (L+1)} := by
  obtain ⟨L, hL, hl, at_⟩ := At_block h f g.body ok.body ok.unique bu mbu
  exact ⟨L, hL, at_, fun t hc => label_path ctx.asm t i L g ok.at_.2.2 hc bu.id hl⟩

/-- The code `branchTo next u` followed by the next blocks reaches the code of
block `bu`. -/
theorem branchTo_path {ctx : Ctx} {i : Nat} {f : Alloc.Function} {h : FrameHint} {g : Asm.Function}
    (ok : FunOk ctx i f h g) (pre post : List Alloc.Block) (b : Alloc.Block)
    (split : f.blocks = pre ++ b :: post) (tail : List Line) (htail : blocksLines h post = some tail)
    (bu : Alloc.Block) (mbu : bu ∈ f.blocks) (t : Asm.State) (q : Nat)
    (hc : t.control = .running i q)
    (hd : g.body.drop q = (branchTo (post.head?.map (·.id)) bu.id).map .inst ++ tail) :
    ∃ n q', 0 < n ∧ Path (Asm.step ctx.asm) n t {t with control := .running i q'} ∧
      At h f g.body bu 0 q' := by
  unfold branchTo at hd
  split at hd
  · rename_i hnext
    cases post with
    | nil => cases hnext
    | cons bk post' =>
      simp only [List.head?_cons, Option.map_some, Option.some.injEq] at hnext
      have mbk : bk ∈ f.blocks := by rw [split]; simp
      have same := block_eq f ok.unique bk bu mbk mbu hnext
      subst same
      obtain ⟨hl, at_⟩ := At_next h f g.body pre b bk post' tail q split htail (by simpa using hd)
      exact ⟨1, q+1, by decide, label_path ctx.asm t i q g ok.at_.2.2 hc bk.id hl, at_⟩
  · obtain ⟨L, hL, at_, path⟩ := jump_path ok bu mbu
    have hstep := step_branch ctx.asm t i q g ok.at_.2.2 hc (.b bu.id) _ (by simpa using hd) bu.id L hL
      true (by simp [execute])
    simp only [if_true] at hstep
    refine ⟨2, L+1, by decide, .next hstep (path _ rfl), at_⟩

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem execute_term_eq (p : Alloc.Program) (s : Alloc.State) (a : Activation) (f : Alloc.Function)
    (b : Alloc.Block) (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.id == a.block) = some b) (hpc : a.pc = b.insts.length) :
    Alloc.execute p s a = (match b.term with
      | .b target => do return {s with control := .running (← Alloc.jump f a target)}
      | .bCond flags cond taken untaken => do
        let yes ← Machine.condition (← Alloc.readFlags a.storage flags) cond
        return {s with control := .running (← Alloc.jump f a (if yes then taken else untaken))}
      | .cb nonzero src taken untaken => do
        let some width := src.class.width | throw (.malformed "allocated flags tested as bits")
        let raw := (← Alloc.readBits a.storage src).setWidth width.bits
        let yes := (raw != 0) == nonzero
        return {s with control := .running (← Alloc.jump f a (if yes then taken else untaken))}
      | .tb nonzero src bit taken untaken => do
        if bit ≥ 64 then throw (.malformed "allocated test bit outside register")
        let yes := (← Alloc.readBits a.storage src).getLsbD bit == nonzero
        return {s with control := .running (← Alloc.jump f a (if yes then taken else untaken))}
      | .brk imm => throw (.diagnostic imm)
      | .ret locs => do
        if !abiRegisters locs then throw (.malformed "allocated results outside the ABI")
        let values ← locs.mapM (Alloc.readValue a.storage)
        let memory := s.memory.endFrame a.owner
        match s.stack with
        | [] => return {s with control := .halted (.ok values), memory}
        | frame :: rest =>
          if !abiRegisters frame.results || frame.results.length != locs.length then
            throw (.malformed "allocated result arity")
          let storage := returnStorage frame.caller.storage a.storage locs.length
          return {s with control := .running {frame.caller with storage}, stack := rest, memory}) := by
  have hnone : b.insts[b.insts.length]? = none := by simp
  simp only [Alloc.execute, hf, hb, hpc, hnone, bne_self_eq_false, Bool.false_eq_true, reduceIte]
  rfl

theorem holds_invert (fl : Flags) (c : Machine.Cond) : fl.holds (invertCond c) = !fl.holds c := by
  obtain ⟨n, z, cy, v⟩ := fl
  cases c <;> cases n <;> cases z <;> cases cy <;> cases v <;> rfl

/-- A two-way terminator: the first instruction jumps to `t'` when
`yes ^^ invert` holds; otherwise `branchTo next u'` follows. The path
reaches the block that the allocated step selects. -/
theorem twoWay_path {ctx : Ctx} {i : Nat} {f : Alloc.Function} {h : FrameHint} {g : Asm.Function}
    (ok : FunOk ctx i f h g) (pre post : List Alloc.Block) (b : Alloc.Block)
    (split : f.blocks = pre ++ b :: post) (tail : List Line) (htail : blocksLines h post = some tail)
    (taken untaken : Nat) (t : Asm.State) (pos : Nat) (hc : t.control = .running i pos)
    (i0 : Inst) (yes : Bool)
    (hd : g.body.drop pos = .inst i0 :: ((branchTo (post.head?.map (·.id))
      (twoWay (post.head?.map (·.id)) taken untaken).2.2).map .inst ++ tail))
    (hexec : execute ctx.asm t i pos i0 =
      if yes ^^ (twoWay (post.head?.map (·.id)) taken untaken).1 then
        jump ctx.asm t i (twoWay (post.head?.map (·.id)) taken untaken).2.1
      else .ok (t.next i pos))
    (bt : Alloc.Block) (mbt : bt ∈ f.blocks) (hbt : bt.id = if yes then taken else untaken) :
    ∃ n q', 0 < n ∧ Path (Asm.step ctx.asm) n t {t with control := .running i q'} ∧
      At h f g.body bt 0 q' := by
  have fall : ∀ (u : Nat), bt.id = u →
      (twoWay (post.head?.map (·.id)) taken untaken).2.2 = u →
      (yes ^^ (twoWay (post.head?.map (·.id)) taken untaken).1) = false →
      ∃ n q', 0 < n ∧ Path (Asm.step ctx.asm) n t {t with control := .running i q'} ∧
        At h f g.body bt 0 q' := by
    intro u hu hu' hno
    rw [hno] at hexec
    have hstep : Asm.step ctx.asm t = some (t.next i pos) := by
      rw [step_at ctx.asm t i pos g i0 _ hc ok.at_.2.2 hd, hexec]; rfl
    have hd' : g.body.drop (pos+1) = (branchTo (post.head?.map (·.id)) bt.id).map .inst ++ tail := by
      rw [← List.drop_drop, hd, hu, ← hu']; rfl
    obtain ⟨n, q', hn, path, at_⟩ := branchTo_path ok pre post b split tail htail bt mbt
      (t.next i pos) (pos+1) rfl hd'
    exact ⟨n+1, q', by omega, .next hstep path, at_⟩
  have jumpTo : ∀ (u : Nat), bt.id = u →
      (twoWay (post.head?.map (·.id)) taken untaken).2.1 = u →
      (yes ^^ (twoWay (post.head?.map (·.id)) taken untaken).1) = true →
      ∃ n q', 0 < n ∧ Path (Asm.step ctx.asm) n t {t with control := .running i q'} ∧
        At h f g.body bt 0 q' := by
    intro u hu hu' hyes
    obtain ⟨L, hL, at_, path⟩ := jump_path ok bt mbt
    rw [hyes, if_pos rfl, hu', ← hu, jump_ok ctx.asm t i g ok.at_.2.2 bt.id L hL] at hexec
    have hstep : Asm.step ctx.asm t = some {t with control := .running i L} := by
      rw [step_at ctx.asm t i pos g i0 _ hc ok.at_.2.2 hd, hexec]
    exact ⟨2, L+1, by decide, .next hstep (path _ rfl), at_⟩
  by_cases hn : post.head?.map (·.id) = some taken ∧ taken ≠ untaken
  · have htw : twoWay (post.head?.map (·.id)) taken untaken = (true, untaken, taken) := by
      simp only [twoWay, if_pos hn]
    cases yes
    · exact jumpTo untaken hbt (by rw [htw]) (by rw [htw]; rfl)
    · exact fall taken hbt (by rw [htw]) (by rw [htw]; rfl)
  · have htw : twoWay (post.head?.map (·.id)) taken untaken = (false, taken, untaken) := by
      simp only [twoWay, if_neg hn]
    cases yes
    · exact fall untaken hbt (by rw [htw]) (by rw [htw]; rfl)
    · exact jumpTo taken hbt (by rw [htw]) (by rw [htw]; rfl)

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem readFlags_ok {st : Storage} {fl : Machine.Flags} (h : Alloc.readFlags st .flags = .ok fl) :
    ∃ d, st .flags = some ⟨.flags fl, d⟩ := by
  simp only [Alloc.readFlags] at h
  obtain ⟨slot, hr, hm⟩ := bind_ok h
  cases slot with
  | value _ => cases hm
  | flags f =>
    cases hm
    simp only [readLoc, Loc.place] at hr
    cases hc : st .flags with
    | none => rw [hc] at hr; cases hr
    | some cell =>
      rw [hc] at hr
      obtain ⟨slot, d⟩ := cell
      cases slot with
      | flags f' => cases hr; exact ⟨d, rfl⟩
      | value _ => simp at hr

theorem readBits_gpr {st : Storage} {regs : Nat → BitVec 64} (shape : Shape st) (rel : RegsRel regs st)
    {n : Nat} {w : Machine.Width} {b : BitVec 64} (h : Alloc.readBits st (.gpr n w) = .ok b) :
    b = Machine.normalizeAt w (regs n) := by
  simp only [Alloc.readBits, Alloc.readValue] at h
  obtain ⟨v, hv, hm⟩ := bind_ok h
  obtain ⟨slot, hr, hs⟩ := bind_ok hv
  cases slot with
  | flags _ => cases hs
  | value val =>
    cases hs
    rw [readLoc_gpr shape rel hr] at hm
    cases hm; rfl

theorem view_ne_zero (w : Machine.Width) (x : BitVec 64) :
    (view w x != 0) = ((Machine.normalizeAt w x).setWidth w.bits != 0) := by
  simp only [bne, beq_toNat, toNat_zero_bv]
  congr 2
  cases w
  · simp only [view, Machine.normalizeAt, Machine.Width.bits, BitVec.toNat_setWidth]
    simp only [Nat.reducePow]
    apply propext
    omega
  · simp [view, Machine.normalizeAt, Machine.Width.bits]

theorem beq_xor (x a b : Bool) : (x == (a ^^ b)) = ((x == a) ^^ b) := by
  cases x <;> cases a <;> cases b <;> rfl

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

/-- The conclusion of one chunk: both machines stop with the same
observation, or both make positive progress to related states. -/
def Advance (ctx : Ctx) (entry : String) (type : Core.Source.IntType) (s0 : Alloc.State)
    (s : Alloc.State) (t : Asm.State) : Prop :=
  (∃ sf tf o, Reaches (Alloc.step ctx.p) s sf ∧ Reaches (Asm.step ctx.asm) t tf ∧
    Alloc.step ctx.p sf = none ∧ Asm.step ctx.asm tf = none ∧
    Alloc.observe type sf = some o ∧ Asm.observe type tf = some o) ∨
  (∃ s' t', Relation.TransGen (Predecessor (Alloc.step ctx.p)) s' s ∧
    Relation.TransGen (Predecessor (Asm.step ctx.asm)) t' t ∧
    (Reaches (Alloc.step ctx.p) s0 s' ∧ Running ctx entry s' t'))

theorem advance_of_step {ctx : Ctx} {entry : String} {type : Core.Source.IntType}
    {s0 s s' : Alloc.State} {t t' : Asm.State} (reach : Reaches (Alloc.step ctx.p) s0 s)
    (hstep : Alloc.step ctx.p s = some s') {n : Nat} (path : Path (Asm.step ctx.asm) n t t')
    (pos : 0 < n) (rel : Running ctx entry s' t') : Advance ctx entry type s0 s t :=
  .inr ⟨s', t', .single hstep, path.positive pos, reach.trans (.next hstep (.refl _)), rel⟩

theorem jump_inv {f : Alloc.Function} {a a' : Activation} {target : Nat}
    (h : Alloc.jump f a target = .ok a') :
    ∃ bt, f.blocks.find? (·.id == target) = some bt ∧ bt.id = target ∧
      a' = {a with block := target, pc := 0} := by
  simp only [Alloc.jump] at h
  split at h
  · rename_i bt hbt
    cases h
    have := List.find?_some hbt
    exact ⟨bt, hbt, by simpa using this, rfl⟩
  · cases h

theorem jump_malformed (f : Alloc.Function) (a : Activation) (target : Nat) :
    MalformedOnly (Alloc.jump f a target) := by
  intro e h
  simp only [Alloc.jump] at h
  split at h
  · cases h
  · cases h; exact ⟨_, rfl⟩

theorem readFlags_malformed (st : Storage) (l : Loc) : MalformedOnly (Alloc.readFlags st l) := by
  unfold Alloc.readFlags
  apply bind_malformed _ _ (readLoc_malformed st l)
  intro slot _
  cases slot with
  | flags _ => exact pure_malformed _
  | value _ => intro e h; cases h; exact ⟨_, rfl⟩

theorem readBits_malformed (st : Storage) (l : Loc) : MalformedOnly (Alloc.readBits st l) := by
  unfold Alloc.readBits Alloc.readValue
  apply bind_malformed
  · apply bind_malformed _ _ (readLoc_malformed st l)
    intro slot _
    cases slot with
    | value _ => exact pure_malformed _
    | flags _ => intro e h; cases h; exact ⟨_, rfl⟩
  · intro v _
    cases v with
    | int _ => exact pure_malformed _
    | pointer _ _ => intro e h; cases h; exact ⟨_, rfl⟩

theorem width_class (w : Machine.Width) : (Machine.Width.class w).width = some w := by
  cases w <;> rfl

/-- The chunk of an unconditional or conditional branch. -/
theorem advance_branch {ctx : Ctx} {entry : String} (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    {type : Core.Source.IntType} {s0 : Alloc.State} (safe : Safe ctx.p type s0)
    {s : Alloc.State} {t : Asm.State} (reach : Reaches (Alloc.step ctx.p) s0 s)
    (c : Cursor ctx entry s t) (hpc : c.a.pc = c.b.insts.length)
    (branch : (∃ k, c.b.term = .b k) ∨ (∃ fl cond tk un, c.b.term = .bCond fl cond tk un) ∨
      (∃ nz src tk un, c.b.term = .cb nz src tk un) ∨ (∃ nz src bit tk un, c.b.term = .tb nz src bit tk un)) :
    Advance ctx entry type s0 s t := by
  have hf : ctx.p.functions.find? (·.name == c.a.function) = some c.f := by
    rw [← c.name]; exact names_find ctx checked c.cur.index c.f c.ok.at_.1
  have hexec := execute_term_eq ctx.p s c.a c.f c.b hf c.block hpc
  obtain ⟨pre, post, tmpl, tail, split, htmpl, htail, hd⟩ :=
    At_term c.cur.hint c.f c.g.body c.b c.a.pc c.pos c.at_ (by omega)
  have malformed : ∀ msg, Alloc.execute ctx.p s c.a = .error (.malformed msg) → False := by
    intro msg he
    have hstep : Alloc.step ctx.p s = some {s with control := .halted (.error (.malformed msg))} := by
      simp [Alloc.step, c.hs, he]
    exact no_malformed safe reach hstep msg rfl
  -- The allocated step selects a block `bt`; the path of the template reaches it.
  have finish : ∀ (bt : Alloc.Block), c.f.blocks.find? (·.id == bt.id) = some bt →
      Alloc.execute ctx.p s c.a = .ok {s with control := .running {c.a with block := bt.id, pc := 0}} →
      (∃ n q', 0 < n ∧ Path (Asm.step ctx.asm) n t {t with control := .running c.cur.index q'} ∧
        At c.cur.hint c.f c.g.body bt 0 q') →
      Advance ctx entry type s0 s t := by
    intro bt hfind hok ⟨n, q', hn, path, at_⟩
    exact advance_of_step reach (by simp [Alloc.step, c.hs, hok]) path hn (running_goto c bt hfind q' at_)
  rcases branch with ⟨k, hterm⟩ | ⟨fl, cond, tk, un, hterm⟩ | ⟨nz, src, tk, un, hterm⟩ |
    ⟨nz, src, bit, tk, un, hterm⟩
  · -- `b k`
    rw [hterm] at hexec htmpl
    simp only [termTemplate, Option.some.injEq] at htmpl
    subst htmpl
    simp only [bind, Except.bind] at hexec
    cases hj : Alloc.jump c.f c.a k with
    | error e =>
      obtain ⟨msg, rfl⟩ := jump_malformed c.f c.a k e hj
      exact absurd (malformed msg (by rw [hexec, hj])) id
    | ok a' =>
      obtain ⟨bt, hbt, hid, rfl⟩ := jump_inv hj
      have mbt := List.mem_of_find?_eq_some hbt
      apply finish bt (blocks_find c.f c.ok.unique bt mbt) (by rw [hexec, hj, hid]; rfl)
      apply branchTo_path c.ok pre post c.b split tail htail bt mbt t c.pos c.ht
      rw [hd, hid]
  · -- `b.cond`
    rw [hterm] at hexec htmpl
    generalize htw : twoWay (post.head?.map (·.id)) tk un = tw at htmpl
    obtain ⟨inv, t', u'⟩ := tw
    simp only [termTemplate, Option.ite_none_right_eq_some, Option.some.injEq] at htmpl
    obtain ⟨rfl, rfl⟩ := htmpl
    cases hr : Alloc.readFlags c.a.storage .flags with
    | error e =>
      obtain ⟨msg, rfl⟩ := readFlags_malformed c.a.storage .flags e hr
      exact absurd (malformed msg (by simp [hexec, hr, bind, Except.bind])) id
    | ok fl =>
      cases hcond : Machine.condition fl cond with
      | error e =>
        obtain ⟨msg, rfl⟩ := condition_malformed fl cond e hcond
        exact absurd (malformed msg (by simp [hexec, hr, hcond, bind, Except.bind])) id
      | ok yes =>
        cases hj : Alloc.jump c.f c.a (if yes then tk else un) with
        | error e =>
          obtain ⟨msg, rfl⟩ := jump_malformed c.f c.a _ e hj
          exact absurd (malformed msg (by simp [hexec, hr, hcond, hj, bind, Except.bind])) id
        | ok a' =>
          obtain ⟨bt, hbt, hid, rfl⟩ := jump_inv hj
          have mbt := List.mem_of_find?_eq_some hbt
          obtain ⟨d, hst⟩ := readFlags_ok hr
          have holds := c.local_.flags fl d cond yes hst hcond
          apply finish bt (blocks_find c.f c.ok.unique bt mbt)
            (by simp [hexec, hr, hcond, hj, hid, bind, Except.bind, pure, Except.pure])
          apply twoWay_path c.ok pre post c.b split tail htail tk un t c.pos c.ht _ yes
            (by rw [hd, htw]; rfl) (by
              rw [htw]
              simp only [execute]
              congr 1
              cases inv <;> simp [holds_invert, holds])
            bt mbt hid
  · -- `cbz` and `cbnz`
    rw [hterm] at hexec htmpl
    generalize htw : twoWay (post.head?.map (·.id)) tk un = tw at htmpl
    obtain ⟨inv, t', u'⟩ := tw
    simp only [termTemplate, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq, Prod.exists] at htmpl
    obtain ⟨n, sv, hn, rfl⟩ := htmpl
    cases gprOf_eq hn
    simp only [Loc.class, Loc.width, width_class] at hexec
    cases hr : Alloc.readBits c.a.storage (.gpr n sv) with
    | error e =>
      obtain ⟨msg, rfl⟩ := readBits_malformed c.a.storage _ e hr
      exact absurd (malformed msg (by simp only [hexec, hr, bind, Except.bind])) id
    | ok bits =>
      have hbits := readBits_gpr c.local_.shape c.local_.regs hr
      cases hj : Alloc.jump c.f c.a (if ((bits.setWidth sv.bits != 0) == nz) then tk else un) with
      | error e =>
        obtain ⟨msg, rfl⟩ := jump_malformed c.f c.a _ e hj
        exact absurd (malformed msg (by simp only [hexec, hr, hj, bind, Except.bind])) id
      | ok a' =>
        obtain ⟨bt, hbt, hid, rfl⟩ := jump_inv hj
        have mbt := List.mem_of_find?_eq_some hbt
        apply finish bt (blocks_find c.f c.ok.unique bt mbt)
          (by simp only [hexec, hr, hj, hid, bind, Except.bind, pure, Except.pure])
        apply twoWay_path c.ok pre post c.b split tail htail tk un t c.pos c.ht _
          ((bits.setWidth sv.bits != 0) == nz) (by rw [hd, htw]; rfl) (by
            rw [htw]
            simp only [execute, State.get, State.reg]
            congr 1
            rw [view_ne_zero, ← hbits, beq_xor])
          bt mbt hid
  · -- `tbz` and `tbnz`
    rw [hterm] at hexec htmpl
    generalize htw : twoWay (post.head?.map (·.id)) tk un = tw at htmpl
    obtain ⟨inv, t', u'⟩ := tw
    simp only [termTemplate, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
      Option.some.injEq, Prod.exists] at htmpl
    obtain ⟨n, sv, hn, rfl⟩ := htmpl
    cases gprOf_eq hn
    by_cases big : bit ≥ 64
    · exact absurd (malformed "allocated test bit outside register" (by
        simp only [hexec, big, if_true]; rfl)) id
    cases hr : Alloc.readBits c.a.storage (.gpr n sv) with
    | error e =>
      obtain ⟨msg, rfl⟩ := readBits_malformed c.a.storage _ e hr
      exact absurd (malformed msg (by simp only [hexec, big, if_false, hr, bind, Except.bind])) id
    | ok bits =>
      have hbits := readBits_gpr c.local_.shape c.local_.regs hr
      cases hj : Alloc.jump c.f c.a (if (bits.getLsbD bit == nz) then tk else un) with
      | error e =>
        obtain ⟨msg, rfl⟩ := jump_malformed c.f c.a _ e hj
        exact absurd (malformed msg (by simp only [hexec, big, if_false, hr, hj, bind, Except.bind])) id
      | ok a' =>
        obtain ⟨bt, hbt, hid, rfl⟩ := jump_inv hj
        have mbt := List.mem_of_find?_eq_some hbt
        apply finish bt (blocks_find c.f c.ok.unique bt mbt)
          (by simp only [hexec, big, if_false, hr, hj, hid, bind, Except.bind, pure, Except.pure])
        apply twoWay_path c.ok pre post c.b split tail htail tk un t c.pos c.ht _
          (bits.getLsbD bit == nz) (by rw [hd, htw]; rfl) (by
            rw [htw]
            simp only [execute, State.get, State.reg]
            congr 1
            rw [view_eq, ← hbits, beq_xor])
          bt mbt hid

end EvoIR.Asm.Proof
