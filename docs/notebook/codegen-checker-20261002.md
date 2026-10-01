# N04: bounded C checker interface

**Bounded interface accepted.** Plan frozen in `917f8955`:
[contract](../../tests/contracts/codegen-n04-20261002.json).
Implementation and this record share the containing commit. [Compact receipt](../../tests/contracts/codegen-n04-20261002-receipt.json).

| Item | Evidence / status |
| --- | --- |
| Initial public-path RED | 9 tests failed: missing `--check`, `--explain`, JSON interfaces |
| Explanation catalogue | 89 authority entries; 78 named codes with explicit explanation TODOs |
| Reviewed UI outputs | 8 `.c` / `.stderr` pairs; no automatic blessing |
| Focused acceptance | 12 PASS / 0 FAIL: 10 public-path tests, 2 diagnostic unit tests |
| Full workspace | 377 suites; 2,915 PASS / 0 FAIL / 4 ignored; Clippy and formatting PASS |
| Lean / native campaign | Not required: no semantic or registry change, no native execution claim |

`--check` enters the existing EvoIR program interpreter through its unchanged
main contract, lifecycle and 100,000-step limit. A completed path reports the
integer returned by `main`; a nonzero program result is not a checker error.
Existing E-codes remain unchanged. Fuel exhaustion, unresolved calls and other
unsupported execution remain failures, never successful checks or automatically
UB findings. [User workflow and JSON fields](../checker.md).

The build generates explanations from EvoIR section 6.3; missing authority
entries remain TODO rather than invented meanings. New JSON diagnostics report
unavailable locations as `null`; legacy text retains its compatibility fallback.
The initial focused rerun exposed two incorrect new test expectations (array
diagnostic column 11 instead of the observed 12, and an assumed 90 table rows
instead of the actual 89). Both were corrected against existing behavior.
A further public probe reproduced a fabricated `1:1` JSON input-error position;
its regression test and presentation-only fix are included.

**Claim ceiling:** existing interpreter diagnostics on supported, executed
paths, not static whole-program analysis, absence-of-UB proof or complete
libc/system/concurrency execution. No optimization or verified-size claim.

**Disk:** approximately +31 KiB exclusive sources, UI fixtures and documentation;
no raw capture or runtime output retention. Shared driver and Cargo costs are
recorded separately in N01; no unrelated host free-space change is credited.
