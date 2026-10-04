import EvoIR.Core.SourceSpan
import EvoIR.Core.TargetCutpoint
import EvoIR.Core.CheckpointTests
import EvoIR.Core.LoopCorrespondence
import EvoIR.Core.CutpointInvariant
import EvoIR.Core.LoopReturn
import EvoIR.Core.CutpointInput

/-! Frozen loop_join exports exercise actual for/while/do-while, break and
continue spans. Whole-pair admission checks the complete cyclic table and
actual entry; the proposed associations below are test data, not a whitelist. -/
namespace EvoIR.Core.Symbolic.CutpointTests

private def sourceText := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[0],[["function",0,"loop_join",["uint",false],false,[["parameter",1,["uint",false],"none"],["parameter",2,["uint",false],"none"]],["compound",[["object",3,"total",["uint",false],"none",["store_scalar",["uint",false],["lit","int","00000000"]]],["for",["declarations",[["object",4,"i",["uint",false],"none",["store_scalar",["uint",false],["lit","int","00000000"]]]]],["binary","int","lt",["ident","uint",4],["ident","uint",2]],["update","uint","pre_inc",4,["uint",false],["ident","uint",4]],["compound",[["if",["binary","int","eq",["binary","uint","bit_and",["ident","uint",4],["lit","uint","00000003"]],["lit","uint","00000000"]],["continue"],null],["if",["binary","int","eq",["ident","uint",4],["lit","uint","00000011"]],["break"],null],["expr",["assign","uint","add",3,["uint",false],["ident","uint",3],["conditional","uint",["binary","uint","bit_and",["ident","uint",1],["lit","uint","00000001"]],["ident","uint",4],["binary","uint","bit_xor",["ident","uint",4],["ident","uint",1]]]]],["expr",["assign","uint","set",1,["uint",false],["ident","uint",1],["binary","uint","bit_or",["binary","uint","shr",["ident","uint",1],["lit","int","00000001"]],["binary","uint","shl",["ident","uint",1],["lit","int","0000001f"]]]]]]]],["while",["binary","int","gt",["ident","uint",2],["lit","uint","00000014"]],["expr",["assign","uint","sub",2,["uint",false],["ident","uint",2],["lit","uint","00000007"]]]],["do_while",["compound",[["expr",["assign","uint","add",3,["uint",false],["ident","uint",3],["ident","uint",2]]],["expr",["assign","uint","shr",2,["uint",false],["ident","uint",2],["lit","int","00000001"]]]]],["binary","int","ne",["ident","uint",2],["lit","uint","00000000"]]],["return",["uint",false],["ident","uint",3]]]]]]]"#
private def targetText := r#"func @loop_join(%arg0: bits<32>, %arg1: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>):
    %v2 = const.bits<32> 0
    %v3 = const.bits<32> 0
    br ^b1(%v3, %v0, %v1, %v2)

^b1(%v4: bits<32>, %v5: bits<32>, %v6: bits<32>, %v7: bits<32>):
    %v8 = icmp.ult %v4, %v6
    cond_br %v8, ^b2(%v4, %v5, %v6, %v7), ^b9(%v6, %v7)

^b2(%v9: bits<32>, %v10: bits<32>, %v11: bits<32>, %v12: bits<32>):
    %v13 = const.bits<32> 3
    %v14 = and.bits<32> %v9, %v13
    %v15 = const.bits<32> 0
    %v16 = icmp.eq %v14, %v15
    cond_br %v16, ^b7(%v9, %v10, %v11, %v12), ^b3(%v9, %v10, %v11, %v12)

^b3(%v17: bits<32>, %v18: bits<32>, %v19: bits<32>, %v20: bits<32>):
    %v21 = const.bits<32> 17
    %v22 = icmp.eq %v17, %v21
    cond_br %v22, ^b9(%v19, %v20), ^b4(%v17, %v18, %v19, %v20)

^b4(%v23: bits<32>, %v24: bits<32>, %v25: bits<32>, %v26: bits<32>):
    %v27 = const.bits<32> 1
    %v28 = and.bits<32> %v24, %v27
    %v29 = const.bits<32> 0
    %v30 = icmp.ne %v28, %v29
    cond_br %v30, ^b5(%v23, %v24, %v25, %v26), ^b6(%v23, %v24, %v25, %v26)

^b5(%v31: bits<32>, %v32: bits<32>, %v33: bits<32>, %v34: bits<32>):
    br ^b8(%v31, %v32, %v33, %v34, %v31)

^b6(%v35: bits<32>, %v36: bits<32>, %v37: bits<32>, %v38: bits<32>):
    %v39 = xor.bits<32> %v35, %v36
    br ^b8(%v35, %v36, %v37, %v38, %v39)

^b7(%v40: bits<32>, %v41: bits<32>, %v42: bits<32>, %v43: bits<32>):
    %v44 = const.bits<32> 1
    %v45 = add.bits<32> %v40, %v44
    br ^b1(%v45, %v41, %v42, %v43)

^b8(%v46: bits<32>, %v47: bits<32>, %v48: bits<32>, %v49: bits<32>, %v50: bits<32>):
    %v51 = add.bits<32> %v49, %v50
    %v52 = const.bits<32> 1
    %v53 = shift_count_defined.signed %v47, %v52
    ub.assume.invalid_shift_count %v53
    %v54 = lshr.bits<32> %v47, %v52
    %v55 = const.bits<32> 31
    %v56 = shift_count_defined.signed %v47, %v55
    ub.assume.invalid_shift_count %v56
    %v57 = shl.bits<32> %v47, %v55
    %v58 = or.bits<32> %v54, %v57
    br ^b7(%v46, %v58, %v48, %v51)

^b9(%v59: bits<32>, %v60: bits<32>):
    br ^b10(%v59, %v60)

^b10(%v61: bits<32>, %v62: bits<32>):
    %v63 = const.bits<32> 20
    %v64 = icmp.ult %v63, %v61
    cond_br %v64, ^b11(%v61, %v62), ^b12(%v61, %v62)

^b11(%v65: bits<32>, %v66: bits<32>):
    %v67 = const.bits<32> 7
    %v68 = sub.bits<32> %v65, %v67
    br ^b10(%v68, %v66)

^b12(%v69: bits<32>, %v70: bits<32>):
    br ^b13(%v69, %v70)

^b13(%v71: bits<32>, %v72: bits<32>):
    %v73 = add.bits<32> %v72, %v71
    %v74 = const.bits<32> 1
    %v75 = shift_count_defined.signed %v71, %v74
    ub.assume.invalid_shift_count %v75
    %v76 = lshr.bits<32> %v71, %v74
    br ^b14(%v76, %v73)

^b14(%v77: bits<32>, %v78: bits<32>):
    %v79 = const.bits<32> 0
    %v80 = icmp.ne %v77, %v79
    cond_br %v80, ^b13(%v77, %v78), ^b15(%v78)

^b15(%v81: bits<32>):
    ret %v81
}"#

private def widths (_ : Var) : Nat := 32
private def localSlot (id : Nat) : LocalSlot widths :=
  ⟨id, ⟨.uint, false⟩, some (.var (toString id) 32 rfl)⟩

private structure Point where
  site : SourceSite
  label : String
  locals : List Nat
  arguments : List Nat

private def points (f : Source.Function) : Option (List Point) := do
  let .compound [.declaration total, .statement forStmt, .statement whileStmt,
      .statement doStmt, .statement ret] := f.body | none
  let .for (.declarations [index]) condition increment body := forStmt | none
  let .while whileCondition whileBody := whileStmt | none
  let .doWhile doBody doCondition := doStmt | none
  let [x, n] := f.parameters | none
  let outer := [Source.Frame.scope 2, .function none f.returnType.type]
  let remaining := [Source.BlockItem.statement whileStmt, .statement doStmt, .statement ret]
  let forSite := SourceSite.expression (condition.getD (.literal .int 1))
    (.loopTest (condition.getD (.literal .int 1)) increment body ::
      .scope 3 :: .sequence remaining :: outer)
  let whileSite := SourceSite.expression whileCondition
    (.loopTest whileCondition none whileBody ::
      .sequence [.statement doStmt, .statement ret] :: outer)
  let doSite := SourceSite.statement doBody
    (.loopBody doCondition none doBody :: .sequence [.statement ret] :: outer)
  return [
    ⟨forSite, "b1", [index.symbol, total.symbol, x.symbol, n.symbol],
      [index.symbol, x.symbol, n.symbol, total.symbol]⟩,
    ⟨whileSite, "b10", [total.symbol, x.symbol, n.symbol], [n.symbol, total.symbol]⟩,
    ⟨doSite, "b13", [total.symbol, x.symbol, n.symbol], [n.symbol, total.symbol]⟩]

private def Point.cursor (point : Point) : SourceCursor widths :=
  match point.site with
  | .expression e k => ⟨.expression e, point.locals.map localSlot, k⟩
  | .statement s k => ⟨.statement s, point.locals.map localSlot, k⟩

private def projectedScope (target : IR.Function) (point : Point) (ls : LocalStore widths) :
    Option (Scope widths) := do
  let block ← target.blocks.find? (·.name == point.label)
  let args ← point.arguments.mapM (fun id => do
    let w ← ls.read id .uint
    return Slot.word w.word)
  bindSlots (fun _ => .world) block.parameters args

private def leafMatches (target : IR.Function) (ps : List Point)
    (s : SourceCursor widths) (t : TargetExit widths) : Bool :=
  match s.control, t with
  | .returned w, .returned v => decide (w.type = v.type) && sameWord w.word v.word
  | _, .next label scope =>
    ps.any fun point => decide (s.site = some point.site) &&
      point.label == label && s.locals.map LocalSlot.symbol == point.locals &&
      decide (projectedScope target point s.locals = some scope)
  | _, _ => false

private def spansMatch (target : IR.Function) (ps : List Point)
    (s : CutTree widths (SourceCursor widths)) (t : CutTree widths (TargetExit widths)) : Bool :=
  (cutMatch (related := fun a b => leafMatches target ps a b = true)
    (fun a b => if h : leafMatches target ps a b = true then some ⟨h⟩ else none)
    s.normalizeBranches t.normalizeBranches).isSome

private def spans (source : Source.Function) (target : IR.Function) (ps : List Point) :
    Option (List (String × CutTree widths (SourceCursor widths) × CutTree widths (TargetExit widths))) := do
  let initialLocals := source.parameters.map (fun p => localSlot p.symbol)
  let some block := target.blocks.head? | none
  let entry : Point := ⟨.statement source.body [.function none source.returnType.type],
    block.name, source.parameters.map Source.Parameter.symbol,
    source.parameters.map Source.Parameter.symbol⟩
  (entry :: ps).mapM fun point => do
    let cursor := if point.label == block.name then
      ({point.cursor with locals := initialLocals}) else point.cursor
    let scope ← projectedScope target point cursor.locals
    let s ← sourceCutDiagram ⟨[source.symbol], [source]⟩ (ps.map Point.site) 160 true cursor
    let t ← targetCutDiagram (fun _ => .world) target source.returnType.type
      (ps.map Point.label) (target.blocks.length+1) true scope point.label
    return (point.label, s.val, t.val)

private def layouts (source : Source.Function) (target : IR.Function) (ps : List Point) :
    Option (List CutpointLayout) := do
    let block ← target.blocks.head?
    let entry : Point := ⟨.statement source.body [.function none source.returnType.type],
      block.name, source.parameters.map Source.Parameter.symbol,
      source.parameters.map Source.Parameter.symbol⟩
    (entry :: ps).mapM fun (point : Point) => do
      let block ← target.blocks.find? (fun (b : IR.Block) => b.name == point.label)
      if block.parameters.length != point.arguments.length then none else
      return ⟨point.site, (point.locals.map localSlot).map LocalSlot.shape, point.label,
        ((block.parameters.zip point.arguments).map
          (fun (pair : IR.Binding × Nat) => (pair.1.id, pair.2, Source.IntType.uint))).reverse, []⟩

private def closedTable (source : Source.Function) (target : IR.Function) (ps : List Point) : Bool :=
  (do
    let proposed ← (← layouts source target ps).mapM layoutPoint
    return (checkedSpans ⟨[source.symbol], [source]⟩ ⟨[target]⟩ proposed source.symbol target false source.returnType.type 160
      (proposed.map LayoutPoint.joint)).isSome).getD false

private def wholePair (source : Source.Function) (target : IR.Function) (ps : List Point) : Bool :=
  (do
    let proposed ← layouts source target ps
    return (loopPair ⟨[source.symbol], [source]⟩ ⟨[target]⟩ proposed 160).isSome).getD false

private def proposalText := r#"["core-cutpoints-v1",160,[
  [0,"entry",[[1,"uint",false,true],[2,"uint",false,true]],[[1,2,"uint"],[0,1,"uint"]]],
  [2,"b1",[[4,"uint",false,true],[3,"uint",false,true],[1,"uint",false,true],[2,"uint",false,true]],[[7,3,"uint"],[6,2,"uint"],[5,1,"uint"],[4,4,"uint"]]],
  [11,"b10",[[3,"uint",false,true],[1,"uint",false,true],[2,"uint",false,true]],[[62,3,"uint"],[61,2,"uint"]]],
  [14,"b13",[[3,"uint",false,true],[1,"uint",false,true],[2,"uint",false,true]],[[72,3,"uint"],[71,2,"uint"]]]
]]"#

private def wirePair (source : Source.Program) (target : IR.Program) (text : String) : Bool :=
  (do
    let [sf] := source.functions | none
    let (budget, proposed) ← readCutpoints sf text
    return (loopPair source target proposed budget).isSome).getD false

private def infiniteSource : Source.Program :=
  ⟨[0], [⟨0, "forever", ⟨.uint, false⟩, false, [],
    .compound [.statement (.while (.literal .int 1) .null),
      .statement (.return ⟨.uint, false⟩ (.literal .uint 0))]⟩]⟩

private def infiniteTarget : IR.Program :=
  ⟨[⟨"forever", [], [.bits 32], false, IR.Reader.emptyEffects,
    [⟨"entry", [], [], .branch ⟨"loop", []⟩⟩,
     ⟨"loop", [], [], .branch ⟨"loop", []⟩⟩]⟩]⟩

private def infiniteProposal := r#"["core-cutpoints-v1",32,[[0,"entry",[],[]],[2,"loop",[],[]]]]"#

def cutpointSelfTest : IO UInt32 := do
  let .ok source := Source.readProgram sourceText | IO.eprintln "loop source decode failed"; return 1
  let .ok target := IR.Reader.readProgram targetText | IO.eprintln "loop target decode failed"; return 1
  let [s] := source.functions | return 1
  let [t] := target.functions | return 1
  let some ps := points s | IO.eprintln "frozen loop shape changed"; return 1
  let some results := spans s t ps | IO.eprintln "actual cyclic span refused"; return 1
  let mut failures := 0
  if !wirePair source target proposalText then
    IO.eprintln "wire proposal refused"
    failures := failures+1
  if !wirePair infiniteSource infiniteTarget infiniteProposal then
    IO.eprintln s!"unbounded divergent pair refused: source={reprStr (Source.checkWellFormed infiniteSource)} target={reprStr (IR.checkProgram infiniteTarget)}"
    failures := failures+1
  let proposalMutations := [
    ("version", "core-cutpoints-v1", "core-cutpoints-v0"),
    ("zero span budget", ",160,", ",0,"),
    ("unknown site", "[2,\"b1\"", "[900,\"b1\""),
    ("wrong source site", "[2,\"b1\"", "[3,\"b1\""),
    ("wrong destination", "[2,\"b1\"", "[2,\"b2\""),
    ("wrong qualifier", "[4,\"uint\",false,true]", "[4,\"uint\",true,true]"),
    ("uninitialized local", "[4,\"uint\",false,true]", "[4,\"uint\",false,false]"),
    ("wrong rank", "[4,\"uint\",false,true]", "[4,\"int\",false,true]"),
    ("wrong binding", "[4,4,\"uint\"]", "[4,3,\"uint\"]"),
    ("swapped entry", "[[1,2,\"uint\"],[0,1,\"uint\"]]", "[[1,1,\"uint\"],[0,2,\"uint\"]]"),
    ("empty proposal", proposalText, "[\"core-cutpoints-v1\",160,[]]")]
  for (name, before, after) in proposalMutations do
    if !proposalText.contains before || wirePair source target (proposalText.replace before after) then
      IO.eprintln s!"cutpoint wire mutation escaped: {name}"
      failures := failures+1
  if !wholePair s t ps then
    IO.eprintln "whole cyclic pair refused"
    failures := failures+1
  for altered in [{t with name := "wrong-entry"}, {t with parameters := []},
      {t with internal := !t.internal}] do
    if wholePair s altered ps then
      IO.eprintln "entry contract mutation escaped whole-pair admission"
      failures := failures+1
  if !closedTable s t ps then
    IO.eprintln "independent cyclic successor closure refused"
    failures := failures+1
  for (name, a, b) in results do
    if !spansMatch t ps a b then
      IO.eprintln s!"cyclic span mismatch: {name}"
      failures := failures+1
  for point in ps do
    let some scope := projectedScope t point point.cursor.locals | return 1
    if (sourceCutDiagram source (ps.map Point.site) 0 true point.cursor).isSome ||
        (targetCutDiagram (fun _ => .world) t .uint (ps.map Point.label) 0 true scope point.label).isSome then
      IO.eprintln "zero-step root was accepted"
      failures := failures+1
  let targetMutations := [
    ("wrong for increment", "add.bits<32> %v40, %v44", "add.bits<32> %v40, %v40"),
    ("lost continue increment", "^b1(%v45, %v41, %v42, %v43)", "^b1(%v40, %v41, %v42, %v43)"),
    ("swapped backedge arguments", "^b1(%v45, %v41, %v42, %v43)", "^b1(%v41, %v45, %v42, %v43)"),
    ("broken break edge", "^b9(%v19, %v20)", "^b9(%v20, %v19)"),
    ("wrong while decrement", "const.bits<32> 7", "const.bits<32> 6"),
    ("wrong do condition", "icmp.ne %v77, %v79", "icmp.eq %v77, %v79"),
    ("lost rotation diagnostic", "    ub.assume.invalid_shift_count %v53\n", ""),
    ("wrong rotation diagnostic", "ub.assume.invalid_shift_count %v53", "ub.assume.signed_overflow %v53"),
    ("wrong final result", "ret %v81", "ret %v0"),
    ("wrong backedge width", "%v4: bits<32>", "%v4: bits<64>")]
  for (name, before, after) in targetMutations do
    if !targetText.contains before then
      IO.eprintln s!"mutation has no target: {name}"
      failures := failures+1
    else
      let .ok changed := IR.Reader.readProgram (targetText.replace before after) | continue
      let [altered] := changed.functions | continue
      if wholePair s altered ps then
        IO.eprintln s!"whole cyclic pair mutation escaped: {name}"
        failures := failures+1
      if closedTable s altered ps then
        IO.eprintln s!"cyclic invariant mutation escaped: {name}"
        failures := failures+1
      if let some result := spans s altered ps then
        if result.all (fun (_, a, b) => spansMatch altered ps a b) then
          IO.eprintln s!"cyclic mutation escaped: {name}"
          failures := failures+1
  let sourceMutations := [
    ("continue becomes break", "[\"continue\"]", "[\"break\"]"),
    ("break becomes continue", "[\"break\"]", "[\"continue\"]"),
    ("for increment reversed", "\"pre_inc\"", "\"pre_dec\""),
    ("wrong break constant", "00000011", "00000012")]
  for (name, before, after) in sourceMutations do
    if !sourceText.contains before then
      IO.eprintln s!"mutation has no source: {name}"
      failures := failures+1
    else
      let .ok changed := Source.readProgram (sourceText.replace before after) | continue
      let [altered] := changed.functions | continue
      let some alteredPoints := points altered | continue
      if wholePair altered t alteredPoints then
        IO.eprintln s!"whole cyclic source mutation escaped: {name}"
        failures := failures+1
      if closedTable altered t alteredPoints then
        IO.eprintln s!"cyclic source-invariant mutation escaped: {name}"
        failures := failures+1
      if let some result := spans altered t alteredPoints then
        if result.all (fun (_, a, b) => spansMatch t alteredPoints a b) then
          IO.eprintln s!"cyclic mutation escaped: {name}"
          failures := failures+1
  let some first := ps.head? | return 1
  let badInvariant := {first with arguments := first.arguments.reverse} :: ps.tail
  if closedTable s t badInvariant || closedTable s t ps.tail then
    IO.eprintln "false or incomplete cyclic invariant admitted"
    failures := failures+1
  if let some result := spans s t badInvariant then
    if result.all (fun (_, a, b) => spansMatch t badInvariant a b) then
      IO.eprintln "false entry/backedge association escaped"
      failures := failures+1
  if (spans s t ps.tail).isSome then
    IO.eprintln "uncovered cyclic path was bounded-unrolled"
    failures := failures+1
  if failures == 0 then IO.println "Finite and divergent whole-loop pairs pass; 4 frozen cyclic spans close; 6 zero-step roots and 30 mutations refuse"
  return if failures == 0 then 0 else 1

end EvoIR.Core.Symbolic.CutpointTests
