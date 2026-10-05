import EvoIR.MachineExecution

/-! Constructor-level semantic-fidelity controls. These kernel-reduced finite
examples test the independent MIR machine; they do not certify a compiler pair
or replace a universal CFG/call/divergence simulation. -/
namespace EvoIR.Machine.ExecutionTests

private def w (id : Nat) : VReg := ⟨id, .gpr32⟩
private def x (id : Nat) : VReg := ⟨id, .gpr64⟩
private def f (id : Nat) : VReg := ⟨id, .flags⟩
private def empty : Memory := ⟨1, []⟩
private def ints (values : List (Nat × Nat)) : RegisterFile :=
  values.map fun (id, bits) => (id, Slot.value (.int (BitVec.ofNat 64 bits)))

private def output (inst : Instruction) (regs : RegisterFile) (dst : VReg) :
    Except Core.Source.Fault Value := do
  let (next, _) ← evalInstruction 1 inst regs empty
  read next dst

theorem movImm_w_clears_upper :
    output (.movImm (w 0) 0x10000002a) [] (x 0) = .ok (.int (42#64)) := by decide

theorem mov_w_clears_upper :
    output (.mov (w 0) (x 1)) (ints [(1, 0x10000002a)]) (x 0) = .ok (.int (42#64)) := by decide

theorem binary_spelling_controls_width :
    output (.binary .add (x 0) (x 1) (x 2) .w)
      (ints [(1, 0xffffffff), (2, 1)]) (x 0) = .ok (.int (0#64)) := by decide

theorem shift_masks_count :
    output (.binaryImm .lsl (x 0) (x 1) 33 .w) (ints [(1, 1)]) (x 0) =
      .ok (.int (2#64)) := by decide

theorem arithmetic_shift_sign_fills :
    evalBinary .asr .w (0x80000000#64) (33#64) = (0xc0000000#64) := by decide

theorem signed_division_overflow_is_total :
    evalBinary .sdiv .w (0x80000000#64) (0xffffffff#64) = (0x80000000#64) := by decide

theorem division_zero_is_total :
    evalBinary .sdiv .x (7#64) (0#64) = (0#64) ∧
    evalBinary .udiv .w (7#64) (0#64) = (0#64) := by decide

theorem adds_sets_signed_overflow :
    arithmeticFlags .adds .w (0x7fffffff#64) (1#64) =
      ((0x80000000#64), .arithmetic true false false true) := by decide

theorem subs_sets_no_borrow_carry :
    arithmeticFlags .subs .w (0#64) (1#64) =
      ((0xffffffff#64), .arithmetic true false false false) ∧
    arithmeticFlags .subs .w (1#64) (1#64) =
      ((0#64), .arithmetic false true true false) := by decide

theorem compare_exposes_overflow_and_carry :
    condition (.integer (0x7fffffff#64) (0xffffffff#64) .w) .vs = .ok true ∧
    condition (.integer (0x7fffffff#64) (0xffffffff#64) .w) .cs = .ok false := by decide

theorem flagBinary_actual_writes :
    evalInstruction 1 (.flagBinary .adds (w 0) (f 3) (x 1) (x 2) .w)
      (ints [(1, 0x7fffffff), (2, 1)]) empty =
      .ok ([(3, .flags (.arithmetic true false false true)),
        (0, .value (.int (0x80000000#64)))] ++ ints [(1, 0x7fffffff), (2, 1)], empty) := by decide

theorem wide_products_keep_signed_high_half :
    output (.wideMultiply .smull (x 0) (w 1) (w 2))
      (ints [(1, 0xfffffffe), (2, 3)]) (x 0) = .ok (.int (0xfffffffffffffffa#64)) ∧
    evalWideMultiply .smulh (0x8000000000000000#64) (2#64) = (0xffffffffffffffff#64) := by decide

theorem fused_multiply_arithmetic :
    output (.msub (x 0) (x 1) (x 2) (x 3) .x) (ints [(1, 2), (2, 3), (3, 17)]) (x 0) =
      .ok (.int (11#64)) ∧
    output (.madd (x 0) (x 1) (x 2) (x 3) .x) (ints [(1, 2), (2, 3), (3, 17)]) (x 0) =
      .ok (.int (23#64)) := by decide

theorem extension_and_bitfield_are_signed :
    output (.extend .sxtw (x 0) (w 1)) (ints [(1, 0xffffffff)]) (x 0) =
      .ok (.int (0xffffffffffffffff#64)) ∧
    output (.bfx true (x 0) (x 1) 0 8 .x) (ints [(1, 0x80)]) (x 0) =
      .ok (.int (0xffffffffffffff80#64)) := by decide

theorem comparison_cset_and_tst :
    (do
      let (regs, memory) ← evalInstruction 1 (.cmp (f 3) (x 1) (x 2) .x)
        (ints [(1, 9), (2, 9)]) empty
      let (regs, _) ← evalInstruction 1 (.cset (w 0) (f 3) .eq) regs memory
      read regs (x 0)) = .ok (.int (1#64)) ∧
    (do
      let (regs, memory) ← evalInstruction 1 (.tstImm (f 3) (x 1) 64 .x)
        (ints [(1, 64)]) empty
      let (regs, _) ← evalInstruction 1 (.cset (w 0) (f 3) .ne) regs memory
      read regs (x 0)) = .ok (.int (1#64)) := by decide

theorem csel_reads_only_selected_operand :
    output (.csel (x 0) (f 3) (x 1) (x 2) .eq .x)
      ((3, .flags (.arithmetic false true false false)) :: ints [(1, 10)]) (x 0) =
      .ok (.int (10#64)) ∧
    output (.csel (x 0) (f 3) (x 1) (x 2) .ne .x)
      ((3, .flags (.arithmetic false true false false)) :: ints [(1, 10)]) (x 0) =
      .error (.malformed "MIR register read before write") := by decide

private def object : Object := ⟨1, 99, 1, 8, true, List.replicate 8 none⟩
private def memory : Memory := ⟨2, [object]⟩
private def pointerRegs : RegisterFile := [(1, .value (.pointer 1 0))]

theorem fresh_scalar_load_is_not_zero :
    evalInstruction 1 (.load 0 (w 0) (x 1) 0 .w false) pointerRegs memory =
      .error (.diagnostic 3001) := by decide

theorem byte_store_and_load_roundtrip :
    (do
      let (regs, memory) ← evalInstruction 1 (.store 0 (x 2) (x 1) 0 .w false)
        ((2, .value (.int (0x10000002a#64))) :: pointerRegs) memory
      let (regs, _) ← evalInstruction 1 (.load 0 (x 0) (x 1) 0 .w false) regs memory
      read regs (x 0)) = .ok (.int (42#64)) := by decide

theorem frameSlot_mints_uninitialized_fresh_identity :
    evalInstruction 1 (.frameSlot (x 1) 99 8 8) [] empty =
      .ok (pointerRegs, memory) := by decide

theorem stackRelease_rejects_foreign_owner :
    evalInstruction 2 (.stackRelease (x 1)) pointerRegs memory =
      .error (.diagnostic 3018) := by decide

theorem stackRelease_ends_actual_lifetime :
    (do
      let (regs, memory) ← evalInstruction 1 (.stackRelease (x 1)) pointerRegs memory
      evalInstruction 1 (.load 0 (w 0) (x 1) 0 .w false) regs memory) =
      .error (.diagnostic 3010) := by decide

private def pairFunction : Function := ⟨"pair", [
  ⟨0, [x 1, x 2], [], .b 1 [x 2, x 1]⟩,
  ⟨1, [x 1, x 2], [], .ret [x 1, x 2]⟩]⟩
private def pairActivation : Activation :=
  ⟨"pair", 0, 0, ints [(1, 10), (2, 20)], 1, [.int (10#64), .int (20#64)]⟩

theorem jump_snapshots_all_sources :
    (do
      let entered ← jump .ssa pairFunction pairActivation 1 [x 2, x 1]
      return (← read entered.regs (x 1), ← read entered.regs (x 2))) =
      .ok (.int (20#64), .int (10#64)) := by decide

private def persistentFunction : Function := ⟨"persistent", [
  ⟨0, [x 1], [.mov (x 2) (x 1)], .b 1 []⟩,
  ⟨1, [], [], .ret [x 2]⟩]⟩
private def persistentProgram : Program := ⟨[persistentFunction]⟩

theorem postSSA_retains_live_values_after_copy :
    (do
      let entry ← initial persistentProgram "persistent" [.int (37#64)]
      return (runSteps .postSSA persistentProgram 3 entry).control) =
      .ok (.halted (.ok [.int (37#64)])) := by decide

private def entryBackedgeFunction : Function := ⟨"entry-backedge", [
  ⟨0, [x 1], [.binaryImm .add (x 1) (x 1) 1 .x], .b 0 []⟩]⟩
private def entryBackedgeProgram : Program := ⟨[entryBackedgeFunction]⟩

theorem postSSA_entry_backedge_does_not_rebind_call_signature :
    (do
      let entry ← initial entryBackedgeProgram "entry-backedge" [.int (37#64)]
      let state := runSteps .postSSA entryBackedgeProgram 4 entry
      match state.control with
      | .running activation => read activation.regs (x 1)
      | .halted _ => throw (Core.Source.Fault.malformed "unexpected halt")) =
      .ok (.int (39#64)) := by decide

theorem postSSA_refuses_uncleared_edge_arguments :
    jump .postSSA pairFunction pairActivation 0 [x 1, x 2] =
      .error (.malformed "MIR post-SSA edge arguments") := by decide

theorem SSA_does_not_invent_missing_block_arguments :
    (do
      let entry ← initial persistentProgram "persistent" [.int (37#64)]
      return (runSteps .ssa persistentProgram 3 entry).control) =
      .ok (.halted (.error (.malformed "MIR register read before write"))) := by decide

private def nineRegs : List VReg := (List.range 9).map x
private def nineValues : List Value := (List.range 9).map fun n => .int (BitVec.ofNat 64 (100+n))
private def callProgram : Program := ⟨[
  ⟨"caller", [⟨0, nineRegs, [.bl "callee" nineRegs [x 20]], .ret [x 20]⟩]⟩,
  ⟨"callee", [⟨0, nineRegs, [], .ret [x 8]⟩]⟩]⟩

theorem calls_retain_ninth_logical_argument_and_bind_result :
    (do
      let entry ← initial callProgram "caller" nineValues
      return (runSteps .ssa callProgram 3 entry).control) =
      .ok (.halted (.ok [.int (108#64)])) := by decide

private def branchProgram : Program := ⟨[
  ⟨"branch", [
    ⟨0, [w 0], [], .cb true (w 0) 1 [] 2 []⟩,
    ⟨1, [], [], .brk 3004⟩,
    ⟨2, [], [.movImm (w 1) 7], .ret [w 1]⟩]⟩]⟩

theorem cb_and_brk_preserve_diagnostic :
    (do
      let entry ← initial branchProgram "branch" [.int (1#64)]
      return (runSteps .ssa branchProgram 2 entry).control) =
      .ok (.halted (.error (.diagnostic 3004))) := by decide

private def bitBranchProgram : Program := ⟨[
  ⟨"bits", [
    ⟨0, [x 0], [], .tb true (x 0) 8 1 [] 2 []⟩,
    ⟨1, [], [], .brk 3005⟩,
    ⟨2, [], [], .brk 3002⟩]⟩]⟩

theorem tb_tests_named_bit_without_inventing_width :
    (do
      let entry ← initial bitBranchProgram "bits" [.int (256#64)]
      return (runSteps .ssa bitBranchProgram 2 entry).control) =
      .ok (.halted (.error (.diagnostic 3005))) := by decide

private def conditionProgram : Program := ⟨[
  ⟨"condition", [
    ⟨0, [x 0], [.cmpImm (f 1) (x 0) 0 .x], .bCond (f 1) .eq 1 [] 2 []⟩,
    ⟨1, [], [], .brk 3003⟩,
    ⟨2, [], [], .brk 3002⟩]⟩]⟩

theorem bCond_consumes_actual_flags :
    (do
      let entry ← initial conditionProgram "condition" [.int (0#64)]
      return (runSteps .ssa conditionProgram 3 entry).control) =
      .ok (.halted (.error (.diagnostic 3003))) := by decide

end EvoIR.Machine.ExecutionTests
