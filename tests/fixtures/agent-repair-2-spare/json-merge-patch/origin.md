# json-merge-patch

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

`cJSON_Utils.c`: `merge_patch` (1321-1379) and the case rule of
`compare_strings` (83-109). `cJSON.c`: `cJSON_New_Item` (241-250),
`get_object_item` (1936-1966), `suffix_object` (1984-1988),
`add_item_to_array` (2012-2043), `add_item_to_object` (2067-2102),
`cJSON_DetachItemViaPointer` (2243-2277), `cJSON_DetachItemFromObject` and
its case-sensitive form (2294-2306), `cJSON_IsNull` (2997-3005),
`cJSON_IsObject` (3037-3045), `cJSON_Duplicate_rec` (2774-2858) and the
compact paths of `print_value`, `print_array` and `print_object` (1418-1489,
1590-1649, 1770-1881).

Adaptation:

- A 64-node pool replaces heap allocation. `cJSON_Delete` becomes a no-op:
  detached and replaced nodes stay in the pool unused.
- Keys and string values are inline 8-byte fields, copied with truncation.
- Numbers are `int` only. Printing is compact; strings print without escape
  handling, because all keys and values are ASCII letters.
- `merge_patch` takes the pool and returns null when the pool is exhausted.
  The upstream case-sensitive and case-insensitive detach calls become one
  helper with a flag. The two type predicates are single expressions.
- `print_children` merges the compact array and object printers.

The standalone source has no preprocessing, external calls or I/O. Public
`evaluate(seed)` builds a target object
`{"id","tags":[..],"meta":{"v","w"},"name"[,"Zip"]}` and a patch whose
members come from the bits of a seed-derived mask. The patch can replace,
delete, add and merge members, and differs in key case. Even seeds compare
keys strictly; odd seeds ignore ASCII case. The patch is merged into the
target and then again into the result. The result hashes both printed
documents and the pool use. `main` evaluates seeds 4 and 8; the interpreter's
step limit allows two seeds. `reference_main.c` evaluates 12 distinct seeds
and `program_main`; `expected.txt` contains the fixed output.

## Injected defects

1. `cJSON_IsObject` (line 74) loses its null check. `merge_patch` calls it
   for the patch and for the target, at several call sites through recursion.
   A patch member whose value is an object, for a key that the target lacks,
   detaches nothing and recurses with a null target. That call reads through
   a null pointer (EvoCC E3009). Seeds 3 and 8 add the new object member
   `"geo"`; existing nested objects such as `"meta"` do not reach it.
   With defect 2 also present, members lost from the list miss the lookup
   too, so seeds 1, 2, 3, 4, 7, 8 and 11 reach the null access.
2. `cJSON_DetachItemViaPointer` (line 136) tests `item->prev == 0` instead
   of `item->next == 0`. Detaching the last child then leaves the head's
   `prev` on the detached node. The next append links after that node, so the
   appended member and every later one disappear from the printed document.
   Patching or deleting the current last member reaches this path.

Call distance from `main`: `main -> evaluate -> merge_patch ->
cJSON_IsObject` is three calls, and the faulting call is four calls away
through the recursive `merge_patch`. `main -> evaluate -> merge_patch ->
detach_item_from_object -> cJSON_DetachItemViaPointer` is four calls.

Fixing only defect 1 still changes 10 of the 12 hidden seed outputs. Fixing
only defect 2 still ends the hidden run with a null access on seed 3. Fix
spans are original one-based inclusive line ranges of the minimal patch.

## Qualification status

Tool qualification results are in `../qualification.json`. No model preflight
has been run. This item is a spare and is not registered.
