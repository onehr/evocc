```text
EvoCC ── (2026-09-08 UTC,EvoIR 1.9.79 / EvoMIR 0.22, checkout 91158d1)
═════════════════════════════════════════════════════════════════════════════════

┌─ F ─────────────────────────┐ ┌─ S ── 17 ───────────────┐
│ 2,798/2,798 ████████ │ │ gate 21,000 0 ████████ │
│ 2,798/2,798 ████████ │ │ evp2 :/epoch/premise ████████ │
│ 52/2,798 ░░░░░░░░ │ │ , congruence, , ████████ │
│ 6 ; AC/AD/AF │ │ :, │
└──────────────────────────────────┘  └─────────────────────────────────────────┘
                                        │
                                        ▼
┌─ B ── ,C ───────────────────────────────────────────────────┐
│ EvoMIR, , ████████ 21,000/21,000 lane 0 │
│ , , ████████ │
│ C Darwin 384/384 ████████ clang │
│ C Linux 384/384 ████████ GCC-14 │
│ 11 ████████ │
└─────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─ ── ──────────────────────────────────────────────────────────────────────────────────┐
│ PASS ████████ GCC ,EvoCC , │
│ K1 receipt: EvoCC 41 ; 52/2,799 ░░░░░░░░ │
│ measure-20260907T025031Z: PASS; 2; NOT_PASS: two byte-proof mismatches │
│ fail-closed 8 :E3021 E3025 E3026 E3029 E3030 E3031 E3033 E3034 │
│ E0 c-testsuite 206/220 ███████░ torture 1196/1692 ██████░░ │
│ Csmith 932/1000, 0;zlib 19/19 zstd 2/44 cjson 0/2 tinycc 4/11 toybox 195/203 musl 494/1339 │
│ C1 EvoCC ░░░░ C2 vmlinux ░░░░ G1 ░░░░ │
└───────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

(,)
Q0 █████████░ 101 , 40 ,0
Q1 kernel ████░░░░░░ 2,798/2,798, 52/2,798, 384/384
Q2 e-graph pipeline ███░░░░░░░ , GCC -O2
Q3 ███░░░░░░░ ,(13 3 )
═════════════════════════════════════════════════════════════════════════════════
```

## Unified benchmark evaluation

Run `20260908T090630.550681Z`; checkout `91158d1`; selected gates `INCOMPLETE`. Run again with `python3 tools/bench.py all`.

Program counts and translation-unit counts are different measures; an incomplete mixed build is not a closed roadmap milestone. Unselected lanes retain their previous reading and compiler identity; they are not remeasured by this run.

| Benchmark | Rung | Status | Validated / total | Measurement | Main blockers |
|---|---|---|---|---|---|
| ctest | E0b | INCOMPLETE | 206/220 programs | 20260908T090630.550681Z | E3101: 7, E2202: 2, E3030: 1 |
| torture | E0a | INCOMPLETE | 1196/1692 programs | 20260908T090630.550681Z | E3101: 132, E1001: 108, E2001: 42 |
| csmith | E0c | INCOMPLETE | 932/1000 programs | 20260908T080325.213428Z (previous) | original seeds; unavailable oracles are not passes |
| zlib | E1 | PASS | 19/19 units | 20260907T141450.311159Z (previous) | mixed-link program tests |
| zstd | E2 | INCOMPLETE | 2/44 units | 20260907T141450.311159Z (previous) | E1001: 38, E2921: 4 |
| cjson | E3 | VALIDATION_GAP | 0/2 units | 20260907T141450.311159Z (previous) | E1001: 2 |
| tinycc | E5 | INCOMPLETE | 4/11 units | 20260908T061410.809670Z (previous) | E3101: 2, E0113: 1, E2921: 1 |
| toybox | E6 | INCOMPLETE | 195/203 units | 20260908T080325.213428Z (previous) | E3101: 8 |
| musl | E7 | INCOMPLETE | 494/1339 units | 20260908T042040.192297Z (previous) | E3101: 240, E1006: 83, E2001: 14 |
| kernel | K1/K5-K7 ramp | ERROR | unavailable | 20260908T030624.773880Z (previous) | receipt unchanged; previous evidence is not a new measurement |
| mbedtls | E4 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no benchmark adapter yet |
| polybench | E8 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; floating-point rung deferred |
| llvm-test-suite | E9 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; mixed integer/FP suite |
| jq | E10 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; floating-point rung deferred |
| lua | E11 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; floating-point rung deferred |
| sqlite | E12 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no adapter; floating-point rung deferred |
| kernel-full | K6 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | K1 measures mixed link, not a fully EvoCC-built kernel |
| kernel-selftest | K7 | NOT_IMPLEMENTED | not measured | 20260907T141450.311159Z (previous) | no kselftest adapter yet |

Details and commands: [bench receipt](../tests/contracts/bench-receipt.json). Partial failed runs and source-level blockers: [benchmark review](notebook/benchmark-evaluation.md).
