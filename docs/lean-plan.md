# Lean 4 integration plan for EvoCC (second version)

Date: 2026-09-09. Status: the implementation version after two rounds of external review. The repository implements L0–L4 according to this document; L5 waits for the kernel milestone and is then decided by measurement.
Basis: the team brief (`docs/notebook/lean-semantics-brief.md`), the two rounds of replies from the review panel on 2026-09-08, the local measurements of Section 0, and the independent exhaustive check by the review panel in Section 0b. Anything not marked "measured" is a plan, not a fact.

This document is self-contained. Appendix A is the current registry, unabridged. The probe source files are `lean/probes/probe2.lean` and `lean/probes/probe3.lean`.

## Changes in the second version relative to the first

1. `RuleSound` becomes a multi-width interface: each variable has its own bit width, and the rules, terms, fact environment and transfer functions share one set of type information. Different widths no longer go into one `BitVec n` (Section 4).
2. The result-class fact R stays a separate premise. The plan states the complete chain of A2, B, A3 and A7 on which it depends, and explicitly forbids using the fact of the merged class to prove the same merge (Section 4).
3. The quantifier of `MeetSound` is corrected to an existential (Section 4).
4. Coverage has two kinds: segment A covers rule instances (`S(r) ⊆ P(r)`), and segment B covers the analysis instances used to produce facts. The plan states who enforces "an uncovered instance is not enabled", and an old digest implies no new enabling condition (Section 2).
5. The shift-amount width M does not change the registry semantics. The proof splits on "whether the fact is fully known / how `k.toNat` relates to N", and does the bitwise transfer only on `0 ≤ s < N` (Section 4). 128-bit shift amounts are in scope, and L3 adds the corresponding vectors (Section 6).
6. The audit no longer passes a theorem on an axiom-name pattern. Instead, it fixes the modules and the allowed imports, collects the actual transitive dependencies and records the declaring module and the type, rejects unknown declarations, and checks the complete proposition (Section 2, R3).
7. The promise of the generator becomes "a fixed `RuleSound` prevents the prover from replacing the definition of correctness; the correspondence from text to data still depends on the trusted decoder, the round-trip check and independent review" (Section 3).
8. The three inductive conditions of L3b state an explicit direction and coverage requirements, with a negative test. Its claim is lowered to "independent checking evidence" (Section 6).
9. The acceptance of L1/L2 is rewritten: each receipt row must correspond to an existing theorem, the correct parameter space, a fixed proposition and the actual dependencies. "The only risk is time" becomes "not yet verified". "Primitives only, abbrev required" is lowered to a preprocessing choice of the proof script (Section 6).
10. The formulas for `or`, `lshr`, `ashr` and `icmp.ne/sle/ule`, which the first version gave only as "dual", are written out in full for checking (Section 4).

---

## 0. Facts measured on this machine (2026-09-08)

Toolchain and axioms (`lean +leanprover/lean4:v4.32.2`, available offline; the machine default toolchain is 4.25.0, and the project pins the version with `lean-toolchain`):

| Theorem shape | Proof method | `#print axioms` report |
|---|---|---|
| `x + 0 = x`, `x * 1 = x` (bits<32>/<64>) | `bv_decide`, closed in the preprocessing stage, SAT not called | `[propext]` |
| `x * 2 = x + x` (bits<64>) | Same as above | `[propext, Quot.sound]` |
| `x.slt x = false`(bits<64>) | `bv_decide` | `[propext, Classical.choice, Quot.sound]` |
| known-bits transfer rows (see below) | `bv_decide`, SAT + LRAT | The three above plus one native axiom named after the theorem: `<theorem name>._native.bv_decide.ax_1_5` |

On 4.25.0, the same file reports `Lean.ofReduceBool, Lean.trustCompiler` on the SAT path. Axiom names change between versions, so the audit cannot depend on a single name (Section 2, R3).

Measured and passing (the two probes take 15.3 seconds in total; this is the measured subset, not all rows and all widths):

- Tier A rules at 32/64 bits; `mul(x,2) = x+x` at 64 bits.
- Transfer rows, all at 64 bits: `and`, `add`, `mul`, `icmp.ult`, `icmp.slt` (both sign bits known), `shl` (shift amount fully known and below the width, M = 64); `zext`/`sext`/`trunc` at 32↔64.
- The shift amount is decoded at its own width: `decide` decides `(1#8) <<< (256#16) = 0#8`. Lean defines the shift of a `BitVec` by a `BitVec` as `x <<< k.toNat`, which agrees with EvoIR 3.4.
- Preprocessing in the proof script: when a transfer function is a plain `def`, `bv_decide` treats it as an opaque variable and gives a spurious counterexample. The proof passes if `simp only [defs]` unfolds it first, or with `abbrev`. This is a preprocessing choice of the tactic; the semantic definitions can stay plain `def`.

Current state of the repository:

- Current registry `p0.scalar.v4`: 55 rules, digest `1a5391…9d74` (Appendix A). Six kinds of condition: `true`, `all_const`, `fully_known`, `covers`, `known_zero`, `width_min(?x,2)`. Of these rules, 18 `known_const` rules can instantiate the same lemma with the correct result type, and 12 `fold` rules are definitional equalities under the Lean definitions. The receipt still lists each rule.
- Normalization of known-bits facts exists already: spec section 11.3 states that a premise is malformed if its value has a one where the mask is zero, and the checker's `Fact::from_premise` (`crates/evocc-proofcheck/src/facts.rs`) rejects it. The internal representation of the checker is `(zeros, ones)`, which is in bijection with `(mask, value)`.
- The repository has no `.evp` file. Tests generate the triples in a temporary directory (`write_case` in `crates/evocc-proofcheck/tests/day8_corpus_replay.rs`). The driver has only `-Oeq-scalar`/`-Oeq`.
- The seed-facts fixed point is in `crates/evocc-proofcheck/src/seed_facts.rs`: `entry_arguments` (entry top), `by_current` (worklist), `carry` (propagation along edges), `by_whole_passes` and `agreement` (oracles for tests only).
- Spec section 8.5: the root and the witness are `bits<N>`, `1 ≤ N ≤ 64`; the two operands of a comparison have the same width; a shift amount is "any `bits<M>`"; both ends of a cast are in bounds. Instance parameter space: one N for a unary/binary rule; one N for a comparison; (N, M) for a shift; (W, N) for a cast. The original spec sentence gives the shift-amount width M no upper bound, but lane B measured that the two independent eligibility implementations, in the producer and in the checker, both limit it to 1..64, and the fact lattice has only a 64-bit mask. On this basis, spec revision 1.9.87 sets M of the P0 profile to 1..64 (a wider shift amount makes the region ineligible; the interpreter still executes it as 3.4 states). 128-bit roots, casts and shift amounts are all outside the P0 profile. 128-bit shift amounts are still part of the language semantics, and the L3 named vectors stay for the interpreter.

## 0b. Independent exhaustive check by the review panel (2026-09-08, external evidence)

The review panel used an independent Python program to check the formulas of Section 4 of the first version exhaustively at bit widths 1–4: 120 normalized facts and 7,380 pairs of same-width facts; the relations of `meet`, conflict, `join`, the order and concretization; the and/or conclusions of `covers`; 69,904 concrete inputs each for `and`, `xor`, `add`, `sub`, `mul`, `icmp.eq`, `icmp.ult` and `icmp.slt`; 115,600 inputs for `shl`, with N and M each taken independently from 1–4; 448, 448 and 1,252 inputs for `zext`/`sext`/`trunc`. It found no counterexample, and all output facts were normalized. Not checked: `or`, `lshr` and `ashr`, which the first version gave only as "dual", and the unexpanded `icmp.ne/sle/ule`; Section 4 of this version writes them out in full. An exhaustive check at small bit widths is not a proof; it verifies neither the registry decoding nor the Rust/Lean correspondence.

---

## 1. Decision

Lean 4 is only an offline proof layer for the P0 scalar semantics, the registry rules and the known-bits analysis. It does not change the IR, the host language, the e-graph or the independence of `evo-proofcheck`, and it is not a prerequisite of the kernel milestone. The goal of Stage 1 is to reduce manual trust in the existing rules and analysis formulas. It is not a verified C compiler, and it is not a new formalization framework.

## 2. Coverage, audit, equality: implementing the four corrections

**R1 Coverage has two kinds, and both live in the registry and the receipt.**

Let `S(r)` be the set of instances that a digest-bound registry allows, and `P(r)` the set of instances that the proof covers. To mark the whole registry as proved, `S(r) ⊆ P(r)` must hold for each rule r in it. This is segment A.

Segment B: the analysis instances that produce the fact premises of rules must also be covered. Counterexample shape: `shift<N=8, M=128>` gives a `Fact 8` through its transfer, and this fact is a premise of a later `add`/`xor`/`covers`/`known_const`. The later rules are all in the proved range, but the uncovered shift analysis still took part in the premise. Narrowing only the shift rewrite rules does not close this gap. The acceptance object of segment B is therefore "each transfer that the current P0 actually uses is proved over the required parameter range". The first choice is to make the M dimension of shifts parametric (Section 4); if that is not possible, keep the side-path, partial-coverage claim. After spec revision 1.9.87 limits M to 1..64, the `shift<N=8, M=128>` of the counterexample shape cannot enter the e-graph at all in either of today's two implementations, and complete segment B coverage for P0 is N and M each in 1..64. If partial analysis coverage is to be used at run time in the future, an unsupported analysis branch must conservatively return top, or an explicit additional trust assumption must stay. This plan builds no fact provenance tracking.

Enforcer: the old registry stays in use for the existing flow, and the receipt marks its coverage as incomplete. For a Lean-gated version, either complete the whole instance space, or publish a new registry with narrowed conditions (the certificates bind the new digest, and the producer and the checker must interpret the new side condition in the same way). The separate Lean gate can fail, but it does not make the kernel build fail. An old digest implies no enabling condition that the registry does not state.

**R2 The three guarantees stay separate.** Three theorem families, three receipt segments: A `RuleSound`; B `TransferSound`/`MeetSound`/`JoinSound`/predicate lemmas; C `Correspondence` (differential test evidence only). Full coverage of segment A does not mean full coverage of segment B; the status of segment C does not stand for A or B.

**R3 Audit: fixed proof code, a checkable dependency report.** An axiom-name pattern cannot prove a legitimate origin: a custom axiom can take the same name, and when a proof reuses a native-backed helper lemma, the final theorem inherits the axioms named after the helper lemma. The audit procedure is therefore:

1. The audit covers the fixed modules `lean/EvoIR/**` and the proof scripts in the repository. The only allowed import is Lean core. No external proof input is accepted.
2. After `lake build`, enumerate all `axiom` declarations in the environment. For each declaration other than the three core axioms (`propext`, `Classical.choice`, `Quot.sound`), record the name, the declaring module and the type. Reject a declaration if its declaring module is not one of our proof modules, or if its type does not have the shape of a `bv_decide` reflection axiom.
3. For each target theorem, collect the transitive dependencies from `#print axioms`, write them to the receipt, and mark the theorem `kernel-only` (the three core axioms only) or `native-backed`.
4. Check the complete proposition: the type of the target theorem must match, character for character, the `RuleSound (rule "p0_001")`/`TransferSound op T` (with the parameter space) that the audit script generates itself from the registry data. A check of only the name and the axioms does not pass.
5. The receipt binds: the registry digest, the spec revision number and the `lean/` source commit, the Lean toolchain version, and the name and proposition text of each target theorem.
6. State the trust assumptions: the audit script, the fixed Lean modules, the Lean kernel and the compiler (on the native path) are trusted. The procedure does not apply to arbitrary untrusted proof programs; to accept them in the future, design an isolation separately.

**R4 P0 is conditional equality.** The conclusion of `RuleSound` is an equality. The plan introduces no refinement relation and does not change the symmetric e-class. A future rule with a `ub.assume` premise is first treated as a conditional equality within its scope, and a one-way transformation takes a separate, directed verification step. Neither is in this plan.

## 3. Structure

Directory `lean/`, Lake package `EvoIR`. It depends only on Lean core (`Std.Tactic.BVDecide`) and does not import Mathlib. `lean-toolchain` pins `leanprover/lean4:v4.32.2`. The build is offline.

```text
lean/EvoIR/Sem.lean        P0 opcode semantics: one denoteOp, a BitVec function parameterized by type
lean/EvoIR/Term.lean       small term syntax (typed): Var | Const | App; denoteTerm only recurses and then calls denoteOp
lean/EvoIR/Fact.lean       Fact n, γ, order, meet/join, six side-condition predicates, transfer functions (types follow op)
lean/EvoIR/Rule.lean       rule data type, WellTyped, the fixed RuleSound definition
lean/EvoIR/Registry.lean   generated: the registry text decoded into rule data (not propositions)
lean/EvoIR/Proofs/*.lean   theorems: rules, transfer rows, meet/join, predicate lemmas
lean/EvoIR/Eval.lean       executable evaluator (lake exe evoir-sem), for differential tests
lean/probes/*.lean         the original probe sources of Section 0
tools/lean/registry_to_lean.py   extracts the registry block from the spec, checks that its SHA-256 equals the digest in the spec, decodes it into Registry.lean, and renders the data back to text for a byte-for-byte comparison with the original
tools/lean/audit.py              the six steps of R3; writes tests/contracts/lean-p0-receipt.json
tools/lean/vectors.py            generates differential vectors (the L3 format of Section 6)
tools/lean/diff.py               compares the outputs of the two sides
tools/gate-lean.sh               separate gate, not on the kernel path
```

One `denoteOp` plus a small term syntax; the first step does not represent the CFG, memory or C. The generator produces only rule data, and the propositions come from the fixed `RuleSound`. A fixed `RuleSound` prevents the prover from replacing the definition of rule correctness. It does not verify that the decoding is faithful: if the decoder decodes a rule as `lhs = lhs`, the fixed `RuleSound` correctly proves this wrong rule. The correspondence from text to rule data still depends on the trusted decoder (A6), the round-trip check and independent review. The round-trip check shows only that the two conversions agree with each other.

## 4. Theorem statements

**Typed interface.** Each variable has its own bit width, and both sides of a rule have a result bit width. Data of different widths do not share one `BitVec n`. Below is the structure, not compiled Lean:

```text
Γ : Var → Nat                          -- bit width of each variable
Term Γ n                               -- term of result width n: Var v (Γ v = n) | Const n c | App op args (the signature of op gives the argument types)
r : Rule Γ out                         -- lhs : Term Γ out; rhs : Term Γ out or known_const; cond
σ : (v : Var) → BitVec (Γ v)
F : (v : Var) → Fact (Γ v)
R : Fact out

RuleSound r :=
  ∀ σ F R,
    (∀ v, γ (F v) (σ v)) →               -- the fact of each variable holds
    γ R (denoteTerm r.lhs σ) →           -- the fact of the result class holds (pre-merge state)
    CondHolds r.cond σ F R →             -- the side condition evaluates to true on F and R
    denoteTerm r.lhs σ = denoteRhs r.rhs σ R
```

The implementation can use an intrinsically typed inductive type, or a plain AST plus a `WellTyped` predicate. `WellTyped` checks the operation signatures (the two operands of an arithmetic/bitwise operation have the same width; the two operands of a comparison have the same width and the result has width 1; a shift amount has any M; `zext/sext` requires W ≤ N; `trunc` requires N ≤ W; a same-width cast is the identity). The same σ handles a nonlinear pattern (`add(?x,?x)`).

**Semantics.** `denoteOp` follows spec sections 3.3–3.6: `add/sub/mul` are `+ - *`; `and/or/xor`; `shl x k = x <<< k.toNat`, `lshr x k = x >>> k.toNat`, `ashr x k = x.sshiftRight k.toNat` (`k : BitVec M`; when `k.toNat ≥ N`, the results are 0, 0 and the sign fill, measured to agree with 3.4); `icmp.*` returns `BitVec 1`; `zext/sext/trunc` are `zeroExtend/signExtend/truncate`.

**Facts.** `Fact n := { mask value : BitVec n // value &&& ~~~mask = 0 }`; `γ K x := x &&& K.mask = K.value`. `fullyKnown K := K.mask = allOnes`; `knownZero K := K.mask = allOnes ∧ K.value = 0`; `covers M X := (~~~(X.mask &&& ~~~X.value)) &&& ~~~M.value = 0`.

**Order and merge.** `K ⊑ K' := K'.mask &&& ~~~K.mask = 0 ∧ K.value &&& K'.mask = K'.value`. `meet K K'`: if `(K.value ^^^ K'.value) &&& K.mask &&& K'.mask ≠ 0`, then `conflict`; otherwise `some {mask ||| mask', value ||| value'}`. `join K K' := {mask &&& mask' &&& ~~~(value ^^^ value'), value &&& that mask}`.

```text
MeetSound     : γ K x → γ K' x → ∃ K'', meet K K' = some K'' ∧ γ K'' x
MeetConflict  : γ K x → γ K' x → meet K K' ≠ conflict
JoinSound     : γ K x → γ (join K K') x (and the symmetric case)
```

**Transfer signatures and soundness.**

```text
T_add T_sub T_mul T_and T_or T_xor : Fact N → Fact N → Fact N
T_icmp_*                           : Fact N → Fact N → Fact 1
T_shl T_lshr T_ashr                : Fact N → Fact M → Fact N
T_zext T_sext                      : Fact W → Fact N   (W ≤ N)
T_trunc                            : Fact W → Fact N   (N ≤ W)
TransferSound op T := ∀ x y A B, γ A x → γ B y → γ (T A B) (op x y)   (likewise for unary operations; the types follow op)
```

Row formulas (`m` and `v` are mask and value; `kz(K) = K.mask &&& ~~~K.value` is the known-zero set, and `K.value` is the known-one set):

```text
and  : m' = (mA & ~vA) | (mB & ~vB) | (mA & mB);            v' = vA & vB & m'
or   : m' = (mA & vA) | (mB & vB) | (mA & mB);              v' = (vA | vB) & m'
xor  : m' = mA & mB;                                        v' = (vA ^ vB) & m'
add/sub : t = mA & mB;  m' = (~t & (t+1)) - 1;              v' = (vA ± vB) & m'
mul  : z(K) = ~kz(K) & (kz(K)+1);  m' = z(A) * z(B) - 1;    v' = 0
icmp.eq  : both fullyKnown and vA = vB → one; (vA ^ vB) & mA & mB ≠ 0 → zero; otherwise top
icmp.ne  : (vA ^ vB) & mA & mB ≠ 0 → one; both fullyKnown and vA = vB → zero; otherwise top
icmp.ult : lo(A) = vA, hi(A) = vA | ~mA; hi(A) <u lo(B) → one; lo(A) ≥u hi(B) → zero; otherwise top
icmp.ule : hi(A) ≤u lo(B) → one; lo(A) >u hi(B) → zero; otherwise top
icmp.slt / sle : the two rows above, plus the condition mA.msb = mB.msb = true, with a signed comparison; otherwise top
shl  : see the split below; when s < N, m' = (mA <<< s) | ((1 <<< s) - 1), v' = vA <<< s
lshr : when s < N, m' = (mA >>> s) | ~(allOnes >>> s), v' = vA >>> s
ashr : when s < N, m' = mA.sshiftRight s, v' = vA.sshiftRight s (when the sign bit is unknown, the high bits of m' are 0 automatically; normalization guarantees vA.msb = 0)
zext : m' = zeroExtend mA | ~zeroExtend allOnes_W;           v' = zeroExtend vA
sext : m' = signExtend mA;                                  v' = signExtend vA
trunc: m' = truncate mA;                                    v' = truncate vA
```

**Proof split for shift transfers (M has no upper bound; the registry semantics do not change).** Let the shift-amount fact be `K : Fact M`:

1. `¬ fullyKnown K`: the transfer returns top, and soundness holds directly.
2. `fullyKnown K`: `γ K k` gives `k = K.value`, so `s := k.toNat = K.value.toNat`.
   - `s < N`: prove the bitwise transfer of the table above. For a fixed N, `0 ≤ s < N` has only finitely many cases; if necessary, enumerate s instead of all M-bit shift amounts.
   - `s ≥ N`: return top, as the current analysis defines.

The comparison must be the natural-number comparison `k.toNat < N`; it must not first truncate N to `BitVec M`. The shift-amount decoding lemma holds for any positive M. The time cost of this split has not been measured on 4.32.2.

**Rules.** The right side `fold.op` of an `all_const` rule is `denoteOp` itself, so the theorem is trivial, and its content goes to segment C. A `known_const` rule uses the conditional lemma `fullyKnown R ∧ γ R t → t = R.value`. R stays a separate premise and is not forced to be `R = T_op(F)`, because the fact of the result class can come through meet from other terms already proved equivalent. The complete chain that makes R trustworthy:

```text
A2 / A7   seed facts hold in the actual program
B         Lean lemmas for transfer, meet and the predicates
A3 / A7   replay associates facts with the correct class and uses the pre-merge state in the correct order
   ⇒ R does describe the value of this lhs
   ⇒ RuleSound: this known_const rewrite holds
```

The forbidden cycle: first merge the lhs with a constant, then take R from the merged class, and finally use R to prove the same merge. Stage 1 does not prove the replayer, but R is explicitly a premise that depends on the pre-merge state. `covers` rules: `γ M m → γ X x → covers M X → x &&& m = x` (the `or` dual: `x ||| m = m`). `width_min(?x,2)` is the premise `2 ≤ n`.

**Width coverage.** Prefer the parametric form (`∀ n`); where a parametric proof fails, enumerate `n ∈ 1..64`. Shift rules and transfer rows are parametric in M (the split above); casts enumerate the legal (W, N).

## 5. Trusted base and assumptions after Stage 1

| ID | Object still trusted | Guarantor | Reduced by this plan |
|---|---|---|---|
| A1 | Section 8.5 eligibility and taint: every value that enters the e-graph is total, poison-free and free of side effects | Rust, separate implementations in the producer and the checker | No |
| A2 | Function-level seed-facts fixed point (entry top, every edge, back edges, unreachable blocks) | Rust, `seed_facts.rs`, recomputed independently by the checker | No; L3b adds independent checking evidence |
| A3 | The checker's replay, extraction and reassembly, including the association of facts with the correct class and order | Rust | No; a Stage 2 target |
| A4 | Section 9 parser (shared) | Rust | No |
| A5 | Lean kernel; on the SAT path, the per-theorem native axioms and the compiler | Lean 4.32.2 | New, disclosed |
| A6 | The registry decoder and the Lean semantic definitions themselves; the audit script | Generator + manual review | New; round-trip comparison |
| A7 | Agreement of the Rust semantics (interpreter, fold, transfer) with the Lean semantics | Differential test evidence | New; segment C reports evidence only |

Manual trust that the Lean proofs replace: the mathematical correctness of the 55 registry rules; the soundness of the Section 8.10 transfer table, meet/join and the four predicates.

## 6. Deliverables, order, acceptance

Run only one heavy job at a time. The kernel milestone and the front-end lane are not affected.

**L0 Real triples (first; does not wait for the Lean package).** Add an example to `evocc-eqopt` that, for a given `.evo`, writes `before.evo / after.evo / proof.evp` and the event counts. Take one case from the reduced Csmith cases and one from the `eq.c` demo, and seal them, with the exit status of `evo-proofcheck`, in `tests/fixtures/certificates/`. Acceptance: checker status 0; the event, region and rewrite counts go into the README of that directory.

**L1 Semantics, rules, audit (Lean lane).** `Sem/Term/Fact/Rule/Registry`, the theorems of the 55 rules (parametric first, then enumerated), `audit.py` and receipt segment A. Acceptance: `lake build` is green, with no `sorry`; each receipt row corresponds to an existing theorem, the correct type parameter space, a fixed target proposition (the audit generates it itself and checks it character for character) and the actual transitive dependencies; full coverage of segment A does not mean coverage of segment B. A ten-minute build is the target.

**L2 Analysis theorems (same lane).** Each transfer row (with the shift split), `MeetSound`, `MeetConflict`, `JoinSound`, the `covers`/`knownZero`/`fullyKnown` lemmas; receipt segment B. Section 0 measured only some rows, at 64 bits and at 32↔64. Automation for the other rows, the other widths and any M is not yet verified. The measured results stay and are not extrapolated.

**L3 Correspondence evidence.** Vector format (shared by the two sides; JSON Lines, one case per line; numbers are hexadecimal strings, without width truncation):

```text
{"kind":"value","op":"shl","n":8,"m":16,"args":["0x1","0x100"]}
{"kind":"value","op":"icmp.slt","n":32,"args":["0x80000000","0x0"]}
{"kind":"value","op":"sext","w":8,"n":32,"args":["0x80"]}
{"kind":"transfer","op":"add","n":4,"facts":[{"mask":"0xc","value":"0x8"},{"mask":"0xf","value":"0x3"}]}
{"kind":"transfer","op":"shl","n":8,"m":128,"facts":[...]}
{"kind":"meet","n":8,"facts":[...]}   {"kind":"join","n":8,"facts":[...]}   {"kind":"covers","n":8,"facts":[M,X]}
```

Each side copies the input unchanged, appends `"result"` (a value is hexadecimal; a fact is `{"mask","value"}`; a meet conflict is `"conflict"`; a predicate is `true/false`), and writes `<name>.lean.jsonl` / `<name>.rust.jsonl`; `diff.py` compares them line by line. Vector sets: transfer functions, exhaustive over normalized facts at width ≤ 4 (3^4 = 81 facts per operand); binary value operations, exhaustive over 65,536 pairs at width 8; random vectors at widths up to 64, with emphasis on uncommon widths, shift amounts of independent width, boundary shift amounts (N-1, N, N+1, 2N), sign bits, same-width and cross-width casts, and conflicting facts. Named vectors (expected values from the EvoIR definitions, not C semantics):

```text
shl(1 : bits<8>, 256 : bits<16>) = 0
shl(1 : bits<8>, 0 : bits<128>) = 1
shl(1 : bits<8>, 1 : bits<128>) = 2
shl(1 : bits<8>, 2^64 : bits<128>) = 0
ashr(128 : bits<8>, 2^64 : bits<128>) = 255
```

Also cover the shapes M = 1, N > M and M > N. Rust side: compare both the producer's `evocc-eqopt::known_bits` and the checker's `facts.rs` (which reads the vectors in a unit test inside the crate); for value operations, compare the interpreter of `evocc-evoir`. Receipt segment C records only the numbers of vectors, passes and differences, and marks them as test evidence.

**L3b (optional).** Add an inductive-invariant check of the seed-fact table to the checker. The three conditions state an explicit direction in the order where "more precise is smaller":

```text
Entry:        InitialFacts ⊑ I(entry)                 -- with no added premise, InitialFacts = top, so the entry-argument table can only be top
Instruction:  Transfer_op(I(operands)) ⊑ I(result)
Edge:         RenameAndCarry_edge(I(source)) ⊑ I(destination arguments)
```

Over the actual CFG, the checker covers every entry, instruction and edge that is structurally reachable from the entry (back edges included). It does not trust the table's own unreachable claims. It treats a missing fact as top or rejects it. Negative test: the loop argument is 0 at the entry, and the back edge passes `x+1`, but the table claims "the loop argument is always 0"; the checker must reject this table. Claim: if the checker is correct and its coverage of entries and edges is complete, it rejects a fact table that does not satisfy the inductive conditions. This stage adds independent checking evidence, not a formal proof of A2. It does not block L1/L2.

**L4 Spec and wording (architect).** 8.10 clarifies "a fully known constant is a minimal element, not the unique bottom; a conflict is not a fact"; a new 11.7 "Trusted base" (the table of Section 5); the two-kind coverage principle of R1 goes into 11.6; README/roadmap adopt the wording of Section 8.

**L5 (after the kernel milestone; decided later from measurements of the L0 triples).** No preselection of Aeneas or of full replay. The two-level statement "region replacement / back into the module" of Section 8 of the review is the target theorem framework of Stage 2.

## 7. Point-by-point response to the second review round

| Review section | Response |
|---|---|
| 1 Multi-width interface | Adopted: Γ, `Rule Γ out`, dependently typed σ/F, a separate R, transfer signatures by op; `MeetSound` becomes existential; the chain of R adds A3; the ban on circular reasoning is written down. |
| 2 Coverage | Adopted `S(r) ⊆ P(r)`; the enforcer and the old-digest rule are explicit; new analysis-instance coverage in segment B; no provenance tracking. |
| 3 M has no upper bound | Adopted: no registry change; the proof splits on whether the fact is fully known and on `k.toNat`; 128-bit shift amounts are in scope, and the vectors are completed. |
| 4 L3b | Adopted the explicit direction, the CFG coverage, the negative test and the lowered claim. |
| 5 Audit and generator | Adopted: the six-step procedure, rejection of unknown declarations, the check of the complete proposition, the four receipt bindings; the generator promise is rewritten. |
| 6 Acceptance | Adopted: row-by-row check for L1, "not yet verified" for L2, `def` kept, 30 of the 55 rules instantiate shared lemmas. |
| 7 Exhaustive-check record | Included as Section 0b; the unchecked formulas are written out in full. |

## 8. What may be written after L1–L3

May write: "The 55 P0 scalar rewrite rules listed in `p0.scalar.v4` are machine-checked against the Lean 4 model of the EvoIR scalar semantics, within the bit widths and conditions that the receipt lists; the known-bits transfers, merge and predicates of 8.10 are checked as well; the build report gives the exact proof dependencies (including the native axioms of the SAT path); the agreement between the Rust implementation and the Lean model has only differential test evidence."

May not write: verified C compiler; kernel compilation correctness is proved; the Rust checker is verified; all widths are proved (if any instance is uncovered); a rule is proved and therefore faster.

## 9. Two open points

1. Static decision of segment B coverage: if the shift transfers are parametric in M and the other rows enumerate N ≤ 64, segment B is complete for P0. If the enumeration of s in the shift split is too slow at some N, how is the gap stated?
2. The implementation of audit step 4, "check the complete proposition": the audit script generates the expected proposition text from the registry data and compares it character for character with the `#check` output. Is a stronger method necessary (a check inside Lean with `example : type_of thm = expected := rfl`)?

---

## 11. Implementation record

### 2026-09-09: L0, L3, L3b merged into main

- L0: `crates/evocc-eqopt/examples/certificate_triple.rs` produces the triple and `counts.json` from `.evo` or `.c`. Three cases are sealed in `tests/fixtures/certificates/` (`tier-a-identities` 19 regions/88 rewrites, `tier-b-known-bits` 6/8, `shift-and-width-facts` 17/62; premises 10/9/36 respectively), and `evo-proofcheck` exits 0 on each. `crates/evocc-proofcheck/tests/sealed_certificate_fixtures.rs` replays each case, derives `after.evo` and `proof.evp` again from the sealed `before.evo`, and compares them byte for byte (the architect replaced the original "lower again from `source.c` and compare", so that the fixtures do not break when the front end changes; `source.c` and the commit IDs in the README are provenance records). The reduced Csmith case depends on the untracked `csmith.h`, cannot be regenerated on a clean checkout, and is not sealed.
- L3: `tools/lean/vectors.py` generates 1,157,725 vectors (transfer 132,534; lattice 22,140; width-8 value operations 983,040; random 20,000; named 11); three Rust runners (producer `known_bits`, checker `facts` through `evo-facts-vectors`, interpreter) and `tools/lean/diff.py`. Producer against checker: 174,674 comparisons, 0 differences; the 574 `unsupported` answers are all shift transfers with M ∈ {65, 128}. The interpreter answers 983,040 + 5,788 vectors, with 209 `unsupported` (M = 65). All 11 named vectors agree with 3.4; `crates/evocc-evoir/tests/shift_amount_widths.rs` pins them. Permanent regression test `crates/evocc-proofcheck/tests/known_bits_correspondence.rs`: 13,383 vectors, 0 differences.
- L3b: `SeedFacts::inductive` checks the three conditions on every block and edge that is structurally reachable in the CFG (back edges included), and treats a missing fact as top; a violation returns the existing status 3. The negative test (entry 0, back edge x+1, the table claims always 0) is rejected; the randgate corpus has 0 violations in 635 functions.
- Gates: the full-workspace tests have only two failures, and both fail in the same way on the baseline commit (`composite_function_declarations_*`, in progress in the front-end lane); clippy, fmt and ruff pass.

### Seven findings of lane B and their rulings

1. Both eligibility implementations limit the shift-amount width to 64, which conflicts with the original sentence of 8.5. Ruling: the spec follows the implementation (1.9.87), because the two independent implementations agree, the fact lattice is 64 bits wide, and a wider region is only ineligible, not miscompiled.
2. The fact lattice can represent only ≤ 64 bits: consistent with ruling 1. A transfer wider than 64 bits has no Rust counterpart even where Lean has a theorem for it, so segment C makes no claim for it.
3. The interpreter executes only 1–64 and 128 bits (`not-executed: wide-integer`): a known limit, unrelated to P0; recorded.
4. The certificates of the default policy bind `p0.scalar.v3` (`aca1f15b…`), not v4: the receipt binds v4 and records v3 as its byte-for-byte prefix (the generator checks both digests and the prefix relation), so the v4 proofs cover the v3 certificates.
5. `Inst::Const` is 64 bits wide, and a 128-bit constant cannot be written: a 128-bit shift amount can enter only as an argument. After ruling 1, this is unrelated to the proofs.
6. The producer's `join` returns `Option`, and the checker's returns `Fact`: not observable on same-width vectors; recorded as an implementation difference.
7. The instruction condition of L3b is empty on the checker's own table (the table records only block arguments): accepted. It acts only on a table that claims instruction results, and the negative test covers that case; extension to instruction results is left for later.

## Appendix A: full text of `p0.scalar.v4` (from `docs/evoir-spec.md` 11.6)

```text
p0_001_add_zero_rhs add(?x,const_like(?x,0)) => ?x when true
p0_002_add_zero_lhs add(const_like(?x,0),?x) => ?x when true
p0_003_mul_one_rhs mul(?x,const_like(?x,1)) => ?x when true
p0_004_mul_one_lhs mul(const_like(?x,1),?x) => ?x when true
p0_005_and_zero_rhs and(?x,const_like(?x,0)) => const_like(?x,0) when true
p0_006_and_zero_lhs and(const_like(?x,0),?x) => const_like(?x,0) when true
p0_007_or_zero_rhs or(?x,const_like(?x,0)) => ?x when true
p0_008_or_zero_lhs or(const_like(?x,0),?x) => ?x when true
p0_009_double_neg sub(const_like(?x,0),sub(const_like(?x,0),?x)) => ?x when true
p0_010_icmp_eq_self icmp.eq(?x,?x) => bool(1) when true
p0_011_icmp_ne_self icmp.ne(?x,?x) => bool(0) when true
p0_012_icmp_slt_self icmp.slt(?x,?x) => bool(0) when true
p0_013_icmp_sle_self icmp.sle(?x,?x) => bool(1) when true
p0_014_icmp_ult_self icmp.ult(?x,?x) => bool(0) when true
p0_015_icmp_ule_self icmp.ule(?x,?x) => bool(1) when true
p0_100_fold_add add(?a,?b) => fold.add(?a,?b) when all_const
p0_101_fold_sub sub(?a,?b) => fold.sub(?a,?b) when all_const
p0_102_fold_mul mul(?a,?b) => fold.mul(?a,?b) when all_const
p0_103_fold_and and(?a,?b) => fold.and(?a,?b) when all_const
p0_104_fold_or or(?a,?b) => fold.or(?a,?b) when all_const
p0_105_fold_xor xor(?a,?b) => fold.xor(?a,?b) when all_const
p0_106_fold_icmp_eq icmp.eq(?a,?b) => fold.icmp.eq(?a,?b) when all_const
p0_107_fold_icmp_ne icmp.ne(?a,?b) => fold.icmp.ne(?a,?b) when all_const
p0_108_fold_icmp_slt icmp.slt(?a,?b) => fold.icmp.slt(?a,?b) when all_const
p0_109_fold_icmp_sle icmp.sle(?a,?b) => fold.icmp.sle(?a,?b) when all_const
p0_110_fold_icmp_ult icmp.ult(?a,?b) => fold.icmp.ult(?a,?b) when all_const
p0_111_fold_icmp_ule icmp.ule(?a,?b) => fold.icmp.ule(?a,?b) when all_const
p1_200_known_and and(?a,?b) => known_const when fully_known
p1_201_known_or or(?a,?b) => known_const when fully_known
p1_202_known_xor xor(?a,?b) => known_const when fully_known
p1_203_known_add add(?a,?b) => known_const when fully_known
p1_204_known_sub sub(?a,?b) => known_const when fully_known
p1_205_known_mul mul(?a,?b) => known_const when fully_known
p1_210_known_icmp_eq icmp.eq(?a,?b) => known_const when fully_known
p1_211_known_icmp_ne icmp.ne(?a,?b) => known_const when fully_known
p1_212_known_icmp_slt icmp.slt(?a,?b) => known_const when fully_known
p1_213_known_icmp_sle icmp.sle(?a,?b) => known_const when fully_known
p1_214_known_icmp_ult icmp.ult(?a,?b) => known_const when fully_known
p1_215_known_icmp_ule icmp.ule(?a,?b) => known_const when fully_known
p1_220_and_cover_rhs and(?x,?m) => ?x when covers(?m,?x)
p1_221_and_cover_lhs and(?m,?x) => ?x when covers(?m,?x)
p1_222_or_cover_rhs or(?x,?m) => ?m when covers(?m,?x)
p1_223_or_cover_lhs or(?m,?x) => ?m when covers(?m,?x)
p1_224_xor_known_zero_rhs xor(?x,?z) => ?x when known_zero(?z)
p1_225_xor_known_zero_lhs xor(?z,?x) => ?x when known_zero(?z)
p1_206_known_shl shl(?a,?k) => known_const when fully_known
p1_207_known_lshr lshr(?a,?k) => known_const when fully_known
p1_208_known_ashr ashr(?a,?k) => known_const when fully_known
p1_230_known_zext zext(?a) => known_const when fully_known
p1_231_known_sext sext(?a) => known_const when fully_known
p1_232_known_trunc trunc(?a) => known_const when fully_known
p2_300_mul_two_rhs mul(?x,const_like(?x,2)) => add(?x,?x) when width_min(?x,2)
p2_301_mul_two_lhs mul(const_like(?x,2),?x) => add(?x,?x) when width_min(?x,2)
p2_302_add_self_shl add(?x,?x) => shl(?x,const_like(?x,1)) when width_min(?x,2)
p2_303_shl_one_add shl(?x,const_like(?x,1)) => add(?x,?x) when width_min(?x,2)
```
