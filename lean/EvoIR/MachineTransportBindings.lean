import EvoIR.MachineTransportWrites
import EvoIR.MachineEntry

/-! Actual call-result and SSA-parameter binding expressed as the concrete
Writes used by result-token realization. Values are captured before writing,
and every destination is normalized exactly as the real Machine.bind. -/
namespace EvoIR.Machine.Transport

theorem eraseDups_length_le (ids : List Nat) : ids.eraseDups.length ≤ ids.length := by
  have bounded : ∀ size (values : List Nat), values.length = size → values.eraseDups.length ≤ size := by
    intro size
    induction size using Nat.strongRecOn with
    | ind size ih =>
      intro values lengthEq
      cases values with
      | nil => simp at lengthEq; subst size; simp
      | cons head rest =>
        let filtered := rest.filter (fun id => !id == head)
        have less : filtered.length < size := by
          have short := List.length_filter_le (fun id => !id == head) rest
          simp only [List.length_cons] at lengthEq
          dsimp [filtered]
          omega
        have tailBound := ih filtered.length less filtered rfl
        have filterBound := List.length_filter_le (fun id => !id == head) rest
        simp only [List.eraseDups_cons, List.length_cons]
        dsimp [filtered] at tailBound
        simp only [List.length_cons] at lengthEq
        omega
  exact bounded ids.length ids rfl

theorem nodup_of_eraseDups_length (ids : List Nat)
    (distinct : ids.eraseDups.length = ids.length) : ids.Nodup := by
  induction ids with
  | nil => exact .nil
  | cons head rest ih =>
    simp only [List.eraseDups_cons, List.length_cons, Nat.add_right_cancel_iff] at distinct
    have erasedBound := eraseDups_length_le (rest.filter (fun id => !id == head))
    have filteredBound := List.length_filter_le (fun id => !id == head) rest
    have sameLength : (rest.filter (fun id => !id == head)).length = rest.length := by omega
    have keepAll := List.length_filter_eq_length_iff.mp sameLength
    have sameList := List.filter_eq_self.mpr keepAll
    rw [sameList] at distinct
    apply List.nodup_cons.mpr
    exact ⟨by intro member; have impossible := keepAll head member; simp at impossible, ih distinct⟩

theorem lookup_mem_unique (mapping : UseMap) (distinct : (mapping.map Prod.fst).Nodup)
    (entry : Nat × VReg) (member : entry ∈ mapping) : mapping.lookup entry.1 = some entry.2 := by
  induction mapping with
  | nil => simp at member
  | cons head rest ih =>
    rcases head with ⟨id, representative⟩
    have keys := List.nodup_cons.mp distinct
    rcases List.mem_cons.mp member with first | later
    · subst entry
      simp [List.lookup_cons]
    · have different : entry.1 ≠ id := by
        intro same
        exact keys.1 (by rw [← same]; exact List.mem_map.mpr ⟨entry, later, rfl⟩)
      have boolean : (entry.1 == id) = false := beq_eq_false_iff_ne.mpr different
      simpa only [List.lookup_cons, boolean, Bool.false_eq_true, ↓reduceIte] using ih keys.2 later

def bindingWrites (bindings : List (VReg × Value)) : Writes :=
  bindings.map fun (dst, value) => (dst, .value (normalize dst.class value))

theorem bindValues_writes (regs : RegisterFile) (bindings : List (VReg × Value)) :
    bindValues regs bindings = applyWrites regs (bindingWrites bindings) := by
  induction bindings generalizing regs with
  | nil => rfl
  | cons entry rest ih =>
    rcases entry with ⟨dst, value⟩
    exact ih (write regs dst value)

theorem bindingWrites_normalized (bindings : List (VReg × Value)) :
    WritesNormalized (bindingWrites bindings) := by
  intro entry member _
  obtain ⟨pair, _, same⟩ := List.mem_map.mp member
  rcases pair with ⟨dst, value⟩
  cases same
  exact ⟨normalize dst.class value, rfl, normalize_idempotent dst.class value⟩

theorem bind_success_shape (regs : RegisterFile) (params : List VReg) (values : List Value)
    (after : RegisterFile) (accepted : bind regs params values = .ok after) :
    params.length = values.length ∧
      (params.map VReg.id).eraseDups.length = params.length ∧
      after = applyWrites regs (bindingWrites (params.zip values)) := by
  by_cases arity : params.length = values.length
  · by_cases distinct : (params.map VReg.id).eraseDups.length = params.length
    · have exactBind := bind_exact regs params values arity distinct
      have same := Except.ok.inj (accepted.symm.trans exactBind)
      exact ⟨arity, distinct, same.trans (bindValues_writes regs (params.zip values))⟩
    · have distinctValues : (params.map VReg.id).eraseDups.length ≠ values.length := by omega
      simp [bind, arity, distinctValues, Bind.bind, Except.bind, Functor.map, Except.map,
        MonadExcept.throw, throwThe, MonadExceptOf.throw] at accepted
  · simp [bind, arity, Bind.bind, Except.bind, Functor.map, Except.map,
      MonadExcept.throw, throwThe, MonadExceptOf.throw] at accepted

/-- A successful actual source return binding determines the whole target
binding, not only a demanded projection. The logical result vector is never
truncated at a physical calling-convention limit. -/
theorem bind_shared_writes (source target : RegisterFile) (params : List VReg)
    (values : List Value) (sourceAfter : RegisterFile)
    (sourceBound : bind source params values = .ok sourceAfter) :
    let writes := bindingWrites (params.zip values)
    writes.map Prod.fst = params ∧ WritesNormalized writes ∧
      sourceAfter = applyWrites source writes ∧
      bind target params values = .ok (applyWrites target writes) := by
  obtain ⟨arity, distinct, sourceShape⟩ := bind_success_shape source params values sourceAfter sourceBound
  refine ⟨?_, bindingWrites_normalized _, sourceShape, ?_⟩
  · simpa [bindingWrites, List.map_map, Function.comp_def] using
      (List.map_fst_zip (l₁ := params) (l₂ := values) (by omega))
  · rw [bind_exact target params values arity distinct, bindValues_writes]

theorem bind_normalized_reads (regs : RegisterFile) (params : List VReg) (values : List Value)
    (after : RegisterFile) (accepted : bind regs params values = .ok after) :
    ∀ reg ∈ params, ∃ value, readOperand after reg = .ok (.value value) ∧ normalize reg.class value = value := by
  obtain ⟨arity, distinct, bank⟩ := bind_success_shape regs params values after accepted
  have firsts : (params.zip values).map Prod.fst = params := List.map_fst_zip (by omega)
  have ids : (params.zip values).map (fun p => p.1.id) = params.map VReg.id := by
    simpa only [List.map_map, Function.comp_def] using congrArg (List.map VReg.id) firsts
  have zipDistinct : ((params.zip values).map (fun p => p.1.id)).Nodup := by
    rw [ids]
    exact nodup_of_eraseDups_length _ (by simpa only [List.length_map] using distinct)
  intro reg member
  have inFirsts : reg ∈ (params.zip values).map Prod.fst := by rw [firsts]; exact member
  obtain ⟨pair, inZip, pairFirst⟩ := List.mem_map.mp inFirsts
  rcases pair with ⟨dst, input⟩
  change dst = reg at pairFirst
  subst dst
  have located := bindValues_lookup_member regs (params.zip values) zipDistinct reg input inZip
  have afterBank : after = bindValues regs (params.zip values) := by
    rw [bank, bindValues_writes]
  refine ⟨normalize reg.class input, ?_, normalize_idempotent reg.class input⟩
  simp only [afterBank, readOperand, readSlot, located]

theorem bind_parameter_read (regs : RegisterFile) (params : List VReg) (values : List Value)
    (after : RegisterFile) (accepted : bind regs params values = .ok after)
    (reg : VReg) (value : Value) (member : (reg, value) ∈ params.zip values) :
    readOperand after reg = .ok (.value (normalize reg.class value)) := by
  obtain ⟨arity, distinct, bank⟩ := bind_success_shape regs params values after accepted
  have firsts : (params.zip values).map Prod.fst = params := List.map_fst_zip (by omega)
  have ids : (params.zip values).map (fun p => p.1.id) = params.map VReg.id := by
    simpa only [List.map_map, Function.comp_def] using congrArg (List.map VReg.id) firsts
  have zipDistinct : ((params.zip values).map (fun p => p.1.id)).Nodup := by
    rw [ids]
    exact nodup_of_eraseDups_length _ (by simpa only [List.length_map] using distinct)
  have located := bindValues_lookup_member regs (params.zip values) zipDistinct reg value member
  have afterBank : after = bindValues regs (params.zip values) := by rw [bank, bindValues_writes]
  simp only [afterBank, readOperand, readSlot, located]

theorem reads_zip_pairs (params args : List VReg) (regs : RegisterFile) (values : List Value)
    (readArgs : args.mapM (read regs) = .ok values) (param arg : VReg)
    (member : (param, arg) ∈ params.zip args) :
    ∃ value, (param, value) ∈ params.zip values ∧ read regs arg = .ok value := by
  induction params generalizing args values with
  | nil => simp at member
  | cons firstParam restParams ih =>
    cases args with
    | nil => simp at member
    | cons firstArg restArgs =>
      cases firstRead : read regs firstArg with
      | error fault => simp [List.mapM_cons, firstRead, Bind.bind, Except.bind] at readArgs
      | ok firstValue =>
        cases restRead : restArgs.mapM (read regs) with
        | error fault => simp [List.mapM_cons, firstRead, restRead, Bind.bind, Except.bind] at readArgs
        | ok restValues =>
          have shape : firstValue :: restValues = values := by
            simpa [List.mapM_cons, firstRead, restRead, Bind.bind, Except.bind, Pure.pure, Except.pure] using readArgs
          subst values
          rcases List.mem_cons.mp member with same | tail
          · cases same
            exact ⟨firstValue, by simp, firstRead⟩
          · obtain ⟨value, binding, readValue⟩ := ih restArgs restValues restRead tail
            exact ⟨value, List.mem_cons_of_mem _ binding, readValue⟩

theorem bound_results (block pc : Nat) (params : List VReg) (values : List Value)
    (source target : RegisterFile) (sourceAfter : RegisterFile)
    (sourceBound : bind source params values = .ok sourceAfter)
    (sourceEnv targetEnv : Origins) (valuation : Origin → Slot)
    (sourceReal : Realizes sourceEnv valuation source)
    (targetReal : Realizes targetEnv valuation target)
    (sourceFresh : FutureFresh block pc 0 sourceEnv)
    (targetFresh : FutureFresh block pc 0 targetEnv)
    (normalized : OriginsNormalized valuation) :
    ∃ targetAfter nextValuation,
      bind target params values = .ok targetAfter ∧ OriginsNormalized nextValuation ∧
      Realizes (resultOrigins block pc 0 params sourceEnv) nextValuation sourceAfter ∧
      Realizes (resultOrigins block pc 0 params targetEnv) nextValuation targetAfter := by
  let writes := bindingWrites (params.zip values)
  obtain ⟨footprint, written, sourceShape, targetBound⟩ := bind_shared_writes source target params values
    sourceAfter sourceBound
  have actualFootprint : writes.map Prod.fst = params := footprint
  refine ⟨applyWrites target writes, writeResultValues block pc 0 writes valuation, targetBound,
    writeResultValues_normalized block pc 0 writes valuation normalized written, ?_, ?_⟩
  · rw [sourceShape]
    simpa only [actualFootprint] using
      realizes_results block pc 0 writes sourceEnv valuation source sourceReal sourceFresh
  · simpa only [actualFootprint] using
      realizes_results block pc 0 writes targetEnv valuation target targetReal targetFresh

end EvoIR.Machine.Transport
