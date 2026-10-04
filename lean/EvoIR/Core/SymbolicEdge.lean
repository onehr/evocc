import EvoIR.Core.SymbolicBindings

/-! Exact, simultaneous block-argument transport. The successor and its
parameter types come from the actual target CFG. Integer terms and opaque
state tokens are both preserved; transport grants no new integer facts. -/
namespace EvoIR.Core.Symbolic

deriving instance DecidableEq for Slot

def Slot.type {Γ : Var → Nat} (types : Nat → IR.Ty) : Slot Γ → IR.Ty
  | .word w => .bits w.1
  | .opaque id => types id

theorem slot_type_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (types : Nat → IR.Ty) (typed : ∀ id, (ρ id).type = types id) (slot : Slot Γ) :
    (slot.denote σ ρ).type = slot.type types := by
  cases slot with
  | word w => rfl
  | «opaque» id => exact typed id

def bindSlotsLoop {Γ : Var → Nat} (types : Nat → IR.Ty) :
    List IR.Binding → List (Slot Γ) → Scope Γ → Option (Scope Γ)
  | [], [], values => some values
  | p :: ps, v :: vs, values =>
    if p.type != v.type types || values.any (fun entry => entry.1 == p.id) then none
    else bindSlotsLoop types ps vs ((p.id, v) :: values)
  | _, _, _ => none

def bindSlots {Γ : Var → Nat} (types : Nat → IR.Ty)
    (parameters : List IR.Binding) (args : List (Slot Γ)) : Option (Scope Γ) :=
  if parameters.length != args.length then none else bindSlotsLoop types parameters args []

/-- This is definitionally the body of the actual machine's `IR.bind` loop.
The proof below connects to that function, rather than trusting a parallel
binding interpreter. -/
private def bindingStep (pair : IR.Binding × IR.Value) (values : IR.Bindings) :
    Except Source.Fault (ForInStep IR.Bindings) := do
  let (p, v) := pair
  if p.type != v.type then throw (.malformed "IR binding type")
  if values.any (fun entry => entry.1 == p.id) then throw (.malformed "IR duplicate binding")
  return .yield ((p.id, v) :: values)

private theorem binding_loop_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (types : Nat → IR.Ty) (typed : ∀ id, (ρ id).type = types id)
    (parameters : List IR.Binding) (args : List (Slot Γ)) (scope next : Scope Γ)
    (accepted : bindSlotsLoop types parameters args scope = some next) :
    forIn (parameters.zip (args.map (Slot.denote σ ρ))) (scope.denote σ ρ) bindingStep =
      .ok (next.denote σ ρ) := by
  induction parameters generalizing args scope next with
  | nil =>
    cases args with
    | nil => cases Option.some.inj accepted; rfl
    | cons _ _ => simp [bindSlotsLoop] at accepted
  | cons p ps ih =>
    cases args with
    | nil => simp [bindSlotsLoop] at accepted
    | cons v vs =>
      simp only [bindSlotsLoop] at accepted
      split at accepted
      · contradiction
      · rename_i shape
        have clean : (p.type != v.type types || scope.any (fun entry => entry.1 == p.id)) = false :=
          Bool.eq_false_iff.mpr shape
        have type : p.type = v.type types := by
          have := (Bool.or_eq_false_iff.mp clean).1
          simpa using this
        have fresh := (Bool.or_eq_false_iff.mp clean).2
        have concreteFresh : (scope.denote σ ρ).any (fun entry => entry.1 == p.id) = false := by
          simpa only [Scope.denote, List.any_map, Function.comp_def] using fresh
        simp only [List.map_cons, List.zip_cons_cons, List.forIn_cons, bindingStep,
          slot_type_sound σ ρ types typed v, type, concreteFresh,
          bne_self_eq_false, Bool.false_eq_true, ↓reduceIte,
          bind, Except.bind, pure, Except.pure]
        exact ih vs ((p.id, v) :: scope) next accepted

theorem bind_slots_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (types : Nat → IR.Ty) (typed : ∀ id, (ρ id).type = types id)
    (parameters : List IR.Binding) (args : List (Slot Γ)) (next : Scope Γ)
    (accepted : bindSlots types parameters args = some next) :
    IR.bind parameters (args.map (Slot.denote σ ρ)) = .ok (next.denote σ ρ) := by
  unfold bindSlots at accepted
  split at accepted
  · contradiction
  · rename_i arity
    have length : parameters.length = args.length := by simpa using arity
    simp only [IR.bind, List.length_map, length, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
    have loop := binding_loop_sound σ ρ types typed parameters args [] next accepted
    change forIn (parameters.zip (args.map (Slot.denote σ ρ))) [] bindingStep =
      .ok (next.denote σ ρ) at loop
    change Except.bind (forIn (parameters.zip (args.map (Slot.denote σ ρ))) [] bindingStep)
      Except.ok = _
    rw [loop]
    rfl

theorem slots_lookup_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (scope : Scope Γ) (ids : List Nat) (args : List (Slot Γ))
    (accepted : ids.mapM (fun id => scope.lookup id) = some args) :
    ids.mapM (IR.lookup (scope.denote σ ρ)) = .ok (args.map (Slot.denote σ ρ)) := by
  induction ids generalizing args with
  | nil =>
    simp only [List.mapM_nil, pure, Option.some.injEq] at accepted
    subst args
    rfl
  | cons id ids ih =>
    simp only [List.mapM_cons, bind, Option.bind] at accepted
    split at accepted
    · contradiction
    · rename_i slot found
      dsimp only at accepted
      cases tail : ids.mapM (fun id => scope.lookup id) with
      | none => simp [tail] at accepted
      | some rest =>
        simp [tail] at accepted
        subst args
        simp [List.mapM_cons, IR.lookup, lookup_denote, found,
          ih rest tail, bind, Except.bind, pure, Except.pure]

/-- Read all source arguments in the old block before constructing the new
scope. The previous block's unpassed bindings are discarded, not carried into
the successor or mistaken for facts at a join/backedge. -/
def edge {Γ : Var → Nat} (types : Nat → IR.Ty) (f : IR.Function)
    (scope : Scope Γ) (successor : IR.Edge) : Option (Scope Γ) := do
  let block ← f.blocks.find? (·.name == successor.block)
  let args ← successor.arguments.mapM (fun id => scope.lookup id)
  bindSlots types block.parameters args

theorem edge_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (types : Nat → IR.Ty) (typed : ∀ id, (ρ id).type = types id)
    (f : IR.Function) (a : IR.Activation) (scope next : Scope Γ) (successor : IR.Edge)
    (values : a.values = scope.denote σ ρ)
    (accepted : edge types f scope successor = some next) :
    IR.jump f a successor =
      .ok {a with block := successor.block, pc := 0, values := next.denote σ ρ} := by
  unfold edge at accepted
  simp only [bind, Option.bind] at accepted
  split at accepted
  · contradiction
  · rename_i block found
    dsimp only at accepted
    split at accepted
    · contradiction
    · rename_i args arguments
      have blockName : block.name = successor.block := by
        have := List.find?_some found
        simpa using this
      simp only [IR.jump, found, values, slots_lookup_sound σ ρ scope _ args arguments,
        bind, Except.bind, bind_slots_sound σ ρ types typed block.parameters args next accepted]
      simp [blockName, pure, Except.pure]

def checkEdge {Γ : Var → Nat} (types : Nat → IR.Ty) (f : IR.Function)
    (scope : Scope Γ) (successor : IR.Edge) (expected : Scope Γ) : Bool :=
  decide (edge types f scope successor = some expected)

/-- An actual unconditional terminator reaches exactly the checked successor
environment. A syntactically accepted edge not at this PC is not sufficient. -/
theorem branch_execution_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (types : Nat → IR.Ty) (typed : ∀ id, (ρ id).type = types id)
    (p : IR.Program) (s : IR.State) (a : IR.Activation) (f : IR.Function) (b : IR.Block)
    (scope next : Scope Γ) (successor : IR.Edge)
    (running : s.control = .running a)
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.name == a.block) = some b)
    (atEnd : a.pc = b.instructions.length)
    (terminator : b.terminator = .branch successor)
    (values : a.values = scope.denote σ ρ)
    (accepted : checkEdge types f scope successor next = true) :
    IR.step p s = some {s with control := .running {a with
      block := successor.block, pc := 0, values := next.denote σ ρ}} := by
  have checked : edge types f scope successor = some next := by
    simpa only [checkEdge, decide_eq_true_eq] using accepted
  have jump := edge_sound σ ρ types typed f a scope next successor values checked
  simp [IR.step, running, IR.execute, function, block, atEnd, terminator, jump,
    bind, Except.bind, pure, Except.pure]

theorem conditional_execution_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (types : Nat → IR.Ty) (typed : ∀ id, (ρ id).type = types id)
    (p : IR.Program) (s : IR.State) (a : IR.Activation) (f : IR.Function) (b : IR.Block)
    (scope yesScope noScope : Scope Γ) (id : Nat) (condition : Term Γ 1)
    (yes no : IR.Edge)
    (running : s.control = .running a)
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.name == a.block) = some b)
    (atEnd : a.pc = b.instructions.length)
    (terminator : b.terminator = .conditional id yes no)
    (values : a.values = scope.denote σ ρ)
    (test : scope.word id = some ⟨1, condition⟩)
    (yesChecked : checkEdge types f scope yes yesScope = true)
    (noChecked : checkEdge types f scope no noScope = true) :
    IR.step p s = some {s with control := .running {a with
      block := if (denoteTerm σ condition).toNat == 0 then no.block else yes.block,
      pc := 0,
      values := if (denoteTerm σ condition).toNat == 0 then noScope.denote σ ρ else yesScope.denote σ ρ}} := by
  have yesEdge : edge types f scope yes = some yesScope := by
    simpa only [checkEdge, decide_eq_true_eq] using yesChecked
  have noEdge : edge types f scope no = some noScope := by
    simpa only [checkEdge, decide_eq_true_eq] using noChecked
  have yesJump := edge_sound σ ρ types typed f a scope yesScope yes values yesEdge
  have noJump := edge_sound σ ρ types typed f a scope noScope no values noEdge
  have lookup := word_lookup_sound σ ρ scope id ⟨1, condition⟩ test
  have lookupAt : IR.lookup a.values id = .ok (.bits 1 (denoteTerm σ condition)) := by
    simpa [values, Word.denote] using lookup
  by_cases zero : (denoteTerm σ condition).toNat == 0
  all_goals simp [IR.step, running, IR.execute, function, block, atEnd, terminator,
    lookupAt, IR.Value.asBits, zero, yesJump, noJump, bind, Except.bind, pure, Except.pure]

end EvoIR.Core.Symbolic
