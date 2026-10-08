# frame-block-walk

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz` (Zstandard 1.5.7), SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`.
Exact local root: `.build/ladder/zstd/zstd-1.5.7/`. License: BSD, selected
from the upstream dual license; retained in `../LICENSE.zstd`.

## Function provenance and adaptation

`lib/decompress/zstd_decompress.c`: `ZSTD_frameHeaderSize_internal`
(416-429), `ZSTD_getFrameHeader_advanced` (447-551), `readSkippableFrameSize`
(587-601), `ZSTD_copyRawBlock` (896-907), `ZSTD_setRleBlock` (909-920),
`ZSTD_decompressFrame` (953-1066) and the frame loop of
`ZSTD_decompressMultiFrame` (1068-1169).
`lib/decompress/zstd_decompress_block.c`: `ZSTD_getcBlockSize` (63-77).
`lib/compress/zstd_compress.c`: `ZSTD_writeFrameHeader` (4695-4743) and
`ZSTD_writeSkippableFrame` (4751-4763).
`lib/compress/zstd_compress_internal.h`: the block headers of
`ZSTD_noCompressBlock` and `ZSTD_rleCompressBlock` (646-670), merged into
`ZSTD_writeBlock`. `lib/common/zstd_internal.h`: `ZSTD_fcs_fieldSize` and
`ZSTD_did_fieldSize` (79-80).

Adaptation:

- Only raw and RLE blocks exist. A compressed or reserved block is an error.
- The decoder writes into the file-scope array `g_regen` by position. The
  128-byte array holds the decoded bytes of all frames, back to back. The
  remaining capacity is `REGEN_CAPACITY - op`, as `oend - op` upstream.
- The content checksum is FNV-1a of the frame content. It stands in for the
  low 32 bits of XXH64.
- Error codes are small negative `size_t` values. There is no dictionary,
  legacy format, magicless format, streaming API or in-place guard.
- Frame-header fields that the port does not use (block size limit, frame
  type) are dropped. The decoder mixes the window size, dictionary ID and
  skippable-frame variants into a running hash.
- Byte loops replace `memcpy`, `memmove` and `memset`.

The standalone source has no preprocessing, external calls or I/O. Public
`evaluate(seed)` writes two to four frames and one skippable frame. Each
frame has one to three segments; a run becomes an RLE block and other bytes
a raw block. Frame parameters vary the content-size flag, checksum flag,
window log and dictionary ID. Then the frames are decoded. The result hashes
the decoder metadata, the stream size, the written content size, and either
the error code or the decoded size and bytes. `main` evaluates seeds 4 and
10; the interpreter's step limit allows two seeds. `reference_main.c`
evaluates 12 distinct seeds and `program_main`; `expected.txt` contains the
fixed output.

## Injected defects

1. `ZSTD_frameHeaderSize_internal` (line 134) adds the extra content-size
   byte only when `fcsId` is nonzero. A single-segment frame with
   content-size code 0 has a 1-byte content size, so its header is one byte
   short. The first block header is then read one byte early.
2. `ZSTD_decompressFrame` (line 268) passes the whole capacity, not the
   remaining capacity, to `ZSTD_setRleBlock`. Raw blocks still get the
   remaining capacity. An RLE block that crosses the end of `g_regen` then
   passes the size check and writes past the array (EvoCC E3011).
   `ZSTD_setRleBlock` runs for every RLE block; only an RLE block that
   crosses the end reaches the bad index. Seeds 5 and 10 have such a block.
   Their frames before that block have no content-size field, so defect 1
   does not stop them first.

Call distance from `main`: `main -> evaluate -> ZSTD_decompressMultiFrame ->
ZSTD_decompressFrame` is three calls; `ZSTD_frameHeaderSize_internal` and
the faulting write in `ZSTD_setRleBlock` are four calls away.

Fixing only defect 2 still changes 8 of the 12 hidden seed outputs. Fixing
only defect 1 still changes seeds 5 and 10 and the `main` result. Fix spans
are original one-based inclusive line ranges of the minimal patch.

## Qualification status

Tool qualification results are in `../qualification.json`. No model preflight
has been run. This item is a spare and is not registered.
