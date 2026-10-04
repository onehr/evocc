import EvoIR.Core.IRReader

/-! Independent token/access/effect admission for the decoded core target.
These checks are prerequisites, not a source/target result check. Their
connection to the execution invariant remains a separate proof obligation. -/
namespace EvoIR.Core.IR

def Effects.join (a b : Effects) : Effects :=
  ⟨a.readsMemory || b.readsMemory, a.writesMemory || b.writesMemory,
   a.volatile || b.volatile, a.atomic || b.atomic, a.io || b.io, a.fenv || b.fenv,
   a.mayTrap || b.mayTrap, a.mayUnwind || b.mayUnwind⟩

def operationEffects (summaries : List (String × Effects)) : Operation → Effects
  | .allocate .. | .endLifetime => ⟨true, true, false, false, false, false, true, false⟩
  | .access .. | .load .. => ⟨true, false, false, false, false, false, true, false⟩
  | .store => ⟨false, true, false, false, false, false, true, false⟩
  | .assume .. | .division .. => {Reader.emptyEffects with mayTrap := true}
  | .call name => (summaries.lookup name).getD
      ⟨true, true, true, true, true, true, true, true⟩
  | _ => Reader.emptyEffects

def deriveEffects (p : Program) : Nat → List (String × Effects)
  | 0 => p.functions.map (fun f => (f.name, Reader.emptyEffects))
  | n+1 =>
    let previous := deriveEffects p n
    p.functions.map fun f => (f.name,
      (f.blocks.flatMap Block.instructions).foldl
        (fun e i => e.join (operationEffects previous i.op)) Reader.emptyEffects)

def acyclicCalls (p : Program) : Nat → String → Bool
  | 0, _ => false
  | depth+1, name => match p.functions.find? (·.name == name) with
    | none => false
    | some f => (f.blocks.flatMap Block.instructions).all fun i => match i.op with
      | .call callee => acyclicCalls p depth callee
      | _ => true

def tokenized (types : List Ty) : Except String Bool := do
  let ordinary := fun t => t == .bits 32 || t == .bits 64
  if types.all ordinary then return false
  if types.length ≥ 2 && types.drop (types.length-2) == [.memory, .world] &&
      (types.dropLast.dropLast).all ordinary then return true
  throw "invalid core function signature or token order"

def checkBlockTokens (f : Function) (b : Block) : Except String Unit := do
  let tokenMode ← tokenized f.parameters
  let memories := b.parameters.filter (·.type == .memory)
  let worlds := b.parameters.filter (·.type == .world)
  if memories.length != (if tokenMode then 1 else 0) ||
      worlds.length != (if tokenMode then 1 else 0) then throw "block token multiplicity"
  let mut m := memories.head?.map Binding.id
  let mut w := worlds.head?.map Binding.id
  let mut bindings := b.parameters
  let mut guard : Option (Nat × Nat × Nat × Nat) := none
  -- Baseline C lowering retains explicit div/rem obligations even though the
  -- raw IR operations also trap. Do not let intrinsic traps conceal a deleted
  -- source obligation in a proposed checked compilation.
  let mut definitions : List (Nat × (Operation × List Nat)) := []
  let mut assumptions : List (Reason × Operation × List Nat) := []
  for i in b.instructions do
    let actual ← i.operands.mapM (Reader.operandType bindings)
    let memoryOperands := (i.operands.zip actual).filter (·.2 == .memory) |>.map Prod.fst
    let worldOperands := (i.operands.zip actual).filter (·.2 == .world) |>.map Prod.fst
    let memoryResults := i.results.filter (·.type == .memory) |>.map Binding.id
    let worldResults := i.results.filter (·.type == .world) |>.map Binding.id
    if let .call _ := i.op then
      if memoryOperands.isEmpty || worldOperands.isEmpty then throw "call lacks memory/world"
    if !memoryOperands.isEmpty then
      if memoryOperands != m.toList || memoryResults.length != 1 then throw "stale or duplicated memory token"
    else if !memoryResults.isEmpty then throw "memory token manufactured"
    if !worldOperands.isEmpty then
      if worldOperands != w.toList || worldResults.length != 1 then throw "stale or duplicated world token"
    else if !worldResults.isEmpty then throw "world token manufactured"
    match i.op, i.operands with
    | .assume reason, [id] =>
      if let some (predicate, operands) := definitions.lookup id then
        assumptions := (reason, predicate, operands) :: assumptions
    | .division signed remainder n, [a, divisor] =>
      let reason := if remainder then Reason.remainderByZero else Reason.divisionByZero
      if !assumptions.contains (reason, .scalar (.nonzero n), [divisor]) then
        throw "division lacks matching explicit nonzero obligation"
      if signed && !assumptions.contains
          (.signedOverflow, .scalar (.guard .divRem n), [a, divisor]) then
        throw "signed division lacks matching explicit overflow obligation"
    | .load n, [mem, _, ptr] =>
      if guard != some (mem, ptr, n/8, n/8) then throw "load lacks matching current access guard"
    | .store, [mem, _, ptr, val] =>
      let n ← Reader.operandWidth bindings val
      if guard != some (mem, ptr, n/8, n/8) then throw "store lacks matching current access guard"
    | _, _ => pure ()
    if !memoryResults.isEmpty then
      m := memoryResults.head?
      guard := none
    if !worldResults.isEmpty then w := worldResults.head?
    if let .access size align := i.op then
      let [_, ptr] := i.operands | throw "access shape"
      let some output := m | throw "access result"
      guard := some (output, ptr, size, align)
    if let [result] := i.results then
      definitions := (result.id, (i.op, i.operands)) :: definitions
    bindings := bindings ++ i.results
  let paths := match b.terminator with
    | .return ids => [ids]
    | .branch edge => [edge.arguments]
    | .conditional _ yes no => [yes.arguments, no.arguments]
  for ids in paths do
    let types ← ids.mapM (Reader.operandType bindings)
    let carriedM := (ids.zip types).filter (·.2 == .memory) |>.map Prod.fst
    let carriedW := (ids.zip types).filter (·.2 == .world) |>.map Prod.fst
    if carriedM != m.toList || carriedW != w.toList then throw "edge/return loses or reuses token"

def checkProgram (p : Program) : Except String Unit := do
  let effects := deriveEffects p p.functions.length
  if effects != deriveEffects p (p.functions.length+1) then throw "effect summary did not close"
  for f in p.functions do
    if !acyclicCalls p p.functions.length f.name then throw "recursive or external call closure"
    let tokens ← tokenized f.parameters
    if (← tokenized f.returns) != tokens ||
        f.returns.length != (if tokens then 3 else 1) then throw "core return signature"
    if (effects.lookup f.name) != some f.effects then throw "incorrect derived effects"
    for b in f.blocks do checkBlockTokens f b

end EvoIR.Core.IR
