# json-parse-reformat

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

`cJSON.c`: the buffer access macros (299-304), `parse_number` (307-408),
`parse_string` (819-946), `buffer_skip_whitespace` (1085-1108),
`cJSON_ParseWithLengthOpts` as called by `cJSON_ParseWithOpts` with
`require_null_terminated` (1126-1219), `parse_value` (1363-1415),
`parse_array` (1492-1587), `parse_object` (1652-1767), `cJSON_New_Item`
(241-250), and the formatted paths of `print_value`, `print_number`,
`print_string_ptr`, `print_array` and `print_object` (1418-1489, 591-658,
949-1068, 1590-1649, 1770-1881).

Adaptation:

- A 40-node pool replaces heap allocation. Keys and strings are inline
  16-byte fields; longer strings are truncated. A failed parse leaves its
  nodes in the pool.
- `parse_number` reads integers only, without `strtod`. It keeps the cJSON
  saturation of `valueint`, but a negative value saturates to -2147483647
  instead of `INT_MIN`.
- `parse_string` accepts the single-character escapes and rejects `\u`.
- `parse_array` and `parse_object` share one loop (`parse_children`), with
  the same whitespace, empty-container and step-back logic. The nesting limit
  is 8 (upstream 1000).
- The printer writes into a fixed buffer and fails when it is full, like
  `cJSON_PrintPreallocated`. It keeps the cJSON formatted layout, including
  `{\n` plus depth-1 tabs for an empty object.
- Function-like macros become small functions; `strncmp` becomes a byte
  comparison.

The standalone source has no preprocessing, external calls or I/O. Public
`evaluate(seed)` parses one of 12 fixed JSON texts and hashes the formatted
output, or returns the parse stop position. The texts vary whitespace
(none, spaces, tabs and newlines), nesting, escapes, boundary integers and
long integers, and one text is malformed. `main` evaluates seeds 1, 4 and 9.
`reference_main.c` evaluates 12 distinct seeds and `program_main`;
`expected.txt` contains the fixed output.

## Injected defects

1. `buffer_skip_whitespace` (line 67) stops at bytes below 32, so a space is
   no longer whitespace. Texts with spaces between tokens (seeds 1, 6, 9 and
   11) then fail to parse or stop at a different position. Tabs and
   newlines still pass.
2. `parse_number` (line 84) loses the saturation guard. A digit run above
   2147483647 then overflows `int` (EvoCC E3004). `parse_number` is reached
   from `parse_value` for top-level values, array elements and object
   members; only a long digit run overflows. Seeds 4 and 7 have one.

Call distance from `main`: `main -> evaluate -> cJSON_Parse ->
buffer_skip_whitespace` is three calls. `main -> evaluate -> cJSON_Parse ->
parse_value -> parse_number` is four calls; the overflowing values sit inside
an object, six calls away.

Fixing only defect 2 still changes 4 of the 12 hidden seed outputs. Fixing
only defect 1 still changes seeds 4 and 7. Fix spans are original one-based
inclusive line ranges of the minimal patch.

## Qualification status

Tool qualification results are in `../qualification.json`. No model preflight
has been run. This item is a spare and is not registered.
