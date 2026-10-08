# huffman-weight-stats

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz`, SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`. The source is the repository's
existing pinned ladder checkout. License: BSD, selected from the upstream dual license; retained in ../LICENSE.zstd.

## Function provenance and adaptation

`lib/common/entropy_common.c`: uncompressed-nibble branch and weight/rank validation of `HUF_readStats_body` (243-306). Weight total, inferred final weight, rank-one validity and result parameters retain the upstream algorithm.

The FSE-compressed weight branch is explicitly unsupported; no substitute claim is made. Rank storage is local and copied out on success so its actual bound is visible to runtime checking. Corpus glue packs uncompressed weights, reconstructs lengths/canonical codes, and includes a malformed high-weight header that the fixed parser refuses.

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

1. Weaken the decoded-weight bound from 12 to 15. The final malformed nibble 15 enters the shared parser and indexes beyond its 13-entry rank array; ordinary valid weights are unaffected.
2. Omit the one-based weight adjustment for the inferred final symbol, corrupting rank statistics and canonical-code reconstruction.

The ordinary main calls the shared algorithm for seeds 0, 1, and 2 before the
boundary case 11. Both mutations affect the same composed result. Fix spans
are original, one-based inclusive ranges computed from the minimal patch.

## Qualification status

Construction only. Sealed GCC-14 -O0 buggy/fixed scoring, EvoCC UB diagnosis,
sealed UBSan, hidden-test mutation ablations, and three C0 preflight responses
remain required before registration. No model preflight has been run here.

The twelve headers are distinct: eleven complete chain trees with two through
twelve leaves, followed by a malformed weight-15 tree. The rank workspace is a
separate array. Output code SIZE_MAX is ordinary weight corruption; SIZE_MAX-1
reports an excessive derived table log. The initial unregistered struct layout
hid the subobject overflow from EvoCC, though UBSan reported it. That miss is
retained as a corpus-selection limit, not a compiler coverage claim.
