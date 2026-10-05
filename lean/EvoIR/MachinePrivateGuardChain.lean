import EvoIR.MachinePrivateGuardEdges
import EvoIR.MachinePrivateGuardPrefix
import EvoIR.MachinePrivateBranch

/-! Ordered checked guards in the actual mixed MIR bank. Each taken failure
edge reaches the actual BRK immediately. A successful edge captures the full
pointer/word vector before binding the fresh continuation bank. -/
namespace EvoIR.Core.Symbolic.MachineSelection

structure PrivateNativeGuardResult (Γ : Var → Nat) where
  block : EvoIR.Machine.Block
  pc : Nat
  bank : PrivateBank Γ

def PrivateNativeGuardDiagram {Γ : Var → Nat} (program : EvoIR.Machine.Program)
    (function : EvoIR.Machine.Function) (block : EvoIR.Machine.Block) (pc : Nat)
    (before : PrivateBank Γ) (prior checks : List (Guard Γ)) (result : PrivateNativeGuardResult Γ) : Prop :=
  result.bank.source = before.source ∧
  ∀ σ values incoming owner frame memory regs logicalInputs stack,
    before.Models σ values incoming owner frame memory regs → guards σ prior = .ok () →
    ∃ last,
      Transition.Reaches (EvoIR.Machine.step .ssa program)
        (privateMachineState function block pc regs owner frame logicalInputs stack memory) last ∧
      match guards σ checks with
      | .error fault => last.control = .halted (.error fault)
      | .ok _ => ∃ nextRegs,
          last = privateMachineState function result.block result.pc nextRegs owner frame logicalInputs stack memory ∧
          result.bank.Models σ values incoming owner frame memory nextRegs

private theorem private_guard_pass {Γ : Var → Nat} (σ : Subst Γ) (g : Guard Γ)
    (rest : List (Guard Γ)) (truth : (denoteTerm σ g.condition).getLsbD 0 = true) :
    guards σ (g::rest) = guards σ rest := by
  have bits : denoteTerm σ g.condition = 1#1 := by
    rw [one_bit_reify (denoteTerm σ g.condition), truth]
    rfl
  simp [guards, bits]

private theorem private_guard_fail {Γ : Var → Nat} (σ : Subst Γ) (g : Guard Γ)
    (rest : List (Guard Γ)) (truth : (denoteTerm σ g.condition).getLsbD 0 = false) :
    guards σ (g::rest) = .error (.diagnostic g.reason.code) := by
  have bits : denoteTerm σ g.condition = 0#1 := by
    rw [one_bit_reify (denoteTerm σ g.condition), truth]
    rfl
  simp [guards, bits]

private theorem private_prior_add {Γ : Var → Nat} (σ : Subst Γ) (prior : List (Guard Γ))
    (g : Guard Γ) (passed : guards σ prior = .ok ())
    (truth : (denoteTerm σ g.condition).getLsbD 0 = true) :
    guards σ (prior++[g]) = .ok () := by
  rw [guards_append, passed, private_guard_pass σ g [] truth]
  rfl

private def privateKnownGuard {Γ : Var → Nat} (prior : List (Guard Γ)) (g : Guard Γ) :
    Option (PLift (∀ σ, guards σ prior = .ok () → (denoteTerm σ g.condition).getLsbD 0 = true)) :=
  if known : g ∈ prior then
    some ⟨by intro σ passed; exact guards_member_passed σ prior passed g known⟩
  else if constant : constantBranch (canonical g.condition) = some true then
    some ⟨by
      intro σ passed
      have truth := constant_branch_sound σ (canonical g.condition) true constant
      rw [canonical_sound, ← one_bit_nonzero] at truth
      exact truth⟩
  else none

private def privateGuardTrap (program : EvoIR.Machine.Program)
    (function : EvoIR.Machine.Function) (block : EvoIR.Machine.Block) (code : Nat) :
    Option (PLift (∀ regs owner frame logicalInputs stack memory,
      EvoIR.Machine.step .ssa program (privateMachineState function block 0 regs owner frame logicalInputs stack memory) =
        some {(privateMachineState function block 0 regs owner frame logicalInputs stack memory) with
          control := .halted (.error (.diagnostic code))})) :=
  if foundFunction : program.functions.find? (·.name == function.name) = some function then
    if foundBlock : function.blocks.find? (·.id == block.id) = some block then
      if empty : block.insts = [] then
        if term : block.term = .brk code then
          some ⟨by
            intro regs owner frame logicalInputs stack memory
            exact EvoIR.Machine.Selection.step_trap .ssa program
              (privateMachineState function block 0 regs owner frame logicalInputs stack memory)
              ⟨function.name, block.id, 0, regs, owner, logicalInputs⟩ function block code
              rfl foundFunction foundBlock (by simp [empty]) term⟩
        else none
      else none
    else none
  else none

private theorem private_guard_branch {Γ : Var → Nat} (program : EvoIR.Machine.Program)
    (function : EvoIR.Machine.Function) (block : EvoIR.Machine.Block) (pc count : Nat)
    (before middle : PrivateBank Γ) (prior : List (Guard Γ)) (g : Guard Γ)
    (checks : List (Guard Γ)) (result : PrivateNativeGuardResult Γ)
    (pureSegment : PrivateNativePrefixDiagram program function block pc count before middle)
    (atEnd : pc+count = block.insts.length) (choice : PrivateBranchChoice)
    (branched : PrivateBranchDiagram program function block middle.words g.condition prior choice)
    (yes : {result : PrivateNativeJumpResult Γ // PrivateNativeJumpDiagram function middle
      choice.yes.target choice.yes.args result})
    (no : {result : PrivateNativeJumpResult Γ // PrivateNativeJumpDiagram function middle
      choice.no.target choice.no.args result})
    (trap : ∀ regs owner frame logicalInputs stack memory,
      EvoIR.Machine.step .ssa program (privateMachineState function no.val.block 0 regs owner frame logicalInputs stack memory) =
        some {(privateMachineState function no.val.block 0 regs owner frame logicalInputs stack memory) with
          control := .halted (.error (.diagnostic g.reason.code))})
    (tail : PrivateNativeGuardDiagram program function yes.val.block 0 yes.val.bank (prior++[g]) checks result) :
    PrivateNativeGuardDiagram program function block pc before prior (g::checks) result := by
  refine ⟨tail.1.trans (yes.property.2.2.1.trans pureSegment.1), ?_⟩
  intro σ values incoming owner frame memory regs logicalInputs stack models earlier
  obtain ⟨middleRegs, prefixPath, middleModels⟩ := pureSegment.2 σ values incoming owner frame memory
    regs logicalInputs stack models
  rw [atEnd] at prefixPath
  cases truth : (denoteTerm σ g.condition).getLsbD 0 with
  | false =>
    obtain ⟨nextRegs, jumped, nextModels⟩ := no.property.2.2.2 σ values incoming owner frame memory
      middleRegs logicalInputs block.id block.insts.length middleModels
    have step := branched σ middleRegs owner frame logicalInputs stack memory
      ⟨function.name, choice.no.target, 0, nextRegs, owner, logicalInputs⟩ middleModels.words earlier
      (by simpa only [truth, Bool.false_eq_true, ↓reduceIte] using jumped)
    have actualStep : EvoIR.Machine.step .ssa program
        (privateMachineState function block block.insts.length middleRegs owner frame logicalInputs stack memory) =
        some (privateMachineState function no.val.block 0 nextRegs owner frame logicalInputs stack memory) := by
      simpa only [privateMachineState, no.property.2.1] using step
    let last := {(privateMachineState function no.val.block 0 nextRegs owner frame logicalInputs stack memory) with
      control := .halted (.error (.diagnostic g.reason.code))}
    refine ⟨last, prefixPath.reaches.trans (.next actualStep (.next
      (trap nextRegs owner frame logicalInputs stack memory) (.refl _))), ?_⟩
    rw [private_guard_fail σ g checks truth]
  | true =>
    obtain ⟨nextRegs, jumped, nextModels⟩ := yes.property.2.2.2 σ values incoming owner frame memory
      middleRegs logicalInputs block.id block.insts.length middleModels
    have step := branched σ middleRegs owner frame logicalInputs stack memory
      ⟨function.name, choice.yes.target, 0, nextRegs, owner, logicalInputs⟩ middleModels.words earlier
      (by simpa only [truth, ↓reduceIte] using jumped)
    have actualStep : EvoIR.Machine.step .ssa program
        (privateMachineState function block block.insts.length middleRegs owner frame logicalInputs stack memory) =
        some (privateMachineState function yes.val.block 0 nextRegs owner frame logicalInputs stack memory) := by
      simpa only [privateMachineState, yes.property.2.1] using step
    obtain ⟨last, rest, finish⟩ := tail.2 σ values incoming owner frame memory nextRegs logicalInputs stack
      nextModels (private_prior_add σ prior g earlier truth)
    refine ⟨last, prefixPath.reaches.trans (.next actualStep rest), ?_⟩
    rw [private_guard_pass σ g checks truth]
    exact finish

private theorem private_guard_skip {Γ : Var → Nat} (program : EvoIR.Machine.Program)
    (function : EvoIR.Machine.Function) (block : EvoIR.Machine.Block) (pc count : Nat)
    (before middle : PrivateBank Γ) (prior : List (Guard Γ)) (g : Guard Γ)
    (checks : List (Guard Γ)) (result : PrivateNativeGuardResult Γ)
    (pureSegment : PrivateNativePrefixDiagram program function block pc count before middle)
    (known : ∀ σ, guards σ prior = .ok () → (denoteTerm σ g.condition).getLsbD 0 = true)
    (tail : PrivateNativeGuardDiagram program function block (pc+count) middle prior checks result) :
    PrivateNativeGuardDiagram program function block pc before prior (g::checks) result := by
  refine ⟨tail.1.trans pureSegment.1, ?_⟩
  intro σ values incoming owner frame memory regs logicalInputs stack models earlier
  obtain ⟨middleRegs, prefixPath, middleModels⟩ := pureSegment.2 σ values incoming owner frame memory
    regs logicalInputs stack models
  obtain ⟨last, rest, finish⟩ := tail.2 σ values incoming owner frame memory middleRegs logicalInputs stack middleModels earlier
  refine ⟨last, prefixPath.reaches.trans rest, ?_⟩
  rw [private_guard_pass σ g checks (known σ earlier)]
  exact finish

def privateNativeGuardChain {Γ : Var → Nat} (program : EvoIR.Machine.Program)
    (function : EvoIR.Machine.Function) : (checks prior : List (Guard Γ)) →
    (block : EvoIR.Machine.Block) → (pc : Nat) → (before : PrivateBank Γ) →
    Option {result : PrivateNativeGuardResult Γ //
      PrivateNativeGuardDiagram program function block pc before prior checks result}
  | [], prior, block, pc, before => do
    let count := purePrefix (block.insts.drop pc)
    let pureSegment ← privateNativePrefix program function block count pc before
    return ⟨⟨block, pc+count, pureSegment.val⟩, pureSegment.property.1, by
      intro σ values incoming owner frame memory regs logicalInputs stack models earlier
      obtain ⟨nextRegs, path, nextModels⟩ := pureSegment.property.2 σ values incoming owner frame memory
        regs logicalInputs stack models
      exact ⟨_, path.reaches, nextRegs, rfl, nextModels⟩⟩
  | g::checks, prior, block, pc, before => do
    let count := purePrefix (block.insts.drop pc)
    let pureSegment ← privateNativePrefix program function block count pc before
    let realBranch := if atEnd : pc+count = block.insts.length then do
      let choice ← privateBranchChoiceUnder program function block pureSegment.val.words g.condition prior
      let yes ← privateNativeJump function pureSegment.val choice.val.yes.target choice.val.yes.args
      let no ← privateNativeJump function pureSegment.val choice.val.no.target choice.val.no.args
      let trap ← privateGuardTrap program function no.val.block g.reason.code
      let tail ← privateNativeGuardChain program function checks (prior++[g]) yes.val.block 0 yes.val.bank
      return (⟨tail.val, private_guard_branch program function block pc count before pureSegment.val
        prior g checks tail.val pureSegment.property atEnd choice.val choice.property yes no trap.down tail.property⟩ :
        {result : PrivateNativeGuardResult Γ //
          PrivateNativeGuardDiagram program function block pc before prior (g::checks) result})
      else none
    match realBranch with
    | some result => some result
    | none => do
      let known ← privateKnownGuard prior g
      let tail ← privateNativeGuardChain program function checks prior block (pc+count) pureSegment.val
      return ⟨tail.val, private_guard_skip program function block pc count before pureSegment.val
        prior g checks tail.val pureSegment.property known.down tail.property⟩

end EvoIR.Core.Symbolic.MachineSelection
