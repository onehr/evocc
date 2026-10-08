# bitstream-codec

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz`, SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`. The source is the repository's
existing pinned ladder checkout. License: BSD, selected from the upstream dual license; retained in ../LICENSE.zstd.

## Function provenance and adaptation

`lib/common/bitstream.h`: `BIT_initCStream`, `BIT_addBits`, `BIT_addBitsFast`, `BIT_flushBits`, `BIT_closeCStream`, `BIT_initDStream`, `BIT_skipBits`, `BIT_readBits`, `BIT_reloadDStream_internal`, `BIT_reloadDStream`, and `BIT_endOfDStream`. The scalar 64-bit path is retained.

Portable 64-bit byte loads/stores and explicit bit masks replace intrinsics and macro tables; compile-time fallthrough markers and assertions are omitted. The adapter checks capacity before appending fields, writes a backward-readable stream, then reloads/decodes all fields in reverse order. Bounds are limited to a 128-byte buffer and field widths at most 16.

The adapter's valid input domain is an unsigned seed from 0 through 11 inclusive.
Each seed denotes the fully specified data/length/alphabet/partition construction
in `evaluate`; calls outside this bounded corpus-adapter domain are unsupported.
The transplanted helper preconditions remain as documented above.

The standalone input has no preprocessing or external calls. The public
`evaluate(seed)` composes the algorithm into deterministic input cases.
`reference_main.c` supplies 12 distinct seeds and also calls `program_main`;
`expected.txt` records the fixed program's exact output. These are construction
oracles, not evidence of GCC-14 qualification.

## Injected defects

1. Relax the shared pre-append capacity check at exact word width. Ordinary widths in main never fill 64 bits exactly, while width 16 on the final path reaches a full container and flushes by shifting it 64 bits.
2. Write a zero end marker instead of one, breaking backward-stream framing even when all payload fields fit.

The ordinary main calls the shared algorithm for seeds 0, 1, and 2 before the
boundary case 11. Both mutations affect the same composed result. Fix spans
are original, one-based inclusive ranges computed from the minimal patch.

## Qualification status

Construction only. Sealed GCC-14 -O0 buggy/fixed scoring, EvoCC UB diagnosis,
sealed UBSan, hidden-test mutation ablations, and three C0 preflight responses
remain required before registration. No model preflight has been run here.
