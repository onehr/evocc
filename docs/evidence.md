# EvoCC evidence and status

Receipts are in [`tests/contracts/`](../tests/contracts/), and dated ledgers are in
[`docs/archive/notebook/`](archive/notebook/). The generated [status dashboard](status.md) shows the
current receipts, and [TODO.md](../TODO.md) lists the open tasks. The capture notes of the README
gallery are in [`images/showcase/README.md`](images/showcase/README.md). The
[verification contract](../spec/verification.md) states the theorem, the subset `core-v1`, the
stage checkers, proof-required mode and the trusted base.

GCC 14.2 and Clang 19.1 in the sealed image `evo-nexlab:sealed` are the reference compilers.
Linux 6.18.44 is the kernel target.

<a id="status--2026-09-28"></a>

## Status — 2026-10-27

On 2026-10-27, one compiler, built from commit `8177cff2` (`ab0c9043…` on macOS,
`3520c267…` for Linux), ran all 15 application lanes and Doom in one serial campaign, and built and
booted the direct Kbuild kernel. The compiler of release 0.1.0 writes the same bytes for 792 test
objects ([record](../tests/contracts/object-identity-0.1.0.json)); the campaign did not run again
with it. The application lanes have 10 PASS, 2 PROFILE_CLEAN and
3 INCOMPLETE, and no lane has a compiler failure. Each invocation started on a clean tree. No
population, budget, timeout, flag or disposition rule changed. Each receipt keeps its compiler and
input identity. The [application ledger](archive/notebook/benchmark-evaluation-20260915.md) and the
[kernel issue ledger](archive/notebook/kernel-selftest-gaps-20260914.md) keep the history of the
application lanes and of the kernel issues.

### Application benchmarks

`PASS` holds for the recorded build and test selection. `PROFILE_CLEAN` means zero compiler
failures after the separation of evidenced deferrals, inapplicable cases and unusable-oracle cases;
it does not mean that all originals passed. A TU count is the number of EvoCC translation units in
the tested build of the lane, not a number of tests. A mixed-link build can contain reference-built
code.

| Benchmark | Status | Recorded result | Scope or remaining issue |
|---|---|---:|---|
| GCC torture | PROFILE_CLEAN | 1,543/1,692 programs PASS | 0 compiler failures; 146 not applicable, 3 oracle-unusable |
| c-testsuite | PASS | 220/220 programs PASS | Full population, zero exclusions or deferrals |
| Csmith | INCOMPLETE | 932/1,000 checksum agreements | All 1,000 compile; 0 refusals, 0 mismatches; 68 unusable-oracle cases are not passes |
| zlib | PASS | 19/19 TU | Recorded original program tests |
| zstd | PASS | 44/44 TU | Recorded original tests; build adjustments remain in the receipt |
| cJSON | PASS | 23/23 TU | Both libraries; `cJSON_test` and 26 Unity test programs: 216 tests, 0 failures, 1 upstream IGNORE (lane log of run `20261027T114729Z`; the receipt records the 23 units) |
| Mbed TLS | PASS | 382/382 TU | Default configuration selftest and 148/148 CTest |
| TinyCC | PASS | 11/11 TU | Original `make test`; relink/test exits and installed objects audited |
| toybox | INCOMPLETE (shared test FAIL retained) | 203/203 TU installed; all 1,669 test occurrences complete | Both builds: 1,648 PASS / 1 FAIL / 20 SKIP; the failing `tar ownership` test also fails with GCC; zero EvoCC regressions |
| musl + libc-test | INCOMPLETE | 1,339/1,339 TU in the mixed build | libc-test: the 13 failures of the GCC build, and no added failure |
| PolyBench/C | PASS | 30/30 kernels | MINI_DATASET array-output correctness, not performance; reference-built allocation utility |
| LLVM test-suite | PROFILE_CLEAN | 1,066/1,735 programs PASS | 0 compiler failures; 665 not applicable, 2 FP-deferred, 2 oracle-unusable |
| jq | PASS | 70/70 TU; original `make check` PASS | Real binary128 interface; original `-pthread` preserved; no object fallback |
| Lua | PASS | 34/34 TU | Matching official `all.lua`, without soft skips |
| SQLite | PASS | 86/86 TU | Public Tcl `veryquick`: 329,803 tests; not proprietary TH3 |

The [benchmark receipt](../tests/contracts/bench-receipt.json) and the `ladder-*-receipt.json`
files bind these results to their measured subjects. Every program or unit that passed in the
receipts before this campaign passes again, except LLVM `aarch64-acle-fmv-features`: its reference
build finds compiler-rt and enables FMV, and the unchanged Clang 19 rejects its CSSC check, so the
cause is on the reference side (N09 in TODO.md). LLVM `UnitTests/Vector/Vector-build2` passes for
the first time; it was FP-deferred. `pr23135` is not applicable: its unchanged original executes a
signed overflow (L10 in TODO.md).

### Linux boot, kselftest and KUnit

All kernel rows use Linux 6.18.44 ARM64, with different configurations and Images. EvoCC compiles
the target C objects. Assembly, host build tools and userspace test programs are counted
separately.

| Scope | Verified result | Limit |
|---|---|---|
| Direct Kbuild, retained virtconfig, 2026-10-27 | **2,799/2,799 target-C** and 5 C-to-assembly helpers from one compiler build (`3520c267…`, commit `8177cff2`), compiled through `make CC=evocc` in one full build in a fresh output tree; the banner names EvoCC; stored return addresses signed (`-mbranch-protection=pac-ret`); boot PASS on HVF with pointer authentication and on TCG; 9/9 programs and 125/125 subtests PASS on both kernels | GNU tools assemble the 67 assembly files, link and build host programs; six options have no effect. The original KVM programs pass 10 of 10 modes on GCC; on EvoCC, demand paging and memslot stress both reach the 120-second limit after 6 of 10 modes. Full kselftest and KUnit not rerun |
| Same kernels, fixed 256 MiB diagnostics, 2026-10-27 | Demand paging: 65,682 page-fault handler entries on each side, EvoCC/GCC total instructions **2.59x**, kernel instructions **2.77x**. Memslot stress: 65,679 entries each, **2.59x** total and **2.77x** kernel. All four runs complete with verdict PASS | Diagnostic 1,200-second budget, not the original 120-second exit. Full translated blocks are charged at entry; concurrent waits remain. Not native timing or attribution to an individual change. [Recorded counts](../tests/contracts/kernel-direct-virt-receipt.json) |
| K6 locked configuration, 2026-09-13 | **2,799/2,799 target-C**, complete link, real boot and normal shutdown PASS; uniform EvoCC rebuild, zero GCC target-C fallback | Not every Linux configuration or subsystem |
| BPF/module configuration, 2026-09-28 | **7,008/7,008 target-C** from compiler `ab2eeb7b…`, generated-input fixed point, complete link/boot PASS; 848 modules linked/BTF/signed/installed | Objects substituted into a GCC Kbuild tree; its raw configuration and runtime pair are no longer available. Not all installed modules runtime-tested; QEMU drift recorded |
| FPU and connector follow-up, 2026-09-28 | Both kernels: FPU **1/1 + 4,000/4,000 workers PASS**; bounded connector controls **20/20 each** | The FPU Image is the BPF/module configuration with `TEST_FPU=m`, with the signed FPU module loaded; the unchanged persistent connector listener still times out under its original 45-second runner, not counted PASS |
| K7 selected kselftests, 2026-09-13 | Both kernels: **9/9 programs, 125/125 subtests PASS**, no skip/failure | Not all kselftests or KUnit |
| Full installed kselftest population, 2026-09-22 | All **970 programs started on each kernel**; aggregate INCOMPLETE; counts below | Unbuilt collections and unavailable hardware remain; the later Images need a full rerun |
| Expanded built-in KUnit, 2026-09-22 | Of 1,504 cases: EvoCC **1,432 PASS / 12 FAIL / 60 SKIP**; GCC **1,438 / 7 / 59** | Later filtered repairs are not a new full result |
| Expanded KUnit modules, 2026-09-22 | Both kernels loaded all **31 test modules**, **765/765 cases PASS** | This 6,916-target-C composition differs from the later filtered FORTIFY Images; its 809-module configuration disables module signing |

The direct Kbuild configuration is the retained GCC virtconfig after `olddefconfig` with EvoCC.
Only compiler symbols differ, the capability probes and the version text, and the receipt lists
them. The hardening features behind those probes (shadow call stack, pattern initialization,
zeroed call-used registers, KASAN, KCSAN and kernel BTI) are also off in the GCC configuration. A
return address that stays in `x30` has no signature. QEMU 11.1.2 ran both boot lanes; it differs
from the pinned QEMU contract, and the receipt allows and records this drift. The unchanged
upstream pidfd program passes 7 subtests and skips the same one on both kernels, so the receipt
keeps it INCOMPLETE.

The last full 970-program pair (2026-09-22) records these program-level verdicts. They are not
individual nested assertions:

| Kernel | PASS | FAIL | SKIP | TIMEOUT | INVALID_OUTPUT | INCOMPLETE |
|---|---:|---:|---:|---:|---:|---:|
| GCC control | 532 | 121 | 302 | 11 | 2 | 2 |
| EvoCC | 529 | 114 | 302 | 19 | 3 | 3 |

The BPF/module composition also passes all 7,008 actual-header consumers, its 849 generated export
and module units and the installed-object audits, with zero GCC target-C replacements
([composition record](../tests/fixtures/kernel-current-composition.out)). Its compiler `ab2eeb7b…` is
the pinned Rust 1.97.1 build, and its Image is `18b36fee…`. Its matching signed runtime keeps all
4,396 original non-module userspace files. The [FPU run](../tests/fixtures/fpu-worker-verdict.out)
and the [connector delivery](../tests/fixtures/connector-process-events.out) close two prerequisite
gaps of the 970-program campaign; they do not close that campaign or the expanded KUnit run.

The preceding 2026-09-28 composition used the subject Image `62c29ae4…`. Its real boot and poweroff
and its zero GCC replacement are verified. Its frozen compiler `65035621…` contains Rust 1.98.0
standard-library code, so its results are not results of the Rust 1.97.1 build or of later compiler
fixes. The [matching module runtimes](../tests/fixtures/kernel-uniform-module-runtime.out) of that
composition pass the 8/8 cases of NCI, both splice programs and the 224/0/5 inner results of IOMMU
on both kernels. This six-program selection also keeps FPU SKIP (`TEST_FPU` not enabled) and BPF
`test_kmod` TIMEOUT on both sides. 964 programs stay NOT_RUN. The 2026-09-23 checkpoint is a
one-program BPF replay, not a 970-program campaign. GCC completes with 651/5644 successful
test/subtest counts, 127 skip counts and 11 failed tests. EvoCC times out at the unchanged
300-second limit after group 482. Both shut down normally. The check of the nested `sched_ext` and
BPF summaries does not rewrite the counts.

Boot acceptance audits the installed object hashes and the generated-input stability. Then it
starts the actual Image in QEMU, checks the ordered `/init` markers and the per-run nonce, and
requires normal poweroff. Kselftests compare the two kernels with matching test payloads in fresh
per-program VMs. The CPU/EL2 requirements and the explicit timeout policies are recorded for both
sides. KUnit executes in the kernel. Built-in and module results stay separate.

Targeted repairs bring FORTIFY to **25/26 PASS** (failed assertions **96 → 2**), IOMMU on the new
uniform pair to **224 PASS / 0 FAIL / 5 SKIP**, and the ALSA timer tests to **2/2 PASS** on both. A
minimal pair of 2026-10-16, with only the test unit compiled by EvoCC, passes the unchanged 26-case
FORTIFY suite on both kernels. Three BPF variants each pass the selected 10 groups / 51 module/BTF
subtests. Not every shared failure has a diagnosis. A fresh complete 970-program pair and a
1,504-case KUnit run after those repairs are still outstanding; the KERNEL queue of
[TODO.md](../TODO.md) lists the open kernel tasks.

Receipts: [locked K6](../tests/contracts/kernel-full-receipt.json),
[BPF/module K6](../tests/contracts/kernel-bpf-full-receipt.json),
[selected K7](../tests/contracts/kernel-selftest-receipt.json),
[full kselftest](../tests/contracts/kernel-selftest-full-receipt.json),
[expanded KUnit](../tests/contracts/kernel-expanded-kunit-receipt.json).

### Language and research boundaries

Scalar `float`/`double` execution, both interpreters, AAPCS64 HFA/varargs, integer vectors, TLS,
lifecycle registration and the approved GNU extensions are implemented with targeted tests. The
scalar FP engine has **661,848** independent numerical checks across 32 operations. The strict FP
adapter, the world-threaded IR and backend, the dynamic-rounding source mode and non-trapping
lexical `FENV_ACCESS` have
[targeted value/status and native checks](archive/notebook/strict-floating-implementation-20260927.md).
The scalar `w` asm extension and its original pthread regressions are repaired. Binary128 source
and IR, both interpreters and the hosted native helpers and ABI have targeted tests. Floating
complex and vector types and the other refused forms stay outside the current source profile.

The [remaining-semantics brief](archive/notebook/remaining-semantics-research-brief-20260921.md)
itemizes other measured dialect boundaries. The tiny-vector external ABI, integer-complex division,
pointer-to-VLA type queries and nonzero-depth return addresses follow decided rules (TODO L06 to
L09).

No accepted result closes whole-kernel EqSat reproducibility and certificates (G2, R01), the
object-code size comparison with GCC and Clang (Q2, R02), measured self-improvement (Q3, R03), the
x86-64 backend (R04) or the release audit (R05). Scalar proof results and a successful boot do not
close those milestones.

### Doom integration

On 2026-10-27, the separate headless Doom lane built all **80 engine C units plus one platform
adapter with EvoCC**, without target-C object fallback. It pins doomgeneric
`dcb7a8dbc7a16ce3dda29382ac9aae9d77d21284` and Freedoom 0.13.0 Phase 1. Two bundled demos run two
times each under EvoCC, GCC14 and Clang19: **12 executions, 350 frames each**, with exact
corresponding framebuffer SHA-256 sequences. The runtime libraries, the assembler and the linker
stay the existing tools of the sealed target. This is not a self-contained EvoCC libc build.

The declared profile is GNU11/O2 with **`-fwrapv`**: the original signed `ANG45` multiplication
overflows before the conversion to unsigned. A separately recorded one-line port repair initializes
the previous screen before the initial melt. Without it, the repeated runs of GCC itself have
nondeterministic initial frames. The lane skips no compiler guards and no mismatching frames. The
profile replaces Xlib with a deterministic 320x200 headless adapter. Audio, live keyboard/window
play and other maps/ports are **not tested**. Doom is not one of the 15 application lanes.

```sh
python3 tools/bench.py doom --jobs 2
python3 tools/bench.py doom --status
```

Without `--evocc`, the first command builds `target/release/evocc` and measures it. It requires the
existing sealed Apple Container image and the exported ARM64 headers. It downloads about 26 MiB of
pinned source/data if they are absent. It creates no kernel VM disk. The binaries, a few PPM
snapshots and the detailed frame hashes stay in `.build/ladder/doom`. The Linux ARM64 executable is
`.build/ladder/doom/evocc/doom`. It streams frames and does not open a window. See the
[receipt](../tests/contracts/ladder-doom-receipt.json) and the
[scope, failed attempts and checks](../tests/fixtures/doom-headless.out).

## Checked EqSat measurements

| Date | What ran | Numbers | What it does and does not establish |
|---|---|---|---|
| 2026-09-29 | Shared verified chain ([R01a](archive/notebook/verified-eqsat-implementation-20260929.md), [requalification](archive/notebook/eqsat-review-closure-20260929.md)) | Driver and evaluator run one chain; 45 legal transaction states; checker pinned by SHA-256; full workspace 2,850 tests, 0 failures | Optimized output cannot be emitted without independent replay. Integration found and fixed two real producer defects. |
| 2026-09-30 | Bounded Linux pilot ([protocol and results](archive/notebook/eqsat-r02-linux-pilot-20260930.md), [receipt](../tests/contracts/eqsat-r02-pilot-receipt-20260930.json)) | 576 / 576 result-checked executions; 138 certificates replayed, 0 rejections; 108 checked/unchecked twins; default search and certificate caps hit 0 times | The pipeline runs end to end on zlib, zstd and cJSON. Millisecond fresh-process timing was uninformative by construction. |
| 2026-09-30 | Steady-state timing ([protocol](archive/notebook/eqsat-r02-steady-protocol-20260930.md)) | A/A 10 / 10 admitted; A/B run once: 10 / 10 **INCONCLUSIVE** at a 5% floor; frozen, not rerun | No latency improvement, regression or equivalence is claimed. Latency is `NOT_MEASURABLE_ON_THIS_LANE`; size is the V1 objective. |
| 2026-09-30 | Deterministic preflight ([adjudication](archive/notebook/eqsat-r02-review-repair-20260930.md)) | 1,024 policy evaluations, 6,144 chains, 8,854 proofs replayed; 1,222 candidate rules attempted: 1,174 proved, 48 unknown, **0 installed** | Policy, rule and shortcut tracks are all **INCONCLUSIVE / NOT_ADMITTED**. No AI campaign is admitted; proving rules was never the bottleneck. |
| 2026-10-01 | Object-level size ([S1–S4 ledger](archive/notebook/eqsat-size-work-packages-20261001.md), [DCE control](../tests/contracts/eqsat-size-dce-20261001.json), [policy ranking](../tests/contracts/eqsat-size-policy-ranking-20261001.json)) | see table below; exhaustive best policy beats the default by at most 4 bytes | Verified rewrites shrink object `.text` by 0.8–2.4% on the changed subject TUs. The policy weight space is `FINITE_ENUMERATION_SUFFICIENT` for size. |
| 2026-10-01 | Guard-folding ceiling ([result](../tests/contracts/eqsat-size-guards-result-20261001.json), [guard contract](../spec/evoir.md#812-certified-total-integer-guard-folding), [archived draft](archive/eqsat-guard-folding-draft.md)) | Unverified diagnostic recovers 952 / 1,608 / 80 bytes (zlib / zstd / cJSON) from provably-true UB checks | A diagnostic ceiling for R02e, not a certified benefit. Nothing from the diagnostic is installed. |
| 2026-10-01 | Certified guard first slice ([acceptance](archive/notebook/eqsat-certified-guards-20261001.md)) | Seven g0 rules and seven new A2 rows are kernel-only; ten certificates replay; checked text saves 256 / 984 / 28 bytes in zlib / zstd / cJSON | First-slice acceptance on six original TUs. Maximum per-TU saving is 0.800%; unchecked sizes are unchanged. Diagnostic and P0 savings remain separate. |
| 2026-10-27 | Lean audit of `p0.scalar.v4` and `g0.ub.v1`, Lean commit `bd31621f` ([receipt](../tests/contracts/lean-p0-receipt.json)) | 55 / 55 and 7 / 7 rules proved, kernel-only | Rules are sound equalities of the modeled semantics. Not a proof of the compiler. |

Checked P0 EqSat on the subject translation units, in object `.text` bytes of the checked profile,
with the common dead-code elimination on both sides (A: EqSat off; B: EqSat on, with the same
lowering and the same backend):

| Subject | A | B | Change | Changed TUs | Eligible roots |
|---|---:|---:|---:|---:|---:|
| P0 integer mix | 128 | 128 | 0 | 0 of 1 | 13 |
| zlib `deflate.c` + `inflate.c` | 58,920 | 58,448 | −472 (−0.8%) | 2 of 2 | 3,356 |
| zstd `zstd_compress.c` + `zstd_decompress.c` | 157,712 | 156,272 | −1,440 (−0.9%) | 2 of 2 | 10,298 |
| cJSON `cJSON.c` | 30,228 | 29,496 | −732 (−2.4%) | 1 of 1 | 1,787 |

The checked P0 domain is sound and reproducible, and its measured effect is small, because the
domain is small. P0 cannot touch the checked UB guards (0.4–5.5% of object `.text` on these TUs,
measured as checked minus unchecked), memory operations, inlining or control flow. The
[six-TU GCC/Clang size comparison](archive/notebook/eqsat-size-references-20261001.md) records larger
EvoCC objects.

### EqSat proof layers

The bounded scalar optimizer uses an e-graph over an admitted scalar subset; not every program goes
through it. A failed optimization rolls back the graph and the module. The registry digest pins the
producer and the checker.

| Layer | What it gives |
|---|---|
| Rule proofs | Each rule holds for all widths under its premises. |
| [Known-bits proofs](../lean/EvoIR/Proofs/Transfer.lean) | Transfer functions, lattice and side conditions. The add, sub, mul and comparison rows ([carry](../lean/EvoIR/Proofs/TransferCarry.lean), [mul](../lean/EvoIR/Proofs/TransferMul.lean), [compare](../lean/EvoIR/Proofs/TransferCompare.lean)) use SAT reflection (`bv_decide`): they are **native-backed** and trust the Lean compiler and the SAT/LRAT path. The stage `rewrite` does not use them. |
| Rust `evocc-checker` | One certificate follows the allowed rules. It does not call Lean. |
| [Rust–Lean correspondence tests](../tests/contracts/lean-correspondence-receipt.json) | Agreement on recorded vectors; not a proof that Rust equals the Lean model. |

The registry decoder, the Rust checker, its parser and its eligibility logic stay trusted. The rule
proofs do not prove the front end, the memory model or the backend.

## Other implementation gates

The [Rust–Lean correspondence tests](../tests/contracts/lean-correspondence-receipt.json) record
**1,487,689 / 1,487,689** agreeing comparisons: **1,325,045** across the five P0 groups plus
**162,644** guards. All six groups were rerun together on 2026-10-01. The
[Lean audit receipt](../tests/contracts/lean-p0-receipt.json) binds the registry digests, the Lean
commit and the pinned revision of `spec/evoir.md`, and lists the trusted parts.

The recorded scalar and MIR gates each cover **21,000 generated IR programs** with zero mismatches,
and the native C corpora pass **384/384 on Linux and 384/384 on Darwin**
([semantic gate receipt](../tests/contracts/semantic-gate-contract.json)). Kernel acceptance also
checks the original Kbuild inputs, the EvoCC object provenance through generated-input convergence
and linking, and then the actual boot and runtime tests.

### Lean project size

Files and lines of `lean/` at commit `3ce2bdaf`:

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

## Compilation checks

### Recorded acceptance evidence

The [acceptance receipt](../tests/contracts/core-v1-pilot.json) keeps original inputs, added
controls and historical failures separate. `source_to_ir_completion` holds the source result and
`machine_lowering_checkpoint` the later machine result, both of 2026-10-05. The earlier partial
records in the file are not the completion status.

| Check | Recorded result | Limit of this evidence |
|---|---|---|
| Source-to-EvoIR replay | 32 positive pairs and 36 integer-conversion pairs accepted. One retained negative pair refused. | Acceptance of these inputs, not every C function. The general theorem applies to every pair the checker accepts. |
| Original machine-lowering replay | 68 entries from 17 modules accepted; no refusal or input/tool failure. | Actual EvoIR-to-post-SSA results. Not allocated or emitted code. |
| Private-storage machine controls | Eight entries accepted by the public checker. Separately, 108 bounded executions passed. | The added IR fixtures exercise storage. They are not presented as original C lowering. The bounded runs are tests, not universal proofs. |
| Clean Lean gate | 118 audited Core declarations, all `kernel-only`, with no native axioms in their recorded dependencies. | The modeled semantics and checker theorems. Not a kernel replay of each CLI invocation. |
| Full Rust workspace gate | 402 suites; 3,085 passed, zero failed, five pre-existing ignored campaigns. Clippy and formatting passed. | Implementation regression evidence, not a semantic theorem. Ignored campaigns are not passes. |

The final combined gates bind implementation commit `65237f75`. The source fixtures include the
unchanged `FixedMul` from doomgeneric and `abs` from musl, plus synthetic control-flow and call
cases. This does not prove either complete application.

The negative controls include wrong conversions, changed diagnostics, incorrect edge arguments,
broken copy sequences, missing effects and corrupt caller storage. A successful sample run cannot
override a refused proof check.

The public checker also accepts exported files directly:

```sh
lean/.lake/build/bin/evoir-sem --check-core-leaf-pair source.json target.evo
lean/.lake/build/bin/evoir-sem --check-core-loop-pair source.json target.evo proposal.json
lean/.lake/build/bin/evoir-sem --check-core-machine-pair source.json target.evo ssa.json postssa.json
```

The file names stand for the actual exports and, for loops, the proposed checkpoints. The machine
command uses `source.json` to select an entry and its signature. It does not replace the
source-to-EvoIR check.

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
The acceptance run binds implementation commit `eed7f2f3`. At that commit, the full workspace gate
passes (407 suites, 3,102 passed, 0 failed, 5 ignored; Clippy and formatting pass), and the Lean
gate passes (receipt Lean commit `b64931fb`). The
[closed tracker entry of V06](archive/todo-0.1.0.md#v06) records the binary digests. After V07, the
sweep passes again at `e85f5904`: 17 of 17 modules, 107 of 107 entries and both optimized cases.
The 19 `output.s` files are byte-identical to the earlier output. The
[native-run receipt](../tests/contracts/core-v1-native-run.json) records this repeat.

### Native run of the checked output

On 2026-10-06 (V08, subject `e85f5904`), the output of the 17 pilot modules ran on Linux AArch64
with no difference from the source interpreter in 14,194 comparisons. The run used the sealed image
`evo-flight-lab:sealed`, the earlier tag of `evo-nexlab:sealed` with the same digest, GNU as and
GNU ld 2.44, and a second build with clang-19 and ld.lld 19. The two optimized cases give no
difference in 328 comparisons. The upper-bits probe calls each entry with nonzero upper 32 bits in
each 32-bit argument register: 45,232 calls on 83 pilot entries give no difference. The run does
not compare 178 vectors with undefined behavior (each traps natively with the same diagnostic code)
and 66 vectors that reach the step limit of the source interpreter.

Not covered: no pilot entry has a 32-bit parameter on the stack; the vectors at the step limit have
no source result; a test is not a proof. The assembler, the linker, the runtime and the hardware
stay trusted: the native run tests them but does not prove them. Receipt:
[`tests/contracts/core-v1-native-run.json`](../tests/contracts/core-v1-native-run.json). Repeat with
`python3 tools/proof-native/proof_native.py all WORK_DIR`, with `WORK_DIR` outside the repository.

The script also cross-checks each entry with gcc-14 `-O0` and scans the output statically for reads
of the upper 32 bits of 32-bit arguments. The run needs the image `evo-nexlab:sealed` and the Apple
Container runtime. Without `--evocc` and `--evocc-checker`, it uses the debug builds
`target/debug/evocc` and `target/debug/evocc-checker`.

## README examples

On 2026-10-27, the quick start and the tutorial "Build your first optimization" of the README of
commit `8177cff2`, and the first-program pages of the book, ran again on macOS with the release
binaries of that commit (`evocc` `ab0c9043…`). Each output equals the text, after three text
updates: the checked `abs` keeps its zero operand in `w1`, the interpreter reports `shout.c` with a
rendered diagnostic, and the first 24 assembly lines of `sum.c` changed. The Lean command of the
tutorial did not run in this replay. On the same day, GCC 14 in `evo-nexlab:sealed` compiled
`first_bad.c` with `-O2 -Wall -Wextra` and gave no warning, and the program did not finish within 10
seconds. Apple clang gave no warning, and its program stopped with a trap (exit status 133). GCC 14
with `-Wall -Wextra -fsanitize=undefined` compiled `mean.c` with no warning, and the program gave no
report. It returned 0 at `-O0`, and 34, 226, 188, 87 and 178 in five runs at `-O2`. With the CMake
toolchain file, cJSON 1.7.19 passed 22 of 22 CTest tests and zlib 1.3.2 passed 16 of 16.

## Benchmark runner notes

`python3 tools/bench.py all` runs the available lanes in sequence. The Csmith default is 500
programs **per campaign** (1,000 total). `--budget` limits the toybox/musl bisection probes, not the
corpus size. `--allow-qemu-drift` acknowledges the installed QEMU digest for kernel lanes.

`kernel` is the historical mixed-link ramp. Use `kernel-full` for complete target-C compilation,
installation, link and boot. The selftest commands do not rebuild the kernel. Expanded
configurations need explicit matching receipt, source and runtime arguments (`--help` and the
kernel ledger describe them). `kernel-kunit` uses the boot/init workflow and the pinned upstream
parser again for the separate expanded configuration: 1,504 built-in cases plus 31 actually loaded
modules / 765 cases on **each** kernel. `all` includes it; `kernel-selftest-all` does not. It does
not rebuild Images or copy test disks. Supply matching audited artifacts with
`--kunit-kernel-receipt`, `--kunit-reference-boot`, `--kunit-reference-runtime`,
`--kunit-subject-runtime`, `--kunit-source-root` and `--kunit-builtin-initramfs`. The current
incremental Image and the retained module disk do not match the older full-composition receipt.
Preflight rejects that combination until the final composition/runtime rebuild of T04. The tests of
the adapter and the historical-log replay are **not** a fresh KUnit execution.
`python3 tools/k1/kunit.py --check` performs that read-only preflight.

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

Native lanes require the sealed image and the pinned source/build inputs. Do not run legacy lane
scripts at the same time against the same build directories. For kernel work, use the existing trees
and `--low-disk`; do not make duplicate workspaces. Partial mixed-link coverage or reduced
`--programs` experiments do not close a full milestone.

## EqSat option notes

`-Oeq-scalar` uses the same policy-aware persistent chain as the policy evaluator, and the
independent checker replays each transaction before the driver emits output. Take the digest for
`--eq-checker-sha256` or `EVOCC_CHECKER_SHA256` from the build or release that you trust. An
observed sibling binary or the digest that a certificate declares for itself is not a trust anchor.
An explicit `--eq-checker=PATH` must match the same pin. A missing or wrong pin refuses
compilation. Without EqSat, no checker is necessary. `--eq-checker-timeout-ms=1..300000` sets the
checker deadline, and the chain record keeps it.

`--eq-proof-dir=NEW_DIR` keeps the ordered proof bundles; an existing directory is never
overwritten. `python3 tools/bench.py --verify-eqsat-chain NEW_DIR --checker PATH` replays them
later. Without `-Oeq-scalar`, the same option records an explicit identity chain (zero rewrite
transactions). Audit it with `--verify-eqsat-chain NEW_DIR` and no checker; the byte identity is
reported separately from the proof replay. `target/release/evo-proofcheck before.evoir after.evoir
proof.evp2` checks one scalar rewrite certificate. The
[implementation notes](archive/notebook/verified-eqsat-implementation-20260929.md) give the tested
scope.
