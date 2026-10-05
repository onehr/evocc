import EvoIR.Asm.Reader
import EvoIR.Asm.Print
import EvoIR.Asm.Check
import EvoIR.Alloc.Reader

/-!
Standalone runner of the assembly checks. Do not import this module into the
library root: it defines `main`.

```sh
lake env lean --run EvoIR/Asm/Cli.lean --check-print AST.json ACTUAL.s
lake env lean --run EvoIR/Asm/Cli.lean --check-asm ALLOC.json AST.json
```

`--check-print` exits 0 when the printed AST is equal to the actual text
(`PRINT_MATCH`), 1 when the texts differ, 2 for malformed input.

`--check-asm` reads the allocated program of an `evo-alloc-export` file and
the AST and frame hints of an `evo-asm-ast` file. It exits 0 when
`checkAsm` accepts (`ACCEPTED_ASM`), 1 when it refuses (`REFUSED_ASM` with
the first refused function and check), 2 for malformed input. Acceptance is
the result of `checkAsm`; `checkAsm_sound` gives the refinement. The runner
is not part of the trusted statement.
-/

open EvoIR.Asm

def checkPrint (astPath textPath : String) : IO UInt32 := do
  let input ← IO.FS.readFile astPath
  let actual ← IO.FS.readFile textPath
  match Reader.readFile input with
  | .error message =>
    IO.eprintln s!"MALFORMED_ASM_AST {message}"
    return 2
  | .ok (file, _) =>
    if printMatches file actual then
      IO.println s!"PRINT_MATCH functions={file.functions.length} bytes={actual.utf8ByteSize} scope=ast-to-text"
      return 0
    else
      let line := (firstDifference file actual).getD 0
      IO.println s!"PRINT_MISMATCH first_line={line}"
      return 1

/-- The first function and sub-check that refuse, for diagnosis only. -/
def refusal (p : EvoIR.Alloc.Program) (asm : File) (hints : List FrameHint) : String :=
  if !Check.namesUnique p then "duplicate function names"
  else if p.functions.length != asm.functions.length then "function count"
  else if p.functions.length != hints.length then "hint count"
  else
    let found := (List.range p.functions.length).findSome? fun i =>
      match p.functions[i]?, hints[i]?, asm.functions[i]? with
      | some f, some hint, some g =>
        let reason :=
          if g.name != f.name then some "name"
          else if Check.expectedBody hint f != some g.body then some "template"
          else if !Check.frameOk f hint then some "frame"
          else if !Check.registersOk f then some "registers"
          else if !Check.entryOk f then some "entry"
          else if !Check.stackOk p hints i f hint then some "stack"
          else if !(Check.frameBytes hint ≤ Check.needOf p hints f.name) then some "stack"
          else if !(g.body.length < functionSpan) then some "length"
          else none
        reason.map (fun r => s!"{f.name}: {r}")
      | _, _, _ => some "missing function"
    found.getD "?"

def checkAsmFiles (allocPath astPath : String) : IO UInt32 := do
  let allocInput ← IO.FS.readFile allocPath
  let astInput ← IO.FS.readFile astPath
  match EvoIR.Alloc.Reader.readPair allocInput, Reader.readFile astInput with
  | .error message, _ =>
    IO.eprintln s!"MALFORMED_ALLOC {message}"
    return 2
  | _, .error message =>
    IO.eprintln s!"MALFORMED_ASM_AST {message}"
    return 2
  | .ok (_, alloc, _), .ok (file, hints) =>
    if Check.checkAsm alloc file hints then
      IO.println s!"ACCEPTED_ASM functions={file.functions.length} scope=allocated-to-assembly"
      return 0
    else
      IO.println s!"REFUSED_ASM {refusal alloc file hints}"
      return 1

def main (args : List String) : IO UInt32 := do
  match args with
  | ["--check-print", ast, text] => checkPrint ast text
  | ["--check-asm", alloc, ast] => checkAsmFiles alloc ast
  | _ =>
    IO.eprintln "usage: --check-print AST.json ACTUAL.s | --check-asm ALLOC.json AST.json"
    return 2
