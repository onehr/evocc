import EvoIR.MachineTransportTable

/-!
Untrusted proposals for actual SSA/postSSA pairs. The search follows finite
operand dependencies and actual instruction identities. It never changes
either input program. `checkProgram` independently validates every proposed
binding, instruction position, deletion and edge transfer.

Construction bounds count syntax, not executed steps. Exhausting a bound or
failing to find a correspondence returns no proposal; it does not weaken the
checking obligations or replace the actual input with a normalized program.
-/
namespace EvoIR.Machine.Transport

private def registerIds (function : Function) : List Nat :=
  (function.blocks.flatMap fun block =>
    block.args ++ block.insts.flatMap (fun inst => inst.defs ++ inst.uses) ++
      controlUses block.term ++ block.term.edges.flatMap Prod.snd).map VReg.id |>.eraseDups

private def includeRegs (needed : List Nat) (regs : List VReg) : List Nat :=
  regs.foldl (fun found reg => if reg.id ∈ found then found else found ++ [reg.id]) needed

private def demandPass (function : Function) (needed : List Nat) : List Nat :=
  function.blocks.foldl (fun needed block =>
    let needed := includeRegs needed (controlUses block.term)
    let needed := block.insts.foldl (fun needed inst =>
      if !inst.pure || inst.defs.any (fun reg => decide (reg.id ∈ needed)) then
        includeRegs needed inst.uses
      else needed) needed
    let needed := (block.insts.zip (block.insts.drop 1)).foldl (fun needed (producer, consumer) =>
      if producer.defs.any (fun reg => decide (reg.class = .flags ∧ reg ∈ consumer.uses)) &&
          producer.defs.any (fun reg => decide (reg.id ∈ needed)) then
        includeRegs needed consumer.defs
      else needed) needed
    block.term.edges.foldl (fun needed (destination, args) =>
      match findBlock function destination with
      | none => needed
      | some target => (target.args.zip args).foldl (fun needed (param, value) =>
          if param.id ∈ needed then includeRegs needed [value] else needed) needed) needed) needed

private def demandFixed (function : Function) : Nat → List Nat → Option (List Nat)
  | 0, _ => none
  | fuel+1, needed =>
    let next := demandPass function needed
    if next = needed then some next else demandFixed function fuel next

/-- A proposed demand closure. The structural checker checks the closure
again; this procedure supplies no semantic fact to the proof. -/
def proposeDemand (function : Function) : Option (List Nat) :=
  demandFixed function ((registerIds function).length+2) []

private def resolveAlias (aliases : UseMap) : Nat → VReg → VReg
  | 0, reg => reg
  | fuel+1, reg => match aliases.lookup reg.id with
    | none => reg
    | some next => if next = reg then reg else resolveAlias aliases fuel next

private def incomingValues (function : Function) (block : Block) (index : Nat) : List VReg :=
  function.blocks.flatMap fun predecessor =>
    predecessor.term.edges.filterMap fun (destination, args) =>
      if destination = block.id then args[index]? else none

private def aliasPass (function : Function) (bound : Nat) (aliases : UseMap) : UseMap :=
  function.blocks.foldl (fun aliases block =>
    if block.id = 0 then aliases else
    block.args.zipIdx |>.foldl (fun aliases (param, index) =>
      if (aliases.lookup param.id).isSome then aliases else
      let incoming := (incomingValues function block index).map (resolveAlias aliases bound)
        |>.filter (fun reg => reg.id != param.id) |>.eraseDups
      match incoming with
      | [root] => if root.class = param.class then (param.id, root) :: aliases else aliases
      | _ => aliases) aliases) aliases

private def aliasFixed (function : Function) (bound : Nat) : Nat → UseMap → UseMap
  | 0, aliases => aliases
  | fuel+1, aliases =>
    let next := aliasPass function bound aliases
    if next = aliases then next else aliasFixed function bound fuel next

private def proposeAliases (function : Function) : UseMap :=
  if function.blocks.any (fun block => block.term.edges.any (fun edge => decide (edge.1 = 0))) then []
  else
    let bound := (registerIds function).length+1
    let aliases := aliasFixed function bound bound []
    aliases.map fun (id, reg) => (id, resolveAlias aliases bound reg)

private def alignInstructions (actual : List Instruction) (needed : List Nat)
    (aliases : UseMap) : Nat → List Instruction → Option (List (Option Nat))
  | _, [] => some []
  | start, inst :: rest => do
    let candidate := inst.mapUses (rename aliases)
    match (actual.zipIdx).find? (fun (other, index) => decide (start ≤ index ∧ other = candidate)) with
    | some (_, index) =>
      let tail ← alignInstructions actual needed aliases (index+1) rest
      return some index :: tail
    | none =>
      if checkDCE needed inst then do
        let tail ← alignInstructions actual needed aliases start rest
        return none :: tail
      else none

private def proposeBlock (target : Function) (needed : List Nat) (aliases : UseMap)
    (block : Block) : Option BlockPlan := do
  let actual ← findBlock target block.id
  let alignment ← alignInstructions actual.insts needed aliases 0 block.insts
  let firstRetained := (alignment.filterMap id).head?
  -- A block with no retained source instruction can still contain outgoing
  -- edge copies. Those copies belong to its body, not its entry invariant.
  let prefixLength := if block.id = 0 then 0 else firstRetained.getD 0
  let bindings := block.args.filterMap fun reg =>
    if reg.id ∈ needed then some (reg.id, rename aliases reg) else none
  return ⟨block.id, prefixLength, bindings, alignment⟩

/-- Raw search output for diagnostics and independent checking. Possession
of this value is not proof admission. -/
def searchFunctionPlan (source target : Function) : Option FunctionPlan := do
  let needed ← proposeDemand source
  let aliases := proposeAliases source
  let blocks ← source.blocks.mapM (proposeBlock target needed aliases)
  return ⟨needed, blocks⟩

def proposeFunction (source target : Function) : Option FunctionPlan := do
  let proposed ← searchFunctionPlan source target
  if checkStructure source target proposed then some proposed else none

def proposeProgram (source target : Program) : Option ProgramPlan := do
  let functions ← source.functions.mapM fun function => do
    let actual ← target.functions.find? (·.name == function.name)
    let proposed ← proposeFunction function actual
    return (function.name, proposed)
  let proposed : ProgramPlan := ⟨functions⟩
  if checkProgram source target proposed then some proposed else none

end EvoIR.Machine.Transport
