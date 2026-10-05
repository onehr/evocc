import EvoIR.MachineTransportCopies

/-! Fresh result-token realization for actual instruction writes. A token's
value is obtained from the actual Writes list, never from an annotation. The
freshness condition is a finite symbolic-environment property checked/derived
while walking an actual block. -/
namespace EvoIR.Machine.Transport

def setOriginValue (valuation : Origin → Slot) (key : Origin) (slot : Slot) : Origin → Slot :=
  fun value => if value = key then slot else valuation value

def Fresh (key : Origin) (env : Origins) : Prop :=
  ∀ reg value, origin env reg = some value → value ≠ key

theorem origin_lookup (env : Origins) (reg : VReg) (value : Origin)
    (known : origin env reg = some value) : env.lookup reg.id = some value := by
  unfold origin at known
  cases located : env.lookup reg.id with
  | none => simp [located] at known
  | some found =>
    by_cases same : found.class = reg.class
    · simp [located, same] at known
      cases known
      rfl
    · simp [located, same] at known

theorem origin_mem (env : Origins) (reg : VReg) (value : Origin)
    (known : origin env reg = some value) : (reg.id, value) ∈ env := by
  obtain ⟨before, after, shape, _⟩ := List.lookup_eq_some_iff.mp (origin_lookup env reg value known)
  rw [shape]
  simp

theorem originWrite_cases (env : Origins) (dst reg : VReg) (written found : Origin)
    (known : origin (originWrite env dst written) reg = some found) :
    found = written ∨ origin env reg = some found := by
  by_cases aliases : reg.id = dst.id
  · left
    unfold origin originWrite at known
    simp [aliases] at known
    exact known.2.symm
  · right
    have different : (reg.id == dst.id) = false := beq_eq_false_iff_ne.mpr aliases
    simpa [origin, originWrite, List.lookup_cons, different] using known

def BeforeOrigin (block pc : Nat) : Origin → Prop
  | .result producer position _ _ => producer = block ∧ position < pc
  | _ => True

def KnownBefore (block pc : Nat) (env : Origins) : Prop :=
  ∀ reg value, origin env reg = some value → BeforeOrigin block pc value

theorem knownBefore_mono (block pc next : Nat) (env : Origins)
    (known : KnownBefore block pc env) (later : pc ≤ next) : KnownBefore block next env := by
  intro reg value located
  have previous := known reg value located
  cases value <;> simp_all [BeforeOrigin] <;> omega

theorem knownBefore_write (block pc : Nat) (env : Origins) (dst : VReg) (value : Origin)
    (known : KnownBefore block pc env) (written : BeforeOrigin block pc value) :
    KnownBefore block pc (originWrite env dst value) := by
  intro reg found located
  rcases originWrite_cases env dst reg value found located with same | old
  · rw [same]; exact written
  · exact known reg found old

theorem knownBefore_results (block pc index : Nat) (defs : List VReg) (env : Origins)
    (known : KnownBefore block pc env) :
    KnownBefore block (pc+1) (resultOrigins block pc index defs env) := by
  have updated : KnownBefore block (pc+1) env := knownBefore_mono block pc (pc+1) env known (by omega)
  have build : ∀ (outputs : List VReg) (position : Nat) (prior : Origins),
      KnownBefore block (pc+1) prior →
      KnownBefore block (pc+1) (resultOrigins block pc position outputs prior) := by
    intro outputs
    induction outputs with
    | nil => intros; assumption
    | cons dst rest ih =>
      intro position prior previous
      apply ih (position+1)
      exact knownBefore_write block (pc+1) prior dst (.result block pc position dst.class)
        previous ⟨rfl, by omega⟩
  exact build defs index env updated

theorem knownBefore_copies (block pc : Nat) (insts : List Instruction) (env after : Origins)
    (accepted : copyOrigins insts env = some after) (known : KnownBefore block pc env) :
    KnownBefore block pc after := by
  induction insts generalizing env after with
  | nil => simp only [copyOrigins] at accepted; cases accepted; exact known
  | cons inst rest ih =>
    cases inst <;> try simp [copyOrigins] at accepted
    case mov dst src =>
      by_cases safe : dst.class = src.class ∧ dst.class ≠ .flags
      · cases located : origin env src with
        | none => simp [copyOrigins, safe, located] at accepted
        | some value =>
          have tail : copyOrigins rest (originWrite env dst value) = some after := by
            have sourceGeneral : src.class ≠ .flags := by rw [← safe.1]; exact safe.2
            simpa [copyOrigins, safe, located, sourceGeneral] using accepted
          exact ih _ _ tail (knownBefore_write block pc env dst value known (known src value located))
      · simp [copyOrigins, safe] at accepted

theorem sourceEntryOrigins_before (block pc : Nat) (args : List VReg) (mapping : UseMap) :
    KnownBefore block pc (sourceEntryOrigins args mapping) := by
  intro reg value located
  have member := origin_mem _ reg value located
  obtain ⟨arg, _, pair⟩ := List.mem_map.mp member
  cases found : mapping.lookup arg.id with
  | none =>
    have same : value = .sourceInput arg := by
      simpa [sourceEntryOrigins, found] using (congrArg Prod.snd pair).symm
    simp [same, BeforeOrigin]
  | some representative =>
    have same : value = .targetInput representative := by
      simpa [sourceEntryOrigins, found] using (congrArg Prod.snd pair).symm
    simp [same, BeforeOrigin]

theorem targetEntryOrigins_before (block pc : Nat) (mapping : UseMap) :
    KnownBefore block pc (targetEntryOrigins mapping) := by
  intro reg value located
  have member := origin_mem _ reg value located
  obtain ⟨entry, _, pair⟩ := List.mem_map.mp member
  have same : value = .targetInput entry.2 := by
    simpa [targetEntryOrigins] using (congrArg Prod.snd pair).symm
  simp [same, BeforeOrigin]

theorem realizes_fresh_value (key : Origin) (env : Origins) (valuation : Origin → Slot)
    (regs : RegisterFile) (related : Realizes env valuation regs)
    (fresh : Fresh key env) (slot : Slot) :
    Realizes env (setOriginValue valuation key slot) regs := by
  intro reg value known
  rw [related reg value known]
  simp [setOriginValue, fresh reg value known]

theorem realizes_result_write (key : Origin) (env : Origins) (valuation : Origin → Slot)
    (regs : RegisterFile) (related : Realizes env valuation regs)
    (fresh : Fresh key env) (dst : VReg) (slot : Slot) (sameClass : key.class = dst.class) :
    Realizes (originWrite env dst key) (setOriginValue valuation key slot)
      (writeSlot regs dst slot) := by
  have old := realizes_fresh_value key env valuation regs related fresh slot
  have next := realizes_write env (setOriginValue valuation key slot) regs old dst key sameClass
  simpa [setOriginValue] using next

def FutureFresh (block pc index : Nat) (env : Origins) : Prop :=
  ∀ next regClass, index ≤ next → Fresh (.result block pc next regClass) env

theorem futureFresh_of_before (block pc index : Nat) (env : Origins)
    (known : KnownBefore block pc env) : FutureFresh block pc index env := by
  intro next regClass _ reg value located same
  have previous := known reg value located
  rw [same] at previous
  simp [BeforeOrigin] at previous

theorem futureFresh_write (block pc index : Nat) (env : Origins)
    (fresh : FutureFresh block pc index env) (dst : VReg) :
    FutureFresh block pc (index+1) (originWrite env dst (.result block pc index dst.class)) := by
  intro next regClass later reg value known
  by_cases aliases : reg.id = dst.id
  · have same : value = .result block pc index dst.class := by
      unfold origin originWrite at known
      simp [aliases] at known
      exact known.2.symm
    rw [same]
    intro equal
    cases equal
    omega
  · have old : origin env reg = some value := by
      have different : (reg.id == dst.id) = false := beq_eq_false_iff_ne.mpr aliases
      simpa [origin, originWrite, List.lookup_cons, different] using known
    exact fresh next regClass (by omega) reg value old

def writeResultValues (block pc : Nat) : Nat → Writes → (Origin → Slot) → (Origin → Slot)
  | _, [], valuation => valuation
  | index, (dst, slot) :: rest, valuation =>
    writeResultValues block pc (index+1) rest
      (setOriginValue valuation (.result block pc index dst.class) slot)

theorem realizes_resultValues_unchanged (block pc index : Nat) (writes : Writes)
    (env : Origins) (valuation : Origin → Slot) (regs : RegisterFile)
    (related : Realizes env valuation regs) (fresh : FutureFresh block pc index env) :
    Realizes env (writeResultValues block pc index writes valuation) regs := by
  induction writes generalizing index valuation with
  | nil => exact related
  | cons entry rest ih =>
    rcases entry with ⟨dst, slot⟩
    have nextReal := realizes_fresh_value (.result block pc index dst.class) env valuation regs
      related (fresh index dst.class (by omega)) slot
    exact ih (index+1) _ nextReal (fun next regClass later => fresh next regClass (by omega))

def WritesNormalized (writes : Writes) : Prop :=
  ∀ entry ∈ writes, entry.1.class ≠ .flags → ∃ value,
    entry.2 = .value value ∧ normalize entry.1.class value = value

theorem normalized_setOriginValue (valuation : Origin → Slot) (key : Origin) (slot : Slot)
    (normalized : OriginsNormalized valuation)
    (written : key.class ≠ .flags → ∃ value,
      slot = .value value ∧ normalize key.class value = value) :
    OriginsNormalized (setOriginValue valuation key slot) := by
  intro found general
  by_cases same : found = key
  · subst found
    simpa [setOriginValue] using written general
  · simpa [setOriginValue, same] using normalized found general

theorem writeResultValues_normalized (block pc index : Nat) (writes : Writes)
    (valuation : Origin → Slot) (normalized : OriginsNormalized valuation)
    (written : WritesNormalized writes) :
    OriginsNormalized (writeResultValues block pc index writes valuation) := by
  induction writes generalizing index valuation with
  | nil => exact normalized
  | cons entry rest ih =>
    rcases entry with ⟨dst, slot⟩
    have head := written (dst, slot) (by simp)
    have nextNormalized := normalized_setOriginValue valuation
      (.result block pc index dst.class) slot normalized (by simpa [Origin.class] using head)
    exact ih (index+1) _ nextNormalized (fun entry member => written entry (List.mem_cons_of_mem _ member))

/-- All output slots come from actual successful instruction evaluation.
Writes and definition order are related by the dispatch-proved
evalInstructionOperands_resultRegs lemma, not an assumed footprint table. -/
theorem realizes_results (block pc index : Nat) (writes : Writes)
    (env : Origins) (valuation : Origin → Slot) (regs : RegisterFile)
    (related : Realizes env valuation regs) (fresh : FutureFresh block pc index env) :
    Realizes (resultOrigins block pc index (writes.map Prod.fst) env)
      (writeResultValues block pc index writes valuation) (applyWrites regs writes) := by
  induction writes generalizing index env valuation regs with
  | nil => exact related
  | cons entry rest ih =>
    rcases entry with ⟨dst, slot⟩
    have nextReal := realizes_result_write (.result block pc index dst.class)
      env valuation regs related (fresh index dst.class (by omega)) dst slot rfl
    have nextFresh := futureFresh_write block pc index env fresh dst
    exact ih (index+1) _ _ _ nextReal nextFresh

theorem evalInstruction_success_writes (owner : Nat) (inst : Instruction)
    (regs : RegisterFile) (memory : Memory) (after : RegisterFile) (nextMemory : Memory)
    (evaluated : evalInstruction owner inst regs memory = .ok (after, nextMemory)) :
    ∃ writes, evalInstructionOperands owner inst (readOperand regs) memory = .ok (writes, nextMemory) ∧
      after = applyWrites regs writes := by
  cases outcome : evalInstructionOperands owner inst (readOperand regs) memory with
  | error fault =>
    simp [evalInstruction, outcome, Bind.bind, Except.bind] at evaluated
  | ok pair =>
    rcases pair with ⟨writes, memory'⟩
    simp only [evalInstruction, outcome, Bind.bind, Except.bind, Pure.pure, Except.pure,
      Except.ok.injEq, Prod.mk.injEq] at evaluated
    obtain ⟨regsEqual, memoryEqual⟩ := evaluated
    subst memory'
    exact ⟨writes, rfl, regsEqual.symm⟩

theorem deleted_results (block pc : Nat) (source : Instruction)
    (sourceEnv targetEnv : Origins) (valuation : Origin → Slot)
    (sourceRegs targetRegs : RegisterFile) (owner : Nat) (memory : Memory)
    (sourceReal : Realizes sourceEnv valuation sourceRegs)
    (targetReal : Realizes targetEnv valuation targetRegs)
    (sourceFresh : FutureFresh block pc 0 sourceEnv)
    (targetFresh : FutureFresh block pc 0 targetEnv)
    (writes : Writes)
    (evaluated : evalInstructionOperands owner source (readOperand sourceRegs) memory = .ok (writes, memory)) :
    Realizes (resultOrigins block pc 0 source.defs sourceEnv)
      (writeResultValues block pc 0 writes valuation) (applyWrites sourceRegs writes) ∧
      Realizes targetEnv (writeResultValues block pc 0 writes valuation) targetRegs := by
  have footprint := evalInstructionOperands_resultRegs owner source (readOperand sourceRegs)
    memory writes memory evaluated
  refine ⟨?_, realizes_resultValues_unchanged block pc 0 writes targetEnv valuation targetRegs
    targetReal targetFresh⟩
  rw [← footprint]
  exact realizes_results block pc 0 writes sourceEnv valuation sourceRegs sourceReal sourceFresh

/-- The two actual retained executions receive the same concrete Writes, so
their respective environments are realized by one extended valuation. -/
theorem matched_results (block pc : Nat) (source target : Instruction)
    (sourceEnv targetEnv : Origins) (valuation : Origin → Slot)
    (sourceRegs targetRegs : RegisterFile) (owner : Nat) (memory : Memory)
    (accepted : matchOrigins source target sourceEnv targetEnv = true)
    (sourceReal : Realizes sourceEnv valuation sourceRegs)
    (targetReal : Realizes targetEnv valuation targetRegs)
    (sourceFresh : FutureFresh block pc 0 sourceEnv)
    (targetFresh : FutureFresh block pc 0 targetEnv)
    (writes : Writes) (nextMemory : Memory)
    (evaluated : evalInstructionOperands owner source (readOperand sourceRegs) memory =
      .ok (writes, nextMemory)) :
    evalInstructionOperands owner target (readOperand targetRegs) memory = .ok (writes, nextMemory) ∧
      Realizes (resultOrigins block pc 0 source.defs sourceEnv)
        (writeResultValues block pc 0 writes valuation) (applyWrites sourceRegs writes) ∧
      Realizes (resultOrigins block pc 0 target.defs targetEnv)
        (writeResultValues block pc 0 writes valuation) (applyWrites targetRegs writes) := by
  have candidate := matchOrigins_eval source target sourceEnv targetEnv accepted valuation
    owner sourceRegs targetRegs memory sourceReal targetReal
  have footprint := evalInstructionOperands_resultRegs owner source (readOperand sourceRegs)
    memory writes nextMemory evaluated
  have retained := matchOrigins_retained source target sourceEnv targetEnv accepted
  have sameDefs := checkRetained_defs (useCorrespondence source target) source target retained
  refine ⟨candidate.trans evaluated, ?_, ?_⟩
  · rw [← footprint]
    exact realizes_results block pc 0 writes sourceEnv valuation sourceRegs sourceReal sourceFresh
  · rw [sameDefs, ← footprint]
    exact realizes_results block pc 0 writes targetEnv valuation targetRegs targetReal targetFresh

end EvoIR.Machine.Transport
