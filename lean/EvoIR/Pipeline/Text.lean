import EvoIR.Pipeline.Check
import EvoIR.Core.Source
import EvoIR.Core.IRReader
import EvoIR.Core.CutpointInput
import EvoIR.MachineReader
import EvoIR.Alloc.Reader
import EvoIR.Asm.Reader
import EvoIR.Rewrite.Certificate

/-!
The pipeline check on the texts of the input files of one compilation.

`checkPipelineTexts` reads each text with the reader of its stage command,
then runs `refusal`, that is, `checkPipeline`. The result is one of three
verdicts:

* `accepted`: every stage accepts. `checkPipelineTexts_accepted` gives
  `checkPipeline input = true` for the input that the readers give.
* `rejected stage reason`: the first stage that refuses.
* `unsupported reason`: a text is missing, not readable, or outside the
  profile of its reader.

Two reader results are rejections, as in the stage commands. A certificate
that the reader does not decode is a V05 rejection. An allocation export whose
copy of the program before allocation is not the post-SSA input is a V03
rejection. These two rejections occur before the stage checks run.
-/
namespace EvoIR.Pipeline

open EvoIR.Core

/-- The texts of the input files of one compilation. -/
structure Texts where
  /-- The typed Core source export. It selects one entry. -/
  source : String
  /-- The optional untrusted V01 proposal: cutpoints or call cutpoints. -/
  proposal : Option String := none
  /-- The EvoIR before the rewrites. -/
  ir : String
  /-- For each rewrite step, in order: the EvoIR after the step and the
  `evp2` certificate. -/
  rewrites : List (String × String) := []
  ssa : String
  postSSA : String
  /-- The allocation export: the program before allocation, the allocated
  program and the hints. -/
  allocation : String
  /-- The assembly AST and the frame hints of `evo-asm-ast`. -/
  ast : String
  /-- The actual assembly text. -/
  asm : String

inductive Verdict where
  | accepted (report : String)
  | rejected (stage : String) (reason : String)
  | unsupported (reason : String)
  deriving DecidableEq, Repr

/-- A failure of the readers. -/
inductive ReadFailure where
  | rejected (stage : String) (reason : String)
  | unsupported (reason : String)

def ReadFailure.verdict : ReadFailure → Verdict
  | .rejected stage reason => .rejected stage reason
  | .unsupported reason => .unsupported reason

/-- Read the rewrite steps. `k` is the index of the first step. -/
def readSteps : Nat → List (String × String) → Except ReadFailure (List RewriteStep)
  | _, [] => pure []
  | k, (after, cert) :: rest => do
    let .ok program := IR.Reader.readProgram after
      | throw (.unsupported s!"EvoIR input after rewrite step {k}")
    let .ok certificate := Rewrite.Reader.certificate cert
      | throw (.rejected "V05" s!"certificate form of rewrite step {k} not supported")
    return ⟨program, certificate⟩ :: (← readSteps (k + 1) rest)

/-- Read the texts into the input of `checkPipeline`. -/
def readInput (texts : Texts) : Except ReadFailure Input := do
  let .ok source := Source.readProgram texts.source | throw (.unsupported "source input")
  let some f := selectEntry source
    | throw (.unsupported "source input: expected one selected entry with a source function")
  let request ← match texts.proposal with
    | none => pure .finite
    | some text =>
      match Symbolic.readCutpoints f text with
      | some (budget, layouts) => pure (.cutpoints layouts budget)
      | none =>
        match Symbolic.readCallCutpoints source text with
        | some (budget, rows) => pure (.closure (fun id => rows.lookup id) budget)
        | none => throw (.unsupported "proposal input")
  let .ok ir := IR.Reader.readProgram texts.ir | throw (.unsupported "EvoIR input")
  let rewrites ← readSteps 0 texts.rewrites
  let .ok (.ssa, ssa) := Machine.Reader.readProgram texts.ssa
    | throw (.unsupported "SSA EvoMIR input")
  let .ok (.postSSA, postSSA) := Machine.Reader.readProgram texts.postSSA
    | throw (.unsupported "post-SSA EvoMIR input")
  let .ok (copy, allocated, allocHints) := Alloc.Reader.readPair texts.allocation
    | throw (.unsupported "allocation input")
  if copy != postSSA then
    throw (.rejected "V03" "the program before allocation in the export is not the post-SSA input")
  let .ok (asm, frameHints) := Asm.Reader.readFile texts.ast
    | throw (.unsupported "assembly AST input")
  return { source, request, ir, rewrites, ssa, postSSA, allocated, allocHints, asm, frameHints,
           text := texts.asm }

/-- The pipeline check on the texts of one compilation. -/
def checkPipelineTexts (texts : Texts) : Verdict :=
  match readInput texts with
  | .error failure => failure.verdict
  | .ok input =>
    match refusal input with
    | some (stage, reason) => .rejected stage reason
    | none =>
      let entry := ((selectEntry input.source).map (·.name)).getD "?"
      .accepted s!"entry={entry} rewrites={input.rewrites.length} functions={input.asm.functions.length} scope=source-to-assembly-text"

/-- An accepted verdict comes from an accepted `checkPipeline` on the input
that the readers give. -/
theorem checkPipelineTexts_accepted {texts : Texts} {report : String}
    (accepted : checkPipelineTexts texts = .accepted report) :
    ∃ input, readInput texts = .ok input ∧ checkPipeline input = true := by
  unfold checkPipelineTexts at accepted
  split at accepted
  · rename_i failure _
    cases failure <;> cases accepted
  · rename_i input read
    refine ⟨input, read, ?_⟩
    split at accepted
    · cases accepted
    · rename_i none_
      simp [checkPipeline, none_]

end EvoIR.Pipeline
