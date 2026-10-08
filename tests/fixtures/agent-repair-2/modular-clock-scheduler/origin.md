# modular-clock-scheduler

Written for this corpus. This item is one of the permitted original items;
it is not a transplant and makes no external source-lineage claim.

## Algorithm and full valid input domain

An eight-slot periodic scheduler over the ring of 32-bit unsigned ticks.
IDs are distinct unsigned values. Delay is 0..1,000,000; periods are
1..1,000,000; each advance is 0..1,000,000. Deadline comparisons require
all active deadlines to remain within half the unsigned ring of the current
time, maintained by these bounds and by catch-up on every advance. Inactive
timers are skipped until restarted. Firings and digest arithmetic are unsigned
64-bit (modulo its width); tick arithmetic is unsigned 32-bit. Catch-up counts
every elapsed period and leaves each active deadline strictly in the future.
Add/restart reject invalid bounds; duplicate IDs and an exhausted slot table
are rejected. Cancellation preserves historical firing counts. Cancelling
through a null optional scheduler is a no-op that reports no cancelled timer.

The deterministic adapter `evaluate` accepts seeds 0 through 11 inclusive.
Those seeds create twelve distinct data/operation sets within the contract.
The general algorithms are not limited to those twelve sets. Calls to the
adapter outside its documented seed range are unsupported.

## Injected interacting defects

1. Omit the null optional-scheduler check in cancellation. Ordinary cancellation calls have live scheduler objects; the last scenario exercises the no-op path with a null handle.
2. Advance a caught-up periodic timer by only one period while counting all missed firings. This leaves stale deadlines and corrupts future catch-up counts and next-delay queries.

Both defects affect the same data structure and composed query results.
`main` combines ordinary scenarios 0, 1 and 2 before scenario 11. It does not
stop early on a logic mismatch or compare against placeholder golden values.
`reference_main.c` checks twelve input sets through exact stdout and also
calls `program_main`. Fix spans are original one-based inclusive minimal ranges.

## Qualification status

Construction only: local host checks and expected-output generation do not
establish sealed GCC-14 -O0, EvoCC, sealed UBSan, mutation-ablation, or C0
preflight qualification. All five required conditions remain subject to the
registered experiment's validation and preflight procedure.

The rejected initial mutation used signed modular subtraction. It raised UB
but an otherwise repaired GCC -O0 program matched the fixed outputs. That
mutation could not meet the hidden-test requirement and was replaced.
