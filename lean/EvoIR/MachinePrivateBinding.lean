import EvoIR.MachinePrivateBody
import EvoIR.MachineEntry

/-! Positional private SSA bindings keep unbounded pointer identities outside
the scalar term language. The full actual argument vector is bound first;
the integer overlay and the named pointer links are then justified from that
same concrete binding. No destination is supplied by an ABI convention. -/
namespace EvoIR.Core.Symbolic.MachineSelection

inductive PrivateInput (Γ : Var → Nat) where
  | word (source : Nat) (target : EvoIR.Machine.VReg) (word : Word Γ)
  | pointer (source : Nat) (target : EvoIR.Machine.VReg) (key : Nat)
  deriving DecidableEq

def PrivateInput.target {Γ : Var → Nat} : PrivateInput Γ → EvoIR.Machine.VReg
  | .word _ target _ | .pointer _ target _ => target

def PrivateInput.source {Γ : Var → Nat} : PrivateInput Γ → Nat
  | .word source _ _ | .pointer source _ _ => source

def PrivateInput.followsScope {Γ : Var → Nat} (scope : Scope Γ) : PrivateInput Γ → Prop
  | .word source _ scalar => scope.word source = some scalar
  | .pointer source _ key => scope.lookup source = some (.opaque key)

instance {Γ : Var → Nat} (scope : Scope Γ) (input : PrivateInput Γ) :
    Decidable (input.followsScope scope) := by
  cases input <;> unfold PrivateInput.followsScope <;> infer_instance

def privatePointerValue (values : List IR.Value) (key : Nat) : EvoIR.Machine.Value :=
  match privateToken values key with
  | .pointer id offset => .pointer id offset
  | _ => .int 0

def PrivateInput.value {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value) :
    PrivateInput Γ → EvoIR.Machine.Value
  | .word _ _ scalar => .int ((denoteTerm σ scalar.2).setWidth 64)
  | .pointer _ _ key => privatePointerValue values key

def privateBindingWords {Γ : Var → Nat} (inputs : List (PrivateInput Γ)) : Bank Γ :=
  inputs.filterMap fun input => match input with
    | .word _ target word => some (target.id, .word (.normalize target.class (.input word)))
    | .pointer _ _ _ => none

def privateBindingPointers {Γ : Var → Nat} (inputs : List (PrivateInput Γ)) : List PrivatePointer :=
  inputs.filterMap fun input => match input with
    | .word _ _ _ => none
    | .pointer source target _ => some ⟨source, target⟩

def PrivateInputsBound {Γ : Var → Nat} (inputs : List (PrivateInput Γ))
    (values : List IR.Value) (source : IR.Bindings) : Prop :=
  ∀ name reg key, PrivateInput.pointer name reg key ∈ inputs →
    ∃ id, IR.lookup source name = .ok (.pointer id 0) ∧
      privateToken values key = .pointer id 0

def privateBindingValues {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (inputs : List (PrivateInput Γ)) : List (EvoIR.Machine.VReg × EvoIR.Machine.Value) :=
  inputs.map fun input => (input.target, input.value σ values)

private theorem lookup_member {α : Type} (entries : List (Nat × α)) (id : Nat) (value : α)
    (found : entries.lookup id = some value) : (id, value) ∈ entries := by
  induction entries with
  | nil => simp at found
  | cons entry rest ih =>
    rcases entry with ⟨key, slot⟩
    by_cases same : id = key
    · subst key
      have equal : slot = value := by simpa [List.lookup_cons] using found
      subst slot
      exact List.mem_cons_self
    · apply List.mem_cons_of_mem
      apply ih
      simpa [List.lookup_cons, beq_eq_false_iff_ne.mpr same] using found

private theorem native_word_normalize {Γ : Var → Nat} (σ : Subst Γ)
    (reg : EvoIR.Machine.VReg) (word : Word Γ) :
    (BankSlot.word (.normalize reg.class (.input word))).denote σ =
      .value (EvoIR.Machine.normalize reg.class (.int ((denoteTerm σ word.2).setWidth 64))) := by
  cases regClass : reg.class <;>
    simp [BankSlot.denote, WordExpr.denote, EvoIR.Machine.normalize, EvoIR.Machine.normalizeAt,
      EvoIR.Machine.Width.bits, regClass]

theorem private_binding_models {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (inputs : List (PrivateInput Γ)) (source : IR.Bindings)
    (unique : (inputs.map (fun input => input.target.id)).Nodup)
    (pointers : PrivateInputsBound inputs values source) :
    let regs := EvoIR.Machine.bindValues [] (privateBindingValues σ values inputs)
    (privateBindingWords inputs).Models σ regs ∧
      PrivatePointer.Models (privateBindingPointers inputs) source regs := by
  dsimp only
  have concreteUnique : ((privateBindingValues σ values inputs).map (fun pair => pair.1.id)).Nodup := by
    simpa only [privateBindingValues, List.map_map, Function.comp_def] using unique
  constructor
  · intro id slot found
    have member := lookup_member (privateBindingWords inputs) id slot found
    obtain ⟨input, included, entry⟩ := List.mem_filterMap.mp member
    cases input with
    | word name reg word =>
      have equal : (reg.id, BankSlot.word (.normalize reg.class (.input word))) = (id, slot) := by
        simpa only [privateBindingWords, Option.some.injEq] using entry
      cases equal
      have bound := EvoIR.Machine.bindValues_lookup_member [] (privateBindingValues σ values inputs)
        concreteUnique reg (.int ((denoteTerm σ word.2).setWidth 64))
        (List.mem_map.mpr ⟨.word name reg word, included, rfl⟩)
      simpa only [native_word_normalize] using bound
    | pointer name reg key => simp [privateBindingWords] at entry
  · intro link included
    obtain ⟨input, member, entry⟩ := List.mem_filterMap.mp included
    cases input with
    | word name reg word => simp [privateBindingPointers] at entry
    | pointer name reg key =>
      have equal : (⟨name, reg⟩ : PrivatePointer) = link := by
        simpa only [privateBindingPointers, Option.some.injEq] using entry
      cases equal
      obtain ⟨id, sourceRead, actual⟩ := pointers name reg key member
      have bound := EvoIR.Machine.bindValues_lookup_member [] (privateBindingValues σ values inputs)
        concreteUnique reg (.pointer id 0)
        (List.mem_map.mpr ⟨.pointer name reg key, member, by
          simp only [PrivateInput.target, PrivateInput.value, privatePointerValue, actual]⟩)
      refine ⟨id, sourceRead, ?_⟩
      simp [EvoIR.Machine.readOperand, EvoIR.Machine.readSlot, bound, EvoIR.Machine.normalize]

end EvoIR.Core.Symbolic.MachineSelection
