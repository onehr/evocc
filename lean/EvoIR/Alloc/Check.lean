import EvoIR.Alloc.Semantics

/-!
The allocation checker. It takes the program before allocation (post-SSA
EvoMIR), the allocated program and untrusted hints: one abstract state for
each block entry. An abstract state is a list of equations. The equation
`(place, reg)` says that `place` holds the current value of the virtual
register `reg` of the program before allocation.

The checker walks the two instruction lists of each block in lockstep:

* An inserted move, spill store, spill reload or stack argument access of
  the allocated program copies the equations of its source place.
* A same-class move of the program before allocation renames: the
  destination register joins each place of the source register. This covers
  moves that the allocator deleted.
* Each other original instruction must be the same operation with the same
  immediates. Each used register must be in the location that the allocated
  instruction reads. The definitions replace the equations of their places.
* A call clears the call-clobbered places. The arguments and the results
  must be in the ABI locations.

At each edge, the hint of the target block must be a subset of the state.
The entry hint must be a subset of the ABI entry state. No step trusts the
hints; the soundness theorem is in `Alloc/Soundness.lean`.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Alloc

open EvoIR.Machine (VReg RegClass Width)

abbrev Equations := List (Place × VReg)

/-- Hints: for each function name, one state for each block identity. -/
abbrev Hints := List (String × List (Nat × Equations))

def hintOf (hints : Hints) (function : String) (block : Nat) : Equations :=
  ((hints.lookup function).bind (·.lookup block)).getD []

def Loc.fits (loc : Loc) (regClass : RegClass) : Bool := loc.place.at regClass == some loc

def kill (E : Equations) (place : Place) : Equations := E.filter (fun e => e.1 != place)

def forget (E : Equations) (reg : VReg) : Equations := E.filter (fun e => e.2.id != reg.id)

/-- `loc` holds the value of `reg` and reads it through the view of its class. -/
def holds (E : Equations) (loc : Loc) (reg : VReg) : Bool :=
  E.contains (loc.place, reg) && loc.fits reg.class

def define (E : Equations) (loc : Loc) (reg : VReg) : Equations :=
  (loc.place, reg) :: forget (kill E loc.place) reg

/-- The source of an inserted copy holds at least one general value. -/
def sourceDefined (E : Equations) (src : Loc) : Bool :=
  E.any (fun e => e.1 == src.place && src.fits e.2.class && e.2.class != .flags)

def copy (E : Equations) (dst src : Loc) : Equations :=
  E.filterMap (fun e =>
    if e.1 == src.place && src.fits e.2.class && dst.fits e.2.class then some (dst.place, e.2)
    else none) ++ kill E dst.place

def rename (E : Equations) (dst src : VReg) : Equations :=
  let E := forget E dst
  E.filterMap (fun e => if e.2 == src then some (e.1, dst) else none) ++ E

def clobber (E : Equations) : Equations := E.filter (fun e => !e.1.callClobbered)

/-- The destination and source of an inserted copy. -/
def Instruction.copyOf : Instruction → Option (Loc × Loc)
  | .mov dst src => some (dst, src)
  | .spillStore slot src => some (.spill slot src.width, src)
  | .spillReload dst slot => some (dst, .spill slot dst.width)
  | .incomingArg dst slot => some (dst, .incoming slot dst.width)
  | .outgoingArg slot src => some (.outgoing slot src.width, src)
  | _ => none

/-! The operand positions of an original form, in the order of
`Machine.Instruction.uses` and `Machine.Instruction.defs`. -/

def Instruction.useLocs : Instruction → List Loc
  | .mov _ src | .extend _ _ src | .bfx _ _ src _ _ _ => [src]
  | .movImm _ _ | .frameSlot _ _ _ _ => []
  | .binary _ _ lhs rhs _ | .flagBinary _ _ _ lhs rhs _
  | .wideMultiply _ _ lhs rhs | .cmp _ lhs rhs _ => [lhs, rhs]
  | .binaryImm _ _ lhs _ _ | .cmpImm _ lhs _ _ | .tstImm _ lhs _ _ => [lhs]
  | .msub _ n m a _ | .madd _ n m a _ => [n, m, a]
  | .cset _ flags _ => [flags]
  | .csel _ flags lhs rhs _ _ => [flags, lhs, rhs]
  | .load _ _ base _ _ _ => [base]
  | .store _ src base _ _ _ => [src, base]
  | .stackRelease address => [address]
  | .bl _ args _ => args
  | .spillStore _ _ | .spillReload _ _ | .incomingArg _ _ | .outgoingArg _ _ => []

def Instruction.defLocs : Instruction → List Loc
  | .mov dst _ | .movImm dst _ | .binary _ dst _ _ _
  | .binaryImm _ dst _ _ _ | .wideMultiply _ dst _ _
  | .msub dst _ _ _ _ | .madd dst _ _ _ _ | .extend _ dst _
  | .bfx _ dst _ _ _ _ | .cmp dst _ _ _ | .cmpImm dst _ _ _
  | .tstImm dst _ _ _ | .cset dst _ _ | .csel dst _ _ _ _ _
  | .load _ dst _ _ _ _ | .frameSlot dst _ _ _ => [dst]
  | .flagBinary _ dst flags _ _ _ => [dst, flags]
  | .store _ _ _ _ _ _ | .stackRelease _ => []
  | .bl _ _ results => results
  | .spillStore _ _ | .spillReload _ _ | .incomingArg _ _ | .outgoingArg _ _ => []

/-- The allocated form of `inst` with use locations `u` and definition
locations `d`. -/
def ofMachine (u d : VReg → Loc) : Machine.Instruction → Instruction
  | .mov dst src => .mov (d dst) (u src)
  | .movImm dst imm => .movImm (d dst) imm
  | .binary op dst lhs rhs width => .binary op (d dst) (u lhs) (u rhs) width
  | .binaryImm op dst lhs imm width => .binaryImm op (d dst) (u lhs) imm width
  | .flagBinary op dst flags lhs rhs width => .flagBinary op (d dst) (d flags) (u lhs) (u rhs) width
  | .wideMultiply op dst lhs rhs => .wideMultiply op (d dst) (u lhs) (u rhs)
  | .msub dst n m a width => .msub (d dst) (u n) (u m) (u a) width
  | .madd dst n m a width => .madd (d dst) (u n) (u m) (u a) width
  | .extend op dst src => .extend op (d dst) (u src)
  | .bfx signed dst src lsb bits width => .bfx signed (d dst) (u src) lsb bits width
  | .cmp dst lhs rhs width => .cmp (d dst) (u lhs) (u rhs) width
  | .cmpImm dst lhs imm width => .cmpImm (d dst) (u lhs) imm width
  | .tstImm dst lhs imm width => .tstImm (d dst) (u lhs) imm width
  | .cset dst flags cond => .cset (d dst) (u flags) cond
  | .csel dst flags lhs rhs cond width => .csel (d dst) (u flags) (u lhs) (u rhs) cond width
  | .load align dst base offset kind volatile => .load align (d dst) (u base) offset kind volatile
  | .store align src base offset kind volatile => .store align (u src) (u base) offset kind volatile
  | .frameSlot dst origin size align => .frameSlot (d dst) origin size align
  | .stackRelease address => .stackRelease (u address)
  | .bl symbol args results => .bl symbol (args.map u) (results.map d)

/-- The location paired with `reg`. -/
def assign (regs : List VReg) (locs : List Loc) (reg : VReg) : Loc :=
  ((regs.zip locs).lookup reg).getD .flags

/-- The definition positions that receive flags, and all other definitions
receive values. This is a class condition on the program before allocation. -/
def defClassesOK : Machine.Instruction → Bool
  | .cmp dst _ _ _ | .cmpImm dst _ _ _ | .tstImm dst _ _ _ => dst.class == .flags
  | .flagBinary _ dst flags _ _ _ => dst.class != .flags && flags.class == .flags
  | inst => inst.defs.all (·.class != .flags)

/-- The identities are distinct, in the form of the check of `Machine.bind`. -/
def distinctIds (regs : List VReg) : Bool := (regs.map VReg.id).eraseDups.length == regs.length

/-- Apply the definitions in their order. -/
def defineAll (E : Equations) : List (VReg × Loc) → Equations
  | [] => E
  | (reg, loc) :: rest => defineAll (define E loc reg) rest

/-- An original instruction that is not a call, together with its allocated
form. The result is the state after the instruction. -/
def matchOriginal (E : Equations) (o : Machine.Instruction) (a : Instruction) : Option Equations :=
  let U := assign o.uses a.useLocs
  let D := assign o.defs a.defLocs
  if a == ofMachine U D o && o.uses.all (fun r => holds E (U r) r) &&
      o.defs.all (fun r => (D r).fits r.class) && defClassesOK o then
    some (defineAll E (o.defs.map (fun r => (r, D r))))
  else none

/-- The ABI place of argument `index`: a general register, then the outgoing
(at a call) or incoming (at entry) stack argument slots. -/
def argPlace (outgoing : Bool) (index : Nat) : Place :=
  if index < abiRegisterCount then .gpr index
  else if outgoing then .outgoing (index - abiRegisterCount)
  else .incoming (index - abiRegisterCount)

/-- General register `index` through the view of the class of `reg`. -/
def regLoc (index : Nat) (reg : VReg) : Loc := ((Place.gpr index).at reg.class).getD .flags

/-- `place` holds `reg` and has a view for its class. -/
def heldAt (E : Equations) (place : Place) (reg : VReg) : Bool :=
  E.contains (place, reg) && (place.at reg.class).isSome

/-- The ABI entry state for the parameters of a function. -/
def entryState (params : List VReg) : Equations :=
  params.zipIdx.map (fun e => (argPlace false e.2, e.1))

/-- The ABI result registers of a call or a return. -/
def resultRegs (results : List VReg) : List Loc := results.zipIdx.map (fun e => regLoc e.2 e.1)

def preParams (p : Machine.Program) (name : String) : Option (List VReg) := do
  let f ← p.functions.find? (·.name == name)
  let block ← f.blocks.find? (·.id == 0)
  return block.args

/-- Every return of `name` in the allocated program uses the result views. -/
def returnViews (post : Program) (name : String) (views : List Width) : Bool :=
  match post.functions.find? (·.name == name) with
  | some f => f.blocks.all (fun block => match block.term with
      | .ret locs => locs.map Loc.width == views
      | _ => true)
  | none => false

/-- The state after a call. -/
def callExit (E : Equations) (results : List VReg) (locs : List Loc) : Equations :=
  (locs.zip results).map (fun e => (e.1.place, e.2)) ++
    (clobber E).filter (fun e => !(results.map (·.id)).contains e.2.id)

def matchCall (pre : Machine.Program) (post : Program) (E : Equations)
    (symbol : String) (args results : List VReg)
    (symbol' : String) (aargs aresults : List Loc) : Option Equations :=
  match preParams pre symbol with
  | none => none
  | some params =>
    if symbol == symbol' && params.length == args.length &&
        (args.zip params).all (fun e => e.1.class == e.2.class) &&
        args.zipIdx.all (fun e => heldAt E (argPlace true e.2) e.1) &&
        aargs == resultRegs (args.take abiRegisterCount) &&
        decide (results.length ≤ abiRegisterCount) && results.all (·.class != .flags) &&
        aresults == resultRegs results && distinctIds results &&
        returnViews post symbol (aresults.map Loc.width) then
      some (callExit E results aresults)
    else none

/-- A move before allocation that only renames: both classes are equal. -/
def renameOf : Machine.Instruction → Option (VReg × VReg)
  | .mov d s => if d.class == s.class then some (d, s) else none
  | _ => none

/-- An allocated move that implements a class-changing move before allocation. -/
def classChange (E : Equations) (a : Instruction) :
    List Machine.Instruction → Option (List Machine.Instruction × Equations)
  | .mov d s :: os =>
    match a with
    | .mov ld ls =>
      if d.class != s.class && d.class != .flags && holds E ls s && ld.fits d.class then
        some (os, define E ld d)
      else none
    | _ => none
  | _ => none

/-- An original instruction and its allocated form. -/
def matchStep (pre : Machine.Program) (post : Program) (E : Equations) :
    Machine.Instruction → Instruction → Option Equations
  | .bl symbol args results, .bl symbol' aargs aresults =>
    matchCall pre post E symbol args results symbol' aargs aresults
  | .bl _ _ _, _ => none
  | o, a => matchOriginal E o a

/-- One action of the walk. -/
inductive Next where
  | done (E : Equations)
  | step (os : List Machine.Instruction) (as : List Instruction) (E : Equations)
  | fail

/-- The next action of the walk. Inserted copies come first. A rename of the
program before allocation comes before a matched instruction. -/
def next (pre : Machine.Program) (post : Program) (os : List Machine.Instruction) :
    List Instruction → Equations → Next
  | a :: as, E =>
    match a.copyOf with
    | some (dst, src) =>
      match classChange E a os with
      | some (os', E') => .step os' as E'
      | none => if sourceDefined E src then .step os as (copy E dst src) else .fail
    | none =>
      match os with
      | o :: os' =>
        match renameOf o with
        | some (d, s) => .step os' (a :: as) (rename E d s)
        | none =>
          match matchStep pre post E o a with
          | some E' => .step os' as E'
          | none => .fail
      | [] => .fail
  | [], E =>
    match os with
    | [] => .done E
    | o :: os' =>
      match renameOf o with
      | some (d, s) => .step os' [] (rename E d s)
      | none => .fail

/-- The lockstep walk of one block body. `fuel` bounds the number of actions;
the initial fuel is the sum of the two lengths plus one. -/
def walk (pre : Machine.Program) (post : Program) :
    Nat → List Machine.Instruction → List Instruction → Equations → Option Equations
  | 0, _, _, _ => none
  | fuel+1, os, as, E =>
    match next pre post os as E with
    | .done E => some E
    | .step os as E => walk pre post fuel os as E
    | .fail => none

def hasBlock (blocks : List Machine.Block) (blocks' : List Block) (id : Nat) : Bool :=
  (blocks.find? (·.id == id)).isSome && (blocks'.find? (·.id == id)).isSome

def edgeOK (hint : Nat → Equations) (blocks : List Machine.Block) (blocks' : List Block)
    (X : Equations) (target : Nat) : Bool :=
  hasBlock blocks blocks' target && (hint target).all X.contains

def termOK (hint : Nat → Equations) (blocks : List Machine.Block) (blocks' : List Block)
    (X : Equations) : Machine.Terminator → Terminator → Bool
  | .b target args, .b target' =>
    args.isEmpty && target == target' && edgeOK hint blocks blocks' X target
  | .bCond flags cond taken ta untaken ua, .bCond loc cond' taken' untaken' =>
    ta.isEmpty && ua.isEmpty && cond == cond' && taken == taken' && untaken == untaken' &&
      holds X loc flags && edgeOK hint blocks blocks' X taken && edgeOK hint blocks blocks' X untaken
  | .cb nonzero src taken ta untaken ua, .cb nonzero' loc taken' untaken' =>
    ta.isEmpty && ua.isEmpty && nonzero == nonzero' && taken == taken' && untaken == untaken' &&
      holds X loc src && edgeOK hint blocks blocks' X taken && edgeOK hint blocks blocks' X untaken
  | .tb nonzero src bit taken ta untaken ua, .tb nonzero' loc bit' taken' untaken' =>
    ta.isEmpty && ua.isEmpty && nonzero == nonzero' && bit == bit' && taken == taken' &&
      untaken == untaken' && holds X loc src &&
      edgeOK hint blocks blocks' X taken && edgeOK hint blocks blocks' X untaken
  | .ret values, .ret locs =>
    abiRegisters locs && values.length == locs.length &&
      (values.zip locs).all (fun e => holds X e.2 e.1)
  | .brk imm, .brk imm' => imm == imm'
  | _, _ => false

def checkBlock (pre : Machine.Program) (post : Program) (hint : Nat → Equations)
    (f : Machine.Function) (g : Function) (block : Machine.Block) : Bool :=
  match g.blocks.find? (·.id == block.id) with
  | none => false
  | some block' =>
    match walk pre post (block.insts.length + block'.insts.length + 1) block.insts block'.insts
        (hint block.id) with
    | some X => termOK hint f.blocks g.blocks X block.term block'.term
    | none => false

def checkFunction (pre : Machine.Program) (post : Program) (hints : Hints)
    (f : Machine.Function) : Bool :=
  match post.functions.find? (·.name == f.name), f.blocks.find? (·.id == 0) with
  | some g, some entry =>
    let hint := hintOf hints f.name
    distinctIds entry.args && entry.args.all (·.class != .flags) &&
      (hint 0).all (entryState entry.args).contains &&
      f.blocks.all (checkBlock pre post hint f g)
  | _, _ => false

/-- The allocation checker. `checkAllocation_sound` proves that acceptance
implies refinement of the program before allocation. -/
def checkAllocation (pre : Machine.Program) (post : Program) (hints : Hints) : Bool :=
  pre.functions.all (checkFunction pre post hints)

end EvoIR.Alloc
