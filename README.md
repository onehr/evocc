# EvoCC

**An equality-native C compiler.** EvoCC compiles GNU C to AArch64 without LLVM or GCC anywhere in the pipeline — its own preprocessor, parser, IR, optimizer, and backend. Its distinguishing choice is that **equivalence is persistent compiler state**: optimization accumulates alternative forms of a program in an e-graph rather than destructively rewriting it, and every rewrite emits a proof certificate that a standalone checker replays without trusting the optimizer. The target is not a toy: a pinned Linux 6.18.44 `arm64 virtconfig` kernel that compiles, links, and boots under QEMU, with an EvoCC-built `/init` printing the marker.

![The pipeline](docs/images/architecture.png)

## Why

Conventional compilers destroy information at every step: each rewrite discards the form it replaced, so later passes cannot recover it and phase ordering becomes a permanent source of missed optimizations. Equality saturation fixes that by keeping every equivalent form until a cost model chooses, but it has stayed largely in research because real languages have effects, undefined behavior, and pointer provenance that an e-graph will happily "optimize" away. The bet here is that this becomes tractable if the semantics are made explicit *before* the e-graph sees the program — memory as tokens, UB as assumptions, provenance as part of pointer identity — and if every union carries a machine-checkable justification. A compiler whose rewrite library will increasingly be written by AI agents needs proofs as a product, not a debugging aid. Booting a real kernel is the only honest test that the semantics survived contact with reality.

![Rewriting in place versus merging into an e-class](docs/images/why.png)

## How

Every claim in this repository is a **measured number with the command that produced it**, replayed from sealed evidence. Layout and language semantics are frozen against a sealed GCC-14.2 / Clang-19.1 container: where the two disagree we record `ORACLE_CONFLICT` and take the answer that refuses rather than voting, and no specification sentence about oracle-observable behavior ships without its probe. Work runs as a loop: freeze the rule in `docs/evoir-spec.md`, open a tests-only RED window whose failures are declared in advance, implement, then publish a kernel-telemetry dashboard from observer-sealed evidence that reports the next first-error site. Kernel telemetry orders work inside a stream; nothing is claimed from intuition, and a specification gap stops execution rather than being guessed at. The specification is authoritative and versioned (currently revision 1.9.21, ~3,900 lines); the acceptance contract records every ruling and every deferral with the error code that counts it.

![How the work is driven](docs/images/loop.png)

---

## Roadmap 0.1

Status is measured, not estimated. `DONE` means a gate is green on sealed evidence.

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

The parse count has sat at 2/16 while nine distinct blockers were cleared and
republished with zero regressions: `__attribute__`, `typeof`/`_Generic`, pointer
updates, `asm` and the overflow builtins, statement expressions, the
compile-time builtins, aggregate copies, `__typeof_unqual__`, and places rooted
at an address-of. A unit is clean only when its *last* blocker falls, so the
count is a lagging indicator and the frontier is the real one.

### Front end

**EvoCPP — preprocessor · DONE**
A full C11 preprocessing-token implementation (`crates/evocc-cpp`), not a token approximation: pp-numbers including pasted results like `1_MODULE`, stray characters that survive stringification, `##` re-lexing, and `_Pragma`. It presents itself as the layout oracle's dialect — the sealed GCC-14.2 aarch64-linux predefined set of 421 macros is copied into the crate and digest-checked by a test, and `__has_attribute` / `__has_builtin` / `__has_feature` / `__has_include` answer from measured tables rather than guesses. `#pragma pack` is applied to record layout with its measured numbers. **Gate green: all 16 kernel cohort units preprocess clean (16/16).**

**Lexer and C11 parser · DONE**
`crates/evocc-lex` and `crates/evocc-parser` cover C11 plus the GNU dialect the kernel actually uses, verified against 142 sealed lexer goldens and a 1,000-snippet generated parse corpus. Implemented and oracle-checked: `__attribute__` in all ten positions with `aligned`/`packed` applied to layout, `typeof`, `__typeof_unqual__`, `_Generic`, statement expressions, designated initializers and compound literals, `asm` including `asm goto`, the compile-time builtin family, `__auto_type`, case ranges, and anonymous-member promotion. Every construct's semantics were measured in the sealed container before implementation.

**Type system and layout · DONE**
`crates/evocc-types` computes layout for the AArch64 target, checked byte for byte against the sealed GCC/Clang consensus by two frozen corpora — 128 base cases and 22 attributed cases, both at 0 mismatches. Bit-fields, packing, alignment attributes and zero-length arrays are included; the oracle corrected the specification twice during development, and both corrections are recorded in the revision log.

**Kernel parse closure · OPEN**
_Placeholder._ 2 of 16 cohort units parse clean under both oracles as of the last publication. The remaining blocker measured there is a place expression rooted at an address-of (`(&h->head)->next`), which `container_of` and `list_for_each_entry` expand to — 28 lanes across 14 units.

### Semantics

**C-HIR and the semantic interpreter · DONE**
`crates/evocc-chir` is a checked high-level IR carrying source spans, an allocation manifest, and a memory model with per-byte initializedness and pointer provenance. Its interpreter is one of the two authorities used for differential testing, and it reports undefined behavior as typed diagnostics rather than by producing a value.

**EvoIR — block-argument SSA · DONE**
`crates/evocc-evoir` defines a typed SSA IR where memory and observable effects are explicit `mem<all>` and `world` tokens rather than implicit ordering, so a rewrite cannot silently reorder them. It has a validator, an interpreter, and a byte-level textual form that round-trips. `fib`, loops, recursion and calls execute identically in the C-HIR and EvoIR interpreters (day-6 receipt `ACCEPTED`).

**Semantic lowering · DONE**
`crates/evocc-lower` lowers C-HIR to EvoIR with undefined behavior made explicit as `ub.assume.<reason>` obligations before any optimization runs. Aggregate assignment, copy-initialization and by-value calls lower to a single `copy` instruction that transfers bytes with their initializedness; front-end-resolved constructs (`typeof`, `_Generic`, compile-time builtins) are proven to leave no trace by lowering them beside their hand-written equivalents and requiring byte-identical modules.

**Equality engine and proofs · DONE (v0)**
`crates/evocc-egraph` provides union-find, hashconsing and congruence with a justification on every union; `crates/evocc-eqopt` applies the reviewed `p0.scalar.v1` rewrite set and emits EVP1 certificates. `evo-proofcheck` is a standalone binary that independently validates every arithmetic rewrite without trusting the optimizer (day-8 receipt `ACCEPTED`). Memory instructions are opaque to the optimizer, and a trap terminator is explicitly a report and never a licence to treat a block as dead.

**Analysis lattices and bounded scheduler · OPEN**
_Placeholder._ Not started. Budgets must be counted in iterations and nodes, never wall-clock, or the reproducible-build gate is unreachable.

**Randomized semantic gate · OPEN**
_Placeholder._ Not started: no random EvoIR generator, no Csmith harness, no proof-replay campaign.

### Backend

**EvoMIR, register allocation, AArch64 · OPEN**
_Placeholder._ Not started. The compiler emits no machine code today.

**Apple and Linux ABIs, native flight gates · OPEN**
_Placeholder._ Not started.

### Kernel

**Kernel telemetry and publication · DONE**
A frozen 16-unit cohort from the pinned kernel is compiled through three lanes (raw EvoCPP, GCC-preprocessed, Clang-preprocessed) under a pinned observer that seals stdout, stderr and exit status. `tests/acceptance/publish-kernel-dashboard.py` refuses to publish on any lane regression, on unsealed evidence, or on a cohort mismatch, captures into a per-commit directory, and takes its run directory from its own capture so a stale run cannot be republished. Every refusal path is exercised by `--self-test`. This is what turns "the compiler is getting better" into a number with a command beside it.

**Diagnostic survey · PARTIAL**
`evocc --parse-only --survey` reports every diagnostic instead of the first, and `tools/survey-cohort.py` aggregates across a corpus so the remaining work can be enumerated rather than discovered one construct per build. Its per-unit roots agree exactly with the published dashboard. Two limits are stated rather than hidden: the aggregate is still dominated by cascades from a single failed declaration, and recovery is not yet sound after a declaration fails inside a function body, so affected units are reported as *unavailable* rather than as clean.

**Full-config sweep, kernel codegen, object gate, boot · OPEN**
_Placeholder._ Not started. The flight lab itself — ARM64 Linux VM, Kbuild integration, QEMU, initramfs — is also unbuilt.

---

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

Every commit passes the same gate: workspace tests, `clippy -D warnings`, `rustfmt`, Ruff on the Python harnesses, both layout corpora against the sealed oracles, and an isolated offline clone replay. Measured at `b14eccd`, the most recent commit with no open test window: **956 tests passed, 0 failed**, clippy 0, corpora 128/128 and 22/22 at 0 mismatches. A higher count on a later commit may include an open red window, where declared failures are the point; the receipts in `tests/contracts/` say which commit each number came from. Nothing is claimed that these do not measure.

The figures are generated, not drawn: `python3 tools/render-readme-figures.py` re-renders them.
