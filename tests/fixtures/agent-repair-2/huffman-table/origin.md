# huffman-table

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zlib-1.3.2.tar.gz`, SHA-256 `bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`. The source is the repository's
existing pinned ladder checkout. License: zlib, retained in ../LICENSE.zlib.

## Function provenance and adaptation

`inftrees.c`: `inflate_table` (46-309), including the length counts, canonical sorting, backwards code increments, subtables, and literal/length/distance metadata. The full function body is retained with type/macro substitutions.

Standalone types replace zlib headers and compile-time constants become literals. The corpus wrapper constructs complete bounded code sets, varies requested root bits, initializes table storage, and hashes all produced entries.

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

1. Shorten the symbol-sorting workspace by one entry. Ordinary trees use at most eight entries; the depth-15 complete tree has sixteen symbols and reaches the out-of-bounds write.
2. Change decoded nonzero literals by one, corrupting canonical decode table values while preserving table traversal.

The ordinary main calls the shared algorithm for seeds 0, 1, and 2 before the
boundary case 11. Both mutations affect the same composed result. Fix spans
are original, one-based inclusive ranges computed from the minimal patch.

## Qualification status

Construction only. Sealed GCC-14 -O0 buggy/fixed scoring, EvoCC UB diagnosis,
sealed UBSan, hidden-test mutation ablations, and three C0 preflight responses
remain required before registration. No model preflight has been run here.

The unnecessary initialization of all 852 output entries was removed before
qualification; the builder writes the consumed entries. The previous draft
exhausted EvoCC's default fuel. The workspace is a separate array. The initial unregistered struct layout
allowed a subobject overflow that UBSan reported but EvoCC did not. Keep this
as a selection limit, not an EvoCC coverage claim. The wrapper now also reports
an inconsistent output extent; a native run that does not crash must still
match the correct table result. No compiler or sanitizer rule was changed.
