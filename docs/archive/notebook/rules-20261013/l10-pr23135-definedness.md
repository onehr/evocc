# L10: source definedness of LLVM `pr23135`

This is a decision packet for the lead. It was prepared on 2026-10-13. It is a
proposal. It does not approve a disposition.

## Subject

- Compiler: `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (546fe46a26e8
  2026-10-13)`, SHA-256
  `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04`.
- Specifications: `spec/evoir.md` Revision 2.1.0 at HEAD `546fe46a`.
- Linux references: GCC 14.2.0-19 and Clang 19.1.7 in `evo-nexlab:sealed`,
  image `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`,
  glibc 2.41.
- Darwin reference: host Apple clang 21.0.0 (clang-2100.1.1.101).
- Original: LLVM test-suite `2919e55176a572ee085cb46b291c47b7092c363e`,
  `SingleSource/Regression/C/gcc-c-torture/execute/pr23135.c`, SHA-256
  `55b64d3f85e48564b7275634e277d9c3a86bb9ba9181ebb9aed0e64f64295693` (equal to
  the ladder record of run `20260927T085137Z`).
- Original command (from that record): `clang-19 -DNDEBUG -O3 -DNDEBUG -w
  -Werror=date-time -Wno-implicit-int -Wno-int-conversion
  -Wno-implicit-function-declaration -w -Wno-psabi
  -Wno-error=incompatible-pointer-types -c pr23135.c`. There is no `-fwrapv`.

## 1. Question

Under its unchanged source and flags, does `pr23135.c` execute undefined
behavior? If it does, what evidence is enough for a case-bound
`not-applicable` disposition, given that GCC UBSan reports an overflow and
Clang's instrumentation is silent?

## 2. Evidence

### 2.1 C standard and dialect

- The program uses GNU vectors: `typedef int __attribute__((mode(SI)))
  __attribute__((vector_size(8))) vecint;`, two `int` lanes.
- C11 6.5p5: "If an exceptional condition occurs during the evaluation of an
  expression (that is, if the result is not mathematically defined or not in
  the range of representable values for its type), the behavior is
  undefined."
- EvoIR 10.1.14 "Integer-vector profile" applies this per lane: "other
  integer obligations and wrapping options apply per lane".
- GCC defines vector operators lane by lane. GCC's middle end treats a signed
  lane overflow as undefined, its UBSan instruments it (2.5), and its
  optimizers exploit it (2.2).

### 2.2 GCC 14 and upstream GCC

- The GCC torture copy of the same test, at GCC commit
  `78d4ac73dd391005b895a6148cd9831e28e1208b`, differs from the LLVM copy in one
  line: `/* { dg-options "-Wno-psabi -fwrapv" } */`.
- GCC commit `ca2b8c082c4f16919071c9f8de8db0b33b54c405` (Marc Glisse,
  2020-08-04, PR tree-optimization/95433, "Simplify X * C1 == C2 with undefined
  overflow") made that change with the entry
  "gcc.c-torture/execute/pr23135.c: Add -fwrapv to avoid undefined behavior."
  The new GCC transformation relies on signed overflow being undefined, and it
  broke this test until the test asked for wrap-around.
- The LLVM test-suite copy predates that change, and the LLVM harness does not
  read `dg-options` anyway.

### 2.3 Clang

- `clang/lib/CodeGen/CGExprScalar.cpp` at `llvmorg-19.1.7`, `EmitMul` (lines
  739-786): only a type for which `isSignedIntegerOrEnumerationType()` holds
  gets `nsw` or the `-fsanitize=signed-integer-overflow` check. A vector type
  fails that test and reaches the plain `Builder.CreateMul` (line 785).
- So Clang wraps signed vector lanes and its sanitizer cannot report them.
  Clang's silence is a property of its code generator, not evidence that the
  source is defined.

### 2.4 Target ABI

No ABI question.

### 2.5 Probes and measurements

The exact operation: at line 131, `k = k0 * k1 * k3 * k4 * k5 * k6 * k7;`.
Lane 0 holds 160, 1500, 2, 158, 156, -150 and -151. The partial products
160 x 1500 x 2 x 158 = 75,840,000 fit `int`; the next step 75,840,000 x 156 =
11,831,040,000 exceeds `INT_MAX` (2,147,483,647). Line 133 then expects
`1456467968` and `-1579586240`, which are the wrapped products. The source
therefore depends on wrap-around for its expected output.

Commands (new runs on 2026-10-13; `E=.build/frozen/366fd796/evocc-darwin`):

```sh
# container evo-nexlab:sealed, --network none:
gcc-14|clang-19 -std=gnu11 -O0|-O2 -Wno-psabi -fsanitize=undefined \
  -fno-sanitize-recover=all pr23135.c -o u && ./u; echo $?
gcc-14|clang-19 -std=gnu11 -O0|-O2 -w -Wno-psabi [-fwrapv] pr23135.c -o x && ./x
# EvoCC:
$E --target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu \
   -std=gnu11 -O0|-O2 -Wno-psabi [-fwrapv] --interpret-c-hir|--interpret-evoir|-S pr23135.c
$E -std=gnu11 -O0|-O2 -Wno-psabi [-fwrapv] pr23135.c -o evod && ./evod   # Darwin
```

| Engine | Unchanged flags | With `-fwrapv` (GCC upstream flags) |
| --- | --- | --- |
| GCC 14 + UBSan, O0 | exit 1: `131:25: runtime error: signed integer overflow: 156 * 75840000 cannot be represented in type 'int'` | not run |
| GCC 14 + UBSan, O2 | exit 1: same report, `75840000 * 156` | not run |
| Clang 19 + UBSan, O0/O2 | exit 0, no report | not run |
| GCC 14, Clang 19, Apple clang, O0/O2 | exit 0 | exit 0 |
| EvoCC C-HIR and EvoIR interpreters, O0/O2 | E3004 `signed-integer-overflow` at `131:7` | exit 0 |
| EvoCC native Linux, O0/O2 | exit 133 (SIGTRAP from the checked lane obligation) | exit 0 |
| EvoCC native Darwin, O0/O2 | SIGTRAP | exit 0 |

The retained reduction `tests/fixtures/llvm-vector-overflow-reduction.{c,out}`
already shows the scalar form `75840000 * 156` reported by both GCC and Clang
UBSan at `-O0` and `-O2`, and the widened defined form passing in every
engine.

## 3. Current EvoCC behavior

EvoCC compiles the original. Both interpreters stop with E3004 at the first
overflowing lane; native code traps at the same operation. The LLVM ladder
records `programOutcome: runtime-failure` and `disposition:
compiler-failure`. `tests/fixtures/llvm-failure-reconciliation.out` keeps it as
"OPEN integer-vector overflow | V / L10". This is the correct semantic
result under 10.1.14; it is not a compiler defect.

## 4. Recommended disposition and evidence threshold

Principle applied: GCC 14 defines the dialect on Linux, and GCC treats the
lane overflow as undefined; the repository already diagnoses it. Clang's
wrapping is its code generator's choice, and its sanitizer cannot see it.

Recommended disposition for the B01 ledger (one new row in
`tests/contracts/benchmark-program-dispositions.json` keyed by the source
hash and original arguments above):

> `not-applicable`. The unchanged original executes signed `int` overflow in
> vector lane 0 at line 131 (`75840000 * 156`), and line 133 expects the
> wrapped value. GCC UBSan reports this operation on the unchanged original at
> `-O0` and `-O2`; Clang's instrumentation does not cover vector
> multiplication (`CGExprScalar.cpp` `EmitMul`). GCC upstream added `-fwrapv`
> to its copy of this test "to avoid undefined behavior" (`ca2b8c08`). Both
> EvoCC interpreters report E3004 at the same operation.

Proposed evidence threshold for a source-UB disposition in B01 (this case
meets all six):

1. The first undefined operation of the unchanged original under its recorded
   command, with source location and operand values.
2. The clause that makes it undefined in EvoCC's dialect (C11 and the EvoIR
   per-lane rule).
3. At least one independent instrumented reference report on the unchanged
   original, at the same location and operands.
4. A cited implementation reason for every silent reference. Silence never
   counts as evidence of definedness.
5. Both EvoCC interpreters report the same operation and kind.
6. Upstream acknowledgment, when it exists, as supporting evidence only.

The `-fwrapv` replay is a separate, labeled control that shows EvoCC computes
the expected values when wrap-around is requested. It must not count as the
original's PASS, because the original command does not request `-fwrapv`.

Rejected alternatives:

- **Treat GNU vector lanes as wrapping (Clang's behavior).** It contradicts
  GCC 14, GCC's upstream fix of this very test and EvoIR 10.1.14, and it would
  hide undefined behavior that the repository diagnoses today.
- **Count Clang's clean run as definedness.** Its sanitizer has no check for
  this operation.
- **Rewrite the original command with `-fwrapv`.** AGENTS.md forbids
  replacing a failing original input or command with an easier one.

## 5. Implementation scope

### 5.1 Compiler

None. The current E3004 and native trap are the specified behavior.

### 5.2 Runtime helpers

None.

### 5.3 Tests for the TODO exit

- The existing reduction and its record stay unchanged.
- Add a record with the 2026-10-13 runs above: GCC UBSan output on the
  unchanged original, Clang's silent run with the code-path citation, both
  EvoCC interpreters' E3004 location, and the labeled `-fwrapv` control in
  both interpreters and native code.
- Add the disposition row and run the existing disposition validation (the
  adapter that checks source hash, original arguments and evidence paths).

### 5.4 Original programs to replay

- LLVM SingleSource `pr23135` with its recorded command: expected E3004 or
  native trap, now classified `not-applicable` with evidence, not PASS.
- Feed the reclassification into B01. Do not change any historical numerator.

## 6. Size, dependencies and risks

- Size: **small** (evidence record and one reviewed disposition row).
- Dependencies: none.
- Risks: the threshold accepts one positive sanitizer report when the other
  reference cannot instrument the operation. The risk is low here, because
  the arithmetic is exact, the expected output is the wrapped value and the
  GCC maintainers state the undefinedness themselves.

## 7. Open points

None for this case. The lead should confirm the six-point threshold as the
general B01 rule for future source-UB dispositions.
