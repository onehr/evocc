# ldm-gear-buckets

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz` (Zstandard 1.5.7), SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`.
Exact local root: `.build/ladder/zstd/zstd-1.5.7/`. License: BSD, selected
from the upstream dual license; retained in `../LICENSE.zstd`.

## Function provenance and adaptation

`lib/compress/zstd_ldm.c`: `ZSTD_ldm_gear_init` (32-63), `ZSTD_ldm_gear_reset`
(65-94), `ZSTD_ldm_gear_feed` (96-133), `ZSTD_ldm_adjustParameters` (135-167),
`ZSTD_ldm_getBucket` and `ZSTD_ldm_insertEntry` (186-206),
`ZSTD_ldm_countBackwardsMatch` (210-224), `ZSTD_ldm_fillHashTable` (285-329)
and the prefix-only path of `ZSTD_ldm_generateSequences_internal` (342-514).
`lib/compress/zstd_compress_internal.h`: `ZSTD_rollingHash_append` and
`ZSTD_rollingHash_compute` (981-1001); `ZSTD_count` (854) as a byte loop.

Adaptation:

- The 256 gear constants come from a SplitMix64 generator, not from
  `zstd_ldm_geartab.h`.
- The Rabin-Karp rolling hash replaces XXH64 for split-point hashes. The low
  `hBits` bits select the bucket. The high 32 bits are the checksum.
- Tables are scaled down: default minimum match 8 (upstream 64), bucket-size
  log 1..3 (upstream 4..8), hash log 4..6, batch size 16. The rate formula
  `7 - strategy / 3` and the btultra halving are unchanged.
- Positions are byte indexes from the buffer start. Offset 0 marks an empty
  entry. Upstream starts its window index above zero for the same purpose.
- The unrolled feed loops become one loop. The separate candidate pass
  (a prefetch stage) merges into the match loop; the order of table updates
  is unchanged.
- `ZSTD_ldm_gear_reset` stores the recomputed rolling hash. The upstream
  function discards it. The model-visible comment states the stored form.
- There is no external dictionary, chunking or window overflow handling.
  Bytes `[0, 40)` only seed the table; matching covers the rest.

The standalone source has no preprocessing, external calls or I/O. Public
`evaluate(seed)` builds a 120..156-byte input with repeated phrases, derives
parameters from the seed through `ZSTD_ldm_adjustParameters`, runs the table
fill and the sequence generator, replays the sequences against the input, and
hashes parameters, sequences, replay errors, table entries and bucket offsets.
`main` evaluates seeds 7 and 2; the interpreter's step limit allows two seeds.
`reference_main.c` evaluates 12 distinct seeds and `program_main`;
`expected.txt` contains the fixed output.

## Injected defects

1. `ZSTD_ldm_gear_init` (line 74) loses the `hashRateLog <= maxBitsInMask`
   condition. The function has two call sites: the table fill and the
   sequence generator. Only a hash rate above the minimum match reaches the
   faulty branch. Seed 2 has strategy 9 (minimum match 4) and an explicit
   hash log that gives rate 5. The mask shift count becomes `4 - 5` as an
   unsigned value. This is an invalid shift (EvoCC E3005). AArch64 masks the
   count, so the stop mask and all split points change.
2. `ZSTD_ldm_generateSequences_internal` (line 266) computes `litLength`
   from the split point instead of the start of the backward-extended match.
   The match length still includes the backward bytes. Every sequence with a
   nonzero backward extension is wrong, and the replay check fails.

Call distance from `main`: `main -> evaluate -> ldm_process ->
ZSTD_ldm_fillHashTable / ZSTD_ldm_generateSequences_internal ->
ZSTD_ldm_gear_init` is four calls. The second defect is three calls away.

Fixing only defect 1 still changes 9 of the 12 hidden seed outputs. Fixing
only defect 2 still changes seed 2 and the `main` result. Fix spans are
original one-based inclusive line ranges of the minimal patch.

## Qualification status

Tool qualification results are in `../qualification.json`. No model preflight
has been run. This item is a spare and is not registered.
