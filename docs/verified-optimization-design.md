# EvoCC Q2/Q3: verified optimization and performance architecture

Design revision 7, 2026-10-01. This is a design, not an acceptance receipt. The
historical appendix at the end keeps the August 31 discussion; its measurements,
claims and `N%` targets are not current results or authority.

**Status.** These parts of the design are implemented: the `p0.scalar.v4` and
`g0.ub.v1` rule registries, the independent certificate checker, and the
composition of checked rewrites into the proved compilation pipeline (the stage
`rewrite` of `checkPipeline`). The search tracks are not active: policy and rule
discovery, the R02e second slice, R03a–R03h, the latency lane R02c and the steady
timing lane. The verified chain, the S1 size contract and the claim rules below
stay in force. The [evidence](evidence.md#checked-eqsat-measurements) gives the
measured results and limits; [TODO.md](../TODO.md) gives the open tasks.

## 1. Authority and scope

Inspected implementation: `91fce9c8a2a2bb43620dd02e91affaec76e9b7ac`;
revision-5 source audit at `4235230` (the preceding design-only commit). The
design was written against EvoIR 1.9.115 and EvoMIR 0.54. The semantic
authorities are now the [EvoIR specification](../spec/evoir.md) (revision 2.0.0),
the [EvoMIR specification](../spec/evomir.md) (revision 1.0.0) and the
[verification contract](../spec/verification.md); where this design differs from
them, they decide. This document specifies the proposed implementation
and evaluation architecture; it does not implicitly extend an opcode, rule
registry, proof format or trust policy. Such changes require their own versioned
spec delta and tests before enabling.

| Question | Meaning | Acceptance owner |
| --- | --- | --- |
| Q0 | Can AI-assisted engineering enforce its declared contracts? | Auditable changes and truthful classification; no new quantitative claim here |
| Q1 | Does the compiler correctly compile the declared programs/configurations? | Existing semantic, ABI, application and kernel gates |
| Q2 | What code quality and compilation cost does verified EqSat deliver? | R01/G2 and R02, with correctness admission first |
| Q3 | Does AI-assisted discovery improve on fixed and non-AI search baselines? | R03, with frozen held-out evaluation |

Correctness is a prerequisite, never a term traded for performance. A sound
rewrite, a valid application, correct native lowering and useful performance
are separate obligations. More rules or fewer IR operations do not close Q2/Q3.
A measured loss or degenerate search space is a completed negative study, not
R03's positive improvement milestone. Do not keep searching until a desired
answer appears: freeze a stop decision and distinguish study completion from
product improvement. **Both policy and rule discovery** must first pass the
track-specific effect-ceiling gates in Section 5.2. Neither weight diversity nor
a new provable identity implies a useful research search space.

For a claim that AI discovers useful V1 rules beyond the completed bounded
enumerator, the intersection of post-lowering residual opportunities **outside
that enumerated grammar/depth** and rules provable by the fixed universal
templates must be nonempty. Establishing whether that intersection has witnesses
is a first research output, not an assumption. Deeper patterns are not sufficient:
they must also be expressible by current witnesses, matchable, useful and proved.
This condition concerns new-rule coverage beyond exhaustive enumeration, not a
separately declared search-efficiency claim under an incomplete evaluation budget.
The witness-containing grammar cell must also exceed the frozen preflight
enumeration budget under Section 5.2.3's stopping rule. If it is exhaustively
enumerated within that budget, classify the track as finite enumeration/search
efficiency, not beyond-enumeration discovery. Deeper syntax alone never meets
this condition, and infrastructure failure is not evidence of search difficulty.

Initial discovery uses the existing pure scalar P0 domain: `bits<1..64>`,
admitted operations, existing guards and initializedness/eligibility rules.
Pointer, memory, volatile, atomic, FP, vector, inline-assembly, terminator and
whole-CFG rewrites are outside V1. Functions containing those operations may
still contain eligible scalar regions; measure eligibility rather than infer
coverage from source types. Do not relax admission merely to improve coverage.

## 2. Existing implementation and gaps

This table records the revision-5 design-freeze baseline, not a claim that its
implementation gaps remain unchanged. The R01a implementation checkpoint below
tracks subsequent code and tests separately from Q2/Q3 acceptance.

| Component | Reuse | Required extension |
| --- | --- | --- |
| Scalar search/admission | `crates/evocc-eqopt/src/{scalar_eq,eligibility,seed_facts}.rs` | Root-level coverage and exclusion reasons; independent checker admission preserved |
| Certified persistent transaction | `crates/evocc-eqopt/src/{evp1,persistent}.rs` | Policy-aware entry point and joint rollback with each admitted policy |
| Driver | `crates/evocc/src/main.rs`, `-Oeq-scalar` | Currently takes `.module` from one default-budget transaction; export proof/report and share a verified chain runner |
| Policies | `crates/evocc-eqopt/src/policy.rs` | Validated candidate identity/config; current names and registry ceilings are compiled-in |
| Evaluation/search | `crates/evocc-policy/` | Native metrics and AI/non-AI comparisons; current search fitness is interpreter steps |
| Standalone transaction proof replay | `crates/evocc-proofcheck/` | Checker is stateless; the run wrapper must enforce chain identity and checker-before-install |
| Formal semantics/proofs | `lean/EvoIR/{Sem,Term,Fact,Rule,Registry}.lean`, `Proofs/` | Bind data-only candidates to existing universal `RuleCovered`; bounded-width admission is a separate interface change |
| Lean audit | `tools/lean/{registry_to_lean,audit,correspondence}.py` | Existing audit assumes reviewed repository modules, not arbitrary external proof programs |
| Application runner | `tools/bench.py`, `tools/ladder/` | Separate performance mode/receipts in the existing entry point, not another framework |
| Tracking | `TODO.md`, `tools/render-status.py` | Child tasks and later receipt-backed metrics, not inferred completion |

At the design freeze, the retained Lean receipt bound 55 `p0.scalar.v4` rules to
the September 9 EvoIR 1.9.87 subject and pinned Lean 4.32.2. The current binding
is in [`lean-p0-receipt.json`](../tests/contracts/lean-p0-receipt.json). A receipt
is not an audit of every later compiler commit. Refresh the relevant audit and Rust/Lean correspondence before
promotion, preserving the historical receipt identity. Other retained gates
and their limits are in [status.md](status.md).

At the design freeze, the persistent wrapper took `ScalarEqBudget` and constructed its
default policy. `optimize_scalar_module_with_policy` was not itself the full
persistent wrapper. Add a policy-aware persistent entry point and make the
existing default API delegate to it. The policy harness chains up to
`max_transactions` (default eight), checks adjacent digests and invokes the
standalone checker, whereas the driver discarded the single transaction's
certificate. That divergence motivated the shared runner below. Do not benchmark
one optimizer and ship another.

Implementation checkpoint (September 29): `evocc-eqopt::run_scalar_chain_with_checker`
now owns the shared persistent chain for the driver and policy evaluator. It checks
every transaction, including the terminal zero-region proof, and commits caller
state only after the whole chain succeeds. `--eq-proof-dir=NEW_DIR` retains the
ordered bundles and a canonical `evocc-verified-chain-v1` record; existing
destinations are refused. The independent archive admission path is
`python3 tools/bench.py --verify-eqsat-chain DIR --checker PATH`. It replays the
actual retained bytes instead of trusting a stored VERIFIED label. These paths
do not implement R02 performance measurement, R03 discovery or G2.

R02 prerequisite checkpoint: exporting `--eq-proof-dir` with EqSat off now
produces `evocc-identity-chain-v1`, binding the same canonical input/output IR
with zero transactions and no EVP proof success. Its independent byte-identity
audit needs no checker. The bounded generated-IR cap collector is also available
through the existing bench entry point. Its first 32-program diagnostic observed
no default cap hits; it is not the application preflight, native performance
admission or an AI-track verdict. See the
[scoped protocol and result](archive/notebook/eqsat-cap-census-20260929.md).

R02a foundation checkpoint: policy report v3 separates producer, independent
replay, checker identity and wrapper costs without changing chain/proof
identity. Redundant terminal interpretation, repeated Admission construction
and an extra whole-chain staging clone have been removed without bypassing
checker replay or final identity validation. The bench CLI also has a read-only
frozen-plan audit for checked native A/A calibration; its successful output
explicitly remains EXECUTION_NOT_RUN. General effective-control validation,
application artifact/result binding and the complete collector remain open. See
[implementation boundaries and gates](archive/notebook/eqsat-r02a-foundations-20260929.md);
this does not change the reviewed experiment contract below.

The subsequent [bounded native adapter](archive/notebook/eqsat-native-calibration-20260929.md)
implements exact-digest IR-to-MIR transport and registered Darwin P0 A/A capture,
with per-execution result checks, balanced order, raw samples and failure
retention. A frozen SDK-bound run completed 24 timed executions and four warmups
of one binary; the independent capture audit passed. The initial SDK-less link
failure is retained with zero samples. This is pipeline calibration, not an
EqSat speedup or an application census. Its source/backend profile boundary is
explicit. It is not yet the
general application measurement runner, optimized-chain admission or an R02b
effect-ceiling verdict; the notebook records its actual acceptance state.

R02a/b/d qualification checkpoint (September 30): the registered Linux AArch64
hardware-VM pilot now implements the manifest/collector, shared verified-IR
checked/unchecked fan-out and separate three-track preflight. Its independent
audit passes for 576 timed executions, 138 EVP certificates and 108 checked-twin
admissions; ten read-only real-capture mutations are rejected. The default-cap
table comes first and observes no hits in twelve baseline transactions. All
policy/rule/shortcut ceiling verdicts and all twelve A/B latency comparisons
are INCONCLUSIVE; no AI campaign or final Q2/G2/Q3 gate is admitted. Scope,
mixed links, complete negative evidence, two pilot failures and the repaired
regional-fuel/collector bugs are recorded in the
[qualification notebook](archive/notebook/eqsat-r02-linux-pilot-20260930.md) and
[tracked receipt](../tests/contracts/eqsat-r02-pilot-receipt-20260930.json).
This checkpoint supersedes the OPEN implementation statements above, not the
reviewed experiment or trust-boundary contract below.

September 30 review correction: the original capture is reproducible, but its
fresh-process millisecond timing protocol did not perform Linux-specific A/A
calibration before inheriting Darwin's sample count. Artifact audit is not timing
admission. Preserve the capture and its inconclusive latency results; complete
R02b1's deterministic policy/template enumeration and R02a1's steady-state Linux
calibration before another A/B run. Collector work must be silent during the
guest's measured interval, with identity checks at explicit batch boundaries.
Template UNKNOWN is reserved for completed proof-process failures; retention,
artifact mutation, launch and watchdog failures abort the preflight. Zombie-only
groups are not live execution, guard counts cover the complete UB reason family,
and malformed wall-scope telemetry must fail explicitly instead of underflowing.

September 30 frozen close-out (analysis/receipt, not a semantic contract change):
R02b1's full registered deterministic census is independently accepted;
R02a1's steady A/A admits all ten comparisons and its single preregistered A/B
completes 312 timed samples with **10/10 INCONCLUSIVE** outcomes at the unchanged
5% gates. All typed execution checks pass, but no latency claim or AI campaign
is admitted. The original millisecond pilot and failed auditors remain retained.
The [formal three-track adjudication](archive/notebook/eqsat-r02-review-repair-20260930.md#formal-three-track-adjudication-and-stop-decision)
freezes policy/rule/shortcut uncertainty and caps further audit engineering;
the [steady result](archive/notebook/eqsat-r02-steady-protocol-20260930.md#frozen-single-ab-result)
records raw identities, actual intervals and the unchanged split-runtime audit.
October 1 owner decision supersedes the open choices: V1 Q2's primary objective
is **object code size**. Latency is **NOT_MEASURABLE_ON_THIS_LANE**; no further
VM A/B timing, retuning or sample-count expansion is scheduled. R03 V1 is frozen
as a **completed negative-admission study**, with policy/rule/shortcut verdicts
INCONCLUSIVE / NOT_ADMITTED. R03a/b/c/d do not start, grammar and vocabulary do
not expand to force a positive result, and R03e is deferred. EqSat plus Lean is
a trust anchor for the rewrite domains requiring proof, not a claim about the
whole optimizer or useful AI rule discovery. R02e guard-predicate folding is the
only next semantic slice, conditional on its diagnostic size ceiling. Audit
tooling is capped at observed failure modes. The sequential S1–S6 execution and
claim boundaries are recorded in the [size work-package ledger](archive/notebook/eqsat-size-work-packages-20261001.md).

The current root census counts single-result `bits<1..64>` instructions in each
transaction input, including constants and excluded instructions; block
arguments, multi-result operations and other types are outside this denominator.
Exclusion reasons are disjoint, with taint taking precedence over unlisted
instruction shape and propagated dependency exclusion. `admission.regions` is
staged work; `roots.accepted` is retained work after rollback. Matching visits
remain null (NOT_MEASURED), not inferred from EVP fuel. Aggregate transaction
counts are not unique source coverage.

### 2.1 Review findings and source anchors

These observations justify revisions 2–5; they are not performance measurements.

| Finding | Inspected implementation | Consequence |
| --- | --- | --- |
| Weights filter a proposed replacement; they do not choose the globally cheapest alternative | `evp1.rs::policy_keeps`, `alias_representative`, `cheaper_member`; `scalar_eq.rs::extract_cheapest` uses fixed shape/order preference | Enumerate behavior classes before AI search; cost-aware multi-candidate extraction is separate work |
| v1–v3 produce aliases/constants; v4 adds four emitting rules | `evp1.rs::emitting_rewrite`, registry `p2_300..303`; `extract_cheapest` chooses by `(rank, OpId)` before `policy_keeps` | Weights only accept/reject the already selected root/replacement pair; they cannot choose add versus shl; no measured search-space cardinality claimed |
| Budget stop retains earlier admitted regions | `evp1.rs` admission loop; `scalar_eq.rs` budget stop | Changed output and budget exhaustion may coexist |
| Legacy budget/report fields have multiple scopes | `scalar_eq.rs` transient bridge counters; `evp1.rs` admission counters and `scheduler_record` | Separate search counters, certificate admission and recorded per-rule/epoch fuel; see Section 3.3 |
| Driver discards proof; evaluator checks a chain | `evocc/src/main.rs::optimize_scalar_evoir`; `evocc-policy/src/lib.rs::evaluate`; `evocc-proofcheck/src/main.rs` | Producer and verification verdicts must have separate authorities |
| Lowering already simplifies the baseline | `evocc-lower/src/lib.rs`, `constants`, `cfg`, `reuse` | EqSat-off does not mean unoptimized; count opportunities after lowering |
| Checked guards survive P0 rewriting | EvoIR Section 8.5; `evocc-mir/src/lower.rs::lower_checked_result` and `UbAssume` handling | Preserve UB obligations; native selection can share arithmetic/flags but is not EqSat guard discharge |
| Lean rule coverage is universal over `Nat` width environments | `lean/EvoIR/Rule.lean::{CondHolds,RuleSound,RuleCovered}`; `tools/lean/audit.py` exact-type check | A receipt cannot invent a narrower proved domain |
| Rules are integrated per ID on both sides | `evp1.rs::{exact_rule,emitting_rewrite}`; `evocc-proofcheck/src/main.rs` rule handlers | Initial new-rule throughput is explicitly bounded by reviewed manual integration |

At the revision-5 design-freeze baseline, the MIR spec described checked/unchecked
profiles and a driver flag, but the inspected driver options and `UbAssume`
lowering did not expose unchecked. That implementation gap is now closed under
R02d: the explicit native-only switch and common verified-IR twin admission are
qualified in the September 30 checkpoint. Historical absence is not a current
capability claim; no harness deletion of source/IR obligations is authorized.

## 3. Architecture and interfaces

```text
Development workloads / profiles / missed opportunities
                         |
             Untrusted candidate proposer
             AI / enumeration / evolutionary search
                         |
             Typed admission + counterexamples
                 /                    \
        Policy candidate         Rewrite candidate
        bounded config           utility screening, then fixed proof
                 \                    /
                  Versioned experiment config
                            |
             Shared certified persistent optimizer
                            |
             before IR + after IR + certificate
                            |
                 Independent proof checker
                            |
              Native correctness -> measurement
                            |
                Held-out evaluation / promotion
```

No model runs in default compilation. Discovery and theorem proving are
offline. The proposed proof-required path uses frozen policies/registries and
independent certificate replay. The design-freeze driver lacked that replay;
the R01a implementation checkpoint in Section 2 records the shared replacement.
Through the candidate interface, AI cannot edit semantics, validators,
theorem definitions, acceptance expectations or held-out inputs. Changing those
is a separate reviewed engineering change.

### 3.1 Candidate data contracts

These are **proposed interfaces**, not existing schemas or CLI options. Implement
schema validation before accepting input. Use closed typed data, not executable
callbacks or model-supplied shell commands.

| Record | Required fields |
| --- | --- |
| `RewriteCandidate` | schema version; base semantic/registry digests; typed LHS/RHS AST; variable/result width expressions; existing guard AST; relation exactly `eq`; optional bounded derivation data; provenance; **no template selector** |
| `PolicyCandidate` | schema version; admitted registry digest; all 13 current operator weights; scoped budget mapping from Section 3.3 and transaction ceiling; extractor/schedule revision; objective ID |
| `ProofAdmission` | candidate digest; exact `RuleCovered rule_<id>` proposition and semantic closure digests; existing typing/guard predicate identity; trusted template-ladder revision, attempted templates and actual successful template; toolchain IDs; actual transitive axiom/analysis dependencies; replay result |
| `ExperimentManifest` | subject identities; variants; workloads/splits; effective flags; runtime machine; semantic profile; budgets; measurement/statistical protocol; acceptance bounds |
| `Measurement` | manifest/variant/workload IDs; correctness/proof verdicts; artifact hashes; raw samples; optimization coverage; exclusions/fallbacks; resource/timeout status |

Reject unknown fields, duplicate keys, malformed ASTs, unresolved names,
out-of-range integers, non-finite metadata and unsupported profiles. V1 accepts
only equality: reject every relation other than exactly `eq`, even if a model
calls a refinement an equality. Reject proof-source/tactic/import fields.
Also reject a candidate-supplied template ID: trusted code alone selects the
fixed ordered ladder in Section 4.4. Candidate semantic identity hashes the
typed equality, guard and semantic/registry identities; derivation evidence has
its own digest, and proof results bind the actually used evidence/template.
Changing proposer metadata cannot select a different proof path or change that
semantic candidate identity.
There is no independently selectable `proved_domain` or `instance_domain`
field in V1; Section 4.1 defines the actual quantified proposition. Hash
canonical semantic data, not display names or LLM prose. Specify one deterministic
UTF-8 JSON encoding with sorted object keys, no insignificant whitespace and
exact integer/string encodings; test independent decoding/hashing. Record metric
units explicitly; use specified decimal encodings for noninteger measurements.

Keep proposer provenance separate from semantic identity: model/version,
prompt/response hashes, search seed, parent candidate and feedback IDs. Search
randomness is not an optimizer seed: replaying the same policy and module is
deterministic. The transaction ceiling is part of the optimizer identity, not
merely evaluator bookkeeping.
Replaying a frozen candidate must not depend on regenerating an LLM response.
Never store credentials. Sending source to a remote model requires existing
provider/data authorization; this plan does not grant it.

The proposer receives a bounded development packet: a reduced typed IR region,
the applicable semantic/guard definitions, current registry and policy,
eligibility/stop diagnostics, relevant emitted-code metrics and previously
exposed development counterexamples. Do not send the entire evidence tree.
Target instruction suggestions are hypotheses, not new IR operators; adding
machine-level equalities requires a separate semantic/ABI proof boundary.

The initial diagnostic interface uses existing weights and schedule fields.
It may prove too small for R03a; it is not a commitment to run AI over a
degenerate space. R03e specifies the conditional cost-extraction extension;
rule masks, epoch allocation and region ordering are additional versioned
control surfaces only if separately implemented and validated. Learned feature
models are not accepted executable callbacks. Validate producer-local ceilings
against the spec ceilings. Keep normative certificate budget fields separate
from tighter producer budgets, as the current EVP path requires.

### 3.2 Optimizer and driver boundary

Extend the existing `CertifiedScalarEqOutcome`, not a parallel optimizer.
Preserve before/after canonical digests, certificate, policy digest and report.
Use orthogonal fields rather than a flat disposition enum (names below are
proposed schema values):

```text
TransactionRecord = {
  chain_id, index, input_digest, output_digest,
  result: CHANGED | UNCHANGED | ROLLED_BACK_ERROR,
  stop: NORMAL | POLICY_BUDGET | INTERNAL_ERROR,
  budget_hits: [scoped counter/limit records from Section 3.3],
  roots: {total, visited, eligible, proposed, accepted,
          declined_by_reason, unvisited},
  certificate_state: EMITTED | NONE,
  verification: NOT_CHECKED | VERIFIED | REJECTED,
  rejection_reason: NONE | CERTIFICATE_MALFORMED | PROOF_INVALID | CHECKER_TIMEOUT |
                    CHECKER_UNAVAILABLE | CHECKER_ERROR |
                    CERTIFICATE_MISSING | BUNDLE_MISMATCH
}
ChainRecord = {
  chain_id, input_digest, output_digest,
  max_transactions, attempted_transactions, verified_prefix,
  stop: NO_CHANGE | TRANSACTION_LIMIT | TRANSACTION_BUDGET | ERROR,
  publication: NOT_PUBLISHED | VERIFIED | REJECTED,
  rejection_reason: NONE | <transaction rejection reason> | PRODUCER_ERROR
}
```

The following is the **closed set of legal core transaction-record combinations**
after the producer attempt returns. A list inside a cell means each listed
value is allowed with the other cells in that row; every other combination is
invalid. Verification verdicts are assigned only by the wrapper. `ROLLED_BACK_ERROR`
and `INTERNAL_ERROR` deliberately carry a redundant consistency check, not two
independently selectable error states.

| result | stop | certificate_state | verification | Required invariant |
| --- | --- | --- | --- | --- |
| CHANGED | NORMAL, POLICY_BUDGET | EMITTED | NOT_CHECKED, VERIFIED, REJECTED | At least one retained accepted region; canonical output differs from input |
| UNCHANGED | NORMAL, POLICY_BUDGET | EMITTED | NOT_CHECKED, VERIFIED, REJECTED | Zero retained accepted regions; output equals canonical input |
| CHANGED, UNCHANGED | NORMAL, POLICY_BUDGET | NONE | REJECTED | Wrapper observed required certificate missing; retain result/count consistency but reject publication |
| ROLLED_BACK_ERROR | INTERNAL_ERROR | NONE | NOT_CHECKED | Producer error; zero retained regions; canonical input restored; reject publication |
| ROLLED_BACK_ERROR | INTERNAL_ERROR | EMITTED | NOT_CHECKED, VERIFIED, REJECTED | Recovery/diagnostic zero-region certificate only; canonical input restored; reject publication even if that proof verifies |

Refine every row by the following **closed rejection-reason table**. The core
table and this refinement must both hold; reject unknown enum values and any
other combination. Free-text diagnostics cannot substitute for these fields.

| verification | certificate_state | rejection_reason | Meaning / required evidence |
| --- | --- | --- | --- |
| NOT_CHECKED, VERIFIED | As allowed by the core table | NONE | No rejection; VERIFIED requires completed affirmative replay, not absence of an error |
| REJECTED | NONE | CERTIFICATE_MISSING | Producer claimed CHANGED/UNCHANGED success, but wrapper could not obtain the required certificate |
| REJECTED | EMITTED | CERTIFICATE_MALFORMED | Checker exited 2: malformed input module or certificate; a producer/transport defect, not a well-formed claim rejected by replay |
| REJECTED | EMITTED | PROOF_INVALID | Checker exited 3: well-formed input whose claim was checked and refused |
| REJECTED | EMITTED | CHECKER_TIMEOUT | The recorded checker invocation exceeded its declared limit without a completed verdict |
| REJECTED | EMITTED | CHECKER_UNAVAILABLE | Checker could not be started; no replay verdict exists |
| REJECTED | EMITTED | CHECKER_ERROR | Checker crashed or its versioned result protocol failed; no completed validation verdict exists |
| REJECTED | EMITTED | BUNDLE_MISMATCH | Retained transaction artifacts disagree with the claimed before/after/proof identities; wrapper rejects before replay |

The two NONE-certificate core rows intentionally differ: success claimed by the
producer incurs a proof obligation, so a missing certificate is REJECTED with
CERTIFICATE_MISSING. An explicit producer rollback already disclaims success;
without a diagnostic certificate there is nothing to replay, hence NOT_CHECKED
with reason NONE. Its **chain still rejects for producer failure**. Do not label
that case a proof failure or silently turn a success claim into a rollback.
An emitted recovery certificate uses the same reason table as any other emitted
certificate, without permitting publication of the failed producer attempt.

For emitted bundles, wrapper identity validation precedes launch; a mismatch
therefore takes BUNDLE_MISMATCH. Otherwise use the versioned checker adapter's
single terminal verdict. Preserve the existing [EvoIR Section 11.4 exit contract](../spec/evoir.md#114-exit-statuses):
0 maps to VERIFIED/NONE, 2 to REJECTED/CERTIFICATE_MALFORMED, and 3 to
REJECTED/PROOF_INVALID. No parser heuristic or checker change is needed for that
distinction. Other completed exit statuses or crashes are CHECKER_ERROR;
wrapper-enforced timeout and failure to launch keep their separate reasons.
A generic nonzero exit is not automatically PROOF_INVALID. Preserve secondary
diagnostics without assigning multiple terminal reasons to one attempt. Adjacent
transaction/chain identity errors remain chain rejection even when all local
transaction proofs verify; they are not fabricated local proof failures.

Hash the immutable completed chain record and its ordered retained bundle
manifest for downstream receipts. Native emission and execution receipts
reference that hash; they do not mutate the verified chain record. EqSat-off
identity chains have zero attempted transactions/verified prefix, NO_CHANGE,
and identical canonical input/output. Wrapper VERIFIED for that special case
means identity/provenance checked, not an EVP rewrite proof or an EqSat gain.

`POLICY_BUDGET` requires at least one scoped stop record (which may describe the
next admission exceeding a cap, not current consumption equalling it). `NORMAL`
has no budget hit. An internal error overrides the display stop reason but
retains any preceding budget hits in diagnostics. Keep attempted/staged region
counts separately from `roots.accepted`, which counts retained output regions.
`EMITTED` requires exact retained bundle identities, not just a producer boolean.
An emitted-but-invalid proof is REJECTED, not NONE; checker timeout/unavailability
also rejects publication, but has an unknown proof outcome, not a theorem
counterexample. `NOT_CHECKED` may transition to VERIFIED or REJECTED once; update
the reason atomically with the verdict. Retries
are new linked attempts. A valid earlier transaction can remain VERIFIED while
the chain as a whole is REJECTED.

Schema REDs must enumerate both products and reject their complements, then check
cross-field hashes/counts and chain invariants. In particular reject rollback
with NORMAL, nonzero retained regions after rollback, NONE plus VERIFIED, and
chain publication when any producer attempt failed or any required check is
missing. Pending internal work before a producer return is not a completed
TransactionRecord and cannot enter measurement denominators as a no-op.
Fault-inject every rejection reason, the two missing-certificate paths, and
an unavailable checker incorrectly reported as PROOF_INVALID. Keep malformed
input (exit 2) and well-formed invalid claim (exit 3) as separate REDs, including
an attempted merge of their accounting buckets. These are schema
and adapter acceptance requirements, not claims that the current CLI emits them.

The wrapper assigns **transaction** verification after checking that transaction's
exact bundle with the standalone checker. This verdict can remain VERIFIED even
if a later transaction fails, or if it verifies only a diagnostic recovery proof.
**Chain publication** is VERIFIED only after every required transaction succeeds,
all proofs pass and all chain checks pass; a producer error always prevents it.
The producer may issue `CERTIFICATE_EMITTED`, which is not proof acceptance.
`CHANGED_VERIFIED`, if retained as a display label, is derived from a changed
transaction in a successfully published verified chain. Neither the producer nor
a single stateless checker invocation certifies the chain or native emission.

Preserve existing partial progress on a policy budget stop: earlier admitted
regions remain in the output and certificate; unadmitted roots remain unchanged.
Thus `(CHANGED, POLICY_BUDGET, accepted>0)` is legitimate and can become verified.
It loses coverage, not proof validity. With zero admitted regions, retain the
existing zero-region certificate/canonicalization contract. Never replace an
accepted partial transaction by a zero-region certificate merely to simplify
the status report. Stop the proposed shared run after a transaction budget stop;
do not silently obtain extra budget by retrying it. Reaching the chain's
transaction limit similarly permits publication of its fully verified result.
Define optimization change relative to canonical input and admitted replacements;
zero-region canonicalization alone is not an optimization gain.

Run the driver and evaluator through the same policy-aware persistent **chain**
entry point, including transaction limit, stopping rules and state updates.
Check the first digest against the frozen input, every adjacent digest, the
last digest against codegen input, and exact chain length/order. Each transaction
has its own existing proof budget record; chain settings live in the run receipt
and policy identity. Persistent history cannot supply unauthorized premises to
a new certificate. Checker acceptance proves equivalence, not optimality or
that a telemetry claim of saturation is true.

Initially the evaluation wrapper launches `evo-proofcheck` on the exact
before/after/proof bundle before installing or executing a candidate artifact.
The compiler package also builds the same independent checker implementation as
`evocc-checker`; its dependency graph excludes the optimizer. Proof-required
entry points must compare the executable digest to an expected identity supplied
by the trusted build/release or frozen campaign, before execution. They must not
derive that expected identity from the current sibling at compilation time or
from untrusted candidate/certificate data. Retain and recheck the actual binary
identity as well. A local test may explicitly trust the artifact its Cargo build
produced; this is test provenance, not release authentication. The Rust and
archive-consumer deadline domain is uniformly 1..300000 ms. Timeouts remain
unknown outcomes, not invalid proofs.
If codegen has already produced an object, quarantine it until acceptance.
In proof-required release mode, any internal error, checker failure/timeout,
missing proof or broken chain rejects publication of the **whole chain** and
returns failure with no new installed artifact. Stage the run's persistent state
and outputs; commit them together only after verification. Retain the verified
prefix for diagnosis, but do not publish that prefix or reuse a failed run's
state as if the requested run succeeded. This is a proposed outer atomic
publication contract, not a change to within-transaction budget semantics.
If the current producer recovers an internal failure with a zero-region
certificate, even successful checker replay of that certificate must not erase
the failure recorded by the producer.
The verified-prefix count includes only successful producer attempts with
accepted proofs; a diagnostic VERIFIED zero-region recovery proof is excluded.
Baseline recompilation is a separate explicitly requested invocation/receipt,
never silent recovery and never a successful optimized sample. Existing
non-proof-required compilation remains labeled uncertified until migrated.

**Telemetry trust boundary.** Distinguishing a legitimate no-op from an internal
fallback depends on producer telemetry (`fell_back`, `proof_replay_failed` and
the future typed error result). This classification is trusted measurement
instrumentation, **not certified by EVP replay**. Both paths can yield the same
valid zero-region proof. Require a versioned complete telemetry record; absence
is a measurement/contract error, not implicit success. Fault-inject `apply`
failure and persistent-state replay failure after staging regions; assert
ROLLED_BACK_ERROR, whole-chain rejection, no installed artifact and retained
failure counts even if the recovery proof passes. Include a legitimate eligible
region with no applicable rule as an UNCHANGED control. Independently recomputed
eligibility counts can warn of a suspicious no-op but cannot prove failure or
force optimization. This trusts producer telemetry for experimental accounting,
not producer equality claims for soundness.

Bind installed object hashes to the compiler, effective argv, source/generated
input hashes and verified IR output. Codegen consumes that output without an
unrecorded optimizer pass. The certificate proves EvoIR replacement, **not**
machine emission; native and ABI tests remain mandatory.

Report total scalar roots, eligible roots, proposed/accepted replacements,
exclusion reasons, scoped counters/limits from Section 3.3, proof bytes and
stage-specific stop reasons. Define denominators explicitly: changed TUs / attempted TUs
and changed roots / eligible roots are different. `regions_admitted` alone is
not an eligibility percentage. Define counts per transaction input; aggregate
chain counts may count a root more than once and must not be presented as unique
source coverage. Root outcomes and transaction stop reason are not the same
denominator. If admission scanning stopped early, mark unavailable counts
unknown or derive them in a separate read-only inventory, never as zero.

### 3.3 Budget scopes and counter identities

The current code reuses `ScalarEqBudget.max_nodes/max_epochs` in two stages,
then overwrites some `ScalarEqReport` fields. Preserve both stages' counters
before adapting the report. The new schema must record **scope, unit, consumed,
limit, and hit reason**; an unqualified `epochs` or `nodes` metric is invalid.

| Scope / proposed metric | Current source meaning | Bound and interpretation |
| --- | --- | --- |
| `search.bridge_nodes` | Transient `scalar_eq.rs` accumulates `bridge.node_count` before certificate admission | Current `max_nodes` also limits this sweep; a node cap can stop discovery before the policy gate sees later roots |
| `search.rewrite_events` | Transient `report.epochs` advances on accepted transient rewrite receipts | Current `max_epochs` also limits this count; it is not a measurement of repeated saturation passes |
| `admission.accepted_regions` | `evp1.rs`: `regions.len()` after `policy_keeps` | Current `max_epochs` is reused as a retained-region cap, not an epoch counter |
| `admission.proof_nodes` | Sum of `EvpEvent::Node` events in accepted region certificates | Current `max_nodes` is reused as this cap; not bytes, unique persistent nodes, match attempts or CPU time |
| `evp.nodes`, `evp.epochs` | `scheduler_record`: serialized node events and maximum epoch marker respectively | Existing fixed EVP profile bounds; never replace with a producer-local cap or the accepted-region count |
| `evp.fuel` | Largest recorded firing count of any one `(rule, epoch)` pair; `n` is the largest before-witness count across regions | Bound `max(256, 8*n)` under the existing record contract; not summed over rules/epochs and not a cap on all matching attempts |
| `search.match_attempts`, CPU/wall time | Actual matcher visits/work, including failed matches and declined candidates | Separate instrumentation and resource/watchdog caps; NOT_MEASURED until implemented; cannot be inferred from proof fuel |
| `chain.transactions` | Producer attempts under the frozen shared run policy | Separate chain limit; not an epoch or region count |

The legacy API currently couples the two node caps and the two event/region
caps. Record that mapping and their equal configured values; do not claim four
independently adjustable knobs until a versioned implementation separates them.
Its single `budget_exhausted` boolean cannot identify the stopping stage. R01a
must preserve stage-specific hits; historical ambiguous receipts remain
UNKNOWN_SCOPE and are ineligible for budget-specific causal attribution.

In the **certificate-admission loop**, rejected policy candidates consume no
accepted-region/proof-node quota. Zero weights can therefore reserve certificate
capacity for later candidates only if those candidates reached this loop and
its caps bind; they cannot recover transient work or roots already lost to the
earlier sweep. Per-rule/epoch firing fuel is another constraint, not this quota.
Without a binding admission cap, that quota-reservation argument disappears.
Any stronger dominance claim still needs an explicit objective and downstream
code/chain assumptions; an alias cheaper in IR is not a proof of native benefit.

## 4. Rule soundness and admission

### 4.1 Fixed semantics and quantified coverage

Reuse the exact `RuleSound`/`RuleCovered`, not a similarly named replacement.
The present interface is:

```text
RuleCovered r := forall (Gamma : Var -> Nat) (out : Nat),
                   RuleSound Gamma out r

RuleSound requires, for every substitution and sound input/result facts:
  denotePat(Gamma, lhs) succeeds at out and existing guard holds
  implies denoteRhs(Gamma, rhs) is the same value.
```

The theorem quantifies over **all natural widths**, including zero and widths
above 64; typing and the rule's existing guard determine its meaningful cases.
`CondHolds` has `widthMin` but no `widthMax` or general width-relation predicate.
The Rust admitted P0 instances are narrower than this theorem, not an extra
parameter to it. The audit expects exactly `RuleCovered rule_<id>`.

V1 keeps that interface. A candidate valid only at widths 1..64 cannot be
admitted by attaching a `proved_domain` field or 64 proofs to the receipt.
Mixed-width shifts also have independent count widths; testing data widths alone
does not cover their domain. A bounded proof that fails to establish the current
universal statement is `UNSUPPORTED_DOMAIN`, not proved.

R03g is an optional, separately reviewed extension: define a real admitted-instance
predicate, a coverage theorem over that predicate, and a proof that every Rust
producer/checker-enabled instance belongs to it. Version the spec, audit's
expected proposition, decoder and registry/profile identities together. Include
zero, >64, mixed-width and missing-relation negatives. Finite `1..64` enumeration
is available only for a genuinely bounded dimension of that new proposition;
unbounded count widths still require a proof or enforced explicit boundary.
No such domain extension is enabled by this design update.

Random tests and small-width exhaustive checks reject candidates cheaply; they
are not universal proofs. A solver's bare `unsat` result is not a Lean proof.

Preserve C definedness obligations. Modular IR identities do not authorize
dropping source UB checks. Equality is symmetric within its proven scope;
one-way refinement needs a separate future relation and cannot justify e-class
union. Guarded equalities cannot escape their fact scope into global equality.

Check that the typing domain is inhabited and retain concrete satisfiable guard
witnesses for intended instance families. This catches useless/vacuous proposals,
not proves soundness. V1 cannot add an arbitrary `False` guard or assume the
desired equality itself. A theorem assumes sound facts: new guards/analyses would
need their own proofs and independent checker support; V1 uses existing guard
forms only. Result-class facts come from the pre-merge state, not the equality
currently being justified.

### 4.2 Derived shortcuts before new rules

**Derived shortcut:** express a proposed rewrite as a bounded sequence of
existing registry rules/congruence steps. Expand its certificate into those
steps, including substitutions and premises. It adds no trusted rule ID and may
reduce discovery work; whether it saves actual work is a measurement, not a
property of its shorter spelling. Current certificates record a per-rule/epoch
firing limit `max(256, 8*n)`, not a total matcher-work limit (Section 3.3), and
matching is hand-written. Report actual matcher visits, scoped node/event counts,
compiler/checker time, and expanded certificate bytes, not just
macro steps or a smaller nominal bound.

In particular, a `const_like` LHS premise must name a constant in the **before
witness closure** of that transaction. A newly invented/derived constant does
not satisfy it. Reject an unexpandable shortcut; do not fabricate a before
node or silently add a transaction. If an additional verified transaction
legitimately makes it a before node, record that transaction and its costs.
Include exactly this missing-before-constant case in the RED set.

**New proved rule:** construct the exact Lean theorem against frozen semantics,
audit it, then introduce a reviewed registry revision. Update spec, generator,
producer, checker, fixtures and digest together. Never reinterpret an old rule
ID/digest or permit model-supplied runtime registry extensions. The current
hard-coded producer/checker handling requires explicit integration.

### 4.3 Rule integration is an explicit work package

**V1 choice: a small, manually integrated batch**, with the candidate count and
review/proof budget frozen before discovery. R03f establishes the integration
checklist and independent mutation fixtures before R03b or R03c. Each new rule
touches its Lean data/theorem, spec registry/digest, producer matcher/builder,
independent checker handler and tests. Shortcuts need producer integration and
independently replayable expansions. Account for that engineering time: this
route cannot claim automatic high-throughput rule admission.

A registry-AST-driven generic matcher/builder is a conditional R03f follow-on,
not a one-line plug-in promise. It changes the A3/A4 correspondence boundary and
must land separately before a high-throughput campaign: independently implement
producer matching/building and checker substitution/replay; test wrong bindings,
widths, guard scope, invented constants, RHS nodes and malformed ASTs. Do not
share producer matching decisions as checker evidence. Generated shared rule
data needs independent decoder/correspondence checks. A verified matcher is a
possible later reduction of the TCB, not an existing property or a prerequisite
for the bounded manual pilot.

### 4.4 Fixed-template proof execution and trust audit

**V1 accepts no AI tactic scripts or compiled proof artifacts.** AI supplies
typed candidate data/derivations; trusted code constructs the exact proposition
and tries a versioned, **fixed-order template ladder** with fixed imports and
per-template limits. The proposer cannot select or reorder it. Freeze the ladder
in the experiment manifest; ProofAdmission records attempted templates and the
first successful audited template. Rejected template-selector injection must
not create a second semantic identity for the same candidate. Infrastructure
timeouts retain UNKNOWN/resource status, not a model-directed retry path.
First try reviewed universal simplification/lemma templates. A fixed
`bv_decide` template over enumerated widths is a later route only after R03g supplies the correct domain
interface and a reviewed trust policy for its actual dependencies. Neither a
template ID nor successful bounded execution weakens `RuleCovered`.

Run even fixed templates under resource limits with no network, credentials or
held-out data. Do not interpolate candidate text as Lean commands. Exact AST
decoding, header construction, escaping and definition hashes are acceptance
boundaries; mutate them in tests. Arbitrary script isolation/export/rechecking
belongs to V2 and is removed from the V1 critical path. No claim is made that
`lean4checker` or Mathlib is installed or qualified for pinned Lean 4.32.2.

Successful compilation is not admission. An independent clean checking stage:

1. Binds candidate AST/guards to exact rule data and the universal proposition;
   any future bounded mode additionally binds its actual domain predicate; checks
   encoding fidelity independently, not only parser/renderer round-trip.
2. Pins and compares the referenced semantic definition closure.
3. Rejects `sorryAx` dependencies, unauthorized axioms and substituted imports.
4. Rechecks the proof and exact statement, recording transitive dependencies.
5. Records the actual transitive proof dependencies, including fact-transfer
   theorems. The current 55 rule proofs are kernel-only, but fact-driven admission
   also relies on native-backed enumerated transfer rows. A kernel-only rule
   theorem does not make that entire chain kernel-only. Existing audited
   reflection dependencies are scoped evidence, not permission to accept any
   new native-backed theorem or arbitrary axiom. New template/dependency classes
   need explicit review; absence of `sorryAx` alone is insufficient.
6. Refreshes relevant Rust/Lean correspondence and independent mutation tests.

Do not infer safety of arbitrary proof source from a `#print axioms` transcript:
V1's generated data cannot replace definitions, commands, imports or audit
output. Qualifying arbitrary proof export/recheck on the pinned toolchain would
be a separate V2 task. See the
[Lean validation guidance](https://lean-lang.org/doc/reference/latest/ValidatingProofs/).

Lean proves the modeled rule. The model, decoder, fact derivation, matching,
Rust correspondence and backend remain explicit trust/validation boundaries.
No whole-compiler verification claim follows.

## 5. Performance experiment contract

### 5.1 Correctness admission and workload inventory

Extend `tools/bench.py` with a proposed performance mode and internal
`tools/ladder/` adapter; reuse source pins, runtime setup, locks and builders.
Do not change the existing correctness CLI meaning or write performance results
over historical correctness receipts.

Every variant must pass the frozen workload predicate on the same source/input
before its metrics are eligible. Validate every timed execution's result,
outside the timed interval where possible. Check the exact timed binary, not
an unrelated instrumented build. Unknown UB, wrong output and missing proof
make a result ineligible, not fast. Oracle agreement supplements the approved
semantics rather than defining it.

Pilot: existing P0 cases, pinned zlib/zstd and one interpreter/data-processing
lane that meets fresh correctness admission. Select by coverage, not observed
speedup. Freeze exact programs/inputs before candidate search. Historical PASS
does not admit a new compiler/workload automatically. Expand only after the
additional lanes meet their own prerequisites.

| Workload | Primary metric | Controls |
| --- | --- | --- |
| P0 small C/IR cases | Emitted code bytes; search cost | Boundary values, both interpreters, proof replay, native result; not application-speed evidence |
| zlib / zstd | Compression and decompression throughput separately | Fixed bytes, level/dictionary, output-size accounting and round-trip |
| jq / Lua / cJSON | Task latency or throughput | Fixed scripts/documents and outputs; startup vs steady state separate |
| SQLite | Query/transaction latency and throughput | Same data, SQL, durability and result checks; in-memory vs disk separate |
| Doom | CPU frame-computation time | Existing pinned demo/profile, fixed frame hashes; no display pacing/GPU/audio claim |
| LLVM test-suite / PolyBench | Suitable native workload time | Frozen applicable subset/dataset; correctness outside timing; MINI_DATASET PASS is not performance |
| Kernel | Executable section size first; runtime later | G2-accepted exact configuration and separately frozen workloads; boot/selftest timeout is not a score |

Existing mixed-link lanes may be used only as **mixed-link experiments**, with
identical substituted-object coverage and reference-built dependencies across
variants. Full-program claims require the declared subject C objects built by
that variant; no silent reference fallback. FP-containing programs can measure
scalar-integer improvements while FP stays outside P0. Keep contraction,
rounding, wrapping, aliasing, ISA **and native UB-check profile** equal within
each causal comparison; do not introduce fast-math.
A larger performance dataset is a new frozen workload, not replacement evidence
for the old correctness campaign.

Manifest controls must include:

- Requested and effective `ub.assume` mode, exact driver/backend implementation
  identity and emitted-guard verification. Never infer unchecked mode from `-O2`
  or `-fwrapv`; the latter changes source arithmetic semantics.
- The baseline lowering-pass profile: integer constant folding, immutable-global
  propagation, pure-value reuse, constant-edge pruning and linear-block joining,
  with implementation identity and effective optimization level/macros.
- The shared chain runner, extractor/scheduler revision and scoped limit mapping
  from Section 3.3, plus proof-required mode. Pipeline identity is part of the experiment.

Measure checked and unchecked EvoCC profiles separately **once the missing
unchecked implementation is closed under R02d**. Until then it is NOT_RUN, not
simulated by deleting obligations from the source/IR. Within each profile A–E/N
are matched. Checked EvoCC versus ordinary GCC/Clang `-O2` is a comparison of
different runtime-diagnostic contracts, labeled as such, not equal-feature
optimizer overhead. The unchecked comparison supplies the ordinary release
context on the admitted defined-input domain. A diagnostic reference build with
similar sanitizers is supplementary; no equivalence to EvoCC's checks is assumed.
Keep native source-UB guards distinct from interpreter initializedness/provenance
checks and other fail-closed machine checks.

**Unchecked admission rule.** For each unchecked EvoCC variant and workload
input, require a completed passing **checked twin** first: same source/generated
inputs, runtime data/seed, compiler, policy/chain and semantic flags, differing
only in the native UB-check profile. This is a **hard artifact identity rule**:
optimize/verify once, then fan out the same canonical EvoIR bytes to checked and
unchecked MIR lowering. Both native receipts must reference the identical
verified-IR output digest, immutable chain record/bundle digest and retained
proof bundles. The native profile is not an EvoIR-chain input; it remains in
each native artifact/cache identity and effective argv. Do not rerun EqSat for
the second twin, strip IR obligations, or accept merely equal manifest settings.
For variant A (EqSat off), share the canonical input and an explicit identity
chain record (zero transactions, identical input/output bytes, no claimed EVP
rewrite proof); the wrapper checks that identity rather than inventing a proof.

Before execution, the wrapper rejects different IR bytes/digests, different
chain records or any mismatch between recorded and actual codegen input. Thus
EqSat coverage is identical by construction, while checked/unchecked machine
outputs and their hashes can legitimately differ. The checked execution must
produce the expected result with no trap or runtime error; bind its artifact
and execution receipt, common IR/chain identities and runtime-input digest to
the unchecked measurement. Repeat correctness validation of the exact unchecked
timed binary. A missing/failed/timed-out twin or identity mismatch blocks
admission, not just an optional RED. A changed input or candidate invalidates
that pairing. For external
G/L comparisons use the admitted input population, without calling their binaries
EvoCC twins. A checked pass is necessary but not a general C-definedness proof:
unmodeled UB and nondeterministic paths remain separate risks; observed UB or
unknown definedness still excludes a case. R02d must not silently disable other
mandatory runtime checks while omitting `ub.assume` branches.

UB predicates are total pure boolean computations, but the current eligibility
domain excludes both those predicates and `ub.assume`. P0 cannot remove the
check overhead just because it simplified the arithmetic result. MIR already
has bounded arithmetic/flag sharing, so quantify surviving guards in emitted
code rather than equating one IR guard to a fixed machine cost. R02e is a
separate proposed predicate-folding domain: prove the predicate equality first,
preserve poison and all other uses, then separately specify any removal of a
proven-true obligation in reconstruction/checking. It must not silently admit
`ub.assume` as an ordinary pure e-node or drop C definedness obligations.
S4 now has a significant measured diagnostic ceiling. The independent
[R02e semantic-delta proposal](archive/eqsat-guard-folding-draft.md) is not an active
registry or implementation acceptance; the current P0 domain is unchanged.

### 5.2 Mandatory opportunity and effect-ceiling preflight

R02b precedes AI campaigns on **R03a, R03b and R03c**, with separate verdicts for
policy selection, derived shortcuts and new rules. A pass on one track never
admits another. Freeze a small preflight budget; bounded proof/integration probes
may resolve its questions, but bulk discovery is not the preflight itself.

#### 5.2.1 Shared census and first output

The **first output** is a default-limit saturation table for every pilot input:
transient search cap hits, certificate-admission region/node cap hits, recorded
EVP fuel/markers, chain limit hits, and matching work/time when instrumented.
Use Section 3.3's units and denominators; if instrumentation is missing, say
UNKNOWN rather than infer an admission stop from `budget_exhausted`.

Inventory pre-/post-lowering eligible roots, rule opportunities, exclusions,
guards and emitted code, then repeat after the existing verified chain. Attribute
residuals to eligibility, unsupported matching/witnesses, search limits,
certificate caps, policy rejection, existing-rule scheduling, or missing rules;
do not count an existing rule missed by a budget as a newly discovered equality.
Include reduced controls and the frozen applications. A post-lowering empty
opportunity set is a pipeline result, not a failed AI.

#### 5.2.2 Policy track

**Expected control-space collapse:** `extract_cheapest` first chooses a member
by `(fixed shape rank, OpId)`, independent of policy weights. `policy_keeps`
then sees only that selected `(root, replacement)` pair. Before R03e, C can
accept or reject it, but cannot request shl instead of an already selected add
(or expose another alternative by rejecting the first). Across transactions,
earlier admissions can change later inputs; that does not create a per-root
cost-aware choice. Record this limitation before measuring policy diversity.

1. At fixed registry and schedule, derive the comparisons actually consulted by
   the current weight gate. Enumerate their feasible outcome classes, including
   zero/nonzero tests and accept/reject inequalities for the **selected pairs**.
   An unconsulted add/mul/shl weight ordering is not a distinct control choice;
   retain the selected pair and gate result in the trace. Use small canonical
   representative vectors and independently check the enumeration. Numeric
   magnitude alone does not make a different behavior class. Separately enumerate
   the small declared schedule grid; do not conflate more transactions with
   better weights. Deduplicate observed IR and code outputs while retaining
   policy and resource differences. Corpus equivalence is not universal equivalence.
2. The source structure predicts a very small space; its exact count and global
   non-dominance are **NOT MEASURED**. Do not discard zero weights as globally
   dominated without checking the scoped census. If no admission cap binds over
   the compared policies, the certificate-quota reservation effect is absent;
   then test the reduced positive-weight space under the stated objective and
   downstream assumptions. Non-exhaustion for B alone does not establish it for
   every alternative. Do not claim that skipping admission refunds transient
   matching work. An analytical reduction must state these assumptions.
3. Check every changed transaction and original output. Measure deterministically
   emitted bytes/instructions, materialization and compile work for every class.
   Interpreter steps do not distinguish single add/mul/shl substitutions and
   cannot alone establish useful separation. Use A/A-calibrated native A/B
   characterization for the pilot, not as adaptive candidate-search fitness.
4. Freeze `SEARCHABLE`, `FINITE_ENUMERATION_SUFFICIENT`, `NO_OBSERVABLE_EFFECT`,
   or `INCONCLUSIVE` per objective and workload. Identical code, or native differences unresolved
   within the predeclared noise/effect threshold, do not justify an AI speedup
   campaign. Prefer complete enumeration when it is cheap. A tiny finite space
   can test search efficiency under a stated budget, not AI superiority over its
   exhaustively measured best member.

**Expected V1 size result, to test rather than discover post hoc:** an isolated
single AArch64 add/mul/shl substitution has the same four instruction bytes.
Current weights usually affect size through retained alias/constant opportunities
and admission competition, not a byte difference among those three instructions.
If lowering has consumed those opportunities and caps do not bind, byte-fitness
ties are the structural expectation. This is not a proof of whole-function size
equality: removed constant materialization, instruction selection, register
pressure, spills and layout can change surrounding code. Record ties and these
exceptions; do not switch to noisy timing fitness to manufacture separation.

#### 5.2.3 Rule and shortcut tracks: residual missed-opportunity census

1. Freeze the candidate grammar and enumerator contract: LHS/RHS operator count
   and depth **separately**, number of variables, constant vocabulary or symbolic
   constant forms, width expressions, existing guards, DAG-sharing convention,
   deduplication, template ladder and effort bound. Freeze a **finite grammar
   lattice**, its cell visitation order, per-cell enumeration/proof/integration
   limits, total preflight limit and deterministic stop rules. A cell specifies
   all these grammar bounds, not depth alone. Start with a small stated
   bound (for example three operator nodes); this is an experiment bound, not
   an asserted universal limit of P0 or `simp`. A bounded enumerator can be
   syntactically complete only for that exact grammar after it finishes. Fuzz
   eliminates counterexamples but proves neither semantic nor proof-search
   completeness; UNKNOWN template outcomes cannot be counted as impossible rules.

   **Campaign-scale enumeration floor.** Before preflight, freeze `B_AI`, the
   planned total AI candidate-evaluation budget for this track across all cells,
   seeds and restarts, in Section 5.3's common whole-development-suite evaluation
   unit. For every cell used to justify discovery-SEARCHABLE, require
   `B_enum(cell) >= ceil(k * B_AI)`, with a predeclared `k >= 1` (default 1).
   Apply the same candidate granularity, suite, compile/proof caps and cache/retry
   charging to both methods. Raw syntax visits, fuzz trials and template attempts
   are separate counters, not substitute candidate evaluations. Record both the
   allowed and consumed evaluation counts. A healthy exhaustive completion may
   stop early; an exhaustion claim requires actually reaching the qualifying cap.
   A smaller exploratory cap, unknown/zero `B_AI`, or a proof/time/total-preflight
   limit stopping enumeration before that cap yields insufficient preflight
   evidence (INCONCLUSIVE), not discovery-SEARCHABLE. Thus neither a small cell
   quota nor an underfunded total preflight can manufacture discovery difficulty.
   Revalidate this floor if the planned AI campaign budget increases; do not
   reuse a verdict qualified only for a smaller campaign. This is a qualification
   rule, not authorization to start a campaign or enlarge preflight resource caps.
2. Compare admitted post-lowering/post-baseline regions with a pinned external
   diagnostic reference, such as a precisely configured InstCombine pass or
   bounded superoptimizer. Record translator/pipeline/tool identities and exact
   before/after forms. This is an opportunity oracle, **not** a semantic authority
   or a completeness oracle. Transfer only cases whose semantics map faithfully;
   classify mapping failures separately. LLVM poison/overflow flags, shift-count
   semantics, undef and one-way refinement cannot become EvoIR equalities by
   analogy. The [LLVM shift contract](https://llvm.org/docs/LangRef.html#shl-instruction)
   and [InstCombine proof guidance](https://llvm.org/docs/InstCombineContributorGuide.html#proofs)
   illustrate why an LLVM result or proof does not replace the fixed EvoIR theorem.
3. Reduce observed misses to small typed witnesses, count occurrences and unique
   pattern families, and record observed LHS/RHS nodes/depth, guard requirements,
   in/out-of-enumerated-grammar status, witness/matcher representability, and
   fixed-template proof status. Observed depth is not a proven minimum depth.
   Re-evaluate plausible alternatives in EvoIR semantics; external speed or
   canonical form alone does not establish gain under the chosen objective.
   Separate new equalities from derived shortcuts whose only possible benefit
   is search/checking cost. Do not create a new trusted rule for a budget artifact.
4. Compare the bounded enumerator and its fuzz/template survivors against these
   residuals under the same grammar and proof/integration budgets. Record each
   cell's enumeration termination as ENUM_COMPLETE, ENUM_BUDGET_EXHAUSTED or
   ENUM_INCOMPLETE_ERROR. ENUM_COMPLETE requires a checked syntactic coverage
   record. ENUM_BUDGET_EXHAUSTED requires a healthy enumerator reaching the frozen
   enumeration cap, with consumed resources and unvisited frontier recorded;
   it means this enumerator/run did not exhaust the cell within that cap, not
   that exhaustive search is intrinsically impossible. An unrelated watchdog,
   failed adapter, crash or missing telemetry is ENUM_INCOMPLETE_ERROR and gives
   no evidence for SEARCHABLE. Keep enumeration and template-proof completion
   separate: syntactic completion does not turn proof UNKNOWN into impossibility.
   If completed enumeration and completed qualification cover the useful
   candidates in scope, return FINITE_ENUMERATION_SUFFICIENT. If the bounded
   census finds no materialized opportunity with no unresolved candidate that
   could change the verdict, return NO_OBSERVABLE_EFFECT for that measured scope.
   Otherwise, absent the witness and budget evidence below, return INCONCLUSIVE.
5. To mark the beyond-enumeration **new-rule** track SEARCHABLE, retain a concrete
   residual witness outside the completed baseline grammar/depth, admitted by
   current semantics/guard/witness rules, proved by the fixed template ladder,
   and shown useful after independently checked bounded integration. Use R03f/c
   preflight probes for that witness; their costs are part of the preflight and
   its output cannot claim AI superiority. The witness must lie in a predeclared
   expanded cell marked **ENUM_BUDGET_EXHAUSTED** at the campaign-scale floor
   above, not a completed or underfunded cell. Give the
   non-AI comparator exactly that expanded grammar and matched budgets; it may
   find the witness without exhausting the cell, so this verdict licenses a
   discovery experiment, not an AI-win claim.

   **Termination/reclassification rule:** visit cells only in the frozen order
   and stop at the first qualified SEARCHABLE witness, completed census, or
   total preflight limit. If a witness-containing cell instead reaches
   ENUM_COMPLETE within its frozen cap, it provides no beyond-enumeration
   discovery evidence: classify that cell as finite enumeration/search
   efficiency, subject to separate proof/utility completion. If all qualified
   opportunities in the visited scope have that status, the track is an
   efficiency study, not discovery-SEARCHABLE. Without a qualified SEARCHABLE
   witness, unresolved in-scope candidates that could change a coverage/no-effect
   verdict leave it INCONCLUSIVE; exhausted cells without a qualified witness
   also do not establish SEARCHABLE. A witness outside the
   frozen lattice is OUT_OF_SCOPE for this campaign. Do not repeatedly increase
   depth/budget after discoveries; any further lattice is a new preregistered
   preflight retaining the earlier result. A later efficiency study can use a
   separately declared smaller budget, but cannot relabel the completed census
   as missing syntactic coverage. Do not expand the universal proof interface
   to manufacture the intersection.
6. Freeze a separate shortcut verdict: verify expansion under current before
   premises and measure actual work/proof savings even when final code is equal.
   Compare against enumerated existing-rule compositions; a new-rule witness
   does not by itself admit a shortcut campaign, or vice versa.

Keep the census as a compact table of counts, reduced witnesses and dispositions;
do not build an unrestricted external-IR importer or a new benchmark framework.
R03h may screen unproved utility, but native code/materialization evidence in
this gate still requires proof and bounded integration. The pilot may therefore
report separate provisional INCONCLUSIVE and later evidence-backed verdicts.

For native results, failure to reject a difference is not evidence of equal
performance. Predeclare the smallest meaningful effect, precision target and
multiple-comparison handling for the policy inventory. If intervals are too
wide to bound that effect, return `INCONCLUSIVE`; only the predeclared calibration
or retry protocol may add samples. A/A noise is a measurement limit, not a
license to assert a zero upper bound. Policy-space completeness is a separate
structural claim from whether the pilot workloads exercise its distinctions.

If this gate fails, close the experiment with that negative result and route
implementation to R03e, R03f/g or another explicitly motivated extension as
appropriate to the failed track. Do not spend
model budget or enlarge the test set until a favorable effect appears. A revised
control surface gets a new manifest/preflight; the failed study remains intact.

Optional attribution-only `A_diag/B_diag` disable selected *optimization* folds
in lowering while keeping semantic constant evaluation, preprocessing, type
checking and required initializers unchanged. This requires an explicit tested
switch/profile; it is not the shipped baseline or a headline benefit claim.
Compare these two variants against each other, never B against deliberately
weakened A_diag and call the difference an EqSat release gain.

### 5.3 Variants and causal attribution

| ID | Rules | Policy | Purpose |
| --- | --- | --- | --- |
| A | EqSat off | Existing lowering/backend optimizations retained | Incremental benefit/cost of EqSat; not an unoptimized compiler |
| B | Current admitted registry | Current fixed policy | Reproducible control |
| C | Same as B | AI-assisted candidate | Policy effect |
| D | B plus admitted rules/shortcuts | Same as B | Rule-package effect at fixed resource caps, including budget competition |
| E | Same as D | Candidate policy | Combined effect |
| N | Same admitted search space as its AI comparator | Non-AI evolutionary/random or bounded enumeration | Added value of AI |
| G / L | GCC-14 / Clang-19 | Declared `-O2`; separate size profile if measured | External context, not AI attribution |

EvoCC variants share one compiler build, backend, source flags and dependencies;
only the experimental factor changes. Do not compare `-O0` to `-O2 -Oeq-scalar`
and attribute the whole delta to EqSat: optimization macros can change source
paths. Audit preprocessed-input identity or label the difference as a separate
end-to-end comparison. Preserve linker/CRT/library identities and link options
where possible; otherwise limit the causal claim explicitly.

Size and speed are separate profiles. Match evaluation/resource budgets for N
and AI; disclose model inference and proof/discovery costs separately from
end-user compilation. More search effort must not masquerade as AI benefit.

Define the common search unit as an evaluator invocation of one canonical
candidate on the entire frozen development suite, with identical compile/proof
caps. Report attempted proposals, invalid/duplicate proposals, unique evaluations,
cache hits, successes, timeouts and total CPU/wall time separately. Freeze the
duplicate-cache charging and stopping rule; neither method gets free hidden
evaluations or retries. Model tokens/spend and proof-worker CPU are additional
budgets, not interchangeable with candidate count. For a finite weight space N
includes exhaustive enumeration; for new-rule discovery use the same integration
and proof budgets as well as utility-evaluation counts.

More rules can increase matching visits/CPU without consuming additional
certificate quota; longer accepted proofs or more accepted regions can instead
hit the certificate-admission caps sooner. Distinguish those effects from the
earlier transient sweep caps and the per-rule/epoch firing limit. Report scoped
hit rates, accepted region/proof-node counts, matcher work, verified-prefix
lengths and outputs under Section 3.3's units. Do not infer matcher exhaustion
from proof-node exhaustion or vice versa. The primary comparison freezes the
legacy coupled cap mapping and separate work/watchdog limits. A high-cap
sensitivity study records **every scope changed** by a legacy knob, or uses a
separately versioned decoupled interface; it does not attribute that combined
change solely to certificate capacity. Retain interaction and regressions rather
than calling every D–B difference the mathematical utility of a rule.

### 5.4 Frozen manifest and measurement protocol

Commit the manifest under `tests/contracts/` before running a campaign:

- HEAD plus scoped dirty/untracked identity, compiler/checker hashes, policy,
  registry/spec/toolchain identities, sources/datasets and every effective argv.
- Object/dependency coverage, result predicate, timing boundaries, warm/cold
  process/cache regimes, ISA/ABI, UB-check profile, lowering-pass profile and
  verified optimizer-chain identity; requested and effective settings separately.
  Checked/unchecked twins bind the same canonical verified-IR digest and immutable
  chain record/bundle digest, plus their separate native artifacts and input-bound
  checked execution receipt.
- Host CPU/OS, runtime image/libc, native/VM lane, affinity/resource policy,
  clock, available counters and environment controls.
- Variant and split manifests, seeds, scoped optimizer counter/ceiling mapping,
  separate watchdogs, repetitions, ordering, estimators and thresholds.
- Preflight grammar lattice/order, per-cell and total resource caps, enumerator
  identity/completion records and stopping/reclassification rules; track-total
  `B_AI`, multiplier `k` and each cell's evaluation cap/consumption/floor check;
  versioned rejection-reason schema and checker adapter protocol.
- Disk/resource caps, retained artifacts, failure/retry rules and objectives.

Start with A/A calibration, excluded from final claims, to choose adequate
workload duration/repetitions. Freeze the protocol **before** seeing candidate
performance. Run balanced/interleaved blocks with a recorded seed, consistent
warmup and one heavy job. Do not discard a run because it is slow. Keep noisy
or interrupted attempts with reasons; predeclared retries append linked attempts.

Retain raw samples, median, dispersion and paired confidence intervals for the
declared estimator. Tail latency needs enough independent request samples and
a declared quantile estimator, not p99 from ten process runs. Report all cases
and worst regressions. Aggregates use frozen weighting and same-metric ratios,
never mix seconds, bytes and RSS in an unexplained score. Do not repeatedly use
final-test uncertainty to select candidates.

Use native AArch64 on a controlled declared machine when available. A fixed
hardware VM yields a scoped VM result with host interference recorded. TCG
timings are emulator results, not physical-CPU optimization evidence. Do not
compare variants across machines. Emitted instruction counts are deterministic
for a fixed artifact. Retired-instruction counts measure a dynamic execution;
they require a frozen input, counter scope, privilege filters, calibration and
control of nondeterministic paths. PMU counts are **not inherently deterministic**
fitness and do not substitute for timing or correctness. A deterministic
instrumented execution count may be a development metric once its counting
contract and repeatability are tested. Unavailable counters are NOT_MEASURED,
not zero.

Follow [LLVM measurement guidance](https://llvm.org/docs/Benchmarking.html) for
noise control and repeated measurements; reuse applicable
[test-suite facilities](https://llvm.org/docs/TestSuiteGuide.html) rather than
timing every short or I/O-dominated correctness test as a compiler benchmark.

### 5.5 Metrics and promotion criteria

#### V1 size contract (S1, revision 7)

The primary observation is a **relocatable target object per TU**, not a linked
executable or a source/IR operation count. Sum the sizes of `.text` and `.text.*`
SHT_PROGBITS sections with SHF_ALLOC | SHF_EXECINSTR and without SHF_WRITE.
Reject a named text section with incompatible flags/type. Other executable
sections are reported separately, not silently added to that denominator.
Report `.rodata` plus `.rodata.*` (allocated, non-writable, non-executable
PROGBITS), in-text literal pools/data, other allocated read-only sections,
writable/zero-fill/debug/file bytes separately. Linked `.text` is secondary.

For AArch64, first partition text bytes into mapped instruction words, layout
padding, mapping-symbol data, and unclassified bytes; only then divide
instruction bytes by four. `$x`/`$d` mapping symbols and sized STT_FUNC extents
provide the object-level partition. Literal pools and other in-text data (for
example jump tables) cannot be distinguished by mapping symbols alone and are
explicitly reported together, **not counted as instructions**. Layout padding
means recognized NOP/zero fill between sized function extents with no intervening
non-mapping symbol. Interior NOPs, including internal alignment NOPs, remain
static instruction slots: object bytes alone cannot distinguish those from
intentional/patchable NOPs. Missing mapping information yields unclassified
bytes and a NOT_MEASURED total instruction count, not guessed instructions.
This is a static layout count, never an executed-instruction count. Never call
every NOP padding or a literal-pool move an instruction elimination. The primary
text total includes **all** of these bytes and is unaffected by the partition.

Require two builds with **identical complete object bytes**, including
relocations, before admitting a new TU measurement. Equal section sizes alone
do not establish repeatability. Preserve source, compiler, flags, verified IR,
chain and checked/unchecked profile identity from the existing capture contract.
Historical single-build census analysis stays explicitly single-build evidence;
it is not upgraded to revision-7 repeatability by recomputing its sizes.

Report checked and unchecked separately. For each program family, record A/B
text-byte totals, the distribution per TU (including unchanged TUs), the worst
regression, changed-TU count, eligible-root denominator and byte delta per
changed TU / eligible root (N/A for zero). Never average across program families.
Eligible roots are counted on the common input IR, not across repeated chain
transactions. Do not attribute baseline DCE or UNVERIFIED_DIAGNOSTIC guard
folds to verified EqSat. R02c compares these sizes on the same six TUs and full
applications against GCC-14 / Clang-19 at `-Os` and `-O2`; unchecked EvoCC is the
release-context comparison and checked is labeled a diagnostic-contract build.
There is no parity threshold. G2 vmlinux `.text` A/B is recorded alongside R01's
two builds, without a separately scheduled kernel build.

The existing ELF helper implements this contract as `evocc-tu-size-v1` while
preserving frozen `evocc-elf-size-v1` receipts. The three observed metric hazards
(padding inflation, literal-pool migration, section misclassification) have
focused contract tests. No new audit layer or experiment identity is added.

| Metric family | Required separation |
| --- | --- |
| Product time | Startup vs steady state; completion vs request latency; result validation |
| Size | Executable sections, read-only/writable data, zero-fill memory and stripped file bytes; debug info separate |
| Product memory | Process peak RSS and, where instrumentable, live heap/stack; not VM/container allocation; instrumentation cost separate |
| Compiler resources | Wall/CPU time, peak RSS, frontend/EqSat/backend stages, checker time and certificate bytes at fixed parallelism |
| Optimizer health | Pre-/post-lowering and residual opportunities, IR change/materialization, guard counts, scoped search/admission/EVP counters and hits, telemetry failures and reproducibility |
| Discovery resources | All candidates, proof successes/rejections/unknowns, model tokens/cost if available, CPU/time and failed evaluations |

**Mechanical proof accounting.** For EVP transaction attempts, partition the
records using Section 3.2; counts are per attempt, not per unique candidate:

- Proof successes: verification VERIFIED and result not ROLLED_BACK_ERROR.
- Recovery proof successes: verification VERIFIED and result ROLLED_BACK_ERROR,
  recorded separately as `recovery_proof_successes` (zero when absent in legacy
  no-recovery captures). They are successful diagnostic replays, never
  publishable optimizer successes. A producer failure sets the chain-level
  rejection reason to PRODUCER_ERROR; a proof rejection retains its specific
  checker reason. NONE cannot explain a rejected chain.
- Completed proof rejections: REJECTED with PROOF_INVALID only.
- Proof unknowns: REJECTED with CHECKER_TIMEOUT, CHECKER_UNAVAILABLE or CHECKER_ERROR.
- Bundle/obligation failures: REJECTED with CERTIFICATE_MALFORMED (checker exit 2),
  BUNDLE_MISMATCH or CERTIFICATE_MISSING; never merge exit 2 with PROOF_INVALID
  (checker exit 3).
- Not checked: NOT_CHECKED; distinguish producer rollback without a certificate
  from pending checks. Neither is an implicit proof success.

These disjoint buckets cover the transaction verification records; display
total REJECTED as the sum of the three rejection buckets, not as a theorem-failure
count. EqSat-off identity verification is a separate chain-only counter, not a
transaction proof success. Count chain/telemetry errors and publication failures
separately; do not fabricate PROOF_INVALID for a chain mismatch. Linked retries
retain their original outcome and cost; a later pass cannot erase an earlier
failure/unknown. New-rule Lean ProofAdmission statistics have their own candidate
and template-attempt denominator and proved/rejected/unknown outcomes; do not
mix those with EVP replay counts or call template UNKNOWN a disproved equality.

Each profile has one primary objective and explicit secondary bounds. Retain
nondominated size/time/memory alternatives; faster-but-larger is not universally
better. Size means emitted bytes, not an IR operator-count proxy. On AArch64,
instruction bytes are four times instruction count, while section sizes can
also include padding, literal pools and veneers. Report that decomposition;
bytes and static instruction count are highly correlated, not independent
confirmation of benefit. Memory may not improve from local scalar rewriting;
report that instead of substituting size.

Record **materialization rate** at a fixed TU granularity: the number of
verified IR-changed TUs with changed emitted code/relocation semantics divided
by all verified IR-changed TUs (N/A if zero). Also report identical-code TUs and
instruction/size-change distributions: changed machine code can have the same
size. Keep raw object hashes and a versioned code comparison including relocation
targets/addends and layout; exclude debug/build IDs only in that explicitly
named comparison. Do not normalize away actual padding, addresses or relocation
changes that affect execution. Many-to-one IR-to-machine effects are not a
root-level speedup count.

V1's frozen census used a deterministic emitted-byte objective with resource
bounds and an explicit tie-break. No V1 adaptive search is now admitted. Static/deterministic
execution counts can supplement it when qualified. Unverified shadow estimates
are only screening data. The current VM runtime lane is closed as
NOT_MEASURABLE_ON_THIS_LANE; its frozen A/A and A/B results are not rerun or
retuned. If no
deterministic objective distinguishes candidates, record a tie and revisit the
control surface; do not fit timing noise. Size-based selection alone does not
promise latency gains.

Separate outcomes: `CORRECTNESS_REJECTED`, `MEASUREMENT_INVALID`,
`RESOURCE_LIMIT`, `NO_SIGNIFICANT_GAIN`, `REGRESSION`, `TRADEOFF`, `IMPROVEMENT`.
Keep timeouts/refusals in the frozen denominator. Speedup summaries disclose
their complete-case population and cannot hide unsupported programs.

Do not invent a GCC parity threshold. R02 first delivers an honest comparison;
"within X% of GCC" requires a separately frozen product target. Promotion needs
zero correctness/proof failures **and affirmative verification of every required
bundle/chain** for the promoted artifact and its required evaluation set. Zero
PROOF_INVALID counts alone do not qualify: unknown, missing, mismatched or
pending required checks block promotion. Rejected discovery proposals remain
in search-cost denominators; they are not installed rules or promoted artifacts.
Also require the declared primary improvement and all secondary bounds. Choose
thresholds from calibration/product needs before
search, never after seeing a favorable candidate.

## 6. Search, data separation and promotion

Maintain training/development, tuning/validation and final hold-out manifests.
Split by program family as well as inputs where possible; different seeds alone
do not prove application generalization. The three-family pilot is feasibility
and attribution work, not a statistically supported cross-family generalization
study. The existing generated-seed hold-outs test those generators, not unseen
applications. A broad family-level claim needs a separately sized, frozen
population and inference unit; otherwise report only the named pilot cases.
Track source/IR digests and feedback
exposure. Once shown to a proposer, results are no longer unseen. Public
benchmarks may already be in model training; disclose that limitation and retain
fresh held-out workloads/inputs for the claim.

Kernel acceptance and protected final inputs never enter fitness/prompts. Keep
the evaluator's `.build`/`.evidence` and kernel-cohort refusal; use explicit
admitted development manifests, not forbidden modules copied to another path.
Public correctness regressions can be development controls, but final acceptance
data remains protected. Repairing after a final-test failure is allowed;
retesting exposed data is a regression check, not a new unseen evaluation.

```text
PROPOSED -> WELL_TYPED -> NO_COUNTEREXAMPLE_FOUND
         -> optional UNVERIFIED_DEV utility screening
         -> PROVED (rules only) -> INTEGRATION_VERIFIED
         -> VERIFIED_DEVELOPMENT_MEASURED -> FROZEN -> HELD_OUT_MEASURED
         -> PROMOTED or NOT_PROMOTED
```

R03h is an optional, isolated **shadow evaluator** for ranking candidates before
expensive proofs. It may count pattern matches and estimate extraction costs
on copied development graphs. Its types, output schema, cache namespace and
storage are separate from trusted optimizer state. It emits only explicitly
unverified utility/counterexample records: no installable registry, proof
certificate, codegen input, executable or native performance score. No unsound
union or speculative fact reaches the certified graph. An installation gate
rejects every `UNVERIFIED_DEV` artifact even when it has a plausible digest.

Only survivors consume the bounded proof/manual-integration budget. A shadow
estimate can be wrong and is not evidence of an optimization gain. R03h can be
omitted for a tiny manual batch; then pre-proof screening is testing/matching
only, not a permission to run unproved native code. This removes mandatory
proof-before-utility ordering without weakening the production trust boundary.

Proof timeout/unsupported theorem is `UNKNOWN`, not falsehood or soundness.
Retain compact rejected/inconclusive records. Promotion is a reviewed change
to a pinned configuration, not a model editing production defaults. Preserve
the old policy for rollback.

Begin with effect-ceiling enumeration, not paid weight search. If R03e is needed,
its first extension makes the existing costs select among multiple legal
representatives, with deterministic tie-breaks, cycle-safe bounded extraction,
and fresh independently replayable certificates. Reuse permitted operations and
before-witness premises; do not infer DAG sharing or register-allocation savings
from a sum of local costs. Test two same-class emitting alternatives whose order
of insertion is opposite to the cheaper cost; changing only the weights must
select the intended one and leave semantic checking independent of the cost.
Also test ties, cycles, shared subexpressions, overflow and budget stops.

This producer-side work need not change the trusted rule semantics if existing
certificates can express its choices. Any required new witness, matcher or
certificate feature receives its own review instead of silently widening the
checker. Rule masks, epoch allocation or region ordering can be subsequent
bounded revisions if actual opportunity data warrants them. Each revision repeats
the preflight. None is required simply to measure the current B baseline.

## 7. Implementation sequence and RED gates

Every row follows RED -> shared repair -> focused tests -> affected original
replay -> changed gate. Do not rebuild the whole kernel per candidate.

| Step / TODO | Deliverable | Required RED / exit evidence |
| --- | --- | --- |
| P0 / R01a | Shared policy-aware persistent chain; closed state/rejection-reason tables; scoped counters; checker-before-install | Illegal state/reason tuples; checker exits 2/3 distinguished in records/counts; every rejection reason injected; missing-certificate success versus rollback; checker outage counted as unknown; valid recovery proof cannot erase apply/history failure; lost transient counters; budget partials; driver/evaluator parity; rejected chain publishes nothing |
| P1 / R02a | Bench manifest, collector and separate receipt | Wrong binary/argv/data; omitted UB/lowering profile; missing result; stale receipt; invalid metrics; lock/cleanup ownership |
| P2 / R02b | Default-limit hit table; selected-pair gate census; separate rule/shortcut ceilings; frozen grammar lattice/stop and campaign-scale enumeration floor; materialization; A/A and A/B | Search cap confused with certificate cap; unconsulted weights counted as choices; completed/underfunded cell labeled discovery; k below 1; wrong evaluation units; earlier proof/time cap or increased AI budget bypasses floor; crash labeled exhaustion; post-hoc lattice expansion; fuzz labeled complete; external semantic mismatch; UNKNOWN reported empty; negative verdicts retained |
| Profile / R02d | Checked/unchecked MIR fan-out from one verified IR/chain; native profile plumbing | Ignored flag; changed source semantics; missing profile identity; checked trap preserved; equal manifests with different IR or chain rejected; EqSat-off identity control; missing/failed twin blocks unchecked measurements |
| Domain / R02e | Separately reviewed UB-predicate folding design and implementation | False guard folded true; poison bypass; other uses lost; obligation removed without proof; no automatic P0 allowlist expansion |
| P3 prerequisite / R03e | Conditional cost-aware multi-candidate extraction/control surface | Cheaper alternative ignored due to insertion order; cycle; overflow; tie nondeterminism; invented premise; redo effect ceiling before search |
| P3 / R03a | Data-only policy candidates; AI/non-AI search only after useful-space gate | Unknown fields; budget overflow; wrong transaction limit; forbidden corpus; deterministic replay mismatch; duplicated/free hidden evaluations |
| P4/P5 prerequisite / R03f | Explicit bounded manual rule integration; generic machinery only as separately gated follow-on | Producer/checker disagree on substitution/type/guard; unreviewed registry digest; injected before constant; one-sided new rule acceptance |
| Optional / R03h | Data-only unverified shadow utility evaluator | Unverified result enters codegen/registry/certified graph; cache collision; tainted facts or shadow metric labeled verified |
| P4 / R03b | Derived shortcuts with expanded existing-rule proofs | Missing **before-witness** constant; wrong substitution; escaping guard; poison; extra unrecorded transaction; fuel savings assumed from shorter spelling |
| P5 / R03c | Rule-track preflight; trusted fixed-order template ladder; manually reviewed rules | Candidate template-selector injection; altered ladder/actual-template identity; changed proposition; unauthorized axiom; wrong domain; stale audit; native-backed fact dependency mislabeled kernel-only |
| Conditional / R03g | Explicit bounded-domain theorem/audit/profile extension before finite-width templates | Width zero/>64; independent shift width missed; enabled Rust instance outside proved predicate; incomplete enumeration admitted as universal proof |
| P6 / R01 | Frozen full G2 | Two identical Image hashes; all optimized TU proofs independently checked; object provenance; exact-image boot/shutdown; explicit coverage/fallback |
| P7 / R02c, R03d | Broad comparison and held-out A-E/N evaluation | No held-out tuning; losses/timeouts retained; declared statistics/resource bounds; actual original-program outputs |

Mutation tests change one field at a time: RHS, width, guard, relation, registry
digest, trusted template-ladder/actual-template ID, policy weight, transaction
limit, scoped budget identity, rejection reason, enumeration stop/frontier,
UB/lowering profile, checked-twin receipt/common chain digest,
IR/object identity, workload hash and expected
result. Reject semantic/provenance forgeries. A legitimate changed policy gets
a distinct experiment identity, not rejection merely for different costs.
All out-of-P0 effect/FP/metadata fields survive reconstruction unchanged.

Reuse `crates/evocc-eqopt/tests/`, `crates/evocc-proofcheck/tests/`,
`crates/evocc-policy/tests/`, `tools/lean/tests/`, `tools/ladder/tests/` and
`tests/fixtures/`. New CLI switches/schema paths are implementation work, not
runnable commands in this document. This design-only change does not bump IR
semantics; real registry/format changes require their own revision.

P0-P2 infrastructure/development measurements can run on correctness-admitted
workloads without waiting for unrelated language breadth or every kernel test.
This does not reorder final acceptance: R01 precedes final R02, then R03.
P3-P5 provide development evidence until those gates close. Kernel/broad runs
remain serial under existing ARM64 priorities.

The table is a work-package inventory, not a requirement to finish AI discovery
before G2. The dependency graph is:

```text
R01a ---------------------------------> R01 (current rules suffice)
R01a + R02a -> R02b preflight ----------> R02c (+ R01 and R02d profiles)
                 |
                 +-- useful space ----> R03a policy search
                 +-- degenerate ------> R03e -> repeat preflight, or stop study

R02b rule/shortcut census + R03f ------> bounded preflight witness qualification
                    + track verdict -> R03b derived-shortcut campaign
                    + track verdict -> R03c fixed-template new-rule campaign
R03h shadow utility (optional) --------> screen candidates before either track
R03g bounded-domain extension --------> finite-width template route only

R02c + relevant frozen R03a/b/c tracks -> R03d held-out evaluation
R02e UB-predicate domain: separate reviewed extension, not an implicit shortcut
```

Derived/new-rule experiments also require the measurement runner for any
performance claim. R03a may deliver a policy-only improvement without R03c,
if the preflight supports that experiment. R03b and R03c share integration
prerequisites; neither requires completion of the other's discovery track.
The bounded qualification needed to resolve a rule preflight is not a bulk
R03c campaign; it may use R03f/c template and integration infrastructure before
the track verdict is frozen. Every track must pass its own verdict before
discovery spending, regardless of another track's success.
A conditional task may be closed as not needed only with an explicit scope
decision and evidence, not marked implementation PASS.

## 8. Resource and artifact policy

Reuse the checkout, source caches and runtime image. No workspace clone, kernel
tree or large VM disk per candidate. Use the existing heavy-run lock. Preflight
free space/reserve and projected artifacts; refuse a run exceeding the manifest's
disk cap. Set caps from current inventory, not an old free-space estimate.

Commit manifests, candidate definitions/proofs, compact receipts, raw timing
samples and failure summaries. Stream/deduplicate large proof artifacts by
digest where useful; retain actual before/after/proof bytes required for replay,
not hashes alone. Keep baseline/winner/reproducer binaries. Retire losing build
products only after ownership/process checks; record when historical exact
binaries are no longer retained. Never relabel old receipts.

## 9. Research and decisions

No broad deep-research report blocks chain/provenance repair, measurement
controls, effect-ceiling enumeration or the small manual/fixed-template pilot.
The review identifies concrete engineering gaps, not a need to rediscover EqSat.

| Question | Required next evidence | Blocking scope |
| --- | --- | --- |
| Bounded Lean instance interface | Exact predicate, all enabled Rust instances included, independent-width negatives, audited fixed template dependencies | R03g/finite-width candidate proofs only; universal templates can proceed |
| Reliable benchmark host | A/A calibration and declared native/VM/resource/counter inventory | Hardware speed claim, not size/correctness work |
| Richer extraction/cost features | Enumerated behavior classes, post-lowering opportunities, code materialization and budget interactions | May block a meaningful R03a campaign; R03e before search if needed |
| Rule-integration throughput | Measured manual batch cost; independent AST decoder/matcher REDs if generalized | High-throughput Q3 only; do not pretend manual handlers are an automated pipeline |
| UB predicate folding | Exact total-predicate semantics, poison propagation and independently checked guard discharge | R02e domain expansion only, not baseline performance measurement |
| Arbitrary Lean source | Pinned isolation/export/recheck and adversarial definition/import/audit tests | V2 only; excluded from this implementation sequence |
| Memory/contextual/FP/whole-CFG rewrites | Separate state/effect/refinement model and proof design | Outside V1; architecture review before enablement |

If R03g or a generic matcher is pursued, request a bounded architecture review
of the actual proposition/coverage mapping or A3/A4 delta, with reduced examples
and mutation evidence. Do not request another general AI + EqSat survey or
assume existing reflection evidence authorizes a new trust class.

Before a frozen performance campaign, choose the host, primary objectives,
resource/regression bounds and any remote-model budget/data authorization.
Use calibration to propose concrete values. Those choices need not block
infrastructure; no favorable threshold may be chosen after measurement.

## 10. References and claim ceiling

- [Roadmap](archive/roadmap.md) (archived): architecture intent; this document supplies the
  bounded Q2/Q3 implementation path, not a whole-roadmap completion claim.
- [Lean plan](archive/lean-plan.md) (archived): the history of the fixed rule semantics, analysis
  and correspondence. The contract is now [`spec/verification.md`](../spec/verification.md).
- [Lean proof validation](https://lean-lang.org/doc/reference/latest/ValidatingProofs/):
  theorem meaning and proof replay; moving documentation does not qualify our pin.
- [Ruler](https://homes.cs.washington.edu/~djg/papers/oopsla2021.pdf): automated
  rule discovery as a methodological baseline, not proof of EvoIR rules.
- [LLVM benchmarking](https://llvm.org/docs/Benchmarking.html) and
  [test-suite guide](https://llvm.org/docs/TestSuiteGuide.html): measurement practice.

This revision delivers design/task decomposition only: no new production rule,
theorem, speedup measurement or R01/R02/R03 closure. Start implementation with
P0/R01a and the effect-ceiling pilot; report results only from actual receipts.

### 10.1 Review disposition

This table records the first review's revision-2 decisions, retained for provenance.

| Review item | Revision-2 resolution |
| --- | --- |
| 1: degenerate policy space | Section 5.2 mandatory enumeration/stop gate; R03e conditional prerequisite; exact class count remains unmeasured |
| 2–3: UB checks and pre-lowering optimization | Sections 5.1–5.3 explicit profiles, actual unchecked gap, post-lowering census, attribution-only controls; R02d/R02e separated |
| 4–5: flat status, partial commits, verification authority, chain mismatch | Section 3.2 orthogonal transaction/stop/root fields and wrapper-owned verification; retained partial budget work; whole-chain error rejection; shared runner |
| 6: nonexistent proof domain | Section 4.1 current universal proposition retained; R03g required before bounded-width admission |
| 7: kernel-only shortcut and arbitrary scripts | Section 4.4 data-only candidates, trusted templates, transitive trust audit; arbitrary scripts V2 |
| 8: hand-written matching | Section 4.3/R03f bounded manual V1 with costs; generic matching is a separate TCB project |
| 9: proof before utility | Section 6/R03h optional non-installable shadow screening before proof |
| 10: derived premises/fuel | Section 4.2 before-witness constant rule and measured work, not assumed savings |
| Experimental/statistical follow-ups | Sections 5.3–5.5 explicit evaluation units, cap effects, deterministic fitness, qualified counters, machine materialization; Section 6 pilot claim ceiling |
| Small contract fixes | Section 3.1 relation exactly `eq`, search seed provenance; Section 3.2 deterministic release rejection; Section 7 corrected dependency graph |

This is closure of review **in the design**, not measured policy diversity,
implementation of unchecked mode, a new Lean theorem, a generic matcher or an
optimization/performance PASS.

### 10.2 Follow-up review disposition (revision 3)

| Follow-up item | Resolution and remaining evidence |
| --- | --- |
| 1: rule-track ceiling | Sections 1 and 5.2 require separate policy/rule/shortcut verdicts; freeze grammar and residual census; beyond-enumeration claim needs a proved useful residual witness, not assumed `simp` closure or fuzz completeness |
| 2: budget meanings | Section 3.3 separates transient search, certificate-admission quota, EVP markers/fuel and actual matcher work; first preflight output is default-limit hit evidence; legacy knobs remain explicitly coupled |
| 3: template selection | No candidate template selector; fixed trusted ladder; ProofAdmission records actual attempts/success; mutation/identity REDs |
| 4: redundant state | Section 3.2 provides a closed legal-combination table, transition and cross-field constraints; schema must reject its complement |
| 5: no-op versus fallback | Producer telemetry is a measurement trust boundary; inject apply/history failure, preserve error despite valid zero proof; eligible no-rule control remains a legal no-op |
| 6: unchecked eligibility | Section 5.1 requires a matched successful checked twin per variant/input; necessary evidence, not a universal no-UB theorem |
| 7: byte-fitness ties | Section 5.2 states single-instruction tie expectation and tests surrounding-code exceptions; no adaptive timing escape hatch |
| 8: MIR documentation drift | EvoMIR Section 5 now marks unchecked as specified but not implemented in the inspected 0.54 path, with R02d and the admission rule linked |

These are specification/planning corrections. No scoped budget census,
enumeration, new schema implementation or native experiment has run in this
documentation revision; the corresponding TODO gates remain open.

### 10.3 Follow-up review disposition (revision 4)

| Follow-up item | Resolution and remaining evidence |
| --- | --- |
| 1: fixed representative before policy | Sections 2.1/5.2.2 explicitly predict selected-pair acceptance/rejection only; trace actual pairs and inequalities, not hypothetical add/shl preference; R03e is still required for alternative selection |
| 2: discovery termination | Section 5.2.3 freezes a finite grammar lattice and caps; only a useful proved witness in a healthy budget-exhausted cell qualifies discovery-SEARCHABLE; completed cells imply finite enumeration/efficiency, failures remain inconclusive, out-of-scope depth requires a new preflight |
| 3: closed rejection causes | Section 3.2 refines the state table with a closed reason enum, evidence and precedence; success-without-proof versus explicit rollback is specified; Section 5.5 derives disjoint success/invalid/unknown/contract-failure counters and promotion rules |
| 4: hard checked-twin identity | Section 5.1 requires one canonical IR and immutable verified chain/bundle feeding both native profiles; mismatches reject even with equal manifests; EqSat-off uses an explicit identity record, not a fictitious proof |

Implementation order remains R01a shared verification chain, then R02a/R02b
measurement infrastructure and preflight. R02b's first result is the scoped
**default-limit saturation table**, not an AI campaign. This revision closes
these design comments only; the proposed schemas, preflight and native-profile
fan-out still require their TODO implementation and acceptance evidence.

### 10.4 Final design corrections (revision 5)

- Restore EvoIR Section 11.4's existing exit distinction: CERTIFICATE_MALFORMED
  is exit 2 and a bundle/obligation failure; PROOF_INVALID is exit 3 and a
  completed proof rejection. The checker already supplies this information.
- Require each discovery-qualifying cell's enumeration cap to be at least the
  planned whole AI track's evaluation budget, or a predeclared factor `k >= 1`.
  Use identical evaluation units and require actual exhaustion at that scale;
  a smaller quota or earlier resource stop is only inconclusive preflight data.

No new architecture review, checker semantics or campaign is introduced by
these corrections. R01a and then R02b's default-limit saturation table remain
the next implementation and measurement steps; neither is closed here.

---

# Historical appendix: August 31 research discussion

The text below is preserved verbatim for provenance, not current authority.
In particular, its old backend status, expert/agent statistics, suggested axiom
promotion and unconditional safety claims for policy search are superseded by
the bounded design above. It is not a current to-do list or a new approval request.












|---|---|---|
















| | rule | cost model | schedule |
|---|---|---|---|














