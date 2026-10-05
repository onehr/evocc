import EvoIR.Alloc.Lemmas
import EvoIR.MachineStep
import EvoIR.MachineProgress
import EvoIR.Core.Simulation

/-!
The simulation relation between the program before allocation (post-SSA
EvoMIR) and the allocated program, and its local steps.

A pre-allocation activation and an allocated activation are related when
both are in the same checked block, the checker accepts the rest of the
block from their two positions with some abstract state `E`, and `E` holds.
Return frames are related in the same way at the position after the call,
with the equations that a call keeps. Inserted copies are alloc-only steps.
Moves before allocation that the allocator deleted are pre-only steps that
precede the next matched step.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Alloc

open EvoIR.Machine (VReg RegClass Width Value Slot RegisterFile normalize normalize_idempotent)
open Core.Transition (Reaches)

section
variable (pre : Machine.Program) (post : Program) (hints : Hints)

/-- Both activations are in one checked block. The checker accepts the rest
of the block from the two program counters with state `E`. -/
def Cursor (a : Machine.Activation) (b : Activation) (E : Equations) : Prop :=
  a.function = b.function ∧ a.block = b.block ∧ a.owner = b.owner ∧
  ∃ f g block block' fuel X,
    pre.functions.find? (·.name == a.function) = some f ∧
    post.functions.find? (·.name == a.function) = some g ∧
    f.blocks.find? (·.id == a.block) = some block ∧
    g.blocks.find? (·.id == a.block) = some block' ∧
    a.pc ≤ block.insts.length ∧ b.pc ≤ block'.insts.length ∧
    walk pre post fuel (block.insts.drop a.pc) (block'.insts.drop b.pc) E = some X ∧
    termOK (hintOf hints a.function) f.blocks g.blocks X block.term block'.term = true

def Matched (a : Machine.Activation) (b : Activation) : Prop :=
  ∃ E, Cursor pre post hints a b E ∧ Holds E a.regs b.storage

/-- A return frame after a call to `callee`. `K` are the equations that the
call keeps. After the return, the result registers hold the results. -/
def FrameRel (callee : String) (fr : Machine.ReturnFrame) (fr' : ReturnFrame) : Prop :=
  resultLocs 0 fr.results = some fr'.results ∧ distinctIds fr.results = true ∧
  returnViews post callee (fr'.results.map Loc.width) = true ∧
  ∃ K, (∀ e ∈ K, e.1.callClobbered = false ∧ (fr.results.map (·.id)).contains e.2.id = false) ∧
    Holds K fr.caller.regs fr'.caller.storage ∧
    Cursor pre post hints fr.caller fr'.caller
      ((fr'.results.zip fr.results).map (fun e => (e.1.place, e.2)) ++ K)

inductive StackRel : String → List Machine.ReturnFrame → List ReturnFrame → Prop
  | nil (callee : String) : StackRel callee [] []
  | cons (callee : String) (fr : Machine.ReturnFrame) (fr' : ReturnFrame)
      (rest : List Machine.ReturnFrame) (rest' : List ReturnFrame) :
      FrameRel pre post hints callee fr fr' → StackRel fr.caller.function rest rest' →
      StackRel callee (fr :: rest) (fr' :: rest')

def Related (entry : Machine.State) (s : Machine.State) (t : State) : Prop :=
  Reaches (Machine.step .postSSA pre) entry s ∧ s.memory = t.memory ∧ s.nextFrame = t.nextFrame ∧
  ((∃ a b, s.control = .running a ∧ t.control = .running b ∧ Matched pre post hints a b ∧
      StackRel pre post hints a.function s.stack t.stack) ∨
    (∃ r, s.control = .halted r ∧ t.control = .halted r))

/-- The number of allocated instructions left in the current block. Each
alloc-only step decreases it. -/
def rank (_ : Machine.State) (t : State) : Nat :=
  match t.control with
  | .running b =>
    match post.functions.find? (·.name == b.function) with
    | some g =>
      match g.blocks.find? (·.id == b.block) with
      | some block' => block'.insts.length - b.pc
      | none => 0
    | none => 0
  | .halted _ => 0

end

/-! ## Facts of the global check -/

section checked
variable {pre : Machine.Program} {post : Program} {hints : Hints}

theorem find_name {fs : List Machine.Function} {name : String} {f : Machine.Function}
    (found : fs.find? (·.name == name) = some f) : f.name = name := by
  simpa using List.find?_some found

theorem find_name' {fs : List Function} {name : String} {g : Function}
    (found : fs.find? (·.name == name) = some g) : g.name = name := by
  simpa using List.find?_some found

theorem checked_function (checked : checkAllocation pre post hints = true) {name : String}
    {f : Machine.Function} (found : pre.functions.find? (·.name == name) = some f) :
    checkFunction pre post hints f = true :=
  List.all_eq_true.mp checked f (List.mem_of_find?_eq_some found)

/-- Each block of a checked function is accepted from its hint. -/
theorem block_entry (checked : checkAllocation pre post hints = true)
    {name : String} {id : Nat} {f : Machine.Function} {block : Machine.Block}
    (foundF : pre.functions.find? (·.name == name) = some f)
    (foundB : f.blocks.find? (·.id == id) = some block) :
    ∃ g block' X, post.functions.find? (·.name == name) = some g ∧
      g.blocks.find? (·.id == id) = some block' ∧
      walk pre post (block.insts.length + block'.insts.length + 1) block.insts block'.insts
        (hintOf hints name id) = some X ∧
      termOK (hintOf hints name) f.blocks g.blocks X block.term block'.term = true := by
  have fn := checked_function checked foundF
  have name_eq := find_name foundF
  have id_eq : block.id = id := by simpa using List.find?_some foundB
  unfold checkFunction at fn
  split at fn
  · rename_i g entry foundG _
    simp only [Bool.and_eq_true] at fn
    have all := List.all_eq_true.mp fn.2 block (List.mem_of_find?_eq_some foundB)
    unfold checkBlock at all
    split at all
    · simp at all
    · rename_i block' foundB'
      split at all
      · rename_i X walked
        refine ⟨g, block', X, by rw [← name_eq]; exact foundG, by rw [← id_eq]; exact foundB', ?_, ?_⟩
        · rw [← name_eq, ← id_eq]; exact walked
        · rw [← name_eq]; exact all
      · simp at all
  · simp at fn

theorem entry_facts (checked : checkAllocation pre post hints = true)
    {name : String} {f : Machine.Function} {entry : Machine.Block}
    (foundF : pre.functions.find? (·.name == name) = some f)
    (foundE : f.blocks.find? (·.id == 0) = some entry) :
    distinctIds entry.args = true ∧ (argumentLocs false 0 entry.args).isSome = true ∧
      (hintOf hints name 0).all (entryState entry.args).contains = true := by
  have fn := checked_function checked foundF
  have name_eq := find_name foundF
  unfold checkFunction at fn
  split at fn
  · rename_i g entry' _ foundE'
    rw [foundE] at foundE'
    cases foundE'
    simp only [Bool.and_eq_true] at fn
    rw [← name_eq]
    exact ⟨fn.1.1.1, fn.1.1.2, fn.1.2⟩
  · simp at fn

theorem cursor_entry (checked : checkAllocation pre post hints = true)
    {f : Machine.Function} {block : Machine.Block} (a : Machine.Activation) (b : Activation)
    (foundF : pre.functions.find? (·.name == a.function) = some f)
    (foundB : f.blocks.find? (·.id == a.block) = some block)
    (function : a.function = b.function) (blockSame : a.block = b.block) (owner : a.owner = b.owner)
    (pcZero : a.pc = 0) (pcZero' : b.pc = 0) :
    Cursor pre post hints a b (hintOf hints a.function a.block) := by
  obtain ⟨g, block', X, foundG, foundB', walked, term⟩ := block_entry checked foundF foundB
  refine ⟨function, blockSame, owner, f, g, block, block',
    block.insts.length + block'.insts.length + 1, X, foundF, foundG, foundB, foundB', by omega,
    by omega, ?_, term⟩
  rw [pcZero, pcZero', List.drop_zero, List.drop_zero]
  exact walked

end checked

/-! ## Steps of the two machines -/

theorem trans_of_reaches_step {step : α → Option α} {a b c : α}
    (path : Reaches step a b) (edge : step b = some c) :
    Relation.TransGen (Core.Transition.Predecessor step) c a := by
  induction path with
  | refl => exact .single edge
  | @next a next b first _ ih => exact (ih edge).tail first

theorem alloc_step_ok {p : Program} {t next : State} {b : Activation}
    (running : t.control = .running b) (executed : execute p t b = .ok next) :
    step p t = some next := by
  simp [step, running, executed]

theorem alloc_step_error {p : Program} {t : State} {b : Activation} {fault : Core.Source.Fault}
    (running : t.control = .running b) (executed : execute p t b = .error fault) :
    step p t = some {t with control := .halted (.error fault)} := by
  simp [step, running, executed]

theorem pre_step_ok {p : Machine.Program} {s next : Machine.State} {a : Machine.Activation}
    (running : s.control = .running a) (executed : Machine.execute .postSSA p s a = .ok next) :
    Machine.step .postSSA p s = some next := by
  simp [Machine.step, running, executed]

theorem pre_step_error {p : Machine.Program} {s : Machine.State} {a : Machine.Activation}
    {fault : Core.Source.Fault}
    (running : s.control = .running a) (executed : Machine.execute .postSSA p s a = .error fault) :
    Machine.step .postSSA p s = some {s with control := .halted (.error fault)} := by
  simp [Machine.step, running, executed]

theorem alloc_execute_inst {p : Program} (t : State) (b : Activation) {g : Function}
    {block' : Block} {inst : Instruction}
    (foundG : p.functions.find? (·.name == b.function) = some g)
    (foundB : g.blocks.find? (·.id == b.block) = some block')
    (foundI : block'.insts[b.pc]? = some inst)
    (notCall : ∀ symbol args results, inst ≠ .bl symbol args results) :
    execute p t b = (evalInstruction b.owner inst b.storage t.memory).map
      (fun r => {t with control := .running {b with pc := b.pc+1, storage := r.1}, memory := r.2}) := by
  cases inst <;> try exact absurd rfl (notCall _ _ _)
  all_goals simp only [execute, foundG, foundB, foundI]
  all_goals cases evalInstruction _ _ _ _ <;> rfl

theorem pre_mov_eval (owner : Nat) (d s : VReg) (regs : RegisterFile) (memory : Machine.Memory) :
    Machine.evalInstruction owner (.mov d s) regs memory =
      (Machine.read regs s).map (fun v => (Machine.writeSlot regs d (.value (normalize d.class v)), memory)) := by
  simp only [Machine.evalInstruction, Machine.evalInstructionOperands, Machine.operandValue_readOperand]
  cases Machine.read regs s <;> rfl

section progress
variable {pre : Machine.Program} {type : Core.Source.IntType} {entry : Machine.State}

/-- A reachable state before allocation never fails with a malformed fault. -/
theorem no_malformed (progress : Machine.ObservedProgress .postSSA pre type entry)
    {s : Machine.State} {a : Machine.Activation}
    (reach : Reaches (Machine.step .postSSA pre) entry s) (running : s.control = .running a)
    {message : String} (failed : Machine.execute .postSSA pre s a = .error (.malformed message)) :
    False := by
  have edge : Machine.step .postSSA pre s = some {s with control := .halted (.error (.malformed message))} := by
    simp [Machine.step, running, failed]
  rcases progress _ (reach.trans (.next edge (.refl _))) with ⟨next, h⟩ | ⟨o, h⟩
  · simp [Machine.step] at h
  · simp [Machine.observe] at h

end progress

/-! ## List positions -/

theorem drop_cons_at {l : List α} {i : Nat} {x : α} {rest : List α} (h : l.drop i = x :: rest) :
    l[i]? = some x ∧ l.drop (i+1) = rest ∧ i < l.length := by
  have lt : i < l.length := by
    apply Classical.byContradiction
    intro ge
    rw [List.drop_eq_nil_of_le (Nat.le_of_not_lt ge)] at h
    cases h
  rw [List.drop_eq_getElem_cons lt] at h
  injection h with first rest
  exact ⟨by rw [List.getElem?_eq_getElem lt, first], rest, lt⟩

theorem drop_nil_at {l : List α} {i : Nat} (h : l.drop i = []) (le : i ≤ l.length) :
    i = l.length ∧ l[i]? = none := by
  have := List.drop_eq_nil_iff.mp h
  exact ⟨by omega, List.getElem?_eq_none (by omega)⟩

theorem read_error_malformed {regs : RegisterFile} {r : VReg} {e : Core.Source.Fault}
    (failed : Machine.read regs r = .error e) : ∃ m, e = .malformed m := by
  unfold Machine.read at failed
  split at failed <;> simp at failed <;> exact ⟨_, failed.symm⟩

theorem state_eta {s : Machine.State} {a : Machine.Activation} (running : s.control = .running a) :
    {s with control := .running a} = s := by
  cases s; simp_all

/-! ## Unfolding the walk -/

section walk
variable {pre : Machine.Program} {post : Program}

theorem walk_inv {fuel : Nat} {os : List Machine.Instruction} {as : List Instruction}
    {E X : Equations} (walked : walk pre post fuel os as E = some X) :
    ∃ fuel', fuel = fuel' + 1 ∧
      (next pre post os as E = .done X ∨
        ∃ os' as' E', next pre post os as E = .step os' as' E' ∧
          walk pre post fuel' os' as' E' = some X) := by
  cases fuel with
  | zero => simp [walk] at walked
  | succ fuel =>
    refine ⟨fuel, rfl, ?_⟩
    simp only [walk] at walked
    split at walked
    · rename_i E' found; cases walked; exact .inl found
    · rename_i os' as' E' found; exact .inr ⟨os', as', E', found, walked⟩
    · simp at walked

theorem walk_step {fuel : Nat} {os os' : List Machine.Instruction} {as as' : List Instruction}
    {E E' : Equations} (found : next pre post os as E = .step os' as' E') :
    walk pre post (fuel+1) os as E = walk pre post fuel os' as' E' := by
  simp [walk, found]

theorem renameOf_some {o : Machine.Instruction} {d s : VReg} (found : renameOf o = some (d, s)) :
    o = .mov d s ∧ d.class = s.class := by
  cases o with
  | mov dst src =>
    simp only [renameOf] at found
    split at found
    · rename_i same
      simp only [Option.some.injEq, Prod.mk.injEq] at found
      obtain ⟨rfl, rfl⟩ := found
      exact ⟨rfl, by simpa using same⟩
    · simp at found
  | _ => simp [renameOf] at found

theorem classChange_some {E E' : Equations} {a : Instruction} {os os' : List Machine.Instruction}
    (found : classChange E a os = some (os', E')) :
    ∃ d s ld ls, os = .mov d s :: os' ∧ a = .mov ld ls ∧ d.class ≠ s.class ∧ d.class ≠ .flags ∧
      holds E ls s = true ∧ ld.fits d.class = true ∧ E' = define E ld d := by
  unfold classChange at found
  split at found
  · rename_i d s os''
    split at found
    · rename_i ld ls
      split at found
      · rename_i cond
        simp only [Option.some.injEq, Prod.mk.injEq] at found
        obtain ⟨rfl, rfl⟩ := found
        simp only [Bool.and_eq_true, bne_iff_ne, ne_eq] at cond
        exact ⟨d, s, ld, ls, rfl, rfl, cond.1.1.1, cond.1.1.2, cond.1.2, cond.2, rfl⟩
      · simp at found
    · simp at found
  · simp at found

theorem matchStep_some {E E' : Equations} {o : Machine.Instruction} {a : Instruction}
    (found : matchStep pre post E o a = some E') :
    (∃ symbol args results symbol' aargs aresults,
        o = .bl symbol args results ∧ a = .bl symbol' aargs aresults ∧
        matchCall pre post E symbol args results symbol' aargs aresults = some E') ∨
      ((∀ symbol args results, o ≠ .bl symbol args results) ∧ matchOriginal E o a = some E') := by
  unfold matchStep at found
  split at found
  · exact .inl ⟨_, _, _, _, _, _, rfl, rfl, found⟩
  · simp at found
  · rename_i first second
    refine .inr ⟨?_, found⟩
    intro symbol args results same
    subst same
    first
      | exact first _ _ _ rfl
      | exact second _ _ _ rfl
      | exact second _ _ _ _ _ _ rfl rfl
      | (cases a <;> first | exact first _ _ _ _ _ _ rfl rfl | exact second _ _ _ rfl)

end walk

/-! ## Pre-only moves -/

section settle
variable {pre : Machine.Program} {post : Program} {type : Core.Source.IntType} {entry : Machine.State}

/-- No inserted copy is next in the allocated list. -/
def NoCopyNext (as : List Instruction) : Prop := ∀ inst ∈ as.head?, inst.copyOf = none

/-- Run the same-class moves before allocation that the walk renames. After
them, the next instruction before allocation is not such a move. -/
theorem settle (progress : Machine.ObservedProgress .postSSA pre type entry) (storage : Storage) :
    ∀ (fuel : Nat) (s : Machine.State) (a : Machine.Activation) (E X : Equations)
      (f : Machine.Function) (block : Machine.Block) (as : List Instruction),
      Reaches (Machine.step .postSSA pre) entry s → s.control = .running a →
      pre.functions.find? (·.name == a.function) = some f →
      f.blocks.find? (·.id == a.block) = some block →
      a.pc ≤ block.insts.length →
      walk pre post fuel (block.insts.drop a.pc) as E = some X →
      NoCopyNext as → Holds E a.regs storage →
      ∃ a' E' fuel', Reaches (Machine.step .postSSA pre) s {s with control := .running a'} ∧
        a'.function = a.function ∧ a'.block = a.block ∧ a'.owner = a.owner ∧
        a'.pc ≤ block.insts.length ∧
        walk pre post fuel' (block.insts.drop a'.pc) as E' = some X ∧
        Holds E' a'.regs storage ∧
        (∀ o ∈ (block.insts.drop a'.pc).head?, renameOf o = none) := by
  intro fuel
  induction fuel with
  | zero =>
    intro s a E X f block as _ _ _ _ _ walked
    simp [walk] at walked
  | succ fuel ih =>
    intro s a E X f block as reach running foundF foundB le walked nonCopy state
    have stay : Reaches (Machine.step .postSSA pre) s {s with control := .running a} := by
      rw [state_eta running]; exact .refl _
    cases preRest : block.insts.drop a.pc with
    | nil =>
      exact ⟨a, E, fuel+1, stay, rfl, rfl, rfl, le, walked, state, by simp [preRest]⟩
    | cons o os =>
      cases renamed : renameOf o with
      | none =>
        exact ⟨a, E, fuel+1, stay, rfl, rfl, rfl, le, walked, state, by simp [preRest, renamed]⟩
      | some pair =>
        obtain ⟨d, src⟩ := pair
        obtain ⟨oEq, sameClass⟩ := renameOf_some renamed
        subst oEq
        obtain ⟨foundI, nextDrop, lt⟩ := drop_cons_at preRest
        have stepped : next pre post (.mov d src :: os) as E = .step os as (rename E d src) := by
          cases as with
          | nil => simp [next, renamed]
          | cons inst rest =>
            have noCopy : inst.copyOf = none := nonCopy inst (by simp)
            simp [next, noCopy, renamed]
        rw [preRest, walk_step stepped] at walked
        cases readSrc : Machine.read a.regs src with
        | error e =>
          obtain ⟨m, eq⟩ := read_error_malformed readSrc
          subst eq
          apply False.elim
          apply no_malformed progress reach running (message := m)
          apply Machine.execute_evalInstruction_error .postSSA pre s a f block (.mov d src) _
            foundF foundB foundI (by intro _ _ _ h; cases h)
          rw [pre_mov_eval, readSrc]; rfl
        | ok v =>
          have slotSrc : Machine.readSlot a.regs src = some (.value v) := by
            unfold Machine.read at readSrc
            split at readSrc <;> simp_all
          let a₁ : Machine.Activation :=
            {a with pc := a.pc+1, regs := Machine.writeSlot a.regs d (.value (normalize d.class v))}
          have edge : Machine.step .postSSA pre s = some {s with control := .running a₁} := by
            have := Machine.step_evalInstruction .postSSA pre s a f block (.mov d src)
              (Machine.writeSlot a.regs d (.value (normalize d.class v))) s.memory running foundF foundB
              foundI (by intro _ _ _ h; cases h) (by rw [pre_mov_eval, readSrc]; rfl)
            simpa using this
          have reach₁ := reach.trans (.next edge (.refl _))
          have state₁ : Holds (rename E d src) a₁.regs storage := Holds.rename state sameClass slotSrc
          obtain ⟨a', E', fuel', path, fn, bl, ow, le', walked', state', settled⟩ :=
            ih {s with control := .running a₁} a₁ (rename E d src) X f block as reach₁ rfl foundF
              foundB (by simp [a₁]; omega) (by simpa [a₁, nextDrop] using walked) nonCopy state₁
          exact ⟨a', E', fuel', .next edge (by simpa using path), fn, bl, ow, le', walked', state',
            settled⟩

end settle

end EvoIR.Alloc
