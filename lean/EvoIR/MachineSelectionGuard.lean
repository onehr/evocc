import EvoIR.MachineSelectionBranch

/-!
The concrete source-guided guard-chain interface. A diagram reaches an actual
MIR call/terminator/private-effect cursor on successful guards, or the actual
first BRK diagnostic on failure. It carries arbitrary memory/caller context;
it does not treat the continuation as a complete callee refinement.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def purePrefix : List EvoIR.Machine.Instruction → Nat
  | [] => 0
  | inst :: rest => if inst.pure then purePrefix rest + 1 else 0

structure GuardChainResult (Γ : Var → Nat) where
  block : EvoIR.Machine.Block
  pc : Nat
  bank : Bank Γ

def GuardChainDiagram {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (pc : Nat) (bank : Bank Γ)
    (checks : List (Guard Γ)) (result : GuardChainResult Γ) : Prop :=
  ∀ σ owner nextFrame incoming stack memory, ∃ last,
    EvoIR.Machine.Steps .ssa p (machineState σ f b pc bank owner nextFrame incoming stack memory) last ∧
      match guards σ checks with
      | .error fault => last.control = .halted (.error fault)
      | .ok _ => last = machineState σ f result.block result.pc result.bank
          owner nextFrame incoming stack memory

def Passed {Γ : Var → Nat} (σ : Subst Γ) (seen : List (Guard Γ)) : Prop :=
  ∀ g ∈ seen, (denoteTerm σ g.condition).getLsbD 0 = true

private def GuardChainAuxDiagram {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (pc : Nat) (bank : Bank Γ)
    (seen : List (Guard Γ)) (checks : List (Guard Γ)) (result : GuardChainResult Γ) : Prop :=
  ∀ σ, Passed σ seen → ∀ owner nextFrame incoming stack memory, ∃ last,
    EvoIR.Machine.Steps .ssa p (machineState σ f b pc bank owner nextFrame incoming stack memory) last ∧
      match guards σ checks with
      | .error fault => last.control = .halted (.error fault)
      | .ok _ => last = machineState σ f result.block result.pc result.bank
          owner nextFrame incoming stack memory

private theorem steps_trans {p : EvoIR.Machine.Program} {s t u : EvoIR.Machine.State}
    (first : EvoIR.Machine.Steps .ssa p s t) (rest : EvoIR.Machine.Steps .ssa p t u) :
    EvoIR.Machine.Steps .ssa p s u := by
  induction first with
  | refl => exact rest
  | next edge _ ih => exact .next edge (ih rest)

private theorem steps_of_path {p : EvoIR.Machine.Program} {count : Nat}
    {s t : EvoIR.Machine.State} (path : Transition.Path (EvoIR.Machine.step .ssa p) count s t) :
    EvoIR.Machine.Steps .ssa p s t := by
  induction path with
  | refl => exact .refl _
  | next edge _ ih => exact .next (.execute edge) ih

private theorem guards_pass {Γ : Var → Nat} (σ : Subst Γ) (g : Guard Γ)
    (checks : List (Guard Γ)) (passed : (denoteTerm σ g.condition).getLsbD 0 = true) :
    guards σ (g :: checks) = guards σ checks := by
  have bits : denoteTerm σ g.condition = 1#1 := by
    rw [one_bit_reify (denoteTerm σ g.condition), passed]
    rfl
  simp [guards, bits]

private theorem guards_fail {Γ : Var → Nat} (σ : Subst Γ) (g : Guard Γ)
    (checks : List (Guard Γ)) (failed : (denoteTerm σ g.condition).getLsbD 0 = false) :
    guards σ (g :: checks) = .error (.diagnostic g.reason.code) := by
  have bits : denoteTerm σ g.condition = 0#1 := by
    rw [one_bit_reify (denoteTerm σ g.condition), failed]
    rfl
  simp [guards, bits]

private theorem passed_guards {Γ : Var → Nat} (σ : Subst Γ) (seen : List (Guard Γ))
    (passed : Passed σ seen) : guards σ seen = .ok () := by
  induction seen with
  | nil => rfl
  | cons g rest ih =>
    rw [guards_pass σ g rest (passed g (by simp))]
    exact ih (by intro guard member; exact passed guard (List.mem_cons_of_mem _ member))

private def constantGuard {Γ : Var → Nat} (condition : Term Γ 1) :
    Option (PLift (∀ σ, (denoteTerm σ condition).getLsbD 0 = true)) :=
  if known : constantBranch (canonical condition) = some true then
    some ⟨by
      intro σ
      have good := constant_branch_sound σ (canonical condition) true known
      rw [canonical_sound, ← one_bit_nonzero] at good
      exact good⟩
  else none

private def trapCursor {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (bank : Bank Γ) (code : Nat) :
    Option (PLift (∀ σ owner nextFrame incoming stack memory,
      EvoIR.Machine.step .ssa p (machineState σ f b 0 bank owner nextFrame incoming stack memory) =
        some {(machineState σ f b 0 bank owner nextFrame incoming stack memory) with
          control := .halted (.error (.diagnostic code))})) :=
  if function : p.functions.find? (·.name == f.name) = some f then
    if block : f.blocks.find? (·.id == b.id) = some b then
      if empty : b.insts = [] then
        if term : b.term = .brk code then
          some ⟨by
            intro σ owner nextFrame incoming stack memory
            exact EvoIR.Machine.Selection.step_trap .ssa p
              (machineState σ f b 0 bank owner nextFrame incoming stack memory)
              ⟨f.name, b.id, 0, bankDenote σ bank, owner, incoming⟩ f b code
              rfl function block (by simp [empty]) term⟩
        else none
      else none
    else none
  else none

private theorem prepend_guard_skip {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (pc count : Nat)
    (bank next : Bank Γ) (seen : List (Guard Γ)) (g : Guard Γ)
    (rest : List (Guard Γ)) (result : GuardChainResult Γ)
    (prefixDiagram : SpanDiagram p f b pc count bank next)
    (passed : ∀ σ, Passed σ seen → (denoteTerm σ g.condition).getLsbD 0 = true)
    (tail : GuardChainAuxDiagram p f b (pc+count) next seen rest result) :
    GuardChainAuxDiagram p f b pc bank seen (g :: rest) result := by
  intro σ held owner nextFrame incoming stack memory
  obtain ⟨last, path, finish⟩ := tail σ held owner nextFrame incoming stack memory
  refine ⟨last, steps_trans (steps_of_path (prefixDiagram σ owner nextFrame incoming stack memory)) path, ?_⟩
  rw [guards_pass σ g rest (passed σ held)]
  exact finish

private theorem prepend_guard_branch {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (pc count : Nat)
    (bank next : Bank Γ) (seen : List (Guard Γ)) (g : Guard Γ)
    (rest : List (Guard Γ)) (result : GuardChainResult Γ)
    (prefixDiagram : SpanDiagram p f b pc count bank next) (atEnd : pc+count = b.insts.length)
    (choice : BranchChoice Γ f next) (branched : BranchUnderDiagram p f b next g.condition seen choice)
    (trapped : ∀ σ owner nextFrame incoming stack memory,
      EvoIR.Machine.step .ssa p
        (machineState σ f choice.no.result.block 0 choice.no.result.bank owner nextFrame incoming stack memory) =
        some {(machineState σ f choice.no.result.block 0 choice.no.result.bank
          owner nextFrame incoming stack memory) with
            control := .halted (.error (.diagnostic g.reason.code))})
    (tail : GuardChainAuxDiagram p f choice.yes.result.block 0 choice.yes.result.bank
      (g :: seen) rest result) :
    GuardChainAuxDiagram p f b pc bank seen (g :: rest) result := by
  intro σ held owner nextFrame incoming stack memory
  have prefixPath := steps_of_path (prefixDiagram σ owner nextFrame incoming stack memory)
  rw [atEnd] at prefixPath
  have branch := branched σ owner nextFrame incoming stack memory (passed_guards σ seen held)
  cases truth : (denoteTerm σ g.condition).getLsbD 0 with
  | false =>
    simp only [truth, Bool.false_eq_true, ↓reduceIte] at branch
    let stopped := {(machineState σ f choice.no.result.block 0 choice.no.result.bank
        owner nextFrame incoming stack memory) with
      control := .halted (.error (.diagnostic g.reason.code))}
    refine ⟨stopped, steps_trans prefixPath
      (.next (.execute branch) (.next (.execute (trapped σ owner nextFrame incoming stack memory)) (.refl _))), ?_⟩
    rw [guards_fail σ g rest truth]
  | true =>
    simp only [truth, ↓reduceIte] at branch
    have passed : Passed σ (g :: seen) := by
      intro guard member
      cases List.mem_cons.mp member with
      | inl same => simpa only [same] using truth
      | inr member => exact held guard member
    obtain ⟨last, path, finish⟩ := tail σ passed owner nextFrame incoming stack memory
    refine ⟨last, steps_trans prefixPath (.next (.execute branch) path), ?_⟩
    rw [guards_pass σ g rest truth]
    exact finish

/-- Scan actual MIR pure prefixes and each actual conditional terminator in
the ordered source guard trace. A skipped guard must be proved constant true
or already have passed; no arithmetic count restriction is globally assumed.
The recursion is on the finite guard trace, not execution fuel. -/
private def guardChainAux {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) : (checks : List (Guard Γ)) →
    (seen : List (Guard Γ)) → (b : EvoIR.Machine.Block) → (pc : Nat) → (bank : Bank Γ) →
    Option {result : GuardChainResult Γ // GuardChainAuxDiagram p f b pc bank seen checks result}
  | [], seen, b, pc, bank => do
    let count := purePrefix (b.insts.drop pc)
    let span ← machineSpan p f b count pc bank
    return ⟨⟨b, pc+count, span.val⟩, by
      intro σ held owner nextFrame incoming stack memory
      exact ⟨_, steps_of_path (span.property σ owner nextFrame incoming stack memory), rfl⟩⟩
  | g :: rest, seen, b, pc, bank => do
    let count := purePrefix (b.insts.drop pc)
    let span ← machineSpan p f b count pc bank
    let checkedBranch := if atEnd : pc+count = b.insts.length then do
      let choice ← branchChoiceUnder p f b span.val g.condition seen
      let trap ← trapCursor p f choice.val.no.result.block choice.val.no.result.bank g.reason.code
      let tail ← guardChainAux p f rest (g :: seen)
        choice.val.yes.result.block 0 choice.val.yes.result.bank
      pure (⟨tail.val, prepend_guard_branch p f b pc count bank span.val seen g rest tail.val
        span.property atEnd choice.val choice.property trap.down tail.property⟩ :
        {result : GuardChainResult Γ // GuardChainAuxDiagram p f b pc bank seen (g :: rest) result})
      else none
    match checkedBranch with
    | some result => some result
    | none => do
      let passed : PLift (∀ σ, Passed σ seen → (denoteTerm σ g.condition).getLsbD 0 = true) ←
        if known : g ∈ seen then some ⟨by
          intro σ held
          exact held g known⟩
        else (constantGuard g.condition).map fun constant =>
          (⟨by intro σ held; exact constant.down σ⟩ :
            PLift (∀ σ, Passed σ seen → (denoteTerm σ g.condition).getLsbD 0 = true))
      let tail ← guardChainAux p f rest seen b (pc+count) span.val
      return ⟨tail.val, prepend_guard_skip p f b pc count bank span.val seen g rest tail.val
        span.property passed.down tail.property⟩

def guardChain {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (pc : Nat) (bank : Bank Γ)
    (checks : List (Guard Γ)) :
    Option {result : GuardChainResult Γ // GuardChainDiagram p f b pc bank checks result} := do
  let result ← guardChainAux p f checks [] b pc bank
  return ⟨result.val, by
    intro σ owner nextFrame incoming stack memory
    exact result.property σ (by intro condition member; cases member)
      owner nextFrame incoming stack memory⟩

def GuardChainUnderDiagram {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (pc : Nat) (bank : Bank Γ)
    (prior checks : List (Guard Γ)) (result : GuardChainResult Γ) : Prop :=
  ∀ σ owner nextFrame incoming stack memory, guards σ prior = .ok () → ∃ last,
    EvoIR.Machine.Steps .ssa p (machineState σ f b pc bank owner nextFrame incoming stack memory) last ∧
      match guards σ checks with
      | .error fault => last.control = .halted (.error fault)
      | .ok _ => last = machineState σ f result.block result.pc result.bank
          owner nextFrame incoming stack memory

/-- Only checks that have already executed successfully justify conditional
matching. The current guard is not added until its real branch passes. -/
def guardChainUnder {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (pc : Nat) (bank : Bank Γ)
    (prior checks : List (Guard Γ)) :
    Option {result : GuardChainResult Γ // GuardChainUnderDiagram p f b pc bank prior checks result} := do
  let result ← guardChainAux p f checks prior b pc bank
  return ⟨result.val, by
    intro σ owner nextFrame incoming stack memory passed
    exact result.property σ (fun g member => guards_member_passed σ prior passed g member)
      owner nextFrame incoming stack memory⟩

end EvoIR.Core.Symbolic.MachineSelection
