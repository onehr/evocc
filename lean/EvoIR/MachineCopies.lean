import EvoIR.Core.CheckpointSimulation

/-!
Result checking for actual sequential edge-copy lists. The checker does not
reproduce the producer's scheduling algorithm. It evaluates the proposed moves
on symbolic register origins and compares the demanded outputs with a parallel
assignment. The soundness theorem quantifies over every initial register store.
Scratch registers may differ only when they are absent from the demanded set.
-/
namespace EvoIR.Machine.Copies

abbrev Copy := Nat × Nat
abbrev Store (α : Type) := Nat → α

def write (store : Store α) (dst : Nat) (value : α) : Store α :=
  fun reg => if reg = dst then value else store reg

@[simp] theorem write_same (store : Store α) (dst : Nat) (value : α) :
    write store dst value dst = value := by simp [write]

@[simp] theorem write_other (store : Store α) (dst reg : Nat) (value : α)
    (different : reg ≠ dst) : write store dst value reg = store reg := by
  simp [write, different]

def sequential (moves : List Copy) (store : Store α) : Store α :=
  moves.foldl (fun current move => write current move.1 (current move.2)) store

/-- Each source is read from the initial store, not from previous writes. -/
def parallel (moves : List Copy) (store : Store α) : Store α :=
  moves.foldl (fun current move => write current move.1 (store move.2)) store

def sequentialOrigins (moves : List Copy) : Store Nat := sequential moves id
def parallelOrigins (moves : List Copy) : Store Nat := parallel moves id

theorem write_map (f : α → β) (store : Store α) (dst : Nat) (value : α) :
    write (fun reg => f (store reg)) dst (f value) =
      fun reg => f (write store dst value reg) := by
  funext reg
  by_cases h : reg = dst <;> simp [write, h]

theorem sequential_map (f : α → β) (moves : List Copy) (store : Store α) :
    sequential moves (fun reg => f (store reg)) =
      fun reg => f (sequential moves store reg) := by
  induction moves generalizing store with
  | nil => rfl
  | cons move rest ih =>
    simp only [sequential, List.foldl_cons]
    rw [write_map]
    exact ih _

private theorem parallel_fold_map (f : α → β) (moves : List Copy)
    (initial current : Store α) :
    moves.foldl (fun state move => write state move.1 (f (initial move.2)))
      (fun reg => f (current reg)) =
    fun reg => f (moves.foldl (fun state move =>
      write state move.1 (initial move.2)) current reg) := by
  induction moves generalizing current with
  | nil => rfl
  | cons move rest ih =>
    simp only [List.foldl_cons]
    rw [write_map]
    exact ih _

theorem parallel_map (f : α → β) (moves : List Copy) (store : Store α) :
    parallel moves (fun reg => f (store reg)) =
      fun reg => f (parallel moves store reg) := by
  exact parallel_fold_map f moves store store

theorem sequential_origins (moves : List Copy) (store : Store α) (reg : Nat) :
    sequential moves store reg = store (sequentialOrigins moves reg) := by
  have h := congrFun (sequential_map store moves (id : Store Nat)) reg
  simpa only [sequentialOrigins, id_eq] using h

theorem parallel_origins (moves : List Copy) (store : Store α) (reg : Nat) :
    parallel moves store reg = store (parallelOrigins moves reg) := by
  have h := congrFun (parallel_map store moves (id : Store Nat)) reg
  simpa only [parallelOrigins, id_eq] using h

/-- All supplied live outputs are compared, including non-destinations. This
also rejects a scratch write that overwrites an independently live value. -/
def check (live : List Nat) (edge : List Copy) (moves : List Copy) : Bool :=
  live.all (fun reg => decide (sequentialOrigins moves reg = parallelOrigins edge reg))

theorem check_sound (live : List Nat) (edge moves : List Copy)
    (accepted : check live edge moves = true) (store : Store α) :
    ∀ reg ∈ live, sequential moves store reg = parallel edge store reg := by
  intro reg member
  have all : ∀ reg ∈ live,
      decide (sequentialOrigins moves reg = parallelOrigins edge reg) = true := by
    simpa only [check, List.all_eq_true] using accepted
  have same : sequentialOrigins moves reg = parallelOrigins edge reg :=
    of_decide_eq_true (all reg member)
  rw [sequential_origins, parallel_origins, same]

/-- The same concrete list can be checked against an already proposed entry
origin map. The map is an annotation, not an assertion of equivalence. Its
concrete interpretation is supplied by a separately checked entry relation. -/
def checkFrom (live : List Nat) (expected actual : Store Nat)
    (moves : List Copy) : Bool :=
  live.all (fun reg => decide (sequential moves actual reg = expected reg))

theorem checkFrom_sound (live : List Nat) (expected actual : Store Nat)
    (moves : List Copy) (accepted : checkFrom live expected actual moves = true)
    (valuation : Store α) :
    ∀ reg ∈ live,
      sequential moves (fun r => valuation (actual r)) reg = valuation (expected reg) := by
  intro reg member
  have all : ∀ reg ∈ live,
      decide (sequential moves actual reg = expected reg) = true := by
    simpa only [checkFrom, List.all_eq_true] using accepted
  have same : sequential moves actual reg = expected reg :=
    of_decide_eq_true (all reg member)
  rw [congrFun (sequential_map valuation moves actual) reg, same]

end EvoIR.Machine.Copies
