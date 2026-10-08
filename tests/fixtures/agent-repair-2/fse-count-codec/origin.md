# fse-count-codec

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz`, SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`. The source is the repository's
existing pinned ladder checkout. License: BSD, selected from the upstream dual license; retained in ../LICENSE.zstd.

## Function provenance and adaptation

`lib/compress/fse_compress.c`: `FSE_writeNCount_generic` (233-326). `lib/common/entropy_common.c`: `FSE_readNCount_body` (42-185). Both bodies, including zero-run encoding, threshold reduction, and padded short-header decoding, are retained.

Compile-time bounds become constants; byte loops replace memory helpers and scalar bit scans replace intrinsics. The caller owns a zero-padded 64-byte storage region and hashes both decoded counts and wire bytes, as well as actual consumed/written lengths. Input alphabets vary from four to sixteen symbols.

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

1. Underallocate the decoded normalized-count alphabet. Small alphabets fit; the final 16-symbol distribution writes and later reads one entry past the array.
2. Overreport the normalized-count header length by one byte, corrupting the codec framing contract and its composed length result.

The ordinary main calls the shared algorithm for seeds 0, 1, and 2 before the
boundary case 11. Both mutations affect the same composed result. Fix spans
are original, one-based inclusive ranges computed from the minimal patch.

## Qualification status

Construction only. Sealed GCC-14 -O0 buggy/fixed scoring, EvoCC UB diagnosis,
sealed UBSan, hidden-test mutation ablations, and three C0 preflight responses
remain required before registration. No model preflight has been run here.

The workspace is a separate array. The initial unregistered struct layout
allowed a subobject overflow that UBSan reported but EvoCC did not. Keep this
as a selection limit, not an EvoCC coverage claim. The wrapper now also reports
an inconsistent output extent; a native run that does not crash must still
match the correct table result. No compiler or sanitizer rule was changed.
