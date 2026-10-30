# R02e — certified guard first slice

Implementation **`871ea379`**, review repair **`3fef1dca`**; [normative contract](../../../spec/evoir.md#812-certified-total-integer-guard-folding),
[frozen plan](../../../tests/contracts/eqsat-size-certified-guards-20261001.json).
Exact scope: seven total predicates, independent `g0.ub.v1`, existing known bits
and all-edge seeds; no signed-shift, P0 expansion or second slice.

Review acceptance: **371 suites, 2,881 PASS / 0 FAIL / 4 existing ignored**,
Clippy/fmt green (`.evidence/eqsat-size/g0-review-full-workspace.log`). Existing Lean
gate green: 62 rules / 36 analysis rows; the seven new A2 transfer rows and
seven guard rules are width-parametric, kernel-only, no Mathlib or `bv_decide`.
Inherited P0 fact-transfer dependencies retain their prior native-backed trust.
All six Rust/Lean groups rerun: **1,487,689 comparisons, zero differences**
(1,325,045 P0 + 162,644 guards). Selected runs preserve other groups and their
metadata. The Lean gate started from a clean tree; its receipt records
`lean_worktree_clean: true`, Lean commit `871ea379`, and actual answers in all
six groups (`.evidence/eqsat-size/g0-review-gate-lean.log`). Producer nonzero
transfer now uses `ones()`, with a stray-value-bit regression. Focused Python
review tests: **18 Lean + 7 status PASS**; original acceptance: **103/103**.

Capture `.evidence/eqsat-size/g0-dbzfkkrm/receipt.json`, SHA-256
`d7192637a21283b4d303d03f0a9d5c3f5bcfc19f9d681b8b0a499e8236d3299d`:
six chains / **10 independently replayed certificates**, zero completed proof
rejections, bundle failures or unknowns; **48 objects / 24 byte-identical pairs**;
16 original mixed-link variants / **24 passing correctness executions**. The
sealed ARM64 VM stopped normally. No timeout, bypass, timing or diagnostic execution.

S1 object `.text + .text.*` bytes, common baseline DCE enabled in both arms:

| TU | Checked B | Certified guard result | Certified saving | Diagnostic ceiling | Retained predicates |
| --- | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 0 | 0 | 0 / 0 |
| deflate | 30,184 | 29,960 | 224 (0.742%) | 872 | 106 / 121 |
| inflate | 28,264 | 28,232 | 32 (0.113%) | 80 | 103 / 105 |
| zstd_compress | 122,972 | 121,988 | 984 (0.800%) | 1,460 | 94 / 159 |
| zstd_decompress | 33,300 | 33,300 | 0 | 148 | 16 / 16 |
| cJSON | 29,496 | 29,468 | 28 (0.095%) | 80 | 4 / 6 |

Four of six TUs change; worst regression is zero. Within-family savings are
zlib 256, zstd 984, cJSON 28 bytes; no cross-family average. All 84 extracted
roots are direct true constants. EvoIR obligations, reasons and effects remain.
Source-authored true selection has **zero baseline delta** here and receives no
certified credit. P0/DCE savings and diagnostic ceilings remain separate frozen
results. Unchecked B/result sizes are identical: 128 / 28,668 / 26,520 / 120,592 /
33,052 / 29,384; `.rodata`, padding and in-text data are unchanged.

**Claim ceiling:** native figures above remain the original frozen capture,
not a new measurement by this receipt repair. The domain-extension pipeline closes end to end for
this first slice and six original TUs. Benefit is below the expected 1–3%; no
admission relaxation follows. Not whole-compiler proof, whole-application EvoCC
coverage, latency, R03 or G2 acceptance. S6 stays with R01's two G2 builds.
