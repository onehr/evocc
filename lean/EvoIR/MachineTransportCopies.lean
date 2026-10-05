import EvoIR.MachineTransport

/-! Concrete execution of the actual checked copy spans. Known origins ensure
every move source is readable; class checks and conforming slots ensure that a
32-bit move does not silently change a claimed symbolic origin. -/
namespace EvoIR.Machine.Transport

def evalCopySpan (owner : Nat) : List Instruction → RegisterFile → Memory →
    Except Core.Source.Fault (RegisterFile × Memory)
  | [], regs, memory => .ok (regs, memory)
  | inst :: rest, regs, memory => do
    let (next, memory) ← evalInstruction owner inst regs memory
    evalCopySpan owner rest next memory

theorem origin_class (env : Origins) (reg : VReg) (value : Origin)
    (known : origin env reg = some value) : value.class = reg.class := by
  unfold origin at known
  cases lookup : env.lookup reg.id with
  | none => simp [lookup] at known
  | some found =>
    by_cases same : found.class = reg.class
    · simp [lookup, same] at known
      subst value
      exact same
    · simp [lookup, same] at known

theorem readOperand_writeSlot_other (regs : RegisterFile) (dst src : VReg) (value : Slot)
    (different : src.id ≠ dst.id) :
    readOperand (writeSlot regs dst value) src = readOperand regs src := by
  simp [readOperand, different]

theorem realizes_write (env : Origins) (valuation : Origin → Slot) (regs : RegisterFile)
    (related : Realizes env valuation regs) (dst : VReg) (value : Origin)
    (sameClass : value.class = dst.class) :
    Realizes (originWrite env dst value) valuation (writeSlot regs dst (valuation value)) := by
  intro reg found known
  by_cases aliases : reg.id = dst.id
  · have same : found = value := by
      unfold origin originWrite at known
      simp [List.lookup_cons, aliases] at known
      exact known.2.symm
    subst found
    simp [readOperand, readSlot, writeSlot, List.lookup_cons, aliases]
  · have old : origin env reg = some found := by
      have different : (reg.id == dst.id) = false := beq_eq_false_iff_ne.mpr aliases
      simpa [origin, originWrite, List.lookup_cons, different] using known
    rw [readOperand_writeSlot_other _ _ _ _ aliases]
    exact related reg found old

theorem conforms_value (regClass : RegClass) (slot : Slot)
    (general : regClass ≠ .flags) (typed : slot.Conforms regClass) :
    ∃ value, slot = .value value ∧ normalize regClass value = value := by
  cases slot with
  | flags flags => cases regClass <;> simp [Slot.Conforms] at typed general
  | value value =>
    refine ⟨value, rfl, normalize_eq_of_conforms regClass value ?_⟩
    cases regClass <;> simp_all [Slot.Conforms]

theorem mov_eval (owner : Nat) (dst src : VReg) (value : Value)
    (regs : RegisterFile) (memory : Memory)
    (readable : readOperand regs src = .ok (.value value))
    (normalized : normalize dst.class value = value) :
    evalInstruction owner (.mov dst src) regs memory =
      .ok (writeSlot regs dst (.value value), memory) := by
  simp only [evalInstruction, evalInstructionOperands, operandValue, readable]
  change (Except.ok (writeSlot regs dst (.value (normalize dst.class value)), memory)) = _
  rw [normalized]

def OriginsNormalized (valuation : Origin → Slot) : Prop :=
  ∀ value, value.class ≠ .flags → ∃ concrete,
    valuation value = .value concrete ∧ normalize value.class concrete = concrete

/-- Acceptance ensures all actual move sources are initialized and preserves
the same concrete interpretation of every symbolic register origin. The list
recursion gives finite administrative progress; no producer ordering is used. -/
theorem copyOrigins_sound_normalized (insts : List Instruction) (env after : Origins)
    (accepted : copyOrigins insts env = some after)
    (valuation : Origin → Slot) (owner : Nat) (regs : RegisterFile) (memory : Memory)
    (related : Realizes env valuation regs)
    (normalized : OriginsNormalized valuation) :
    ∃ next, evalCopySpan owner insts regs memory = .ok (next, memory) ∧
      Realizes after valuation next := by
  induction insts generalizing env after regs with
  | nil =>
    simp only [copyOrigins] at accepted
    cases accepted
    exact ⟨regs, rfl, related⟩
  | cons inst rest ih =>
    cases inst <;> try simp [copyOrigins] at accepted
    case mov dst src =>
      by_cases safe : dst.class = src.class ∧ dst.class ≠ .flags
      · simp only [copyOrigins, safe, ↓reduceIte] at accepted
        cases known : origin env src with
        | none => simp [known] at accepted
        | some value =>
          have sameClass : value.class = src.class := origin_class env src value known
          have general : value.class ≠ .flags := by rw [sameClass, ← safe.1]; exact safe.2
          obtain ⟨concrete, concreteSlot, normalizedValue⟩ := normalized value general
          have readable : readOperand regs src = .ok (.value concrete) := by
            rw [related src value known, concreteSlot]
          have normalizedDst : normalize dst.class concrete = concrete := by
            rw [safe.1, ← sameClass]
            exact normalizedValue
          have nextReal : Realizes (originWrite env dst value) valuation
              (writeSlot regs dst (.value concrete)) := by
            rw [← concreteSlot]
            exact realizes_write env valuation regs related dst value (sameClass.trans safe.1.symm)
          have tail : copyOrigins rest (originWrite env dst value) = some after := by
            have sourceGeneral : src.class ≠ .flags := by rw [← safe.1]; exact safe.2
            simpa [known, sourceGeneral] using accepted
          obtain ⟨next, evaluated, finalReal⟩ := ih _ _ tail _ nextReal
          refine ⟨next, ?_, finalReal⟩
          simp only [evalCopySpan, mov_eval owner dst src concrete regs memory readable normalizedDst]
          exact evaluated
      · simp [copyOrigins, safe] at accepted

/-- The stronger class-conformance invariant is one way to discharge the
exact normalization premise, but pointers do not need to be excluded merely
because they occur in a 32-bit view: normalize leaves pointers unchanged. -/
theorem copyOrigins_sound (insts : List Instruction) (env after : Origins)
    (accepted : copyOrigins insts env = some after)
    (valuation : Origin → Slot) (owner : Nat) (regs : RegisterFile) (memory : Memory)
    (related : Realizes env valuation regs)
    (typed : ∀ value, (valuation value).Conforms value.class) :
    ∃ next, evalCopySpan owner insts regs memory = .ok (next, memory) ∧
      Realizes after valuation next := by
  apply copyOrigins_sound_normalized insts env after accepted valuation owner regs memory related
  intro value general
  exact conforms_value value.class (valuation value) general (typed value)

end EvoIR.Machine.Transport
