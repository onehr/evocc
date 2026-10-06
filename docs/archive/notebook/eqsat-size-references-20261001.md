# S5 — frozen TU size comparison

Implementation commit `f6b233ca`; contract
[`tests/contracts/eqsat-size-references-20261001.json`](../../tests/contracts/eqsat-size-references-20261001.json).
Full-workspace gate: **369 suites, 2,870 PASS / 0 FAIL / 4 ignored**;
Clippy/fmt green (`.evidence/eqsat-size/full-workspace-S5.log`). S1's seven ELF
contract tests pass. Capture `.evidence/eqsat-size/S5-jj9babiy/receipt.json`,
SHA-256 `616bbfdefb73b762b8ad12cb4fe361eeba76e20f081a653124704b72af01be6a`:
53 commands, 48 reference objects / 24 byte-identical pairs; original A/B
pairs rechecked. The sealed VM was stopped. No timing, links or executions.

Object `.text + .text.*` bytes; common baseline DCE enabled in A and B.
Unchecked is the release-context comparison, checked has a diagnostic contract.

| TU | Eligible roots | A unchecked | B unchecked | A checked | B checked | GCC14 -Os | GCC14 -O2 | Clang19 -Os | Clang19 -O2 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 13 | 128 | 128 | 128 | 128 | 84 | 96 | 96 | 100 |
| zlib deflate | 2,013 | 29,012 | 28,668 | 30,516 | 30,184 | 12,404 | 14,432 | 13,036 | 17,300 |
| zlib inflate | 1,343 | 26,664 | 26,520 | 28,404 | 28,264 | 7,640 | 9,940 | 9,064 | 11,656 |
| zstd compress | 8,363 | 121,652 | 120,592 | 124,024 | 122,972 | 38,240 | 48,760 | 42,584 | 51,816 |
| zstd decompress | 1,935 | 33,440 | 33,052 | 33,688 | 33,300 | 11,892 | 14,492 | 12,624 | 15,220 |
| cJSON | 1,787 | 30,116 | 29,384 | 30,228 | 29,496 | 11,404 | 14,400 | 13,104 | 17,796 |

Every EvoCC byte above is an instruction slot: no classified padding or pool.
GCC -O2 inter-function padding is respectively 0/72/52/416/180/256 bytes;
Clang and GCC -Os have none. Reference literal/in-text data and unclassified
text are zero. The receipt separately records every TU's `.rodata` and static
instruction slots; section migration is not treated as a saving.

Within-family unchecked B excess over GCC -Os: zlib 131.1%–247.1% (worst
inflate), zstd 177.9%–215.4% (worst compress), cJSON 157.7%; over Clang -Os:
119.9%–192.6%, 161.8%–183.2%, 124.2%, respectively. No cross-family average.
All six B objects remain larger than both references; no parity threshold.

**Claim ceiling:** reproducible six-TU size comparison of the frozen original
source/flags. Not whole-application EvoCC coverage, new correctness, latency,
or certified guard savings. Linked application correctness stays at the prior
S2 receipt's explicit mixed-object scope.

**Separate guard row (`595272a2`):** certified checked B text becomes
128 / 29,960 / 28,232 / 121,988 / 33,300 / 29,468 in the same TU order;
unchecked bytes are unchanged. [Guard evidence and boundaries](eqsat-certified-guards-20261001.md).
The reference comparison above remains frozen; no reference or timing rerun.
