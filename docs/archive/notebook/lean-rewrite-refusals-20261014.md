# Lean rewrite certificate refusals (T03z, Lean part), 2026-10-14

This note records the Lean part of TODO T03z. Audit finding A36
(`docs/archive/notebook/spec-audit-20261013.md`) says that no test covers
seven refusals of the Lean rewrite certificate checker. The fixtures are now
in `lean/EvoIR/Rewrite/Tests.lean`. For each refusal, the note gives the
check that refuses it, its fixture and its control. A Lean path without a
directory is in `lean/EvoIR/Rewrite/`.

## Base and method

- Base: commit `d362bde0`. The Lean tree had no working-tree change. Only
  `lean/EvoIR/Rewrite/Tests.lean` changed. No checker, theorem or
  definition outside the test file changed.
- Toolchain: Lean `v4.32.2` (`lean/lean-toolchain`). No Cargo ran.
- A scratch copy of the order of checks of `checkRewrite` found the check
  that refuses each fixture. Labelled scratch copies of `finish`,
  `insertNode` and `rewriteStep` found the line. The scratch files are not
  in the repository.
- The same scratch copy skipped one check at a time. With the refusing
  check skipped, it accepts the fixtures of clauses 6 and 7. So no later
  check refuses them.
- Four scratch mutations of `Tests.lean` made the file fail, as they must.
  One expects a refusal to be `accepted`. One expects a control to refuse.
  One skips `cleanRegion` in the label function. The last also skips it and
  gives a control that label; the comparison with `checkRewriteText` stops
  it.

## The count

Verification Section 9 lists ten refusal clauses. Before this change, the
tests covered three clauses: the alias form (`pressure-p0`), the reused root
node (`root reused`) and a difference outside the roots (three cases). They
covered one clause in part: a poison seed, but no witness result. Six clauses
had no test, and the witness part of the poison clause had none. That is
seven, the count of the audit. Some clauses have more than one check in the
code, so the change adds 15 refusals and 9 controls.

The scratch run also located the refusals of the existing cases: the alias
form at `Replay.lean:363`, `root reused` at `Replay.lean:365`, `false fact`
at the premise check `Replay.lean:269`, and `poison seed` at `Check.lean:51`.

## Refusals, checks and controls

`refusingCheck` in `Tests.lean` repeats the order of checks of
`rewriteRoots` and `regionRoot` (`Check.lean:59-82`). It names the first
check that refuses. Each case also requires that `checkRewriteText` gives
the same verdict. So the copy cannot drift from the checker without a
failure on these fixtures. The label `event j` names the event numbered `j`
in the certificate.

| Section 9 clause | Fixture (label in `Tests.lean`) | Refusing check | Control (accepted) |
|---|---|---|---|
| 1. Another version or a malformed line | `evp1 header`: the first line is `evp1` | `Certificate.lean:229-230`: "unsupported certificate header" | `evp2 certificate` |
| | `two spaces in a node record` | `Certificate.lean:148`: "bad node record" | `evp2 certificate` |
| | `space after the root record` | `Certificate.lean:223`: "missing root or endregion" | `evp2 certificate` |
| | `class with a leading zero`: `c06` | `Certificate.lean:100`: "noncanonical integer 06" | `evp2 certificate` |
| 2. An unknown registry or digest | `unknown registry name`: `p0.scalar.v9` | `Rules.lean:98`: no pair matches; used at `Check.lean:76` | `p0.scalar.v2`, `p0.scalar.v3` and `p0.scalar.v4`, each with its digest |
| | `p0.scalar.v1 with the v2 digest` | `Rules.lean:98`, at `Check.lean:76` | as above |
| 3. An extraction with more than one new instruction | `x + x to x << 1, derived constant`: the constant 1 of `shl` is a derived node | `Replay.lean:366`: `existing` gives no value for a derived node (`:351`) | `x + x to x << 1, witness constant`: the constant 1 is the witness `%v5` |
| | `an extraction entry for it`: an entry for an unused derived constant | `Replay.lean:367` | `an unused derived constant`: the same node, no entry |
| 4. A root or replacement that is not a pure constant or scalar operation | `uninitialized g0 root`: the program has `uninitialized.bits<1>` at the root; the certificate has the guard node | `Replay.lean:116` (event 4) | `g0 FixedMul fold` |
| | `uninitialized replacement`: the derived node is `op=uninitialized` | `Replay.lean:106`: `nodeOp` has no row (`:97`, `:101`) (event 5) | `g0 FixedMul fold` |
| | `uninitialized p0 root` | `Check.lean:64`, at `:53`: `allowed` (`Flow.lean:99-104`) excludes the root result | `p0 short_circuit, two roots` |
| 5. A guard region whose replacement is not the constant 1 | `fold of a false guard to 0`: count 64, premise `c6:1:0`, replacement 0 | `Replay.lean:269`: `premiseOk` (`:146-149`) (event 6) | `g0 FixedMul fold` |
| | `second root extraction in g0` | `Replay.lean:368`: the count of extraction entries | `second root extraction in p0` |
| 6. A witness result that can be poison | `guard result as a witness`: the guard `%v4` of FixedMul is a witness | `Check.lean:64`, at `:52`: `allowed` excludes a guard result | `g0 FixedMul fold` |
| 7. Two regions with one root | `two regions with one root`: the second region of `control-p0` copies the first | `Check.lean:81` | `p0 short_circuit, two roots` |

Notes on the fixtures:

- Clause 3 uses the `pressure-p0` program with `%v6 = add.bits<64> %v0, %v0`.
  Its certificates are written in the test file, with the registry
  `p0.scalar.v4` and the rule `p2_302_add_self_shl`.
- Clause 5: a count of 64 makes the guard false, so the premise `c6:1:0` is
  true. The checker cannot confirm it. A guard transfer gives a known 1 or
  no fact (`guardFact`, `lean/EvoIR/Fact.lean:222-223`).
- Clause 6: a witness operand is a seed or an earlier witness result
  (`Replay.lean:318-322`, `:334-336`). A seed is a block parameter
  (`Replay.lean:299-301`). So poison reaches a witness only through a seed,
  and the seed check (`Check.lean:51`) refuses first. With clean seeds, only
  an excluded operator gives a bad witness result. EvoIR Section 8.5 lists a
  guard result as excluded. The certificate is written in the test file.
- Clause 7: the program after keeps `%v30`. So it is equal to the program
  that `replaceAll` builds from the duplicated roots.

## Checks that no certificate reaches

Two checks repeat a rule that earlier checks already enforce. No fixture
reaches them. These statements come from reading the code. No theorem states
them.

- `Check.lean:68`, `isPure op && isPure old.op`. `nodeOp` gives only a
  constant or a scalar operation (`Replay.lean:91-101`). `witnessOk` makes
  the operation of the root instruction equal to that of the root node
  (`Replay.lean:332`). So clause 4 is enforced by `nodeOp`, by `insertNode`
  and, in a p0 region, by `cleanRegion`.
- The constant test of `Replay.lean:368`, `nd.op == .constant 1 1`. A guard
  transfer gives a known 1 or no fact (`lean/EvoIR/Fact.lean:222-223`).
  Every g0 rule has the right side `known_const`
  (`lean/EvoIR/GuardRegistry.lean`). So a rule joins a guard class only to a
  class that holds the constant 1. A congruence joins two nodes only if they
  have the same operation. So the root class of a guard region holds only
  its guard and constants 1. The premise check refuses a fold to another
  value first.

## Build and stage list

All commands ran from `lean/` with Lean `v4.32.2`.

- `lake env lean EvoIR/Rewrite/Tests.lean`: exit 0. It printed the 16
  earlier cases and the 24 new cases, with no error and no warning. It
  writes no build output. It took about 5 s and at most 0.67 GB of memory.
- `lake build --no-build EvoIR`: exit 3. The one target out of date is
  `EvoIR.Rewrite.Tests` ("target is out-of-date and needs to be rebuilt").
  So `lake build` rebuilds only that module and the root `EvoIR`.
- `lake build --no-build evoir-sem`: exit 0, "All targets up-to-date (692
  jobs)". The executable does not import the test module.
- `lake build`: not run. From 10:35 to 12:14, free disk space stayed below
  25 GiB (between 19.0 and 24.78 GiB), and the task rules forbid a heavy
  step below 25 GiB. Two guarded 20-minute waits did not start it. The
  review run of `tools/gate-lean.sh` builds it. The root `EvoIR.lean`
  imports every module. A search of the Lean tree found no other
  declaration of the nine new top-level names of `Tests.lean`, so that
  import adds no name clash.
- `lake exe evoir-sem --list-stages`: exit 0. Standard output, exactly:

```text
source-to-evoir required=source,ir optional=proposal
rewrite required=source,before,after,certificate optional=
lowering required=source,ir,ssa,postssa optional=
allocation required=source,postssa,allocation optional=
frame-emission required=source,allocation,asm,text optional=
```

  The SHA-256 of that output is
  `22c6399383dbefb5c550ac1763375ce352af15f03f886f276fbe7f210de133f0`. The
  binary `.lake/build/bin/evoir-sem` (built 2026-10-14 02:10) has the SHA-256
  `79526c4fbda59e48e136ae66716e63ee235b378a823e85185dc836904911dc42`.
  Standard error held only the replay of cached linter warnings and axiom
  reports of other modules.

## Left for the Rust lane

- Compare `evoir-sem --list-stages` with `STAGES`
  (`crates/evocc/src/proof_required.rs:138-194`). `--list-stages` prints the
  five rows of `Eval.stages` (`lean/EvoIR/Eval.lean:496-505`). `STAGES` has
  the same five identifiers and the final row `pipeline`, which
  `--list-stages` does not print. Lean splits the roles into required and
  optional. `STAGES` keeps one list per row. The role names and their order
  agree: the Rust list is the Lean required list, then the optional list.
  `proposal` is the one optional role. A Rust test can compare the Rust list
  with the two Lean lists joined, or the Rust table can carry the split.
- The other T03z items: the table-driven failure cases in
  `crates/evocc/tests/proof_required.rs`, the edge taint for `cond_br`,
  `br.indirect` and `asm.goto` in the Rust checker, the traversal test over
  all 59 instruction variants (A29), the boundary scan (A44), and the E4004
  and alias-truth tests (A45).
- A29 concerns the Rust producer and checker. The Lean checker derives
  eligibility itself (`Flow.lean:99-127`, `Check.lean:48-53`). The fixtures
  of clauses 4 and 6 exercise that path.

## Lean checks still without a fixture

These checks are outside the seven. A later task can choose from them.

- The other messages of `Rewrite.Reader` (`Certificate.lean:98-244`), for
  example the numbering of events, witnesses, seeds and extraction entries.
- The region position and the first two conjuncts of `cleanRegion`
  (`Check.lean:61-63`, `:50-51`).
- `start` (`Replay.lean:298-303`).
- The numbering and order checks of `insertNode` (`Replay.lean:104-105`,
  `:112-115`).
- `rewriteStep` at `Replay.lean:265-268` and `:273-274`.
- `congruenceStep` (`Replay.lean:278-285`). No fixture holds a congruence
  event, accepted or refused.
- The shape and root checks of `finish` (`Replay.lean:356-358`, `:362`) and
  the seed choice of `existing` (`Replay.lean:344-347`).
