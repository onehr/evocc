import EvoIR.Core.SymbolicBindings

/-! Checked local diagrams retain an ordered trace of assumptions. A false
guard stops at its original diagnostic; terms may be constructed after it,
but their denotation cannot bypass that failure. Unsupported operations are
refusals, not opaque instructions asserted to preserve state. -/
namespace EvoIR.Core.Symbolic

structure Guard (Γ : Var → Nat) where
  reason : IR.Reason
  condition : Term Γ 1
  deriving DecidableEq

def guards {Γ : Var → Nat} (σ : Subst Γ) : List (Guard Γ) →
    Except Source.Fault Unit
  | [] => .ok ()
  | g :: rest =>
    if (denoteTerm σ g.condition).toNat == 0 then .error (.diagnostic g.reason.code)
    else guards σ rest

def operation {Γ : Var → Nat} : IR.Operation → List (Word Γ) →
    Option (List (Guard Γ) × List (Word Γ))
  | .constant n bits, [] =>
    if n == 0 || bits ≥ 2^n then none
    else some ([], [⟨n, .const n (BitVec.ofNat n bits)⟩])
  | .scalar op, args => do return ([], [← scalar op args])
  | .assume reason, [arg] => do return ([⟨reason, ← arg.at 1⟩], [])
  | _, _ => none

theorem operation_sound {Γ : Var → Nat} (σ : Subst Γ) (owner : Nat)
    (op : IR.Operation) (args results : List (Word Γ)) (checks : List (Guard Γ))
    (accepted : operation op args = some (checks, results)) :
    IR.evalOperation owner op (args.map (Word.denote σ)) =
      (do guards σ checks; return results.map (Word.denote σ)) := by
  cases op with
  | constant n bits =>
    cases args with
    | cons _ _ => simp [operation] at accepted
    | nil =>
      simp only [operation] at accepted
      split at accepted
      · contradiction
      · rename_i valid
        cases Option.some.inj accepted
        simp [IR.evalOperation, valid, guards, Word.denote, denoteTerm,
          pure, Except.pure, bind, Except.bind]
  | scalar op =>
    simp only [operation, bind, Option.bind, pure] at accepted
    split at accepted
    · contradiction
    · rename_i result found
      cases Option.some.inj accepted
      simp [IR.evalOperation, scalar_sound σ op args result found, guards,
        Functor.map, Except.map]
  | assume reason =>
    cases args with
    | nil => simp [operation] at accepted
    | cons arg rest =>
      cases rest with
      | cons _ _ => simp [operation] at accepted
      | nil =>
        simp only [operation, bind, Option.bind, pure] at accepted
        split at accepted
        · contradiction
        · rename_i condition found
          cases Option.some.inj accepted
          simp only [List.map_cons, List.map_nil, IR.evalOperation,
            word_at_sound σ arg 1 condition found, bind, Except.bind, guards]
          split <;> rfl
  | _ => simp [operation] at accepted

def Scope.appendWords {Γ : Var → Nat} (scope : Scope Γ)
    (results : List IR.Binding) : List (Word Γ) → Option (Scope Γ)
  | [] => if results.isEmpty then some scope else none
  | [w] => scope.appendWord results w
  | _ => none

theorem append_words_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (scope next : Scope Γ) (results : List IR.Binding) (words : List (Word Γ))
    (accepted : scope.appendWords results words = some next) :
    IR.appendResults (scope.denote σ ρ) results (words.map (Word.denote σ)) =
      .ok (next.denote σ ρ) := by
  cases words with
  | nil =>
    cases results with
    | cons _ _ => simp [Scope.appendWords] at accepted
    | nil =>
      simp only [Scope.appendWords, List.isEmpty_nil, ↓reduceIte,
        Option.some.injEq] at accepted
      subst next
      simp [IR.appendResults, IR.bind, pure, Except.pure]
      rfl
  | cons w rest =>
    cases rest with
    | nil => exact append_word_sound σ ρ scope next results w accepted
    | cons _ _ => simp [Scope.appendWords] at accepted

theorem operands_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (scope : Scope Γ) (ids : List Nat) (args : List (Word Γ))
    (accepted : ids.mapM scope.word = some args) :
    ids.mapM (IR.lookup (scope.denote σ ρ)) = .ok (args.map (Word.denote σ)) := by
  induction ids generalizing args with
  | nil =>
    simp only [List.mapM_nil, pure, Option.some.injEq] at accepted
    subst args
    rfl
  | cons id ids ih =>
    simp only [List.mapM_cons, bind, Option.bind] at accepted
    split at accepted
    · contradiction
    · rename_i word found
      dsimp only at accepted
      cases tail : ids.mapM scope.word with
      | none => simp [tail] at accepted
      | some tailArgs =>
        simp [tail] at accepted
        subst args
        simp [List.mapM_cons, word_lookup_sound σ ρ scope id word found, ih tailArgs tail]
        rfl

def instruction {Γ : Var → Nat} (scope : Scope Γ) (inst : IR.Instruction) :
    Option (List (Guard Γ) × Scope Γ) := do
  let args ← inst.operands.mapM scope.word
  let (checks, results) ← operation inst.op args
  return (checks, ← scope.appendWords inst.results results)

/-- This is exactly the non-call arm of `IR.execute`, not a substitute
instruction semantics. Program counters and calls remain outer obligations. -/
def concreteInstruction (owner : Nat) (values : IR.Bindings) (inst : IR.Instruction) :
    Except Source.Fault IR.Bindings := do
  let args ← inst.operands.mapM (IR.lookup values)
  let results ← IR.evalOperation owner inst.op args
  IR.appendResults values inst.results results

theorem instruction_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (owner : Nat) (scope next : Scope Γ) (inst : IR.Instruction) (checks : List (Guard Γ))
    (accepted : instruction scope inst = some (checks, next)) :
    concreteInstruction owner (scope.denote σ ρ) inst =
      (do guards σ checks; return next.denote σ ρ) := by
  simp only [instruction, bind, Option.bind, pure] at accepted
  split at accepted
  · contradiction
  · rename_i args hargs
    dsimp only at accepted
    split at accepted
    · contradiction
    · rename_i output hop
      rcases output with ⟨trace, results⟩
      dsimp only at accepted
      split at accepted
      · contradiction
      · rename_i after happend
        cases Option.some.inj accepted
        simp only [concreteInstruction, operands_sound σ ρ scope inst.operands args hargs,
          bind, Except.bind, operation_sound σ owner inst.op args results checks hop]
        cases hchecks : guards σ checks with
        | error fault => rfl
        | ok unitValue =>
          cases unitValue
          simpa [pure, Except.pure] using
            append_words_sound σ ρ scope next inst.results results happend

theorem instruction_not_call {Γ : Var → Nat} (scope next : Scope Γ)
    (inst : IR.Instruction) (checks : List (Guard Γ))
    (accepted : instruction scope inst = some (checks, next)) (name : String) :
    inst.op ≠ .call name := by
  intro same
  simp only [instruction, same, bind, Option.bind] at accepted
  split at accepted <;> simp [operation] at accepted

/-- Connect the local diagram to the original machine's instruction arm.
Function/block membership and the exact PC are premises, not producer claims
silently consumed by this component. The enclosing CFG checker must discharge
them against the decoded program. -/
theorem execute_instruction {p : IR.Program} {s : IR.State} {a : IR.Activation}
    {f : IR.Function} {b : IR.Block} {inst : IR.Instruction}
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.name == a.block) = some b)
    (atPc : b.instructions[a.pc]? = some inst)
    (noncall : ∀ name, inst.op ≠ .call name) :
    IR.execute p s a = do
      let values ← concreteInstruction a.owner a.values inst
      return {s with control := .running {a with pc := a.pc+1, values}} := by
  cases op : inst.op <;> simp only [IR.execute, function, block, atPc, op]
  all_goals first | exact False.elim (noncall _ op) | skip
  all_goals
    unfold concreteInstruction
    rw [op]
    cases args : inst.operands.mapM (IR.lookup a.values) with
    | error fault => rfl
    | ok values =>
      dsimp only [bind, Except.bind]
      split <;> rfl

theorem instruction_execution_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    {p : IR.Program} {s : IR.State} {a : IR.Activation}
    {f : IR.Function} {b : IR.Block} {inst : IR.Instruction}
    (scope next : Scope Γ) (checks : List (Guard Γ))
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.name == a.block) = some b)
    (atPc : b.instructions[a.pc]? = some inst)
    (values : a.values = scope.denote σ ρ)
    (accepted : instruction scope inst = some (checks, next)) :
    IR.execute p s a = do
      guards σ checks
      return {s with control := .running {a with pc := a.pc+1, values := next.denote σ ρ}} := by
  rw [execute_instruction function block atPc (instruction_not_call scope next inst checks accepted)]
  rw [values, instruction_sound σ ρ a.owner scope next inst checks accepted]
  cases guards σ checks <;> rfl

def instructions {Γ : Var → Nat} (scope : Scope Γ) : List IR.Instruction →
    Option (List (Guard Γ) × Scope Γ)
  | [] => some ([], scope)
  | inst :: rest => do
    let (checks, next) ← instruction scope inst
    let (later, finalScope) ← instructions next rest
    return (checks ++ later, finalScope)

def concreteInstructions (owner : Nat) (values : IR.Bindings) :
    List IR.Instruction → Except Source.Fault IR.Bindings
  | [] => .ok values
  | inst :: rest => do
    let next ← concreteInstruction owner values inst
    concreteInstructions owner next rest

theorem guards_append {Γ : Var → Nat} (σ : Subst Γ) (first later : List (Guard Γ)) :
    guards σ (first ++ later) = (do guards σ first; guards σ later) := by
  induction first with
  | nil => rfl
  | cons g rest ih =>
    simp only [List.cons_append, guards]
    split
    · rfl
    · exact ih

theorem instructions_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (owner : Nat) (scope finalScope : Scope Γ) (code : List IR.Instruction)
    (checks : List (Guard Γ))
    (accepted : instructions scope code = some (checks, finalScope)) :
    concreteInstructions owner (scope.denote σ ρ) code =
      (do guards σ checks; return finalScope.denote σ ρ) := by
  induction code generalizing scope finalScope checks with
  | nil =>
    cases Option.some.inj accepted
    rfl
  | cons inst rest ih =>
    simp only [instructions, bind, Option.bind, pure] at accepted
    split at accepted
    · contradiction
    · rename_i first hfirst
      rcases first with ⟨trace, next⟩
      dsimp only at accepted
      split at accepted
      · contradiction
      · rename_i tail htail
        rcases tail with ⟨later, final⟩
        dsimp only at accepted
        cases Option.some.inj accepted
        simp only [concreteInstructions, instruction_sound σ ρ owner scope next inst trace hfirst,
          guards_append]
        cases guards σ trace with
        | error _ => rfl
        | ok unitValue =>
          cases unitValue
          exact ih next finalScope later htail

/-- The expected trace and outputs must come from a separately justified
source diagram. This checks the actual target instructions against them;
it does not assert that an arbitrary producer-supplied expectation is C. -/
def checkFragment {Γ : Var → Nat} (scope : Scope Γ) (code : List IR.Instruction)
    (outputs : List Nat) (expectedChecks : List (Guard Γ))
    (expectedValues : List (Word Γ)) : Bool :=
  match instructions scope code with
  | none => false
  | some (checks, finalScope) =>
    decide (checks = expectedChecks) &&
      decide (outputs.mapM finalScope.word = some expectedValues)

theorem fragment_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (owner : Nat) (scope : Scope Γ) (code : List IR.Instruction) (outputs : List Nat)
    (expectedChecks : List (Guard Γ)) (expectedValues : List (Word Γ))
    (accepted : checkFragment scope code outputs expectedChecks expectedValues = true) :
    (do let values ← concreteInstructions owner (scope.denote σ ρ) code
        outputs.mapM (IR.lookup values)) =
      (do guards σ expectedChecks; return expectedValues.map (Word.denote σ)) := by
  unfold checkFragment at accepted
  split at accepted
  · contradiction
  · rename_i checks finalScope found
    simp only [Bool.and_eq_true, decide_eq_true_eq] at accepted
    rcases accepted with ⟨same, outputsFound⟩
    subst checks
    rw [instructions_sound σ ρ owner scope finalScope code expectedChecks found]
    cases guards σ expectedChecks with
    | error _ => rfl
    | ok unitValue =>
      cases unitValue
      exact operands_sound σ ρ finalScope outputs expectedValues outputsFound

end EvoIR.Core.Symbolic
