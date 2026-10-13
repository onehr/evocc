# Rule decisions, 2026-10-13

On 2026-10-13 the owner asked to finish every open TODO item. These decisions
close the NEEDS_RULE state of K07, L03 to L10 and L11a to L11c. They rest on
the packets in this directory. The Rust lane implements them; each item stays
open until its own exit holds.

Principle:

- On Linux, follow GCC 14 unless the C standard requires otherwise.
- On Darwin, follow Apple clang where the platform ABI or the C library
  depends on the choice. Elsewhere, use the Linux rule on both targets.
- Keep undefined behavior diagnosed as the repository already does.
- Claim no interoperability that a crossed test has not shown.
- Where a shape is not yet implemented, refuse it with a named diagnostic.
  Never approximate it.

## Decisions

| Item | Decision | Open points settled here |
| --- | --- | --- |
| K07 | Accept the packet's rule. `__builtin_strlen` of an immutable character array with static storage duration at file scope, or of a `T *const` file-scope object that points into one, is an extended integer constant expression on both targets and at every `-O`, with constant offsets, casts and constant conditionals. No call is emitted. Every other operand stays an ordinary call. | Automatic `const` objects and block-scope `static` arrays are excluded, as GCC excludes block-scope statics. The rule stays bounded. |
| L07 | Accept GCC's wide (Smith) algorithm in the component type, with the existing E3002 and E3004 obligations at each step. | The same rule applies on Darwin, because integer complex division depends on no platform ABI or library. `-fcx-limited-range` and `-ffast-math` do not change integer division. |
| L08 | Accept. A `typeof` operand is evaluated exactly when its type is variably modified (C23 6.7.3.6p4). GCC 14's skip is GCC bug c/114831, fixed in GCC 15. | None. |
| L09 | `__builtin_return_address(n)` and `__builtin_frame_address(n)` with `n` from 1 to 65535 yield a null pointer on both targets, as GCC does. The interpreters give the same null. | The packet's Darwin frame walk is not adopted. It depends on no platform ABI rule, and a null never fabricates an address. |
| L06 | Linux: 1-, 2- and 4-byte integer vectors pass and return as integers in general registers, as GCC does. Calls to Clang-compiled results stay a recorded incompatibility. Darwin: follow Apple clang's widened `v0` result. | The Darwin choice follows the platform ABI. |
| L10 | Accept the case-bound disposition: LLVM `pr23135` is source undefined behavior (`not-applicable`), and the six-point evidence threshold of the packet is the B01 rule for later source-UB dispositions. No compiler change. | None. |
| L11b | Accept: the GNU fetch-op, op-fetch, exchange, strong and weak compare-exchange, test-and-set, clear, fences and lock-free queries on 1-, 2-, 4- and 8-byte scalars, as inline code. E2947 for exactly the memory-order pairs that GCC warns about. Unadmitted `__atomic_*` and `__sync_*` spellings are reserved and refused, never implicit calls, `-w` or not. | `__atomic_fetch_add` and its relatives on pointers add bytes, unscaled, on both targets: GCC documents this for these builtins. C11 operators and generic functions on `_Atomic` pointer objects scale, as C11 requires. |
| L11a | Accept: `_Atomic` objects with the GCC layout on Linux and the Apple layout on Darwin. Reads and stores are `seq_cst`. Compound assignment, `++` and `--` are one `seq_cst` read-modify-write with the existing obligations. | Compound assignment on a floating `_Atomic` object is refused with a named diagnostic in this slice. |
| L11c | Accept: naturally aligned 1-, 2-, 4- and 8-byte records through the memory-to-memory atomic forms, with byte transfer and byte comparison, and E3001 on an uninitialized byte. | 16-byte records are refused with a named diagnostic in this slice. |
| L03 | Accept: `_Float16`, `__fp16` and `__bf16` are storage formats. Each operation evaluates in binary32 and rounds once to the half format. This gives the same bits as native half arithmetic. AAPCS64 scalars and HFAs are supported, except `__bf16` HFAs, which are refused (GCC PR 120007 is open). | The same evaluation rule applies on Darwin, so both targets give the same bits as Apple clang's native arithmetic for single operations. A half-to-integer conversion out of range keeps the existing floating-to-integer diagnostic. |
| L04 | Accept: complex values are HFA pairs. `+`, `-` and mixed operands work per part. Multiply and divide call each target's runtime helper in native code (libgcc on Linux, compiler-rt on Darwin). The interpreters and constant folding share one model of the Linux helpers. | Darwin interpreters and constant folding refuse complex multiply and divide with a named diagnostic until the compiler-rt sequence is modeled; native Darwin code calls the platform helper. The binary128 special-value grid must run before long double complex is admitted. |
| L05 | Accept: GNU `float` and `double` vectors with per-lane scalar rules, no implicit contraction, E3063 per lane in conversions, and HVA passing by vector size. `v++` and `&v[i]` follow GCC. Under `-mgeneral-regs-only` floating vectors are refused. No `arm_neon.h` intrinsic is added in this item; the full NEON original becomes item L05b. | Darwin mask lanes follow Apple clang. Records of floating vectors pass by reference today, which is a silent ABI mismatch; until L05 lands, the backend refuses them. |
