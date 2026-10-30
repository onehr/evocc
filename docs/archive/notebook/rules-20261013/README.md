# Decision packets for the NEEDS_RULE items, 2026-10-13

These packets prepare the lead's rule decisions for K07, L03 to L10 and L11a
to L11c. Each packet is a proposal. None approves a rule, changes a
specification or closes a TODO item. The Rust lane implements a rule only
after the lead accepts it.

## Common subject

- Compiler: `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (546fe46a26e8
  2026-10-13)`, SHA-256
  `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04`. It
  targets Darwin by default and Linux with
  `--target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu`.
- Specifications: `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md`
  Revision 1.0.1 at HEAD `546fe46a`.
- Linux references: GCC 14.2.0-19 (`gcc-14`) and Clang 19.1.7 (`clang-19`)
  in `evo-nexlab:sealed`, image
  `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`,
  glibc 2.41, run with `container run --rm --platform linux/arm64 --network none`.
- Darwin reference: host Apple clang 21.0.0 (clang-2100.1.1.101),
  arm64-apple-darwin25.6.0.
- Probe sources: `tests/fixtures/rules-20261013/`, named by item. Each packet
  lists its probe hashes, exact commands and measured results. EvoCC runs use
  `--interpret-c-hir`, `--interpret-evoir` and `-S` (then `gcc-14` links and
  runs the assembly in the container) at `-O0` and `-O2`; the interpreters
  report `main`'s value and do not execute `printf`.

Principle applied in every packet: on Linux follow GCC 14 unless the standard
requires otherwise; on Darwin follow Apple clang where the platform ABI or
library depends on it; keep undefined behavior diagnosed as the repository
already does; claim no interoperability that a crossed test has not shown.

## Packets

| Item | Packet | Recommendation in one line | Size |
| --- | --- | --- | --- |
| K07 | [k07-nonliteral-strlen-ice.md](k07-nonliteral-strlen-ice.md) | `__builtin_strlen` of an immutable static character array or `T *const` pointer object (with constant offsets, casts, constant conditionals) is an extended ICE on both targets and at every `-O`; no call; everything else stays an ordinary call | small (plus kernel replay) |
| L07 | [l07-integer-complex-division.md](l07-integer-complex-division.md) | GCC's wide (Smith) algorithm in the component type, every step with its existing E3002/E3004 obligations; same rule on Darwin | small to medium |
| L08 | [l08-pointer-vla-typeof.md](l08-pointer-vla-typeof.md) | Evaluate a `typeof` operand exactly when its type is variably modified (C23 6.7.3.6p4, GCC manual, GCC 15 fix PR c/114831); GCC 14's skip is a known defect | small |
| L09 | [l09-ancestor-return-address.md](l09-ancestor-return-address.md) | Linux: depth 1..65535 yields a null pointer, as GCC; Darwin: native frame-chain walk with a zero-link check, interpreters stop with E3031 | small (Linux), medium (Darwin) |
| L06 | [l06-tiny-vector-abi.md](l06-tiny-vector-abi.md) | Linux: 1/2/4-byte integer vectors pass and return as integers in general registers (GCC); Darwin: Apple clang's widened `v0` result; Clang-on-Linux results stay a recorded incompatibility | small (Linux), medium (Darwin) |
| L10 | [l10-pr23135-definedness.md](l10-pr23135-definedness.md) | Classify LLVM `pr23135` as source UB (`not-applicable`): lane overflow at line 131, GCC UBSan report, explained Clang silence, GCC upstream `-fwrapv` fix; adopt a six-point evidence threshold | small (no compiler change) |
| L11b | [l11b-atomic-rmw-cas.md](l11b-atomic-rmw-cas.md) | Admit the GNU fetch-op, op-fetch, exchange, strong/weak CAS, test-and-set, clear, fences and lock-free queries on 1/2/4/8-byte scalars with inline loops; E2947 exactly for GCC-warned order pairs; reserve unadmitted spellings | medium |
| L11a | [l11a-general-atomic-objects.md](l11a-general-atomic-objects.md) | Admit `_Atomic` with GCC layout on Linux and Apple layout on Darwin; reads and stores are seq_cst; compound assignment and `++`/`--` are one seq_cst RMW with existing obligations | large |
| L11c | [l11c-aggregate-atomics.md](l11c-aggregate-atomics.md) | Admit naturally aligned 1/2/4/8-byte records through memory-to-memory atomic forms with byte transfer and byte comparison (E3001 on uninitialized bytes); 16 bytes as a second step | large |
| L03 | [l03-half-bfloat.md](l03-half-bfloat.md) | `_Float16`, `__fp16`, `__bf16` are storage formats; operations evaluate in binary32 (GCC 14, C23 H.3 method 0); conversions round once; AAPCS64 scalars and HFAs except refused `__bf16` HFAs | medium |
| L04 | [l04-floating-complex.md](l04-floating-complex.md) | Complex values are HFA pairs; `+`, `-` and mixed operands work per part; multiply and divide become `fp.cmul`/`fp.cdiv` (and strict forms) defined by each target's qualified helper (libgcc on Linux, compiler-rt on Darwin); interpreters share one model, native code calls the helpers; phases A-D unblock the 68 musl objects | large |
| L05 | [l05-floating-vectors.md](l05-floating-vectors.md) | GNU `float`/`double` vectors with per-lane scalar rules, no implicit contraction, E3063 per lane in conversions, HVA by vector size; a small optional Linux intrinsic set; the full NEON original becomes its own item | large |

## Points the evidence does not settle

Each packet's Section 7 names the missing fact or the policy choice.

- K07: whether to exclude automatic const objects (both references admit
  them) and whether block-scope static arrays count (GCC excludes them).
- L07: Darwin rule (GCC algorithm or Apple clang's textbook formula); handling
  of `-fcx-limited-range`/`-ffast-math`, tied to L04 R14.
- L09: Darwin walk or GCC's null on both targets.
- L06: Darwin widened result or GCC's `w0` result on both targets.
- L11b and L11a: scaling of `__atomic_fetch_add` on pointers (GCC unscaled,
  C11 and Clang scaled); floating compound assignment on `_Atomic` objects
  (depends on the strict floating-point rule).
- L11c: inline 16-byte code or libatomic calls; the name of the padding
  comparison refusal.
- L03: Darwin evaluation method; `__bf16` HFAs (GCC PR 120007 open); half to
  integer overflow.
- L04: Darwin division flags (needs the macOS `__divsc3`/`__divdc3`
  instruction sequence); binary128 special-value grid NOT RUN; static
  initializer folding choice.
- L05: Darwin mask lane type; `v++` and `&v[i]`; `-mgeneral-regs-only`
  copies; whether to add any intrinsic now.

L08 and L10 have no open point.

## Defects found while probing

These need no rule decision. Four of them were reproduced again before this
index was written (marked "re-checked").

- Darwin `long double` is 16 bytes with 16-byte alignment in EvoCC; Apple's is
  8 bytes, binary64 (re-checked: `sizeof*10+_Alignof` gives 176 against 88).
  It gates `_Atomic long double` (L11a) and `long double _Complex` (L04) on
  Darwin.
- A read of an `_Atomic int` compiles to a plain `ldr`, not an atomic load
  (re-checked with the Linux target at `-O2`). Atomic bit-fields, `_Atomic`
  function types and member access of atomic records compile without error,
  and atomic record layout differs from GCC and Apple clang (L11a).
- With `-w`, `-std=gnu89` or `--cap-lints=allow`, unknown `__atomic_*` names
  become implicit external calls; `atomic-scope-boundaries.c` CASE=2 then
  crashes with SIGSEGV against libatomic (L11b; re-checked with `-w`: `bl
  __atomic_load`, exit 139). `__has_builtin` answers 0 for every atomic
  builtin, including implemented ones.
- Records of float vectors are passed as general composites by reference,
  while GCC and Clang use SIMD registers (re-checked: `struct { f4 a, b; }`
  argument goes through `x0` in EvoCC and `q0`/`q1` in GCC). Integer-vector
  records of mixed lane types also fail crossed calls. Source:
  `homogeneous_vector_aggregate` in `crates/evocc-lower/src/lib.rs` (L05).
- The C-HIR interpreter still reports withdrawn E3007 for vector values (L05).
- E4002 prints `evomir-lowering-refused`; the spec's message is
  `not-lowered: vector` (L06).
- The TODO's L11 text cites E2001; the current first refusals are W0003 and
  E2931 (L11b).

## Dependencies between items

- L11b before L11a before L11c.
- L04 and L07 edit the same `complex_binary` function; L04 R14 decides the
  limited-range options that L07 also needs.
- L05 should follow the HVA defect fix; L03 gates half lanes in L05; L06 gates
  tiny floating vectors.
- The Darwin `long double` repair gates the Darwin parts of L04 and L11a.

## Probe index

- K07: `k07-strlen-ice-matrix.c`, `k07-strlen-ice-contexts.c`,
  `k07-strlen-call-effects.c`.
- L03: `l03-half-conversions.c`, `l03-half-arithmetic.c`, `l03-half-fenv.c`,
  `l03-half-abi-worker.c`, `l03-half-abi-driver.c`, `l03-half-varargs.c`.
- L04: `l04-complex-ops.c`, `l04-helper-model.c`, `l04-codegen.c`,
  `l04-abi-worker.c`, `l04-abi-driver.c`.
- L05: `l05-lane-identity.c`, `l05-convert.c`, `l05-shuffle.c`,
  `l05-broadcast.c`, `l05-operators.c`, `l05-contract.c`, `l05-flags.c`,
  `l05-storage-boundary.c`, `l05-general-regs.c`, `l05-abi-crossed.c`,
  `l05-hva-mixed-lanes.c`, `l05-neon-subset.c`.
- L06: `l06-tiny-vector-abi.h`, `l06-tiny-vector-worker.c`,
  `l06-tiny-vector-driver.c`.
- L07: `l07-integer-complex-division.c`.
- L08: `l08-typeof-vm-operands.c`.
- L09: `l09-ancestor-return-address.c`.
- L11a: `l11a-atomic-layout.c`, `l11a-atomic-objects.c`, `l11a-stdatomic.c`,
  `l11a-atomic-negative.c`, `l11a-codegen.c`.
- L11b: `l11b-rmw-cas.c`, `l11b-rmw-negative.c`, `l11b-codegen.c`.
- L11c: `l11c-aggregate.c`, `l11c-misaligned.c`, `l11c-crossed-worker.c`,
  `l11c-crossed-driver.c`.

No test harness collects these files. They are inputs for the packets'
commands, not regression tests.
