import EvoIR.Alloc.Reader

/-!
Command-line run of the allocation checker on exports of `evo-alloc-export`:

```sh
cd lean && lake env lean --run EvoIR/Alloc/Run.lean EXPORT.json...
```

For each file, the runner prints `ACCEPTED_ALLOCATION` or `REFUSED_ALLOCATION`
with the first refused block. Acceptance is the result of the proved
`checkAllocation`; `checkAllocation_sound` gives the refinement. The runner
itself is not part of the trusted statement.
-/
namespace EvoIR.Alloc.Run

/-- The first function and block that the checker refuses, for diagnosis. -/
def firstRefusal (pre : Machine.Program) (post : Program) (hints : Hints) : Option String :=
  pre.functions.findSome? fun f =>
    if checkFunction pre post hints f then none
    else
      match post.functions.find? (·.name == f.name) with
      | none => some s!"{f.name}: no allocated function"
      | some g =>
        let hint := hintOf hints f.name
        match f.blocks.find? (fun b => !checkBlock pre post hint f g b) with
        | some b => some s!"{f.name}: block {b.id}"
        | none => some s!"{f.name}: entry state"

def checkFile (path : String) : IO Bool := do
  let input ← IO.FS.readFile path
  match Reader.readPair input with
  | .error message =>
    IO.println s!"MALFORMED {path}: {message}"
    return false
  | .ok (pre, post, hints) =>
    if checkAllocation pre post hints then
      IO.println s!"ACCEPTED_ALLOCATION {path} functions={pre.functions.length}"
      return true
    else
      IO.println s!"REFUSED_ALLOCATION {path} at {(firstRefusal pre post hints).getD "?"}"
      return false

end EvoIR.Alloc.Run

def main (args : List String) : IO UInt32 := do
  let mut failed := 0
  for path in args do
    if !(← EvoIR.Alloc.Run.checkFile path) then failed := failed + 1
  IO.println s!"SUMMARY files={args.length} refused_or_malformed={failed}"
  return if failed == 0 then 0 else 1
