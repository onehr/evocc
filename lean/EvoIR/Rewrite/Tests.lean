import EvoIR.Rewrite.Soundness

/-!
Acceptance and rejection tests for the rewrite checker. Run from `lean/` with
`lake env lean EvoIR/Rewrite/Tests.lean`. Each case reads the actual compiler
output in `tests/fixtures/verification/rewrite/`. A wrong result stops the
file with an error.
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
