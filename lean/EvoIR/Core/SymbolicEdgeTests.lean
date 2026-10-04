import EvoIR.Core.SymbolicEdge
import EvoIR.Core.IRReader

/-! Simultaneous copies and local SSA boundaries, including loops and tokens.
No test below claims that target-edge checking alone verifies a source CFG. -/
namespace EvoIR.Core.Symbolic.EdgeTests

private def widths (_ : Var) : Nat := 32
private def x : Word widths := ⟨32, .var "x" 32 rfl⟩
private def y : Word widths := ⟨32, .var "y" 32 rfl⟩
private def input : Scope widths := [(0, .word x), (1, .word y), (2, .opaque 0), (3, .opaque 1)]
private def types (id : Nat) : IR.Ty := if id == 0 then .memory else .world
private def block (name : String) : IR.Block :=
  ⟨name, [⟨0, .bits 32⟩, ⟨1, .bits 32⟩, ⟨2, .memory⟩, ⟨3, .world⟩], [], .branch ⟨name, [1, 0, 2, 3]⟩⟩
private def f : IR.Function :=
  ⟨"f", [.bits 32, .bits 32, .memory, .world], [.bits 32, .memory, .world], true,
    IR.Reader.emptyEffects, [block "entry", block "loop"]⟩
private def swapped : Scope widths := [(3, .opaque 1), (2, .opaque 0), (1, .word x), (0, .word y)]

example : checkEdge types f input ⟨"loop", [1, 0, 2, 3]⟩ swapped = true := by decide
example : checkEdge types f input ⟨"entry", [1, 0, 2, 3]⟩ swapped = true := by decide
-- A sequentialized swap loses x; the simultaneous transfer must reject it.
example : checkEdge types f input ⟨"loop", [1, 0, 2, 3]⟩
    [(3, .opaque 1), (2, .opaque 0), (1, .word y), (0, .word y)] = false := by decide
example : checkEdge types f input ⟨"missing", [1, 0, 2, 3]⟩ swapped = false := by decide
example : checkEdge types f input ⟨"loop", [1, 9, 2, 3]⟩ swapped = false := by decide
example : checkEdge types f input ⟨"loop", [1, 0, 3, 2]⟩ swapped = false := by decide
example : checkEdge types f input ⟨"loop", [1, 0, 2]⟩ swapped = false := by decide
example : checkEdge types f input ⟨"loop", [1, 0, 2, 3, 0]⟩ swapped = false := by decide
-- A join cannot retain a previous block's unpassed local value.
example : checkEdge types f ((9, .word x) :: input) ⟨"loop", [1, 0, 2, 3]⟩
    ((9, .word x) :: swapped) = false := by decide
example : checkEdge types f ((9, .word x) :: input) ⟨"loop", [1, 0, 2, 3]⟩ swapped = true := by decide
example : (bindSlots types [⟨0, .bits 32⟩, ⟨0, .bits 32⟩] [.word x, .word y]).isNone := by decide
example : (bindSlots types [⟨0, .bits 64⟩] [.word x]).isNone := by decide
example : (bindSlots (Γ := widths) types [⟨0, .bits 32⟩] [.opaque 0]).isNone := by decide

example (σ : Subst widths) (ρ : Nat → IR.Value) (typed : ∀ id, (ρ id).type = types id)
    (a : IR.Activation) (values : a.values = input.denote σ ρ) :
    IR.jump f a ⟨"loop", [1, 0, 2, 3]⟩ =
      .ok {a with block := "loop", pc := 0, values := swapped.denote σ ρ} :=
  edge_sound σ ρ types typed f a input swapped ⟨"loop", [1, 0, 2, 3]⟩ values (by decide)

end EvoIR.Core.Symbolic.EdgeTests
