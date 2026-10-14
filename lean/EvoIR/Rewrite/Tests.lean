import EvoIR.Rewrite.Soundness

/-!
Acceptance and rejection tests for the rewrite checker. Run from `lean/` with
`lake env lean EvoIR/Rewrite/Tests.lean`. Each case reads the actual compiler
output in `tests/fixtures/verification/rewrite/` and may edit it. Three
certificates of the Section 9 refusal cases are written in this file. A wrong
result stops the file with an error.
-/

open EvoIR.Rewrite

def fixture (name : String) : IO String :=
  IO.FS.readFile ("../tests/fixtures/verification/rewrite/" ++ name)

def expect (label : String) (want : Bool) (name : String)
    (before after cert : String → String := id) : IO Unit := do
  let b ← fixture (name ++ ".before.evo")
  let a ← fixture (name ++ ".after.evo")
  let c ← fixture (name ++ ".proof.evp")
  match checkRewriteText (before b) (after a) (cert c) with
  | .ok got =>
    if got != want then throw (IO.userError s!"{label}: expected {want}, got {got}")
    IO.println s!"{label}: {if got then "ACCEPT" else "REJECT"}"
  | .error e =>
    if want then throw (IO.userError s!"{label}: input refused: {e}")
    IO.println s!"{label}: REJECT (input: {e})"

/-! Accepted instances from actual `-O0 -fno-builtin -Oeq-scalar` output. -/

-- g0.ub.v1: `FixedMul/entry/6`, rule `g0_007_signed_count`, premise `c6:1:1`.
#eval expect "g0 FixedMul shift count fold" true "fixedmul-g0"
-- p0.scalar.v1 (a prefix of p0.scalar.v4): `short_circuit/b1/2` and
-- `short_circuit/b7/2`, rule `p0_107_fold_icmp_ne`.
#eval expect "p0 short_circuit comparison folds" true "control-p0"
-- g0.ub.v1: three `loop_join` count folds and one `short_circuit` add fold.
#eval expect "g0 control guard folds" true "control-g0"
-- The p0 alias form (`pressure/entry/1`, `p0_001_add_zero_rhs`) deletes the
-- root. This checker does not support that form and refuses it.
#eval expect "p0 pressure alias (unsupported form)" false "pressure-p0"

/-! Rejections. -/

-- A fact that is not true: the premise claims that the guard is false.
#eval expect "false fact" false "fixedmul-g0"
  (after := (·.replace "%v7 = const.bits<1> 1" "%v7 = const.bits<1> 0"))
  (cert := fun c => (c.replace "premises c6:1:1" "premises c6:1:0").replace
    "op=const sort=bits<1> effect=pure attrs=value=1" "op=const sort=bits<1> effect=pure attrs=value=0")
-- A premise on a seed that the recomputed facts do not support.
#eval expect "unsupported seed fact" false "control-g0"
  (cert := (·.replace "premises c2:1:1" "premises c2:1:1,c0:ffffffff:00000000"))
-- A rule name that is not in the named registry, and a name in no registry.
#eval expect "p0 rule under g0 registry" false "fixedmul-g0"
  (cert := (·.replace "g0_007_signed_count" "p0_107_fold_icmp_ne"))
#eval expect "unknown rule name" false "fixedmul-g0"
  (cert := (·.replace "g0_007_signed_count" "g0_008_signed_count"))
-- A rewrite of a value that can be poison: the seed receives `uninitialized`.
#eval expect "poison seed" false "poison-join"
#eval expect "same region without poison" true "poison-join"
  (before := (·.replace "^b1(%v3), ^b1(%v0)" "^b1(%v1), ^b1(%v0)"))
  (after := (·.replace "^b1(%v3), ^b1(%v0)" "^b1(%v1), ^b1(%v0)"))
-- A deleted obligation.
#eval expect "deleted ub.assume" false "fixedmul-g0"
  (after := (·.replace "    ub.assume.invalid_shift_count %v7\n" ""))
-- A change outside the declared roots.
#eval expect "change outside roots" false "fixedmul-g0"
  (after := (·.replace "%v6 = const.bits<32> 16" "%v6 = const.bits<32> 15"))
#eval expect "change in another function" false "control-p0"
  (after := (·.replace "%v21 = const.bits<32> 17" "%v21 = const.bits<32> 18"))
-- A changed witness: the certificate no longer matches the program.
#eval expect "witness mismatch" false "fixedmul-g0"
  (before := (·.replace "%v6 = const.bits<32> 16" "%v6 = const.bits<32> 64"))
  (after := (·.replace "%v6 = const.bits<32> 16" "%v6 = const.bits<32> 64"))
-- A count of 64 makes the guard false; the fact cannot be confirmed.
#eval expect "guard not known true" false "fixedmul-g0"
  (before := (·.replace "%v6 = const.bits<32> 16" "%v6 = const.bits<32> 64"))
  (after := (·.replace "%v6 = const.bits<32> 16" "%v6 = const.bits<32> 64"))
  (cert := (·.replace "attrs=value=16" "attrs=value=64"))
-- The extracted node replaced by the original root node.
#eval expect "root reused" false "control-p0"
  (cert := (·.replace "extract 0 c2 node:n3" "extract 0 c2 node:n2"))

/-! Refusals of verification Section 9 that the cases above do not reach.
`refusingCheck` names the first check that refuses, and each case requires
that `checkRewriteText` gives the same verdict. Each refusal has a control
that passes the same check, so a checker that refuses everything stops the
file. -/

/-- The first check of `checkRewrite` that refuses, or `accepted`. The order is
that of `rewriteRoots` and `regionRoot`: `registryRules`, `tableOk`, then for
region `i` its position, `cleanRegion`, `start`, the `step` of event `j`,
`finish` and `isPure`, then the duplicate-root check and the comparison with
the program after the rewrite. -/
def refusingCheck (before after : EvoIR.Core.IR.Program) (cert : Certificate) : String :=
  Id.run do
    let some rules := registryRules cert.registry cert.digest | return "registryRules"
    let T := propose before (65 * (paramCount before + 1)) (entryTable before)
    if !tableOk before T then return "tableOk"
    let guards := isGuardRegistry cert.registry
    let mut roots : List Root := []
    for (region, i) in cert.regions.zipIdx do
      let some f := before.functions.find? (·.name == region.function)
        | return s!"region {i}: position"
      let some b := f.blocks.find? (·.name == region.block) | return s!"region {i}: position"
      let some old := b.instructions[region.index]? | return s!"region {i}: position"
      if !cleanRegion (badValues f (valueCount f + 1) []) b region guards then
        return s!"region {i}: cleanRegion"
      let ctx : Ctx := ⟨b, region, rules, guards, seedFacts T f.name b region⟩
      let some st := start region b | return s!"region {i}: start"
      let mut st := st
      for (e, j) in region.events.zipIdx do
        let some next := step ctx st e | return s!"region {i}: event {j}"
        st := next
      let some (op, operands) := finish ctx st | return s!"region {i}: finish"
      if !(isPure op && isPure old.op) then return s!"region {i}: isPure"
      roots := roots ++ [⟨f.name, b.name, region.index, ⟨old.results, op, operands⟩⟩]
    let keys := roots.map fun r => (r.function, r.block, r.index)
    if keys.eraseDups != keys then return "duplicate root"
    if after != replaceAll before roots then return "after"
    return "accepted"

/-- The checker on three texts must refuse at `want`, or accept if `want` is
`accepted`. A refusal of the certificate reader is `reader: MESSAGE`. -/
def expectTexts (label want before after cert : String) : IO Unit := do
  let .ok b := EvoIR.Core.IR.Reader.readProgram before
    | throw (IO.userError s!"{label}: the program before is not readable")
  let .ok a := EvoIR.Core.IR.Reader.readProgram after
    | throw (IO.userError s!"{label}: the program after is not readable")
  let got := match EvoIR.Rewrite.Reader.certificate cert with
    | .ok c => refusingCheck b a c
    | .error e => s!"reader: {e}"
  if got != want then throw (IO.userError s!"{label}: expected {want}, got {got}")
  let verdict := match checkRewriteText before after cert with
    | .ok true => "accepted"
    | .ok false => "refused"
    | .error e => s!"reader: {e}"
  if verdict != (if want == "accepted" || want.startsWith "reader: " then want else "refused") then
    throw (IO.userError s!"{label}: checkRewriteText gives {verdict}")
  IO.println s!"{label}: {want}"

/-- `expectTexts` on the files of fixture `name`, with edits as in `expect`. -/
def expectAt (label want name : String) (before after cert : String → String := id) :
    IO Unit := do
  expectTexts label want (before (← fixture (name ++ ".before.evo")))
    (after (← fixture (name ++ ".after.evo"))) (cert (← fixture (name ++ ".proof.evp")))

-- Another version, or a malformed line. The reader splits at each line feed
-- and each space. It trims nothing. The control is the certificate as written.
#eval expectAt "evp2 certificate" "accepted" "fixedmul-g0"
#eval expectAt "evp1 header" "reader: unsupported certificate header" "fixedmul-g0"
  (cert := (·.replace "evp2\n" "evp1\n"))
#eval expectAt "two spaces in a node record" "reader: bad node record" "fixedmul-g0"
  (cert := (·.replace "node 0 n0 c2 source" "node 0 n0 c2  source"))
#eval expectAt "space after the root record" "reader: missing root or endregion" "fixedmul-g0"
  (cert := (·.replace "root c6\n" "root c6 \n"))
#eval expectAt "class with a leading zero" "reader: noncanonical integer 06" "fixedmul-g0"
  (cert := (·.replace "extract 0 c6 node:n5" "extract 0 c06 node:n5"))

-- An unknown registry, or a known name with another digest. Each registry
-- that holds `p0_107_fold_icmp_ne` is a control.
def p0v1Registry : String :=
  "p0.scalar.v1 ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9"
#eval expectAt "unknown registry name" "registryRules" "control-p0"
  (cert := (·.replace "registry p0.scalar.v1" "registry p0.scalar.v9"))
#eval expectAt "p0.scalar.v1 with the v2 digest" "registryRules" "control-p0"
  (cert := (·.replace p0v1Registry
    "p0.scalar.v1 c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce"))
#eval expectAt "p0.scalar.v2" "accepted" "control-p0"
  (cert := (·.replace p0v1Registry
    "p0.scalar.v2 c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce"))
#eval expectAt "p0.scalar.v3" "accepted" "control-p0"
  (cert := (·.replace p0v1Registry
    "p0.scalar.v3 aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15"))
#eval expectAt "p0.scalar.v4" "accepted" "control-p0"
  (cert := (·.replace p0v1Registry
    "p0.scalar.v4 1a5391603386e18e18bfb1b2fc61985c9eb252c6e65bcb3259104bd49b2a9d74"))

-- An extraction with more than one new instruction. The edited `pressure-p0`
-- program has `%v6 = x + x`, and `p2_302_add_self_shl` rewrites it to `x << 1`.
-- The constant 1 must be an earlier witness. The extraction may name only the
-- root class and the operand classes of the replacement.
def shlHeader : String :=
  "evp2\nregistry p0.scalar.v4 1a5391603386e18e18bfb1b2fc61985c9eb252c6e65bcb3259104bd49b2a9d74\n" ++
  "input 0000000000000000000000000000000000000000000000000000000000000000\n" ++
  "budget nodes=3/100000 epochs=1/1024 fuel=1/256 n=2 stop=saturated\n"

def shlFooter : String :=
  "endregion\noutput 0000000000000000000000000000000000000000000000000000000000000000\n"

/-- The constant 1 is the witness `%v5`. `node` and `entry` add a node and an
extraction entry. -/
def shlWitness (node entry : String := "") : String :=
  shlHeader ++
  "region pressure/entry/3 root %v6 witnesses 2 seeds 1\n" ++
  "witness 0 pressure/entry/2\nwitness 1 pressure/entry/3\n" ++
  "seed 0 %v0 bits<64> input c0\n" ++
  "node 0 n0 c1 source=before op=const sort=bits<64> effect=pure attrs=value=1 children=-\n" ++
  "node 1 n1 c2 source=before op=add sort=bits<64> effect=pure attrs=- children=c0,c0\n" ++
  "epoch 1\n" ++
  "node 2 n2 c3 source=derived op=shl sort=bits<64> effect=pure attrs=- children=c0,c1\n" ++
  "rewrite 3 p2_302_add_self_shl c2 c3 c2 node:n1 node:n2 bind ?x:c0\n" ++ node ++
  "extract 0 c2 node:n2\nextract 1 c0 seed:0\nextract 2 c1 node:n0\n" ++ entry ++
  "root c2\n" ++ shlFooter

/-- The constant 1 is a derived node, a second new instruction. -/
def shlDerived : String :=
  shlHeader ++
  "region pressure/entry/3 root %v6 witnesses 1 seeds 1\n" ++
  "witness 0 pressure/entry/3\n" ++
  "seed 0 %v0 bits<64> input c0\n" ++
  "node 0 n0 c1 source=before op=add sort=bits<64> effect=pure attrs=- children=c0,c0\n" ++
  "epoch 1\n" ++
  "node 1 n1 c2 source=derived op=const sort=bits<64> effect=pure attrs=value=1 children=-\n" ++
  "node 2 n2 c3 source=derived op=shl sort=bits<64> effect=pure attrs=- children=c0,c2\n" ++
  "rewrite 3 p2_302_add_self_shl c1 c3 c1 node:n0 node:n2 bind ?x:c0\n" ++
  "extract 0 c1 node:n2\nextract 1 c0 seed:0\nextract 2 c2 node:n1\n" ++
  "root c1\n" ++ shlFooter

#eval do
  let p ← fixture "pressure-p0.before.evo"
  let before := p.replace "%v6 = add.bits<64> %v0, %v5" "%v6 = add.bits<64> %v0, %v0"
  let after := p.replace "%v6 = add.bits<64> %v0, %v5" "%v6 = shl.bits<64> %v0, %v5"
  let node := "node 4 n3 c4 source=derived op=const sort=bits<64> effect=pure attrs=value=1 children=-\n"
  expectTexts "x + x to x << 1, witness constant" "accepted" before after shlWitness
  expectTexts "x + x to x << 1, derived constant" "region 0: finish" before after shlDerived
  expectTexts "an unused derived constant" "accepted" before after (shlWitness node)
  expectTexts "an extraction entry for it" "region 0: finish" before after
    (shlWitness node "extract 3 c4 node:n3\n")

-- A root or replacement that is not a pure constant or scalar operation. In a
-- p0 region, `cleanRegion` excludes such a root. In a g0 region, `cleanRegion`
-- does not check the root, which is a guard. There the node checks of `step`
-- refuse.
#eval expectAt "g0 FixedMul fold" "accepted" "fixedmul-g0"
#eval expectAt "uninitialized g0 root" "region 0: event 4" "fixedmul-g0"
  (before := (·.replace "%v7 = shift_count_defined.signed %v5, %v6" "%v7 = uninitialized.bits<1>"))
#eval expectAt "uninitialized replacement" "region 0: event 5" "fixedmul-g0"
  (cert := (·.replace "op=const sort=bits<1> effect=pure attrs=value=1"
    "op=uninitialized sort=bits<1> effect=pure attrs=-"))
#eval expectAt "uninitialized p0 root" "region 0: cleanRegion" "control-p0"
  (before := (·.replace "%v8 = icmp.ne %v6, %v7" "%v8 = uninitialized.bits<1>"))

-- A guard region whose replacement is not the constant 1. A guard transfer
-- gives a known 1 or no fact. A count of 64 makes the guard false. The premise
-- check of event 6 still refuses its fold to 0. `finish` refuses a second
-- extraction entry for the root of a guard region. A p0 region admits it.
#eval expectAt "fold of a false guard to 0" "region 0: event 6" "fixedmul-g0"
  (before := (·.replace "%v6 = const.bits<32> 16" "%v6 = const.bits<32> 64"))
  (after := fun a => (a.replace "%v6 = const.bits<32> 16" "%v6 = const.bits<32> 64").replace
    "%v7 = const.bits<1> 1" "%v7 = const.bits<1> 0")
  (cert := fun c => ((c.replace "attrs=value=16" "attrs=value=64").replace
    "premises c6:1:1" "premises c6:1:0").replace
    "op=const sort=bits<1> effect=pure attrs=value=1" "op=const sort=bits<1> effect=pure attrs=value=0")
#eval expectAt "second root extraction in g0" "region 0: finish" "fixedmul-g0"
  (cert := (·.replace "extract 0 c6 node:n5\n" "extract 0 c6 node:n5\nextract 1 c6 node:n5\n"))
#eval expectAt "second root extraction in p0" "accepted" "control-p0"
  (cert := (·.replace "extract 0 c2 node:n3\n" "extract 0 c2 node:n3\nextract 1 c2 node:n3\n"))

-- A witness result that is excluded: the FixedMul guard `%v4` as a witness.
-- Every witness operand is a seed or an earlier witness. So with clean seeds,
-- only an excluded operator gives such a result. The control is the FixedMul
-- fold above.
def guardWitness : String :=
  "evp2\nregistry g0.ub.v1 c49a0caac284fe276cf1605b312f45a8ddb5378b61dc0aaf352e03a41d21cc84\n" ++
  "input 29764f97f2e8761a7c508455b9fdcda4719d7ebf95517ebccb8d27cea65b8dde\n" ++
  "budget nodes=7/100000 epochs=1/1024 fuel=1/256 n=6 stop=saturated\n" ++
  "region FixedMul/entry/6 root %v7 witnesses 6 seeds 2\n" ++
  "witness 0 FixedMul/entry/0\nwitness 1 FixedMul/entry/1\nwitness 2 FixedMul/entry/2\n" ++
  "witness 3 FixedMul/entry/4\nwitness 4 FixedMul/entry/5\nwitness 5 FixedMul/entry/6\n" ++
  "seed 0 %v0 bits<32> input c0\nseed 1 %v1 bits<32> input c1\n" ++
  "node 0 n0 c2 source=before op=sext sort=bits<64> effect=pure attrs=- children=c0\n" ++
  "node 1 n1 c3 source=before op=sext sort=bits<64> effect=pure attrs=- children=c1\n" ++
  "node 2 n2 c4 source=before op=signed_mul_defined sort=bits<1> effect=pure attrs=- children=c2,c3\n" ++
  "node 3 n3 c5 source=before op=mul sort=bits<64> effect=pure attrs=- children=c2,c3\n" ++
  "node 4 n4 c6 source=before op=const sort=bits<32> effect=pure attrs=value=16 children=-\n" ++
  "node 5 n5 c7 source=before op=shift_count_defined.signed sort=bits<1> effect=pure attrs=- children=c5,c6\n" ++
  "epoch 1\n" ++
  "node 6 n6 c8 source=derived op=const sort=bits<1> effect=pure attrs=value=1 children=-\n" ++
  "rewrite 7 g0_007_signed_count c7 c8 c7 node:n5 node:n6 bind ?a:c5,?k:c6 premises c7:1:1\n" ++
  "extract 0 c7 node:n6\nroot c7\nendregion\n" ++
  "output e36cdb25e1d484331f8c90c1e6eee7a1cde492db7266774c03b307913e6e6263\n"
#eval do
  expectTexts "guard result as a witness" "region 0: cleanRegion"
    (← fixture "fixedmul-g0.before.evo") (← fixture "fixedmul-g0.after.evo") guardWitness

-- Two regions with one root: the second region of control-p0 becomes a copy
-- of the first. The program after keeps `%v30`. So only the duplicate-root
-- check refuses.
#eval expectAt "p0 short_circuit, two roots" "accepted" "control-p0"
#eval expectAt "two regions with one root" "duplicate root" "control-p0"
  (after := (·.replace "%v30 = const.bits<1> 1" "%v30 = icmp.ne %v28, %v29"))
  (cert := fun c => ((c.replace "short_circuit/b7/" "short_circuit/b1/").replace
    "root %v30" "root %v8").replace "value=9" "value=3")

/-! Axiom audit. A native or unproved axiom changes the message and fails the file. -/

/-- info: 'EvoIR.Rewrite.checkRewrite_refines' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms EvoIR.Rewrite.checkRewrite_refines

/-- info: 'EvoIR.Rewrite.source_rewrite_refines' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms EvoIR.Rewrite.source_rewrite_refines

/-- info: 'EvoIR.Rewrite.check_source_rewrite_refines' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms EvoIR.Rewrite.check_source_rewrite_refines
