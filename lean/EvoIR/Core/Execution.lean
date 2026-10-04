import EvoIR.Core.Scalar

/-!
Small-step execution of the complete exported core source tree. A transition
performs at most one source operation or one continuation action. Loops and
calls are not recursive Lean evaluation and have no semantic fuel bound.

Malformed states are explicit and distinct from the four admitted diagnostics.
The input checker is not yet proved to exclude them. No source-to-IR simulation
or compilation acceptance follows merely from running this machine.
-/
namespace EvoIR.Core.Source

structure Slot where
  symbol : Nat
  type : QualifiedType
  value : Option Value
  deriving Repr

abbrev Locals := List Slot

def readSlot (locals : Locals) (symbol : Nat) (t : IntType) : Result := do
  let some slot := locals.find? (·.symbol == symbol)
    | throw (.malformed "unbound local")
  if slot.type.type != t then throw (.malformed "local read type mismatch")
  let some value := slot.value | throw (.malformed "uninitialized local read")
  if value.type != t then throw (.malformed "local representation type mismatch")
  return value

/-- Initialization may write a const object exactly once; assignment may not.
Lookup and update concern the same first active binding. -/
def writeSlot : Locals → Nat → QualifiedType → Value → Bool → Except Fault Locals
  | [], _, _, _, _ => .error (.malformed "unbound local write")
  | slot :: rest, symbol, destination, value, initialization => do
    if slot.symbol == symbol then
      if slot.type != destination || value.type != destination.type then
        throw (.malformed "local write type mismatch")
      if initialization then
        if slot.value.isSome then throw (.malformed "duplicate initialization")
      else if slot.type.isConst then throw (.malformed "const assignment")
      return {slot with value := some value} :: rest
    else return slot :: (← writeSlot rest symbol destination value initialization)

inductive Flow where
  | normal | break | continue
  | returned (value : Value)
  deriving Repr

inductive Control where
  | expression (expr : Expr)
  | value (value : Value)
  | statement (stmt : Stmt)
  | items (items : List BlockItem)
  | object (object : Object)
  | flow (flow : Flow)
  | call (callee : Nat) (resultType : IntType) (arguments : List Value)
  | halted (result : Result)
  deriving Repr

inductive Frame where
  | unary (type : IntType) (op : Unary)
  | binaryLeft (type : IntType) (op : Binary) (right : Expr)
  | binaryRight (type : IntType) (op : Binary) (left : Value)
  | choice (type : IntType) (yes no : Expr)
  | convert (type : IntType)
  | assign (type : IntType) (op : Assignment) (symbol : Nat) (destination : QualifiedType)
  | callArgument (type : IntType) (callee : Nat) (destination : QualifiedType)
      (remaining : List (QualifiedType × Expr)) (reverseValues : List Value)
  | discard
  | return (destination : QualifiedType)
  | initialize (symbol : Nat) (destination : QualifiedType)
  | sequence (remaining : List BlockItem)
  | scope (incomingLength : Nat)
  | branch (yes : Stmt) (no : Option Stmt)
  | startLoop (condition : Expr) (increment : Option Expr) (body : Stmt)
  | loopTest (condition : Expr) (increment : Option Expr) (body : Stmt)
  | loopBody (condition : Expr) (increment : Option Expr) (body : Stmt)
  | loopIncrement (condition : Expr) (increment : Expr) (body : Stmt)
  | function (caller : Option (Nat × Locals)) (resultType : IntType)
  deriving Repr

structure State where
  control : Control
  locals : Locals
  continuation : List Frame
  functionId : Nat
  deriving Repr

def fail (s : State) (f : Fault) : State :=
  {s with control := .halted (.error f)}

def valueResult (s : State) (expected : IntType) (r : Result) : State :=
  match r with
  | .error f => fail s f
  | .ok v => if v.type == expected then {s with control := .value v}
      else fail s (.malformed "expression result type mismatch")

def place (s : State) (symbol : Nat) (destination : QualifiedType)
    (target : Expr) : Bool :=
  match target, s.locals.find? (·.symbol == symbol) with
  | .ident type id, some slot =>
    id == symbol && type == destination.type && slot.type == destination && !slot.type.isConst
  | _, _ => false

def beginArguments (s : State) (type : IntType) (callee : Nat)
    (remaining : List (QualifiedType × Expr)) (reverseValues : List Value) : State :=
  match remaining with
  | [] => {s with control := .call callee type reverseValues.reverse}
  | (destination, expr) :: rest =>
    {s with
      control := .expression expr
      continuation := .callArgument type callee destination rest reverseValues :: s.continuation}

def enterExpression (s : State) (e : Expr) : State :=
  if let some v := wrappingConstant e then valueResult s e.type (.ok v) else
  match e with
  | .literal t n =>
    if n < 2^t.width then valueResult s t (.ok (.ofNat t n))
    else fail s (.malformed "noncanonical source literal")
  | .ident t id => valueResult s t (readSlot s.locals id t)
  | .unary t op a =>
    {s with control := .expression a, continuation := .unary t op :: s.continuation}
  | .binary t op a b =>
    {s with control := .expression a, continuation := .binaryLeft t op b :: s.continuation}
  | .conditional t c a b =>
    {s with control := .expression c, continuation := .choice t a b :: s.continuation}
  | .cast t destination a =>
    if t != destination.type then fail s (.malformed "cast result mismatch") else
      {s with control := .expression a, continuation := .convert t :: s.continuation}
  | .comma t a b =>
    {s with control := .expression a, continuation := .binaryLeft t .comma b :: s.continuation}
  | .assign t op id destination target rhs =>
    if !place s id destination target then fail s (.malformed "invalid assignment place") else
      {s with
        control := .expression rhs
        continuation := .assign t op id destination :: s.continuation}
  | .update t op id destination target =>
    if !place s id destination target then fail s (.malformed "invalid update place") else
      let result : Except Fault (Value × Locals) := do
        let before ← readSlot s.locals id destination.type
        let binary := if op == .preInc || op == .postInc then Binary.add else .sub
        let after := (← binaryValue binary before (.ofNat .int 1)).cast destination.type
        let locals ← writeSlot s.locals id destination after false
        return (if op == .postInc || op == .postDec then before else after, locals)
      match result with
      | .error f => fail s f
      | .ok (v, locals) => valueResult {s with locals} t (.ok v)
  | .call t callee arguments => beginArguments s t callee arguments []

def enterStatement (s : State) (stmt : Stmt) : State :=
  match stmt with
  | .null => {s with control := .flow .normal}
  | .break => {s with control := .flow .break}
  | .continue => {s with control := .flow .continue}
  | .expression e =>
    {s with control := .expression e, continuation := .discard :: s.continuation}
  | .return destination e =>
    {s with control := .expression e, continuation := .return destination :: s.continuation}
  | .compound items =>
    {s with control := .items items, continuation := .scope s.locals.length :: s.continuation}
  | .ite condition yes no =>
    {s with control := .expression condition, continuation := .branch yes no :: s.continuation}
  | .while condition body =>
    {s with
      control := .expression condition
      continuation := .loopTest condition none body :: s.continuation}
  | .doWhile body condition =>
    {s with
      control := .statement body
      continuation := .loopBody condition none body :: s.continuation}
  | .for init condition increment body =>
    let first := match init with
      | .empty => Control.flow .normal
      | .expression e => .statement (.expression e)
      | .declarations objects => .items (objects.map BlockItem.declaration)
    {s with control := first, continuation :=
      .startLoop (condition.getD (.literal .int 1)) increment body ::
      .scope s.locals.length :: s.continuation}

def enterObject (s : State) (o : Object) : State :=
  if s.locals.any (·.symbol == o.symbol) then fail s (.malformed "duplicate active local") else
  let fresh := {s with locals := ⟨o.symbol, o.type, none⟩ :: s.locals}
  match o.initializer with
  | none => {fresh with control := .flow .normal}
  | some (destination, expr) =>
    if destination != o.type then fail s (.malformed "initializer destination mismatch") else
    {fresh with
      control := .expression expr
      continuation := .initialize o.symbol destination :: fresh.continuation}

def resumeValue (s : State) (frame : Frame) (v : Value) : State :=
  match frame with
  | .unary t op => valueResult s t (unaryValue op v)
  | .binaryLeft t op right =>
    if op == .and && !v.truth then valueResult s t (.ok (.boolean false))
    else if op == .or && v.truth then valueResult s t (.ok (.boolean true))
    else {s with control := .expression right, continuation := .binaryRight t op v :: s.continuation}
  | .binaryRight t op left => valueResult s t (binaryValue op left v)
  | .choice t yes no =>
    {s with
      control := .expression (if v.truth then yes else no)
      continuation := .convert t :: s.continuation}
  | .convert t => {s with control := .value (v.cast t)}
  | .assign t op id destination =>
    let result : Except Fault (Value × Locals) := do
      let value ← match op.binary with
        | none => pure v
        | some binary => do binaryValue binary (← readSlot s.locals id destination.type) v
      let converted := value.cast destination.type
      let locals ← writeSlot s.locals id destination converted false
      return (converted, locals)
    match result with
    | .error f => fail s f
    | .ok (value, locals) => valueResult {s with locals} t (.ok value)
  | .callArgument t callee destination remaining values =>
    beginArguments s t callee remaining (v.cast destination.type :: values)
  | .discard => {s with control := .flow .normal}
  | .return destination => {s with control := .flow (.returned (v.cast destination.type))}
  | .initialize id destination =>
    match writeSlot s.locals id destination (v.cast destination.type) true with
    | .error f => fail s f
    | .ok locals => {s with locals, control := .flow .normal}
  | .branch yes no =>
    {s with control := if v.truth then .statement yes else .statement (no.getD .null)}
  | .loopTest condition increment body =>
    if v.truth then {s with
      control := .statement body
      continuation := .loopBody condition increment body :: s.continuation}
    else {s with control := .flow .normal}
  | .loopIncrement condition increment body =>
    {s with
      control := .expression condition
      continuation := .loopTest condition (some increment) body :: s.continuation}
  | _ => fail s (.malformed "value at statement continuation")

def resumeFlow (s : State) (frame : Frame) (flow : Flow) : State :=
  match frame with
  | .scope n =>
    if s.locals.length < n then fail s (.malformed "invalid scope stack") else
    {s with locals := s.locals.drop (s.locals.length - n), control := .flow flow}
  | .sequence remaining =>
    {s with control := match flow with | .normal => .items remaining | _ => .flow flow}
  | .startLoop condition increment body =>
    match flow with
    | .normal => {s with
        control := .expression condition
        continuation := .loopTest condition increment body :: s.continuation}
    | _ => {s with control := .flow flow}
  | .loopBody condition increment body =>
    match flow with
    | .break => {s with control := .flow .normal}
    | .returned _ => {s with control := .flow flow}
    | .normal | .continue =>
      match increment with
      | none => {s with
          control := .expression condition
          continuation := .loopTest condition none body :: s.continuation}
      | some e => {s with
          control := .expression e
          continuation := .loopIncrement condition e body :: s.continuation}
  | .function caller expected =>
    match flow with
    | .returned value =>
      if value.type != expected then fail s (.malformed "function return type mismatch") else
      match caller with
      | none => {s with control := .halted (.ok value), locals := []}
      | some (functionId, locals) => {s with control := .value value, locals, functionId}
    | _ => fail s (.malformed "function fell through or escaped loop control")
  | _ => fail s (.malformed "statement at expression continuation")

def parameterLocals : List Parameter → List Value → Except Fault Locals
  | [], [] => .ok []
  | p :: ps, v :: vs => do
    if p.type.type != v.type then throw (.malformed "call argument type mismatch")
    let rest ← parameterLocals ps vs
    if rest.any (·.symbol == p.symbol) then throw (.malformed "duplicate parameter")
    return ⟨p.symbol, p.type, some v⟩ :: rest
  | _, _ => .error (.malformed "call argument count mismatch")

def enterFunction (p : Program) (s : State) (callee : Nat) (resultType : IntType)
    (arguments : List Value) (caller : Option (Nat × Locals)) : State :=
  match p.functions.find? (·.symbol == callee) with
  | none => fail s (.malformed "missing function")
  | some f =>
    if f.returnType.type != resultType then fail s (.malformed "call result type mismatch") else
    match parameterLocals f.parameters arguments with
    | .error error => fail s error
    | .ok locals => {
        control := .statement f.body
        locals
        continuation := .function caller resultType :: s.continuation
        functionId := callee}

/-- A total single-step function, with `none` only at a terminal state. -/
def step (p : Program) (s : State) : Option State :=
  match s.control with
  | .halted _ => none
  | .expression e => some (enterExpression s e)
  | .statement stmt => some (enterStatement s stmt)
  | .object o => some (enterObject s o)
  | .call callee t args => some (enterFunction p s callee t args (some (s.functionId, s.locals)))
  | .items [] => some {s with control := .flow .normal}
  | .items (first :: rest) => some {s with
      control := match first with | .declaration o => .object o | .statement stmt => .statement stmt,
      continuation := .sequence rest :: s.continuation}
  | .value v => some <| match s.continuation with
      | [] => fail s (.malformed "missing value continuation")
      | frame :: rest => resumeValue {s with continuation := rest} frame v
  | .flow flow => some <| match s.continuation with
      | [] => fail s (.malformed "missing statement continuation")
      | frame :: rest => resumeFlow {s with continuation := rest} frame flow

def initial (p : Program) (entry : Nat) (args : List Value) : Except Fault State := do
  if !p.entries.contains entry then throw (.malformed "entry not selected")
  let some f := p.functions.find? (·.symbol == entry) | throw (.malformed "missing entry")
  let locals ← parameterLocals f.parameters args
  return ⟨.statement f.body, locals, [.function none f.returnType.type], entry⟩

def Step (p : Program) (s t : State) : Prop := step p s = some t

inductive Steps (p : Program) : State → State → Prop where
  | refl (s) : Steps p s s
  | next {s t u} : Step p s t → Steps p t u → Steps p s u

/-- An infinite execution is an infinite sequence of actual transitions, not
a finite run that used up evaluator fuel. -/
def Diverges (p : Program) (s : State) : Prop :=
  ∃ trace : Nat → State, trace 0 = s ∧ ∀ n, Step p (trace n) (trace (n + 1))

theorem step_deterministic (p : Program) (s t u : State)
    (ht : Step p s t) (hu : Step p s u) : t = u := by
  exact Option.some.inj (ht.symm.trans hu)

/-- Semantic diagnostics and malformed states are terminal; neither executes
a later return, effect or diagnostic. This does not prove malformed states
unreachable from inputs accepted by `checkWellFormed`. -/
theorem halt_has_no_step (p : Program) (r : Result) (locals : Locals) (k : List Frame) (functionId : Nat) :
    step p ⟨.halted r, locals, k, functionId⟩ = none := rfl

theorem halt_steps_identity (p : Program) (r : Result) (locals : Locals)
    (k : List Frame) (functionId : Nat) (t : State) (h : Steps p ⟨.halted r, locals, k, functionId⟩ t) :
    t = ⟨.halted r, locals, k, functionId⟩ := by
  cases h with
  | refl => rfl
  | next h _ => simp [Step, step] at h

/-- A bounded observer for tests, not a replacement for the transition
semantics. Returning a nonterminal state means only that the bound was reached. -/
def runSteps (p : Program) : Nat → State → State
  | 0, s => s
  | n + 1, s => match step p s with
    | none => s
    | some t => runSteps p n t

theorem runSteps_prefix (p : Program) (fuel : Nat) (s : State) :
    Steps p s (runSteps p fuel s) := by
  induction fuel generalizing s with
  | zero => exact .refl s
  | succ n ih =>
    simp only [runSteps]
    split
    · exact .refl s
    · rename_i t h
      exact .next h (ih t)

theorem diagnostic_cannot_be_overwritten (p : Program) (code : Nat)
    (locals : Locals) (k : List Frame) (functionId fuel : Nat) :
    runSteps p fuel ⟨.halted (.error (.diagnostic code)), locals, k, functionId⟩ =
      ⟨.halted (.error (.diagnostic code)), locals, k, functionId⟩ := by
  exact halt_steps_identity p _ locals k functionId _ (runSteps_prefix p fuel _)

end EvoIR.Core.Source
