import EvoIR.Asm.Semantics
import EvoIR.Alloc.Semantics

/-!
The checker of the frame and emission stage: allocated EvoMIR, the assembly
AST and the untrusted frame hints.

`checkAsm` accepts only if all of these hold for each function:

* The frame hint is consistent. The outgoing area, the spill slots, the
  frame objects and the callee-saved save slots are in the frame and do not
  overlap. Each spill slot and each frame object that the function uses has
  exactly one offset. Each callee-saved register that the function writes has
  a save slot.
* The allocated function uses only `x0`-`x15` and `x19`-`x28`. Each
  operand view agrees with the width of its instruction.
* The pointer places of the function are moved only through 64-bit views and
  are used only as load and store bases. No pointer is an argument, a stack
  argument or a result.
* Frame objects are created only in the entry block. No branch goes to the
  entry block. Block identifiers are unique.
* The local stack bound of the function covers its frame and each callee.
* The assembly body is equal to the fixed templates: the prologue, each block
  label, each instruction template, each terminator template with the
  epilogue at each return.

The templates are those of `emit.rs` with all optional variants off. The
scratch registers `x16` and `x17`, the platform register `x18`, the frame
pointer and the link register are not allocatable, so no template operand is
live in them. `checkAsm_sound` in `EvoIR.Asm.Soundness` proves the refinement.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Check

open EvoIR.Alloc

def isCalleeSaved (n : Nat) : Bool := 19 ≤ n && n ≤ 28

def allocatable (n : Nat) : Bool := n ≤ 15 || isCalleeSaved n

/-! Templates. -/

def gprOf : Loc → Option (Nat × Machine.Width)
  | .gpr n v => some (n, v)
  | _ => none

/-- `emit_immediate`: `movz` of the low half, then `movk` of each nonzero
higher half. -/
def immediateLines (v : Machine.Width) (d imm : Nat) : List Inst :=
  .movz v (.r d) (imm % 65536) ::
    (List.range (v.bits / 16 - 1)).filterMap fun i =>
      let shift := 16 * (i + 1)
      let part := imm / 2^shift % 65536
      if part = 0 then none else some (.movk v (.r d) part shift)

def lookupOffset (entries : List (Nat × Nat)) (key : Nat) : Option Nat := entries.lookup key

def loadWidth : Machine.LoadKind → Machine.Width
  | .w => .w | .x => .x

def storeWidth : Machine.StoreKind → Machine.Width
  | .w => .w | .x => .x

/-- The template of one allocated instruction. `none` refuses the form. -/
def instTemplate (hint : FrameHint) : Instruction → Option (List Inst)
  | .mov dst src => do
    let (d, dv) ← gprOf dst
    let (s, sv) ← gprOf src
    return [.mov (if dv = sv then dv else .w) (.r d) (.r s)]
  | .movImm dst imm => do
    let (d, v) ← gprOf dst
    return immediateLines v d imm
  | .binary op dst lhs rhs w => do
    let (d, dv) ← gprOf dst
    let (n, nv) ← gprOf lhs
    let (m, mv) ← gprOf rhs
    if dv = w ∧ nv = w ∧ mv = w then some [.alu op w (.r d) (.r n) (.r m)] else none
  | .binaryImm op dst lhs imm w => do
    let (d, dv) ← gprOf dst
    let (n, nv) ← gprOf lhs
    if dv = w ∧ nv = w then some [.aluImm op w (.r d) (.r n) imm] else none
  | .flagBinary op dst flags lhs rhs w => do
    let (d, dv) ← gprOf dst
    let (n, nv) ← gprOf lhs
    let (m, mv) ← gprOf rhs
    if flags = .flags ∧ dv = w ∧ nv = w ∧ mv = w then some [.flagAlu op w (.r d) (.r n) (.r m)]
    else none
  | .wideMultiply op dst lhs rhs => do
    let (d, dv) ← gprOf dst
    let (n, nv) ← gprOf lhs
    let (m, mv) ← gprOf rhs
    let source : Machine.Width := match op with | .smull => .w | .smulh | .umulh => .x
    if dv = .x ∧ nv = source ∧ mv = source then some [.wideMul op (.r d) (.r n) (.r m)] else none
  | .msub dst n m a w => do
    let (d, dv) ← gprOf dst
    let (n, nv) ← gprOf n
    let (m, mv) ← gprOf m
    let (a, av) ← gprOf a
    if dv = w ∧ nv = w ∧ mv = w ∧ av = w then some [.msub w (.r d) (.r n) (.r m) (.r a)] else none
  | .madd dst n m a w => do
    let (d, dv) ← gprOf dst
    let (n, nv) ← gprOf n
    let (m, mv) ← gprOf m
    let (a, av) ← gprOf a
    if dv = w ∧ nv = w ∧ mv = w ∧ av = w then some [.madd w (.r d) (.r n) (.r m) (.r a)] else none
  | .extend op dst src => do
    let (d, dv) ← gprOf dst
    let (s, _) ← gprOf src
    return [.extend op dv (.r d) (.r s)]
  | .bfx signed dst src lsb bits w => do
    let (d, dv) ← gprOf dst
    let (s, sv) ← gprOf src
    if dv = w ∧ sv = w then some [.bfx signed w (.r d) (.r s) lsb bits] else none
  | .cmp flags lhs rhs w => do
    let (n, nv) ← gprOf lhs
    let (m, mv) ← gprOf rhs
    if flags = .flags ∧ nv = w ∧ mv = w then some [.cmp w (.r n) (.r m)] else none
  | .cmpImm flags lhs imm w => do
    let (n, nv) ← gprOf lhs
    if flags = .flags ∧ nv = w then some [.cmpImm w (.r n) imm] else none
  | .tstImm flags lhs imm w => do
    let (n, nv) ← gprOf lhs
    if flags = .flags ∧ nv = w then some [.tstImm w (.r n) imm] else none
  | .cset dst flags cond => do
    let (d, dv) ← gprOf dst
    if flags = .flags then some [.cset dv (.r d) cond] else none
  | .csel dst flags lhs rhs cond w => do
    let (d, dv) ← gprOf dst
    let (n, nv) ← gprOf lhs
    let (m, mv) ← gprOf rhs
    if flags = .flags ∧ dv = w ∧ nv = w ∧ mv = w then some [.csel w (.r d) (.r n) (.r m) cond]
    else none
  | .load _ dst base offset kind false => do
    let (d, dv) ← gprOf dst
    let (b, bv) ← gprOf base
    if dv = loadWidth kind ∧ bv = .x then some [.ldr dv (.r d) (.r b) offset] else none
  | .store _ src base offset kind false => do
    let (s, sv) ← gprOf src
    let (b, bv) ← gprOf base
    if sv = storeWidth kind ∧ bv = .x then some [.str sv (.r s) (.r b) offset] else none
  | .frameSlot dst origin _ _ => do
    let (d, dv) ← gprOf dst
    let offset ← lookupOffset hint.slots origin
    if dv = .x then some [.addFromSp (.r d) offset] else none
  | .bl symbol _ _ => some [.bl symbol]
  | .spillStore slot src => do
    let (s, sv) ← gprOf src
    let offset ← lookupOffset hint.spills slot
    return [.str sv (.r s) .sp offset]
  | .spillReload dst slot => do
    let (d, dv) ← gprOf dst
    let offset ← lookupOffset hint.spills slot
    return [.ldr dv (.r d) .sp offset]
  | .incomingArg dst slot => do
    let (d, dv) ← gprOf dst
    return [.ldr dv (.r d) (.r 29) (16 + 8 * slot)]
  | .outgoingArg slot src => do
    let (s, sv) ← gprOf src
    return [.str sv (.r s) .sp (8 * slot)]
  | _ => none

def invertCond : Machine.Cond → Machine.Cond
  | .eq => .ne | .ne => .eq | .lt => .ge | .ge => .lt | .le => .gt | .gt => .le
  | .lo => .hs | .hs => .lo | .ls => .hi | .hi => .ls | .vs => .vc | .vc => .vs
  | .cs => .cc | .cc => .cs

def prologue (hint : FrameHint) : List Inst :=
  [.stpPre 29 30 (-16), .movFromSp 29] ++ (if hint.size = 0 then [] else [.subSp hint.size]) ++
    hint.saved.map (fun (r, off) => .str .x (.r r) .sp off)

def epilogue (hint : FrameHint) : List Inst :=
  hint.saved.map (fun (r, off) => .ldr .x (.r r) .sp off) ++
    (if hint.size = 0 then [] else [.addSp hint.size]) ++ [.ldpPost 29 30 16]

def branchTo (next : Option Nat) (target : Nat) : List Inst :=
  if next = some target then [] else [.b target]

/-- The branch layout of a two-way terminator: the taken block falls
through when it is the next block. -/
def twoWay (next : Option Nat) (taken untaken : Nat) : Bool × Nat × Nat :=
  if next = some taken ∧ taken ≠ untaken then (true, untaken, taken) else (false, taken, untaken)

def termTemplate (hint : FrameHint) (next : Option Nat) : Terminator → Option (List Inst)
  | .b target => some (branchTo next target)
  | .bCond flags cond taken untaken =>
    let (invert, t, u) := twoWay next taken untaken
    if flags = .flags then some (.bCond (if invert then invertCond cond else cond) t :: branchTo next u)
    else none
  | .cb nonzero src taken untaken => do
    let (s, sv) ← gprOf src
    let (invert, t, u) := twoWay next taken untaken
    return .cb (nonzero ^^ invert) sv (.r s) t :: branchTo next u
  | .tb nonzero src bit taken untaken => do
    let (s, sv) ← gprOf src
    let (invert, t, u) := twoWay next taken untaken
    return .tb (nonzero ^^ invert) sv (.r s) bit t :: branchTo next u
  | .ret _ => some (epilogue hint ++ [.ret])
  | .brk imm => some [.brk imm]

def blockLines (hint : FrameHint) (next : Option Nat) (b : Alloc.Block) : Option (List Line) := do
  let insts ← b.insts.mapM (instTemplate hint)
  let term ← termTemplate hint next b.term
  return .label b.id :: (insts.flatten ++ term).map .inst

def blocksLines (hint : FrameHint) : List Alloc.Block → Option (List Line)
  | [] => some []
  | b :: rest => do
    let here ← blockLines hint (rest.head?.map (·.id)) b
    let tail ← blocksLines hint rest
    return here ++ tail

/-- The expected assembly body of one allocated function. -/
def expectedBody (hint : FrameHint) (f : Alloc.Function) : Option (List Line) := do
  let blocks ← blocksLines hint f.blocks
  return (prologue hint).map .inst ++ blocks

/-! Locations and places that a function uses. -/

def instLocs : Instruction → List Loc
  | .mov dst src | .extend _ dst src => [dst, src]
  | .movImm dst _ | .frameSlot dst _ _ _ => [dst]
  | .binary _ dst lhs rhs _ | .wideMultiply _ dst lhs rhs | .cmp dst lhs rhs _ => [dst, lhs, rhs]
  | .binaryImm _ dst lhs _ _ | .cmpImm dst lhs _ _ | .tstImm dst lhs _ _ => [dst, lhs]
  | .flagBinary _ dst flags lhs rhs _ => [dst, flags, lhs, rhs]
  | .msub dst n m a _ | .madd dst n m a _ => [dst, n, m, a]
  | .bfx _ dst src _ _ _ => [dst, src]
  | .cset dst flags _ => [dst, flags]
  | .csel dst flags lhs rhs _ _ => [dst, flags, lhs, rhs]
  | .load _ dst base _ _ _ => [dst, base]
  | .store _ src base _ _ _ => [src, base]
  | .stackRelease address => [address]
  | .bl _ args results => args ++ results
  | .spillStore slot src => [.spill slot src.width, src]
  | .spillReload dst slot => [dst, .spill slot dst.width]
  | .incomingArg dst slot => [dst, .incoming slot dst.width]
  | .outgoingArg slot src => [.outgoing slot src.width, src]

/-- The locations that an instruction writes. A call writes its results;
the call-clobbered registers are not values of the caller after the call. -/
def instWritten : Instruction → List Loc
  | .mov dst _ | .movImm dst _ | .binary _ dst _ _ _ | .binaryImm _ dst _ _ _
  | .wideMultiply _ dst _ _ | .msub dst _ _ _ _ | .madd dst _ _ _ _ | .extend _ dst _
  | .bfx _ dst _ _ _ _ | .cset dst _ _ | .csel dst _ _ _ _ _ | .load _ dst _ _ _ _
  | .frameSlot dst _ _ _ | .spillReload dst _ | .incomingArg dst _ => [dst]
  | .flagBinary _ dst flags _ _ _ => [dst, flags]
  | .cmp flags _ _ _ | .cmpImm flags _ _ _ | .tstImm flags _ _ _ => [flags]
  | .spillStore slot src => [.spill slot src.width]
  | .outgoingArg slot src => [.outgoing slot src.width]
  | .bl _ _ results => results
  | .store _ _ _ _ _ _ | .stackRelease _ => []

def termLocs : Terminator → List Loc
  | .b _ | .brk _ => []
  | .bCond flags _ _ _ => [flags]
  | .cb _ src _ _ | .tb _ src _ _ _ => [src]
  | .ret values => values

def instructionsOf (f : Alloc.Function) : List Instruction := f.blocks.flatMap (·.insts)

def locsOf (f : Alloc.Function) : List Loc :=
  (instructionsOf f).flatMap instLocs ++ f.blocks.flatMap (fun b => termLocs b.term)

/-! Pointer places. A pointer comes only from `frameSlot` and moves only
through `mov`, spill stores and spill reloads. The set is closed under these
moves; it is computed by iteration and then checked to be closed. -/

/-- The moves of an instruction: (destination, source). -/
def instMove : Instruction → Option (Loc × Loc)
  | .mov dst src => some (dst, src)
  | .spillStore slot src => some (.spill slot src.width, src)
  | .spillReload dst slot => some (dst, .spill slot dst.width)
  | .incomingArg dst slot => some (dst, .incoming slot dst.width)
  | .outgoingArg slot src => some (.outgoing slot src.width, src)
  | _ => none

def pointerSeeds (f : Alloc.Function) : List Place :=
  (instructionsOf f).filterMap fun
    | .frameSlot dst _ _ _ => some dst.place
    | _ => none

def closeOnce (f : Alloc.Function) (places : List Place) : List Place :=
  places ++ (instructionsOf f).filterMap fun inst =>
    match instMove inst with
    | some (dst, src) => if src.place ∈ places ∧ dst.place ∉ places then some dst.place else none
    | none => none

def pointerPlaces (f : Alloc.Function) : List Place :=
  (List.range ((instructionsOf f).length + 1)).foldl (fun places _ => closeOnce f places) (pointerSeeds f)

/-- The checked closure property: the seeds are in the set, and each move
from a place in the set goes to a place in the set. -/
def pointerClosed (f : Alloc.Function) (places : List Place) : Bool :=
  (pointerSeeds f).all (· ∈ places) &&
    (instructionsOf f).all fun inst =>
      match instMove inst with
      | some (dst, src) => !(src.place ∈ places) || dst.place ∈ places
      | none => true

/-- The uses of the pointer places that the checker admits. Instructions
that read their operands as integer bits are not restricted: a pointer
operand makes the allocated step malformed. -/
def pointerUses (places : List Place) (inst : Instruction) : Bool :=
  let ptr (loc : Loc) : Bool := loc.place ∈ places
  match inst with
  | .mov dst src => !(ptr src) || (src.width == .x && dst.width == .x)
  | .spillStore _ src => !(ptr src) || src.width == .x
  | .spillReload dst slot => !((Place.spill slot) ∈ places) || dst.width == .x
  | .incomingArg _ slot => !((Place.incoming slot) ∈ places)
  | .outgoingArg _ src => !(ptr src)
  | .frameSlot dst _ _ _ => dst.width == .x
  | .bl _ args _ => args.all (fun a => !(ptr a))
  | .binary _ _ lhs rhs _ => !(ptr lhs) && !(ptr rhs)
  | .binaryImm _ _ lhs _ _ => !(ptr lhs)
  | .cmp _ lhs rhs _ => !(ptr lhs) && !(ptr rhs)
  | .cmpImm _ lhs _ _ => !(ptr lhs)
  | .csel _ _ lhs rhs _ _ => !(ptr lhs) && !(ptr rhs)
  | _ => true

def pointerSafe (f : Alloc.Function) : Bool :=
  let places := pointerPlaces f
  pointerClosed f places && (instructionsOf f).all (pointerUses places) &&
    f.blocks.all fun b => match b.term with
      | .ret values => values.all (fun v => !(v.place ∈ places))
      | _ => true

/-! Frame layout. -/

def regionsDisjoint : List (Nat × Nat) → Bool
  | [] => true
  | (a, n) :: rest => rest.all (fun (b, k) => a + n ≤ b || b + k ≤ a) && regionsDisjoint rest

def keysUnique : List Nat → Bool
  | [] => true
  | k :: rest => !(k ∈ rest) && keysUnique rest

/-- The frame objects of a function: origin and size. -/
def frameObjects (f : Alloc.Function) : List (Nat × Nat) :=
  (instructionsOf f).filterMap fun
    | .frameSlot _ origin size _ => some (origin, size)
    | _ => none

def spillSlotsUsed (f : Alloc.Function) : List Nat :=
  (instructionsOf f).filterMap fun
    | .spillStore slot _ | .spillReload _ slot => some slot
    | _ => none

def outgoingSlotsUsed (f : Alloc.Function) : List Nat :=
  (instructionsOf f).filterMap fun
    | .outgoingArg slot _ => some slot
    | _ => none

/-- The callee-saved registers that the function writes. -/
def calleeSavedWritten (f : Alloc.Function) : List Nat :=
  (instructionsOf f).flatMap (fun inst => (instWritten inst).filterMap fun
    | .gpr n _ => if isCalleeSaved n then some n else none
    | _ => none)

def frameRegions (f : Alloc.Function) (hint : FrameHint) : List (Nat × Nat) :=
  (if hint.outgoing = 0 then [] else [(0, hint.outgoing)]) ++
    hint.spills.map (fun (_, off) => (off, 8)) ++
    (frameObjects f).filterMap (fun (origin, size) =>
      (lookupOffset hint.slots origin).map (fun off => (off, size))) ++
    hint.saved.map (fun (_, off) => (off, 8))

def frameOk (f : Alloc.Function) (hint : FrameHint) : Bool :=
  hint.size % 16 = 0 && hint.size < 4096 &&
  (frameRegions f hint).all (fun (off, n) => 0 < n && off + n ≤ hint.size) &&
  regionsDisjoint (frameRegions f hint) &&
  keysUnique (hint.spills.map Prod.fst) && keysUnique (hint.saved.map Prod.fst) &&
  keysUnique (hint.slots.map Prod.fst) && keysUnique ((frameObjects f).map Prod.fst) &&
  hint.saved.all (fun (r, _) => isCalleeSaved r) &&
  (calleeSavedWritten f).all (fun r => r ∈ hint.saved.map Prod.fst) &&
  (spillSlotsUsed f).all (fun k => (lookupOffset hint.spills k).isSome) &&
  (frameObjects f).all (fun (origin, _) => (lookupOffset hint.slots origin).isSome) &&
  (outgoingSlotsUsed f).all (fun k => 8 * k + 8 ≤ hint.outgoing)

/-! Function structure. -/

def registersOk (f : Alloc.Function) : Bool :=
  (locsOf f).all fun
    | .gpr n _ => allocatable n
    | _ => true

def entryOk (f : Alloc.Function) : Bool :=
  (f.blocks.head?.map (·.id)) = some 0 &&
  keysUnique (f.blocks.map (·.id)) &&
  f.blocks.all (fun b => !(0 ∈ b.term.targets)) &&
  (f.blocks.drop 1).all (fun b => b.insts.all fun
    | .frameSlot _ _ _ _ => false
    | _ => true) &&
  (instructionsOf f).all (fun
    | .stackRelease _ => false
    | _ => true)

/-! Stack bound. `need` is the number of bytes below the entry stack pointer
that the function and its deepest callee chain use. -/

def callees (f : Alloc.Function) : List String :=
  (instructionsOf f).filterMap fun
    | .bl symbol _ _ => some symbol
    | _ => none

def frameBytes (hint : FrameHint) : Nat := hint.size + 16

/-- A proposed bound, computed with fuel. The checker does not trust it: it
checks the local inequality for each call. -/
def stackNeed (p : Program) (hints : List FrameHint) : Nat → String → Nat
  | 0, _ => 0
  | fuel+1, name =>
    match p.functions.findIdx? (·.name == name) with
    | none => 0
    | some i =>
      let hint := hints[i]?.getD ⟨0, [], [], [], 0⟩
      let deepest := ((p.functions[i]?.map callees).getD []).foldl
        (fun acc g => max acc (stackNeed p hints fuel g)) 0
      frameBytes hint + deepest

def needOf (p : Program) (hints : List FrameHint) (name : String) : Nat :=
  stackNeed p hints (p.functions.length + 1) name

def stackOk (p : Program) (hints : List FrameHint) (i : Nat) (f : Alloc.Function) (hint : FrameHint) :
    Bool :=
  let _ := i
  (callees f).all fun g =>
    (p.functions.any (·.name == g)) &&
      frameBytes hint + needOf p hints g ≤ needOf p hints f.name

/-! The checker. -/

def namesUnique (p : Program) : Bool := keysUnique' (p.functions.map (·.name))
where
  keysUnique' : List String → Bool
    | [] => true
    | k :: rest => !(k ∈ rest) && keysUnique' rest

def checkFunction (p : Program) (hints : List FrameHint) (i : Nat) (f : Alloc.Function)
    (hint : FrameHint) (g : Asm.Function) : Bool :=
  g.name == f.name &&
  expectedBody hint f == some g.body &&
  frameOk f hint && registersOk f && pointerSafe f && entryOk f &&
  stackOk p hints i f hint &&
  frameBytes hint ≤ needOf p hints f.name

def checkAsm (p : Program) (asm : File) (hints : List FrameHint) : Bool :=
  namesUnique p &&
  p.functions.length == asm.functions.length &&
  p.functions.length == hints.length &&
  p.functions.length < 2^30 &&
  (List.range p.functions.length).all fun i =>
    match p.functions[i]?, hints[i]?, asm.functions[i]? with
    | some f, some hint, some g => checkFunction p hints i f hint g &&
        g.body.length < functionSpan
    | _, _, _ => false

end EvoIR.Asm.Check
