import Lean.Data.Json
import EvoIR.Registry
import EvoIR.Core.Source
import EvoIR.Core.WellFormedTests
import EvoIR.Core.ExecutionTests
import EvoIR.Core.IRTests
import EvoIR.Core.CorrespondenceTests

/-!
# The executable evaluator

`lake exe evoir-sem <in.jsonl> <out.jsonl>` reads the differential vector
format of the plan's section 6 and writes each input line back with a
`"result"` field appended.

Every answer comes from the definitions the proofs are about: values from
`denoteOp`, facts from `transfer`, and the lattice answers from `Fact.meet`,
`Fact.join` and `covers`.  There is no second implementation of the semantics
here, which is the whole point of the differential evidence.

Numbers are hexadecimal strings with a `0x` prefix, lower case, no leading
zeros, and no padding to the width: `0x0`, `0x1`, `0xff`.  A result line is
the input line's bytes with `,"result":<value>` inserted before the closing
brace, so a reader can compare the two sides line by line.
-/

namespace EvoIR.Eval

open Lean (Json)

/-! ## Hexadecimal -/

def hexDigit (d : Nat) : Char :=
  if d < 10 then Char.ofNat (48 + d) else Char.ofNat (87 + d)

def toHexDigits (v : Nat) : String :=
  if h : v = 0 then have := h; ""
  else toHexDigits (v / 16) ++ String.singleton (hexDigit (v % 16))
termination_by v
decreasing_by exact Nat.div_lt_self (Nat.pos_of_ne_zero h) (by omega)

def toHex (v : Nat) : String := if v = 0 then "0x0" else "0x" ++ toHexDigits v

def ofHexDigit (c : Char) : Option Nat :=
  let n := c.toNat
  if 48 ≤ n && n ≤ 57 then some (n - 48)
  else if 97 ≤ n && n ≤ 102 then some (n - 87)
  else if 65 ≤ n && n ≤ 70 then some (n - 55)
  else none

def ofHex (s : String) : Option Nat := do
  if !s.startsWith "0x" then none else
  let body := s.toList.drop 2
  if body.isEmpty then none else
  body.foldlM (fun acc c => do let d ← ofHexDigit c; pure (acc * 16 + d)) 0

/-! ## Reading the vector fields -/

def field (j : Json) (k : String) : Option Json := (j.getObjVal? k).toOption

def natField (j : Json) (k : String) : Option Nat := do
  let v ← field j k
  (v.getNat?).toOption

def strField (j : Json) (k : String) : Option String := do
  let v ← field j k
  (v.getStr?).toOption

def hexField (j : Json) (k : String) : Option Nat := do ofHex (← strField j k)

def arrField (j : Json) (k : String) : Option (Array Json) := do
  let v ← field j k
  (v.getArr?).toOption

/-- The operator an entry names, with the widths its signature needs.  `n` is
the value width, `m` a shift amount's own width, `w` a width change's source.
-/
def parseOp (name : String) (n m w : Nat) : Option Op :=
  match name with
  | "add" => some (.bin .add n)
  | "sub" => some (.bin .sub n)
  | "mul" => some (.bin .mul n)
  | "and" => some (.bin .and n)
  | "or" => some (.bin .or n)
  | "xor" => some (.bin .xor n)
  | "icmp.eq" => some (.cmp .eq n)
  | "icmp.ne" => some (.cmp .ne n)
  | "icmp.slt" => some (.cmp .slt n)
  | "icmp.sle" => some (.cmp .sle n)
  | "icmp.ult" => some (.cmp .ult n)
  | "icmp.ule" => some (.cmp .ule n)
  | "shl" => some (.shift .shl n m)
  | "lshr" => some (.shift .lshr n m)
  | "ashr" => some (.shift .ashr n m)
  | "zext" => some (.cast .zext w n)
  | "sext" => some (.cast .sext w n)
  | "trunc" => some (.cast .trunc w n)
  | "signed_add_defined" => some (.guard .add n)
  | "signed_sub_defined" => some (.guard .sub n)
  | "signed_mul_defined" => some (.guard .mul n)
  | "signed_div_rem_no_overflow" => some (.guard .divRem n)
  | "divisor_nonzero" => some (.nonzero n)
  | "shift_count_defined.unsigned" => some (.count false n m)
  | "shift_count_defined.signed" => some (.count true n m)
  | _ => none

/-- A premise pair, refused when a value bit falls outside the mask: Section
11.3 calls that malformed and `Fact::from_premise` rejects it. -/
def mkFact (n mask value : Nat) : Option (Fact n) :=
  let mm := BitVec.ofNat n mask
  let vv := BitVec.ofNat n value
  if h : vv &&& ~~~mm = 0#n then some ⟨mm, vv, h⟩ else none

def factJson {n : Nat} (K : Fact n) : String :=
  "{\"mask\":\"" ++ toHex K.mask.toNat ++ "\",\"value\":\"" ++ toHex K.value.toNat ++ "\"}"

/-! ## The five kinds -/

/-- `value`: the operator applied to raw operands, through `denoteOp`. -/
def runValue (o : Op) (args : List Nat) : Option Nat :=
  match o, args with
  | .bin b n, [a, c] => some (denoteOp (.bin b n) (BitVec.ofNat n a, BitVec.ofNat n c)).toNat
  | .cmp b n, [a, c] => some (denoteOp (.cmp b n) (BitVec.ofNat n a, BitVec.ofNat n c)).toNat
  | .shift b n m, [a, k] =>
      some (denoteOp (.shift b n m) (BitVec.ofNat n a, capAmount n (BitVec.ofNat m k))).toNat
  | .cast b w n, [a] => some (denoteOp (.cast b w n) (BitVec.ofNat w a)).toNat
  | .guard b n, [a, c] => some (denoteOp (.guard b n) (BitVec.ofNat n a, BitVec.ofNat n c)).toNat
  | .nonzero n, [a] => some (denoteOp (.nonzero n) (BitVec.ofNat n a)).toNat
  | .count s n m, [a, k] =>
      some (denoteOp (.count s n m) (BitVec.ofNat n a, BitVec.ofNat m k)).toNat
  | _, _ => none

/-- `transfer`: the row of Section 8.10, through the same `transfer` the
soundness theorems are about. -/
def runTransfer (o : Op) (facts : List (Nat × Nat)) : Option String :=
  match o, facts with
  | .bin b n, [(m1, v1), (m2, v2)] => do
      let A ← mkFact n m1 v1
      let B ← mkFact n m2 v2
      pure (factJson (transfer (.bin b n) (A, B)))
  | .cmp b n, [(m1, v1), (m2, v2)] => do
      let A ← mkFact n m1 v1
      let B ← mkFact n m2 v2
      pure (factJson (transfer (.cmp b n) (A, B)))
  | .shift b n m, [(m1, v1), (m2, v2)] => do
      let A ← mkFact n m1 v1
      let K ← mkFact m m2 v2
      pure (factJson (transfer (.shift b n m) (A, K)))
  | .cast b w n, [(m1, v1)] => do
      let A ← mkFact w m1 v1
      pure (factJson (transfer (.cast b w n) A))
  | .guard b n, [(m1, v1), (m2, v2)] => do
      let A ← mkFact n m1 v1
      let B ← mkFact n m2 v2
      pure (factJson (transfer (.guard b n) (A, B)))
  | .nonzero n, [(m1, v1)] => do
      let A ← mkFact n m1 v1
      pure (factJson (transfer (.nonzero n) A))
  | .count s n m, [(m1, v1), (m2, v2)] => do
      let A ← mkFact n m1 v1
      let K ← mkFact m m2 v2
      pure (factJson (transfer (.count s n m) (A, K)))
  | _, _ => none

def runMeet (n : Nat) (facts : List (Nat × Nat)) : Option String :=
  match facts with
  | [(m1, v1), (m2, v2)] => do
      let A ← mkFact n m1 v1
      let B ← mkFact n m2 v2
      match A.meet B with
      | none => pure "\"conflict\""
      | some K => pure (factJson K)
  | _ => none

def runJoin (n : Nat) (facts : List (Nat × Nat)) : Option String :=
  match facts with
  | [(m1, v1), (m2, v2)] => do
      let A ← mkFact n m1 v1
      let B ← mkFact n m2 v2
      pure (factJson (A.join B))
  | _ => none

def runCovers (n : Nat) (facts : List (Nat × Nat)) : Option String :=
  match facts with
  | [(m1, v1), (m2, v2)] => do
      let M ← mkFact n m1 v1
      let X ← mkFact n m2 v2
      pure (if covers M X then "true" else "false")
  | _ => none

def readFacts (js : Array Json) : Option (List (Nat × Nat)) :=
  js.toList.mapM fun j => do pure (← hexField j "mask", ← hexField j "value")

/-- The result value for one vector, as the JSON text it contributes. -/
def evalCase (j : Json) : Option String := do
  let kind ← strField j "kind"
  let n := (natField j "n").getD 0
  let m := (natField j "m").getD 0
  let w := (natField j "w").getD 0
  match kind with
  | "value" => do
      let o ← parseOp (← strField j "op") n m w
      let args ← (← arrField j "args").toList.mapM fun a => do ofHex (← (a.getStr?).toOption)
      pure ("\"" ++ toHex (← runValue o args) ++ "\"")
  | "transfer" => do
      let o ← parseOp (← strField j "op") n m w
      runTransfer o (← readFacts (← arrField j "facts"))
  | "meet" => runMeet n (← readFacts (← arrField j "facts"))
  | "join" => runJoin n (← readFacts (← arrField j "facts"))
  | "covers" => runCovers n (← readFacts (← arrField j "facts"))
  | _ => none

/-- Append the result to the input line's own bytes.

A vector this implementation cannot answer gets `"unsupported"` rather than
stopping the run, so a runner that covers less of the space still produces a
line the comparison can treat as an abstention.  A line that is not a JSON
object is a defect in the vector file and stops the run. -/
def evalLine (line : String) : Except String String := do
  let trimmed := line.trimAscii.toString
  let j ← Json.parse trimmed
  let result := match evalCase j with
    | some r => r
    | none => "\"unsupported\""
  if trimmed.endsWith "}" then
    let head := (trimmed.dropEnd 1).toString
    let sep := if head.trimAscii.toString.endsWith "{" then "" else ","
    pure (head ++ sep ++ "\"result\":" ++ result ++ "}")
  else
    throw s!"not a JSON object: {trimmed}"

/-! ## The named vectors of the plan's section 6 -/

/-- The self test, as `(description, line, expected result text)`. -/
def namedVectors : List (String × String × String) :=
  [ ("shl(1 : bits<8>, 256 : bits<16>) = 0",
     "{\"kind\":\"value\",\"op\":\"shl\",\"n\":8,\"m\":16,\"args\":[\"0x1\",\"0x100\"]}",
     "\"0x0\"")
  , ("shl(1 : bits<8>, 0 : bits<128>) = 1",
     "{\"kind\":\"value\",\"op\":\"shl\",\"n\":8,\"m\":128,\"args\":[\"0x1\",\"0x0\"]}",
     "\"0x1\"")
  , ("shl(1 : bits<8>, 1 : bits<128>) = 2",
     "{\"kind\":\"value\",\"op\":\"shl\",\"n\":8,\"m\":128,\"args\":[\"0x1\",\"0x1\"]}",
     "\"0x2\"")
  , ("shl(1 : bits<8>, 2^64 : bits<128>) = 0",
     "{\"kind\":\"value\",\"op\":\"shl\",\"n\":8,\"m\":128,\"args\":[\"0x1\",\"0x10000000000000000\"]}",
     "\"0x0\"")
  , ("ashr(128 : bits<8>, 2^64 : bits<128>) = 255",
     "{\"kind\":\"value\",\"op\":\"ashr\",\"n\":8,\"m\":128,\"args\":[\"0x80\",\"0x10000000000000000\"]}",
     "\"0xff\"")
  ]

def selfTest : IO UInt32 := do
  let mut failed := 0
  for (name, line, expected) in namedVectors do
    match evalLine line with
    | .error e => IO.eprintln s!"FAIL {name}: {e}"; failed := failed + 1
    | .ok out =>
        let want := (line.dropEnd 1).toString ++ ",\"result\":" ++ expected ++ "}"
        if out == want then
          IO.println s!"ok   {name}"
        else
          IO.eprintln s!"FAIL {name}\n  expected {want}\n  got      {out}"
          failed := failed + 1
  if failed == 0 then
    IO.println s!"{namedVectors.length} named vectors pass"
    pure 0
  else
    pure 1

/-! ## Entry point -/

def usage : String :=
  "usage: evoir-sem <in.jsonl> <out.jsonl> | evoir-sem --selftest | evoir-sem --registry | evoir-sem --read-core <source.json> | evoir-sem --check-core-source <source.json> | evoir-sem --run-core <source.json> <entry> <step-limit> [0xARG ...] | evoir-sem --read-core-ir <target.evo> | evoir-sem --run-core-ir <target.evo> <entry> <step-limit> [0xARG ...] | evoir-sem --check-core-pilot-pair <source.json> <target.evo>"

def run (args : List String) : IO UInt32 := do
  match args with
  | ["--selftest"] => do
      let arithmetic ← selfTest
      let source ← EvoIR.Core.Source.readerSelfTest
      let wellFormed ← EvoIR.Core.Source.wellFormedSelfTest
      let execution ← EvoIR.Core.Source.executionSelfTest
      let ir ← EvoIR.Core.IR.irSelfTest
      let correspondence ← EvoIR.Core.correspondenceSelfTest
      return if arithmetic == 0 && source == 0 && wellFormed == 0 && execution == 0 && ir == 0 && correspondence == 0 then 0 else 1
  | ["--check-core-pilot-pair", sourcePath, targetPath] => do
      let .ok source := EvoIR.Core.Source.readProgram (← IO.FS.readFile sourcePath)
        | IO.eprintln "CORE_PAIR_MALFORMED: source input"; return 2
      let .ok target := EvoIR.Core.IR.Reader.readProgram (← IO.FS.readFile targetPath)
        | IO.eprintln "CORE_PAIR_MALFORMED: target input"; return 2
      if !EvoIR.Core.AbsCorrespondence.checkPilotPair source target then
        IO.eprintln "CORE_PAIR_REFUSED: no proved correspondence for this complete pair"
        return 1
      IO.println "PROVED_PILOT_PAIR entry=abs arguments=all-bits32 scope=source-to-evoir backend=UNPROVED core-v1=INCOMPLETE"
      return 0
  | ["--read-core-ir", input] => do
      match EvoIR.Core.IR.Reader.readProgram (← IO.FS.readFile input) with
      | .error message => IO.eprintln s!"CORE_IR_REFUSED: {message}"; return 2
      | .ok p =>
        IO.println s!"DECODED_NOT_VERIFIED functions={p.functions.length}"
        return 0
  | "--run-core-ir" :: input :: entry :: limit :: args => do
      let some fuel := limit.toNat? | IO.eprintln "invalid step limit"; return 2
      let .ok p := EvoIR.Core.IR.Reader.readProgram (← IO.FS.readFile input)
        | IO.eprintln "malformed core IR input"; return 2
      match EvoIR.Core.IR.checkProgram p with
      | .error message => IO.eprintln s!"CORE_IR_REFUSED: {message}"; return 2
      | .ok _ => pure ()
      let some f := p.functions.find? (·.name == entry)
        | IO.eprintln "missing IR entry"; return 2
      let mut arguments := args
      let mut values := []
      for type in f.parameters do
        match type with
        | .bits n =>
          let arg :: rest := arguments | IO.eprintln "missing integer argument"; return 2
          let some v := ofHex arg | IO.eprintln "invalid argument bits"; return 2
          if v ≥ 2^n then IO.eprintln "out-of-width argument"; return 2
          values := values ++ [EvoIR.Core.IR.Value.bits n (BitVec.ofNat n v)]
          arguments := rest
        | .memory => values := values ++ [.memory ⟨1, []⟩]
        | .world => values := values ++ [.world]
        | .pointer => IO.eprintln "pointer entry outside core"; return 2
      if !arguments.isEmpty then IO.eprintln "extra argument"; return 2
      let .ok s := EvoIR.Core.IR.initial p entry values
        | IO.eprintln "IR entry refused"; return 2
      match (EvoIR.Core.IR.runSteps p fuel s).control with
      | .halted (.ok results) =>
        let bits := results.filterMap fun v => match v with
          | .bits _ b => some (toHex b.toNat) | _ => none
        IO.println s!"EXECUTED_NOT_VERIFIED returns={bits}"
        return 0
      | .halted (.error (.diagnostic code)) =>
        IO.println s!"EXECUTED_NOT_VERIFIED diagnostic=E{code}"
        return 1
      | .halted (.error (.malformed message)) =>
        IO.eprintln s!"CORE_IR_MACHINE_MALFORMED: {message}"; return 2
      | _ => IO.println "STEP_LIMIT_NOT_VERIFICATION"; return 3
  | "--run-core" :: input :: entry :: limit :: args => do
      let some fuel := limit.toNat? | IO.eprintln "invalid step limit"; return 2
      let .ok program := EvoIR.Core.Source.readProgram (← IO.FS.readFile input)
        | IO.eprintln "malformed core source input"; return 2
      match EvoIR.Core.Source.checkWellFormed program with
      | .error message => IO.eprintln s!"CORE_SOURCE_REFUSED: {message}"; return 1
      | .ok _ => pure ()
      let some f := program.functions.find? (·.name == entry)
        | IO.eprintln "missing entry function"; return 2
      if args.length != f.parameters.length then IO.eprintln "argument count mismatch"; return 2
      let values := (args.zip f.parameters).mapM fun (arg, param) => do
        let n ← ofHex arg
        if n < 2^param.type.type.width then
          some (EvoIR.Core.Source.Value.ofNat param.type.type n)
        else none
      let some values := values | IO.eprintln "invalid or out-of-width argument bits"; return 2
      let .ok start := EvoIR.Core.Source.initial program f.symbol values
        | IO.eprintln "entry admission failed"; return 2
      match (EvoIR.Core.Source.runSteps program fuel start).control with
      | .halted (.ok v) =>
        IO.println s!"EXECUTED_NOT_VERIFIED return={toHex v.bits.toNat} type={reprStr v.type}"
        return 0
      | .halted (.error (.diagnostic code)) =>
        IO.println s!"EXECUTED_NOT_VERIFIED diagnostic=E{code}"
        return 1
      | .halted (.error (.malformed message)) =>
        IO.eprintln s!"CORE_MACHINE_MALFORMED: {message}"
        return 2
      | _ => IO.println "STEP_LIMIT_NOT_VERIFICATION"; return 3
  | ["--check-core-source", input] => do
      match EvoIR.Core.Source.readProgram (← IO.FS.readFile input) with
      | .error message => IO.eprintln message; return 2
      | .ok program =>
        match EvoIR.Core.Source.checkWellFormed program with
        | .error message => IO.eprintln s!"CORE_SOURCE_REFUSED: {message}"; return 1
        | .ok _ =>
          IO.println s!"WELL_FORMED_NOT_COMPILATION_VERIFIED entries={program.entries.length} functions={program.functions.length}"
          return 0
  | ["--read-core", input] => do
      match EvoIR.Core.Source.readProgram (← IO.FS.readFile input) with
      | .error message => IO.eprintln message; return 2
      | .ok program =>
        IO.println s!"DECODED_NOT_VERIFIED entries={program.entries.length} functions={program.functions.length}"
        IO.println (reprStr program)
        return 0
  | ["--registry"] => do
      for r in registry do IO.println r.render
      pure 0
  | [inPath, outPath] => do
      let text ← IO.FS.readFile inPath
      let lines := text.splitOn "\n" |>.filter (fun l => !l.trimAscii.toString.isEmpty)
      IO.FS.withFile outPath IO.FS.Mode.write fun handle => do
        for line in lines do
          match evalLine line with
          | .error e => IO.eprintln e; return 2
          | .ok o => handle.putStr (o ++ "\n")
        IO.println s!"{lines.length} vectors evaluated"
        pure 0
  | _ => do IO.eprintln usage; pure 64

end EvoIR.Eval
