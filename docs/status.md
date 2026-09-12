```text
EvoCC ── (2026-09-12 UTC,EvoIR 1.9.92 / EvoMIR 0.29, checkout b358bf1)
══════════════════════════════════════════════════════════════════════════════════

┌─ F ─────────────────────────┐ ┌─ S ── 17 ────────────────┐
│ 2,798/2,798 ████████ │ │ gate 21,000 0 ████████ │
│ 2,798/2,798 ████████ │ │ evp2 :/epoch/premise ████████ │
│ 2,008/2,798 ██████░░ │ │ , congruence, , ████████ │
│ 6 ; AC/AD/AF │ │ :, │
└──────────────────────────────────┘ │ Lean 55/55 29 ████████ │
                                      │ Rust Lean 1,325,045 0 │
                                      └──────────────────────────────────────────┘
                                        │
                                        ▼
┌─ B ── ,C ────────────────────────────────────────────────────┐
│ EvoMIR, , ████████ 21,000/21,000 lane 0 │
│ , , ████████ │
│ C Darwin 384/384 ████████ clang │
│ C Linux 384/384 ████████ GCC-14 │
│ 11 ████████ │
└──────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─ ── ──────────────────────────────────────────────────────────────────────────────────┐
│ PASS ████████ GCC ,EvoCC , │
│ K1 receipt: EvoCC 76 ; 90/2,799 ░░░░░░░░ │
│ measure-20260907T025031Z: PASS; 2; NOT_PASS: two byte-proof mismatches │
│ fail-closed 8 :E3021 E3025 E3026 E3029 E3030 E3031 E3033 E3034 │
│ E0 (): c-testsuite 209/220 ████████ torture 1269/1692 ██████░░ │
│ Csmith 932/1000, 0;zlib 19/19 zstd 6/44 cjson 0/2 tinycc 8/11 toybox 195/203 musl 1010/1339 │
│ C1 EvoCC ░░░░ C2 vmlinux ░░░░ G1 ░░░░ │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

(,)
Q0 █████████░ 121 ,
Q1 kernel ████░░░░░░ 2,798/2,798, 2,008/2,798, 384/384
Q2 e-graph pipeline ███░░░░░░░ , GCC -O2
Q3 ███░░░░░░░ ,(13 3 )
══════════════════════════════════════════════════════════════════════════════════
```

## Unified benchmark evaluation

Run `20260912T191806.546521Z`; checkout `b358bf1`; selected gates `ERROR`. Run again with `python3 tools/bench.py all`.

Program counts and translation-unit counts are different measures; an incomplete mixed build is not a closed roadmap milestone. Unselected lanes retain their previous reading and compiler identity; they are not remeasured by this run.

| Benchmark | Rung | Status | Validated / total | Measurement | Main blockers |
|---|---|---|---|---|---|
| ctest | E0b | INCOMPLETE | 209/220 programs | 20260909T171506.687331Z (previous) | E3101: 8, E3030: 1, E2102: 1 |
| torture | E0a | INCOMPLETE | 1268/1692 programs | 20260909T215349.472835Z (previous) | E3101: 128, E2201: 22, E2001: 21 |
| csmith | E0c | INCOMPLETE | 932/1000 programs | 20260909T193124.118062Z (previous) | original seeds; unavailable oracles are not passes |
| zlib | E1 | PASS | 19/19 units | 20260907T141450.311159Z (previous) | mixed-link program tests |
| zstd | E2 | INCOMPLETE | 6/44 units | 20260909T074238.247563Z (previous) | E1001: 37, E3101: 1 |
| cjson | E3 | INCOMPLETE | 0/2 units | 20260908T135846.118517Z (previous) | E2001: 1, E3101: 1 |
| tinycc | E5 | INCOMPLETE | 8/11 units | 20260909T132841.817643Z (previous) | E3101: 3 |
| toybox | E6 | INCOMPLETE | 195/203 units | 20260909T132841.817643Z (previous) | E3101: 8 |
| musl | E7 | INCOMPLETE | 1010/1339 units | 20260909T132841.817643Z (previous) | E3101: 216, E1006: 83, E2001: 14 |
| kernel | K1/K5-K7 ramp | INCOMPLETE | 2797/2799 assembled; boot NOT_RUN | 20260912T182636.704003Z (previous) | Compilation only; E3101: 2 |
| mbedtls | E4 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no benchmark adapter yet |
| polybench | E8 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; floating-point rung deferred |
| llvm-test-suite | E9 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; mixed integer/FP suite |
| jq | E10 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; floating-point rung deferred |
| lua | E11 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; floating-point rung deferred |
| sqlite | E12 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; floating-point rung deferred |
| kernel-full | K6 | ERROR | 0/2799 units | 20260912T191806.546521Z | E3101: 41, E3026: 1 |
| kernel-selftest | K7 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no kselftest adapter yet |

Details and commands: [bench receipt](../tests/contracts/bench-receipt.json). Partial failed runs and source-level blockers: [benchmark review](notebook/benchmark-evaluation.md).
