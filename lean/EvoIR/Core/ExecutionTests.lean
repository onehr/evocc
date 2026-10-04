import EvoIR.Core.Execution

/-! Finite semantic regressions, not universal compilation proofs. -/
namespace EvoIR.Core.Source

private def q (t : IntType := .int) : QualifiedType := ⟨t, false⟩
private def lit (n : Int) (t : IntType := .int) : Expr :=
  .literal t (BitVec.ofInt t.width n).toNat
private def x : Expr := .ident .int 1
private def set (id : Nat) (e : Expr) : Expr :=
  .assign .int .set id (q) (.ident .int id) e
private def inc (id : Nat) : Expr := .update .int .postInc id (q) (.ident .int id)
private def ret (e : Expr) : Stmt := .return (q) e
private def stmt (e : Expr) : BlockItem := .statement (.expression e)
private def decl (id : Nat) (e : Expr) : BlockItem :=
  .declaration ⟨id, "local", q, .none, some (q, e)⟩
private def expr (op : Binary) (a b : Expr) : Expr := .binary .int op a b
private def function (body : Stmt) : Function :=
  ⟨100, "f", q, false, [⟨1, q, .none⟩], body⟩
private def program (body : Stmt) : Program := ⟨[100], [function body]⟩
private def result (p : Program) (arg : Int := 0) (fuel : Nat := 10000) : Option Result :=
  match initial p 100 [.ofInt .int arg] with
  | .error f => some (.error f)
  | .ok s => match (runSteps p fuel s).control with
    | .halted r => some r
    | _ => none

private def loopProgram (loop : Stmt) : Program := program (.compound [
  decl 2 (lit 0), .statement loop, .statement (ret (.ident .int 2))])

private def cases : List (String × Program × Int × Option Result) := [
  ("identity", program (ret x), -9, some (.ok (.ofInt .int (-9)))),
  ("signed add", program (ret (expr .add x (lit 1))), 7, some (.ok (.ofInt .int 8))),
  ("runtime overflow", program (ret (expr .add x (lit 1))), 2147483647,
    some (.error (.diagnostic 3004))),
  ("literal overflow wraps", program (ret (expr .add (lit 2147483647) (lit 1))), 0,
    some (.ok (.ofInt .int (-2147483648)))),
  ("comma is not a constant operand", program (ret (expr .add
    (.comma .int (lit 0) (lit 2147483647)) (lit 1))), 0, some (.error (.diagnostic 3004))),
  ("division zero", program (ret (expr .div x (lit 0))), 1, some (.error (.diagnostic 3002))),
  ("remainder zero", program (ret (expr .rem x (lit 0))), 1, some (.error (.diagnostic 3003))),
  ("first diagnostic", program (ret (expr .add (expr .div x (lit 0))
    (expr .rem x (lit 0)))), 1, some (.error (.diagnostic 3002))),
  ("literal rule cannot hide MIN/-1", program (ret (expr .add
    (expr .div (lit (-2147483648)) (lit (-1))) (lit 1))), 0, some (.error (.diagnostic 3004))),
  ("independent shift width", program (ret (.binary .int .shr x (lit 4294967296 .ulong))),
    1, some (.error (.diagnostic 3005))),
  ("short-circuit skipped diagnostic", program (ret (expr .and x (expr .div x (lit 0)))),
    0, some (.ok (.ofInt .int 0))),
  ("short-circuit executed diagnostic", program (ret (expr .and x (expr .div x (lit 0)))),
    1, some (.error (.diagnostic 3002))),
  ("post increment", program (ret (inc 1)), 4, some (.ok (.ofInt .int 4))),
  ("sequenced post increment", program (ret (.comma .int (inc 1) x)),
    4, some (.ok (.ofInt .int 5))),
  ("nested scope keeps outer writes", program (.compound [
    .statement (.compound [decl 2 (lit 8), stmt (set 1 (.ident .int 2))]),
    .statement (ret x)]), 1, some (.ok (.ofInt .int 8))),
  ("return unwinds scopes", program (.compound [
    .statement (.compound [decl 2 (lit 8), .statement (ret (.ident .int 2))]),
    .statement (ret (lit 0))]), 0, some (.ok (.ofInt .int 8))),
  ("while counts down", loopProgram (.while (expr .gt x (lit 0)) (.compound [
    stmt (set 1 (expr .sub x (lit 1))), stmt (inc 2)])), 3, some (.ok (.ofInt .int 3))),
  ("do executes once", loopProgram (.doWhile (.expression (inc 2)) x),
    0, some (.ok (.ofInt .int 1))),
  ("for continue still increments", loopProgram (.for (.expression (set 1 (lit 0)))
    (some (expr .lt x (lit 4))) (some (inc 1)) (.compound [
      .statement (.ite (expr .eq x (lit 2)) .continue none), stmt (inc 2)])),
    0, some (.ok (.ofInt .int 3))),
  ("break skips increment", program (.compound [
    .statement (.for .empty none (some (inc 1)) .break), .statement (ret x)]),
    7, some (.ok (.ofInt .int 7))),
  ("infinite loop remains nonterminal", program (.compound [
    .statement (.while (lit 1) .null), .statement (ret x)]), 0, none)
]

private def callCase : Program := ⟨[100], [
  ⟨50, "callee", q, true, [⟨1, q, .none⟩],
    .compound [stmt (set 1 (expr .add x (lit 1))), .statement (ret x)]⟩,
  function (.compound [decl 2 (.call .int 50 [(q, x)]),
    .statement (ret (expr .add x (.ident .int 2)))]) ]⟩

def executionSelfTest : IO UInt32 := do
  let mut failures := 0
  for (name, p, arg, expected) in cases do
    if (checkWellFormed p).toOption.isNone then
      IO.eprintln s!"execution fixture refused: {name}"
      failures := failures + 1
    if result p arg != expected then
      IO.eprintln s!"core execution mismatch: {name}: {reprStr (result p arg)}"
      failures := failures + 1
  -- Reusing the same parameter symbol in two functions must not share a slot.
  if (checkWellFormed callCase).toOption.isNone || result callCase 5 != some (.ok (.ofInt .int 11)) then
    IO.eprintln "core execution mismatch: isolated call frame"
    failures := failures + 1
  if failures == 0 then
    IO.println s!"{cases.length + 1} core source execution cases pass (no compilation claim)"
    return 0
  return 1

end EvoIR.Core.Source
