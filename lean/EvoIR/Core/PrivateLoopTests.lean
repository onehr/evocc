import EvoIR.Core.PrivateCyclic

/-! Whole-pair controls for a persistent cell across a backedge and a fresh
cell on each iteration. Proof search is bounded; accepted executions are not.
The runtime samples below check the concrete machine independently of that
unbounded theorem. They do not supply premises to the checker. -/
namespace EvoIR.Core.Symbolic.PrivateLoopTests

private def condition : Source.Expr :=
  .binary .int .ne (.ident .uint 2) (.literal .uint 0)

private def body : Source.Stmt := .compound [
  .declaration ⟨3, "iteration", ⟨.uint, false⟩, .none,
    some (⟨.uint, false⟩, .ident .uint 2)⟩,
  .statement (.expression (.assign .uint .set 2 ⟨.uint, false⟩ (.ident .uint 2)
    (.binary .uint .sub (.ident .uint 3) (.literal .uint 1))))]

private def returned : Source.Stmt := .return ⟨.uint, false⟩ (.ident .uint 2)

private def source (test : Source.Expr := condition) : Source.Program :=
  ⟨[0], [⟨0, "stored_loop", ⟨.uint, false⟩, false, [⟨1, ⟨.uint, false⟩, .none⟩],
    .compound [
      .declaration ⟨2, "remaining", ⟨.uint, false⟩, .none,
        some (⟨.uint, false⟩, .ident .uint 1)⟩,
      .statement (.while test body), .statement returned]⟩]⟩

private def entry : IR.Block :=
  ⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], [
    ⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 2 4 4, [1, 2]⟩,
    ⟨[⟨6, .memory⟩], .access 4 4, [4, 3]⟩,
    ⟨[⟨7, .memory⟩, ⟨8, .world⟩], .store, [6, 5, 3, 0]⟩],
    .branch ⟨"head", [3, 7, 8]⟩⟩

private def head (forever : Bool := false) : IR.Block :=
  ⟨"head", [⟨10, .pointer⟩, ⟨11, .memory⟩, ⟨12, .world⟩], [
    ⟨[⟨13, .memory⟩], .access 4 4, [11, 10]⟩,
    ⟨[⟨14, .bits 32⟩, ⟨15, .memory⟩, ⟨16, .world⟩], .load 32, [13, 12, 10]⟩,
    ⟨[⟨17, .bits 32⟩], .constant 32 0, []⟩,
    if forever then ⟨[⟨18, .bits 1⟩], .constant 1 1, []⟩
    else ⟨[⟨18, .bits 1⟩], .scalar (.cmp .ne 32), [14, 17]⟩],
    .conditional 18 ⟨"body", [10, 14, 15, 16]⟩ ⟨"exit", [14, 15, 16]⟩⟩

private def loopCode : List IR.Instruction := [
  ⟨[⟨24, .pointer⟩, ⟨25, .memory⟩, ⟨26, .world⟩], .allocate 3 4 4, [22, 23]⟩,
  ⟨[⟨27, .memory⟩], .access 4 4, [25, 24]⟩,
  ⟨[⟨28, .memory⟩, ⟨29, .world⟩], .store, [27, 26, 24, 21]⟩,
  ⟨[⟨30, .memory⟩], .access 4 4, [28, 24]⟩,
  ⟨[⟨31, .bits 32⟩, ⟨32, .memory⟩, ⟨33, .world⟩], .load 32, [30, 29, 24]⟩,
  ⟨[⟨34, .bits 32⟩], .constant 32 1, []⟩,
  ⟨[⟨35, .bits 32⟩], .scalar (.bin .sub 32), [31, 34]⟩,
  ⟨[⟨36, .memory⟩], .access 4 4, [32, 20]⟩,
  ⟨[⟨37, .memory⟩, ⟨38, .world⟩], .store, [36, 33, 20, 35]⟩,
  ⟨[⟨39, .memory⟩, ⟨40, .world⟩], .endLifetime, [37, 38, 24]⟩]

private def target (instructions : List IR.Instruction := loopCode) (forever : Bool := false) : IR.Program :=
  ⟨[⟨"stored_loop", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false,
    ⟨true, true, false, false, false, false, true, false⟩,
    [entry, head forever,
      ⟨"body", [⟨20, .pointer⟩, ⟨21, .bits 32⟩, ⟨22, .memory⟩, ⟨23, .world⟩],
        instructions, .branch ⟨"head", [20, 39, 40]⟩⟩,
      ⟨"exit", [⟨50, .bits 32⟩, ⟨51, .memory⟩, ⟨52, .world⟩], [], .return [50, 51, 52]⟩]⟩]⟩

private def layouts (test : Source.Expr := condition) : List CutpointLayout := [
  ⟨.statement ((source test).functions[0]'(by simp [source])).body [.function none .uint],
    [⟨1, ⟨.uint, false⟩, true⟩], "entry", [(0, 1, .uint)], [(2, 1), (1, 0)]⟩,
  ⟨.expression test [.loopTest test none body, .sequence [.statement returned],
      .scope 1, .function none .uint],
    [⟨2, ⟨.uint, false⟩, true⟩, ⟨1, ⟨.uint, false⟩, true⟩], "head",
    [(10, 2, .uint)], [(12, 1), (11, 0)]⟩]

private def mutate (index : Nat) (replace : IR.Instruction → IR.Instruction) : List IR.Instruction :=
  loopCode.zipIdx.map fun (inst, position) => if position == index then replace inst else inst

private def admitted (instructions : List IR.Instruction := loopCode) : Bool :=
  (privateLoopPair (source) (target instructions) (layouts) 128).isSome

private def concreteRun (count : Nat) : Bool := Id.run do
  let .ok initial := IR.initial (target) "stored_loop"
    [.bits 32 (BitVec.ofNat 32 count), .memory ⟨1, []⟩, .world] | return false
  let result := IR.runSteps (target) (32 * (count+1)) initial
  let .halted (.ok [.bits width bits, .memory memory, .world]) := result.control | return false
  return width == 32 && bits.toNat == 0 && memory.nextId == count+2 &&
    memory.objects.length == count+1 && memory.objects.all (fun object => !object.live) &&
    decide (memory.objects.map IR.Object.id).Nodup

def privateLoopSelfTest : IO UInt32 := do
  let wrongPointer := mutate 9 fun inst => {inst with operands := [37, 38, 20]}
  let wrongValue := mutate 8 fun inst => {inst with operands := [36, 33, 20, 34]}
  let staleMemory := mutate 9 fun inst => {inst with operands := [32, 38, 24]}
  let controls := [
    ("source loop validates", (Source.checkWellFormed (source)).isOk),
    ("target loop validates", (IR.checkProgram (target)).isOk),
    ("fresh allocation on every iteration with persistent backedge cell", admitted),
    ("persistent cell killed before backedge refuses", !admitted wrongPointer),
    ("wrong stored update refuses", !admitted wrongValue),
    ("stale memory token on lifetime end refuses", !admitted staleMemory),
    ("wrong source link on backedge refuses", (privateLoopPair (source) (target)
      ((layouts).map fun point => if point.label == "head" then
        {point with links := [(10, 1, .uint)]} else point) 128).isNone),
    ("missing cyclic checkpoint refuses", (privateLoopPair (source) (target) ((layouts).take 1) 128).isNone),
    ("zero proof-search budget refuses", (privateLoopPair (source) (target) (layouts) 0).isNone),
    ("unbounded repeated allocation admits divergence", (privateLoopPair
      (source (.literal .int 1)) (target loopCode true) (layouts (.literal .int 1)) 128).isSome)] ++
    [0, 1, 5, 17].map (fun count => (s!"concrete fresh identities after {count} iterations", concreteRun count))
  let mut failures : UInt32 := 0
  for (name, passed) in controls do
    if !passed then
      IO.eprintln s!"private loop control failed: {name}"
      failures := failures+1
  if failures == 0 then IO.println s!"{controls.length} private loop controls pass"
  return failures

end EvoIR.Core.Symbolic.PrivateLoopTests
