# checksum-segment-tree

Written for this corpus. This item is one of the permitted original items;
it is not a transplant and makes no external source-lineage claim.

## Algorithm and full valid input domain

A segment tree over zero through 64 bytes, each 0..255. A segment stores
its length and the polynomial checksum obtained by repeatedly multiplying by
two and adding the next byte, modulo 2^32. Concatenation is associative but not
commutative: left.hash * 2^(right.length) + right.hash. Construction uses a
128-node heap, sufficient for the balanced binary tree at this capacity. Point
updates require an existing index and a byte value. Queries accept half-open
bounds 0 <= first <= last <= count, with an empty segment as identity; invalid
bounds are rejected to the same empty result. An independent direct fold audits
all subranges after the composed updates. Unsigned arithmetic deliberately
implements modulo-2^32 checksums; signed overflow is not part of the contract.

The deterministic adapter `evaluate` accepts seeds 0 through 11 inclusive.
Those seeds create twelve distinct data/operation sets within the contract.
The general algorithms are not limited to those twelve sets. Calls to the
adapter outside its documented seed range are unsupported.

## Injected interacting defects

1. Include exponent 32 in the single-word shift fast path of the power helper. Short joins use smaller exponents; a 32-byte right segment reaches a full-width shift instead of modular power zero.
2. Reverse children while joining a partial-range query. The noncommutative polynomial checksum changes ordering even though full stored nodes and lengths remain intact.

Both defects affect the same data structure and composed query results.
`main` calls the join helper on two short segments before a whole-word right
segment. Full range enumeration remains in the hidden twelve scenarios; the
initial full-enumeration main exceeded EvoCC's default execution fuel.
`reference_main.c` checks twelve input sets through exact stdout and also
calls `program_main`. Fix spans are original one-based inclusive minimal ranges.

## Qualification status

Construction only: local host checks and expected-output generation do not
establish sealed GCC-14 -O0, EvoCC, sealed UBSan, mutation-ablation, or C0
preflight qualification. All five required conditions remain subject to the
registered experiment's validation and preflight procedure.

The rejected initial signed-multiply mutation raised UB but had the same
low-word results under GCC -O0. It did not satisfy hidden-test qualification.
