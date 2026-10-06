# Research brief: executable floating point in EvoCC

The current follow-up assignment is the
[2026-09-21 remaining-semantics brief](remaining-semantics-research-brief-20260921.md),
against EvoIR 1.9.104 / EvoMIR 0.34. The checkpoint and original assignment
below retain their historical subject and results; they are not a request to
reimplement the completed scalar foundation.

## Report received and implementation checkpoint — 2026-09-20

The supplied final research report answers this brief, not the separate
`simd-inline-asm-research-brief-20260920.md`. It is sufficient to start the
finite scalar foundation; its NOT RUN numerical/ABI matrix is not acceptance.
At this checkpoint, local authority was EvoIR **1.9.103** plus Appendix D and EvoMIR **0.33**
plus addenda, on `9f571ca` with the existing shared dirty tree. No clone or
historical evidence archive was created.

Corrections to the report's snapshot: reviewed GNU inline and restricted raw
source bytes are implemented and have targeted/original-input evidence in the
benchmark notebook. Q2 lifecycle, Q3 integer-vector and Q6 TLS targeted closure
is also already recorded; that is not complete application or expanded-kernel
acceptance. Do not reopen their principles or count them as newly fixed here.

| Order | Concrete work | State / acceptance boundary |
|---|---|---|
| 1 | FP00/02/05: source-format literals, shared finite constant arithmetic and exact integer conversion | Two declared REDs reproduced silent binary32 double-rounding and hexadecimal refusal, with zero build errors. Three reduced C programs now pass both interpreters, six native executions and twelve fixed-source oracle executions; 47 language regressions pass. |
| 2 | FP06: qualify the software engine | All 3,072 target arithmetic cases agree with fixed GCC/Clang and subject native execution. Independent Berkeley TestFloat level 1 adds 661,848 exact cases across 32 operations, including 1,898 invalid integer conversions correctly rejected; the freshly built SoftFloat reference also passes slowfloat checks. Strict flags and IEEE completeness are not claimed. See `tests/fixtures/floating-testfloat.out`. |
| 3 | FP02/03: native environment and NaN contract | Approved fixed FPCR guard is implemented without resetting caller state. Four incompatible environment settings each trap under both reference-linked native drivers; exact representation transport does not reset the environment. Strict fenv remains refused. |
| 4 | FP07/08: runtime IR, both interpreters, shared physical bank, ABI | Implemented f32/f64 through both interpreters and native. Seven self/reference/mixed ABI combinations pass scalar, HFA, callback, ninth/stack and variadic cases. A 24-double + 8-float + 4-vector live-across-clobber case passes both target drivers. Further regression/gate and application closure remain required. |
| 5 | FP09/10: builtins, normal library calls, flags and identity | Classification, fabs/copysign and infinity builtins use exact representation paths and single evaluation; 8–128-bit integer-conversion negatives report E3063, poison remains E3001. Strict/fast/contraction requests fail closed; layout identity has advanced. External libc/libm execution still requires original-program tests. |
| 6 | FP11: original applications, then serial campaign | Only after the reduced numerical/ABI cases pass, run cJSON, jq/Lua, SQLite and the existing roadmap lanes through tools/bench.py. No old FP disposition becomes PASS merely because this report was received. |

Original cJSON now passes 2/2 installed objects and its public test plus 54
Unity assertions; Lua passes 34/34 and the unchanged matching `all.lua`.
jq remains 68/69 because `nexttoward(double, long double)` genuinely requires
the deferred binary128 ABI. SQLite's 86/86 compilation exposed nine crashes:
the reduced switch-default environment defect is fixed and all nine original
files pass a 1,207-assertion replay. Fresh child `20260919T204939Z` subsequently
passes the whole public veryquick configuration: all 86 objects are EvoCC-built,
1,238 files and 329,803 assertions pass, with no exclusions or make replacements.
The SQLite adapter now checks upstream job state/errors instead of trusting
the runner's zero exit. Detailed identities remain in the benchmark notebook.

The three locked compiler dependencies are vendored (about 1.1 MiB), preserving
their licenses/checksums; empty-Cargo-home offline resolution now works. The
completed workspace gate uses that empty home, not the developer registry cache,
and reuses the one target directory rather than cloning the workspace. It passes
2,571 tests (zero failures, four historical ignored), all-target Clippy with
warnings denied, and fmt. Independent temporary-client builds explicitly load
the same vendor configuration; their separate 13-test offline replay also passes.
Fresh layout checks are 128/128 and 22/22. This gate is not an application or
expanded-kernel acceptance result.

The next complete six-lane run additionally closes Mbed TLS **382/382** with
selftest/148 CTest and zstd **44/44** with its original tests. It also exposes
an HFA stack-transport alignment defect: the object alignment is not always
the AAPCS copy alignment. The shared alignment metadata and lowering repair
passes **26 native/mixed controls** and unchanged torture `pr92904`; independent
proof reconstruction rejects alignment mutation. Two harness defects are also
repaired: E0's missing static libm, and PolyBench's reference FMA contraction
under a subject profile requiring separate multiply/add. All eight formerly
differing kernels match both noncontracting reference dumps exactly; a new
complete run remains required. Details and failed attempts stay in the
evaluation notebook. None of this implements deferred binary128/complex/vector.

The initial backend audit found only `Simd64`/`Simd128`; scalar FP now adds
`Simd32` within that same physical bank, retaining reserved v30/v31 and call
spilling. The report's example `fadd.f32` with one f64 operand is not a
valid proposed typing test; negative tests must continue to reject that mix.

The engine's [upstream source and license](https://github.com/rust-lang/rustc_apfloat)
were inspected, including its LLVM exception for GPLv2 combinations. The
downloaded crate checksum is
`486c2179b4796f65bfe2ee33679acf0927ac83ecf583ad6c91c3b4570911b9ad`;
the lock also pins `bitflags` and `smallvec`. It is a compiler-side dependency,
not a new generated-program runtime or LLVM backend dependency.

`floating-engine-nan.rs` executes the pinned library's unwrapped add/mul/div
on `7ff8000000000011` and `fff0000000000022`: it returns the first qNaN
`7ff8000000000011`, whereas the ordered ARM64 instructions in all four fixed
controls select and quiet the second operand to `fff8000000000022`.
This is a target-contract mismatch, not a claim that the library violates
IEEE's general NaN rules. The admitted finite API rejects both inputs; the
runtime adapter now implements the pinned target's selection policy; the raw
engine mismatch remains as a regression explaining why the adapter is needed.

One additional exploratory source remains unimplemented: a file-scope array
bound `(16777216.0f + 1.0f == 16777216.0f) ? 1 : -1` is folded as a GNU
extension by both controls but currently reports E2203. The finite-arithmetic
regression tests a normal arithmetic static initializer instead; it does not
claim that this non-ICE array-bound extension was fixed.

Native FP environment details and the separate integer-SIMD `w` admission were
approved on 2026-09-20: first-release native execution checks the fixed RNE,
gradual-underflow, non-default-NaN environment instead of resetting the caller's
FPCR; incompatible environments fail explicitly, not as source UB, and dynamic
strict-fenv remains deferred. Full integer/polynomial SIMD `w` binding (including
read/write, early-clobber, matching, aliases, spills and native tests) is approved
separately; neither approval is implementation acceptance. No further principles
review is pending for these two boundaries.
Final changed gate: **2,544 passed, 0 failed, 4 historical ignored** across
308 groups/all 17 crates; Clippy `-D warnings` and fmt pass. Fresh layout
corpora are 128/128 and 22/22 with zero differences. The gate has exited;
these results close this constant-repair batch, not FP02–FP11 as a whole.

## Assignment and required result

Design a practical, implementation-ready floating-point extension for EvoCC,
an existing C compiler with its own C-HIR, EvoIR, verified rewriting boundary,
EvoMIR, and AArch64 backend. Do not propose replacing those layers with LLVM.
We want real applications to compile, link and execute correctly, not merely
accept `float` declarations or print floating instructions. Recommend the
smallest coherent first release and the subsequent steps to broader C support.
Return a decision report, proposed specification deltas, an ordered engineering
plan and runnable RED probes. Do not implement code or claim unrun probes pass.

Product motivation includes games, jq, Lua, SQLite, cJSON, compression and the
existing compiler benchmarks. In particular, verify a **specific Doom port**:
classic fixed-point gameplay, a port's renderer, sound, platform dependencies
and floating-point requirements are different things. Do not assume every Doom
port needs FP, or that FP alone makes one work. Recommend a pinned, legally
reproducible port/data/test combination and a deterministic demo or framebuffer
test; interactive graphics/audio is a separate acceptance claim.

## Repository baseline and evidence boundary

Prepared 2026-09-15 against HEAD
`9f571cac0e6be6d27915aa82b4c8866ae84eaf9a`, **with a substantial uncommitted
implementation tree**. HEAD alone does not identify the implementation below.
Read the supplied files/diff; if no checkout is available, explicitly distinguish
provided repository observations from independently verified ones.

- `spec/evoir.md`: revision **1.9.101**, including Appendix D.
- `spec/evomir.md`: revision **0.32**, including later corrections to its
  earlier scalar-only sections. Identify contradictory old paragraphs rather
  than treating their presence as an implementation requirement.
- Target of this work: little-endian LP64 `aarch64-unknown-linux-gnu`, GNU11.
  Darwin is an existing separate backend profile: preserve it; do not silently
  impose Linux varargs, `long double`, or loader behavior on it.
- Fixed comparison environment: `evo-flight-lab:sealed`, linux/arm64,
  GCC Debian 14.2.0-19, Clang Debian 19.1.7, glibc 2.41 (versions from the
  retained project probe record; recheck before new measurements).
- `output/compiler-review/20260909/decisions.md`: review decisions, followed
  by newer specification rulings where they supersede those decisions.
- `docs/notebook/benchmark-evaluation-20260915.md`: current results and their
  limitations. Mixed-reference builds and targeted fixes are not full passes.

Useful implementation entry points:

| Layer | Files to inspect |
|---|---|
| C types and layout | `crates/evocc-types/src/lib.rs` |
| Parsing and type conversions | `crates/evocc-parser/src/lib.rs`, `src/vector.rs`, `src/complex.rs` |
| C-HIR and constant evaluation | `crates/evocc-chir/src/lib.rs`, `constant.rs`, `constant/floating.rs`, `interp.rs`, `vector.rs`, `complex.rs` |
| Front-end lowering | `crates/evocc-lower/src/lib.rs`, `vector.rs`, `complex.rs` |
| IR, text and execution | `crates/evocc-evoir/src/lib.rs`, `text.rs`, `text_parse.rs`, `vector.rs`, `variadic.rs` |
| Representation/initializedness | `crates/evocc-memory/src/lib.rs` |
| Machine representation and ABI | `crates/evocc-mir/src/{lib,lower,validate,interp,linscan,allocverify,emit,native}.rs` |
| Optimization and checking | `crates/evocc-eqopt`, `crates/evocc-proof`, `crates/evocc-proofcheck` |
| Driver and measurements | `crates/evocc/src/main.rs`, `tools/bench.py`, `tools/ladder/` |

Current building blocks are **not full FP support**:

- EvoIR's type universe has `f32` and `f64`; C types also describe F16/F128.
  A type spelling is not an executable operation contract.
- Floating objects can already be laid out, copied as representations, and
  initialized with some constant binary32/binary64 bytes. A bounded constant
  folder uses host Rust floating arithmetic. Audit its rounding, parsing and
  host-dependence before reusing it as the general semantic engine.
  An original representation-only boundary is LLVM
  `SingleSource/Regression/C/gcc-c-torture/execute/ieee/builtin-nan-1.c`
  (SHA-256 `bd80d9b0bc24317fec2b9e462d85f3d8c2444e650d5ec0d9ef8cdcf8d0ef82d8`):
  two static `__builtin_nan` initializers with `0x1`/`0X1` payload spellings,
  followed only by `memcmp`. A subsequent bounded exact-payload implementation
  now passes that original under both native assemblers; its matrix and retained
  conflicts are in `tests/fixtures/nan-constant-representation.out`. Audit and
  extend that representation policy for general floating semantics; this program
  must not be excused as requiring runtime floating arithmetic.
- Real integer vectors, SIMD64/SIMD128 classes, vector/HVA ABI and SIMD vararg
  save areas exist. They do not authorize floating operations or HFA passing.
- Mixed nonhomogeneous records with unused floating bytes can already pass
  through the composite ABI. Do not regress this by confusing representation
  transport with executing an FP value.
- C-HIR, EvoIR and MIR interpreters exist. Explicit initializedness, object
  provenance, local block-argument SSA, memory snapshots and `world` ordering
  are architectural contracts, not optional checking conveniences.
- Runtime FP refusals include `E4001` and generic front-end/lowering failures;
  audit the registry instead of assigning a second meaning to existing codes.
- The kernel's `-mgeneral-regs-only` must remain enforced. Hosted FP support
  neither authorizes kernel SIMD usage nor changes the frozen kernel flags.

## Questions the report must decide

### 1. Language profile and numerical contract

Give a table for `float`, `double`, `long double`, `_FloatN`, `__fp16`, floating
complex and floating vectors: storage, executable operations, ABI, first-release
status, diagnostic outside scope. Decide whether a binary32/binary64 first
release is useful without executable binary128; quantify actual source needs.
Never implement target `long double` by silently using host `double`.

Specify literals (decimal and hexadecimal, suffixes, correctly rounded parsing),
usual arithmetic conversions, source evaluation width, `FLT_EVAL_METHOD`, casts,
truth/conditional operations, compound assignments, ++/-- and static initializers.
Include integer conversions through 128 bits, rounding and range boundaries.
Distinguish numerical conversion from representation bitcast/copy.

Define signed zero, subnormals, infinity, quiet/signaling NaNs, NaN payload policy,
ordered/unordered comparisons and invalid/out-of-range conversions. Distinguish
an IEEE result/exception from C undefined behavior and from the existing poison
model. Do not turn every nonfinite result into a compiler trap or zero.

Audit what plain GNU11, IEC 60559 support, `FENV_ACCESS`, dynamic rounding,
`math_errhandling`, errno, traps and `#pragma STDC FP_CONTRACT` actually require.
Recommend explicit baseline and strict-environment modes, or explain why one
mode is sufficient. Decide how unsupported requests fail closed.

### 2. EvoIR semantics, effects and canonical text

Propose an exact minimal operation set and signatures: constants, arithmetic,
negation, comparisons, conversions, loads/stores, selects/joins, calls/returns,
and any essential classification or fused operations. Separate required
instructions from operations best expressed as ordinary library calls.

Specify result types, full bit-pattern text syntax, validator constraints,
poison propagation, exception behavior and interpreter behavior for each form.
How do rounding state and exception flags appear in functional snapshots and
explicit token flow? Can default-mode operations be pure, and under exactly
what observable-state assumptions? Do not hide mutable FP state outside the
execution model or introduce implicit cross-block operands.

Update Section 1/3/4/6/9, C-HIR lowering contracts, Appendix A and diagnostic
registry together. Cover malformed text, round-trip, operand traversal,
renumbering, serialization identity and persistent-cache invalidation.

### 3. Deterministic interpretation and constant evaluation

Compare constrained host arithmetic, LLVM APFloat-style software arithmetic,
Berkeley SoftFloat and realistic Rust libraries. Research exact version,
licensing, integration size, supported formats/rounding/flags and maintenance.
Recommend one approach with a small fallback, not a new numeric framework.

Avoid a false differential test in which compiler folding and both interpreters
all use the same erroneous host conversion. Specify independent reference
vectors, bit-exact observations, NaN allowances and cross-host tests. Explain
where native host FP is safe and where it is not. Keep compile-time folding
consistent with the chosen source/environment contract and unevaluated operands.

### 4. AArch64 machine lowering, register allocation and ABI

Map arithmetic, comparisons (especially unordered branches), conversions,
constant materialization and memory accesses to baseline AArch64 instructions.
Specify FP/SIMD register overlap, scalar versus vector widths, spill/reload,
callee-save portions, scratch registers and mixed integer/FP call clobbers.
Reuse existing SIMD allocation where correct; show exactly which invariants
need changing. Include live-across-call values, copy cycles and flag liveness.

Provide AAPCS64 classification tables and mixed GCC/Clang caller/callee probes
for scalar arguments/results, ninth arguments, mixed register exhaustion,
stack alignment, HFAs of 1..4 members, nested records/arrays/unions, callbacks,
variadic float-to-double promotion and `va_arg(double/long double/HFA)`.
Integrate the existing vector/HVA and byval/sret rules, rather than applying a
second independent stack cursor. A compiler calling itself is insufficient.

Explain FPCR/FPSR initialization, preservation and environment access; don't
assume process defaults satisfy every requested mode. Keep Linux/Darwin ABI
differences and `-mgeneral-regs-only` explicit. Assess binary128 and complex
helper calls, their ABI and libgcc/compiler-rt linkage rather than inventing
hardware support or an unversioned runtime dependency.

### 5. Builtins, libc/libm and optimization boundary

Prioritize actual benchmark blockers: `__builtin_isnan`, `isinf`, `isfinite`,
`signbit`, `nan/nanf`, `inf/huge_val`, `fabs`, `copysign`, `sqrt`, `fma`, rounding
and conversion functions. Inventory suffixes and operand evaluation rules.
For each choose front-end resolution, native instruction or ordinary library
call, with correct prototype, errno/environment semantics and freestanding
behavior. `printf/scanf` varargs and `strtod` consumers are not optional ABI
details. Integer-only bit classification must not require accidental FP use.

Do not default to fast math. Specify handling of `-ffast-math`, contraction,
reassociation, reciprocal approximations, signed zeros and finite-math claims;
consume supported flags or explicitly refuse them. Distinguish instruction
selection from legality of replacing multiply-plus-add with FMA.

Keep FP outside the current EqSat allowlist initially unless a bounded rule
set is independently justified. Nevertheless checker/reassembly must preserve
every new operation, environment/fast-math field and effect; a one-field forged
change must fail. No validator weakening or treating unknown FP as an integer.

### 6. Applications and staged acceptance

Use the existing `tools/bench.py`; do not design another benchmark framework.
Propose dependency-ordered stages, with exact entry/exit criteria:

1. scalar representation/semantics + interpreter + RED numerical matrix;
2. native scalar execution and mixed ABI/variadic/HFA closure;
3. builtin/libm integration and original cJSON/jq/Lua/SQLite programs;
4. broader formats, environment and FP vector/complex capability as justified;
5. pinned Doom-port run and fresh torture/LLVM/application campaigns.

Adjust that order if dependencies demand it, and explain why. No dates or
predicted passing counts without measurement. Identify non-FP blockers for
each application separately (TLS, loader, intrinsics, encoding, external calls,
platform APIs, etc.). Integer SIMD and polynomial/NEON types are distinct from
scalar FP; account for shared machinery without conflating the populations.

The RED matrix must include bit-exact boundary values and subnormals, signed
zeros, NaNs/infinities, conversion limits, integer/FP mixes, conditional and
short-circuit side effects, uninitialized storage, recursion/callbacks,
ABI spills/stack arguments, HFA/varargs, rounding modes, flags and contraction.
State ULP/NaN allowances **before** observing results, never relax them after a
failure. Check native code and actual values, not only parser/IR acceptance.

For each stage: small probes -> shared implementation -> tailored tests ->
unchanged affected programs -> changed gate. Run the expensive campaigns only
after their prerequisite reductions are green. Preserve programs, seeds,
options, expected outputs and denominators. Oracle-unusable remains distinct
from PASS; disagreement gets a documented profile decision, not edited output.

## Evidence, sources and deliverable format

Use primary, version-specific sources: WG14/C11 and relevant defect reports,
IEEE 754 where publicly accessible, GCC 14.2, Clang/LLVM 19.1.7 source/manuals,
Arm AAPCS64/AAELF64 and instruction documentation, libc/libm and runtime-helper
source, and the selected applications' original build/test documentation.
Later standards are context, not silently imported GNU11 requirements.

Run small probes in the fixed target image if available:

```sh
container run --rm --platform linux/arm64 --network none \
  -v "$PROBE_DIR:/work/probes" evo-flight-lab:sealed \
  sh -lc 'cd /work/probes; gcc-14 --version; clang-19 --version; ...'
```

The ellipsis is a placeholder for explicit probe commands, not a runnable
finished script. Record flags, versions, stdout bits, diagnostics and exit
codes. Host-only observations must be labeled as such; absence of the sealed
environment means NOT RUN, not inferred consensus. Do not upload full Linux
builds, clone the workspace, or retain large repeated binary/log collections:
small sources, commands and result tables suffice for this research.

Return:

1. <=2-page executive decision: what, why, alternatives/risks, first-release
   scope, remaining decisions and application claim ceiling;
2. repository gap map with exact files/symbols where inspected;
3. review-ready normative deltas with cross-section consistency checklist;
4. operation/ABI/environment tables and ordered implementation packages;
5. runnable minimal probes, RED matrix and measured/NOT RUN result inventory;
6. open questions that genuinely change implementation, each with a recommended
   answer and concrete tradeoff. Separate missing facts from design choices.

The report is advisory until reviewed. Existing compiler failures do not become
FP deferrals merely because FP work is being planned, and adding FP does not
by itself establish full C conformance, complete benchmark PASS or kernel tests.
