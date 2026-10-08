# fse-decode-table

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz`, SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`. The source is the repository's
existing pinned ladder checkout. License: BSD, selected from the upstream dual license; retained in ../LICENSE.zstd.

## Function provenance and adaptation

`lib/common/fse_decompress.c`: `FSE_buildDTable_internal` (58-159), including low-probability placement, sequential 64-bit spread staging, two-way table spreading, and state/bit-count construction.

The table header and workspace are explicit local typed storage rather than type-punned allocations. Capacity is bounded to 64 states and sixteen symbols. Byte-wise writes replace machine-width stores. The wrapper hashes the complete table and performs a guarded deterministic decode-state walk.

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

1. Underallocate the decode table by one entry. All ordinary table-log-5 builds fit; the table-log-6 call writes entry 63 outside the array.
2. Offset every new-state transition by one, breaking FSE decoding state evolution. The walker refuses an out-of-range transition rather than indexing it.

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
