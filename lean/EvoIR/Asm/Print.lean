import EvoIR.Asm.Syntax

/-!
The printer from the restricted assembly AST to assembly text.

This printer is the only trusted link between the AST and the text that the
compiler writes. The check compares the printed text with the actual text of
the same compilation, byte for byte. A difference is a refusal. There is no
parser of assembly text.

The fixed lines are those of the checked Linux AArch64 profile with all
optional emission variants off: the profile comment, `.text`, `.p2align 2`,
`.type` and `.size` for each function, and the final `.ident` directive.
-/
namespace EvoIR.Asm


def binOpName : Machine.BinOp → String
  | .add => "add" | .sub => "sub" | .mul => "mul" | .sdiv => "sdiv" | .udiv => "udiv"
  | .and => "and" | .orr => "orr" | .eor => "eor" | .lsl => "lsl" | .lsr => "lsr"
  | .asr => "asr"

def flagOpName : Machine.FlagOp → String
  | .adds => "adds" | .subs => "subs"

def wideMulName : Machine.WideMul → String
  | .smull => "smull" | .smulh => "smulh" | .umulh => "umulh"

def extOpName : Machine.ExtOp → String
  | .sxtb => "sxtb" | .sxth => "sxth" | .sxtw => "sxtw" | .uxtb => "uxtb" | .uxth => "uxth"

def condName : Machine.Cond → String
  | .eq => "eq" | .ne => "ne" | .lt => "lt" | .le => "le" | .gt => "gt" | .ge => "ge"
  | .lo => "lo" | .ls => "ls" | .hi => "hi" | .hs => "hs" | .vs => "vs" | .vc => "vc"
  | .cs => "cs" | .cc => "cc"

/-- The register spelling in one view. -/
def GReg.print (width : Machine.Width) : GReg → String
  | .r n => (match width with | .w => "w" | .x => "x") ++ toString n
  | .zr => match width with | .w => "wzr" | .x => "xzr"

def Base.print : Base → String
  | .sp => "sp"
  | .r n => "x" ++ toString n

/-- The local label of one block of the function `function`. -/
def blockLabel (function : String) (block : Nat) : String :=
  ".L" ++ function ++ "_block" ++ toString block

def imm (value : Nat) : String := "#" ++ toString value

def offset (value : Int) : String := "#" ++ toString value

def address (base : Base) (value : Int) : String :=
  "[" ++ base.print ++ ", " ++ offset value ++ "]"

/-- The text of one instruction, without indentation or line end. -/
def Inst.print (function : String) : Inst → String
  | .stpPre first second off => s!"stp x{first}, x{second}, [sp, {offset off}]!"
  | .ldpPost first second off => s!"ldp x{first}, x{second}, [sp], {offset off}"
  | .movFromSp d => s!"mov x{d}, sp"
  | .subSp n => s!"sub sp, sp, {imm n}"
  | .addSp n => s!"add sp, sp, {imm n}"
  | .addFromSp d n => s!"add {d.print .x}, sp, {imm n}"
  | .ldr v t base off => s!"ldr {t.print v}, {address base off}"
  | .str v t base off => s!"str {t.print v}, {address base off}"
  | .mov v d s => s!"mov {d.print v}, {s.print v}"
  | .movz v d n => s!"movz {d.print v}, {imm n}"
  | .movk v d n shift => s!"movk {d.print v}, {imm n}, lsl {imm shift}"
  | .alu op v d n m => s!"{binOpName op} {d.print v}, {n.print v}, {m.print v}"
  | .aluImm op v d n k => s!"{binOpName op} {d.print v}, {n.print v}, {imm k}"
  | .flagAlu op v d n m => s!"{flagOpName op} {d.print v}, {n.print v}, {m.print v}"
  | .wideMul op d n m =>
    let source : Machine.Width := match op with | .smull => .w | .smulh | .umulh => .x
    s!"{wideMulName op} {d.print .x}, {n.print source}, {m.print source}"
  | .msub v d n m a => s!"msub {d.print v}, {n.print v}, {m.print v}, {a.print v}"
  | .madd v d n m a => s!"madd {d.print v}, {n.print v}, {m.print v}, {a.print v}"
  | .extend op v d s => s!"{extOpName op} {d.print v}, {s.print .w}"
  | .bfx signed v d s lsb bits =>
    s!"{if signed then "sbfx" else "ubfx"} {d.print v}, {s.print v}, {imm lsb}, {imm bits}"
  | .cmp v n m => s!"cmp {n.print v}, {m.print v}"
  | .cmpImm v n k => s!"cmp {n.print v}, {imm k}"
  | .tstImm v n k => s!"tst {n.print v}, {imm k}"
  | .cset v d c => s!"cset {d.print v}, {condName c}"
  | .csel v d n m c => s!"csel {d.print v}, {n.print v}, {m.print v}, {condName c}"
  | .bl symbol => "bl " ++ symbol
  | .b k => "b " ++ blockLabel function k
  | .bCond c k => "b." ++ condName c ++ " " ++ blockLabel function k
  | .cb nonzero v r k =>
    s!"{if nonzero then "cbnz" else "cbz"} {r.print v}, {blockLabel function k}"
  | .tb nonzero v r bit k =>
    s!"{if nonzero then "tbnz" else "tbz"} {r.print v}, {imm bit}, {blockLabel function k}"
  | .ret => "ret"
  | .brk n => "brk " ++ imm n

def Line.print (function : String) : Line → String
  | .label k => blockLabel function k ++ ":\n"
  | .inst i => "    " ++ i.print function ++ "\n"

def Function.print (f : Function) : String :=
  "\n" ++ (if f.global then ".globl " ++ f.name ++ "\n" else "") ++
    ".p2align 2\n" ++ ".type " ++ f.name ++ ", %function\n" ++ f.name ++ ":\n" ++
    String.join (f.body.map (Line.print f.name)) ++
    ".size " ++ f.name ++ ", .-" ++ f.name ++ "\n"

/-- The profile comment that the native driver writes before the assembly. -/
def profileComment : String := "// evocc-native-ub-checks=checked\n"

def File.print (file : File) : String :=
  profileComment ++ ".text\n" ++ String.join (file.functions.map Function.print) ++
    "\n.ident \"" ++ file.ident ++ "\"\n"

/-- The print check: the printed AST must be equal to the actual text. -/
def printMatches (file : File) (actual : String) : Bool :=
  file.print == actual

/-- The first line number (counted from one) where the printed text and the
actual text differ. This is a diagnostic only; `printMatches` decides. -/
def firstDifference (file : File) (actual : String) : Option Nat :=
  let printed := (file.print.splitOn "\n").toArray
  let given := (actual.splitOn "\n").toArray
  let limit := max printed.size given.size
  (List.range limit).find? (fun i => printed[i]? != given[i]?) |>.map (· + 1)

end EvoIR.Asm
