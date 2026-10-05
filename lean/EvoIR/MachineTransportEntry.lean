import EvoIR.MachineTransportBindings
import EvoIR.MachineTransportReads

/-! Rebase block-local origin tokens from actual incoming banks. Unknown
registers receive no asserted environment entry; a default interpretation is
used only for tokens not present in the checked environment. -/
namespace EvoIR.Machine.Transport

def inputSlot (regs : RegisterFile) (reg : VReg) : Slot :=
  match readSlot regs reg with
  | some (.value value) => .value (normalize reg.class value)
  | _ => .value (.int 0)

def inputValuation (source target : RegisterFile) : Origin → Slot
  | .sourceInput reg => inputSlot source reg
  | .targetInput reg => inputSlot target reg
  | .result _ _ _ _ => .value (.int 0)

theorem inputSlot_normalized (regs : RegisterFile) (reg : VReg) :
    ∃ value, inputSlot regs reg = .value value ∧ normalize reg.class value = value := by
  unfold inputSlot
  cases found : readSlot regs reg with
  | none => refine ⟨.int 0, rfl, ?_⟩; cases reg.class <;> rfl
  | some slot =>
    cases slot with
    | flags => refine ⟨.int 0, rfl, ?_⟩; cases reg.class <;> rfl
    | value value => exact ⟨normalize reg.class value, rfl, normalize_idempotent reg.class value⟩

theorem inputValuation_normalized (source target : RegisterFile) :
    OriginsNormalized (inputValuation source target) := by
  intro value _
  cases value with
  | sourceInput reg => exact inputSlot_normalized source reg
  | targetInput reg => exact inputSlot_normalized target reg
  | result block pc output regClass => refine ⟨.int 0, rfl, ?_⟩; cases regClass <;> rfl

theorem operand_sameId (regs : RegisterFile) (left right : VReg) (same : left.id = right.id) :
    readOperand regs left = readOperand regs right := by
  simp only [readOperand, readSlot, same]

theorem inputSlot_read (regs : RegisterFile) (reg : VReg) (value : Value)
    (readable : readOperand regs reg = .ok (.value value))
    (normalized : normalize reg.class value = value) : inputSlot regs reg = .value value := by
  cases found : readSlot regs reg with
  | none => simp [readOperand, found] at readable
  | some slot =>
    have shape : slot = .value value := by simpa [readOperand, found] using readable
    simp [inputSlot, found, shape, normalized]

def EntryReady (args : List VReg) (mapping : UseMap) (source target : RegisterFile) : Prop :=
  (∀ reg ∈ args, ∃ value,
    readOperand source reg = .ok (.value value) ∧ normalize reg.class value = value ∧
    ∀ representative, mapping.lookup reg.id = some representative →
      readOperand target representative = .ok (.value value) ∧
        normalize representative.class value = value) ∧
  (∀ entry ∈ mapping, ∃ value,
    readOperand target entry.2 = .ok (.value value) ∧ normalize entry.2.class value = value)

theorem entryReady_realizes (args : List VReg) (mapping : UseMap) (source target : RegisterFile)
    (ready : EntryReady args mapping source target) :
    Realizes (sourceEntryOrigins args mapping) (inputValuation source target) source ∧
      Realizes (targetEntryOrigins mapping) (inputValuation source target) target := by
  constructor
  · intro reg value found
    obtain ⟨param, member, pair⟩ := List.mem_map.mp (origin_mem _ reg value found)
    have idEq : reg.id = param.id := by
      simpa [sourceEntryOrigins] using (congrArg Prod.fst pair).symm
    obtain ⟨concrete, readable, normalized, representatives⟩ := ready.1 param member
    cases mapped : mapping.lookup param.id with
    | none =>
      have originEq : value = .sourceInput param := by
        simpa [sourceEntryOrigins, mapped] using (congrArg Prod.snd pair).symm
      rw [operand_sameId source reg param idEq, readable, originEq]
      simp only [inputValuation, inputSlot_read source param concrete readable normalized]
    | some representative =>
      have originEq : value = .targetInput representative := by
        simpa [sourceEntryOrigins, mapped] using (congrArg Prod.snd pair).symm
      obtain ⟨targetReadable, targetNormalized⟩ := representatives representative mapped
      rw [operand_sameId source reg param idEq, readable, originEq]
      simp only [inputValuation, inputSlot_read target representative concrete targetReadable targetNormalized]
  · intro reg value found
    obtain ⟨entry, member, pair⟩ := List.mem_map.mp (origin_mem _ reg value found)
    have idEq : reg.id = entry.2.id := by
      simpa [targetEntryOrigins] using (congrArg Prod.fst pair).symm
    have originEq : value = .targetInput entry.2 := by
      simpa [targetEntryOrigins] using (congrArg Prod.snd pair).symm
    obtain ⟨concrete, readable, normalized⟩ := ready.2 entry member
    rw [operand_sameId target reg entry.2 idEq, readable, originEq]
    simp only [inputValuation, inputSlot_read target entry.2 concrete readable normalized]

theorem bindingsValid_member (block : Block) (plan : BlockPlan) (needed : List Nat)
    (accepted : bindingsValid block plan needed = true) (entry : Nat × VReg)
    (member : entry ∈ plan.bindings) :
    ∃ param ∈ block.args, param.id = entry.1 ∧ param.class = entry.2.class := by
  simp only [bindingsValid, Bool.and_eq_true, List.all_eq_true] at accepted
  have row : decide (entry.1 ∈ needed) && block.args.any (fun reg =>
      decide (reg.id = entry.1 ∧ reg.class = entry.2.class)) = true := by grind
  simp only [Bool.and_eq_true, List.any_eq_true, decide_eq_true_eq] at row
  exact row.2

theorem bindingsValid_needed (block : Block) (plan : BlockPlan) (needed : List Nat)
    (accepted : bindingsValid block plan needed = true) (entry : Nat × VReg)
    (member : entry ∈ plan.bindings) : entry.1 ∈ needed := by
  simp only [bindingsValid, Bool.and_eq_true, List.all_eq_true] at accepted
  have row : decide (entry.1 ∈ needed) && block.args.any (fun reg =>
      decide (reg.id = entry.1 ∧ reg.class = entry.2.class)) = true := by grind
  simp only [Bool.and_eq_true, decide_eq_true_eq] at row
  exact row.1

theorem bindingsValid_unique (block : Block) (plan : BlockPlan) (needed : List Nat)
    (accepted : bindingsValid block plan needed = true) : (plan.bindings.map Prod.fst).Nodup := by
  apply nodup_of_eraseDups_length
  simp only [bindingsValid, Bool.and_eq_true, unique, decide_eq_true_eq] at accepted
  grind

theorem entryReady_same_bank (block : Block) (plan : BlockPlan) (needed : List Nat)
    (accepted : bindingsValid block plan needed = true)
    (identity : ∀ entry ∈ plan.bindings, entry.1 = entry.2.id)
    (bank : RegisterFile)
    (readable : ∀ reg ∈ block.args, ∃ value,
      readOperand bank reg = .ok (.value value) ∧ normalize reg.class value = value) :
    EntryReady block.args plan.bindings bank bank := by
  have representatives : ∀ entry ∈ plan.bindings, ∃ value,
      readOperand bank entry.2 = .ok (.value value) ∧ normalize entry.2.class value = value := by
    intro entry member
    obtain ⟨param, paramMember, sameId, sameClass⟩ := bindingsValid_member block plan needed accepted entry member
    obtain ⟨value, readParam, normParam⟩ := readable param paramMember
    refine ⟨value, ?_, ?_⟩
    · rw [operand_sameId bank entry.2 param ((identity entry member).symm.trans sameId.symm)]
      exact readParam
    · rw [← sameClass]; exact normParam
  refine ⟨?_, representatives⟩
  intro reg member
  obtain ⟨value, readReg, normReg⟩ := readable reg member
  refine ⟨value, readReg, normReg, ?_⟩
  intro representative mapped
  obtain ⟨before, after, shape, _⟩ := List.lookup_eq_some_iff.mp mapped
  have inMapping : (reg.id, representative) ∈ plan.bindings := by rw [shape]; simp
  obtain ⟨sameValue, readRepresentative, normRepresentative⟩ := representatives _ inMapping
  have sameRead := operand_sameId bank reg representative (identity _ inMapping)
  have equal : value = sameValue := by
    rw [readReg, readRepresentative] at sameRead
    exact Slot.value.inj (Except.ok.inj sameRead)
  simpa only [equal] using And.intro readRepresentative normRepresentative

end EvoIR.Machine.Transport
