# EvoCC remaining work

Updated **2026-09-23**, audited against `a83d803`, EvoIR **1.9.104** and
EvoMIR **0.34**. This is the Git-tracked execution backlog, not a new test run.
Task IDs are stable: update or split an item with cross-references; do not recycle
IDs or silently delete unresolved work. Historical review IDs remain in their
original ledgers; the IDs below belong to this tracker.

The [README](README.md#status--2026-09-23) summarizes measured capability;
[docs/status.md](docs/status.md) is generated from receipts. This file answers
**what remains, what to do next, and what evidence closes it**. Detailed sources,
commands and measurements stay in the [kernel ledger][kernel],
[application ledger][apps], [semantic research brief][semantics] and
[roadmap][roadmap], rather than being copied into another evidence tree.

## How to use this tracker

- `OPEN`: diagnosis or implementation remains. `NEEDS_RULE`: a specific semantic
  decision remains; collect the bounded probes and continue independent work.
- `RETEST`: the stated prerequisite repairs exist, but the named acceptance run
  has not been completed. `LATER`: a separate roadmap capability, not a current
  kernel-boot blocker. Checked items have evidence for their **stated scope**.
- `P1`: current correctness/runtime failures, verdict integrity and minimal
  reproductions. `P2`: remaining language/application coverage. `P3`: later
  research, portability and release gates. Dependencies take precedence.
- For each fix: **RED → shared implementation → tailored tests → unchanged
  affected original → changed gate → receipt → status → commit**. Use existing
  fixtures and `tools/bench.py`; do not create another benchmark framework.
- A small-case PASS is not an original-program PASS. A reference failure, source
  UB, unsupported feature, skip or timeout is not a PASS. An evidenced disposition
  can close a *classification* task, but cannot increase the PASS numerator.
- Preserve source/flags, seeds, denominators, assertions and declared budgets.
  Do not weaken validators/UB obligations or substitute reference-built target
  objects to close an item. Spec changes must state semantics, not suppress errors.
- Update this file in the implementation commit: status, next action, fixture,
  affected-original result and evidence/commit reference. Commit bounded completed
  changes promptly; do not accumulate unrelated days of uncommitted work.
- Record an active long run's PID/container, log, compiler/Image/payload identity
  and last observation in its existing ledger. A stale receipt is not a live job.
  Keep heavy campaigns serial; reuse build trees and retain only necessary evidence.

## Execution order

1. **Reconcile known cases (T01/B01), then diagnose runtime failures K01–K05**
   using small reductions; B02 is the outstanding application timeout.
2. **Repair configuration/build gaps K06/K10–K12**, independently of pending
   language decisions. Resolve K07 and L01–L10 with bounded rules/probes; implement
   the shared paths and replay affected originals B05–B09.
3. **Finish verification/tooling T02–T05.** Only after tailored failures are fixed
   or individually dispositioned, run K13/K14 and B10 serially on identified final
   subjects. Diagnose any newly exposed class and return to a minimal reproducer.
4. **Proceed to R01–R06** as their prerequisites permit. A successful kernel boot
   or application campaign does not close the optimizer/performance/release gates.

No item below is marked actively running merely because it is queued here.

## Kernel runtime and acceptance

K6 boot is already accepted for the locked 2,799-target-C configuration. The
6,765-object BPF/module composition also links and boots. Neither establishes
complete K7/KUnit success; these configurations and their Images are distinct.

- [ ] **K01 — Diagnose the intermittent sched_ext kernel NULL fault** — `OPEN`, P1.
  `hotplug` → `reload_loop` reproduces the fault on both EvoCC and GCC; an outer
  runner exit of zero can conceal inner failures. Reduce/trace the static-key and
  scheduler-lifetime path without GDB-induced timing assumptions. Close with a
  demonstrated cause and unchanged paired replay, or an evidenced shared/upstream
  disposition; one successful retry and the failed single-thread TCG experiment
  do not close it. [Evidence][kernel].

  - [x] **K01a — Fail closed on truncated sched_ext result summaries**.
    [Minimal fixture](tests/fixtures/kselftest-scheduler-truncated.tap) reproduced
    seven false-valid assertions. The parser now retains malformed/missing/empty
    summary errors through program accounting; outer TAP results are not rewritten.
    All 219 K1 Python regressions and targeted Ruff/diff checks pass. This closes the verdict subcase,
    **not** the kernel NULL fault or the fresh paired run.

- [ ] **K02 — KVM demand_paging_test timeout** — `OPEN`, P1.
  The subject reaches the original 120-second limit after three guest modes;
  the reference completes. Profile the exact fixed-CPU/EL2 lane, isolate the hot
  path and correct any code-generation/runtime cause. Close with the complete
  original mode matrix and output checks inside the unchanged budget. [Evidence][kernel].

- [ ] **K03 — KVM memslot_modification_stress_test timeout** — `OPEN`, P1.
  The subject is interrupted in its third mode while the reference completes.
  Isolate this separately from K02 until a shared cause is demonstrated; retain
  all modes and the 120-second limit. Close with unchanged paired execution,
  not a faster isolated compiler fixture. [Evidence][kernel].

- [ ] **K04 — ublk stress/shutdown stall** — `OPEN`, P1.
  `test_stress_04.sh` hits the 45-second wrapper limit and stalls in shutdown sync;
  a later diagnostic invocation passes in 4.983 seconds. Capture the blocked I/O
  path and establish cause before changing anything. Close with the original
  workload and clean shutdown, or a source-bound non-compiler disposition;
  intermittency alone is not a fix. [Evidence][kernel].

- [ ] **K05 — PM runtime KUnit pending-resume race** — `OPEN`, P1.
  `pm_runtime_error_test` has both PASS and FAIL observations around asynchronous
  resume/barrier timing. Reduce the worker/request ordering and compare matched
  kernels. Close with an explained source/runtime or compiler cause and the
  original test's disposition; do not claim a compiler repair from a retry. [Evidence][kernel].

- [ ] **K06 — Build and run the IOPF-enabled configuration** — `OPEN`, P1.
  IOMMU is 218 PASS / 6 FAIL / 5 SKIP on both kernels; the six failures reach the
  disabled IOPF stub. The small Kconfig candidate resolves IOPF through
  `CONFIG_ARM_SMMU_V3_SVA=y` but has not been built. Compile/link with actual
  generated inputs, install matching modules, then replay unchanged
  `iommu:iommufd` on both kernels. Close on runtime evidence, not `.config`. [Evidence][kernel].

- [ ] **K07 — Define and implement nonliteral strlen ICE admission** — `NEEDS_RULE`, P1.
  Filtered FORTIFY is 25/26 PASS with two assertions left; current §10.1.18 grants
  ICE only to literal operands. Use the existing [19-line fixture][strlen] and
  [oracle record][strlen-out] to define a bounded static-constant-object rule,
  preserving ordinary-call effects and negative controls. Close with reviewed
  wording, shared classification tests and the unchanged 26-case kernel suite;
  K14 remains separate. [Decision request][semantics].

- [ ] **K08 — Disposition the seven shared EL1 breakpoint failures** — `OPEN`, P1.
  The test requests per-task kernel-address breakpoints that the ARM64 source
  rejects with `-EINVAL`. Bind each failure to the exact source/config contract
  and decide applicability or a separately identified upstream repair. Preserve
  the frozen FAILs; an unsupported hardware/API combination is not a passed test.
  Close with case-level evidence and an explicit coverage limit. [Evidence][kernel].

- [ ] **K09 — Disposition ntsync wake_all's return-value expectation** — `OPEN`, P1.
  Both kernels pass 10/11; the remaining assertion expects zero from
  `CREATE_EVENT`, which returns descriptor 9 in the capture. Verify the exact
  ioctl/test contract and upstream resolution. Close with an evidence-backed
  disposition or separately recorded test repair and paired replay; do not
  silently relax the frozen assertion. [Evidence][kernel].

- [ ] **K10 — Replay and diagnose the full BPF test_progs program** — `OPEN`, P1.
  The old subject run times out at 300 seconds after group 478. The new matching
  BTF/module payload passes selected 10-group/51-subtest sets in three variants,
  not the full program. Replay the original affected program on that payload;
  reduce any remaining failure/timeout. Close with complete nested accounting
  for both sides, retaining reference failures and skips. [Evidence][kernel].

- [ ] **K11 — Finish case-level triage of all other kselftest outcomes** — `OPEN`, P1.
  Shared FAIL/TIMEOUT/INVALID_OUTPUT/INCOMPLETE and SKIP results are not all
  explained. Map each to a source/config/runtime/compiler cause, extract a small
  reproducer per new class and assign a child ID. Close when every non-PASS has
  evidence and an action or justified limit; no blanket “both fail” exemption.
  Use the [full receipt][selftest-receipt] and [ledger][kernel].

- [ ] **K12 — Close collection-build and configuration coverage gaps** — `OPEN`, P2.
  The separate Kconfig review resolves 100 disabled requests in 27 collections;
  it is not built coverage. Review `kexec`, `prctl`, `thermal/intel/power_floor`
  and `thermal/intel/workload_hint` build gaps and architecture/hardware needs.
  Build runnable supported sets with matching payloads; give precise reasons
  for unavailable ones. Keep any changed population separate from the frozen 970. [Evidence][kernel].

- [ ] **K13 — Fresh complete paired kselftest acceptance** — `OPEN`, P1, after K01–K12/T04.
  The last 970-program pair is INCOMPLETE and bound to an older Image; its counts
  cannot be transferred to repaired Images. Run the whole installed population
  serially with matched payloads, audit nested verdicts and zero lost programs,
  and report all failures/skips separately. Close the execution task with fresh
  receipts; claim all-tests PASS only if the actual acceptance predicate holds.
  [Historical receipt][selftest-receipt].

- [ ] **K14 — Fresh complete expanded built-in and module KUnit acceptance** — `OPEN`, P1.
  After K05/K07/K08 and final composition, rerun all 1,504 built-in cases and
  actually load all 31 test modules (765 cases) on each matching kernel. Preserve
  duplicate-case multiplicity, the upstream switch-entry XFAIL and config limits.
  Later filtered FORTIFY results cannot rewrite the old full totals or inherit
  module acceptance from another Image. [Historical receipt][kunit-receipt].

## Application failures, accounting and replays

- [ ] **B01 — Reconcile every original LLVM failure with subsequent repairs** — `OPEN`, P1.
  The frozen 1,735-program run has 44 compiler failures; several originals now
  pass, so 44 is not the current defect count. Join original source/argv records
  to tailored fixes, named deferrals and approved dispositions. Close with a
  per-case residual list and a minimal repro for every distinct real failure;
  unresolved E3101 is not automatically FP. L10 handles `pr23135`. [Evidence][apps].

- [ ] **B02 — LLVM LARGE floyd-warshall performance failure** — `OPEN`, P1.
  It still times out at 60 seconds after the core shrank from 194 to 167
  instructions; `lu` and `ludcmp` now pass their original LARGE runs. Profile and
  reduce the remaining code-generation bottleneck, preserving guards and effects.
  Close on unchanged LARGE input/output checks within the original limit, not
  MINI_DATASET correctness or assembly size. [Evidence][apps].
  The [pointer-index reduction](tests/fixtures/repeated-pointer-index.c) now
  shares pure MIR index preparation and passes all interpreter/native controls
  and its changed gate; the unchanged LARGE replay still times out (166 core
  instructions). [Bounded result](tests/fixtures/repeated-pointer-index.out).

- [ ] **B03 — Account for Csmith's 68 unusable-oracle cases** — `OPEN`, P1.
  All 1,000 programs emit, 932 checksums agree and zero mismatch; the remaining
  31 base / 37 bitfield cases are not passes. Diagnose reference compile/runtime
  failures with the original seeds and budgets; retain unusable outcomes unless
  a valid matched replay exists. Close with per-case reasons and any recoverable
  comparisons, without replacing seeds. [Receipt](tests/contracts/ladder-csmith-receipt.json).

- [ ] **B04 — Resolve remaining torture/HVA reference limitations** — `OPEN`, P2.
  Torture retains three oracle-unusable originals, and GCC's HVA-varargs
  self-control failure remains reference evidence, not ABI acceptance. Diagnose
  those exact controls and record supported conclusions and limits. Close their
  accounting without calling a broken reference PASS; tiny-vector ABI is L06.
  [Torture receipt](tests/contracts/ladder-e0-receipt.json), [ABI ledger][apps].

- [ ] **B05 — Replace TinyCC's final two reference-built FP objects** — `OPEN`, P2.
  Current coverage is 9/11; the static-initializer E3101 is already repaired and
  installed. Map the two named deferred FP paths to L02–L05, repair their shared
  semantics, install the objects and run unchanged original tests. Close at
  11/11 with no replacement rebuild/symbol exclusion. [Receipt](tests/contracts/ladder-tinycc-receipt.json).

- [ ] **B06 — Finish toybox compilation and chattr diagnosis** — `OPEN`, P2.
  Coverage is 201/203 with two deferred FP objects; `chattr` also fails on the
  baseline. Treat FP implementation and filesystem/test capability as separate
  causes. Close with all objects installed and original applet results, giving
  source/filesystem evidence for any remaining inapplicability rather than a
  blanket baseline waiver. [Receipt](tests/contracts/ladder-toybox-receipt.json).

- [ ] **B07 — Finish jq's binary128 interface and original tests** — `OPEN`, P2, after L02.
  `src/builtin.c` calls `nexttoward(double, long double)`; 68/69 objects and mixed
  `make check` do not prove full EvoCC coverage. Compile/install the final object
  with the real binary128 ABI, then replay unchanged `make check` and audit that
  no target object falls back to the reference. [Receipt](tests/contracts/ladder-jq-receipt.json).

- [ ] **B08 — Build and test musl with its actual semantic flags** — `OPEN`, P2, after L01.
  Honoring `-frounding-math` currently leaves 0/1,339 units compiled; this is one missing
  capability, not 1,339 independent bugs. Keep the original flag, implement the
  required strict semantics, replay small original units, then build and run
  libc-test. Classify any newly exposed non-fenv errors separately; do not assume
  L01 alone guarantees full coverage. [Receipt](tests/contracts/ladder-musl-receipt.json).

- [ ] **B09 — Exercise cJSON's complete upstream private-helper suite** — `OPEN`, P2.
  The recorded 2/2 build and public API tests pass; they do not cover the full
  Unity/private-helper test selection named by the roadmap. Make that original
  test build available through the existing lane and execute it without changing
  assertions. Close with its exact selection/count and installed-object audit.
  [Receipt](tests/contracts/ladder-cjson-receipt.json).

- [ ] **B10 — One final serial 15-lane application campaign** — `OPEN`, P1, after tailored closure.
  Freeze the compiler/profile and run the existing `tools/bench.py` lanes without
  reducing populations. Retest passing zlib, zstd, cJSON, Mbed TLS, PolyBench, Lua
  and SQLite as well as repaired lanes. Close with fresh source-bound five-way
  accounting and zero unexplained compiler failures; deferrals/N/A/unusable
  oracles remain distinct from ALL PASS. [Current aggregate](tests/contracts/bench-receipt.json).

## Remaining numerical and language contracts

These are not proposals to redo scalar f32/f64, integer vectors, TLS,
constructors/destructors, GNU inline or raw-byte lexing. Those shared capabilities
have implementations and targeted evidence. The following boundaries remain
explicit; [the research brief][semantics] supplies the detailed probe contracts.

- [ ] **L01 — Strict floating environment** — `NEEDS_RULE`, P2; blocks B08.
  Define the finite `-frounding-math`/FENV_ACCESS contract, exceptions, rounding,
  calls and thread-owned explicit state. Implement it through constant evaluation,
  both interpreters, IR/proof metadata and native FPCR/FPSR behavior. Close with
  exact bits/flags, all admitted rounding modes and original musl representatives;
  do not reset caller FP state or merely accept the flag. [Brief §A][semantics].

- [ ] **L02 — Executable binary128 / long double** — `NEEDS_RULE`, P2; blocks B07 and FP originals.
  Qualify real binary128 semantics and target runtime helpers, then implement
  literals, operations, conversions, both interpreters and the integrated ABI.
  Close with GCC/Clang crossed calls, HFA/stack/varargs and affected originals,
  including c-testsuite's remaining case and torture's 18 binary128 deferrals.
  Binary64 substitution is forbidden. [Brief §B][semantics].

- [ ] **L03 — Binary16/bfloat16 executable boundaries** — `NEEDS_RULE`, P2.
  Bind each requested type/operation to the existing named LLVM/application
  refusal; specify its numerical and ABI contract independently. Implement only
  the approved scope and retain explicit refusals elsewhere. Close with
  bit-exact interpreter/native tests and unchanged affected programs, not type
  spelling or storage-only support. [Source classifications][apps].

- [ ] **L04 — Floating complex execution** — `NEEDS_RULE`, P2.
  Torture retains 15 floating-complex originals. Specify complex arithmetic,
  exceptional values, conversions and ABI, with required runtime helpers and
  fenv interactions. Implement shared semantics and close with tailored numerical
  and crossed-ABI tests plus original programs. Integer complex support does not
  close this task. [Brief][semantics], [classifications][apps].

- [ ] **L05 — Floating vector execution and intrinsics** — `NEEDS_RULE`, P2.
  Six torture originals and other named LLVM paths remain deferred. Define lane
  FP semantics, conversion/mask operations and required intrinsics while preserving
  `-mgeneral-regs-only` boundaries. Close through interpreter/native/ABI tests
  and affected originals; integer SIMD and scalar FP passes are not sufficient.
  [Brief][semantics], [classifications][apps].

- [ ] **L06 — Sub-8-byte integer-vector external ABI** — `NEEDS_RULE`, P2.
  GCC GPR and Clang SIMD conventions disagree and crossed controls fail.
  Choose/document a bounded ABI from the actual target contract; do not invent
  interoperability from self-calls. Close with explicit mode identity, both
  caller/callee directions for each supported reference and negative controls;
  otherwise retain the named refusal and incompatibility. [Brief §C][semantics].

- [ ] **L07 — Integer-complex division** — `NEEDS_RULE`, P2.
  `(9+7i)/(2+1i)` yields GCC `4+3i` and Clang `5+1i`; current refusal is E2948.
  Specify the algorithm, promotions, truncation, overflow and zero-divisor
  contract, then implement shared evaluation/lowering. Close with boundary and
  compound-assignment tests plus affected originals, not one selected result.
  [Brief §C][semantics].

- [ ] **L08 — Pointer-to-VLA typeof operand evaluation** — `NEEDS_RULE`, P2.
  Oracles disagree on a side-effect counter (GCC 0, Clang 1); agreed array forms,
  bound capture and allocation/lifetime cases already work. Decide expression
  categories and evaluation/capture rules, then close with scope, side-effect and
  lifetime controls and original replays. Do not reopen all VLA support.
  [Existing fixtures](tests/fixtures/variable-array-typeof-profile.c), [brief][semantics].

- [ ] **L09 — Nonzero-depth return-address support boundary** — `NEEDS_RULE`, P2.
  GCC returns zero while Clang reads an ancestor LR in the retained probe.
  Define target frame/PAC/optimization assumptions and distinguish reached-only
  unsupported execution from UB. Close with native and interpreter boundary
  tests plus original replays; never fabricate a valid ancestor address.
  [Fixture](tests/fixtures/ancestor-return-address.c), [brief][semantics].

- [ ] **L10 — LLVM pr23135 source-definedness decision** — `NEEDS_RULE`, P1.
  GCC UBSan reports vector signed overflow while Clang instrumentation is silent.
  Analyze the unchanged source and effective flags and establish an adequate
  evidence threshold. Close with either an approved case-bound disposition or a
  compiler RED/fix/replay; silence from one oracle is not proof of definedness.
  Feed the result into B01, not directly into a historical numerator. [Brief §C][semantics].

- [ ] **L11 — Inventory additional atomic scope before extending it** — `OPEN`, P2.
  Scalar atomic load/store and legacy `__sync` RMW already work. Identify actual
  remaining demand for general `_Atomic`, generic aggregate atomics and additional
  `__atomic` RMW/CAS interfaces. Split real blockers into bounded child tasks with
  width/order/provenance/native tests; keep unsupported scope explicit. This task
  closes an inventory, not arbitrary concurrent-C conformance. [Brief][semantics].

## Cross-cutting verification and tooling

- [ ] **T01 — Reconcile the historical issue list with checked-in regressions** — `OPEN`, P1.
  Map every old inline-asm, GNU89 call/prototype, VLA, relative-label/static-address,
  aggregate-varargs, pointer-qualification and E3101 case to a fixture, current
  original replay or named unresolved item. Preserve genuine boundaries such as
  narrowing-then-widening label constants; do not relabel entire previously fixed
  families OPEN. Close with no orphan known case or unexplained generic waiver.
  [Historical reductions and follow-ups][apps].

- [ ] **T02 — Put expanded KUnit behind the common bench entry point** — `OPEN`, P1.
  `tools/bench.py` has no standalone `kernel-kunit` adapter. Wrap the existing
  composition/runtime workflow, retaining built-in/module populations, Image and
  payload checks and nested verdicts. Close with adapter REDs, dry-run/status
  behavior and real K14 execution; do not invent a second runner or let `all`
  silently imply KUnit ran. [Current usage](README.md#evaluate-the-roadmap).

- [ ] **T03 — Spec/implementation/acceptance consistency audit** — `OPEN`, P1.
  Map current clauses to positive/negative tests and original evidence. Start with
  stale EvoMIR E4006/E4008 diagnostic text versus implemented indirect branches
  and pointer differences; distinguish errata from new authority. Include operand
  traversal, effects, metadata mutation and independent proof reconstruction.
  Close discrepancies or assign specific child tasks, not a blanket “spec done”.
  [EvoIR](docs/evoir-spec.md), [EvoMIR](docs/evomir-spec.md).

- [ ] **T04 — Bind final kernel claims to the actual final composition** — `OPEN`, P1.
  New scheduler code-size repairs are not installed, and existing kernel receipts
  include audited incremental compositions. Before final acceptance, freeze the
  selected compiler, actual generated inputs, every installed target object,
  Image and module/runtime hashes. Use a uniform rebuild for any single-compiler
  claim; otherwise state incremental provenance. Close with link/boot and
  replacement-object audits, without inheriting stale runtime results. [Evidence][kernel].

- [ ] **T05 — Final compiler-health and documentation gate** — `OPEN`, P1.
  After the last semantic repair, run workspace tests, Clippy `-D warnings`, fmt,
  relevant Ruff/Python checks, grammar/layout corpora and isolated offline replay.
  Keep historical ignored tests explicit. Refresh receipts, run
  `python3 tools/render-status.py`, update README/this tracker and commit the
  bounded result. Passing this gate alone does not close B10/K13/K14.
  [Workflow](README.md#layout-and-development).

## Later roadmap gates — not substitutes for runtime closure

- [ ] **R01 — G2 whole-kernel EqSat reproducibility and certificates** — `LATER`, P3.
  Produce the two required EqSat builds with equal Image hashes and independently
  check every optimized translation unit's certificate. Record exact optimization
  coverage; existing scalar proofs and unoptimized boot do not satisfy G2. [Gate][roadmap].

- [ ] **R02 — Q2 fair GCC -O2 performance evaluation** — `LATER`, P3.
  Use the same pinned programs, workloads, machine conditions and result checks;
  record compile/runtime costs and regressions. MINI_DATASET output correctness
  and code-size reductions are not performance acceptance. Close with the
  roadmap's measured comparison, not an inferred speedup. [Roadmap][roadmap].

- [ ] **R03 — Q3 measured self-improvement** — `LATER`, P3.
  Freeze the baseline, candidate policy and evaluation protocol, then demonstrate
  validated improvement on the declared workloads without correctness regression.
  Close with measured results and independent checking; more rewrites or an
  e-graph implementation alone do not establish benefit. [Roadmap][roadmap].

- [ ] **R04 — x86-64 userspace backend/ABI milestone** — `LATER`, P3.
  Implement the gated SysV/backend slice after ARM64 acceptance priorities,
  retaining shared IR semantics. Close with native userspace ABI/corpus evidence.
  Full x86 kernel support remains a separately declared stretch, not an implied
  consequence of ARM64 boot. [Roadmap][roadmap].

- [ ] **R05 — Release, reproducible bootstrap and proof-claim audit** — `LATER`, P3.
  Audit the remaining release deliverables, portable builds, packaged artifacts,
  reproducibility and proof boundaries against the roadmap. Give each missing
  deliverable a child task before release. A Rust compiler compiling a C compiler
  is not literal self-hosting; tested scalar certificates are not a proof of the
  entire compiler. Close with the actual release checklist and artifacts. [Roadmap][roadmap].

- [ ] **R06 — Pinned Doom integration** — `LATER`, P3.
  Select a specific port, legally usable data, platform shim and deterministic
  input/demo; add an adapter to the existing bench tool. Close with original
  program execution and reproducible framebuffer/output checks. First inventory
  the chosen port's real FP/platform needs; accepting scalar FP does not itself
  prove the game works. [FP integration plan](docs/notebook/floating-point-research-brief-20260915.md).

## Benchmark coverage index

Every measured application lane remains in the final campaign; already passing
lanes are regression obligations, not newly invented defects. Counts below stay
in the receipts/README so this index cannot independently drift.

| Population | Tasks controlling closure |
|---|---|
| GCC torture; c-testsuite | B04, L02/L04/L05, approved per-case dispositions, B10 |
| Csmith | B03, B10; original 500 base + 500 bitfield seeds |
| LLVM test-suite | B01/B02, L03–L10 as applicable, B10 |
| TinyCC; toybox; jq; musl + libc-test | B05; B06; B07; B08, then B10 |
| cJSON | B09, B10 |
| zlib; zstd; Mbed TLS; PolyBench/C; Lua; SQLite | Preserve recorded passing scope and rerun B10; Q2 is R02 |
| K6 locked and expanded configurations | Preserve existing boot acceptance; T04 for changed compositions |
| Full kselftest; expanded built-in/module KUnit | K01–K14, T02/T04/T05 |
| EqSat, performance, portability, release, Doom | R01–R06; not application-PASS arithmetic |

## Verified work to retain — not reopened by this list

- [x] **C01 — Locked K6 full compile/link/boot/shutdown:** 2,799/2,799 target-C,
  zero GCC target-C fallback; [receipt](tests/contracts/kernel-full-receipt.json).
- [x] **C02 — Function-section repair and BPF/module boot:** 6,765/6,765 target-C,
  745 modules linked; current signed installation and selected BTF/module tests
  verified. Full-program/runtime follow-up remains K10/K13. [Receipt](tests/contracts/kernel-bpf-full-receipt.json), [ledger][kernel].
- [x] **C03 — Selected K7 subset:** both kernels 9 programs / 125 subtests PASS;
  not the full installed population. [Receipt](tests/contracts/kernel-selftest-receipt.json).
- [x] **C04 — Retained expanded KUnit module composition:** both kernels actually
  loaded 31 modules and passed 765/765 cases. Later filtered Images require K14.
  [Receipt][kunit-receipt].
- [x] **C05 — ALSA timer userspace dependency repair:** tiny RED abort → exit 0,
  unchanged `utimer-test` 2/2 on both kernels; not an EvoCC code-generation fix.
  [Evidence][kernel].
- [x] **C06 — TinyCC static-initializer E3101 and targeted LLVM repairs:** original
  TinyCC tests after installation; `compare-fp-1/3/4`, `pr50310`, `20020720-1`,
  LARGE `lu` and `ludcmp` pass their recorded replays. Full LLVM remains B01/B10,
  and `floyd-warshall` remains B02. [Evidence][apps].

[kernel]: docs/notebook/kernel-selftest-gaps-20260914.md
[apps]: docs/notebook/benchmark-evaluation-20260915.md
[semantics]: docs/notebook/remaining-semantics-research-brief-20260921.md
[roadmap]: docs/roadmap.md
[selftest-receipt]: tests/contracts/kernel-selftest-full-receipt.json
[kunit-receipt]: tests/contracts/kernel-expanded-kunit-receipt.json
[strlen]: tests/fixtures/fortify-nonliteral-strlen-ice.c
[strlen-out]: tests/fixtures/fortify-nonliteral-strlen-ice.out
