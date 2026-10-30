# EvoCC AArch64 SIMD inline assembly — focused research brief

## Task and decision required

Act as an AArch64 compiler/backend architect. Produce an implementation-ready
adjudication for integer/polynomial SIMD inline-assembly operands in EvoCC.
The immediate decision is whether and how to extend the closed constraint
vocabulary with `w`, `=w`, `+w`, `=&w`, `+&w` and the corresponding existing
matching-operand forms. This is a narrowly scoped design review, not a new
compiler architecture, a floating-point implementation, or a general benchmark
survey. Research recommendations are not implementation acceptance evidence.

Return a concrete, minimal contract and RED matrix that allow one coherent
implementation across C-HIR, EvoIR, EvoMIR, allocation, emission and validation.
Do not recommend deleting an asm, weakening validation, choosing GPRs for a
SIMD constraint, or accepting an opaque operation as a successful interpreter
execution. Prefer extending the current representations and shared paths.

## Supplied project evidence — distinguish from your own measurements

- Target: `aarch64-unknown-linux-gnu`, LP64, little-endian, GNU C11.
- Checkout baseline: `52f8e3bdeb91c17e8155d482c4690b19e95ab4e6` with substantial
  uncommitted changes. **HEAD alone does not identify the subject.** If given
  repository access, inspect the current files; otherwise label these facts
  as supplied evidence, not independently verified implementation behavior.
- Current tested debug executable SHA-256:
  `09346f2a6072f8b60f03e64b9b4b4a3307b44c4bfe9195ea948653031b13499c`.
- Fixed oracle image: `evo-flight-lab:sealed`, linux/arm64, network disabled;
  digest `2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2`.
  Compilers: GCC-14 14.2.0-19 and Clang-19 19.1.7. Runtime: Debian glibc 2.41.
- Latest changed gate: 2,519 passed, zero failed, four historical ignored;
  Clippy `-D warnings`, fmt and Ruff clean; both layout corpora 128/128 and
  22/22. These results **do not** certify SIMD asm support.
- Ordinary integer vectors, vector ABI, integer/polynomial target intrinsics,
  and AES operations already have implementation and targeted native evidence.
  Do not reopen them wholesale; asm operand binding is a separate missing path.
- EvoIR Section 3.11 still explicitly refuses constraints outside its table.
  The current first error is E3021, not an FP deferral.

Two actual consumers:

1. Linux `arch/arm64/lib/xor-neon.c`, approximately line 188: vector `=w`/`w`
   operands, including the original `eor3` form. Five other previously failing
   kernel NEON objects now assemble; this one remains refused. Inspect the
   original target/extension flags, not just the reduced `eor` example below.
2. Mbed TLS 4.2.0,
   `tf-psa-crypto/drivers/builtin/src/aesce.c:579`:
   `asm volatile ("" : "+w" (r));` where `r` is `uint64x2_t`. This is an
   intentional optimization barrier, not redundant text to remove. The latest
   complete mixed application run installed 379/382 EvoCC objects and passed
   all 148 CTest tests; subsequent targeted fixes are not a new full-build count.

Both following fixtures build and return zero with the two fixed compilers
at O0/O2. EvoCC refuses the relevant constraints. New tests must preserve the
original objects and commands, not replace their headers or source macros.

```c
typedef unsigned long V __attribute__((vector_size(16)));
static V combine(V a, V b) {
    V result;
    __asm__("eor %0.16b, %1.16b, %2.16b"
            : "=w"(result) : "w"(a), "w"(b));
    return result;
}
int main(void) {
    V a = {0x0123456789abcdefUL, 0xfedcba9876543210UL};
    V b = {0xff00ff00ff00ff00UL, 0x00ff00ff00ff00ffUL};
    V c = combine(a, b);
    return c[0] != (a[0] ^ b[0]) || c[1] != (a[1] ^ b[1]);
}
```

```c
typedef unsigned long V __attribute__((vector_size(16)));
int main(void) {
    V r = {0x0123456789abcdefUL, 0xfedcba9876543210UL};
    __asm__ volatile ("" : "+w" (r));
    return r[0] != 0x0123456789abcdefUL || r[1] != 0xfedcba9876543210UL;
}
```

## Existing architecture that must be preserved

- C-HIR evaluates operand expressions and output places once. EvoIR retains
  the template, constraint, symbolic name, type, incoming read/write value,
  clobbers and explicit `asm.goto` successors. Read/write incoming operands do
  not change the template's numeric operand numbering.
- A register output produces a fresh SSA result; a read/write output also
  has an incoming value. Inputs cannot refer to this instruction's results.
  Memory operands retain their separate place/effect semantics.
- Every block uses its own definitions/parameters. `mem<all>` and `world` are
  explicit and linear. `asm.goto` outputs and writeback follow the existing
  per-edge contract, including named targets and fallthrough.
- Asm is opaque and excluded from EqSat. Interpreters report E3021 when they
  reach it; native execution plus independent allocation validation is the
  execution acceptance path. Do not add an asm emulator for this task.
- EvoMIR already has `Simd64`/`Simd128` register classes and an independent
  allocation checker. At the supplied checkpoint, v30/v31 are emission
  scratch, v8-v15 are not normally allocated, and full vector values live
  across calls spill. Verify this locally before proposing changes.
- Source floating arithmetic and SVE/SME are outside this extension. Opaque
  asm text is not inspected to prove that its instructions are integer-only:
  distinguish the admitted C operand types from arbitrary assembler text.
  Preserve the existing `-mgeneral-regs-only` boundary and diagnose conflicts;
  never silently remove effective per-unit flags.

## Questions the report must settle

### 1. Accepted types and closed syntax

Give an explicit accepted/refused table for 64/128-bit integer and polynomial
vectors, nominal polynomial types, scalar integer operands, tiny/wide vectors,
pointer operands and floating/SVE types. Separate required current consumers
from useful but optional extensions; do not silently admit scalar forms merely
because the two current examples use vectors. Unsupported combinations need
an existing appropriate diagnostic, not a late assembler accident.

Specify read, write, read/write, early-clobber and matching operands, including
legal ties to an early-clobber output and independent inputs that may not
overlap. State whether mixed/comma alternatives are outside this bounded
extension; add them only if an original consumer demonstrates a requirement.
Do not make `r` accept SIMD merely by selecting a register from the value type.

### 2. Formatting and physical register aliases

Measure the default `%0`/`%[name]` spelling and relevant AArch64 modifiers
(`%b`, `%h`, `%s`, `%d`, `%q`, and other forms actually needed). Distinguish the
constraint `w` from the existing **modifier** `%w`, which is a different
mechanism. Cover templates with arrangement suffixes such as `%0.16b`.
Specify invalid type/modifier pairs and aliases among v/q/d/s/h/b views.
Clobbers and matching must refer to physical register identity, not independent
names for overlapping views. Do not invent behavior from a string replacement.

### 3. Allocation, preservation and spills

Propose the smallest sound change to the current allocator and independent
checker. Explain where input copies, output copies, reloads and spills occur.
Cover live incoming values that must survive after a destructive/read-write
asm, early-clobber interference, explicit register clobbers, simultaneous
outputs, tied-value coalescing and impossible register demands. Asm operands
must actually be in valid SIMD registers at the instruction.

Check scratch-register conflicts, full 128-bit preservation, copy cycles,
SIMD/GPR bank separation, stack alignment and values live across calls. Use
the pinned AAPCS64 contract, including partial callee-save obligations, rather
than treating a 128-bit vector as a universally preserved 64-bit value.
No new allocator framework is required unless a concrete counterexample shows
the current one cannot be extended soundly.

### 4. Complete IR/control-flow contract

Propose focused amendments to EvoIR Section 3.11/Section 9 and the corresponding
EvoMIR constraint/allocation/emission clauses. Preserve operand types and bank
identity through parsing, printing, lowering, module identity and independent
proof reconstruction. Reuse existing asm result/token/edge semantics.

For SIMD `asm.goto` outputs, explain selected-edge writeback, spills and copies
without cross-block SSA leaks or executing all successors' effects. Confirm
opaque interpretation and EqSat exclusion remain intact. Provide a mutation
test that changes a constraint/class/clobber outside an allowed scalar rewrite;
round-trip equality alone is insufficient evidence of metadata preservation.

### 5. RED and acceptance matrix

Keep the matrix compact and table-driven. It must distinguish acceptance,
generated-register constraints, allocated-code validity and actual results:

- `w`, `=w`, `+w`, early-clobber and matching forms; 64/128-bit lane patterns.
- Empty volatile read/write barrier preserving every bit; side-effecting input
  and output-place expressions evaluated exactly once.
- Several outputs and values live after asm; a real early-clobber hazard.
- Enough register pressure to force spills, a copy cycle, and live-across-call
  data; test full 128-bit contents, not only the low lane.
- Legal modifiers, invalid modifiers, symbolic references and clobber aliases.
- Direct/fallthrough/taken `asm.goto` paths with output values consumed.
- Independent checker rejection of forged allocation, forbidden overlap,
  wrong bank and metadata mutation; reuse existing test infrastructure.
- General-register-only refusal and required architecture-extension flags.
- Subject↔GCC and subject↔Clang calls around the asm-bearing function.
- Original `xor-neon.c` and `aesce.c`, then the unchanged affected application
  and kernel lane. Passing the small matrix does not certify a whole kernel.

For each row give the smallest C or IR input, flags, expected value/diagnostic,
and how it detects the defect. Do not create dozens of redundant tests merely
to give every spelling its own file.

## Evidence protocol and sources

Prefer GCC 14.2 documentation/source, Clang/LLVM 19.1.7 source, and Arm ABI
documents. Pin the version/commit of mutable ABI pages. Useful starting points:

- [GCC 14.2 machine constraints](https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Machine-Constraints.html)
- [GCC 14.2 extended asm](https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Extended-Asm.html)
- [AAPCS64](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst)
- GCC `releases/gcc-14.2.0`: `gcc/config/aarch64/constraints.md` and operand
  formatting; LLVM `llvmorg-19.1.7`: AArch64 constraint/class selection,
  operand printing and Clang target constraint validation.

When the fixed image is available, run minimal controls at O0/O2, preserving
commands, versions, return codes, diagnostics and relevant assembly. Example:

```sh
container run --rm --platform linux/arm64 --network none \
  -v "$PROBE_DIR:/work/probes:ro" evo-flight-lab:sealed \
  sh -eu -c 'gcc-14 -std=gnu11 -O2 /work/probes/case.c -o /tmp/probe; /tmp/probe'
```

Repeat with `clang-19` and O0. Required CPU extensions must be explicit.
If no fixed ARM64 environment is available, distinguish primary-source
conclusions, supplied measurements, host-only exploration and NOT RUN cases.
Do not claim x86, Clang17, emulated interpreter success or compile-only results
as the requested native acceptance. If oracles disagree, isolate the smallest
case and recommend an explicit bounded profile rather than claiming consensus.

## Minimum repository material, if you lack checkout access

Request only current excerpts/files for the following; do not request an entire
workspace, historical evidence tree or Linux clone:

- EvoIR Sections 2.4, 3.11, relevant Section 9 grammar and Appendix D's pending
  SIMD-assembly entry; EvoMIR register classes, ABI and assembly allocation.
- `crates/evocc-evoir/src/assembly.rs`, relevant C-HIR/lowering asm paths.
- Relevant `AsmConstraint`/`AsmValue` definitions in `crates/evocc-mir/src/lib.rs`,
  plus the asm portions of `lower.rs`, `linscan.rs`, `allocverify.rs`, `emit.rs`
  and text parsing/rendering. Include scratch/pool and spill code where needed.
- Existing inline-assembly/allocator tests and the two `arm-neon-assembly*.c`
  fixtures with their small result records.
- Original argv and asm-containing functions from the two affected units.

Unprovided implementation details must remain unverified; do not fabricate
line-accurate patches or assert a local algorithm already has a property.

## Deliverable and stopping boundary

Provide: (1) a one-page decision summary; (2) concise normative amendments;
(3) one shared-path implementation plan with identified risk points;
(4) the RED matrix and small probe sources/results; (5) any genuine remaining
decision, each in at most three sentences. Keep the main report focused,
roughly five pages, with raw probe output separate and small.

State exactly which extension is recommended, what is still refused and why,
and which checks are NOT RUN. Do not predict campaign success counts, promise
full kernel/KUnit closure, implement floating point, redesign EqSat, add crates,
or build another benchmark/runner framework. The goal is an actionable reviewed
contract, followed by implementation and measured closure in the existing tools.
