# Compilation-core pilot and source input — 2026-10-04

Contract: [V00–V06](../lean-plan.md#first-public-release-verification).
Population/commands: [core-v1-pilot.json](../../tests/contracts/core-v1-pilot.json).
**V00/V01 remain open; there is no whole-function or compilation theorem.**

| Commit / slice | Result | Claim ceiling |
| --- | --- | --- |
| `63c1f5b8` — inventory, original functions, checked arithmetic | Nine focused Rust tests pass; initial four theorem targets are kernel-only. | Finite interpreter/pipeline tests and primitive correspondence, not compilation. |
| `ffb862f2` — clean Lean receipt | Full Lean gate passed on clean `63c1f5b8`; all six historical vector groups retained. | Receipt covers that exact Lean source, not later work. |
| `1b99fae2` / receipt `833982ed` — source input | Fourteen focused Rust tests, ten reader cases and five exporter/reader pairs pass; clean Lean gate passes, five kernel-only primitive targets. | `DECODED_NOT_VERIFIED`; no whole-function checker. |
| `4483ef67` — independent source validation | 44 input cases, five frozen exports and 36 integer-type pairs pass; an exported unsequenced-access case refuses. Clean Lean gate passes. | `WELL_FORMED_NOT_COMPILATION_VERIFIED`; execution safety and source/target correspondence remain unproved. |
| Integration gate on the same Rust implementation | 401 suites: **3,079 passed / 0 failed / 5 ignored**; Clippy and formatting pass. | No compilation or verified-release acceptance. |
| `857d7d90` — source-machine slice | 22 semantic cases, six Rust source-interface tests and ten-function exported closure pass; both Rust interpreters agree. Full workspace: **401 suites, 3,080 passed / 0 failed / 5 ignored**; Clippy/format pass. Clean Lean gate: ten core targets, all kernel-only. | Unbounded source steps and kernel-only machine/primitive lemmas; no source/IR simulation or checker soundness. |

| Existing-pipeline pilot | Functions with wrapper | Blocks | Spill store/reload pairs | Incoming stack args |
| --- | ---: | ---: | ---: | ---: |
| Doom `FixedMul` | 2 | 12 | 0 | 0 |
| musl `abs` | 2 | 15 | 0 | 0 |
| `abs(INT_MIN)` → E3004 | 2 | 10 | 0 | 0 |
| Loops, joins and short-circuit side effects | 3 | 49 | 0 | 0 |
| 32-argument pressure closure | 3 | 3 | 15 | 24 |

Observed REDs: ordinary-name `abs` substitution bypassed the original body;
the frozen pilot now explicitly uses `-O0 -fno-builtin`. The source exporter
initially did not exist. New CLI tests initially used unsupported `-` input;
they now use uniquely created, immediately removed temporary C files without
changing the compiler's input contract. An unprototyped declaration in an
otherwise prototyped chain refuses at export rather than inventing a prototype.

The first full-workspace attempt was interrupted during compilation for disk
recovery (exit 143), not recorded as PASS. The sandboxed follow-up completed:
3,077 passed, two container-permission failures, five ignored. The same-scope
rerun with container access passes, including both previously denied probes.
Raw logs remain outside Git at `.evidence/verification-core/20261004`
(under 1 MiB; cap 500 MiB); commands and counts are in the pilot manifest.
No optional compiler optimization, P0/g0 registry or public verification claim
was enabled by this slice.

Disk: only regenerable `target/frontend` caches were retired after checking for
open handles. Cleanup removed 911,226 `.rcgu.o` directory entries; their summed
logical size is **not** physical space recovered because many were hard-linked.
Old test binaries (3.64 GiB logical) and incremental caches were also retired.
Current executables, manifests, receipts, sealed fixtures, kernel runtimes and
Lean `.lake` were preserved. After the gate and unused incremental-cache cleanup,
`target/frontend` is **9,686,632 KiB (9.2 GiB)** versus approximately 24 GiB at
entry: its net footprint is roughly **15 GiB lower**, not an exact physical
recovery or whole-project net measurement. New tracked text and retained logs
are under 2 MiB. Host-wide free-space variation also includes unrelated swap.
The source-machine slice retired another 400 idle single-link Mach-O test
binaries (3.483 GiB logical), preserving compiler/checker binaries and evidence.
Its bounded execution results are `EXECUTED_NOT_VERIFIED`; a step limit is not
an observation or divergence proof. `while_true_diverges` instead constructs an
infinite trace. The primitive bridge retains executed literal wrapping versus
runtime overflow, independent count widths, and distinct div/rem diagnostics.
After this gate, idle test/example binaries and release library/object caches
were retired (about 6.6 GiB logical; all regenerable). Free space rose from
20,841,272 to 27,393,504 KiB; compiler/checker executables and evidence remain.
This slice's retained text/log growth is under 2 MiB, with build-cache growth
removed; the host-wide free-space delta is not attributed solely to this task.
