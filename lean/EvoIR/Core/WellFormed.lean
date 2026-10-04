import EvoIR.Core.Source

/-!
Independent validation of the complete source input. This does not consult the
producer's inventory, evaluate the program, or accept a compilation certificate.
It checks bindings, C integer ranks/conversions, definite initialization, scopes,
closed acyclic calls, and a conservative unsequenced-access restriction.

The recursion bound limits input syntax only. Loop bodies are checked once under
conservative incoming initialization; no execution fuel or bounded-loop argument
is used. Soundness with respect to the small-step semantics in Execution.lean remains
a separate V01 obligation. In particular this file makes no progress theorem.
-/
namespace EvoIR.Core.Source

def IntType.unsigned : IntType → IntType
  | .int | .uint => .uint
  | .long | .ulong => .ulong
  | .longLong | .ulongLong => .ulongLong

/-- Usual arithmetic conversions for the six already-promoted LP64 types. -/
def usualType (a b : IntType) : IntType :=
  if a.signed == b.signed then if a.rank ≥ b.rank then a else b
  else
    let s := if a.signed then a else b
    let u := if a.signed then b else a
    if u.rank ≥ s.rank then u
    else if s.width > u.width then s else s.unsigned

def binaryType (op : Binary) (a b : IntType) : IntType :=
  match op with
  | .lt | .le | .gt | .ge | .eq | .ne | .and | .or => .int
  | .shl | .shr => a
  | .comma => b
  | _ => usualType a b

private def require (condition : Bool) (message : String) : Except String Unit :=
  if condition then .ok () else .error message

private def union (a b : List Nat) : List Nat :=
  a ++ b.filter (fun n => !a.contains n)

private def intersection (a b : List Nat) : List Nat := a.filter b.contains
private def disjoint (a b : List Nat) : Bool := a.all (fun n => !b.contains n)

structure LocalState where
  bindings : List (Nat × QualifiedType)
  initialized : List Nat
  deriving Repr

private def lookup (s : LocalState) (id : Nat) : Option QualifiedType :=
  (s.bindings.find? (fun p => p.1 == id)).map Prod.snd

structure ExpressionFacts where
  initialized : List Nat
  reads : List Nat := []
  writes : List Nat := []
  deriving Repr

private def accessUnion (a b : ExpressionFacts) (initialized : List Nat) : ExpressionFacts :=
  ⟨initialized, union a.reads b.reads, union a.writes b.writes⟩

/-- Non-sequenced operands may not modify an object accessed by the other. -/
private def nonconflicting (a b : ExpressionFacts) : Bool :=
  disjoint a.writes (union b.reads b.writes) && disjoint b.writes a.reads

private def readLocal (s : LocalState) (id : Nat) (t : IntType) : Except String Unit := do
  let some binding := lookup s id | throw "unbound local identifier"
  require (binding.type == t) "identifier type differs from its binding"
  require (s.initialized.contains id) "read of a possibly uninitialized local"

private def checkPlace (s : LocalState) (id : Nat) (dest : QualifiedType)
    (target : Expr) (read : Bool) : Except String Unit := do
  let some binding := lookup s id | throw "unbound assignment target"
  require (binding == dest && !binding.isConst) "assignment changes its binding or writes const"
  match target with
  | .ident t symbol => require (id == symbol && t == dest.type) "assignment place mismatch"
  | _ => throw "nonlocal assignment target"
  if read then readLocal s id dest.type

/-- Neither the exported result type nor a producer initialization claim is
trusted. All operand trees are checked, including non-executed alternatives. -/
def checkExpr (callees : List Function) : Nat → LocalState → Expr → Except String ExpressionFacts
  | 0, _, _ => .error "source expression nesting limit"
  | fuel + 1, s, e => do
    match e with
    | .literal t bits =>
      require (bits < 2^t.width) "literal exceeds its source format"
      return ⟨s.initialized, [], []⟩
    | .ident t id =>
      readLocal s id t
      return ⟨s.initialized, [id], []⟩
    | .unary t op a =>
      let f ← checkExpr callees fuel s a
      require (t == if op == .not then .int else a.type) "unary result type mismatch"
      return f
    | .binary t op a b =>
      require (t == binaryType op a.type b.type) "binary result type mismatch"
      require (!(op == .shl && a.type.signed)) "signed left shift is outside core-v1"
      let l ← checkExpr callees fuel s a
      if op == .comma || op == .and || op == .or then
        let r ← checkExpr callees fuel {s with initialized := l.initialized} b
        return accessUnion l r (if op == .comma then r.initialized else l.initialized)
      else
        -- Do not use a sibling's side effect as an initialization premise.
        let r ← checkExpr callees fuel s b
        require (nonconflicting l r) "unsequenced conflicting operand accesses"
        return accessUnion l r (union l.initialized r.initialized)
    | .conditional t c a b =>
      require (t == usualType a.type b.type) "conditional result type mismatch"
      let condition ← checkExpr callees fuel s c
      let branchState := {s with initialized := condition.initialized}
      let yes ← checkExpr callees fuel branchState a
      let no ← checkExpr callees fuel branchState b
      let arms := accessUnion yes no (intersection yes.initialized no.initialized)
      return accessUnion condition arms arms.initialized
    | .cast t destination a =>
      require (t == destination.type) "cast destination mismatch"
      checkExpr callees fuel s a
    | .comma t a b =>
      require (t == b.type) "comma result type mismatch"
      let l ← checkExpr callees fuel s a
      let r ← checkExpr callees fuel {s with initialized := l.initialized} b
      return accessUnion l r r.initialized
    | .assign t op id destination target value =>
      require (t == destination.type) "assignment result type mismatch"
      require (!(op == .shl && t.signed)) "signed left assignment shift is outside core-v1"
      checkPlace s id destination target (op != .set)
      let rhs ← checkExpr callees fuel s value
      -- Conservative: even internally sequenced nested writes to the assigned
      -- object are refused; this is not a claim to admit all defined C.
      require (!rhs.writes.contains id) "nested modification of assignment target"
      return ⟨union rhs.initialized [id],
        if op == .set then rhs.reads else union rhs.reads [id], union rhs.writes [id]⟩
    | .update t _ id destination target =>
      require (t == destination.type) "update result type mismatch"
      checkPlace s id destination target true
      return ⟨s.initialized, [id], [id]⟩
    | .call t id arguments =>
      let some f := callees.find? (fun f => f.symbol == id)
        | throw "call is not in the preceding acyclic definition closure"
      require ((lookup s id).isNone) "call symbol is shadowed by a local binding"
      require (t == f.returnType.type && arguments.length == f.parameters.length)
        "call result type or arity mismatch"
      (arguments.zip f.parameters).foldlM (fun accumulated (arg, parameter) => do
        require (arg.1 == parameter.type) "argument conversion differs from the parameter"
        let current ← checkExpr callees fuel s arg.2
        require (nonconflicting accumulated current) "unsequenced conflicting arguments"
        return accessUnion accumulated current (union accumulated.initialized current.initialized))
        ⟨s.initialized, [], []⟩

private def expressionState (callees : List Function) (fuel : Nat)
    (s : LocalState) (e : Expr) : Except String LocalState := do
  let facts ← checkExpr callees fuel s e
  return {s with initialized := facts.initialized}

private def declare (callees : List Function) (fuel : Nat)
    (s : LocalState) (o : Object) : Except String LocalState := do
  require ((lookup s o.symbol).isNone) "duplicate active object symbol"
  require (!(callees.any (fun f => f.symbol == o.symbol))) "object reuses a function symbol"
  let fresh : LocalState := ⟨(o.symbol, o.type) :: s.bindings, s.initialized⟩
  match o.initializer with
  | none => return fresh
  | some (destination, value) =>
    require (destination == o.type) "initializer conversion differs from object type"
    let out ← expressionState callees fuel fresh value
    return {out with initialized := union out.initialized [o.symbol]}

structure FlowFacts where
  state : LocalState
  fallsThrough : Bool := true
  deriving Repr

private def restoreScope (incoming out : LocalState) : LocalState :=
  ⟨incoming.bindings, out.initialized.filter (fun id => (lookup incoming id).isSome)⟩

def checkStmt (callees : List Function) (returnType : QualifiedType) :
    Nat → Nat → LocalState → Stmt → Except String FlowFacts
  | 0, _, _, _ => .error "source statement nesting limit"
  | fuel + 1, loopDepth, s, stmt => do
    match stmt with
    | .null => return ⟨s, true⟩
    | .break | .continue =>
      require (loopDepth > 0) "break or continue outside a loop"
      return ⟨s, false⟩
    | .expression e => return ⟨← expressionState callees fuel s e, true⟩
    | .return destination e =>
      require (destination == returnType) "return conversion differs from function result"
      return ⟨← expressionState callees fuel s e, false⟩
    | .compound items =>
      let out ← items.foldlM (fun (flow : FlowFacts) item => do
        match item with
        | .declaration object =>
          return {flow with state := ← declare callees fuel flow.state object}
        | .statement child =>
          let next ← checkStmt callees returnType fuel loopDepth flow.state child
          return {next with fallsThrough := flow.fallsThrough && next.fallsThrough}) ⟨s, true⟩
      return {out with state := restoreScope s out.state}
    | .ite condition yes no =>
      let branchState ← expressionState callees fuel s condition
      let a ← checkStmt callees returnType fuel loopDepth branchState yes
      let b ← match no with
        | none => pure (FlowFacts.mk branchState true)
        | some child => checkStmt callees returnType fuel loopDepth branchState child
      let initialized := if a.fallsThrough then
          if b.fallsThrough then intersection a.state.initialized b.state.initialized
          else a.state.initialized
        else b.state.initialized
      return ⟨{branchState with initialized}, a.fallsThrough || b.fallsThrough⟩
    | .while condition body =>
      let incoming ← expressionState callees fuel s condition
      let _ ← checkStmt callees returnType fuel (loopDepth + 1) incoming body
      return ⟨incoming, true⟩
    | .doWhile body condition =>
      let _ ← checkStmt callees returnType fuel (loopDepth + 1) s body
      -- A continue/break can bypass an initializing statement in the body.
      let _ ← expressionState callees fuel s condition
      return ⟨s, true⟩
    | .for init condition increment body =>
      let start ← match init with
        | .empty => pure s
        | .expression e => expressionState callees fuel s e
        | .declarations objects => objects.foldlM (declare callees fuel) s
      let incoming ← match condition with
        | none => pure start
        | some e => expressionState callees fuel start e
      let _ ← checkStmt callees returnType fuel (loopDepth + 1) incoming body
      if let some e := increment then
        let _ ← expressionState callees fuel incoming e
      return ⟨restoreScope s incoming, true⟩

private def checkFunction (callees : List Function) (f : Function) : Except String Unit := do
  let initial ← f.parameters.foldlM (fun (s : LocalState) p => do
    require (p.storage != .auto) "auto storage is invalid for a parameter"
    require ((lookup s p.symbol).isNone && p.symbol != f.symbol &&
      !(callees.any (fun g => g.symbol == p.symbol))) "duplicate or nonlocal parameter symbol"
    return ⟨(p.symbol, p.type) :: s.bindings, p.symbol :: s.initialized⟩) ⟨[], []⟩
  let out ← checkStmt callees f.returnType 1024 0 initial f.body
  require (!out.fallsThrough) "function may fall off its value-returning body"

/-- Independent input validation only. Every supplied function is checked;
the ordered closure makes recursive, forward-only and external calls refuse. -/
def checkWellFormed (p : Program) : Except String Unit := do
  require (!p.entries.isEmpty && !p.functions.isEmpty) "empty selected program"
  require (p.entries.eraseDups.length == p.entries.length) "duplicate selected entry"
  let _ ← p.functions.foldlM (fun (prior : List Function) f => do
    require (!(prior.any (fun g => g.symbol == f.symbol || g.name == f.name)))
      "duplicate function symbol or name"
    checkFunction prior f
    return prior ++ [f]) []
  require (p.entries.all (fun id => p.functions.any (fun f => f.symbol == id)))
    "selected entry has no definition"

end EvoIR.Core.Source
