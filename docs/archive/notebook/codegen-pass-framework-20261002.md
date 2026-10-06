# N02: bounded pass framework and identity validation

Plan: [`917f8955`](../../tests/contracts/codegen-n02-20261002.json).
Implementation: `7680ae23`; bounded framework accepted. Resource plan: `4cbe9be9`.
[Compact receipt](../../tests/contracts/codegen-n02-20261002-receipt.json).

| Evidence | Result |
| --- | --- |
| Initial framework RED | 0 PASS / 7 FAIL |
| Initial driver RED | 2 PASS / 3 FAIL: opt-in flags absent |
| Focused framework / source / driver | 14 / 3 / 5 PASS, zero failures |
| Existing 21,000-program population | 6,274 identity TV admitted; 14,726 opaque and excluded |
| Unchanged EvoIR measurement | 21,000 defined; 3,884,440 steps before and after |
| Existing native-main population | 256 inputs; 72 identity TV / MIR / native comparisons; 184 opaque excluded |
| Small measurement | 257 defined observations; 70 TV admitted / 187 opaque; 52,825 unchanged steps |
| Integer-mix cross-interpreter bridge | **MISMATCH_NOT_VALIDATED**, described below |
| Full workspace | 377 suites; 2,915 PASS / 0 FAIL / 4 ignored; Clippy and formatting PASS |
| Frozen identity resource pair | 64/228-byte objects identical; 10/41 steps unchanged; compile ratios 1.031/1.089 (bound 1.25) |

`evocc-passes` stages each module pass and validates every intermediate output
before publishing the chain. Existing typed observation equality compares
returns, surviving memory and volatile events; diagnostics are separate results.
Unsupported execution and fuel fail closed. The driver exposes only opt-in
`--opt-pass=identity` plus `--pass-report`; default assembly and EvoIR remain
byte-identical. No optimizing pass enters `-O2`, and no size benefit is claimed.
The source bridge compares return values, volatile events and diagnostics;
surviving-memory equality is checked between EvoIR before/after executions,
not established between the two IR interpreters by those three source tests.

The 21,000 measurements are **not** 21,000 TV passes. Opaque modules are measured
unchanged, never transformed or admitted. Native checks reuse the existing
runner, not a second backend harness; their 256-input population is distinct
from the standing 384 C-source gate. N05 consumes compact steps and outcome
hashes while retaining admission and source-bridge status.

**Observed legacy gap.** Unchanged `integer-mix.c` returns zero in both
interpreters, but C-HIR emits 1,001 volatile events and EvoIR 1,002: the latter
includes the local volatile initializer store. The minimal input is
[`volatile-initializer-observation.c`](../../tests/fixtures/codegen/volatile-initializer-observation.c).
C-HIR `store_initialized_value` passes `Qualifiers::default()`; EvoIR
`lower_initializer_at` propagates the destination's volatile qualifier. This
slice changes neither implementation and neither removes nor normalizes away
the extra event. The mismatch remains an open source-bridge issue. Its 11,017
unchanged EvoIR steps are measurement-only, not two-interpreter agreement or TV
acceptance. The successful measurement process retains `MISMATCH_NOT_VALIDATED`.

**Claim ceiling:** execution validation on named admitted inputs, not universal
equivalence, whole-program coverage, an optimization improvement or certified
EqSat. The bounded identity integration does not close the pending bridge gap.
The one-shot resource pair uses two existing supported C inputs, the retained
release binary and S1 object inspection. Four object-twin pairs are identical,
as are the two cross-arm pairs. Three host compile samples per arm meet the
frozen bound on these inputs only; these millisecond observations are not a
program-latency or statistically general performance claim. Six-TU optimizing
pass admission remains a future per-pass obligation, not N02 acceptance.

**Disk:** net retained captures +127,627 and +25,532 bytes plus small sources; native temporary
products use the existing runner's cleanup. Shared driver and Cargo costs are
not separable by lane; the N01 record reports the combined cache checkpoint.
