# L06: external ABI of integer vectors smaller than 8 bytes

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
- ABI documents: AAPCS64 release 2025Q4 (`ARM-software/abi-aa` main at
  `ec3e2fb6`, read 2026-10-13); Apple, "Writing ARM64 code for Apple
  platforms".

## 1. Question

A GNU integer vector of 1, 2 or 4 bytes (a "tiny vector": `char` x1/x2/x4,
`short` x1/x2, `int` x1) can be a parameter, a result, a variadic argument or a
record member. EvoCC executes these values locally but refuses them at a
function boundary with E4002. The rule must state, per target:

1. The register class and bit layout for parameters, results, variadic
   arguments and records.
2. Which reference compiler EvoCC interoperates with, in which directions,
   and which crossed calls stay incompatible (negative controls).
3. How the chosen convention is identified, and what happens under
   `-mgeneral-regs-only`.

## 2. Evidence

### 2.1 C standard

C has no vector types. `vector_size` is a GNU extension; its call convention
is a target matter.

### 2.2 GCC 14

GCC treats a tiny vector like an integer of the same size: the argument goes
in the next general register (`w`), the result in `w0`, lanes packed from bit
0 in memory order. On the stack each takes an 8-byte slot: after eight `long`
arguments, GCC and Clang 19 both read `v1`, `v4`, `v1` at `[sp]`,
`[sp, #8..11]` and `[sp, #16]` (scratch probe, `-O2`). Measured at `-O2`: `give4` returns `mov w0, 5643; movk w0,
0x2c21, lsl 16` (bytes 11, 22, 33, 44); `give_h1` returns `mov w0, -3`;
`give_i1` returns `mov w0, -5`. GCC accepts all of this under
`-mgeneral-regs-only`.

### 2.3 Clang

- Arguments: `clang/lib/CodeGen/Targets/AArch64.cpp` treats a vector that is
  not 8 or 16 bytes as illegal and coerces it to `i32` (`coerceIllegalVector`),
  so arguments go in a general register, exactly as in GCC.
- Results: the vector type stays as is, and LLVM type legalization widens it
  into `v0`. Measured: 4 x `u8` returns as 4 x 16-bit lanes in `d0` (each
  byte in the low half of an `h` lane); 2 x `u8` and 2 x `u16` return as
  2 x 32-bit lanes in `d0`; 1 x `u8`, 1 x `short` and 1 x `int` return in
  lane 0 of `v0`.
- Under `-mgeneral-regs-only`, Clang refuses the worker: "'x' requires 'v4'
  (vector of 4 'unsigned char' values) type support, but ABI 'aapcs' does not
  support it".
- Apple clang emits the same argument and result code as Clang 19. On the
  Darwin stack, the `i32` images take packed 4-byte slots: after eight `long`
  arguments, a callee reads `v1`, `v4`, `v1` at `[sp]`, `[sp, #4..7]` and
  `[sp, #8]` (scratch probe, `-O2`).

### 2.4 Target ABI

- AAPCS64 "Short Vectors": "A short vector ... may be 8 or 16 bytes in total
  size." A 1-, 2- or 4-byte vector is not a short vector, not a Fundamental
  Data Type of the C binding, and not an HVA member. AAPCS64 does not define
  its convention.
- AAPCS64 rule C.9 copies an Integral argument of at most 8 bytes to the
  least significant bits of `x[NGRN]`. GCC's choice is that rule applied to
  an integral image of the vector's size.
- Apple's arm64 notes do not mention vectors below 8 bytes. On Darwin the
  only existing practice is Apple clang's. Apple's `<simd/simd.h>` defines
  4-byte types such as `simd_uchar4` and `simd_short2` with the same Clang
  rules; most of their functions are inline.

### 2.5 Probes

Probe sources (new): `tests/fixtures/rules-20261013/l06-tiny-vector-abi.h`
(SHA-256 `936195c2…0eaf9`), `l06-tiny-vector-worker.c` (`a0d0f13c…f0f9`),
`l06-tiny-vector-driver.c` (`0c64fbb7…69198`). The driver exits with one bit
per failing direction: 1 arguments, 2 `v4` result, 4 `v2` result, 8 two-lane
`short` result, 16 both directions, 32 record member, 64 varargs.

Commands:

```sh
# container evo-nexlab:sealed, --network none, for o in O0 O2:
cc=gcc-14|clang-19
$cc -std=gnu11 -$o -w -c l06-tiny-vector-worker.c -o w-$cc-$o.o
$cc -std=gnu11 -$o -w -c l06-tiny-vector-driver.c -o d-$cc-$o.o
gcc-14 d-$DRIVER-$o.o w-$WORKER-$o.o -o x && ./x; echo $?
# host:
clang -std=gnu11 -O0|-O2 -w l06-tiny-vector-driver.c l06-tiny-vector-worker.c -o x && ./x
# EvoCC:
E=.build/frozen/366fd796/evocc-darwin
$E --target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu \
   -std=gnu11 -O2 -S -o /dev/null l06-tiny-vector-worker.c
```

| Driver \ worker | GCC O0 | Clang O0 | GCC O2 | Clang O2 |
| --- | --- | --- | --- | --- |
| GCC | 0 | 30 | 0 | 30 |
| Clang | 30 | 0 | 30 | 0 |

`30` means bits 2, 4, 8 and 16: every result direction fails. Arguments,
records and varargs pass in every crossed pair. Apple clang passes its own
pair at `-O0` and `-O2`.

Original programs (hashes equal to the ladder records):

| Original | Tiny-vector use | GCC, Clang, Apple (O0/O2) | EvoCC interpreters | EvoCC native |
| --- | --- | --- | --- | --- |
| torture `pr60960.c` (`b70701c4…1276`, E0 run `20260927T084053Z`) | `v4qi` parameters and results of external functions | exit 0 | exit 0 | E4002 |
| LLVM SingleSource `pr60960.c` (same bytes, run `20260927T085137Z`) | same | exit 0 | exit 0 | E4002 |
| LLVM SingleSource `20050316-1.c` (`7b4b7355…7585`, `-Wno-psabi`) | `V2HI` (4-byte) parameter only | exit 0 | exit 0 | E4002 |

All three are single translation units, so their own calls do not cross
compilers. They fail natively only because EvoCC refuses the signature.

## 3. Current EvoCC behavior

10.1.14: "A sub-8-byte vector at an external ABI boundary is E4002." EvoMIR
Section 7 describes E4002 as "Native arguments and results of integer vectors
below 8 bytes are refused; local operations on them are admitted".
`crates/evocc-mir/src/lower.rs` reports it for parameters, results, call
operands and `va_arg` with the note "sub-8-byte vector ABI differs between the
sealed compilers". The command line prints `error[E4002]:
evomir-lowering-refused`; the spec's message is `not-lowered: vector`. The
EvoIR stage and both interpreters accept the probe and the originals.
EvoMIR 2.7 already keeps 1-, 2- and 4-byte vectors in `gpr32`.

## 4. Recommended rule

Principle applied: on Linux, follow GCC 14. Its convention also matches Clang
for arguments, records and varargs. On Darwin, follow Apple clang, the
platform compiler and the only existing practice.

Proposed replacement for the last sentence of 10.1.14 "Integer-vector
profile", with matching EvoMIR text in Section 6:

> A tiny vector (a GNU integer vector of 1, 2 or 4 bytes) at a call boundary
> is classified by target:
>
> - `aarch64-unknown-linux-gnu`: as an unsigned Integral value of the same
>   size, for parameters, results and anonymous arguments. It uses the next
>   general register (`w` view) or an 8-byte stack slot, and a result uses
>   `w0`. Lane `i` occupies bits `8*i*w` to `8*(i+1)*w - 1`, where `w` is the
>   lane size in bytes. Bits above the vector are unspecified, as for other
>   sub-width values (EvoMIR 3.1). This needs no SIMD register, so it is
>   admitted under `-mgeneral-regs-only`.
> - `aarch64-apple-darwin`: a parameter is the 4-byte integer image that
>   Apple clang coerces it to (`i32`): the next general register, or a 4-byte
>   stack slot under Darwin's packed stack rule; an anonymous argument takes
>   Darwin's 8-byte variadic slot. A result is returned in `v0` with each
>   lane widened to fill 64 bits: 4 x 8-bit as 4 x 16-bit, 2 x 8-bit and
>   2 x 16-bit as 2 x 32-bit, and a one-lane vector in lane 0. The caller
>   reads only the low bits of each widened lane.
>
> A record that contains a tiny vector follows the ordinary composite rules;
> a tiny vector is never an HVA member. The target triple identifies the
> convention; no option changes it.

Interoperability claims (and only these):

- Linux: GCC in both directions for every shape. Clang for arguments, records
  and varargs. Calls that return a tiny vector between EvoCC and Clang code
  are a recorded incompatibility, with crossed negative controls.
- Darwin: Apple clang in both directions, after the crossed tests of 5.3
  pass.

Rejected alternatives:

- **Clang's split convention on Linux.** It contradicts GCC, the platform
  compiler, and cannot work under `-mgeneral-regs-only`.
- **A user-selectable "Clang-compatible" mode on Linux.** It doubles the test
  matrix, needs a module identity mark and has no consumer in the recorded
  programs. It can be added later as its own item.
- **Keep E4002.** The three originals stay unsupported although GCC's
  convention is fully measured.
- **One GCC convention on both targets.** It breaks result interoperability
  with the Darwin platform compiler. Kept as open point 1.

## 5. Implementation scope

### 5.1 Compiler

- `crates/evocc-mir/src/lower.rs`: remove the four E4002 refusals for
  `vector_bytes` 1, 2 and 4 (parameters near line 563, results near 590,
  calls near 684, `va_arg` near 739).
- `crates/evocc-mir/src/isa/aarch64/abi.rs` and `abi_tables.rs`: classify a
  tiny vector as a general-register value of its size on both targets, except
  the Darwin result, which gets a new SIMD result class with the widened lane
  layout. `crates/evocc-mir/src/generic/abi.rs` if the class is shared.
- Variadic: `variadic_slot` must give a tiny vector an 8-byte general slot.
- `spec/evoir.md` 10.1.14 and `spec/evomir.md` 6.3 (Linux) and the Darwin
  argument section; E4002's description; pins.

### 5.2 Runtime helpers

None.

### 5.3 Tests for the TODO exit

- Linux crossed matrix with the three probe files: EvoCC worker with GCC and
  EvoCC drivers, and EvoCC driver with GCC and EvoCC workers, at `-O0` and
  `-O2`: exit 0. Negative controls: EvoCC with Clang in both directions must
  give exactly 30 (results fail; arguments, records and varargs pass).
- Darwin crossed matrix: EvoCC with Apple clang in both directions: exit 0.
- `-mgeneral-regs-only` on Linux: the worker compiles with EvoCC and links
  with a GCC driver built with the same option.
- One test per shape (1 x 8, 2 x 8, 4 x 8, 1 x 16, 2 x 16, 1 x 32 bits) that
  checks the result register and lane bits on each target, plus a stack-passed
  argument after eight general arguments.
- Records: `struct { v4 x; int tag; }` in registers and on the stack.

### 5.4 Original programs to replay

- Torture `pr60960` (E0 ladder) and LLVM SingleSource `pr60960` and
  `20050316-1`: expected PASS natively; today they are `not-applicable` with
  E4002.

## 6. Size, dependencies and risks

- Size: **small** for Linux (remove refusals, one classification).
  **Medium** with the Darwin widened result class.
- Dependencies: L05 for floating tiny vectors (`float` x1, half lanes); this
  rule covers integer lanes only.
- Risks:
  - Linux EvoCC objects that return tiny vectors cannot be mixed with Clang
    objects. The incompatibility is recorded, not hidden.
  - The Darwin result layout comes from LLVM type legalization, not from a
    written ABI. A future Apple clang could change it; the crossed tests must
    run against each pinned Apple clang.

## 7. Open points

1. **Darwin.** Apple clang's widened `v0` result (proposed) or GCC's `w0`
   result on both targets. The Apple convention is unwritten but is the
   platform's only practice; GCC's is simpler.
2. **Clang mode on Linux.** Not proposed. Add it only if an original program
   needs to link tiny-vector results with Clang-built code.
