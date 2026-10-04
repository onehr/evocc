import EvoIR.Core.CallFlow

/-! Executable proof construction for source call closures. The original
source is retained. Finite statements reuse the existing symbolic storage and
flow trees; calls in initializers, branches and expressions execute the actual
source machine. Cyclic call composition remains a separate obligation. -/
namespace EvoIR.Core.Symbolic

def ContextBody {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (body : Source.Stmt) (tree : StateTree Γ) : Prop :=
  ∀ σ fid, Source.ReturnsContext p (locals.denote σ) fid body (tree.denote σ)

def closedBody {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (body : Source.Stmt) : Option {tree // ContextBody p locals body tree} := do
  let flow ← flowDiagram (Source.statementDepth body) locals (.statement body)
  let returned ← flow.val.returns?
  return ⟨returned.val, by
    intro σ fid
    apply Source.ReturnsState.inContext
    intro k
    have path := flow.property σ p fid k
    rw [returned.property.sound σ] at path
    cases outcome : (returned.val.denote σ).2 <;>
      simpa [Source.ReturnsState, outcome, Except.map] using path⟩

theorem body_invocation {Γ : Var → Nat} (p : Source.Program) (f : Source.Function)
    (args : List (CWord Γ)) (locals : LocalStore Γ) (tree : StateTree Γ)
    (found : p.functions.find? (·.symbol == f.symbol) = some f)
    (bound : parameterStore f.parameters args = some locals)
    (body : ContextBody p locals f.body tree) (typed : tree.HasType f.returnType.type) :
    InvocationDiagram p f args tree.erase := by
  intro σ callerLocals caller
  rw [StateTree.erase_sound]
  exact Source.invoke_context_body p f (args.map (CWord.denote σ)) (locals.denote σ)
    callerLocals caller (tree.denote σ) found (parameter_store_sound σ _ _ _ bound)
    (body σ f.symbol) (typed σ)

mutual
  def callExpression {Γ : Var → Nat} (p : Source.Program) : (fuel : Nat) →
      (locals : LocalStore Γ) → (expr : Source.Expr) →
      Option {tree // ContextDiagram p locals expr tree}
    | 0, _, _ => none
    | fuel+1, locals, expr =>
      match stateExpression (Source.expressionDepth expr) locals expr with
      | some simple => some ⟨simple.val, fun σ fid => (simple.property σ p fid).inContext⟩
      | none => match expr with
        | .call type callee arguments => do
          let built ← callArguments p fuel locals type callee arguments []
          return ⟨built.val, fun σ fid => Source.context_call type callee arguments _ (built.property σ fid)⟩
        | .cast type destination operand => do
          if typed : type = destination.type then
            let r ← callExpression p fuel locals operand
            return ⟨r.val.cast destination.type, by
              intro σ fid
              rw [StateTree.cast_sound]
              exact typed ▸ Source.context_cast destination operand _ (r.property σ fid)⟩
          else none
        | .unary type op operand => do
          if noWrap : Source.wrappingConstant (.unary type op operand) = none then
            let a ← callExpression p fuel locals operand
            let b ← a.val.bind? (next := UnaryContinuation type op) (fun next word => do
              if typed : (unary op word).2.type = type then
                return ⟨.linear next (unary op word), typed, rfl⟩
              else none)
            return ⟨b.val, context_unary_diagram p locals type op operand a.val b.val noWrap
              a.property b.property⟩
          else none
        | .binary type .and left right =>
          contextShortExpression p (callExpression p fuel) locals type false left right
        | .binary type .or left right =>
          contextShortExpression p (callExpression p fuel) locals type true left right
        | .binary type op left right => do
          if noWrap : Source.wrappingConstant (.binary type op left right) = none then
            if notAnd : op ≠ .and then
              if notOr : op ≠ .or then
                let a ← callExpression p fuel locals left
                let b ← a.val.bind? (next := fun next word tree => ∃ middle,
                    ContextDiagram p next right middle ∧ TreeBind (BinaryContinuation type op word) middle tree)
                    (fun next word => do
                  let r ← callExpression p fuel next right
                  let applied ← r.val.bind? (next := BinaryContinuation type op word) (fun ls v => do
                    let result ← binaryResult op word v
                    if typed : result.val.2.type = type then
                      return ⟨.linear ls result.val, result.val, result.property, typed, rfl⟩
                    else none)
                  return ⟨applied.val, r.val, r.property, applied.property⟩)
                return ⟨b.val, context_binary_diagram p locals type op left right a.val b.val
                  noWrap notAnd notOr a.property b.property⟩
              else none
            else none
          else none
        | .comma type left right => do
          let a ← callExpression p fuel locals left
          let b ← a.val.bind? (next := fun next _ tree =>
              ContextDiagram p next right tree ∧ tree.HasType type) (fun next _ => do
            let r ← callExpression p fuel next right
            if typed : r.val.Typed type = true then
              return ⟨r.val, r.property, StateTree.typed_sound typed⟩
            else none)
          return ⟨b.val, context_comma_diagram p locals type left right a.val b.val a.property b.property⟩
        | .assign type op id destination (.ident targetType targetId) rhs => do
          if typed : type = destination.type then
            if targetEq : targetType = destination.type ∧ targetId = id then
              if place : locals.writable id destination = true then
                let a ← callExpression p fuel locals rhs
                let b ← a.val.bind? (next := AssignmentContinuation op id destination) (fun next word => do
                  let r ← assignmentResult next op id destination word
                  match written : next.write id destination (r.val.2.cast destination.type) false with
                  | none => none
                  | some updated => return ⟨(StateTree.value updated (r.val.2.cast destination.type)).prefix next r.val.1,
                      r.val, updated, r.property, written, rfl⟩)
                return ⟨b.val, by
                  simpa only [typed, targetEq.1, targetEq.2] using
                    context_assignment_diagram p locals op id destination rhs a.val b.val place a.property b.property⟩
              else none
            else none
          else none
        | .conditional type condition yes no => do
          let a ← callExpression p fuel locals condition
          let b ← a.val.bind? (next := ContextChoice p type yes no) (fun next word => do
            let first ← callExpression p fuel next yes
            let second ← callExpression p fuel next no
            return ⟨.branch word.truthTerm (first.val.cast type) (second.val.cast type),
              first.val, second.val, first.property, second.property, rfl⟩)
          return ⟨b.val, context_choice_diagram p locals type condition yes no a.val b.val a.property b.property⟩
        | _ => none

  def callArguments {Γ : Var → Nat} (p : Source.Program) : (fuel : Nat) →
      (locals : LocalStore Γ) → (type : Source.IntType) → (callee : Nat) →
      (pending : List (Source.QualifiedType × Source.Expr)) → (reverseWords : List (CWord Γ)) →
      Option {tree // ArgumentsDiagram p locals type callee pending reverseWords tree}
    | 0, _, _, _, _, _ => none
    | fuel+1, locals, type, callee, [], reverseWords => do
      let f : {f // p.functions.find? (·.symbol == callee) = some f} ←
        match found : p.functions.find? (·.symbol == callee) with
        | none => none
        | some f => some ⟨f, found⟩
      have id : f.val.symbol = callee := by simpa using List.find?_some f.property
      if typed : type = f.val.returnType.type then
        let body ← callFunction p fuel f.val reverseWords.reverse
        return ⟨body.val.withLocals locals, by
          intro σ fid
          rw [ValueTree.withLocals_sound]
          have invoked := body.property σ (locals.denote σ) fid
          cases result : body.val.denote σ <;>
            simpa only [Source.CallsContext, Source.Invokes, Source.beginArguments,
              List.map_reverse, typed, ← id, result] using invoked⟩
      else none
    | fuel+1, locals, type, callee, (destination, expr) :: pending, reverseWords => do
      let head ← callExpression p fuel locals expr
      let tail ← head.val.bind? (next := fun next word tree => ArgumentsDiagram p next type callee
        pending (word.cast destination.type :: reverseWords) tree)
        (fun next word => callArguments p fuel next type callee pending
          (word.cast destination.type :: reverseWords))
      return ⟨tail.val, arguments_cons_diagram p locals type callee destination expr pending
        reverseWords head.val tail.val head.property tail.property⟩

  def callBody {Γ : Var → Nat} (p : Source.Program) : (fuel : Nat) →
      (locals : LocalStore Γ) → (body : Source.Stmt) →
      Option {tree // ContextBody p locals body tree}
    | 0, _, _ => none
    | fuel+1, locals, body =>
      match closedBody p locals body with
      | some simple => some simple
      | none => do
        let flow ← callFlow p fuel locals (.statement body)
        let returned ← flow.val.returns?
        return ⟨returned.val, by
          intro σ fid k
          have path := flow.property σ fid k
          rw [returned.property.sound σ] at path
          cases result : (returned.val.denote σ).2 <;>
            simpa only [result, Except.map] using path⟩

  def callFlow {Γ : Var → Nat} (p : Source.Program) : (fuel : Nat) →
      (locals : LocalStore Γ) → (control : Source.Control) →
      Option {tree // ContextFlowDiagram p locals control tree}
    | 0, _, _ => none
    | fuel+1, locals, control =>
      match flowDiagram (fuel+1) locals control with
      | some simple => some ⟨simple.val, fun σ fid => (simple.property σ p fid).inContext⟩
      | none => match control with
        | .object object => do
          let fresh : {ls // locals.declare object.symbol object.type = some ls} ←
            match h : locals.declare object.symbol object.type with
            | none => none
            | some ls => some ⟨ls, h⟩
          match initialized : object.initializer with
          | none => none -- The call-free checker already handles this case.
          | some (destination, expr) =>
            if same : destination = object.type then
              let e ← callExpression p fuel fresh.val expr
              let t ← e.val.bindFlow? (next := (ContextFrameDiagram p ·
                  (.initialize object.symbol object.type) · ·)) (fun ls w => do
                match written : ls.write object.symbol object.type (w.cast object.type.type) true with
                | none => none
                | some updated => return ⟨.value updated .normal, fun σ fid =>
                    (initialize_frame ls updated w object.symbol object.type written σ p fid).inContext⟩)
              return ⟨t.val, context_initialized_object p locals fresh.val object expr e.val t.val
                (same ▸ initialized) fresh.property e.property t.property⟩
            else none
        | .items (item :: rest) => do
          let a ← callFlow p fuel locals (itemControl item)
          let b ← a.val.bindNormal? (next := fun ls tree => ContextFlowDiagram p ls (.items rest) tree)
            (fun ls => callFlow p fuel ls (.items rest))
          return ⟨b.val, context_sequence p locals item rest a.val b.val a.property b.property⟩
        | .statement (.expression expr) => do
          let a ← callExpression p fuel locals expr
          let b ← a.val.bindFlow? (next := (ContextFrameDiagram p · .discard · ·))
            (fun ls w => some ⟨.value ls .normal,
              fun σ fid => (discard_frame ls w σ p fid).inContext⟩)
          return ⟨b.val, context_expression_flow p locals locals _ expr .discard a.val b.val
            (by intros; rfl) a.property b.property⟩
        | .statement (.return destination expr) => do
          let a ← callExpression p fuel locals expr
          let b ← a.val.bindFlow? (next := (ContextFrameDiagram p · (.return destination) · ·))
            (fun ls w => some ⟨.value ls (.returned (w.cast destination.type)),
              fun σ fid => (return_frame ls w destination σ p fid).inContext⟩)
          return ⟨b.val, context_expression_flow p locals locals _ expr (.return destination) a.val b.val
            (by intros; rfl) a.property b.property⟩
        | .statement (.ite condition yes no) => do
          let a ← callExpression p fuel locals condition
          let b ← a.val.bindFlow? (next := (ContextFrameDiagram p · (.branch yes no) · ·))
            (fun ls w => do
              let first ← callFlow p fuel ls (.statement yes)
              let second ← callFlow p fuel ls (.statement (no.getD .null))
              return ⟨.branch w.truthTerm first.val second.val,
                context_branch_frame p ls w yes no first.val second.val first.property second.property⟩)
          return ⟨b.val, context_expression_flow p locals locals _ condition (.branch yes no) a.val b.val
            (by intros; rfl) a.property b.property⟩
        | .statement (.compound items) => do
          let a ← callFlow p fuel locals (.items items)
          let b ← a.val.mapLeaves? (scopeExit locals.length)
          return ⟨b.val, context_scope p locals items a.val b.val a.property b.property⟩
        | _ => none

  def callFunction {Γ : Var → Nat} (p : Source.Program) : (fuel : Nat) →
      (f : Source.Function) → (args : List (CWord Γ)) →
      Option {tree // InvocationDiagram p f args tree}
    | 0, _, _ => none
    | fuel+1, f, args => do
      if found : p.functions.find? (·.symbol == f.symbol) = some f then
        let locals : {ls // parameterStore f.parameters args = some ls} ←
          match bound : parameterStore f.parameters args with
          | none => none
          | some ls => some ⟨ls, bound⟩
        let body ← callBody p fuel locals.val f.body
        if typed : body.val.Typed f.returnType.type = true then
          return ⟨body.val.erase, body_invocation p f args locals.val body.val found
            locals.property body.property (StateTree.typed_sound typed)⟩
        else none
      else none
end

/-- Statements containing calls preserve scope exits and non-normal flow;
faults may halt within a callee and never execute the remaining statements. -/
theorem call_flow_sound {Γ : Var → Nat} (p : Source.Program) (fuel : Nat)
    (locals : LocalStore Γ) (control : Source.Control) (tree : FlowTree Γ)
    (accepted : (callFlow p fuel locals control).map Subtype.val = some tree) :
    ContextFlowDiagram p locals control tree := by
  cases built : callFlow p fuel locals control with
  | none => simp [built] at accepted
  | some result =>
    simp only [built, Option.map_some, Option.some.injEq] at accepted
    exact accepted ▸ result.property

/-- A successful construction carries execution evidence for every typed
valuation and every caller continuation, not only the vectors in the tests. -/
theorem call_function_sound {Γ : Var → Nat} (p : Source.Program) (fuel : Nat)
    (f : Source.Function) (args : List (CWord Γ)) (tree : ValueTree Γ)
    (accepted : (callFunction p fuel f args).map Subtype.val = some tree) :
    InvocationDiagram p f args tree := by
  cases built : callFunction p fuel f args with
  | none => simp [built] at accepted
  | some result =>
    simp only [built, Option.map_some, Option.some.injEq] at accepted
    exact accepted ▸ result.property

end EvoIR.Core.Symbolic
