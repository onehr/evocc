# L11b: Additional `__atomic` read-modify-write, exchange and compare-exchange

This is a decision packet for the lead. It was prepared on 2026-10-13. It is a
proposal. It does not approve a rule and it changes no specification text.
Every result below was measured for this packet unless a line names an older
record. Section 7 lists the points that the evidence does not settle.

## Subject

| Item | Identity |
| --- | --- |
| Compiler | `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (366fd796613e 2026-10-13)`, SHA-256 `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04` |
| Specifications | `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md` Revision 1.0.1 at HEAD `366fd796` |
| Linux references | GCC 14.2.0-19 (`gcc-14`), Clang 19.1.7 (`clang-19`), libatomic1 14.2.0-19, libgcc 14.2.0-19, glibc 2.41 in `evo-nexlab:sealed`, image `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`, linux/arm64, no network |
| Darwin reference | Host Apple clang 21.0.0 (clang-2100.1.1.101), arm64-apple-darwin25.6.0 |
| Host CPU seen by the guest | `/proc/cpuinfo` lists `atomics` (LSE) and `uscat` (LSE2) |

Probe sources in `tests/fixtures/rules-20261013/` (SHA-256 prefix):

- `l11b-rmw-cas.c` (`2c1a28cd`): nine value cases, `CASE=0..8`.
- `l11b-codegen.c` (`78a9e29b`): code-shape probe, `-S` only.
- `l11b-rmw-negative.c` (`1959ec66`): eleven compile-only controls, `NEG=0..10`.
- The existing `tests/fixtures/atomic-scope-boundaries.c` (`CASE=1`) and
  `tests/fixtures/atomic-cas-boundaries.c` (`CASE=0..2`) were rerun.

## 1. Question

Which GNU `__atomic` builtins beyond `__atomic_load_n` and `__atomic_store_n`
does EvoCC admit, and what do they mean? The rule must answer:

1. The spelling set: fetch-op and op-fetch forms, exchange, strong and weak
   compare-exchange, test-and-set, clear, the two fences, the lock-free
   queries, and the generic pointer-to-value forms on scalar objects.
2. The operand domain: widths, `_Bool`, pointers, `const`, `volatile`,
   `_Atomic` pointees, floating pointees and 16-byte integers.
3. The order domain: valid constant orders per builtin, the failure order of
   compare-exchange, and run-time orders.
4. The compare-exchange contract: the read of `*expected`, the comparison,
   the write-back on failure, spurious weak failure and the result.
5. Arithmetic: wraparound, `nand`, and pointer scaling.
6. Operand evaluation count and order.
7. Representation in EvoIR and proof certificates, and native emission.
8. What the interpreters establish, and what only native tests can show.
9. What happens to an `__atomic_*` spelling that the profile does not admit.

## 2. Evidence

### 2.1 C standard

GNU `__atomic` builtins are an extension. C defines the corresponding generic
functions in `<stdatomic.h>`. The citations are N1570 (C11; C17 keeps the
numbering) and N3220 (C23).

- C11 7.17.7.4p2 and C23 7.17.7.4p2 say the same: "The failure argument shall
  not be memory_order_release nor memory_order_acq_rel. The failure argument
  shall be no stronger than the success argument." C23 kept both sentences.
  A failed comparison "updates the value in expected"; memory is affected by
  `success` or by `failure`. 7.17.7.4p3 (NOTE 1) describes the effect with
  `memcmp` and `memcpy`.
- C11 7.17.7.4p4: a weak compare-exchange "may fail spuriously", and then it
  stores back the same contents to `expected`.
- C11 7.17.7.5p1: fetch-and-modify does not apply to `atomic_bool`. 7.17.7.5p3:
  signed arithmetic uses "silent wrap-around on overflow; there are no
  undefined results. For address types, the result may be an undefined
  address, but the operations otherwise have no undefined behavior."
- C11 7.17.1p5: "For atomic pointer types, M is ptrdiff_t."
- C11 7.1.3p1: identifiers that begin with two underscores "are always
  reserved for any use". A refusal of an unknown `__atomic_*` spelling conforms.

### 2.2 GCC 14

Documented in the GCC 14.2 manual, "Built-in Functions for Memory Model
Aware Atomic Operations"
(https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/_005f_005fatomic-Builtins.html):

- The builtins accept "any integral scalar or pointer type that is 1, 2, 4,
  or 8 bytes in length", and 16-byte integral types where `__int128` exists.
- Fetch-op operands "must not be a boolean type". Pointer operands are
  computed "as if the operands were of the uintptr_t type. That is, they are
  not scaled by the size of the type to which the pointer points."
- Run-time orders: "These built-in functions map any run-time value to
  `__ATOMIC_SEQ_CST`".
- Compare-exchange: on inequality "the current contents of `*ptr` are written
  into `*expected`". The failure order "cannot be `__ATOMIC_RELEASE` nor
  `__ATOMIC_ACQ_REL`. It also cannot be a stronger order than that specified
  by success_memorder." Many targets ignore `weak`.
- `__atomic_clear` admits only relaxed, release and seq_cst.

Measured:

- GCC 14 enables `-moutline-atomics` by default (`gcc-14 -Q --help=target`).
  At `-O2`, 1/2/4/8-byte RMW, exchange and compare-exchange call the
  `__aarch64_{ldadd,swp,cas}N_{relax,acq,rel,acq_rel}` helpers. These are
  static members of `libgcc.a` (126 `__aarch64_*` symbols; none in
  `libgcc_s.so.1`). `fetch_nand` has no LSE instruction and stays an inline
  LDXRB/STXRB loop. With `-mno-outline-atomics` every form is an inline
  exclusive loop (LDAXR/STLXR for seq_cst, LDAXR/STXR for acquire); with
  `-march=armv8.4-a` it is CASAL, CASA, LDADDAL, SWPAL or SWPALB.
- 16-byte `__int128` load, store, fetch-add and compare-exchange call
  libatomic `__atomic_load_16`, `__atomic_store_16`, `__atomic_fetch_add_16`
  and `__atomic_compare_exchange_16` at every `-march` tried. In libatomic
  14.2 these are IFUNC symbols. The resolver selects LDXP/STXP, LDP (LSE2) or
  CASP bodies; the disassembly shows no lock in those bodies. The run-time
  `__atomic_is_lock_free(16, p)` still answers 0.
- Legacy `__sync_val_compare_and_swap` on `unsigned __int128` calls
  `__aarch64_cas16_sync` from libgcc. Both GCC and Clang define
  `__GCC_HAVE_SYNC_COMPARE_AND_SWAP_16`.
- Thread fences: seq_cst gives `dmb ish`, acquire gives `dmb ishld`.
  `__atomic_signal_fence` emits no instruction.
- Order matrix (all 36 constant success/failure pairs on a 4-byte CAS):
  GCC warns `-Winvalid-memory-model` when the failure order is release or
  acq_rel, or numerically greater than the success order. That rule matches
  all 36 pairs. Every warned pair except (consume, acquire) calls
  `__aarch64_cas4_acq_rel`, the helper GCC also uses for seq_cst. The pair
  (consume, acquire) calls `__aarch64_cas4_acq`, because GCC implements
  consume as acquire. Unwarned pairs merge the two orders: success release
  with failure acquire calls the `acq_rel` helper.

### 2.3 Clang

Clang documents GNU `__atomic_*` as "GCC-compatible" and adds `__c11_atomic_*`
for `<stdatomic.h>` (Clang 19 LanguageExtensions, "__c11_atomic builtins").
Measured:

- Clang 19 on Linux also uses `+outline-atomics` and calls the same
  `__aarch64_*` helpers. 16-byte load, store and fetch-add are inline
  LDAXP/STLXP loops; 16-byte compare-exchange calls `__aarch64_cas16_acq_rel`.
- Apple clang inlines LSE: LDADDAL, CASAL, CASA, SWPAL, SWPALB, and a CASB
  loop for `fetch_nand`. 16-byte load is LDP plus DMB; store is DMB, STP,
  DMB; compare-exchange is CASPAL.
- Clang accepts `__atomic_fetch_add` on `_Bool` and `float` (GCC rejects
  both). Clang rejects a `const` pointee (GCC warns and accepts) and any
  `_Atomic` pointee (GCC accepts).
- Invalid constant orders differ in code, not only in diagnostics. For
  `__atomic_load_n(p, 3)` Clang 19 and Apple clang warn and delete the load:
  the function body is only `ret`. GCC warns and emits LDAR. Clang replaces
  an invalid failure order with relaxed and keeps the success order; GCC
  makes the whole compare-exchange seq_cst. Clang never warns about a failure
  order stronger than the success order; it merges them.
- Operand evaluation order differs (CASE 7). GCC evaluates address, value,
  order. Clang and Apple clang evaluate address, order, value. C leaves the
  order of argument evaluation unspecified.

### 2.4 Target ABI

- AAPCS64 2025Q4 (`ARM-software/abi-aa`, `aapcs64.rst`, issued 2026-01-23)
  has no clause on atomic operations or `_Atomic` types. The only occurrence
  of "atomic" is in the volatile bit-field section.
- The interoperability contract is therefore the toolchain contract.
  Exclusive-monitor loops, LSE instructions and the libgcc helpers act on the
  same coherent memory and interoperate (Arm architecture; LLVM "Atomics"
  documentation notes that the AArch64 helpers fall back to LL/SC without
  LSE). A lock-based library path is compatible only if every access to that
  object uses it.
- Apple's "Writing ARM64 code for Apple platforms" page says nothing about
  atomics. The platform behavior is Apple clang's.

### 2.5 Probes

Commands, run from a scratch copy of each probe:

```sh
# Linux references (inside evo-nexlab:sealed, --network none):
gcc-14   -std=gnu11 -O{0,2} -DCASE=$n l11b-rmw-cas.c -o p -latomic && ./p
clang-19 -std=gnu11 -O{0,2} -DCASE=$n l11b-rmw-cas.c -o p -latomic && ./p
# Darwin reference (host):
clang -std=gnu11 -O{0,2} -DCASE=$n l11b-rmw-cas.c -o p && ./p
# EvoCC, Linux target, three modes:
E=.build/frozen/366fd796/evocc-darwin
$E --target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu \
   -std=gnu11 -O{0,2} -DCASE=$n {--interpret-c-hir,--interpret-evoir,-S} l11b-rmw-cas.c
# EvoCC, Darwin target:
$E -std=gnu11 -O{0,2} -DCASE=$n l11b-rmw-cas.c -o p
# Code shape:
gcc-14 -std=gnu11 -O2 [-mno-outline-atomics | -march=armv8.4-a] -S l11b-codegen.c
clang-19 -std=gnu11 -O2 -S l11b-codegen.c ; clang -std=gnu11 -O2 -S l11b-codegen.c
# Compile-only controls: CC -std=gnu11 -O2 -c -DNEG=$n l11b-rmw-negative.c
```

`l11b-rmw-cas.c` exit status (the same at `-O0` and `-O2`):

| CASE | Content | GCC 14 | Clang 19 | Apple clang | EvoCC first diagnostic (all modes, both targets) |
| --- | --- | --- | --- | --- | --- |
| 0 | all 12 fetch-op/op-fetch forms, widths 1/2/4/8, wrap | 0 | 0 | 0 | `W0003` function has no declaration (18:9) |
| 1 | strong/weak CAS, write-back, orders (5,5) (4,2) (3,0) (0,0) | 0 | 0 | 0 | `E2931` unprototyped-call-mismatch (35:13) |
| 2 | exchange on int, pointer, `_Bool` | 0 | 0 | 0 | `E2931` (41:9) |
| 3 | `__atomic_fetch_add(&p, 4, 5)` on `int *` | 0 (unscaled) | 0 (unscaled) | 0 (unscaled) | `W0003` (46:5) |
| 4 | run-time order operand | 0 | 0 | 0 | `W0003` (52:9) |
| 5 | test-and-set, clear, both fences | 0 | 0 | 0 | `E2931` (60:14) |
| 6 | `unsigned __int128` fetch-add and CAS | 0 | 0 | 0 | `W0003` (65:9) |
| 7 | evaluation order code (0 = address, value, order) | 0 | 32 (address, order, value) | 32 | `W0003` (70:20) |
| 8 | GNU builtins on an `_Atomic int` object | 0 | compile error | compile error | `E2913` builtin-operand-not-convertible (78:28) |

`l11b-rmw-negative.c` (`rc` 0 means accepted):

| NEG | Control | GCC 14 | Clang 19 | Apple clang | EvoCC |
| --- | --- | --- | --- | --- | --- |
| 0 | fetch_add on `_Bool` | error | accepted | accepted | `W0003` |
| 1 | fetch_add on `float` | error | accepted | accepted | `W0003` |
| 2 | `__atomic_load_n` on a record | error | error | error | `E2913` |
| 3 | fetch_add on `const int` | warning, accepted | error | error | `W0003` |
| 4 | CAS failure order release | warning | warning | warning | `W0003` |
| 5 | CAS failure order acq_rel | warning | warning | warning | `W0003` |
| 6 | `__atomic_clear` with acquire | warning | accepted | warning | `W0003` |
| 7 | `__atomic_store_n` with acquire | warning | warning | warning | `E2947` |
| 8 | fetch_add on a `void *` object | accepted | accepted | accepted | `W0003` |
| 9 | order constant 6 | warning | warning | warning | `W0003` |
| 10 | CAS success relaxed, failure seq_cst | warning | accepted | accepted | `W0003` |

The reruns of `atomic-cas-boundaries.c` `CASE=0..2` give one `W0003` each.

## 3. Current EvoCC behavior

- The front end knows `__atomic_load_n`, `__atomic_store_n` and the legacy
  `__sync_*` set (`legacy_atomic` and `parse_builtin_atomic` in
  `crates/evocc-parser/src/lib.rs`). Any other `__atomic_*` spelling is an
  ordinary undeclared function name.
- In GNU11 the first symptom is `error[W0003]: function has no declaration`
  or, when two calls promote differently, `error[E2931]:
  unprototyped-call-mismatch`. The TODO's `E2001` is out of date. The CLI
  test `broader_atomic_interfaces_are_not_mistaken_for_implemented_scalar_access`
  in `crates/evocc/tests/scalar_atomic_access.rs` already expects `W0003`.
- `W0003` is a lint. With `-w`, `-Wno-implicit-function-declaration`,
  `--cap-lints=allow` or `-std=gnu89`, EvoCC emits ordinary calls. Measured:
  `atomic-scope-boundaries.c` `CASE=1` with `-w` assembles and fails to link
  (`__atomic_fetch_add` and `__atomic_compare_exchange_n` are undefined).
  `CASE=2` with `-w` links against libatomic's generic
  `__atomic_load(size_t, void *, void *, int)` with three wrong arguments and
  ends with SIGSEGV (exit 139). This is a silent wrong-code path, not a refusal.
- `__has_builtin` answers 0 for every atomic spelling, including the
  implemented `__atomic_load_n` and `__sync_fetch_and_add`. GCC and Clang
  answer 1 (Clang also for `__c11_atomic_load`).
- The predefined macros copy GCC's set, including
  `__GCC_HAVE_SYNC_COMPARE_AND_SWAP_16`, but a 16-byte `__sync` CAS is `E2913`.
- The existing EvoIR instruction already covers most of the semantics:
  `atomic.rmw.<op> <order>` with `add`, `sub`, `and`, `or`, `xor`, `nand`,
  `exchange` and `cmpxchg` on `bits<8|16|32|64>` or a pointer (exchange and
  cmpxchg only), one order, old-value result (EvoIR 3.8). EvoMIR 4.2 emits an
  exclusive loop: LDAXR or LDXR, STLXR or STXR, `dmb ish` before and after
  seq_cst, and CLREX on a failed comparison. The interpreters compare pointer
  operands by address (`MemoryState::atomic_rmw` in
  `crates/evocc-memory/src/atomic.rs`).

## 4. Recommended rule

The rule is the same on both targets unless a line says otherwise. It reuses
the existing order constants 0..5 and the existing left-to-right evaluation.

> **Additional GNU atomic builtins.** The front end resolves
> `__atomic_exchange_n`, `__atomic_compare_exchange_n`, `__atomic_fetch_OP`
> and `__atomic_OP_fetch` for OP in `add`, `sub`, `and`, `or`, `xor`, `nand`,
> `__atomic_test_and_set`, `__atomic_clear`, `__atomic_thread_fence`,
> `__atomic_signal_fence`, `__atomic_always_lock_free`,
> `__atomic_is_lock_free`, and the generic `__atomic_load`, `__atomic_store`,
> `__atomic_exchange` and `__atomic_compare_exchange` when the pointee is a
> scalar of the domain below. None is an integer constant expression except
> `__atomic_always_lock_free`.
>
> **Operand domain.** The object is a 1/2/4/8-byte integer, `_Bool`, or an
> object or function pointer, naturally aligned. Arithmetic forms require an
> integer that is not `_Bool`. Exchange, compare-exchange and the generic
> forms also admit `_Bool` and pointers. A `const` pointee, a floating
> pointee, a record or union (see L11c), a 16-byte integer and arithmetic on a
> pointer are `E2913`. An `_Atomic`-qualified pointee is admitted and means
> its non-atomic type. `volatile` keeps its existing meaning: one atomic event
> with the volatile flag.
>
> **Values.** Arithmetic wraps at the operand width. `nand` is
> `~(old & value)`. Fetch-op returns the old value; op-fetch returns the
> computed new value. Exchange returns the old value. Test-and-set stores the
> byte 1 at the first byte of the object and returns whether that byte was
> nonzero; clear stores the byte 0.
>
> **Compare-exchange.** Evaluate `ptr`, `expected`, `desired` (or the pointer
> to it), `weak`, `success`, `failure` once each, left to right. Read
> `*expected` with an ordinary load before the atomic operation. Compare the
> current value with that value: integers by value, pointers by address. On
> equality store `desired` and return 1. Otherwise store the observed value
> into `*expected` with an ordinary store and return 0. A weak request may be
> implemented as strong. The interpreters never fail spuriously.
>
> **Orders.** Fetch-op, op-fetch, exchange and test-and-set admit all six
> constant orders. Clear admits relaxed, release and seq_cst. Compare-exchange
> admits any success order and a failure order that is not release or
> acq_rel and is not numerically greater than the success order. Every other
> constant order is `E2947`. This is exactly the set on which GCC 14 does not
> warn. A non-constant order is evaluated once and the operation becomes
> seq_cst (both halves for compare-exchange), as for the existing builtins.
> The compare-exchange keeps both source orders in EvoIR. Native code uses
> acquire if either order acquires, release if the success order releases,
> and seq_cst if either is seq_cst.
>
> **Fences.** `__atomic_thread_fence(order)` is an ordered event with the
> current memory and world tokens; relaxed has no effect. Native code emits
> `dmb ishld` for consume and acquire and `dmb ish` otherwise.
> `__atomic_signal_fence(order)` orders the compiler only and emits nothing.
>
> **Lock-free queries.** `__atomic_always_lock_free(size, 0)` is 1 for sizes
> 1, 2, 4 and 8. Size 16 is 0 on Linux and 1 on Darwin. Other sizes are 0.
> `__atomic_is_lock_free` folds to 1 where the first query is 1; otherwise it
> is a call to the external `__atomic_is_lock_free` (Linux libatomic, Darwin
> libSystem), which the interpreters report as `E3039`.
>
> **Unadmitted spellings.** Every overloaded `__atomic_` and `__c11_atomic_`
> spelling without a size suffix is a builtin name. A use outside the admitted
> domain is a front-end refusal (`E2935` unsupported-library-builtin, or a
> new code), never an implicit declaration. `__atomic_is_lock_free` is also a
> library function with the libatomic prototype
> `_Bool (size_t, const volatile void *)`; a compatible declaration, alias or
> definition is allowed and an incompatible one is `E2927`. The sized library
> names (`__atomic_load_4` and so on) keep the ordinary declaration rules,
> because runtime sources define them.

Interpreter ceiling: both interpreters execute one thread. They establish
values, operand effects, the write-back and the order of atomic events. They
do not establish inter-thread ordering. Native crossed tests carry that claim.

Rejected alternatives:

- Copy GCC's warnings: accept invalid orders and strengthen them. The
  references disagree in code (Clang deletes an invalid-order load), so
  acceptance would choose one compiler's undefined-behavior outcome. EvoCC
  already refuses invalid load and store orders with `E2947`.
- Admit pointer arithmetic forms now. GCC documents unscaled byte arithmetic.
  `<stdatomic.h>` on Linux maps `atomic_fetch_add` to that builtin, so GCC
  gives a one-byte step for `_Atomic(int *)` (L11a measures 1 against Clang's
  4). C11 names `ptrdiff_t` as the operand type. Refusal avoids choosing.
- Call the `__aarch64_*` outline helpers. Inline exclusive loops already
  interoperate with them, need no new link dependency and keep EvoMIR's
  existing emission and proof path.
- Admit 16-byte operands now. GCC calls libatomic and Clang inlines. Both are
  lock-free with libatomic 14.2 (L11c crossed control), but an EvoCC lowering
  needs a 128-bit atomic instruction or a helper-call contract first.

## 5. Implementation scope

### 5.1 Compiler

- `crates/evocc-parser/src/lib.rs` (and the new `builtins.rs` that another
  lane is adding): reserve the overloaded spellings; extend
  `parse_builtin_atomic` with the new forms, the operand domain, the order
  rules and the generic pointer-to-value forms. Remove the `is_atomic`
  rejection for the `_n` forms. Update `__has_builtin` only for admitted names.
- `crates/evocc-chir/src/lib.rs`: extend `CHirAtomicAccess` with a failure
  order, a weak flag, an expected-object pointer for write-back, a
  test-and-set or clear kind, and a fence node. `inline.rs` must keep the
  dynamic-order marker through inlining, as it does now.
- `crates/evocc-chir/src/interp.rs` and `crates/evocc-memory/src/atomic.rs`:
  single-thread semantics, write-back, fence events.
- `crates/evocc-lower/src/lib.rs` (`lower_atomic`): emit the ordinary load of
  `*expected`, the `atomic.rmw.cmpxchg`, the address or value comparison, and
  the conditional ordinary store on failure.
- `crates/evocc-evoir/src/atomic.rs`, `text.rs`, `text_parse.rs`, validation
  and `spec/evoir.md` 3.8 and 9.3: a `failure=` order and a `weak` marker on
  `cmpxchg`; a new `atomic.fence <order>` with tokens. Proof reconstruction
  and the certificate path must check both orders.
- `crates/evocc-mir/src/emit.rs` (`MInst::AtomicRmw`): merged order
  selection; a fence instruction. EvoMIR 4.2 text.
- Interaction: `__sync_synchronize` currently lowers to `asm volatile("dmb
  ish")`, which both interpreters report as `E3021`. It can move to
  `atomic.fence seq_cst`.

### 5.2 Runtime helpers

None for the admitted domain. EvoCC keeps inline exclusive loops. The loops
interoperate with libgcc's `__aarch64_*` helpers, GCC's inline LSE code and
Apple's LSE code. `__atomic_is_lock_free` for sizes outside 1/2/4/8 is an
external call to libatomic (Linux, needs `-latomic`) or libSystem (Darwin).

### 5.3 Tests for the TODO exit

1. Value matrix: `l11b-rmw-cas.c` `CASE=0..6,8` in both interpreters and
   native at `-O0` and `-O2` on Linux, plus native on Darwin; `CASE=7` must
   give 0 (left to right) in EvoCC.
2. Negative matrix: `l11b-rmw-negative.c` with the codes of section 4, and
   `-w`/`-std=gnu89` reruns of `atomic-scope-boundaries.c` that must still
   refuse instead of emitting a call.
3. Text and proof: `atomic.rmw.cmpxchg` with each legal order pair and
   `weak`; one-field mutations of the failure order, the weak marker and the
   fence order must fail the checker.
4. Native shape: no `bl __atomic` in the output; exclusive loops with the
   merged order; `dmb ish`/`dmb ishld` for fences.
5. Crossed native: two threads, each side compiled by a different compiler
   (EvoCC/GCC, GCC/EvoCC, EvoCC/Clang, Clang/EvoCC), on 1/2/4/8-byte counters
   with fetch-add, exchange and weak CAS loops; publication and SC litmus as
   in `atomic-thread-driver.c`. Existing `atomic-rmw-driver.c` is the model.
6. Unchanged TinyCC sources as extra inputs:
   `tests/tests2/136_atomic_gcc_style.c` (GNU builtins) must match its
   `.expect`. GCC 14 and Clang 19 pass it at `-O0` and `-O2` today.

### 5.4 Original programs to replay

- TinyCC `0fb54300b565` in the nondefault `arm64-libtcc1-usegcc=yes`
  configuration, so EvoCC compiles `lib/stdatomic.c`. That file defines the
  sized `__atomic_*_N` library functions with generic `__atomic_load` and
  weak `__atomic_compare_exchange`, and declares `__atomic_is_lock_free` as an
  alias when the compiler is not tcc. Then the unchanged `make test`. This is
  a chosen nondefault configuration; the default build compiles these files
  with the freshly built tcc, not with EvoCC.
- zlib 1.3.2 with `DYNAMIC_CRC_TABLE` (uses `__atomic_test_and_set` through
  `atomic_flag_test_and_set`). This also needs L11a.
- SQLite: no new interface; replay to show no regression of the relaxed
  `__atomic_load_n`/`__atomic_store_n` path.

## 6. Size, dependencies and risks

- Size: medium. The IR instruction, interpreter model and MIR loop exist.
  New work is the parser domain, the failure order and weak marker, write-back
  lowering, fences, the reserved-name refusal and the tests.
- Dependencies: none to start. L11a needs this item for compound assignment
  and for GCC's `<stdatomic.h>`. L11c builds on the generic forms.
- Do first, independent of the decision: reserve the spellings so `-w` can no
  longer emit wrong calls. That is a small safety repair.
- Risks: proof certificates that hash the atomic instruction text change. The
  merged native order is stronger than the source order; that is sound but
  must not be presented as an exact copy of GCC's helpers. Crossed tests show
  no lost update on one CPU; they are not a memory-model proof.

## 7. Open points

- Pointer arithmetic forms. The evidence does not give one rule: GCC's
  documented unscaled result and C11's `ptrdiff_t` operand disagree for atomic
  pointers. The recommended refusal stands until an original program needs
  it. Missing fact: an original that uses `__atomic_fetch_add` on a pointer.
- The `__GCC_HAVE_SYNC_COMPARE_AND_SWAP_16` macro. Keeping it matches GCC's
  preprocessing; removing it would steer programs to other paths. The
  recommendation keeps it and keeps the 16-byte refusal. A lead decision on
  16-byte support (L11c) closes this.
