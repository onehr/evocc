# Focused research handoff: floating complex and the remaining decision batch

## Task

Act as a compiler architect preparing **implementation-ready decisions**, not a
general C/FP tutorial. Read `todo-closure-batch-20260927.md` and the packet's
`SHA256SUMS` first. The first priority is **L04 floating complex**, which blocks
all 68 remaining musl target objects. Return one consolidated decision annex for
the other pending language questions so implementation does not stop for a new
question after every patch. Engineering diagnosis of kernel/QEMU timing failures
is not a prerequisite for this research and is not delegated to you.

The authoritative supplied documents are **EvoIR 1.9.114 / EvoMIR 0.50**. Use
their exact hashed bodies and included selected implementation files. The
compiler measured here has SHA-256
`e850aacaffba056b0edac81416c604d725f3c77d367e21e5273ffb5d231deab5`.
The older September 21 brief remains historical context, not the current support
table. Strict non-trapping scalar FP, real binary128 and scalar f32/f64 `w`
assembly are now implemented and tested; **do not redesign them from scratch**.
Current musl is 1271/1339 EvoCC objects, 68 complex refusals, no added failures
in two full mixed libc-test trials, and 17 unchanged reference failures. This is
not full-library acceptance. Only small probes are freshly run for this packet.

## Supplied evidence and how to interpret it

- Four new C files cover complex, half/bfloat, floating vectors and CAS. Existing
  strlen, integer-complex, tiny-vector, typeof, ancestor-address and overflow
  reductions are reused. Each selected fixture is hash-bound in the packet.
- `cases.json` gives 37 case definitions; `reference.json` contains 148 actual
  fixed-arm64 GCC14/Clang19 O0/O2 compile/run records. Only 92 executions have
  pass/fail value assertions. The other 56 **record observations**.
- `subject.json` contains 222 current EvoCC commands; diagnostics are not
  sanitized into one generic “FP deferred” result. Half/bfloat signatures expose
  E3101; complex component forms expose E1003 and constructor E2001. These
  first errors must not hide the later interpreter/backend work.
- `crossed.json` proves the tiny-vector reference ABI disagreement: four self
  controls pass and four cross controls fail. No EvoCC interoperability claim.
- `target.json` records versions and presence of six complex arithmetic helpers
  in libgcc. Presence is **not** helper behavior, license or ABI qualification.
- `originals.json` contains historical source-bound musl/LLVM/torture failures,
  original argv where captured, projected argv/omissions and current source hashes.
  Do not claim these originals were re-executed during the small capture.

Fixed target: AArch64 Linux LP64 little-endian; GCC 14.2.0-19, Clang 19.1.7,
glibc 2.41 in the sealed image descriptor recorded in the packet. If you cannot
execute this image, mark proposed experiments NOT RUN. Host x86 results are
exploration only. No full workspace or historical .evidence archive is needed.

## Primary decision: floating complex, all three component formats

Give normative language/IR/ABI rules for `float _Complex`, `double _Complex` and
`long double _Complex` using existing executable f32/f64/f128 components.

1. **Type, representation, expressions.** Define two-component storage, alignment,
   lvalue real/imag access, initializedness at component granularity, construction,
   conjugation, casts among real/integer/complex types, usual conversions, truth,
   equality, compound assignment, constants, initializer forms and varargs. Do not
   confuse complex with vectors or infer a public ABI from storage size alone.
2. **Arithmetic algorithm.** Specify finite multiply/divide, overflow/underflow
   avoidance, division by zero, infinities, qNaNs/sNaNs, signed zero and payload
   selection. Decide what is exact-bit specified and what is library/profile
   choice; provide complete executable algorithms, not only one expected result.
   Distinguish default GNU11 behavior from `-fcx-limited-range`, fast-math or
   Annex G promises. Those options/claims must not silently become enabled.
3. **Observable status and rounding.** Reuse current world/fenv semantics and all
   four admitted modes. Explain the exact sequence of rounded component operations
   and permissible/required exception flags. The fresh zero-divisor probe yields
   both components +infinity, but flag mask **1 for f32 versus 3 for f64/f128**
   under both references. Resolve why before making interpreter/native “match”.
   Do not arbitrarily choose mathematically prettier flags or discard statuses.
4. **Construction is not arithmetic.** GCC documents that `__builtin_complex`
   preserves cases that `real + I * imag` does not, including infinity/NaN/-0.
   Define operand typing/evaluation and exact component transport separately.
   [GCC 14.2 complex extension](https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Complex.html).
5. **One numerical implementation.** Reuse pinned APFloat and existing scalar
   adapter. Should complex operators expand into scalar IR with explicit ordering,
   or need a semantic complex operation? Explain how both interpreters and native
   lowering execute the same contract. No host Rust complex/libm semantic oracle,
   no fake external-call success, no new pointer/memory universe.
6. **Runtime helpers.** Audit the fixed-version source and behavior of
   `__mulsc3/__muldc3/__multc3` and `__divsc3/__divdc3/__divtc3`. State helper
   rounding/status/NaN compatibility, linkage/license, clobbers and freestanding
   limits. Libgcc symbol availability alone cannot close any of those questions.
   Keep helper spellings below semantic EvoIR. Ordinary musl complex libm
   functions remain C library code/calls; do not implement all libm in the compiler.
7. **ABI.** Pin the applicable AAPCS64 release. Cover real/imag register mapping,
   caller/callee directions, stack exhaustion, aggregates containing complex,
   variadic access, callbacks, live 128-bit components and existing shared SIMD
   alias/callee-preservation rules. Require GCC/EvoCC and Clang/EvoCC both ways.
8. **Fidelity.** Name necessary canonical text, effect, local SSA, operand traversal,
   poison, serializer, cache/layout identity and proof-checker changes. All FP
   stays outside P0 EqSat. Include independent one-field mutations, not only
   parser/printer round-trip.

Deliver exact reviewable deltas against the supplied current specs, small C/IR
REDs with bit/status expectations, and a phased implementation plan whose final
gate replays all 68 musl originals and named torture/LLVM originals unchanged.
Separate language core, helpers, full library tests and full benchmark claims.

## Single bounded decision annex

Resolve these together where evidence suffices; do not restart broad research.
If unresolved, name the **one missing fact and minimal experiment**, not another
open-ended request for the full checkout.

| IDs | Required output |
| --- | --- |
| K07 | Nonliteral builtin strlen ICE rule for static const objects/pointers; qualifier, linkage, offsets/NUL/cycles and unevaluated limits. Preserve user-defined ordinary strlen effects. GCC masks 3/7, Clang 7/7, EvoCC 1/1 are supplied facts |
| L03 | Distinct `_Float16`, `__fp16`, `__bf16` format, promotion, scalar/ABI and feature-gating scope. Fresh measured sizeof(+x) is 2/4/2; do not merge all names into one half type |
| L05 | Floating vector lane rounding/status/NaN/masks/conversions, followed by a separately enumerated intrinsic/header domain; do not imply all NEON is admitted |
| L06 | Explicit tiny-vector ABI selection and incompatibility. Clang preference is not a claim to interoperate with GCC's different convention |
| L07 | Integer-complex division algorithm including intermediate overflow, promotions/truncation, zero and `/=`; not a rule hard-coded for 9+7i |
| L08 | `typeof` pointer-to-VLA side-effect/capture rule by expression category; preserve agreed array and lifetime rules |
| L09 | Supported ancestor depth/frame/PAC/optimization assumptions, invalid/reached-only refusal and interpreter ceiling; no fabricated return address |
| L10 | Source-definedness and evidence-policy finding for pr23135. Exact overflowing product plus both scalar UBSan controls and GCC original report are supplied; explain Clang's vector instrumentation gap. Do not rewrite original flags or count source UB as PASS |
| L11a/b | General `_Atomic`, RMW/exchange/CAS width/order/evaluation/expected-writeback rules using current memory model; validate pointer semantics and malformed orders |
| L11c | Generic aggregate atomics: initializedness, padding, pointer-bearing bytes, linearization, helper ABI and non-lock-free boundaries. Recommend a bounded initial scope with explicit remainder |

For atomic details, [GCC 14.2's builtin contract](https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/_005f_005fatomic-Builtins.html)
distinguishes generic helper fallback, failure writeback/order restrictions and
unscaled builtin pointer arithmetic. Reconcile these with EvoCC provenance and
C11 interfaces; do not mechanically reuse the legacy `__sync` spelling/semantics.

## Deliverable and authority rules

Return (1) concise decision table, (2) normative deltas, (3) exact RED source and
commands, (4) shared-path reuse map, (5) acceptance/claim boundaries and (6) finite
remaining questions. Cite primary versioned WG14, Arm, GCC and LLVM sources.
Separate supplied measurements, newly run experiments and reasoning. No new
global dominance SSA, implicit effect channel, numeric pointer recovery or hidden
fenv state. Do not weaken existing UB/initializedness/provenance checks to make a
benchmark green. A recommended rule remains pending review until accepted.

Implementation then proceeds as one sustained repair batch: small tests and
bounded commits, coherent integration gates, unchanged affected-original replay,
then one frozen full campaign. This report must unblock that work—not create a
new benchmark framework or require a full campaign to answer a tiny semantic question.
