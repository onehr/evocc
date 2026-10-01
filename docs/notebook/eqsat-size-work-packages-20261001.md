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
| S2 | [DCE attribution plan](../../tests/contracts/eqsat-size-dce-20261001.json), same A/B verified IR; common backend DCE | DONE. Implementation `e625b89c`; measurement closeout is this record's commit. Existing baseline confirmed, not newly credited to EqSat. Final allocated MIR retains the separately disclosed unused-parameter copies. |
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
are measured separately for A/B and checked/unchecked.

Actual capture: `.evidence/eqsat-size/S2-cdgrncjl/receipt.json`, digest
`d5cefa723ec49923d461f6eaecce77b701f1bf3bb9208dbb5e687e6a3b12a7fc`.
There are 96 object builds, **48/48 byte-identical pairs**, 32 linked variants
and **48/48 original correctness executions**. All six A/B chain bindings were
rechecked: **12 verified proofs, zero completed rejections, zero bundle failures,
zero unknowns**; identity chains are not counted as proofs. All ON object sizes
match the earlier retained default-policy objects. No timing experiment ran.

Primary subject-object `.text` bytes (sums only within each family):

| Family | UB profile | A DCE OFF | A DCE ON | B DCE OFF | B DCE ON | B minus A, DCE ON |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| P0 | checked | 132 | 128 | 132 | 128 | 0 |
| P0 | unchecked | 132 | 128 | 132 | 128 | 0 |
| zlib | checked | 71,240 | 58,920 | 71,340 | 58,448 | -472 |
| zlib | unchecked | 70,196 | 55,676 | 70,292 | 55,188 | -488 |
| zstd | checked | 186,600 | 157,712 | 193,164 | 156,272 | -1,440 |
| zstd | unchecked | 185,832 | 155,092 | 192,396 | 153,644 | -1,448 |
| cJSON | checked | 36,808 | 30,228 | 37,044 | 29,496 | -732 |
| cJSON | unchecked | 36,764 | 30,116 | 37,000 | 29,384 | -732 |

OFF minus ON is **baseline DCE attribution**, not verified EqSat savings. The
large B/OFF witness expansion is not a regression of the released pipeline.
Object totals differ from linked text deltas because linking/alignment changes
the denominator. Eligible-root denominators are 13 / 3,356 / 10,298 / 1,787 for
P0 / zlib / zstd / cJSON; changed TUs are 0/1, 2/2, 2/2 and 1/1 respectively.
No changed TU regresses with DCE ON.

| TU | Direct unused P0-shape IR results A / B | Demand-dead pre-SSA MIR A / B (checked) | After SSA A / B | After allocation A / B |
| --- | ---: | ---: | ---: | ---: |
| integer-mix.c | 0 / 0 | 1 / 1 | 0 / 0 | 0 / 0 |
| deflate.c | 82 / 136 | 1,813 / 1,912 | 0 / 0 | 0 / 0 |
| inflate.c | 42 / 62 | 1,208 / 1,244 | 0 / 0 | 1 / 1 |
| zstd_compress.c | 691 / 1,531 | 6,072 / 7,111 | 0 / 0 | 2 / 2 |
| zstd_decompress.c | 106 / 395 | 984 / 1,371 | 0 / 0 | 0 / 0 |
| cJSON.c | 246 / 399 | 1,613 / 1,838 | 0 / 0 | 0 / 0 |

The [allocated-MIR supplement](../../tests/contracts/eqsat-size-dce-residue-20261001.json)
records both profiles. Its 24 assemblies are byte-identical to the actual capture.
The nonzero final counts are newly introduced ABI-entry `Mov` copies for unused
parameters of `inflateUndermine`, `ZSTD_checkBufferStability`, and
`ZSTD_transferSequences_noDelim`, not surviving EqSat witnesses. They are retained
and disclosed; S2 does **not** claim an entirely residue-free backend. The counts
use the existing producer demand analysis, not an independent disassembler or
proof. Memory/FP/ABI effects remain outside its discardable-computation scope.

Acceptance: full workspace **2,864 PASS / 0 FAIL / 4 ignored**, 366 Rust suites,
Clippy with warnings denied and formatting PASS:
`.evidence/eqsat-size/full-workspace-S2.log`. S2's claim ceiling is reproducible
baseline-DCE attribution and tested native correctness for the six mixed-link
subjects; it is not whole-application EvoCC coverage or a new verified domain.
