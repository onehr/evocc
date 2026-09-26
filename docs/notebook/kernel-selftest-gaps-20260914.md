# Kernel test closure: remaining concrete gaps

Updated 2026-09-26. This is a diagnosis ledger. The latest full pair is
`.evidence/kselftest-full/20260922T033617Z/summary.json`: all 970 programs started
and have records on each kernel, but the aggregate verdict is INCOMPLETE, not
PASS. The historical 935-program comparison and observations remain below.
Targeted results and additional configurations use separate denominators.

### Dying-memcg control and reference tracing — 2026-09-26

K11g now reuses the frozen 30-group dead-cgroup workload and the existing
100-group threaded workload, including the original helper's ignored argument.
The old matched BPF Images retain 5/4 and 2/8 dying descendants after 20 seconds
(GCC/EvoCC). Root-cgroup fork/affinity/page activity clears all four populations
before the distinct-memcg displacement phase begins. A bounded trace on a fresh
30-group pair observes the remaining 3/5 CSS releases during that control, with
no dropped events. An out-of-line `drain_stock` probe alone cannot rule out
inlined drains or establish the final reference holder. Both trace flights
complete and shut down; their TAP assertion is capture completion, not an
original kselftest PASS. Plain observation programs have no TAP and remain
INVALID_OUTPUT in the generic parser, despite successful observation exits.

The source has the pattern addressed by upstream
[empty-stock reference release](https://lkml.rescloud.iu.edu/2608.2/04093.html),
but these controls do not establish that patch as the cause or remedy here.
K11g stays OPEN for the final reference-drop path. No kernel, original timeout,
test assertion or production runtime changes. Payloads total under 6 MiB and
reuse the existing 64 MiB scratch disk. [Sources and hashes](../../tests/fixtures/cgroup-stock-displacement.out).

### Conditional fall-through selection — 2026-09-26

K02e closes a bounded emission inefficiency observed in both original KVM
objects. When the taken block is adjacent, invert the emitted condition and
branch to the other block; do not reorder blocks or change logical edge copies.
EvoMIR 0.42 records this rule, including subsequent long-branch relaxation.
Two shape REDs become GREEN; three MIR tests, the CLI/interpreter fixture,
14 ARM64 native/mixed-link controls and the 1017-test affected gate pass.
Clippy/fmt pass, with four historical tests still ignored. Original command
replays retain exported symbols: text 171236 → 166792 and 34636 → 33804 bytes;
1101/219 redundant adjacent branches become zero. Nothing is installed, and
the ongoing full build still uses its frozen earlier compiler. This is not
an original KVM runtime PASS. [Exact evidence](../../tests/fixtures/conditional-fallthrough.out).

### Concurrent instruction publication outside the kernel probe — 2026-09-26

The follow-up corrects an important boundary: the first probe raced MOV stores
with execution, outside Arm's special concurrent-replacement instruction set.
Its old measurements remain but cannot independently establish an emulator bug.
The corrected aligned direct-B probe, with cache maintenance, sync-core and
the same retirement lock, still fails six concurrent variants on **both**
kernels under MT-TCG. All six memory-gate/locked-MOV/handshake controls pass.
HVF and diagnostic ST-TCG pass all 12 modes on each kernel; all six flights
boot and shut down. These 72 program results add only a 2.3 MiB initramfs and
reuse the scratch disk. There is no source-specific compiler workaround.
The measured boundary is sharper, but the invalidation/retranslation hypothesis
still needs instrumentation and the original sched_ext replay. K01 stays OPEN.
[Corrected protocol, authority, counts and hashes](../../tests/fixtures/instruction-publish-alias.out).

Historical first experiment (subject to the correction above):
K01b now reproduces a narrower shared discrepancy in an ordinary userspace
program built by GCC14 and Clang19, without kernel static keys, RCU or modules.
An aligned AArch64 MOV/RET is published with cache maintenance and registered
Linux sync-core barriers; a writer-preferring rwlock provides the data-retirement
grace period. Both kernels on MT-TCG pass the three handshake controls but fail
all five continuous controls, including single mappings and reader ISB.
Initial bad snapshots read `mov w0, #0` while the call returned `1`.

The same eight modes all pass on both HVF and diagnostic single-thread TCG;
all six flights boot and shut down normally. This removes several confounders,
but does not identify a specific QEMU fix or close the original sched_ext fault.
The tiny 2.3 MiB payload and existing scratch disk add no full runtime copy;
acceptance lanes, original verdicts and budgets are unchanged.
[Source, protocol, counts and hashes](../../tests/fixtures/instruction-publish-alias.out).

### Single-pass automatic representation zeroing — 2026-09-26

K02d removes redundant ordinary value stores after whole-object automatic
zeroing, while retaining typed Null provenance and all admission checks.
34 CLI tests, 12 sealed ARM64 native controls and the 1121-test affected gate
pass, including Clippy/fmt. The original new-config KVM commands reassemble
with matching exports: text 175264 → 171236 and 34996 → 34636 bytes.
The debug compiler's first 60-second timeout remains recorded separately;
the isolated optimized candidate completes both original commands within that
unchanged limit. No objects are installed and no runtime improvement is claimed.
The active uniform survey remains bound to its earlier release digest and
does not inherit this change. [Source and evidence](../../tests/fixtures/automatic-zero-single-pass.out).

### rseq registration-precondition control — 2026-09-26

K11e now has a real small C reduction and unchanged original pair, beyond the
earlier source-only diagnosis. Both retained Images pass its two TAP assertions:
glibc's existing registration causes EINVAL before address validation; disabling
registration before the probe's exec yields EFAULT at the same aligned invalid
address. Its own register/unregister cycle changes that result back and forth,
with the expected EBUSY duplicate-registration control. It never unregisters
glibc's own area or changes a process-wide test policy.

The unchanged direct `syscall_errors_test` still FAILs on both kernels; the
unchanged upstream `run_syscall_errors_test.sh` PASSes on both by setting
`glibc.pthread.rseq=0` before exec. All six flights boot and power down normally;
existing disk hashes remain unchanged. The first diagnostic packaging attempt
missed init's mandatory huge-page settings file and did not execute a test;
its failed setup capture is retained separately, not counted as PASS.

This closes **classification of an upstream direct-run precondition**, not a
compiler repair, removal from the 970-program population, or new full-campaign
acceptance. The 2 MiB initramfs reuses the 64 MiB scratch disk; originals use
the already retained runtime disks. [Source, commands and hashes](../../tests/fixtures/rseq-registration-order.out).

### Constant field-address instruction selection — 2026-09-26

TODO K02c closes another bounded compiler repair, not the original timeouts.
One same-block, immediately usable constant `ptr.offset` can now select the
existing load/store displacement. Index width/signedness, exact scale/direction,
encoding bounds and strict-alignment byte expansion are checked; memory accesses,
volatile events, guards, ordinary block arguments and atomics are not rewritten.

The permanent C/MIR reductions pass both language interpreters, pre-allocation
MIR, independent allocation verification, 48 AArch64 assembler checks and
12 sealed native controls (three compilers × O0/O2 × ordinary/strict alignment).
Changed gate against `b92cc58`: **1008 PASS, 0 FAIL, 4 historical ignored**;
Clippy and fmt pass. The two unchanged old-config KVM commands were recovered
from the subject tree and checked against their recorded hashes, avoiding the
concurrently changing reference build. Exported names/kinds still agree; text
is **187376 → 176264** and **37328 → 34996** bytes. Objects are **not installed**,
runtime is NOT RUN, and full-run totals are unchanged. EvoMIR 0.40 records the
bounded selection contract. [Reproducer and measurements](../../tests/fixtures/memory-address-selection.out).

### Runtime configuration build and artifact transition — 2026-09-26

K06/K12 now proceeds beyond Kconfig-only diagnosis. The reviewed candidate
`f2a06079b52ed50a40416b470417ffdbc104696c06eaac4ef9f62621e4f1ecbd`
completed with the frozen GCC14 toolchain, two jobs/eight GiB, network off,
in the **existing** `case/bpf-config` tree (3381.083 seconds, exit zero).
No Linux/workspace clone or full runtime disk was created.
The old reference Image `8fa43fb4…`, `.config` and `System.map`
are retained with copy-on-write under
`.build/floating-targeted/runtime-config-20260926/before-reference/`.
Historical receipts are unchanged; that retained Image is the old flight subject,
not whatever the mutable `case/bpf-config` directory produces next. Its matching
old runtime disks remain available. New reference Image is `13bd4d0b…` (40,716,800
bytes). `.build/floating-targeted/runtime-config-20260926/reference.json` and
`reference.log` preserve the completed command and identity.

Targeted compiler checkpoint: `io-pgfault.o`, `iommu-sva.o` and IOMMUFD's
`selftest.o` now all emit and assemble with the final `aef1a3a` release binary,
matching GCC exported names/kinds. Their original argv and generated-header
hashes are in `runtime-config-20260926/iopf-objects.json`; nothing is installed.
An unchanged iommufd ELF plus its original libc/loader/libcap form a **3 MiB**
diagnostic payload, reusing the 64 MiB snapshot scratch disk. Its **GCC reference
runtime now passes: 224 PASS / 0 FAIL / 5 SKIP**, versus 218/6/5 previously,
with the unchanged 45-second program limit and actual boot/shutdown. The five
skips are explicit `no_viommu` branches in the frozen upstream source. This runs
the unchanged ELF directly, not the complete campaign's outer wrapper, and is
not yet an EvoCC runtime claim. [Complete reference record](../../tests/fixtures/iopf-enabled-reference.out).

The module-to-built-in transition also exposed stale generated `.mod.o.cmd`
records surviving Kbuild. The [new inventory regression](../../tests/fixtures/kernel-stale-module-inventory.out)
rejects such records outside current `modules.order`; it does not silently
decrease the denominator. All 243 K1 tests pass, and the prior composition still
enumerates 6,765 records. After the new build completed, exactly one obsolete
`iommufd.mod.o` record was reconciled: **7,040 raw → 7,039 current target-C**,
including **847** current module metadata records. Five old generated files
were recoverably moved into `runtime-config-20260926/obsolete-reference-module/`;
the built-in object and reference Image remain unchanged.

The existing `case/bpf/out` is now reseeded from this reference. Its prior Image
`97d511d3…`, `.config` and `System.map` are retained under `before-subject/`.
The checked transition replaced generated build products and retired eleven
obsolete module/temporary-link files; it did not copy or delete source trees.
**The reseeded output is GCC input, not an accepted EvoCC composition.**

Uniform subject run launched at 2026-09-26 08:03 UTC through `tools/bench.py
kernel-full`: owner PID **11928**, worker **11932**, two jobs, low-disk batches,
release SHA `e8bddc0ac0fe8368c4534a2781a77d651ed2fedc8d4d33812e393d21686e312f`.
Invocation and state: `.evidence/bench/20260926T080306.877549Z/summary.json`;
capture: `.evidence/k1/20260926T080306Z/`. **Both processes exited** before
source compilation: unknown `-fomit-frame-pointer` exposed a second inventory
bug, not a need to weaken the option audit. With `HOSTCC=gcc-14`, the old
compiler-name-only classifier counted 37 host-tool objects as target-C.

T04b now recognizes the frozen Kbuild host-recipe include marker while retaining
target `__KERNEL__` generators even under `scripts/`. Its minimal saved-command
RED has two failures and zero import errors; all **246** K1 regressions now pass,
plus Ruff. `roles.json` lists each original command/hash and verifies **7,002
target-C + 37 HOSTCC**, with zero unreviewed target options and no artifact
mutation. The earlier 7,039 count was mixed-role, not 37 compiler failures or
test exclusions. The 847-module count is unchanged. No frame option was admitted
or silently ignored. [Role regression](../../tests/fixtures/kernel-host-command-role.out).

Corrected uniform run `20260926T080946.994870Z` is now executing: owner PID
**13358**, worker **13361**, same compiler/config and two jobs. At the first
checkpoint, **64/7,002** objects are assembled and symbol-verified, zero failures;
successful assembly is released. Its append-only unit journal is
`.evidence/k1/20260926T080947Z/units.jsonl`; the common bench summary is under
`.evidence/bench/20260926T080946.994870Z/`. Do not infer future liveness from this
observation. Actual-header
convergence, all-object/module audits and boot must pass before the subject
IOMMU replay. K13/K14 remain NOT RUN on this new configuration. Disk checkpoint:
108 GiB host free, 30 GiB build-volume free; no new large runtime disk.

### Original KVM replay after assembly repair — 2026-09-26

The two gated objects are now installed; **6,765/6,765** object hashes and all
generated inputs survive full linking, and all **745** modules relink with the
new BTF. This remains an incremental composition, not a uniform new-compiler
survey. New Image: `97d511d3b32b8c85e0cd9c7ef4dc447e4b8407c4c764f4f5aebd804285f043d1`.

Both unchanged original KVM programs were rerun serially with zero arguments,
all ten modes, the same fixed EL2 machine and **120-second program limit**.
GCC completes **10/10** for each; EvoCC completes **3/10**, enters mode four and
times out for each. All four boot/shutdown predicates pass. Thus K02/K03 remain
OPEN: this compiler repair does not solve the whole-kernel timing gap. The
4 MiB payload and 64 MiB snapshot scratch disk are reused. New module runtime
is NOT RUN; do not pair this Image with an older module payload.
[Exact commands, composition and outcomes](../../tests/fixtures/kvm-transport-replay.out).

### Assembly-aware value transport — 2026-09-26

TODO K02b closes a bounded compiler repair, not the KVM runtime timeouts.
Ordinary validated-SSA assembly retains its template, constraints, clobbers and
effects while trivial direct-edge input transport is removed. Fixed bindings,
asm-goto and indirect dispatch remain outside that optimization. A new minimal
C control independently reproduced an allocator rejection when the same value
supplied two different fixed input registers; per-occurrence boundary copies
repair that case while real tied-register conflicts still reject.

Nine MIR tests include forced spills in four register classes; the CLI case
also verifies that both interpreters still reject reached assembly with E3021.
GCC14, Clang19 and EvoCC native O0/O2 controls all pass. The affected gate is
**1001 PASS, 0 FAIL, 4 historical ignored**, with Clippy and fmt green.
After reboot, the current debug compiler emits byte-identical controls and
the six native controls were rerun successfully in the sealed offline image.

The unchanged original commands and generated headers produce both KVM objects
with matching exported names/kinds. Against the previously installed composition,
text falls **238032 → 187376** and **41816 → 37328** bytes; this includes the
preceding direct-edge repair and is not a causal timing measurement. At the
compile checkpoint they were **not installed**; the subsequent integration above
does not change any full-run total.
[Fixture, binary identities and results](../../tests/fixtures/asm-value-transport.out).

### Original udmabuf HugeTLB workload — 2026-09-24

TODO K11c now closes the two huge-page cases' environment prerequisite. The
frozen source requests **1024 × 2 MiB = 2 GiB**, before kernel/userspace overhead;
the ordinary 2 GiB guest and one-page pool cannot satisfy it. The
[small C reduction](../../tests/fixtures/selftest-hugetlb-reservation.c) shows
that a 4 GiB guest with one reserved hugepage still returns ENOMEM, while the
same guest with 1024 pages maps, writes and reads the complete original-sized
allocation on both kernels.

The common per-program policy uses a separate 4 GiB machine contract and a
verified 1024-page pool only for `drivers/dma-buf:udmabuf`. The fresh original
pair improves from **five cases then FAIL to 7/7 PASS each**, under the same
45-second timeout; all guest shutdowns and disk-hash checks pass. The locked
K6 machine is unchanged. 241 K1 tests, Ruff and shell syntax pass; only 5.2 MiB
of diagnostic/initramfs artifacts are added, without disk copies.
[Identities, controls and limits](../../tests/fixtures/selftest-hugetlb-reservation.out).
This is a setup repair on retained Images, not a new full 970-program result.

### Disk-backed /tmp mountpoint repair — 2026-09-24

TODO K11a closes a real setup defect rather than waiving openat2. Its unchanged
`resolve_test.c:54` requests `MS_PRIVATE` on `/tmp`; the full-suite init kept
ext4 backing but not a mountpoint. `fs/namespace.c:do_change_type` explicitly
returns EINVAL for that shape. A [small C reduction](../../tests/fixtures/selftest-tmp-mount.c)
on both retained Images reproduces the error and shows that self-binding the
same directory fixes it while preserving inode, data, filesystem and permissions.

The common init now applies this self-bind and rejects setup failure. Fresh
unchanged-original flights improve from **INVALID_OUTPUT, 0/88 executed to
88/88 PASS on both kernels**. `openat2_test` (116), `statmount_test_ns` (2) and
`mount_setattr_test` (34) also pass in both repaired environments: four programs,
240 nested PASS per kernel, no FAIL/SKIP, eight orderly shutdowns. The original
45-second limits and all test bytes are unchanged; 239 K1 tests and Ruff pass.
The [record](../../tests/fixtures/selftest-tmp-mount.out) binds exact Images,
payloads, original binary and captures. It reuses disks through snapshots,
creating only 6.1 MiB of small diagnostics, not a new runtime image.

This is a targeted userspace-environment repair, not a new compiler or complete
970-program result. The old runtime initramfs files and full totals remain
historical; K13 must use the repaired common init after final composition.

## Current closure queue — 2026-09-23

| Required result | Verified state | Remaining action |
|---|---|---|
| BPF/module kernel, 6,765 target-C records | PASS: `bpf-linkage-isolated-20260923` replaces one audited object and relinks against 6,764 unchanged objects; 812 generated inputs are unchanged, all 745 modules relinked, nonce-bound boot/shutdown PASS | Incremental composition, not a new single-compiler survey. The prior full run and its Image are retained |
| Matching runtime | PASS: 745 signed EvoCC modules and 17 rebuilt original GCC helpers installed with 970 programs. Three BPF variants each pass the explicit 10-group / 51-subtest module/BTF selector on both kernels, zero FAIL/SKIP | This proves execution of the selected modules, not loading every hardware driver or full BPF acceptance |
| Full kselftest, 970 installed programs | Pair finished, no reference reuse: GCC 532 PASS / 121 FAIL / 302 SKIP / 11 TIMEOUT / 2 INVALID_OUTPUT / 2 INCOMPLETE; EvoCC 529 PASS / 114 FAIL / 302 SKIP / 19 TIMEOUT / 3 INVALID_OUTPUT / 3 INCOMPLETE; zero NOT_RUN on either side. These are wrapper results; the nested sched_ext audit below finds an additional internal reference failure | Overall INCOMPLETE. Serial targeted replay resolves three of seven reference-PASS differences; four remain. The sched_ext NULL fault now also reproduces on GCC, so it is not an established EvoCC regression. Shared failures and collection build gaps remain |
| BPF original full program | Fresh reference: 651/5644 successful test/subtest counts, 127 skips, 11 failures. Subject TIMEOUT at the 300-second execution limit, after reaching group 478 | Keep the timeout and its observed nested BTF failure; correct runtime packaging and replay separately, without calling the incomplete program PASS |
| Expanded KUnit, 6,916 target-C records | Full composition and two audited incremental repairs completed. Latest built-in execution: EvoCC 1,432 PASS / 12 FAIL / 60 SKIP; GCC 1,438 PASS / 7 FAIL / 59 SKIP, each of 1,504 | Four FORTIFY query cases and timing-sensitive PM runtime remain reference-PASS differences, plus one upstream switch-entry XFAIL/SKIP. Preserve the original 19-FAIL and intermediate 11-FAIL captures |
| Filtered FORTIFY follow-up | Original 26-case suite on audited one-object incremental Images: 22/23/24/25 PASS, now one FAIL; failed assertions 96 → 66 → 31 → 2. All allocation cases and the local initialized-byte query now pass | Not a new 1,504-case run or module acceptance. Two nonliteral `strlen` ICE assertions conflict with the current literal-only specification; no assertion or rule was bypassed |
| Expanded KUnit modules | Both 809-module installations verified. The retained full-suite/module-tested Image actually loaded all 31 test modules and passed 765/765 cases; the GCC control also passed 765/765 with equal case/outcome multisets including duplicate multiplicity | That composition's module execution is complete, not the later filtered FORTIFY Image; its signature option is disabled, unlike the separately signed BPF configuration |
| Function-section placement | CLOSED: EvoIR 1.9.104 / EvoMIR 0.34, tailored interpreter/native/ELF tests, seven original objects, full workspace gate and isolated offline replay pass. Fresh full composition has zero modpost warnings (previously 2,515), complete link and real boot PASS | Preserve regressions; this does not close unrelated KUnit or kselftest failures |
| Further configuration coverage | Original Kconfig resolves 100 disabled requests in 27 collections in the small existing config-review directory | Build and test that configuration later; do not mutate either active kernel or advertise its proposed options as coverage |

The current-payload TLS/team confirmation is
`.evidence/kselftest-full/network-module-controls-20260920/summary.json`
(`adbaee3763419e623af5f2c4794306b2836481be581b6bc0d48a1bf1e2713679`).
It confirms earlier preflight passes, not a newly repaired compiler defect.
The unchanged locked K6 boot and selected 9-program K7 acceptance remain
separate. Neither the older KUnit pair nor a reference-only control closes
the new rows above.

### Matched runtime repairs — 2026-09-23

`runtime-repair-pair-20260923/results.json` under `.evidence/kselftest-full`
records 18 fresh serial flights: nine selections on each kernel, the same
upstream programs and matching signed module payloads. `test_progs`,
`test_progs-no_alu32` and `test_progs-cpuv4` each pass 10 groups / 51 subtests
with the explicit module/BTF selector. Unfiltered `test_tcpnotify_user`,
`drivers/net:xdp.py` and `drivers/net:napi_threaded.py` also pass both sides.
The new subject Image is `f2d59e05e2a3b15cadc56b3479574f81716be7a7a6a3dd280568e42b81124280`.
The canonical composition receipt now names it; the old complete 970-program
results remain bound to their older Image and are intentionally displayed as
STALE_IMAGE, not applied to the replacement.

The unchanged IOMMU test requires 128 MiB of hugetlb memory in
`domain_dirty128M_huge`; its former one-page (2 MiB) setup cannot satisfy it.
The existing runner now reserves 64 hugepages only for `iommu:iommufd`, on
both Images, and records the override. Init validates and verifies the pool;
the original program, arguments, assertions and timeout are unchanged.
The fresh `iommu-hugepages-20260923/results.json` pair improves **196 PASS /
28 FAIL / 5 SKIP to 218 PASS / 6 FAIL / 5 SKIP**, each of 229 subtests.
This removes 22 setup failures without claiming the program PASS. The six
remaining failures all request IOPF: the frozen configuration has no
`CONFIG_IOMMU_IOPF`, so `iopf_queue_alloc()` is a NULL-returning stub, and
`mock_dev_enable_iopf()` returns `-ENODEV`. A separately built configuration
is still required; neither running Image is modified to conceal that gap.
The existing 1.8 MiB candidate directory now resolves the hidden IOPF symbol
through `CONFIG_ARM_SMMU_V3_SVA=y` using the original Kconfig, not by editing
generated headers. The configuration-only receipt is
`.build/floating-targeted/selftest-iopf-config-review-20260923.json`;
this is a build prerequisite, not a runtime PASS.

Two other shared failures now have exact non-compiler causes. ALSA's
`snd_async_del_handler` missing-TIMER case is repaired below.
The original ntsync suite passes 10/11 cases on each kernel; `wake_all`
line 971 incorrectly expects zero from `NTSYNC_IOC_CREATE_EVENT`, although
`drivers/misc/ntsync.c:ntsync_create_event` returns a newly allocated file
descriptor (9 in both captures). The adjacent three allocations test for a
nonnegative descriptor. The frozen test's FAIL is retained; its assertion
has not been weakened or rewritten to obtain a passing verdict.

The ALSA runtime repair now passes the unchanged original `utimer-test` on
both kernels (2/2 nested cases each), in
`.evidence/kselftest-full/alsa-cleanup-repaired-20260923/results.json`.
The tiny device-free `tools/k1/tests/alsa-timer-async.c` first aborts against
unmodified 1.2.14 at the same assertion (exit 134); after the patch it exits
zero, checking two shared handlers, last-handler notification shutdown and
close-time cleanup. The real library repair adds TIMER handling to both
cleanup switches, under ALSA's existing `BUILD_PCM` guard; no assertion is
removed. The pinned upstream archive, explicit local patch and GCC-built
library hash are recorded as a **userspace dependency**, with no EvoCC code
generation credit. The checked-in runtime installer also builds the patched
library and passes this regression. Both new runtime disks are COW copies;
the original failing payloads, modules and kernel Images remain intact.
This closes the observed ALSA failure, not unrelated sound-hardware tests.

### K08/K09: measured API/test mismatches — 2026-09-23

These are now **closed classifications**, not compiler repairs or extra PASSs.
The unchanged originals and their FAIL records remain in the full receipts.
The checked-in reductions are `tests/fixtures/ntsync-create-event.c`,
`hw-breakpoint-kernel-task.c` and its userspace loader; the compact measurement
is `tests/fixtures/kernel-api-contracts.out`. Both programs pass on each existing
BPF-composition Image with valid TAP, exit zero and orderly nonce-bound shutdown.
The diagnostics are GCC-built; the ntsync module comes from each Image's own
signed installation. The breakpoint diagnostic is separately reference-built,
signed, actually loaded and unloaded. This is not another full KUnit flight.

**ntsync:** frozen `Documentation/userspace-api/ntsync.rst:137–154` specifies a
descriptor return; `drivers/misc/ntsync.c:721–738,789–807` allocates, installs and
returns that descriptor. Frozen `wake_all:971` instead expects zero, unlike its
three immediately preceding allocations. The reduction keeps fd 0 occupied,
observes descriptor **5 on both kernels**, and checks FD_CLOEXEC, event READ/RESET,
EFAULT and EBADF. The upstream maintainer's [July 23 correction, patch 1/5](https://list.winehq.org/hyperkitty/list/wine-devel%40list.winehq.org/thread/VRHDVNLX6JZDTN3IZ6MIQNFGFF34ODQB/)
changes that same expectation to a nonnegative-descriptor check. It is supporting
upstream evidence, **not a patch applied to our frozen suite**; no kernel change
or synthetic local fix is needed to explain this failure, and 10/11 stays 10/11.

**ARM64 breakpoints:** every failing original uses `register_test_bp` with
`&break_vars[idx]` and a non-NULL task; it fails its first positive-allocation
assertion at `hw_breakpoint_test.c:70`, before testing capacity/accounting.
`arch_build_bp_info` classifies the address as EL1;
`hw_breakpoint_arch_parse:577–582` then explicitly refuses EL1 + task with EINVAL.
The small module observes CPU-wide success and **-22 for both task forms** on
both kernels. This is the selected ARM64 API boundary, not insufficient emulated
slots, a missing kernel option, or evidence of a register-allocation bug.

| Original failed case | First unsupported request |
|---|---|
| `test_one_task_on_all_cpus` | `cpu=-1, task=current` |
| `test_two_tasks_on_all_cpus` | `cpu=-1, task=current` before creating the second task's breakpoint |
| `test_one_task_on_one_cpu` | first online CPU, `task=current` |
| `test_one_task_mixed` | first online CPU, `task=current` |
| `test_two_tasks_on_one_cpu` | first online CPU, `task=current` before the second task |
| `test_two_tasks_on_one_all_cpus` | first online CPU, `task=current` |
| `test_task_on_all_and_one_cpu` | `cpu=-1, task=current` |

The unmodified `test_one_cpu` and `test_many_cpus` controls pass on both original
expanded KUnit runs; both configurations enable ARM64 hardware breakpoints and
the suite, and the >1-CPU/available-slot setup checks were reached successfully.
The seven task-bound EL1 capacity scenarios cannot be claimed as exercised on
this API. A future architecture-appropriate upstream test redesign or separate
architecture would have a separate identity; deleting these assertions or
changing the compiler to bypass the kernel's rejection is not a fix.

Frozen source SHA-256:

```text
drivers/misc/ntsync.c                         2fb92d6befb6a67cfba3ccab51bd65bc313994fe5c20c1223d3b79cb74129e45
tools/testing/selftests/drivers/ntsync/ntsync.c fe18dff484db066b198250b97f3cf4985d4c6cdbe4e95e44991f102bf1aabc63
Documentation/userspace-api/ntsync.rst        9800abd461ecbc6af8c045941631238e0a86e992d94af85ed27f7f70076ebc9f
kernel/events/hw_breakpoint_test.c            26edba8c8257ceef40c1a3c80066cf0f1e4d0a6bd13b504da95358095198f2a4
arch/arm64/kernel/hw_breakpoint.c             09d821905139bc157f84f90130335fe0705ac238c6e248cd655b0cd64c05584c
```

Original captures: `.evidence/kselftest-full/runtime-repair-pair-20260923/results.json`
and `.evidence/kunit/expanded-{reference,subject-query}-builtin-20260922/boot.log`.
The two new diagnostic captures live under
`.build/floating-targeted/ntsync-contract-20260923/{reference,subject}-pair-boot/`.
They preserve the original Images/runtime disks and do not alter either frozen
test population, historical numerator or acceptance receipt.

### Serial replay of the seven subject differences — 2026-09-23

The existing paired runtime, Images, assertions and declared budgets were used
for 14 serial flights. All seven reference programs pass. The subject now passes
`pidfd:pidfd_test`, `net:ioam6.sh` and `net/forwarding:sch_tbf_prio.sh`; the latter
retains its original 5% tolerance. No compiler or kernel change is attributed to
these recovered results. The four other subject programs remain TIMEOUT:

- `sched_ext:runner` is **not just slow**: the original `reload_loop` reaches
  a kernel NULL dereference at `__scx_update_idle+0x290`, reading
  `scx_root->has_op` at address `0x1c0`, on multiple CPUs. The static-key/call
  path is under diagnosis; no kernel NULL check or test waiver is introduced.
- `ublk:test_stress_04.sh` reaches its 45-second wrapper limit and then stalls
  during shutdown sync. A later diagnostic init requests SysRq blocked-task
  stacks after the same test; that invocation instead passes and powers down
  at 4.983 seconds. This is evidence of intermittency, not a repaired defect.
- KVM `demand_paging_test` completes three guest modes before its original
  120-second limit; `memslot_modification_stress_test` finishes two modes and
  is interrupted in the third. The reference completes each original matrix.

Captured results: `.evidence/kselftest-full/seven-differences-serial-20260923/results.json`
(`3465ddded474ca249d1b93c83da08bd7d80afd3af15fa0288a4191746c8e5a59`).
The separate ublk diagnostic is
`.evidence/kselftest-full/ublk-blocked-stacks-20260923/result.json`
(`67037a60878fa2dde64344b480d6ab5d9d6a5e7ec4496e0e76788f4d4b0853aa`).
Neither changes the old 970-program totals or constitutes a new full campaign.

The later [worker-verdict reduction](../../tests/fixtures/ublk-background-exit.out)
exposes a separate upstream shell property: each background worker has its own
`ERR_CODE`, and `wait` with no operands does not forward its failed status.
Fault-injected additions/removals in the unchanged original yield three child
failures followed by parent PASS/exit zero. The shared parser now retains their
failure summaries/helper diagnostics; expected block I/O errors are not failures.
All 238 K1 and 240 ladder regressions pass. Re-parsing 122 retained ublk flights
does not change their outer outcomes; six already-failing flights gain explicit
inner-failure records. No historical receipt is rewritten.

One new serial pair on the original matched Images/runtime disks also passes
the unchanged `stress_04` test and powers off at 3.465313/4.656888 seconds. The
existing post-test SysRq diagnostic reports no blocked tasks. This again does
**not** explain the old shutdown stall: K04a closes verdict handling; K04 remains
open. Capture: `.build/floating-targeted/ublk-verdict-20260923/result.json`;
source, payload, Image, disk and serial hashes are in the linked record. The
current two-object KVM candidate was not substituted into these old payloads.

The subsequent `sched-blocked-stacks-20260923` diagnostic reaches the original
120-second program limit during `init_enable_count`: its children are sleeping
in `do_nanosleep`, and the parent is in `do_wait`. The test performs two sets of
1,024 forks. This capture does not reproduce or explain the earlier NULL crash.
The diagnostic SysRq dump interleaves the end marker, so its parsed program
result is INCOMPLETE; the raw marker records rc=124. The VM's orderly shutdown
is not a test PASS. A native branch/nop static-key control passes on GCC14,
Clang19 and EvoCC (`assembly-static-key-toggle.c`); it does not close sched_ext.

### Reduced scheduler reproduction and verdict repair — 2026-09-23

The unchanged upstream `hotplug` followed by unchanged `reload_loop`, on one
boot with the original 120-second per-program limit, reproduces a NULL read
at `balance_one+0x144` (`scx_root->ops.flags`, address `0x120`) on the subject.
Removing all GDB/watchpoint stops and running the same reduction serially on
both Images then produces **the same NULL field access on the GCC reference**
(`balance_scx+0x74`, 17.547 seconds); this subject invocation instead passes
and shuts down at 54.101 seconds. These are intermittent shared failures, not
proof of an EvoCC miscompile or a closed test. No source/test assertion,
kernel NULL guard, Image or module payload was changed. The diagnostic init
only selects and orders these two existing tests.

Results: `.build/floating-targeted/sched-key-watch-20260923/plain-pair/results.json`
(`23db63ef5b52a86ebd4fcf3c25c2270164b79154d45134cb7c4e6b250ccd1141`).
The reference serial digest is
`81d78e74430440a34b3d3abd64c7c73ed9c86a0157d790490c007c0dcb59e74a`;
the subject serial digest is
`94c0ddc6b1bf2f390ebc6c3be8569ef00256ef3fe025c1c2d105ae2239e41225`.
QEMU remains `45d74da5…9d91a3e`, fixed Cortex-A72 TCG, four vCPUs, 2 GiB.
Single-thread TCG is a separate diagnostic, not an accepted replacement lane.
That diagnostic has now finished: **both sides time out at the outer 195-second
limit**, after `hotplug` fails its original one-second scheduler watchdog;
`reload_loop` remains incomplete. The capture's `hotplugPass` boolean only
reflects the runner's zero exit status and is **not** an assertion PASS.
The raw logs contain `not ok 1 hotplug` / `FAILED: 1` on both sides. This does
not resolve the intermittent NULL fault, justify a longer watchdog or replace
the fixed multi-thread TCG lane. Capture:
`.build/floating-targeted/sched-key-watch-20260923/single-tcg-pair/results.json`.

The upstream scheduler runner also unconditionally returns zero after printing
its `PASSED` / `SKIPPED` / `FAILED` totals (`runner.c:198–203`). The old complete
reference capture has `22 / 0 / 1`: `allowed_cpus` fails at line 64, despite the
outer `ok 1 selftests: sched_ext: runner`. The parser now preserves that outer
verdict **and** records the complete nested summary; both the full-suite and
selected-pair gates refuse all-tests PASS when those summaries contain failures
or skips. A minimal RED reproduces the previously empty failure list; the
corrected Python gate passes 214 tests. Historical captures and 970 wrapper
totals are unchanged; the new audit prevents interpreting them as all inner
assertions passing. The later seven-program serial reference genuinely prints
`23 / 0 / 0`, so its outcome is not retroactively discarded.

Separately, compiler commit `752e193` folds pure executable integer arithmetic
through the existing executed-expression evaluator. It does not use the more
permissive ICE evaluator or discard entered labels, effects or reached UB.
The focused controls pass both interpreters and GCC14/Clang19/EvoCC native;
the changed gate passes **1,080 tests, zero failures, three historical ignored**,
Clippy `-D warnings` and fmt. An original-command, actual-generated-input
replay of `kernel/sched/build_policy.o` emits in 13.226 seconds and assembles;
including the preceding Boolean-normalization repair, its text-size count
falls from 729,626 to 663,870 bytes. This object is **not installed** into a
kernel: these are code-generation results, not a new boot or timeout closure.

The new execution receipt is `tests/contracts/kernel-expanded-kunit-receipt.json`;
it remains FAIL because completed execution is not all-tests success. The latest
31-module flights are `.evidence/kunit/expanded-reference-modules-20260922` and
`.evidence/kunit/expanded-subject-query-modules-20260922`. The initial built-in
comparison preserves seven common per-task EL1 breakpoint failures, one Poly1305
context-padding regression, ten FORTIFY query failures, and the PM runtime
asynchronous-pending expectation failure. The one additional stackinit skip is
its upstream switch-entry XFAIL, not a missing test.

GCC's raw serial transcript contains one explicitly replayed printk prefix that
confuses the upstream KTAP parser. `console-normalization.json` records the exact
partial prefix and identical complete replacement; the raw capture and original
parser result remain intact. The normalized capture is parsed by the unchanged
upstream KUnit parser, with no synthesized test result or discarded failure.
Poly1305's compound-literal padding and allocation-hook statement-result extent
transport now have small RED/GREEN tests; complete repaired kernel execution is
recorded in `expanded-subject-repaired-builtin-20260922`: all 1,504 cases ran,
with 1,433 PASS / 11 FAIL / 60 SKIP. Poly1305 and six previously failing FORTIFY
cases now pass. PM runtime also passes this run, but no PM implementation was
changed: its earlier asynchronous-pending failure remains in the first capture.
The repaired composition is explicitly incremental, not a new single-compiler
survey: 6,914 original objects plus two newly compiled EvoCC objects, all audited.

Four reference-PASS FORTIFY cases remain after that run. Two further minimal
repairs now pass both interpreters and fixed ARM64 GCC14/Clang19/EvoCC execution:
recognize `alloc_size` between a function's return pointer and name, and retain
proved sizes across genuinely dead branches. Query-dependent branches must first
consume substituted arguments; unresolved queries are not frozen to their early
fallback. Native evidence is `kunit-regressions-20260922/native-query-final.json`
under `.build/floating-targeted`; the original FORTIFY object replay has identical
defined symbols. The subsequent relink is now complete in
`tests/contracts/kernel-expanded-kunit-query-receipt.json`; actual built-in
execution is `expanded-subject-query-builtin-20260922` (1,432 PASS / 12 FAIL /
60 SKIP). These narrow fixes do not close the four remaining original FORTIFY
cases; the real replay, not their small-case PASS, controls the ledger. The PM
pending-resume expectation fails again in this flight.

The latest full-suite/module-tested Image is `985fc7ca62a06a07e2eadd78ff306d955258cf0a52a760ab6c65c49a8ccbe9d6`.
Its matching unsigned 809-module runtime is
`kunit-runtime-20260921/subject-repaired` under `.build/k7-coverage`, with disk
SHA-256 `5df403aee1f87ef4c2c06019b40dcba7c6255443a41327fdc673ca0777929b99`.
Actual loading and execution of all 31 KUnit test modules passes all 765 cases,
with no empty suite, load failure, or lost duplicate case. The composition remains
incremental: 6,914 base objects, the earlier Poly1305 repair and the final FORTIFY
repair, not 6,916 objects rebuilt with the final compiler.

The non-compiler boundaries remain explicit:

* Seven common failures request per-task EL1 hardware breakpoints. The test's
  `register_test_bp()` uses a kernel address and task; ARM64
  `arch/arm64/kernel/hw_breakpoint.c` explicitly returns `-EINVAL` for that pair.
  Both kernels' recorded FAILs remain FAILs, not synthesized passes.
* `stackinit/test_switch_1_none` is an upstream `ALWAYS_FAIL`/XFAIL case:
  `lib/tests/stackinit_kunit.c` explains that declarations before the first switch
  case cannot be initialized on case entry. Its `kunit_skip` path is preserved.
* `pm_runtime_error_test` expects a pending asynchronous resume to remain pending
  until `pm_runtime_barrier()`. `drivers/base/power/runtime.c` documents zero when
  none remains, and its worker clears `request_pending` before handling the
  resume. The first FAIL and subsequent PASS are timing-sensitive observations,
  not evidence that the compiler's PM lowering was repaired.
* `fortify_test_known_sizes` includes two ICE expectations for non-literal
  `__builtin_strlen`. EvoIR Section 10.1.18 currently grants ICE status only to
  literal operands; the broader constant-memory queries also lack precision.
  This remains a recorded compiler/profile limitation, not an excluded pass.
  A further 22-line allocation-wrapper reproducer is now isolated at
  `.build/floating-targeted/kunit-regressions-20260922/inline-allocation-extent.c`:
  both frozen-compiler interpreters return 1 instead of the required 0, with
  no compile error. An ordinary inline wrapper loses its callee's proved
  allocation extent at the generated return/result boundary; a direct call
  control, including an intervening call, succeeds. This isolates one of the
  devm-family precision gaps without claiming that it explains every FORTIFY
  assertion. Native oracle/GREEN work remains queued after the timed pair.
  The pending repair transports proved extents through a single unconditional
  inline return, using existing allocation-time captures. It does not invent
  a function-wide extent for a non-inlined call or multiple return paths;
  regressions also cover nested wrappers and one-time argument side effects.
  The Rust source parses under rustfmt, but Cargo/native verification has not
  run yet and this repair is not part of either active kernel Image.

The allocation and BTF paragraphs above/below preserve the earlier pending
checkpoints. The September 23 follow-up below records the subsequent original
object installation, relink and execution; it does not rewrite those captures.

### Selected-return and query-context repair — 2026-09-23

Four shared C-HIR repairs have small RED/GREEN cases: transport allocation
extents through the actually selected inline return; retain the executed
checked arithmetic result; prove initialized narrow-array bytes; and preserve
those facts through nested `unlikely`/`!!` expressions and provably disjoint
volatile stores. Actual argument evaluations, checked-result stores, unknown
return arms, aliasing, volatile reads and uninitialized accesses remain intact.
The final `fortify-query-context.c` passes both interpreters and sealed ARM64
GCC14/Clang19/EvoCC native execution. Changed gate: **1,786 PASS / zero FAIL /
three historical ignored**, Clippy `-D warnings` and fmt PASS.

Two unchanged original `fortify_kunit.o` replays have exact symbol parity and
unchanged source/generated inputs. Their separate diagnostic links audit all
**6,916 objects**, with **6,915 unchanged**, and preserve all 65 generated inputs.
The original filtered suite first improves to **24 PASS / two FAIL** (31 failed
assertions), then **25 PASS / one FAIL** (two failed assertions). The second
Image is `29090f227ea34a63462bc4289df4264aba783d659102e0c9da0fb08ba07c94a7`;
the nonce-bound boot/shutdown succeeds. Both executions and hashes are appended
to `kernel-expanded-kunit-receipt.json`, without recomputing the historical
1,504-case totals or crediting the old 765-case module result to these Images.

Only `fortify_test_known_sizes` lines 95 and 101 remain: the original test
requires `__builtin_strlen` of a static const array and a const pointer to a
literal to be ICE. EvoIR 10.1.18 instead explicitly makes every nonliteral
operand an external `strlen` call. This is a profile-rule conflict, not an
unexplained code-generation defect; the existing observable user-defined
`strlen` regression must also be preserved. No blanket constant folding or
test exclusion was used to obtain the 25 passes.
The source-bound reduction is `tests/fixtures/fortify-nonliteral-strlen-ice.c`:
its ICE masks are GCC14 O0=3/O2=7, Clang19 O0/O2=7 and EvoCC=1 in both
interpreters/native. The desired two extra bits identify static const array
and const pointer separately; GCC's O0 difference is retained. The pending
research brief now includes this exact contract question.

### Runtime packaging follow-up — 2026-09-23

The refreshed reference payload restores the real `nc` command. The unchanged
`bpf:test_tcpnotify_user` now **passes** in
`.evidence/kselftest-full/runtime-prereqs-reference-20260923`; its earlier
callback failure was not a kernel-code repair. Installing `snd_timer` also
lets the original ALSA test create its userspace-driven timer, but exposes a
second packaging omission: libasound needs `/usr/share/alsa/alsa.conf`. The
shared runtime copier now retains the package's ALSA configuration directory;
the new RED is GREEN and all **208 K1 harness tests**, Ruff and diff checks pass.

The unchanged ALSA replay with that configuration remains **FAIL**, not PASS:
`global.wrong_timers_test` passes, while the helper aborts in libasound
`snd_async_del_handler` (`async.c:167`) before printing its final tick count.
Capture `.evidence/kselftest-full/alsa-config-reference-20260923` has serial
SHA-256 `63e95de25f75ff8c68766d4f7cd6cc9d5fe10e79b6a681c4cd3925265191c0f6`.
The installed runtime is Debian `libasound2t64 1.2.14-1`, not EvoCC-built code.
Upstream [1.2.14 timer.c](https://github.com/alsa-project/alsa-lib/blob/v1.2.14/src/timer/timer.c)
sets `SND_ASYNC_HANDLER_TIMER` and closes its registered handler through
`snd_async_del_handler`; [the same version's async.c](https://github.com/alsa-project/alsa-lib/blob/v1.2.14/src/async.c)
handles only PCM and CTL in that switch, with an assertion in the default arm.
Both exact files were checked locally. This explains the observed abort;
it does not prove the suppressed tick count or waive the original test. No
test, library assertion or kernel source has been patched.

### Isolated compiler follow-up — 2026-09-22

Work proceeded against the existing frontend build directory with one Cargo
worker, without rebuilding or changing the timed pair's compilers, Images or
runtime disks. Three concrete checks now have RED/GREEN evidence:

* **Inline allocation results:** a wrapper that stores an `alloc_size` result in
  a local before returning it lost the proved extent. The shared C-HIR path now
  transports the fact through a final unconditional return, using the original
  allocation-time captures. Nested wrappers, offsets and one-time side effects
  pass; jumps, mutated/escaped pointers and volatile locals remain conservative.
  `tests/fixtures/inline-allocation-result.c` passes both interpreters and native
  GCC14/Clang19/EvoCC. The opaque allocator reports extents 11 and 22, two
  allocations and four wrapper effects. Native evidence is
  `.build/floating-targeted/inline-allocation-opaque-20260922.c0NFLu/native.log`.
  The first fixture exposed its 128-byte backing array to Clang and produced a
  different valid query result; that source and divergent capture are retained,
  not counted as three-way agreement.
* **Literal-byte constant queries:** the frozen compiler returned 2 on the
  minimal literal query case. The query-only helper now proves narrow literal
  bytes without executing operands or changing source ICE admission. The full
  `constant-query-literal-bytes.c` fixture and a separate original guarded Linux
  `__compiletime_strlen` macro case pass GCC14/Clang19/EvoCC natively; the latter
  prints 8. Both interpreters pass the full fixture. Negative cases retain
  volatile, effects, unknown/mutable storage and bounds checks. Evidence is
  `.build/floating-targeted/literal-bytes-native-20260922.ZxqqW3/native.log`.
  Non-literal `strlen` ICE support remains a separate unresolved boundary.
* **DWARF/BTF linkage:** the existing source repair is now tested through real
  pahole 1.30 rather than assembly text alone. The frozen compiler incorrectly
  marks two internal variables GLOBAL; repaired output has five correct variable
  linkages, six correct function DWARF flags and 11 BTF records matching GCC.
  All three ELF inputs pass `llvm-dwarfdump-19 --verify`. The first consumer
  assertion wrongly assumed default pahole preserves function BTF linkage;
  GCC disproved that assumption. Its failure remains recorded; function BTF is
  checked for parity only. Evidence is
  `.build/floating-targeted/dwarf-linkage-isolated-20260922.voHSnb/consumer-final.json`.

Final tailored gate: 65 compiler-query tests + 31 compilation-option tests +
42 C-HIR library tests = **138 PASS**, zero failures; `cargo clippy -p evocc
--all-targets --offline -j 1 -- -D warnings`, workspace fmt and status-renderer
checks PASS. This is not a new complete workspace/offline-replay gate. Original
kernel FORTIFY/BTF object replay, relink and execution with these changes remain
NOT RUN: small-case results do not close the four original FORTIFY failures.

The 970-program pair finished at 17:04 +0800. Its seven reference-PASS differences
are `kvm:demand_paging_test`, `kvm:memslot_modification_stress_test`,
`net:ioam6.sh`, `net/forwarding:sch_tbf_prio.sh`, `pidfd:pidfd_test`,
`sched_ext:runner` and `ublk:test_stress_04.sh`. Five are timeouts, two are FAILs;
they need isolated replay and diagnosis, not automatic attribution to compiler
semantics. `rseq:run_param_test.sh` passes both sides. The post-campaign gate also
exited successfully; no QEMU or compiler test from this round remains running.

### Cleanup and original-object follow-up — 2026-09-22

After confirming there was no active project build or QEMU flight, removed
1,777,400 KiB (about 1.70 GiB) of reproducible Cargo incremental cache. The
kernel build volume was cleanly detached, compacted and remounted at its original
path; `hdiutil` reclaimed another 8.9 GB of unused blocks. Both current Images,
both configs and both System.map files retained all six before/after SHA-256
values. No source, kernel object, runtime disk or evidence record was deleted.
Historical reconnaissance text remains: its exact paths are receipt inputs, so
compression or shared-write hardlinks are not an unqualified cleanup operation.
Host free space reached 83 GiB, including concurrent unrelated cleanup; only
the cache deletion and reported image compaction are attributed to this work.

Original FORTIFY replay with the debug compiler timed out at the unchanged
180-second bound. The separately identified optimized compiler replay completed
in 37.001 seconds, assembled and preserved exact global-symbol parity using the
actual generated headers. The debug timeout remains in `replay.json`; it is not
relabeled. Original `kernel/bpf/bpf_cgrp_storage.o` also replayed successfully:
pahole 1.30 reads `bpf_cgrp_storage_busy` as STATIC in repaired output and GCC,
but GLOBAL in the installed old output. DWARF verification passed for both
EvoCC objects; GCC's optional verifier was CPU-bound and stopped after 354.64
CPU seconds, retained as TIMEOUT. The successful GCC BTF conversion is separate
from that verifier timeout. Captures use the small existing directory
`.build/floating-targeted/original-fortify-btf-20260922`.

The original KUnit FORTIFY suite independently reproduces 22 PASS / 4 FAIL out
of 26 on the previous Image (`expanded-fortify-baseline-20260922`), parsed by the
unchanged upstream parser. A first attempt used the selftest-specific initramfs
without its required root disk: its FORTIFY results were produced, but boot
failed with `MARKERS_MISSING`; that failed invocation remains separate. The
successful baseline uses the existing standalone boot initramfs, with no runtime
disk copy. This also isolates the four failures from the timing-sensitive PM
test and unrelated module tests.

The static kmemdup-size failure now has a precise reduction: KUnit's unrelated
volatile location store invalidated the fact for an unexposed automatic scalar
length after allocation. The correction retains that scalar fact while keeping
the volatile write and all real mutations; escaped and volatile scalar controls
remain conservative. The 44-line `allocation-static-size-flow.c` passes both
interpreters and GCC14/Clang19/EvoCC native execution. The 139-test tailored gate,
Clippy and fmt pass. This does not authorize more source ICEs or merge arbitrary
inline return paths. Review also identified memory-output asm constraints as an
implicit address exposure; `=m`, `+m`, `=Q` and `+Q` now mark the object exposed,
with validated-IR regression checks and unchanged register/unsupported-constraint
controls. Final tailored gate: 140 Rust tests, 47 benchmark/status Python tests,
Clippy, fmt and Ruff PASS. No full campaign was started for these changes.

The one-object incremental relink completed in 789.997 seconds. All 6,916
installed target-C objects match their expected hashes afterward; 6,915 are
unchanged and only `lib/tests/fortify_kunit.o` was replaced. The 65 audited
generated source/header/config inputs have unchanged content. The later compiler
with the asm-exposure correction emitted byte-identical FORTIFY assembly, so its
verified object was reused without another identical relink. This is an explicit
mixed-compiler-version incremental composition, not a fresh single-compiler survey.

Image `64a993ef5fa42345abe3a50bcddb552ec49aeacad5e33155178280bfa008dee9`
booted and shut down with nonce `k1-20260922T100250Z-19714837`. The unchanged
upstream parser reports **23 PASS / 3 FAIL / 0 SKIP of 26**, versus the same-lane
baseline's 22/4/0. `fortify_test_alloc_size_kmalloc_const` is now PASS; failed
assertions fell from 96 to 66. Raw records are in
`.evidence/kunit/expanded-fortify-repaired-20260922`; composition and object audits
are in `.evidence/k1/expanded-fortify-isolated-20260922/repair.json`.

The working KUnit output now contains that filtered-test Image. The prior
full-suite/module-tested Image remains at
`.build/floating-targeted/original-fortify-btf-20260922/base-Image`, with unchanged
SHA-256 `985fc7ca…be9d6`. The existing 809-module runtime and 765-case module PASS
belong to that retained Image, **not** the new BTF base; no module install/load
claim was made for the filtered repair. The BPF kselftest Image/runtime were not
modified. `targetedFollowups` in the existing receipt and its status rendering
keep these populations separate.

Remaining FORTIFY cases are `known_sizes`, `alloc_size_devm_kmalloc_const` and
`alloc_size_devm_kmalloc_dynamic`. The checked-multiply/early-NULL-return shape is
now a 33-line RED at `original-fortify-btf-20260922/checked-array-return.c` under
`.build/floating-targeted`: sealed GCC14/Clang19 execute with exit 0; both EvoCC
interpreters compile without errors but return 1 on the missing dynamic extent.
One-time argument evaluation and allocation count checks pass first. The next
fix needs path-aware returned bounds, not a last-return fact applied to every
path. Nonliteral `strlen` source ICEs remain the separate literal-only profile
boundary; ordinary constant-object byte queries still need precision work.

Before the follow-up builds, 2.9 GiB of idle Cargo incremental cache was removed.
Sources, executable test binaries, frozen compilers and runtime evidence were
retained; Cargo can regenerate that cache.

After the latest module flight, the idle existing case-sensitive sparse image
was cleanly detached, compacted and remounted at the same path. `hdiutil`
reclaimed 12.3 GB; eight protected Image/config/symbol/runtime-metadata hashes
matched before and after, and host free space recovered to about 25 GiB. No
source tree, installed kernel object or runtime evidence was removed by compaction.
The next complete BPF composition passed in `20260921T230431Z`, using frozen
compiler `cd3c571be7de7dbe1d25267d06bbfb0904bea9b956dd683c5b2c48c9ed8d4379`
and `SOURCE_DATE_EPOCH=1786299935`. Both 6,765-record passes, generated-input
convergence, all 745 module links/BTF and installed-object audits completed.
Image `1d4890fa479fc598e0864e11d28e6657a9c798ce5ea08bc2b2bad7d755015f60`
booted and shut down with nonce `k1-20260922T032609Z-c77274b4`; this is not a
selftest result. Its BTF base is
`ba64d9e8df938ac46d5d070bbc882db367b7fac0bb61e5323f12f8e1d3979715`.
The serial continuation uses the existing installation and flight functions.
It signed/installed all 745 modules and rebuilt the 17 original GCC test helpers.
The resulting runtime disk is
`e8094867481b809e457dd0508c834ea3e019616bdfd3a1f7ae3f908d38e2b8fb`.
Actual loading and the 9-group/33-subtest BTF selector pass with no FAIL or SKIP
in `.evidence/kselftest-full/subject-module-btf-closure-20260922/summary.json`;
the boot nonce is `k1-20260922T033428Z-c5f2825a`. The original `kfunc_call`
failure is closed by this real execution, not only the small BTF probe.
The fresh 970-program pair then started as `20260922T033617Z`, with two workers
and a fresh VM per program. No reference execution is reused; the GCC disk
remains `c681b696714388f978fc6ef57594f2531a7c7b4a2cf17163a1b26b0e970c4827`.
Its live log is `.build/floating-targeted/kernel-runtime-followthrough-20260922.txt`;
preparation or execution is not counted as PASS before its actual result exists.
At 05:28 UTC, both sides have recorded 427/970 programs and are executing
`net:fcnal-ipv6.sh`. The preceding IPv4 program completed all 427 internal
checks on each Image with zero failures. The original bench/harness PIDs
44972/44975 remain running, not stopped. Post-campaign PID 61711 waits for
that harness to exit and verifies all 970 programs actually started on both
sides before running the pending compiler regressions, Clippy and fmt; its
log is `.build/floating-targeted/kernel-postcampaign-gate-20260922.txt`.
It does not start a competing compilation or credit the unfinished pair as
PASS. The 207 K1 Python tests and 34 bench-interface tests currently pass.

Early observations from this fresh pair are not its final totals:

* `alsa:utimer-test` fails on both Images opening `/dev/snd/timer`. Unlike the
  old configuration discussed below, this configuration has `CONFIG_SND_UTIMER=y`
  and `CONFIG_SND_TIMER=m`; the runtime preload file currently names only FUSE.
  Missing device creation/module loading is a targeted runtime follow-up, not
  evidence that the feature is disabled or a compiler pass.
* Both `bpf:test_tcpnotify_user` flights fail with zero callbacks after the
  test's `nc 127.0.0.1 ...` command fails. The runtime lacks `/usr/bin/nc` and
  `/bin/nc`; the source suppresses that command's stderr and prints stale
  `errno` rather than its exit status. Restore the real command and repeat the
  unchanged test before attributing this to kernel networking.
* The original `test_progs-no_alu32` and `test_progs-cpuv4` runners change into
  their flavor directories (`test_progs.c:cd_flavor_subdir`). The first variant's
  new GCC log reports `Can't find bpf_testmod.ko kernel module: -2`; its final
  66 failed groups therefore include missing-helper fallout, not just compiler
  failures. The rebuilt helpers exist at the primary BPF directory only.
  Correct flavor-directory packaging and replay the affected original programs;
  do not remove these variants from the 970-program denominator.
* GCC's original full `bpf:test_progs` reports 651/5,644 successful test/subtest
  counts, 127 SKIP and 11 FAIL. The subject reaches group 478 before the original
  300-second execution watchdog; exit 124 and orderly shutdown at 315.714 boot
  seconds are retained as TIMEOUT, not a complete BPF result.
* Within that incomplete subject run, `btf_dump: var_data` is an additional
  failure: expected `static int bpf_cgrp_storage_busy = (int)2`, actual text lacks
  `static`. The frozen source declares it static. `dwarfdump` on the subject's
  original object shows `DW_AT_external=0`; the reference omits that attribute.
  This identifies a DWARF-to-BTF compatibility boundary to reproduce and fix;
  it is not waived because the outer BPF program also fails on the reference.
  The upstream dwarves loader uses attribute presence for external variables
  and functions ([source](https://raw.githubusercontent.com/acmel/dwarves/master/dwarf_loader.c));
  the frozen kernel records pahole 1.30, whose exact behavior remains subject to
  the targeted reproducer. No active Image or test payload is changed mid-pair.
  `tests/fixtures/debug-linkage.c` now reproduces the false external flag with
  the frozen compiler (C compile exit 0, expected metadata assertion failure).
  The source repair and CLI regression also preserve external linkage for
  hidden/protected objects; compilation and native BTF GREEN remain queued
  until the timed campaign finishes. This is not an implemented-runtime PASS.
* `drivers/ntsync:ntsync` similarly fails opening its device although
  `CONFIG_NTSYNC=m` and `drivers/misc/ntsync.ko` is installed. IOMMUFD also has
  an installed module rather than a built-in device. The runtime repair adds
  these and `snd_timer` to the existing, inventory-checked preload mechanism.
* `drivers/net:xdp.py` cannot find `bpftool` on PATH even though the original
  executable is installed at `/tests/bpf/bpftool`. The repair uses a relative
  guest-root alias, not a substitute tool. `napi_threaded.py` is a different
  dependency gap: recorded ethtool 6.14.2 rejects `--json -l`; upstream added
  that operation in [6.15](https://kernel.googlesource.com/pub/scm/network/ethtool/ethtool/+/b70c928661024cd07914feb49122275daab904ea/NEWS).
  The frozen kernel already has `CONFIG_ETHTOOL_NETLINK=y`, so disabling a test
  or changing its JSON expectations would not fix the actual prerequisite.
* `drivers/net:netpoll_basic.py` requests the netconsole module, but the frozen
  configuration explicitly has `CONFIG_NETCONSOLE` disabled. Keep this separate
  from installed modules whose device creation was omitted.
* Both `cpu-hotplug:cpu-on-off-test.sh` flights panic in `cpu_die` on HVF.
  A paired fixed-CPU TCG replay is still required before applying the already
  measured scheduler-hotplug policy to this original program. Both
  `cpufreq:main.sh` flights are INVALID_OUTPUT: their nested TAP bails out because
  no CPU is managed by the cpufreq core, despite an outer zero exit status.
  The new full `ftrace:ftracetest-ktap` pair reaches the same HVF panic on both
  sides (GCC around 24 seconds, EvoCC around 124 seconds); its fixed-TCG
  replay is also pending. This is not the function-graph test silently passing.
* `kvm:demand_paging_test` is the first new program-level reference-PASS
  difference. The reference completes all ten guest modes and shuts down at
  92.594 boot seconds. EvoCC completes three modes, then reaches the unchanged
  120-second program limit in the fourth. Its first 4K-page stage takes
  61.305 seconds versus 20.671 seconds on GCC; the 16K and 64K stages are also
  about three times slower. This is a measured performance lead for diagnosis,
  not proof that the unfinished modes work and not authority to relabel the
  original TIMEOUT. Both used the already-declared Neoverse-N2 EL2 lane.
  The subsequent `kvm:memslot_modification_stress_test` is another
  reference-PASS difference: GCC finishes four modes and shuts down at 94.433
  boot seconds; EvoCC finishes two modes but reaches the same unchanged
  120-second limit during its third. Both run the fixed Cortex-A72 EL2 profile.
* `kvm:mmu_stress_test` times out on both sides. The subject's unfinished
  progress line directly precedes `KSELFTEST_END ... 124`, so the line-anchored
  parser records INVALID_OUTPUT rather than recovering that exit code. The raw
  capture and this campaign's original result remain unchanged. A small
  RED/GREEN now makes init put its end marker on a new line; the parser still
  rejects incomplete TAP and does not turn exit 124 into PASS. All 79 tests in
  `test_selftest.py` and `test_selftest_full.py` pass, as does Ruff. The actual
  rebuilt init and original-program replay remain pending after this campaign.

Four small runtime RED tests now reproduce the missing device preloads, missing
`nc` dependency, absent bpftool PATH alias, and omitted flavor helpers. Their
source repair passes all 68 `test_selftest_full.py` tests and Ruff, including
rejecting stale/copied or redirected flavor modules. The flavor helpers alias
the primary files rebuilt against the actual kernel; they do not add module
compilation credit or overwrite flavor-specific BPF objects. These edits only
change future preparation: the active pair's Images, disks, machine profiles
and original program inventory remain frozen. The ethtool 6.15 archive is now
cached (434 KiB) and matches the upstream SHA-256
`9477c365114d910120aaec5336a1d16196c833d8486f7c6da67bedef57880ade`.
The existing runtime preparation path builds that unmodified tool and records
its actual installed binary separately from the older distro package manifest;
the small regression verifies this packaging contract, not guest execution.
Actual refreshed-runtime replay remains pending after this complete measurement.

During final linking, the obsolete generated KUnit disk
`kunit-runtime-20260921/subject/scratch.img` was removed (2.6 GiB allocated),
after verifying its recorded `c156189b…f733` hash and the current repaired disk's
`5df403ae…b99` hash, and confirming no QEMU was using it. The repaired subject,
GCC reference, old build metadata and raw execution captures remain. Repeating
the superseded module flight would require regenerating its disk; the current
31-module/765-case acceptance uses the retained repaired disk instead.

The earlier completed section-repair evidence remains under
`.evidence/k1/20260921T065917Z`, using frozen compiler `dc6f68ee…875763d`.
The current `tests/contracts/kernel-bpf-full-receipt.json` now records the
subsequent runtime-repair composition described below.
Both 6,765-record passes succeeded; generated headers reached a content fixed
point, all 746 generated export/module units were substituted, and the final
object audit records 6,765 unchanged hashes and zero Kbuild replacements.
The 745 module links use BTF base `9fe0eda3…7b11`, the new `vmlinux` hash.
The new Image hash is `359d0835…8960b`; its HVF-host boot prints nonce
`k1-20260921T110023Z-63d0d43f`, `EVO_INIT_DONE` and `reboot: Power down`.
The installed QEMU differs from the older contract; this authorized
`--allow-qemu-drift` run records its actual hash `45d74da5…a3e`.
There are zero modpost warnings and no CALL26, build-assertion or GOT/PLT
link errors. No kernel source, linker script or reference object was used
as a substitute repair. Host tools, assembly inputs and the boot payload
remain their contracted separate scopes. Signing and a failed actual-loading
preflight followed, as recorded below; the new 970-program kselftest pair and
expanded KUnit execution are not results of this composition run.

### Actual module-loading preflight, 2026-09-21

The signed subject runtime is `selftests/runtime/subject` (745 kernel modules,
17 original BPF/livepatch helpers, 970 installed programs). Preparation uses
the original Kbuild signing/install rules and protects every kernel build
input; it does not compile replacement target-C objects. Its initial disk
hash is `a5e9c507c890652de3be7a0b6dde331ec1d03a5b4acc1ad687cd2bf7bba0eda7`.

`.evidence/kselftest-full/subject-module-preflight-20260921` records a real
boot, FUSE load and BPF helper load attempt. Boot/shutdown PASS; the original
BPF selector reports `6/4 PASSED, 4 SKIPPED, 3 FAILED`. This is a failed
targeted preflight, not the full campaign. Three concrete causes now have
small RED tests; none is waived as configuration noise:

* `nft_limit.ko`: checked mixed-sign multiplication introduces `__udivti3`
  solely to test overflow. Exact limb/carry lowering removes that artificial
  freestanding dependency; 11 targeted arithmetic/interpreter tests PASS.
* `pktgen.ko`: the guarded `min(datalen, PAGE_SIZE)` leaves
  `__compiletime_assert_1399` because constant-query resolution loses the
  dominating `datalen > 0` fact. Shared C-HIR range facts pass 18 hint tests,
  including mutation, volatile, alias, label-bypass and loop-exit negatives.
* `bpf_testmod.ko`: BTF's `bpf_struct_ops_common_value.refcnt` points directly
  to the canonical record in EvoCC, but to `refcount_t` in the original helper.
  The `refcount_struct.refs` member similarly loses `atomic_t`. These missing
  source typedef edges prevent deduplication; the kernel correctly rejects
  the module with `-EINVAL`. Fix source DWARF references, not the kernel check.

The initial failed payload/results remain attributable. Module signature
trailers and successful link are not counted as loading acceptance.

Tailored closure uses frozen compiler `7cff29cd…fd8be2`
(`.build/floating-targeted/evocc-module-runtime-20260921`): 55 targeted Rust
tests PASS, library/binary Clippy `-D warnings` PASS, fmt PASS, and 197 kernel
harness tests PASS. The small ARM64 examples execute correctly; independent
DWARF verification finds no errors. Split BTF made from an original GCC
helper now reuses all three EvoCC record types, including nested typedefs,
qualified pointers, arrays and wide integer members. The sealed Clang19
wide-overflow control lacks `__muloti4` (and compiler-rt); this unavailable
oracle is explicitly NOT PASS, while GCC14 and EvoCC execute that case.
Original `pktgen.o` and `nft_limit.o` both assemble without either unwanted
symbol. The actual kernel rebuild has now passed; no runtime closure is
inferred from these tailored results or the composition alone.

The preflight also exposed `fuse: no symbol version for strcpy`. The old
`make modpost` updated only `vmlinux.symvers`, not module descriptors. The
existing harness now replays audited module aggregate links first and runs
module-enabled modpost before compiling every generated descriptor with
EvoCC. Installation rejects unresolved depmod warnings even when depmod exits
zero. These build-order changes have RED/GREEN regressions; their original
module loading confirmation follows the new complete composition.

The fresh rebuild `20260921T124812Z` uses frozen compiler
`7cff29cd…fd8be2` and the existing low-disk path. Both 6,765-record passes
completed without missing/extra defined symbols, refusals or Kbuild object
replacement; the prepared-header pass reused 5,999 unchanged assemblies and
assembled 766 others. Module-enabled modpost has no errors or warnings, and
all 746 generated export/descriptor units were compiled with EvoCC. All
745 module links pass against new BTF base `2e6146a5…27b7`.
Image `b6d0f621…fb51` booted and shut down with nonce
`k1-20260921T165457Z-67a348b8`; the complete composition receipt is PASS.
The final audit confirms all 6,765 installed hashes unchanged after linking.
The new FUSE descriptor includes `strcpy` CRC `0xe914e41e`. Actual module
loading and the new selftest/KUnit executions remain separate acceptance
steps, not inferred from these construction checks.

The refreshed signed runtime has disk hash
`277c4553c3ee468010fd3f6009757c881151d2c3d0053703f37152050bdb1d96`.
All 745 installed module bodies/signature trailers and dependency entries
were checked; 17 original GCC-built test helpers were rebuilt for this BTF
and symbol-version base. The reference runtime was unchanged. Actual run
`.evidence/kselftest-full/subject-module-preflight-repaired-20260921` boots
Image `b6d0f621…fb51`, loads FUSE without the old CRC warning, and loads
`bpf_testmod` without the previous BTF rejection. The original selector gives
`8/22 PASSED, 0 SKIPPED, 1 FAILED`: `kfunc_call` still rejects its `sock`
argument because the helper and kernel have distinct BTF identities.

That remaining defect has source-level metadata repairs, not a verifier
bypass: preserve written typedefs in callback/definition parameter types;
encode all array dimensions in a single array DIE (including an intervening
array typedef); use the GNU unsigned-128 debug type name; omit unnamed
padding bitfields from member DIEs without changing record layout. Callback
type DIEs do not acquire per-variable names; definition parameters retain
their names. Compiler `52a5d365…ee83` passes 29 compiler-option tests; the
parameter representation also passes 47 parser/source-fidelity tests and
workspace construction checks. The original `kernel/bpf/core.o` and
`net/core/sock.o` commands assemble in isolated targeted output, not installed
kernel trees. The same-source original-helper comparison in
`.build/floating-targeted/btf-callback-arrays-20260922/split-btf-closure.json`
passes: GCC helper BTF reuses the repaired `sock`, `sock_common`, `proto`
and `mem_cgroup_per_node` identities, and the small GCC/EvoCC native controls
both exit zero. This is tailored GREEN, not a new complete-kernel or loading
result. The expanded KUnit build continues with its own earlier frozen
binary; it does not silently acquire this candidate's changes.
The candidate's completed changed gate runs all 17 workspace crates:
2,607 tests PASS, zero failures, four historical ignored; all-target Clippy
`-D warnings` and fmt pass. The 197 kernel-harness and 32 bench-entry tests
also pass. Gate output is
`.build/floating-targeted/gate-btf-metadata-20260922.txt`.

The independent KUnit runtime is
`.build/k7-coverage/kunit-runtime-20260921/reference`. It reuses the original
init and four command tools, not a Linux clone or a second selftest userspace
build. The unchanged KUnit configuration has `CONFIG_MODULE_SIG` disabled:
its 809 installed modules are **unsigned**, unlike the BPF configuration's
745 signed modules. The preparation console's initial inaccurate
`signedInstalledModules` label is corrected explicitly in `build.json`;
neither disk contents nor test verdicts were changed. The installer receipt
now distinguishes enabled signing from an unsigned configuration, with a
RED/GREEN regression; all 197 kernel harness tests and Ruff pass.

The user pause terminated the expanded build process; it did not keep running.
All 6,916 installed first-pass object hashes still match the original inventory.
The existing benchmark entrypoint now accepts `--resume-kernel-prepared`:
it requires the complete first-pass denominator, unchanged compiler/source/
command/input-cache identity and matching installed object hashes, then repeats
every prepared-input compilation, final link, module composition and boot.
The partially completed second pass is not counted or reused as compilation
evidence. Two declared RED tests now pass; 198 kernel-harness and 33 bench-entry
tests pass, as does Ruff. The original interrupted capture stays unchanged;
the resumed log is `.build/floating-targeted/kernel-expanded-kunit-resumed-20260922.txt`.

During this continuation, the unused September 14 directory
`.build/k7-coverage/case/assembly-batch` was removed after checking its 554
generated assembly/object/assembler-output files, absence of symlinks and
open file handles, and absence from the active run's inputs. This recovered
about 661 MiB; the files are regenerable, not retained in a recovery copy.
Current build trees, reference Images and result records were preserved.
The unused `module-link-tailored-20260920/out/vmlinux` copy was also removed
after a byte-for-byte comparison with retained `case/bpf-config/vmlinux` and
an open-handle check. Its 564,669,392 bytes remain recoverable from that
identical reference; no other tailored-test file was removed.

The completed prepared-input pass reused 6,200 byte-identical objects and
assembled 716 others, with zero failed units and a stable generated-input
fixed point. All 842 generated export/descriptor sources were recompiled;
modpost has no symbol/section errors, but retains sub-second container clock
skew warnings. Final linking `vmlinux.unstripped` exhausted the 64 GiB test
volume, so `20260921T192403Z` remains INCOMPLETE, not a runtime PASS.

Recovery removed the unused 3.3 GiB BPF assembly/object cache (installed BPF
objects, Images and receipts were preserved), terminated its orphaned
Virtualization process after confirming every container was stopped, and
cleanly grew/remounted the existing sparse volume at the same path to 80 GiB.
The new link-only retry uses the existing harness helpers, verifies all
6,916 installed object hashes, the original source/reference snapshot and
generated sources, and preserves the failed capture. It repeats final/module
linking and boot, not compilation; its log is
`.build/floating-targeted/kernel-expanded-kunit-link-retry-20260922.txt`.

The following paragraphs preserve the earlier failed runs and repair sequence;
their pending statements describe those historical checkpoints, not the closed
section result above.

BPF run `20260920T103126Z` ended INCOMPLETE at final link. The KUnit queue
checked that outcome and exited without starting another build; it did not
bypass the lock or turn the failed prerequisite into a PASS. No queued KUnit
capture was created. Full paired kselftest still follows a successful link,
per-kernel module-payload preparation and runtime preflight.

The exact final-link diagnostics are in that run's `relink.log`:
`__pi_create_init_idmap` and `__pi_early_map_kernel` overflow CALL26, while
Landlock contributes four undefined `__compiletime_assert_*` references and
ICMPv6 contributes two. `kernel-build-assert-folds.c` reduces initialized
record-member checks, a known object address, and a known function argument;
its lowering test must remove impossible calls, not define assertion stubs.
The link also reports unexpected GOT/PLT entries. These are observed failures,
not a new successful kernel numerator; the receipt's old reference Image
hash is not an EvoCC-built boot result.

The six assertion references now have tailored closure on release
`4fbbf3a8…901eb2e`. Complementary-byte initialization proofs no longer discard
known integer members because another member is unsupported; Boolean address
facts preserve weak-symbol and numeric-address uncertainty; private unchanged
function parameters may use identical named-function facts from all callers.
The noinline call, parameter conversion and original argument effects remain.
The 48-test hints/switch suite passes, both interpreters return zero, and the
final reduction runs successfully under native EvoCC, GCC-14 and Clang-19.
All three original units emit against actual generated headers in 9.67, 10.39
and 7.85 seconds, assemble with zero assertion references and match reference
defined names/binding kinds. Exact hashes and boundaries are in
`tests/fixtures/kernel-build-assert-folds.out`. All workspace tests completed
with 2,593 PASS, zero failures and four historical ignored. That changed gate
subsequently finished green, including Clippy and fmt; its tail is in
`.build/floating-targeted/gate-kernel-asserts-final-20260921.txt`. No repaired
object was installed into the failed composition by the targeted replay.

The new KUnit-module denominator comes from sealed `readelf -SW` over every
`.ko` named by `case/pristine/modules.order`, not a filename search for
`test`. Its configuration is `9e28277b…056def0`, the same expanded reference
as the 1,504 built-in cases. Enumeration is not module-loading evidence; both
the built-in and module paths must complete before closing this configuration.

The function-section reduction and exact commands are in
`tests/fixtures/kernel-function-section.{c,out}`. Both GCC-14 and Clang-19 at
O0/O2 place `boot_function` in `.init.text`; EvoCC places it in `.text`.
All binaries return zero, demonstrating why execution alone cannot validate
placement. The 2,515 warnings name 372 distinct referring functions, not
2,515 failed units; other warning causes still require individual review.
At the time of that RED, the body preserved unapplied function `section`,
whereas Appendix D authorized only object sections. This required explicit
approval, not an emitter-only patch or a waiver of modpost output.
Read-only inspection now confirms the same violation in the two actual
CALL26 targets: `__pi_create_init_idmap` and `__pi_early_map_kernel` are both
in `.init.text` in the reference objects and `.text` in the installed subject
objects, despite their original `__init` definitions. Exact object hashes
are in the section fixture's output file. Applying correct sections still
requires a fresh link verdict; it is not already a proven complete-link fix.

Approval has now been implemented end to end: symbol/declaration merging,
canonical function metadata, validation, proof preservation and ELF emission.
The field changes neither linkage nor the interpreters' address identities.
Entry padding, aliases, static label relocations and link-ordered patch tables
retain their placement; an ordinary following function returns to `.text`.
The first section request only after a definition is rejected with E2943:
sealed GCC applies it while Clang ignores it. Identical later repeats pass.
`tests/fixtures/function-section-boundaries.{c,out}` records this boundary.
Release `4ec65ed2…e2d866f` replays six original objects against actual generated
headers; both PI targets and `start_kernel` now occupy `.init.text`, all
defined symbols match, and the six assertion references remain absent.
The new full-workspace gate passes: 2,598 tests, zero failures, four historical
ignored; Clippy `-D warnings` and fmt pass. Both frozen layout corpora also
pass (128 + 22 cases), as do the 197 K1 and 32 bench-entry Python tests.
Full composition is still pending; no old warning total, Image hash, KUnit
or selftest result is promoted.
Final encoding checks additionally preserve UTF-8 section bytes across source,
IR and both assemblers; numeric attribute escapes are explicitly E2943 after
Clang's rejection. The final frozen release is `73eff5c4…e4a46b`, with the same
six original assembly/object digests. Full run `20260921T033257Z` used that
independent binary and the existing low-disk work tree, then was interrupted
after a further placement bug was found. `section` between `*` and a declared
name remained on the pointer type instead of entering symbol merging; this
misplaced `late_time_init` and `xen_pcibk_backend`, and also affected functions
returning pointers. Fixed ARM64 GCC/Clang controls at O0/O2 agree on placement;
two lowering REDs failed without compilation errors, and all eight linkage
tests now pass after the shared parser correction. This is not a new boot PASS.
The name audit did not prove identical nm storage categories: eleven R/D
differences follow the existing explicit-section permission policy, while the
two D/B differences above were genuine omissions. Frozen release
`dc6f68ee…875763d` now passes the complete workspace gate (2,598 PASS, zero
failures, four historical ignored; Clippy and fmt green), both layout corpora,
229 K1/bench-entry Python tests and Ruff. All seven original objects assemble
with matching defined names and nm categories, and the two missing object
sections are restored. That snapshot subsequently completed full composition,
745 module links/BTF and the actual boot/shutdown in `20260921T065917Z`.
An additional empty-CARGO_HOME, locked/offline replay passes all 13 producer,
independent-checker, corpus and public-client tests without a new workspace.
Details are in `tests/fixtures/pointer-declarator-section.out`.

## Latest full paired run — 2026-09-14: execution complete, acceptance incomplete

All 935 programs started on each unchanged expanded Image, with two VM
workers. GCC reports 287 PASS / 111 FAIL / 533 SKIP / one INVALID_OUTPUT /
one INCOMPLETE / two TIMEOUT; EvoCC reports 287 / 111 / 532 / two / one / two.
Equal PASS and FAIL totals do not imply equal results: five program outcomes
differ. Cgroup freezer and ublk stress 04 pass only on EvoCC in this run;
HSR ping times out only there; seccomp's timing benchmark fails only there;
the MM wrapper is SKIP on GCC and INVALID_OUTPUT on EvoCC.

`nested-failure-review.json` in the same capture reparses the original logs
without changing their outcomes. Both MM logs contain 50 PASS / 16 SKIP /
one FAIL internally: mremap reports 18 PASS / five FAIL / three XFAIL in the
two-GiB VM. On EvoCC an allocation warning interrupts the TAP comment prefix,
making an inner `ok 9` look top-level; the parser correctly rejects that
output. The earlier eight-GiB replay removed four allocation failures on
both Images but left the missing USERFAULTFD case; a new configuration is
required before closing that case. Seccomp's additional failure is the
per-filter timing estimate, not a pid/value assertion. The unchanged serial
replay `20260914T152937Z`, after other compiler builds had finished, reports
seven PASS / zero FAIL subtests on both Images. Its estimates are 8 versus
7 ns on GCC and 30 versus 31 ns on EvoCC; each pair satisfies the unchanged
test's tolerance. The original FAIL is retained: this is a later observation
of a timing-sensitive assertion, not a compiler repair or a new full-suite
numerator. The primary selftest receipt now names that one-program replay;
its other 934 programs are explicitly NOT_RUN.

Shared hotplug/HVF, perf-watchpoint, configuration and userspace-test failures
remain below. No full-suite PASS or complete expanded configuration is claimed.

## Completed since that full run

- Transparent-union argument lowering and private frame storage: repaired
  kernel GUP and COW runs no longer crash; the same GCC subtest results are
  reproduced. The original K6 configuration and 16 KiB task stack are unchanged.
- Filesystem stress: `file_stressor.c` intentionally sleeps 900 seconds and
  declares `TEST_F_TIMEOUT(..., 900 * 2)`. The collection lacks a settings file,
  so the generic 45-second runner could never finish it. Explicit per-program
  `run_kselftest.sh -o 1800` now reaches both watchdog layers. Both kernels
  finish **1 PASS / 0 FAIL / 0 SKIP**, with powerdown at 900.147 and 900.222
  boot seconds. Capture: `source-timeout-20260914/{reference,subject}-cli`.
  The earlier environment-only transport attempts were interrupted and remain
  failed captures; the source test and its assertions were not changed.
- Seven more expanded-configuration modules assemble with matching symbols
  after pure floating constant arithmetic is resolved to its integer result.
  Commit `1e8a5f0`, replay `floating-constants-20260914`. Total tailored repairs
  were **36 of the original 52 refusals**, not a new full-survey numerator.
- `bb98e29` applies the approved allocation-size attribute exception and shared
  dynamic object-size resolution. All ten original refusing units assemble
  with matching symbols in `dynamic-extents-20260914`; the changed gate passes.
  Tailored repairs now cover **46/52** old refusals. The 2,897 built-in records
  are being recompiled against actual generated headers for the KUnit Image.
- That complete recompile found ten new extent-capture regressions: pointer
  conditions were treated as integer values, and allocation arguments were
  captured before their formal-parameter conversion. The shared capture path
  now normalizes conditions to truth values and captures converted arguments.
  All ten actual-header replays emit, assemble, match symbols and install;
  the 46-test query suite, changed gate and GCC/Clang/EvoCC native fixture pass.
  Commit `0d1fab9`; the original incomplete composition is retained.
- The complete 2,897-object composition passes its generated-input fixed point,
  export-table check and post-link object-hash audit (zero GCC fallback), but
  **linking fails** on six impossible compile-time assertion calls in two
  objects, `ffs_kunit.o` and `printf_kunit.o`. Their minimal tests were RED
  (47 PASS / 2 FAIL, no build errors). Sequential private-value propagation
  now stops at mutation/escape, and numeric pointer bits fold only in the
  execution context, not as source ICEs. The query suite is 49/49 and changed
  gate is green; both original units re-emit with no assertion calls.
  The earlier approval-capacity failures prevented launches, not test passes.
  After approval resumed, `cfc5d64`'s GCC/Clang/EvoCC native fixture passed,
  both objects installed, the generated export object was recompiled, and
  full linking completed. All **2,897/2,897 built-in target-C object hashes**
  survive the post-link audit, with **zero GCC fallback**. Capture:
  `build-assertions-20260914/composition.json`; this is an incremental compiler
  composition, not a uniform latest-binary rebuild. Modpost section-mismatch
  warnings remain in the capture and need separate investigation.
- The first full EvoCC KUnit run completed and powered down on the same TCG
  lane and 600-second cap as the control: **1,297 PASS / 35 FAIL / 108 SKIP**,
  1,440 tests. Capture: `20260914-subject-tcg`. Seven hardware-breakpoint
  failures match the control, **28 stackinit failures are new**, and the
  control's PM race passed here. Comparing net totals alone would incorrectly
  call this 27 regressions. The stackinit failures expose missing padding and
  union-tail initialization with `-ftrivial-auto-var-init=zero`; a minimal RED
  fails with E3001, while both sealed oracles pass O0/O2. A shared padding-only
  repair passes 16 option tests and the changed-crate gate. The original
  stackinit suite is **106 PASS / 0 FAIL / 2 existing SKIP**. Full replay
  `20260914-subject-padding-tcg` then finishes **1,325 PASS / 7 FAIL / 108 SKIP**,
  with the same case sequence as the reference and **zero new regressions**.
  Only the seven source-explained arm64 breakpoint failures remain; they are
  not relabeled PASS. All 2,897 installed target-C object hashes remain audited.
- The symbol comparison now rejects failed `nm` execution or missing object
  verdicts instead of treating them as empty symbol sets. Genuine successful
  empty objects remain valid. Two declared RED subcases now pass; all **141**
  harness tests and Ruff pass. This protects measurement integrity and does
  not change or upgrade any prior kernel execution result.

## Runtime packaging: targeted guest replay complete

The full module recompile exposed three additional initializer identity failures
in the two DesignWare DMA debugfs units and `nouveau_uvmm.o`. Statement-expression
fact invalidation discarded the occurrence mapping between a normalized plan
and its written initializer, so later traversal cloned declarations with the
same semantic identity. The repair pins that mapping per initializer entry,
without weakening manifest checks or retaining stale value facts. The 50-test
query suite and changed gate pass; GCC-14, Clang-19 and native EvoCC all return
zero on the reduced fixture. All three original units now emit against the
actual generated headers in `initializer-identity-20260914`. This remains a
tailored repair until the full module composition installs the repaired objects.

| Problem | Direct evidence | Repair / pending check |
|---|---|---|
| Missing `/mnt` and `mkfs.ext4` | `mount_setattr_test.c:397` mounts tmpfs there; ten later cases invoke the ext4 formatter. | Directory and e2fsprogs supplied; **34/34 subtests PASS on each Image**. |
| Unmounted securityfs | CONFIG_SECURITYFS=y, but PID1 had not mounted it; LSM reads `/sys/kernel/security/lsm`. | Mounted when available; **3/3 programs, 15/15 subtests PASS on each Image**. |
| FAT tools and wrong file ownership | The original test first lacked mkfs.vfat, then sudo rejected rootfs files owned by macOS uid 501. The share also strips set-ID bits. | dosfstools/sudo supplied; stream an archive with original Linux metadata to the Linux formatter. **Original FAT program PASS on each Image**. |
| Hidden initramfs `/dev` mount | Foreign-namespace listmount selected the hidden first child and found one mount instead of ten. | Unmount temporary `/dev` before moving the ext4 root. Original `statmount_test_ns` **2/2 PASS on both Images**; subject `mount_setattr_test` still **34/34 PASS**. |

Four new packaging assertions and the existing harness tests total 138 PASS;
Ruff, shell syntax and the init's GCC `-Werror` build pass. Captures are
`runtime-prerequisites-20260914` and `guest-ownership-20260914`; the first
mount/FAT failures remain there. Initial approval-service and missing-libarchive
formatter failures were not counted as guest runs. The final image audit
confirms sudoers is 0:0/0440 and sudo is 0:0/04755. No additional tarball or
kernel clone is retained. This is targeted evidence, not a new full-suite pass.

## Shared test defects: not compiler passes

### Ublk now executes on the expanded configuration

All 29 original programs were rerun on both expanded Images with the supplied
helpers and `kublk`: initially 25 PASS, three SKIP and one timeout on each.
`test_stress_05.sh` runs several sequential recovery-mode phases, each launching
`fio --runtime=40`; the inherited 45-second runner stopped it mid-workload.
An explicit 600-second measurement budget, without changing the program,
finishes PASS on both (powerdown at 102.690 and 103.017 seconds). The targeted
result is therefore **26 PASS / 3 SKIP per kernel**; the original timeouts remain.
Capture: `.evidence/kunit/ublk-expanded-20260914`, including `stress-budget`.
The three remaining programs require bpftrace and block completion tracepoints;
FTRACE/KPROBES are disabled in this configuration, so supplying a binary alone
does not establish that coverage.

Cgroup `test_cpucg_max` and `test_cpucg_max_nested` write `cpu.max`, but
CONFIG_CFS_BANDWIDTH is disabled. `kernel/sched/core.c` registers that file only
under the corresponding guard. These two failures are an identified test
configuration gap; the unrelated memory-accounting failures remain unresolved.

### Matching modules and overlayfs isolation

The original Kbuild linker commands now link VETH, FUSE and overlayfs from
31 freshly compiled EvoCC C records against the actual generated headers.
Every installed input hash is audited, with no GCC fallback. All three modules
load on both matching expanded Images. FUSE's control test and overlayfs
`dev_in_maps` each pass unchanged on both kernels. Capture:
`.evidence/kunit/runtime-modules-20260914/runtime-results.json`.

The original seven-case overlay layer program fails its first case, then
leaves directories behind after failed teardown. Isolating each unchanged
case in its own VM gives **5 PASS / 2 FAIL on each kernel**:

- `set_layers_via_detached_mount_fds` creates `u/upper`, `u/work` and detached
  layer mounts below `/set_layers_via_fds_tmpfs`, but lines 690–706 assert the
  `/tmp/w`, `/tmp/u` and `/tmp/l*` paths from a different test. Line 711 fails;
  the teardown then cannot remove the still-busy mount directory.
- `set_override_creds_nomknod` drops both CAP_MKNOD and CAP_SYS_ADMIN before
  recording override credentials (lines 502–508). Overlay's super.c calls
  `clone_private_mount`, whose namespace.c implementation explicitly checks
  CAP_SYS_ADMIN and returns EPERM. Both kernels log `failed to clone upperpath`.

These failures remain FAIL; passing five isolated cases is not seven passes.
The test sources have not been patched. The full module population and NEON
inputs remain unfinished.

### Generated network helpers

The upstream top-level selftest Makefile includes `net/lib` through
`INSTALL_DEP_TARGETS`, not TARGETS. The harness omitted that variable and
only copied the helper source directory. It now builds and installs the
declared dependency without treating it as another runnable collection.
One RED assertion and 142 GREEN harness tests pin that behavior and make a
dependency build failure visible. The original offline Makefiles generated
`xdp_dummy.bpf.o`, `xdp_native.bpf.o`, `csum` and `xdp_helper`; each installed
file was read back from both runtime disks and compared byte-for-byte.

VETH now loads XDP and exercises channel checks, but UDP receives fail with
EAFNOSUPPORT on both kernels: `udpgso_bench_rx` defaults to AF_INET6 and
CONFIG_IPV6=m has no installed ipv6 module yet. All 3,413 module C records
are being recompiled against actual generated headers before broader linking
and installation. The 935-program full-run counts are not replaced by these
targeted measurements.

### clone3 and the libcap private ABI

The separately labeled public-API variant now passes on both expanded Images:
`clone3-public-api-20260914/results.json`. It changes only the obsolete private
`cap_t` writes to `cap_set_flag` and keeps every clone3 and permission assertion.
The original upstream FAIL remains in the full-suite inventory; this is a
userspace compatibility diagnosis, not an unmodified-test PASS.

The unchanged `clone3_cap_checkpoint_restore.c:116-122` casts opaque `cap_t`
to its own two-word-header `struct libcap` and writes words 5 and 6 to enable
CAP_CHECKPOINT_RESTORE in effective and permitted sets. The actual runtime is
libcap 2.75 (`libcap.so.2.75` SHA-256
`2d74e5a4d536311c2e84d1603edb4c8dd7cdfaa92f1305ec4e317edcd062ddec`).
An offline sealed-container probe loaded that exact library with ctypes,
used `cap_init`, `cap_set_flag` and `cap_to_text`, then applied the test's
two raw writes. No process capabilities were changed.

```text
public API, SETUID/SETGID: cap_setgid,cap_setuid=ep
first 8 words: 0,20080522,0,c0,c0,0,0,0
after test writes: cap_setpcap=i cap_setgid,cap_setuid+ep cap_checkpoint_restore+e
public API setting capability 40: cap_setpcap=i cap_setgid,cap_setuid,cap_checkpoint_restore+ep
```

The private writes set an unintended inheritable bit and do not set the
requested permitted bit. Requesting effective capability 40 without its
permitted bit explains `cap_set_proc: Operation not permitted` on **both**
kernels. A compatible userspace test must use the public capability API;
changing the kernel or replacing the expected result would be wrong. No test
source was patched and its original FAIL remains. The probe reads only the
same eight words accessed by the source's cast.

### Harness constructor ordering

The GCC-built `harness-selftest.sh` compares exact output to a source-order
fixture. Its stored diff shows `fixture.timeout, fixture.fail, fixture.pass`
instead of `fixture.pass, fixture.fail, fixture.timeout`; each case has the
expected individual behavior. In `kselftest_harness.h`, TEST_F constructors
use priority 20000, while `__constructor_order_first` uses the default
priority and sets the direction flag later. `__LIST_APPEND` prepends while
that flag is false. The previously unexecuted probe is now measured (2026-09-24):
[minimal C](../../tests/fixtures/selftest-constructor-order.c) reproduces that
list on GCC14, Clang19 and EvoCC at O0/O2; moving only the detector to priority
10000 fixes the source-order control in all six pairs. Both interpreters agree.
Eighteen native observations include six expected REDs, not original passes.

Fresh unchanged-original replays still fail the same ordering-only diff on both
retained kernels; both boot/shutdown predicates pass and disk hashes are intact.
The 506-test affected gate, Clippy and fmt pass. This closes TODO K11b's
**upstream harness-assumption classification**, without modifying the upstream
test/expectation or changing full-run totals. [Exact identities and evidence](../../tests/fixtures/selftest-constructor-order.out).

## Configuration, architecture and implementation boundaries

- The frozen K6 runtime has no installed modules. The expanded runtime now
  has matching EvoCC VETH, FUSE and OVERLAY modules; broader module dependencies
  including IPv6 are still being prepared, not supplied by GCC fallback.
- Ublk helper files are now present. All 29 original scripts reach the
  prerequisite check and SKIP on K6 because CONFIG_BLK_DEV_UBLK is disabled.
  The separate expanded configuration enables it; build/install/runtime
  verification of that configuration is not complete.
- USERFAULTFD is disabled. Four additional mremap failures request 2 GiB
  allocations inside a 2 GiB VM and return ENOMEM. These are actual resource
  and configuration limits, not proved compiler defects or passes.
- Landlock's eight programs reach ENOSYS because CONFIG_SECURITY_LANDLOCK is
  disabled in both configurations. IPC's MSG_COPY reaches the explicit ENOSYS
  branch in `ipc/msg.c` when CONFIG_CHECKPOINT_RESTORE is disabled. The two
  unsealed vDSO/vvar failures require CONFIG_MSEAL_SYSTEM_MAPPINGS, also
  disabled. These are configuration coverage gaps, not hardware exclusions.
- ALSA utimer, ntsync and iommufd fail while opening their device nodes, before
  their assertions exercise the interfaces. CONFIG_SND_UTIMER, CONFIG_NTSYNC
  and CONFIG_IOMMUFD are all disabled in both configurations. Their original
  failures remain; enabling the interfaces requires another explicit test
  configuration, rather than attributing missing devices to the compiler.
- Syscall-user-dispatch is gated by CONFIG_GENERIC_ENTRY. Arm64 does not
  select it in this tree; `include/linux/syscall_user_dispatch.h` provides
  EINVAL stubs. This explains the dispatch and ptrace-SUD failures on this
  architecture; an x86 implementation is not evidence of arm64 support.
- The direct `rseq:syscall_errors_test` expects no pre-existing registration,
  but glibc registers rseq. The upstream companion
  `run_syscall_errors_test.sh` explicitly sets `glibc.pthread.rseq=0` before
  executing the same binary. The direct run's EINVAL versus expected EFAULT
  follows the existing-registration branch in `kernel/rseq.c`, before its
  address check. The original direct FAIL is retained.
- BTF requires real compiler type/debug metadata, not the GCC kernel's BTF.
  Source-derived DWARF now passes LLVM verification without warnings and
  converts through the sealed `pahole` on a fixture and the actual
  `lib/tests/bitfield_kunit.o` (248,098-byte BTF). The fixture's machine code
  is byte-identical with/without `-g`; its decoded records preserve recursive
  pointers, bit-fields, packed offsets and source function signatures.
  Capture: `.evidence/builtin-probes/20260914-dwarf-types/linked-types`.
  This does not provide line tables or local-variable locations, and neither
  current Image contains BTF yet. Full BTF kernel integration and its tests
  remain pending; the targeted conversion is not kernel coverage.
- GCC KUnit's seven failing hardware-breakpoint cases request per-task EL1
  breakpoints that arm64 `arch_build_bp_info` explicitly rejects with EINVAL.
  The one PM runtime failure does not reproduce in isolated 6/6 control
  runs, but its full-run FAIL is retained. EvoCC's separate full run has the
  same seven breakpoint failures; its 28 padding regressions are now repaired.
- The two built-in KUnit compilation refusals require dynamic object size
  inferred from `alloc_size(1)` / `(1,2)`. Ten units depend on that rule.
  Revision 1.9.97 records the user-approved exception; `bb98e29` passes 45
  focused tests, the changed gate, and all ten original-unit replays. The
  built-in objects have been rebuilt; later link blockers are repaired and
  the full EvoCC KUnit run now exists.
- Six NEON module units reach the original GCC resource header and fail at
  its compiler vector type `__Int8x8_t`. C executable vectors and MIR vector
  lowering are missing. This is authorized compiler work still to do, not
  evidence that those modules are inapplicable to ARM64.
- Other shared failures (including rseq's long workload, timer tolerances,
  cgroup, perf and ptrace) still require individual diagnosis.
  Equality with the GCC control is not a blanket waiver. The complete unchanged
  rseq pressure matrix now finishes PASS on both expanded kernels, in 527.633
  and 518.724 boot seconds. Its upstream `timeout=0` was previously replaced
  by a 300-second fallback; an explicit 7,200-second budget allowed the full
  workload to finish. Capture: `rseq-complete-20260914/results.json`.
  The original timeout and the inner architecture-specific skipped operations
  remain recorded; one passing wrapper is not 1,184 passing TAP cases.

## Next execution sequence

1. Matching module installation now covers **702/704** modules built from
   **3,407** audited EvoCC C inputs and six assembly inputs; all installed
   files and dependency indexes were read back byte-for-byte. The GCC control
   has its independently built 704 modules. The seven additional initializer
   identity refusals all replayed successfully; the two missing subject
   modules still need the six NEON C units. Capture: `module-link-20260914`.
   IPv6 and VETH load and exercise traffic; both kernels now fail packet-count
   assertions (for example 2 instead of 1, 13 instead of 10) rather than socket
   or helper setup. Those assertions still need diagnosis.
2. Complete integer-vector implementation with original-unit replay; it is
   the remaining six-unit module compilation gap, not a hardware exclusion.
3. Run the expanded Image's full selftests against its corresponding GCC
   control through `tools/bench.py kernel-selftest-all --reuse-selftest-build
   --expanded-selftest-kernel --selftest-runtime .build/k7/module-runtime/full
   --allow-qemu-drift --jobs 2`. The selector checks object hashes, Image hashes
   and matching configurations before launching. It does not replace the K6
   image or copy build logs for each runtime replay. Also execute the 33 module
   files containing KUnit suites; the 1,440 cases above are built-in suites.
4. Complete individual diagnoses of the other shared failures before a new
   full comparison. The original 935-program denominator remains visible.

No new kernel clone was created. The first full KUnit and targeted packaging
VMs exited normally; the padding repair's changed gate and release build pass.
Existing runtime disks and earlier failed captures are retained.

## Module execution and immutable read repair

The 33 module files with KUnit sections were actually loaded, independently
of the built-in suites, in `module-suites-20260914`. GCC loaded all 33;
EvoCC loaded ten and failed to load 23 because DRM and iwlwifi retained
undefined `__compiletime_assert_*` calls. Successful `ld -r` and defined-symbol
parity did not test this condition. The GCC serial stream contains a partial
KTAP line followed by an explicit printk replay, producing 11 parser errors;
its provisional 773 PASS / one SKIP is not yet a clean suite result.

The assertion calls reduce to immutable string/record loads after inlining.
The lowerer's bounded value analysis now tracks source-immutable addresses
and initialized non-relocation bytes through explicit CFG arguments. Loads,
bounds guards and effects remain in the IR; mutable, volatile, unknown and
relocation-bearing reads do not acquire constants. The 52 focused tests and
changed gate pass. The combined minimal fixture runs with result zero under
sealed GCC-14, Clang-19 and EvoCC at `-O2`; the first native invocation omitted
`-O2` and its link failure is retained, not called a compiler regression.

Actual `drm_edid.o` and `iwl-io.o` now have no assertion calls. Other original
commands exposed a separate `-g` defect: lifted function-static objects are
matched against the pre-inline source inventory. Remaining replays were
stopped to repair that mismatch; module loading must still be rerun. The
original full kselftest pair continues against its unchanged payloads.

The debug mismatch is repaired by consuming the module and its actual
normalized C-HIR together, before the inline arena expires. No second inline
pass or guessed synthetic-name mapping is used. All 20 compilation-option
tests and the changed gate pass; the previously failing original `-g`
commands now emit successfully.

Module KTAP is now read from complete kernel-ring records after each load,
with asynchronous console delivery suppressed for this separate measurement.
Neither assertions nor test sources change. The reference rerun in
`module-suites-20260914/reference-records` accounts for all 33 modules and
**781 cases: 780 PASS, zero FAIL, one SKIP, zero parser errors**. The raw
console attempts are retained: one lost seven cases to a replayed partial
line, and another interleaved the next module marker with delayed output.
EvoCC must pass the same record-based execution after its module repairs.

The 75 original module commands now emit and assemble without the retained
assertion calls (`sequential-queries-20260914`). Two remaining shapes required
sequential facts for unescaped assignments and bounded alternatives from an
already evaluated immutable initializer. Initialization, conversions, side
effects and source-ICE admission are unchanged; 54 focused tests, the changed
gate and sealed GCC/Clang/EvoCC native probes pass. The full 702-module relink
preserves all 3,407 audited C leaves and six assembly leaves, without fallback.

The first repaired module execution is **not fully green**:
`module-suites-20260914/subject-repaired-records` has 32 successful loads,
780 cases (778 PASS, one FAIL, one SKIP), and one missing suite. The remaining
iwlmvm load failure names `iwl_mvm_send_roaming_forbidden_event`: its caller is
reachable only through an otherwise unused static callback table. The PRP
failure compares a saved jiffies value with a later live read, differing by
one tick; this needs a separate timing diagnosis, not a changed assertion.

The original CPU-hotplug test now passes on both kernels in the TCG lane
(`hotplug-tcg-20260914`). Its earlier shared HVF panic remains recorded:
the CPU-off implementation returns from the CPU-die path. This isolates the
execution backend rather than changing the test or calling the HVF result PASS.

The callback defect is repaired at the shared lowered symbol graph: an
unreferenced internal table is not an unconditional root for its relocations.
Unused address transport is removed across complete CFG edges; original
evaluations, guards, memory operations and poison consumers remain. External
symbols, explicit sections, used/retain objects, aliases and assembly retain
their definitions. The standalone fixture links and runs under both sealed
oracles and EvoCC, including independently verified DWARF. The original
`iwlwifi/mvm/ops.o` command now drops the unreachable external call.

`module-suites-20260914/subject-complete-records` loads **33/33 modules** and
reports **781 cases: 780 PASS, zero FAIL, one SKIP, zero parser errors**,
matching `reference-records` case identities and outcomes. The Image shuts
down normally; log SHA-256 is
`1d4711cd89973358918fefb6f52d317357213bdd1bba40b1124448b5bd28eded`.
This later measurement does not erase the earlier one-tick PRP failure:
`hsr_framereg.c` saves jiffies, reenables bottom halves, and the test compares
a later live jiffies read without excluding timer interrupts. No compiler
repair or test-assertion change was made for that timing-sensitive comparison.

The supplementary BTF test configuration is separate from the running full
selftest Images. Its IOMMU test requires `FAULT_INJECTION`; merely requesting
`IOMMUFD_TEST` without that dependency left it disabled. Likewise
`MSEAL_SYSTEM_MAPPINGS` depends on `!CHECKPOINT_RESTORE` in `security/Kconfig`.
The former is enabled in the main coverage fragment; mseal has a separate
fragment that explicitly disables checkpoint/restore. These are distinct
configurations, not simultaneous coverage attributed to one Image.

## Supplemental configuration: actual build and first compiler blockers

The case-sensitive reference build completes with configuration SHA-256
`9e28277bea95477ff8ccd9a88809ef18368ecc9db57552b16fa06f9aa056def0`.
Its 6,916 target-C records comprise 3,137 built-in and 3,779 module records;
the existing command audit finds no unclassified omitted options.
`coverage-reference-builtin-20260914` boots the actual new GCC Image and the
upstream parser reports 1,504 KUnit tests: 1,437 PASS, seven FAIL, 60 SKIP.
All seven failures are in `hw_breakpoint`; this is not a passing KUnit claim.

The supplemental EvoCC survey was stopped after 128 recorded units:
118 symbol drifts, seven verified installations, three refusals
(`coverage-compile-20260914/summary.json`). The drifts add the same 15 FORTIFY
wrapper definitions: GNU `extern inline` definitions must not acquire
independent external definitions. The subsequent approved revision 1.9.102
replaces the preserve-only rule with full GNU/ISO declaration-chain semantics;
the implementation now separates inline bodies from emitted definitions.
Both sealed compilers' O0/O2 symbol and execution probes
are in `builtin-probes/20260914-gnu-inline/oracles.out`.

An independent refusal in `machine_halt` reduces to `stop(); while (1);`:
dead address pruning discarded loop-only effect parameters. The repair roots
existing mem/world parameters without changing validator rules or loop
termination; commit `783a4e7` passes its four control-flow variants, the 56-test
query suite and changed gate, and the original `process.o` emits assembly.
The other two parser refusals are static numeric-pointer comparisons in
trace-field signedness metadata, not 100 independent recovery errors.
The shared numeric-address evaluator now admits the static initializer, while
the existing interpreter task machine evaluates its casts and comparison:
58 query tests pass, both interpreters return zero for the reduced matrix,
and its generated AArch64 assembly executes with exit zero in the sealed
container. Original `arm.o` and `handle_exit.o` commands both emit assembly
with no diagnostics (debug compiler, 115.087 and 73.616 seconds under concurrent
gate load). These are emission checks, not symbol-parity or installed-object
claims; the independent GNU-inline linkage blocker still applies. The final
release binary (`160568e35535306cb0a673baf922945c998959365707becbcb4ae316dbaf2fe6`)
repeats both emissions with empty stderr in 10.263 and 6.138 seconds, and its
native fixture again exits zero. An initial shortcut bypassed static fuel
accounting and failed the existing budget test; it was removed before commit.
Both that budget test and the nonrecursive deep-initializer tests pass.
Commit `386339d` closes the narrow compiler repair; its changed gate reports
1,576 passed / zero failed / three existing ignored tests, Clippy and fmt
clean. Commit `783a4e7` closes the independent infinite-loop token repair.
Full supplemental composition and execution remain pending. After the inline
repair, targeted `.vmlinux.export.o` and `arch/arm64/kernel/process.o` replays
assemble and have exact reference-symbol parity (zero and 26 external
definitions respectively), with none of the 15 extra FORTIFY definitions.
This is two affected objects, not a completion claim for the 6,916-record
supplemental configuration. Six mixed-compiler native inline-linkage runs pass;
retained address references without an external implementation fail to link.

## BTF-dependent userspace builds need their own effective configuration

`kselftest-full/coverage-build-20260914/configured-build.json` records the
completed build attempt with the actual reference `vmlinux` BTF, configured
test-module input, versioned LLVM commands, openssl and rst2man. It installs
961 of 970 declared programs; HID now builds all 13, while BPF installs
13/21 and sched_ext 0/1. These are build counts, not executions, and do not
replace the earlier 935-program runtime inventory.

- BPF's generated `tools/include/vmlinux.h` contains `struct foo` and `bar`
  from `lib/tests/overflow_kunit.c:1174-1184`; BPF test sources define their
  own names and fail at redefinition. Combining every built-in KUnit suite
  with this BPF header is not a usable test configuration. Keep the already
  measured KUnit Image and prepare a separate BPF configuration; do not edit
  test types or delete records from actual BTF.
- The BPF config requests FTRACE_SYSCALLS and NET_SCH_BPF, which this Image
  lacks; generated syscall trace types and qdisc kfunc declarations are
  consequently missing. sched_ext likewise lacks SCHED_CLASS_EXT and the
  `task_struct.scx` field. The requested function-tracing build flags require
  compiler support, not omission from the command projection.
  The separate fragment `tests/configs/kernel-bpf-coverage.config`, applied
  after the upstream BPF/HID/sched_ext fragments, resolves all five options
  and disables KUnit. This is only an `olddefconfig` check: no Image from it
  has been built or run. Its arm64 Makefile selects
  `-fpatchable-function-entry=4,2`; the compiler must implement the entry NOPs,
  recorded entry table and the kernel's `patchable_function_entry(0,0)`
  notrace override rather than silently dropping this option.
  The sealed `builtin-probes/20260914-function-entry/probe.c` now measures it
  on both GCC-14 and Clang-19, at O0/O2: four NOPs with the function symbol
  after the first two, a table relocation to the first NOP, no NOPs or table
  entry for the `(0,0)` override, and three NOPs with one before the symbol
  for `(3,1)`. All four native programs exit zero. This is oracle evidence,
  not an EvoCC implementation; `oracles.out` SHA-256 is
  `6ae24aefc50664369e9b7727cb7017c3f2dbbf0cd3a2a1f117fb758f3fa6a05f`.
  On 2026-09-20 the working-tree implementation passes its CLI/IR reduction,
  native execution assembled by both fixed toolchains, exact NOP/table/link-order
  inspection, and independent proof-field mutation tests. The command projection
  now preserves the option and its override order. Explicit function alignment
  follows the existing EvoIR callable-symbol rule; the measured oracle conflict
  and late conflicting redeclaration refusal are in Appendix D and
  `tests/fixtures/kernel-function-entry.out`. This removes the targeted compiler
  blocker, **not** the remaining BPF-config build/install/runtime acceptance.
  The actual reference build also exposed `-fmin-function-alignment=8`:
  the driver now preserves the option, raises the existing function `align`
  for cold/ordinary definitions, and retains that minimum in both interpreters
  without changing source `__alignof__`. The combined alignment/patching fixture
  passes native execution and ELF address checks with both fixed assemblers;
  94 CLI, one text-completeness and 161 K1 harness checks pass. The reference
  Image/modules are still building; no new kernel execution is credited.
  Original `arch/arm64/kernel/ftrace.o`, `kernel/fork.o` and
  `kernel/sched/build_policy.o` (including `ext.c`) now emit, assemble and
  match both global symbol inventories under their recorded flags and actual
  generated headers. These are three targeted objects, not a completed survey.
- The certificate helper also needs `xxd`; that prerequisite is now recorded.
  Livepatch's module Makefile silently skips construction when KDIR is absent;
  the shared build options now pass the configured KDIR to all collections,
  while preserving their separate userspace output directories. The new
  dependency and KDIR assertions were RED, then GREEN; livepatch execution
  still requires its own enabled kernel support.
- kexec, prctl and the two Intel thermal collections declare no runnable
  arm64 targets in this source; their `make all` failures remain explicit,
  not successful test results. Their architecture guards, unlike BPF's
  missing configuration, are genuine inapplicability evidence.

The supplemental userspace runtime now contains bpftrace 0.23.2 and its
runtime libraries; chrooting into that exact root and running `bpftrace
--version` exits zero. Commit `09f72ff` adds the declared dependency.
The earlier 935-program payload was not changed, and the three tracing-based
ublk cases have not yet run against the new kernel configuration. Installing
the command does not substitute for the required kernel tracepoints.

The status renderer also used to compare every selftest Image with the K6
Image even when the receipt explicitly selected the separate KUnit pair.
It now compares with the named kernel receipt. A RED test reproduced the
incorrect STALE_IMAGE label; 26 benchmark tests now pass, including refusal
to accept a changed Image or a missing named receipt. Runtime Image and
object-hash checks were not relaxed.

## Source/configuration follow-up, 2026-09-20

These diagnoses use the retained `20260914T121718Z` logs and the actual Image
configuration, not equal reference/subject totals as a waiver:

- `cgroup:test_cpu` passes its first seven cases and fails only the two
  `test_cpucg_max*` cases. Both write `cpu.max` (`test_cpu.c:667,738`), while
  the measured KUnit configuration has `CONFIG_CFS_BANDWIDTH` disabled.
  The separate BPF/coverage configuration enables it; runtime closure is pending.
- `net:tls` prints `Failure setting TCP_ULP, testing without tls`; the measured
  configuration has `CONFIG_TLS` disabled. `tls.c:144-149` enters its plaintext
  fallback, but later rekey/get-sockopt tests still request `SOL_TLS`, and some
  subsequent checks dereference the absent control message. Its raw 128 FAIL
  subcases and overall FAIL remain; the other 669 PASS subcases do not prove TLS
  coverage. The new configuration has `CONFIG_TLS=m`, requiring a matching module.
- `drivers/net/team:propagation.sh` explicitly fails `modprobe netdevsim`;
  the original configuration lacks `CONFIG_NETDEVSIM`, now selected as a module.
  Bonding/dummy/VXLAN were also absent in that Image and selected in the new
  configuration; each affected original program still needs its own replay.
- `perf_events:remove_on_exec` fails all four fixture setups at
  `perf_event_open` (`remove_on_exec.c:90`), before testing exec inheritance.
  Its event is `PERF_TYPE_HARDWARE/PERF_COUNT_HW_INSTRUCTIONS`, and the same
  boot log reports failure to probe the virtual PMU. The unchanged TCG replay
  `20260920T014342Z` also fails all four fixture setups on both Images, although
  both boots/shutdowns pass. This is not a TCG repair or a test PASS; the exact
  event-open error remains under diagnosis, separately from the watchpoint-based
  `sigtrap_threads` timeouts.
- `ptrace:vmaccess` has a 30-second timeout in its attach subcase, not a
  reported value mismatch; `cgroup:test_kmem` reports real accounting deviations
  and delayed dead-cgroup cleanup. Neither is yet classified as inapplicable.

The minimal `tests/fixtures/perf-instructions-availability.c` now identifies
the event boundary on both Images: ordinary TCG has a registered PMUv3 driver,
working hardware-cycle/software-task-clock events, but instruction counting and
sampling both return `ENOENT`. A separate **diagnostic-only** configuration adds
`-accel tcg,thread=single -icount shift=0` with the same Cortex-A72, four CPUs,
memory and Images; instruction-event setup succeeds. This matches QEMU's
[precise-icount event registration](https://lists.gnu.org/archive/html/qemu-arm/2018-12/msg00087.html)
and is not an amendment of the locked K6/QEMU contract.

The original `remove_on_exec` binary and runtime then run three times per Image
in that diagnostic configuration: GCC **4/4, 4/4, 4/4**; EvoCC **3/4, 4/4, 4/4**.
The first subject run reaps an unexpectedly exited child at source line 190;
the two later passes do not erase that failure or establish its cause.
All six boot/shutdown predicates pass; the experiment remains **INCOMPLETE**
pending that intermittent subject failure and explicit integration of the
additional execution configuration. Captures and the exact diagnostic contract
are under `.build/floating-targeted/perf-probe/`; existing full-run counts and
original test sources are unchanged.

The separate TCG replay `20260920T020223Z` leaves both original programs red
on both Images: `ptrace:vmaccess` has one PASS / one FAIL (the 30-second
`attach` timeout); `perf_events:sigtrap_threads` times out at the unchanged
120-second program limit after three 30-second subcase failures. All four
boots/shutdowns pass. The other 933 inventory entries are NOT RUN in this
selection, not new failures or passes. Summary SHA-256:
`902cabe0079024cfd8b925d7287115cdf750abcf58af39f5a65513c04d5ede98`.

- The frozen kernel itself marks `BP_SIGNAL_IS_SUPPORTED=0` for aarch64 in
  `tools/perf/tests/tests.h:179-200`, explaining the repeated watchpoint
  instruction and missing signal/single-step progress. Its perf test checks
  that guard, but the separate kselftest does not. The
  [upstream arm64 sigtrap exclusion](https://lkml.iu.edu/hypermail/linux/kernel/2202.2/03028.html)
  documents the same architecture limitation. This is source-supported
  attribution of the observed timeout, not a newly captured instruction trace;
  no original test is patched to skip, and the raw TIMEOUT remains.
- `vmaccess.c:53-66` creates a traced sibling, enters exec and then attaches
  from the tracer. The frozen `kernel/ptrace.c:457` waits on
  `cred_guard_mutex`; `fs/exec.c` holds it while `de_thread` waits for the
  traced sibling to be reaped. The blocked tracer cannot perform that reap.
  This is the exact deadlock described by the
  [upstream exec/ptrace repair proposal](https://www.mail-archive.com/linux-kernel%40vger.kernel.org/msg2598092.html),
  which also changes this test's original EAGAIN expectation. Updating that
  frozen kernel/test pair is a separate baseline change, not a compiler fix.
  The original FAIL is retained rather than turning the proposed patch into
  an unmeasured PASS.

The wait-cycle attribution is now directly measured (2026-09-24, TODO K11d),
not just source reasoning. The [bounded C reduction](../../tests/fixtures/ptrace-exec-reap-cycle.c)
captures `ptrace_attach` in the blocked tracer and `begin_new_exec` in the
tracee on both retained kernels. A second tracer thread reaps the ptraced
sibling; that releases the blocked attach, which returns 0 rather than EAGAIN.
Stop/detach/kill/reap checks confirm no child remains. Fresh unchanged-original
flights retain **1 PASS / 1 attach-timeout FAIL** on each kernel, both with
normal shutdown. This closes causal classification, not test PASS or permission
to modify the frozen kernel API. [Exact stacks and identities](../../tests/fixtures/ptrace-exec-reap-cycle.out).

Five additional unmodified BPF/tracing command records now emit and assemble
with release `77faa6b3479296b0560e55f611254585146f5b0d443dcd72c3d6c82e48f27d8a`:
`kernel/trace/trace.o`, `kernel/events/core.o`, `kernel/trace/bpf_trace.o`,
`drivers/net/netdevsim/bpf.o`, and `net/sched/cls_bpf.o`. Each has zero missing
or extra external definitions against its reference object. Captures are in
`.build/floating-targeted/function-entry/tracing/`; these objects have not yet
been installed in a linked/booted composition.

The separate BPF reference build finishes successfully on 2026-09-20:
**6,765 target-C command records, 745 linked modules, zero unreviewed omitted
options**. This is a new configuration denominator, not a reduction of the
older 6,916-record KUnit configuration. Config SHA-256:
`2f938cda8169ed22e90e46b4e43a9b3752daa6c457497fb13e14a3d66abdfa28`;
reference Image:
`8fa43fb43db82b6f112d09dbf5fcdbd29de848c68e225719d3b92380e380337b`.
The original nonce/normal-shutdown boot predicate passes in the declared HVF
lane (`.evidence/k1/bpf-reference-boot-20260920`); QEMU drift remains explicitly
recorded. This is still a reference boot, not an EvoCC composition.

The working output `.build/k7-coverage/case/bpf/out` is an APFS copy-on-write
snapshot of that **build output only**; `pristine` refers to the completed
reference. The Linux source, Cargo target, accepted K6/KUnit Images and old
runtime payloads are reused without cloning or modifying them. The existing
unified bench now accepts supplemental full-link roots and a separate receipt,
so this configuration cannot overwrite the accepted K6 receipt by default.
The same explicit receipt selection is checked on later selftest reads, with
identity mismatch and stale-receipt negatives retained.

Original userspace preparation against this reference now builds the
`sched_ext` runner successfully. The BPF build exits 2 with only the three
arena prototype conflicts, repeated in the default/no_alu32/cpuv4 builds
of `stream.bpf.o`. The image's pahole 1.30 reports **no `attributes` capability**
in `--supported_btf_features`, despite `scripts/Makefile.btf` requesting it.
The generated `vmlinux.h` lacks `address_space(1)` on arena argument/return
types, while the original test correctly declares those address spaces.
Pahole's [1.30 release notes](https://kernel.googlesource.com/pub/scm/devel/pahole/pahole/2385c55f293b9d8d6c6c20465950a80684deb43a/changes-v1.30)
explain that this feature depends on the loaded libbpf API, not the version
string alone. This is a measured reference-build prerequisite failure, not
an EvoCC refusal. The original tests and active kernel-survey inputs remain
unchanged while the library capability is checked separately.

That prerequisite is now repaired without changing the frozen image or kernel:
the frozen source's libbpf 1.7.0 is built separately and exposed only to the
pahole process. Its capability list gains `attributes`; strict feature checking
then produces detached BTF from the unchanged reference `vmlinux`. Library
SHA-256: `4d353546e0d919c0e109a5401401615bf6d9e74cb56f9e1062ee350e45178223`;
BTF: `3bd0328ebefc91863be09efdae3fb8101316f14e62543b58855bd8712353919f`.
The original BPF `all` build now exits **0**, including `test_maps` and all
three `test_progs` variants. Original sources and assertions are unchanged.
The same capability check and detached-BTF preparation are integrated into
the existing selftest builder, with three RED-to-GREEN tests, **165/165 K1
harness tests**, Ruff and a real sealed-container invocation passing.
This closes userspace construction only: it neither repairs the linked
kernel's runtime BTF nor supplies BPF runtime acceptance. The 128-collection
build/install refresh runs separately with the reference kernel mounted
read-only; its old build metadata is preserved as `build-before-btf-fix.json`.

The small `perf-remove-on-exec-status.c` diagnostic now records the previously
missing child exit status. In the separately declared icount configuration,
the GCC reference has **30 PASS / 2 FAIL** (rounds 5 and 22 die with SIGTRAP,
wait status `0x5`), and the EvoCC Image has **32 PASS / 0 FAIL**. Both boot and
shut down normally. This demonstrates the failure shape on the reference too;
it does **not** retroactively establish the exact cause of the original
subject's uninstrumented failure or erase it. Captures:
`.build/floating-targeted/perf-exec-status/{reference,subject}/boot.log`;
SHA-256 respectively `084c3e877164848ec956e8e31eafbbb991399d470aec2ca00a5732826e2b26eb`
and `9e499f750412db62e03dc3e7003e7b2d6f42147d61aef5c43849b31802e1a49b`.
The upstream pending-perf-task discussion describes a compatible race, but
the present evidence does not instrument the exact kernel instruction path.
No kernel patch, test skip or timeout increase is counted as a repair.

`test_percpu_basic` now has a separate measured reduction,
`tests/fixtures/cgroup-percpu-accounting.c`. Both unchanged KUnit Images,
with four CPUs and 4 KiB pages, report the same four snapshots:
`current=12845056`, `percpu=9683136`, `slab=3084912` bytes. The original
comparison misses its 1,048,576-byte margin by using a **3,161,920-byte**
delta; including slab leaves only **77,008 bytes**. This directly supports
the [upstream missing-slab diagnosis](https://www.spinics.net/lists/kernel/msg6181815.html)
instead of attributing the failure to compiler code generation. The diagnostic
does not replace the original test or change its FAIL result. Captures are
under `.build/floating-targeted/cgroup-percpu-accounting/`; source SHA-256
`aad061d068084258578c98793aab63a6c19596fa33738ff19990625f2f26365d`.
Its first attempt lacked the small init's cgroup mount and exited 4; the
corrected setup, not a relaxed comparison, obtains these observations.
The separate memcg-deletion accounting and dying-cgroup failures remain open.

The userspace refresh exposes another shared build-path error: livepatch's
recursive module make inherited `O=/work/full/obj`, even though `KDIR` named
the configured kernel. The targeted assertion is RED then GREEN; forwarding
the actual kernel output in both variables builds all **12 livepatch test
modules**, including BTF, with that output mounted read-only. This is still
GCC-built test support, not additional EvoCC target-C coverage. The existing
builder finishes the installation normally; no hand-written PASS replaces
its build metadata. The refreshed inventory contains **970 declared and
970 installed programs** across 128 collections and one installation
dependency. BPF installs 21/21, sched_ext 1/1 and livepatch 8/8 test scripts
plus its 12 support modules. The four remaining build exits are the original
architecture guards in kexec, prctl, thermal/intel/power_floor and
thermal/intel/workload_hint; they are not relabelled as passing builds.

The original Kbuild module-install recipe installs and signs the reference's
**745 modules** into this separate runtime. Every installed module retains
its byte-identical reference ELF prefix and the expected appended signature
structure; cryptographic acceptance still requires loading it on the kernel.
Their hashes and reference Image/configuration identity are in
`selftests/build.json` under `kernelModuleSupport`. They are GCC-built support,
not EvoCC target-C coverage. No accepted K6 or earlier selftest disk changes.

The resulting rootfs exceeds the old fixed 2 GiB image. A sparse 3 GiB
fixture first fails the capacity assertion, then passes after the existing
formatter derives capacity from contents plus explicit metadata/free-space
allowance. Tar data is still streamed once, without another payload archive.
The actual **4 GiB** image formats successfully; SHA-256
`77e48efe6ff7c14c446a9e81b977158c073244f70ad931cd09a10af887713555`.
The K1 harness is **166/166 PASS**, Ruff and `git diff --check` pass.
Original affected-program runtime preflight and the new EvoCC composition
are still pending; build/install completion alone does not close them.

The separate `cgroup-deletion-observation.c` reuses the original
`cg_run_in_subcgroups(..., alloc_kmem_smp, ..., 100)` workload, without changing
its assertions or time limits. On the unchanged KUnit pair, the initial
`memory.current - anon - file - kernel - sock` deltas are **2,187,264 bytes
(GCC)** and **3,047,424 bytes (EvoCC)**, above the 1,048,576-byte margin.
At one second they fall to **757,760** and **548,864**, respectively, and
remain there through second 20. Dying descendants fall from 36 to 5 and
25 to 4, but do not reach zero. This measures delayed accounting and retained
objects on both kernels; it does not prove their exact reference holder or
waive the separate dead-cgroup test. The original immediate FAIL is retained.
The diagnostic exits zero only to report observation completion. Captures:
`.build/floating-targeted/cgroup-deletion-observation/{reference,subject}/boot.log`,
SHA-256 `023d4b9c69e586fad8410073eef62e40b23c8152a5d231ba425d7f216441e36e`
and `c5086fa5bf2e75cf11e143e53b645344d9b672752d95effaf435015cf985e8ed`.
Source SHA-256: `fca5e16dac0c28672da11749c8c15140dbe8afa1204e047d11e3602ca144dfcd`.

### Expanded reference runtime preflight — 2026-09-20

The eleven original affected programs finish on reference Image `8fa43fb4…337b`:
**3 PASS, 5 FAIL, 1 SKIP, 1 TIMEOUT, 1 INCOMPLETE**. Evidence is
`.evidence/kselftest-full/bpf-reference-preflight-20260920/summary.json`;
this is reference-only measurement, not a paired EvoCC result.

| Original program | Observed result and next boundary |
| --- | --- |
| `drivers/net/team:propagation.sh`, `net:tls`, `ublk:test_generic_12.sh` | PASS with matching modules and the real bpftrace runtime; previous setup gaps are no longer the first failure |
| `cgroup:test_cpu` | First seven cases pass; original runner's 45-second limit terminates the program before the final bandwidth cases finish; TIMEOUT retained |
| `cgroup:test_kmem` | Dead descendants and missing-slab comparison fail; the deletion-accounting case passes in this run, without erasing earlier failures |
| `sched_ext:runner` | Seven cases pass before CPU hotplug reaches `cpu_die`'s BUG on HVF; boot is INCOMPLETE, not PASS |
| `livepatch:test-livepatch.sh` | SKIP because the runtime lacks the actual kernel build directory; prebuilt test modules alone do not satisfy its guard |
| `bpf:test_maps` | Unbound UDP socket insertion is refused; exact minimal state probe below |
| `ublk:test_generic_01.sh`, `ublk:test_generic_02.sh` | Real tracing runs and reports out-of-order completions; these are now FAIL, not tool-missing SKIP; attribution remains open |
| `bpf:test_progs` | Completes in 303.646 boot seconds; original summary is `649/5639 PASSED, 128 SKIPPED, 13 FAILED`; not a passing program |

The unchanged sched_ext **hotplug-only** selection passes on the contract's
fixed `tcg-cortex-a72` lane (1 PASS, 0 FAIL), retaining the 45-second test
limit. Its serial SHA-256 is
`7f84efc31ddf6e125f960c68efbb75563efb12499457d9eda3eeff8dd134fc33`.
This isolates an HVF/CPU-model boundary; it is not a complete TCG sched_ext
run or an EvoCC-kernel result. The first mistyped lane name was rejected
before QEMU execution and is not credited as a run.

`bpf-udp-sockmap-state.c` measures **unbound update = -1 / errno 95** and
**bound update = 0** on this exact reference Image. The frozen kernel's
`sock_map_sk_state_allowed` requires `sk_hashed` for UDP, while `test_maps`
expects success before bind. The
[upstream UDP state restriction](https://kernel.googlesource.com/pub/scm/linux/kernel/git/bpf/bpf-next/+/66efd3368ae10d05e08fbe6425b50fdec7186ac7%5E%21/)
corroborates that rule. Source SHA-256:
`7e133059e137e5a3a004e83c1ffcc1869a338187043e98aafaab27e132f3d2ef`;
capture `.build/floating-targeted/bpf-udp-sockmap-state/reference/boot.log`.
The original test is not patched or relabelled PASS.

The 13 BPF failure groups are recorded separately: `bpf_tcp_ca`, `build_id`,
`cgroup1_hierarchy`, `cgroup_v1v2`, `cgrp_local_storage`, `htab_update`,
`livepatch_trampoline`, `res_spin_lock_stress`, `sockmap_basic`,
`sockmap_listen`, `stacktrace_build_id`, `verif_scale_pyperf600`, and
`verify_pkcs7_sig`. Missing `keyctl` and the livepatch sample path are runtime
packaging repairs in progress. The configuration lacks net_cls and BBR;
`res_spin_lock_stress` requests hardware CPU-cycle perf events, unavailable
on this HVF lane (its module file is present). `htab_update` expects EBUSY
but receives EDEADLK; pyperf600 exceeds the verifier's 8192-jump-chain limit.
Build-id, stacktrace and the remaining cases still need individual closure;
none is excused merely because this is the GCC control.

### Runtime prerequisite repairs and original replays — 2026-09-20

The prepared external-module headers now come from the original kernel's
`scripts/package/install-extmod-build`, staged on a Linux filesystem before
copying to the runtime (about 68 MiB, not another source/build clone).
The real configured livepatch sample, matching Module.symvers, and `keyutils`
are installed. `KBUILD_OUTPUT` is exported only when that build directory
actually exists. Reference replay
`.evidence/kselftest-full/bpf-runtime-prerequisites-20260920/summary.json`
passes both livepatch trampoline subtests and PKCS7-from-map. Its fs-verity
subtest still skips because `fsverity` is absent; the top-level program PASS
does not close that subtest. The livepatch shell test progresses from its KDIR
guard to real patch/unpatch execution but FAILs its expected log comparison:
four dynamic-debug transition lines are missing. Both `CONFIG_DYNAMIC_DEBUG`
and `CONFIG_DYNAMIC_DEBUG_CORE` are disabled, while the original collection's
`config` requires the former. No expected line is deleted and no kernel
configuration is changed during the active compilation.

The subsequent RED → GREEN repair installs pinned `fsverity` 1.6-1.2, enables
the ext4 image's `verity` feature, and retains disk-backed `/tmp` for full-suite
VMs. Each VM still uses its own QEMU snapshot; `/dev/shm` remains tmpfs and
the selected K7 lane retains its original tmpfs contract. The original
PKCS7 signature tests then pass **2/2**, including real fs-verity signatures.
The build-id test also passes all three subtests in this fresh VM; that one
observation does not establish why its earlier paged-out case failed.
`stacktrace_build_id` still fails its byte comparison. Evidence:
`.evidence/kselftest-full/bpf-fsverity-stacktrace-20260920/summary.json`.
Runtime build SHA-256:
`f27011d6968155da44d0d314b8b1de849fe00a6107505231e71068bc7768b501`;
initramfs `e3279bdbb00c0e72e2d9db47e26158a08dc9ee562b5f09fdbc8bcad60310f24a`;
disk `d8077a6f4af131f190bb3e8e66054ed1e753c1587ddd95b07d273e82fb642d8f`.

On the fixed `tcg-cortex-a72` lane, the full sched_ext invocation prints
23 PASS / 0 FAIL / 0 SKIP but the original runner reports **TIMEOUT at 45 s**;
the invocation remains TIMEOUT, not PASS. The separate BPF TCG replay passes
`res_spin_lock_stress` and `stacktrace_build_id_nmi`, which lacked hardware
CPU-cycle events on HVF. Ordinary `stacktrace_build_id` still fails on TCG.
Capture: `.evidence/kselftest-full/bpf-tcg-prerequisites-20260920/summary.json`.
These are reference-only results, not the new EvoCC composition's acceptance.

The existing selftest entry point now also consumes a supplemental
`kernel-full` receipt directly, plus its separate reference boot receipt.
It requires the complete installed-object inventory and current hashes,
stable prepared/export inputs, linked vmlinux, matching configurations,
both Image identities and unchanged successful boot logs. It does not
manufacture a KUnit receipt or accept a compilation-only survey.
The K1 harness passes **169/169** tests, the benchmark entry tests **32/32**,
Ruff and `git diff --check` pass. Full new kernel composition, module
relink/install and paired K7 execution are still pending.

The stacktrace discrepancy now has a concrete reduction, not just a shared
FAIL. `tests/fixtures/bpf-stacktrace-representation.c` loads the **unchanged
original BPF program** and runs the original `dd`/`urandom_read` workload.
Both untraced and function-traced observations find two keys and fourteen
different frames: the stack-map copy has status `VALID` with build ID and
file offset, while the subsequent `bpf_get_stack` copy has status `IP`, zero
build ID and the corresponding live address. Function tracing shows both
helper calls with the `d` (interrupts-disabled) flag. In the frozen
`kernel/bpf/mmap_unlock_work.h`, the first lookup queues its mmap unlock on
per-CPU irq_work; a second lookup before that work can run must fall back to
IP-only representation. `kernel/bpf/stackmap.c` implements that fallback,
whereas the original test requires byte equality of the two representations.
This source path explains the measured difference on the reference arm64
kernel; no compiler byte corruption or missing map entry is needed.
The test remains FAIL and the kernel/test source is not patched.
Diagnostic completion is not original-test PASS. Traced capture:
`.build/floating-targeted/bpf-stacktrace-representation/reference-traced/boot.log`,
SHA-256 `62fb82739347d7b881c266173d97f92e3293caa8547413479cb994695cf3def7`.
The static-link attempt failed on missing symbols in the sealed libelf
archive; the working probe uses the same real shared libraries as the
selftest runtime. An initial missing-disk invocation failed before the
diagnostic and remains separate from the two completed observations.

### Original module finalization and execution — 2026-09-20

The module linker now walks original Kbuild link records to their audited C
and assembly leaves, executes those exact links, then runs the original BTF
finalization against the selected vmlinux. It checks protected inputs before
and after; it never invokes a C compiler or substitutes reference module BTF.
The tailored test rebuilt the three original inputs of
`samples/livepatch/livepatch-sample.ko` with frozen EvoCC `77faa6b3…f27d8a`:
all three assemble with matching symbols, link, and complete pahole/resolve_btfids.
Resulting module SHA-256:
`76c761cb52eb6e24675fd8e0f5e0c0700125b1e373c460db6d725a503f06c615`.

Both the unchanged GCC module and this EvoCC module were actually loaded
in separate flights of the unchanged GCC reference Image `8fa43fb4…337b`.
Each passes **5/5** checks: load, execute the replacement function, disable
the patch, observe the restored original function, unload. The small harness
is `tests/fixtures/kernel-livepatch-module.c`; it does not replace upstream
livepatch selftests or their still-failing dynamic-debug expectations.
Capture: `.evidence/k1/module-link-tailored-20260920/{module-link.json,execution.json}`.
Reference/subject serial hashes: `11297799b9718a77ea83d8ecdedaca7ae271c1e84d05f87054636842444010cf`
and `cbdcc51c2047b42db97a38bedc070a8a0843421a3b390323e9e3ecb614ac76d7`.
This is one-module execution evidence, not full EvoCC kernel or 745-module
acceptance. K1 harness **170/170**, Ruff and diff checks PASS.

The remaining ublk ordering failures also have an upstream test-contract
explanation. Maintainer commit
[`5314d25afbc44d0449fa2519d2c9d7f3c319f74c`](https://github.com/torvalds/linux/commit/5314d25afbc44d0449fa2519d2c9d7f3c319f74c)
removes generic_01 because block-layer reordering can yield false positives;
it changes generic_02 to compare request-start and completion order, rather
than expecting monotonically increasing sectors, and adds probe readiness
and CPU affinity. The frozen 6.18.44 copies retain the older sector-based
check. This is corroborating evidence for a known baseline test limitation,
not proof that every individual observed reorder has the same origin.
Both original FAILs and their unchanged sources remain in the denominator;
that upstream patch has not been applied to the acceptance payload.

Reference-result reuse now distinguishes reference and subject runtime
payloads: new EvoCC modules do not invalidate an unchanged GCC control.
Changed reference hashes, missing payload identity and incomplete results
still reject reuse. Its declared RED becomes GREEN; the full-selftest unit
suite is **41/41**. No historical failure is relabeled by this repair.

### Previously missing NEON modules: native data verification — 2026-09-20

The twenty original C inputs of `xor-neon.ko`, its `crypto/xor.ko` caller and
`raid6_pq.ko` now compile with frozen `77faa6b3…f27d8a`, retain exact defined
symbols and link through their original Kbuild graph. Separate flights of the
preserved GCC/EvoCC KUnit Images each pass **156/156 byte comparisons** and
**8/8 load/unload checks**. The reference-built driver explicitly calls all
four RAID6 NEON variants at 256/1024/4096 bytes, checks P/Q against independent
scalar GF(256) calculations, and recovers every pair of lost data disks and
every data/P pair. Direct NEON XOR with one through four sources is also checked.
The original modules' ELF exports, source and code are not patched.

The first subject flight failed the driver's assumption that calibration
must choose NEON: the slower EvoCC NEON code lost to scalar RAID6. Explicit
variant calls remove that coverage dependency; they do not claim performance
parity. Initial console-interleaved results remain separate. Final captures
use the existing low-console setting and read actual kernel log records;
the strict nonce/TAP parser accepts both. No timeout or verdict rule changed.
Receipt: `.evidence/kunit/neon-modules-20260920/execution.json`.
This closes the old NEON refusal class with native data evidence, not the
new BPF composition or a fresh complete module-installation denominator.

The stale module-KUnit receipt text is synchronized with its already completed,
rehash-verified September 14 captures: **33/33 modules**, **780 PASS / 0 FAIL /
1 SKIP** on each kernel. It is not described as a new execution. Current K1
Python tests are **173/173**, benchmark-entry tests **32/32**, Ruff/diff green.

The still-running 6,765-record BPF survey exposed `kernel/trace/trace_fprobe.o`:
E3101 duplicate label. Reduced `inline-source-end-label.c` also makes the C-HIR
interpreter wrongly reach an `end:` label after an early return. Its RED has
zero compilation errors. Inline source-label renaming and synthetic returns
formerly both produced `__evocc_inline_<n>_end`; disjoint generated names fix
the reduction in both interpreters and native execution. Both sealed oracles
also exit zero at O0/O2. The unchanged original tracing unit now emits,
assembles with exact external-symbol parity, and passes DWARF verification;
the failed handler remains present. See `inline-source-end-label.out` for
commands and identities. The changed gate completes **2,588 PASS / 0 FAIL /
4 historical ignored**, all 17 workspace crates, 310 test groups, Clippy
`-D warnings` and fmt. The active survey's compiler is not replaced and its
failure is not retroactively changed.

A separate snapshot RED finds that valid `SOURCE_DATE_EPOCH` was still
invalidated by midnight, even though the driver's `__DATE__` stays constant.
Snapshot identity now follows the driver's signed-64-bit epoch parser; absent
or malformed values retain UTC-date invalidation. Header/content/compiler
checks are unchanged. This protects explicitly frozen overnight runs, not
reuse across different compilers, and does not modify the active process.

A further `cgroup-dead-reference-observation.c` diagnostic leaves the frozen
30-child workload and its twenty-second wait unchanged. There are still five
dying children on GCC and six on EvoCC. Explicit userspace execution on each
of the four CPUs, followed by five seconds of observation, changes neither
count. This rules out that particular scheduling intervention as a remedy;
it does not identify the reference holder or turn the original test green.
Both diagnostic flights complete and shut down. Captures:
`.build/floating-targeted/cgroup-dead-reference/{reference,subject}`; detailed
identities are in `tests/fixtures/cgroup-dead-reference-observation.out`.
Source inspection also confirms that the frozen helper passes NULL instead
of its `arg`, so its requested negative-dentry allocation does not occur.
Upstream fixes that separate coverage defect in
[a8c6daab4b0e](https://kernel.googlesource.com/pub/scm/linux/kernel/git/netdev/net/+/a8c6daab4b0e276508b7ffdd66c60fd3020a9178).
The fix is not applied to this frozen suite and does not explain away its FAIL.

The subsequent stock-drain diagnostic reproduces **8 dying children on GCC
and 9 on EvoCC after 20 seconds** on fixed TCG. Sequentially offlining and
restoring CPUs drains both to **zero**, which remains stable for five seconds.
The frozen `consume_stock` retains an empty slot's css reference while the
ordinary drain skips zero-page slots: exactly the defect in the
[upstream stock-reference fix](https://lists.openwall.net/linux-kernel/2026/08/18/1232)
(`48863da10ba1`). Together these support a shared kernel-stock diagnosis;
hotplug also drains object stocks, so the intervention alone is not exclusive
reference-holder instrumentation. No frozen source or original FAIL changes.
Commands, hashes and the unsuccessful pinned/HVF precursor are retained in
`tests/fixtures/cgroup-dead-stock-observation.out`. The earlier page-ownership
scan is explicitly **inconclusive**: `page_cgroup_ino` reports the closest online
ancestor, not the deleted child. Its zero matches cannot prove absence of
charged pages (`cgroup-dead-page-observation.out`).

Two build-path REDs also catch gaps before the next full composition: generated
module `.mod.c` descriptors were not recompiled after modpost, and the existing
audited module-link/BTF function was not called by `kernel-full`. Both paths are
now connected; generated-source hashes are checked again after final linking.
The full-selftest reader independently checks the module denominator, actual
`.ko` hashes, BTF base and configuration. K1 harness **176/176**, Ruff and diff
checks pass. The active survey uses its original loaded harness/binary; these
changes do not convert its result or claim that 745 new modules have run.

That frozen survey subsequently finishes at **6,764 assembled + 1 refused =
6,765**, with zero symbol drift and all 745 module descriptors enumerated.
Its sole refusal remains `trace_fprobe.o`; the original binary is unchanged.
Receipt `.evidence/k1/20260920T023330Z/substitution.json` says INCOMPLETE,
installed=false, boot=NOT_RUN. The release rebuild and fresh full composition
follow this closure; the targeted repaired unit is not substituted into the
old receipt as though the whole denominator used the new compiler.

### BPF control failures: errno and verifier budget — 2026-09-20

Two more control failures now have matching source-level upstream diagnoses,
not just the observation that GCC also fails. In `htab_update/reenter_update`,
the frozen test expects `-EBUSY` (-16), but `htab_lock_bucket` propagates the
resilient spinlock's documented `-EDEADLK` (-35) on recursive acquisition.
The trace reaches and passes attach and map insertion before observing -35;
the concurrent-update control passes. Upstream
[`590699d85823`](https://git.kernel.org/bpf/bpf-next/c/590699d85823)
corrects this test's errno and makes its tracing attachment more robust
([review and acceptance](https://www.mail-archive.com/linux-kernel%40vger.kernel.org/msg2605619.html)).
No speculative compiler change is needed to account for this exact assertion.

`verif_scale_pyperf600` fails after 116,112 processed instructions, well below
the one-million instruction limit: its pending branch stack reaches **8,193**,
exceeding the separate `BPF_COMPLEXITY_LIMIT_JMP_SEQ=8192`. The frozen test
unrolls 150 copies of an always-inlined frame reader; the loop-helper,
iterator and non-unrolled controls all pass. Upstream
[`c7e617552938`](https://git.zx2c4.com/linux-rng/commit/tools/testing/selftests/bpf?id=c7e617552938dc5e28d9ad87ae485caa35389be7)
addresses this exact test/budget interaction by independently verifying a
global frame-reader function. This is not the older Clang-12 `loop6` problem
documented in the BPF README, and increasing the kernel verifier limit is not
part of this repair round.

Both observations are from the existing reference preflight, not new EvoCC
execution. Original sources and FAILs remain unchanged. The serial capture
`.evidence/kselftest-full/bpf-reference-preflight-20260920/bpf/test_progs/boot.log`
has SHA-256 `52fac1554c92dfa5bb8917509706507413562d08612764fc4de81e5507ee2669`.
Frozen `htab_update.c`, `pyperf.h`, `pyperf600.c` source hashes respectively:
`2ab379136a7912a545633e2d97a6d622079dd480d8bb0293cd3a5ceaadefa491`,
`552211f97afd68eddc2ea45a208d03e8a1eca3c3c1b629efddc096467dbf89c9`,
`1773abaa0a685c576ef90302479d39c72f65f615d5fb781bbf9a2e25a4fc110d`.
The two `sockmap_basic`/`sockmap_listen` groups likewise fail at unbound UDP
insertion with errno 95; their later deletion errors follow failed insertion.
They share the already measured bound/unbound socket-state distinction above.

### Module payload binding and CPU-cgroup runtime — 2026-09-20

Two new REDs reject an old reference module body or test disk being paired
with a new subject Image. Runtime refresh now runs the original
`Makefile.modinst` signing and depmod recipes, without target-C compilation
or stripping, verifies every installed ELF body plus signature trailer and
dependency inventory, and hashes the completed disk. Supplemental full-kernel
selftests require this installation record to match their actual Image and
the linked-module inventory. This does not claim cryptographic verification
from the trailer alone; real module loading remains a separate check.

The original GCC BPF configuration actually installs and verifies **745/745**
modules through that path. `module-install-reference.json` in the existing
selftest work root has SHA-256
`214d6a034631a05637b5d421f5ff150fc0372323c6538ec2d21e008b66e485d6`.
The regenerated disk is
`a1344960087b028a19f88adc5de34c8f13c46816a64ed1ad63dc5b2928b18daa`;
its build record is `2beea2082d39d4dc104336f7f2a2de7159be7849524a8cb2dbf8ccec1aa48acb`.
The earlier disk hash remains part of the historical captures, not current
runtime identity. No new source, build tree or Cargo target is created.
Original livepatch-trampoline and PKCS7-map/fsverity subtests pass on this
newly packaged reference runtime; their earlier failures are retained.

The CPU-cgroup timeout is also a demonstrated runner-budget defect, not a
relaxed compiler watchdog: `test_cpu.c` serially requires at least **46 seconds**
(2 + 2 + four times 10 + 1 + 1), exceeding the inherited 45-second default.
A specific **120-second** program budget now leaves all nine original
assertions intact. The unchanged reference program passes **9/9**, shutting
down at 46.395 boot seconds. Serial SHA-256:
`5929f12298a46ca73ab7a8ed1e6d2f6c93c77df4541a6275e0166c09f39d5898`.
Capture: `.evidence/kselftest-full/reference-modules-budget-20260920/summary.json`.
The new subject result still requires the active full composition; no old
TIMEOUT is rewritten and no full-suite numerator changes.

The same reference-only round also completes unchanged `sched_ext:runner`:
**23 PASS / 0 FAIL / 0 SKIP** and a normal shutdown on `tcg-cortex-a72`, with
an explicit 120-second program budget. Both future compiler sides use that
declared lane: HVF's already reproduced CPU-hotplug BUG is not hidden by a
kernel patch. The old 45-second TCG timeout is still a failed earlier run.
Its serial SHA-256 is
`98298a76d2950604b60718aa2cb622f556a56b6cf4d3cd37477c216db1b05ea2`.
The timeout/lane policy has dedicated RED/GREEN tests and is recorded in
subsequent full-suite receipts; unrelated programs retain their original
budgets. Current K1 harness **180/180**, ladder harness **186/186**, Ruff and
diff checks pass. The compiler binary used by the active survey is unchanged.

### Explicit configuration coverage audit — 2026-09-20

The runtime receipt now compares each collection's original `config` fragment
with the actual boot configuration, without changing any test verdict. A RED
distinguishes disabled features from a built-as-module requirement and from a
module built into the Image; **181 K1 harness tests** pass. On the frozen BPF
configuration, 74 of 128 collections have such fragments: 37 name at least one
disabled feature (130 occurrences, 127 distinct options); 54 have no generic
fragment. These are configuration observations, not 127 proven test failures.
Architecture-specific fragments and prerequisites not listed in a fragment
still require source review; for example BPF's `config.aarch64` additionally
requests `CONFIG_CGROUP_NET_CLASSID`.

The next configuration must address supported DAMON, GPIO simulation, test
modules, tracing and traffic-control prerequisites, alongside BBR/net_cls and
dynamic debug. Architecture-specific entries such as s390/x86 and Rust toolchain
requirements stay separately identified, not silently enabled or excused.
KUnit is disabled in this BPF configuration, so even its completed selftest
run would not constitute the pending expanded KUnit execution. No active
kernel source/configuration or historical runtime result is changed by this audit.

### Bind external test helpers to the actual kernel — 2026-09-20

Installing the 745 kernel modules is insufficient: the runtime also contains
17 upstream BPF/livepatch test-support modules whose BTF and symbol versions
depend on the linked kernel. Runtime refresh now rebuilds those modules in
their two existing staged output directories using the original Kbuild rules
and GCC-14, with the actual kernel mounted read-only. They remain reference-built
test infrastructure, never EvoCC target-C compilation credit. The runtime gate
rejects helpers bound to an old `.config`, `vmlinux` or `Module.symvers` even if
the installed kernel-module inventory itself matches.

Both new regression tests fail before the fix and pass afterward; **183 K1
harness tests**, Ruff and diff checks pass. The real reference rebuild completes
**17/17** module links and BTF processing, preserving all 22 recorded original
source inputs and the protected kernel hashes. An initial build invocation
failed because the generated Makefile retained its former `/work/build`
location; explicit `O=<actual kernel>` fixes that invocation without changing
the kernel. The rebuild record is
`.build/k7-coverage/case/selftests/test-modules-reference-refresh.json`, SHA-256
`ae91de68169421b99dea09c2d94b2ad70a9c29db3030cf6821876128a34a5e8c`.

A dedicated copy-on-write reference runtime was repackaged with these helpers,
using the already installed guest formatter/libraries: no new source tree,
package download or retained archive. The first formatter invocation lacked
`mke2fs` in the sealed image and did not produce a usable disk. The completed
`runtime/reference/scratch.img` is
`05533306e7c3682757b1b7bef10223ae54fba4e049893376b49e349a512f704d`;
its build record is `917a10fe90b9eff58e4d587f9413ec1f6ba832340df7d8edf0f93d6720c9436b`.
The original runtime disk and earlier captures remain unchanged.

The unchanged GCC Image then passes the selected original BPF helper-module
workload: **9 groups / 33 subtests PASS, 0 SKIP, 0 FAIL**, with normal shutdown
at 50.449 boot seconds. This covers BTF module resolution, kfunc/module attach,
module removal, livepatch trampoline and PKCS7 paths; the exact selection is in
`.evidence/kselftest-full/reference-helper-modules-20260920/summary.json`.
Serial SHA-256: `2feacfb946ce861724e172ea9e16518780e5bbaa01c3eec92b9a918da3b72c6a`.
The measurement and summary were saved successfully; a subsequent convenience
print used `status` instead of the boot receipt's `verdict` and raised KeyError.
That reporting error is not a second test run. This remains reference-only
targeted evidence: the new subject's helper rebuild/loading, full 970-program
pair and expanded KUnit execution are still pending.

### KVM needs guest EL2 and the test's IRQ-chip model — 2026-09-20

The existing Image already enables KVM, but the ordinary fixed TCG machine
does not expose EL2. An unchanged `kvm:guest_print_test` control therefore
skips: `Cannot open '/dev/kvm'`. A separate supplemental machine profile now
uses the same `cortex-a72`, four CPUs, 2 GiB and `virt-11.0`, with explicit
`virtualization=on,gic-version=3`. The locked `kernel-target.json`, ordinary
K6 lanes, program sources/arguments and upstream 120-second budgets are
unchanged. Future full-suite receipts hash this profile and use it for both
compilers; a profile mutation invalidates reference reuse and final acceptance.

Native controls establish both prerequisites independently. EL2 alone passes
guest execution, hypercalls and memory-slot operations, but `irqfd_test` still
skips its `kvm_arch_has_default_irqchip()` requirement. The original arm64
helper requires VGICv3; explicitly providing GICv3 then passes **4/4 original
programs**: guest_print_test, irqfd_test, vgic_init and psci_test. All four
shut down normally. `vgic_init` still explicitly skips unsupported additional
priority/EL2 register probes; its program PASS is not every register tested.
These are reference controls, not the unrun subject or all 37 KVM programs.

Capture: `.evidence/kselftest-full/kvm-el2-20260920/`, including both earlier
skip controls and `reference-el2-gic3.json`. Supplemental contract SHA-256:
`0cd57d165402552cb2422e848e31e3910327fa2b000c0a2974da0cf2a99a5d24`.
The GICv3 serial hashes, in the four-program order above, are:
`34c7391c9e69db0eceb87a2fcd0329194713d05846b32fc632aecf52459243ef`,
`ab99120e4d6c31fd94118af4fe7d1a3e118a466cb61d5be67e6a35d33ed8f976`,
`2b8618fb7ada7b2a9c7dc1448cd00e874cb7c2c41c5f8d43ff4ebf459ff841b4`,
`4b8fd45b8d6ffaf13ea933338a2835e583009eb8a68b6e0c623bf8c134a4003d`.

The same control exposes a lower-level display bug: the upstream runner
returns zero for a skipped program, so `parse_transcript` formerly displayed
PASS despite its sole TAP case being SKIP. It now reports SKIP. The full-suite
`program_case` path already inspected that TAP case, so its historical SKIP
counts are not rewritten. Separately, a successful wrapper can contain only
skipped inner assertions (the original FUSE control does). Inner skip totals
are now retained as `nestedSkipSummaries`/`nestedSkipReview`, blocking an
all-tests-PASS claim without guessing a replacement wrapper verdict. Dedicated
REDs become GREEN; current **188 K1 harness tests**, Ruff and diff checks pass.

### FUSE module before control-filesystem mount — 2026-09-20

`filesystems/fuse:fusectl_test` initially reports a successful outer wrapper
but **0 PASS / 1 SKIP** inside: `FUSE control filesystem not mounted`.
`CONFIG_FUSE_FS=m` and the real `fs/fuse/fuse.ko` are present, but the sysfs
mountpoint does not exist until that module loads. Path lookup fails before
the mount can autoload `fs-fusectl`. Runtime refresh now generates its existing
`/tests/modules` prerequisite list from the installed module inventory; the
existing init loads `fuse` before mounting control filesystems. No source test,
kernel configuration, timeout or assertion changes.

The exact original program then executes **1/1 inner assertion PASS, 0 SKIP**
and shuts down normally. Capture:
`.evidence/kselftest-full/fuse-prerequisite-20260920/reference-after.json`;
serial SHA-256 `abf271b3facc3882f2264aef4fe10a188b3495cd6cc2e0685c5f5714d387d5ee`.
The prior skipped execution remains in `reference-before.json`; its wrapper
PASS is not evidence that the FUSE assertion ran.

For this targeted control only, the stopped reference disk received the one
generated `/tests/modules` file through `debugfs`; readback confirmed `fuse\n`.
Its previous bytes/build record/initramfs are preserved with APFS copy-on-write
under `runtime/reference-helper-only`, not another full physical disk copy.
Current disk: `c681b696714388f978fc6ef57594f2531a7c7b4a2cf17163a1b26b0e970c4827`;
build record: `4e16d3b4b3f7af2c96a681fb18dd33b3876182267c4dd05902eb35ed5f74aae4`.
The ordinary production refresh emits the same prerequisite file before its
normal disk packaging. Subject module loading is still a separate acceptance
step after complete composition. This correction does not increase any frozen
970-program or historical full-suite count.

### Full KVM reference collection — 2026-09-20

All **37/37 original programs start** on the separate EL2/GICv3 Cortex-A72
reference machine: **24 PASS, 4 SKIP, 2 FAIL, 6 TIMEOUT, 1 INCOMPLETE**.
Capture: `.evidence/kselftest-full/kvm-reference-el2-full-20260920/summary.json`,
SHA-256 `d6ddd5de49b9981fc1250764dd54a1ebf77a107ade2d9bc396ae26f9614bc762`.
The Image, userspace, original arguments and 120-second upstream timeout are
unchanged. This is reference-only evidence, not a subject or full-suite PASS.
`set_id_regs` additionally retains skipped inner assertions despite its outer
PASS; `vgic_init` reports unsupported priority/EL2 register probes.

The four SKIPs have distinct causes: AArch32 ID-register test requires an
AArch64-only CPU; nested EL2 and SVE are unavailable on this machine; idle-page
tracking is disabled in this configuration. The first three are capability
boundaries of this lane, while the last is a remediable configuration gap.
`arch_timer` misses the unchanged 100-microsecond interrupt margin.
`mmu_stress_test` is OOM-killed while pre-faulting its default 2 GiB memslot
inside a VM with only 2 GiB total RAM; this is not an EvoCC-generated kernel.

The six timeouts are demand_paging_test, kvm_page_table_test,
arch_timer_edge_cases, dirty_log_perf_test, memslot_perf_test and rseq_test.
Paging controls log completed intermediate modes; the timer-edge source also
explicitly permits an IRQ wait to hang until its runner timeout. Neither kind
is excused as a PASS. memslot_modification_stress_test hits reference-kernel
RCU stalls in stage2_unmap_walker, then its timeout marker is interrupted by
kernel output: the recorded INCOMPLETE is retained, not reconstructed as PASS.

After the entire collection finished, the supplemental KVM profile was raised
to **4 GiB** to provide room for the unchanged MMU workload and kernel/page-table
overhead. A regression test is RED at 2 GiB and GREEN at 4 GiB. The locked K6
machine is unchanged. The targeted original MMU rerun no longer OOMs: it
reaches the default 128 GiB virtual mapping/three-vCPU execution, then still
times out at 120 seconds. This repairs a prerequisite, not the full test.
Capture: `.evidence/kselftest-full/kvm-memory-repaired-20260920/summary.json`,
SHA-256 `5e3ce3f6782a6b8859680fb5f35f439bfa1005592103e84a0b55819db8e10923`;
serial SHA-256 `ddc239fc3961fbf594c439ae82b96d4720264819b45d0a9e6e068d47321400f3`.
The prior 37-program capture embeds its exact old 2 GiB machine contract;
the new profile is `85e6a696a4d7208c2634c5c313c61da48bc0926410dd5f38f4027fe7f8ae39c2`.

Configuration auditing now also records the literal arm64 `config.aarch64`
fragment separately from generic requests. Its RED ensures conflicting
generic/architecture values remain visible and x86 requests are not imported.
The real BPF fragment exposes the previously missed idle-page-tracking and
net_cls prerequisites among other differences; these are dependency-review
inputs, not automatically required features or test waivers. The compact audit
is `.build/floating-targeted/selftest-config-audit-20260920.json`.
Both repairs pass **189 K1 harness tests**, Ruff and diff checks. No compiler
binary, active kernel input or original test was changed by these repairs.

The original `arch_timer` also has a successful instruction-clock control:
all **four vCPUs PASS** with `tcg,thread=single -icount shift=0`, the same
Cortex-A72/EL2/GICv3, four CPUs and 4 GiB. The 100-microsecond margin, test
arguments and upstream timeout are unchanged. Serial SHA-256:
`0d118c80c864d8b92fa434699a642a1dfc5ec24d13717d3a22fe5e7aecd9d4ff`;
capture `.evidence/kselftest-full/kvm-timer-icount-20260920/summary.json`
(`82fe40b6bc3694c86428864bd942ffd1e070d2e7d7cde2c9c8c66b793c838a41`).
The full-suite runtime policy now explicitly selects that same lane for this
one program on both Images; other KVM programs keep their original timing
lane. Its regression test is RED then GREEN. This tests virtual timer
semantics, not physical interrupt-delivery latency; the earlier wall-clock
failure is retained. Supplemental contract after adding this lane:
`c306bfe3689eb6ebfcd2bb9ebb5913d058560d096811e7d5506ed1c792ef0d62`.

The same instruction-clock diagnostic does **not** close
`arch_timer_edge_cases`: the original program starts, produces no completed
case, and hits the unchanged 195-second outer watchdog. Its TIMEOUT is in
`.evidence/kselftest-full/kvm-timer-icount-20260920/edge-cases.json`, serial
SHA-256 `4689e19be37d4e2e23af5ee0310d4ba0510fdaa35e8becda90ee538cdf849a95`.
That program therefore keeps its existing full-suite lane. The combined
harness changes pass **190 tests**, Ruff and diff checks; no subject-kernel
runtime result is implied.

Two CPU-capability gaps have subsequent original-program **reference PASS**
controls, with the same Image, payload, arguments and 120-second timeout:

- `host_sve` passes signal/KVM-run SVE preservation on fixed Neoverse-N2;
  serial `83cc4a9b364fa4ecba9ec54890d8283dc8c8acd3236967a38230344c5e13acba`,
  capture `.evidence/kselftest-full/kvm-capabilities-20260920/n2.json`
  (`96e07bccd6473fdf3e50887d85301cfe756e8010fb2dc7e1053bb026e14eb150`).
- `aarch32_id_regs` passes the original RAZ/WI assertions on fixed A64FX;
  serial `c7e344681988706b71ac892c93580e8ab553b2cfa93e1b7bccb45cb64ad60e87`,
  capture `kvm-capabilities-20260920/a64fx.json` in that evidence root
  (`771eb953be5ab9d566429cf04e1b9c71188eea1a5c84be87d727cf4616a1b7c1`).
  Its actual source predicate checks AArch64-only **EL0**, not just EL1;
  the Neoverse-N2 and max controls still skip and remain recorded.

The production per-program policy selects these fixed CPUs on both Images;
other KVM programs retain their existing lanes. QEMU documents that
[max features can change between versions](https://www.qemu.org/docs/master/system/arm/virt),
so the successful exploratory max SVE run is not the selected model.
These controls do not rewrite the earlier 37-program totals or claim a subject
PASS. The initial `hello_el2` controls still skip: the frozen kernel requires
`kvm-arm.mode=nested` plus FEAT_NV2 before exposing `KVM_CAP_ARM_EL2`
(`Documentation/admin-guide/kernel-parameters.txt`, `has_nested_virt_support`).
With that explicit mode, the same original test **passes** on fixed
Neoverse-N2. The boot log confirms `Nested Virtualization Support` and the
original EL2/E2H/ID-register assertions complete: serial
`b24f0f625c2757c6f0b506dc2ef4190bed4cb32061f2a27751180904f4c41782`;
capture `kvm-capabilities-20260920/n2-nested.json`
(`2b08f7877ac0ad52dd90e07dc45b970c77fd261269feba1c8e7357292a576efd`).
The exploratory max/nested PASS is retained separately; the production policy
uses the fixed CPU and records `kvmMode=nested` for this program on both Images.
The default empty mode emits no added kernel option; locked K6 boot arguments,
Image and compiler remain unchanged. All three policy repairs and the explicit
boot-mode path were RED then GREEN. **195 K1 tests**, Ruff, shell syntax and
diff checks pass; final supplemental contract:
`8c55837f4b4d6ab6e3059117800870216a9ed90882dd9e31ccc455cdcf81174a`.

The subsequent fixed Neoverse-N2 control completes the original
`demand_paging_test`: **10 guest modes finish, outer PASS**, within the
unchanged 120-second limit. Serial SHA-256:
`4a32b8c1e322ea6907c85ae0eeb8e7cb3d76d66e91f696e8634afea682b8101d`.
The production policy now uses this same CPU for the program on both Images,
without nested mode, argument changes or timeout extensions. The original
Cortex-A72 timeout remains recorded; this is a fixed-machine control, not
portable performance evidence or a subject PASS.

The same CPU does **not** repair `memslot_perf_test` or `rseq_test`: both still
hit 120 seconds and keep their prior lanes. Memslot setup alone takes
26.1/26.3 seconds in two completed modes; the test also explicitly declines
its first mode because the default slot count exceeds that mode's 8,209 cap.
Rseq's original workload demands 100,000 migrations and more than 50,000
KVM_RUNs. Neither limit nor workload was reduced. All three controls are in
`.evidence/kselftest-full/kvm-capabilities-20260920/n2-timeouts.json`
(`cedc0cb5948574cfa8471835beecfb8b5226df6aacc3b28eb2fa161d5c86c15b`).
The new policy was RED then GREEN; **196 K1 harness tests** and Ruff pass.

The two additional Neoverse-N2 controls, `kvm_page_table_test` and
`dirty_log_perf_test`, both still hit 120 seconds. The former completes
individual mapping phases in roughly 24/16 seconds but not its whole mode
matrix; the latter completes several two-iteration guest modes before the
limit. Neither changes production policy. Their unchanged-program captures
are in `kvm-capabilities-20260920/n2-paging-timeouts.json`
(`817ef0550cccbba299855d40616bbba8f7f05af5911c206b769aab1222ad2ab8`).

The 6,765-object composition has completed its first full assembly/symbol
check and installation, with zero symbol differences. Its conservative
generated-input phase recompiles the entire population against the prepared
output before checking the content fixed point and linking. A separate
6,916-object KUnit invocation was refused by the unified bench's shared-work
lock before compilation; it is **NOT_RUN**, not a second active build.
The lock is retained and the active BPF composition is not interrupted.

### Complete BPF reference replay and PMU boundary — 2026-09-20

With all 745 matching kernel modules and 17 original test-support modules,
the unchanged, unfiltered `bpf:test_progs` completes and powers down at
314.444 boot seconds: **652/5645 PASSED, 127 SKIPPED, 10 FAILED**. These are
the upstream successful-test/successful-subtest counters, not a new top-level
kselftest denominator. The prior complete control was 649/5639, 128 skips,
13 failures; neither result is a program PASS. Capture:
`bpf-full-reference-modules-20260920/summary.json` under
`.evidence/kselftest-full`, SHA-256
`bf97fd343f2ba398ca4976f045cfc3713126dc118fa1aa5cd043ea638c07c2a2`;
serial `1796b1e9b032b2b072d7bcee6c9a4c4347ee2b0a012693381a79d19f6240fbbc`.

`res_spin_lock_stress` is not missing its `.ko`: the installed module hash
matches the payload, and `testing_helpers.c` reaches `finit_module`, which
returns ENOENT. Module initialization requests per-CPU hardware cycle events;
this HVF boot reports `failed to probe PMU`. The same original test, selected
only for diagnosis, **passes both AA and ABBA phases on fixed Cortex-A72 TCG**,
whose boot registers seven PMUv3 counters. It powers down at 19.673 seconds;
serial `d70abd3ae9c1952969e0752cb29c9ead62a9c154fb9ca19c72ea1872b2119a58`,
capture `bpf-rqspinlock-cpu-control-20260920/summary.json`
(`a53706eaeb662f124c934e72c84be864e83eb2ec88b33ecca771aa1358dad207`).
This is reference-only capability evidence, not whole-BPF TCG or subject
acceptance. No missing-file workaround, module source patch or test waiver
is introduced. Full BPF on a suitable machine remains to be measured.

The subsequent **unfiltered** fixed-TCG control times out after completed
group 186; PID1's existing 300-second fallback fires and the guest powers
down at 317.311 seconds. The outer 315+60-second measurement budget and
original arguments were unchanged. Capture `bpf-full-reference-tcg-20260920/summary.json`
(`7e6f8dddb736d6540beb580204ede87693d457c56268e81fd2c1279602705709`),
serial `a3d5493b2e6a20d1cdc1056dae1b86fab523f7fc951446bb1a159e8a47e1e29d`.
Its partial execution is not a complete TCG PASS and does not replace the
full HVF result or cause an unmeasured global BPF CPU-policy switch.

The previously incomplete `kvm:memslot_modification_stress_test` also gets a
current-profile control: **PASS**, all four enabled Cortex-A72 guest modes
join their vCPU threads and power down at 102.617 seconds. It uses the already
specified 4 GiB/EL2/GICv3 lane, unchanged arguments and upstream 120-second
limit. Capture `kvm-memslot-memory-control-20260920/summary.json`
(`591d710439a14a98a7b1b33a2e2ed75614507648d9c942662fc3d353b3104239`),
serial `030822ff7133b6719de0dc969667bbdd6650e436b436ded9870b18d3c4e33249`.
This resolves the reference execution in the current profile; it does not
by itself prove memory pressure was the sole cause of the historical 2 GiB
RCU stall, rewrite that incomplete capture, or establish a subject PASS.

The parser now also retains BPF's internal failure and skip summary in the
existing review fields, without adding its mixed-grain counters to TAP totals
or rewriting an upstream wrapper verdict. A declared RED becomes GREEN;
**197 K1 harness tests**, Ruff and diff checks pass. Old sealed captures are
not rewritten using the new parser.

A bounded native-vGIC control does **not** repair this failure. On the actual
Apple M2 Max / Mac14,6, macOS 26.6 (25G72), `virt-11.0,gic-version=3` with
`-accel hvf,kernel-irqchip=on -cpu host` boots the unchanged reference Image
and reports one PMUv3 counter, but both original AA/ABBA module-init attempts
still fail with ENOENT. This follows the distinction in
[QEMU 11.1.1's HVF implementation](https://raw.githubusercontent.com/qemu/qemu/v11.1.1/target/arm/hvf/hvf.c)
between advertising an OS-emulated cycle counter and the ordinary no-PMU
profile; advertising it is not proof that Linux's requested perf event works.
Capture `bpf-rqspinlock-hvf-vgic-20260920/summary.json`
(`034d1d7a3e316bd537cb4cdecb843811a02fe36ecdb035c333321bbf184fa825`),
serial `4a787db084d7abe9f87dd6165d02e8cf0f556ed3af84dce822de67e6e23d7ac2`.

The existing small `perf-instructions-availability.c` diagnostic confirms that
both instruction and hardware-cycle event opens return ENOENT under that
profile, while software task-clock succeeds. Its completed capture is
`bpf-rqspinlock-hvf-vgic-20260920/perf-event-control-with-disk/boot.json`, serial
`525a3e0891acb43f5635756a635cf39c65974af0de068ca29b68dfbd597f762b`.
The prior invocation omitted the diagnostic's required disk and failed before
running it; `perf-event-control` remains invalid setup evidence, not a test
result. No payload, kernel, original BPF assertion or timeout was changed.
The same host's read-only `hv_vm_config_get_el2_supported` query returns
success with **false**; hardware-assisted nested KVM is not available here.
That measured capability is retained in the summary rather than inferred
from the macOS version. These unsuccessful controls do not change production
boot policy or replace the successful fixed-TCG single-test control.

### Resolve the next runtime configuration without rebuilding — 2026-09-20

Reusing the existing **1.8 MiB** `config-review` directory, original Kconfig
merge/olddefconfig resolves **100 disabled requests in 27 collections**. It
combines 26 original per-collection fragments with the existing BPF overlay
and the new small `tests/configs/kernel-selftest-runtime.config`. The latter
supplies the missing parent options for TCP BBR/DCTCP and HMM, plus KVM's
idle-page interface. The preexisting active and accepted configurations are
untouched; there is **no new Image or runtime PASS** from this operation.

Candidate config SHA-256:
`5bfbe52ad5eaff304df09accdb874f6bcec9ba8050bafb3701cbe1f71f71e382`.
Ordered fragments/hashes, exact merge/olddefconfig forms and before/after
differences are in `.build/floating-targeted/selftest-runtime-config-review-20260920.json`
(`573139cc91c90ada453f941b783e1a33124d2f971ef6c62042e33bb994a41fb1`).
Generic fragment module/built-in disagreements remain visible, not silently
erased by the merge order. Remaining source-defined boundaries include
MEM_SOFT_DIRTY requiring HAVE_ARCH_SOFT_DIRTY (not selected by frozen arm64),
x86-only KVM/TDX/resctrl, s390 UV, and the separately configured
mseal/checkpoint-restore conflict. LKDTM instrumentation/hardening, Rust and
IMA requirements remain separate work. The obsolete TEST_BLACKHOLE_DEV
request is not an available Kconfig symbol; the current code uses
BLACKHOLE_DEV_KUNIT_TEST. None of these observations becomes a test waiver.

### Truncated scheduler verdict regression — 2026-09-23

`tests/fixtures/kselftest-scheduler-truncated.tap` reduces the runner protocol
to an outer PASS with a missing inner FAILED count. Seven RED assertions
confirmed that missing/malformed/negative/reordered/empty inner counters were
silently treated as valid evidence. The parser now reports `nestedSummaryErrors`,
preserves the outer counts, and carries those errors into program accounting as
INVALID_OUTPUT. A passed scheduler wrapper without its summary is also invalid;
an upstream SKIP or an unrelated RESULTS heading is not reclassified.

All **219 K1 Python tests**, targeted Ruff and `git diff --check` pass;
`gate-changed.sh HEAD` correctly selects no Rust crates. No kernel run or sealed
receipt was rewritten. This closes TODO K01a's verdict-integrity subcase, not
K01's runtime NULL fault or K13's full paired acceptance.

### Bounded repair workspace — 2026-09-23

The user authorized retiring superseded payloads and shrinking the existing
case-sensitive build volume before further TODO repairs. This does not change
the kernel configuration, test population, timeouts or historical verdicts.
The 79 GiB host sparse image is a build workspace, not the approximately
143 MiB boot Image. Keep the BPF and expanded-KUnit reference/subject build
roots, their Images, modules, symbols, configs and signing material while
kernel reductions still require them. Keep the shared source and exact headers.

After checking that no QEMU/project container or open file handle used them,
retired these seven `scratch.img` files under `case/selftests/`:

- `scratch.img` (the duplicate staging disk);
- `runtime/reference/scratch.img`;
- `runtime/reference-helper-only/scratch.img`;
- `runtime/subject/scratch.img`;
- `runtime-alsa-20260923/reference/scratch.img`;
- `runtime-alsa-20260923/subject/scratch.img`;
- `runtime-prereqs-20260923/reference/scratch.img`.

Their small build manifests, initramfs files, source/commands and measured logs
remain. Exact historical runtime disks are **no longer retained**: old receipts
remain observations, not promises that those payloads can be replayed without
reconstruction. Rebuilding a historical payload does not inherit its verdict.
The current BPF pair is `runtime-alsa-fixed-20260923/{reference,subject}`;
the separately required KUnit runtime is unchanged. QEMU already uses
`snapshot=on`, so a new test flight must not clone a full runtime disk.

The deleted payloads had about 23.2 GiB of summed allocated-file sizes, but
APFS shared extents make that **not** a physical-space recovery measurement.
Record volume/image measurements after compaction and verify retained hashes
before claiming recovery. No new full workspace or full kernel copy is created.

Also retire the generated `case/selftests/rootfs` staging tree (3.9 GiB)
and only `*.s`, `*.o`, `*.aserr` in `case/bpf/evocc-out` (a 3.4 GiB assembly
cache). Keep the cache manifest/recipe and every installed object in `bpf/out`.
The selftest `--reuse-build --runtime-root` path reads the retained payloads
and build manifest, not the staging tree; `--refresh-runtime` will require
restoring/rebuilding staging first. No source or accepted installed object is
deleted. These generated files are reconstructible, not backed up byte-for-byte.

In-place compaction completed and the volume remounted at its unchanged path.
Host allocated bytes fell from **85,277,872,128 to 64,999,383,040**
(79.42 to 60.54 GiB), recovering **18.89 GiB**, measured on this image rather
than inferred from unrelated host free-space changes. The volume itself now
uses about 54 GiB; its 80 GiB maximum is retained for the later complete link.
All **ten** before/after SHA-256 checks match: four BPF/KUnit Images, their four
configs and both current full runtime disks. This closes the cleanup check,
not any outstanding compiler or kernel-test issue.

The diagnostic workspace now has one **64 MiB** ext4 scratch disk at
`.build/k7-coverage/repair-runtime/scratch.img`, made with the existing selected
selftest formatter command (`mke2fs -q -F -t ext4 -b 1024 -E
lazy_itable_init=0,lazy_journal_init=0 ... 65536`). Its SHA-256 is
`257ea05494f245c52109a8c2cde4fe5e0b8830e9c33d46d988889477e3039fcb`.
The unchanged 1.8 MiB PM-barrier initramfs and both unchanged BPF kernels
successfully load/unload the signed diagnostic module, finish its TAP test
with exit zero, emit the nonce and power off on this smaller disk. Serial hashes:
reference `b8741d48b439675aed9966586463e2b345929f8481984b883adf785d99ad51c6`,
subject `2aab28726851a134d2acea7cf2d7f6645fb59f61229c03c71f7f86515cb0759f`.
These are small repair-environment checks, not a new K6/K7 full-suite result.

### PM barrier completion-order diagnosis — 2026-09-23

TODO K05 is now dispositioned with a real signed-module reduction, not a
successful retry. In the frozen `drivers/base/power/runtime.c`, the worker
clears `request_pending` before handling its queued resume. The documented
`pm_runtime_barrier` contract returns one only when a resume remains pending
and the barrier must wake the device; it returns zero otherwise. The original
`runtime-test.c:177` requires one immediately after an asynchronous get without
preventing that worker from completing first.

`tests/fixtures/pm-runtime-barrier-completion.c` forces the completed-worker
ordering with `flush_work` on 32 of 64 cycles and requires barrier zero in each.
Both GCC/EvoCC kernels pass those checks, allow either documented return in
the other cycles, and verify active state followed by synchronous suspension.
The signed diagnostic actually loads/unloads; its userspace TAP exits zero.
The unchanged original case was then replayed on the separate expanded-KUnit
pair: GCC PASS, EvoCC FAIL at precisely `expected 1, actual 0`. That original
FAIL remains recorded; no assertion or compiler behavior was changed to hide it.

This closes K05's **cause/classification** as an upstream scheduling assumption,
not original-test PASS or K14's full execution. Source, commands, paired counts,
hashes, smaller-disk replay and original KTAP results are recorded in
`tests/fixtures/pm-runtime-barrier-completion.out`.

### Smaller static-key runtime reductions — 2026-09-23

K01 remains OPEN. The tracked `sched-static-key-lifetime.c` module reproduces
missing publication under fixed Cortex-A72 TCG on **both** existing kernels,
without hotplug, BPF, object freeing or scheduler callbacks. It counts missing
pointers rather than dereferencing them. The matching release/acquire memory
gate control passes. In the final paired flight, static-key missing counts are
10,068,125 (GCC) and 5,043,611 (EvoCC); memory-gate counts are both zero.

The separate `static-key-immediate.c` control checks 256 local/cross-CPU toggles
and 512 concurrent toggles bounded by a stable sequence number, without RCU or
object publication. Both kernels pass, with 28,441,826 / 23,169,747 stable queries
and no errors. Disabling TCG block chaining, adding reader ISB, or adding a full
memory barrier does not cure the RCU reduction. None of these observations alone
proves an emulator or kernel root cause, and no production workaround is made.

All flights reuse the **same 64 MiB snapshot disk** and the unchanged Images;
only small signed diagnostic modules/initramfs payloads are built. Existing TAP
parsing retains test FAIL even when boot/shutdown is PASS. Source, build, payload,
counts and serial identities are in
[the bounded record](../../tests/fixtures/sched-static-key-lifetime.out).
This diagnosis exceeded its one-hour slice: retain the reduced failure and next
read-side entry/retirement question, continue independent TODO work, and do not
restart the full campaign or label K01 closed.

### Bounded KVM demand-paging profile — 2026-09-23

K02 remains OPEN. With the unchanged installed demand-paging binary, fixed
Neoverse-N2 EL2 lane and diagnostic `-m 3 -b 64M`, both kernels pass while guest
execution takes **1.487756 / 5.220563 seconds** (GCC / EvoCC). This preserves the
original ten-mode/1 GiB/120-second result as a separate, still-failing workload.

The new `kvm-demand-fault-profile.c` fixture traces the common
`kvm_handle_guest_abort` entry for just 256 KiB, since GCC inlines the inner
`user_mem_abort` helper. Both profiles contain 87 root calls, 86 page-acquisition
and stage-2 mapping calls, and zero buffer overruns/dropped events. Inclusive
instrumented time is higher across acquisition, mapping and page release;
this does not isolate a single runtime stall or establish a compiler root cause.
Depth-limited/inlined call trees are not equivalent per-function cost partitions.

The original binary plus its exact two runtime dependencies, existing init and
the diagnostic occupy a **4.1 MiB** payload on the same **64 MiB** scratch disk.
Both diagnostic TAP results and nonce-bound shutdowns pass. Full commands,
timings, negative setup result and artifact hashes are in
[the bounded profile record](../../tests/fixtures/kvm-demand-fault-profile.out).
No new full runtime, kernel configuration or campaign has been created.

The corresponding compiler reduction found `!x` lowered as two comparisons
(`x != 0`, then boolean `== 0`), preventing the existing branch fusion. The
shared truth-comparison path now emits a direct full-width integer/pointer
comparison, preserving operand effects, poison obligations and the separate
floating unordered path. Its small fixture passes O0/O2 in both interpreters
and GCC14/Clang19/EvoCC native execution. Negative controls retain E3001, E3002
and E3005. A null guard falls from 19 to 14 native instructions.

The unchanged original `virt/kvm/kvm_main.c` command also emits and assembles;
on identical source/flags, prior/current compiler text sizes are 243232/237968
bytes with identical global symbol names/types. The earlier installed object's
larger size includes other fixes and is not used as this patch's delta.
**1094 PASS, 0 FAIL, 3 historical ignores** across six affected crates, plus
clippy and fmt, close this small optimization, not K02. No replacement object
was installed and the accepted Images are unchanged. See
[the replay record](../../tests/fixtures/negated-truth-branch.out).

### Bounded memslot invalidation/destruction profile — 2026-09-23

K03 now has a separate paired reduction, not an assumed shared cause with K02.
The unchanged installed test passes `-m 3 -b 64M -i 2` on both Images. At 256 KiB,
the shared diagnostic traces two invalidations and four page-table destructions
on each kernel with no buffer loss. Deep instrumentation changes costs because
the helper inlining differs, so a second root-only control is retained: whole
test **0.208469 / 1.274435 s**, destruction **3.898 / 81.148 ms**, invalidation
**0.250 / 0.606 ms**, reference / subject. These are single instrumented samples;
neither the nested sums nor the destruction delta explains the whole runtime.

Both inner TAP results and nonce-bound shutdowns pass in both controls. A first
malformed init list and then a short-write diagnostic failure are recorded as
failed setup attempts, not PASS. The existing fixture now handles ftrace's
one-filter-per-write behavior and depth-one leaf rendering. The current payload
is about 5 MiB and reuses the same 64 MiB scratch disk; historical deep logs are
kept, not a second large runtime. [Full record](../../tests/fixtures/kvm-memslot-fault-profile.out).
The original all-mode 120-second timeout remains OPEN; next reduce the repeated
page-table helpers and verify any compiler repair before rebuilding an Image.

### Bounded private leaf substitution — 2026-09-23

The shared C-HIR inliner now admits small, unaddressed private leaves without
requiring an explicit inline hint. Its initial cost model excludes local
objects, loops/calls, label/asm/activation-sensitive behavior and declaration-chain
optimization barriers. The K03 predicate reduction and native controls pass;
the original `arch/arm64/kvm/hyp/pgtable.o` reassembles with identical exports.
Three `kvm_pgtable_walk_continue` calls disappear, but text is **41688 → 41816**
bytes, so this is not evidence of a runtime speedup. No installed object or
Image changed; K03's all-mode 120-second acceptance remains OPEN.

Broader testing also reduced and repaired two existing explicit-inline E3101
paths (floating operand SSA transport and integer-complex parameter/result
initialization). Both interpreters and 18 GCC/Clang/EvoCC O0/O2 native runs pass.
The affected gate is **1812 PASS / 0 FAIL / 3 historical ignored**, with Clippy
and fmt green. [Commands, boundaries and identities](../../tests/fixtures/private-leaf-substitution.out).

### Complete BPF program replay and paired contract controls — 2026-09-23

The common bench flight `20260923T124453Z` reruns the entire original
`bpf:test_progs` on the matching installed payloads, one worker, without
changing its 300-second limit. GCC completes: **651/5644 successful test/subtest
counts, 127 skips, 11 failed tests**. EvoCC completes group 482 then returns
TIMEOUT; both guests power off. Only this program is selected; the other 969
are NOT_RUN in the new checkpoint, not erased historical results. The previous
complete September 22 receipt remains in its evidence directory and Git.

The earlier PMU, resilient-lock and UDP-state diagnoses now have paired small
kernel/API evidence: a signed diagnostic actually loads/unloads, hardware
cycle events return ENOENT while software events work, recursive locking returns
EDEADLK, and all four IPv4/IPv6 x SOCKMAP/SOCKHASH bound/unbound controls pass.
This extends existing reference/source evidence, not a new compiler repair.
The original 11 failure groups, remaining configuration/runtime questions,
commands and all identities are in [the record](../../tests/fixtures/bpf-complete-replay.out).

Eight parser/status RED assertions are corrected: incomplete or duplicated BPF
summaries cannot be accepted behind an outer PASS, all four mixed-grain counters
survive accounting, and the ASCII renderer labels the targeted selection.
234 K1 and 240 ladder tests pass. Reparse preserves actual FAIL/TIMEOUT;
no source assertion, budget, Image, module installation or full-run numerator
is changed. The small paired diagnostic payload is about 10 MiB and reuses the
64 MiB scratch disk. No test from this flight remains active.

### Integrated KVM repair replay — 2026-09-23

The gated negated-truth/private-leaf repairs are installed in `kvm_main.o`
and hyp `pgtable.o`. The complete kernel and 745 modules relink, all 6,765
target-C records retain audited provenance, and generated inputs are unchanged.
The first audit stopped on a canonical-path spelling mismatch in the one-off
adapter; after fixing the lookup, the full audit passes. This is an incremental
composition (two new-compiler objects), not a uniform rebuild.

The new Image `2b87033b…c06626c` boots and powers off in both KVM replays.
Nevertheless, both unchanged default programs still hit **120 seconds** after
three complete guest modes, whereas GCC finishes all ten. **K02/K03 stay OPEN**.
The 4 MiB payload reuses the 64 MiB snapshot scratch disk and original binaries;
no full runtime copy was created. Direct exit/mode observations do not replace
K13's wrapper/nested verdicts. All four flights ended.

[The record](../../tests/fixtures/kvm-hotpath-original-replay.out) binds commands,
objects, generated inputs, Image, module link and serial captures. Parent
Image/vmlinux are retained copy-on-write under
`.build/floating-targeted/kvm-hotpath-20260923/before/`. New modules are not yet
signed/installed/tested, so the old accepted payload cannot accompany this
candidate. Published K6/C02 acceptance remains its historical subject; T04
still requires final uniform composition before new full claims.

### BPF build-id and callback ordering diagnoses — 2026-09-23

Two small real-API observers now close the **classification** of the remaining
build-id/stream assumptions, not full BPF acceptance. The
[source-bound record](../../tests/fixtures/bpf-runtime-ordering.out) preserves
commands, hashes, failed setup attempts, original results and claim limits.

For `stacktrace_build_id`, both Images return seven build-id frames from the
first helper and seven IP fallback frames from the next. Reversing
`get_stackid`/`get_stack` reverses which map contains which representation.
The frozen kernel explicitly allows one outstanding IRQ-disabled mmap-unlock
work item per CPU and falls back when it is busy. The original byte comparison
assumes away that allowed difference; its unchanged replay still fails on both.

For `stream_arena_callback_fault`, the observer loads the unchanged upstream
BPF skeleton and reads the stream before/after callback progress. GCC returns
zero bytes initially, then 888 bytes at the first 10 ms poll; EvoCC already has
958 bytes at its initial read. Both synchronous controls return their fault
record immediately. The upstream runner has no callback-completion observation
before requiring a nonempty stream. The probe does not increase its timeout.

Both diagnostic programs pass all four assertions per Image and shut down.
The original `test_progs -t stacktrace_build_id,stream_success` pair reports
`2/8 PASSED, 1 SKIPPED, 1 FAILED` on each: stream 8/8, build-id comparison FAIL,
NMI variant SKIP on HVF. The earlier full reference stream failure stays in the
historical receipt; this later PASS is not attributed to a compiler change.
The 5.4 MiB diagnostic payload uses the existing 64 MiB snapshot scratch disk;
original-group replays reuse the matched full runtime disks without copying.
K10 still needs complete execution within 300 seconds, plus K12 configuration.

### Static-key publication phase isolation — 2026-09-23

The [phase-bounded signed module](../../tests/fixtures/sched-static-key-phases.out)
uses the same payload on GCC/EvoCC and three accelerator modes. Multi-thread
TCG reports 753,386 / 549,995 missing pointers, and disabling reader preemption
does not cure it; the release/acquire memory-gate controls pass. Nearly all
misses have unchanged phase observations after disable, RCU grace period and
pointer clear. The same static, memory and preemption-disabled tests all pass
on HVF and single-thread TCG on both kernels. All six boots shut down normally.

This narrows K01's runtime discrepancy without attributing a compiler repair,
changing production code or promoting the original sched_ext test to PASS.
The diagnostic payload is about 4 MiB; every flight reuses the 64 MiB snapshot
disk. The exact source, module, payload, Images, serials and command boundary
are recorded in the fixture. K01 remains OPEN pending causal linkage and the
unchanged original replay; there is no live test from this slice.

September 26 negative control: disabling direct translated-block chaining with
QEMU `-d nochain` still produces 273,139 / 105,318 missing pointers in the static
mode and 335,854 / 148,643 with preemption disabled; the memory controls pass.
Both Images finish all tests and shut down. This same-payload experiment rules
out direct block chaining as a necessary condition, not other MT-TCG behavior
or a kernel/runtime interaction. The fixture records its command and hashes;
neither the original sched_ext verdict nor K01's OPEN status changes.

### Four collection build guards — 2026-09-24

The four original arm64 build commands still fail before invoking a compiler:
their upstream Makefiles do not define `all` on this architecture. Twenty
read-only variable controls confirm kexec's x86/ppc64le guard and the native-x86
guards around prctl and the two Intel thermal collections. The prctl conclusion
is deliberately about this frozen recipe, not every individual API/source.

[The source-bound record](../../tests/fixtures/selftest-build-guards.out)
contains Makefile identities, the exact control matrix and capture hash.
No source guards, historical build exits, program population or PASS counts
change. K12a closes classification; K12's new configuration still needs its
own build/runtime evidence. The diagnostic creates no Image or disk copy.

### New reference's module-backed splice prerequisite — 2026-09-26

The reviewed configuration's `CONFIG_TEST_LKM=m` now has actual runtime
evidence: the matching signed `test_module` loads, exposes both sysfs paths,
and unloads. Four full/two-byte splice comparisons and all eight diagnostic
assertions pass. The unchanged upstream `short_splice_read.sh` and helper also
exit zero under the original five-second limit. Its old paired FAIL explicitly
reported the missing module, before trying the absent sysfs objects.

[The small source-bound record](../../tests/fixtures/module-sysfs-splice.out)
binds the module, Image, config, original inputs and serial. Only the GCC
reference is executed here; the new uniform EvoCC composition is still building
and must supply its own matching module. K12 and the old full-run totals remain
open/unchanged. This flight uses a 4.7 MiB initramfs and the existing 64 MiB
snapshot scratch disk; all its processes exited without a large runtime copy.

### PID namespace recycling boundary and bounded progress — 2026-09-26

The [1024-cycle C reduction](../../tests/fixtures/pidfd-recycle-progress.out)
passes in both root and fresh PID/mount namespaces on the retained pair. Root
`pid_max` is 32768, but each new namespace is initialized to 4194304 by the
frozen kernel. The original's 32768-bound search explicitly skips when it
passes that bound; the root sysctl is not its effective recycling limit.

The unchanged original now completes on both sides under 45 seconds, with
**7 PASS / 1 SKIP** internally (outer PASS). Neither that retry nor the bounded
controls constitutes a compiler repair or evidence of actual PID reuse. The
old subject TIMEOUT remains, and the small loops still measure a substantial
subject/reference duration gap. K11f stays OPEN for final-subject validation.
All flights end normally; the controls use a 2.25 MiB payload and the existing
scratch disk, and original replays reuse the retained runtime pair.
