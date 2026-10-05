import EvoIR.MachineSelectionArithmetic

/-!
An exact symbolic register bank for actual MIR instruction sequences. The
word expressions retain machine opcodes, widths and flag producers. They
are not an EvoIR reconstruction. Conversion to source scalar terms is a
separate, proved check. A missing register or a value/flags mismatch refuses.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

attribute [local simp] bind Except.bind pure Except.pure Functor.map Except.map
  Except.toOption Option.getD

mutual
inductive WordExpr (Γ : Var → Nat) where
  | input (word : Word Γ)
  | literal (bits : Nat)
  | binary (op : EvoIR.Machine.BinOp) (width : EvoIR.Machine.Width) (lhs rhs : WordExpr Γ)
  | wide (op : EvoIR.Machine.WideMul) (lhs rhs : WordExpr Γ)
  | extend (op : EvoIR.Machine.ExtOp) (input : WordExpr Γ)
  | bfx (signed : Bool) (width : EvoIR.Machine.Width) (input : WordExpr Γ) (lsb bits : Nat)
  | cset (flags : FlagExpr Γ) (cond : EvoIR.Machine.Cond)
  | csel (flags : FlagExpr Γ) (cond : EvoIR.Machine.Cond) (width : EvoIR.Machine.Width)
      (yes no : WordExpr Γ)
  | normalize (regClass : EvoIR.Machine.RegClass) (input : WordExpr Γ)
  deriving DecidableEq

inductive FlagExpr (Γ : Var → Nat) where
  | comparison (width : EvoIR.Machine.Width) (lhs rhs : WordExpr Γ)
  | arithmetic (op : EvoIR.Machine.FlagOp) (width : EvoIR.Machine.Width) (lhs rhs : WordExpr Γ)
  | logical (width : EvoIR.Machine.Width) (value : WordExpr Γ)
  deriving DecidableEq
end

def flagTruth (flags : EvoIR.Machine.Flags) (cond : EvoIR.Machine.Cond) : Bool :=
  (EvoIR.Machine.condition flags cond).toOption.getD false

mutual
def WordExpr.denote {Γ : Var → Nat} (σ : Subst Γ) : WordExpr Γ → BitVec 64
  | .input word => (denoteTerm σ word.2).setWidth 64
  | .literal bits => BitVec.ofNat 64 bits
  | .binary op width lhs rhs => EvoIR.Machine.evalBinary op width (lhs.denote σ) (rhs.denote σ)
  | .wide op lhs rhs => EvoIR.Machine.evalWideMultiply op (lhs.denote σ) (rhs.denote σ)
  | .extend op input => EvoIR.Machine.evalExtend op (input.denote σ)
  | .bfx signed width input lsb bits => EvoIR.Machine.evalBfx signed width (input.denote σ) lsb bits
  | .cset flags cond => BitVec.ofNat 64 (if flagTruth (flags.denote σ) cond then 1 else 0)
  | .csel flags cond width yes no => EvoIR.Machine.normalizeAt width
      (if flagTruth (flags.denote σ) cond then yes.denote σ else no.denote σ)
  | .normalize regClass input =>
      if regClass == .gpr32 then EvoIR.Machine.normalizeAt .w (input.denote σ) else input.denote σ

def FlagExpr.denote {Γ : Var → Nat} (σ : Subst Γ) : FlagExpr Γ → EvoIR.Machine.Flags
  | .comparison width lhs rhs => .integer (lhs.denote σ) (rhs.denote σ) width
  | .arithmetic op width lhs rhs =>
      (EvoIR.Machine.arithmeticFlags op width (lhs.denote σ) (rhs.denote σ)).2
  | .logical width value =>
      let bits := (value.denote σ).setWidth width.bits
      .arithmetic (decide (bits.toInt < 0)) (bits == 0) false false
end

theorem flag_truth_sound {Γ : Var → Nat} (σ : Subst Γ) (flags : FlagExpr Γ)
    (cond : EvoIR.Machine.Cond) :
    EvoIR.Machine.condition (flags.denote σ) cond = .ok (flagTruth (flags.denote σ) cond) := by
  cases flags <;> cases cond <;>
    simp [FlagExpr.denote, flagTruth, EvoIR.Machine.condition,
      EvoIR.Machine.arithmeticFlags]

inductive BankSlot (Γ : Var → Nat) where
  | word (word : WordExpr Γ)
  | flags (flags : FlagExpr Γ)
  deriving DecidableEq

def BankSlot.denote {Γ : Var → Nat} (σ : Subst Γ) : BankSlot Γ → EvoIR.Machine.Slot
  | .word expr => .value (.int (expr.denote σ))
  | .flags expr => .flags (expr.denote σ)

abbrev Bank (Γ : Var → Nat) := List (Nat × BankSlot Γ)

def bankDenote {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ) : EvoIR.Machine.RegisterFile :=
  bank.map fun (id, slot) => (id, slot.denote σ)

theorem bank_lookup {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ)
    (id : Nat) :
    (bankDenote σ bank).lookup id = (bank.lookup id).map (BankSlot.denote σ) := by
  induction bank with
  | nil => rfl
  | cons entry rest ih =>
    rcases entry with ⟨key, slot⟩
    by_cases same : id = key
    · subst key
      simp [bankDenote, List.lookup_cons]
    · have miss : (id == key) = false := by
        cases comparison : id == key
        · rfl
        · exact False.elim (same (beq_iff_eq.mp comparison))
      simpa [bankDenote, List.lookup_cons, miss] using ih

def bankRead {Γ : Var → Nat} (bank : Bank Γ) (reg : EvoIR.Machine.VReg) : Option (BankSlot Γ) :=
  bank.lookup reg.id

def wordOperand {Γ : Var → Nat} (bank : Bank Γ) (reg : EvoIR.Machine.VReg) :
    Option {word : WordExpr Γ // ∀ σ,
      EvoIR.Machine.operandBits (EvoIR.Machine.readOperand (bankDenote σ bank)) reg =
        .ok (word.denote σ)} :=
  match found : bankRead bank reg with
  | some (.word word) => some ⟨word, by
      intro σ
      simp [EvoIR.Machine.operandBits, EvoIR.Machine.operandValue,
        EvoIR.Machine.readOperand, EvoIR.Machine.readSlot,
        bank_lookup, bankRead] at found ⊢
      rw [found]
      simp [BankSlot.denote]⟩
  | _ => none

def flagOperand {Γ : Var → Nat} (bank : Bank Γ) (reg : EvoIR.Machine.VReg) :
    Option {flags : FlagExpr Γ // ∀ σ,
      EvoIR.Machine.operandFlags (EvoIR.Machine.readOperand (bankDenote σ bank)) reg =
        .ok (flags.denote σ)} :=
  match found : bankRead bank reg with
  | some (.flags flags) => some ⟨flags, by
      intro σ
      simp [EvoIR.Machine.operandFlags, EvoIR.Machine.readOperand,
        EvoIR.Machine.readSlot, bank_lookup, bankRead] at found ⊢
      rw [found]
      simp [BankSlot.denote]⟩
  | _ => none

theorem word_operand_value {Γ : Var → Nat} (bank : Bank Γ) (reg : EvoIR.Machine.VReg)
    (word : WordExpr Γ) (good : ∀ σ,
      EvoIR.Machine.operandBits (EvoIR.Machine.readOperand (bankDenote σ bank)) reg = .ok (word.denote σ))
    (σ : Subst Γ) :
    EvoIR.Machine.operandValue (EvoIR.Machine.readOperand (bankDenote σ bank)) reg =
      .ok (.int (word.denote σ)) := by
  have bits := good σ
  simp only [EvoIR.Machine.operandBits] at bits
  cases hvalue : EvoIR.Machine.operandValue (EvoIR.Machine.readOperand (bankDenote σ bank)) reg with
  | error fault => simp [hvalue] at bits
  | ok val =>
    cases val with
    | pointer id offset => simp [hvalue] at bits
    | int raw =>
      simp only [hvalue, bind, Except.bind, pure, Except.pure, Except.ok.injEq] at bits
      exact congrArg Except.ok (congrArg EvoIR.Machine.Value.int bits)

abbrev SymbolicWrites (Γ : Var → Nat) := List (EvoIR.Machine.VReg × BankSlot Γ)

def writesDenote {Γ : Var → Nat} (σ : Subst Γ) (writes : SymbolicWrites Γ) : EvoIR.Machine.Writes :=
  writes.map fun (reg, slot) => (reg, slot.denote σ)

def symbolicValueWrite {Γ : Var → Nat} (dst : EvoIR.Machine.VReg) (word : WordExpr Γ) :
    SymbolicWrites Γ := [(dst, .word (.normalize dst.class word))]

theorem value_write_denote {Γ : Var → Nat} (σ : Subst Γ)
    (dst : EvoIR.Machine.VReg) (word : WordExpr Γ) :
    writesDenote σ (symbolicValueWrite dst word) =
      EvoIR.Machine.valueWrite dst (.int (word.denote σ)) := by
  cases cls : dst.class <;>
    simp [writesDenote, symbolicValueWrite, BankSlot.denote, WordExpr.denote,
      EvoIR.Machine.valueWrite, EvoIR.Machine.normalize, EvoIR.Machine.normalizeAt,
      EvoIR.Machine.Width.bits, cls]

def bankWrite {Γ : Var → Nat} (bank : Bank Γ) (reg : EvoIR.Machine.VReg) (slot : BankSlot Γ) : Bank Γ :=
  (reg.id, slot) :: bank

def applySymbolicWrites {Γ : Var → Nat} (bank : Bank Γ) (writes : SymbolicWrites Γ) : Bank Γ :=
  writes.foldl (fun bank (reg, slot) => bankWrite bank reg slot) bank

theorem apply_writes_denote {Γ : Var → Nat} (σ : Subst Γ)
    (bank : Bank Γ) (writes : SymbolicWrites Γ) :
    bankDenote σ (applySymbolicWrites bank writes) =
      EvoIR.Machine.applyWrites (bankDenote σ bank) (writesDenote σ writes) := by
  induction writes generalizing bank with
  | nil => rfl
  | cons entry rest ih =>
    rcases entry with ⟨reg, slot⟩
    simp only [applySymbolicWrites, EvoIR.Machine.applyWrites, writesDenote,
      List.map_cons, List.foldl_cons]
    exact ih (bankWrite bank reg slot)

def OperandDiagram {Γ : Var → Nat} (bank : Bank Γ) (inst : EvoIR.Machine.Instruction)
    (writes : SymbolicWrites Γ) : Prop :=
  ∀ σ owner memory,
    EvoIR.Machine.evalInstructionOperands owner inst
      (EvoIR.Machine.readOperand (bankDenote σ bank)) memory =
        .ok (writesDenote σ writes, memory)

theorem flag_binary_value (op : EvoIR.Machine.FlagOp) (width : EvoIR.Machine.Width)
    (lhs rhs : BitVec 64) :
    (EvoIR.Machine.arithmeticFlags op width lhs rhs).1 =
      EvoIR.Machine.evalBinary (if op == .subs then .sub else .add) width lhs rhs := by
  cases op <;> rfl

/-- This proof-valued interpreter covers the actual initialized-integer,
effect-free instruction arm. Calls/private effects are handled by productive
span boundaries, not passed through this evaluator as opaque instructions. -/
def machineOperands {Γ : Var → Nat} (bank : Bank Γ) (inst : EvoIR.Machine.Instruction) :
    Option {writes : SymbolicWrites Γ // OperandDiagram bank inst writes} :=
  match inst with
  | .mov dst src => do
    let input ← wordOperand bank src
    return ⟨symbolicValueWrite dst input.val, by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands,
        word_operand_value bank src input.val input.property σ, value_write_denote]⟩
  | .movImm dst imm =>
    some ⟨symbolicValueWrite dst (.literal imm), by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands, value_write_denote, WordExpr.denote]⟩
  | .binary op dst lhs rhs width => do
    let left ← wordOperand bank lhs
    let right ← wordOperand bank rhs
    return ⟨symbolicValueWrite dst (.binary op width left.val right.val), by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands,
        word_operand_value bank lhs left.val left.property σ,
        word_operand_value bank rhs right.val right.property σ,
        value_write_denote, EvoIR.Machine.evalBinaryValue, WordExpr.denote]⟩
  | .binaryImm op dst lhs imm width => do
    let left ← wordOperand bank lhs
    return ⟨symbolicValueWrite dst (.binary op width left.val (.literal imm)), by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands,
        word_operand_value bank lhs left.val left.property σ,
        value_write_denote, EvoIR.Machine.evalBinaryValue, WordExpr.denote]⟩
  | .flagBinary op dst flags lhs rhs width => do
    let left ← wordOperand bank lhs
    let right ← wordOperand bank rhs
    let value := WordExpr.binary (if op == .subs then .sub else .add) width left.val right.val
    let set := FlagExpr.arithmetic op width left.val right.val
    return ⟨symbolicValueWrite dst value ++ [(flags, .flags set)], by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands, left.property σ, right.property σ,
        writesDenote, BankSlot.denote, value_write_denote, WordExpr.denote,
        FlagExpr.denote, flag_binary_value, value, set]
      simpa [value, WordExpr.denote, writesDenote, BankSlot.denote]
        using (value_write_denote σ dst value).symm⟩
  | .wideMultiply op dst lhs rhs => do
    let left ← wordOperand bank lhs
    let right ← wordOperand bank rhs
    return ⟨symbolicValueWrite dst (.wide op left.val right.val), by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands, left.property σ, right.property σ,
        value_write_denote, WordExpr.denote]⟩
  | .msub dst n m a width => do
    let x ← wordOperand bank n
    let y ← wordOperand bank m
    let z ← wordOperand bank a
    return ⟨symbolicValueWrite dst (.binary .sub width z.val (.binary .mul width x.val y.val)), by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands, x.property σ, y.property σ, z.property σ,
        value_write_denote, WordExpr.denote]⟩
  | .madd dst n m a width => do
    let x ← wordOperand bank n
    let y ← wordOperand bank m
    let z ← wordOperand bank a
    return ⟨symbolicValueWrite dst (.binary .add width z.val (.binary .mul width x.val y.val)), by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands, x.property σ, y.property σ, z.property σ,
        value_write_denote, WordExpr.denote]⟩
  | .extend op dst src => do
    let input ← wordOperand bank src
    return ⟨symbolicValueWrite dst (.extend op input.val), by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands, input.property σ,
        value_write_denote, WordExpr.denote]⟩
  | .bfx signed dst src lsb bits width => do
    let input ← wordOperand bank src
    return ⟨symbolicValueWrite dst (.bfx signed width input.val lsb bits), by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands, input.property σ,
        value_write_denote, WordExpr.denote]⟩
  | .cmp dst lhs rhs width => do
    let left ← wordOperand bank lhs
    let right ← wordOperand bank rhs
    return ⟨[(dst, .flags (.comparison width left.val right.val))], by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands,
        word_operand_value bank lhs left.val left.property σ,
        word_operand_value bank rhs right.val right.property σ,
        writesDenote, BankSlot.denote, FlagExpr.denote, EvoIR.Machine.compare]⟩
  | .cmpImm dst lhs imm width => do
    let left ← wordOperand bank lhs
    return ⟨[(dst, .flags (.comparison width left.val (.literal imm)))], by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands,
        word_operand_value bank lhs left.val left.property σ,
        writesDenote, BankSlot.denote, FlagExpr.denote, WordExpr.denote, EvoIR.Machine.compare]⟩
  | .tstImm dst lhs imm width => do
    let left ← wordOperand bank lhs
    let value := WordExpr.binary .and width left.val (.literal imm)
    return ⟨[(dst, .flags (.logical width value))], by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands, left.property σ,
        writesDenote, BankSlot.denote, FlagExpr.denote, WordExpr.denote, value,
        EvoIR.Machine.evalBinary, EvoIR.Machine.Selection.encode_view,
        BitVec.setWidth_ofNat_of_le, EvoIR.Machine.Selection.width_le_64]⟩
  | .cset dst flags cond => do
    let set ← flagOperand bank flags
    return ⟨symbolicValueWrite dst (.cset set.val cond), by
      intro σ owner memory
      simp [EvoIR.Machine.evalInstructionOperands, set.property σ,
        flag_truth_sound, value_write_denote, WordExpr.denote]⟩
  | .csel dst flags lhs rhs cond width => do
    let set ← flagOperand bank flags
    let left ← wordOperand bank lhs
    let right ← wordOperand bank rhs
    return ⟨symbolicValueWrite dst (.csel set.val cond width left.val right.val), by
      intro σ owner memory
      simp only [EvoIR.Machine.evalInstructionOperands, set.property σ,
        flag_truth_sound, bind, Except.bind]
      cases truth : flagTruth (set.val.denote σ) cond <;>
        simp only [truth, Bool.false_eq_true, ↓reduceIte,
          word_operand_value bank lhs left.val left.property σ,
          word_operand_value bank rhs right.val right.property σ,
          bind, Except.bind, pure, Except.pure, value_write_denote, WordExpr.denote,
          truth, ↓reduceIte]⟩
  | _ => none

def InstructionDiagram {Γ : Var → Nat} (bank : Bank Γ) (inst : EvoIR.Machine.Instruction)
    (next : Bank Γ) : Prop :=
  ∀ σ owner memory, EvoIR.Machine.evalInstruction owner inst (bankDenote σ bank) memory =
    .ok (bankDenote σ next, memory)

def machineInstruction {Γ : Var → Nat} (bank : Bank Γ) (inst : EvoIR.Machine.Instruction) :
    Option {next : Bank Γ // InstructionDiagram bank inst next} := do
  let writes ← machineOperands bank inst
  return ⟨applySymbolicWrites bank writes.val, by
    intro σ owner memory
    rw [EvoIR.Machine.evalInstruction, writes.property σ owner memory]
    simp [apply_writes_denote]⟩

def evalInstructions (owner : Nat) (memory : EvoIR.Machine.Memory)
    (regs : EvoIR.Machine.RegisterFile) : List EvoIR.Machine.Instruction →
      Except Source.Fault (EvoIR.Machine.RegisterFile × EvoIR.Machine.Memory)
  | [] => .ok (regs, memory)
  | inst :: rest => do
    let (next, memory) ← EvoIR.Machine.evalInstruction owner inst regs memory
    evalInstructions owner memory next rest

def InstructionsDiagram {Γ : Var → Nat} (bank : Bank Γ)
    (code : List EvoIR.Machine.Instruction) (next : Bank Γ) : Prop :=
  ∀ σ owner memory, evalInstructions owner memory (bankDenote σ bank) code =
    .ok (bankDenote σ next, memory)

def machineInstructions {Γ : Var → Nat} (bank : Bank Γ) :
    (code : List EvoIR.Machine.Instruction) → Option {next : Bank Γ // InstructionsDiagram bank code next}
  | [] => some ⟨bank, by intro σ owner memory; rfl⟩
  | inst :: rest => do
    let first ← machineInstruction bank inst
    let tail ← machineInstructions first.val rest
    return ⟨tail.val, by
      intro σ owner memory
      rw [evalInstructions, first.property σ owner memory]
      exact tail.property σ owner memory⟩

end EvoIR.Core.Symbolic.MachineSelection
