# EvoCC evidence and status

This document collects the measured results of EvoCC. The [README](../README.md) describes the
design and the use of the compiler. The results below link to their receipts in
[`tests/contracts/`](../tests/contracts/) and their dated ledgers in [`docs/archive/notebook/`](archive/notebook/). This
document is not a live status. The generated [status dashboard](status.md) shows the current
receipt checkpoint, and [TODO.md](../TODO.md) gives the remaining work.

GCC 14.2 and Clang 19.1 in the sealed AArch64 lab give the reference measurements. Documented
profile decisions resolve supported disagreements. The remaining unsupported forms have explicit
refusal boundaries. Linux 6.18.44 is the kernel target. The application ladder measures progress
beyond kernel boot.

## Measurement dates

The dates come from the README text of each block, or from [TODO.md](../TODO.md) where the README
gave none. No other date is added.

| Block | When the numbers were measured |
|---|---|
| [Status — 2026-09-28](#status--2026-09-28): application benchmarks | Audit of existing measurements on 2026-09-28. Unchanged-program replays on September 27 cover torture, c-testsuite, TinyCC, toybox, LLVM SingleSource, jq, cJSON and musl. The other lanes keep their earlier measurements. |
| [Linux boot, kselftest and KUnit](#linux-boot-kselftest-and-kunit) | Last full 970-program pair: September 22. One-program BPF replay: September 23. The latest and the preceding uniform compositions: September 28. The other rows give no date. |
| [Language and research boundaries](#language-and-research-boundaries) | musl replay: September 27. The other items give no date. |
| [Doom integration](#doom-integration) | Part of the status of 2026-09-28. The text gives no separate run date. |
| [Checked EqSat measurements](#checked-eqsat-measurements) | One date for each row, from 2026-09-09 to 2026-10-01. |
| [Other implementation gates](#other-implementation-gates) | Rust–Lean correspondence: rerun on 2026-10-01. The 21,000-program gates and the 384/384 corpora: the text gives no date. |
| [Recorded acceptance evidence](#recorded-acceptance-evidence) | V01 and V02 completion on 2026-10-05 (from [TODO.md](../TODO.md#first-public-release-verification)). |
| [Acceptance sweep](#acceptance-sweep-2026-10-06) | 2026-10-06, at implementation commit `8770b186`; repeated at `e657709b`. |
| [Native run of the checked output](#native-run-of-the-checked-output) | 2026-10-06, subject `e657709b` (the receipt date). |
| [Showcase](#showcase-notes) | The text gives no date. See the capture notes. |
| [Verification details and limits](#verification-details-and-limits) | Moved from the README of commit `3b0f18fc`; the V00 to V08 state is that of 2026-10-06. The README examples ran at the same commit. |

## Showcase notes

The README keeps short captions. These are the full caption facts of each image:

| Image | Caption facts |
|---|---|
| Doom / Freedoom | A real game engine; headless replay. |
| TinyCC paints reflections | EvoCC → TinyCC → a C ray tracer. |
| Lua explores a fractal | Floating-point iteration → RGB pixels. |
| SQLite | Its own `veryquick` suite: 0 errors in 329,803 tests. |
| jq | `make check`: 9 of 9 test programs pass; `main.c` is GCC-built (B07a). |
| cJSON | `cJSON_test` prints the example JSON; 216 unit tests, 0 failures. |
| Linux 6.18 | 7,008 EvoCC C units; boots on 4 CPUs, runs init, powers down. |
| Toybox | 1,669 tests; PASS, FAIL and SKIP lines identical to the GCC build. |
| Mbed TLS seals pixels | Encrypt → restore → reject tampering. |
| Zstandard packs a fractal | Smaller bytes; identical pixels. |
| zlib ships a tiny arcade | Pixel-art sprites → gzip → intact. |
| PolyBench models a field | 20 simulation steps; FDTD-2D. |
| OpenTyrian | 52 C units; three frames match GCC exactly. |
| Neverball | Physics + OpenGL; one disclosed source fix. |
| Quakespasm / LibreQuake | 77 C units; real 3D demo replay. |

Doom is a bounded headless replay, not interactive gameplay. Kernel boot is not full kernel-test
completion. The SQLite, jq, cJSON, Linux and Toybox cards quote recorded test logs; their text is
verbatim. The library demos use explicitly labeled reference-built adapters. The charts are
renderings of captured data, not native application UIs. The three games use reference SDL/OpenGL
libraries. Their screenshots are bounded native runs, not full-game acceptance or graphics-driver
compilation. The [capture notes, exact commands and limitations](images/showcase/README.md) keep
these small examples separate from full benchmark acceptance.

## Status — 2026-09-28

**Full kernel boot works. Full kernel testing and the complete application ladder are not yet
closed.** The latest recorded application lanes have **10 PASS, 1 PROFILE_CLEAN and 4
INCOMPLETE**.

The application rows are an audit of existing measurements. They are not a new run of the checkout
of that day. The unchanged-program replays of September 27 cover torture, c-testsuite, TinyCC,
toybox, LLVM SingleSource, jq, cJSON and musl. The other lanes keep their earlier measurements.
This is not a completed fresh 15-lane campaign and not a kernel-test rerun. Each receipt keeps its
compiler and input identity. See the generated [status dashboard](status.md), the
[archived roadmap](archive/roadmap.md), the [application ledger](archive/notebook/benchmark-evaluation-20260915.md)
and the [kernel issue ledger](archive/notebook/kernel-selftest-gaps-20260914.md). The Git-tracked
[TODO list](../TODO.md) gives each remaining task a stable ID, a priority, a next action and an
evidence-based closure condition.

### Application benchmarks

`PASS` is limited to the recorded build and test selection. `PROFILE_CLEAN` means zero compiler
failures after the separation of evidenced deferrals, inapplicable cases and unusable-oracle cases.
It does **not** mean that all originals passed. TU counts are the EvoCC translation units in the
tested build of the lane. They are not test counts. Mixed-link builds can still contain
reference-built code.

| Benchmark | Status | Recorded result | Scope or remaining issue |
|---|---|---:|---|
| GCC torture | PROFILE_CLEAN | 1,522/1,692 programs PASS | 0 compiler failures; 21 FP-deferred, 146 not applicable, 3 oracle-unusable; September 27 full replay |
| c-testsuite | PASS | 220/220 programs PASS | Fresh full population, zero exclusions or deferrals |
| Csmith | INCOMPLETE | 932/1,000 checksum agreements | All 1,000 compile; 0 mismatches; 68 unusable-oracle cases are not passes |
| zlib | PASS | 19/19 TU | Recorded original program tests |
| zstd | PASS | 44/44 TU | Recorded original tests; build adjustments remain in the receipt |
| cJSON | PASS | 23/23 TU | Both libraries + all 21 Unity/private-helper programs; 161 cases PASS, 1 upstream IGNORE; public-API controls also pass |
| Mbed TLS | PASS | 382/382 TU | Default configuration selftest and 148/148 CTest |
| TinyCC | PASS | 11/11 TU | Original `make test`; relink/test exits and installed objects audited |
| toybox | INCOMPLETE (shared test FAIL retained) | 203/203 TU installed; all 1,669 test occurrences complete | Both builds: 1,648 PASS / 1 FAIL / 20 SKIP; tar expected-hash contradiction reproduced; zero EvoCC regressions |
| musl + libc-test | INCOMPLETE | 1,271/1,339 TU retained in the mixed build | Two complete runtime replays: zero added failures or replaced objects; 68 floating-complex refusals and 17 shared reference failures remain, with option omissions separately recorded |
| PolyBench/C | PASS | 30/30 kernels | MINI_DATASET array-output correctness, not performance; reference-built allocation utility |
| LLVM test-suite | INCOMPLETE | 1,039/1,735 programs PASS | Fresh full run: 1 unresolved compiler failure, 30 FP-deferred, 664 not applicable, 1 oracle-unusable; no installed object rebuilt |
| jq | PASS | 69/69 TU; original `make check` PASS | Real binary128 interface; original `-pthread` preserved; no object fallback |
| Lua | PASS | 34/34 TU | Matching official `all.lua`, without soft skips |
| SQLite | PASS | 86/86 TU | Public Tcl `veryquick`: 329,803 assertions; not proprietary TH3 |

The [benchmark receipt](../tests/contracts/bench-receipt.json) and the individual
`ladder-*-receipt.json` files bind these results to their measured subjects. The complete LLVM
replay of September 27 adds 25 original-program passes over the old full run. These include the
earlier targeted FP repairs and all three LARGE `lu`, `ludcmp` and `floyd-warshall` workloads under
the unchanged 60-second limit. All 1,048 installed objects survive the final build unchanged. The
remaining compiler-failure classification is `pr23135` (L10 in TODO). It waits for its
integer-vector source-definedness ruling. It is not FP-deferred.

### Linux boot, kselftest and KUnit

All kernel rows use Linux 6.18.44 ARM64, but the configurations and Images are different. The
target-C objects come from EvoCC. Assembly, host build tools and userspace test dependencies are
counted separately.

| Scope | Verified result | Limit |
|---|---|---|
| K6 locked configuration | **2,799/2,799 target-C**, complete link, real boot and normal shutdown PASS; uniform EvoCC rebuild, zero GCC target-C fallback | Not every Linux configuration or subsystem |
| BPF/module configuration | **7,008/7,008 target-C**, pinned current compiler, generated-input fixed point, complete link/boot PASS; 848 modules linked/BTF/signed/installed | Not all installed modules runtime-tested; fresh full kselftest/KUnit still pending; QEMU drift explicitly recorded |
| Current FPU and connector follow-up | Both kernels: FPU **1/1 + 4,000/4,000 workers PASS**; bounded connector controls **20/20 each** | Actual signed FPU module loaded; unchanged persistent connector listener still times out under its original 45-second runner, not counted PASS |
| K7 selected kselftests | Both kernels: **9/9 programs, 125/125 subtests PASS**, no skip/failure | Not all kselftests or KUnit |
| Full installed kselftest population | All **970 programs started on each kernel**; aggregate INCOMPLETE; counts below | Unbuilt collections and unavailable hardware remain; latest Image needs a full rerun |
| Expanded built-in KUnit | Of 1,504 cases: EvoCC **1,432 PASS / 12 FAIL / 60 SKIP**; GCC **1,438 / 7 / 59** | Subsequent filtered repairs are not a new full result |
| Expanded KUnit modules | Both kernels actually loaded all **31 test modules**, **765/765 cases PASS** | This retained 6,916-target-C composition differs from later filtered FORTIFY Images; its 809-module configuration disables module signing |

The last full 970-program pair (September 22) records these **program-level** verdicts. They are
not individual nested assertions:

| Kernel | PASS | FAIL | SKIP | TIMEOUT | INVALID_OUTPUT | INCOMPLETE |
|---|---:|---:|---:|---:|---:|---:|
| GCC control | 532 | 121 | 302 | 11 | 2 | 2 |
| EvoCC | 529 | 114 | 302 | 19 | 3 | 3 |

The latest [uniform composition](../tests/fixtures/kernel-current-composition.out) of September 28
uses the pinned Rust 1.97.1 compiler `ab2eeb7b…` and Image `18b36fee…`. All 7,008 actual-header
consumers, 849 generated export/module units, the full link, 848 module links/BTF and the
installed-object audits pass. Actual boot/poweroff passes, with zero GCC target-C replacements. Its
matching signed runtime keeps all 4,396 original non-module userspace files. Actual
[FPU execution](../tests/fixtures/fpu-worker-verdict.out) and
[connector delivery](../tests/fixtures/connector-process-events.out) close those two prerequisite
gaps. They do not close the complete 970-program campaign or expanded KUnit.

The preceding September 28 composition used the subject Image `62c29ae4…`. Its real boot/poweroff
and zero GCC replacement are verified. This is not a full kselftest or KUnit result. The September
22 totals stay historical. The frozen `65035621…` compiler of that composition contains Rust 1.98.0
std code. It does not get retroactive credit for the newly pinned Rust 1.97.1 build or for later
compiler fixes. The [new matching module runtimes](../tests/fixtures/kernel-uniform-module-runtime.out)
of that composition pass the 8/8 cases of NCI, both splice programs and the 224/0/5 inner results
of IOMMU on both kernels. This
six-program selection also keeps FPU SKIP (TEST_FPU not enabled) and BPF test_kmod TIMEOUT on both
sides. 964 programs stay NOT_RUN. The September 23 checkpoint is explicitly a **one-program BPF
replay**, not a new 970-program campaign. GCC completes with 651/5644 successful test/subtest
counts, 127 skip counts and 11 failed tests. EvoCC times out at the unchanged 300-second limit
after group 482. Both shut down normally. No test from that flight is still running. The check of
the nested `sched_ext` and BPF summaries does not rewrite the historical counts.

Boot acceptance audits the installed object hashes and the generated-input stability. Then it
starts the actual Image in QEMU, checks the ordered `/init` markers and the per-run nonce, and
requires normal poweroff. Kselftests compare the two kernels with matching test payloads in fresh
per-program VMs. The CPU/EL2 requirements and the explicit timeout policies are recorded for both
sides. KUnit executes in the kernel. Built-in and module results stay separate.

Targeted repairs bring FORTIFY to **25/26 PASS** (failed assertions **96 → 2**), IOMMU on the new
uniform pair to **224 PASS / 0 FAIL / 5 SKIP**, and the ALSA timer tests to **2/2 PASS** on both.
Three BPF variants each pass the selected 10 groups / 51 module/BTF subtests. The remaining work
includes the nonliteral `strlen` constant-expression rule, two KVM timeouts, intermittent
`sched_ext` and `ublk` failures, PM runtime timing, shared breakpoint failures, the remaining
config coverage and the full BPF program. Not every shared failure has a diagnosis. A fresh
complete 970-program pair and a 1,504-case KUnit run after those repairs are still outstanding.

Evidence: [locked K6](../tests/contracts/kernel-full-receipt.json),
[BPF/module K6](../tests/contracts/kernel-bpf-full-receipt.json),
[selected K7](../tests/contracts/kernel-selftest-receipt.json),
[full kselftest](../tests/contracts/kernel-selftest-full-receipt.json),
[expanded KUnit](../tests/contracts/kernel-expanded-kunit-receipt.json).

### Language and research boundaries

Scalar `float`/`double` execution, both interpreters, AAPCS64 HFA/varargs, integer vectors, TLS,
lifecycle registration and the approved GNU extensions are implemented with targeted tests. The
scalar FP engine has **661,848** independent numerical checks across 32 operations. The strict FP
adapter and the world-threaded IR/backend, the dynamic-rounding source mode and non-trapping
lexical `FENV_ACCESS` now have
[targeted value/status and native checks](archive/notebook/strict-floating-implementation-20260927.md). The
musl replay of September 27 keeps 1,271 EvoCC objects through two complete libc-test runs with no
added failures. It is still a mixed build, not full musl acceptance. The scalar `w` asm extension
and its original pthread regressions are repaired. Binary128 source/IR, both interpreters and the
hosted native helpers/ABI now have targeted tests. The fresh 69/69-object original test replay of
jq also passes. Floating complex/vectors and other explicitly refused forms stay outside the
current source profile. FP is not the only remaining application or kernel-test blocker.

The [remaining-semantics brief](archive/notebook/remaining-semantics-research-brief-20260921.md) itemizes
other measured dialect boundaries. These include the tiny-vector external ABI, integer-complex
division, pointer-to-VLA type queries and nonzero-depth return addresses.

Whole-kernel EqSat reproducibility/certificate acceptance (G2), the GCC `-O2` performance
comparison (Q2), measured self-improvement (Q3) and the x86/release gates still have no accepted
completion result. Scalar proof results and successful boot do not close those milestones.

### Doom integration

The separate headless Doom lane builds all **80 engine C units plus one platform adapter with
EvoCC**, without target-C object fallback. It pins doomgeneric
`dcb7a8dbc7a16ce3dda29382ac9aae9d77d21284` and Freedoom 0.13.0 Phase 1. Two bundled demos run two
times each under EvoCC, GCC14 and Clang19: **12 executions, 350 frames each**, with exact
corresponding framebuffer SHA-256 sequences. The runtime libraries, the assembler and the linker
stay the existing tools of the sealed target. This is not a self-contained EvoCC libc build.

The declared profile is GNU11/O2 with **`-fwrapv`**: the original signed `ANG45` multiplication
overflows before the conversion to unsigned. A separately recorded one-line port repair initializes
the previous screen before the initial melt. Without it, the repeated runs of GCC itself have
nondeterministic initial frames. The lane skips no compiler guards and no mismatching frames. The
profile replaces Xlib with a deterministic 320x200 headless adapter. Audio, live keyboard/window
play and other maps/ports are **not tested**. This additional R06 lane does not change the frozen
15-lane application campaign.

```sh
python3 tools/bench.py doom --evocc target/pinned-check/release/evocc --jobs 2
python3 tools/bench.py doom --status
```

The first command requires the existing sealed Apple Container image and the exported ARM64
headers. It downloads about 26 MiB of pinned source/data if they are absent. It creates no kernel
VM disk. The binaries, a few PPM snapshots and the detailed frame hashes stay in
`.build/ladder/doom`. The Linux ARM64 executable is `.build/ladder/doom/evocc/doom`. It streams
frames and does not open a window. See the [receipt](../tests/contracts/ladder-doom-receipt.json)
and the [scope, failed attempts and checks](../tests/fixtures/doom-headless.out).

## Checked EqSat measurements

Every row has a receipt. The right column is the claim ceiling, not a summary.

| Date | What ran | Numbers | What it does and does not establish |
|---|---|---|---|
| 2026-09-09 | Lean audit of `p0.scalar.v4` | 55 / 55 rules proved, kernel-only; [receipt](../tests/contracts/lean-p0-receipt.json) | Rules are sound equalities of the modeled semantics. Not a proof of the compiler. |
| 2026-09-29 | Shared verified chain ([R01a](archive/notebook/verified-eqsat-implementation-20260929.md), [requalification](archive/notebook/eqsat-review-closure-20260929.md)) | Driver and evaluator run one chain; 45 legal transaction states; checker pinned by SHA-256; full workspace 2,850 tests, 0 failures | Optimized output cannot be emitted without independent replay. Integration found and fixed two real producer defects. |
| 2026-09-30 | Bounded Linux pilot ([protocol and results](archive/notebook/eqsat-r02-linux-pilot-20260930.md), [receipt](../tests/contracts/eqsat-r02-pilot-receipt-20260930.json)) | 576 / 576 result-checked executions; 138 certificates replayed, 0 rejections; 108 checked/unchecked twins; default search and certificate caps hit 0 times | The pipeline runs end to end on zlib, zstd and cJSON. Millisecond fresh-process timing was uninformative by construction. |
| 2026-09-30 | Steady-state timing ([protocol](archive/notebook/eqsat-r02-steady-protocol-20260930.md)) | A/A 10 / 10 admitted; A/B run once: 10 / 10 **INCONCLUSIVE** at a 5% floor; frozen, not rerun | No latency improvement, regression or equivalence is claimed. Latency is `NOT_MEASURABLE_ON_THIS_LANE`; size is the V1 objective. |
| 2026-09-30 | Deterministic preflight ([adjudication](archive/notebook/eqsat-r02-review-repair-20260930.md)) | 1,024 policy evaluations, 6,144 chains, 8,854 proofs replayed; 1,222 candidate rules attempted: 1,174 proved, 48 unknown, **0 installed** | Policy, rule and shortcut tracks are all **INCONCLUSIVE / NOT_ADMITTED**. No AI campaign is admitted; proving rules was never the bottleneck. |
| 2026-10-01 | Object-level size ([S1–S4 ledger](archive/notebook/eqsat-size-work-packages-20261001.md), [DCE control](../tests/contracts/eqsat-size-dce-20261001.json), [policy ranking](../tests/contracts/eqsat-size-policy-ranking-20261001.json)) | see table below; exhaustive best policy beats the default by at most 4 bytes | Verified rewrites shrink object `.text` by 0.8–2.4% on the changed subject TUs. The policy weight space is `FINITE_ENUMERATION_SUFFICIENT` for size. |
| 2026-10-01 | Guard-folding ceiling ([result](../tests/contracts/eqsat-size-guards-result-20261001.json), [guard contract](../spec/evoir.md#812-certified-total-integer-guard-folding), [archived draft](archive/eqsat-guard-folding-draft.md)) | Unverified diagnostic recovers 952 / 1,608 / 80 bytes (zlib / zstd / cJSON) from provably-true UB checks | A diagnostic ceiling for R02e, not a certified benefit. Nothing from the diagnostic is installed. |
| 2026-10-01 | Certified guard first slice ([acceptance](archive/notebook/eqsat-certified-guards-20261001.md)) | Seven g0 rules and seven new A2 rows are kernel-only; ten certificates replay; checked text saves 256 / 984 / 28 bytes in zlib / zstd / cJSON | First-slice acceptance on six original TUs. Maximum per-TU saving is 0.800%; unchecked sizes are unchanged. Diagnostic and P0 savings remain separate. |

The table below gives the effect of checked P0 EqSat on the subject translation units: checked
profile, object `.text` bytes, with the common dead-code elimination on both sides (A: EqSat off,
B: EqSat on, same lowering, same backend).

| Subject | A | B | Change | Changed TUs / eligible roots |
|---|---:|---:|---:|---|
| P0 integer mix | 128 | 128 | 0 | 0 / 13 |
| zlib `deflate.c` + `inflate.c` | 58,920 | 58,448 | −472 (−0.8%) | 2 / 3,356 |
| zstd `zstd_compress.c` + `zstd_decompress.c` | 157,712 | 156,272 | −1,440 (−0.9%) | 2 / 10,298 |
| cJSON `cJSON.c` | 30,228 | 29,496 | −732 (−2.4%) | 1 / 1,787 |

What these numbers mean: the checked P0 domain is sound and reproducible. Its measured effect is
small, because the domain is small. P0 cannot touch the checked UB guards (0.4–5.5% of object
`.text` on these TUs, measured as checked minus unchecked), memory operations, inlining or control
flow. The design response is to grow the checked domain one reviewed slice at a time, and to
measure the ceiling before any proof. The first guard slice is now accepted. The
[six-TU GCC/Clang size comparison](archive/notebook/eqsat-size-references-20261001.md) is complete and
records larger EvoCC objects. G2 whole-kernel reproducibility, full Q2 acceptance and any Q3
self-improvement claim stay **NOT_ACCEPTED**. None of these test populations is a substitute for
the others.

### Other implementation gates

The [Rust–Lean correspondence tests](../tests/contracts/lean-correspondence-receipt.json) record
**1,487,689 / 1,487,689** agreeing comparisons: **1,325,045** across the five P0 groups plus
**162,644** guards. All six groups were rerun together on 2026-10-01. Differential agreement is not
a proof that the Rust implementation equals the Lean model. The
[proof audit and trust list](../tests/contracts/lean-p0-receipt.json) bind the registry digest and
the historical proof subject. They are not a blanket theorem about every feature in the latest
specification.

The recorded scalar and MIR gates each cover **21,000 generated IR programs** with zero mismatches.
Separately, the native C corpora pass **384/384 on Linux and 384/384 on Darwin**. Kernel acceptance
also checks the original Kbuild inputs, the EvoCC object provenance through generated-input
convergence and linking, and then the actual boot and runtime tests.

## Compilation checks

### Recorded acceptance evidence

The [acceptance receipt](../tests/contracts/core-v1-pilot.json) keeps original inputs, added
controls and historical failures separate. Read `source_to_ir_completion` for the source result and
`machine_lowering_checkpoint` for the later machine result. Earlier partial records stay in the
file. They are not the current completion status.

| Check | Recorded result | Limit of this evidence |
|---|---|---|
| Source-to-EvoIR replay | 32 positive pairs and 36 integer-conversion pairs accepted. One retained negative pair refused. | Acceptance of these inputs, not every C function. The general theorem applies to every pair the checker accepts. |
| Original machine-lowering replay | 68 entries from 17 modules accepted; no refusal or input/tool failure. | Actual EvoIR-to-post-SSA results. Not allocated or emitted code. |
| Private-storage machine controls | Eight entries accepted by the public checker. Separately, 108 bounded executions passed. | The added IR fixtures exercise storage. They are not presented as original C lowering. The bounded runs are tests, not universal proofs. |
| Clean Lean gate | 118 audited Core declarations, all `kernel-only`, with no native axioms in their recorded dependencies. | The modeled semantics and checker theorems. Not a kernel replay of each CLI invocation. |
| Full Rust workspace gate | 402 suites; 3,085 passed, zero failed, five pre-existing ignored campaigns. Clippy and formatting passed. | Implementation regression evidence, not a semantic theorem. Ignored campaigns are not passes. |

The final combined gates bind implementation commit `56af3157`. The
[Lean audit](../tests/contracts/lean-p0-receipt.json) records the clean source identity, Lean
4.32.2 and the theorem dependencies. The source fixtures include the unchanged `FixedMul` from
doomgeneric and `abs` from musl, plus synthetic control-flow and call cases. This does not prove
either complete application.

The negative controls include wrong conversions, changed diagnostics, incorrect edge arguments,
broken copy sequences, missing effects and corrupt caller storage. A successful sample run cannot
override a refused proof check.

To rebuild the Lean checker, run its tests and regenerate its audit receipt, run this command from
the repository root with the pinned Lean toolchain:

```sh
tools/gate-lean.sh
```

The public checker also accepts exported files directly:

```sh
lean/.lake/build/bin/evoir-sem --check-core-leaf-pair source.json target.evo
lean/.lake/build/bin/evoir-sem --check-core-loop-pair source.json target.evo proposal.json
lean/.lake/build/bin/evoir-sem --check-core-machine-pair source.json target.evo ssa.json postssa.json
```

These file names are placeholders for the actual exports and, for loops, the proposed checkpoints.
The machine command uses `source.json` to select an entry and its signature. It does **not**
replace the source-to-EvoIR check. Normal compilation does not automatically get this complete
proof chain.

### Acceptance sweep (2026-10-06)

The sweep ran proof-required mode on each of the 17 pilot modules in the
[acceptance receipt](../tests/contracts/core-v1-pilot.json) (`machine_lowering_checkpoint`). Every
function of each output was an entry. Each loop entry had its proposal file.

| Run | Result |
| --- | --- |
| 17 pilot modules, `-O0 -fno-builtin` | 17 of 17 modules accepted (exit 0). 107 of 107 entries checked from typed Core source to final assembly text (`CHECKED_TO_FINAL_ASSEMBLY`): the 68 pilot entries and 39 other functions of the same modules. No entry refused at any stage. |
| `doom-fixedmul.c` (`FixedMul`), `-Oeq-scalar --eq-policy=guard-known-true` | Accepted. One g0 guard fold (`g0_007_signed_count`) through the complete pipeline. |
| `control.c` (`loop_join`, `short_circuit`), `-Oeq-scalar` | Accepted. One P0 replacement (`p0_107_fold_icmp_ne`) through the complete pipeline. |

In `source-execution.c`, the functions `loop_join` and `control` are also entries. Their proposals
are the elements for symbols 7 and 56 of
`tests/fixtures/verification/source-execution.cutpoints.json`. Without these proposals, the stage
`source-to-evoir` refuses the module with exit 3. This is a missing input, not a defect of the mode.
The acceptance run binds implementation commit `8770b186`. At that commit, the full workspace gate
passes (407 suites, 3,102 passed, 0 failed, 5 ignored; Clippy and formatting pass), and the Lean
gate passes (receipt Lean commit `5d917f39`). The
[TODO entry for V06](../TODO.md#first-public-release-verification) records the binary digests.
After V07, the sweep passes again at `e657709b`: 17 of 17 modules, 107 of 107 entries and both
optimized cases. The 19 `output.s` files are byte-identical to the earlier output. The
[native-run receipt](../tests/contracts/core-v1-native-run.json) records this repeat.

### Native run of the checked output

The output of the 17 pilot modules was executed on Linux AArch64 with no difference from the source
interpreter in 14,194 comparisons (V08, subject `e657709b`). The run uses the existing sealed image
`evo-flight-lab:sealed`, GNU as and GNU ld 2.44, and a second build with clang-19 and ld.lld 19.
The two optimized cases give no difference in 328 comparisons. The upper-bits probe calls each entry
with nonzero upper 32 bits in each 32-bit argument register: 45,232 calls on 83 pilot entries give
no difference. The run does not compare 178 vectors with undefined behavior (each traps natively
with the same diagnostic code) and 66 vectors that reach the step limit of the source interpreter.

Not covered: no pilot entry has a 32-bit parameter on the stack; the vectors at the step limit have
no source result; a test is not a proof. The assembler, the linker, the runtime and the hardware
stay trusted: the native run tests them but does not prove them. Receipt:
[`tests/contracts/core-v1-native-run.json`](../tests/contracts/core-v1-native-run.json). Repeat with
`python3 tools/proof-native/proof_native.py all WORK_DIR` (`WORK_DIR` outside the repository).

The script also cross-checks each entry with gcc-14 `-O0` and scans the output statically for reads
of the upper 32 bits of 32-bit arguments. `WORK_DIR` must be outside the repository. The run needs
the sealed image `evo-flight-lab:sealed` and the Apple Container runtime. Without `--evocc` and
`--evocc-checker`, it uses the debug builds `target/frontend/debug/evocc` and
`target/frontend/debug/evocc-checker`.

## Verification details and limits

The [README](../README.md) gives the pipeline theorem, its three hypotheses and a short scope
paragraph. This section keeps the details, qualifiers and limits that the README of commit
`3b0f18fc` stated. The [verification contract](../spec/verification.md) is the theorem contract.
The archived [verification plan](archive/lean-plan.md#first-public-release-verification) keeps the
history of the plan.

### Proof scope

For a C subset on Linux AArch64, Lean 4 checkers give a proved, fail-closed compilation path: each
accepted output is checked from typed Core source to final assembly text. The proofs do not cover
the whole compiler, the GNU C language or Linux. On 2026-10-06, the tasks V00 to V08 are complete
in the frozen `core-v1` scope.

Lean 4 is an offline proof tool, not a code generator or a runtime dependency. It proves the
rewrite rules and the checkers of compilation results:

1. The Rust compiler stays untrusted.
2. For each stage, a small Lean checker reads the actual input and output of the stage.
3. A theorem says that an accepted pair refines. The Lean kernel checks it once, for all inputs.
4. The driver runs the compiled checkers on the actual compiler output.
5. If a check is missing or refuses, the driver publishes no output (fail-closed).

The result is a proved, fail-closed compilation path for the listed Core C-HIR subset and target,
with an explicit trusted boundary. Existing scalar rewrites compose with that path. No stage proves
that EvoCC compiles each accepted C program correctly. A valid program can be refused when proof
search finds no correspondence or exhausts its budget; a refusal never gives an unchecked output a
proof label. Ordinary output gets no proof label.

**Design rule: translation validation.** EvoCC does not prove its compiler algorithms; it checks
each result. The compiler and its searches are untrusted; they propose a result and, if necessary, a
correspondence. A small independent checker accepts or refuses, and the proof is about the checker.
This serves audit. It does not make the IRs better than LLVM or GCC, and it costs compile time.

### Refinement

*Refines* (`EvoIR.Core.Transition.Refines`) has three parts, from related entry states. (1) Each
final state of the compiled program has the observation of a final source state: a return value
with its type and bits, or a checked diagnostic `E3002` to `E3005`. (2) If the compiled program
runs forever, the source runs forever. (3) No reachable compiled state is stuck. No fuel is used.

### Stage checkers and theorems

| Stage | What the checker compares | Checker | Theorem | File |
|---|---|---|---|---|
| `source-to-evoir` | Typed Core source and EvoIR: types, order, conversions, diagnostics, loops, calls, private storage | `checkSourceTargetPair` | `check_source_target_refines` | [`Core/SourceTargetCheck.lean`](../lean/EvoIR/Core/SourceTargetCheck.lean) |
| `rewrite` | One rewrite step: EvoIR before and after, and the certificate | `checkRewrite` | `checkRewrite_refines`, `source_rewrite_refines` | [`Rewrite/Soundness.lean`](../lean/EvoIR/Rewrite/Soundness.lean) |
| `lowering` | EvoIR, SSA EvoMIR and post-SSA EvoMIR: instruction selection, then edge copies | `checkLowering` | field `checked` of its result; `source_machine_refines` | [`MachineLowering.lean`](../lean/EvoIR/MachineLowering.lean), [`MachineCorrespondence.lean`](../lean/EvoIR/MachineCorrespondence.lean) |
| `allocation` | Post-SSA and allocated EvoMIR: registers and spill slots | `checkAllocation` | `checkAllocation_sound` | [`Alloc/Soundness.lean`](../lean/EvoIR/Alloc/Soundness.lean) |
| `frame-emission` | Allocated EvoMIR and assembly AST (frame, emission); printed AST and assembly text, byte for byte | `checkAsm`, `printMatches` | `checkAsm_sound`, `source_asm_refines` | [`Asm/Soundness.lean`](../lean/EvoIR/Asm/Soundness.lean), [`Asm/Baseline.lean`](../lean/EvoIR/Asm/Baseline.lean) |

```text
Typed Core C-HIR                complete selected functions and callees
        |  source-to-EvoIR checker                      source-to-evoir
Actual EvoIR
        |  rewrite-step checker, only with -Oeq-scalar  rewrite
Rewritten EvoIR                 the same program if no rewrite ran
        |  instruction-selection checker                lowering
SSA EvoMIR                      virtual registers and block arguments
        |  edge and copy checker                        lowering
Post-SSA EvoMIR                 virtual registers and explicit copies
        |  allocation checker                           allocation
Allocated EvoMIR                physical registers and spill slots
        |  frame, emission and print checker            frame-emission
Final assembly text             the published output.s
        ---------------------  current formal boundary
        |  assembler, linker and runtime libraries
Native program                  tested; not covered by these theorems
```

`source-to-evoir` also has a source validator for types, bindings, definite initialization,
sequencing, scopes and the direct-call graph. Its theorem `checked_source_safety`
([`ValidatorSafety.lean`](../lean/EvoIR/Core/ValidatorSafety.lean)) says that an accepted source
never stops in a malformed state. The checked `abs(INT_MIN)` must report `E3004`; it cannot return
`INT_MIN`. Loop checks prove relations at checkpoints from an untrusted proposal; they do not unroll
a loop. `lowering` checks both steps against the same SSA program and entry state, and also checks
the always-on dead-code elimination and dead poison normalization.

### The pipeline theorem and the ABI entry assumption

`checkPipeline` ([`Check.lean`](../lean/EvoIR/Pipeline/Check.lean)) runs `source-to-evoir`,
`rewrite` for zero or more rewrite steps, `lowering`, `allocation` and `frame-emission` on the same
values. In words, `checkPipeline_sound` says: if the check accepts, the printed assembly AST is the
actual text. Take the selected entry function `f`, any argument list of its parameter types, and any
assembly entry state that obeys the ABI. Then the source has an initial state, and the assembly
refines the source from these two states. The theorem holds for each such argument list, not only
for tested values.

`AbiSourceEntry` ([`Asm/Soundness.lean`](../lean/EvoIR/Asm/Soundness.lean)): the caller obeys
AAPCS64 for the low bits of each argument and gives sufficient stack. Control is at the first line
of `f`; the stack pointer `E` is 16-byte aligned; `x30` holds a return address outside the program.
For an argument of width `w`, the low `w` bits of its register (`x0` to `x7`) or 8-byte stack slot
are the argument. The upper bits are free: the theorem holds for each value of them. Task V07
replaced the older `AbiEntry`, which required zero upper bits; `entryViewsOk` now refuses at
`lowering` an entry that does not read each 32-bit parameter through a 32-bit register.
`AbiSourceEntry` reads the allocated program only for the index and the stack need of `f`.

`SourceToAsmRefines` is *refines* between the source and the AArch64 semantics. The proof joins the
stage theorems with `Refines.trans`; the intermediate entry states are not hypotheses.
`checkPipelineTexts_sound` gives the same result for the files that the readers decode;
`evoir-sem --check-pipeline` runs that check.

A further axiom (from `sorry`, `native_decide` or `bv_decide`) changes the message of the
`#guard_msgs` tests, and `lake build` fails. To see the list, evaluate
`#print axioms EvoIR.Pipeline.checkPipeline_sound` in `lean/`. [`tools/gate-lean.sh`](../tools/gate-lean.sh)
runs `tools/lean/audit.py`. It fails on any `sorryAx`, classifies each audited theorem as
`kernel-only` or `native-backed`, and writes
[`lean-p0-receipt.json`](../tests/contracts/lean-p0-receipt.json).

### The Lean semantics

Each level has a small-step semantics without fuel. The theorem uses the semantics of only the first
and the last level, so only these two are trusted specifications.

| Level | Lean namespace and file | Trusted |
|---|---|---|
| Typed Core C source | `EvoIR.Core.Source` ([`Core/Execution.lean`](../lean/EvoIR/Core/Execution.lean)) | Yes: it must match the C semantics |
| EvoIR | `EvoIR.Core.IR` ([`Core/IRExecution.lean`](../lean/EvoIR/Core/IRExecution.lean)) | No: used only inside the proof |
| SSA and post-SSA EvoMIR | `EvoIR.Machine` ([`MachineExecution.lean`](../lean/EvoIR/MachineExecution.lean)) | No: used only inside the proof |
| Allocated EvoMIR | `EvoIR.Alloc` ([`Alloc/Semantics.lean`](../lean/EvoIR/Alloc/Semantics.lean)) | No: used only inside the proof |
| AArch64 subset: `x0` to `x30` with 32-bit views, `sp`, NZCV, byte memory | `EvoIR.Asm` ([`Asm/Semantics.lean`](../lean/EvoIR/Asm/Semantics.lean)) | Yes: it must match the hardware |

### The Lean project

The toolchain is `leanprover/lean4:v4.32.2`, with no dependencies and no Mathlib.
`cd lean && lake build` makes the library `EvoIR` and the executable `evoir-sem`
([`Eval.lean`](../lean/EvoIR/Eval.lean)). The counts below are those of the README of commit
`3b0f18fc`.

| Part of `lean/` | Content | Files | Lines |
|---|---|---:|---:|
| `EvoIR/Core/` | Source and EvoIR syntax, readers and semantics; validator; `source-to-evoir` | 163 | 29,854 |
| `EvoIR/Machine*.lean` | EvoMIR syntax, readers and semantics; `lowering` | 142 | 19,626 |
| `EvoIR/Asm/` | AArch64 subset, printer, `frame-emission`, baseline composition | 27 | 6,277 |
| `EvoIR/Rewrite/` | `rewrite` | 12 | 3,601 |
| `EvoIR/Alloc/` | Allocated EvoMIR, `allocation` | 10 | 3,357 |
| `EvoIR/Proofs/` | Rule and known-bits proofs | 7 | 1,301 |
| `EvoIR/Pipeline/` | Pipeline check, theorem and tests | 4 | 730 |
| Other `EvoIR/*.lean` | Rule language, registries, poison normalization, command line | 18 | 3,312 |
| Total, with the root files and two probes | | 387 | 68,193 |

### The checked subset `core-v1`

The proof input is typed Core C-HIR, not raw C text: complete function bodies, types, evaluation
order and local bindings. The target is Linux AArch64 LP64, with `-O0 -fno-builtin`. The subset has
`int`, `long`, `long long` and their unsigned forms, with distinct rank and signedness; integer
arithmetic, bitwise, comparison, conversion, conditional and short-circuit expressions, but no
signed left shift; structured control (branches, `while`, `do-while`, `for`, `break`, `continue`,
returns) with no loop bound; non-escaping scalar locals; and direct calls to defined, prototyped,
nonvariadic, non-recursive functions. The observations are return bits, the first diagnostic
`E3002` to `E3005`, and infinite execution. Floating point, pointers, heap, global memory, external
calls, volatile or atomic access and inline assembly are outside the subset; that some of them work
in the ordinary compiler does not make them proved. Source validation and the proved invariants
exclude uninitialized reads and unsupported memory faults. Not all C undefined behavior becomes a
checked native diagnostic.

| Limit | Content |
|---|---|
| ABI entry | The theorem assumes `AbiSourceEntry`. |
| Allocation | `allocation` refuses an allocation with edge bridge blocks. No pilot entry has one. |
| Rewrites | `rewrite` refuses the root-deletion (alias) certificate form, for example the P0 alias form of `p0_001_add_zero_rhs`, and an extraction with more than one instruction. It does not use facts through add, sub, mul and comparison: their result is "unknown", because their proofs use SAT reflection. |
| Options | Linux AArch64, default code generation options, `-O0 -fno-builtin`. The one optional rewrite option is `-Oeq-scalar` with `--eq-checker-sha256`. |
| Entries | Each function in the output must be a selected entry. A loop entry needs an untrusted proposal, `--proof-proposal=NAME=PATH`. Each pipeline check has one selected entry. |

### The trusted base

The Lean kernel checks the general theorems; the checker that runs is a compiled Lean executable,
and a run sends no new proof term to the kernel. These parts are trusted:

- preprocessing, parsing, elaboration and the typed source export;
- the Lean kernel, and the compiled checker with the Lean compiler, its runtime and host toolchain;
- the readers of the input files, and the printer of the assembly AST (`EvoIR.Asm.File.print`);
- the Lean semantics of typed Core C and of the AArch64 subset, and the ABI entry assumption;
- the assembler, the linker, the runtime libraries and the hardware.

The exported snapshots are not trusted: a stage checks each one, and a digest chain binds them to
the published text. Tests check the trusted parts but do not prove them.

### Proof-required mode details

`--proof-required=NEW_DIR` needs a directory that does not exist. The driver exports the inputs of
each stage into `NEW_DIR/inputs/` (with `-Oeq-scalar`, also each rewrite step). It runs `evoir-sem
--check-stage` for `source-to-evoir`, `rewrite`, `lowering`, `allocation` and `frame-emission` for
each `--core-entry` (for example `evoir-sem --check-stage source-to-evoir source=PATH ir=PATH`),
then `evoir-sem --check-pipeline` as the final decision. If all accept, the driver writes
`NEW_DIR/output.s` with the checked bytes and `NEW_DIR/record.json`. Options:
`--proof-checker-sha256`, `--proof-timeout-ms`, `--proof-proposal`.

In the record, `result` is `CHECKED_TO_FINAL_ASSEMBLY` and `scope` is "checked from typed Core
source to final assembly text". `stages` has one row for each stage and for `pipeline`, with its
identifier, theorem and input digests. `chain` shows that adjacent stages read the same bytes and
that `output.s` is the text that `frame-emission` checked. `checker` and `rewrite_checker` give the
digests of the Lean checker and the Rust certificate checker. `trusted` lists the trusted base
above.

Exit codes: 0, each check accepted each entry; 1, a command-line or compilation error before the
proof inputs exist; 3, a stage rejected an entry; 4, a proof input is missing, changed or not
readable; 5, the checker is missing, failed or timed out; 6, a construct is outside `core-v1`. On a
failure, the driver writes no `output.s` and no record. A refused rewrite step stops the
compilation, with no fallback; the Rust certificate checker also runs on each step. A private frame
object is refused, and an option that changes the text fails the `frame-emission` print check.

### EqSat proof layers and limits

The bounded scalar optimizer uses an e-graph over an admitted scalar subset; not every program goes
through it. Equivalent alternatives stay until extraction. A failed optimization rolls back the
graph and the module. This does not solve all phase ordering or promise faster code. The registry
digest pins the producer and the checker. The stage `rewrite` puts a rewrite step into the pipeline
theorem: `checkRewrite` replays each region with the tables `p0Covered` and `g0Covered`, whose
entries carry their `RuleCovered` theorems. It rebuilds the program and compares it with the given
one. It refuses any difference outside the replaced roots, a deleted obligation included.

| Layer | What it gives |
|---|---|
| Rule proofs | All 55 + 7 rules, for all widths, under their premises. Kernel-only. |
| [Known-bits proofs](../lean/EvoIR/Proofs/Transfer.lean) | Transfer functions, lattice and side conditions. The add, sub, mul and comparison rows use SAT reflection (`bv_decide`): they are **native-backed** and trust the Lean compiler and the SAT/LRAT path. The stage `rewrite` does not use them. |
| Rust `evocc-checker` | One certificate follows the allowed rules. It does not call Lean. |
| [Rust–Lean correspondence tests](../tests/contracts/lean-correspondence-receipt.json) | Agreement on recorded vectors; not a proof that Rust equals the Lean model. |

The registry decoder, the Rust checker, its parser and eligibility logic stay trusted. The rule
proofs do not prove the front end, the memory model or the backend, and do not make code smaller or
faster; the [measured effect](#checked-eqsat-measurements) is small. The
[Q2/Q3 design](verified-optimization-design.md) gates any AI search behind measured opportunity ceilings.

### Model and backend notes

EvoIR is specified in [revision 2.0.0](../spec/evoir.md) and EvoMIR in [revision 1.0.0](../spec/evomir.md); both are pinned in `spec/pins.json`.
The EvoIR models of UB, effects, memory and provenance are specified and tested; not all are
proved. SSA is block-local: each cross-block value is an explicit edge argument. `(x + 0) * 1 → x`
is a bit-vector identity under the registry premises; it does not permit removal of the evaluation
that produced `x` or apply to FP. In EvoMIR, scalar FP and SIMD share one physical register bank.
The allocation verifier checks defined register reads, live-value conflicts and flags dependencies.
An EvoMIR interpreter checks lowering; inline assembly needs native tests.

The README of commit `3b0f18fc` described EvoCC as a compiler for AArch64 on Linux and macOS. The
driver target is `aarch64-unknown-linux-gnu`; `targets/aarch64/abi-darwin` holds the Darwin arm64
ABI, and the native C corpora run on both systems ([other gates](#other-implementation-gates)).

### README examples and reproduction rows

The quick-start results of the README (13 from both interpreters, the `--check` record, the
`-Oeq-scalar` EvoIR of `f` and the `abs` excerpt with `CHECKED_TO_FINAL_ASSEMBLY`) were reproduced
on macOS with the existing debug driver at commit `3b0f18fc`; no Rust source was newer than that
binary. The emitted `demo.s` assembles for `aarch64-linux-gnu` and defines only `main`; the native
`gcc demo.s` step was not run for that revision.

The Linux row of the README reproduction table gives the `kernel-full` lane, which builds and boots
the locked configuration. The FPU image comes from the separate 7,008-object BPF/module
configuration with `TEST_FPU=m`. Its exact commands and identities are in
[`fpu-worker-verdict.out`](../tests/fixtures/fpu-worker-verdict.out) and
[`kernel-uniform-module-runtime.out`](../tests/fixtures/kernel-uniform-module-runtime.out). The
evidence command of the Doom lane above uses `--evocc target/pinned-check/release/evocc`; the README
row builds the default release binary in `target/frontend`. The game rows give the EvoCC emission
step; the link and capture steps run in the container
([game reproduction](images/showcase/README.md#game-reproduction)).

### Workflow qualifiers

The gates test the changed crates and their dependents, the full workspace, and the Lean project. A
passing changed gate is not a new application or kernel measurement. A change of the Rust pin in
`rust-toolchain.toml` selects the whole workspace. Use `CARGO_BUILD_JOBS=2` during kernel work. A
targeted fix does not close a full benchmark or kernel task without its replay.
[`render-status.py`](../tools/render-status.py) makes the [dashboard](status.md) and
[`render-figures.py`](../tools/render-figures.py) the figures; neither is live. The
[receipts](../tests/contracts/) bind each result to its compiler and inputs.

## Benchmark runner notes

These notes explain the commands of "Evaluate the roadmap" in the [README](../README.md).

`all` runs the available lanes in sequence. It is a long native evaluation, not a unit-test
shortcut. The Csmith default is 500 programs **per campaign** (1,000 total). `--budget` limits the
toybox/musl bisection probes, not the corpus size. `--allow-qemu-drift` explicitly acknowledges
the installed QEMU digest for kernel lanes. `--dry-run` prints the commands. `--evocc PATH`
measures an existing compiler without a rebuild.

`kernel` is the historical mixed-link ramp. Use `kernel-full` for complete target-C compilation,
installation, link and boot. The selftest commands do not rebuild the kernel. The examples select
the locked default configuration. Expanded configurations need explicit matching receipt, source
and runtime arguments (`--help` and the kernel ledger describe them). `kernel-kunit` uses the
boot/init workflow and the pinned upstream parser again for the separate expanded configuration:
1,504 built-in cases plus 31 actually loaded modules / 765 cases on **each** kernel. `all`
includes it; `kernel-selftest-all` does not. It does not rebuild Images or copy test disks. Supply
matching audited artifacts with `--kunit-kernel-receipt`, `--kunit-reference-boot`,
`--kunit-reference-runtime`, `--kunit-subject-runtime`, `--kunit-source-root` and
`--kunit-builtin-initramfs`. The current incremental Image and the retained module disk do not
match the older full-composition receipt. Preflight rejects that combination until the final
composition/runtime rebuild of T04. The tests of the adapter and the historical-log replay are
**not** a fresh KUnit execution. `python3 tools/k1/kunit.py --check` performs that read-only
preflight. `bench.py ... --status` shows the retained receipts and does not validate the artifacts
again.

`kernel-selftest` uses one shared reference-compiled userspace payload on both kernels.
`kernel-selftest-all` attempts the upstream `TARGETS` and `TARGETS_HOTPLUG` with `SKIP_TARGETS`
empty. Then it runs the installed programs/scripts in fresh VM disk snapshots, at most two at the
same time. It requires the provisioned lab and additional dependencies, and it can take hours.
`--reuse-selftest-build` requires a matching installed payload and still executes the tests again.
Do not use it for the initial preparation. The upstream timeouts stay, except for recorded
per-program budgets that apply equally to both kernels. Other programs without a limit have a
300-second safety watchdog, plus cleanup/boot allowances.

The script streams the lane output, keeps a summary under `.evidence/bench/`, updates
`tests/contracts/bench-receipt.json` and renders `docs/status.md`. Single-lane runs keep older
measurements with their original compiler identity. A dashboard refresh is not a fresh full run.
The script references existing evidence; it does not copy it into another archive. Exit 0 means
that the selected gates are PASS or evidenced PROFILE_CLEAN, **not necessarily that every original
passed**. Exit 1 means measured gaps/failures. Exit 2 means a runner, usage or evidence error.

Native lanes require the sealed flight-lab image and the pinned source/build inputs. Do not run
legacy lane scripts at the same time against the same build directories. For kernel work, use the
existing trees and `--low-disk`; do not make duplicate workspaces. Partial mixed-link coverage or
reduced `--programs` experiments do not close a full milestone.

## EqSat option notes

These notes keep the details of the EqSat options that the README shortens.

`-Oeq-scalar` uses the same policy-aware persistent chain as the policy evaluator. It replays each
transaction independently before it emits output. The build or installation of the `evocc` package
also makes `evocc-checker`, its default independent companion. The standalone package still gives
`evo-proofcheck`. Supply `--eq-checker-sha256=TRUSTED_SHA256` or `EVOCC_CHECKER_SHA256` from the
build or release that you trust. An observed sibling binary or the digest that a certificate
declares for itself is not a trust anchor. An explicit `--eq-checker=PATH` must match the same pin.
A missing or wrong pin refuses compilation. Without EqSat, no checker is necessary. The admitted
checker deadline is recorded, and `--eq-checker-timeout-ms=1..300000` sets it.

To keep the ordered proof bundles, add `--eq-proof-dir=NEW_DIR`. To check them again later, use
`python3 tools/bench.py --verify-eqsat-chain NEW_DIR --checker PATH`. An existing proof directory is
never overwritten. Without `-Oeq-scalar`, the same option records an explicit identity chain (zero
rewrite transactions). Audit it with `--verify-eqsat-chain NEW_DIR` and no checker; the byte
identity is reported separately from the proof replay. The standalone
`target/frontend/release/evo-proofcheck before.evoir after.evoir proof.evp2` checks one scalar
rewrite certificate. An optimization can remove work, but it cannot discard the original
undefined-behavior obligations. See the
[implementation receipt](archive/notebook/verified-eqsat-implementation-20260929.md) for the tested scope.
This does not close the whole-kernel G2 or the performance milestones.
