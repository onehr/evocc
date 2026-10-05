import EvoIR.MachineTransportResults

/-! Actual input-reader agreement for checked origin correspondences. This
includes the complete logical call vector and ordered return values. -/
namespace EvoIR.Machine.Transport

theorem read_eq_of_operand_eq (source target : RegisterFile) (left right : VReg)
    (same : readOperand source left = readOperand target right) :
    read source left = read target right := by
  cases sl : readSlot source left <;> cases tr : readSlot target right <;>
    simp only [readOperand, sl, tr, Except.error.injEq, Except.ok.injEq] at same
  all_goals try contradiction
  · simp [read, sl, tr]
  · subst_vars
    cases ‹Slot› <;> simp [read, sl, tr]

theorem readFlags_eq_of_operand_eq (source target : RegisterFile) (left right : VReg)
    (same : readOperand source left = readOperand target right) :
    readFlags source left = readFlags target right := by
  cases sl : readSlot source left <;> cases tr : readSlot target right <;>
    simp only [readOperand, sl, tr, Except.error.injEq, Except.ok.injEq] at same
  all_goals try contradiction
  · simp [readFlags, sl, tr]
  · subst_vars
    simp [readFlags, sl, tr]

theorem readBits_eq_of_operand_eq (source target : RegisterFile) (left right : VReg)
    (same : readOperand source left = readOperand target right) :
    readBits source left = readBits target right := by
  simp only [readBits, read_eq_of_operand_eq source target left right same]

theorem reads_rename (source target : RegisterFile) (regs : List VReg) (f : VReg → VReg)
    (same : ∀ reg ∈ regs, readOperand source reg = readOperand target (f reg)) :
    regs.mapM (read source) = (regs.map f).mapM (read target) := by
  induction regs with
  | nil => rfl
  | cons reg rest ih =>
    simp only [List.map_cons, List.mapM_cons,
      read_eq_of_operand_eq source target reg (f reg) (same reg (by simp)),
      ih (fun reg member => same reg (List.mem_cons_of_mem _ member))]

def slotValue : Slot → Except Core.Source.Fault Value
  | .value value => .ok value
  | .flags _ => .error (.malformed "MIR flags read as value")

theorem read_of_operand_ok (regs : RegisterFile) (reg : VReg) (slot : Slot)
    (readable : readOperand regs reg = .ok slot) : read regs reg = slotValue slot := by
  cases found : readSlot regs reg with
  | none => simp [readOperand, found] at readable
  | some value =>
    simp only [readOperand, found, Except.ok.injEq] at readable
    subst slot
    cases value <;> simp [read, found, slotValue]

theorem operandOrigins_values (env : Origins) (regs : List VReg) (values : List Origin)
    (found : operandOrigins env regs = some values) (valuation : Origin → Slot)
    (bank : RegisterFile) (related : Realizes env valuation bank) :
    regs.mapM (read bank) = values.mapM (fun value => slotValue (valuation value)) := by
  induction regs generalizing values with
  | nil => simp [operandOrigins] at found; cases found; rfl
  | cons reg rest ih =>
    rw [operandOrigins, List.mapM_cons] at found
    change (origin env reg >>= fun value => operandOrigins env rest >>= fun remaining =>
      pure (value :: remaining)) = some values at found
    cases head : origin env reg with
    | none => simp [head] at found
    | some value =>
      cases tail : operandOrigins env rest with
      | none => simp [head, tail] at found
      | some remaining =>
        have shape : value :: remaining = values := by
          simpa [head, tail] using found
        subst values
        simp only [List.mapM_cons,
          read_of_operand_ok bank reg (valuation value) (related reg value head),
          ih remaining tail]

theorem operandOrigins_reads_equal (sourceEnv targetEnv : Origins)
    (source target : List VReg) (values : List Origin)
    (sourceFound : operandOrigins sourceEnv source = some values)
    (targetFound : operandOrigins targetEnv target = some values)
    (valuation : Origin → Slot) (sourceRegs targetRegs : RegisterFile)
    (sourceReal : Realizes sourceEnv valuation sourceRegs)
    (targetReal : Realizes targetEnv valuation targetRegs) :
    source.mapM (read sourceRegs) = target.mapM (read targetRegs) := by
  rw [operandOrigins_values sourceEnv source values sourceFound valuation sourceRegs sourceReal,
    operandOrigins_values targetEnv target values targetFound valuation targetRegs targetReal]

theorem matchOrigins_bl_args (symbol : String) (args results : List VReg)
    (candidate : Instruction) (sourceEnv targetEnv : Origins)
    (accepted : matchOrigins (.bl symbol args results) candidate sourceEnv targetEnv = true)
    (valuation : Origin → Slot) (sourceRegs targetRegs : RegisterFile)
    (sourceReal : Realizes sourceEnv valuation sourceRegs)
    (targetReal : Realizes targetEnv valuation targetRegs) :
    candidate = .bl symbol (args.map (rename (useCorrespondence (.bl symbol args results) candidate))) results ∧
      args.mapM (read sourceRegs) =
        (args.map (rename (useCorrespondence (.bl symbol args results) candidate))).mapM (read targetRegs) := by
  have shape := checkRetained_shape _ _ _ (matchOrigins_retained _ _ _ _ accepted)
  refine ⟨shape.symm, reads_rename sourceRegs targetRegs args _ ?_⟩
  intro reg member
  exact (matchOrigins_reads _ _ _ _ accepted valuation sourceRegs targetRegs sourceReal targetReal reg
    (by simpa [Instruction.uses] using member)).symm

end EvoIR.Machine.Transport
