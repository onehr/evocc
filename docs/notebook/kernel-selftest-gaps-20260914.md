# Kernel test closure: remaining concrete gaps

Updated 2026-09-14 15:40 UTC. This is a diagnosis ledger. The latest complete
935-program comparison is `.evidence/kselftest-full/20260914T121718Z/summary.json`;
the historical entries below retain their original observations. Targeted
results and additional configurations remain separate from that denominator.

## Latest full run: completed execution, incomplete acceptance

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
that flag is false. This explains the reversed fixture list without a kernel
execution difference. An additional isolated reference-compiler probe was
NOT RUN because of the same approval-service failure; the actual VM diff
and source reasoning are distinguished from that unexecuted check.

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
independent external definitions. Section 10.1.9 still explicitly preserves
`gnu_inline` without effect, so a body correction has been requested, not
silently applied. Both sealed compilers' O0/O2 symbol and execution probes
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
Full supplemental composition and execution remain pending.

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
