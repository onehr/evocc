# encoding-type-cost

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz` (Zstandard 1.5.7), SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`.
Exact local root: `.build/ladder/zstd/zstd-1.5.7/`. License: BSD, selected
from the upstream dual license; retained in `../LICENSE.zstd`.

## Function provenance and adaptation

`lib/compress/zstd_compress_sequences.c`: `kInverseProbabilityLog256` (21-44),
`ZSTD_entropyCost` (84-98), `ZSTD_crossEntropyCost` (139-154) and
`ZSTD_selectEncodingType` (156-235), both the heuristic and the cost branch.
`lib/compress/hist.c`: `HIST_count_simple` (39-63).
`lib/compress/fse_compress.c`: `FSE_NCountWriteBound` (223-231),
`FSE_minTableLog` (348-355) and `FSE_optimalTableLog_internal` with
`minus = 2` (357-374). `lib/common/zstd_internal.h`: `LL_defaultNorm`,
`ML_defaultNorm` and `OF_defaultNorm` (126-164).

Adaptation:

- `ZSTD_NCountCost` returns `FSE_NCountWriteBound` for the optimal table log.
  It does not normalize and write the header. The value stays in bytes, as
  upstream.
- `ZSTD_fseBitCost` has no FSE compression table here. The previous table is
  kept as normalized counts. The port checks that every present code has a
  nonzero count, then prices the counts with `ZSTD_crossEntropyCost`.
- `normalize_counts` is a short largest-remainder rule. It stands in for
  `FSE_normalizeCount`. It has no low-probability (-1) entries.
- The FSE table log limit is 8 for all three streams (upstream 9 for literal
  and match lengths). Every previous table then fits the 256-entry cost table.
- `ZSTD_highbit32` is a portable loop. Assertions are omitted.
- `ZSTD_selectEncodingType` writes its three cost estimates to `costs`.
- The offset stream allows the default table only when its largest code is at
  most 28, as `ZSTD_entropyCompressSeqStore_internal` does.

The standalone source has no preprocessing, external calls or I/O. Public
`evaluate(seed)` runs three blocks. Each block draws literal-length, offset
and match-length codes, counts them and selects an encoding type per stream.
Repeat state and the previous tables carry across blocks. Seeds 0, 3, 6 and
9 start from a valid dictionary table. The result hashes each type, repeat
mode, cost estimate, largest code and largest count. `main` evaluates seeds 1
and 4; the interpreter's step limit allows two seeds. `reference_main.c`
evaluates 12 distinct seeds and `program_main`; `expected.txt` contains the
fixed output.

## Injected defects

1. `HIST_count_simple` (line 94) stops the largest-count scan before
   `maxSymbolValue`. The trimmed maximum is always a present code, so a stream
   with one distinct code (RLE) reports a largest count of 0. Selection then
   misses the RLE case. In the heuristic strategies the stream becomes basic.
   In the cost strategies (lazy and above) `ZSTD_entropyCost` receives
   `count[s] == total` and reads `kInverseProbabilityLog256[256]` (EvoCC
   E3011). `ZSTD_entropyCost` has one call site, but the stream encoder calls
   it for three streams in each block; only an RLE stream under a cost
   strategy reaches the bad index. Seeds 1, 5, 6 and 8 reach it.
2. `ZSTD_selectEncodingType` (line 221) adds the normalized-count header cost
   in bytes to a cost in bits. The compressed estimate is then too low for
   every cost-strategy block.

Call distance from `main`: `main -> evaluate -> encode_block -> encode_stream
-> HIST_count_simple / ZSTD_selectEncodingType` is four calls. The faulting
read in `ZSTD_entropyCost` is five calls away.

Fixing only defect 1 still changes 7 of the 12 hidden seed outputs. Fixing
only defect 2 still changes 9 of them. Fix spans are original one-based
inclusive line ranges of the minimal patch.

## Qualification status

Tool qualification results are in `../qualification.json`. No model preflight
has been run. This item is a spare and is not registered.
