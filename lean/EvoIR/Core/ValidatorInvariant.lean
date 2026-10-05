import EvoIR.Core.ValidatorStorage

/-!
Concrete proof interface for the independent source-validator branch.

The invariant records store facts, pending result types, exact scope suffixes,
and the checked syntax suspended by every source continuation. It does not use
compilation-pair acceptance, execution fuel, or a future no-malformed predicate.
The actual checker-to-invariant construction and exhaustive machine-step
preservation are proved in `ValidatorAdmission`, the transfer modules, and
`ValidatorSafety`. Its public consequence has no proof-closure input.
-/
namespace EvoIR.Core.Source

abbrev LocalLayout := List (Nat × QualifiedType)

def localLayout (locals : Locals) : LocalLayout :=
  locals.map (fun slot => (slot.symbol, slot.type))

/-- A pending const initializer needs its own freshness fact. Definite
initialization alone neither implies nor preserves this fact. -/
structure ValidatorEnvironment where
  state : LocalState
  layout : LocalLayout
  freshConst : List Nat := []

structure ValidatorEnvironment.Models (environment : ValidatorEnvironment)
    (locals : Locals) : Prop where
  realizes : environment.state.Realizes locals
  layout : localLayout locals = environment.layout
  distinct : (locals.map Slot.symbol).Nodup
  fresh : ∀ id ∈ environment.freshConst, ∃ slot,
    locals.find? (·.symbol == id) = some slot ∧
      slot.type.isConst = true ∧ slot.value = none

/-- This is logical weakening of concrete local facts, not an assertion about
future machine execution. -/
structure ValidatorEnvironment.Entails (strong weak : ValidatorEnvironment) : Prop where
  bindings : ∀ id, lookup strong.state id = lookup weak.state id
  layout : strong.layout = weak.layout
  initialized : ∀ id ∈ weak.state.initialized, id ∈ strong.state.initialized
  fresh : ∀ id ∈ weak.freshConst, id ∈ strong.freshConst

theorem ValidatorEnvironment.Models.weaken (strong weak : ValidatorEnvironment)
    (models : strong.Models locals) (entails : strong.Entails weak) :
    weak.Models locals := by
  refine ⟨⟨?_, models.realizes.values, ?_⟩, models.layout.trans entails.layout,
    models.distinct, ?_⟩
  · intro id
    exact (models.realizes.bindings id).trans (entails.bindings id)
  · intro id member
    exact models.realizes.initialized id (entails.initialized id member)
  · intro id member
    exact models.fresh id (entails.fresh id member)

def ValidatorEnvironment.afterExpression (environment : ValidatorEnvironment)
    (facts : ExpressionFacts) : ValidatorEnvironment :=
  {environment with state.initialized := union environment.state.initialized facts.initialized}

/-- Abrupt item execution may stop before later declarations. Its retained
suffix is the incoming scope, not the eventual normal-result layout. -/
def ValidatorEnvironment.Grown (environment : ValidatorEnvironment)
    (locals : Locals) : Prop :=
  ∃ introduced retained : Locals, locals = introduced ++ retained ∧
    environment.Models retained ∧ (locals.map Slot.symbol).Nodup ∧
    ∀ slot ∈ locals, ∀ value, slot.value = some value → value.type = slot.type.type

structure CheckedExpression (callees : List Function) (input : LocalState) (expression : Expr) where
  fuel : Nat
  facts : ExpressionFacts
  accepted : checkExpr callees fuel input expression = .ok facts

structure CheckedStatement (callees : List Function) (result : QualifiedType)
    (depth : Nat) (input : LocalState) (statement : Stmt) where
  fuel : Nat
  facts : FlowFacts
  accepted : checkStmt callees result fuel depth input statement = .ok facts

/-- All supplied definitions are checked in their actual preceding closure.
The ordering witness also explains why a permitted callee is a definition in
the complete program, rather than an assumed external implementation. -/
structure CheckedSourceFunction (program : Program) (function : Function) where
  preceding : List Function
  following : List Function
  order : program.functions = preceding ++ function :: following
  parameterState : LocalState
  parameterCheck : function.parameters.foldlM (fun (state : LocalState) parameter => do
    require (parameter.storage != Storage.auto) "auto storage is invalid for a parameter"
    require ((lookup state parameter.symbol).isNone && parameter.symbol != function.symbol &&
      !(preceding.any (fun callee => callee.symbol == parameter.symbol)))
      "duplicate or nonlocal parameter symbol"
    return ⟨(parameter.symbol, parameter.type) :: state.bindings,
      parameter.symbol :: state.initialized⟩) ⟨[], []⟩ = .ok parameterState
  body : CheckedStatement preceding function.returnType 0 parameterState function.body
  returns : body.facts.fallsThrough = false

structure CheckedSourceProgram (program : Program) where
  accepted : checkWellFormed program = .ok ()
  functions : ∀ function ∈ program.functions, CheckedSourceFunction program function
  symbols : (program.functions.map Function.symbol).Nodup

structure ValidatorFunctionContext (program : Program) (functionId : Nat)
    (callees : List Function) where
  function : Function
  defined : program.functions.find? (·.symbol == functionId) = some function
  symbol : function.symbol = functionId
  checked : CheckedSourceFunction program function
  preceding : checked.preceding = callees

def checkItems (callees : List Function) (result : QualifiedType) (fuel depth : Nat)
    (input : LocalState) (items : List BlockItem) : Except String FlowFacts :=
  items.foldlM (fun (flow : FlowFacts) item => do
    match item with
    | .declaration object =>
      return {flow with state := ← declare callees fuel flow.state object}
    | .statement child =>
      let next ← checkStmt callees result fuel depth flow.state child
      return {next with fallsThrough := flow.fallsThrough && next.fallsThrough}) ⟨input, true⟩

structure CheckedItems (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (items : List BlockItem) where
  facts : FlowFacts
  accepted : checkItems callees result fuel depth input items = .ok facts

structure CheckedObject (callees : List Function) (fuel : Nat) (input : LocalState)
    (object : Object) where
  state : LocalState
  accepted : declare callees fuel input object = .ok state

structure ValidatorFlowProtocol where
  before : ValidatorEnvironment
  normal : ValidatorEnvironment
  fallsThrough : Bool
  loopDepth : Nat
  resultType : IntType
  restoresLayout : Bool
  allowsAbrupt : Bool

inductive ValidatorProtocol where
  | value (environment : ValidatorEnvironment) (type : IntType)
  | flow (protocol : ValidatorFlowProtocol)
  | terminal

def ValidatorFlowProtocol.Models (protocol : ValidatorFlowProtocol)
    (flow : Flow) (locals : Locals) : Prop :=
  match flow with
  | .normal => protocol.fallsThrough = true ∧ protocol.normal.Models locals
  | .break | .continue => protocol.allowsAbrupt = true ∧ protocol.loopDepth > 0 ∧
      if protocol.restoresLayout then protocol.before.Models locals
      else protocol.before.Grown locals
  | .returned value => protocol.allowsAbrupt = true ∧ value.type = protocol.resultType ∧
      if protocol.restoresLayout then protocol.before.Models locals
      else protocol.before.Grown locals

def ValidatorProtocol.Models (protocol : ValidatorProtocol)
    (control : Control) (locals : Locals) : Prop :=
  match protocol, control with
  | .value environment type, Control.value actual => actual.type = type ∧ environment.Models locals
  | .flow expected, Control.flow actual => expected.Models actual locals
  | _, _ => False

def ValidatorProtocol.Entails (strong weak : ValidatorProtocol) : Prop :=
  ∀ control locals, strong.Models control locals → weak.Models control locals

/-- The certificate is an actual checker equation, with runtime facts allowed
to be stronger than its conservative input. This is necessary for independently
checked binary siblings and call arguments. -/
structure ValidatorExpressionTask (callees : List Function) (expression : Expr) where
  environment : ValidatorEnvironment
  checkedInput : LocalState
  checked : CheckedExpression callees checkedInput expression
  bindings : ∀ id, lookup environment.state id = lookup checkedInput id
  initialized : ∀ id ∈ checkedInput.initialized, id ∈ environment.state.initialized

def ValidatorExpressionTask.output (task : ValidatorExpressionTask callees expression) :
    ValidatorProtocol :=
  .value (task.environment.afterExpression task.checked.facts) expression.type

structure ValidatorStatementTask (callees : List Function) (statement : Stmt) where
  environment : ValidatorEnvironment
  checkedInput : LocalState
  bindings : environment.state.bindings = checkedInput.bindings
  initialized : ∀ id ∈ checkedInput.initialized, id ∈ environment.state.initialized
  result : QualifiedType
  depth : Nat
  checked : CheckedStatement callees result depth checkedInput statement
  outputEnvironment : ValidatorEnvironment
  outputBindings : outputEnvironment.state.bindings = checked.facts.state.bindings
  outputInitialized : outputEnvironment.state.initialized =
    union environment.state.initialized checked.facts.state.initialized
  outputLayout : outputEnvironment.layout = environment.layout
  outputFresh : outputEnvironment.freshConst = environment.freshConst

def statementAllowsAbrupt : Stmt → Bool
  | .null | .expression _ => false
  | _ => true

def ValidatorStatementTask.output (task : ValidatorStatementTask callees statement) :
    ValidatorProtocol :=
  .flow ⟨task.environment, task.outputEnvironment, task.checked.facts.fallsThrough,
    task.depth, task.result.type, true, statementAllowsAbrupt statement⟩

structure ValidatorItemsTask (callees : List Function) (items : List BlockItem) where
  environment : ValidatorEnvironment
  initializedBound : ∀ id ∈ environment.state.initialized,
    (lookup environment.state id).isSome = true
  freshBound : ∀ id ∈ environment.freshConst,
    (lookup environment.state id).isSome = true
  checkedInput : LocalState
  bindings : environment.state.bindings = checkedInput.bindings
  initialized : ∀ id ∈ checkedInput.initialized, id ∈ environment.state.initialized
  result : QualifiedType
  depth : Nat
  fuel : Nat
  checked : CheckedItems callees result fuel depth checkedInput items
  introduced : LocalLayout
  outputEnvironment : ValidatorEnvironment
  outputBindings : outputEnvironment.state.bindings = checked.facts.state.bindings
  bindingsGrow : checked.facts.state.bindings = introduced ++ checkedInput.bindings
  outputInitialized : outputEnvironment.state.initialized =
    union environment.state.initialized checked.facts.state.initialized
  outputLayout : outputEnvironment.layout = introduced ++ environment.layout
  outputFresh : outputEnvironment.freshConst = environment.freshConst

def ValidatorItemsTask.output (task : ValidatorItemsTask callees items) : ValidatorProtocol :=
  .flow ⟨task.environment, task.outputEnvironment, task.checked.facts.fallsThrough,
    task.depth, task.result.type, false, items.any (fun item =>
      match item with | .declaration _ => false | .statement _ => true)⟩

structure ValidatorObjectTask (callees : List Function) (object : Object) where
  environment : ValidatorEnvironment
  checkedInput : LocalState
  bindings : environment.state.bindings = checkedInput.bindings
  initialized : ∀ id ∈ checkedInput.initialized, id ∈ environment.state.initialized
  result : QualifiedType
  depth : Nat
  fuel : Nat
  checked : CheckedObject callees fuel checkedInput object
  outputEnvironment : ValidatorEnvironment
  outputBindings : outputEnvironment.state.bindings = checked.state.bindings
  outputInitialized : outputEnvironment.state.initialized =
    union environment.state.initialized checked.state.initialized
  outputLayout : outputEnvironment.layout = (object.symbol, object.type) :: environment.layout
  outputFresh : outputEnvironment.freshConst = environment.freshConst

def ValidatorObjectTask.output (task : ValidatorObjectTask callees object) : ValidatorProtocol :=
  .flow ⟨task.environment, task.outputEnvironment, true, task.depth, task.result.type, false, false⟩

inductive ValidatorItemTask (callees : List Function) : BlockItem → Type where
  | declaration (task : ValidatorObjectTask callees object) :
      ValidatorItemTask callees (.declaration object)
  | statement (task : ValidatorStatementTask callees statement) :
      ValidatorItemTask callees (.statement statement)

def ValidatorItemTask.environment : ValidatorItemTask callees item → ValidatorEnvironment
  | .declaration task => task.environment
  | .statement task => task.environment

def ValidatorItemTask.checkedInput : ValidatorItemTask callees item → LocalState
  | .declaration task => task.checkedInput
  | .statement task => task.checkedInput

def ValidatorItemTask.result : ValidatorItemTask callees item → QualifiedType
  | .declaration task => task.result
  | .statement task => task.result

def ValidatorItemTask.depth : ValidatorItemTask callees item → Nat
  | .declaration task => task.depth
  | .statement task => task.depth

def ValidatorItemTask.fuel : ValidatorItemTask callees item → Nat
  | .declaration task => task.fuel
  | .statement task => task.checked.fuel

def ValidatorItemTask.output : ValidatorItemTask callees item → ValidatorProtocol
  | .declaration task => task.output
  | .statement task => task.output

structure ValidatorCallTask (program : Program) (callee : Nat) (type : IntType)
    (arguments : List Value) where
  function : Function
  defined : program.functions.find? (·.symbol == callee) = some function
  checked : CheckedSourceFunction program function
  result : function.returnType.type = type
  typedArguments : arguments.map Value.type = function.parameters.map (fun parameter => parameter.type.type)
  environment : ValidatorEnvironment

/-- Definitionally the actual `checkExpr` call-argument fold callback. Each
argument is independently checked against the original conservative input. -/
def validatorArgumentStep (callees : List Function) (fuel : Nat) (input : LocalState)
    (accumulated : ExpressionFacts) (argument : (QualifiedType × Expr) × Parameter) :
    Except String ExpressionFacts := do
  require (argument.1.1 == argument.2.type) "argument conversion differs from the parameter"
  let current ← checkExpr callees fuel input argument.1.2
  require (nonconflicting accumulated current) "unsequenced conflicting arguments"
  return accessUnion accumulated current (union accumulated.initialized current.initialized)

/-- A real argument-evaluation suffix. Previously evaluated values have the
same reverse order as the execution frame; the current runtime environment
may retain extra initialization beyond the original independent checks. -/
structure ValidatorArgumentsTask (program : Program) (callees : List Function)
    (type : IntType) (callee : Nat) (remaining : List (QualifiedType × Expr))
    (reverseValues : List Value) where
  function : Function
  defined : program.functions.find? (·.symbol == callee) = some function
  checked : CheckedSourceFunction program function
  result : function.returnType.type = type
  remainingParameters : List Parameter
  arity : remaining.length = remainingParameters.length
  typedPrefix : reverseValues.reverse.map Value.type ++
    remainingParameters.map (fun parameter => parameter.type.type) =
      function.parameters.map (fun parameter => parameter.type.type)
  checkedInput : LocalState
  fuel : Nat
  accumulated : ExpressionFacts
  facts : ExpressionFacts
  accepted : (remaining.zip remainingParameters).foldlM
    (validatorArgumentStep callees fuel checkedInput) accumulated = .ok facts
  base : ValidatorEnvironment
  environment : ValidatorEnvironment
  preserves : environment.Entails base
  bindings : ∀ id, lookup base.state id = lookup checkedInput id
  initialized : ∀ id ∈ checkedInput.initialized, id ∈ base.state.initialized
  accumulatedInitialized : ∀ id ∈ accumulated.initialized, id ∈ environment.state.initialized

def ValidatorArgumentsTask.output
    (task : ValidatorArgumentsTask program callees type callee remaining reverseValues) :
    ValidatorProtocol :=
  .value (task.base.afterExpression task.facts) type

/-- A validated loop package is reused on its real backedge. The package is
finite syntax evidence; it contains no execution bound or recursive correctness
assumption about another iteration. -/
structure ValidatorLoop (callees : List Function) where
  condition : Expr
  increment : Option Expr
  body : Stmt
  conditionTask : ValidatorExpressionTask callees condition
  bodyTask : ValidatorStatementTask callees body
  incrementTask : ∀ expression, increment = some expression →
    ValidatorExpressionTask callees expression
  before : ValidatorEnvironment
  exit : ValidatorEnvironment
  result : QualifiedType
  outerDepth : Nat
  conditionBefore : before.Entails conditionTask.environment
  testedExit : (conditionTask.environment.afterExpression conditionTask.checked.facts).Entails exit
  testedBody : (conditionTask.environment.afterExpression conditionTask.checked.facts).Entails
    bodyTask.environment
  bodyBefore : bodyTask.environment.Entails before
  bodyExit : bodyTask.environment.Entails exit
  bodyNormalBefore : bodyTask.outputEnvironment.Entails before
  bodyResult : bodyTask.result = result
  bodyDepth : bodyTask.depth = outerDepth + 1
  incrementBefore : ∀ expression (present : increment = some expression),
    bodyTask.environment.Entails (incrementTask expression present).environment
  incrementNormal : ∀ expression (present : increment = some expression),
    bodyTask.outputEnvironment.Entails (incrementTask expression present).environment
  incrementAfter : ∀ expression (present : increment = some expression),
    ((incrementTask expression present).environment.afterExpression
      (incrementTask expression present).checked.facts).Entails before

def ValidatorLoop.output (loop : ValidatorLoop callees) : ValidatorProtocol :=
  .flow ⟨loop.before, loop.exit, true, loop.outerDepth, loop.result.type, true, true⟩

/-- Static scope-retention evidence for both compound and for-init scopes.
No field assumes a retained runtime store or a future execution invariant. -/
structure ValidatorScopeProtocol (input output : ValidatorFlowProtocol) where
  introduced : LocalLayout
  bindings : input.normal.state.bindings = introduced ++ output.before.state.bindings
  layout : input.normal.layout = introduced ++ output.before.layout
  separate : ∀ id, (lookup output.before.state id).isSome = true →
    introduced.find? (fun pair => pair.1 == id) = none
  outputBindings : output.normal.state.bindings = output.before.state.bindings
  outputLayout : output.normal.layout = output.before.layout
  initialized : ∀ id ∈ output.normal.state.initialized,
    id ∈ input.normal.state.initialized ∧ (lookup output.before.state id).isSome = true
  fresh : ∀ id ∈ output.normal.freshConst,
    id ∈ input.normal.freshConst ∧ (lookup output.before.state id).isSome = true
  fallsThrough : input.fallsThrough = true → output.fallsThrough = true
  abruptIntroduced : LocalLayout
  abruptBindings : input.before.state.bindings = abruptIntroduced ++ output.before.state.bindings
  abruptLayout : input.before.layout = abruptIntroduced ++ output.before.layout
  abruptSeparate : ∀ id, (lookup output.before.state id).isSome = true →
    abruptIntroduced.find? (fun pair => pair.1 == id) = none
  abruptInitialized : ∀ id ∈ output.before.state.initialized,
    id ∈ input.before.state.initialized ∧ (lookup output.before.state id).isSome = true
  abruptFresh : ∀ id ∈ output.before.freshConst,
    id ∈ input.before.freshConst ∧ (lookup output.before.state id).isSome = true
  depth : input.loopDepth = output.loopDepth
  result : input.resultType = output.resultType
  allowsAbrupt : input.allowsAbrupt = true → output.allowsAbrupt = true
  restoresLayout : output.restoresLayout = true

/-- Local frame rules name their suspended syntax, stores and permitted flow.
Unlike a future-safety predicate, the rules do not mention `step`, `Steps`,
`Diverges`, or compilation refinement. -/
inductive ValidatorFrame (program : Program) (entryType : IntType) :
    Nat → Nat → Frame → ValidatorProtocol → ValidatorProtocol → Prop where
  | unary (functionId : Nat) (environment : ValidatorEnvironment) (inputType : IntType)
      (op : Unary) :
      ValidatorFrame program entryType functionId functionId (.unary (if op == .not then .int else inputType) op)
        (.value environment inputType) (.value environment (if op == .not then .int else inputType))
  | binaryRight (functionId : Nat) (environment : ValidatorEnvironment) (op : Binary)
      (left : Value) (rightType : IntType)
      (admitted : !(op == .shl && left.type.signed) = true) :
      ValidatorFrame program entryType functionId functionId
        (.binaryRight (binaryType op left.type rightType) op left)
        (.value environment rightType) (.value environment (binaryType op left.type rightType))
  | binaryLeft (functionId : Nat) (callees : List Function) (type : IntType) (op : Binary)
      (left right : Expr) (context : ValidatorFunctionContext program functionId callees)
      (parent : ValidatorExpressionTask callees (.binary type op left right))
      (checkedLeft : CheckedExpression callees parent.checkedInput left)
      (fuel : parent.checked.fuel = checkedLeft.fuel + 1) :
      ValidatorFrame program entryType functionId functionId (.binaryLeft type op right)
        (.value (parent.environment.afterExpression checkedLeft.facts) left.type) parent.output
  | choice (functionId : Nat) (callees : List Function) (type : IntType)
      (condition yes no : Expr) (context : ValidatorFunctionContext program functionId callees)
      (parent : ValidatorExpressionTask callees (.conditional type condition yes no))
      (checkedCondition : CheckedExpression callees parent.checkedInput condition)
      (fuel : parent.checked.fuel = checkedCondition.fuel + 1) :
      ValidatorFrame program entryType functionId functionId (.choice type yes no)
        (.value (parent.environment.afterExpression checkedCondition.facts) condition.type) parent.output
  | commaLeft (functionId : Nat) (callees : List Function) (type : IntType)
      (left right : Expr) (context : ValidatorFunctionContext program functionId callees)
      (parent : ValidatorExpressionTask callees (.comma type left right))
      (checkedLeft : CheckedExpression callees parent.checkedInput left)
      (fuel : parent.checked.fuel = checkedLeft.fuel + 1) :
      ValidatorFrame program entryType functionId functionId (.binaryLeft type .comma right)
        (.value (parent.environment.afterExpression checkedLeft.facts) left.type) parent.output
  | convert (functionId : Nat) (environment : ValidatorEnvironment) (inputType type : IntType) :
      ValidatorFrame program entryType functionId functionId (.convert type)
        (.value environment inputType) (.value environment type)
  | assign (functionId : Nat) (environment : ValidatorEnvironment) (inputType : IntType)
      (op : Assignment) (id : Nat) (destination : QualifiedType)
      (binding : lookup environment.state id = some destination)
      (writable : destination.isConst = false)
      (initialized : op ≠ .set → id ∈ environment.state.initialized)
      (admitted : !(op == .shl && destination.type.signed) = true) :
      ValidatorFrame program entryType functionId functionId (.assign destination.type op id destination)
        (.value environment inputType)
        (.value {environment with state.initialized := id :: environment.state.initialized}
          destination.type)
  | discard (functionId : Nat) (environment : ValidatorEnvironment) (type : IntType)
      (depth : Nat) (result : QualifiedType) :
      ValidatorFrame program entryType functionId functionId .discard (.value environment type)
        (.flow ⟨environment, environment, true, depth, result.type, true, false⟩)
  | returned (functionId : Nat) (environment : ValidatorEnvironment) (type : IntType)
      (depth : Nat) (destination : QualifiedType) :
      ValidatorFrame program entryType functionId functionId (.return destination) (.value environment type)
        (.flow ⟨environment, environment, false, depth, destination.type, true, true⟩)
  | initialize (functionId : Nat) (before : ValidatorEnvironment) (inputType : IntType)
      (id : Nat) (destination : QualifiedType)
      (binding : lookup before.state id = some destination)
      (fresh : destination.isConst = true → id ∈ before.freshConst)
      (after : ValidatorEnvironment)
      (afterBindings : after.state.bindings = before.state.bindings)
      (afterLayout : after.layout = before.layout)
      (afterInitialized : ∀ other ∈ after.state.initialized,
        other = id ∨ other ∈ before.state.initialized)
      (afterFresh : ∀ other ∈ after.freshConst, other ∈ before.freshConst ∧ other ≠ id)
      (depth : Nat) (result : QualifiedType) :
      ValidatorFrame program entryType functionId functionId (.initialize id destination) (.value before inputType)
        (.flow ⟨after, after, true, depth, result.type, true, false⟩)
  | sequence (functionId : Nat) (callees : List Function) (first : BlockItem) (remaining : List BlockItem)
      (context : ValidatorFunctionContext program functionId callees)
      (parent : ValidatorItemsTask callees (first :: remaining))
      (functionResult : parent.result = context.function.returnType)
      (child : ValidatorItemTask callees first)
      (environment : child.environment = parent.environment)
      (input : child.checkedInput = parent.checkedInput)
      (result : child.result = parent.result)
      (depth : child.depth = parent.depth)
      (fuel : child.fuel = parent.fuel) :
      ValidatorFrame program entryType functionId functionId (.sequence remaining)
        child.output parent.output
  | scope (functionId : Nat) (callees : List Function) (items : List BlockItem)
      (context : ValidatorFunctionContext program functionId callees)
      (parent : ValidatorStatementTask callees (.compound items))
      (functionResult : parent.result = context.function.returnType)
      (child : ValidatorItemsTask callees items)
      (environment : child.environment = parent.environment)
      (input : child.checkedInput = parent.checkedInput)
      (result : child.result = parent.result)
      (depth : child.depth = parent.depth)
      (fuel : parent.checked.fuel = child.fuel + 1) :
      ValidatorFrame program entryType functionId functionId (.scope parent.environment.layout.length)
        child.output parent.output
  | scopeProtocol (functionId : Nat) (input output : ValidatorFlowProtocol)
      (certificate : ValidatorScopeProtocol input output) :
      ValidatorFrame program entryType functionId functionId (.scope output.before.layout.length)
        (.flow input) (.flow output)
  | branch (functionId : Nat) (callees : List Function) (condition : Expr) (yes : Stmt) (no : Option Stmt)
      (context : ValidatorFunctionContext program functionId callees)
      (parent : ValidatorStatementTask callees (.ite condition yes no))
      (result : parent.result = context.function.returnType)
      (checkedCondition : CheckedExpression callees parent.checkedInput condition)
      (fuel : parent.checked.fuel = checkedCondition.fuel + 1) :
      ValidatorFrame program entryType functionId functionId (.branch yes no)
        (.value (parent.environment.afterExpression checkedCondition.facts) condition.type) parent.output
  | loopTest (functionId : Nat) (callees : List Function)
      (context : ValidatorFunctionContext program functionId callees) (loop : ValidatorLoop callees)
      (result : loop.result = context.function.returnType) :
      ValidatorFrame program entryType functionId functionId
        (.loopTest loop.condition loop.increment loop.body) loop.conditionTask.output loop.output
  | loopBody (functionId : Nat) (callees : List Function)
      (context : ValidatorFunctionContext program functionId callees) (loop : ValidatorLoop callees)
      (result : loop.result = context.function.returnType) :
      ValidatorFrame program entryType functionId functionId
        (.loopBody loop.condition loop.increment loop.body) loop.bodyTask.output loop.output
  | loopIncrement (functionId : Nat) (callees : List Function)
      (context : ValidatorFunctionContext program functionId callees) (loop : ValidatorLoop callees)
      (result : loop.result = context.function.returnType)
      (expression : Expr) (present : loop.increment = some expression) :
      ValidatorFrame program entryType functionId functionId
        (.loopIncrement loop.condition expression loop.body)
        (loop.incrementTask expression present).output loop.output
  | startLoop (functionId : Nat) (callees : List Function)
      (context : ValidatorFunctionContext program functionId callees) (loop : ValidatorLoop callees)
      (result : loop.result = context.function.returnType)
      (input : ValidatorFlowProtocol) (after : input.normal.Entails loop.before)
      (onlyNormal : input.allowsAbrupt = false) :
      ValidatorFrame program entryType functionId functionId
        (.startLoop loop.condition loop.increment loop.body) (.flow input) loop.output
  | argument (functionId : Nat) (callees : List Function) (type : IntType) (callee : Nat)
      (destination : QualifiedType) (expression : Expr) (remaining : List (QualifiedType × Expr))
      (reverseValues : List Value) (context : ValidatorFunctionContext program functionId callees)
      (parent : ValidatorArgumentsTask program callees type callee
        ((destination, expression) :: remaining) reverseValues)
      (checkedArgument : CheckedExpression callees parent.checkedInput expression)
      (fuel : checkedArgument.fuel = parent.fuel) :
      ValidatorFrame program entryType functionId functionId
        (.callArgument type callee destination remaining reverseValues)
        (.value (parent.environment.afterExpression checkedArgument.facts) expression.type) parent.output
  | entry (functionId : Nat) (flow : ValidatorFlowProtocol)
      (noNormal : flow.fallsThrough = false) (noEscapedLoop : flow.loopDepth = 0)
      (result : flow.resultType = entryType) :
      ValidatorFrame program entryType functionId functionId (.function none entryType) (.flow flow) .terminal
  | caller (calleeId callerId : Nat) (flow : ValidatorFlowProtocol)
      (noNormal : flow.fallsThrough = false) (noEscapedLoop : flow.loopDepth = 0)
      (saved : Locals) (environment : ValidatorEnvironment) (models : environment.Models saved) :
      ValidatorFrame program entryType calleeId callerId (.function (some (callerId, saved)) flow.resultType)
        (.flow flow) (.value environment flow.resultType)

inductive ValidatorContinuation (program : Program) (entryType : IntType) :
    Nat → ValidatorProtocol → List Frame → Prop where
  | terminal (functionId : Nat) : ValidatorContinuation program entryType functionId .terminal []
  | frame {functionId nextFunctionId : Nat} {input output : ValidatorProtocol} {frame : Frame} {rest : List Frame}
      (typed : ValidatorFrame program entryType functionId nextFunctionId frame input output)
      (tail : ValidatorContinuation program entryType nextFunctionId output rest) :
      ValidatorContinuation program entryType functionId input (frame :: rest)
  | weaken {functionId : Nat} {strong weak : ValidatorProtocol} {frames : List Frame}
      (entails : strong.Entails weak)
      (typed : ValidatorContinuation program entryType functionId weak frames) :
      ValidatorContinuation program entryType functionId strong frames

inductive ValidatorControl (program : Program) (entryType : IntType) (functionId : Nat) :
    Control → Locals → ValidatorProtocol → Prop where
  | expression (callees : List Function) (expression : Expr)
      (context : ValidatorFunctionContext program functionId callees)
      (task : ValidatorExpressionTask callees expression)
      (locals : Locals) (models : task.environment.Models locals) :
      ValidatorControl program entryType functionId (.expression expression) locals task.output
  | statement (callees : List Function) (statement : Stmt)
      (context : ValidatorFunctionContext program functionId callees)
      (task : ValidatorStatementTask callees statement)
      (result : task.result = context.function.returnType)
      (locals : Locals) (models : task.environment.Models locals) :
      ValidatorControl program entryType functionId (.statement statement) locals task.output
  | items (callees : List Function) (items : List BlockItem)
      (context : ValidatorFunctionContext program functionId callees)
      (task : ValidatorItemsTask callees items)
      (result : task.result = context.function.returnType)
      (locals : Locals) (models : task.environment.Models locals) :
      ValidatorControl program entryType functionId (.items items) locals task.output
  | object (callees : List Function) (object : Object)
      (context : ValidatorFunctionContext program functionId callees)
      (task : ValidatorObjectTask callees object)
      (result : task.result = context.function.returnType)
      (locals : Locals) (models : task.environment.Models locals) :
      ValidatorControl program entryType functionId (.object object) locals task.output
  | call (callee : Nat) (type : IntType) (arguments : List Value)
      (task : ValidatorCallTask program callee type arguments)
      (locals : Locals) (models : task.environment.Models locals) :
      ValidatorControl program entryType functionId (.call callee type arguments) locals
        (.value task.environment type)
  | delivered (control : Control) (locals : Locals) (protocol : ValidatorProtocol)
      (models : protocol.Models control locals) :
      ValidatorControl program entryType functionId control locals protocol

inductive ValidatorMachineInvariant (program : Program) (entryType : IntType) : State → Prop where
  | running (state : State) (protocol : ValidatorProtocol)
      (checked : CheckedSourceProgram program)
      (control : ValidatorControl program entryType state.functionId state.control state.locals protocol)
      (continuation : ValidatorContinuation program entryType state.functionId protocol state.continuation) :
      ValidatorMachineInvariant program entryType state
  | terminal (state : State) (result : Result) (halted : state.control = .halted result)
      (safe : ResultSafe entryType result) : ValidatorMachineInvariant program entryType state

theorem ValidatorControl.not_halted
    {program : Program} {entryType : IntType} {functionId : Nat}
    {control : Control} {locals : Locals} {protocol : ValidatorProtocol} {result : Result}
    (typed : ValidatorControl program entryType functionId control locals protocol) :
    control ≠ .halted result := by
  intro halted
  cases typed with
  | expression => cases halted
  | statement => cases halted
  | items => cases halted
  | object => cases halted
  | call => cases halted
  | delivered control locals protocol models =>
    rw [halted] at models
    cases protocol <;> simp [ValidatorProtocol.Models] at models

/-- Terminal classification is a consequence of the concrete typing rules,
not the definition of the whole machine invariant. -/
theorem validator_terminal_safe (program : Program) (entryType : IntType) (state : State)
    (invariant : ValidatorMachineInvariant program entryType state)
    (halted : state.control = .halted result) : ResultSafe entryType result := by
  cases invariant with
  | terminal actual terminal safe =>
    have same : actual = result := Control.halted.inj (terminal.symm.trans halted)
    simpa [same] using safe
  | running protocol _ control _ =>
    exact False.elim (control.not_halted halted)

/-- Progress uses the real total transition function. The invariant's separate
terminal lemma supplies the classification at its only `none` case. -/
theorem validator_progress (program : Program) (entryType : IntType) (state : State)
    (invariant : ValidatorMachineInvariant program entryType state) :
    (∃ result, state.control = .halted result ∧ ResultSafe entryType result) ∨
      ∃ next, Step program state next := by
  rcases state with ⟨control, locals, continuation, functionId⟩
  cases control with
  | halted result =>
    left
    exact ⟨result, rfl, validator_terminal_safe program entryType _ invariant rfl⟩
  | items items => right; cases items <;> simp [Step, step]
  | _ => right; simp [Step, step]

end EvoIR.Core.Source
