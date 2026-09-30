# R02a/b/d: qualified bounded Linux pilot

Date: 2026-09-30. The registered measurement runner, three-track preflight and
native UB-profile fan-out are implemented and qualified for this **bounded
pilot**. This closes the stated implementation/pilot scope of R02a, R02b and
R02d in [TODO.md](../../TODO.md), not their parent R02, final Q2, G2 or Q3.
All three search-track verdicts and all twelve A/B latency comparisons are
**INCONCLUSIVE**. No AI campaign is admitted. Negative measurements are results,
not reasons to silently increase the frozen budget or weaken acceptance.

The [compact tracked receipt](../../tests/contracts/eqsat-r02-pilot-receipt-20260930.json)
indexes the raw capture, hashes, gates and denominators. It is a checkpoint;
replay the raw evidence to requalify it, rather than trusting its status label.

## Frozen subject and measurement boundary

- Implementation: `b27e5e230eabd899459a429c7e172f4ba8077590`; manifest commit:
  `2d39a968`. Earlier regional-fuel and historical-fixture corrections are
  `3bb0666a` and `52ce286c` respectively.
- [Frozen manifest](../../tests/contracts/eqsat-linux-pilot-20260930-scratch-fix.json):
  SHA-256 `9d21b5838ed38129248bdd898b8bf70e327680349068fb8f13d66f9e84740cc2`.
  Compiler, checker, backend, census, collector, Lean/imports, source/header,
  spec and guest tool identities are frozen there.
- Raw capture: `.evidence/eqsat-pilot/pilot-xb4aobn_`; receipt SHA-256:
  `aee9f7ff20cc06b5cc7410c884e2a1074a2ed316dce9ad577710ab6891c16530`.
  The capture is ignored, not a second source archive in Git.
- Linux AArch64 hardware VM on Apple M2 Max, two vCPUs, 3 GiB RAM, sealed
  `evo-flight-lab:sealed` image digest
  `2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2`.
  This is **not bare-metal Linux or an instruction-emulator speed measurement**.
- Reference tools: GCC 14.2, Clang/LLVM 19.1.7 and glibc 2.41. The actual
  binary hashes, not these human-readable version labels alone, are admitted.
- A: EqSat off with a canonical zero-transaction identity chain. B: the shared
  persistent default-v4 chain, at most eight independently checked transactions.
  Both use the same ordinary O2 lowering: folds, reuse, linear joins, constant
  edge pruning and immutable globals. A is **not an unoptimized front end**.
- Semantic controls remain GNU11, default wrapping/aliasing, no fast math,
  existing FP/contraction profile and PIC. Checked and unchecked native profiles
  are separate experiments, not different source or interpreter languages.

| Case | EvoCC subject coverage | Other linked C objects / workload |
| --- | --- | --- |
| P0 | One complete integer-mix TU | GCC-built harness; both interpreters and native |
| zlib 1.3.2 | `deflate.c`, `inflate.c` | Nine other core TUs + harness use GCC; fixed 64 KiB compression/decompression, 64 rounds |
| zstd 1.5.7 | `zstd_compress.c`, `zstd_decompress.c` | 24 other TUs + harness use GCC; ASM disabled, legacy support 0; same fixed workload |
| cJSON 1.7.19 | Complete `cJSON.c` | GCC-built harness; 4,096 parse/print/compare rounds |

These are four registered cases, three library families, six input modes and six
subject TUs. No selected subject object is silently replaced on failure.
GCC controls validate the same results, but **there is no timed GCC performance
comparison here**. This pilot does not rerun or increase the original application
campaign PASS counts. Libc-based workloads have native evidence, not fabricated
C-HIR/EvoIR interpreter results.

## Implementation and admission

`python3 tools/bench.py --eqsat-pilot MANIFEST` uses the existing bench lock and
bounded capture path. It owns exactly one disposable VM, checks identities before
work, preserves every attempt and cleans up only that VM. Each attempt records
argv, output identity, status, wall/CPU/RSS and scope; failures cannot be replaced
by zero or successful old results. Retained storage is capped at 1 GiB, launch
headroom at 8 GiB, one job, no retries. Existing source trees/image are reused;
no Linux clone, workspace clone or large test disk was created.

`--ub-checks=checked|unchecked` now reaches the driver, CodegenOptions and MIR.
The unchecked backend omits **only native UbAssume trap enforcement**. C lowering
still creates its obligations; interpreters retain the same UB diagnoses. Other
predicate users, explicit traps, object/lifetime operations and FP admission are
not removed. Native guards expressed as UbAssume are precisely what unchecked
omits; this must not be described as preserving those runtime checks.
Requested/effective profile and backend identity remain explicit.

Each variant fans out **one canonical verified IR and immutable chain/bundle**
through `evo-ir-asm`, not a second frontend or EqSat invocation. Both native twins
must bind identical IR/chain/bundle digests. Every unchecked variant/input first
requires the corresponding checked execution to pass without a trap. This is an
observed necessary definedness condition, not proof against all unmodeled C UB.
Tests cover a real checked SIGTRAP, defined twin parity, unchanged interpreter
errors, exact identity mismatches, other predicate users and preserved explicit
traps. Invalid unchecked executions are not used as performance samples.

The complete capture contains:

- **102 verified optimized chains**: six baseline + 96 policy-evaluation chains.
- **138 independently replayed EVP certificates**: 12 baseline + 126 policy.
  EqSat-off identity chains contribute **zero EVP proof successes**.
- **108 checked/unchecked twin admissions**: 12 baseline + 96 policy/input pairs.
- **576/576 result-checked timed executions**, plus **48 warmups**. No missing
  result, failed policy evaluation or proof rejection in the successful capture.

The independent audit reconstructs build arguments, expected stage inventory,
IR/twin identities, every chain, outputs, ELF metrics, caps, enumeration and
statistics. It does not rerun timing or turn collector telemetry into execution
attestation. Ten read-only forged-capture variants are rejected, including profile,
argv, failed checked twin, IR digest, missing sample, policy class count and rule/
shortcut SEARCHABLE claims. The original evidence remains byte-identical.

## R02b first output: default-limit saturation

| Subject TU | Checked transactions | Transient cap hits | Admission cap hits | Chain ceiling hit |
| --- | ---: | ---: | ---: | --- |
| P0 | 1 | 0 | 0 | No |
| zlib deflate | 2 | 0 | 0 | No |
| zlib inflate | 2 | 0 | 0 | No |
| zstd compress | 2 | 0 | 0 | No |
| zstd decompress | 3 | 0 | 0 | No |
| cJSON | 2 | 0 | 0 | No |

These twelve transactions include terminal no-op certificates. The defaults did
not bind on this population. Therefore zero-weight rejection is **not demonstrated
to reserve scarce certificate quota here**, and the proposed budget-competition
effect is not an observed explanation. This does not prove that caps never bind
on other programs. EVP fuel is scoped per region/rule/epoch; actual matcher visits
remain **NOT_MEASURED**, not inferred from fuel, accepted regions or chain count.

## Separate opportunity ceilings

| Track | Bounded observation | Verdict and consequence |
| --- | --- | --- |
| Policy | 512 feasible selected-pair gate signatures; 16 full-suite evaluations of the first eight signatures at schedules 1/8; eight machine-code classes; no newly consulted pairs | INCONCLUSIVE: complete Boolean signature census is not exhaustive native utility evaluation of all 1,024 signature/schedule combinations |
| Rule | Two syntax cells complete: 162 + 5,832 visits; 123 + 4,649 counterexamples; 1,222 fuzz survivors; two universal fixed-template controls proved; 1,220 templates unattempted | INCONCLUSIVE: no bounded new-rule installation or useful new-rule proof; controls do not establish a discovery ceiling |
| Shortcut | 1,480 verified baseline regions, 13 multi-rewrite regions, 11 composition kinds | INCONCLUSIVE: before-witness premises retained; neither matcher cost nor fuel/proof savings measured |

The policy changes accept/reject **already selected** root/replacement pairs.
They cannot request another e-class representative such as shl instead of add.
The rule grammar is ordered one-variable trees, constants 0/1, six operators and
a leaf RHS, with one/two-operator LHS cells. Its syntactic completion is not
semantic completeness or coverage of the whole P0 rule space. Fuzz only filters.
The two proofs are trusted universal `RuleCovered` simp controls, **not new rules
installed into producer/checker**. No change to the registry, guard forms or EqSat
allowlist is smuggled in by the census.

Pinned LLVM19 contributes 335 bounded patterns and 34 reduced-node opportunity
hints. Only initialized bits, no overflow flags and constant in-range shifts are
mapped. LLVM poison/undef/shift semantics do not supply EvoIR equations. The
unresolved source/grammar/depth and applicability information is retained in the
raw preflight. There are zero full-suite new-rule utility evaluations and no
campaign-scale exhausted grammar cell; this cannot justify discovery-SEARCHABLE.

The planned whole-track AI budget is 16 **suite evaluation units**, not 16 syntax
visits. The preflight preserves the separate units and enumeration-floor rule.
Completed syntax cells, resource stops and unattempted proofs cannot authorize
AI search by themselves. All three tracks require new qualified opportunity
evidence before their corresponding R03 campaign.

## Code size, materialization and timing

Five changed-IR subject TUs materialize to different allocatable/relocation
machine sections in both profiles: **10/10 changed TU/profile pairs**. P0 changes
neither IR nor machine code. The linked ELF `.text` sizes below include possible
alignment/literal pools; instruction/padding/pool decomposition is NOT_MEASURED.

| Case | Profile | A `.text` bytes | B `.text` bytes | A/B stripped file bytes |
| --- | --- | ---: | ---: | ---: |
| P0 | checked / unchecked | 424 | 424 | 67,544 / 67,544 |
| zlib | checked | 77,996 | 77,516 | 133,216 / 133,216 |
| zlib | unchecked | 74,764 | 74,252 | 133,216 / 133,216 |
| zstd | checked | 525,988 | 524,548 | 592,112 / 592,112 |
| zstd | unchecked | 523,364 | 521,924 | 592,112 / 592,112 |
| cJSON | checked | 30,844 | 30,112 | 67,680 / 67,680 |
| cJSON | unchecked | 30,732 | 30,000 | 67,680 / 67,680 |

Thus there are actual code-section reductions; total stripped file sizes stay
unchanged due to layout. Neither result proves latency or memory improvement.

Timing is fresh-process spawn-to-stdio-drain **inside the guest**. Controller/VM
launch costs are excluded. The frozen protocol has 12 balanced pairs per
comparison, one warmup per label, seed 20260930, no retries or sample clipping,
5% effect/precision floors and Bonferroni simultaneous intervals over all 24
comparisons. There are twelve A/A and twelve A/B comparisons, independently
recomputed from the retained raw samples.

Only **2/12 A/A** comparisons satisfy the effect floor (zlib decompression, both
profiles). **12/12 A/B are INCONCLUSIVE**. No speedup, equivalence, regression
absence or sufficiently low-noise campaign admission follows. P0's unchanged
code is especially useful as a negative control. Future changes to workload
duration, affinity, sampling or budgets must be preregistered as a **new** run;
they cannot repair this capture retrospectively.

CPU and peak RSS are retained from wait4, with native units explicitly converted.
Peak RSS may contain approximately 20 MiB inherited fork/preexec collector state;
it is not steady-state application heap. Compile/check/codegen costs come from a
host **debug** compiler and are diagnostics, not a fair release-GCC compilation
comparison. No memory-footprint or compilation-speed benefit is claimed.

## Failures found, reduced and repaired

1. **Regional EVP fuel**, `3bb0666a`: the first zstd pilot reported 815 firings
   against a 256 limit because the producer accumulated independent regions.
   The checker correctly rejected it. A 300-region RED now reports the correct
   maximum of one firing per region; standalone replay and interpreter checks
   pass. The checker/limit were not relaxed. The original zstd IR then replays
   correctly under the same input/output identity.
2. **Historical sealed fixtures**, `52ce286c`: the full workspace caught a
   byte-exact test regression after that telemetry correction. All historical
   fixture files remain unchanged. Only three pinned old proof hashes admit
   independently recomputed global-to-regional fuel normalization; every other
   byte remains exact, and both old and corrected proofs are checked.
3. **Collector scratch race and failed-preflight admission**, `b27e5e23`: a
   checker deleted a scratch file between the collector's existence/stat checks.
   The collector now tolerates only concurrent FileNotFound deletion; missing
   capture roots, permission/I/O failures and storage overruns still fail closed.
   Failed/missing policy evaluations now block timing. Deterministic file and
   directory race REDs, root/permission negatives and admission tests pass.

The failed captures `pilot-wdm6xhwh` and `pilot-cm5sr2ke`, their frozen manifests
and the failed workspace log are retained. The latter run was interrupted and
cleaned up; its partial timing data is **not** in the successful measurement.
The successful run was frozen afterward with 16 instead of 32 suite evaluations
to fit the declared storage cap. Source, result assertions and timing protocol
were unchanged; no native observations were used to select a faster policy.

## Gates and reproduction

- Full workspace: **366 suites, 2,861 passed tests, zero failed, four historical
  ignored campaigns**; all-target Clippy with warnings denied and fmt pass.
  Those four ignored campaigns remain NOT_RUN. No Rust source changed after
  this gate; later changes were collector fixes, the new manifest and reporting.
- Full ladder Python discovery: **450 tests, 449 passed, one skipped real-capture
  test**. The earlier collector subset had 154 tests (153 passed, one skipped).
  The real-capture test was run explicitly: **6/6 audit tests passed**, including
  the full retained capture and ten read-only mutations. These scopes overlap;
  do not add them as unique test counts.
- Lean fixed-template controls: **2/2 universal proofs**. This is not the new-rule
  registry integration or a fresh whole-compiler Lean proof.
- Independent raw audit: **CAPTURE_CONSISTENT**, including all sixteen policy
  evaluations. Its top-level twelve certificate/twin counts cover the baseline
  only; totals 138/108 above include independently audited policy evaluations.

```sh
# Read-only raw audit. It replays proofs, not timing; it needs retained artifacts.
python3 -m tools.ladder.eqsat_pilot_audit \
  .evidence/eqsat-pilot/pilot-xb4aobn_ \
  --manifest-sha256 9d21b5838ed38129248bdd898b8bf70e327680349068fb8f13d66f9e84740cc2

# Explicit real-capture negative gate; no evidence is edited.
EVOCC_R02_CAPTURE="$PWD/.evidence/eqsat-pilot/pilot-xb4aobn_" \
EVOCC_R02_MANIFEST_SHA256=9d21b5838ed38129248bdd898b8bf70e327680349068fb8f13d66f9e84740cc2 \
  python3 -m unittest tools.ladder.tests.test_eqsat_pilot_audit

# Full compiler gate, not a selected-crate substitute.
CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true \
  tools/gate-changed.sh --full-workspace --offline

# New execution requires the exact frozen inputs/tools and sufficient headroom.
python3 tools/bench.py --eqsat-pilot \
  tests/contracts/eqsat-linux-pilot-20260930-scratch-fix.json
```

The successful capture occupies approximately **640 MiB**, below its 1 GiB cap.
Its owned VM stopped/auto-removed with exit 0. No test remains active because a
receipt says OBSERVED. Raw samples/proofs are kept once, not embedded in Git.

## Next gates — explicitly not completed

R01/G2 still requires whole-kernel EqSat builds, equal Image hashes and every
optimized TU's certificate. R02c still requires a fair, frozen native comparison
against declared GCC/Clang profiles with adequate calibration, release build
costs and explicit coverage. R03 still requires an admitted useful search space,
candidate integration and held-out evidence. The pilot does not close these,
the full kernel/application roadmap, all-library performance, generic
cost-aware extraction, guard discharge or new-rule/shortcut benefit.
