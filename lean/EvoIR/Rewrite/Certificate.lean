/-!
The certificate data that the rewrite checker replays, and a reader for the
`evp2` text of Section 11.3. The reader only decodes. It gives no authority:
every field is checked again against the program before the rewrite.

The reader keeps the region blocks and their events. It reads and ignores the
budget line, the epoch markers and the input and output digests. The checker
compares the complete program after the rewrite instead of a digest.
-/

namespace EvoIR.Rewrite

inductive NodeSource where
  | before
  | derived
  deriving DecidableEq, Repr

structure NodeRec where
  node : Nat
  owner : Nat
  source : NodeSource
  op : String
  sort : Nat
  value : Option Nat
  children : List Nat
  deriving DecidableEq, Repr

inductive ProofRef where
  | cls (c : Nat)
  | node (n : Nat)
  deriving DecidableEq, Repr

structure Premise where
  cls : Nat
  digits : Nat
  mask : Nat
  value : Nat
  deriving DecidableEq, Repr

structure RewriteRec where
  rule : String
  lhs : Nat
  rhs : Nat
  result : Nat
  lhsRef : ProofRef
  rhsRef : ProofRef
  bindings : List (String × Nat)
  premises : List Premise
  deriving DecidableEq, Repr

structure CongruenceRec where
  lhs : Nat
  rhs : Nat
  result : Nat
  left : Nat
  right : Nat
  deriving DecidableEq, Repr

inductive Event where
  | node (n : NodeRec)
  | rewrite (r : RewriteRec)
  | congruence (c : CongruenceRec)
  deriving DecidableEq, Repr

inductive Choice where
  | seed (i : Nat)
  | node (n : Nat)
  deriving DecidableEq, Repr

structure SeedRec where
  value : Nat
  width : Nat
  cls : Nat
  deriving DecidableEq, Repr

structure Region where
  function : String
  block : String
  index : Nat
  root : Nat
  witnesses : List Nat
  seeds : List SeedRec
  events : List Event
  extracts : List (Nat × Choice)
  rootClass : Nat
  deriving DecidableEq, Repr

structure Certificate where
  registry : String
  digest : String
  regions : List Region
  deriving DecidableEq, Repr

namespace Reader

abbrev R := Except String

def nat (s : String) : R Nat :=
  match s.toNat? with
  | some n => if toString n == s then .ok n else .error s!"noncanonical integer {s}"
  | none => .error s!"not an integer: {s}"

def hexDigit (c : Char) : Option Nat :=
  if '0' ≤ c ∧ c ≤ '9' then some (c.toNat - '0'.toNat)
  else if 'a' ≤ c ∧ c ≤ 'f' then some (c.toNat - 'a'.toNat + 10)
  else none

def hex (s : String) : R Nat :=
  if s.isEmpty then .error "empty hexadecimal field" else
  s.toList.foldlM (fun acc c => match hexDigit c with
    | some d => .ok (acc * 16 + d)
    | none => .error "bad hexadecimal digit") 0

def prefixed (mark s : String) : R String :=
  if s.startsWith mark then .ok (s.drop mark.length).toString else .error s!"expected {mark} in {s}"

def cls (s : String) : R Nat := do nat (← prefixed "c" s)
def nodeId (s : String) : R Nat := do nat (← prefixed "n" s)
def valueName (s : String) : R Nat := do nat (← prefixed "%v" s)

def sort (s : String) : R Nat := do
  let body ← prefixed "bits<" s
  if !body.endsWith ">" then throw "bad sort"
  nat (body.dropEnd 1).toString

def list (item : String → R α) (s : String) : R (List α) :=
  if s == "-" then .ok [] else (s.splitOn ",").mapM item

def regionId (s : String) : R (String × String × Nat) := do
  let [f, b, i] := s.splitOn "/" | throw "bad region identity"
  return (f, b, ← nat i)

def proofRef (s : String) : R ProofRef :=
  if s.startsWith "class:" then do return .cls (← cls (s.drop 6).toString)
  else if s.startsWith "node:" then do return .node (← nodeId (s.drop 5).toString)
  else .error "bad proof reference"

def binding (s : String) : R (String × Nat) := do
  let [v, c] := s.splitOn ":" | throw "bad binding"
  return ((← prefixed "?" v), ← cls c)

def premise (s : String) : R Premise := do
  let [c, m, v] := s.splitOn ":" | throw "bad premise"
  if m.length != v.length then throw "premise digit count differs"
  return ⟨← cls c, m.length, ← hex m, ← hex v⟩

def nodeLine (fields : List String) : R NodeRec := do
  let [n, o, src, op, srt, eff, attrs, ch] := fields | throw "bad node record"
  let source ← match ← prefixed "source=" src with
    | "before" => pure NodeSource.before
    | "derived" => pure .derived
    | _ => throw "bad node source"
  if eff != "effect=pure" then throw "node effect is not pure"
  let attrs ← prefixed "attrs=" attrs
  let value ← if attrs == "-" then pure none else some <$> nat (← prefixed "value=" attrs)
  return ⟨← nodeId n, ← cls o, source, ← prefixed "op=" op, ← sort (← prefixed "sort=" srt),
    value, ← list cls (← prefixed "children=" ch)⟩

def rewriteLine : List String → R RewriteRec
  | [rule, l, r, res, lr, rr, "bind", bs] => do
    return ⟨rule, ← cls l, ← cls r, ← cls res, ← proofRef lr, ← proofRef rr, ← list binding bs, []⟩
  | [rule, l, r, res, lr, rr, "bind", bs, "premises", ps] => do
    return ⟨rule, ← cls l, ← cls r, ← cls res, ← proofRef lr, ← proofRef rr, ← list binding bs,
      ← list premise ps⟩
  | _ => .error "bad rewrite record"

def choice (s : String) : R Choice :=
  if s.startsWith "seed:" then do return .seed (← nat (s.drop 5).toString)
  else if s.startsWith "node:" then do return .node (← nodeId (s.drop 5).toString)
  else .error "bad extraction choice"

/-- Read the lines of one region block, up to and with `endregion`. -/
def regionBlock (header : List String) (lines : List (List String)) :
    R (Region × List (List String)) := do
  let ["region", rid, "root", root, "witnesses", w, "seeds", s] := header
    | throw "bad region record"
  let (f, b, i) ← regionId rid
  let w ← nat w
  let s ← nat s
  let mut rest := lines
  let mut witnesses := []
  for k in List.range w do
    let ["witness", kk, wid] :: tail := rest | throw "missing witness record"
    if (← nat kk) != k then throw "witness position"
    let (f', b', j) ← regionId wid
    if f' != f || b' != b then throw "witness outside the region block"
    witnesses := witnesses ++ [j]
    rest := tail
  let mut seeds := []
  for k in List.range s do
    let ["seed", kk, v, t, "input", c] :: tail := rest | throw "missing input seed record"
    if (← nat kk) != k then throw "seed position"
    seeds := seeds ++ [⟨← valueName v, ← sort t, ← cls c⟩]
    rest := tail
  let mut events := []
  let mut extracts := []
  let mut count := 0
  let mut fuel := rest.length + 1
  while fuel > 0 do
    fuel := fuel - 1
    match rest with
    | ("node" :: e :: fields) :: tail =>
      if (← nat e) != count then throw "event number"
      events := events ++ [.node (← nodeLine fields)]
      count := count + 1
      rest := tail
    | ("rewrite" :: e :: fields) :: tail =>
      if (← nat e) != count then throw "event number"
      events := events ++ [.rewrite (← rewriteLine fields)]
      count := count + 1
      rest := tail
    | ["congruence", e, l, r, res, a, b] :: tail =>
      if (← nat e) != count then throw "event number"
      events := events ++ [.congruence ⟨← cls l, ← cls r, ← cls res, ← nodeId a, ← nodeId b⟩]
      count := count + 1
      rest := tail
    | ["epoch", _] :: tail => rest := tail
    | ["extract", k, c, ch] :: tail =>
      if (← nat k) != extracts.length then throw "extraction position"
      extracts := extracts ++ [(← cls c, ← choice ch)]
      rest := tail
    | _ => fuel := 0
  let ["root", c] :: ["endregion"] :: tail := rest | throw "missing root or endregion"
  return (⟨f, b, i, ← valueName root, witnesses, seeds, events, extracts, ← cls c⟩, tail)

def certificate (text : String) : R Certificate := do
  if !text.endsWith "\n" || text.endsWith "\n\n" then throw "certificate line structure"
  let lines := ((text.splitOn "\n").dropLast).map (·.splitOn " ")
  let ["evp2"] :: ["registry", name, digest] :: ["input", _] :: budget :: rest := lines
    | throw "unsupported certificate header"
  if budget.head? != some "budget" then throw "missing budget record"
  let mut rest := rest
  let mut regions := []
  let mut fuel := rest.length + 1
  while fuel > 0 do
    fuel := fuel - 1
    match rest with
    | header@("region" :: _) :: tail =>
      let (region, tail) ← regionBlock header tail
      regions := regions ++ [region]
      rest := tail
    | _ => fuel := 0
  let [["output", _]] := rest | throw "missing output record"
  return ⟨name, digest, regions⟩

end Reader

end EvoIR.Rewrite
