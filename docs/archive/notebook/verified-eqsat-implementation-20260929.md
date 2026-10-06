# Verified EqSat implementation — September 29, 2026

Scope: [design revision 5](../../verified-optimization-design.md), starting with TODO R01a.
This is not G2, R02 performance acceptance, an AI search result, or full
implementation of the design. The starting checkout was `a4c5209`.

## R01a shared verification chain

Reopened after review: the four-crate Rust gate below omitted proofcheck. The
sealed-certificate suite failed all three tests because chain/census directories
were incorrectly placed among certificate triples. These historical scoped
counts remain valid but do not close R01a. The subsequent
[review corrections and full-workspace requalification](eqsat-review-closure-20260929.md)
at `31234f5` close all four review blockers and R01a. This does not admit an
application census without R02a's own manifests and correctness gates.

The driver and policy evaluator now call the same policy-aware persistent
runner. Policy identity includes all thirteen weights, registry ceiling,
extractor, coupled limit mapping, and transaction ceiling. Every transaction,
including a terminal zero-region proof, is replayed by the standalone checker
before publication. Internal/proof/chain failures discard the entire staged
state. Budget stops preserve already admitted regions and stop the chain.

Records distinguish result, stop, certificate availability, verification and
the closed rejection reason. Checker exit 2 is `CERTIFICATE_MALFORMED`; exit 3
is `PROOF_INVALID`. Timeouts, failure to launch and abnormal exits remain
unknown proof outcomes. A valid recovery proof cannot turn a producer failure
into successful optimization. Root/admission/search/EVP counters retain their
different units; matcher work is explicitly unmeasured.

`--eq-proof-dir=NEW_DIR` retains the actual ordered before/after/proof bytes and
the immutable chain record. Existing directories are refused. Re-admission uses
the existing benchmark entry point, not the stored `VERIFIED` label:

```sh
python3 tools/bench.py --verify-eqsat-chain DIR --checker target/frontend/debug/evo-proofcheck
```

## Reduced defects found by integration

1. **Unbound emitted definitions.** A weighted first transaction emitted a fresh
   `add` result; the next transaction could not find its persistent binding.
   Survivor renumbering alone does not bind new definitions. Ingesting actual
   after-IR shapes reconnects the emitted result to its already-proved class by
   hash-consing, without asserting another equality. The reduced cross-edge
   `bits<7>` case returns 126 before and after the chain.
2. **Stale normalized premises.** Region reconstruction could derive stronger
   facts through an emitted RHS than the transient sweep. Its normalized rule
   was paired with the sweep's old facts, causing persistent replay to reject a
   valid region. The adapter now retains the region's own reconstructed facts;
   standalone replay remains independent. The reduced `bits<64>`
   multiply/mask case returns 471102676 before and after optimization.

Both reductions are in `crates/evocc-eqopt/tests/verified_chain.rs`. Original
generated seed 3400002 and the 9300–9331 corpus reproduce the integration paths.
They are generated IR regressions, not an application performance corpus.

The first broader attempt failed on these defects and on a legacy checker
error-message compatibility regression. A concurrent rebuild also invalidated
one rustdoc dependency path. These failed/interrupted attempts are not PASS;
the final gate is serialized, with no concurrent Cargo build into the same
target directory. Production typed checker outcomes do not depend on messages.

## Acceptance commands and retained evidence

Reuse `CARGO_TARGET_DIR=target/frontend`, `CARGO_BUILD_JOBS=2`, and offline
dependencies. Do not create another target tree or kernel workspace.

```sh
CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 cargo build --offline -p evocc -p evocc-proofcheck -p evocc-policy
CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh --full-workspace --offline
python3 -m unittest tools.ladder.tests.test_eqsat_chain tools.ladder.tests.test_bench tools.ladder.tests.test_todo_schedule
```

For explicit native artifact retention, set `EVOCC_R01A_RECEIPT_DIR` to a new
directory when running the `driver_chain` integration test. On Darwin ARM64 it
lowers the exact driver-verified IR through the existing MIR/toolchain path,
retains assembly/object/executable plus proof bundles, and writes `native.json`
only after exit 0. This is a Darwin focused smoke, not the Linux target/G2 gate.

Integration capture: `.evidence/eqsat-r01a-20260929/`. The serialized Rust gate
completed with **243 PASS, 0 FAIL, 3 ignored** across 53 test-suite summaries.
The ignored standing long campaigns are NOT_RUN. This includes the 40-weight
axis sweep over 24 generated programs, 1,000-program semantic differential,
native 256-program differential, native four-lane 128-program differential,
CLI/driver parity, state rollback and checker-process failures. These populations
overlap test suites; do not sum them into a new unique-program denominator.
The Python chain/benchmark/TODO tests completed **78 PASS**.
Five-package, all-target Clippy with `-D warnings`, formatting and whitespace
checks passed. A final Clippy run caught one redundant `u16::from` conversion
in the normalized-fact repair; removing it did not change the value or type.
The changed Markdown documents have 257 checked local inline links with no
missing targets. The tracked-document commit-reference audit also passed.

The retained two-transaction archive passed independent replay with two proof
successes and zero rejection/malformed/unknown buckets. Its exact codegen output
assembled, linked and exited 0 on Darwin ARM64 with Apple clang 21.0.0. The
small text artifacts and hashes are also tracked in
[the sealed fixture](../../../tests/fixtures/eqsat-chains/verified-chain/README.md).
Original compiler/checker binaries and native object/executable remain local;
their hashes are in `native.json`. Replaying with a rebuilt checker is a new
semantic check, not a rewrite of the historical native/compiler identity.

## Next dependency

With R01a requalified, implement R02a's frozen performance/admission manifests and
R02b's default-limit saturation table first. Policy, shortcut and new-rule
tracks require separate preflight verdicts. No paid AI campaign, expanded
registry, unchecked native profile, or positive Q2/Q3 claim is authorized by
the shared-chain tests alone.
