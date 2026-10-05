import EvoIR.MachinePrivateCutpoint

/-!
Finite, untrusted private-cell cutpoint discovery over the actual IR graph.
Widths flow through actual full edge bindings. Initialization hints use a
descending must analysis; an allocation starts uninitialized, a store marks
its named cell initialized, and lifetime end removes that hint. Neither
analysis grants admission. The independent body and successor checks must
establish every proposed cell, byte value, pointer identity and full binding.

Keys include the block name because EvoIR values are block-local. The pass
does not identify two pointers merely because their widths agree. Conflicting
widths, incomplete edge bindings and unsupported scalar widths fail closed.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

namespace PrivateProposal

abbrev Key := String × Nat
abbrev Widths := List (Key × List Nat)
abbrev Initialized := List (String × List Nat)

private def edges : IR.Terminator → List IR.Edge
  | .branch edge => [edge]
  | .conditional _ yes no => [yes, no]
  | .return _ => []

private def bindings (block : IR.Block) : List IR.Binding :=
  block.parameters ++ block.instructions.flatMap IR.Instruction.results

private def pointerParameters (block : IR.Block) : List Nat :=
  block.parameters.filterMap fun binding =>
    if binding.type == .pointer then some binding.id else none

private def addWidth (widths : Widths) (key : Key) (width : Nat) : Widths :=
  let old := (widths.lookup key).getD []
  (key, (old ++ [width]).eraseDups) :: widths.filter (fun entry => entry.1 != key)

private def addWidths (widths : Widths) (key : Key) (values : List Nat) : Widths :=
  values.foldl (fun state width => addWidth state key width) widths

private def seedInstruction (block : IR.Block) (widths : Widths)
    (instruction : IR.Instruction) : Widths :=
  match instruction.op, instruction.results, instruction.operands with
  | .allocate _ size _, pointer :: _, _ => addWidth widths (block.name, pointer.id) (8 * size)
  | .load width, _, [_, _, pointer] => addWidth widths (block.name, pointer) width
  | .store, _, [_, _, pointer, value] =>
    match (bindings block).find? (·.id == value) with
    | some ⟨_, .bits width⟩ => addWidth widths (block.name, pointer) width
    | _ => widths
  | _, _, _ => widths

private def aliases (function : IR.Function) : Option (List (Key × Key)) :=
  function.blocks.foldlM (fun previous block =>
    (edges block.terminator).foldlM (fun previous edge => do
      let target ← function.blocks.find? (·.name == edge.block)
      if edge.arguments.length != target.parameters.length then none else
      (edge.arguments.zip target.parameters).foldlM (fun previous pair => do
        if pair.2.type == .pointer then
          let actual ← (bindings block).find? (·.id == pair.1)
          if actual.type != .pointer then none else
          return previous ++ [((block.name, pair.1), (target.name, pair.2.id))]
        else return previous) previous) previous) []

private def widthPass (links : List (Key × Key)) (widths : Widths) : Widths :=
  links.foldl (fun state link =>
    let left := (state.lookup link.1).getD []
    let right := (state.lookup link.2).getD []
    addWidths (addWidths state link.1 right) link.2 left) widths

private def repeatWidths (links : List (Key × Key)) : Nat → Widths → Widths
  | 0, widths => widths
  | fuel + 1, widths => repeatWidths links fuel (widthPass links widths)

/-- The finite propagation bound limits proposal discovery, not execution or
the admitted theorem. All distinct widths are retained to reject conflicts. -/
def inferWidths (function : IR.Function) : Option Widths := do
  let links ← aliases function
  let initial := function.blocks.foldl (fun state block =>
    block.instructions.foldl (seedInstruction block) state) []
  let count := (function.blocks.flatMap bindings).length
  return repeatWidths links (count + 1) initial

private def initializedExit (block : IR.Block) (initial : List Nat) : List Nat :=
  block.instructions.foldl (fun state instruction =>
    match instruction.op, instruction.results, instruction.operands with
    | .allocate _ _ _, pointer :: _, _ => state.filter (· != pointer.id)
    | .store, _, [_, _, pointer, _] => (pointer :: state).eraseDups
    | .endLifetime, _, [_, _, pointer] => state.filter (· != pointer)
    | _, _, _ => state) initial

private def incoming (function : IR.Function) (target : IR.Block) : List (IR.Block × IR.Edge) :=
  function.blocks.flatMap fun block =>
    ((edges block.terminator).filter (·.block == target.name)).map (block, ·)

private def initializedPass (function : IR.Function) (state : Initialized) : Initialized :=
  function.blocks.map fun block =>
    let predecessors := incoming function block
    let initialized := (pointerParameters block).filter fun id =>
      !predecessors.isEmpty && predecessors.all (fun pair =>
        let after := initializedExit pair.1 ((state.lookup pair.1.name).getD [])
        ((block.parameters.zip pair.2.arguments).find? (fun binding => binding.1.id == id)).any
          (fun binding => after.contains binding.2))
    (block.name, initialized)

private def descendInitialized (function : IR.Function) : Nat → Initialized → Option Initialized
  | 0, state => if initializedPass function state == state then some state else none
  | fuel + 1, state =>
    let next := initializedPass function state
    if next == state then some state else descendInitialized function fuel next

/-- Start at the finite top and remove unsupported incoming claims. A loop
does not obtain initialization from itself unless every incoming edge also
supports it. The eventual proof checker still validates the actual bytes. -/
def inferInitialized (function : IR.Function) : Option Initialized :=
  let initial := function.blocks.map fun block => (block.name, pointerParameters block)
  let count := (function.blocks.flatMap pointerParameters).length
  descendInitialized function (count + 1) initial

private def intType : Nat → Option Source.IntType
  | 32 => some .uint
  | 64 => some .ulongLong
  | _ => none

private def point (widths : Widths) (initialized : Initialized)
    (block : IR.Block) (target : Nat) : Option PrivatePointProposal := do
  let locals ← block.parameters.reverse.filterMap (fun binding =>
    match binding.type with
    | .bits _ | .pointer => some binding
    | .memory | .world => none) |>.mapM (fun binding => do
      let dimensions : Option (Nat × Bool) := match binding.type with
        | .bits width => some (width, true)
        | .pointer => do
          let [width] ← widths.lookup (block.name, binding.id) | none
          some (width, ((initialized.lookup block.name).getD []).contains binding.id)
        | _ => none
      let (width, ready) ← dimensions
      let type ← intType width
      return ((binding.id, binding.id, type),
        (⟨binding.id, ⟨type, false⟩, ready⟩ : LocalShape)))
  let tokens := block.parameters.reverse.filterMap fun binding =>
    match binding.type with
    | .memory => some (binding.id, 0)
    | .world => some (binding.id, 1)
    | _ => none
  return {
    layout := {
      -- Only a shape bank is reused. No Source execution occurs in this path.
      site := .statement .null [], shape := locals.map Prod.snd,
      label := block.name, links := locals.map Prod.fst, tokens },
    target }

def propose (function : IR.Function) : Option (List PrivatePointProposal) := do
  let widths ← inferWidths function
  let initialized ← inferInitialized function
  (function.blocks.mapIdx fun index block => (block, index)).mapM
    (fun (block, index) => point widths initialized block index)

end PrivateProposal
end EvoIR.Core.Symbolic.MachineSelection
