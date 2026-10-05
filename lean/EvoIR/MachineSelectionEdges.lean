import EvoIR.MachineSelectionSourceSpan
import EvoIR.MachineEntry

/-! Symbolic simultaneous edge capture and fresh SSA parameter binding.
Every vector element is read before the destination bank is built. Register
classes and full logical arity remain fields of the actual input snapshot.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def readWords {Γ : Var → Nat} (bank : Bank Γ) : (args : List EvoIR.Machine.VReg) →
    Option {words : List (WordExpr Γ) // ∀ σ,
      args.mapM (EvoIR.Machine.read (bankDenote σ bank)) =
        .ok (words.map fun word => EvoIR.Machine.Value.int (word.denote σ))}
  | [] => some ⟨[], by intro σ; rfl⟩
  | reg :: rest => do
    let first ← wordOperand bank reg
    let tail ← readWords bank rest
    return ⟨first.val :: tail.val, by
      intro σ
      have read := word_operand_value bank reg first.val first.property σ
      rw [EvoIR.Machine.operandValue_readOperand] at read
      simp only [List.mapM_cons, read, tail.property σ,
        List.map_cons, bind, Except.bind, pure, Except.pure]⟩

def bindWordValues {Γ : Var → Nat} (bank : Bank Γ)
    (values : List (EvoIR.Machine.VReg × WordExpr Γ)) : Bank Γ :=
  values.foldl (fun bank (reg, word) => bankWrite bank reg (.word (.normalize reg.class word))) bank

private theorem bank_write_normalized {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ)
    (reg : EvoIR.Machine.VReg) (word : WordExpr Γ) :
    bankDenote σ (bankWrite bank reg (.word (.normalize reg.class word))) =
      EvoIR.Machine.write (bankDenote σ bank) reg (.int (word.denote σ)) := by
  cases cls : reg.class <;>
    simp [bankDenote, bankWrite, BankSlot.denote, WordExpr.denote,
      EvoIR.Machine.write, EvoIR.Machine.writeSlot, EvoIR.Machine.normalize,
      EvoIR.Machine.normalizeAt, EvoIR.Machine.Width.bits, cls]

theorem bind_word_values_denote {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ)
    (values : List (EvoIR.Machine.VReg × WordExpr Γ)) :
    bankDenote σ (bindWordValues bank values) =
      EvoIR.Machine.bindValues (bankDenote σ bank)
        (values.map fun (reg, word) => (reg, EvoIR.Machine.Value.int (word.denote σ))) := by
  induction values generalizing bank with
  | nil => rfl
  | cons entry rest ih =>
    rcases entry with ⟨reg, word⟩
    simp only [bindWordValues, EvoIR.Machine.bindValues, List.map_cons, List.foldl_cons]
    rw [← bank_write_normalized]
    exact ih (bankWrite bank reg (.word (.normalize reg.class word)))

def bindWords {Γ : Var → Nat} (params : List EvoIR.Machine.VReg)
    (words : List (WordExpr Γ)) : Option {bank : Bank Γ // ∀ σ,
      EvoIR.Machine.bind [] params (words.map fun word => EvoIR.Machine.Value.int (word.denote σ)) =
        .ok (bankDenote σ bank)} :=
  if arity : params.length = words.length then
    if unique : (params.map EvoIR.Machine.VReg.id).eraseDups.length = params.length then
      some ⟨bindWordValues [] (params.zip words), by
        intro σ
        rw [EvoIR.Machine.bind_exact [] params _
          (by simpa only [List.length_map] using arity) unique]
        rw [bind_word_values_denote]
        simp only [bankDenote, List.map_nil, List.zip_map_right]
        apply congrArg Except.ok
        apply congrArg (EvoIR.Machine.bindValues [])
        apply List.map_congr_left
        intro pair member
        cases pair
        rfl⟩
    else none
  else none

structure JumpResult (Γ : Var → Nat) where
  block : EvoIR.Machine.Block
  bank : Bank Γ

def JumpDiagram {Γ : Var → Nat} (f : EvoIR.Machine.Function) (bank : Bank Γ)
    (target : Nat) (args : List EvoIR.Machine.VReg) (result : JumpResult Γ) : Prop :=
  f.blocks.find? (·.id == target) = some result.block ∧ result.block.id = target ∧
  ∀ σ owner incoming block pc,
    EvoIR.Machine.jump .ssa f ⟨f.name, block, pc, bankDenote σ bank, owner, incoming⟩ target args =
      .ok ⟨f.name, target, 0, bankDenote σ result.bank, owner, incoming⟩

def jumpBank {Γ : Var → Nat} (f : EvoIR.Machine.Function) (bank : Bank Γ)
    (target : Nat) (args : List EvoIR.Machine.VReg) :
    Option {result : JumpResult Γ // JumpDiagram f bank target args result} := do
  let successor : {block // f.blocks.find? (·.id == target) = some block} ←
    match found : f.blocks.find? (·.id == target) with
    | none => none
    | some block => some ⟨block, found⟩
  let words ← readWords bank args
  let bound ← bindWords successor.val.args words.val
  return ⟨⟨successor.val, bound.val⟩, by
    have same : successor.val.id = target := by simpa using List.find?_some successor.property
    refine ⟨successor.property, same, ?_⟩
    intro σ owner incoming block pc
    simp only [EvoIR.Machine.jump, successor.property, words.property σ,
      bind, Except.bind, ↓reduceIte, bound.property σ, pure, Except.pure]⟩

end EvoIR.Core.Symbolic.MachineSelection
