# EvoCC remaining work

Updated **2026-10-07**, initially audited against `a83d803`; current clauses are
EvoIR **2.0.0** and EvoMIR **1.0.1** (pinned in `spec/`). This is the Git-tracked execution backlog,
not a new test run.
Task IDs are stable: update or split an item with cross-references; do not recycle
IDs or silently delete unresolved work. Historical review IDs remain in their
original ledgers; the IDs below belong to this tracker.

The [evidence document](docs/evidence.md#status--2026-09-28) summarizes measured capability;
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
| KERNEL | K01, K01b, K02, K03, K04, K10, K11, K11f, K12, K16, K17 | Start with K02/K03's existing current-Image profiles; produce a cause-backed hot-path reduction, then address BPF/pidfd budgets and intermittent sched_ext/ublk failures. |
| APPLICATION | B08, B08b, B08d, B08d10, P17a, P22 | P21 completed 144 registered sessions: P1-P3 held; P4 and the earlier control penalty did not. P22 is separate, not started. P01 through P20 retain their completed scope: P20 had no repair-count advantage; P18 stopped at the provider key limit. Existing application tasks keep separate exits. |
| REVIEW | K07, L03, L04, L05, L06, L07, L08, L09, L10, L11a, L11b, L11c | Existing K07/L10 and L04 decisions stay separate; use the bounded decision packet, not new broad FP research. The owner resolved P13's demo choice without extending the proof profile. |
| AUDIT | T03, D01, D02, D03, D04, D05, D06 | V00-V08 are complete in the frozen scope. T03 preserves the wider semantic/acceptance boundaries; frozen EqSat studies are unchanged. D01-D05 are the code defects of the 2026-10-06 specification review. D06 retains the separate 2026-10-07 guarded-division lowering refusal; do not fix it as part of P13-P16. |
| ACCEPTANCE | T04, T02, K13, K14, B10, T05 | Independently freeze repaired kernel/application subjects, run their existing paired/serial acceptance and refresh health/docs. |
| LATER | R01, R02, R03, R03a, R03b, R03c, R03d, R03e, R03f, R03g, R03h, R04, R05, N06 | S1–S5 and R02e first slice are closed; S6 stays with R01's two G2 builds. R03 V1 negative-admission study is frozen; positive Q3 remains unachieved and R03a/b/c/d NOT_ADMITTED, R03e deferred. No VM timing or audit expansion; x86-64 and release stay separate. The bounded N01–N05/N07 batch is dispositioned, not a complete optimizing compiler: N03b/N03e preflights are negative, N07 E3 has zero SMT proofs, and all five library size targets remain missed. N03d C5 has size/correctness evidence but three compile-cost misses; C2/C5 remain outside the admitted cumulative configuration, and T1's misses stay frozen. All new defaults remain OFF. Retain N02's 70 admitted/187 opaque inputs and source-bridge mismatch. Broader inlining, mid-level and allocator work need newly scoped designs; no paused R activity is automatically reopened. |
<!-- closure-queues:end -->

## Diagnostics and command-line product work

Owner contract: October 7. Complete these packages in order. Close each package
with its RED test commit, implementation, full offline workspace gate and the
observed exit results. Do not start the next package before that gate passes.
Do not change Lean, the verification specification, the proof-required pipeline
or rewrite checkers. Preserve existing emitted IR and assembly bytes. Do not
add dependencies, crates or configuration systems. Do not push.

- [x] **P01 — Render source diagnostics and freeze their format** — `DONE`, P1.
  Final review: retain the stable format and all 62 current snapshot triples.
  Exit: at least 40 text/JSON UI snapshots, workspace message-style checks,
  the hand-written JSON validator and full workspace gate pass. Budget:
  900 production Rust lines and 300 specification lines. Runtime locations
  must come from execution metadata, not a guessed source expression.
  October 7 checkpoint: renderer REDs committed at `77dec0cc`; nine public
  presentation/location tests pass. Runtime origins follow the executed
  instruction through calls and SSA renumbering. Included source snippets use
  the bytes read during preprocessing. A paired lowering test checks identical
  IR and execution results with source capture on and off. The presentation
  contract is pinned at revision 1.0.0 (126 lines).
  Second checkpoint: RED `74990d2e` records missing related declaration labels
  and execution-limit notes. All 12 presentation tests now pass. The product
  suite passes 11 tests, including byte-exact replay of 42 text/JSON pairs and
  schema mutation controls. The 34-line JSON validator checks every snapshot;
  the corpus covers E0--E4, related spans, multiline spans, notes, help, multiple
  diagnostics and includes. Input errors use command-line source rather than
  invented C positions. Runtime refusal and resource-limit notes keep their
  meaning. Targeted Clippy passes with warnings denied. Conservative inserted
  production Rust at this checkpoint was 754 of 900 lines. The 13 legacy-format suites now request
  `short` explicitly and pass on replay; their diagnostic codes, locations and
  exits remain checked. Two native suites pass all eight tests after moving
  disposable files outside retained evidence (`a294e3df`). The broad driver run
  initially failed these 15 targets; all were replayed, but a new full workspace
  gate has not run. This is partial work: workspace-wide
  style checks, remaining invocation errors and the full workspace exit remain
  open. P02 through P05 have not started.
  Third checkpoint: RED `13f68767` records three command-line renderer bypasses.
  Empty invocations, unsupported capabilities and malformed probes now retain
  exit 69 but use the common renderer and command-line span. The diagnostic,
  product, EvoIR CLI and cc-driver suites pass 44 tests; targeted Clippy with
  warnings denied and workspace formatting pass. Conservative production Rust
  insertions, including blank lines, are 765 of 900. Workspace-wide message
  style and the full workspace exit remain open. No later package has started.
  Fourth checkpoint: RED `e6aeead2` and implementation `6e297e35` cover header
  style and disposable native-test paths. The focused presentation, product
  and style suites pass 35 tests. Inventory checks cover 679 source headers,
  43 snapshot headers and 823 error-detail templates; the period and redundant-prefix checks also
  inspect details. RED `c9b5a4bb` fixes an inventory blind spot after test-only
  items without excluding the later production code. RED `72d614cb` records
  unreadable input files being labeled as the whole command, not the affected
  argument; the correction covers spaces, newlines and UTF-8 byte columns.
  The first full gate passes: 412 suites, 3,147 passed, zero failed, five
  ignored; Clippy and formatting pass. It compiled tests before the final
  span regression, so it does not close the package. The final focused replay
  passes 36 tests, including 42 unchanged snapshot pairs. Its first attempt
  caught an overbroad span change in the backend snapshot; only E0003 now uses
  the input argument. No expectation was rewritten to hide that regression.
  Current inserted/replaced production Rust is 782 of 900 lines, excluding
  test modules. The final full gate remains required. No later package has
  started. Build space is above the 25 GiB admission threshold; only the
  unused, rebuildable target/frontend incremental cache was removed (5.8 GiB),
  after process and open-handle checks. No protected artifact was removed.
  Completion: the final clean subject `a05d24e8` passes the full offline
  workspace gate: 412 suites, 3,148 passed, zero failed, five pre-existing
  ignored campaigns; Clippy with warnings denied and formatting pass. Log:
  `.evidence/diagnostics/20261007/a05d24e8-full-workspace.log` (under 500 MiB).
  All 42 C/text/JSON snapshot triples pass without UPDATE_EXPECT. The 34-line
  JSON validator, eight style tests and 17 public rendering tests pass; a
  separate PTY check confirms automatic terminal color. The contract is 126
  lines and the production Rust budget is 782/900. The eight production files
  are cpp/lib, evoir/lib, evoir/lifecycle, lower/lib, parser/lib, evocc/cc,
  evocc/diagnostics and evocc/main. Tests preserve legacy short output, source
  bytes, executed origins, related labels and output stability. This changes
  presentation only: no new lint, diagnostic code or semantic acceptance.
- [x] **P02 — Explain every authoritative diagnostic** — `DONE`, P1.
  Final review: 122 pages now cover the authority table, including five warnings;
  205 tagged examples are tested. The 20 explicit Trigger: none exceptions remain.
  Exit: complete page/table coverage, triggering and fixed examples checked,
  `--explain`, generated book index and full workspace gate pass. Report every
  `Trigger: none` exception. Budget: 300 Rust/Python lines excluding pages;
  each page at most 40 lines, index generator at most 150 lines.
  Checkpoint: RED `7a7bc855` precedes the implementation. All 117 authority
  rows now have hand-written pages; all 195 tagged C examples pass with the
  first diagnostic checked or no diagnostic for the fixed example. The public
  explanation and check-product suites pass 14 tests, including all 117 exact
  CLI texts, table/page equality and generated-index freshness. mdBook 0.4.52
  builds the chapter. Inserted/replaced Rust and Python total 229 lines,
  including tests; the index generator has 38 lines. No compiler semantics,
  code numbers, proof path or Lean source changed.
  `Trigger: none` (20): E0109, E2902, E2918, E3006, E3007, E3017, E3018,
  E3020, E3023, E3025, E3029, E3032, E3034, E3035, E3036, E3037, E3042,
  E3043, E3045, E3062. Each page explains its reason. Observed differences
  remain outside this slice: E0109 wraps without a diagnostic; the parser
  emits E1003 for the E2918 condition; the memory fault for E3018 maps to
  E3007. Nonzero return-address requests are rejected as E2913 before E3025.
  The first full gate stopped at test compilation: one catalogue unit test
  still used the removed struct fields. Its assertions now use the page
  catalogue; the full gate must be repeated. No later package has started. Only text and
  a temporary mdBook download/site were added; no protected artifacts were
  deleted. Final disk accounting follows the exit gate.
  Completion: RED `7a7bc855`, implementation `73d558fb`, catalogue-test
  correction `48b31f1f`. The clean final subject passes the full offline
  workspace gate: 413 suites, 3,151 passed, zero failed, five pre-existing
  ignored campaigns; Clippy with warnings denied and formatting pass. Log:
  `.evidence/diagnostics/20261007/48b31f1f-full-workspace.log` (SHA-256
  `442f838b211c46e00454b13f998589830fa61b5fa83474f92127b6a263420312`).
  The prior sandbox run had two Linux container permission failures; the
  complete authorized replay passes both, without skipping tests. Both failed
  attempts are retained beside the final log. The 117 pages total 2,041 lines
  (maximum 27 each); the generated index has 2,174 lines. Examples: 97 trigger
  and 98 fixed cases, all checked. The 20 exceptions above remain explicit.
  The Rust/Python budget is 229/300 including tests. The index generator,
  five specification pins and Git whitespace checks pass. mdBook 0.4.52 built
  the chapter; no hosted CI result is claimed. The temporary book binary,
  archive and HTML site were removed (about 16 MiB); protected data was not
  removed. From the measured 02:59 checkpoint, target/frontend plus temporary
  capture fell by 19,396 KiB before retained-log copies. The full-package
  build-cache baseline was not measured; no full-package disk delta is claimed.
  Available space is 30 GiB. No later package started before this gate passed.
- [x] **P03 — Add five front-end lints and level control** — `DONE`, P1.
  Final review: all five warnings and their level controls retain their original
  application acceptance. No additional warning or GCC-compatibility claim is added.
  Closed on October 7 after the full workspace gate and final application replay.
  Exit: warning-level UI matrix, unchanged zlib/cJSON builds with recorded
  warnings and full workspace gate pass. Pin the revised authority table.
  Budget: 700 production Rust lines. No lint registry or new lint framework.
  October 7 checkpoint: RED `147bb7af` records six failing public tests.
  The implementation adds the five warnings, named and group levels, promotion,
  suppression and caps. Symbol identities distinguish shadowed variables;
  a source CFG checks reachable function ends without changing executable IR.
  Prototypes, unused attributes, complete returns, loops, switches and gotos
  have controls. A separate RED exposed unused file-local static objects and
  now passes. Return-value type mismatches keep the existing E1207 error;
  they are not the approved return-type lint. Focused results: 28 tests pass,
  including all 62 C/text/JSON snapshots, all 122 explanation pages and 205
  tagged examples. Only the old undeclared-call snapshot changes to W0003.
  The five warning pages use explicit named deny flags for their trigger tests.
  EvoIR revision 2.0.1 and its pin add the warning authority and level contract.
  cJSON 1.7.19 `make all test CC=evocc` passes with its original `-Wall -Werror`:
  zero W0001--W0005 messages and three existing unsupported-stack-protector
  warnings. zlib 1.3.2 configure plus `make test` passes for static and shared
  libraries, with zero warnings. That configure chose `-O`; a second clean
  configure/build with `CFLAGS='-O -Wall -Wextra'` also passes, with zero
  warnings. Source copies and logs occupy under 8 MiB in
  `/tmp/evocc-lints.UWMB17`, with a 500 MiB retention cap. The existing source
  trees were unchanged. Production inserted/replaced Rust is 413/700 lines
  at this checkpoint. Full workspace, Clippy and final cleanup remain required;
  this is not package closure. No later package has started.
  **October 7 closure:** implementation `f1faa2e9`; attribute RED `61eb8677`
  and fix `08d8f8bf`; prior diagnostic controls updated in `16dfd8ad` and
  `c2209111`. The first full gate has 3,154 passes and three failures: an empty
  non-void definition now warns, and two undeclared-call controls now use
  W0003 rather than E2001. All three inputs and their exit/no-output checks
  remain. A separate RED reproduces false positives for `used` objects and
  direct `noreturn` calls; the fix reads declaration attributes by symbol.
  The affected replay passes 32 tests. On clean subject `c2209111`, the final
  full workspace gate passes: 414 suites, 3,158 passed, zero failed, five
  existing ignored campaigns; Clippy with warnings denied and formatting pass.
  The final binary again passes cJSON `all test` with its own warning flags
  and zlib static/shared tests with `-O -Wall -Wextra`. Both have zero lint
  warnings; cJSON retains three unsupported-stack-protector warnings.
  Production inserted/replaced Rust is 436/700 lines: CHIR export 1, CHIR
  lints 262, parser 37, catalogue build 4, cc dispatch 9, lint levels 80,
  driver 43. The public lint test has 209 lines. Five explanation pages have
  17/19/18/17/19 lines. The 62 UI cases and 205 tagged examples pass.
  Logs, including failed runs, are retained under
  `.evidence/diagnostics/20261007/lints/` (716 KiB, below the 500 MiB cap).
  The final full-gate log SHA-256 is
  `d40d737b1b27fcc54edc89fddfa813d2e63721e82c945d8e4b6a06e406dd5e04`.
  Removed the 8.3 MiB temporary source/build capture after handle checks and
  log preservation; original application sources and protected evidence stay.
  Since the recorded P02 closure checkpoint, target/frontend grew by
  1,251,676 KiB; retained logs add 716 KiB (net 1,252,392 KiB for those measured
  paths). This is not filesystem-wide accounting. Available disk is 26 GiB.
  Claim: five controlled front-end lints, not all GCC warning semantics.
- [x] **P04 — Add user-intent commands** — `DONE`, P1.
  Final review: all six verbs, packaged sibling lookup and the cJSON build receipt
  retain their recorded acceptance. No build system or interpreter libc was added.
  Closed on October 7 after the full gate and packaged-command acceptance. The six
  verbs delegate to the existing driver; no driver library was introduced.
  RED: `9b1793f0`, seven failures because the binary did not exist. The focused
  suite now passes eleven tests, including the real cJSON build, writable
  argc/argv, non-UTF-8 argument bytes, retained proof files on copy failure,
  warning counts, source/output distinction and expected configure failures.
  The wrapper uses 406 Rust lines; driver/interpreter argument and definition
  interfaces add 179, for 585/600 total. Book and release changes use 91/100
  added lines. Book commands were executed in `/tmp/evocc-user-book.5vvZWJ`;
  cJSON exits 0, lists three compiled C files and no coded warnings. Captures
  have a 500 MiB cap. The first focused Clippy run found manual modulo parity;
  use `is_multiple_of` instead. Its replay and the full gate remain required.
  No later package has started. Lean and the proof-required pipeline are unchanged.
  October 7 entry check: P03 is closed. The existing C-HIR command calls main
  without arguments and rejects `main(int argc, char **argv)` with E3007.
  It has no program-argument option. A thin command wrapper alone cannot meet
  `evo run FILE.c -- ARGS`. The required argument bridge is part of the explicit
  run contract, not a new feature. Count its Rust lines in the 600-line budget;
  preserve old invocations and do not rewrite C source or drop ARGS.
  Reproducer: `.evidence/diagnostics/20261007/evo-run-arguments-red.log`.
  Exit: verb tests, actual cJSON build receipt, tested book commands, release
  packaging and full workspace gate pass. Budget: 600 lines for the binary and
  100 lines for release-script and book changes.
  **October 7 closure:** implementation `d69cadc5` passes the clean offline
  full workspace gate: 416 suites, 3,169 passed, zero failed and five existing
  ignored campaigns. Clippy with warnings denied and formatting pass. Its log
  SHA-256 is `ee3745b0e3ea99aec0256673a68632ace2e0813e1bcc801917f1a8cda2c4543b`.
  The release build passes. The Darwin archive contains all four binaries;
  all packaged file hashes match. Without EVOCC or EVOCC_PROOF_CHECKER,
  the packaged verbs use their sibling binaries: run returns 55, check
  completes, explain prints its page, and prove accepts abs with
  CHECKED_TO_FINAL_ASSEMBLY. The packaged build command passes cJSON's
  original `all test -j2`, records three source inputs and zero coded warnings.
  Archive SHA-256: `5d457612abe158fb9071150dffa1023fac270d05ead862157d04b1aa76c4b4d4`.
  The binary is 406 lines; bridge code adds 179 (585/600 total); tests are
  300 lines. Book/release additions remain 91/100. The retained capture is
  `.evidence/diagnostics/20261007/user-commands/` (412 KiB), including the
  failed Clippy attempt, successful gate, build logs and packaged proof.
  After process and handle checks, removed 4,666,884 KiB of rebuildable
  incremental cache and about 152 MiB of owned temporary packages/application
  products. Protected trees and original sources were not deleted. From the
  measured 04:25 checkpoint, target/frontend plus retained evidence fell by
  2,358,100 KiB; the full-package starting cache size was not measured.
  Free space is 29 GiB. Claim: six delegated user commands, not external-libc
  interpreter execution or new proof coverage. Linux archive execution is not
  claimed by this Darwin packaging check. P05 may now start.
- [x] **P05 — Package the Linux header sysroot** — `DONE`, P1.
  Closed after the owner approved the excluded-include wording. The default
  macOS volume is supported. The book and generated contract state that the
  eight excluded headers are unsupported and promise neither lookup result.
  Include lookup stays unchanged; no new diagnostic or installation restriction
  is added. All original exit checks pass as recorded below. Earlier requests
  for a storage decision or file-not-found guarantee are superseded.
  The owner approved default macOS volume support by excluding uppercase
  case-colliding headers; no case-sensitive installation requirement remains.
  October 7 entry check: four isolated driver tests reproduce three missing
  behaviors: bundled/environment lookup, selected-root errors and the explicit
  cross-linker requirement. The `-nostdinc` control already passes. Capture cap:
  500 MiB under `.evidence/diagnostics/20261007/sysroot/`; retain only logs,
  contracts and required small outputs. The repository volume is case-insensitive.
  The sealed headers have distinct `xt_CONNMARK.h` and `xt_connmark.h` files,
  with different contents, on the existing case-sensitive test volume. A full
  sysroot cannot be unpacked faithfully on the repository volume. Owner decision
  requested: require a case-sensitive sysroot location for this release, or
  approve a separate storage/lookup change. Do not drop or rename headers.
  Driver checkpoint: RED commit `a7003b21` has three expected failures and one
  passing control. The implementation now passes all six sysroot tests and
  all eleven cc-driver tests; focused Clippy with warnings denied passes.
  Lookup is explicit, environment, bundled, then existing sealed exports on
  macOS; native Linux retains its host root as the final native fallback.
  A selected but invalid root fails without fallback. Header discovery does
  not change linker library search. `.S` inputs receive the selected root.
  Production driver changes use 96/250 inserted/replaced lines. Export,
  packaging, contract, cross-build and full-workspace acceptance remain open.
  The case collision decision blocks faithful release packaging, not these
  driver tests. Retained capture is 16 KiB; test-owned temporary directories
  are removed. Since the prior 04:47 target measurement, target/frontend grew
  by 123,468 KiB before the final Clippy completion; this is a checkpoint,
  not final package disk accounting. Available space remains 29 GiB.
  Export checkpoint: RED `32e3409a` records two failures because the exporter
  was absent. `tools/export-sysroot.sh` now uses 106/120 lines, reads the actual
  image identity and GCC version, follows header-directory symlinks, refuses
  case-insensitive storage and keeps differing existing files intact. The
  existing case-sensitive volume holds one new 63,676 KiB export at
  `.build/k7-coverage/case/product-sysroot-20261007/`. Its tracked contract is
  `contracts/sysroot-aarch64-unknown-linux-gnu.json`: 3,974 files. Independent
  `find -L` and `sha256sum` in the image find zero missing, extra or changed
  files. GCC reports no `include-fixed` directory; the contract records that.
  A second export preserves contract SHA-256
  `32bdf48db11bdb7f2a8de5a6742766bd3b55c3c400db6d472663d298ba6414f5`.
  Four Python checks pass with the real export; all seven sysroot Rust tests
  pass, including the contract check invoked by the workspace test. Formatting
  and shell syntax pass. The first runtime command refused a bare image digest;
  the exporter now uses the installed Apple image tag and checks that its
  metadata is unchanged, while Docker uses the immutable image ID. Both run
  without guest network access. Logs retain that failure. No new image or
  disk was created. Package layout, CI, cross-build, full-workspace acceptance
  and the owner decision on macOS storage remain open.
  Cross-build checkpoint: the actual adjacent `bin/` and `sysroot/` layout
  produces an ELF AArch64 `hello.o` with no sysroot option or environment
  override. The unmodified zlib 1.3.2 source builds all 15 members of `libz.a`
  with `CFLAGS=-O` and no include paths. Its configure script needs link
  probes: the first run correctly refused implicit Linux linking and then
  misdetected libc features. A test-only explicit linker invokes GCC in the
  existing sealed container; the repeated original configure finds errno,
  fseeko, strerror and vsnprintf normally. No source or feature macro is
  changed, and no insecure fallback is enabled. Native hello, the original
  zlib example and minigzip round trip exit zero. The debug compiler hash is
  `a4ffb63b8f1b5e17bc576a61af81eb171e7f97bf79f537eff5545905e9e7fa6f`
  (version subject `32e3409a`); this is not final release acceptance.
  Archive RED `ccc6953a` fails on the old root-level binary layout. The new
  `bin/` layout and all 3,974 header hashes pass the five Python checks on a
  real archive. Release-script changes use 18/60 lines; the five changed
  workflow lines only follow the new binary paths, not the pending CI header
  export or cross-host transfer. The archive structure check used the earlier
  `d69cadc5` release binaries and is not a current-binary acceptance claim.
  Book cross-compilation commands were executed. The storage warning states
  a physical file limitation, not owner approval of a narrowed release goal.
  Current-binary checkpoint: the release build at `f425e503` passes. Its
  archive SHA-256 is
  `6f9b5439bda84b9c926803fac7d2deae21dd6f5ee92fe4cb7c88ea7c64ed45e1`.
  All five archive/contract checks pass. The packaged compiler produces the
  same Linux hello object bytes as the earlier debug compiler. This checks
  adjacent header discovery, not extraction onto a case-insensitive volume.
  The first full workspace run completes 417 suites: 3,175 passed, one
  failed and five ignored. The new cross-link error starts with "Linux",
  which fails the diagnostic lowercase rule. Clippy and formatting did not
  run after the test failure. The retained log SHA-256 is
  `190cc1fa46367b8de12d09297c44c09a721be64d66cab20f0bb6b864952e48a0`.
  Rewording the error passes all eight style tests and seven sysroot tests;
  full-workspace acceptance still requires a new run. After checking open
  handles, removed the owned 210,772 KiB cross-build temporary directory.
  Source, small objects/archive, commands, hashes and logs remain in the
  protected capture (3,924 KiB). Since the start-of-cleanup measurement,
  target/frontend plus capture plus the temporary directory fell by
  210,528 KiB. No protected file or shared header export was removed.
  Header-transfer CI and the owner storage decision remain open.
  **October 7 gate checkpoint:** clean implementation `08ed5e0c` passes
  `tools/gate-changed.sh --full-workspace --offline`: 417 suites, 3,176
  passed, zero failed and five existing ignored campaigns. Clippy with
  warnings denied and formatting pass. Log:
  `.evidence/diagnostics/20261007/sysroot/08ed5e0c-full-workspace.log`, SHA-256
  `f3ce5a249ebd40f1b1a0cab06e51268b1a67847e0ed25c8131ad180561ae701d`.
  The original failure remains retained. This closes the workspace gate for
  header discovery/export/packaging, not P05 or the final stage acceptance.
  No gate process remains running. The storage decision has been requested
  across successive turns and remains unanswered; do not rerun this gate
  while waiting. Header CI, final clean-tree gates and the changelog remain.
  Since the post-cleanup checkpoint, target/frontend plus this capture grew
  by 265,796 KiB. Capture is 4,228 KiB, below 500 MiB; free space is 28 GiB.
  **Owner decision and implementation:** the earlier storage blocker is
  superseded. RED `bdd14b03` detects the missing exclusion field and case
  collisions; RED `321e6d30` detects the absent CI handoff. The exporter
  copies inside the existing Linux container, drops eight uppercase entries,
  then copies 3,966 files to the default macOS volume. The contract records
  the sorted list. No driver lookup, mapping or disk image was added. Five
  local checks pass; the archive check awaits the release replay. The macOS
  Clang-assembled object links and prints the expected text in sealed Linux.
  The workflow now exports on Linux, supplies both archive jobs and checks
  the macOS object on an ARM64 Linux runner; hosted execution is not claimed.
  The excluded-include probe exposes a host-filesystem limit: xt_MARK.h
  resolves to retained xt_mark.h on default macOS, rather than file-not-found.
  Keep the requested reader unchanged and report this result, not a false
  missing-header guarantee. Change budgets: exporter 24/40 lines (118 total),
  tests 16/20, book paragraph 8/10; workflow 36/40 for the whole package.
  **Portable release acceptance:** clean implementation `c054a0fd` passes the
  full workspace gate: 417 suites, 3,176 passed, zero failed, five existing
  ignored campaigns; Clippy with warnings denied and formatting pass. Log:
  `.evidence/diagnostics/20261007/sysroot/c054a0fd-full-workspace.log`, SHA-256
  `0066d7726e57565325f7416f5107ca8ddf195cecfa798cfeefb9fe6e4bc8235f`.
  The actual release archive and its extraction on the default macOS volume
  each pass all six Python checks. The packaged compiler produces an ELF
  AArch64 hello object with no sysroot override. Original zlib 1.3.2 builds
  all 15 archive members without manual include flags; hello, the original
  zlib example and minigzip round trip exit zero in sealed Linux. The explicit
  test linker remains necessary for configure probes, not header discovery.
  Archive SHA-256:
  `f7c4ada71cdd1b9aaa94a83ba2cd9f962b779ce4040d50a194aa9838fd611c51`.
  Contract SHA-256:
  `48ed0e80c56a0ae114a27af9cef8dbb229d487bd2a9021a53786c606e293fed6`.
  Hosted cross-job CI is configured, not executed. No driver header reader
  changed. The only unfulfilled added expectation is file-not-found for an
  excluded uppercase spelling on a case-insensitive filesystem. Keep that
  explicit; this is not a case-sensitive installation restriction or a need
  for a new storage mechanism. The owner wording and final stage closure below
  supersede this checkpoint's remaining contract clarification.
  Removed 186,600 KiB of owned unpacked/build temporary files after checking
  open handles. Since the pre-cleanup measurement, target/frontend plus this
  capture plus those temporary files fell by 109,876 KiB. Capture is 5,008 KiB,
  below 500 MiB; free space is 25.116 GiB. No protected evidence was removed.
  Exit: macOS produces ELF AArch64 objects and zlib without manual include
  flags, contract checks and full workspace gate pass. Add the cross-host CI
  link/run check; do not bundle a linker or runtime. Budget: 250 driver lines,
  120 export-script lines, 60 release-script lines and 40 workflow lines.

**Final product acceptance — complete, October 7:** P01 through P05 are closed.
The owner-approved wording is identical in the book, contract description and
exporter. The final collision change uses 25/40 exporter lines (119/120 total),
16/20 test lines and 10/10 book lines. No include-reader change was added.
Contract SHA-256 after the description-only update:
`e522f1bea58a120f72020fb6d3865d3aa19e1f3d60371a24321fd173cbda0f80`.

Both final gates started on clean commit `d3d583fb`, with exact disk preflights
of 25.052 GiB and 25.543 GiB respectively. Cargo ran serially with two jobs and
the existing target directory. The full workspace gate exits zero: 417 suites,
3,176 passed, zero failed, five existing ignored campaigns; Clippy with warnings
denied and formatting pass. Log:
`.evidence/diagnostics/20261007/sysroot/d3d583fb-final-workspace.log`, SHA-256
`68de4bc280eb18978b46e26f2f94727f4677e20d64c8251c202969d1cd7a32f6`.
The Lean gate exits zero: all six steps pass, including 20 generator tests,
the pinned build, evaluator self-tests, 1,246,159 Lean-side vectors and the
existing theorem/axiom audit. Its log is
`.evidence/diagnostics/20261007/sysroot/d3d583fb-final-lean.log`, SHA-256
`b3f9b3adc710db05df0ee4c174da8363711ea3efe4729713c6c9b29cf1e71759`.
The regenerated receipt binds EvoIR 2.0.1 and a clean Lean tree. This is not a
new Rust/Lean differential campaign or expanded proof scope; Lean sources and
the paused verification pipeline are unchanged. Existing Lean lint warnings
remain visible in the log.

The final tree has 62 UI snapshot triples, 122 explanation pages (2,131 lines,
at most 27 each), 205 tagged examples and six user verbs. Twenty pages retain
their documented Trigger: none exceptions. Original application and packaged
cross-build exits remain bound to their package subjects above. CHANGELOG 0.1.0
records these features and limits. Hosted release CI is configured, not run;
no push or release publication occurred.

After the final Cargo gate and open-handle checks, removed 259 obsolete test
executables, the unused incremental/example caches and 240,620 link-intermediate
object files. Current test executables and release binaries remain. Protected
evidence, contracts, fixtures and the Lean cache remain intact. From the prior
pre-stage checkpoint, target/frontend plus this capture fell by 8,294,540 KiB
(about 7.91 GiB); concurrent work prevents attributing filesystem-wide changes
to this slice. Capture is 5,844 KiB, below its 500 MiB cap. Free space after the
gates is 26.535 GiB. No new target tree, workspace copy or disk image was made.

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

- [x] **P06 — Correct help and early stdin rejection** — `DONE`, P1.
  RED `ad291c6e` reproduces help/status, JSON help and EPIPE failures; fix
  `bca14544` passes all 23 focused tests. Inserted/replaced code, tests and
  book text total 55/60 lines. Help exits 0; no arguments gives the same
  usage and exits 64. The stdin helper tolerates early argument rejection.
  Three consecutive complete evocc-crate runs on clean `bca14544` each pass:
  81 suites, 711 passed, zero failed, one existing ignored test.
  The full offline workspace gate on that same clean subject passes:
  417 suites, 3,178 passed, zero failed, five existing ignored campaigns;
  Clippy with warnings denied and formatting also pass. The gate log hash is
  `f9381460ec277d45eafb12c0ad00676bd985c213e860936f3553cce9a7f428bf`.
  Logs: `bca14544-service-restored-{1,2,3}.log` and
  `bca14544-full-workspace.log` in the capture below. Preserve the interrupted
  run and the first failed complete run (709 passed, two container-service
  failures). After starting the existing service, the sealed GCC/Clang probe
  and all subsequent native tests pass; the consecutive count starts anew.
  Prior-stage limits remain explicit: 20 Trigger: none pages; five separate
  ignored campaigns; hosted release CI configured but not run; no guarantee
  for excluded netfilter names; cross-linking needs an explicit linker;
  the interpreter does not implement arbitrary external libc calls.
  Removed 40 KiB of owned failed-test temporary output after handle checks;
  failed logs remain. From this turn's measured target/capture checkpoint,
  those paths grow by 391,284 KiB; cleanup makes the measured net 391,244 KiB.
  Capture is 660 KiB; free space is 205.72 GiB. Other host work is excluded
  from this accounting. Claim: command defects closed, not repair support.
  Capture cap: 500 MiB at `.evidence/diagnostics/20261007/command-fixes/`.
- [x] **P07 — Expose and apply diagnostic repairs** — `DONE`, P1.
  RED `2c00b3eb`: all 12 public-path tests fail as expected. At that subject,
  the compiler has no repairs field and evo has no fix verb. The tests cover
  the five repair records and byte-exact edits, safe/review separation,
  parse-error refusal, edit order, shared declarations and macro/volatile
  boundaries. Log: `.evidence/diagnostics/20261007/repairs/2c00b3eb-red.log`.
  Implementation `512cb75f` has the five repair records, plan/apply and rollback.
  All 17 repair tests pass, including adjacent declarations, volatile aggregate
  initialization, prototype types and equal-position insertion order. The five
  existing UI cases have repair JSON snapshots; all 55 existing records keep
  their old structural fields. Focused diagnostics/lint tests and Clippy pass.
  Production Rust: 500 inserted/replaced lines; specification: 49 lines.
  The 52 focused diagnostics/lint tests pass, plus all 12 evo command tests.
  On clean `512cb75f`, unchanged cJSON 1.7.19 passes its original Makefile's
  all/test targets. zlib 1.3.2 passes both static and shared original tests;
  all C objects use EvoCC, while the upstream shared link uses system cc.
  These are Makefile replays, not a replacement Unity/ladder campaign.
  The same clean subject passes the full offline workspace gate: 418 suites,
  3,195 passed, zero failed, five existing ignored campaigns; Clippy with
  warnings denied and formatting pass. Log: `512cb75f-full-workspace.log` in
  the capture below; SHA-256:
  `c5578df193c81bdc919f55fd4bad0fe524276657647e373b876df5cf2837c7aa`.
  `512cb75f-commands.txt` records application commands, source and binary hashes.
  Removed the owned 7,708 KiB temporary application tree after handle checks;
  object hashes/sizes and logs remain. Target/frontend plus capture grows by
  454,704 KiB from the recorded repair RED checkpoint; no host-wide attribution.
  Capture is 444 KiB before this closure record; free space is 202.06 GiB.
  An EMFILE failure prevented new shell queries during the gate, not the gate
  itself. Its original handle completed normally; no restart or bypass was used.
  Claim: the five supported source repairs and original build compatibility.
  A syntax recheck is not a behavior proof; review edits still require a human.
  Exit: byte-exact apply tests, parse-error refusal, safe/review separation,
  cJSON and zlib replay, full gate. Budget: 500 production Rust lines and
  60 specification lines. Pin diagnostics revision 1.1.0.
  Capture cap: 500 MiB at `.evidence/diagnostics/20261007/repairs/`.
- [x] **P08 — Add machine-readable user commands** — `DONE`, P1.
  The owner confirms the independent JSON reader/writer limit is 250 lines.
  The 247-line implementation and the existing green functional exits meet it.
  RED `b0aeae9c`: seven command tests fail before JSON support. RED `73fe91ce`:
  both stdio/instruction tests fail before the server and agent files exist.
  RED `dfccc79e` also catches scalar `tools/list` params being accepted; the
  server now rejects them with -32602. The book's exact session is executed.
  All command verbs have JSON output. The five stdio tools delegate to those
  commands. Proof summaries come from the retained record, not English logs;
  output-copy failure stays nonzero even when the proof record is checked.
  All 26 focused tests pass, including the 12 existing command regressions;
  targeted Clippy with warnings denied passes. No Lean, checker, interpreter,
  emitted-code path, dependency, daemon or network service was changed.
  First clean full gate on `cd0eefbc`: 420 suites, 3,208 passed, one style-test
  failure, five existing ignored campaigns. Two new error messages started
  with uppercase JSON; their wording is corrected without changing the rule.
  The corrected full gate on clean `04605cd1` passes: 420 suites, 3,209 passed,
  zero failed, five existing ignored campaigns; target generation, Clippy with
  warnings denied and formatting pass. Keep the first failed log unchanged.
  Final log: `corrected-full-workspace.log` in the capture below; SHA-256:
  `73dbe5933db008aea9e64a66ced580a459df85a9360168c3abdb4742d8847cbd`.
  Implementation: 635 inserted/replaced production Rust lines, tests excluded.
  The JSON module is 247 lines and reuses the existing string quote function.
  AGENTS.md and llms.txt are identical, 53 lines each; the book chapter is 98.
  Capture cap: 500 MiB at `.evidence/diagnostics/20261007/agent-interface/`.
  Final target/frontend plus capture delta from entry is +30,364 KiB;
  capture is 692 KiB, free space 201.28 GiB. Test-owned source/build/proof
  directories were removed. This is not a host-wide disk attribution.
  Claim: the CLI/stdio package meets the restored contract. No new gate is needed
  for a clarified limit; the implementation is unchanged from tested 04605cd1.
  The end-of-stage Lean gate, changelog and six-package report remain pending.
  Exit: verb JSON snapshots, five MCP tools over pipes, agent instructions,
  llms.txt, tested book session and full gate. No network server or dependencies.
  Budget: 700 production Rust lines; JSON reader/writer at most 250 lines;
  AGENTS.md at most 120 lines.
- [x] **P09 — Add editor diagnostics and repair actions** — `DONE`, P1.
  Implemented full-text stdio LSP, 300 ms debounce, compilation-database flags,
  unsaved-source identity, UTF-16 positions and safe versioned edits. The latest
  focused gate passes 44 tests, including the independent pipe client and 13
  VS Code client checks. Shared-header diagnostics remain until all includers
  clear them. Production budget: 698/700 Rust and 73/80 TypeScript lines.
  Clean-tree full gate on `430fa7c0`: 422 suites, 3,216 passed, zero failed,
  five existing ignored campaigns; Clippy and formatting pass. The gate exited
  zero. Log SHA-256: `3dc5fae0695c17261f75a60b0de200f1c591b267403e4647f33f6727bf150513`.
  The owner selected Zed for the native UI exit, then waived the screenshot
  on 2026-10-07. Zed launched evo lsp and displayed an error marker; the UI
  connector returned cgWindowNotFound before capture. No screenshot PASS is
  claimed. Keep the VS Code extension and its tests.
  Logs: `.evidence/diagnostics/20261007/editor-interface/`.
- [x] **P10 — Integrate existing build systems** — `DONE`, P1.
  CMake/compiler-identification REDs are committed in `ca0f7a2a`; library and
  native stack-protector regressions are in `94a9fb29`. Eight focused checks
  pass: compiler identity, warning controls, native CMake, Linux object CMake
  with a space-containing sysroot, pkg-config, Make dependencies and native
  stack protection, plus generated angle-header precedence (`74b92076`).
  Original cJSON 1.7.19 CMake utility tests pass 22/22. The original default
  zlib build first failed on unsupported -coverage. The owner's later decision
  permits a warning for four coverage options, with no instrumentation and no
  test exclusions. RED b223419e and fix ff520ce0 close the option refusal in
  five driver lines. The 20 focused driver/build tests pass. The default zlib
  1.3.2 build and all 16 CTest commands now pass. Its infcover assertions pass;
  Apple's gcov reports missing .gcno but exits 0. The coverage-summary command
  is therefore not evidence of coverage support. No .gcno/.gcda is produced.
  The final clean-tree gate on 95466ea0 passes 423 suites: 3,225 passed,
  zero failed, five existing ignored; Clippy and formatting pass. The corrected
  Darwin snapshots and authorized Linux target replays pass 9/9. Clean `82c9543a` passes the full
  offline gate: 423 suites, 3,224 passed, zero failed, five existing ignored;
  Clippy and formatting pass, exit 0. Log SHA-256:
  `438891e78717057388cfbf24fd42af5c230e2b9c45085b643d80b0b6a05dcbea`.
  The original interrupted and failed logs remain. Budget at that checkpoint:
  22/200 added or replaced Rust lines and 43/80 CMake lines. Removed the owned
  CMake source/build tree after hashes and open-handle checks (15,464 KiB).
  Measured target/frontend plus capture net change: +960,244 KiB; capture
  556 KiB, host free 190.59 GiB. This is not a host-wide disk attribution.
  Evidence: `.evidence/diagnostics/20261007/build-systems/`.
  Final replay: cJSON 22/22 and default zlib 16/16, no filters. The independent
  check confirms all 254 regular zlib source files match the pinned archive.
  `zlib-coverage-options-last-test.log` retains the missing-coverage diagnostic.
  Removed this replay's owned 20,008 KiB source/build tree after hashes and
  no-open-handle checks. The coverage compatibility follow-up changes five
  driver lines within its 20-line budget; total integration Rust is 27/200.
  Exit: original cJSON/zlib CMake tests, Linux object build, pkg-config and
  dependency-file replay, book commands and full gate. Budgets: 200 Rust,
  80 CMake lines. No custom CMake compiler-id module or package manager.
- [x] **P11 — Publish channel metadata and compatibility** — `DONE`, P1.
  RED tests are prepared in `94a9fb29`: eight cases run, the positive packaging
  control passes, and missing matrix/nightly/changelog checks fail as expected.
  Implementation follows the unchanged default zlib replay. The matrix uses
  retained receipts only; channel CI does not imply a hosted release run.
  All eight focused tests pass. The generator is 129/150 lines and includes
  all 15 ladder receipts without changing their status or denominator. The
  nightly caller is 21/60 lines and reuses the native/cross-host release smoke
  checks; the packaging changelog guard adds 8/20 lines. act is absent, so YAML,
  embedded shell and permission/channel paths were reviewed instead. No hosted
  CI or publication was run. Implementation is c8a9be0f. The final clean-tree
  full workspace gate on 95466ea0 passes 3,225 tests with zero failures and
  five existing ignored. All six Lean gate steps also pass on that clean tree.
  Exit: every ladder receipt in the generated chapter, nightly dry run with
  act if present or careful review, changelog guard and full gate. Known
  budgets: 150 Python lines and 60 workflow lines. Do not publish a release.

The six packages are accepted with the stated limits. Both final gates started
on clean 95466ea0; no source changed between them. CHANGELOG 0.1.0 is updated.
Logs in `.evidence/diagnostics/20261007/build-systems/`:
`product-final-full-workspace.log` SHA-256
`d8bbb9a704961a4370a3f40e83bc302cd09adf809d5966476465240655971d5c`;
`product-final-lean.log` SHA-256
`85dc3c3bb00025ade7f5cb023dcb8a5f5db7bf15650eaa81bd74f00fe3d29181`.
The Lean receipt remains byte-identical and records a clean Lean tree with all
six vector groups answered. This does not expand the verified language subset.
Final capture is 1,536 KiB, below its 500 MiB cap. Since this follow-up's entry,
target/frontend plus the capture grew by 2,276 KiB; owned replay intermediates
are gone. This is not host-wide disk attribution. Last free-space check: 188.95 GiB.

- [x] **P12 — Preserve compiler errors when stdin closes early** — `DONE`, P1.
  Implemented in `f3fd04ee`: 45 compiler stdin writes in 45 files now discard
  write errors and retain the existing output assertions. The inventory is
  57 write sites in 53 files: three file writes, six Python-validator writes,
  three already-safe compiler writes and 45 affected compiler writes. No helper
  refactor, compiler change, test filter or assertion change was made.
  Five consecutive `cargo test --offline -p evocc --no-fail-fast` runs each pass
  87 suites: 758 passed, zero failed, one existing ignored. The reported
  bounded GCC tuning test passes 5/5; no failing child stderr was produced.
  The first clean-tree full gate completed tests with 3,223 passed, two failed and
  five existing ignored. Both failures are `container run` reporting
  `Operation not permitted` in the control/extension Linux tests; they are not
  stdin-write panics. That gate exited 101 before Clippy and formatting.
  The first authorized replay attempt could not start because the terminal
  service reported `Too many open files`. Preserve that failure log, SHA-256:
  `b5477b00b5882fc4886fe85ae148d4c361473201fd4c668871da557e5809a966`.
  After terminal recovery, the authorized full offline gate on clean `402fb4f3`
  passes: 423 suites, 3,225 passed, zero failed, five existing ignored; workspace
  Clippy with warnings denied and formatting pass, exit 0. Both previously
  denied Linux tests pass. No code or test selection changed for the replay.
  `full-workspace-authorized.log` SHA-256:
  `c9c30520fc0f1e31983683ef5a288f6dd0b04f0579e997cd0ac0308fbcf78ab4`.
  TODO queue and status-render checks pass. No EMFILE occurred during this
  replay; this is observed recovery, not a claim that its root cause is fixed.
  Capture: `.evidence/diagnostics/20261007/stdin-race/`, cap 500 MiB.
  Final capture is 1,044 KiB. This replay's target/frontend plus capture growth
  is 704 KiB; removed 40 KiB of generated failed-test inputs after hashes and
  no-open-handle checks, for a net 664 KiB. Inputs can be regenerated by the
  unchanged tests. Logs, fixtures and receipts remain; host free is 177.50 GiB.

- [x] **P13 — Record the agent repair loop** — `DONE`, P1.
  Keep the scripted loop separate from the real MCP model session. The focused
  RED fails because `tools/demo/agent-loop.sh` is absent. Require a retained
  checked proof record, an unchanged source fixture, a host run below 30 seconds,
  actual command transcripts and the full offline workspace gate.
  Owner decision, October 7: use the integer-only `magnitude(int a)` absolute-
  value shape in `tools/demo/loop/buggy.c`, not the original division fixture.
  W0001 removes the constant-unused local `int unused = 7;`; W0004 supplies
  the explicit `int` return type. Fix E3004 with the selected `INT_MIN`
  saturation guard: return `2147483647` before evaluating `-a` for
  `a == (-2147483647 - 1)`. Keep the source fixture unchanged by the loop.
  No core-v1, exporter or Lean change is authorized by this demo choice.
  Historical division evidence remains negative: its 70-line loop repaired
  E3002 and the next check passed, but W0002 inserted `(void)unused;`, outside
  core-v1 (exit 6). The integer-only control exported and then failed lowering
  (exit 3); D06 retains that exact program and refusal. Do not report the new
  fixture as a pass for the original. Capture:
  `.evidence/diagnostics/20261007/agent-demo/loop-proof-red.log` and
  `division-lowering-refusal/`. Implementation: `0f459aaa`; script 72/80 lines,
  fixture 10/30 lines, one test. The focused run passes in 19.53 seconds; the
  clean full offline gate passes in `loop-full-workspace.log`: 424 suites,
  3,226 passed, zero failed, five existing ignored; Clippy and formatting pass.
  Log SHA-256: `dcdb1c30278a65c1759bfc9b44d8a1b3d217541b8a38bdee7064600d2c85bb25`.
  The full-gate demo takes 18.38 seconds. `loop-transcript.txt` and
  `model-session.jsonl` retain actual output; the chapter copies it unchanged.
  Claude Code 2.1.292, model `claude-fable-5-1`, performs the real MCP loop on
  October 7: check, explain, safe repair plan, edit, check, prove, run. The proof
  covers `magnitude` and `main`; rewrite remains `NOT_CHECKED`. Its final run
  returns 255, the low byte of INT_MAX. Historical transcripts bind their own
  compiler digest, not the compiler rebuilt by the later gate. No checker or
  proof-profile change. Capture is 544 KiB; measured target/frontend plus capture
  delta from the recorded pre-gate checkpoint is -164 KiB. Protected evidence
  and the small recorded model workspace remain. P14-P16 keep their order.
  Source-checkout follow-up: `c5d23f8d` adds 14 driver lines for nearest-ancestor
  checker discovery and its JSON path. One regression runs with no checker
  override, a restricted PATH and an unrelated working directory. All four
  demo scripts are executable; the refusal script reads the chosen path from
  `evo`, without a second lookup. Both release scripts exit zero under `env -i`
  with only `target/frontend/release` prepended to the original PATH; no EVO
  overrides are set and `evoir-sem` is absent from PATH. The three deliberate
  refusals remain exits 1/3/5. Capture: `discovery-{loop,refusal}-green.txt`.
  The clean full offline gate passes: 425 suites, 3,228 passed, zero failed,
  five existing ignored; Clippy and formatting pass. `discovery-full-workspace.log`
  SHA-256: `1f45bad061ef64a8c78ec6c1b2069015f2da15a091a51c9b7648580529b86509`.
  Focused tests pass 26/26. No core-v1, exporter or Lean change; no new Lean
  gate claim. Capture is 2,424 KiB; measured target/frontend plus capture net
  change is -9,984 KiB from the follow-up's first pre-build measurement.
  Test-owned temporary files are removed; recorded demo proofs remain. Free
  space is 157 GiB. D06 stays OPEN.
- [x] **P14 — Record rejected proposals** — `DONE`, P1.
  `66dd2f5c` adds the 60-line `tools/demo/no-fooling.sh` and the actual chapter
  transcript. Its source is byte-identical to P13's repaired program. One
  certificate byte changes: archive replay exits 1, `BUNDLE_MISMATCH`, before
  semantic replay. One exported `ub.assume.signed_overflow` line is deleted,
  once across all entry callbacks: both source checks accept, then the real
  lowering check for `magnitude` rejects, exit 3. A wrong checker digest uses
  the published raw-driver command and exits 5. No refused assembly or success
  record is published; accepted assembly remains separate. No new driver flag,
  checker rule, core-v1 or export change. The first repeat-mutation attempt is
  retained separately; `refusal-transcript.txt` is the corrected final run.
  Clean full offline gate: 424 suites, 3,226 passed, zero failed, five existing
  ignored; Clippy and formatting pass. `refusal-full-workspace.log` SHA-256:
  `eaa2939f19748840efc62675bfc11481fd84003de49e92416a2375afd461c26b`.
  Capture: `.evidence/diagnostics/20261007/agent-demo/`, 872 KiB, cap 500 MiB.
  This slice's measured target/frontend plus capture net change is +820 KiB;
  retained demo inputs/proofs are small historical evidence. The log records
  its own binary identities; it does not claim a later rebuilt checker replay.
- [x] **P15 — Test deterministic agent results** — `DONE`, P1.
  After P14, compare two runs of each specified JSON output and proof record.
  Normalize temporary paths only. Require three consecutive runs and the gate.
  The 134-line `crates/evocc/tests/determinism.rs` adds one test: failed check,
  safe repair plan, explanation, successful check/run and a real musl-abs proof
  record, each twice. Three consecutive focused runs pass unchanged in 25.13,
  25.30 and 24.60 seconds; formatting passes. Source/checker/compiler hashes,
  stage order, JSON bytes and assembly remain checked. `SOURCE_DATE_EPOCH=0`
  is explicit preprocessing context, not output normalization. No timestamp
  field is removed. Capture: `agent-demo/determinism-{red,1,2,3}.log` under the
  same October 7 evidence directory. Implementation: `95a883d5`. Its clean full
  offline gate passes: 425 suites, 3,227 passed, zero failed, five existing
  ignored; Clippy and formatting pass. `determinism-full-workspace.log` SHA-256:
  `16b3c3761e01b004451d5f37ab3e7777bce159349a2c8210a36dc156a02dabdf`.
  The paired-record test also passes in that gate, in 24.23 seconds. The claim
  covers the fixed fixture matrix and context, not arbitrary clock-dependent
  programs or external inputs. AGENTS.md and llms.txt are byte-identical.
  Capture is 1,212 KiB; this slice's target/frontend plus capture net increase
  is 5,420 KiB. Test-owned temporary sources and proof outputs are removed.
- [x] **P16 — Document a real agent connection** — `DONE`, P1.
  `200b4e73` adds the 37/40-line connection section, the 65/80-line standalone
  skill and `tools/demo/connect-agent.sh`. The skill validator passes. Claude
  Code 2.1.292 accepts the scratch project configuration and reports pending
  approval; the separate P13 session establishes its real connected use.
  Codex 0.160.1 discovers exactly `check`, `explain`, `fix_plan`, `prove`, `run`
  through its actual client catalog, without a model turn. Other servers are
  disabled only in that child process; no persistent client configuration is
  changed. The initial sandbox denial of Codex's state database is retained;
  the authorized retry exits zero. Both final gates start on clean `200b4e73`.
  Full offline workspace: 425 suites, 3,227 passed, zero failed, five existing
  ignored; Clippy and formatting pass. The demo takes 12.41 seconds and the
  paired-record test 24.35 seconds. Lean: all six steps pass, including 20
  generator tests; the retained receipt remains byte-identical and clean.
  Capture: `.evidence/diagnostics/20261007/agent-demo/`, cap 500 MiB.
  `connection-full-workspace.log` SHA-256:
  `2e90506147300d94c0fcc26ab4db5060cc436ab8f0042f6b0f8f8a303bb62bdb`.
  `final-lean.log` SHA-256:
  `7ee6f6d12d77d30ca2ee84738ef5c720b086cfb6adc15e24932f5b66c150f4c7`.
  No proof profile, export, Lean checker, compiler behavior or global client
  setting changes. D06 remains OPEN. Capture is 2,096 KiB; target/frontend plus
  capture grows by 1,200 KiB from the P15 checkpoint. Scratch configuration and
  test-owned outputs are removed; historical proof evidence remains. Host free
  space is 164 GiB. The four demo packages are complete in this bounded scope.

- [x] **P17 — Measure agent repairs with compiler feedback** — `DONE`.
  The owner approved the missing Clang runtime. The resealed image adds only
  `libclang-rt-19-dev`; both compiler hashes and the other 200 packages remain
  unchanged. Both UBSan probes pass. The old image stays at
  `evo-flight-lab:sealed-20260810`; historical receipts are not rewritten.
  The [corpus preflight](tests/contracts/agent-repair-preflight-20261008.json)
  records 30/30 fixed hidden passes, 30/30 buggy hidden failures and 10/10 UB
  findings in each tool. Twenty items derive from pinned public sources;
  ten are explicitly synthetic. Two ineligible drafts were changed before
  registration, with reasons retained. Each hidden driver also checks the
  original entry point after a missing-main candidate exposed a scoring gap.
  The actual c1/c2/c3 feedback paths and neutral scorer pass a model-free smoke
  check. Final Python checks pass 40/40: 32 experiment and eight tracker tests.
  The exact model is `deepseek/deepseek-v4.1-flash`. The owner approved an
  initial USD 5 budget and delegated temperature; the protocol fixes it at 0.
  `tools/experiment/protocol.json` records round, cost and denominator rules.
  `.env` stays ignored; the final receipt and nested transcript payloads pass
  the credential-prefix scan. `agent-repair-corpus-1` is retained with zero model calls: its full-repository
  file query exceeded the capture cap. The bounded query and regression test
  were committed before `agent-repair-corpus-2` at `7eed5eac` and the first request.
  The [final receipt](tests/contracts/agent-repair-20261008.json) records 270/270
  repetitions, 272 requests and USD 0.1982890478. GCC feedback passes 88/90,
  Clang 82/90 and EvoCC 87/90. All successful repairs pass in round one.
  Thirteen output-limit errors remain in the denominator; no failed sample
  was rerun. `stop_reason=completed` and `acceptance=false` are both preserved:
  the study finished, but the harness requires zero trial errors for acceptance.
  The [transcript archive](tests/contracts/agent-repair-transcripts-20261008.tar.gz)
  is 802,549 bytes; the [report](book/src/evaluation-agents.md) includes all
  class and item tables. There is no overall advantage over both baselines.
  The authorized full workspace gate passes 425 suites: 3,228 passed,
  zero failed and five existing ignored; Clippy and formatting pass. It began
  on clean `8f09f06c`; the later registration-query fix changes no Rust or spec.
  The earlier two sandbox container-access failures remain in the log.
  No Lean source changed and no new proof claim is made. Capture cap is 500 MiB
  at `.evidence/diagnostics/20261007/agent-repair-preflight/`.

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

- [x] **P21 — Replicate repair effort and test larger programs** — `DONE`, P1.
  Keep experiment 4 repetition 1 descriptive. Register two new repetitions
  of its unchanged 24 items and 12 larger silent-defect programs. Use the
  same model, prompt, C1/C3 feedback and sandbox. Count a censored session
  as not repaired. Test item-mean effort with an exact one-sided signed-rank
  test; do not select items by the agent's repair outcome.
  The [larger corpus](tests/fixtures/agent-repair-5/README.md) has 12 qualified
  items, 722 lines each, four per E3001/E3010/E3011. All share one adapted
  module bundle and four input workloads. This is not a random-source sample.
  The [qualification](tests/fixtures/agent-repair-5/qualification.json) retains
  all reference runs, line-specific findings and unrelated warnings.
  [Feedback identity](tests/contracts/agent-repair-5-feedback-identity.json)
  passes all 96 raw-byte comparisons, including the fixed-source controls.
  The [four-session pilot](tests/contracts/agent-repair-5-pilot-20261010.json)
  completes 4/4 repairs with zero escapes. Its quota increase is 0.011111;
  the fixed projection is 0.205332, so Part B keeps two repetitions.
  [Registration](tools/experiment/protocol-5.md) fixes the package stop line
  at 0.363758. Tag `agent-repair-experiment-5` is `9672d2d9`. All 96 Part A
  and 48 Part B sessions complete, with zero censoring, unknown outcomes or
  escapes. There are 36 allowed in-session overload notices, no session
  reruns and no provider refusal. Package quota rises from 0.113758 to
  0.288314, an increase of 0.174556. P1-P3 hold; P4 does not: relative
  step savings are 36.62% on A silent and 35.27% on B. The aggregate
  control penalty does not repeat. See the [analysis](tests/contracts/agent-repair-5-20261010.json)
  and [report](book/src/evaluation-agents.md#experiment-5-repair-effort-and-program-size).
  A repairs C1 44/48 and C3 47/48; B repairs 24/24 in both conditions.
  All 127 experiment tests pass. Exact p-values also match an independent
  calculation. Secret scans cover all captures, including four credential
  values, without printing them. Results are committed in `0afe36cc`.
  The full offline workspace gate passed on that clean tree: 425 suites,
  3,239 passed, zero failed, five existing ignored campaigns; Clippy and
  formatting passed. No bypass or new Lean claim is made. Gate log:
  `/private/tmp/evocc-effort-cleanup.a6FU3J/full-workspace.log`, SHA-256
  `2fefc441b1bcec84ebedf2dd33f2e69009c8a77c194ab40f603ce6eec2afdee3`.
  The final 127-test experiment run also passes. Both status render commands
  pass. Frozen inputs and analysis remain identical to the registration tag.
  Capture retention limit is 500 MiB per run, outside the repository.
  Cleanup removed the stopped containers `evocc-asm-edges-20260905`
  (2,186,324 KiB), `evocc-asm-edges-20260905-retry` (2,186,308 KiB) and
  `buildkit` (3,129,952 KiB), with `container rm`. Container-accounted size
  fell from 7,682,646,016 bytes to zero; `df` changed from 38 to 40 GiB free.
  The two old containers ran the retained September 5 asm-goto GCC/Clang
  probe without automatic removal; its receipt and source remain intact.
  Current tool launches use `--rm` or explicit cleanup in `finally`.
  Archived copies removed with `rm -rf` after archive and handle checks:

  | Deleted path (under `/private/tmp/`) | KiB |
  | --- | ---: |
  | `evocc-agent-repair-20261008` | 1,488 |
  | `evocc-silent-pilot-20261009` | 1,368 |
  | `evocc-repair-2-measured-20261009` | 16,196 |
  | `evocc-repair-2-preflight-20261008` | 116 |
  | `evocc-repair-2-preflight-contracts-20261008` | 184 |
  | `evocc-silent-measured-20261009` | 10,056 |
  | `evocc-repair-2-preflight-fresh-20261008` | 4,828 |
  | `evocc-repair-2-preflight-resume-20261008-2057` | 368 |
  | `agent-repair-codecs.s14zOF` (retired source/build scratch) | 1,012 |
  | `claude-501/-Users-onehrxn-dev-30days-c30/f86a2a66-604e-49db-ba1c-7e287919f116/scratchpad/exp3-out/pilot-20261009` | 3,172 |
  | `claude-501/-Users-onehrxn-dev-30days-c30/f86a2a66-604e-49db-ba1c-7e287919f116/scratchpad/exp3-out/repilot-20261009` | 2,372 |

  `rmdir .evidence/b11-zero-runs` removed an empty directory (0 KiB).
  Raw-cleanup `df` readings were 38/38 and 40/40 GiB before/after; active
  qualification containers can change `container system df` during checks.
  Protected image aliases share tracked digests and were not removed.
  All large suggested evidence directories have tracked references and stay.
  Before the final gate, free space was below 50 GiB. With no process using
  the dev tree, `CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 cargo
  clean --offline --target-dir target/frontend --profile dev` removed
  265,080 files (Cargo logical total: 16.5 GiB; prior `du`: 12,616,560 KiB).
  Free space rose from 35 to 46 GiB; container storage stayed at zero.
  The gate rebuilt this one dev tree once. Release binaries kept their hashes.

  After the gate, `rm -rf --` removed only the reviewed paths below, after
  open-handle checks. The common temporary prefix is
  `/private/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/`.
  Each numbered cache path is a separate item, exactly 2,012 KiB. The full
  148-path list and per-path sizes remain in the cleanup ledger.

  | Deleted path | KiB |
  | --- | ---: |
  | `agent-repair-ws-h2kx3pzg/{01..04}/home/Library/Caches` | 4 × 2,012 = 8,048 |
  | `agent-repair-ws-0o9p_tqq/{01..96}/home/Library/Caches` | 96 × 2,012 = 193,152 |
  | `agent-repair-ws-zm18o9a8/{01..48}/home/Library/Caches` | 48 × 2,012 = 96,576 |
  | `evocc-c-corpus-38782-day5` | 1,024 |
  | `evocc-c-corpus-38782-day7` | 528 |
  | `evocc-reference-38782` | 0 |
  | repository `tools/experiment/__pycache__` | 140 |
  | repository `tools/experiment/tests/__pycache__` | 212 |

  This final cleanup removed 299,680 KiB. Free space changed from 35 to
  36 GiB; container storage stayed at zero. All 148 retained final-source
  hashes and the three release executable hashes still match. These caches
  can be rebuilt. Deleted old captures have checked archives in Git; the
  old container runtimes must be recreated from their retained inputs.
  No deletion used Trash.

  Cleanup commands were `container rm evocc-asm-edges-20260905
  evocc-asm-edges-20260905-retry buildkit`, `rm -rf --` with the exact
  reviewed paths in the two tables, the dev-profile Cargo command above,
  and `rmdir .evidence/b11-zero-runs`. The cache loop accepted only the
  three explicit roots and two-digit session directories; it checked that
  each cache was a directory, not a symlink. No cleanup script was added.
  Before/after `df -h .` and `container system df`, deletion lists, handle
  checks and retained-file checks are in `/private/tmp/evocc-effort-cleanup.a6FU3J`.

  Kept because of tracked references or uncertain retention:

  | Retained item | KiB or stated image size |
  | --- | ---: |
  | `.evidence/eqsat-completion` | 3,400,792 KiB |
  | `.evidence/day5-kernel-reconnaissance` | 1,726,688 KiB |
  | `.evidence/full-config-lowering-survey` | 1,486,196 KiB |
  | `.evidence/full-config-parse-survey` | 685,928 KiB |
  | `.evidence/eqsat-pilot` | 753,960 KiB |
  | `.evidence/cpp-conformance` | 10,792 KiB |
  | `.evidence/eqsat-native-20260929-5h9u11` | 300 KiB |
  | `.evidence/harness-checks` | 32 KiB |
  | `.evidence/inline-assembly` | 4,072 KiB |
  | `.evidence/kernel-dashboard-aa2`, `-runtime` | 20 KiB each |
  | `.evidence/kernel-dashboard-alignof`, `-constant-p`, `-expect`, `-retry`, `-retry-absolute` | 28 KiB each |
  | `.evidence/kernel-dashboard-failed`, `-runtime-absolute` | 8 / 40 KiB |
  | `.evidence/layout-attributed`, `.evidence/library-prototypes` | 2,696 / 3,704 KiB |
  | Debian index `29ab26a75d6c...`, variant named by tracked digest | 30,145,116 compressed bytes |
  | `evocc-asm-edges:20260905`, shares the protected sealed digest | 272,180,903 compressed bytes |
  | `/private/tmp/evocc-silent-source.Of31zp` | 92 KiB |
  | `/private/tmp/evocc-silent-gate.EVypk3` | 308 KiB |
  | `/private/tmp/evocc-effort-sources.aXoUcO` | 512 KiB |
  | `/private/tmp/evocc-effort-{pilot,replication,scale}-20261010` | 1,576 / 21,460 / 16,084 KiB |
  | temporary `agent-repair-ws-{h2kx3pzg,0o9p_tqq,zm18o9a8}` after cache removal | 24 / 392 / 200 KiB |

  Older temporary agent roots also stay because their retention is uncertain:
  `agent-repair-ws-n_0ux3xv` (2,680 KiB), `-is_u5geo` (2,024),
  `-sosryqy_` (12,104), `-1njj8gin` (6,056), `-lrql3am4` (12,104),
  `-jffsspvk` (2,680), `-603b9z49` (2,024), `-ldzhxpa1` (2,680),
  and `-fckh8rqc` (2,024). The older unarchived capture also stays at
  `/private/tmp/claude-501/-Users-onehrxn-dev-30days-c30/f86a2a66-604e-49db-ba1c-7e287919f116/scratchpad/exp3-out/smoke`
  (348 KiB). No protected build tree, sysroot, Lean cache or actual Kimi
  home configuration was removed.

  Disk checkpoint: `target/frontend` fell from 13,583,820 to 8,922,524 KiB.
  New corpus, contracts, captures, remaining session roots, source scratch
  and the cleanup ledger total 49,292 KiB. This measured scope therefore
  fell by 4,612,004 KiB. It excludes `.git`, modified documentation, the
  41,160 KiB of retired external copies, deleted containers and unrelated
  host activity. Do not treat a host-wide `df` change as attributable usage.

- [ ] **P22 — Shorten the check feedback of the agent interface; the controls of experiments 4 and 5 show its cost** — `OPEN`, P2.
  Keep separate from the registered effort experiment. Do not change its
  frozen feedback. The title retains the owner's requested follow-up, not
  an established penalty: experiment 5 controls use 60 versus 63 steps and
  1,077,325 versus 1,120,150 tokens in C3 versus C1. The aggregate penalty
  did not repeat. Do not infer a causal feedback-length effect from either
  experiment. No implementation work starts in this package.

- [x] **P20 — Agent repairs of silent undefined behavior** — `DONE`, P1.
  All 24 items qualify: six each of E3001/E3010/E3011 and two each of
  E3002/E3004/E3005. Two pre-registration replacements remain in the
  [qualification receipt](tests/fixtures/agent-repair-4/qualification.json).
  The six-session pilot passes C1 2/3 and C3 3/3; its quota projection is
  0.057864, so all items remain. Tag `agent-repair-experiment-4` fixes clean
  `5b765db0`, C1/C3, prompt-3 and GCC O0/O2 plus Clang O2 scoring.
  The [formal receipt](tests/contracts/agent-repair-4-20261009.json) records
  48/48 completed sessions: each condition passes 18/18 silent items and
  5/6 controls. P1 does not hold (exact McNemar p=1); P2 holds (difference 0).
  Both failures are incorrect `mean-empty` repairs on all three builds.
  There are zero overloads, retries, unknown scores, unattempted cells and
  escapes. The weekly quota rises by 0.035898, below the approved 0.30.
  The [report](book/src/evaluation-agents.md#experiment-4-silent-undefined-behavior)
  retains the matrix, pilot and descriptive cost counts without a general
  repair-advantage claim. All 112 experiment tests and both secret scans pass.
  On clean `2c0153a6`, the offline full-workspace gate passes 425 suites:
  3,239 passed, zero failed and five existing ignored; Clippy and formatting
  pass. The report's paired counts and p-value were independently recomputed.
  All 24 matrix rows match the qualification receipt; copied evidence is
  byte-identical. TODO rendering and the status checkpoint checks pass.
  Gate log: `/private/tmp/evocc-silent-gate.EVypk3/full-workspace.log`, SHA-256
  `9b9e4b9043365c3fc5c0fcaf632f889a927af0dbf6297c342ac8d19e7ed02448`.
  No Rust or Lean semantic source changed; the Lean gate is not part of this
  package's exit. No additional model requests or frozen-input changes remain.
  Retained pilot/formal captures use 1,368/10,056 KiB, below 500 MiB each.
  Removed 108,880 KiB of completed-session tools/caches after an open-handle
  check, plus 368 KiB of copied sources and generated scratch views. Final
  source hashes and all evidence remain. In the closing interval, the measured
  target and scratch/capture paths decrease by 108,200 KiB: target/frontend
  grows 724 KiB and the gate capture adds 324 KiB. This is not a whole-host
  disk attribution. No new target tree or runtime image was created.

- [x] **P19 — Consistent options and checkout defaults** — `DONE`, P1.
  D07 supplies the four fixes and its retained full-workspace acceptance.
  The broader option-order requirement also exposed `evo prove -o OUT.s FILE.c`:
  the public regression failed with `unknown prove option: .../out.s` before
  implementation. The parser now accepts one file before, between or after
  its existing option pairs. Twelve JSON command tests pass, including six
  prove permutations, ten invalid-argument controls, and both JSON orders for
  run/check/explain/build. No proof pipeline or sysroot semantics changed.
  On clean `9bb1bdbd`, the complete offline workspace gate passes 425 suites:
  3,239 passed, zero failed, five existing ignored; Clippy and formatting pass.
  The report and tracker Python checks pass 11 tests. No Lean source changes.
  The prerequisite production change is 51 net Rust lines including D07;
  this final parser change adds five net lines. The P17 addendum contains only
  the requested sentence in each report; all prior report bytes are unchanged.
  At gate completion, target/frontend plus capture grew 63,860 KiB from the
  preceding closure measurement; the capture is 324 KiB, with no new target tree.
  Capture cap: 500 MiB at
  `.evidence/diagnostics/20261008/repair-experiment-prerequisites/`.

- [x] **P18 — Measure repair cost, edit precision and safe automation** — `DONE`.
  Extend the existing experiment, not its frozen result: 30 composed C items
  of 150–400 lines, each with interacting UB and another defect, at least ten
  hidden inputs, minimal fixes and original-coordinate fix spans. Qualify
  originals, fixes, UB findings and three C0 preflight responses before a new
  registration. Compare C0–C4 with DeepSeek V4.1 Flash and the delegated small
  model choice, `qwen/qwen3.5-9b`. Freeze five
  predictions, common prompts, retry/cost rules and the 16,384-token response
  limit before measurement. Report response errors separately, token cost,
  diff size, span overlap and safe auto-repairs; publish negative results too.
  The owner approved `agent-repair-experiment-2`: the old
  `agent-repair-corpus-2` remains at P17's `7eed5eac`. The owner also confirmed
  30 items × five conditions × two models × three repetitions × three rounds:
  at most 2,700 normal requests, with one retry charged separately. Report
  repair rate excluding response errors and completion rate over all attempts.
  Tool qualification: [receipt](tests/contracts/agent-repair-2-tool-preflight-20261008.json)
  records 30 original hidden failures, 30 fixed passes, 60 single-repair failures,
  and 30 UB diagnoses each from EvoCC, GCC and Clang. All fixed executions
  complete. The CRC original's sanitizer and hidden timeouts remain failures.
  The corpus has 20 public-source adaptations and ten marked synthetic items.
  Commit `1e98993f` adds 115 harness lines and 58 report lines; 56 mock harness
  and report tests pass. None of these results establishes model difficulty.
  The first C0 preflight at `a03a4f50` was stopped after source review found
  that the sorting item's hidden stable-order requirement was absent from the
  model-visible program. No score completed. The first response had no final
  C file and exhausted 16,384 tokens (reported cost USD 0.00309618); the
  interrupted identical retry has unknown per-request usage. Preserve the
  [receipt](tests/contracts/agent-repair-2-preflight-interrupted-20261008.json)
  and [transcript](tests/contracts/agent-repair-2-preflight-interrupted-20261008.tar.gz).
  This is neither model difficulty evidence nor a measured repair result.
  Model-visible contracts are clarified in 17 items without changing any
  algorithm suffix, hidden input or expected output. Both tool-qualification
  tests pass again (60 host checks, 120 native variants); 59 harness/report/
  secret tests pass. No compiler or proof semantics change.
  The provider's read-only key-usage query at 2026-10-08T10:56:04Z reports
  USD 0.005037825 for the day (USD 0.203326857 lifetime); these aggregates do
  not identify the retry's exact charge. Reserve USD 1 of the total preflight
  allowance for this attempt; a new draft's cap is at most USD 11, not USD 12.
  The second preflight at `3bfb68c5` stopped with `cost_unknown:transport_error`:
  four of 180 slots attempted, two scored (one pass, one failure), two response
  errors, six HTTP attempts, USD 0.03732025 reported cost. The other 176 slots
  were not attempted. One error exhausted the output budget twice; the other
  had two transport failures with unknown per-request usage. Preserve its
  [receipt](tests/contracts/agent-repair-2-preflight-transport-20261008.json)
  and [transcripts](tests/contracts/agent-repair-2-preflight-transport-20261008.tar.gz).
  A read-only usage query at 2026-10-08T12:04:16Z reports USD 0.049194125 for
  the day and USD 0.247483157 lifetime; it does not identify individual charges.
  Restore the owner's original preflight rule: two real scored failures out
  of three slots. An error in the third slot is not a failure. The earlier
  all-three-valid restriction was added by the implementation, not the owner.
  Do not rerun attempted slots. The owner approved a +150-line harness limit
  for prefix-only continuation. Resume only the unattempted suffix into a new
  directory, preserving all prior outcomes, transcripts, provenance and cost.
  The transport/admission fixes pass 62 harness, report and secret tests.
  The harness is 411 lines, exactly 120 above its original baseline. The full
  offline workspace gate started on clean `3bfb68c5`: 425 suites, 3,239 passed,
  zero failed, five existing ignores; Clippy and formatting pass. No Rust
  source changed during that gate. This does not complete model qualification.
  The read-only usage query at 2026-10-08T12:54:09Z is unchanged from 12:04:16Z.
  Keep the USD 1 reserve for prior unitemized charges and carry the current
  campaign's reported cost against its USD 11 cap; neither amount resets.
  Prefix-only continuation passes six new regressions (baseline lacks the API
  and is RED); all 68 harness, report and secret tests pass. The script is
  438 lines, a net increase of 147. No Rust, model, prompt or corpus changed.
  Specify the span coordinates, mixed-defect classes, preflight eligibility,
  prediction comparisons and dry-run cost cap before the new tag.
  The required P17 ceiling-effect sentence is added to README RQ6 and the book
  without changing any previous result. Exit still requires the qualified
  corpus, tagged run or cost stop, secret-checked receipt/transcripts, all five
  prediction outcomes, report, changelog and full gate. Budgets: +150 harness
  lines (owner-approved continuation exception) and +80 report lines.
  This is not a completion or repair-benefit claim.
  Run record (2026-10-09). Commit `55214a6e` fixes the 14-minute stall:
  urllib's 120 s timeout limits each socket read, and OpenRouter's keep-alive
  whitespace kept the read alive. Each HTTP attempt now has a 300 s total
  deadline. Unknown usage adds a USD 0.05 reserve instead of stopping.
  `--parallel N` starts slots in registered order under one lock. The harness
  change is +87/-34 lines and the report change +76/-30; 78 harness, report
  and secret tests pass. The fresh preflight ran 180/180 slots for USD
  2.2509958723 reported, with 78 unknown-usage attempts. DeepSeek has 1 hard
  and 29 easy items; Qwen has 8 hard and 22 easy. More than six items are easy
  for the stronger model, so the corpus stays unchanged and the predictions
  use each model's hard items
  ([preflight receipt](tests/contracts/agent-repair-2-preflight.json)).
  The tag `agent-repair-experiment-2` is `0701fea5`. The measured run started at
  2026-10-08T16:17:48Z with six parallel slots and the USD 160 cap. The key's
  own USD 10 limit stopped it at slot 443: OpenRouter answered HTTP 403 "Key
  limit exceeded". The tagged harness stops only on 401 and 402, so the raw
  [receipt](tests/contracts/agent-repair-2-20261009.json) shows 900 rows,
  914 refused attempts and `stop_reason=completed`. The report applies the
  registered key-refusal stop: 444 repetitions attempted and 456 unattempted,
  1,080 HTTP attempts, USD 5.8110284793 reported and 188 unknown-usage
  attempts. Outcomes: 144 passed, 23 failed, 275 response errors, 2 stopped.
  For both models, all five predictions did not hold: the population is
  incomplete, with further ties and missing C0/C1 successes. Two transcript
  archives are below 10 MiB; the secret scans pass. The
  [report](book/src/evaluation-agents.md#experiment-2), README RQ6 and the
  changelog record the result. Not done: the harness does not stop on the
  HTTP 403 key refusal, and the unattempted repetitions cannot be continued
  under this registration.
  The full offline workspace gate on clean `249e522d` passes 425 suites:
  3,239 passed, zero failed, five existing ignored; Clippy and formatting
  pass. No Rust source changed. Capture:
  `.evidence/diagnostics/20261009/repair-experiment-2/full-workspace-249e522d.log`.

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

- **K02/K03:** the current pair times out at 7/10 reference versus 3/10 subject;
  the retained smaller trace has 87 matching fault-handler entries and zero
  dropped events, but cost is distributed. Inspect that trace and emitted hot
  functions before another speculative peephole or 7,008-object rebuild. Keep
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

  - [x] **K01a — Fail closed on truncated sched_ext result summaries**.
    [Minimal fixture](tests/fixtures/kselftest-scheduler-truncated.tap) reproduced
    seven false-valid assertions. The parser now retains malformed/missing/empty
    summary errors through program accounting; outer TAP results are not rewritten.
    All 219 K1 Python regressions and targeted Ruff/diff checks pass. This closes the verdict subcase,
    **not** the kernel NULL fault or the fresh paired run.

  - [ ] **K01b — Resolve the concurrent instruction-publication runtime discrepancy** — `OPEN`, P1.
    The [userspace C reduction](tests/fixtures/instruction-publish-alias.c)
    removes kernel static keys, RCU, hotplug and EvoCC-generated test code.
    The first MOV probe exceeded Arm's concurrent-modification boundary; its
    evidence is explicitly qualified, not erased. The corrected direct-B probe
    still fails all six concurrent modes on both kernels under MT-TCG, including
    aliases and reader ISB. All six same-protocol memory-gate, locked-MOV and
    handshake controls pass; all 12 modes pass on HVF and diagnostic ST-TCG.
    First bad snapshots contain a branch to return zero but observe return one.
    This is a concrete shared runtime boundary, not yet a
    localized QEMU fix or proof of the original sched_ext cause. Diagnose the
    publication/invalidation path, then replay the unchanged original. Do not
    silently change the locked lane or waive the failure. [Full matrix](tests/fixtures/instruction-publish-alias.out).
    A [read-only target observer](tests/fixtures/instruction-publication-observer.out)
    now sees actual entry into the immutable return-one path after the disabling
    code/data stores. Its **196224/320384** witnesses exactly match guest misses
    on GCC/EvoCC MT-TCG; both ST-TCG controls pass 6/6 with zero witnesses.
    Instruction-data API samples reread memory and are not decoded-opcode proof;
    the target witness avoids that inference. This localizes the reduced wrong
    control flow, not its exact invalidation race or the original NULL fault.
    The store-window extension correlates **25792/8448** wrong-target executions
    with translations observed during a pending patch store; both ST controls
    still pass. It does not expose the internal decode/invalidation ordering,
    so that observer alone was insufficient. The [matching-version internal
    experiment](tests/fixtures/qemu-instruction-publication.out) now captures
    actual old-instruction fetches after invalidation. A private, address-independent
    four-byte-store intervention makes all six userspace and three static-key
    cases pass on both Images. It is not a general emulator fix. Both baseline
    and intervention pass the latest original hotplug/reload sequence, so that
    intermittent NULL fault is not yet causally closed. Production QEMU and
    full-run acceptance remain unchanged.

    September 28's [cache-maintenance experiment](tests/fixtures/qemu-cache-maintenance.out)
    removes the store-width/address intervention and invalidates physical RAM
    lines at guest IC IVAU. Both Images pass 6/6 userspace and 3/3 static-key
    cases; removing only invalidation restores three failures per Image despite
    retaining the helper, lookup, barrier and block boundary. This is still a
    private, incomplete cache-maintenance implementation, not a production
    runtime change. The complete unchanged 23-test sched_ext program passes on
    GCC but times out on the retained old EvoCC Image with BOTH pristine and
    candidate QEMU; neither pair reproduces the NULL fault. New-composition
    runtime validation and the remaining emulator paths are still required.

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

  - [x] **K02a — Remove redundant negated-truth materialization**, 2026-09-23.
    [Minimal C case](tests/fixtures/negated-truth-branch.c) went RED → GREEN;
    both interpreters and native GCC/Clang/EvoCC controls pass at O0/O2.
    The original `virt/kvm/kvm_main.o` reassembles with identical global symbols,
    text 243232 → 237968 bytes on controlled prior/current builds. The 1094-test
    affected gate, clippy and fmt pass. No Image was changed; K02 still needs its
    unchanged full-mode runtime replay after final rebuild. [Record](tests/fixtures/negated-truth-branch.out).

  - [x] **K02b — Preserve assembly operands while removing trivial transport**, 2026-09-26.
    Ordinary validated-SSA assembly now admits direct-edge identity forwarding;
    fixed-register, asm-goto and indirect-dispatch boundaries remain excluded.
    The [C reduction](tests/fixtures/asm-value-transport.c) also exposed an
    independent allocator bug: one value bound to two separate fixed input
    registers was rejected. Distinct operand copies repair it without relaxing
    genuine tied-operand conflicts. Nine MIR tests, the CLI regression, six
    native O0/O2 controls and the 1001-test changed gate pass. Both original KVM
    objects assemble with identical exports; this alone is not a runtime PASS.
    [Evidence and exact scope](tests/fixtures/asm-value-transport.out).

  - [x] **K02c — Select bounded constant memory displacements**, 2026-09-26.
    The [minimal C](tests/fixtures/memory-address-selection.c) first failed its
    offset-shape assertion without compilation errors. Same-block constant
    pointer offsets now use existing typed load/store immediates; volatility,
    access width/alignment, guards and effect boundaries remain unchanged.
    Seven Rust tests, 48 assembler checks, 12 native controls and the 1008-test
    affected gate pass, including Clippy/fmt. Original KVM objects retain their
    exports; text shrinks 187376 → 176264 and 37328 → 34996 bytes. Nothing is
    installed and no timing claim is inferred. K02/K03 remain OPEN.
    [Exact evidence](tests/fixtures/memory-address-selection.out).

  - [x] **K02d — Avoid duplicate automatic-zero value stores**, 2026-09-26.
    The [small C case](tests/fixtures/automatic-zero-single-pass.c) covers fixed
    and dynamic records, pointer provenance, padding and admitted scalar/vector
    types. After representation zeroing, the shared initializer only repairs
    typed Null pointers; explicit initializers and rejection boundaries remain.
    34 CLI tests, 12 native controls and the 1121-test affected gate pass.
    Original new-config KVM objects retain their exports; text decreases by
    4028/360 bytes. They are not installed, and the running frozen-release survey
    does not include this change. K02/K03 and final-composition replay stay OPEN.
    [Exact evidence](tests/fixtures/automatic-zero-single-pass.out).

  - [x] **K02e — Use either adjacent conditional successor as fall-through**, 2026-09-26.
    Two MIR shape assertions first fail without compilation errors. Final
    emission now inverts the condition when the taken block is physically next;
    logical edges, argument copies, labels and relaxation remain unchanged.
    Three MIR regressions, the CLI/interpreter reduction, 14 sealed native and
    mixed-link controls and the 1017-test affected gate pass (Clippy/fmt green).
    Original KVM objects retain exports; redundant adjacent branches fall from
    1101/219 to zero and text shrinks by 4444/832 bytes. No objects are installed;
    the running frozen survey excludes this change. K02/K03 remain OPEN.
    [Source and evidence](tests/fixtures/conditional-fallthrough.out).

  - [x] **K02f — Select bounded constant shift immediates**, 2026-09-26.
    The [C reduction](tests/fixtures/constant-shift-selection.c) and MIR shape
    assertion go RED → GREEN. Count decoding keeps its own width; source guards,
    narrow-value extensions, dynamic counts and pair operations are unchanged.
    Four MIR tests, the CLI/interpreter fixture, 60 assembler checks, 14 sealed
    native/mixed-link controls and the 1022-test affected gate pass; Clippy/fmt
    pass. Two original KVM objects retain exports; 87/25 shifts become immediate,
    but text changes by only 0/-56 bytes. Nothing is installed; no runtime
    speedup is claimed and the frozen full build excludes this repair.
    K02/K03 stay OPEN. [Exact boundaries and results](tests/fixtures/constant-shift-selection.out).

  - [x] **K02g — Select bounded constant add/sub immediates**, 2026-09-26.
    The [small C case](tests/fixtures/constant-addend-selection.c) and MIR shape
    assertions go RED → GREEN. Full-register constants in [0,4095] use existing
    immediate forms; only ADD commutes evaluated operands. Overflow guards and
    all nonmatching paths remain. Four MIR tests, one CLI test, 48 assembler
    encodings, 14 native/mixed-link controls and the 1027-test affected gate pass;
    Clippy/fmt pass. The spill regression still forces real spills. Original KVM
    objects keep exports and shrink by 732/128 text bytes. No objects installed,
    no runtime speedup claim; the frozen full build excludes this repair.
    K02/K03 stay OPEN. [Exact evidence](tests/fixtures/constant-addend-selection.out).

  - [x] **K02h — Select and validate full-width logical immediates**, 2026-09-28.
    Initial REDs expose missed mask selection and acceptance of unencodable
    MIR operands; a second RED catches complement-expression constant facts.
    The shared recognizer covers 6,636 encodings and 131,072 random controls;
    narrow/pair and nonmatching operations retain their register paths.
    Both interpreters/MIR, 240 assembler shape/target combinations, fourteen
    native/cross-compiler controls and both original KVM object assemblies pass.
    The local affected gate has 1,077 PASS, zero failures, four historical
    ignores; Clippy/fmt pass. The active kernel's frozen compiler excludes this
    repair. No objects were installed; K02/K03 and final pinned T05 remain OPEN.
    [Exact scope, debug timeout and identities](tests/fixtures/logical-immediate-selection.out).

  - [x] **K02i — Select and validate bounded immediate comparisons**, 2026-09-28.
    Shape and malformed-imm12 REDs now pass. Full-width comparisons preserve
    reversed relations, flags adjacency, source effects and unmatched paths.
    Six MIR/CLI tests cover 96 shapes, 1,056 interpreter comparisons and 192
    assembler combinations; fourteen sealed native/crossed controls pass.
    Original KVM objects retain their exports and shrink by 4,048/884 text bytes.
    The pinned Rust 1.97.1 affected gate has 1,083 PASS, zero FAIL and four
    historical ignores, with Clippy/fmt PASS. No objects are installed and no
    runtime speedup is inferred; K02/K03 remain OPEN.
    [Exact result and retained failed attempts](tests/fixtures/constant-compare-selection.out).

  - [x] **K02j — Select direct-null tests and repair MIR null transport**, 2026-09-28.
    The [minimal C case](tests/fixtures/null-compare-selection.c) exposes both
    redundant pointer comparison materialization and an existing interpreter
    E3013 on typed null stores. One shared conversion now handles null for
    ordinary/atomic stores and admitted pointer exchange/CAS; nonzero raw
    provenance and ordinary integer-store fragments remain rejected. Ten focused
    tests, 40 assembler combinations and fourteen native/crossed executions
    pass. The final five-crate gate has 1,093 PASS, zero FAIL, four historical
    ignores; Clippy/fmt pass. Both original KVM objects preserve symbols, with
    text reductions of 1,024/112 bytes. No object is installed; the existing
    7,008-object kernel does not include this change and K02/K03 stay OPEN.
    [Exact scope and identities](tests/fixtures/null-compare-selection.out).

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

  - [x] **K03a — Substitute bounded private leaf helpers**, 2026-09-23.
    The [small C case](tests/fixtures/private-leaf-substitution.c) goes RED → GREEN
    through the existing C-HIR inliner, with declaration-chain barriers and
    explicit cost/lifetime/address boundaries. Interpreter/native controls and
    the 1,812-test affected gate pass. The original page-table object assembles
    with identical exports and three fewer helper calls, but text grows by 128
    bytes; no runtime improvement is inferred. No Image changed: K03 remains
    OPEN. [Full result](tests/fixtures/private-leaf-substitution.out).

  - [x] **K03b — Inline a single-site visitor without expanding runtime recursion**, 2026-09-27.
    Read-only instruction counters independently match both KVM reductions'
    ftrace entry counts and expose repeated out-of-line visitor overhead.
    The [C reduction](tests/fixtures/recursive-inline-visitor.c) goes RED → GREEN:
    the visitor expands into its out-of-line walker; the recursive call remains.
    Existing object identity/lifetime/effect rules and explicit size/activation
    boundaries are preserved. The repair also honors noinline/noipa/noclone on
    the complete declaration chain for every optional substitution path.
    Nine CLI regressions, six sealed native controls and the 1868-test affected
    gate pass (Clippy/fmt green). The exact original
    page-table object retains all exports, removes the visit call/definition and
    changes text 33068 → 32964 bytes. No runtime speedup is inferred. K02/K03
    still require the unchanged original tests on the rebuilt composition.
    [Repair evidence](tests/fixtures/recursive-inline-visitor.out),
    [instruction-count limits](tests/fixtures/kvm-instruction-count.out).

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

  - [x] **K04b — Isolate ublk completion-order assertions** — `DONE`, P1, 2026-09-28, classification only.
    Newly exposed generic_01 fails on both kernels; generic_02 fails on GCC
    and passes on EvoCC. Both use the frozen fio/trace completion-order check.
    The [small AIO control](tests/fixtures/ublk-aio-completion-order.c) passes
    four checks and 65,536 exact completions per Image, but its bounded batches
    do not reproduce reordering. Compare continuous submission and the trace
    observer before attributing the failure; the general block-layer absence
    of an ordering guarantee alone is not a diagnosis of this test.
    [Results and limits](tests/fixtures/ublk-aio-completion-order.out).
    The [continuous-refill follow-up](tests/fixtures/ublk-aio-refill-order.out)
    also passes 4/4 cases and 65,536 completions per Image, without reordering.
    Batch replenishment alone does not explain the missing reproduction;
    fio submission details and the trace observer still need isolation.
    The [independent event observer](tests/fixtures/ublk-trace-order.out) then
    reproduces actual cross-CPU completion reordering with original fio and
    bpftrace inputs: all 131,072 requests are issued sequentially and every
    sector completes exactly once, with zero trace loss. The installed default
    is two queues, not one. One-queue and pinned-submitter controls have zero
    order breaks on both Images. Unchanged original generic_01 still fails on
    both; generic_02 is GCC PASS/EvoCC FAIL this time. This closes the shared
    strict-order assumption's classification, not those originals as PASS,
    the stress/shutdown problem K04 or final K13. No acceptance input is pinned
    or rewritten; previous negative observations remain intact.

  - [x] **K04a — Preserve ublk worker failures beneath a successful wrapper**, 2026-09-23.
    The [small shell reduction](tests/fixtures/ublk-background-exit.sh) and
    fault injection into the unchanged upstream script reproduce parent exit 0
    after three child failures. The parser now retains matching inner FAIL/SKIP
    and fatal helper diagnostics, rejecting missing/malformed summaries without
    mistaking expected device I/O errors for failures. All 238 K1 and 240 ladder
    tests pass. Both original diagnostic flights pass and shut down, but the old
    stall is not reproduced or explained: **K04 remains OPEN**.
    [Scope, hashes and replay](tests/fixtures/ublk-background-exit.out).

- [x] **K05 — PM runtime KUnit pending-resume race** — `DONE`, P1, 2026-09-23.
  The [signed-module reduction](tests/fixtures/pm-runtime-barrier-completion.c)
  selects the completed-worker ordering with `flush_work`: barrier returns zero
  in all 32 such cycles on both kernels, as the frozen API documents. Both also
  pass all 64 active/suspend checks. The unchanged original replay remains GCC
  PASS / EvoCC FAIL at its unsupported assumption that resume is still pending.
  This closes **classification as a timing-sensitive upstream assertion**, not
  that original as PASS or a compiler repair. [Paired record](tests/fixtures/pm-runtime-barrier-completion.out).
  K14's fresh complete execution remains open.

- [x] **K06 — Build and run the IOPF-enabled configuration** — `DONE`, 2026-09-28.
  September 26: the reviewed `CONFIG_ARM_SMMU_V3_SVA=y` configuration builds
  and boots on GCC. The unchanged iommufd ELF improves **218 PASS / 6 FAIL /
  5 SKIP → 224 PASS / 0 FAIL / 5 SKIP**, under the original 45-second limit;
  the five skips explicitly exclude the upstream `no_viommu` variant.
  Three new IOPF objects also compile/assemble with EvoCC and identical exports.
  The corrected inventory is **7,002 target-C / 847 modules** plus 37 separate
  HOSTCC objects. The first common-bench launch rejected the mixed-role inventory
  before compilation (T04b); the following attempt encountered bitmap assertions
  and 13 GCC object replacements. After those repairs, uniform run
  `20260927T135325Z` completes all 7,002 actual-header compilations, stable
  generated inputs, link, zero replacement and boot. Both new Images then pass
  the unchanged original executable: **224 PASS / 0 FAIL / 5 SKIP**, exit 0,
  45-second limit unchanged, all 229 cases accounted. The five upstream
  `no_viommu` skips remain SKIP. This closes this configuration/runtime gap,
  not K13/K14 or untested IOMMU hardware.
  [Identities and scope](tests/fixtures/iopf-enabled-reference.out), [run ledger][kernel].

- [ ] **K07 — Define and implement nonliteral strlen ICE admission** — `NEEDS_RULE`, P1.
  Filtered FORTIFY is 25/26 PASS with two assertions left; current §10.1.18 grants
  ICE only to literal operands. Use the existing [19-line fixture][strlen] and
  [oracle record][strlen-out] to define a bounded static-constant-object rule,
  preserving ordinary-call effects and negative controls. Close with reviewed
  wording, shared classification tests and the unchanged 26-case kernel suite;
  K14 remains separate. [Decision request][semantics].

- [x] **K08 — Disposition the seven shared EL1 breakpoint failures** — `DONE`, P1, 2026-09-23.
  All seven cases are mapped to their first task-bound kernel-address allocation.
  A [real kernel-API reduction](tests/fixtures/hw-breakpoint-kernel-task.c) verifies
  CPU-wide EL1 success and both per-task forms returning `-EINVAL` on GCC/EvoCC
  kernels; its signed diagnostic module actually loads and unloads. This closes
  **classification as an ARM64 API/test mismatch**, not those seven tests as PASS.
  The two CPU-wide original controls pass; no assertion/config was changed.
  [Paired measurement](tests/fixtures/kernel-api-contracts.out), [case table][kernel].

- [x] **K09 — Disposition ntsync wake_all's return-value expectation** — `DONE`, P1, 2026-09-23.
  Frozen documentation/implementation and the maintainer's upstream correction
  agree: `CREATE_EVENT` returns a descriptor, not zero. The [small real ioctl
  test](tests/fixtures/ntsync-create-event.c) passes both kernels, including state,
  close, EFAULT and EBADF controls. This is an **upstream assertion defect**, not
  a compiler defect or a waived PASS: the unchanged original remains 10/11.
  [Paired measurement](tests/fixtures/kernel-api-contracts.out), [source evidence][kernel].

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

  - [x] **K10a — Reject incomplete BPF summaries and label selected replays**.
    A [small TAP fixture](tests/fixtures/kselftest-bpf-truncated.tap) and malformed,
    missing, duplicate and empty variants expose false-valid output. Complete
    counters now survive program accounting; upstream error-log replays are not
    doubled. The renderer distinguishes a one-program replay from all 970
    programs. 234 K1/240 ladder tests pass; actual serial captures still reparse
    as FAIL/TIMEOUT. No original failure is promoted to PASS.

  - [x] **K10b — Pair the known PMU/lock/UDP API reductions**, 2026-09-23.
    The signed [kernel probe](tests/fixtures/bpf-runtime-contract.c) confirms
    hardware-event ENOENT, working software counters and recursive-lock EDEADLK
    on both Images. The expanded [UDP test](tests/fixtures/bpf-udp-sockmap-state.c)
    confirms the bind prerequisite for both address families and both map types.
    Both kernels pass all five checks and unload the module. This extends earlier
    source/reference diagnosis, not a newly repaired compiler or full BPF PASS.

  - [x] **K10c — Diagnose consecutive stack build-id helper results**, 2026-09-23.
    The [raw-BPF reduction](tests/fixtures/bpf-stack-buildid-order.c) confirms
    on both Images that the first helper converts seven frames while the second
    falls back to seven IPs; reversing helper order reverses the representations.
    Frozen `mmap_unlock_work.h` explicitly allows this when IRQ-disabled lookup
    has already queued its deferred unlock. The unchanged original still fails
    its byte-equality assertion on both kernels. This closes **classification
    of an invalid test assumption**, not that original as PASS. [Record](tests/fixtures/bpf-runtime-ordering.out).

  - [x] **K10d — Diagnose the timer callback's premature stream read**, 2026-09-23.
    A [small observer](tests/fixtures/bpf-stream-callback-completion.c) uses the
    retained original BPF skeleton. On GCC its initial read returns zero and
    a later read returns the fault record; EvoCC already has the record at the
    first read. Both synchronous controls pass. The original selected group
    now passes all eight subtests on both sides, while its earlier full-run
    failure remains recorded. This is **asynchronous-completion classification**,
    not a timeout relaxation or compiler fix. [Paired source-bound result](tests/fixtures/bpf-runtime-ordering.out).

- [ ] **K11 — Finish case-level triage of all other kselftest outcomes** — `OPEN`, P1.
  Shared FAIL/TIMEOUT/INVALID_OUTPUT/INCOMPLETE and SKIP results are not all
  explained. Map each to a source/config/runtime/compiler cause, extract a small
  reproducer per new class and assign a child ID. Close when every non-PASS has
  evidence and an action or justified limit; no blanket “both fail” exemption.
  Use the [full receipt][selftest-receipt] and [ledger][kernel].

  - [x] **K11a — Restore /tmp's mountpoint prerequisite on ext4**, 2026-09-24.
    The [minimal C case](tests/fixtures/selftest-tmp-mount.c) and unchanged
    `openat2:resolve_test` reproduce EINVAL on both Images before all 88 cases.
    The common init now self-binds /tmp, preserving disk-backed objects and
    failing closed on setup errors. The original passes 88/88 on both kernels;
    three unchanged mount controls pass another 152/152 each. All eight flights
    shut down; disk hashes remain unchanged. 239 K1 tests and Ruff pass. This
    closes one runtime-environment failure, not K11/K13's full population.
    [Identity-bound record](tests/fixtures/selftest-tmp-mount.out).

  - [x] **K11b — Verify harness constructor-order failure**, 2026-09-24.
    The [minimal C case](tests/fixtures/selftest-constructor-order.c) reproduces
    priority-20000 registration before the default-priority direction detector.
    All six native compiler/optimization pairs produce the reversed list;
    moving only the detector earlier fixes the causal control. Both interpreters
    agree, and the 506-test changed gate passes. The unchanged original still
    fails only its ordering diff on both kernels, with normal shutdown. This
    closes **upstream-assumption classification**, not that test as PASS.
    [Paired record](tests/fixtures/selftest-constructor-order.out).

  - [x] **K11c — Supply udmabuf's actual HugeTLB requirement**, 2026-09-24.
    The [minimal C case](tests/fixtures/selftest-hugetlb-reservation.c) keeps the
    original 2 GiB mapping and proves ENOMEM with one reserved page versus
    successful access to all 1024 pages on both kernels. The shared runtime
    now selects an explicit 4 GiB machine and 1024-page pool for this program
    only; locked K6, original binary and 45-second timeout stay unchanged.
    Both original replays improve from five completed cases then FAIL to
    **7/7 PASS**, with orderly shutdown. 241 K1 tests and Ruff pass. This is
    resource provisioning, not new compiler or full-run credit.
    [Paired record](tests/fixtures/selftest-hugetlb-reservation.out).

  - [x] **K11d — Demonstrate the frozen ptrace/exec wait cycle**, 2026-09-24.
    The [small C case](tests/fixtures/ptrace-exec-reap-cycle.c) captures the
    tracer inside `ptrace_attach` and tracee inside `begin_new_exec` on both
    kernels. A second tracer thread's sibling reap releases the blocked attach;
    stop/detach/reap controls pass without leaving children. The unchanged
    original still has one PASS and one 30-second attach FAIL on each kernel.
    This closes **frozen kernel/test mismatch classification**, not an original
    PASS; changing that baseline is not an EvoCC compiler fix.
    [Source-bound record](tests/fixtures/ptrace-exec-reap-cycle.out).

  - [x] **K11e — Verify rseq's unregistered-thread prerequisite**, 2026-09-26.
    The [small C reduction](tests/fixtures/rseq-registration-order.c) observes
    EINVAL with glibc registration, EFAULT without it, then EINVAL/EFAULT again
    around its own registration/unregistration, on both retained kernels.
    Both unchanged direct originals still FAIL; both unchanged upstream
    companion scripts PASS because they disable glibc registration before exec.
    All six diagnostic/original flights shut down normally. This closes
    **registration-precondition classification**, not the direct test as PASS;
    no global tunable, source, original timeout or historical total is changed.
    [Paired record](tests/fixtures/rseq-registration-order.out).

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

  - [x] **K11g — Attribute delayed dying-memcg release** — `DONE`, P1, 2026-09-26.
    The [small frozen-workload control](tests/fixtures/cgroup-stock-displacement.c)
    and final-reference stack traces account for every delayed CSS identity:
    5/5 GCC and 8/8 EvoCC releases come from per-CPU LRU folio batches through
    folios_put/uncharge, not the hypothesized empty charge-stock slot. New page
    faults drain the batches; simply waiting 20s does not. No lost trace events.
    Unchanged original pairs still FAIL: GCC inner 4 PASS/2 FAIL, EvoCC 3/3;
    dead-cgroup FAIL remains on both. This closes **source-bound classification**,
    not a kernel patch, compiler repair, original PASS or K13 acceptance.
    [Measured controls and limits](tests/fixtures/cgroup-stock-displacement.out).

  - [x] **K11h — Bound immediate memcg-deletion accounting** — `DONE`, P1, 2026-09-26.
    The [controlled page-charge case](tests/fixtures/cgroup-stock-capacity.c)
    measures stable 6,905,856/7,008,256-byte current-vs-stat differences on
    GCC/EvoCC. The frozen kernel has seven cache slots per CPU, but its selftest
    allows only one batch (1,048,576 bytes). Both measurements fit the seven-slot
    capacity; the upstream tolerance patch identifies this same source mismatch.
    This closes **test-bound classification**, not a compiler repair. Original
    EvoCC FAIL at exactly the strict one-batch margin remains FAIL; no new
    tolerance or sleep is applied. Separate LRU/slab findings remain separate.
    [Evidence, source and unchanged verdicts](tests/fixtures/cgroup-stock-capacity.out).

  - [x] **K11i — Verify arm64 syscall-user-dispatch's unavailable boundary** — `DONE`, P1, 2026-09-26.
    The [small C probe](tests/fixtures/syscall-dispatch-unavailable.c) confirms
    all three frozen `!GENERIC_ENTRY` EINVAL stubs while ordinary tracing still
    stops, resumes and reaps its child. GCC/Clang-built probes pass 14 assertions
    per kernel. All three unchanged originals still FAIL on both kernels with
    normal shutdown; none is relabeled PASS. This closes **architecture-boundary
    classification**, not dispatch support or K11/K13's full population.
    [Source, original replays and hashes](tests/fixtures/syscall-dispatch-unavailable.out).

  - [x] **K11j — Separate seccomp values from timing-model assertions** — `DONE`, P1, 2026-09-26.
    The [small filter-chain C case](tests/fixtures/seccomp-filter-chain.c) checks
    the original four-filter sequence, invalid load rejection and enforced EPERM
    beneath prior ALLOW filters. GCC/Clang controls pass 28 assertions per Image.
    Historical original failures are exactly two nanosecond-cost comparisons,
    not incorrect syscall values; the following SKIPs depend on those comparisons.
    Fresh unchanged originals both pass 7/7 within 180 seconds. No repair or
    stable-performance claim is inferred from this retry; historical FAILs and
    the measured subject slowdown remain. Final K13 is still required.
    [Arithmetic, source and paired evidence](tests/fixtures/seccomp-filter-chain.out).

  - [x] **K11k — Isolate OVPN's frozen key-prefix failure**, 2026-09-28.
    Restoring missing inputs (K12d) exposes all seven original scripts failing
    in ovpn-cli before traffic. Its 256-byte cap truncates the supplied 349-byte
    Base64 file to 253 symbols, an incomplete quantum. The
    [small C case](tests/fixtures/ovpn-base64-prefix.c) passes 42 observations
    across GCC/Clang/EvoCC O0/O2 against the actual runtime library, including
    complete-input and aligned-prefix controls. This closes **source-bound
    classification**, not a helper patch or original PASS. All seven unchanged
    reference direct originals remain FAIL. The later unchanged full-runner
    [selected pair](tests/fixtures/kernel-uniform-selected-runtime.out) reaches
    the same decoding error on both kernels and records seven TIMEOUTs per side;
    neither outcome is rewritten or promoted to PASS.
    [Commands, hashes and exact boundary](tests/fixtures/ovpn-base64-prefix.out).

  - [x] **K11l — Attribute the shared BPF module hardening timeout**, 2026-09-28.
    The [small C observer](tests/fixtures/bpf-module-harden-progress.c) reduces
    the unchanged signed module to original case 851: its 98,787-instruction
    program undergoes per-constant whole-array/branch rewriting only with
    harden=2. Neighbor/disabled-blinding controls pass on both Images; bounded
    kprobes prove advancing patch positions and counts, not an entropy wait.
    The same single case completes uninstrumented in approximately 43.5/92.8s
    (GCC/EvoCC). Both full original four-mode programs still TIMEOUT at 300s.
    This closes **source-bound workload classification**, not an original PASS
    or a compiler performance repair. No watchdog or kernel source is changed.
    [Controls, exact source and limits](tests/fixtures/bpf-module-harden-progress.out).

  - [x] **K11m — Preserve FPU worker skips/failures and completion counts**, 2026-09-28.
    The original standalone worker prints SKIP but exits zero; its outer TAP
    PASS previously hid that skip. The [small C protocol](tests/fixtures/fpu-worker-verdict.c)
    and parser RED reproduce it. Shared parsing and full-campaign mapping now
    retain inner counts, reject malformed/missing worker results, and require
    the init-owned nonce-bound CPU count for the stress denominator. All 282 K1
    and 317 ladder tests, two sealed compiler controls and Ruff pass. Original
    reference replay retains the standalone inner SKIP and records 4000/4000
    stress workers OK with shutdown. This closes **result accounting**, not
    the missing standalone prerequisite, new subject acceptance or K12f/K13.
    [Commands, source and raw identities](tests/fixtures/fpu-worker-verdict.out).

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

  - [x] **K12a — Classify four architecture-gated collection recipes**, 2026-09-24.
    The original `kexec`, `prctl` and two Intel thermal Makefiles reproduce
    missing `all` before any compiler invocation on arm64. Twenty original
    Makefile variable controls establish their exact architecture/CROSS_COMPILE
    guards. This is a frozen-recipe limit, not a claim all prctl APIs require
    x86. No guards, build outcomes or 970-program denominator change; no test
    is counted as PASS. [Source-bound record](tests/fixtures/selftest-build-guards.out).

  - [x] **K12b — Prove bitmap helper results before constant queries**, 2026-09-27.
    The 7,002-object prepared pass reaches modpost but `lib/test_bitmap.o` retains
    eight `__compiletime_assert_*` references. The scalar compound-assignment
    [reduction](tests/fixtures/bitmap-constant-mutation.c) is now GREEN in both
    interpreters and all three AArch64 compiler controls. The subsequent
    [pointer/array reduction](tests/fixtures/bitmap-constant-memory.c) drives
    shared, bounded private-memory facts through inlining and constant queries;
    executed accesses and UB checks remain. The original object now has **zero**
    assertion references with unchanged exports. Its signed module and the GCC
    control each execute **390447/390447** original checks on the matching GCC
    Image and unload; seven observer checks pass. Three small fixtures pass 15
    native controls; 81 query tests and the **1841-test** changed gate pass,
    including Clippy/fmt (three historical ignored tests remain).
    This closes the bitmap compiler/module defect, not uniform kernel composition,
    full modpost or K13/K14. [Exact evidence](tests/fixtures/bitmap-constant-memory.out).

  - [x] **K12c — Register the installed virtual NCI device before tests**, 2026-09-28.
    The old pair lacks NFC_VIRTUAL_NCI; the new configuration builds it, but
    the udev-free runtime also omitted its preload. A new prerequisite-list
    RED is repaired using the exact installed module inventory. On the matching
    new GCC Image, signed dependency loads reproduce the original 0/8 failure;
    loading virtual_ncidev makes the same executable pass **8/8**. Eleven
    load/device/ioctl/unload controls and boot/shutdown pass; 261 K1 Python tests
    and Ruff pass. This closes reference prerequisite diagnosis/packaging,
    not subject-module acceptance at that checkpoint. The subsequent
    [new matching pair](tests/fixtures/kernel-uniform-module-runtime.out)
    actually loads the new modules and passes the unchanged original **8/8**
    on both kernels, closing the subject prerequisite too.
    [Small C and identity-bound result](tests/fixtures/nci-module-prerequisite.out).

  - [x] **K12d — Install OVPN's omitted original peer tables and key**, 2026-09-28.
    The install regression goes RED → GREEN using the common runtime helper
    path; all 262 K1 tests and Ruff pass. Only the two original peer tables and
    original key are restored, with missing-source rejection and unchanged
    scripts/peer counts. Eight reference before/after flights boot and shut
    down. The seven restored originals expose K11k and remain FAIL, not PASS;
    no complete new runtime disk or subject Image is manufactured.
    [Identity-bound record](tests/fixtures/ovpn-base64-prefix.out).

  - [x] **K12e — Enable the missing connector process-event configuration** — `DONE`, P2, 2026-09-28.
    The earlier uniform configuration disables CONNECTOR; the collection has no
    upstream config fragment. Two reference controls pass 12/12 absence checks,
    while unchanged proc_filter still fails EPROTONOSUPPORT. PROC_EVENTS needs
    CONNECTOR=y, not a separately loaded module. The shared configuration audit
    now records these source-pinned prerequisites and invalidates changed-source
    reviews; 263 K1 tests and Ruff pass. After the active frozen composition,
    batch both built-in options into a separately identified configuration and
    replay the original on its matching pair. No active config or historical
    outcome changes. [Exact evidence](tests/fixtures/connector-configuration.out).
    The [batched configuration preflight](tests/fixtures/kernel-connector-fpu-preflight.out)
    now solves exactly CONNECTOR=y, PROC_EVENTS=y and TEST_FPU=m, and all five
    newly enabled original C units assemble with reference symbol parity using
    the current pinned compiler. No active Image/config/module changes; full
    composition and unchanged paired runtime remain required for both items.
    The current 848-module GCC reference enables both options and receives
    events. A [bounded C control](tests/fixtures/connector-process-events.c)
    passes 20/20 checks across GCC/Clang builds: owned fork/exit delivery and
    normal SIGINT termination of the unchanged original. Source inspection
    confirms proc_filter is a persistent listener, so the original 45-second
    runner still legitimately records TIMEOUT rather than a finite-test PASS.
    [Exact controls and retained failures](tests/fixtures/connector-process-events.out).
    The final pinned 7,008-object subject now enables both options. The same
    qualified payload passes **20/20** checks there, including the unchanged
    original's owned-child events and normal SIGINT exit. The unchanged
    unattended original still records its **45-second TIMEOUT** on both sides:
    this closes the missing-config/event-delivery task, not that program's
    outer PASS count. Its persistent-listener disposition remains explicit.

  - [x] **K12f — Enable and execute the omitted kernel FPU module** — `DONE`, P2, 2026-09-28.
    Both unchanged originals on the new uniform pair explicitly SKIP because
    CONFIG_TEST_FPU=m is absent. The collection has no upstream config fragment.
    Source-pinned audit now records TEST_FPU=m, architecture support and the
    !KCOV_INSTRUMENT_ALL dependency; its RED and all 271 K1 tests pass. The
    reviewed config has architecture support and no KCOV, so this is an omitted
    request, not an arm64 limitation. Include it with K12e's next configuration
    batch, then compile, sign/install and run the original on both Images.
    [Original logs and precise boundary](tests/fixtures/kernel-uniform-module-runtime.out).
    The new 848-module reference runtime is now installed and signed; unchanged
    run_test_fpu.sh actually loads test_fpu and records 4000/4000 worker OKs.
    K11m exposes the standalone inner SKIP instead of hiding it behind exit 0.
    The shared prerequisite installer now loads the matching installed test_fpu
    module. Its RED and all 283 K1 tests pass; unchanged reference replays now
    record 1/1 standalone and 4000/4000 stress worker OKs, with normal shutdown.
    Only /tests/modules changes on an APFS clone; the old disk/captures remain.
    The final pinned subject's matching module is now signed, installed and
    actually loaded. Both unchanged originals pass: **1/1 standalone** and
    **4000/4000 stress workers**, zero FAIL/SKIP, normal shutdown. This matches
    the qualified GCC reference and closes the omitted-module/runtime gap;
    it does not close the other 968 programs or K13/K14.
    [Final subject identity and original results](tests/fixtures/fpu-worker-verdict.out).

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

- [x] **K15 — Sign kernel return addresses when Kbuild asks for it** — `DONE`, P2, 2026-10-13.
  Found on 2026-10-13, when the README gallery moved to the direct Kbuild
  receipt. Linux 6.18 enables `ARM64_PTR_AUTH_KERNEL` by default. Without the
  `-mbranch-protection=pac-ret` capability, `arch/arm64/Makefile` passes
  `-msign-return-address=non-leaf` without a probe. EvoCC reports that option
  as having no effect. So the direct-build kernel does not sign return
  addresses, although its configuration asks for it; the GCC reference kernel
  signs them.
  Closed by `2a8968a4` and a direct Kbuild run with `fb05debf`. The capability
  probe asks for `-mbranch-protection=pac-ret+leaf`, which EvoCC did not
  accept. It now does. Each accepted signing option signs every function,
  leaves included. As in GCC 14, `-msign-return-address` has no effect. The
  probe passes, and Kbuild passes `-mbranch-protection=pac-ret` to all 2,799
  target C objects and the 5 C-to-assembly helpers. The receipt lists no
  ignored signing option. The Image has 77,584 `paciasp` and 158,196
  `autiasp` instructions; the previous direct Image had none. Both boot lanes
  pass, and the host lane enables pointer authentication. The nine paired
  programs pass on both kernels with 125/125 subtests.
  [Receipt](tests/contracts/kernel-direct-virt-receipt.json).

- [ ] **K16 — Apply `used` written after a pointer `*` as GCC does** — `RETEST`, P2.
  Found on 2026-10-13 in the K15 Kbuild log. Linux's `__ADDRESSABLE` declares
  `static void * __used __section(...) name = &sym;`. EvoIR 10.1.9 moved only
  `weak`, `alias` and `section` from that position to the declaration, so the
  W0001 lint did not see `used`. One Kbuild run printed 13,601 false
  unused-variable warnings, from `EXPORT_SYMBOL`, initcalls and PCI fixups.
  These kernel objects carry `section`, so they stayed in the output. Without
  `section`, `-O2` dropped such an object; GCC 14 keeps it. GCC 14 also accepts
  `unused` in that position without a warning.
  The owner approved the rule on 2026-10-13. EvoIR 2.1.0 attaches `used` and
  `retain` after a pointer `*` in a named declarator to the declaration, as it
  attaches `weak`, `alias` and `section`. The lint reads `unused` on the
  pointer next to a variable's name. Tests cover the warning, `-O2` retention
  of objects and functions, and the GCC 14 cases that still warn: a farther
  pointer, an array element and a parameter. Next action: rebuild the K15
  configuration with this compiler. Exit: its log has no W0001 from these
  macros.

- [ ] **K17 — Name the affected option in unsupported-option warnings** — `RETEST`, P2.
  Found on 2026-10-13 in the K15 Kbuild log. Each "option is not supported and
  has no effect" warning printed and underlined the complete command line. In
  one Kbuild run, 19,634 such warnings filled 74.5 MiB of the 83.4 MiB build log.
  Diagnostic format section 2 asks for the affected argument, not the complete
  invocation. Also, the code generator never allocates `x18`, so
  `-ffixed-x18` is always in effect, but the vDSO compile still reported it as
  having no effect.
  On 2026-10-13 the owner asked to finish K17, including the shorter text
  form. The warning and its `-Werror` error now label the ignored option.
  Diagnostic format 1.3.0 shows only the labelled bytes of a `<command-line>`
  line longer than 80 bytes; the location and the JSON coordinates do not
  change. `-ffixed-x18` is accepted without a warning (EvoMIR Section 2.5).
  Next action: rebuild the K15 configuration and measure the log. Exit: the
  warnings label only the ignored option, and the log has no `-ffixed-x18`
  warning.

## Application failures, accounting and replays

- [x] **B01 — Reconcile every original LLVM failure with subsequent repairs** — `DONE`, P1, 2026-09-23.
  All 44 original failure identities now have a [case-level mapping](tests/fixtures/llvm-failure-reconciliation.out):
  8 targeted original-program PASS (including September 24 B02), 30 named FP
  deferrals, 5 evidenced not-applicable and 1 OPEN (integer-vector `pr23135` L10).
  The existing adapter validates all 35 reviews against exact source/argv and
  evidence. Eight obsolete scalar-FP waivers are removed with a RED/GREEN test.
  This closes reconciliation only: L02–L05/L10 remain open, and the frozen
  1,735-program run and its PASS numerator are unchanged. [Evidence][apps].
  The September 27 complete replay now supersedes that historical checkpoint:
  **1,039 PASS / 1 compiler failure / 30 FP-deferred / 664 not-applicable /
  1 oracle-unusable**. The remaining failure is precisely L10; all 25 newly
  passing original programs and unchanged installed-object hashes are retained
  in run `20260927T085137Z`. L02 is closed for the hosted profile; L03–L05/L10
  remain separate boundaries, not a blanket FP waiver.

- [x] **B02 — LLVM LARGE floyd-warshall performance failure** — `DONE`, P1, 2026-09-24.
  The [small loop transport case](tests/fixtures/loop-value-transport.c) goes
  RED → GREEN by removing proven trivial phi copies during SSA destruction,
  without moving loads, guards or effects. Counter/swap/indirect-edge negatives,
  real spills, both interpreters and six native controls pass; the changed gate
  is 995 PASS, zero failures, four historical ignored, Clippy/fmt green.
  **Unchanged LARGE execution, output hash and comparison now PASS in 52.0360 s**
  inside the original 60-second limit (98 → 57 core instructions). This closes
  the targeted original, not B10 or a stable performance claim. No kernel Image
  inherits it. [Identity-bound record](tests/fixtures/loop-value-transport.out).
  The preceding unsuccessful reductions are retained below as historical steps.
  The [pointer-index reduction](tests/fixtures/repeated-pointer-index.c) now
  shares pure MIR index preparation and passes all interpreter/native controls
  and its changed gate; the unchanged LARGE replay still times out (166 core
  instructions). [Bounded result](tests/fixtures/repeated-pointer-index.out).

  - [x] **B02a — Reuse the value computed by checked add/sub**, 2026-09-23.
    The [small C reduction](tests/fixtures/checked-arithmetic-result.c) and IR
    shape test went RED → GREEN. Full-width 32/64-bit predicates keep their
    computed value across the guard using explicit block arguments; every
    obligation, trap and volatile read remains. C-HIR/EvoIR/MIR/native controls
    pass, as does the 964-test affected gate, Clippy and fmt. Original LARGE
    replay still times out at 60 s (160 versus 166 core instructions): B02 and
    K02 remain OPEN, and no kernel Image inherits this compiler-only repair.
    [Measured record](tests/fixtures/checked-arithmetic-result.out).

  - [x] **B02b — Reuse stable compared integer values**, 2026-09-23.
    The [minimal selection case](tests/fixtures/compared-value-selection.c)
    exposed both repeated loads and an independent C-HIR dereference-type
    inference refusal. Shared pointee typing is now unevaluated; bounded
    integer selections reuse already compared operands with a bit-vector mux.
    Original loads/guards remain, while volatile/atomic/call/update cases do
    not reuse their observations. Six targeted tests and six sealed native
    controls pass. The unchanged LARGE program still times out at 60 seconds,
    despite 160 → 108 core instructions: **B02 remains OPEN**. The affected
    gate passes 1,819 tests, Clippy and fmt; [record](tests/fixtures/compared-value-selection.out).

  - [x] **B02c — Select bounded compared-value muxes with CMP/CSEL**, 2026-09-23.
    [Minimal C](tests/fixtures/compared-value-csel.c) and MIR RED → GREEN cover
    96 shape/allocation cases, 600 exact executions and eight negative chains.
    Seven CLI regressions, twelve native controls and the 980-test affected
    gate pass, with Clippy/fmt. Existing evaluations, effects and guards remain.
    Original LARGE core shrinks 108 → 102 instructions but still times out
    at 60 seconds; **B02 remains OPEN**. [Record](tests/fixtures/compared-value-csel.out).

  - [x] **B02d — Branch directly on single-use checked flags**, 2026-09-24.
    The [small C case](tests/fixtures/checked-flag-branch.c), 600 MIR boundary
    executions, real-spill continuation and nonmatching controls retain all
    traps, volatile accesses and explicit edge bindings. Six positive native
    runs and ten expected-trap negatives pass; the affected gate is 989 PASS,
    zero failures, four historical ignored, Clippy/fmt green. Original LARGE
    core shrinks 102 → 98 instructions but still times out at 60 seconds:
    **B02 remains OPEN**. No kernel Image changes. [Record](tests/fixtures/checked-flag-branch.out).

- [x] **B03 — Account for Csmith's 68 unusable-oracle cases** — `DONE`, P1, 2026-09-24.
  All 1,000 programs emit, 932 checksums agree and zero mismatch; the remaining
  31 base / 37 bitfield cases are not passes. Diagnose reference compile/runtime
  failures with the original seeds and budgets; retain unusable outcomes unless
  a valid matched replay exists. Close with per-case reasons and any recoverable
  comparisons, without replacing seeds. [Receipt](tests/contracts/ladder-csmith-receipt.json).
  All **68/68** identities now have source-bound repeating-path diagnoses;
  none remains undetermined. The [final fourteen](tests/fixtures/csmith-final-restarts.out)
  distinguish finite callees from restarting callers, unchanged guards and
  width-sensitive transitions, using unchanged-source observations on both
  oracles. Two permanent C controls pass eight interpreter and twelve native
  executions; the changed gate passes 503 tests, Clippy and fmt. All diagnostic
  children are reaped. This closes **accounting only**, not those 68 original
  programs as PASS: the 932-agreement numerator and B10 remain unchanged.

- [x] **B04 — Resolve remaining torture/HVA reference limitations** — `DONE`, P2, 2026-09-23.
  The [separate-TU HVA reduction](tests/fixtures/hva-varargs-reference.c) isolates
  GCC O2's missing register-save-area loads; stack arguments work, as do GCC O0
  and Clang O0/O2. Eight usable EvoCC/native ABI pairs and the unchanged original
  pass; the GCC O2 self-control and callee pairs remain unusable. All three
  original torture refusals are reproduced on fixed Clang 19: missing isinf
  aliases, decimal floating types/literals and GNU floating shuffle. GCC's
  originals pass O0/O2. This closes **reference-limit accounting**, not those
  programs as PASS; no source, flag, historical numerator or ABI changed.
  [Measured record](tests/fixtures/hva-varargs-reference.out); tiny-vector ABI is L06.

- [x] **B05 — Replace TinyCC's final two reference-built FP objects** — `DONE`, P2, 2026-09-27.
  Shared binary128 support repairs `tccgen.o` and `tccpp.o`. Fresh run
  `20260927T083017Z` installs **11/11**, no symbol exclusions or failures, and
  original `make test` passes on the reference and full EvoCC build. The runner
  now checks actual relink/test exit status, an exact upstream terminal marker
  and unchanged installed-object hashes, with three false-positive REDs and
  object-replacement negatives. **260** ladder regressions pass. The original
  target-specific Makefile omissions remain omissions, not newly executed tests.
  [Receipt](tests/contracts/ladder-tinycc-receipt.json).

- [x] **B06 — Finish toybox compilation and chattr diagnosis** — `DONE`, P2, 2026-09-27.
  Full run `20260927T084656Z` compiles/installs **203/203** frozen target units,
  zero refusals/exclusions/rebuilt objects; generated applet-table main remains
  reference-built outside that denominator. Both audited builds complete all
  **1,669 occurrences: 1,648 PASS / 1 FAIL / 20 SKIP**, no subject regression.
  The sole common tar FAIL is a demonstrated contradictory upstream expected
  hash; all 20 skips retain source/config/runtime reasons. This closes full
  execution and diagnosis, **not all-tests PASS**. The common score stays FAIL,
  and B10 still requires its final campaign. [Receipt](tests/contracts/ladder-toybox-receipt.json).

  - [x] **B06a — Isolate chattr's filesystem requirement**, 2026-09-23.
    The [minimal ioctl probe](tests/fixtures/toybox-filesystem-flags.c) returns
    ENOTTY on the benchmark FUSE mount and succeeds on container-local ext4 in
    all five GCC/Clang/EvoCC controls. The unchanged 15-case chattr subset
    completes with 5 FAIL / 10 SKIP on both a real reference relink and a
    hash-audited current-EvoCC applet relink. This closes the original filesystem
    diagnosis, **not** those cases as PASS or B06's remaining full-suite work.
    [Record and limits](tests/fixtures/toybox-filesystem-flags.out).

  - [x] **B06b — Run filesystem-sensitive tests on container-local storage**, 2026-09-27.
    The [mutation reduction](tests/fixtures/toybox-filesystem-mutation.c) proves
    both missing GETFLAGS on FUSE and missing immutable capability on local ext4.
    The shared adapter stages only runtime inputs on local storage and adds only
    `CAP_LINUX_IMMUTABLE`, retaining binary/object audits and original assertions.
    Five compiler controls pass; unchanged chattr is **13 PASS / 0 FAIL / 2 SKIP**
    on both historical audited binaries, versus 0/5/10. The conditional j/P skips
    remain skips. All 253 ladder tests pass; no full campaign was run.
    [Evidence and exact limits](tests/fixtures/toybox-filesystem-mutation.out).

  - [x] **B06c — Repair runtime prerequisites and classify the remaining tar failure**, 2026-09-27.
    The [small C/shell cases](tests/fixtures/toybox-runtime-prerequisites.out)
    reproduce missing USER, insufficient isolated hostname/mount capabilities
    and a loop filename exceeding 64 bytes. The shared runtime fixes these six
    failures without changing tests; eight native controls, the 157-occurrence
    targeted pair and complete 1,669-occurrence pair agree. Both audited tar
    binaries produce byte-identical owner/group-overridden archives before and
    after chown, contradicting only the later upstream expected digest. That
    FAIL is retained. All 261 ladder regressions and Ruff pass.

- [x] **B07 — Finish jq's binary128 interface and original tests** — `DONE`, P2, 2026-09-27.
  Run `20260927T082205Z` compiles/installs all **69/69** objects, including
  `src/builtin.c`'s `nexttoward(double, long double)` call. Unchanged `make check`
  passes nine jq scripts and 21 bundled Oniguruma programs; no target object was
  excluded or replaced by make. The prior run exposed omitted **`-pthread`**;
  the minimal driver/projection RED now passes with `_REENTRANT=1` and ordered
  user overrides, confirmed by fixed GCC14/Clang19 controls. This fresh replay
  retains the option and passes again. All 526 driver regressions, 257 ladder
  regressions, Clippy and fmt pass. B10's final unified campaign remains separate.
  [Receipt](tests/contracts/ladder-jq-receipt.json).

  - [x] **B07a — Count jq's `src/main.c` and rerun `make check`** — `DONE`, P2, 2026-10-10.
    Found on 2026-10-10, when the README gallery quoted run `20260927T082205Z`.
    The ladder build-log parser skips a compile command that continues across
    lines. Automake prints jq's non-libtool rule for `src/main.c` in this form,
    so `main.c` is not one of the 69 measured units, and `unmeasuredBuildUnits`
    is empty. GCC compiled `main.c` in the build. In the test step, clock skew
    made `make` regenerate `src/version.h`, rebuild `main.c` with GCC and
    relink `jq`. The rebuilt-unit check covers only measured units, so it did
    not report this. The tested `jq` had 69 of its 70 C units from EvoCC. The
    B07 statement that make replaced no target object holds only for the 69
    measured units. No other ladder build log has a skipped compile line; the
    other 21 skipped jq lines are libtool wrappers, and their real compile
    commands are captured. Next action: add a parser regression with the exact
    continued automake rule, count continued commands, and make the
    rebuilt-unit check cover every compile in the test log. Then rerun E10 jq
    from its pinned source. Exit: a new receipt with 70/70 units and no compile
    command in the test log.
    Closed by run `20261010T094547Z`: 70 of 70 units by EvoCC, including
    `src/main.o`, in a passing build. The unchanged `make check V=1` passes
    30 test programs (9 jq, 21 Oniguruma). The relink and test logs contain no
    compile command and no `src/version.h` rewrite. The parser now joins
    backslash-continued recipe lines and splits an unquoted `;` that touches a
    word. On every other recorded ladder build log, the unit set and each
    unit's argv are unchanged. The clock skew was not the cause: the release
    ships `src/version.h` but no `.remake-version-h` stamp, so make remade the
    header on every run. The jq build now creates the stamp with an old time;
    `make -o src/version.h` does not help, because make does not pass `-o` to
    sub-makes. The case-sensitive sysroot for this run is a sparse APFS image
    mounted at `.build/case`. [Receipt](tests/contracts/ladder-jq-receipt.json).

- [ ] **B08 — Build and test musl with its actual semantic flags** — `OPEN`, P2.
  Fresh run `20260927T110110Z` attempts all 1,339 records with `-frounding-math`:
  1,271 compile/assemble with exact symbol parity and survive two complete
  libc-test replays without additional failures or replaced objects. This
  supersedes the old 0/1,339 flag refusal, but remains a mixed build. All 68
  remaining refusals are floating-complex interfaces (E3049, L04). The 17
  reference failures (B08d) and explicit command omissions (B08b) remain open;
  no full-library or all-tests-PASS claim follows from the clean relative result.
  [Receipt](tests/contracts/ladder-musl-receipt.json).

  - [x] **B08a — Admit scalar FP inline-assembly bindings** — `DONE`, P2, 2026-09-27.
    Explicitly approved and specified in EvoIR 1.9.114 / EvoMIR 0.50. Scalar
    f32/f64 use the existing alias-aware SIMD bank, exact representation
    transport, matching/early-clobber bindings, spills, asm.goto edge copies
    and conservative world/fenv effects. Default operands print vN, matching
    fixed GCC14/Clang19; s/d views are explicit. Reached opaque asm still
    reports E3021 in both interpreters; f128, floating vectors, GPR bindings
    and general-registers-only remain refused. Eight native environment builds
    pass 58 checks each; sixteen crossed calls and high-pressure selected-edge
    cases pass. All eight original musl units assemble; the four original
    ceil/floor/rint/rintf objects pass 56 checks with each of four reference
    callers, alongside their reference-object controls. Full musl is separate.
    [Source, exact identities and validation](tests/fixtures/floating-assembly.out).

  - [ ] **B08b — Close musl's remaining command-projection semantics** — `OPEN`, P2.
    The current original-command inventory has no omitted `-f`/`-m`/`-O`
    request; bounded tuning admission and fail-closed projection are below.
    Remaining diagnostic/output/assembler actions need their own equivalence
    and refusal tests, including `-Werror` behavior. No full musl
    fidelity claim may be inferred from accepting `-frounding-math` alone.
    - [x] **B08b1 — Preserve already-satisfied conservative codegen options**, 2026-09-27.
      Ten frame/disabled-transform options now reach the driver without being
      discarded. Existing frame, source/minimum alignment and no-transform
      contracts are explicit; positive transforms and unrelated GCC tuning still
      reject. Both interpreters, 12 native controls, ten original musl object
      commands and a 1,339-record option inventory pass. The affected gate is
      green (1,062 passed, four existing ignores), as are 298 ladder tests.
      [Reproducer, boundaries and identities](tests/fixtures/conservative-codegen-options.out).
      This is one B08b subcase, not complete option fidelity.

    - [x] **B08b2 — Preserve unsupported semantic requests and bound GCC tuning**, 2026-09-28.
      Two command projectors silently dropped `-fpack-struct=1`; four native
      reductions then used the wrong layout while eight reference controls
      established the flag-sensitive boundary. Semantic families now reach the
      driver in written order, even when rejected; forced language and joined
      include operands survive too. Three exact tuning requests select existing
      whole-function allocation, absent hoisting and nonduplicating placement
      under EvoMIR 0.51, not arbitrary GCC algorithms. Both interpreters,
      eight tuning native controls, ten unchanged musl object commands and the
      1,339-command read-only inventory pass. Unknown layout/ABI/fast-math requests
      remain explicit refusals. Diagnostic policy remains B08b, not waived.
      [Reproduction and measured boundaries](tests/fixtures/command-projection-packed.out).

    - [x] **B08b3 — Preserve include search classes and joined operands**, 2026-09-28.
      A three-file reduction detects wrong header selection after `-isystem`
      becomes `-I`, or a joined system option disappears. Three of four native
      projected commands fail before repair; all four pass afterward, with
      eight GCC/Clang controls per round. Both projectors preserve search class,
      joined macro/quote/forced-include paths, literal spaces and incomplete
      operands for driver refusal. Unsupported quote/macro options are not
      translated into supported ordinary includes. All 314 ladder tests and
      targeted Ruff pass. This is not complete diagnostic-option fidelity.
      [Exact inputs and results](tests/fixtures/include-projection.out).

    - [x] **B08b4 — Preserve diagnostic requests instead of false acceptance**, 2026-09-28.
      Both adapters discard options that make the small source fail in all
      eight GCC/Clang controls. They now preserve warning policy in written
      order; eight EvoCC projections explicitly refuse, without assembly.
      All 317 ladder tests and targeted Ruff pass. All 1,339 original musl
      commands retain diagnostics; ten actual commands refuse unsupported
      warnings without changing installed objects. This closes silent omission,
      **not warning implementation**: a faithful full musl rebuild still needs
      that support. Historical omitted-option runs retain their claim ceiling.
      [Reproducer and exact results](tests/fixtures/command-projection-diagnostics.out).

    The [standard-excess-precision subcase](tests/fixtures/floating-standard-evaluation.out)
    is now repaired: the existing FLT_EVAL_METHOD=0 semantics satisfy the explicit
    option, which survives shared command projection. Both interpreters, eight
    reference and eight EvoCC native controls pass; 527 CLI and 262 ladder tests,
    Clippy/fmt/Ruff pass. Eight original musl commands have identical results
    with the option retained (four emit, four B08a refusals). This is not full
    option-fidelity or musl acceptance.
    [Per-symbol section placement](tests/fixtures/driver-symbol-sections.out)
    is also implemented in EvoMIR 0.49: explicit attributes override independent
    last-option-wins switches; native ELF/TLS/storage flags, aliases, lifecycle
    and patch roots survive. Twelve reference and twelve EvoCC objects pass
    ordinary/GC linking (48 native executions). All four command projectors
    retain the flags; 38 focused CLI, 264 ladder and 252 K1 tests pass. The same
    eight musl commands retain sections: four assemble, four remain B08a.
    [No-strict-aliasing admission](tests/fixtures/no-strict-aliasing.out) now
    preserves the existing conservative access semantics rather than dropping
    the flag. Both interpreters, four reference and eight EvoCC native builds
    pass; the 530-test affected driver gate is green. The eight original musl
    outcomes remain identical, with this flag retained. Remaining tuning and
    diagnostic omissions are not silently reclassified as implemented options.

    September 27 targeted repair also preserves `-Xassembler` operands, both
    assembler spellings' written override order and literal argv boundaries;
    failed container execution cannot publish partial assembly success. Five
    harness REDs and six real ELF objects verify the fix. The old
    `-Wa,--noexecstack` spelling was already transported to the assembler.
    [Targeted evidence](tests/fixtures/musl-reference-boundaries.out).

  - [x] **B08c — Close original musl pthread runtime regressions** — `DONE`, P2, 2026-09-27.
    The fresh scalar-asm compiler assembles 1,271/1,339 original units, all with
    exact symbol parity; the 68 refusals are floating-complex interfaces (L04).
    Its first complete mixed libc-test trial adds seven pthread failures or
    timeouts to the reference's 17 failures. Object-set bisection in
    `.evidence/ladder/musl/20260927T101243Z` isolates `pthread_barrier_wait` and
    `pthread_cond_timedwait`; its completed final replay accepts only the
    1,269-object set excluding those two. Neither compilation nor that clean
    partial set closes this task. The isolated
    [LL/SC probe](tests/fixtures/assembly-exclusive-window.c) exhibits failed
    progress with separated templates and compiler stack accesses; a joined
    template is a passing control. GCC O0 also fails some split cases, so this
    is not evidence that arbitrary separate asm blocks guarantee adjacency.
    Ordinary scalar asm outputs now stay in SSA when no object is required.
    The same probe improves from eight failed split locations to zero;
    377 lowering and 52 asm integration tests pass. Five unchanged affected
    libc-test programs pass with the repaired object pair and the separate
    reference archive; the old pair reproduces four failures/timeouts there.
    All scalar-w native/crossed/fenv controls remain green. Fresh run
    `20260927T110110Z` rebuilds all original units with the repaired compiler:
    all 1,271 candidates survive both complete replays (482 runtime-result slots
    each), zero added failures, zero changed object hashes and no excluded
    pthread objects. The final replay exits normally; its 17 failures exactly
    match the reference. The changed gate has 1,153 PASS / 0 FAIL / three
    historical ignores, plus Clippy/fmt; 271 ladder tests pass separately.
    [Reduction and final acceptance](tests/fixtures/assembly-exclusive-window.out).

  - [ ] **B08d — Resolve the reference libc-test failures and static/DSO coverage gap** — `OPEN`, P2.
    The fresh reference baseline still has 17 failing cases, not waived PASS:
    one API/header mismatch, five DSO/TLS cases in the static-only build, two
    strptime cases, two strtold rounding cases, two shell/wordexp cases, one powf
    exception case, two daemon timeouts and two fork/rlimit expectation failures.
    The exact observations and unproven root causes are recorded in the
    [benchmark notebook](docs/archive/notebook/benchmark-evaluation-20260915.md#2026-09-27-musl-reference-failure-ledger).
    Add a separate genuine shared-library/loader lane for DSO tests; do not
    relabel those static-lane failures as compiler passes. Diagnose the remaining
    common failures against pinned source and environment without weakening the
    original assertions or increasing watchdogs to conceal a progress defect.

    - [x] **B08d1 — Supply the atfork limit test's unprivileged environment** — `DONE`, 2026-09-27.
      The isolated fork probe and unchanged atfork program fail to induce a
      fork error as root, but pass at uid 65534 with both reference and mixed
      archives. The common adapter now installs a hashed case wrapper for
      exactly the two upstream atfork variants; privilege-drop errors fail,
      other cases retain their identity, and the reference supervisor's
      five-second timeout/100 KiB stack are unchanged. Four native executions
      through the actual wrapper pass. This does not change the historical
      17-failure receipt or close B08d's full acceptance.
    Source-bound probes also reproduce strptime/strtold/wordexp/powf differences
    identically for GCC/Clang/EvoCC callers and both archives (96 records).
    The original daemon test passes in this isolated replay; million-fd
    exhaustion takes about 0.12 seconds, so fd count alone does not explain its
    historical five-second timeout. The later B08d6 filesystem-controlled
    reduction below closes that previously unexplained timeout subcase.
    [Cases, exact identities and observations](tests/fixtures/musl-reference-boundaries.out).

    - [x] **B08d2 — Execute the five DSO/TLS subjects with a genuine loader** — `DONE`, 2026-09-27.
      A separate shared composition reuses 1,268 hash-verified accepted EvoCC C
      objects and compiles both loader C units with EvoCC. Its 68 complex C
      objects remain reference-built; 13 assembly objects are separately
      accounted. All five unchanged original programs pass on both reference
      and candidate loaders, first in the reduction and then through the durable
      adapter (10 + 10 executions). PT_INTERP, object/library hashes and complete
      paired populations are checked. The existing musl/common-bench path now
      includes this supplement; old static receipts are not rewritten. This is
      **1,270/1,338 target-C** shared coverage, not full-musl PASS.
      [Composition, hashes and exact scope](tests/fixtures/musl-shared-loader.out).

    - [x] **B08d3 — Localize the two strtold binary128 reference failures** — `DONE`, 2026-09-27.
      The [small source reduction](tests/fixtures/musl-hex-tail-reduction.c)
      proves the pinned parser collapses both below-halfway and above-halfway
      nonzero tails to a halfway value when 29 leading-1 hex digits fill all
      113 significand bits. Both interpreters and GCC/Clang/EvoCC native O0/O2
      reproduce ...1112 twice; an extra guard-digit control restores ...1113
      and ...1111. This is a source-bound library defect disposition, **not**
      a changed benchmark or original-test PASS. A general library repair or
      dependency-pin change remains a separately declared configuration.
      [Exact rounding argument and executions](tests/fixtures/musl-hex-tail-reduction.out).

    - [x] **B08d4 — Localize powf's upward-overflow reference failure** — `DONE`, 2026-09-27.
      The [source-bound stage reduction](tests/fixtures/musl-powf-rounding-reduction.c)
      executes the pinned log/exp algorithm and a separate exact-conversion
      control. All 12 GCC/Clang/EvoCC O0/O2/archive combinations agree: for
      `powf(max_float, 1)` upward rounding, the pre-narrowing approximation is
      69 binary64 ULPs above the exact input, producing infinity and flags 0x14.
      The explicit early-overflow branch is not taken; exact conversion has no
      flags. This closes **source-library attribution**, not the original test
      or full musl. A library repair/pin change is a distinct configuration.
      [Bit patterns, identities and observer limitation](tests/fixtures/musl-powf-rounding-reduction.out).

    - [x] **B08d5 — Attribute the strptime and API/header baseline gaps** — `DONE`, 2026-09-27.
      The pinned `%s` source deliberately only consumes digits; `%z` requires
      four digits after its sign. The measured three-compiler/two-archive
      controls agree with those paths. POSIX does not require this test's
      particular `%s` tm value or `-06` input extension. Separately, the missing
      API executable follows two names absent from pinned `unistd.h`, not a
      runtime compiler error. All three original failures remain failures;
      no header/test or denominator is changed. [Source hashes and contract](tests/fixtures/musl-reference-boundaries.out).

    - [x] **B08d6 — Isolate descriptor-exhaustion output from host virtiofs**, 2026-09-27.
      The [fork/dup/exit reduction](tests/fixtures/descriptor-exit-filesystem.c)
      and unchanged daemon program fail only with mounted-file stdout; pipe and
      guest-local regular-file controls pass in both archive lanes. Children
      finish descriptor exhaustion and wait in exit_group/filesystem requests.
      The existing wrapper now spools just these two supervisors to a guest
      regular file, preserving exact logs/status/signals and the original
      five-second watchdog, 100 KiB stack and FD limit. Twelve archive/variant-path
      repetitions plus both actual retained executables pass; a deliberate
      timeout still fails. Ten wrapper tests on both hosts, all 296 ladder tests
      and Ruff pass. No full receipt is rewritten. [Evidence](tests/fixtures/descriptor-exit-filesystem.out).

    - [x] **B08d7 — Separate wordexp's library and shell failure families**, 2026-09-27 UTC.
      Fourteen small observations across three compilers, O0/O2 and both pinned
      archives yield 168 identical completed executions, not 168 correctness
      passes. The pinned library ignores WRDE_UNDEF, overrejects parameter braces
      with NOCMD and leaks command-substitution stderr; its helper arguments are
      visible. Direct dash controls explain dollar-quote behavior. Special-
      parameter/comment allowances are not generalized to waive other failures.
      Both unchanged originals still FAIL. Nested grammar assertions remain
      individually unadjudicated; this closes bounded source/environment
      attribution, not wordexp, B08d or full musl.
      [Small cases and exact observations](tests/fixtures/musl-wordexp-boundaries.out).

    - [x] **B08d8 — Repair numerical bugs in an explicitly patched dependency lane**, 2026-09-28.
      User-approved supplement leaves original musl sources, archives and FAIL
      receipts untouched. Backport the upstream hexadecimal guard-digit fix;
      also fix newly reproduced directed subnormal double rounding and exact
      powf(x,1) value/status handling. Integer-derived four-mode probes pass
      **71,040/71,040** checks across GCC/Clang/EvoCC O0/O2 and two base archives.
      **180/180** unchanged original-entry replays pass (15 entry names, eight
      distinct source programs); 192 total executions have verified link maps
      and unchanged inputs. 301 ladder tests pass. This closes only the patched
      numerical supplement, not the original musl lane or B08d parent.
      [Patch, recipe and retained REDs](tests/fixtures/musl-patched-numerics.out).

    - [x] **B08d9 — Verify explicit patched strptime compatibility**, 2026-09-28.
      In the separate dependency configuration, `%s` now populates local broken-
      down time with signed/overflow checks and `%z` accepts HH as well as HHMM.
      **540/540** focused checks and **24/24** unchanged original-entry replays
      pass across three compilers, O0/O2 and both archives. These extend musl's
      intentional behavior; they are not compiler defects or unchanged-original
      PASS. The numerical supplement's 192 executions remain green after sharing
      suite selection. [Bounded contract and evidence](tests/fixtures/musl-patched-time.out).

    - [ ] **B08d10 — Complete patched wordexp behavior** — `OPEN`, P2.
      Child stderr is now redirected before shell parsing/expansion, and helper
      positional arguments are cleared before expanding user words. These two
      repairs pass **576/576** focused checks across the paired 12-way matrix.
      Unchanged original-entry replays still **FAIL 24/24**, with 71 diagnostics
      per transcript versus 107 before repair. Nested NOCMD grammar, ignored
      UNDEF and pinned dash dollar-quote compatibility remain; the overall
      supplement deliberately reports FAIL. No parser safety is inferred from
      transport tests. [Patch and exact remaining boundary](tests/fixtures/musl-patched-wordexp.out).

    - [x] **B08d11 — Repair the missing API names in the patched dependency lane**, 2026-09-28.
      Added real UUCP-unavailable and bounded tmpfs timestamp-resolution
      dispatch, not header-only constants or guessed filesystem values.
      **324/324** focused checks and **12/12** original API-program links/runs
      pass across the paired matrix; all 79 API objects are accounted for
      (78 retained reference objects plus recompiled unistd.c). Other filesystem
      associations explicitly return EINVAL. Clang's separate C99-wrapper
      diagnostic adjustment and its original failure remain recorded. The
      original headers/archives/receipts are untouched; 303 ladder tests pass.
      [Exact scope, patch and evidence](tests/fixtures/musl-patched-conf.out).

    - [x] **B08d12 — Repair wordexp allocation ownership and error handling**, 2026-09-28.
      The previous supplement loses getdelim buffers on EOF/error and a word
      on vector-growth failure; it can also report success after sentinel-read
      or empty-vector allocation failure. Source-only fault injection reproduces
      **34/192 failures per build**, then **2304/2304 checks pass** across twelve
      patched GCC/Clang/EvoCC O0/O2 and reference/mixed-library executions.
      APPEND/DOOFFS, partial results and freeing are checked. The uninstrumented
      transport regression remains 576/576 PASS, but all 24 unchanged original
      wordexp entries still FAIL; B08d10's grammar boundary remains OPEN.
      All 304 ladder tests and targeted Ruff pass. [Exact evidence](tests/fixtures/musl-wordexp-memory.out).

- [x] **B09 — Exercise cJSON's complete upstream private-helper suite** — `DONE`, P2, 2026-09-23.
  The corrected common bench path compiles/installs **23/23** units: both library
  units and all 21 upstream Unity test TUs, including their embedded private
  cJSON implementation. All 21 programs complete: **161 PASS / 0 FAIL / 1 upstream
  IGNORE**, plus the original public test and 54 public-API controls. The pinned
  non-number test unconditionally calls `TEST_IGNORE`; four upstream disabled
  JSON-patch fixtures produce eight skip messages across two test directions.
  Neither is counted as an executed PASS. No assertions changed, no subject
  object was rebuilt/excluded; Unity alone remains a reference-built harness.
  Run `20260923T073554Z`; [receipt](tests/contracts/ladder-cjson-receipt.json),
  [acceptance and limits][apps]. Optional sanitizer/Valgrind configurations were
  not exercised; B10 still reruns this lane with the final compiler.
  September 27 replay `20260927T092953Z` again passes **23/23**, **161 + 54**
  executed Unity checks, one upstream IGNORE and zero make-replaced objects.
  It also closes T05f's stale-archive relink regression; compiler identity remains
  in that receipt rather than being inherited from a later driver build.

- [ ] **B10 — One final serial 15-lane application campaign** — `OPEN`, P1, after tailored closure.
  Freeze the compiler/profile and run the existing `tools/bench.py` lanes without
  reducing populations. Retest passing zlib, zstd, cJSON, Mbed TLS, PolyBench, Lua
  and SQLite as well as repaired lanes. Close with fresh source-bound five-way
  accounting and zero unexplained compiler failures; deferrals/N/A/unusable
  oracles remain distinct from ALL PASS. [Current aggregate](tests/contracts/bench-receipt.json).
  September 27 follow-ups with one frozen subject close jq 69/69, TinyCC 11/11
  and the entire **220/220 c-testsuite** population, with zero c-testsuite
  deferrals/exclusions. These individual fresh lanes do not constitute the
  final 15-lane campaign. Historical binary128/strict-fenv waiver labels no
  longer excuse a new refusal under the now-implemented hosted profile.
  Complete torture replay `20260927T084053Z` also closes all **18** former
  binary128 refusals: **1,522 PASS / 0 compiler failures / 21 FP-deferred /
  146 not-applicable / 3 oracle-unusable**, fixed population 1,692. The six
  original raw execution failures retain their existing source-bound non-PASS
  dispositions; no source, timeout, option or applicability predicate changes.
  LLVM run `20260927T085137Z` adds **25** original-program PASS over the old full
  run, leaving only L10 as an unclosed compiler-failure classification. All
  1,735 identities remain and none of the 1,048 installed objects is rebuilt.
  The subsequent complete cJSON replay adds a seventh fresh lane. The eight
  remaining lanes still retain their earlier measurements; this is not B10 closure.

## Remaining numerical and language contracts

These are not proposals to redo scalar f32/f64, integer vectors, TLS,
constructors/destructors, GNU inline or raw-byte lexing. Those shared capabilities
have implementations and targeted evidence. The following boundaries remain
explicit; [the research brief][semantics] supplies the detailed probe contracts.

- [x] **L01 — Strict floating environment** — `DONE`, P2, 2026-09-27, admitted non-trapping profile.
  The September 27 report approves strict world-threaded FP, a separate `fenv`
  effect, four rounding modes and five sticky flags; dynamic rounding precedes
  full non-trapping FENV_ACCESS. Traps and Annex F claims remain excluded.
  September 27 targeted progress: the explicit rounding/status adapter and
  world snapshot passed 2,630,976 independent TestFloat value/status records;
  22,568 Arm instruction records agree in GCC/Clang O0/O2. Strict IR, effect
  derivation, text/proof preservation and MIR lowering are implemented in
  1.9.106/0.45. A reference caller of actual EvoCC-emitted strict IR passes
  62 checks in each of the four sealed native builds, including narrow integer
  conversion and a signaling-NaN definedness predicate that must not set flags.
  The source 1.9.107/0.46 batch adds dynamic-rounding lowering and C-HIR
  execution, including runtime constants, conditional conversions and quiet
  comparison builtins. Native source checks pass 40/40 in each of eight
  EvoCC/reference optimization combinations. The explicit target fenv resolver
  now covers the eleven get/set/clear/raise/save/hold/update functions; glibc's
  overflow/underflow raise also sets inexact, unlike the separate musl adapter.
  Both interpreters and eight native source/caller combinations pass the
  all-mask/all-rounding library probe. Two synchronized native threads preserve
  opposite rounding modes. Strict integer128 helpers now pass 36,784 exact
  values/status/control records in each of eight source/caller builds, plus
  768 raw-IR/prior-status checks in each of four reference callers; both source
  interpreters pass all four modes. FENV_ACCESS lexical scope now preserves
  ON/OFF/DEFAULT across preprocessing, inline substitution, direct/indirect goto,
  switch, loop exits, statement expressions and return conversions. Eight source
  programs, including the unchanged fenv library probe, pass both interpreters
  and eight native crossed builds; Clang19 O0/O2 controls also pass. After B08a
  approval, the eight-unit
  targeted replay now assembles all eight, including ARM64 ceil/floor/rint/rintf;
  the original four scalar-assembly objects also pass native value/status
  checks with four reference callers. Fresh original run `20260927T110110Z`
  now retains 1,271/1,339 objects in two complete libc-test replays, without
  additional failures or replaced objects. That closes the original replay
  requirement for this finite strict-FP implementation, not B08's full-library
  claim: the 68 floating-complex refusals, 17 reference failures and option
  accounting remain separate. No caller FP state is silently reset. Traps,
  Annex F and floating-complex/vector execution are not authorized by this
  closure. [Brief §A][semantics].

- [x] **L02 — Executable binary128 / long double** — `DONE`, P2, 2026-09-27, hosted Linux profile.
  The September 27 report approves real f128, the existing shared ABI classifier
  and audited hosted helpers. Fixed-RNE and strict helper behavior are separate
  gates; approval does not itself establish implementation or original-program PASS.
  The sealed hosted runtime exports the required TFmode helper families. A
  new 6,400-record binary128 arithmetic/FPCR/FPSR probe agrees byte-for-byte
  under GCC14/Clang19 O0/O2. The shared APFloat adapter now independently matches
  those records and 908,544 TestFloat arithmetic/comparison/conversion records.
  Quad FMA adds 24,532,992 independent records and 32,000 native bit/flag/control/
  errno records in each of four builds. Runtime default NaN and FMA payload
  selection differ from scalar hardware and have explicit regressions. This
  qualifies numerical prerequisites. The 1.9.110 batch adds actual f128 source/IR,
  exact-bit text, typed objects, both interpreters and quad HFA/vararg metadata.
  Arithmetic/NaN/dynamic-mode/vararg programs and E3001/E3063 negatives pass;
  independent proof mutation rejects changed bits, format and HFA role.
  Hosted native quad lowering now passes eight EvoCC/reference numeric builds
  (6,400 arithmetic, 32,000 library-FMA, 18,412 integer128, 9,256 integer32/64,
  304 format conversions and 327,680 predicate checks per build). Sixteen crossed
  ABI builds pass scalar/mixed/stack/HFA/vararg/callback tests, including a
  callback that clobbers the unpreserved SIMD upper halves. The raw MIR matrix
  also exposed and fixed an APFloat adapter panic at signed bits<1>; both pure
  definedness and strict total conversion now have all-width/mode regressions.
  Raw IR adds 5,376 total-conversion checks and 32,000 explicit FMA records in
  each of four native builds. The changed-crate gate passes **2,416 tests / zero
  failures / four retained ignores**, Clippy and fmt. jq's fresh 69/69 original
  replay also closes the discovered `-pthread` projection gap (B07). This closes
  the audited hosted Linux numerical/ABI profile, not freestanding helpers,
  Darwin quad, Annex F, floating complex/vectors or the full B10 campaign.
  Qualify real binary128 semantics and target runtime helpers, then implement
  literals, operations, conversions, both interpreters and the integrated ABI.
  Close with GCC/Clang crossed calls, HFA/stack/varargs and affected originals,
  including c-testsuite's remaining case and torture's 18 binary128 deferrals.
  Binary64 substitution is forbidden. [Brief §B][semantics].

- [ ] **L03 — Binary16/bfloat16 executable boundaries** — `NEEDS_RULE`, P2.
  Bind each requested type/operation to the existing named LLVM/application
  refusal; specify its numerical and ABI contract independently. Implement only
  the approved scope and retain explicit refusals elsewhere. Close with
  bit-exact interpreter/native tests and unchanged affected programs, not type
  spelling or storage-only support. [Source classifications][apps].
  The [fresh batch](tests/fixtures/remaining-boundaries-20260927.out) measures
  `_Float16`, `__fp16` and `__bf16` separately: 24 fixed-reference executions
  pass, including argument/return shapes. EvoCC still refuses all six cases;
  half/bfloat function signatures originally exposed E3101, and other paths report parser,
  value or interpreter diagnostics. These are now named reproductions, not an
  unexplained E3101 or an automatic generic-FP waiver.
  September 28's [T01b repair](tests/fixtures/typed-call-requirements.out) gives
  those signatures the existing E3049 classification; all eight original
  command replays retain refusal. This does not implement L03.

- [ ] **L04 — Floating complex execution** — `NEEDS_RULE`, P2.
  Torture retains 15 floating-complex originals. Specify complex arithmetic,
  exceptional values, conversions and ABI, with required runtime helpers and
  fenv interactions. Implement shared semantics and close with tailored numerical
  and crossed-ABI tests plus original programs. Integer complex support does not
  close this task. [Brief][semantics], [classifications][apps].
  The [batch probe](tests/fixtures/floating-complex-boundaries.c) separates
  component stores, finite arithmetic, exact construction and exceptional
  arithmetic for all three formats. Sixty reference executions are captured;
  only 24 are asserted finite controls, 36 are bit/status observations. Division
  by complex zero raises different flag sets between f32 and f64/f128 in the
  sealed references. All 68 current musl refusals and their original argv/source
  identities are indexed in the [batch packet][closure-batch]. Do not replace
  this numerical/helper decision with merely enabling complex signatures.

- [ ] **L05 — Floating vector execution and intrinsics** — `NEEDS_RULE`, P2.
  Six torture originals and other named LLVM paths remain deferred. Define lane
  FP semantics, conversion/mask operations and required intrinsics while preserving
  `-mgeneral-regs-only` boundaries. Close through interpreter/native/ABI tests
  and affected originals; integer SIMD and scalar FP passes are not sufficient.
  [Brief][semantics], [classifications][apps].

- [ ] **L06 — Sub-8-byte integer-vector external ABI** — `NEEDS_RULE`, P2.
  GCC GPR and Clang SIMD conventions disagree and crossed controls fail.
  Choose/document a bounded ABI from the actual target contract; do not invent
  interoperability from self-calls. Close with explicit mode identity, both
  caller/callee directions for each supported reference and negative controls;
  otherwise retain the named refusal and incompatibility. [Brief §C][semantics].

- [ ] **L07 — Integer-complex division** — `NEEDS_RULE`, P2.
  `(9+7i)/(2+1i)` yields GCC `4+3i` and Clang `5+1i`; current refusal is E2948.
  Specify the algorithm, promotions, truncation, overflow and zero-divisor
  contract, then implement shared evaluation/lowering. Close with boundary and
  compound-assignment tests plus affected originals, not one selected result.
  [Brief §C][semantics].

- [ ] **L08 — Pointer-to-VLA typeof operand evaluation** — `NEEDS_RULE`, P2.
  Oracles disagree on a side-effect counter (GCC 0, Clang 1); agreed array forms,
  bound capture and allocation/lifetime cases already work. Decide expression
  categories and evaluation/capture rules, then close with scope, side-effect and
  lifetime controls and original replays. Do not reopen all VLA support.
  [Existing fixtures](tests/fixtures/variable-array-typeof-profile.c), [brief][semantics].

- [ ] **L09 — Nonzero-depth return-address support boundary** — `NEEDS_RULE`, P2.
  GCC returns zero while Clang reads an ancestor LR in the retained probe.
  Define target frame/PAC/optimization assumptions and distinguish reached-only
  unsupported execution from UB. Close with native and interpreter boundary
  tests plus original replays; never fabricate a valid ancestor address.
  [Fixture](tests/fixtures/ancestor-return-address.c), [brief][semantics].

- [ ] **L10 — LLVM pr23135 source-definedness decision** — `NEEDS_RULE`, P1.
  GCC UBSan reports vector signed overflow while Clang instrumentation is silent.
  Analyze the unchanged source and effective flags and establish an adequate
  evidence threshold. Close with either an approved case-bound disposition or a
  compiler RED/fix/replay; silence from one oracle is not proof of definedness.
  Feed the result into B01, not directly into a historical numerator. [Brief §C][semantics].

- [x] **L11 — Inventory additional atomic scope before extending it** — `DONE`, P2, 2026-09-23.
  The [source/command inventory](tests/fixtures/atomic-scope-boundaries.out)
  distinguishes SQLite's supported scalar accesses from TinyCC's uncaptured
  atomic runtime/test paths and zlib's nondefault dynamic CRC table. Three small
  [C cases](tests/fixtures/atomic-scope-boundaries.c) pass all 12 fixed-oracle
  controls; nine CLI boundaries retain E2702/E2001 refusals, not false support.
  No new current-lane target-object blocker was found. This closes the inventory,
  **not** additional atomic support; the explicit capability tasks below remain.

  - [ ] **L11a — General `_Atomic` objects and updates** — `NEEDS_RULE`, P2.
    TinyCC's bounds-checking runtime and C11 atomic test configuration expose
    this scope; fixture CASE=0 currently refuses E2702. Define object operations,
    widths, orders, volatile/provenance and library/lock-free boundaries; implement
    shared paths and verify the actual runtime configuration, not just 11/11
    compiler objects. zlib's optional dynamic-table atomic_flag is also separate.
  - [ ] **L11b — Additional `__atomic` RMW/CAS/exchange** — `NEEDS_RULE`, P2.
    CASE=1 refuses E2001. Specify success/failure orders, weak/strong CAS,
    expected-value updates, dynamic orders and width limits before extending the
    existing legacy RMW representation. Require native crossed-compiler tests;
    single-thread interpreter success is not concurrent-memory-model proof.
  - [ ] **L11c — Generic aggregate atomics** — `NEEDS_RULE`, P2.
    CASE=2 refuses E2001 for a naturally aligned eight-byte record. Specify
    representation/initializedness/provenance and lock-free versus runtime-helper
    obligations; qualify target runtime/ABI support. Scalar builtin acceptance
    must not silently admit arbitrary size/alignment or unimplemented helpers.

## Cross-cutting verification and tooling

- [x] **T01 — Reconcile the historical issue list with checked-in regressions** — `DONE`, P1, 2026-09-23.
  The [complete identity/fixture crosswalk](tests/fixtures/language-issue-reconciliation.out)
  maps the old asm, declaration, VLA, static-address, varargs and qualification
  cases without reopening repaired families. All 107 formerly unclassified
  torture E3101 failures map to 88 later PASS, 15 named deferrals and four
  evidenced exclusions; all 148 historical LLVM identities map to actual later
  results and B01's follow-ups. Source/evidence validation and 68 targeted Rust
  tests pass. This closes reconciliation, **not** L02/L05/L07–L11, B02/B10 or a
  fresh full campaign. Narrowing label constants and independent oracle limits
  remain explicit; generic E3101 is never itself an FP waiver. [Detail][apps].

  - [x] **T01a — Repair two independently reproduced inline E3101 paths**.
    [Floating builtin operands](tests/fixtures/inline-value-transport.c) now
    transport earlier values across later operands' CFG edges; [integer-complex
    parameters/results](tests/fixtures/inline-complex-parameter.c) use complete
    pair conversion, assignment and result storage. Both explicit always-inline
    cases fail on the prior release, independently of K03a's new eligibility.
    Both interpreters and all GCC/Clang/EvoCC O0/O2 native controls pass; validator
    and initializer rules are unchanged. [Measured record](tests/fixtures/private-leaf-substitution.out).

  - [x] **T01b — Select the typed call machine from reached signatures**, 2026-09-28.
    An independent positive control exposes E3007 for unused pointer/FP
    parameters whose actual arguments are integer constants. The shared C-HIR
    reachability scan now includes call signatures, preserving alias/transitive
    calls, unevaluated sizeof and uninitialized-read checks. Both interpreters
    and 54 fixed native controls pass. Nominal floating signatures now use the
    existing E3049 rather than generic E3101; no new numeric type is executable.
    All eight original half-interface replays retain named refusal. The 1,876
    affected tests pass with three historical ignores; Clippy/fmt and the
    changed gate against `1160db6` pass. No kernel/native acceptance is inherited.
    [Minimal cases, negative attempts and identity](tests/fixtures/typed-call-requirements.out).

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

- [ ] **T03 — Spec/implementation/acceptance consistency audit** — `OPEN`, P1.
  Map current clauses to positive/negative tests and original evidence. Start with
  stale EvoMIR E4006/E4008 diagnostic text versus implemented indirect branches
  and pointer differences; distinguish errata from new authority. Include operand
  traversal, effects, metadata mutation and independent proof reconstruction.
  Close discrepancies or assign specific child tasks, not a blanket “spec done”.
  [EvoIR](spec/evoir.md), [EvoMIR](spec/evomir.md).

  - [x] **T03a — Synchronize retired diagnostic boundaries**.
    Errata align E3033/E4003 with Appendix D's approved hosted 128-bit division,
    E4006 with full indirect edges and E4008 with pointer subtraction. Existing
    arithmetic guards, invalid-target checks, local SSA and runtime-library
    dependencies remain intact; no new feature is authorized by the edit.
    The 96-bit refusal test is named precisely rather than implying 128-bit
    values are rejected. The larger metadata/proof audit remains T03.

  - [x] **T03b — Remove the obsolete static-pointer emission refusal**.
    EvoMIR §8's introduction still contradicted the already-approved 0.12/0.15
    relocation rules and its own E4005 withdrawal. A text assertion reproduced
    that contradiction; the erratum now names symbolic/addend and block targets,
    without widening frontend initializer admission. All four
    [native-emission shape tests](crates/evocc-mir/tests/section8_relocations.rs)
    and four [IR relocation/address tests](crates/evocc-evoir/tests/relocations_and_addresses.rs)
    pass. This is clause consistency, not a new implementation or full T03 closure.

  - [x] **T03c — Rehash semantic proof-mutation candidates** — 2026-09-23.
    The existing [independent replay tests](crates/evocc-proofcheck/tests/copy_is_opaque.rs)
    now submit well-typed canonical changes with matching producer-authored
    output digests. All 37 mutations still fail with semantic exit 3, including
    same-priority lifecycle order, TLS address identity, FP/ABI, atomic order,
    visibility and indirect-edge bindings. Fourteen valid replay controls and
    the 44-test affected gate pass. This strengthens acceptance; no checker
    implementation defect or whole-kernel certificate closure is claimed.
    [Scope and field matrix](tests/fixtures/proof-semantic-metadata.out).

  - [x] **T03d — Synchronize dynamic-stack forms and frame clauses**, 2026-09-26.
    The MIR spec regression went RED while the executable allocation/release
    control passed. EvoMIR 0.41 now reflects the existing Appendix D contract:
    stable fixed storage, explicit runtime allocation, disjoint outgoing area
    and top-only release without popping function-lifetime objects. The 28-test
    lowering suite, native spill variants, three object-lifetime and six VLA CLI
    tests pass; affected gate **1010 PASS / 0 FAIL / 4 historical ignored**,
    Clippy/fmt green. This closes an erratum, not a new compiler feature or T03.
    [Exact scope and commands](tests/fixtures/dynamic-stack-contract.out).

  - [x] **T03e — Close missing instruction sentinels and main-grammar errata**, 2026-09-28.
    An independent enum inventory goes RED at 52/59 instantiated variants;
    the expanded non-executable field sentinel now covers all 59 and 143/143
    registered fields, including nested assembly, declarations and relocations.
    Ten added operand-remapping cases preserve edge identities. A valid byval
    varargs proof replays, while four rehashed semantic forgeries are rejected.
    Main grammar tests expose and repair stale pointer, f128, dynamic-stack,
    varargs and other already-approved Appendix D spellings. Five text/field
    tests, 17 checker tests and the 1,644-test affected gate pass; Clippy/fmt
    pass. This closes verification gaps and errata, not new compiler features
    or the entire spec audit. [Exact scope and evidence](tests/fixtures/section9-contract-audit.out).

  - [x] **T03f — Enforce return/prototype compatibility at every written site** — `DONE`, 2026-09-28.
    Twenty small invalid programs previously passed parsing, including dead
    arms and indirect calls. The shared assignment rule now rejects these
    before execution/lowering; void/value mismatches receive E1207 without
    reusing E1205/E1206. Follow-up regressions preserve by-value va_list and
    full-width integer pointer typing instead of weakening the new check.
    Both interpreters, 80 explicit-Werror reference controls, 12 native legal
    controls and ten unchanged original musl object emissions/assemblies pass.
    All 1,339 original musl sources pass a read-only parser sweep; this is NOT
    1,339 compiled/assembled objects. The ten-crate gate passes 2,362 tests
    with zero failures and four historical ignores; Clippy/fmt also pass.
    [Exact scope and preserved failed attempts](tests/fixtures/return-conversion-constraints.out).

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

  - [x] **T04a — Reject obsolete generated-module survey records**, 2026-09-26.
    An in-place module-to-built-in change leaves stale `.mod.o.cmd` files.
    The inventory now checks those records against a safe, unique current
    `modules.order`, rejecting rather than silently dropping them. Two RED
    regressions and all 243 K1 tests pass; Ruff passes. The retained prior
    composition still enumerates 6,765 records / 745 module metadata records.
    The completed new reference reconciles exactly one obsolete IOMMUFD module
    record: raw 7,040 → current 7,039, with five old generated files recoverably
    archived and the built-in object untouched. T04 acceptance remains pending.
    [Minimal reproduction and boundary](tests/fixtures/kernel-stale-module-inventory.out).

  - [x] **T04b — Distinguish same-binary HOSTCC from target compilation**, 2026-09-26.
    `HOSTCC=gcc-14` exposed 37 host-tool objects in the old compiler-name-only
    classifier. Frozen Kbuild host-header and target `__KERNEL__` markers now
    identify the roles; directories and `-fomit-frame-pointer` are not waivers.
    The two RED variants now pass, as do all 246 K1 regressions and Ruff.
    Original-command audit retains all 7,002 target-C and 37 named host records;
    no target source/object is deleted or dropped, no flag is newly ignored.
    [Exact reproduction and accounting](tests/fixtures/kernel-host-command-role.out).

  - [x] **T04c — Recover verified prepared-input compilation batches**, 2026-09-27.
    An interrupted second pass previously lost every completed compilation.
    Recovery now binds the entire prepared input snapshot, compiler, command,
    assembler image, successful preparation and retained object; symbol parity
    and the final generated-input fixed point are still checked. Fault-injection
    tests and all 250 K1 Python tests pass. The real 65-object C reduction resumes
    64 verified objects, compiles one, links and returns the new header value in
    all 65 functions. This is recovery validation, not kernel acceptance; the
    already-running worker loaded the old implementation and is unchanged.
    [Fixture, invalidation matrix and result](tests/fixtures/kernel-prepared-consumer.out).

  - [x] **T04d — Include late host-generated inputs in preparation** — `DONE`, 2026-09-28.
    Modpost regenerated ASN.1 sources/headers, CRC tables, consolemap, logo and
    RAID6 tables after the prepared-input pass, rebuilding 13 consumers with GCC.
    Current generated contents match pristine, but that does not authorize a
    provenance exemption or blind timestamp touch. The shared path now runs
    the original vmlinux/module producer graph before freezing consumer inputs.
    A real one-C-object Make reduction goes RED -> GREEN: header 7 -> 37,
    native exit 0, subject hash unchanged after the late producer. All 251 K1
    tests pass and the actual Kbuild preparation target exits 0. The fresh full
    7,002-object composition `20260927T135325Z` now completes the real producer
    fixed point, modpost, final link, 847 module links and nonce-bound boot.
    All installed target-C hashes survive: zero GCC replacements. The prior
    13 replacements remain a failed observation, not rewritten evidence.
    [Exact scope and evidence](tests/fixtures/kernel-late-generator.out).

  - [x] **T04e — Recover producer-replaced objects without crediting them**, 2026-09-28.
    Real recovery stopped because preparation replaced 17 first-pass working
    objects. Recovery now labels every row prepared-pending, without installed
    provenance, and requires prepared validation and installation. Two RED
    assertions, 275 K1 tests and a real 65-object native recovery pass; full
    coverage still requires the post-link audit. No object is exempted. The
    full run retains the same compiler/configuration; T04/K13/K14 stay OPEN.
    [Reproduction and identities](tests/fixtures/kernel-prepared-phase.out).

- [ ] **T05 — Final compiler-health and documentation gate** — `OPEN`, P1.
  After the last semantic repair, run workspace tests, Clippy `-D warnings`, fmt,
  relevant Ruff/Python checks, grammar/layout corpora and isolated offline replay.
  Keep historical ignored tests explicit. Refresh receipts, run
  `python3 tools/render-status.py`, update README/this tracker and commit the
  bounded result. Passing this gate alone does not close B10/K13/K14.
  [Workflow](CONTRIBUTING.md#the-change-workflow).
  September 28's [post-repair checkpoint](tests/fixtures/compiler-health-20260927.out)
  at compiler source `13082c6` passes **2784 workspace tests**, Clippy/fmt, **563 Python tests**, Ruff,
  54/54 grammar shapes, both frozen layout corpora (**128 + 22**) and **13**
  isolated offline replays. Four historical Rust ignores remain explicit.
  This verifies current code, not a last-semantic-change or final campaign claim;
  T05 stays OPEN until its remaining-task dependencies are actually closed.
  The later [pinned checkpoint](tests/fixtures/kernel-regression-identity.out)
  includes the compare-immediate repair and passes **2807 Rust tests**, four
  historical ignores, **274 K1 + 315 ladder tests**, Clippy/fmt/Ruff, both layout
  corpora, 54 grammar shapes and thirteen isolated offline replays. The same
  final-campaign boundaries apply.

  - [x] **T05a — Preserve multiline benchmark command failures**.
    Six RED assertions showed that trailing newlines/comments detached log
    redirection and replaced exit 7 with exit 0 in build/relink/test wrappers.
    Shared command grouping now preserves output and status; configuration
    failures also retain their `set -e` behavior. Unit tests cover all paths.
    The invalid cJSON flight `20260923T073305Z` is superseded, not accepted.
    This repairs the harness, not all historical benchmark claims; B10 remains.

  - [x] **T05b — Run the workspace gate for manifest-only changes**.
    Five RED assertions showed that changes only to `Cargo.toml` or `Cargo.lock`
    incorrectly took the no-crates early exit, including a simulated failing test.
    Manifest selection now precedes that exit. The actual shell script is covered
    by offline stand-ins for committed/dirty changes, reverse dependencies,
    non-Cargo no-op changes and failure propagation in
    [regressions](tools/ladder/tests/test_gate_changed.py). This does not substitute
    for the final real workspace gate.

  - [x] **T05c — Reject partial or unlinked toybox acceptance**.
    REDs reproduce truncated/shared failures, duplicate-name loss, stale binaries,
    missing tests and stale bisection results. The adapter now replays the exact
    original object-link argv, audits installed objects, uses upstream
    `VERBOSE=all` with normal-return/status checks, and confirms the final selected
    build uncached. The real pinned harness and targeted chattr replay validate
    the path; historical receipts are explicitly invalidated for full coverage.
    [Regression tests](tools/ladder/tests/test_toybox.py), [record](tests/fixtures/toybox-filesystem-flags.out).

  - [x] **T05d — Select Rust tests when their external fixtures change**, 2026-09-24.
    Eleven RED assertions exposed missed `include!`/`include_str!`/`include_bytes!`
    dependencies outside crates, including a concealed failing test. Selection
    now follows literal/nested inputs and reverse dependencies; unresolved
    expressions fail closed by selecting their owner. Deleted/dirty inputs,
    raw literals, cycles and the actual compared-value CLI source are covered by
    [12 offline gate tests](tools/ladder/tests/test_gate_changed.py).
    All 248 ladder tests, Ruff, shell syntax and the real 7-test CLI suite pass.
    Stand-in Cargo checks prove selection/error propagation, not a workspace
    PASS; the final health gate remains T05.

  - [x] **T05e — Show new kernel attempts beside historical acceptance**, 2026-09-26.
    Three RED status-rendering controls hid RUNNING/ERROR/INTERRUPTED behind a
    retained old K6 PASS. The ASCII and generated status now show the selected
    attempt's state/run separately, without erasing historical results or
    claiming live-process monitoring. Two regression tests (including all
    three states) and all 250 ladder tests pass; Ruff and render/check pass.
    An unrelated application run does not relabel an older kernel attempt.
    [Regression](tools/ladder/tests/test_bench.py).

  - [x] **T05f — Reject stale archives and incomplete musl runtime evidence**, 2026-09-27.
    Failure-injected shell/adapter tests reproduce swallowed make errors, stale
    markers, incomplete runtime inventories, object replacement and cached
    bisection acceptance. musl now requires a fresh final trial, complete
    upstream runtime results, unchanged selected objects/watchdog and identical
    rebuilt/installed archives. Reference objects are restored on every exit.
    A separate controlled-timestamp cJSON RED requires its generated archives
    to be rebuilt; the unchanged full cJSON replay passes with **23/23** subject
    objects and no make replacements. These harness checks do not promote the
    old musl receipt: its original full campaign remains NOT RUN after repair.
    [Regressions and scope](tests/fixtures/benchmark-relink-boundaries.out).

  - [x] **T05g — Isolate the zlib benchmark from Python's standard codec**, 2026-09-27.
    Three fresh-process REDs reproduce search-path mutation, missing `subject`
    imports and a broken `gzip` import. The historical entry point now delegates
    codec imports outside its own real directory, preserving the caller's
    path; CLI and named benchmark loading retain their prior Subject. Six
    controls pass on macOS and sealed Linux, including symlink and byte-roundtrip
    cases; all 290 ladder tests and targeted Ruff pass. No benchmark result or
    compiler behavior was changed. [Failure and repair](tests/fixtures/benchmark-import-isolation.out).

  - [x] **T05h — Avoid host-stack overflow on long arithmetic chains**, 2026-09-27.
    The [600-addition C reduction](tests/fixtures/inline-deep-expression.c)
    parses but aborts the debug compiler in both interpreters and assembly
    lowering. Explicit continuations reuse the existing rewrite, task-machine
    and binary-lowering paths; constant admission, selected effects and UB
    guards remain. Eleven focused tests, twelve native controls and the
    1,869-test affected gate pass, with Clippy/fmt. Original page-table assembly
    is byte-identical; no running build's compiler or installed object changes.
    This is bounded debug robustness, not a release-kernel miscompile or a
    guarantee for arbitrary AST depth. [Evidence](tests/fixtures/inline-deep-expression.out).

  - [x] **T05i — Reject incomplete assembler-container execution** — `DONE`, 2026-09-28.
    Actual exit-17 failure injection proves the current K1 assembler can accept
    preceding OK records. The [isolated patch](tests/fixtures/kernel-assembly-verdict.patch)
    rejects failed processes, duplicate/missing/foreign verdicts and missing or
    empty objects, records exit codes and separates legacy cache keys. All 53
    affected tests and actual exit-17, empty-object and normal-exit controls pass
    on the candidate. Empty files cannot become verified cache hits. Integration
    waits until process 80898 exits, preserving its implementation identity.
    The integrated path then passes all 270 K1 tests, Ruff, three actual native
    failure/normal controls and two original KVM object replays with the pinned
    compiler. Existing receipts and the completed kernel build do not inherit
    this later hardening.
    [Exact reproducer, hashes and integration boundary](tests/fixtures/kernel-assembly-verdict.out).

  - [x] **T05j — Harden the other whole-application assembly verdict path**, 2026-09-28.
    The `subject.py` adapter also accepts actual successful assembly followed
    by runner exit 17. It now requires a successful process, exact complete
    unique verdicts and nonempty object files; stale/duplicate/foreign/missing
    observations cannot publish `assembled`. Eight unit scenarios and four
    real sealed runner controls cover RED, GREEN and normal execution; all
    310 ladder tests pass. This fixes that adapter, not the separately queued
    live K1 path T05i or a historical full-campaign result.
    [Small reproduction and exact identities](tests/fixtures/command-projection-packed.out).

  - [x] **T05k — Pin the actual host toolchain and revalidate current compiler code**, 2026-09-28.
    The ambient stable alias had moved to Rust 1.98.0; the repository now pins
    the contract's installed, hash-verified 1.97.1 with Clippy/rustfmt. A RED
    also shows toolchain-file changes incorrectly selected no crates; either
    filename now selects the whole workspace. All 2,801 Rust tests pass with
    four historical ignores, Clippy/fmt pass, 315 ladder tests pass, and fresh
    layout (128+22), grammar (54 shapes) and thirteen isolated offline replays
    pass. Six fixture assemblies match their previously native-tested bytes.
    The running kernel's frozen binary still contains the 1.98.0 library marker;
    it is not retroactively called a pinned-host build. The final campaign must
    record its actual pinned compiler. This does not close the T05 parent.
    [Tool identities, REDs and exact scope](tests/fixtures/pinned-host-toolchain.out).

  - [x] **T05l — Compare paired regressions by canonical program identity**, 2026-09-28.
    Three small tests expose duplicated collection prefixes, ordinal-based
    mispairing and ambiguous IDs. The shared comparison now indexes canonical
    IDs and rejects duplicate/foreign inputs. All 274 K1 tests pass; the actual
    retained selected receipt recomputes to `sched_ext:runner`. Historical
    observations and verdicts remain unchanged. This is report integrity, not
    a kernel repair. [RED, GREEN and health](tests/fixtures/kernel-regression-identity.out).

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

  - [x] **R01a — Policy-aware persistent optimization and proof provenance** — `DONE`, requalified 2026-09-29.
    Review reproduced three failures in the sealed-certificate suite, omitted by
    the previous four-crate gate. Commit `31234f5` repairs fixture placement,
    recovery-proof accounting, inherited scratch-file limits and checker
    packaging/identity admission. The full workspace gate passed: 2,850 Rust
    tests, zero failures, four existing ignored campaigns, all-target Clippy
    with warnings denied and formatting. The Python gate passed 118 tests;
    an isolated fresh-target package gate passed 560 tests. These are distinct,
    overlapping qualification scopes, not a unique-case sum. R02 application
    admission and the three effect-ceiling preflights remain open.
    [Review closure and exact scope](docs/archive/notebook/eqsat-review-closure-20260929.md).
    Implemented design P0: one driver/evaluator persistent chain runner with policy
    identity including transaction count, proof/report export and wrapper-owned
    verification. Separate transaction result, stop reason and root counts;
    validate the closed state/rejection-reason tables. Distinguish completed proof
    rejection (PROOF_INVALID, exit 3), malformed bundles (CERTIFICATE_MALFORMED,
    exit 2), checker resource/infrastructure unknowns and missing/mismatched
    bundles; fault-inject each reason and merged exit-2/exit-3 accounting, plus
    missing-proof success versus explicit rollback. Unknown is not proof failure
    or success. Preserve transient search
    counters before EVP overwrites them; separate admission region/node quotas,
    recorded fuel/markers and actual matcher work, including legacy knob coupling.
    Preserve prior accepted regions on budget exhaustion. Any internal/proof/chain
    failure rejects whole-chain publication and staged state, not a successful
    prefix or silent baseline. Test chain parity, second-transaction rejection,
    budget-partial and zero-region cases, artifact identities and denominators.
    Fault-inject apply/history failure: valid zero proof cannot erase producer
    ROLLED_BACK_ERROR telemetry. An eligible no-rule case remains a legal no-op;
    independent eligibility counts warn but do not certify fallback classification.
    The earlier, insufficient gate had only 243 scoped Rust tests, 78 Python tests and the retained Darwin ARM64
    native proof bundle (two independently replayed transactions, executable
    exit 0). Three long Rust campaigns were ignored in that narrower gate; the
    full workspace gate has four ignored/NOT_RUN campaigns. The
    native smoke is not Linux-target or G2 acceptance. Integration also reduced
    and fixed fresh-definition binding and stale normalized-fact failures.
    [Implementation and commands](docs/archive/notebook/verified-eqsat-implementation-20260929.md);
    [sealed fixture](tests/fixtures/eqsat-chains/verified-chain/README.md).

- [ ] **R02 — Q2 object code-size evaluation** — `LATER`, P3.
  V1 primary objective is size under revision 7, using pinned programs and result
  checks. Compare GCC/Clang `-Os`/`-O2` without a parity threshold. Latency is
  NOT_MEASURABLE_ON_THIS_LANE; no timing rerun/retuning. Final G2 and whole-scope
  acceptance remain separate from the six-TU pilot. [Roadmap][roadmap].

  - [x] **R02a — Performance manifests and measurement runner** — `DONE`, 2026-09-30, bounded registered Linux pilot.
    Design P1 is implemented through `tools/bench.py` and existing adapters/locks.
    The frozen P0/zlib/zstd/cJSON pilot completed 576/576 timed executions and
    48 warmups; CPU/RSS, ELF sections/stripped files, compiler/checker costs and
    every output are retained with explicit measurement scope. The independent
    auditor replays 138 certificates across 102 optimized chains and verifies
    all 108 checked-twin admissions. Ten read-only capture mutations are rejected;
    the full Rust workspace passes 2,861 tests (four historical ignores),
    Clippy and formatting; full Python ladder discovery passes 449/450 tests,
    with the real-capture skip executed separately (6/6 tests pass).
    Mixed-link coverage and host-debug compile costs
    are explicit: this is not a timed GCC comparison or whole-library PASS.
    [Qualified protocol, failures and claim ceiling](docs/archive/notebook/eqsat-r02-linux-pilot-20260930.md);
    [tracked checkpoint](tests/contracts/eqsat-r02-pilot-receipt-20260930.json).
    Implemented requirements:
    Freeze effective UB-check/lowering profiles, shared chain identity, semantic
    flags, scoped budget units/mapping, checked-twin common IR/chain digests,
    closed rejection accounting, object coverage,
    timing boundaries, raw metrics,
    resource caps and separate performance receipts. REDs cover stale/mismatched
    artifacts, missing results, invalid metrics, interference and cleanup ownership.
    Historical September 29 foundations (superseded by the qualification above):
    the foundation batch removes repeated terminal no-op
    interpretation, duplicate per-function Admission construction and the extra
    whole-chain staged-state clone. Every changed module is still interpreted;
    every certificate, including the terminal no-op, is replayed. Checker
    identity checks retain both full reads/hashes and the final commit boundary.
    Policy-report v3 separates producer, checker, identity and wrapper wall time;
    v2 historical reports remain readable without fabricated timings.
    The frozen-plan audit covers checked native A/A declarations and
    artifact/source identity. The next bounded native adapter now implements
    exact-IR MIR transport, registered Darwin P0 A/A execution, per-attempt
    result/artifact checks, owned capture/lock/retention and raw paired statistics.
    Its bounded native pipeline was exercised after an explicit SDK/link-editor
    repair: 38 completed steps, 24 result-checked timed runs and four warmups of
    one binary; read-only capture audit passed. The earlier link failure is
    retained with zero timing samples. This is not Linux/application performance
    acceptance or an EqSat speedup; R02a was OPEN at that checkpoint. The broader
    pilot requirements named there are now exercised within the registered scope above.
    [Foundation scope and gates](docs/archive/notebook/eqsat-r02a-foundations-20260929.md).
    [Native pipeline and limits](docs/archive/notebook/eqsat-native-calibration-20260929.md).
    September 29 checkpoint: the default-cap diagnostic collector reuses the
    bench entry point and lock, with committed bounded input manifests and
    scoped/unknown counters. Native performance manifests and runners were still
    open at that checkpoint; the generated-IR collector smoke was not their acceptance.
    EqSat-off export now records a zero-transaction identity chain over the
    exact codegen IR. Independent byte-identity audit reports zero EVP proof
    successes; checked/unchecked fan-out was still open then and is now closed by R02d.
    [Scope and protocol](docs/archive/notebook/eqsat-cap-census-20260929.md).

  - [x] **R02b — Opportunity/effect-ceiling preflight and A/A plus A/B pilot** — `DONE`, 2026-09-30, bounded pilot with negative/inconclusive outcomes.
    **Review correction:** DONE here describes the historical bounded implementation
    and capture, not a qualified timing protocol. The Linux lane reused Darwin's
    sample count without its own calibration and ran millisecond fresh processes.
    Its timing conclusions remain INCONCLUSIVE and cannot be used as latency
    acceptance. R02a1 and R02b1 below are mandatory follow-ups; the original raw
    capture/manifest remain unchanged. Code-size and checked-twin evidence stands.
    The first default-limit table covers six subject TUs and twelve transactions:
    no transient, admission or chain cap hits. Actual matching work remains
    NOT_MEASURED. The selected-pair census enumerates 512 feasible signatures;
    16 fixed-order suite evaluations at schedules 1/8 observe eight code classes.
    Two bounded rule syntax cells complete (5,994 visits, 1,222 fuzz survivors);
    two universal template controls prove, but no new rule is installed or
    qualified for utility. LLVM19's 34 reduced-node hints are not equations.
    Thirteen multi-rewrite regions expose eleven shortcut compositions without
    demonstrated fuel/matcher savings. Policy/rule/shortcut verdicts are all
    INCONCLUSIVE; no AI campaign is admitted. All 24 timing comparisons are
    complete: only 2/12 A/A are within the frozen effect floor and 12/12 A/B
    are inconclusive. Code-section reductions are real, not latency evidence.
    [Results, resource stops and retained failures](docs/archive/notebook/eqsat-r02-linux-pilot-20260930.md).
    Implemented preflight contract, with every unresolved measurement retained:
    After R01a/R02a, first report whether default transient and certificate caps
    actually bind, with separate EVP fuel and chain counts. Freeze inputs and
    enumerate acceptance/rejection of transient-selected root/replacement pairs,
    not weight-driven alternative selection; test expected add/mul/shl byte ties and
    surrounding-code exceptions. Zero-weight certificate reservation only applies
    at a binding admission cap, not to transient work already spent.
    Also census residual misses after lowering/current rules using a pinned,
    semantics-qualified external reference plus a frozen bounded rule enumerator.
    Freeze a finite grammar lattice, visitation order, per-cell/total budgets and
    termination. Each discovery-qualifying cell needs an enumeration cap at least
    ceil(k * B_AI), k >= 1, where B_AI is the planned whole-track AI evaluation
    budget across seeds/restarts in the same suite-evaluation units. Require actual
    exhaustion at that scale; smaller quotas, wrong units or earlier resource
    stops cannot qualify SEARCHABLE. Revalidate after increasing B_AI.
    Record constants/widths/guards, proof/matcher applicability and
    materialization. Fuzz is filtering, not completeness. Beyond-enumeration
    discovery needs a useful template-proved witness in a healthy budget-exhausted
    cell and the same search space for non-AI. A completed witness-containing cell
    permits finite enumeration/efficiency claims, not discovery; crashes, unresolved
    evidence and out-of-scope depth cannot justify expansion of this campaign.
    Allow bounded R03f/c qualification probes, not a bulk
    discovery campaign, to resolve the preflight. Freeze separate policy/rule/
    shortcut searchable, enumeration-sufficient, no-effect or inconclusive verdicts.
    Calibrate A/A and compare matched A/B; no campaign bypasses its track verdict.
    Lowering-fold-disabled variants are attribution-only; three pilot families
    do not establish cross-family generalization. Retain all negative results.

  - [x] **R02b1 — Complete deterministic preflight and close review boundaries** — `DONE`, P3, 2026-09-30.
    The completed retained capture and committed audit-only supplement pass
    independent replay plus all eight real-capture mutation tests: 1,024 suite
    evaluations, 6,144 chains, 8,854 proof successes, 380 code classes and 966
    backend objects. All 1,222 templates were attempted: 1,174 proved, 48 UNKNOWN,
    none installed. Transient/certificate cap hits are zero at both schedules;
    the one-transaction schedule has 2,326 chain-limit stops, while all 3,072
    eight-transaction chains stop at NO_CHANGE. The new `(add, shl)` comparison
    leaves policy closure INCONCLUSIVE; rule/shortcut utility also remains
    INCONCLUSIVE. No AI or native timing claim follows. The original failed
    auditor, raw experiment and historical timing pilot are retained unchanged.
    [Repair, versioned replay and cap table](docs/archive/notebook/eqsat-r02-review-repair-20260930.md#independent-completion-acceptance).
    First close the four September 30 REDs: integrity/resource failures cannot
    become template UNKNOWN, zombie-only process groups cannot count as live,
    every UB diagnostic guard is counted independently, and inconsistent public
    wall scopes report an error without panic/wraparound. Then freeze the same
    canonical six-TU population, all 512 selected-pair signatures at schedules
    1/8 and all 1,222 universal fixed-template candidates. Reuse content-addressed
    evidence, not another source/workspace clone. Recompute code-size classes,
    caps, proved/UNKNOWN counts and residual applicability with an independent
    audit. No native-time search fitness, arbitrary tactic scripts or new-rule
    installation. Unresolved grammar/matcher/shortcut utility stays explicit.

  - [x] **R02a1 — Calibrate and freeze a Linux steady-state timing protocol** — `DONE`, P3, 2026-09-30, bounded measurement with a frozen inconclusive result.
    The protocol, typed/mutation gates and default-control composition are
    implemented. Real mixed-link build and A/A pass independent audit: ten
    A/A comparisons admitted, 312 timed samples. The separately preregistered
    A/B ran once at the frozen 5% gates: ten comparisons, 312 timed samples
    and ten prior checked admissions, all result checks pass, but all ten
    latency outcomes are INCONCLUSIVE. The owned VM exited normally. The
    Python-version diagnostic rounding failure is retained; unchanged
    split-runtime replay verifies the original samples without a gate change.
    [Frozen result and receipt identities](docs/archive/notebook/eqsat-r02-steady-protocol-20260930.md#frozen-single-ab-result).
    No rerun or larger budget follows. Final R02/Q2/G2 is not accepted by this
    bounded task. Formal policy/rule/shortcut verdicts remain INCONCLUSIVE,
    with all R03 activities NOT_ADMITTED; audit tooling is capped.
    [Adjudication and three owner decisions](docs/archive/notebook/eqsat-r02-review-repair-20260930.md#formal-three-track-adjudication-and-stop-decision).
    Implemented protocol retained for reproducibility:
    After R02b1, make each measured process repeat internally for at least 1–2
    seconds with setup/startup excluded. Pin a guest CPU, keep host capture/hash
    work outside measured intervals, and retain every correctness result. Freeze
    a separate Linux A/A plan first; choose workload repeats and pair count from
    that calibration alone, then commit a new A/B manifest. A/A must satisfy its
    declared precision gate before A/B. Failed calibration is a measured limit,
    not permission to tune against A/B or retry the old millisecond protocol.

  - [x] **R02c — Frozen object-code size comparison** — `DONE`, 2026-10-01, TU scope.
    Owner unlocked S5 independently of S4: the same six original TUs,
    GCC-14/Clang-19 `-Os` and `-O2`, EvoCC unchecked release-context and checked
    diagnostic-contract builds separately. Per-family distributions/worst
    regressions, no cross-family average or parity threshold. Linked text is
    secondary. No further VM A/B timing; latency NOT_MEASURABLE_ON_THIS_LANE.
    24 GCC/Clang object pairs are byte-identical; original A/B object pairs
    rechecked under S1. Full workspace 2,870 PASS / 0 FAIL / 4 ignored.
    [Frozen size table and claim ceiling](docs/archive/notebook/eqsat-size-references-20261001.md).
    This is TU size evidence, not new full-application correctness or parity.

  - [x] **R02d — Implement and verify the stated native UB-check profiles** — `DONE`, 2026-09-30.
    `--ub-checks=checked|unchecked` now reaches the driver and MIR; only native
    UbAssume enforcement is omitted in unchecked mode. Source obligations,
    interpreter diagnostics, other predicate users, explicit traps and FP checks
    remain. Real native trap/defined-parity tests and both interpreter profiles
    pass. The Linux pilot has 108/108 admitted twins (baseline twelve + policy
    96), each bound to the same canonical verified IR, chain and bundle, with
    a passing checked execution for that input before unchecked use. Profile
    identities stay separate; EqSat-off uses a zero-proof identity chain.
    [Native evidence, exact boundaries and mutations](docs/archive/notebook/eqsat-r02-linux-pilot-20260930.md).
    Implemented explicit effective-profile plumbing and receipt identity;
    no switch or source/IR obligation is invented/deleted by the harness.
    REDs: ignored flag, missing identity, checked traps,
    defined-input parity and interpreter contract preservation. Every unchecked
    variant/input must have a passing no-trap checked twin. Fan out one canonical
    verified IR and immutable chain/bundle to both MIR profiles; require exact
    common digests, not just equal source/input/policy manifests. Reject mismatched
    IR/chain or missing/failed twins; native profile/cache identities remain
    separate. Include the EqSat-off identity-chain control. That is not
    proof against unmodeled UB. Measure checked
    and unchecked separately; GCC sanitizer coverage is not assumed equivalent.

  - [x] **R02e — Guard-predicate folding domain, separately reviewed** — `DONE`, 2026-10-01, exact first slice only.
    Implementation `595272a2`: seven total predicates, separate `g0.ub.v1`,
    existing known-bit/all-edge seed facts, independent checker and seven
    kernel-only parametric A2 transfer proofs. P0 and interpreters unchanged;
    IR obligations/effects remain, MIR elides only direct canonical true.
    Review repair `7b385567`: full workspace **2,881 PASS / 0 FAIL / 4 existing ignored**,
    Clippy/fmt and clean-tree Lean gate green. All six Rust/Lean groups rerun:
    **1,487,689** comparisons (1,325,045 P0 + 162,644 guards), zero differences;
    subset receipts merge, and nonzero transfer requires a known one bit.
    Six chains / ten independently replayed certificates: zero completed
    rejections, bundle failures or unknowns. 24 byte-identical object pairs;
    24 original mixed-link correctness executions pass.
    Certified within-family text savings: zlib **256**, zstd **984**, cJSON
    **28** bytes, separate from P0/DCE and frozen diagnostic ceilings
    **952 / 1,608 / 80**. Maximum per-TU saving **0.800%**, below the expected
    1–3%; no relaxation follows. Unchecked sizes are unchanged. No second slice
    is authorized, designed or scheduled; no R03/timing/kernel run.
    [Six-TU table, commits, gates and claim ceiling](docs/archive/notebook/eqsat-certified-guards-20261001.md).

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

- [ ] **R04 — x86-64 userspace backend/ABI milestone** — `LATER`, P3.
  Implement the gated SysV/backend slice after ARM64 acceptance priorities,
  retaining shared IR semantics. Close with native userspace ABI/corpus evidence.
  Full x86 kernel support remains a separately declared stretch, not an implied
  consequence of ARM64 boot. [Roadmap][roadmap].

- [ ] **R05 — Release, reproducible bootstrap and proof-claim audit** — `LATER`, P3.
  Audit the remaining release deliverables, portable builds, packaged artifacts,
  reproducibility and proof boundaries against the roadmap. Give each missing
  deliverable a child task before release. A Rust compiler compiling a C compiler
  is not literal self-hosting; tested scalar certificates are not a proof of the
  entire compiler. The October 4 V00–V06 correctness path is a mandatory first-
  public-release dependency, not optional follow-up. Close with the actual release
  checklist and artifacts only after V06. [Roadmap][roadmap].

- [x] **R06 — Pinned Doom integration** — `DONE`, 2026-09-28, bounded headless profile.
  doomgeneric `dcb7a8db...` plus Freedoom 0.13.0 Phase 1: all 80 engine units
  and the platform adapter compile/link with EvoCC, zero target-C fallback.
  Two bundled demos, two repeats, EvoCC/GCC14/Clang19 produce identical full
  350-frame sequences (12 runs / 4,200 captured frames). The common bench
  entry owns execution; original 15-lane populations remain unchanged.
  Explicit GNU11/O2/-fwrapv handles the original signed angle multiplication;
  a source-bound, shared one-line video initialization repair removes initial
  wipe nondeterminism also reproduced by GCC self-repeats. The unadjusted
  attempts remain FAIL, not unchanged-upstream PASS. No compiler language
  expansion was necessary. Window/audio/live input, all maps and other ports
  are not covered. [Receipt](tests/contracts/ladder-doom-receipt.json),
  [profile and checks](tests/fixtures/doom-headless.out).

## First public release verification

Authority and exact envelope: [V1 verification plan](docs/archive/lean-plan.md#first-public-release-verification).
V00 to V06 are the seven release-blocking tasks. The first result must
be a proved baseline path for a declared subset, then a proved optimized path;
neither an allocation-only theorem nor existing rule proofs close the parent
release requirement. Keep unsupported programs available in the ordinary
compiler without implying a proof. No new registry, fact domain or AI campaign.

**Current state (2026-10-06).** V00 to V08 are complete in the frozen `core-v1`
scope. The result is a proved,
fail-closed compilation path for the listed Core C-HIR subset and target, with
an explicit trusted boundary. Existing scalar rewrites compose with that path.
Proof-required mode publishes an output only if the output is checked from
typed Core source to final assembly text. The acceptance sweep accepts all 17
pilot modules and all 107 entries of these modules, and both optimized cases.
The V06 entry below gives the first acceptance run. V07 removes the
zero-extension premise from the entry assumption. The V08 entry gives the
repeat of the sweep at `e657709b` and the native run: the output of the 17
pilot modules was executed on Linux AArch64 with no difference from the
source interpreter in 14,194 comparisons. The proofs do not cover the whole
compiler, the GNU C language or Linux.

Limits of the accepted result:

1. The entry assumption is `AbiSourceEntry`. The caller obeys AAPCS64 for the
   low `w` bits of each argument of width `w` (its register or 8-byte stack
   slot) and gives sufficient stack. The upper bits are free. The pipeline
   check refuses at stage `lowering` a post-SSA entry that does not read each
   32-bit parameter through a 32-bit register (`entryViewsOk`).
2. The stage `allocation` refuses an allocation with edge bridge blocks. No
   pilot entry has one.
3. The stage `rewrite` refuses the root-deletion (alias) certificate form. It
   refuses an extraction with more than one instruction. It does not use
   facts through add, sub, mul and comparison: the result for these facts is
   "unknown", because their existing proofs use SAT reflection.
4. The mode supports only Linux AArch64 with the default code generation
   options, and only `-O0 -fno-builtin`. The one optional rewrite option is
   `-Oeq-scalar` with a pinned `--eq-checker-sha256`. Every function in the
   output must be a selected entry. A loop entry needs an untrusted proposal
   file. Each pipeline check has one selected entry.
5. Trusted: preprocessing, parsing, elaboration and the typed source export;
   the Lean kernel, the compiled checker and its toolchain; the readers of the
   input files; the printer of the assembly AST; the Lean semantics of typed
   Core C and of the AArch64 subset; the ABI entry assumption
   (`AbiSourceEntry`); the assembler, the linker, the runtime and the hardware.
6. The native run of V08 tests the assembler, the linker, the runtime and the
   hardware on the pilot output. It does not prove them. Not covered: no pilot
   entry has a 32-bit parameter on the stack; the vectors at the step limit
   have no source result; a test is not a proof.

- [x] **V00 — Freeze the core compilation theorem and real pilot**, complete in the frozen scope; P1.
  Name the source/target states, observation/refinement relation, exact supported
  constructors, trusted boundaries and snapshot interfaces at `19bb5f7d` or a
  recorded successor. Freeze the joined/loop, spill, direct-call/stack-argument,
  checked-diagnostic cases and two unchanged real application functions with
  nearby refusal cases before implementation. Exit is an exact theorem contract
  and existing-pipeline admission map, not a new benchmark framework or this
  planning document alone.
  October 4: exact source constructor/state contract and pinned unchanged
  Doom `FixedMul` / musl `abs` excerpts are recorded in
  [the pilot manifest](tests/contracts/core-v1-pilot.json). The conservative
  C-HIR inventory is implemented; the existing pipeline has exercised 15
  spill/reload pairs and 24 incoming stack arguments. The pilot explicitly
  disables standard-name builtin substitution so it calls the original `abs`.
  Fourteen focused tests pass, including the original-call population and
  complete source-input export. All five selected entry exports decode through
  the independent Lean grammar reader. The full Rust gate passes (3,079 / 0 / 5
  ignored, plus Clippy and formatting), and the clean Lean gate passes at
  `4483ef67`. Earlier interrupted/permission-failed attempts remain recorded.
  This is not a compilation proof.

- [x] **V01 — Prove the source-to-EvoIR result checker**, 2026-10-05; complete in the frozen scope; P1.
  After V00, define the admitted Core C-HIR and EvoIR small-step semantics in the
  existing Lean project and check actual `evocc-lower` output. Prove evaluation,
  local storage, guards, calls and CFG correspondence. Reject changed evaluation
  order, conversions, diagnostics and missing effects. Preserve N02's recorded
  volatile-initializer mismatch as an out-of-profile regression/refusal, not an
  implied fix. Exit requires checker soundness, not interpreter agreement alone.
  The first primitive bridge is `lean/EvoIR/Core/CheckedInt.lean`: signed
  add/sub/mul/neg and the original `abs` / `FixedMul` expressions preserve
  value bits, signed conversion/shift behavior and overflow diagnostics.
  Five audited theorem targets are kernel-only,
  with no native axioms. The structural fragment check does not cover
  operand bindings or the surrounding CFG. Complete typed-tree export and an
  independent grammar reader now exist, with a `DECODED_NOT_VERIFIED` result.
  The separate Lean source validator independently checks binding/type/place
  consistency, scopes, initialization, calls and unsequenced conflicting local
  accesses: 44 input cases, all five exported entries and 36 ordered integer
  type pairs pass; an actually exported unsequenced-access fixture refuses.
  Its result is
  `WELL_FORMED_NOT_COMPILATION_VERIFIED`. The source small-step machine now
  executes typed arithmetic, control, scoped locals and isolated call frames.
  Twenty-two semantic regressions and the exported ten-function original-body
  closure pass; the latter agrees with both existing Rust interpreters. New
  kernel-only lemmas cover step determinism, finite prefixes, terminal
  diagnostics, an infinite loop trace and the typed arithmetic bridge. These
  are not compilation theorems. The distinct canonical EvoIR reader, token/
  access/effect checks and small-step execution now run the same frozen entries
  and whole closure; 51 target regressions pass. That closure has ten source
  functions but nine target functions because existing private-leaf substitution
  inlines `frame`; V01 must prove that actual transformation too. The first
  complete original-function proof is now `AbsCorrespondence.pilot_pair_refines`:
  complete source/target structural admission, all 32-bit arguments, preserved
  E3004, terminal observations, divergence and progress. The retained original
  exports pass `--check-core-pilot-pair`; 36 input/mutation cases pass. This is
  one exact-pair bootstrap, not the general checker or a narrower final profile.
  The owner reopened bounded verification after cleanup. The complete `abs`
  case now passes the clean Lean gate (14 kernel-only core targets) and full
  workspace (3,080 / 0 / 5 ignored; Clippy/format pass) at `7f79966a`.
  The resumed Lean gate exposed an exact-type audit expectation mismatch
  (Lean omits inferable existential binder types); a RED and exact spelling
  correction retain rejection of changed premises, widths, entries and relations.
  Generic ranked local-diagram soundness, cross-stage refinement composition,
  read-after-local-write, other-local noninterference and scope-depth preservation
  are now kernel-proved components. Administrative cycles and changed terminal
  observations are excluded by their regression proofs. This does not implement
  the generic program-pair checker. The clean Lean gate at `dbd98afb` passes
  19 kernel-only core targets. Executable symbolic integer-fragment checking now
  has universal correctness theorems, an actual `IR.execute` bridge and negative
  cases for SSA collisions, opaque/poison operands, width/operand changes,
  missing obligations and changed diagnostic reasons. It does not justify its
  own source expectation or admit a program pair. The clean gate at `88e7f458`
  records 22 kernel-only core targets. Source-directed arithmetic and conversion
  checking now preserves all six C ranks and signedness, and checked finite
  instruction segments are connected to actual `IR.Steps`. At `6321f218`, the
  clean Lean gate passes 25 kernel-only core targets (zero native axioms),
  and full workspace passes 401 suites / 3,080 tests / zero failures / five
  ignored, plus Clippy/format. The resumed slice's measured cache/capture net
  growth is 3.86 GiB, principally the single frontend Cargo cache; 51.5 GiB
  remains free. Temporary proof logs were removed; captures total 4.73 MiB.
  Source-tree-driven expression checks now connect actual source/IR steps for
  initialized reads, literals, conversions, arithmetic and admitted shifts;
  `FixedMul`'s expression, source/IR mutations and a changed-pair control exercise
  the component test. Actual
  target branch/conditional edges now have independently checked simultaneous
  parameter/token transport, including backedge/copy-cycle regressions. These
  are not full-function admission. The next step is assembling the source CFG
  and local-state diagrams with checked entry/join/backedge invariants, then
  calls and private-leaf substitution; remaining expression rows also refuse
  until implemented. No frozen function or constructor is removed from V01.
  Validator soundness and generic source/target simulation for all frozen
  functions remain open. Test step limits are not proof or divergence outcomes.
  The straight-line leaf case is now an executable proved checker:
  `LeafPair.refines` covers all original-signature arguments, actual parameter
  binding, complete return bodies/scopes, conversions, first diagnostics and
  actual target returns. `--check-core-leaf-pair` admits original `FixedMul`
  without an exact-tree whitelist; 65 runtime cases and 15 kernel examples
  exercise admission, mutations and all 36 conversion pairs. This does not
  close the branching/looping/state/call cases or V01; the frozen scope is unchanged.
  The same checker now admits all 108 AND/OR/XOR and source-type pairs through
  a parametric source-semantics bridge; 108 target-op mutations are refused.
  The clean Lean gate passes at `2fb00d28` (35 kernel-only core targets).
  The source/target return checker now covers conditional-expression CFGs:
  `ReturnPair.refines` admits both original `FixedMul` and `abs` for every
  signature value, without an exact-tree whitelist. Comparison/unary bridges
  pass 888 runtime cases; 26 branching whole-return cases and eight kernel
  examples cover changed pairs, swapped branches/joins, missing obligations,
  wrong diagnostics and target cycles. The clean Lean gate passes at `f2d51bc8`
  (39 kernel-only core targets; 1,246,159 vector answers). No full-workspace
  PASS is claimed for this Lean-only checkpoint. General expression combinations, short-circuit
  state, div/rem, statement control/loops, mutable storage, calls/private-leaf
  substitution and validator safety still block V01.
  October 5 continuation: exact symbolic local storage and stateful expression
  diagrams now feed the same `ReturnPair.refines` checker. Simple assignment,
  comma, short-circuit AND/OR, nested conditional/cast and supported numeric
  operations preserve the local store at the first diagnostic. The actual
  `stateful-return.c` exporter pair and 11 mutations pass; 81 source-machine
  comparisons, 18 storage boundaries and 26 expression examples pass locally.
  Checkpoint `74b33c80` now passes the clean Lean gate (42 kernel-only core
  targets) and full workspace (401 suites; 3,080/0/5 ignored; Clippy/format).
  Supported compound assignments and all four pre/post updates now use the same
  stateful checker: actual mixed-signedness and consecutive-update exporter pairs
  pass, and discarded-update diagnostics remain checked. The local gate totals
  21 whole-return cases and 136 source-machine comparisons; the extension's
  clean Lean gate passes at `1746ceed` (42 kernel-only core targets).
  Div/rem (including compound forms),
  statement/declaration integration, loops, calls,
  private-leaf substitution and validator safety remain OPEN; V02–V06 are not
  advanced by this component proof.
  October 5 finite-statement continuation: declarations/initialization, statement
  sequencing, `if`, and real scope exits now compose into `ReturnPair.refines`.
  The actual `statement-return.c` pair and frozen `short_circuit` pair pass;
  the latter is selected from the untouched complete `control.evo` module.
  Local regressions add 54 source-machine comparisons and 28 whole-function
  cases (24 mutations), plus 28 kernel boundary examples. The clean Lean gate
  passes at `6c532776` (44 kernel-only core targets); seven actual export pairs
  replay successfully. Full workspace passes 401 suites / 3,080 tests / zero
  failures / five ignored, with Clippy/format. Div/rem, loops, calls/private-leaf substitution and validator
  safety remain required; this does not close V01 or advance V02–V06.
  October 5 cyclic-span continuation: proof-carrying source/target cutpoint
  builders and the conditional unbounded composition theorem compile. Four
  frozen `loop_join` spans match; six zero-step roots and 16 source/target/
  association mutations refuse. The whole-loop checker still needs independent
  successor-invariant instantiation and the actual entry connection; matching
  spans are not loop admission. Focused checks pass; clean Lean/full-workspace
  gates subsequently passed at `8cf85cc6`: 47 kernel-only core targets and
  full workspace 3,080 / 0 / 5 with Clippy/format (two container-permission
  failures in the earlier sandboxed attempt remain recorded).
  October 5 whole-loop continuation: independently checked successor/store
  instantiation and actual entry binding are now implemented. `LoopPair.refines`
  covers all typed arguments and unbounded execution. The public file interface
  accepts the original `loop_join` source against the full unchanged target
  module with an untrusted cutpoint proposal; the seven finite pairs still pass.
  A divergent loop passes, while six zero-step roots and 30 mutations refuse.
  Clean Lean acceptance at `493aa9ac` passes (49 kernel-only core targets,
  1,246,159 vector answers); full workspace passes 401 suites / 3,080 tests /
  zero failures / five ignored, plus Clippy/format. Six actual exported
  division/remainder/compound-assignment pairs now reproduce the next RED.
  Div/rem, calls/private-leaf
  substitution and validator safety remain OPEN; no V02–V06 implementation or
  release claim is made. The original V01 scope is unchanged.

  October 5 division continuation: the six original div/rem/compound pairs now
  pass, plus original nested, conditional and cyclic division inputs. Numerical
  and diagnostic correspondence is proved without extending P0; repeated-check
  normalization changes proof diagrams only, with explicit source obligations
  still required. Focused tests: 144 mixed-type pairs accepted, 1,060 mutations
  refused, 11 original control cases passed; development kernel audit PASS.
  Clean Lean gate at `b718a856`: PASS, 53 kernel-only core targets and
  1,246,159 vector answers. Full workspace: 401 suites / 3,080 passed /
  zero failures / five ignored, plus Clippy/format. Calls/private-leaf substitution,
  validator safety and complete frozen-population acceptance remain OPEN.

  October 5 call continuation: four original complete pairs now pass the public
  checker, including nested checked calls and `pressure` with 32 arguments.
  `CallPair.refines` proves all-input source/IR correspondence with the real call
  stack and memory/world signature, not sampled execution. Nine execution/diagram
  controls, six source refusals and 18 target mutations pass; 61 kernel-only core
  targets pass the clean Lean gate at `1b859487`; full workspace passes 401 suites /
  3,080 tests / zero failures / five ignored, plus Clippy/format. All five named
  pilot entries replay successfully, not the combined ten-function fixture. More
  calling statement forms, cyclic/private-storage call composition, private-leaf
  substitution, validator safety and full constructor/closure coverage remain OPEN.
  The original `caller`/inlined-`frame` pair is the next recorded RED (exit 1).

  **October 5 private-leaf continuation:** that unchanged original pair now
  passes the all-input call correspondence checker. Calls in finite statements,
  declaration initialization and scope exits have program-relative source
  proofs; target unconditional edges transport all bindings simultaneously,
  preserving unused poison without admitting numeric uses. The focused gate
  passes 17 call execution controls, five pairs, six call-boundary refusals,
  three source mutations and 33 target mutations. Implementation `b7a0a70e`
  passes clean Lean (64 kernel-only core targets, 1,246,159 vector answers)
  and full workspace (401 suites; 3,080 / 0 / 5 ignored; Clippy/format).
  All five frozen named entries plus the original `caller` pair replay.
  Five well-formed calling-expression REDs in `call-control.c` still refuse
  correspondence checking, as does the complete original `main` closure.
  Calls combined with loops/private
  storage, the remaining calling expression forms, full constructor coverage
  and validator safety remain OPEN. V02–V06 have not advanced.

  **October 5 calling-control continuation:** implementation `0eec2a26` closes
  the five `call-control.c` REDs and four branching-callee pairs. Checked result
  trees preserve the selected call/return path, caller locals, tokens and first
  diagnostic. Nine complete pairs, 31 execution controls and 21 target mutations
  pass; all earlier call controls remain green. The clean Lean gate passes with
  69 kernel-only core targets; full workspace passes 401 suites / 3,080 tests /
  zero failures / five ignored, with Clippy/format. The five
  frozen primary entries and original private-leaf pair replay. New minimal
  `call-loop.c` REDs isolate calls inside loops and calls to looping functions;
  both validate and execute consistently but still refuse proof. Loop/call and
  private-storage composition, full coverage and validator safety keep V01 OPEN.

  **October 5 calls-in-loops continuation:** `loop_call` now passes on its
  unchanged source/IR. Program-relative spans compose finite actual callees
  with cyclic invariants while retaining returned tokens and advanced frame
  counters. Four original pairs (including branching, diagnostic and divergent
  cases), ten execution controls and 29 input/proposal mutations pass. Existing
  primary entries replay. `call_loop` still refuses: cyclic callees need a
  continuation-aware proof, not finite unfolding. Private storage, complete
  coverage and validator safety remain required; V02–V06 remain OPEN.

  Implementation `10f0c558` passes clean Lean (69 kernel-only core targets)
  and full workspace (401 suites / 3,080 passed / zero failed / five ignored,
  Clippy/format pass). Receipts: `cyclic_call_correspondence` in the pilot
  manifest. Build/capture disk delta +4,612 KiB; target/frontend unchanged,
  52 GiB free. Protected evidence, fixtures and Lean cache remain.

  **October 5 cyclic-callee context continuation:** `854f6b9e` proves
  `LoopDiagram.call_refines`: actual call entry, context-relative cyclic spans,
  preserved caller state/frame counters, diagnostics and unbounded composition
  with a separately proved return continuation. Six suspended-context controls
  and a residual-return negative pass; clean Lean has 70 kernel-only core
  targets. The existing audit records the full conditional theorem. Automatic
  argument/frontier and post-return expression admission is still missing:
  original `call_loop` remains refused. Receipt: `cyclic_callee_context` in the
  pilot manifest. V01, private storage, full coverage and validator safety
  remain OPEN; no advancement of V02–V06 is claimed.
  Full workspace: 401 suites / 3,080 passed / zero failed / five ignored,
  Clippy/format pass; ten original pairs replay. Disk delta +3,108 KiB,
  target/frontend unchanged, 51 GiB free; protected artifacts retained.

  **October 5 checked-return construction:** `51b27bd2` constructs the caller
  suffix proof and composes it with a cyclic callee. Six continuation controls,
  the original `call_loop` return suffix, eight mutation refusals and 18
  fresh-return controls pass. Clean Lean: 72 kernel-only core targets;
  full workspace: 401 suites / 3,080 passed / zero failed / five ignored,
  Clippy/format pass. Whole `call_loop` admission still refuses: the actual
  argument/frontier and IR call-site prefixes and the callee cutpoint proposal
  must be connected. Private storage, full coverage and validator safety remain
  required. Receipt: `checked_return_continuation` in the pilot manifest.
  Artifact delta +3,252 KiB; target/frontend unchanged, 51 GiB free. Thirteen
  temporary logs/development receipts were retired; final evidence retained.

  **October 5 whole cyclic caller entry:** `60adb247` closes the actual
  argument/frontier and call-site prefixes and verifies the callee proposal;
  unchanged `call_loop` is now a whole-entry PASS for all typed arguments and
  unbounded execution. Eleven original pairs replay; six frontier controls,
  three whole-pair controls, 21 mutation refusals and three execution/frame
  controls pass. Clean Lean: 74 kernel-only targets; full workspace: 401 suites /
  3,080 passed / zero failed / five ignored, Clippy/format pass. The old caller-
  only proposal still refuses. `call-loop-context.c` adds four valid-source REDs:
  right operand, conditional result, assignment and nested cyclic calls. Three
  fail source-prefix construction; the conditional fails return resumption;
  all target prefixes and callee diagrams construct. Complete these contexts,
  private storage, full coverage and validator safety before closing V01.
  Receipt: `whole_cyclic_caller`. Build/capture delta +4,292 KiB, target/frontend
  unchanged, 51 GiB free; intermediate scratch retired, final evidence retained.

  **October 5 call-context closure:** `a1338248` / `014bfa62` close the four
  preceding REDs; `e5db8d5e` closes initializer calls, post-call short circuit
  and a branch with a return-only arm. Nineteen original pairs replay.
  New context tests: 8 original/renamed pairs, 17 mutations, 20 executions.
  Clean Lean: 77 kernel-only core targets; full Rust baseline at `014bfa62`:
  3,080 / 0 / 5 ignored, Clippy/format pass (Rust and gate unchanged).
  Two valid-source REDs remain in `call-loop-short-right.c`: preserve the
  selected path premise when a logical right operand calls a cyclic callee.
  Private-storage composition, complete frozen closure coverage and validator
  safety still block V01. Receipts: `saved_call_contexts`,
  `call_statement_contexts`. Combined artifact delta +6,396 KiB, 53 GiB free;
  temporary probe removed, protected evidence retained.

  **October 5 logical RHS continuation:** `9ab35d27` closes those two REDs.
  Branch premises are established by matched paths and renamed across returns;
  they are not unconditional facts or new EqSat rules. Twenty-one original
  source/IR pairs replay, the caller-only negative still refuses; four new
  original/renamed pairs, 18 mutations, 14 executions, 15 premise controls and
  24 six-type resumption diagrams pass. Clean Lean: 79 kernel-only core targets,
  1,246,159 vector answers. Retained Rust gate is still `014bfa62`, not a new
  run. Full call/constructor closure, private storage and validator safety remain
  OPEN. Receipt: `logical_right_call_contexts` in the existing pilot manifest.

  **October 5 mixed-call continuation:** `59e0f8cc` closes three more original
  call contexts without assuming a finite callee summary: proposed entry-only
  spans still pass the independent checker. Twenty-four original pairs replay;
  six new original/renamed pairs, 23 refusals, 27 execution/frame controls and
  the clean Lean gate pass (79 kernel-only targets). Next RED:
  `call-loop-nested.c` plus `call-loop-nested.cutpoints.json`, where the caller
  itself loops and calls a cyclic callee. Source/IR execution agrees on three
  controls, but the whole-pair checker still refuses. Close the caller backedge
  and suspended callee jointly; no loop unrolling/fuel-bound acceptance.
  Private storage, complete closure/constructor coverage and validator safety
  remain OPEN. Receipt: `mixed_call_contexts` in the existing pilot manifest.

  **October 5 cyclic caller continuation:** `d14f4d95` closes
  `call-loop-nested.c`: productive open fragments compose callee returns with
  the caller's independently checked backedges. Twenty-five original pairs and
  the retained negative replay; 2 original/renamed pairs, 24 mutations,
  27 execution/frame controls and 2 nonterminating variants pass. Clean Lean:
  all steps PASS, 82 kernel-only core targets, zero native axioms in those
  targets. Rust baseline remains `014bfa62`. Next RED: `call-loop-deep.c`
  invokes this nested-loop function from another function; source validation
  and three execution controls pass, but the whole-pair checker refuses.
  Reuse open caller tables for callees under a suspended outer return context;
  do not finitely unfold or assume a summary. Private storage, complete closure
  and constructor coverage, and validator safety remain OPEN. Receipt:
  `cyclic_caller_contexts`. Artifact delta +3,268 KiB; 52 GiB free; no temporary
  source remains, protected evidence retained.

  **October 5 general closure and safety:** `8b7d16d0` generalizes suspended
  callers through the nonrecursive closure; `8afff094` proves unused-poison
  transport and closed-term diagram normalization. The unchanged combined
  `source-execution.c` / `main` closure now passes, without input restriction or
  an execution bound. Thirty-one positive original pairs and one retained
  negative replay. `05fb62d4` proves scalar/read safety under explicit runtime
  store/operand invariants; `4a8ac98e` proves accepted whole-pair refinement
  excludes malformed source terminals. Standalone validator safety is still
  OPEN: establish and preserve those invariants through the complete machine.
  Private lowered-storage composition and complete constructor coverage also
  remain OPEN; no V02–V06 or release completion is claimed. Clean Lean PASS:
  90 kernel-only core targets, no native axioms in those targets. Full workspace
  at `8afff094`: 401 suites, 3,080 PASS / 0 FAIL / 5 ignored, Clippy/format PASS.
  Receipt: `general_call_closure` and `validator_safety_foundation` in the existing
  pilot manifest. Artifact delta +6,412 KiB; 52 GiB free; target/frontend
  unchanged, capture 17,832 KiB; temporary diagnostic inputs removed.

  **October 5 source writes:** `6c28845a` proves actual-validator-backed safety
  for assignment entry/continuation and updates, plus store preservation,
  declaration, initialization and caller restoration. This exposed and fixed
  the accepted-but-malformed `int x = (x = 1, 2);` model case; its original
  source/IR pair now passes, with const and uninitialized-read negatives retained.
  `4db6012e` adds scope-exit preservation under explicit binding-separation and
  retained-type premises; establishing that frame invariant globally is still
  required. Clean Lean: 98 kernel-only core targets, zero native axioms in those targets;
  33 replays have their expected results (32 positive, one retained negative).
  Full workspace: 401 suites, 3,081 PASS / 0 FAIL / 5 ignored; Clippy/format
  PASS. Scoped artifact delta +19,260 KiB; 52 GiB free, capture under 19 MiB.
  Whole-expression/continuation invariant preservation, private lowered storage
  and complete constructor coverage still block V01. V02–V06 remain OPEN.
  Gate results and disk accounting: `validator_storage_preservation` in the
  existing pilot receipt, not a new acceptance layer.

  **October 5 completion (current state):** V01 is complete in the frozen scope.
  `checked_source_safety` and `check_source_target_refines` are kernel-checked.
  The original replay accepts 32 positive pairs and 36 conversion pairs and
  refuses the retained negative pair. Receipt: `source_to_ir_completion`
  (`PASS_FROZEN_PROFILE`) in the pilot manifest. The paragraphs above are
  history. This is not a source-to-assembly or release claim.

- [x] **V02 — Prove selection and out-of-SSA result checking**, 2026-10-05; complete in the frozen scope; P1.
  After V01's semantic interfaces, cover EvoIR-to-EvoMIR, always-on baseline
  lowering/DCE and simultaneous edge copies. Prove simulation; reject wrong
  operation/width, lost effects, swapped edges and destructive copy cycles.
  No unproved semantic pass may sit between the checked stage snapshots.
  **October 5 completion (current state):** `checkLowering` checks the actual
  EvoIR, SSA EvoMIR and post-SSA EvoMIR. `source_machine_refines` composes it
  with V01. The replay accepts 68 original entries from 17 modules and eight
  private-storage entries. Receipt: `machine_lowering_checkpoint`
  (`PASS_FROZEN_PROFILE`) in the pilot manifest. Allocation, frames and
  emission are not covered.

- [x] **V03 — Prove two-input allocation validation including spills**, 2026-10-05; complete in the frozen scope; P1.
  After V02, compare original/candidate MIR, track registers and stack slots,
  validate exact ABI bindings and inductive block annotations. Include copies,
  spills, slot reuse and call clobbers. Reject wrong reloads, original-operation
  mutations, false loop invariants and width/alias errors. The existing one-input
  register-holder checker is not this theorem; translating it unchanged cannot
  close V03.
  **October 5 completion (current state):** `checkAllocation` in
  `lean/EvoIR/Alloc/` checks the post-SSA EvoMIR, the allocated program and
  untrusted block hints. `checkAllocation_sound` proves refinement, and
  `source_allocated_refines` composes it with V01 and V02.
  `EvoIR.Alloc.AcceptanceTests` accepts the actual allocations of the 17 pilot
  modules. It refuses wrong reload slots, live values destroyed by a call or a
  write, exchanged arguments, changed operations, false loop invariants and
  slot reuse. `export_core_allocation` in
  `crates/evocc-mir/src/verify_alloc.rs` makes the export. Proof-required mode
  runs V03 (commits `d3a9c4d8`, `17873336`). Frames and emission are not
  covered.
  October 6: the acceptance sweep of proof-required mode accepts V03 for all
  107 entries of the 17 pilot modules. No pilot entry has an edge bridge block.

- [x] **V04 — Close frame/emission verification and compose the baseline**, 2026-10-06; complete in the frozen scope; P1.
  After V01–V03, prove frame separation, save/restore, stack arguments and the
  correspondence to the AST decoded from the actual final emitted assembly.
  Compose a Core C-HIR-to-assembly
  theorem, accept all frozen in-profile original functions and retain native
  tests at the disclosed decoder/toolchain boundary. An isolated backend proof
  cannot close V04. Baseline closure precedes V05 optimizer integration.
  In work (October 5). `checkAsm` in `lean/EvoIR/Asm/` is proved
  (`checkAsm_sound`), and `source_asm_refines` composes V01 to V04. The print
  check compares the printed AST with the actual text byte for byte.
  Proof-required mode runs V04 on the text that it publishes (commits
  `2bbee1ed`, `c4bed6a1`, `642234db`). Private frame objects are refused. V04
  stays OPEN until the lead runs the full gates.
  **October 6 completion (current state):** V04 is complete in the frozen
  scope. `checkAsm_sound` and `source_asm_refines` are kernel-checked, and the
  full gates pass (see V06). Proof-required mode accepts all 68 pilot entries
  of the 17 pilot modules to `CHECKED_TO_FINAL_ASSEMBLY`. Every function in
  the output must be an entry, so the mode also checks the 39 other functions
  of these modules. It accepts all 107 entries. The paragraph above is history.

- [x] **V05 — Compose existing verified rewrites with the proved pipeline**, 2026-10-06; complete in the frozen scope; P1.
  After V04, reuse frozen P0/g0 rules; close admitted seed/poison/fact premises,
  certificate replay, extraction and replacement in context. Show both a P0
  rewrite and g0 guard fold on the frozen path. Reject forged premises and
  unauthorized obligation/effect/splice changes. Audit transitive assumptions;
  native-backed historical rows do not silently become kernel-only. No new rules
  or fact domain. Exit is the composed optimized-compilation theorem.
  In work (October 5). `checkRewrite` in `lean/EvoIR/Rewrite/` is proved
  (`checkRewrite_refines`, `check_source_rewrite_refines`). It accepts a P0
  fold and a g0 guard fold from actual `-Oeq-scalar` output and rejects forged
  premises, deleted obligations and changes outside the roots. Proof-required
  mode runs V05 on each rewrite step (commits `f4f71dc5`, `46be6d80`), and V02
  to V04 check the rewritten program to the final assembly text. The pipeline
  check (`checkPipelineTexts_sound`) gives one theorem for the whole optimized
  chain. V05 stays OPEN until the lead runs the full gates.
  **October 6 completion (current state):** V05 is complete in the frozen
  scope. `checkPipeline_sound` covers zero or more rewrite steps between V01
  and V02. Proof-required mode accepts two optimized compilations to
  `CHECKED_TO_FINAL_ASSEMBLY`:
  - `doom-fixedmul.c` (`FixedMul`) with `-Oeq-scalar
    --eq-policy=guard-known-true`: one g0 guard fold,
    `regions=FixedMul/entry/6 rules=g0_007_signed_count`.
  - `control.c` (`loop_join`, `short_circuit`) with `-Oeq-scalar`: one P0
    replacement, `regions=short_circuit/b1/2,short_circuit/b7/2
    rules=p0_107_fold_icmp_ne`.

  Each record has two rewrite steps. Step 0 has the rule. Step 1 names no
  region and no rule. Limits: V05 refuses the root-deletion (alias)
  certificate form and an extraction with more than one instruction. V05 does
  not use facts through add, sub, mul and comparison. The result for these
  facts is "unknown", because their existing proofs use SAT reflection. The
  paragraph above is history.

- [x] **V06 — Enforce proof-required compilation and final release acceptance**, 2026-10-06; complete in the frozen scope; P1.
  After V00–V05, integrate the compiled proved Lean checker at the driver output
  boundary with no silent fallback or unproved callee. Unsupported/rejected/
  timed-out proofs cannot publish verified output. Run full workspace, Lean and
  existing selected native/original gates on one frozen subject; retain exact
  coverage, theorem dependencies and execution trusted base. Bind actual source,
  checked stages and emitted assembly through the existing artifact path. R05
  cannot close until this gate is accepted; ordinary compilation remains distinct.
  In work (October 5): `evocc --emit=asm --proof-required=NEW_DIR` exports the
  typed source, EvoIR, SSA EvoMIR, post-SSA EvoMIR, the allocation export, the
  assembly AST and the assembly text that it will publish. With `-Oeq-scalar`
  and a pinned `--eq-checker-sha256`, it also exports each rewrite step. The
  compiled Lean checker (`evoir-sem --check-stage`) runs V01, V05 for each
  rewrite step, V02, V03 and V04 for each `--core-entry`. Then
  `evoir-sem --check-pipeline` runs `EvoIR.Pipeline.checkPipelineTexts` on all
  files as the final decision (record row `PIPELINE`, theorem
  `checkPipelineTexts_sound`). The record has a
  digest chain: adjacent stages read the same files, and `output.s` is the text
  that V04 checked. Output and record appear only if every stage accepts. Exit
  3 is a rejection, 4 a missing or changed input, 5 a missing, failed or
  timed-out checker, and 6 a construct outside `core-v1`. For the `core-v1`
  subset the record says "checked from typed Core source to final assembly
  text" (`CHECKED_TO_FINAL_ASSEMBLY`). Limits: Linux AArch64 with default code
  generation options; no private frame objects; edge bridge blocks are refused
  at V03; rewrite certificate forms that V05 does not support are refused; a
  loop entry needs a proposal file. Trusted: the source export, the Lean kernel
  and the compiled checker, the file readers, the printer of the assembly AST,
  the Lean semantics of typed Core C and of the AArch64 subset, `AbiEntry`
  (the caller obeys the ABI and gives sufficient stack), and the assembler,
  the linker and the hardware. Limit at that time (V07 removed it): `AbiEntry`
  uses zero-extended argument values; the theorem covers only entry states in
  which the upper 32 bits of each 32-bit argument register and stack slot are
  zero, and AAPCS64 does not require this of a caller. Release acceptance is
  not run. The 15
  tests in `crates/evocc/tests/proof_required.rs` pass.
  **October 6 completion (current state):** V06 is complete in the frozen
  scope. No refusal in the acceptance sweep is a defect of the mode. The
  acceptance run:
  - Subject: implementation commit `8770b186`. The sweep ran at `4c4cc849`,
    which adds only the Lean gate receipt. Binary SHA-256 digests: `evocc`
    `aedc2cd02dda98ab65a18f3e25831eb3344d78d6c7ba371e5bff67bfed89dbb2`,
    `evoir-sem`
    `f2940a8e4fb852fd6f8bf7315f3c1e72578aa47570b9277a1f3b0e5a6def70c6`,
    `evocc-checker`
    `723354bf72ce5cbbc9cd3e7d3d35339c46159bbb87678590fc8eeda3be9f078f`.
  - Full workspace gate (`tools/gate-changed.sh --full-workspace --offline`)
    at `8770b186`, clean tree: 407 suites, 3,102 passed, 0 failed, 5 ignored.
    Clippy and formatting pass.
  - Lean gate (`tools/gate-lean.sh`) at `8770b186`, clean tree: all steps
    pass. The receipt `tests/contracts/lean-p0-receipt.json` names Lean
    commit `5d917f39`.
  - Sweep: proof-required mode ran on each of the 17 pilot modules of
    `machine_lowering_checkpoint`, with all functions of the output as
    entries and the proposal files below. 17 of 17 modules are accepted
    (exit 0). 107 of 107 entries are accepted to `CHECKED_TO_FINAL_ASSEMBLY`:
    the 68 pilot entries and 39 other functions of the same modules. No
    entry is refused at V01, V02, V03, V04 or the pipeline check. Both
    optimized cases of V05 are accepted.
  - Proposal files: the frozen `*.cutpoints.json` files give each loop
    entry its proposal. In `source-execution.c`, `loop_join` and `control`
    are also entries. Their proposals are the elements for symbols 7 and 56
    of `tests/fixtures/verification/source-execution.cutpoints.json`; the
    fixtures do not hold them as separate files.
  - Refused run: the first run of `source-execution.c` gave
    `loop-join.cutpoints.json` (symbol IDs of `control.c`) to `loop_join`
    and no proposal to `control`. The mode refused the module at V01 with
    exit 3. The reason line was `STAGE_REJECTED stage=V01 entry=control no
    checked source-to-EvoIR correspondence`. A direct check of the exported
    inputs refused `loop_join` at V01 with the same reason. All other entries
    of that module, `caller` and `main` included, passed every stage in the
    direct check. This refusal is a missing input, not a defect of the mode.

  - Not run in this acceptance run: the selected native gates that the task
    text names. Free disk space was below the stop line for heavy runs. V08
    ran them later on the output of the same 17 modules and the two optimized
    cases (see V08).

  The proof covers the listed subset only. The assembler, the linker, the
  runtime and the hardware stay trusted. Native tests do not remove them from
  the trusted base. The paragraph that starts "In work (October 5)" is
  history.

- [x] **V07 — Remove the zero-extension premise from the ABI entry assumption**, 2026-10-06; complete in the frozen scope; P1.
  First follow-up after V06. `EvoIR.Asm.Proof.AbiEntry` binds each argument
  register and stack slot to the zero-extended argument value. So the theorem
  covers only entry states in which the upper 32 bits of each 32-bit argument
  register and stack slot are zero. AAPCS64 does not require this of a
  caller. Next action: state `AbiEntry` with arbitrary upper 32 bits for each
  32-bit argument. Then prove that the checked code does not use these bits,
  or make the checker refuse code that uses them. Exit: `checkPipeline_sound`
  holds without the zero-extension premise, and the acceptance sweep of the
  17 pilot modules and the two optimized cases passes again.
  **October 6 completion (current state):** V07 is complete in the frozen
  scope (commits `3db5b76a`, `c00ae749`, `61868fdf`).
  - The pipeline theorems `EvoIR.Pipeline.checkPipeline_sound` and
    `checkPipelineTexts_sound` use the entry assumption
    `EvoIR.Asm.Proof.AbiSourceEntry` (`lean/EvoIR/Asm/Soundness.lean`). For a
    source argument of width `w`, only the low `w` bits of its argument
    register or 8-byte stack slot are equal to the argument. The upper bits
    are arbitrary. The zero-extension premise is removed.
    `abiSourceEntry_of_abiEntry` shows that the old assumption implies the
    new one.
  - The pipeline check has one new Boolean check, `entryViewsOk`
    (`lean/EvoIR/Pipeline/Check.lean`). Each 32-bit parameter must have a
    32-bit register class at the post-SSA entry. A refusal has stage `V02`.
  - `lean/EvoIR/Pipeline/Tests.lean` has a kernel-checked example with
    `x0 = 0xFFFFFFFF00000005` and source argument 5.
  - The change has 322 inserted Lean lines. The axioms are `propext`,
    `Classical.choice` and `Quot.sound`.
  - Gates at `61868fdf`, clean tree: the full workspace gate passes (407
    suites, 3,102 passed, 0 failed, 5 ignored; Clippy and formatting pass).
    The Lean gate passes (receipt Lean commit `61868fdf`, committed in
    `e657709b`).
  - The acceptance sweep passes again at `e657709b`: 17 of 17 modules, 107 of
    107 entries and both optimized cases are checked from typed Core source
    to final assembly text (see V08).

  The first paragraph is history.

- [x] **V08 — Run the selected native gates on proof-required output**, 2026-10-06; complete in the frozen scope; P1.
  The V06 acceptance run did not assemble or execute the output, because free
  disk space was below the stop line for heavy runs. Next action: when free
  disk space is above 25 GiB, assemble and run the proof-required output of
  the 17 pilot modules with the existing Linux AArch64 native harness. Compare
  each result with the result of the source interpreter. Do not add a new
  harness. Exit: each pilot entry gives the expected result, or each
  difference is recorded with its cause. A difference is a defect in a
  trusted part (the AArch64 subset semantics, the printer, the assembler or
  the linker), not a gap in a proved stage.
  **October 6 completion (current state):** V08 is complete in the frozen
  scope. The output of the 17 pilot modules was executed on Linux AArch64
  with no difference from the source interpreter in 14,194 comparisons.
  Receipt: [`tests/contracts/core-v1-native-run.json`](tests/contracts/core-v1-native-run.json).
  Command to repeat: `python3 tools/proof-native/proof_native.py all WORK_DIR`
  (`WORK_DIR` must be outside the repository). The scripts are in
  `tools/proof-native/` (commit `94a13c24`); the receipt gives their digests.
  - Subject: `e657709b`, with the binaries built at that state. The receipt
    gives their SHA-256 digests. Proof-required mode accepts all 19 cases
    with `CHECKED_TO_FINAL_ASSEMBLY`: the 17 pilot modules (107 of 107
    entries) and both optimized cases. This is also the repeat of the
    acceptance sweep that the exit of V07 requires. All 19 `output.s` files
    are byte-identical to the output of the first native run (subject
    `8dd00f59`, with the V06 binaries). The two runs give the same numbers.
  - Environment: the existing sealed image `evo-flight-lab:sealed` (digest
    `2c23684c9cc5`) on the Apple Container runtime 1.2.2. Guest kernel:
    Linux 6.18.15 aarch64. Assembler and linker: GNU as 2.44 and GNU ld 2.44,
    through gcc-14 14.2.0. Second build: the clang-19.1.7 integrated
    assembler and ld.lld 19.1.7. The two assemblers give identical `.text`
    bytes for all 19 outputs. No new image, VM disk or kernel tree.
  - Oracle: the source interpreter (`evoir-sem --run-core`, step limit
    10,000,000) for each module. Cross-check: the same C file compiled by
    gcc-14 -O0. It agrees with the source interpreter in each comparison.
  - Totals for the 17 modules (107 entries): 14,438 vectors; 14,194
    compared; 0 differences. The clang-19 build gives the same 14,194
    results. 178 vectors with undefined behavior are not compared; each of
    them traps natively with the same diagnostic code (`brk #3004`). 66
    vectors reach the step limit and have no source result: 10 return the
    gcc -O0 result, 55 time out in both builds, and `loop_checked(5,
    0xffffffff)` traps with `brk #3004` (signed overflow of `x`). The two
    optimized cases: 332 vectors, 328 compared, 0 differences.
  - Upper-bits probe: each entry with a 32-bit parameter is also called
    with nonzero upper 32 bits in each 32-bit argument register (four
    patterns). 45,232 calls on 83 pilot entries and 1,312 calls on the
    3 optimized entries give 0 differences. `loop_forever` has no result, so
    the probe does not call it. A static scan of the control flow from each
    entry finds no read of the upper bits of a 32-bit argument register. The
    8 reads that it finds are explicit zero extensions.
  - Not covered: no pilot entry has a 32-bit parameter on the stack; the
    vectors at the step limit have no source result; a test is not a proof.
    The assembler, the linker, the runtime and the hardware stay trusted.
    The native run tests them but does not prove them.

  The first paragraph is history.

### Defects found by the specification review and proof replay

The specification editors found D01-D05 on 2026-10-06, when they
stabilized the pinned documents in `spec/`. Each one is a difference between
the code and a pinned document. A fix changes the code, or it changes the
document with a new revision. None of D01-D05 changes the claim of
`spec/verification.md`: that claim does not depend on the Rust certificate
parser, the EvoMIR text parser or the Darwin ABI profile, and floating
builtins are outside `core-v1`. D06 is the separate October 7 proof-replay
refusal; changing the P13 demo does not close it.

- [ ] **D01 — Give builtin-argument-type its own diagnostic code** — `OPEN`, P2.
  `E2911` names two diagnostics: `static-assertion-failed` and
  `builtin-argument-type`. The parser reports `builtin-argument-type` for a
  wrong operand type of a quiet floating comparison builtin
  (`__builtin_isgreater` and its family) in
  `crates/evocc-parser/src/floating.rs`. The Diagnostic Authority table of
  `spec/evoir.md` section 6.3 lists `E2911` only as
  `static-assertion-failed`, and section 3.13.1 also names `E2911` for the
  wrong operand type. Next action: give `builtin-argument-type` a new code,
  add it to the section 6.3 table and correct section 3.13.1. Exit: one code
  per diagnostic, the section 6.3 table updated, and
  `crates/evocc/tests/check_product.rs` extended to check the new code.

- [ ] **D02 — Refuse the EVP epoch marker 0** — `OPEN`, P2.
  `parse_evp1` in `crates/evocc-proof/src/evp1.rs` accepts an `epoch 0`
  marker. `spec/evoir.md` section 11.3 numbers the epochs from 1 and says
  that a marker `0` is malformed (status 2). No test covers the marker `0`.
  Next action: make the reader refuse `epoch 0` as malformed, and add a test
  with an `evp2` certificate that is valid except for this marker. Exit: the
  reader refuses `epoch 0`, and a test shows the refusal.

- [ ] **D03 — Read every EvoMIR text form that the writer prints** — `OPEN`, P2.
  The EvoMIR writer `crates/evocc-mir/src/text.rs` prints `spill.store`,
  `spill.reload`, `arg.incoming` and `arg.outgoing`, and a `#zero:` asm
  operand of each register class. The parser
  `crates/evocc-mir/src/text_parse.rs` reads none of the `spill.*` and
  `arg.*` forms, and it reads `#zero:` only for `gpr32` and `gpr64`.
  `spec/evomir.md` Appendix A records these two limits. Next action: add a
  parser production for each form that the writer prints, then remove the
  limits from Appendix A. Exit: the parser reads every form that the writer
  prints, and a round-trip test (write, parse, write) gives the same text.

- [ ] **D04 — Darwin profile: 128-bit argument pairs** — `OPEN`, P2.
  Under the Darwin ABI profile, `record_plan` in
  `crates/evocc-mir/src/lower.rs` rounds a `bits<128>` argument to an even
  register pair, as under Linux. Apple clang does not round: after one
  64-bit argument, the pair is `x1`:`x2`
  (`.evidence/builtin-probes/20260905-b6/b6-darwin-abi.out`). Thus a call
  across the boundary to Darwin-compiled code can pass wrong values; calls
  between EvoCC functions agree. `spec/evomir.md` section 6.5 records this in
  its conformance note. The C driver emits the Linux ABI profile, so only the
  Darwin profile of the backend library is affected. Next action: decide if
  the Darwin profile follows the platform or keeps the even pair as an
  intended difference. Exit: the decision in section 6.5 and a test: the
  test shows the platform placement, or the section documents the difference
  as intended and the test shows the even pair.

- [ ] **D05 — Darwin profile: variadic rules** — `RETEST`, P2.
  The backend option `CodegenOptions::anonymous_on_stack` (`a794cab5`)
  implements the Apple arm64 convention: a call passes every anonymous
  argument on the stack, and `va_list` is one pointer (`7c1fe668`). The C
  driver enables it for `aarch64-apple-darwin` only (`c1985dd0`).
  `spec/evomir.md` revision 1.0.1 specifies it in sections 6.1 and 6.12. On
  a macOS host at `7b64cc1c`, `darwin_variadic_calls_agree_with_the_c_library`
  in `crates/evocc/tests/driver_cc.rs` passes, and cJSON 1.7.19 and zlib
  1.3.2 pass their own `make test` with `CC=evocc` (zlib: static and shared
  library). No test compares each variadic case of section 6.12 with the
  Apple reference. The driver test covers anonymous `int`, pointer, `long`
  and `double` arguments, `va_copy` and a `va_list` that `vsnprintf` reads.
  It does not cover `va_arg` of `double` in EvoCC code, 16-byte-aligned
  values, HFA and HVA records, or records above 16 bytes. The backend tests
  do not use the option. Next action: add a test against the Apple
  reference for each variadic case in spec 6.12. Exit: that test passes on a
  macOS host.

- [ ] **D06 — Lowering check refuses the guarded division shape** — `OPEN`, P2.
  Preserve the exact source from
  `.evidence/diagnostics/20261007/agent-demo/evocc-demo-core-integer.c`:

  ```c
  int divide(int divisor, int unused)
  {
      if (divisor < 0)
          return 0;
      if (divisor == 0) return 0;
      return 12 / divisor;
  }
  int main(void) { return divide(0, 7); }
  ```

  The retained `integer-proof.json` reports exit 3: the `lowering` stage for
  entry `divide` is rejected and no output is written. The target is
  `aarch64-unknown-linux-gnu`, with `-O0 -fno-builtin` and both
  `--core-entry=divide` and `--core-entry=main`; retain the full original
  command, pinned checker identity and inputs in that capture. The file
  `division-lowering-refusal/checks/lowering-divide.out` is empty; the exact
  report is in its sibling `lowering-divide.err`:
  `STAGE_REJECTED stage=lowering entry=divide no checked selection or SSA transport`.
  Historical read-only Lean probes narrow the refusal: real SSA transport
  passes, but selection fails. The signed-division guard's one-bit OR and
  the emitted 32-bit OR of zero-extended comparison results do not match in
  the current canonicalizer. An isolated `checkWordUnder` probe rejects the
  same pair even after the nonzero guard. A proved width/bitwise conversion
  is missing; this is not a claim that it is the only acceptance gap.
  Do not fix D06 in P13-P16 or change core-v1, exports, Lean, emitted assembly,
  IR or certificates to make the demo pass. Next action, when separately
  scoped: add a small regression before implementation, preserve this original
  input, and replay the same proof-required path. Exit: the unchanged guarded
  division regression has retained checked lowering evidence, with affected
  gates passed. The original slice's capture was 124 KiB with 171 GiB free;
  no before-build target size was recorded, so its total disk delta is unknown.
  No new proof replay or raw-capture commit is claimed by this tracker update.

- [x] **D07 — Consistent command defaults and repair output** — `DONE`, P1.
  Seven public regressions at `b11047ce` cover the four reported issues:
  fix argument order, whole-line unused-declaration removal, producer version,
  and source-checkout sysroot lookup. Each issue has a retained failing run in
  `.evidence/diagnostics/20261008/driver-usability/` (500 MiB cap).
  The fixes preserve neighboring code and comments, explicit sysroot errors,
  installed-root precedence and historical assembly/proof fixtures. The driver
  and emitter inherit one workspace release version. Two additional controls
  cover half-open deletion ranges next to a macro and conservative insertion
  boundaries. The seven focused suites pass 83 tests; the final repair suite
  passes 22 tests, including those two controls. Release/tracker Python checks
  pass 17 tests. Implementation: `2ce447b7`; ABI metadata test: `57e61c73`.
  On clean `57e61c73`, `tools/gate-changed.sh --full-workspace --offline`
  passes 425 suites: 3,237 passed, zero failed, five existing ignored tests;
  Clippy and formatting pass. The first full run retained one ABI snapshot
  failure from the corrected producer version. The historical hash table is
  unchanged; the test normalizes only that exact metadata line. All 23 Linux
  ABI hashes then match, with the Darwin path unchanged.
  The rebuilt release accepts options before the fix input, removes the whole
  unused-declaration line, and returns `CHECKED_TO_FINAL_ASSEMBLY` with the
  matching `EvoCC 0.1.0` producer. Cross preprocessing and ELF AArch64 object
  compilation pass from an unrelated directory without sysroot overrides;
  preprocessing also passes without the container on PATH. This does not
  extend the development check action: bare `evo check` on the header probe
  still refuses `stdint.h` with E0105, and is not counted as a check pass.
  No Lean semantics or sealed fixture changes; no new Lean gate is claimed.
  Task-owned temporary sources, object and duplicate proof output were removed
  after retaining the source, object hash and proof. Measured target/frontend
  plus capture growth is 4,067,496 KiB (about 3.88 GiB), mostly reusable build
  cache; the retained capture is 784 KiB and free space remains above 133 GiB.

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

- [x] **N01 — Size-gap attribution census** — `DONE`, P3.
  Before any optimization work, disassemble EvoCC, GCC-14 and Clang-19 `-O2`
  objects of the six S5 units and classify every instruction with the frozen
  13-category rule table in the plan; record TU-local inlining evidence
  separately. Totals must equal S1 instruction bytes. Deliver a frozen table
  under `tests/contracts/` and a ranked list of levers with estimated bytes.
  Read-only tooling; no compiler change. One week.
  Implementation and retained-object census are complete. Full workspace:
  2,915 PASS / 0 FAIL / 4 ignored; Clippy and formatting PASS; shared Python 135 PASS.
  [Receipt](tests/contracts/codegen-n01-20261002-receipt.json): 42 arm rows,
  37 distinct objects, exact S1 reconciliation; 647 of 2,210 additional
  historical ladder records classified. Library TU ratios remain 1.986–2.668,
  not a size improvement or target attainment.

- [x] **N02 — Pass framework, translation validation and pass telemetry** — `DONE (bounded framework)`, P3.
  An EvoIR pass interface with before/after hooks; a validation runner that
  reuses the two interpreters, the random corpus, ladder inputs and the
  four-lane native differential; per-pass telemetry (seen, taken,
  declined by reason, bytes); receipt field for pass class `VERIFIED`,
  `VALIDATED` or `LOWERING`. RED: a deliberately wrong pass must be caught on
  the corpus. Default output unchanged while no pass is enabled. Two weeks.
  Bounded identity framework implemented; [receipt](tests/contracts/codegen-n02-20261002-receipt.json).
  6,274/21,000 generated inputs admitted to TV; 14,726 opaque inputs only
  measured. A separate native-main population admits and runs 72/256.
  Bounded framework full gate: 2,915 PASS / 0 FAIL / 4 ignored, Clippy/fmt PASS.
  Frozen no-pass/identity resource pair: two supported inputs retain identical
  64/228-byte objects and 10/41 interpreter steps; observed compile ratios
  1.031/1.089 meet the 1.25 bound. This closes identity wiring only, not any
  optimizing pass or six-TU promotion; those bounds remain per-pass obligations.
  **Unclosed source bridge:** the unchanged
  integer-mix control records 1,001 C-HIR versus 1,002 EvoIR volatile events
  (local initialization store), despite matching returns. [Minimal input](tests/fixtures/codegen/volatile-initializer-observation.c).
  Keep `MISMATCH_NOT_VALIDATED`; do not normalize the event away, alter
  interpreter semantics in this slice, or admit an optimizing default pass.

- [x] **N03 — Codegen improvements, bounded batch** — `CLOSED_WITH_TARGET_MISS`, P3.
  Parent of the slices below. Product target declared before measurement:
  unchecked object text at or below 1.4× GCC-14 `-O2` per S5 unit by the end
  of the quarter. Each slice lands behind a flag, is measured on the six units
  and the ladder, and enters `-O2` only with validation green and the size,
  compile-time and interpreter-step bounds met. N01 has replaced the
  provisional order: summed positive excess over GCC-14 `-O2` on the five
  library units is 44,484 bytes of prologue/epilogue, 23,224 of constant
  materialization, 15,972 of memory access, 12,952 of register moves and
  6,712 of stack access on inflate alone. Work order: **N03f, then N03d, then
  N03c**; N03b and N03e wait on a redesign after their bounded negative
  preflight. This was the initial ordering; the bounded slices below are now
  dispositioned. Negative results are closure of this batch, not implementation
  of every general transform or achievement of the product target.
  [Final bounded status and six-TU table](docs/archive/codegen-quality-plan.md#11-status-and-order-of-work-2026-10-03):
  the admitted cumulative C3 configuration leaves five library ratios at
  1.642–2.478; the measured C5 configuration leaves 1.639–2.472 but misses
  three compile-cost bounds and is **not admitted**. Default output is unchanged.
  Broader inlining, mid-level optimization, global spill quality and the missing
  1.4x objective remain explicit gaps; reopening them requires a new bounded
  design, not relabeling these negative measurements.

  - [x] **N03a — Backend layering, behavior-preserving** — `DONE`, P3.
    Separate the target-independent allocator, out-of-SSA, spill placement and
    verifier from AArch64 facts: instruction operand descriptors read by both
    allocator and verifier, an ABI interface with Darwin and Linux
    implementations, the boundary test against ISA names in generic modules,
    and a synthetic test ISA for unit tests. Includes target-description
    steps 1–3 (`regfile`, `insts`, ABI tables). Exit: byte-identical objects
    for the six units and the native corpora, verifier verdicts unchanged,
    full workspace gate. No size change may ride along.
    [Frozen plan](tests/contracts/codegen-n03a-20261002.json);
    [receipt](tests/contracts/codegen-n03a-20261002-receipt.json);
    [result](docs/archive/notebook/codegen-backend-layering-20261002.md), with
    [preparation](docs/archive/notebook/codegen-n03-foundations-20261002.md),
    [step-1](docs/archive/notebook/codegen-n03a-regfile-20261002.md) and
    [step-2](docs/archive/notebook/codegen-n03a-machine-20261002.md) records.
    `targets/aarch64/{regfile,insts,abi-darwin,abi-linux,profile-kernel}`
    generate every migrated table; `src/generic` holds SSA destruction, linear
    scan, spill placement, slot compaction, the independent verifier, the
    argument cursor and the frame driver, and the workspace boundary test keeps
    it free of ISA names. At `efcf8c57`: 24/24 original A/B objects, 12/12
    retained-g0 objects and 384/384 native objects on each of Darwin and Linux
    are byte-identical, both corpora execute and agree 384/384, and 10 g0
    certificates replay. Full workspace: 2,986 PASS / 0 FAIL / 4 ignored;
    Clippy, formatting and regeneration PASS. Zero changed object bytes or
    verifier verdicts; no optimization or ABI-correction claim. Current Darwin
    variadic and stack rules remain as applied (unmeasured), recorded
    explicitly in `abi-darwin`.

  - [x] **N03b — TU-local inlining, bounded first slice** — `DONE_NEGATIVE`, P3.
    Size-aware inliner at EvoIR level for TU-local callees, as a validated
    pass under N02. The certificate chain runs after inlining and must still
    verify. A size increase on any unit is a recorded regression.
    [First slice](tests/contracts/codegen-n03b-20261002.json) begins with a
    read-only original-source/IR opportunity census. No transform is implemented
    before a nonzero source-permitted, structurally eligible, TV-admitted result.
    The bounded preflight is closed as `NO_ADMITTED_OPPORTUNITY`
    ([record](docs/archive/notebook/codegen-negative-preflight-20261002.md)); the
    full-workspace gate it was waiting for has since passed at `efcf8c57`.
    N01 still counts 3/1/85/24/23 TU-local bodies absent from the reference
    objects, so the negative result is about the frozen admission rules, not
    about inlining. [Closure receipt](tests/contracts/codegen-n03-preflight-closure-20261002-receipt.json)
    preserves the original failed-gate receipt and binds the later full green
    recheck. Zero transformations, no gain or default admission. A broader
    inliner requires a new design before any transform.

  - [x] **N03c — Register allocation improvements, bounded slices** — `DONE (bounded)`, P3.
    Coalescing and spill placement in the generic layer first; a different
    allocator only if N01 shows allocation dominates. The allocation verifier
    is the gate. After N03a.
    Recorded during N03a and deliberately not changed there: with a synthetic
    ABI whose argument registers are the whole allocatable pool, the shared
    allocator's spill-retry loop did not converge. AArch64 is unaffected, but
    a target with few registers would be; give it a reduced reproducer and a
    bounded-termination fix in this slice.
    [N03a note](docs/archive/notebook/codegen-backend-layering-20261002.md).
    T1 is frozen in the [bounded-progress plan](tests/contracts/codegen-n03c-termination-20261002.json):
    bounded synthetic RED, saturated call-preparation repair and refusal of
    recursive spill-temporary expansion. No copy preference or size claim;
    the historical multi-GiB reproducer must not be rerun unbounded.
    RED `74473d62` reached the test-only 2,048-visit limit in both cases.
    The repair now passes all 59 tests in the six focused allocator/ABI/SSA
    suites, including successful saturated scalar/wide/mixed call setup and
    finite refusal of impossible local pressure. T1 correctness is closed in
    the [result](docs/archive/notebook/codegen-allocation-termination-20261003.md) and
    [receipt](tests/contracts/codegen-n03c-t1-20261003-receipt.json): full workspace
    3,020/0/5 ignored, both native corpora 384/384, all 768 corpus and 24
    six-TU hashes match C2 OFF. All 24 original harness executions pass.
    Three of 24 compile-median ratios miss 1.25x (maximum 1.4563x); the
    negative measurements are frozen, not performance admission. No size
    gain or broader allocation closure is claimed. Copy preferences and
    spill-quality work remain open; do not rerun to erase the cost misses.
    T2 [copy-hint plan](tests/contracts/codegen-n03c-copy-hints-20261003.json)
    is frozen in `5a477d38`; `e5a9361f` records allocator and public-option
    REDs. Same-class, already assigned locations are optional preferences
    behind `--regalloc-opt=copy-hints`, with unchanged interference checks,
    spill policy and independent verifier. T2 is now **closed opt-in** in the
    [result](docs/archive/notebook/codegen-copy-hints-20261003.md) and
    [receipt](tests/contracts/codegen-n03c-t2-20261003-receipt.json): full workspace
    3,028/0/5 ignored; native ON/OFF each 384/384 on both OS lanes, all OFF
    hashes match T1; 72 original selected-TU harness executions pass with
    zero proof/bundle failures. All 48 size/cost comparisons meet their bounds
    (maximum compile ratio 1.021x). B unchecked move-byte savings are
    0/44/400/628/148/44; no stack-byte improvement. Cumulative frame+C1+copy
    library ratios are 1.643–2.488, still above 1.4. Defaults remain OFF;
    T1/C2 negative cost results are not upgraded. T3's
    [local reload plan](tests/contracts/codegen-n03c-reload-reuse-20261003.json)
    is frozen in `22d8e3d1`, with REDs in `e44ed429`. The default-OFF
    `reload-reuse` implementation removes only same-slot, same-class,
    same-location local reloads after allocation; calls, physical clobbers,
    stores, edge uses and reserved expansion scratch block reuse. No live
    interval, assignment choice or independent verifier rule changes.
    T3 is now **closed opt-in** in the
    [result](docs/archive/notebook/codegen-reload-reuse-20261003.md) and
    [receipt](tests/contracts/codegen-n03c-t3-20261003-receipt.json): full workspace
    3,039/0/5 existing ignored; 73 focused and 4 public-driver tests PASS;
    native ON/OFF each 384/384 on both platforms, all OFF hashes unchanged.
    All 72 selected-TU harness executions and object-twin checks pass, with
    zero proof/bundle failures. All 48 size/cost comparisons meet bounds
    (maximum 1.232x). Reload-only B unchecked saves 0/4/96/64/12/0 stack
    bytes; cumulative frame+C1+copy+reload saves another 0/4/96/60/12/0
    versus T2, leaving library ratios 1.642–2.478. All five miss 1.4x.
    N03c's T1/T2/T3 work is dispositioned, not a complete/global allocator:
    cumulative inflate still has 7,300 stack-access bytes. No further global
    redesign in this slice; defaults OFF, T1/C2 cost misses retained, and
    N02's 70 admitted / 187 opaque inputs and source-bridge mismatch unchanged.
    Retired 816 idle historical objects; compact evidence and current
    OFF/cumulative baselines remain. Scoped disk net +254,750,720 bytes.

  - [x] **N03d — Instruction selection, bounded C1–C5 sequence** — `CLOSED_WITH_COST_MISSES`, P3.
    Addressing modes, immediates, shifted operands, conditional select and
    compare forms, extension elimination. New rules enter through the target
    description's pattern table with EvoMIR revision text; existing match arms
    migrate only when touched. After N03a.
    First slice [C1 plan](tests/contracts/codegen-n03d-constants-20261002.json):
    opt-in GPR constant materialization using MOVZ/MOVK, MOVN/MOVK or a logical
    immediate ORR, with generated pattern/cost descriptors. Default OFF;
    frozen RED, OFF identity, native, size and interaction gates required.
    C1 opt-in is closed: [result](docs/archive/notebook/codegen-constants-20261002.md),
    [receipt](tests/contracts/codegen-n03d-c1-20261002-receipt.json).
    Full workspace 3,005 PASS / 0 FAIL / 5 ignored; Darwin/Linux ON and OFF
    each agree on 384/384, with OFF byte identity. All six original TU captures
    pass their harness/twin/proof gates; 187 interpreter inputs and the old
    source-bridge mismatch remain unvalidated. C1 B savings are
    0/252/184/5,176/2,016/88 unchecked bytes, entirely constant materialization.
    Combined with opt-in frames, library ratios are 1.653–2.528; no default
    flip and no 1.4x claim. Retired completed object payloads, retained receipts.
    Addressing, shifted operands, compare/select and extension opportunities
    remain part of N03d after C1; no closure is inferred from this plan.
    C2 is now frozen in [the local-selection plan](tests/contracts/codegen-n03d-local-20261002.json):
    adjacent single-use shift/extension/multiply and indexed-memory idioms,
    emission only, default OFF. The real-driver RED was reproduced at `504e012a`;
    implementation `b478bc7c` now has full-workspace 3,018/0/5 ignored and
    both 384-case native corpora ON/OFF green; all OFF historical hashes
    match and all six TU captures complete without proof/bundle failures.
    [C2 result](docs/archive/notebook/codegen-local-selection-20261002.md),
    [receipt](tests/contracts/codegen-n03d-c2-20261002-receipt.json):
    **CLOSED_WITH_COMPILE_BOUND_MISS**, not full admission. Local-only B
    checked zstd-compress is 1.7977x OFF versus the frozen 1.25x ceiling;
    retain all samples, no resampling or default flip. Unchecked B savings
    are 0/648/268/916/592/392 bytes, entirely compute/extension removal.
    Cumulative library ratios are 1.612–2.501; all five still miss 1.4x.
    Exact completed object payloads retired after hash/handle checks;
    manifests, receipts and logs retained. Continue N03c/N07 independently;
    remaining select/compare opportunities require their own measured scope.

    C3 is closed opt-in: [plan](tests/contracts/codegen-n03d-extensions-20261003.json),
    [result](docs/archive/notebook/codegen-extension-reuse-20261003.md),
    [receipt](tests/contracts/codegen-n03d-c3-20261003-receipt.json), subject
    `93847196`. Exact local repeated Extend/Bfx/low-mask reuse; no opcode,
    interpreter, ABI, allocator or registry change. Full workspace
    3,051 PASS / 0 FAIL / 5 existing ignored; 16 focused Rust and 94 Python
    PASS. Both 384-case native corpora ON/OFF and 72 selected-TU executions
    pass; zero proof/bundle failures. All 768 OFF hashes match the retained
    baseline, and ON changes no corpus object. B unchecked savings are
    0/8/4/0/0/0 extension/mask bytes. All 48 size/cost comparisons meet bounds,
    maximum compile ratio 1.107x. Cumulative library ratios remain
    1.642–2.478; all five miss 1.4x. Defaults OFF, C2 excluded from cumulative
    options; 70 admitted/187 opaque and source-bridge limitations unchanged.
    Invalid Linux-to-Darwin signed-narrow probe retained, not counted PASS.
    Retired 792 idle intermediate objects; current baselines and records kept.
    Scoped disk net +325,369,856 bytes; about 32 GiB free at closure.
    C4 residual control preflight is closed positive:
    [result](docs/archive/notebook/codegen-control-preflight-20261003.md),
    [receipt](tests/contracts/codegen-n03d-c4-20261003-receipt.json).
    Subject `2ce2385c`; full workspace 3,051/0/5 existing ignored, 16 Python
    PASS. Forty-eight retained object rows reconcile with S1; current
    B/unchecked has 109 Boolean-branch and 25 compare-combination candidates.
    All 25 combinations are inclusive nonnegative imm12 intervals; one
    retained IR fragment explains missed branch fusion after baseline DCE.
    Syntactic candidates are not validated transforms; zero new optimized
    bytes. This preflight led to bounded C5 selection below, not general CCMP
    or CFG if-conversion. Scoped disk net +413,696 bytes;
    no object rebuild, full disassembly retention or new timing samples.

    C5 is closed with compile-cost misses: [plan](tests/contracts/codegen-n03d-controls-20261003.json),
    [result](docs/archive/notebook/codegen-control-selection-20261003.md),
    [receipt](tests/contracts/codegen-n03d-c5-20261003-receipt.json).
    Implementation `d71b40c4`, measured clean subject `21edc5e8`; EvoMIR 0.64.
    Bounded adjacent private-use Boolean branches and inclusive imm12 intervals
    reuse existing MIR operations before unchanged allocation. Full workspace
    3,065 PASS / 0 FAIL / 5 existing ignored; 19 focused Rust and 60 Python PASS.
    Both 384-case native corpora pass OFF/ON, all OFF bytes match C3, and ON
    changes zero corpus objects; focused native tests actually select both
    patterns. All 72 original selected-TU executions and 72 object twins pass,
    with zero proof/bundle failures and unchanged IR/chain/bundle/step records.
    B unchecked savings are 0/40/60/552/40/44 compare/select bytes. All 48 size
    bounds pass, but only 45/48 compile-cost comparisons meet 1.25x (maximum
    1.627x). No resampling: C5 stays outside the admitted cumulative arm, alongside
    C2's earlier cost miss. Prior C3 remains the admitted configuration; defaults
    remain OFF. General CCMP/CFG if-conversion is unimplemented, not disproved.
    Retired 792 hash-checked idle intermediates; exact intermediate runtimes are
    no longer retained, but current/prior baselines and all compact records remain.
    Scoped disk delta is recorded in the linked receipt and one-page result.

  - [x] **N03e — EvoIR mid-level passes, bounded first slice** — `DONE_NEGATIVE`, P3.
    GVN/CSE, SCCP, dead-code elimination and simple loop-invariant motion on
    the integer and control subset, each a validated pass under N02. EqSat
    stays the verified path for what it covers.
    [First slice](tests/contracts/codegen-n03e-20261002.json) checks exact
    all-incoming-edge reuse of already-carried integer results. Read-only
    preflight is complete; broad GVN/SCCP/LICM is not silently admitted.
    Same bounded preflight and disposition as N03b: no complete all-edge match
    under the frozen rules. The same closure receipt records zero transforms;
    broader passes require a new design before implementation.

  - [x] **N03f — Frame and call overhead, opt-in slice** — `DONE`, P3.
    Leaf-frame elision, callee-saved minimization and argument shuffling
    through the ABI interface, with EvoMIR revision text and ABI corpora.
    After N03a.
    **Closed opt-in, 2026-10-02.** [Frozen plan](tests/contracts/codegen-n03f-20261002.json)
    at `161d81d0`: three opt-in sub-steps behind `--frame-opt=LIST`, paired
    saves (F1), one shared epilogue per function (F2) and leaf-frame elision
    (F3); every default stays off in this slice and a default flip is a
    separate owner decision. Fresh options-off baselines were re-recorded at
    that commit (six units A/B, both 384-case native corpora) because N03a
    deleted its retained objects at closure. The interrupted F1 branch
    `wip/n03f-paired-saves` (`3e5e4688`) has now been reviewed and integrated;
    F2/F3 and EvoMIR 0.56 are implemented behind the same default-off options.
    RED checkpoints: `0771b134`, `fb12b2d9`; protection clarification:
    `718f155f`. Nine focused frame tests PASS; native interactions PASS for
    all four cumulative variants unsigned on Darwin and all eight variant /
    PAC combinations on sealed Linux (including canary exit 0/77).
    The Darwin PAC/unwind reference control itself fails (Clang-built subject,
    unsigned harness: exit 90, only two frames); that combination is explicitly
    NOT_VALIDATED, not counted as a successful unwind test.
    At `af8687f1`: full workspace 2,997 PASS / 0 FAIL / 5 ignored; Clippy,
    format and regeneration PASS. OFF: 24/24 six-TU A/B and 384/384 per-OS
    objects match the baseline. ON: both 384-case corpora execute/agree with
    byte-identical twins, 17 ABI tests pass, all six unit harnesses run under
    both profiles. B unchecked frame savings are 0/3,100/1,208/18,372/7,080/
    4,660 bytes; five library ratios remain 1.717–2.546, above the 1.4 goal.
    [Receipt](tests/contracts/codegen-n03f-20261002-receipt.json) and
    [one-page result](docs/archive/notebook/codegen-frame-variants-20261002.md)
    separate frame gains from P0, preserve the original failed Clippy run and
    source-bridge mismatch, and record the limited default-admission proposal.
    **Defaults stay off; no kernel or Darwin signed-unwind acceptance.**

- [x] **N04 — The C checker as a product** — `DONE`, P3.
  One command that runs the EvoIR interpreter as a precise undefined-behavior
  detector with the existing diagnostic codes; `--explain` pages generated
  from the specification's diagnostic table; JSON diagnostics; ui tests with
  blessed output. No interpreter semantics change. Parallel lane, three weeks.
  Implemented: [workflow](docs/checker.md), 8 blessed UI pairs, 12 focused tests
  PASS, 89 authority explanations and 78 explicit explanation TODOs.
  [Receipt](tests/contracts/codegen-n04-20261002-receipt.json); full workspace
  2,915 PASS / 0 FAIL / 4 ignored, Clippy/fmt PASS. Existing supported execution
  only; no whole-program absence-of-UB or complete system-model claim.

- [x] **N05 — Measurement automation and bisect** — `DONE`, P3.
  On each push: S1 sizes for the six units, compile time, a ladder subset and
  interpreter steps, appended as trend receipts; a receipt-driven bisect over
  commits. RED: an injected size regression is flagged and bisected to its
  commit. Lands before the first N03 slice. Parallel lane, two weeks.
  Real Git/ELF injection 8→24→24 bytes correctly located `ff5b17a0`;
  [receipt](tests/contracts/codegen-n05-20261002-receipt.json). Fresh six-TU
  collection completed at `7680ae23`: 12 byte-identical object pairs, 36 source
  compiles, 12 bounded native replays and 257 step measurements. Independent
  replay: zero completed rejection/bundle failure. Local pre-push activated
  and exercised through its real entry point; no remote push. Full workspace
  2,915 PASS / 0 FAIL / 4 ignored, Clippy/fmt PASS; shared Python 135 PASS.
  The integer-mix source-bridge mismatch is retained independently of its
  measurable unchanged-EvoIR step count; observation is not TV acceptance.

- [ ] **N06 — EqSat memory and effect domain** — `LATER`, P3.
  Not scheduled. The place where equality saturation has real value (CSE,
  dead code, alias-aware forwarding) needs a state and effect model and its
  own proof design. Reopen only after the N03 slices land, with a separate
  design and owner decision.

- [x] **N07 — Target description and ISA-model oracles** — `DONE`, P3;
  bounded migration/differential closed, SMT qualification closed negative,
  **not whole-backend proof acceptance**.
  Make the internal target description the only place a target fact is
  written. Steps 1–3 run inside N03a. Then: encoding legality with the
  assembler round trip; a pinned slice of the vendor ISA model with its
  emulator as a differential lane; bounded SMT verdicts for every existing
  lowering rule. Lean statements for patterns start on RISC-V and only after
  those steps. No hand-written ISA specification, no full-ISA proofs.
  E1's [frozen plan](tests/contracts/codegen-n07-encodings-20261003.json)
  is closed by [implementation `4b945e9e`](docs/archive/notebook/codegen-encodings-20261003.md)
  and its [receipt](tests/contracts/codegen-n07-e1-20261003-receipt.json):
  full workspace 3,032 PASS / 0 FAIL / 5 ignored; 52 focused Rust and 45
  Python PASS. Both 384-case native corpora and 24 A/B/profile TU objects
  are byte-identical before/after; all executions agree and proof replay has
  zero rejection/bundle failure. The generated encoding owner and existing
  BinaryImm validator repair change no selection or defaults. Retired 24
  duplicate before-TU objects; keep 792 current objects as the sole open
  E2/E3 migration baselines. Scoped disk net +60,121,088 bytes; about 74 GiB free.
  E2 is [closed for its bounded integer model](docs/archive/notebook/codegen-vendor-model-20261003.md):
  [receipt](tests/contracts/codegen-n07-e2-20261003-receipt.json), implementation
  `060f9065`; 2,600 exact cases per interpreter/model/native/QEMU lane, four
  boundary refusals, full workspace 3,032/0/5 ignored and 51 Python PASS.
  All 768 native and 24 TU objects remain byte-identical; no new size benefit.
  No-op generation preserves mtimes. Retired duplicate E1 TU objects and
  temporary model products; current E2 replacements plus native baselines
  remain for E3. Scoped disk net at most +164,175,872 bytes; about 67 GiB free.
  E3 is [closed negative](docs/archive/notebook/codegen-bounded-smt-20261003.md):
  implementation `98542c3f`; [inventory](tests/contracts/codegen-n07-e3-verdicts-20261003.json)
  covers 59 instruction families, 44 specification rows, nine patterns and
  28 specialized paths. The pinned SMT generator fails before the first
  `add/64` query on `undefined_bitvector`; ordinary constant folding does not
  help. The remaining 88 bodies/solver controls are NOT RUN; zero proofs and
  zero counterexamples. **N07-E3-D1 is dispositioned as a tool limitation**,
  not a compiler miscompile or permission to change the model. All uncovered
  domains remain explicit. A verbose diagnostic exceeded the capture cap;
  its 4.27 GB log was terminated, hashed and removed, with the incident retained.
  Current E2 objects remain as regression baselines; no new native/size claim.
  Full workspace 3,032 PASS / 0 FAIL / 5 existing ignored; 55 focused Python
  PASS. [Receipt](tests/contracts/codegen-n07-e3-20261003-receipt.json).
  Scoped retained disk increment +503,808 bytes, shared frontend cache unchanged;
  about 64 GiB free. No B1/B6 proof acceptance.

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

- [x] **C01 — Locked K6 full compile/link/boot/shutdown:** 2,799/2,799 target-C,
  zero GCC target-C fallback; [receipt](tests/contracts/kernel-full-receipt.json).
- [x] **C02 — Function-section repair and BPF/module boot:** 6,765/6,765 target-C,
  745 modules linked; current signed installation and selected BTF/module tests
  verified. Full-program/runtime follow-up remains K10/K13. [Receipt](tests/contracts/kernel-bpf-full-receipt.json), [ledger][kernel].
- [x] **C03 — Selected K7 subset:** both kernels 9 programs / 125 subtests PASS;
  not the full installed population. [Receipt](tests/contracts/kernel-selftest-receipt.json).
- [x] **C04 — Retained expanded KUnit module composition:** both kernels actually
  loaded 31 modules and passed 765/765 cases. Later filtered Images require K14.
  [Receipt][kunit-receipt].
- [x] **C05 — ALSA timer userspace dependency repair:** tiny RED abort → exit 0,
  unchanged `utimer-test` 2/2 on both kernels; not an EvoCC code-generation fix.
  [Evidence][kernel].
- [x] **C06 — TinyCC static-initializer E3101 and targeted LLVM repairs:** original
  TinyCC tests after installation; `compare-fp-1/3/4`, `pr50310`, `20020720-1`,
  LARGE `lu` and `ludcmp` pass their recorded replays. Full LLVM remains B01/B10,
  and `floyd-warshall` remains B02. [Evidence][apps].

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
