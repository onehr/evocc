import EvoIR.MachineCopies

namespace EvoIR.Machine.Copies.Tests

/-- A correctly saved swap, with its temporary projected out. -/
example : check [0, 1] [(0, 1), (1, 0)] [(2, 1), (1, 0), (0, 2)] = true := by
  decide

/-- The superficially plausible sequential swap loses the first source. -/
example : check [0, 1] [(0, 1), (1, 0)] [(0, 1), (1, 0)] = false := by
  decide

example : check [0, 1, 2] [(0, 1), (1, 2), (2, 0)]
    [(3, 1), (1, 2), (2, 0), (0, 3)] = true := by decide

example : check [0] [(0, 0)] [] = true := by decide
example : check [0] [(0, 0)] [(0, 0)] = true := by decide

/-- Omitting a non-self transfer is rejected. -/
example : check [0, 1] [(0, 1)] [] = false := by decide

/-- A copied but independently live temporary is not allowed to change. -/
example : check [0, 1, 2] [(0, 1), (1, 0)]
    [(2, 1), (1, 0), (0, 2)] = false := by decide

/-- Reversing an edge operand is rejected even if the proposed move is valid. -/
example : check [0, 1] [(0, 1)] [(1, 0)] = false := by decide

example : checkFrom [0] (fun _ => 8) (fun _ => 7) [(0, 1)] = false := by decide
example : checkFrom [0] (fun _ => 7) (fun _ => 7) [(0, 1)] = true := by decide

#print axioms check_sound
#print axioms checkFrom_sound

end EvoIR.Machine.Copies.Tests
