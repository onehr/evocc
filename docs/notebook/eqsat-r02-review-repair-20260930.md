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

During the independent completion run, a storage check found 4,184,140 KiB of
rebuildable Rust incremental state in `target/debug/incremental`. No `cargo` or
`rustc` was running, and this cache is not a frozen campaign input. Only that
exact cache directory was removed; compiler binaries, source, partial captures
and kernel build inputs were preserved. This cleanup is not an acceptance result.

Before freezing the next timing build, header REDs exposed an integration defect:
the producer emits `OBSERVED_BUILD`, while the outer auditor expected
`OBSERVED_build` from the lowercase plan mode. The auditor now uses the canonical
uppercase stage status. Both valid build-stage admission and refusal of the
noncanonical spelling were failing before the repair. The two REDs are retained
in `python-steady-build-header-red-valid-fixture.log`, SHA-256
`b121b1c7c1d600df1e984690dc7bcdbd73a0a9946cf8cd341cde35f2c051f647`.
The deliberately incomplete fixtures still fail command replay; they cannot
establish native acceptance. The focused steady gate passes 12/12. Full ladder
discovery passes 482 with zero failures and nine optional capture tests skipped
(491 total), `python-build-stage-final.log`, SHA-256
`9914fff5f1792dab24d65fafa74a248160f653b023fe75e87f846833a9c627a9`.
The current deterministic collector does not consume this steady auditor, so
its pinned inputs, bounds and running capture are unchanged.

The same integration review found that the steady auditor calls the shared
`audit_usage` validator without pinning its defining file. That file is now a
required code-identity input of every steady plan; omission is rejected. The
source-identity RED is retained in `python-steady-auditor-pin-red.log`, SHA-256
`59ae11149fee7c3da015433c58556cb881630e17d582ce0046fdb46ec4d7a818`.
Outer receipt scope and the claimed timing metric are also checked against the
registered stage rather than passed through or ignored. A one-field RED exposed
six accepted forged scope/metric headers across build, A/A and A/B; the repaired
header rejects them, while the earlier PMU rejection remains intact. That RED is
`python-steady-scope-red.log`, SHA-256
`cc56b0d78ddd27d2e0c16378d5b98e9dd8dd74b744d31ad3085691354bc98ade`.
The focused steady gate passes 14/14; full ladder discovery passes 484, zero
failures and nine optional real-capture tests skipped (493 total),
`python-steady-integration-final.log`, SHA-256
`3435808b71f22f2ad40583da4a5e2ff8b4e28786597bcd1f1463bbbf941479f9`.
These are admission/plumbing tests, not native performance measurements. No
already-frozen campaign inputs or measurements are changed.

The next build also exposed a genuine default-control mismatch: canonical
policy witnesses do not include `[1; 13]`; their fully positive representative
uses `mul=2`. Selecting it as the compiler default would change the experiment.
The steady build now explicitly runs the existing census adapter with all-one
weights and the fixed eight-transaction schedule, independently checks its
chain, and emits both native profiles from that same verified output. Its plan
pins the adapter, exact optimizer control and historical composition manifest;
the build auditor reconstructs those commands and binds the resulting objects.
No candidate is selected by code-size or timing results.

The minimal missing-default RED is `python-steady-default-red.log`, SHA-256
`d4219cd47a0b8d222584754f840694e05226064f45707c0e23c559e5aa11435c`.
A second RED caught the plan's attempt to inventory a future B bundle before
the producer created it (`python-steady-default-plan-red.log`, SHA-256
`489b74d9e05fbeb0d0b0963faf893f399bc24d708bd5e821967fa1e92bb3a3e8`).
The repaired plan inventories existing A inputs only; bundle creation stays
with the Rust writer, whose public contract requires a new directory. Focused
steady tests pass 16/16. The full Python ladder gate passes 486, zero failures
and nine optional capture tests skipped (495 total),
`python-steady-default-plan-final.log`, SHA-256
`6f11d9ac2338476528955210b5f1468d383525058f01736f0bae1f2fbf493adc`.
These gates do not substitute for the forthcoming real build/audit/A/A.

The running deterministic producer has observed all 1,024 policy evaluations,
380 object-code classes and a new consulted `(add, shl)` pair outside its frozen
comparison vocabulary. Its policy label is therefore `INCONCLUSIVE`, not
finite-closure success. These are still producer observations until template
completion and independent replay; the census is neither rewritten nor given
an expanded budget, and AI remains unadmitted.

Direct inspection of the five historical library A bundles confirmed that all
use `evocc-identity-chain-v1`. The steady composer still passed a checker to
their byte-identity verifier, which correctly refuses that incompatible API
combination. Replacing the mocked baseline in the missing-default RED with
actual identity-bundle bytes reproduced the failure. A now uses the existing
identity binding with no asserted EVP proof; B still requires independent
checker replay. The RED is `python-steady-identity-baseline-red.log`, SHA-256
`573d74c326aca160a8760811034ccd3ae7b5f46e28f04fa585a3647aa970274b`.
The steady/twin gate passes 19/19 and the complete ladder gate passes 486, zero
failures and nine optional capture tests skipped (495 total),
`python-steady-identity-baseline-final.log`, SHA-256
`6a6ad7e8e04b35d84c2d97dbc0d72487b1c98c3e2b3e507ba1b9522419e81bca`.
All ten A-profile backend objects are present in the running completion's cache;
that read-only inventory observation is not native timing admission.

A further reserve check found 5,807,168 KiB (about 5.54 GiB) of idle incremental
state in `target/frontend/debug/incremental`. Two process checks found no
`cargo`/`rustc`; the exact target was a normal directory and is not a frozen
input. Only this rebuildable cache was removed. The release checker, IR backend
and census executable hashes were identical before and after deletion. Debug
and release binaries, retained captures, library sources and kernel builds are
unchanged. This is storage maintenance, not a compiler or timing result.

## Completed census and audit-only repair

The owned deterministic worker completed `complete-9bjq0w16` and exited its VM
cleanly. The original receipt has SHA-256
`7b57a622e5adae8dc6de9a654ace597f2fd1a08fa34a050e268f8bc796ec5ae5`:
1,024/1,024 policy evaluations, 1,222/1,222 template attempts, 1,174 producer
template successes and 48 UNKNOWN outcomes. It observed 380 object-code classes,
966 backend objects and the previously unseen `(add, shl)` comparison. The
policy track remains INCONCLUSIVE: finite closure of the originally consulted
comparison vocabulary was not established. No rule was installed and no AI or
native timing was performed.

The first actual independent gate failed: seven adversarial tests passed but
the positive replay raised `gate/cap telemetry changed`. Its retained stderr
is `audit-completion-9bjq0w16.stderr`, SHA-256
`3263f6aca999b4a26a18ad81250ab260a77c0a39e7e464dc56a0d619ca04f56a`.
Recorded JSON pairs are arrays; the recomputed pairs are Python tuples. The
contents, ordering and null operands matched, but Python container equality
did not. A small regression reproduced that error. The auditor now converts
only the recomputed pairs to arrays; wrong values, order or multiplicity still
fail. The same repair covers the final new-comparison check.

The completed manifest and capture are not rewritten, and the four-hour
experiment is not repeated to repair its reader. A separately committed
`evocc-r02-completion-reaudit-v1` supplement binds the original manifest and
receipt, the original committed collector/auditor hashes and the new committed
audit-only code hashes. Only those two files can change; every other original
tool, source, semantic file and budget is still checked unchanged. This is an
explicit versioned re-audit, not a claim that the original auditor passed.
Independent acceptance and steady build/A/A/A/B remain pending until their
actual gates run.

The first full Python gate encountered the sandbox's denial of `/bin/ps`, not
EMFILE: 498 tests, one process-table permission error and nine optional capture
skips. The failed log is preserved as `python-completion-reaudit-final.log`,
SHA-256 `376845e32186bad2ac79c9489d286eb13cc51f9e80b6e49dfd2fd5fab4459e00`.
The process cleanup test requires read-only process-table access; its complete
gate is rerun with that permission, without skipping the test.
The complete authorized gate passes 489 tests, zero failures and nine optional
capture skips (498 total), `python-completion-reaudit-unsandboxed.log`, SHA-256
`8177b1f306f6570761aa1accfbbb720018c10858426f1b3547081653a6a7b4a9`.
The eight focused completion tests pass as well. These are code/admission
regressions; they do not yet accept the actual completed capture.
An additional real temporary-Git fixture checks the public supplement binder:
uncommitted supplements, changed original manifests/receipts and a declared
revision lacking the repaired code are rejected. All nine focused completion
tests pass. The live replay still uses the same frozen two-file repair; this
additional test does not modify its code or supplement identities.
