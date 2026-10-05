import EvoIR.Core.SymbolicStorage

/-! Regressions for exact local bindings, including uninitialized shadowing.
These are state-adapter tests, not whole-function admission. -/
namespace EvoIR.Core.Symbolic.StorageTests

private def widths : Var → Nat := fun _ => 32
private def x : CWord widths := ⟨.int, .var "x" 32 rfl⟩
private def mutableInt : Source.QualifiedType := ⟨.int, false⟩
private def constInt : Source.QualifiedType := ⟨.int, true⟩
private def initialized : LocalStore widths := [⟨1, mutableInt, some x.term⟩]
private def uninitialized : LocalStore widths := [⟨1, mutableInt, none⟩]
private def constant : LocalStore widths := [⟨1, constInt, none⟩]

example : (initialized.read 1 .int).isSome := by decide
example : (initialized.read 1 .uint).isNone := by decide
example : (initialized.read 2 .int).isNone := by decide
example : (uninitialized.read 1 .int).isNone := by decide
example : ((uninitialized ++ initialized).bindings.lookup 1).isNone := by decide
example : ((uninitialized ++ initialized).read 1 .int).isNone := by decide
example : (initialized.write 1 mutableInt x false).isSome := by decide
-- A sequenced initializer may have already assigned the mutable object.
example : (initialized.write 1 mutableInt x true).isSome := by decide
example : (LocalStore.write ([⟨1, constInt, some x.term⟩] : LocalStore widths) 1 constInt x true).isNone := by decide
example : (uninitialized.write 1 mutableInt x true).isSome := by decide
example : (constant.write 1 constInt x true).isSome := by decide
example : (constant.write 1 constInt x false).isNone := by decide
example : (initialized.write 2 mutableInt x false).isNone := by decide
example : (initialized.write 1 constInt x false).isNone := by decide
example : (initialized.write 1 mutableInt ⟨.uint, .const 32 7⟩ false).isNone := by decide
example : (initialized.declare 1 mutableInt).isNone := by decide
example : (initialized.declare 2 mutableInt).isSome := by decide
example : (initialized.exitScope 2).isNone := by decide
example : ((uninitialized ++ initialized).exitScope 1).map
    (fun s => (s.read 1 .int).isSome) = some true := by decide

-- The actual source-machine relation holds for every represented value.
example (σ : Subst widths) :
    (uninitialized ++ initialized).bindings.Related σ
      ((uninitialized ++ initialized).denote σ) := local_bindings_related σ _

end EvoIR.Core.Symbolic.StorageTests
