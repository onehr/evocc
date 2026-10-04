import EvoIR.Core.StateExpression
import EvoIR.Core.SymbolicStorageTests

namespace EvoIR.Core.Symbolic.StateTests

private def widths : Var → Nat := fun _ => 32
private def σ : Subst widths := fun _ => 0
private def locals : LocalStore widths := [⟨1, ⟨.int, false⟩, some (.const 32 0)⟩]
private def set (n : Nat) : Source.Expr :=
  .assign .int .set 1 ⟨.int, false⟩ (.ident .int 1) (.literal .int n)
private def read : Source.Expr := .ident .int 1
private def summarize (e : Source.Expr) : Option (Source.Result × Source.Result) := do
  let tree ← stateExpression 20 locals e
  let result := tree.val.denote σ
  return (result.2, Source.readSlot result.1 1 .int)

-- Existing read-only expression checking refuses this RED; the stateful
-- checker must account for the write, not erase it to make the input fit.
example : (expression locals.bindings (set 7)).isNone := by decide
example : summarize (set 7) = some (.ok (.ofNat .int 7), .ok (.ofNat .int 7)) := by decide
example : summarize (.comma .int (set 7) read) =
    some (.ok (.ofNat .int 7), .ok (.ofNat .int 7)) := by decide
example : summarize (.comma .int (set 7) (set 9)) =
    some (.ok (.ofNat .int 9), .ok (.ofNat .int 9)) := by decide
example : summarize (.conditional .int (set 1) (set 7) (set 9)) =
    some (.ok (.ofNat .int 7), .ok (.ofNat .int 7)) := by decide
example : summarize (.conditional .int (set 0) (set 7) (set 9)) =
    some (.ok (.ofNat .int 9), .ok (.ofNat .int 9)) := by decide
example : summarize (.cast .ulong ⟨.ulong, false⟩ (set 0xffffffff)) =
    some (.ok (.ofNat .ulong 0xffffffffffffffff), .ok (.ofNat .int 0xffffffff)) := by decide

private def overflow : Source.Expr := .binary .int .add read (.literal .int 1)
example : summarize (.comma .int (set 0x7fffffff) overflow) =
    some (.error (.diagnostic 3004), .ok (.ofNat .int 0x7fffffff)) := by decide
example : summarize (.comma .int (.comma .int (set 0x7fffffff) overflow) (set 9)) =
    some (.error (.diagnostic 3004), .ok (.ofNat .int 0x7fffffff)) := by decide
example : summarize (.conditional .int (set 0) overflow (set 9)) =
    some (.ok (.ofNat .int 9), .ok (.ofNat .int 9)) := by decide
example : (stateExpression (Γ := widths) 20 [⟨1, ⟨.int, true⟩, some (.const 32 0)⟩] (set 7)).isNone := by decide
example : (stateExpression 20 locals
    (.assign .int .set 1 ⟨.int, false⟩ (.ident .int 2) (.literal .int 7))).isNone := by decide
example : (stateExpression 20 locals
    (.assign .long .set 1 ⟨.int, false⟩ (.ident .int 1) (.literal .int 7))).isNone := by decide
example : (stateExpression 20 locals (.comma .long (set 7) read)).isNone := by decide

example : summarize (.binary .int .and (set 0) (set 9)) =
    some (.ok (.ofNat .int 0), .ok (.ofNat .int 0)) := by decide
example : summarize (.binary .int .and (set 2) (set 9)) =
    some (.ok (.ofNat .int 1), .ok (.ofNat .int 9)) := by decide
example : summarize (.binary .int .or (set 2) (set 9)) =
    some (.ok (.ofNat .int 1), .ok (.ofNat .int 2)) := by decide
example : summarize (.binary .int .or (set 0) (set 9)) =
    some (.ok (.ofNat .int 1), .ok (.ofNat .int 9)) := by decide
example : summarize (.binary .int .or (set 0x7fffffff) overflow) =
    some (.ok (.ofNat .int 1), .ok (.ofNat .int 0x7fffffff)) := by decide
example : summarize (.binary .int .and (set 0x7fffffff) overflow) =
    some (.error (.diagnostic 3004), .ok (.ofNat .int 0x7fffffff)) := by decide
example : summarize (.binary .int .add (set 7) (.literal .int 1)) =
    some (.ok (.ofNat .int 8), .ok (.ofNat .int 7)) := by decide
example : summarize (.binary .int .add (.literal .int 1) (set 7)) =
    some (.ok (.ofNat .int 8), .ok (.ofNat .int 7)) := by decide
example : summarize (.unary .int .neg (set 7)) =
    some (.ok (.ofInt .int (-7)), .ok (.ofNat .int 7)) := by decide
example : summarize (.unary .int .neg (set 0x80000000)) =
    some (.error (.diagnostic 3004), .ok (.ofNat .int 0x80000000)) := by decide
example : summarize (.binary .int .shr (set 7) (.literal .long 64)) =
    some (.error (.diagnostic 3005), .ok (.ofNat .int 7)) := by decide
example : summarize (.binary .int .eq (set 7) (.literal .uint 7)) =
    some (.ok (.ofNat .int 1), .ok (.ofNat .int 7)) := by decide

-- Every returned diagram proves the actual source transition sequence for
-- all valuations and continuations, including the local store at a fault.
example (e : Source.Expr) (tree : StateTree widths)
    (checked : StateDiagram locals e tree) (valuation : Subst widths)
    (p : Source.Program) (fid : Nat) :
    Source.EvaluatesState p (locals.denote valuation) fid e (tree.denote valuation) :=
  checked valuation p fid

private def sampleExpressions : List Source.Expr := [set 7,
  .comma .int (set 7) read, .comma .int (set 7) (set 9),
  .conditional .int (set 1) (set 7) (set 9), .conditional .int (set 0) (set 7) (set 9),
  .cast .ulong ⟨.ulong, false⟩ (set 0xffffffff),
  .comma .int (set 0x7fffffff) overflow,
  .comma .int (.comma .int (set 0x7fffffff) overflow) (set 9),
  .conditional .int (set 0) overflow (set 9),
  .binary .int .and (set 0) (set 9), .binary .int .and (set 2) (set 9),
  .binary .int .or (set 2) (set 9), .binary .int .or (set 0) (set 9),
  .binary .int .or (set 0x7fffffff) overflow, .binary .int .and (set 0x7fffffff) overflow,
  .binary .int .add (set 7) (.literal .int 1), .binary .int .add (.literal .int 1) (set 7),
  .unary .int .neg (set 7), .unary .int .neg (set 0x80000000),
  .binary .int .shr (set 7) (.literal .long 64), .binary .int .eq (set 7) (.literal .uint 7)]

-- Test-only bounded execution of the actual source step function, stopping
-- before the surrounding continuation. It supplies no proof/admission.
private def runExpression : Nat → Source.State → Option Source.StateOutcome
  | 0, _ => none
  | fuel+1, state => match state.control, state.continuation with
    | .value value, [] => some (state.locals, .ok value)
    | .halted result, _ => some (state.locals, result)
    | _, _ => do
      let next ← Source.step ⟨[], []⟩ state
      runExpression fuel next

def stateExpressionSelfTest : IO UInt32 := do
  let mut failures := 0
  let mut cases := 0
  let expressions := sampleExpressions ++
    ([Source.Binary.add, .sub, .mul, .bitAnd, .bitOr, .bitXor, .eq, .ne, .lt, .le, .gt, .ge].flatMap
      fun op => ([0, 1, 0x7fffffff, 0x80000000, 0xffffffff].map fun value =>
        let type := if [Source.Binary.add, .sub, .mul, .bitAnd, .bitOr, .bitXor].contains op
          then Source.IntType.uint else .int
        Source.Expr.binary type op (set value) (.literal .uint 1)))
  for expr in expressions do
    cases := cases+1
    let some diagram := stateExpression (Source.expressionDepth expr) locals expr
      | IO.eprintln "stateful expression unexpectedly refused"; failures := failures+1; continue
    let expected := diagram.val.denote σ
    let some actual := runExpression 1000 ⟨.expression expr, locals.denote σ, [], 0⟩
      | IO.eprintln "stateful source test did not terminate"; failures := failures+1; continue
    if expected.2 != actual.2 || Source.readSlot expected.1 1 .int != Source.readSlot actual.1 1 .int then
      IO.eprintln s!"stateful source/diagram disagreement: {repr expr}"
      failures := failures+1
  if failures == 0 then IO.println s!"{cases} stateful expression/source-machine comparisons pass"
  return if failures == 0 then 0 else 1

end EvoIR.Core.Symbolic.StateTests
