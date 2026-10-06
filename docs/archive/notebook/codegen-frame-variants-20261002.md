# N03f: opt-in frame and call overhead

Implementation `a1b0d7f0`, tested checkpoint `af8687f1`; RED `0771b134` /
`fb12b2d9`, protection amendment `718f155f`, EvoMIR 0.56.
[Plan](../../../tests/contracts/codegen-n03f-20261002.json);
[receipt](../../../tests/contracts/codegen-n03f-20261002-receipt.json).
F1 pairs adjacent saves without moving slots; F2 shares the complete epilogue;
F3 removes only eligible leaf frame records. All default OFF; kernel markers
and explicit frame-pointer requests suppress all three.

Full workspace: **2,997 PASS / 0 FAIL / 5 ignored**, 389 suites including docs;
Clippy, formatting and regeneration PASS. The first gate failed Clippy's test
module placement check; its log remains, and the final checkpoint passed.
OFF: 24/24 six-TU A/B objects and 384/384 objects per native OS match the
baseline. ON: Darwin and Linux each execute/agree 384/384, with identical
object twins; 17 ABI tests pass. Linux's eight variant/PAC combinations pass
unwind, dynamic/aligned frames, varargs and canary exit 0/77. Darwin's four
unsigned combinations pass; signed unwind is **NOT_VALIDATED** because the
Clang reference control itself fails. No kernel acceptance is claimed.

Object `.text` bytes, arm B. F2/F3 are cumulative; ratios are unchecked F3
versus frozen GCC-14 `-O2`. No family averaging.

| TU | OFF | F1 | F2 | F3 | Checked OFF → F3 | Ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 128 | 128 | 128 → 128 | 1.333 |
| zlib deflate | 28,668 | 27,220 | 25,592 | 25,568 | 30,184 → 27,084 | 1.772 |
| zlib inflate | 26,520 | 26,008 | 25,336 | 25,312 | 28,264 → 27,056 | 2.546 |
| zstd compress | 120,592 | 113,812 | 102,880 | 102,220 | 122,972 → 104,600 | 2.096 |
| zstd decompress | 33,052 | 30,172 | 26,140 | 25,972 | 33,300 → 26,220 | 1.792 |
| cJSON | 29,384 | 27,576 | 25,000 | 24,724 | 29,496 → 24,836 | 1.717 |

N01 predicted repeated epilogues first, paired saves second, leaves third;
the cumulative reductions confirm that ranking. Frame-category deltas are
0/−3,384/−1,356/−20,032/−7,848/−5,212; added branch bytes are
0/284/148/1,660/768/552, below one branch per removed return copy. Every other
category is unchanged. Padding/literal pools are zero; rodata is unchanged.
Frame savings apply to A as well as B and are **not verified EqSat savings**.

Each measurement has 12 identical object twins, 12 selected-TU native replays,
zero completed proof rejections/bundle failures and 257 unchanged step/output
pairs. Only 70 interpreter inputs are TV-admitted; 187 remain opaque, and the
existing integer-mix source-bridge event mismatch remains NOT_VALIDATED.
Compile observations are retained, not used as a new timing or default gate.

**Disposition:** close the opt-in slice; propose F1/F2 for a separately gated
default decision and keep F3 off pending the frame/kernel rollout. All five
library TUs still miss 1.4×. Next: N03d, then N03c. This is neither full-app,
default, kernel, latency nor proved-backend acceptance.
**Disk:** retired 1,608 hash-checked idle objects (4,677,064 bytes); exact
historical runtime payloads are no longer retained. Logs/receipts, categories
and failed controls remain. This slice's evidence net is +4,573,269 bytes;
shared cache net is unmeasured (24,287,363,072 allocated bytes after the gate).
