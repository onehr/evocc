# N03d C2: adjacent emission idioms

Implementation and clean measurement subject `5ead5857`; RED `eda405ac`;
EvoMIR 0.58. [Plan](../../../tests/contracts/codegen-n03d-local-20261002.json),
[receipt](../../../tests/contracts/codegen-n03d-c2-20261002-receipt.json).
`--isel-opt=local` composes adjacent, single-use allocated shift/extension,
multiply and indexed-memory sequences. No IR, allocation, interpreter, guard
or proof semantics change. Default OFF; protected profiles unchanged.

Full workspace **3,018 PASS / 0 FAIL / 5 ignored**, 392 suites; Clippy,
formatting and target regeneration PASS. Focused tests: 20 Rust, 83 Python,
142,336 native value/NZCV comparisons and 4,096 real-driver inputs.
Darwin/Linux ON/OFF each execute and agree **384/384**, with identical
object twins. OFF matches all 768 historical corpus hashes and 24 six-TU
A/B hashes. ON changes 48 corpus objects per OS. Six fresh captures each
complete 12 original harness executions, with zero proof rejections or
bundle failures. These are selected TUs plus reference dependencies, not
complete application rebuilds.

Arm B object `.text` bytes. Combined is N03f + C1 + C2; ratio uses unchecked.

| TU | OFF | C2 only | Combined | Checked OFF → C2 | Ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 128 | 128 → 128 | 1.333 |
| zlib deflate | 28,668 | 28,020 | 24,668 | 30,184 → 29,536 | 1.709 |
| zlib inflate | 26,520 | 26,252 | 24,860 | 28,264 → 28,028 | 2.501 |
| zstd compress | 120,592 | 119,676 | 96,128 | 122,972 → 122,048 | 1.971 |
| zstd decompress | 33,052 | 32,460 | 23,364 | 33,300 → 32,708 | 1.612 |
| cJSON | 29,384 | 28,992 | 24,244 | 29,496 → 29,104 | 1.684 |

C2 reductions are in compute and extension categories, identical over the
previous frame+C1 combination. No text or text+rodata growth; padding and
literal-pool accounting retained. They are not P0, DCE or frame gains.
All 257 step/output records and per-arm IR/chain/bundle bindings agree across
options. Only 70 inputs are TV-admitted; 187 remain opaque, and the legacy
source-bridge event mismatch remains unvalidated.

**Disposition: closed with compile-bound miss, not admitted.** Of 48 matched
compile-median comparisons, 47 meet 1.25×; local-only B checked zstd-compress
is **1.7977×** (5.6398 s versus 3.1372 s). All three samples and the negative
result are frozen; no resampling or threshold change. Its cause is not
established. Correctness and size results do not erase this failed gate.
Keep the implementation opt-in; no default/performance admission. All five
libraries still miss 1.4×. Continue N03c and N07 independently; broader N03d
selection remains open. No kernel, latency or proved-backend claim; combined
frames retain signed-Darwin-unwind NOT_VALIDATED.

**Disk:** retired 1,608 idle, hash-checked objects (4,732,824 bytes); exact
historical runtime payloads are no longer retained. Logs/manifests/receipts
remain. Shared-cache growth from the implementation snapshot is 154,308,608
allocated bytes; retained evidence net is +4,186,112 bytes. Scoped cache plus
evidence net is +158,494,720 bytes (excluding tracked source/docs and unrelated
activity); free space is about 101 GiB. Exact scopes are in the receipt.
