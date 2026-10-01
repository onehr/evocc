# EvoCC remaining work

Updated **2026-10-01**, initially audited against `a83d803`; current clauses are
EvoIR **1.9.115** and EvoMIR **0.54**. This is the Git-tracked execution backlog,
not a new test run.
Task IDs are stable: update or split an item with cross-references; do not recycle
IDs or silently delete unresolved work. Historical review IDs remain in their
original ledgers; the IDs below belong to this tracker.

The [README](README.md#status--2026-09-28) summarizes measured capability;
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
- Work in **coherent repair batches**, not one full campaign per small patch:
  enumerate the batch's REDs/decisions first; then shared implementation and
  focused tests per fix, inexpensive unchanged-original replays, affected-crate
  gates at integration points, receipts/status and bounded commits. Once all
  batch prerequisites are green or explicitly dispositioned, freeze one subject
  and run the complete acceptance campaigns serially. A new failure returns to
  a small reproducer, not an automatic rerun of every already-green campaign.
  Use existing fixtures and `tools/bench.py`; do not create another framework.
  The [September 27 batch packet][closure-batch] indexes every open task, current
  evidence, genuine decisions and required final gates. Ordinary implementation
  steps do not require another user approval; new semantic authority does.
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

September 28 replan at `6bd69b7`: prioritize **bounded ARM64 acceptance**, then
the remaining language breadth, then the wider research/release roadmap. This
is sequencing, not approval to drop features or convert failures into passes.
The project has working boot and substantial application coverage, but the full
roadmap is not just a few cleanup fixes. T01/B01, K05/K06/K08/K09, L01/L02,
B05–B07/B09 and R06 already have their stated closure evidence; do not schedule
them again as missing implementations.

The queue below is checked against every unchecked task by
`python3 tools/render-status.py --todo`. It counts **tracker entries, including
parents and children**, not independent bugs, effort or a completion percentage.
Scheduling does not launch a background worker. No compiler/kernel test process
was found in the read-only process check at this planning checkpoint.

<!-- closure-queues:start -->
| Queue | Open IDs | Next deliverable |
| --- | --- | --- |
| KERNEL | K01, K01b, K02, K03, K04, K10, K11, K11f, K12 | Start with K02/K03's existing current-Image profiles; produce a cause-backed hot-path reduction, then address BPF/pidfd budgets and intermittent sched_ext/ublk failures. |
| APPLICATION | B08, B08b, B08d, B08d10 | Implement the retained warning policies, then finish the separately patched wordexp lane and reference-case accounting; full musl also needs L04. |
| REVIEW | K07, L03, L04, L05, L06, L07, L08, L09, L10, L11a, L11b, L11c | Resolve K07/L10 and L04 first; use the existing bounded decision packet, not new broad FP research. |
| AUDIT | T03 | Existing compiler semantic/proof boundaries. R02b1/R02a1 are closed with frozen results; no further EqSat audit-tool expansion. |
| ACCEPTANCE | T04, T02, K13, K14, B10, T05 | Freeze repaired subjects, build matching compositions once, run complete paired kernel and serial application gates, then refresh final health/docs. |
| LATER | R01, R02, R02c, R02e, R03, R03a, R03b, R03c, R03d, R03e, R03f, R03g, R03h, R04, R05 | Execute the approved S1–S6 size work packages in order. R03 V1 negative-admission study is frozen; positive Q3 remains unachieved and R03a/b/c/d NOT_ADMITTED, R03e deferred. No VM timing or audit expansion; x86-64 and release stay separate. |
<!-- closure-queues:end -->

### Working sequence and release gates

| Stage | Work and dependency | Exit / stop condition |
| --- | --- | --- |
| 0 — Reconcile (this checkpoint) | Current receipts, task queues, live jobs, retained subjects and disk headroom. | Every open ID scheduled exactly once; historical PASS stays bound to its own subject. |
| 1 — Engineering batch | K02/K03 measured hot-path diagnosis first; B08b warning policy is independent CPU-light work. Follow with K10/K11f and K01/K04 reductions. | An exact RED, a shared repair or source-bound disposition, focused tests and unchanged affected-original replay per class. A code-size win alone is not a runtime repair. |
| 2 — Reviewed language batch | K07 unlocks FORTIFY/KUnit; L10 resolves LLVM's remaining classification; L04 unlocks 68 musl objects. Implement those once their bounded rules are approved. Then L03/L05–L09/L11a–c. | Both interpreters, native/crossed ABI and metadata negatives as applicable, followed by original-source replay. No silent expansion of the currently approved profile. |
| 3 — Kernel acceptance | Kernel-affecting repairs/dispositions + K07 + T03 → T04. Build the BPF/selftest and independent expanded-KUnit configurations with matching module payloads. | K13's complete 970-program pair and K14/T02's 1,504 built-in + 765 cases in 31 loaded modules per side. Non-PASS cases retain individual reasons and zero missing results. |
| 4 — Application acceptance | Relevant language/option fixes + B08 reference/patched-lane accounting + T03 → freeze one compiler/profile. | B10's 15 lanes serially, unchanged populations and original flags. Report originals, patched supplements and approved deferrals separately. Finish T05 on the same source. |
| 5 — Full roadmap | After ARM64 correctness stabilizes: R01 → R02 → R03; R04 is a separate backend project; R05 audits the whole delivered scope. | Actual optimization/proof/performance/backend/release evidence, not a renamed ARM64 milestone. |

Stages 1 and the review work in stage 2 are independent workstreams, **not a
request to run multiple heavy jobs at once**. Kernel acceptance does not wait
for floating complex, half/vector FP, generic atomics or x86-64 unless an actual
kernel source requires them. Conversely, K13/K14 cannot close musl or B10.
No calendar completion date is asserted before intermittent failures have
reliable reductions and the numerical decisions have an accepted contract.

Immediate boundaries that affect the schedule:

- **K02/K03:** the current pair times out at 7/10 reference versus 3/10 subject;
  the retained smaller trace has 87 matching fault-handler entries and zero
  dropped events, but cost is distributed. Inspect that trace and emitted hot
  functions before another speculative peephole or 7,008-object rebuild. Keep
  the 1 GiB / ten-mode / 120-second original gate; do not replace it with a
  quiet-host guess or the passing reduced workload.
- **B08b:** warning requests already survive both adapters. Current faithful
  musl commands therefore refuse unsupported warnings; the older 1,271/1,339
  receipt contains omissions. The next job is actual diagnostic policy and
  diagnostics, **not another projection fix** or accepting `-Werror` as a no-op
  ([GCC 14.2 warning policy](https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Warning-Options.html)).
- **Review:** the supplied scalar/strict/binary128 report closed L01/L02, not
  floating complex or the other pending decisions. Use the existing
  [focused handoff][complex-prompt]. No fresh user approval is needed for ordinary
  engineering under an existing contract; genuine new language/ABI choices
  remain explicitly review-gated.
- **Storage/runtime:** currently about 61 GiB host free at preflight. Reuse the
  existing small diagnostic payloads and 64 MiB scratch disk. Keep one heavy
  run active, check required headroom before launches, and never delete active
  or sole retained evidence. A new configuration/build must first identify what
  it replaces and which historical artifacts will remain recoverable.
- **Stop repeating uninformative experiments:** each bounded diagnostic must
  name the competing causes and the observation that distinguishes them. If
  that observation cannot be obtained, record the specific missing capability
  and switch to an independent ready task; do not count retries as fixes.

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
  September 26's identical-payload MT-TCG `-d nochain` control still fails the
  static and preemption-disabled modes on both Images (memory controls pass).
  Direct-block chaining is not a necessary cause; the control is not a repair.

  - [x] **K01a — Fail closed on truncated sched_ext result summaries**.
    [Minimal fixture](tests/fixtures/kselftest-scheduler-truncated.tap) reproduced
    seven false-valid assertions. The parser now retains malformed/missing/empty
    summary errors through program accounting; outer TAP results are not rewritten.
    All 219 K1 Python regressions and targeted Ruff/diff checks pass. This closes the verdict subcase,
    **not** the kernel NULL fault or the fresh paired run.

  - [ ] **K01b — Resolve the concurrent instruction-publication runtime discrepancy** — `OPEN`, P1.
    The [userspace C reduction](tests/fixtures/instruction-publish-alias.c)
    removes kernel static keys, RCU, hotplug and EvoCC-generated test code.
    The first MOV probe exceeded Arm's concurrent-modification boundary; its
    evidence is explicitly qualified, not erased. The corrected direct-B probe
    still fails all six concurrent modes on both kernels under MT-TCG, including
    aliases and reader ISB. All six same-protocol memory-gate, locked-MOV and
    handshake controls pass; all 12 modes pass on HVF and diagnostic ST-TCG.
    First bad snapshots contain a branch to return zero but observe return one.
    This is a concrete shared runtime boundary, not yet a
    localized QEMU fix or proof of the original sched_ext cause. Diagnose the
    publication/invalidation path, then replay the unchanged original. Do not
    silently change the locked lane or waive the failure. [Full matrix](tests/fixtures/instruction-publish-alias.out).
    A [read-only target observer](tests/fixtures/instruction-publication-observer.out)
    now sees actual entry into the immutable return-one path after the disabling
    code/data stores. Its **196224/320384** witnesses exactly match guest misses
    on GCC/EvoCC MT-TCG; both ST-TCG controls pass 6/6 with zero witnesses.
    Instruction-data API samples reread memory and are not decoded-opcode proof;
    the target witness avoids that inference. This localizes the reduced wrong
    control flow, not its exact invalidation race or the original NULL fault.
    The store-window extension correlates **25792/8448** wrong-target executions
    with translations observed during a pending patch store; both ST controls
    still pass. It does not expose the internal decode/invalidation ordering,
    so that observer alone was insufficient. The [matching-version internal
    experiment](tests/fixtures/qemu-instruction-publication.out) now captures
    actual old-instruction fetches after invalidation. A private, address-independent
    four-byte-store intervention makes all six userspace and three static-key
    cases pass on both Images. It is not a general emulator fix. Both baseline
    and intervention pass the latest original hotplug/reload sequence, so that
    intermittent NULL fault is not yet causally closed. Production QEMU and
    full-run acceptance remain unchanged.

    September 28's [cache-maintenance experiment](tests/fixtures/qemu-cache-maintenance.out)
    removes the store-width/address intervention and invalidates physical RAM
    lines at guest IC IVAU. Both Images pass 6/6 userspace and 3/3 static-key
    cases; removing only invalidation restores three failures per Image despite
    retaining the helper, lookup, barrier and block boundary. This is still a
    private, incomplete cache-maintenance implementation, not a production
    runtime change. The complete unchanged 23-test sched_ext program passes on
    GCC but times out on the retained old EvoCC Image with BOTH pristine and
    candidate QEMU; neither pair reproduces the NULL fault. New-composition
    runtime validation and the remaining emulator paths are still required.

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
  The September 26 [assembly-transport candidate](tests/fixtures/kvm-transport-replay.out)
  again completes three modes before 120 seconds; GCC completes ten. All 6,765
  installed objects and generated inputs survive linking. Smaller code in two
  objects does not close a whole-kernel performance gap.
  September 28's [uniform 7,002-object original replay](tests/fixtures/kvm-uniform-original-replay.out)
  still times out after 3/10 modes; matching GCC completes 10/10. Both use the
  unchanged 1 GiB workload and 120-second limit. All inputs remain unchanged;
  neither the cumulative code-size repairs nor a successful boot closes K02.
  The later pinned **7,008-object** composition also times out after 3/10;
  this round's matching GCC reaches 7/10 and also times out. Both enter the
  next mode and shut down normally. Retain this new common deadline failure
  rather than importing the older GCC PASS; no runtime speedup is established.

  - [x] **K02a — Remove redundant negated-truth materialization**, 2026-09-23.
    [Minimal C case](tests/fixtures/negated-truth-branch.c) went RED → GREEN;
    both interpreters and native GCC/Clang/EvoCC controls pass at O0/O2.
    The original `virt/kvm/kvm_main.o` reassembles with identical global symbols,
    text 243232 → 237968 bytes on controlled prior/current builds. The 1094-test
    affected gate, clippy and fmt pass. No Image was changed; K02 still needs its
    unchanged full-mode runtime replay after final rebuild. [Record](tests/fixtures/negated-truth-branch.out).

  - [x] **K02b — Preserve assembly operands while removing trivial transport**, 2026-09-26.
    Ordinary validated-SSA assembly now admits direct-edge identity forwarding;
    fixed-register, asm-goto and indirect-dispatch boundaries remain excluded.
    The [C reduction](tests/fixtures/asm-value-transport.c) also exposed an
    independent allocator bug: one value bound to two separate fixed input
    registers was rejected. Distinct operand copies repair it without relaxing
    genuine tied-operand conflicts. Nine MIR tests, the CLI regression, six
    native O0/O2 controls and the 1001-test changed gate pass. Both original KVM
    objects assemble with identical exports; this alone is not a runtime PASS.
    [Evidence and exact scope](tests/fixtures/asm-value-transport.out).

  - [x] **K02c — Select bounded constant memory displacements**, 2026-09-26.
    The [minimal C](tests/fixtures/memory-address-selection.c) first failed its
    offset-shape assertion without compilation errors. Same-block constant
    pointer offsets now use existing typed load/store immediates; volatility,
    access width/alignment, guards and effect boundaries remain unchanged.
    Seven Rust tests, 48 assembler checks, 12 native controls and the 1008-test
    affected gate pass, including Clippy/fmt. Original KVM objects retain their
    exports; text shrinks 187376 → 176264 and 37328 → 34996 bytes. Nothing is
    installed and no timing claim is inferred. K02/K03 remain OPEN.
    [Exact evidence](tests/fixtures/memory-address-selection.out).

  - [x] **K02d — Avoid duplicate automatic-zero value stores**, 2026-09-26.
    The [small C case](tests/fixtures/automatic-zero-single-pass.c) covers fixed
    and dynamic records, pointer provenance, padding and admitted scalar/vector
    types. After representation zeroing, the shared initializer only repairs
    typed Null pointers; explicit initializers and rejection boundaries remain.
    34 CLI tests, 12 native controls and the 1121-test affected gate pass.
    Original new-config KVM objects retain their exports; text decreases by
    4028/360 bytes. They are not installed, and the running frozen-release survey
    does not include this change. K02/K03 and final-composition replay stay OPEN.
    [Exact evidence](tests/fixtures/automatic-zero-single-pass.out).

  - [x] **K02e — Use either adjacent conditional successor as fall-through**, 2026-09-26.
    Two MIR shape assertions first fail without compilation errors. Final
    emission now inverts the condition when the taken block is physically next;
    logical edges, argument copies, labels and relaxation remain unchanged.
    Three MIR regressions, the CLI/interpreter reduction, 14 sealed native and
    mixed-link controls and the 1017-test affected gate pass (Clippy/fmt green).
    Original KVM objects retain exports; redundant adjacent branches fall from
    1101/219 to zero and text shrinks by 4444/832 bytes. No objects are installed;
    the running frozen survey excludes this change. K02/K03 remain OPEN.
    [Source and evidence](tests/fixtures/conditional-fallthrough.out).

  - [x] **K02f — Select bounded constant shift immediates**, 2026-09-26.
    The [C reduction](tests/fixtures/constant-shift-selection.c) and MIR shape
    assertion go RED → GREEN. Count decoding keeps its own width; source guards,
    narrow-value extensions, dynamic counts and pair operations are unchanged.
    Four MIR tests, the CLI/interpreter fixture, 60 assembler checks, 14 sealed
    native/mixed-link controls and the 1022-test affected gate pass; Clippy/fmt
    pass. Two original KVM objects retain exports; 87/25 shifts become immediate,
    but text changes by only 0/-56 bytes. Nothing is installed; no runtime
    speedup is claimed and the frozen full build excludes this repair.
    K02/K03 stay OPEN. [Exact boundaries and results](tests/fixtures/constant-shift-selection.out).

  - [x] **K02g — Select bounded constant add/sub immediates**, 2026-09-26.
    The [small C case](tests/fixtures/constant-addend-selection.c) and MIR shape
    assertions go RED → GREEN. Full-register constants in [0,4095] use existing
    immediate forms; only ADD commutes evaluated operands. Overflow guards and
    all nonmatching paths remain. Four MIR tests, one CLI test, 48 assembler
    encodings, 14 native/mixed-link controls and the 1027-test affected gate pass;
    Clippy/fmt pass. The spill regression still forces real spills. Original KVM
    objects keep exports and shrink by 732/128 text bytes. No objects installed,
    no runtime speedup claim; the frozen full build excludes this repair.
    K02/K03 stay OPEN. [Exact evidence](tests/fixtures/constant-addend-selection.out).

  - [x] **K02h — Select and validate full-width logical immediates**, 2026-09-28.
    Initial REDs expose missed mask selection and acceptance of unencodable
    MIR operands; a second RED catches complement-expression constant facts.
    The shared recognizer covers 6,636 encodings and 131,072 random controls;
    narrow/pair and nonmatching operations retain their register paths.
    Both interpreters/MIR, 240 assembler shape/target combinations, fourteen
    native/cross-compiler controls and both original KVM object assemblies pass.
    The local affected gate has 1,077 PASS, zero failures, four historical
    ignores; Clippy/fmt pass. The active kernel's frozen compiler excludes this
    repair. No objects were installed; K02/K03 and final pinned T05 remain OPEN.
    [Exact scope, debug timeout and identities](tests/fixtures/logical-immediate-selection.out).

  - [x] **K02i — Select and validate bounded immediate comparisons**, 2026-09-28.
    Shape and malformed-imm12 REDs now pass. Full-width comparisons preserve
    reversed relations, flags adjacency, source effects and unmatched paths.
    Six MIR/CLI tests cover 96 shapes, 1,056 interpreter comparisons and 192
    assembler combinations; fourteen sealed native/crossed controls pass.
    Original KVM objects retain their exports and shrink by 4,048/884 text bytes.
    The pinned Rust 1.97.1 affected gate has 1,083 PASS, zero FAIL and four
    historical ignores, with Clippy/fmt PASS. No objects are installed and no
    runtime speedup is inferred; K02/K03 remain OPEN.
    [Exact result and retained failed attempts](tests/fixtures/constant-compare-selection.out).

  - [x] **K02j — Select direct-null tests and repair MIR null transport**, 2026-09-28.
    The [minimal C case](tests/fixtures/null-compare-selection.c) exposes both
    redundant pointer comparison materialization and an existing interpreter
    E3013 on typed null stores. One shared conversion now handles null for
    ordinary/atomic stores and admitted pointer exchange/CAS; nonzero raw
    provenance and ordinary integer-store fragments remain rejected. Ten focused
    tests, 40 assembler combinations and fourteen native/crossed executions
    pass. The final five-crate gate has 1,093 PASS, zero FAIL, four historical
    ignores; Clippy/fmt pass. Both original KVM objects preserve symbols, with
    text reductions of 1,024/112 bytes. No object is installed; the existing
    7,008-object kernel does not include this change and K02/K03 stay OPEN.
    [Exact scope and identities](tests/fixtures/null-compare-selection.out).

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
  September 26's [new two-object replay](tests/fixtures/kvm-transport-replay.out)
  has the same original-program timeout: three completed modes, ten for GCC.
  The September 28 [uniform-composition replay](tests/fixtures/kvm-uniform-original-replay.out)
  confirms the same 3/10 versus 10/10 outcome under the unchanged workload,
  fixed-CPU/EL2 lane and timeout. No new full-test PASS is inferred.
  The later pinned 7,008-object replay yields subject **3/10 TIMEOUT** and
  reference **7/10 TIMEOUT**, with orderly shutdown on both. The current
  original remains unclosed; smaller current-image profiles are diagnostic,
  not substitutes for the unchanged full workload.

  - [x] **K03a — Substitute bounded private leaf helpers**, 2026-09-23.
    The [small C case](tests/fixtures/private-leaf-substitution.c) goes RED → GREEN
    through the existing C-HIR inliner, with declaration-chain barriers and
    explicit cost/lifetime/address boundaries. Interpreter/native controls and
    the 1,812-test affected gate pass. The original page-table object assembles
    with identical exports and three fewer helper calls, but text grows by 128
    bytes; no runtime improvement is inferred. No Image changed: K03 remains
    OPEN. [Full result](tests/fixtures/private-leaf-substitution.out).

  - [x] **K03b — Inline a single-site visitor without expanding runtime recursion**, 2026-09-27.
    Read-only instruction counters independently match both KVM reductions'
    ftrace entry counts and expose repeated out-of-line visitor overhead.
    The [C reduction](tests/fixtures/recursive-inline-visitor.c) goes RED → GREEN:
    the visitor expands into its out-of-line walker; the recursive call remains.
    Existing object identity/lifetime/effect rules and explicit size/activation
    boundaries are preserved. The repair also honors noinline/noipa/noclone on
    the complete declaration chain for every optional substitution path.
    Nine CLI regressions, six sealed native controls and the 1868-test affected
    gate pass (Clippy/fmt green). The exact original
    page-table object retains all exports, removes the visit call/definition and
    changes text 33068 → 32964 bytes. No runtime speedup is inferred. K02/K03
    still require the unchanged original tests on the rebuilt composition.
    [Repair evidence](tests/fixtures/recursive-inline-visitor.out),
    [instruction-count limits](tests/fixtures/kvm-instruction-count.out).

- [ ] **K04 — ublk stress/shutdown stall** — `OPEN`, P1.
  `test_stress_04.sh` hits the 45-second wrapper limit and stalls in shutdown sync;
  a later diagnostic invocation passes in 4.983 seconds. Capture the blocked I/O
  path and establish cause before changing anything. Close with the original
  workload and clean shutdown, or a source-bound non-compiler disposition;
  intermittency alone is not a fix. [Evidence][kernel].
  September 27's [bounded blocked-stack observer](tests/fixtures/ublk-blocked-observer.out)
  preserves the original workload/budget. Six new pairs all pass and shut down;
  no invocation reaches the 25-second diagnostic or shows a blocked final task.
  One initial reference capture is INVALID_OUTPUT from interleaved printk, not
  PASS. The 835 KiB initramfs adds no full disk. The old stall remains unexplained;
  retain K04 rather than attributing the successful retries to a compiler fix.
  September 28's [39-program original pair](tests/fixtures/kernel-uniform-selected-runtime.out)
  again reproduces GCC stress_04's 45-second timeout and blocked shutdown;
  EvoCC passes that invocation. The cause and full K13 acceptance remain open.
  A subsequent original GCC failure now has real blocked-task evidence:
  two writeback workers wait in `blk_mq_get_tag`, while fio/io_uring readers
  wait on folios. The 45-second inner TIMEOUT is preserved despite wrapper
  exit zero; shutdown still exceeds the 105-second outer bound. This narrows
  the stall to request/writeback progress, without proving its cause. Expanded
  per-thread/device observation then gives 12 successful original invocations;
  these negative reproductions are not a fix. See the same observer record.
  The [single-device C reduction](tests/fixtures/ublk-kill-cancel.out) passes
  4 cold-null + 3 warm-loop cases per retained Image, including real queued I/O,
  forced writeback, bounded client/deletion completion and normal shutdown.
  Earlier fixture setup failures are retained separately. This does not
  reproduce the multi-device original or justify a compiler/kernel repair.
  Running-daemon controls then pass 6/6 (no preceding SIGSTOP). The new
  [shared-backing pair](tests/fixtures/ublk-shared-cancel.c) reproduces one GCC
  deletion exceeding its eight-second diagnostic bound after all clients exit;
  later cases are NOT_RUN, and shutdown still completes. EvoCC passes 3/3.
  A follow-up observed pair passes 3/3 each, not a fix. This is narrower than
  the original blocked-writeback/45-second failure and remains separately
  recorded. Later source inspection finds kublk's normal close-notification
  fallback itself waits **10 seconds**, exceeding the diagnostic's old 8-second
  bound. That observation alone cannot prove a kernel latency bug; retain the
  historical failure and use the source-qualified syscall control. Do not waive
  K04 or lengthen the upstream watchdog.

  - [x] **K04b — Isolate ublk completion-order assertions** — `DONE`, P1, 2026-09-28, classification only.
    Newly exposed generic_01 fails on both kernels; generic_02 fails on GCC
    and passes on EvoCC. Both use the frozen fio/trace completion-order check.
    The [small AIO control](tests/fixtures/ublk-aio-completion-order.c) passes
    four checks and 65,536 exact completions per Image, but its bounded batches
    do not reproduce reordering. Compare continuous submission and the trace
    observer before attributing the failure; the general block-layer absence
    of an ordering guarantee alone is not a diagnosis of this test.
    [Results and limits](tests/fixtures/ublk-aio-completion-order.out).
    The [continuous-refill follow-up](tests/fixtures/ublk-aio-refill-order.out)
    also passes 4/4 cases and 65,536 completions per Image, without reordering.
    Batch replenishment alone does not explain the missing reproduction;
    fio submission details and the trace observer still need isolation.
    The [independent event observer](tests/fixtures/ublk-trace-order.out) then
    reproduces actual cross-CPU completion reordering with original fio and
    bpftrace inputs: all 131,072 requests are issued sequentially and every
    sector completes exactly once, with zero trace loss. The installed default
    is two queues, not one. One-queue and pinned-submitter controls have zero
    order breaks on both Images. Unchanged original generic_01 still fails on
    both; generic_02 is GCC PASS/EvoCC FAIL this time. This closes the shared
    strict-order assumption's classification, not those originals as PASS,
    the stress/shutdown problem K04 or final K13. No acceptance input is pinned
    or rewritten; previous negative observations remain intact.

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

- [x] **K06 — Build and run the IOPF-enabled configuration** — `DONE`, 2026-09-28.
  September 26: the reviewed `CONFIG_ARM_SMMU_V3_SVA=y` configuration builds
  and boots on GCC. The unchanged iommufd ELF improves **218 PASS / 6 FAIL /
  5 SKIP → 224 PASS / 0 FAIL / 5 SKIP**, under the original 45-second limit;
  the five skips explicitly exclude the upstream `no_viommu` variant.
  Three new IOPF objects also compile/assemble with EvoCC and identical exports.
  The corrected inventory is **7,002 target-C / 847 modules** plus 37 separate
  HOSTCC objects. The first common-bench launch rejected the mixed-role inventory
  before compilation (T04b); the following attempt encountered bitmap assertions
  and 13 GCC object replacements. After those repairs, uniform run
  `20260927T135325Z` completes all 7,002 actual-header compilations, stable
  generated inputs, link, zero replacement and boot. Both new Images then pass
  the unchanged original executable: **224 PASS / 0 FAIL / 5 SKIP**, exit 0,
  45-second limit unchanged, all 229 cases accounted. The five upstream
  `no_viommu` skips remain SKIP. This closes this configuration/runtime gap,
  not K13/K14 or untested IOMMU hardware.
  [Identities and scope](tests/fixtures/iopf-enabled-reference.out), [run ledger][kernel].

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

  - [x] **K11a — Restore /tmp's mountpoint prerequisite on ext4**, 2026-09-24.
    The [minimal C case](tests/fixtures/selftest-tmp-mount.c) and unchanged
    `openat2:resolve_test` reproduce EINVAL on both Images before all 88 cases.
    The common init now self-binds /tmp, preserving disk-backed objects and
    failing closed on setup errors. The original passes 88/88 on both kernels;
    three unchanged mount controls pass another 152/152 each. All eight flights
    shut down; disk hashes remain unchanged. 239 K1 tests and Ruff pass. This
    closes one runtime-environment failure, not K11/K13's full population.
    [Identity-bound record](tests/fixtures/selftest-tmp-mount.out).

  - [x] **K11b — Verify harness constructor-order failure**, 2026-09-24.
    The [minimal C case](tests/fixtures/selftest-constructor-order.c) reproduces
    priority-20000 registration before the default-priority direction detector.
    All six native compiler/optimization pairs produce the reversed list;
    moving only the detector earlier fixes the causal control. Both interpreters
    agree, and the 506-test changed gate passes. The unchanged original still
    fails only its ordering diff on both kernels, with normal shutdown. This
    closes **upstream-assumption classification**, not that test as PASS.
    [Paired record](tests/fixtures/selftest-constructor-order.out).

  - [x] **K11c — Supply udmabuf's actual HugeTLB requirement**, 2026-09-24.
    The [minimal C case](tests/fixtures/selftest-hugetlb-reservation.c) keeps the
    original 2 GiB mapping and proves ENOMEM with one reserved page versus
    successful access to all 1024 pages on both kernels. The shared runtime
    now selects an explicit 4 GiB machine and 1024-page pool for this program
    only; locked K6, original binary and 45-second timeout stay unchanged.
    Both original replays improve from five completed cases then FAIL to
    **7/7 PASS**, with orderly shutdown. 241 K1 tests and Ruff pass. This is
    resource provisioning, not new compiler or full-run credit.
    [Paired record](tests/fixtures/selftest-hugetlb-reservation.out).

  - [x] **K11d — Demonstrate the frozen ptrace/exec wait cycle**, 2026-09-24.
    The [small C case](tests/fixtures/ptrace-exec-reap-cycle.c) captures the
    tracer inside `ptrace_attach` and tracee inside `begin_new_exec` on both
    kernels. A second tracer thread's sibling reap releases the blocked attach;
    stop/detach/reap controls pass without leaving children. The unchanged
    original still has one PASS and one 30-second attach FAIL on each kernel.
    This closes **frozen kernel/test mismatch classification**, not an original
    PASS; changing that baseline is not an EvoCC compiler fix.
    [Source-bound record](tests/fixtures/ptrace-exec-reap-cycle.out).

  - [x] **K11e — Verify rseq's unregistered-thread prerequisite**, 2026-09-26.
    The [small C reduction](tests/fixtures/rseq-registration-order.c) observes
    EINVAL with glibc registration, EFAULT without it, then EINVAL/EFAULT again
    around its own registration/unregistration, on both retained kernels.
    Both unchanged direct originals still FAIL; both unchanged upstream
    companion scripts PASS because they disable glibc registration before exec.
    All six diagnostic/original flights shut down normally. This closes
    **registration-precondition classification**, not the direct test as PASS;
    no global tunable, source, original timeout or historical total is changed.
    [Paired record](tests/fixtures/rseq-registration-order.out).

  - [ ] **K11f — Retest pidfd's recycling-loop timeout on the final Image** — `OPEN`, P1.
    The [bounded syscall reduction](tests/fixtures/pidfd-recycle-progress.out)
    completes 1024 stop/continue/reap cycles in root and fresh namespaces on
    both kernels. The namespace's actual 4M PID limit explains the original's
    bounded-search SKIP. Fresh unchanged originals both finish within 45s with
    7 PASS / 1 SKIP, but the historical subject timeout and measured slowdown
    are not declared repaired by that retry. Preserve them for K13's final pair.
    September 28's new pinned 7,008-object pair again completes the unchanged
    original: both have 7 nested PASS / 1 SKIP, with shutdown at 17.505/42.139s.
    The original 45-second limit, disk/payload hashes and historical timeout
    remain intact. This updates current-image evidence but does not prove a
    performance repair or a complete-campaign margin. It excludes K02j's later
    compiler change. [Current capture](tests/fixtures/pidfd-recycle-progress.out).

  - [x] **K11g — Attribute delayed dying-memcg release** — `DONE`, P1, 2026-09-26.
    The [small frozen-workload control](tests/fixtures/cgroup-stock-displacement.c)
    and final-reference stack traces account for every delayed CSS identity:
    5/5 GCC and 8/8 EvoCC releases come from per-CPU LRU folio batches through
    folios_put/uncharge, not the hypothesized empty charge-stock slot. New page
    faults drain the batches; simply waiting 20s does not. No lost trace events.
    Unchanged original pairs still FAIL: GCC inner 4 PASS/2 FAIL, EvoCC 3/3;
    dead-cgroup FAIL remains on both. This closes **source-bound classification**,
    not a kernel patch, compiler repair, original PASS or K13 acceptance.
    [Measured controls and limits](tests/fixtures/cgroup-stock-displacement.out).

  - [x] **K11h — Bound immediate memcg-deletion accounting** — `DONE`, P1, 2026-09-26.
    The [controlled page-charge case](tests/fixtures/cgroup-stock-capacity.c)
    measures stable 6,905,856/7,008,256-byte current-vs-stat differences on
    GCC/EvoCC. The frozen kernel has seven cache slots per CPU, but its selftest
    allows only one batch (1,048,576 bytes). Both measurements fit the seven-slot
    capacity; the upstream tolerance patch identifies this same source mismatch.
    This closes **test-bound classification**, not a compiler repair. Original
    EvoCC FAIL at exactly the strict one-batch margin remains FAIL; no new
    tolerance or sleep is applied. Separate LRU/slab findings remain separate.
    [Evidence, source and unchanged verdicts](tests/fixtures/cgroup-stock-capacity.out).

  - [x] **K11i — Verify arm64 syscall-user-dispatch's unavailable boundary** — `DONE`, P1, 2026-09-26.
    The [small C probe](tests/fixtures/syscall-dispatch-unavailable.c) confirms
    all three frozen `!GENERIC_ENTRY` EINVAL stubs while ordinary tracing still
    stops, resumes and reaps its child. GCC/Clang-built probes pass 14 assertions
    per kernel. All three unchanged originals still FAIL on both kernels with
    normal shutdown; none is relabeled PASS. This closes **architecture-boundary
    classification**, not dispatch support or K11/K13's full population.
    [Source, original replays and hashes](tests/fixtures/syscall-dispatch-unavailable.out).

  - [x] **K11j — Separate seccomp values from timing-model assertions** — `DONE`, P1, 2026-09-26.
    The [small filter-chain C case](tests/fixtures/seccomp-filter-chain.c) checks
    the original four-filter sequence, invalid load rejection and enforced EPERM
    beneath prior ALLOW filters. GCC/Clang controls pass 28 assertions per Image.
    Historical original failures are exactly two nanosecond-cost comparisons,
    not incorrect syscall values; the following SKIPs depend on those comparisons.
    Fresh unchanged originals both pass 7/7 within 180 seconds. No repair or
    stable-performance claim is inferred from this retry; historical FAILs and
    the measured subject slowdown remain. Final K13 is still required.
    [Arithmetic, source and paired evidence](tests/fixtures/seccomp-filter-chain.out).

  - [x] **K11k — Isolate OVPN's frozen key-prefix failure**, 2026-09-28.
    Restoring missing inputs (K12d) exposes all seven original scripts failing
    in ovpn-cli before traffic. Its 256-byte cap truncates the supplied 349-byte
    Base64 file to 253 symbols, an incomplete quantum. The
    [small C case](tests/fixtures/ovpn-base64-prefix.c) passes 42 observations
    across GCC/Clang/EvoCC O0/O2 against the actual runtime library, including
    complete-input and aligned-prefix controls. This closes **source-bound
    classification**, not a helper patch or original PASS. All seven unchanged
    reference direct originals remain FAIL. The later unchanged full-runner
    [selected pair](tests/fixtures/kernel-uniform-selected-runtime.out) reaches
    the same decoding error on both kernels and records seven TIMEOUTs per side;
    neither outcome is rewritten or promoted to PASS.
    [Commands, hashes and exact boundary](tests/fixtures/ovpn-base64-prefix.out).

  - [x] **K11l — Attribute the shared BPF module hardening timeout**, 2026-09-28.
    The [small C observer](tests/fixtures/bpf-module-harden-progress.c) reduces
    the unchanged signed module to original case 851: its 98,787-instruction
    program undergoes per-constant whole-array/branch rewriting only with
    harden=2. Neighbor/disabled-blinding controls pass on both Images; bounded
    kprobes prove advancing patch positions and counts, not an entropy wait.
    The same single case completes uninstrumented in approximately 43.5/92.8s
    (GCC/EvoCC). Both full original four-mode programs still TIMEOUT at 300s.
    This closes **source-bound workload classification**, not an original PASS
    or a compiler performance repair. No watchdog or kernel source is changed.
    [Controls, exact source and limits](tests/fixtures/bpf-module-harden-progress.out).

  - [x] **K11m — Preserve FPU worker skips/failures and completion counts**, 2026-09-28.
    The original standalone worker prints SKIP but exits zero; its outer TAP
    PASS previously hid that skip. The [small C protocol](tests/fixtures/fpu-worker-verdict.c)
    and parser RED reproduce it. Shared parsing and full-campaign mapping now
    retain inner counts, reject malformed/missing worker results, and require
    the init-owned nonce-bound CPU count for the stress denominator. All 282 K1
    and 317 ladder tests, two sealed compiler controls and Ruff pass. Original
    reference replay retains the standalone inner SKIP and records 4000/4000
    stress workers OK with shutdown. This closes **result accounting**, not
    the missing standalone prerequisite, new subject acceptance or K12f/K13.
    [Commands, source and raw identities](tests/fixtures/fpu-worker-verdict.out).

- [ ] **K12 — Close collection-build and configuration coverage gaps** — `OPEN`, P2.
  The separate Kconfig review resolves 100 disabled requests in 27 collections;
  the uniform 7,002-object composition now boots, and both new runtimes install
  all 847 signed kernel modules plus 17 original test-support modules. The
  [new original pair](tests/fixtures/kernel-uniform-module-runtime.out) passes
  NCI's 8/8 cases, both splice programs and IOMMU's 224/0/5 inner results.
  FPU still SKIPs for a missing configuration; BPF test_kmod still times out on
  both sides. This selected six-program run leaves 964 NOT_RUN, not K13 PASS.
  Other affected collections still require replay. The four build gaps are
  source-bound below.
  Build runnable supported sets with matching payloads; give precise reasons
  for unavailable ones. Keep any changed population separate from the frozen 970. [Evidence][kernel].

  - [x] **K12a — Classify four architecture-gated collection recipes**, 2026-09-24.
    The original `kexec`, `prctl` and two Intel thermal Makefiles reproduce
    missing `all` before any compiler invocation on arm64. Twenty original
    Makefile variable controls establish their exact architecture/CROSS_COMPILE
    guards. This is a frozen-recipe limit, not a claim all prctl APIs require
    x86. No guards, build outcomes or 970-program denominator change; no test
    is counted as PASS. [Source-bound record](tests/fixtures/selftest-build-guards.out).

  - [x] **K12b — Prove bitmap helper results before constant queries**, 2026-09-27.
    The 7,002-object prepared pass reaches modpost but `lib/test_bitmap.o` retains
    eight `__compiletime_assert_*` references. The scalar compound-assignment
    [reduction](tests/fixtures/bitmap-constant-mutation.c) is now GREEN in both
    interpreters and all three AArch64 compiler controls. The subsequent
    [pointer/array reduction](tests/fixtures/bitmap-constant-memory.c) drives
    shared, bounded private-memory facts through inlining and constant queries;
    executed accesses and UB checks remain. The original object now has **zero**
    assertion references with unchanged exports. Its signed module and the GCC
    control each execute **390447/390447** original checks on the matching GCC
    Image and unload; seven observer checks pass. Three small fixtures pass 15
    native controls; 81 query tests and the **1841-test** changed gate pass,
    including Clippy/fmt (three historical ignored tests remain).
    This closes the bitmap compiler/module defect, not uniform kernel composition,
    full modpost or K13/K14. [Exact evidence](tests/fixtures/bitmap-constant-memory.out).

  - [x] **K12c — Register the installed virtual NCI device before tests**, 2026-09-28.
    The old pair lacks NFC_VIRTUAL_NCI; the new configuration builds it, but
    the udev-free runtime also omitted its preload. A new prerequisite-list
    RED is repaired using the exact installed module inventory. On the matching
    new GCC Image, signed dependency loads reproduce the original 0/8 failure;
    loading virtual_ncidev makes the same executable pass **8/8**. Eleven
    load/device/ioctl/unload controls and boot/shutdown pass; 261 K1 Python tests
    and Ruff pass. This closes reference prerequisite diagnosis/packaging,
    not subject-module acceptance at that checkpoint. The subsequent
    [new matching pair](tests/fixtures/kernel-uniform-module-runtime.out)
    actually loads the new modules and passes the unchanged original **8/8**
    on both kernels, closing the subject prerequisite too.
    [Small C and identity-bound result](tests/fixtures/nci-module-prerequisite.out).

  - [x] **K12d — Install OVPN's omitted original peer tables and key**, 2026-09-28.
    The install regression goes RED → GREEN using the common runtime helper
    path; all 262 K1 tests and Ruff pass. Only the two original peer tables and
    original key are restored, with missing-source rejection and unchanged
    scripts/peer counts. Eight reference before/after flights boot and shut
    down. The seven restored originals expose K11k and remain FAIL, not PASS;
    no complete new runtime disk or subject Image is manufactured.
    [Identity-bound record](tests/fixtures/ovpn-base64-prefix.out).

  - [x] **K12e — Enable the missing connector process-event configuration** — `DONE`, P2, 2026-09-28.
    The earlier uniform configuration disables CONNECTOR; the collection has no
    upstream config fragment. Two reference controls pass 12/12 absence checks,
    while unchanged proc_filter still fails EPROTONOSUPPORT. PROC_EVENTS needs
    CONNECTOR=y, not a separately loaded module. The shared configuration audit
    now records these source-pinned prerequisites and invalidates changed-source
    reviews; 263 K1 tests and Ruff pass. After the active frozen composition,
    batch both built-in options into a separately identified configuration and
    replay the original on its matching pair. No active config or historical
    outcome changes. [Exact evidence](tests/fixtures/connector-configuration.out).
    The [batched configuration preflight](tests/fixtures/kernel-connector-fpu-preflight.out)
    now solves exactly CONNECTOR=y, PROC_EVENTS=y and TEST_FPU=m, and all five
    newly enabled original C units assemble with reference symbol parity using
    the current pinned compiler. No active Image/config/module changes; full
    composition and unchanged paired runtime remain required for both items.
    The current 848-module GCC reference enables both options and receives
    events. A [bounded C control](tests/fixtures/connector-process-events.c)
    passes 20/20 checks across GCC/Clang builds: owned fork/exit delivery and
    normal SIGINT termination of the unchanged original. Source inspection
    confirms proc_filter is a persistent listener, so the original 45-second
    runner still legitimately records TIMEOUT rather than a finite-test PASS.
    [Exact controls and retained failures](tests/fixtures/connector-process-events.out).
    The final pinned 7,008-object subject now enables both options. The same
    qualified payload passes **20/20** checks there, including the unchanged
    original's owned-child events and normal SIGINT exit. The unchanged
    unattended original still records its **45-second TIMEOUT** on both sides:
    this closes the missing-config/event-delivery task, not that program's
    outer PASS count. Its persistent-listener disposition remains explicit.

  - [x] **K12f — Enable and execute the omitted kernel FPU module** — `DONE`, P2, 2026-09-28.
    Both unchanged originals on the new uniform pair explicitly SKIP because
    CONFIG_TEST_FPU=m is absent. The collection has no upstream config fragment.
    Source-pinned audit now records TEST_FPU=m, architecture support and the
    !KCOV_INSTRUMENT_ALL dependency; its RED and all 271 K1 tests pass. The
    reviewed config has architecture support and no KCOV, so this is an omitted
    request, not an arm64 limitation. Include it with K12e's next configuration
    batch, then compile, sign/install and run the original on both Images.
    [Original logs and precise boundary](tests/fixtures/kernel-uniform-module-runtime.out).
    The new 848-module reference runtime is now installed and signed; unchanged
    run_test_fpu.sh actually loads test_fpu and records 4000/4000 worker OKs.
    K11m exposes the standalone inner SKIP instead of hiding it behind exit 0.
    The shared prerequisite installer now loads the matching installed test_fpu
    module. Its RED and all 283 K1 tests pass; unchanged reference replays now
    record 1/1 standalone and 4000/4000 stress worker OKs, with normal shutdown.
    Only /tests/modules changes on an APFS clone; the old disk/captures remain.
    The final pinned subject's matching module is now signed, installed and
    actually loaded. Both unchanged originals pass: **1/1 standalone** and
    **4000/4000 stress workers**, zero FAIL/SKIP, normal shutdown. This matches
    the qualified GCC reference and closes the omitted-module/runtime gap;
    it does not close the other 968 programs or K13/K14.
    [Final subject identity and original results](tests/fixtures/fpu-worker-verdict.out).

- [ ] **K13 — Fresh complete paired kselftest acceptance** — `OPEN`, P1, after K01–K12/T04.
  The last 970-program pair is INCOMPLETE and bound to an older Image; its counts
  cannot be transferred to repaired Images. Run the whole installed population
  serially with matched payloads, audit nested verdicts and zero lost programs,
  and report all failures/skips separately. Close the execution task with fresh
  receipts; claim all-tests PASS only if the actual acceptance predicate holds.
  [Historical receipt][selftest-receipt].
  The September 28 [selected replay](tests/fixtures/kernel-uniform-selected-runtime.out)
  starts 39/39 on each Image: GCC 28 PASS / 2 FAIL / 9 TIMEOUT, EvoCC
  29 PASS / 1 FAIL / 9 TIMEOUT. The other 931 remain NOT_RUN; this is not a
  new complete campaign. The owner/worker has exited with INCOMPLETE.

- [ ] **K14 — Fresh complete expanded built-in and module KUnit acceptance** — `OPEN`, P1.
  After K05/K07/K08 and final composition, rerun all 1,504 built-in cases and
  actually load all 31 test modules (765 cases) on each matching kernel. Preserve
  duplicate-case multiplicity, the upstream switch-entry XFAIL and config limits.
  Later filtered FORTIFY results cannot rewrite the old full totals or inherit
  module acceptance from another Image. [Historical receipt][kunit-receipt].

## Application failures, accounting and replays

- [x] **B01 — Reconcile every original LLVM failure with subsequent repairs** — `DONE`, P1, 2026-09-23.
  All 44 original failure identities now have a [case-level mapping](tests/fixtures/llvm-failure-reconciliation.out):
  8 targeted original-program PASS (including September 24 B02), 30 named FP
  deferrals, 5 evidenced not-applicable and 1 OPEN (integer-vector `pr23135` L10).
  The existing adapter validates all 35 reviews against exact source/argv and
  evidence. Eight obsolete scalar-FP waivers are removed with a RED/GREEN test.
  This closes reconciliation only: L02–L05/L10 remain open, and the frozen
  1,735-program run and its PASS numerator are unchanged. [Evidence][apps].
  The September 27 complete replay now supersedes that historical checkpoint:
  **1,039 PASS / 1 compiler failure / 30 FP-deferred / 664 not-applicable /
  1 oracle-unusable**. The remaining failure is precisely L10; all 25 newly
  passing original programs and unchanged installed-object hashes are retained
  in run `20260927T085137Z`. L02 is closed for the hosted profile; L03–L05/L10
  remain separate boundaries, not a blanket FP waiver.

- [x] **B02 — LLVM LARGE floyd-warshall performance failure** — `DONE`, P1, 2026-09-24.
  The [small loop transport case](tests/fixtures/loop-value-transport.c) goes
  RED → GREEN by removing proven trivial phi copies during SSA destruction,
  without moving loads, guards or effects. Counter/swap/indirect-edge negatives,
  real spills, both interpreters and six native controls pass; the changed gate
  is 995 PASS, zero failures, four historical ignored, Clippy/fmt green.
  **Unchanged LARGE execution, output hash and comparison now PASS in 52.0360 s**
  inside the original 60-second limit (98 → 57 core instructions). This closes
  the targeted original, not B10 or a stable performance claim. No kernel Image
  inherits it. [Identity-bound record](tests/fixtures/loop-value-transport.out).
  The preceding unsuccessful reductions are retained below as historical steps.
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

  - [x] **B02d — Branch directly on single-use checked flags**, 2026-09-24.
    The [small C case](tests/fixtures/checked-flag-branch.c), 600 MIR boundary
    executions, real-spill continuation and nonmatching controls retain all
    traps, volatile accesses and explicit edge bindings. Six positive native
    runs and ten expected-trap negatives pass; the affected gate is 989 PASS,
    zero failures, four historical ignored, Clippy/fmt green. Original LARGE
    core shrinks 102 → 98 instructions but still times out at 60 seconds:
    **B02 remains OPEN**. No kernel Image changes. [Record](tests/fixtures/checked-flag-branch.out).

- [x] **B03 — Account for Csmith's 68 unusable-oracle cases** — `DONE`, P1, 2026-09-24.
  All 1,000 programs emit, 932 checksums agree and zero mismatch; the remaining
  31 base / 37 bitfield cases are not passes. Diagnose reference compile/runtime
  failures with the original seeds and budgets; retain unusable outcomes unless
  a valid matched replay exists. Close with per-case reasons and any recoverable
  comparisons, without replacing seeds. [Receipt](tests/contracts/ladder-csmith-receipt.json).
  All **68/68** identities now have source-bound repeating-path diagnoses;
  none remains undetermined. The [final fourteen](tests/fixtures/csmith-final-restarts.out)
  distinguish finite callees from restarting callers, unchanged guards and
  width-sensitive transitions, using unchanged-source observations on both
  oracles. Two permanent C controls pass eight interpreter and twelve native
  executions; the changed gate passes 503 tests, Clippy and fmt. All diagnostic
  children are reaped. This closes **accounting only**, not those 68 original
  programs as PASS: the 932-agreement numerator and B10 remain unchanged.

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

- [x] **B05 — Replace TinyCC's final two reference-built FP objects** — `DONE`, P2, 2026-09-27.
  Shared binary128 support repairs `tccgen.o` and `tccpp.o`. Fresh run
  `20260927T083017Z` installs **11/11**, no symbol exclusions or failures, and
  original `make test` passes on the reference and full EvoCC build. The runner
  now checks actual relink/test exit status, an exact upstream terminal marker
  and unchanged installed-object hashes, with three false-positive REDs and
  object-replacement negatives. **260** ladder regressions pass. The original
  target-specific Makefile omissions remain omissions, not newly executed tests.
  [Receipt](tests/contracts/ladder-tinycc-receipt.json).

- [x] **B06 — Finish toybox compilation and chattr diagnosis** — `DONE`, P2, 2026-09-27.
  Full run `20260927T084656Z` compiles/installs **203/203** frozen target units,
  zero refusals/exclusions/rebuilt objects; generated applet-table main remains
  reference-built outside that denominator. Both audited builds complete all
  **1,669 occurrences: 1,648 PASS / 1 FAIL / 20 SKIP**, no subject regression.
  The sole common tar FAIL is a demonstrated contradictory upstream expected
  hash; all 20 skips retain source/config/runtime reasons. This closes full
  execution and diagnosis, **not all-tests PASS**. The common score stays FAIL,
  and B10 still requires its final campaign. [Receipt](tests/contracts/ladder-toybox-receipt.json).

  - [x] **B06a — Isolate chattr's filesystem requirement**, 2026-09-23.
    The [minimal ioctl probe](tests/fixtures/toybox-filesystem-flags.c) returns
    ENOTTY on the benchmark FUSE mount and succeeds on container-local ext4 in
    all five GCC/Clang/EvoCC controls. The unchanged 15-case chattr subset
    completes with 5 FAIL / 10 SKIP on both a real reference relink and a
    hash-audited current-EvoCC applet relink. This closes the original filesystem
    diagnosis, **not** those cases as PASS or B06's remaining full-suite work.
    [Record and limits](tests/fixtures/toybox-filesystem-flags.out).

  - [x] **B06b — Run filesystem-sensitive tests on container-local storage**, 2026-09-27.
    The [mutation reduction](tests/fixtures/toybox-filesystem-mutation.c) proves
    both missing GETFLAGS on FUSE and missing immutable capability on local ext4.
    The shared adapter stages only runtime inputs on local storage and adds only
    `CAP_LINUX_IMMUTABLE`, retaining binary/object audits and original assertions.
    Five compiler controls pass; unchanged chattr is **13 PASS / 0 FAIL / 2 SKIP**
    on both historical audited binaries, versus 0/5/10. The conditional j/P skips
    remain skips. All 253 ladder tests pass; no full campaign was run.
    [Evidence and exact limits](tests/fixtures/toybox-filesystem-mutation.out).

  - [x] **B06c — Repair runtime prerequisites and classify the remaining tar failure**, 2026-09-27.
    The [small C/shell cases](tests/fixtures/toybox-runtime-prerequisites.out)
    reproduce missing USER, insufficient isolated hostname/mount capabilities
    and a loop filename exceeding 64 bytes. The shared runtime fixes these six
    failures without changing tests; eight native controls, the 157-occurrence
    targeted pair and complete 1,669-occurrence pair agree. Both audited tar
    binaries produce byte-identical owner/group-overridden archives before and
    after chown, contradicting only the later upstream expected digest. That
    FAIL is retained. All 261 ladder regressions and Ruff pass.

- [x] **B07 — Finish jq's binary128 interface and original tests** — `DONE`, P2, 2026-09-27.
  Run `20260927T082205Z` compiles/installs all **69/69** objects, including
  `src/builtin.c`'s `nexttoward(double, long double)` call. Unchanged `make check`
  passes nine jq scripts and 21 bundled Oniguruma programs; no target object was
  excluded or replaced by make. The prior run exposed omitted **`-pthread`**;
  the minimal driver/projection RED now passes with `_REENTRANT=1` and ordered
  user overrides, confirmed by fixed GCC14/Clang19 controls. This fresh replay
  retains the option and passes again. All 526 driver regressions, 257 ladder
  regressions, Clippy and fmt pass. B10's final unified campaign remains separate.
  [Receipt](tests/contracts/ladder-jq-receipt.json).

- [ ] **B08 — Build and test musl with its actual semantic flags** — `OPEN`, P2.
  Fresh run `20260927T110110Z` attempts all 1,339 records with `-frounding-math`:
  1,271 compile/assemble with exact symbol parity and survive two complete
  libc-test replays without additional failures or replaced objects. This
  supersedes the old 0/1,339 flag refusal, but remains a mixed build. All 68
  remaining refusals are floating-complex interfaces (E3049, L04). The 17
  reference failures (B08d) and explicit command omissions (B08b) remain open;
  no full-library or all-tests-PASS claim follows from the clean relative result.
  [Receipt](tests/contracts/ladder-musl-receipt.json).

  - [x] **B08a — Admit scalar FP inline-assembly bindings** — `DONE`, P2, 2026-09-27.
    Explicitly approved and specified in EvoIR 1.9.114 / EvoMIR 0.50. Scalar
    f32/f64 use the existing alias-aware SIMD bank, exact representation
    transport, matching/early-clobber bindings, spills, asm.goto edge copies
    and conservative world/fenv effects. Default operands print vN, matching
    fixed GCC14/Clang19; s/d views are explicit. Reached opaque asm still
    reports E3021 in both interpreters; f128, floating vectors, GPR bindings
    and general-registers-only remain refused. Eight native environment builds
    pass 58 checks each; sixteen crossed calls and high-pressure selected-edge
    cases pass. All eight original musl units assemble; the four original
    ceil/floor/rint/rintf objects pass 56 checks with each of four reference
    callers, alongside their reference-object controls. Full musl is separate.
    [Source, exact identities and validation](tests/fixtures/floating-assembly.out).

  - [ ] **B08b — Close musl's remaining command-projection semantics** — `OPEN`, P2.
    The current original-command inventory has no omitted `-f`/`-m`/`-O`
    request; bounded tuning admission and fail-closed projection are below.
    Remaining diagnostic/output/assembler actions need their own equivalence
    and refusal tests, including `-Werror` behavior. No full musl
    fidelity claim may be inferred from accepting `-frounding-math` alone.
    - [x] **B08b1 — Preserve already-satisfied conservative codegen options**, 2026-09-27.
      Ten frame/disabled-transform options now reach the driver without being
      discarded. Existing frame, source/minimum alignment and no-transform
      contracts are explicit; positive transforms and unrelated GCC tuning still
      reject. Both interpreters, 12 native controls, ten original musl object
      commands and a 1,339-record option inventory pass. The affected gate is
      green (1,062 passed, four existing ignores), as are 298 ladder tests.
      [Reproducer, boundaries and identities](tests/fixtures/conservative-codegen-options.out).
      This is one B08b subcase, not complete option fidelity.

    - [x] **B08b2 — Preserve unsupported semantic requests and bound GCC tuning**, 2026-09-28.
      Two command projectors silently dropped `-fpack-struct=1`; four native
      reductions then used the wrong layout while eight reference controls
      established the flag-sensitive boundary. Semantic families now reach the
      driver in written order, even when rejected; forced language and joined
      include operands survive too. Three exact tuning requests select existing
      whole-function allocation, absent hoisting and nonduplicating placement
      under EvoMIR 0.51, not arbitrary GCC algorithms. Both interpreters,
      eight tuning native controls, ten unchanged musl object commands and the
      1,339-command read-only inventory pass. Unknown layout/ABI/fast-math requests
      remain explicit refusals. Diagnostic policy remains B08b, not waived.
      [Reproduction and measured boundaries](tests/fixtures/command-projection-packed.out).

    - [x] **B08b3 — Preserve include search classes and joined operands**, 2026-09-28.
      A three-file reduction detects wrong header selection after `-isystem`
      becomes `-I`, or a joined system option disappears. Three of four native
      projected commands fail before repair; all four pass afterward, with
      eight GCC/Clang controls per round. Both projectors preserve search class,
      joined macro/quote/forced-include paths, literal spaces and incomplete
      operands for driver refusal. Unsupported quote/macro options are not
      translated into supported ordinary includes. All 314 ladder tests and
      targeted Ruff pass. This is not complete diagnostic-option fidelity.
      [Exact inputs and results](tests/fixtures/include-projection.out).

    - [x] **B08b4 — Preserve diagnostic requests instead of false acceptance**, 2026-09-28.
      Both adapters discard options that make the small source fail in all
      eight GCC/Clang controls. They now preserve warning policy in written
      order; eight EvoCC projections explicitly refuse, without assembly.
      All 317 ladder tests and targeted Ruff pass. All 1,339 original musl
      commands retain diagnostics; ten actual commands refuse unsupported
      warnings without changing installed objects. This closes silent omission,
      **not warning implementation**: a faithful full musl rebuild still needs
      that support. Historical omitted-option runs retain their claim ceiling.
      [Reproducer and exact results](tests/fixtures/command-projection-diagnostics.out).

    The [standard-excess-precision subcase](tests/fixtures/floating-standard-evaluation.out)
    is now repaired: the existing FLT_EVAL_METHOD=0 semantics satisfy the explicit
    option, which survives shared command projection. Both interpreters, eight
    reference and eight EvoCC native controls pass; 527 CLI and 262 ladder tests,
    Clippy/fmt/Ruff pass. Eight original musl commands have identical results
    with the option retained (four emit, four B08a refusals). This is not full
    option-fidelity or musl acceptance.
    [Per-symbol section placement](tests/fixtures/driver-symbol-sections.out)
    is also implemented in EvoMIR 0.49: explicit attributes override independent
    last-option-wins switches; native ELF/TLS/storage flags, aliases, lifecycle
    and patch roots survive. Twelve reference and twelve EvoCC objects pass
    ordinary/GC linking (48 native executions). All four command projectors
    retain the flags; 38 focused CLI, 264 ladder and 252 K1 tests pass. The same
    eight musl commands retain sections: four assemble, four remain B08a.
    [No-strict-aliasing admission](tests/fixtures/no-strict-aliasing.out) now
    preserves the existing conservative access semantics rather than dropping
    the flag. Both interpreters, four reference and eight EvoCC native builds
    pass; the 530-test affected driver gate is green. The eight original musl
    outcomes remain identical, with this flag retained. Remaining tuning and
    diagnostic omissions are not silently reclassified as implemented options.

    September 27 targeted repair also preserves `-Xassembler` operands, both
    assembler spellings' written override order and literal argv boundaries;
    failed container execution cannot publish partial assembly success. Five
    harness REDs and six real ELF objects verify the fix. The old
    `-Wa,--noexecstack` spelling was already transported to the assembler.
    [Targeted evidence](tests/fixtures/musl-reference-boundaries.out).

  - [x] **B08c — Close original musl pthread runtime regressions** — `DONE`, P2, 2026-09-27.
    The fresh scalar-asm compiler assembles 1,271/1,339 original units, all with
    exact symbol parity; the 68 refusals are floating-complex interfaces (L04).
    Its first complete mixed libc-test trial adds seven pthread failures or
    timeouts to the reference's 17 failures. Object-set bisection in
    `.evidence/ladder/musl/20260927T101243Z` isolates `pthread_barrier_wait` and
    `pthread_cond_timedwait`; its completed final replay accepts only the
    1,269-object set excluding those two. Neither compilation nor that clean
    partial set closes this task. The isolated
    [LL/SC probe](tests/fixtures/assembly-exclusive-window.c) exhibits failed
    progress with separated templates and compiler stack accesses; a joined
    template is a passing control. GCC O0 also fails some split cases, so this
    is not evidence that arbitrary separate asm blocks guarantee adjacency.
    Ordinary scalar asm outputs now stay in SSA when no object is required.
    The same probe improves from eight failed split locations to zero;
    377 lowering and 52 asm integration tests pass. Five unchanged affected
    libc-test programs pass with the repaired object pair and the separate
    reference archive; the old pair reproduces four failures/timeouts there.
    All scalar-w native/crossed/fenv controls remain green. Fresh run
    `20260927T110110Z` rebuilds all original units with the repaired compiler:
    all 1,271 candidates survive both complete replays (482 runtime-result slots
    each), zero added failures, zero changed object hashes and no excluded
    pthread objects. The final replay exits normally; its 17 failures exactly
    match the reference. The changed gate has 1,153 PASS / 0 FAIL / three
    historical ignores, plus Clippy/fmt; 271 ladder tests pass separately.
    [Reduction and final acceptance](tests/fixtures/assembly-exclusive-window.out).

  - [ ] **B08d — Resolve the reference libc-test failures and static/DSO coverage gap** — `OPEN`, P2.
    The fresh reference baseline still has 17 failing cases, not waived PASS:
    one API/header mismatch, five DSO/TLS cases in the static-only build, two
    strptime cases, two strtold rounding cases, two shell/wordexp cases, one powf
    exception case, two daemon timeouts and two fork/rlimit expectation failures.
    The exact observations and unproven root causes are recorded in the
    [benchmark notebook](docs/notebook/benchmark-evaluation-20260915.md#2026-09-27-musl-reference-failure-ledger).
    Add a separate genuine shared-library/loader lane for DSO tests; do not
    relabel those static-lane failures as compiler passes. Diagnose the remaining
    common failures against pinned source and environment without weakening the
    original assertions or increasing watchdogs to conceal a progress defect.

    - [x] **B08d1 — Supply the atfork limit test's unprivileged environment** — `DONE`, 2026-09-27.
      The isolated fork probe and unchanged atfork program fail to induce a
      fork error as root, but pass at uid 65534 with both reference and mixed
      archives. The common adapter now installs a hashed case wrapper for
      exactly the two upstream atfork variants; privilege-drop errors fail,
      other cases retain their identity, and the reference supervisor's
      five-second timeout/100 KiB stack are unchanged. Four native executions
      through the actual wrapper pass. This does not change the historical
      17-failure receipt or close B08d's full acceptance.
    Source-bound probes also reproduce strptime/strtold/wordexp/powf differences
    identically for GCC/Clang/EvoCC callers and both archives (96 records).
    The original daemon test passes in this isolated replay; million-fd
    exhaustion takes about 0.12 seconds, so fd count alone does not explain its
    historical five-second timeout. The later B08d6 filesystem-controlled
    reduction below closes that previously unexplained timeout subcase.
    [Cases, exact identities and observations](tests/fixtures/musl-reference-boundaries.out).

    - [x] **B08d2 — Execute the five DSO/TLS subjects with a genuine loader** — `DONE`, 2026-09-27.
      A separate shared composition reuses 1,268 hash-verified accepted EvoCC C
      objects and compiles both loader C units with EvoCC. Its 68 complex C
      objects remain reference-built; 13 assembly objects are separately
      accounted. All five unchanged original programs pass on both reference
      and candidate loaders, first in the reduction and then through the durable
      adapter (10 + 10 executions). PT_INTERP, object/library hashes and complete
      paired populations are checked. The existing musl/common-bench path now
      includes this supplement; old static receipts are not rewritten. This is
      **1,270/1,338 target-C** shared coverage, not full-musl PASS.
      [Composition, hashes and exact scope](tests/fixtures/musl-shared-loader.out).

    - [x] **B08d3 — Localize the two strtold binary128 reference failures** — `DONE`, 2026-09-27.
      The [small source reduction](tests/fixtures/musl-hex-tail-reduction.c)
      proves the pinned parser collapses both below-halfway and above-halfway
      nonzero tails to a halfway value when 29 leading-1 hex digits fill all
      113 significand bits. Both interpreters and GCC/Clang/EvoCC native O0/O2
      reproduce ...1112 twice; an extra guard-digit control restores ...1113
      and ...1111. This is a source-bound library defect disposition, **not**
      a changed benchmark or original-test PASS. A general library repair or
      dependency-pin change remains a separately declared configuration.
      [Exact rounding argument and executions](tests/fixtures/musl-hex-tail-reduction.out).

    - [x] **B08d4 — Localize powf's upward-overflow reference failure** — `DONE`, 2026-09-27.
      The [source-bound stage reduction](tests/fixtures/musl-powf-rounding-reduction.c)
      executes the pinned log/exp algorithm and a separate exact-conversion
      control. All 12 GCC/Clang/EvoCC O0/O2/archive combinations agree: for
      `powf(max_float, 1)` upward rounding, the pre-narrowing approximation is
      69 binary64 ULPs above the exact input, producing infinity and flags 0x14.
      The explicit early-overflow branch is not taken; exact conversion has no
      flags. This closes **source-library attribution**, not the original test
      or full musl. A library repair/pin change is a distinct configuration.
      [Bit patterns, identities and observer limitation](tests/fixtures/musl-powf-rounding-reduction.out).

    - [x] **B08d5 — Attribute the strptime and API/header baseline gaps** — `DONE`, 2026-09-27.
      The pinned `%s` source deliberately only consumes digits; `%z` requires
      four digits after its sign. The measured three-compiler/two-archive
      controls agree with those paths. POSIX does not require this test's
      particular `%s` tm value or `-06` input extension. Separately, the missing
      API executable follows two names absent from pinned `unistd.h`, not a
      runtime compiler error. All three original failures remain failures;
      no header/test or denominator is changed. [Source hashes and contract](tests/fixtures/musl-reference-boundaries.out).

    - [x] **B08d6 — Isolate descriptor-exhaustion output from host virtiofs**, 2026-09-27.
      The [fork/dup/exit reduction](tests/fixtures/descriptor-exit-filesystem.c)
      and unchanged daemon program fail only with mounted-file stdout; pipe and
      guest-local regular-file controls pass in both archive lanes. Children
      finish descriptor exhaustion and wait in exit_group/filesystem requests.
      The existing wrapper now spools just these two supervisors to a guest
      regular file, preserving exact logs/status/signals and the original
      five-second watchdog, 100 KiB stack and FD limit. Twelve archive/variant-path
      repetitions plus both actual retained executables pass; a deliberate
      timeout still fails. Ten wrapper tests on both hosts, all 296 ladder tests
      and Ruff pass. No full receipt is rewritten. [Evidence](tests/fixtures/descriptor-exit-filesystem.out).

    - [x] **B08d7 — Separate wordexp's library and shell failure families**, 2026-09-27 UTC.
      Fourteen small observations across three compilers, O0/O2 and both pinned
      archives yield 168 identical completed executions, not 168 correctness
      passes. The pinned library ignores WRDE_UNDEF, overrejects parameter braces
      with NOCMD and leaks command-substitution stderr; its helper arguments are
      visible. Direct dash controls explain dollar-quote behavior. Special-
      parameter/comment allowances are not generalized to waive other failures.
      Both unchanged originals still FAIL. Nested grammar assertions remain
      individually unadjudicated; this closes bounded source/environment
      attribution, not wordexp, B08d or full musl.
      [Small cases and exact observations](tests/fixtures/musl-wordexp-boundaries.out).

    - [x] **B08d8 — Repair numerical bugs in an explicitly patched dependency lane**, 2026-09-28.
      User-approved supplement leaves original musl sources, archives and FAIL
      receipts untouched. Backport the upstream hexadecimal guard-digit fix;
      also fix newly reproduced directed subnormal double rounding and exact
      powf(x,1) value/status handling. Integer-derived four-mode probes pass
      **71,040/71,040** checks across GCC/Clang/EvoCC O0/O2 and two base archives.
      **180/180** unchanged original-entry replays pass (15 entry names, eight
      distinct source programs); 192 total executions have verified link maps
      and unchanged inputs. 301 ladder tests pass. This closes only the patched
      numerical supplement, not the original musl lane or B08d parent.
      [Patch, recipe and retained REDs](tests/fixtures/musl-patched-numerics.out).

    - [x] **B08d9 — Verify explicit patched strptime compatibility**, 2026-09-28.
      In the separate dependency configuration, `%s` now populates local broken-
      down time with signed/overflow checks and `%z` accepts HH as well as HHMM.
      **540/540** focused checks and **24/24** unchanged original-entry replays
      pass across three compilers, O0/O2 and both archives. These extend musl's
      intentional behavior; they are not compiler defects or unchanged-original
      PASS. The numerical supplement's 192 executions remain green after sharing
      suite selection. [Bounded contract and evidence](tests/fixtures/musl-patched-time.out).

    - [ ] **B08d10 — Complete patched wordexp behavior** — `OPEN`, P2.
      Child stderr is now redirected before shell parsing/expansion, and helper
      positional arguments are cleared before expanding user words. These two
      repairs pass **576/576** focused checks across the paired 12-way matrix.
      Unchanged original-entry replays still **FAIL 24/24**, with 71 diagnostics
      per transcript versus 107 before repair. Nested NOCMD grammar, ignored
      UNDEF and pinned dash dollar-quote compatibility remain; the overall
      supplement deliberately reports FAIL. No parser safety is inferred from
      transport tests. [Patch and exact remaining boundary](tests/fixtures/musl-patched-wordexp.out).

    - [x] **B08d11 — Repair the missing API names in the patched dependency lane**, 2026-09-28.
      Added real UUCP-unavailable and bounded tmpfs timestamp-resolution
      dispatch, not header-only constants or guessed filesystem values.
      **324/324** focused checks and **12/12** original API-program links/runs
      pass across the paired matrix; all 79 API objects are accounted for
      (78 retained reference objects plus recompiled unistd.c). Other filesystem
      associations explicitly return EINVAL. Clang's separate C99-wrapper
      diagnostic adjustment and its original failure remain recorded. The
      original headers/archives/receipts are untouched; 303 ladder tests pass.
      [Exact scope, patch and evidence](tests/fixtures/musl-patched-conf.out).

    - [x] **B08d12 — Repair wordexp allocation ownership and error handling**, 2026-09-28.
      The previous supplement loses getdelim buffers on EOF/error and a word
      on vector-growth failure; it can also report success after sentinel-read
      or empty-vector allocation failure. Source-only fault injection reproduces
      **34/192 failures per build**, then **2304/2304 checks pass** across twelve
      patched GCC/Clang/EvoCC O0/O2 and reference/mixed-library executions.
      APPEND/DOOFFS, partial results and freeing are checked. The uninstrumented
      transport regression remains 576/576 PASS, but all 24 unchanged original
      wordexp entries still FAIL; B08d10's grammar boundary remains OPEN.
      All 304 ladder tests and targeted Ruff pass. [Exact evidence](tests/fixtures/musl-wordexp-memory.out).

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
  September 27 replay `20260927T092953Z` again passes **23/23**, **161 + 54**
  executed Unity checks, one upstream IGNORE and zero make-replaced objects.
  It also closes T05f's stale-archive relink regression; compiler identity remains
  in that receipt rather than being inherited from a later driver build.

- [ ] **B10 — One final serial 15-lane application campaign** — `OPEN`, P1, after tailored closure.
  Freeze the compiler/profile and run the existing `tools/bench.py` lanes without
  reducing populations. Retest passing zlib, zstd, cJSON, Mbed TLS, PolyBench, Lua
  and SQLite as well as repaired lanes. Close with fresh source-bound five-way
  accounting and zero unexplained compiler failures; deferrals/N/A/unusable
  oracles remain distinct from ALL PASS. [Current aggregate](tests/contracts/bench-receipt.json).
  September 27 follow-ups with one frozen subject close jq 69/69, TinyCC 11/11
  and the entire **220/220 c-testsuite** population, with zero c-testsuite
  deferrals/exclusions. These individual fresh lanes do not constitute the
  final 15-lane campaign. Historical binary128/strict-fenv waiver labels no
  longer excuse a new refusal under the now-implemented hosted profile.
  Complete torture replay `20260927T084053Z` also closes all **18** former
  binary128 refusals: **1,522 PASS / 0 compiler failures / 21 FP-deferred /
  146 not-applicable / 3 oracle-unusable**, fixed population 1,692. The six
  original raw execution failures retain their existing source-bound non-PASS
  dispositions; no source, timeout, option or applicability predicate changes.
  LLVM run `20260927T085137Z` adds **25** original-program PASS over the old full
  run, leaving only L10 as an unclosed compiler-failure classification. All
  1,735 identities remain and none of the 1,048 installed objects is rebuilt.
  The subsequent complete cJSON replay adds a seventh fresh lane. The eight
  remaining lanes still retain their earlier measurements; this is not B10 closure.

## Remaining numerical and language contracts

These are not proposals to redo scalar f32/f64, integer vectors, TLS,
constructors/destructors, GNU inline or raw-byte lexing. Those shared capabilities
have implementations and targeted evidence. The following boundaries remain
explicit; [the research brief][semantics] supplies the detailed probe contracts.

- [x] **L01 — Strict floating environment** — `DONE`, P2, 2026-09-27, admitted non-trapping profile.
  The September 27 report approves strict world-threaded FP, a separate `fenv`
  effect, four rounding modes and five sticky flags; dynamic rounding precedes
  full non-trapping FENV_ACCESS. Traps and Annex F claims remain excluded.
  September 27 targeted progress: the explicit rounding/status adapter and
  world snapshot passed 2,630,976 independent TestFloat value/status records;
  22,568 Arm instruction records agree in GCC/Clang O0/O2. Strict IR, effect
  derivation, text/proof preservation and MIR lowering are implemented in
  1.9.106/0.45. A reference caller of actual EvoCC-emitted strict IR passes
  62 checks in each of the four sealed native builds, including narrow integer
  conversion and a signaling-NaN definedness predicate that must not set flags.
  The source 1.9.107/0.46 batch adds dynamic-rounding lowering and C-HIR
  execution, including runtime constants, conditional conversions and quiet
  comparison builtins. Native source checks pass 40/40 in each of eight
  EvoCC/reference optimization combinations. The explicit target fenv resolver
  now covers the eleven get/set/clear/raise/save/hold/update functions; glibc's
  overflow/underflow raise also sets inexact, unlike the separate musl adapter.
  Both interpreters and eight native source/caller combinations pass the
  all-mask/all-rounding library probe. Two synchronized native threads preserve
  opposite rounding modes. Strict integer128 helpers now pass 36,784 exact
  values/status/control records in each of eight source/caller builds, plus
  768 raw-IR/prior-status checks in each of four reference callers; both source
  interpreters pass all four modes. FENV_ACCESS lexical scope now preserves
  ON/OFF/DEFAULT across preprocessing, inline substitution, direct/indirect goto,
  switch, loop exits, statement expressions and return conversions. Eight source
  programs, including the unchanged fenv library probe, pass both interpreters
  and eight native crossed builds; Clang19 O0/O2 controls also pass. After B08a
  approval, the eight-unit
  targeted replay now assembles all eight, including ARM64 ceil/floor/rint/rintf;
  the original four scalar-assembly objects also pass native value/status
  checks with four reference callers. Fresh original run `20260927T110110Z`
  now retains 1,271/1,339 objects in two complete libc-test replays, without
  additional failures or replaced objects. That closes the original replay
  requirement for this finite strict-FP implementation, not B08's full-library
  claim: the 68 floating-complex refusals, 17 reference failures and option
  accounting remain separate. No caller FP state is silently reset. Traps,
  Annex F and floating-complex/vector execution are not authorized by this
  closure. [Brief §A][semantics].

- [x] **L02 — Executable binary128 / long double** — `DONE`, P2, 2026-09-27, hosted Linux profile.
  The September 27 report approves real f128, the existing shared ABI classifier
  and audited hosted helpers. Fixed-RNE and strict helper behavior are separate
  gates; approval does not itself establish implementation or original-program PASS.
  The sealed hosted runtime exports the required TFmode helper families. A
  new 6,400-record binary128 arithmetic/FPCR/FPSR probe agrees byte-for-byte
  under GCC14/Clang19 O0/O2. The shared APFloat adapter now independently matches
  those records and 908,544 TestFloat arithmetic/comparison/conversion records.
  Quad FMA adds 24,532,992 independent records and 32,000 native bit/flag/control/
  errno records in each of four builds. Runtime default NaN and FMA payload
  selection differ from scalar hardware and have explicit regressions. This
  qualifies numerical prerequisites. The 1.9.110 batch adds actual f128 source/IR,
  exact-bit text, typed objects, both interpreters and quad HFA/vararg metadata.
  Arithmetic/NaN/dynamic-mode/vararg programs and E3001/E3063 negatives pass;
  independent proof mutation rejects changed bits, format and HFA role.
  Hosted native quad lowering now passes eight EvoCC/reference numeric builds
  (6,400 arithmetic, 32,000 library-FMA, 18,412 integer128, 9,256 integer32/64,
  304 format conversions and 327,680 predicate checks per build). Sixteen crossed
  ABI builds pass scalar/mixed/stack/HFA/vararg/callback tests, including a
  callback that clobbers the unpreserved SIMD upper halves. The raw MIR matrix
  also exposed and fixed an APFloat adapter panic at signed bits<1>; both pure
  definedness and strict total conversion now have all-width/mode regressions.
  Raw IR adds 5,376 total-conversion checks and 32,000 explicit FMA records in
  each of four native builds. The changed-crate gate passes **2,416 tests / zero
  failures / four retained ignores**, Clippy and fmt. jq's fresh 69/69 original
  replay also closes the discovered `-pthread` projection gap (B07). This closes
  the audited hosted Linux numerical/ABI profile, not freestanding helpers,
  Darwin quad, Annex F, floating complex/vectors or the full B10 campaign.
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
  The [fresh batch](tests/fixtures/remaining-boundaries-20260927.out) measures
  `_Float16`, `__fp16` and `__bf16` separately: 24 fixed-reference executions
  pass, including argument/return shapes. EvoCC still refuses all six cases;
  half/bfloat function signatures originally exposed E3101, and other paths report parser,
  value or interpreter diagnostics. These are now named reproductions, not an
  unexplained E3101 or an automatic generic-FP waiver.
  September 28's [T01b repair](tests/fixtures/typed-call-requirements.out) gives
  those signatures the existing E3049 classification; all eight original
  command replays retain refusal. This does not implement L03.

- [ ] **L04 — Floating complex execution** — `NEEDS_RULE`, P2.
  Torture retains 15 floating-complex originals. Specify complex arithmetic,
  exceptional values, conversions and ABI, with required runtime helpers and
  fenv interactions. Implement shared semantics and close with tailored numerical
  and crossed-ABI tests plus original programs. Integer complex support does not
  close this task. [Brief][semantics], [classifications][apps].
  The [batch probe](tests/fixtures/floating-complex-boundaries.c) separates
  component stores, finite arithmetic, exact construction and exceptional
  arithmetic for all three formats. Sixty reference executions are captured;
  only 24 are asserted finite controls, 36 are bit/status observations. Division
  by complex zero raises different flag sets between f32 and f64/f128 in the
  sealed references. All 68 current musl refusals and their original argv/source
  identities are indexed in the [batch packet][closure-batch]. Do not replace
  this numerical/helper decision with merely enabling complex signatures.

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

  - [x] **T01b — Select the typed call machine from reached signatures**, 2026-09-28.
    An independent positive control exposes E3007 for unused pointer/FP
    parameters whose actual arguments are integer constants. The shared C-HIR
    reachability scan now includes call signatures, preserving alias/transitive
    calls, unevaluated sizeof and uninitialized-read checks. Both interpreters
    and 54 fixed native controls pass. Nominal floating signatures now use the
    existing E3049 rather than generic E3101; no new numeric type is executable.
    All eight original half-interface replays retain named refusal. The 1,876
    affected tests pass with three historical ignores; Clippy/fmt and the
    changed gate against `1160db6` pass. No kernel/native acceptance is inherited.
    [Minimal cases, negative attempts and identity](tests/fixtures/typed-call-requirements.out).

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

  - [x] **T03d — Synchronize dynamic-stack forms and frame clauses**, 2026-09-26.
    The MIR spec regression went RED while the executable allocation/release
    control passed. EvoMIR 0.41 now reflects the existing Appendix D contract:
    stable fixed storage, explicit runtime allocation, disjoint outgoing area
    and top-only release without popping function-lifetime objects. The 28-test
    lowering suite, native spill variants, three object-lifetime and six VLA CLI
    tests pass; affected gate **1010 PASS / 0 FAIL / 4 historical ignored**,
    Clippy/fmt green. This closes an erratum, not a new compiler feature or T03.
    [Exact scope and commands](tests/fixtures/dynamic-stack-contract.out).

  - [x] **T03e — Close missing instruction sentinels and main-grammar errata**, 2026-09-28.
    An independent enum inventory goes RED at 52/59 instantiated variants;
    the expanded non-executable field sentinel now covers all 59 and 143/143
    registered fields, including nested assembly, declarations and relocations.
    Ten added operand-remapping cases preserve edge identities. A valid byval
    varargs proof replays, while four rehashed semantic forgeries are rejected.
    Main grammar tests expose and repair stale pointer, f128, dynamic-stack,
    varargs and other already-approved Appendix D spellings. Five text/field
    tests, 17 checker tests and the 1,644-test affected gate pass; Clippy/fmt
    pass. This closes verification gaps and errata, not new compiler features
    or the entire spec audit. [Exact scope and evidence](tests/fixtures/section9-contract-audit.out).

  - [x] **T03f — Enforce return/prototype compatibility at every written site** — `DONE`, 2026-09-28.
    Twenty small invalid programs previously passed parsing, including dead
    arms and indirect calls. The shared assignment rule now rejects these
    before execution/lowering; void/value mismatches receive E1207 without
    reusing E1205/E1206. Follow-up regressions preserve by-value va_list and
    full-width integer pointer typing instead of weakening the new check.
    Both interpreters, 80 explicit-Werror reference controls, 12 native legal
    controls and ten unchanged original musl object emissions/assemblies pass.
    All 1,339 original musl sources pass a read-only parser sweep; this is NOT
    1,339 compiled/assembled objects. The ten-crate gate passes 2,362 tests
    with zero failures and four historical ignores; Clippy/fmt also pass.
    [Exact scope and preserved failed attempts](tests/fixtures/return-conversion-constraints.out).

- [ ] **T04 — Bind final kernel claims to the actual final composition** — `OPEN`, P1.
  The September 26 common bench uses release `e8bddc0a…` for the reviewed
  configuration; its first launch stopped on the inventory-role defect T04b.
  The corrected launch completed its 7002-object first pass, then was found
  interrupted with a stale RUNNING summary. The verified prepared-input resume
  completed (`20260926T133321Z`) but is **INCOMPLETE**: all 7,002 second-pass
  compilations and the prepare fixed point succeeded; modpost finds eight
  unresolved bitmap assertions (K12b), and late generators cause GCC to replace
  13 target objects (T04d). No fresh link/boot occurred; no worker remains live.
  Later K02d/K02e/K02f/K02g are independently tested but deliberately absent from that frozen
  executable; its final integration must not be inferred from this run.
  The replacement bench `20260927T135325.342339Z` **completed with exit 0**
  at 21:43 UTC (September 28 local). Frozen release `65035621…` substitutes
  **7,002/7,002 target-C objects**: first pass 6,966 fresh plus 36 verified cache
  rows; actual-generated-input pass 7,002 fresh, zero resumed. The generated
  content fixed point, 848 generated-C recompilations, complete link and all
  installed-object hash checks pass, with zero GCC replacement. All **847
  modules link/BTF PASS**; signing, installation and execution remain separate.
  Image `62c29ae4…` boots and powers off with the actual nonce on `hvf-host`.
  The run uses explicitly permitted QEMU drift, not the locked QEMU executable.
  Its compiler contains Rust 1.98.0 standard-library code and excludes later
  frontend/logical-immediate repairs and T05k's Rust 1.97.1 pin. Thus this closes
  the measured uniform composition, not final-current-compiler integration or
  K13/K14. No build worker remains live. Earlier interrupted launches and
  September 26 negative evidence stay recorded. [Exact receipt and boundaries][kernel].
  A separately identified current-pinned run, `20260927T233955Z`, started
  with compiler `ab2eeb7b…` and configuration `95853321…` (connector/process
  events and FPU module enabled). Its **7,008/7,008 first-pass objects** passed,
  with zero cache reuse. It was later found stopped with a 1,920-object
  second-pass checkpoint and stale RUNNING record. After fixing T04e, recovery
  `20260928T075419Z` **completed** at 11:07 UTC: all **7,008/7,008 prepared
  consumers** compile, with zero old prepared-cache reuse; the generated-input
  fixed point and **849** generated export/module C units pass. Complete link,
  **848 module links/BTF**, and installed-object audits pass, with zero GCC
  replacement. Image `18b36fee…` boots and shuts down with its nonce on
  `hvf-host`, with explicit QEMU drift. This is the pinned current-compiler
  composition, not a stale image. [Exact identities and scope](tests/fixtures/kernel-current-composition.out).
  Its matching runtime now installs/signs all 848 modules, rebuilds the 17
  reference test-support modules, and preserves all 4,396 original non-module
  userspace files. Actual FPU loading/execution and connector delivery pass
  their targeted gates. K13/K14 and the expanded-KUnit configuration still
  require their own current acceptance; installed does not mean every module
  was executed.
  Before final acceptance, freeze the
  selected compiler, actual generated inputs, every installed target object,
  Image and module/runtime hashes. Use a uniform rebuild for any single-compiler
  claim; otherwise state incremental provenance. Close with link/boot and
  replacement-object audits, without inheriting stale runtime results. [Evidence][kernel].

  - [x] **T04a — Reject obsolete generated-module survey records**, 2026-09-26.
    An in-place module-to-built-in change leaves stale `.mod.o.cmd` files.
    The inventory now checks those records against a safe, unique current
    `modules.order`, rejecting rather than silently dropping them. Two RED
    regressions and all 243 K1 tests pass; Ruff passes. The retained prior
    composition still enumerates 6,765 records / 745 module metadata records.
    The completed new reference reconciles exactly one obsolete IOMMUFD module
    record: raw 7,040 → current 7,039, with five old generated files recoverably
    archived and the built-in object untouched. T04 acceptance remains pending.
    [Minimal reproduction and boundary](tests/fixtures/kernel-stale-module-inventory.out).

  - [x] **T04b — Distinguish same-binary HOSTCC from target compilation**, 2026-09-26.
    `HOSTCC=gcc-14` exposed 37 host-tool objects in the old compiler-name-only
    classifier. Frozen Kbuild host-header and target `__KERNEL__` markers now
    identify the roles; directories and `-fomit-frame-pointer` are not waivers.
    The two RED variants now pass, as do all 246 K1 regressions and Ruff.
    Original-command audit retains all 7,002 target-C and 37 named host records;
    no target source/object is deleted or dropped, no flag is newly ignored.
    [Exact reproduction and accounting](tests/fixtures/kernel-host-command-role.out).

  - [x] **T04c — Recover verified prepared-input compilation batches**, 2026-09-27.
    An interrupted second pass previously lost every completed compilation.
    Recovery now binds the entire prepared input snapshot, compiler, command,
    assembler image, successful preparation and retained object; symbol parity
    and the final generated-input fixed point are still checked. Fault-injection
    tests and all 250 K1 Python tests pass. The real 65-object C reduction resumes
    64 verified objects, compiles one, links and returns the new header value in
    all 65 functions. This is recovery validation, not kernel acceptance; the
    already-running worker loaded the old implementation and is unchanged.
    [Fixture, invalidation matrix and result](tests/fixtures/kernel-prepared-consumer.out).

  - [x] **T04d — Include late host-generated inputs in preparation** — `DONE`, 2026-09-28.
    Modpost regenerated ASN.1 sources/headers, CRC tables, consolemap, logo and
    RAID6 tables after the prepared-input pass, rebuilding 13 consumers with GCC.
    Current generated contents match pristine, but that does not authorize a
    provenance exemption or blind timestamp touch. The shared path now runs
    the original vmlinux/module producer graph before freezing consumer inputs.
    A real one-C-object Make reduction goes RED -> GREEN: header 7 -> 37,
    native exit 0, subject hash unchanged after the late producer. All 251 K1
    tests pass and the actual Kbuild preparation target exits 0. The fresh full
    7,002-object composition `20260927T135325Z` now completes the real producer
    fixed point, modpost, final link, 847 module links and nonce-bound boot.
    All installed target-C hashes survive: zero GCC replacements. The prior
    13 replacements remain a failed observation, not rewritten evidence.
    [Exact scope and evidence](tests/fixtures/kernel-late-generator.out).

  - [x] **T04e — Recover producer-replaced objects without crediting them**, 2026-09-28.
    Real recovery stopped because preparation replaced 17 first-pass working
    objects. Recovery now labels every row prepared-pending, without installed
    provenance, and requires prepared validation and installation. Two RED
    assertions, 275 K1 tests and a real 65-object native recovery pass; full
    coverage still requires the post-link audit. No object is exempted. The
    full run retains the same compiler/configuration; T04/K13/K14 stay OPEN.
    [Reproduction and identities](tests/fixtures/kernel-prepared-phase.out).

- [ ] **T05 — Final compiler-health and documentation gate** — `OPEN`, P1.
  After the last semantic repair, run workspace tests, Clippy `-D warnings`, fmt,
  relevant Ruff/Python checks, grammar/layout corpora and isolated offline replay.
  Keep historical ignored tests explicit. Refresh receipts, run
  `python3 tools/render-status.py`, update README/this tracker and commit the
  bounded result. Passing this gate alone does not close B10/K13/K14.
  [Workflow](README.md#layout-and-development).
  September 28's [post-repair checkpoint](tests/fixtures/compiler-health-20260927.out)
  at compiler source `13082c6` passes **2784 workspace tests**, Clippy/fmt, **563 Python tests**, Ruff,
  54/54 grammar shapes, both frozen layout corpora (**128 + 22**) and **13**
  isolated offline replays. Four historical Rust ignores remain explicit.
  This verifies current code, not a last-semantic-change or final campaign claim;
  T05 stays OPEN until its remaining-task dependencies are actually closed.
  The later [pinned checkpoint](tests/fixtures/kernel-regression-identity.out)
  includes the compare-immediate repair and passes **2807 Rust tests**, four
  historical ignores, **274 K1 + 315 ladder tests**, Clippy/fmt/Ruff, both layout
  corpora, 54 grammar shapes and thirteen isolated offline replays. The same
  final-campaign boundaries apply.

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

  - [x] **T05d — Select Rust tests when their external fixtures change**, 2026-09-24.
    Eleven RED assertions exposed missed `include!`/`include_str!`/`include_bytes!`
    dependencies outside crates, including a concealed failing test. Selection
    now follows literal/nested inputs and reverse dependencies; unresolved
    expressions fail closed by selecting their owner. Deleted/dirty inputs,
    raw literals, cycles and the actual compared-value CLI source are covered by
    [12 offline gate tests](tools/ladder/tests/test_gate_changed.py).
    All 248 ladder tests, Ruff, shell syntax and the real 7-test CLI suite pass.
    Stand-in Cargo checks prove selection/error propagation, not a workspace
    PASS; the final health gate remains T05.

  - [x] **T05e — Show new kernel attempts beside historical acceptance**, 2026-09-26.
    Three RED status-rendering controls hid RUNNING/ERROR/INTERRUPTED behind a
    retained old K6 PASS. The ASCII and generated status now show the selected
    attempt's state/run separately, without erasing historical results or
    claiming live-process monitoring. Two regression tests (including all
    three states) and all 250 ladder tests pass; Ruff and render/check pass.
    An unrelated application run does not relabel an older kernel attempt.
    [Regression](tools/ladder/tests/test_bench.py).

  - [x] **T05f — Reject stale archives and incomplete musl runtime evidence**, 2026-09-27.
    Failure-injected shell/adapter tests reproduce swallowed make errors, stale
    markers, incomplete runtime inventories, object replacement and cached
    bisection acceptance. musl now requires a fresh final trial, complete
    upstream runtime results, unchanged selected objects/watchdog and identical
    rebuilt/installed archives. Reference objects are restored on every exit.
    A separate controlled-timestamp cJSON RED requires its generated archives
    to be rebuilt; the unchanged full cJSON replay passes with **23/23** subject
    objects and no make replacements. These harness checks do not promote the
    old musl receipt: its original full campaign remains NOT RUN after repair.
    [Regressions and scope](tests/fixtures/benchmark-relink-boundaries.out).

  - [x] **T05g — Isolate the zlib benchmark from Python's standard codec**, 2026-09-27.
    Three fresh-process REDs reproduce search-path mutation, missing `subject`
    imports and a broken `gzip` import. The historical entry point now delegates
    codec imports outside its own real directory, preserving the caller's
    path; CLI and named benchmark loading retain their prior Subject. Six
    controls pass on macOS and sealed Linux, including symlink and byte-roundtrip
    cases; all 290 ladder tests and targeted Ruff pass. No benchmark result or
    compiler behavior was changed. [Failure and repair](tests/fixtures/benchmark-import-isolation.out).

  - [x] **T05h — Avoid host-stack overflow on long arithmetic chains**, 2026-09-27.
    The [600-addition C reduction](tests/fixtures/inline-deep-expression.c)
    parses but aborts the debug compiler in both interpreters and assembly
    lowering. Explicit continuations reuse the existing rewrite, task-machine
    and binary-lowering paths; constant admission, selected effects and UB
    guards remain. Eleven focused tests, twelve native controls and the
    1,869-test affected gate pass, with Clippy/fmt. Original page-table assembly
    is byte-identical; no running build's compiler or installed object changes.
    This is bounded debug robustness, not a release-kernel miscompile or a
    guarantee for arbitrary AST depth. [Evidence](tests/fixtures/inline-deep-expression.out).

  - [x] **T05i — Reject incomplete assembler-container execution** — `DONE`, 2026-09-28.
    Actual exit-17 failure injection proves the current K1 assembler can accept
    preceding OK records. The [isolated patch](tests/fixtures/kernel-assembly-verdict.patch)
    rejects failed processes, duplicate/missing/foreign verdicts and missing or
    empty objects, records exit codes and separates legacy cache keys. All 53
    affected tests and actual exit-17, empty-object and normal-exit controls pass
    on the candidate. Empty files cannot become verified cache hits. Integration
    waits until process 80898 exits, preserving its implementation identity.
    The integrated path then passes all 270 K1 tests, Ruff, three actual native
    failure/normal controls and two original KVM object replays with the pinned
    compiler. Existing receipts and the completed kernel build do not inherit
    this later hardening.
    [Exact reproducer, hashes and integration boundary](tests/fixtures/kernel-assembly-verdict.out).

  - [x] **T05j — Harden the other whole-application assembly verdict path**, 2026-09-28.
    The `subject.py` adapter also accepts actual successful assembly followed
    by runner exit 17. It now requires a successful process, exact complete
    unique verdicts and nonempty object files; stale/duplicate/foreign/missing
    observations cannot publish `assembled`. Eight unit scenarios and four
    real sealed runner controls cover RED, GREEN and normal execution; all
    310 ladder tests pass. This fixes that adapter, not the separately queued
    live K1 path T05i or a historical full-campaign result.
    [Small reproduction and exact identities](tests/fixtures/command-projection-packed.out).

  - [x] **T05k — Pin the actual host toolchain and revalidate current compiler code**, 2026-09-28.
    The ambient stable alias had moved to Rust 1.98.0; the repository now pins
    the contract's installed, hash-verified 1.97.1 with Clippy/rustfmt. A RED
    also shows toolchain-file changes incorrectly selected no crates; either
    filename now selects the whole workspace. All 2,801 Rust tests pass with
    four historical ignores, Clippy/fmt pass, 315 ladder tests pass, and fresh
    layout (128+22), grammar (54 shapes) and thirteen isolated offline replays
    pass. Six fixture assemblies match their previously native-tested bytes.
    The running kernel's frozen binary still contains the 1.98.0 library marker;
    it is not retroactively called a pinned-host build. The final campaign must
    record its actual pinned compiler. This does not close the T05 parent.
    [Tool identities, REDs and exact scope](tests/fixtures/pinned-host-toolchain.out).

  - [x] **T05l — Compare paired regressions by canonical program identity**, 2026-09-28.
    Three small tests expose duplicated collection prefixes, ordinal-based
    mispairing and ambiguous IDs. The shared comparison now indexes canonical
    IDs and rejects duplicate/foreign inputs. All 274 K1 tests pass; the actual
    retained selected receipt recomputes to `sched_ext:runner`. Historical
    observations and verdicts remain unchanged. This is report integrity, not
    a kernel repair. [RED, GREEN and health](tests/fixtures/kernel-regression-identity.out).

## Later roadmap gates — not substitutes for runtime closure

The September 29 revision-6 [Q2/Q3 implementation architecture][eqsat-plan] refines R01–R03
without closing them. R01a/R02a infrastructure and R02b development measurements
can proceed on correctness-admitted workloads. September 30's R02a/b/d
implementation and bounded Linux pilot are qualified; all three opportunity
ceilings and all A/B latency comparisons remain INCONCLUSIVE. Final acceptance remains
R01 → R02 → R03. `LATER` describes roadmap priority, not an implementation or
live-job status. Keep heavy campaigns serial under the existing ARM64 priorities.
Each policy/rule/shortcut track needs its own effect-ceiling preflight before
AI search. Conditional tasks below are explicit
design branches, not newly enabled semantics or required speculative projects;
record a justified not-needed disposition separately from implementation PASS.

- [ ] **R01 — G2 whole-kernel EqSat reproducibility and certificates** — `LATER`, P3.
  Produce the two required EqSat builds with equal Image hashes and independently
  check every optimized translation unit's certificate. Record exact optimization
  coverage; existing scalar proofs and unoptimized boot do not satisfy G2. [Gate][roadmap].

  - [x] **R01a — Policy-aware persistent optimization and proof provenance** — `DONE`, requalified 2026-09-29.
    Review reproduced three failures in the sealed-certificate suite, omitted by
    the previous four-crate gate. Commit `31234f5` repairs fixture placement,
    recovery-proof accounting, inherited scratch-file limits and checker
    packaging/identity admission. The full workspace gate passed: 2,850 Rust
    tests, zero failures, four existing ignored campaigns, all-target Clippy
    with warnings denied and formatting. The Python gate passed 118 tests;
    an isolated fresh-target package gate passed 560 tests. These are distinct,
    overlapping qualification scopes, not a unique-case sum. R02 application
    admission and the three effect-ceiling preflights remain open.
    [Review closure and exact scope](docs/notebook/eqsat-review-closure-20260929.md).
    Implemented design P0: one driver/evaluator persistent chain runner with policy
    identity including transaction count, proof/report export and wrapper-owned
    verification. Separate transaction result, stop reason and root counts;
    validate the closed state/rejection-reason tables. Distinguish completed proof
    rejection (PROOF_INVALID, exit 3), malformed bundles (CERTIFICATE_MALFORMED,
    exit 2), checker resource/infrastructure unknowns and missing/mismatched
    bundles; fault-inject each reason and merged exit-2/exit-3 accounting, plus
    missing-proof success versus explicit rollback. Unknown is not proof failure
    or success. Preserve transient search
    counters before EVP overwrites them; separate admission region/node quotas,
    recorded fuel/markers and actual matcher work, including legacy knob coupling.
    Preserve prior accepted regions on budget exhaustion. Any internal/proof/chain
    failure rejects whole-chain publication and staged state, not a successful
    prefix or silent baseline. Test chain parity, second-transaction rejection,
    budget-partial and zero-region cases, artifact identities and denominators.
    Fault-inject apply/history failure: valid zero proof cannot erase producer
    ROLLED_BACK_ERROR telemetry. An eligible no-rule case remains a legal no-op;
    independent eligibility counts warn but do not certify fallback classification.
    The earlier, insufficient gate had only 243 scoped Rust tests, 78 Python tests and the retained Darwin ARM64
    native proof bundle (two independently replayed transactions, executable
    exit 0). Three long Rust campaigns were ignored in that narrower gate; the
    full workspace gate has four ignored/NOT_RUN campaigns. The
    native smoke is not Linux-target or G2 acceptance. Integration also reduced
    and fixed fresh-definition binding and stale normalized-fact failures.
    [Implementation and commands](docs/notebook/verified-eqsat-implementation-20260929.md);
    [sealed fixture](tests/fixtures/eqsat-chains/verified-chain/README.md).

- [ ] **R02 — Q2 object code-size evaluation** — `LATER`, P3.
  V1 primary objective is size under revision 7, using pinned programs and result
  checks. Compare GCC/Clang `-Os`/`-O2` without a parity threshold. Latency is
  NOT_MEASURABLE_ON_THIS_LANE; no timing rerun/retuning. Final G2 and whole-scope
  acceptance remain separate from the six-TU pilot. [Roadmap][roadmap].

  - [x] **R02a — Performance manifests and measurement runner** — `DONE`, 2026-09-30, bounded registered Linux pilot.
    Design P1 is implemented through `tools/bench.py` and existing adapters/locks.
    The frozen P0/zlib/zstd/cJSON pilot completed 576/576 timed executions and
    48 warmups; CPU/RSS, ELF sections/stripped files, compiler/checker costs and
    every output are retained with explicit measurement scope. The independent
    auditor replays 138 certificates across 102 optimized chains and verifies
    all 108 checked-twin admissions. Ten read-only capture mutations are rejected;
    the full Rust workspace passes 2,861 tests (four historical ignores),
    Clippy and formatting; full Python ladder discovery passes 449/450 tests,
    with the real-capture skip executed separately (6/6 tests pass).
    Mixed-link coverage and host-debug compile costs
    are explicit: this is not a timed GCC comparison or whole-library PASS.
    [Qualified protocol, failures and claim ceiling](docs/notebook/eqsat-r02-linux-pilot-20260930.md);
    [tracked checkpoint](tests/contracts/eqsat-r02-pilot-receipt-20260930.json).
    Implemented requirements:
    Freeze effective UB-check/lowering profiles, shared chain identity, semantic
    flags, scoped budget units/mapping, checked-twin common IR/chain digests,
    closed rejection accounting, object coverage,
    timing boundaries, raw metrics,
    resource caps and separate performance receipts. REDs cover stale/mismatched
    artifacts, missing results, invalid metrics, interference and cleanup ownership.
    Historical September 29 foundations (superseded by the qualification above):
    the foundation batch removes repeated terminal no-op
    interpretation, duplicate per-function Admission construction and the extra
    whole-chain staged-state clone. Every changed module is still interpreted;
    every certificate, including the terminal no-op, is replayed. Checker
    identity checks retain both full reads/hashes and the final commit boundary.
    Policy-report v3 separates producer, checker, identity and wrapper wall time;
    v2 historical reports remain readable without fabricated timings.
    The frozen-plan audit covers checked native A/A declarations and
    artifact/source identity. The next bounded native adapter now implements
    exact-IR MIR transport, registered Darwin P0 A/A execution, per-attempt
    result/artifact checks, owned capture/lock/retention and raw paired statistics.
    Its bounded native pipeline was exercised after an explicit SDK/link-editor
    repair: 38 completed steps, 24 result-checked timed runs and four warmups of
    one binary; read-only capture audit passed. The earlier link failure is
    retained with zero timing samples. This is not Linux/application performance
    acceptance or an EqSat speedup; R02a was OPEN at that checkpoint. The broader
    pilot requirements named there are now exercised within the registered scope above.
    [Foundation scope and gates](docs/notebook/eqsat-r02a-foundations-20260929.md).
    [Native pipeline and limits](docs/notebook/eqsat-native-calibration-20260929.md).
    September 29 checkpoint: the default-cap diagnostic collector reuses the
    bench entry point and lock, with committed bounded input manifests and
    scoped/unknown counters. Native performance manifests and runners were still
    open at that checkpoint; the generated-IR collector smoke was not their acceptance.
    EqSat-off export now records a zero-transaction identity chain over the
    exact codegen IR. Independent byte-identity audit reports zero EVP proof
    successes; checked/unchecked fan-out was still open then and is now closed by R02d.
    [Scope and protocol](docs/notebook/eqsat-cap-census-20260929.md).

  - [x] **R02b — Opportunity/effect-ceiling preflight and A/A plus A/B pilot** — `DONE`, 2026-09-30, bounded pilot with negative/inconclusive outcomes.
    **Review correction:** DONE here describes the historical bounded implementation
    and capture, not a qualified timing protocol. The Linux lane reused Darwin's
    sample count without its own calibration and ran millisecond fresh processes.
    Its timing conclusions remain INCONCLUSIVE and cannot be used as latency
    acceptance. R02a1 and R02b1 below are mandatory follow-ups; the original raw
    capture/manifest remain unchanged. Code-size and checked-twin evidence stands.
    The first default-limit table covers six subject TUs and twelve transactions:
    no transient, admission or chain cap hits. Actual matching work remains
    NOT_MEASURED. The selected-pair census enumerates 512 feasible signatures;
    16 fixed-order suite evaluations at schedules 1/8 observe eight code classes.
    Two bounded rule syntax cells complete (5,994 visits, 1,222 fuzz survivors);
    two universal template controls prove, but no new rule is installed or
    qualified for utility. LLVM19's 34 reduced-node hints are not equations.
    Thirteen multi-rewrite regions expose eleven shortcut compositions without
    demonstrated fuel/matcher savings. Policy/rule/shortcut verdicts are all
    INCONCLUSIVE; no AI campaign is admitted. All 24 timing comparisons are
    complete: only 2/12 A/A are within the frozen effect floor and 12/12 A/B
    are inconclusive. Code-section reductions are real, not latency evidence.
    [Results, resource stops and retained failures](docs/notebook/eqsat-r02-linux-pilot-20260930.md).
    Implemented preflight contract, with every unresolved measurement retained:
    After R01a/R02a, first report whether default transient and certificate caps
    actually bind, with separate EVP fuel and chain counts. Freeze inputs and
    enumerate acceptance/rejection of transient-selected root/replacement pairs,
    not weight-driven alternative selection; test expected add/mul/shl byte ties and
    surrounding-code exceptions. Zero-weight certificate reservation only applies
    at a binding admission cap, not to transient work already spent.
    Also census residual misses after lowering/current rules using a pinned,
    semantics-qualified external reference plus a frozen bounded rule enumerator.
    Freeze a finite grammar lattice, visitation order, per-cell/total budgets and
    termination. Each discovery-qualifying cell needs an enumeration cap at least
    ceil(k * B_AI), k >= 1, where B_AI is the planned whole-track AI evaluation
    budget across seeds/restarts in the same suite-evaluation units. Require actual
    exhaustion at that scale; smaller quotas, wrong units or earlier resource
    stops cannot qualify SEARCHABLE. Revalidate after increasing B_AI.
    Record constants/widths/guards, proof/matcher applicability and
    materialization. Fuzz is filtering, not completeness. Beyond-enumeration
    discovery needs a useful template-proved witness in a healthy budget-exhausted
    cell and the same search space for non-AI. A completed witness-containing cell
    permits finite enumeration/efficiency claims, not discovery; crashes, unresolved
    evidence and out-of-scope depth cannot justify expansion of this campaign.
    Allow bounded R03f/c qualification probes, not a bulk
    discovery campaign, to resolve the preflight. Freeze separate policy/rule/
    shortcut searchable, enumeration-sufficient, no-effect or inconclusive verdicts.
    Calibrate A/A and compare matched A/B; no campaign bypasses its track verdict.
    Lowering-fold-disabled variants are attribution-only; three pilot families
    do not establish cross-family generalization. Retain all negative results.

  - [x] **R02b1 — Complete deterministic preflight and close review boundaries** — `DONE`, P3, 2026-09-30.
    The completed retained capture and committed audit-only supplement pass
    independent replay plus all eight real-capture mutation tests: 1,024 suite
    evaluations, 6,144 chains, 8,854 proof successes, 380 code classes and 966
    backend objects. All 1,222 templates were attempted: 1,174 proved, 48 UNKNOWN,
    none installed. Transient/certificate cap hits are zero at both schedules;
    the one-transaction schedule has 2,326 chain-limit stops, while all 3,072
    eight-transaction chains stop at NO_CHANGE. The new `(add, shl)` comparison
    leaves policy closure INCONCLUSIVE; rule/shortcut utility also remains
    INCONCLUSIVE. No AI or native timing claim follows. The original failed
    auditor, raw experiment and historical timing pilot are retained unchanged.
    [Repair, versioned replay and cap table](docs/notebook/eqsat-r02-review-repair-20260930.md#independent-completion-acceptance).
    First close the four September 30 REDs: integrity/resource failures cannot
    become template UNKNOWN, zombie-only process groups cannot count as live,
    every UB diagnostic guard is counted independently, and inconsistent public
    wall scopes report an error without panic/wraparound. Then freeze the same
    canonical six-TU population, all 512 selected-pair signatures at schedules
    1/8 and all 1,222 universal fixed-template candidates. Reuse content-addressed
    evidence, not another source/workspace clone. Recompute code-size classes,
    caps, proved/UNKNOWN counts and residual applicability with an independent
    audit. No native-time search fitness, arbitrary tactic scripts or new-rule
    installation. Unresolved grammar/matcher/shortcut utility stays explicit.

  - [x] **R02a1 — Calibrate and freeze a Linux steady-state timing protocol** — `DONE`, P3, 2026-09-30, bounded measurement with a frozen inconclusive result.
    The protocol, typed/mutation gates and default-control composition are
    implemented. Real mixed-link build and A/A pass independent audit: ten
    A/A comparisons admitted, 312 timed samples. The separately preregistered
    A/B ran once at the frozen 5% gates: ten comparisons, 312 timed samples
    and ten prior checked admissions, all result checks pass, but all ten
    latency outcomes are INCONCLUSIVE. The owned VM exited normally. The
    Python-version diagnostic rounding failure is retained; unchanged
    split-runtime replay verifies the original samples without a gate change.
    [Frozen result and receipt identities](docs/notebook/eqsat-r02-steady-protocol-20260930.md#frozen-single-ab-result).
    No rerun or larger budget follows. Final R02/Q2/G2 is not accepted by this
    bounded task. Formal policy/rule/shortcut verdicts remain INCONCLUSIVE,
    with all R03 activities NOT_ADMITTED; audit tooling is capped.
    [Adjudication and three owner decisions](docs/notebook/eqsat-r02-review-repair-20260930.md#formal-three-track-adjudication-and-stop-decision).
    Implemented protocol retained for reproducibility:
    After R02b1, make each measured process repeat internally for at least 1–2
    seconds with setup/startup excluded. Pin a guest CPU, keep host capture/hash
    work outside measured intervals, and retain every correctness result. Freeze
    a separate Linux A/A plan first; choose workload repeats and pair count from
    that calibration alone, then commit a new A/B manifest. A/A must satisfy its
    declared precision gate before A/B. Failed calibration is a measured limit,
    not permission to tune against A/B or retry the old millisecond protocol.

  - [ ] **R02c — Frozen object-code size comparison** — `LATER`, P3.
    Owner-approved S5 follows S1–S4: the same six TUs plus full applications,
    GCC-14/Clang-19 `-Os` and `-O2`, EvoCC unchecked release-context and checked
    diagnostic-contract builds separately. Per-family distributions/worst
    regressions, no cross-family average or parity threshold. Linked text is
    secondary. No further VM A/B timing; latency NOT_MEASURABLE_ON_THIS_LANE.
    [Sequential size work packages](docs/notebook/eqsat-size-work-packages-20261001.md).

  - [x] **R02d — Implement and verify the stated native UB-check profiles** — `DONE`, 2026-09-30.
    `--ub-checks=checked|unchecked` now reaches the driver and MIR; only native
    UbAssume enforcement is omitted in unchecked mode. Source obligations,
    interpreter diagnostics, other predicate users, explicit traps and FP checks
    remain. Real native trap/defined-parity tests and both interpreter profiles
    pass. The Linux pilot has 108/108 admitted twins (baseline twelve + policy
    96), each bound to the same canonical verified IR, chain and bundle, with
    a passing checked execution for that input before unchecked use. Profile
    identities stay separate; EqSat-off uses a zero-proof identity chain.
    [Native evidence, exact boundaries and mutations](docs/notebook/eqsat-r02-linux-pilot-20260930.md).
    Implemented explicit effective-profile plumbing and receipt identity;
    no switch or source/IR obligation is invented/deleted by the harness.
    REDs: ignored flag, missing identity, checked traps,
    defined-input parity and interpreter contract preservation. Every unchecked
    variant/input must have a passing no-trap checked twin. Fan out one canonical
    verified IR and immutable chain/bundle to both MIR profiles; require exact
    common digests, not just equal source/input/policy manifests. Reject mismatched
    IR/chain or missing/failed twins; native profile/cache identities remain
    separate. Include the EqSat-off identity-chain control. That is not
    proof against unmodeled UB. Measure checked
    and unchecked separately; GCC sanitizer coverage is not assumed equivalent.

  - [ ] **R02e — Guard-predicate folding domain, separately reviewed** — `LATER`, P3.
    **OWNER_APPROVED, ceiling first:** S4 begins with a MIR-only
    UNVERIFIED_DIAGNOSTIC known-bits guard fold, never installable or PASS.
    If its recoverable text is below the order of post-S2 B–A savings, freeze a
    negative result and do not extend semantics. Only a significant ceiling
    permits the independent total-predicate/obligation spec delta, checker and
    Lean proofs with R03f manual integration. Existing forms only. Preserve
    all source-definedness, poison and other predicate uses; native unchecked
    comparisons are not proof that a guard can be folded.
    If retained guards materially limit Q2, specify and prove total UB-predicate
    folds plus any independently checked proven-true obligation elimination.
    Preserve poison, other uses and source definedness; no silent P0 expansion.
    This conditional semantic/checker change is not required to measure baseline B.

- [ ] **R03 — Q3 measured self-improvement** — `LATER`, P3.
  **OWNER_DECIDED:** current V1 is a completed, frozen negative-admission study;
  this unchecked parent is the unachieved positive improvement milestone.
  All three tracks are formally INCONCLUSIVE/NOT_ADMITTED; do not start R03a/b/c/d
  or expand the census/auditors to force an affirmative result. See the
  [formal stop and decision record](docs/notebook/eqsat-r02-review-repair-20260930.md#formal-three-track-adjudication-and-stop-decision).
  Freeze the baseline, candidate policy and evaluation protocol, then demonstrate
  validated improvement on the declared workloads without correctness regression.
  Close with measured results and independent checking; more rewrites or an
  e-graph implementation alone do not establish benefit. [Roadmap][roadmap].

  - [ ] **R03a — Bounded AI and non-AI policy search** — `LATER`, P3.
    Implement design P3 only after R02b finds a useful control space, or after
    R03e and a fresh preflight. Closed candidate schema, deterministic shared-chain
    replay and checker gating; search seeds are provenance, not optimizer inputs.
    Compare AI with enumeration/random/evolutionary search using explicit candidate
    evaluation units, cache/retry accounting and separate model/proof budgets.
    Use deterministic development fitness, not adaptive native timing; preserve
    corpus exclusions. A finite/no-effect study may end without R03 improvement.

  - [ ] **R03b — Derived rewrite shortcuts** — `LATER`, P3.
    Implement design P4 after R03f and the R02b shortcut-specific verdict, over
    the current scalar domain and guard forms. Compare enumerated compositions;
    native-size opportunity is not evidence of actual search/proof savings.
    Expand shortcut certificates into existing rules; test substitutions, missing
    before-witness constants, fact scope, poison and mixed-width boundaries.
    No invented premise or unrecorded extra transaction. Measure actual matcher
    work and expanded proof cost; shorter spelling is not automatic fuel savings.

  - [ ] **R03c — New-rule Lean admission and registry integration** — `LATER`, P3.
    Implement design P5 after R03f: AI supplies only typed data with relation `eq`;
    trusted code tries a frozen fixed-order template ladder for the exact universal
    `RuleCovered` statement. Candidates cannot select templates; retain actual
    attempts/success in admission identity. Run only bounded qualification probes
    until R02b's rule-specific preflight is resolved; campaign admission cannot
    inherit a successful policy preflight.
    Reject proof scripts, import/statement injection, sorry and unauthorized axioms;
    audit actual transitive dependencies, including native-backed analysis.
    Integrate a bounded manually reviewed registry batch across producer/checker/
    spec/tests. Finite-width template admission requires R03g; arbitrary scripts
    are V2, not this task's critical path. R03b completion is not a prerequisite.

  - [ ] **R03d — Held-out ablation and measured promotion** — `LATER`, P3.
    After final R02 and the relevant candidate paths, freeze rules/policies before
    held-out A–E/non-AI evaluation. Match per-track grammar/limits and separate
    certificate quota, transient search and matching work. Report correctness, gain/regressions, uncertainty
    and discovery/compilation cost; no tuning on final feedback. Promote only under
    predeclared objectives/bounds. A negative study is not R03 improvement closure.

  - [ ] **R03e — Cost-aware extraction if the initial policy space is degenerate** — `LATER`, P3.
    **OWNER_DECISION_PENDING:** recommend deferral. Selected-pair limitations
    are observed, but a qualified useful alternative-selection witness is not.
    This would be a separately scoped optimizer change, not an extension of
    the frozen negative preflight or its audit tools.
    First make multiple legal e-class representatives selectable by actual cost,
    with deterministic ties, bounded cycle handling and existing certificate
    replay. REDs cover insertion-order bias, overflow, shared nodes and budgets.
    If justified, separately version rule masks/scheduling controls; no ignored
    fields or automatic checker expansion. Repeat R02b before any new AI campaign.

  - [ ] **R03f — Rule integration and independent matching gate** — `LATER`, P3.
    Before R03b/R03c, freeze a small manual integration budget and checklist for
    spec/digest, Lean, producer, checker and mutation tests; measure human/proof
    cost. V1 is not automatic high-throughput admission. A generic registry-AST
    matcher/builder is a separately reviewed A3/A4 change, with independent
    implementations and wrong-binding/type/guard/constant tests before enabling.

  - [ ] **R03g — Bounded-width proof interface if needed** — `LATER`, P3.
    Current `RuleCovered` quantifies all Nat width environments; JSON metadata
    cannot narrow it. If finite-width candidates are needed, first review/version
    an actual instance predicate, theorem/audit interface and producer/checker
    coverage map. Test zero, >64 and independently wide shift counts; review
    fixed-template reflection dependencies. Universal-template V1 can proceed
    without this extension; 64 individual proofs are not universal coverage.

  - [ ] **R03h — Optional unverified utility screening before proof** — `LATER`, P3.
    A shadow graph may estimate matches/extraction utility for development inputs;
    separate its types, caches and artifacts as `UNVERIFIED_DEV`. No proof,
    installable rule, executable or certified-state mutation can leave this path.
    Test every installation/codegen boundary against shadow artifacts. Prove and
    integrate survivors before actual verified metrics; screening is not a gain.

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

- [x] **R06 — Pinned Doom integration** — `DONE`, 2026-09-28, bounded headless profile.
  doomgeneric `dcb7a8db...` plus Freedoom 0.13.0 Phase 1: all 80 engine units
  and the platform adapter compile/link with EvoCC, zero target-C fallback.
  Two bundled demos, two repeats, EvoCC/GCC14/Clang19 produce identical full
  350-frame sequences (12 runs / 4,200 captured frames). The common bench
  entry owns execution; original 15-lane populations remain unchanged.
  Explicit GNU11/O2/-fwrapv handles the original signed angle multiplication;
  a source-bound, shared one-line video initialization repair removes initial
  wipe nondeterminism also reproduced by GCC self-repeats. The unadjusted
  attempts remain FAIL, not unchanged-upstream PASS. No compiler language
  expansion was necessary. Window/audio/live input, all maps and other ports
  are not covered. [Receipt](tests/contracts/ladder-doom-receipt.json),
  [profile and checks](tests/fixtures/doom-headless.out).

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
[closure-batch]: docs/notebook/todo-closure-batch-20260927.md
[complex-prompt]: docs/notebook/remaining-complex-research-prompt-20260927.md
[eqsat-plan]: docs/framing-draft.md
