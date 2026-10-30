# Final acceptance runbook — 2026-10-13

This runbook plans the acceptance work that runs after the last compiler
repair: T04, K12, K13, K14, T02, K11 with K11f, and B10. It is a plan only.
Nothing in it has run. It was prepared at HEAD `adea2e70` from `TODO.md`, the
[kernel ledger](kernel-selftest-gaps-20260914.md), the receipts in
`tests/contracts/`, the retained evidence and the tooling.

Only read-only commands were used: `tools/bench.py --list`, `--status` and
`--dry-run`, `df`, `du`, `ls`, `uptime`, `hdiutil info`, `diskutil info`,
`shasum`, `container image list` and file reads. The measurements are from
about 02:00 local time on 2026-10-14.

The K13 run also supplies the unchanged replays that K01, K02, K03, K04 and
K10 need (section 6.5). Those items keep their own closure conditions. The T05
code gate is a prerequisite of this plan, not a part of it.

## Update of 2026-10-14

T04g added the tools that this plan lacked. Use them in place of the
manual steps:

- `tools/k1/configure.py` solves configurations C1 to C6 in the sealed image
  and checks each SHA-256. A run reproduced C1 `6afbd212`, C2 `9e28277b`, C3
  `2f938cda`, C5 `f2a06079` and C6 `95853321` exactly, so no replacement
  baseline is needed. Section 3.4 has the C5 recipe wrong: C5 uses plain
  `-m`, not `-y`, and its fragment list includes `net/config` (27 fragments
  in name order), followed by the BPF and runtime overlays. Use
  `configure.py` for step S1.
- `tools/k1/reference.py` builds the GCC reference kernel for steps S2 and K2
  (`--inventory` checks it against the retained inventory). Only its
  configuration step has run.
- `tools/k1/kunit_payload.py --install-deps` builds the KUnit payloads for
  step K4. It has not run on a kernel.
- `kunit.py` and `selftest_full.py` default to the audited source and this
  plan's layout. A selected replay writes its own receipt beside the complete
  one. `tools/bench.py --status ctest` works.
- The owner approved the disk steps of P0 on 2026-10-14. `out-def` and
  `out-pac` are deleted, so their exact trees are no longer retained. The
  receipt `kernel-direct-virt-receipt.json` keeps their Image hashes
  `ce7bd3e3…` and `16a87d16…`. Compaction reclaimed 8.2 GB. The image now has
  a 64 GiB capacity (68.7 GB container, 60.2 GB free). The mount options did
  not change. `/` had 25 GiB free afterward.

## 1. Facts

| Item | Value | Source |
|---|---|---|
| Host volume `/` | 45 GiB free at 02:00, 26 GiB about an hour later. It moved between 22 and 50 GiB on 2026-10-13. Another project shares it. | `df -h /`; ledger, 2026-10-13 |
| Case-sensitive volume | `.build/case`, APFS volume `evocc-case`: 21.3 GB container, 16.2 GB used, 5.0 GB free | `diskutil info` |
| Backing image | `.build/case.sparseimage`: 20 GiB capacity, 17.97 GB allocated on `/`, attached by PID 28144 | `hdiutil info`, `ls -l` |
| Case volume content | `kbuild-probe/src` 1.6 GiB (Linux 6.18.44; 91,117 files matched the archive on 2026-10-12). `out-def` 2.5 GiB and `out-pac` 4.8 GiB: superseded virtconfig trees (Images `ce7bd3e3…`, `16a87d16…`). `out-k16` 4.8 GiB: current direct virtconfig subject (`add1c487…`). `scratch` 0.9 GiB. `m1`: another agent's musl work. `ladder-sysroot` 47 MiB. | `du`; `kernel-direct-virt-receipt.json` |
| Other large host users | `.evidence` 10 GiB, `.build/lean-d06` 11 GiB (another task), `target` 6.8 GiB, `lean/.lake` 5.0 GiB, `.build/reference` 2.4 GiB | `du` |
| Host load | 1-minute load average 15.3 on 12 cores | `uptime` |
| QEMU | `/opt/homebrew/bin/qemu-system-aarch64` 11.1.2, SHA-256 `2239f9f1…`. The contract names 11.0.3, `af7f6f87…`. The September compositions used `45d74da5…`. | `contracts/kernel-target.json`; `tests/fixtures/kernel-current-composition.out` |
| Sealed image | `evo-nexlab:sealed` is `sha256:e8d52bdc…` since 2026-10-08 (adds `libclang-rt-19-dev`). The September compositions used `2c23684c…`. | `contracts/build-environment.json` |
| Build identity | The contract now sets `KBUILD_BUILD_HOST=nexlab`, user `evo`, timestamp `Sun Aug 9 18:25:35 UTC 2026`, `SOURCE_DATE_EPOCH=1786299935`. | `contracts/build-environment.json` |
| Deleted on 2026-10-07 | `.build/k7-coverage`: both kernel pairs, all configurations, runtime disks, the selftest work root, headers and KUnit runtimes. `.build/floating-targeted`: merge records and payload builders. `.build/k1`, `.build/k7` and `.build/kselftest-full` are also absent. | ledger 2026-10-11 and 2026-10-13; `ls .build` |
| Retained inventories | 7,008 target-C units and 848 modules: `.evidence/k1/20260928T075419Z/{units.jsonl,module-link.json}`. 6,916 units and 809 modules: `.evidence/k1/expanded-kunit-link-retry-20260922/`. | `ls` |
| Retained recipes | Exact C2 command: `.evidence/kunit/coverage-config-20260914/reference-network.json`. Fragments: `tests/configs/*.config`. Base: `.toolchain/records/frozen-kernel-gcc.config`. Audit of C5: `configurationAudit` in `tests/contracts/kernel-selftest-full-receipt.json`. | files |
| Bench state | `kernel-kunit` NOT_RUN (no receipt). `kernel-selftest-all` INCOMPLETE 29/970 (selected replay `20260927T223104Z`). `--status ctest` prints INVALID_RECEIPT (risk R10). | `tools/bench.py --status` |

## 2. Order and gates

```text
G0 pre-freeze gate -> F freeze -> P0 disk preparation -> A  B10
  -> S  selftest/BPF line: S1 configuration -> S2 GCC reference -> S3 T04-S
        -> S4 selftest payloads -> S5 K13 -> S6 K11/K11f triage
  -> K  KUnit line: K0 retire S -> K1 configuration -> K2 GCC reference
        -> K3 T04-K -> K4 KUnit payloads -> K5 K14 through T02 -> K6 triage
  -> C  close K12 and T04, refresh receipts and status, T05 documents
```

- **One compiler identity.** All campaigns use the compiler frozen at F.
  A compiler change after F voids every campaign that already ran. Return to a
  small reproducer, repair, pass G0 again, freeze again and rerun.
- **One heavy job at a time.** `.build/bench.lock` serializes `tools/bench.py`
  only. Direct `selftest_full.py`, `kunit.py --check` and `container run` steps
  are not locked. Do not overlap them with a bench run or with another agent's
  Cargo, container or QEMU work.
- **B10 runs first after F.** It is independent of the kernel and takes about
  three hours. A compiler failure found there costs no kernel time.
- **K13** needs T04-S PASS and both S runtime payloads.
- **K14 and T02** need T04-K PASS, the KUnit runtime payloads (gap G2) and a
  byte-exact C2 configuration (check V2), or a recorded contract decision.
- **K11 and K11f** need the K13 receipt. Targeted replays need the S trees
  intact, because every selftest launch re-audits all installed objects.
- **K12** closes after K13 has run the changed collections on the final
  configuration and every remaining gap has a stated reason.
- **T04** closes after both compositions pass their audits and boots.

**Gate G0 (the lead's work, before F):**

1. Merge and commit every compiler-affecting repair: N08 to N11 (the cost
   causes of K01, K02, K03, K10 and K11f), K07 (FORTIFY KUnit), the T03 and D
   items that change code generation, and the APPLICATION queue items that B10
   measures (B08, B08b, L03 to L11c).
2. Pass the full gate on that exact source:
   `tools/gate-changed.sh --full-workspace --offline` (AGENTS.md).
3. Commit the acceptance inputs that are dirty now, for example
   `tests/contracts/benchmark-program-dispositions.json`.
4. Build the release compiler with the project Cargo policy
   (`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2`).

## 3. Configuration

### 3.1 Recorded identities

| ID | SHA-256 | Derivation | Record | State |
|---|---|---|---|---|
| C0 | `4245f8e6…` | K6 arm64 virtconfig, GCC | `.toolchain/records/frozen-kernel-gcc.config` | retained, tracked |
| C1 | `6afbd212…` | C0 with `KUNIT=y`, `KUNIT_ALL_TESTS=y`, `BLK_DEV_UBLK=y` (inferred base; the receipt lists these three requests) | `config` in `tests/contracts/kernel-kunit-receipt.json` | deleted; command not recorded |
| C2 | `9e28277b…` | `merge_config.sh -y -m` of C1, 12 upstream fragments (`net/af_unix`, `net/can`, `net`, `net/forwarding`, `net/hsr`, `net/mptcp`, `net/netfilter`, `net/openvswitch`, `net/ovpn`, `net/packetdrill`, `net/tcp_ao`, `mm`) and `kernel-selftest-coverage.config` (`025dc131…`, unchanged), then `olddefconfig` | `.evidence/kunit/coverage-config-20260914/reference-network.json` | deleted; exact recipe retained; pinned by `kernel-expanded-kunit-contract.json` |
| C3 | `2f938cda…` | C2 with the upstream `bpf`, `hid` and `sched_ext` fragments and `kernel-bpf-coverage.config` last (it disables KUnit) | ledger, "BTF-dependent userspace builds" | deleted; merge record absent; base inferred |
| C5 | `f2a06079…` | C3 with 26 per-collection fragments, the BPF overlay again and `kernel-selftest-runtime.config` (the 2026-09-23 form adds `ARM_SMMU_V3=y`, `ARM_SMMU_V3_SVA=y`). The 2026-09-20 candidate C4 (`5bfbe52a…`) used the older overlay. | ledger 2026-09-20 and 2026-09-26 | deleted; per-collection audit retained |
| C6 | `95853321…` | C5 with `CONNECTOR=y`, `PROC_EVENTS=y`, `TEST_FPU=m`. Exactly these three values change. | `tests/fixtures/kernel-connector-fpu-preflight.out` | deleted; 7,008-unit and 848-module inventory retained |

The 26 fragments are not guessed. The C3 audit
(`.evidence/kselftest-full/20260922T033617Z/summary.json`) and the C5 audit
differ by exactly 100 resolved requests in 27 collections. That is the ledger's
"100 disabled requests in 27 collections". The `bpf` requests (BBR, DCTCP) come
from the runtime overlay. The other 26 collections give the list: `damon`,
`dmabuf-heaps`, `drivers/net`, `drivers/net/bonding`, `drivers/net/team`,
`drivers/net/virtio_net`, `filesystems/binderfs`, `firmware`, `ftrace`,
`gpio`, `iommu`, `ipc`, `kvm`, `lib`, `livepatch`, `memory-hotplug`, `mm`,
`nci`, `pstore`, `splice`, `static_keys`, `sync`, `sysctl`, `tc-testing`,
`user_events`, `zram`.

C6 is the final selftest/BPF configuration (line S). C2 is the final KUnit
configuration (line K). The configurations stay separate: overflow_kunit's
`foo` and `bar` types break the BPF `vmlinux.h` (ledger 2026-09-20).

### 3.2 Options that K01, K04, K10 and K12 need

| Need | Options | Fragment | In deleted C6 | Evidence |
|---|---|---|---|---|
| K01 sched_ext | `SCHED_CLASS_EXT=y` | `kernel-bpf-coverage.config` (C3) | yes | `sched_ext:runner` ran on the C5 pair on 2026-09-28 (GCC 23/23, EvoCC timeout) |
| K04 ublk | `BLK_DEV_UBLK=y` | C1 request | yes | `ublk:test_stress_04.sh` ran on the C5 pair (GCC 45-second timeout) |
| K10 BTF | `DEBUG_INFO_DWARF4=y`, `DEBUG_INFO_BTF=y`, `DEBUG_INFO_BTF_MODULES=y` | coverage overlay (C2), BPF overlay (C3) | yes | 848 module links with BTF PASS (`kernel-current-composition.out`) |
| K10 tracing | `FTRACE`, `FUNCTION_TRACER`, `FTRACE_SYSCALLS`, `NET_SCH_BPF` | BPF overlay | yes | ledger 2026-09-20 |
| K10 BBR, net_cls | `TCP_CONG_ADVANCED`, `TCP_CONG_BBR`, `TCP_CONG_DCTCP`; `NET_CLS_CGROUP=m` selects `CGROUP_NET_CLASSID` | runtime overlay, `tc-testing` | yes | audit difference C3 to C5 |
| K12 review | 100 requests in 27 collections | 26 fragments, runtime overlay | yes | same |
| K06 IOPF | `ARM_SMMU_V3=y`, `ARM_SMMU_V3_SVA=y` | runtime overlay | yes | K06 |
| K12e | `CONNECTOR=y`, `PROC_EVENTS=y` | none tracked | yes | preflight fixture |
| K12f | `TEST_FPU=m` | none tracked | yes | preflight fixture |

All of them were in the deleted C6. The current virtconfig pair (`b6f206de…`,
`add1c487…`) has neither `SCHED_CLASS_EXT` nor `BLK_DEV_UBLK`, and no
retained Image has BTF. For T04, add no option beyond C6. A new option makes a
new configuration identity and a separate population.

### 3.3 Remaining gaps for K12 (from the C5 audit)

| Class | Options (collection) | Reason or decision |
|---|---|---|
| Not arm64 | `X86_AMD_PSTATE_UT` (amd-pstate), `S390_UV_UAPI` (drivers/s390x/uvdevice), `KVM_AMD`, `KVM_INTEL` (kvm), `PROC_CPU_RESCTRL`, `X86_CPU_RESCTRL` (resctrl), `TDX_GUEST_DRIVER` (tdx) | architecture guard |
| arm64 does not select | `GENERIC_ENTRY` (syscall_user_dispatch, K11i); `MEM_SOFT_DIRTY` (mm, needs `HAVE_ARCH_SOFT_DIRTY`) | source-bound |
| No arm64 target | `IMA_APPRAISE`, `IMA_ARCH_POLICY` (kexec); the kexec, prctl and two thermal recipes | K12a |
| Toolchain | `RUST`, `SAMPLES_RUST`, `SAMPLE_RUST_MINIMAL`, `SAMPLE_RUST_PRINT` (rust) | separate work (2026-09-20 review) |
| Kconfig conflict | `MSEAL_SYSTEM_MAPPINGS` (needs `!CHECKPOINT_RESTORE`) | separate population, `kernel-mseal-coverage.config`; not in the 970 |
| Not a symbol | `TEST_BLACKHOLE_DEV` (net) | 6.18.44 has `BLACKHOLE_DEV_KUNIT_TEST` |
| Global hardening | `LKDTM`, `DEBUG_LIST`, `HARDENED_USERCOPY`, `INIT_ON_ALLOC_DEFAULT_ON`, `INIT_ON_FREE_DEFAULT_ON`, `KSTACK_ERASE`, `RANDOMIZE_KSTACK_OFFSET_DEFAULT`, `SLAB_FREELIST_HARDENED`, `SLUB_DEBUG_ON`, `UBSAN`, `UBSAN_BOUNDS` (lkdtm) | separate work. `UBSAN` adds sanitizer options to target commands; `substitute.py` refuses unreviewed effective options. |
| **No recorded reason** | `BOOTPARAM_HARDLOCKUP_PANIC`, `BOOTPARAM_SOFTLOCKUP_PANIC` (bpf) | decide in K12 |
| **No recorded reason** | 55 differences of `bpf/config.aarch64` (41 disabled). It was never merged. It asks for `PROVE_LOCKING`, `DEBUG_LOCKDEP`, `LOCK_STAT`, `DEBUG_PAGEALLOC`, `PANIC_ON_OOPS`, `HZ_100`, `NR_CPUS=256` and more. | decide in K12. These options change timing for the whole kernel. |
| Reported only | 57 module, 26 built-in and 2 value differences across the generic fragments | observations, not failures |
| **No recorded reason** | `move_mount_set_group` installs no program | confirm in K12 |

Recommendation: keep C6 exactly for T04 and give each item above a reason in
K12. Enable an undecided option only by an explicit decision, in a separately
identified configuration, never inside the 970-program run.

### 3.4 Reconstruction and checks

Run the solver in the sealed image on the case-sensitive source. Keep every
input and output in `$CAP/config` (variables in section 6.0). The two small
fragments copy recorded requests; they add nothing.

```sh
mkdir -p "$CAP/config"
printf 'CONFIG_KUNIT=y\nCONFIG_KUNIT_ALL_TESTS=y\nCONFIG_BLK_DEV_UBLK=y\n' > "$CAP/config/kunit-base.config"
printf 'CONFIG_CONNECTOR=y\nCONFIG_PROC_EVENTS=y\nCONFIG_TEST_FPU=m\n' > "$CAP/config/connector-fpu.config"
cat > "$CAP/config/solve.sh" <<'EOF'
set -eu
umask 022
export LC_ALL=C TZ=UTC
T=/work/src/tools/testing/selftests
M=/work/src/scripts/kconfig/merge_config.sh
step() {
  name=$1; base=$2; shift 2
  rm -rf "/tmp/$name"; mkdir -p "/tmp/$name"
  "$M" -y -m -O "/tmp/$name" "$base" "$@" > "/work/cap/$name.merge.log"
  make -s -C /work/src O="/tmp/$name" ARCH=arm64 CC=gcc-14 HOSTCC=gcc-14 olddefconfig
  cp "/tmp/$name/.config" "/work/cap/$name.config"
}
step c1 /work/base.config /work/cap/kunit-base.config
step c2 /work/cap/c1.config $T/net/af_unix/config $T/net/can/config $T/net/config \
  $T/net/forwarding/config $T/net/hsr/config $T/net/mptcp/config $T/net/netfilter/config \
  $T/net/openvswitch/config $T/net/ovpn/config $T/net/packetdrill/config $T/net/tcp_ao/config \
  $T/mm/config /work/configs/kernel-selftest-coverage.config
step c3 /work/cap/c2.config $T/bpf/config $T/hid/config $T/sched_ext/config \
  /work/configs/kernel-bpf-coverage.config
step c5 /work/cap/c3.config $T/damon/config $T/dmabuf-heaps/config $T/drivers/net/config \
  $T/drivers/net/bonding/config $T/drivers/net/team/config $T/drivers/net/virtio_net/config \
  $T/filesystems/binderfs/config $T/firmware/config $T/ftrace/config $T/gpio/config \
  $T/iommu/config $T/ipc/config $T/kvm/config $T/lib/config $T/livepatch/config \
  $T/memory-hotplug/config $T/mm/config $T/nci/config $T/pstore/config $T/splice/config \
  $T/static_keys/config $T/sync/config $T/sysctl/config $T/tc-testing/config \
  $T/user_events/config $T/zram/config \
  /work/configs/kernel-bpf-coverage.config /work/configs/kernel-selftest-runtime.config
step c6 /work/cap/c5.config /work/cap/connector-fpu.config
cd /work/cap
sha256sum c1.config c2.config c3.config c5.config c6.config > SHA256SUMS
/work/src/scripts/diffconfig c5.config c6.config > c5-c6.diff
EOF
container run --rm -i --platform linux/arm64 --network none --cpus 4 --memory 8G \
  -v "$SRC:/work/src:ro" -v "$PWD/tests/configs:/work/configs:ro" \
  -v "$PWD/.toolchain/records/frozen-kernel-gcc.config:/work/base.config:ro" \
  -v "$CAP/config:/work/cap" evo-nexlab:sealed sh -e -s < "$CAP/config/solve.sh"
```

| Check | Predicate | If it fails |
|---|---|---|
| V1 | `c1.config` is `6afbd212…` | Try the other recorded merge form (no `-y`) and `scripts/config --enable`; record each attempt. |
| V2 | `c2.config` is `9e28277b…` | K14 is blocked: `kunit.py` refuses any other KUnit configuration. The owner must decide on a contract revision with a fresh population count. |
| V3 | `c3.config` is `2f938cda…` | Record it; continue to V5. |
| V4 | `c5.config` is `f2a06079…` | Retry without `-y`; record it; continue to V5. |
| V5 | `c6.config` is `95853321…` | Use the equivalence checks V6 and V7. |
| V6 | `configuration_gaps()` of `c6.config` equals the C5 audit, except that the connector and fpu prerequisites are now met; `c5-c6.diff` shows exactly the three changes | Stop. This is a K12 decision. |
| V7 | After S2, `substitute.target_c_records()` lists the same 7,008 units as the retained `units.jsonl`, and `modules.order` gives the same 848 modules as `module-link.json`. For K2: 6,916 units and 809 modules. | Stop. Report the difference. |

If V5 holds, the deleted identity is restored exactly. If only V6 and V7
hold, record an equivalent replacement configuration with its new hash
before S3. The 2026-10-11 preflight asks for that record before a replacement
campaign. Keep the raw `.config` files and merge logs: on 2026-10-11 the hash
was retained without the small raw configuration, and that was a retention
gap. Track the two small fragments in `tests/configs/` after they reproduce
the hashes (the lead's decision).

## 4. Disk plan

### 4.1 Size basis (estimates, not measurements of the new trees)

| Artifact | Estimate | Basis |
|---|---|---|
| GCC reference tree, C6, with objects | 10–12 GiB | The 2,799-unit EvoCC virtconfig tree allocates 5,094,092 KiB (`storage` in `kernel-direct-virt-receipt.json`). C6 has 2.5 times the units and 848 modules. |
| Its target objects (retire after T04 PASS) | 6–7 GiB | On 2026-10-04, retiring the intermediates and link temporaries of four such trees freed 27.4 GiB (volume used 68.31 → 40.87 GiB). |
| Subject tree, an APFS clone after substitution | 9–11 GiB unshared | All 7,008 objects, `vmlinux` and 848 modules are rewritten. |
| Link temporaries | 2–3 GiB at the final link | On 2026-09-21 the final link of `vmlinux.unstripped` filled a 64 GiB volume that held both pairs. |
| Selftest work root | 5–6 GiB | estimate (userspace build of 128 collections, BPF objects) |
| Rootfs staging | about 4 GiB, temporary | 3.9 GiB (2026-09-23); about 4 GiB (2026-09-28) |
| Selftest runtime pair | 7 GiB allocated (3.1 + 3.9) | `tests/fixtures/kernel-uniform-module-runtime.out` |
| KUnit pair (6,916 units, 809 modules) | 21–24 GiB at the link | same basis as the S pair |
| KUnit runtime pair | about 5 GiB | the 2026-09-21 subject disk allocated 2.6 GiB |
| Evidence | under 50 MiB per kernel campaign; under 0.5 GiB for B10 | `.evidence/k1/20260928T075419Z` 20 MiB; K13 capture of 2026-09-22 23 MiB; `.evidence/ladder` about 0.4 GiB in total |
| B10 work trees in `.build/ladder` | 3–6 GiB | estimate; not measured since the 2026-10-04 retirement |

Measure `du -sxk` after every step and correct this table in the ledger.

### 4.2 Budget by phase

A sparse image does not give space back to `/` when files inside it are
deleted. Only `hdiutil compact` does that, and it needs the image detached.
The image allocation therefore keeps its high-water mark until a compaction.
The table assumes P0 leaves about 9 GiB in the volume.

| Point | Volume used | Image allocation | `/` free if 52 GiB at P0 end |
|---|---:|---:|---:|
| After P0 | ~9 | ~10 | 52 |
| S2 reference built | ~21 | ~22 | ~40 |
| S3 T04-S at the final link | ~33 | ~34 | ~28 |
| S3 then retire reference objects and temporaries | ~24 | ~34 | ~28 (~37 after a compaction) |
| S4 payloads at peak | ~40 | ~41 | ~21 |
| S4 staging removed, then compaction | ~36 | ~37 | ~25 |
| K0 retire S to the minimum, then compaction | ~11 to ~21 | ~12 to ~22 | ~50 to ~40 |
| K3 T04-K at the final link | +23 | +23 | −23 |
| K4 KUnit payloads | +6 within freed blocks | no growth | no change |

Two rules govern each step: at least 25 GiB free at its start, and at least
15 GiB free at its end. So:

- **Line S needs about 52–55 GiB free on `/` at the end of P0**, with
  compaction checkpoints after S3 and S4. Without them it needs about 57 GiB.
- **Line K needs about 40–45 GiB free on `/` after K0**, with a compaction
  after K3. If an S item stays open, its runtime pair and Images stay, and
  K0 gives back about 10 GiB less.
- With 25–33 GiB free, as on some days, no kernel phase may start. Free space
  first (4.3), or wait. If line S cannot get its space, line K can run first:
  it needs about 10 GiB less. It still needs G2 and V2.
- Set the image capacity to **64 GiB**. The sequential peak is about 41 GiB.
  A capacity near the need also limits the worst case on `/`. Both pairs at
  once would need about 80 GiB of capacity and more space on `/` than exists.

### 4.3 What to free or move

| Order | Action | Gain on `/` | Approval |
|---|---|---|---|
| 1 | Delete `.build/case/kbuild-probe/out-def` and `out-pac`. Their Images and provenance are in the receipts. Record that their exact trees are no longer retained. Then compact. | about 7 GiB | owner |
| 2 | After G0 and F, with no Cargo job running: retire `target/frontend/debug/{incremental,deps,build,.fingerprint}`, as on 2026-10-11. Keep release binaries and the frozen copy. | measure; 7.5 GiB on 2026-10-11 | lead |
| 3 | After S3 PASS: retire the reference target objects (the units in the new `units.jsonl`) and `.tmp_vmlinux*` in both S trees. Write a SHA-256 manifest first. Keep host-tool objects, `scripts/`, headers, `.config`, `Module.symvers`, `vmlinux`, modules and Images. | 8–10 GiB in the volume | lead |
| 4 | After both S runtime disks exist: remove the staging rootfs. Optionally retire the work root's build products and keep `build.json` and `logs/`. | 4–9 GiB in the volume | lead |
| 5 | After S6: retire S to the minimum. Keep Images, configurations, `System.map`, `vmlinux`, modules and receipts. Keep the runtime pair only while an S item stays open. Then compact. | up to ~25 GiB | lead; record retirement |
| 6 | After T04-S supersedes it: retire the objects of `out-k16`. Keep its Image, `vmlinux`, configuration and provenance. | about 4 GiB | owner (the receipt promises the objects) |
| 7 | `.build/lean-d06` (another task) and the other project's data | up to 11 GiB | not ours; ask |

Never delete `.evidence/`, `tests/contracts/`, `tests/fixtures/`,
`lean/.lake`, `.build/ladder`, `.build/reference`, `.build/linux-src`,
`.build/frozen` or another agent's files.

### 4.4 Resize and compaction (P0 and each checkpoint)

```sh
lsof +D "$PWD/.build/case"                 # must print nothing: no agent may use the volume
mount | grep '/.build/case'                # note the current mount options
hdiutil detach "$PWD/.build/case"
hdiutil compact "$PWD/.build/case.sparseimage"
hdiutil resize -size 64g "$PWD/.build/case.sparseimage"     # P0 only
hdiutil attach -nobrowse -owners off -mountpoint "$PWD/.build/case" "$PWD/.build/case.sparseimage"
diskutil info "$PWD/.build/case" | grep -E 'Container Total|Container Free'
df -h / "$PWD/.build/case"
```

If the container did not grow, find the APFS container with `diskutil list`
and run `diskutil apfs resizeContainer <container> 0`. Compare the new
`mount` line with the old one. On 2026-10-04 compaction reclaimed 28.0 GB; on
2026-10-02 it failed with "Function not implemented" while still attached.

### 4.5 Where each output lives

| Output | Location | Volume |
|---|---|---|
| Frozen compiler | `.build/frozen/<commit>/evocc-darwin` | `/` |
| Configurations, merge logs, manifests, job logs, monitors | `.evidence/acceptance-20261014/` (500 MiB cap) | `/` |
| S trees, headers, selftest work root, runtime pair | `.build/case/final-s/{pristine,out,headers,selftests,runtime/{reference,subject}}` | case |
| K trees and KUnit runtime pair | `.build/case/final-k/{pristine,out,kunit-runtime/{reference,subject}}` | case |
| T04 captures | `.evidence/k1/<run>/` (units, logs, module links, `boot/`) | `/` |
| Reference boots | `.evidence/k1/final-s-reference-boot-<date>/`, `.evidence/k1/final-k-reference-boot-<date>/` | `/` |
| K13 captures | `.evidence/kselftest-full/<run>/` | `/` |
| K14 captures | `.evidence/kunit/<run>/` | `/` |
| B10 captures and work trees | `.evidence/ladder/<lane>/<run>/`, `.build/ladder/<lane>/` | `/` |
| Bench summaries | `.evidence/bench/<run>/summary.json` | `/` |
| Receipts | `tests/contracts/kernel-bpf-full-receipt.json`, `kernel-expanded-kunit-full-receipt.json`, `kernel-selftest-full-receipt.json`, `kernel-kunit-bench-receipt.json` (new), `ladder-*-receipt.json`, `bench-receipt.json` | Git |
| Status page | `docs/status.md`; every bench run rewrites it | Git |

## 5. Time estimates

| Step | Estimate | Basis |
|---|---|---|
| P0 | under 1 h | compaction and resize |
| F | under 0.5 h | copy and hashes |
| A: B10 | 2.5–3 h | The retained lane times in `bench-receipt.json` sum to 5,695 s (csmith 2,053, mbedtls 980, musl 733, llvm-test-suite 649, torture 333, sqlite 252 …). Downloads and the sysroot check add time. |
| S1, K1 configuration | under 0.5 h each | `olddefconfig` steps |
| S2, K2 GCC reference | 1–2 h each at `-j4` | The C5 rebuild in an existing tree took 3,381 s at `-j2` (ledger 2026-09-26). |
| S3 T04-S | 6–6.5 h at `--jobs 4` | The 7,002-unit run took 23,013 s. Each 7,008-unit pass needs about 35,300 s of single-thread compilation (`units.jsonl` of `20260927T233955Z` and `20260928T075419Z`), about 2.5 h on four workers. |
| S4 selftest payloads | 2–4 h | Estimate: clean userspace build and two runtime refreshes. Repeat builds took 112–427 s. |
| S5 K13 | about 11 h at `--jobs 1` | The 2026-09-22 complete pair took 19,742 s with two workers (bench `20260922T033527Z`). One worker takes up to twice as long. |
| S6 triage replays | per selection | The 39-program selected pair took 3,068 s at `--jobs 1`. |
| K3 T04-K | 7–8 h at `--jobs 4` | A 6,916-unit pass needs 42,908 s of single-thread compilation, about 3 h on four workers. |
| K4 KUnit payloads | under 1 h after G2 exists | estimate |
| K5 K14 | under 1 h | Four flights with a 900-second cap each. On 2026-09-22 the guests powered down at 85 and 286 s (built-in) and 8 and 19 s (modules). |
| **Total machine time** | **about 33–40 h** | Triage analysis, compaction waits and any repair loop are extra. |

The final compiler can compile faster or slower than the September ones.
Allow ±30 percent.

## 6. Procedures

### 6.0 Common setup

```sh
cd /Users/onehrxn/dev/30days/c30
C=<frozen commit, 8 hex digits>
EVOCC=$PWD/.build/frozen/$C/evocc-darwin
SRC=$PWD/.build/case/kbuild-probe/src
S=$PWD/.build/case/final-s
K=$PWD/.build/case/final-k
CAP=$PWD/.evidence/acceptance-20261014
JOB=$CAP/jobs
export EVOCC_BENCH_SYSROOT=$PWD/.build/case/ladder-sysroot   # e0.py and substitute.py need a case-sensitive sysroot
export HOMEBREW_NO_AUTO_UPDATE=1                              # no QEMU upgrade during the campaign
mkdir -p "$CAP/config" "$JOB"
```

Step zero before every heavy step:

```sh
df -g / "$PWD/.build/case"      # '/' needs >= 25 and >= (step growth + 15)
uptime                          # record the load; aim for a 1-minute load below 4
pgrep -fl 'qemu-system|substitute.py|selftest_full.py|kunit.py|bench.py|cargo|container run'   # expect no other heavy job
shasum -a 256 "$EVOCC" /opt/homebrew/bin/qemu-system-aarch64
container image inspect evo-nexlab:sealed |
  python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["configuration"]["descriptor"]["digest"])'
```

Launch each long step detached, in its own process group, with a monitor.
A non-interactive shell makes background jobs ignore SIGINT, so enable job
control first. `tools/bench.py` turns SIGINT into an INTERRUPTED receipt.
The snippets use arrays, because zsh does not split an unquoted `$VAR`.

```sh
launch() {
  name=$1; shift
  set -m
  nohup "$@" > "$JOB/$name.log" 2>&1 &
  pid=$!
  echo "$pid" > "$JOB/$name.pid"
  nohup sh -c 'while kill -0 "$1" 2>/dev/null; do
      free=$(df -g / | awk "NR==2 {print \$4}")
      echo "$(date -u +%FT%TZ) free=${free}GiB $(uptime)"
      [ "$free" -lt 15 ] && kill -INT -- "-$1"
      sleep 300
    done' sh "$pid" > "$JOB/$name.monitor" 2>&1 &
}
```

Record each launch in the ledger: PID, log, compiler, configuration, Images,
payload hashes, free space and load. A stale RUNNING receipt is not a live
job; check the PID.

### 6.1 F — freeze

```sh
git status --porcelain -- crates spec tools contracts tests/contracts tests/configs \
  Cargo.toml Cargo.lock rust-toolchain.toml                                   # must print nothing
mkdir -p ".build/frozen/$C"
cp target/frontend/release/evocc "$EVOCC"
shasum -a 256 "$EVOCC"; "$EVOCC" --version
shasum -a 256 Cargo.lock rust-toolchain.toml contracts/build-environment.json \
  contracts/kernel-target.json contracts/kernel-selftest-kvm-target.json \
  contracts/kernel-selftest-memory-target.json tests/contracts/kernel-expanded-kunit-contract.json \
  tests/contracts/benchmark-program-dispositions.json > "$CAP/freeze.sha256"
find "$EVOCC_BENCH_SYSROOT" -type f -exec shasum -a 256 {} + > "$CAP/ladder-sysroot.sha256"
```

Freeze these inputs: commit `$C`; the compiler hash and version text; Rust
1.97.1; `Cargo.lock`; the sealed image digest `e8d52bdc…`; the QEMU hash
`2239f9f1…`; the ladder sysroot manifest; the Linux source. Repeat the
archive comparison of `sourceAudit` in `kernel-direct-virt-receipt.json`
(archive `0f72d938…`) before S2. The source must not change afterwards.

### 6.2 A — B10, the 15 application lanes

Inputs: the frozen compiler, the lane scripts at `$C` with their pinned
archive URLs and digests, the committed dispositions file and the sysroot.
The lanes download missing archives, so `/` needs network access.

```sh
LANES=(ctest torture csmith zlib zstd cjson mbedtls tinycc toybox musl polybench llvm-test-suite jq lua sqlite)
python3 tools/bench.py "${LANES[@]}" --evocc "$EVOCC" --jobs 2 --dry-run    # read-only
launch b10 python3 tools/bench.py "${LANES[@]}" --evocc "$EVOCC" --jobs 2
for l in "${LANES[@]}"; do python3 tools/bench.py --status "$l"; done        # not 'all' (R10)
```

Keep the default populations: `--programs 500 --campaign all --budget 40`.
`--evocc` prevents a Cargo build. The run writes `ladder-*-receipt.json`
(ctest and torture share `ladder-e0-receipt.json`), `.evidence/ladder/`,
`.build/ladder/`, `bench-receipt.json`, `.evidence/bench/<run>/` and
`docs/status.md`.

Last retained denominators: ctest 220, torture 1,692, csmith 1,000, zlib 19,
zstd 44, cjson 23, mbedtls 382, tinycc 11, toybox 203, musl 1,339, polybench
30, llvm-test-suite 1,735, jq 70 (since `1f0928ca` counts `src/main.c`), lua
34, sqlite 86. Any other denominator needs a stated cause.

**Predicate (TODO B10):** one serial run of all 15 lanes with one compiler
hash; unchanged populations and original flags; fresh source-bound five-way
accounting (PASS, compiler-failure, FP-deferred, not-applicable,
oracle-unusable); zero unexplained compiler failures. Report originals,
patched supplements and approved deferrals separately. N/A, deferrals and
unusable oracles are not ALL PASS.

### 6.3 S1–S3 — T04 for the selftest/BPF line

S1: run section 3.4 and its checks V1–V6.

S2: build the GCC reference and boot it.

```sh
mkdir -p "$S/pristine"
cp "$CAP/config/c6.config" "$S/pristine/.config"
cat > "$CAP/reference-s.sh" <<'EOF'
set -eu
umask 022
export KBUILD_BUILD_TIMESTAMP='Sun Aug 9 18:25:35 UTC 2026' SOURCE_DATE_EPOCH=1786299935 \
  KBUILD_BUILD_USER=evo KBUILD_BUILD_HOST=nexlab KBUILD_BUILD_VERSION=1 LC_ALL=C TZ=UTC
make -C /work/src O=/work/out ARCH=arm64 CC=gcc-14 HOSTCC=gcc-14 olddefconfig
cmp /work/out/.config /work/cap/c6.config
make -C /work/src O=/work/out ARCH=arm64 CC=gcc-14 HOSTCC=gcc-14 -j4 Image modules
EOF
launch reference-s sh -c 'container run --rm -i --platform linux/arm64 --network none --cpus 4 --memory 10G \
  -v "$1:/work/src:ro" -v "$2/pristine:/work/out" -v "$3/config:/work/cap:ro" -w /work/out \
  evo-nexlab:sealed sh -e -s < "$3/reference-s.sh"' sh "$SRC" "$S" "$CAP"
# after it ends with exit 0:
K1_ALLOW_QEMU_DRIFT=1 K1_BOOT_LANE=hvf-host K1_KUNIT_AUTORUN=0 \
  tools/k1/boot.sh .build/case/final-s/pristine/arch/arm64/boot/Image .evidence/k1/final-s-reference-boot-<date>
python3 - <<'EOF'      # check V7; append, so tools/k1/bisect.py cannot shadow the standard module
import json, sys
from pathlib import Path
sys.path.append('tools/k1')
import substitute
ref = Path('.build/case/final-s/pristine')
new = sorted(unit for unit, _ in substitute.target_c_records(ref))
old = sorted(json.loads(line)['unit'] for line in open('.evidence/k1/20260928T075419Z/units.jsonl'))
mods = sorted(line[:-2] + '.ko' for line in (ref / 'modules.order').read_text().splitlines())
oldm = sorted(row['unit'] for row in json.load(open('.evidence/k1/20260928T075419Z/module-link.json'))['modules'])
print(len(new), new == old, len(mods), mods == oldm)
EOF
```

The reference tree uses `HOSTCC=gcc-14`, as in T04b. Pass the boot record a
path relative to the repository root. `kernel_selection()` requires that path
to resolve to `<referenceRoot>/arch/arm64/boot/Image`.

S3: clone the reference and run the substitution.

```sh
cp -cRp "$S/pristine" "$S/out"              # APFS clone, not rsync: no second full copy
shasum -a 256 "$S/pristine/arch/arm64/boot/Image" "$S/out/arch/arm64/boot/Image"
ARGS=(--evocc "$EVOCC" --jobs 4 --low-disk --kernel-work-root .build/case/final-s
  --kernel-source-root .build/case/kbuild-probe/src
  --kernel-full-receipt tests/contracts/kernel-bpf-full-receipt.json --allow-qemu-drift)
python3 tools/bench.py kernel-full "${ARGS[@]}" --dry-run              # read-only
shasum -a 256 tests/contracts/kernel-bpf-full-receipt.json             # record the old receipt hash
launch t04-s python3 tools/bench.py kernel-full "${ARGS[@]}"
```

If the run stops after a complete first pass, resume with
`--resume-kernel-prepared .evidence/k1/<first run>` (T04c, T04e). Never edit
a receipt to force reuse.

**Writes:** `tests/contracts/kernel-bpf-full-receipt.json` (overwritten; Git
keeps the old version), `.evidence/k1/<run>/`, `$S/out`,
`$S/{substituted.json,survey-cache.json,prepared-cache.json,evocc-out/,symbols/}`
and a bench row.

**Predicate (TODO T04):** status PASS and `completeComposition` true;
`counts == {substituted: N}` with N = `requestedUnits` (7,008 if V7 holds);
`cacheHits` 0; prepare and verify exit 0; `preparedInputsStable` true;
`failedUnits` empty; export table ready and unchanged after the link;
`relinkExitCode` 0; module links PASS with the BTF base equal to the
`vmlinux` hash; every `objectSha256AfterRelink` equals `installedSha256`
(zero GCC replacement); boot PASS on the same Image hash with nonce and
power-off; `driverSha256` equal to the frozen compiler; QEMU drift recorded.
No runtime result is inherited. Then freeze in the ledger: compiler,
configuration, both Images, `vmlinux`, `unitsSha256`, `inputSnapshotSha256`,
`compileInputSnapshotSha256`, module link plan, receipt and reference boot
hashes. Commit the receipt, then do step 3 of section 4.3.

### 6.4 S4 — selftest payloads

The bench entry cannot build per-kernel payloads: `--runtime-root` requires
`--reuse-build`. Use the existing `selftest_full.py` path, as on 2026-09-28.
Each step needs network access for the pinned apt snapshot, ethtool 6.15 and
alsa-lib 1.2.14 (SHA-256 checked).

```sh
mkdir -p "$S/headers" "$S/selftests" "$S/runtime/reference" "$S/runtime/subject"
container run --rm --platform linux/arm64 --network none -v "$SRC:/work/src:ro" \
  -v "$S/headers:/work/headers" evo-nexlab:sealed \
  make -C /work/src ARCH=arm64 HOSTCC=gcc-14 O=/tmp/headers INSTALL_HDR_PATH=/work/headers headers_install
COMMON=(--jobs 4 --install-deps --build-only --expanded-kernel
  --kernel-receipt tests/contracts/kernel-bpf-full-receipt.json
  --reference-boot .evidence/k1/final-s-reference-boot-<date>/boot.json
  --work-root "$S/selftests" --source-root "$SRC" --headers-root "$S/headers" --allow-qemu-drift)
launch payload-reference python3 -u tools/k1/selftest_full.py "${COMMON[@]}" --kernel-build-root "$S/pristine"
# after it ends with exit 0:
cp "$S/selftests/build.json" "$S/runtime/reference/"
mv "$S/selftests/initramfs.cpio" "$S/selftests/scratch.img" "$S/runtime/reference/"
launch payload-subject python3 -u tools/k1/selftest_full.py "${COMMON[@]}" --refresh-runtime --kernel-build-root "$S/out"
# after it ends with exit 0:
cp "$S/selftests/build.json" "$S/runtime/subject/"
mv "$S/selftests/initramfs.cpio" "$S/selftests/scratch.img" "$S/runtime/subject/"
```

Run the second step only after the first ends. `refresh_runtime()` reads
`build.json` in the work root, so copy it; move only the two large files.
Check each `build.json`: 970 programs in 128 collections plus `net/lib`;
`kernelModules.status` PASS with the Image hash of its own side; 848 signed
modules; 17 rebuilt test-support modules bound to that side's `.config`,
`vmlinux` and `Module.symvers`; `rootfsSha256` equal to the moved disk.
Then remove `$S/selftests/rootfs` (section 4.3, step 4).

### 6.5 S5 — K13

```sh
ARGS=(--no-build --jobs 1 --reuse-selftest-build --expanded-selftest-kernel
  --selftest-kernel-receipt tests/contracts/kernel-bpf-full-receipt.json
  --selftest-reference-boot .evidence/k1/final-s-reference-boot-<date>/boot.json
  --selftest-build-root .build/case/final-s/selftests --selftest-source-root .build/case/kbuild-probe/src
  --selftest-runtime .build/case/final-s/runtime --allow-qemu-drift)
python3 tools/bench.py kernel-selftest-all "${ARGS[@]}" --dry-run      # read-only
launch k13 python3 tools/bench.py kernel-selftest-all "${ARGS[@]}"
```

`--jobs 1` follows K13's "run the whole installed population serially". It
also keeps the host load down for the 120-second KVM programs. Writes:
`tests/contracts/kernel-selftest-full-receipt.json` (overwritten),
`.evidence/kselftest-full/<run>/` and a bench row. Commit the receipt at once.

**Predicate (TODO K13):** all 970 programs started on each side with matched
payloads; zero lost programs (no NOT_RUN); nested verdicts audited; every
failure and skip reported separately. The receipt status is PASS only if
every program passes; otherwise the execution task closes with the fresh
receipt and the triage of 6.6.

This run also supplies these unchanged replays:

| Item | Program | Lane and budget | Still needed to close |
|---|---|---|---|
| K01 | `sched_ext:runner` | `tcg-cortex-a72` (MT-TCG), 120 s | a demonstrated cause or an evidenced disposition |
| K02 | `kvm:demand_paging_test` | `tcg-neoverse-n2-el2`, 120 s | all ten modes inside 120 s, 1 GiB workload |
| K03 | `kvm:memslot_modification_stress_test` | `tcg-cortex-a72-el2`, 4 GiB, 120 s | the same |
| K04 | `ublk:test_stress_04.sh` and the other ublk scripts | 45 s | clean shutdown, or a source-bound disposition; if it stalls, capture the request and reference holders |
| K10 | `bpf:test_progs`, `-no_alu32`, `-cpuv4` | 300 s fallback (`timeout=0`) | complete nested accounting on both sides |
| K11f | `pidfd:pidfd_test` | 45 s | both sides inside 45 s |
| K12e | `connector:proc_filter` | 45 s | the persistent listener's TIMEOUT stays a disposition, not PASS |
| K12f | `fpu:run_test_fpu.sh` | 45 s | 1/1 standalone and 4000/4000 workers on both sides |

### 6.6 S6 — K11 and K11f triage

1. List every non-PASS of both sides from the receipt (`execution.*.collections[].cases[]`
   and `regressions`): FAIL, TIMEOUT, INVALID_OUTPUT, INCOMPLETE, SKIP, NOT_RUN.
2. Match each one to an existing disposition only by the same observable:
   assertion text, errno, source line or skip reason. Existing classes
   include K04b, K09, K10b–K10d, K11a–K11m, K12a and K12e, and the ledger's
   perf, overlayfs, clone3/libcap and net/tls entries. A program name alone is
   not a match.
3. Give each unmatched case a K11 child with a small reproducer and paired
   controls. Prefer `tools/k1/boot.sh` with a small payload and the 64 MiB
   scratch disk, as in earlier diagnoses. Change no timeout and no test.
4. Record the table in the kernel ledger. No blanket "both fail" exemption.
5. K11f: read the `pidfd:pidfd_test` rows. Both sides must finish inside 45 s;
   record the nested counts (7 PASS / 1 SKIP before) and the shutdown times
   (17.5 and 42.1 s on 2026-09-28).
6. A `--selftest-select` replay overwrites the receipt path. Commit the full
   receipt first. After replays, copy `.evidence/kselftest-full/<K13 run>/summary.json`
   back to `tests/contracts/kernel-selftest-full-receipt.json`; the bytes are
   identical.

**Predicate (TODO K11):** every non-PASS has evidence and an action or a
justified limit; each new class has a small reproducer and a child ID.
**K11f:** the unchanged original passes on the final Image with N08–N11 in
place.

### 6.7 K0–K3 — T04 for the KUnit line

K0: retire S to the minimum and compact (section 4.3, step 5), but only after
S6 and the K01, K02, K03, K04 and K10 replays. K1: use `c2.config`; V2 must
hold. K2: as S2, but copy `c2.config` into `$K/pristine/.config` and use a
`reference-k.sh` that compares against `c2.config`. Check V7 against
`.evidence/k1/expanded-kunit-link-retry-20260922/` (6,916 units, 809
modules). Boot the reference with `K1_KUNIT_AUTORUN=0` into
`.evidence/k1/final-k-reference-boot-<date>`. If HVF fails, use
`K1_BOOT_LANE=tcg-cortex-a72 K1_BOOT_TIMEOUT=900`.

K3:

```sh
cp -cRp "$K/pristine" "$K/out"
ARGS=(--evocc "$EVOCC" --jobs 4 --low-disk --kernel-work-root .build/case/final-k
  --kernel-source-root .build/case/kbuild-probe/src
  --kernel-full-receipt tests/contracts/kernel-expanded-kunit-full-receipt.json --allow-qemu-drift)
python3 tools/bench.py kernel-full "${ARGS[@]}" --dry-run              # read-only
shasum -a 256 tests/contracts/kernel-expanded-kunit-full-receipt.json  # record the old receipt hash
export K1_KUNIT_AUTORUN=0
launch t04-k python3 tools/bench.py kernel-full "${ARGS[@]}"
unset K1_KUNIT_AUTORUN
```

`K1_KUNIT_AUTORUN=0` keeps the composition boot separate from K14. The
predicate is the T04 predicate of 6.3, with 6,916 units and 809 unsigned
modules (`MODULE_SIG` is off in C2). Then retire the K reference objects and
temporaries, as for S.

### 6.8 K4 — KUnit runtime payloads (gap G2)

No tracked tool builds this payload. `kunit.py` and `selftest-init.c` only
read it; the September builder lived in `.build/floating-targeted`. Before
K5, the lead must assign a small builder in `tools/k1` with RED/GREEN tests.
It must produce, for each side, a folder that `kunit.runtime_inputs()` accepts:

- `build.json` with `suiteModules` equal to the contract list (31 modules, in
  order), `kernelModules` from `selftest_full.install_kernel_modules()` for
  that side's tree, `initramfsSha256` and `rootfsSha256`;
- `initramfs.cpio` with the init from `tools/k1/selftest-init.c` and the
  `/full-suite` marker;
- `scratch.img`: an ext4 root with the installed modules,
  `/tests/kunit-modules` (the 31 names) and the module tools.

The subject side must install exactly the linked modules of the T04-K
receipt; `check_module_runtime()` compares them.

### 6.9 K5 — K14 through T02

```sh
ARGS=(--allow-qemu-drift --kunit-source-root .build/case/kbuild-probe/src
  --kunit-kernel-receipt tests/contracts/kernel-expanded-kunit-full-receipt.json
  --kunit-reference-boot .evidence/k1/final-k-reference-boot-<date>/boot.json
  --kunit-reference-runtime .build/case/final-k/kunit-runtime/reference
  --kunit-subject-runtime .build/case/final-k/kunit-runtime/subject
  --kunit-builtin-initramfs .build/initramfs/initramfs.cpio)
python3 tools/k1/kunit.py --check --source-root .build/case/kbuild-probe/src \
  --kernel-receipt tests/contracts/kernel-expanded-kunit-full-receipt.json \
  --reference-boot .evidence/k1/final-k-reference-boot-<date>/boot.json \
  --reference-runtime .build/case/final-k/kunit-runtime/reference \
  --subject-runtime .build/case/final-k/kunit-runtime/subject      # read-only preflight
python3 tools/bench.py kernel-kunit "${ARGS[@]}" --dry-run             # read-only
launch k14 python3 tools/bench.py kernel-kunit "${ARGS[@]}"
```

The KUnit parser files in `kbuild-probe/src` match the contract hashes
(`a9314b79…`, `1bb43859…`, `6f0f9f96…`). Pass every path: the `kunit.py`
defaults name deleted `.build/k7-coverage` paths. Writes:
`tests/contracts/kernel-kunit-bench-receipt.json` (new),
`.evidence/kunit/<run>/` and a bench row.

**Predicate (TODO K14):** all 1,504 built-in cases and all 31 modules with
765 cases run on each matching kernel; duplicate-case multiplicity kept; the
upstream switch-entry XFAIL and configuration limits kept; every non-PASS
explained individually. Expected known classes: seven `hw_breakpoint` FAIL
(K08), the PM runtime assertion (K05), FORTIFY (K07 must have landed). The
bench status is PASS only if every case passes.
**Predicate (TODO T02):** that real K14 execution ran through
`tools/bench.py kernel-kunit` and wrote a fresh receipt.

### 6.10 C — close K12 and T04

- **K12:** confirm that the 27 reviewed collections, connector and fpu ran on
  both sides in K13, or record why not. Give every gap of 3.3 a reason. Keep
  any changed population, such as mseal, separate from the 970.
  **Predicate (TODO K12):** runnable supported sets built with matching
  payloads; precise reasons for the unavailable ones.
- **T04:** both compositions frozen and audited (6.3 and 6.7), with boots.
- Refresh receipts and run `python3 tools/render-status.py`. Update TODO.md
  and the ledgers in the same commit. Finish the T05 documentation part.

## 7. Risks

| ID | Risk | Effect | Mitigation |
|---|---|---|---|
| R1 | Disk on `/` is shared and moves by tens of GiB | A phase cannot start or must stop | Section 4; free space first; monitor; compaction checkpoints |
| R2 | Compaction and resize need the volume detached | Other agents use `.build/case` (`m1`, `scratch`) | Agree on idle windows; check with `lsof +D` |
| R3 | 120-second KVM limits under host load | On 2026-09-28 even GCC reached only 7 of 10 modes. The load was 15.3 at this check. | `--jobs 1`; pause other heavy work; log the load every 5 minutes; never raise a budget; a reference TIMEOUT is a shared deadline failure |
| R4 | QEMU drift | Every result is a drift result, not the locked 11.0.3 lane | `--allow-qemu-drift`; hash QEMU before and after each phase; no `brew upgrade` |
| R5 | MT-TCG instruction-publication defect (K01b) in QEMU 11.1.2 | Code patching on MT-TCG lanes can run stale instructions on both Images: the KUnit lane, `sched_ext:runner` and the TCG KVM lanes. HVF and the single-thread icount lane are not affected. | Check each FAIL that involves static keys, ftrace, kprobes, BPF trampolines or livepatch against the K01b signature with evidence. Do not change the recorded lanes. |
| R6 | Configuration not byte-exact | C2 mismatch blocks K14 (contract pin); a C6 mismatch makes a replacement baseline | V1–V7; owner decision for the KUnit contract |
| R7 | Tool defaults name deleted inputs | `kunit.py` defaults to `.build/k7-coverage`; `selftest_full.py` to `.build/linux-src` (case-damaged), `.build/kselftest-full` and `.build/kselftest/headers`; `substitute.py` to `.build/k1` and `.build/linux-src`; the old receipts and reference boots name deleted trees | Pass every root explicitly (section 6) |
| R8 | Missing builders | No tracked KUnit payload builder (G2), configuration merge tool or reference build tool | Section 3.4 and 6.3 scripts; G2 is a code task before K5 |
| R9 | Receipts are overwritten in place | Status shows the new run; a selected replay replaces the full K13 receipt | Record old hashes; commit at once; restore from the run's `summary.json` |
| R10 | `--status ctest` prints INVALID_RECEIPT | `ladder-e0-receipt.json` holds only the last e0 suite; `--status all` stops with exit 2 | Check lanes one by one; the aggregate keeps both rows |
| R11 | Long runs die silently | Stale RUNNING records (2026-09-26, 2026-09-27) | Detached launch, PID files, monitor; `--resume-kernel-prepared` |
| R12 | Identity drift outside the compiler | New random module-signing keys; sealed image `e8d52bdc…` instead of `2c23684c…`; build host `nexlab` | Record them; no comparison of key-dependent bytes with September |
| R13 | Network needed | B10 downloads; `--install-deps` uses apt, ethtool and alsa-lib downloads | Run when network is available; all downloads are hash-checked |
| R14 | A repair during acceptance | Mixed compiler identities | Stop, reproduce small, repair, pass G0, freeze again, rerun all |

## 8. Checklist

1. [ ] G0: repairs merged; full gate green; acceptance inputs committed; release compiler built.
2. [ ] F: frozen compiler copied and hashed; freeze manifest written; source audit repeated.
3. [ ] Owner decisions recorded: delete `out-def`/`out-pac`; image to 64 GiB; KUnit contract if V2 fails; replacement baseline if V5 fails; K12 items without a reason.
4. [ ] P0: volume idle; delete approved trees; detach, compact, resize, attach; `df` shows the plan's free space.
5. [ ] A: B10 dry-run, run, per-lane status; five-way accounting checked; commit.
6. [ ] S1: configurations solved; V1–V6 recorded; raw configurations kept.
7. [ ] S2: reference built; reference boot PASS; V7 holds.
8. [ ] S3: clone; T04-S dry-run, run; predicate checked; identities frozen in the ledger; commit; retire reference objects.
9. [ ] Compaction checkpoint if `/` is below 25 GiB plus the next step's growth.
10. [ ] S4: headers; reference and subject payloads; `build.json` checks; staging removed.
11. [ ] S5: K13 dry-run, run with `--jobs 1` on a quiet host; commit the receipt at once.
12. [ ] S6: triage table; K11 children; K11f verdict; K01–K04/K10 verdicts recorded; full receipt restored after replays.
13. [ ] K0: retire S to the minimum; compact; `/` has at least 40–45 GiB free.
14. [ ] K1–K3: V2 holds; reference built and booted; V7 holds; T04-K predicate checked; commit; retire reference objects.
15. [ ] K4: G2 builder merged and tested; both KUnit payloads built and checked.
16. [ ] K5: `kunit.py --check` PASS; K14 through `tools/bench.py kernel-kunit`; commit.
17. [ ] K6: every K14 non-PASS explained.
18. [ ] C: K12 reasons; T04 closure; receipts, status, TODO and ledgers updated; T05 documents.
