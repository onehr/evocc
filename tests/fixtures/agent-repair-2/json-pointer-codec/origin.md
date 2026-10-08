# json-pointer-codec

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

Exact source locations: `cJSON_Utils.c:157-196 and cJSON_Utils.c:359-390`.
Transplanted/composed functions: `pointer_encoded_length, encode_string_as_pointer, decode_pointer_inplace`.

Preserve pointer-token escaped-length counting, slash/tilde encoding, and in-place two-cursor decoding. Replace dynamic allocation with a 32-byte token struct and make the terminator part of the capacity contract. Unlike upstream decode_pointer_inplace, the repaired adaptation copies ordinary bytes after compaction and places decoded slash at the write cursor. Roundtrips compose both algorithms and verify equality rather than supplying expected constants.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Compare the encoded length against the whole Token object size instead of its data capacity. Only seed 11 constructs a 40-byte encoded token, which exceeds both the data array and the enclosing object.
2. Restore upstream's wrong slash destination offset in decode_pointer_inplace, writing at write + 1 instead of write and corrupting roundtrips.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

The first unregistered Token layout hid a subobject overflow from EvoCC. A
last-member layout then escaped GCC's default bounds sanitizer. The current
Token has explicit length, data and capacity fields, and the erroneous
whole-object capacity admits a write past the entire object. These selection
limits do not establish coverage of arbitrary subobject or last-member errors.

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
