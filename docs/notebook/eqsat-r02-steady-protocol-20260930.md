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
Canonical signature representatives need not contain that exact weight vector:
the current finite census's positive representative has `mul=2`, while the
compiler default has `mul=1`. The build therefore replays the exact fixed default
on A's canonical IR with the pinned census adapter and independent checker.
It neither substitutes a representative nor chooses a census winner. The plan's
`optimizer` field fixes all thirteen weights, registry, extractor, counter
mapping and all three schedule caps. A's backend objects are reused by verified
IR digest and UB profile; B's two profiles are emitted from that newly verified
default chain. The build auditor replays B and reconstructs its backend and
assembler commands before accepting the resulting binaries.
Both profiles bind the same IR, chain record and bundle. A freshly GCC-built,
non-LTO harness invokes the unchanged libraries. Reference-composition result
checks precede EvoCC timing. Sources, headers, objects, proofs, executables,
tools, image, collectors and exact command recipes are pinned in committed
build, A/A and A/B manifests. The historical source-composition manifest is also
pinned: its recipe is consumed even when source objects are reused. The build
plan contains only already-existing A inputs, not paths to future B bundles.

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

The default-control integration gate adds isolated REDs for the missing exact
default and for a build-plan `KeyError` that expected B before B existed.
The producer owns creation of each new B bundle, matching the Rust writer's
new-directory contract. Focused steady tests pass 16/16. Full ladder discovery
passes 486 tests, with zero failures and nine optional real-capture tests skipped
(495 total), `python-steady-default-plan-final.log`, SHA-256
`6f11d9ac2338476528955210b5f1468d383525058f01736f0bae1f2fbf493adc`.
Those results establish collector/planning behavior, not real build or timing
qualification. The running deterministic capture and its pins remain unchanged.

The baseline RED now constructs and binds real identity-bundle bytes instead
of mocking that boundary. The five original library A bundles are EqSat-off
identity chains: no checker or EVP proof is asserted for A. The repaired
steady/twin tests pass 19/19; full ladder discovery passes 486 with zero failures
and nine optional capture tests skipped (495 total),
`python-steady-identity-baseline-final.log`, SHA-256
`6a6ad7e8e04b35d84c2d97dbc0d72487b1c98c3e2b3e507ba1b9522419e81bca`.

## Real build and admitted A/A (later September 30 checkpoint)

The earlier NOT_RUN statements above describe their historical checkpoint.
Build `build-9t8d1etw` is independently accepted: three fixed mixed-link library
compositions and five reference-result modes, with fresh exact-default B chains.
A/A `aa-f9dcv13a` completed once: 312 timed samples, ten admitted comparisons,
guest CPU 0, unchanged 5% effect/precision gates. Selected pair counts are
12/12 for each zlib mode, 24/12 for each zstd mode and 24/12 for cJSON
(checked/unchecked). Repeats are 7,168 / 16,896 for zlib compress/decompress,
33,024 / 222,976 for zstd and 762,624 for cJSON. Timed samples span
1.195674709--2.627673418 seconds; startup is excluded.

The first Python 3.9 audit is retained as a failure: the exact diagnostic
`sample_cv` equality differs from Python 3.13 by one final-bit rounding
(0.008000266907678558 versus 0.008000266907678556). No interval or qualification
gate differs. No frozen source, sample, manifest, comparison or threshold was
changed. Read-only replay uses the **unchanged** `audit_capture` in its pinned
Python 3.9 for outer artifact/command/manifest checks and delegates its full
**unchanged** `audit_guest` to Python 3.13 for raw guest reconstruction. All raw
guest streams are also read/pinned by the outer checker; both runtime identities
are recorded, and the helper binary is checked before/after execution. This is
a split-runtime audit, not a passing claim about the original Python 3.9-only
CLI. It changes no audit logic and adds no audit tool or retry.

- A/A manifest SHA-256:
  `58c37ca9d24c7e7d02c079d52dbdb17c364909119c959d85d869111fdef66d38`.
- A/A accepted `audit.json` SHA-256:
  `9b73506caa92bc07e8f83ef315c6e161b832dbcc928445216d441014502d09af`.
- Accepted split-runtime stdout:
  `.evidence/eqsat-r02-review-20260930/steady-aa-split-audit-20260930.stdout`,
  SHA-256 `a46cfa8d773eca8080db53bc4567214d7f79803cc287d69099aa993ad573c7fa`.
- Original failed audit stderr SHA-256:
  `9436459220f7dbaffc92b6cc7605bb496e29c9434daa6b148e811193cd9f0213`.

The separately committed [A/B plan](../../tests/contracts/eqsat-steady-ab-20260930.json)
has SHA-256 `84a5dc34affb06189e29c558b5a444600aaaef9ea8794fa30ce08585ab5b7713`.
It pins 1,120 prerequisite artifacts and the same all-one eight-transaction
control. Execute it once, with zero retries and no native-result selection.
Freeze its result even if it is inconclusive or a regression. No new census,
auditor framework or R03 search follows automatically.

## Frozen single A/B result

The committed A/B plan was executed **once** after A/A admission. Capture
`ab-01ws6896` completed all ten comparisons, 312 timed samples and ten prior
checked admissions, then cleaned up its owned VM with exit code zero. All
322 child executions passed the typed result contract. The independently
reconstructed report is STEADY_CAPTURE_CONSISTENT, not a performance PASS.
Timed samples span 1.214653876--4.258964502 seconds. Both admitted native
profiles bind the same verified IR/chain/bundle; CPU affinity, hot-loop clock,
quiet host collection, chosen repeats/pair counts and the 5% gates are unchanged.

The auditor uses the same unchanged split-runtime checks documented above.
Its Python 3.13 helper must exactly match the runtime identity in the frozen
A/A audit; the pinned Python 3.9 outer checker verifies all original manifest,
artifact, command and cleanup identities. No auditor source was modified.

| Workload | Profile | Pairs | Simultaneous median-delta CI (% of A median) | Precision gate | Frozen outcome |
| --- | --- | ---: | ---: | --- | --- |
| zlib compress | checked | 12 | -4.01 to +5.34 | Met | INCONCLUSIVE |
| zlib compress | unchecked | 12 | -5.80 to +21.43 | Not met | INCONCLUSIVE |
| zlib decompress | checked | 12 | -78.11 to +70.33 | Not met | INCONCLUSIVE |
| zlib decompress | unchecked | 12 | -19.18 to +24.91 | Not met | INCONCLUSIVE |
| zstd compress | checked | 24 | -10.03 to +15.44 | Not met | INCONCLUSIVE |
| zstd compress | unchecked | 12 | -23.35 to +27.03 | Not met | INCONCLUSIVE |
| zstd decompress | checked | 24 | -20.91 to +4.89 | Not met | INCONCLUSIVE |
| zstd decompress | unchecked | 12 | -14.97 to +10.16 | Not met | INCONCLUSIVE |
| cJSON parse/print | checked | 24 | -10.05 to +13.85 | Not met | INCONCLUSIVE |
| cJSON parse/print | unchecked | 12 | -9.67 to +10.33 | Not met | INCONCLUSIVE |

Negative delta means B is faster. Rounding in this table is display-only;
raw gates use the unrounded captured values and simultaneous ten-comparison
intervals. Even zlib checked compression's sufficiently narrow interval crosses
the +5% boundary, so it cannot claim no material effect. The other nine fail
precision. No comparison establishes material improvement, regression or
absence of a material effect. A/A admission does not guarantee stability of
the subsequent A/B window; host affinity/interference remains uncontrolled.
This is retained uncertainty, not a new diagnosis of a compiler bug, and does
not justify retuning or another run in this study.

The exact default B still reduces linked `.text` relative to A:
zlib 480/512 bytes, zstd 1,440/1,440 bytes and cJSON 736/736 bytes
(checked/unchecked). These are the **fresh steady-harness composition** sizes,
including layout effects, not edits to the earlier pilot's 732-byte cJSON
observation. Code-size benefit is distinct from the inconclusive latency result.

- Manifest SHA-256:
  `84a5dc34affb06189e29c558b5a444600aaaef9ea8794fa30ce08585ab5b7713`;
  preregistration commit `4e25506f`.
- Outer receipt SHA-256:
  `cbed2b185b4c9c2a2f349fca8f4d4ab10c9f2e15d995255969e8ad2f9cf9241f`.
- Raw guest receipt SHA-256:
  `a5f9a45e32244512aa5c927e5676423afdb7ac4b0d719b259075b7fd85244e07`.
- Accepted `audit.json` SHA-256:
  `677cfc09e4d17d46880ade3b1cf4932fc6b5d3b262345dad78544d16ff217b41`.
- Independent audit stdout:
  `.evidence/eqsat-r02-review-20260930/steady-ab-split-audit-20260930.stdout`,
  SHA-256 `cbcfc78f3949ec01bce35f6e95f10a244e3668c4f9a65b8d15ca2c05a69c5b8c`;
  stderr is empty. All captures remain retained locally, not duplicated in Git.

**Stop:** R02a1's bounded build/calibration/one-run measurement task is complete.
Its frozen latency outcome is 10/10 INCONCLUSIVE; final R02/Q2/G2 and R03's
positive improvement milestone remain NOT_ACCEPTED/NOT_ACHIEVED. There is no
retry, larger census, new AI campaign or audit-tool expansion. The
[three-track adjudication and owner decisions](eqsat-r02-review-repair-20260930.md#formal-three-track-adjudication-and-stop-decision)
are the next handoff, not another engineering phase.
