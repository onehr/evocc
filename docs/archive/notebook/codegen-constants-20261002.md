# N03d C1: opt-in constant materialization

Implementation `5142e0ec`; measurement subject `10124226`; RED `f7cd8f1a`;
EvoMIR 0.57. [Plan](../../../tests/contracts/codegen-n03d-constants-20261002.json),
[receipt](../../../tests/contracts/codegen-n03d-c1-20261002-receipt.json).
`--isel-opt=constants` selects strictly shorter MOVZ/MOVK, MOVN/MOVK or
logical-immediate ORR sequences from generated target tables. Ties retain
legacy spelling. Default OFF; kernel/frame-pointer protections unchanged.

Full workspace: **3,005 PASS / 0 FAIL / 5 ignored**, 390 suites; Clippy,
formatting and regeneration PASS. Nine focused Rust tests and 80 Python
tests pass, including 256 native value/NZCV checks. The gate started at
`5142e0ec`; the later commit only corrected four unexecuted plan labels,
not code/tests. All subsequent captures used clean `10124226`.
Darwin/Linux OFF and ON each execute/agree **384/384**, with identical
object twins. OFF also matches every baseline object and all 24 six-TU
A/B objects. Six captures each have 12 native harness executions and zero
completed proof rejections or bundle failures.

Object `.text` bytes, arm B. Combined means N03f F1/F2/F3 plus C1;
ratios use unchecked combined versus frozen GCC-14 `-O2`.

| TU | OFF | C1 | Combined | Checked OFF → C1 | Ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 128 | 128 → 128 | 1.333 |
| zlib deflate | 28,668 | 28,416 | 25,316 | 30,184 → 29,932 | 1.754 |
| zlib inflate | 26,520 | 26,336 | 25,128 | 28,264 → 28,080 | 2.528 |
| zstd compress | 120,592 | 115,416 | 97,044 | 122,972 → 117,780 | 1.990 |
| zstd decompress | 33,052 | 31,036 | 23,956 | 33,300 → 31,284 | 1.653 |
| cJSON | 29,384 | 29,296 | 24,636 | 29,496 → 29,408 | 1.711 |

C1 deltas are entirely constant materialization; every other category,
rodata, padding and literal-pool count is unchanged. The same deltas apply
over N03f. N01's 23,224-byte category gap was not a C1 promise. C1 benefits
A as well as B (different deltas); neither C1 nor frame savings are verified
EqSat savings. No g0 expansion. Maximum compile-median ratio is 1.061×
(bound 1.25×); all 257 step/output pairs stay identical. Only 70 inputs
are TV-admitted; 187 stay opaque, including the existing integer-mix
source-bridge event mismatch. No interpreter change or default admission.

**Disposition:** C1 opt-in closed; default stays OFF. All five libraries
still miss 1.4×. Remaining selection opportunities, N03c and N07 continue.
This is not full-application, kernel, latency or proved-backend acceptance;
combined frames retain N03f's signed-Darwin-unwind limitation.
**Disk:** retired 1,632 hash-checked idle objects (5,990,792 bytes); exact
historical runtime payloads are no longer retained. Logs, manifests and
receipts remain. Evidence net is +4,817,820 bytes before final TODO rendering;
shared cache growth since the implementation snapshot is 131,883,008
allocated bytes. Free space after cleanup is about 111 GiB.
