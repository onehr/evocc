# V1 EqSat size work packages — 2026-10-01

Owner decision: code size is Q2's primary objective; latency is
NOT_MEASURABLE_ON_THIS_LANE. Frozen timing is never rerun or retuned. R03 V1 is
a completed negative-admission study (three tracks INCONCLUSIVE / NOT_ADMITTED),
R03a/b/c/d do not start, and R03e is deferred. EqSat/Lean is the trust anchor for
the reviewed rewrite domains. No unobserved audit feature is added.

Each package first records its RED, then its implementation, focused checks and
`tools/gate-changed.sh --full-workspace`. No DCE or unverified guard-attribution
bytes enter the verified-EqSat numerator. No cross-family averages.

| Package | Contract / input | State and claim ceiling |
| --- | --- | --- |
| S1 | [framing revision 7 §5.5](../framing-draft.md#v1-size-contract-s1-revision-7); `tools/ladder/eqsat_elf.py` | DONE, `04fec0d7`. Object layout metrics only, not dynamic instruction counts or a new Q2 acceptance result. |
| S2 | Same A/B verified IR; common backend DCE | In progress. Existing baseline confirmed; attribution-control RED → GREEN. Measurement and full-workspace pending. |
| S3 | Existing 1,024-evaluation capture, no new evaluation | Pending S2. Size verdict only FINITE_ENUMERATION_SUFFICIENT or NO_OBSERVABLE_EFFECT; does not reopen R03a. |
| S4 | Existing guard forms / known bits, MIR diagnostic first | Pending S3. UNVERIFIED_DIAGNOSTIC is non-installable and never PASS. Stop if its ceiling is below the order of post-S2 EqSat savings; otherwise submit the independent semantic/checker/Lean delta. |
| S5 | Six original TUs and full applications, GCC14/Clang19 `-Os`/`-O2` | Pending S4. Honest size gap, not parity or latency. |
| S6 | G2 vmlinux `.text` alongside R01's two builds | Pending R01; no separate kernel campaign is authorized by this package. |

## S1 — RED and contract

The object-level hazards have focused tests in
`tools/ladder/tests/test_eqsat_elf.py`: treating a real NOP as padding, moving a
mapped pool to `.rodata` and pretending instructions disappeared, incompatible
`.text` flags, equal-sized but different builds, and absent mapping symbols.
Before implementation the new cases failed (missing S1 entry points); after
implementation all seven ELF tests pass. Frozen historical ELF-v1 rendering is
unchanged. The contract explicitly distinguishes mapping-symbol data from proven
literal provenance and retains interior NOPs as static instruction slots.

Acceptance: `tools/gate-changed.sh --full-workspace --offline --locked -j1`
passed all 366 Rust suites: **2,863 PASS / 0 FAIL / 4 ignored**, all-target
Clippy with warnings denied and formatting. Log:
`.evidence/eqsat-size/full-workspace-S1.log`. Python ladder discovery:
**495 PASS / 9 SKIP** (504 tests). The first sandboxed run's existing process
liveness test could not launch `/bin/ps`; its failure is retained. Running the
unchanged suite with process-inspection permission passes, log
`.evidence/eqsat-size/python-S1-unsandboxed.log`. Skipped retained-capture tests
are not claimed as rerun. The focused ELF contract has **7/7 PASS**.

The synthetic partition is 32 text bytes = 16 instruction bytes (4 slots) +
8 inter-function padding + 8 in-text data. Moving that pool gives 24 text bytes
and 8 rodata bytes, still 4 slots and 32 combined bytes. These are contract
fixtures, **not application savings**. Implementation commit: the S1 commit
containing this record: `04fec0d7`.

## S2 — existing common DCE, not a newly added optimization

The reviewed premise that no baseline DCE exists is false in this checkout.
`crates/evocc-mir/src/regalloc.rs::out_of_ssa` already calls the all-edge
`needed_edge_values` analysis and filters `discardable_computation` before
allocation. `native.rs` applies it to both A and B. It preserves every access,
call, assembly, allocation/lifetime/ABI action, FP operation and guard control
flow. Flags producer/consumer bundles remain legal. No new deletion semantics
or EqSat rule is added by S2.

The attribution control disables that existing deletion and retains the
necessary edge transports too. It is available only through the exact-IR
backend adapter; the production driver retains its previous default. The RED
for the missing control/counters fails to compile before implementation; the
focused SSA suite now passes **12/12**, including three dead computations removed
only by the ON arm and a dead-result nonvolatile load preserved in both arms.

The committed [S2 plan](../../tests/contracts/eqsat-size-dce-20261001.json) binds
the original six TUs, A/B chains, existing checker binaries, native tool and
original correctness harnesses. It uses the existing bench lock/Capture/native
pipeline, two complete backend/assembly builds per object and no timing workload.
The IR residue count is direct-unreferenced P0-shape results (not a transitive
dead-subgraph estimate); the MIR residue is the existing demand-analysis count.
Both are producer diagnostics, not independent proof claims. Native text deltas
are measured separately for A/B and checked/unchecked. No DONE or new size-gain
claim precedes the actual capture and full-workspace gate.
