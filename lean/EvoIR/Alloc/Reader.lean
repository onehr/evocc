import EvoIR.Alloc.Check
import EvoIR.MachineReader

/-!
Independent decoding of the allocation export of `evo-alloc-export`. The
first part is a `core-mir-v1` post-SSA program, read by
`Machine.Reader.program`. The second part is the allocated program and the
proposed block states. The reader checks the wire grammar and the bounds
only. It does not infer, drop or reorder instructions. A decoded export is
not a checked allocation.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Alloc.Reader

open Lean (Json)
open EvoIR.Machine (VReg RegClass Width)

private def text (j : Json) : Option String := j.getStr?.toOption
private def array (j : Json) : Option (List Json) := j.getArr?.toOption.map Array.toList
private def boolean (j : Json) : Option Bool := j.getBool?.toOption

private def natural (bits : Nat) (j : Json) : Option Nat := do
  let n ← j.getNat?.toOption
  if n < 2^bits then some n else none

private def offset (j : Json) : Option Int := do
  let n ← j.getInt?.toOption
  if -(2^63 : Int) ≤ n ∧ n < 2^63 then some n else none

private def width (j : Json) : Option Width := do
  match ← text j with
  | "w" => some .w | "x" => some .x | _ => none

private def regClass (j : Json) : Option RegClass := do
  match ← text j with
  | "gpr32" => some .gpr32 | "gpr64" => some .gpr64 | "flags" => some .flags | _ => none

private def reg (j : Json) : Option VReg := do
  match ← array j with
  | [id, cls] => return ⟨← natural 32 id, ← regClass cls⟩
  | _ => none

/-- General registers are `x0`-`x30`. -/
def loc (j : Json) : Option Loc := do
  let tag :: fields ← array j | none
  match ← text tag, fields with
  | "gpr", [n, v] => do
    let n ← natural 8 n
    if n < 31 then return .gpr n (← width v) else none
  | "flags", [] => return .flags
  | "spill", [s, v] => return .spill (← natural 32 s) (← width v)
  | "incoming", [s, v] => return .incoming (← natural 32 s) (← width v)
  | "outgoing", [s, v] => return .outgoing (← natural 32 s) (← width v)
  | _, _ => none

private def locs (j : Json) : Option (List Loc) := do (← array j).mapM loc

def place (j : Json) : Option Place := do
  let tag :: fields ← array j | none
  match ← text tag, fields with
  | "gpr", [n] => do
    let n ← natural 8 n
    if n < 31 then return .gpr n else none
  | "flags", [] => return .flags
  | "spill", [s] => return .spill (← natural 32 s)
  | "incoming", [s] => return .incoming (← natural 32 s)
  | "outgoing", [s] => return .outgoing (← natural 32 s)
  | _, _ => none

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

private def loadKind (j : Json) : Option Machine.LoadKind := do
  match ← text j with
  | "w" => some .w | "x" => some .x | _ => none

private def storeKind (j : Json) : Option Machine.StoreKind := do
  match ← text j with
  | "w" => some .w | "x" => some .x | _ => none

def instruction (j : Json) : Option Instruction := do
  let tag :: fields ← array j | none
  match ← text tag, fields with
  | "mov", [dst, src] => return .mov (← loc dst) (← loc src)
  | "mov_imm", [dst, imm] => return .movImm (← loc dst) (← natural 64 imm)
  | "binary", [op, dst, lhs, rhs, w] =>
    return .binary (← binOp op) (← loc dst) (← loc lhs) (← loc rhs) (← width w)
  | "binary_imm", [op, dst, lhs, imm, w] =>
    return .binaryImm (← binOp op) (← loc dst) (← loc lhs) (← natural 64 imm) (← width w)
  | "flag_binary", [op, dst, flags, lhs, rhs, w] =>
    return .flagBinary (← flagOp op) (← loc dst) (← loc flags) (← loc lhs) (← loc rhs) (← width w)
  | "wide_multiply", [op, dst, lhs, rhs] =>
    return .wideMultiply (← wideMul op) (← loc dst) (← loc lhs) (← loc rhs)
  | "msub", [dst, n, m, a, w] => return .msub (← loc dst) (← loc n) (← loc m) (← loc a) (← width w)
  | "madd", [dst, n, m, a, w] => return .madd (← loc dst) (← loc n) (← loc m) (← loc a) (← width w)
  | "extend", [op, dst, src] => return .extend (← extOp op) (← loc dst) (← loc src)
  | "bfx", [signed, dst, src, lsb, bits, w] =>
    return .bfx (← boolean signed) (← loc dst) (← loc src) (← natural 16 lsb) (← natural 16 bits)
      (← width w)
  | "cmp", [dst, lhs, rhs, w] => return .cmp (← loc dst) (← loc lhs) (← loc rhs) (← width w)
  | "cmp_imm", [dst, lhs, imm, w] =>
    return .cmpImm (← loc dst) (← loc lhs) (← natural 64 imm) (← width w)
  | "tst_imm", [dst, lhs, imm, w] =>
    return .tstImm (← loc dst) (← loc lhs) (← natural 64 imm) (← width w)
  | "cset", [dst, flags, c] => return .cset (← loc dst) (← loc flags) (← cond c)
  | "csel", [dst, flags, lhs, rhs, c, w] =>
    return .csel (← loc dst) (← loc flags) (← loc lhs) (← loc rhs) (← cond c) (← width w)
  | "load", [align, dst, base, off, kind, volatile] =>
    return .load (← natural 32 align) (← loc dst) (← loc base) (← offset off) (← loadKind kind)
      (← boolean volatile)
  | "store", [align, src, base, off, kind, volatile] =>
    return .store (← natural 32 align) (← loc src) (← loc base) (← offset off) (← storeKind kind)
      (← boolean volatile)
  | "frame_slot", [dst, origin, size, align] =>
    return .frameSlot (← loc dst) (← natural 64 origin) (← natural 64 size) (← natural 32 align)
  | "stack_release", [address] => return .stackRelease (← loc address)
  | "bl", [symbol, args, results] => return .bl (← text symbol) (← locs args) (← locs results)
  | "spill_store", [slot, src] => return .spillStore (← natural 32 slot) (← loc src)
  | "spill_reload", [dst, slot] => return .spillReload (← loc dst) (← natural 32 slot)
  | "incoming_arg", [dst, slot] => return .incomingArg (← loc dst) (← natural 32 slot)
  | "outgoing_arg", [slot, src] => return .outgoingArg (← natural 32 slot) (← loc src)
  | _, _ => none

def terminator (j : Json) : Option Terminator := do
  let tag :: fields ← array j | none
  match ← text tag, fields with
  | "b", [target] => return .b (← natural 32 target)
  | "b_cond", [flags, c, yes, no] =>
    return .bCond (← loc flags) (← cond c) (← natural 32 yes) (← natural 32 no)
  | "cb", [nonzero, src, yes, no] =>
    return .cb (← boolean nonzero) (← loc src) (← natural 32 yes) (← natural 32 no)
  | "tb", [nonzero, src, bit, yes, no] =>
    return .tb (← boolean nonzero) (← loc src) (← natural 16 bit) (← natural 32 yes)
      (← natural 32 no)
  | "ret", [values] => return .ret (← locs values)
  | "brk", [imm] => return .brk (← natural 16 imm)
  | _, _ => none

def block (j : Json) : Option Block := do
  match ← array j with
  | [id, insts, term] =>
    return ⟨← natural 32 id, ← (← array insts).mapM instruction, ← terminator term⟩
  | _ => none

private def equation (j : Json) : Option (Place × VReg) := do
  match ← array j with
  | [p, r] => return (← place p, ← reg r)
  | _ => none

private def blockHint (j : Json) : Option (Nat × Equations) := do
  match ← array j with
  | [id, equations] => return (← natural 32 id, ← (← array equations).mapM equation)
  | _ => none

def function (j : Json) : Option (Function × List (Nat × Equations)) := do
  match ← array j with
  | [name, blocks, hints] =>
    return (⟨← text name, ← (← array blocks).mapM block⟩, ← (← array hints).mapM blockHint)
  | _ => none

def allocated (j : Json) : Option (Program × Hints) := do
  match ← array j with
  | [tag, target, profile, functions] =>
    if (← text tag) != "core-alloc-v1" || (← text target) != "aarch64-unknown-linux-gnu" ||
        (← text profile) != "checked" then none else do
      let functions ← (← array functions).mapM function
      return (⟨functions.map (·.1)⟩, functions.map (fun f => (f.1.name, f.2)))
  | _ => none

/-- The three parts of one export: the post-SSA program, the allocated program
and the proposed hints. -/
def readPair (input : String) : Except String (Machine.Program × Program × Hints) := do
  let parsed ← Json.parse input
  match array parsed with
  | some [tag, pre, post] =>
    if text tag != some "core-alloc-pair-v1" then throw "unknown allocation export tag"
    let some (mode, pre) := Machine.Reader.program pre | throw "malformed post-SSA program"
    if mode != .postSSA then throw "the program before allocation is not post-SSA"
    let some (post, hints) := allocated post | throw "malformed allocated program"
    return (pre, post, hints)
  | _ => throw "malformed allocation export"

end EvoIR.Alloc.Reader
