# L11a: General `_Atomic` objects and updates

This is a decision packet for the lead. It was prepared on 2026-10-13. It is a
proposal. It does not approve a rule and it changes no specification text.
Every result below was measured for this packet unless a line names an older
record. Section 7 lists the points that the evidence does not settle. L11b
covers the GNU builtins that GCC's `<stdatomic.h>` needs; L11c covers records
and 16-byte objects.

## Subject

| Item | Identity |
| --- | --- |
| Compiler | `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (366fd796613e 2026-10-13)`, SHA-256 `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04` |
| Specifications | `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md` Revision 1.0.1 at HEAD `366fd796` |
| Linux references | GCC 14.2.0-19 (`gcc-14`), Clang 19.1.7 (`clang-19`), libatomic1 14.2.0-19, glibc 2.41 in `evo-nexlab:sealed`, image `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`, linux/arm64, no network |
| Darwin reference | Host Apple clang 21.0.0 (clang-2100.1.1.101), arm64-apple-darwin25.6.0 |
| Headers | Linux: the sysroot's GCC 14 `stdatomic.h`. Darwin: Xcode's `clang/21/include/stdatomic.h` |

Probe sources in `tests/fixtures/rules-20261013/` (SHA-256 prefix):

- `l11a-atomic-layout.c` (`7f77a762`): size, alignment and lock-free answers
  for 18 atomic types. `-DPRINT` prints; `-DCASE=n` returns size×32+alignment.
- `l11a-atomic-objects.c` (`31940a32`): C operators on atomic objects,
  `CASE=0..8`.
- `l11a-stdatomic.c` (`a9c76f21`): the target's `<stdatomic.h>`, with zlib's
  once pattern and TinyCC bcheck's `_Atomic int` counter.
- `l11a-atomic-negative.c` (`63d4bac7`): compile-only controls, `NEG=0..6`.
- `l11a-codegen.c` (`cadb7f48`): code-shape probe, `-S` only.
- Unchanged TinyCC `0fb54300b565` sources, fetched from the GitHub mirror:
  `tests/tests2/124_atomic_counter.c` (16 threads, its local `threads.h`) and
  `tests/tests2/136_atomic_gcc_style.c`, with their `.expect` files.

## 1. Question

What do `_Atomic` types and objects mean in EvoCC? The rule must answer:

1. Syntax and constraints: the qualifier, the `_Atomic ( type-name )`
   specifier, arrays, functions, qualified types and bit-fields.
2. Layout: size and alignment of every atomic type on each target.
3. Which C operations on an atomic object are atomic operations, with which
   order: lvalue conversion, assignment, compound assignment, `++` and `--`,
   initialization, member access, address-of and pointer conversion.
4. Per type: integers, `_Bool`, pointers, floating types, `long double`,
   records and unions, and the existing undefined-behavior obligations.
5. The `<stdatomic.h>` path on each target and what it requires.
6. Lock-free and library boundaries, `volatile`, `const` and provenance.
7. What the interpreters establish.

## 2. Evidence

### 2.1 C standard

N1570 (C11; C17 keeps the numbering) and N3220 (C23) say:

- 6.2.5p27 (C23 6.2.5p32): "The size, representation, and alignment of an
  atomic type need not be the same as those of the corresponding unqualified
  type." The phrase "qualified or unqualified type" excludes atomic types.
- 6.2.6.1p9 (same in C23): "Loads and stores of objects with atomic types are
  done with memory_order_seq_cst semantics."
- 6.3.2.1p2: lvalue conversion of an atomic lvalue gives "the non-atomic
  version of the type".
- 6.5.2.3p5 (C23 6.5.3.4p5): "Accessing a member of an atomic structure or
  union object results in undefined behavior."
- 6.5.2.4p2 (C23 6.5.3.5p2): postfix `++` on an atomic object "is a
  read-modify-write operation with memory_order_seq_cst memory order
  semantics". Prefix forms are compound assignments (6.5.3.1p2).
- 6.5.16.2p1: `+=` and `-=` admit an "atomic, qualified, or unqualified
  pointer to a complete object type". 6.5.16.2p3 (C23 6.5.17.3p3): atomic
  compound assignment is one seq_cst read-modify-write. Footnote 113 gives the
  compare-exchange loop and says floating exceptions of discarded iterations
  "should also be discarded", with `feholdexcept`, `feclearexcept` and
  `feupdateenv`. The arithmetic itself is `E1 op (E2)`, so signed overflow
  keeps its 6.5p5 meaning; only `atomic_fetch_*` wrap (7.17.7.5p3).
- 6.7.2.4p3 (C23 6.7.3.5): the specifier's type name "shall not refer to an
  array type, a function type, an atomic type, or a qualified type".
  6.7.3p3: `_Atomic` shall not modify an array or function type.
- 6.7.2.1p5: "It is implementation-defined whether atomic types are
  permitted" as bit-fields.
- 7.17.2.2: `atomic_init` "does not avoid data races". C23 removes
  `ATOMIC_VAR_INIT`; GCC's header still defines it before C23.
- 7.17.6p1: each `atomic_` type name has "the same representation and
  alignment requirements as the corresponding direct type".
- 7.17.8p2: `atomic_flag` operations "shall be lock free".
- 6.10.8.3: `__STDC_NO_ATOMICS__` signals no atomic support. GCC, Clang and
  EvoCC do not define it. zlib tests it.

### 2.2 GCC 14

- Layout: the size never changes; sizes 1, 2, 4, 8 and 16 raise the
  alignment to the size. Measured: `_Atomic struct {int a, b;}` 8/8 (plain
  8/4), `struct {char c[2];}` 2/2, `struct {short s[3];}` 6/2, 3-byte 3/1,
  12-byte 12/4, `struct {long a, b;}` 16/16, 17-byte 17/1.
- Lock-free: `__atomic_always_lock_free` is 1 for 1, 2, 4, 8 and 0 for 16 and
  odd sizes. The run-time `__atomic_is_lock_free` answers 1 for the 3, 5 and
  6-byte objects at their addresses and 0 for 12 and 16.
- Code (`-O2`): read LDAR, write STLR. `+=` and `x++` call
  `__aarch64_ldadd4_acq_rel`. `*=` and `<<=` are LDAR plus a
  `__aarch64_cas4_acq_rel` loop. `p += 2` on `int *_Atomic` calls
  `__aarch64_ldadd8_acq_rel` with delta 8, so it scales. `_Bool` `b++` is
  LDARB plus a `cas1` loop. `_Atomic double +=` reads FPCR and FPSR, runs a
  `cas8` loop, restores FPSR and calls libatomic `__atomic_feraiseexcept`.
  `_Atomic long double +=` calls `__atomic_load_16`, `__addtf3`,
  `__atomic_compare_exchange_16` and `__atomic_feraiseexcept`.
- GCC's `<stdatomic.h>` maps `atomic_load`, `atomic_store`,
  `atomic_exchange` and `atomic_compare_exchange_*` to the generic
  `__atomic_load` (and so on) with `__auto_type` and
  `__typeof__ ((void)0, *ptr)` temporaries; `atomic_fetch_*` to
  `__atomic_fetch_*` on the `_Atomic` object itself; `atomic_flag` to
  `_Atomic struct { _Bool __val; }` with `__atomic_test_and_set` and
  `__atomic_clear`; `atomic_init` to a relaxed store.
- GCC accepts GNU builtins on `_Atomic` objects (L11b `CASE=8`).
  `atomic_fetch_add(&p, 1)` on `_Atomic(int *)` moves one byte: GCC Bug 64843
  (2015) reports the same unscaled result.
- Diagnostics: member access of an atomic record is a warning; atomic array,
  qualified type, function type and bit-field are errors; dropping `_Atomic`
  in a pointer conversion is an error.

### 2.3 Clang

- Layout (Clang 19 Linux and Apple clang): sizes up to 16 round up to a power
  of two and the alignment equals that size: 3-byte 4/4, 6-byte 8/8, 5-byte
  8/8, 12-byte 16/16. 17 bytes stay 17/1. `__atomic_always_lock_free` is 1
  through 16 bytes. Apple `long double` is 8 bytes.
- Code: the same LDAR, STLR and outline helpers on Linux; inline LSE on
  Darwin (LDADDAL, CASAL, SWPALB). `_Atomic double +=` is a plain CAS loop with
  no FPCR or FPSR handling. `_Atomic long double +=` on Linux uses LDAXP/STLXP
  and `__aarch64_cas16_acq_rel`.
- Clang rejects `p += 2` and `p -= 1` on `int *_Atomic` ("invalid operands
  to binary expression ('_Atomic(int *)' and 'int')"), which 6.5.16.2p1
  allows; Apple clang accepts `p++` and `--p` (each operator compiled
  alone). Clang 19 stops at the same `+=`. It rejects a compound literal
  `(_Atomic int){8}` ("illegal initializer type"). Member access of an
  atomic record is an error (`-Watomic-access`).
- `_Atomic _Bool`: Clang 19 and Apple clang store the byte 2 after `b += 1`
  from 1, and the byte 255 after `c--` from 0. GCC stores 1 and 1. A `_Bool`
  object holds only 0 or 1, so Clang's result is wrong code.
- Clang's `<stdatomic.h>` uses `__c11_atomic_*` and `_Atomic(T)`.
  `__c11_atomic_fetch_add` scales pointer operands: 4 bytes for one `int`.

### 2.4 Target ABI

AAPCS64 2025Q4 has no clause on `_Atomic`. Apple's "Writing ARM64 code for
Apple platforms" page has none either. The layouts above are compiler
conventions. They differ between GCC and Clang for records whose size is not
a power of two, or whose alignment is below the size: llvm-project issue
#26836 ("GCC/clang C11 _Atomic incompatibility", open) and GCC PR 115954
("Alignment of _Atomic structs incompatible between GCC and LLVM"). The LLVM
issue records that Clang's rule is the Darwin convention. GCC is the Linux
system compiler, so a Linux object built by EvoCC must match GCC.

### 2.5 Probes

```sh
# Layout and lock-free answers:
gcc-14 / clang-19 / clang -std=gnu11 -O0 -DPRINT l11a-atomic-layout.c [-latomic]
$E --target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu \
   -std=gnu11 -O2 --interpret-c-hir -DCASE=$n l11a-atomic-layout.c   # n = 0..17
# Operators and headers, Linux and Darwin as in L11b section 2.5:
CC -std=gnu11 -O{0,2} -DCASE=$n l11a-atomic-objects.c -o p [-latomic] && ./p
CC -std=gnu11 -O{0,2} l11a-stdatomic.c -o p [-latomic] && ./p
CC -std=gnu11 -O2 -c -DNEG=$n l11a-atomic-negative.c
# TinyCC tests, unchanged (container):
CC -std=gnu11 -O{0,2} -pthread 124_atomic_counter.c -o t -latomic && ./t | diff - 124_atomic_counter.expect
CC -std=gnu11 -O{0,2} 136_atomic_gcc_style.c -o t && ./t | diff - 136_atomic_gcc_style.expect
```

Layout (size/alignment):

| Atomic type | GCC 14 | Clang 19 | Apple clang | EvoCC Linux | EvoCC Darwin |
| --- | --- | --- | --- | --- | --- |
| `char`, `short`, `int`, `long`, pointer, `float`, `double`, `_Bool` | natural | natural | natural | natural | natural |
| `long double` | 16/16 | 16/16 | 8/8 | 16/16 | 16/16 |
| `__int128` | 16/16 | 16/16 | 16/16 | 16/16 | not run |
| `struct {char c[2];}` | 2/2 | 2/2 | 2/2 | **2/1** | not run |
| `struct {char c[3];}` | 3/1 | 4/4 | 4/4 | 3/1 | **3/1** |
| `struct {short s[3];}` | 6/2 | 8/8 | 8/8 | 6/2 | not run |
| `struct {int a, b;}` | 8/8 | 8/8 | 8/8 | **8/4** | **8/4** |
| `struct {char c[5];}` | 5/1 | 8/8 | 8/8 | 5/1 | not run |
| `struct {int a, b, c;}` | 12/4 | 16/16 | 16/16 | 12/4 | **12/4** |
| `struct {long a, b;}` | 16/16 | 16/16 | 16/16 | **16/8** | not run |
| `struct {char c[17];}` | 17/1 | 17/1 | 17/1 | 17/1 | not run |

Bold marks a mismatch with the target's reference. EvoCC's Darwin
`long double` is 16 bytes even without `_Atomic`; Apple's is 8. That is an
existing Darwin defect outside L11a.

Operators, `l11a-atomic-objects.c` (exit status, same at `-O0` and `-O2`):

| CASE | Content | GCC 14 | Clang 19 | Apple clang | EvoCC Linux (C-HIR / EvoIR / native) |
| --- | --- | --- | --- | --- | --- |
| 0 | integer `=`, `++`, `--`, all compound operators | 0 | 0 | 0 | `E2702` at 17:10 |
| 1 | `int *_Atomic` `+=`, `++`, `--`, `-=` | 0 | compile error | compile error | `E2702` |
| 2 | `_Atomic _Bool` `=`, `+=`, `--` | 0 | 2 | 2 | `E2702` |
| 3 | `double` and `float` compound | 0 | 0 | 0 | `E2702` |
| 4 | `long double` compound | 0 | 0 | 0 | `E2702` |
| 5 | whole-record load and store | 0 | 0 | 0 | `E2702` |
| 6 | atomic parameter and return | 0 | 0 | 0 | `E3007` unsupported-interpreter-type / 0 / 0 |
| 7 | initializer, static, compound literal | 0 | compile error | compile error | `E3007` / 0 / 0 |
| 8 | `volatile` and `const` atomic | 0 | 0 | 0 | `E2702` |

EvoCC Darwin native gives `E2702` for 0-5 and 8, and exit 0 for 6 and 7.

Header and original-test paths:

| Input | GCC 14 | Clang 19 | Apple clang | EvoCC Linux | EvoCC Darwin |
| --- | --- | --- | --- | --- | --- |
| `l11a-stdatomic.c` | 0 | 0 | 0 | `E2702` (25:5); without that line `E1004` trailing-token at `_Atomic(intptr_t)` | `E1001` in Xcode `stdatomic.h:97:17` (`_Atomic(_Bool)`) |
| TinyCC `124_atomic_counter.c` | same as `.expect` | same | not run | `E2702` | not run |
| TinyCC `136_atomic_gcc_style.c` | same as `.expect` | same | not run | `W0003` | not run |

Compile-only controls, `l11a-atomic-negative.c`:

| NEG | Control | GCC 14 | Clang 19 | Apple clang | EvoCC |
| --- | --- | --- | --- | --- | --- |
| 0 | member of an atomic record | warning | error | error | **accepted** |
| 1 | `_Atomic(int[3])` | error | error | error | `E1001` |
| 2 | `_Atomic(const int)` | error | error | error | `E1001` |
| 3 | `int *` from `&atomic_int` | error | warning | warning | `E2202` |
| 4 | atomic bit-field | error | error | error | **accepted** |
| 5 | update of an `_Atomic int` array element | accepted | accepted | accepted | `E2702` |
| 6 | `_Atomic` on a function typedef | error | error | error | **accepted** |

## 3. Current EvoCC behavior

- Declarations with the `_Atomic` qualifier are accepted. The layout ignores
  `_Atomic` (`crates/evocc-types/src/lib.rs` uses `is_atomic` only for
  compatibility).
- The `_Atomic ( type-name )` specifier is not parsed: `E1001`
  expected-declaration at file scope, `E1004` trailing-token in a block.
- Reads are split. The C-HIR interpreter refuses with `E3007
  unsupported-interpreter-expression: atomic memory access`. The EvoIR path
  lowers the read to an ordinary `load` and native code to `ldr`. Measured:
  `static _Atomic int x = 3; return x;` gives `load ... : bits<32>` and
  `ldr w0, [x0, #0]`, where GCC emits LDAR. This silently drops the seq_cst
  order of 6.2.6.1p9.
- Every update (assignment, compound assignment, `++`, `--`) is `E2702`
  atomic-update-unsupported from `typed_update_target`, `dereference_place`
  and `finish_scalar_update_place` in `crates/evocc-parser/src/lib.rs`.
- Initialization is accepted as an ordinary store.
- `__typeof__((void)0, a)` keeps `_Atomic`, so a later `t = 1` is `E2702`.
  GCC's header temporaries rely on this expression being non-atomic.
- GNU builtins on an `_Atomic` pointee are `E2913`.
- Spec text: EvoIR 10.1.2 lists `_Atomic` as "Typed unsupported"; 10.1.3 says
  "`_Atomic` objects remain outside this profile (`E2702`)". The two accepted
  paths above (EvoIR reads; three constraint violations) contradict that text.

## 4. Recommended rule

> **Atomic types.** `_Atomic` is admitted as a qualifier and as the
> `_Atomic ( type-name )` specifier. The constraints of 6.7.2.4p3 and 6.7.3p3
> are front-end errors. An atomic bit-field is not permitted (the
> implementation-defined choice of 6.7.2.1p5, as in GCC and Clang).
>
> **Layout.** On `aarch64-unknown-linux-gnu` an atomic type has the size of
> its non-atomic type; when that size is 1, 2, 4, 8 or 16 its alignment is
> raised to the size. On `aarch64-apple-darwin` a type of size at most 16 has
> its size rounded up to a power of two and an alignment equal to that size;
> larger types keep their layout. The extra bytes are padding. Records,
> unions, arrays of atomic elements and calls use this layout.
>
> **Accesses.** Lvalue conversion of an atomic object is an atomic load,
> and simple assignment is an atomic store, both `seq_cst` (EvoIR 3.8
> `atomic.load` and `atomic.store`). Compound assignment, prefix and postfix
> `++` and `--` are one `seq_cst` read-modify-write. The value is the
> non-atomic type (6.3.2.1p2); `typeof` of a non-lvalue, `__auto_type` and a
> `_Generic` controlling expression see the non-atomic type.
>
> **Operations by type.** For integers and `_Bool` the computation is the
> ordinary C computation with its obligations: signed overflow is `E3004`,
> division by zero `E3002`, an invalid shift `E3005`; a `_Bool` result is 0
> or 1. For a pointer, `+=`, `-=`, `++` and `--` scale by the pointee size and
> follow the ordinary pointer-formation rules (`E3011`). Floating compound
> assignment and floating `++`/`--` keep `E2702` until the strict
> floating-point lane specifies the hold, clear and update of footnote 113.
>
> **Width domain.** Atomic accesses of 1, 2, 4 and 8 bytes are lock-free
> instructions. Loads, stores and updates of a 16-byte atomic object (Linux
> `long double`, `__int128`, a 16-byte record) and whole-object accesses of
> an atomic record or union keep a named refusal until L11c admits them.
> Declarations, initializers, `&`, `sizeof` and the byte operations of
> `atomic_flag` (GCC's `_Atomic struct { _Bool __val; }`) are admitted now;
> zlib and TinyCC need only those. Atomic types larger than 16 bytes are
> refused when accessed.
>
> **Initialization.** A declaration initializer, a compound literal and
> `atomic_init` initialize without an atomic event. Static objects need no
> code.
>
> **Members and conversions.** `.` or `->` that designates a member of an
> atomic record or union is undefined behavior and a front-end error when the
> operand is evaluated; an unevaluated operand is allowed. A pointer
> conversion that adds or drops `_Atomic` without a cast stays an error.
>
> **Qualifiers.** `volatile _Atomic` gives one atomic event with the volatile
> flag. `const _Atomic` reads are atomic loads; writes stay errors.
> Provenance follows the existing atomic load and store rules.
>
> **Headers.** On Linux the sysroot's GCC `<stdatomic.h>` is used unchanged;
> it needs L11b's builtins on `_Atomic` pointees. On Darwin the SDK header is
> used unchanged; it needs the `__c11_atomic_*` builtins with C11 semantics
> (pointer operands scale). `__c11_atomic_*` is not admitted on Linux, where
> GCC does not provide it.

Interpreter ceiling: both interpreters execute one thread. They show values,
obligations and the order of atomic events. Inter-thread behavior is a native
claim, carried by the crossed tests in 5.3.

Rejected alternatives:

- Keep the current layout: it mismatches GCC and Apple clang for the bold
  rows above.
- Use Clang's layout on Linux, or GCC's on Darwin: each breaks the system
  compiler's ABI on that platform.
- Copy Clang for `_Bool` and floating updates: Clang stores invalid `_Bool`
  bytes and drops the floating-environment discipline.
- Copy Clang's rejection of pointer `+=`: 6.5.16.2p1 requires acceptance;
  GCC accepts and scales.
- Wrap signed overflow in compound assignment: only `atomic_fetch_*` wrap.

## 5. Implementation scope

### 5.1 Compiler

- `crates/evocc-types/src/lib.rs` (`TargetDataLayout` and record layout): the
  per-target atomic layout rule.
- `crates/evocc-parser/src/lib.rs`: the specifier syntax; constraint checks
  for arrays, functions, qualified types and bit-fields; non-atomic lvalue
  conversion types; atomic member-access errors; replace the five `E2702`
  sites with atomic read-modify-write construction. Darwin `__c11_atomic_*`
  parsing (beside L11b's work in `parse_builtin_atomic`).
- `crates/evocc-chir`: atomic loads, stores and read-modify-writes from
  operators as `CHirExpr::Atomic`; remove the `E3007` atomic refusal in
  `interp.rs` for admitted widths; keep the dynamic-order and evaluation
  rules through `inline.rs`.
- `crates/evocc-lower/src/lib.rs`: `atomic.load seq_cst` for every read of an
  admitted atomic place (several sites already treat `is_atomic` like
  `is_volatile`); compound assignment as `atomic.rmw` where the operation and
  type allow, otherwise a compare-exchange loop with an explicit retry edge.
- EvoIR and EvoMIR need no new instruction for this scope. Spec text changes
  in EvoIR 10.1.2, 10.1.3, 10.1.14 and a new atomic-object section.

### 5.2 Runtime helpers

None for 1/2/4/8 bytes. GCC's floating path calls libatomic
`__atomic_feraiseexcept`; EvoCC should not depend on it in this scope. The
16-byte path is L11c.

### 5.3 Tests for the TODO exit

1. `atomic-scope-boundaries.c` `CASE=0` passes in both interpreters and
   native at `-O0` and `-O2` (replacing the current refusal test).
2. `l11a-atomic-objects.c` `CASE=0,1,2,6,7,8` pass in all modes and on
   Darwin; `CASE=3,4,5` keep named refusals until their lanes land.
3. `l11a-atomic-layout.c` `CASE=0..17` match the GCC column on Linux and the
   Apple column on Darwin, in both interpreters.
4. `l11a-atomic-negative.c`: NEG 0, 1, 2, 3, 4 and 6 are errors; NEG 5 passes.
5. IR and assembly: reads are `atomic.load seq_cst`/LDAR, writes
   `atomic.store seq_cst`/STLR, no plain `ldr` of an atomic object.
6. `l11a-stdatomic.c` on Linux (after L11b) and on Darwin (after
   `__c11_atomic_*`).
7. Crossed native layout and operation test: an `_Atomic` integer, pointer
   and 8-byte-aligned record defined in one unit and updated from units built
   by EvoCC, GCC and Clang in both directions, with two threads.
8. Unchanged TinyCC `124_atomic_counter.c` (16 threads) and
   `136_atomic_gcc_style.c` built by EvoCC must match their `.expect` files.

### 5.4 Original programs to replay

- TinyCC `0fb54300b565` with `arm64-libtcc1-usegcc=yes`, so EvoCC compiles
  `lib/bcheck.c` (`static _Atomic int never_fatal`, `atomic_fetch_add`) and
  `lib/stdatomic.c`; then the unchanged `make test`, including the
  bounds-checking tests. This configuration is chosen to exercise the scope;
  the default build compiles both files with tcc.
- zlib 1.3.2 with `DYNAMIC_CRC_TABLE`: `z_once` uses `atomic_flag`,
  `atomic_int`, `atomic_load`, `atomic_store` and `atomic_flag_test_and_set`.
  Then the unchanged `make test`.
- SQLite: replay to show that the existing relaxed builtin path is unchanged.

## 6. Size, dependencies and risks

- Size: large. It touches the type layout, the parser, both interpreters,
  lowering with retry loops, two system headers and the specification.
- Dependencies: L11b first (builtins, compare-exchange write-back, reserved
  spellings). L11c for records and 16-byte objects. The strict floating-point
  lane for floating compound assignment.
- Interim repairs, independent of this decision: lower atomic reads to
  `atomic.load seq_cst` (or refuse them in every mode, as C-HIR does), and
  reject atomic bit-fields, `_Atomic` function types and atomic member
  access. Today these paths compile without a diagnostic.
- Risks: the layout change alters objects already built by EvoCC that contain
  promoted atomic records; they already disagree with GCC. Darwin
  `long double` is a separate open defect and blocks `_Atomic long double`
  agreement on Darwin. Removing the C-HIR refusal must keep refusals for
  widths outside the admitted domain.

## 7. Open points

- `atomic_fetch_add` and `atomic_fetch_sub` on `_Atomic(T *)` through GCC's
  header: GCC moves by bytes, Clang and the C11 operand type `ptrdiff_t` move
  by elements. The evidence does not give one rule. The recommendation is the
  L11b refusal of pointer arithmetic builtins; operators still scale.
- Floating compound assignment: GCC implements footnote 113 and Clang does
  not. The choice belongs to the strict floating-point lane; the missing fact
  is that lane's rule for flags of discarded iterations.
