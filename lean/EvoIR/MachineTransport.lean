import EvoIR.MachineCopies
import EvoIR.MachineTransportInstruction

/-!
Executable structural obligations for an actual SSA/postSSA function pair.
Annotations propose demanded registers, entry representatives and instruction
positions; every proposed correspondence is checked against both AST inputs.
No forwarding worklist, DCE fixed point or copy-scheduling producer is replayed.

`checkStructure` is the function-local syntax check. Whole-program admission
is `checkProgram` in MachineTransportTable, whose actual-machine soundness
theorem is MachineTransportSound.sound. It consumes the independently proved
preceding SSA progress; no semantic invariant is accepted in annotations.
-/
namespace EvoIR.Machine.Transport

inductive Origin where
  | sourceInput (reg : VReg)
  | targetInput (reg : VReg)
  | result (block pc output : Nat) (regClass : RegClass)
  deriving DecidableEq, Repr

def Origin.class : Origin → RegClass
  | .sourceInput reg | .targetInput reg => reg.class
  | .result _ _ _ regClass => regClass

abbrev Origins := List (Nat × Origin)

def origin (env : Origins) (reg : VReg) : Option Origin := do
  let value ← env.lookup reg.id
  if value.class = reg.class then some value else none

def originWrite (env : Origins) (reg : VReg) (value : Origin) : Origins :=
  (reg.id, value) :: env

def resultOrigins (block pc : Nat) : Nat → List VReg → Origins → Origins
  | _, [], env => env
  | index, reg :: rest, env =>
    resultOrigins block pc (index+1) rest
      (originWrite env reg (.result block pc index reg.class))

def copyOrigins : List Instruction → Origins → Option Origins
  | [], env => some env
  | .mov dst src :: rest, env => do
    if dst.class = src.class ∧ dst.class ≠ .flags then do
      let value ← origin env src
      copyOrigins rest (originWrite env dst value)
    else none
  | _ :: _, _ => none

structure BlockPlan where
  block : Nat
  prefixLength : Nat
  bindings : UseMap
  /-- Actual target instruction positions, not reconstructed instructions. -/
  alignment : List (Option Nat)
  deriving DecidableEq, Repr

structure FunctionPlan where
  needed : List Nat
  blocks : List BlockPlan
  deriving DecidableEq, Repr

def unique (ids : List Nat) : Bool := decide (ids.eraseDups.length = ids.length)

def allNeeded (needed : List Nat) (regs : List VReg) : Bool :=
  regs.all (fun reg => decide (reg.id ∈ needed))

def controlUses : Terminator → List VReg
  | .b _ _ | .brk _ => []
  | .bCond flags _ _ _ _ _ => [flags]
  | .cb _ src _ _ _ _ | .tb _ src _ _ _ _ _ => [src]
  | .ret values => values

private def allUses (term : Terminator) : List VReg :=
  controlUses term ++ term.edges.flatMap Prod.snd

def controlShape : Terminator → Terminator
  | .b _ _ => .b 0 []
  | .bCond flags cond _ _ _ _ => .bCond ⟨0, flags.class⟩ cond 0 [] 0 []
  | .cb nonzero src _ _ _ _ => .cb nonzero ⟨0, src.class⟩ 0 [] 0 []
  | .tb nonzero src bit _ _ _ _ => .tb nonzero ⟨0, src.class⟩ bit 0 [] 0 []
  | .ret values => .ret (values.map fun reg => ⟨0, reg.class⟩)
  | .brk imm => .brk imm

def operandOrigins (env : Origins) (regs : List VReg) : Option (List Origin) :=
  regs.mapM (origin env)

def matchControls (source target : Terminator) (sourceEnv targetEnv : Origins) : Bool :=
  decide (controlShape source = controlShape target) &&
    match operandOrigins sourceEnv (controlUses source), operandOrigins targetEnv (controlUses target) with
    | some left, some right => decide (left = right)
    | _, _ => false

def useCorrespondence (source target : Instruction) : UseMap :=
  (source.uses.zip target.uses).map fun (left, right) => (left.id, right)

def matchOrigins (source target : Instruction) (sourceEnv targetEnv : Origins) : Bool :=
  let mapping := useCorrespondence source target
  checkRetained mapping source target &&
    source.uses.all (fun reg =>
      match origin sourceEnv reg, origin targetEnv (rename mapping reg) with
      | some left, some right => decide (left = right)
      | _, _ => false)

/-- All values asserted by a symbolic environment are actual readable slots.
Unknown target registers have no entry and cannot justify an operand read. -/
def Realizes (env : Origins) (valuation : Origin → Slot) (regs : RegisterFile) : Prop :=
  ∀ reg value, origin env reg = some value → readOperand regs reg = .ok (valuation value)

theorem matchOrigins_retained (source target : Instruction) (sourceEnv targetEnv : Origins)
    (accepted : matchOrigins source target sourceEnv targetEnv = true) :
    checkRetained (useCorrespondence source target) source target = true := by
  simp only [matchOrigins, Bool.and_eq_true] at accepted
  exact accepted.1

theorem matchOrigins_reads (source target : Instruction) (sourceEnv targetEnv : Origins)
    (accepted : matchOrigins source target sourceEnv targetEnv = true)
    (valuation : Origin → Slot) (sourceRegs targetRegs : RegisterFile)
    (sourceReal : Realizes sourceEnv valuation sourceRegs)
    (targetReal : Realizes targetEnv valuation targetRegs) :
    ∀ reg ∈ source.uses,
      readOperand targetRegs (rename (useCorrespondence source target) reg) =
        readOperand sourceRegs reg := by
  intro reg used
  simp only [matchOrigins, Bool.and_eq_true, List.all_eq_true] at accepted
  have equal := accepted.2 reg used
  cases sourceValue : origin sourceEnv reg with
  | none => simp [sourceValue] at equal
  | some sourceOrigin =>
    cases targetValue : origin targetEnv (rename (useCorrespondence source target) reg) with
    | none => simp [sourceValue, targetValue] at equal
    | some targetOrigin =>
      have same : sourceOrigin = targetOrigin := by simpa [sourceValue, targetValue] using equal
      rw [sourceReal reg sourceOrigin sourceValue, targetReal _ targetOrigin targetValue, same]

/-- This is the concrete semantic bridge used by the structural matcher, not
a theorem about an unrelated checker. Exact opcode fields and checked origin
equality establish the actual operand reader relation for every retained use. -/
theorem matchOrigins_eval (source target : Instruction) (sourceEnv targetEnv : Origins)
    (accepted : matchOrigins source target sourceEnv targetEnv = true)
    (valuation : Origin → Slot) (owner : Nat) (sourceRegs targetRegs : RegisterFile)
    (memory : Memory) (sourceReal : Realizes sourceEnv valuation sourceRegs)
    (targetReal : Realizes targetEnv valuation targetRegs) :
    evalInstructionOperands owner target (readOperand targetRegs) memory =
      evalInstructionOperands owner source (readOperand sourceRegs) memory := by
  apply checkRetained_eval (useCorrespondence source target) source target
    (matchOrigins_retained source target sourceEnv targetEnv accepted)
    owner sourceRegs targetRegs memory
  intro reg used
  simp only [matchOrigins, Bool.and_eq_true, List.all_eq_true] at accepted
  have equal := accepted.2 reg used
  cases sourceValue : origin sourceEnv reg with
  | none => simp [sourceValue] at equal
  | some sourceOrigin =>
    cases targetValue : origin targetEnv (rename (useCorrespondence source target) reg) with
    | none => simp [sourceValue, targetValue] at equal
    | some targetOrigin =>
      have same : sourceOrigin = targetOrigin := by
        simpa [sourceValue, targetValue] using equal
      rw [sourceReal reg sourceOrigin sourceValue,
        targetReal _ targetOrigin targetValue, same]

def bindingsValid (block : Block) (plan : BlockPlan) (needed : List Nat) : Bool :=
  unique (plan.bindings.map Prod.fst) &&
    plan.bindings.all (fun (id, representative) =>
      decide (id ∈ needed) &&
        block.args.any (fun reg => decide (reg.id = id ∧ reg.class = representative.class))) &&
    block.args.all (fun reg => if reg.id ∈ needed then
      (plan.bindings.lookup reg.id).isSome else true)

def sourceEntryOrigins (args : List VReg) (mapping : UseMap) : Origins :=
  args.map fun reg => (reg.id, match mapping.lookup reg.id with
    | some representative => Origin.targetInput representative
    | none => Origin.sourceInput reg)

def targetEntryOrigins (mapping : UseMap) : Origins :=
  mapping.map fun (_, representative) => (representative.id, Origin.targetInput representative)

structure Cutpoint where
  sourcePC : Nat
  targetPC : Nat
  source : Origins
  target : Origins
  deriving DecidableEq, Repr

structure BodyResult where
  source : Origins
  target : Origins
  /-- Checked continuation points, including the actual PCs saved by calls. -/
  cuts : List Cutpoint := []
  deriving DecidableEq, Repr

def checkBodyAux (block : Nat) (needed : List Nat) (actual : List Instruction) :
    Nat → Nat → List Instruction → List (Option Nat) → BodyResult → Option BodyResult
  | _, targetPC, [], [], env => do
    let target ← copyOrigins (actual.drop targetPC) env.target
    return {env with target}
  | sourcePC, targetPC, inst :: rest, position :: positions, env => do
    match position with
    | none =>
      if checkDCE needed inst then
        checkBodyAux block needed actual (sourcePC+1) targetPC rest positions
          {env with source := resultOrigins block sourcePC 0 inst.defs env.source}
      else none
    | some targetIndex =>
      if targetIndex < targetPC then none else do
        let target ← copyOrigins ((actual.drop targetPC).take (targetIndex-targetPC)) env.target
        let candidate ← actual[targetIndex]?
        if matchOrigins inst candidate env.source target then
          let nextSource := resultOrigins block sourcePC 0 inst.defs env.source
          let nextTarget := resultOrigins block sourcePC 0 candidate.defs target
          checkBodyAux block needed actual (sourcePC+1) (targetIndex+1) rest positions
            {source := nextSource, target := nextTarget,
             cuts := ⟨sourcePC+1, targetIndex+1, nextSource, nextTarget⟩ :: env.cuts}
        else none
  | _, _, _, _, _ => none

def checkBody (source target : Block) (plan : BlockPlan) (needed : List Nat) :
    Option BodyResult := do
  if source.id ≠ plan.block || source.id ≠ target.id || plan.prefixLength > target.insts.length then
    none
  else if !bindingsValid source plan needed then none else
    let sourceEnv := sourceEntryOrigins source.args plan.bindings
    let targetEnv := targetEntryOrigins plan.bindings
    checkBodyAux source.id needed target.insts 0 plan.prefixLength source.insts plan.alignment
      {source := sourceEnv, target := targetEnv,
       cuts := [⟨0, plan.prefixLength, sourceEnv, targetEnv⟩]}

def findBlock (f : Function) (id : Nat) : Option Block := f.blocks.find? (·.id == id)
def findPlan (p : FunctionPlan) (id : Nat) : Option BlockPlan := p.blocks.find? (·.block == id)

/-- Only the actual inserted blocks and actual mov instructions are followed.
The length bound detects administrative cycles; it is not an execution fuel
bound on the checked program, whose original blocks may contain backedges. -/
def routeOrigins (source target : Function) (plans : FunctionPlan) (expected : Nat) :
    Nat → Nat → Origins → Option Origins
  | 0, _, _ => none
  | fuel+1, current, env => do
    if current = expected then
      let block ← findBlock target current
      let plan ← findPlan plans current
      if plan.prefixLength > block.insts.length then none
      else copyOrigins (block.insts.take plan.prefixLength) env
    else if (findBlock source current).isSome then none else do
      let block ← findBlock target current
      if !block.args.isEmpty then none else do
        let next ← copyOrigins block.insts env
        match block.term with
        | .b successor [] => routeOrigins source target plans expected fuel successor next
        | _ => none

def edgeParameterValid (needed : List Nat) (mapping : UseMap)
    (source transported : Origins) (param value : VReg) : Bool :=
  if param.class ≠ value.class then false
  else if param.id ∈ needed then
    match mapping.lookup param.id, origin source value with
    | some representative, some expected => decide (origin transported representative = some expected)
    | _, _ => false
  else true

def edgeValid (source target : Function) (plans : FunctionPlan)
    (env : BodyResult) (original actual : Nat × List VReg) : Bool :=
  match findBlock source original.1, findPlan plans original.1 with
  | some destination, some plan =>
    if destination.args.length ≠ original.2.length || !actual.2.isEmpty then false else
    match routeOrigins source target plans original.1 (target.blocks.length+1) actual.1 env.target with
    | none => false
    | some transported =>
      (destination.args.zip original.2).all fun (param, value) =>
        edgeParameterValid plans.needed plan.bindings env.source transported param value
  | _, _ => false

private def flagsDemandValid (block : Block) (needed : List Nat) : Bool :=
  (block.insts.zip (block.insts.drop 1)).all fun (producer, consumer) =>
    if producer.defs.any (fun reg => decide (reg.class = .flags ∧ reg ∈ consumer.uses)) &&
        producer.defs.any (fun reg => decide (reg.id ∈ needed)) then
      allNeeded needed consumer.defs
    else true

private def neededValid (source : Function) (needed : List Nat) : Bool :=
  source.blocks.all fun block =>
    allNeeded needed (controlUses block.term) && flagsDemandValid block needed &&
      block.insts.all (fun inst =>
        if checkDCE needed inst then true else allNeeded needed inst.uses) &&
      block.term.edges.all (fun (id, args) => match findBlock source id with
        | none => false
        | some destination =>
          decide (destination.args.length = args.length) &&
          (destination.args.zip args).all (fun (param, value) =>
            if param.id ∈ needed then decide (value.id ∈ needed) else true))

private def registerKnown (known : List VReg) (reg : VReg) : Bool :=
  known.any (fun value => decide (value = reg))

def general (regs : List VReg) : Bool :=
  regs.all (fun reg => decide (reg.class ≠ .flags))

def instructionShapeValid : Instruction → Bool
  | .mov dst src => general [dst, src]
  | .movImm dst _ => general [dst]
  | .binary _ dst lhs rhs _ | .wideMultiply _ dst lhs rhs => general [dst, lhs, rhs]
  | .binaryImm _ dst lhs _ _ | .extend _ dst lhs | .bfx _ dst lhs _ _ _ => general [dst, lhs]
  | .flagBinary _ dst flags lhs rhs _ => general [dst, lhs, rhs] && decide (flags.class = .flags)
  | .msub dst n m a _ | .madd dst n m a _ => general [dst, n, m, a]
  | .cmp dst lhs rhs _ => decide (dst.class = .flags) && general [lhs, rhs]
  | .cmpImm dst lhs _ _ | .tstImm dst lhs _ _ => decide (dst.class = .flags) && general [lhs]
  | .cset dst flags _ => general [dst] && decide (flags.class = .flags)
  | .csel dst flags lhs rhs _ _ => general [dst, lhs, rhs] && decide (flags.class = .flags)
  | .load _ dst base _ _ volatile => general [dst] && decide (base.class = .gpr64) && !volatile
  | .store _ src base _ _ volatile => general [src] && decide (base.class = .gpr64) && !volatile
  | .frameSlot dst _ size align => decide (dst.class = .gpr64 ∧ 0 < size ∧ 0 < align)
  | .stackRelease address => decide (address.class = .gpr64)
  | .bl _ args results => general (args ++ results)

private def termShapeValid : Terminator → Bool
  | .b _ _ | .brk _ => true
  | .bCond flags _ _ _ _ _ => decide (flags.class = .flags)
  | .cb _ src _ _ _ _ => general [src]
  | .tb _ src bit _ _ _ _ => general [src] && decide (bit < 64)
  | .ret values => general values

private def flagsValid (block : Block) : Bool :=
  (block.insts.zipIdx).all fun (inst, index) =>
    (inst.defs.filter (fun reg => decide (reg.class = .flags))).all fun flag =>
      let nextUses := match block.insts[index+1]? with
        | some next => next.uses
        | none => controlUses block.term
      decide (flag ∈ nextUses) &&
        decide ((block.insts.flatMap Instruction.uses ++ controlUses block.term).count flag = 1)

private def sourceInstructionsKnown : List Instruction → List VReg → Bool
  | [], _ => true
  | inst :: rest, known =>
    inst.uses.all (registerKnown known) &&
      sourceInstructionsKnown rest (inst.defs ++ known)

def sourceBlockKnown (block : Block) : Bool :=
  block.args.all (fun reg => decide (reg.class ≠ .flags)) &&
    block.insts.all instructionShapeValid && termShapeValid block.term && flagsValid block &&
    sourceInstructionsKnown block.insts block.args &&
    (allUses block.term).all (registerKnown (block.insts.flatMap Instruction.defs ++ block.args))

private def definitions (source : Function) : List VReg :=
  source.blocks.flatMap fun block => block.args ++ block.insts.flatMap Instruction.defs

def sourceWellFormed (source : Function) : Bool :=
  unique (source.blocks.map Block.id) && unique ((definitions source).map VReg.id) &&
    source.blocks.all sourceBlockKnown &&
    (findBlock source 0).isSome

def originalBlockValid (source target : Function) (plans : FunctionPlan)
    (block : Block) : Bool :=
  match findBlock target block.id, findPlan plans block.id with
  | some candidate, some plan =>
    if block.id = 0 then
      if candidate.args ≠ block.args || plan.prefixLength ≠ 0 ||
          !plan.bindings.all (fun (id, reg) => decide (id = reg.id)) then false
      else finish candidate plan
    else if !candidate.args.isEmpty then false else finish candidate plan
  | _, _ => false
where
  finish (candidate : Block) (plan : BlockPlan) : Bool :=
    match checkBody block candidate plan plans.needed with
    | none => false
    | some env =>
      matchControls block.term candidate.term env.source env.target &&
        decide (block.term.edges.length = candidate.term.edges.length) &&
        (block.term.edges.zip candidate.term.edges).all (fun (original, actual) =>
          edgeValid source target plans env original actual)

private def administrativeBlock (block : Block) : Bool :=
  block.args.isEmpty &&
    block.insts.all (fun inst => match inst with
      | .mov dst src => decide (dst.class = src.class ∧ dst.class ≠ .flags)
      | _ => false) &&
    match block.term with | .b _ [] => true | _ => false

/-- Function-local structural validation. The whole-program theorem in
MachineTransportSound derives, rather than assumes, the execution relation. -/
def checkStructure (source target : Function) (plans : FunctionPlan) : Bool :=
  decide (source.name = target.name) && sourceWellFormed source &&
    unique (target.blocks.map Block.id) && unique (plans.blocks.map BlockPlan.block) &&
    decide (source.blocks.length = plans.blocks.length) &&
    neededValid source plans.needed &&
    source.blocks.all (originalBlockValid source target plans) &&
    target.blocks.all (fun block =>
      if (findBlock source block.id).isSome then true else administrativeBlock block)

end EvoIR.Machine.Transport
