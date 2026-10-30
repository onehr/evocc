# L04: Floating complex execution

This is a decision packet for the lead. It was prepared on 2026-10-13. It
proposes a rule. It approves nothing. Labels: **M** marks a fact measured for
this packet, **D** a documented fact with its source, and **I** an inference.

## Subject

- Compiler: `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (546fe46a26e8
  2026-10-13)`, SHA-256 `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04`;
  `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md` Revision 1.0.1 at HEAD `546fe46a`.
- Linux: GCC 14.2.0-19 and Clang 19.1.7 in `evo-nexlab:sealed`, image
  `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`,
  glibc 2.41; helpers from `libgcc-14-dev` 14.2.0-19 `libgcc.a`, SHA-256
  `60dbf748e1c310aa50300a33f30bf4ee070061071aee74a52cf115a7b8765e50`.
- Darwin: Apple clang 21.0.0 (clang-2100.1.1.101), arm64-apple-darwin25.6.0,
  macOS 26.6 (25G72); helpers from the system compiler-rt in libSystem.
- Sources (D): GCC 14.2.0 `libgcc2.c`, `tree-complex.cc`, `c-typeck.cc`,
  `c-opts.cc`, `opts.cc`, `fold-const.cc`, `builtins.cc`; LLVM 19.1.7
  `CGExprComplex.cpp` and compiler-rt `mul?c3.c`, `div?c3.c`, `fp_lib.h`;
  AAPCS64 2025Q1; Apple "Writing ARM64 code for Apple platforms"; N1570; N3220.

## 1. Question

Which rules make `float`, `double` and `long double _Complex` executable, so
that both interpreters and native code run one contract on Linux and Darwin?
The rule must cover representation, construction, parts and conversions;
equality, truth and mixed real/complex operands; the multiply and divide
algorithms with their NaN results and status flags in all four rounding modes;
static initializers; the call ABI; and options, helpers and the claim ceiling.

## 2. Evidence

### 2.1 C standard (D)

- C11 6.2.5p11–13: three complex types, a conditional feature
  (`__STDC_NO_COMPLEX__`), each laid out as `T[2]`, real part first.
- C11 6.3.1.6–6.3.1.7: conversions work per part; real to complex gives a
  `+0` imaginary part; complex to real discards the imaginary part. 6.3.1.2:
  `_Bool` is 0 when the value compares equal to 0.
- C11 6.3.1.8: the usual arithmetic conversions keep the type domain, so a
  real operand stays real. 6.5.9p4: complex values are equal when both parts
  are equal. 5.2.4.2.2p6: the accuracy of `+ - * /` is implementation-defined.
- C11 6.5.2.4p1 and 6.5.3.1p1: `++` and `--` need a real or pointer type, so
  complex increment is a GNU extension. 7.3.4: `CX_LIMITED_RANGE` only permits
  the plain formulas, default off. 7.3.9.3 defines the `CMPLX` macros.
- Annex G binds only an implementation that defines `__STDC_IEC_559_COMPLEX__`
  (C11 G.1) or, in C23, `__STDC_IEC_60559_COMPLEX__` (N3220 G.1; the old macro
  is obsolescent, 6.10.10.4). C11 G.5.1p2–3 (C23 G.5.2p2–3): a real-by-complex
  product and a complex-by-real quotient use the per-part formulas. G.5.1p4
  lists the infinity properties. G.5.1p5 lets complex `*` and `/` raise
  spurious exceptions.

### 2.2 GCC 14 and libgcc (D, M)

- (D) C sets `flag_complex_method = 2` by default (`c-opts.cc`
  `c_common_init_options_struct`). `-fcx-limited-range` and `-ffast-math`
  select method 0. `-fcx-fortran-rules` selects method 1 (`opts.cc`).
- (D) `tree-complex.cc`: at `-O0` every complex-by-complex multiply calls
  `__mul?c3`. At `-O1` and above GCC inlines `ar*br - ai*bi` and
  `ar*bi + br*ai`, tests `isunordered` and calls the helper on that path.
  Division always calls `__div?c3`. GCC never treats a floating zero part as
  zero while signed zeros are honored (`some_nonzerop`).
- (D) `c-typeck.cc` `build_binary_op`: complex op real (`+ - * /`) and real op
  complex (`+ - *`) work per part. Real divided by complex converts the real
  operand to `(r, +0)` and calls the helper.
- (D) `libgcc2.c` MULC3: four products, `x = ac - bd`, `y = ad + bc`; when both
  are NaN it boxes infinities, zeroes NaNs and recomputes with `INFINITY *`.
  DIVC3: binary32 computes the plain formula in binary64 when hardware double
  exists (`__LIBGCC_HAVE_HWDBL__`); binary64 and binary128 use Smith's method
  with RBIG/RMIN scaling and a subnormal-ratio branch. All share one
  NaN-recovery step.
- (M) The qualified `libgcc.a` is not the plain source. Its `__divdc3` has 14
  FMADD/FMSUB/FNMSUB instructions: every Smith product-plus-term (`denom`, both
  numerators, both paths, both ratio branches) is fused. `__divsc3` fuses in
  binary64, where float products are exact. `__mulsc3` and `__muldc3` fuse only
  in their recovery path, where one product of each pair is exact.
  `__multc3` and `__divtc3` call soft-fp helpers and fuse nothing.
- (M) GCC's default `-ffp-contract=fast` also fuses the inline `-O2` multiply.
  100,000 random double products hash to `671d1df4154fedff` under GCC `-O2`,
  and to `f38dd7039822c824` under GCC `-O0`, GCC `-O2 -ffp-contract=off` and
  Clang `-O0`/`-O2`.
- (M) GCC `-O2 -frounding-math` reuses one `__divdc3` result, and one product,
  across `fesetround(FE_UPWARD)` and `fesetround(FE_TONEAREST)`. GCC `-O0` and
  Clang `-O0`/`-O2` compute both.
- (D, M) Constant folding uses MPC (`const_binop`, `do_mpc_arg2`,
  `do_mpc_ckconv`): each part is the correctly rounded exact result. Runtime
  expressions fold only when finite without overflow or underflow, and under
  `-frounding-math` only when exact. Static initializers always fold.
- (M) GCC rejects `static _Bool b = 0.0 + 1.0i;` and a static `float _Complex`
  set from a `double` complex quotient; Clang accepts both. GCC ignores
  `#pragma STDC CX_LIMITED_RANGE ON` (`-Wunknown-pragmas`) and still calls the
  helpers. `__builtin_complex(1.0f, 2.0)` is an error ("operands of different
  types"). (D) gcc-14.2.0 `Complex.html` documents `__builtin_complex`,
  `__real__`, `__imag__`, `~` and the `i`/`j` constant suffixes.
- (D) GCC predefines `__GCC_IEC_559_COMPLEX` = 2, so glibc's `stdc-predef.h`
  defines `__STDC_IEC_559_COMPLEX__` and `__STDC_IEC_60559_COMPLEX__`.

### 2.3 Clang and compiler-rt (D, M)

- (D) `CGExprComplex.cpp` with the default `-fcomplex-arithmetic=full`: the
  multiply is inline, tests "real NaN and imaginary NaN", then calls the
  helper, at every `-O` level. Division calls the helper. Mixed operands work
  per part; real divided by complex passes `b = +0`.
- (M) Clang 19 on Linux links the libgcc helpers, so its results and flags
  equal GCC's. `basic`, `-fcx-limited-range` and `CX_LIMITED_RANGE ON` inline
  the plain formulas; `improved` and `-fcx-fortran-rules` inline Smith;
  `promoted` divides doubles with binary128 soft-fp calls.
- (D) compiler-rt divides with `logb`/`scalbn` scaling of the divisor only, the
  plain formula and the same recovery step. Its multiply is MULC3.
- (M) Apple's `long double` is binary64, so `long double _Complex` calls
  `__muldc3` and `__divdc3`. Apple's helpers equal the compiler-rt source with
  fused sums (table 2.5.2).
- (M) Static folding differs between compilers (table 2.5.3).

### 2.4 Target ABI (D, M)

- (D) AAPCS64 2025Q1, C/C++ type mapping: each complex type is laid out as
  `struct { T re; T im; }`. It is therefore an HFA of two members. Rules B.3,
  C.2, C.3, C.4 and C.6 apply: two consecutive s/d/q registers when
  NSRN + 2 ≤ 8. Otherwise NSRN becomes 8 and the value goes to the stack,
  8- or 16-byte aligned; later FP arguments do not backfill. A record
  member that is complex counts as two members.
- (D) Apple: `long double` equals `double`; each variadic argument takes 8-byte
  stack slots; `va_list` is `char *`. (M) Linux passes variadic complex values
  in SIMD registers (`d0/d1`, `s2/s3`, `q4/q5`); Apple stores `d0,d1` at
  `[sp]`, packs `s2,s3` into the slot at `[sp+16]` and stores `d4,d5` at `[sp+24]`.
- (M) The crossed matrix passes 8/8 (GCC or Clang worker × GCC or Clang
  driver × `-O0`/`-O2`); Apple self-links pass at `-O0` and `-O2`.

### 2.5 Probes (M)

Probes in `tests/fixtures/rules-20261013/`: `l04-complex-ops.c`
(KIND=32/64/128 × CASE=0..9), `l04-helper-model.c`, `l04-codegen.c`,
`l04-abi-worker.c`, `l04-abi-driver.c`. Linux runs use `container run --rm
--platform linux/arm64 --network none -v DIR:/work/p evo-nexlab:sealed`:

```text
CC -std=gnu11 -O0|-O2 -frounding-math -ffp-contract=off -DKIND=K -DCASE=C l04-complex-ops.c -o t -lm && ./t
CC -std=gnu11 -O0|-O2 -frounding-math -ffp-contract=off l04-helper-model.c -o h -lm && ./h
CC -std=gnu11 -O0|-O2 [-ffp-contract=off|-fcx-limited-range|...] -S l04-codegen.c
CCW -std=gnu11 -O0|-O2 -c l04-abi-worker.c; CCD ... -c l04-abi-driver.c; gcc-14 d.o w.o && ./a.out
evocc --target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu
      -std=gnu11 -O0|-O2 -frounding-math -ffp-contract=off -DKIND=K -DCASE=C
      --interpret-c-hir|--interpret-evoir|-S l04-complex-ops.c
```

CC is `gcc-14` or `clang-19`. On Darwin the same commands use host `clang`.

**2.5.1 Emitted code** (`l04-codegen.c`, `-S`):

| Form | GCC -O0 | GCC -O2 | Clang -O0/-O2, Apple |
| --- | --- | --- | --- |
| f32/f64 `a*b` | call `__mul?c3` | inline, 2 fused ops by default, 0 with `-ffp-contract=off`; 1 unordered FCMP; call | inline unfused; 2 FCMP; call |
| f128 `a*b` | call `__multc3` | 4 `__multf3`, `__subtf3`, `__addtf3`, `__unordtf2`, `__multc3` | same with 2 `__unordtf2`; Apple: `__muldc3` |
| `a/b` | call `__div?c3` | call | call; Apple long double: `__divdc3` |
| complex op real, real op complex (`+ - *`), complex / real | per part, no call | same | same |
| real / complex | `__divdc3(r, +0, c, d)` | same | same |
| `==`, `!=` | two FCMP | FCMP then FCCMP | two FCMP |
| truth, `(_Bool)` | real first; imaginary only when real is 0 | same | both FCMP |

**2.5.2 Helper qualification** (`l04-helper-model.c`). The grid has
13⁴ = 28,561 special operand sets. The random set has 200,000 finite operand
sets in each rounding mode. Results are value / exact-bit mismatches;
parentheses give flag-set mismatches.

| Linux, probe built by GCC -O2 | grid | random RNE / UP / DOWN / ZERO |
| --- | --- | --- |
| `__divdc3` vs libgcc source, fused sums | 0 / 0 (0) | 0 / 0 / 0 / 0 |
| `__divdc3` vs libgcc source, unfused | 272 / 4,441 | 15,789 / 45,736 / 45,455 / 58,079 |
| `__divsc3` vs libgcc source (binary64, fused) | 0 / 0 (0) | 0 / 0 / 0 / 0 |
| `__divsc3` vs unfused source | 0 / 4,711 (NaN bits only) | not counted |
| `__muldc3` vs MULC3 source | 0 / 0 (0) | 0 / 0 / 0 / 0 |
| `__divtc3` vs libgcc source (quad operations) | NOT RUN | 0 / 0 / 0 / 0 |
| `__divdc3` vs compiler-rt source, fused | 3,760 / 7,952 | 123,969 in RNE |

The same fused model built by GCC `-O0` or by Clang gives 0 value and 0 flag
mismatches but 396 and 4,491 NaN-bit mismatches. The NaN sign and payload
therefore follow the operand order of the helper's instructions. The helper
reads the dynamic rounding mode: its UP, DOWN and ZERO results differ from its
RNE result in 168,331, 181,269 and 174,221 of 200,000 cases.

| Darwin, probe built by Apple clang | grid | random, all four modes |
| --- | --- | --- |
| `__divdc3` vs compiler-rt source, fused | 0 / 0 (1,836 or 4,775 flag sets differ, by probe build) | 0 |
| `__divdc3` vs compiler-rt source, unfused | not counted | 32,188 / 98,392 / 72,317 / 77,431 |
| `__divsc3` vs compiler-rt float source, fused | 0 / 0 (1,488 flag sets differ) | 0 |
| `__muldc3` vs MULC3 source | 0 / 0 (0) | 0 |
| `__divdc3` vs libgcc source, fused | 3,760 / 5,627 | 123,969 in RNE |

Status of `(1+1i)/(0+0i)`, both parts `+inf` everywhere:

| Target | binary32 | binary64 | long double |
| --- | --- | --- | --- |
| Linux (GCC, Clang, direct helper call) | INVALID | INVALID, DIVBYZERO | INVALID, DIVBYZERO |
| Darwin (Apple clang) | INVALID | INVALID | INVALID (binary64) |

Cause (D, M): `__divsc3` evaluates `0/0` in binary64 (INVALID) and recovers
with an exact `inf * 1`. `__divdc3` takes the Smith path, so `ratio = 0/0`
(INVALID). A NaN ratio fails `|ratio| > RMIN`, and the subnormal-ratio branch
computes `b/c = 1/0` (DIVBYZERO). The targets also differ beyond the last
bit: `(2^127 + 2^127 i)/(2^127 + 2^127 i)` is `1 + 0i` with no flags on Linux,
but `+inf + 0i` with OVERFLOW and INEXACT on Darwin. Compiler-rt scales only
the divisor, so `a*c + b*d` overflows.

**2.5.3 Static and folded values** of `(1+1i)/(0.2+0.5i)`, real and imaginary:

| Source | Value |
| --- | --- |
| GCC static, `-O0`/`-O2`, with or without `-frounding-math` | `0x1.34f72c234f72cp+1`, `-0x1.08d3dcb08d3dcp+0` (the correctly rounded quotient, checked with exact rationals) |
| Clang 19 and Apple static | `0x1.34f72c234f72bp+1`, `-0x1.08d3dcb08d3dcp+0` |
| Linux runtime helper | `0x1.34f72c234f72cp+1`, `-0x1.08d3dcb08d3ddp+0` |
| GCC local constant expression | static value without `-frounding-math`; runtime value with it |

For `(1+1i)/(0.1f+0.1fi)` in binary32, GCC static and the Linux runtime give
10; Clang static and the Darwin runtime give `0x1.3ffffep+3`.

**2.5.4 Operation matrix** (`l04-complex-ops.c`): 180 reference executions
(3 compilers × 30 KIND/CASE × 2 levels). 176 pass. The 4 failures are Clang
and Apple KIND=64 CASE=7 check 5, the static quotient above.

**2.5.5 Originals** (M). musl 1.2.6 (tarball SHA-256 `d585fd3b…` equals the
receipt) builds all 68 `src/complex` objects with the original flags under
GCC. Only `cpow.o`, `cpowf.o` and `cpowl.o` reference helpers
(`__muldc3`, `__mulsc3`, `__multc3`); none references a division helper.
musl's `complex_impl.h` builds `CMPLX` from a union compound literal of
`_Complex T` and `T[2]`, and its `cimag` reads that union. In the GCC torture
originals at `78d4ac73` (hashes equal the registry), only `complex-5`
references a helper (`__divsc3`). LLVM `mandel` and `mandel-2` reference
`__muldc3`. All 17 references pass under GCC `-O2`.

## 3. Current EvoCC behavior (M)

| Construct | First diagnostic (C-HIR, EvoIR and `-S`; the matrix at `-O0` and `-O2`) |
| --- | --- |
| `__real__ z`, `__imag__ z` on floating complex | `E1003` expected-expression |
| `2.5i`, `2.5fi`, `2.5if`, `1.0iF`, `2.5Li`, `2.5j`, `0x1p3i`; glibc `I` | `E0112` stray-token (only integer `3i` lexes) |
| `__builtin_complex`, glibc `CMPLX` | `W0003` function has no declaration (error) |
| `z++` | `E2102` invalid-update-operands |
| `+ - * / == !=`, casts, copies, union member read | C-HIR: `E3007` unsupported-interpreter-type; EvoIR/asm: `E3101` evoir-lowering-failed: object subvalue is neither an integer nor pointer |
| complex parameter or result | `E3049` not-lowered: floating-call-interface (lowering) |
| file-scope `double _Complex g = 1.0;` | `E3101` global initializer is not an admitted integer constant (lowering) |
| `double _Imaginary`; `_Float16 _Complex` | `E1001`; `E1007` |
| `-fcx-limited-range`, `-fcx-fortran-rules`, `-fcomplex-arithmetic=basic`, `-fno-cx-limited-range`; `-ffast-math` | unsupported option; unsupported floating-point profile option |
| `#pragma STDC CX_LIMITED_RANGE ON` | accepted with no diagnostic |

All 180 EvoCC commands of the operation matrix (30 builds × 2 levels × C-HIR,
EvoIR and `-S`) fail. CASE 0, 2, 4, 5, 6 and 9 stop at the first `__real__`
(`E1003`), CASE 1 and 7 at an imaginary constant (`E0112`), CASE 3 at
`__builtin_complex` (`W0003`) and CASE 8 at `z++` (`E2102`).

Linux layouts are already right: 8/4, 16/8 and 32/16. On the Darwin target
`sizeof(long double)` is 16 while `__LDBL_MANT_DIG__` is 53 (Apple: 8 bytes),
and native long double arithmetic is `E4009`. Neither `__STDC_IEC_559_COMPLEX__`
nor `__STDC_NO_COMPLEX__` is predefined. The generic `E3101` contradicts
10.1.7, which gives `E3050` to a floating value on an integer-only path. The
September batch recorded `E2001` for `__builtin_complex`; now it is `W0003`.

## 4. Recommended rule

**R1 Types.** `float`, `double` and `long double _Complex` (and `__complex__`)
are admitted on Linux with f32, f64 and f128 parts. Darwin admits the first
two. Darwin `long double _Complex` stays refused until Darwin `long double`
has Apple's 8-byte binary64 layout. `_Imaginary`, half and `_FloatN` complex
stay refused with named diagnostics. Integer complex is unchanged.

**R2 Values.** A complex object is two adjacent parts, real first, laid out as
`T[2]`. Each part is an ordinary section 3.13 scalar: bits are preserved and
NaN is not poison. An operation must find each part it consumes initialized.
A copy transports both parts and their initializedness. A union read of
`_Complex T` after `T[2]` is a same-format representation transport.

**R3 Construction.** `__builtin_complex(x, y)` needs two operands of one real
floating type after lvalue conversion, with no promotion; a mismatch is an
error, as in GCC. Operands evaluate once, left to right, and are copied
exactly, with no arithmetic and no status. A floating constant with an `i`,
`I`, `j` or `J` suffix (either order with `f`, `F`, `l`, `L`) has real part
`+0` and the rounded literal as imaginary part. glibc `I` is `(0.0f, 1.0f)`.

**R4 Parts.** `__real__` and `__imag__` are lvalue projections of one part.
They read nothing else. On a real operand they follow GCC (`__imag__` gives 0).
`__builtin_creal*` and `__builtin_cimag*` project. `~z` and `__builtin_conj*`
negate the imaginary part. Unary `-` negates both parts with a sign flip and
no status. GNU `++` and `--` add or subtract 1 on the real part in the
current mode, as GCC and Clang do.

**R5 Conversions.** Conversions follow C11 6.3.1.6–6.3.1.7, part by part,
real part first. A complex-to-real conversion does not read the imaginary
part. A complex-to-integer conversion converts the real part and keeps E3063.
A complex-to-`_Bool` conversion and a truth test use quiet `une` against 0
on the real part first. They compare the imaginary part only when the real
part equals 0, as GCC's front end builds them.

**R6 Equality.** `==` and `!=` execute both quiet comparisons, real then
imaginary, as GCC `-O0` and Clang do. GCC `-O2` skips the second when the real
parts differ; only a signaling NaN shows the difference.

**R7 Mixed operands.** Complex op real (`+ - * /`) and real op complex
(`+ - *`) convert the real operand to the part type and work per part:
`(a+bi)+r = (a+r)+bi`, `r-(a+bi) = (r-a)+(-b)i`, `(a+bi)*r = ar+bri`,
`(a+bi)/r = a/r+(b/r)i`. Real divided by complex is complex division of
`(r, +0)`. Both targets agree (C11 6.3.1.8, G.5.1p2–3).

**R8 Multiplication.** The result is the MULC3 algorithm (libgcc and
compiler-rt agree) in the part format. Compute `ac`, `bd`, `ad`, `bc`, then
`ac - bd` and `ad + bc`, each rounded in the current mode with no contraction.
When both results are NaN, apply the recovery step. The status is the union
of the executed operations. An inline fast path followed by the helper call
raises the same sticky set. This equals GCC `-O0`, GCC `-ffp-contract=off`,
Clang and Apple.

**R9 Division.** The result is the target profile's helper algorithm, run in
the current rounding mode:

- Linux, binary32: the plain formula in binary64, then one conversion per part.
- Linux, binary64: Smith's method with the libgcc scaling. Every
  product-plus-term (denominator and both numerators, in every branch) is
  fused, as in the qualified `libgcc.a`.
- Linux, binary128: Smith's method with unfused quad operations.
- Darwin, binary32 and binary64: the compiler-rt `logb`/`scalbn` algorithm with
  fused sums.

Each algorithm ends with its source's recovery step. The status is the union
of the executed operations, including the signaling ordered comparisons and
the spurious flags that G.5.1p5 permits. A NaN result takes its sign and
payload from the qualified binary's operand order.

**R10 IR.** Add `fp.cmul`, `fp.cdiv`, `fp.strict.cmul` and `fp.strict.cdiv`.
Each takes four operands of one format and returns two parts; the strict forms
also thread world. They have the effects of their scalar peers and stay
outside P0. Every other complex operation expands to existing scalar
instructions, real part first. `-frounding-math` and `FENV_ACCESS ON` select
the strict forms. A strict form is never merged, hoisted or dropped across a
world change, unlike GCC `-O2`. One shared implementation of each profile
algorithm, on the pinned APFloat adapter with its FMA, serves the C-HIR,
EvoIR and MIR interpreters. No host libm or host complex type is involved.

**R11 Native code.** EvoMIR lowers `cmul` to the inline unfused fast path, an
unordered test and a `bl.strict_fp` or `bl.fixed_fp` call of `__mul?c3` on the
slow path, and `cdiv` to a call of `__div?c3`. Darwin uses its four
compiler-rt helpers. Calls keep FPCR and the AAPCS64 clobbers. Helper names
stay below EvoIR.

**R12 Static initializers.** `+`, `-`, negation, conjugation, conversions and
mixed per-part operations are translation-time RNE operations per part.
Complex-by-complex `*` and `/` follow GCC on Linux: each part is the correctly
rounded (RNE) exact result, when all operands are finite and both exact parts
are finite and in the normal range. Any other static `*` or `/` is refused
with a named diagnostic; MPC special values and subnormal double rounding are
not reproduced. On Darwin only an exact static `*` or `/` is admitted, because
Clang folds with yet another algorithm. Runtime expressions are never folded
with a different algorithm. EvoCC accepts the two static forms GCC rejects,
because C allows them.

**R13 ABI.** A complex type is an AAPCS64 HFA of two parts as argument,
result, variadic argument and record member. The existing `hfa=2xT` record
path, stack packing and `va_arg.byval` carry it. Darwin variadic arguments use
the Apple 8-byte slot rule. No scalarized private ABI is introduced.

**R14 Options and pragmas.** `-fcx-limited-range`, `-fcx-fortran-rules`,
`-fcomplex-arithmetic=*` and `-ffast-math` stay refused. The `-fno-` forms may
later become equivalent defaults. `#pragma STDC CX_LIMITED_RANGE` is accepted
and has no effect, as in GCC; C11 7.3.4 makes ON a permission only. Darwin
differs from Apple clang here; this is recorded, not hidden.

**R15 Claims.** `__GCC_IEC_559_COMPLEX` stays 0. Neither IEC 60559 complex
macro is defined. Interpreter-native equality is claimed only with the
qualified helper binaries: the `libgcc.a` hash above and the macOS build.

Rejected alternatives: the plain libgcc source as the model (it misses the
qualified binary in 15,789 of 200,000 RNE divisions); one algorithm for both
targets (it breaks Darwin results and flags, such as the 2^127 case); an
EvoCC-owned inline division (exact, but it leaves GCC's call shape; keep it for
a later freestanding mode); MPC folding of runtime expressions (the value would
depend on constant propagation); complex as a two-lane vector (HVA, not HFA,
and other mixed-operand rules); an Annex G claim (EvoCC makes no Annex F claim).

## 5. Implementation scope

### 5.1 Compiler

1. `crates/evocc-lex/src/lib.rs` (integer imaginary suffix near line 478):
   floating imaginary constants.
2. `crates/evocc-parser/src/complex.rs`, `lib.rs`: floating complex in
   `complex_expression_type`, `complex_component` (reuse the address
   projection) and `complex_binary` (`E2948` stays for integer division);
   mixed-operand typing, `__builtin_complex`, `creal`/`cimag`/`conj`
   builtins, `~`, `++`/`--`, truth and `_Bool` conversion.
3. `crates/evocc-chir/src/complex.rs`, `constant.rs`, `constant/floating.rs`:
   floating parts and R12 static evaluation (exact rational rounding with a
   small in-tree big integer). `interp.rs`, `interp/complex.rs`,
   `interp/floating.rs`: complex values (today `E3007`) and strict world.
4. `crates/evocc-types/src/floating.rs`: one implementation of MULC3, libgcc
   DIVC3 (sc, dc, tc) and compiler-rt division, returning bits and flags.
5. `crates/evocc-evoir` (`lib.rs`, `floating.rs`, `strict_floating.rs`,
   `text.rs`, `text_parse.rs`): the four instructions, validation, text,
   rejection rules and proof identity; FP stays out of EqSat.
6. `crates/evocc-lower/src/complex.rs`, `lib.rs`: scalar pairs, two-part memory
   access, `E3050` instead of the generic `E3101`, calls through the HFA path;
   `crates/evocc-chir/src/lib.rs` `homogeneous_floating_aggregate` counts a
   complex type as two parts.
7. `crates/evocc-mir/src/floating/helpers.rs` (`qualified`: ten helpers, four
   arguments, two-register result), `lower.rs`, `lower/quad.rs`,
   `interp/hosted_fp.rs`, `isa/aarch64/abi.rs`, `emit.rs`.
8. Specifications: new EvoIR 3.13.5; EvoIR 10.1.7 and 10.1.14; EvoMIR 2.6;
   `spec/diagnostics.md`; `spec/pins.json`.

### 5.2 Runtime helpers

| Target | Helpers | Source |
| --- | --- | --- |
| Linux | `__mulsc3`, `__muldc3`, `__multc3`, `__divsc3`, `__divdc3`, `__divtc3` | `libgcc.a` (GCC runtime library exception); the tc helpers call `__multf3`, `__addtf3`, `__subtf3`, `__divtf3`, `__eqtf2`, `__letf2`, `__lttf2`, `__getf2`, `__gttf2`, `__unordtf2`, `__floatsitf` |
| Darwin | `__mulsc3`, `__muldc3`, `__divsc3`, `__divdc3` | libSystem compiler-rt |

The helpers are a native runtime dependency, as with GCC. `-ffreestanding`
does not remove it; musl links `-lgcc`. (I) Kernel builds do not use complex.

### 5.3 Tests that meet the TODO exit

1. `l04-complex-ops.c`: all 30 builds at `-O0`/`-O2`, both interpreters and
   native Linux; Darwin native for KIND=32 and 64.
2. `tests/fixtures/floating-complex-boundaries.c`: EvoCC bits and flags equal
   the recorded references in all 15 builds.
3. A differential variant of `l04-helper-model.c` that uses `*` and `/`
   operators: 0 value, bit and flag mismatches against libgcc on the grid and
   the 4 × 200,000 set, in both interpreters and native code.
4. `l04-abi-worker.c` and `l04-abi-driver.c` with EvoCC on either side against
   GCC and Clang at `-O0` and `-O2`; on Darwin against Apple clang.
5. Two identical divisions around `fesetround` give two results everywhere.
6. Negatives: `_Imaginary`, `_Float16 _Complex`, mismatched or integer
   `__builtin_complex` operands, the refused options, a refused static
   quotient, Darwin `long double _Complex`.
7. IR fidelity: a text round trip of the four instructions; the validator and
   proof checker reject one-field mutations of format, operation, world order
   and result arity; MIR rejects a wrong helper signature.
8. Integer complex tests and `E2948` stay unchanged.

### 5.4 Original programs to replay

- musl 1.2.6 through `tools/ladder/musl.py`: the 68 units `__cexp`, `__cexpf`,
  `cabs*`, `cacos*`, `cacosh*`, `carg*`, `casin*`, `casinh*`, `catan*`,
  `catanh*`, `ccos*`, `ccosh*`, `cexp*`, `cimag*`, `clog*`, `conj*`, `cpow*`,
  `cproj*`, `creal*`, `csin*`, `csinh*`, `csqrt*`, `ctan*`, `ctanh*`. All
  1,339 units must assemble with symbol parity, and two libc-test trials must
  keep exactly the 17 reference failures.
- GCC torture (`78d4ac73`): 20020227-1, 20020411-1, 20030910-1, 20050121-1,
  20070614-1, 960512-1, complex-1, complex-2, complex-4, complex-5, complex-6,
  complex-7, pr38969, pr42248, pr49644.
- LLVM test-suite (`2919e551`): `Benchmarks/Misc/mandel.c`, `mandel-2.c`, and
  the `gcc-c-torture/execute` copies 20010605-2, 20020411-1, 20030910-1,
  20050121-1, 20070614-1, complex-1, complex-2, complex-5, complex-6,
  complex-7, pr38969, pr42248, pr49644.

## 6. Size, dependencies and risks

**Size: large.** Phases: A lexer, parser and typing (medium); B values in both
interpreters, lowering and memory (medium); C HFA ABI (small); D multiplication
(medium); E division models and the qualification harness (medium); F static
correct rounding (small to medium); G Darwin models (medium). A–D unblock all
68 musl units and the multiply originals. E unblocks `complex-5` and the
division tests.

**Dependencies.** L02 supplies f128 parts and quad helpers; the strict
environment work supplies the world model. A new Darwin `long double` layout
item gates Darwin `long double _Complex` only. L07 edits the same
`complex_binary`, so the lanes must coordinate. L03 and L05 are independent.
T03 gains the new mutation cases.

**Risks.**

1. The exact model holds only for the qualified `libgcc.a`. Another build or
   GCC release can give other bits. (I) GCC commit `54f0224d` (April 2021,
   "Practical improvement to libgcc complex divide") changed DIVC3, so
   releases before GCC 12 differ.
2. Darwin helpers belong to the OS. An update can change them.
3. EvoCC differs from GCC `-O2` by design: it does not contract the multiply,
   fold runtime constants with MPC, or reuse results across a rounding change.
4. Exact NaN bits need operand-order fidelity. A Clang-built model already
   misses 4,491 grid cases.
5. musl reads complex values through unions. If the memory model refuses that
   transport, the refusals move instead of disappearing.

## 7. Open points

1. Darwin division flags. Values and bits equal the fused compiler-rt source,
   but 1,488 (sc) and 1,836–4,775 (dc) grid flag sets differ from the model. The
   missing fact is the macOS 26.6 instruction sequence of `__divsc3` and
   `__divdc3`. Minimal experiment: disassemble them from the dyld shared cache
   (an lldb attempt here did not complete) and rebuild the model. Until then,
   Darwin interpreters should refuse a reached complex division; native code
   follows Apple clang.
2. The binary128 special-value grid for `__divtc3` and `__multc3` was NOT RUN.
   Random finite operands match in all four modes.
3. R12 is a lead choice. The alternative is to refuse every inexact static
   `*` and `/`. No affected original needs either.
