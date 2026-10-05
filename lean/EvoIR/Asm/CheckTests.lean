import EvoIR.Asm.Check
import EvoIR.Asm.Reader
import EvoIR.Asm.Print
import EvoIR.Alloc.Reader

/-!
Acceptance and refusal tests of `checkAsm` on frozen exports of the actual
compiler (`tests/fixtures/verification/asm`). Each refusal test changes one
part of an accepted input. When a test changes the frame hint, it also
changes the assembly lines in the same way, so that only the frame check can
refuse. Run with `lake env lean EvoIR/Asm/CheckTests.lean` in `lean/`.
-/
namespace EvoIR.Asm.CheckTests

open EvoIR.Asm

def fixture (name : String) : String := "../tests/fixtures/verification/asm/" ++ name

def load (name : String) : IO (Alloc.Program × File × List FrameHint × String) := do
  let alloc ← IO.FS.readFile (fixture (name ++ ".alloc.json"))
  let ast ← IO.FS.readFile (fixture (name ++ ".ast.json"))
  let text ← IO.FS.readFile (fixture (name ++ ".s"))
  let some (_, p, _) := (Alloc.Reader.readPair alloc).toOption | throw (IO.userError "alloc")
  let some (file, hints) := (Reader.readFile ast).toOption | throw (IO.userError "ast")
  return (p, file, hints, text)

/-- Replace each line `old` of function `name` by the lines `new`. -/
def editLines (file : File) (name : String) (old : Line) (new : List Line) : File :=
  {file with functions := file.functions.map fun g =>
    if g.name = name then {g with body := g.body.flatMap (fun l => if l = old then new else [l])}
    else g}

def editHint (file : File) (hints : List FrameHint) (name : String) (f : FrameHint → FrameHint) :
    List FrameHint :=
  (file.functions.zip hints).map fun (g, h) => if g.name = name then f h else h

def expect (label : String) (actual expected : Bool) : IO Unit := do
  if actual != expected then throw (IO.userError s!"FAIL {label}: got {actual}")
  IO.println s!"PASS {label}"

set_option linter.constructorNameAsVariable false

/-- The template check and the frame check of one function, separately. -/
def parts (p : Alloc.Program) (file : File) (hints : List FrameHint) (name : String) : Bool × Bool :=
  match (p.functions.zip (hints.zip file.functions)).find? (·.1.name = name) with
  | some (f, h, g) => (Check.expectedBody h f == some g.body, Check.frameOk f h)
  | none => (false, false)

def str (w : Machine.Width) (r : Nat) (base : Base) (off : Int) : Line := .inst (.str w (.r r) base off)
def ldr (w : Machine.Width) (r : Nat) (base : Base) (off : Int) : Line := .inst (.ldr w (.r r) base off)

def run : IO Unit := do
  -- Every frozen pilot module prints its actual text and is accepted.
  for name in ["doom-fixedmul", "musl-abs", "control", "pressure"] do
    let (p, file, hints, text) ← load name
    expect s!"{name} prints the actual text" (printMatches file text) true
    expect s!"{name} accepted" (Check.checkAsm p file hints) true
  let (p, file, hints, text) ← load "pressure"
  expect "pressure prints the actual text" (printMatches file text) true
  expect "pressure accepted" (Check.checkAsm p file hints) true
  -- Two frame areas overlap: the save slot of x19 moves onto spill slot 6.
  let overlapFile := editLines (editLines file "wide" (str .x 19 .sp 56) [str .x 19 .sp 48])
    "wide" (ldr .x 19 .sp 56) [ldr .x 19 .sp 48]
  let overlapHints := editHint file hints "wide" fun h =>
    {h with saved := h.saved.map (fun (r, o) => if r = 19 then (r, 48) else (r, o))}
  expect "overlapping save and spill areas refused" (Check.checkAsm p overlapFile overlapHints) false
  expect "overlap: template matches, frame check refuses"
    (parts p overlapFile overlapHints "wide" == (true, false)) true
  -- Two frame areas overlap: spill slot 0 moves into the outgoing area.
  let outgoingFile := editLines (editLines file "pressure" (str .x 28 .sp 192) [str .x 28 .sp 184])
    "pressure" (ldr .x 8 .sp 192) [ldr .x 8 .sp 184]
  let outgoingHints := editHint file hints "pressure" fun h =>
    {h with spills := h.spills.map (fun (k, o) => if k = 0 then (k, 184) else (k, o))}
  expect "spill slot in the outgoing area refused" (Check.checkAsm p outgoingFile outgoingHints) false
  expect "outgoing overlap: template matches, frame check refuses"
    (parts p outgoingFile outgoingHints "pressure" == (true, false)) true
  -- A callee-saved register is not restored.
  expect "missing restore of x19 refused"
    (Check.checkAsm p (editLines file "wide" (ldr .x 19 .sp 56) []) hints) false
  -- A callee-saved register is neither saved nor restored, consistently.
  let unsavedFile := editLines (editLines file "wide" (str .x 19 .sp 56) []) "wide" (ldr .x 19 .sp 56) []
  let unsavedHints := editHint file hints "wide" fun h => {h with saved := h.saved.filter (·.1 != 19)}
  expect "unsaved written callee-saved register refused"
    (Check.checkAsm p unsavedFile unsavedHints) false
  expect "unsaved register: template matches, frame check refuses"
    (parts p unsavedFile unsavedHints "wide" == (true, false)) true
  -- A wrong stack argument offset: outgoing slot 0 written at offset 8.
  expect "wrong outgoing argument offset refused"
    (Check.checkAsm p (editLines file "pressure" (str .x 9 .sp 0) [str .x 9 .sp 8]) hints) false
  -- A wrong stack argument offset: incoming slot 0 read at offset 24.
  expect "wrong incoming argument offset refused"
    (Check.checkAsm p (editLines file "wide" (ldr .x 8 (.r 29) 16) [ldr .x 8 (.r 29) 24]) hints) false
  -- A store with a smaller width than the value: a 64-bit spill as `str w`.
  expect "narrow spill store refused"
    (Check.checkAsm p (editLines file "pressure" (str .x 28 .sp 248) [str .w 28 .sp 248]) hints) false
  -- A template with a changed instruction.
  let addLine : Line := .inst (.alu .add .x (.r 0) (.r 0) (.r 1))
  let subLine : Line := .inst (.alu .sub .x (.r 0) (.r 0) (.r 1))
  expect "changed template instruction refused"
    (Check.checkAsm p (editLines file "wide" addLine [subLine]) hints) false
  -- An exchanged pair of registers in the saves, and in the restores.
  let swapSave := editLines (editLines file "wide" (str .x 19 .sp 56) [str .x 20 .sp 56])
    "wide" (str .x 20 .sp 64) [str .x 19 .sp 64]
  expect "exchanged save registers refused" (Check.checkAsm p swapSave hints) false
  let swapRestore := editLines (editLines file "wide" (ldr .x 19 .sp 56) [ldr .x 20 .sp 56])
    "wide" (ldr .x 20 .sp 64) [ldr .x 19 .sp 64]
  expect "exchanged restore registers refused" (Check.checkAsm p swapRestore hints) false
  -- A frame hint without the save slot of a used register, with the same text.
  expect "hint with a missing save slot refused"
    (Check.checkAsm p file (editHint file hints "wide" fun h => {h with saved := h.saved.drop 1})) false
  let (q, privateFile, privateHints, privateText) ← load "machine-private"
  expect "machine-private prints the actual text" (printMatches privateFile privateText) true
  -- Private frame objects are outside the proved forms: the checker refuses them.
  expect "machine-private refused (private frame objects)"
    (Check.checkAsm q privateFile privateHints) false

#eval run

end EvoIR.Asm.CheckTests
