# K07: nonliteral `__builtin_strlen` as an integer constant expression

This is a decision packet for the lead. It was prepared on 2026-10-13. It is a
proposal. It does not approve a rule. The TODO cites "§10.1.18"; at HEAD the
`__builtin_strlen` text is in Section 10.1.17 and the ICE list is in 10.1.19.

## Subject

- Compiler: `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (546fe46a26e8
  2026-10-13)`, SHA-256
  `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04`.
- Specifications: `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md`
  Revision 1.0.1 at HEAD `546fe46a`.
- Linux references: GCC 14.2.0-19 and Clang 19.1.7 (`clang-19`) in
  `evo-nexlab:sealed`, image
  `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`,
  glibc 2.41.
- Darwin reference: host Apple clang 21.0.0 (clang-2100.1.1.101),
  arm64-apple-darwin25.6.0.
- Original: `lib/tests/fortify_kunit.c` (local copy SHA-256
  `484fa5e1997fed431d634a0580b34b2a8828df228d8f128ee947bc2a3944cbb0`),
  lines 95 and 101 of `fortify_test_known_sizes`.

## 1. Question

Which operands make `__builtin_strlen(e)` an integer constant expression (ICE)
of type `size_t`, besides a bare string literal? The rule must answer:

1. Which objects qualify: storage duration, scope, linkage, qualifiers,
   initializer visibility, weak or alias binding.
2. Which designator forms qualify: decay, constant offsets, subscripts,
   members, casts, conditionals, comma, statement expressions.
3. How the value is computed: embedded NUL, missing NUL, one-past positions.
4. How at most one pointer object is read, so that no cycle can arise.
5. What happens to calls and effects: a user-defined `strlen`, operands with
   effects, and unevaluated contexts.

The FORTIFY test needs `static const char array_of_10[]` and
`static const char *const unchanging_12` to be ICEs. It needs a mutable static
pointer, a mutable static array, an automatic array and `test->name` to stay
non-ICE. The kernel reads the classification through `__is_constexpr`, a
null-pointer-constant test inside `sizeof`.

## 2. Evidence

### 2.1 C standard

- C11 6.6p3 (N1570): constant expressions "shall not contain assignment,
  increment, decrement, function-call, or comma operators, except when they
  are contained within a subexpression that is not evaluated."
- C11 6.6p6: the operands of an ICE are integer, enumeration and character
  constants, `sizeof` and `_Alignof` expressions, and floating constants that
  are cast operands. A call is not an ICE operand.
- C11 6.6p10: "An implementation may accept other forms of constant
  expressions."
- C11 6.3.2.3p3: a null pointer constant is an ICE with value 0, or such an
  expression cast to `void *`. The kernel's `__is_constexpr` depends on it.
- C23 6.6p14 (as quoted in WG14 issue 1081): an implementation may accept
  extended constant expressions. "It is implementation-defined whether
  extended constant expressions are usable in the same manner as the constant
  expressions defined in this document, including whether or not extended
  integer constant expressions are considered to be integer constant
  expressions." Implementation-defined behavior must be documented.
- C11 7.1.3p2: a program that defines a reserved identifier with external
  linkage, such as `strlen`, has undefined behavior. EvoCC already gives such
  a definition a project meaning: it is called for a nonliteral operand.
- C11 7.24.6.3: `strlen` counts the characters before the terminating null
  character. A read past the array without a NUL is undefined (6.5.6p8).

Conclusion: the standard permits the extension and requires no particular
answer. Under C23 the choice is implementation-defined and must be written
down.

### 2.2 GCC 14

Measured facts are in 2.5. The mechanism is in the GCC 14.2 sources
(`releases/gcc-14.2.0`):

- `gcc/c/c-fold.cc`, `c_fully_fold_internal`: a variable is replaced by its
  initializer only `if (VAR_P (expr) && !lval && (optimize || in_init))`. Array
  and BLKmode values are never replaced. `decl_constant_value` returns the
  declaration unchanged outside a function (`gcc/c/c-typeck.cc`). So the
  const-pointer form folds only with optimization and only inside a function.
- `gcc/varpool.cc`, `ctor_for_folding`: an automatic read-only variable with
  an initializer is foldable "during FE folding". A static or external
  variable needs a varpool node that is usable for folding. Volatile objects
  are never folded.
- `__builtin_strlen` then folds through `c_strlen` on the known initializer.

GCC therefore has no single source-level rule. The const-pointer answer depends
on `-O` level and on scope. Block-scope static arrays do not fold, but
block-scope static pointers do at `-O2`.

### 2.3 Clang

- Clang documents constant evaluation for the builtin forms of `strlen` and
  other string functions ("String builtins", Clang 19.1.0
  LanguageExtensions).
- Clang 19 and Apple clang give identical masks at `-O0` and `-O2`. Clang
  admits every const object whose initializer it can evaluate, including
  block-scope statics, automatic const objects, elements of const pointer
  arrays, pointer members of const records and `__func__`.
- With `-pedantic`, Clang accepts these forms in `_Static_assert`, enumerators
  and array bounds but warns "expression is not an integer constant
  expression; folding it to a constant is a GNU extension"
  (`-Wgnu-folding-constant`). GCC 14 gives no such warning for the array form.

### 2.4 Target ABI

There is no ABI question. The rule changes only source classification and
constant values. AAPCS64 and the Apple arm64 notes do not apply. One rule
serves both targets, because Apple clang and Clang 19 agree.

### 2.5 Probes

Probe sources (new):

- `tests/fixtures/rules-20261013/k07-strlen-ice-matrix.c` (SHA-256
  `a5cd91fb…a0cd5`): `__is_constexpr` masks, CASE=0..4, eight forms each.
- `tests/fixtures/rules-20261013/k07-strlen-ice-contexts.c` (SHA-256
  `94732c7a…e5ae1`): required-ICE contexts and values, CASE=0..8.
- `tests/fixtures/rules-20261013/k07-strlen-call-effects.c` (SHA-256
  `1dcab6c8…0ab1`): a user-defined `strlen` with a call counter, and the plain
  `strlen` spelling.

Commands (`E=.build/frozen/366fd796/evocc-darwin`):

```sh
L="--target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu -std=gnu11"
$E $L -O0|-O2 -DCASE=N --interpret-c-hir FILE.c
$E $L -O0|-O2 -DCASE=N --interpret-evoir FILE.c
$E $L -O0|-O2 -DCASE=N -S -o evo.s FILE.c    # then in the container:
container run --rm --platform linux/arm64 --network none -v DIR:/work/p \
  evo-nexlab:sealed bash -c 'cd /work/p && gcc-14 evo.s -o evo && ./evo; echo $?'
# references inside the same container:
gcc-14   -std=gnu11 -O0|-O2 -w -DCASE=N FILE.c -o ref && ./ref; echo $?
clang-19 -std=gnu11 -O0|-O2 -w -DCASE=N FILE.c -o ref && ./ref; echo $?
# Darwin, on the host:
clang -std=gnu11 -O0|-O2 -w -DCASE=N FILE.c -o ref && ./ref; echo $?
$E -std=gnu11 -O0|-O2 -DCASE=N FILE.c -o evod && ./evod; echo $?
```

Classification matrix (`k07-strlen-ice-matrix.c`; Y = ICE). "Apple" is Apple
clang. The column "Rule" is the recommended EvoCC answer of Section 4.

| CASE.bit | Operand | GCC O0 | GCC O2 | Clang 19 | Apple | EvoCC now | Rule |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0.0 | `"88888888"` | Y | Y | Y | Y | Y | Y |
| 0.1 | `static const char array_of_10[]` | Y | Y | Y | Y | - | Y |
| 0.2 | `static const char *const unchanging_12` | - | Y | Y | Y | - | Y |
| 0.3 | `static const char *ptr_of_11` | - | - | - | - | - | - |
| 0.4 | automatic `char stack[80]` | - | - | - | - | - | - |
| 0.5 | `static char array_unknown[]` | - | - | - | - | - | - |
| 0.6 | `test->name` | - | - | - | - | - | - |
| 0.7 | `array_of_10 + 2` | Y | Y | Y | Y | - | Y |
| 1.0 | external `const char d[] = "abcd"` | Y | Y | Y | Y | - | Y |
| 1.1 | `extern const char d2[];` only | - | - | - | - | - | - |
| 1.2 | block-scope `static const char a[]` | - | - | Y | Y | - | Y (open point 2) |
| 1.3 | automatic `const char a[] = "abc"` | Y | Y | Y | Y | - | - (open point 1) |
| 1.4 | automatic `const char *const p = "abc"` | - | Y | Y | Y | - | - (open point 1) |
| 1.5 | `static const volatile char[]` (cast) | - | - | - | - | - | - |
| 1.6 | `static const char *const = array_of_10 + 3` | - | Y | Y | Y | - | Y |
| 1.7 | `static const char *const = "hello" + 1` | - | Y | Y | Y | - | Y |
| 2.0 | `"ab\0cd"` array (value 2) | Y | Y | Y | Y | - | Y |
| 2.1 | `const char x[3] = "abc"` (no NUL) | - | - | - | - | - | - |
| 2.2 | `array_of_10 + 10` (at NUL, value 0) | Y | Y | Y | Y | - | Y |
| 2.3 | `array_of_10 + 11` (one past) | - | - | - | - | - | - |
| 2.4 | `&array_of_10[2]` | Y | Y | Y | Y | - | Y |
| 2.5 | member array of a const record | Y | Y | Y | Y | - | Y |
| 2.6 | `names[1]`, const pointer array | - | - | Y | Y | - | - |
| 2.7 | row of `const char rows[2][4]` | Y | Y | Y | Y | - | Y |
| 3.0 | `(const char *)&static_const_int` | - | - | - | - | - | - |
| 3.1 | `(const char *)` const `unsigned char[]` | Y | Y | - | - | - | Y |
| 3.2 | pointer initialized with its own address | - | - | - | - | - | - |
| 3.3 | `1 ? array_of_10 : "xyz"` | Y | Y | Y | Y | - | Y |
| 3.4 | `(const char *)array_of_10` | Y | Y | Y | Y | - | Y |
| 3.5 | `({ array_of_10; })` | Y | Y | Y | Y | - | - |
| 3.6 | `(0, array_of_10)` | Y | Y | Y | Y | - | - |
| 3.7 | `array_of_10 + static_const_int` | - | - | - | - | - | - |
| 4.0 | `static char *const p = "abc"` | - | Y | Y | Y | - | Y |
| 4.1 | `static char *const q = array_unknown` | - | - | - | - | - | - |
| 4.2 | `static const char padded[10] = "abc"` | Y | Y | Y | Y | - | Y |
| 4.3 | `static const char t[4];` (no initializer) | - | - | - | - | - | - |
| 4.4 | definition after the use | - | - | - | - | - | - |
| 4.5 | `__attribute__((weak)) const char w[]` | - | - | - | - | - | - |
| 4.6 | `static const char *const n = 0` | - | - | - | - | - | - |
| 4.7 | `(const char *)L"ab"` pointer | - | - | - | - | - | - |

Measured exit masks: CASE=0 GCC 131/135, Clang 135/135, Apple 135/135, EvoCC
1 (both interpreters and native, both levels). CASE=1 GCC 9/217, Clang 221,
EvoCC 0. CASE=2 GCC 181/181, Clang 245, EvoCC 0. CASE=3 GCC 122/122, Clang
120, EvoCC 0. CASE=4 GCC 4/5, Clang 5, EvoCC 0. `-fno-builtin` does not
change the CASE=0 masks, and `-fPIC` does not change the CASE=1 masks. The
original 19-line fixture reproduces GCC 3/7, Clang 7/7, Apple 7/7 and EvoCC 1.

Extra forms (scratch probe, not tracked): `"abc" + 1`, `&"abc"[1]` and
`(char *)"abc"` are ICEs in GCC and Clang and are not ICEs in EvoCC now.
`__func__`, a `const char *const` member of a const record and an element of a
const pointer array are ICEs only in Clang. A block-scope static const pointer
is an ICE in GCC at `-O2` only and in Clang always. A brace-initialized
`static const char b[] = {'a','b',0}` is an ICE in both.

Required contexts (`k07-strlen-ice-contexts.c`):

| CASE | Context | GCC 14 | Clang 19 / Apple | EvoCC now |
| --- | --- | --- | --- | --- |
| 0 | file-scope `_Static_assert`, array and const pointer | array accepted; pointer: "expression in static assertion is not constant" | accepted | E2912 `static-assertion-not-constant` |
| 1 | file-scope enumerators | array accepted; pointer: "enumerator value for 'E12' is not an integer constant" | accepted | E1303 `enumerator-value-is-not-an-integer-constant` |
| 2 | file-scope array bound | accepted | accepted | E2203 `variable-length-object` |
| 3 | `case` label | accepted | accepted | E3101 `evoir-lowering-failed: case value is not an integer constant expression` |
| 4 | static initializer from the pointer | "initializer element is not constant" | accepted | E2208 `non-constant-static-initializer` |
| 5 | bit-field width | accepted | accepted | E1303 `bit-field-width-is-not-an-integer-constant` |
| 6 | `__builtin_choose_expr` condition | accepted | accepted | E2912 |
| 7 | runtime values 10, 12, 2, 0, 3 | exit 0 | exit 0 | native exit 0 (external calls) |
| 8 | `__builtin_constant_p` mask | O0 1, O2 3 | 3 | 0 |

Inside a function, GCC at `-O2` accepts the const-pointer form in
`_Static_assert`, `case` and an enumerator. At `-O0` it rejects all three.
Clang accepts them at both levels.

Call effects (`k07-strlen-call-effects.c`, CASE=0; a user `strlen` returns
1000 and counts calls; bits: a, b, c folded or called, then the count):

| Compiler | Mask | Meaning |
| --- | --- | --- |
| GCC O0, Clang O0, Apple O0 | 101 | array and const pointer folded; mutable pointer called once |
| GCC O2 | 21 | all three folded, no call: GCC proves the static mutable pointer is never written |
| Clang O2, Apple O2 | 37 | array and pointer folded; `c == 1000` but the counter reads 0 |
| EvoCC, all modes | 234 | three calls, all values 1000 |

The plain `strlen` spelling (CASE=1, declaration only) gives mask 0 in GCC and
EvoCC and 7 in Clang. With `-fno-builtin`, Clang also gives 0.

## 3. Current EvoCC behavior

Section 10.1.17 folds only a syntactic narrow string literal operand. The parser
does this in `resolve_builtin_library_call`
(`crates/evocc-parser/src/lib.rs`, the `literal_arity` branch). Any other
operand, including `"abc" + 1` and `(char *)"abc"`, becomes an ordinary
external call of `strlen`. The classification mask is 1 in both interpreters
and native code at `-O0` and `-O2`, on both targets. Required contexts report
E2912, E1303, E2203, E2208 or E3101 as in the table. A user-defined `strlen`
runs for every nonliteral operand.

## 4. Recommended rule

Principle applied: GCC 14 gives no source-level rule, because its answer
depends on `-O` level and scope. The proposal takes GCC's `-O2` in-function
classification where GCC is consistent. Where GCC depends on optimization,
scope or a varpool artifact, it takes the static-storage answer that Clang and
Apple clang give at every level. One rule holds at every optimization level,
in both interpreters, in native code and on both targets.

Proposed replacement for the `__builtin_strlen` bullet of 10.1.17:

> `__builtin_strlen(s)`: when `s` is a *constant string designator*, the call
> is an integer constant expression of type `size_t`. Its value is the number
> of bytes from the designated position to the first zero byte of the
> innermost array that contains the position. The operand is not evaluated
> and no function is called, even when the program defines `strlen`. Under
> C23 6.6p14 this is an extended integer constant expression, and this
> profile treats it as an integer constant expression in every context.
>
> A constant string designator is, after removing parentheses and casts to a
> pointer to `char`, `signed char` or `unsigned char` with any qualifiers:
>
> 1. a narrow string literal (no prefix or `u8`);
> 2. the decay of a character array that is an *immutable object*, or that
>    is a subobject of one reached by member access and constant subscripts;
> 3. the value of an *immutable character pointer object*;
> 4. `d + k`, `k + d` or `&d[k]`, with `d` of form 1 to 3 and `k` an integer
>    constant expression, when the position stays inside the innermost array
>    and does not pass its end;
> 5. `c ? x : y` with an integer-constant condition `c` and a selected arm of
>    forms 1 to 4 (the conditional address-constant rule of 10.1.18).
>
> An immutable object is an object with static storage duration (file scope
> or block-scope `static`), not thread-local, whose type is `const`-qualified
> (for an array, its element type), without `volatile` or `_Atomic`, and
> whose definition with an explicit initializer precedes the use in the
> translation unit. Every byte of the designated character array must be
> known from that initializer: a string literal or integer constant
> expressions, with the zero fill of 6.7.9.
> An immutable character pointer object is an object with static storage
> duration, not thread-local, of type `T *const` where `T` is a character
> type, without `volatile` or `_Atomic`, whose explicit initializer precedes
> the use and is an address constant of form 1, 2 or 4. Neither object may
> carry `weak` or `alias` on a declaration visible at the use.
>
> When no zero byte lies between the position and the end of the innermost
> array, `s` is not a constant string designator. Every other operand keeps
> the ordinary call to the external function `strlen` with the builtin's
> prototype: the operand is evaluated once, its effects happen, and a
> user-defined `strlen` is called. `__func__`, automatic objects, elements of
> pointer arrays, pointer members, the comma operator and statement
> expressions are not constant string designators. The plain spelling
> `strlen(...)` is unchanged: it is always an ordinary call.

Add to the list of 10.1.19: "`__builtin_strlen` of a constant string
designator (10.1.17)." Add a sentence that documents the C23 6.6p14 choice.

Consequences:

- FORTIFY lines 95 and 101 become TRUE. Lines 89, 98, 104 and 108 stay FALSE.
- Matrix masks become CASE=0 135, CASE=1 197, CASE=2 181, CASE=3 26 and
  CASE=4 5. CASE=0 and CASE=4 equal GCC `-O2` and Clang. CASE=2 equals GCC.
- No pointer object is read through another pointer object, so no cycle can
  occur. One pointer read and one array scan bound the work.
- `__builtin_constant_p(__builtin_strlen(array_of_10))` becomes 1, because the
  operand is an ICE.

Rejected alternatives:

- **Literal-only (current rule).** Keeps lines 95 and 101 failing. GCC and
  Clang both classify the array form as constant at every level.
- **Mirror GCC exactly.** It needs `-O` and scope dependent answers and a
  varpool-like rule. EvoCC classifies before optimization by design.
- **Two tiers: ICE for arrays, null-pointer-constant only for pointers.** It
  splits 10.1.19's single ICE notion and breaks the rule "an ICE with value 0
  is a null pointer constant".
- **Clang's full evaluator.** Admits automatic objects, pointer arrays,
  members and `__func__`. This widens the rule beyond the TODO's bound and
  would remove the reads that EvoCC checks for uninitialized automatic objects.
- **Fold after inlining.** Optimization-dependent constants are forbidden by
  10.1.14 and the TODO.

## 5. Implementation scope

### 5.1 Compiler

- `crates/evocc-parser/src/lib.rs`: extend the `__builtin_strlen` arm of
  `resolve_builtin_library_call` from the syntactic literal to a designator
  resolver. Keep the folded result as `size_literal`, so every later stage
  sees a literal. No change reaches C-HIR execution, EvoIR, EvoMIR or the
  proof checker.
- Same file: add a table of immutable character arrays and pointer objects,
  filled next to `remember_initialized_const_integer` (about line 2357) from
  the normalized `InitPlan`, with the same declaration-transaction undo. Check
  `weak` and `alias` from the attribute inventory of 10.1.9 at the use.
- Reuse the subobject offset logic of `__builtin_object_size`
  (`crates/evocc-chir/src/constant/object_size.rs`) to find the innermost
  array of a member or subscript path. Do not reuse the inliner's query facts.
- `spec/evoir.md` 10.1.17 and 10.1.19 wording; `spec/pins.json`, TODO and
  status rendering as AGENTS.md requires.

### 5.2 Runtime helpers

None. A non-admitted operand keeps the existing external `strlen` call.

### 5.3 Tests for the TODO exit

- `crates/evocc-parser/tests/builtin_strlen.rs`: each admitted form in
  `_Static_assert`, an enumerator, a file-scope array bound, a bit-field width,
  a `case` label, a static initializer and `__builtin_choose_expr`; each
  excluded form keeps its current diagnostic (E2912, E1303, E2203, E2208).
- Keep `non_literal_length_is_an_ordinary_call_to_visible_strlen` unchanged.
  Add: a user `strlen` with an admitted operand is not called; with a mutable
  pointer operand it is called once and its counter changes; an operand with
  an effect is evaluated once.
- Shared fixtures: the three probes above with expected EvoCC masks (135,
  197, 181, 26, 5; contexts all exit 0; effects mask 101) in both
  interpreters, native Linux and native Darwin at `-O0` and `-O2`. The
  original fixture expects 7.
- Value checks: embedded NUL (2), position at NUL (0), padded array (3),
  `unsigned char` array, record member and 2-D row.

### 5.4 Original programs to replay

- The unchanged 26-case FORTIFY KUnit suite (`lib/tests/fortify_kunit.o`) on
  the filtered FORTIFY Image route recorded in
  `docs/archive/notebook/kernel-selftest-gaps-20260914.md` ("Selected-return
  and query-context repair"). Exit: 26/26 PASS with lines 89-108 unchanged.
- Any kernel object that calls the FORTIFY `strlen()` macro of
  `include/linux/fortify-string.h:257` with a static const operand changes
  from `__fortify_strlen` to a constant. The next uniform kernel composition
  (T04) and K14 must rebuild those objects; K14 stays separate.

## 6. Size, dependencies and risks

- Size: **small** in the compiler (one parser resolver, one table, tests).
  The kernel replay is the costly part.
- Dependencies: none on other NEEDS_RULE items.
- Risks:
  - EvoCC accepts file-scope const-pointer forms in `_Static_assert`,
    enumerators and static initializers that GCC 14 rejects. Clang accepts
    them with `-Wgnu-folding-constant`. An optional warning could mark them.
  - At `-O0`, GCC classifies `unchanging_12` as non-ICE; EvoCC would not.
    Programs that select code with `__builtin_choose_expr` on that answer
    take a different branch than GCC `-O0`. The kernel never builds at `-O0`.
  - Classification differs from both references for automatic const objects,
    comma and statement expressions. The value never differs.
  - A later declaration that adds `weak` after an admitted use would leave a
    stale constant. GCC has the same hazard. A refusal for that order is
    cheap and avoids a silent mismatch.
  - FORTIFY code paths that test `__builtin_constant_p` change for static
    const operands, as they do with GCC `-O2`.

## 7. Open points

1. **Automatic const objects.** GCC (array at every level, pointer at `-O2`)
   and Clang admit them. The proposal excludes them. Reason: a jump into the
   block can skip their initialization, and EvoCC diagnoses that read today.
   The lead must accept this classification-only difference.
2. **Block-scope static arrays.** GCC excludes them; Clang includes them; GCC
   includes block-scope static pointers at `-O2`. The proposal includes all
   static-storage objects. The alternative is "file scope only".
3. **`unsigned char` arrays.** GCC admits them through a cast; Clang does
   not. The proposal follows GCC.
