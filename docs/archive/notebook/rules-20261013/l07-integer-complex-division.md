# L07: integer-complex division

This is a decision packet for the lead. It was prepared on 2026-10-13. It is a
proposal. It does not approve a rule.

## Subject

- Compiler: `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (546fe46a26e8
  2026-10-13)`, SHA-256
  `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04`.
- Specifications: `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md`
  Revision 1.0.1 at HEAD `546fe46a`.
- Linux references: GCC 14.2.0-19 and Clang 19.1.7 in `evo-nexlab:sealed`,
  image `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`,
  glibc 2.41.
- Darwin reference: host Apple clang 21.0.0 (clang-2100.1.1.101),
  arm64-apple-darwin25.6.0.

## 1. Question

GNU integer complex types (`_Complex int`, `_Complex unsigned`,
`_Complex short` and so on) have no ISO definition of division. EvoCC refuses
`/` and `/=` on them with E2948. The rule must state:

1. The algorithm: which component operations run, in which type and order.
2. Operand conversion: integer promotion, the common component type, a real
   operand on either side, and the conversion back for `/=`.
3. Truncation, wrap-around and undefined behavior for each step, including a
   zero divisor, the signed minimum and intermediate overflow.
4. Constant evaluation and the effect of `-fcx-limited-range` or
   `-ffast-math`.
5. Whether Linux and Darwin share one rule.

## 2. Evidence

### 2.1 C standard

- C11 6.2.5p11 defines only three complex types: `float _Complex`,
  `double _Complex` and `long double _Complex`. C17 and C23 keep this.
- So integer complex types and their division are outside ISO C. GCC 14.2
  "Complex Numbers": "GCC also supports complex integer data types which are
  not part of ISO C99."
- C11 6.5.5p5-6 gives the scalar rules that the component steps reuse:
  integer division truncates toward zero; division by zero is undefined; an
  unrepresentable quotient is undefined.

### 2.2 GCC 14

- `gcc/c-family/c-opts.cc`: C sets `flag_complex_method = 2` by default ("C99-like
  requirements for complex multiply and divide").
- `gcc/tree-complex.cc`, `expand_complex_division`: with method 2 a floating
  component type calls the libgcc helper; an integer component type falls
  through to method 1, `expand_complex_div_wide` (Smith's method), in the
  component type with truncating division.
- `gcc/fold-const.cc`, `const_binop`, uses the same algorithm for constant
  operands ("Keep this algorithm in sync with
  tree-complex.cc:expand_complex_div_wide()").
- `-fcx-limited-range` (also implied by `-ffast-math`) sets method 0 and
  selects `expand_complex_div_straight`, the textbook formula.
  `-fcx-fortran-rules` sets method 1 and keeps Smith's method.
- With integer truncation, `ratio` is 0 unless `|c| == |d|`. So GCC's
  quotient ignores the smaller divisor component: `(9+7i)/(2+i)` is
  `(9/2) + (7/2)i = 4+3i`.
- GCC UBSan (`-fsanitize=undefined`) reports nothing for a zero divisor,
  `INT_MIN / -1` or `|INT_MIN|` inside complex division. The lowered steps are
  ordinary signed tree operations, which GCC's middle end treats as not
  overflowing.

### 2.3 Clang

- `clang/lib/CodeGen/CGExprComplex.cpp` (llvmorg-19.1.7, lines 1088-1108):
  `((ac+bd)/(cc+dd)) + i((bc-ad)/(cc+dd))` with plain `mul`/`add`/`sub`
  (no `nsw`, so wrapping) and `sdiv` or `udiv`. A real operand becomes
  complex first.
- The constant evaluator uses the same formula. Clang UBSan reports nothing
  for complex division either.
- Apple clang gives the same results as Clang 19 in every probe.
- The textbook formula overflows easily: `cc+dd` can wrap to zero for a
  nonzero divisor.

### 2.4 Target ABI

Integer complex values already pass "as an ordinary composite under AAPCS64"
(Section 10.1.14 "Integer complex"), and the crossed ABI controls pass. The
division rule adds no ABI question. Apple's arm64 notes add nothing here.

### 2.5 Probes

Probe source: `tests/fixtures/rules-20261013/l07-integer-complex-division.c`
(SHA-256 `97410bcc…16d0`). `smith()` reproduces GCC's step sequence and
`straight()` the textbook formula in plain `int` code. Bit 0 means "every
quotient equals `smith()`"; bit 1 means "equals `straight()`".

Commands (`E=.build/frozen/366fd796/evocc-darwin`):

```sh
L="--target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu -std=gnu11"
$E $L -O0|-O2 -DCASE=N --interpret-c-hir|--interpret-evoir FILE.c
$E $L -O0|-O2 -DCASE=N -S -o evo.s FILE.c
# container evo-nexlab:sealed, --network none:
gcc-14|clang-19 -std=gnu11 -O0|-O2 -w -DCASE=N FILE.c -o ref && ./ref; echo $?
gcc-14 -std=gnu11 -O0|-O2 -w -fcx-limited-range|-fcx-fortran-rules -DCASE=N FILE.c ...
# host:
clang -std=gnu11 -O0|-O2 -w -DCASE=N FILE.c -o ref && ./ref; echo $?
$E -std=gnu11 -O0|-O2 -DCASE=N FILE.c -o evod
```

| CASE | Content | GCC O0/O2 | Clang 19 O0/O2 | Apple O0/O2 | EvoCC (all modes, both targets) |
| --- | --- | --- | --- | --- | --- |
| 0 | ten runtime `_Complex int` vectors (`|c|>|d|`, `|c|<|d|`, `c==d`, `c==-d`, real and imaginary divisors, signs) | 1 (Smith) | 2 (straight) | 2 | E2948 |
| 1 | constant operands `(9+7i)/(2+1i)`, `(9+7i)/(1+2i)` | 1: 4+3i, 3-4i | 2: 5+1i, 4-2i | 2 | E2948 |
| 2 | `_Complex unsigned` | 1 | 2 | 2 | E2948 |
| 3 | `_Complex short`; `sizeof(x / y) == 4` | 5 | 6 | 6 | E2948 |
| 4 | `z /= c + di` and `w /= c` | 5 (z 4+3i, w 4+3i) | 6 (z 5+1i, w 4+3i) | 6 | E2948 |
| 5 | `9 / (2+1i)` | 1: 4+0i | 2: 3-1i | 2 | E2948 |

With `-fcx-limited-range`, GCC gives 2 for CASE=0 and CASE=1 (the textbook
formula). With `-fcx-fortran-rules`, GCC gives 1.

Scratch probes (not tracked):

- `int x = 9; x /= c + d*1i;` gives 4 under GCC and 3 under Clang and Apple
  clang. A `_Complex short` target divided by a `_Complex int` value is
  computed in `int` components in both.
- With `-fsanitize=undefined`, neither reference reports a zero divisor
  (result `0+0i`), `INT_MIN / -1` (result `INT_MIN+0i`) or a divisor
  `INT_MIN+1i`. For `(INT_MIN + INT_MIN i)/(1+2i)` GCC gives
  `-1073741824-1073741824i` and Clang `-429496729-429496729i`.

Original program: GCC torture `gcc.c-torture/execute/pr104604.c` at the pinned
commit `78d4ac73dd391005b895a6148cd9831e28e1208b` (SHA-256
`b64e3f41a7ff8955913ea2ee781000135942dffab23b1baa3a2fb69652631e2b`, equal to
the ladder record) uses `_Complex unsigned t; t /= c;` with a real divisor.
GCC, Clang and Apple clang pass it at `-O0` and `-O2`. EvoCC refuses it with
E2948 in every mode. Run `20260927T084053Z` records it as refused with
disposition `not-applicable`. Both algorithms give the same answer for this
real divisor.

## 3. Current EvoCC behavior

`crates/evocc-parser/src/complex.rs`, `complex_binary`, reports `E2948`
(`integer-complex-division-oracle-conflict`) for `/`. `parse_assignment` in
`crates/evocc-parser/src/lib.rs` reports it for `/=` when either side is integer
complex. The explanation page is `crates/evocc/explain/E2948.md`.
`crates/evocc/tests/language_semantics.rs` asserts the refusal. Addition,
subtraction, multiplication, conjugation and equality work per component at
component width, with per-component overflow obligations (10.1.14).

## 4. Recommended rule

Principle applied: the standard is silent, so the Linux rule follows GCC 14.
No platform ABI or library depends on this algorithm, so the proposal uses the
same rule on Darwin and records Apple clang's different answer.

Proposed replacement for the last two sentences of 10.1.14 "Integer complex":

> Division of integer complex values uses GCC's wide algorithm in the common
> component type `U` (the usual arithmetic conversion of the component types,
> where a real integer operand is promoted and gets a zero imaginary part).
> For `x = a + bi` and `y = c + di`, with `|v|` the negation of a negative
> `v`:
>
> ```text
> if |c| < |d|:  ratio = c / d;  div = c * ratio + d
>                r = (a * ratio + b) / div;  i = (b * ratio - a) / div
> else:          ratio = d / c;  div = d * ratio + c
>                r = (b * ratio + a) / div;  i = (b - a * ratio) / div
> ```
>
> Every step is the C operation on `U`, evaluated in the order written, with
> its existing obligations. Signed overflow, including `|v|` of the minimum
> value and `MIN / -1`, is `E3004`. A zero divisor reaches `d / c` with
> `c == 0` and is `E3002`. Unsigned steps wrap. Only the selected branch
> executes. The result has type `_Complex U`. `x /= y` computes `x / y` in
> the common type and converts each component back to the type of `x`; a
> real `x` keeps the real component. When both operands are integer constant
> expressions the same steps apply with the constant arithmetic of 10.1.19,
> and a zero divisor makes the expression non-constant.

Consequences: `(9+7i)/(2+1i)` is `4+3i`; `9/(2+1i)` is `4+0i`; a real divisor
divides each component, which equals the measured GCC results.

Option interaction (depends on L04): GCC selects the textbook formula under
`-fcx-limited-range` and `-ffast-math`. The EvoCC driver now ignores both with
the warning "is not supported and has no effect". The rule must not silently
compute the default algorithm when the original command asked for the limited
range. L04 (rule R14) proposes that these options stay unsupported. If the
driver keeps accepting them with a warning, integer complex division under
either option must report a named refusal. If the driver rejects them, no
separate integer rule is needed.

Rejected alternatives:

- **Clang's textbook formula on both targets.** It contradicts GCC on Linux,
  and its `cc+dd` overflows for moderate divisors.
- **Mathematically exact truncated quotient.** It matches neither reference.
  It would invent a third behavior.
- **Target-dependent algorithms** (GCC on Linux, Clang on Darwin). It matches
  each system compiler, but one source gives different values on the two
  EvoCC targets. Kept as open point 1.
- **Keep E2948.** The torture original stays unsupported, and the TODO asks
  for a complete bounded profile.

## 5. Implementation scope

### 5.1 Compiler

- `crates/evocc-parser/src/complex.rs` (`complex_binary`) and
  `crates/evocc-parser/src/lib.rs` (`parse_assignment`): remove both E2948
  sites and build `ComplexOp::Binary(BinaryOp::Div)` with the existing
  common-type logic, which already promotes a real operand.
- `crates/evocc-chir/src/complex.rs`: add `Div` to `binary` and to
  `constant_components` (zero divisor gives `None`).
- `crates/evocc-chir/src/interp/complex.rs`: execute the steps with E3002 and
  E3004 reports.
- `crates/evocc-lower/src/complex.rs` (`complex_arithmetic`): lower to scalar
  EvoIR with a real branch and a join block. A `select` form would execute the
  unselected division and report a false E3002, so it is not allowed.
- No new EvoIR instruction, so EvoMIR, the proof checker and EqSat stay
  unchanged.
- `spec/evoir.md` 10.1.14 and the diagnostic table; withdraw or reword E2948
  and `crates/evocc/explain/E2948.md`; `spec/diagnostics.md`; pins.

### 5.2 Runtime helpers

None. GCC and Clang emit inline integer code; no libgcc or compiler-rt
helper exists for integer complex division.

### 5.3 Tests for the TODO exit

- The probe above with expected masks 1, 1, 1, 5, 5, 1 in both interpreters
  and native code on both targets, at `-O0` and `-O2`.
- Boundary tests: zero divisor (`E3002` in both interpreters), `INT_MIN / -1`
  through the real-divisor path, `|INT_MIN|` in the comparison and
  `b * ratio - a` overflow (`E3004`), unsigned wrap, `_Complex char` and
  `_Complex long` widths.
- Compound assignment: `_Complex short /= _Complex int` with narrowing, real
  target `/=` complex value, complex `/=` real.
- Constant folding: a static initializer `(9+7i)/(2+1i)`, and a zero divisor
  in a static initializer (E2208).
- Replace the E2948 test in `crates/evocc/tests/language_semantics.rs` with
  these positive tests and the option refusal.

### 5.4 Original programs to replay

- GCC torture `pr104604.c` (pinned commit above), expected to change from
  E2948/not-applicable to PASS in both interpreters and native code.
- `tests/fixtures/integer-complex-profile.c`: its divide row must print
  `4/3` like GCC.
- No other refusal with E2948 appears in the recorded torture run; a search of
  the other ladder receipts during implementation should confirm this.

## 6. Size, dependencies and risks

- Size: **small to medium**. The parser change is small; the branch lowering
  and the three execution engines are the main work.
- Dependencies: L04 for the `-fcx-limited-range`/`-ffast-math` option policy.
  None for the default rule.
- Risks:
  - GCC's algorithm is numerically poor. Users may expect the exact quotient.
    The rule reproduces GCC exactly and states the formula.
  - EvoCC diagnoses overflow and zero divisors that GCC and Clang execute
    silently (their UBSan builds report nothing). This follows the existing
    integer complex profile but can stop programs that "work" under GCC.
  - Darwin programs see GCC's values, not Apple clang's.

## 7. Open points

1. **Darwin.** One GCC-based rule (proposed) or Apple clang's textbook
   formula on Darwin. The evidence does not force either; the owner's
   principle points to one rule because no Darwin ABI or library is involved.
2. **Limited-range options.** Refuse (proposed) or implement the textbook
   formula with the same per-step obligations. This follows the L04 R14
   option decision; the two lanes also edit the same `complex_binary`
   function and must coordinate.
