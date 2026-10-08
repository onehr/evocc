# xxhash64-stream

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz`, SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`. The source is the repository's
existing pinned ladder checkout. License: BSD, selected from the upstream dual license; retained in ../LICENSE.zstd.

## Function provenance and adaptation

`lib/common/xxhash.h`: `XXH64_round`, `XXH64_avalanche`, `XXH64_reset`, `XXH64_update`, `XXH64_digest`, `XXH64_mergeRound`, `XXH64_finalize`. Streaming and digest bodies are transplanted, not a newly named substitute algorithm.

Keep only portable scalar streaming with caller-owned state. Replace macro constants, assertions, allocation interfaces, endian/unaligned helpers and memcpy/memset with bounded byte operations. Pointer capacity checks use remaining lengths to avoid manufacturing out-of-object pointers. Portable rotation is extended to permit zero for digest composition. API wrapper varies data, seed, size and chunk partition.

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

1. Remove the existing null-input no-op guard in the update function. Ordinary chunk updates use data; the optional update in seed 11 has a null input.
2. Replace addition of accumulated byte length by XOR in digest finalization. Chunked and single-update calls share the corrupted digest.

The ordinary main calls the shared algorithm for seeds 0, 1, and 2 before the
boundary case 11. Both mutations affect the same composed result. Fix spans
are original, one-based inclusive ranges computed from the minimal patch.

## Qualification status

Construction only. Sealed GCC-14 -O0 buggy/fixed scoring, EvoCC UB diagnosis,
sealed UBSan, hidden-test mutation ablations, and three C0 preflight responses
remain required before registration. No model preflight has been run here.

The rotation retains OR and its explicit zero-count return. An initial
zero-rotation mutation raised UB but GCC -O0 recognized the rotate and returned
the same bits. That mutation failed the hidden-test qualification and was
replaced by omission of the update function's existing null-input guard.
