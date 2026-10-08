# sequence-delta-index

Written for this corpus. This item is one of the permitted original items;
it is not a transplant and makes no external source-lineage claim.

## Algorithm and full valid input domain

A block-indexed delta codec for zero through 32 arbitrary signed 32-bit
values, with a baseline of zero. Each delta is signed 64-bit (magnitude at most
4,294,967,295), zigzag maps it to an unsigned value below 2^34, and base-128
varints occupy at most five bytes. The 192-byte store therefore exceeds the
160-byte worst case. Every four values get a byte-offset and pre-block baseline.
Get accepts item indices below count; sum accepts half-open bounds
0 <= begin <= end <= count; find scans arbitrary, not necessarily sorted,
sequences for the first value at least a signed 64-bit threshold. Missing values
return count. Range sums fit signed 64-bit. Get/find/sum require an index produced
by a successful build and not subsequently mutated; malformed varints are
rejected by the internal reader rather than shifted by an unbounded count.

The deterministic adapter `evaluate` accepts seeds 0 through 11 inclusive.
Those seeds create twelve distinct data/operation sets within the contract.
The general algorithms are not limited to those twelve sets. Calls to the
adapter outside its documented seed range are unsupported.

## Injected interacting defects

1. Widen only after signed 32-bit subtraction. Nearby values work, but the last scenario crosses from near INT_MAX to near INT_MIN and overflows before the cast.
2. Record each block checkpoint after applying its first value instead of before its first delta. Random access then applies that delta twice, corrupting range sums and threshold search over the same encoded sequence.

Both defects affect the same data structure and composed query results.
`main` combines ordinary scenario 0 and boundary scenario 11. Both call the
shared codec and random-access helpers many times. The initial four-scenario
main exceeded EvoCC's fuel; hidden scoring still uses all twelve scenarios.
`reference_main.c` checks twelve input sets through exact stdout and also
calls `program_main`. Fix spans are original one-based inclusive minimal ranges.

## Qualification status

Construction only: local host checks and expected-output generation do not
establish sealed GCC-14 -O0, EvoCC, sealed UBSan, mutation-ablation, or C0
preflight qualification. All five required conditions remain subject to the
registered experiment's validation and preflight procedure.
