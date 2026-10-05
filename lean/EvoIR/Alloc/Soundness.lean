import EvoIR.Alloc.Transfer
import EvoIR.MachineCorrespondence

/-!
Soundness of the allocation checker, and its composition with the
source-to-machine theorem. The refinement notion is the one of
`MachineCorrespondence.lean`: `Core.Transition.Refines` with the same
observations (returned bits or the first checked diagnostic).
-/
namespace EvoIR.Alloc

open Core.Transition (Reaches Refines RankedSimulation)

/-- The allocated program refines the program before allocation. -/
def AllocationRefines (pre : Machine.Program) (post : Program) (type : Core.Source.IntType)
    (preEntry : Machine.State) (postEntry : State) : Prop :=
  Refines (Machine.step .postSSA pre) (step post) (Machine.observe type) (observe type) preEntry postEntry

/-- Typed Core source refined by allocated EvoMIR. -/
def SourceToAllocatedRefines (source : Core.Source.Program) (post : Program)
    (type : Core.Source.IntType) (sourceEntry : Core.Source.State) (postEntry : State) : Prop :=
  Refines (Core.Source.step source) (step post) Core.observeSource (observe type) sourceEntry postEntry

theorem observe_same {type : Core.Source.IntType} {s : Machine.State} {t : State}
    {r : Except Core.Source.Fault (List Machine.Value)}
    (haltA : s.control = .halted r) (haltB : t.control = .halted r) :
    observe type t = Machine.observe type s := by
  unfold observe Machine.observe
  rw [haltA, haltB]
  cases r with
  | error e => cases e <;> rfl
  | ok vs => rcases vs with _ | ⟨v, _ | ⟨w, ws⟩⟩ <;> (try cases v) <;> rfl

section
variable {pre : Machine.Program} {post : Program} {hints : Hints} {type : Core.Source.IntType}
  {entry : Machine.State}

theorem simulation (checked : checkAllocation pre post hints = true)
    (progress : Machine.ObservedProgress .postSSA pre type entry) :
    RankedSimulation (Machine.step .postSSA pre) (step post) (Machine.observe type) (observe type)
      (Related pre post hints entry) (rank post) where
  transfer _ _ _ rel stepped := transfer checked progress rel stepped
  terminal s t rel stop := by
    obtain ⟨reach, _, _, ⟨_, _, _, runB, _⟩ | ⟨r, haltA, haltB⟩⟩ := rel
    · simp [step, runB] at stop
    · rcases progress s reach with ⟨_, h⟩ | ⟨o, h⟩
      · simp [Machine.step, haltA] at h
      · exact ⟨s, o, .refl _, by simp [Machine.step, haltA], h, by rw [observe_same haltA haltB, h]⟩
  progress s t rel := by
    obtain ⟨reach, _, _, ⟨_, _, _, runB, _⟩ | ⟨r, haltA, haltB⟩⟩ := rel
    · exact .inl (by simp only [step, runB]; exact ⟨_, rfl⟩)
    · rcases progress s reach with ⟨_, h⟩ | ⟨o, h⟩
      · simp [Machine.step, haltA] at h
      · exact .inr ⟨o, by rw [observe_same haltA haltB, h]⟩

/-- The argument lists `preArgs` and `args` give the same entry registers to
the entry function `name` of `pre`. The two lists have the same length. For
each parameter of the entry block, the two arguments are equal after the
normalization to the class of the parameter. Thus a parameter with a 32-bit
register uses only the low 32 bits of its argument. -/
def EntryAgrees (pre : Machine.Program) (name : String) (preArgs args : List Machine.Value) :
    Prop :=
  args.length = preArgs.length ∧
  ∀ f block, pre.functions.find? (·.name == name) = some f →
    f.blocks.find? (·.id == 0) = some block →
    ∀ i (h : i < block.args.length) (h' : i < preArgs.length) (h'' : i < args.length),
      Machine.normalize block.args[i].class args[i] =
        Machine.normalize block.args[i].class preArgs[i]

theorem EntryAgrees.refl (pre : Machine.Program) (name : String) (args : List Machine.Value) :
    EntryAgrees pre name args args :=
  ⟨rfl, fun _ _ _ _ _ _ _ _ => rfl⟩

/-- The initial state before allocation with the arguments `preArgs` and the
initial allocated state with the arguments `args` are related when the two
lists agree at the entry. -/
theorem initial_related_args (checked : checkAllocation pre post hints = true) {name : String}
    {preArgs args : List Machine.Value} {memory : Machine.Memory} {preEntry : Machine.State}
    (started : Machine.initial pre name preArgs memory = .ok preEntry)
    (agrees : EntryAgrees pre name preArgs args) :
    ∃ postEntry, initial post name args memory = .ok postEntry ∧
      Related pre post hints preEntry preEntry postEntry := by
  obtain ⟨length, same⟩ := agrees
  unfold Machine.initial at started
  cases foundF : pre.functions.find? (·.name == name) with
  | none => simp [foundF, MonadExcept.throw, throwThe, MonadExceptOf.throw] at started
  | some f =>
    cases foundE : f.blocks.find? (·.id == 0) with
    | none => simp [foundF, Machine.enter, foundE, MonadExcept.throw, throwThe,
        MonadExceptOf.throw, bind, Except.bind] at started
    | some entryBlock =>
      cases bound : Machine.bind [] entryBlock.args preArgs with
      | error e => simp [foundF, Machine.enter, foundE, bound, bind, Except.bind] at started
      | ok regs =>
        obtain ⟨arity, unique, _⟩ := Machine.Transport.bind_success_shape [] _ _ regs bound
        rw [Machine.bind_exact [] _ _ arity unique] at bound
        cases bound
        simp only [foundF, Machine.enter, foundE, Machine.bind_exact [] _ _ arity unique, bind,
          Except.bind, pure, Except.pure, Except.ok.injEq] at started
        subst started
        obtain ⟨distinct, general, hintSub⟩ := entry_facts checked foundF foundE
        obtain ⟨g, entry', X, foundG, foundE', _, _⟩ := block_entry checked foundF foundE
        have name_f := find_name foundF
        have name_g := find_name' foundG
        have id0 : entryBlock.id = 0 := by simpa using List.find?_some foundE
        have id0' : entry'.id = 0 := by simpa using List.find?_some foundE'
        refine ⟨⟨.running ⟨g.name, entry'.id, 0, entryStorage args, 1⟩, [], 2, memory⟩,
          by simp only [initial, foundG, enter, foundE', bind, Except.bind, pure, Except.pure],
          .refl _, rfl, rfl, .inl ⟨_, _, rfl,
          rfl, ⟨hintOf hints name 0, ?_, ?_⟩, ?_⟩⟩
        · simpa [name_f, id0] using cursor_entry checked (a := ⟨f.name, entryBlock.id, 0,
            Machine.bindValues [] (entryBlock.args.zip preArgs), 1, preArgs⟩)
            (b := ⟨g.name, entry'.id, 0, entryStorage args, 1⟩) (by simpa [name_f] using foundF)
            (by simpa [id0] using foundE) (by simp [name_f, name_g]) (by simp [id0, id0']) rfl rfl rfl
        · apply Holds.mono (fun e member => by
            simpa [name_f] using List.all_eq_true.mp hintSub e member)
          apply entry_holds arity distinct (by simpa using general)
          intro i h h' loc fits
          have h'' : i < args.length := by omega
          rw [← same f entryBlock foundF foundE i h h' h'']
          have gen : entryBlock.args[i].class ≠ .flags := by
            have := List.all_eq_true.mp general _ (List.getElem_mem h); simpa using this
          have notFlags : loc ≠ .flags := fun e => gen ((Place.at_flags fits).mp e)
          have cell : entryStorage args loc.place = some ⟨.value args[i], .x⟩ := by
            rw [Place.at_place fits]
            unfold argPlace entryStorage
            by_cases small : i < abiRegisterCount
            · simp [small, List.getElem?_eq_getElem h'']
            · have e : abiRegisterCount + (i - abiRegisterCount) = i := by omega
              simp [small, e, List.getElem?_eq_getElem h'']
          have cls := Place.at_class fits
          rw [Loc.class_of_general notFlags] at cls
          clear same
          cases loc <;> simp_all [readLoc, Loc.width]
        · exact .nil _

theorem initial_related (checked : checkAllocation pre post hints = true) {name : String}
    {args : List Machine.Value} {memory : Machine.Memory} {preEntry : Machine.State}
    (started : Machine.initial pre name args memory = .ok preEntry) :
    ∃ postEntry, initial post name args memory = .ok postEntry ∧
      Related pre post hints preEntry preEntry postEntry :=
  initial_related_args checked started (.refl pre name args)

end

/-- **Soundness of the allocation checker.** If `checkAllocation` accepts the
program before allocation, the allocated program and the hints, then for every
entry function, argument vector and private memory with a successful start
before allocation, the allocated program starts and refines the program before
allocation: same terminal observations, divergence only with divergence, and
no stuck reachable state. The explicit hypothesis `progress` says that the
program before allocation itself never reaches a stuck or malformed state
from this entry. It follows from the earlier stages (see
`source_allocated_refines`). -/
theorem checkAllocation_sound (pre : Machine.Program) (post : Program) (hints : Hints)
    (accepted : checkAllocation pre post hints = true) (type : Core.Source.IntType)
    (name : String) (args : List Machine.Value) (memory : Machine.Memory) (preEntry : Machine.State)
    (started : Machine.initial pre name args memory = .ok preEntry)
    (progress : Machine.ObservedProgress .postSSA pre type preEntry) :
    ∃ postEntry, initial post name args memory = .ok postEntry ∧
      AllocationRefines pre post type preEntry postEntry := by
  obtain ⟨postEntry, init, related⟩ := initial_related accepted started
  exact ⟨postEntry, init, (simulation accepted progress).sound related⟩

/-- `checkAllocation_sound` with two argument lists. The allocated program
starts with `args`. The program before allocation starts with `preArgs`. The
two lists must agree at the entry (`EntryAgrees`). The other bits of `args`
are arbitrary. -/
theorem checkAllocation_sound_args (pre : Machine.Program) (post : Program) (hints : Hints)
    (accepted : checkAllocation pre post hints = true) (type : Core.Source.IntType)
    (name : String) (preArgs args : List Machine.Value) (memory : Machine.Memory)
    (preEntry : Machine.State) (started : Machine.initial pre name preArgs memory = .ok preEntry)
    (agrees : EntryAgrees pre name preArgs args)
    (progress : Machine.ObservedProgress .postSSA pre type preEntry) :
    ∃ postEntry, initial post name args memory = .ok postEntry ∧
      AllocationRefines pre post type preEntry postEntry := by
  obtain ⟨postEntry, init, related⟩ := initial_related_args accepted started agrees
  exact ⟨postEntry, init, (simulation accepted progress).sound related⟩

/-- Composition with `Machine.source_machine_refines`: typed Core source to
allocated EvoMIR. The progress hypothesis of the allocation theorem is the
progress field of the transport refinement; no new premise is added. -/
theorem source_allocated_refines {source : Core.Source.Program} {ir : Core.IR.Program}
    {ssa lowered : Machine.Program} {allocated : Program} {hints : Hints}
    {type : Core.Source.IntType} {sourceEntry : Core.Source.State} {irEntry : Core.IR.State}
    {ssaEntry loweredEntry : Machine.State} {name : String} {args : List Machine.Value}
    {memory : Machine.Memory}
    (front : Core.LoweringRefines source ir type sourceEntry irEntry)
    (selection : Machine.SelectionRefines ir ssa type irEntry ssaEntry)
    (transport : Machine.TransportRefines ssa lowered type ssaEntry loweredEntry)
    (started : Machine.initial lowered name args memory = .ok loweredEntry)
    (accepted : checkAllocation lowered allocated hints = true) :
    ∃ allocatedEntry, initial allocated name args memory = .ok allocatedEntry ∧
      SourceToAllocatedRefines source allocated type sourceEntry allocatedEntry := by
  obtain ⟨allocatedEntry, init, allocation⟩ := checkAllocation_sound lowered allocated hints
    accepted type name args memory loweredEntry started transport.progress
  exact ⟨allocatedEntry, init,
    Refines.trans (Machine.source_machine_refines front selection transport) allocation⟩

end EvoIR.Alloc
