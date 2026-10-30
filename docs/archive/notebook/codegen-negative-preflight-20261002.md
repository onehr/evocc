# N03b/N03e: bounded negative preflight

Implementation: `018aefec`; plans frozen in `449d589e`.
Receipts: [original](../../../tests/contracts/codegen-n03-preflight-20261002-receipt.json),
[closure](../../../tests/contracts/codegen-n03-preflight-closure-20261002-receipt.json).
Disposition: **CLOSED_NEGATIVE_FIRST_SLICES / NO_ADMITTED_OPPORTUNITY**.

The read-only census inspected the frozen 21,000 generated modules and fresh
original-source A/current-EqSat-B outputs for the six S5 units. All six source
inventories reproduced the original A bytes. Existing B chains replayed 12
transactions: zero completed rejection, bundle failure or unknown outcome.
No transformation was implemented or executed.

| TU | Inline calls A=B | Integer candidates A/B | Scan-cap operator exclusions A=B |
| --- | ---: | ---: | ---: |
| integer-mix | 0 | 3/3 | 0 |
| zlib deflate | 123 | 414/413 | 99 |
| zlib inflate | 60 | 281/281 | 247 |
| zstd compress | 576 | 841/819 | 162 |
| zstd decompress | 200 | 227/227 | 64 |
| cJSON | 215 | 184/183 | 0 |
| Total | 1,174 | 1,950/1,926 | 572 |

N03b found 81 internal strong single-call sites per arm. Three passed the leaf
shape, all in zstd-compress; all three failed the frozen module-poison
condition. Another 55 sites exceeded callee block/instruction caps. No site
was excluded by the growth cap. N03e checked 63/51 edge bindings in A/B and
found no complete all-edge match. The 572 excluded operators per arm are not
572 functions, and their exclusion does not prove absence of opportunities.
All six complete source modules remain N02-opaque.

The generated population admitted 6,274 defined inputs and excluded 14,726
opaque modules. Its 32,406 calls have no C declaration-chain permission;
unknown permission is not absence of inlining opportunity. Its 954,563
integer candidates did not satisfy the frozen edge pattern. Generated
residual-B inspection was NOT RUN.

The frozen stop rules therefore allow negative closure without widening
opacity, poison handling, operators or budgets. This does not establish
that general inlining or CSE is unprofitable. Transform TV, native
differentials, size/resource bounds and transformed-object twins are
NOT RUN; no pass is admitted to default O2. The existing volatile-initializer
source-bridge mismatch remains MISMATCH_NOT_VALIDATED.

Gates: focused 11/11 PASS; the original 2,938-test run failed Clippy and stays
in the original receipt. The complete recheck at `aab201fd` passed 2,986 tests,
0 failed, 4 ignored, plus Clippy, format and regeneration. At `02ed98ef`, the
capture hash still matches and `evocc-passes` is unchanged since `018aefec`.
This closes the frozen negative slices; broader inlining/CSE needs a new
design, not a weakened preflight. No measurement was rerun.
Disk: original capture +75,377 apparent bytes; closure adds only compact
receipt/docs, no runtime payload. Joint preparation cache growth remains
accounted once in the N03a baseline receipt.
