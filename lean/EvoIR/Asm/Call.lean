import EvoIR.Asm.Prologue

/-!
The call chunk: the allocated `bl` step and the assembly `bl`, the prologue
of the callee and the label of its entry block. The callee frame lies below
the stack pointer of the caller; the outgoing area of the caller is the
incoming area of the callee.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem names_map (ctx : Ctx) (checked : checkAsm ctx.p ctx.asm ctx.hints = true) :
    ctx.asm.functions.map (·.name) = ctx.p.functions.map (·.name) := by
  apply List.ext_getElem?
  intro i
  simp only [List.getElem?_map]
  cases hf : ctx.p.functions[i]? with
  | none =>
    have : ctx.p.functions.length ≤ i := by
      rcases Nat.lt_or_ge i ctx.p.functions.length with h | h
      · rw [List.getElem?_eq_getElem h] at hf; cases hf
      · exact h
    have count : ctx.p.functions.length = ctx.asm.functions.length := by
      simp only [checkAsm, Bool.and_eq_true, beq_iff_eq] at checked; exact checked.1.1.1.2
    rw [List.getElem?_eq_none (by omega)]; rfl
  | some f =>
    obtain ⟨h, g, ok⟩ := funOk_of_check ctx checked i f hf
    rw [ok.at_.2.2]; simp [ok.name]

/-- The callee of a call: its index in both programs. -/
theorem callee_index (ctx : Ctx) (checked : checkAsm ctx.p ctx.asm ctx.hints = true) (sym : String)
    (callee : Alloc.Function) (hfind : ctx.p.functions.find? (·.name == sym) = some callee) :
    ∃ j, ctx.p.functions[j]? = some callee ∧ functionIndex ctx.asm sym = some j := by
  obtain ⟨hp, i, hi, hget, before⟩ := List.find?_eq_some_iff_getElem.mp hfind
  have hidx : ctx.p.functions.findIdx? (·.name == sym) = some i := by
    rw [List.findIdx?_eq_some_iff_getElem]
    refine ⟨hi, by rw [hget]; exact hp, fun j hj => ?_⟩
    have := before j hj
    simpa using this
  refine ⟨i, by rw [List.getElem?_eq_getElem hi, hget], ?_⟩
  have e1 : functionIndex ctx.asm sym = (ctx.asm.functions.map (·.name)).findIdx? (· == sym) := by
    simp only [functionIndex, List.findIdx?_map]; rfl
  have e2 : ctx.p.functions.findIdx? (·.name == sym) =
      (ctx.p.functions.map (·.name)).findIdx? (· == sym) := by
    simp only [List.findIdx?_map]; rfl
  rw [e1, names_map ctx checked, ← e2, hidx]

theorem callStorage_shape (st : Storage) (count : Nat) (shape : Shape st) (small : count ≤ 8) :
    Shape (callStorage st count) := by
  refine ⟨fun place v d h => ?_, fun n hn => ?_⟩
  · cases place with
    | gpr n =>
      simp only [callStorage] at h
      split at h
      · exact shape.1 _ v d h
      · cases h
    | incoming k => exact shape.1 _ v d h
    | _ => cases h
  · simp only [callStorage]
    have : ¬ n < count := by
      intro lt; simp [allocatable, isCalleeSaved] at hn; omega
    simp [this]

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

/-- The entry block is the first block; its label follows the prologue. -/
theorem entry_block {ctx : Ctx} {i : Nat} {f : Alloc.Function} {h : FrameHint} {g : Asm.Function}
    (ok : FunOk ctx i f h g) :
    ∃ b0 rest, f.blocks = b0 :: rest ∧ b0.id = 0 ∧ f.blocks.find? (·.id == 0) = some b0 ∧
      g.body.drop 0 = (prologue h).map .inst ++ g.body.drop (prologue h).length ∧
      g.body[(prologue h).length]? = some (.label 0) ∧ At h f g.body b0 0 ((prologue h).length + 1) := by
  have entry := ok.entry
  simp only [entryOk, Bool.and_eq_true] at entry
  obtain ⟨⟨⟨⟨head, unique⟩, _⟩, _⟩, _⟩ := entry
  cases hb : f.blocks with
  | nil => rw [hb] at head; simp at head
  | cons b0 rest =>
    rw [hb] at head
    have id0 : b0.id = 0 := by simpa using head
    obtain ⟨lines, hl, hbody⟩ := body_lines h f g.body ok.body
    rw [hb] at hl
    obtain ⟨here, C, hhere, hC, hlines⟩ := blocksLines_cons h b0 rest lines hl
    obtain ⟨code, hc, rfl⟩ := blockLines_eq h _ b0 here hhere
    refine ⟨b0, rest, rfl, id0, ?_, ?_, ?_, ?_⟩
    · rw [← id0, ← hb]; exact blocks_find f ok.unique b0 (by rw [hb]; simp)
    · rw [hbody, hlines]; simp
    · rw [hbody, hlines, id0]; simp
    · refine ⟨[], rest, code, C, hb, hc, hC, ?_⟩
      rw [hbody, hlines]
      have := drop_prefix ((prologue h).map Line.inst) (Line.label b0.id :: code ++ C) 1
      simpa using this

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem execute_call_ok (p : Alloc.Program) (s : Alloc.State) (a : Activation) (f : Alloc.Function)
    (b : Alloc.Block) (sym : String) (args res : List Loc)
    (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.id == a.block) = some b) (hi : b.insts[a.pc]? = some (.bl sym args res))
    (habi : abiRegisters args = true) (callee : Alloc.Function)
    (hcallee : p.functions.find? (·.name == sym) = some callee) (entered : Activation)
    (henter : Alloc.enter callee (callStorage a.storage args.length) s.nextFrame = .ok entered) :
    Alloc.execute p s a = .ok { s with
        control := .running entered
        stack := ⟨{a with pc := a.pc+1}, res⟩ :: s.stack
        nextFrame := s.nextFrame+1 } := by
  simp only [Alloc.execute, hf, hb, hi, habi, hcallee, henter, Bool.not_true, Bool.false_eq_true,
    if_false, bind, Except.bind, pure, Except.pure]

theorem execute_call_bad (p : Alloc.Program) (s : Alloc.State) (a : Activation) (f : Alloc.Function)
    (b : Alloc.Block) (sym : String) (args res : List Loc)
    (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.id == a.block) = some b) (hi : b.insts[a.pc]? = some (.bl sym args res))
    (bad : abiRegisters args = false ∨ p.functions.find? (·.name == sym) = none) :
    ∃ msg, Alloc.execute p s a = .error (.malformed msg) := by
  rcases bad with habi | hnone
  · exact ⟨_, by simp only [Alloc.execute, hf, hb, hi, habi, Bool.not_false, if_true, bind,
      Except.bind]; rfl⟩
  · by_cases habi : abiRegisters args = true
    · exact ⟨_, by simp only [Alloc.execute, hf, hb, hi, habi, hnone, Bool.not_true,
        Bool.false_eq_true, if_false, bind, Except.bind]; rfl⟩
    · simp only [Bool.not_eq_true] at habi
      exact ⟨_, by simp only [Alloc.execute, hf, hb, hi, habi, Bool.not_false, if_true, bind,
        Except.bind]; rfl⟩

theorem abiRegisters_length (locs : List Loc) (h : abiRegisters locs = true) : locs.length ≤ 8 := by
  simp only [abiRegisters, Bool.and_eq_true, decide_eq_true_eq, abiRegisterCount] at h
  exact h.1

theorem enter_ok (f : Alloc.Function) (b0 : Alloc.Block) (hfind : f.blocks.find? (·.id == 0) = some b0)
    (id0 : b0.id = 0) (st : Storage) (owner : Nat) :
    Alloc.enter f st owner = .ok ⟨f.name, 0, 0, st, owner⟩ := by
  simp [Alloc.enter, hfind, id0]
  rfl

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem pathAppend {step : α → Option α} {n m : Nat} {s t u : α}
    (first : Path step n s t) (second : Path step m t u) : Path step (n + m) s u := by
  induction first with
  | refl => simpa using second
  | next edge _ ih => rw [Nat.add_right_comm]; exact .next edge (ih second)

theorem prologue_straight (h : FrameHint) : (prologue h).all straight = true := by
  simp only [prologue, List.all_append, List.all_cons, List.all_nil, Bool.and_true,
    Bool.and_eq_true, List.all_map, List.all_eq_true]
  refine ⟨⟨⟨rfl, rfl⟩, ?_⟩, fun _ _ => rfl⟩
  split <;> simp [straight]

theorem FunOk.callee_need {ctx : Ctx} {i : Nat} {f : Alloc.Function} {h : FrameHint}
    {g : Asm.Function} (ok : FunOk ctx i f h g) (sym : String) (args res : List Loc)
    (mi : Instruction.bl sym args res ∈ instructionsOf f) :
    frameBytes h + needOf ctx.p ctx.hints sym ≤ needOf ctx.p ctx.hints f.name := by
  have st := ok.stack
  simp only [stackOk, List.all_eq_true, Bool.and_eq_true, decide_eq_true_eq] at st
  exact (st sym (by simp only [callees, List.mem_filterMap]; exact ⟨_, mi, rfl⟩)).2

theorem find_name {p : Alloc.Program} {sym : String} {callee : Alloc.Function}
    (h : p.functions.find? (·.name == sym) = some callee) : callee.name = sym := by
  have := List.find?_some h
  simpa using this

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem outermost_push (a a' : Activation) (rf : ReturnFrame) (stack : List ReturnFrame)
    (h : rf.caller.function = a.function) :
    outermost a' (rf :: stack) = outermost a stack := by
  cases stack with
  | nil => simp [outermost, h]
  | cons x rest =>
    simp only [outermost, List.getLast?_cons_cons]
    rw [List.getLast?_eq_some_getLast (l := x :: rest) (by simp)]
    rfl

/-- The chunk of a call. -/
theorem advance_call {ctx : Ctx} {entry : String} (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    {type : Core.Source.IntType} {s0 : Alloc.State} (safe : Safe ctx.p type s0)
    {s : Alloc.State} {t : Asm.State} (reach : Reaches (Alloc.step ctx.p) s0 s)
    (c : Cursor ctx entry s t) (hi : c.a.pc < c.b.insts.length) (sym : String) (args res : List Loc)
    (hinst : c.b.insts[c.a.pc] = .bl sym args res) :
    ∃ s' t', Relation.TransGen (Predecessor (Alloc.step ctx.p)) s' s ∧
      Relation.TransGen (Predecessor (Asm.step ctx.asm)) t' t ∧ Running ctx entry s' t' := by
  obtain ⟨a, cur, frs, pos, f, g, b, hs, ht, outer, ok, name, block, pc, at_, hl, need, chain⟩ := c
  simp only at hi hinst
  have hf : ctx.p.functions.find? (·.name == a.function) = some f := by
    rw [← name]; exact names_find ctx checked cur.index f ok.at_.1
  have hget : b.insts[a.pc]? = some (.bl sym args res) := by
    rw [List.getElem?_eq_getElem hi, hinst]
  have mi : Instruction.bl sym args res ∈ instructionsOf f := by
    rw [← hinst]; exact inst_mem f b a.pc hi (.inr (List.mem_of_find?_eq_some block))
  have malformed : ∀ msg, Alloc.execute ctx.p s a = .error (.malformed msg) → False := by
    intro msg he
    have hstep : Alloc.step ctx.p s = some {s with control := .halted (.error (.malformed msg))} := by
      simp [Alloc.step, hs, he]
    exact no_malformed safe reach hstep msg rfl
  cases habi : abiRegisters args
  · obtain ⟨msg, he⟩ := execute_call_bad ctx.p s a f b sym args res hf block hget (.inl habi)
    exact absurd (malformed msg he) id
  cases hcallee : ctx.p.functions.find? (·.name == sym) with
  | none =>
    obtain ⟨msg, he⟩ := execute_call_bad ctx.p s a f b sym args res hf block hget (.inr hcallee)
    exact absurd (malformed msg he) id
  | some callee =>
  obtain ⟨j, hj, hidx⟩ := callee_index ctx checked sym callee hcallee
  obtain ⟨h', g', ok'⟩ := funOk_of_check ctx checked j callee hj
  have cname : callee.name = sym := find_name hcallee
  obtain ⟨b0, rest0, hblocks0, id0, hfind0, hd0, hlabel0, at0⟩ := entry_block ok'
  have henter := enter_ok callee b0 hfind0 id0 (callStorage a.storage args.length) s.nextFrame
  have hexec := execute_call_ok ctx.p s a f b sym args res hf block hget habi callee hcallee _ henter
  have hstep : Alloc.step ctx.p s = some { s with
      control := .running ⟨callee.name, 0, 0, callStorage a.storage args.length, s.nextFrame⟩
      stack := ⟨{a with pc := a.pc+1}, res⟩ :: s.stack
      nextFrame := s.nextFrame+1 } := by
    simp [Alloc.step, hs, hexec]
  -- The template and the stack bounds.
  obtain ⟨tmpl, htmpl, hdrop, at'⟩ := At_inst cur.hint f g.body b a.pc pos at_ hi
  rw [hinst] at htmpl
  simp only [instTemplate, Option.some.injEq] at htmpl
  subst htmpl
  have count := abiRegisters_length args habi
  have needCallee := ok.callee_need sym args res mi
  rw [name] at needCallee
  have frameCallee := ok'.need
  rw [cname] at frameCallee
  unfold frameBytes at needCallee frameCallee
  have entryLe := hl.top
  have spLow := hl.bottom
  have high := hl.high
  have sp16 := hl.sp16
  unfold Frame.entry at need entryLe
  have hspNat : cur.sp < 2^64 := by omega
  -- The assembly `bl`.
  let t1 : Asm.State := {t.set .x (.r 30) (codeAddress cur.index (pos+1)) with
    control := .running j 0}
  have t1low : t1.low = t.low := rfl
  have t1high : t1.high = t.high := rfl
  have t1sp : t1.sp = t.sp := rfl
  have t1mem : t1.memory = t.memory := rfl
  have t1regs : t1.regs = update t.regs 30 (codeAddress cur.index (pos+1)) := by
    simp [t1, view]
  have hbl : Asm.step ctx.asm t = some t1 := by
    rw [step_at ctx.asm t cur.index pos g (.bl sym) _ ht ok.at_.2.2 (by simpa using hdrop)]
    simp [execute, hidx, t1]
    rfl
  -- The prologue of the callee.
  obtain ⟨t2, hrun, hsp2, hfl2, hlow2, hhigh2, hregs2, hrec29, hrec30, hsaves, hkeep⟩ :=
    prologue_run j h' ok'.layout t1 cur.sp (by rw [t1sp, hl.sp]) hl.sp16
      (by rw [t1low]; omega) (by rw [t1high]; omega) (by rw [t1high]; omega)
  obtain ⟨path2, ctrl2⟩ := run_path ctx.asm j g' ok'.at_.2.2 (prologue h') 0 _ t1 t2 rfl hd0
    (prologue_straight h') hrun
  have path3 := label_path ctx.asm t2 j (prologue h').length g' ok'.at_.2.2
    (by rw [ctrl2]; simp) 0 hlabel0
  let t3 : Asm.State := {t2 with control := .running j ((prologue h').length + 1)}
  have t3low : t3.low = t.low := by simp only [t3]; rw [hlow2, t1low]
  have t3high : t3.high = t.high := by simp only [t3]; rw [hhigh2, t1high]
  have t3sp : t3.sp = BitVec.ofNat 64 (cur.sp - 16 - h'.size) := hsp2
  have t3regs : t3.regs = update (update t.regs 30 (codeAddress cur.index (pos+1))) 29
      (BitVec.ofNat 64 (cur.sp - 16)) := by simp only [t3]; rw [hregs2, t1regs]
  have path : Path (Asm.step ctx.asm) ((prologue h').length + 1 + 1) t t3 :=
    pathAppend (Path.next hbl path2) path3
  refine ⟨_, t3, .single hstep, path.positive (by omega), ?_⟩
  -- The new frame.
  let cur' : Frame := ⟨j, cur.sp - 16 - h'.size, h'⟩
  have fp' : cur'.fp = cur.sp - 16 := by simp only [Frame.fp, cur']; omega
  have entry' : cur'.entry = cur.sp := by simp only [Frame.entry, cur']; omega
  have memAbove : ∀ x, cur.sp ≤ x → t3.memory x = t.memory x := by
    intro x hx; simp only [t3]; rw [hkeep x hx, t1mem]
  have regsCallee : ∀ r, isCalleeSaved r = true → restoreView cur' t3.memory t3.regs r = t.regs r := by
    intro r hr
    have r29 : r ≠ 29 := by intro e; rw [e] at hr; simp [isCalleeSaved] at hr
    have r30 : r ≠ 30 := by intro e; rw [e] at hr; simp [isCalleeSaved] at hr
    unfold restoreView
    cases hoff : lookupOffset cur'.hint.saved r with
    | none =>
      rw [t3regs, update_other _ _ _ _ r29, update_other _ _ _ _ r30]
    | some off =>
      have m := lookupOffset_mem _ _ _ hoff
      have := hsaves (r, off) m
      simp only [t3, cur'] at this ⊢
      rw [this, t1regs, update_other _ _ _ _ r30]
  have hl' : Local cur' (callStorage a.storage args.length) t3 := by
    refine ⟨?_, ?_, ?_, ?_, ?_, ?_, callStorage_shape _ _ hl.shape count, ?_, ?_, ?_, ?_⟩
    · exact t3sp
    · simp only [cur']; have := ok'.layout.size16; omega
    · rw [t3regs, update_same, fp']
    · rw [t3high]; exact high
    · rw [entry', t3high]; omega
    · rw [t3low]; simp only [cur']; omega
    · intro n bits d hc
      simp only [callStorage] at hc
      split at hc
      · rename_i lt
        have n29 : n ≠ 29 := by omega
        have n30 : n ≠ 30 := by omega
        rw [t3regs, update_other _ _ _ _ n29, update_other _ _ _ _ n30]
        exact hl.regs n bits d hc
      · cases hc
    · intro fl d c' b' hc; simp [callStorage] at hc
    · intro k cell hc; simp [callStorage] at hc
    · refine ⟨fun k cell hc => by simp [callStorage] at hc, fun k cell hc => ?_⟩
      simp only [callStorage] at hc
      obtain ⟨hk, rel⟩ := hl.outgoing k cell hc
      have out := ok.layout.outgoing
      refine ⟨by rw [entry', t3high]; unfold Frame.fp at *; omega, ?_⟩
      rw [entry']
      exact SlotRel_congr _ _ _ cell (fun i _ => memAbove _ (by omega)) rel
  have chain' : Chain ctx t3.low t3.high t3.memory cur' t3.regs
      (⟨{a with pc := a.pc+1}, res⟩ :: s.stack) (cur :: frs) := by
    refine ⟨entry'.symm, hl.sp16, ?_, ?_, ⟨pos + 1, ?_, ?_, ?_⟩, hl.shape, ?_, ?_, ?_⟩
    · rw [t3low]; unfold Frame.entry; omega
    · rw [fp']
      simp only [t3]
      rw [hrec29, t1regs, update_other _ _ _ _ (by decide : (29 : Nat) ≠ 30), hl.fp]
    · exact ⟨f, g, b, ok.at_, name, block, Nat.succ_le_of_lt hi, at'⟩
    · have : pos + 1 ≤ g.body.length := by
        have := congrArg List.length hdrop
        simp at this; omega
      have := ok.span; omega
    · rw [fp']
      simp only [t3]
      rw [show cur.sp - 16 + 8 = cur.sp - 8 by omega, hrec30, t1regs, update_same]
    · rw [t3high]
      exact OwnSlots_congr _ _ _ cur _ (fun x hx => memAbove x hx) hl.slots
    · intro r bits d hr hc
      rw [regsCallee r hr]; exact hl.regs r bits d hc
    · rw [t3low, t3high]
      apply chain_congr ctx t.low t.high s.stack frs cur t.memory _ t.regs _
        (fun x hx => memAbove x (by unfold Frame.fp at hx; omega))
        (fun r off ho i hi => memAbove _ (by omega))
        (fun r hr _ => regsCallee r hr) chain
  have needNew : t3.low + needOf ctx.p ctx.hints callee.name ≤ cur'.entry := by
    rw [entry', cname, t3low]
    omega
  exact running_intro ⟨callee.name, 0, 0, callStorage a.storage args.length, s.nextFrame⟩ cur'
    (cur :: frs) ((prologue h').length + 1) callee g' b0 rfl rfl
    (by rw [outermost_push _ _ _ _ rfl]; exact outer) ok'.at_ rfl hfind0
    (Nat.zero_le _) at0 hl'
    needNew chain'

end EvoIR.Asm.Proof
