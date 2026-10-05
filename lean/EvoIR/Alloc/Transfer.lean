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


/-! ## Terminators -/

theorem pre_jump {f : Machine.Function} (a : Machine.Activation) {target : Nat}
    (found : (f.blocks.find? (·.id == target)).isSome = true) :
    Machine.jump .postSSA f a target [] = .ok {a with block := target, pc := 0} := by
  obtain ⟨blk, h⟩ := Option.isSome_iff_exists.mp found
  simp [Machine.jump, h]; rfl

theorem alloc_jump {g : Function} (b : Activation) {target : Nat}
    (found : (g.blocks.find? (·.id == target)).isSome = true) :
    jump g b target = .ok {b with block := target, pc := 0} := by
  obtain ⟨blk, h⟩ := Option.isSome_iff_exists.mp found
  simp [jump, h]; rfl

theorem edgeOK_parts {hint : Nat → Equations} {fb : List Machine.Block} {gb : List Block}
    {X : Equations} {target : Nat} (edge : edgeOK hint fb gb X target = true) :
    (fb.find? (·.id == target)).isSome = true ∧ (gb.find? (·.id == target)).isSome = true ∧
      ∀ e ∈ hint target, e ∈ X := by
  simp only [edgeOK, hasBlock, Bool.and_eq_true, List.all_eq_true, List.contains_iff_mem] at edge
  exact ⟨edge.1.1, edge.1.2, edge.2⟩

/-- Both machines jump to the same checked block. -/
theorem jump_case (checked : checkAllocation pre post hints = true)
    {s s₁ : Machine.State} {t t' : State} {a : Machine.Activation} {b : Activation}
    {X : Equations} {f : Machine.Function} {g : Function} {target : Nat}
    (reach : Reaches (PreStep pre) entry s) (path : Reaches (PreStep pre) s s₁)
    (run₁ : s₁.control = .running a) (runB : t.control = .running b)
    (mem : s₁.memory = t.memory) (nf : s₁.nextFrame = t.nextFrame)
    (stack : StackRel pre post hints a.function s₁.stack t.stack)
    (fn : a.function = b.function) (ow : a.owner = b.owner)
    (foundF : pre.functions.find? (·.name == a.function) = some f)
    (edge : edgeOK (hintOf hints a.function) f.blocks g.blocks X target = true)
    (state : Holds X a.regs b.storage)
    (preExec : Machine.execute .postSSA pre s₁ a =
      .ok {s₁ with control := .running {a with block := target, pc := 0}})
    (allocExec : execute post t b =
      .ok {t with control := .running {b with block := target, pc := 0}})
    (stepped : step post t = some t') :
    ∃ s', Relation.TransGen (Predecessor (PreStep pre)) s' s ∧ Related pre post hints entry s' t' := by
  obtain ⟨preFound, _, sub⟩ := edgeOK_parts edge
  obtain ⟨blk, foundT⟩ := Option.isSome_iff_exists.mp preFound
  have stepEdge := pre_step_ok run₁ preExec
  have same := alloc_step_ok runB allocExec
  rw [stepped] at same
  cases same
  exact ⟨_, trans_of_reaches_step path stepEdge, reach.trans (path.trans (.next stepEdge (.refl _))),
    mem, nf, .inl ⟨_, _, rfl, rfl, ⟨hintOf hints a.function target,
      cursor_entry checked _ _ foundF foundT fn rfl ow rfl rfl,
      Holds.mono sub state⟩, stack⟩⟩

theorem readFlags_of_holds {X : Equations} {regs : RegisterFile} {st : Storage} {loc : Loc}
    {r : VReg} (state : Holds X regs st) (held : holds X loc r = true) :
    (∃ fl, Machine.readFlags regs r = .ok fl ∧ readFlags st loc = .ok fl) ∨
      Machine.readFlags regs r = .error (.malformed "MIR value read as flags") := by
  obtain ⟨slot, found, _, read⟩ := state.read held
  cases slot with
  | flags fl => exact .inl ⟨fl, by simp [Machine.readFlags, found], by simp [readFlags, read]; rfl⟩
  | value v => exact .inr (by simp [Machine.readFlags, found])

theorem readBits_of_holds {X : Equations} {regs : RegisterFile} {st : Storage} {loc : Loc}
    {r : VReg} (state : Holds X regs st) (held : holds X loc r = true) :
    (∃ bits, Machine.readBits regs r = .ok bits ∧ readBits st loc = .ok bits) ∨
      ∃ m, Machine.readBits regs r = .error (.malformed m) := by
  obtain ⟨slot, found, _, read⟩ := state.read held
  cases slot with
  | flags fl => exact .inr ⟨_, by simp [Machine.readBits, Machine.read, found]; rfl⟩
  | value v =>
    cases v with
    | int bits => exact .inl ⟨bits, by simp [Machine.readBits, Machine.read, found]; rfl,
        by simp [readBits, readValue, read]; rfl⟩
    | pointer o off => exact .inr ⟨_, by simp [Machine.readBits, Machine.read, found]; rfl⟩


/-- Both machines fail with the same fault. -/
theorem fault_case {s s₁ : Machine.State} {t t' : State} {a : Machine.Activation} {b : Activation}
    {e : Core.Source.Fault}
    (reach : Reaches (PreStep pre) entry s) (path : Reaches (PreStep pre) s s₁)
    (run₁ : s₁.control = .running a) (runB : t.control = .running b)
    (mem : s₁.memory = t.memory) (nf : s₁.nextFrame = t.nextFrame)
    (preExec : Machine.execute .postSSA pre s₁ a = .error e) (allocExec : execute post t b = .error e)
    (stepped : step post t = some t') :
    ∃ s', Relation.TransGen (Predecessor (PreStep pre)) s' s ∧ Related pre post hints entry s' t' := by
  have stepEdge := pre_step_error run₁ preExec
  have same := alloc_step_error runB allocExec
  rw [stepped] at same
  cases same
  exact ⟨_, trans_of_reaches_step path stepEdge, reach.trans (path.trans (.next stepEdge (.refl _))),
    mem, nf, .inr ⟨.error e, rfl, rfl⟩⟩

theorem gpr_loc_eq {loc loc' : Loc} {n : Nat} (h : loc.place = .gpr n) (h' : loc'.place = .gpr n)
    (w : loc.width = loc'.width) : loc = loc' := by
  cases loc <;> cases loc' <;> simp_all [Loc.place, Loc.width]

theorem returnStorage_kept (caller callee : Storage) {n : Nat} {place : Place} (short : n ≤ 8)
    (kept : place.callClobbered = false) : returnStorage caller callee n place = caller place := by
  cases place <;> simp_all [returnStorage, Place.callClobbered]
  rename_i number
  rw [if_neg (by omega), if_neg (by omega)]

theorem mapM_congr_getElem {f : α → Except ε γ} {g : β → Except ε γ} :
    ∀ {l : List α} {l' : List β}, l.length = l'.length →
      (∀ i (h : i < l.length) (h' : i < l'.length), f l[i] = g l'[i]) → l.mapM f = l'.mapM g
  | [], [], _, _ => rfl
  | x :: l, y :: l', len, each => by
    have head := each 0 (by simp) (by simp)
    have tail := mapM_congr_getElem (f := f) (g := g) (l := l) (l' := l') (by simpa using len)
      (fun i h h' => each (i+1) (by simpa using h) (by simpa using h'))
    simp only [List.mapM_cons, List.getElem_cons_zero] at head ⊢
    rw [head, tail]


theorem StackRel.cons_inv {callee : String} {fr : Machine.ReturnFrame}
    {rest : List Machine.ReturnFrame} {l : List ReturnFrame}
    (h : StackRel pre post hints callee (fr :: rest) l) :
    ∃ fr' rest', l = fr' :: rest' ∧ FrameRel pre post hints callee fr fr' ∧
      StackRel pre post hints fr.caller.function rest rest' := by
  cases h with
  | cons _ _ fr' _ rest' frame tail => exact ⟨fr', rest', rfl, frame, tail⟩

theorem StackRel.nil_inv {callee : String} {l : List ReturnFrame}
    (h : StackRel pre post hints callee [] l) : l = [] := by
  cases h; rfl

/-- The return values agree, and the result registers after a return hold
the returned values. -/
theorem return_values {X : Equations} {regs : RegisterFile} {st : Storage} {vs : List VReg}
    {ls : List Loc} (state : Holds X regs st) (abi : abiRegisters ls = true)
    (len : vs.length = ls.length) (held : (vs.zip ls).all (fun e => holds X e.2 e.1) = true) :
    ∃ values, vs.mapM (Machine.read regs) = .ok values ∧ ls.mapM (readValue st) = .ok values ∧
      ∀ i (h : i < vs.length) (h' : i < ls.length) (h'' : i < values.length),
        readLoc st ls[i] = .ok (.value values[i]) ∧ normalize vs[i].class values[i] = values[i] ∧
        ls[i].class = vs[i].class := by
  have abiAt := abiRegistersFrom_getElem ls 0 (by simp [abiRegisters] at abi; exact abi.2)
  have each : ∀ i (h : i < vs.length) (h' : i < ls.length), ∃ v,
      Machine.readSlot regs vs[i] = some (.value v) ∧ normalize vs[i].class v = v ∧
      readLoc st ls[i] = .ok (.value v) ∧ ls[i].class = vs[i].class := by
    intro i h h'
    have hi : holds X ls[i] vs[i] = true := zip_all_getElem held i h h'
    obtain ⟨slot, found, written, read⟩ := state.read hi
    have fits := (holds_iff.mp hi).2
    have general : vs[i].class ≠ .flags := by
      intro isFlags
      have := (Place.at_flags fits).mpr isFlags
      have p := abiAt i h'
      rw [this] at p
      cases p
    cases slot with
    | flags => exact False.elim (general written)
    | value v => exact ⟨v, found, written.2, read, Place.at_class fits⟩
  obtain ⟨values, readVs⟩ := mapM_ok_of_each (f := Machine.read regs) vs (by
    intro x member
    obtain ⟨i, h, rfl⟩ := List.getElem_of_mem member
    obtain ⟨v, found, _⟩ := each i h (by omega)
    exact ⟨v, by simp [Machine.read, found]⟩)
  obtain ⟨valuesLen, eachRead⟩ := mapM_getElem readVs
  refine ⟨values, readVs, ?_, ?_⟩
  · rw [← readVs]
    apply mapM_congr_getElem len.symm
    intro i h h'
    obtain ⟨v, found, _, read, _⟩ := each i h' h
    simp [readValue, read, Machine.read, found]; rfl
  · intro i h h' h''
    obtain ⟨v, found, normal, read, cls⟩ := each i h h'
    have : values[i] = v := by
      have := eachRead i h h''
      simp [Machine.read, found] at this
      exact this.symm
    rw [this]
    exact ⟨read, normal, cls⟩


section term
variable {type : Core.Source.IntType}

/-- A terminator step. The pre-allocation activation is at the end of its
block after the renamed moves. -/
theorem terminator_case (checked : checkAllocation pre post hints = true)
    (progress : Machine.ObservedProgress .postSSA pre type entry)
    {s s₁ : Machine.State} {t t' : State} {a : Machine.Activation} {b : Activation}
    {X : Equations} {f : Machine.Function} {g : Function} {block : Machine.Block} {block' : Block}
    (reach : Reaches (PreStep pre) entry s) (path : Reaches (PreStep pre) s s₁)
    (run₁ : s₁.control = .running a) (runB : t.control = .running b)
    (mem : s₁.memory = t.memory) (nf : s₁.nextFrame = t.nextFrame)
    (stack : StackRel pre post hints a.function s₁.stack t.stack)
    (fn : a.function = b.function) (bl : a.block = b.block) (ow : a.owner = b.owner)
    (foundF : pre.functions.find? (·.name == a.function) = some f)
    (foundG : post.functions.find? (·.name == a.function) = some g)
    (foundB : f.blocks.find? (·.id == a.block) = some block)
    (foundB' : g.blocks.find? (·.id == a.block) = some block')
    (preAt : block.insts.drop a.pc = []) (preLe : a.pc ≤ block.insts.length)
    (allocAt : block'.insts.drop b.pc = []) (allocLe : b.pc ≤ block'.insts.length)
    (term : termOK (hintOf hints a.function) f.blocks g.blocks X block.term block'.term = true)
    (state : Holds X a.regs b.storage) (stepped : step post t = some t') :
    ∃ s', Relation.TransGen (Predecessor (PreStep pre)) s' s ∧ Related pre post hints entry s' t' := by
  obtain ⟨atEnd, noI⟩ := drop_nil_at preAt preLe
  obtain ⟨atEnd', noI'⟩ := drop_nil_at allocAt allocLe
  have foundG' : post.functions.find? (·.name == b.function) = some g := fn ▸ foundG
  have foundB'' : g.blocks.find? (·.id == b.block) = some block' := bl ▸ foundB'
  have bad : ∀ m, Machine.execute .postSSA pre s₁ a = .error (.malformed m) → False :=
    fun m h => no_malformed progress (reach.trans path) run₁ h
  have pe : Machine.execute .postSSA pre s₁ a = Machine.execute .postSSA pre s₁ a := rfl
  have ae : execute post t b = execute post t b := rfl
  conv at pe => rhs; simp only [Machine.execute, foundF, foundB, atEnd,
    List.getElem?_eq_none (Nat.le_refl _), bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
  conv at ae => rhs; simp only [execute, foundG', foundB'', atEnd',
    List.getElem?_eq_none (Nat.le_refl _), bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
  have jumpTo := fun (target : Nat) (edge : edgeOK (hintOf hints a.function) f.blocks g.blocks X target = true)
      (preExec : Machine.execute .postSSA pre s₁ a = .ok {s₁ with control := .running {a with block := target, pc := 0}})
      (allocExec : execute post t b = .ok {t with control := .running {b with block := target, pc := 0}}) =>
    jump_case checked reach path run₁ runB mem nf stack fn ow foundF edge state preExec allocExec stepped
  have jumps := fun (target : Nat) (edge : edgeOK (hintOf hints a.function) f.blocks g.blocks X target = true) =>
    And.intro (pre_jump a (edgeOK_parts edge).1) (alloc_jump b (edgeOK_parts edge).2.1)
  cases ht : block.term <;> cases ht' : block'.term <;>
    simp only [termOK, ht, ht', Bool.false_eq_true, Bool.and_eq_true, beq_iff_eq,
      List.isEmpty_iff] at term <;> simp only [ht] at pe <;> simp only [ht'] at ae
  case b.b target args target' =>
    obtain ⟨⟨rfl, rfl⟩, edge⟩ := term
    obtain ⟨pj, aj⟩ := jumps target edge
    exact jumpTo target edge (by rw [pe, pj]; rfl) (by rw [ae, aj]; rfl)
  case bCond.bCond flags cond taken ta untaken ua loc cond' taken' untaken' =>
    obtain ⟨⟨⟨⟨⟨⟨⟨rfl, rfl⟩, rfl⟩, rfl⟩, rfl⟩, held⟩, edgeT⟩, edgeU⟩ := term
    rcases readFlags_of_holds state held with ⟨fl, pr, ar⟩ | pr
    · cases hc : Machine.condition fl cond with
      | error e =>
        exact fault_case (e := e) reach path run₁ runB mem nf (by rw [pe, pr]; simp [bind, Except.bind, hc])
          (by rw [ae, ar]; simp [bind, Except.bind, hc]) stepped
      | ok yes =>
        cases yes
        · obtain ⟨pj, aj⟩ := jumps untaken edgeU
          exact jumpTo untaken edgeU (by rw [pe, pr]; simp [bind, Except.bind, hc, pj]; rfl)
            (by rw [ae, ar]; simp [bind, Except.bind, hc, aj]; rfl)
        · obtain ⟨pj, aj⟩ := jumps taken edgeT
          exact jumpTo taken edgeT (by rw [pe, pr]; simp [bind, Except.bind, hc, pj]; rfl)
            (by rw [ae, ar]; simp [bind, Except.bind, hc, aj]; rfl)
    · exact (bad _ (by rw [pe, pr]; rfl)).elim
  case cb.cb nonzero src taken ta untaken ua nonzero' loc taken' untaken' =>
    obtain ⟨⟨⟨⟨⟨⟨⟨rfl, rfl⟩, rfl⟩, rfl⟩, rfl⟩, held⟩, edgeT⟩, edgeU⟩ := term
    have cls : loc.class = src.class := Place.at_class (holds_iff.mp held).2
    cases hw : src.class.width with
    | none => exact (bad _ (by rw [pe, hw]; rfl)).elim
    | some w =>
      rcases readBits_of_holds state held with ⟨bits, pr, ar⟩ | ⟨m, pr⟩
      · by_cases yes : (BitVec.setWidth w.bits bits != 0#w.bits) = nonzero
        · obtain ⟨pj, aj⟩ := jumps taken edgeT
          exact jumpTo taken edgeT (by rw [pe, hw, pr]; simp [bind, Except.bind, yes, pj]; rfl)
            (by rw [ae, cls, hw, ar]; simp [bind, Except.bind, yes, aj]; rfl)
        · obtain ⟨pj, aj⟩ := jumps untaken edgeU
          exact jumpTo untaken edgeU (by rw [pe, hw, pr]; simp [bind, Except.bind, yes, pj]; rfl)
            (by rw [ae, cls, hw, ar]; simp [bind, Except.bind, yes, aj]; rfl)
      · exact (bad m (by rw [pe, hw, pr]; rfl)).elim
  case tb.tb nonzero src bit taken ta untaken ua nonzero' loc bit' taken' untaken' =>
    obtain ⟨⟨⟨⟨⟨⟨⟨⟨rfl, rfl⟩, rfl⟩, rfl⟩, rfl⟩, rfl⟩, held⟩, edgeT⟩, edgeU⟩ := term
    by_cases large : bit ≥ 64
    · exact (bad _ (by rw [pe]; simp [large]; rfl)).elim
    · rcases readBits_of_holds state held with ⟨bits, pr, ar⟩ | ⟨m, pr⟩
      · by_cases yes : (bits.getLsbD bit == nonzero) = true
        · obtain ⟨pj, aj⟩ := jumps taken edgeT
          exact jumpTo taken edgeT (by rw [pe]; simp [large, pr, bind, Except.bind, yes, pj]; rfl)
            (by rw [ae]; simp [large, ar, bind, Except.bind, yes, aj]; rfl)
        · obtain ⟨pj, aj⟩ := jumps untaken edgeU
          exact jumpTo untaken edgeU (by rw [pe]; simp [large, pr, bind, Except.bind, yes, pj]; rfl)
            (by rw [ae]; simp [large, ar, bind, Except.bind, yes, aj]; rfl)
      · exact (bad m (by rw [pe]; simp [large, pr, bind, Except.bind])).elim
  case brk.brk imm imm' =>
    subst term
    exact fault_case (e := .diagnostic imm) reach path run₁ runB mem nf (by rw [pe]; rfl) (by rw [ae]; rfl)
      stepped
  case ret.ret vs ls =>
    obtain ⟨⟨abi, len⟩, held⟩ := term
    obtain ⟨values, rv, rl, each⟩ := return_values state abi len held
    have hasRet : block' ∈ g.blocks := List.mem_of_find?_eq_some foundB'
    simp only [rv, abi, rl, Bool.not_true, Bool.false_eq_true, ↓reduceIte, bind, Except.bind] at pe ae
    cases hS : s₁.stack with
    | nil =>
      have tS := StackRel.nil_inv (hS ▸ stack)
      have edge := pre_step_ok run₁ (by rw [pe, hS]; rfl)
      have same := alloc_step_ok runB (by rw [ae, tS]; rfl)
      rw [stepped] at same
      cases same
      exact ⟨_, trans_of_reaches_step path edge, reach.trans (path.trans (.next edge (.refl _))),
        by simp [mem, ow], nf, .inr ⟨.ok values, rfl, rfl⟩⟩
    | cons fr rest =>
      obtain ⟨fr', rest', tS, frame, stackRest⟩ := StackRel.cons_inv (hS ▸ stack)
      obtain ⟨resEq, short, general, distinct, views, K, kept, holdsK, cursorK⟩ := frame
      have viewsLs : ls.map Loc.width = fr'.results.map Loc.width := by
        simp only [returnViews, foundG] at views
        have := List.all_eq_true.mp views block' hasRet
        simpa [ht', beq_iff_eq] using this
      have lenR : fr'.results.length = ls.length := by
        simpa using (congrArg List.length viewsLs).symm
      have lenF : fr.results.length = fr'.results.length := by rw [resEq, length_resultRegs]
      have valuesLen := (mapM_getElem rv).1
      have abiR : abiRegisters fr'.results = true := resEq ▸ abiRegisters_resultRegs _ short general
      have lsShort : ls.length ≤ 8 := by simp [abiRegisters] at abi; exact abi.1
      have unique : (fr.results.map VReg.id).eraseDups.length = fr.results.length := by
        simpa [distinctIds] using distinct
      have bound := Machine.bind_exact fr.caller.regs fr.results values (by omega) unique
      have edge := pre_step_ok run₁ (by rw [pe, hS]; simp only [bound]; rfl)
      have same := alloc_step_ok runB (by rw [ae, tS]; simp [abiR, lenR]; rfl)
      rw [stepped] at same
      cases same
      refine ⟨_, trans_of_reaches_step path edge, reach.trans (path.trans (.next edge (.refl _))),
        by simp [mem, ow], nf, .inl ⟨_, _, rfl, rfl, ⟨_, cursorK, ?_⟩, stackRest⟩⟩
      intro e member
      rcases List.mem_append.mp member with inRes | inK
      · obtain ⟨p, inZip, rfl⟩ := List.mem_map.mp inRes
        obtain ⟨i, hi, rfl⟩ := List.getElem_of_mem inZip
        simp only [List.length_zip] at hi
        have hR : i < fr.results.length := by omega
        have hR' : i < fr'.results.length := by omega
        simp only [List.getElem_zip]
        have rGeneral := general _ (List.getElem_mem hR)
        have locEq : fr'.results[i] = regLoc i fr.results[i] := by
          simp only [resEq, getElem_resultRegs]
        have fitsR := regLoc_at (i := i) rGeneral
        have lsEq : ls[i]'(by omega) = regLoc i fr.results[i] := by
          apply gpr_loc_eq (abiRegistersFrom_getElem ls 0 (by simp [abiRegisters] at abi; exact abi.2) i
            (by omega)) (by rw [regLoc_place rGeneral]; simp)
          have w := List.getElem_of_eq viewsLs (i := i) (by simp; omega)
          simp only [List.getElem_map] at w
          rw [w, locEq]
        obtain ⟨read, normal, cls⟩ := each i (by omega) (by omega) (by omega)
        have clsEq : fr.results[i].class = vs[i].class := by
          rw [← Place.at_class fitsR, ← lsEq, cls]
        refine ⟨regLoc i fr.results[i], .value values[i], by rw [locEq, regLoc_place rGeneral]; exact fitsR,
          ?_, ⟨rGeneral, by rw [clsEq]; exact normal⟩, ?_⟩
        · have ids : ((fr.results.zip values).map (fun p => p.1.id)).Nodup := by
            have firsts : (fr.results.zip values).map Prod.fst = fr.results := List.map_fst_zip (by omega)
            have := Machine.Transport.nodup_of_eraseDups_length (fr.results.map VReg.id)
              (by simpa using unique)
            rw [← firsts, List.map_map] at this
            exact this
          have inPairs : (fr.results[i], values[i]'(by omega)) ∈ fr.results.zip values := by
            have hz : i < (fr.results.zip values).length := by simp; omega
            simpa [List.getElem_zip] using List.getElem_mem hz
          have := Machine.bindValues_lookup_member fr.caller.regs _ ids _ _ inPairs
          simp only [Machine.readSlot, this, clsEq, normal]
        · rw [readLoc_congr (st' := b.storage) (by
            rw [regLoc_place rGeneral]; simp [returnStorage]; omega), ← lsEq]
          exact read
      · obtain ⟨loc, slot, fits, found, written, read⟩ := holdsK e inK
        obtain ⟨notClob, notRes⟩ := kept e inK
        refine ⟨loc, slot, fits, ?_, written, ?_⟩
        · unfold Machine.readSlot at found ⊢
          rw [Machine.bindValues_lookup_other]
          · exact found
          · intro p hp same
            have inResults : p.1 ∈ fr.results := (List.of_mem_zip hp).1
            have : (fr.results.map (·.id)).contains e.2.id = true := by
              simp only [List.contains_iff_mem, List.mem_map]; exact ⟨p.1, inResults, same.symm⟩
            rw [this] at notRes; cases notRes
        · rw [readLoc_congr (st' := fr'.caller.storage) (by
            rw [Place.at_place fits]; exact returnStorage_kept _ _ (by omega) notClob)]
          exact read


/-- Each allocated step matches a positive number of steps before allocation,
or it is an inserted copy that decreases the rank. -/
theorem transfer (checked : checkAllocation pre post hints = true)
    (progress : Machine.ObservedProgress .postSSA pre type entry)
    {s : Machine.State} {t t' : State} (rel : Related pre post hints entry s t)
    (stepped : step post t = some t') :
    (∃ s', Relation.TransGen (Predecessor (PreStep pre)) s' s ∧ Related pre post hints entry s' t') ∨
      (Related pre post hints entry s t' ∧ rank post s t' < rank post s t) := by
  obtain ⟨reach, mem, nf, ctrl⟩ := rel
  rcases ctrl with ⟨a, b, runA, runB, ⟨E, ⟨fn, bl, ow, f, g, block, block', fuel, X, foundF,
    foundG, foundB, foundB', le, le', walked, term⟩, state⟩, stack⟩ | ⟨r, _, haltB⟩
  case inr => simp [step, haltB] at stepped
  -- After the renamed moves before allocation.
  have settled := fun (as : List Instruction) (h : block'.insts.drop b.pc = as) (nc : NoCopyNext as) =>
    settle progress b.storage fuel s a E X f block as reach runA foundF foundB le (h ▸ walked) nc state
  have moved := fun (a₁ : Machine.Activation) (fn₁ : a₁.function = a.function) (bl₁ : a₁.block = a.block)
      (ow₁ : a₁.owner = a.owner) =>
    And.intro (show ({s with control := .running a₁} : Machine.State).control = .running a₁ from rfl)
      (And.intro (fn₁.trans fn) (And.intro (bl₁.trans bl) (And.intro (ow₁.trans ow)
        (And.intro (fn₁ ▸ foundF) (And.intro (fn₁ ▸ foundG) (And.intro (bl₁ ▸ foundB)
          (And.intro (bl₁ ▸ foundB')
            (show StackRel pre post hints a₁.function s.stack t.stack by rw [fn₁]; exact stack))))))))
  cases allocRest : block'.insts.drop b.pc with
  | cons inst rest =>
    cases isCopy : inst.copyOf with
    | some pair =>
      obtain ⟨dst, src⟩ := pair
      obtain ⟨fuel', rfl, done | ⟨os', as', E', found, walked'⟩⟩ := walk_inv walked
      · rw [allocRest] at done
        simp only [next, isCopy] at done
        split at done <;> (try split at done) <;> cases done
      · rw [allocRest] at found
        simp only [next, isCopy] at found
        split at found
        · rename_i os'' E'' changed
          cases found
          obtain ⟨d, src', ld, ls, osEq, instEq, _, general, held, fits, result⟩ := classChange_some changed
          subst instEq
          exact .inl (instruction_case reach (.refl _) runA runB mem nf stack fn bl ow foundF foundG
            foundB foundB' osEq allocRest walked' term (by intro _ _ _ h; cases h) (fun _ => ls)
            (fun _ => ld) rfl (by simp [Machine.Instruction.uses, held])
            (by simpa [Machine.Instruction.defs, Loc.fits_iff] using fits) general
            (by simp [Machine.Instruction.defs, defineAll, result]) state stepped)
        · split at found
          · rename_i defined
            cases found
            exact .inr (copy_case reach runA runB mem nf stack fn bl ow foundF foundG foundB foundB'
              le allocRest walked' term isCopy defined state stepped)
          · cases found
    | none =>
      obtain ⟨a₁, E₁, fuel₁, path, fn₁, bl₁, ow₁, _, walked₁, state₁, settledHead⟩ :=
        settled (inst :: rest) allocRest (by simp [NoCopyNext, isCopy])
      obtain ⟨run₁, fnA, blA, owA, foundF₁, foundG₁, foundB₁, foundB₁', stack₁⟩ := moved a₁ fn₁ bl₁ ow₁
      obtain ⟨fuel₂, rfl, done | ⟨os', as', E', found, walked'⟩⟩ := walk_inv walked₁
      · cases hd : block.insts.drop a₁.pc with
        | nil => rw [hd] at done; simp [next, isCopy] at done
        | cons o os =>
          rw [hd] at done
          simp only [next, isCopy] at done
          split at done <;> (try split at done) <;> cases done
      · cases preRest : block.insts.drop a₁.pc with
        | nil => rw [preRest] at found; simp [next, isCopy] at found
        | cons o os =>
          rw [preRest] at found settledHead
          have noRename : renameOf o = none := settledHead o (by simp)
          simp only [next, isCopy, noRename] at found
          split at found
          · rename_i E'' matched
            cases found
            rcases matchStep_some matched with
              ⟨symbol, args, results, symbol', aargs, aresults, rfl, rfl, called⟩ | ⟨notCall, orig⟩
            · exact .inl (call_case checked reach path run₁ runB mem nf stack₁ fnA blA owA foundF₁
                foundG₁ foundB₁ foundB₁' preRest allocRest walked' (fn₁ ▸ term) called state₁ stepped)
            · obtain ⟨shape, uses, defs, outputs, result⟩ := matchOriginal_some orig
              exact .inl (instruction_case reach path run₁ runB mem nf stack₁ fnA blA owA foundF₁
                foundG₁ foundB₁ foundB₁' preRest allocRest walked' (fn₁ ▸ term) notCall _ _ shape uses defs
                outputs result state₁ stepped)
          · cases found
  | nil =>
    obtain ⟨a₁, E₁, fuel₁, path, fn₁, bl₁, ow₁, le₁, walked₁, state₁, settledHead⟩ :=
      settled [] allocRest (by simp [NoCopyNext])
    obtain ⟨run₁, fnA, blA, owA, foundF₁, foundG₁, foundB₁, foundB₁', stack₁⟩ := moved a₁ fn₁ bl₁ ow₁
    obtain ⟨fuel₂, rfl, done | ⟨os', as', E', found, _⟩⟩ := walk_inv walked₁
    · cases preRest : block.insts.drop a₁.pc with
      | nil =>
        rw [preRest] at done
        simp only [next, Next.done.injEq] at done
        subst done
        exact .inl (terminator_case checked progress reach path run₁ runB mem nf stack₁ fnA blA owA
          foundF₁ foundG₁ foundB₁ foundB₁' preRest le₁ allocRest le' (fn₁ ▸ term) state₁ stepped)
      | cons o os =>
        rw [preRest] at done settledHead
        simp [next, settledHead o (by simp)] at done
    · cases preRest : block.insts.drop a₁.pc with
      | nil => rw [preRest] at found; simp [next] at found
      | cons o os =>
        rw [preRest] at found settledHead
        simp [next, settledHead o (by simp)] at found

end term

end transfer

end EvoIR.Alloc
