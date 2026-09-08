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

The user's follow-up authorization on 2026-09-08 covers the four previously
blocked body changes: real variable-length objects, integer vectors, symbolic
assembly immediates and PIE-safe strong external-data addressing. These are now
implementation work, not pending questions. Each change must still specify and
test its semantics across the consumers it affects; floating-point execution
alone stays deferred. Closing this inventory does not close unmeasured roadmap
milestones or turn unusable oracle programs into passes.

The numbered rows remain the inventory authority; this checklist schedules
their remaining closure work without creating a second set of measurements.

- [x] Restore nested macro push/pop state; replay the original failures (row 1).
- [ ] Finish native wrong-result classification and repairs; preserve runtime
  obligations and consume required test options explicitly (rows 4, 17).
- [x] Repair goto-entered automatic storage, initialization and lifetime;
  replay toybox cut/httpd/sed and control-flow regressions (rows 8, 12).
- [ ] Finish aliases/weak symbols and register-binding original-program
  validation, separating FP-dependent libc failures (rows 2, 3).
- [x] Correct strong external-data and function addressing for the original
  TinyCC PIE links; all seven assembled objects now pass together (row 11).
- [x] Replay assembly register alternatives and kernel substitution with the
  exact measured binary; close each substitution mismatch (rows 5, 6).
- [ ] Resolve the remaining kernel assembly/symbol/control-flow constructs,
  retaining explicit specification conflicts rather than bypassing them (row 7).
- [x] Classify remaining toybox function-value and object-subvalue forms:
  all eight measured sites require deferred FP execution (row 9), not PASS.
- [ ] Admit unused system-header types independently of FP execution (row 10).
- [ ] Classify and implement admitted declaration/call forms, not a blanket
  diagnostic-code exception (row 13).
- [ ] Implement the required integer-vector behavior under a complete contract
  and replay the original initializer/place failures (row 14).
- [ ] Close the variable-length-object requirement under an authorized contract;
  a current profile refusal is not a non-FP pass (row 15).
  The reduced `void f(int n, int values[n]);` also reports E2001 for the
  earlier parameter instead of the existing E2203 profile boundary; its
  parameter-name visibility defect remains separate from prototype `[*]`.
- [ ] Reduce and repair every remaining non-FP diagnostic family, including
  builtins, ABI, initializers and the remaining TinyCC compile refusals
  (rows 11, 16).
- [ ] Account for every original Csmith seed, including all 68 unavailable
  references; never replace a seed or count a timeout as agreement (row 18).
- [x] Make cJSON tests execute the subject objects, not rebuilt reference C
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
| 4 | Remaining native failures | ctest 1 runtime failure; torture 23 runtime failures, zero assembly/link failures; nine option-dependent originals and three absolute-value originals now pass | `ctest torture` | PARTIAL: storage, packed access, wrapping and absolute-value repairs benched; other failures OPEN |
| 5 | Kernel substitution mismatches | Fresh `measure-20260908T102907Z` boots with 76 substituted units, 64 linked; corrected NOBITS memory-image replay proves 61 and records 3 no-symbol units, zero mismatches, same linked-image digest | `kernel` | CLOSED for the identified cache/extent/zero-fill defects; original failed proof retained beside `proof-zero-fill.json` |
| 6 | Assembly register alternatives | All twelve authorized spellings select `r` after one evaluation; native operand/branch tests pass, and the original kernel replay has no unresolved alternative or register-alias first error | `kernel` | CLOSED for the identified selection/binding defects in `20260908T123730.141577Z`; later assembly failures remain in row 7 |
| 7 | Remaining kernel refusals | Fresh survey: 2,712 `%a` assembly references, one computed goto and one frame address; all prior `asm:i`/`+rm` first errors clear | `kernel` | OPEN: 85 assemble, 83 substitute and boot, 69 linked; kaslr missing-global and insn signed-shift failures isolated and retained; zero substitution mismatches in the passing combination |
| 8 | Return and edge typing | toybox now 195/203, zero regressions; cut/httpd/sed all assemble and enter the passing mixed build, preserving all 203 original identities | `toybox` | CLOSED: return and goto-storage fixes benched in `20260908T080325.213428Z` |
| 9 | Other toybox lowering refusals | Eight retained refusals: args/xwrap/sort call strtod, hash calls sin, seq calls fabs, printf calls strtold, od loads a long double, uptime computes FP load averages | `toybox` | FP-DEFERRED: every measured first site classified; none counted as PASS |
| 10 | Unused system-header types | Nominal target types move cJSON to its actual FP builtin/value boundaries; zstd reaches 37 integer-NEON header declarations and 5 pretty-function sites | `zstd cjson` | PARTIAL: 12 measured header identities implemented and replayed; integer vector declarations remain OPEN |
| 11 | TinyCC compiler build | 7/11 assemble and pass together; all original PIE link failures are repaired; four compile refusals remain | `tinycc` | PARTIAL: external object/function addresses CLOSED in `20260908T102835Z`; remaining source forms OPEN |
| 12 | Switch, labels, function prototypes | Nested cases, promoted dispatch, discarded abrupt exits, unprototyped calls, enum/integer pointer compatibility and jump-entered storage pass original replay | `ctest torture toybox` | CLOSED for these identified constructs; remaining declaration forms stay in row 13 |
| 13 | Torture declaration coverage | 106 E1001 refusals, not 106 proven instances of one cause; source locations now retained for classification | `torture` | OPEN |
| 14 | Integer vectors | 23 excess initializers and 17 projected-place failures mention vectors; non-FP does not imply scalar; keep vector execution distinct from header admission | `torture` | OPEN |
| 15 | Variable-length objects | 19 torture refusals plus other lanes; explicit profile limitation, not a solved test or silent miscompile | `torture musl` | OPEN |
| 16 | Remaining diagnostic classes | Torture 459 total refusals and musl 352 remain listed in raw results; the three added enum-mode refusals replace ignored attributes and remain explicit oracle conflicts, not passes | `torture musl` | OPEN |
| 17 | Torture applicability/options | Signed-wrapping modes now pass nine previously failing originals; 326 programs still carry unhandled requirements, recorded rather than guessed or removed | `torture` | PARTIAL: wrapping semantics and original replay CLOSED in `20260908T135846.118517Z`; remaining requirements OPEN |
| 18 | Unavailable Csmith references | Fresh `20260908T080716Z`: all 1,000 emit; 932 agree, 68 oracle-unusable, zero mismatch; original identities and outcomes unchanged | `csmith` | OPEN: unavailable references are not passes |
| 19 | cJSON subject test linkage | Native mutations of either installed object fail; original replay relinks both objects into the public test and 54 public-API assertions without recompiling their implementations | `cjson` | CLOSED: mechanism replayed in `20260908T121307Z`; 0/2 subject objects means the milestone is still INCOMPLETE |
| 20 | Reference baseline failures | musl 17 and toybox 1; separate environmental/applicability faults from subject regressions | `musl toybox` | OPEN |
| 21 | Missing milestone adapters | Mbed TLS, PolyBench, llvm-test-suite, jq, Lua, SQLite, full-kernel and kselftest: eight NOT_IMPLEMENTED entries; prerequisites and FP deferral remain visible, not silently waived | named adapters | OPEN |

## Progress

### Explicit legacy integer declarations

`46228fa` is the tests-only RED for explicit GNU89 omitted integer types,
identifier-list parameters, undeclared calls, mode macros and exact option
propagation into emission and diagnosis. The default GNU11 behavior remains
strict. The implementation creates ordinary linked function declarations,
retains parsed qualifiers and rejects non-callable objects with the measured
E2937 rule; no missing function is replaced by an interpreter model.
Both sealed compilers agree on the positive and negative probes in
`.evidence/builtin-probes/20260908-legacy-declarations/results.txt`.
Both interpreters and native Darwin pass the execution fixtures. The focused
run passes 44 Rust tests and 18 Python tests. The changed all-targets gate
against `46228fa` passes 1,257 tests across eight crates, zero failures and
three pre-existing standing-run ignores; Clippy, fmt, Ruff and diff-check pass.
The first gate found two call-target regressions: missing-parenthesis syntax
must retain its diagnostic priority, and a precedence fixture had incorrectly
declared a called function as an integer object. Both are corrected and the
gate rerun is green. The 72 originally refused GNU89 programs still require
original-program replay; neither their success nor complete GNU89 conformance
is inferred from the focused tests.

### Flexible character-array initializer storage

`4ca0c26` is the two-test RED for direct file-scope flexible members initialized
by narrow, wide or empty strings. The declared array type remains incomplete;
the shared string extent calculation supplies storage, including the terminating
zero, without changing `sizeof` of the record. Fixed arrays retain their exact
bound, including omitted terminators and zero-filled padding. Both interpreters
and native Darwin now execute the fixture successfully.
The separate RED `0131fa1` prevents accidentally admitting nonempty flexible
initializers nested in another record or union: GCC and Clang disagree on the
string form, so it remains E2202. Empty nested lists are accepted by both and
remain accepted; automatic flexible initialization retains E2919.
Sealed GCC-14/Clang-19 O0/O2 results and the negative admissions are recorded in
`.evidence/builtin-probes/20260909-flexible-strings/results.txt`.
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true
tools/gate-changed.sh 4ca0c26 --all-targets --locked --offline --quiet`
passes 1,986 tests across all 17 workspace crates, zero failures and four
pre-existing standing-run ignores; Clippy and fmt pass. The shared merge changed
Cargo.lock, so this is a full-workspace rather than five-crate gate.
The repair is `b0da087`; wrapper `20260908T172546.081411Z` replays the exact
original corpus using executable SHA-256
`1c4c9532fa7d152b134e09a5a9b4e503e8c074a178d5187a2adc6f7927d35b25`.
All identities are unchanged: torture `20010924-1` changes from REFUSED to
PASS, while `pr28865` stays REFUSED and now reports the measured nested-form
E2202 instead of E3101. No outcome regresses. Torture is 1,210 passes,
459 refusals and 23 failures out of 1,692; ctest stays 206 passes, 13 refusals
and one failure out of 220. Captures: `.evidence/ladder/e0/20260908T172546Z`
and `20260908T172644Z`. The direct storage defect is closed; the retained
oracle conflict is not counted as a success.

### Compatible function declarations and actual definition arity

`0f0a6a5` is the tests-only RED for compatible unspecified declarations
following prototypes, including typedef-based declarations and an empty
definition following a zero-parameter prototype. Lowering now reuses the
type interner's parameter-compatibility rule for declarations without
weakening a definition's bindings or arity. The accompanying negative
regression exposed an existing parser bug: an empty definition after a
one-parameter prototype was accepted. It now retains E2002, as do conflicting
return types and promotion-incompatible prototypes.
Both interpreters and native Darwin execute the fixture successfully;
GCC-14 and Clang-19 at O0/O2 agree, and both reject all three negative probes
(`.evidence/builtin-probes/20260908-function-redeclarations/`).
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true
tools/gate-changed.sh 0f0a6a5 --all-targets --locked --offline --quiet`
passes 1,630 tests across 12 affected crates, zero failures and four existing
standing-run ignores; Clippy, fmt, Ruff and diff-check pass. The repair is
`4f7bee9`; original replay `20260908T163813.683179Z` uses executable SHA-256
`23f7bd31fb89bfbc21b26589870f3ec155ad8978e2918b9fae9ecba7ec6361bc`.
All original identities are unchanged: torture `const-addr-expr-1` changes
from REFUSED to PASS, while `comp-goto-1` remains refused and reaches the
existing E3029 computed-goto boundary instead of E3101. No outcome regresses.
Torture is now 1,209 passes, 460 refusals and 23 failures out of 1,692;
ctest remains 206 passes, 13 refusals and one failure out of 220.
Captures: `.evidence/ladder/e0/20260908T163814Z` and `20260908T163937Z`.
This closes the identified composite-declaration defect, not all of row 13.

### Variadic record return placement

`f065b94` is the tests-only RED for the original `920726-1` call mismatch.
The IR return destination is now placed before anonymous arguments, while
the machine's indirect return address is placed last in its call operand
list so it goes to x8. Fixing only the first exposed a native write to
address 30, the final anonymous argument, rather than closing the case.
Both interpreters and native Darwin now pass the direct/indirect small and
large record regression; both Linux cross-link directions pass with GCC-14
and Clang-19 at O0/O2 (eight executions). The changed all-targets gate against
`f065b94` passes 764 tests, zero failures and four pre-existing standing-run
ignores; Clippy, fmt, Ruff and diff-check pass. The repair is `7f55a01`.
Evidence: `.evidence/builtin-probes/20260908-variadic-record/results.txt`.
The subject assembly uses the specified GNU assembler; the initial Clang
integrated-assembler refusal of `:pg_hi21:` is recorded, not called a pass.
Original replay `20260908T160746.894867Z`, executable SHA-256
`4bbe84757e3e6adbdb70a53cbd5140a3d13cfe1f8108e0d8162f47b6f73dd2e0`,
changes exactly `920726-1` from REFUSED to PASS, with no regressions.
Torture now has 1,208 passes, 461 refusals and 23 failures out of the same
1,692 programs; ctest is unchanged at 206/220. Captures:
`.evidence/ladder/e0/20260908T160747Z` and `20260908T160812Z`.

### Kernel replay and linked object definitions

`python3 tools/bench.py kernel --jobs 4 --allow-qemu-drift`, wrapper
`20260908T141252.550489Z`, captures subject `be2349c` and executable
SHA-256 `a29b0d83db55706c5e14041b6e4271330e3422ed4103c3d04af8e3dc75b588e8`.
All 2,799 original identities remain: 85 assemble, 2,714 refuse. Six formerly
refused units assemble; none moves in the other direction. All 2,712 E3021
first errors now name `%a0` at processor.h:390, not `i` or `+rm`.
The 85-object link fails on kaslr's missing `__kaslr_is_enabled`; removing
only kaslr exposes a boot timeout, independently reduced to `insn.o`.
The retained 83-object combination boots, with 69 linked objects: 15 proven,
51 proven modulo relocations, three no-symbol and 14 not-linked, zero proof
mismatches. These exclusions are failures, not successful substitutions.
Capture: `.evidence/k1/measure-20260908T141252Z`; passing Image digest
`90ee50eec552e947e3b2cb3dfe93ac5e21f365488b49cecd2092d9bd66983d5e`.

`ce08737` is the tests-only global-storage RED, `3f629ec` the repair:
declarations bound in several scopes share one storage identity, irrespective
of the first declaration's scope. Both interpreters and native execution now
pass the int, bool, incomplete-array and static-address cases. The affected
all-targets gate against `ce08737`, Clippy, fmt, Ruff and diff-check pass.
A direct replay of the original kaslr command using that debug subject emits
one exported `__kaslr_is_enabled` and its `strb`; the kernel measurement above
predates the repair and is not its mixed-link closure.

The exact pre-repair `insn.o` and reference object agree on basic immediate
encoding, but the first negative branch-offset decoding traps in EvoCC
(exit 133), versus reference result -16. The reduction is `(imm << 6) >> 4`.
Both sealed compilers return `-2 -2147483648 -16` for the standalone signed
shift probe at O0/O2; Clang's LLVM `shl` has no `nsw` flag. GCC explicitly
documents this signed-shift extension even though UBSan diagnoses it:
<https://gcc.gnu.org/onlinedocs/gcc/Integers-implementation.html>.
Evidence: `.evidence/builtin-probes/20260908-instruction-encoding/`.
The current body's strict shift rule conflicts with this GNU behavior;
an amendment has been requested, not bypassed. The `%a` contract conflict
is separately measured in `20260908-assembly-address/address.c` at both
optimization levels and with both `p`/`r` inputs. Neither is an FP issue.

### Integer absolute values and runtime-domain classification

`d598bfd` implements the three explicit integer absolute builtins and the
ordinary standard-name call selection, with options and same-unit assembler
identities preserved. The original tests-only checkpoint is `a3884e3`.
Both interpreter/native focused tests pass; the affected all-targets gate
against that checkpoint passes 1,032 tests, zero failures or ignored tests,
plus Clippy, fmt and Ruff. Original-program replay now closes the three
identified absolute-value cases, not the remaining native failures.
`afabd27` separately records the missing benchmark builtin-option projection
as a tests-only RED; `b01821d` implements it after the frozen kernel run
finishes, with all 42 targeted Python tests passing.

`python3 tools/bench.py ctest torture --jobs 4`, wrapper
`20260908T151818.777128Z`, measures subject `d160f3f`, executable SHA-256
`1ad01a326c9969a433b49e3beaf66f6c72afee0d934ce698f8309ffc449744a3`.
ctest remains 206 passed, 13 refused and one failed out of 220; torture is
1,207 passed, 462 refused and 23 failed out of the same 1,692 identities.
Exactly `20021127-1` moves FAIL to PASS and `pr34130` / `pr42614` move
REFUSED to PASS, with no regressions and no assembly/link failures.
Captures: `.evidence/ladder/e0/20260908T151818Z` and
`.evidence/ladder/e0/20260908T151844Z`. This executable predates the
abrupt-expression repair below; that repair has no benchmark closure yet.

The original `arith-rand` and `arith-rand-ll` each pass ordinary sealed GCC-14
and Clang-19 at O0/O2, but **both independent UBSan instrumentations** report
signed-minimum negation at source line 86. Twelve other retained torture
failures and ctest `00200` produce signed-left-shift diagnostics in both
instrumentations. `pr7284-1`, with its required C89 mode, is a sanitizer
disagreement: GCC exits zero, Clang reports signed shift overflow. These are
not new compiler passes. In particular the signed-shift reports alone do not
establish that these GNU programs are out of scope: GCC's documented extension
above is diagnosed by UBSan too. Keep that contract issue open independently
of ordinary C signed-overflow diagnostics.
Evidence and exact instrumentation/link settings are
`.evidence/builtin-probes/20260908-runtime-domains/results.txt`; Clang uses
the installed GCC sanitizer runtime because its own standalone archive is
absent. Every original program and headline outcome remains unchanged.

### Newly reduced control-flow case

`12f9eb5` and `d80c546` pin abrupt exits in value expressions, including
comma operands and conditionals with no normal continuation. Lowering now
retains a predecessorless insertion point, not a fabricated value or an edge
from the abrupt exit. The C-HIR task machine discards the comma's left operand
without requiring a scalar result. Fifteen focused lowering tests and the
native CLI regression pass; the affected all-targets gate against `12f9eb5`,
Clippy, fmt, Ruff and diff-check pass. The repair is `33bd17c`.
The reduced `int f(void) { int first=({ goto done; 0; }); int k=5;
done:return k; }` now lowers and both interpreters report E3001 for the
reached uninitialized read. It must never be folded into a constant return.
The original replay `20260908T154642.149536Z`, executable SHA-256
`c52a9d530825b65b1f288cd0daa4d8b3d93df8d6b27bfeac35a69c01c15f34f6`,
preserves every prior outcome: ctest 206/220, torture 1,207/1,692,
zero regressions. The new regression fixes a diagnosed semantic hole;
it is not an additional original-program pass. Captures:
`.evidence/ladder/e0/20260908T154642Z` and `20260908T154708Z`.

### Latest completed incremental replay

`python3 tools/bench.py ctest torture zstd cjson --jobs 4`, wrapper
`20260908T135846.118517Z`, captures clean `02d66b1` and executable
SHA-256 `4bc8805812e92f47ef470b9532a57aad85598a750c1c71a1899640514c856f36`.
The production includes wrapping and target-header types; its assembly RED
tests are deliberately still red, and no later assembly fix enters this run.
ctest stays 206/220, with 13 refused and one runtime failure. Torture keeps
all 1,692 identities: 1,204 pass, 464 refuse, 24 fail, zero link/assembly
failures. Exactly `20040409-1w`, `20040409-2w`, `20040409-3w`, `920612-1`,
`930529-1`, `950704-1`, `pr22493-1`, `pr23047` and `pr57124` move FAIL to
PASS; no other program changes outcome. The two header-dependent E1001
refusals now report unsupported value execution, not a new pass.
Native captures are `20260908T135846Z` and `20260908T135911Z`.

zstd remains 2/44 in its passing mixed build (`20260908T140139Z`); 37 units
now reach `arm_neon.h:40`, `typedef __Int8x8_t int8x8_t`, and five reach
`__PRETTY_FUNCTION__`. cJSON remains 0/2 (`20260908T140339Z`): `cJSON.c`
reaches `__builtin_nanf`, and `cJSON_Utils.c` reaches a floating-point
function type. Its public tests still execute the installed reference
objects; this is not an EvoCC cJSON pass.
The header implementation passed the changed all-targets gate against
`57bc8c2`: 1,889 passed, zero failed, four pre-existing ignored; Clippy,
fmt, Ruff lint and diff-check passed. This is not the final offline replay.

`python3 tools/bench.py kernel --jobs 4 --allow-qemu-drift`, wrapper
`20260908T123730.141577Z`, captures clean subject `bc7cc9b` and executable
SHA-256 `fc07b29798f1bc914a0064a3a7f90c2cb43cd253f6e29f0e830e9a64aa892a69`.
All 2,799 original unit identities remain: 79 assemble and 2,720 refuse.
Only `drivers/acpi/apei/ghes_helpers.o` changes assembly outcome (refused to
assembled); the 2,260 prior register-alias errors now reach later assembly
constraints. The exact new frontier is 2,716 `asm:i` at jump_label.h:54,
one at jump_label.h:38, one `asm:+rm` at vdso/math64.h:13, one computed goto
and one frame-address refusal. No count is inferred from a probe.
All 79 assembled objects substitute and relink, including both formerly
failing EFI objects; the mixed image boots. The substitution proof has
13 proven, 49 proven modulo relocations, three no-symbol and 14 not-linked
objects, zero mismatches: 65 linked objects, not a full EvoCC kernel.
Capture is `.evidence/k1/measure-20260908T123730Z`, image digest
`18aba753678cc7273f5e6edf4e1e4ecca6f1558ac66a474913c1a5027cfa5bc4`.
The wrapper correctly exits INCOMPLETE. This run predates signed-wrapping
and target-header-type changes and is not evidence for those changes.

Signed-wrapping options now pass the CLI/native regression and
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true
tools/gate-changed.sh dbd89ee --all-targets --quiet -- --test-threads=1`:
1,018 tests, zero failures or ignored tests in the five selected crates,
Clippy and fmt green. The three option/preparation Python tests and thirteen
benchmark-entry tests pass, as do Ruff lint and diff-check. The default and
explicitly disabled modes still report E3004; shift, division and pointer
checks remain tested. The original torture replay is recorded above; its
measurement retained every source and did not change the earlier kernel
run's binary or results.

The digit-builtin, register-alias and expression-scope static fixes pass
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true
tools/gate-changed.sh afc4c63 --all-targets --quiet -- --test-threads=1`:
1,015 tests, zero failures or ignored tests in the five selected crates,
Clippy and fmt green. Ruff lint and diff-check pass. This is an incremental
all-targets gate, not the final whole-workspace/doctest replay. Their original
kernel replay is recorded above, separately from the preceding measurements.

`python3 tools/bench.py ctest torture tinycc cjson --jobs 4`, wrapper
`20260908T120936.530123Z`, measures executable SHA-256
`ba3d6f93ef7b053ff642c9f98c43a5a9032e2352c482a450d31052e56a839135`.
The captured subject is `a81c243` with only a new C test fixture pending;
the measured production includes `5b3c87a`, not the later digit/register fixes.
Every original ctest and torture identity is retained. ctest remains
206 pass / 13 refused / 1 failed out of 220. Torture becomes
1,195 pass / 464 refused / 33 failed out of 1,692: `memclr` and `misalign`
change from FAIL to PASS, while `20050107-1`, `20050119-1` and `20050119-2`
change from PASS with ignored enum attributes to E2903 under the measured
enum-mode conflict. These five are the only outcome changes; this is not a
zero-regression claim or closure of the non-FP objective.
TinyCC remains 7/11 assembled and all seven pass together; `tccpp` advances
past the repaired ordinary-call directive case to an FP call refusal.
cJSON relinks and executes its actual installed objects successfully, but
both are reference objects because its two subject units still hit header
declarations. This validates the adapter repair, not an EvoCC cJSON PASS.
Native runs are `20260908T120936Z`, `20260908T121000Z`,
`20260908T121237Z` and `20260908T121307Z`, respectively.

Shared symbol addressing and link-time assembly operands `7ac11d3` are
measured by `python3 tools/bench.py tinycc kernel --jobs 4 --allow-qemu-drift`,
wrapper `20260908T102835.690828Z`, frozen executable SHA-256
`b3b730c0afa50abb5c4821db6c5997966f42a2b08b980ef064c7a00665b7a95b`.
Its completed TinyCC lane `20260908T102835Z` rises from 6/11 to 7/11 in
the passing mixed build: `tccrun.o` joins the original six, all eleven
identities and assembly outcomes are unchanged, and no isolated relink
failure remains. Four compilation refusals remain; this is not TinyCC PASS.
The kernel lane completed with 78/2,799 assembled, of which 76 enter a booting
mixed link after excluding two EFI units referencing `__efistub_isdigit`.
The initial proof falsely reports two NOBITS objects as mismatches; the
wrapper's original ERROR is retained. After `0e44fde`, replay on exactly the
same linked image gives 13 proven, 48 proven modulo relocations, 3 no-symbol
and 12 not-linked units, zero mismatches (`proof-zero-fill.json` and
`reconciled-receipt.json` in `measure-20260908T102907Z`). NOBITS checks exact
symbol extent and zero-filled memory, not nonexistent file bytes; malformed
or missing extents still fail. This is 64 linked units, not a full kernel.
The binary was not rebuilt while the lane consumed it; later C-preprocessor
and test-linkage changes are not attributed to this measurement.

Integer `mode` and packed-subobject execution landed in `5b3c87a`; the
original memory failures are now replayed above, with the enum conflicts
kept separate from the successful repairs.
The exact memory fixture passes both sealed compilers at O0/O2 and the two
interpreters plus native execution; the changed gate found an old wide-access
test that required natural alignment even for packed storage. Its replacement
pins both reduced power-of-two alignment and invalid alignment refusals.
The successful serialized rerun is `CARGO_TARGET_DIR=target/frontend
CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh f747dc5
--quiet -- --test-threads=1`: 1,541 pass, zero fail, four pre-existing ignored,
Clippy and fmt green; Ruff lint and diff-check also pass. `cargo test` with
the same ten-package set and `-- --list` lists 1,545 including those four.
The preceding 1,540-pass/one-failure attempt remains a failed gate.
The enum-mode oracle conflict remains visible: the three original programs
named above previously passed with ignored attributes, but the oracles disagree on
the resulting type compatibility. This is not a new PASS or a reason to drop
the original program; `_Bool`, enum and abstract-type-name mode conflicts
are explicitly refused under the existing conflict protocol.

The final serialized gate against `c489ffa` passes 1,533 tests, zero
failures, four pre-existing ignored tests, plus Clippy, fmt, Ruff lint and
diff-check. An earlier invocation passed its native tests but failed a
doctest after a concurrent Cargo rebuild replaced a dependency; that attempt
is not green. The final replay uses one Cargo invocation through completion.
Both sealed compilers link and run the final symbolic-immediate fixture and
the undefined-symbol-only-in-a-comment control. The latter exposed and fixed
spurious runtime address loads; pure asm-literal definitions need no runtime
reference, while ordinary uses retain their allocation and address materialization.

External-data GOT addressing `9065012` is measured in
`20260908T093536.969536Z` (`python3 tools/bench.py tinycc --jobs 4`), native
run `20260908T093537Z`: TinyCC rises from 4/11 to 6/11 units in a passing
mixed build. All eleven identities and all seven assembly outcomes remain;
`arm64-link.o` and `tcc.o` join the previously passing four. The remaining
`tccrun.o` link now reports a direct relocation against `longjmp`, a function
address rather than the earlier `environ` object. It remains OPEN, not hidden
behind the passing data-address probe. Capture `9065012` is clean, binary
SHA-256 `5e03b5a43164641ff0ff5107ee2e09c9a35a4b8416668c394e624483801e67fc`.
The changed gate against `3b0065a` passes 518 tests, zero failures, with four
pre-existing ignored tests; Clippy, Rust fmt, Ruff lint and diff-check pass.
The sealed GCC-14/Clang-19 static/PIE source and subject probe has eight
build/run successes, and the Darwin native regression passes. Ruff formatting
of the untouched Python tree was also inspected: 43 existing files do not
match `ruff format`; no unrelated formatter rewrite was made or claimed green.

The prototype-array repair `858b8c2` is measured in
`20260908T090630.550681Z` (`python3 tools/bench.py ctest torture --jobs 4`):
c-testsuite rises to 206/220 and torture remains 1,196/1,692. Original
`00162` is the only changed outcome, refused to native PASS; both identity
sets and all earlier passes are unchanged. The source checkout `91158d1`
is clean at capture, with executable SHA-256
`384941528b53ad45a9c9ff1ae2879476bf01e017db673e28d31ffb02892e5f31`.
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true
tools/gate-changed.sh f1c2453 --quiet` passes 990 tests, zero failed or
ignored, plus Clippy, fmt, Ruff and diff-check. This closes prototype `[*]`,
not real VLA execution or the entire declaration-coverage family.

The attributed-type repair `1cd7701` is measured in
`20260908T085411.464607Z` (`python3 tools/bench.py ctest torture --jobs 4`):
c-testsuite rises to 205/220 and torture remains 1,196/1,692. Original
`00210` is the only changed outcome, refused to native PASS, with all
identities and earlier passes preserved. The five-crate changed gate passes
985 tests, zero failed/ignored, plus Clippy, fmt, Ruff and diff-check.
Executable SHA-256 is
`d9d3077f8221e8679fcf9610f050799d2e9b5c7330fdb5a19f779a0329748e50`;
the checkout is `5aeea8c`, with only the next tests being written while its
release build finished. Prototype-array production changes were made later
and are not included in this replay.

Toybox's eight remaining first sites were individually replayed with the
same adapter transformations (including re-escaping regenerated flags.h).
They require FP execution: `lib_args`, `lib_xwrap` and `sort` reach strtod;
`lib_hash` reaches sin; `seq` reaches fabs; `printf` reaches strtold;
`od` has a long-double subvalue; `uptime` forms FP load averages.
`CFG_TOYBOX_FLOAT` is one in this measured configuration. All eight remain
refusals in the 203-unit universe, not invented non-FP passes, and this
does not waive other outstanding toybox input-adaptation/baseline issues.

The line-control repair `2d063ef` is replayed in
`20260908T084118.829770Z` (`python3 tools/bench.py ctest torture --jobs 4`):
c-testsuite rises to 204/220 and torture remains 1,196/1,692. The only
changed program is original `00152`, refused to native PASS; no identities
or earlier passes are lost. Its changed gate runs 1,090 tests with no
failures or ignored tests, Clippy, fmt, Ruff and diff-check green.
The source identity is `0a914b7`, executable SHA-256
`213abc7bd72465d157a2346d1bdff87f76b39b4043fb635028ce119dd26f4a92`;
the dirty flag records pending test/receipt work, not the subsequent
type-name implementation. Earlier attempt `20260908T084012.993329Z`
could not start the sealed execution environment and remains NOT RUN,
not evidence of a compiler regression or a passing replay.

`python3 tools/bench.py toybox ctest torture csmith --campaign all
--programs 500 --jobs 4 --budget 40` completed run
`20260908T080325.213428Z`: toybox 195/203, c-testsuite 203/220,
torture 1,196/1,692 and Csmith 932/1,000 agreeing. All original identities
remain present, with no outcome regressions; toybox cut/httpd/sed are the
only changed outcomes, refused to assembled, and join the 195-unit
no-regression mixed build. Its one reference-baseline failure remains visible.
Csmith still has 68 oracle-unusable results and zero mismatches.

The run correctly records dirty checkout `82e2cff`: the already-staged
production patch was committed as `2f268ab` after the wrapper captured its
checkout identity. The measured executable SHA-256 is
`040927dddb910127976bff07817b36faaeb9c2d95d381688273bd57ccd3a8884`;
it was not rebuilt during the campaign. This is not a clean-commit replay.
The goto repair passed 981 tests, zero failed/ignored, through
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true
tools/gate-changed.sh 82e2cff --quiet`, plus Clippy, fmt, Ruff and diff-check.
The following line-control change is not attributed to this executable.

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

Native arithmetic classification was extended without changing any original
program or benchmark outcome. Both instrumented compilers at O0/O2 report
overflow/invalid shifts on twenty programs when their per-program options
are omitted. Seven are **not** invalid-test exclusions: `20040409-1w`,
`20040409-2w`, `20040409-3w`, `920612-1`, `930529-1`, `950704-1`
require `-fwrapv`, and `pr57124` requires `-fno-strict-overflow`; all 28
correct-option instrumented executions pass. The adapter and execution
profile must implement those requested semantics explicitly. Clang uses the
system UBSan runtime because its standalone runtime is absent. `pr7284-1`
requests C89 and has a GCC/Clang sanitizer-dialect disagreement, retained
separately. Per-program first faults and both command forms are in
`.evidence/builtin-probes/20260908-torture-options/results.txt`.

`memclr` and `misalign` also expose ignored integer `mode` attributes,
not merely inline-assembly interpreter refusals: `mode(QI)` remains a
four-byte unsigned int instead of the one-byte type both oracles use.
The reduced sizeof assertion is RED E2911, and the sealed mode probe
measures ranks, signedness, member layout and truncation. GCC/Clang disagree
on applying `mode(QI)` in an abstract type name (sizeof 1 versus 4), so that
placement must not silently choose a width. This shared type repair remains
OPEN and precedes less consequential coverage work.

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
This was an implementation issue, not a permission to weaken validation;
`2f268ab` and replay `20260908T080325.213428Z` close it as recorded above.

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
Base and bitfields seed 307 have now also been inspected. Their preprocessed
bodies are byte-identical (SHA-256
`c0cc0b2e72bfc407c6352251c0a548b2e9e334c063693dcf9b9497c8a6b068a4`).
`func_22` reaches `lbl_213` with `p_24=65535` and `l_207=6`; each pass resets
`g_49` to zero and a nonzero 16-bit sum takes the goto before its update.
In a bounded diagnostic of the unchanged source, GCC-14 O2 times out with
or without `-fno-finite-loops`; Clang-19 O2 prints the old checksum by default
but times out with that option. The option also removes its emitted
`llvm.loop.mustprogress` metadata. This is an optimization-dependent reference
domain, not a checksum that every oracle supplies; both original verdicts
remain oracle-unusable. The compact commands, output and source reasoning are
in `.evidence/builtin-probes/20260909-csmith-loops/results.txt`.
The other 65 unavailable inputs still need independent source classification;
none of the 68 unavailable originals is counted as agreement.

Final confirmation command (only after the applicable incremental checks):

```sh
python3 tools/bench.py all --jobs 4 --programs 500 --budget 40 --allow-qemu-drift
```

An incomplete lane, unresolved issue, or deferred prerequisite is reported as
such; it is never changed to PASS merely to reach an all-green summary.
