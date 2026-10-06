# N07 E2: bounded vendor-model execution

Implementation `060f9065`; plan `542541a9`; RED `e961b1a8` / `cec6a473`;
measurement plans `3105e33f`. [Plan](../../../tests/contracts/codegen-n07-model-20261003.json),
[receipt](../../../tests/contracts/codegen-n07-e2-20261003-receipt.json).
**Closed for the frozen integer slice.** The model is verbatim Arm 2023-03
ASL translated to Sail, pinned at `sail-arm` `1bf2e557`; source spans, hashes
and BSD-3-Clause-Clear license are retained. Sail 0.20.3 generates its runtime;
the project supplies only reset/observation and transport boundaries. No-op
target generation now leaves all eight unchanged outputs' mtimes untouched.
Production code generation, interpreters, registries and defaults are unchanged.

| Gate / lane | Result |
| --- | --- |
| Full workspace | 394 suites; 3,032 PASS / 0 FAIL / 5 existing ignored; Clippy, format, regeneration PASS |
| Focused Python | 51 PASS / 0 FAIL |
| EvoIR / EvoMIR / model / Apple native / diskless QEMU | 2,600 exact results in each lane: 81 op/width shapes plus eight constant controls |
| Model boundary negatives | Unknown instruction and SP use exit 3; partial header/body exit 2 |
| Native regression | Darwin 384/384; Linux 384/384; object twins and E1 full-byte comparisons identical |
| Six TU A/B × checked/unchecked | 24 object pairs identical to E1; 24 original harness executions pass; zero proof rejection/bundle failure |

The model object has 409 words / 1,636 `.text` bytes, including 89 terminal
RET boundaries; the model executes 320 body-word positions, 9,496 dynamic
instructions. No pool, padding or relocation; ELF/Mach-O body words agree.
NZCV starts/remains zero in this finite corpus, not a general state proof.

Unchanged TU `.text` bytes; checked / unchecked shown separately:

| TU | A | B | Migration delta |
| --- | ---: | ---: | ---: |
| integer-mix | 128 / 128 | 128 / 128 | 0 |
| deflate | 30,516 / 29,012 | 30,184 / 28,668 | 0 |
| inflate | 28,404 / 26,664 | 28,264 / 26,520 | 0 |
| zstd compress | 124,024 / 121,652 | 122,972 / 120,592 | 0 |
| zstd decompress | 33,688 / 33,440 | 33,300 / 33,052 | 0 |
| cJSON | 30,228 / 30,116 | 29,496 / 29,384 | 0 |

All six bindings per arm and 257 step records are unchanged. N02 still admits
70 inputs, leaves 187 opaque and retains the integer-mix source-event mismatch.
**Claim ceiling:** finite integer model agreement, not SMT, complete ISA/backend,
memory/FP validation, application/kernel acceptance or size benefit. E3 remains
open and receives its own frozen scope; no timing or default promotion.

**Disk:** retired 24 idle, byte-checked historical E1 TU duplicates plus generated
C/header and a redundant tool archive: 41,510,568 logical bytes. Exact historical
E1 TU runtimes are no longer retained; their logs/receipts remain. Keep 792 current
migration objects, the model runtime and extracted tool cache for E3. Scoped
disk net **at most +164,175,872 bytes**, excluding tracked files/unrelated activity;
the upper bound conservatively gives no credit for historical blocks not measured
before deletion. Free space about 67 GiB; every capture stays below 500 MiB.
