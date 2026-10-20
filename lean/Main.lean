import EvoIR.Eval

/-- Release identity, checked against Cargo's workspace version by `evo_cli`.
This is compiled into the executable; an installed checker needs no source tree. -/
private def releaseVersion : String := "0.1.0"

def main (args : List String) : IO UInt32 := do
  if args == ["--version"] then
    IO.println s!"evoir-sem {releaseVersion}"
    return 0
  EvoIR.Eval.run args
