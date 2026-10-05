import EvoIR.MachineSelectionReturn
import EvoIR.MachineEntry
import EvoIR.Core.SymbolicEntry

/-!
Bind all actual entry arguments before a selection diagram starts. The
register list is read from block zero of the supplied MIR, not inferred from
the first few argument registers. Argument normalization follows each actual
destination class, and arity and duplicate identities are checked explicitly.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def machineInputs {Γ : Var → Nat} (σ : Subst Γ) (args : List (Word Γ)) :
    List EvoIR.Machine.Value :=
  args.map fun word => .int ((denoteTerm σ word.2).setWidth 64)

def bindInputBank {Γ : Var → Nat} (bank : Bank Γ)
    (args : List (EvoIR.Machine.VReg × Word Γ)) : Bank Γ :=
  args.foldl (fun bank (reg, word) =>
    bankWrite bank reg (.word (.normalize reg.class (.input word)))) bank

theorem bind_input_bank_denote {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ)
    (args : List (EvoIR.Machine.VReg × Word Γ)) :
    bankDenote σ (bindInputBank bank args) =
      EvoIR.Machine.bindValues (bankDenote σ bank)
        (args.map fun (reg, word) => (reg, EvoIR.Machine.Value.int ((denoteTerm σ word.2).setWidth 64))) := by
  induction args generalizing bank with
  | nil => rfl
  | cons pair rest ih =>
    rcases pair with ⟨reg, word⟩
    simp only [bindInputBank, List.foldl_cons, List.map_cons, EvoIR.Machine.bindValues]
    have head : bankDenote σ (bankWrite bank reg (.word (.normalize reg.class (.input word)))) =
        EvoIR.Machine.write (bankDenote σ bank) reg (.int ((denoteTerm σ word.2).setWidth 64)) := by
      cases cls : reg.class <;>
        simp [bankDenote, bankWrite, BankSlot.denote, WordExpr.denote,
          EvoIR.Machine.write, EvoIR.Machine.writeSlot, EvoIR.Machine.normalize,
          EvoIR.Machine.normalizeAt, EvoIR.Machine.Width.bits, cls]
    rw [← head]
    exact ih _

def machineEntry {Γ : Var → Nat} (f : EvoIR.Machine.Function) (args : List (Word Γ)) :
    Option {entry : EvoIR.Machine.Block × Bank Γ //
      ∀ σ owner, EvoIR.Machine.enter f (machineInputs σ args) owner =
        .ok ⟨f.name, entry.1.id, 0, bankDenote σ entry.2, owner, machineInputs σ args⟩} := do
  let block : {b // f.blocks.find? (·.id == 0) = some b} ←
    match found : f.blocks.find? (·.id == 0) with
    | none => none
    | some block => some ⟨block, found⟩
  if arity : block.val.args.length = args.length then
    if unique : (block.val.args.map EvoIR.Machine.VReg.id).eraseDups.length = block.val.args.length then
      let bank := bindInputBank [] (block.val.args.zip args)
      return ⟨(block.val, bank), by
        intro σ owner
        have bound := EvoIR.Machine.bind_exact [] block.val.args (machineInputs σ args)
          (by simpa only [machineInputs, List.length_map] using arity) unique
        have same := bind_input_bank_denote σ [] (block.val.args.zip args)
        have zip : (block.val.args.zip args).map (fun (reg, word) =>
            (reg, EvoIR.Machine.Value.int ((denoteTerm σ word.2).setWidth 64))) =
            block.val.args.zip (machineInputs σ args) := by
          simp [machineInputs, List.zip_map_right]
        rw [zip] at same
        have exactBind : EvoIR.Machine.bind [] block.val.args (machineInputs σ args) =
            .ok (bankDenote σ bank) := by
          rw [bound]
          exact congrArg Except.ok same.symm
        simp [EvoIR.Machine.enter, block.property, exactBind,
          Bind.bind, Except.bind, Pure.pure, Except.pure]⟩
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
