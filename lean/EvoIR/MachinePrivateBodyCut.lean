import EvoIR.MachinePrivateBodySpan

/-!
Private prefixes stop before their first observable arithmetic guard. The
prefix checker returns an unconditional actual path, while the stopped row
retains both its first-fault equation and its successful state transfer. A
caller cannot turn this local result into admission by assuming guards pass.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

structure PrivateBodyCut {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) where
  result : PrivateBodySpanResult Γ
  noChecks : result.checks = []
  sound : PrivateBodySpanDiagram source target sf sb spc tf tb tpc before result

def privateBodyCut {Γ : Var → Nat} (source : IR.Program)
    (target : EvoIR.Machine.Program) (sf : IR.Function) (sb : IR.Block) (spc : Nat)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (before : PrivateBank Γ) : Option (PrivateBodyCut source target sf sb spc tf tb tpc before) := do
  let span ← privateBodySpanStopping true source target sf sb spc tf tb tpc before
  if noChecks : span.val.checks = [] then
    return ⟨span.val, noChecks, span.property⟩
  else none

structure PrivateSourceGuardRow {Γ : Var → Nat} (before : PrivateBank Γ)
    (inst : IR.Instruction) where
  row : CheckedPrivateRow before (some inst) none
  fault : ∀ σ values owner code,
    guards σ row.checks = .error code →
    concreteInstruction owner (before.source.scope.denote σ (privateToken values)) inst =
      .error code

/-- Both equations refer to the same actual instruction and computed trace.
The source scalar checker, not a producer condition, selects the guards. -/
def privateSourceGuardRow {Γ : Var → Nat} (before : PrivateBank Γ)
    (inst : IR.Instruction) : Option (PrivateSourceGuardRow before inst) := do
  let computed : {result // instruction before.source.scope inst = some result} ←
    match found : instruction before.source.scope inst with
    | none => none
    | some result => some ⟨result, found⟩
  let row ← privateSourceScalar before inst
  if sameChecks : row.checks = computed.val.1 then
    return ⟨row, by
      intro σ values owner code failed
      have equation := instruction_sound σ (privateToken values) owner before.source.scope
        computed.val.2 inst computed.val.1 computed.property
      rw [← sameChecks, failed] at equation
      exact equation⟩
  else none

end EvoIR.Core.Symbolic.MachineSelection
