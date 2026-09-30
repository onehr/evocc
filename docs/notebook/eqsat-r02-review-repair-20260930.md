# R02 review repairs and deterministic completion

The September 30 review identified four collector/telemetry defects and an
unqualified latency protocol. These are separate from optimizer correctness and
from the already audited historical code-size/proof results.

## Repairs

- Fixed-template `UNKNOWN` now requires a completed nonzero command. Retention,
  artifact-integrity, launch and watchdog failures remain campaign failures.
- Process-group inspection distinguishes zombie-only groups from live children.
  Unknown process states are conservatively live only in the owned group.
- The independent guard-site readers include remainder-zero `3003` and
  leading/trailing-zero `3022`, as well as the other five admitted UB reasons.
- Public chain wall scopes use checked subtraction. Inconsistent telemetry is an
  explicit error, not a debug panic or a release-mode wrapped duration.

The RED captures are retained under `.evidence/eqsat-r02-review-20260930`.
`python-red.log` records nine failures; `wall-scopes-red.log` records the actual
single-test subtraction panic (not the earlier zero-test filter invocation).

## Changed gate

| Gate | Result | Retained log SHA-256 |
| --- | --- | --- |
| Rust full workspace | 2,863 passed, 0 failed, 4 ignored; 366 suites | `4e6fea9f81ed3baf49c5e6beca9165b0e054f8a195078559caee65dd70896f5a` |
| Python ladder discovery | 466 passed, 0 failed, 1 skipped | `4d929fe80c9300aa92a6c71a978cea8c27f5a822b52cefaac6b18da8c0f98639` |
| Workspace format | PASS | `format.log`, empty successful output |
| Workspace/all-target clippy | PASS, warnings denied | `3befba6f992372445365bf6344a85de7a436d61a58680ab1659d3c7a06ecc207` |
| Release checker/census/IR backend | Built successfully | `release-tools.log` |

The Python skip is the optional historical-real-capture lane, not a failed test
or a claim that the new completion capture has already passed independent audit.

## Next frozen execution

R02b1 completes all 512 selected-pair signatures at both chain schedules, using
deterministic object-code size rather than latency, and attempts every one of
the 1,222 fixed-template candidates (including the two prior controls).
All policy chains still use the independent checker. Identical immutable
artifacts are hard-linked without discarding their names or exact bytes; object
emission is memoized by verified IR digest and UB profile. The retained-byte cap
counts unique inodes, not repeated links. No AI run, new rule installation or
unchecked runtime result is authorized by this census.

Execution and independent capture audit are **NOT RUN** at this checkpoint.
The completion plan must be committed before running. The Linux zombie-only
regression is the first guest execution before any policy census result.

R02a1 remains separate: in-process steady-state work, guest CPU pinning, a quiet
host collector, Linux A/A calibration and a separately frozen A/B plan are
required. The existing steady-mode entry points deliberately fail closed until
that runner is implemented and qualified. The historical 576 timing samples
remain retained but are **NOT QUALIFIED** for latency claims. R03 remains closed
to AI activity until the preflight verdicts justify its admission.
