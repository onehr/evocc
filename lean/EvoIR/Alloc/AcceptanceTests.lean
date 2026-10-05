import EvoIR.Alloc.Reader
import EvoIR.Alloc.Soundness

/-!
Acceptance of the actual allocator output for the frozen pilot population,
and refusal of six classes of changed inputs. The fixtures in
`EvoIR/Alloc/Fixtures/` are the output of

```sh
target/frontend/debug/evo-alloc-export SHA256 .evidence/verification-core/20261004/source-execution/MODULE.evo
```

for the 17 pilot modules. Each mutation below changes one actual export at a
deterministic site. Build this module from the `lean/` directory:
`lake build +EvoIR.Alloc.AcceptanceTests`. A failed expectation stops the
build.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Alloc.AcceptanceTests

def modules : List String := ["doom-fixedmul", "musl-abs", "control", "pressure",
  "source-execution", "call-loop", "call-loop-control", "call-loop-context",
  "call-loop-closure-context", "call-loop-short-right", "call-loop-mixed", "call-loop-nested",
  "call-loop-deep", "call-loop-deep-context", "call-constant-tail", "initializer-self-assignment",
  "source-conversions"]

abbrev Export := Machine.Program × Program × Hints

def load (name : String) : IO Export := do
  let text ← IO.FS.readFile s!"EvoIR/Alloc/Fixtures/{name}.json"
  match Reader.readPair text with
  | .ok result => return result
  | .error message => throw (IO.userError s!"{name}: {message}")

/-! ## Mutation helpers -/

def Loc.mapPlace (f : Place → Place) : Loc → Loc
  | .gpr n v => match f (.gpr n) with | .gpr m => .gpr m v | _ => .gpr n v
  | loc => loc

def Instruction.mapLocs (f : Loc → Loc) : Instruction → Instruction
  | .mov d s => .mov (f d) (f s)
  | .movImm d i => .movImm (f d) i
  | .binary op d l r w => .binary op (f d) (f l) (f r) w
  | .binaryImm op d l i w => .binaryImm op (f d) (f l) i w
  | .flagBinary op d fl l r w => .flagBinary op (f d) (f fl) (f l) (f r) w
  | .wideMultiply op d l r => .wideMultiply op (f d) (f l) (f r)
  | .msub d n m a w => .msub (f d) (f n) (f m) (f a) w
  | .madd d n m a w => .madd (f d) (f n) (f m) (f a) w
  | .extend op d s => .extend op (f d) (f s)
  | .bfx sg d s l b w => .bfx sg (f d) (f s) l b w
  | .cmp d l r w => .cmp (f d) (f l) (f r) w
  | .cmpImm d l i w => .cmpImm (f d) (f l) i w
  | .tstImm d l i w => .tstImm (f d) (f l) i w
  | .cset d fl c => .cset (f d) (f fl) c
  | .csel d fl l r c w => .csel (f d) (f fl) (f l) (f r) c w
  | .load al d b o k v => .load al (f d) (f b) o k v
  | .store al s b o k v => .store al (f s) (f b) o k v
  | .frameSlot d o s a => .frameSlot (f d) o s a
  | .stackRelease a => .stackRelease (f a)
  | .bl sym args res => .bl sym (args.map f) (res.map f)
  | .spillStore slot s => .spillStore slot (f s)
  | .spillReload d slot => .spillReload (f d) slot
  | .incomingArg d slot => .incomingArg (f d) slot
  | .outgoingArg slot s => .outgoingArg slot (f s)

def Terminator.mapLocs (f : Loc → Loc) : Terminator → Terminator
  | .bCond fl c t u => .bCond (f fl) c t u
  | .cb nz s t u => .cb nz (f s) t u
  | .tb nz s b t u => .tb nz (f s) b t u
  | .ret vs => .ret (vs.map f)
  | term => term

def mapBlocks (f : Block → Block) (g : Function) : Function := {g with blocks := g.blocks.map f}

def updateFn (p : Program) (name : String) (f : Function → Function) : Program :=
  ⟨p.functions.map (fun g => if g.name == name then f g else g)⟩

def mapHintPlaces (hints : Hints) (name : String) (f : Place → Place) : Hints :=
  hints.map (fun (n, blocks) => if n == name then
    (n, blocks.map (fun (b, eqs) => (b, eqs.map (fun e => (f e.1, e.2))))) else (n, blocks))

def swapRegs (x y : Nat) : Place → Place
  | .gpr n => if n == x then .gpr y else if n == y then .gpr x else .gpr n
  | p => p

def Instruction.slots : Instruction → List Nat
  | .spillStore s _ | .spillReload _ s => [s]
  | _ => []

def Instruction.renameSlot (old new : Nat) : Instruction → Instruction
  | .spillStore s x => .spillStore (if s == old then new else s) x
  | .spillReload x s => .spillReload x (if s == old then new else s)
  | i => i

def usesLoc (g : Function) (p : Loc → Bool) : Bool :=
  g.blocks.any (fun b => b.insts.any (fun i =>
    (Check.copyOfLocs i ++ i.useLocs ++ i.defLocs).any p))
where
  Check.copyOfLocs (i : Instruction) : List Loc :=
    match i.copyOf with | some (d, s) => [d, s] | none => []

/-- Replace instruction `index` of block `blockId` of function `name`. -/
def setInst (p : Program) (name : String) (blockId index : Nat) (inst : Instruction) : Program :=
  updateFn p name (mapBlocks fun b =>
    if b.id == blockId then {b with insts := b.insts.set index inst} else b)

/-- All (function, block, index, instruction) sites of the allocated program. -/
def sites (p : Program) : List (String × Nat × Nat × Instruction) :=
  p.functions.flatMap fun g => g.blocks.flatMap fun b =>
    (b.insts.zipIdx).map fun (i, k) => (g.name, b.id, k, i)

/-! ## The six mutation classes -/

/-- (1) A reload reads another slot than the one that holds its value. -/
def wrongReloadSlot (e : Export) : Option Export := do
  let (pre, post, hints) := e
  let (name, blockId, k, inst) ← (sites post).find? fun (_, _, _, i) =>
    match i with | .spillReload _ _ => true | _ => false
  let .spillReload dst slot := inst | none
  let g ← post.functions.find? (·.name == name)
  let other ← (g.blocks.flatMap (·.insts.flatMap Instruction.slots)).find? (· != slot)
  return (pre, setInst post name blockId k (.spillReload dst other), hints)

def instWrites (i : Instruction) : List Loc :=
  i.defLocs ++ match i.copyOf with | some (d, _) => [d] | none => []

def instReads (i : Instruction) : List Loc :=
  i.useLocs ++ match i.copyOf with | some (_, s) => [s] | none => []

/-- A callee-saved register that a block reads after a call, with no write
to it between the call and the read. -/
def liveAcrossCall (g : Function) : Option Nat :=
  g.blocks.findSome? fun b =>
    let insts := b.insts.zipIdx
    insts.findSome? fun (call, k) => match call with
      | .bl .. => insts.findSome? fun (use, k') =>
          if k' ≤ k then none else
          (instReads use).findSome? fun l => match l.place with
            | .gpr r =>
              if 19 ≤ r && r ≤ 28 && !(insts.any fun (w, k'') => k < k'' && k'' < k' &&
                  (instWrites w).any (·.place == .gpr r)) then some r else none
            | _ => none
      | _ => none

/-- (2a) A call destroys a live value: a callee-saved register that holds a
value across a call is exchanged with the caller-saved register `x9` in the
whole function and its hints. The exchange is consistent, so only the call
clobber is wrong. -/
def callDestroys (e : Export) : Option Export := do
  let (pre, post, hints) := e
  let (g, r) ← post.functions.findSome? fun g => (liveAcrossCall g).map (g, ·)
  let f := Loc.mapPlace (swapRegs r 9)
  let post := updateFn post g.name (mapBlocks fun b =>
    {b with insts := b.insts.map (Instruction.mapLocs f), term := Terminator.mapLocs f b.term})
  return (pre, post, mapHintPlaces hints g.name (swapRegs r 9))

/-- (2b) A write destroys a live value: the destination of a constant move
becomes a register that the next instruction reads. -/
def writeDestroys (e : Export) : Option Export := do
  let (pre, post, hints) := e
  let site ← (sites post).find? fun (name, blockId, k, inst) =>
    match inst with
    | .movImm (.gpr n _) _ =>
      match (sites post).find? (fun (n', b', k', _) => n' == name && b' == blockId && k' == k+1) with
      | some (_, _, _, next) => next.useLocs.any fun l => match l with
          | .gpr m _ => m != n | _ => false
      | none => false
    | _ => false
  let (name, blockId, k, inst) := site
  let .movImm (.gpr n view) imm := inst | none
  let (_, _, _, next) ← (sites post).find? (fun (n', b', k', _) => n' == name && b' == blockId && k' == k+1)
  let victim ← next.useLocs.findSome? fun l => match l with
    | .gpr m _ => if m != n then some m else none
    | _ => none
  return (pre, setInst post name blockId k (.movImm (.gpr victim view) imm), hints)

/-- (3) Two arguments are exchanged: the moves into `x0` and `x1` before a
call read each other's source. -/
def argumentsExchanged (e : Export) : Option Export := do
  let (pre, post, hints) := e
  let all := sites post
  let (name, blockId, k0, s0, s1, k1, d0, d1) ← all.findSome? fun (name, blockId, k, inst) =>
    match inst with
    | .mov (.gpr 0 v0) s0 =>
      all.findSome? fun (n', b', k', inst') =>
        if n' == name && b' == blockId && k' > k then
          match inst' with
          | .mov (.gpr 1 v1) s1 => if s0.place != s1.place then
              some (name, blockId, k, s0, s1, k', Loc.gpr 0 v0, Loc.gpr 1 v1) else none
          | _ => none
        else none
    | _ => none
  let post := setInst post name blockId k0 (.mov d0 s1)
  return (pre, setInst post name blockId k1 (.mov d1 s0), hints)

/-- (4) An original operation is changed: the first `add` becomes `sub`. -/
def operationChanged (e : Export) : Option Export := do
  let (pre, post, hints) := e
  let (name, blockId, k, inst) ← (sites post).find? fun (_, _, _, i) =>
    match i with | .binary .add _ _ _ _ => true | _ => false
  let .binary _ d l r w := inst | none
  return (pre, setInst post name blockId k (.binary .sub d l r w), hints)

/-- (5) A false loop invariant: the hint of a loop header (a block that a
later block jumps back to) exchanges the registers of two equations. -/
def falseLoopInvariant (e : Export) : Option Export := do
  let (pre, post, hints) := e
  let (name, header, eqs) ← post.functions.findSome? fun g =>
    let ids := g.blocks.map (·.id)
    g.blocks.findSome? fun b =>
      let later := g.blocks.drop ((ids.idxOf b.id) + 1)
      if later.any (fun l => l.term.targets.contains b.id) then
        let eqs := hintOf hints g.name b.id
        match eqs with
        | e1 :: e2 :: _ => if e1.1 != e2.1 && e1.2 != e2.2 then some (g.name, b.id, eqs) else none
        | _ => none
      else none
  let e1 :: e2 :: rest := eqs | none
  let changed := (e1.1, e2.2) :: (e2.1, e1.2) :: rest
  let hints := hints.map fun (n, blocks) => if n == name then
    (n, blocks.map fun (b, old) => if b == header then (b, changed) else (b, old)) else (n, blocks)
  return (pre, post, hints)

/-- (6) A slot is used again while its old value is live: in one block slot
`a` is stored, slot `b` is stored, and slot `a` is reloaded. All uses of `b`
in the function become `a`, so the second store overwrites a live value. -/
def slotReusedWhileLive (e : Export) : Option Export := do
  let (pre, post, hints) := e
  let (name, a, b) ← post.functions.findSome? fun g => g.blocks.findSome? fun blk =>
    let insts := blk.insts.zipIdx
    insts.findSome? fun (i, k) => match i with
      | .spillStore a _ => insts.findSome? fun (j, k') => match j with
        | .spillStore b _ =>
          if k' > k && b != a &&
              (insts.any fun (r, k'') => k'' > k' && match r with
                | .spillReload _ s => s == a | _ => false) &&
              !(insts.any fun (r, k'') => k'' > k && k'' < k' && match r with
                | .spillStore s _ => s == a | _ => false) then some (g.name, a, b) else none
        | _ => none
      | _ => none
  let post := updateFn post name (mapBlocks fun blk =>
    {blk with insts := blk.insts.map (Instruction.renameSlot b a)})
  return (pre, post, hints)

def accepted (e : Export) : Bool := checkAllocation e.1 e.2.1 e.2.2

def mutations : List (String × (Export → Option Export)) := [
  ("wrong reload slot", wrongReloadSlot),
  ("live value destroyed by a call", callDestroys),
  ("live value destroyed by a write", writeDestroys),
  ("two arguments exchanged", argumentsExchanged),
  ("changed original operation", operationChanged),
  ("false loop invariant in the hints", falseLoopInvariant),
  ("slot used again while its old value is live", slotReusedWhileLive)]

/-- Run all expectations. Every module must be accepted. Each mutation class
must apply to at least one actual export, and every mutated export must be
refused. -/
def run : IO Unit := do
  let mut functions := 0
  let mut exports := #[]
  for name in modules do
    let e ← load name
    if !accepted e then throw (IO.userError s!"REFUSED actual export {name}")
    functions := functions + e.1.functions.length
    exports := exports.push (name, e)
  IO.println s!"ACCEPTED modules={modules.length} functions={functions}"
  for (label, mutate) in mutations do
    let mut applied := 0
    let mut refused := 0
    for (_, e) in exports do
      match mutate e with
      | none => pure ()
      | some changed =>
        applied := applied + 1
        if !accepted changed then refused := refused + 1
    IO.println s!"MUTATION {label}: applied={applied} refused={refused}"
    if applied == 0 || refused != applied then
      throw (IO.userError s!"mutation class not refused: {label}")

#eval run

/-! ## Kernel-checked small examples -/

private def r (id : Nat) : Machine.VReg := ⟨id, .gpr32⟩

/-- `twice(x) = x + x` before allocation, with a deleted copy. -/
private def before : Machine.Program := ⟨[⟨"twice", [⟨0, [r 0],
  [.mov (r 1) (r 0), .binary .add (r 2) (r 1) (r 0) .w], .ret [r 2]⟩]⟩]⟩

/-- The allocated form keeps `x` in `x0`, spills it, reloads it into `x9`. -/
private def after (reload : Nat := 0) : Program := ⟨[⟨"twice", [⟨0,
  [.spillStore 0 (.gpr 0 .w), .spillReload (.gpr 9 .w) reload,
   .binary .add (.gpr 0 .w) (.gpr 9 .w) (.gpr 0 .w) .w], .ret [.gpr 0 .w]⟩]⟩]⟩

private def hints : Hints := [("twice", [(0, [(.gpr 0, r 0)])])]

example : checkAllocation before (after) hints = true := by decide
example : checkAllocation before (after 1) hints = false := by decide
example : checkAllocation before (after) [("twice", [(0, [(.gpr 1, r 0)])])] = false := by decide

/-- The accepted example refines its source for every argument vector. -/
theorem twice_refines (type : Core.Source.IntType) (args : List Machine.Value)
    (preEntry : Machine.State) (started : Machine.initial before "twice" args = .ok preEntry)
    (progress : Machine.ObservedProgress .postSSA before type preEntry) :
    ∃ postEntry, initial (after) "twice" args = .ok postEntry ∧
      AllocationRefines before (after) type preEntry postEntry :=
  checkAllocation_sound before (after) hints (by decide) type "twice" args _ preEntry started progress

#print axioms twice_refines

end EvoIR.Alloc.AcceptanceTests
