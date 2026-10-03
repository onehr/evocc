# Compilation-core pilot and source input — 2026-10-04

Contract: [V00–V06](../lean-plan.md#first-public-release-verification).
Population/commands: [core-v1-pilot.json](../../tests/contracts/core-v1-pilot.json).
**V00/V01 remain open; there is no whole-function or compilation theorem.**

| Commit / slice | Result | Claim ceiling |
| --- | --- | --- |
| `63c1f5b8` — inventory, original functions, checked arithmetic | Nine focused Rust tests pass; initial four theorem targets are kernel-only. | Finite interpreter/pipeline tests and primitive correspondence, not compilation. |
| `ffb862f2` — clean Lean receipt | Full Lean gate passed on clean `63c1f5b8`; all six historical vector groups retained. | Receipt covers that exact Lean source, not later work. |
| Source-input follow-up | Fourteen focused Rust tests, ten Lean reader cases and five actual exporter/reader pairs pass; `FixedMul` expression theorem checked. | Source tree decoding is explicitly `DECODED_NOT_VERIFIED`; no independent source well-formedness or whole-function checker yet. |

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
recovery (exit 143), not recorded as PASS. The follow-up full gate is pending.
No optional compiler optimization, P0/g0 registry or public verification claim
was enabled by this slice.

Disk: only regenerable `target/frontend` caches were retired after checking for
open handles. Cleanup removed 911,226 `.rcgu.o` directory entries; their summed
logical size is **not** physical space recovered because many were hard-linked.
Old test binaries (3.64 GiB logical) and incremental caches were also retired.
Current executables, manifests, receipts, sealed fixtures, kernel runtimes and
Lean `.lake` were preserved. Start/end project disk delta is recorded after
the follow-up gate; host-wide free-space variation also includes unrelated swap.
