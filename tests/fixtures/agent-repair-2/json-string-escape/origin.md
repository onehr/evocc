# json-string-escape

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

Exact source locations: `cJSON.c:485-581 and cJSON.c:949-1068`.
Transplanted/composed functions: `print_string_ptr, ensure`.

Keep the two-pass escape-counting and output-pointer loops, including the no-escape fast path and null-to-empty-string convention. Replace printbuffer reallocation with a bounded inline buffer and replace sprintf of control-byte Unicode escapes with the same four hexadecimal digits emitted explicitly. Wrapper inputs exercise escaped quotes, backslashes, newlines, generic control bytes, successful printing and insufficient capacity.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Omit print_string_ptr's null-to-empty-string normalization; seed 11 takes the shared printer through a null input and reads through it.
2. Emit the high hexadecimal nibble twice for generic control bytes, corrupting JSON Unicode escape values while preserving output lengths.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
