# json-unicode-scan

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

Exact source locations: `cJSON.c:661-818 and cJSON.c:819-944`.
Transplanted/composed functions: `parse_hex4, utf16_literal_to_utf8, parse_string`.

Retain hexadecimal nibble accumulation, surrogate validation, backward UTF-8 emission, and string escape scanning. Replace parse_buffer and allocation with explicit lengths and caller-owned output. Invalid hex returns a separate sentinel, so U+0000 remains representable. The public wrapper scans two independently selected strings and probes the same hexadecimal helper with a generated seed-dependent fragment. All twelve API inputs change actual scanner data or hexadecimal fragments, not only result mixing.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Omit the null-input refusal in parse_hex4; seed 11 probes a missing hexadecimal fragment and reads through null.
2. Use an eight-bit rather than ten-bit shift when composing the high surrogate; valid non-BMP input produces the wrong UTF-8 bytes.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
