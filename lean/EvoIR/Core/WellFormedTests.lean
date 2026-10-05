import EvoIR.Core.WellFormed

/-! Input validation tests. Acceptance here is not a compilation theorem. -/
namespace EvoIR.Core.Source

private def q (t : IntType := .int) : QualifiedType := ⟨t, false⟩
private def x : Expr := .ident .int 1
private def one : Expr := .literal .int 1
private def setX (e : Expr) : Expr := .assign .int .set 1 (q) x e
private def incX : Expr := .update .int .postInc 1 (q) x
private def ret (e : Expr) : Stmt := .return (q) e
private def functionOf (body : Stmt) : Function :=
  ⟨100, "f", q, false, [⟨1, q, .none⟩], body⟩
private def programOf (body : Stmt) : Program := ⟨[100], [functionOf body]⟩
private def exprProgram (e : Expr) : Program := programOf (ret e)
private def localObject (id : Nat) (initializer : Option Expr) : BlockItem :=
  .declaration ⟨id, "local", q, .none, initializer.map (q, ·)⟩
private def accepted (p : Program) : Bool :=
  match checkWellFormed p with | .ok _ => true | .error _ => false

private def setLocal (id : Nat) : Stmt :=
  .expression (.assign .int .set id (q) (.ident .int id) one)
private def callProgram (t : IntType) (args : List (QualifiedType × Expr)) : Program :=
  ⟨[100], [⟨50, "g", q, true,
    [⟨5, q .long, .none⟩, ⟨6, q, .none⟩], ret (.ident .int 6)⟩,
    functionOf (ret (.call t 50 args))]⟩

private def wellFormedCases : List (String × Program × Bool) := [
  ("identity", exprProgram x, true),
  ("arithmetic", exprProgram (.binary .int .add x one), true),
  ("wrong operator result type", exprProgram (.binary .long .add x one), false),
  ("unbound identifier", exprProgram (.ident .int 2), false),
  ("wrong binding type", exprProgram (.ident .uint 1), false),
  ("wrong literal width", exprProgram (.literal .int (2^32)), false),
  ("wrong cast destination", exprProgram (.cast .int (q .long) x), false),
  ("wrong comparison result", exprProgram (.binary .uint .lt x one), false),
  ("signed left shift", exprProgram (.binary .int .shl x one), false),
  ("independent shift count width", exprProgram
    (.binary .int .shr x (.literal .ulongLong 1)), true),
  ("assignment reads old value", exprProgram (setX (.binary .int .add x one)), true),
  ("unsequenced sibling", exprProgram (.binary .int .add incX x), false),
  ("unsequenced nested assignment", exprProgram (setX incX), false),
  ("sequenced comma", exprProgram (.comma .int incX x), true),
  ("short circuit sequencing", exprProgram (.binary .int .and incX x), true),
  ("wrong assignment place", exprProgram (.assign .int .set 2 (q) x one), false),
  ("uninitialized", programOf (.compound [localObject 2 none,
    .statement (ret (.ident .int 2))]), false),
  ("self initialization", programOf (.compound [localObject 2 (some (.ident .int 2)),
    .statement (ret one)]), false),
  ("scope exit", programOf (.compound [
    .statement (.compound [localObject 2 (some one)]),
    .statement (ret (.ident .int 2))]), false),
  ("branch does not establish initialization", programOf (.compound [localObject 2 none,
    .statement (.ite x (.expression (.assign .int .set 2 (q) (.ident .int 2) one)) none),
    .statement (ret (.ident .int 2))]), false),
  ("fallthrough", programOf (.expression x), false),
  ("break outside loop", programOf (.compound [.statement .break, .statement (ret x)]), false),
  ("continue outside loop", programOf (.compound [.statement .continue, .statement (ret x)]), false),
  ("wrong return conversion", programOf (.return (q .uint) x), false),
  ("missing entry", ⟨[101], [functionOf (ret x)]⟩, false),
  ("duplicate entry", ⟨[100, 100], [functionOf (ret x)]⟩, false),
  ("duplicate definition", ⟨[100], [functionOf (ret x), functionOf (ret x)]⟩, false),
  ("recursion", exprProgram (.call .int 100 [(q, x)]), false),
  ("external call", exprProgram (.call .int 200 [(q, x)]), false),
  ("const mutation", ⟨[100], [{functionOf (ret incX) with
    parameters := [⟨1, ⟨.int, true⟩, .none⟩]}]⟩, false),
  ("duplicate parameter", ⟨[100], [{functionOf (ret x) with
    parameters := [⟨1, q, .none⟩, ⟨1, q, .none⟩]}]⟩, false),
  ("both branches initialize", programOf (.compound [localObject 2 none,
    .statement (.ite x (setLocal 2) (some (setLocal 2))),
    .statement (ret (.ident .int 2))]), true),
  ("only continuing branch initializes", programOf (.compound [localObject 2 none,
    .statement (.ite x (ret one) (some (setLocal 2))),
    .statement (ret (.ident .int 2))]), true),
  ("loop may execute zero times", programOf (.compound [localObject 2 none,
    .statement (.while x (setLocal 2)),
    .statement (ret (.ident .int 2))]), false),
  ("continue bypasses initialization for increment", programOf (.compound [localObject 2 none,
    .statement (.for .empty (some x) (some (.ident .int 2))
      (.compound [.statement .continue, .statement (setLocal 2)])),
    .statement (ret one)]), false),
  ("short circuit does not guarantee right initialization", programOf (.compound [localObject 2 none,
    .statement (.expression (.binary .int .and x
      (.assign .int .set 2 (q) (.ident .int 2) one))),
    .statement (ret (.ident .int 2))]), false),
  ("duplicate active local", programOf (.compound [localObject 1 (some one),
    .statement (ret one)]), false),
  ("initializer sequenced self-assignment", programOf (.compound [
    localObject 2 (some (.comma .int (.assign .int .set 2 (q) (.ident .int 2) one) one)),
    .statement (ret (.ident .int 2))]), true),
  ("wrong initializer conversion", programOf (.compound [
    .declaration ⟨2, "v", q, .none, some (q .uint, one)⟩,
    .statement (ret one)]), false),
  ("closed call and implicit conversion", callProgram .int [(q .long, x), (q, one)], true),
  ("wrong argument conversion", callProgram .int [(q, x), (q, one)], false),
  ("wrong call result", callProgram .long [(q .long, x), (q, one)], false),
  ("wrong call arity", callProgram .int [(q .long, x)], false),
  ("conflicting call arguments", callProgram .int [(q .long, incX), (q, x)], false),
  ("dead branch still validated", programOf (.ite (.literal .int 0)
    (ret (.ident .int 99)) (some (ret x))), false)
]

-- Kernel-reduced tests of signedness/rank, including equal width/different rank.
example : usualType .int .uint = .uint := by decide
example : usualType .long .uint = .long := by decide
example : usualType .longLong .ulong = .ulongLong := by decide
example : usualType .ulong .longLong = .ulongLong := by decide
example : usualType .long .longLong = .longLong := by decide

def wellFormedSelfTest : IO UInt32 := do
  let mut failures := 0
  for (name, program, expected) in wellFormedCases do
    if accepted program != expected then
      IO.eprintln s!"core well-formedness mismatch: {name} ({reprStr (checkWellFormed program)})"
      failures := failures + 1
  if failures == 0 then
    IO.println s!"{wellFormedCases.length} core well-formedness cases pass (no compilation claim)"
    return 0
  return 1

end EvoIR.Core.Source
