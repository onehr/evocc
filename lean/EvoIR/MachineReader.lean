import EvoIR.Machine
import Lean.Data.Json

/-!
Independent decoding of actual machine snapshots. This reader does not infer
flags from instruction order, erase register classes, prune code, or replace
unknown instructions by no-ops. It checks wire grammar and representation
bounds only. A decoded snapshot is not a proved compilation result.
-/
namespace EvoIR.Machine.Reader

open Lean (Json)

private def text (j : Json) : Option String := j.getStr?.toOption
private def array (j : Json) : Option (List Json) := j.getArr?.toOption.map Array.toList
private def boolean (j : Json) : Option Bool := j.getBool?.toOption

private def natural (bits : Nat) (j : Json) : Option Nat := do
  let n ← j.getNat?.toOption
  if n < 2^bits then some n else none

private def offset (j : Json) : Option Int := do
  let n ← j.getInt?.toOption
  if -(2^63 : Int) ≤ n ∧ n < 2^63 then some n else none

private def regClass (j : Json) : Option RegClass := do
  match ← text j with
  | "gpr32" => some .gpr32
  | "gpr64" => some .gpr64
  | "flags" => some .flags
  | _ => none

private def reg (j : Json) : Option VReg := do
  match ← array j with
  | [id, cls] => return ⟨← natural 32 id, ← regClass cls⟩
  | _ => none

private def regs (j : Json) : Option (List VReg) := do (← array j).mapM reg

private def width (j : Json) : Option Width := do
  match ← text j with
  | "w" => some .w | "x" => some .x | _ => none

private def binOp (j : Json) : Option BinOp := do
  match ← text j with
  | "add" => some .add | "sub" => some .sub | "mul" => some .mul
  | "sdiv" => some .sdiv | "udiv" => some .udiv
  | "and" => some .and | "orr" => some .orr | "eor" => some .eor
  | "lsl" => some .lsl | "lsr" => some .lsr | "asr" => some .asr
  | _ => none

private def flagOp (j : Json) : Option FlagOp := do
  match ← text j with
  | "adds" => some .adds | "subs" => some .subs | _ => none

private def wideMul (j : Json) : Option WideMul := do
  match ← text j with
  | "smull" => some .smull | "smulh" => some .smulh | "umulh" => some .umulh
  | _ => none

private def extOp (j : Json) : Option ExtOp := do
  match ← text j with
  | "sxtb" => some .sxtb | "sxth" => some .sxth | "sxtw" => some .sxtw
  | "uxtb" => some .uxtb | "uxth" => some .uxth | _ => none

private def cond (j : Json) : Option Cond := do
  match ← text j with
  | "eq" => some .eq | "ne" => some .ne | "lt" => some .lt | "le" => some .le
  | "gt" => some .gt | "ge" => some .ge | "lo" => some .lo | "ls" => some .ls
  | "hi" => some .hi | "hs" => some .hs | "vs" => some .vs | "vc" => some .vc
  | "cs" => some .cs | "cc" => some .cc | _ => none

private def loadKind (j : Json) : Option LoadKind := do
  match ← text j with
  | "w" => some .w | "x" => some .x | _ => none

private def storeKind (j : Json) : Option StoreKind := do
  match ← text j with
  | "w" => some .w | "x" => some .x | _ => none

def instruction (j : Json) : Option Instruction := do
  let tag :: fields ← array j | none
  match ← text tag, fields with
  | "mov", [dst, src] => return .mov (← reg dst) (← reg src)
  | "mov_imm", [dst, imm] => return .movImm (← reg dst) (← natural 64 imm)
  | "binary", [op, dst, lhs, rhs, w] =>
    return .binary (← binOp op) (← reg dst) (← reg lhs) (← reg rhs) (← width w)
  | "binary_imm", [op, dst, lhs, imm, w] =>
    return .binaryImm (← binOp op) (← reg dst) (← reg lhs) (← natural 64 imm) (← width w)
  | "flag_binary", [op, dst, flags, lhs, rhs, w] =>
    return .flagBinary (← flagOp op) (← reg dst) (← reg flags)
      (← reg lhs) (← reg rhs) (← width w)
  | "wide_multiply", [op, dst, lhs, rhs] =>
    return .wideMultiply (← wideMul op) (← reg dst) (← reg lhs) (← reg rhs)
  | "msub", [dst, n, m, a, w] =>
    return .msub (← reg dst) (← reg n) (← reg m) (← reg a) (← width w)
  | "madd", [dst, n, m, a, w] =>
    return .madd (← reg dst) (← reg n) (← reg m) (← reg a) (← width w)
  | "extend", [op, dst, src] => return .extend (← extOp op) (← reg dst) (← reg src)
  | "bfx", [signed, dst, src, lsb, bits, w] =>
    return .bfx (← boolean signed) (← reg dst) (← reg src)
      (← natural 16 lsb) (← natural 16 bits) (← width w)
  | "cmp", [dst, lhs, rhs, w] => return .cmp (← reg dst) (← reg lhs) (← reg rhs) (← width w)
  | "cmp_imm", [dst, lhs, imm, w] =>
    return .cmpImm (← reg dst) (← reg lhs) (← natural 64 imm) (← width w)
  | "tst_imm", [dst, lhs, imm, w] =>
    return .tstImm (← reg dst) (← reg lhs) (← natural 64 imm) (← width w)
  | "cset", [dst, flags, c] => return .cset (← reg dst) (← reg flags) (← cond c)
  | "csel", [dst, flags, lhs, rhs, c, w] =>
    return .csel (← reg dst) (← reg flags) (← reg lhs) (← reg rhs) (← cond c) (← width w)
  | "load", [align, dst, base, off, kind, volatile] =>
    return .load (← natural 32 align) (← reg dst) (← reg base) (← offset off)
      (← loadKind kind) (← boolean volatile)
  | "store", [align, src, base, off, kind, volatile] =>
    return .store (← natural 32 align) (← reg src) (← reg base) (← offset off)
      (← storeKind kind) (← boolean volatile)
  | "frame_slot", [dst, origin, size, align] =>
    return .frameSlot (← reg dst) (← natural 64 origin) (← natural 64 size) (← natural 32 align)
  | "stack_release", [address] => return .stackRelease (← reg address)
  | "bl", [symbol, args, results, Json.null] => return .bl (← text symbol) (← regs args) (← regs results)
  | _, _ => none

def terminator (j : Json) : Option Terminator := do
  let tag :: fields ← array j | none
  match ← text tag, fields with
  | "b", [target, args] => return .b (← natural 32 target) (← regs args)
  | "b_cond", [flags, c, yes, yesArgs, no, noArgs] =>
    return .bCond (← reg flags) (← cond c) (← natural 32 yes) (← regs yesArgs)
      (← natural 32 no) (← regs noArgs)
  | "cb", [nonzero, src, yes, yesArgs, no, noArgs] =>
    return .cb (← boolean nonzero) (← reg src) (← natural 32 yes) (← regs yesArgs)
      (← natural 32 no) (← regs noArgs)
  | "tb", [nonzero, src, bit, yes, yesArgs, no, noArgs] =>
    return .tb (← boolean nonzero) (← reg src) (← natural 16 bit)
      (← natural 32 yes) (← regs yesArgs) (← natural 32 no) (← regs noArgs)
  | "ret", [values] => return .ret (← regs values)
  | "brk", [imm] => return .brk (← natural 16 imm)
  | _, _ => none

def block (j : Json) : Option Block := do
  match ← array j with
  | [id, args, insts, term] => return ⟨← natural 32 id, ← regs args,
      ← (← array insts).mapM instruction, ← terminator term⟩
  | _ => none

def function (j : Json) : Option Function := do
  match ← array j with
  | [name, blocks] => return ⟨← text name, ← (← array blocks).mapM block⟩
  | _ => none

def program (j : Json) : Option (Mode × Program) := do
  match ← array j with
  | [tag, target, profile, mode, functions] =>
    if (← text tag) != "core-mir-v1" || (← text target) != "aarch64-unknown-linux-gnu" ||
        (← text profile) != "checked" then none else do
      let mode ← match ← text mode with
        | "ssa" => some Mode.ssa | "post-ssa" => some Mode.postSSA | _ => none
      return (mode, ⟨← (← array functions).mapM function⟩)
  | _ => none

def readProgram (input : String) : Except String (Mode × Program) := do
  let parsed ← Json.parse input
  match program parsed with
  | some result => .ok result
  | none => .error "unsupported or malformed Core machine snapshot"

end EvoIR.Machine.Reader
