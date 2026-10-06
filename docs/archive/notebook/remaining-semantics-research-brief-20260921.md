# EvoCC: targeted research for the remaining numerical and dialect contracts

## Assignment

Act as a compiler architect reviewing implementation-ready specification changes,
not as an author of a general floating-point tutorial. Produce a decision report,
exact proposed normative wording, and small runnable acceptance probes for the
questions below. Explain the selected behavior, alternatives, compatibility costs,
and implementation dependencies. Do not implement the compiler or claim unrun tests
passed. We want to unblock original programs without weakening checks or changing
their sources, flags, seeds, workloads, or output tolerances.

## Current subject and what is already done

### September 27 current batch supersedes the early checkpoint

Use [the current evidence packet](todo-closure-batch-20260927.md) and
[focused complex/decision prompt](remaining-complex-research-prompt-20260927.md)
for new work. Current specs are **EvoIR 1.9.114 / EvoMIR 0.50**; hashes:
`5ece3b8f273a19e39871ed2f7e01761d8abc0e1319e4f12f41c26a82dd8cd5ec` /
`6a0ffe0005b26d6eba71ffc78b609cda727a4da778c3a7e6fb5223332b387c4d`.
Current tested compiler is
`e850aacaffba056b0edac81416c604d725f3c77d367e21e5273ffb5d231deab5`,
at source checkpoint `73d5ba3`. Strict scalar FP, real binary128, and admitted
scalar `w` assembly are implemented, with current musl **1271/1339** assembled,
68 floating-complex refusals, two full mixed-library trials without additional
failures and 17 still-failing reference cases. L01/L02 are closed for their
explicit scopes. The earlier 0/1339 musl and 68/69 jq numbers below are historical.
Sections A/B no longer request a new principles decision. The new packet includes
37 current small cases, actual fixed-arm64 results and all remaining TODO IDs;
its main external research need is L04 complex arithmetic and the consolidated
decision annex. Kernel composition/K13/K14 remain independent unfinished work.

### Early September 27 checkpoint (historical)

The original handoff below is historical. Current clauses are EvoIR **1.9.105**
and EvoMIR **0.44**; their SHA-256 values are respectively
`717e99625a1e3d8219de6d1728da9a168568f935d8053ca40911ff7d59e14ca1` and
`75d481e3d36cd8ad1409d204dcc4d1e3add10c4b7916baa7d6a3f12427b6f9c0`.
Compiler implementation commit `ab50a13` adds bounded private integer-memory
facts through the existing C-HIR inliner; diagnostic commit `a82fde2` changes
no compiler code. The checked debug executable is
`eee2c03bc045d181b2f5aefb4f6047b90a7a7728f6fc90373bb1a9a228d0a9d8`.
Do not substitute this identity for the retained release used by old campaigns.

That repair removes all eight original bitmap compile-time assertion references;
the resulting module and its reference each execute 390,447 checks successfully
on a matching GCC Image. It **does not** broaden source ICE classification,
resolve the nonliteral-strlen question below, or establish a new uniform kernel
composition. The 7,002-object uniform attempt ended INCOMPLETE before boot;
producer-ordering and bitmap fixes require fresh composition verification.
K13/K14 and the final application campaign remain separate, not inherited PASS.
The subsequent September 27 user-supplied architecture report approves A/B:
strict FP through explicit world state plus a `fenv` effect; four rounding modes,
five sticky flags, explicit FMA without implicit contraction; real binary128
through the existing type/ABI path and audited hosted helpers. Preserve the
working scalar baseline and pinned APFloat. Dynamic rounding, complete
non-trapping fenv, fixed-RNE binary128 and strict binary128 remain distinct
implementation/acceptance gates. No traps, Annex F, half, complex or floating
vector support is inferred. Sections C/D's atomic and dialect questions are not
adjudicated by this FP report. See the current [task ledger](../../../TODO.md).

### Original September 21 handoff

The working specifications at handoff are EvoIR **1.9.104** and EvoMIR **0.34**,
on HEAD `9f571cac0e6be6d27915aa82b4c8866ae84eaf9a` with substantial working-tree
changes. HEAD alone does not identify the implementation. Use the supplied current
files, not the older 1.9.102/0.32 research snapshot. Function-section support has
been implemented independently. Fresh run `20260921T065917Z` compiles and links
all 6,765 target-C records, links 745 modules with matching BTF, and boots/shuts
down the new Image successfully; runtime selftests remain separate. Do not
reopen section semantics or make this research a prerequisite for kernel testing.

Handoff fingerprints (SHA-256, checked 2026-09-21):

```text
EvoIR:  bd87360f78b926cef130aaa83f846bd286fac2208ec4ccfa49783168cd1d104a
EvoMIR: 0cd239a4ee34c4f8e4d0b5c270fde2b6aa18dbd3aff4833ece81b94b5cda6c91
EvoCC:  dc6f68eeabee81f9332d25250b31048e9eb36f1b49d5b885b379dc962875763d
```

The contracted sealed image configuration identity is
`sha256:2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2`.
The vendored engine is `rustc_apfloat 0.2.3+llvm-462a31f5a5ab` (registry package
checksum `486c2179b4796f65bfe2ee33679acf0927ac83ecf583ad6c91c3b4570911b9ad`).
These identify the supplied subject; they do not stand in for your own probe
execution or an audit of the EvoCC adapter.
The local container runtime executes `evo-flight-lab:sealed`; verify its
configuration descriptor against the digest before and after a probe. Do not
use this configuration digest as though it were a registry image name.

Fixed target: AArch64 Linux, LP64, little endian, GNU11 unless an original command
explicitly selects another supported dialect. Fixed reference compilers are Debian
GCC 14.2.0-19 and Clang 19.1.7 in the project's sealed arm64 image. The user prefers
Clang/LLVM compatibility where a deliberate dialect choice is required; challenge
that preference with concrete ABI, kernel, standards or interoperability evidence,
not a blanket rule that either oracle always wins.

Already implemented and tested: scalar binary32/binary64 literals, arithmetic,
conversions, both interpreters, exact-bit IR, one shared SIMD/FP register bank,
mixed-compiler scalar/HFA/stack/varargs ABI, integer vectors, lifecycle registration,
TLS, scalar atomic load/store, legacy __sync RMW, GNU inline and restricted raw
source bytes. Do not propose implementing these from scratch. The compiler uses a
pinned rustc_apfloat adapter; an independent TestFloat corpus already checked
661,848 cases across 32 operations. That does not establish strict fenv or binary128.

The scalar baseline is RNE, gradual underflow, source-format evaluation and no
implicit contraction. Representation transport preserves bits. Arithmetic has a
target-specific NaN policy. Native operations admit the specified FP environment
with a check rather than silently resetting FPCR. Incompatible environments are
E3064; strict FENV_ACCESS requests are E3065. FP operations remain outside P0 EqSat.

Current application facts: cJSON, Lua, SQLite, zstd and the selected Mbed TLS
configuration pass their recorded original tests. jq is 68/69 EvoCC objects because
nexttoward(double, long double) requires binary128. Faithfully forwarding musl's
-frounding-math now refuses its 1,339 objects: one missing semantic capability,
not 1,339 independent bugs. Torture's 39 FP deferrals comprise 18 binary128,
15 floating complex and six floating vector originals. These are not PASS.

## A. Strict floating environment: highest research priority

Specify a finite first strict-fenv milestone that can genuinely consume musl's
original flags. Distinguish -frounding-math, FENV_ACCESS ON, exception flags,
trapping, signaling NaNs, errno, contraction and Annex F; accepting one must not
silently promise all the others. Audit actual musl uses before choosing the scope.

Resolve all of the following:

1. What state is observable: rounding direction, accrued exceptions, trap enables,
   FPCR/FPSR fields, subnormal/default-NaN modes? Which states are supported or
   explicitly rejected, and why? How do ordinary and strict compiled functions
   interoperate, including constructors, callbacks, TLS and reference libraries?
2. Reuse explicit world state if sufficient. Define strict operations, state
   snapshots, branch/call transport, thread ownership, instruction scheduling and
   exception accumulation. No interpreter-global mutable fenv or implicit SSA edges.
3. Define literal translation and static initializers separately from executed
   expressions. Specify which constant folding, common-subexpression elimination,
   speculation and dead-code removal remain legal with dynamic rounding/flags.
4. Give exact numerical and NaN rules for arithmetic, comparisons and conversions,
   including FP-to-int UB versus IEEE invalid/inexact status. Audit the existing
   software engine's status/rounding API against native AArch64.
5. Define native FPCR/FPSR access, save/restore responsibilities, external calls,
   fenv library/builtin boundaries and unsupported-mode diagnostics. Never reset
   caller state merely to make the baseline pass.
6. Provide a flag/pragma matrix and compilation/proof/cache identity changes.
   Explain whether a staged strict-rounding-only release can honestly build musl,
   or which additional pieces its unchanged sources require.

Required probes: all admitted rounding modes, half-ULP and underflow boundaries,
sticky flags, clear/test/hold/update environment, constant-versus-runtime cases,
NaN/sNaN, signed zero, calls/callbacks, two threads, and world/metadata tampering.
Show exact expected bits/flags where defined; identify implementation choices.

## B. Binary128 / long double: next practical coverage milestone

Specify real binary128, never a binary64 substitute. Cover source literals and
suffixes, object representation/alignment, arithmetic/comparisons, conversions
to/from binary32/64 and signed/unsigned integers through 128 bits, NaNs, poison,
constant evaluation, both interpreters, function signatures and varargs.

Choose a practical native strategy: existing runtime helpers, compiler-supplied
helpers or another justified route. Audit the exact target toolchain/runtime for
helper names, ABI, semantics, linkage, availability and licensing; do not guess
that every __*tf* helper exists or has the required fenv behavior. Distinguish
hosted support from -ffreestanding/-mgeneral-regs-only and kernel builds.

Give a single AAPCS64 classification integrated with existing scalar FP/HFA/HVA
machinery. Include mixed arguments, register exhaustion, HFA, callbacks, stack
alignment, va_arg(long double), return values, spills and caller/callee preservation.
Require GCC/EvoCC and Clang/EvoCC calls in both directions. Compare bytes only where
the format's padding/NaN rules permit it. Audit nexttoward, formatting/conversion
and libm interfaces without proposing to reimplement all libm in the compiler.

Explain interactions with strict fenv and a staged dependency order. Identify
whether binary16/bfloat16, floating complex and floating vectors are independent
follow-on projects; do not claim binary128 closes those cases.

## C. Resolve the remaining measured non-floating dialect disagreements

For each item distinguish C requirements, GNU extensions, ABI rules, implementation
choices, source UB, unsupported execution and reference-tool failure. Recommend a
specific EvoCC contract or a precisely bounded refusal, with compatibility costs.

* Sub-8-byte integer-vector external ABI: existing four-byte vector self-controls
  pass, but GCC uses GPRs and Clang SIMD registers and crossed controls fail.
  Determine applicable AAPCS rules and whether a documented Clang-compatible mode
  is appropriate. No private scalarized ABI masquerading as interoperability.
* Integer-complex division: retained (9+7i)/(2+1i) probe gives GCC 4+3i and Clang
  5+1i at O0/O2. Determine the algorithms, rounding/overflow/zero-divisor rules,
  promotions and compound-assignment contract, not just that one expected value.
* Pointer-to-VLA typeof expression operands: GCC leaves the side-effect counter
  zero while Clang increments it; array operands agree. Define evaluation and
  capture by expression category without reopening already working VLA allocation.
* Nonzero-depth return_address: with frame pointers and sibling calls disabled,
  GCC gives zero while Clang reads ancestor LR. Separate language guarantee,
  target unwinding/PAC, optimization assumptions and reached-only unsupported
  behavior. No invented valid address or interpreter/native equivalence claim.
* LLVM pr23135: GCC UBSan reports vector signed overflow; Clang instrumentation is
  silent. Establish source definedness under the exact original flags. Silence is
  not a proof of defined behavior, and one tool report is not automatically a
  two-oracle disposition. Recommend the evidence threshold for this specific case.
* Nonliteral `__builtin_strlen` constant-expression admission (September 23
  addendum): all allocation/byte-query fixes now pass the original FORTIFY suite
  except its static const array and static const pointer ICE expectations
  (`lib/tests/fortify_kunit.c:95,101`). Current 10.1.18 explicitly makes every
  nonliteral operand an external call. The 19-line
  `tests/fixtures/fortify-nonliteral-strlen-ice.c` measures classification masks:
  GCC14 O0=3, O2=7; Clang19 O0/O2=7; EvoCC both interpreters/native=1. All compile.
  Bits 0/1/2 denote literal/static-const-array/static-const-pointer; mutable
  pointer, automatic array and mutable array controls are not ICE. Decide a
  bounded source-classification rule, independently of later optimization;
  preserve visible user-defined `strlen` effects on the ordinary call path.
  Address object linkage, qualifier/initializer requirements, offsets, embedded
  NUL, cycles, invalid pointers and unevaluated contexts. This is not authority
  to fold arbitrary object reads or delete the existing call-effect regression.

## D. Bounded additional atomic contracts

The inventory is now complete; do not repeat it as though it were implementation.
`tests/fixtures/atomic-scope-boundaries.{c,out}` separates TinyCC's additional
bounds-checking/C11-atomic configuration and zlib's optional dynamic CRC table
from their current measured lanes. The three cases have 12 fixed-oracle controls
and nine current CLI refusals. No additional target-object blocker was found in
the current lanes; none of this establishes support for the requested interfaces.
Keep TODO L11a/L11b/L11c distinct from supported scalar load/store and legacy
`__sync`, and from a general-purpose concurrency model checker.

Provide finite normative deltas and small native/interpreter tests for:

* General `_Atomic` objects: initialization, implicit access/update order,
  qualifiers, representation/alignment, lock-free widths and library fallback.
  State exactly which ordinary C operators become atomic operations; do not
  drop the qualifier after layout or treat every update as separate load/store.
* Additional `__atomic` RMW/exchange/CAS: success/failure orders, invalid constant
  orders, dynamic-order evaluation, weak/strong semantics, expected-value writes,
  signed arithmetic and pointer scaling/provenance. Explain where GNU builtins
  and C11 generic interfaces differ instead of mechanically aliasing their names.
* Generic aggregate accesses: admitted sizes/alignment, representation/padding,
  initializedness and pointer provenance, helper ABI and runtime dependency.
  Specify comparison behavior and refusal boundaries without making uninitialized
  padding zero or fabricating provenance from numeric bytes.

For each, preserve explicit order/effect metadata through text, proof rebuilding
and emission; give the shared-memory claim boundary of both interpreters.
Require crossed-compiler native controls plus publication/SC/CAS cases appropriate
to the admitted scope. A single-thread value test is not concurrent correctness.

## Evidence and deliverables

Use primary C/WG14, pinned Arm ABI/architecture, GCC-14 and LLVM/Clang-19 sources.
Separate externally documented facts, supplied project measurements, your actual
measurements, inferences and NOT RUN. A host x86_64 result is not fixed ARM64 evidence.
If the sealed environment or checkout is unavailable, provide commands and expected
observations, not fabricated outcomes. Oracle agreement alone does not prove no UB.

Deliver:

1. An executive decision table: rule, recommended choice, rejected alternatives,
   affected original programs, dependencies and exact claim ceiling.
2. Normative patch text against the supplied current EvoIR/EvoMIR: syntax, typing,
   effects, object representation, ABI, diagnostics, proof fidelity and identity.
   Preserve local SSA, explicit linear tokens and the existing EqSat allowlist.
3. Small runnable C/IR probes and exact commands, with independently checkable
   expected bits/flags/ELF ABI observations. Include negative metadata mutations;
   parser/renderer round-trip alone is insufficient.
4. A dependency-ordered RED -> shared implementation -> tailored tests -> unchanged
   original-program replay plan, reusing tools/bench.py and existing fixtures.
5. A finite list of unresolved decisions, if any. Do not reopen settled scalar FP,
   GNU inline, source-byte, lifecycle, integer-vector or TLS principles.

Minimal input bundle only: current spec/evoir.md, spec/evomir.md,
floating-point research brief's current checkpoint, benchmark evaluation and
applicability excerpts, relevant fixture .c/.out files, Cargo.lock and the FP
adapter, plus exact failing original sources/argv for the questions above.
Ask for specific missing snippets. Do not request a full workspace/kernel clone
or historical .evidence archive. Function-section repair, LLVM performance
profiling and full kernel runtime testing proceed independently of this report.
