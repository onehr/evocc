import EvoIR.Core.PrivateChecker

namespace EvoIR.Core.Symbolic.PrivateStorageTests

private abbrev Widths : Var → Nat := fun _ => 32

private def entry : PrivateConfig Widths :=
  ⟨[(0, .opaque 0), (1, .opaque 1)], [.memory, .world], 0, 1, []⟩

private def allocation (width : Nat) : IR.Instruction :=
  ⟨[⟨2, .pointer⟩, ⟨3, .memory⟩, ⟨4, .world⟩], .allocate 7 (width/8) (width/8), [0, 1]⟩

private def code (width bits : Nat) : List IR.Instruction := [
  allocation width,
  ⟨[⟨5, .bits width⟩], .constant width bits, []⟩,
  ⟨[⟨6, .memory⟩], .access (width/8) (width/8), [3, 2]⟩,
  ⟨[⟨7, .memory⟩, ⟨8, .world⟩], .store, [6, 4, 2, 5]⟩,
  ⟨[⟨9, .memory⟩], .access (width/8) (width/8), [7, 2]⟩,
  ⟨[⟨10, .bits width⟩, ⟨11, .memory⟩, ⟨12, .world⟩], .load width, [9, 8, 2]⟩]

private def inspect (instructions : List IR.Instruction) : Option (PrivateConfig Widths) :=
  instructions.foldlM (fun before inst => (privateInstruction before inst).map (·.next)) entry

private def mutate (instructions : List IR.Instruction) (index : Nat)
    (replace : IR.Instruction → IR.Instruction) : List IR.Instruction :=
  instructions.zipIdx.map fun (inst, position) => if position == index then replace inst else inst

private def loadedBits (instructions : List IR.Instruction) : Option (Nat × Nat) := do
  let after ← inspect instructions
  let word ← after.scope.word 10
  return (word.1, (denoteTerm (fun _ => 0#32) word.2).toNat)

private def endLifetime : IR.Instruction :=
  ⟨[⟨13, .memory⟩, ⟨14, .world⟩], .endLifetime, [11, 12, 2]⟩

private def readAfterLifetime : IR.Instruction :=
  ⟨[⟨15, .bits 32⟩, ⟨16, .memory⟩, ⟨17, .world⟩], .load 32, [13, 14, 2]⟩

-- Keep a live caller object and an old dead object. Allocation must use nextId,
-- not list length, and the actual function return must preserve both objects.
private def caller : IR.Memory := ⟨37, [
  ⟨8, 21, 0, 8, true, IR.scalarBytes 64 (0xfedcba9876543210#64)⟩,
  ⟨4, 17, 0, 4, false, List.replicate 4 none⟩]⟩

private def concreteReturn (width bits : Nat) : Bool := Id.run do
  let function : IR.Function := ⟨"stored", [.memory, .world], [.bits width, .memory, .world],
    false, ⟨true, true, false, false, false, false, true, false⟩,
    [⟨"entry", [⟨0, .memory⟩, ⟨1, .world⟩], code width bits, .return [10, 11, 12]⟩]⟩
  let program : IR.Program := ⟨[function]⟩
  let .ok start := IR.initial program "stored" [.memory caller, .world] | return false
  let result := IR.runSteps program ((code width bits).length + 1) start
  let .halted (.ok [.bits actualWidth value, .memory memory, .world]) := result.control
    | return false
  let returned : IR.Object :=
    ⟨37, 7, 1, width/8, false, IR.scalarBytes width (BitVec.ofNat width bits)⟩
  return actualWidth == width && value.toNat == bits && memory.nextId == 38 &&
    memory.objects == returned :: caller.objects

def privateStorageSelfTest : IO UInt32 := do
  let base := code 32 29
  let initialized := base.take 4
  let controls := [
    ("32-bit initialized transport", loadedBits base == some (32, 29)),
    ("64-bit initialized transport", loadedBits (code 64 0xfedcba9876543210) ==
      some (64, 0xfedcba9876543210)),
    ("zero initialized transport", loadedBits (code 32 0) == some (32, 0)),
    ("all-one initialized transport", loadedBits (code 32 0xffffffff) == some (32, 0xffffffff)),
    ("lifetime end accepted", (inspect (base ++ [endLifetime])).isSome),
    ("fresh identity and caller bytes at 32-bit return", concreteReturn 32 29),
    ("fresh identity and caller bytes at 64-bit return", concreteReturn 64 0xfedcba9876543210),
    ("zero allocation size refused",
      (inspect (mutate base 0 fun inst => {inst with op := .allocate 7 0 4})).isNone),
    ("insufficient allocation alignment refused",
      (inspect (mutate base 0 fun inst => {inst with op := .allocate 7 4 2})).isNone),
    ("unsupported allocation width refused", (inspect (code 16 29)).isNone),
    ("duplicate result binding refused",
      (inspect (mutate base 0 fun inst => {inst with results :=
        [⟨0, .pointer⟩, ⟨3, .memory⟩, ⟨4, .world⟩]})).isNone),
    ("stale store memory refused",
      (inspect (mutate base 3 fun inst => {inst with operands := [3, 4, 2, 5]})).isNone),
    ("stale load world refused",
      (inspect (mutate base 5 fun inst => {inst with operands := [9, 4, 2]})).isNone),
    ("unknown pointer refused",
      (inspect (mutate base 5 fun inst => {inst with operands := [9, 8, 99]})).isNone),
    ("wrong load width refused",
      (inspect (mutate base 5 fun inst => {inst with op := .load 64})).isNone),
    ("wrong load result type refused",
      (inspect (mutate base 5 fun inst => {inst with results :=
        [⟨10, .bits 64⟩, ⟨11, .memory⟩, ⟨12, .world⟩]})).isNone),
    ("load of uninitialized cell refused",
      (inspect [allocation 32, ⟨[⟨5, .bits 32⟩, ⟨6, .memory⟩, ⟨7, .world⟩],
        .load 32, [3, 4, 2]⟩]).isNone),
    ("poison store refused",
      (inspect (mutate base 1 fun inst => {inst with op := .uninitialized (.bits 32)})).isNone),
    ("load after lifetime end refused", (inspect (base ++ [endLifetime, readAfterLifetime])).isNone),
    ("repeated lifetime end refused",
      (inspect (base ++ [endLifetime, ⟨[⟨15, .memory⟩, ⟨16, .world⟩],
        .endLifetime, [13, 14, 2]⟩])).isNone),
    ("out-of-bounds access refused",
      (inspect (initialized ++ [⟨[⟨9, .memory⟩], .access 8 4, [7, 2]⟩])).isNone),
    ("over-aligned access refused",
      (inspect (initialized ++ [⟨[⟨9, .memory⟩], .access 4 8, [7, 2]⟩])).isNone),
    ("zero access size refused",
      (inspect (initialized ++ [⟨[⟨9, .memory⟩], .access 0 4, [7, 2]⟩])).isNone),
    ("zero access alignment refused",
      (inspect (initialized ++ [⟨[⟨9, .memory⟩], .access 4 0, [7, 2]⟩])).isNone)]
  let mut failures : UInt32 := 0
  for (name, passed) in controls do
    if !passed then
      IO.eprintln s!"private storage control failed: {name}"
      failures := failures + 1
  if failures == 0 then
    IO.println s!"{controls.length} private storage transfer controls pass (whole-pair closure checked separately)"
  return failures

end EvoIR.Core.Symbolic.PrivateStorageTests
