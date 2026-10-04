import EvoIR.Core.IR

/-! Instruction-by-instruction EvoIR execution. This is not a compilation
checker. State contains block-local bindings and immutable memory snapshots;
loops/calls use explicit control state rather than evaluator recursion. -/
namespace EvoIR.Core.IR

open Source (Fault)

abbrev Bindings := List (Nat × Value)

def lookup (values : Bindings) (id : Nat) : Except Fault Value :=
  match values.lookup id with
  | some v => .ok v
  | none => .error (.malformed "IR use outside current block")

def bind (parameters : List Binding) (args : List Value) : Except Fault Bindings := do
  if parameters.length != args.length then throw (.malformed "IR binding arity")
  let mut values := []
  for (p, v) in parameters.zip args do
    if p.type != v.type then throw (.malformed "IR binding type")
    if values.any (fun entry => entry.1 == p.id) then throw (.malformed "IR duplicate binding")
    values := (p.id, v) :: values
  return values

def appendResults (values : Bindings) (results : List Binding) (args : List Value) :
    Except Fault Bindings := do
  let fresh ← bind results args
  if fresh.any (fun (id, _) => (values.lookup id).isSome) then
    throw (.malformed "IR result already bound")
  return fresh ++ values

private def pointer : Value → Except Fault (Nat × Int)
  | .pointer id offset => .ok (id, offset)
  | .poison _ => .error (.diagnostic 3013)
  | _ => .error (.malformed "IR expected pointer")

private def memory : Value → Except Fault Memory
  | .memory m => .ok m
  | _ => .error (.malformed "IR expected memory")

private def world (v : Value) : Except Fault Unit :=
  if v == .world then .ok () else .error (.malformed "IR expected world")

private def liveObject (m : Memory) (id : Nat) : Except Fault Object := do
  if id == 0 then throw (.diagnostic 3009)
  let some o := m.objects.find? (·.id == id) | throw (.diagnostic 3013)
  if !o.live then throw (.diagnostic 3010)
  return o

def accessObject (m : Memory) (p : Value) (size align : Nat) : Except Fault (Object × Nat) := do
  let (id, offset) ← pointer p
  let o ← liveObject m id
  if size == 0 || align == 0 then throw (.malformed "IR zero access layout")
  if offset < 0 || offset + size > o.bytes.length then throw (.diagnostic 3011)
  if o.align < align || offset % align != 0 then throw (.diagnostic 3012)
  return (o, offset.toNat)

def Memory.replace (m : Memory) (o : Object) : Memory :=
  {m with objects := m.objects.map fun old => if old.id == o.id then o else old}

def Memory.endFrame (m : Memory) (frame : Nat) : Memory :=
  {m with objects := m.objects.map fun o => if o.owner == frame then {o with live := false} else o}

/-- Pure execution of a non-call instruction. All bytes of fresh objects are
uninitialized. Loading checks each byte; poison never becomes an arbitrary zero. -/
def evalOperation (owner : Nat) (op : Operation) (args : List Value) :
    Except Fault (List Value) := do
  match op, args with
  | .constant n bits, [] =>
    if n == 0 || bits ≥ 2^n then throw (.malformed "IR noncanonical constant")
    return [.bits n (BitVec.ofNat n bits)]
  | .uninitialized t, [] => return [.poison t]
  | .scalar op, args => return [← evalScalar op args]
  | .division signed rem n, args => return [← evalDivision signed rem n args]
  | .assume reason, [v] =>
    if (← v.asBits 1).toNat == 0 then throw (.diagnostic reason.code)
    return []
  | .allocate origin size align, [mv, wv] =>
    let m ← memory mv
    world wv
    if size == 0 || align == 0 || m.nextId == 0 then throw (.malformed "IR allocation layout")
    let id := m.nextId
    if m.objects.any (·.id == id) then throw (.malformed "IR reused object identity")
    let o : Object := ⟨id, origin, owner, align, true, List.replicate size none⟩
    return [.pointer id 0, .memory ⟨id+1, o :: m.objects⟩, .world]
  | .endLifetime, [mv, wv, p] =>
    let m ← memory mv
    world wv
    let (id, offset) ← pointer p
    let o ← liveObject m id
    if offset != 0 || o.owner != owner then throw (.diagnostic 3018)
    return [.memory (m.replace {o with live := false}), .world]
  | .access size align, [mv, p] =>
    let m ← memory mv
    let _ ← accessObject m p size align
    return [mv]
  | .load n, [mv, wv, p] =>
    let m ← memory mv
    world wv
    if n != 32 && n != 64 then throw (.malformed "IR scalar load width")
    let (o, offset) ← accessObject m p (n/8) (n/8)
    let mut bits := 0
    for i in List.range (n/8) do
      let some (some byte) := o.bytes[offset+i]? | throw (.diagnostic 3001)
      bits := bits + byte.toNat * 2^(8*i)
    return [.bits n (BitVec.ofNat n bits), mv, .world]
  | .store, [mv, wv, p, .bits n value] =>
    let m ← memory mv
    world wv
    if n != 32 && n != 64 then throw (.malformed "IR scalar store width")
    let (o, offset) ← accessObject m p (n/8) (n/8)
    let bytes := (List.range (n/8)).map fun i => some (BitVec.ofNat 8 (value.toNat / 2^(8*i)))
    let o := {o with bytes := o.bytes.take offset ++ bytes ++ o.bytes.drop (offset+n/8)}
    return [.memory (m.replace o), .world]
  | .store, [mv, wv, p, .poison (.bits n)] =>
    let m ← memory mv
    world wv
    let _ ← accessObject m p (n/8) (n/8)
    throw (.diagnostic 3001)
  | _, _ => throw (.malformed "IR instruction operand shape")

structure Activation where
  function : String
  block : String
  pc : Nat
  values : Bindings
  owner : Nat
  deriving Repr

structure ReturnFrame where
  caller : Activation
  results : List Binding
  deriving Repr

inductive Control where
  | running (activation : Activation)
  | halted (result : Except Fault (List Value))
  deriving Repr

structure State where
  control : Control
  stack : List ReturnFrame
  nextFrame : Nat
  deriving Repr

def enter (f : Function) (args : List Value) (owner : Nat) : Except Fault Activation := do
  if f.parameters != args.map Value.type then throw (.malformed "IR function argument types")
  let some b := f.blocks.head? | throw (.malformed "IR function has no entry")
  if b.parameters.map Binding.type != f.parameters then throw (.malformed "IR entry signature")
  return ⟨f.name, b.name, 0, ← bind b.parameters args, owner⟩

def jump (f : Function) (a : Activation) (edge : Edge) : Except Fault Activation := do
  let some b := f.blocks.find? (·.name == edge.block) | throw (.malformed "IR missing successor")
  -- Capture every source before installing any destination, including cycles.
  let args ← edge.arguments.mapM (lookup a.values)
  return {a with block := b.name, pc := 0, values := ← bind b.parameters args}

def execute (p : Program) (s : State) (a : Activation) : Except Fault State := do
  let some f := p.functions.find? (·.name == a.function) | throw (.malformed "IR missing function")
  let some b := f.blocks.find? (·.name == a.block) | throw (.malformed "IR missing block")
  if let some inst := b.instructions[a.pc]? then
    let args ← inst.operands.mapM (lookup a.values)
    if let .call name := inst.op then
      let some callee := p.functions.find? (·.name == name) | throw (.malformed "IR external call")
      if inst.results.map Binding.type != callee.returns then throw (.malformed "IR call results")
      return ⟨.running (← enter callee args s.nextFrame),
        ⟨{a with pc := a.pc+1}, inst.results⟩ :: s.stack, s.nextFrame+1⟩
    else
      let results ← evalOperation a.owner inst.op args
      let values ← appendResults a.values inst.results results
      return {s with control := .running {a with pc := a.pc+1, values}}
  if a.pc != b.instructions.length then throw (.malformed "IR invalid program counter")
  match b.terminator with
  | .branch edge => return {s with control := .running (← jump f a edge)}
  | .conditional id yes no =>
    let c ← (← lookup a.values id).asBits 1
    return {s with control := .running (← jump f a (if c.toNat == 0 then no else yes))}
  | .return ids =>
    let values ← ids.mapM (lookup a.values)
    if values.map Value.type != f.returns then throw (.malformed "IR return types")
    let values := values.map fun v => match v with
      | .memory m => Value.memory (m.endFrame a.owner)
      | _ => v
    match s.stack with
    | [] =>
      if values.any (fun v => match v with | .poison _ => true | _ => false) then
        throw (.diagnostic 3001)
      return {s with control := .halted (.ok values)}
    | frame :: rest =>
      let values ← appendResults frame.caller.values frame.results values
      return {s with control := .running {frame.caller with values}, stack := rest}

def step (p : Program) (s : State) : Option State :=
  match s.control with
  | .halted _ => none
  | .running a => some (match execute p s a with
      | .ok next => next
      | .error fault => {s with control := .halted (.error fault)})

def initial (p : Program) (entry : String) (args : List Value) : Except Fault State := do
  let some f := p.functions.find? (·.name == entry) | throw (.malformed "IR unknown entry")
  return ⟨.running (← enter f args 1), [], 2⟩

inductive Step (p : Program) : State → State → Prop where
  | execute {a b} : step p a = some b → Step p a b

inductive Steps (p : Program) : State → State → Prop where
  | refl (s) : Steps p s s
  | next {a b c} : Step p a b → Steps p b c → Steps p a c

def Diverges (p : Program) (s : State) : Prop :=
  ∃ trace : Nat → State, trace 0 = s ∧ ∀ n, Step p (trace n) (trace (n+1))

def runSteps (p : Program) : Nat → State → State
  | 0, s => s
  | n+1, s => match step p s with | none => s | some next => runSteps p n next

theorem step_deterministic (p : Program) (a b c : State)
    (hb : Step p a b) (hc : Step p a c) : b = c := by
  cases hb with | execute hb =>
    cases hc with | execute hc => exact Option.some.inj (hb.symm.trans hc)

theorem runSteps_prefix (p : Program) (n : Nat) (s : State) :
    Steps p s (runSteps p n s) := by
  induction n generalizing s with
  | zero => exact .refl s
  | succ n ih =>
    simp only [runSteps]
    split
    next => exact .refl s
    next next h => exact .next (.execute h) (ih next)

end EvoIR.Core.IR
