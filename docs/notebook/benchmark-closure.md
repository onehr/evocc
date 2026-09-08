# Benchmark issue closure

Baseline: `e6c7938`; all-lane measurement `20260907T141450.311159Z`,
musl rerun `20260907T153648.272156Z`, compiler SHA-256
`b13d4a08539dfb6056e61f47631768357ced2e9f404247d7893593643c9ca6e6`.
The [evaluation](benchmark-evaluation.md) and its linked raw records are the
inventory authority. Counts below overlap where a row is a subcategory.

## Execution and closure rules

Each semantic family gets a minimal failing regression, a shared-path fix,
focused tests, `tools/gate-changed.sh`, and an incremental measurement through
`python3 tools/bench.py <affected lanes>`. A focused PASS alone is not closure.
Keep unavailable references, unsupported programs and baseline failures visible.
No compiler changes are justified merely by reducing a diagnostic count.
Do not rerun all lanes after every patch; the final all-lane run follows the
incremental closures. Floating-point execution remains deferred by the user's
standing instruction; FP-dependent milestones cannot be called complete.

Order is by correctness first, then shared-path reach. Small independent
wrong-result fixes precede changes that require new machine constraint plumbing.
Only one heavyweight benchmark invocation runs at once.

## Completion checklist

The unit of work is the complete issue inventory below, not the next individual
repair. The user's 2026-09-08 instruction makes floating-point execution the
only deferred implementation category. Non-floating-point vectors, variable
length objects, unsupported syntax, linking failures and validation gaps remain
work, not reasons to label the non-floating-point objective complete. Existing
specification-body restrictions remain visible until an authorized rule changes
them; an implementation must not silently contradict the body.

The numbered rows remain the inventory authority; this checklist schedules
their remaining closure work without creating a second set of measurements.

- [x] Restore nested macro push/pop state; replay the original failures (row 1).
- [ ] Finish native wrong-result classification and repairs; preserve runtime
  obligations and consume required test options explicitly (rows 4, 17).
- [ ] Repair goto-entered automatic storage, initialization and lifetime;
  replay toybox cut/httpd/sed and control-flow regressions (rows 8, 12; next).
- [ ] Finish aliases/weak symbols and register-binding original-program
  validation, separating FP-dependent libc failures (rows 2, 3).
- [ ] Correct strong external-data addressing for the original TinyCC PIE link,
  then classify the other isolated relink failures (row 11).
- [ ] Replay assembly register alternatives and kernel substitution with the
  exact measured binary; close each substitution mismatch (rows 5, 6).
- [ ] Resolve the remaining kernel assembly/symbol/control-flow constructs,
  retaining explicit specification conflicts rather than bypassing them (row 7).
- [ ] Repair remaining toybox function-value and object-subvalue forms (row 9).
- [ ] Admit unused system-header types independently of FP execution (row 10).
- [ ] Classify and implement admitted declaration/call forms, not a blanket
  diagnostic-code exception (row 13).
- [ ] Implement the required integer-vector behavior under a complete contract
  and replay the original initializer/place failures (row 14).
- [ ] Close the variable-length-object requirement under an authorized contract;
  a current profile refusal is not a non-FP pass (row 15).
- [ ] Reduce and repair every remaining non-FP diagnostic family, including
  builtins, ABI, initializers and the remaining TinyCC compile refusals
  (rows 11, 16).
- [ ] Account for every original Csmith seed, including all 68 unavailable
  references; never replace a seed or count a timeout as agreement (row 18).
- [ ] Make cJSON tests execute the subject objects, not rebuilt reference C
  sources (row 19).
- [ ] Resolve or independently establish reference-baseline/environment failures;
  retain them in the reported universe (row 20).
- [ ] Implement missing non-FP milestone adapters when their prerequisites are
  ready; FP-dependent milestones remain explicitly deferred (row 21).
- [ ] Run one final full benchmark campaign after incremental closures, with
  unchanged program identities and an explicit per-program FP classification.
- [ ] Reconcile every outcome, update receipts and render status; report the
  non-FP objective complete only with no unresolved non-FP compiler failures,
  coverage refusals, regressions or unverified subject runs.

Invalid or nonterminating reference programs are not compiler successes. Their
disposition needs individual evidence and remains visible beside the original
denominator. An unclassified or unavailable reference blocks an "ALL passed"
claim. Completion means closure of this measured inventory and final campaign,
not a claim that no unknown compiler bug can exist.

| Order | Issue family | Baseline and closure requirement | Incremental bench | State |
|---|---|---|---|---|
| 1 | Macro push/pop state | `00206` and `pushpop_macro` now pass; definitions, undefined and nested states restored before subsequent uses | `ctest torture` | CLOSED: `20260908T005741.383578Z`, zero regressions |
| 2 | Object aliases and weak symbols | Three original alias link failures now pass; musl symbol exclusions 167 to 0; fresh replay verifies a 494/1,339-unit combination, not the withdrawn 987-unit claim | `torture musl` | PARTIAL: symbol repair benched; remaining execution failures include FP varargs |
| 3 | Explicit register operands | Binding reaches IR, allocation and all output edges; native spills and binding validation pass; corrected musl recorder preserves 1,339 identities | `musl` | PARTIAL: fresh replay complete, 494-unit passing combination; further failures remain |
| 4 | Remaining native failures | ctest 1 runtime failure; torture 35 runtime failures, zero assembly/link failures; original `medce-1` now passes | `ctest torture` | PARTIAL: static label relocation and constant-branch pruning CLOSED; other failures OPEN |
| 5 | Kernel substitution mismatches | Original six extent defects fixed in `measure-20260907T170640Z`; new replay booted but retained overwritten cached assembly and could not verify zero-storage objects | `kernel` | REOPENED: cache-content guard fixed; original replay and zero-storage classification required |
| 6 | Assembly register alternatives | All twelve authorized spellings select `r` after one evaluation; native operand/branch tests pass, kernel measurement pending | `kernel` | OPEN: implementation green; awaiting original-corpus replay |
| 7 | Remaining kernel refusals | Latest survey: 2,717 `asm:i`, one `asm:+rm`, one computed goto, one frame address, one missing function-static global; symbolic `%c` operands require a rule beyond integer-only `i` | `kernel` | OPEN |
| 8 | Return and edge typing | toybox now 192/203, zero regressions; cut/httpd/sed retain three edge-value refusals involving goto entry and object storage | `toybox` | PARTIAL: return fix benched, edge storage OPEN |
| 9 | Other toybox lowering refusals | Eight further refusals: seven function-value types and one object subvalue; classify FP dependence individually | `toybox` | OPEN |
| 10 | Unused system-header types | zstd 31 vector-type declarations, 7 `_Float32`, 4 pretty-function; cJSON 2 vector declarations; separate declaration admission from SIMD/FP execution | `zstd cjson` | OPEN |
| 11 | TinyCC compiler build | 7/11 assemble, 4/11 enter a passing build; three relink failures and four compile refusals remain; external-data PIE is independently reduced for tccrun | `tinycc` | PARTIAL: macro-rescan failures in tccdbg/tccelf CLOSED by original replay |
| 12 | Switch, labels, function prototypes | Nested cases, promoted dispatch, discarded abrupt exits, unprototyped calls and enum/integer pointer compatibility pass original native replay | `ctest torture` | PARTIAL: these repaired originals CLOSED; jump-entered storage OPEN |
| 13 | Torture declaration coverage | 108 E1001 refusals, not 108 proven instances of one cause; classify source forms and fix admitted forms | `torture` | OPEN |
| 14 | Integer vectors | 23 excess initializers and 17 projected-place failures mention vectors; non-FP does not imply scalar; keep vector execution distinct from header admission | `torture` | OPEN |
| 15 | Variable-length objects | 19 torture refusals plus other lanes; explicit profile limitation, not a solved test or silent miscompile | `torture musl` | OPEN |
| 16 | Remaining diagnostic classes | Torture 461 total refusals and musl 352 remain listed in raw results; classify residual E3101/builtin/ABI/initializer classes after preceding fixes | `torture musl` | OPEN |
| 17 | Torture applicability/options | `dg-options`, target requirements and timeout factors are not consumed; define explicit applicability, preserve excluded outcomes, never claim ALL non-FP from a filtered success count | `torture` | OPEN |
| 18 | Unavailable Csmith references | Fresh `20260908T061706Z`: all 1,000 emit; 932 agree, 68 oracle-unusable, zero mismatch; original identities and outcomes unchanged | `csmith` | OPEN: unavailable references are not passes |
| 19 | cJSON subject test linkage | Existing `make test` recompiles reference sources; tests must consume substituted objects before a subject PASS is possible | `cjson` | OPEN |
| 20 | Reference baseline failures | musl 17 and toybox 1; separate environmental/applicability faults from subject regressions | `musl toybox` | OPEN |
| 21 | Missing milestone adapters | Mbed TLS, PolyBench, llvm-test-suite, jq, Lua, SQLite, full-kernel and kselftest: eight NOT_IMPLEMENTED entries; prerequisites and FP deferral remain visible, not silently waived | named adapters | OPEN |

## Progress

### Latest completed incremental replay

`python3 tools/bench.py ctest torture csmith tinycc toybox --campaign all
--programs 500 --jobs 4 --budget 40` completed run
`20260908T061410.809670Z`, measuring the frozen `a1393e9` executable
(`2d6241716a6f42339c5a38a3903d20f3ce572f5706fc9637c3368de7688d661b`).
The wrapper returns INCOMPLETE, not PASS: c-testsuite 203/220, torture
1,195/1,692, Csmith 932/1,000 agreeing, TinyCC 4/11 units in a passing
mixed build, toybox 192/203. Csmith retains all original seeds and outcomes,
including 68 oracle-unusable and zero mismatches. TinyCC's original
`tccdbg.o` and `tccelf.o` change from refusal to assembled and join the
passing four-unit combination; `arm64-link.o`, `tcc.o`, and `tccrun.o`
remain isolated relink failures. No earlier PASS was lost in these lanes.
The subsequent enum repair is not attributed to this older executable.

`python3 tools/bench.py ctest torture --jobs 4` then measures `7b436bc`
in run `20260908T070638.695281Z`: 203/220 c-testsuite passes and
1,196/1,692 torture passes. Both original identity sets are unchanged;
the only outcome change is torture `enum-3`, refused to native PASS,
with no previously passing program lost. Torture retains 461 refusals
and 35 runtime failures, with zero emission timeouts or assembly/link
failures. Unselected lanes keep their earlier compiler identities; this
is not a new all-milestone PASS.

### Original-program replay and capture correction

The frozen `8a9f19d` executable was measured through
`python3 tools/bench.py torture musl --jobs 4 --budget 40`.
Torture `20260908T034747Z` retains the identical 1,692 program identities:
1,189 pass, 469 refused, 34 runtime failures, zero emission timeouts or link
failures. All formerly passing programs still pass; only `memclr` changed,
from emission timeout to runtime failure, so no extra PASS is claimed.

Musl `20260908T035026Z` is invalid as a full-denominator comparison: a
parallel recorder merged `sysconf.lo` and `crypt.lo`, silently reducing
1,339 units to 1,337. Its partial bisection identified `fprintf.lo`, and
the substituted test report includes failed floating-point formatting;
Section 6's GPR-only varargs ABI does not preserve FP arguments. The run
was interrupted after 25 probes, not reported as a passing mixed build.
`a86454c` holds one lock across the complete command record; 95 Python
tests and a 16-writer sealed stress test preserving 512/512 identities pass.
The fix requires a fresh original-unit replay, not relabeling this run.

The earlier musl run `20260908T015031Z` also lacks `substituted-REPORT`:
the adapter treated an absent report as an empty failure set. Its claimed
987-unit passing build is withdrawn in the derived receipt, with the
historical number retained under `invalidatedUnitsInPassingBuild`; it is
not replaced by a made-up zero. `6cdc159` removes that fallback, with a
missing-versus-empty regression and 96/96 Python tests. Fresh FP failures
therefore cannot yet be called regressions from a proven old passing build.

The two `arith-rand` originals also contain signed `-INT_MIN` at line 86:
both sealed compilers' O0/O2 UBSan instrumentation refuses that operation.
Clang's bundled runtime is absent, so its successful diagnostic runs use
the explicitly named system `libubsan`; the failed initial links remain
recorded in `.evidence/builtin-probes/20260908-arithmetic-torture/results.txt`.
This diagnoses an applicability issue without changing either original or
their native FAIL outcomes into PASS.

### Positional parameters without source names

Tests-only `a316754` fails semantically with C-HIR E3007 and lowering
E3101, with no build errors. `a26bb02` retains all ABI slots and argument
effects while making only the C identifier binding optional. Both
interpreters and native execution pass the fixture, including an unnamed
aggregate parameter; the original `pr103209` now returns zero in both
interpreters. The changed-crate gate against `a316754` passes 950 tests
with zero failures or ignored tests, plus Clippy/fmt. Original native
torture run `20260908T042040Z` passes `pr103209`: 1,190/1,692 pass,
468 refused, 34 runtime failures, zero emission timeouts or link failures.
Against `20260908T034747Z`, the identity set is unchanged and this is the
only changed outcome; all 1,189 previous passes are retained.

The same frozen executable (source `2a50ced`, SHA-256
`70b313d9ff4b1c5798b43ba6ec36775c7f0c8e4f7d1498ac52090a84a4728710`)
completed musl run `20260908T042300Z`. Capture now contains all 1,339
units; 987 assemble with zero symbol-parity exclusions. The 40-probe
budget is exhausted, with a 494-unit no-new-failures combination verified
in round 2, relative to 17 reference failures. Single-unit failures are
`fprintf.lo` and `snprintf.lo`; further failures remain after excluding
both. The final report exists (48,100 bytes), but belongs to the final
failing probe, not the best passing subset; `bisect.json` records which
combination each probe actually measured. This is INCOMPLETE, not a
restoration of the withdrawn 987-unit passing claim.

### Pointer interfaces and switch entries

`274ede9` compares compatible function/void pointers through their existing
address representation. `631f03e` routes switch segments through the same
label-aware sequence lowering as ordinary compounds, retaining goto targets
after a terminated path. `d32f676` preserves unprototyped function addresses
in storage, promotes indirect-call arguments, and constructs the actual call
interface without weakening dynamic signature checks. A long argument or
missing argument to an integer-parameter target remains E3019 in both
interpreters. This does not claim support for nested case labels or jumps
past unallocated automatic objects.

`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true
tools/gate-changed.sh abfa496 --quiet` passed 956 tests, zero failures and
zero ignored, plus Clippy and fmt. The focused tests include native execution;
original c-testsuite and torture replay remains the benchmark acceptance.

Replay `20260908T050555.942587Z` (compiler source `5a8d94b`) now measures
200/220 c-testsuite passes and 1,194/1,692 torture passes. The original
identity sets are unchanged, and no prior pass is lost. c-testsuite
`00087`, `00088`, `00089`, `00209`, and torture `20041212-1`, `20111208-1`,
`pr17377`, `pr67037` change from refusal to native PASS. Torture `eeprof-1`
changes from refusal to runtime FAIL: it requires `-finstrument-functions`,
which the adapter does not consume; its expected instrumentation callbacks
therefore never run. It remains a recorded failure, not a compiler PASS.

### Nested switch dispatch

Tests-only `3fabe4b` exposes non-compound switch refusal and wrong C-HIR
values for cases inside conditionals and loops. `fa6f895` additionally pins
integer promotion of a narrow selector and ownership of nested cases and
continue targets. The sealed GCC-14/Clang-19 O0/O2 probe executes all shapes
with exit zero (`.evidence/builtin-probes/20260908-nested-switch/results.txt`).

The C-HIR entry point previously selected a different recursive statement
executor for a call-free, memory-free function. That executor retained the
same top-level-only case scan. The repair removes it and runs every function
through the existing iterative machine; switch entry shares goto's scope and
loop continuations. Lowering retains the compact flat-switch representation,
while nested labels use the existing label/edge machinery and case matcher,
not flattened loop bodies. A narrow selector's promoted value must be carried
instead of being replaced by its original narrow local on later comparisons.
`19aa801` passes the changed gate against `3fabe4b`: 965 tests, zero
failures or ignored, Clippy and fmt clean. The test inventory was also
recounted with the same five crates' `cargo test -- --list` (965).
Entry past automatic memory-object
declarations remains a separate open storage/lifetime issue, not waived here.

Original replay `20260908T054623.296899Z` measures source `19aa801`:
c-testsuite `00051` and `00143` change from refusal to PASS, giving 202/220.
Torture retains all 1,194 prior passes; `medce-1` changes from refusal to
link failure at its deliberately unresolved `link_error` symbol, while
EvoIR execution returns zero. It is not a benchmark PASS: constant branch
emission still leaves the unreachable call in the object. The 1,692 original
identities are unchanged. Toybox now has 192/203 assembled substitutions
in its passing combination, relative to its one baseline failure; TinyCC
has 5 assembled, 2/11 in a passing build, with three isolated link failures
(`arm64-link.o`, `tcc.o`, `tccrun.o`). These remain incomplete milestones.

### Discarded expressions with abrupt control flow

Tests-only `c6a4138` records three semantic failures, all `statement
expression has no normal path to its value`, with no build errors. A
discarded expression can return, break, continue, or jump rather than
produce a normal continuation. The tracked fixture
`tests/fixtures/discarded-expression-exits.c` compiles and exits zero with
both pinned compilers at O0 and O2 in the sealed network-disabled image.
The repair reuses statement `Flow` for discarded expressions and merges
only actual fallthrough states; value-required contexts remain explicitly
checked. `aa08860` passes all 329 tests selected by the changed gate against
`c6a4138`, plus Clippy/fmt. Original run `20260908T055627.176403Z` moves
only c-testsuite `00213` from refusal to PASS (203/220); torture's original
1,692 identities and every outcome remain unchanged (1,194 PASS).

### Macro inhibition survives argument substitution

Two TinyCC parser failures have one preprocessing cause: `#define field
p->field` inside a prescanned argument becomes `p->p->field` on a later
rescan. In the originals this is `s1->s1->eh_frame_section` at
`tccdbg.c:815` and `s1->s1->symtab_section` at `tccelf.c:1978`, not a new
kind of projected lvalue. Tests-only `e9abc66` records two wrong-token
failures plus a passing function-name boundary control, with no build errors.
Both pinned preprocessors give the same nine lines for tracked fixture
`tests/fixtures/macro-rescan-inhibition.c` under `-std=gnu11 -E -P` in the
network-disabled sealed image. Preserve inhibition on the token occurrence,
not on its spelling globally: later source occurrences and newly pasted
tokens must remain eligible. All 105 C-preprocessor tests pass. `a1393e9`
and the constant-branch repair `b6cf13f` pass the combined changed gate
against `d22b5fc`: 1,243 tests, zero failed or ignored, Clippy `-D warnings`
and fmt clean. Original-program replay remains the closure requirement.

TinyCC's `tccrun.o` link failure independently names
`R_AARCH64_ADR_PREL_PG_HI21` against libc `environ` under PIE linking.
`arm64-link.o` also addresses `stderr`; `tcc.o` addresses `stderr`/`stdout`,
but their isolated linker diagnostics have not yet been recaptured. The
existing authorized GOT exception applies to undefined weak references,
not all external symbols. Keep this address-recipe boundary open; disabling
PIE in the benchmark is not an implementation fix.
The tracked reduction `tests/fixtures/external-object-pie.c` compiles and
exits zero with GCC-14 and Clang-19 using explicit `-fPIE -pie`; the
subject's `a1393e9` assembly fails GCC linking with the same relocation
error. Clang's default integrated assembler rejects the prescribed
`:pg_hi21:` spelling before linking, so that attempt is not reported as
the same linker diagnosis; use of GNU `as` must be explicit when isolating
the linkage failure. Explicitly rerunning Clang-19 with
`-fno-integrated-as -fPIE -pie` reaches the same `environ` relocation failure
(link exit 1), isolating the address recipe from the assembler syntax.

### Compatible enumeration pointer types

The original torture `enum-3` combines pointers to a negative enumeration
and its compatible `int` representation. The producer and place consumer
did not share the same compatibility check, and the interner itself did
not recognize an enum's existing underlying rank. Tests-only `71949c6`
records three semantic failures with zero build errors: false rank
compatibility, refused pointer-expression execution, and E2911 on the
positive compile-time query. The Appendix D entry records the pinned
O0/O2 positive executions and a separate GCC/Clang qualifier disagreement;
the existing nested-qualifier rule is retained, not loosened to follow GCC.
The extended fixture also exposed E2002 at a compatible redeclaration,
then allocation-manifest and interpreter-return-type identity checks;
these consumers now use the same target rank and type compatibility,
without changing the independent SSA validator or nominal tag identity.
Implementation `7b436bc` passes
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true
tools/gate-changed.sh 71949c6 --quiet`: 1,560 passed, zero failed, four
pre-existing standing-campaign tests ignored, Clippy `-D warnings` and fmt
clean for the twelve affected/reverse-dependent crates. Ruff, diff-check,
and the 12 benchmark-entry Python tests pass. This is a changed-crate gate,
not a newly claimed full-workspace or isolated-clone replay. Original
torture run `20260908T070702Z` closes the reduced enum family by native PASS.

The full original-seed Csmith replay `20260908T051022Z`, through
`python3 tools/bench.py csmith --campaign all --programs 500 --jobs 4`,
retains all 1,000 identities and every prior outcome: base 469 agree/31
oracle-unusable, bitfields 463 agree/37 oracle-unusable, zero mismatch and
zero subject refusal. Every unusable case times out in both GCC O0 and O2;
this does not establish that every such source is nonterminating. The native
campaign measured source `3fabe4b`, before the nested-switch repair, and is
not relabeled as verification of a later commit.

### Explicit local-register operand binding

`.evidence/builtin-probes/20260908-local-register-operands/{probe.c,results.txt}`
pins a local `register long wanted asm("x8")` passed as an `r` input.
GCC-14/Clang-19 at O0/O2 each exit zero; subject `25ab85d` emits assembly
that links successfully but exits one: it reads the old x8 value rather
than the argument. No original program or seed was replaced by this reduction.

LOCAL-REGISTER-OPERAND-REPRESENTATION: Section 10.1.18 says a local
register binding matters at an asm operand, but Section 3.11/9's operand
records and admitted constraint vocabulary have no field or spelling for it.
The binding must reach allocation and output edges, including tied/early-clobber
and asm.goto operands; inserting moves outside that boundary is not an adequate
substitute. This is separate from the immediate/register alternatives already
authorized by revision 1.9.78. The user authorized carrying the binding through
the IR and allocation boundary on 2026-09-08.
Tests-only checkpoint `ce22a80` records both semantic failures without a build
error. Binding now survives both textual IR forms, uses short-lived pinned
operands, and transfers outputs over every asm.goto edge. The independent
allocation verifier rejects a mutated binding; native tests cover both goto
edges under spilling and a callee-saved x19 binding. A new negative test also
ensures the source cannot directly admit the lowered `@xN` spelling.

`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true tools/gate-changed.sh ce22a80 --quiet`:
843 passed, zero failed, four existing ignored; Clippy/fmt PASS. The earlier
842-test gate was superseded by the additional source-vocabulary negative.
K1 Python tests: 88/88; bench tests: 12/12; Ruff/diff-check PASS; the grammar
audit has 42/42 instruction shapes with zero disagreement (not a full grammar
proof). Original musl and torture replay is still required for family closure.

### Immediate-register input alternatives

Tests-only checkpoint `9c46be8` has three E3021 semantic failures (`Jr`/`Kr`)
and zero build errors. Lowering now selects the existing `r` alternative for
exactly `I`, `J`, `K`, `L`, `i`, `n` paired with `r` in either order, after
evaluating the operand once. No backend constraint or instruction is added.
Two old rejection rows are migrated to still-invalid `Jrr`/`rII`; the newly
admitted `Jr`/`rI` are covered by the positive table, not silently dropped.

The eleven focused tests pass, including native assembly/link/execution of
all twelve spellings, 32/64-bit values, single evaluation and both asm.goto
output edges. The six logical/add/sub values match the sealed `kr.c` probe.
Outputs and 128-bit register inputs retain E3021.
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh 9c46be8 --quiet`:
303 passed, zero failed or ignored, Clippy and fmt PASS; Ruff and diff-check
PASS. Original kernel replay is still required before this family closes.

### Symbol linkage

The immutable-linkage reuse fix (`eecdb45`, RED `4069f80`) passes the
reverse-dependency changed gate, Clippy and fmt. The original torture
`memclr.c`, previously exceeding the unchanged 60-second emission limit,
now emits assembly in **1.397 seconds**, exit 0, empty stderr, using release
SHA-256 `e7dc4f01d59d217bb75784686b8f7145e582cf2d039e3dcb55f04a671d6a7069`.
Command: `target/frontend/release/evocc --target=aarch64-unknown-linux-gnu --emit=asm .build/ladder/run-torture/src/memclr.c`
under `subprocess.run(timeout=60)`, stdout discarded. This closes the
emission timeout, not its earlier runtime failure; the original program still
needs the full execution lane. Profile samples had identified repeated
whole-unit `SymbolLinkage::derive` inside each scalar initializer.

Kernel replay `measure-20260908T030624Z` reused the valid 2,799-unit survey
`measure-20260908T022219Z/survey`; every reduction retained release `25ab85d`
SHA-256 `d59b26ba3656f811592b782e36a2ad0a1cf0a8fb61a1c9bf5efad783b3b71d09`.
78 assembled, 2,721 refused; `efi/libstub/string.o` and `vsprintf.o` were
individually reduced link failures (`__efistub_isdigit`). The other 76 objects
relinked and booted, but `proof.json` reports 52 proven, 2 mismatched,
7 unverifiable, 12 not linked, 3 without symbols. The benchmark correctly
remains ERROR, not a validated kernel pass: inspect the zero-filled object
slices (`xen_features`, `guid_null`, `uuid_null`) and the missing extents for
`ctype`/`fdt` objects before claiming substitution closure.

Tests-only checkpoint `0f2a71d` has nine semantic failures and zero build
errors. Module records now preserve weak marks and flattened object/function
aliases; aliases share allocation, call identity, effects and relocation
targets in both interpreters, without forwarding bodies or copied storage.
Alias references keep internal targets, including otherwise removable
always-inline definitions. Invalid weak/internal combinations and alias
targets report E3060/E3061; the shared-type constructor changes are mechanical.

`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh 0f2a71d --quiet`:
1,471 passed, zero failed, four pre-existing ignored tests; Clippy and fmt
PASS. `ruff check tools tests/acceptance`, `python3 tools/check-textual-grammar.py`
(42/42 instruction shapes, not a full syntax claim) and diff-check PASS.
The first gate caught a malformed-C-HIR diagnostic-order regression; the
shared call-type check was corrected, the original negative assertion retained,
and the entire changed gate rerun. Focused linkage/address tests are 34/34.

Native AArch64 static-link probes pass object-alias writes, equal loaded
addresses, function aliases, weak definitions alone and strong-over-weak
replacement under both GCC-14 and Clang-19 link drivers. This does **not**
close weak external references: the explicit counterexample is
`.evidence/builtin-probes/20260908-weak-external-address/{probe.c,subject.s,results.txt}`.
Both sealed oracles at O0/O2 exit zero; the emitted subject links statically
but exits one. Default PIE linking additionally refuses its direct relocations.

WEAK-EXTERNAL-ADDRESS: EvoMIR Section 8's external-symbol
rule prescribes direct `adrp`/`add`, while revision 0.22 requires undefined weak
references to resolve to address zero; the measured linked address is 0x400000.
The user authorized GOT-based address materialization only for weak
extern/declare records on 2026-09-08, with no new EvoIR instruction. The
implementation preserves the direct address recipe for strong references and
definitions; its nine focused machine-linkage tests pass. Linux GCC-14 and
Clang-19 static/PIE links execute the original counterexample successfully;
the additional absent/present/call/read checks also pass in all four modes.
Evidence: `.evidence/builtin-probes/20260908-weak-external-address/got-results.txt`.

Darwin's default linker rejects unresolved weak references for both system
Clang and the subject. With the same explicit `-U` symbol list, both execute
the counterexample with exit zero; the native regression asserts both sides.
This is a measured linker-policy requirement, not a lowered-language change.
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true tools/gate-changed.sh d2d8295 --quiet`:
501 passed, zero failed, four pre-existing ignored; Clippy and fmt PASS.
Ruff and diff-check PASS. Alias/weak remains OPEN for the static-initializer
performance regression below, not for an unimplemented GOT exception.

The original `memclr` source again times out at 60 seconds. A two-second
system stack sample has 178/178 samples below static integer interpretation's
construction of a fresh linkage map; every initializer repeats a whole-unit
declaration scan. This diagnostic run overlapped the tail of the kernel
survey, so it is a hot-path observation, not an unloaded timing comparison.
The immutable linkage result must be reused without changing error checks;
the final original-program replay retains the 60-second limit.

### Kernel reduction compiler mismatch

Wrapper `20260908T022219.374623Z` finished its 2,799-unit survey at
`measure-20260908T022219Z/survey`: 78 assembled, 2,721 refused, no timeout or
panic. The initial mixed link reports missing `__efistub_isdigit`. Subsequent
reductions used a different executable because `bisect.py` ignored `K1_EVOCC`:
survey digest `d59b26ba...71d09`, reduction digest `4c39a20f...5f01`. Those
reduction PASS outcomes are invalid and the owned subprocesses were terminated;
the wrapper records ERROR, not completion. The valid per-unit survey is retained.
Tests-only `f7fe61b` and fix `e5f71eb` pin the override through reduction and
expose exact-binary survey reuse through `tools/bench.py`. K1 tests 87/87,
benchmark tests 12/12, Ruff and diff-check PASS. A correct mixed-link replay
is still required; no kernel closure is inferred from the broken reduction.

`python3 tools/bench.py torture musl --no-build --jobs 4 --budget 40` finishes
INCOMPLETE in wrapper `20260908T014701.174683Z`, subject `b504ee9`, compiler
`eea68d679e70193687b8ecd61b6e92e2fff1aeae4fd7dbe92f3088d9fc24fff8`.
The release executable was unchanged throughout both lanes.

- Torture `20260908T014701Z` retains all 1,692 original identities and all
  1,186 prior passes. `alias-2`, `alias-3` and `alias-4` change from link
  failure to PASS: 1,189 pass, 469 refused, 33 runtime failures, zero link
  failures and one emission timeout. `memclr` changed from runtime failure
  to the 60-second emission timeout; this remains a measured failure, not
  a cleared issue or a reason to increase the watchdog.
- Musl `20260908T015031Z` retains all 1,339 original units and every prior
  assembled unit. `intscan.lo` and `sigaction.lo` change from refusal to
  assembled: 987 assembled, 352 refused. Symbol-parity exclusions fall from
  167 to zero. This run originally claimed all 987 substituted units passed
  together against the 17-failure reference baseline; that claim is withdrawn
  because its substituted test report is absent. The valid fresh replay above
  establishes a 494-unit passing combination, not 987, and does not establish
  complete musl coverage.

An additional RED checkpoint `f1ee3cf` pins an alias whose target definition
was first declared by a block-scope extern. Fix `66a1f03` identifies actual
top-level definition nodes rather than the first symbol scope. Its changed
gate (`tools/gate-changed.sh f1ee3cf --quiet`, same offline/build settings)
passes 939 tests, zero failures or ignored tests, Clippy and fmt PASS.
This follow-up is not included in the benchmark compiler identified above.

### Macro definition restoration

Revision 1.9.77 is implemented through per-name stacks that preserve the whole
definition and its feature-query implementation, or its undefined state.
The tests-only checkpoint `a6c302d` has five semantic failures, zero build
errors; restoring `_Pragma` effects during rescanning also fixes their ordering
relative to subsequent macro uses. Header spelling, pack independence, invalid
argument classification and fresh-translation-unit stacks are pinned.
The measured probe has one empty pop; malformed push/pop arguments are counted
separately as ignored pragmas by name, and successful stack operations are not
reported as ignored-safe.

`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh a6c302d --quiet`:
1,037 passed, zero failed or ignored; Clippy and fmt PASS. Ruff and diff-check
PASS. Implementation `dba5385` is measured by
`python3 tools/bench.py ctest torture --jobs 4`, wrapper run
`20260908T005741.383578Z`, compiler
`0154d28e92cf57b72cbcd16bd542b8780e3d8e3586a08152ce1a44f0a317876b`.
Native runs `20260908T005741Z` and `20260908T005804Z` retain every original
220/1,692 identity and every prior pass: the only outcome changes are
`00206` and `pushpop_macro`, both failure to pass. c-testsuite rises 195 to
196/220; torture rises 1,185 to 1,186/1,692. The macro family is closed;
the benchmark suites remain INCOMPLETE, with 1/34 runtime failures respectively.

### Void return effects

Tests-only checkpoint `427951e` produced three declared semantic failures and
zero build errors. The shared void-type query, interpreter task dispatch and
effect-only lowering preserve direct/indirect calls, casts, comma expressions,
conditional arms and the final statement-expression value without inventing a
scalar result. Negative return-type checks remain refused.

- `cargo test -p evocc-lower --test static_storage_and_discard --offline -j 4`
  with `CARGO_TARGET_DIR=target/frontend`: 11 passed, zero failed.
- `CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true tools/gate-changed.sh e6c7938`:
  tests, Clippy and fmt green for evocc/chir/lower/parser/proofcheck.
- `ruff check tools tests/acceptance` and `git diff --check`: PASS.
- `python3 tools/bench.py toybox --jobs 4 --budget 20`:
  run `20260907T165020.113321Z`, native run `20260907T165020Z`,
  compiler `92084dcb125b537b46d5b8e1c554b510a5edc6fd452961a9dfdde39aebbd0792`.
  The same 203 units now have 191 assembled and linked together, 12 refused,
  no symbol exclusions and no runtime regression relative to the unchanged
  one-failure reference baseline. All 177 previously assembled units remain so;
  14 improve. The benchmark is still INCOMPLETE, not a closed toybox milestone.

The remaining first errors are edge-value availability (cut/httpd/sed), one
prototype (dd), seven function-value-type refusals and one object-subvalue
refusal. Both httpd and sed advanced beyond their original return refusal.

### Independent diagnosis before subsequent edits

The six K1 byte mismatches all first differ after the actual emitted function
ends: the probe was reading GCC's size into the next section/function because
EvoCC omitted ELF symbol sizes. This is a measurement-boundary defect, not
evidence for changing instruction semantics; exact subject symbol extents and
section-bound checks are required, not a wider relocation mask.

The original 2,799-unit K1 replay `measure-20260907T170640Z`, invoked through
`python3 tools/bench.py kernel --jobs 4 --allow-qemu-drift` (wrapper run
`20260907T170640.283040Z`), measures compiler
`171ab6aaeca625acc11a3d8127137e0edcf2f7d022665498fec2558246b7c52a`.
All 52 formerly assembled units remain assembled, with 2,747 refusals unchanged.
The mixed kernel boots and the substitution proof now has 9 exact matches,
30 matches modulo the existing relocation masks, zero mismatches, 2 no-symbol
units and 11 units outside the vmlinux link. Each of the six former mismatches
now verifies. The kernel adapter remains INCOMPLETE: 41 linked units, of which
39 carry verified bytes, are not 2,799 compiled kernel units or a kselftest pass.
This run precedes the function-address and static-code-relocation fixes below;
its compiler identity is not attributed to those later changes.

Four original wide-bit-field torture tests (`bitfld-3`, `bitfld-5`, `pr32244-1`,
`pr34971`) each exit zero with sealed GCC-14 and 134 with Clang-19 at `-O0`.
Their GCC-specific width arithmetic cannot be treated as a dual-oracle
positive requirement. Original failure counts remain unchanged.

Torture `pr19687` is already covered by Appendix D's inactive-union-tail
decision: GCC and Clang agree at O0 but disagree at O2. Its E3001 does not
authorize initializing bytes outside the profile's selected union member.
`pr22493-1` and `pr23047` explicitly request `-fwrapv`; their native traps
cannot justify removing default signed-overflow obligations.

Two additional actual defects have tests-only checkpoints and focused fixes:

- `9e4f3a2`: function addresses were routed through object storage, and
  `&*get()` through an object-pointer operation. The lowerer now uses the
  resolved function call interface, while both consumers retain a function
  designator's pointer without loading memory. Fifteen address/relocation
  regressions pass, including exactly-once calls and `&*null` cancellation.
  Original c-testsuite `00095` and torture `941014-1` now pass natively.
- `4b50b19`: torture `20071220-2` emits `.xword bar^b1`, which the assembler
  reads as an invalid expression. Static relocations must name the same
  platform-local labels as `block_addr`, including blocks not named by an
  `adr`. Four relocation tests and eight block-address tests now pass;
  original torture `20071220-2` now assembles, links and executes successfully.

`python3 tools/bench.py ctest torture --jobs 4` finishes as INCOMPLETE, not
PASS, in wrapper run `20260907T174641.388158Z`, using compiler
`4746bc92624a640dd1236d6bb12a93ccd79b05c835c19f928e5736ffd6d79c17`.
Native runs `20260907T174641Z` and `20260907T174705Z` preserve the original
220 and 1,692 program identity sets respectively:

- c-testsuite: 194 to 195 pass, 24 to 23 refused, 2 failures unchanged;
  `00095` is the only changed outcome.
- Torture: 1,175 to 1,185 pass, 478 to 469 refused, 35 runtime failures and
  3 link failures unchanged, assembly failures 1 to 0. Newly passing originals
  are `20000314-3`, `20000914-1`, `20071220-2`, `941014-1`, `memchr-1`,
  `pr41239`, `pr46309`, `return-addr`, `strlen-2`, and `strlen-3`.
- Both suites retain every formerly passing program; zero regressions.
  These gains cover the void-return fix as well as the function-address and
  static-label fixes, so they are not attributed entirely to the last patch.

The final working-tree gate is
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh 4b50b19 --quiet`:
1,314 passed, zero failed, 4 previously ignored tests remain ignored, Clippy
and fmt pass. This is the selected reverse-dependency crate set, not a new
whole-workspace or isolated-clone claim. Python discovery in `tools/k1/tests`
and `tools/ladder/tests` passes 86 and 93 tests; Ruff and diff-check pass.

A goto entering a scope past an array declaration is reduced to
`.evidence/builtin-probes/20260908-goto-storage/probe.c`; both sealed oracles
return zero and C-HIR agrees, while lowering requests an unavailable edge
address. The missing value is a real allocation, not a scalar phi: substituting
zero or cloning an ordinary allocation origin would violate the object model.
This remains an implementation issue, not a permission to weaken validation.

The first unavailable Csmith input, base seed 11, has a concrete nonterminating
induction cycle, not an EvoCC checksum disagreement. `func_25` returns `&g_37`,
so `func_23` takes the arm containing
`for (g_37 = 0; g_37 >= -28; g_37 = safe_sub_func_uint8_t_u_u(..., g_37, ..., 6))`.
That macro explicitly returns `uint8_t`; each update is in 0..255 and the body
does not change the counter. The loop cannot reach its negative exit bound.
The original sealed run already has four timeouts and a Clang O2 exit 1 for
this seed. A further GCC O1 UBSan check times out; Clang's trapping UBSan build
exits 139 (the non-trapping sanitizer runtime is unavailable in the image).
None of those outcomes is a PASS or authority to rewrite the original seed.
The other 67 unavailable inputs remain to be classified independently.

Final confirmation command (only after the applicable incremental checks):

```sh
python3 tools/bench.py all --jobs 4 --programs 500 --budget 40 --allow-qemu-drift
```

An incomplete lane, unresolved issue, or deferred prerequisite is reported as
such; it is never changed to PASS merely to reach an all-green summary.
