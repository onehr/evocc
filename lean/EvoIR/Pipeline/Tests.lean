import EvoIR.Pipeline.Text
import EvoIR.Pipeline.Soundness

/-!
Tests of the pipeline check on the actual output of the compiler. Run from
`lean/` with `lake env lean EvoIR/Pipeline/Tests.lean`. A wrong result stops
the file with an error.

The fixtures in `tests/fixtures/verification/pipeline/` come from one
compilation each. The driver command is:

```sh
target/frontend/debug/evocc --emit=asm --proof-required=DIR \
  --proof-checker=lean/.lake/build/bin/evoir-sem --core-entry=ENTRY \
  -O0 -fno-builtin [REWRITE] SOURCE.c
```

* `abs`: `ENTRY=abs`, `musl-abs.c`, no rewrite.
* `fixedmul`: `ENTRY=FixedMul`, `doom-fixedmul.c`, no rewrite.
* `fixedmul-g0`: `ENTRY=FixedMul`, `doom-fixedmul.c`, and `REWRITE` is
  `-Oeq-scalar --eq-checker=target/frontend/debug/evocc-checker
  --eq-checker-sha256=SHA --eq-policy=guard-known-true`. The driver gives two
  rewrite steps. Step 0 is `tests/fixtures/verification/rewrite/fixedmul-g0`.
  Step 1 has no region.

Each directory holds the files of `DIR/inputs/`, and `output.s` of the
driver. `ast.json` is the output of
`target/frontend/debug/evo-asm-ast linux-aarch64 SHA X.evo`, where `X.evo` is
the EvoIR after the last step and `SHA` is its sha256. The text of
`evo-ir-asm linux-aarch64 SHA X.evo` is equal to `output.s`.
-/
namespace EvoIR.Pipeline.Tests

open EvoIR.Pipeline

def fixture (name file : String) : IO String :=
  IO.FS.readFile ("../tests/fixtures/verification/pipeline/" ++ name ++ "/" ++ file)

/-- The texts of one fixture. `steps` is the number of rewrite steps. -/
def load (name : String) (steps : Nat := 0) : IO Texts := do
  let rewrites ← (List.range steps).mapM fun k => do
    let stem := "rewrite-" ++ "".pushn '0' (3 - (toString k).length) ++ toString k
    return (← fixture name (stem ++ ".after.evo"), ← fixture name (stem ++ ".proof.evp"))
  return { source := ← fixture name "source.json", ir := ← fixture name "evoir.evo", rewrites,
           ssa := ← fixture name "ssa-evomir.json", postSSA := ← fixture name "post-ssa-evomir.json",
           allocation := ← fixture name "allocation.json", ast := ← fixture name "ast.json",
           asm := ← fixture name "output.s" }

/-- Replace one text. The old text must occur exactly once. -/
def edit (label text old new : String) : IO String := do
  if (text.splitOn old).length != 2 then throw (IO.userError s!"{label}: edit is not unique")
  return text.replace old new

/-- Read the texts, then run `checkPipeline` and `checkPipelineTexts`. -/
def expectAccepted (label : String) (texts : Texts) : IO Unit := do
  let .ok input := readInput texts | throw (IO.userError s!"FAIL {label}: input not read")
  if !checkPipeline input then
    throw (IO.userError s!"FAIL {label}: refused: {repr (refusal input)}")
  match checkPipelineTexts texts with
  | .accepted report => IO.println s!"PASS {label}: accepted {report}"
  | verdict => throw (IO.userError s!"FAIL {label}: {repr verdict}")

/-- The verdict must be a rejection by `stage`. -/
def expectRejected (label stage : String) (texts : Texts) : IO Unit := do
  if let .ok input := readInput texts then
    if checkPipeline input then throw (IO.userError s!"FAIL {label}: checkPipeline accepted")
  match checkPipelineTexts texts with
  | .rejected got reason =>
    if got != stage then throw (IO.userError s!"FAIL {label}: rejected by {got}, not {stage}")
    IO.println s!"PASS {label}: rejected by {got}: {reason}"
  | verdict => throw (IO.userError s!"FAIL {label}: {repr verdict}")

def run : IO Unit := do
  /- Accepted compilations. -/
  let abs ← load "abs"
  expectAccepted "abs" abs
  let fixedMul ← load "fixedmul"
  expectAccepted "FixedMul without rewrite" fixedMul
  let fixedMulG0 ← load "fixedmul-g0" (steps := 2)
  expectAccepted "FixedMul with the g0 rewrite" fixedMulG0
  /- A changed assembly text: the print check of V04 refuses. -/
  expectRejected "abs, changed assembly text" "V04"
    {abs with asm := ← edit "text" abs.asm "cmp w0, #0" "cmp w0, #1"}
  expectRejected "FixedMul g0, changed assembly text" "V04"
    {fixedMulG0 with asm := ← edit "text" fixedMulG0.asm "asr x0, x0, #16" "asr x0, x0, #15"}
  /- A changed allocated program: the return moves `x2`, not `x1`, to `x0`. -/
  let allocation ← edit "allocation" abs.allocation
    "[3,[[\"mov\",[\"gpr\",0,\"w\"],[\"gpr\",1,\"w\"]]]" "[3,[[\"mov\",[\"gpr\",0,\"w\"],[\"gpr\",2,\"w\"]]]"
  expectRejected "abs, changed allocated program" "V03" {abs with allocation}
  /- A changed copy of the program before allocation in the export. -/
  expectRejected "abs, changed copy of the post-SSA program" "V03"
    {abs with allocation := ← edit "copy" abs.allocation "\"gt\",1,[],2,[]" "\"ge\",1,[],2,[]"}
  /- A changed EvoIR before the rewrites: the result is the argument, not
  its absolute value. V01 refuses. -/
  expectRejected "abs, changed EvoIR" "V01"
    {abs with ir := ← edit "ir" abs.ir "ret %v9" "ret %v8"}
  /- A changed SSA EvoMIR: the return gives the first value of block 3. -/
  let ssa ← edit "ssa" abs.ssa "[\"ret\",[[12,\"gpr32\"]]]" "[\"ret\",[[11,\"gpr32\"]]]"
  expectRejected "abs, changed SSA EvoMIR" "V02" {abs with ssa}
  /- A changed EvoIR after rewrite step 0: V05 refuses step 0. -/
  let (after, cert) :: rest := fixedMulG0.rewrites | throw (IO.userError "FAIL: no rewrite step")
  let after ← edit "after" after "%v6 = const.bits<32> 16" "%v6 = const.bits<32> 17"
  expectRejected "FixedMul g0, changed EvoIR after step 0" "V05"
    {fixedMulG0 with rewrites := (after, cert) :: rest}
  /- The EvoIR before the rewrite, with the EvoMIR of the rewritten EvoIR and
  no rewrite step. V02 is a semantic check: the shift count guard of the
  constant 16 is always true, so V02 also accepts this pair. -/
  expectAccepted "FixedMul g0 EvoMIR, rewrite steps removed" {fixedMulG0 with rewrites := []}
  /- The same pair with a changed EvoIR: the shift count is 17, not 16. The
  source refuses this EvoIR at V01. -/
  let ir ← edit "ir" fixedMulG0.ir "%v6 = const.bits<32> 16" "%v6 = const.bits<32> 17"
  expectRejected "FixedMul, changed EvoIR" "V01" {fixedMulG0 with ir, rewrites := []}

#eval run

/-! Axiom audit. A native or unproved axiom changes the message and fails the file. -/

/-- info: 'EvoIR.Pipeline.checkPipeline_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms EvoIR.Pipeline.checkPipeline_sound

/-- info: 'EvoIR.Pipeline.checkPipelineTexts_accepted' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms EvoIR.Pipeline.checkPipelineTexts_accepted

end EvoIR.Pipeline.Tests
