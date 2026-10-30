# N01: retained-object size attribution

Plan: [six units](../../../tests/contracts/codegen-n01-20261002.json),
[historical ladder](../../../tests/contracts/codegen-n01-ladder-20261002.json).
The [compact receipt](../../../tests/contracts/codegen-n01-20261002-receipt.json)
contains all 13 categories for each compiler/profile. Implementation: `449d589e`;
the receipt's source baseline is not a clean-build claim.
Focused census/S1 tests: 15 PASS, 0 FAIL. Full workspace: 377 suites,
2,915 PASS / 0 FAIL / 4 ignored; Clippy and formatting PASS. Shared Python: 135 PASS.

| TU | B unchecked `.text` | GCC14-O2 | Ratio | Largest positive category excesses (bytes) |
|---|---:|---:|---:|---|
| integer-mix | 128 | 96 | 1.333 | move 16; frame 12; memory 8 |
| zlib deflate | 28,668 | 14,432 | 1.986 | frame 3,748; constants 2,464; compute 1,852 |
| zlib inflate | 26,520 | 9,940 | 2.668 | stack 6,712; move 2,052; branch 1,904 |
| zstd compress | 120,592 | 48,760 | 2.473 | frame 23,492; constants 13,448; memory 11,272 |
| zstd decompress | 33,052 | 14,492 | 2.281 | frame 8,944; constants 4,104; memory 1,548 |
| cJSON | 29,384 | 14,400 | 2.041 | frame 6,592; constants 1,816; branch 1,656 |

All 42 arm rows (37 distinct objects) retain byte-identical twins and reconcile
exactly to S1 instruction bytes. Checked, unchecked, A, B, certified g0 and the
two references remain separate. Only the control currently meets the 1.4 target;
the five library TUs do not. No cross-family mean is used.

Prioritize N03a's behavior-preserving foundation, then frame/call overhead and
constant/address selection; inflate additionally motivates spill/coalescing
work. EvoCC-local bodies absent from GCC number 3/1/85/24/23 across the library
TUs, motivating a bounded TU-local inliner experiment. These are syntactic
upper estimates, not additive or guaranteed recoverable bytes. Missing local
bodies alone do not prove inlining. Stack accesses are not automatically spills;
shrink-wrapped saves outside the entry/exit pattern are reported separately.
Direct-trap guard attribution is not the whole checked-profile cost.

Additional ladder inventory: 2,210 historical records; 647 classified,
1,282 without recorded object identity, 68 originally not assembled, 203
replaced objects, 10 unclassified/over the frozen per-object limit. These rows
have no retained two-build evidence and are not fresh application acceptance.
Full disassembly is never retained. Superseded development summaries are retired;
only final category/function summaries and their hashes remain. Disk net delta:
approximately +1.81 MiB owned retention, including 1,806,307 capture bytes;
shared Cargo gate checkpoint +4.12 GiB (15.83 to 19.95 GiB), not charged to
each lane again. Capture ceiling 500 MiB. No compiler or semantic change,
new optimization gain, latency result, or full-ladder PASS is claimed.
