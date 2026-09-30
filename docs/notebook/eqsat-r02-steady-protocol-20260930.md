# Linux steady-state timing protocol

This is the R02a1 replacement for the unqualified millisecond fresh-process
timing pilot. It extends `tools/bench.py` and the existing bounded capture and
native-twin pipeline; it is not a new benchmark framework or an AI campaign.
Implementation/collector tests are distinct from real timing qualification.

## Population and controls

The frozen population is zlib compress/decompress, zstd compress/decompress and
cJSON parse/print, each in checked and unchecked profiles: ten comparisons.
The selected EvoCC translation units and GCC-built remaining library objects
are unchanged from the six-TU pilot. P0 remains a no-change pipeline control; it
is deliberately not a performance workload in this protocol. Claims are about
these mixed-link compositions, not full libraries, bare metal or GCC parity.

A is the original EqSat-off verified identity chain. B is the all-one-weight,
eight-transaction default policy, not a policy selected by native timings.
The release backend objects are reused by verified IR digest and UB profile.
Both profiles bind the same IR, chain record and bundle. A freshly GCC-built,
non-LTO harness invokes the unchanged libraries. Reference-composition result
checks precede EvoCC timing. Sources, headers, objects, proofs, executables,
tools, image, collectors and exact command recipes are pinned in committed
build, A/A and A/B manifests.

## Clock and isolation

Each harness process prepares inputs, warms up `iterations / 32 + 1` times,
then repeats library operations inside a `CLOCK_MONOTONIC_RAW` interval.
Process startup, input preparation, final byte/result validation and output are
outside that interval. Error/length checks that belong to the operation remain
inside it. Each process still validates its result; elapsed time is not inferred
from host/container RPC duration or child spawn-to-drain telemetry.

The guest collector and its children are pinned to guest CPU 0. The hardware VM
has two vCPUs and 3 GiB memory. Host affinity and other-host interference remain
uncontrolled and explicitly reported. One benchmark owns the existing lock.
No other heavy job is started during calibration or A/B measurement.

One host command captures the complete timing batch in quiet mode. Host tree
scans, artifact hashes and disk observations occur before/after that batch, not
while the C interval runs. The guest also performs retention checks outside
each child interval. Deadline checks and bounded output pipes remain active.
The registered artifact envelope is 64 MiB guest capture and 256 MiB host
capture, with an 8 GiB free-space reserve. Each child has a 15-second deadline;
the batch/owned VM has a two-hour cap. No retries erase noisy or failed runs.

## A-only duration and A/A calibration

Only A is executed during A/A. Initial repeats are 16 for zlib, 256 for zstd and
8,192 for cJSON. At most four A-only probe rounds select a shared repeat count
for both UB profiles, targeting two seconds with upward integer quantization
and a 4,194,304 repeat cap. Each measured sample must span 1–8 seconds. A shorter
or excessively interrupted observation is retained but cannot qualify timing.
No B duration or outcome chooses repeats or the number of pairs.

The predeclared A/A looks are 12, 24 and 48 pairs. Every prefix is balanced with
seed 20260930 plus its comparison index. A look admits its comparison only when
the simultaneous median interval lies within the 5% effect floor and its
half-width meets the 5% precision target. Ten comparisons times three looks
are included in the conservative simultaneous interval. Stop at the first
admitted look; otherwise retain `INCONCLUSIVE_AA`. All ten must qualify before
any A/B execution. A failed calibration is not permission to retune using B.

## Separately frozen A/B

After independent A/A audit, a new committed A/B manifest pins the calibration
capture and its chosen repeats/pair counts. A/B uses the same frozen harness,
inputs, binaries, profiles and per-comparison counts, with separately balanced
seeded order. The final intervals cover the ten-comparison family.

Every unchecked probe or sample has a prior checked pass at identical input
and repeat count, bound to the same verified input chain. The native admission
uses a typed harness-result predicate, not exit-zero alone. Audit reconstructs
raw C elapsed values, output hashes, input/native identities, ordering, checked
admissions, stopping rules, intervals and qualification independently from the
receipt's labels. Raw attempt indices must follow the frozen chronology,
including duration probes, checked admissions and every ordered sample.
Rearranging attempts cannot be legitimized by internally consistent pairs or
statistics. CPU affinity is compared with its canonical JSON type; boolean
`false` is not accepted as CPU number zero. Mutation tests cover CPU mask, quiet mode, incorrect inputs,
duration/value substitution, statistics, ordering and missing admissions.

Report improvement/regression only when the declared interval clears the 5%
floor with adequate precision. Otherwise retain no-material-effect or
inconclusive results as appropriate. PMU counters are `NOT_MEASURED`. This is a
Linux hardware-VM pilot, not final Q2, Q3 or a cross-family performance claim.

## Execution status

The runner and synthetic-record mutation tests are implemented. The full Python
collector gate passes: 475 passed, 0 failed, 1 skipped (`python-steady-final.log`,
SHA-256 `72b36666ceaa4ad919d7e55db4e2fdd0e5a8291ec3c762bd2ee414e973ed27e3`).
The seven new test methods include independently constructed A/A and A/B records
and eleven one-field negative mutations; they are not actual timing samples.
The later input-inventory and raw-chronology gates pass 478 tests, with zero
failures and nine optional capture tests skipped (487 total;
`python-chronology-unsandboxed.log`, SHA-256
`5fad7bdbb4b2fc1e678623ac95d55f17e4983a8f9d396bca0c22201944f83d62`).
The ten steady-specific methods pass, including a RED that previously accepted
swapped raw A/A execution order (`steady-chronology-red.log`, SHA-256
`4fbac31de98f395e199c895e060bff176db0be851e41e92c100af980de1f6afb`).
The sandboxed gate's macOS `/bin/ps` permission error is retained separately;
it is an infrastructure failure, not counted as a compiler defect or PASS.
The additional boolean-affinity RED is retained in
`steady-boolean-affinity-red.log`; steady/completion focused tests pass 15/15
in `steady-completion-typed-final.log`.
Real build,
Linux A/A and A/B are **NOT RUN** at this checkpoint. R02b1 deterministic
completion must finish and pass independent audit first; the historical timing
capture remains immutable and cannot substitute for this admission.
