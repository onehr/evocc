# EvoCC

**An equality-native C compiler, built to answer four questions.** EvoCC compiles GNU C to AArch64 with no LLVM or GCC anywhere in the pipeline — its own preprocessor, parser, IR, optimizer, and backend. Its distinguishing choice is that **equivalence is persistent compiler state**: optimization accumulates alternative forms of a program in an e-graph rather than destructively rewriting it, and every rewrite emits a proof certificate that a standalone checker replays without trusting the optimizer. The target is a pinned Linux 6.18.44 `arm64 virtconfig` kernel that compiles, links, and boots under QEMU with an EvoCC-built `/init`. The compiler is the instrument; the questions below are the experiment.

![The pipeline](docs/images/architecture.png)

## The questions

One principle runs through all four: **separate what must be true from what must be found.** Give the first to machines that cannot be persuaded — sealed oracles, proof checkers, interpreters, a specification that cites its probes. Give the second to machines that are good at proposing and bad at being right — language models and evolutionary search. A single structural decision, an e-class whose members are equivalent by construction plus an independent proof checker, serves every question here. It was built for the second; it turned out to be the governance mechanism for the first and the safety boundary for the last.

**Q0 — Can a specification be the sole authority for an AI executor?**
The claim: an AI executor bound by a versioned specification, sealed oracles, tests-only red windows declared in advance, and the right to *stop* at a specification gap produces a compiler whose errors surface as stops, not as miscompilations. Refuted by a semantic divergence that passes every gate and reaches a published dashboard — or by governance that costs more than the errors it prevents.

**Q1 — Can AI build, from nothing, a compiler that is correct on industrial input?**
Not "state of the art": *correct on the Linux kernel*, layer by layer — layout bit-exact against sealed GCC and Clang, semantics agreed by two interpreters, every rewrite proof-replayed, and finally a kernel that boots. The expert's role in this is judge and designer of measurement, not source of truth; the data so far says the expert is wrong about one assertion in five, and the oracle catches it. Refuted by a class of construct the loop cannot specify and measure, or — most informatively — by a kernel that compiles, passes every gate, and does not boot.

**Q2 — Can superoptimization replace the pass pipeline?**
In its literal form this has a known answer: unguided saturation explodes. The falsifiable form: within a *deterministic* budget (iterations and nodes, never wall-clock), can a proof-carrying e-graph replace the scalar and memory optimization pipeline on the frozen kernel, producing code within a fixed margin of GCC `-O2` on code size first and runtime second? The field's prior is that e-graphs replace the scalar mid-end and nothing more; the part beyond that prior is memory rewrites carrying provenance proofs. Refuted by budget exhaustion on a material fraction of kernel functions, by extracted code outside the margin, or by memory rewrites that cannot carry a checkable proof.

**Q3 — Can the optimizer's policy improve itself?**
The policy is three things — the rule set, the cost model, the schedule — and today all three are hand-written. The question is whether they can be proposed by a language model, selected by evolution, and measured by regression *while correctness stays entirely outside the loop*. The cost model and the schedule search a space that is already verified: every candidate an e-class offers is equivalent by construction, so no tuning can make the compiler wrong. Rules are different — a new rule is a universal claim and must be derived from existing rules, proven, or reviewed before it enters the release set; differential testing alone does not count. Refuted by evolved policies that fail to beat hand-written ones on held-out corpora, or by proposed rules whose survival rate does not justify the cost of verifying them.

![Rewriting in place versus merging into an e-class](docs/images/why.png)

## How

Every claim in this repository is a **measured number with the command that produced it**, replayed from sealed evidence. Language and layout semantics are frozen against a sealed GCC-14.2 / Clang-19.1 container: where the two disagree the specification records `ORACLE_CONFLICT` and takes the answer that refuses rather than voting, and no sentence about oracle-observable behavior ships without its probe. Work runs as a loop — freeze the rule in `docs/evoir-spec.md`, open a tests-only red window whose failures are declared first, implement, gate, publish a kernel-telemetry dashboard from observer-sealed evidence that names the next first-error site — and a specification gap stops execution rather than being guessed at. The specification is authoritative and versioned (revision 1.9.23); the acceptance contract records every ruling and every deferral with the diagnostic code that counts it.

![How the work is driven](docs/images/loop.png)

---

## Roadmap 0.1

Status is measured, not estimated. `done` means a gate is green on sealed evidence.

| Capability | Status | What the gate measures |
|---|---|---|
| EvoCPP preprocessor | done | 16/16 cohort units preprocess clean |
| Lexer, C11 + GNU parser | done | 142 sealed lexer goldens, 1,000-snippet parse corpus |
| Type system and layout | done | 128/128 and 22/22 against the sealed oracles, 0 mismatches |
| C-HIR and semantic interpreter | done | typed UB, provenance, per-byte initializedness |
| EvoIR block-argument SSA | done | both interpreters agree on the differential corpus |
| Semantic lowering | done | UB explicit before any rewrite; front-end forms leave no trace |
| Equality engine and proofs | done | EVP1 replayed by a standalone `evo-proofcheck` |
| Kernel telemetry and publication | done | sealed evidence; refuses to publish on a regression |
| Diagnostic survey | partial | works; cascade ranking and recovery soundness unfinished |
| Kernel parse closure | open | 2/16 cohort units parse clean under both oracles |
| Analysis lattices, bounded scheduler | open | — |
| Randomized semantic gate | open | — |
| EvoMIR, register allocation, AArch64 | open | — |
| ABIs and native flight gates | open | — |
| Kernel codegen, link, boot | open | — |

**EvoCPP** is a full C11 preprocessing-token implementation: pp-numbers including pasted results like `1_MODULE`, stray characters that survive stringification, `_Pragma`, `#pragma pack` applied to layout with its measured numbers, and the sealed GCC-14.2 predefined set of 421 macros copied into the crate and digest-checked, with the feature queries answering from measured tables. **The parser** covers C11 plus the GNU dialect the kernel actually uses — attributes in every position, `typeof` and `__typeof_unqual__`, `_Generic`, statement expressions, designated initializers, compound literals, `asm` including `asm goto`, the compile-time builtin family, `__auto_type`, case ranges, anonymous-member promotion — each construct's semantics measured in the sealed container before it was implemented. **EvoIR** makes memory and observable effects explicit `mem<all>` and `world` tokens so a rewrite cannot silently reorder them; aggregate copies lower to one `copy` instruction that transfers bytes with their initializedness. **The equality engine** applies the reviewed `p0.scalar.v1` rewrite set and emits EVP1 certificates that `evo-proofcheck` replays without trusting the optimizer; memory instructions are opaque to it, and a trap terminator is a report, never a licence to treat a block as dead.

## Layout

```
crates/evocc            driver: -E, --parse-only, --emit=c-hir, --emit=evoir,
                        --interpret-c-hir, --interpret-evoir, --survey
crates/evocc-cpp        preprocessor          crates/evocc-lex     lexer
crates/evocc-parser     C11 + GNU parser      crates/evocc-types   types and layout
crates/evocc-chir       checked high-level IR crates/evocc-memory  memory model
crates/evocc-lower      C-HIR -> EvoIR        crates/evocc-evoir   block-argument SSA
crates/evocc-egraph     e-graph core          crates/evocc-eqopt   rewrites, EVP1
crates/evocc-proof      certificates          crates/evocc-proofcheck  evo-proofcheck
docs/evoir-spec.md      the authority         tests/contracts/     frozen corpora, receipts
tests/acceptance/       sealed-oracle harnesses and the dashboard publisher
tools/                  routing, survey and figure rendering
```

## Verification

Every commit passes the same gate: workspace tests, `clippy -D warnings`, `rustfmt`, Ruff on the Python harnesses, both layout corpora against the sealed oracles, and an isolated offline clone replay. The receipts in `tests/contracts/` say which commit each published number came from; a count on a commit inside an open red window includes declared failures, and that is the point of the window. Nothing is claimed that these do not measure. The figures are generated, not drawn: `python3 tools/render-readme-figures.py` re-renders them.

---

## Progress toward the questions

A scoreboard, not a narrative. Each row is what exists today, what is missing, and what observation would settle the question against us.

### Q0 — specification as sole authority

| | |
|---|---|
| Evidence | 72 dashboard publications, 0 regressions. The executor has stopped at a specification gap six times rather than guess; the architect's unprobed assertions were overturned five times by the oracle or the executor, and once the executor caught a commit whose message did not match its diff. |
| Failure taxonomy so far | unprobed assertion; reference to an undefined construct; rule-versus-fact conflation in a diagnostic; one rule implemented along one code path but not another (five instances); a record that does not match the thing it describes; a symptom fix that changes the success path. Each has a mechanical countermeasure that has since fired. |
| Cost | about one round in five consumed by a specification defect; 23 revisions. The comparison that matters is against debugging a miscompilation inside a kernel that does not boot. |
| Would refute | a semantic divergence that passes every gate and is published. |

### Q1 — correct on the kernel

| | |
|---|---|
| Evidence | preprocess 16/16; dual-oracle parse 2/16 with the frontier at one construct (`E2102 invalid-update-operands`, 26 lanes); layout 128/128 and 22/22 bit-exact; two interpreters agree on the differential corpora; proof replay 100%. The parse count is a conjunction over ~60,000 preprocessed lines per unit, so it lags: twelve distinct blockers have been cleared and republished with zero regressions while it read 2/16. |
| Missing | any machine code; the full-config sweep (~2,500 units against 16); the flight lab (ARM64 VM, Kbuild, QEMU, initramfs); behavioral differential against a GCC-built kernel. |
| Open sub-question | whether the frontier loop that converges at 16 units converges at 2,500; the survey mode is the bet that one run yields the whole worklist. |
| Would refute | a construct class the loop cannot specify and measure; or a kernel that passes every gate and does not boot. |

### Q2 — superoptimization instead of a pipeline

| | |
|---|---|
| Evidence | none that bears on the question. The e-graph, the reviewed scalar rule set, EVP1 certificates and the standalone checker exist and are gated; but the cost is "newly emitted instructions", there is no target model, and saturation is same-block only. |
| Blocked on | a backend and a true cost model. Without them Q2 cannot be measured at all, which makes the backend experimental apparatus rather than a deliverable. |
| Known hard points | inlining — the kernel depends on it, a single-function e-graph cannot do it, a cross-function one explodes; the persistent equality database is the intended answer and is unbuilt. Constructs the kernel expects to be answered *after* optimization (`__builtin_constant_p` under `statically_true`) are answered in the front end here, correctly and at a performance cost that must be charged to this question. |
| Margin | the margin against GCC `-O2` is fixed before the first measurement, not after. |
| Would refute | budget exhaustion on a material share of kernel functions; extracted code outside the margin; memory rewrites that cannot carry a checkable proof. |

### Q3 — a self-improving policy

| | |
|---|---|
| Evidence | none. All three components of the policy are hand-written; the rule set is about thirty rules. |
| Not blocked | learning the cost model and the schedule can start before the backend, with the EvoIR interpreter's dynamic step count as the fitness signal — a measured quantity, not an estimate, which avoids fitting an instruction count to itself. Native timing replaces it when the backend exists, and the correlation between the two is itself a result. |
| Red lines | no rule enters the release set without machine verification, and differential testing alone is not verification; the kernel cohort is the acceptance gate and never enters a fitness function — training and acceptance corpora are physically separate and the receipt proves it by digest. |
| Would refute | evolved policies that do not beat hand-written ones on held-out corpora within budget; or a rule survival rate too low to justify verification. |
