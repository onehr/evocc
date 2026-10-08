# json-member-sort

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

Exact source locations: `cJSON_Utils.c:83-109 and cJSON_Utils.c:484-601`.
Transplanted/composed functions: `compare_strings, sort_list, sort_object`.

Transplant the linked-list mergesort, already-sorted fast path, slow/fast midpoint walk, split, merge and prev-pointer restoration. Replace strcmp/tolower with ASCII loops. This adaptation defines missing names to sort last and equal keys to retain insertion order, extending the upstream comparator and changing merge ties to stable selection. Two independently constructed lists, repeated sorting, backward links, identities and key order all contribute to the result.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Omit the missing-right-name comparator guard; seed 11 supplies one unnamed member, causing comparison to read through null.
2. Restore upstream's right-biased strict merge tie selection, violating this adaptation's stable-key-order contract.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
