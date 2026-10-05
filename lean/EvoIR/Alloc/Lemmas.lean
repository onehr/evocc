import EvoIR.Alloc.Check
import EvoIR.MachineTransportBindings

/-!
Lemmas for the allocation soundness proof: locations, storage reads and
writes, the meaning of an abstract state, and one generic lemma about the
semantics before allocation. The generic lemma says that an instruction reads
only its used registers and writes only its defined registers, so that a
renaming of the operands gives the same operation. No lemma in this file is
specific to one instruction form of the simulation.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Alloc

open EvoIR.Machine (VReg RegClass Width Value Slot RegisterFile normalize normalize_idempotent)

/-! ## Locations -/

private theorem digit (n k : Nat) (small : k < 10) : (10 * n + k) % 10 = k ∧ (10 * n + k) / 10 = n := by
  omega

theorem Loc.decode_code (loc : Loc) : Loc.decode loc.code = some loc := by
  cases loc with
  | gpr n view =>
    cases view
    · have := digit n 0 (by omega); simp only [Nat.add_zero] at this
      simp [Loc.code, Loc.decode, this.1, this.2]
    · have := digit n 1 (by omega); simp [Loc.code, Loc.decode, this.1, this.2]
  | flags => rfl
  | spill n view =>
    cases view
    · have := digit n 3 (by omega); simp [Loc.code, Loc.decode, this.1, this.2]
    · have := digit n 4 (by omega); simp [Loc.code, Loc.decode, this.1, this.2]
  | incoming n view =>
    cases view
    · have := digit n 5 (by omega); simp [Loc.code, Loc.decode, this.1, this.2]
    · have := digit n 6 (by omega); simp [Loc.code, Loc.decode, this.1, this.2]
  | outgoing n view =>
    cases view
    · have := digit n 7 (by omega); simp [Loc.code, Loc.decode, this.1, this.2]
    · have := digit n 8 (by omega); simp [Loc.code, Loc.decode, this.1, this.2]

theorem Place.at_place {place : Place} {regClass : RegClass} {loc : Loc}
    (found : place.at regClass = some loc) : loc.place = place := by
  cases place <;> cases regClass <;> simp [Place.at, RegClass.width] at found <;>
    subst found <;> rfl

theorem Place.at_class {place : Place} {regClass : RegClass} {loc : Loc}
    (found : place.at regClass = some loc) : loc.class = regClass := by
  cases place <;> cases regClass <;> simp [Place.at, RegClass.width] at found <;>
    subst found <;> rfl

theorem Place.at_flags {place : Place} {regClass : RegClass} {loc : Loc}
    (found : place.at regClass = some loc) : loc = .flags ↔ regClass = .flags := by
  cases place <;> cases regClass <;> simp [Place.at, RegClass.width] at found <;>
    subst found <;> simp

theorem Place.at_width {place : Place} {regClass : RegClass} {loc : Loc}
    (found : place.at regClass = some loc) (general : regClass ≠ .flags) :
    regClass.width = some loc.width := by
  cases place <;> cases regClass <;> simp [Place.at, RegClass.width] at found general ⊢ <;>
    subst found <;> rfl

theorem Loc.fits_iff {loc : Loc} {regClass : RegClass} :
    loc.fits regClass = true ↔ loc.place.at regClass = some loc := by
  simp [Loc.fits]

theorem Loc.class_of_general {loc : Loc} (general : loc ≠ .flags) : loc.class = loc.width.class := by
  cases loc <;> simp_all [Loc.class]

theorem Loc.reg_class (loc : Loc) : loc.reg.class = loc.class := rfl

/-! ## Storage -/

theorem readLoc_congr {st st' : Storage} {loc : Loc} (same : st loc.place = st' loc.place) :
    readLoc st loc = readLoc st' loc := by
  unfold readLoc
  rw [same]

theorem writeLoc_same_place (st : Storage) (loc : Loc) (slot : Slot) :
    writeLoc st loc slot loc.place = some (loc.cell slot) := by
  simp [writeLoc]

theorem writeLoc_other_place (st : Storage) (loc : Loc) (slot : Slot) (place : Place)
    (other : place ≠ loc.place) : writeLoc st loc slot place = st place := by
  simp [writeLoc, other]

theorem readLoc_writeLoc_other (st : Storage) (loc : Loc) (slot : Slot) (loc' : Loc)
    (other : loc'.place ≠ loc.place) : readLoc (writeLoc st loc slot) loc' = readLoc st loc' :=
  readLoc_congr (writeLoc_other_place st loc slot loc'.place other)

/-- A slot written through a location that fits its register class is read
back unchanged. -/
theorem readLoc_writeLoc_same (st : Storage) (loc : Loc) (reg : VReg) (slot : Slot)
    (fits : loc.place.at reg.class = some loc) (written : slot.WrittenAt reg) :
    readLoc (writeLoc st loc slot) loc = .ok slot := by
  have cls := Place.at_class fits
  have flagsIff := Place.at_flags fits
  cases slot with
  | flags nzcv =>
    simp only [Machine.Slot.WrittenAt] at written
    have isFlags : loc = .flags := flagsIff.mpr written
    subst isFlags
    simp [readLoc, writeLoc, Loc.cell, Loc.place]
  | value v =>
    simp only [Machine.Slot.WrittenAt] at written
    obtain ⟨general, normal⟩ := written
    have notFlags : loc ≠ .flags := fun h => general (flagsIff.mp h)
    have widthClass : loc.width.class = reg.class := by
      rw [← Loc.class_of_general notFlags]; exact cls
    cases loc with
    | flags => exact False.elim (notFlags rfl)
    | gpr n view =>
      simp only [Loc.width] at widthClass
      simp [readLoc, writeLoc, Loc.cell, Loc.place, Loc.written, Loc.width, widthClass, normal]
    | spill n view =>
      simp only [Loc.width] at widthClass
      cases view <;> simp [readLoc, writeLoc, Loc.cell, Loc.place, Loc.written, Loc.width,
        widthClass, normal]
    | incoming n view =>
      simp only [Loc.width] at widthClass
      cases view <;> simp [readLoc, writeLoc, Loc.cell, Loc.place, Loc.written, Loc.width,
        widthClass, normal]
    | outgoing n view =>
      simp only [Loc.width] at widthClass
      cases view <;> simp [readLoc, writeLoc, Loc.cell, Loc.place, Loc.written, Loc.width,
        widthClass, normal]

/-- Two general locations with the same width read the same cell equally. -/
theorem readLoc_same_cell {st st' : Storage} {loc loc' : Loc}
    (cell : st loc.place = st' loc'.place) (width : loc.width = loc'.width)
    (general : loc ≠ .flags) (general' : loc' ≠ .flags) :
    readLoc st loc = readLoc st' loc' := by
  unfold readLoc
  rw [cell]
  cases st' loc'.place with
  | none => rfl
  | some c =>
    rcases c with ⟨slot, defined⟩
    cases slot with
    | flags => rfl
    | value v =>
      cases loc <;> cases loc' <;> simp_all [Loc.width]

/-! ## The meaning of an abstract state -/

/-- Each equation `(place, reg)` holds: `reg` has a slot before allocation,
the slot is normal for the class of `reg`, and `place`, read through the view
of that class, gives the same slot. -/
def Holds (E : Equations) (regs : RegisterFile) (st : Storage) : Prop :=
  ∀ e ∈ E, ∃ loc slot, e.1.at e.2.class = some loc ∧ Machine.readSlot regs e.2 = some slot ∧
    slot.WrittenAt e.2 ∧ readLoc st loc = .ok slot

theorem Holds.mono {E E' : Equations} {regs : RegisterFile} {st : Storage}
    (sub : ∀ e ∈ E', e ∈ E) (holds : Holds E regs st) : Holds E' regs st :=
  fun e member => holds e (sub e member)

theorem holds_iff {E : Equations} {loc : Loc} {reg : VReg} :
    holds E loc reg = true ↔ (loc.place, reg) ∈ E ∧ loc.place.at reg.class = some loc := by
  simp [holds, Loc.fits]

theorem Holds.read {E : Equations} {regs : RegisterFile} {st : Storage} {loc : Loc} {reg : VReg}
    (state : Holds E regs st) (held : holds E loc reg = true) :
    ∃ slot, Machine.readSlot regs reg = some slot ∧ slot.WrittenAt reg ∧ readLoc st loc = .ok slot := by
  obtain ⟨member, fits⟩ := holds_iff.mp held
  obtain ⟨loc', slot, fits', found, written, read⟩ := state _ member
  have same : loc' = loc := Option.some.inj (fits'.symm.trans fits)
  subst same
  exact ⟨slot, found, written, read⟩

theorem Holds.operand {E : Equations} {regs : RegisterFile} {st : Storage} {loc : Loc} {reg : VReg}
    (state : Holds E regs st) (held : holds E loc reg = true) :
    readLoc st loc = Machine.readOperand regs reg := by
  obtain ⟨slot, found, _, read⟩ := state.read held
  simp [read, Machine.readOperand, found]

theorem Holds.define {E : Equations} {regs : RegisterFile} {st : Storage} {loc : Loc} {reg : VReg}
    {slot : Slot} (state : Holds E regs st) (fits : loc.place.at reg.class = some loc)
    (written : slot.WrittenAt reg) :
    Holds (Alloc.define E loc reg) (Machine.writeSlot regs reg slot) (writeLoc st loc slot) := by
  intro e member
  simp only [Alloc.define, forget, kill, List.mem_cons, List.mem_filter] at member
  rcases member with same | ⟨⟨kept, otherPlace⟩, otherReg⟩
  · subst same
    exact ⟨loc, slot, fits, by simp [Machine.readSlot, Machine.writeSlot], written,
      readLoc_writeLoc_same st loc reg slot fits written⟩
  · obtain ⟨loc', slot', fits', found, written', read⟩ := state e kept
    have placeNe : e.1 ≠ loc.place := by simpa using otherPlace
    have idNe : e.2.id ≠ reg.id := by simpa using otherReg
    refine ⟨loc', slot', fits', ?_, written', ?_⟩
    · rw [Machine.readSlot_writeSlot_other regs reg e.2 slot idNe]; exact found
    · rw [readLoc_writeLoc_other st loc slot loc' (by rw [Place.at_place fits']; exact placeNe)]
      exact read

/-- The state after a copy of a general value from `src` to `dst`. -/
theorem Holds.copy {E : Equations} {regs : RegisterFile} {st : Storage} {dst src : Loc} {v : Value}
    (state : Holds E regs st) (source : readLoc st src = .ok (.value v)) :
    Holds (Alloc.copy E dst src) regs (writeLoc st dst (.value (normalize dst.class v))) := by
  intro e member
  simp only [Alloc.copy, kill, List.mem_append, List.mem_filterMap, List.mem_filter] at member
  rcases member with ⟨e', kept, moved⟩ | ⟨kept, otherPlace⟩
  · by_cases chosen : (e'.1 == src.place && src.fits e'.2.class && dst.fits e'.2.class) = true
    · simp only [chosen, ↓reduceIte, Option.some.injEq] at moved
      subst moved
      simp only [Bool.and_eq_true, beq_iff_eq, Loc.fits_iff] at chosen
      obtain ⟨⟨samePlace, srcFits⟩, dstFits⟩ := chosen
      obtain ⟨loc', slot, fits', found, written, read⟩ := state e' kept
      rw [samePlace] at fits'
      have same : loc' = src := Option.some.inj (fits'.symm.trans srcFits)
      subst same
      rw [source] at read
      cases read
      have cls := Place.at_class dstFits
      have normal : normalize dst.class v = v := by rw [cls]; exact written.2
      refine ⟨dst, .value v, dstFits, found, written, ?_⟩
      rw [normal]
      exact readLoc_writeLoc_same st dst e'.2 (.value v) dstFits written
    · simp [chosen] at moved
  · obtain ⟨loc', slot, fits', found, written, read⟩ := state e kept
    have placeNe : e.1 ≠ dst.place := by simpa using otherPlace
    refine ⟨loc', slot, fits', found, written, ?_⟩
    rw [readLoc_writeLoc_other st dst _ loc' (by rw [Place.at_place fits']; exact placeNe)]
    exact read

/-- A same-class move before allocation joins the places of its source. -/
theorem Holds.rename {E : Equations} {regs : RegisterFile} {st : Storage} {d s : VReg} {v : Value}
    (state : Holds E regs st) (sameClass : d.class = s.class)
    (source : Machine.readSlot regs s = some (.value v)) :
    Holds (Alloc.rename E d s) (Machine.writeSlot regs d (.value (normalize d.class v))) st := by
  intro e member
  simp only [Alloc.rename, forget, List.mem_append, List.mem_filterMap, List.mem_filter] at member
  rcases member with ⟨e', ⟨kept, otherReg⟩, moved⟩ | ⟨kept, otherReg⟩
  · by_cases chosen : e'.2 = s
    · simp only [chosen, beq_self_eq_true, ↓reduceIte, Option.some.injEq] at moved
      subst moved
      obtain ⟨loc', slot, fits', found, written, read⟩ := state e' kept
      rw [chosen] at fits' found written
      rw [source] at found
      cases found
      refine ⟨loc', .value v, by rw [sameClass]; exact fits', ?_, ?_, read⟩
      · have normal : normalize d.class v = v := by rw [sameClass]; exact written.2
        simp [Machine.readSlot, Machine.writeSlot, normal]
      · simpa [Machine.Slot.WrittenAt, sameClass] using written
    · simp [chosen] at moved
  · obtain ⟨loc', slot, fits', found, written, read⟩ := state e kept
    have idNe : e.2.id ≠ d.id := by simpa using otherReg
    refine ⟨loc', slot, fits', ?_, written, read⟩
    rw [Machine.readSlot_writeSlot_other regs d e.2 _ idNe]
    exact found

/-! ## The generic instruction lemma -/

/-- Rename the definition positions only. -/
def mapDefs (f : VReg → VReg) : Machine.Instruction → Machine.Instruction
  | .mov dst src => .mov (f dst) src
  | .movImm dst imm => .movImm (f dst) imm
  | .binary op dst lhs rhs width => .binary op (f dst) lhs rhs width
  | .binaryImm op dst lhs imm width => .binaryImm op (f dst) lhs imm width
  | .flagBinary op dst flags lhs rhs width => .flagBinary op (f dst) (f flags) lhs rhs width
  | .wideMultiply op dst lhs rhs => .wideMultiply op (f dst) lhs rhs
  | .msub dst n m a width => .msub (f dst) n m a width
  | .madd dst n m a width => .madd (f dst) n m a width
  | .extend op dst src => .extend op (f dst) src
  | .bfx signed dst src lsb bits width => .bfx signed (f dst) src lsb bits width
  | .cmp dst lhs rhs width => .cmp (f dst) lhs rhs width
  | .cmpImm dst lhs imm width => .cmpImm (f dst) lhs imm width
  | .tstImm dst lhs imm width => .tstImm (f dst) lhs imm width
  | .cset dst flags cond => .cset (f dst) flags cond
  | .csel dst flags lhs rhs cond width => .csel (f dst) flags lhs rhs cond width
  | .load align dst base offset kind volatile => .load align (f dst) base offset kind volatile
  | .store align src base offset kind volatile => .store align src base offset kind volatile
  | .frameSlot dst origin size align => .frameSlot (f dst) origin size align
  | .stackRelease address => .stackRelease address
  | .bl symbol args results => .bl symbol args (results.map f)

def renameWrites (f : VReg → VReg) (writes : Machine.Writes) : Machine.Writes :=
  writes.map (fun e => (f e.1, e.2))

private theorem map_bind {ε α β γ : Type} (x : Except ε α) (k : α → Except ε β) (g : β → γ) :
    Except.map g (x >>= k) = x >>= fun a => Except.map g (k a) := by
  cases x <;> rfl

private theorem map_pure {ε β γ : Type} (b : β) (g : β → γ) :
    Except.map g (pure b : Except ε β) = pure (g b) := rfl

private theorem map_throw {ε β γ : Type} (e : ε) (g : β → γ) :
    Except.map g (throw e : Except ε β) = throw e := rfl

private theorem map_ite {ε β γ : Type} (c : Prop) [Decidable c] (x y : Except ε β) (g : β → γ) :
    Except.map g (if c then x else y) = if c then Except.map g x else Except.map g y := by
  split <;> rfl

/-- The generic lemma: a renaming of the definition positions that keeps the
register classes gives the same operation, with the writes renamed. -/
theorem eval_mapDefs (owner : Nat) (inst : Machine.Instruction) (f : VReg → VReg)
    (get : Machine.OperandReader) (memory : Machine.Memory)
    (classes : ∀ reg ∈ inst.defs, (f reg).class = reg.class) :
    Machine.evalInstructionOperands owner (mapDefs f inst) get memory =
      (Machine.evalInstructionOperands owner inst get memory).map
        (fun r => (renameWrites f r.1, r.2)) := by
  cases inst <;> simp only [Machine.Instruction.defs, List.mem_cons,
    List.not_mem_nil, forall_eq_or_imp, forall_eq, or_false] at classes
  all_goals simp only [mapDefs, Machine.evalInstructionOperands, map_bind, map_pure, map_throw,
    map_ite, Machine.valueWrite, renameWrites, List.map_cons, List.map_nil, List.map_append]
  all_goals first
    | rfl
    | (simp only [classes]; done)
    | simp only [classes.1, classes.2]


theorem toMachine_ofMachine (u d : VReg → Loc) (o : Machine.Instruction) :
    (ofMachine u d o).toMachine Loc.reg =
      mapDefs (fun r => (d r).reg) (o.mapUses (fun r => (u r).reg)) := by
  cases o <;> simp [ofMachine, Instruction.toMachine, mapDefs, Machine.Instruction.mapUses,
    List.map_map, Function.comp_def]

theorem operandReader_reg (st : Storage) (loc : Loc) :
    operandReader st loc.reg = readLoc st loc := by
  simp [operandReader, Loc.reg, Loc.decode_code]

theorem applyWrites_cons (st : Storage) (loc : Loc) (slot : Slot) (rest : Machine.Writes) :
    applyWrites st ((loc.reg, slot) :: rest) = applyWrites (writeLoc st loc slot) rest := by
  simp [applyWrites, Loc.reg, Loc.decode_code]

/-- The semantics of an allocated original form, in terms of the evaluation
of the instruction before allocation. -/
theorem original_eval (owner : Nat) {E : Equations} {regs : RegisterFile} {st : Storage}
    (memory : Machine.Memory) (o : Machine.Instruction) (a : Instruction) (U D : VReg → Loc)
    (shape : a.toMachine Loc.reg = mapDefs (fun r => (D r).reg) (o.mapUses (fun r => (U r).reg)))
    (uses : ∀ r ∈ o.uses, holds E (U r) r = true)
    (defs : ∀ r ∈ o.defs, (D r).place.at r.class = some (D r))
    (state : Holds E regs st) :
    evalInstruction owner a st memory =
      (Machine.evalInstructionOperands owner o (Machine.readOperand regs) memory).map
        (fun r => (applyWrites st (renameWrites (fun x => (D x).reg) r.1), r.2)) := by
  have classes : ∀ r ∈ (o.mapUses (fun r => (U r).reg)).defs, ((D r).reg).class = r.class := by
    intro r member
    rw [Machine.Instruction.defs_mapUses] at member
    exact Place.at_class (defs r member)
  have agree : ∀ r ∈ o.uses, operandReader st ((U r).reg) = Machine.readOperand regs r := by
    intro r member
    rw [operandReader_reg]
    exact state.operand (uses r member)
  unfold evalInstruction
  rw [shape, eval_mapDefs owner _ _ _ memory classes,
    Machine.evalInstructionOperands_mapUses owner o _ _ _ memory agree]
  cases Machine.evalInstructionOperands owner o (Machine.readOperand regs) memory <;> rfl

/-- After the writes of an original instruction, each definition is in its
location and every other equation still holds. -/
theorem Holds.writes (D : VReg → Loc) :
    ∀ (ws : Machine.Writes) (E : Equations) (regs : RegisterFile) (st : Storage),
      Holds E regs st →
      (∀ e ∈ ws, e.2.WrittenAt e.1 ∧ (D e.1).place.at e.1.class = some (D e.1)) →
      Holds (defineAll E (ws.map (fun e => (e.1, D e.1)))) (Machine.applyWrites regs ws)
        (applyWrites st (renameWrites (fun x => (D x).reg) ws))
  | [], _, _, _, state, _ => state
  | e :: rest, E, regs, st, state, fine => by
    have head := fine e (by simp)
    have next := Holds.define (loc := D e.1) state head.2 head.1
    have tail := Holds.writes D rest _ _ _ next (fun e' member => fine e' (by simp [member]))
    simpa only [List.map_cons, defineAll, renameWrites, applyWrites_cons, Machine.applyWrites,
      List.foldl_cons] using tail

theorem defClassesOK_output (o : Machine.Instruction) (accepted : defClassesOK o = true) :
    Machine.OutputClassesValid o := by
  cases o <;> simp_all [defClassesOK, Machine.OutputClassesValid, Machine.Instruction.defs]

end EvoIR.Alloc
