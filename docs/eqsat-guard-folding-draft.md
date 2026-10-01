# R02e guard-predicate folding — proposed semantic delta

Draft 1 — 2026-10-01. **Exact first slice OWNER_APPROVED.**
The normative integration is EvoIR 1.9.116 Section 8.12 and `g0.ub.v1` in
Section 11.6. Production acceptance remains open. P0 stays unchanged; no
second slice is authorized, designed or scheduled.

## Evidence and scope

The [S4 diagnostic](notebook/eqsat-size-work-packages-20261001.md#s4-diagnostic-result--frozen-not-a-verified-optimization)
recovers 952 / 1,608 / 80 checked text bytes in zlib / zstd / cJSON. Their
post-S2 P0 savings are 472 / 1,440 / 732 bytes. The preregistered negative stop
rule is not met. Diagnostic bytes are not verified benefit or promised yield
for a certified profile; no diagnostic was linked, executed or installed.

This is deterministic proof-carrying guard folding, not AI discovery. R03 stays
frozen. No division/remainder, memory, pointer, FP, vector or whole-CFG rewrite
is added; `ub.assume` is never an e-node. No new audit layer is proposed.

## Total meanings, restating existing semantics

U_N is unsigned decode; S_N is two's-complement signed decode. L_N=-2^(N-1),
H_N=2^(N-1)-1. Each predicate produces `bits<1>`; arithmetic operands share N.

| Root | True exactly when |
| --- | --- |
| `divisor_nonzero(y)` | U_N(y) != 0 |
| `signed_div_rem_no_overflow(x,y)` | not (S_N(x)=L_N and S_N(y)=-1) |
| `signed_add_defined(x,y)` | L_N <= S_N(x)+S_N(y) <= H_N |
| `signed_sub_defined(x,y)` | L_N <= S_N(x)-S_N(y) <= H_N |
| `signed_mul_defined(x,y)` | L_N <= S_N(x)*S_N(y) <= H_N |
| `shift_count_defined.unsigned(x,k)` | U_M(k) < N |
| `shift_count_defined.signed(x,k)` | 0 <= S_M(k) < N |

Count width M is independent of N; decode before narrowing/machine masking.
The shifted value remains an operand: the interpreter reads it, so replacing
the predicate must not erase poison observation. Legacy `signed_shl_defined`,
FP conversions and other obligations remain excluded. No withdrawn signed-left-
shift obligation is reintroduced.

## Separate profile and fact transfers

Use `g0.ub.v1`, separate from P0, with the reviewed text/digest in Section
11.6. Only the seven typed root forms
above are added. Witness operations remain existing P0 pure integer forms.
For the first slice, every operand cone/witness/seed passes the existing P0
exclusion and all-edge poison fixed points, except the permitted predicate
root itself. Loads, effects and earlier guard results cannot become new opaque
seeds; a predicate cannot admit another guard cone transitively. N/M are 1..64;
zero/wider/mismatched widths are ineligible, never truncated.

This is deliberately narrower than the diagnostic. Report retained predicates;
do not relax admission to recover its headline bytes. Facts come only from
existing normalized known bits and the independent all-edge seed table, never
branch conditions, assumptions, runtime success or producer assertions.

Add guard-profile-only transfers that return exact true (mask=1,value=1) under
the following sufficient tests, otherwise TOP (0,0). No false fold is proposed.
A=[a_lo,a_hi], B=[b_lo,b_hi] conservatively contain every signed value of their
bit facts, including both signs when the sign bit is unknown. Use mathematical
integer calculations; i128 suffices for the 1..64 endpoint products in Rust.
Precisely, for a normalized fact (m,v) at width N, set
u=((2^N-1) xor m), s=2^(N-1). If u has the sign bit, its signed interval is
[S_N(v or s), S_N(v or (u and (s-1)))]; otherwise it is
[S_N(v), S_N(v or u)]. The maximum unsigned value is v or u. These formulas
are part of the transfer contract, not a producer-selectable range heuristic.

| Transfer | Sufficient test |
| --- | --- |
| nonzero | divisor has a known-one bit |
| div/rem | lhs excludes INT_MIN or rhs excludes the all-one -1 pattern |
| add | [a_lo+b_lo,a_hi+b_hi] lies in [L_N,H_N] |
| sub | [a_lo-b_hi,a_hi-b_lo] lies in [L_N,H_N] |
| mul | min/max of four endpoint products lie in [L_N,H_N] |
| unsigned count | maximum represented M-bit unsigned count < mathematical N |
| signed count | M-bit sign known zero and maximum represented unsigned count < N |

Do not compare with `const.bits<M> N`, which can wrap. Unknown unsigned bits<2>
is valid for a 64-bit shift; the signed fact includes negatives and is not
proven valid. Count 64 for width 32 is invalid, not masked zero.

Each manually integrated rule uses the existing form
`<total_predicate>(...) => known_const when fully_known`. No new side-condition
vocabulary or arbitrary script is needed. The new proof obligation is soundness
of these transfers; `fullyKnownValue` supplies equality from a sound exact result
fact. Give each typed root a separate rule ID/pattern/sorts/signedness, and assign
the reviewed text/digest and typed Lean data together.

## Independent proof and discharge

Reuse EVP and the standalone checker, selecting guard-only eligibility/replay
by registry. The old P0 path still rejects guards. Record exact before witnesses,
sorts/attributes, fact premises, derived constant and one-root extraction.
Checker eligibility, seed derivation, transfers and rule handlers are independent
of the optimizer/diagnostic. Stronger forged premises are rejected.

Replace only the predicate instruction with `const.bits<1> 1`, retaining its
result identity. Other uses, witnesses, `ub.assume`/reason, effects, block args,
terminators, symbols and module fields stay unchanged under whole-module splice
comparison. The certificate does not authorize deleting EvoIR obligations.
Malformed bundles stay exit 2; well-formed false claims stay exit 3.

Separately specify checked-MIR selection: an obligation directly consuming
canonical `const.bits<1> 1` needs no trap branch. False constants and other
conditions keep their original checked behavior/reason. No emitter-side fact
analysis, diagnostic ID map or unchecked run establishes truth. Source-authored
true constants are trivially safe but not certificate/guard-folding successes.
Rewritten predicates require checker-verified chains before codegen. Keep
conservative IR effects; do not weaken summaries.

Use the existing shared chain/staging entry point with an explicit guard profile,
existing policy/registry/transaction identity fields and no second fallback or
installation path. Old scalar chains/captures keep their policies/counts. Verify
once, then fan the same output IR/chain out to checked/unchecked MIR. No production
default changes before review and acceptance.

## Lean and acceptance

Extend typed semantics/pattern elaboration with the exact predicates and
independent N/M/signedness. Use `RuleCovered rule_<id>` and fixed templates;
prove transfer soundness against `gamma` over all Rust-enabled instances.
A larger universal domain is acceptable, an unrecorded finite-width assumption
is not. Keep native-backed transfer dependencies visible in existing receipts.
Each rule follows R03f's spec/digest, Lean, producer, independent checker,
negative/mutation and original-replay checklist. Extend existing semantic target
lists only for this actual domain, not a new general audit/search service.

REDs: false/unknown guards; INT_MIN/-1; negative/count-N shifts; N/M swaps or
masking; poison across edges/unused shifted inputs; excluded seeds; other users
lost; assumptions used as their own premises; forged sort/signedness/fact/rule/
digest/root; deleted obligations/weakened effects; one-sided integration or old-
P0 authorization. Native false guards must retain their original diagnostic.

Acceptance: full-workspace gate, existing focused tests and Lean gate without
bypasses; both interpreters/native small controls before the six original TUs/
harnesses. Independently replay every proof with zero completed rejections or
bundle failures; timeouts are not PASS. Two objects must be byte-identical under
S1. Report certified guard savings separately from P0/DCE and the diagnostic
ceiling, including retained forms.

**Current:** diagnostic plumbing/regression gate green; the seven parametric
Lean transfer proofs compile. Independent checker/profile integration, full
Lean acceptance, ordinary MIR elision and certified original replay remain
**IN PROGRESS / NOT ACCEPTED**. Approval does not close those gates.
