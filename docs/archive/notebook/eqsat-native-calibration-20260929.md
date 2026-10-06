# R02a bounded native A/A pipeline

Date: 2026-09-29. This extends the
[measurement foundations](eqsat-r02a-foundations-20260929.md), not the Q2/Q3
claim ceiling. R02a remains OPEN. The initial implementation is a Darwin arm64
P0 pipeline check; it is not Linux performance, an application campaign, an
EqSat speedup or a release benchmark.

## Exact-input backend adapter

`evo-ir-asm`, built by the existing `evocc-mir` package, accepts:

```text
evo-ir-asm (darwin-arm64|linux-aarch64) SHA256 INPUT.evo
```

It reads one snapshot, checks the exact digest, parses and validates the module,
requires canonical text, and calls the existing checked PIC MIR backend and
allocation verifier. It does not run C lowering, EqSat or a second ABI path.
Malformed/noncanonical input, a wrong digest or unsupported target emits no
assembly. A digest match is transport binding, not proof admission: the caller
must check the identity or verified chain before invoking it. No unchecked
profile is exposed by this adapter.

The full C driver still targets Linux. The initial native wrapper explicitly
records `linux-lp64-header-free-integer` source semantics and Darwin arm64
code generation. It permits only two reviewed, header-free integer fixtures,
no foreign dependencies, no arguments and empty output with exit zero. This
bounded overlap does not establish a general Darwin C frontend or Linux-native
execution. Linux code generation in the thin adapter is tested as assembly
emission, not misreported as Linux execution on this host.

## Frozen native plan and publication

`evocc-performance-plan-v2` adds an `ir_backend` executable pin, the collector's
Python executable and five source-file hashes, and a closed `native` recipe to
the earlier read-only plan. V1 remains readable but cannot
be executed. The bench entry point is:

```sh
python3 tools/bench.py --eqsat-calibrate tests/contracts/NAME.json
```

The plan must be committed before execution. It binds exact source/spec/tool
hashes, source revision, source/backend profiles, effective recipe argv, host
identity, closed environment, result predicate, sample order/seed and resource
bounds. Only native Darwin arm64, `-O0`, EqSat off, checked PIC lowering and a
single subject object are implemented here. Modified argv, optimization,
wrapping/FP flags, new source fixtures, held-out inputs, uncontrolled injected
environment variables and unsupported profiles are refused. A/A labels run the
same executable; they do not select different compilers or rerun optimization.

The `darwin-p0-aa-v2` recipe additionally freezes the SDK's `SDKSettings.json`
and `usr/lib/libSystem.tbd`, supplies the corresponding explicit `SDKROOT` in
the closed child environment, and pins/selects the real link editor with
`-fuse-ld=ABSOLUTE_PATH`. This avoids relying on the invoking shell's SDK setup.
Both subject and reference links use that same frozen selection. These pins
do not attest the entire SDK or every transitive system-library stub.

Execution uses the existing `.build/bench.lock`, one new exclusive evidence
directory and no deletion of previous runs. The exact plan is retained. Each
attempt is published as STARTED before launch and is updated even on failure.
The wrapper performs:

```text
C-HIR and EvoIR source execution (expected full return value 0)
 -> one canonical EvoIR export + explicit EqSat-off identity chain
 -> independent byte-identity audit
 -> exact-digest MIR assembly -> subject object -> subject executable
 -> reference build/result + checked subject result admission
 -> retained warmups -> balanced paired A/A samples
 -> final source/artifact identity check -> OBSERVED_AA
```

Every timed execution checks its own exit status and complete stdout/stderr
digests. The source, tools, chain, assembly, object and exact timed executable
are checked for changes outside each measured interval. A changed artifact or
wrong result stops the run without publishing an eligible measurement. The
receipt retains earlier attempts and any partial sample prefix, but its global
FAILED/INTERRUPTED status prevents promotion of that prefix. Replays append new
owned directories, never overwrite history. There are no automatic retries.

The reference object/executable is separate and never substitutes for the
subject object. The receipt records all actual commands and the emitted
IR/chain/assembly/object/executable digests. EqSat-off identity is labeled
`IDENTITY_ONLY_NO_EVP_REWRITE`, not counted as a rewrite proof success.

The trusted boundary still includes the selected compiler, backend and system
toolchain. Hash checks are not reproducible-build attestations or a proof of
machine semantics. This is not a hermetic linker/SDK or hostile-code sandbox.

### Read-only capture audit

`python3 -m tools.ladder.eqsat_native_audit DIRECTORY --manifest-sha256 SHA256`
checks a completed capture against an independently supplied frozen-plan digest.
It launches no child, writes no receipt and never retries an attempt. It checks
the identity chain and actual codegen artifacts, exact command sequence,
retained stdout/stderr bytes, every result predicate, the full warmup/sample
population, deterministic pair ordering and statistics recomputed from attempt
metrics. It rereads observed files before returning. Changed artifacts, missing
or extra attempts, forged sample indices, invalid metrics and modified summary
statistics fail closed. Failed/interrupted captures cannot become successful
by retaining an earlier sample prefix.

Its positive result is `CAPTURE_CONSISTENT`, not a new run or performance PASS.
The recorded times, exit statuses and host observations remain trusted collector
telemetry. This audit neither independently observes historical process execution
nor reattests compiler build provenance. The underlying measurement implementation
and frozen plan are unchanged by this separate reader.

## Measurement and resource boundaries

- Clock: `monotonic_ns`, from immediately before process launch through child
  exit and captured stream draining; process startup and capture supervision
  are included. File hashing, result comparisons and receipt writes are outside
  the successful interval. This is not steady-state kernel time.
- The existing bounded pipe capture controls combined stdout/stderr bytes,
  watchdog and free-space reserve. No inherited `RLIMIT_FSIZE` can truncate
  checker scratch proofs. An additional owned-directory size watcher bounds
  retained work files; it is polling/backpressure, not a filesystem hard quota.
- On failure, the owned process group is killed and the leader reaped. A
  same-group descendant surviving an otherwise completed leader is a failed
  attempt, not successful completion. Detached hostile processes are outside
  this registered-fixture runner's isolation claim.
- Planned process count times per-child deadline cannot exceed one hour.
  Runtime is one job; output/retention/reserve caps remain separately recorded.
  Another cooperating benchmark owning the common lock prevents launch.
- Every pair runs one A0 and one A1. The frozen seed shuffles an exactly balanced
  set of A0-first/A1-first pairs; odd pair counts are rejected. Warmups are
  retained but excluded from statistics. Slow samples are never discarded.
- Raw samples bind attempt indices and one executable digest. Statistics include
  paired median A1-minus-A0 nanoseconds, median absolute deviation and a
  distribution-free 95% median interval from binomial order-statistic ranks.
  That nominal coverage assumes independent, identically distributed paired
  differences; uncontrolled host interference may violate the assumption.
  Small populations without a finite 95% interval report null, not invented
  precision. There is no speedup or promotion verdict.

The host is recorded, not controlled: affinity, cache and thermal/interference
conditions are not established. CPU/RSS, executable-section decomposition,
retired instructions and steady-state throughput are NOT_MEASURED. Short P0
processes primarily exercise startup/supervision. These limits prevent turning
this infrastructure check into application performance evidence.

## Validation checkpoint

The executable plan is
[`eqsat-native-aa-20260929.json`](../../../tests/contracts/eqsat-native-aa-20260929.json),
committed in `935eb09` before any timing. Its implementation checkpoint is
`252c5152ae53b9dccc0f041df0f84e5f6dd4df53`. The read-only audit checked 13
distinct artifact paths; the exact manifest SHA-256 is
`fafdab8c50f19c1980c1cb79ed95cfb88b5b68bb9d00d5553bee9bfd11dab043`.

| Frozen item | Value |
| --- | --- |
| Population | One header-free unsigned integer-mix fixture; 1,000 loop iterations |
| Host | Darwin 25.6.0 arm64, Apple M2 Max; uncontrolled affinity/interference |
| Timed population | 12 balanced pairs, 24 executions of one subject executable |
| Warmup | Two per label, four retained executions excluded from estimates |
| Order / retry | Seed 20260929; no automatic retries |
| Resource envelope | One job; 10 seconds per child; 4 MiB combined captured output per attempt; 64 MiB retained work; 8 GiB free-space reserve |
| Expected attempts | 10 build/admission commands + 4 warmups + 24 timed executions = 38 |

Source/host identity and the entire plan are rechecked immediately before the
run. The full workspace gate precedes calibration so its build/test workload is
not an intentional contaminant. Other host activity is not controlled. Passing
the plan audit itself is not evidence that any native sample has run.

REDs were added before the missing MIR binary and Python capture implementation.
The owned-descendant test also failed against the previous capture helper before
its repair. The initial focused Python integration passed 146 tests, including
real child timeout/overflow/file-growth, failed output, artifact self-mutation,
lock exclusion, unique retained runs, paired order and statistical boundaries.
Fixture-tool tests are harness tests, not native/compiler acceptance.

The full workspace gate completed: 2,855 PASS, 0 FAIL and four existing ignored
campaigns, over 364 test-result summaries; workspace all-target Clippy and format
checking both passed. No Rust build input changed in the subsequent SDK-recipe
repair. The gate log SHA-256 is
`271d0f21af3e8e62700648a6df49d654010d00cedc4c75ea99a250e118873bfc`.

The first actual native attempt, `.evidence/eqsat-native/aa-8te9uzww`, is FAILED
and retained, not erased or retried automatically. Both interpreters returned
zero; canonical IR export, identity audit, backend emission and assembly passed.
Attempt 5, the subject link, exited 1 with `ld: library 'System' not found`:
the closed environment had removed ambient SDK discovery. No subject execution,
warmup or timing sample ran. The failed receipt SHA-256 is
`fe57f6ba2e70283e6c16dab521d44ca24fc4fa12fe176cea39d61d6045903625`.

The repair is the explicit SDK/link-editor recipe above, not looser environment
inheritance or a compiler-semantic change. Its unit RED requires the actual
SDKROOT environment and explicit link-editor argument; negative cases reject
missing/mismatched SDK and linker pins. A separate plan must be frozen before
the repaired run. That subsequent run is recorded below. No application census
or AI/model search has been launched.

## SDK-bound native observation

The repaired plan,
[`eqsat-native-aa-sdk-20260929.json`](../../../tests/contracts/eqsat-native-aa-sdk-20260929.json),
was committed as `7210cac4` before execution. It names implementation
`db5c4cc67bbec2a5459eb0e522754261f36b7f10`, preserves the original source,
protocol and resource bounds, and adds explicit SDK/link-editor selection.
Its 16 declared artifact identities were checked before launch. Plan SHA-256:
`55a8b266d6bf2b4ea07739272415fd6af744eb5141b82adb49b5d11045803915`.

The new capture is `.evidence/eqsat-native/aa-0kqzmxwm`; it does not overwrite
`aa-8te9uzww` and is an explicit repair rerun, not an automatic retry. Its
receipt SHA-256 is
`3a0759b185f275383de2f504617a7e84bd6049c9c078fec92b01c7b4deef6550`.

| Check | Observed result |
| --- | --- |
| Collector outcome | `OBSERVED_AA`, not performance PASS |
| Attempt population | 38/38 completed: six build/export commands and 32 result-checked executions, including two interpreters |
| Native sample population | 24/24 timed runs, four retained warmups, one checked subject admission and one checked reference execution |
| Correctness | Every admitted execution returned its exact expected result and output hashes |
| Independent capture audit | `CAPTURE_CONSISTENT`; exact argv, files, chain, population, order and recomputed statistics |
| Proof scope | One EqSat-off identity audit; zero EVP rewrite proof successes |
| Retained work | 81,070 logical bytes, below the 64 MiB envelope |
| Performance, Linux and application claims | `NOT_CLAIMED` / `NOT_RUN` |

IR digest: `fca2714408b2445443a0504e8c22f2fac1088d0cd63bb61848d194c1c6fcb163`.
Assembly digest: `757939d8b37d5e6348fad85507a2019f7f25befbcfef329261a4c7f9596fa6c1`.
Object digest: `623e7265b033f3e87acff351fb9658a57c9cdb1f512bb0a54d984e284351beb0`.
Timed executable digest: `eaf6465e082a8caab2be7af4d245f05c71b0807e8c2ef7a4e3a677dfffea0b8a`.

The raw paired process-wall samples, in nanoseconds, are retained here as well
as in the receipt. No slow sample was removed:

```text
pair       A0       A1
   0  5532417  4826834
   1  5249417  5816791
   2  4139958  4489584
   3  4570292  5026792
   4  4139583  4573125
   5  4325500  3855917
   6  4095084  4066166
   7  4068792  4352250
   8  4195917  4155208
   9  4059750  3915167
  10  4413875  4699791
  11  5325084  4500500
```

The paired median A1-minus-A0 difference is 127,270 ns, MAD 289,062.5 ns,
with nominal 95% interval [-469,583, 433,542] ns. A0 and A1 label medians
are 4,260,708.5 and 4,495,042 ns. The interval includes zero, but this is not
proof of equivalence or adequate resolution for optimizer effects: these are
short fresh processes on an uncontrolled host, dominated by startup/supervision.
No workload tuning, A/B comparison, candidate selection or promotion followed.

After the SDK repair the Python integration gate passed 148 tests. Its log
SHA-256 is `83df2cbfb774f0c45e091e2e7e5d37fc2130e45d366e2462421c25eef3d9af34`.
The Rust gate above covers the same unchanged Rust inputs and binaries. The
combined commands were:

```sh
CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true \
  tools/gate-changed.sh --full-workspace --offline
python3 -m unittest \
  tools.ladder.tests.test_eqsat_chain tools.ladder.tests.test_eqsat_census \
  tools.ladder.tests.test_eqsat_manifest tools.ladder.tests.test_eqsat_native \
  tools.ladder.tests.test_bench tools.ladder.tests.test_todo_schedule \
  tools.ladder.tests.test_gate_changed
python3 tools/bench.py --eqsat-calibrate tests/contracts/eqsat-native-aa-sdk-20260929.json
python3 -m tools.ladder.eqsat_native_audit .evidence/eqsat-native/aa-0kqzmxwm \
  --manifest-sha256 55a8b266d6bf2b4ea07739272415fd6af744eb5141b82adb49b5d11045803915
```

R02a still needs broader effective-profile verification, optimized-chain native
admission, application builders/inputs, CPU/RSS and section metrics, the complete
measurement/rejection ledger and cross-profile binding with R02d. R02b's first
application result remains the scoped default-limit saturation table; this A/A
plumbing test is not that table or a track verdict.
