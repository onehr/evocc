# EvoCC remaining work

Updated **2026-09-23**, initially audited against `a83d803`; current clauses are
EvoIR **1.9.105** and EvoMIR **0.36**. This is the Git-tracked execution backlog,
not a new test run.
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
- During tailored repair, retain one current reference/subject runtime pair per
  required kernel configuration. Retire superseded large payloads after checking
  open handles and preserving their build manifests/results; record that their
  exact historical runtime is no longer retained. Do not clone the workspace or
  reduce the final kernel configuration to save storage. Rebuild final artifacts
  with the repaired compiler before K13/K14; old Images cannot inherit new fixes.

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

Storage checkpoint (2026-09-23): superseded runtime disks and generated staging/
assembly caches were retired; in-place compaction recovered **18.89 GiB**.
All ten retained Image/config/runtime hashes match. Small module diagnostics now
use one 64 MiB scratch disk and existing initramfs payloads; both kernel controls
booted, ran the PM reduction and powered off. This is environment maintenance,
not K13/K14 acceptance. [Retention and recovery details][kernel].

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
  The [small signed-module matrix](tests/fixtures/sched-static-key-lifetime.out)
  now reproduces missing publication on both kernels without BPF/hotplug/freeing.
  Ordinary memory-gate and sequence-bounded static-key controls pass; instruction
  and data-barrier variants do not cure the concurrent RCU reduction. Root cause
  and linkage to the original fault remain unproved. The bounded diagnosis slice
  is recorded; next isolate read-side entry/retirement, not a production NULL guard.
  The [phase observer](tests/fixtures/sched-static-key-phases.out) now sees most
  misses after disable/grace-period/clear, even with reader preemption disabled.
  The identical payload passes all three modes on both HVF and single-thread
  TCG; only multi-thread TCG fails. This is a narrower runtime discrepancy,
  not a proven emulator cause or an unchanged sched_ext PASS.

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
  The [bounded paired profile](tests/fixtures/kvm-demand-fault-profile.out)
  reproduces a 1.49 s / 5.22 s gap at 64 MiB. A 256 KiB function graph has the
  same 87 fault-handler invocations on both kernels and no dropped events;
  excess time spans page acquisition/mapping/release, not one proven stall.
  The 4.1 MiB diagnostic reuses the 64 MiB scratch disk. No original PASS claim.
  [Integrated two-object replay](tests/fixtures/kvm-hotpath-original-replay.out):
  the repaired Image boots, but the original default workload still times out
  after three complete modes; GCC finishes all ten. K02 remains OPEN.

  - [x] **K02a — Remove redundant negated-truth materialization**, 2026-09-23.
    [Minimal C case](tests/fixtures/negated-truth-branch.c) went RED → GREEN;
    both interpreters and native GCC/Clang/EvoCC controls pass at O0/O2.
    The original `virt/kvm/kvm_main.o` reassembles with identical global symbols,
    text 243232 → 237968 bytes on controlled prior/current builds. The 1094-test
    affected gate, clippy and fmt pass. No Image was changed; K02 still needs its
    unchanged full-mode runtime replay after final rebuild. [Record](tests/fixtures/negated-truth-branch.out).

- [ ] **K03 — KVM memslot_modification_stress_test timeout** — `OPEN`, P1.
  The subject is interrupted in its third mode while the reference completes.
  Isolate this separately from K02 until a shared cause is demonstrated; retain
  all modes and the 120-second limit. Close with unchanged paired execution,
  not a faster isolated compiler fixture. [Evidence][kernel].
  The [small paired memslot profile](tests/fixtures/kvm-memslot-fault-profile.out)
  now separates invalidation and destruction: both kernels perform four page-table
  destructions and two invalidations without trace loss. Root-only tracing still
  measures 0.208 / 1.274 s (GCC / EvoCC); destruction is 3.898 / 81.148 ms, so it
  is not the entire gap. Both reduced TAP tests and shutdowns pass; original
  all-mode acceptance remains open. Next isolate the repeated page-table helpers.
  The [new linked candidate](tests/fixtures/kvm-hotpath-original-replay.out)
  installs the two gated KVM repairs but still times out in the fourth mode;
  GCC completes all ten. Both shutdowns pass; this is not K03 closure.

  - [x] **K03a — Substitute bounded private leaf helpers**, 2026-09-23.
    The [small C case](tests/fixtures/private-leaf-substitution.c) goes RED → GREEN
    through the existing C-HIR inliner, with declaration-chain barriers and
    explicit cost/lifetime/address boundaries. Interpreter/native controls and
    the 1,812-test affected gate pass. The original page-table object assembles
    with identical exports and three fewer helper calls, but text grows by 128
    bytes; no runtime improvement is inferred. No Image changed: K03 remains
    OPEN. [Full result](tests/fixtures/private-leaf-substitution.out).

- [ ] **K04 — ublk stress/shutdown stall** — `OPEN`, P1.
  `test_stress_04.sh` hits the 45-second wrapper limit and stalls in shutdown sync;
  a later diagnostic invocation passes in 4.983 seconds. Capture the blocked I/O
  path and establish cause before changing anything. Close with the original
  workload and clean shutdown, or a source-bound non-compiler disposition;
  intermittency alone is not a fix. [Evidence][kernel].

  - [x] **K04a — Preserve ublk worker failures beneath a successful wrapper**, 2026-09-23.
    The [small shell reduction](tests/fixtures/ublk-background-exit.sh) and
    fault injection into the unchanged upstream script reproduce parent exit 0
    after three child failures. The parser now retains matching inner FAIL/SKIP
    and fatal helper diagnostics, rejecting missing/malformed summaries without
    mistaking expected device I/O errors for failures. All 238 K1 and 240 ladder
    tests pass. Both original diagnostic flights pass and shut down, but the old
    stall is not reproduced or explained: **K04 remains OPEN**.
    [Scope, hashes and replay](tests/fixtures/ublk-background-exit.out).

- [x] **K05 — PM runtime KUnit pending-resume race** — `DONE`, P1, 2026-09-23.
  The [signed-module reduction](tests/fixtures/pm-runtime-barrier-completion.c)
  selects the completed-worker ordering with `flush_work`: barrier returns zero
  in all 32 such cycles on both kernels, as the frozen API documents. Both also
  pass all 64 active/suspend checks. The unchanged original replay remains GCC
  PASS / EvoCC FAIL at its unsupported assumption that resume is still pending.
  This closes **classification as a timing-sensitive upstream assertion**, not
  that original as PASS or a compiler repair. [Paired record](tests/fixtures/pm-runtime-barrier-completion.out).
  K14's fresh complete execution remains open.

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

- [x] **K08 — Disposition the seven shared EL1 breakpoint failures** — `DONE`, P1, 2026-09-23.
  All seven cases are mapped to their first task-bound kernel-address allocation.
  A [real kernel-API reduction](tests/fixtures/hw-breakpoint-kernel-task.c) verifies
  CPU-wide EL1 success and both per-task forms returning `-EINVAL` on GCC/EvoCC
  kernels; its signed diagnostic module actually loads and unloads. This closes
  **classification as an ARM64 API/test mismatch**, not those seven tests as PASS.
  The two CPU-wide original controls pass; no assertion/config was changed.
  [Paired measurement](tests/fixtures/kernel-api-contracts.out), [case table][kernel].

- [x] **K09 — Disposition ntsync wake_all's return-value expectation** — `DONE`, P1, 2026-09-23.
  Frozen documentation/implementation and the maintainer's upstream correction
  agree: `CREATE_EVENT` returns a descriptor, not zero. The [small real ioctl
  test](tests/fixtures/ntsync-create-event.c) passes both kernels, including state,
  close, EFAULT and EBADF controls. This is an **upstream assertion defect**, not
  a compiler defect or a waived PASS: the unchanged original remains 10/11.
  [Paired measurement](tests/fixtures/kernel-api-contracts.out), [source evidence][kernel].

- [ ] **K10 — Replay and diagnose the full BPF test_progs program** — `OPEN`, P1.
  The [fresh unchanged replay](tests/fixtures/bpf-complete-replay.out) on matching
  BTF/module payloads completes on GCC (651/5644 successful test/subtest counts,
  127 skips, 11 failed tests). EvoCC completes group 482 then times out at the
  unchanged 300-second limit; both guests shut down normally. Known PMU, errno,
  UDP-state and verifier-budget causes remain source-bound dispositions, not
  PASS. BBR/net_cls need K12; stacktrace and stream assumptions are now
  source-bound dispositions below. The 300-second whole-program timeout remains.
  Close with complete nested accounting on both sides after repair, not the
  earlier selected 10-group/51-subtest controls. [Evidence][kernel].

  - [x] **K10a — Reject incomplete BPF summaries and label selected replays**.
    A [small TAP fixture](tests/fixtures/kselftest-bpf-truncated.tap) and malformed,
    missing, duplicate and empty variants expose false-valid output. Complete
    counters now survive program accounting; upstream error-log replays are not
    doubled. The renderer distinguishes a one-program replay from all 970
    programs. 234 K1/240 ladder tests pass; actual serial captures still reparse
    as FAIL/TIMEOUT. No original failure is promoted to PASS.

  - [x] **K10b — Pair the known PMU/lock/UDP API reductions**, 2026-09-23.
    The signed [kernel probe](tests/fixtures/bpf-runtime-contract.c) confirms
    hardware-event ENOENT, working software counters and recursive-lock EDEADLK
    on both Images. The expanded [UDP test](tests/fixtures/bpf-udp-sockmap-state.c)
    confirms the bind prerequisite for both address families and both map types.
    Both kernels pass all five checks and unload the module. This extends earlier
    source/reference diagnosis, not a newly repaired compiler or full BPF PASS.

  - [x] **K10c — Diagnose consecutive stack build-id helper results**, 2026-09-23.
    The [raw-BPF reduction](tests/fixtures/bpf-stack-buildid-order.c) confirms
    on both Images that the first helper converts seven frames while the second
    falls back to seven IPs; reversing helper order reverses the representations.
    Frozen `mmap_unlock_work.h` explicitly allows this when IRQ-disabled lookup
    has already queued its deferred unlock. The unchanged original still fails
    its byte-equality assertion on both kernels. This closes **classification
    of an invalid test assumption**, not that original as PASS. [Record](tests/fixtures/bpf-runtime-ordering.out).

  - [x] **K10d — Diagnose the timer callback's premature stream read**, 2026-09-23.
    A [small observer](tests/fixtures/bpf-stream-callback-completion.c) uses the
    retained original BPF skeleton. On GCC its initial read returns zero and
    a later read returns the fault record; EvoCC already has the record at the
    first read. Both synchronous controls pass. The original selected group
    now passes all eight subtests on both sides, while its earlier full-run
    failure remains recorded. This is **asynchronous-completion classification**,
    not a timeout relaxation or compiler fix. [Paired source-bound result](tests/fixtures/bpf-runtime-ordering.out).

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

- [x] **B01 — Reconcile every original LLVM failure with subsequent repairs** — `DONE`, P1, 2026-09-23.
  All 44 original failure identities now have a [case-level mapping](tests/fixtures/llvm-failure-reconciliation.out):
  7 targeted original-program PASS, 30 named FP deferrals, 5 evidenced
  not-applicable and 2 OPEN (`floyd-warshall` B02, integer-vector `pr23135` L10).
  The existing adapter validates all 35 reviews against exact source/argv and
  evidence. Eight obsolete scalar-FP waivers are removed with a RED/GREEN test.
  This closes reconciliation only: L02–L05/B02/L10 remain open, and the frozen
  1,735-program run and its PASS numerator are unchanged. [Evidence][apps].

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

  - [x] **B02a — Reuse the value computed by checked add/sub**, 2026-09-23.
    The [small C reduction](tests/fixtures/checked-arithmetic-result.c) and IR
    shape test went RED → GREEN. Full-width 32/64-bit predicates keep their
    computed value across the guard using explicit block arguments; every
    obligation, trap and volatile read remains. C-HIR/EvoIR/MIR/native controls
    pass, as does the 964-test affected gate, Clippy and fmt. Original LARGE
    replay still times out at 60 s (160 versus 166 core instructions): B02 and
    K02 remain OPEN, and no kernel Image inherits this compiler-only repair.
    [Measured record](tests/fixtures/checked-arithmetic-result.out).

  - [x] **B02b — Reuse stable compared integer values**, 2026-09-23.
    The [minimal selection case](tests/fixtures/compared-value-selection.c)
    exposed both repeated loads and an independent C-HIR dereference-type
    inference refusal. Shared pointee typing is now unevaluated; bounded
    integer selections reuse already compared operands with a bit-vector mux.
    Original loads/guards remain, while volatile/atomic/call/update cases do
    not reuse their observations. Six targeted tests and six sealed native
    controls pass. The unchanged LARGE program still times out at 60 seconds,
    despite 160 → 108 core instructions: **B02 remains OPEN**. The affected
    gate passes 1,819 tests, Clippy and fmt; [record](tests/fixtures/compared-value-selection.out).

  - [x] **B02c — Select bounded compared-value muxes with CMP/CSEL**, 2026-09-23.
    [Minimal C](tests/fixtures/compared-value-csel.c) and MIR RED → GREEN cover
    96 shape/allocation cases, 600 exact executions and eight negative chains.
    Seven CLI regressions, twelve native controls and the 980-test affected
    gate pass, with Clippy/fmt. Existing evaluations, effects and guards remain.
    Original LARGE core shrinks 108 → 102 instructions but still times out
    at 60 seconds; **B02 remains OPEN**. [Record](tests/fixtures/compared-value-csel.out).

- [ ] **B03 — Account for Csmith's 68 unusable-oracle cases** — `OPEN`, P1.
  All 1,000 programs emit, 932 checksums agree and zero mismatch; the remaining
  31 base / 37 bitfield cases are not passes. Diagnose reference compile/runtime
  failures with the original seeds and budgets; retain unusable outcomes unless
  a valid matched replay exists. Close with per-case reasons and any recoverable
  comparisons, without replacing seeds. [Receipt](tests/contracts/ladder-csmith-receipt.json).
  Source-bound reductions now identify 54 reached repeating paths; 14 remain
  undetermined. The latest [invariant controls](tests/fixtures/csmith-invariant-restart.out)
  resolve eight further identities using source transitions and, for five cases,
  stopped-process values. Both interpreters and all six native controls pass.
  This is diagnosis, not a terminating original-program checksum or added PASS.

- [x] **B04 — Resolve remaining torture/HVA reference limitations** — `DONE`, P2, 2026-09-23.
  The [separate-TU HVA reduction](tests/fixtures/hva-varargs-reference.c) isolates
  GCC O2's missing register-save-area loads; stack arguments work, as do GCC O0
  and Clang O0/O2. Eight usable EvoCC/native ABI pairs and the unchanged original
  pass; the GCC O2 self-control and callee pairs remain unusable. All three
  original torture refusals are reproduced on fixed Clang 19: missing isinf
  aliases, decimal floating types/literals and GNU floating shuffle. GCC's
  originals pass O0/O2. This closes **reference-limit accounting**, not those
  programs as PASS; no source, flag, historical numerator or ABI changed.
  [Measured record](tests/fixtures/hva-varargs-reference.out); tiny-vector ABI is L06.

- [ ] **B05 — Replace TinyCC's final two reference-built FP objects** — `OPEN`, P2.
  Current coverage is 9/11; the static-initializer E3101 is already repaired and
  installed. Map the two named deferred FP paths to L02–L05, repair their shared
  semantics, install the objects and run unchanged original tests. Close at
  11/11 with no replacement rebuild/symbol exclusion. [Receipt](tests/contracts/ladder-tinycc-receipt.json).

- [ ] **B06 — Finish toybox compilation and chattr diagnosis** — `OPEN`, P2.
  Historical compilation is 201/203 with two deferred FP objects, but both old
  test logs stop after 57 occurrences at the first `chattr` failure; they are
  not full-suite acceptance. The adapter now requires the recorded object link,
  complete upstream execution, every duplicate occurrence and final object hashes.
  Close with all objects installed and original applet results; the common score
  is `VALIDATION_GAP`, not 201 validated units. [Receipt](tests/contracts/ladder-toybox-receipt.json).

  - [x] **B06a — Isolate chattr's filesystem requirement**, 2026-09-23.
    The [minimal ioctl probe](tests/fixtures/toybox-filesystem-flags.c) returns
    ENOTTY on the benchmark FUSE mount and succeeds on container-local ext4 in
    all five GCC/Clang/EvoCC controls. The unchanged 15-case chattr subset
    completes with 5 FAIL / 10 SKIP on both a real reference relink and a
    hash-audited current-EvoCC applet relink. This closes the original filesystem
    diagnosis, **not** those cases as PASS or B06's remaining full-suite work.
    [Record and limits](tests/fixtures/toybox-filesystem-flags.out).

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

- [x] **B09 — Exercise cJSON's complete upstream private-helper suite** — `DONE`, P2, 2026-09-23.
  The corrected common bench path compiles/installs **23/23** units: both library
  units and all 21 upstream Unity test TUs, including their embedded private
  cJSON implementation. All 21 programs complete: **161 PASS / 0 FAIL / 1 upstream
  IGNORE**, plus the original public test and 54 public-API controls. The pinned
  non-number test unconditionally calls `TEST_IGNORE`; four upstream disabled
  JSON-patch fixtures produce eight skip messages across two test directions.
  Neither is counted as an executed PASS. No assertions changed, no subject
  object was rebuilt/excluded; Unity alone remains a reference-built harness.
  Run `20260923T073554Z`; [receipt](tests/contracts/ladder-cjson-receipt.json),
  [acceptance and limits][apps]. Optional sanitizer/Valgrind configurations were
  not exercised; B10 still reruns this lane with the final compiler.

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

- [x] **L11 — Inventory additional atomic scope before extending it** — `DONE`, P2, 2026-09-23.
  The [source/command inventory](tests/fixtures/atomic-scope-boundaries.out)
  distinguishes SQLite's supported scalar accesses from TinyCC's uncaptured
  atomic runtime/test paths and zlib's nondefault dynamic CRC table. Three small
  [C cases](tests/fixtures/atomic-scope-boundaries.c) pass all 12 fixed-oracle
  controls; nine CLI boundaries retain E2702/E2001 refusals, not false support.
  No new current-lane target-object blocker was found. This closes the inventory,
  **not** additional atomic support; the explicit capability tasks below remain.

  - [ ] **L11a — General `_Atomic` objects and updates** — `NEEDS_RULE`, P2.
    TinyCC's bounds-checking runtime and C11 atomic test configuration expose
    this scope; fixture CASE=0 currently refuses E2702. Define object operations,
    widths, orders, volatile/provenance and library/lock-free boundaries; implement
    shared paths and verify the actual runtime configuration, not just 11/11
    compiler objects. zlib's optional dynamic-table atomic_flag is also separate.
  - [ ] **L11b — Additional `__atomic` RMW/CAS/exchange** — `NEEDS_RULE`, P2.
    CASE=1 refuses E2001. Specify success/failure orders, weak/strong CAS,
    expected-value updates, dynamic orders and width limits before extending the
    existing legacy RMW representation. Require native crossed-compiler tests;
    single-thread interpreter success is not concurrent-memory-model proof.
  - [ ] **L11c — Generic aggregate atomics** — `NEEDS_RULE`, P2.
    CASE=2 refuses E2001 for a naturally aligned eight-byte record. Specify
    representation/initializedness/provenance and lock-free versus runtime-helper
    obligations; qualify target runtime/ABI support. Scalar builtin acceptance
    must not silently admit arbitrary size/alignment or unimplemented helpers.

## Cross-cutting verification and tooling

- [x] **T01 — Reconcile the historical issue list with checked-in regressions** — `DONE`, P1, 2026-09-23.
  The [complete identity/fixture crosswalk](tests/fixtures/language-issue-reconciliation.out)
  maps the old asm, declaration, VLA, static-address, varargs and qualification
  cases without reopening repaired families. All 107 formerly unclassified
  torture E3101 failures map to 88 later PASS, 15 named deferrals and four
  evidenced exclusions; all 148 historical LLVM identities map to actual later
  results and B01's follow-ups. Source/evidence validation and 68 targeted Rust
  tests pass. This closes reconciliation, **not** L02/L05/L07–L11, B02/B10 or a
  fresh full campaign. Narrowing label constants and independent oracle limits
  remain explicit; generic E3101 is never itself an FP waiver. [Detail][apps].

  - [x] **T01a — Repair two independently reproduced inline E3101 paths**.
    [Floating builtin operands](tests/fixtures/inline-value-transport.c) now
    transport earlier values across later operands' CFG edges; [integer-complex
    parameters/results](tests/fixtures/inline-complex-parameter.c) use complete
    pair conversion, assignment and result storage. Both explicit always-inline
    cases fail on the prior release, independently of K03a's new eligibility.
    Both interpreters and all GCC/Clang/EvoCC O0/O2 native controls pass; validator
    and initializer rules are unchanged. [Measured record](tests/fixtures/private-leaf-substitution.out).

- [ ] **T02 — Put expanded KUnit behind the common bench entry point** — `OPEN`, P1.
  The `kernel-kunit` adapter now wraps the existing boot/init workflow and
  pinned upstream parser, with separate 1,504 built-in / 31-module, 765-case
  populations. Boundary, duplicate-case, stale-payload and verdict REDs pass;
  historical raw-log replay reproduces both old full results exactly without
  writing a fresh receipt. Dry-run/status are read-only; `all` explicitly includes
  the lane. Live preflight rejects the outdated composition's changed
  `arch/arm64/crypto/sha3-ce.ko`, rather than booting mismatched artifacts.
  **Real K14 execution after T04 remains required** to close T02.
  [Adapter record](tests/fixtures/kernel-kunit-adapter.out),
  [current usage](README.md#evaluate-the-roadmap).

- [ ] **T03 — Spec/implementation/acceptance consistency audit** — `OPEN`, P1.
  Map current clauses to positive/negative tests and original evidence. Start with
  stale EvoMIR E4006/E4008 diagnostic text versus implemented indirect branches
  and pointer differences; distinguish errata from new authority. Include operand
  traversal, effects, metadata mutation and independent proof reconstruction.
  Close discrepancies or assign specific child tasks, not a blanket “spec done”.
  [EvoIR](docs/evoir-spec.md), [EvoMIR](docs/evomir-spec.md).

  - [x] **T03a — Synchronize retired diagnostic boundaries**.
    Errata align E3033/E4003 with Appendix D's approved hosted 128-bit division,
    E4006 with full indirect edges and E4008 with pointer subtraction. Existing
    arithmetic guards, invalid-target checks, local SSA and runtime-library
    dependencies remain intact; no new feature is authorized by the edit.
    The 96-bit refusal test is named precisely rather than implying 128-bit
    values are rejected. The larger metadata/proof audit remains T03.

  - [x] **T03b — Remove the obsolete static-pointer emission refusal**.
    EvoMIR §8's introduction still contradicted the already-approved 0.12/0.15
    relocation rules and its own E4005 withdrawal. A text assertion reproduced
    that contradiction; the erratum now names symbolic/addend and block targets,
    without widening frontend initializer admission. All four
    [native-emission shape tests](crates/evocc-mir/tests/section8_relocations.rs)
    and four [IR relocation/address tests](crates/evocc-evoir/tests/relocations_and_addresses.rs)
    pass. This is clause consistency, not a new implementation or full T03 closure.

  - [x] **T03c — Rehash semantic proof-mutation candidates** — 2026-09-23.
    The existing [independent replay tests](crates/evocc-proofcheck/tests/copy_is_opaque.rs)
    now submit well-typed canonical changes with matching producer-authored
    output digests. All 37 mutations still fail with semantic exit 3, including
    same-priority lifecycle order, TLS address identity, FP/ABI, atomic order,
    visibility and indirect-edge bindings. Fourteen valid replay controls and
    the 44-test affected gate pass. This strengthens acceptance; no checker
    implementation defect or whole-kernel certificate closure is claimed.
    [Scope and field matrix](tests/fixtures/proof-semantic-metadata.out).

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

  - [x] **T05a — Preserve multiline benchmark command failures**.
    Six RED assertions showed that trailing newlines/comments detached log
    redirection and replaced exit 7 with exit 0 in build/relink/test wrappers.
    Shared command grouping now preserves output and status; configuration
    failures also retain their `set -e` behavior. Unit tests cover all paths.
    The invalid cJSON flight `20260923T073305Z` is superseded, not accepted.
    This repairs the harness, not all historical benchmark claims; B10 remains.

  - [x] **T05b — Run the workspace gate for manifest-only changes**.
    Five RED assertions showed that changes only to `Cargo.toml` or `Cargo.lock`
    incorrectly took the no-crates early exit, including a simulated failing test.
    Manifest selection now precedes that exit. The actual shell script is covered
    by offline stand-ins for committed/dirty changes, reverse dependencies,
    non-Cargo no-op changes and failure propagation in
    [regressions](tools/ladder/tests/test_gate_changed.py). This does not substitute
    for the final real workspace gate.

  - [x] **T05c — Reject partial or unlinked toybox acceptance**.
    REDs reproduce truncated/shared failures, duplicate-name loss, stale binaries,
    missing tests and stale bisection results. The adapter now replays the exact
    original object-link argv, audits installed objects, uses upstream
    `VERBOSE=all` with normal-return/status checks, and confirms the final selected
    build uncached. The real pinned harness and targeted chattr replay validate
    the path; historical receipts are explicitly invalidated for full coverage.
    [Regression tests](tools/ladder/tests/test_toybox.py), [record](tests/fixtures/toybox-filesystem-flags.out).

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
