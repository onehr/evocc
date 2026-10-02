# EvoCC Q4: codegen quality and compiler experience

Plan revision 1 — 2026-10-01. **Work plan, not an acceptance receipt.** It
supersedes the search-oriented part of the [Q2/Q3 design](framing-draft.md)
(revision 7) for the next quarter. That design's verified chain, size contract
and claim rules stay in force; its discovery tracks are paused, not cancelled.

## 0. Why this plan, in numbers

**Disk precondition (owner decision, 2026-10-02).** Complete the retention
review and reach 80 GiB free before starting this track. Before every rerun,
require at least 25 GiB free; below 15 GiB stop and clean up. Use only
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2`, with Cargo calls serialized.
Do not create another target/workspace/kernel tree or a large VM disk.
Each frozen plan states its capture cap (500 MiB default); hash and measure
intermediate objects, then delete them except the minimal reproduction set.
N01 keeps disassembly summaries/category tables, never full disassembly dumps;
N05 trend receipts contain only numbers and hashes. Keep raw captures out of Git.
At slice end remove temporary outputs and record the disk net change. Preserve
all contract manifests/receipts, sealed fixtures, open-task evidence and `.lake`;
kernel runtime retirement follows [TODO's retention rule](../TODO.md#disk-discipline-owner-decision-2026-10-02).

The frozen S5 comparison ([record](notebook/eqsat-size-references-20261001.md))
measured the same six translation units under EvoCC and the reference compilers.
Object `.text + .text.*` bytes, release context (unchecked EvoCC):

| Family | EvoCC B | GCC-14 -O2 | GCC-14 -Os | EvoCC / GCC -O2 |
| --- | ---: | ---: | ---: | ---: |
| zlib (deflate + inflate) | 55,188 | 24,372 | 20,044 | 2.26 |
| zstd (compress + decompress) | 153,644 | 63,252 | 50,132 | 2.43 |
| cJSON | 29,384 | 14,400 | 11,404 | 2.04 |

Against that gap, the verified scalar rewrites move 0.8–2.4% and the certified
guard slice another 0.1–0.8%. The distance to GCC is not in the equality
saturation domain; it is in instruction selection, register allocation, frame
and call overhead and the absence of an optimizing inliner. Growing the verified
domain further now optimizes the wrong 1%.

The assets that remain decisive are not the rewrite rules: an IR with explicit
UB obligations and provenance, two independent interpreters with numbered
diagnostics, a certificate checker, and a receipt discipline that already
measures everything it touches. This plan spends the quarter on those.

## 1. Decisions

| # | Decision | Consequence |
| --- | --- | --- |
| D1 | **Pause** the EqSat discovery and domain-growth tracks: R02e second slice, R03a–R03h, latency-oriented R02c, steady-timing campaigns | Marked `PAUSED` in TODO with this document as the reason; nothing is deleted, no receipt is relabeled |
| D2 | **Keep** R01/G2 on the kernel track (S6 rides along); keep `g0.ub.v1` shipped as opt-in; keep `p0.scalar.v4` frozen | EqSat is the trust anchor for what it already proves; no new rules this quarter |
| D3 | **Primary objective stays code size** under the S1 object contract, checked and unchecked reported separately; static instruction slots alongside | Latency remains `NOT_MEASURABLE_ON_THIS_LANE` until a dedicated idle machine exists |
| D4 | **New optimizations are validated, not proved**: every new pass ships behind translation validation plus the existing gates; none is ever credited as verified | Receipts carry a per-pass class: `VERIFIED` (EqSat certificates), `VALIDATED` (new passes), `LOWERING` (existing) |
| D5 | **Data before passes**: no optimization work starts before the size-gap census (N01) has ranked the levers | The provisional order in N03 is a guess until N01 replaces it |
| D6 | **No new audit layers.** The census is read-only tooling; translation validation reuses the two interpreters, the random corpora and the ladder; receipts reuse the S1/S2 schemas | A new gate requires an observed failure that no existing gate catches |
| D7 | **Backend work lands in target-independent layers.** Every N03 change is written against the layering in Section 4a, so that R04 (x86-64) is "write the ISA module", not "rewrite the backend" | A behavior-preserving layering step (N03a) precedes any allocator or selector change; byte-identical output is its exit test |

## 2. Authority and scope

Inspected implementation: `5e7fb334` (certified guard first slice) and
`963c5442` (figures). [EvoIR](evoir-spec.md) and [EvoMIR](evomir-spec.md)
remain the semantic authorities; this plan changes no IR semantics, registry,
proof format or trust policy. Backend changes that alter EvoMIR lowering rules
need their own EvoMIR revision as today. Correctness admission (two interpreters,
21,000-program gates, 384/384 native corpora, kernel boot) is unchanged and
remains a prerequisite for every size number reported here.

Out of scope this quarter: implementing x86-64 (R04), release packaging (R05),
memory or whole-CFG rewrite domains for EqSat, any AI-assisted search. The
backend layering in Section 4a is in scope precisely so that R04 can start
from a shared framework rather than a second copy of `evocc-mir`.

## 3. Objectives, metrics and the product target

**Primary metric.** Per translation unit, object `.text + .text.*` bytes and
static instruction slots under the S1 contract, with `.rodata`, padding and
in-text data reported separately so section migration is never a saving.
Report per TU and per family; no cross-family average.

**Secondary bounds** (predeclared, checked on every landing):

- compile time per TU on the host, release build, not worse than the frozen
  baseline by more than the declared bound;
- EvoIR interpreter steps on the frozen inputs as a deterministic dynamic
  proxy: a size win that increases steps by more than the declared bound is a
  `TRADEOFF`, not an improvement;
- zero correctness regressions on the existing gates; zero certificate
  rejections for the verified chain.

**Product target, declared before any measurement, not a claim:** by the end
of the quarter, unchecked EvoCC `.text` on the six S5 units at or below
**1.4×** GCC-14 -O2 per unit, with the checked profile reported alongside.
Missing the target is a result to record, not a reason to move the target.

## 4. Trust model for validated passes

The project's standing rule is that no optimization is trusted on its own
authority. For the verified chain that authority is the standalone checker.
For the new passes it is **translation validation (TV)**: the pass's output is
checked against its input on real executions, every time, with the evidence
recorded.

```text
EvoIR before ──> [validated pass] ──> EvoIR after
       │                                   │
       ├── C-HIR / EvoIR interpreters on the frozen inputs: outputs and
       │   diagnostics must agree (random 21,000 corpus subset + ladder inputs)
       ├── four-lane native differential where the TU is native-admitted
       ├── bounded certificate replay for any region the pass changed that is
       │   P0/g0-expressible (optional, when cheap)
       └── telemetry: opportunities seen / taken / declined-by-reason,
           estimated bytes, actual bytes after assembly
```

Rules:

- A pass is admitted to `-O2` only after its TV coverage is green on the full
  corpus and the full workspace gate passes. Until then it is behind a flag.
- TV does not cover floating-point environment, inline assembly, volatile
  ordering or anything the interpreters cannot execute deterministically.
  Passes keep those constructs opaque; this is the same boundary A1 draws.
- A TV mismatch is a pass bug by definition. The interpreters are not changed
  to make a pass pass.
- Telemetry denominators are explicit per pass (eligible sites, not TUs), the
  same way `regions_admitted` was never allowed to stand in for coverage.
- Receipts name the pass class per transformation. `CHANGED_VERIFIED` keeps
  its meaning: EqSat certificates only.

This is where the typed IR and the Lean work pay off this quarter: not as
theorems per pass, but as an executable semantics precise enough to validate
every transformation instance, and as a verified kernel for the pieces that
already have certificates.

## 4a. Target extensibility: three layers, enforced

The facts each layer may hold, and how they are generated and checked, are
fixed by the [internal target description contract](target-spec.md).

Today `evocc-mir` is a single-ISA backend: `linscan.rs` refers to the AArch64
instruction enum `MInst` 113 times and `regalloc.rs` 26 times, `Target` names
only Darwin and Linux, and ABI logic lives inside the 6,139-line `lower.rs`.
Writing N03 against that shape would be paid again for x86-64. The plan
therefore fixes the layering first and keeps it fixed by tests.

```text
 Layer 1  EvoIR passes (inlining, GVN/SCCP/DCE/LICM, TV harness, telemetry)
          target-independent; consult one small TargetInfo (data layout,
          pointer width, size-cost table). No ISA names.
 ─────────────────────────────────────────────────────────────────────────
 Layer 2  Generic MIR framework
          virtual registers + RegClass, operand descriptors (use/def, fixed,
          early-clobber, tied), out-of-SSA, linear scan + coalescing, spill
          placement, allocation verifier, frame/ABI driver over an AbiSpec,
          emission driver. Sees instructions ONLY through a MachInst trait.
 ─────────────────────────────────────────────────────────────────────────
 Layer 3  Per-ISA module (today: AArch64; later: x86-64)
          instruction enum, lowering rules EvoIR -> MInst, legalization and
          immediates, encoding and assembly printing, per-ISA peepholes,
          AbiSpec implementations per OS (Darwin arm64, AAPCS64 Linux, kernel
          constraints such as the sysreg stack guard and patchable entries).
```

This is the Cranelift shape (generic VCode and regalloc over per-ISA
instruction enums with an `ABIMachineSpec`), not the LLVM one (no TableGen,
no generic opcode space). Two ISAs are enough to justify it; a third will not
require a new design.

**Rules.**

- The generic layer imports no ISA names. A workspace test greps the generic
  modules for `MInst::`, AArch64 register names and mnemonics and fails on
  any hit; this is the cheapest way to keep the boundary honest without a
  second real target.
- A synthetic test ISA (a dozen instructions, two register classes, a toy
  ABI) exercises the allocator, out-of-SSA, frame driver and verifier in unit
  tests. It is never a compilation target; it exists so the generic layer is
  tested against something other than AArch64.
- Lowering rules become data **incrementally**. Selection today is a
  hand-written `match` (`lower_inst`, about 790 lines and 50 arms, plus the
  fusion patterns in `lower_block`). N03a does not rewrite it. New rules from
  N03d go through the pattern table of the [target description](target-spec.md);
  existing arms migrate only when they are touched. A big-bang conversion is
  out of scope.
- The two existing OS ABIs on AArch64 (Darwin arm64 and Linux AAPCS64) become
  two `AbiSpec` implementations; the kernel's extra constraints are expressed
  there, not in the allocator.
- The layering step is behavior-preserving. Its exit test is byte-identical
  objects for the six S5 units and the native corpora (the S1 two-build
  identity already measures this), plus the full workspace gate. No size
  improvement is allowed to ride along with the refactor; that would hide
  regressions in either.
- Costs that passes consult (instruction size for the size objective, move
  and spill costs for the allocator) come from a per-ISA table, never from
  constants in Layer 1 or Layer 2 code.
- EvoMIR's specification is split in a later revision into the generic
  framework sections and an AArch64 ISA appendix; the plan does not change
  the spec this quarter beyond what N03d's own revisions require.

**What stays ISA-specific on purpose.** Instruction selection patterns,
immediate and addressing-mode legality, condition-code modeling, encodings,
assembly syntax, and the ABI tables. Pretending these are generic is how
backends become slow and wrong; the point of the layering is to make them
the only ISA-specific code, not to eliminate them.


## 5. Work packages

Every package follows the standing sequence: RED first, implementation,
focused tests, affected original replay, full-workspace gate, receipt. Time
boxes are limits, not estimates; a package that overruns stops and reports.

| ID | Package | Purpose | Method | RED / exit evidence | Box |
| --- | --- | --- | --- | --- | --- |
| **N01** | Size-gap attribution census | Replace guesses about the 2–3× with a ranked table of causes | Disassemble EvoCC and GCC-14/Clang-19 `-O2` objects of the six S5 units plus the ladder subject objects; map functions by symbol; classify every instruction by category (prologue/epilogue and frame, register moves, spills and reloads, address computation, extension/truncation/masking, constant materialization, call and argument shuffling, branch structure, UB guards, other); count call sites to TU-local functions that the reference inlined; emit bytes per category per TU | Forged category or unmapped function fails; totals must equal S1 bytes; frozen table under `tests/contracts/`, ranked levers with estimated bytes | 1 week |
| **N02** | Pass framework, TV harness, pass telemetry | Make any new pass safe and measurable before writing one | A pass trait over EvoIR modules with before/after hooks; TV runner reusing the two interpreters, random corpus subset, ladder inputs and the four-lane native differential; per-pass telemetry schema; receipt field for pass class; `--opt-pass=` flags for development; a `time-passes`-style report | Inject a deliberately wrong pass: TV must catch it on the corpus; telemetry denominators checked; no change to default output while no pass is enabled | 2 weeks |
| **N03** | Codegen improvements, ordered by N01 | Close the gap with standard, validated passes | Each sub-package is one pass or one allocator/selector change, landed behind a flag, measured on the six units and the ladder, admitted to `-O2` only with TV green and all bounds met | Per slice: baseline bytes, N01 estimate, measured bytes, compile-time and steps bounds, full gate | 2-week slices |
| N03a | Backend layering (Section 4a) | Make every later N03 change reusable by R04 | Introduce `MachInst` operand descriptors, make linear scan, out-of-SSA, spill placement and the allocation verifier generic over them; extract `AbiSpec` with the two AArch64 OS implementations; add the boundary grep test and the synthetic test ISA; keep AArch64 lowering/emission as the single Layer 3 module | Byte-identical objects for the six S5 units and the native corpora; allocation verifier green; boundary test green; full workspace gate | 2 weeks, before N03b–N03f |
| N03b | TU-local inlining (Layer 1) | Kernel and libraries rely on `static inline`; the current C-HIR substitution covers only required and bounded private leaves | Size-aware inliner at EvoIR level for TU-local callees; budget by callee size and call count; never across TUs | TV; certificate chain still verifies on inlined regions; size up on a unit is a recorded regression | 2 weeks |
| N03c | Register allocation (Layer 2) | Linear scan without coalescing leaves moves and spills | Coalescing and spill placement improvements first; a global allocator only if N01 shows allocation dominates | Allocation verifier unchanged and green; moves/spills category bytes down | 2 weeks per step |
| N03d | Instruction selection (Layer 3, rule tables) | Addressing modes, immediates, shifted operands, `csel`/`ccmp`, extension elimination | Table-driven additions in EvoMIR lowering with their own EvoMIR revision text | Three-lane MIR gate; per-category bytes down | 2 weeks |
| N03e | EvoIR mid-level passes (Layer 1) | GVN/CSE, SCCP, DCE, simple LICM on the integer and control subset | Validated passes under N02; EqSat remains the verified path for what it covers | TV; steps bound; no interaction with certificate admission | 2 weeks per pass |
| N03f | Frame and calls (Layer 2 driver, Layer 3 AbiSpec) | Leaf-frame elision, callee-saved minimization, argument shuffling | EvoMIR frame rules with revision text and ABI tests | ABI corpora green; prologue/epilogue bytes down | 1–2 weeks |
| **N04** | The C checker as a product | The one capability GCC cannot offer: precise, explainable UB detection from the interpreters | One command (`evocc --check`, name to be chosen) that runs the EvoIR interpreter with the existing E-codes; `--explain E3xxx` pages generated from spec section 6.3; `--error-format=json` for editors; `tests/ui/` with blessed `.stderr`; documentation and a short demo | ui tests; JSON schema test; every E-code has an explain page or an explicit TODO; no interpreter semantics change | 3 weeks, parallel |
| **N05** | Measurement automation | Catch regressions on every change and bisect them without a human | On each push: S1 sizes for the six units, compile time, a ladder subset, interpreter steps; append-only trend receipts; `render-status` trend view; `tools/bisect.py` that walks commits using retained receipts and rebuilds only when needed | Inject a size regression on a branch: the run flags it and bisect names the commit | 2 weeks, parallel |
| **N06** | EqSat memory and effect domain | The place where EqSat has real value (CSE, DCE, alias-aware forwarding) | **Not scheduled.** Reopen only after N03 lands and with a separate design: state/effect model, persistent equality database, proof design for memory facts | None this quarter | — |
| **N07** | Target description and ISA-model oracles | One internal, machine-readable description per target, with the backend generated from and checked against it | Follow the [target description contract](target-spec.md): migrate `regfile`, `insts` and the ABI tables as part of N03a; then `encodings` with the assembler round trip, a pinned slice of the vendor ISA model with its emulator as a differential lane, and bounded SMT verdicts for the existing lowering rules | Per migration step: byte-identical objects; generated files regenerate without difference; every existing lowering rule has a verdict | Steps 1–3 inside N03a; steps 4–6 in 2-week slices after N03d starts |

What N01 must answer before N03 starts:

1. Which three categories account for the most bytes, per TU.
2. How many bytes the reference saves through inlining alone (callee bodies
   that disappear versus call sites that stay).
3. Whether allocation (moves, spills, reloads) or selection (extensions,
   address arithmetic, immediates) dominates on the compressors.
4. The guard share, so the checked profile's gap is separated from the
   release gap.

**N01 classification, frozen before the run.** Every instruction of every
function goes to exactly one category; the first matching row wins; totals per
TU must equal the S1 instruction bytes. The categories are decidable from the
disassembly alone, so the same classifier runs on EvoCC, GCC and Clang objects.

| Order | Category | Rule |
| ---: | --- | --- |
| 1 | `guard` | A conditional branch whose target block is only a trap (`brk`), that trap, and a compare or flag-setting instruction whose only consumer is that branch |
| 2 | `prologue_epilogue` | Stack-pointer adjustment, saves and restores of `x29`, `x30` and callee-saved registers, frame-pointer setup, stack-guard load and check, `ret` |
| 3 | `stack_access` | Any other load or store addressed off `sp` or `x29` |
| 4 | `move` | Register-to-register copies, including the `mov` aliases and `fmov` between registers |
| 5 | `const_materialize` | `movz`/`movn`/`movk` sequences, immediate `mov` aliases, literal-pool loads |
| 6 | `address` | `adrp`, `adr`, `:lo12:` additions, GOT and TLS address loads |
| 7 | `extend_mask` | `sxt*`/`uxt*` aliases and `and` with `0xff`, `0xffff` or `0xffffffff` |
| 8 | `call` | `bl`, `blr`, and `b` to another function symbol |
| 9 | `compare_select` | `cmp`, `cmn`, `tst`, `ccmp`, `cset`, `csel` and their relatives |
| 10 | `branch` | Remaining `b`, `b.cond`, `cbz`/`cbnz`, `tbz`/`tbnz` |
| 11 | `memory` | Remaining loads and stores |
| 12 | `compute` | Arithmetic, logic, shifts, multiply, divide, bit operations |
| 13 | `other` | Everything else, listed by mnemonic in the receipt |

A second table records inlining separately: for each TU-local function, its
body bytes in each compiler's object and the number of direct calls to it. A
function present in EvoCC's object and absent or uncalled in the reference is
inlining evidence, not a category.

## 6. What pauses, what continues

| Tracker item | Disposition | Reason |
| --- | --- | --- |
| R02e second slice (seed admission of non-poison excluded values) | PAUSED | Worth ≤ 1% of `.text`; reopen after N03 |
| R03, R03a–R03h | PAUSED, V1 frozen as a completed negative-admission study | All three tracks INCONCLUSIVE / NOT_ADMITTED |
| R02c latency comparison, steady-timing lane | PAUSED | NOT_MEASURABLE_ON_THIS_LANE |
| R02c size comparison (S5) | DONE, frozen | Baseline for N01 and the N03 target |
| R01 / G2 whole-kernel chain, S6 | CONTINUES on the kernel track | Independent of this plan |
| `g0.ub.v1` first slice | CONTINUES as shipped opt-in | Verified, 0.1–0.8% |
| `p0.scalar.v4`, Lean audit, checker | FROZEN | Trust anchor; no new rules |

Open evidence corrections carried from the R02e review stay due before any
new receipt is produced: the overwritten correspondence receipt (restore the
five P0 vector sets beside `guards`) and the Lean audit receipt regenerated on
a clean worktree.

## 7. Dependencies

```text
N01 census ─────────────────┐
N02 pass framework + TV ────┼──> N03a layering ──> N03b..N03f, in the order N01 dictates
N05 automation ─────────────┘    (N05 before the first N03 slice lands, so
                                 every slice is measured automatically;
                                 N03b and N03e need only N02, not N03a)
N04 C checker product ──────────> independent; parallel track
N07 target description ────────> steps 1–3 inside N03a; steps 4–6 after N03d starts
N06 EqSat memory domain ────────> not scheduled; reopened by a separate design
```

## 8. Risks and standing rules

- **Inlining and the certificate chain.** Inlined bodies become ordinary
  EvoIR; the chain runs after inlining and verifies as before. The inliner
  itself is a validated pass, never credited to EqSat.
- **TV blind spots.** FP environment, inline assembly, volatile and atomic
  ordering are opaque to the new passes; a pass that needs to reason about
  them is out of scope this quarter.
- **Allocator changes are high risk.** Prefer incremental coalescing and spill
  placement over a rewrite; the allocation verifier is the gate either way.
- **Layering can become its own project.** N03a is a mechanical,
  behavior-preserving refactor with a two-week box and a byte-identity exit
  test. It introduces exactly the interfaces two ISAs need (operand
  descriptors, `AbiSpec`, rule tables, cost tables) and nothing speculative;
  a generic opcode space or a lowering DSL is out of scope.
- **Machine time.** The kernel track and the ladder share one VM lane; N05
  runs a bounded subset per push and the full ladder on a schedule.
- **Over-engineering.** No new audit framework, no new identity-binding
  layer, no second benchmark runner. The census and TV reuse what exists.
- **Claims.** Size numbers are per TU under the S1 contract. No latency,
  parity or "faster than GCC" claim follows from anything here. G2, Q2 and
  Q3 remain `NOT_ACCEPTED` until their own gates close.

## 9. What Rust's tooling teaches, mapped

| rustc / cargo practice | EvoCC counterpart in this plan |
| --- | --- |
| Miri as a precise UB detector | N04: the two interpreters and E-codes, productized |
| `--explain`, JSON diagnostics, ui tests with blessed output | N04 |
| perf.rust-lang.org per-commit measurement | N05 |
| crater ecosystem runs | N05: ladder subset per push, full ladder on schedule |
| `cargo bisect-rustc` | N05: receipt-driven bisect |
| `-Z time-passes`, self-profile | N02: pass telemetry and timing report |
| LLVM does the codegen | Not available to EvoCC; N03 does the work with validation instead of inherited maturity |
| Cranelift: generic VCode and regalloc2 over per-ISA instruction enums, `ABIMachineSpec`, a regalloc checker | Section 4a and N03a: the same shape at EvoCC's scale; the allocation verifier already plays the checker's role |

The last row is the honest one. Rust never had to close a 2× codegen gap;
EvoCC does, and this plan treats that as the quarter's main engineering task
while the surrounding experience is built in parallel.

## 10. Working agreement

One page, so that five packages can move at once without colliding.

**Lifecycle of a package or slice.**

1. Freeze a plan under `tests/contracts/` (`codegen-<package>-YYYYMMDD.json`):
   inputs, tool identities, metric, bounds and the stop rule. Commit it before
   running anything it describes.
2. Write the RED: the failing test or the missing measurement.
3. Implement behind a flag when the change can alter emitted code.
4. Run the focused tests, then the gates below.
5. Record one result note in `docs/notebook/` (`codegen-<topic>-YYYYMMDD.md`,
   one page: commit, gate result, number table, claim ceiling) and a compact
   receipt in `tests/contracts/`. Raw captures stay under `.evidence/`.
6. Update `TODO.md` in the same commit.

**Gates.**

```sh
tools/gate-changed.sh --full-workspace --offline     # acceptance; no filters
tools/gate-changed.sh <base-commit>                   # development only
python3 -m unittest tools.ladder.tests.<module> ...   # touched tool modules
tools/gate-lean.sh                                    # only when lean/ or a registry changes
python3 tools/render-status.py --todo                 # after any TODO.md edit
```

**Definition of done, by kind of change.**

| Kind | Done when |
| --- | --- |
| Census or measurement (N01, N05) | Plan frozen first; totals reconcile with S1 bytes; two builds byte-identical; negative results kept |
| Behavior-preserving refactor (N03a, N07 steps 1–4) | Objects byte-identical for the six S5 units and the native corpora; boundary test green; full workspace gate |
| Validated pass (N02, N03b, N03e) | Translation validation green on the full corpus; telemetry recorded; size, compile-time and steps bounds met; behind a flag until then |
| Selection or frame change (N03c, N03d, N03f) | EvoMIR revision text; three-lane MIR gate; native corpora; category bytes from N01 move in the predicted direction |
| Product feature (N04) | ui tests with blessed output; documentation; no interpreter semantics change |

**Lanes that can run in parallel, by files touched.**

| Lane | Packages | Owns |
| --- | --- | --- |
| Measurement | N01, N05 | `tools/ladder/`, `tests/contracts/`, read-only on crates |
| Mid-level | N02, N03b, N03e | new pass crate or module, `crates/evocc-eqopt` untouched |
| Backend | N03a, N03c, N03d, N03f, N07 | `crates/evocc-mir`, `targets/`; one slice at a time |
| Product | N04 | driver, interpreters' diagnostics surface, `tests/ui/` |

Heavy runs (ladder, kernel, VM captures) stay serial on the one VM lane. The
backend lane is single-threaded by design: N03a must land before any other
backend slice starts.

**Stop and report, do not improvise, when:** a time box is exceeded; a change
needs new semantic authority (IR, registry, trust policy); a gate fails for a
reason outside the slice; or a result contradicts the N01 table that ordered
the work.

**Report format.** Commit, plan and receipt paths, gate result, one number
table, claim ceiling. Nothing else.

## 11. Status and order of work, 2026-10-02

This section is the entry point for whoever continues. It records state; the
sections above remain the contract.

| Package | State | Evidence |
| --- | --- | --- |
| N01 census | Done | [note](notebook/codegen-census-20261002.md), [receipt](../tests/contracts/codegen-n01-20261002-receipt.json) |
| N02 pass framework and validation | Done, bounded framework | [note](notebook/codegen-pass-framework-20261002.md) |
| N03a backend layering | Done, independently rechecked | [note](notebook/codegen-backend-layering-20261002.md): every one of the 384 Darwin and 384 Linux corpus object hashes equals the pre-refactor baseline; `src/generic/` holds no ISA name; gate 2,986 passed, 0 failed, 4 ignored |
| N04 C checker | Done | [note](notebook/codegen-checker-20261002.md) |
| N05 measurement and bisect | Done | [note](notebook/codegen-measurement-20261002.md) |
| N03f frame and calls | Done, opt-in only; defaults OFF | [result](notebook/codegen-frame-variants-20261002.md), [receipt](../tests/contracts/codegen-n03f-20261002-receipt.json): F1/F2/F3, full workspace 2,997/0/5 ignored, OFF byte identity and ON execution on both 384-case corpora, six-TU sizes. Darwin signed unwind stays NOT_VALIDATED. |
| N03d | C1 closed opt-in; C2 closed with compile-bound miss, not admitted; remaining selection patterns open | [C1](notebook/codegen-constants-20261002.md), [C2 result](notebook/codegen-local-selection-20261002.md), [C2 receipt](../tests/contracts/codegen-n03d-c2-20261002-receipt.json): full workspace 3,018/0/5 ignored, native and size controls green; local-only B checked zstd-compress compile median 1.7977x OFF exceeds 1.25x. Negative result frozen, no default flip. |
| N03c | T1 correctness closed with compile-bound misses; T2 copy hints closed opt-in; spill quality open | [T1 result](notebook/codegen-allocation-termination-20261003.md) preserves three compile-bound misses. [T2 result](notebook/codegen-copy-hints-20261003.md), [receipt](../tests/contracts/codegen-n03c-t2-20261003-receipt.json): full workspace 3,028/0/5 ignored; both 384-case corpora ON/OFF and 72 selected-TU harness executions green. Copy-only B unchecked savings 0/44/400/628/148/44 move bytes; all 48 size/cost comparisons within bounds, maximum compile ratio 1.021x. Defaults OFF; no full allocator closure. |
| N03b, N03e | First slices closed negative; broader transforms need redesign | [note](notebook/codegen-negative-preflight-20261002.md), [closure](../tests/contracts/codegen-n03-preflight-closure-20261002-receipt.json) |
| N07 steps 4–6 | E1 encoding migration under validation; steps 5–6 open | [Frozen E1 plan](../tests/contracts/codegen-n07-encodings-20261003.json); fresh before objects retained for exact post-migration comparisons. Steps 1–3 landed inside N03a. No model/SMT or full N07 acceptance claim. |
| N06 | Not scheduled | |

**Order of the remaining slices, from N01.** Summed positive excess of
unchecked EvoCC over GCC-14 `-O2` on the five library units:

| Category | Excess bytes | Slice |
| --- | ---: | --- |
| Prologue and epilogue | 44,484 | N03f |
| Constant materialization | 23,224 | N03d |
| Memory access | 15,972 | N03d (addressing modes) |
| Register moves | 12,952 | N03c |
| Branches | 11,336 | N03d |
| Compute | 11,044 | N03d |
| Compare and select | 8,288 | N03d |
| Extension and masks | 4,132 | N03d |
| Stack access, inflate only | 6,712 | N03c |

The census above predates N03f. With that opt-in slice closed, next:
**N03d, then N03c.** N07 steps 4–6 follow once N03d has
started. N03b and N03e are not next: their frozen preflight rules admitted
nothing, and inlining in particular needs a new design before any transform.

**What remains true of the default frame code**: callee-saved registers
are saved and restored one at a time with `str` and `ldr`; the whole epilogue
is emitted at every return; every function builds a frame record, with no leaf
case. N03f's opt-in alternatives now pair saves, share epilogues and omit
eligible leaf records without changing frame layout. On the five library
units, all three enabled leave unchecked B at 1.717–2.546 times GCC-14 `-O2`.
Adding N03d C1 leaves 1.653–2.528 times; C2's cumulative size observations
leave 1.612–2.501 times, but its compile-cost ceiling was missed on one
comparison. The 1.4 target is still not met and default output has not changed.
C1's measured reduction is entirely constant materialization; C2 removes
compute/extension instructions. Both are separate from frame and P0 gains.
Do not reclassify C2's size/correctness results as full admission. Continue
with N07 after N03c T1/T2. T2 copy hints save only move bytes on the frozen
population; frame+C1+copy leaves library ratios 1.643–2.488 times GCC-O2,
without including C2 or upgrading its negative cost result. Remaining N03c
spill-quality and N03d selection work require separately frozen scopes.

**Rules that hold for the rest of N03.**

- New behavior lands behind an option and stays off by default. Turning a
  default on changes every emitted object and the exact-text tests that pin
  them, so it is a separate, owner-approved step with its own baselines, not
  part of a slice.
- With the options off, objects stay byte-identical to the slice's recorded
  baseline. With them on, the native corpora and the six units' harnesses
  must execute and agree.
- The kernel profile and any frame-pointer-requesting option keep today's
  output until the kernel gates have been run with the new code.
  This protection also applies when a frame variant is explicitly requested:
  all three variants are suppressed, not only leaf-frame elision. The N03f
  plan's handoff amendment records this narrower opt-in population; its
  assembly header records the request, not a claim that every function took it.
- One writer in the tree at a time; all builds share `target/frontend`.

**N03a observation, repaired by N03c T1.** The synthetic whole-pool ABI now
has bounded spill progress and valid saturated-call preparation. Existing
AArch64 objects are unchanged. This correctness closure does not close
allocation quality or excuse the frozen compile-cost misses above.
