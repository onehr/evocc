import EvoIR.Core.PrivateStorage
import EvoIR.Core.SymbolicEdge
import EvoIR.Core.SymbolicStorage

/-! Storage-aware symbolic checkpoints. A token bank is finite syntax within
one checked span, not an execution history. Concrete memory is existentially
rebound at every join/backedge, retaining all actual object identities. -/
namespace EvoIR.Core.Symbolic

def privateToken (values : List IR.Value) (id : Nat) : IR.Value :=
  values[id]?.getD (.poison (.bits 0))

def privateTokenType (types : List IR.Ty) (id : Nat) : IR.Ty :=
  types[id]?.getD (.bits 0)

def Slot.inBank {Γ : Var → Nat} (size : Nat) : Slot Γ → Bool
  | .word _ => true
  | .opaque id => id < size

def Scope.inBank {Γ : Var → Nat} (scope : Scope Γ) (size : Nat) : Bool :=
  scope.all (fun entry => entry.2.inBank size)

theorem private_token_typed (values : List IR.Value) (types : List IR.Ty)
    (typed : values.map IR.Value.type = types) (id : Nat) :
    (privateToken values id).type = privateTokenType types id := by
  subst types
  simp only [privateToken, privateTokenType, List.getElem?_map]
  cases values[id]? <;> rfl

theorem private_token_old (values added : List IR.Value) (id : Nat) (bound : id < values.length) :
    privateToken (values ++ added) id = privateToken values id := by
  simp [privateToken, List.getElem?_append_left bound]

theorem scope_private_extension {Γ : Var → Nat} (σ : Subst Γ) (scope : Scope Γ)
    (values added : List IR.Value) (bounded : scope.inBank values.length = true) :
    scope.denote σ (privateToken (values ++ added)) = scope.denote σ (privateToken values) := by
  apply List.map_congr_left
  intro entry member
  obtain ⟨id, slot⟩ := entry
  have bound := List.all_eq_true.mp bounded (id, slot) member
  cases slot with
  | word word => rfl
  | «opaque» key =>
    have small : key < values.length := by simpa [Slot.inBank] using bound
    simp only [Slot.denote, private_token_old values added key small]

structure PrivateCell (Γ : Var → Nat) where
  pointer : Nat
  width : Nat
  live : Bool
  value : Option (Term Γ width)
  deriving DecidableEq

def PrivateCell.id {Γ : Var → Nat} (values : List IR.Value) (cell : PrivateCell Γ) : Nat :=
  match privateToken values cell.pointer with
  | .pointer id 0 => id
  | _ => 0

def PrivateCell.bytes {Γ : Var → Nat} (σ : Subst Γ) (cell : PrivateCell Γ) :
    List (Option (BitVec 8)) :=
  match cell.value with
  | none => List.replicate (cell.width/8) none
  | some term => IR.scalarBytes cell.width (denoteTerm σ term)

def PrivateCell.Realizes {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (incoming : IR.Memory) (owner : Nat) (m : IR.Memory) (cell : PrivateCell Γ) : Prop :=
  privateToken values cell.pointer = .pointer (cell.id values) 0 ∧
  incoming.nextId ≤ cell.id values ∧
  ∃ o, m.objects.find? (·.id == cell.id values) = some o ∧
    o.id = cell.id values ∧ o.owner = owner ∧ o.live = cell.live ∧
    o.bytes = cell.bytes σ ∧ cell.width/8 ≤ o.align

structure PrivateConfig (Γ : Var → Nat) where
  scope : Scope Γ
  tokens : List IR.Ty
  memory : Nat
  world : Nat
  cells : List (PrivateCell Γ)
  deriving DecidableEq

def PrivateConfig.memoryValue {Γ : Var → Nat} (values : List IR.Value)
    (config : PrivateConfig Γ) : Option IR.Memory :=
  match privateToken values config.memory with
  | .memory m => some m
  | _ => none

def PrivateConfig.WellShaped {Γ : Var → Nat} (config : PrivateConfig Γ) : Bool :=
  config.scope.inBank config.tokens.length &&
  config.memory < config.tokens.length &&
  privateTokenType config.tokens config.memory == .memory &&
  config.world < config.tokens.length &&
  privateTokenType config.tokens config.world == .world &&
  decide (config.cells.map PrivateCell.pointer).Nodup &&
  config.cells.all (fun cell => cell.pointer < config.tokens.length &&
    privateTokenType config.tokens cell.pointer == .pointer &&
    (cell.width == 32 || cell.width == 64))

/-- Incoming caller bytes, private ownership, fresh identities, initialized
bytes and the advanced call-frame counter are all semantic premises carried
by a checkpoint. None is read from a producer assertion. -/
def PrivateConfig.Realizes {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (incoming : IR.Memory) (owner nextFrame : Nat) (config : PrivateConfig Γ) : Prop :=
  config.WellShaped = true ∧ values.map IR.Value.type = config.tokens ∧
  privateToken values config.world = .world ∧ incoming.WellAllocated ∧
  ∃ m, config.memoryValue values = some m ∧ m.WellAllocated ∧
    m.BeforeFrame nextFrame ∧ owner < nextFrame ∧ incoming.PrivateFrame owner m ∧
    (∀ a ∈ config.cells, ∀ b ∈ config.cells, a.pointer ≠ b.pointer →
      a.id values ≠ b.id values) ∧
    ∀ cell ∈ config.cells, cell.Realizes σ values incoming owner m

def privateState {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (f : IR.Function) (label : String) (pc owner nextFrame : Nat)
    (config : PrivateConfig Γ) (stack : List IR.ReturnFrame) : IR.State :=
  ⟨.running ⟨f.name, label, pc, config.scope.denote σ (privateToken values), owner⟩,
    stack, nextFrame⟩

theorem private_cell_bytes_length {Γ : Var → Nat} (σ : Subst Γ) (cell : PrivateCell Γ) :
    (cell.bytes σ).length = cell.width/8 := by
  cases value : cell.value <;> simp [PrivateCell.bytes, IR.scalarBytes, value]

theorem private_cell_bytes_live {Γ : Var → Nat} (σ : Subst Γ) (cell : PrivateCell Γ) (live : Bool) :
    ({cell with live} : PrivateCell Γ).bytes σ = cell.bytes σ := by
  cases cell with
  | mk pointer width oldLive value => cases value <;> rfl

theorem private_cell_id_old {Γ : Var → Nat} (values added : List IR.Value)
    (cell : PrivateCell Γ) (bound : cell.pointer < values.length) :
    cell.id (values ++ added) = cell.id values := by
  simp [PrivateCell.id, private_token_old values added cell.pointer bound]

/-- Source/target storage links retain exact six-way C types. A private cell
may stand for an initialized local, while an SSA word may stand for another;
neither route changes the source store or its lexical ordering. -/
inductive PrivateLocalLink where
  | ssa (target source : Nat) (type : Source.IntType)
  | cell (pointer source : Nat) (type : Source.IntType)
  deriving DecidableEq

def PrivateConfig.localWord {Γ : Var → Nat} (config : PrivateConfig Γ)
    (link : PrivateLocalLink) : Option (CWord Γ) := do
  match link with
  | .ssa id _ type =>
    let word ← config.scope.word id
    let term ← word.at type.width
    return ⟨type, term⟩
  | .cell id _ type =>
    let cell ← config.cells.find? (·.pointer == id)
    if !cell.live then none else
    let term ← cell.value
    if same : cell.width = type.width then return ⟨type, same ▸ term⟩ else none

def PrivateLocalLink.source : PrivateLocalLink → Nat
  | .ssa _ symbol _ | .cell _ symbol _ => symbol

def Scope.appendPrivate {Γ : Var → Nat} (scope : Scope Γ) (types : List IR.Ty)
    (results : List IR.Binding) (args : List (Slot Γ)) : Option (Scope Γ) := do
  let fresh ← bindSlots (privateTokenType types) results args
  if fresh.any (fun (id, _) => (scope.lookup id).isSome) then none else some (fresh ++ scope)

theorem append_private_sound {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (scope next : Scope Γ) (types : List IR.Ty) (results : List IR.Binding)
    (args : List (Slot Γ)) (typed : values.map IR.Value.type = types)
    (accepted : scope.appendPrivate types results args = some next) :
    IR.appendResults (scope.denote σ (privateToken values)) results
      (args.map (Slot.denote σ (privateToken values))) =
        .ok (next.denote σ (privateToken values)) := by
  unfold Scope.appendPrivate at accepted
  simp only [Bind.bind, Option.bind] at accepted
  split at accepted
  · contradiction
  · rename_i fresh bound
    dsimp only at accepted
    split at accepted
    · contradiction
    · rename_i disjoint
      cases Option.some.inj accepted
      have binding := bind_slots_sound σ (privateToken values) (privateTokenType types)
        (private_token_typed values types typed) _ _ _ bound
      have distinct : ((fresh.denote σ (privateToken values)).any
          (fun (id, _) => ((scope.denote σ (privateToken values)).lookup id).isSome)) = false := by
        change (fresh.map (fun x => (x.1, x.2.denote σ (privateToken values)))).any _ = false
        rw [List.any_map]
        simpa only [Function.comp_def, lookup_denote, Option.isSome_map]
          using Bool.eq_false_iff.mpr disjoint
      dsimp only at distinct
      rw [IR.appendResults, binding]
      simp only [Bind.bind, Except.bind]
      rw [distinct]
      simp [Scope.denote, List.map_append, pure, Except.pure]

theorem private_token_new (values added : List IR.Value) (id : Nat) :
    privateToken (values ++ added) (values.length+id) = privateToken added id := by
  have bound : values.length ≤ values.length + id := by omega
  simp [privateToken, List.getElem?_append_right bound]

theorem PrivateConfig.scope_bound {Γ : Var → Nat} {config : PrivateConfig Γ}
    (shaped : config.WellShaped = true) : config.scope.inBank config.tokens.length = true := by
  simp only [PrivateConfig.WellShaped, Bool.and_eq_true] at shaped
  exact shaped.1.1.1.1.1.1

theorem PrivateConfig.cell_shape {Γ : Var → Nat} {config : PrivateConfig Γ}
    (shaped : config.WellShaped = true) (cell : PrivateCell Γ) (member : cell ∈ config.cells) :
    cell.pointer < config.tokens.length ∧ (cell.width = 32 ∨ cell.width = 64) := by
  simp only [PrivateConfig.WellShaped, Bool.and_eq_true] at shaped
  have selected := List.all_eq_true.mp shaped.2 cell member
  simp only [Bool.and_eq_true, Bool.or_eq_true, beq_iff_eq, decide_eq_true_eq] at selected
  exact ⟨selected.1.1, selected.2⟩

theorem private_cell_id_pointer {Γ : Var → Nat} (values : List IR.Value)
    (a b : PrivateCell Γ) (same : a.pointer = b.pointer) : a.id values = b.id values := by
  simp only [PrivateCell.id, same]

theorem PrivateConfig.current_token {Γ : Var → Nat} {config : PrivateConfig Γ}
    {values : List IR.Value} {m : IR.Memory} (current : config.memoryValue values = some m) :
    privateToken values config.memory = .memory m := by
  unfold PrivateConfig.memoryValue at current
  split at current <;> simp_all

end EvoIR.Core.Symbolic
