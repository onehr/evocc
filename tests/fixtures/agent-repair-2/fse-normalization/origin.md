# fse-normalization

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz`, SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`. The source is the repository's
existing pinned ladder checkout. License: BSD, selected from the upstream dual license; retained in ../LICENSE.zstd.

## Function provenance and adaptation

`lib/compress/fse_compress.c`: `FSE_minTableLog`, `FSE_normalizeM2`, and `FSE_normalizeCount` (379-527). Primary fixed-point rounding and the full secondary redistribution algorithm are retained.

Header types/macros become scalar C types/constants. Integer division and high-bit helpers are portable. The wrapper explicitly supports empty histograms and builds paired normalizations at table logs 6 and 7; upstream library code normally expects empty input to be handled by its caller.

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

1. Misclassify the empty histogram in the shared normalization adapter. Only the final all-zero distribution reaches the primary normalizer with a zero divisor.
2. Subtract instead of adding the remaining normalization mass to the largest bucket, changing the normalized distribution shared by both scale choices.

The ordinary main calls the shared algorithm for seeds 0, 1, and 2 before the
boundary case 11. Both mutations affect the same composed result. Fix spans
are original, one-based inclusive ranges computed from the minimal patch.

## Qualification status

Construction only. Sealed GCC-14 -O0 buggy/fixed scoring, EvoCC UB diagnosis,
sealed UBSan, hidden-test mutation ablations, and three C0 preflight responses
remain required before registration. No model preflight has been run here.

The adapter reports UINT_MAX for an empty histogram, which cannot form a
normalization table. Its initial zero-status convention made the UB-only
mutant accidentally match GCC -O0 outputs. The input was not qualified then.
