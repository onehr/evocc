import EvoIR.Core.SourceExpression
import EvoIR.Core.IRReader

/-! Source-AST/target-segment regressions. These exercise the general expression
checker, not an exact-pair whitelist or whole-function acceptance. -/
namespace EvoIR.Core.Symbolic.ExpressionTests

private def widths (name : Var) : Nat := if name = "count" then 64 else 32
private def a : CWord widths := ⟨.int, .var "a" 32 rfl⟩
private def b : CWord widths := ⟨.int, .var "b" 32 rfl⟩
private def count : CWord widths := ⟨.long, .var "count" 64 rfl⟩
private def env : CBindings widths := [(1, a), (2, b), (3, count)]
private def scope : Scope widths := [(0, .word a.word), (1, .word b.word), (20, .word count.word)]
private def sum : Source.Expr := .binary .int .add (.ident .int 1) (.ident .int 2)
private def add : List IR.Instruction := [
  ⟨[⟨2, .bits 1⟩], .scalar (.guard .add 32), [0, 1]⟩,
  ⟨[], .assume .signedOverflow, [2]⟩,
  ⟨[⟨3, .bits 32⟩], .scalar (.bin .add 32), [0, 1]⟩]
private def shifted : Source.Expr := .binary .int .shr sum (.ident .long 3)
private def shiftCode (signed : Bool := true) (op : ShOp := .ashr) : List IR.Instruction := [
  ⟨[⟨4, .bits 1⟩], .scalar (.count signed 32 64), [3, 20]⟩,
  ⟨[], .assume .invalidShiftCount, [4]⟩,
  ⟨[⟨5, .bits 32⟩], .scalar (.shift op 32 64), [3, 20]⟩]

example : checkExpression env sum scope add 3 = true := by decide
example : checkExpression env shifted scope (add ++ shiftCode) 5 = true := by decide
example : checkExpression env shifted scope (add ++ shiftCode false) 5 = false := by decide
example : checkExpression env shifted scope (add ++ shiftCode true .lshr) 5 = false := by decide
example : checkExpression env shifted scope (add ++ shiftCode true .shl) 5 = false := by decide

-- The bits result alone is identical; moving a reached shift diagnostic ahead
-- of a reached arithmetic diagnostic is nevertheless a different program.
private def reordered : List IR.Instruction := [
  ⟨[⟨3, .bits 32⟩], .scalar (.bin .add 32), [0, 1]⟩,
  ⟨[⟨4, .bits 1⟩], .scalar (.count true 32 64), [3, 20]⟩,
  ⟨[], .assume .invalidShiftCount, [4]⟩,
  ⟨[⟨2, .bits 1⟩], .scalar (.guard .add 32), [0, 1]⟩,
  ⟨[], .assume .signedOverflow, [2]⟩,
  ⟨[⟨5, .bits 32⟩], .scalar (.shift .ashr 32 64), [3, 20]⟩]
example : checkExpression env shifted scope reordered 5 = false := by decide
example : checkExpression env sum scope (add.drop 1) 3 = false := by decide
example : checkExpression env sum scope (add.take 1 ++ add.drop 2) 3 = false := by decide
example : checkExpression env (.binary .int .sub (.ident .int 1) (.ident .int 2))
    scope add 3 = false := by decide
example : checkExpression env (.binary .long .add (.ident .int 1) (.ident .int 2))
    scope add 3 = false := by decide
example : checkExpression env (.ident .long 1) scope [] 0 = false := by decide
example : checkExpression env (.ident .int 99) scope [] 0 = false := by decide
example : checkExpression env sum [(0, .opaque 0), (1, .word b.word)] add 3 = false := by decide
example : checkExpression env (.call .int 8 []) scope [] 0 = false := by decide
example : checkExpression env (.assign .int .set 1 ⟨.int, false⟩ (.ident .int 1) (.literal .int 1))
    scope [] 0 = false := by decide
example : (expression env (.binary .int .shl (.ident .int 1) (.literal .int 1))).isNone := by decide
example : (expression env (.literal .int (2^32))).isNone := by decide
example : (expression env (.cast .int ⟨.long, false⟩ (.ident .int 1))).isNone := by decide

-- Only syntax-classified literal overflow receives the source wrapping rule.
private def wrapped : Source.Expr :=
  .binary .int .add (.literal .int 0x7fffffff) (.literal .int 1)
private def constantCode : List IR.Instruction := [⟨[⟨4, .bits 32⟩], .constant 32 0x80000000, []⟩]
example : checkExpression env wrapped scope constantCode 4 = true := by decide
example : checkExpression env sum scope constantCode 4 = false := by decide

private def valuation (x y : BitVec 32) (k : BitVec 64) : Subst widths :=
  fun name => if h : name = "count" then by simpa [widths, h] using k
    else by simpa [widths, h] using (if name = "a" then x else y)

example : (expression env shifted).map (fun r => r.val.denote
    (valuation 0x7fffffff 1 64)) = some (.error (.diagnostic 3004)) := by decide
example : (expression env shifted).map (fun r => r.val.denote
    (valuation 0 1 32)) = some (.error (.diagnostic 3005)) := by decide
example : (expression env shifted).map (fun r => r.val.denote
    (valuation 0 1 0xffffffffffffffff)) = some (.error (.diagnostic 3005)) := by decide
example : (expression env shifted).map (fun r => r.val.denote
    (valuation 0 1 0x100000001)) = some (.error (.diagnostic 3005)) := by decide
example : (expression env shifted).map (fun r => r.val.denote
    (valuation 0xffffffff 0 31)) = some (.ok (.ofNat .int 0xffffffff)) := by decide

-- This quantifies every operand and continuation, rather than testing a
-- finite selection of values or treating interpreter agreement as a proof.
example (σ : Subst widths) (p : Source.Program) (locals : Source.Locals) (fid : Nat)
    (related : env.Related σ locals) :
    ∃ result, Source.Evaluates p locals fid shifted result ∧
      (do let values ← concreteInstructions 1 (scope.denote σ (fun _ => .world)) (add ++ shiftCode)
          [5].mapM (IR.lookup values)) = result.map (fun v => [sourceValueIR v]) :=
  expression_fragment_sound σ (fun _ => .world) 1 env shifted scope (add ++ shiftCode) 5
    p locals fid related (by decide)

-- Actual frozen exporter text, decoded with the existing readers. The return
-- destination contributes its real implicit conversion to the expression
-- obligation; this test does NOT certify the surrounding function/CFG.
private def fixedSource := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[2],[["function",2,"FixedMul",["int",false],false,[["parameter",3,["int",false],"none"],["parameter",4,["int",false],"none"]],["compound",[["return",["int",false],["binary","long","shr",["binary","long","mul",["cast","long",["long",false],["ident","int",3]],["cast","long",["long",false],["ident","int",4]]],["lit","int","00000010"]]]]]]]]"#
private def fixedTarget := "func @FixedMul(%arg0: bits<32>, %arg1: bits<32>) -> (bits<32>) cc = target_default\n" ++
  "effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }\n{\n" ++
  "^entry(%v0: bits<32>, %v1: bits<32>):\n%v2 = sext.bits<64> %v0\n%v3 = sext.bits<64> %v1\n" ++
  "%v4 = signed_mul_defined %v2, %v3\nub.assume.signed_overflow %v4\n%v5 = mul.bits<64> %v2, %v3\n" ++
  "%v6 = const.bits<32> 16\n%v7 = shift_count_defined.signed %v5, %v6\nub.assume.invalid_shift_count %v7\n" ++
  "%v8 = ashr.bits<64> %v5, %v6\n%v9 = trunc.bits<32> %v8\nret %v9\n}\n"

private def fixedExpressionCheck (s t : String) : Bool :=
  match Source.readProgram s, IR.Reader.readProgram t with
  | .ok source, .ok target => Id.run do
    let some sf := source.functions.head? | return false
    let .compound [.statement (.return destination expr)] := sf.body | return false
    let some tf := target.functions.head? | return false
    let [block] := tf.blocks | return false
    let .return [output] := block.terminator | return false
    return checkExpression [(3, a), (4, b)] (.cast destination.type destination expr)
      [(0, .word a.word), (1, .word b.word)] block.instructions output
  | _, _ => false

def sourceExpressionSelfTest : IO UInt32 := do
  let mut failures := 0
  if !fixedExpressionCheck fixedSource fixedTarget then
    IO.eprintln "original FixedMul source/IR expression refused"
    failures := failures+1
  let mutations := [
    ("signed extension", "sext.bits<64>", "zext.bits<64>"),
    ("shift sign", "ashr.bits<64>", "lshr.bits<64>"),
    ("count sign", "shift_count_defined.signed", "shift_count_defined.unsigned"),
    ("count value", "const.bits<32> 16", "const.bits<32> 15"),
    ("missing overflow", "ub.assume.signed_overflow %v4\n", ""),
    ("missing shift check", "ub.assume.invalid_shift_count %v7\n", ""),
    ("diagnostic", "ub.assume.invalid_shift_count", "ub.assume.signed_overflow"),
    ("operator", "mul.bits<64>", "add.bits<64>"),
    ("count width", "const.bits<32> 16", "const.bits<64> 16"),
    ("value operand", "mul.bits<64> %v2, %v3", "mul.bits<64> %v2, %v2")]
  for (name, before, after) in mutations do
    let changed := fixedTarget.replace before after
    if changed == fixedTarget || fixedExpressionCheck fixedSource changed then
      IO.eprintln s!"source/IR expression mutation not refused: {name}"
      failures := failures+1
  let sourceMutations := [
    ("source count", "00000010", "0000000f"),
    ("source operation", "\"mul\"", "\"add\""),
    ("source operand", "\"ident\",\"int\",3", "\"ident\",\"int\",4"),
    ("source shift", "\"shr\"", "\"shl\""),
    ("source signedness", "\"long\"", "\"ulong\"")]
  for (name, before, after) in sourceMutations do
    let changed := fixedSource.replace before after
    if changed == fixedSource || fixedExpressionCheck changed fixedTarget then
      IO.eprintln s!"source expression mutation not refused: {name}"
      failures := failures+1
  -- A different source/target pair is accepted by the same structural code.
  -- No exact FixedMul tree or named-function whitelist is used by the checker.
  if !fixedExpressionCheck (fixedSource.replace "00000010" "0000000f")
      (fixedTarget.replace "const.bits<32> 16" "const.bits<32> 15") then
    IO.eprintln "general expression checker refused a consistent changed pair"
    failures := failures+1
  if failures == 0 then
    IO.println s!"{mutations.length+sourceMutations.length+2} original-expression cases pass (component only; whole-function admission incomplete)"
    return 0
  return 1

end EvoIR.Core.Symbolic.ExpressionTests
