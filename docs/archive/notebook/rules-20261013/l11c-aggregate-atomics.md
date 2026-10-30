# L11c: Generic aggregate atomics

This is a decision packet for the lead. It was prepared on 2026-10-13. It is a
proposal. It does not approve a rule and it changes no specification text.
Every result below was measured for this packet unless a line names an older
record. Section 7 lists the points that the evidence does not settle. L11b
covers the generic forms on scalar objects; L11a covers `_Atomic` objects.

## Subject

| Item | Identity |
| --- | --- |
| Compiler | `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (366fd796613e 2026-10-13)`, SHA-256 `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04` |
| Specifications | `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md` Revision 1.0.1 at HEAD `546fe46a` |
| Linux references | GCC 14.2.0-19 (`gcc-14`), Clang 19.1.7 (`clang-19`), libatomic1 14.2.0-19, glibc 2.41 in `evo-nexlab:sealed`, image `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`, linux/arm64, no network |
| Darwin reference | Host Apple clang 21.0.0 (clang-2100.1.1.101), arm64-apple-darwin25.6.0 |
| Guest CPU | Apple silicon under the container VM; `/proc/cpuinfo` lists `atomics` (LSE) and `uscat` (LSE2) |

Probe sources in `tests/fixtures/rules-20261013/` (SHA-256 prefix):

- `l11c-aggregate.c` (`a9dd5f0b`): generic forms on records, `CASE=0..5`.
- `l11c-misaligned.c` (`7657a3e2`): references only; an 8-byte record with
  alignment 4 at an address that is 4 modulo 8, `CASE=0..1`.
- `l11c-crossed-worker.c` (`8c50a07f`) and `l11c-crossed-driver.c`
  (`4dd90216`): a two-thread control whose two sides come from different
  compilers.
- The code-shape probe `l11b-codegen.c` (functions `load3`, `load8`,
  `load12`, `cas8`, `load16`, `store16`, `cas16`) and the layout probe
  `l11a-atomic-layout.c` also feed this packet.
- The existing `atomic-scope-boundaries.c` `CASE=2` and
  `atomic-cas-boundaries.c` `CASE=2` were rerun.

## 1. Question

What do `__atomic_load`, `__atomic_store`, `__atomic_exchange` and
`__atomic_compare_exchange` mean on records, unions and other objects that are
not 1/2/4/8-byte scalars? The same rule governs whole-object access to an
`_Atomic` record (L11a). The rule must answer:

1. The admitted sizes and alignments, including under-aligned objects and
   16-byte objects.
2. Which bytes move, and what happens to padding, initializedness and the
   provenance of pointer bytes.
3. What compare-exchange compares.
4. Which accesses are lock-free instructions and which need a library, and
   when two compilers' code may share an object.
5. The lock-free queries for these sizes.
6. How EvoIR represents the operation, given that EvoIR has no record values.
7. What the interpreters establish.

## 2. Evidence

### 2.1 C standard

- C11 7.17.7.4p2 compares "the value pointed to by object for equality with
  that in expected". NOTE 1 (7.17.7.4p3) gives the effect with `memcmp` and
  `memcpy`. C23 7.17.7.4p3 (N3220) is more direct: it "compares the contents
  of the memory pointed to by object for equality with that pointed to by
  expected" and "updates the contents of the memory pointed to by expected".
  C23 keeps the `memcmp` note. Padding bytes therefore take part.
- C11 6.2.6.1p6: when a value is stored in a structure or union, "the bytes
  of the object representation that correspond to any padding bytes take
  unspecified values". A comparison of padding is not undefined behavior in
  C; its result is unspecified.
- C11 7.17.6 and 6.2.5p27 allow any complete object type as an atomic type,
  records included. 6.5.2.3p5 makes member access of an atomic record
  undefined; whole-object load and store are the defined operations.

### 2.2 GCC 14

- Documented: the generic forms work "on any data type". GCC "uses the
  lock-free built-in function if the specific data type size makes that
  possible; otherwise, an external call is left to be resolved at run time",
  with a `size_t` size as the first argument.
- Measured: the decision depends on the size only. An 8-byte record with
  alignment 4 gets LDAR, STLR and `__aarch64_cas8_acq_rel` at `-O2`
  (`load8`, `cas8`, and `l11c-misaligned.c`). A 16-byte record calls
  libatomic `__atomic_load_16` and `__atomic_compare_exchange_16`. Sizes 3 and
  12 call the generic `__atomic_load` and `__atomic_compare_exchange`.
- libatomic 14.2: the `_16` entry points are IFUNC symbols whose candidates
  are LDXP/STXP, LDP (LSE2) and CASP bodies without a lock. The generic
  entry points serve sizes without a lock-free body; the library contains
  `pthread_mutex_lock` calls for that path. LLVM's "Atomics" document says
  these routines "will certainly use mutexes for some sizes/alignments" and
  that "the set of atomic sizes supported lock-free must be a superset of the
  sizes any compiler can emit".
- `__atomic_always_lock_free`: 1 for an 8-byte record, 0 for 16, 3 and 12
  (`l11c-aggregate.c` `CASE=5` returns 1).

### 2.3 Clang

- Clang 19 decides by size and alignment. The 4-aligned 8-byte record calls
  the generic `__atomic_load` and `__atomic_compare_exchange` and warns
  `-Watomic-alignment`. Sizes 3 and 12 also call the generic functions.
- 16 bytes: Clang 19 inlines LDAXP/STLXP for load and store and calls
  `__aarch64_cas16_acq_rel` for compare-exchange. Apple clang uses LDP plus
  DMB, DMB plus STP plus DMB, and CASPAL.
- Darwin generic calls go to `___atomic_exchange`, `___atomic_load` and
  `___atomic_compare_exchange`, which `nm -m` resolves "from libSystem";
  `CASE=2` and `CASE=3` link and pass with Apple clang.
- `__atomic_always_lock_free` is 1 for 8 and 16 bytes (`CASE=5` returns 3).

### 2.4 Target ABI

AAPCS64 2025Q4 says nothing about atomics. The contract is the libatomic ABI
that GCC defines and LLVM follows (generic functions with a size argument,
sized `_1` to `_16` variants), plus the rule that every access to one object
must use compatible mechanisms. Lock-free instructions and lock-free library
bodies interoperate. A lock-based library path interoperates only with other
accesses that use the same library lock table, which needs one shared
libatomic in the process.

### 2.5 Probes

```sh
CC -std=gnu11 -O{0,2} -DCASE=$n l11c-aggregate.c -o p [-latomic] && ./p
CC -std=gnu11 -O{0,2} -DCASE=$n l11c-misaligned.c -o p [-latomic] && ./p
$E --target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu \
   -std=gnu11 -O{0,2} -DCASE=$n {--interpret-c-hir,--interpret-evoir,-S} l11c-aggregate.c
# Crossed control (container), for A, B in {gcc-14, clang-19}:
A -std=gnu11 -O2 -DSIDE=a -c l11c-crossed-worker.c -o a.o
B -std=gnu11 -O2 -DSIDE=b -c l11c-crossed-worker.c -o b.o
gcc-14 -std=gnu11 -O2 -c l11c-crossed-driver.c -o d.o
gcc-14 -pthread d.o a.o b.o -o x -latomic && ./x     # three runs per pair
```

`l11c-aggregate.c` (exit status, same at `-O0` and `-O2`):

| CASE | Content | GCC 14 | Clang 19 | Apple clang | EvoCC (all modes, both targets) |
| --- | --- | --- | --- | --- | --- |
| 0 | 8-byte, 8-aligned record: load, exchange, CAS, store | 0 | 0 | 0 | `W0003` (21:5) |
| 1 | 16-byte pointer-bearing record: CAS, load | 0 | 0 | 0 | `W0003` (35:10) |
| 2 | 3-byte record exchange | 0 | 0 | 0 | `W0003` (43:5) |
| 3 | 12-byte record CAS and load | 0 | 0 | 0 | `W0003` (48:10) |
| 4 | equal members, different padding bytes (0 = bytes compared) | 0 | 0 | 0 | `W0003` (57:14) |
| 5 | `always_lock_free` mask for 8/16/3/12 bytes | 1 | 3 | 3 | `W0003` (60:13) |

`l11c-misaligned.c`: GCC (LDAR, STLR, outline CAS) and Clang (generic
library calls) both exit 0 at `-O0` and `-O2`; Apple clang also exits 0. This
CPU has LSE2. NOT RUN: a CPU without LSE2, where an unaligned LDAR or CASAL
raises an alignment fault.

Crossed control: all 12 runs (four pairs, three runs each) exit 0 with
`c16=400000/800000 c12=400000/800000 c8=400000/800000`. The GCC side calls
`__atomic_load_16`, `__atomic_compare_exchange_16`, the generic 12-byte
functions and `__aarch64_cas8_acq_rel`. The Clang side calls
`__aarch64_cas16_acq_rel`, the generic 12-byte functions and
`__aarch64_cas8_acq_rel`. This shows no lost update on this CPU and this
libatomic. It is a stress observation, not a memory-model proof.

## 3. Current EvoCC behavior

- No generic form is a builtin. In GNU11 every case stops at `W0003` (or
  `E2931`); the TODO's `E2001` is out of date. `atomic-scope-boundaries.c`
  `CASE=2` and `atomic-cas-boundaries.c` `CASE=2` give `W0003`.
- With `-w`, `-Wno-implicit-function-declaration`, `--cap-lints=allow` or
  `-std=gnu89`, `atomic-scope-boundaries.c` `CASE=2` compiles to
  `bl __atomic_load` with three arguments. Linked with libatomic, whose
  `__atomic_load` takes `(size_t, void *, void *, int)`, it ends with SIGSEGV
  (exit 139). This is silent wrong code. L11b's reserved-spelling repair
  removes it.
- EvoIR atomic instructions take `bits<1|8|16|32|64>` or a pointer (EvoIR
  3.8). There is no record-typed value and no 128-bit atomic access. EvoMIR
  4.1 states that the `bits<128>` LDP/STP pair "is not single-copy atomic".
- The interpreters' `memcmp` already reads bytes with initializedness checks
  and reports `E3001` on an uninitialized byte (EvoIR 10.1.3). Record copies
  transfer initializedness and leave padding uninitialized (also 10.1.3).

## 4. Recommended rule

> **Generic forms on records and unions.** `__atomic_load`,
> `__atomic_store`, `__atomic_exchange` and `__atomic_compare_exchange` admit
> a complete record or union type `T` when every pointer operand points to
> `T` (ignoring qualifiers and `_Atomic`), `sizeof(T)` is 1, 2, 4 or 8, and
> the alignment of `T` is at least its size. L11b covers scalar `T`; an array
> `T` is refused. For an `_Atomic` record the L11a layout rule decides the
> alignment. The same rule governs whole-object load and store of an
> `_Atomic` record or union.
>
> **Transfer.** Load, store and exchange move the object representation as
> `copy` does: every byte keeps its initializedness and pointer provenance,
> and a padding byte that was uninitialized stays uninitialized. Natively the
> object side is one single-copy-atomic access of the whole size; the buffer
> side (`ret`, `val`, `desired`, `expected`) uses ordinary accesses.
>
> **Comparison.** Compare-exchange compares every byte of `*ptr` with the
> same byte of `*expected` (C23 7.17.7.4p3). Padding takes part. Both
> interpreters report `E3001` when a compared byte is uninitialized, as their
> `memcmp` does; this is the profile's existing conservative boundary, not a
> claim that C makes the comparison undefined. Pointer bytes compare by
> address; provenance does not take part. Success transfers `*desired` into
> the object; failure transfers the observed bytes into `*expected`.
>
> **Orders, evaluation and weak.** As L11b.
>
> **Lock-free queries.** `__atomic_always_lock_free(n, 0)` gives GCC's answer
> on Linux (1 for 1/2/4/8, 0 otherwise) and Apple clang's on Darwin (1 for
> 1/2/4/8/16), whatever EvoCC implements.
>
> **Refusals.** Other sizes, alignment below the size, a variably modified
> `T`, and mismatched pointee types keep a named front-end refusal (the L11b
> code). These objects need the generic library path (libatomic on Linux,
> libSystem on Darwin), which may use locks; EvoCC does not emit that call in
> this scope.

Second step (16 bytes), proposed for separate approval:

> A 16-byte `T` with alignment 16 is admitted. Linux native code uses an
> LDAXP/STLXP exclusive loop for every form; Clang 19 does so for load,
> store and fetch-add and calls the lock-free `__aarch64_cas16_acq_rel` for
> compare-exchange. This interoperates with GCC's libatomic 14.2 `_16`
> bodies, which are lock-free, and with Clang's code (crossed control above). Darwin native code uses LDP
> and STP with DMB, and CASPAL, as Apple clang does. A 16-byte atomic load
> writes the object on Linux, so an object in read-only memory faults; the
> same holds for libatomic's LDXP/STXP body on CPUs without LSE2.

Interpreter ceiling: one thread. The interpreters show the transferred bytes,
initializedness, provenance and the order of atomic events. Inter-thread
claims rest on the crossed native tests.

Rejected alternatives:

- Load the record as `bits<N>` with the scalar `atomic.load`. That needs every
  byte initialized and turns pointer bytes into numbers, so it would make
  padding fail as `E3001` on every load and would drop provenance.
- Treat padding as zero, or compare members only. The references and C23
  compare bytes; zero-filling invents values.
- Follow GCC's size-only rule for under-aligned objects. It emits LDAR and
  CASAL on addresses that may be 4 modulo 8; Clang calls the library for the
  same object. The two compilers then use different mechanisms on one object.
- Emit generic libatomic calls now. That adds a lock-based runtime boundary
  that the interpreters cannot execute and whose interoperability needs one
  shared library instance.

## 5. Implementation scope

### 5.1 Compiler

- `crates/evocc-parser/src/lib.rs`: the generic forms (shared with L11b) with
  the record domain and pointee-type checks.
- `crates/evocc-chir`: an atomic node with memory operands (object, value or
  result buffer, expected buffer) and a byte size; C-HIR interpreter support.
- `crates/evocc-memory/src/atomic.rs`: byte-level atomic transfer, exchange
  and compare-exchange that reuse `copy_representation` and the `memcmp`
  byte reader in `crates/evocc-memory/src/lib.rs`, and append one atomic
  event with the observed bytes.
- `crates/evocc-evoir`: new memory-to-memory atomic forms (for example
  `atomic.copy`, `atomic.xchg.mem`, `atomic.cmpxchg.mem` with size, orders and
  weak marker); text, parser, validation, effects (`reads={All}`,
  `writes={All}`, `atomic=true`), proof reconstruction and one-field mutation
  checks. Specification: EvoIR 3.8, 4.2 table, 9.3 grammar, 10.1.14.
- `crates/evocc-mir`: lowering to LDAR/STLR or an exclusive loop of the whole
  width through a general register, with ordinary buffer accesses; the
  second step adds LDAXP/STLXP. EvoMIR 4.2.

### 5.2 Runtime helpers

None in the first step. The second step needs none either if it inlines
LDAXP/STLXP. Following GCC literally would need libatomic `__atomic_*_16`
calls and `-latomic` at link, plus an interpreter model of those calls.

### 5.3 Tests for the TODO exit

1. `l11c-aggregate.c` `CASE=0` and `CASE=4` (must give 0) in both
   interpreters and native, `-O0` and `-O2`, Linux and Darwin.
2. A variant of `CASE=4` without the `memset` of `want` must give `E3001` in
   both interpreters.
3. `atomic-scope-boundaries.c` `CASE=2` (8 bytes) passes in the first step
   and `atomic-cas-boundaries.c` `CASE=2` (16 bytes) in the second; with `-w`
   and `-std=gnu89` the refused shapes still refuse and emit no
   `bl __atomic_load`.
4. `CASE=2` and `CASE=3` (3 and 12 bytes) give the named refusal; `CASE=5`
   gives 1 on Linux and 3 on Darwin.
5. Provenance: after a successful record CAS that stores `&second`, a load
   through the stored pointer passes; a record whose pointer bytes came from
   an integer gives `E3013` at the dereference.
6. Text and proof: one-field mutations of size, orders and buffer operands
   fail the checker.
7. Crossed native: `l11c-crossed-worker.c` with EvoCC as either side against
   GCC and Clang, for the 8-byte record now and the 16-byte record in the
   second step; three runs per pair; no lost update.

### 5.4 Original programs to replay

The L11 inventory found no generic aggregate atomic in the captured
populations. TinyCC's `125_atomic_misc.c` section `test_atomic_store_struct`
stores a 4-byte `_Atomic` record and then reads its members, which 6.5.2.3p5
makes undefined; it is not a valid oracle. The replays that matter are the
L11a and L11b ones (TinyCC usegcc configuration, zlib `DYNAMIC_CRC_TABLE`),
which use the generic forms on scalar atomics, plus SQLite for no regression.

## 6. Size, dependencies and risks

- Size: large. It adds an EvoIR operation family on memory operands, byte
  semantics in both interpreters, proof support and MIR lowering.
- Dependencies: L11b (spellings, orders, evaluation, write-back) and L11a
  (atomic layout) first.
- Risks: the `E3001` comparison boundary rejects some programs that C allows
  with an unspecified result; the packet records that as a deliberate
  conservative choice. The crossed control ran on one CPU with LSE and LSE2;
  CPUs without LSE2 are NOT RUN. A later libatomic that changed its 16-byte
  bodies to locks would break the inline second step; the libatomic version
  is part of the sysroot pin.

## 7. Open points

- 16 bytes: inline LDAXP/STLXP (Clang's choice, measured compatible) or
  libatomic calls (GCC's choice). The evidence supports both on this
  sysroot. The recommendation is inline, for interpreter coverage and no new
  link dependency. The missing fact is whether the lead accepts the
  read-only-memory fault of an exclusive-pair load.
- The comparison of uninitialized padding: `E3001` (recommended, consistent
  with `memcmp`) or an unsupported-execution code that does not suggest
  undefined behavior. This is a naming decision; the behavior is the same.
