# sequence-code-stream

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz` (Zstandard 1.5.7), SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`.
Exact local root: `.build/ladder/zstd/zstd-1.5.7/`. License: BSD, selected
from the upstream dual license; retained in `../LICENSE.zstd`.

## Function provenance and adaptation

`lib/compress/zstd_compress_internal.h`: `ZSTD_LLcode` (584-596),
`ZSTD_MLcode` (598-613), the offBase convention (718-726) and
`ZSTD_updateRep` (813-835). `lib/compress/zstd_compress.c`: `ZSTD_seqToCodes`
(2693-2716) and `ZSTD_finalizeOffBase` (6622-6637).
`lib/decompress/zstd_decompress_block.c`: the offset, match-length and
literal-length part of `ZSTD_decodeSequence` (1277-1325), including the
repeat-offset history update. Tables: `LL_bits` and `ML_bits`
(`lib/common/zstd_internal.h` 119-143), `LL_base`, `OF_base` and `ML_base`
(`lib/decompress/zstd_decompress_internal.h` 30-56), and the repeat start
values {1, 4, 8} (`zstd_internal.h` 65).

Adaptation:

- There are no FSE tables. Codes are stored as plain 6-, 5- and 6-bit fields,
  followed by the offset, match-length and literal-length extra bits. A
  forward LSB-first bit writer and reader replace `bitstream.h`.
- `ZSTD_sequenceBits` counts the packed size of one sequence. It follows the
  extra-bit part of `ZSTD_litLengthPrice` and `ZSTD_getMatchPrice`
  (`lib/compress/zstd_opt.c` 295 and 324). It is the second caller of
  `ZSTD_LLcode` and `ZSTD_MLcode`.
- `ZSTD_storeBlock` stores sequences like `ZSTD_storeSeq` and refuses a block
  that would not fit the 96-byte stream. Lengths stay below 0x10000, so the
  long-length escape is not needed.
- `ZSTD_highbit32` is a portable loop. Macros become expressions.
  Assertions and 32-bit long-offset paths are omitted.

The standalone source has no preprocessing, external calls or I/O. Public
`evaluate(seed)` draws 10..15 sequences. Offsets often repeat one of the last
three distinct offsets, and some literal lengths are 0. It encodes and
decodes the sequences twice: once in full, and once without the first
sequence, so the offset history differs. The result hashes the packed size,
the stream bytes, the decode mismatch count and both offset histories.
`main` evaluates seeds 0 and 1; the interpreter's step limit allows two
seeds. `reference_main.c` evaluates 12 distinct seeds and `program_main`;
`expected.txt` contains the fixed output.

## Injected defects

1. `ZSTD_LLcode` (line 74) uses the table up to literal length 64. The table
   has 64 entries, so length 64 reads `LL_Code[64]` (EvoCC E3011). Two
   functions call `ZSTD_LLcode`: `ZSTD_seqToCodes` and `ZSTD_sequenceBits`.
   Only the argument 64 reaches the bad index. Seeds 1 and 3 draw it.
2. `ZSTD_updateRep` (line 107) always moves entry 1 to entry 2. After a
   repeat code that selects entry 1 (code 2, or code 1 with literal length
   0), the encoder history keeps a stale entry 2. The decoder history stays
   correct. Later repeat codes then name different offsets, and the encoder
   misses or misuses repeat codes.

Call distance from `main`: `main -> evaluate -> code_block ->
ZSTD_seqToCodes -> ZSTD_LLcode` is four calls (five through
`ZSTD_storeBlock -> ZSTD_sequenceBits`). `main -> evaluate -> code_block ->
ZSTD_storeBlock -> ZSTD_updateRep` is four calls.

Fixing only defect 1 still changes 5 of the 12 hidden seed outputs. Fixing
only defect 2 still changes seeds 1 and 3 and the `main` result. Fix spans
are original one-based inclusive line ranges of the minimal patch.

## Qualification status

Tool qualification results are in `../qualification.json`. No model preflight
has been run. This item is a spare and is not registered.
