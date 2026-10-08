# adler-stream-combine

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/zlib/zlib-1.3.2.tar.gz`, SHA-256 `bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`.
Exact local root: `.build/ladder/zlib/zlib-1.3.2/`.
License: zlib; retained unmodified in `../LICENSE.zlib`.

## Function provenance and adaptation

Exact source locations: `adler32.c:63-126 and adler32.c:133-155`.
Transplanted/composed functions: `adler32_z, adler32_combine_`.

Transplant byte-at-a-time, short-buffer, NMAX-block and remaining-buffer accumulation together with the exact two-sum combination formula and negative-length sentinel. Expand DO16 macros into a scalar 16-byte loop, replace MOD macros by unsigned remainder, keep 32-bit checksum words and a nonnegative long-long combination length. This adaptation explicitly treats every null data pointer as checksum initialization before the one-byte fast path. Compose whole-buffer, chunk-stream and split-combined checksums.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Exempt the one-byte fast path from null initialization, reproducing a read through null for seed 11's absent one-byte buffer.
2. Add rather than subtract the reduced second-stream length in the combination correction, corrupting checksum equality with streaming updates.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
