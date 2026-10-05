import EvoIR.Core.PrivateCallingClosure
import EvoIR.Core.SourceTargetCheck

/-! Whole-pair private call controls through the common dispatcher. A caller
cell stays live across a cyclic callee's fresh allocations and real endFrame.
Scalar-only call controls exercise the internal context component and raw
machine separately: the existing public memory/world call protocol refuses
those programs. -/
namespace EvoIR.Core.Symbolic.PrivateCallTests

private def testCondition : Source.Expr := .binary .int .ne (.ident .uint 21) (.literal .uint 0)

private def loopBody : Source.Stmt := .compound [
  .declaration ⟨22, "iteration", ⟨.uint, false⟩, .none, some (⟨.uint, false⟩, .ident .uint 21)⟩,
  .statement (.expression (.assign .uint .set 21 ⟨.uint, false⟩ (.ident .uint 21)
    (.binary .uint .sub (.ident .uint 22) (.literal .uint 1))))]

private def innerReturn : Source.Stmt := .return ⟨.uint, false⟩ (.ident .uint 21)

private def innerSource (scalar : Bool) : Source.Function :=
  ⟨10, "inner_private", ⟨.uint, false⟩, false, [⟨20, ⟨.uint, false⟩, .none⟩],
    if scalar then .return ⟨.uint, false⟩ (.ident .uint 20)
    else .compound [
      .declaration ⟨21, "remaining", ⟨.uint, false⟩, .none, some (⟨.uint, false⟩, .ident .uint 20)⟩,
      .statement (.while testCondition loopBody), .statement innerReturn]⟩

private def outerSource : Source.Function :=
  ⟨0, "outer_private", ⟨.uint, false⟩, false, [⟨1, ⟨.uint, false⟩, .none⟩], .compound [
    .declaration ⟨2, "saved", ⟨.uint, false⟩, .none, some (⟨.uint, false⟩, .ident .uint 1)⟩,
    .declaration ⟨3, "answer", ⟨.uint, false⟩, .none,
      some (⟨.uint, false⟩, .call .uint 10 [(⟨.uint, false⟩, .ident .uint 1)])⟩,
    .statement (.return ⟨.uint, false⟩ (.binary .uint .add (.ident .uint 2) (.ident .uint 3)))]⟩

private def source (scalar : Bool := false) : Source.Program := ⟨[0], [innerSource scalar, outerSource]⟩

private def memoryEffects : IR.Effects := ⟨true, true, false, false, false, false, true, false⟩

private def innerLoopCode : List IR.Instruction := [
  ⟨[⟨24, .pointer⟩, ⟨25, .memory⟩, ⟨26, .world⟩], .allocate 22 4 4, [22, 23]⟩,
  ⟨[⟨27, .memory⟩], .access 4 4, [25, 24]⟩,
  ⟨[⟨28, .memory⟩, ⟨29, .world⟩], .store, [27, 26, 24, 21]⟩,
  ⟨[⟨30, .memory⟩], .access 4 4, [28, 24]⟩,
  ⟨[⟨31, .bits 32⟩, ⟨32, .memory⟩, ⟨33, .world⟩], .load 32, [30, 29, 24]⟩,
  ⟨[⟨34, .bits 32⟩], .constant 32 1, []⟩,
  ⟨[⟨35, .bits 32⟩], .scalar (.bin .sub 32), [31, 34]⟩,
  ⟨[⟨36, .memory⟩], .access 4 4, [32, 20]⟩,
  ⟨[⟨37, .memory⟩, ⟨38, .world⟩], .store, [36, 33, 20, 35]⟩,
  ⟨[⟨39, .memory⟩, ⟨40, .world⟩], .endLifetime, [37, 38, 24]⟩]

private def innerTarget (scalar : Bool) (code : List IR.Instruction := innerLoopCode) : IR.Function :=
  if scalar then
    ⟨"inner_private", [.bits 32], [.bits 32], false, IR.Reader.emptyEffects,
      [⟨"entry", [⟨0, .bits 32⟩], [], .return [0]⟩]⟩
  else ⟨"inner_private", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, memoryEffects, [
    ⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], [
      ⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 21 4 4, [1, 2]⟩,
      ⟨[⟨6, .memory⟩], .access 4 4, [4, 3]⟩,
      ⟨[⟨7, .memory⟩, ⟨8, .world⟩], .store, [6, 5, 3, 0]⟩], .branch ⟨"head", [3, 7, 8]⟩⟩,
    ⟨"head", [⟨10, .pointer⟩, ⟨11, .memory⟩, ⟨12, .world⟩], [
      ⟨[⟨13, .memory⟩], .access 4 4, [11, 10]⟩,
      ⟨[⟨14, .bits 32⟩, ⟨15, .memory⟩, ⟨16, .world⟩], .load 32, [13, 12, 10]⟩,
      ⟨[⟨17, .bits 32⟩], .constant 32 0, []⟩,
      ⟨[⟨18, .bits 1⟩], .scalar (.cmp .ne 32), [14, 17]⟩],
      .conditional 18 ⟨"body", [10, 14, 15, 16]⟩ ⟨"exit", [14, 15, 16]⟩⟩,
    ⟨"body", [⟨20, .pointer⟩, ⟨21, .bits 32⟩, ⟨22, .memory⟩, ⟨23, .world⟩], code,
      .branch ⟨"head", [20, 39, 40]⟩⟩,
    ⟨"exit", [⟨50, .bits 32⟩, ⟨51, .memory⟩, ⟨52, .world⟩], [], .return [50, 51, 52]⟩]⟩

private def outerCode (scalar : Bool) : List IR.Instruction := [
  ⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 2 4 4, [1, 2]⟩,
  ⟨[⟨6, .memory⟩], .access 4 4, [4, 3]⟩,
  ⟨[⟨7, .memory⟩, ⟨8, .world⟩], .store, [6, 5, 3, 0]⟩,
  if scalar then ⟨[⟨9, .bits 32⟩], .call "inner_private", [0]⟩
  else ⟨[⟨9, .bits 32⟩, ⟨10, .memory⟩, ⟨11, .world⟩], .call "inner_private", [0, 7, 8]⟩,
  ⟨[⟨12, .memory⟩], .access 4 4, [if scalar then 7 else 10, 3]⟩,
  ⟨[⟨13, .bits 32⟩, ⟨14, .memory⟩, ⟨15, .world⟩], .load 32, [12, if scalar then 8 else 11, 3]⟩,
  ⟨[⟨16, .bits 32⟩], .scalar (.bin .add 32), [13, 9]⟩]

private def target (scalar : Bool := false) (callerCode : List IR.Instruction := outerCode scalar)
    (calleeCode : List IR.Instruction := innerLoopCode) : IR.Program :=
  ⟨[⟨"outer_private", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, memoryEffects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], callerCode, .return [16, 14, 15]⟩]⟩,
    innerTarget scalar calleeCode]⟩

private def innerLayouts : List CutpointLayout := [
  ⟨.statement (innerSource false).body [.function none .uint], [⟨20, ⟨.uint, false⟩, true⟩],
    "entry", [(0, 20, .uint)], [(2, 1), (1, 0)]⟩,
  ⟨.expression testCondition [.loopTest testCondition none loopBody, .sequence [.statement innerReturn],
      .scope 1, .function none .uint],
    [⟨21, ⟨.uint, false⟩, true⟩, ⟨20, ⟨.uint, false⟩, true⟩], "head", [(10, 21, .uint)], [(12, 1), (11, 0)]⟩]

private def proposals (scalar : Bool := false) : CalleeProposals :=
  fun id => if !scalar && id == 10 then some (256, innerLayouts) else none

private def accepts (scalar : Bool := false) (callerCode : List IR.Instruction := outerCode scalar)
    (calleeCode : List IR.Instruction := innerLoopCode) : Bool :=
  (checkSourceTargetPair (source scalar) (target scalar callerCode calleeCode) (.closure (proposals scalar) 256)).isSome

private def mutation (code : List IR.Instruction) (index : Nat)
    (replace : IR.Instruction → IR.Instruction) : List IR.Instruction :=
  code.zipIdx.map (fun (inst, position) => if position == index then replace inst else inst)

private def afterCallee (p : IR.Program) : Nat → IR.State → Option IR.State
  | 0, _ => none
  | fuel+1, state =>
    match state.control with
    | .running activation =>
      if activation.function == "outer_private" && activation.pc == 4 && state.nextFrame == 3 then some state
      else do afterCallee p fuel (← IR.step p state)
    | _ => none

private def concreteCall (scalar : Bool) (count : Nat) : Bool := Id.run do
  let p := target scalar
  let args := [Source.Value.ofNat .uint count]
  let .ok sourceEntry := Source.initial (source scalar) 0 args | return false
  let .ok targetEntry := IR.initial p "outer_private" (loopInputValues true args) | return false
  let fuel := if scalar then 256 else 64*(count+1)
  let some resumed := afterCallee p fuel targetEntry | return false
  let .running activation := resumed.control | return false
  let .ok (.memory memory) := IR.lookup activation.values (if scalar then 7 else 10) | return false
  let some caller := memory.objects.find? (·.id == 1) | return false
  if caller.owner != 1 || !caller.live || caller.bytes != IR.scalarBytes 32 (BitVec.ofNat 32 count) then return false
  if memory.objects.any (fun object => object.owner == 2 && object.live) then return false
  if memory.nextId != (if scalar then 2 else count+3) then return false
  let sf := Source.runSteps (source scalar) (if scalar then 256 else 256*(count+1)) sourceEntry
  let tf := IR.runSteps p 32 resumed
  let expected := (if scalar then 2*count else count) % 2^32
  if observeSource sf != some (.returned .uint expected) || observeTarget .uint tf != some (.returned .uint expected) then return false
  let .halted (.ok [_, .memory returned, .world]) := tf.control | return false
  return tf.nextFrame == 3 && returned.nextId == memory.nextId && returned.objects.length == memory.objects.length &&
    returned.objects.all (fun object => !object.live) && decide (returned.objects.map IR.Object.id).Nodup

private def callerLoopCondition : Source.Expr := .binary .int .ne (.ident .uint 2) (.literal .uint 0)

private def callerLoopBody : Source.Stmt := .expression
  (.assign .uint .set 2 ⟨.uint, false⟩ (.ident .uint 2)
    (.call .uint 10 [(⟨.uint, false⟩, .ident .uint 2)]))

private def callerLoopReturn : Source.Stmt := .return ⟨.uint, false⟩ (.ident .uint 3)

private def callerLoopFunction : Source.Function :=
  ⟨0, "outer_private", ⟨.uint, false⟩, false, [⟨1, ⟨.uint, false⟩, .none⟩], .compound [
    .declaration ⟨2, "remaining", ⟨.uint, false⟩, .none, some (⟨.uint, false⟩, .ident .uint 1)⟩,
    .declaration ⟨3, "saved", ⟨.uint, false⟩, .none, some (⟨.uint, false⟩, .ident .uint 1)⟩,
    .statement (.while callerLoopCondition callerLoopBody), .statement callerLoopReturn]⟩

private def callerLoopSource : Source.Program :=
  ⟨[0], [⟨10, "inner_private", ⟨.uint, false⟩, false, [⟨20, ⟨.uint, false⟩, .none⟩], .compound [
      .declaration ⟨21, "copied", ⟨.uint, false⟩, .none, some (⟨.uint, false⟩, .ident .uint 20)⟩,
      .statement (.return ⟨.uint, false⟩ (.binary .uint .sub (.ident .uint 21) (.literal .uint 1)))]⟩,
    callerLoopFunction]⟩

private def callerLoopCall : IR.Instruction :=
  ⟨[⟨35, .bits 32⟩, ⟨36, .memory⟩, ⟨37, .world⟩], .call "inner_private", [32, 33, 34]⟩

private def callerLoopTarget (call : IR.Instruction := callerLoopCall) : IR.Program :=
  ⟨[⟨"outer_private", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, memoryEffects, [
    ⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], [
      ⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 2 4 4, [1, 2]⟩,
      ⟨[⟨13, .memory⟩], .access 4 4, [4, 3]⟩,
      ⟨[⟨6, .memory⟩, ⟨7, .world⟩], .store, [13, 5, 3, 0]⟩,
      ⟨[⟨8, .pointer⟩, ⟨9, .memory⟩, ⟨10, .world⟩], .allocate 3 4 4, [6, 7]⟩,
      ⟨[⟨14, .memory⟩], .access 4 4, [9, 8]⟩,
      ⟨[⟨11, .memory⟩, ⟨12, .world⟩], .store, [14, 10, 8, 0]⟩], .branch ⟨"head", [3, 8, 11, 12]⟩⟩,
    ⟨"head", [⟨20, .pointer⟩, ⟨21, .pointer⟩, ⟨22, .memory⟩, ⟨23, .world⟩], [
      ⟨[⟨29, .memory⟩], .access 4 4, [22, 20]⟩,
      ⟨[⟨24, .bits 32⟩, ⟨25, .memory⟩, ⟨26, .world⟩], .load 32, [29, 23, 20]⟩,
      ⟨[⟨27, .bits 32⟩], .constant 32 0, []⟩,
      ⟨[⟨28, .bits 1⟩], .scalar (.cmp .ne 32), [24, 27]⟩],
      .conditional 28 ⟨"body", [20, 21, 24, 25, 26]⟩ ⟨"exit", [21, 25, 26]⟩⟩,
    ⟨"body", [⟨30, .pointer⟩, ⟨31, .pointer⟩, ⟨32, .bits 32⟩, ⟨33, .memory⟩, ⟨34, .world⟩], [call,
      ⟨[⟨46, .memory⟩], .access 4 4, [36, 30]⟩,
      ⟨[⟨38, .memory⟩, ⟨39, .world⟩], .store, [46, 37, 30, 35]⟩], .branch ⟨"head", [30, 31, 38, 39]⟩⟩,
    ⟨"exit", [⟨40, .pointer⟩, ⟨41, .memory⟩, ⟨42, .world⟩], [
      ⟨[⟨47, .memory⟩], .access 4 4, [41, 40]⟩,
      ⟨[⟨43, .bits 32⟩, ⟨44, .memory⟩, ⟨45, .world⟩], .load 32, [47, 42, 40]⟩], .return [43, 44, 45]⟩]⟩,
    ⟨"inner_private", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, memoryEffects, [
      ⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], [
        ⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 21 4 4, [1, 2]⟩,
        ⟨[⟨13, .memory⟩], .access 4 4, [4, 3]⟩,
        ⟨[⟨6, .memory⟩, ⟨7, .world⟩], .store, [13, 5, 3, 0]⟩,
        ⟨[⟨14, .memory⟩], .access 4 4, [6, 3]⟩,
        ⟨[⟨8, .bits 32⟩, ⟨9, .memory⟩, ⟨10, .world⟩], .load 32, [14, 7, 3]⟩,
        ⟨[⟨11, .bits 32⟩], .constant 32 1, []⟩,
        ⟨[⟨12, .bits 32⟩], .scalar (.bin .sub 32), [8, 11]⟩], .return [12, 9, 10]⟩]⟩]⟩

private def callerLoopLayouts : List CutpointLayout := [
  ⟨.statement callerLoopFunction.body [.function none .uint], [⟨1, ⟨.uint, false⟩, true⟩],
    "entry", [(0, 1, .uint)], [(2, 1), (1, 0)]⟩,
  ⟨.expression callerLoopCondition [.loopTest callerLoopCondition none callerLoopBody,
      .sequence [.statement callerLoopReturn], .scope 1, .function none .uint],
    [⟨3, ⟨.uint, false⟩, true⟩, ⟨2, ⟨.uint, false⟩, true⟩, ⟨1, ⟨.uint, false⟩, true⟩],
    "head", [(21, 3, .uint), (20, 2, .uint)], [(23, 1), (22, 0)]⟩]

private def callerLoopPlans : CalleeProposals := fun id => if id == 0 then some (256, callerLoopLayouts) else none

private def renamedSource : Source.Program :=
  {(source) with functions := (source).functions.map (fun f => {f with name := f.name ++ "_unlisted"})}

private def renamedTarget : IR.Program :=
  {(target) with functions := (target).functions.map (fun f =>
    {f with name := f.name ++ "_unlisted", blocks := f.blocks.map (fun block =>
      {block with instructions := block.instructions.map (fun inst =>
        match inst.op with
        | .call name => {inst with op := .call (name ++ "_unlisted")}
        | _ => inst)})})}

private def concreteCallerLoop (count : Nat) : Bool := Id.run do
  let args := [Source.Value.ofNat .uint count]
  let .ok se := Source.initial callerLoopSource 0 args | return false
  let .ok te := IR.initial (callerLoopTarget) "outer_private" (loopInputValues true args) | return false
  let sf := Source.runSteps callerLoopSource (256*(count+1)) se
  let tf := IR.runSteps (callerLoopTarget) (64*(count+1)) te
  if observeSource sf != some (.returned .uint count) || observeTarget .uint tf != some (.returned .uint count) then return false
  let .halted (.ok [_, .memory memory, .world]) := tf.control | return false
  let some saved := memory.objects.find? (·.id == 2) | return false
  return tf.nextFrame == count+2 && memory.nextId == count+3 && memory.objects.length == count+2 &&
    saved.bytes == IR.scalarBytes 32 (BitVec.ofNat 32 count) && saved.owner == 1 &&
    memory.objects.all (fun object => !object.live) && decide (memory.objects.map IR.Object.id).Nodup

def privateCallSelfTest : IO UInt32 := do
  let badCallTokens := mutation (outerCode false) 3 fun inst => {inst with operands := [0, 8, 7]}
  let staleCallerMemory := mutation (outerCode false) 4 fun inst => {inst with operands := [7, 3]}
  let staleWorld := mutation (outerCode false) 5 fun inst => {inst with operands := [12, 8, 3]}
  let badCalleeUpdate := mutation innerLoopCode 6 fun inst => {inst with op := .scalar (.bin .add 32)}
  let killPersistent := mutation innerLoopCode 9 fun inst => {inst with operands := [37, 38, 20]}
  let wrongReturn := mutation (outerCode false) 6 fun inst => {inst with operands := [9, 9]}
  let controls := [
    ("private source closure validates", (Source.checkWellFormed (source)).isOk),
    ("private target closure validates", (IR.checkProgram (target)).isOk),
    ("cyclic private callee with live caller storage admits through dispatcher", accepts),
    ("unlisted renamed private caller and callee admit through dispatcher", (checkSourceTargetPair
      renamedSource renamedTarget (.closure (proposals) 256)).isSome),
    ("finite private builder alone refuses call", (privateFinitePair (source) (target) 256).isNone),
    ("scalar-call context component retains caller private storage", (privateCallingFunctionClosure
      (source true) (target true) (proposals true) 256 3 outerSource ((target true).functions[0]'(by simp [target]))).isSome),
    ("scalar-only call remains outside public core token protocol", (checkSourceTargetPair (source true) (target true) .finite).isNone &&
      (checkSourceTargetPair (source true) (target true) (.closure (proposals true) 256)).isNone),
    ("cyclic callee without supplied checkpoint refuses", (checkSourceTargetPair (source) (target) (.closure (fun _ => none) 256)).isNone),
    ("explicit rejected private callee plan has no fallback", (checkSourceTargetPair (source) (target)
      (.closure (fun id => if id == 10 then some (0, []) else none) 256)).isNone),
    ("swapped actual call tokens refuse", !accepts false badCallTokens),
    ("stale caller memory after callee return refuses", !accepts false staleCallerMemory),
    ("stale caller world after callee return refuses", !accepts false staleWorld),
    ("wrong symbolic caller return refuses", !accepts false wrongReturn),
    ("wrong callee loop update refuses", !accepts false (outerCode false) badCalleeUpdate),
    ("callee persistent lifetime violation refuses", !accepts false (outerCode false) killPersistent),
    ("callee forged live-cell source association refuses", (checkSourceTargetPair (source) (target)
      (.closure (fun id => if id == 10 then some (256, innerLayouts.map fun point =>
        if point.label == "head" then {point with links := [(10, 20, .uint)]} else point) else none) 256)).isNone),
    ("private caller loop with a fresh private callee on every backedge admits", (checkSourceTargetPair
      callerLoopSource (callerLoopTarget) (.closure callerLoopPlans 256)).isSome),
    ("memory caller loop source validates", (Source.checkWellFormed callerLoopSource).isOk),
    ("memory caller loop guarded IR validates", (IR.checkProgram (callerLoopTarget)).isOk),
    ("private caller loop cannot omit its own plan", (checkSourceTargetPair callerLoopSource (callerLoopTarget)
      (.closure (fun _ => none) 256)).isNone),
    ("private caller loop call argument token mutation refuses", (checkSourceTargetPair callerLoopSource
      (callerLoopTarget {callerLoopCall with operands := [32, 34, 33]}) (.closure callerLoopPlans 256)).isNone)] ++
    [0, 1, 5, 17].map (fun count => (s!"caller bytes and callee finalization after {count} iterations", concreteCall false count)) ++
    [0, 1, 0xffffffff].map (fun count => (s!"scalar return preserves caller bytes for {count}", concreteCall true count)) ++
    [0, 1, 5, 17].map (fun count => (s!"private caller backedge keeps saved bytes across {count} fresh call frames", concreteCallerLoop count))
  let mut failures : UInt32 := 0
  for (name, passed) in controls do
    if !passed then
      IO.eprintln s!"private call control failed: {name}"
      failures := failures+1
  if failures == 0 then IO.println s!"{controls.length} private call controls pass"
  return failures

end EvoIR.Core.Symbolic.PrivateCallTests
