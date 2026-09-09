# Reviewed original-program applicability

Authority: architecture review of 2026-09-09. Original program identities and
raw compilation/execution outcomes remain in the benchmark; a classification
does not turn a refusal into PASS. The per-program contract binds these
decisions to the exact source digest and requires the evidence files to exist.

## c-testsuite floating execution

Source pin: `5c7275656d75`, `tests/single-exec`. These seven originals require
floating execution on a reached path, not merely a floating declaration.

| Original | Reached floating operation |
|---|---|
| 00113 | Converts `a + 1` to float and compares the stored float against `a`. |
| 00119 | Reads initialized double `x` and compares it with 1. |
| 00123 | Reads initialized double `x` and compares it with 1. |
| 00174 | Executes floating arithmetic, conversions, formatted output and `sin`. |
| 00175 | Calls a function with floating arguments/results and formats the results. |
| 00195 | Stores and reads double members and passes them to formatted output. |
| 00204 | `main` reaches `arg` and `ret`, passing and returning float/double/long-double HFAs and printing their members; it also exercises floating varargs. |

The source files themselves are separately listed as evidence in
`tests/contracts/benchmark-program-dispositions.json`. Their original source
digests must match before the benchmark can apply FP-deferred.

The other six refusals in capture `.evidence/ladder/e0/20260909T062900Z`
are **not** assigned FP-deferred: 00140 passes an aggregate with an unused
float member but accesses only integer members; 00144 is a pointer-conversion
and uninitialized-value case; 00178 only takes `sizeof` of an uninitialized
double; 00207 requires VLA semantics; 00216 requires aggregate-initializer
analysis; 00220 requires the hosted wide-character representation.
They remain compiler failures until repaired or individually supported by
different applicability evidence. A search for the word `float` is not a
classification rule.

## Explicitly excluded GNU function extensions

The architecture review excludes GNU nested functions, rather than weakening
ordinary function/label checking. Source inspection of the following 24
originals confirms an actual definition inside another function, including
the six cross-function label uses and the multi-label declaration case:

`20000822-1`, `20010605-1`, `20030501-1`, `20040520-1`, `20061220-1`,
`20090219-1`, `920415-1`, `920428-2`, `920612-2`, `920721-4`, `921215-1`,
`931002-1`, `nest-align-1`, `nest-stdar-1`, `nestfunc-1`, `nestfunc-2`,
`nestfunc-3`, `nestfunc-5`, `nestfunc-6`, `nestfunc-7`, `pr103405`, `pr24135`,
`pr51447`, `pr71494`.

`nestfunc-4` is deliberately absent: a filename is not a construct and its
passing original result is retained. The record for `nest-stdar-1` is the
explicit nested-function exclusion, not a second FP label.

The same review explicitly excludes these builtin contracts:

| Originals | Required out-of-profile construct |
|---|---|
| built-in-setjmp, pr60003, pr64242 | Compiler builtin setjmp/longjmp context and nonlocal return. |
| pr47237 | `__builtin_apply` and `__builtin_apply_args`. |
| va-arg-pack-1 | Inline forwarding with `__builtin_va_arg_pack`. |
| pr105777, pr105984, pr30314 | `__builtin_mul_overflow_p`, including the two pinned gcc.dg sibling includes. |
| 20001111-1, pr103052 | `__builtin_exit`, distinct from ordinary supported library calls. |

The exact original sources and relevant included sibling sources accompany
each contract entry. Existing raw refusals are recorded by
`.evidence/ladder/e0/20260909T062923Z/results.jsonl`; the classification
applies to those identities, not every diagnostic with the same code.

`991014-1` is the separately reviewed extreme-type oracle conflict:
GCC-14 admits its huge array types and Clang-19 refuses them. Its structure
size is still below PTRDIFF_MAX; it must not be described as an overflow of
that limit. The probe in `20260909-large-layout/results.txt` supports the
oracle-conflict disposition, not an invented smaller numeric layout limit.

## New refusals of formerly passing originals

Capture `.evidence/ladder/e0/20260909T074333Z` has six fewer raw torture
PASS results than `20260909T062923Z`. The loss is retained and is not counted
as a compiler improvement. The review's 15:30 supplement narrows the earlier
blanket wide-field refusal: the first three remain compiler failures pending
shape-specific reduction and repair, not new inapplicability entries.

| Originals | Source-level reviewed boundary |
|---|---|
| 991118-1, bf64-1 | Compound arithmetic on 52/53-bit fields; OPEN under the narrowed conflict-only rule. |
| bf-sign-2 | Arithmetic on an unsigned 35-bit field; OPEN under the narrowed conflict-only rule. |
| pr86659-1, pr86659-2, pr87623 | Explicit `scalar_storage_order`, whose presence must be refused rather than silently ignored. |

Both oracles at O0/O2 pass **all 24 executions** of these original programs;
Clang warns that it ignores the storage-order attribute. Evidence:
`.evidence/builtin-probes/20260909-wide-field-conflicts/additional-originals.txt`.
These are not six newly observed oracle conflicts. Only the three explicit
storage-order attributes remain reviewed-profile exclusions; the other three
must not be relabelled to hide the over-broad refusal. New classification
entries apply on the next actual replay, not retroactively to this sealed
run's counts or raw outcomes.
