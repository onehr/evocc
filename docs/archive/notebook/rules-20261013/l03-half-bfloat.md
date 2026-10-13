# L03: Binary16 and bfloat16 executable boundaries

This is a decision packet for the lead. It was prepared on 2026-10-13. It is a
proposal, not an approved rule. It changes no specification text. Every
measured result below was run for this packet unless a line names an older
record. Section 7 lists the points that the evidence does not settle.

## Subject

| Item | Identity |
| --- | --- |
| Compiler | `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (366fd796613e 2026-10-13)`, SHA-256 `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04` |
| Specifications | `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md` Revision 1.0.1 at HEAD `366fd796` |
| Linux references | GCC 14.2.0-19 (`gcc-14`) and Clang 19.1.7 (`clang-19`) in `evo-nexlab:sealed`, image `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`, glibc 2.41, no network |
| Darwin reference | Host Apple clang 21.0.0 (clang-2100.1.1.101), arm64-apple-darwin25.6.0, `libclang_rt.osx.a` |
| Standards | C23 working draft N3096 (5.2.4.2.2, 6.3.1.4, 6.3.1.5, 6.5.2.2, Annex H); AAPCS64 2026Q3 (`ARM-software/abi-aa` at `7485cc1a1240`); ACLE `main` at `284d90dc2968` |
| Original | LLVM test-suite `SingleSource/Regression/C/float16-smoke.c`, commit `2919e55176a5`, SHA-256 `baad753ee9de59d618a1a7a31f9f85c11d2d8d37eb268eec1b80db6d15a224df` (matches `tests/fixtures/llvm-floating-value-paths.out`) |

Probe sources are in `tests/fixtures/rules-20261013/`. Each file states its
cases and its exit-status meaning:

- `l03-half-conversions.c`: 22 conversion cases with exact expected bits.
- `l03-half-arithmetic.c`: 15 cases that classify the evaluation format and
  the expression typing.
- `l03-half-fenv.c`: conversions under four rounding directions with flags.
- `l03-half-abi-worker.c` and `l03-half-abi-driver.c`: crossed call shapes.
- `l03-half-varargs.c`: one anonymous half argument, callee and caller built
  separately (`-DROLE=0`, `-DROLE=1`).

`KIND=0`, `1` and `2` select `_Float16`, `__fp16` and `__bf16`. The existing
`tests/fixtures/floating-half-boundaries.c` was also rerun; its results match
`tests/fixtures/remaining-boundaries-20260927.out`.

## 1. Question

1. In which format does an operation on `_Float16`, `__fp16` or `__bf16`
   operands evaluate? Where does the value round to 16 bits?
2. Which C types do these expressions have, and which mixtures are admitted?
3. What exact bits do conversions produce: ties, subnormals, overflow, NaN
   payloads, integers of every width, binary64 and binary128 sources?
4. How do these values cross calls on Linux and on Darwin: named arguments,
   stack slots, HFAs, records and anonymous arguments?
5. Which conversions are undefined behavior, and which are defined IEEE
   results?
6. What is the first executable scope, and which named refusals remain?

## 2. Evidence

### 2.1 C standard and ACLE (documented)

- N3096 5.2.4.2.2: "assignment and cast operators yield values in the format
  of the type". `FLT_EVAL_METHOD` characterizes the evaluation format.
- N3096 H.3p2: when `FLT_EVAL_METHOD` is 0, operations **and constants** whose
  type is a strict subset of `float` evaluate to the range and precision of
  `float`. The value 16 means evaluation in `_Float16`.
- N3096 H.4.1: an integer converts to an interchange type "correctly rounded
  with exceptions raised as specified in IEC 60559". An out-of-range value of
  interchange type converts to an integer with "invalid" and an unspecified
  result.
- N3096 6.3.1.4p1, p3 and 6.3.1.5p2 state undefined behavior only for
  standard floating types.
- N3096 H.4.2: a mixture of two floating types where neither value set is a
  subset of the other has undefined behavior.
- N3096 6.5.2.2p6: the default argument promotions promote `float` to
  `double`. They do not promote `_Float16`.
- ACLE "Half-precision floating-point": `__fp16` promotes to at least `float`
  in every arithmetic operation, also when FP16 hardware exists.
  `__ARM_FP16_ARGS` admits it as an argument and result. A conversion between
  `__fp16` and `_Float16` keeps the representation. In a mixed operation the
  `_Float16` operand becomes `__fp16` and the operation is done as for two
  `__fp16` operands.
- ACLE "Half-precision brain floating-point": `__bf16` is intrinsic-only and
  needs `__ARM_BF16_FORMAT_ALTERNATIVE`. No reference defines that macro at
  the default `-march`. GCC 14 and Clang 19 still accept `__bf16` arithmetic
  as an extension (measured below; the Clang 19 manual states it).
- `__fp16` and `__bf16` are not C types. C23 does not define `__bf16`.

### 2.2 GCC 14 (measured and documented)

- Macros: `__FLT_EVAL_METHOD__` 0 and `__FLT_EVAL_METHOD_TS_18661_3__` 0 at the
  default `-march`. With `-march=armv8.2-a+fp16` both become 16.
  `__FLT16_*` and `__BFLT16_*` are defined; `__FLT16_MAX__` uses the `F16`
  suffix and `__BFLT16_MAX__` the `BF16` suffix. EvoCC predefines the same
  values for both targets.
- Evaluation: `_Float16` and `__bf16` operations evaluate in binary32 and
  round once at assignment, cast, return, prototyped argument and compound
  assignment. This holds in `-std=gnu11`, `c11` and `c2x`, with
  `-fexcess-precision=standard` and `=fast`. Only `-fexcess-precision=16`
  rounds after each operation. Integer operands and suffixed constants also
  take binary32: `float f = 0.1f16` holds `0x3dcccccd`, and
  `1.000488282181322574615478515625f16` rounds through binary32 to `0x3c00`.
- Instructions: `_Float16` conversions use `fcvt` between `h`, `s` and `d`.
  `int` and `long long` go through an exact binary64 step (`long long`
  saturates to 32 bits first). `__int128` and `long double` use libgcc. Every
  `__bf16` conversion except to integer calls a libgcc helper.
- Variadic calls: `_Float16` and `__bf16` stay unpromoted in `v` registers.
  `__fp16` becomes `double`.
- Records: a record of three `__bf16` members passes in `x0`, not as an HFA.
  Arm's ABI maintainers call this "unambiguously incorrect" (abi-aa PR #314,
  merged 2025-04-29). GCC PR 120007 "AArch64 incorrectly handles 16-bit
  HFAs" is status NEW (Bugzilla REST query on 2026-10-13; last change
  2025-04-29).
- `_Float16 + __bf16` has type `_Float16`.
- libgcc (`libgcc.a`, `libgcc_s.so.1`) exports `__extendhftf2`,
  `__trunctfhf2`, `__fix{,uns}hfti` and `__float{,un}tihf` at `GCC_11.0`;
  the nine bf16 helpers `__extendbfsf2`, `__trunc{sf,df,tf,hf}bf2` and
  `__float{di,undi,ti,unti}bf` at `GCC_13.0.0`; `__floatbitint{hf,bf}` at
  `GCC_14.0.0`. It has no `__extendhfsf2` or `__truncsfhf2`.

### 2.3 Clang (measured and documented)

- Clang 19 on Linux rounds `_Float16` after each operation. It still reports
  `__FLT_EVAL_METHOD__` 0 and no TS macro. Under N3096 H.3 that value means
  binary32 evaluation, so the macro does not describe the arithmetic.
  `-Xclang -ffloat16-excess-precision=standard` changes nothing on AArch64.
- Clang 19 evaluates `__bf16` chains in binary32, but it rounds a product to
  `__bf16` before a comparison (case 2 below). It rejects the `bf16`
  constant suffix.
- Clang `__bf16` conversions are inline integer sequences. They round to
  nearest under every rounding mode and raise no inexact, overflow or
  underflow flag. A `__bf16` signaling NaN widens to binary32 without
  quieting. `(__bf16)` of the binary64 NaN `0x7fffffffffffffff` gives
  `0x8000` (negative zero).
- Clang 19 crashes in the backend for `unsigned __int128` to `__bf16`,
  `__bf16` to and from `long double`, and `_Float16 + __bf16`. At `-O0` it
  computes `x++` for `__bf16` 256 as `0x45a0` (5120).
- Apple clang 21 has native FP16 and emits `fadd h`, `fmul h` and
  `scvtf h`, so `_Float16` rounds after each operation. It shares the Clang
  `__bf16` conversion defects (no quieting, `0x8000`, no flags, the
  `__int128` crash), but its `__bf16` `x++` result is correct.
- Apple clang fails to link `unsigned __int128` to `_Float16`:
  `libclang_rt.osx.a` has no `___floatuntihf`. Its only half helpers are
  `___extendhf{sf,df}2`, `___trunc{sf,df}hf2`, `___extendbfsf2` and
  `___trunc{sf,df}bf2`.
- Apple clang has no stable variadic convention for these types. At `-O0` a
  `_Float16` caller stores 2 bytes and the callee reads 2 bytes; at `-O2`
  both use a `double`. A `__bf16` caller always stores a `double`. The `-O0`
  `__bf16` callee reads a `double` when the `va_arg` result goes straight
  into a call, but reads 2 bytes when it initializes a local. Measured
  caller/callee optimization pairs, value 1.5, result read through a local
  and through a call:

| Type | O0/O0 | O0/O2 | O2/O0 | O2/O2 |
| --- | --- | --- | --- | --- |
| `_Float16` (`3e00`) | `3e00`, `3e00` | `0000`, `0000` | `0000`, `0000` | `3e00`, `3e00` |
| `__bf16` (`3fc0`) | `0000`, `3fc0` | `3fc0`, `3fc0` | `0000`, `3fc0` | `3fc0`, `3fc0` |

### 2.4 Target ABI (documented and measured)

- AAPCS64 C.1: a half-precision argument takes the low bits of `v[NSRN]`;
  unused register bits are unspecified. C.5: on the stack it takes an
  8-byte slot. The ABI maps `__fp16` and `__bf16` to one fundamental type,
  "half precision".
- AAPCS64 "Homogeneous Aggregates": "the different half-precision floating
  point formats share a single Fundamental Data Type". A record of up to four
  `__bf16` members, or of mixed `__fp16` and `__bf16` members, is an HFA.
- Apple's "Writing ARM64 code for Apple platforms": `__fp16` is IEEE; stack
  arguments pack by natural alignment; every variadic argument takes 8-byte
  stack slots. The page names no rule for `_Float16` or `__bf16` variadic
  arguments. Measured: Apple clang reads the tenth half argument at `[sp, #2]`.

### 2.5 Probes and results

Linux commands run inside `container run --rm --platform linux/arm64
--network none -v DIR:/work/p evo-nexlab:sealed`:

```text
{gcc-14,clang-19} -std=gnu11 -O{0,2} -w -ffp-contract=off -DKIND=k -DCASE=c -DPRINT FILE -o t -lm && ./t
clang -std=gnu11 -O{0,2} -w -ffp-contract=off -DKIND=k -DCASE=c -DPRINT FILE -o t && ./t     # host
{gcc-14,clang-19} -std=gnu11 -O{0,2} -frounding-math -ffp-contract=off -DKIND=k l03-half-fenv.c -o t -lm
CCw -std=gnu11 -O{0,2} -DKIND=k [-DNO_HFA] -c l03-half-abi-worker.c; CCd ... -DPRINT -c l03-half-abi-driver.c; gcc-14 *.o
```

Conversions (`l03-half-conversions.c`, 22 cases x 3 kinds x O0/O2, expected
values are the single-rounding IEEE bits):

| Compiler | Match | Mismatches |
| --- | --- | --- |
| GCC 14 | 132/132 | none |
| Clang 19 | 124/132 | bf16 case 13 (`7f810000`, not quieted), case 17 (`8000`), cases 18 and 21 crash |
| Apple clang | 118/132 | `_Float16` case 18 link failure; case 21 on all kinds (Darwin `long double` is binary64, expected); bf16 cases 13, 17 as Clang; case 18 crash |

The confirmed contract cases include: binary32 ties to even (`3c00`, `3c02`;
`3f80`, `3f82`); binary64 direct rounding (`3c01`, `3f81`, where a binary32
step gives the tie); overflow (`7c00`, `7f80`) and the value below it
(`7bff`, `7f7f`); subnormal ties (`0000`, `0002`); signaling NaN to `7e00` and
`7fc0`; negative quiet NaN `ffc12345` to `fe09` and `ffc1`; `int` 2049 to
`6800`; `int` 65520 to `7c00`; `long long` 2^60+2^52+1 to `5d81`.

Evaluation (`l03-half-arithmetic.c`, classes as the file defines; identical
at O0 and O2 unless stated):

Each cell lists `_Float16` / `__fp16` / `__bf16`; `-` means not applicable.

| Case | GCC 14 | Clang 19 | Apple clang |
| --- | --- | --- | --- |
| 0 `a*b+c` | 2 / 2 / 2 | 1 / 2 / 2 | 1 / 2 / 2 |
| 1 `(max*2)/2` | 2 / 2 / 1 | 1 / 2 / 1 | 1 / 2 / 1 |
| 2 `a*b == d` | 2 / 2 / 2 | 1 / 2 / 1 | 1 / 2 / 1 |
| 6 `-x`, `x` signaling NaN | 2 / 2 / 2 | O0 2, O2 1 / O0 2, O2 1 / 1 | 1 / O0 2, O2 1 / 1 |
| 7 `_Float16 + __bf16` | type `_Float16` | backend crash | type `_Float16` |
| 8-11 statement, cast, compound, call | 1 / 1 / 1 | 1 / 1 / 1 | 1 / 1 / 1 |
| 12 int operand `(z+2049)-2048` | 2 / 2 / 2 | 1 / 2 / 1 | 1 / 2 / 1 |
| 13 `float f = 0.1f16` | 2 (`3dcccccd`) / - / 2 | 1 (`3dccc000`) / - / suffix rejected | 1 / - / suffix rejected |
| 14 decimal constant near a tie | 2 (`3c00`) / - / 2 (`3f80`) | 1 (`3c01`) / - / suffix rejected | 1 / - / suffix rejected |

For `__bf16` in case 1, binary32 overflows as well, so every compiler gives
infinity. Cases 3 and 4 give one result everywhere except Clang 19 `-O0`
case 4 for `__bf16`. Case 5 (typing mask) is 15 for `_Float16` and `__bf16`
and 8 for `__fp16` in every compiler: only `__fp16` expressions have type
`float`.

Rounding and flags (`l03-half-fenv.c`, four directions x six inputs):
`_Float16` conversions agree in all compilers. They honor the direction and
detect tininess before rounding (`387ff000` gives `0400` with underflow and
inexact). For `__bf16`, GCC (libgcc) matches the IEEE table, also before
rounding. Clang and Apple clang return the nearest result with no flags,
except invalid for a signaling NaN.

Crossed calls on Linux (`l03-half-abi-*.c`, 4 worker/driver compiler pairs
x O0/O2 x 3 kinds = 24 combinations): 20 combinations pass every shape
(named, mixed cursor, ten arguments with two on the stack, three-member HFA,
five-member record, mixed record, unpromoted anonymous halves). The 4
failures are the GCC/Clang and Clang/GCC `__bf16` pairs, and only the HFA
shape fails. With `-DNO_HFA` all 24 combinations pass. Reading anonymous
`_Float16` or `__bf16` arguments as `double` fails everywhere on Linux; for
`__fp16` it succeeds everywhere. Apple clang self-pairs pass every shape on
Darwin. `l03-half-varargs.c` agrees in all 32 Linux combinations (2 kinds x
4 optimization pairs x 4 compiler pairs); the Darwin results are the table
in Section 2.3.

Original: `float16-smoke.c` prints its reference output with GCC 14 and
Clang 19 at O0/O2 and with Apple clang at O0/O2.

## 3. Current EvoCC behavior

Commands, at O0 and O2 for every probe case:

```text
.build/frozen/366fd796/evocc-darwin --target=aarch64-unknown-linux-gnu \
  --sysroot=sysroot/aarch64-unknown-linux-gnu -std=gnu11 -O{0,2} -ffp-contract=off \
  -DKIND=k -DCASE=c {--interpret-c-hir | --interpret-evoir | -S -o out.s} FILE
.build/frozen/366fd796/evocc-darwin -std=gnu11 -O{0,2} -ffp-contract=off \
  -DKIND=k -DCASE=c FILE -o exe          # Darwin target
```

Declarations, `sizeof`, `_Alignof` (2/2) and pointers to half objects work.
Nothing executes:

| Form | `_Float16` | `__fp16`, `__bf16` |
| --- | --- | --- |
| Implicit conversion (`T x = 1.0f;`, `float f = x;`) | E2202 `invalid-initializer-type` (parser) | C-HIR E3007 `unsupported-interpreter-type`; EvoIR and native E3101 `object subvalue is neither an integer nor pointer` |
| Arithmetic result in an initializer | E2202 | C-HIR E3007; EvoIR and native E3101 |
| `sizeof(+x)`, `sizeof(x + x)` | E1003 `expected-expression` | C-HIR answers 2 and 2 for both types; the references answer 4 and 4 for `__fp16`, so the ACLE promotion is missing. EvoIR stops with E3101 at `T x;` |
| Explicit cast into the type | C-HIR E3007; EvoIR E3050 `not-lowered: floating-value` | same |
| Half object from a statement expression | C-HIR E3007; EvoIR E3101 (generic) | same |
| `x++`, `r *= b` | E2102 `invalid-update-operands` | E2101 `expected-modifiable-lvalue` |
| `_Float16 + __bf16` | E2202 | E2202 |
| Static initializer from a constant | E2202 (`float16-smoke.c:16:17`) | C-HIR E3007 `nonconstant expression in static integer initializer`; EvoIR E3101 |
| `f16` or `bf16` suffix, also via `__FLT16_MAX__` | E0112 `stray-token` | E0112 |
| Parameter or result | E3049 `not-lowered: floating-call-interface` | E3049 |

The E3101 rows are generic leaks, not named refusals. September 28's T01b
named only the signature path. Spec 10.1.14 lists `_Float16` and `__fp16`
among the admitted target spellings but not `__bf16`, although
`TargetBuiltinType::ArmBf16` accepts it.

## 4. Recommended rule

**One numerical model for all three spellings and both targets.** binary16
and bfloat16 are storage and interchange formats. EvoIR never performs
arithmetic in them.

1. **Evaluation.** Every arithmetic, relational, equality and unary `+`/`-`
   operator, and `++`/`--`, converts each half operand to binary32 and
   evaluates in binary32 under the admitted scalar rules. The widening is
   exact for every value except a signaling NaN, which it quiets. An integer
   operand of such an operation converts directly to binary32. A constant
   with the `f16`/`F16` or `bf16`/`BF16` suffix rounds once to binary32. The
   value rounds to the half format exactly once at assignment,
   initialization, cast, `return`, prototyped argument conversion and the
   store of a compound assignment or increment. Comparisons compare binary32
   values. This is N3096 H.3 method 0. It matches the `__FLT_EVAL_METHOD__`
   and `__FLT_EVAL_METHOD_TS_18661_3__` values that EvoCC already advertises,
   and it matches GCC 14 in every measured case except the mixture that item
   2 refuses. The signaling-NaN result of case 6 follows from it.
2. **Typing.** `_Float16` and `__bf16` expressions keep their own type
   (`sizeof(x+x)` is 2). Both rank below `float` in the usual arithmetic
   conversions. `__fp16` operands promote to `float` before every operator,
   as ACLE requires; a `__fp16` with `_Float16` mixture gives `float`. A
   `_Float16` with `__bf16` mixture is a named compile-time refusal (H.4.2
   makes the analogous mixture undefined; Clang 19 crashes; no original
   needs it).
3. **Conversions.** Each conversion rounds once, directly from the source
   value, to nearest-even in the baseline. Strict conversions use the world
   rounding direction, set the five flags and detect tininess before
   rounding. NaN conversion quiets and keeps the sign and the leading payload
   bits. Widening is exact; a signaling NaN is quieted with invalid. An
   integer or wider float beyond the format range is an IEEE overflow
   (infinity when rounding to nearest). This is defined behavior, as for the
   existing `u128` to `f32` and `f64` to `f32` conversions (both measured:
   infinity, no report). A half value converts to an integer by truncation
   under the existing E3063 obligation: NaN, infinity and out-of-range values
   are reported as undefined.
4. **Constants.** Translation-time evaluation uses the same rules: suffixed
   constants round to binary32 first; a cast or a `static` initializer then
   rounds once to the half format (`float16-smoke.c`'s `g_fp16 = 1.0`
   becomes `3c00`).
5. **Linux ABI.** Scalar halves follow AAPCS64 C.1 and C.5: low 16 bits of a
   `v` register, unspecified upper bits, 8-byte stack slots. HFAs of one to
   four `_Float16` members, or of one to four `__fp16` members, use `v`
   registers (GCC and Clang agree for three members). A record that mixes
   `_Float16` and `__fp16` members is an HFA under AAPCS64; that shape is
   NOT RUN.
   A record that AAPCS64 makes an HFA because of a `__bf16` member is a named
   E3049 refusal at an external boundary, in both directions, because GCC 14
   and AAPCS64/Clang disagree. Other records use the general composite rule.
   Anonymous `_Float16` and `__bf16` arguments stay unpromoted (N3096
   6.5.2.2p6; GCC and Clang agree); `va_arg` reads them as written. An
   anonymous `__fp16` becomes `double`.
6. **Darwin ABI.** Named scalar halves and `_Float16`/`__fp16` HFAs follow
   the Linux register rule; stack arguments stay refused (E4010). Anonymous
   `_Float16` and `__bf16` arguments, and `va_arg` of those types, are named
   refusals: Apple clang has no stable convention. An anonymous `__fp16`
   becomes `double`. `__bf16` HFAs stay refused until a Darwin rule is tested
   against Apple clang (which treats them as HFAs).
7. **Retained refusals.** Half vectors (L05) and half complex (L04);
   `-fexcess-precision=16`; half values under `-mgeneral-regs-only`; half
   `asm` operands (E3021); type-generic classification builtins on half
   operands until a use appears; the mixtures in item 2.

Rejected alternatives:

- Per-operation rounding (Clang, Apple clang). It contradicts the advertised
  `FLT_EVAL_METHOD` under H.3, differs from GCC 14 in seven measured cases,
  and needs half arithmetic in EvoIR.
- A target-dependent model (Apple behavior on Darwin). Evaluation does not
  cross the ABI, and the Darwin profile advertises the same macros.
- Following GCC 14 for `__bf16` HFAs. It violates AAPCS64 and breaks Clang
  interoperability. Following AAPCS64 breaks GCC 14 interoperability.
- Annex H unspecified results for half-to-integer overflow. EvoCC does not
  claim Annex H, and the repository already diagnoses this class as E3063.

Proposed normative text (EvoIR 3.13, new subsection "Half formats"):

> The Linux and Darwin profiles admit `f16` (IEEE binary16) and `bf16`
> (bfloat16) as storage formats. Their values are exact 16-bit
> representations. Constants, copies, loads, stores and `to_bits/from_bits`
> with `bits<16>` preserve every bit. The only numeric operations on them are
> `convert`, `from_signed`, `from_unsigned`, `to_signed`, `to_unsigned` and
> their strict forms; each rounds once from the operand value. C source
> arithmetic on `_Float16`, `__fp16` and `__bf16` operands evaluates in
> binary32 and converts at assignment, initialization, cast, return,
> argument conversion and compound-assignment store. No other operation
> accepts `f16` or `bf16`.

## 5. Implementation scope

### 5.1 Compiler

- `crates/evocc-lex`: accept `f16`, `F16`, `bf16`, `BF16` floating suffixes
  (`strip_float_suffix`, `is_c_floating_constant`).
- `crates/evocc-types`: add `FloatKind::BF16` (F16 exists); map
  `GnuFloat16`/`ArmFp16` to `F16` and `ArmBf16` to `BF16`; extend
  `floating.rs` (`with_format!`, quiet bit, payload width) with apfloat
  `Half` and `BFloat`, which the pinned `rustc_apfloat 0.2.3` provides.
- `crates/evocc-parser`: `operand_type_class` returns `Other` for
  `_Float16` today (the source of E2202/E2102/E1003); add the typing,
  promotion, suffix and mixture rules of items 1-2 and the static
  initializer path.
- `crates/evocc-chir`: constant evaluation (`constant/floating.rs` returns
  `None` for F16 today), interpreter values (`interp/floating.rs`, E3007), and
  an explicit binary32 evaluation node with rounding points, so both
  interpreters and lowering share one placement.
- `crates/evocc-lower`: `floating::ir_type` (E3050), `function_value_type`
  (E3049), the HFA classifier with a half base and the `__bf16` refusal,
  `__fp16` variadic promotion, the Darwin anonymous refusal, strict
  conversions, and replacement of the E3101 leak.
- `crates/evocc-evoir`: types `f16`/`bf16` (Section 1.1), `const.f16` and
  `const.bf16` with four hex digits, the conversion operations, `hfa=Cxf16`,
  text, parser, validator and proof-checker rules.
- `crates/evocc-mir`: `h` views of SIMD registers, `ldr h`/`str h`, `fcvt`
  between `h`, `s` and `d` (base Armv8.0; no FEAT_FP16), integer conversion
  through exact binary64 with 32-bit saturation (GCC's method; no helper on
  either target), bf16 sequences or helpers, ABI tables and variadic access.
  FPCR.AHP must be 0; the existing FPCR=0 admission already ensures it.

### 5.2 Runtime helpers

- Linux hosted libgcc: `__extendhftf2`, `__trunctfhf2` (binary128); `__truncsfbf2`,
  `__truncdfbf2`, `__trunctfbf2`, `__extendbfsf2`, `__floatdibf`,
  `__floatundibf`, `__floattibf`, `__floatuntibf`, `__trunchfbf2`. These need
  the `GCC_13.0.0` symbol version. GCC's helpers honor dynamic rounding and
  raise flags (measured), so strict mode can use them.
- Darwin: compiler-rt has only float/double to and from bf16 and half. Use
  `fcvt` and inline integer paths; do not emit `__floatuntihf`. Strict FP is
  Linux-only today, so Darwin needs no flag-exact bf16 path.
- Baseline (non-strict) bf16 may use inline sequences. They must quiet NaNs
  and must not reproduce Clang's `0x8000` NaN defect.

### 5.3 Tests that meet the TODO exit

- Both interpreters and native code at O0 and O2 on Linux: all 22 cases of
  `l03-half-conversions.c` for all kinds return 0; Darwin native returns 0
  except case 21.
- `l03-half-arithmetic.c`: class 2 for cases 0, 2, 6 and 12; class 2 for
  cases 13 and 14 with `_Float16` and `__bf16` (they have no `__fp16` form);
  class 2 for case 1 with the IEEE half kinds and 1 with `__bf16`; class 1
  for cases 3, 4 and 8-11; masks 15/8/15 for case 5; a named refusal for
  case 7.
- `l03-half-varargs.c`: EvoCC in both roles against GCC 14 and Clang 19 on
  Linux; a named refusal for both kinds on Darwin.
- `l03-half-fenv.c` with `-frounding-math`: GCC 14's table for both kinds.
- Crossed Linux calls GCC/EvoCC and Clang/EvoCC in both directions for every
  shape of `l03-half-abi-*.c`; `__bf16` with `-DNO_HFA`; a negative test for
  the refused `__bf16` HFA signature; a test with garbage upper bits in
  `v0`-`v7`. Darwin: Apple clang/EvoCC both directions for named shapes and
  refusal tests for anonymous halves.
- One-field text and proof mutations (`f16` to `bf16`, constant bits,
  conversion rounding, `hfa=` base), as T03 requires.

### 5.4 Original programs to replay

- `SingleSource/Regression/C/float16-smoke.c` (the only binary16 deferral in
  `tests/fixtures/llvm-failure-reconciliation.out`), unchanged command. It
  needs a `static` `_Float16` initializer, a named `_Float16` parameter,
  casts of binary64 constants, one addition and widening to `double`.
- The eight `floating-half-boundaries.c` commands recorded in
  `tests/fixtures/typed-call-requirements.out` (`KIND=1/2`, `CASE=1`, O0/O2,
  EvoIR and native).
- No musl, torture, application or kernel original needs scalar halves. The
  kernel's `arm_neon.h:44` `__Float16x4_t` stop belongs to L05.

## 6. Size, dependencies and risks

Size: **medium**. No half arithmetic enters EvoIR; the work is a lexer change,
typing, two storage formats, conversions in both interpreters and the
backend, and ABI classification.

Dependencies: L01 strict FP (done) for strict conversions; L02 binary128
(done on Linux) for `long double` conversions; L04 for `_Float16 _Complex`;
L05 for half vectors and NEON half intrinsics.

Risks:

- A missing or extra rounding point changes results silently. One shared
  C-HIR node and the class tests above guard placement.
- Programs tuned for Clang see different `_Float16` results (cases 0-2, 12-14).
  This is the price of the GCC and H.3 choice.
- EvoCC Darwin results differ from Apple clang for chained `_Float16`
  expressions. The ABI is unaffected.
- bf16 helpers require a GCC 13 or newer runtime.
- GCC may fix PR 120007 later; the `__bf16` HFA refusal must then be
  revisited with a pinned GCC version.

## 7. Open points

1. **Darwin evaluation.** The packet recommends the binary32 model on Darwin
   too. The lead may instead prefer Apple clang's per-operation results on
   Darwin. That choice needs half arithmetic in EvoIR and a Darwin macro
   change. Minimal experiment: none; it is a policy choice.
2. **`__bf16` HFAs.** Refusal is recommended. If the lead prefers AAPCS64
   (Clang), the missing fact is whether a released GCC fixes PR 120007.
   Minimal experiment: rerun `l03-half-abi-*.c` with that GCC.
3. **Half to integer overflow.** E3063 is recommended. Annex H instead gives
   invalid and an unspecified value. Choosing Annex H would need a defined
   target value (FCVTZS saturation) in both interpreters.
