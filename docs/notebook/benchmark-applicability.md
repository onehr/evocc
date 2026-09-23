# Reviewed original-program applicability

Authority: architecture review of 2026-09-09. Original program identities and
raw compilation/execution outcomes remain in the benchmark; a classification
does not turn a refusal into PASS. The per-program contract binds these
decisions to the exact source digest and requires the evidence files to exist.

## Scalar floating profile update — 2026-09-20

The earlier floating tables below are dated source analyses, not a blanket
current exclusion. Scalar binary32/binary64 execution and its ABI are now
implemented. A fresh emitted program must pass its actual link and execution;
an old FP label cannot hide either failure or prevent a new PASS.
Only a source-bound `deferredFeatures` review can retain a current FP refusal:
binary16/bfloat16/binary128, floating complex/vector, or strict fenv.
The complete scalar-profile torture run exposed 39 surviving reviewed refusals;
reinspection of unchanged source hashes classifies 18 binary128, 15 floating
complex and 6 floating vector. Seventeen LLVM copies with identical verified
source hashes receive the same feature refinement, retaining their own argv
bindings. Neither set is counted as PASS. See `torture-floating-frontier.out`.
The current c-testsuite `00204` still requires binary128 HFA/varargs execution;
`00174`'s new failure instead exposed missing `-lm` in the harness and was fixed,
not waived as floating execution. Fresh complete lane results are recorded in
the [evaluation notebook](benchmark-evaluation-20260915.md).

## Csmith unavailable-reference accounting

Capture `.evidence/ladder/csmith/20260909T193124Z/results.jsonl`, subject
`691b654`, retains all 1,000 original campaign/seed identities: 932 agree and
68 remain oracle-unusable, with zero compiler refusals or mismatches.
Every per-program result, including all five execution statuses and checksums,
is identical to capture `20260909T162714Z`; generator options, defines and
runtime digests are unchanged. A ten-second timeout alone proves neither
undefined behavior nor nontermination.

The fresh scalar-FP campaign `20260919T222655Z` again retains these same
68 unavailable identities (932 agreements, zero mismatches). The diagnostic
refresh below changes source analysis, not any original execution verdict.
The reviewed two-way analysis labels account for every unavailable identity:

| Campaign | Repeating source path identified | Undetermined; timeout is not a proof |
|---|---|---|
| base | 11, 14, 33, 42, 64, 68, 83, 87, 88, 93, 112, 120, 122, 128, 161, 204, 223, 237, 239, 241, 246, 287, 307, 315, 322, 327, 340, 345, 362, 442, 466 (31) | None (0) |
| bitfields | 11, 14, 21, 24, 26, 54, 69, 83, 104, 114, 115, 122, 128, 132, 146, 162, 173, 192, 206, 234, 239, 240, 242, 259, 292, 307, 322, 340, 352, 357, 360, 362, 404, 455, 457, 466, 481 (37) | None (0) |

The source arguments for the first four identities are in
[the closure notebook](benchmark-closure.md); the bounded optimization probes
for both 307 identities and bitfields/457 are in
`.evidence/builtin-probes/20260909-csmith-loops/results.txt`.
Their repeating source paths explain why the reference executions do not
provide a common terminating checksum; they do not turn an unusable input
into a passing compiler test. All 68 keep their original benchmark label,
and the dated diagnostic sequence below accounts for the remaining identities.

On 2026-09-20, the unchanged 68 sources were independently compiled at O0
by both sealed oracles with a separate diagnostic sampling translation unit,
[csmith-timeout-sample.c](../../tests/fixtures/csmith-timeout-sample.c).
All 136 diagnostic runs reached three sampled PCs and exited **77**, meaning
sample completion, not benchmark PASS. Source/command/runtime hashes and
symbolized locations occupy one 154 KiB capture:
`.build/floating-targeted/csmith-oracle-timeout-samples-20260920.json`, SHA-256
`6d3ab40325606ca66653df9692e9eb194bc63dec5b4e8f11ce43b7967b0f5171`.
Sampling locates execution; it does not prove nontermination or definedness.
Inspection of the reached source paths adds these eight identities:

| Original | Source-level repeating transition |
|---|---|
| base/42 | Lines 129–156: `p_10` is reset to 9 and decremented to -26; the following loop immediately takes `if (p_10) goto lbl_285` before its update. Each restart repeats the reset. |
| base/64 | Lines 312–317: unsigned `l_587` increments; `g_83` is reset to 1, then the true `if (g_83)` returns to `lbl_590` before the loop update. |
| base/88 | Lines 89–121: one loop is skipped after resetting its index to -11; the next resets `g_6` to 9 and takes its backedge before changing it. The intervening array writes do not change the predicate. |
| base/128, bitfields/128 | Lines 53–102: the unsigned decrement remains defined; resetting `g_3` to 0 makes its `<= -2` loop unreachable, leaving `g_15=5` and its backward branch true. The two C bodies match after excluding generator/header and final statistics comments. |
| base/237 | Line 35: the signed destination receives an unsigned 8-bit subtraction result, always 0..255, so `> -27` remains true; subtracting 6 modulo 256 has a 128-step cycle. |
| base/287 | Line 82: unsigned 32-bit `g_8` begins at 29 and subtracts 8 through unsigned 64-bit arithmetic. Its residue stays 5 modulo 8, so it can never reach 0 or 1, the only values that end `> 1`. |
| bitfields/11 | Its C body matches already-reviewed base/11 after excluding those comments; both oracles again sample the same loop at line 1062. |

The finite reduced transition checks in
[csmith-loop-invariants.c](../../tests/fixtures/csmith-loop-invariants.c)
pass both EvoCC interpreters, EvoCC native execution, and GCC-14/Clang-19
O0/O2. Exact identities and the scope limitation are recorded in
[csmith-loop-invariants.out](../../tests/fixtures/csmith-loop-invariants.out).
These controls are not replacement programs, a whole-source UB proof, or
authorization to discard any of the 68 original seeds.

Seven further reached loops have a narrower, type-range explanation:

| Originals | Counter update and impossible exit |
|---|---|
| base/112 (line 1010), bitfields/259 (154), bitfields/357 (341) | A u16 subtraction result is assigned to i32. Its range is 0..65535, so respectively `!= -26`, `> -29`, and `>= -29` stay true. |
| base/327 (170), bitfields/192 (47), bitfields/322 (428) | A u8 subtraction result is assigned to i16/i32. Its range is 0..255, so respectively `!= -9`, `> -9`, and `>= -11` stay true. |
| bitfields/404 (379) | The i64 counter starts at -27, then receives a u32 increment result (first 4294967270); no later value can equal -18. Its inner loop changes a different object. |

The inspected bodies do not change their induction counter or escape the
loop. [csmith-narrow-loop-invariants.c](../../tests/fixtures/csmith-narrow-loop-invariants.c)
and its [execution record](../../tests/fixtures/csmith-narrow-loop-invariants.out)
check the conversion boundaries and finite u8 cycles with both interpreters,
EvoCC native execution, and both sealed oracles at O0/O2. The current source
analysis at that checkpoint was **19 identified / 49 undetermined**, while the actual campaign
remains 932 agreements, 68 oracle-unusable, and zero mismatches.

Two subsequent source inspections identify base/315 (line 87) and
bitfields/69 (line 155): both i32 counters receive u16 subtraction results,
which cannot satisfy the negative exit thresholds (`<= -7` and `== -25`).
Their loop bodies neither change those counters nor escape the loops.
[Finite controls](../../tests/fixtures/csmith-unsigned-update-invariants.c)
pass both interpreters, EvoCC native, and both sealed oracles at O0/O2;
[the record](../../tests/fixtures/csmith-unsigned-update-invariants.out)
binds the unchanged original sources and explains the range argument.
That checkpoint was **21 identified / 47 undetermined**, with all original
campaign outcomes and seeds unchanged.

Base/68 (lines 527–532) repeatedly takes the const-parameter branch before
its for-update; the target resets the counter to zero. Bitfields/115
(149–155) similarly restarts its initializer after a structure self-copy
that preserves the nonzero branch predicate. Both oracle samples reach these
paths. [Finite reset/self-copy controls](../../tests/fixtures/csmith-restarted-paths.c),
including the terminating zero-predicate cases, pass both interpreters,
EvoCC native and both sealed oracles at O0/O2. The original source hashes and
scope are in [their record](../../tests/fixtures/csmith-restarted-paths.out).
Both /122 sources also repeatedly take the branch at lines 337–348: a
nonnull saved object pointer stays unequal to null; local pointer-array
initialization changes neither that pointer nor the nonzero `g_37` guarding
the backward goto. Its selected array element is null but is not dereferenced
on this path. The same finite fixture now covers these two identities and
passes all five execution variants after explicitly bounding the reduction's
pointer-array iterations; the initial E3008 resource stop is retained in its
record. That checkpoint was **25 identified / 43 undetermined**.

The same fixture now covers base/241's reset after a bounded function call:
`g_10=2`, then `g_25.f0=1`, then a true branch before either update; the
callee writes a distinct local and cannot prevent the caller's reset.
Base/246 initially remained undetermined because its XOR target can alias
its own guard. A subsequent read-only stopped-process observation of the
unchanged GCC-14 and Clang-19 O0 programs resolves the reaching state:
both have `g_31 == &g_32`, distinct from `&g_99`, while `g_99` remains
`0x66c5d899` and the XOR target alternates between 0 and 3. The previously
sampled `func_9` loop changes neither that pointer nor the guard; its inner
loop is skipped after resetting `g_306=29`. The existing finite control
already tests this exact nonaliasing guard, as well as aliasing/terminating
controls. This combines source reasoning, location samples and object/value
observation; it is not a claim of whole-program definedness or an original
terminating checksum. Commands, capture identity and execution hashes are
in the same [record](../../tests/fixtures/csmith-restarted-paths.out).
Both /83 sources have the same reached `func_17` cycle: a nonzero `static
const int64_t g_671` guards the tail goto. Its address never escapes and
neither branch changes it; the other u16/u32 counters wrap without altering
that control decision. A further finite fixture extension tests both branch
paths, chained pointer rebinding and the wrap boundaries. Both interpreters,
EvoCC native and the two sealed oracles at O0/O2 pass. Original identities
and the distinction between these controls and benchmark results remain in
the same record.
Three more localized paths are now resolved: base/87 resets its inner counter
to one and takes the goto before either nested-loop update; base/120's local
union guard is initialized to -1, never modified or escaped, and both arms
return to that guard; bitfields/242 assigns -8 to a u16 loop counter, skipping
the entire body and preserving the nonzero backward-branch guard. Finite
controls for the reset, unchanged union member, and unsigned initialization
pass both interpreters, EvoCC native and both sealed oracles at O0/O2; they
include zero-guard terminating controls. The same record binds their source
identities and current fixture/assembly hashes.
That checkpoint was **32 identified / 36 undetermined**; none of the 68
oracle-unusable benchmark outcomes is converted to PASS or removed.

**2026-09-23:** bitfields/352 adds one reached type-range cycle. Lines 444–520
assign a u8 decrement result to a signed i16 union member, keeping `>= -8`
true. The only reachable outer break tests an independent local array element
after `3 & 0xc538d2c8`, which becomes zero and remains zero. The nonzero
parameter either breaks the inner loop or allows its finite 0/9/18 progression;
the later return is inside an initializer-skipped loop. No call or other store
in the repeating body changes the counter, mask or tested element. The original
GCC/Clang samples reach this body, and its source digest still matches.
[The finite reduction](../../tests/fixtures/csmith-union-loop-invariant.c)
checks one u8 cycle, both inner paths and genuinely terminating mask controls.
Both interpreters, EvoCC native and GCC14/Clang19 O0/O2 pass; [identities and
limits](../../tests/fixtures/csmith-union-loop-invariant.out) are recorded.
That checkpoint was **33 identified / 35 undetermined**, not a new campaign:
all 68 retain oracle-unusable and the 932 agreement numerator is unchanged.

A further [finite reset/guard reduction](../../tests/fixtures/csmith-restarted-state.c)
resolves both campaigns' /14, /340 and /362, plus base/161, base/204,
bitfields/26 and bitfields/104. The first three pairs differ only in generator
and statistics comments; their executable text was compared, not inferred from
the seed. The source arguments distinguish written and tested array elements,
pass-by-value parameters from later pointee stores, skipped unsigned loops and
backward branches before the induction update. Both interpreters and GCC14,
Clang19 O0/O2 plus EvoCC native pass the finite controls, including terminating
zero-guard alternatives. [Hashes, lines and limits](../../tests/fixtures/csmith-restarted-state.out)
bind all ten to the existing original-process PC captures. Current analysis is
**43 identified / 25 undetermined** at that checkpoint. No original checksum, budget, source,
seed or benchmark verdict changed; these are not ten newly passing programs.

Read-only stopped-process observations now resolve base/239 and both /466
identities. Both oracles have the same unchanged nonzero guard in /239's
reset-before-update loop. In /466 the sampled callee loops themselves are
finite; its caller repeatedly resets the argument state and branches back on
the callee's eight counter increments. The original hashes, observed global
values/aliases and source transition argument are in
[the record](../../tests/fixtures/csmith-reset-call-cycle.out). Finite controls
pass both interpreters and all five native variants. Current diagnosis is
**46 identified / 22 undetermined**, still 68 oracle-unusable, not 68 PASS.

Eight more bitfield cases (/54, /114, /132, /162, /173, /206, /234 and /360)
are now bound to [finite transition controls and original state observations](../../tests/fixtures/csmith-invariant-restart.out).
The analysis distinguishes finite callees/inner loops from their restarting
callers, constant-selected paths and independent unchanged guards. Five require
additional read-only stopped-process observations; all ten diagnostic children
were reaped. Two interpreters and six native controls pass, without changing any
original source or result. That checkpoint was **54 identified / 14 undetermined**.
The then-remaining identities were base /33, /93, /223, /322, /345, /442 and bitfields
/21, /24, /146, /239, /240, /292, /455, /481. All 68 remain oracle-unusable.

**2026-09-24 — B03 accounting closed:** all fourteen now have
[source-bound diagnoses and original-process observations](../../tests/fixtures/csmith-final-restarts.out).
The analysis checks conversions and aliasing, skipped guard-changing paths,
and caller restarts after finite inner loops. The two permanent small C
controls pass C-HIR/EvoIR at O0/O2 (eight executions) and GCC14/Clang19/EvoCC
native O0/O2 (twelve executions). The evocc changed gate passes 503 tests,
Clippy and fmt. All 28 original diagnostic children were killed and reaped;
their diagnostic exit -9 is not a benchmark result. Current diagnosis:
**68 identified / 0 undetermined**, but still **932 agreeing / 68 oracle-unusable**.
No original source, seed, budget, checksum or historical numerator changed;
the final complete campaign remains B10, and this is not a whole-program
definedness proof or a new claim that all Csmith programs terminate.

## c-testsuite floating execution

Source pin: `5c7275656d75`, `tests/single-exec`. These seven originals require
floating execution on a reached path, not merely a floating declaration.

| Original | Reached floating operation |
|---|---|
| 00113 | Converts `a + 1` to float and compares the stored float against `a`. |
| 00119 | Reads initialized double `x` and compares it with 1. |
| 00123 | Reads initialized double `x` and compares it with 1. |
| 00174 | Executes floating arithmetic, conversions, formatted output and `sin`. |
| 00175 | Calls a function with floating arguments/results and formats the results. |
| 00195 | Stores and reads double members and passes them to formatted output. |
| 00204 | `main` reaches `arg` and `ret`, passing and returning float/double/long-double HFAs and printing their members; it also exercises floating varargs. |

The source files themselves are separately listed as evidence in
`tests/contracts/benchmark-program-dispositions.json`. Their original source
digests must match before the benchmark can apply FP-deferred.

The other six refusals in capture `.evidence/ladder/e0/20260909T062900Z`
are **not** assigned FP-deferred: 00140 passes an aggregate with an unused
float member but accesses only integer members; 00144 is a pointer-conversion
and uninitialized-value case; 00178 only takes `sizeof` of an uninitialized
double; 00207 requires VLA semantics; 00216 requires aggregate-initializer
analysis; 00220 requires the hosted wide-character representation.
They remain compiler failures until repaired or individually supported by
different applicability evidence. A search for the word `float` is not a
classification rule.

## Torture complex floating execution

Source pin: `78d4ac73dd391005b895a6148cd9831e28e1208b`. Each of these five
originals was read through its reached call path, then compiled and run
unchanged with both sealed compilers at O0/O2: all 20 executions return zero.
Commands and outcomes are in
`.evidence/builtin-probes/20260910-floating-applicability/results.txt`.

| Original | Reached floating operation |
|---|---|
| 20020227-1 | `main -> f1 -> f2` stores and compares a complex-float member in a packed record. |
| 20020411-1 | `foo` sets two floating components and invokes `conjf`; `main` converts the complex result to double and checks its components. |
| 960512-1 | `f` converts an integer array element to complex double; `main` evaluates the returned complex value's truth. |
| pr38969 | `main` passes initialized complex float through `bar -> foo` and compares the returned value. |
| pr42248 | `main -> init` stores a complex-double argument through a member; `check` compares that member with another complex-double argument. |

These identities receive FP-deferred, never PASS. Their existing raw failures
are retained, and the new labels are measured only by the next real replay.
`stdarg-4` was deliberately not included: its first refusal exposed an
integer-only always-inline list-copy defect, reproduced separately by
`tests/fixtures/inline-variadic-copy.c`; floating syntax is not permission to
hide that defect. After repair, original replay `20260909T171533Z` advances
from the line-19 list initializer to line 9's `va_arg(ap, double)`, called
from `main -> f1 -> f1i`. The full original passes sealed GCC-14/Clang-19
at O0/O2, and the integer-only reduction passes both interpreters and both
native links (`20260910-inline-variadic/results.txt` under the probe directory).
This identifies reached FP execution: the source-bound disposition now marks
it FP-deferred for subsequent replays, without relabelling the frozen run or
claiming the original passes. Integer `_Complex` cases remain separate open
coverage.

## Bracket-attribute original with an oracle disagreement

The unchanged torture `pr124358` compiles with both sealed compilers at O0/O2.
GCC-14 O2 aborts (exit 134), while GCC-14 O0 and Clang-19 O0/O2 exit zero;
commands and results are in
`.evidence/builtin-probes/20260910-attribute-namespaces/results.txt`.
Under R01 this identity is not-applicable (oracle conflict), not an inferred
compiler PASS. Its source digest is bound in the disposition table, and
subsequent subject replays still record its actual compile/run outcome.

## Explicitly excluded GNU function extensions

The architecture review excludes GNU nested functions, rather than weakening
ordinary function/label checking. Source inspection of the following 24
originals confirms an actual definition inside another function, including
the six cross-function label uses and the multi-label declaration case:

`20000822-1`, `20010605-1`, `20030501-1`, `20040520-1`, `20061220-1`,
`20090219-1`, `920415-1`, `920428-2`, `920612-2`, `920721-4`, `921215-1`,
`931002-1`, `nest-align-1`, `nest-stdar-1`, `nestfunc-1`, `nestfunc-2`,
`nestfunc-3`, `nestfunc-5`, `nestfunc-6`, `nestfunc-7`, `pr103405`, `pr24135`,
`pr51447`, `pr71494`.

`nestfunc-4` is deliberately absent: a filename is not a construct and its
passing original result is retained. The record for `nest-stdar-1` is the
explicit nested-function exclusion, not a second FP label.

The tailored-first source review on 2026-09-10 identifies five more originals
under already-decided exclusions, not five newly fixed compiler defects:

| Original | Source-bound reason |
|---|---|
| 920501-7 | `x` defines nested `y`, which jumps to the enclosing `xlab`; S06 applies even under its original gnu89 option. |
| comp-goto-2 | The same nested `y` and outer-label jump, in addition to a local indirect jump; S06 applies, not a blanket exclusion of computed goto. |
| pr117432 | `main -> fred/corge -> qux` requires a definition with only `...` and single-argument `va_start`; R01/F01 exclude these C23 forms. |
| pr123864 | `main -> foo` calls `__builtin_mul_overflow_p`; F09 excludes this builtin independently of the now-admitted bracket attribute. |
| 20241029-1 | `main -> foo` reaches `__builtin_puts`; F09 retains the specific oracle-conflict refusal recorded in Appendix D, not an exclusion of ordinary declared `puts`. |

Each exact upstream source and its digest is attached to the disposition
entry. These labels apply only on the next real campaign; capture
`20260909T215349Z` and its 268 compiler-failure count are not rewritten.
The two nested definitions above extend the 24-source list, while ordinary
computed goto and all in-profile integer varargs defects remain open.

The same review explicitly excludes these builtin contracts:

| Originals | Required out-of-profile construct |
|---|---|
| built-in-setjmp, pr60003, pr64242 | Compiler builtin setjmp/longjmp context and nonlocal return. |
| pr47237 | `__builtin_apply` and `__builtin_apply_args`. |
| va-arg-pack-1 | Inline forwarding with `__builtin_va_arg_pack`. |
| pr105777, pr105984, pr30314 | `__builtin_mul_overflow_p`, including the two pinned gcc.dg sibling includes. |
| 20001111-1, pr103052, pr115033 | `__builtin_exit`, distinct from ordinary supported library calls. |

The exact original sources and relevant included sibling sources accompany
each contract entry. Existing raw refusals are recorded by
`.evidence/ladder/e0/20260909T062923Z/results.jsonl`; the classification
applies to those identities, not every diagnostic with the same code.

`pr115033` was previously blocked by the independently missing trap builtin.
Its unchanged original now has a separate reference measurement in
`.evidence/builtin-probes/20260910-builtin-trap/results.txt`: GCC-14 O0/O2
compile and execute successfully, while Clang-19 O0/O2 refuse the
`__builtin_exit` at line 34. The F09 disposition applies on the next actual
subject replay, not retroactively to the earlier sealed failure or as a PASS.

`991014-1` is the separately reviewed extreme-type oracle conflict:
GCC-14 admits its huge array types and Clang-19 refuses them. Its structure
size is still below PTRDIFF_MAX; it must not be described as an overflow of
that limit. The probe in `20260909-large-layout/results.txt` supports the
oracle-conflict disposition, not an invented smaller numeric layout limit.

## Unevaluated type queries and actual floating execution

`20040709-1` and `20040709-2` initially stopped at the integer-only `T(A)`:
lowering still visited the unselected `5.25` assignment after resolving
`__builtin_classify_type`. That was a compiler defect, not an FP exclusion.
After the branch repair, both untouched originals reach `T(W)` at line 114;
their mains call `testW`, which stores and compares a long-double member.
Both references execute each original at O0/O2 with exit zero. Diagnostic
reductions retaining all A–V integer tests also execute correctly under
both references and the subject's emitted machine code. The reductions do
not replace either original benchmark input or count as original passes.
Evidence: `.evidence/builtin-probes/20260910-type-classification/results.txt`.

Three additional originals require executed FP, not merely a floating
declaration: `20000603-1` performs double addition, `20000731-1` uses a double
loop variable, and `20001017-1` passes double arguments. All twelve unchanged
reference compile/run pairs succeed, recorded in
`.evidence/builtin-probes/20260910-floating-execution/results.txt`.
The five source-bound FP-deferred entries apply to the next replay; they
neither change previous captures nor add any subject PASS.

## New refusals of formerly passing originals

Capture `.evidence/ladder/e0/20260909T074333Z` has six fewer raw torture
PASS results than `20260909T062923Z`. The loss is retained and is not counted
as a compiler improvement. The review's 15:30 supplement narrows the earlier
blanket wide-field refusal: the first three remain compiler failures pending
shape-specific reduction and repair, not new inapplicability entries.

| Originals | Source-level reviewed boundary |
|---|---|
| 991118-1, bf64-1 | Compound arithmetic on 52/53-bit fields; OPEN under the narrowed conflict-only rule. |
| bf-sign-2 | Arithmetic on an unsigned 35-bit field; OPEN under the narrowed conflict-only rule. |
| pr86659-1, pr86659-2, pr87623 | Explicit `scalar_storage_order`, whose presence must be refused rather than silently ignored. |

Both oracles at O0/O2 pass **all 24 executions** of these original programs;
Clang warns that it ignores the storage-order attribute. Evidence:
`.evidence/builtin-probes/20260909-wide-field-conflicts/additional-originals.txt`.
These are not six newly observed oracle conflicts. Only the three explicit
storage-order attributes remain reviewed-profile exclusions; the other three
must not be relabelled to hide the over-broad refusal. New classification
entries apply on the next actual replay, not retroactively to this sealed
run's counts or raw outcomes.

## Variable-array source classification (tailored, 2026-09-10)

The twenty E2203 rows in frozen torture capture `20260909T215349Z` are not
twenty ordinary VLA allocation defects. Targeted sealed syntax checks of
the unchanged sources separate the following twelve non-consensus forms:

| Source construct | Original identities | Measured result |
|---|---|---|
| Variable-sized record members, including packed/aligned and typedef forms | `20020412-1`, `20040308-1`, `20040423-1`, `20041218-2`, `20070919-1`, `align-nest`, `pr41935`, `pr82210` (8) | GCC-14 accepts; Clang-19 refuses the variable member. R02's common-extension boundary applies. |
| Nested function definitions | `20010209-1`, `pr22061-3`, `pr22061-4`, `921017-1` (4) | GCC-14 accepts; Clang-19 refuses the nested definition. S06 applies; `921017-1` retains its original gnu89 option. |

Exact commands, exit codes and first-error locations are retained in
`tests/fixtures/variable-array-original-refusals.out`; each disposition binds
the original source digest. No program was removed or counted as PASS, and
the frozen receipt was not rewritten. The six reductions in
`variable-record-members.c` also keep the implementation fail-closed through
typedefs; ordinary callback members with prototype-local array bounds remain
accepted and executed.

Two other E2203 originals (`20040411-1`, `pr22061-2`) have tailored native
passes recorded in `captured-array-originals.out`, not new campaign PASS
counts. Six identities remain: `20040811-1`, `20221006-1`, `920721-2`,
`920929-1`, `pr43220`, `vla-dealloc-1`. Their dynamic object/lifetime work and
any separately evidenced floating-execution classification remain OPEN.

## Reference-baseline failures, not subject regressions

The 17 musl failures in `20260909T132842Z/baseline-REPORT` and toybox's one
failure in `20260909T133141Z/baseline-tests.log` remain failures in their
original captures. The following groups partition those 18 test identities;
they are not additional compiler program counts or newly passing tests.

| Cases | Count | Observed baseline stop |
|---|---:|---|
| musl `api/main` | 1 | The pinned libc-test requests `_PC_TIMESTAMP_RESOLUTION` and `_SC_XOPEN_UUCP`, absent from the reference musl headers; its executable is not built. |
| musl `dlopen`, `tls_align`, `tls_align_dlopen`, `tls_init_dlopen`, `tls_get_new-dtv` | 5 | The recorded lane uses `--disable-shared` and static test linkage; these tests require dynamic loading or link a shared object. |
| musl `strptime`, normal and static | 2 | Reference parsing disagrees for `%s` and abbreviated `%z`. |
| musl `strtold`, normal and static; `powf` | 3 | Reference floating rounding/exception mismatches. |
| musl `wordexp`, normal and static | 2 | Reference word expansion and shell diagnostics disagree with the test expectations; library-versus-shell attribution remains open. |
| musl `daemon-failure`, normal and static | 2 | Timeout while the test fills the descriptor table; the observed 1,048,576 descriptor limit is a hypothesis for cost, not a proved cause. |
| musl `pthread_atfork-errno-clobber`, normal and static | 2 | The test expects `RLIMIT_NPROC=0` to prevent a fork, but the container runs as root. |
| toybox `chattr immutable` | 1 | The mounted filesystem rejects the initial flag query with ENOTTY. |

The last two rows have an independent, bounded environment reproduction in
`.evidence/builtin-probes/20260910-baseline-context/results.txt`: a read-only
flag query returns ENOTTY; a single root fork succeeds at NPROC=0, while the
same process after dropping to UID 65534 receives EAGAIN. This changes no
benchmark timeout, source, denominator or receipt. No full baseline-clean
claim is made, and the remaining runtime causes still require investigation.
