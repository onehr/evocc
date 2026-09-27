# Strict floating-point implementation checkpoint

This extends the existing scalar profile; it is not a new FP implementation
or a kernel rerun. Later sections record the hosted quad and jq replay; their
acceptance boundaries do not retroactively strengthen earlier checkpoints.

## Numerical prerequisite

Commit `7eb4097` adds explicit rounding and exception results to the pinned
APFloat adapter, and immutable FP environment snapshots to `WorldState`.
The independent SoftFloat/TestFloat run checks **2,630,976 value/status pairs**.
The retained target instruction probe checks **22,568 records** in each of
GCC14/Clang19 O0/O2; the four outputs have SHA-256
`a9b2612bea820e5c1916aef16cfad6804030a1caba55b8c8dcc78d5448a7134e`.

Measured repairs include Arm tininess-before-rounding, missing APFloat
directed-overflow status when returning maximum finite, and Arm FMA NaN
selection in addend/first-multiplicand/second-multiplicand order. Production
semantics still use APFloat, not host arithmetic or the independent oracle.

## IR and machine extension

EvoIR 1.9.106 makes `fenv` mandatory and adds `fp.strict.*` with explicit current
world input/fresh world output. Exceptions are sticky; no memory, volatile or
atomic event is invented. Explicit FMA is distinct from multiply-plus-add.
Both FP families remain outside P0. All operands and effect metadata participate
in validation, renumbering, proof reconstruction and mutation rejection.

EvoMIR 0.45 preserves strictness and the logical width of narrow integer
conversions. Its FPCR admission permits the four rounding modes for strict
operations, retains the fixed-mode check for baseline arithmetic, and invalidates
admission across opaque calls/assembly. No check writes FPCR/FPSR. Scalar FP
continues to share the existing physical SIMD bank.

The pure conversion-definedness predicate first classifies NaNs as integer bits:
an FCMP on an sNaN would otherwise introduce an undeclared INVALID effect.
Strict raw invalid conversions return the specified total value and raise INVALID;
C's separate E3063 guard remains mandatory. Strict 128-bit integer helper
conversions were initially refused; the qualified extension is recorded below.

Historical sealed certificates are not rewritten. Their reader is explicitly
selected (`--legacy-pre-fenv`); current parsing rejects a missing field. The
historical reader rejects strict FP and new FMA forms. New proofs bind current
canonical bytes. The pinned random seed corpus retains its old hashes through
this explicitly selected historical serialization, not changed expected digests.

## Native targeted execution

Fixtures: `floating-strict-world.evoir`, `floating-strict-conversions.evoir`,
and the reference-compiled `floating-strict-ir-driver.c`.
`evocc-mir`'s `evoir_assembly` example uses actual validation, lowering,
allocation, independent allocation checking and Linux emission.
Each GCC14/Clang19 O0/O2 caller executes **62 checks, PASS**, covering four
rounding modes, prior and new flags, NaN payloads, signed zero, narrow conversion,
pure definedness checks, and FZ/DN rejection in disposable subprocesses.
This is **IR/backend evidence**, not C strict-lowering evidence.

All native runs reuse the offline `evo-flight-lab:sealed` configuration
`2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2`.
Target compilers are Debian GCC 14.2.0-19 and Clang 19.1.7.

Reproduction (from the repository root, existing target directory):

```sh
CARGO_TARGET_DIR=target/frontend cargo run --locked --offline \
  -p evocc-mir --example evoir_assembly \
  < tests/fixtures/floating-strict-world.evoir > /tmp/strict-add.s
CARGO_TARGET_DIR=target/frontend cargo run --locked --offline \
  -p evocc-mir --example evoir_assembly \
  < tests/fixtures/floating-strict-conversions.evoir > /tmp/strict-conversions.s
# In the sealed target, for each compiler and -O0/-O2:
gcc-14 -std=gnu11 -O2 -Wall -Wextra -Werror -frounding-math \
  -ffp-contract=off floating-strict-ir-driver.c strict-add.s \
  strict-conversions.s -o strict-ir-driver
./strict-ir-driver
```

Small local logs live in `.build/floating-targeted/strict-20260927/`; no new
kernel clone, guest disk or persistent TestFloat corpus was allocated.

## Binary128 audit, not implementation acceptance

`floating-quad-helpers.c` records 6,400 exact arithmetic/result/status/control
tuples. All four target compiler/optimization combinations produce SHA-256
`112819c39437cd68af96d7d9b929a76115cb691aa189ca6766e29abc0b484a11`.
The hosted archive is `/usr/lib/gcc/aarch64-linux-gnu/14/libgcc.a`, SHA-256
`60dbf748e1c310aa50300a33f30bf4ee070061071aee74a52cf115a7b8765e50`.
The helper inventory includes arithmetic, comparisons and f32/f64/integer
conversions. Compiler agreement here shares a runtime and is **not** an
independent numerical-engine differential. f128 IR, source lowering, ABI and
original jq replay remain required.

## Open work

Checkpoint verification: `cargo test --workspace --locked --offline
--no-fail-fast` exits 0 with **2,735 PASS / 0 FAIL / 4 historical ignored**,
336 result groups. Workspace/all-target Clippy with `-D warnings` and fmt pass.
Python K1 tests: 251 PASS; ladder tests: 256 PASS (including three grammar-hint
regressions). Ruff passes. The shape hint reports 54/54 shapes; it is not a
grammar-completeness proof. These tests reuse `target/frontend` and retain the
old release executable for historical measurements.

The subsequent 1.9.107/0.46 source batch implements last-option-wins
`-frounding-math`, explicit C-HIR world execution and executed-expression
folding boundaries. Automatic initializers and conditional conversions observe
runtime rounding; static initializers retain RNE. Quiet GNU relational builtins
signal only sNaNs; ordinary C relational operations signal all NaNs. Both
interpreters check ten entries under four rounding modes at O0/O2, preserving
prior flags. The native source fixture executes 40 checks in each of eight
EvoCC O0/O2 and GCC14/Clang19 O0/O2 caller combinations, all PASS. The fixture
and reference caller are `floating-dynamic-rounding{,-driver}.c`; no source
substitution or host-FP constant evaluation is used.

The source checkpoint's full workspace test run exits 0 with 2,738 PASS,
0 FAIL, 4 historical ignored and 338 result groups; all-target Clippy passes.
The independently added quiet-comparison proof-mutation test also passes.

## Target fenv resolver

Revision 1.9.108 adds explicit memory/world library models, not host fenv
calls. Native glibc measurement establishes that raise/update of overflow or
underflow also sets inexact. The explicitly selected musl adapter retains
musl's requested-bit-only raise behavior. Both interpreters use the GNU/glibc
binding and pass the unchanged `floating-environment-library.c` probe:
32 requested masks in four modes, selective flag transport, save/hold/update,
default environment and executed arithmetic. Eight EvoCC O0/O2 × reference
caller compiler/O0/O2 builds execute the same probe with exit 0. Two reference
thread-driver builds each execute 128 synchronized opposite-rounding operations
against EvoCC code, PASS. This does not claim an interpreter pthread model.

Memory negatives retain initializedness, alignment, bounds and unsupported
control checks. Pure FP after a library switches away from RNE now fails E3064
in C-HIR, EvoIR and MIR, matching the native admission rule. Targeted regression
and machine tests: 9 PASS. Current all-target Clippy passes.

The original eight-unit musl replay preserves `-frounding-math`: four units emit,
while ceil/floor/rint/rintf encounter scalar FP `w` assembly operands. The
current specification explicitly excludes those operands; their extension is
a requested decision, not a validator bypass. This replay uses the existing
command projector and records its omitted flags (including function/data
sections and excess-precision); it is not faithful full-build acceptance.

The later [standard-excess-precision driver regression](../../tests/fixtures/floating-standard-evaluation.out)
closes that particular omission under revision 1.9.112: FLT_EVAL_METHOD=0
already uses semantic source precision for operations, assignments and casts.
The explicit option now reaches the driver without changing numerical/rounding
semantics. Eight oracle and eight EvoCC native controls pass; both interpreters,
527 CLI regressions and 262 ladder regressions pass. The eight original musl
commands still divide four emitted/four scalar-assembly refusals; their other
option omissions and full-build acceptance remain open.

At that checkpoint `FENV_ACCESS ON` lexical scoping was not yet advertised;
the subsequently validated scope implementation is recorded below.
Binary128, musl, jq and full campaigns remain separately tracked
in `TODO.md`. No traps, Annex F, floating vectors/complex or half support is
implied by this checkpoint.

## Strict integer128 conversions (EvoMIR 0.47)

The existing scalar formats now use qualified Linux hosted helpers for strict
integer128 conversions. They do not acquire an f128 type. `bl.strict_fp`
preserves the admitted helper identity, ABI signature and interpreter status
contract; `fp.env.check.strict` is distinct from the fixed-mode check. C's
E3063 guard remains. Raw IR invalid conversion yields zero plus INVALID without
invoking a runtime helper on an out-of-domain input. Non-Linux strict helper
lowering remains refused rather than assuming the same runtime behavior.

`floating-wide-conversions.c` covers every integer exponent boundary with
nearby values, 512 deterministic random inputs and defined fractional/wide
FP-to-integer controls. Each of GCC14/Clang19 O0/O2 emits **36,784** records
with SHA-256
`ffa97d889c7b0d86b5d1ce25de2704c6e98f81f87934541bcaff072d1079ed90`.
The independent APFloat adapter checks every value, status and unchanged FPCR.
Actual EvoCC O0/O2 functions in `floating-wide-source.c`, called by all four
reference compiler/optimization combinations, reproduce that same stream:
**eight builds, 36,784 records per build, PASS**.

`floating-strict-wide.evoir` and its native driver additionally exercise all
32 prior flag masks under all four modes, including sNaN/qNaN/out-of-range raw
conversion: **768 checks in each of four reference caller builds, PASS**.
Both source interpreters agree across eight entry functions, four modes and
both optimization settings. MIR tests cover typed-call validation, text,
allocation, explicit environment/status and unsupported-target rejection.
This closes the strict wide-conversion gap only, not lexical FENV_ACCESS,
binary128 or either original application campaign.

## Lexical non-trapping FENV_ACCESS (EvoIR 1.9.109)

The retained RED was E3065 on `floating-fenv-scope.c`. Preprocessing now carries
ON/OFF/DEFAULT changes to the parser, including `_Pragma` and included source;
`-E` emits equivalent directives. Compound C-HIR nodes carry their effective
lexical policy, and the unit's optimization guard is independently derived by
walking that syntax. Inlining copies this policy rather than adopting the
caller's mode. No source-scope entry/exit changes runtime FPCR/FPSR.

Both interpreters and native code preserve policy through nested scopes,
function return conversion, switch dispatch, direct/computed goto, break,
continue and statement expressions. Static bytes remain RNE. Discarded
arithmetic and conversions still accumulate all five sticky flags. OFF retains
baseline semantics unless the independent `-frounding-math` option is active.
Malformed or misplaced directives remain errors.

The fixture includes the unchanged environment-library probe under ON.
**Eight entry programs pass in both interpreters at both optimization settings**;
**eight EvoCC/reference native combinations each pass all eight programs**.
Clang19 compiles/runs the same whole-source fixture at O0 and O2, also PASS.
Its constant-int-to-FP warning is retained in the native log; it does not change
the measured upward-rounded runtime result. GCC14 is used as a crossed caller,
not as evidence that GCC implements the FENV_ACCESS pragma.

The preceding strict-wide commit's complete workspace test exits 0 with
**2,745 PASS / 0 FAIL / 4 historical ignored**, 340 result groups.
The current combined scope/numerical prerequisite workspace run has
**2,753 PASS / 0 FAIL / 4 historical ignored**, 342 result groups. It retains a
regression found by the first full run: return conversion must transport a
same-type uninitialized result without consuming it merely because the return
scope is being exited. Integer and floating falloff/forward/discard controls
now pass. A separate overlapping-build attempt invalidated rustdoc dependency
metadata; the final run uses one Cargo invocation and includes every doctest.
The final changed gate also passes all-target Clippy and workspace formatting.
Original musl replay, scalar-FP assembly, binary128/jq and remaining TODO items
are still separate gates; no benchmark or kernel receipt was promoted.

## Binary128 numerical adapter prerequisite

The shared adapter now computes binary128 arithmetic, format/integer conversions,
comparisons and explicit FMA without host floating arithmetic. A private
113-bit-precision, extended-exponent APFloat format checks directed overflow;
using ordinary Quad as its own wider intermediate would miss overflow. This
format is never exposed as a C/IR type or serialized. The original parser already
rounds long-double literal representations directly to Quad.

The existing TestFloat 3e generator was rebuilt with `-DFLOAT64 -DFLOAT128
-DSOFTFLOAT_FAST_INT64`, using the retained ARM-VFPv2 SoftFloat 3e specialization.
Seed **20260927**, level **1**, `-tininessbefore -exact`: **908,544** independent
value/status records pass across four arithmetic operations, f32/f64-to/from-quad,
32/64-bit signed/unsigned integer-to-quad in all four modes, the reverse integer
conversions toward zero, and three comparisons. Explicit quad FMA in all four
modes passes another **24,532,992** records. All records were streamed, not saved
as a large corpus. Repeating f32/f64 arithmetic, comparison and conversion
controls with the extended adapter passes **1,794,624** independent records.

The previously captured **6,400** native arithmetic tuples now also pass the
independent adapter. The new `floating-quad-fma.c` gives **32,000** tuples in
each GCC14/Clang19 O0/O2 build, with identical SHA-256
`fa4d988ebd7866531935ab5179170cb11d85a2348e3416c9145c7963a73310fd`.
It checks unchanged FPCR and unchanged sentinel errno as well as exact result
bits and five flags. The adapter agrees on every tuple.
The measured runtime is glibc **2.41**; `/lib/aarch64-linux-gnu/libm.so.6`
has SHA-256 `a8afb2d1f4a0c5e14c0fc82f794ec839eafe29738907aa92895410f57630bbe2`,
with `fmal@@GLIBC_2.17`. The libgcc archive identity is recorded above. These
observations do not authorize arbitrary libm substitution on other runtimes.

Two target-specific representation facts must survive subsequent f128 lowering:

- Invalid non-NaN quad arithmetic returns positive NaN with all-one fraction,
  unlike scalar hardware's default zero payload. The
  [pinned GCC14 AArch64 runtime source](https://raw.githubusercontent.com/gcc-mirror/gcc/releases/gcc-14.2.0/libgcc/config/aarch64/sfp-machine.h)
  corroborates the measured default, payload selection, four rounding directions
  and tininess-before-rounding policy. TestFloat adapts only this precise default
  encoding; unrelated payload/status mutations must still fail.
- Hosted quad FMA prioritizes signaling addend, then signaling multiplicands;
  among quiet NaNs the multiplicands precede the addend. Scalar FMADD instead
  prioritizes a quiet addend. The native matrix caught this difference before
  f128 source/IR admission. Zero-times-infinity remains invalid even with a quiet
  addend.

Five small adapter tests retain rounding, payload, narrowing, tininess, overflow
and FMA regressions. Independent-checker mutation tests reject altered result
bits, flags and FPCR. This is numerical/runtime qualification, **not executable
f128 source, IR, MIR, public ABI or jq acceptance**. Those L02/B07 tasks remain.

The wide integer/quad conversion fixture adds **18,412** defined-domain records
per GCC14/Clang19 O0/O2 build, all with SHA-256
`c2c11d77795bb8bcc6fbc38a6c7ff7fd18603ecf60257d4475439b6e0a1d9c3c`.
The adapter agrees on every value, status and unchanged FPCR. The final types
all-target test run also executes the independent-checker mutation negatives.

`floating-quad-core.c` is the next source RED: EvoCC reports E3049 on the real
quad function interface. Three source programs cover arithmetic beyond binary64
precision, constants, objects, HFA transport, conversions, runtime NaNs and
dynamic rounding; an additional entry retains the uninitialized-read negative.
GCC14/Clang19 O0/O2 pass all three defined entries. NaN inputs are volatile:
the initial nonvolatile GCC O2 control folded the expression with a different
payload choice; it was not misreported as a target-runtime result.

## Binary128 source/IR semantics (EvoIR 1.9.110)

`f128` extends the existing scalar instruction family and exact-bit runtime
carrier. Constants now retain all 128 bits with exactly 32 hexadecimal digits.
Typed loads/stores, initializedness, block arguments, strict world effects and
E3063 conversion guards use the existing paths. No helper symbol is embedded
in EvoIR. Quad HFA and anonymous arguments retain explicit type/shape metadata.

The source RED now passes four defined entry programs (core precision/objects/
calls, runtime NaNs, dynamic rounding, nine anonymous quad arguments) plus
E3001 and E3063 negatives in both interpreters at O0/O2. A separate prefix
without any FENV_ACCESS directive tests ordinary constant folding. Exact IR
tests cover raw high/low payload bits, malformed widths, all four rounding
modes with prior flags, and mandatory quad HFA alignment. The independent
checker accepts unchanged reconstruction and rejects well-typed forgeries of
quad payload, operation, format and HFA role even with recomputed output hashes.
Former scalar-quad refusal cases move to positive typed-IR tests; floating
complex and target-header-only type negatives remain. Native quad lowering
still refuses explicitly in this semantic checkpoint.

The hosted comparison audit adds **229,376 checks per GCC14/Clang19 O0/O2
build**, including all 32 prior flag masks and four control modes. All pass.
Its initial hypothesis of unordered +/-1 was wrong: the pinned helpers return
**+2/-2** for le/lt and ge/gt families. The retained initial log records this
qualification finding; equality/unordered helpers signal invalid only for
sNaNs, while ordered helpers signal for any NaN. The
[GCC14 runtime interface](https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gccint/Soft-float-library-routines.html)
specifies sign-based comparison contracts; native lowering must not assume an
unmeasured exact unordered encoding. This is runtime qualification, not an
EvoCC native comparison PASS.

The format-conversion fixture adds **304 records per reference build** for
quad/single/double narrowing and widening, including subnormal/overflow,
midpoint and signaling-NaN boundaries. All four streams have SHA-256
`3a88df43ef470215f33d5f02e5eb1ba2ded1649d3d7cd9540588b281fc873f43`;
the independent adapter checker matches every bit, flag and control. Its
negative tests independently alter result, status, control and input width.
The 32/64-bit integer conversion fixture adds **9,256 records per reference
build**, hash `d5d5be476c703acc0d353e3faf9cd8272e1bc7992f5976647c798505b8900d07`.
The adapter agrees on all records. These fixtures retain separately compilable
subject functions for later crossed EvoCC/native execution; current results
qualify the target runtime only.

Changed-crate validation ran all 308 test groups: 2,411 passed, four retained
ignores, and one stale coverage-manifest sentinel failed. After updating that
independent sentinel, its targeted rerun passes (2,412 distinct tests total).
The full workspace Clippy/all-targets and formatting checks also pass. This is
the source/IR checkpoint, not a completed native or application gate.

## Hosted binary128 backend (EvoMIR 0.48)

The original native source RED reported E4009. Linux lowering now uses the
existing SIMD128 bank and qualified runtime calls. Typed loads/stores, HFA,
stack arguments, varargs and allocator call preservation share the established
paths. Darwin quad remains refused. Bit-only C conversion predicates neither
quiet sNaNs nor change status; invalid raw IR inputs never reach a helper outside
its integer-conversion domain.

The raw MIR width matrix exposed an APFloat signed bits<1> panic: its signed
conversion delegates to an unsigned width-zero operation. The shared adapter
now converts through width two and applies the real {-1,0} domain, preserving
inexact for valid fractional inputs and INVALID for out-of-domain inputs.
All three admitted formats have an isolated regression. No vendored code or
dependency pin changed.

Native EvoCC O0/O2 subject objects pass with GCC14/Clang19 O0/O2 callers:

- four core source programs;
- 6,400 exact arithmetic records;
- 32,000 ordinary-library FMA records, including unchanged errno;
- 18,412 integer128 conversion records;
- 9,256 integer32/64 conversion records;
- 304 format-conversion records;
- 327,680 equality/ordered/quiet comparison checks (all four modes and every
  prior sticky-status mask).

Each of those counts is **per one of eight builds**, not a new application
numerator. Four reference-only ABI controls and sixteen crossed ABI builds
pass both directions: mixed scalars, ninth quad argument, HFA2/3/4, HFA register
exhaustion, one shared mixed stack, variadic quad/HFA and live values across a
callback that changes v8..v15 upper halves. The source `fmal` remains a library
call; a failed `__builtin_fmal` probe was not relabeled supported.

Machine text distinguishes ordinary `bl`, `bl.fixed_fp` and `bl.strict_fp`.
This avoids applying pure-IR RNE semantics to an ordinary libm call that reads
the actual rounding mode and raises flags. Mode identity survives allocation
and independent text checks; fixed admission must reject a dynamic mode even
if an explicit admission marker is omitted.

The final native matrix used frozen subject SHA-256
`b107970071a9c1c199373774b237245b647aab76d8ca0eb6c509613b59345dad`.
Raw IR adds **5,376 total-conversion checks** and **32,000 explicit fused IR
records** per GCC14/Clang19 O0/O2 caller. All pass, including invalid inputs
without invalid C casts. `floating-quad-raw.evoir` and `floating-quad-fma.evoir`
are emitted using the validated `evoir_assembly` bridge; the native matrix is
retained in `tests/acceptance/quad-hosted-native.sh`. The changed-crate gate
against `9234f15` passes **2,416 tests, zero failures, four retained ignores**,
followed by Clippy/all-targets and fmt. Logs are `quad-native-frozen.log`,
`quad-raw-fma-native.log` and `quad-native-gate.log` in the small local directory.

## Original jq replay and discovered command gap

Run `20260927T080457Z` uses that exact frozen subject and the existing
case-sensitive sealed header export. All **69/69** target-C objects compile and
are installed; there are no refusals, exclusions, unmeasured build units or
make-rebuilt target objects. Original `make check V=1` passes all nine jq scripts,
seven Oniguruma tests and fourteen Oniguruma samples. The earlier attempt
`20260927T080351.108606Z` stopped at the case-sensitive-sysroot precondition,
before compilation, and is not a test failure or a PASS.

This establishes the real `nexttoward` quad interface and full-object execution
under the recorded command projection. Audit then found `-pthread` among omitted
flags: the driver currently rejects it, and the adapter drops it. B07 remains
open until this behavior-affecting option is implemented and the original replay
repeated. The pinned GCC14/Clang19 macro census shows `_REENTRANT=1` as its
preprocessing effect; user `-U_REENTRANT` still overrides it. No source or test
assertion is changed to compensate for the gap.

The subsequent minimal driver and projection tests first fail, then pass after
implementing the compile-time `-pthread` contract (EvoIR 1.9.111). Seven macro
and `-D`/`-U` precedence cases pass under GCC14/Clang19 O0/O2; all seven also
pass both EvoCC interpreters, preprocessing and assembly emission at O0/O2.
The changed-driver gate is **526 PASS**, Clippy/fmt green; the ladder suite is
**257 PASS**. A mistaken exploratory `-undef` compile-and-link control produced
an invalid executable and was stopped; it is not among those seven admitted
controls or part of the `-pthread` contract.

Fresh original jq run **`20260927T082205Z`**, compiler SHA-256
`523a698237c9ab72f868fdd69b7d1a1f6e0fd3285694d986b3bf007dee691eaa`,
retains `-pthread` and again passes **69/69** installed units and all original
tests, with no replacement/exclusion. B07 and the hosted Linux L02 profile can
now close; this is not an Annex F, freestanding-runtime or full-roadmap claim.
The unchanged c-testsuite `00204.c` quad-HFA/varargs program separately passes
its complete original expected output with EvoCC O0/O2 and GCC14/Clang19 O0/O2
controls (`ctest204-native.log`); the old 220-program receipt is not yet rerun.
