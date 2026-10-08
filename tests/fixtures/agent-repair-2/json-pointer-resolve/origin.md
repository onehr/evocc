# json-pointer-resolve

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

Exact source locations: `cJSON_Utils.c:120-155 and cJSON_Utils.c:256-342`.
Transplanted/composed functions: `compare_pointers, get_array_item, decode_array_index_from_pointer, get_item_from_pointer`.

Keep JSON Pointer token matching, tilde escapes, leading-zero refusal, linked-array traversal, and recursive path-state iteration. Replace type predicates by explicit caller-owned object/array/number tags, tolower by the ASCII subset, and size_t by a checked bounded unsigned accumulator. Compose ordinal paths with escaped object-key paths and both case-sensitive and insensitive lookup.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Remove the current-element guard when continuing a path; seed 11 first resolves a missing array member and then dereferences null at its following token.
2. Accumulate decimal indices with radix nine; two-digit indices resolve the wrong array element even after the null guard is repaired.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
