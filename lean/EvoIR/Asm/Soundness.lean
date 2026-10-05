import EvoIR.Asm.Return

/-!
Soundness of the frame and emission checker.

`checkAsm_sound`: an accepted allocated program `p`, assembly file `asm` and
frame hints give a refinement of the allocated machine by the assembly
machine from related entry states. The refinement is
`Core.Transition.Refines` with the observations of `Alloc.observe` and
`Asm.observe`: every terminal assembly state has the observation of a
terminal allocated state, every infinite assembly execution has an infinite
allocated execution, and no reachable assembly state is stuck without an
observation.

The explicit hypotheses are:

* `AbiEntry`: a valid ABI entry state. The control is at the first line of
  the entry function, the argument registers and the incoming stack slots
  hold the integer arguments, the stack pointer is 16-byte aligned, the
  return address in `x30` is outside the program, and the stack region has
  enough bytes below the stack pointer for the deepest call chain of the
  entry function (`needOf`). The stack region ends below `2^64`.
  `AbiEntry` takes the machine arguments with all 64 bits. The pipeline
  theorem uses `AbiSourceEntry`, which fixes only the low `w` bits of the
  register or stack slot of a source argument of width `w`.
  `AbiSourceEntry.abiEntry` gives `AbiEntry` for the machine arguments of the
  entry state (`abiArguments`).
* `ResultView`: the result type is not wider than the view of the returned
  register of the entry function.
* `Safe`: each reachable allocated state steps or has an observation. The
  allocation stage proves this for its target program; the composition in
  `EvoIR.Asm.Baseline` discharges it.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path Refines CheckpointSimulation)

/-- The chunk of `brk`. -/
theorem advance_brk {ctx : Ctx} {entry : String} (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    {type : Core.Source.IntType} {s0 : Alloc.State} (safe : Safe ctx.p type s0)
    {s : Alloc.State} {t : Asm.State} (reach : Reaches (Alloc.step ctx.p) s0 s)
    (c : Cursor ctx entry s t) (hpc : c.a.pc = c.b.insts.length) (imm : Nat)
    (hterm : c.b.term = .brk imm) : Advance ctx entry type s0 s t := by
  obtain ⟨a, cur, frs, pos, f, g, b, hs, ht, outer, ok, name, block, pc, at_, hl, need, chain⟩ := c
  simp only at hpc hterm
  have hf : ctx.p.functions.find? (·.name == a.function) = some f := by
    rw [← name]; exact names_find ctx checked cur.index f ok.at_.1
  have hexec : Alloc.execute ctx.p s a = .error (.diagnostic imm) := by
    rw [execute_term_eq ctx.p s a f b hf block hpc, hterm]; rfl
  let sf : Alloc.State := {s with control := .halted (.error (.diagnostic imm))}
  have hstep : Alloc.step ctx.p s = some sf := by simp [Alloc.step, hs, hexec, sf]
  obtain ⟨pre, post, tmpl, tail, split, htmpl, htail, hd⟩ :=
    At_term cur.hint f g.body b a.pc pos at_ (by omega)
  rw [hterm] at htmpl
  simp only [termTemplate, Option.some.injEq] at htmpl
  subst htmpl
  let tf : Asm.State := {t with control := .halted (.error (.diagnostic imm))}
  have hstep2 : Asm.step ctx.asm t = some tf := by
    rw [step_at ctx.asm t cur.index pos g (.brk imm) tail ht ok.at_.2.2 (by simpa using hd)]
    rfl
  obtain ⟨o, ho⟩ : ∃ o, Alloc.observe type sf = some o := by
    rcases safe sf (reach.trans (.next hstep (.refl _))) with ⟨_, h⟩ | h
    · simp [Alloc.step, sf] at h
    · exact h
  refine .inl ⟨sf, tf, o, .next hstep (.refl _), .next hstep2 (.refl _),
    by simp [Alloc.step, sf], halted_stuck ctx.asm tf _ rfl, ho, ?_⟩
  simp only [Alloc.observe, sf] at ho
  simp only [Asm.observe, tf]
  exact ho

/-- Every related pair advances. -/
theorem advance {ctx : Ctx} {entry : String} (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    {type : Core.Source.IntType} {s0 : Alloc.State} (safe : Safe ctx.p type s0)
    (rv : ResultView ctx.p entry type) {s : Alloc.State} {t : Asm.State}
    (reach : Reaches (Alloc.step ctx.p) s0 s) (rel : Running ctx entry s t) :
    Advance ctx entry type s0 s t := by
  obtain ⟨c⟩ := cursor_of_running checked rel
  by_cases hi : c.a.pc < c.b.insts.length
  · by_cases call : ∃ sym args res, c.b.insts[c.a.pc] = .bl sym args res
    · obtain ⟨sym, args, res, hinst⟩ := call
      obtain ⟨s', t', hs, ht, rel'⟩ := advance_call checked safe reach c hi sym args res hinst
      exact .inr ⟨s', t', hs, ht, reach.trans (Core.Transition.reaches_of_positive hs), rel'⟩
    · have notCall : ∀ sym args res, c.b.insts[c.a.pc] ≠ .bl sym args res := by
        intro sym args res e; exact call ⟨sym, args, res, e⟩
      obtain ⟨s', t', hs, ht, rel'⟩ := advance_body checked safe reach c hi notCall
      exact .inr ⟨s', t', hs, ht, reach.trans (Core.Transition.reaches_of_positive hs), rel'⟩
  · have hpc : c.a.pc = c.b.insts.length := by have := c.pc; omega
    cases hterm : c.b.term with
    | b k => exact advance_branch checked safe reach c hpc (.inl ⟨k, hterm⟩)
    | bCond fl cond tk un =>
      exact advance_branch checked safe reach c hpc (.inr (.inl ⟨fl, cond, tk, un, hterm⟩))
    | cb nz src tk un =>
      exact advance_branch checked safe reach c hpc (.inr (.inr (.inl ⟨nz, src, tk, un, hterm⟩)))
    | tb nz src bit tk un =>
      exact advance_branch checked safe reach c hpc
        (.inr (.inr (.inr ⟨nz, src, bit, tk, un, hterm⟩)))
    | ret locs => exact advance_ret checked safe rv reach c hpc locs hterm
    | brk imm => exact advance_brk checked safe reach c hpc imm hterm

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path Refines CheckpointSimulation)

/-- A valid ABI entry state of the assembly program for `entry` with the
arguments `args`, and sufficient stack memory. `E` is the entry stack
pointer. -/
structure AbiEntry (ctx : Ctx) (entry : String) (args : List Machine.Value) (t0 : Asm.State)
    (E : Nat) : Prop where
  function : ∃ i f, ctx.p.functions[i]? = some f ∧ f.name = entry ∧ t0.control = .running i 0
  sp : t0.sp = BitVec.ofNat 64 E
  aligned : E % 16 = 0
  registers : ∀ n v, n < 8 → args[n]? = some v → v = .int (t0.regs n)
  stackArgs : ∀ k v, args[8 + k]? = some v →
    v = .int (load64 t0.memory (E + 8 * k)) ∧ E + 8 * k + 8 ≤ t0.high
  returnAddress : decodeAddress (t0.regs 30) = none
  stackLow : t0.low + needOf ctx.p ctx.hints entry ≤ E
  stackHigh : E ≤ t0.high
  addressSpace : t0.high < 2^64

/-- The machine argument `n` of the entry state `t0` with the entry stack
pointer `E`: register `n` for `n < 8`, else the 8-byte stack slot at
`E + 8 * (n - 8)`. The value keeps all 64 bits. -/
def abiArgument (t0 : Asm.State) (E n : Nat) : Machine.Value :=
  if n < 8 then .int (t0.regs n) else .int (load64 t0.memory (E + 8 * (n - 8)))

/-- The first `count` machine arguments of the entry state `t0`. -/
def abiArguments (t0 : Asm.State) (E count : Nat) : List Machine.Value :=
  (List.range count).map (abiArgument t0 E)

theorem abiArguments_getElem? (t0 : Asm.State) (E count n : Nat) :
    (abiArguments t0 E count)[n]? = if n < count then some (abiArgument t0 E n) else none := by
  rw [abiArguments, List.getElem?_map]
  by_cases h : n < count <;> simp [h]

/-- A valid ABI entry state of the assembly program for `entry` with the
source arguments `args`. `E` is the entry stack pointer. The fields other than
`registers` and `stackArgs` are those of `AbiEntry`. Argument `n` is in
register `n` for `n < 8`, else in the 8-byte stack slot at `E + 8 * (n - 8)`.
For an argument of width `w`, only the low `w` bits of this register or slot
are equal to the argument. The other bits are arbitrary, as in AAPCS64. -/
structure AbiSourceEntry (ctx : Ctx) (entry : String) (args : List Core.Source.Value)
    (t0 : Asm.State) (E : Nat) : Prop where
  function : ∃ i f, ctx.p.functions[i]? = some f ∧ f.name = entry ∧ t0.control = .running i 0
  sp : t0.sp = BitVec.ofNat 64 E
  aligned : E % 16 = 0
  registers : ∀ n v, n < 8 → args[n]? = some v → (t0.regs n).setWidth v.type.width = v.bits
  stackArgs : ∀ k v, args[8 + k]? = some v →
    (load64 t0.memory (E + 8 * k)).setWidth v.type.width = v.bits ∧ E + 8 * k + 8 ≤ t0.high
  returnAddress : decodeAddress (t0.regs 30) = none
  stackLow : t0.low + needOf ctx.p ctx.hints entry ≤ E
  stackHigh : E ≤ t0.high
  addressSpace : t0.high < 2^64

/-- The low bits of each machine argument are the source argument. -/
theorem AbiSourceEntry.low {ctx : Ctx} {entry : String} {args : List Core.Source.Value}
    {t0 : Asm.State} {E : Nat} (abi : AbiSourceEntry ctx entry args t0 E) (i : Nat)
    (h : i < args.length) :
    ∃ raw, abiArgument t0 E i = .int raw ∧ raw.setWidth args[i].type.width = args[i].bits := by
  unfold abiArgument
  by_cases small : i < 8
  · exact ⟨_, if_pos small, abi.registers i _ small (List.getElem?_eq_getElem h)⟩
  · obtain ⟨k, rfl⟩ : ∃ k, i = 8 + k := ⟨i - 8, by omega⟩
    refine ⟨_, if_neg small, ?_⟩
    simpa using (abi.stackArgs k _ (List.getElem?_eq_getElem h)).1

/-- An ABI entry state for the source arguments is an `AbiEntry` for the
machine arguments of the state. -/
theorem AbiSourceEntry.abiEntry {ctx : Ctx} {entry : String} {args : List Core.Source.Value}
    {t0 : Asm.State} {E : Nat} (abi : AbiSourceEntry ctx entry args t0 E) :
    AbiEntry ctx entry (abiArguments t0 E args.length) t0 E where
  function := abi.function
  sp := abi.sp
  aligned := abi.aligned
  registers n v small found := by
    rw [abiArguments_getElem?] at found
    split at found
    · cases found; simp [abiArgument, small]
    · cases found
  stackArgs k v found := by
    rw [abiArguments_getElem?] at found
    split at found
    · rename_i lt
      cases found
      refine ⟨by simp [abiArgument, show ¬ 8 + k < 8 by omega], ?_⟩
      exact (abi.stackArgs k _ (List.getElem?_eq_getElem lt)).2
    · cases found
  returnAddress := abi.returnAddress
  stackLow := abi.stackLow
  stackHigh := abi.stackHigh
  addressSpace := abi.addressSpace

theorem entryStorage_shape (args : List Machine.Value)
    (ints : ∀ v ∈ args, ∃ b, v = .int b) : Shape (entryStorage args) := by
  refine ⟨fun place v d h => ?_, fun n hn => ?_⟩
  · cases place with
    | gpr n =>
      simp only [entryStorage] at h
      split at h
      · cases hv : args[n]? with
        | none => rw [hv] at h; cases h
        | some w =>
          rw [hv] at h; simp at h; obtain ⟨rfl, _⟩ := h
          exact ints _ (List.mem_of_getElem? hv)
      · cases h
    | incoming k =>
      simp only [entryStorage] at h
      cases hv : args[abiRegisterCount + k]? with
      | none => rw [hv] at h; cases h
      | some w =>
        rw [hv] at h; simp at h; obtain ⟨rfl, _⟩ := h
        exact ints _ (List.mem_of_getElem? hv)
    | _ => cases h
  · simp only [entryStorage, abiRegisterCount]
    have : ¬ n < 8 := by intro lt; simp [allocatable, isCalleeSaved] at hn; omega
    simp [this]

/-- The entry: the prologue of the entry function and the label of its entry
block relate the initial allocated state to the assembly state at block 0. -/
theorem entry_running (ctx : Ctx) (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    (entry : String) (args : List Machine.Value) (s0 : Alloc.State)
    (hs0 : Alloc.initial ctx.p entry args = .ok s0) (t0 : Asm.State) (E : Nat)
    (abi : AbiEntry ctx entry args t0 E) :
    ∃ n t1, Path (Asm.step ctx.asm) n t0 t1 ∧ Running ctx entry s0 t1 := by
  obtain ⟨i, f, hf, fname, hc⟩ := abi.function
  obtain ⟨h, g, ok⟩ := funOk_of_check ctx checked i f hf
  have hfind : ctx.p.functions.find? (·.name == entry) = some f := by
    rw [← fname]; exact names_find ctx checked i f hf
  obtain ⟨b0, rest0, hblocks0, id0, hfind0, hd0, hlabel0, at0⟩ := entry_block ok
  have henter := enter_ok f b0 hfind0 id0 (entryStorage args) 1
  have hs0' : s0 = ⟨.running ⟨f.name, 0, 0, entryStorage args, 1⟩, [], 2, ⟨1, []⟩⟩ := by
    simp [Alloc.initial, hfind, henter] at hs0
    have : (Except.ok s0 : Except Core.Source.Fault Alloc.State) =
        .ok ⟨.running ⟨f.name, 0, 0, entryStorage args, 1⟩, [], 2, ⟨1, []⟩⟩ := hs0.symm
    exact Except.ok.inj this
  subst hs0'
  have frameNeed := ok.need
  rw [fname] at frameNeed
  unfold frameBytes at frameNeed
  have low := abi.stackLow
  have top := abi.stackHigh
  have high := abi.addressSpace
  obtain ⟨t1, hrun, hsp1, hfl1, hlow1, hhigh1, hregs1, hrec29, hrec30, hsaves, hkeep⟩ :=
    prologue_run i h ok.layout t0 E abi.sp abi.aligned (by omega) top high
  obtain ⟨path1, ctrl1⟩ := run_path ctx.asm i g ok.at_.2.2 (prologue h) 0 _ t0 t1 hc hd0
    (prologue_straight h) hrun
  have path2 := label_path ctx.asm t1 i (prologue h).length g ok.at_.2.2 (by rw [ctrl1]; simp) 0
    hlabel0
  let t2 : Asm.State := {t1 with control := .running i ((prologue h).length + 1)}
  refine ⟨(prologue h).length + 1, t2, pathAppend path1 path2, ?_⟩
  let cur : Frame := ⟨i, E - 16 - h.size, h⟩
  have fp' : cur.fp = E - 16 := by simp only [Frame.fp, cur]; omega
  have entry' : cur.entry = E := by simp only [Frame.entry, cur]; omega
  have ints : ∀ v ∈ args, ∃ b, v = .int b := by
    intro v m
    obtain ⟨n, hn⟩ := List.getElem?_of_mem m
    by_cases small : n < 8
    · exact ⟨_, abi.registers n v small hn⟩
    · obtain ⟨k, rfl⟩ : ∃ k, n = 8 + k := ⟨n - 8, by omega⟩
      exact ⟨_, (abi.stackArgs k v hn).1⟩
  have hl : Local cur (entryStorage args) t2 := by
    refine ⟨hsp1, ?_, ?_, ?_, ?_, ?_, entryStorage_shape args ints, ?_, ?_, ?_, ?_⟩
    · simp only [cur]; have := ok.layout.size16; have := abi.aligned; omega
    · simp only [t2]; rw [hregs1, update_same, fp']
    · simp only [t2]; rw [hhigh1]; exact high
    · rw [entry']; simp only [t2]; rw [hhigh1]; exact top
    · simp only [t2, cur]; rw [hlow1]; omega
    · intro n b d hcell
      simp only [entryStorage, abiRegisterCount] at hcell
      split at hcell
      · rename_i lt
        cases hv : args[n]? with
        | none => rw [hv] at hcell; cases hcell
        | some v =>
          rw [hv] at hcell
          simp only [Option.map_some, Option.some.injEq, Cell.mk.injEq] at hcell
          obtain ⟨hcell, _⟩ := hcell
          have := abi.registers n v lt hv
          rw [Machine.Slot.value.injEq] at hcell
          subst hcell
          cases this
          simp only [t2]
          rw [hregs1, update_other _ _ _ _ (by omega)]
      · cases hcell
    · intro fl d c b hcell; simp [entryStorage] at hcell
    · intro k cell hcell; simp [entryStorage] at hcell
    · refine ⟨fun k cell hcell => by simp [entryStorage] at hcell, fun k cell hcell => ?_⟩
      simp only [entryStorage, abiRegisterCount] at hcell
      cases hv : args[8 + k]? with
      | none => rw [hv] at hcell; cases hcell
      | some v =>
        rw [hv] at hcell
        simp only [Option.map_some, Option.some.injEq] at hcell
        subst hcell
        obtain ⟨rfl, bound⟩ := abi.stackArgs k v hv
        rw [entry']
        refine ⟨by simp only [t2]; rw [hhigh1]; exact bound, ?_⟩
        simp only [SlotRel]
        exact load64_congr _ _ _ (fun j _ => by simp only [t2]; exact hkeep _ (by omega))
  have chain : Chain ctx t2.low t2.high t2.memory cur t2.regs [] [] := by
    refine ⟨by rw [entry']; simp only [t2]; rw [hhigh1]; exact top, ?_⟩
    rw [fp', show E - 16 + 8 = E - 8 by omega]
    simp only [t2]
    rw [hrec30]
    exact abi.returnAddress
  exact running_intro ⟨f.name, 0, 0, entryStorage args, 1⟩ cur [] ((prologue h).length + 1) f g b0
    rfl rfl (by simp [outermost, fname]) ok.at_ rfl hfind0 (Nat.zero_le _) at0 hl
    (by rw [entry', fname]; simp only [t2]; rw [hlow1]; exact low) chain

/-- Soundness of `checkAsm`: the assembly refines the allocated program from
related entry states. -/
theorem checkAsm_sound (p : Alloc.Program) (asm : File) (hints : List FrameHint)
    (checked : checkAsm p asm hints = true) (type : Core.Source.IntType) (entry : String)
    (args : List Machine.Value) (s0 : Alloc.State) (hs0 : Alloc.initial p entry args = .ok s0)
    (t0 : Asm.State) (E : Nat) (abi : AbiEntry ⟨p, asm, hints⟩ entry args t0 E)
    (rv : ResultView p entry type) (safe : Safe p type s0) :
    Refines (Alloc.step p) (Asm.step asm) (Alloc.observe type) (Asm.observe type) s0 t0 := by
  let ctx : Ctx := ⟨p, asm, hints⟩
  obtain ⟨n, t1, path, rel⟩ := entry_running ctx checked entry args s0 hs0 t0 E abi
  have sim : CheckpointSimulation (Alloc.step p) (Asm.step asm) (Alloc.observe type)
      (Asm.observe type) (fun s t => Reaches (Alloc.step p) s0 s ∧ Running ctx entry s t) := by
    constructor
    intro s t ⟨reach, rel⟩
    exact advance (ctx := ctx) checked safe rv reach rel
  exact Refines.prepend (sim.sound ⟨.refl _, rel⟩) (.refl _) path.reaches

end EvoIR.Asm.Proof
