# json-tree-duplicate

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

Exact source locations: `cJSON.c:188-206, cJSON.c:241-255, and cJSON.c:2774-2855`.
Transplanted/composed functions: `cJSON_Duplicate_rec, cJSON_New_Item, cJSON_strdup`.

Transplant recursive duplication, field copying, reference-bit removal, child recursion, sibling crosswiring and head/tail prev restoration. Replace heap allocation and deletion with a checked 16-node monotonic pool plus checkpoint rollback on failure; strings are bounded inline copies. Source nodes remain live and mutate after cloning, so deep ownership and no aliasing affect hashes. Recurse-off and recurse-on call the same duplication body.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Permit allocation at pool index 16; seed 11 builds a 17-node tree and the seventeenth allocation writes beyond the inline node array.
2. Toggle rather than clear the cJSON reference flag, marking copies of ordinary owned nodes as references.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
