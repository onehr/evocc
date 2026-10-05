import EvoIR.Alloc.Simulation

/-!
The local transfer steps of the allocation simulation. Each allocated step
either matches a positive number of steps before allocation (a matched
instruction, call or terminator, after the moves that the walk renames), or
it is an inserted copy that keeps the state before allocation and decreases
the rank.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Alloc

open EvoIR.Machine (VReg RegClass Width Value Slot RegisterFile normalize normalize_idempotent)
open Core.Transition (Reaches Predecessor)

section transfer
variable {pre : Machine.Program} {post : Program} {hints : Hints} {entry : Machine.State}

abbrev PreStep (pre : Machine.Program) := Machine.step .postSSA pre

/-- An inserted copy keeps the state before allocation and decreases the rank. -/
theorem copy_case {s : Machine.State} {t t' : State} {a : Machine.Activation} {b : Activation}
    {E X : Equations} {f : Machine.Function} {g : Function} {block : Machine.Block}
    {block' : Block} {fuel : Nat} {inst : Instruction} {rest : List Instruction} {dst src : Loc}
    (reach : Reaches (PreStep pre) entry s) (runA : s.control = .running a)
    (runB : t.control = .running b) (mem : s.memory = t.memory) (nf : s.nextFrame = t.nextFrame)
    (stack : StackRel pre post hints a.function s.stack t.stack)
    (fn : a.function = b.function) (bl : a.block = b.block) (ow : a.owner = b.owner)
    (foundF : pre.functions.find? (·.name == a.function) = some f)
    (foundG : post.functions.find? (·.name == a.function) = some g)
    (foundB : f.blocks.find? (·.id == a.block) = some block)
    (foundB' : g.blocks.find? (·.id == a.block) = some block')
    (le : a.pc ≤ block.insts.length) (allocRest : block'.insts.drop b.pc = inst :: rest)
    (walked : walk pre post fuel (block.insts.drop a.pc) rest (copy E dst src) = some X)
    (term : termOK (hintOf hints a.function) f.blocks g.blocks X block.term block'.term = true)
    (isCopy : inst.copyOf = some (dst, src)) (defined : sourceDefined E src = true)
    (state : Holds E a.regs b.storage) (stepped : step post t = some t') :
    Related pre post hints entry s t' ∧ rank post s t' < rank post s t := by
  obtain ⟨foundI', nextDrop', lt'⟩ := drop_cons_at allocRest
  have foundG' : post.functions.find? (·.name == b.function) = some g := fn ▸ foundG
  have foundB'' : g.blocks.find? (·.id == b.block) = some block' := bl ▸ foundB'
  obtain ⟨v, eval, state'⟩ := copy_eval b.owner t.memory isCopy defined state
  let st' := writeLoc b.storage dst (.value (normalize dst.class v))
  have exec : execute post t b = .ok {t with control := .running {b with pc := b.pc+1, storage := st'}} := by
    rw [alloc_execute_inst t b foundG' foundB'' foundI' (copyOf_notCall isCopy), eval]; rfl
  have same := alloc_step_ok runB exec
  rw [stepped] at same
  cases same
  refine ⟨⟨reach, mem, nf, .inl ⟨a, _, runA, rfl, ⟨copy E dst src, ⟨fn, bl, ow, f, g, block, block',
    fuel, X, foundF, foundG, foundB, foundB', le, by simp; omega, by simpa [nextDrop'] using walked,
    term⟩, state'⟩, stack⟩⟩, ?_⟩
  simp only [rank, runB, foundG', foundB'']
  omega

theorem toMachine_notCall {inst : Instruction} {o : Machine.Instruction} {U D : VReg → Loc}
    (shape : inst.toMachine Loc.reg = mapDefs (fun r => (D r).reg) (o.mapUses (fun r => (U r).reg)))
    (notCall : ∀ x y z, o ≠ .bl x y z) : ∀ x y z, inst ≠ .bl x y z := by
  intro x y z same
  subst same
  cases o <;> simp_all [Instruction.toMachine, mapDefs, Machine.Instruction.mapUses]

/-- A matched original instruction that is not a call. The state before
allocation first performs the renamed moves from `s` to `s₁`. -/
theorem instruction_case {s s₁ : Machine.State} {t t' : State} {a : Machine.Activation}
    {b : Activation} {E E' X : Equations} {f : Machine.Function} {g : Function}
    {block : Machine.Block} {block' : Block} {fuel : Nat} {o : Machine.Instruction}
    {inst : Instruction} {os : List Machine.Instruction} {rest : List Instruction}
    (reach : Reaches (PreStep pre) entry s) (path : Reaches (PreStep pre) s s₁)
    (run₁ : s₁.control = .running a) (runB : t.control = .running b)
    (mem : s₁.memory = t.memory) (nf : s₁.nextFrame = t.nextFrame)
    (stack : StackRel pre post hints a.function s₁.stack t.stack)
    (fn : a.function = b.function) (bl : a.block = b.block) (ow : a.owner = b.owner)
    (foundF : pre.functions.find? (·.name == a.function) = some f)
    (foundG : post.functions.find? (·.name == a.function) = some g)
    (foundB : f.blocks.find? (·.id == a.block) = some block)
    (foundB' : g.blocks.find? (·.id == a.block) = some block')
    (preRest : block.insts.drop a.pc = o :: os) (allocRest : block'.insts.drop b.pc = inst :: rest)
    (walked : walk pre post fuel os rest E' = some X)
    (term : termOK (hintOf hints a.function) f.blocks g.blocks X block.term block'.term = true)
    (notCall : ∀ x y z, o ≠ .bl x y z) (U D : VReg → Loc)
    (shape : inst.toMachine Loc.reg = mapDefs (fun r => (D r).reg) (o.mapUses (fun r => (U r).reg)))
    (uses : ∀ r ∈ o.uses, holds E (U r) r = true)
    (defs : ∀ r ∈ o.defs, (D r).place.at r.class = some (D r))
    (outputs : Machine.OutputClassesValid o)
    (result : E' = defineAll E (o.defs.map (fun r => (r, D r))))
    (state : Holds E a.regs b.storage) (stepped : step post t = some t') :
    ∃ s', Relation.TransGen (Predecessor (PreStep pre)) s' s ∧ Related pre post hints entry s' t' := by
  obtain ⟨foundI, nextDrop, lt⟩ := drop_cons_at preRest
  obtain ⟨foundI', nextDrop', lt'⟩ := drop_cons_at allocRest
  have foundG' : post.functions.find? (·.name == b.function) = some g := fn ▸ foundG
  have foundB'' : g.blocks.find? (·.id == b.block) = some block' := bl ▸ foundB'
  have exec := alloc_execute_inst t b foundG' foundB'' foundI' (toMachine_notCall shape notCall)
  rcases matched_eval a.owner s₁.memory U D shape uses defs outputs result state with
    ⟨e, preErr, allocErr⟩ | ⟨regs', st', m', preOk, allocOk, state'⟩
  · have edge := pre_step_error run₁
      (Machine.execute_evalInstruction_error .postSSA pre s₁ a f block o e foundF foundB foundI
        notCall preErr)
    rw [ow, mem] at allocErr
    have same := alloc_step_error runB (by rw [exec, allocErr]; rfl)
    rw [stepped] at same
    cases same
    exact ⟨_, trans_of_reaches_step path edge, reach.trans (path.trans (.next edge (.refl _))),
      mem, nf, .inr ⟨.error e, rfl, rfl⟩⟩
  · have edge := pre_step_ok run₁
      (Machine.execute_evalInstruction .postSSA pre s₁ a f block o regs' m' foundF foundB foundI
        notCall preOk)
    rw [ow, mem] at allocOk
    have same := alloc_step_ok runB (by rw [exec, allocOk]; rfl)
    rw [stepped] at same
    cases same
    exact ⟨_, trans_of_reaches_step path edge, reach.trans (path.trans (.next edge (.refl _))),
      rfl, nf, .inl ⟨_, _, rfl, rfl, ⟨E', ⟨fn, bl, ow, f, g, block, block', fuel, X, foundF, foundG,
        foundB, foundB', by simp; omega, by simp; omega, by simpa [nextDrop, nextDrop'] using walked,
        term⟩, state'⟩, stack⟩⟩


/-! ## Calls -/

theorem argPlace_at {outgoing : Bool} {i : Nat} {r : VReg} (general : r.class ≠ .flags) :
    ∃ loc, (argPlace outgoing i).at r.class = some loc ∧ loc ≠ .flags ∧ loc.width.class = r.class := by
  cases h : r.class
  case flags => exact absurd h general
  all_goals unfold argPlace; split <;> (try split) <;>
    simp [Place.at, Machine.RegClass.width, Machine.Width.class, Loc.width]

theorem heldAt_read {E : Equations} {regs : RegisterFile} {st : Storage} {place : Place} {r : VReg}
    (state : Holds E regs st) (held : heldAt E place r = true) (general : r.class ≠ .flags) :
    ∃ loc v, place.at r.class = some loc ∧ Machine.readSlot regs r = some (.value v) ∧
      normalize r.class v = v ∧ readLoc st loc = .ok (.value v) := by
  simp only [heldAt, Bool.and_eq_true, List.contains_iff_mem] at held
  obtain ⟨loc, slot, fits, found, written, read⟩ := state _ held.1
  cases slot with
  | flags => exact False.elim (general written)
  | value v => exact ⟨loc, v, fits, found, written.2, read⟩

theorem mapM_ok_of_each {f : α → Except ε β} :
    ∀ (l : List α), (∀ x ∈ l, ∃ v, f x = .ok v) → ∃ vs, l.mapM f = .ok vs
  | [], _ => ⟨[], rfl⟩
  | x :: rest, each => by
    obtain ⟨v, hv⟩ := each x (by simp)
    obtain ⟨vs, hvs⟩ := mapM_ok_of_each rest (fun y m => each y (by simp [m]))
    exact ⟨v :: vs, by simp [List.mapM_cons, hv, hvs, bind, Except.bind, pure, Except.pure]⟩

/-- The ABI entry state holds in a fresh activation whose parameter places
hold the bound values. -/
theorem entry_holds {params : List VReg} {values : List Value} {st : Storage}
    (arity : params.length = values.length) (unique : distinctIds params = true)
    (general : ∀ p ∈ params, p.class ≠ .flags)
    (cells : ∀ i (h : i < params.length) (h' : i < values.length) loc,
      (argPlace false i).at params[i].class = some loc →
      readLoc st loc = .ok (.value (normalize params[i].class values[i]))) :
    Holds (entryState params) (Machine.bindValues [] (params.zip values)) st := by
  intro e member
  simp only [entryState, List.mem_map] at member
  obtain ⟨x, inZip, rfl⟩ := member
  have found := List.mem_zipIdx_iff_getElem?.mp inZip
  have lt : x.2 < params.length := by
    rcases List.getElem?_eq_some_iff.mp found with ⟨h, _⟩; exact h
  have same : params[x.2] = x.1 := by
    rcases List.getElem?_eq_some_iff.mp found with ⟨h, eq⟩; exact eq
  obtain ⟨loc, fits, _, _⟩ := argPlace_at (outgoing := false) (i := x.2) (general x.1 (same ▸ List.getElem_mem lt))
  have ids : ((params.zip values).map (fun p => p.1.id)).Nodup := by
    have firsts : (params.zip values).map Prod.fst = params := List.map_fst_zip (by omega)
    have := Machine.Transport.nodup_of_eraseDups_length (params.map VReg.id)
      (by simpa [distinctIds] using unique)
    rw [← firsts, List.map_map] at this
    exact this
  have inPairs : (x.1, values[x.2]'(by omega)) ∈ params.zip values := by
    rw [← same]
    have hz : x.2 < (params.zip values).length := by simp; omega
    simpa [List.getElem_zip] using List.getElem_mem hz
  refine ⟨loc, .value (normalize x.1.class values[x.2]), fits, ?_, ?_, ?_⟩
  · exact Machine.bindValues_lookup_member [] _ ids x.1 _ inPairs
  · exact ⟨general x.1 (same ▸ List.getElem_mem lt), normalize_idempotent _ _⟩
  · have := cells x.2 lt (by omega) loc (by rw [same]; exact fits)
    simpa [same] using this

theorem matchCall_some {E E' : Equations} {symbol symbol' : String} {args results : List VReg}
    {aargs aresults : List Loc}
    (found : matchCall pre post E symbol args results symbol' aargs aresults = some E') :
    ∃ params, preParams pre symbol = some params ∧ symbol = symbol' ∧ params.length = args.length ∧
      (∀ i (h : i < args.length) (h' : i < params.length), args[i].class = params[i].class) ∧
      (∀ i (h : i < args.length), heldAt E (argPlace true i) args[i] = true) ∧
      aargs = resultRegs (args.take abiRegisterCount) ∧ results.length ≤ abiRegisterCount ∧
      (∀ r ∈ results, r.class ≠ .flags) ∧ aresults = resultRegs results ∧
      distinctIds results = true ∧ returnViews post symbol (aresults.map Loc.width) = true ∧
      E' = callExit E results aresults := by
  unfold matchCall at found
  split at found
  · simp at found
  · rename_i params hp
    split at found
    · rename_i cond
      simp only [Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq, List.all_eq_true,
        bne_iff_ne, ne_eq, and_assoc] at cond
      obtain ⟨sym, len, classes, held, aargsEq, short, general, aresultsEq, distinct, views⟩ := cond
      cases found
      refine ⟨params, hp, sym, len, ?_, ?_, aargsEq, short, general, aresultsEq, distinct, views, rfl⟩
      · intro i h h'
        have hz : i < (args.zip params).length := by simp; omega
        have := classes _ (List.getElem_mem hz)
        simpa [List.getElem_zip] using this
      · intro i h
        exact held (args[i], i) (List.mem_zipIdx_iff_getElem?.mpr (by simp [h]))
    · simp at found

theorem preParams_some {symbol : String} {params : List VReg}
    (found : preParams pre symbol = some params) :
    ∃ callee entryBlock, pre.functions.find? (·.name == symbol) = some callee ∧
      callee.blocks.find? (·.id == 0) = some entryBlock ∧ entryBlock.args = params := by
  unfold preParams at found
  cases hf : pre.functions.find? (·.name == symbol) with
  | none => simp [hf] at found
  | some callee =>
    cases hb : callee.blocks.find? (·.id == 0) with
    | none => simp [hf, hb] at found
    | some entryBlock =>
      simp only [hf, hb, Option.bind_some, Option.some.injEq, bind, pure] at found
      exact ⟨callee, entryBlock, rfl, hb, found⟩


theorem alloc_execute_bl {p : Program} (t : State) (b : Activation) {g callee : Function}
    {block' entryBlock : Block} {symbol : String} {args results : List Loc}
    (foundG : p.functions.find? (·.name == b.function) = some g)
    (foundB : g.blocks.find? (·.id == b.block) = some block')
    (foundI : block'.insts[b.pc]? = some (.bl symbol args results))
    (abi : abiRegisters args = true)
    (foundC : p.functions.find? (·.name == symbol) = some callee)
    (foundE : callee.blocks.find? (·.id == 0) = some entryBlock) :
    execute p t b = .ok {t with
      control := .running ⟨callee.name, entryBlock.id, 0, callStorage b.storage args.length, t.nextFrame⟩,
      stack := ⟨{b with pc := b.pc+1}, results⟩ :: t.stack, nextFrame := t.nextFrame+1} := by
  simp [execute, foundG, foundB, foundI, abi, foundC, enter, foundE]
  rfl

theorem pre_enter {callee : Machine.Function} {entryBlock : Machine.Block} {values : List Value}
    (owner : Nat) (foundE : callee.blocks.find? (·.id == 0) = some entryBlock)
    (arity : entryBlock.args.length = values.length) (unique : distinctIds entryBlock.args = true) :
    Machine.enter callee values owner = .ok ⟨callee.name, entryBlock.id, 0,
      Machine.bindValues [] (entryBlock.args.zip values), owner, values⟩ := by
  have u : (entryBlock.args.map VReg.id).eraseDups.length = entryBlock.args.length := by
    simpa [distinctIds] using unique
  simp [Machine.enter, foundE, Machine.bind_exact [] _ _ arity u]
  rfl

theorem heldAt_general {E : Equations} {outgoing : Bool} {i : Nat} {r : VReg}
    (held : heldAt E (argPlace outgoing i) r = true) : r.class ≠ .flags := by
  intro isFlags
  simp only [heldAt, Bool.and_eq_true, Option.isSome_iff_exists] at held
  obtain ⟨_, loc, found⟩ := held
  rw [isFlags] at found
  unfold argPlace at found
  split at found <;> (try split at found) <;> simp [Place.at, Machine.RegClass.width] at found

theorem callStorage_argPlace (st : Storage) {i n : Nat} (lt : i < n) :
    callStorage st (min n abiRegisterCount) (argPlace false i) = st (argPlace true i) := by
  unfold argPlace
  split
  · simp [callStorage]; omega
  · simp [callStorage]

theorem width_eq_of_class {loc loc' : Loc} (general : loc ≠ .flags) (general' : loc' ≠ .flags)
    (same : loc.class = loc'.class) : loc.width = loc'.width := by
  rw [Loc.class_of_general general, Loc.class_of_general general'] at same
  cases h : loc.width <;> cases h' : loc'.width <;> simp_all [Machine.Width.class]

/-- A matched call enters the callee at its checked entry block. -/
theorem call_case (checked : checkAllocation pre post hints = true)
    {s s₁ : Machine.State} {t t' : State} {a : Machine.Activation} {b : Activation}
    {E E' X : Equations} {f : Machine.Function} {g : Function} {block : Machine.Block}
    {block' : Block} {fuel : Nat} {symbol symbol' : String} {args results : List VReg}
    {aargs aresults : List Loc} {os : List Machine.Instruction} {rest : List Instruction}
    (reach : Reaches (PreStep pre) entry s) (path : Reaches (PreStep pre) s s₁)
    (run₁ : s₁.control = .running a) (runB : t.control = .running b)
    (mem : s₁.memory = t.memory) (nf : s₁.nextFrame = t.nextFrame)
    (stack : StackRel pre post hints a.function s₁.stack t.stack)
    (fn : a.function = b.function) (bl : a.block = b.block) (ow : a.owner = b.owner)
    (foundF : pre.functions.find? (·.name == a.function) = some f)
    (foundG : post.functions.find? (·.name == a.function) = some g)
    (foundB : f.blocks.find? (·.id == a.block) = some block)
    (foundB' : g.blocks.find? (·.id == a.block) = some block')
    (preRest : block.insts.drop a.pc = .bl symbol args results :: os)
    (allocRest : block'.insts.drop b.pc = .bl symbol' aargs aresults :: rest)
    (walked : walk pre post fuel os rest E' = some X)
    (term : termOK (hintOf hints a.function) f.blocks g.blocks X block.term block'.term = true)
    (called : matchCall pre post E symbol args results symbol' aargs aresults = some E')
    (state : Holds E a.regs b.storage) (stepped : step post t = some t') :
    ∃ s', Relation.TransGen (Predecessor (PreStep pre)) s' s ∧ Related pre post hints entry s' t' := by
  obtain ⟨foundI, nextDrop, lt⟩ := drop_cons_at preRest
  obtain ⟨foundI', nextDrop', lt'⟩ := drop_cons_at allocRest
  have foundG' : post.functions.find? (·.name == b.function) = some g := fn ▸ foundG
  have foundB'' : g.blocks.find? (·.id == b.block) = some block' := bl ▸ foundB'
  obtain ⟨params, hp, symEq, len, classes, held, aargsEq, short, general, aresultsEq, distinct,
    views, exitEq⟩ := matchCall_some called
  subst symEq
  obtain ⟨callee, entryBlock, foundC, foundE, argsEq⟩ := preParams_some hp
  subst argsEq
  obtain ⟨unique, paramsGeneral, hintSub⟩ := entry_facts checked foundC foundE
  have argGeneral : ∀ i (h : i < args.length), args[i].class ≠ .flags :=
    fun i h => heldAt_general (held i h)
  -- The arguments are read before allocation.
  obtain ⟨values, readArgs⟩ := mapM_ok_of_each (f := Machine.read a.regs) args (by
    intro x member
    obtain ⟨i, h, rfl⟩ := List.getElem_of_mem member
    obtain ⟨_, v, _, slot, _, _⟩ := heldAt_read state (held i h) (argGeneral i h)
    exact ⟨v, by simp [Machine.read, slot]⟩)
  obtain ⟨valuesLen, eachRead⟩ := mapM_getElem readArgs
  have entered := pre_enter (callee := callee) (values := values) s₁.nextFrame foundE
    (by rw [valuesLen]; exact len) unique
  have edge := pre_step_ok run₁ (Machine.execute_bl .postSSA pre s₁ a f block symbol args results
    values callee _ foundF foundB foundI readArgs foundC entered)
  -- The allocated call.
  obtain ⟨g', entry', X', foundC', foundE', walkedE, termE⟩ := block_entry checked foundC foundE
  have abi : abiRegisters aargs = true := by
    rw [aargsEq]
    exact abiRegisters_resultRegs _ (List.length_take_le _ _) (by
      intro r member
      obtain ⟨i, h, rfl⟩ := List.getElem_of_mem member
      simp only [List.getElem_take]
      exact argGeneral i (by simp at h; omega))
  have same := alloc_step_ok runB (alloc_execute_bl t b foundG' foundB'' foundI' abi foundC' foundE')
  rw [stepped] at same
  cases same
  have aargsLen : aargs.length = min args.length abiRegisterCount := by
    rw [aargsEq, length_resultRegs, List.length_take, Nat.min_comm]
  have calleeName : callee.name = symbol := find_name foundC
  have calleeName' : g'.name = symbol := find_name' foundC'
  have entryId : entryBlock.id = 0 := by simpa using List.find?_some foundE
  have entryId' : entry'.id = 0 := by simpa using List.find?_some foundE'
  refine ⟨_, trans_of_reaches_step path edge, reach.trans (path.trans (.next edge (.refl _))),
    mem, by simp [nf], .inl ⟨_, _, rfl, rfl, ?_, ?_⟩⟩
  · -- The callee starts at its entry block with the ABI entry state.
    refine ⟨hintOf hints symbol 0, ?_, ?_⟩
    · have := cursor_entry checked (a := ⟨callee.name, entryBlock.id, 0,
          Machine.bindValues [] (entryBlock.args.zip values), s₁.nextFrame, values⟩)
        (b := ⟨g'.name, entry'.id, 0, callStorage b.storage aargs.length, t.nextFrame⟩)
        (by simpa [calleeName] using foundC) (by simpa [entryId] using foundE)
        (by simp [calleeName, calleeName']) (by simp [entryId, entryId']) (by simp [nf]) rfl rfl
      simpa [calleeName, entryId] using this
    · apply Holds.mono (fun e member => by
        simpa using List.all_eq_true.mp hintSub e member)
      apply entry_holds (by omega) unique (by simpa using paramsGeneral)
      intro i h h' loc fits
      obtain ⟨loc₁, v, fits₁, slot, normal, read⟩ := heldAt_read state (held i (by omega))
        (argGeneral i (by omega))
      have valueEq : values[i] = v := by
        have := eachRead i (by omega) h'
        simp [Machine.read, slot] at this
        exact this.symm
      have cellEq : callStorage b.storage aargs.length loc.place = b.storage loc₁.place := by
        rw [Place.at_place fits, Place.at_place fits₁, aargsLen]
        exact callStorage_argPlace b.storage (by omega)
      have paramGeneral : entryBlock.args[i].class ≠ .flags := by
        have := List.all_eq_true.mp paramsGeneral _ (List.getElem_mem h); simpa using this
      have locGeneral : loc ≠ .flags := fun e => paramGeneral ((Place.at_flags fits).mp e)
      have locGeneral₁ : loc₁ ≠ .flags := fun e => argGeneral i (by omega) ((Place.at_flags fits₁).mp e)
      have classEq := classes i (by omega) h
      have widthEq : loc.width = loc₁.width := width_eq_of_class locGeneral locGeneral₁
        (by rw [Place.at_class fits, Place.at_class fits₁, classEq])
      rw [readLoc_same_cell cellEq widthEq locGeneral locGeneral₁, read, valueEq, ← classEq, normal]
  · refine .cons _ _ _ _ _ ⟨aresultsEq, short, general, distinct, by simpa [calleeName] using views,
      ?_⟩ (by simpa using stack)
    refine ⟨(clobber E).filter (fun e => !(results.map (·.id)).contains e.2.id), ?_, ?_, ?_⟩
    · intro e member
      simp only [List.mem_filter, clobber, Bool.not_eq_true'] at member
      exact ⟨by simpa using member.1.2, by simpa using member.2⟩
    · exact Holds.mono (fun e member => by
        simp only [List.mem_filter, clobber] at member; exact member.1.1) state
    · exact ⟨fn, bl, ow, f, g, block, block', fuel, X, foundF, foundG, foundB, foundB',
        by simp; omega, by simp; omega, by simpa [nextDrop, nextDrop', callExit, exitEq] using walked,
        term⟩

end transfer

end EvoIR.Alloc
