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

The first committed completion plan stopped at its initial Linux zombie RED:
the sealed image has no `ps`. No policy evaluation or template attempt ran.
Capture `complete-mxd1qlnf` and its failed receipt remain retained. Linux process
observation now reads kernel `/proc/<pid>/stat`, handling parentheses in `comm`
and process disappearance without depending on procps. Malformed/unavailable
snapshots still fail closed. The added parser REDs and the full Python gate
pass: 468 passed, 0 failed, 1 skipped (`python-proc-final.log`, SHA-256
`cc16cec84f7ddd8f0bb93ff0a792c619b1c6fb58b3643c17c37283ecf13fbeae`).

Successful completion execution and independent capture audit are **NOT RUN**.
The committed kernel-state plan started capture `complete-fxbm__9o`, but the
Codex daemon restart terminated its collector. Its unchanged receipt ends at
423 complete policy evaluations and a partially started 424th evaluation;
zero template proofs ran. Receipt SHA-256:
`ae6d72b89adcf2f8973033285f8bdc72a2a6fc2247dfd722c4192dbf2d1e3eeb`.
The exact owned orphan VM was subsequently stopped. This interrupted capture is
retained, not promoted, and will not be merged into the next run or given a
reset campaign budget. A fresh capture uses the same committed manifest and
the same limits. The Linux zombie-only regression precedes every policy census.

That fresh capture, `complete-kz95w4d2`, was safely interrupted after 509 complete
policy evaluations and zero template attempts. Its receipt is `INTERRUPTED`,
not a completed census; owned VM cleanup exited 0. Receipt SHA-256:
`9e6a01e7bd174271fc1a254f611642e6818be185842d180f1566f11b35920954`.
The interruption preserved all 47,037 files (751,883,936 unique-inode bytes).
No partial evaluations are merged into another run.

The collector's cooldown began at the start of a retention scan. Once scans
took longer than one second, the next pipe poll immediately began another full
scan. An actual one-second collector stack sample and two isolated REDs exposed
the rescan feedback and per-file `Path` reconstruction. The repair starts the
cooldown at scan completion and uses `os.stat` on the existing walk names. Full
unique-inode counting, permission-error refusal, disappearing scratch handling,
the byte ceiling and the mandatory final scan remain intact. The retention
label now explicitly states the minimum completion-to-next-scan interval; a new
manifest must pin this collector before restarting. The optimizer, checker,
policy/template populations and all resource limits are unchanged.

On the stable interrupted capture, three diagnostic scans measured median times
of 1.133 seconds before and 0.844 seconds after the per-file repair. The new
scanner accepts the exact old byte ceiling and rejects one byte less. This is
collector diagnostics, **not** compiler-performance evidence. The focused gate
passes 36/36; full ladder discovery passes 480 with zero failures and nine
optional capture tests skipped (489 total), `python-retention-final.log`, SHA-256
`0da9e585601a50e0092d124e4ffb2f1bf40cd477c37b2b209f4359401c9f630d`.
The separate Lean-tool Python gate passes 15/15. No Rust implementation changed
in this repair; the earlier full-workspace gate is not relabeled as a new run.

The repaired foreground capture `complete-slksjlzi` was then safely stopped at
185 complete evaluations (zero templates), with `INTERRUPTED` and cleanup exit
0, before moving this long test out of the interactive daemon's process tree.
Receipt SHA-256:
`a30e2cf22a60a7532cc91f97b04fe86c251bea7c45778f7260a9de5deffaff06`.
It is retained separately, not resumed or merged. An empty bounded subprocess
probe and the real worker both verified independent sessions with PPID 1 and
closed inherited stdio. Actual daemon-restart survival is not separately tested.

Capture `complete-9bjq0w16` uses the same committed repaired manifest, SHA-256
`2272bf7bbb10ecc47543a20346ee58fa8eb51052fd59687656a8f9ce2db1ee29`,
from an independent bounded worker (initial PID 20740). Its launch argv and log
are recorded in `.evidence/eqsat-r02-review-20260930/detached-completion-owner.json`.
The worker still owns the bench lock and the same four-hour/3-GiB limits. Its
completion, independent replay and steady timing admission remain unclaimed.

R02a1 remains separate: in-process steady-state work, guest CPU pinning, a quiet
host collector, Linux A/A calibration and a separately frozen A/B plan are
required. The runner and independent auditor are now implemented, including
pinning every consumed proof, object and header and inheriting predecessor
inventories. The latest Python gate is 477 passed, 0 failed, 9 skipped
(`python-inventory-final.log`, SHA-256
`d4c3473f7cbd8288e7b48b92bffc269639f4d2fc931552217ebfd2586ddd78dd`).
The skips are the optional historical capture lane and eight new real-completion
audit/mutation tests; they are not claims of native qualification. New Linux
steady build, A/A and A/B executions remain **NOT RUN**. The historical 576
timing samples remain retained but are **NOT QUALIFIED** for latency claims.
R03 remains closed to AI activity until the preflight verdicts justify admission.

A further steady-audit RED swapped the raw execution chronology while retaining
valid A/A hashes, paired values and statistics. The auditor now requires every
attempt index in the frozen probe/admission/sample order. The full Python gate
is 478 passed, 0 failed, 9 optional real-capture tests skipped (487 total);
`python-chronology-unsandboxed.log`, SHA-256
`5fad7bdbb4b2fc1e678623ac95d55f17e4983a8f9d396bca0c22201944f83d62`.
The first sandboxed invocation failed only because macOS denied `/bin/ps` in
the surviving-descendant test; that unsuccessful log is retained and excluded
from the PASS count.
