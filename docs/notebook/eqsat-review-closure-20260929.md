# R01a review corrections and requalification

Date: 2026-09-29. Scope: the R01a/R02 prerequisite review, not R02 application
admission, performance results, G2 or completion of the architecture plan.

## Reproduced failure and gate correction

The original four-crate Rust gate omitted `evocc-proofcheck`. Running
`cargo test --offline -p evocc-proofcheck --test sealed_certificate_fixtures`
reproduced **0 passed / 3 failed**: `cap-census-20260929` and `verified-chain`
were placed in a directory whose contract requires every child to be a sealed
certificate triple. The fixtures moved byte-for-byte to
`tests/fixtures/eqsat-chains/`; no certificate, native receipt or census result
was regenerated. The sealed-triple suite subsequently passed all three tests.
R01a was reopened; its earlier scoped counts remain historical, not sufficient
acceptance. `tools/gate-changed.sh --full-workspace` now makes the required
workspace test, all-target Clippy and formatting gates explicit. R01a and R02
acceptance must use this mode, even if a selected-crate development gate passes.

## Corrections

| Review item | Implementation and regression |
| --- | --- |
| Fixture placement / omitted suite | Separate chain/census fixtures from sealed triples; retain the strict triple reader; full-workspace gate selection test. |
| Recovery proof counted as optimization success | `recovery_proof_successes` is separate (legacy absence means zero); rejected chains expose `PRODUCER_ERROR` or the actual checker rejection. Actual apply/history fault injection still replays the recovery certificate but refuses publication and state commit. |
| Inherited 4 MiB scratch limit | Remove `RLIMIT_FSIZE`. A bounded pipe collector caps combined stdout/stderr only, drains both streams, and kills/reaps on overflow, timeout or disk reserve failure. Tests include a grandchild's 8 MiB proof file and a leader exiting with a live pipe-holding descendant. |
| Missing checker package dependency | `evocc` depends on the independent checker library and builds `evocc-checker` as a separate companion binary. `evo-proofcheck` remains the standalone package entry point; both invoke the same checker implementation, with no optimizer dependency. Cargo's package-local tests use `CARGO_BIN_EXE_evocc-checker`. |
| Untrusted adjacent checker | Shared chain admission requires an externally selected expected SHA-256 before execution. Driver flag `--eq-checker-sha256` or `EVOCC_CHECKER_SHA256`; the evaluator consumes the latter from the frozen runner. Merely hashing an observed sibling is not admission. A substituted always-success script must not execute. |
| Cost flag ordering | Reject repeated/conflicting `--eq-policy` and `--eq-weights` in either order. |
| Legacy checker timeout | Legacy `check()` remains unbounded unless its caller explicitly sets a deadline. Verified chains use a recorded bounded deadline; `EVOCC_CHECKER_TIMEOUT_MS` configures evaluator callers, and both Rust and Python enforce 1..300000 ms for admitted chains. |
| Duplicate persistent mutation path | The compatibility API delegates to the certified transaction's mapping/commit path, then discards the certificate view. It does not grant independent proof authority. State/module parity and the terminal no-op are tested. |
| Hyphenated stop reason | Archive audit accepts the actual `no-new-best` budget spelling. The checker still validates the certificate. |
| Implementation commit label | Collector verifies the named commit exists and its tracked/untracked Cargo/crate build inputs match the live source, before and after collection, in addition to frozen binary hashes. This is source/artifact binding supplied by a trusted build manifest, not a reproducible-build proof. |
| Publication error masks rejection | Preserve the primary rejection and append a secondary bundle-publication error; never emit code on either failure. |

Binary trust is intentionally explicit. For a local development build, the
operator may pin the checker they just built from the reviewed source. Release
and campaign users must obtain the digest from their trusted release/build
process; do not compute a new expected digest from an arbitrary adjacent binary
on every compile. This change is not a release-signing system.

## Qualification

Use the existing target tree; serialize Cargo invocations that share it:

```sh
CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true \
  tools/gate-changed.sh --full-workspace --offline
python3 -m unittest tools.ladder.tests.test_eqsat_chain \
  tools.ladder.tests.test_eqsat_census tools.ladder.tests.test_bench \
  tools.ladder.tests.test_todo_schedule tools.ladder.tests.test_gate_changed
```

The isolated package gate passed **560 tests across 58 suite summaries, zero
failures/ignored**, with a fresh temporary Cargo target, incremental compilation
disabled and Rust test/dev debuginfo disabled. The command was
`cargo test --offline -p evocc --no-fail-fast`, with two build jobs. The directory
contained the package-built `evocc-checker` and no `evo-proofcheck`; no preexisting
sibling could supply its dependency. The 407 MiB temporary target was removed
after exit 0; logs and compiler/checker hashes remain in
`.evidence/eqsat-review-20260929-WesLHU/`.

Full-workspace qualification is in progress. Do not infer PASS from focused
gates. No real-application census was run during repair;
the frozen 32-program generated-IR capture remains unchanged and scoped to its
original subject. The three effect-ceiling tracks remain NOT_RUN.

The first full-workspace attempt stopped at a new test's illegal call to a
crate-private method; the test now exercises the next public transaction instead.
The next attempt exposed a new CLI test using positional numbers instead of the
required named weight vector; that fixture was corrected. Neither failed attempt
is an acceptance pass. Their logs are retained with the final qualification.
That full run also exposed the MIR native C-corpus caller: it enabled EqSat
without passing a checker identity, so all 384 source inputs were refused.
`Driver::with_checker_digest` now transfers an explicit caller-selected pin;
both host and Linux corpus fixtures select their workspace-built companion and
record its digest. The helper never automatically trusts a neighboring binary.

The review's duplicate Admission, staged-copy and repeated terminal-interpretation
work remains explicitly tracked under R02a, before timing. It is separate from
the four acceptance blockers. Optimization must not remove checker identity
checks or weaken rollback merely to reduce overhead.
