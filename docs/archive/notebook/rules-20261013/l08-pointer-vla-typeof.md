# L08: evaluation of a `typeof` operand of pointer-to-VLA type

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

`__typeof__(e)` names the type of `e`. When is `e` evaluated? The agreed
cases already work: an operand of VLA type is evaluated once, and a type-name
operand captures its bounds once. The open case is an operand whose type is
variably modified but is not an array, such as `(calls++, &a)` with
`int a[n]`: GCC 14 leaves `calls` at 0 and Clang sets it to 1. The rule must
state:

1. Which expression categories are evaluated.
2. When and how often the evaluation happens, and which bounds the resulting
   type uses.
3. The scope, jump and lifetime consequences.
4. Unevaluated contexts that contain such a `typeof`.

## 2. Evidence

### 2.1 C standard

- C11 has no `typeof`. EvoCC's `gnu11` dialect takes `__typeof__` from GCC.
- C23 (N3220) 6.7.3.6p4: "If the type of the operand is a variably modified
  type, the operand is evaluated; otherwise, the operand is not evaluated."
- C11 6.7.6p3: a type is variably modified when its declarator nest contains
  a variable length array declarator. A pointer to a VLA is variably modified.
- C11 6.8.6.1p1 (constraint): a `goto` "shall not jump from outside the scope
  of an identifier having a variably modified type to inside the scope of
  that identifier." C11 6.8.4.2p2 states the same for `switch`.
- C23 6.5.4.4p2: `sizeof` evaluates its operand only when the operand has
  VLA type. A pointer operand of `sizeof` is not evaluated.

### 2.2 GCC 14

- The GCC 14.2 manual ("Typeof"): "The operand of `typeof` is evaluated for
  its side effects if and only if it is an expression of variably modified
  type or the name of such a type."
- GCC 14 does not do this for a pointer type made by `&` or by array decay.
  GCC PR c/114831, commit `9f1798c1a93257526196a3c19828e40fb28ac551`
  (Martin Uecker, 2024-05-18): "We did not evaluate expressions with variably
  modified types correctly in typeof and did not produce warnings when
  jumping over declarations using typeof. After addressof or array-to-pointer
  decay we construct new pointer types that have to be marked variably
  modified." The commit is on trunk after the GCC 14 branch point; it is not
  an ancestor of `releases/gcc-14.2.0`. Its run test `gcc.dg/pr114831-2.c`
  requires `typeof((n++, a))` with `int a[1][n]` to increment `n`.
- So the GCC 14 result is a known defect that GCC 15 repairs. It contradicts
  GCC's own documentation.

### 2.3 Clang

Clang 19 and Apple clang evaluate every operand of variably modified type and
diagnose a jump into the scope of such a declaration ("cannot jump from this
goto statement to its label"). They agree with C23 and the GCC manual in every
probe below.

### 2.4 Target ABI

No ABI question. `typeof` is resolved in the front end.

### 2.5 Probes

Probe source: `tests/fixtures/rules-20261013/l08-typeof-vm-operands.c`
(SHA-256 `50226290…a86df`). Each CASE returns `10 * side effects + size check`;
`11` means "evaluated, size correct", `1` means "not evaluated, size correct".

Commands (`E=.build/frozen/366fd796/evocc-darwin`):

```sh
L="--target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu -std=gnu11"
$E $L -O0|-O2 -DCASE=N --interpret-c-hir|--interpret-evoir FILE.c
$E $L -O0|-O2 -DCASE=N -S -o evo.s FILE.c
# container evo-nexlab:sealed, --network none:
gcc-14|clang-19 -std=gnu11 -O0|-O2 -w -DCASE=N FILE.c -o ref && ./ref; echo $?
# host:
clang -std=gnu11 -O0|-O2 -w -DCASE=N FILE.c -o ref && ./ref; echo $?
$E -std=gnu11 -O0|-O2 -DCASE=N FILE.c -o evod && ./evod; echo $?
```

| CASE | Operand | Type | GCC 14 O0/O2 | Clang 19 / Apple | EvoCC now (all modes) |
| --- | --- | --- | --- | --- | --- |
| 0 | `(calls++, &a)` | `int (*)[n]` | 1 | 11 | E2904 |
| 1 | `*(calls++, &a)` | `int [n]` | 11 | 11 | 11 |
| 2 | `({ calls++; &a; })` | `int (*)[n]` | 1 | 11 | E2904 |
| 3 | `({ calls++; a; })` | `int *` (not VM) | 1 | 1 | 1 |
| 4 | `p++`, `p` declared `int (*)[n]` | `int (*)[n]` | 11 | 11 | E2904 |
| 5 | `calls++ ? &a : &a` | `int (*)[n]` | 1 | 11 | E2904 |
| 6 | `(int (*)[n])(calls++, raw)` | `int (*)[n]` | 11 | 11 | E2904 |
| 7 | `sizeof(__typeof__((calls++, &a)))` | pointer, `sizeof` operand | 1 | 1 | E2904 |

GCC 14 evaluates the operand when the VM type comes from a declaration (CASE 4)
or a type name (CASE 6). It skips evaluation only when `&` or a decay builds
the pointer type (CASE 0, 2, 5). This is exactly the PR c/114831 defect.

Jump control (scratch probe, not tracked): `goto` past
`__typeof__(*(n++, &a)) b;` is an error in GCC, Clang and EvoCC (E2945
`jump-into-variable-type-scope`). `goto` past `__typeof__((n++, &a)) q;` is
an error in Clang, is accepted silently by GCC 14, and is E2904 in EvoCC.

The retained fixture `tests/fixtures/variable-array-typeof-profile.c`
reproduces "pointer" and "pointer-statement" counters GCC 0, Clang 1, and
"array" 1 and "decayed" 0 in both.

## 3. Current EvoCC behavior

Section 10.1.10: "An expression operand of another variably modified type,
such as a pointer to a variable-length array, is `E2904`." The parser reports
`E2904` (`not-covered: variably-modified-typeof`) at the `typeof` keyword
(`crates/evocc-parser/src/lib.rs`, the `typeof` operand path that tests
`type_is_variably_modified` and then rejects non-array types). The VLA-type
operand already uses a hidden bound capture to evaluate the operand once,
before the declaration's initializer.
`crates/evocc/tests/language_semantics.rs` asserts E2904 for
`variable-array-typeof-effects.c`, whose own checks already expect `calls == 1`.

## 4. Recommended rule

Principle applied: the standard requires the answer here. C23 6.7.3.6p4 and
the GCC manual both say "evaluate iff variably modified". GCC 14 deviates by a
known defect that GCC 15 repairs. So the rule follows the documented semantics,
Clang and GCC 15, not the GCC 14 binary.

Proposed replacement for the third and fourth sentences of 10.1.10's first
paragraph:

> The operand expression is evaluated if and only if its type is variably
> modified: a variable-length array, or any type derived from one, such as a
> pointer to a variable-length array, including pointer types formed by `&`,
> by array-to-pointer conversion, by a cast, by a conditional operator or as
> the value of a statement expression. Otherwise it is not evaluated, so
> `typeof(i++)` names `int` and leaves `i` unchanged. An evaluated operand runs
> exactly once, when the declaration, typedef or cast that contains the
> `typeof` executes, before any initializer of that declaration, and its value
> is discarded. Its operand-local declarations keep their own scope. The
> resulting type keeps the bound captures of the operand's type; a cast or
> compound literal inside the operand captures its bounds during that
> evaluation. A `typeof` that appears inside an operand that is not evaluated
> (for example a `sizeof` whose operand type is not a variable-length array,
> or an unselected `__builtin_choose_expr` arm) is not evaluated.
> `__typeof_unqual__` follows the same rule. An identifier declared with such
> a type has a variably modified type, so a jump into its scope is `E2945`.

`E2904` (`not-covered: variably-modified-typeof`) is withdrawn for expression
operands. Keep it, if still needed, only for the abstract-declarator site that
uses the same message.

Rejected alternatives:

- **Follow GCC 14 (no evaluation for `&` and decay forms).** It violates C23
  6.7.3.6p4 and the GCC manual, depends on how the pointer type was formed,
  and also drops the required jump diagnostic. GCC itself classifies it as a
  defect.
- **Keep E2904.** The TODO asks for a decision, and the evidence settles it.
- **Evaluate every `typeof` operand.** It contradicts every reference for
  non-VM operands (`typeof(i++)`).

## 5. Implementation scope

### 5.1 Compiler

- `crates/evocc-parser/src/lib.rs`, the `typeof` expression-operand path:
  remove the early `E2904` return for non-array variably modified types and
  route them through the existing "evaluated" branch (hidden capture
  sequencing before the declaration). `type_is_variably_modified` already
  marks the `&`/decay pointer types, because the current refusal fires for
  them.
- Same file: confirm that declarations whose type comes from such a `typeof`
  register as variably modified for the E2945 jump and `switch` checks.
- C-HIR execution, EvoIR lowering and EvoMIR need no new construct: the
  evaluated operand is an ordinary expression statement sequenced by the
  existing hidden capture.
- `spec/evoir.md` 10.1.10 wording; pins; TODO.

### 5.2 Runtime helpers

None.

### 5.3 Tests for the TODO exit

- `l08-typeof-vm-operands.c`: expected `11` for CASE 0, 1, 2, 4, 5, 6 and `1`
  for CASE 3 and 7, in both interpreters and native code on both targets at
  `-O0` and `-O2`.
- Convert `variable-array-typeof-effects.c` in
  `crates/evocc/tests/language_semantics.rs` from the E2904 assertion to an
  execution check; its source already expects `calls == 1`.
- `crates/evocc-parser/tests/gnu_typeof.rs`: replace the E2904 case with
  positive cases, plus `__typeof_unqual__`.
- Scope and jump controls: `goto` and `switch` into the scope of
  `__typeof__((n++, &a)) q;` report E2945; a typedef with such a `typeof`
  evaluates once when reached, not at each use.
- Side-effect and lifetime controls: the operand runs once per execution of
  the declaration inside a loop; a VLA that has ended its lifetime is not
  touched by a later `typeof` of a pointer to it (only the pointer value is
  computed); `sizeof` of a pointer-typed `typeof` stays unevaluated.

### 5.4 Original programs to replay

- No recorded original refuses with E2904; a search of `.evidence/ladder` and
  `.evidence/bench` found none.
- Replay the VLA originals that already pass, to show no regression: torture
  `20040811-1`, `20221006-1`, `920721-2`, `920929-1`, `pr43220`,
  `vla-dealloc-1` and c-testsuite `00207`
  (`tests/fixtures/language-issue-reconciliation.out`).
- GCC's own `gcc.dg/pr114831-2.c` and `gnu23-varmod-2.c` express the target
  semantics but need `-std=c23`/`gnu23`, which EvoCC does not admit. They are
  external evidence, not replay subjects.

## 6. Size, dependencies and risks

- Size: **small**. One refusal becomes the existing evaluated path.
- Dependencies: none.
- Risks:
  - EvoCC executes side effects that GCC 14 skips. A program that relies on the
    GCC 14 defect changes behavior; the kernel avoids VLAs (`-Wvla`), so the
    exposure is low.
  - EvoCC rejects jumps that GCC 14 accepts (E2945). This is a C constraint
    violation, so the rejection is required.

## 7. Open points

None. The standard, the GCC manual, the GCC 15 fix and Clang agree. Only the
GCC 14 binary differs.
