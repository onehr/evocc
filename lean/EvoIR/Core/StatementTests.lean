import EvoIR.Core.StatementDiagram
import EvoIR.Core.LeafCorrespondence

namespace EvoIR.Core.Symbolic.StatementTests

private def widths : Var → Nat := fun _ => 32
private def σ : Subst widths := fun _ => 0
private def intType : Source.QualifiedType := ⟨.int, false⟩
private def constant (n : Nat) : Source.Expr := .literal .int n
private def read (id : Nat) : Source.Expr := .ident .int id
private def store (id n : Nat) : Source.Stmt :=
  .expression (.assign .int .set id intType (read id) (constant n))
private def object (id : Nat) (initializer : Option Source.Expr) (isConst := false) : Source.Object :=
  ⟨id, "local", ⟨.int, isConst⟩, .none, initializer.map (⟨.int, isConst⟩, ·)⟩
private def ret (e : Source.Expr) : Source.Stmt := .return intType e
private def declaration (id : Nat) (value : Option Source.Expr) (isConst := false) : Source.BlockItem :=
  .declaration (object id value isConst)
private def statements (items : List Source.BlockItem) : Source.Stmt := .compound items
private def initial : LocalStore widths := [⟨1, intType, some (.const 32 7)⟩]

private def result (stmt : Source.Stmt) : Option Source.Result := do
  let tree ← flowDiagram (Source.statementDepth stmt) initial (.statement stmt)
  match (tree.val.denote σ).2 with
  | .error fault => some (.error fault)
  | .ok (.returned value) => some (.ok value)
  | _ => none

private def initialized : Source.Stmt := statements [
  declaration 2 (some (read 1)) true, .statement (ret (read 2))]
private def uninitialized : Source.Stmt := statements [
  declaration 2 none, .statement (ret (read 2))]
private def assigned : Source.Stmt := statements [
  declaration 2 none, .statement (store 2 9), .statement (ret (read 2))]
private def scope : Source.Stmt := statements [
  .statement (statements [declaration 2 (some (constant 3)), .statement (store 1 9)]),
  .statement (ret (read 1))]
private def earlyReturn : Source.Stmt := statements [
  declaration 2 (some (constant 3)), .statement (ret (read 2)),
  .statement (store 1 100)]
private def overflow : Source.Expr := .binary .int .add (read 1) (constant 1)
private def failingInitializer : Source.Stmt := statements [
  .statement (store 1 0x7fffffff), declaration 2 (some overflow),
  .statement (store 1 1), .statement (ret (read 1))]

-- The old return-shape path rejects a block with declarations, even when
-- the body is valid and its actual lowering is a scalar return.
example : (Source.returnShape initialized).isNone := by decide
example : result initialized = some (.ok (.ofNat .int 7)) := by decide
example : result assigned = some (.ok (.ofNat .int 9)) := by decide
example : result uninitialized = none := by decide
example : result scope = some (.ok (.ofNat .int 9)) := by decide
example : result earlyReturn = some (.ok (.ofNat .int 3)) := by decide
example : result failingInitializer = some (.error (.diagnostic 3004)) := by decide
example : result (statements [declaration 2 (some (constant 1)) true,
    .statement (store 2 9), .statement (ret (read 2))]) = none := by decide
example : result (statements [declaration 2 (some (read 2)),
    .statement (ret (read 2))]) = none := by decide
example : result (statements [declaration 1 none, .statement (ret (read 1))]) = none := by decide
example : result (statements [
    .statement (statements [declaration 2 (some (constant 3))]),
    .statement (ret (read 2))]) = none := by decide
example : result (statements [
    .statement (.ite (read 1) (ret (constant 3)) (some (ret (constant 4)))),
    .statement (store 1 100)]) = some (.ok (.ofNat .int 3)) := by decide
example : result (statements [
    .statement (.ite (constant 0) (store 1 4) none),
    .statement (ret (read 1))]) = some (.ok (.ofNat .int 7)) := by decide
example : (flowDiagram (Γ := widths) 0 initial (.statement initialized)).isNone := by decide
example : (flowDiagram (Γ := widths) 10 initial
    (.statement (.while (constant 1) .null))).isNone := by decide
example : (do let t ← flowDiagram (Γ := widths) 10 initial (.statement .null)
              t.val.returns?.map Subtype.val).isNone := by decide
example : (do let t ← flowDiagram (Γ := widths) 10 initial (.statement .break)
              t.val.returns?.map Subtype.val).isNone := by decide
example : (do let t ← flowDiagram (Γ := widths) 10 initial (.statement .continue)
              t.val.returns?.map Subtype.val).isNone := by decide

-- Diagnostic exits retain both the preceding write and the newly declared,
-- still uninitialized object. They must not unwind the scope or run its tail.
example : (do
    let t ← flowDiagram (Source.statementDepth failingInitializer) initial (.statement failingInitializer)
    pure ((t.val.denote σ).1.length,
      Source.readSlot (t.val.denote σ).1 1 .int,
      Source.readSlot (t.val.denote σ).1 2 .int)) =
    some (2, .ok (.ofNat .int 0x7fffffff), .error (.malformed "uninitialized local read")) := by decide

-- Success unwinds a local scope for normal and all nonlocal control exits.
private def scopedFlow (stmt : Source.Stmt) : Option Nat := do
  let body := statements [declaration 2 (some (constant 3)), .statement stmt]
  let t ← flowDiagram (Source.statementDepth body) initial (.statement body)
  pure (t.val.denote σ).1.length
example : scopedFlow .null = some 1 := by decide
example : scopedFlow .break = some 1 := by decide
example : scopedFlow .continue = some 1 := by decide
example : scopedFlow (ret (read 2)) = some 1 := by decide

private def program : Source.Program := ⟨[], []⟩

/-- Test observer stops at a statement exit before consuming its surrounding
continuation. This finite bound is never used by proof admission. -/
private def runStatement : Nat → Source.State → Source.State
  | 0, state => state
  | n+1, state => match state.control, state.continuation with
    | .flow _, [] | .halted _, _ => state
    | _, _ => match Source.step program state with
      | none => state
      | some next => runStatement n next

private def snapshot (outcome : Source.FlowOutcome) : String :=
  reprStr outcome

def statementSelfTest : IO UInt32 := do
  let cases := [initialized, assigned, scope, earlyReturn, failingInitializer,
    statements [.statement (.ite (read 1) (store 1 5) (some (store 1 9))), .statement (ret (read 1))],
    statements [declaration 2 (some (constant 3)), .statement .null],
    statements [declaration 2 (some (constant 3)), .statement .break],
    statements [declaration 2 (some (constant 3)), .statement .continue]]
  let mut failures := 0
  let mut count := 0
  for bits in [0, 1, 7, 0x7fffffff, 0x80000000, 0xffffffff] do
    let locals : LocalStore widths := [⟨1, intType, some (.const 32 (BitVec.ofNat 32 bits))⟩]
    for stmt in cases do
      count := count+1
      let some tree := flowDiagram (Source.statementDepth stmt) locals (.statement stmt)
        | IO.eprintln "statement diagram refused a supported case"; failures := failures+1; continue
      let finalState := runStatement 1000 ⟨.statement stmt, locals.denote σ, [], 0⟩
      let actual : Option Source.FlowOutcome := match finalState.control with
        | .flow flow => some (finalState.locals, .ok flow)
        | .halted (.error fault) => some (finalState.locals, .error fault)
        | _ => none
      if actual.map snapshot != some (snapshot (tree.val.denote σ)) then
        IO.eprintln s!"statement/source-machine mismatch at case {count}"
        failures := failures+1
  if failures == 0 then
    IO.println s!"{count} statement/source-machine comparisons pass"
    return 0
  return 1

end EvoIR.Core.Symbolic.StatementTests
