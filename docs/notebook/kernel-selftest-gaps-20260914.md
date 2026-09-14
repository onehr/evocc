# Kernel test closure: remaining concrete gaps

Updated 2026-09-14 08:52 UTC. This is a diagnosis ledger, not a new full-run
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

## Runtime packaging: targeted guest replay complete

| Problem | Direct evidence | Repair / pending check |
|---|---|---|
| Missing `/mnt` and `mkfs.ext4` | `mount_setattr_test.c:397` mounts tmpfs there; ten later cases invoke the ext4 formatter. | Directory and e2fsprogs supplied; **34/34 subtests PASS on each Image**. |
| Unmounted securityfs | CONFIG_SECURITYFS=y, but PID1 had not mounted it; LSM reads `/sys/kernel/security/lsm`. | Mounted when available; **3/3 programs, 15/15 subtests PASS on each Image**. |
| FAT tools and wrong file ownership | The original test first lacked mkfs.vfat, then sudo rejected rootfs files owned by macOS uid 501. The share also strips set-ID bits. | dosfstools/sudo supplied; stream an archive with original Linux metadata to the Linux formatter. **Original FAT program PASS on each Image**. |

Four new packaging assertions and the existing harness tests total 138 PASS;
Ruff, shell syntax and the init's GCC `-Werror` build pass. Captures are
`runtime-prerequisites-20260914` and `guest-ownership-20260914`; the first
mount/FAT failures remain there. Initial approval-service and missing-libarchive
formatter failures were not counted as guest runs. The final image audit
confirms sudoers is 0:0/0440 and sudo is 0:0/04755. No additional tarball or
kernel clone is retained. This is targeted evidence, not a new full-suite pass.

## Shared test defects: not compiler passes

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

- The frozen K6 configuration has VETH, FUSE and OVERLAY as modules, but the
  test runtime has no installed modules. For example, repaired tc-testing
  gets past `/var/run/netns` and fails on veth creation with EOPNOTSUPP on
  both kernels. This needs genuine matching EvoCC modules, not GCC modules
  copied into a purported all-EvoCC composition.
- Ublk helper files are now present. All 29 original scripts reach the
  prerequisite check and SKIP on K6 because CONFIG_BLK_DEV_UBLK is disabled.
  The separate expanded configuration enables it; build/install/runtime
  verification of that configuration is not complete.
- USERFAULTFD is disabled. Four additional mremap failures request 2 GiB
  allocations inside a 2 GiB VM and return ENOMEM. These are actual resource
  and configuration limits, not proved compiler defects or passes.
- BTF requires real compiler type/debug metadata, not the GCC kernel's BTF.
  That compiler capability remains missing; BPF/BTF-dependent tests cannot
  be claimed covered by a no-BTF Image.
- GCC KUnit's seven failing hardware-breakpoint cases request per-task EL1
  breakpoints that arm64 `arch_build_bp_info` explicitly rejects with EINVAL.
  The one PM runtime failure does not reproduce in isolated 6/6 control
  runs, but its full-run FAIL is retained. Neither fact proves EvoCC KUnit.
- The two built-in KUnit compilation refusals require dynamic object size
  inferred from `alloc_size(1)` / `(1,2)`. Ten units depend on that rule.
  Revision 1.9.97 records the user-approved exception; `bb98e29` passes 45
  focused tests, the changed gate, and all ten original-unit replays. The
  complete built-in Image is being rebuilt; EvoCC KUnit has not run yet.
- Six NEON module units reach the original GCC resource header and fail at
  its compiler vector type `__Int8x8_t`. C executable vectors and MIR vector
  lowering are missing. This is authorized compiler work still to do, not
  evidence that those modules are inapplicable to ARM64.
- Other shared failures (including rseq's long workload, timer tolerances,
  statmount, cgroup, perf and ptrace) still require individual diagnosis.
  Equality with the GCC control is not a blanket waiver.

## Next execution sequence

1. Finish the running complete built-in composition and execute EvoCC KUnit.
2. Complete integer-vector implementation with original-unit replay; it is
   the remaining six-unit module compilation gap, not a hardware exclusion.
3. Install matching modules; run the expanded Image's selftests against
   the corresponding GCC control. Retain every failure, including shared ones.
4. Complete individual diagnoses of the other shared failures before a new
   full comparison. The original 935-program denominator remains visible.

No new kernel clone was created. The stress and packaging VMs have exited
normally. The 2,897-unit built-in composition is still running separately.
