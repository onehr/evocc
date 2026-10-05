import EvoIR.MachineStep
import EvoIR.MachineEntry

/-!
Independent demand checking for the actual pre-allocation SSA machine. A
proposed needed-register set is checked on every actual instruction, edge and
direct call. The producer's demand worklist is not replayed or trusted.

Demand agreement is deliberately separate from initialized operand kinds:
gpr64 alone does not assert an integer, and pointer identity flags are not
silently treated as total ordered NZCV. Local pure noninterference consumes
the independently proved typed/readable invariant.
-/
namespace EvoIR.Machine.Demand

structure FunctionDemand where
  function : String
  needed : List Nat
  deriving DecidableEq, Repr

abbrev Plan := List FunctionDemand

inductive Aligned (related : α → β → Prop) : List α → List β → Prop where
  | nil : Aligned related [] []
  | cons {a b as bs} : related a b → Aligned related as bs → Aligned related (a :: as) (b :: bs)

theorem Aligned.length_eq {related : α → β → Prop} {as : List α} {bs : List β}
    (aligned : Aligned related as bs) : as.length = bs.length := by
  induction aligned with
  | nil => rfl
  | cons _ _ ih => exact congrArg Nat.succ ih

def needed (plan : Plan) (function : String) : List Nat :=
  ((plan.find? (·.function == function)).map FunctionDemand.needed).getD []

def allNeeded (ids : List Nat) (regs : List VReg) : Bool :=
  regs.all (fun reg => decide (reg.id ∈ ids))

def allUnneeded (ids : List Nat) (regs : List VReg) : Bool :=
  regs.all (fun reg => decide (reg.id ∉ ids))

def checkLocalInstruction (ids : List Nat) (inst : Instruction) : Bool :=
  (inst.pure && allUnneeded ids inst.defs) || allNeeded ids inst.uses

def InstructionClosed (ids : List Nat) (inst : Instruction) : Prop :=
  (inst.pure = true ∧ ∀ reg ∈ inst.defs, reg.id ∉ ids) ∨
    ∀ reg ∈ inst.uses, reg.id ∈ ids

theorem checkLocalInstruction_sound (ids : List Nat) (inst : Instruction)
    (accepted : checkLocalInstruction ids inst = true) : InstructionClosed ids inst := by
  simpa [checkLocalInstruction, InstructionClosed, allUnneeded, allNeeded,
    Bool.or_eq_true, Bool.and_eq_true, List.all_eq_true] using accepted

/-- The destination/source correspondence is positional and exact. Arity is
part of this recursive check; no trailing argument may disappear. -/
def checkParameters (destinationNeeded sourceNeeded : List Nat) : List VReg → List VReg → Bool
  | [], [] => true
  | dst :: destinations, src :: sources =>
    (if dst.id ∈ destinationNeeded then decide (src.id ∈ sourceNeeded) else true) &&
      checkParameters destinationNeeded sourceNeeded destinations sources
  | _, _ => false

theorem checkParameters_sound (destinationNeeded sourceNeeded : List Nat)
    (destinations sources : List VReg)
    (accepted : checkParameters destinationNeeded sourceNeeded destinations sources = true) :
    Aligned (fun dst src => dst.id ∈ destinationNeeded → src.id ∈ sourceNeeded)
      destinations sources := by
  induction destinations generalizing sources with
  | nil =>
    cases sources with
    | nil => exact .nil
    | cons _ _ => simp [checkParameters] at accepted
  | cons dst rest ih =>
    cases sources with
    | nil => simp [checkParameters] at accepted
    | cons src tail =>
      simp only [checkParameters, Bool.and_eq_true] at accepted
      refine .cons ?_ (ih tail accepted.2)
      intro used
      simpa [used] using accepted.1

def controlUses : Terminator → List VReg
  | .b _ _ | .brk _ => []
  | .bCond flags _ _ _ _ _ => [flags]
  | .cb _ src _ _ _ _ | .tb _ src _ _ _ _ _ => [src]
  | .ret values => values

def checkEdge (function : Function) (ids : List Nat) (edge : Nat × List VReg) : Bool :=
  match function.blocks.find? (·.id == edge.1) with
  | none => false
  | some block => checkParameters ids ids block.args edge.2

def checkCall (program : Program) (plan : Plan) (caller : Function)
    (symbol : String) (args : List VReg) : Bool :=
  match program.functions.find? (·.name == symbol) with
  | none => false
  | some callee =>
    match callee.blocks.find? (·.id == 0) with
    | none => false
    | some entry => checkParameters (needed plan callee.name) (needed plan caller.name) entry.args args

def checkInstruction (program : Program) (plan : Plan) (function : Function) : Instruction → Bool
  | .bl symbol args _ => checkCall program plan function symbol args
  | inst => checkLocalInstruction (needed plan function.name) inst

def checkBlock (program : Program) (plan : Plan) (function : Function) (block : Block) : Bool :=
  block.insts.all (checkInstruction program plan function) &&
    allNeeded (needed plan function.name) (controlUses block.term) &&
    block.term.edges.all (checkEdge function (needed plan function.name))

/-- Finite syntactic closure only, not yet an admission receipt for the
whole noninterference theorem. Both the exact machine and a checked typing
invariant remain visible in the semantic statements below. -/
def checkStructure (program : Program) (plan : Plan) : Bool :=
  decide (plan.map FunctionDemand.function = program.functions.map Function.name) &&
    decide ((program.functions.map Function.name).eraseDups.length = program.functions.length) &&
    program.functions.all (fun function => function.blocks.all (checkBlock program plan function))

theorem checkStructure_instruction (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true)
    (function : Function) (functionMember : function ∈ program.functions)
    (block : Block) (blockMember : block ∈ function.blocks)
    (inst : Instruction) (instMember : inst ∈ block.insts) :
    checkInstruction program plan function inst = true := by
  simp only [checkStructure, Bool.and_eq_true, List.all_eq_true] at accepted
  have checked := accepted.2 function functionMember block blockMember
  simp only [checkBlock, Bool.and_eq_true, List.all_eq_true] at checked
  exact checked.1.1 inst instMember

theorem checkStructure_controls (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true)
    (function : Function) (functionMember : function ∈ program.functions)
    (block : Block) (blockMember : block ∈ function.blocks) :
    ∀ reg ∈ controlUses block.term, reg.id ∈ needed plan function.name := by
  simp only [checkStructure, Bool.and_eq_true, List.all_eq_true] at accepted
  have checked := accepted.2 function functionMember block blockMember
  simp only [checkBlock, Bool.and_eq_true, List.all_eq_true, allNeeded,
    decide_eq_true_eq] at checked
  exact checked.1.2

theorem checkStructure_edge (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true)
    (function : Function) (functionMember : function ∈ program.functions)
    (block : Block) (blockMember : block ∈ function.blocks)
    (edge : Nat × List VReg) (edgeMember : edge ∈ block.term.edges) :
    checkEdge function (needed plan function.name) edge = true := by
  simp only [checkStructure, Bool.and_eq_true, List.all_eq_true] at accepted
  have checked := accepted.2 function functionMember block blockMember
  simp only [checkBlock, Bool.and_eq_true, List.all_eq_true] at checked
  exact checked.2 edge edgeMember

@[simp] theorem mapUses_id (inst : Instruction) : inst.mapUses id = inst := by
  cases inst <;> simp [Instruction.mapUses]

theorem RegistersAgree.operand (ids : List Nat) (source target : RegisterFile)
    (agree : RegistersAgree ids source target) (reg : VReg) (used : reg.id ∈ ids) :
    readOperand source reg = readOperand target reg := by
  simp only [readOperand, readSlot, agree reg.id used]

def EvaluationRelated (ids : List Nat) : Except Core.Source.Fault (RegisterFile × Memory) →
    Except Core.Source.Fault (RegisterFile × Memory) → Prop
  | .ok (left, memory), .ok (right, memory') => memory = memory' ∧ RegistersAgree ids left right
  | .error fault, .error fault' => fault = fault'
  | _, _ => False

theorem instruction_reads_noninterference (owner : Nat) (inst : Instruction) (ids : List Nat)
    (source target : RegisterFile) (memory : Memory)
    (used : ∀ reg ∈ inst.uses, reg.id ∈ ids) (agree : RegistersAgree ids source target) :
    EvaluationRelated ids (evalInstruction owner inst source memory)
      (evalInstruction owner inst target memory) := by
  have inputs : ∀ reg ∈ inst.uses, readOperand target reg = readOperand source reg := by
    intro reg member
    exact (RegistersAgree.operand ids source target agree reg (used reg member)).symm
  have effects := evalInstructionOperands_mapUses owner inst id
    (readOperand source) (readOperand target) memory inputs
  simp only [mapUses_id] at effects
  cases sourceEffects : evalInstructionOperands owner inst (readOperand source) memory with
  | error fault =>
    simp only [sourceEffects] at effects
    simp only [evalInstruction, sourceEffects, effects, Bind.bind, Except.bind, EvaluationRelated]
  | ok output =>
    rcases output with ⟨writes, nextMemory⟩
    simp only [sourceEffects] at effects
    simp only [evalInstruction, sourceEffects, effects, Bind.bind, Except.bind,
      Pure.pure, Except.pure, EvaluationRelated]
    refine ⟨True.intro, ?_⟩
    intro id member
    exact applyWrites_lookup_congr source target writes ⟨id, .gpr64⟩ (agree id member)

/-- Exact local demand noninterference. Retained/observable instructions
execute identically from demanded reads. Discarded pure instructions execute
successfully on both checked typed states and cannot modify demanded slots.
All opcode/width/memory agreement is proved through the actual dispatcher. -/
theorem instruction_noninterference (owner : Nat) (inst : Instruction) (ids : List Nat)
    (source target : RegisterFile) (memory : Memory)
    (closed : InstructionClosed ids inst) (agree : RegistersAgree ids source target)
    (typed : InstClassWellFormed inst)
    (sourceReady : IntegerOperands inst (readOperand source))
    (targetReady : IntegerOperands inst (readOperand target)) :
    EvaluationRelated ids (evalInstruction owner inst source memory)
      (evalInstruction owner inst target memory) := by
  rcases closed with ⟨pure, outside⟩ | used
  · let live := ids.map fun id => VReg.mk id .gpr64
    have disjoint : ∀ reg ∈ live, ∀ dst ∈ inst.defs, reg.id ≠ dst.id := by
      intro reg member dst defined same
      obtain ⟨id, idMember, rfl⟩ := List.mem_map.mp member
      have equal : id = dst.id := same
      exact outside dst defined (equal ▸ idMember)
    obtain ⟨afterSource, hs, sourceLive⟩ := evalInstruction_drop_unobserved owner inst
      source memory live pure typed sourceReady disjoint
    obtain ⟨afterTarget, ht, targetLive⟩ := evalInstruction_drop_unobserved owner inst
      target memory live pure typed targetReady disjoint
    rw [hs, ht]
    refine ⟨rfl, ?_⟩
    intro id member
    have liveMember : VReg.mk id .gpr64 ∈ live := List.mem_map.mpr ⟨id, member, rfl⟩
    exact (sourceLive _ liveMember).trans ((agree id member).trans (targetLive _ liveMember).symm)
  · exact instruction_reads_noninterference owner inst ids source target memory used agree

theorem read_agree (ids : List Nat) (source target : RegisterFile)
    (agree : RegistersAgree ids source target) (reg : VReg) (used : reg.id ∈ ids) :
    read source reg = read target reg := by
  simp only [read, readSlot, agree reg.id used]

theorem readFlags_agree (ids : List Nat) (source target : RegisterFile)
    (agree : RegistersAgree ids source target) (reg : VReg) (used : reg.id ∈ ids) :
    readFlags source reg = readFlags target reg := by
  simp only [readFlags, readSlot, agree reg.id used]

theorem readBits_agree (ids : List Nat) (source target : RegisterFile)
    (agree : RegistersAgree ids source target) (reg : VReg) (used : reg.id ∈ ids) :
    readBits source reg = readBits target reg := by
  simp only [readBits, read_agree ids source target agree reg used]

theorem read_list_total (regs : RegisterFile) (args : List VReg)
    (ready : ∀ reg ∈ args, ∃ value, read regs reg = .ok value) :
    ∃ values, args.mapM (read regs) = .ok values ∧ values.length = args.length := by
  induction args with
  | nil => exact ⟨[], rfl, rfl⟩
  | cons reg rest ih =>
    obtain ⟨value, readValue⟩ := ready reg (by simp)
    obtain ⟨values, readValues, arity⟩ := ih (by intro reg member; exact ready reg (by simp [member]))
    refine ⟨value :: values, ?_, by simp [arity]⟩
    simp only [List.mapM_cons, readValue, readValues]
    rfl

theorem read_parameter_agreement (destinationNeeded sourceNeeded : List Nat)
    (source target : RegisterFile) (agree : RegistersAgree sourceNeeded source target)
    (params args : List VReg)
    (closed : Aligned (fun dst src => dst.id ∈ destinationNeeded → src.id ∈ sourceNeeded) params args)
    (sourceValues targetValues : List Value)
    (sourceRead : args.mapM (read source) = .ok sourceValues)
    (targetRead : args.mapM (read target) = .ok targetValues) :
    BindingAgreement destinationNeeded (params.zip sourceValues) (params.zip targetValues) := by
  induction closed generalizing sourceValues targetValues with
  | nil =>
    simp only [List.mapM_nil, pure, Except.pure, Except.ok.injEq] at sourceRead targetRead
    subst sourceValues
    subst targetValues
    exact .nil
  | @cons param arg params args neededArg tail ih =>
    simp only [List.mapM_cons, Bind.bind, Except.bind] at sourceRead targetRead
    cases sourceHead : read source arg <;> cases targetHead : read target arg <;>
      simp only [sourceHead, targetHead] at sourceRead targetRead
    all_goals try contradiction
    next sourceValue targetValue =>
      cases sourceTail : args.mapM (read source) <;> cases targetTail : args.mapM (read target) <;>
        simp only [sourceTail, targetTail, Pure.pure, Except.pure,
          Except.ok.injEq] at sourceRead targetRead
      all_goals try contradiction
      next sourceRest targetRest =>
        subst sourceValues
        subst targetValues
        refine .cons param sourceValue targetValue ?_ (ih sourceRest targetRest sourceTail targetTail)
        intro used
        have same := read_agree sourceNeeded source target agree arg (neededArg used)
        rw [sourceHead, targetHead] at same
        exact congrArg (normalize param.class) (Except.ok.inj same)

structure ActivationRelated (plan : Plan) (source target : Activation) : Prop where
  function : source.function = target.function
  block : source.block = target.block
  pc : source.pc = target.pc
  owner : source.owner = target.owner
  regs : RegistersAgree (needed plan source.function) source.regs target.regs

structure FrameRelated (plan : Plan) (source target : ReturnFrame) : Prop where
  caller : ActivationRelated plan source.caller target.caller
  results : source.results = target.results

inductive ControlRelated (plan : Plan) : Control → Control → Prop where
  | running {source target} : ActivationRelated plan source target →
      ControlRelated plan (.running source) (.running target)
  | halted (result : Except Core.Source.Fault (List Value)) :
      ControlRelated plan (.halted result) (.halted result)

structure StateRelated (plan : Plan) (source target : State) : Prop where
  control : ControlRelated plan source.control target.control
  stack : Aligned (FrameRelated plan) source.stack target.stack
  nextFrame : source.nextFrame = target.nextFrame
  memory : source.memory = target.memory

theorem jump_related (plan : Plan) (function : Function) (source target : Activation)
    (related : ActivationRelated plan source target) (functionName : function.name = source.function)
    (destination : Nat) (args : List VReg) (block : Block)
    (found : function.blocks.find? (·.id == destination) = some block)
    (unique : (block.args.map VReg.id).eraseDups.length = block.args.length)
    (closed : checkParameters (needed plan function.name) (needed plan function.name) block.args args = true)
    (sourceReady : ∀ reg ∈ args, ∃ value, read source.regs reg = .ok value)
    (targetReady : ∀ reg ∈ args, ∃ value, read target.regs reg = .ok value) :
    ∃ afterSource afterTarget,
      jump .ssa function source destination args = .ok afterSource ∧
      jump .ssa function target destination args = .ok afterTarget ∧
      ActivationRelated plan afterSource afterTarget := by
  have parameters := checkParameters_sound _ _ block.args args closed
  have arity := parameters.length_eq
  obtain ⟨sourceValues, sourceRead, sourceLength⟩ := read_list_total source.regs args sourceReady
  obtain ⟨targetValues, targetRead, targetLength⟩ := read_list_total target.regs args targetReady
  have registers : RegistersAgree (needed plan function.name) source.regs target.regs := by
    simpa only [functionName] using related.regs
  have values := read_parameter_agreement _ _ source.regs target.regs registers
    block.args args parameters sourceValues targetValues sourceRead targetRead
  obtain ⟨sourceRegs, targetRegs, sourceBind, targetBind, bound⟩ := bind_agree
    (needed plan function.name) [] [] (by intro id used; rfl) block.args sourceValues targetValues
    (arity.trans sourceLength.symm) (arity.trans targetLength.symm) unique values
  refine ⟨{source with block := destination, pc := 0, regs := sourceRegs},
    {target with block := destination, pc := 0, regs := targetRegs}, ?_, ?_, ?_⟩
  · simp only [jump, found, sourceRead]
    change (do let regs ← bind [] block.args sourceValues
               pure {source with block := destination, pc := 0, regs}) = _
    rw [sourceBind]
    rfl
  · simp only [jump, found, targetRead]
    change (do let regs ← bind [] block.args targetValues
               pure {target with block := destination, pc := 0, regs}) = _
    rw [targetBind]
    rfl
  · exact ⟨related.function, rfl, rfl, related.owner, by simpa only [functionName] using bound⟩

theorem enter_related (plan : Plan) (function : Function) (block : Block)
    (found : function.blocks.find? (·.id == 0) = some block) (owner : Nat)
    (sourceValues targetValues : List Value)
    (sourceArity : block.args.length = sourceValues.length)
    (targetArity : block.args.length = targetValues.length)
    (unique : (block.args.map VReg.id).eraseDups.length = block.args.length)
    (values : BindingAgreement (needed plan function.name)
      (block.args.zip sourceValues) (block.args.zip targetValues)) :
    ∃ source target, enter function sourceValues owner = .ok source ∧
      enter function targetValues owner = .ok target ∧ ActivationRelated plan source target := by
  obtain ⟨sourceRegs, targetRegs, sourceBind, targetBind, agree⟩ := bind_agree
    (needed plan function.name) [] [] (by intro id used; rfl) block.args sourceValues targetValues
    sourceArity targetArity unique values
  refine ⟨⟨function.name, block.id, 0, sourceRegs, owner, sourceValues⟩,
    ⟨function.name, block.id, 0, targetRegs, owner, targetValues⟩, ?_, ?_, ?_⟩
  · simp only [enter, found, sourceBind]
    rfl
  · simp only [enter, found, targetBind]
    rfl
  · exact ⟨rfl, rfl, rfl, rfl, agree⟩

end EvoIR.Machine.Demand
