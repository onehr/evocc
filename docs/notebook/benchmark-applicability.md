# Reviewed original-program applicability

Authority: architecture review of 2026-09-09. Original program identities and
raw compilation/execution outcomes remain in the benchmark; a classification
does not turn a refusal into PASS. The per-program contract binds these
decisions to the exact source digest and requires the evidence files to exist.

## Csmith unavailable-reference accounting

Capture `.evidence/ladder/csmith/20260909T193124Z/results.jsonl`, subject
`691b654`, retains all 1,000 original campaign/seed identities: 932 agree and
68 remain oracle-unusable, with zero compiler refusals or mismatches.
Every per-program result, including all five execution statuses and checksums,
is identical to capture `20260909T162714Z`; generator options, defines and
runtime digests are unchanged. A ten-second timeout alone proves neither
undefined behavior nor nontermination.

The reviewed two-way analysis labels account for every unavailable identity:

| Campaign | Repeating source path identified | Undetermined; timeout is not a proof |
|---|---|---|
| base | 11, 307 (2) | 14, 33, 42, 64, 68, 83, 87, 88, 93, 112, 120, 122, 128, 161, 204, 223, 237, 239, 241, 246, 287, 315, 322, 327, 340, 345, 362, 442, 466 (29) |
| bitfields | 307, 457 (2) | 11, 14, 21, 24, 26, 54, 69, 83, 104, 114, 115, 122, 128, 132, 146, 162, 173, 192, 206, 234, 239, 240, 242, 259, 292, 322, 340, 352, 357, 360, 362, 404, 455, 466, 481 (35) |

The source arguments for these four identities are in
[the closure notebook](benchmark-closure.md); the bounded optimization probes
for both 307 identities and bitfields/457 are in
`.evidence/builtin-probes/20260909-csmith-loops/results.txt`.
Their repeating source paths explain why the reference executions do not
provide a common terminating checksum; they do not turn an unusable input
into a passing compiler test. All 68 keep their original benchmark label,
and individual source classification of the other 64 remains open.

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
