import EvoIR.Core.SourceTargetCheck
import EvoIR.Rewrite.Check
import EvoIR.MachineLowering
import EvoIR.Alloc.Check
import EvoIR.Asm.Check
import EvoIR.Asm.Print

/-!
The single pipeline check of one compilation, from typed Core source to the
assembly text.

`checkPipeline` runs every stage check on the same values in memory:

1. `source-to-evoir`: the source validator `Source.checkWellFormed`, the entry
   selection of the stage command, and `checkSourceTargetPair`.
2. `rewrite`: `checkRewrite` for each rewrite step, in sequence. The list of
   steps can be empty.
3. `lowering`: `checkLowering` on the EvoIR after the last step.
4. `allocation`: `checkAllocation` on the post-SSA EvoMIR of `lowering`.
5. `frame-emission`: `checkAsm`, the result view check `resultViewOk` and the
   print check `printMatches`.

The pipeline also checks that adjacent stages agree on the entry:
`source-to-evoir` must give a pair for the selected source function, and
`lowering` must use the same argument tokens as `source-to-evoir`. The check
`entryViewsOk` refuses a post-SSA entry in which a 32-bit parameter has a
64-bit register. Each of these checks is a Boolean test. No check trusts a
producer annotation.

The theorem `checkPipeline_sound` in `EvoIR.Pipeline.Soundness` states what
an accepted check gives.
-/
namespace EvoIR.Pipeline

open EvoIR.Core

/-- One rewrite step: the EvoIR after the step and its certificate. The EvoIR
before the step is the EvoIR after the previous step, or the EvoIR of the
stage `source-to-evoir`. -/
structure RewriteStep where
  after : IR.Program
  certificate : Rewrite.Certificate

/-- The values of one compilation. -/
structure Input where
  /-- The typed Core source program. -/
  source : Source.Program
  /-- The `source-to-evoir` request, with the untrusted cutpoint or closure
  proposal. -/
  request : Symbolic.SourceTargetRequest
  /-- The EvoIR before the rewrites. -/
  ir : IR.Program
  rewrites : List RewriteStep
  ssa : Machine.Program
  postSSA : Machine.Program
  allocated : Alloc.Program
  allocHints : Alloc.Hints
  asm : Asm.File
  frameHints : List Asm.FrameHint
  /-- The actual assembly text. -/
  text : String

/-- The selected entry of the stage commands: the source must select one
entry, and a source function must have its symbol. -/
def selectEntry (source : Source.Program) : Option Source.Function :=
  match source.entries with
  | [entry] => source.functions.find? (·.symbol == entry)
  | _ => none

/-- The `source-to-evoir` pair is the pair of the selected source function
`f`. -/
def pairLinked {source : Source.Program} {ir : IR.Program} (f : Source.Function)
    (pair : Symbolic.CallFrontierPair source ir) : Bool :=
  pair.sourceFunction.symbol == f.symbol &&
  pair.targetFunction.name == f.name &&
  pair.sourceFunction.parameters.map (·.type.type) == f.parameters.map (·.type.type) &&
  pair.sourceFunction.returnType.type == f.returnType.type

/-- Each 32-bit parameter of the source entry `f` has a 32-bit register in
block 0 of the post-SSA entry function. The write of an argument to a 32-bit
register keeps only the low 32 bits. Thus the bits of an argument above its
width do not change the program. -/
def entryViewsOk (p : Machine.Program) (f : Source.Function) : Bool :=
  match p.functions.find? (·.name == f.name) with
  | none => false
  | some g =>
    match g.blocks.find? (·.id == 0) with
    | none => false
    | some b => (b.args.zip f.parameters).all fun (r, q) =>
      q.type.type.width == 64 || r.class == .gpr32

/-- Check each rewrite step in sequence. The result is the EvoIR after the
last step, or the index of the first step that the checker refuses. -/
def rewriteChain : Nat → IR.Program → List RewriteStep → Except Nat IR.Program
  | _, ir, [] => .ok ir
  | k, ir, step :: rest =>
    if Rewrite.checkRewrite ir step.after step.certificate then rewriteChain (k + 1) step.after rest
    else .error k

/-- Each returned location of the entry function has a view that is at least
as wide as the result type. This check decides `Asm.Proof.ResultView`. -/
def resultViewOk (p : Alloc.Program) (entry : String) (type : Source.IntType) : Bool :=
  p.functions.all fun f => f.name != entry || f.blocks.all fun b =>
    match b.term with
    | .ret locs => locs.all fun l => decide (type.width ≤ l.width.bits)
    | _ => true

/-- The first stage that refuses, as the stage identifier and a reason. The
result `none` means that every stage accepts. -/
def refusal (input : Input) : Option (String × String) :=
  match Source.checkWellFormed input.source with
  | .error message => some ("source-to-evoir", s!"source validator: {message}")
  | .ok _ =>
  match selectEntry input.source with
  | none => some ("source-to-evoir", "expected one selected entry with a source function")
  | some f =>
  match Symbolic.checkSourceTargetPair input.source input.ir input.request with
  | none =>
    some ("source-to-evoir", s!"entry={f.name} no checked source-to-EvoIR correspondence")
  | some (_, pair) =>
  if !pairLinked f pair then
    some ("source-to-evoir",
      s!"entry={f.name} the checked pair is not the pair of the selected entry")
  else
  match rewriteChain 0 input.ir input.rewrites with
  | .error k => some ("rewrite", s!"entry={f.name} rewrite step {k} not confirmed")
  | .ok last =>
  match Machine.checkLowering last input.ssa input.postSSA f.name f.parameters
      f.returnType.type with
  | none => some ("lowering", s!"entry={f.name} no checked selection or SSA transport")
  | some lowered =>
  if lowered.tokens != pair.tokens then
    some ("lowering",
      s!"entry={f.name} the argument tokens of lowering are not the tokens of source-to-evoir")
  else if !entryViewsOk input.postSSA f then
    some ("lowering",
      s!"entry={f.name} a 32-bit parameter has no 32-bit register at the post-SSA entry")
  else if !Alloc.checkAllocation input.postSSA input.allocated input.allocHints then
    some ("allocation", s!"entry={f.name} allocation not confirmed")
  else if !Asm.Check.checkAsm input.allocated input.asm input.frameHints then
    some ("frame-emission", s!"entry={f.name} frame or emission not confirmed")
  else if !resultViewOk input.allocated f.name f.returnType.type then
    some ("frame-emission",
      s!"entry={f.name} a returned view is narrower than the result type")
  else if !Asm.printMatches input.asm input.text then
    some ("frame-emission", s!"entry={f.name} the printed assembly is not the actual text")
  else none

/-- The pipeline check: every stage accepts. -/
def checkPipeline (input : Input) : Bool := (refusal input).isNone

end EvoIR.Pipeline
