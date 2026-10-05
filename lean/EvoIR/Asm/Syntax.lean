import EvoIR.Machine

/-!
The restricted AArch64 assembly AST of the checked core-v1 path.

The constructors are the instruction lines that the emitter writes for the
core-v1 envelope with all optional emission variants off. Each constructor is
one line of the final text. The printer in `EvoIR.Asm.Print` is the only
trusted link between this AST and that text. Unsupported lines have no
constructor. The reader refuses them; it does not approximate them.

The operand types reuse the opcode, width and condition names of
`EvoIR.Machine`. The meaning of each line is defined separately in
`EvoIR.Asm.Semantics`.
-/
namespace EvoIR.Asm


/-- A general register operand: `x0` to `x30` in a 32-bit or 64-bit view,
or the zero register. The reader admits register numbers 0 to 30 only. -/
inductive GReg where
  | r (number : Nat)
  | zr
  deriving DecidableEq, Repr

/-- The base register of a load or store with an immediate offset. -/
inductive Base where
  | sp
  | r (number : Nat)
  deriving DecidableEq, Repr

/-- One instruction line. Branch targets are block identifiers of the
enclosing function. The printer writes them as that function's block labels. -/
inductive Inst where
  /-- `stp xa, xb, [sp, #offset]!` -/
  | stpPre (first second : Nat) (offset : Int)
  /-- `ldp xa, xb, [sp], #offset` -/
  | ldpPost (first second : Nat) (offset : Int)
  /-- `mov xd, sp` -/
  | movFromSp (dst : Nat)
  /-- `sub sp, sp, #imm` -/
  | subSp (imm : Nat)
  /-- `add sp, sp, #imm` -/
  | addSp (imm : Nat)
  /-- `add xd, sp, #imm` -/
  | addFromSp (dst : GReg) (imm : Nat)
  /-- `ldr t, [base, #offset]` -/
  | ldr (width : Machine.Width) (dst : GReg) (base : Base) (offset : Int)
  /-- `str t, [base, #offset]` -/
  | str (width : Machine.Width) (src : GReg) (base : Base) (offset : Int)
  /-- `mov d, s` -/
  | mov (width : Machine.Width) (dst src : GReg)
  /-- `movz d, #imm` -/
  | movz (width : Machine.Width) (dst : GReg) (imm : Nat)
  /-- `movk d, #imm, lsl #shift` -/
  | movk (width : Machine.Width) (dst : GReg) (imm shift : Nat)
  /-- `op d, n, m` -/
  | alu (op : Machine.BinOp) (width : Machine.Width) (dst lhs rhs : GReg)
  /-- `op d, n, #imm` -/
  | aluImm (op : Machine.BinOp) (width : Machine.Width) (dst lhs : GReg) (imm : Nat)
  /-- `adds d, n, m` and `subs d, n, m` -/
  | flagAlu (op : Machine.FlagOp) (width : Machine.Width) (dst lhs rhs : GReg)
  /-- `smull xd, wn, wm`, `smulh xd, xn, xm` and `umulh xd, xn, xm` -/
  | wideMul (op : Machine.WideMul) (dst lhs rhs : GReg)
  /-- `msub d, n, m, a` -/
  | msub (width : Machine.Width) (dst n m a : GReg)
  /-- `madd d, n, m, a` -/
  | madd (width : Machine.Width) (dst n m a : GReg)
  /-- `sxtb d, wn` and the other extensions; `width` is the view of `d`. -/
  | extend (op : Machine.ExtOp) (width : Machine.Width) (dst src : GReg)
  /-- `sbfx d, n, #lsb, #bits` and `ubfx d, n, #lsb, #bits` -/
  | bfx (signed : Bool) (width : Machine.Width) (dst src : GReg) (lsb bits : Nat)
  /-- `cmp n, m` -/
  | cmp (width : Machine.Width) (lhs rhs : GReg)
  /-- `cmp n, #imm` -/
  | cmpImm (width : Machine.Width) (lhs : GReg) (imm : Nat)
  /-- `tst n, #imm` -/
  | tstImm (width : Machine.Width) (lhs : GReg) (imm : Nat)
  /-- `cset d, cond` -/
  | cset (width : Machine.Width) (dst : GReg) (cond : Machine.Cond)
  /-- `csel d, n, m, cond` -/
  | csel (width : Machine.Width) (dst lhs rhs : GReg) (cond : Machine.Cond)
  /-- `bl symbol` -/
  | bl (symbol : String)
  /-- `b label` -/
  | b (target : Nat)
  /-- `b.cond label` -/
  | bCond (cond : Machine.Cond) (target : Nat)
  /-- `cbz r, label` and `cbnz r, label` -/
  | cb (nonzero : Bool) (width : Machine.Width) (src : GReg) (target : Nat)
  /-- `tbz r, #bit, label` and `tbnz r, #bit, label` -/
  | tb (nonzero : Bool) (width : Machine.Width) (src : GReg) (bit target : Nat)
  /-- `ret` -/
  | ret
  /-- `brk #imm` -/
  | brk (imm : Nat)
  deriving DecidableEq, Repr

/-- A line of a function body: a block label or an instruction. -/
inductive Line where
  | label (block : Nat)
  | inst (inst : Inst)
  deriving DecidableEq, Repr

/-- One function definition. `global` selects the `.globl` directive. The
body is the text between the symbol label and the `.size` directive. -/
structure Function where
  name : String
  global : Bool
  body : List Line
  deriving DecidableEq, Repr

/-- The complete assembly file of one compilation. `ident` is the producer
text in the final `.ident` directive. -/
structure File where
  functions : List Function
  ident : String
  deriving DecidableEq, Repr

/-- The untrusted frame layout of one function, as the compiler proposes it.
Offsets are bytes above `sp` after the prologue. The checker in
`EvoIR.Asm.Check` decides whether the layout is consistent; the theorem does
not trust it. -/
structure FrameHint where
  /-- Bytes that the prologue subtracts after the frame record. -/
  size : Nat
  /-- Callee-saved register and its save offset, in save order. -/
  saved : List (Nat × Nat)
  /-- Spill slot and its offset. -/
  spills : List (Nat × Nat)
  /-- Frame object origin and its offset. -/
  slots : List (Nat × Nat)
  /-- Bytes of the outgoing stack argument area at the bottom of the frame. -/
  outgoing : Nat
  deriving DecidableEq, Repr

end EvoIR.Asm
