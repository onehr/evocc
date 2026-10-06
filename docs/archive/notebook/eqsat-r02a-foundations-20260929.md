# R02a measurement foundations

Date: 2026-09-29. This is a partial implementation checkpoint for
[R02a](../../TODO.md), following the full-workspace
[R01a requalification](eqsat-review-closure-20260929.md). It does not close
R02a, R02b, Q2/Q3 or any application acceptance gate. No application census,
native A/A calibration, performance comparison or AI campaign was run here.

## Removed redundant work

| Review finding | Change | Preserved boundary |
| --- | --- | --- |
| Repeated execution for steps and observations, including the terminal unchanged module | Project both from one interpreter outcome; execute each changed transaction; reuse the last execution only for identical unchanged bytes | Every transaction, including the terminal no-op, still requires independent proof replay; changed observations are still compared against the original |
| Two Admission analyses of the same function | Cache one analysis per function for inventory and certificate construction within EVP | Transient optimization retains its separate analysis; eligibility and facts are unchanged |
| Redundant whole-chain state clone | Stage once and return the owned state without committing until final checker identity validation | Per-transaction rollback remains; any failure rejects the entire staged chain, not just the failing suffix |

The regression fixture is
[`identity-tail.evo`](../../tests/fixtures/eqsat-performance/identity-tail.evo).
Unit tests count interpreter calls and proof attempts, inject an observation
mismatch and verify that fuel-exhausted input never enters optimization.
A checker-process negative test lets the real checker finish its replays, then
changes the explicitly pinned wrapper bytes: proof successes do not authorize
state publication when the checker identity changed. Existing chain-level
rollback and recovery-proof tests remain part of the full workspace gate.

These changes remove redundant work by construction. They do not establish a
measured compiler or application speedup. Cached per-function Admission records
live until the EVP invocation completes; this is a storage/time tradeoff, not a
measured peak-memory reduction.

## Resource accounting

`evocc-policy-report-v3` adds `chainResources` alongside the existing
producer-only `optimizerWallNanos`. Its `shared-chain-v1` boundary partitions:

```text
totalWallNanos
  = optimizerWallNanos             producer invocation only
  + checkerWallNanos               independent replay, process startup, scratch I/O
  + identityWallNanos              checker admission and final full-byte check
  + wrapperWallNanos               remaining shared-chain orchestration
```

The total includes initial staging and final state publication. It excludes
frontend lowering, initial EqualityState creation by the evaluator, interpreter
executions, report serialization and later code generation. It is not total
compiler wall time, CPU time or RSS. A failed chain retains its attempted costs.
The existing checker identity cache may reuse a hash only after comparing all
file bytes; these scopes include that work, not a timestamp-only shortcut.
No-attempt program records use `chainResources: null`; elided timing mode uses
null counters rather than invented zero measurements. Aggregate totals sum
attempted chain scopes. Timing is excluded from deterministic chain/proof
identity.

The census reader checks the disjoint sum and independently recomputes the
aggregate. Negative cases cover missing/negative/overlapping counters, invented
no-attempt costs and stale aggregates. Version 2 remains readable exactly as
before; it does not acquire inferred checker costs. The historical
[`cap-census-20260929`](../../tests/fixtures/eqsat-chains/cap-census-20260929)
capture was not regenerated with this compiler.

## Frozen-plan audit: intentionally not a runner

The existing bench CLI now offers a read-only mode:

```sh
python3 tools/bench.py --check-eqsat-manifest tests/contracts/NAME.json
```

`NAME.json` is a plan that must be authored and committed before audit, not a
provided runnable campaign. The schema implementation is
[`eqsat_manifest.py`](../../tools/ladder/eqsat_manifest.py); the synthetic unit
data in `test_eqsat_manifest.py` are not source or binary receipts.

V1 is deliberately limited to declared native, checked, development-only A/A
calibration (`A0`, `A1`). It freezes compiler/checker/reference/assembler/linker
hashes, specification hashes, implementation source revision, source/data hashes,
argv templates, expected output hashes/exit code, declared object coverage,
target/ISA/ABI, requested/effective UB profile, semantic flags, lowering profile,
shared-chain/extractor/registry identities, scoped limits, timing boundary,
repetitions/order/seed, one-job/resource bounds and host/environment declarations.
Unknown fields, duplicate JSON keys, incomplete controls, unsupported profiles
and out-of-scope data paths fail closed. Explicit absolute tool pins may reference
the installed toolchain without copying it into this repository. Other artifacts
must stay inside the workspace, including after symlink resolution.

The audit requires exact committed plan bytes under `tests/contracts`, verifies
declared source revision against the live build inputs, hashes all declared
files and rechecks source/artifact identity. It neither builds nor runs any
workload, changes any receipt, or checks that declared controls were actually
applied. Hashing a binary is not proof that it was built from the named source.
There is no claim of reproducible-build attestation here.

An exit code of zero means only:

```text
FROZEN_PLAN_AUDITED_EXECUTION_NOT_RUN
correctness = NOT_RUN
proof = NOT_RUN
performance = NOT_RUN
effective_controls = NOT_VERIFIED
host_controls = NOT_VERIFIED
```

The source/binary pin is necessary but insufficient for measurement admission.
The execution adapter still needs to bind the actual codegen IR/chain and
objects, check effective flags and emitted guards, validate every exact timed
binary/input/result, enforce runtime resource/lock/ownership rules, retain raw
samples and implement the declared statistics. Unchecked twins additionally
depend on R02d and must share the same verified IR and chain bundle. V1 rejects
unchecked, A/B, search, held-out and emulator timing plans instead of silently
substituting checked A/A.

## Validation record

New focused Rust tests passed before integration: 22 tests across the eqopt
unit/verified-chain and policy unit suites. Python focused tests passed: 92.
These scoped numbers are not full-workspace acceptance. The initial REDs
included missing-manifest imports and unsupported v3 report rejection; an initial
Rust test compile error was corrected before the passing run.

Full integration gate completed:

| Gate | Result and scope |
| --- | --- |
| `cargo test --workspace --no-fail-fast --offline` | 2,854 PASS, 0 FAIL, 4 existing ignored campaigns; 362 test-result summaries |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo fmt --all -- --check` | PASS |
| Python integration command below | 135 PASS; bench test messages use mocks and are not kernel/application runs |
| TODO queue validation and `git diff --check` | PASS |

The four ignored campaigns remain NOT_RUN: whole sealed C corpus, extended
native randomized, standing four-lane and extended randomized campaigns. Host
conditional test success does not establish a Linux-target run. The Rust gate
log SHA-256 is
`83cbbe81a6aec933ec9dc2abebc0a9e3347cbdd4f2a5bdeb71f2cc742bb99874`.
The first Python integration run caught R02a's OPEN/LATER queue mismatch; both
the task and queue were updated before the passing run. No failed run is counted
as acceptance.

```sh
CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true \
  tools/gate-changed.sh --full-workspace --offline
python3 -m unittest \
  tools.ladder.tests.test_eqsat_chain \
  tools.ladder.tests.test_eqsat_census \
  tools.ladder.tests.test_eqsat_manifest \
  tools.ladder.tests.test_bench \
  tools.ladder.tests.test_todo_schedule \
  tools.ladder.tests.test_gate_changed
```

The gate uses the existing target directory, serializes heavy work and records
its log under `.evidence/eqsat-r02a-20260929-2YGCMW/`. It does not build a second
workspace or start kernel/application campaigns. R02a remains OPEN until its
collector and admission requirements are implemented and validated; R02b's
first application-level deliverable remains the scoped default-cap saturation
table, not an AI search or a speedup claim.

## Actual evaluator/consumer bridge

After the full workspace gate, the newly built evaluator ran the single
`identity-tail.evo` fixture in measured and elided modes with seed 7 and 1,000
interpreter steps, plus the intentional zero-step failure case. This is an
interface smoke, not a frozen application population or a native performance
run. The workspace-built checker was explicitly selected as the test trust
anchor; a neighboring binary is not a release trust anchor.

| Input | Producer report | Consumer classification |
| --- | --- | --- |
| Measured timing, 1,000 steps | One measured program, two replayed transactions | Scoped no-hits observation; all three effect-ceiling verdicts NOT_RUN |
| Elided timing, 1,000 steps | Same chain record, null timing counters | Same scoped observation; no invented zero costs |
| Zero steps | `fuelExhausted`, no chain, zero transactions/replays, null resources | INCOMPLETE, not successful zero-hit coverage |

This bridge found and fixed a reader inconsistency: a zero step budget was
mistaken for missing fuel even though the evaluator legitimately emits that
failure record. The reader now retains it, but rejects a claimed successful
chain or executed steps under zero fuel. This was reproduced as a failing unit
case before repair. The diagnostic collector's runnable-manifest positive fuel
requirement is unchanged.

Exact artifacts:

- Evaluator SHA-256: `ae95a39ae08119cbd442f2e10b8deac2efa5044cadaf7c37e2244167ba244500`.
- Checker SHA-256: `4947c24343cc177387c5a4a7debc4c6bd82acd8e7588c1303b68c3959f4fb1bd`.
- Source fixture SHA-256: `06e19bfa380ea1a1f92237026c9bd6f99ca3bf0c612d02fffac504de7e1e9d94`.

The retained `policy-{measured,elided,no-attempt}.json` reports and corresponding
`census-*.json` outputs are in the same small evidence directory. Reproduction:

```sh
EVOCC_CHECKER_SHA256=4947c24343cc177387c5a4a7debc4c6bd82acd8e7588c1303b68c3959f4fb1bd \
  target/frontend/debug/evo-policy-eval --policy instruction-count --seed 7 \
  --corpus tests/fixtures/eqsat-performance --steps 1000
```

Add `--elide-timing` for the deterministic report, or use `--steps 0` for the
failure record. Feed the captured JSON to `tools/bench.py --eqsat-census PATH`.
Neither this command nor its JSON reader makes a native runtime claim.
