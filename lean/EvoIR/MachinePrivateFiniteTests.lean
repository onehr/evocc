import EvoIR.MachinePrivateFinite
import EvoIR.MachinePrivateUninitialized

/-! Focused mutations of the actual private load/store shapes. Successful
finite evaluation is not admission; positive cases invoke the same universal
function checker used by the public actual-program-pair entry. -/
namespace EvoIR.Core.Symbolic.MachineSelection.PrivateFiniteTests

private def effects : IR.Effects := ⟨true, true, false, false, false, false, true, false⟩
private def parameters : List Source.Parameter := [⟨0, ⟨.uint, false⟩, .none⟩]
private def w (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr32⟩
private def x (id : Nat) : EvoIR.Machine.VReg := ⟨id, .gpr64⟩

private def source (initialized := true) : IR.Program := ⟨[
  ⟨"cell", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, effects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩],
      [⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 0 4 4, [1, 2]⟩,
       ⟨[⟨6, .memory⟩], .access 4 4, [4, 3]⟩] ++
      (if initialized then [⟨[⟨7, .memory⟩, ⟨8, .world⟩], .store, [6, 5, 3, 0]⟩] else []) ++
      [⟨[⟨9, .memory⟩], .access 4 4, [if initialized then 7 else 6, 3]⟩,
       ⟨[⟨10, .bits 32⟩, ⟨11, .memory⟩, ⟨12, .world⟩], .load 32,
         [9, if initialized then 8 else 5, 3]⟩,
       ⟨[⟨13, .memory⟩, ⟨14, .world⟩], .endLifetime, [11, 12, 3]⟩],
       .return [10, 13, 14]⟩]⟩]⟩

private def target (origin := 0) (stored := true) (kind : EvoIR.Machine.LoadKind := .w)
    (wrongReturn := false) (wrongAddress := false) : EvoIR.Machine.Program := ⟨[
  ⟨"cell", [⟨0, [w 0],
    [.frameSlot (x 3) origin 4 4] ++
    (if stored then [.store 0 (w 0) (x 3) 0 .w false] else []) ++
    [.load 0 (w 10) (if wrongAddress then x 99 else x 3) 0 kind false] ++
    (if wrongReturn then [.movImm (w 20) 1] else []),
    .ret [if wrongReturn then w 20 else w 10]⟩]⟩]⟩

private def admitted (ir : IR.Program) (machine : EvoIR.Machine.Program) : Bool :=
  (privateFiniteFunctionPair ir machine "cell" parameters .uint).isSome

example : admitted (source) (target) = true := by decide
example : admitted (source) (target 1) = false := by decide
example : admitted (source) (target 0 false) = false := by decide
example : admitted (source) (target 0 true .x) = false := by decide
example : admitted (source) (target 0 true .w true) = false := by decide
example : admitted (source) (target 0 true .w false true) = false := by decide

-- Both machines diagnose uninitialized bytes, but diagnostic 3001 is not an
-- admitted Core observation. Agreement alone must not widen that boundary.
example : admitted (source false) (target 0 false) = false := by decide

theorem private_cell_all_inputs (values : List Source.Value)
    (typed : values.map Source.Value.type = parameters.map (fun p => p.type.type))
    (accepted : admitted (source) (target) = true) :
    ∃ sourceEntry targetEntry,
      IR.initial (source) "cell" (values.map sourceValueIR ++ [.memory ⟨1, []⟩, .world]) =
        .ok sourceEntry ∧
      EvoIR.Machine.initial (target) "cell" (values.map EvoIR.Machine.sourceArgument) =
        .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines (source) (target) .uint sourceEntry targetEntry := by
  cases checked : privateFiniteFunctionPair (source) (target) "cell" parameters .uint with
  | none => simp [admitted, checked] at accepted
  | some certificate => exact certificate.checked values typed

#print axioms private_cell_all_inputs
#print axioms EvoIR.Machine.PrivateMemory.load_uninitialized_transfer

end EvoIR.Core.Symbolic.MachineSelection.PrivateFiniteTests
