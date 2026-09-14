# Kernel test closure: remaining concrete gaps

Updated 2026-09-14 11:02 UTC. This is a diagnosis ledger, not a new full-run
result. The last complete 935-program comparison remains
`.evidence/kselftest-full/20260913T145405Z/summary.json`; later targeted results
remain separate in the full-selftest and KUnit receipts.

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
  That compiler capability remains missing; BPF/BTF-dependent tests cannot
  be claimed covered by a no-BTF Image.
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
  Equality with the GCC control is not a blanket waiver.

## Next execution sequence

1. Complete matching module installation and targeted runtime tests. The first
   three high-impact modules are VETH, FUSE and overlayfs; their 31 original
   target-C records now emit and assemble with exact symbol parity. This does
   not close the complete module population or the six NEON refusals.
2. Complete integer-vector implementation with original-unit replay; it is
   the remaining six-unit module compilation gap, not a hardware exclusion.
3. Install matching modules; run the expanded Image's selftests against
   the corresponding GCC control. Retain every failure, including shared ones.
4. Complete individual diagnoses of the other shared failures before a new
   full comparison. The original 935-program denominator remains visible.

No new kernel clone was created. The first full KUnit and targeted packaging
VMs exited normally; the padding repair's changed gate and release build pass.
Existing runtime disks and earlier failed captures are retained.
