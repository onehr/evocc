import EvoIR.Asm.Syntax
import Lean.Data.Json

/-!
Reader of the exported assembly AST and frame hints.

The input is the JSON form that `evo-asm-ast` writes. The reader checks the
wire grammar and representation bounds only. It does not read assembly text.
A decoded AST has no authority: the print check compares it with the actual
text, and the checker compares it with the allocated program.

Grammar:

```text
["core-asm-v1", "aarch64-unknown-linux-gnu", "checked", ident, functions]
function = [name, global, lines, frame]
frame = [size, [[register, offset], ...], [[slot, offset], ...],
         [[origin, offset], ...], outgoing]
line = ["label", block] | instruction
register operand = 0..30 | "zr"
base = "sp" | 0..30
```
-/
namespace EvoIR.Asm.Reader

open Lean (Json)

private def text (j : Json) : Option String := j.getStr?.toOption
private def array (j : Json) : Option (List Json) := j.getArr?.toOption.map Array.toList
private def boolean (j : Json) : Option Bool := j.getBool?.toOption

private def natural (bits : Nat) (j : Json) : Option Nat := do
  let n ← j.getNat?.toOption
  if n < 2^bits then some n else none

private def integer (j : Json) : Option Int := do
  let n ← j.getInt?.toOption
  if -(2^32 : Int) ≤ n ∧ n < 2^32 then some n else none

private def registerNumber (j : Json) : Option Nat := do
  let n ← j.getNat?.toOption
  if n ≤ 30 then some n else none

private def greg (j : Json) : Option GReg :=
  match text j with
  | some "zr" => some .zr
  | some _ => none
  | none => return .r (← registerNumber j)

private def base (j : Json) : Option Base :=
  match text j with
  | some "sp" => some .sp
  | some _ => none
  | none => return .r (← registerNumber j)

private def width (j : Json) : Option Machine.Width := do
  match ← text j with
  | "w" => some .w | "x" => some .x | _ => none

private def binOp (j : Json) : Option Machine.BinOp := do
  match ← text j with
  | "add" => some .add | "sub" => some .sub | "mul" => some .mul
  | "sdiv" => some .sdiv | "udiv" => some .udiv
  | "and" => some .and | "orr" => some .orr | "eor" => some .eor
  | "lsl" => some .lsl | "lsr" => some .lsr | "asr" => some .asr
  | _ => none

private def flagOp (j : Json) : Option Machine.FlagOp := do
  match ← text j with
  | "adds" => some .adds | "subs" => some .subs | _ => none

private def wideMul (j : Json) : Option Machine.WideMul := do
  match ← text j with
  | "smull" => some .smull | "smulh" => some .smulh | "umulh" => some .umulh
  | _ => none

private def extOp (j : Json) : Option Machine.ExtOp := do
  match ← text j with
  | "sxtb" => some .sxtb | "sxth" => some .sxth | "sxtw" => some .sxtw
  | "uxtb" => some .uxtb | "uxth" => some .uxth | _ => none

private def cond (j : Json) : Option Machine.Cond := do
  match ← text j with
  | "eq" => some .eq | "ne" => some .ne | "lt" => some .lt | "le" => some .le
  | "gt" => some .gt | "ge" => some .ge | "lo" => some .lo | "ls" => some .ls
  | "hi" => some .hi | "hs" => some .hs | "vs" => some .vs | "vc" => some .vc
  | "cs" => some .cs | "cc" => some .cc | _ => none

def instruction (j : Json) : Option Inst := do
  let tag :: fields ← array j | none
  match ← text tag, fields with
  | "stp_pre", [a, b, off] => return .stpPre (← registerNumber a) (← registerNumber b) (← integer off)
  | "ldp_post", [a, b, off] => return .ldpPost (← registerNumber a) (← registerNumber b) (← integer off)
  | "mov_from_sp", [d] => return .movFromSp (← registerNumber d)
  | "sub_sp", [n] => return .subSp (← natural 12 n)
  | "add_sp", [n] => return .addSp (← natural 12 n)
  | "add_from_sp", [d, n] => return .addFromSp (← greg d) (← natural 12 n)
  | "ldr", [w, t, b, off] => return .ldr (← width w) (← greg t) (← base b) (← integer off)
  | "str", [w, t, b, off] => return .str (← width w) (← greg t) (← base b) (← integer off)
  | "mov", [w, d, s] => return .mov (← width w) (← greg d) (← greg s)
  | "movz", [w, d, n] => return .movz (← width w) (← greg d) (← natural 16 n)
  | "movk", [w, d, n, shift] => return .movk (← width w) (← greg d) (← natural 16 n) (← natural 6 shift)
  | "alu", [op, w, d, n, m] => return .alu (← binOp op) (← width w) (← greg d) (← greg n) (← greg m)
  | "alu_imm", [op, w, d, n, k] =>
    return .aluImm (← binOp op) (← width w) (← greg d) (← greg n) (← natural 64 k)
  | "flag_alu", [op, w, d, n, m] =>
    return .flagAlu (← flagOp op) (← width w) (← greg d) (← greg n) (← greg m)
  | "wide_mul", [op, d, n, m] => return .wideMul (← wideMul op) (← greg d) (← greg n) (← greg m)
  | "msub", [w, d, n, m, a] => return .msub (← width w) (← greg d) (← greg n) (← greg m) (← greg a)
  | "madd", [w, d, n, m, a] => return .madd (← width w) (← greg d) (← greg n) (← greg m) (← greg a)
  | "extend", [op, w, d, s] => return .extend (← extOp op) (← width w) (← greg d) (← greg s)
  | "bfx", [signed, w, d, s, lsb, bits] =>
    return .bfx (← boolean signed) (← width w) (← greg d) (← greg s) (← natural 6 lsb) (← natural 7 bits)
  | "cmp", [w, n, m] => return .cmp (← width w) (← greg n) (← greg m)
  | "cmp_imm", [w, n, k] => return .cmpImm (← width w) (← greg n) (← natural 64 k)
  | "tst_imm", [w, n, k] => return .tstImm (← width w) (← greg n) (← natural 64 k)
  | "cset", [w, d, c] => return .cset (← width w) (← greg d) (← cond c)
  | "csel", [w, d, n, m, c] => return .csel (← width w) (← greg d) (← greg n) (← greg m) (← cond c)
  | "bl", [symbol] => return .bl (← text symbol)
  | "b", [k] => return .b (← natural 32 k)
  | "b_cond", [c, k] => return .bCond (← cond c) (← natural 32 k)
  | "cb", [nonzero, w, r, k] => return .cb (← boolean nonzero) (← width w) (← greg r) (← natural 32 k)
  | "tb", [nonzero, w, r, bit, k] =>
    return .tb (← boolean nonzero) (← width w) (← greg r) (← natural 6 bit) (← natural 32 k)
  | "ret", [] => return .ret
  | "brk", [n] => return .brk (← natural 16 n)
  | _, _ => none

def line (j : Json) : Option Line := do
  match array j with
  | some [tag, k] =>
    if text tag == some "label" then return .label (← natural 32 k) else .inst <$> instruction j
  | _ => .inst <$> instruction j

/-- A symbol name must be a C identifier. The printer writes it unchanged. -/
def symbolName (name : String) : Bool :=
  match name.toList with
  | [] => false
  | first :: rest =>
    (first.isAlpha || first == '_') && rest.all (fun c => c.isAlphanum || c == '_')

/-- The producer text of `.ident` may not contain a quote, a backslash or a
control character. -/
def identText (value : String) : Bool :=
  value.toList.all (fun c => c != '"' && c != '\\' && 32 ≤ c.toNat && c.toNat < 127)

private def pair (j : Json) : Option (Nat × Nat) := do
  match ← array j with
  | [a, b] => return (← natural 32 a, ← natural 32 b)
  | _ => none

private def pairs (j : Json) : Option (List (Nat × Nat)) := do (← array j).mapM pair

def frame (j : Json) : Option FrameHint := do
  match ← array j with
  | [size, saved, spills, slots, outgoing] =>
    return ⟨← natural 32 size, ← pairs saved, ← pairs spills, ← pairs slots, ← natural 32 outgoing⟩
  | _ => none

def function (j : Json) : Option (Function × FrameHint) := do
  match ← array j with
  | [name, global, lines, hint] =>
    let name ← text name
    if !symbolName name then none
    return (⟨name, ← boolean global, ← (← array lines).mapM line⟩, ← frame hint)
  | _ => none

def file (j : Json) : Option (File × List FrameHint) := do
  match ← array j with
  | [tag, target, profile, ident, functions] =>
    if (← text tag) != "core-asm-v1" || (← text target) != "aarch64-unknown-linux-gnu" ||
        (← text profile) != "checked" then none
    let ident ← text ident
    if !identText ident then none
    let decoded ← (← array functions).mapM function
    return (⟨decoded.map Prod.fst, ident⟩, decoded.map Prod.snd)
  | _ => none

def readFile (input : String) : Except String (File × List FrameHint) := do
  let parsed ← Json.parse input
  match file parsed with
  | some result => .ok result
  | none => .error "unsupported or malformed core assembly AST"

end EvoIR.Asm.Reader
