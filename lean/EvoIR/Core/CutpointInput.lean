import EvoIR.Core.LoopReturn

/-! Untrusted cutpoint proposals address syntactic sites in the supplied source
tree, never a named fixture. The site walk is only proposal decoding: its
accuracy is not assumed by LoopPair.refines. Incorrect sites, shapes or links
must still pass the independent entry/span/successor checks. -/
namespace EvoIR.Core.Symbolic

inductive SiteWalk where
  | statement (stmt : Source.Stmt)
  | items (items : List Source.BlockItem)

def sourceSites : Nat → Nat → List Source.Frame → SiteWalk → List SourceSite
  | 0, _, _, _ => []
  | _+1, _, _, .items [] => []
  | fuel+1, count, k, .items (.declaration _ :: rest) =>
    sourceSites fuel (count+1) k (.items rest)
  | fuel+1, count, k, .items (.statement stmt :: rest) =>
    sourceSites fuel count (.sequence rest :: k) (.statement stmt) ++
      sourceSites fuel count k (.items rest)
  | fuel+1, count, k, .statement stmt =>
    .statement stmt k :: (match stmt with
    | .compound items => sourceSites fuel count (.scope count :: k) (.items items)
    | .ite _ yes no => sourceSites fuel count k (.statement yes) ++
        no.toList.flatMap (fun s => sourceSites fuel count k (.statement s))
    | .while condition body =>
      .expression condition (.loopTest condition none body :: k) ::
        sourceSites fuel count (.loopBody condition none body :: k) (.statement body)
    | .doWhile body condition =>
      sourceSites fuel count (.loopBody condition none body :: k) (.statement body)
    | .for init condition increment body =>
      let n := count + (match init with | .declarations objects => objects.length | _ => 0)
      let test := condition.getD (.literal .int 1)
      .expression test (.loopTest test increment body :: .scope count :: k) ::
        sourceSites fuel n (.loopBody test increment body :: .scope count :: k) (.statement body)
    | _ => [])

def functionSites (source : Source.Function) : List SourceSite :=
  sourceSites (Source.statementDepth source.body) source.parameters.length
    [.function none source.returnType.type] (.statement source.body)

private def fields (j : Lean.Json) : Option (List Lean.Json) :=
  j.getArr?.toOption.map Array.toList

private def natural (j : Lean.Json) : Option Nat := do
  let n ← j.getNat?.toOption
  if n < 2^32 then some n else none

private def integerType (j : Lean.Json) : Option Source.IntType := do
  match ← j.getStr?.toOption with
  | "int" => some .int | "uint" => some .uint
  | "long" => some .long | "ulong" => some .ulong
  | "long_long" => some .longLong | "ulong_long" => some .ulongLong
  | _ => none

private def localShape (j : Lean.Json) : Option LocalShape := do
  let [symbol, type, qualifier, initialized] ← fields j | none
  return ⟨← natural symbol, ⟨← integerType type, ← qualifier.getBool?.toOption⟩,
    ← initialized.getBool?.toOption⟩

private def localLink (j : Lean.Json) : Option LocalLink := do
  let [target, source, type] ← fields j | none
  return (← natural target, ← natural source, ← integerType type)

def readCutpoints (source : Source.Function) (text : String) :
    Option (Nat × List CutpointLayout) := do
  let input ← (Lean.Json.parse text).toOption
  let [version, fuel, proposals] ← fields input | none
  let version ← version.getStr?.toOption
  if version != "core-cutpoints-v1" && version != "core-cutpoints-v2" then none else
  let budget ← natural fuel
  if budget == 0 || budget > 4096 then none else
  let sites := functionSites source
  let rows ← fields proposals
  let layouts ← rows.mapM fun row => do
    let row ← fields row
    let (site, label, shape, links, tokens) ← match version, row with
      | "core-cutpoints-v1", [site, label, shape, links] => some (site, label, shape, links, [])
      | "core-cutpoints-v2", [site, label, shape, links, tokens] => do
        let tokens ← (← fields tokens).mapM fun token => do
          let [id, kind] ← fields token | none
          let kind ← kind.getStr?.toOption
          let index ← match kind with | "mem" => some 0 | "world" => some 1 | _ => none
          return (← natural id, index)
        some (site, label, shape, links, tokens)
      | _, _ => none
    return {
      site := ← sites[← natural site]?, label := ← label.getStr?.toOption,
      shape := ← (← fields shape).mapM localShape,
      links := ← (← fields links).mapM localLink, tokens : CutpointLayout }
  return (budget, layouts)

end EvoIR.Core.Symbolic
