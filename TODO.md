# EvoCC remaining work

Updated **2026-10-27**, initially audited against `a83d803`; current clauses are
EvoIR **2.39.0** and EvoMIR **1.16.0** (pinned in `spec/`). This is the Git-tracked execution
backlog, not a new test run.
Task IDs are stable: update or split an item with cross-references; do not recycle
IDs or silently delete unresolved work. Historical review IDs remain in their
original ledgers; the IDs below belong to this tracker.

The [evidence document](docs/evidence.md#status--2026-10-27) summarizes measured capability;
[docs/status.md](docs/status.md) is generated from receipts. This file answers
**what remains, what to do next, and what evidence closes it**. Detailed sources,
commands and measurements stay in the [kernel ledger][kernel],
[application ledger][apps], [semantic research brief][semantics] and
[roadmap][roadmap], rather than being copied into another evidence tree.

## How to use this tracker

- `OPEN`: diagnosis or implementation remains. `NEEDS_RULE`: a specific semantic
  decision remains; collect the bounded probes and continue independent work.
- `RETEST`: the stated prerequisite repairs exist, but the named acceptance run
  has not been completed. `LATER`: a separate roadmap capability, not a current
  kernel-boot blocker. Checked items have evidence for their **stated scope**.
- `P1`: current correctness/runtime failures, verdict integrity and minimal
  reproductions. `P2`: remaining language/application coverage. `P3`: later
  research, portability and release gates. Dependencies take precedence.
- Work in **coherent repair batches**, not one full campaign per small patch:
  enumerate the batch's REDs/decisions first; then shared implementation and
  focused tests per fix, inexpensive unchanged-original replays, affected-crate
  gates at integration points, receipts/status and bounded commits. Once all
  batch prerequisites are green or explicitly dispositioned, freeze one subject
  and run the complete acceptance campaigns serially. A new failure returns to
  a small reproducer, not an automatic rerun of every already-green campaign.
  Use existing fixtures and `tools/bench.py`; do not create another framework.
  The [September 27 batch packet][closure-batch] indexes every open task, current
  evidence, genuine decisions and required final gates. Ordinary implementation
  steps do not require another user approval; new semantic authority does.
- A small-case PASS is not an original-program PASS. A reference failure, source
  UB, unsupported feature, skip or timeout is not a PASS. An evidenced disposition
  can close a *classification* task, but cannot increase the PASS numerator.
- Preserve source/flags, seeds, denominators, assertions and declared budgets.
  Do not weaken validators/UB obligations or substitute reference-built target
  objects to close an item. Spec changes must state semantics, not suppress errors.
- Update this file in the implementation commit: status, next action, fixture,
  affected-original result and evidence/commit reference. Commit bounded completed
  changes promptly; do not accumulate unrelated days of uncommitted work.
- Record an active long run's PID/container, log, compiler/Image/payload identity
  and last observation in its existing ledger. A stale receipt is not a live job.
  Keep heavy campaigns serial; reuse build trees and retain only necessary evidence.
- During tailored repair, retain one current reference/subject runtime pair per
  required kernel configuration. Retire superseded large payloads after checking
  open handles and preserving their build manifests/results; record that their
  exact historical runtime is no longer retained. Do not clone the workspace or
  reduce the final kernel configuration to save storage. Rebuild final artifacts
  with the repaired compiler before K13/K14; old Images cannot inherit new fixes.

### Disk discipline (owner decision, 2026-10-02)

- Step zero, before any build or rerun: inspect free space and retention. Do not
  start below **25 GiB** free; stop active work and clean up below **15 GiB**.
  This cleanup checkpoint must reach at least **80 GiB** before new work.
- Use only `CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2`; serialize Cargo
  invocations in that tree. The repository Cargo defaults match this policy.
  No extra target trees, workspace clones, kernel trees or large VM disks.
- Freeze each capture's retention cap in its plan: **500 MiB by default**.
  After hashing and measurement, remove intermediate objects and keep only the
  minimal reproduction set. Raw captures never enter Git.
- N01 retains disassembly summaries and category tables, not full disassembly;
  N05 trend receipts retain numbers and hashes only.
- End each slice by removing its temporary products and recording its disk net
  change. Preserve manifest/results before retiring superseded payloads; check
  open handles and record that the exact historical runtime is no longer retained.
- Never delete `tests/contracts/`, sealed `tests/fixtures/`, evidence required by
  an open task, or Lean's `.lake` cache. Keep one current reference/subject runtime
  pair per still-required kernel configuration, including diagnostic controls.

October 2 cleanup completed: host free **17.910 → 82.078 GiB**; measured project
artifact net **−57.822 GiB** (mounted volume not double-counted). Eight retained
runtime hashes and 3,291 preserved records/fixtures are unchanged. The existing
sparseimage was remounted after unsupported compaction; no compaction saving is
claimed. No build or benchmark ran. [Exact retention and recovery record][kernel].

October 4 follow-up: retire superseded runtime disks, generated objects, link
temporaries and assembly/benchmark caches after recording hashes; preserve
current runtime pairs, kernel products, sources and evidence. Official sparseimage
compaction now succeeds. Project host-backed allocation **153.931 → 99.097 GiB**
(net **−54.834 GiB**); host free **15.560 → 62.105 GiB**. The **80 GiB** cleanup
target remains unmet, so heavy work stays blocked; this is not a new compiler
or kernel PASS. [Retained artifacts and measurements][kernel].

October 4 owner follow-up supersedes that temporary block: bounded V00–V06
verification may resume below 80 GiB, retaining the 25/15 GiB start/stop lines,
500 MiB capture cap and single two-job `target/frontend` tree. No new kernel
build, runtime retirement or large disk is authorized by this exception.

October 11 cleanup: remove only unused Cargo dev intermediates (`incremental`,
`deps`, `build`, `.fingerprint`) and the removed website's `web/node_modules`.
No Cargo process or open handle used those paths. Keep the running editor's
debug tools, all release tools, the Linux probe and reference outputs, all
evidence, sealed fixtures, toolchain images and Lean `.lake`. The 1,943 tracked
contract/fixture files and 25 retained tool binaries keep their hashes.
Project host-backed allocation is **33.303 → 25.582 GiB** (net **−7.720 GiB**);
host free space is **18.764 → 26.381 GiB**, measured separately. The mounted
case-sensitive tree is not counted again with its backing image. These caches
were deleted without Trash and must be rebuilt; no historical runtime was
retired. No build or full gate ran. The previous 80 GiB target remains unmet.
Local record: `.evidence/storage-cleanup-20261011/summary.json`.

## Execution order

**Release verification path:** V00 to V08 are complete in the frozen `core-v1`
scope (see "First public release verification" below and
[`spec/verification.md`](spec/verification.md)). The October 4 requirement that
the first public version includes this path is met. The specification-review
defects D01 to D05 and the separate lowering refusal D06 are follow-up items.
The bounded codegen batch remains closed with misses; no new optimization slice
or default flip is scheduled. The older kernel/application queues below retain
their independent acceptance obligations; none is marked complete by this plan.

September 28 replan at `6bd69b7`: prioritize **bounded ARM64 acceptance**, then
the remaining language breadth, then the wider research/release roadmap. This
is sequencing, not approval to drop features or convert failures into passes.
The project has working boot and substantial application coverage, but the full
roadmap is not just a few cleanup fixes. T01/B01, K05/K06/K08/K09, L01/L02,
B05–B07/B09 and R06 already have their stated closure evidence; do not schedule
them again as missing implementations.

The queue below is checked against every unchecked task by
`python3 tools/render-status.py --todo`. It counts **tracker entries, including
parents and children**, not independent bugs, effort or a completion percentage.
Scheduling does not launch a background worker. No compiler/kernel test process
was found in the read-only process check at this planning checkpoint.

<!-- closure-queues:start -->
| Queue | Open IDs | Next deliverable |
| --- | --- | --- |
| KERNEL | K01, K02, K03, K04, K10, K11, K11f, K12, N08, N09, N11 | Start with K02/K03's existing current-Image profiles; produce a cause-backed hot-path reduction, then address BPF/pidfd budgets and intermittent sched_ext/ublk failures. |
| APPLICATION | L11a, P17a, P22 | P21 completed 144 registered sessions: P1-P3 held; P4 and the earlier control penalty did not. P22's pilot of 2026-10-20 failed two go criteria on a false escape record. P01 through P20 retain their completed scope: P20 had no repair-count advantage; P18 stopped at the provider key limit. Existing application tasks keep separate exits. |
| REVIEW |  | No rule decision is open. The 2026-10-13 decisions for K07, L03 to L10 and L11a to L11c are in docs/archive/notebook/rules-20261013/DECISIONS.md. |
| AUDIT | D33 | D24 to D28 closed the follow-ups of D22, D23, L04 and L05; D33 changes the alignment model. D01-D15 are closed in their recorded scope. Keep the T03 audit boundary; kernel/application acceptance and deferred proof domains remain separate. |
| ACCEPTANCE | T04, T02, K13, K14, R05, R05k | On 2026-10-27 the compiler of `08bf09bd` passed the frozen acceptance (B10 and the direct Kbuild replay), the final gate T05 passed, and both archives were built (R05a). Next: the owner's R05k: the dating commit, the rebuild of both archives from it, the tag and the publication. |
| RELEASE | Q05, R04, R04a, R07 | Release 0.2.0, foundation: Q01 to Q05 and R04a; R07a's measured design is complete. Then x86-64 (R04b to R04e), RISC-V (R07b to R07e), Lean core-v2 (V09 to V13), the EqSat kit (E01 to E03) and the release (R05m). |
| LATER | R05m, R01, R02, R03, R03a, R03b, R03c, R03d, R03e, R03f, R03g, R03h, R04b, R04c, R04d, R04e, N06, N06a, N06b, N06c, N06d, N06e, R07b, R07c, R07d, R07e, V09, V10, V11, V12, V13, E01, E02, E03 | S1–S5 and R02e first slice are closed; S6 stays with R01's two G2 builds. R03 V1 negative-admission study is frozen; positive Q3 remains unachieved and R03a/b/c/d NOT_ADMITTED, R03e deferred. No VM timing or audit expansion; x86-64 and release stay separate. The bounded N01–N05/N07 batch is dispositioned, not a complete optimizing compiler: N03b/N03e preflights are negative, N07 E3 has zero SMT proofs, and all five library size targets remain missed. N03d C5 has size/correctness evidence but three compile-cost misses; C2/C5 remain outside the admitted cumulative configuration, and T1's misses stay frozen. All new defaults remain OFF. Retain N02's 70 admitted/187 opaque inputs and source-bridge mismatch. Broader inlining, mid-level and allocator work need newly scoped designs; no paused R activity is automatically reopened. |
<!-- closure-queues:end -->

## Release 0.2.0

Owner decision of 2026-10-28: EvoCC 0.2.0 is extendable, easy to adopt and ready for production
use. It supports AArch64 (Linux and macOS), x86-64 Linux userspace and RISC-V (RV64GC) Linux
userspace. The proof-required path stays AArch64 Linux only and refuses the other targets with
exit status 6. The work runs in six parts: foundation, x86-64, RISC-V, Lean `core-v2`, the EqSat
kit, and the release. AArch64 output stays byte-identical through the first three parts, apart
from named fixes.

- [x] **Q01 — Size limits with a ratchet** — `DONE`, P1, 2026-10-29.
  Files and functions in `crates/*/src/` and `tools/` have at most 1,500 and 150 lines. A
  baseline lists the older items above a limit; it only shrinks. Exit: `tools/check-size.py` runs
  in the gate and in `ci.yml`.
  The scanner counts doc/attribute prefixes and nested functions, and skips Rust literals
  and nested comments. The baseline comes from committed source and updates cannot add or
  raise an exception. Both gate modes stop before Cargo on a size failure. The isolated gate
  fixture supplies the checker stand-in authorized by the resume instruction; assertions stay.
- [x] **Q02 — Split the four largest source files** — `DONE`, P1, 2026-10-29.
  `evocc-parser/src/lib.rs`, `evocc-lower/src/lib.rs`, `evocc-chir/src/interp.rs` and
  `evocc-evoir/src/lib.rs` become modules of at most 1,500 lines. Exit: the AArch64 identity check
  and the full gate.
  The parser now has 31 responsibility files with a 1,448-line root; lowering has 33 files
  with a 713-line root; the C-HIR interpreter has 28 files with a 1,208-line root; EvoIR has
  18 files with a 1,457-line root. Function comparisons cover 504, 256, 212 and 188 items.
  All production function tokens and literal
  contents stay unchanged; two C-HIR unit calls lose only optional trailing commas under
  rustfmt. Each move matches all 792 full-object hashes of the start baseline, with matching
  twins and native results on Darwin/Linux. Each commit requires the full workspace gate.
  The source-path lists add only the new modules. The diagnostic inventory follows
  InterpreterError to errors.rs; its original eight checks pass. The instruction inventory
  still reads the unchanged Inst declaration at the EvoIR root. The stale validator path
  in spec/evoir.md stays unchanged under the goal's specification-edit boundary.
- [x] **Q03 — Split the long functions of the backend path** — `DONE`, P1, 2026-10-29.
  `parse_options`, `emit_inst`, `lower_inst` and `lower_block` become functions of at most 150
  lines. Exit: the AArch64 identity check and the full gate.
  The four functions have 5, 75, 73 and 61 lines. Six helper modules hold 70 bounded
  functions, including the instruction dispatcher; the largest function has 126 lines
  and the largest new file has 1,178. Existing root file exceptions only shrink.
  All 262 other function sequences and exact string/character literals stay unchanged.
  Option families keep their order; argument, obligation and variadic phases keep their
  value/block allocation order. Existing tests are unchanged. All 792 start-baseline
  objects match, with equal twins, complete native agreements and determinism.
  The full offline workspace gate is required before the completion commit.
- [x] **Q04 — An architecture page and a README for each crate** — `DONE`, P1, 2026-10-29.
  Exit: `ARCHITECTURE.md` and 18 crate READMEs, each fact checked against the code.
  The architecture names the actual pipeline, both source/IR interpreters and the separate
  ordinary and proof-required paths. Each crate page lists its interfaces, inputs, outputs,
  invariants, contract and tests. The enforced dependency boundaries link to their tests.
  The architecture has 127 lines; the crate pages have at most 32. Fact checks and the
  mdBook 0.4.52 build must pass before the full offline gate and completion commit.
- [ ] **Q05 — Keep active work in the tracker, and inventory the tools** — `OPEN`, P2.
  Exit: closed entries move to `docs/archive/todo-0.1.0.md`; an inventory classifies each file
  under `tools/`.
- [ ] **R07 — RISC-V (RV64GC) Linux userspace backend** — `OPEN`, P1.
  The same shared EvoMIR as x86-64 (R04). The proof-required path refuses RISC-V.
  - [x] **R07a — Design** — `DONE`, P1, 2026-10-29.
    [Design](docs/design/riscv64-linux.md): RV64GC/LP64D, measured GCC 14/QEMU environment,
    every current machine form, software flags, binary128, fenv and ABI boundaries. The local
    probe image and sources/results are in `OUT/riscv-design/` of the foundation goal.
    This is a design, not an implemented backend. No compiler behavior changes.
  - [ ] **R07b — Integer-only functions and calls** — `LATER`, P1.
    Estimate: 3,000 production/tooling and 1,600 test lines, plus target tables. Exit: every
    integer expansion passes GNU/LLVM assembly and QEMU execution; scalar/pair ABI calls pass
    both ways; 256 integer seeds agree; proof-required RISC-V refuses with exit 6 before output;
    checked traps, AArch64 full-object identity and the full gate pass. See design Section 6 M1.
  - [ ] **R07c — Memory, records, globals and the full LP64D convention** — `LATER`, P1.
    Estimate: 2,300 production/tooling and 1,600 test lines. Exit: record/hidden-result ABI,
    bank exhaustion, original 128-/256-case corpus populations, PIC/TLS, memory and atomics pass;
    grammar, allocation-negative tests, AArch64 identity and the full gate pass. See Section 6 M2.
  - [ ] **R07d — Floating point and variadic calls** — `LATER`, P1.
    Estimate: 2,200 production/tooling and 1,500 test lines. Exit: strict values/status in all
    four modes, both tininess boundaries, NaN boxing, audited binary128 helpers/conversions
    and variadic cross-calls pass; AArch64 identity and the full gate pass. See Section 6 M3.
  - [ ] **R07e — Run the corpus on RISC-V** — `LATER`, P1. Exit: c-testsuite, cJSON, zlib,
    Lua and SQLite pass their own tests with `CC=evocc` under `qemu-riscv64`.
    Estimate: 800 production/tooling and 500 test lines, plus a 500-line defect allowance.
    Keep pinned inputs/flags and every counted EvoCC object; no reference rebuild. Baseline ISA
    audit, AArch64 identity and the full gate pass. See design Section 6 M4.
- [ ] **V09 — Propose loop cutpoints automatically** — `LATER`, P1.
  `evo prove` makes the cutpoint proposal that a loop needs today by hand. The proposal stays
  untrusted. Exit: the `core-v1` pilot loops pass with no hand-written proposal.
- [ ] **V10 — `char`, `short`, `_Bool` and `switch` in `core-v2`** — `LATER`, P1.
- [ ] **V11 — Frame objects at the assembly stage: `&local` and local arrays** — `LATER`, P1.
- [ ] **V12 — Recursion on the proof-required path** — `LATER`, P2.
- [ ] **V13 — EvoIR as a standalone Lean library with a conformance suite** — `LATER`, P1.
  Exit: its own Lake library, a text round-trip theorem, and a suite that the Lean and the Rust
  interpreters both run in CI.
- [ ] **E01 — One rule registry for the producer, the checker and Lean** — `LATER`, P1.
  Exit: a new rule is one registry entry and one Lean theorem; the checker keeps its independent
  rule table (spec section 11.6).
- [ ] **E02 — A generic certificate replay checker crate** — `LATER`, P1.
  Exit: the EvoIR checker is its first instance, and a second toy language uses it in a test.
- [ ] **E03 — An `egg` saturation producer, measured** — `LATER`, P2.
  Needs the owner's approval of the download. Exit: certificates that `evocc-checker` accepts,
  and a ladder size and compile-time comparison against the current producer.

## Diagnostics and command-line product work

- [x] **P01 — Render source diagnostics and freeze their format** — `DONE`. [History](docs/archive/todo-0.1.0.md#p01).
- [x] **P02 — Explain every authoritative diagnostic** — `DONE`. [History](docs/archive/todo-0.1.0.md#p02).
- [x] **P03 — Add five front-end lints and level control** — `DONE`. [History](docs/archive/todo-0.1.0.md#p03).
- [x] **P04 — Add user-intent commands** — `DONE`. [History](docs/archive/todo-0.1.0.md#p04).
- [x] **P05 — Package the Linux header sysroot** — `DONE`. [History](docs/archive/todo-0.1.0.md#p05).

### Working sequence and release gates

## Repair, agent and integration work

Owner contract: October 7, second product stage. Keep the existing scope,
line budgets, RED-first commits, serial offline builds and per-package full
workspace gate. New result-producing commands/options have a JSON form.
The owner removed the retired kernel runtime; disk admission is available
again (100.2 GiB at entry). Check exact free bytes before every build.
Retain at most 500 MiB per capture. Do not reopen the verification track.
The October 7 owner reply restores the P08--P10 clauses, including the 250-line
JSON limit and didSave. Read its P11 clauses together with the original attachment;
retain the original scope where one copy has a truncated sentence.

- [x] **P06 — Correct help and early stdin rejection** — `DONE`. [History](docs/archive/todo-0.1.0.md#p06).
- [x] **P07 — Expose and apply diagnostic repairs** — `DONE`. [History](docs/archive/todo-0.1.0.md#p07).
- [x] **P08 — Add machine-readable user commands** — `DONE`. [History](docs/archive/todo-0.1.0.md#p08).
- [x] **P09 — Add editor diagnostics and repair actions** — `DONE`. [History](docs/archive/todo-0.1.0.md#p09).
- [x] **P10 — Integrate existing build systems** — `DONE`. [History](docs/archive/todo-0.1.0.md#p10).
- [x] **P11 — Publish channel metadata and compatibility** — `DONE`. [History](docs/archive/todo-0.1.0.md#p11).
- [x] **P12 — Preserve compiler errors when stdin closes early** — `DONE`. [History](docs/archive/todo-0.1.0.md#p12).
- [x] **P13 — Record the agent repair loop** — `DONE`. [History](docs/archive/todo-0.1.0.md#p13).
- [x] **P14 — Record rejected proposals** — `DONE`. [History](docs/archive/todo-0.1.0.md#p14).
- [x] **P15 — Test deterministic agent results** — `DONE`. [History](docs/archive/todo-0.1.0.md#p15).
- [x] **P16 — Document a real agent connection** — `DONE`. [History](docs/archive/todo-0.1.0.md#p16).
- [x] **P17 — Measure agent repairs with compiler feedback** — `DONE`. [History](docs/archive/todo-0.1.0.md#p17).
- [ ] **P17a — Separate response limits from overflow-feedback value** — `OPEN`, P2.
  The frozen run's three EvoCC failures all received E3004 and exhausted the
  output budget. It establishes no missing diagnostic or safe repair.
  First examine those retained traces. A later experiment can test concise
  E3004 guidance and an intent-specific, human-reviewed overflow repair plan,
  with a registered response budget and provider policy. Saturation changes
  behavior and must not become a generic safe repair. Report output delivery
  separately from hidden-test correctness. Any new model requests need a new
  registration; do not tune or rerun failed cells of P17. This is an untested
  improvement proposal, not an observed compiler defect or an active campaign.
  October 13: the [trace
  analysis](docs/archive/notebook/agent-feedback-20261013/p17a-trace-analysis.md)
  finds that all three failures spent the 4,096-token budget on reasoning after
  a correct repair appeared in the first 4 to 21 percent of it. Every delivered
  response on the overflow items passed the hidden test (37/37). Provider
  routing was not controlled. Shorter guidance might save one or two of the
  three; a larger budget would most likely save all three. [Protocol
  A](docs/archive/notebook/agent-feedback-20261013/protocols.md) waits for the
  owner's approval: a 10-call pilot capped at USD 0.30, then 400 calls capped at
  USD 3.50. Do not rerun P17.
  October 14: `tools/experiment/budget.py` implements Protocol A, with the
  unregistered `tools/experiment/protocol-6.json` and 23 tests against a local
  fake server. The free checks pass: the G0 bytes equal P17 for 5/5 items, and
  the five Part B drafts pass the sealed scorer. The approved pilot stopped at
  request 1: OpenRouter refused it with HTTP 403, because the key had used USD
  10.45 of its USD 10 limit. The reported cost is USD 0, and no model response
  exists. The [pilot
  note](docs/archive/notebook/agent-feedback-20261013/p17a-pilot.md) and the
  [receipt](tests/contracts/agent-repair-6-pilot-20261014.json) record it.
  AtlasCloud was degraded during the run. Next action: the owner raises the key
  limit or supplies a key; then the same 10 requests run again.
  October 20: after the owner raised the key limit, the same 10 pilot requests
  ran with the compiler of `7fec1ce5`, whose G0 feedback is byte-identical to
  P17 for 5/5 items. All 10 completed from AtlasCloud with no retry, and all
  five go criteria hold: 9 delivered, valid and correct responses; one L4 G0
  response stopped at the 4,096-token limit; the largest L16 completion used
  9,715 tokens. Reported cost USD 0.018.
  [Receipt](tests/contracts/agent-repair-6-pilot-20261020.json). Next action:
  the owner approves registration and the 400-request measured run (cap USD
  3.50), or stops here.
  October 22: the owner unfroze the release binaries, which had stayed at commit
  7fec1ce5 for the agent experiments. The pilots of 2026-10-20 used that
  compiler, so a measured run now needs a new pilot with the current compiler
  and a new registration commit, because commits followed the registration of
  3a19ce9a.

- [x] **P21 — Replicate repair effort and test larger programs** — `DONE`. [History](docs/archive/todo-0.1.0.md#p21).
- [ ] **P22 — Shorten the check feedback of the agent interface; the controls of experiments 4 and 5 show its cost** — `OPEN`, P2.
  Keep separate from the registered effort experiment. Do not change its
  frozen feedback. The title retains the owner's requested follow-up, not
  an established penalty: experiment 5 controls use 60 versus 63 steps and
  1,077,325 versus 1,120,150 tokens in C3 versus C1. The aggregate penalty
  did not repeat. Do not infer a causal feedback-length effect from either
  experiment. No implementation work starts in this package.
  October 13: the
  [design](docs/archive/notebook/agent-feedback-20261013/p22-feedback-design.md)
  measures 217 feedback blocks from experiments 4 and 5 and proposes an opt-in
  `concise-1` form: 74.3 percent fewer feedback bytes, about 1.8 percent of
  session tokens. `full-1` stays the default and byte-identical for replays. The
  Rust lane implements it. [Protocol
  B](docs/archive/notebook/agent-feedback-20261013/protocols.md), 96 sessions
  and about 7 percent of the weekly Kimi quota, waits for the owner's approval;
  it cannot detect effort changes below about 16 percent.
  October 16 compatibility correction: T03s changes the completion scope in
  both forms to name unchecked pointer relations. Historical feedback fixtures
  and their byte counts remain unchanged. Current records are no longer
  byte-identical to those completions; the regression permits only this exact
  scope correction and scratch paths. Historical replays need their recorded
  compiler. This correction does not authorize a new experiment or change a
  frozen protocol.
  October 20: the owner approved F as the current `full-1`, with the T03s scope
  sentence. The [protocol
  amendment](docs/archive/notebook/agent-feedback-20261013/protocols.md) records
  the old and new sentences. `tools/experiment/agent_repair.py` adds condition
  `k`, the Protocol B order and pilot, and a precondition and go evaluation that
  allow exactly that scope difference and still record the raw comparison
  (`current_scope`, the rule of `crates/evocc/tests/frozen_feedback.rs`).
  `tools/experiment/protocol-7.json` is the unregistered machine form. The 26
  harness tests and all 176 experiment tests pass without a model request. The
  pilot has not run.
  October 20 pilot: the precondition passed (48 of 48 records in both forms
  after the scope correction). All 10 sessions completed; 9 repaired, and
  session 10 (`mean-empty`, K) failed its functional score, as `mean-empty` did
  in experiment 4. Three go criteria hold. Two fail on one escape record: the
  backstop counted a fixture that a concurrent Rust agent created in the
  repository during session 10; no tool call or wire log names it. The quota
  ratio rose by 0.0066. [Pilot
  note](docs/archive/notebook/agent-feedback-20261013/p22-pilot.md),
  [receipt](tests/contracts/agent-repair-7-pilot-20261020.json). Next action:
  the owner chooses a new pilot with no concurrent repository writer, or the
  measured run.
  October 22: the owner unfroze the release binaries, which had stayed at commit
  7fec1ce5 for the agent experiments. The pilots of 2026-10-20 used that
  compiler, so a measured run now needs a new pilot with the current compiler.

- [x] **P20 — Agent repairs of silent undefined behavior** — `DONE`. [History](docs/archive/todo-0.1.0.md#p20).
- [x] **P19 — Consistent options and checkout defaults** — `DONE`. [History](docs/archive/todo-0.1.0.md#p19).
- [x] **P18 — Measure repair cost, edit precision and safe automation** — `DONE`. [History](docs/archive/todo-0.1.0.md#p18).

### Remaining verification and application sequence

Documentation checkpoint (2026-10-11): the README's
[first optimization tutorial](README.md#build-your-first-optimization) follows the existing
add-zero rule, with runnable C and Lean examples. Native macOS execution passes for 0, 42 and
UINT32_MAX; the assembly loses one instruction, and both EqSat certificates replay successfully.
The example theorem checks with only `propext` and `Quot.sound`. No registry, compiler or proof
profile changes. This is not a new optimization or a new full-workspace acceptance result.

The V00–V06 release dependency above takes priority over starting another codegen
slice. The following table retains the separate runtime-closure sequence; it is
not a claim that formal verification replaces those tests.

| Stage | Work and dependency | Exit / stop condition |
| --- | --- | --- |
| 0 — Reconcile (this checkpoint) | Current receipts, task queues, live jobs, retained subjects and disk headroom. | Every open ID scheduled exactly once; historical PASS stays bound to its own subject. |
| 1 — Engineering batch | K02/K03 measured hot-path diagnosis first; B08b warning policy is independent CPU-light work. Follow with K10/K11f and K01/K04 reductions. | An exact RED, a shared repair or source-bound disposition, focused tests and unchanged affected-original replay per class. A code-size win alone is not a runtime repair. |
| 2 — Reviewed language batch | K07 unlocks FORTIFY/KUnit; L10 resolves LLVM's remaining classification; L04 unlocks 68 musl objects. Implement those once their bounded rules are approved. Then L03/L05–L09/L11a–c. | Both interpreters, native/crossed ABI and metadata negatives as applicable, followed by original-source replay. No silent expansion of the currently approved profile. |
| 3 — Kernel acceptance | Kernel-affecting repairs/dispositions + K07 + T03 → T04. Build the BPF/selftest and independent expanded-KUnit configurations with matching module payloads. | K13's complete 970-program pair and K14/T02's 1,504 built-in + 765 cases in 31 loaded modules per side. Non-PASS cases retain individual reasons and zero missing results. |
| 4 — Application acceptance | Relevant language/option fixes + B08 reference/patched-lane accounting + T03 → freeze one compiler/profile. | B10's 15 lanes serially, unchanged populations and original flags. Report originals, patched supplements and approved deferrals separately. Finish T05 on the same source. |
| 5 — Full roadmap | After ARM64 correctness stabilizes: R01 → R02 → R03; R04 is a separate backend project; R05 audits the whole delivered scope. | Actual optimization/proof/performance/backend/release evidence, not a renamed ARM64 milestone. |

Stages 1 and the review work in stage 2 are independent workstreams, **not a
request to run multiple heavy jobs at once**. Kernel acceptance does not wait
for floating complex, half/vector FP, generic atomics or x86-64 unless an actual
kernel source requires them. Conversely, K13/K14 cannot close musl or B10.
No calendar completion date is asserted before intermittent failures have
reliable reductions and the numerical decisions have an accepted contract.

Immediate boundaries that affect the schedule:

- **K02/K03:** October 19's latest original demand-paging and memslot-stress
  pairs each complete 10/10 modes on GCC and 7/10 on EvoCC. Both EvoCC runs
  time out. The fixed 256 MiB diagnostic has identical page-fault counts per
  pair and zero dropped records. EvoCC executes 2.61 times the total
  instructions and 2.79/2.80 times the kernel instructions. Inspect the
  retained hot functions before another speculative peephole or rebuild. Keep
  the 1 GiB / ten-mode / 120-second original gate; do not replace it with a
  quiet-host guess or the passing reduced workload.
- **B08b:** warning requests already survive both adapters. Current faithful
  musl commands therefore refuse unsupported warnings; the older 1,271/1,339
  receipt contains omissions. The next job is actual diagnostic policy and
  diagnostics, **not another projection fix** or accepting `-Werror` as a no-op
  ([GCC 14.2 warning policy](https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Warning-Options.html)).
- **Review:** the supplied scalar/strict/binary128 report closed L01/L02, not
  floating complex or the other pending decisions. Use the existing
  [focused handoff][complex-prompt]. No fresh user approval is needed for ordinary
  engineering under an existing contract; genuine new language/ABI choices
  remain explicitly review-gated.
- **Storage/runtime:** apply the October 2 disk discipline above; the earlier
  61 GiB checkpoint is historical, not live headroom. Reuse the
  existing small diagnostic payloads and 64 MiB scratch disk. Keep one heavy
  run active, check required headroom before launches, and never delete active
  or sole retained evidence. A new configuration/build must first identify what
  it replaces and which historical artifacts will remain recoverable.
- **Stop repeating uninformative experiments:** each bounded diagnostic must
  name the competing causes and the observation that distinguishes them. If
  that observation cannot be obtained, record the specific missing capability
  and switch to an independent ready task; do not count retries as fixes.

No item below is marked actively running merely because it is queued here.

Storage checkpoint (2026-09-23): superseded runtime disks and generated staging/
assembly caches were retired; in-place compaction recovered **18.89 GiB**.
All ten retained Image/config/runtime hashes match. Small module diagnostics now
use one 64 MiB scratch disk and existing initramfs payloads; both kernel controls
booted, ran the PM reduction and powered off. This is environment maintenance,
not K13/K14 acceptance. [Retention and recovery details][kernel].

## Kernel runtime and acceptance

K6 boot is already accepted for the locked 2,799-target-C configuration. The
6,765-object BPF/module composition also links and boots. Neither establishes
complete K7/KUnit success; these configurations and their Images are distinct.

Direct Kbuild checkpoint (2026-10-11): Linux 6.18.44 `tinyconfig` builds from
an empty output tree with `CC=evocc`, fixed compiler `e5bf1a72`, and no seeded
GCC target objects. The build produces 446 C objects and four C-to-assembly
helpers; host tools, assembly and linking still use the GNU toolchain.
Driver capability probes, assembly stdin/preprocessing/dependencies, masked
asm immediates and inlined pointer assignments are repaired. A clean build
generates its own EvoCC compiler banner. The full offline gate passes 425
suites: 3,249 tests, zero failures, five existing ignores; Clippy/fmt and 115
related Python tests pass. Seven unsupported options remain disclosed.
**Boot is NOT RUN.** This does not replace the historical 7,008-unit boot or
close any runtime acceptance item below. [Receipt](tests/contracts/kernel-direct-tiny-receipt.json)
and [scope/retention record][kernel].

Acceptance preflight (2026-10-11): the exact tinyconfig disables initramfs,
ELF loading, printk and TTY. It cannot satisfy the `/init` and serial-nonce
boot predicate. The historical 7,008-unit config (`95853321…`) and matching
runtime pair are no longer present after retirement of `.build/k7-coverage`.
No matching config was found among 235 local config candidates or the four
retained embedded reference configs. QEMU is now 11.1.2, not the contract's
11.0.3. Restore the original inputs, or obtain approval for a newly frozen
paired baseline before a replacement campaign. Boot and kselftests remain
NOT_RUN for the direct build; no historical verdict changes. [Details][kernel].

Fresh direct-build replay (2026-10-12): the retained arm64 virtconfig input
now builds with EvoCC `03600f80`. All **2,799 target C objects** and five
C-to-assembly helpers match this compiler's recorded invocations; no GCC
target C object is seeded or substituted. The new Image passes **HVF and TCG
boot**, including `/init`, the current nonce and shutdown. The unchanged nine
selected upstream kselftests pass on both the GCC reference and this Image:
**125/125 subtests each, zero failures or skips**. The full offline workspace
gate passes 425 suites: **3,253 PASS / 0 FAIL / 5 existing ignores**, with
Clippy/fmt green. This repairs the conflicting-macro capability probe and a
false missing-return diagnostic for computed goto. Nine ignored compiler
options and natural olddefconfig capability differences remain disclosed.
QEMU 11.1.2 is an explicit drift, not the locked 11.0.3 environment. This is
not the missing 7,008-unit configuration, full 970-program acceptance or
KUnit, and it closes none of those separate tasks.
[Receipt](tests/contracts/kernel-direct-virt-receipt.json) and [details][kernel].

Frozen release replay (2026-10-27): the frozen compiler of `08bf09bd` builds the retained
arm64 virtconfig input in a fresh output tree, in one uninterrupted Kbuild run (5,084 s of
the 7,200 s limit). All 2,799 target C objects and the 5 C-to-assembly helpers come from one
compiler, `3520c267…`; no target C comes from GCC and no object is seeded. The GCC reference
and the EvoCC Image boot on HVF with pointer authentication and on TCG, with fresh nonces.
The nine selected programs pass on both kernels with 125/125 subtests. QEMU 11.1.2 stays a
disclosed drift. The original KVM programs still reach their 120-second limit on EvoCC after
6 of 10 modes (GCC 10/10), so K02 and K03 stay open. This is not the 7,008-unit composition,
full kselftest or KUnit acceptance.
[Receipt](tests/contracts/kernel-direct-virt-receipt.json).

- [ ] **K01 — Diagnose the intermittent sched_ext kernel NULL fault** — `OPEN`, P1.
  `hotplug` → `reload_loop` reproduces the fault on both EvoCC and GCC; an outer
  runner exit of zero can conceal inner failures. Reduce/trace the static-key and
  scheduler-lifetime path without GDB-induced timing assumptions. Close with a
  demonstrated cause and unchanged paired replay, or an evidenced shared/upstream
  disposition; one successful retry and the failed single-thread TCG experiment
  do not close it. [Evidence][kernel].
  The [small signed-module matrix](tests/fixtures/sched-static-key-lifetime.out)
  now reproduces missing publication on both kernels without BPF/hotplug/freeing.
  Ordinary memory-gate and sequence-bounded static-key controls pass; instruction
  and data-barrier variants do not cure the concurrent RCU reduction. Root cause
  and linkage to the original fault remain unproved. The bounded diagnosis slice
  is recorded; next isolate read-side entry/retirement, not a production NULL guard.
  The [phase observer](tests/fixtures/sched-static-key-phases.out) now sees most
  misses after disable/grace-period/clear, even with reader preemption disabled.
  The identical payload passes all three modes on both HVF and single-thread
  TCG; only multi-thread TCG fails. This is a narrower runtime discrepancy,
  not a proven emulator cause or an unchanged sched_ext PASS.
  September 26's identical-payload MT-TCG `-d nochain` control still fails the
  static and preemption-disabled modes on both Images (memory controls pass).
  Direct-block chaining is not a necessary cause; the control is not a repair.
  October 13: the unchanged replay is NOT_RUN. Its 7,008-unit Images,
  configuration and payloads were deleted on October 7 with
  `.build/k7-coverage`, and the current virtconfig pair lacks `SCHED_CLASS_EXT`.
  The [timeout diagnosis](tests/fixtures/k01-sched-ext-timeout-diagnosis.out)
  finds a slowdown, not a hang and not the MT-TCG defect: the cache-maintenance
  QEMU that cures the reductions still times out. On the current pair, the
  `init_enable_count` fork/exit/wait pattern executes 9.1 to 10.3 times GCC's
  instructions. EvoCC's `copy_pte_range` is one 1,329,200-byte function and
  takes 2.06 G of 9.04 G instructions; GCC's whole `copy_page_range` is 8,992
  bytes. Replay on the T04 composition with `SCHED_CLASS_EXT` before closing.

  - [x] **K01a — Fail closed on truncated sched_ext result summaries** — `DONE`. [History](docs/archive/todo-0.1.0.md#k01a).
  - [x] **K01b — Resolve the concurrent instruction-publication runtime discrepancy** — `DONE`. [History](docs/archive/todo-0.1.0.md#k01b).
- [ ] **K02 — KVM demand_paging_test timeout** — `OPEN`, P1.
  The subject reaches the original 120-second limit after three guest modes;
  the reference completes. Profile the exact fixed-CPU/EL2 lane, isolate the hot
  path and correct any code-generation/runtime cause. Close with the complete
  original mode matrix and output checks inside the unchanged budget. [Evidence][kernel].
  The [bounded paired profile](tests/fixtures/kvm-demand-fault-profile.out)
  reproduces a 1.49 s / 5.22 s gap at 64 MiB. A 256 KiB function graph has the
  same 87 fault-handler invocations on both kernels and no dropped events;
  excess time spans page acquisition/mapping/release, not one proven stall.
  The 4.1 MiB diagnostic reuses the 64 MiB scratch disk. No original PASS claim.
  [Integrated two-object replay](tests/fixtures/kvm-hotpath-original-replay.out):
  the repaired Image boots, but the original default workload still times out
  after three complete modes; GCC finishes all ten. K02 remains OPEN.
  The September 26 [assembly-transport candidate](tests/fixtures/kvm-transport-replay.out)
  again completes three modes before 120 seconds; GCC completes ten. All 6,765
  installed objects and generated inputs survive linking. Smaller code in two
  objects does not close a whole-kernel performance gap.
  September 28's [uniform 7,002-object original replay](tests/fixtures/kvm-uniform-original-replay.out)
  still times out after 3/10 modes; matching GCC completes 10/10. Both use the
  unchanged 1 GiB workload and 120-second limit. All inputs remain unchanged;
  neither the cumulative code-size repairs nor a successful boot closes K02.
  The later pinned **7,008-object** composition also times out after 3/10;
  this round's matching GCC reaches 7/10 and also times out. Both enter the
  next mode and shut down normally. Retain this new common deadline failure
  rather than importing the older GCC PASS; no runtime speedup is established.
  October 13's [TCG instruction
  profile](tests/fixtures/kernel-tcg-instruction-profile.out) uses the retained
  virtconfig pair, because the 7,008-object pair was deleted on October 7. For
  the same 65,687 stage-2 faults, EvoCC executes 5.25 times GCC's instructions
  (kernel code 5.60 times). At least 36 percent of the excess is block-argument
  transport through stack slots: such copy runs are 29.6 percent of EvoCC's
  executed kernel instructions and 1.6 percent of GCC's. `user_mem_abort` runs
  3,649 instructions per fault (GCC 159). About 23 percent of the excess is
  time-scaled waiting that the slowdown amplifies. The
  [reductions](tests/fixtures/kernel-codegen-reductions/) isolate the causes;
  N08 to N11 hold the repairs. Replay the unchanged original after them.
  October 19's [direct-build replay](tests/contracts/kernel-direct-virt-receipt.json)
  includes the current N08-N11 changes in all 2,799 target C objects, rebuilt
  with one retained compiler snapshot. Both boot lanes and all 125 paired
  selected subtests pass. The original 120-second demand-paging run completes
  10/10 modes on GCC but only 7/10 on EvoCC, then times out. K02 stays open;
  the fixed-work instruction profile is diagnostic, not a replacement exit.
  The unchanged 256 MiB diagnostic completes on both kernels: each has
  65,682 `__handle_mm_fault` entries. EvoCC/GCC execute 1,754,085,900 /
  672,400,059 total instructions (2.61x), including 1,683,998,173 /
  602,619,606 kernel instructions (2.79x). Long transport runs account for
  6.09% / 1.96% of kernel instructions. Dropped and irregular records are zero.
  This is an integrated checkpoint, not causal attribution to one repair or
  native timing; the diagnostic's 1,200-second budget is not the original exit.
  October 23 (diagnostic TCG profile of the rebuilt K16 Image 63087e12, compiler
  f6c6f5e1): the fixed 256 MiB demand-paging VM runs 2.79 times GCC's kernel
  instructions (October 13: 5.43). The unchanged original still ends at the
  120-s limit with EvoCC, while GCC now finishes it on a quiet host.
  [Record](tests/fixtures/kernel-tcg-profile-20261023.out).

  - [x] **K02a — Remove redundant negated-truth materialization** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02a).
  - [x] **K02b — Preserve assembly operands while removing trivial transport** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02b).
  - [x] **K02c — Select bounded constant memory displacements** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02c).
  - [x] **K02d — Avoid duplicate automatic-zero value stores** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02d).
  - [x] **K02e — Use either adjacent conditional successor as fall-through** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02e).
  - [x] **K02f — Select bounded constant shift immediates** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02f).
  - [x] **K02g — Select bounded constant add/sub immediates** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02g).
  - [x] **K02h — Select and validate full-width logical immediates** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02h).
  - [x] **K02i — Select and validate bounded immediate comparisons** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02i).
  - [x] **K02j — Select direct-null tests and repair MIR null transport** — `DONE`. [History](docs/archive/todo-0.1.0.md#k02j).
- [ ] **K03 — KVM memslot_modification_stress_test timeout** — `OPEN`, P1.
  The subject is interrupted in its third mode while the reference completes.
  Isolate this separately from K02 until a shared cause is demonstrated; retain
  all modes and the 120-second limit. Close with unchanged paired execution,
  not a faster isolated compiler fixture. [Evidence][kernel].
  The [small paired memslot profile](tests/fixtures/kvm-memslot-fault-profile.out)
  now separates invalidation and destruction: both kernels perform four page-table
  destructions and two invalidations without trace loss. Root-only tracing still
  measures 0.208 / 1.274 s (GCC / EvoCC); destruction is 3.898 / 81.148 ms, so it
  is not the entire gap. Both reduced TAP tests and shutdowns pass; original
  all-mode acceptance remains open. Next isolate the repeated page-table helpers.
  The [new linked candidate](tests/fixtures/kvm-hotpath-original-replay.out)
  installs the two gated KVM repairs but still times out in the fourth mode;
  GCC completes all ten. Both shutdowns pass; this is not K03 closure.
  September 26's [new two-object replay](tests/fixtures/kvm-transport-replay.out)
  has the same original-program timeout: three completed modes, ten for GCC.
  The September 28 [uniform-composition replay](tests/fixtures/kvm-uniform-original-replay.out)
  confirms the same 3/10 versus 10/10 outcome under the unchanged workload,
  fixed-CPU/EL2 lane and timeout. No new full-test PASS is inferred.
  The later pinned 7,008-object replay yields subject **3/10 TIMEOUT** and
  reference **7/10 TIMEOUT**, with orderly shutdown on both. The current
  original remains unclosed; smaller current-image profiles are diagnostic,
  not substitutes for the unchanged full workload.
  October 13's [TCG instruction
  profile](tests/fixtures/kernel-tcg-instruction-profile.out): for the same 256
  MiB of work EvoCC executes 5.43 times GCC's instructions, with K02's ranking.
  Memslot code is under 1 percent of the excess, so K03 shares K02's fault,
  allocate and unmap cause. It stays open until the unchanged original passes.
  October 19's [direct-build replay](tests/contracts/kernel-direct-virt-receipt.json)
  completes 10/10 original modes on GCC and 7/10 on EvoCC before the unchanged
  120-second limit. Both kernels shut down normally. This replaces the earlier
  6/10 versus 3/10 checkpoint, whose logs remain retained; it does not establish
  a native speedup. K03 stays open.
  The unchanged 256 MiB diagnostic completes on both kernels: each has
  65,679 `__handle_mm_fault` entries. EvoCC/GCC execute 1,761,065,434 /
  674,646,195 total instructions (2.61x), including 1,690,958,878 /
  604,819,730 kernel instructions (2.80x). Long transport runs account for
  6.07% / 1.96% of kernel instructions. Dropped and irregular records are zero.
  Snapshot 7 covers VM creation through destruction. This remains a diagnostic,
  with full translated blocks charged at entry and concurrent waits included.
  October 23 (same profile): the fixed 256 MiB memslot VM runs 2.79 times GCC's
  kernel instructions (October 13: 5.81). The unchanged original still ends at
  the 120-s limit with EvoCC, while GCC now finishes it.
  [Record](tests/fixtures/kernel-tcg-profile-20261023.out).

  - [x] **K03a — Substitute bounded private leaf helpers** — `DONE`. [History](docs/archive/todo-0.1.0.md#k03a).
  - [x] **K03b — Inline a single-site visitor without expanding runtime recursion** — `DONE`. [History](docs/archive/todo-0.1.0.md#k03b).
- [ ] **K04 — ublk stress/shutdown stall** — `OPEN`, P1.
  `test_stress_04.sh` hits the 45-second wrapper limit and stalls in shutdown sync;
  a later diagnostic invocation passes in 4.983 seconds. Capture the blocked I/O
  path and establish cause before changing anything. Close with the original
  workload and clean shutdown, or a source-bound non-compiler disposition;
  intermittency alone is not a fix. [Evidence][kernel].
  September 27's [bounded blocked-stack observer](tests/fixtures/ublk-blocked-observer.out)
  preserves the original workload/budget. Six new pairs all pass and shut down;
  no invocation reaches the 25-second diagnostic or shows a blocked final task.
  One initial reference capture is INVALID_OUTPUT from interleaved printk, not
  PASS. The 835 KiB initramfs adds no full disk. The old stall remains unexplained;
  retain K04 rather than attributing the successful retries to a compiler fix.
  September 28's [39-program original pair](tests/fixtures/kernel-uniform-selected-runtime.out)
  again reproduces GCC stress_04's 45-second timeout and blocked shutdown;
  EvoCC passes that invocation. The cause and full K13 acceptance remain open.
  A subsequent original GCC failure now has real blocked-task evidence:
  two writeback workers wait in `blk_mq_get_tag`, while fio/io_uring readers
  wait on folios. The 45-second inner TIMEOUT is preserved despite wrapper
  exit zero; shutdown still exceeds the 105-second outer bound. This narrows
  the stall to request/writeback progress, without proving its cause. Expanded
  per-thread/device observation then gives 12 successful original invocations;
  these negative reproductions are not a fix. See the same observer record.
  The [single-device C reduction](tests/fixtures/ublk-kill-cancel.out) passes
  4 cold-null + 3 warm-loop cases per retained Image, including real queued I/O,
  forced writeback, bounded client/deletion completion and normal shutdown.
  Earlier fixture setup failures are retained separately. This does not
  reproduce the multi-device original or justify a compiler/kernel repair.
  Running-daemon controls then pass 6/6 (no preceding SIGSTOP). The new
  [shared-backing pair](tests/fixtures/ublk-shared-cancel.c) reproduces one GCC
  deletion exceeding its eight-second diagnostic bound after all clients exit;
  later cases are NOT_RUN, and shutdown still completes. EvoCC passes 3/3.
  A follow-up observed pair passes 3/3 each, not a fix. This is narrower than
  the original blocked-writeback/45-second failure and remains separately
  recorded. Later source inspection finds kublk's normal close-notification
  fallback itself waits **10 seconds**, exceeding the diagnostic's old 8-second
  bound. That observation alone cannot prove a kernel latency bug; retain the
  historical failure and use the source-qualified syscall control. Do not waive
  K04 or lengthen the upstream watchdog.
  October 13's [stall record](tests/fixtures/k04-ublk-stall-disposition.out):
  the original pair is NOT_RUN, because its inputs were deleted on October 7 and
  virtconfig lacks `BLK_DEV_UBLK`. The retained campaigns time out in 2 of 4 GCC
  and 2 of 4 EvoCC captures, so the stall is shared. stress_04 deletes a device
  only after it reads DEAD, which sets `ublksrv_pid` to -1, so kublk's 10-second
  fallback is not on this path. DEAD needs `del_gendisk` to return. A request
  that never completes keeps the kill loop polling into the 45-second limit and
  blocks the final sync, as the writeback threads in `blk_mq_get_tag` show. The
  incomplete request is not identified, and no upstream fix exists. Replay on
  the T04 composition with `BLK_DEV_UBLK`, and capture the request and reference
  holders at the stall.

  - [x] **K04b — Isolate ublk completion-order assertions** — `DONE`. [History](docs/archive/todo-0.1.0.md#k04b).
  - [x] **K04a — Preserve ublk worker failures beneath a successful wrapper** — `DONE`. [History](docs/archive/todo-0.1.0.md#k04a).
- [x] **K05 — PM runtime KUnit pending-resume race** — `DONE`. [History](docs/archive/todo-0.1.0.md#k05).
- [x] **K06 — Build and run the IOPF-enabled configuration** — `DONE`. [History](docs/archive/todo-0.1.0.md#k06).
- [x] **K07 — Define and implement nonliteral strlen ICE admission** — `DONE`. [History](docs/archive/todo-0.1.0.md#k07).
- [x] **K08 — Disposition the seven shared EL1 breakpoint failures** — `DONE`. [History](docs/archive/todo-0.1.0.md#k08).
- [x] **K09 — Disposition ntsync wake_all's return-value expectation** — `DONE`. [History](docs/archive/todo-0.1.0.md#k09).
- [ ] **K10 — Replay and diagnose the full BPF test_progs program** — `OPEN`, P1.
  The [fresh unchanged replay](tests/fixtures/bpf-complete-replay.out) on matching
  BTF/module payloads completes on GCC (651/5644 successful test/subtest counts,
  127 skips, 11 failed tests). EvoCC completes group 482 then times out at the
  unchanged 300-second limit; both guests shut down normally. Known PMU, errno,
  UDP-state and verifier-budget causes remain source-bound dispositions, not
  PASS. BBR/net_cls need K12; stacktrace and stream assumptions are now
  source-bound dispositions below. The 300-second whole-program timeout remains.
  Close with complete nested accounting on both sides after repair, not the
  earlier selected 10-group/51-subtest controls. [Evidence][kernel].
  October 13's [TCG instruction
  profile](tests/fixtures/kernel-tcg-instruction-profile.out): no retained Image
  has BTF, so `test_progs` cannot run. The retained logs show GCC using over 285
  of its 300 seconds, and EvoCC reaching group 482 1.13 to 1.26 times later: a
  spread-out slowdown, not one stuck test. A raw-BPF proxy workload runs 3.75
  times GCC's kernel instructions for the same work (verifier about 3 times,
  hash-map updates 13 times). It needs N08 to N11 and a BTF-capable pair.
  October 23 (same profile): the raw-BPF proxy runs 2.54 times GCC's kernel
  instructions (October 13: 3.75); the largest excess is the verifier's
  `backtrack_insn` (2.58 times, 16.7 percent).
  [Record](tests/fixtures/kernel-tcg-profile-20261023.out).

  - [x] **K10a — Reject incomplete BPF summaries and label selected replays** — `DONE`. [History](docs/archive/todo-0.1.0.md#k10a).
  - [x] **K10b — Pair the known PMU/lock/UDP API reductions** — `DONE`. [History](docs/archive/todo-0.1.0.md#k10b).
  - [x] **K10c — Diagnose consecutive stack build-id helper results** — `DONE`. [History](docs/archive/todo-0.1.0.md#k10c).
  - [x] **K10d — Diagnose the timer callback's premature stream read** — `DONE`. [History](docs/archive/todo-0.1.0.md#k10d).
- [ ] **K11 — Finish case-level triage of all other kselftest outcomes** — `OPEN`, P1.
  Shared FAIL/TIMEOUT/INVALID_OUTPUT/INCOMPLETE and SKIP results are not all
  explained. Map each to a source/config/runtime/compiler cause, extract a small
  reproducer per new class and assign a child ID. Close when every non-PASS has
  evidence and an action or justified limit; no blanket “both fail” exemption.
  Use the [full receipt][selftest-receipt] and [ledger][kernel].

  - [x] **K11a — Restore /tmp's mountpoint prerequisite on ext4** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11a).
  - [x] **K11b — Verify harness constructor-order failure** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11b).
  - [x] **K11c — Supply udmabuf's actual HugeTLB requirement** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11c).
  - [x] **K11d — Demonstrate the frozen ptrace/exec wait cycle** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11d).
  - [x] **K11e — Verify rseq's unregistered-thread prerequisite** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11e).
  - [ ] **K11f — Retest pidfd's recycling-loop timeout on the final Image** — `OPEN`, P1.
    The [bounded syscall reduction](tests/fixtures/pidfd-recycle-progress.out)
    completes 1024 stop/continue/reap cycles in root and fresh namespaces on
    both kernels. The namespace's actual 4M PID limit explains the original's
    bounded-search SKIP. Fresh unchanged originals both finish within 45s with
    7 PASS / 1 SKIP, but the historical subject timeout and measured slowdown
    are not declared repaired by that retry. Preserve them for K13's final pair.
    September 28's new pinned 7,008-object pair again completes the unchanged
    original: both have 7 nested PASS / 1 SKIP, with shutdown at 17.505/42.139s.
    The original 45-second limit, disk/payload hashes and historical timeout
    remain intact. This updates current-image evidence but does not prove a
    performance repair or a complete-campaign margin. It excludes K02j's later
    compiler change. [Current capture](tests/fixtures/pidfd-recycle-progress.out).
    October 13's [TCG instruction
    profile](tests/fixtures/kernel-tcg-instruction-profile.out): per fork/exit
    cycle EvoCC executes 11.6 times GCC's kernel instructions. `copy_pte_range`
    alone runs 206 times GCC's count and holds 36.5 percent of the excess: EvoCC
    builds it as one 1,329,200-byte function that runs about 1,700 instructions
    per page-table slot. It stays open until N08 to N11 land and the final pair
    passes.
    October 19's current 2,799-object pair again completes the unchanged
    45-second original with 7 PASS / 1 SKIP on both kernels. The shared skip
    is a coverage gap, not a reference failure or an EvoCC regression. The
    comparison now records it separately; a new subject-only skip still
    fails. This does not close the historical timeout or full-campaign exit.
    October 23 (static sizes, no kernel run): the `mm/memory.c` of the K16 tree,
    preprocessed with GCC 14 and compiled under the kernel flags, gives a
    21,404-byte `copy_pte_range` (5,381 instructions, 1,904-byte frame) with the
    release compiler of 31b4e6e2, and 1,329,212 bytes with the profile's frozen
    compiler 820bfa29. All text of the unit falls from 2,425,156 to 236,812
    bytes; GCC 14 gives 61,848 bytes and inlines `copy_pte_range` into a
    9,000-byte `copy_page_range`. The dynamic ratios of October 13 therefore
    describe the old compiler. A new TCG profile with the current compiler comes
    before any fix is chosen.
    October 23 (TCG profile of the K16 Image rebuilt on October 19, 63087e12,
    compiler f6c6f5e1): per fork/exit cycle EvoCC runs 2.83 times GCC's kernel
    instructions (October 13: 11.6), and `copy_pte_range` 4.19 times (205.94). The largest excess is now
    `zap_present_ptes` (5.42 times, 7.6 percent); boot to the first mark is 2.81
    times. Both originals still end at the 45-second limit of the recycling
    loop. [Record](tests/fixtures/kernel-tcg-profile-20261023.out).

  - [x] **K11g — Attribute delayed dying-memcg release** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11g).
  - [x] **K11h — Bound immediate memcg-deletion accounting** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11h).
  - [x] **K11i — Verify arm64 syscall-user-dispatch's unavailable boundary** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11i).
  - [x] **K11j — Separate seccomp values from timing-model assertions** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11j).
  - [x] **K11k — Isolate OVPN's frozen key-prefix failure** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11k).
  - [x] **K11l — Attribute the shared BPF module hardening timeout** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11l).
  - [x] **K11m — Preserve FPU worker skips/failures and completion counts** — `DONE`. [History](docs/archive/todo-0.1.0.md#k11m).
- [ ] **K12 — Close collection-build and configuration coverage gaps** — `OPEN`, P2.
  The separate Kconfig review resolves 100 disabled requests in 27 collections;
  the uniform 7,002-object composition now boots, and both new runtimes install
  all 847 signed kernel modules plus 17 original test-support modules. The
  [new original pair](tests/fixtures/kernel-uniform-module-runtime.out) passes
  NCI's 8/8 cases, both splice programs and IOMMU's 224/0/5 inner results.
  FPU still SKIPs for a missing configuration; BPF test_kmod still times out on
  both sides. This selected six-program run leaves 964 NOT_RUN, not K13 PASS.
  Other affected collections still require replay. The four build gaps are
  source-bound below.
  Build runnable supported sets with matching payloads; give precise reasons
  for unavailable ones. Keep any changed population separate from the frozen 970. [Evidence][kernel].

  - [x] **K12a — Classify four architecture-gated collection recipes** — `DONE`. [History](docs/archive/todo-0.1.0.md#k12a).
  - [x] **K12b — Prove bitmap helper results before constant queries** — `DONE`. [History](docs/archive/todo-0.1.0.md#k12b).
  - [x] **K12c — Register the installed virtual NCI device before tests** — `DONE`. [History](docs/archive/todo-0.1.0.md#k12c).
  - [x] **K12d — Install OVPN's omitted original peer tables and key** — `DONE`. [History](docs/archive/todo-0.1.0.md#k12d).
  - [x] **K12e — Enable the missing connector process-event configuration** — `DONE`. [History](docs/archive/todo-0.1.0.md#k12e).
  - [x] **K12f — Enable and execute the omitted kernel FPU module** — `DONE`. [History](docs/archive/todo-0.1.0.md#k12f).
- [ ] **K13 — Fresh complete paired kselftest acceptance** — `OPEN`, P1, after K01–K12/T04.
  The last 970-program pair is INCOMPLETE and bound to an older Image; its counts
  cannot be transferred to repaired Images. Run the whole installed population
  serially with matched payloads, audit nested verdicts and zero lost programs,
  and report all failures/skips separately. Close the execution task with fresh
  receipts; claim all-tests PASS only if the actual acceptance predicate holds.
  [Historical receipt][selftest-receipt].
  The September 28 [selected replay](tests/fixtures/kernel-uniform-selected-runtime.out)
  starts 39/39 on each Image: GCC 28 PASS / 2 FAIL / 9 TIMEOUT, EvoCC
  29 PASS / 1 FAIL / 9 TIMEOUT. The other 931 remain NOT_RUN; this is not a
  new complete campaign. The owner/worker has exited with INCOMPLETE.

- [ ] **K14 — Fresh complete expanded built-in and module KUnit acceptance** — `OPEN`, P1.
  After K05/K07/K08 and final composition, rerun all 1,504 built-in cases and
  actually load all 31 test modules (765 cases) on each matching kernel. Preserve
  duplicate-case multiplicity, the upstream switch-entry XFAIL and config limits.
  Later filtered FORTIFY results cannot rewrite the old full totals or inherit
  module acceptance from another Image. [Historical receipt][kunit-receipt].

- [x] **K15 — Sign kernel return addresses when Kbuild asks for it** — `DONE`. [History](docs/archive/todo-0.1.0.md#k15).
- [x] **K16 — Apply `used` written after a pointer `*` as GCC does** — `DONE`. [History](docs/archive/todo-0.1.0.md#k16).
- [x] **K17 — Name the affected option in unsupported-option warnings** — `DONE`. [History](docs/archive/todo-0.1.0.md#k17).

## Application failures, accounting and replays

- [x] **B01 — Reconcile every original LLVM failure with subsequent repairs** — `DONE`. [History](docs/archive/todo-0.1.0.md#b01).
- [x] **B02 — LLVM LARGE floyd-warshall performance failure** — `DONE`. [History](docs/archive/todo-0.1.0.md#b02).
  - [x] **B02a — Reuse the value computed by checked add/sub** — `DONE`. [History](docs/archive/todo-0.1.0.md#b02a).
  - [x] **B02b — Reuse stable compared integer values** — `DONE`. [History](docs/archive/todo-0.1.0.md#b02b).
  - [x] **B02c — Select bounded compared-value muxes with CMP/CSEL** — `DONE`. [History](docs/archive/todo-0.1.0.md#b02c).
  - [x] **B02d — Branch directly on single-use checked flags** — `DONE`. [History](docs/archive/todo-0.1.0.md#b02d).
- [x] **B03 — Account for Csmith's 68 unusable-oracle cases** — `DONE`. [History](docs/archive/todo-0.1.0.md#b03).
- [x] **B04 — Resolve remaining torture/HVA reference limitations** — `DONE`. [History](docs/archive/todo-0.1.0.md#b04).
- [x] **B05 — Replace TinyCC's final two reference-built FP objects** — `DONE`. [History](docs/archive/todo-0.1.0.md#b05).
- [x] **B06 — Finish toybox compilation and chattr diagnosis** — `DONE`. [History](docs/archive/todo-0.1.0.md#b06).
  - [x] **B06a — Isolate chattr's filesystem requirement** — `DONE`. [History](docs/archive/todo-0.1.0.md#b06a).
  - [x] **B06b — Run filesystem-sensitive tests on container-local storage** — `DONE`. [History](docs/archive/todo-0.1.0.md#b06b).
  - [x] **B06c — Repair runtime prerequisites and classify the remaining tar failure** — `DONE`. [History](docs/archive/todo-0.1.0.md#b06c).
- [x] **B07 — Finish jq's binary128 interface and original tests** — `DONE`. [History](docs/archive/todo-0.1.0.md#b07).
  - [x] **B07a — Count jq's `src/main.c` and rerun `make check`** — `DONE`. [History](docs/archive/todo-0.1.0.md#b07a).
- [x] **B08 — Build and test musl with its actual semantic flags** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08).
  - [x] **B08a — Admit scalar FP inline-assembly bindings** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08a).
  - [x] **B08b — Close musl's remaining command-projection semantics** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08b).
    - [x] **B08b1 — Preserve already-satisfied conservative codegen options** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08b1).
    - [x] **B08b2 — Preserve unsupported semantic requests and bound GCC tuning** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08b2).
    - [x] **B08b3 — Preserve include search classes and joined operands** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08b3).
    - [x] **B08b4 — Preserve diagnostic requests instead of false acceptance** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08b4).
  - [x] **B08c — Close original musl pthread runtime regressions** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08c).
  - [x] **B08d — Resolve the reference libc-test failures and static/DSO coverage gap** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d).
    - [x] **B08d1 — Supply the atfork limit test's unprivileged environment** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d1).
    - [x] **B08d2 — Execute the five DSO/TLS subjects with a genuine loader** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d2).
    - [x] **B08d3 — Localize the two strtold binary128 reference failures** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d3).
    - [x] **B08d4 — Localize powf's upward-overflow reference failure** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d4).
    - [x] **B08d5 — Attribute the strptime and API/header baseline gaps** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d5).
    - [x] **B08d6 — Isolate descriptor-exhaustion output from host virtiofs** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d6).
    - [x] **B08d7 — Separate wordexp's library and shell failure families** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d7).
    - [x] **B08d8 — Repair numerical bugs in an explicitly patched dependency lane** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d8).
    - [x] **B08d9 — Verify explicit patched strptime compatibility** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d9).
    - [x] **B08d10 — Complete patched wordexp behavior** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d10).
    - [x] **B08d11 — Repair the missing API names in the patched dependency lane** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d11).
    - [x] **B08d12 — Repair wordexp allocation ownership and error handling** — `DONE`. [History](docs/archive/todo-0.1.0.md#b08d12).
- [x] **B09 — Exercise cJSON's complete upstream private-helper suite** — `DONE`. [History](docs/archive/todo-0.1.0.md#b09).
- [x] **B10 — One final serial 15-lane application campaign** — `DONE`. [History](docs/archive/todo-0.1.0.md#b10).
  - [x] **B10a — Restore the evidence that the disposition rows cite** — `DONE`. [History](docs/archive/todo-0.1.0.md#b10a).

## Remaining numerical and language contracts

These are not proposals to redo scalar f32/f64, integer vectors, TLS,
constructors/destructors, GNU inline or raw-byte lexing. Those shared capabilities
have implementations and targeted evidence. The following boundaries remain
explicit; [the research brief][semantics] supplies the detailed probe contracts.

- [x] **L01 — Strict floating environment** — `DONE`. [History](docs/archive/todo-0.1.0.md#l01).
- [x] **L02 — Executable binary128 / long double** — `DONE`. [History](docs/archive/todo-0.1.0.md#l02).
- [x] **L03 — Binary16/bfloat16 executable boundaries** — `DONE`. [History](docs/archive/todo-0.1.0.md#l03).
- [x] **L04 — Floating complex execution** — `DONE`. [History](docs/archive/todo-0.1.0.md#l04).
- [x] **L05 — Floating vector execution and intrinsics** — `DONE`. [History](docs/archive/todo-0.1.0.md#l05).
- [x] **L05b — `arm_neon.h` intrinsics for the NEON original** — `DONE`. [History](docs/archive/todo-0.1.0.md#l05b).
- [x] **L06 — Sub-8-byte integer-vector external ABI** — `DONE`. [History](docs/archive/todo-0.1.0.md#l06).
- [x] **L07 — Integer-complex division** — `DONE`. [History](docs/archive/todo-0.1.0.md#l07).
- [x] **L08 — Pointer-to-VLA typeof operand evaluation** — `DONE`. [History](docs/archive/todo-0.1.0.md#l08).
- [x] **L09 — Nonzero-depth return-address support boundary** — `DONE`. [History](docs/archive/todo-0.1.0.md#l09).
- [x] **L10 — LLVM pr23135 source-definedness decision** — `DONE`. [History](docs/archive/todo-0.1.0.md#l10).
- [x] **L11 — Inventory additional atomic scope before extending it** — `DONE`. [History](docs/archive/todo-0.1.0.md#l11).
  - [ ] **L11a — General `_Atomic` objects and updates** — `OPEN`, P2.
    TinyCC's bounds-checking runtime and C11 atomic test configuration expose
    this scope; fixture CASE=0 currently refuses E2702. Define object operations,
    widths, orders, volatile/provenance and library/lock-free boundaries; implement
    shared paths and verify the actual runtime configuration, not just 11/11
    compiler objects. zlib's optional dynamic-table atomic_flag is also separate.
    Rule decided on 2026-10-13, under the owner's instruction to finish every
    item: `_Atomic` objects use the GCC layout on Linux and the Apple layout on
    Darwin; reads and stores are `seq_cst`; compound assignment, `++` and `--`
    are one `seq_cst` read-modify-write; floating compound assignment is refused
    in this slice.
    [Decision](docs/archive/notebook/rules-20261013/DECISIONS.md),
    [packet](docs/archive/notebook/rules-20261013/l11a-general-atomic-objects.md).
    October 20 (EvoIR 2.10.0): `_Atomic` objects use GCC's layout on Linux and
    Apple clang's on Darwin, and `_Atomic(T)` is parsed. A read of a 1-, 2-, 4-
    or 8-byte scalar is a seq_cst atomic load and a store is a seq_cst atomic
    store; `+=`, `++` and `--` are one seq_cst compare-exchange loop that keeps
    E3004, E3002 and E3005. The C-HIR interpreter now admits block-scope
    `_Atomic` declarations. E2955 (invalid atomic type or atomic bit-field) and
    E2956 (evaluated member access of an atomic record) are new. Floating
    updates, whole-record accesses and 16-byte objects stay E2702. Fixture
    CASE=0 runs in both interpreters and natively. Crossed thread tests pass in
    both directions against gcc-14 on Linux and Apple clang on Darwin, with the
    object defined on either side; Clang 19 on Linux uses Apple's layout rule, a
    recorded incompatibility. Linux `<stdatomic.h>` runs natively; the
    interpreters run it without its two fences (E3021). L11a stays open: the
    TinyCC `arm64-libtcc1-usegcc=yes` configuration is NOT_RUN because no pinned
    TinyCC checkout exists here; unchanged `136_atomic_gcc_style` matches its
    expect file at O0 and O2, and `124_atomic_counter` stopped at the
    `pthread.h` E2952 regression of D11; Darwin `<stdatomic.h>` needs
    `__c11_atomic_*`. [Tests](crates/evocc/tests/atomic_objects.rs). The lead's
    full-workspace gate of 2026-10-20 passes 465 suites, 3,520 tests, zero
    failures and 13 ignores, plus Clippy and formatting.
    October 21 (EvoIR 2.23.0, commit e00f9871): Darwin `<stdatomic.h>` compiles
    and runs through the 14 `__c11_atomic_*` builtins that Clang's header uses,
    on the existing EvoIR 3.8 forms, with the ten `__CLANG_ATOMIC_*_LOCK_FREE`
    macros. The stdatomic probe and every macro and generic function of the
    header agree with Apple clang natively and in both interpreters; crossed
    thread tests with Apple clang pass in both directions (142 checks per run).
    Linux keeps glibc's GCC path. L11a stays open only for the TinyCC
    `arm64-libtcc1-usegcc=yes` and zlib `DYNAMIC_CRC_TABLE` replays, which need
    pinned checkouts that this host does not have, and for floating compound
    updates (E2702). The full offline workspace gate passes 490 suites, 3,710
    tests, zero failures and 35 ignores, plus Clippy and formatting.
  - [x] **L11b — Additional `__atomic` RMW/CAS/exchange** — `DONE`. [History](docs/archive/todo-0.1.0.md#l11b).
  - [x] **L11c — Generic aggregate atomics** — `DONE`. [History](docs/archive/todo-0.1.0.md#l11c).

## Cross-cutting verification and tooling

- [x] **T01 — Reconcile the historical issue list with checked-in regressions** — `DONE`. [History](docs/archive/todo-0.1.0.md#t01).
  - [x] **T01a — Repair two independently reproduced inline E3101 paths** — `DONE`. [History](docs/archive/todo-0.1.0.md#t01a).
  - [x] **T01b — Select the typed call machine from reached signatures** — `DONE`. [History](docs/archive/todo-0.1.0.md#t01b).
- [ ] **T02 — Put expanded KUnit behind the common bench entry point** — `OPEN`, P1.
  The `kernel-kunit` adapter now wraps the existing boot/init workflow and
  pinned upstream parser, with separate 1,504 built-in / 31-module, 765-case
  populations. Boundary, duplicate-case, stale-payload and verdict REDs pass;
  historical raw-log replay reproduces both old full results exactly without
  writing a fresh receipt. Dry-run/status are read-only; `all` explicitly includes
  the lane. Live preflight rejects the outdated composition's changed
  `arch/arm64/crypto/sha3-ce.ko`, rather than booting mismatched artifacts.
  **Real K14 execution after T04 remains required** to close T02.
  [Adapter record](tests/fixtures/kernel-kunit-adapter.out),
  [current usage](book/src/evaluate.md#the-ladder).

- [x] **T03 — Spec/implementation/acceptance consistency audit** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03).
  - [x] **T03a — Synchronize retired diagnostic boundaries** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03a).
  - [x] **T03b — Remove the obsolete static-pointer emission refusal** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03b).
  - [x] **T03c — Rehash semantic proof-mutation candidates** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03c).
  - [x] **T03d — Synchronize dynamic-stack forms and frame clauses** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03d).
  - [x] **T03e — Close missing instruction sentinels and main-grammar errata** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03e).
  - [x] **T03f — Enforce return/prototype compatibility at every written site** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03f).
  - [x] **T03g — Save the callee-saved registers that `asm` clobbers** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03g).
  - [x] **T03h — Darwin calls across the compiler boundary** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03h).
  - [x] **T03i — State and test the Darwin target profile of EvoIR** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03i).
  - [x] **T03j — Fail closed for `_Atomic` reads and nonconstant range initializers** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03j).
  - [x] **T03k — Keep the division rule of a zero-size pointer difference** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03k).
  - [x] **T03l — Closed checker statuses and the certificate rule matrix** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03l).
  - [x] **T03m — Enforce the checks that the specs name** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03m).
  - [x] **T03n — Proof-required publication and admission** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03n).
  - [x] **T03o — Report EvoMIR refusals where §10 says** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03o).
  - [x] **T03p — Make the allocation verifier match §9.2** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03p).
  - [x] **T03q — Make EvoMIR validation a gate** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03q).
  - [x] **T03r — Refuse ABI-changing options on the cc path** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03r).
  - [x] **T03s — State and test the clean-check scope for pointer relations** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03s).
  - [x] **T03t — Front-end refusals that EvoIR §10.1 and §10.5 name** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03t).
  - [x] **T03u — Make the EvoIR validator match its rules** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03u).
  - [x] **T03v — Key persistent e-nodes by result sort** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03v).
  - [x] **T03w — EvoIR errata for §4.4, §6.4, §6.5, §7.2, §8, §10.2, §10.3 and §11** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03w).
  - [x] **T03x — EvoMIR and `target.md` errata** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03x).
  - [x] **T03y — `verification.md`, receipt binding and `spec/README.md`** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03y).
  - [x] **T03z — Missing negative-test matrices** — `DONE`. [History](docs/archive/todo-0.1.0.md#t03z).
- [ ] **T04 — Bind final kernel claims to the actual final composition** — `OPEN`, P1.
  The September 26 common bench uses release `e8bddc0a…` for the reviewed
  configuration; its first launch stopped on the inventory-role defect T04b.
  The corrected launch completed its 7002-object first pass, then was found
  interrupted with a stale RUNNING summary. The verified prepared-input resume
  completed (`20260926T133321Z`) but is **INCOMPLETE**: all 7,002 second-pass
  compilations and the prepare fixed point succeeded; modpost finds eight
  unresolved bitmap assertions (K12b), and late generators cause GCC to replace
  13 target objects (T04d). No fresh link/boot occurred; no worker remains live.
  Later K02d/K02e/K02f/K02g are independently tested but deliberately absent from that frozen
  executable; its final integration must not be inferred from this run.
  The replacement bench `20260927T135325.342339Z` **completed with exit 0**
  at 21:43 UTC (September 28 local). Frozen release `65035621…` substitutes
  **7,002/7,002 target-C objects**: first pass 6,966 fresh plus 36 verified cache
  rows; actual-generated-input pass 7,002 fresh, zero resumed. The generated
  content fixed point, 848 generated-C recompilations, complete link and all
  installed-object hash checks pass, with zero GCC replacement. All **847
  modules link/BTF PASS**; signing, installation and execution remain separate.
  Image `62c29ae4…` boots and powers off with the actual nonce on `hvf-host`.
  The run uses explicitly permitted QEMU drift, not the locked QEMU executable.
  Its compiler contains Rust 1.98.0 standard-library code and excludes later
  frontend/logical-immediate repairs and T05k's Rust 1.97.1 pin. Thus this closes
  the measured uniform composition, not final-current-compiler integration or
  K13/K14. No build worker remains live. Earlier interrupted launches and
  September 26 negative evidence stay recorded. [Exact receipt and boundaries][kernel].
  A separately identified current-pinned run, `20260927T233955Z`, started
  with compiler `ab2eeb7b…` and configuration `95853321…` (connector/process
  events and FPU module enabled). Its **7,008/7,008 first-pass objects** passed,
  with zero cache reuse. It was later found stopped with a 1,920-object
  second-pass checkpoint and stale RUNNING record. After fixing T04e, recovery
  `20260928T075419Z` **completed** at 11:07 UTC: all **7,008/7,008 prepared
  consumers** compile, with zero old prepared-cache reuse; the generated-input
  fixed point and **849** generated export/module C units pass. Complete link,
  **848 module links/BTF**, and installed-object audits pass, with zero GCC
  replacement. Image `18b36fee…` boots and shuts down with its nonce on
  `hvf-host`, with explicit QEMU drift. This is the pinned current-compiler
  composition, not a stale image. [Exact identities and scope](tests/fixtures/kernel-current-composition.out).
  Its matching runtime now installs/signs all 848 modules, rebuilds the 17
  reference test-support modules, and preserves all 4,396 original non-module
  userspace files. Actual FPU loading/execution and connector delivery pass
  their targeted gates. K13/K14 and the expanded-KUnit configuration still
  require their own current acceptance; installed does not mean every module
  was executed.
  Before final acceptance, freeze the
  selected compiler, actual generated inputs, every installed target object,
  Image and module/runtime hashes. Use a uniform rebuild for any single-compiler
  claim; otherwise state incremental provenance. Close with link/boot and
  replacement-object audits, without inheriting stale runtime results. [Evidence][kernel].

  - [x] **T04a — Reject obsolete generated-module survey records** — `DONE`. [History](docs/archive/todo-0.1.0.md#t04a).
  - [x] **T04b — Distinguish same-binary HOSTCC from target compilation** — `DONE`. [History](docs/archive/todo-0.1.0.md#t04b).
  - [x] **T04c — Recover verified prepared-input compilation batches** — `DONE`. [History](docs/archive/todo-0.1.0.md#t04c).
  - [x] **T04d — Include late host-generated inputs in preparation** — `DONE`. [History](docs/archive/todo-0.1.0.md#t04d).
  - [x] **T04e — Recover producer-replaced objects without crediting them** — `DONE`. [History](docs/archive/todo-0.1.0.md#t04e).
  - [x] **T04g — Rebuild the acceptance tooling that the October 7 deletion broke** — `DONE`. [History](docs/archive/todo-0.1.0.md#t04g).
- [x] **T05 — Final compiler-health and documentation gate** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05).
  - [x] **T05a — Preserve multiline benchmark command failures** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05a).
  - [x] **T05b — Run the workspace gate for manifest-only changes** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05b).
  - [x] **T05c — Reject partial or unlinked toybox acceptance** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05c).
  - [x] **T05d — Select Rust tests when their external fixtures change** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05d).
  - [x] **T05e — Show new kernel attempts beside historical acceptance** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05e).
  - [x] **T05f — Reject stale archives and incomplete musl runtime evidence** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05f).
  - [x] **T05g — Isolate the zlib benchmark from Python's standard codec** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05g).
  - [x] **T05h — Avoid host-stack overflow on long arithmetic chains** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05h).
  - [x] **T05i — Reject incomplete assembler-container execution** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05i).
  - [x] **T05j — Harden the other whole-application assembly verdict path** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05j).
  - [x] **T05k — Pin the actual host toolchain and revalidate current compiler code** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05k).
  - [x] **T05l — Compare paired regressions by canonical program identity** — `DONE`. [History](docs/archive/todo-0.1.0.md#t05l).

## Later roadmap gates — not substitutes for runtime closure

The September 29 revision-6 [Q2/Q3 implementation architecture][eqsat-plan] refines R01–R03
without closing them. R01a/R02a infrastructure and R02b development measurements
can proceed on correctness-admitted workloads. September 30's R02a/b/d
implementation and bounded Linux pilot are qualified; all three opportunity
ceilings and all A/B latency comparisons remain INCONCLUSIVE. Final acceptance remains
R01 → R02 → R03. `LATER` describes roadmap priority, not an implementation or
live-job status. Keep heavy campaigns serial under the existing ARM64 priorities.
Each policy/rule/shortcut track needs its own effect-ceiling preflight before
AI search. Conditional tasks below are explicit
design branches, not newly enabled semantics or required speculative projects;
record a justified not-needed disposition separately from implementation PASS.

**Paused on 2026-10-01** by the owner decision recorded in the
[codegen quality plan](docs/archive/codegen-quality-plan.md): the latency half of R02,
any second slice of R02e, and R03 with R03a–R03h. They keep their `LATER`
state and all retained evidence; nothing is relabeled. R01/G2 continues on the
kernel track, `g0.ub.v1` stays shipped as opt-in and `p0.scalar.v4` is frozen.
Reopening any of them needs a new owner decision after the N03 slices land.

- [ ] **R01 — G2 whole-kernel EqSat reproducibility and certificates** — `LATER`, P3.
  Produce the two required EqSat builds with equal Image hashes and independently
  check every optimized translation unit's certificate. Record exact optimization
  coverage; existing scalar proofs and unoptimized boot do not satisfy G2. [Gate][roadmap].

  - [x] **R01a — Policy-aware persistent optimization and proof provenance** — `DONE`. [History](docs/archive/todo-0.1.0.md#r01a).
- [ ] **R02 — Q2 object code-size evaluation** — `LATER`, P3.
  V1 primary objective is size under revision 7, using pinned programs and result
  checks. Compare GCC/Clang `-Os`/`-O2` without a parity threshold. Latency is
  NOT_MEASURABLE_ON_THIS_LANE; no timing rerun/retuning. Final G2 and whole-scope
  acceptance remain separate from the six-TU pilot. [Roadmap][roadmap].

  - [x] **R02a — Performance manifests and measurement runner** — `DONE`. [History](docs/archive/todo-0.1.0.md#r02a).
  - [x] **R02b — Opportunity/effect-ceiling preflight and A/A plus A/B pilot** — `DONE`. [History](docs/archive/todo-0.1.0.md#r02b).
  - [x] **R02b1 — Complete deterministic preflight and close review boundaries** — `DONE`. [History](docs/archive/todo-0.1.0.md#r02b1).
  - [x] **R02a1 — Calibrate and freeze a Linux steady-state timing protocol** — `DONE`. [History](docs/archive/todo-0.1.0.md#r02a1).
  - [x] **R02c — Frozen object-code size comparison** — `DONE`. [History](docs/archive/todo-0.1.0.md#r02c).
  - [x] **R02d — Implement and verify the stated native UB-check profiles** — `DONE`. [History](docs/archive/todo-0.1.0.md#r02d).
  - [x] **R02e — Guard-predicate folding domain, separately reviewed** — `DONE`. [History](docs/archive/todo-0.1.0.md#r02e).
- [ ] **R03 — Q3 measured self-improvement** — `LATER`, P3.
  **OWNER_DECIDED:** current V1 is a completed, frozen negative-admission study;
  this unchecked parent is the unachieved positive improvement milestone.
  All three tracks are formally INCONCLUSIVE/NOT_ADMITTED; do not start R03a/b/c/d
  or expand the census/auditors to force an affirmative result. See the
  [formal stop and decision record](docs/archive/notebook/eqsat-r02-review-repair-20260930.md#formal-three-track-adjudication-and-stop-decision).
  Freeze the baseline, candidate policy and evaluation protocol, then demonstrate
  validated improvement on the declared workloads without correctness regression.
  Close with measured results and independent checking; more rewrites or an
  e-graph implementation alone do not establish benefit. [Roadmap][roadmap].

  - [ ] **R03a — Bounded AI and non-AI policy search** — `LATER`, P3.
    Implement design P3 only after R02b finds a useful control space, or after
    R03e and a fresh preflight. Closed candidate schema, deterministic shared-chain
    replay and checker gating; search seeds are provenance, not optimizer inputs.
    Compare AI with enumeration/random/evolutionary search using explicit candidate
    evaluation units, cache/retry accounting and separate model/proof budgets.
    Use deterministic development fitness, not adaptive native timing; preserve
    corpus exclusions. A finite/no-effect study may end without R03 improvement.

  - [ ] **R03b — Derived rewrite shortcuts** — `LATER`, P3.
    Implement design P4 after R03f and the R02b shortcut-specific verdict, over
    the current scalar domain and guard forms. Compare enumerated compositions;
    native-size opportunity is not evidence of actual search/proof savings.
    Expand shortcut certificates into existing rules; test substitutions, missing
    before-witness constants, fact scope, poison and mixed-width boundaries.
    No invented premise or unrecorded extra transaction. Measure actual matcher
    work and expanded proof cost; shorter spelling is not automatic fuel savings.

  - [ ] **R03c — New-rule Lean admission and registry integration** — `LATER`, P3.
    Implement design P5 after R03f: AI supplies only typed data with relation `eq`;
    trusted code tries a frozen fixed-order template ladder for the exact universal
    `RuleCovered` statement. Candidates cannot select templates; retain actual
    attempts/success in admission identity. Run only bounded qualification probes
    until R02b's rule-specific preflight is resolved; campaign admission cannot
    inherit a successful policy preflight.
    Reject proof scripts, import/statement injection, sorry and unauthorized axioms;
    audit actual transitive dependencies, including native-backed analysis.
    Integrate a bounded manually reviewed registry batch across producer/checker/
    spec/tests. Finite-width template admission requires R03g; arbitrary scripts
    are V2, not this task's critical path. R03b completion is not a prerequisite.

  - [ ] **R03d — Held-out ablation and measured promotion** — `LATER`, P3.
    After final R02 and the relevant candidate paths, freeze rules/policies before
    held-out A–E/non-AI evaluation. Match per-track grammar/limits and separate
    certificate quota, transient search and matching work. Report correctness, gain/regressions, uncertainty
    and discovery/compilation cost; no tuning on final feedback. Promote only under
    predeclared objectives/bounds. A negative study is not R03 improvement closure.

  - [ ] **R03e — Cost-aware extraction if the initial policy space is degenerate** — `LATER`, P3.
    **OWNER_DECISION_PENDING:** recommend deferral. Selected-pair limitations
    are observed, but a qualified useful alternative-selection witness is not.
    This would be a separately scoped optimizer change, not an extension of
    the frozen negative preflight or its audit tools.
    First make multiple legal e-class representatives selectable by actual cost,
    with deterministic ties, bounded cycle handling and existing certificate
    replay. REDs cover insertion-order bias, overflow, shared nodes and budgets.
    If justified, separately version rule masks/scheduling controls; no ignored
    fields or automatic checker expansion. Repeat R02b before any new AI campaign.

  - [ ] **R03f — Rule integration and independent matching gate** — `LATER`, P3.
    Before R03b/R03c, freeze a small manual integration budget and checklist for
    spec/digest, Lean, producer, checker and mutation tests; measure human/proof
    cost. V1 is not automatic high-throughput admission. A generic registry-AST
    matcher/builder is a separately reviewed A3/A4 change, with independent
    implementations and wrong-binding/type/guard/constant tests before enabling.

  - [ ] **R03g — Bounded-width proof interface if needed** — `LATER`, P3.
    Current `RuleCovered` quantifies all Nat width environments; JSON metadata
    cannot narrow it. If finite-width candidates are needed, first review/version
    an actual instance predicate, theorem/audit interface and producer/checker
    coverage map. Test zero, >64 and independently wide shift counts; review
    fixed-template reflection dependencies. Universal-template V1 can proceed
    without this extension; 64 individual proofs are not universal coverage.

  - [ ] **R03h — Optional unverified utility screening before proof** — `LATER`, P3.
    A shadow graph may estimate matches/extraction utility for development inputs;
    separate its types, caches and artifacts as `UNVERIFIED_DEV`. No proof,
    installable rule, executable or certified-state mutation can leave this path.
    Test every installation/codegen boundary against shadow artifacts. Prove and
    integrate survivors before actual verified metrics; screening is not a gain.

- [ ] **R04 — x86-64 userspace backend/ABI milestone** — `OPEN`, P1.
  Implement the gated SysV/backend slice after ARM64 acceptance priorities,
  retaining shared IR semantics. Close with native userspace ABI/corpus evidence.
  Full x86 kernel support remains a separately declared stretch, not an implied
  consequence of ARM64 boot. [Roadmap][roadmap].
  The 2026-10-13 [design](docs/design/x86-64-linux.md)
  makes x86-64 a third target of the shared EvoMIR instruction set, so the
  lowering, the EvoMIR interpreter, SSA destruction, the allocator, the verifier
  and the frame layout stay shared. Each EvoMIR form emits a fixed AT&T
  sequence; a per-form clobber table carries the fixed registers of shifts,
  division and string moves. The proof-required path refuses x86-64.
  `linux/amd64` containers run on this host through Rosetta and QEMU; bit-exact
  floating-point tests must use QEMU, because Rosetta differs from x86 for some
  NaN results. Every milestone keeps the AArch64 output byte-identical.

  - [ ] **R04a — Prepare the shared code for a third target** — `OPEN`, P1.
    Target predicates, ordered argument and result register lists, the generator
    and the layout plumbing, about 830 lines and 300 test lines. Exit: the
    AArch64 output of the workspace tests and the ladder programs is
    byte-identical.

  - [ ] **R04b — Compile integer-only functions and calls** — `LATER`, P1.
    About 3,400 lines and 1,800 test lines. Exit: integer unit tests per EvoMIR
    form pass under QEMU in the amd64 container, and integer ABI calls agree
    with GCC 14 in both directions.

  - [ ] **R04c — Memory, records, globals and the full SysV convention** — `LATER`, P1.
    About 2,300 lines and 1,400 test lines, including the SysV record classes in
    the `byval` mark. Exit: every argument class crosses to and from GCC 14 with
    the GCC value.

  - [ ] **R04d — SSE2 floating point and variadic calls** — `LATER`, P1.
    About 2,100 lines and 1,200 test lines, with the register save area and
    `%al`. Exit: bit-exact floating-point tests under QEMU, and variadic calls
    in both directions with GCC 14.

  - [ ] **R04e — Run the corpus on x86-64** — `LATER`, P1.
    About 950 lines, 400 test lines and room for defects, with the small `rdtsc`
    inline-assembly subset that SQLite's test build needs. Exit: c-testsuite,
    cJSON, zlib, Lua and SQLite pass their own tests with `CC=evocc` in the
    amd64 container.

- [ ] **R05 — Release, reproducible bootstrap and proof-claim audit** — `OPEN`, P1.
  Audit the remaining release deliverables, portable builds, packaged artifacts,
  reproducibility and proof boundaries against the roadmap. Give each missing
  deliverable a child task before release. A Rust compiler compiling a C compiler
  is not literal self-hosting; tested scalar certificates are not a proof of the
  entire compiler. The October 4 V00–V06 correctness path is a mandatory first-
  public-release dependency, not optional follow-up. Close with the actual release
  checklist and artifacts only after V06. [Roadmap][roadmap].
  On 2026-10-13 the owner asked to finish every open item, so this task is
  active. The 2026-10-13 audit is in `docs/release-checklist.md`. The archives
  now carry the license texts of the bundled Linux headers, and the sysroot
  contract pins them. The children R05a to R05l hold the missing deliverables.
  October 27 claims refresh: the README, the book, the paper, the evidence
  document, the gallery and `CHANGELOG.md` cite the receipts of the frozen
  acceptance of `08bf09bd`. With that compiler the code is 1.8 to 2.3 times
  the size of GCC 14 `-O2` code on five library files
  ([default receipt](tests/contracts/application-size-default-20261027-receipt.json),
  [EqSat receipt](tests/contracts/application-size-eqsat-20261027-receipt.json)).
  The gallery captures and log cards come from the same runs. A number that no
  run measured again keeps its date. The three games (2026-09-29) were not built
  again, because their sources and data are not retained offline.

  - [x] **R05a — Build and check the Linux archive** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05a).
  - [x] **R05b — Ship the license texts of the runtime code in the binaries** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05b).
  - [x] **R05c — Pin the sysroot image** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05c).
  - [x] **R05d — Trim the bundled headers or record why they stay** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05d).
  - [x] **R05e — Build path-independent binaries** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05e).
  - [x] **R05f — Document how to reproduce a release archive** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05f).
  - [x] **R05g — Correct the install text** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05g).
  - [x] **R05h — State the glibc floor of the Linux archive** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05h).
  - [x] **R05i — State how macOS treats a downloaded archive** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05i).
  - [x] **R05j — Print versions from every binary** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05j).
  - [ ] **R05k — Date, tag and publish 0.1.0** — `OPEN`, P1.
    The owner tags and pushes. Exit: CHANGELOG dates 0.1.0, tag `v0.1.0`
    exists, one merged `SHA256SUMS` is published, and the README download
    URLs resolve.
    On 2026-10-28 the owner set the release date: the dating commit dates
    0.1.0 as 2026-10-28. The archives are built again from it; the tag and
    the publication remain.
  - [x] **R05l — Name the proof profile in the README** — `DONE`. [History](docs/archive/todo-0.1.0.md#r05l).
  - [ ] **R05m — Make the hosted CI and release workflows pass** — `LATER`, P1.
    Paused by the owner on 2026-10-20. No run of `ci.yml`, `nightly.yml` or
    `release.yml` has succeeded; since 2026-10-07 GitHub refuses every job
    because of the account's billing state. Known causes after billing: the Rust
    job runs the tests that need `lean/.lake/build/bin/evoir-sem` without
    building it (six test files, 25 tests in `proof_required.rs` alone), and the
    older SDK of the macOS runner can expose the `__builtin___*_chk` failures of
    the macOS 15 SDK. The relative-link check passes on a clean checkout. The
    release workflow builds the Linux archive (R05a) and merges the checksum
    files (R05k). Exit: one green run of each workflow, and of `release.yml` for
    a test tag.
    On 2026-10-27 a clean Lean build, as CI and `tools/release.sh --with-lean`
    make it, failed twice on a busy host: `bv_decide` timed out at the default
    10 s in `Proofs/Transfer.lean`, and that one module peaked at 17 GiB, more
    than the 7 GB of a macOS runner. `02d84269` gives each `bv_decide` 60 s.
    The bv_decide rows now live in `TransferCarry`, `TransferMul` (with
    `Elab.async false`) and `TransferCompare`, which only the umbrella `EvoIR`
    imports: a clean `lake build` peaks at 8.3 GiB in 164 s, and a clean
    `lake build evoir-sem` at 3.9 GiB with no `bv_decide` module.

- [x] **R06 — Pinned Doom integration** — `DONE`. [History](docs/archive/todo-0.1.0.md#r06).

## First public release verification

- [x] **V00 — Freeze the core compilation theorem and real pilot** — `DONE`. [History](docs/archive/todo-0.1.0.md#v00).
- [x] **V01 — Prove the source-to-EvoIR result checker** — `DONE`. [History](docs/archive/todo-0.1.0.md#v01).
- [x] **V02 — Prove selection and out-of-SSA result checking** — `DONE`. [History](docs/archive/todo-0.1.0.md#v02).
- [x] **V03 — Prove two-input allocation validation including spills** — `DONE`. [History](docs/archive/todo-0.1.0.md#v03).
- [x] **V04 — Close frame/emission verification and compose the baseline** — `DONE`. [History](docs/archive/todo-0.1.0.md#v04).
- [x] **V05 — Compose existing verified rewrites with the proved pipeline** — `DONE`. [History](docs/archive/todo-0.1.0.md#v05).
- [x] **V06 — Enforce proof-required compilation and final release acceptance** — `DONE`. [History](docs/archive/todo-0.1.0.md#v06).
- [x] **V07 — Remove the zero-extension premise from the ABI entry assumption** — `DONE`. [History](docs/archive/todo-0.1.0.md#v07).
- [x] **V08 — Run the selected native gates on proof-required output** — `DONE`. [History](docs/archive/todo-0.1.0.md#v08).

### Defects found by the specification review and proof replay

The specification editors found D01-D05 on 2026-10-06, when they
stabilized the pinned documents in `spec/`. Each one is a difference between
the code and a pinned document. A fix changes the code, or it changes the
document with a new revision. None of D01-D05 changes the claim of
`spec/verification.md`: that claim does not depend on the Rust certificate
parser, the EvoMIR text parser or the Darwin ABI profile, and floating
builtins are outside `core-v1`. D06 is the separate October 7 proof-replay
refusal; changing the P13 demo does not close it.

- [x] **D01 — Give builtin-argument-type its own diagnostic code** — `DONE`. [History](docs/archive/todo-0.1.0.md#d01).
- [x] **D02 — Refuse the EVP epoch marker 0** — `DONE`. [History](docs/archive/todo-0.1.0.md#d02).
- [x] **D03 — Read every EvoMIR text form that the writer prints** — `DONE`. [History](docs/archive/todo-0.1.0.md#d03).
- [x] **D04 — Darwin profile: 128-bit argument pairs** — `DONE`. [History](docs/archive/todo-0.1.0.md#d04).
- [x] **D05 — Darwin profile: variadic rules** — `DONE`. [History](docs/archive/todo-0.1.0.md#d05).
- [x] **D06 — Lowering check refuses the guarded division shape** — `DONE`. [History](docs/archive/todo-0.1.0.md#d06).
- [x] **D07 — Consistent command defaults and repair output** — `DONE`. [History](docs/archive/todo-0.1.0.md#d07).
- [x] **D08 — Compile fortified C library calls** — `DONE`. [History](docs/archive/todo-0.1.0.md#d08).
- [x] **D09 — Check the remaining fortified builtins** — `DONE`. [History](docs/archive/todo-0.1.0.md#d09).
- [x] **D10 — Check `-Oeq-scalar` rewrites of Darwin EvoIR text** — `DONE`. [History](docs/archive/todo-0.1.0.md#d10).
- [x] **D11 — `naked` functions get a frame record** — `DONE`. [History](docs/archive/todo-0.1.0.md#d11).
- [x] **D12 — Caller-constant specialization breaks a proof-required entry** — `DONE`. [History](docs/archive/todo-0.1.0.md#d12).
- [x] **D13 — Use the target variadic-list size in both interpreters** — `DONE`. [History](docs/archive/todo-0.1.0.md#d13).
- [x] **D14 — Do not fill an unused argument bank for Darwin stack words** — `DONE`. [History](docs/archive/todo-0.1.0.md#d14).
- [x] **D15 — Accept `returns_twice` prototypes from `<pthread.h>`** — `DONE`. [History](docs/archive/todo-0.1.0.md#d15).
- [x] **D16 — Calls that return twice by name** — `DONE`. [History](docs/archive/todo-0.1.0.md#d16).
- [x] **D17 — Accept vector-PCS prototypes so `<math.h>` compiles on Linux** — `DONE`. [History](docs/archive/todo-0.1.0.md#d17).
- [x] **D18 — Implement `__has_include_next` so Darwin `<limits.h>` reaches the SDK** — `DONE`. [History](docs/archive/todo-0.1.0.md#d18).
- [x] **D19 — Close the Darwin SDK header gaps of the header sweep** — `DONE`. [History](docs/archive/todo-0.1.0.md#d19).
- [x] **D20 — Implement `__builtin_tgmath` for glibc `<tgmath.h>`** — `DONE`. [History](docs/archive/todo-0.1.0.md#d20).
- [x] **D21 — Implement `__builtin_fpclassify` for glibc `fpclassify`** — `DONE`. [History](docs/archive/todo-0.1.0.md#d21).
- [x] **D22 — Make `wchar_t` an `int` on Darwin** — `DONE`. [History](docs/archive/todo-0.1.0.md#d22).
- [x] **D23 — A bit-field store is not a read of its neighbors** — `DONE`. [History](docs/archive/todo-0.1.0.md#d23).
- [x] **D24 — Give wide character constants the signedness of `wchar_t` in `#if`** — `DONE`. [History](docs/archive/todo-0.1.0.md#d24).
- [x] **D25 — Finish per-bit initializedness for volatile bit-fields and EvoMIR** — `DONE`. [History](docs/archive/todo-0.1.0.md#d25).
- [x] **D26 — Run aggregate statement expressions in the EvoIR interpreter** — `DONE`. [History](docs/archive/todo-0.1.0.md#d26).
- [x] **D27 — Close the small complex gaps that L04 left** — `DONE`. [History](docs/archive/todo-0.1.0.md#d27).
- [x] **D28 — Close the vector gaps that L05 found** — `DONE`. [History](docs/archive/todo-0.1.0.md#d28).
- [x] **D29 — Run by-value records in general registers in the EvoMIR interpreter** — `DONE`. [History](docs/archive/todo-0.1.0.md#d29).
- [x] **D30 — Evaluate `defined` from macro expansion in `#if`, and UTF-8 character constants** — `DONE`. [History](docs/archive/todo-0.1.0.md#d30).
- [x] **D31 — Keep record chunk accesses aligned under `-mstrict-align`** — `DONE`. [History](docs/archive/todo-0.1.0.md#d31).
- [x] **D32 — Give the interpreter refusals a code of their own** — `DONE`. [History](docs/archive/todo-0.1.0.md#d32).
- [ ] **D33 — Judge access alignment by concrete addresses** — `OPEN`, P2.
  Owner decision of 2026-10-24, from the D28 diagnosis. The EvoIR model (5.4,
  step 5) and `accessObject` in `lean/EvoIR/Core/IRExecution.lean` judge
  alignment by the object's declared alignment and the offset, so `20060420-1.c`
  and `pr92618.c` report E3012 although their native accesses are aligned. Next
  action: give every object a deterministic concrete address in the Rust
  interpreters and in the Lean model, and test the absolute address against the
  required alignment. Exit: both originals run to exit 0 in both interpreters on
  both targets; a misaligned absolute address still reports E3012; the spec, the
  Rust/Lean model differential and the Lean gate agree.
  October 25 design, no code change. The Rust interpreters already give each
  object a deterministic concrete address (a hash of site, instance and thread)
  with an 8-byte floor, but the access check tests the declared alignment and
  the relative offset. Lean Core has no object base. The design raises the floor
  to 16, keeps the allocator, and adds a base to the Lean objects, with the
  private-cell invariants and the machine/source relations. The Rust/Lean
  differential has six scalar groups and no memory group. Four goals, in order:
  (1) the address function, floor 16 and RED cases (the two originals and a real
  offset-1 fault); (2) the Lean fields and proofs; (3) a memory group in the
  differential, with a mutant that it must reject; (4) the rule switch in Rust
  and Lean in one commit, with the spec, the changed expectations, the originals
  in both interpreters on both targets, and the full and Lean gates. One memory
  unit test pins the old rule, and a MIR record-transport test changes from
  E3012 to E3001 (uninitialized padding). Evidence:
  `.evidence/d33a-design-20261025/`.

## Codegen quality track — next quarter

Authority: [codegen quality plan](docs/archive/codegen-quality-plan.md) and the
[internal target description contract](spec/target.md). The frozen S5
comparison puts unchecked EvoCC object text at 2.04–2.43× GCC-14 `-O2` by
library family (not per TU; N01 records 1.986–2.668× for the five library TUs),
while verified rewrites and the certified guard slice move
about 1–2.5%. This track closes that gap with standard passes under
translation validation and builds the tooling around the compiler. Size under
the S1 contract is the objective; no latency, parity or verified-optimization
claim follows from these items. `LATER` is the tracker's roadmap state, not a
statement that the work is idle. Follow the plan's working agreement: frozen
plan, RED, flag, gates, one-page note, TODO update in the same commit.

- [x] **N01 — Size-gap attribution census** — `DONE`. [History](docs/archive/todo-0.1.0.md#n01).
- [x] **N02 — Pass framework, translation validation and pass telemetry** — `DONE`. [History](docs/archive/todo-0.1.0.md#n02).
- [x] **N03 — Codegen improvements, bounded batch** — `DONE`. [History](docs/archive/todo-0.1.0.md#n03).
  - [x] **N03a — Backend layering, behavior-preserving** — `DONE`. [History](docs/archive/todo-0.1.0.md#n03a).
  - [x] **N03b — TU-local inlining, bounded first slice** — `DONE`. [History](docs/archive/todo-0.1.0.md#n03b).
  - [x] **N03c — Register allocation improvements, bounded slices** — `DONE`. [History](docs/archive/todo-0.1.0.md#n03c).
  - [x] **N03d — Instruction selection, bounded C1–C5 sequence** — `DONE`. [History](docs/archive/todo-0.1.0.md#n03d).
  - [x] **N03e — EvoIR mid-level passes, bounded first slice** — `DONE`. [History](docs/archive/todo-0.1.0.md#n03e).
  - [x] **N03f — Frame and call overhead, opt-in slice** — `DONE`. [History](docs/archive/todo-0.1.0.md#n03f).
- [x] **N04 — The C checker as a product** — `DONE`. [History](docs/archive/todo-0.1.0.md#n04).
- [x] **N05 — Measurement automation and bisect** — `DONE`. [History](docs/archive/todo-0.1.0.md#n05).
- [ ] **N06 — EqSat memory and effect domain** — `LATER`, P3.
  Not scheduled. The place where equality saturation has real value (CSE,
  dead code, alias-aware forwarding) needs a state and effect model and its
  own proof design. Reopen only after the N03 slices land, with a separate
  design and owner decision.
  The 2026-10-13
  [design](docs/archive/notebook/n06-eqsat-memory-design-20261013.md) makes a
  memory snapshot an e-graph value of sort `mem<all>`, with Kleene equality over
  partial values. A memory transaction only deletes instructions and renames
  their results; token chains stay linear, and an instruction is deleted only
  when its class holds a defined, available representative, so no trap or
  diagnostic disappears. Alias facts stay local (one SSA root with an exact
  offset, distinct globals, fresh unexposed allocas); calls that write, `asm`,
  volatile and atomic accesses and fences are barriers. A new Lean byte-level
  model `EvoIR.Mem` proves each memory rule, the Rust checker recomputes every
  fact, and the proof-required path refuses `-Oeq-mem`. The lead's provisional
  answers to the design's open decisions: keep strict trap preservation, keep
  volatile accesses as full barriers, add the definition seeds as a new policy
  version without changing the frozen `p0.scalar.v4`, keep `max_transactions` at
  8 until a measurement justifies more, and leave forwarding through `copy` to
  N10. T03v is a precondition.

  - [ ] **N06a — Definition seeds for loads, divisions and UB predicates** — `LATER`, P3.
    About 450 Rust, 650 test, 300 Lean and 150 spec lines, with a stop rule
    against the S4 ceiling. Exit: the new seeds pass the randomized gate and the
    certificate replay, and the frozen policy is unchanged.

  - [ ] **N06b — Merge straight-line block chains with a certified step** — `LATER`, P3.
    About 500 Rust, 450 test, 450 Lean and 120 spec lines (`c0.chain.v1`). Exit:
    a chain-merge refinement theorem over `Core.IR` and checked replays of the
    merged chains.

  - [ ] **N06c — Memory-aware equality saturation** — `LATER`, P3.
    A diagnostic preflight with a stop rule, then `m0.mem.v1` with `evp3` and
    the Lean memory model: about 4,650 Rust, 2,600 test, 2,700 Lean and 650 spec
    lines. Exit: every memory rule has its `MRuleCovered` theorem, differential
    vectors agree with `evocc-memory`, and the randomized gate also compares
    diagnostics and volatile traces.

  - [ ] **N06d — Dead-store elimination by a checked scan** — `LATER`, P3.
    About 800 Rust, 600 test, 700 Lean and 150 spec lines (`m1.dead.v1`). Exit:
    the dead-store insensitivity lemma and negative tests for each barrier.

  - [ ] **N06e — Measure the benefit by counts** — `LATER`, P3.
    Exit: EvoIR counts, interpreter steps, S1 bytes, the reductions' instruction
    counts and a K02 rerun show the change; no timing claim is made.

- [x] **N07 — Target description and ISA-model oracles** — `DONE`. [History](docs/archive/todo-0.1.0.md#n07).
- [ ] **N08 — Keep values in registers across block joins** — `OPEN`, P1.
  Found on 2026-10-13 by the kernel TCG instruction profile. At each if/else,
  switch, `asm goto` or loop back edge, EvoCC passes every live value as a
  block argument and moves it through stack slots, sometimes into the same
  slot. On the K02 workload, such copy runs are 29.6 percent of EvoCC's
  executed kernel instructions and 1.6 percent of GCC's: at least 36 percent
  of the excess. `tests/fixtures/kernel-codegen-reductions/loop-carried-transport.c`
  runs 179,135 instructions per call (GCC 14: 8,236), and
  `asm-goto-live-transport.c` runs 488 (GCC 14: 67). Next action: coalesce
  block arguments with their sources in out-of-SSA, share the spill slots of
  values that do not interfere, and keep loop-invariant values in
  callee-saved registers, with the allocation validation unchanged. Exit:
  both reductions run within twice GCC 14's count at -O2; the workspace gate,
  the allocation verifier and the proof-required path pass; the K02 profile
  is rerun.
  October 14: the
  [analysis](docs/archive/notebook/perf-n08-n11-analysis-20261014.md) measured
  the causes. One `asm.goto` turns identical-value transport off for its whole
  function (`crates/evocc-mir/src/generic/ssa.rs:212-219`,
  `isa/aarch64/insts.rs:715-717`), and every K02 hot function has `asm goto`
  patch sites. Without it the loop runs 28 instead of 175 instructions per
  iteration. Read-write and tied `asm` outputs do the same; spill temporaries
  ignore their copy partner; slot compaction leaves same-slot pairs. New next
  action, replacing the one above: first add an independent SSA-destruction
  check, then admit `asm.goto` without fixed bindings and read-write or tied
  outputs, then copy hints and live ranges with holes in the allocator, not in
  SSA destruction, because the Lean transport check needs equal definitions. The
  2x exit for `loop-carried-transport.c` also needs Boolean threading and guard
  hoisting, which need their own checker.
  October 16: the independent SSA/post-SSA transport checker now rejects
  altered operands, opcodes, returns, effects, edge transport and asm outputs.
  Assembly-goto without fixed bindings permits identical-value forwarding;
  fixed bindings and computed branches remain excluded. On unchanged sources,
  the loop falls from 179,130 to 29,734 instructions per call (GCC 8,236),
  and `fault` from 488 to 171 (GCC 67). Counts exclude callees and use
  disjoint executable addresses; the earlier overlapping-init count is not
  used. The constant-address follow-up reduces `fault` further to 154.
  Original `mm/page_alloc.c` and `arch/arm64/kvm/mmu.c` compile and assemble;
  their objects were not installed or booted. The combined full-workspace
  gate passes 3,423 tests, zero failures and eight ignores, plus Clippy and
  formatting; all six Lean steps pass. The 2x exit and K02 replay remain open.
  [Transport record](tests/fixtures/kernel-ssa-transport.out).
  The live-segment follow-up reduces the loop further to 28,710 and `fault`
  to 153 instructions per call. All 15 container and 15 TCG comparisons pass
  with matching checksums and no dropped records. Interference uses per-block
  segments; copy-name unions retain both incoming definitions. A parser tag
  index also removes the observed quadratic lookup in the original KVM unit.
  The combined gate passes 3,436 tests, zero failures and eight ignores, plus
  Clippy, formatting and all six Lean steps. The 2x exit and K02 replay are
  still not met. [Live-segment record](tests/fixtures/kernel-live-segments.out).
  October 19 integration now supplies the current kernel profile: long transport
  runs occupy 6.09% of executed kernel instructions (GCC 1.96%) in demand paging.
  The original K02 workload still times out on EvoCC, so N08 remains open.
  October 24 recount of the five reductions of
  `tests/fixtures/kernel-codegen-reductions/` with the method of October 13
  (tbprof, one vCPU, the GCC Image) and the compiler of 0d6aeda1, instructions
  per call, EvoCC / GCC 14: `loop-carried-transport` 26,663 / 8,236 (3.24; was
  179,135), `asm-goto-live-transport` 147 / 67 (2.19; was 488),
  `leaf-helper-call` 32 / 9 (3.56; was 85; the helper is now inlined),
  `struct-value-through-stack` 46 / 23 (2.00) and `local-array-constant-index`
  171 / 94 (1.82). The two transport reductions still miss twice GCC 14: EvoCC
  saves the callee-saved registers with single `str`/`ldr` (GCC pairs them),
  moves values through stack slots at joins, and keeps many unconditional join
  branches.
  Owner decision of 2026-10-24: the Lean transport check (`checkRetained` in
  `MachineTransportInstruction.lean`) may change to admit coalesced block
  arguments, and allocator and frame improvements may become the default. A goal
  that changes object hashes or proof-required outputs re-baselines them in the
  same commit and lists them.
  October 25 measurement of the nine opt-in code-generation variants with the
  compiler of 91348ade. Under the kernel flags every variant, and all of them
  together, give byte-identical reduction objects and the same `memory.c`
  `.text` (234,248 bytes): the variants do not change the K02 reductions under
  the kernel contract. With `-fomit-frame-pointer -mno-general-regs-only` added,
  a diagnostic and not the kernel contract, all variants together give 25,626 /
  134 / 15 / 41 / 136 instructions per call for loop / asm-goto / helper /
  struct / array (baseline 26,660 / 147 / 17 / 43 / 141; GCC 14 8,236 / 67 / 9 /
  23 / 94). Plain `-O2` `memory.c` `.text` is 220,044 bytes at baseline and
  200,676 with all variants (-8.8 percent); the largest single savings are
  shared epilogues (-7,408), constants (-4,624) and paired saves (-4,572). Every
  checksum pair matches GCC 14. Each variant and the combination refuse
  proof-required admission (30 refusals, no output), so a default needs that
  admission first. Order by gain and checker coverage: copy hints, paired saves,
  constants, local, shared epilogues, controls, leaf frames, extensions, reload
  reuse. Evidence: `.evidence/n08b-variants-20261025/`.

- [ ] **N09 — Inline small static helpers by default** — `OPEN`, P1.
  The K02 fault path makes about 204 calls to small helpers that GCC
  inlines; each call pays a frame and a return-signing pair.
  `tests/fixtures/kernel-codegen-reductions/leaf-helper-call.c` runs 85
  instructions per call (GCC 14: 9). N03b's bounded TU-local slice was
  negative, and the kernel flags turn the opt-in quality flags off
  (`protects_codegen()`). Next action: inline small `static` and `static
  inline` functions within a size budget, by default and for the kernel
  flags, with the existing translation validation. Exit: the reduction runs
  within twice GCC 14's count; the ladder programs keep their results, and
  their total code size does not grow.
  October 14: the
  [analysis](docs/archive/notebook/perf-n08-n11-analysis-20261014.md) found that
  the inliner refuses a plain `static` helper with a local object
  (`crates/evocc-chir/src/inline.rs:989-1044`); inlined, the reduction runs 49
  instead of 85. Next action: admit such helpers to the size-bounded class with
  a per-unit growth cap; the exit also needs the N10 offset fold and constant
  shift guards.
  October 16: small private leaves with fixed scalar locals now use the
  existing substitution, with a unit-wide 1,024 source-node budget and retained
  callable definitions on budget exhaustion. Storage, attribute, escape and
  activation barriers remain. The unchanged reduction falls from 73 to 37
  instructions per call including its helper (GCC 9); function text falls
  from 268 to 172 bytes. Three container and three TCG runs agree. Three
  original kernel units compile and assemble, but are not installed or booted.
  The combined 3,436-test workspace gate and six Lean steps pass. N09 stays
  open: the 2x exit and all-ladder result/size parity remain unmet.
  [Helper record](tests/fixtures/kernel-private-helper.out).
  October 19: the unchanged helper now takes 32 instructions per call and
  152 function bytes, versus GCC's retained nine-instruction result. Three
  container and three TCG executions agree; final compiler readback is
  byte-identical. Five constant-count shift checks still execute 15
  instructions. Their removal is not admitted by ordinary machine selection:
  EvoMIR Section 5.2 requires a direct canonical true operand before it omits
  a trap. Keep those obligations until the proof path establishes them. The
  18-instruction exit and all-ladder result/size parity remain open.
  [Current reduction](tests/fixtures/kernel-zero-branch.out).
  October 24 recount of the five reductions of
  `tests/fixtures/kernel-codegen-reductions/` with the method of October 13
  (tbprof, one vCPU, the GCC Image) and the compiler of 0d6aeda1, instructions
  per call, EvoCC / GCC 14: `loop-carried-transport` 26,663 / 8,236 (3.24; was
  179,135), `asm-goto-live-transport` 147 / 67 (2.19; was 488),
  `leaf-helper-call` 32 / 9 (3.56; was 85; the helper is now inlined),
  `struct-value-through-stack` 46 / 23 (2.00) and `local-array-constant-index`
  171 / 94 (1.82). `leaf-helper-call` no longer pays a call; its excess is a
  shift-count guard (`movz`, `cmp #64`, `b.hs` to `brk #3005`) kept before each
  constant shift after inlining, and `lsr`/`and` where GCC uses `ubfx`.
  October 24 (EvoIR 2.31.3): the optimized lowering gives `shift_count_defined`
  the canonical true result when the amount is a constant in `[0, N)` in the
  same block, so checked MIR omits the count guard; a constant amount outside
  the range and a nonconstant amount keep it. `leaf-helper-call` falls from 32
  to 17 instructions per call (GCC 14: 9), and the K16 `mm/memory.c` keeps 20 of
  its 90 `brk #3005` guards, none with a constant count. 120 constant shifts of
  every integer type give GCC 14's result at O0 and O2.
  Owner decision of 2026-10-24: run the application ladder to check the rest of
  the exit (the ladder programs keep their results, and their total code size
  does not grow).
  October 25: all 15 application lanes and the separate Doom lane were replayed
  with compiler SHA-256 `978a3771912d7b2eafe89228ff2cb3d3c0c996826b2b2875c4cb74932a655ce4`.
  The original jq timeout is repaired: 70/70 units and all 30 upstream test
  programs pass with the unchanged 300-second compiler limit. zlib, zstd,
  cJSON, TinyCC, PolyBench, Lua, SQLite and Doom retain complete coverage.
  Toybox's 1,669 results match GCC, including one FAIL and 20 SKIP. Musl
  retains all 1,339 objects through two libc-test runs, with 13 shared
  reference failures and no added failure or replaced object.
  N09 remains OPEN. Old passing cases are lost in ctest (two), torture (five),
  Csmith (16) and LLVM (12, including an unusable reference). Mbed TLS retains
  91/382 units: the other units refuse under unchanged `-Werror` flags.
  Csmith's qualified-return reduction exposes an inliner result-cell defect;
  the first Mbed TLS refusal comes from unused parameters in the exported
  GCC NEON header. These measurement results are not repairs or passes.
  The frozen six-TU S1 captures are reproducible and smaller than the retained
  default baseline: checked A/B totals 213,808/213,740 bytes versus
  246,988/244,344; unchecked 208,068/207,984 versus 241,012/238,344.
  Every measured TU shrinks, but these six TUs do not establish whole-ladder
  size non-growth. [Default receipt](tests/contracts/application-size-default-20261025-receipt.json),
  [EqSat receipt](tests/contracts/application-size-eqsat-20261025-receipt.json).
  October 26: the two measured production regressions are repaired, not merely
  reclassified. Inlined result cells remove only their outer qualifiers;
  pointee and member qualifiers remain. A single final nonmodifiable record
  return uses the existing statement-expression value copy. Csmith run
  `20261025T162452Z` restores all 16 lost cases: 932 checksum agreements,
  the same 68 oracle-unusable cases, zero refusals and zero mismatches. All
  916 previously agreeing cases keep their checksum. The 1,000 sources and
  both runtime-header sets are byte-identical to the preceding run.
  System-header warning provenance now follows the include search, and the
  ladder preserves implicit system paths as `-isystem`. Compiler-created VLA
  extent captures count as used through their array type; real unused source
  objects still warn. Mbed TLS run `20261025T171209Z` retains all 382/382 EvoCC
  objects, with no refusal, exclusion, unmeasured unit or make replacement.
  Its original selftest and 148/148 CTest pass under unchanged `-Werror`.
  The final production sources (`e7a3408c`) pass 515 workspace suites:
  3,875 PASS, zero FAIL, 51 existing ignores, Clippy and formatting; all six
  Lean steps pass and the Lean receipt is unchanged. The return-value probes
  also pass 60 native reference/candidate executions on Darwin and Linux.
  This does not close N09: the other historical source-profile refusals and
  unusable reference remain recorded, and whole-ladder size non-growth has
  not been measured. Six-TU sizes are not that exit. The completed Csmith
  runtimes remain in verified lossless archives; only their expanded copies
  were retired. [Csmith receipt](tests/contracts/ladder-csmith-receipt.json),
  [Mbed TLS receipt](tests/contracts/ladder-mbedtls-receipt.json).
  October 27 classification of the 19 old PASS programs that the run of
  2026-10-25 lost (ctest 2, torture 5, LLVM 12), with the compiler of e7a3408c
  and each original command. Six stop at `optimize` (E2952): torture `pr60062`,
  `pr60072`, `alias-1` and `pr79043`, and two LLVM copies. One stops at
  `stdcall` (E2952): ctest 00210. Seven LLVM programs stop at a strict-driver
  option, because the LLVM projection has passed `-f` options to the driver
  since 2026-09-28: `-fno-common`, `-fno-builtin-abort`, `-fno-inline`,
  `-finput-charset=utf-8`, `-fexpensive-optimizations`, `-ftree-vectorize` with
  `-fno-vect-cost-model`, and `-fno-trapping-math`. Two stop at an incompatible
  pointer argument under `-Wno-error=incompatible-pointer-types` (E2202):
  PolyBench `correlation` and Shootout `methcall`. One stops at a call to an
  old-style definition (E2202): torture `920302-1`. One stops at a nonconstant
  range-designator value (E2917): ctest 00216. LLVM `acle-fmv-features` still
  passes natively; its reference build gives no executable after
  `--rtlib=compiler-rt`, so it is a reference loss and not a compiler loss. The
  `optimize`, `stdcall`, four option and range cases have goals. The three
  permission-only options and `-fno-trapping-math`, the incompatible-pointer
  downgrade and the old-style call wait for owner decisions.
  October 27 range-value repair: Linux evaluates each surviving nonconstant
  scalar or record range value once, in destination order, and copies a
  snapshot of a shared record. Fully overwritten values do not execute.
  Darwin now reports E2969 at the value; E2917 remains an index/bound code.
  A later fix gives E2969, not E2202, to the Linux refusal of a nonconstant
  array value; 11 forms that GCC 14 refuses are tested on both targets.
  The original c-testsuite `00216.c` and its expected output are embedded
  byte-for-byte in `range_designator_values.rs`. Its native GCC/EvoCC replay
  matches all 43 output lines and exits zero. Eighteen smaller cases cover
  both interpreters, `--check`, native GCC comparison, overlap, macro values,
  conversions, branches, snapshots and repeated executions. The five parser
  controls pass; all six Lean gate steps pass. The package's full-workspace
  result is retained with the release-batch evidence. This bounded replay
  restores one old PASS; the complete frozen B10 campaign remains open.
  October 27 `optimize` repair: the attribute accepts integer levels 0 to
  3, the level strings `O`, `O0` to `O3`, `Os` and `Og`, and the twelve
  disabling `-f` options that the strict driver accepts with no change of
  code, `no-strict-aliasing` among them. Every other option keeps E2952.
  Both drivers now accept `-fno-tree-vectorize`, `-fno-tree-loop-vectorize`
  and `-fno-tree-slp-vectorize` on both targets, as GCC 14 does. On Darwin
  EvoCC differs from Apple clang, which refuses `-fno-tree-loop-vectorize`
  as an unknown argument and ignores `optimize` with a warning. At `-O2`,
  Apple clang's strict aliasing makes torture `alias-1` and `pr79043`
  abort; EvoCC and GCC 14 run them with exit 0. `optimize_attribute_levels.rs`
  embeds `pr60062`, `pr60072`, `alias-1` and `pr79043`: each exits 0 natively
  on Darwin and in the sealed GCC 14 image on Linux, at `-O0` and `-O2`.
  The two LLVM copies of `pr60062` and `pr60072` wait for the B10 campaign.
  October 27 option repair: both drivers accept `-fno-common` and
  `-finput-charset=` with `utf-8` or `utf8` in any letter case, with no
  change of code. `-fno-builtin-NAME` disables the recognition of one
  name, and the name stays disabled after `-fbuiltin`, as in GCC 14.
  `-fno-inline` substitutes only `always_inline` callees. `-fcommon`,
  other charsets, `-finline` and an empty builtin name stay unsupported.
  GCC 14 also accepts iconv names of UTF-8, such as `ISO-10646/UTF8/`,
  which EvoCC refuses; Apple clang refuses `utf8`. With their original
  arguments, LLVM `20040709-3`, `pr68506`, `stkalign` and `wchar_t-1` exit
  0 in the sealed GCC 14 image with GCC 14's output
  (`selected_contract_options.rs`). The `driver_cc.rs` warning test now
  uses `-finstrument-functions` as its unsupported option.
  October 27 permission-option repair: both drivers accept
  `-fexpensive-optimizations`, `-ftree-vectorize`, `-fno-vect-cost-model`,
  `-ftrapping-math` and `-fno-trapping-math` on both targets with no change
  of code. On Linux a final `-fno-trapping-math` defines
  `__NO_TRAPPING_MATH__`, as in GCC 14; Darwin defines nothing, as Apple
  clang does not. The fast-math options and `-Ofast` keep their rule.
  Apple clang warns (`-Wignored-optimization-argument`) for
  `-fexpensive-optimizations` and `-fno-vect-cost-model` and fails with
  `-Werror`; EvoCC stays silent. With their original arguments, LLVM
  `pr68381`, `pr68532` and `compare-fp-3` exit 0 in the sealed GCC 14 image
  with GCC 14's output (`permission_only_options.rs`).
  October 27 x86 calling-convention repair: `stdcall`, `fastcall`,
  `thiscall`, `vectorcall`, `regcall` and `sseregparm` on both targets, and
  `regparm` on Linux, are the warning W0011 (`attributes`) at the attribute
  name and have no effect on code, as in GCC 14 and Apple clang.
  `-Wno-attributes` and `-w` silence it; `-Werror` and `-Werror=attributes`
  make it an error. E2952 stays where a reference refuses the form: after the
  abstract declarator of a type name, and on Darwin for a convention that
  Apple clang knows, on a null statement or with an argument. On Darwin, a
  known convention with an argument, such as `stdcall(1)`, is now E2952 also
  on a prototype, which D17 accepted; Apple clang refuses it there. Darwin
  keeps E2952 for `regparm`. Apple clang refuses `regparm` also on an unused
  Darwin prototype, and EvoCC accepts and records it there (D17).
  `x86_calling_conventions.rs` embeds ctest `00210`: it prints
  `00210.c.expected` and exits 0 natively on Darwin and in the sealed GCC 14
  image. Calls in both directions with GCC 14 and Apple clang objects agree.
  October 27 incompatible-pointer repair: W0012 (`incompatible-pointer-types`)
  reports the assignments, initializations, prototyped arguments, returns and
  conditional operators that GCC 14 reports with `-Wincompatible-pointer-types`,
  with GCC 14's levels: an error in C99 and later modes and a warning in C89 and
  GNU89, lowered to a warning by `-fpermissive` and
  `-Wno-error=incompatible-pointer-types`, and removed by
  `-Wno-incompatible-pointer-types`; `-w` removes only the warning. A warned
  conversion is an explicit cast, and such a conditional has type `void *`.
  `incompatible_pointer_types.rs` checks the 50 measured sources in four modes
  with the 37 measured option sets on both targets. With their original
  arguments, LLVM PolyBench `correlation` and Shootout `methcall` give the
  reference output and exit 0 in the sealed GCC 14 image. Three forms that
  EvoCC accepted are now W0012 as in GCC 14: a conditional whose result was
  discarded or whose operands pointed to character types, `_Atomic T *` from
  `T *`, and `(T *)0` to another pointer type. A GCC 14 scan of the LLVM,
  torture and ctest units of the October 25 run finds no such form in a unit
  that compiled. LLVM `Regression-C-globalrefs` and `Vector-build2` also
  compile now; their dispositions wait for B10. On Darwin EvoCC is stricter
  than Apple clang, which only warns for data pointers and conditionals. Two
  differences from GCC 14 remain: `-fpermissive` lowers only W0012, while
  GCC 14 also lowers implicit function declarations, implicit `int` and
  integer-pointer conversions; and `void *` from a pointer to an atomic type
  stays E2202, while GCC 14 accepts it without a diagnostic.
  October 27 old-style call repair: a call without a prototype of a function
  defined with an identifier list or an empty parameter list takes the default
  argument promotions, as in C11 6.5.2.2p6, and EvoCC compiles it as GCC 14
  does, with no diagnostic. Both interpreters and `--check` report E3068
  (`old-style-call-mismatch`) at a call whose argument count differs or whose
  promoted types do not agree, outside the two exceptions; native code has no
  check for it (EvoMIR 5.2). `old_style_definition_calls.rs` checks the 25
  measured sources in four modes on both targets. The torture original
  `920302-1` exits 0 natively on Darwin and in the sealed GCC 14 image, and
  `--check` reports E3068 at its line 41. Calls with a prototype in scope keep
  their rules; GCC 14 refuses too many or too few arguments there, while EvoCC
  reports E3051 only at code generation. No C23 mode exists yet; in C23
  `int f()` is a prototype, and the rule must not apply there. Open: a call
  without a prototype of a prototyped definition is also undefined and is not
  reported; a call through a function pointer object keeps the E3019 signature
  check and does not get this rule, and `(*f)(...)` and `(&f)(...)` keep the
  prototype check.

  October 27 frozen campaign (B10): 18 of the 19 old PASS programs that the run of 2026-10-25
  lost pass again with their original commands. `optimize` (`d56d3e5f`): torture `pr60062`,
  `pr60072`, `alias-1`, `pr79043` and the LLVM copies of `pr60062` and `pr60072`. `stdcall`
  (`7a79e9b6`): ctest 00210. The strict-driver options (`b37e9499`): LLVM `20040709-3`,
  `pr68506`, `stkalign` and `wchar_t-1`. The permission-only options (`e424f638`): LLVM
  `pr68381`, `pr68532` and `compare-fp-3`. Incompatible pointers (`53a9bf31`): LLVM `correlation`
  and `Shootout-methcall`. The old-style call (`0c953e05`): torture `920302-1`. The range value
  (`e77966c0`, `5b082542`): ctest 00216. LLVM `acle-fmv-features` keeps its reference-side cause:
  its reference build finds compiler-rt, enables FMV, and the unchanged Clang 19 rejects its
  CSSC check. The 2x and whole-ladder size exits stay open.

- [x] **N10 — Keep one-word records and small arrays in registers** — `DONE`. [History](docs/archive/todo-0.1.0.md#n10).
- [ ] **N11 — Sign only the returns that need a signature** — `OPEN`, P2.
  K15 signs every function, leaves included, for `-mbranch-protection=pac-ret`.
  GCC 14 signs only non-leaf functions for that option. The K02 fault path
  runs 1,337 return-signing instructions per fault (GCC 14: 321). Next
  action: sign every function that stores its return address, and leave out
  the signature only where the return address never leaves `x30`; keep
  signing every function for `pac-ret+leaf`. Exit: driver tests pin both
  options, and a function that stores `x30` is always signed.
  October 14: the
  [analysis](docs/archive/notebook/perf-n08-n11-analysis-20261014.md) found that
  `protects_codegen()` keeps a frame record in every function, so every leaf
  signs. Next action: make the protection a per-variant set, let only
  `-mno-omit-leaf-frame-pointer` keep leaf frames at the kernel flags, as GCC 14
  does, and leave frameless leaves unsigned under `pac-ret`. Kernel stack
  traces, ftrace and kprobes need the acceptance lane.
  October 16: allocated leaf admission now separates the leaf-frame request
  from the other protected variants. The two PAC scopes and ACLE macro share
  one effective policy. Sixteen target-native runs and eight TCG runs pass;
  the existing Linux unwind/canary test passes all eight combinations. The
  original page allocator/MMU probes omit 19/15 unused records; every saved
  x30 remains signed. Combined acceptance passes 3,449 tests with zero failures,
  eight ignores, Clippy/fmt and all six Lean steps. Kernel
  tracing acceptance remains NOT_RUN. D13/D14 separately close the Darwin
  failures found by the runtime fixture, with their own acceptance record.
  [Signing record](tests/fixtures/kernel-return-signing.out).
  October 19 integration: the retained 2,799-target-C virtconfig kernel builds
  directly with these changes and boots on both HVF (pointer authentication
  enabled) and TCG cortex-a72 (no pointer authentication). Nine paired programs,
  125 subtests per kernel, pass. The build-specific full-workspace gate passes
  3,473 tests, zero failures, eight ignores, Clippy/fmt; all six Lean steps
  pass. The one compiler snapshot and all per-object counts are retained in the
  [direct-build receipt](tests/contracts/kernel-direct-virt-receipt.json).
  This dirty incremental build does not close tracing acceptance: ftrace and
  kprobes are disabled in this configuration and remain NOT_RUN.

## Benchmark coverage index

Every measured application lane remains in the final campaign; already passing
lanes are regression obligations, not newly invented defects. Counts below stay
in the receipts/README so this index cannot independently drift.

| Population | Tasks controlling closure |
|---|---|
| GCC torture; c-testsuite | B04, L02/L04/L05, approved per-case dispositions, B10 |
| Csmith | B03, B10; original 500 base + 500 bitfield seeds |
| LLVM test-suite | B01/B02, L03–L10 as applicable, B10 |
| TinyCC; toybox; jq; musl + libc-test | B05; B06; B07; B08, then B10 |
| cJSON | B09, B10 |
| zlib; zstd; Mbed TLS; PolyBench/C; Lua; SQLite | Preserve recorded passing scope and rerun B10; Q2 is R02 |
| K6 locked and expanded configurations | Preserve existing boot acceptance; T04 for changed compositions |
| Full kselftest; expanded built-in/module KUnit | K01–K14, T02/T04/T05 |
| EqSat, performance, portability, release, Doom | R01–R06; not application-PASS arithmetic |

## Verified work to retain — not reopened by this list

- [x] **C01 — Locked K6 full compile/link/boot/shutdown:** — `DONE`. [History](docs/archive/todo-0.1.0.md#c01).
- [x] **C02 — Function-section repair and BPF/module boot:** — `DONE`. [History](docs/archive/todo-0.1.0.md#c02).
- [x] **C03 — Selected K7 subset:** — `DONE`. [History](docs/archive/todo-0.1.0.md#c03).
- [x] **C04 — Retained expanded KUnit module composition:** — `DONE`. [History](docs/archive/todo-0.1.0.md#c04).
- [x] **C05 — ALSA timer userspace dependency repair:** — `DONE`. [History](docs/archive/todo-0.1.0.md#c05).
- [x] **C06 — TinyCC static-initializer E3101 and targeted LLVM repairs:** — `DONE`. [History](docs/archive/todo-0.1.0.md#c06).
[kernel]: docs/archive/notebook/kernel-selftest-gaps-20260914.md
[apps]: docs/archive/notebook/benchmark-evaluation-20260915.md
[semantics]: docs/archive/notebook/remaining-semantics-research-brief-20260921.md
[roadmap]: docs/archive/roadmap.md
[selftest-receipt]: tests/contracts/kernel-selftest-full-receipt.json
[kunit-receipt]: tests/contracts/kernel-expanded-kunit-receipt.json
[strlen]: tests/fixtures/fortify-nonliteral-strlen-ice.c
[strlen-out]: tests/fixtures/fortify-nonliteral-strlen-ice.out
[closure-batch]: docs/archive/notebook/todo-closure-batch-20260927.md
[complex-prompt]: docs/archive/notebook/remaining-complex-research-prompt-20260927.md
[eqsat-plan]: docs/verified-optimization-design.md
