import EvoIR.Asm.Reader
import EvoIR.Asm.Print

/-!
Standalone runner of the assembly print check. Do not import this module into
the library root: it defines `main`.

```sh
lake env lean --run EvoIR/Asm/Cli.lean --check-print AST.json ACTUAL.s
```

Exit 0: the printed AST is equal to the actual text (`PRINT_MATCH`).
Exit 1: the texts differ (`PRINT_MISMATCH`). Exit 2: malformed input.
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

def main (args : List String) : IO UInt32 := do
  match args with
  | ["--check-print", ast, text] => checkPrint ast text
  | _ =>
    IO.eprintln "usage: --check-print AST.json ACTUAL.s"
    return 2
