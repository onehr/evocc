import EvoIR.Core.CallExpression

/-! Executable proof construction for source call closures. The original
source is retained. Finite no-call bodies reuse the existing statement checker;
program-relative return expressions can contain nested calls and conversions.
Other calling statement forms and cyclic call composition remain obligations. -/
namespace EvoIR.Core.Symbolic

def ContextBody {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (body : Source.Stmt) (tree : StateTree Γ) : Prop :=
  ∀ σ fid, Source.ReturnsContext p (locals.denote σ) fid body (tree.denote σ)

def ReturnScope {Γ : Var → Nat} (depth : Nat) (locals : LocalStore Γ)
    (word : CWord Γ) (tree : StateTree Γ) : Prop :=
  ∃ updated, locals.exitScope depth = some updated ∧ tree = .value updated word

theorem context_compound {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (body : Source.Stmt) (before after : StateTree Γ)
    (path : ContextBody p locals body before)
    (scope : TreeBind (ReturnScope locals.length) before after) :
    ContextBody p locals (.compound [.statement body]) after := by
  intro σ fid k
  have length : (locals.denote σ).length = locals.length := by simp [LocalStore.denote]
  have start : Source.Step p
      ⟨.statement (.compound [.statement body]), locals.denote σ, k, fid⟩
      ⟨.items [.statement body], locals.denote σ, .scope locals.length :: k, fid⟩ := by
    simp [Source.Step, Source.step, Source.enterStatement, length]
  have enter : Source.Step p
      ⟨.items [.statement body], locals.denote σ, .scope locals.length :: k, fid⟩
      ⟨.statement body, locals.denote σ, .sequence [] :: .scope locals.length :: k, fid⟩ := rfl
  have inner := path σ fid (.sequence [] :: .scope locals.length :: k)
  rcases scope.outcome σ with ⟨ls, fault, failed, output⟩ |
    ⟨ls, word, tree, ⟨updated, exit, shape⟩, success, output⟩
  · rw [output]
    simp only [failed] at inner
    obtain ⟨failedLocals, failedK, failedFid, steps⟩ := inner
    exact ⟨failedLocals, failedK, failedFid, .next start (.next enter steps)⟩
  · rw [output, shape]
    simp only [success] at inner
    have leave := local_scope_exit_sound σ ls updated locals.length exit
      ⟨.flow (.returned (word.denote σ)), ls.denote σ, k, fid⟩ rfl
      (.returned (word.denote σ))
    exact .next start (.next enter (inner.trans (.next rfl
      (.next (congrArg some leave) (.refl _)))))

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
      | none => match body with
        | .return destination expr => do
          let r ← callExpression p fuel locals expr
          return ⟨r.val.cast destination.type, by
            intro σ fid
            rw [StateTree.cast_sound]
            exact Source.context_return destination expr _ (r.property σ fid)⟩
        | .compound [.statement inner] => do
          let r ← callBody p fuel locals inner
          let exit ← r.val.bind? (next := ReturnScope locals.length) (fun ls word => do
            match h : ls.exitScope locals.length with
            | none => none
            | some updated => return ⟨.value updated word, updated, h, rfl⟩)
          return ⟨exit.val, context_compound p locals inner r.val exit.val r.property exit.property⟩
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
