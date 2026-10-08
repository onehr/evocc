# json-tree-compare

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

Exact source locations: `cJSON.c:1936-1967 and cJSON.c:3057-3178`.
Transplanted/composed functions: `get_object_item, cJSON_Compare`.

Preserve masked type comparison, allowed type validation, identity fast path, primitive cases, ordered array recursion, unordered object lookup, and the second object-membership pass required to reject strict subsets. Restrict numeric values to exact integers and external string functions to bounded caller-owned ASCII loops. Build equal, changed, reordered-key, strict-subset, case-sensitive, array-wrapped and malformed-string comparisons without expected-result constants.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Omit the left string's null validation; seed 11 compares a missing text value with an ordinary value and reads through null.
2. Traverse the left object's members again during the reverse membership pass, accepting strict subsets and violating comparison symmetry.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
