# json-array-edit

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

Exact source locations: `cJSON.c:1907-1925, cJSON.c:2012-2043, and cJSON.c:2243-2402`.
Transplanted/composed functions: `add_item_to_array, get_array_item, cJSON_DetachItemViaPointer, cJSON_InsertItemInArray, cJSON_ReplaceItemViaPointer`.

Preserve cJSON's intrusive list convention in which the first child's prev stores the tail. Transplant append, detach, indexed insert, and replacement bodies with allocation/deletion removed in favor of caller-owned live Item objects. Compose edits, forward hashes, backward-link checks, tail checks, and node counts. All nodes remain live for the whole call, so a wrong tail is a logic error rather than an accidental freed-memory fault.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Remove detach's null-item guard; only seed 11 detaches a missing ordinal and dereferences null while inspecting its prev link.
2. Retain the detached tail as the first child's tail pointer, disconnecting the subsequent appended member and corrupting later edits.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
