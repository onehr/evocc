import EvoIR.IRPoisonBindings

/-! A small, independently checked EvoIR normalization leg. It can only change
an operand-free singleton `uninitialized.bits` whose result is never consumed.
Every ordinary operand, condition and return is demanded. Edge closure checks
the actual complete argument vector against the actual successor parameters.
The supplied second program is checked for exact syntax, not identified by a
digest or a producer claim. -/
namespace EvoIR.Core.IR.Poison

abbrev DemandPlan := List (String × List Nat)

def neededFor (plan : DemandPlan) (name : String) : List Nat :=
  (plan.lookup name).getD []

def testUses : Terminator → List Nat
  | .branch _ => []
  | .conditional id _ _ => [id]
  | .return ids => ids

def edges : Terminator → List Edge
  | .branch edge => [edge]
  | .conditional _ yes no => [yes, no]
  | .return _ => []

def edgeDemand (needed : List Nat) : List Binding → List Nat → Bool
  | [], [] => true
  | parameter :: parameters, id :: ids =>
    (!needed.contains parameter.id || needed.contains id) && edgeDemand needed parameters ids
  | _, _ => false

def edgeValid (needed : List Nat) (f : Function) (edge : Edge) : Bool :=
  match f.blocks.find? (·.name == edge.block) with
  | none => false
  | some block => edgeDemand needed block.parameters edge.arguments

def blockValid (needed : List Nat) (f : Function) (block : Block) : Bool :=
  block.instructions.all (fun inst => inst.operands.all needed.contains) &&
    (testUses block.terminator).all needed.contains &&
    (edges block.terminator).all (edgeValid needed f)

def functionValid (needed : List Nat) (f : Function) : Bool :=
  f.blocks.all (blockValid needed f)

def programValid (plan : DemandPlan) (source : Program) : Bool :=
  source.functions.all (fun f => functionValid (neededFor plan f.name) f)

def rewriteInstruction (needed : List Nat) (inst : Instruction) : Instruction :=
  match inst.op, inst.results, inst.operands with
  | .uninitialized (.bits width), [result], [] =>
    if 0 < width && !needed.contains result.id then
      {inst with op := .constant width 0}
    else inst
  | _, _, _ => inst

def rewriteBlock (needed : List Nat) (block : Block) : Block :=
  {block with instructions := block.instructions.map (rewriteInstruction needed)}

def rewriteFunction (plan : DemandPlan) (f : Function) : Function :=
  {f with blocks := f.blocks.map (rewriteBlock (neededFor plan f.name))}

def rewriteProgram (plan : DemandPlan) (source : Program) : Program :=
  ⟨source.functions.map (rewriteFunction plan)⟩

def checkPair (source target : Program) (plan : DemandPlan) : Bool :=
  programValid plan source && decide (target = rewriteProgram plan source)

theorem checkPair_data (source target : Program) (plan : DemandPlan)
    (accepted : checkPair source target plan = true) :
    programValid plan source = true ∧ target = rewriteProgram plan source := by
  simpa only [checkPair, Bool.and_eq_true, decide_eq_true_eq] using accepted

theorem functionValid_block (needed : List Nat) (f : Function)
    (valid : functionValid needed f = true) (block : Block) (member : block ∈ f.blocks) :
    blockValid needed f block = true := (List.all_eq_true.mp valid) block member

theorem blockValid_data (needed : List Nat) (f : Function) (block : Block)
    (valid : blockValid needed f block = true) :
    (∀ inst ∈ block.instructions, ∀ id ∈ inst.operands, id ∈ needed) ∧
    (∀ id ∈ testUses block.terminator, id ∈ needed) ∧
    (∀ edge ∈ edges block.terminator, edgeValid needed f edge = true) := by
  simp only [blockValid, Bool.and_eq_true, List.all_eq_true, List.contains_iff_mem] at valid
  exact ⟨valid.1.1, valid.1.2, valid.2⟩

theorem programValid_function (plan : DemandPlan) (source : Program)
    (valid : programValid plan source = true) (f : Function) (member : f ∈ source.functions) :
    functionValid (neededFor plan f.name) f = true := (List.all_eq_true.mp valid) f member

theorem find_map {α : Type} (values : List α) (transform : α → α) (predicate : α → Bool)
    (unchanged : ∀ value, predicate (transform value) = predicate value) :
    (values.map transform).find? predicate = (values.find? predicate).map transform := by
  induction values with
  | nil => rfl
  | cons value rest ih => simp only [List.map_cons, List.find?_cons, unchanged, ih]; split <;> rfl

theorem find_function (plan : DemandPlan) (source : Program) (name : String) :
    (rewriteProgram plan source).functions.find? (·.name == name) =
      (source.functions.find? (·.name == name)).map (rewriteFunction plan) :=
  find_map source.functions (rewriteFunction plan) (·.name == name) (fun _ => rfl)

theorem find_block (plan : DemandPlan) (f : Function) (name : String) :
    (rewriteFunction plan f).blocks.find? (·.name == name) =
      (f.blocks.find? (·.name == name)).map (rewriteBlock (neededFor plan f.name)) :=
  find_map f.blocks (rewriteBlock (neededFor plan f.name)) (·.name == name) (fun _ => rfl)

theorem rewrite_block_parameters (needed : List Nat) (block : Block) :
    (rewriteBlock needed block).parameters = block.parameters := rfl

theorem rewrite_block_terminator (needed : List Nat) (block : Block) :
    (rewriteBlock needed block).terminator = block.terminator := rfl

end EvoIR.Core.IR.Poison
