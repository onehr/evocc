# L05: Floating vector execution and intrinsics

This is a decision packet for the lead. It was prepared on 2026-10-13. It is a
proposal, not an approved rule. It changes no specification text. Every
measured result below was run for this packet unless a line names an older
record. Section 7 lists the points that the evidence does not settle.

## Subject

| Item | Identity |
| --- | --- |
| Compiler | `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (366fd796613e 2026-10-13)`, SHA-256 `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04` |
| Specifications | `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md` Revision 1.0.1 at HEAD `546fe46a` |
| Linux references | GCC 14.2.0-19 (`gcc-14`) and Clang 19.1.7 (`clang-19`) in `evo-nexlab:sealed`, image `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`, glibc 2.41, linux/arm64, no network |
| Darwin reference | Host Apple clang 21.0.0 (clang-2100.1.1.101), arm64-apple-darwin25.6.0 |
| Linux target | `--target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu`, GNU11 |

Probe sources are in `tests/fixtures/rules-20261013/`:
`l05-lane-identity.c`, `l05-convert.c`, `l05-shuffle.c`, `l05-broadcast.c`,
`l05-operators.c`, `l05-contract.c`, `l05-flags.c`, `l05-storage-boundary.c`,
`l05-general-regs.c`, `l05-abi-crossed.c`, `l05-hva-mixed-lanes.c` and
`l05-neon-subset.c`. Each file states its cases and its exit-status meaning.
The existing `tests/fixtures/floating-vector-boundaries.c` was also rerun.

## 1. Question

1. What does one operation on a GNU `vector_size` vector of `float` or
   `double` lanes compute: arithmetic, masks, conversions, shuffles, scalar
   broadcast, NaN results, flags, rounding, contraction, initializedness?
2. Which operators and casts are admitted?
3. How do these vectors cross a call boundary on each target, including HVAs,
   wide vectors, vectors below 8 bytes, varargs and `-mgeneral-regs-only`?
4. Which floating NEON intrinsics execute first, and with what semantics?
5. What stays refused, and with which diagnostic?

## 2. Evidence

### 2.1 C standard, IEEE 754 and extension documents

- C11, C17 and C23 define no vector types. GNU `vector_size` is an extension.
  The standard still governs each lane operation and each conversion.
- C11 6.3.1.4p1: converting a floating value outside the integer range is
  undefined. `__builtin_convertvector` is defined as that C cast per lane.
- C11 6.5p8 and 7.12.2: contraction is allowed only where `FP_CONTRACT` is on.
  The default state is implementation-defined.
- C11 6.5.3.3p1 requires a scalar operand for `!`. C11 6.5.2.4p1 requires a
  real or pointer operand for `++`. Both exclude vectors in C.
- C11 7.12.14p1: relational operators may raise invalid for NaN arguments;
  the comparison macros do not. EvoCC does not claim Annex F, but its strict
  scalar profile models this flag.
- IEEE 754-2019 6.2.3: when several inputs are NaN, the result should carry
  the payload of one of them. The standard does not say which one.
- GCC 14.2 manual, "Using Vector Instructions through Built-in Functions"
  (https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Vector-Extensions.html):
  `vector_size` applies "only to integral and floating scalars"; operators
  work element by element; a scalar is broadcast "only if the scalar could be
  safely converted to the vector-element type"; comparisons give 0 or -1 in a
  signed integer vector of equal width; `__builtin_shuffle` takes mask lanes
  "modulo N" (one source) or "modulo 2*N" (two); `__builtin_convertvector`
  gives "a C cast of every element"; vector casts need equal size; `!, &&,
  ||` and a vector `?:` condition are C++ only.
- Clang 19 LanguageExtensions
  (https://releases.llvm.org/19.1.0/tools/clang/docs/LanguageExtensions.html):
  a GNU vector `?:` condition is C++ only; `&v[i]` is not allowed;
  `__builtin_convertvector` applies "a C-style cast ... to each element"; a
  `__builtin_shufflevector` index of -1 is a "don't care".
- ACLE Advanced SIMD intrinsics (`neon_intrinsics/advsimd.md`, ARM-software/acle
  main, fetched 2026-10-13): `vmlaq_f32` is `result = vadd(a, vmul(b, c))`,
  two rounded operations. `vfmaq_f32` is `FMLA`, one fused operation.
  `vcvtq_s32_f32` is `FCVTZS`, which saturates. `vmaxq_f32` is `FMAX`,
  `vmaxnmq_f32` is `FMAXNM` and `vrecpeq_f32` is `FRECPE`.

### 2.2 GCC 14 (measured)

- Lanes equal the scalar operations bit for bit, except one case. At O2 GCC
  swapped the operands of a 32-byte vector addition, so eight lanes with two
  quiet NaN operands carried the second payload (table A). IEEE 754-2019
  6.2.3 allows this. GCC output is therefore no oracle for two-NaN payloads.
- GCC contracts `a*b+c` at O2 by default, for scalars and vectors alike, but
  not at O0 (table E). It keeps `vmlaq_f32` unfused (table K).
- 64-bit mask lanes have type `long` (a `_Generic` probe returns 21).
- GCC accepts `v++` and `&v[1]` on float vectors (table J).
- Under `-mgeneral-regs-only` GCC rejects vector parameters, vector results
  and executed floating arithmetic, but accepts a vector object copy (table I).

### 2.3 Clang 19 and Apple clang 21 (measured)

- Lanes, masks, conversions, shuffles and flags equal the scalar operations
  in every measured case, including the 32-byte addition (tables A to K).
- Both contract `a*b+c` at O0 and O2 by default. This also fuses the body of
  `vmlaq_f32` in Clang's header (table K).
- Both reject `__builtin_shuffle` (table C), `v++` and `&v[1]` (table J).
- 64-bit mask lanes have type `long long` (the `_Generic` probe returns 37).
- Under `-mgeneral-regs-only` both reject vector parameters and results, but
  compile local floating arithmetic to soft-float calls (`__floatsisf`,
  `__addsf3` or `__mulsf3`, `__fixsfsi` in Clang 19) (table I).

### 2.4 Target ABI

- AAPCS64, ARM-software/abi-aa main, `aapcs64.rst` release 2025Q4, issued
  23 January 2026: a short vector is 8 or 16 bytes (section "Short Vectors").
  Rule C.1 puts it in `v[NSRN]`; C.4 and C.6 put it on the stack, aligned to
  8 or 16. HVA homogeneity uses the fundamental types "64-bit vector and
  128-bit vector"; "the type of the elements in the short vector does not
  form part of the test for homogeneity". An HVA has at most four members.
  Rule B.4 passes a composite larger than 16 bytes as a pointer to a caller
  copy. Only the low 64 bits of v8 to v15 are callee-saved. The soft-float
  variant leaves short vectors undefined.
- Apple, "Writing ARM64 code for Apple platforms": each variadic argument goes
  to "the appropriate number of 8-byte stack slots", and `va_list` is
  `char *`. The Apple clang call of `vsum` in `l05-abi-crossed.c` stores its
  anonymous vectors on the stack (`stp q0, q1, [sp]`), as the rule says.
- Measured (table G): GCC and Clang interoperate in all four directions at O0
  and O2 for 8-byte and 16-byte float vectors, a mixed scalar and vector
  cursor, a ninth vector on the stack, HVAs of two and four members, a
  mixed-lane HVA, a non-HVA record, a 32-byte vector, anonymous vectors and a
  vector kept live across a call.
- A 4-byte float vector (`vector_size(4)`) is not a short vector. GCC passes
  each argument in an 8-byte stack slot and returns it in `w0`. Clang passes
  it in `w0`/`w1` and returns it in `s0`. Crossed calls fail (exit 1) at O0
  and O2. This is the L06 conflict with floating lanes.

### 2.5 Probes and results

Commands. `E` is the frozen compiler. `D` is a case definition such as
`-DKIND=32` or `-DCASE=3`. `F` is the probe file.

```sh
E=.build/frozen/366fd796/evocc-darwin
L="--target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu"
$E $L -std=gnu11 -O0 $D --interpret-c-hir $F      # and -O2, --interpret-evoir
$E $L -std=gnu11 -O0 $D -S -o evo.s $F            # native: gcc-14 evo.s -o t; ./t
$E -std=gnu11 -O0 $D $F -o t && ./t               # EvoCC Darwin target
container run --rm --platform linux/arm64 --network none -v "$PWD:/work/p" \
  evo-nexlab:sealed bash -c 'cd /work/p && gcc-14 -std=gnu11 -O2 -w '"$D"' F -o t && ./t'
clang -std=gnu11 -O2 -w $D $F -o t && ./t         # host Apple clang
```

Flags used: `-ffp-contract=off` for tables A, B, C, G and K; no extra flag
for tables D, H, I, J, L and table E's default rows;
`-ffp-contract=off -frounding-math` and `-lm` for table F. Table I adds
`-mgeneral-regs-only` and compiles only.
Each probe ran at `-O0` and `-O2`, except the acceptance matrices of tables
D and J, which ran at `-O0`. For `l05-abi-crossed.c` and
`l05-hva-mixed-lanes.c`, each side was compiled with `-DPART=1` or
`-DPART=2` by one compiler and linked by `gcc-14`.

**Table A. Lane identity** (`l05-lane-identity.c`; exit = lanes whose bits
differ from the scalar operation; 13 operand encodings, five arithmetic
operations, six comparisons)

| KIND | GCC O0 | GCC O2 | Clang O0/O2 | Apple O0/O2 |
| --- | --- | --- | --- | --- |
| 32 (`float x4`) | 0 | 0 | 0/0 | 0/0 |
| 64 (`double x2`) | 0 | 0 | 0/0 | 0/0 |
| 322 (`float x2`) | 0 | 0 | 0/0 | 0/0 |
| 328 (`float x8`, 32 bytes) | 0 | 8 | 0/0 | 0/0 |

The eight GCC O2 lanes are all `+` with operands `7fc00011` and `ffc00022`.
The scalar gave the left payload. The vector gave the right one.

**Table B. Conversions** (`l05-convert.c`). All eight cases exit 0 under GCC,
Clang and Apple clang at O0 and O2. Each lane equals the scalar C cast:
truncation toward zero, RNE from integers, NaN quieting with the high payload
bits kept, and `0x7ff8000220000000` for a widened `0x7f800011`. Case 7 is an
observation of undefined lanes: NaN, 3e9, -3e9 and infinity saturate as
`FCVTZS` does under every reference.

**Table C. Shuffles** (`l05-shuffle.c`). `__builtin_shufflevector` (case 0)
and signaling-NaN transport through shuffle, lane write and copy (case 3)
pass under all references. `__builtin_shuffle` with a double vector and the
mask `{10000000001, 0}` (case 1, the pr85331 form) and with two float sources
(case 2) pass under GCC. Clang and Apple clang refuse both with "use of
unknown builtin '__builtin_shuffle'".

**Table D. Scalar broadcast** (`l05-broadcast.c`). GCC, Clang and Apple clang
agree. They accept `f4 + 2.0`, `d2 + float`, `d2 + int`, `f4 + 2.0L`,
`f4 < 2.0f`, `f4 + char` and `f4 + 16777216`. They reject `f4 + 0.1`,
`f4 + double`, `f4 + 16777217`, `f4 + int`, `d2 + long`, `f4 + 1e40` and
`d2 + 9007199254740993L`. EvoCC's front end agrees, except that it rejects
`f4 + 2.0L` (`E2102 vector-scalar-conversion-loses-values`).

**Table E. Contraction** (`l05-contract.c`; 0 = unfused, 1 = fused)

| Flags | GCC O0 | GCC O2 | Clang O0 | Clang O2 | Apple O0/O2 | EvoCC |
| --- | --- | --- | --- | --- | --- | --- |
| default, scalar | 0 | 1 | 1 | 1 | 1/1 | 0 in both interpreters, Linux native and Darwin native |
| default, `float x4` | 0 | 1 | 1 | 1 | 1/1 | E3050 |
| default, `double x2` | 0 | 1 | 1 | 1 | 1/1 | E3050 |
| `-ffp-contract=off`, all three | 0 | 0 | 0 | 0 | 0/0 | not run |

EvoCC warns that `-ffp-contract=on` and `-ffp-contract=fast` are "not
supported and has no effect".

**Table F. Sticky flags** (`l05-flags.c`; bits INVALID=1, DIVBYZERO=2,
OVERFLOW=4, UNDERFLOW=8, INEXACT=16; 64 = differs from the union of the
scalar lane flags). GCC, Clang and Apple clang give the same value at O0 and
O2 in every case: 19, 20, 1, 0, 1, 0, 24, 1, 2, 0, 0, 1 for cases 0 to 11.
No case sets 64. A vector operation raises exactly the union of its lane
flags. Relational lanes raise INVALID for a quiet NaN. Equality lanes do not.
An 8-byte vector raises no flag from absent upper lanes. Unary minus of a
signaling NaN raises nothing.

**Table G. Crossed calls** (`l05-abi-crossed.c`). Case 0: all eight runs (four
directions, O0 and O2) exit 0. Case 1 (4-byte vector): same-compiler pairs
exit 0, both crossed pairs exit 1, at O0 and O2. Apple clang self pairs exit
0 in both cases.

**Table H. Mixed-lane HVAs** (`l05-hva-mixed-lanes.c`). GCC and Clang pass all
four directions at O0 and O2 for case 0 (`int x4` with `short x8`), case 1
(`float x4` with `int x4`), case 2 (`float x2` with `double x1`) and case 3
(two `float x4`). Both use `v0` and `v1` for the record. EvoCC results are in
section 3.

**Table I. `-mgeneral-regs-only`** (`l05-general-regs.c`, O0 and O2)

| Case | GCC | Clang and Apple clang | EvoCC |
| --- | --- | --- | --- |
| 0 float vector parameter and result | error: vector types | error: ABI does not support it | E3050 |
| 1 local float lane arithmetic | error: floating-point types | accepted (soft-float) | E3050 |
| 2 float vector object copy | accepted | accepted | E3050 |
| 3 int vector parameter and result | error: vector types | error: ABI does not support it | E4002 |
| 4 scalar float arithmetic | error: floating-point types | accepted (soft-float) | E4002 |

EvoCC also refuses an integer vector object copy under this option (E4002).
GCC and Clang accept it.

**Table J. Operators and casts** (`l05-operators.c`)

| Case | GCC | Clang and Apple | EvoCC now |
| --- | --- | --- | --- |
| `%`, `~`, `&` | reject | reject | E2102 |
| `!v` | reject | reject | accepted, then E3050 |
| `v && w` | reject | reject | E2102 |
| `v++` | accept, runs | reject | E2102 `invalid-update-operands` |
| `&v[1]` | accept, runs | reject | E2102 `invalid-address-operand` |
| vector `?:` condition | reject | reject | E2202 |
| `long double` lanes, `vector_size(32)` | accept, runs | accept, runs | accepted, then E3050 |
| scalar `double` to `f2`, `f2` to `double` | reject | reject | E2102 `invalid-vector-cast` |
| equal-size `f4` to `i4` and back | accept, runs | accept, runs | accepted, then E3050 |
| runtime-index lane read and write | accept, runs | accept, runs | runs in all EvoCC modes |

**Table K. NEON families** (`l05-neon-subset.c`). All ten cases exit 0 under
GCC (GCC 14 header), Clang 19 (Clang header) and Apple clang at O0 and O2
with `-ffp-contract=off`. That covers operator forms, `vld1q`/`vst1q`, masks,
`vbslq_f32`, conversions, `vfmaq_f32`, abs/max/min/maxnm/sqrt, reductions,
estimates (`FRECPE(3) = 0x3eaa8000`, `FRSQRTE(4) = 0x3eff8000`), rounding
and lane moves. With default flags, case 4 shows `vmlaq_f32` fused under
Clang 19 and Apple clang (exit 4) and unfused under GCC (exit 0).

**Table L. Storage boundary** (`l05-storage-boundary.c`). Brace
initialization with lane access, a union pun and reads through
`(float *)&v` run in both interpreters, Linux native and Darwin native.
Whole-vector copy, a load through `*(f4 *)p`, a compound literal, a vector
argument and result, and a store through `*(f4 *)p` run only in the C-HIR
interpreter. EvoIR and native refuse them with E3050.

## 3. Current EvoCC behavior

- Every executed floating-vector value is refused. The C-HIR interpreter says
  `E3007 unsupported-interpreter-type` or `E3007
  unsupported-interpreter-expression: non-arithmetic floating operand` (or
  `pointer value in integer task-machine operation` for a conversion). The
  EvoIR interpreter, `-S` and the Darwin target say `E3050
  evoir-lowering-failed: not-lowered: floating-value`. The existing
  `floating-vector-boundaries.c` cases 0 to 3 give these same codes.
- The spec authority table (EvoIR 6.3) lists E3007 only as the withdrawn
  `scalar-function-fell-off-end`. The C-HIR interpreter still uses E3007 for
  every unsupported feature. This mismatch is outside L05.
- A floating NEON builtin reached by the C-HIR interpreter is `E3052
  not-executed: target-builtin: __builtin_aarch64_absv4sf`. Lowering stops
  earlier, with E3050 at the header compound literal of `vdupq_n_f32`
  (`arm_neon.h:10805`). On the Darwin target, `#include <arm_neon.h>` stops at
  `E1004 trailing-token` in Apple's `arm_vector_types.h:20`
  (`typedef __mfp8 mfloat8_t;`).
- Storage-only use already works (table L). The lane value type `vec<L, f32>`
  is what lowering lacks.
- **Silent ABI defect.** EvoCC compiles a caller that passes a record of
  floating vectors, because the caller only copies bytes. It uses the general
  composite rule: the 32-byte records of cases 1 and 3 go as a pointer in
  `x0` with a result pointer in `x8`, and the 16-byte record of case 2 goes
  in `x0`/`x1`. GCC and Clang pass all three as HVAs in `v0`/`v1`. Table H
  cases 1, 2 and 3 therefore exit 1 with an EvoCC caller and a GCC or Clang
  callee, with no diagnostic. Case 3 is an ordinary `struct { f4 a, b; }`.
  The cause is
  `homogeneous_vector_aggregate` in `crates/evocc-lower/src/lib.rs` (line
  3639 at HEAD `546fe46a`): it accepts only integer lanes, and it requires
  equal lane count and width where AAPCS64 requires only equal vector size.
- **Integer defect found on the way.** The same function does not classify
  `struct { int x4; short x8; }` as an HVA. EvoCC callee with a GCC or Clang caller
  crashes (exit 139). EvoCC caller with a GCC or Clang callee exits 1. This
  is an existing integer-vector ABI defect, not a floating question.
- The front end accepts `!v` on vectors (integer vectors run it and return a
  mask). GCC 14 and Clang 19 reject `!` on integer and floating vectors in C.
- `(void)v` on any vector is `E2102 invalid-vector-cast`. C allows a void
  cast of any expression. This defect is outside L05 but blocks probes.
- First error on each affected original, with the recorded flags:
  `UnitTests/Vector/build.c`, `constpool.c`, `simple.c`, `sumarray.c`,
  `sumarray-dbl.c`, `partialsums.c`, `ieee/pr72824-2.c`, `20050316-2.c`,
  `20050604-1.c`, `20060420-1.c`, `pr92618.c`, `pr85331.c`, `scal-to-vec1.c`
  and `scal-to-vec3.c` all stop at E3050. `UnitTests/Vector/build2.c` stops
  earlier at `E2202 invalid-initializer-type` on `printFV(&x)`, which passes
  `v4f32 *` to `FV *`; its recorded `-Wno-error=incompatible-pointer-types`
  does not change this. `aarch64_neon_intrinsics.c` (with `-std=c99 -O3`)
  stops at E3050 in `vcreate_f32` (`arm_neon.h:2571`).

## 4. Recommended rule

The rule has three parts. Part (a) is the language core. Part (b) is a
bounded first intrinsic set for the Linux target. Part (c) lists what stays
refused.

### (a) GNU floating-vector lanes

1. **Types.** `vector_size` vectors of `float` or `double` lanes are
   admitted on both targets, with the existing layout (B bytes, alignment
   min(B, 16), power-of-two lanes). An 8-byte or 16-byte vector is a value
   `vec<L, f32>` or `vec<L, f64>`. A wider vector is a local object with
   per-lane operations, as for integer lanes.
2. **Arithmetic.** `+ - * /` and unary `-` apply the existing scalar
   operation of EvoIR 3.13 to each lane: RNE baseline, gradual underflow, no
   excess precision. NaN results follow the scalar rule per lane: the first
   signaling operand, left before right, else the first quiet operand, with
   the quiet bit set; invalid arithmetic without a NaN gives positive default
   NaN. Unary minus flips only the sign bit. Unary plus is the identity.
   Division by zero gives the IEEE result, not undefined behavior.
3. **No implicit contraction.** A lane `a*b+c` is two rounded operations on
   both targets, as for scalars. This equals `-ffp-contract=off` on every
   reference. It differs from GCC's O2 default and from Clang's default.
4. **Comparisons.** `== != < <= > >=` give a vector of 0 or -1 lanes. The
   lane type is the signed integer of the lane width: `int` for `float`,
   `long` for `double` (the GCC type, shared by both targets). Relational
   lanes are signaling: in a strict region they raise INVALID for any NaN.
   `==` and `!=` raise INVALID only for a signaling NaN.
5. **Sticky flags and rounding.** In a `FENV_ACCESS ON` or `-frounding-math`
   region each lane is a strict world operation. Lanes execute in ascending
   order. The flags of the vector operation are the union of the lane flags.
   Outside such regions the existing fixed-environment check (E3064) applies
   to each lane operation.
6. **Conversions.** `__builtin_convertvector` is the C cast of each lane.
   Floating to integer truncates toward zero. An out-of-range or NaN lane is
   undefined and reports E3063 at that lane, as the scalar cast does. The
   saturating hardware result is not a defined value. Integer to floating
   and `float`/`double` conversions round once under the current mode and
   quiet NaNs with the high payload bits kept.
7. **Representation moves.** Shuffles, lane reads, lane writes, copies,
   loads, stores and equal-size casts between floating and integer vectors
   move bits. They never quiet a signaling NaN and never canonicalize a
   payload. `__builtin_shufflevector` and `__builtin_shuffle` keep the
   existing integer-vector index rules. `__builtin_shuffle` stays admitted on
   both targets because pr85331 needs it on Linux.
8. **Scalar broadcast.** Keep the current front-end rule. It matches GCC 14,
   Clang 19 and Apple clang on 13 of the 14 measured cases. Also admit an
   exact finite binary128 constant (`f4 + 2.0L`), which all three accept.
9. **Initializedness.** Each lane keeps its own initializedness. A partial
   initializer zero-fills the rest. Consuming an uninitialized lane is E3001.
   An out-of-range runtime lane index is E3011, as for integer vectors.
10. **ABI.** Follow AAPCS64 on Linux and the existing Apple variant on
    Darwin. An 8-byte or 16-byte vector uses the shared SIMD cursor. An HVA
    is one to four members of one vector size (64-bit or 128-bit), whatever
    the lane types. A vector wider than 16 bytes is a composite passed by
    reference. Darwin anonymous arguments use 8-byte stack slots; EvoCC's
    Darwin integer-vector varargs already pass with Apple clang (four of four
    pairs in this session).
11. **Equality with integer vectors.** Floating lanes reuse the same `vec.pack`,
    `vec.extract` and `vec.bitcast` forms with `f32`/`f64` lanes. Lane values
    are ordinary scalar `fp` values. All FP stays outside P0 EqSat.

### (b) First intrinsic set (Linux GCC 14 header only)

- Header functions that GCC 14 writes with GNU vector operators become
  executable through part (a) without new builtins. Examples are `vaddq_f32`,
  `vsubq_f32`, `vmulq_f32`, `vdivq_f32`, `vnegq_f32`, `vmulq_n_f32`,
  `vdupq_n_f32`, `vceqq_f32`, `vcltq_f32`, `vcreate_f32`, `vgetq_lane_f32`
  and `vsetq_lane_f32`, with their `f64` and 64-bit forms. Lane bounds keep
  the registered `__builtin_aarch64_im_lane_boundsi`.
- Register these floating builtins, because each maps to an existing EvoIR
  operation with no new numerical semantics: `ld1` and `st1` for `v2sf`,
  `v4sf` and `v2df`; `get_low`/`get_high` for `v4sf` and `v2df`;
  `combinev2sf` and `combinedf`; `fma` for `v2sf`, `v4sf` and `v2df` (per-lane
  `fp.fma`); `float_mla` and `float_mls` (multiply, then add or subtract, two
  roundings, as ACLE `vadd(a, vmul(b, c))` says); `abs` (clear the sign bit);
  `float` and `floatuns` (`fp.from_signed`/`fp.from_unsigned`);
  `float_extend_lo_v2df` and `float_truncate_lo_v2sf` (`fp.convert`); and
  `simd_bsl` on floating modes (bit select on the representation). All names
  are present in `crates/evocc-types/src/neon-signatures.tsv`.
- `vmlaq_f32` stays unfused under any option. Clang's fused result under its
  default contraction is not followed.

### (c) Explicit refusals

- Every other floating `__builtin_aarch64_*` operation keeps
  `E3052 not-executed: target-builtin: NAME` in the interpreters and an
  equivalent named refusal in lowering. This includes saturating
  `FCVTZS`/`FCVTZU` (`lbtrunc`, `lceil`, `lfloor`, `lround`, `lfrintn`),
  `FMAX`/`FMIN`/`FMAXNM`/`FMINNM`, `FSQRT`, all `FRINT*` forms, the estimate
  and step forms (`frecpe`, `frecps`, `frsqrte`, `frsqrts`), `fmulx`,
  `fabd`, `facg*`, pairwise and across-lane reductions, structure loads and
  stores (`ld2`-`ld4`, `st2`-`st4`, `ld*r`) and lane-indexed fused forms.
  Each needs Arm pseudocode-level semantics before admission.
- Half and bfloat lanes (`float16x4_t`, `_Float16 __attribute__((vector_size))`)
  wait for L03.
- `long double` lanes stay E3050 with a named reason. No original needs them.
- A vector smaller than 8 bytes at an external boundary stays E4002. GCC and
  Clang disagree (table G), as L06 records for integer lanes.
- `!`, `&&`, `||`, a vector `?:` condition, `%`, `~`, `&`, `|`, `^` and casts
  between a floating scalar and a vector are E2102. `!` is a change from
  today; both references reject it in C.
- `v++`, `v--` and `&v[i]` stay E2102, as in the integer-vector profile. GCC
  accepts them; Clang rejects them; no affected original needs them.
- Under `-mgeneral-regs-only`, any floating-vector value, argument, result or
  copy is E4002, matching the current integer-vector and scalar FP rule.
- On Darwin, NEON intrinsics stay refused. Apple's `arm_neon.h` is Clang's
  header with `__builtin_neon_*` builtins, and EvoCC cannot parse it today.

### Rejected alternatives

- Treating a floating vector as its integer representation for arithmetic. It
  would give wrong IEEE results.
- Using GCC output as the NaN payload oracle. GCC commutes `+` and `*`.
- Following Clang's default contraction. It changes results with the
  optimization level and contradicts the scalar profile.
- Accepting saturating conversion as the meaning of `__builtin_convertvector`.
  C makes those lanes undefined; only `vcvtq_*` defines saturation.
- Admitting all `__builtin_aarch64_*` floating builtins by analogy. Each
  family has distinct NaN, rounding or estimate semantics.

## 5. Implementation scope

### 5.1 Compiler

- Parser (`crates/evocc-parser/src/vector.rs`): refuse `!` on vectors in
  `make_vector_unary`; admit exact finite binary128 constants in
  `vector_broadcast_operand`. The `(void)` cast defect sits in
  `make_vector_cast` and is separate.
- C-HIR interpreter (`crates/evocc-chir/src/interp/vector.rs`,
  `interp/floating.rs`, `interp/neon.rs`): floating lanes in `vector_lane`;
  lane arithmetic, masks and conversions through the shared scalar engine;
  shuffles as raw moves; the part (b) builtins.
- Lowering (`crates/evocc-lower/src/vector.rs`, `lib.rs`, `floating.rs`,
  `neon.rs`): floating lanes in `ExprValue`, `c_value_type`,
  `function_value_type`, `pack_vector` and `lower_vector_values`; lane `fp.*`
  and strict operations; masks from `fp` predicates; E3063 per converted lane;
  the AAPCS64 size-only test in `homogeneous_vector_aggregate`.
- Types (`crates/evocc-types/src/neon.rs`): the part (b) entries in
  `operation()` only.
- EvoIR (`crates/evocc-evoir`): `VectorInst::validate` for `f32`/`f64` lanes
  in `vec.pack`, `vec.extract` and `vec.bitcast`; the `hva_mark` grammar
  (EvoIR 9.3, `text.rs`, `text_parse.rs`) for floating and mixed lanes of one
  size; the EvoIR interpreter. The proof checker (`crates/evocc-proofcheck`)
  rebuilds the same forms; all FP stays outside P0.
- EvoMIR (`crates/evocc-mir`): lane moves between scalar `simd32`/`simd64`
  and vector `simd64`/`simd128` values (`lower.rs`, `emit.rs`); ABI
  classification (`generic/abi.rs`, `isa/aarch64/abi.rs`, `abi_tables.rs`);
  allocation checks (`verify_alloc.rs`, `generic/allocverify.rs`); the EvoMIR
  interpreter. Per-lane scalar instructions suffice at first.
- Specifications: EvoIR 3.8 (generalize "Integer vector representations"),
  3.10, 3.13 (remove "FP vectors are outside this revision"), 9.3, 10.1.14
  and the appendix table; EvoMIR 2.2, 2.7 and 6.3.

### 5.2 Runtime helpers

None for `f32`/`f64` lanes. Hardware lane operations and existing scalar code
suffice. `long double` lanes would need the quad helpers and stay out of
scope. No libm function is added.

### 5.3 Tests that meet the TODO exit

1. Both interpreters and Linux native, O0 and O2: `l05-lane-identity.c`
   (all four kinds), `l05-convert.c` cases 0 to 6, `l05-shuffle.c` cases 0 to
   3, `l05-contract.c` (expect 0), `l05-flags.c` (expect the table F values),
   `l05-storage-boundary.c` (all cases), the existing
   `floating-vector-boundaries.c` cases 0 to 3, and `l05-neon-subset.c`
   cases 0, 1, 2, 4 and 9 (their header bodies use only part (a), the
   registered lane-bounds builtin and the part (b) builtins).
2. Negative tests: `l05-convert.c` case 7 gives E3063 at the NaN lane; an
   uninitialized lane gives E3001; an invalid runtime index gives E3011;
   `l05-operators.c` gives E2102 for `!v`; `l05-general-regs.c` gives E4002;
   `l05-abi-crossed.c` case 1 gives E4002; `l05-neon-subset.c` cases 3, 5,
   6, 7 and 8 give named E3052 refusals (`lbtrunc`, `fmax_nan`, `sqrt`,
   reductions, estimates, `FRINT*`).
3. ABI: `l05-abi-crossed.c` case 0 and `l05-hva-mixed-lanes.c` cases 0 to 3,
   with EvoCC as caller and as callee against GCC 14 and Clang 19, O0 and O2.
   On Darwin, the same against Apple clang, including anonymous vectors.
4. IR fidelity: one-field mutations of a lane type, an operation and an `hva`
   mark must fail text validation or proof rebuilding; a parser round trip
   alone is not enough.
5. Darwin native lane identity (`l05-lane-identity.c`, `l05-flags.c`).

### 5.4 Original programs to replay

Linux, unchanged sources and recorded argv:

- LLVM test-suite `2919e55176a5`: `Benchmarks/BenchmarkGame/partialsums.c`,
  `Regression/C/gcc-c-torture/execute/ieee/pr72824-2.c`,
  `UnitTests/Vector/{build,build2,constpool,simple,sumarray,sumarray-dbl}.c`
  and the suite copies of `20050316-2`, `20050604-1`, `20060420-1`,
  `scal-to-vec1` and `scal-to-vec3`. The fetched SHA-256 values match
  `tests/contracts/benchmark-program-dispositions.json`.
- GCC torture `78d4ac73dd39`: `20050316-2`, `20050604-1`, `20060420-1`,
  `pr92618`, `scal-to-vec1` and `scal-to-vec3`; also `pr85331` against GCC
  only (Clang rejects it, so that reference is not run).
- `build2.c` also needs an incompatible-pointer argument rule under
  `-Wno-error=incompatible-pointer-types`. Without it, it stays E2202.
- `UnitTests/Vector/AArch64/aarch64_neon_intrinsics.c` does not close here.
  It reaches about 1,800 distinct GCC header builtins, about 300 of them with
  floating or half modes (a regex count over the GCC 14 header), plus FPSR
  inline assembly. Keep its deferral and track it as its own item.

## 6. Size, dependencies and risks

- **Size: large** for part (a) with the ABI fix and part (b). It touches the
  parser, both interpreters, lowering, EvoIR, the proof checker, EvoMIR and
  two specifications. The NEON original is a separate, larger project.
- **Dependencies.** The strict FP world and binary128 are done and reused.
  The integer HVA defect (section 3) should be fixed first, because the same
  classifier serves both. L03 gates half lanes. L06 gates sub-8-byte vectors.
  L04 does not interact. A separate pointer-compatibility rule gates
  `build2.c`.
- **Risk: silent wrong code today.** Callers that pass records of floating
  vectors compile and run wrong against GCC or Clang code. Until L05 lands,
  the lead could make such external calls and definitions E4002.
- **Risk: payload oracles.** Tests must not compare two-NaN payloads of `+`
  and `*` with GCC output. They should compare with the scalar rule.
- **Risk: contraction.** The LLVM suite sets no `-ffp-contract=off` on
  AArch64, and its references come from `clang-19 -O3`. By inspection, the
  affected originals have no multiply-add whose fused and unfused results
  differ, but a replay failure there must be checked against this first.
- **Risk: mask lane type.** `_Generic` can see `long` against Apple clang's
  `long long` on Darwin. No ABI depends on it.

## 7. Open points

1. Darwin mask type for 64-bit lanes: keep the shared `long`, or follow Apple
   clang's `long long` on Darwin? Representation and ABI agree either way.
   The deciding fact is whether any Darwin consumer uses `_Generic` or
   `typeof` on a mask; none is known.
2. `v++`, `v--` and `&v[i]`: the Linux principle says follow GCC, but the
   integer-vector profile already refuses them. The lead should confirm that
   floating lanes keep the integer decision.
3. `-mgeneral-regs-only` object copies: GCC and Clang accept a plain copy;
   EvoCC refuses copies of integer vectors too. This packet keeps the stricter
   shared rule. The minimal experiment is a kernel unit that copies a vector
   object under this option; none is known.
4. Part (b) is a choice, not a consumer requirement. No affected original
   except the full NEON test calls a floating intrinsic. The lead may defer
   part (b) entirely and keep E3052 for every floating builtin.
